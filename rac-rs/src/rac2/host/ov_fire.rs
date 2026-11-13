//! [PC] Fire, Smoke and weapon lights on R&C2's effects (parity rows 30-32; engine viewer/firefx.rs, volsmoke.rs, shotlight.rs).
//! R&C2's effects reach the engine as VU1 program-2 record triangles (ov_layers.rs), not as pool sprites, so the engine's fire /
//! smoke / light paths (keyed on pool sprites, `Gpu::set_particles`) had nothing. Here each camera-facing record quad (two effect
//! triangles with the same 3D point: the sprite's centre, relative to the eye) is matched to the effect-pool slot at that point
//! (rac2/fx_pool.rs: record qw0 xyz = slot +0x10, notes_fx_pool.md section 5) and handed to `set_particles` as the pool sprite it is,
//! drawn as its own two triangles (`Gpu::set_pool_quads`). The sprites the smoke volumes or the fire take over leave the layer list;
//! every other one stays the layer triangle it was (pixel for pixel). Fire, Smoke and the weapon lights all Off (or Lighting
//! Classic): the layers are handed as before and the pool is not read.
use crate::scene::View;
use crate::fx_pool::{self, Slot};
use rac_engine::gsfull::layer_tris::LayerTri;
use rac_engine::prims::{Sprite, FX_TEX};
use rac_engine::viewer::gpu::layer_fx::{PoolQuad, WorldTri};
use rac_engine::viewer::{Camera, Gpu};
use std::collections::HashMap;

/// The farthest (m) a record's centre may lie from its slot's point: the record is the slot's point as the VU1 program read it, so a
/// match is exact up to the camera rounding; two puffs are rarely this close.
const MATCH: f32 = 0.25;

/// Do Fire, Smoke or the weapon lights want the effect sprites this frame (they all need Lighting Enhanced)?
pub(super) fn gate() -> bool {
	crate::options::graphics().lighting != 0
		&& (rac_engine::fire_option::enabled() || rac_engine::smoke_option::level() > 0 || rac_engine::shot_rows::wanted())
}

/// RAC2_FIRELOG is set (read once): per new field, the quads found and matched per pool snapshot (field - 1, field, field - 2).
fn log_on() -> bool {
	static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
	*ON.get_or_init(|| std::env::var_os("RAC2_FIRELOG").is_some_and(|v| v != "0"))
}

/// One camera-facing record quad of the newest field, matched to its pool slot.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Quad {
	/// Its two triangles in the newest field's world list.
	tris: [usize; 2],
	/// The corners in quad order S1, B, S2, A (S1 / S2 shared, B / A the unshared corner of each): (triangle 0 / 1, corner).
	order: [(usize, usize); 4],
	/// The sprite's centre in the world as the field placed it, and the mean of its four placed corners.
	centre: [f32; 3],
	mean: [f32; 3],
	slot: Slot,
}

/// The quads of the newest field and whether the engine holds pool sprites.
#[derive(Default)]
pub(super) struct Fire {
	quads: Vec<Quad>,
	active: bool,
	field: u64,
	/// [b49 U3] The slot grids of the last pool snapshots by field: the snapshot of field F was gridded again as `d` 0, 1 and 2 of three
	/// consecutive fields (a big burst's pool made that 3 hash builds per field).
	grids: Vec<(u64, std::sync::Arc<HashMap<[i32; 3], Vec<Slot>>>)>,
}

/// The unshared corner of `a` against `b` and the shared ones (by the corners' pixel offsets), None = not a quad pair.
fn split(a: &[rac_engine::gsfull::fx_native::WVert; 3], b: &[rac_engine::gsfull::fx_native::WVert; 3]) -> Option<(usize, [usize; 2])> {
	let shared: Vec<usize> = (0..3).filter(|&i| b.iter().any(|w| w.off == a[i].off)).collect();
	(shared.len() == 2).then(|| ((0..3).find(|i| !shared.contains(i)).unwrap_or(0), [shared[0], shared[1]]))
}

/// The quad pairs of a field: consecutive effect triangles with one texture and one point in all six corners (the record's sprite).
/// `src[i]`: the `LayerTri` world triangle `i` came from. Returns (triangle i, i + 1, quad order, centre relative to the eye).
fn pairs(tris: &[LayerTri], src: &[usize], w: &[WorldTri]) -> Vec<([usize; 2], [(usize, usize); 4], [f32; 3])> {
	let mut out = Vec::new();
	let mut i = 0;
	while i + 1 < w.len() {
		let (Some(a), Some(b)) = (tris.get(src[i]).and_then(|t| t.world), tris.get(src[i + 1]).and_then(|t| t.world)) else {
			i += 1;
			continue;
		};
		let p = a[0].p;
		let same = w[i].id == w[i + 1].id && w[i].additive == w[i + 1].additive && a.iter().chain(b.iter()).all(|c| c.p == p);
		match same.then(|| split(&a, &b).zip(split(&b, &a))).flatten() {
			Some(((ua, [s1, s2]), (ub, _))) => {
				out.push(([i, i + 1], [(0, s1), (1, ub), (0, s2), (0, ua)], p));
				i += 2;
			}
			None => i += 1,
		}
	}
	out
}

/// The nearest wanted slot within `MATCH` of `p` (grid of 2 * MATCH cells).
fn nearest(grid: &HashMap<[i32; 3], Vec<Slot>>, p: [f32; 3]) -> Option<Slot> {
	// [b50 V1] A record's centre is its slot's point up to the camera rounding, so first look only within `TIGHT` of `p` (one cell of
	// a few slots instead of 27 cells of a dense burst); when anything is there it is also the nearest within `MATCH` (every slot
	// nearer than it lies in the tight set, and the cells are walked in the same order, so a tie goes to the same slot). Else the
	// whole `MATCH` reach, as before.
	scan(grid, p, TIGHT).or_else(|| scan(grid, p, MATCH))
}

/// The slots within `TIGHT` m of a record's centre are tried before the whole `MATCH` reach.
const TIGHT: f32 = 0.01;

/// The nearest slot within `r` (<= `MATCH`) of `p`: the cells that can hold one, walked in the order of the plain 3 x 3 x 3 walk.
fn scan(grid: &HashMap<[i32; 3], Vec<Slot>>, p: [f32; 3], r: f32) -> Option<Slot> {
	let cell = p.map(|x| (x / (2.0 * MATCH)).floor() as i32);
	// The cells the ball of radius r (plus a rounding margin) touches, never beyond the plain walk's +-1.
	let span: [[i32; 2]; 3] = std::array::from_fn(|k| {
		let (lo, hi) = (((p[k] - r - 1e-3) / (2.0 * MATCH)).floor() as i32, ((p[k] + r + 1e-3) / (2.0 * MATCH)).floor() as i32);
		[lo.max(cell[k] - 1), hi.min(cell[k] + 1)]
	});
	let mut best: Option<(f32, Slot)> = None;
	for cx in span[0][0]..=span[0][1] {
		for cy in span[1][0]..=span[1][1] {
			for cz in span[2][0]..=span[2][1] {
				for s in grid.get(&[cx, cy, cz]).into_iter().flatten() {
					let d = (0..3).map(|k| (s.pos[k] - p[k]).powi(2)).sum::<f32>();
					if d <= r * r && best.is_none_or(|b| d < b.0) {
						best = Some((d, *s));
					}
				}
			}
		}
	}
	best.map(|b| b.1)
}

fn grid(slots: &[Slot]) -> HashMap<[i32; 3], Vec<Slot>> {
	let mut g: HashMap<[i32; 3], Vec<Slot>> = HashMap::new();
	for s in slots.iter().filter(|s| crate::fx_rows::wanted(s.ty)) {
		g.entry(s.pos.map(|x| (x / (2.0 * MATCH)).floor() as i32)).or_default().push(*s);
	}
	g
}

fn mean(c: [[f32; 3]; 4]) -> [f32; 3] {
	std::array::from_fn(|k| c.iter().map(|p| p[k]).sum::<f32>() / 4.0)
}

/// The corners of quad `q` in `w` (None: the list is not the one the quad was found in).
fn corners(q: &Quad, w: &[WorldTri]) -> Option<[[f32; 3]; 4]> {
	let t = [w.get(q.tris[0])?, w.get(q.tris[1])?];
	Some(q.order.map(|(k, c)| t[k].pos[c]))
}

/// R&C1's slot -> sprite rule (rac1/particles.rs `retail_pool_sprite`) at `pos`, drawn with layer texture key `key`.
fn sprite(s: &Slot, pos: [f32; 3], key: u32, additive: bool) -> Sprite {
	let c = s.rgba.to_le_bytes().map(|b| b as f32 / 128.0);
	Sprite {
		pos,
		size: s.size * std::f32::consts::SQRT_2 / 210_000.0,
		rot: s.rot as f32 * std::f32::consts::TAU / 256.0,
		colour: [c[0], c[1], c[2], c[3].clamp(0.0, 1.0)],
		tex: FX_TEX + key,
		additive,
		pool: Some(s.b9),
		flat: false,
	}
}

impl Fire {
	/// A new field's layers (ov_layers.rs `Layers::update`): `w` the world triangles of `tris` (`src[i]` = the source of `w[i]`),
	/// placed with `v`; `field` the GS frame's field.
	pub(super) fn note(&mut self, field: u64, tris: &[LayerTri], src: &[usize], w: &[WorldTri], v: &View) {
		self.quads.clear();
		self.field = field;
		if !gate() {
			return;
		}
		let found = pairs(tris, src, w);
		// The pool of the camera's field (the picture lags its camera by one field: ov_layers.rs GS_CAMERA_LAG), else the field
		// either side; the snapshot with the most matches wins.
		let mut best: (usize, Vec<Quad>, i64) = (0, Vec::new(), 0);
		let mut counts = Vec::new();
		for d in [1i64, 0, 2] {
			let snap = (field as i64 - d).max(0) as u64;
			let g = match self.grids.iter().find(|e| e.0 == snap) {
				Some(e) => e.1.clone(),
				None => {
					let Some(pool) = fx_pool::of_field(snap) else { continue };
					let g = std::sync::Arc::new(grid(&pool));
					self.grids.push((snap, g.clone()));
					if self.grids.len() > 6 {
						self.grids.remove(0);
					}
					g
				}
			};
			let qs: Vec<Quad> = found
				.iter()
				.filter_map(|&(t, order, p)| {
					let centre = std::array::from_fn(|k| v.eye[k] + p[k]);
					// The nearest slot of a row's type; a plasma-coloured one stays a layer quad (fx_rows::wanted_slot).
					let slot = nearest(&g, centre).filter(|s| crate::fx_rows::wanted_slot(s.ty, s.tex, s.rgba))?;
					let q = Quad { tris: t, order, centre, mean: [0.0; 3], slot };
					Some(Quad { mean: mean(corners(&q, w)?), ..q })
				})
				.collect();
			counts.push((d, qs.len()));
			let all = qs.len() == found.len();
			if qs.len() > best.0 {
				best = (qs.len(), qs, d);
			}
			if all {
				break; // [b50 V1] every quad matched: no later snapshot can match more (the first strictly best one wins)
			}
		}
		if log_on() {
			eprintln!("ov_fire: field {field}: {} quads, matched per snapshot (field - d, count) {counts:?}, used d {}", found.len(), best.2);
		}
		self.quads = best.1;
	}

	/// No layers any more (a field without a camera, a whole-picture field).
	pub(super) fn clear(&mut self) {
		self.quads.clear();
		self.grids.clear();
	}

	/// One redraw while the gate is on: `shown` = the layers for this redraw (index-parallel to the newest field's), `cam` the
	/// engine camera. Hands the engine the sprites, then the layers without the triangles of the sprites it took over.
	pub(super) fn step(&mut self, g: &mut Gpu, shown: Vec<WorldTri>, cam: &Camera) {
		self.active = true;
		let (mut sprites, mut quads, mut slots, mut types, mut texs, mut owner) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
		for q in &self.quads {
			let (Some(c), Some(t)) = (corners(q, &shown), shown.get(q.tris[0])) else { continue };
			let Some(key) = g.layer_key(t.id, t.depth) else { continue };
			let m = mean(c);
			let pos = std::array::from_fn(|k| q.centre[k] + m[k] - q.mean[k]);
			let u = [&shown[q.tris[0]], &shown[q.tris[1]]];
			quads.push(PoolQuad { pos: c, uv: q.order.map(|(k, i)| u[k].uv[i]), col: q.order.map(|(k, i)| u[k].col[i]) });
			sprites.push(sprite(&q.slot, pos, key, t.additive));
			slots.push(q.slot.index);
			types.push(crate::fx_rows::sprite_type(q.slot.ty, q.slot.rgba)); // [b47 X1] orange type 11 = the Gravity Bomb's blast
			texs.push(q.slot.tex);
			owner.push(q.tris);
		}
		g.set_pool_quads(quads);
		// [b45 P7] The hero and the movers for the water rings and wakes (fx_movers.rs). [b47 X1] The explosion fire is gated to the
		// player's explosive shots of the shown field (fx_shots.rs): a Blast sprite born where one vanished becomes the fire burst, every
		// other one is drawn as retail draws it ([C] fg1 0531: the volume took a big type-97 explosion's hot core away; 0200: the
		// hoverbike's type-74 exhaust puffs became fireballs). The rockets' lights are points of the same list.
		let own = crate::fx_movers::current();
		let shots = crate::fx_shots::of_field(own.frames as u64);
		let mut fx = crate::fx_movers::pool_fx(&own, &slots, &types, &texs);
		fx.blast_gate = Some(&shots.zones);
		fx.shots = &shots.points;
		g.set_particles(&sprites, &[], &[], &[], cam, Some(fx));
		let mut drop = vec![false; shown.len()];
		for (k, _) in g.pool_taken().iter().enumerate().filter(|e| *e.1) {
			owner[k].iter().for_each(|&i| drop[i] = true);
		}
		let kept: Vec<WorldTri> = shown.into_iter().zip(drop).filter(|e| !e.1).map(|e| e.0).collect();
		g.set_layer_tris(&kept);
	}

	/// The gate is off: once after it was on, the engine drops its sprites (`set_particles` without any) and gets `shown` back whole.
	pub(super) fn off(&mut self, g: &mut Gpu, shown: Option<Vec<WorldTri>>, cam: &Camera) {
		if std::mem::take(&mut self.active) {
			g.set_particles(&[], &[], &[], &[], cam, None);
			if let Some(w) = shown {
				g.set_layer_tris(&w);
			}
		}
	}

	pub(super) fn active(&self) -> bool {
		self.active
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rac_engine::gsfull::fx_native::WVert;
	use rac_engine::gsfull::layer_tris::LayerVert;

	fn tri(p: [f32; 3], offs: [[f32; 2]; 3]) -> LayerTri {
		let v = LayerVert { px: 0.0, py: 0.0, z: 1, rgba: [128; 4], uv: [0.0; 2], f: 255 };
		let tex = std::sync::Arc::new(rac_formats::level::Texture { w: 1, h: 1, rgba: vec![[255; 4]] });
		LayerTri { v: [v; 3], tex, id: 7, additive: true, decal: false, world: Some(offs.map(|off| WVert { p, off })), depth: true, untextured: false, replace: false, zwrite: false, alpha: Default::default() }
	}

	fn wt(t: &LayerTri, pos: [[f32; 3]; 3]) -> WorldTri {
		WorldTri { pos, uv: [[0.0; 2]; 3], col: [[1.0; 4]; 3], id: t.id, tex: t.tex.clone(), additive: t.additive, depth: true, effect: true, replace: false, zwrite: false, alpha: Default::default() }
	}

	/// [b50 V1] The plain 3 x 3 x 3 walk `nearest` replaced.
	fn nearest_plain(grid: &HashMap<[i32; 3], Vec<Slot>>, p: [f32; 3]) -> Option<Slot> {
		let cell = p.map(|x| (x / (2.0 * MATCH)).floor() as i32);
		let mut best: Option<(f32, Slot)> = None;
		for dx in -1..=1 {
			for dy in -1..=1 {
				for dz in -1..=1 {
					for s in grid.get(&[cell[0] + dx, cell[1] + dy, cell[2] + dz]).into_iter().flatten() {
						let d = (0..3).map(|k| (s.pos[k] - p[k]).powi(2)).sum::<f32>();
						if d <= MATCH * MATCH && best.is_none_or(|b| d < b.0) {
							best = Some((d, *s));
						}
					}
				}
			}
		}
		best.map(|b| b.1)
	}

	#[test]
	fn the_tight_first_slot_search_equals_the_plain_walk() {
		let mut seed = 777u32;
		let mut rnd = move || {
			seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
			(seed >> 8) as f32 / (1u32 << 24) as f32
		};
		let base = Slot { index: 0, ty: 44, tex: 3, alpha_op: 0x48, rot: 0, b9: 0x40, rgba: 0x8080_8080, size: 210_000.0, pos: [0.0; 3] };
		// A dense burst (2000 slots in 3 m, some at identical points) next to a sparse area, positions at +-500 m like a real level.
		let mut slots: Vec<Slot> = (0..2000).map(|i| Slot { index: i, pos: [500.0 + rnd() * 3.0, -230.0 + rnd() * 3.0, 220.0 + rnd() * 3.0], ..base }).collect();
		slots.extend((0..20).map(|i| Slot { index: 3000 + i, pos: [rnd() * 900.0 - 450.0, rnd() * 900.0 - 450.0, rnd() * 90.0], ..base }));
		slots.push(Slot { index: 4000, ..slots[5] });
		let g = grid(&slots);
		for n in 0..1500 {
			let s = slots[(n * 13) % slots.len()];
			// Exactly a slot's point, a point a few mm off it (the camera rounding), a point 0.1 m off, and a point anywhere.
			let off = match n % 4 {
				0 => 0.0,
				1 => 0.004,
				2 => 0.1,
				_ => 1.5,
			};
			let p = [s.pos[0] + off * (rnd() - 0.5), s.pos[1] + off * (rnd() - 0.5), s.pos[2] + off * (rnd() - 0.5)];
			assert_eq!(nearest(&g, p), nearest_plain(&g, p), "query {n}");
		}
	}

	#[test]
	fn a_record_quad_is_found_ordered_and_matched_to_its_slot() {
		let p = [1.0, 2.0, 3.0];
		// Triangle A = (-1,-1) (1,-1) (1,1), B = (-1,-1) (1,1) (-1,1): shared (-1,-1) and (1,1).
		let a = tri(p, [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0]]);
		let b = tri(p, [[-1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]]);
		let lone = tri([9.0, 9.0, 9.0], [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
		let w = vec![wt(&lone, [[0.0; 3]; 3]), wt(&a, [[0.0; 3], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]]), wt(&b, [[0.0; 3], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]])];
		let tris = vec![lone, a, b];
		let found = pairs(&tris, &[0, 1, 2], &w);
		assert_eq!(found.len(), 1);
		let (t, order, centre) = found[0];
		assert_eq!((t, centre), ([1, 2], p));
		// S1 = A's corner 0, B's unshared = its corner 2, S2 = A's corner 2, A's unshared = its corner 1.
		assert_eq!(order, [(0, 0), (1, 2), (0, 2), (0, 1)]);
		let s = Slot { index: 5, ty: 44, tex: 3, alpha_op: 0x48, rot: 0, b9: 0x40, rgba: 0x8080_8080, size: 210_000.0, pos: [1.1, 2.0, 3.0] };
		let far = Slot { pos: [1.0, 2.0, 3.4], ..s };
		let skip = Slot { ty: 19, pos: p, ..s };
		let g = grid(&[s, far, skip]);
		assert_eq!(nearest(&g, p), Some(s), "the nearest wanted slot within 0.25 m; type 19 is no row");
		assert_eq!(nearest(&g, [5.0, 5.0, 5.0]), None);
		let q = Quad { tris: t, order, centre: p, mean: [0.0; 3], slot: s };
		assert_eq!(corners(&q, &w), Some([[0.0; 3], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0], [1.0, 0.0, 0.0]]));
		let sp = sprite(&s, p, 0x3000_0002, true);
		assert!((sp.size - std::f32::consts::SQRT_2).abs() < 1e-6 && sp.colour == [1.0; 4] && sp.tex == FX_TEX + 0x3000_0002 && sp.pool == Some(0x40));
	}
}
