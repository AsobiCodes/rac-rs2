use super::*;
use crate::modern_screen::UP;
use crate::modern_qs_page as page;

fn cell(item: u32) -> Cell {
	Cell { item, owned: item != 0, name: format!("n{item}"), ammo: (item < 40).then_some((5, 10)), icon: 0xea60 + item, extra: Default::default() }
}

fn facts(sel: (usize, usize), ring_cursor: u32) -> Facts {
	Facts {
		weapons: WEAPONS.iter().map(|&i| cell(i)).collect(),
		hand: HAND.iter().map(|&i| if i == 0 { Cell::default() } else { cell(i) }).collect(),
		sel,
		ring: (0..8).map(|k| Slot { icon: 0xea64, item: if k < 6 { WEAPONS[k] } else { 0 }, name: String::new() }).collect(),
		ring_cursor,
		tabs: (0..7).map(|k| format!("t{k}")).collect(),
		..Default::default()
	}
}

#[test]
fn gate_page_5_under_the_root() {
	assert!(gate(5, 2, 0x1fb8000));
	assert!(!gate(5, 2, 0) && !gate(5, 3, 1) && !gate(4, 2, 1) && !gate(7, 2, 1));
}

#[test]
fn cursor_words_must_match_the_tables() {
	assert_eq!(sel_of(1, 0, 1, 0, 23), Some((0, 4)));
	assert_eq!(sel_of(1, 3, 5, 0, 9), Some((0, 23)));
	assert_eq!(sel_of(1, 1, 1, 1, 46), Some((1, 4)));
	assert_eq!(sel_of(1, 2, 0, 1, 39), Some((1, 2)));
	assert_eq!(sel_of(1, 0, 1, 0, 24), None); // wrong item for the cell
	assert_eq!(sel_of(1, 0, 1, 1, 0), None); // the hole never holds the cursor
	assert_eq!(sel_of(0, 0, 1, 0, 23), None);
	assert_eq!(sel_of(1, 4, 0, 0, 30), None);
}

#[test]
fn toward_walks_within_and_across_groups() {
	assert_eq!(toward((0, 4), (0, 4)), None);
	assert_eq!(toward((0, 4), (0, 6)), Some(RIGHT)); // column first
	assert_eq!(toward((0, 6), (0, 22)), Some(DOWN));
	assert_eq!(toward((0, 6), (1, 2)), Some(DOWN)); // to row 5 first
	assert_eq!(toward((0, 22), (1, 2)), Some(RIGHT)); // then right: enters the Hand grid
	assert_eq!(toward((1, 4), (0, 3)), Some(LEFT));
	assert_eq!(toward((1, 4), (1, 5)), Some(RIGHT));
	assert_eq!(toward((1, 4), (1, 1)), Some(UP));
}

#[test]
fn ring_turns_the_short_way() {
	assert_eq!(ring_steps(0, 0), (true, 0));
	assert_eq!(ring_steps(0, 3), (true, 3));
	assert_eq!(ring_steps(0, 5), (false, 3));
	assert_eq!(ring_steps(7, 0), (true, 1));
	assert_eq!(ring_steps(0, 4), (true, 4));
}

fn run(w: &mut QWalk, f: &Facts, fields: usize) -> Vec<u16> {
	(0..fields).map(|_| w.step(f)).collect()
}

#[test]
fn walk_pulses_two_on_two_off_and_gives_a_pick_once() {
	let mut w = QWalk { req: Req { grid: Some((0, 4)), pick: true, ..Default::default() }, ..Default::default() };
	// already there: Cross only
	assert_eq!(run(&mut w, &facts((0, 4), 0), 4), vec![BIT[CROSS], BIT[CROSS], 0, 0]);
	assert!(!w.busy());
	let mut w = QWalk { req: Req { grid: Some((0, 5)), ..Default::default() }, ..Default::default() };
	assert_eq!(run(&mut w, &facts((0, 4), 0), 4), vec![RIGHT, RIGHT, 0, 0]);
	assert_eq!(run(&mut w, &facts((0, 5), 0), 4), vec![0; 4]); // arrived, no pick
	assert!(!w.busy());
}

#[test]
fn walk_turns_the_ring_then_the_grid() {
	let mut w = QWalk { req: Req { ring: Some(6), grid: Some((0, 1)), ..Default::default() }, ..Default::default() };
	assert_eq!(run(&mut w, &facts((0, 0), 0), 2), vec![BIT[L1], BIT[L1]]);
	let mut w = QWalk { req: Req { ring: Some(6), ..Default::default() }, ..Default::default() };
	run(&mut w, &facts((0, 0), 5), 4); // cursor 5 = slot 6 is the Cross target
	assert!(!w.busy());
}

#[test]
fn walk_gives_up_on_a_press_the_game_ignores() {
	let mut w = QWalk { req: Req { grid: Some((0, 3)), ..Default::default() }, ..Default::default() };
	let f = facts((0, 0), 0);
	let n = (0..400).map(|_| w.step(&f)).filter(|&b| b != 0).count();
	assert_eq!(n as u8, MAX_PRESSES * 2);
	assert!(!w.busy());
}

#[test]
fn back_is_triangle() {
	let mut w = QWalk { req: Req { back: true, ..Default::default() }, ..Default::default() };
	assert_eq!(run(&mut w, &facts((0, 0), 0), 4), vec![BIT[TRIANGLE], BIT[TRIANGLE], 0, 0]);
}

#[test]
fn page_hit_rects() {
	let f = facts((0, 4), 2);
	let b = page::build(&f, 0);
	assert_eq!(b.cells.len(), 24 + 5);
	let r = b.cells.iter().find(|c| c.0 == (0, 9)).unwrap().1;
	assert_eq!(b.cell_at(r.cx(), r.cy()), Some((0, 9)));
	let h = b.cells.iter().find(|c| c.0 == (1, 4)).unwrap().1;
	assert_eq!(b.cell_at(h.cx(), h.cy()), Some((1, 4)));
	assert!(b.cells.iter().all(|c| c.0 != (1, 3))); // the hole has no rect
	for i in 0..8u32 {
		let (x, y) = page::slot_centre(i as usize);
		assert_eq!(b.slot_at(x, y), Some(i));
	}
	assert!(b.button.is_some());
	assert!(b.prev.cx() < b.slots[0].cx() && b.next.cx() > b.slots[0].cx());
	assert_eq!(b.cell_at(5.0, 5.0), None);
	// the button is left out on an unowned cell
	let mut f2 = facts((0, 4), 0);
	f2.weapons[4].owned = false;
	assert!(page::build(&f2, 0).button.is_none());
}

#[test]
fn ring_marks_and_names() {
	assert_eq!(facts((0, 0), 7).ring_target(), 0);
	assert_eq!(facts((0, 0), 2).ring_target(), 3);
	let f = facts((0, 0), 0);
	assert!(f.in_ring(30) && !f.in_ring(9) && !f.in_ring(0));
}

#[test]
fn ring_slots_get_their_weapon_ammo_bar() {
	let f = facts((0, 0), 0);
	assert_eq!(page::ring_ammo(&f, WEAPONS[0]), cell(WEAPONS[0]).ammo);
	assert_eq!(page::ring_ammo(&f, 0), None, "an empty slot has no bar");
	assert_eq!(page::ring_ammo(&f, 9999), None, "an item outside the weapons grid has no bar");
	let mut g = f.clone();
	g.ring.iter_mut().filter(|s| s.item != 0).for_each(|s| s.item = 9000 + s.item); // same icons, items with no ammo
	let prims = |f: &Facts| page::build(f, 0).prims.len();
	let with = (0..6).filter(|&k| page::ring_ammo(&f, f.ring[k].item).is_some()).count();
	let dots = f.weapons.iter().chain(&f.hand).filter(|c| c.item != 0 && c.owned && f.in_ring(c.item)).count();
	assert!(with > 0);
	assert_eq!(prims(&f) - prims(&g), 2 * with + dots, "two bar fills per ring weapon with ammo (plus the grid's in-ring dots)");
}

/// The ring pointer moves with the level's menu manager word: 0x1a91a0 on most levels, 0x1a91e0 on level 1 (Oozla), where the fixed
/// word left the page classic (RAM of fg1 0025 with Quick Select up, dumped 2026-11-12: two ring slots, items 30 and 42).
#[test]
fn ring_pointer_follows_the_manager_word() {
	assert_eq!(ring_ptr_at(crate::modern_screen::MANAGER), RING_PTR);
	assert_eq!(ring_ptr_at(0x1a910c), 0x1a91e0);
	let Ok(ram) = std::fs::read("E:/ratchet/scratch_rc2_b42/bisect/dump_qs/ram00190.bin") else { return };
	let s = crate::modern_open::find(&ram).and_then(|l| crate::modern_open::snap(&ram, &l)).expect("menu up");
	assert_eq!(s.base, 0x1a910c);
	let p = rd(&ram, ring_ptr_at(s.base));
	assert!((0x10_0000..0x1ff_0000).contains(&p), "ring array pointer {p:#x}");
	assert_eq!((rd(&ram, p + 0x18), rd(&ram, p + RING_STRIDE + 0x18)), (30, 42));
}

#[test]
fn two_fast_arrow_clicks_turn_the_ring_twice() {
	let first = arrow_target(None, 3, true, 1000);
	assert_eq!(first, 4);
	assert_eq!(arrow_target(Some((first, 1000)), 3, true, 1100), 5, "from the slot the first click asked, not retail's lagging cursor");
	assert_eq!(arrow_target(Some((first, 1000)), 4, false, 1000 + ARROW_PENDING_MS), 3, "a stale request is ignored");
	assert_eq!(arrow_target(None, 0, false, 0), 7);
}
