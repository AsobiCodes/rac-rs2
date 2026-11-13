use super::*;

#[test]
fn modern_jump_grid_preset_takes_a_neighbour() {
	// Weapons 4 x 6: the left neighbour + Right, column 0 = the right one + Left; the cursor itself wins (no write).
	assert_eq!(grid_preset(4, 24, Some(0), 6, |_| true), Some((5, RIGHT)));
	assert_eq!(grid_preset(4, 24, Some(0), 8, |_| true), Some((9, LEFT)));
	assert_eq!(grid_preset(4, 24, Some(7), 6, |_| true), Some((7, LEFT)));
	// Gadgets' Hand group 3 x 2 with a hole at 3: cell 4 is reached from 5 with Left.
	let hand = [13u32, 36, 39, 0, 46, 55];
	assert_eq!(grid_preset(3, 6, Some(0), 4, |k| hand[k] != 0), Some((5, LEFT)));
	assert_eq!(grid_preset(1, 1, Some(0), 0, |_| true), None);
}

#[test]
fn modern_jump_list_and_items_presets() {
	assert_eq!(list_preset(7, 0, 5), Some((4, DOWN)));
	assert_eq!(list_preset(7, 0, 0), Some((1, UP)));
	assert_eq!(list_preset(7, 6, 5), Some((6, UP)));
	// Items: every cell has a one-press neighbour under retail's wrapping, and that press lands on it.
	use crate::modern_items::{step, COLS, GROUP_LEN};
	for g in 0..4 {
		for k in 0..GROUP_LEN[g] {
			let ((pg, pk), d) = items_preset((9, 9), (g, k)).expect("a neighbour");
			let pos = |g: usize, k: usize| (g, k % COLS[g], k / COLS[g]);
			assert_eq!(step(pos(pg, pk), d), pos(g, k));
		}
	}
}

#[test]
fn modern_jump_writes_only_when_queued() {
	let mut ram = vec![0u8; 0x100];
	apply(&mut ram);
	assert!(ram.iter().all(|&b| b == 0));
	queue(0x40, 0x0102_0304);
	apply(&mut ram);
	assert_eq!(&ram[0x40..0x44], &[4, 3, 2, 1]);
}
