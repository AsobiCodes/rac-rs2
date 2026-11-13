//! The font byte table of the PS2 Ratchet & Clank games (moved out of rac1/retail/menu_model/text.rs, 2026-11-11): R&C1's
//! [C: all five languages of all_text.bin] and R&C2's [C: rac2/notes_menus.md item 3, the glyph cells are R&C1's]. 0x20..0x7e as
//! ASCII, 0x80..0xb0 the accents (vowel blocks of eight: upper acute / grave / circumflex / umlaut, then the same lower case; then
//! N and C with tilde / cedilla, sharp s, em dash, inverted ! and ?, oe); below 0x20 control bytes and button glyphs.

/// 0x80..=0xb0 of the font.
const ACCENTS: [char; 49] = [
	'\u{c1}', '\u{c0}', '\u{c2}', '\u{c4}', '\u{e1}', '\u{e0}', '\u{e2}', '\u{e4}', // A
	'\u{c9}', '\u{c8}', '\u{ca}', '\u{cb}', '\u{e9}', '\u{e8}', '\u{ea}', '\u{eb}', // E
	'\u{cd}', '\u{cc}', '\u{ce}', '\u{cf}', '\u{ed}', '\u{ec}', '\u{ee}', '\u{ef}', // I
	'\u{d3}', '\u{d2}', '\u{d4}', '\u{d6}', '\u{f3}', '\u{f2}', '\u{f4}', '\u{f6}', // O
	'\u{da}', '\u{d9}', '\u{db}', '\u{dc}', '\u{fa}', '\u{f9}', '\u{fb}', '\u{fc}', // U
	'\u{d1}', '\u{f1}', '\u{c7}', '\u{e7}', '\u{df}', '\u{2014}', '\u{a1}', '\u{bf}', '\u{153}',
];

/// The character a printable font byte shows; None for the control bytes (they are not characters). Bytes above the table pass
/// through as the same Latin-1 char (text the port added itself, such as the PC options' rows, is real Unicode).
pub fn font_char(b: u8) -> Option<char> {
	match b {
		0x20..=0x7e => Some(b as char),
		0x80..=0xb0 => Some(ACCENTS[(b - 0x80) as usize]),
		0xb1..=0xff => Some(b as char),
		_ => None,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn accents_and_controls() {
		assert_eq!(font_char(b'A'), Some('A'));
		assert_eq!(font_char(0x8c), Some('\u{e9}'));
		assert_eq!(font_char(0x01), None);
	}
}
