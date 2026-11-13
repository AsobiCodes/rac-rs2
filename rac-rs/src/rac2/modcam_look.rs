//! The take-over without a cut (lane ccam part 3, 2026-11-12): R&C1's `LookOff` (rac1/retail/modcam/classic.rs) for R&C2's Modern camera.
//!
//! Every seed (`modcam_pose::Cam::seed`) takes yaw, pitch and boom from retail's EYE, so the eye does not jump; but retail's camera does not
//! always look at the Modern pivot (the hero + 0.9 m). After the Aranos respawn it looks 0.23 rad higher [C q60903 RAM field 228, record
//! 0x1b98d0: retail's forward pitch 0.172, the eye -> pivot pitch 0.402], so the first Modern rows (looking at the pivot) tilted the whole
//! view in one field: the take-over cut (lane ccam part 2, run u_new3 229 -> 230; the boom itself stayed at retail's 2.87 m). As in R&C1,
//! the offset between retail's view and the orbit's is taken on the first pose written (that pose then shows retail's view exactly) and
//! fades out by a smoothstep over `LOOK_FADE` fields. Pure: `Cam` feeds it, nothing here touches the game.

use crate::modcam_pose::{Rows, wrap};

/// Fields the take-over's view offset takes to fade out (R&C1 `LOOK_FADE`).
pub const LOOK_FADE: u32 = 12;

/// Retail's view at a seed versus the orbit's, fading out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LookOff {
	/// Retail's (yaw, pitch) at the seed, waiting for the first pose written.
	ret: Option<(f32, f32)>,
	/// The offset still to fade (yaw, pitch) and the fields faded so far.
	off: (f32, f32),
	n: u32,
}

impl LookOff {
	pub const NONE: LookOff = LookOff { ret: None, off: (0.0, 0.0), n: LOOK_FADE };

	/// A seed from the view on screen (`ret` = its yaw, pitch); None (a rolled or unknown view) = no offset.
	pub fn seed(&mut self, ret: Option<(f32, f32)>) {
		*self = match ret {
			Some(r) if r.0.is_finite() && r.1.is_finite() => LookOff { ret: Some(r), off: (0.0, 0.0), n: 0 },
			_ => LookOff::NONE,
		};
	}

	/// Back to no offset (a hand-back, a reset, a seed from a saved camera).
	pub fn clear(&mut self) {
		*self = LookOff::NONE;
	}

	/// The (yaw, pitch) the rows get for the orbit pose (`yaw`, `pitch`) of this field: the pose itself once the offset is gone.
	pub fn apply(&mut self, yaw: f32, pitch: f32) -> (f32, f32) {
		if let Some((ry, rp)) = self.ret.take() {
			self.off = (wrap(ry - yaw), rp - pitch);
			self.n = 0;
		}
		if self.n >= LOOK_FADE {
			return (yaw, pitch);
		}
		let t = self.n as f32 / LOOK_FADE as f32;
		let w = 1.0 - t * t * (3.0 - 2.0 * t);
		self.n += 1;
		(yaw + self.off.0 * w, pitch + self.off.1 * w)
	}
}

/// Retail's view angles (`modcam_pose::angles`) when its rows are not rolled (row 1, the left, level: the angles say it all); None else.
pub fn view_of(rows: &Rows) -> Option<(f32, f32)> {
	(rows[1][2].abs() < 1e-4).then(|| crate::modcam_pose::angles(rows))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::modcam_pose::rows;

	#[test]
	fn the_first_view_is_retails_and_it_fades_to_the_orbit() {
		let mut l = LookOff::NONE;
		assert_eq!(l.apply(1.0, 0.2), (1.0, 0.2), "no seed: the pose as it is");
		l.seed(Some((1.0, 0.17)));
		let (y0, p0) = l.apply(1.0, 0.40);
		assert!((y0 - 1.0).abs() < 1e-6 && (p0 - 0.17).abs() < 1e-6, "the first pose written is retail's view: no cut");
		let mut last = p0;
		for _ in 1..LOOK_FADE {
			let (_, p) = l.apply(1.0, 0.40);
			assert!(p >= last - 1e-6 && p <= 0.40 + 1e-6, "monotone towards the orbit's pitch");
			assert!(p - last < 0.06, "no step over 0.06 rad: {}", p - last);
			last = p;
		}
		assert_eq!(l.apply(1.0, 0.40), (1.0, 0.40), "faded out");
		l.seed(Some((2.0, 0.0)));
		l.clear();
		assert_eq!(l.apply(1.0, 0.40), (1.0, 0.40), "cleared");
	}

	#[test]
	fn the_yaw_offset_takes_the_short_way_round() {
		let mut l = LookOff::NONE;
		l.seed(Some((3.1, 0.3)));
		let (y, _) = l.apply(-3.1, 0.3);
		assert!((wrap(y - 3.1)).abs() < 1e-5, "first view = retail's yaw: {y}");
		let (y1, _) = l.apply(-3.1, 0.3);
		assert!(wrap(y1 - y).abs() < 0.06, "a small step, not a turn the long way: {}", wrap(y1 - y));
	}

	#[test]
	fn a_rolled_or_broken_view_keeps_no_offset() {
		let mut l = LookOff::NONE;
		let mut r = rows(0.5, 0.2);
		assert!(view_of(&r).is_some_and(|(y, p)| (y - 0.5).abs() < 1e-5 && (p - 0.2).abs() < 1e-5));
		r[1][2] = 0.3;
		assert!(view_of(&r).is_none());
		l.seed(view_of(&r));
		assert_eq!(l.apply(0.5, 0.24), (0.5, 0.24));
		l.seed(Some((f32::NAN, 0.1)));
		assert_eq!(l.apply(0.5, 0.24), (0.5, 0.24));
	}
}
