//! The strafe-aim lift of the Modern camera (scratch_rc2_cam, 2026-10-31; port of R&C1's "Raised" aim, `rac1/retail/modcam/shoulder.rs`
//! `lift` + `next_t` + `lift_value` and the constants of `modcam.rs`; the user's favourite, so the numbers are R&C1's exactly).
//!
//! While the strafe is held (R&C2: the L2 or R2 bit of the pad as the game gets it, the window's RMB included: `Input::strafe`) the camera
//! orbits a pivot lifted `AIM_LIFT` above Ratchet's chest, so the screen centre (the reticle) lies above his head and he sits in the
//! lower middle of the frame. `t` runs 0..=`AIM_BLEND` fields while the aim is held and back to 0 after; the lift follows it on a
//! smoothstep and is limited by the ceiling over Ratchet (`cap`: `AIM_CEIL` stays free below the first thing overhead, and the cap
//! moves at most `AIM_CAP_STEP` a field, so a low roof does not jerk the view).
//!
//! R&C2's strafe, as far as it is known [C NOTES.md "Hero and strafe"]: L2 and R2 strafe (Ratchet turns to the camera heading first,
//! then the logic yaw stays put), L1 = first person; the hero state word is not found [?], so the pad bit is the only signal (it is what
//! retail itself reads). L2 and R2 both drive the lift (`Input::strafe`); only `Input::aim` (RMB / the feed) adds the R2 bit to the pad.
//! The caller (`Cam::step`) gives the ceiling from the game's own segment test; this file has no memory access.
//!
//! Hero-state gate [b46 W6]: done by modcam_yield (b45 C2): grind / rail / swing / script freeze / vendor lock hand the camera back to
//! retail (the raised strafe lift is part of the Modern pose, so it is gated with it).

/// The pivot lift at full aim (m above the chest).
pub const AIM_LIFT: f32 = 1.5;
/// Fields of the smooth in / out.
pub const AIM_BLEND: u32 = 12;
/// Room kept between the lifted pivot and a ceiling (m).
pub const AIM_CEIL: f32 = 0.4;
/// The most the ceiling cap moves in one field (m).
pub const AIM_CAP_STEP: f32 = 0.15;

/// The lift state of one camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lift {
	/// Fields into the blend (0..=`AIM_BLEND`).
	t: u32,
	/// The ceiling limit in force (m).
	cap: f32,
	/// The lift in force (m).
	lift: f32,
}

impl Lift {
	pub const ZERO: Lift = Lift { t: 0, cap: AIM_LIFT, lift: 0.0 };

	/// The state as three words (a quick save) and back.
	pub fn pack(&self) -> [u32; 3] {
		[self.t, self.cap.to_bits(), self.lift.to_bits()]
	}

	pub fn unpack(w: [u32; 3]) -> Lift {
		Lift { t: w[0].min(AIM_BLEND), cap: f32::from_bits(w[1]), lift: f32::from_bits(w[2]) }
	}

	/// The lift in force (m above the pivot).
	pub fn value(&self) -> f32 {
		self.lift
	}

	/// No aim, no lift (a hand-back, a seed, a new level).
	pub fn reset(&mut self) {
		*self = Lift::ZERO;
	}

	/// `t` after one field: up to the blend length while the aim is held, down to 0 after.
	fn next_t(&self, aiming: bool) -> u32 {
		if aiming { (self.t + 1).min(AIM_BLEND) } else { self.t.saturating_sub(1) }
	}

	/// Whether this field's `step` wants the ceiling test: the aim is held or the lift is still blending out.
	pub fn wants_ceiling(&self, aiming: bool) -> bool {
		self.next_t(aiming) > 0
	}

	/// One field. `ceiling` = the height (m, above the pivot) of the first thing overhead within `AIM_LIFT + AIM_CEIL` (the segment test
	/// from the pivot straight up), `None` = free. Returns the lift for this field.
	pub fn step(&mut self, aiming: bool, ceiling: Option<f32>) -> f32 {
		self.t = self.next_t(aiming);
		if self.t == 0 {
			self.lift = 0.0;
			return 0.0;
		}
		let goal = match ceiling {
			Some(h) => (h - AIM_CEIL).clamp(0.0, AIM_LIFT),
			None => AIM_LIFT,
		};
		self.cap += (goal - self.cap).clamp(-AIM_CAP_STEP, AIM_CAP_STEP);
		let t = (self.t as f32 / AIM_BLEND as f32).min(1.0);
		self.lift = (AIM_LIFT * t * t * (3.0 - 2.0 * t)).min(self.cap);
		self.lift
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_lift_rises_on_a_smoothstep_to_the_full_height_and_falls_back_to_nothing() {
		let mut l = Lift::ZERO;
		assert_eq!(l.step(false, None), 0.0, "no aim, no lift");
		let up: Vec<f32> = (0..AIM_BLEND).map(|_| l.step(true, None)).collect();
		assert!(up.windows(2).all(|w| w[1] > w[0]), "rises every field: {up:?}");
		assert!((up[AIM_BLEND as usize - 1] - AIM_LIFT).abs() < 1e-6, "full height after the blend");
		assert_eq!(l.step(true, None), AIM_LIFT, "held: stays");
		let down: Vec<f32> = (0..AIM_BLEND).map(|_| l.step(false, None)).collect();
		assert!(down.windows(2).all(|w| w[1] < w[0]), "falls every field: {down:?}");
		assert_eq!(l.step(false, None), 0.0);
		assert_eq!(l, Lift { cap: l.cap, ..Lift::ZERO });
	}

	#[test]
	fn a_low_ceiling_limits_the_lift_and_the_cap_moves_slowly() {
		let mut l = Lift::ZERO;
		// A roof 1.0 m over the pivot: 0.4 m stays free, so the lift ends at 0.6.
		for _ in 0..40 {
			l.step(true, Some(1.0));
		}
		assert!((l.value() - 0.6).abs() < 1e-5, "{}", l.value());
		// The roof goes away: the cap grows at most AIM_CAP_STEP a field.
		let before = l.value();
		let after = l.step(true, None);
		assert!(after - before <= AIM_CAP_STEP + 1e-6 && after > before);
		// A roof lower than AIM_CEIL: no lift at all, never negative.
		let mut m = Lift::ZERO;
		for _ in 0..40 {
			m.step(true, Some(0.1));
		}
		assert_eq!(m.value(), 0.0);
	}

	#[test]
	fn the_ceiling_test_runs_only_while_the_lift_exists() {
		let mut l = Lift::ZERO;
		assert!(!l.wants_ceiling(false) && l.wants_ceiling(true));
		l.step(true, None);
		l.step(true, None);
		assert!(l.wants_ceiling(false), "blending out still needs the roof");
		l.reset();
		assert_eq!(l, Lift::ZERO);
		assert!(!l.wants_ceiling(false));
	}
}
