//! The death gate (lane ccam part 3, 2026-11-12): R&C1's Modern camera hands the camera back to retail in the hero's death states
//! (rac1/retail/modcam.rs `GATE_STATES` via `why_retail`, death_respawn.md section 2: 0x3d, 0x47, 0x77 pit tumble, 0x7a-0x7d, 0x7f,
//! 0x80, 0x82) and seeds again from retail's pose after the respawn. This is the R&C2 twin: without it the Modern camera followed Clank
//! as he sank into the Aranos lava (user quick save q60903, run u_new3 fields 59-109) while retail holds its camera.
//!
//! R&C2 facts [C, level-14 RAM of q60903 (lane run d_u) and its hero state setter, jump table 0x2a76b0 indexed by state, each case
//! storing the group at 0x189ea0+0x229c = 0x18c13c; level 4 has the same table at 0x2c7820, rac2/notes_gamefacts.md]:
//! - the Aranos lava death: state 0x18c134 65 (Clank's fall, group 2) -> 122 (0x7a, group 20, health 0x18c36c 4 -> 0) at field ~64,
//!   held until the black fade; the respawn puts state 63 (group 0), health 4, at field ~209.
//! - group 20 (0x14, R&C1's death group too: 0x3d / 0x7c) is stored by exactly the states 0x39, 0x3d, 0x43, 0x67, 0x74, 0x79, 0x7a,
//!   0x7c, 0x7d and 0x7f; 0x39 0x3d 0x43 0x79 0x7a 0x7c 0x7d zero the health, 0x67 is drowning (notes_gamefacts.md), 0x74 (the case it
//!   shares with the fall state 6 stores group 20 in a branch delay slot, so always for 0x74) is the pit fall, R&C1 0x77's slot.
//! - group 25 (R&C1 0x7b's 0x19, a death state without health loss, death_respawn.md section 2) is stored by exactly the states 0x65
//!   and 0x78 [C, same table scan, part 5]. 0x65 is the swamp sink [C, lane run o24: fg1 0024 (level 1 Oozla), Ratchet walked off the
//!   lily pad into the swamp: state 101 group 25 from field ~58, z 107.99 -> 106.90 at the same x,y, health 5 kept, respawn at the
//!   checkpoint (state 0) at field ~225; classic (run o_cls) holds its camera meanwhile, b43/part-4 Modern dove into the swamp after him].
//!   0x66 (the jump out of the swamp with Cross, run o_esc) keeps group 25: retail's camera also holds through an escape, and the
//!   Modern camera seeds again when he lands (state 0). 0x78 = R&C1 0x7b's slot minus 3 as the other shifted states [I by twin].

/// Hero state, its group and health (EE RAM, the same in every level) [C notes_gamefacts.md].
pub const HERO_STATE: u32 = 0x18c134;
pub const HERO_GROUP: u32 = 0x18c13c;
/// The groups the death states store: 20 (health deaths, drowning, pit falls) and 25 (the swamp sink 0x65, 0x78).
pub const DEATH_GROUPS: [i32; 2] = [20, 25];

/// The hero dies (or drowns, sinks, or falls into a pit) in this state / group: retail's camera holds, the Modern camera hands it back.
/// Every death state stores one of `DEATH_GROUPS`, so the group decides (`_state` is kept for the log / callers).
pub fn dying(_state: i32, group: i32) -> bool {
	DEATH_GROUPS.contains(&group)
}

/// `dying` read from EE RAM (None when the words are out of range).
pub fn dying_in(ram: &[u8]) -> bool {
	let rd = |a: u32| ram.get(a as usize..a as usize + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]));
	matches!((rd(HERO_STATE), rd(HERO_GROUP)), (Some(s), Some(g)) if dying(s, g))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_lava_death_and_the_other_death_states_hand_back() {
		assert!(dying(122, 20), "q60903: Clank sinks in the lava");
		for s in [0x39, 0x3d, 0x43, 0x67, 0x74, 0x79, 0x7a, 0x7c, 0x7d, 0x7f] {
			assert!(dying(s, 20), "state {s:#x} stores group 20");
		}
		assert!(dying(0x78, 25));
		assert!(dying(0x65, 25), "fg1 0024 run o24: Ratchet sinks in the Oozla swamp (state 101, group 25)");
	}

	#[test]
	fn walking_falling_and_the_respawn_stay_modern() {
		// q60903: Clank's walk (65, group 2) before the lava, the respawn (63, group 0); notes_gamefacts.md pairs; o24: Ratchet's walk
		// (2, 1) before the swamp. Swimming (group 17 / 18) and the fall state 6 in its fall group 2 are no death.
		for (s, g) in [(65, 2), (63, 0), (0, 0), (2, 1), (6, 2), (9, 4), (25, 3), (29, 0), (52, 17), (0x35, 18), (8, 5), (0x7b, 28)] {
			assert!(!dying(s, g), "({s}, {g})");
		}
	}

	#[test]
	fn the_words_are_read_from_ram() {
		let mut ram = vec![0u8; 0x18c200];
		ram[HERO_STATE as usize..HERO_STATE as usize + 4].copy_from_slice(&122i32.to_le_bytes());
		ram[HERO_GROUP as usize..HERO_GROUP as usize + 4].copy_from_slice(&20i32.to_le_bytes());
		assert!(dying_in(&ram));
		ram[HERO_GROUP as usize..HERO_GROUP as usize + 4].copy_from_slice(&2i32.to_le_bytes());
		ram[HERO_STATE as usize..HERO_STATE as usize + 4].copy_from_slice(&65i32.to_le_bytes());
		assert!(!dying_in(&ram));
		assert!(!dying_in(&[0u8; 16]), "short RAM: no gate");
	}
}
