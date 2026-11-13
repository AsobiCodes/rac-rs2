//! [PC] [b48 Y4] The mouse on retail's Sound page (pause Options > Sound, page id 0x20) in Menu style Classic. The page has three
//! rows (sound effects bar, music bar, "Toggle stereo/mono") with ONE cursor [S + 0x2d8] that Up (+2 mod 3) and Down (+1 mod 3) move;
//! Left / Right change the selected bar by 3 per held field (held word 0x1383c0), Cross on row 2 toggles. So, read only, like a hand:
//! hovering a row walks the cursor there by Up / Down pulses; a click on a bar holds Left or Right until the value reaches the clicked
//! place (dragging with the button down follows the pointer); a click on the toggle row presses one Cross once the cursor is there; a
//! click anywhere else does nothing. A full sweep takes about 6 s: retail's own speed, nothing is written to the game.
//! Facts [C]: modern_optlist_sound.rs (words, values 0..=0x400) + the Classic picture of the run `b48/y4/runs/snd/shots/w00300.png`
//! (1024 x 768, level of state 0357): row centres y 293 / 351 / 396 px, bars x 530..830 px (full = 0x400, checked: 0x233 = 55 % fills
//! to x 695, a 20-field Left hold from 0x400 = 0x3c1), the toggle text x 283..748.
const DOWN: u16 = 0x4000;
const UP: u16 = 0x1000;
const LEFT: u16 = 0x8000;
const RIGHT: u16 = 0x2000;
const CROSS: u16 = 0x40;
const DPAD: u16 = 0xf000;
pub const PAGE: u32 = 0x20;
const MAX: u32 = 0x400;
/// Pulses tried for one wanted row before giving up.
const BUDGET: u8 = 8;
const BAR_X: (f32, f32) = (530.0 / 1024.0, 830.0 / 1024.0);
const ROW_X: (f32, f32) = (150.0 / 1024.0, 840.0 / 1024.0);
const TOGGLE_X: (f32, f32) = (275.0 / 1024.0, 756.0 / 1024.0);
const ROW_Y: [f32; 3] = [293.0 / 768.0, 351.0 / 768.0, 396.0 / 768.0];
const ROW_HALF: [f32; 3] = [27.0 / 768.0, 27.0 / 768.0, 18.0 / 768.0];

/// The row (0 sound effects, 1 music, 2 toggle) under (fx, fy). Pure.
pub fn row(fx: f32, fy: f32) -> Option<u8> {
	let k = (0..3).find(|&k| (fy - ROW_Y[k]).abs() <= ROW_HALF[k])?;
	let x = if k == 2 { TOGGLE_X } else { ROW_X };
	(x.0..=x.1).contains(&fx).then_some(k as u8)
}

/// The value a pointer at fx on a bar means (0..=0x400). Pure.
pub fn value_at(fx: f32) -> u32 {
	(((fx - BAR_X.0) / (BAR_X.1 - BAR_X.0)).clamp(0.0, 1.0) * MAX as f32).round() as u32
}

/// True when the pointer is on a bar's span (a click there sets a value; a click on the label only selects the row).
pub fn on_bar(fx: f32) -> bool {
	(BAR_X.0 - 0.01..=BAR_X.1 + 0.01).contains(&fx)
}

#[derive(Default)]
pub struct SoundMouse {
	down: bool,
	prev: Option<(f32, f32)>,
	/// the row the pointer asked for (hover) and the value goal of a bar click (row, value)
	tgt: Option<u8>,
	goal: Option<(u8, u32)>,
	toggle: bool,
	pulse: u8,
	spent: u8,
	cross: u8,
}

impl SoundMouse {
	/// One field. `s` = the page's words (sfx, music, stereo, cursor) when the Classic Sound page is up (modern_optlist_sound::read),
	/// `at` = the pointer's last place, `click` = the left button, `b` = the pad.
	pub fn field(&mut self, s: Option<(u32, u32, bool, usize)>, at: Option<(f32, f32)>, click: bool, b: u16) -> u16 {
		let edge = click && !self.down;
		self.down = click;
		let moved = at != self.prev;
		self.prev = at;
		let Some((sfx, music, _, cur)) = s else {
			*self = SoundMouse { down: click, prev: at, ..SoundMouse::default() };
			return b;
		};
		let cur = cur as u8;
		if b & DPAD != 0 {
			(self.tgt, self.goal, self.toggle, self.pulse, self.spent) = (None, None, false, 0, 0);
			return b;
		}
		let over = at.and_then(|(x, y)| row(x, y));
		if moved || edge {
			if let Some(r) = over {
				self.tgt = Some(r);
			}
		}
		if edge {
			match (over, at) {
				(Some(r @ 0..=1), Some((x, _))) if on_bar(x) => self.goal = Some((r, value_at(x))),
				(Some(2), _) => self.toggle = true,
				_ => {}
			}
		} else if click && moved {
			if let (Some((r, _)), Some((x, _))) = (self.goal, at) {
				self.goal = Some((r, value_at(x))); // a drag follows the pointer
			}
		}
		let want = self.goal.map(|g| g.0).or(self.toggle.then_some(2)).or(self.tgt);
		let mut b = b;
		if let Some(w) = want.filter(|&w| w != cur) {
			self.pulse += 1;
			if self.pulse == 1 {
				self.spent += 1;
			}
			if self.spent > BUDGET {
				(self.tgt, self.goal, self.toggle, self.pulse, self.spent) = (None, None, false, 0, 0);
				return b;
			}
			if self.pulse <= 2 {
				b |= if (cur + 1) % 3 == w { DOWN } else { UP };
			}
			if self.pulse >= 4 {
				self.pulse = 0;
			}
			return b;
		}
		(self.pulse, self.spent) = (0, 0);
		if want == Some(cur) {
			self.tgt = None;
		}
		if let Some((r, g)) = self.goal {
			let v = if r == 0 { sfx } else { music };
			if v > g + 2 {
				b |= LEFT;
			} else if v + 2 < g {
				b |= RIGHT;
			} else if !click {
				self.goal = None;
			}
		}
		if self.toggle && cur == 2 {
			(self.toggle, self.cross) = (false, 2);
		}
		if self.cross > 0 {
			self.cross -= 1;
			b |= CROSS;
		}
		b
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn px(x: f32, y: f32) -> Option<(f32, f32)> {
		Some((x / 1024.0, y / 768.0))
	}

	#[test]
	fn places_and_values() {
		assert_eq!(row(700.0 / 1024.0, 293.0 / 768.0), Some(0));
		assert_eq!(row(200.0 / 1024.0, 351.0 / 768.0), Some(1));
		assert_eq!(row(500.0 / 1024.0, 396.0 / 768.0), Some(2));
		assert_eq!(row(500.0 / 1024.0, 322.0 / 768.0), None, "between the rows");
		assert_eq!(row(60.0 / 1024.0, 293.0 / 768.0), None, "left of the panel text");
		assert_eq!(row(850.0 / 1024.0, 396.0 / 768.0), None);
		assert_eq!(value_at(530.0 / 1024.0), 0);
		assert_eq!(value_at(830.0 / 1024.0), MAX);
		assert!((value_at(695.0 / 1024.0) as i32 - 0x233).abs() <= 8, "the 55 % bar of the find-out run");
		assert!(on_bar(700.0 / 1024.0) && !on_bar(300.0 / 1024.0));
	}

	#[test]
	fn hover_walks_the_cursor_with_pulses() {
		let mut m = SoundMouse::default();
		// cursor 0, pointer on the toggle row: Up (+2 mod 3 = row 2) is one press
		assert_eq!(m.field(Some((0x400, 0x233, true, 0)), px(500.0, 396.0), false, 0), UP);
		assert_eq!(m.field(Some((0x400, 0x233, true, 2)), px(500.0, 396.0), false, 0), 0, "arrived: nothing pressed");
		// cursor 2, wanted row 0: Down wraps
		let mut m = SoundMouse::default();
		assert_eq!(m.field(Some((0x400, 0x233, true, 2)), px(700.0, 293.0), false, 0), DOWN);
	}

	#[test]
	fn a_bar_click_holds_left_or_right_until_the_place() {
		let mut m = SoundMouse::default();
		// music bar, cursor already 1, value 0x233, click at x 530 + 300 * 0.25
		let at = px(605.0, 351.0);
		let st = |v| Some((0x400, v, true, 1));
		assert_eq!(m.field(st(0x233), at, true, 0), LEFT);
		assert_eq!(m.field(st(0x230), at, true, 0), LEFT);
		assert_eq!(m.field(st(0x102), at, false, 0), 0, "within 2 of the goal 0x100: done");
		assert_eq!(m.field(st(0x102), at, false, 0), 0);
		// a click higher than the value holds Right
		let mut m = SoundMouse::default();
		assert_eq!(m.field(Some((0x400, 0x10, true, 1)), px(800.0, 351.0), true, 0), RIGHT);
		// the keys win: a D-pad press drops the goal
		assert_eq!(m.field(Some((0x400, 0x10, true, 1)), px(800.0, 351.0), false, UP), UP);
		assert_eq!(m.field(Some((0x400, 0x10, true, 1)), px(800.0, 351.0), false, 0), 0);
	}

	#[test]
	fn the_toggle_is_one_cross_after_the_walk_and_a_click_elsewhere_is_nothing() {
		let mut m = SoundMouse::default();
		let at = px(500.0, 396.0);
		assert_eq!(m.field(Some((1, 1, true, 0)), at, true, 0), UP);
		let out: Vec<u16> = (0..5).map(|_| m.field(Some((1, 1, true, 2)), at, false, 0)).collect();
		assert_eq!(out, vec![CROSS, CROSS, 0, 0, 0]);
		let mut m = SoundMouse::default();
		let off = px(500.0, 322.0);
		assert!((0..6).all(|f| m.field(Some((1, 1, true, 0)), off, f == 1, 0) == 0));
		// a click on a label (left of the bar) only selects the row
		let mut m = SoundMouse::default();
		assert!((0..6).all(|f| m.field(Some((1, 1, true, 0)), px(250.0, 293.0), f == 1, 0) == 0));
		// no page: nothing
		assert_eq!(m.field(None, at, true, 0), 0);
	}
}
