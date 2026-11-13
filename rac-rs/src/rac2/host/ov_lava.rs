//! [PC] b47 X2 (sweep D M8): the heat haze over R&C2's lava, as R&C1 has it over its lava oceans (engine heathaze.rs; Lighting
//! Enhanced with Graphics -> Fire Medium / High, the same rows; Classic or Fire Low / Off = no haze, the picture unchanged).
//! R&C2's lava is no ocean draw: level 14's lava is a GS world layer sheet, ALPHA 0x88 replace with ZMSK off [C: lane lav,
//! RAC_GSPIX q60903; `LayerTri::replace` + `zwrite`]. Its triangles' world points (`ov_layers::unproject_exact`, the sheet's own
//! depth) are kept per level in 2 m cells; a cell counts once it was seen in `SEEN` fields (a stray point of one field never makes
//! haze), and the cells of one height make one box (as R&C1's box over a whole ocean draw), so the haze does not follow the part of
//! the sheet the camera happens to see. The boxes go to the engine each field (viewer/haze_lava.rs). Joba (level 11, fg1 0512)
//! draws its lava without such a sheet [C: run b47/x2/runs/joba_kind, RAC2_OVLKIND: ALPHA 0x44 / 0x48 / 0x49 / 0x68 only]: no box,
//! as R&C1's lava drawn as terrain has none. `LAVA_LEVELS` = the levels whose replace sheets are proven lava.

use super::ov_layers::unproject_exact;
use crate::scene::View;
use rac_engine::gsfull::layer_tris::LayerTri;
use std::collections::HashMap;

/// Levels whose replace + z-write layer sheets are lava [C: level 14, q60903].
const LAVA_LEVELS: &[i32] = &[14];
/// Cell size (world units, x / y), the reach from the eye a point is kept within, one box's height band, the fields a cell must be
/// seen in, and the most boxes (the engine's `MAX_LAVA`).
const CELL: f32 = 2.0;
const REACH: f32 = 80.0;
const BAND: f32 = 1.0;
const SEEN: u32 = 2;
const MOST: usize = 4;

#[derive(Clone, Copy)]
struct Cell {
	lo: f32,
	hi: f32,
	fields: u32,
	last: u64,
}

/// The lava seen on this level, and the boxes made of it.
#[derive(Default)]
pub(super) struct Lava {
	level: i32,
	cells: HashMap<(i32, i32), Cell>,
	boxes: Vec<[f32; 6]>,
	dirty: bool,
	/// The engine holds a non-empty list.
	handed: bool,
}

impl Lava {
	/// One GS frame (host/frame.rs, after `Layers::update`): `level` the game's, `live` = this frame's layers and their camera when
	/// the engine draws them (an overlay field with the world shown), `frozen` = a held menu field (the boxes stay, as the layers do).
	pub(super) fn field(&mut self, level: i32, field: u64, live: Option<(&[LayerTri], &View)>, frozen: bool) {
		if level != self.level {
			*self = Lava { level, handed: self.handed, ..Lava::default() };
		}
		if frozen {
			return;
		}
		match live.filter(|_| LAVA_LEVELS.contains(&level)) {
			Some((tris, v)) => {
				let sheet = tris.iter().filter(|t| t.world.is_none() && t.replace && t.zwrite && t.v.iter().all(|p| p.z > 0 && p.z < 0x00ff_0000));
				self.note(field, sheet.flat_map(|t| t.v.iter().map(|p| unproject_exact(v, p))), v.eye);
				let b = self.boxes();
				self.hand(b);
			}
			None => self.hand(Vec::new()),
		}
	}

	fn hand(&mut self, b: Vec<[f32; 6]>) {
		if b.is_empty() && !self.handed {
			return;
		}
		self.handed = !b.is_empty();
		rac_engine::viewer::haze_lava::set(b);
	}

	/// The sheet points of one field (world, z up), those within `REACH` of the eye.
	fn note(&mut self, field: u64, pts: impl Iterator<Item = [f32; 3]>, eye: [f32; 3]) {
		for p in pts.filter(|p| p.iter().all(|x| x.is_finite())) {
			let d: f32 = (0..3).map(|k| (p[k] - eye[k]) * (p[k] - eye[k])).sum();
			if d > REACH * REACH {
				continue;
			}
			let key = ((p[0] / CELL).floor() as i32, (p[1] / CELL).floor() as i32);
			let c = self.cells.entry(key).or_insert(Cell { lo: p[2], hi: p[2], fields: 0, last: u64::MAX });
			if c.last != field {
				c.last = field;
				c.fields += 1;
				self.dirty |= c.fields == SEEN;
			}
			if c.fields >= SEEN && (p[2] < c.lo || p[2] > c.hi) {
				self.dirty = true;
			}
			(c.lo, c.hi) = (c.lo.min(p[2]), c.hi.max(p[2]));
		}
	}

	/// The boxes (min x, y, z, max x, y, z): the cells seen in `SEEN` fields, one box per connected group per `BAND` of surface height, the `MOST` biggest.
	fn boxes(&mut self) -> Vec<[f32; 6]> {
		if !self.dirty {
			return self.boxes.clone();
		}
		self.dirty = false;
		// [b48 Y1] one box per CONNECTED group of cells of a height band (cells up to 2 apart on the 2 m grid, vertices are sparse): two pools of one height
		// no longer make one box over the floor between them.
		let mut bands: HashMap<i32, HashMap<(i32, i32), Cell>> = HashMap::new();
		for (&k, c) in self.cells.iter().filter(|(_, c)| c.fields >= SEEN) {
			bands.entry((c.hi / BAND).floor() as i32).or_default().insert(k, *c);
		}
		let mut v: Vec<([f32; 6], usize)> = Vec::new();
		for mut left in bands.into_values() {
			while let Some(&start) = left.keys().next() {
				let mut b = [f32::MAX, f32::MAX, f32::MAX, f32::MIN, f32::MIN, f32::MIN];
				let mut n = 0;
				let mut todo = vec![start];
				while let Some((ix, iy)) = todo.pop() {
					let Some(c) = left.remove(&(ix, iy)) else { continue };
					let (x0, y0) = (ix as f32 * CELL, iy as f32 * CELL);
					(b[0], b[1], b[2]) = (b[0].min(x0), b[1].min(y0), b[2].min(c.lo));
					(b[3], b[4], b[5]) = (b[3].max(x0 + CELL), b[4].max(y0 + CELL), b[5].max(c.hi));
					n += 1;
					for (dx, dy) in (-2..=2).flat_map(|dx| (-2..=2).map(move |dy| (dx, dy))) {
						if left.contains_key(&(ix + dx, iy + dy)) {
							todo.push((ix + dx, iy + dy));
						}
					}
				}
				v.push((b, n));
			}
		}
		v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0[5].total_cmp(&b.0[5])));
		self.boxes = v.into_iter().take(MOST).map(|b| b.0).collect();
		self.boxes.clone()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn sheet(l: &mut Lava, field: u64, z: f32) {
		let pts = (0..10).flat_map(|i| (0..10).map(move |j| [i as f32 * 3.0, j as f32 * 3.0, z]));
		l.note(field, pts, [0.0, 0.0, z + 5.0]);
	}

	#[test]
	fn a_sheet_seen_in_two_fields_makes_one_box_and_a_stray_point_none() {
		let mut l = Lava::default();
		sheet(&mut l, 1, 10.0);
		assert!(l.boxes().is_empty(), "one field is not enough");
		sheet(&mut l, 2, 10.0);
		l.note(2, [[50.0, 50.0, 30.0]].into_iter(), [0.0; 3]);
		let b = l.boxes();
		assert_eq!(b.len(), 1);
		assert_eq!(b[0], [0.0, 0.0, 10.0, 28.0, 28.0, 10.0]);
	}

	#[test]
	fn two_pools_of_one_height_make_two_boxes() {
		let mut l = Lava::default();
		for f in 1..3 {
			let pts = (0..4).flat_map(|i| (0..4).map(move |j| [i as f32 * 3.0, j as f32 * 3.0, 10.0]));
			l.note(f, pts, [0.0, 0.0, 15.0]);
			let pts = (0..4).flat_map(|i| (0..4).map(move |j| [40.0 + i as f32 * 3.0, j as f32 * 3.0, 10.0]));
			l.note(f, pts, [0.0, 0.0, 15.0]);
		}
		assert_eq!(l.boxes().len(), 2);
	}

	#[test]
	fn the_box_does_not_shrink_when_the_camera_sees_less() {
		let mut l = Lava::default();
		sheet(&mut l, 1, 10.0);
		sheet(&mut l, 2, 10.0);
		let whole = l.boxes();
		l.note(3, [[1.0, 1.0, 10.0]].into_iter(), [0.0; 3]);
		assert_eq!(l.boxes(), whole);
	}

	#[test]
	fn points_beyond_the_reach_are_not_kept() {
		let mut l = Lava::default();
		for f in 0..3 {
			l.note(f, [[REACH + 10.0, 0.0, 0.0]].into_iter(), [0.0; 3]);
		}
		assert!(l.boxes().is_empty());
	}

	#[test]
	fn only_level_14_has_lava_and_a_level_change_forgets_it() {
		assert_eq!(LAVA_LEVELS, &[14]);
		let mut l = Lava { level: 14, ..Lava::default() };
		sheet(&mut l, 1, 10.0);
		sheet(&mut l, 2, 10.0);
		assert_eq!(l.boxes().len(), 1);
		l.field(3, 3, None, false);
		assert!(l.cells.is_empty() && l.boxes().is_empty());
	}
}
