//! When R&C2's retail camera takes over from the Modern one, and what the Modern camera does about it (scratch_rc2_cam, 2026-10-31;
//! the idea is R&C1's `modcam.rs` `why_retail` + `hand_back`: the pose blends from the last Modern pose to retail's over
//! `modcam_pose::BLEND` fields instead of snapping, and the Modern camera starts again from the pose on screen).
//!
//! Pure decision, no memory access: `modcam_hand.rs` collects the `Facts` of a field and passes `Verdict::retail_drives` on to
//! `PoseIn`. The hand only runs while the follow class (0) is the current camera [C handoff_stageB.md], so a camera of another class
//! is never seen here; what the hand can see is:
//! - RETAIL'S SWITCH BLEND (`SwitchBlend`): the u16 at `basis - 0x4630` (= the current-record cell + 0x100, Aranos 0x1b8f50) is set to
//!   1 (or 2) by the camera switch 0x2d8550 [C Aranos: `lh/sh 0x290(base 0x1b8cc0)` at 0x2d8674..0x2d8690], read by the camera main's
//!   frame driver 0x2da34c (`blend - 1 < 2` -> setup 0x2d8e98 from the previous record, `blend == 3` -> the apply 0x2d98a0 mixes the old
//!   pose into the output), and cleared at 0x2da854. R&C1's twin is `0x1672f0` (gate "camera switch blend 1..=3"). While it runs retail
//!   owns the pose: a live Modern camera blends back to retail's pose (R&C1 does the same), a dead one stays out. The counter runs only
//!   ~3 fields [I], far less than `modcam_pose::BLEND`, so the first field of it LATCHES the hand-back like `Rewritten` (review B3/4 R2:
//!   unlatched, `Cam::seed` restarted the Modern camera from a pose blended ~9% of the way, a short lurch with no hand-back).
//!   Survey [C 2026-10-31, review B3/4 R1]: the word is 0 in all 28 level RAM dumps (Aranos, level 1, levels 2..26 at field 2400;
//!   `scratch_rc2_bounce` `survey_marker.py` dump list and basis scan), so it is a real counter and not another field's value.
//! - THE RECORD WAS REWRITTEN (`Rewritten`): the follow record differs from what the hand wrote last field although the follow camera
//!   ran in between (another follow record became current, a script wrote it): a one-shot event, so it LATCHES the hand-back until
//!   `Cam::blending` says the blend is over (`settle`). While only the latch holds (`Finishing`) the counter or the rewrite is over but
//!   the blend to retail's pose still runs.
//! - THE FOLLOW CAMERA WAS AWAY (`Away`): it did not run for more than `modcam_hand::RECENT` fields and now the record was rewritten
//!   or a retail blend runs: a camera of another class (cutscene, fixed camera, free look) was current meanwhile, whatever Modern pose is
//!   left is stale. The camera ends; the next Modern field seeds from retail's pose (no blend from the stale one). A pause or a menu
//!   (follow still current, record untouched, no blend) is NOT away: the Modern camera resumes where it was.
//! - RETAIL PLACED ITS CAMERA OVER A FRESH SEED (`Placed`, lane ccam part 4): the record was rewritten right after the field whose
//!   only Modern write was the seed (`Cam::fresh`). Nothing Modern was on screen; the seed took a pose retail itself threw away. [C
//!   q60903 Clank lava respawn, field 208: the follow update gave eye 325.3,159.5 looking AWAY from Clank (forward -x), code after it
//!   placed the respawn camera 325.50,158.41 looking at him; latched as `Rewritten`, the hand blended from the thrown-away pose: 20
//!   fields of wall views (run u_new4 210-217).] The camera ends and seeds again from retail's pose at once, no blend.
//!
//! The retail camera's own blend (R&C2's output mix) starts from the pose left in the previous record, which is the last Modern pose
//! the hand wrote, so a switch to another camera class needs nothing from here; a retail CUT stays a cut.
//! Gated since b45 C2 (modcam_yield): hero camera modes 3 / 4 / 7, states 98 / 100 / 111, the grind group. Not found for R&C2 (not gated): game mode and the cinematic gate (R&C1 `why_retail` has them).

/// Retail's switch blend counter relative to the camera basis (`addrs::CAM_BASIS`): the current-record cell is `basis - 0x4730`,
/// the counter 0x100 above it [C Aranos 0x1b8f50, level 1 0x1b9650 by the +0x700 shift; 0 in all 28 dumps, see the header].
pub const BLEND_BELOW_BASIS: u32 = 0x4630;

/// Retail's switch blend runs (set 1 / 2 by the switch, 3 while the apply mixes).
pub fn switch_blend(v: u16) -> bool {
	(1..=3).contains(&v)
}

/// Why retail owns the pose this field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
	SwitchBlend(u16),
	Rewritten,
	/// Only the latch holds: the counter or the rewrite is over, the blend to retail's pose is still running.
	Finishing,
	Away,
	/// Retail rewrote the record over the Modern seed (`Cam::fresh`): seed again from its pose.
	Placed,
}

impl Why {
	pub fn text(self) -> String {
		match self {
			Why::SwitchBlend(v) => format!("camera switch blend {v}"),
			Why::Rewritten => "the follow record was rewritten".into(),
			Why::Finishing => "the blend to retail's pose is finishing".into(),
			Why::Away => "the follow camera was away (another camera class)".into(),
			Why::Placed => "retail placed its camera over the Modern seed (a respawn)".into(),
		}
	}
}

/// What one field of the follow update shows.
#[derive(Clone, Copy, Debug, Default)]
pub struct Facts {
	/// Retail's switch blend counter (`BLEND_BELOW_BASIS`).
	pub blend: u16,
	/// The record is not what the hand wrote last field.
	pub changed: bool,
	/// The follow camera did not run in the last `RECENT` fields.
	pub silent: bool,
	/// The Modern camera is live (it wrote a pose last field).
	pub live: bool,
	/// Its only write so far is the seed's (`Cam::fresh`).
	pub fresh: bool,
}

/// What to do this field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verdict {
	/// The Modern pose is stale: `Cam::end`, seed again from retail's pose.
	pub end: bool,
	/// Retail owns the pose: `PoseIn::retail_drives`.
	pub retail_drives: bool,
	pub why: Option<Why>,
}

/// The verdict for a field. `latch` is the one-shot hand-back that `Rewritten` and the first field of a `SwitchBlend` start; the caller
/// clears it with `settle`.
pub fn decide(f: Facts, latch: &mut bool) -> Verdict {
	let sw = switch_blend(f.blend);
	let away = f.silent && (f.changed || sw);
	let placed = !away && !sw && f.changed && f.fresh;
	if away || placed {
		*latch = false;
	} else if (f.changed || sw) && f.live {
		*latch = true;
	}
	let why = if away {
		Some(Why::Away)
	} else if placed {
		Some(Why::Placed)
	} else if sw {
		Some(Why::SwitchBlend(f.blend))
	} else if *latch {
		Some(if f.changed { Why::Rewritten } else { Why::Finishing })
	} else {
		None
	};
	// Away with no retail blend running: nothing to hand back (the pose is stale), the Modern camera seeds from retail's at once.
	Verdict { end: away || placed, retail_drives: sw || *latch, why }
}

/// After the field's `Cam::step`: the latched hand-back ends once the blend to retail's pose is over.
pub fn settle(latch: &mut bool, blending: bool) {
	if !blending {
		*latch = false;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::modcam_pose::{BLEND, Cam, Config, NoWalls, PIVOT, Pose, PoseIn, eye, len3, rows, sub};

	fn facts(blend: u16, changed: bool, silent: bool, live: bool) -> Facts {
		Facts { blend, changed, silent, live, fresh: false }
	}

	#[test]
	fn retails_switch_blend_is_one_to_three() {
		assert!(!switch_blend(0));
		assert!((1..=3).all(switch_blend));
		assert!(!switch_blend(4) && !switch_blend(0xffff));
	}

	#[test]
	fn a_quiet_field_leaves_the_modern_camera_alone() {
		let mut l = false;
		let v = decide(facts(0, false, false, true), &mut l);
		assert_eq!(v, Verdict { end: false, retail_drives: false, why: None });
		assert!(!l);
		// A pause or a menu: silent for a while but the record is untouched and no blend runs: resume, no end.
		let v = decide(facts(0, false, true, true), &mut l);
		assert_eq!((v.end, v.retail_drives, v.why), (false, false, None));
	}

	#[test]
	fn a_retail_switch_blend_hands_back_while_it_runs() {
		let mut l = false;
		for b in 1..=3 {
			let v = decide(facts(b, false, false, true), &mut l);
			assert_eq!((v.end, v.retail_drives, v.why), (false, true, Some(Why::SwitchBlend(b))));
		}
		assert!(l, "the first field of the counter latched the hand-back");
		// The counter is over but the blend to retail's pose still runs: retail keeps the pose until `Cam::blending` ends.
		let v = decide(facts(0, false, false, true), &mut l);
		assert_eq!((v.retail_drives, v.why), (true, Some(Why::Finishing)));
		settle(&mut l, true);
		assert!(l);
		settle(&mut l, false);
		let v = decide(facts(0, false, false, true), &mut l);
		assert!(!v.retail_drives, "the blend is over: Modern seeds from retail's pose");
	}

	/// The hand's loop in miniature (`modcam_hand::State::after`): `decide`, the camera step, `settle`.
	fn run_fields(cam: &mut Cam, latch: &mut bool, counter: &[u16], retail: Pose, live_from: u32) -> Vec<(bool, Option<Pose>)> {
		let mut out = Vec::new();
		for (k, &b) in counter.iter().enumerate() {
			let field = live_from + k as u32;
			let v = decide(facts(b, false, false, cam.live()), latch);
			let mut pin = PoseIn::idle(field, [0.0; 3], retail);
			pin.retail_drives = v.retail_drives;
			let p = cam.step(&pin, &mut NoWalls);
			settle(latch, cam.blending());
			out.push((v.retail_drives, p));
		}
		out
	}

	#[test]
	fn a_three_field_counter_still_hands_back_the_whole_blend() {
		// Review B3/4 R2: retail re-selects a camera with the same follow record and its pose is far from the Modern one. The counter
		// runs 1, 2, 3 and is gone; unlatched, the Modern camera restarted from the pose blended ~9% of the way.
		let mut cam = Cam::new(Config::new(0.02, 7.0));
		let mut l = false;
		let r0 = Pose { eye: eye([0.0, 0.0, PIVOT], 0.0, 0.35, 7.0), rows: rows(0.0, 0.35) };
		run_fields(&mut cam, &mut l, &[0, 0, 0], r0, 1);
		assert!(cam.live());
		let r1 = Pose { eye: eye([0.0, 0.0, PIVOT], 1.2, 0.2, 6.0), rows: rows(1.2, 0.2) };
		let mut counter = vec![1, 2, 3];
		counter.extend([0; 40]);
		let out = run_fields(&mut cam, &mut l, &counter, r1, 10);
		// Retail owns the pose for exactly BLEND fields (not 3) and the blend ends on retail's eye.
		let driven = out.iter().take_while(|(d, _)| *d).count();
		assert_eq!(driven as u32, BLEND, "the hand-back runs all its fields");
		let last = out[driven - 1].1.expect("the last blend field writes a pose");
		assert!(len3(sub(last.eye, r1.eye)) < 1e-3, "ends on retail's eye");
		assert!(!l && !cam.blending());
		// Then the Modern camera takes over from retail's pose (no lurch toward a half-blended one).
		let after = out[driven].1.expect("Modern drives again");
		assert!(len3(sub(after.eye, r1.eye)) < 0.4, "seeded from retail's pose: {:?}", after.eye);
	}

	#[test]
	fn a_rewritten_record_latches_the_hand_back_until_the_blend_ends() {
		let mut l = false;
		let v = decide(facts(0, true, false, true), &mut l);
		assert_eq!((v.end, v.retail_drives, v.why), (false, true, Some(Why::Rewritten)));
		// The next fields: the record is quiet again but the latch keeps the blend going.
		let v = decide(facts(0, false, false, true), &mut l);
		assert!(v.retail_drives && l);
		settle(&mut l, true);
		assert!(l, "still blending");
		settle(&mut l, false);
		assert!(!l);
		assert!(!decide(facts(0, false, false, true), &mut l).retail_drives);
	}

	#[test]
	fn a_rewritten_record_with_no_live_camera_needs_no_blend() {
		let mut l = false;
		let v = decide(facts(0, true, false, false), &mut l);
		assert!(!v.retail_drives && !l, "nothing Modern on screen to blend from");
	}

	#[test]
	fn a_record_placed_over_a_fresh_seed_seeds_again_at_once() {
		let mut l = false;
		let f = Facts { fresh: true, ..facts(0, true, false, true) };
		let v = decide(f, &mut l);
		assert_eq!((v.end, v.retail_drives, v.why), (true, false, Some(Why::Placed)));
		assert!(!l, "no blend from a pose that never reached the screen");
		// A retail switch blend over a fresh seed still hands back (the counter rules), as does a rewrite of an older Modern pose.
		assert!(decide(Facts { fresh: true, ..facts(1, true, false, true) }, &mut l).retail_drives);
		let mut l = false;
		assert_eq!(decide(facts(0, true, false, true), &mut l).why, Some(Why::Rewritten));
	}

	#[test]
	fn the_respawn_seed_follows_retails_placed_camera_not_the_thrown_away_pose() {
		// q60903 field 208: the follow update gives a pose looking away from Clank, code after it places the respawn camera looking at
		// him (rewrite). The hand's loop: field 1 seeds from the thrown-away pose, field 2 sees the rewrite of a fresh seed.
		let mut cam = Cam::new(Config::new(0.02, 7.0));
		let mut l = false;
		let away = Pose { eye: eye([0.0, 0.0, PIVOT], 0.36, 0.11, 3.2), rows: rows(3.12, 0.11) };
		let placed = Pose { eye: eye([0.0, 0.0, PIVOT], 0.0, 0.11, 2.9), rows: rows(0.0, 0.11) };
		let v = decide(Facts { fresh: cam.fresh(), ..facts(0, true, true, cam.live()) }, &mut l);
		assert!(v.end && !v.retail_drives);
		cam.end();
		assert!(cam.step(&PoseIn::idle(1, [0.0; 3], away), &mut NoWalls).is_some());
		assert!(cam.fresh(), "only the seed's pose was written");
		let v = decide(Facts { fresh: cam.fresh(), ..facts(0, true, false, cam.live()) }, &mut l);
		assert_eq!((v.end, v.retail_drives, v.why), (true, false, Some(Why::Placed)));
		cam.end();
		let p = cam.step(&PoseIn::idle(2, [0.0; 3], placed), &mut NoWalls).expect("Modern seeds from the placed camera");
		assert!(len3(sub(p.eye, placed.eye)) < 0.15, "the eye is the placed camera's (+ one boom release step): {:?}", p.eye);
		assert!(len3(sub(p.rows[0], placed.rows[0])) < 1e-3, "the view is the placed camera's (no turn from the thrown-away one)");
		assert!(cam.fresh());
		let p2 = cam.step(&PoseIn::idle(3, [0.0; 3], placed), &mut NoWalls).expect("drives");
		assert!(!cam.fresh(), "a Modern pose is on screen now: a rewrite hands back again");
		assert!(len3(sub(p2.eye, p.eye)) < 0.15, "no jump on the next field");
	}

	#[test]
	fn a_camera_that_was_away_drops_the_stale_pose() {
		let mut l = true;
		let v = decide(facts(0, true, true, true), &mut l);
		assert_eq!((v.end, v.retail_drives, v.why), (true, false, Some(Why::Away)));
		assert!(!l, "no blend from a stale pose");
		// Back with a retail blend running: the camera ends and retail drives until it is over.
		let v = decide(facts(1, false, true, true), &mut l);
		assert_eq!((v.end, v.retail_drives, v.why), (true, true, Some(Why::Away)));
		let v = decide(facts(3, false, false, false), &mut l);
		assert_eq!((v.end, v.retail_drives, v.why), (false, true, Some(Why::SwitchBlend(3))));
	}

	#[test]
	fn the_counter_sits_a_fixed_distance_under_the_basis() {
		// Aranos: basis 0x1bd580, record cell basis - 0x4730 = 0x1b8e50, counter 0x100 above it = 0x1b8f50.
		assert_eq!(0x1bd580 - BLEND_BELOW_BASIS, 0x1b8f50);
		assert_eq!(0x1bd580 - 0x4730 + 0x100, 0x1bd580 - BLEND_BELOW_BASIS);
	}
}
