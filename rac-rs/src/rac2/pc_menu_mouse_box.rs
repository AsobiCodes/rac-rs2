//! [PC] [b48 Y4] The mouse on retail's Yes / No box in Menu style Classic (Options > Quit Game): the box has NO cursor (Cross = Yes,
//! Triangle = No, nothing else is read), so a click on the word Yes sends one Cross, a click on No one Triangle, a click anywhere
//! else nothing. Read only: the pad only, like a hand. The box is found by `modern_optlist_quit::up` (mode word 4 + the dialog record
//! {9, code, 2}); the left click's own Circle never matters there (the box reads Cross / Triangle only).
//! Places [C measured, 1024 x 768 Classic picture `b47/x4/runs/quit/shots/w00575.png`, level of state 0357]: the buttons sit on one
//! line at y 413 px: "X Yes" x 345..450, "Triangle No" x 598..695. The box is two text lines high in English; the places are
//! generous by ~12 px so a language with a wider word still hits. Other Yes / No boxes (a second dialog type) are not mapped [?].
const CROSS: u16 = 0x40;
const TRIANGLE: u16 = 0x10;
const YES_X: (f32, f32) = (335.0 / 1024.0, 465.0 / 1024.0);
const NO_X: (f32, f32) = (588.0 / 1024.0, 710.0 / 1024.0);
const Y: (f32, f32) = (396.0 / 768.0, 432.0 / 768.0);

/// [b48 Y4] The Galactic Map's two plates, "Select" (x 105..275, y 506..535 px) and "Exit" (y 537..565) [C measured, `b48/y4/runs/map/shots/
/// w00130.png`]: Select = Cross (travels to the planet retail has selected), Exit = Triangle. The planet list's rows are not mapped (REMAINS).
pub fn map_answer(fx: f32, fy: f32) -> Option<Answer> {
	if !(100.0 / 1024.0..=280.0 / 1024.0).contains(&fx) {
		return None;
	}
	match fy * 768.0 {
		y if (504.0..=535.0).contains(&y) => Some(Answer::Yes),
		y if (537.0..=567.0).contains(&y) => Some(Answer::No),
		_ => None,
	}
}

/// [b49 Z2] Retail's overwrite question over a Save page (page id `modern_load::BOX_ID`, kind G+0xe4 = 0..2): Circle = Yes,
/// Triangle = No (its handler 0x2dc870, modern_load.rs). Places [C measured, 1024 x 768 Classic picture, lane ld run b43_clbox shot
/// w00760.png]: "O Yes" x 415..530, "Triangle No" x 795..885, both at y 400 px; the places are generous by ~12 px.
pub fn overwrite_answer(fx: f32, fy: f32) -> Option<Answer> {
	if !(378.0 / 768.0..=424.0 / 768.0).contains(&fy) {
		return None;
	}
	match fx * 1024.0 {
		x if (403.0..=542.0).contains(&x) => Some(Answer::Yes),
		x if (783.0..=897.0).contains(&x) => Some(Answer::No),
		_ => None,
	}
}

/// [b49 Z2] The overwrite question is up this field (pure over RAM; call only in a page menu: it reads the menu snapshot).
pub fn overwrite_up(ram: &[u8]) -> bool {
	let Some(s) = crate::modern_open::snap_now(ram) else { return false };
	let kind = ram.get(s.g as usize + crate::modern_load::BOX_KIND as usize..).and_then(|b| b.get(..4)).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
	s.page_id == crate::modern_load::BOX_ID && kind.is_some_and(|k| crate::modern_load::box_text(k).1)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
	Yes,
	No,
}

/// The button under (fx, fy) (fractions of the picture). Pure.
pub fn answer(fx: f32, fy: f32) -> Option<Answer> {
	if !(Y.0..=Y.1).contains(&fy) {
		return None;
	}
	if (YES_X.0..=YES_X.1).contains(&fx) {
		Some(Answer::Yes)
	} else if (NO_X.0..=NO_X.1).contains(&fx) {
		Some(Answer::No)
	} else {
		None
	}
}

#[derive(Default)]
pub struct BoxMouse {
	/// the left button was down the field before (a click is an edge)
	down: bool,
	/// fields still to press, and what
	hold: u8,
	press: u16,
}

impl BoxMouse {
	/// One field: `up` = the Classic quit box is up, `click` = the left button is down, `at` = the pointer's last place.
	pub fn field(&mut self, up: bool, click: bool, at: Option<(f32, f32)>, b: u16) -> u16 {
		self.field_with(up, click, at, b, answer)
	}

	/// The same with another screen's places (Yes = Cross, No = Triangle).
	pub fn field_with(&mut self, up: bool, click: bool, at: Option<(f32, f32)>, b: u16, places: fn(f32, f32) -> Option<Answer>) -> u16 {
		self.field_yes(up, click, at, b, places, CROSS)
	}

	/// [b49 Z2] The same with the pad bit Yes presses (the overwrite question takes Circle 0x20, the others Cross).
	pub fn field_yes(&mut self, up: bool, click: bool, at: Option<(f32, f32)>, b: u16, places: fn(f32, f32) -> Option<Answer>, yes: u16) -> u16 {
		let edge = click && !self.down;
		self.down = click;
		if !up {
			self.hold = 0;
			return b;
		}
		if edge {
			if let Some(a) = at.and_then(|(x, y)| places(x, y)) {
				(self.hold, self.press) = (2, if a == Answer::Yes { yes } else { TRIANGLE });
			}
		}
		if self.hold > 0 {
			self.hold -= 1;
			return b | self.press;
		}
		b
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn places() {
		assert_eq!(answer(400.0 / 1024.0, 413.0 / 768.0), Some(Answer::Yes));
		assert_eq!(answer(650.0 / 1024.0, 413.0 / 768.0), Some(Answer::No));
		assert_eq!(answer(520.0 / 1024.0, 413.0 / 768.0), None, "between the buttons");
		assert_eq!(answer(400.0 / 1024.0, 360.0 / 768.0), None, "on the question");
		assert_eq!(answer(400.0 / 1024.0, 600.0 / 768.0), None, "off the box");
	}

	#[test]
	fn the_map_plates() {
		assert_eq!(map_answer(190.0 / 1024.0, 520.0 / 768.0), Some(Answer::Yes));
		assert_eq!(map_answer(190.0 / 1024.0, 550.0 / 768.0), Some(Answer::No));
		assert_eq!(map_answer(190.0 / 1024.0, 400.0 / 768.0), None, "the planet list");
		assert_eq!(map_answer(600.0 / 1024.0, 520.0 / 768.0), None, "the picture");
		let mut m = BoxMouse::default();
		let at = Some((190.0 / 1024.0, 520.0 / 768.0));
		let out: Vec<u16> = (0..5).map(|f| m.field_with(true, f == 1, at, 0, map_answer)).collect();
		assert_eq!(out, vec![0, CROSS, CROSS, 0, 0]);
	}

	#[test]
	fn the_overwrite_box_answers_circle_and_triangle() {
		assert_eq!(overwrite_answer(470.0 / 1024.0, 400.0 / 768.0), Some(Answer::Yes));
		assert_eq!(overwrite_answer(840.0 / 1024.0, 400.0 / 768.0), Some(Answer::No));
		assert_eq!(overwrite_answer(640.0 / 1024.0, 400.0 / 768.0), None, "between");
		assert_eq!(overwrite_answer(470.0 / 1024.0, 330.0 / 768.0), None, "on the question text");
		let at = Some((470.0 / 1024.0, 400.0 / 768.0));
		let mut m = BoxMouse::default();
		let out: Vec<u16> = (0..4).map(|f| m.field_yes(true, f == 1, at, 0, overwrite_answer, 0x20)).collect();
		assert_eq!(out, vec![0, 0x20, 0x20, 0]);
		assert!(!overwrite_up(&vec![0u8; 0x100]), "no menu");
	}

	#[test]
	fn a_click_is_one_button_once() {
		let yes = Some((400.0 / 1024.0, 413.0 / 768.0));
		let no = Some((650.0 / 1024.0, 413.0 / 768.0));
		let mut m = BoxMouse::default();
		let out: Vec<u16> = (0..6).map(|f| m.field(true, (1..4).contains(&f), yes, 0)).collect();
		assert_eq!(out, vec![0, CROSS, CROSS, 0, 0, 0], "Yes = one Cross for 2 fields");
		let mut m = BoxMouse::default();
		let out: Vec<u16> = (0..5).map(|f| m.field(true, f == 1, no, 0)).collect();
		assert_eq!(out, vec![0, TRIANGLE, TRIANGLE, 0, 0]);
		let mut m = BoxMouse::default();
		assert!((0..6).all(|f| m.field(true, f == 1, Some((0.5, 0.5)), 0) == 0), "a click elsewhere");
		assert!((0..6).all(|f| m.field(true, f == 1, None, 0) == 0), "no pointer yet");
		// A click that was already down when the box came up (the Cross click that opened it) is no answer.
		let mut m = BoxMouse::default();
		m.field(false, true, yes, 0);
		assert_eq!(m.field(true, true, yes, 0), 0);
		assert_eq!(m.field(false, false, yes, 0), 0);
		// the keys' own buttons pass
		assert_eq!(m.field(true, false, yes, CROSS), CROSS);
	}
}
