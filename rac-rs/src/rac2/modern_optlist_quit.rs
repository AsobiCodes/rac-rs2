//! [PC] [b47 X3] R&C2's retail "Quit Game" question (Options list row kind 0xc) in the modern look. The game's own box runs
//! underneath (Cross = Yes, Triangle = No, nothing else read); the modern list hides its picture, so the page draws the box itself
//! from the game's texts and the window's answers are sent to the game as a Cross / Triangle pulse (`modern_walk::Walk::press`).
//! Facts [C, runs b47\x3\runs\q357c (level of state 0357) + b46\w4\runs\quit2 (level 0), 2026-11-12]: while the box is up the game
//! mode word is 4 (3 in the list) and a dialog record {type 9, a code pointer, 2} sits in the block after the mode word: 0x14f8
//! behind it on level 0 (0x1aa478), 0x14b8 behind it on the level of 0357 (0x1aa438; idle {1, 0, 1}), so the record is found by
//! its shape inside a window, not by an address. Title 0xbc5 "Quit Game?", body 0xbde.
use crate::modern_pages::{word, QUIT_ASK, NO, YES};
use rac_engine::ui::kit::page::{Dialog, DialogButton};
use rac_engine::ui::keycaps::{self, Cap};
use rac_engine::ui::prim::Symbol;

/// Text ids [C]: the box's title and body.
pub const TITLE_ID: u32 = 0xbc5;
pub const BODY_ID: u32 = 0xbde;
/// The dialog type word of the quit box [C].
const TYPE_QUIT: u32 = 9;
/// Mode word value while a dialog box is up [C].
const MODE_BOX: u32 = 4;
const WINDOW: std::ops::Range<usize> = 0x1480..0x1540;

fn rd(ram: &[u8], a: usize) -> Option<u32> {
	ram.get(a..a + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The quit box is up: the mode word (`mode`, `pc_input::find_mode`) is 4 and the dialog record {9, code pointer, 2} is in the
/// window behind it. Pure over RAM.
pub fn up(ram: &[u8], mode: usize) -> bool {
	if rd(ram, mode) != Some(MODE_BOX) {
		return false;
	}
	WINDOW.step_by(4).any(|o| {
		let a = mode + o;
		matches!((rd(ram, a), rd(ram, a + 4), rd(ram, a + 8)), (Some(TYPE_QUIT), Some(p), Some(2)) if (0x10_0000..0x400_0000).contains(&p))
	})
}

/// The modern box: the game's body text (fallback the port's own words), Cross = Yes, Triangle = No.
pub fn dialog(body: Option<&str>, lang: usize) -> Dialog {
	let b = |symbol, cap, t: &rac_engine::ui::T5| DialogButton { symbol, key: keycaps::name(cap).to_string(), text: word(t, lang).to_string() };
	let line = body.filter(|s| !s.is_empty()).map_or_else(|| word(&QUIT_ASK, lang).to_string(), str::to_string);
	Dialog { lines: vec![line], buttons: vec![b(Symbol::Cross, Cap::Enter, &YES), b(Symbol::Triangle, Cap::Esc, &NO)] }
}

#[cfg(test)]
mod tests {
	use super::*;

	fn ram(mode_at: usize, rec_at: usize, mode: u32, rec: [u32; 3]) -> Vec<u8> {
		let mut r = vec![0u8; 0x20_0000];
		r[mode_at..mode_at + 4].copy_from_slice(&mode.to_le_bytes());
		for (i, v) in rec.iter().enumerate() {
			r[rec_at + 4 * i..rec_at + 4 * i + 4].copy_from_slice(&v.to_le_bytes());
		}
		r
	}

	#[test]
	fn found_on_both_levels_and_only_with_mode_4() {
		assert!(up(&ram(0x1a8f80, 0x1aa478, 4, [9, 0x2844d0, 2]), 0x1a8f80));
		assert!(up(&ram(0x1a8f80, 0x1aa438, 4, [9, 0x287160, 2]), 0x1a8f80));
		assert!(!up(&ram(0x1a8f80, 0x1aa438, 3, [9, 0x287160, 2]), 0x1a8f80), "the list (mode 3)");
		assert!(!up(&ram(0x1a8f80, 0x1aa438, 4, [1, 0, 1]), 0x1a8f80), "idle record");
		assert!(!up(&ram(0x1a8f80, 0x1aa438, 4, [8, 0x287160, 2]), 0x1a8f80), "another dialog type");
	}

	#[test]
	fn dialog_has_yes_no() {
		let d = dialog(Some("Quit?"), 0);
		assert_eq!(d.lines, vec!["Quit?".to_string()]);
		assert_eq!(d.buttons.len(), 2);
		assert!(!dialog(None, 0).lines[0].is_empty());
	}
}
