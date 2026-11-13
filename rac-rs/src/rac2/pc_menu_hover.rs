//! [PC] Mouse hover in R&C2's retail pause menu (parity row 48; R&C1 `retail/mouse_menu.rs`): the row under the free pointer
//! becomes the selected row, by D-pad pulses on the pad exactly like a hand (nothing is written to the game), closed-loop on the
//! game's own row cursor. A click (Cross from the window's Menu layer) that comes while the selection still moves is held back and
//! pressed once the hovered row is selected, so a click always confirms the row under the pointer.
//!
//! Facts used [C unless marked] (rac2/notes_menus.md item 1, notes_modern_screens.md "ROOT ROWS"):
//! - Current page id: modern_open.rs (menu globals block G found by code shape per level; [G+0x14] + 0xe4). (The notes_menus.md
//!   "lowest slot" scan is NOT safe: 0262 at field 60 has a stale id-4 record at 0x1ff48c below the live slot 0x1ff754 [C].)
//! - The pause root (page id 2) is a widget, not a label list: its 7 rows (Weapons Gadgets Quick Select Items Help Options
//!   Special) have their cursor at [menu manager] + 0x39160 + 0x4ac, 0..6; the manager word sits at mode word + 0x14c
//!   (0x1a90cc in levels 6 and 26 [C]; the same offset from the mode word elsewhere [I]). Down 0x4000 / Up 0x1000 move it by one.
//! - Row places on the picture (fractions of the retail picture, PCSX2 shot of 0262 and our window shot of 0110 agree): centre
//!   y = 0.219 + 0.084 k, buttons x 0.247..0.731.
use std::sync::Mutex;

const UP: u16 = 0x1000;
const DOWN: u16 = 0x4000;
const DPAD: u16 = 0xf000;
const CROSS: u16 = 0x40;
const ROWS: u8 = 7;
const ROOT_PAGE: u32 = 2;
const ROW0: f32 = 0.219;
const PITCH: f32 = 0.084;
const X: (f32, f32) = (0.247, 0.731);
/// Pulses tried for one hovered row before giving up (the cursor did not follow).
const BUDGET: u8 = 10;

/// The pointer over the retail picture (fractions, None = no pointer yet) and a change counter.
static POINTER: Mutex<(Option<(f32, f32)>, u32)> = Mutex::new((None, 0));

/// Window: the free pointer moved (fractions of the retail picture; outside 0..1 = off the picture).
pub fn pointer(fx: f32, fy: f32) {
	if let Ok(mut p) = POINTER.lock() {
		*p = (Some((fx, fy)), p.1.wrapping_add(1));
	}
}

/// The root row under (fx, fy), if any.
pub fn root_row(fx: f32, fy: f32) -> Option<u8> {
	if !(X.0..=X.1).contains(&fx) {
		return None;
	}
	let k = ((fy - ROW0) / PITCH).round();
	((0.0..ROWS as f32).contains(&k) && (fy - ROW0 - k * PITCH).abs() <= PITCH * 0.45).then_some(k as u8)
}

fn rd32(ram: &[u8], a: usize) -> Option<u32> {
	ram.get(a..a + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()))
}

/// The root's row cursor (0..6) from the mode word's address.
pub fn root_cursor(ram: &[u8], mode_at: usize) -> Option<u8> {
	let mgr = rd32(ram, mode_at + 0x14c)? as usize;
	if mgr == 0 || mgr >= 0x200_0000 {
		return None;
	}
	rd32(ram, mgr + 0x39160 + 0x4ac).filter(|&c| c < ROWS as u32).map(|c| c as u8)
}

/// Fields per steering pulse (2 pressed, the rest released). Debug tuning: RAC2_HOVER_PERIOD.
fn period() -> u8 {
	static P: std::sync::OnceLock<u8> = std::sync::OnceLock::new();
	*P.get_or_init(|| std::env::var("RAC2_HOVER_PERIOD").ok().and_then(|v| v.trim().parse().ok()).filter(|&p: &u8| p >= 4).unwrap_or(4))
}

#[derive(Default)]
pub struct Hover {
	seen: u32,
	target: Option<u8>,
	spent: u8,
	/// Fields into the current pulse (1..=2 pressed, 3..=4 released).
	pulse: u8,
	/// A click held back while the selection moves, then pressed for 2 fields.
	click: u8,
	script: Vec<(u64, u64, char, f32, f32)>,
	/// [b46 W7] The last pointer place seen, and whether the click bit was down the field before (a click is an edge).
	last: Option<(f32, f32)>,
	down: bool,
	/// [b47 X4] A click that began in a Classic page menu is still down.
	held: bool,
	grid: crate::pc_menu_mouse_grid::Grid,
}

impl Hover {
	/// One field (every mode; it acts in the page menu, mode 3, and in the Classic title's two lists). `page` = the current page id
	/// (modern_open::page_id_now), `own` = the mouse is read in this menu (pc_input.rs: a Classic page menu or title list), `click` = the
	/// left button is down (window state, NOT a pad bit: [b47 X4] build 46 carried it as pad bit 0x0004 = L1, so a controller's L1 and
	/// the Q key counted as a click and were eaten), `b` = the pad's buttons; returns them with the steering pulses.
	pub fn field(&mut self, ram: &[u8], mode_at: Option<usize>, mode: u32, page: Option<u32>, field: u64, live: bool, own: bool, click: bool, b: u16) -> u16 {
		// The modern pause / title page drives its own mouse (modern_screen.rs): never two drivers on one page.
		// [b47 fix] The Classic title's root and Options lists (pc_menu_mouse_title.rs) are decided FIRST: at the title the mode word reads 3
		// and the root's page id is 2 as the pause root's, so the pause reader took the title for its 7-row root and pulsed Down for ever
		// (tour b47_title_click, trace `row 0 -> 3`).
		let title = if own { crate::pc_menu_mouse_title::title(ram) } else { None };
		let root = title.is_none() && mode == 3 && page == Some(ROOT_PAGE) && crate::modern_screen::facts().is_none();
		// [b45 P4B] B25: the classic list pages (Options, Help) the same way (pc_menu_mouse.rs).
		let list = if title.is_none() && mode == 3 && !root && page.is_some_and(|p| crate::pc_menu_mouse::LIST_PAGES.contains(&p)) { crate::pc_menu_mouse::list(ram) } else { None };
		let cur = title.map(|t| t.cur).or(mode_at.filter(|_| root).and_then(|m| root_cursor(ram, m))).or(list.map(|l| l.cur));
		// [b47 X4] The click is an edge of the window's left button, only in a menu that reads it; the pad is never changed here.
		let bit = own && click;
		let press = bit && !self.down;
		self.held = bit && (press || self.held); // the hold of a click that began in the menu
		self.down = click;
		// [b46 W7] The Weapons grid: a 2-D walk of its own (pc_menu_mouse_grid.rs).
		let paged = title.is_none() && mode == 3 && !root;
		let weapons = paged && page == Some(crate::pc_menu_mouse_grid::PAGE_WEAPONS);
		let gadgets = paged && page == Some(crate::modern_grid::PAGE_GADGETS);
		let qs = paged && page == Some(crate::modern_qs::PAGE_QS);
		let items = paged && page == Some(crate::modern_items::PAGE_ITEMS); // [b47 X4]
		let grid = if items {
			crate::pc_menu_mouse_items::items(ram)
		} else if weapons {
			crate::pc_menu_mouse_grid::weapons(ram)
		} else if gadgets {
			crate::pc_menu_mouse_grid::gadgets(ram)
		} else if qs {
			crate::pc_menu_mouse_grid::qs(ram)
		} else if paged && matches!(page, Some(30 | 31)) {
			crate::pc_menu_mouse_grid::slots(ram)
		} else {
			None
		};
		if let Some(gcur) = grid {
			let (cell, step): (fn(f32, f32) -> Option<(u8, u8)>, fn((u8, u8), (u8, u8)) -> u16) = if items {
				(crate::pc_menu_mouse_items::cell, crate::pc_menu_mouse_items::press)
			} else if weapons {
				(crate::pc_menu_mouse_grid::weapons_cell, crate::pc_menu_mouse_grid::press_cells)
			} else if qs {
				(crate::pc_menu_mouse_grid::qs_cell, crate::pc_menu_mouse_grid::press_cells)
			} else if matches!(page, Some(30 | 31)) {
				(crate::pc_menu_mouse_grid::slot_cell, crate::pc_menu_mouse_grid::press_cells)
			} else {
				(crate::pc_menu_mouse_grid::gadget_cell, crate::pc_menu_mouse_grid::press_gadgets)
			};
			let moved = self.moved(field, live);
			self.last = moved.or(self.last);
			let on = self.last.and_then(|(x, y)| cell(x, y));
			(self.target, self.pulse, self.click) = (None, 0, 0);
			return self.grid.field(gcur, moved.and_then(|(x, y)| cell(x, y)), on, press, b, step);
		}
		self.grid.reset();
		let Some(cur) = cur else {
			(self.target, self.pulse, self.click) = (None, 0, 0);
			if let Some(p) = self.moved(field, live) {
				self.last = Some(p);
			}
			// [b46 U6] Item 2: a page with no reader (title root / Options, Yes / No boxes, Items, Controls ...) has no rows to match the
			// pointer to: the click is a Cross there, as in build 45 (nothing is worse than before). The pages with readers decide above.
			return if own && self.held { b | CROSS } else { b };
		};
		let row_at = |(x, y): (f32, f32)| match (title, list) {
			(Some(t), _) => crate::pc_menu_mouse_title::row(&t, x, y),
			(None, Some(l)) => crate::pc_menu_mouse::list_row(x, y, l.n),
			(None, None) => root_row(x, y),
		};
		if let Some(p) = self.moved(field, live) {
			self.last = Some(p);
			if let Some(r) = row_at(p) {
				if self.target != Some(r) {
					(self.target, self.spent) = (Some(r), 0);
				}
			}
		}
		let on_row = self.last.and_then(row_at);
		// [b45 P4B] B26: a target dropped without the cursor's arrival (a D-pad press, the pulse budget spent) drops the held click
		// too: a click only ever confirms the row under the pointer (R&C2's vendor walk drops its buy the same way).
		if b & DPAD != 0 {
			(self.target, self.click) = (None, 0);
		} else if press && self.target.is_none() {
			// [b46 R2] A click on another row with a still pointer (the keys moved the cursor away since the hover): walk there, then confirm.
			if let Some(r) = on_row.filter(|&r| r != cur) {
				(self.target, self.spent) = (Some(r), 0);
			}
		}
		let mut b = b;
		if self.pulse == 0 && self.target.is_some_and(|t| t == cur || self.spent >= BUDGET) {
			if self.target != Some(cur) {
				self.click = 0;
			}
			self.target = None;
		}
		if let Some(t) = self.target {
			if press {
				// [b46 W7] only a click that is on the row being walked to is held back for the arrival
				self.click = if on_row == Some(t) { 2 } else { 0 };
			}
			self.pulse += 1;
			if self.pulse == 1 {
				self.spent += 1;
				if std::env::var_os("RAC2_HOVER_TRACE").is_some() {
					eprintln!("pc_menu_hover: field {field} row {cur} -> {t} pulse {}", self.spent);
				}
			}
			// The cursor can arrive on the pulse's first field: then the second presses nothing (an Up there undid the step).
			if self.pulse <= 2 && t != cur {
				b |= if t > cur { DOWN } else { UP };
			}
			if self.pulse >= period() {
				self.pulse = 0;
			}
			return b;
		}
		if press && on_row == Some(cur) {
			self.click = 2; // [b46 W7] a click on the row retail already has selected
		}
		if self.click > 0 {
			self.click -= 1;
			b |= CROSS;
		}
		b
	}

	/// [b48 Y4] The last pointer place seen (fractions of the picture).
	pub(crate) fn last(&self) -> Option<(f32, f32)> {
		self.last
	}

	/// The pointer if it moved since the last field (window, or RAC2_MOUSE_SCRIPT `FROM-TO:p:FX,FY`).
	pub(crate) fn moved(&mut self, field: u64, live: bool) -> Option<(f32, f32)> {
		if self.script.is_empty() {
			self.script = std::env::var("RAC2_MOUSE_SCRIPT").map(|v| crate::pc_input::parse_script(&v)).unwrap_or_default();
			self.script.retain(|e| e.2 == 'p');
			if self.script.is_empty() {
				self.script.push((u64::MAX, u64::MAX, 'x', 0.0, 0.0));
			}
		}
		if let Some(e) = self.script.iter().find(|e| e.2 == 'p' && (e.0..=e.1).contains(&field)) {
			return Some((e.3, e.4));
		}
		// The window's pointer only drives a live run: a replay / scripted pad is driven by the recording alone.
		if !live {
			return None;
		}
		let (p, n) = POINTER.lock().map(|p| *p).unwrap_or((None, 0));
		if n == self.seen {
			return None;
		}
		self.seen = n;
		p
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn rows_and_steering() {
		assert_eq!(root_row(0.5, 0.219), Some(0));
		assert_eq!(root_row(0.5, 0.219 + 0.084 * 6.0), Some(6));
		assert_eq!(root_row(0.1, 0.3), None);
		assert_eq!(root_row(0.5, 0.1), None);
		// A RAM image: mode word at 0x1a8f80, manager 0x1fb8000, cursor 1 (the root page id is passed in).
		let mut ram = vec![0u8; 0x200_0000];
		let w = |r: &mut Vec<u8>, a: usize, v: u32| r[a..a + 4].copy_from_slice(&v.to_le_bytes());
		w(&mut ram, 0x1a8f80 + 0x14c, 0x1fb_8000);
		w(&mut ram, 0x1fb_8000 + 0x39160 + 0x4ac, 1);
		let mut h = Hover::default();
		pointer(0.5, 0.219 + 0.084 * 3.0);
		// Row 3 hovered, cursor 1: Down pulses; a click meanwhile is held back.
		assert_eq!(h.field(&ram, Some(0x1a8f80), 3, Some(2), 1, true, true, true, 0), DOWN);
		assert_eq!(h.field(&ram, Some(0x1a8f80), 3, Some(2), 2, true, true, false, 0), DOWN);
		assert_eq!(h.field(&ram, Some(0x1a8f80), 3, Some(2), 3, true, true, false, 0), 0);
		assert_eq!(h.field(&ram, Some(0x1a8f80), 3, Some(2), 4, true, true, false, 0), 0);
		w(&mut ram, 0x1fb_8000 + 0x39160 + 0x4ac, 3);
		assert_eq!(h.field(&ram, Some(0x1a8f80), 3, Some(2), 5, true, true, false, 0), CROSS);
		assert_eq!(h.field(&ram, Some(0x1a8f80), 3, Some(2), 6, true, true, false, 0), CROSS);
		assert_eq!(h.field(&ram, Some(0x1a8f80), 3, Some(2), 7, true, true, false, 0), 0);
		// Not in the menu: nothing added.
		assert_eq!(h.field(&ram, Some(0x1a8f80), 0, None, 8, true, false, false, 0), 0);
		// The cursor arrives on a pulse's first field (0262 trace: an Up on the second field undid the step): nothing pressed.
		let mut h = Hover::default();
		pointer(0.5, 0.219 + 0.084 * 4.0);
		assert_eq!(h.field(&ram, Some(0x1a8f80), 3, Some(2), 10, true, true, false, 0), DOWN);
		w(&mut ram, 0x1fb_8000 + 0x39160 + 0x4ac, 4);
		assert_eq!(h.field(&ram, Some(0x1a8f80), 3, Some(2), 11, true, true, false, 0), 0);
		assert_eq!(h.field(&ram, Some(0x1a8f80), 3, Some(2), 12, true, true, false, 0), 0);
		// A replay (not live) ignores the window's pointer: the recording alone drives it.
		let mut r = Hover::default();
		pointer(0.5, 0.219 + 0.084 * 5.0);
		assert_eq!(r.field(&ram, Some(0x1a8f80), 3, Some(2), 20, false, true, false, 0), 0);
	}

	/// [b45 P4B] B26: a click held during a walk that gives up (budget spent, the cursor never arrives) is dropped, never pressed.
	#[test]
	fn a_click_is_dropped_when_the_walk_gives_up() {
		let mut ram = vec![0u8; 0x200_0000];
		let w = |r: &mut Vec<u8>, a: usize, v: u32| r[a..a + 4].copy_from_slice(&v.to_le_bytes());
		w(&mut ram, 0x1a8f80 + 0x14c, 0x1fb_8000);
		w(&mut ram, 0x1fb_8000 + 0x39160 + 0x4ac, 0);
		let mut h = Hover { script: vec![(0, 0, 'x', 0.0, 0.0)], ..Hover::default() };
		h.target = Some(5);
		h.last = Some((0.5, ROW0 + PITCH * 5.0));
		let mut out = Vec::new();
		for f in 0..(BUDGET as u64 * 4 + 8) {
			out.push(h.field(&ram, Some(0x1a8f80), 3, Some(2), f, false, true, f == 0, 0));
		}
		assert!(out.iter().all(|&o| o & CROSS == 0), "no blind Cross after the give-up: {out:x?}");
		// A D-pad press mid-walk drops the click too.
		let mut h = Hover { script: vec![(0, 0, 'x', 0.0, 0.0)], ..Hover::default() };
		h.target = Some(5);
		h.last = Some((0.5, ROW0 + PITCH * 5.0));
		h.field(&ram, Some(0x1a8f80), 3, Some(2), 0, false, true, true, 0);
		h.field(&ram, Some(0x1a8f80), 3, Some(2), 1, false, true, false, UP);
		assert!((2..12).all(|f| h.field(&ram, Some(0x1a8f80), 3, Some(2), f, false, true, false, 0) & CROSS == 0));
	}

	/// [b46 W7] A click off every row does nothing; on the selected row it is one Cross; a Cross from Space / Enter is untouched.
	#[test]
	fn a_click_off_every_row_is_nothing() {
		let mut ram = vec![0u8; 0x200_0000];
		let w = |r: &mut Vec<u8>, a: usize, v: u32| r[a..a + 4].copy_from_slice(&v.to_le_bytes());
		w(&mut ram, 0x1a8f80 + 0x14c, 0x1fb_8000);
		w(&mut ram, 0x1fb_8000 + 0x39160 + 0x4ac, 1);
		let at = |x: f32, y: f32| Hover { script: vec![(0, 100, 'p', x, y)], ..Hover::default() };
		let mut h = at(0.1, 0.3);
		let out: Vec<u16> = (0..6).map(|f| h.field(&ram, Some(0x1a8f80), 3, Some(2), f, false, true, f == 1, 0)).collect();
		assert!(out.iter().all(|&o| o == 0), "off the rows: {out:x?}");
		let mut h = at(0.5, 0.219 + 0.084);
		let out: Vec<u16> = (0..6).map(|f| h.field(&ram, Some(0x1a8f80), 3, Some(2), f, false, true, f == 1, 0)).collect();
		assert_eq!(out, vec![0, CROSS, CROSS, 0, 0, 0], "on the selected row");
		let mut h = at(0.1, 0.3);
		assert_eq!(h.field(&ram, Some(0x1a8f80), 3, Some(2), 0, false, true, false, CROSS), CROSS, "Space / Enter stay a Cross");
		// [b46 U6] A page with no reader: the click bit never reaches the game; it is a Cross there (build 45's behaviour).
		assert_eq!(h.field(&ram, Some(0x1a8f80), 3, Some(31), 1, false, true, true, 0), CROSS);
		assert_eq!(h.field(&ram, Some(0x1a8f80), 3, Some(31), 2, false, true, true, 0), CROSS, "held click");
		assert_eq!(h.field(&ram, Some(0x1a8f80), 3, Some(31), 3, false, true, false, 0), 0);
	}

	/// [b47 X4] The click is the window's left button, never a pad bit: a controller's L1 (0x0004) is the game's in every menu (Q / L1
	/// switch tabs) and is no click; a button held into the menu is no click; one click is one Cross on the row under the pointer.
	#[test]
	fn l1_is_the_games_in_every_menu_and_the_click_is_the_buttons() {
		const L1: u16 = 0x0004;
		let mut ram = vec![0u8; 0x200_0000];
		let w = |r: &mut Vec<u8>, a: usize, v: u32| r[a..a + 4].copy_from_slice(&v.to_le_bytes());
		w(&mut ram, 0x1a8f80 + 0x14c, 0x1fb_8000);
		w(&mut ram, 0x1fb_8000 + 0x39160 + 0x4ac, 1);
		let on_row = || Hover { script: vec![(0, 100, 'p', 0.5, 0.219 + 0.084)], ..Hover::default() };
		// L1 held or tapped: passes untouched in play, in a Modern page menu and in a Classic one; it never clicks the row.
		let mut h = on_row();
		assert!((0..5).all(|f| h.field(&ram, Some(0x1a8f80), 0, None, f, true, false, false, L1 | CROSS) == L1 | CROSS), "play");
		let out: Vec<u16> = (5..12).map(|f| h.field(&ram, Some(0x1a8f80), 3, Some(2), f, true, true, false, L1)).collect();
		assert!(out.iter().all(|&o| o == L1), "classic menu, L1 only: {out:x?}");
		// The mouse button held while the Classic pause opens is no click.
		let mut h = on_row();
		h.field(&ram, Some(0x1a8f80), 0, None, 0, false, false, true, 0);
		let out: Vec<u16> = (1..6).map(|f| h.field(&ram, Some(0x1a8f80), 3, Some(2), f, false, true, true, 0)).collect();
		assert!(out.iter().all(|&o| o == 0), "held into the menu: {out:x?}");
		// A real click with L1 held at the same time: the Cross is added, L1 stays.
		let mut h = on_row();
		assert_eq!(h.field(&ram, Some(0x1a8f80), 3, Some(2), 0, false, true, true, L1), L1 | CROSS);
	}
}
