//! Hero states in which retail's own camera drives, not the Modern one (scratch_rc2_modv 2026-11-11; the hand-back gates of R&C1's
//! `rac1/retail/modcam.rs` `why_retail` ported in build 45 P5, sweep C2).
//!
//! R&C1 hands the picture back to retail on: the hero camera mode byte 0x1415d4 in {3 slide/grind rail, 4 look, 7 swing}
//! (`GATE_CAM`), the hero states of a script freeze / grind / swing / slide (`GATE_STATES`) and the death states. R&C2's twins [C,
//! 2026-11-12, `b45\p5\setter.py` + `camby.py` over the 581 fg1 states; table in rac2/notes_gamefacts.md "Hero camera mode"]:
//! - the hero CAMERA MODE word 0x18c340 (hero base 0x189ea0 + 0x24a0; R&C1's byte, same values): R&C2's SetState (level-4 table
//!   0x2c7820) stores 4 for state 1 (first-person look), 7 for the swingshot states 35..37 (group 13), 43 (group 14) and 44, 0xd ledge,
//!   0xb under water, 0x51 weapon ready, 0x50 airborne; 3 is stored outside the table (the rail grind, fg1 0182: state 39 group 15).
//!   Gated: 3, 4, 7 (R&C1's `GATE_CAM`).
//! - the SCRIPT FREEZE state 111 (0x6f, group 9, camera mode 0): R&C1's 0x72 twin by the setter table (same group 9 and mode 0; the
//!   table runs 3 below R&C1 there, as 0x6a -> 0x67 and 0x73 -> 0x70) [C table, I name]; fg1 0059 / 0129: a teleport white-out, the
//!   hacker puzzle.
//! - the GRIND group 15 (fg1 0182 state 39; also state 62 = 0x3e by the setter table, camera mode 0 at its entry) [I: group 15 =
//!   grinding from the shots]: R&C1 gates its grind states by number too (0x29 0x2a 0x74).
//! - the vendor lock (100) and the pad interaction (98): retail scripts read the follow record's heading there (below).
//! Not gated, as R&C1: swimming (0xb; R&C1's Modern camera swims), ledges (0xd), the air (0x50), weapon ready (0x51), walking. The
//! death states are `modcam_death`'s. Game mode / cinematic gate: R&C2's hand runs only while the follow class is current, so a
//! cutscene or menu camera is never seen here (`modcam_blend` "Away").
//!
//! The Platinum weapon-mod vendor (level 3) opens its menu only after a retail script has read the follow record's forward heading
//! (row 0 xy, tolerance < 0.5 degrees) at ~field 1 and found retail's own heading; a Modern pose written there (even a re-seeded one)
//! leaves the script waiting for ever, the menu never opens [C 2026-11-11, 100-field runs of the user's vendor state].
//! The hero is locked in an interaction state meanwhile: state word 0x18c134 = 100 (group 0x18c13c = 0) at the vendor, 98 (group 1)
//! on the Gadgetron / teleport pads [C fg1 states 0198, 0318, user vendor state; the other 580 bank states never have 100].
//! A live Modern camera blends back to retail through `PoseIn::retail_drives` (R&C1's BLEND fields); a camera that is not live writes
//! nothing at all; once the gate ends the Modern camera seeds from retail's pose (no jump either way). The raised strafe aim is part
//! of the Modern pose, so it is gated with it.

/// Hero state word (EE RAM, same address in every level [C notes_gamefacts.md]).
pub const HERO_STATE: u32 = 0x18c134;
/// Hero group word [C notes_gamefacts.md].
pub const HERO_GROUP: u32 = 0x18c13c;
/// Hero camera mode word (R&C1 0x1415d4 twin) [C setter table stores `sw 0x24a0(base 0x189ea0)`].
pub const HERO_CAM: u32 = 0x18c340;

/// The script freeze state (R&C1 0x72 twin).
pub const FREEZE: i32 = 111;
/// The rail-grind group.
pub const GRIND_GROUP: i32 = 15;
/// Hero camera modes that keep retail's camera: rail 3, look 4, swing 7 (R&C1 `GATE_CAM`).
pub const GATE_CAM: [i32; 3] = [3, 4, 7];

/// Why retail drives the camera this field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gate {
	/// The vendor lock (100) or the pad interaction (98).
	Interaction(i32),
	/// Script freeze (111).
	Freeze,
	/// The hero camera mode word (3 rail, 4 look, 7 swing).
	CamMode(i32),
	/// The grind group.
	Grind,
}

impl Gate {
	pub fn text(self) -> String {
		match self {
			Gate::Interaction(s) => format!("hero interaction state {s}"),
			Gate::Freeze => "hero script freeze (state 111)".into(),
			Gate::CamMode(m) => format!("hero camera mode {m}"),
			Gate::Grind => "hero grind group".into(),
		}
	}
}

/// Hero states in which retail drives the camera: the vendor lock (100) and the pad interaction (98).
pub fn retail_scripted(state: i32) -> bool {
	matches!(state, 98 | 100)
}

/// The gate for the hero's (state, group, camera mode), the first that holds (R&C1 `why_retail` order: camera mode, then state).
pub fn gate_of(state: i32, group: i32, cam: i32) -> Option<Gate> {
	if retail_scripted(state) {
		Some(Gate::Interaction(state))
	} else if GATE_CAM.contains(&cam) {
		Some(Gate::CamMode(cam))
	} else if state == FREEZE {
		Some(Gate::Freeze)
	} else if group == GRIND_GROUP {
		Some(Gate::Grind)
	} else {
		None
	}
}

fn rd(ram: &[u8], a: u32) -> i32 {
	ram.get(a as usize..a as usize + 4).map_or(0, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The gate that holds in this RAM image (EE RAM, offset = address), None = the Modern camera may drive.
pub fn hero_gate(ram: &[u8]) -> Option<Gate> {
	gate_of(rd(ram, HERO_STATE), rd(ram, HERO_GROUP), rd(ram, HERO_CAM))
}

/// Retail drives the camera in this field (for other packages: e.g. the cursor rule may read it).
pub fn hero_gated(ram: &[u8]) -> bool {
	hero_gate(ram).is_some()
}

/// What the hand does in this field: `(retail_drives, write_nothing)`. A camera that is not live yet has nothing to blend from,
/// so it stays out completely (retail's record words stay byte-exact for the script).
pub fn verdict(gated: bool, live: bool) -> (bool, bool) {
	(gated && live, gated && !live)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_vendor_and_pad_locks_belong_to_retail() {
		assert!(retail_scripted(100) && retail_scripted(98));
		assert_eq!(gate_of(100, 0, 0), Some(Gate::Interaction(100)));
		assert_eq!(gate_of(98, 1, 0), Some(Gate::Interaction(98)));
	}

	#[test]
	fn rail_look_swing_freeze_and_grind_belong_to_retail() {
		assert_eq!(gate_of(39, 15, 3), Some(Gate::CamMode(3)));
		assert_eq!(gate_of(1, 0, 4), Some(Gate::CamMode(4)));
		assert_eq!(gate_of(36, 13, 7), Some(Gate::CamMode(7)));
		assert_eq!(gate_of(43, 14, 7), Some(Gate::CamMode(7)));
		assert_eq!(gate_of(111, 9, 0), Some(Gate::Freeze));
		assert_eq!(gate_of(62, 15, 0), Some(Gate::Grind));
	}

	#[test]
	fn ordinary_hero_states_stay_modern() {
		// (state, group, camera mode) pairs of the bank: idle, walk, fall, jump, heli glide, ledge hang, weapon ready, swim, ship.
		for (s, g, c) in [(0, 0, 0), (2, 1, 0), (6, 2, 0), (9, 4, 0x50), (15, 4, 0x50), (25, 3, 0xd), (29, 0, 0x51), (52, 17, 0xb), (49, 9, 0), (97, 24, 0)] {
			assert_eq!(gate_of(s, g, c), None, "state {s} group {g} cam {c}");
		}
		assert_eq!(verdict(false, true), (false, false));
		assert_eq!(verdict(false, false), (false, false));
	}

	#[test]
	fn a_live_camera_blends_back_and_a_dead_one_writes_nothing() {
		assert_eq!(verdict(true, true), (true, false));
		assert_eq!(verdict(true, false), (false, true));
	}

	/// The gate against real RAM images of the state bank (skipped when the bank is not on this PC).
	#[test]
	fn the_gate_on_the_state_bank() {
		let cases: [(&str, bool); 10] = [
			("0182", true),  // rail grind (39, 15, cam 3)
			("0183", true),  // swingshot (36, 13, cam 7)
			("0217", true),  // hanging from the swingshot target (43, 14, cam 7)
			("0368", true),  // first-person look (1, 0, cam 4)
			("0059", true),  // script freeze, teleport white-out (111, 9)
			("0198", true),  // pad interaction (98)
			("0002", false), // walking
			("0221", false), // swimming under water
			("0054", false), // ledge hang
			("0294", false), // heli-pack glide
		];
		for (st, want) in cases {
			let Ok(ram) = std::fs::read(format!("E:/ratchet/rac2/states/fg1/{st}/eeMemory.bin")) else {
				eprintln!("state bank not present: skipped");
				return;
			};
			assert_eq!(hero_gated(&ram), want, "fg1 {st}: {:?}", hero_gate(&ram));
		}
	}
}
