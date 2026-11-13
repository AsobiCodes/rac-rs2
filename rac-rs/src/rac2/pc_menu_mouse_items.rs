//! [PC] [b47 X4] The mouse on the classic ITEMS page (page id 6 under the pause root; sweep B25, W7's REMAINS): hovering a cell walks
//! retail's own cursor there by D-pad pulses (the walk `pc_menu_mouse_grid.rs` `Grid` does, over the page's own wrapping rules in
//! `modern_items::press_toward`), a click on a cell presses Cross once the cursor is there, a click on no cell is nothing. Read only.
//!
//! Facts [C] (modern_items.rs header; cell places MEASURED on the classic picture `b46/w7/runs/cls1/shots/w00480.png`, the 1024 x 768
//! window): cursor {1, column, row, item id, group} at [menu manager] + 0x3a558; group 0 Head (2 x 1), 1 Armor (1 cell, right of Head),
//! 2 Items (4 x 2), 3 Collectibles (2 x 2, the lower right panel). Plates 97 x 52 px.
use crate::modern_grid::rd;
use crate::modern_items::{COLS, GROUP_LEN, PAGE_ITEMS};

const CURSOR: u32 = 0x3a558;
/// Cell (group, index in group) -> centre in px of the 1024 x 768 window.
const HEAD: [(f32, f32); 2] = [(86.0, 197.0), (195.0, 197.0)];
const ARMOR: (f32, f32) = (410.0, 197.0);
const ITEMS_X: [f32; 4] = [86.0, 195.0, 303.0, 411.0];
const ITEMS_Y: [f32; 2] = [266.0, 325.0];
const COLL_X: [f32; 2] = [561.0, 668.0];
const COLL_Y: [f32; 2] = [454.0, 514.0];

/// The Items cell (group, index) under the pointer (fractions of the picture), if any. Pure.
pub fn cell(fx: f32, fy: f32) -> Option<(u8, u8)> {
	let (x, y) = (fx * 1024.0, fy * 768.0);
	let hit = |c: (f32, f32)| (x - c.0).abs() <= 48.0 && (y - c.1).abs() <= 26.0;
	if let Some(k) = HEAD.iter().position(|&c| hit(c)) {
		return Some((0, k as u8));
	}
	if hit(ARMOR) {
		return Some((1, 0));
	}
	for (r, &cy) in ITEMS_Y.iter().enumerate() {
		if let Some(c) = ITEMS_X.iter().position(|&cx| hit((cx, cy))) {
			return Some((2, (r * 4 + c) as u8));
		}
	}
	for (r, &cy) in COLL_Y.iter().enumerate() {
		if let Some(c) = COLL_X.iter().position(|&cx| hit((cx, cy))) {
			return Some((3, (r * 2 + c) as u8));
		}
	}
	None
}

/// The page's cursor as (group, index): the words {live, column, row, item id, group}. Pure over RAM.
pub fn cursor(ram: &[u8], page_id: u32, parent_id: u32, manager: u32) -> Option<(u8, u8)> {
	if page_id != PAGE_ITEMS || parent_id != 2 || manager == 0 {
		return None;
	}
	let m = |o: u32| rd(ram, manager.wrapping_add(CURSOR + o)) as usize;
	let (live, col, row, group) = (m(0), m(4), m(8), m(16));
	if live != 1 || group > 3 || col >= COLS[group] {
		return None;
	}
	let k = row * COLS[group] + col;
	(k < GROUP_LEN[group]).then_some((group as u8, k as u8))
}

/// The classic Items page of this field (the cursor cell), None on any other page, with the modern page up, or without a game.
pub fn items(ram: &[u8]) -> Option<(u8, u8)> {
	let s = crate::modern_open::snap_now(ram)?;
	let parent = rd(ram, rd(ram, s.page + 0xe0).wrapping_add(0xe4));
	if crate::modern_screen::manager(ram, &s) == 0 || crate::modern_items::facts().is_some() {
		return None;
	}
	cursor(ram, s.page_id, parent, s.manager)
}

/// The press from one cell toward another over the page's own wrapping rules (Head / Armor / Items / Collectibles).
pub fn press(cur: (u8, u8), to: (u8, u8)) -> u16 {
	crate::modern_items::press_toward((cur.0 as usize, cur.1 as usize), (to.0 as usize, to.1 as usize)).unwrap_or(0)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn cells_on_the_measured_picture() {
		assert_eq!(cell(86.0 / 1024.0, 197.0 / 768.0), Some((0, 0)));
		assert_eq!(cell(195.0 / 1024.0, 197.0 / 768.0), Some((0, 1)));
		assert_eq!(cell(410.0 / 1024.0, 197.0 / 768.0), Some((1, 0)));
		assert_eq!(cell(300.0 / 1024.0, 197.0 / 768.0), None, "the gap between Head and Armor");
		assert_eq!(cell(411.0 / 1024.0, 325.0 / 768.0), Some((2, 7)));
		assert_eq!(cell(86.0 / 1024.0, 325.0 / 768.0), Some((2, 4)));
		assert_eq!(cell(668.0 / 1024.0, 514.0 / 768.0), Some((3, 3)));
		assert_eq!(cell(561.0 / 1024.0, 454.0 / 768.0), Some((3, 0)));
		assert_eq!(cell(700.0 / 1024.0, 300.0 / 768.0), None, "the picture panel");
		assert_eq!(cell(200.0 / 1024.0, 480.0 / 768.0), None, "the description box");
		assert_eq!(cell(561.0 / 1024.0, 560.0 / 768.0), None, "the Exit plate");
	}

	#[test]
	fn the_cursor_reads_from_the_manager() {
		let mut ram = vec![0u8; 0x200_0000];
		let mgr = 0x1fb_8000u32;
		let set = |r: &mut Vec<u8>, words: [u32; 5]| {
			for (k, v) in words.iter().enumerate() {
				let a = (mgr + CURSOR) as usize + 4 * k;
				r[a..a + 4].copy_from_slice(&v.to_le_bytes());
			}
		};
		set(&mut ram, [1, 2, 1, 38, 2]);
		assert_eq!(cursor(&ram, 6, 2, mgr), Some((2, 6)), "Items column 2, row 1");
		assert_eq!(cursor(&ram, 4, 2, mgr), None, "not the Items page");
		set(&mut ram, [1, 1, 1, 0, 3]);
		assert_eq!(cursor(&ram, 6, 2, mgr), Some((3, 3)), "Collectibles column 1, row 1");
		set(&mut ram, [1, 1, 0, 6, 1]);
		assert_eq!(cursor(&ram, 6, 2, mgr), None, "Armor has one column");
		set(&mut ram, [1, 0, 0, 47, 0]);
		assert_eq!(press((0, 0), (0, 1)) & 0x2000, 0x2000, "Right moves Head 0 -> 1");
		assert_eq!(cursor(&ram, 6, 2, mgr), Some((0, 0)));
	}
}
