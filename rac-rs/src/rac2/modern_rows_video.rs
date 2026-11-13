//! [PC] R&C2's Video rows (user 2026-11-11, "Yes, add both"): the PAL disc's movie subtitles. Retail PAL v1.00 has the Subtitles
//! toggle item but no menu row reaches it [C rac2/notes_menus.md item 5 "REACHABILITY"], so this Display row is its only way in.
//! Default Off = retail. pc_options key `subtitles=on` (no key = Off); the switch is the game's byte 0x1a7c1e (subs_sync.rs).
//! The "60 Hz mode" row is HIDDEN (2026-11-11, b60i): the game's NTSC mode on the PAL disc hangs the first level (video60.rs), so
//! the row is in no menu and the key `video60` is neither read nor written.
use crate::modern_rows::{row, NO, TOG};
use rac_engine::pcopts::PcOptions;
use rac_engine::ui::{Cost, UiRow};

pub const KEY_SUBTITLES: &str = "subtitles";

fn on(o: &PcOptions, key: &str) -> bool {
	o.extra_get(key).is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "on" | "1" | "true"))
}

/// The Subtitles row: on = the story movies show their dialogue lines.
pub fn subtitles(o: &PcOptions) -> bool {
	on(o, KEY_SUBTITLES)
}

/// Writes a row as its key (Off = no key, the default).
pub fn set(o: &mut PcOptions, key: &str, v: bool) {
	o.extra_set(key, v.then(|| "on".to_string()));
}

/// The row value `id` has in `o` (None = not one of these rows).
pub fn value(o: &PcOptions, id: &str) -> Option<i32> {
	match id {
		KEY_SUBTITLES => Some(subtitles(o) as i32),
		_ => None,
	}
}

/// Writes row `id` = `v` into `o`; false = not one of these rows.
pub fn apply(o: &mut PcOptions, id: &str, v: i32) -> bool {
	match id {
		KEY_SUBTITLES => {
			set(o, id, v != 0);
			true
		}
		_ => false,
	}
}

pub static VIDEO: [UiRow; 1] = [
	row(
		KEY_SUBTITLES,
		TOG,
		["Subtitles", "Sous-titres", "Untertitel", "Subt\u{ed}tulos", "Sottotitoli"],
		[
			"Shows the dialogue of the story movies as text: the game's own subtitles, hidden in the PAL menus.",
			"Affiche les dialogues des cin\u{e9}matiques en texte : les sous-titres du jeu, cach\u{e9}s dans les menus PAL.",
			"Zeigt die Dialoge der Story-Filme als Text: die eigenen Untertitel des Spiels, in den PAL-Men\u{fc}s versteckt.",
			"Muestra los di\u{e1}logos de las cinem\u{e1}ticas como texto: los subt\u{ed}tulos del propio juego, ocultos en los men\u{fa}s PAL.",
			"Mostra i dialoghi dei filmati come testo: i sottotitoli del gioco, nascosti nei menu PAL.",
		],
		NO,
		0,
		Cost::None,
		None,
	),
];

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn rows_round_trip_and_default_off() {
		let mut o = PcOptions::default();
		assert_eq!(value(&o, KEY_SUBTITLES), Some(0));
		assert!(apply(&mut o, KEY_SUBTITLES, 1));
		assert_eq!(o.extra_get(KEY_SUBTITLES), Some("on"));
		assert!(subtitles(&o));
		assert!(apply(&mut o, KEY_SUBTITLES, 0));
		assert_eq!(o.extra_get(KEY_SUBTITLES), None, "Off = no key");
		assert!(!apply(&mut o, "fps", 1));
		assert_eq!(value(&o, "fps"), None);
	}

	/// The hidden 60 Hz row: in no menu, its key neither read nor written.
	#[test]
	fn the_60_hz_row_is_hidden() {
		let mut o = PcOptions::default();
		assert!(VIDEO.iter().all(|r| r.id != "video60"));
		assert_eq!(value(&o, "video60"), None);
		assert!(!apply(&mut o, "video60", 1));
		assert_eq!(o.extra_get("video60"), None);
	}

	#[test]
	fn rows_are_toggles_with_off_default() {
		for r in &VIDEO {
			assert_eq!(r.def, 0, "{} defaults to retail (Off)", r.id);
		}
	}
}
