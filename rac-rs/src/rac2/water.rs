//! R&C2's water surfaces (VU1 program 10) in the engine's world: gsfull's overlay mode skips the program and keeps its input
//! (`gsfull::water::WATER`, the batches of the last fields: GS state and vertices in world space); each batch becomes the
//! engine's fogged water quads (`Gpu::water_quads`, the pass R&C1's program-10 water uses) with its GS texture in `Gpu::fx_tex`.
//! From lane rc2's native/draw.rs `water_draws`.
//!
//! [PC] Frame smoothing: the game builds the water around the camera of the field before the batches' own (`CAMERA_LAG`): the swamp
//! grid's corner is the eye minus (71, 71) m and its other layer turns with the view [C: Oozla, runs/b23/wl.err, RAC2_WATERLOG,
//! grid corner x = eye x - 71 exactly at fields 2205..2211 while the camera turns]. Drawn under the smoothed camera, the newest
//! field's water stood still for a field and then jumped (the swamp "wobbled" against the shore while turning: round 3 batch 23).
//! So the water is blended per vertex from two consecutive fields to the smoothed camera's time (`blend_at`).

use rac_engine::gsfull::water::{WaterBatch, WVert};
use rac_engine::prims::WorldQuad;
use rac_engine::viewer::Gpu;
use rac_formats::level::Texture;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

/// Fields between the batches' field and the camera they were built around (as ov_layers.rs `GS_CAMERA_LAG` for the GS picture).
const CAMERA_LAG: u64 = 1;

/// fx_tex keys of the water textures: WATER_TEX + n (R&C1's fx keys stay below 0x4000; the flag bits start at 1 << 20).
const WATER_TEX: usize = 0x1_0000;

/// [PC] Live fields of water batches kept for a frozen screen (`Water::field` `held`): more than the scenes host/freeze.rs keeps (12).
const HIST: usize = 16;

/// The water textures uploaded for this level: TEX0 -> fx_tex key.
#[derive(Default)]
pub(crate) struct Water {
	/// TEX0 -> (fx_tex key, the texreq version uploaded under it). [b52 H1] The version: a first read can be of another draw's texels
	/// (a HUD-overlay draw of the same TEX0 word before the water's own draw; Dobbo, state 0357, seen whenever the GS stage was a little
	/// late), texreq then reports `changed`: the new texels are uploaded under the same key. Before, the first upload stayed for the level.
	keys: HashMap<u64, (usize, u32)>,
	/// [PC] The batches of the last live fields (`remember`). Under a frozen-screen menu (the pause, host/freeze.rs) the game sends no
	/// water (the world is its grabbed picture: from the first frozen field on every field publishes no batch [C: fg1 0357, Esc at
	/// field 300, RAC2_WATERLOG: fields 303.. hold 0 batches, 299..302 hold 12]), while the native world is the held scene of a field
	/// before the freeze: its water comes from here, so the sea stays under the pause page as it was (it was gone: user 2026-11-12).
	hist: VecDeque<(u64, Arc<Vec<WaterBatch>>)>,
}

impl Water {
	/// This field's water batches as the engine's water quads; a batch whose texture the GS has not read yet is asked for
	/// (gsfull::texreq) and left out until it arrives.
	/// `blended`: the smoothed camera's place (frame smoothing, smooth.rs `Smooth::blended`); None = the newest batches as they are.
	/// `scene`: the field of the scene drawn; unblended, the water built around its camera is drawn (`CAMERA_LAG` fields later; the GS
	/// pipe's first stage may already hold newer ones, host/gs_sync.rs), else the newest.
	/// `held`: the scene is the one host/freeze.rs holds under a frozen screen: the scene's own water from the live fields kept (`hist`).
	pub(crate) fn field(&mut self, g: &mut Gpu, blended: Option<(u64, f32)>, scene: u64, held: bool) {
		let fields = rac_engine::gsfull::water::fields();
		if held {
			let batches = held_water(&self.hist, scene).map(|b| b.as_ref().clone()).unwrap_or_default();
			self.quads(g, &batches);
			return;
		}
		remember(&mut self.hist, &fields);
		let mixed = blended.and_then(|(f, t)| blend_at(&fields, f, t));
		if std::env::var_os("RAC2_WATERLOG").is_some() {
			// The place between the pair (u: 0..1 between them, > 1 extrapolated, 2 = clamped) and the newest field's grid corner (the
			// least x, y of its biggest batch: the eye of the camera it was built around minus (71, 71) on Oozla).
			let corner = fields.last().and_then(|l| l.1.iter().max_by_key(|b| b.verts.len())).map(|b| b.verts.iter().fold([f32::MAX; 2], |c, v| [c[0].min(v.xyz[0]), c[1].min(v.xyz[1])]));
			eprintln!("water blend: smoothed {blended:?} fields {:?} blended {:?} corner {corner:?}", fields.iter().map(|f| (f.0, f.1.len())).collect::<Vec<_>>(), mixed.as_ref().map(|m| m.1));
			if std::env::var_os("RAC2_WATERLOG").is_some_and(|v| v == "batches") {
				for b in fields.last().map(|l| l.1.as_ref().as_slice()).unwrap_or(&[]) {
					let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
					for v in &b.verts {
						for k in 0..3 {
							(lo[k], hi[k]) = (lo[k].min(v.xyz[k]), hi[k].max(v.xyz[k]));
						}
					}
					let tris: Vec<f32> = triangles(b).map(|t| tri_area(t.map(|v| v.xyz))).collect();
					eprintln!("water batch: prim {:#x} alpha {:#x} verts {} tris {} zero-area {} lo {lo:?} hi {hi:?}", b.prim, b.alpha, b.verts.len(), tris.len(), tris.iter().filter(|a| **a < 1e-4).count());
				}
			}
		}
		let own = fields.iter().rev().find(|f| f.0 <= scene + CAMERA_LAG).or(fields.last());
		let batches = mixed.map(|m| m.0).or_else(|| own.map(|l| l.1.as_ref().clone())).unwrap_or_default();
		self.quads(g, &batches);
	}

	/// `batches` as the engine's water quads.
	fn quads(&mut self, g: &mut Gpu, batches: &[WaterBatch]) {
		let mut quads = Vec::new();
		for b in batches {
			let Some(key) = self.key(g, b.tex0) else { continue };
			quads.extend(triangles(b).map(|t| quad(t, key, b)));
		}
		g.water_quads = quads;
	}

	/// A new level: its own textures (the old level's keys leave fx_tex) and no water kept.
	pub(crate) fn clear(&mut self, g: &mut Gpu) {
		self.keys.values().for_each(|k| drop(g.fx_tex.remove(&k.0)));
		self.keys.clear();
		self.hist.clear();
		g.water_quads.clear();
	}

	fn key(&mut self, g: &mut Gpu, tex0: u64) -> Option<usize> {
		let held = self.keys.get(&tex0).copied();
		let Some((t, ver)) = rac_engine::gsfull::texreq::get_versioned(tex0) else {
			if held.is_none() {
				rac_engine::gsfull::texreq::want(tex0);
			}
			return held.map(|h| h.0); // renewed and not read again yet: the texture drawn so far stays
		};
		if !needs_upload(held.map(|h| h.1), ver) {
			return held.map(|h| h.0);
		}
		let k = held.map_or(WATER_TEX + self.keys.len(), |h| h.0);
		let bind = g.upload_texture(Some(&Texture { w: t.w, h: t.h, rgba: t.rgba.clone() }));
		g.fx_tex.insert(k, bind);
		self.keys.insert(tex0, (k, ver));
		Some(k)
	}
}

/// [b52 H1] Is the texreq texture of version `now` to be uploaded, given the version uploaded so far (None: never)?
fn needs_upload(uploaded: Option<u32>, now: u32) -> bool {
	uploaded != Some(now)
}

/// The water at camera time `field + t` (`field`: a camera's field number): the two newest consecutive fields' batches before it,
/// per vertex (position, ST, colour) between them, or at most one field past the newer one (the newest water is a field behind the
/// newest camera: `CAMERA_LAG`). None: no such pair, or the two fields' batches differ in count, PRIM, texture or vertex count.
/// Also the place `u` used (0..1 between the pair, more = past the newer one; host/gs_sync.rs keeps it in 0..1).
fn blend_at(fields: &[(u64, Arc<Vec<WaterBatch>>)], field: u64, t: f32) -> Option<(Vec<WaterBatch>, f32)> {
	let pair = fields.windows(2).rev().find(|w| w[0].0 + 1 == w[1].0 && w[0].0 <= field + CAMERA_LAG)?;
	let ((fa, a), (_, b)) = (&pair[0], &pair[1]);
	let u = ((field + CAMERA_LAG - fa) as f32 + t).clamp(0.0, 2.0);
	let same = |x: &WaterBatch, y: &WaterBatch| x.prim == y.prim && x.tex0 == y.tex0 && x.verts.len() == y.verts.len();
	if a.len() != b.len() || !a.iter().zip(b.iter()).all(|(x, y)| same(x, y)) {
		return None;
	}
	let mix = |p: f32, q: f32| p + (q - p) * u;
	Some((
		a.iter()
			.zip(b.iter())
			.map(|(x, y)| {
				let verts = x.verts.iter().zip(&y.verts).map(|(p, q)| WVert { st: std::array::from_fn(|k| mix(p.st[k], q.st[k])), rgba: std::array::from_fn(|k| mix(p.rgba[k] as f32, q.rgba[k] as f32).round().clamp(0.0, 255.0) as u8), xyz: std::array::from_fn(|k| mix(p.xyz[k], q.xyz[k])) }).collect();
				WaterBatch { verts, ..y.clone() }
			})
			.collect(),
		u,
	))
}

/// [PC] Adds the gsfull fields newer than the newest kept to `hist` (at most `HIST`, oldest dropped); fields older than the newest
/// kept start it again (a savestate load: gsfull's history starts again too).
fn remember(hist: &mut VecDeque<(u64, Arc<Vec<WaterBatch>>)>, fields: &[(u64, Arc<Vec<WaterBatch>>)]) {
	if fields.last().is_some_and(|l| hist.back().is_some_and(|h| l.0 < h.0)) {
		hist.clear();
	}
	for f in fields {
		if hist.back().is_none_or(|h| f.0 > h.0) {
			hist.push_back(f.clone());
		}
	}
	while hist.len() > HIST {
		hist.pop_front();
	}
}

/// [PC] The water of held scene `scene` (frozen screen): the newest kept field not newer than `scene + CAMERA_LAG` (the camera rule of
/// the live picture), else the oldest kept (the closest to the scene); None with nothing kept.
fn held_water(hist: &VecDeque<(u64, Arc<Vec<WaterBatch>>)>, scene: u64) -> Option<&Arc<Vec<WaterBatch>>> {
	hist.iter().rev().find(|f| f.0 <= scene + CAMERA_LAG).or(hist.front()).map(|f| &f.1)
}

/// A batch's triangles by its GS PRIM: 3 list, 4 strip, 5 fan.
fn triangles(b: &WaterBatch) -> impl Iterator<Item = [&WVert; 3]> {
	let v = &b.verts;
	let n = v.len();
	let (prim, count) = match b.prim & 7 {
		3 => (3, n / 3),
		4 | 5 => (b.prim & 7, n.saturating_sub(2)),
		_ => (0, 0),
	};
	(0..count).map(move |i| match prim {
		3 => [&v[i * 3], &v[i * 3 + 1], &v[i * 3 + 2]],
		4 => [&v[i], &v[i + 1], &v[i + 2]],
		_ => [&v[0], &v[i + 1], &v[i + 2]],
	})
}

/// A triangle's area (world units squared).
fn tri_area(p: [[f32; 3]; 3]) -> f32 {
	let (a, b) = (std::array::from_fn::<f32, 3, _>(|k| p[1][k] - p[0][k]), std::array::from_fn::<f32, 3, _>(|k| p[2][k] - p[0][k]));
	let c = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
	0.5 * (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt()
}

/// One triangle as a quad with its last corner doubled. GS modulate: colour and alpha are vertex byte / 128 (texreq's texture
/// alpha has 0x80 = 255); ALPHA A = Cs, B = 0, D = Cd is additive [C: level 1 water layer 2, ALPHA 0x8000000048], the rest
/// source-alpha-over.
fn quad(t: [&WVert; 3], key: usize, b: &WaterBatch) -> WorldQuad {
	let c = [t[0], t[1], t[2], t[2]];
	let uv = |w: &WVert| {
		let q = if w.st[2] != 0.0 { w.st[2] } else { 1.0 };
		[w.st[0] / q, w.st[1] / q]
	};
	WorldQuad {
		pos: c.map(|w| w.xyz),
		uv: c.map(uv),
		colour: c.map(|w| w.rgba.map(|x| x as f32 / 128.0)),
		tex: key,
		additive: b.alpha & 0x3f == 0x08,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn batch(prim: u64, n: usize) -> WaterBatch {
		let verts = (0..n).map(|i| WVert { st: [i as f32, 0.0, 1.0], rgba: [128; 4], xyz: [i as f32, 0.0, 0.0] }).collect();
		WaterBatch { prim, alpha: 0x44, verts, ..Default::default() }
	}

	#[test]
	fn blend_follows_the_camera() {
		let at = |x: f32| Arc::new(vec![WaterBatch { verts: vec![WVert { xyz: [x, 0.0, 0.0], ..Default::default() }; 3], prim: 3, ..Default::default() }]);
		let f = vec![(10, at(0.0)), (11, at(2.0))];
		let x = |c: u64, t: f32| blend_at(&f, c, t).map(|b| b.0[0].verts[0].xyz[0]);
		// Field 11's water is built around camera 10: camera 10 + 1/2 is a field past it.
		assert_eq!((x(9, 0.0), x(9, 0.5), x(10, 0.0), x(10, 0.5)), (Some(0.0), Some(1.0), Some(2.0), Some(3.0)));
		// At most one field past the newer one; none before the older one.
		assert_eq!((x(12, 0.5), x(8, 0.0)), (Some(4.0), None));
		// Other batches (count, texture, vertex count): no blend.
		let g = vec![(10, at(0.0)), (11, Arc::new(vec![]))];
		assert!(blend_at(&g, 10, 0.5).is_none() && blend_at(&[(10, at(0.0)), (12, at(1.0))], 10, 0.5).is_none());
	}

	#[test]
	fn a_frozen_screen_draws_the_held_scenes_water() {
		// Fields of n batches each, as gsfull publishes them (its own history keeps 4).
		let published = |a: u64, z: u64, n: fn(u64) -> usize| (a..=z).map(|f| (f, Arc::new(vec![WaterBatch::default(); n(f)]))).collect::<Vec<_>>();
		let sea = |f: u64| if f < 303 { 12 } else { 0 }; // the pause: the game sends no water from field 303 on
		let mut hist = VecDeque::new();
		for top in 290..=303 {
			remember(&mut hist, &published(top - 3, top, sea));
		}
		// 287..=303 seen, the newest 16 kept
		assert_eq!((hist.front().map(|f| f.0), hist.back().map(|f| f.0), hist.len()), (Some(288), Some(303), 16));
		// freeze.rs holds the scene of field 301: its water is field 302's (CAMERA_LAG), however long the menu stays up (nothing is
		// remembered while held).
		assert_eq!(held_water(&hist, 301).map(|b| b.len()), Some(12));
		// a held scene older than everything kept: the oldest kept; nothing kept: none
		assert_eq!(held_water(&hist, 100).map(|b| b.len()), Some(12));
		assert!(held_water(&VecDeque::new(), 301).is_none());
		// repeated and overlapping publishes add only newer fields; a savestate load (older fields) starts again
		remember(&mut hist, &published(300, 303, sea));
		assert_eq!(hist.len(), 16);
		remember(&mut hist, &published(10, 12, sea));
		assert_eq!(hist.iter().map(|f| f.0).collect::<Vec<_>>(), vec![10, 11, 12]);
	}

	#[test]
	fn a_changed_texture_version_is_uploaded_again() {
		// [b52 H1] The first read may be of other texels: the key stays, the new version replaces what is under it.
		assert!(needs_upload(None, 5));
		assert!(!needs_upload(Some(5), 5));
		assert!(needs_upload(Some(5), 1048577));
	}

	#[test]
	fn prims_to_triangles() {
		let first = |b: &WaterBatch| triangles(b).map(|t| t.map(|v| v.xyz[0] as u32)).collect::<Vec<_>>();
		assert_eq!(first(&batch(3, 7)), vec![[0, 1, 2], [3, 4, 5]]);
		assert_eq!(first(&batch(4, 5)), vec![[0, 1, 2], [1, 2, 3], [2, 3, 4]]);
		assert_eq!(first(&batch(5, 4)), vec![[0, 1, 2], [0, 2, 3]]);
		assert!(first(&batch(6, 4)).is_empty() && first(&batch(4, 1)).is_empty());
		let b = batch(3, 3);
		let q = quad(triangles(&b).next().unwrap(), 7, &b);
		assert_eq!((q.pos[3], q.colour[0], q.additive), ([2.0, 0.0, 0.0], [1.0; 4], false));
	}
}
