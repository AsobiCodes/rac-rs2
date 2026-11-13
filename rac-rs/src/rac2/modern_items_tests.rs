use super::*;

#[test]
fn gate_needs_page_six_under_the_pause_root() {
	assert!(gate(6, 2, 0x1fb8000));
	assert!(!gate(6, 2, 0));
	assert!(!gate(6, 3, 0x1fb8000));
	assert!(!gate(4, 2, 0x1fb8000));
}

#[test]
fn step_follows_the_retail_cursor_runs() {
	// Observed on level 9 (runs/o_nav): from Items (0,1) Down goes to Head, then Items (0,0), then (0,1).
	assert_eq!(step((2, 0, 1), DOWN), (0, 0, 0));
	assert_eq!(step((0, 0, 0), DOWN), (2, 0, 0));
	assert_eq!(step((2, 0, 0), DOWN), (2, 0, 1));
	// Up the other way round.
	assert_eq!(step((2, 0, 0), UP), (0, 0, 0));
	assert_eq!(step((0, 0, 0), UP), (2, 0, 1));
	// Right from the last Items column enters the Collectibles; Right there wraps; Left from column 0 leaves to Items (3, 1).
	assert_eq!(step((2, 3, 0), RIGHT), (3, 0, 0));
	assert_eq!(step((3, 0, 0), RIGHT), (3, 1, 0));
	assert_eq!(step((3, 1, 0), RIGHT), (3, 0, 0));
	assert_eq!(step((3, 0, 0), LEFT), (2, 3, 1));
	assert_eq!(step((2, 0, 1), LEFT), (2, 3, 1));
	assert_eq!(step((3, 1, 0), DOWN), (3, 1, 1));
	assert_eq!(step((3, 1, 1), DOWN), (3, 1, 0));
}

#[test]
fn every_cell_reaches_every_cell() {
	let cells: Vec<(usize, usize)> = (0..4).flat_map(|g| (0..GROUP_LEN[g]).map(move |k| (g, k))).collect();
	for &a in &cells {
		for &b in &cells {
			assert_eq!(a == b, press_toward(a, b).is_none(), "{a:?} -> {b:?}");
		}
	}
}

#[test]
fn walking_the_first_presses_lands_on_the_target() {
	let cells: Vec<(usize, usize)> = (0..4).flat_map(|g| (0..GROUP_LEN[g]).map(move |k| (g, k))).collect();
	for &a in &cells {
		for &b in &cells {
			let mut cur = a;
			for _ in 0..20 {
				let Some(p) = press_toward(cur, b) else { break };
				let (g, c, r) = step(pos(cur.0, cur.1), p);
				cur = (g, r * COLS[g] + c);
			}
			assert_eq!(cur, b, "{a:?} -> {b:?}");
		}
	}
}

#[test]
fn walk_pulses_two_on_two_off_then_crosses_on_arrival() {
	let mut w = Walk { target: Some((2, 1)), confirm: true, ..Walk::default() };
	// From Items (0,0): one Down = Items (0,1) is cell 4 (not 1), so the walk goes Right first.
	let sel = (2, 0);
	let first: Vec<u16> = (0..4).map(|_| w.step(sel)).collect();
	assert_eq!(first, vec![RIGHT, RIGHT, 0, 0]);
	let mut w = Walk { target: Some((2, 1)), confirm: true, ..Walk::default() };
	let out: Vec<u16> = (0..4).map(|_| w.step((2, 1))).collect();
	assert_eq!(out, vec![BIT[CROSS], BIT[CROSS], 0, 0]);
}

#[test]
fn walk_gives_up_after_the_press_limit_and_back_is_triangle() {
	let mut w = Walk { target: Some((3, 3)), ..Walk::default() };
	let mut presses = 0;
	for _ in 0..200 {
		if w.step((0, 0)) != 0 {
			presses += 1;
		}
	}
	assert_eq!(presses, 2 * MAX_PRESSES as usize);
	assert!(!w.busy());
	let mut w = Walk { back: true, ..Walk::default() };
	assert_eq!(w.step((0, 0)), BIT[TRIANGLE]);
}

#[test]
fn descriptions_read_in_full_and_line_breaks_become_spaces() {
	// A one-record text table: {ptr, id, 0, 0}.
	let mut ram = vec![0u8; 0x10_0000 + 0x2000];
	ram[0x10_0000..0x10_0004].copy_from_slice(&0x10_1000u32.to_le_bytes());
	ram[0x10_0004..0x10_0008].copy_from_slice(&0xfd3u32.to_le_bytes());
	let s = b"This item allows you\x01to repair broken pieces of machinery. ";
	let text: Vec<u8> = s.iter().copied().cycle().take(300).collect();
	ram[0x10_1000..0x10_1000 + 300].copy_from_slice(&text);
	let out = text_in(&ram, 0x10_0000, 1, 0xfd3);
	assert!(out.starts_with("This item allows you to repair"), "{out}");
	assert!(out.len() > 120);
}

#[test]
fn retail_wrapping_of_the_items_page_from_the_draw_code() {
	// FUN_003460a8 / FUN_00346210 / FUN_00346348: column 2 wraps in Items, column 3 leaves to Armor (up) or Collectibles (down).
	assert_eq!(step((2, 2, 1), DOWN), (2, 2, 0));
	assert_eq!(step((2, 2, 0), UP), (2, 2, 1));
	assert_eq!(step((2, 3, 1), DOWN), (3, 0, 0));
	assert_eq!(step((2, 3, 0), UP), (1, 0, 0));
	assert_eq!(step((2, 1, 1), DOWN), (0, 1, 0));
	assert_eq!(step((3, 1, 0), UP), (3, 1, 1));
	assert_eq!(step((3, 0, 0), UP), (2, 3, 1));
	assert_eq!(step((2, 3, 1), RIGHT), (3, 0, 0));
}

#[test]
fn platinum_bolt_counters_follow_the_retail_helpers() {
	let mut ram = vec![0u8; 0x200000];
	for k in [0usize, 5, 9, 111] {
		ram[0x19b2f8 + k] = 1;
	}
	ram[0x1396c8] = 2;
	assert_eq!(platinum(&ram), (4, 2, 2));
	ram[0x1396c8] = 9;
	assert_eq!(platinum(&ram), (4, 9, 0));
}
