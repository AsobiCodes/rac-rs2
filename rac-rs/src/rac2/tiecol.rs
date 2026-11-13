//! R&C2's tie colours (engine split stage E5): the game expands each tie's colour stream itself and sends it with each draw
//! (ee_oracle::rc2::tiecap, by tie record); the pack writes those colours into the engine tie's slot colours
//! (`Gpu::set_tie_colours`). An engine tie's `cols` holds one entry per stream entry of each LOD that decoded, LOD after LOD
//! (rac-formats rc2/ties.rs); a stream's length tells which LOD the game drew. Until the game has sent a LOD, its entries take the
//! colours of the nearest vertex of a LOD it did send (`ties::tie_colour_maps`). From lane rc2's native/gpu.rs `recolour_ties`.

use rac_engine::viewer::Gpu;
use rac_formats::ties::LitTie;
use std::collections::HashMap;
use std::sync::Arc;

/// Per decoded LOD of a tie class: every LOD-0 vertex's entry in that LOD's stream, and the stream's length.
pub type TieMaps = Arc<Vec<(Vec<u32>, u32)>>;

struct Tie {
	/// Index in the engine's tie list.
	at: usize,
	/// Per decoded LOD: its first entry in `cols`.
	base: Vec<usize>,
	maps: TieMaps,
	cols: Vec<[f32; 4]>,
	/// The decoded LODs whose colours the game sent.
	have: Vec<bool>,
}

/// The game's tie colours of one level, by tie record.
#[derive(Default)]
pub struct TieCols {
	by_record: Vec<Option<Tie>>,
}

impl TieCols {
	/// The engine's tie list (the placed ties of `placed`, records of unknown classes left out) and the colour state of each
	/// record. `classes`: each record's tie class; `maps`: `tie_colour_maps` by class. A tie whose stream lengths do not add up
	/// to its `cols` keeps the neutral colours.
	pub fn new(placed: Vec<Option<LitTie>>, classes: &[u32], maps: &HashMap<u32, TieMaps>) -> (Vec<LitTie>, TieCols) {
		let (mut ties, mut by_record, mut bad) = (Vec::new(), Vec::new(), 0);
		for (i, p) in placed.into_iter().enumerate() {
			let Some(t) = p else {
				by_record.push(None);
				continue;
			};
			let m = classes.get(i).and_then(|c| maps.get(c)).cloned().unwrap_or_default();
			let base: Vec<usize> = m.iter().scan(0, |at, x| Some(std::mem::replace(at, *at + x.1 as usize))).collect();
			let ok = !m.is_empty() && m.iter().map(|x| x.1 as usize).sum::<usize>() == t.cols.len();
			bad += !ok as usize;
			by_record.push(ok.then(|| Tie { at: ties.len(), have: vec![false; m.len()], base, maps: m, cols: t.cols.clone() }));
			ties.push(t);
		}
		if bad > 0 {
			eprintln!("rac2: {bad} of {} ties have no colour layout (neutral colours)", ties.len());
		}
		(ties, TieCols { by_record })
	}

	/// Starts the game-side capture for this level (when the engine has its ties).
	pub fn start(&self) {
		ee_oracle::rc2::tiecap::capture(Some(ee_oracle::rc2::tiecap::TieCap { count: self.by_record.len() as u32, at: None, wait: 0, got: HashMap::new() }));
	}

	/// Takes the colours the game sent since the last call and writes the ties that changed into the engine. RGBA8 words, 0x80 = 1.0
	/// like the engine's tie colours.
	pub fn apply(&mut self, gpu: &mut Gpu) {
		for (i, w) in ee_oracle::rc2::tiecap::take() {
			let Some(Some(t)) = self.by_record.get_mut(i as usize) else { continue };
			let Some(j) = t.maps.iter().position(|x| x.1 as usize == w.len()) else { continue };
			let rgba = |c: u32| c.to_le_bytes().map(|b| b as f32);
			let fresh: Vec<[f32; 4]> = w.iter().map(|&c| rgba(c)).collect();
			let at = t.base[j];
			if t.have[j] && t.cols[at..at + fresh.len()] == fresh[..] {
				continue;
			}
			t.cols[at..at + fresh.len()].copy_from_slice(&fresh);
			t.have[j] = true;
			t.spread();
			gpu.set_tie_colours(t.at, &t.cols);
		}
	}
}

impl Tie {
	/// The LODs the game has not sent take their colours from a LOD it did, through the LOD-0 vertices (LOD 0 first).
	fn spread(&mut self) {
		let Some(src) = (0..self.have.len()).find(|&j| self.have[j]) else { return };
		// Each LOD-0 vertex's colour, from the LOD-0 stream or the first LOD the game sent.
		let v0: Vec<[f32; 4]> = self.maps[src].0.iter().map(|&e| self.cols[self.base[src] + e as usize]).collect();
		for j in 0..self.have.len() {
			if self.have[j] {
				continue;
			}
			for (v, &e) in self.maps[j].0.iter().enumerate() {
				if let (Some(c), Some(x)) = (v0.get(v), self.cols.get_mut(self.base[j] + e as usize)) {
					*x = *c;
				}
			}
		}
	}
}
