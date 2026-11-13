use super::*;
use crate::modern_grid_page as page;
use rac_engine::ui::prim::Prim;
use rac_engine::input::binds::{BIT, CROSS, TRIANGLE};

fn put16(ram: &mut [u8], a: usize, v: u16) {
	ram[a..a + 2].copy_from_slice(&v.to_le_bytes());
}

fn put_table(ram: &mut [u8], a: usize, items: &[u32]) {
	for (k, &it) in items.iter().enumerate() {
		let e = a + 10 * k;
		if it != 0 {
			put16(ram, e, 0xea60 + k as u16 + 1);
		}
		put16(ram, e + 6, it as u16);
	}
}

fn ram_with_tables() -> Vec<u8> {
	let mut ram = vec![0u8; 0x20_0000];
	put_table(&mut ram, 0x11_0000, &WEAPONS);
	put_table(&mut ram, 0x12_0002, &HAND);
	put_table(&mut ram, 0x13_0000, &BACK);
	put_table(&mut ram, 0x14_0004, &FOOT);
	ram
}

#[test]
fn gate_needs_a_grid_page_under_the_pause_root_in_a_game() {
	assert_eq!(gate(3, 2, 0x1000), Some(Page::Weapons));
	assert_eq!(gate(4, 2, 0x1000), Some(Page::Gadgets));
	assert_eq!(gate(2, 0, 0x1000), None, "the root itself is the pause page's");
	assert_eq!(gate(3, 2, 0), None, "no game behind it");
	assert_eq!(gate(3, 5, 0x1000), None, "another parent");
	assert_eq!(gate(15, 2, 0x1000), None, "the icon-grid group stays classic");
}

#[test]
fn table_finder_locates_the_four_tables_and_notices_when_they_move() {
	let mut ram = ram_with_tables();
	let t = Tables::find(&ram).expect("tables");
	assert_eq!((t.weapons, t.gadgets), (0x11_0000, [0x12_0002, 0x13_0000, 0x14_0004]));
	assert!(t.still(&ram));
	put16(&mut ram, 0x11_0000 + 6, 7);
	assert!(!t.still(&ram), "an overwritten entry");
	assert!(Tables::find(&ram).is_none());
}

#[test]
fn presses_walk_groups_then_columns_then_rows() {
	let cols = [4usize];
	assert_eq!(press_toward(&cols, (0, 0), (0, 0)), None);
	assert_eq!(press_toward(&cols, (0, 0), (0, 5)), Some(RIGHT), "column first");
	assert_eq!(press_toward(&cols, (0, 1), (0, 5)), Some(DOWN), "then the row");
	assert_eq!(press_toward(&cols, (0, 6), (0, 1)), Some(LEFT));
	assert_eq!(press_toward(&cols, (0, 5), (0, 1)), Some(UP));
	let g = [3usize, 2, 3];
	assert_eq!(press_toward(&g, (0, 4), (2, 1)), Some(DOWN), "the next group is Down");
	assert_eq!(press_toward(&g, (2, 0), (0, 0)), Some(UP));
}

fn facts(sel: (usize, usize)) -> Facts {
	let cell = |item| Cell { item, owned: true, name: format!("n{item}"), ammo: Some((5, 10)), icon: 0xea61, ..Default::default() };
	Facts {
		page: Page::Weapons,
		groups: vec![Group { label: String::new(), cols: 4, cells: WEAPONS.iter().map(|&i| cell(i)).collect() }],
		sel,
		tabs: vec!["Weapons".into(), "Gadgets".into()],
		site: "Veldin".into(),
		bolts: 12,
		equip: "Equip".into(),
		no_ammo: String::new(),
		level: 6,
	}
}

#[test]
fn walk_pulses_two_on_two_off_and_equips_only_on_the_target() {
	let mut w = Walk { target: Some((0, 1)), confirm: true, ..Walk::default() };
	let mut f = facts((0, 0));
	let got: Vec<u16> = (0..4).map(|_| w.step(&f)).collect();
	assert_eq!(got, vec![RIGHT, RIGHT, 0, 0]);
	f.sel = (0, 1);
	let got: Vec<u16> = (0..4).map(|_| w.step(&f)).collect();
	assert_eq!(got, vec![BIT[CROSS], BIT[CROSS], 0, 0], "equip once the game's cursor is there");
	assert!(!w.busy());
}

#[test]
fn walk_never_equips_a_cell_the_cursor_did_not_reach_and_gives_up() {
	let mut w = Walk { target: Some((0, 1)), confirm: true, ..Walk::default() };
	let f = facts((0, 0));
	let mut crossed = false;
	for _ in 0..4 * (MAX_PRESSES as usize + 4) {
		crossed |= w.step(&f) == BIT[CROSS];
	}
	assert!(!crossed);
	assert!(!w.busy(), "gave up after the press limit");
	let mut b = Walk { back: true, ..Walk::default() };
	assert_eq!(b.step(&f), BIT[TRIANGLE]);
}

#[test]
fn page_builds_cells_button_and_hole() {
	let f = facts((0, 5));
	let b = page::build(&f, &rac_engine::ui::font::FontMeasure, 4.0 / 3.0, 0);
	assert_eq!(b.cells.len(), 24);
	assert!(b.button.is_some());
	let (_, r) = b.cells[5];
	assert_eq!(b.cell_at(r.cx(), r.cy()), Some((0, 5)));
	assert!(page::VIEWPORT.contains(b.hole.cx(), b.hole.cy()));
	let wide = page::hole_for(Page::Weapons, 16.0 / 9.0);
	assert!(wide.w / wide.h > b.hole.w / b.hole.h, "a wider picture gives a wider hole");
	assert!(b.uv.iter().all(|v| (0.0..=1.0).contains(v)));
}

#[test]
fn unowned_cells_are_blank_and_have_no_button() {
	let mut f = facts((0, 0));
	f.groups[0].cells[0].owned = false;
	let b = page::build(&f, &rac_engine::ui::font::FontMeasure, 4.0 / 3.0, 0);
	assert!(b.button.is_none());
}

#[test]
fn with_grid_replaces_the_giant_scrim_by_strips() {
	let f = facts((0, 0));
	let b = page::build(&f, &rac_engine::ui::font::FontMeasure, 4.0 / 3.0, 0);
	let screen = page::shell(&f, crate::modern_pages::Look::default());
	let (_, shell) = rac_engine::ui::kit::page_draw::frame(&screen, &rac_engine::ui::font::FontMeasure);
	let giant = |p: &Prim| matches!(p, Prim::RRect { rect, .. } if rect.w >= 10000.0);
	assert!(shell.iter().any(giant));
	let all = page::with_grid(shell, &b);
	let (hx, hy) = (b.hole.cx(), b.hole.cy());
	assert!(!all.iter().any(|p| giant(p) && matches!(p, Prim::RRect { rect, .. } if rect.contains(hx, hy))), "the scrim leaves the hole open");
	assert!(all.len() > b.prims.len());
}

#[test]
fn gadget_group_shapes_skip_the_hole_cell() {
	let mut f = facts((0, 0));
	f.page = Page::Gadgets;
	let c = |item| Cell { item, owned: item != 0, name: String::new(), ammo: None, icon: 0, ..Default::default() };
	f.groups = vec![
		Group { label: "Hand".into(), cols: 3, cells: HAND.iter().map(|&i| c(i)).collect() },
		Group { label: "Back".into(), cols: 2, cells: BACK.iter().map(|&i| c(i)).collect() },
		Group { label: "Foot".into(), cols: 3, cells: FOOT.iter().map(|&i| c(i)).collect() },
	];
	let b = page::build(&f, &rac_engine::ui::font::FontMeasure, 4.0 / 3.0, 0);
	assert_eq!(b.cells.len(), 5 + 2 + 3);
	assert!(b.cells.iter().all(|c| c.0 != (0, 3)));
}
