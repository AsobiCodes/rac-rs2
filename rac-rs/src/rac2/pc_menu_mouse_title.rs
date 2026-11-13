//! [PC] [b47 X4] The mouse on the Classic title's two lists (sweep B25; R&C1 `retail/mouse_menu.rs`): the root (New Game, Load, Options)
//! and the Options list (Language, 16:9, Adjust screen, Vibration). Hover walks retail's own row cursor by Up / Down pulses and a click
//! presses Cross once it is on the row under the pointer, exactly as the pause root (`pc_menu_hover.rs`, which calls this); a click on
//! no row is nothing. Read only. The title has no mode word and no menu manager (the pause readers need both), so the lists are found
//! by their page records: this file only says WHICH list is up and where its rows are.
//!
//! Facts [C] (notes_menus.md "TITLE LISTS", notes_modern_screens.md "TITLE FOUND"; places MEASURED on the Classic picture
//! `b47/x4/runs/title/shots/w00600.png` (root) and `w00700.png` (Options), 1024 x 768 window, the 4:3 retail picture filling it):
//! - root: page record 0x259458 (id 2, parent 0), title state word 0x1f2840 = 4, no manager; cursor 0x1ff6ae8 (0..2, wraps); plates
//!   x 303..704 px, centres y = 240 + 102.5 k px, 66 px high.
//! - Options: page id 8, no manager, parent record id 2; cursor 0x1ff69f8, row table pointer 0x1ff6a00 (4 rows); plates with their icons
//!   x 62..540 px, centres y = 196 + 61.3 k px, 52 px high.
use crate::modern_screen::{self as scr, Kind};

const ROOT_ROWS: u8 = 3;
const ROOT_X: (f32, f32) = (310.0 / 1024.0, 697.0 / 1024.0);
const ROOT_Y0: f32 = 240.0 / 768.0;
const ROOT_PITCH: f32 = 102.5 / 768.0;
const ROOT_HALF: f32 = 30.0 / 768.0;
const OPT_X: (f32, f32) = (62.0 / 1024.0, 540.0 / 1024.0);
const OPT_Y0: f32 = 196.0 / 768.0;
const OPT_PITCH: f32 = 61.3 / 768.0;
const OPT_HALF: f32 = 26.0 / 768.0;

/// A Classic title list up this field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Title {
	/// retail's row cursor and the row count
	pub cur: u8,
	pub n: u8,
	/// the Options list (else the root)
	pub opts: bool,
}

fn rd(ram: &[u8], a: u32) -> u32 {
	let a = (a & 0x1ff_fffc) as usize;
	ram.get(a..a + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The title list of these words: the current page's id, its parent record's id and the parent's parent, the manager word, the title
/// state word, the root cursor, the Options cursor and its row count. Pure.
pub fn title_of(page_id: u32, parent_id: Option<u32>, parent: u32, manager: u32, state: u32, root_cur: u32, opt_cur: u32, opt_rows: usize) -> Option<Title> {
	if state != 4 || manager != 0 {
		return None;
	}
	if scr::gate(page_id, parent, manager, state) == Some(Kind::Title) {
		return (root_cur < ROOT_ROWS as u32).then_some(Title { cur: root_cur as u8, n: ROOT_ROWS, opts: false });
	}
	(page_id == 8 && parent_id == Some(2) && (opt_cur as usize) < opt_rows).then_some(Title { cur: opt_cur as u8, n: opt_rows as u8, opts: true })
}

/// The Classic title list of this field, None anywhere else (the game, the pause menu, the title's other pages).
pub fn title(ram: &[u8]) -> Option<Title> {
	if rd(ram, scr::TITLE_STATE) != 4 {
		return None;
	}
	let s = crate::modern_open::snap_now(ram)?;
	let parent = rd(ram, s.page + 0xe0);
	let parent_id = (parent != 0).then(|| rd(ram, parent + 0xe4));
	let rows = if s.page_id == 8 { crate::modern_optlist::row_ids(ram, rd(ram, crate::modern_optlist::TITLE_LIST_TABLE)).map_or(0, |v| v.len()) } else { 0 };
	title_of(s.page_id, parent_id, parent, scr::manager(ram, &s), rd(ram, scr::TITLE_STATE), rd(ram, scr::TITLE_CURSOR), rd(ram, crate::modern_optlist::TITLE_LIST_CURSOR), rows)
}

/// The row of list `t` under (fx, fy) (fractions of the picture), if any. Pure.
pub fn row(t: &Title, fx: f32, fy: f32) -> Option<u8> {
	let (x, y0, pitch, half) = if t.opts { (OPT_X, OPT_Y0, OPT_PITCH, OPT_HALF) } else { (ROOT_X, ROOT_Y0, ROOT_PITCH, ROOT_HALF) };
	if !(x.0..=x.1).contains(&fx) {
		return None;
	}
	let k = ((fy - y0) / pitch).round();
	((0.0..t.n as f32).contains(&k) && (fy - y0 - k * pitch).abs() <= half).then_some(k as u8)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn which_list_is_up() {
		// root: page 2, parent 0, no manager, state 4
		assert_eq!(title_of(2, None, 0, 0, 4, 1, 0, 0), Some(Title { cur: 1, n: 3, opts: false }));
		assert_eq!(title_of(2, None, 0, 0x1fb_8000, 4, 1, 0, 0), None, "the pause root has a manager");
		assert_eq!(title_of(2, None, 0, 0, 2, 1, 0, 0), None, "the logos");
		assert_eq!(title_of(2, None, 0, 0, 4, 7, 0, 0), None, "a cursor that is not a row");
		// Options: page 8 under the root, 4 rows
		assert_eq!(title_of(8, Some(2), 0x259458, 0, 4, 2, 3, 4), Some(Title { cur: 3, n: 4, opts: true }));
		assert_eq!(title_of(8, Some(8), 0x259588, 0, 4, 2, 3, 4), None, "a sub-page of the list");
		assert_eq!(title_of(8, Some(2), 0x259458, 0, 4, 2, 0, 0), None, "no row table");
		assert_eq!(title_of(9, Some(2), 0x259458, 0, 4, 2, 0, 4), None, "another page (Language, Load ...)");
	}

	#[test]
	fn rows_on_the_measured_pictures() {
		let root = Title { cur: 0, n: 3, opts: false };
		assert_eq!(row(&root, 500.0 / 1024.0, 240.0 / 768.0), Some(0));
		assert_eq!(row(&root, 500.0 / 1024.0, 445.0 / 768.0), Some(2));
		assert_eq!(row(&root, 500.0 / 1024.0, 292.0 / 768.0), None, "the gap between plates");
		assert_eq!(row(&root, 500.0 / 1024.0, 550.0 / 768.0), None, "below Options");
		assert_eq!(row(&root, 200.0 / 1024.0, 343.0 / 768.0), None, "left of the plates");
		let opts = Title { cur: 0, n: 4, opts: true };
		assert_eq!(row(&opts, 280.0 / 1024.0, 196.0 / 768.0), Some(0));
		assert_eq!(row(&opts, 500.0 / 1024.0, 380.0 / 768.0), Some(3), "the icon of the last row");
		assert_eq!(row(&opts, 700.0 / 1024.0, 250.0 / 768.0), None, "the picture panel");
		assert_eq!(row(&opts, 280.0 / 1024.0, 440.0 / 768.0), None, "below the last row");
	}
}
