//! [PC] [b46 W7] The mouse on the classic WEAPONS grid page (sweep B25; R&C1 `retail/mouse_menu.rs`): hovering a cell walks retail's
//! own cursor there by D-pad pulses (read back every field), a click on a cell presses Cross once the cursor has arrived, a click on no
//! cell is nothing (the click bit is stripped in `pc_menu_hover.rs` `Hover::field`, which calls this). Read only: nothing is written to
//! game RAM. With Modern menus the page is modern (modern_grid.rs) and drives its own mouse: then this reader steps aside (the
//! classic click bit exists only in the Classic style, `pc_input.rs`).
//!
//! Facts [C] (modern_grid.rs; RAM of fg1 0279 etc.): page id 3 under the pause root (id 2); the cursor is {1, column 0..3, row 0..5} at
//! [menu manager] + 0x39b90. Cell places MEASURED on the classic picture (`b46/w7/runs/cls1/shots/w00130.png`, 1024 x 768 window, the 4:3
//! retail picture filling it): cell plates are 100 x 52 px, centres x = 102 + 106.3 c, y = 193 + 60.6 r (px), as fractions of the picture
//! below. The Gadgets (item-id cursor, three groups) and Items pages are not mapped: a click there is nothing.
use crate::modern_grid::rd;

pub const PAGE_WEAPONS: u32 = 3;
const COLS: u8 = 4;
const ROWS: u8 = 6;
const CURSOR: u32 = 0x39b90;
const C0: f32 = 102.0 / 1024.0;
const CPITCH: f32 = 106.3 / 1024.0;
const R0: f32 = 193.0 / 768.0;
const RPITCH: f32 = 60.6 / 768.0;
/// Half the plate: a pointer outside it (the gap between plates, the panels) is on no cell.
const HALF: (f32, f32) = (50.0 / 1024.0, 26.0 / 768.0);
const UP: u16 = 0x1000;
const DOWN: u16 = 0x4000;
const LEFT: u16 = 0x8000;
const RIGHT: u16 = 0x2000;
const DPAD: u16 = 0xf000;
const CROSS: u16 = 0x40;
/// Pulses tried for one hovered cell before giving up (the whole grid is 3 + 5 steps away at most).
const BUDGET: u8 = 14;
/// Fields per pulse (2 pressed, the rest released), as the root's walk.
const PERIOD: u8 = 4;

/// The Weapons cell (column, row) under the pointer (fractions of the picture), if any. Pure.
pub fn weapons_cell(fx: f32, fy: f32) -> Option<(u8, u8)> {
	let (c, r) = (((fx - C0) / CPITCH).round(), ((fy - R0) / RPITCH).round());
	let on = (0.0..COLS as f32).contains(&c) && (0.0..ROWS as f32).contains(&r) && (fx - C0 - c * CPITCH).abs() <= HALF.0 && (fy - R0 - r * RPITCH).abs() <= HALF.1;
	on.then_some((c as u8, r as u8))
}

/// The classic Weapons page's cursor (column, row) from RAM, given the page id, the parent record's id and the manager word. Pure over RAM.
pub fn weapons_cursor(ram: &[u8], page_id: u32, parent_id: u32, manager: u32) -> Option<(u8, u8)> {
	if page_id != PAGE_WEAPONS || parent_id != 2 || manager == 0 {
		return None;
	}
	let (flag, col, row) = (rd(ram, manager.wrapping_add(CURSOR)), rd(ram, manager.wrapping_add(CURSOR + 4)), rd(ram, manager.wrapping_add(CURSOR + 8)));
	(flag == 1 && col < COLS as u32 && row < ROWS as u32).then_some((col as u8, row as u8))
}

/// The classic Weapons page of this field (the cursor), None on any other page or without a game.
pub fn weapons(ram: &[u8]) -> Option<(u8, u8)> {
	let s = crate::modern_open::snap_now(ram)?;
	let parent = rd(ram, rd(ram, s.page + 0xe0).wrapping_add(0xe4));
	if crate::modern_screen::manager(ram, &s) == 0 || crate::modern_grid::facts().is_some() {
		return None;
	}
	weapons_cursor(ram, s.page_id, parent, s.manager)
}

/// The D-pad press from `cur` toward `to`: the column first, then the row (as modern_grid.rs `press_toward`).
pub fn press_cells(cur: (u8, u8), to: (u8, u8)) -> u16 {
	if cur.0 != to.0 {
		return if to.0 > cur.0 { RIGHT } else { LEFT };
	}
	if cur.1 != to.1 {
		return if to.1 > cur.1 { DOWN } else { UP };
	}
	0
}

/// Gadgets cells as (group, index in the group), centres in px of the 1024 x 768 picture measured on `b46/w7/runs/cls1/shots/w00290.png`
/// (Hand Items 3 x 2 without cell 3, Back Packs 2 right-aligned, Foot Items 3), plates 96 x 52.
const GADGET_AT: [(u8, u8, f32, f32); 8] = [
	(0, 0, 183.0, 239.0),
	(0, 1, 293.0, 239.0),
	(0, 2, 402.0, 239.0),
	(0, 4, 293.0, 297.0),
	(0, 5, 402.0, 297.0),
	(1, 0, 290.0, 410.0),
	(1, 1, 399.0, 410.0),
	(2, 0, 183.0, 526.0),
];
const GADGET_FOOT: [(f32, f32); 2] = [(293.0, 526.0), (402.0, 526.0)];

/// The Gadgets cell (group, index) under the pointer, if any. Pure.
pub fn gadget_cell(fx: f32, fy: f32) -> Option<(u8, u8)> {
	let (x, y) = (fx * 1024.0, fy * 768.0);
	let hit = |cx: f32, cy: f32| (x - cx).abs() <= 48.0 && (y - cy).abs() <= 26.0;
	GADGET_AT.iter().find(|c| hit(c.2, c.3)).map(|c| (c.0, c.1)).or_else(|| GADGET_FOOT.iter().position(|c| hit(c.0, c.1)).map(|k| (2, k as u8 + 1)))
}

/// The classic Gadgets page's cursor as (group, index): the cursor word block {1, col, row, ITEM ID} names the cell by its item id.
pub fn gadget_cursor(ram: &[u8], page_id: u32, parent_id: u32, manager: u32) -> Option<(u8, u8)> {
	if page_id != crate::modern_grid::PAGE_GADGETS || parent_id != 2 || manager == 0 {
		return None;
	}
	let at = manager.wrapping_add(crate::modern_grid::GADGETS_CURSOR);
	let id = rd(ram, at.wrapping_add(12));
	if rd(ram, at) != 1 || id == 0 {
		return None;
	}
	crate::modern_grid::GADGETS.iter().enumerate().find_map(|(g, (its, _))| its.iter().position(|&i| i == id).map(|k| (g as u8, k as u8)))
}

/// The press from (group, index) toward another: the groups first (Down / Up), then the column, then the row (modern_grid.rs).
pub fn press_gadgets(cur: (u8, u8), to: (u8, u8)) -> u16 {
	let cols = [3usize, 2, 3];
	crate::modern_grid::press_toward(&cols, (cur.0 as usize, cur.1 as usize), (to.0 as usize, to.1 as usize)).unwrap_or(0)
}

/// The classic Gadgets page of this field (the cursor cell), None on any other page or without a game.
pub fn gadgets(ram: &[u8]) -> Option<(u8, u8)> {
	let s = crate::modern_open::snap_now(ram)?;
	let parent = rd(ram, rd(ram, s.page + 0xe0).wrapping_add(0xe4));
	if crate::modern_screen::manager(ram, &s) == 0 || crate::modern_grid::facts().is_some() {
		return None;
	}
	gadget_cursor(ram, s.page_id, parent, s.manager)
}

/// Quick Select page: its weapons grid (4 x 6, plates 96 x 50), centres in px of the 1024 x 768 picture measured on
/// `b46/w7/runs/cls1/shots/w00390.png`: x = 115 + 106 c, y = 207 + 61.8 r. The Hand gadgets grid and the ring are not mapped (a click there is nothing).
pub fn qs_cell(fx: f32, fy: f32) -> Option<(u8, u8)> {
	let (x, y) = (fx * 1024.0, fy * 768.0);
	let (c, r) = (((x - 115.0) / 106.0).round(), ((y - 207.0) / 61.8).round());
	let on = (0.0..COLS as f32).contains(&c) && (0.0..ROWS as f32).contains(&r) && (x - 115.0 - c * 106.0).abs() <= 48.0 && (y - 207.0 - r * 61.8).abs() <= 25.0;
	on.then_some((c as u8, r as u8))
}

/// The Quick Select page's grid cursor (column, row) while it is on the weapons grid (group 0): {+0 widget, +4 column, +8 row, +0xc group}.
pub fn qs_cursor(ram: &[u8], page_id: u32, parent_id: u32, manager: u32) -> Option<(u8, u8)> {
	if page_id != crate::modern_qs::PAGE_QS || parent_id != 2 || manager == 0 {
		return None;
	}
	let at = manager.wrapping_add(crate::modern_qs::CURSOR);
	let (col, row, group) = (rd(ram, at.wrapping_add(4)), rd(ram, at.wrapping_add(8)), rd(ram, at.wrapping_add(12)));
	(group == 0 && col < COLS as u32 && row < ROWS as u32).then_some((col as u8, row as u8))
}

/// The classic Quick Select page of this field (the weapons-grid cursor), None otherwise.
pub fn qs(ram: &[u8]) -> Option<(u8, u8)> {
	let s = crate::modern_open::snap_now(ram)?;
	let parent = rd(ram, rd(ram, s.page + 0xe0).wrapping_add(0xe4));
	if crate::modern_screen::manager(ram, &s) == 0 || crate::modern_qs::facts().is_some() {
		return None;
	}
	qs_cursor(ram, s.page_id, parent, s.manager)
}

/// Save / Load slot pages (ids 30 / 31): the visible slot cards, centres of the 640 x 480 picture measured on `b46/w7/runs/cls2/sheet.png`
/// (cells cls2 / 290 and 390): x 20..200 px, y = 194 + 72 k (k 0..2), cards 62 px high. A cell is (0, slot).
pub fn slot_cell(fx: f32, fy: f32) -> Option<(u8, u8)> {
	let (x, y) = (fx * 640.0, fy * 480.0);
	let k = ((y - 194.0) / 72.0).round();
	((20.0..=200.0).contains(&x) && (0.0..3.0).contains(&k) && (y - 194.0 - k * 72.0).abs() <= 31.0).then_some((0, k as u8))
}

/// The slot page's cursor as (0, slot): [page record + 0xe8] (the page descriptor) + 0x40, 0..3; the 4th slot scrolls the cards (not
/// measured), so the page is unmapped while the cursor is on it.
pub fn slot_cursor(ram: &[u8], page_id: u32, page_rec: u32) -> Option<(u8, u8)> {
	if !matches!(page_id, 30 | 31) || page_rec == 0 {
		return None;
	}
	let cur = rd(ram, rd(ram, page_rec.wrapping_add(0xe8)).wrapping_add(0x40));
	(cur < 3).then_some((0, cur as u8))
}

/// The classic Save / Load slot page of this field (the cursor), None otherwise (with the modern cards up too).
pub fn slots(ram: &[u8]) -> Option<(u8, u8)> {
	let s = crate::modern_open::snap_now(ram)?;
	if crate::modern_load::facts().is_some() {
		return None;
	}
	slot_cursor(ram, s.page_id, s.page)
}

/// The hover / click walk over a grid (state between fields).
#[derive(Default)]
pub struct Grid {
	target: Option<(u8, u8)>,
	spent: u8,
	pulse: u8,
	click: u8,
}

impl Grid {
	/// Forget the walk (the page is not up).
	pub fn reset(&mut self) {
		*self = Grid::default();
	}

	/// One field. `cur` = retail's cursor, `hover` = the cell the pointer moved onto this field, `on` = the cell under the pointer now,
	/// `press` = a left click began this field, `b` = the buttons (without the click bit); returns them with the pulses / the Cross.
	pub fn field(&mut self, cur: (u8, u8), hover: Option<(u8, u8)>, on: Option<(u8, u8)>, press: bool, mut b: u16, step: fn((u8, u8), (u8, u8)) -> u16) -> u16 {
		if let Some(t) = hover.filter(|&t| self.target != Some(t)) {
			(self.target, self.spent) = (Some(t), 0);
		}
		// A D-pad press of the player's own cancels the walk and the held click (a click only confirms the cell under the pointer).
		if b & DPAD != 0 {
			(self.target, self.click) = (None, 0);
		} else if press && self.target.is_none() {
			// [b46 R2] A click on another cell with a still pointer (the keys moved the cursor away since the hover): walk there, then confirm.
			if let Some(t) = on.filter(|&t| t != cur) {
				(self.target, self.spent) = (Some(t), 0);
			}
		}
		if self.pulse == 0 && self.target.is_some_and(|t| t == cur || self.spent >= BUDGET) {
			if self.target != Some(cur) {
				self.click = 0;
			}
			self.target = None;
		}
		if let Some(t) = self.target {
			if press {
				self.click = if on == Some(t) { 2 } else { 0 };
			}
			self.pulse += 1;
			if self.pulse == 1 {
				self.spent += 1;
			}
			// The cursor can arrive on the pulse's first field: then the second presses nothing.
			if self.pulse <= 2 && t != cur {
				b |= step(cur, t);
			}
			if self.pulse >= PERIOD {
				self.pulse = 0;
			}
			return b;
		}
		if press && on == Some(cur) {
			self.click = 2;
		}
		if self.click > 0 {
			self.click -= 1;
			b |= CROSS;
		}
		b
	}
}


#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn cells_on_the_measured_picture() {
		assert_eq!(weapons_cell(102.0 / 1024.0, 193.0 / 768.0), Some((0, 0)));
		assert_eq!(weapons_cell(421.0 / 1024.0, 495.0 / 768.0), Some((3, 5)));
		assert_eq!(weapons_cell(315.0 / 1024.0, 317.0 / 768.0), Some((2, 2)));
		assert_eq!(weapons_cell(155.0 / 1024.0, 193.0 / 768.0), None, "the gap between two plates");
		assert_eq!(weapons_cell(700.0 / 1024.0, 250.0 / 768.0), None, "the picture panel");
		assert_eq!(weapons_cell(102.0 / 1024.0, 130.0 / 768.0), None, "the title bar");
		assert_eq!(weapons_cell(102.0 / 1024.0, 560.0 / 768.0), None, "below the last row");
	}

	#[test]
	fn the_cursor_reads_from_the_manager() {
		let mut ram = vec![0u8; 0x200_0000];
		let mgr = 0x1fb_8000u32;
		let w = |r: &mut Vec<u8>, a: u32, v: u32| r[a as usize..a as usize + 4].copy_from_slice(&v.to_le_bytes());
		for (k, v) in [1u32, 2, 4].iter().enumerate() {
			w(&mut ram, mgr + CURSOR + 4 * k as u32, *v);
		}
		assert_eq!(weapons_cursor(&ram, 3, 2, mgr), Some((2, 4)));
		assert_eq!(weapons_cursor(&ram, 4, 2, mgr), None, "Gadgets is not mapped");
		assert_eq!(weapons_cursor(&ram, 3, 8, mgr), None, "not under the pause root");
		w(&mut ram, mgr + CURSOR + 4, 9);
		assert_eq!(weapons_cursor(&ram, 3, 2, mgr), None);
	}

	#[test]
	fn walk_then_press_and_never_a_blind_click() {
		let f = |g: &mut Grid, cur, hover, on, press, b| g.field(cur, hover, on, press, b, press_cells);
		let mut g = Grid::default();
		// Hover (2, 1) from (0, 0): Right pulses; a click meanwhile is held until the cursor is there.
		assert_eq!(f(&mut g, (0, 0), Some((2, 1)), Some((2, 1)), true, 0), RIGHT);
		assert_eq!(f(&mut g, (0, 0), None, Some((2, 1)), false, 0), RIGHT);
		assert_eq!(f(&mut g, (0, 0), None, Some((2, 1)), false, 0), 0);
		assert_eq!(f(&mut g, (0, 0), None, Some((2, 1)), false, 0), 0);
		assert_eq!(f(&mut g, (2, 0), None, Some((2, 1)), false, 0), DOWN);
		for _ in 0..3 {
			f(&mut g, (2, 0), None, Some((2, 1)), false, 0);
		}
		assert_eq!(f(&mut g, (2, 1), None, Some((2, 1)), false, 0), CROSS);
		assert_eq!(f(&mut g, (2, 1), None, Some((2, 1)), false, 0), CROSS);
		assert_eq!(f(&mut g, (2, 1), None, Some((2, 1)), false, 0), 0);
		// A click off every cell: nothing. A click on the selected cell: one Cross. A D-pad press drops a held click.
		let mut g = Grid::default();
		assert_eq!(f(&mut g, (1, 1), None, None, true, 0), 0);
		assert_eq!(f(&mut g, (1, 1), None, Some((1, 1)), true, 0), CROSS);
		let mut g = Grid::default();
		f(&mut g, (0, 0), Some((3, 3)), Some((3, 3)), true, 0);
		f(&mut g, (0, 0), None, Some((3, 3)), false, UP);
		assert!((0..20).all(|_| f(&mut g, (0, 0), None, Some((3, 3)), false, 0) & CROSS == 0));
		// [b46 R2] A click on another cell with a still pointer: the walk starts on the click, one Cross at the arrival.
		let mut g = Grid::default();
		assert_eq!(f(&mut g, (0, 0), None, Some((1, 0)), true, 0), RIGHT);
		for _ in 0..3 {
			f(&mut g, (0, 0), None, Some((1, 0)), false, 0);
		}
		assert_eq!(f(&mut g, (1, 0), None, Some((1, 0)), false, 0), CROSS);
		// A walk that never arrives gives up without a Cross.
		let mut g = Grid::default();
		let out: Vec<u16> = (0..(BUDGET as usize * 4 + 8)).map(|n| f(&mut g, (0, 0), if n == 0 { Some((3, 5)) } else { None }, Some((3, 5)), n == 0, 0)).collect();
		assert!(out.iter().all(|&o| o & CROSS == 0));
	}

	#[test]
	fn qs_cells_on_the_measured_picture() {
		assert_eq!(qs_cell(115.0 / 1024.0, 207.0 / 768.0), Some((0, 0)));
		assert_eq!(qs_cell(433.0 / 1024.0, 516.0 / 768.0), Some((3, 5)));
		assert_eq!(qs_cell(700.0 / 1024.0, 300.0 / 768.0), None, "the ring panel");
		assert_eq!(qs_cell(700.0 / 1024.0, 455.0 / 768.0), None, "the Hand gadgets grid");
		let mut ram = vec![0u8; 0x200_0000];
		let mgr = 0x1fb_8000u32;
		let w = |r: &mut Vec<u8>, k: u32, v: u32| { let a = (mgr + crate::modern_qs::CURSOR + k) as usize; r[a..a + 4].copy_from_slice(&v.to_le_bytes()); };
		w(&mut ram, 4, 3);
		w(&mut ram, 8, 5);
		assert_eq!(qs_cursor(&ram, 5, 2, mgr), Some((3, 5)));
		w(&mut ram, 12, 1);
		assert_eq!(qs_cursor(&ram, 5, 2, mgr), None, "the cursor is on the Hand gadgets grid");
	}

	#[test]
	fn slot_cards_and_cursor() {
		assert_eq!(slot_cell(100.0 / 640.0, 194.0 / 480.0), Some((0, 0)));
		assert_eq!(slot_cell(100.0 / 640.0, 338.0 / 480.0), Some((0, 2)));
		assert_eq!(slot_cell(100.0 / 640.0, 410.0 / 480.0), None, "no 4th card on the picture");
		assert_eq!(slot_cell(400.0 / 640.0, 266.0 / 480.0), None, "the preview panel");
		let mut ram = vec![0u8; 0x200_0000];
		let w = |r: &mut Vec<u8>, a: u32, v: u32| r[a as usize..a as usize + 4].copy_from_slice(&v.to_le_bytes());
		let (rec, desc) = (0x26_0b58u32, 0x1ab_0d8u32 + 0x100);
		w(&mut ram, rec + 0xe8, desc);
		w(&mut ram, desc + 0x40, 1);
		assert_eq!(slot_cursor(&ram, 31, rec), Some((0, 1)));
		assert_eq!(slot_cursor(&ram, 8, rec), None);
		w(&mut ram, desc + 0x40, 3);
		assert_eq!(slot_cursor(&ram, 30, rec), None, "scrolled cards");
	}

	#[test]
	fn gadget_cells_on_the_measured_picture() {
		assert_eq!(gadget_cell(183.0 / 1024.0, 239.0 / 768.0), Some((0, 0)));
		assert_eq!(gadget_cell(402.0 / 1024.0, 297.0 / 768.0), Some((0, 5)));
		assert_eq!(gadget_cell(183.0 / 1024.0, 297.0 / 768.0), None, "the hole in Hand Items");
		assert_eq!(gadget_cell(399.0 / 1024.0, 410.0 / 768.0), Some((1, 1)));
		assert_eq!(gadget_cell(183.0 / 1024.0, 526.0 / 768.0), Some((2, 0)));
		assert_eq!(gadget_cell(700.0 / 1024.0, 300.0 / 768.0), None, "the picture panel");
		// The cursor: {1, col, row, item id}; item 46 is Hand Items cell 4.
		let mut ram = vec![0u8; 0x200_0000];
		let mgr = 0x1fb_8000u32;
		for (k, v) in [1u32, 1, 1, 46].iter().enumerate() {
			let a = (mgr + crate::modern_grid::GADGETS_CURSOR) as usize + 4 * k;
			ram[a..a + 4].copy_from_slice(&v.to_le_bytes());
		}
		assert_eq!(gadget_cursor(&ram, 4, 2, mgr), Some((0, 4)));
		assert_eq!(gadget_cursor(&ram, 3, 2, mgr), None);
	}
}
