//! [PC] Effects follow the smoothed camera (round 3 issue 9). Frame smoothing (smooth.rs) draws the world between the two newest
//! fields, but the GS picture (effects and HUD) is the newest field's: on a fast camera turn the effects ran ahead of the world.
//! `matrix` gives the engine's overlay shader the rotation between the two cameras; it fetches each effect texel along it
//! (`ov_depth.rs`, `Gpu::set_retail_overlay_warp`). Rotation and translation: the shader reprojects each effect texel by its depth (the eye shift `T` is
//! the second part of the matrix's 20 floats), so a near effect (a ship's cockpit glass a few metres away) follows the parallax too.

use crate::scene::View;

type V3 = [f32; 3];

/// The 20 floats of `Gpu::set_retail_overlay_warp`: the three rows of M (`dn = M * ds`: a direction of the shown camera in
/// the view coordinates right, down, forward, to the same direction in the newest field's camera), each padded to four, then the
/// tangents across the picture (kx, ky, 0, 0), then the newest eye minus the shown eye in the shown view's axes (tx, ty, tz, 0). None when the two cameras turn the same (the overlay is drawn as it is).
pub fn matrix(newest: &View, shown: &View) -> Option<[f32; 20]> {
	let col = |v: &View, j: usize| -> V3 { [v.basis[0][j], v.basis[1][j], v.basis[2][j]] };
	let dot = |a: V3, b: V3| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
	let mut m = [[0.0f32; 3]; 3];
	for (i, row) in m.iter_mut().enumerate() {
		for (j, x) in row.iter_mut().enumerate() {
			*x = dot(col(newest, i), col(shown, j));
		}
	}
	let diff: f32 = (0..3).flat_map(|i| (0..3).map(move |j| (i, j))).map(|(i, j)| (m[i][j] - if i == j { 1.0 } else { 0.0 }).abs()).sum();
	let k = [2.0 * newest.half[0] / newest.fx, 2.0 * newest.half[1] / newest.fy];
	let d = [newest.eye[0] - shown.eye[0], newest.eye[1] - shown.eye[1], newest.eye[2] - shown.eye[2]];
	let t = [dot(col(shown, 0), d), dot(col(shown, 1), d), dot(col(shown, 2), d)];
	if (diff < 1e-5 && dot(t, t) < 1e-8) || !m.iter().flatten().chain(k.iter()).chain(t.iter()).all(|x| x.is_finite()) {
		return None;
	}
	Some([m[0][0], m[0][1], m[0][2], 0.0, m[1][0], m[1][1], m[1][2], 0.0, m[2][0], m[2][1], m[2][2], 0.0, k[0], k[1], 0.0, 0.0, t[0], t[1], t[2], 0.0])
}

#[cfg(test)]
mod tests {
	use super::*;

	fn view(yaw: f32) -> View {
		// View columns right, down, forward as world rows (world x, y, z): forward turns about the world z axis.
		let (s, c) = yaw.sin_cos();
		View { eye: [0.0; 3], basis: [[c, 0.0, -s], [s, 0.0, c], [0.0, 1.0, 0.0]], fx: 400.0, fy: 400.0, half: [256.0, 224.0] }
	}

	#[test]
	fn the_same_camera_needs_no_warp() {
		assert!(matrix(&view(0.3), &view(0.3)).is_none());
	}

	#[test]
	fn a_turn_maps_the_centre_to_the_turn_angle() {
		let (n, s) = (view(0.0), view(0.1));
		let m = matrix(&n, &s).unwrap();
		// The shown camera's forward (0, 0, 1) is, in the newest camera, `yaw` away from its own forward.
		let dn = [m[2], m[6], m[10]];
		assert!((dn[2] - 0.1f32.cos()).abs() < 1e-5, "{dn:?}");
		assert!((dn[0].abs() - 0.1f32.sin()).abs() < 1e-5, "{dn:?}");
		assert!((m[12] - 2.0 * 256.0 / 400.0).abs() < 1e-6 && (m[13] - 2.0 * 224.0 / 400.0).abs() < 1e-6);
	}

	#[test]
	fn a_moved_eye_gives_the_shift_in_the_shown_axes() {
		let (n, mut s) = (view(0.0), view(0.0));
		s.eye = [0.0, 0.0, -1.0];
		let m = matrix(&n, &s).unwrap();
		// view(0): right = world x, down = world z, forward = world y: the newest eye is 1 above the shown one along +z = 1 down.
		let t = [m[16], m[17], m[18]];
		assert!(t[0].abs() < 1e-6 && (t[1] - 1.0).abs() < 1e-6 && t[2].abs() < 1e-6, "{t:?}");
	}

	#[test]
	fn a_nan_view_gives_no_warp() {
		let mut v = view(0.2);
		v.basis[0][0] = f32::NAN;
		assert!(matrix(&view(0.0), &v).is_none());
	}
}
