//! [PC] [b50 V3] The mouse on the planet list of the Classic Galactic Map (page 0xf, Menu style Classic). Hovering a row walks retail's
//! own cursor (Up / Down pulses, `modern_map::Pulse`) onto that planet; a click on a row does the same and NEVER travels (the Select
//! plate does, `pc_menu_mouse_box::map_answer`). Read only: the pad is all this touches, like every other Classic mouse reader.
//!
//! Where the rows are [C measured, 1024 x 768 Classic picture, `b48/y4/runs/map2/shots/w00170.png`]: the list shows 4 planets in
//! the box x 62..440, y 175..411; one planet is a block of two lines (location above, name below) 59 px high; the list scrolls
//! smoothly (a float offset mid-animation, which is why no RAM word matched the first visible entry in Z2's scans: b49 handoff Z2).
//! The FIRST VISIBLE ENTRY is therefore tracked, not read [I]: retail moves the window only when the cursor leaves it (Up shows the
//! selected planet on the top row, Down on the bottom, measured on state 0349), and a map that opens on a planet shows it at the
//! top, or the last 4 when the list ends first (the one opening measured: entry 7 of 10 -> first visible 6). If the first opening
//! differs, the estimate is right again after the first move that reaches the top or bottom row. A wrong estimate only walks the
//! highlight to a neighbour of the row the player meant (visible, and nothing travels), never to a planet by surprise.

use crate::modern_map::{Ask, Pulse};
use crate::modern_open::MENU_BTNS;

/// Rows visible at once, the top of the first row (px), the height of one planet block (px), the list box's x range (px).
const ROWS: usize = 4;
const TOP: f32 = 175.0;
const PITCH: f32 = 59.0;
const X: (f32, f32) = (62.0 / 1024.0, 440.0 / 1024.0);
/// The pointer counts as moved after this much (fraction of the picture width).
const MOVED: f32 = 2.0 / 1024.0;

/// The visible row (0..ROWS) under the pointer, None outside the list box.
pub fn row_at(fx: f32, fy: f32) -> Option<usize> {
	let y = fy * 768.0 - TOP;
	if !(X.0..=X.1).contains(&fx) || !(0.0..PITCH * ROWS as f32).contains(&y) {
		return None;
	}
	Some((y / PITCH) as usize)
}

/// The first visible entry after the cursor moved to `cur` (`start` = before; None = the map just opened) over `n` entries.
pub fn window_start(start: Option<usize>, cur: usize, n: usize) -> usize {
	let last = n.saturating_sub(ROWS);
	match start {
		None => cur.min(last),
		Some(s) if cur < s => cur,
		Some(s) if cur >= s + ROWS => cur + 1 - ROWS,
		Some(s) => s,
	}
	.min(last)
}

#[derive(Default)]
pub struct MapList {
	start: Option<usize>,
	at: Option<(f32, f32)>,
	down: bool,
	pulse: Pulse,
}

impl MapList {
	/// One field. `map` = (the cursor's entry, the entry count) while the Classic map page is current, None otherwise; `at` = the
	/// pointer (fractions of the picture); `click` = the left button; `b` = the pad. Returns the pad with the walk's pulses.
	pub fn field(&mut self, map: Option<(usize, usize)>, at: Option<(f32, f32)>, click: bool, b: u16) -> u16 {
		let edge = click && !self.down;
		self.down = click;
		let Some((cur, n)) = map.filter(|&(c, n)| c < n) else {
			*self = MapList::default();
			return b;
		};
		self.start = Some(window_start(self.start, cur, n));
		if b & MENU_BTNS != 0 {
			self.pulse = Pulse::default(); // the player's own keys end the walk
		}
		let moved = match (at, self.at) {
			(Some(a), Some(p)) => (a.0 - p.0).abs() > MOVED || (a.1 - p.1).abs() > MOVED,
			(Some(_), None) => true,
			_ => false,
		};
		if moved || edge {
			self.at = at;
			let target = at.and_then(|(x, y)| row_at(x, y)).map(|r| self.start.unwrap_or(0) + r).filter(|&t| t < n);
			if let Some(t) = target.filter(|&t| t != cur || self.pulse.busy()) {
				self.pulse.ask(Ask::Row(t, false));
			}
		}
		if self.pulse.busy() { (b & !MENU_BTNS) | self.pulse.step(cur, n) } else { b }
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn rows_by_the_measured_places() {
		assert_eq!(row_at(200.0 / 1024.0, 204.0 / 768.0), Some(0), "Barlow's block (name at y 215)");
		assert_eq!(row_at(200.0 / 1024.0, 393.0 / 768.0), Some(3), "Ship Shack (name at y 393)");
		assert_eq!(row_at(200.0 / 1024.0, 430.0 / 768.0), None, "below the 4 rows");
		assert_eq!(row_at(200.0 / 1024.0, 520.0 / 768.0), None, "the Select plate");
		assert_eq!(row_at(600.0 / 1024.0, 300.0 / 768.0), None, "the star map");
	}

	#[test]
	fn the_window_moves_only_when_the_cursor_leaves_it() {
		assert_eq!(window_start(None, 7, 10), 6, "the measured opening: entry 7 of 10");
		assert_eq!(window_start(None, 1, 10), 1);
		assert_eq!(window_start(Some(6), 8, 10), 6);
		assert_eq!(window_start(Some(6), 5, 10), 5, "Up past the top row: the selected on the top");
		assert_eq!(window_start(Some(2), 6, 10), 3, "Down past the bottom row: the selected on the bottom");
		assert_eq!(window_start(None, 0, 3), 0, "a short list");
	}

	/// Hover on the 3rd visible row starts a walk to entry 8 (cursor on 6, window start 6); a click never presses Cross; a row that
	/// already holds the cursor, no pointer and no map page start nothing.
	#[test]
	fn hover_walks_the_cursor_and_a_click_never_travels() {
		let at = Some((200.0 / 1024.0, 330.0 / 768.0)); // visible row 2 -> entry 8
		let mut m = MapList::default();
		let mut downs = 0;
		for f in 0..12 {
			let b = m.field(Some((6, 10)), at, f == 5, 0);
			assert_eq!(b & 0x40, 0, "never a Cross");
			downs += (b & crate::modern_screen::DOWN != 0) as u32;
		}
		assert!(m.pulse.busy() && downs > 0, "walking Down toward entry 8: {downs}");
		let mut m = MapList::default();
		let top = Some((200.0 / 1024.0, 204.0 / 768.0)); // visible row 0 = entry 6 = the cursor
		assert_eq!(m.field(Some((6, 10)), top, true, 0), 0);
		assert!(!m.pulse.busy());
		assert_eq!(m.field(Some((6, 10)), None, false, 0), 0, "no pointer: nothing");
		assert_eq!(m.field(None, at, true, 0), 0, "no map page: nothing");
	}

	#[test]
	fn a_key_press_ends_the_walk() {
		let mut m = MapList::default();
		let at = Some((200.0 / 1024.0, 380.0 / 768.0));
		m.field(Some((6, 10)), at, false, 0);
		assert!(m.pulse.busy());
		let key = crate::modern_map::TRIANGLE_B;
		m.field(Some((6, 10)), at, false, key);
		assert!(!m.pulse.busy());
	}
}
