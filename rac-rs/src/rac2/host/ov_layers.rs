//! The world layers the GS captured (ship canopy glass, lamp / nanotech / bolt glows; engine `gsfull/layer_tris.rs`) as world-space
//! triangles for the engine's particle draw (round 3 batch 4: issues 3 / 6 / 7 / 9 / 14 / 15). Each overlay field's triangles are
//! unprojected with the game's own projection (screen pixel + GS depth -> view ray * depth) and the newest camera, then stay in the
//! world: the engine draws them at the window's resolution, at the smoothed camera, depth-tested against the native world. A frozen
//! menu field (pause) has none of its own: the last ones stay, as the world does. `RAC2_LAYERS=0` keeps them in the overlay.

use crate::scene::View;
use rac_engine::gsfull::fx_native::WVert;
use rac_engine::gsfull::layer_tris::{LayerTri, LayerVert, depth_m};
use rac_engine::viewer::Gpu;
use rac_engine::viewer::gpu::layer_fx::WorldTri;

/// Is the native draw of the layers on (default; `RAC2_LAYERS=0` = off)?
pub(super) fn enabled() -> bool {
	std::env::var_os("RAC2_LAYERS").is_none_or(|v| v != "0")
}

/// Is the native draw of the effect program (sparks, smoke, rain, fire, trails: engine `fx_native.rs`) on? Default on with the layers;
/// `RAC2_FXNATIVE=0` keeps the effects in the GS overlay (old path).
pub(super) fn fx_enabled() -> bool {
	enabled() && std::env::var_os("RAC2_FXNATIVE").is_none_or(|v| v != "0")
}

/// The least an effect corner is drawn in front of its own point (m): a ring or a spark lying on a surface would otherwise pass the
/// depth test pixel by pixel or not at all.
const FX_BIAS: f32 = 0.05;

/// A camera-facing sprite is drawn this many of its own radii (world metres at its depth) in front of its point, at most `FX_BIAS_MAX`:
/// a glow is centred ON the bolt / crate / lamp it belongs to, so the thing's own surface lies up to a radius in front of the centre
/// and hid the whole glow at 0.05 m [C: rec_user8 field 6400, GS depth = record depth = 8.841 m exactly; bias 0.05 m: no glow, 1.0 m: glow].
const FX_BIAS_RADII: f32 = 1.0;
const FX_BIAS_MAX: f32 = 1.5;

/// A glow centred inside Ratchet or his held weapon (`Scene::hero_group`) is pulled this little: retail draws the hero group AFTER the
/// effect sprites (the glow is the last write of the frame at the pixels probed away from him, and the held gun hides it), so his
/// body must occlude it; a glow centred on a bolt / crate keeps the big pull [C: F1 pixel probes rec_user8 field 4000 / 6400, handoff
/// BATCH 8 PART 4; picture proof runs8_cmp11.png (gun) and b8_cmp12.png (the bolt dies with the small pull)].
const HERO_BIAS_RADII: f32 = 0.1;
const HERO_UNPROJECT_ABS: f32 = 0.02;
const UNPROJECT_ABS: f32 = 0.15;

/// Is the world point inside one of the `hero` spheres (x, y, z, radius)?
fn in_hero(hero: &[[f32; 4]], p: [f32; 3]) -> bool {
	hero.iter().any(|s| (0..3).map(|k| (p[k] - s[k]).powi(2)).sum::<f32>() <= s[3] * s[3])
}

/// The world point of an effect corner: its 3D point, plus (a camera-facing sprite) its pixel offset across the view at the point's depth.
/// The program's points are relative to the camera (its matrix has no translation: clip.xyw = 0 at the origin) [C: vf31 = (0, 0, z0, 0)
/// in every run of rec_oozla2 / rec_user8], so the world point is the eye plus the record's point.
///
/// `tested`: the prim is depth-tested (else it is drawn over the world and the pull is only the small `FX_BIAS`). The pull is always
/// applied and never more than a quarter of the distance to the eye, so a big sprite near the camera has no cliff at some distance.
pub(super) fn place(v: &View, w: &WVert, tested: bool, hero: &[[f32; 4]]) -> [f32; 3] {
	let col = |j: usize, k: usize| v.basis[k][j];
	let rel = w.p;
	let depth: f32 = (0..3).map(|k| rel[k] * col(2, k)).sum();
	let (a, b) = (w.off[0] / v.fx * depth, w.off[1] / v.fy * depth);
	let at: [f32; 3] = std::array::from_fn(|k| v.eye[k] + w.p[k] + a * col(0, k) + b * col(1, k));
	let d: [f32; 3] = std::array::from_fn(|k| at[k] - v.eye[k]);
	let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
	// The sprite's own point (the centre, not the corner) decides whether it belongs to the hero group.
	let radii = if in_hero(hero, std::array::from_fn(|k| v.eye[k] + w.p[k])) { HERO_BIAS_RADII } else { FX_BIAS_RADII };
	let bias = if tested { (radii * (a * a + b * b).sqrt()).clamp(FX_BIAS, FX_BIAS_MAX) } else { FX_BIAS };
	let bias = bias.min(0.25 * len);
	if len > 1e-4 { std::array::from_fn(|k| at[k] - d[k] / len * bias) } else { at }
}

/// The game's cameras of the last fields, by field number: the GS draws field N while the game thread may already have published N + 1.
#[derive(Default)]
pub(super) struct Views(std::collections::VecDeque<(u64, Option<View>)>, std::collections::VecDeque<(u64, Vec<[f32; 4]>)>);

/// Fields between the camera a picture is built from and the picture's own field number.
const GS_CAMERA_LAG: u64 = 1;

/// The game thread published the camera (or none) of `field`: the pictures from `field + GS_CAMERA_LAG` on are placed with it.
pub(super) fn note_camera(field: u64, has: bool) {
	rac_engine::gsfull::layer_tris::note_camera(field + GS_CAMERA_LAG, has);
}

impl Views {
	/// The GS pipeline holds up to 5 fields between the game thread and the picture being drawn, plus the lag of 1.
	const KEEP: usize = 16;

	pub(super) fn push(&mut self, field: u64, view: Option<View>) {
		if self.0.len() == Self::KEEP {
			self.0.pop_front();
		}
		self.0.push_back((field, view));
	}

	/// The hero group (`Scene::hero_group`) of `field`.
	pub(super) fn note_hero(&mut self, field: u64, hero: Vec<[f32; 4]>) {
		if self.1.len() == Self::KEEP {
			self.1.pop_front();
		}
		self.1.push_back((field, hero));
	}

	/// The hero group of the picture of `field`: that of the nearest field kept that is not newer than the camera's (as `of_picture`).
	pub(super) fn hero_of_picture(&self, field: u64) -> Vec<[f32; 4]> {
		let want = field.wrapping_sub(GS_CAMERA_LAG);
		self.1.iter().filter(|e| e.0 <= want).max_by_key(|e| e.0).or_else(|| self.1.iter().min_by_key(|e| e.0)).map(|e| e.1.clone()).unwrap_or_default()
	}

	/// The camera of `field` (None: not kept any more, or the field had no 3D camera).
	pub(super) fn get(&self, field: u64) -> Option<View> {
		self.0.iter().rev().find(|e| e.0 == field).and_then(|e| e.1)
	}

	/// The camera a GS picture of `field` was drawn with: the game computes the camera at the end of a field and the GS draws with it in
	/// the next, so it is the one published with the field before [C: RAC2_LAYERDRIFT on rec_oozla2, fields 2231-2420: a static
	/// vertex's rebuilt world point jitters least with the camera of field - 1 (0.7 cm, the depth quantisation) and twice as much
	/// with field - 2 or field itself (V-shaped around - 1); the one-field lag made the layers trail the world while the camera moved].
	///
	/// A camera no longer kept is replaced by the nearest older one kept (never a newer one: the picture of a GS-bound field lags the
	/// game by several fields, and the newest camera would shift the layers by all of them); before the oldest kept, the oldest.
	pub(super) fn of_picture(&self, field: u64) -> Option<View> {
		let want = field.wrapping_sub(GS_CAMERA_LAG);
		let e = self.0.iter().filter(|e| e.0 <= want).max_by_key(|e| e.0).or_else(|| self.0.iter().min_by_key(|e| e.0))?;
		e.1
	}
}

/// RAC2_LAYERTIME is set (read once): the host's share of the layers' cost is logged too (`layerhost`).
fn timing() -> bool {
	static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
	*ON.get_or_init(|| std::env::var_os("RAC2_LAYERTIME").is_some_and(|v| v != "0"))
}

/// RAC2_OVLKIND is set (read once).
fn log_on() -> bool {
	static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
	*ON.get_or_init(|| std::env::var_os("RAC2_OVLKIND").is_some())
}

/// [b51 G6] One stderr line in ONE write. `eprintln!` on the unbuffered stderr writes every format piece separately (each float is
/// 2-3 pieces): to the run's redirected err.txt the 9-value `layersteps:` line cost 4.5-4.7 ms per field on the window thread, in
/// the `pick` stage, and `layerhost:` 0.7 ms; preformatted, one write is 0.12 ms (b51\g6\ep, micro test of the same lines).
fn err_line(s: String) {
	use std::io::Write;
	let _ = std::io::stderr().write_all(s.as_bytes());
}

/// The point of a layer vertex in the world: the view ray through its screen pixel at its view depth.
pub(super) fn unproject(v: &View, p: &LayerVert, hero: &[[f32; 4]]) -> [f32; 3] {
	// In front of its own depth by 0.2 % + 0.15 m: the GS and this renderer round the same surface differently, and a glow lying flush on
	// the ground would pass the depth test pixel by pixel or not at all. (Was 1.2 %: a point put 0.7 m too near at 60 m shifts against the
	// world by parallax whenever the camera moves.)
	let d = depth_m(p.z) / 1.002;
	let at = ray(v, p);
	// A point on the hero group is pulled only a little (see `HERO_BIAS_RADII`).
	let abs = if in_hero(hero, at(d)) { HERO_UNPROJECT_ABS } else { UNPROJECT_ABS };
	at((d - abs).max(0.1))
}

/// The point of a layer vertex that WRITES depth (a replace sheet with ZMSK off: level 14's lava) at its own view depth: `unproject`'s
/// pull lets a blended layer win the depth ties against the surface it lies on, but a pulled sheet that writes depth covers everything
/// up to 0.2 % + 0.15 m above it [C: the rims of the sunk platforms in the lava, q60903 field 60 vs RAC2_PICTURE=gs].
pub(super) fn unproject_exact(v: &View, p: &LayerVert) -> [f32; 3] {
	ray(v, p)(depth_m(p.z))
}

/// The world point at view depth d on the view ray through a layer vertex's screen pixel.
fn ray<'a>(v: &'a View, p: &LayerVert) -> impl Fn(f32) -> [f32; 3] + 'a {
	let (a, b) = ((p.px - v.half[0]) / v.fx, (p.py - v.half[1]) / v.fy);
	move |d| std::array::from_fn(|k| v.eye[k] + d * (v.basis[k][2] + a * v.basis[k][0] + b * v.basis[k][1]))
}

/// RAC2_LAYERDRIFT=1: for the first triangles of a field, the world point of their first vertex rebuilt with the cameras of the fields
/// around it (d = -2..1) and without the depth bias: a static object's point must be the same in every field for the right d.
pub(super) fn drift_log(field: u64, tris: &[LayerTri], views: &Views) {
	static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
	if !*ON.get_or_init(|| std::env::var_os("RAC2_LAYERDRIFT").is_some()) {
		return;
	}
	for d in -2i64..=1 {
		let Some(v) = views.get((field as i64 + d) as u64) else { continue };
		for (i, t) in tris.iter().take(8).enumerate() {
			let p = &t.v[0];
			let dm = depth_m(p.z);
			let (a, b) = ((p.px - v.half[0]) / v.fx, (p.py - v.half[1]) / v.fy);
			let w: [f32; 3] = std::array::from_fn(|k| v.eye[k] + dm * (v.basis[k][2] + a * v.basis[k][0] + b * v.basis[k][1]));
			eprintln!("drift f{field} d{d} t{i} id{:x} n{} px{:.2} py{:.2} z{dm:.3} w {:.3} {:.3} {:.3} eye {:.3} {:.3} {:.3}", t.id, tris.len(), p.px, p.py, w[0], w[1], w[2], v.eye[0], v.eye[1], v.eye[2]);
		}
	}
}

/// The world triangles of one field's layers (those with a depth the projection can place are kept).
pub(super) fn world(tris: &[LayerTri], v: &View, hero: &[[f32; 4]]) -> Vec<WorldTri> {
	world_src(tris, v, hero).0
}

/// `world`, plus for each world triangle the index of the `LayerTri` it came from (host/ov_fire.rs).
pub(super) fn world_src(tris: &[LayerTri], v: &View, hero: &[[f32; 4]]) -> (Vec<WorldTri>, Vec<usize>) {
	tris.iter()
		.enumerate()
		.filter(|(_, t)| t.world.is_some() || t.v.iter().all(|p| p.z > 0 && p.z < 0x00ff_0000))
		.map(|(i, t)| {
			let col = |p: &LayerVert| {
				// Decal: the texel is the colour (alpha too). The fog factor scales the colour, not the alpha. An untextured prim (rain /
				// spark lines) has the vertex colour as its pixel colour: rgb / 255 (a textured one modulates: texel * rgb / 128); alpha is
				// the blend factor in both (/ 128).
				let mut c = if t.decal { [1.0; 4] } else { p.rgba.map(|x| x as f32 / 128.0) };
				if t.untextured {
					c[..3].iter_mut().zip(p.rgba).for_each(|(x, r)| *x = r as f32 / 255.0);
				}
				c[..3].iter_mut().for_each(|x| *x *= p.f as f32 / 255.0);
				c
			};
			let pos = match &t.world {
				Some(w) => w.map(|c| place(v, &c, t.depth, hero)),
				// A sheet that writes depth sits at its true depth (`unproject_exact`).
				None if t.replace && t.zwrite => t.v.map(|p| unproject_exact(v, &p)),
				None => t.v.map(|p| unproject(v, &p, hero)),
			};
			(WorldTri { pos, uv: t.v.map(|p| p.uv), col: t.v.map(|p| col(&p)), id: t.id, tex: t.tex.clone(), additive: t.additive, depth: t.depth, effect: t.world.is_some(), replace: t.replace, zwrite: t.zwrite, alpha: t.alpha }, i)
		})
		.filter(|w| w.0.pos.iter().flatten().all(|x| x.is_finite()))
		.unzip()
}

/// The layers in the engine, and what the frames since decided.
#[derive(Default)]
pub(super) struct Layers {
	/// Hand the engine an empty list once (the layers of the last overlay field are gone: a whole-picture field).
	cleared: bool,
	last: (bool, bool),
	/// The two newest fields' triangles, blended to the smoothed camera at each redraw (ov_blend.rs).
	history: super::ov_blend::History,
	/// [PC] Fire / Smoke / weapon lights on the effect quads (ov_fire.rs).
	fire: super::ov_fire::Fire,
	/// [PC] While Fire / Smoke / weapon lights are on: the triangles last handed to `set_layer_tris` by the plain paths. A redraw
	/// without a blend (`History::at` None) hands nothing, so the engine keeps that list; ov_fire.rs re-hands exactly it (the newest
	/// field's list instead differed after a blended redraw: 0027 / 0014, 4-813 px with no quad matched).
	handed: Option<Vec<WorldTri>>,
	/// [PC] b47 X2: the heat haze's lava boxes of the lava sheet (ov_lava.rs), fed by host/frame.rs after `update`.
	pub(super) lava: super::ov_lava::Lava,
	/// [b51 G4] RAC2_LAYERTIME: the redraws since the last field, summed (n, `history.at`, the copies, `fire.step` / plain hand-over: ms).
	acc: (u32, [f64; 3]),
}

impl Layers {
	/// A new GS frame: `overlay`/`frozen` as in `Frame`; `tris` its layers; `view` the camera of the frame's own field; `show`: the
	/// engine draws the world of this level (else the camera would be the idle one and the layers would hang in front of nothing).
	/// `field`: the frame's own field.
	#[allow(clippy::too_many_arguments)]
	pub(super) fn update(&mut self, g: &mut Gpu, overlay: bool, frozen: bool, field: u64, tris: &[LayerTri], view: Option<&View>, hero: &[[f32; 4]], show: bool) {
		if self.last != (overlay, frozen) && log_on() {
			eprintln!("layers: frame is now overlay {overlay} frozen {frozen} (view {}, {} tris)", view.is_some(), tris.len());
			self.last = (overlay, frozen);
		}
		match (overlay, frozen, view.filter(|_| show)) {
			(true, false, Some(v)) => {
				if !tris.is_empty() && log_on() {
					let near: Vec<&LayerTri> = tris.iter().filter(|t| depth_m(t.v[0].z) < 25.0).collect();
					let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
					for p in near.iter().flat_map(|t| t.v.iter()) {
						(lo, hi) = ([lo[0].min(p.px), lo[1].min(p.py)], [hi[0].max(p.px), hi[1].max(p.py)]);
					}
					eprintln!("layers: {} tris, {} textures; {} nearer than 25 m, box {lo:?}..{hi:?}", tris.len(), tris.iter().map(|t| t.id).collect::<std::collections::HashSet<_>>().len(), near.len());
				}
				let t0 = timing().then(std::time::Instant::now);
				// [b51 G4] Step timers (RAC2_LAYERTIME only; `lap` is None otherwise and costs nothing).
				let mut lap = t0.map(|t| (t, [0.0f64; 5]));
				let mut tick = |i: usize| {
					if let Some((t, ms)) = lap.as_mut() {
						let now = std::time::Instant::now();
						ms[i] = now.duration_since(*t).as_secs_f64() * 1e3;
						*t = now;
					}
				};
				let (w, src) = world_src(tris, v, hero);
				tick(0);
				g.set_layer_tris(&w);
				tick(1);
				self.handed = super::ov_fire::gate().then(|| super::ov_blend::dup(&w));
				tick(2);
				self.fire.note(field, tris, &src, &w, v);
				tick(3);
				if let Some(t0) = t0 {
					err_line(format!("layerhost: {} tris placed + handed to the GPU in {:.3} ms\n", w.len(), t0.elapsed().as_secs_f64() * 1e3)); // [b51 G6]
				}
				let n = w.len();
				self.history.push(field.wrapping_sub(GS_CAMERA_LAG), w);
				tick(4);
				if let Some((_, ms)) = lap {
					let (rn, r) = std::mem::take(&mut self.acc);
					err_line(format!( // [b51 G6]
						"layersteps: field {field} tris {n} world_src {:.3} set_layer_tris {:.3} handed_dup {:.3} fire_note {:.3} history_push {:.3} | {rn} redraws since: history_at {:.3} copies {:.3} fire_step {:.3} (ms)\n",
						ms[0], ms[1], ms[2], ms[3], ms[4], r[0], r[1], r[2]
					));
				}
				self.cleared = false;
			}
			// No camera to place them with: none are drawn (the next field with one brings its own).
			(true, false, None) if !self.cleared => {
				g.set_layer_tris(&[]);
				self.history.clear();
				self.fire.clear();
				self.handed = Some(Vec::new());
				self.cleared = true;
			}
			// A frozen menu field keeps the last layers (the world is held too).
			(true, true, _) => {}
			(false, ..) if !self.cleared => {
				g.set_layer_tris(&[]);
				self.history.clear();
				self.fire.clear();
				self.handed = Some(Vec::new());
				self.cleared = true;
			}
			_ => {}
		}
	}

	/// A redraw at the smoothed camera `blended` (smooth.rs `Smooth::blended`, None = the newest field as it is): the layers between
	/// the two newest fields (ov_blend.rs).
	/// `cam`: the engine camera of this redraw (None = no world drawn). With Fire, Smoke or the weapon lights on, the layers go through
	/// ov_fire.rs at every redraw (`set_particles` replaces the particle lists, the layer runs with them).
	pub(super) fn redraw(&mut self, g: &mut Gpu, blended: Option<(u64, f32)>, cam: Option<&rac_engine::viewer::Camera>) {
		let t0 = timing().then(std::time::Instant::now); // [b51 G4] step timers, RAC2_LAYERTIME only
		let w = self.history.at(blended);
		let t1 = t0.map(|_| std::time::Instant::now());
		let on = fx_enabled() && super::ov_fire::gate();
		crate::fx_pool::set_wanted(on);
		// What the plain path leaves in the engine: this redraw's blend, else the list handed last (else the newest field's).
		let dup = |v: &Option<Vec<WorldTri>>| v.as_deref().map(super::ov_blend::dup);
		let kept = (on || self.fire.active()).then(|| dup(&w).or_else(|| dup(&self.handed)).or_else(|| self.history.newest())).flatten();
		self.handed = if on { dup(&kept) } else { None };
		let t2 = t0.map(|_| std::time::Instant::now());
		match cam {
			Some(c) if on => self.fire.step(g, kept.unwrap_or_default(), c),
			Some(c) if self.fire.active() => self.fire.off(g, kept, c),
			_ => {
				if let Some(w) = w {
					g.set_layer_tris(&w);
				}
			}
		}
		if let (Some(t0), Some(t1), Some(t2)) = (t0, t1, t2) {
			let ms = |a: std::time::Instant, b: std::time::Instant| b.duration_since(a).as_secs_f64() * 1e3;
			let end = std::time::Instant::now();
			self.acc.0 += 1;
			self.acc.1[0] += ms(t0, t1);
			self.acc.1[1] += ms(t1, t2);
			self.acc.1[2] += ms(t2, end);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn view() -> View {
		// Rows = world axes, columns right, down, forward: forward = world +y, right = +x, down = -z.
		View { eye: [10.0, 20.0, 5.0], basis: [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]], fx: 400.0, fy: 400.0, half: [256.0, 224.0] }
	}

	#[test]
	fn the_gs_fog_factor_scales_the_colour_not_the_alpha() {
		let z = (16777216.0f32 / 320.0 - 480.0) as u32;
		let p = |f| LayerVert { px: 256.0, py: 224.0, z, rgba: [128, 64, 128, 100], uv: [0.0; 2], f };
		let tex = std::sync::Arc::new(rac_formats::level::Texture { w: 1, h: 1, rgba: vec![[255; 4]] });
		let t = LayerTri { v: [p(255), p(204), p(0)], tex, id: 1, additive: false, decal: false, world: None, depth: true, untextured: false, replace: false, zwrite: false, alpha: Default::default() };
		let w = world(&[t], &view(), &[]);
		assert_eq!(w[0].col[0], [1.0, 0.5, 1.0, 100.0 / 128.0]);
		assert!((w[0].col[1][0] - 0.8).abs() < 1e-6 && (w[0].col[1][3] - 100.0 / 128.0).abs() < 1e-6);
		assert_eq!(w[0].col[2][..3], [0.0; 3]);
	}

	#[test]
	fn a_frame_is_placed_with_the_camera_of_its_own_field() {
		let mut v = Views::default();
		let (a, b) = (view(), View { eye: [1.0, 2.0, 3.0], ..view() });
		v.push(10, Some(a));
		v.push(11, Some(b));
		v.push(12, None);
		assert_eq!(v.of_picture(11).map(|x| x.eye), Some(a.eye), "a picture of field 11 was drawn with the camera of field 10");
		assert_eq!(v.get(10).map(|x| x.eye), Some(a.eye));
		assert_eq!(v.get(11).map(|x| x.eye), Some(b.eye));
		assert_eq!(v.get(12), None, "a field without a 3D camera");
		for f in 13..10 + Views::KEEP as u64 + 1 {
			v.push(f, Some(a));
		}
		assert_eq!(v.get(10), None, "only the last fields are kept");
	}

	#[test]
	fn a_picture_whose_camera_was_evicted_gets_the_nearest_older_one_not_the_newest() {
		let mut v = Views::default();
		// Camera x = field number; keep the last KEEP of 100 fields.
		for f in 0..100u64 {
			v.push(f, Some(View { eye: [f as f32, 0.0, 0.0], ..view() }));
		}
		assert_eq!(v.of_picture(100).map(|x| x.eye[0]), Some(99.0), "field 100 is drawn with field 99");
		assert_eq!(v.of_picture(95).map(|x| x.eye[0]), Some(94.0));
		let oldest = 100 - Views::KEEP as u64;
		assert_eq!(v.of_picture(oldest + 1).map(|x| x.eye[0]), Some(oldest as f32));
		assert_eq!(v.of_picture(oldest - 5).map(|x| x.eye[0]), Some(oldest as f32), "before the oldest kept: the oldest, not the newest");
		// A gap (no camera pushed for some fields): the nearest older.
		let mut g = Views::default();
		g.push(10, Some(View { eye: [10.0, 0.0, 0.0], ..view() }));
		g.push(14, Some(View { eye: [14.0, 0.0, 0.0], ..view() }));
		assert_eq!(g.of_picture(13).map(|x| x.eye[0]), Some(10.0));
		assert_eq!(g.of_picture(15).map(|x| x.eye[0]), Some(14.0));
		assert!(Views::KEEP >= 12, "the pipeline depth (5) plus the lag (1) with margin");
	}

	#[test]
	fn the_centre_pixel_is_straight_ahead_at_the_view_depth() {
		let z = (16777216.0f32 / 320.0 - 480.0) as u32; // 10 m, drawn 0.2 % + 0.15 m closer
		let p = LayerVert { px: 256.0, py: 224.0, z, rgba: [128; 4], uv: [0.0; 2], f: 255 };
		let w = unproject(&view(), &p, &[]);
		assert!((w[0] - 10.0).abs() < 0.01 && (w[1] - (20.0 + 10.0 / 1.002 - 0.15)).abs() < 0.05 && (w[2] - 5.0).abs() < 0.01, "{w:?}");
	}

	#[test]
	fn an_effect_corner_is_its_world_point_plus_a_pixel_offset_at_its_depth() {
		// 10 m ahead: 40 px right and down of a 400 px focal length = 1 m right, 1 m down; a sprite of that radius (1.41 m) is then drawn
		// 1.41 m (its radius) closer to the eye along the ray.
		let w = place(&view(), &WVert { p: [0.0, 10.0, 0.0], off: [40.0, 40.0] }, true, &[]);
		let (at, len) = ([11.0f32, 30.0, 4.0], (1.0f32 + 100.0 + 1.0).sqrt());
		let d = [1.0 / len, 10.0 / len, -1.0 / len];
		let r = 2.0f32.sqrt();
		for k in 0..3 {
			assert!((w[k] - (at[k] - d[k] * r)).abs() < 0.01, "{k}: {w:?}");
		}
		let g = place(&view(), &WVert { p: [2.0, 10.0, 0.0], off: [0.0; 2] }, true, &[]);
		assert!((g[0] - 12.0).abs() < 0.06 && (g[2] - 5.0).abs() < 0.06 && g[1] < 30.0 && g[1] > 29.9, "a world corner stays where it is: {g:?}");
		let big = place(&view(), &WVert { p: [0.0, 10.0, 0.0], off: [400.0, 0.0] }, true, &[]);
		assert!((big[1] - 30.0).abs() < 1.6 && big[1] < 30.0, "the bias is capped: {big:?}");
		// Near the camera there is no cliff: a 2 m sprite 2 m ahead is pulled a quarter of its distance, not 0 and not 1.5 m.
		let near = place(&view(), &WVert { p: [0.0, 2.0, 0.0], off: [400.0, 0.0] }, true, &[]);
		let len = (4.0f32 + 4.0).sqrt();
		let at = [12.0f32, 22.0, 5.0];
		// (distance from the eye: three quarters of the way)
		let pulled = ((near[0] - 10.0).powi(2) + (near[1] - 20.0).powi(2) + (near[2] - 5.0).powi(2)).sqrt();
		assert!((pulled - 0.75 * len).abs() < 0.01 && near[0] < at[0], "{near:?} {pulled}");
		// A prim without a depth test only gets the small pull.
		let flat = place(&view(), &WVert { p: [0.0, 10.0, 0.0], off: [400.0, 0.0] }, false, &[]);
		let pull = ((flat[0] - 20.0).powi(2) + (flat[1] - 30.0).powi(2) + (flat[2] - 5.0).powi(2)).sqrt();
		assert!((pull - FX_BIAS).abs() < 1e-3, "{flat:?} {pull}");
	}

	#[test]
	fn a_glow_centred_on_the_hero_group_is_pulled_only_a_little() {
		// The view's eye is (10, 20, 5); a point 10 m ahead is (10, 30, 5). A sprite of radius 1.41 m there.
		let w = WVert { p: [0.0, 10.0, 0.0], off: [40.0, 40.0] };
		let far = |p: [f32; 3]| ((p[0] - 11.0).powi(2) + (p[1] - 30.0).powi(2) + (p[2] - 4.0).powi(2)).sqrt();
		let (free, held) = (place(&view(), &w, true, &[]), place(&view(), &w, true, &[[10.0, 30.0, 5.0, 0.7]]));
		let elsewhere = place(&view(), &w, true, &[[50.0, 30.0, 5.0, 0.7]]);
		assert!((far(free) - 1.41).abs() < 0.02 && (far(held) - 0.141).abs() < 0.02, "{free:?} {held:?}");
		assert_eq!(free, elsewhere, "a sphere that does not hold the centre changes nothing");
	}

	#[test]
	fn an_untextured_prim_is_its_vertex_colour_not_twice_as_bright() {
		let z = (16777216.0f32 / 320.0 - 480.0) as u32;
		let p = LayerVert { px: 256.0, py: 224.0, z, rgba: [255, 128, 0, 64], uv: [0.0; 2], f: 255 };
		let tex = std::sync::Arc::new(rac_formats::level::Texture { w: 1, h: 1, rgba: vec![[255; 4]] });
		let mut t = LayerTri { v: [p, p, p], tex, id: 0, additive: false, decal: false, world: None, depth: true, untextured: true, replace: false, zwrite: false, alpha: Default::default() };
		let c = world(&[t.clone()], &view(), &[])[0].col[0];
		assert_eq!(c, [1.0, 128.0 / 255.0, 0.0, 0.5], "rgb / 255, alpha / 128");
		t.untextured = false;
		assert_eq!(world(&[t], &view(), &[])[0].col[0], [255.0 / 128.0, 1.0, 0.0, 0.5], "a textured prim modulates with rgb / 128");
	}

	#[test]
	fn a_sheet_that_writes_depth_lands_at_its_own_depth_unpulled() {
		// 10 m ahead (the default Z offset 480), 40 px right of the centre.
		let z = (16777216.0f32 / 320.0 - 480.0) as u32;
		let p = LayerVert { px: 256.0 + 40.0, py: 224.0, z, rgba: [128; 4], uv: [0.0; 2], f: 255 };
		let tex = std::sync::Arc::new(rac_formats::level::Texture { w: 1, h: 1, rgba: vec![[255; 4]] });
		let mut t = LayerTri { v: [p; 3], tex, id: 0, additive: false, decal: false, world: None, depth: true, untextured: false, replace: true, zwrite: true, alpha: Default::default() };
		let ahead = |w: &[WorldTri]| w[0].pos[0][1] - 20.0;
		let d = depth_m(z);
		let exact = world(&[t.clone()], &view(), &[]);
		assert!((ahead(&exact) - d).abs() < 1e-4 && (exact[0].pos[0][0] - (10.0 + d * 0.1)).abs() < 1e-4, "{:?}", exact[0].pos[0]);
		assert_eq!(exact[0].pos[0], unproject_exact(&view(), &p));
		// The same sheet without depth writes, and a blended layer: pulled 0.2 % + 0.15 m toward the eye.
		for (replace, zwrite) in [(true, false), (false, false)] {
			(t.replace, t.zwrite) = (replace, zwrite);
			let pulled = world(&[t.clone()], &view(), &[]);
			assert!((ahead(&pulled) - (d / 1.002 - 0.15)).abs() < 1e-3, "replace {replace} zwrite {zwrite}: {:?}", pulled[0].pos[0]);
		}
	}

	#[test]
	fn a_pixel_right_and_below_the_centre_lands_right_and_below() {
		let z = (16777216.0f32 / 320.0 - 480.0) as u32;
		let p = LayerVert { px: 256.0 + 40.0, py: 224.0 + 40.0, z, rgba: [128; 4], uv: [0.0; 2], f: 255 };
		let w = unproject(&view(), &p, &[]);
		assert!((w[0] - (10.0 + (10.0 / 1.002 - 0.15) * 0.1)).abs() < 0.02, "right by 40 / 400 of 10 m: {w:?}");
		assert!((w[2] - (5.0 - (10.0 / 1.002 - 0.15) * 0.1)).abs() < 0.02, "below: {w:?}");
	}
}
