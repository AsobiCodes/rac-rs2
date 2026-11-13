//! [PC] The mouse on R&C2's classic LIST pages (build 45 P4B, sweep B25; R&C1 `retail/mouse_menu.rs` reads every page of its page
//! menu): the pages drawn by the game's menu list object (the pause Options list, page 8, and the Help list, page 7) get the pause
//! root's hover and click (`pc_menu_hover.rs`: hover = Up / Down pulses on retail's own cursor, read back each field; click = Cross
//! only once the hovered row is selected). Read only: nothing is written to game RAM. With Modern menus these pages are modern
//! (modern_optlist.rs, modern_help.rs) and drive their own mouse: then this reader steps aside.
//!
//! Facts [C] (RAM + PCSX2 shots of fg1 0001 (Options, cursor on Controls) and the Help page facts of modern_help.rs):
//! - list object L = [menu manager] + 0x3c210 (`modern_open::Snap::manager`), depth [L+0x31c], cursor [L+0x1b8 + 4 depth], row
//!   table [L+0x310 + 4 depth] (`modern_optlist::row_ids`); the Help page's cursor [manager] + 0x3c3c8 is depth 0's.
//! - Row places on the 640 x 480 shot (the same frame as the root's in pc_menu_hover.rs): row centres y = 127 + 29 k px, the row
//!   plates with their icons x 40..335 px. The Help page's icon rows sit at the same places (overlay rows 84 + 35.8 k of 448 lines,
//!   modern_help_page.rs ICON, = 127 + 28.8 k px of the 360-line picture band at y 60..420).
use crate::modern_open;
use crate::modern_optlist::{row_ids, LIST_OFF};

/// The list pages this reader drives (page ids).
pub const LIST_PAGES: [u32; 2] = [7, 8];
const ROW0: f32 = 127.0 / 480.0;
const PITCH: f32 = 29.0 / 480.0;
const X: (f32, f32) = (40.0 / 640.0, 335.0 / 640.0);

/// A classic list page up this field: retail's cursor and the row count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct List {
	pub cur: u8,
	pub n: u8,
}

/// The row of an `n`-row list under the pointer (fractions of the picture), if any. Pure.
pub fn list_row(fx: f32, fy: f32, n: u8) -> Option<u8> {
	if !(X.0..=X.1).contains(&fx) {
		return None;
	}
	let k = ((fy - ROW0) / PITCH).round();
	((0.0..n as f32).contains(&k) && (fy - ROW0 - k * PITCH).abs() <= PITCH * 0.45).then_some(k as u8)
}

fn rd(ram: &[u8], a: u32) -> u32 {
	let a = (a & 0x1ff_fffc) as usize;
	ram.get(a..a + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Whether a modern page is drawn over list page `page` (it owns the mouse then).
fn modern_up(page: u32) -> bool {
	match page {
		8 => crate::modern_optlist::facts().is_some(),
		7 => crate::modern_help::facts().is_some(),
		_ => false,
	}
}

/// The list as RAM `ram` shows it, from the page id, list depth, manager word and the list words. Pure over RAM.
pub fn list_of(ram: &[u8], page_id: u32, depth: u32, manager: u32) -> Option<List> {
	if !LIST_PAGES.contains(&page_id) || depth > 1 || manager == 0 {
		return None;
	}
	let l = manager.wrapping_add(LIST_OFF);
	let n = row_ids(ram, rd(ram, l + 0x310 + 4 * depth))?.len();
	let cur = rd(ram, l + 0x1b8 + 4 * depth) as usize;
	(cur < n).then_some(List { cur: cur as u8, n: n as u8 })
}

/// The classic list page of this field (pause menu only: the root's parent check), None without one or with a modern page over it.
pub fn list(ram: &[u8]) -> Option<List> {
	let s = modern_open::snap_now(ram)?;
	let parent = rd(ram, s.page + 0xe0);
	if parent == 0 || rd(ram, parent + 0xe4) != 2 || crate::modern_screen::manager(ram, &s) == 0 || modern_up(s.page_id) {
		return None;
	}
	list_of(ram, s.page_id, s.depth, s.manager)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn rows_on_the_0001_shot() {
		// Controls (row 3) at y 214 px of 480, Quit Game (row 6) at 301, the icon plate at x 305.
		assert_eq!(list_row(160.0 / 640.0, 214.0 / 480.0, 7), Some(3));
		assert_eq!(list_row(305.0 / 640.0, 301.0 / 480.0, 7), Some(6));
		assert_eq!(list_row(450.0 / 640.0, 214.0 / 480.0, 7), None, "the description panel");
		assert_eq!(list_row(160.0 / 640.0, 330.0 / 480.0, 7), None, "below the last row");
		assert_eq!(list_row(160.0 / 640.0, 87.0 / 480.0, 7), None, "the title bar");
	}

	#[test]
	fn the_list_reads_from_the_list_object() {
		let mut ram = vec![0u8; 0x200_0000];
		let w = |r: &mut Vec<u8>, a: u32, v: u32| r[a as usize..a as usize + 4].copy_from_slice(&v.to_le_bytes());
		let (mgr, table) = (0x1fb_8000u32, 0x28_9e78u32);
		let l = mgr + LIST_OFF;
		w(&mut ram, l + 0x314, table);
		w(&mut ram, l + 0x1bc, 3);
		for (k, id) in [0xba4u32, 0xba5, 0xba6, 0xb72, 0xba8, 0xba9, 0xbac].iter().enumerate() {
			w(&mut ram, table + 0x14 * k as u32, 0x3f80_0000);
			w(&mut ram, table + 0x14 * k as u32 + 4, *id);
		}
		assert_eq!(list_of(&ram, 8, 1, mgr), Some(List { cur: 3, n: 7 }));
		assert_eq!(list_of(&ram, 2, 1, mgr), None, "the root is a widget (pc_menu_hover.rs)");
		assert_eq!(list_of(&ram, 8, 1, 0), None, "no manager: the title");
		w(&mut ram, l + 0x1bc, 9);
		assert_eq!(list_of(&ram, 8, 1, mgr), None, "a cursor past the rows");
	}
}
