//! RAC2_FXCAM=1: which game camera the effect program's records were built with (round 3 batch 7 review S5). The records are
//! camera-relative points; `ov_layers::place` adds the eye of `Views::of_picture(field)` (= field - 1). For each lag d = -2..1 this logs,
//! per field, over the field's native effect triangles:
//! - `err`: the mean distance (GS pixels) between the GS vertex and the corner re-projected with the basis / focal length of the camera of
//!   field + d (rotation and field of view: the right camera gives ~0);
//! - `jit`: the mean distance (m) of the world point (eye of field + d + record point) to the nearest point of the same texture in the
//!   previous field's set, when it is under 2 m (a static emitter gives ~0 with the right eye; a moving one adds the same to every d).
//!
//! Read it with `grep fxcam`; the d with the least `err` and `jit` over a camera turn / move is the lag.
use super::ov_layers::Views;
use crate::scene::View;
use rac_engine::gsfull::layer_tris::LayerTri;
use std::sync::Mutex;

const LAGS: [i64; 4] = [-2, -1, 0, 1];
/// Triangles per field the log looks at.
const MAX: usize = 96;

fn on() -> bool {
	static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
	*ON.get_or_init(|| std::env::var_os("RAC2_FXCAM").is_some_and(|v| v != "0"))
}

/// The world points of the previous field's effect triangles per lag, as (texture id, point).
static PREV: Mutex<Vec<Vec<(u64, [f32; 3])>>> = Mutex::new(Vec::new());

/// Pixel position of a camera-relative point under `v`'s rotation and focal lengths (None: behind the eye).
fn project(v: &View, rel: [f32; 3], off: [f32; 2]) -> Option<[f32; 2]> {
	let col = |j: usize| (0..3).map(|k| rel[k] * v.basis[k][j]).sum::<f32>();
	let depth = col(2);
	(depth > 0.3).then(|| [v.half[0] + v.fx * col(0) / depth + off[0], v.half[1] + v.fy * col(1) / depth + off[1]])
}

pub(super) fn log(field: u64, tris: &[LayerTri], views: &Views) {
	if !on() {
		return;
	}
	let fx: Vec<&LayerTri> = tris.iter().filter(|t| t.world.is_some()).take(MAX).collect();
	let mut prev = PREV.lock().unwrap();
	prev.resize(LAGS.len(), Vec::new());
	let mut line = format!("fxcam f{field} n{}", fx.len());
	for (li, d) in LAGS.iter().enumerate() {
		let Some(v) = views.get((field as i64 + d) as u64) else {
			line += &format!(" d{d} none");
			prev[li].clear();
			continue;
		};
		let (mut err, mut ne, mut jit, mut nj) = (0.0f32, 0u32, 0.0f32, 0u32);
		let mut now = Vec::with_capacity(fx.len());
		for t in &fx {
			let w = t.world.as_ref().unwrap();
			if let Some(p) = project(&v, w[0].p, w[0].off) {
				err += ((p[0] - t.v[0].px).powi(2) + (p[1] - t.v[0].py).powi(2)).sqrt();
				ne += 1;
			}
			let pt: [f32; 3] = std::array::from_fn(|k| v.eye[k] + w[0].p[k]);
			let nearest = prev[li].iter().filter(|e| e.0 == t.id).map(|e| (0..3).map(|k| (e.1[k] - pt[k]).powi(2)).sum::<f32>().sqrt()).fold(f32::MAX, f32::min);
			if nearest < 2.0 {
				jit += nearest;
				nj += 1;
			}
			now.push((t.id, pt));
		}
		prev[li] = now;
		line += &format!(" d{d} err {:.2} jit {:.4} ({nj})", err / ne.max(1) as f32, jit / nj.max(1) as f32);
	}
	if let (Some(a), Some(b)) = (views.get(field), views.get(field.wrapping_sub(1))) {
		let m: f32 = (0..3).map(|k| (a.eye[k] - b.eye[k]).powi(2)).sum::<f32>().sqrt();
		line += &format!(" cam_move {m:.3}");
	}
	eprintln!("{line}");
}
