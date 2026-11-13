//! [PC] The "Subtitles" row (user 2026-11-11, "Yes, add both") kept in step with the game's own Subtitles byte `addrs::SUBS_ON`
//! (0x1a7c1e, the Options > Subtitles item retail PAL v1.00 never shows [C rac2/notes_menus.md item 5]; the movie subtitles are
//! drawn while it is nonzero, item 7 / movie_subs.rs). The byte is in the card's Options block (save table entry 0x26 =
//! {0x1a7c18, 0x28} [C 0x256138]), so a loaded save brings its own value: both ways, then.
//! - Row -> byte: the row's value is written into the byte at the first look and whenever the row changes (menu or launcher).
//! - Byte -> row: when the game changed the byte (a loaded save), the row follows it (`subtitles=` in pc_options.txt).
//! Off (the default) with a byte of 0 = nothing is ever written (retail).
use std::sync::Mutex;

/// What the game thread does this field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Act {
	None,
	/// Write the game's byte (the row changed, or the first look).
	Byte(u8),
	/// Write the row (the game changed the byte).
	Row(bool),
}

/// `seen` = the byte as the last field left it (None: first look), `byte` = now, `row` = the row in force.
pub fn decide(seen: Option<u8>, byte: u8, row: bool) -> Act {
	let on = byte != 0;
	if on == row {
		return Act::None;
	}
	match seen {
		Some(s) if s != byte => Act::Row(on),
		_ => Act::Byte(row as u8),
	}
}

static SEEN: Mutex<Option<u8>> = Mutex::new(None);

/// Game thread, every field from `options::LANG_WATCH_FROM` (host/game.rs).
pub fn watch(m: &mut ee_oracle::Machine) {
	crate::modern_rows_input::sync(m); // [b45 P2] the Left / Right and Up / Down rows -> the game's camera words
	// (host/game.rs calls this for live runs only: a replay must not write the player's options or the game byte.)
	let byte = m.rd8(crate::addrs::SUBS_ON);
	let row = crate::modern_rows_video::subtitles(&crate::options::get());
	let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
	match decide(*seen, byte, row) {
		Act::None => *seen = Some(byte),
		Act::Byte(v) => {
			m.cpu.mem.wr8(crate::addrs::SUBS_ON, v);
			crate::pc_rec::note_write(crate::pc_rec::Stage::After, crate::addrs::SUBS_ON, &[v]); // a replay writes it as made (pc_rec.rs)
			*seen = Some(v);
			eprintln!("rac2: Subtitles row {}: the game's Subtitles byte set", if v != 0 { "On" } else { "Off" });
		}
		Act::Row(on) => {
			*seen = Some(byte);
			eprintln!("rac2: the game's Subtitles byte changed (a loaded save): Subtitles row {}", if on { "On" } else { "Off" });
			if let Err(e) = crate::options::change(|o| crate::modern_rows_video::set(o, crate::modern_rows_video::KEY_SUBTITLES, on)) {
				eprintln!("rac2: PC options: cannot save subtitles: {e}");
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn row_and_byte_stay_in_step() {
		// Default: Off and 0, nothing written.
		assert_eq!(decide(None, 0, false), Act::None);
		assert_eq!(decide(Some(0), 0, false), Act::None);
		// First look with the row On: the byte follows.
		assert_eq!(decide(None, 0, true), Act::Byte(1));
		// The row changed (byte unchanged since the last field): the byte follows.
		assert_eq!(decide(Some(1), 1, false), Act::Byte(0));
		assert_eq!(decide(Some(0), 0, true), Act::Byte(1));
		// The game changed the byte (a loaded save): the row follows.
		assert_eq!(decide(Some(1), 0, true), Act::Row(false));
		assert_eq!(decide(Some(0), 1, false), Act::Row(true));
		// Any nonzero byte is On.
		assert_eq!(decide(Some(2), 2, true), Act::None);
	}
}
