//! Movie subtitles over the native movie player (retail behaviour the native player had left out; rac2/notes_menus.md item 7).
//!
//! Retail [C, boot ELF FUN_0027daa8 + FUN_002b7038]: before a story movie the movie list reads its subtitle script from the disc
//! and stores the pointer at 0x1b888c (`addrs::SUBS_SCRIPT`, 0 = none); the per-frame GS closer counts DISPLAY frames from the
//! movie's start (0x1b8874, 50 a second, 59.94 in the 60 Hz mode) and draws every entry whose [start, end] holds the count. Script
//! = 16-byte entries of eight i16 {start, end, text offset per language column 0..4 (relative to the script), ..}, ended by an entry
//! with start < 0. Column = language word 2..5 -> word - 1, else 0. Shown only while the Options > Subtitles byte
//! (`addrs::SUBS_ON`) is set, except texts starting with '*' (always): "**" = a title card at the top left (x 0x20, y 0x40; 0x18
//! with the 16:9 byte), '*' = centred (0x100, 0xd0); both fade over 17 frames at each end. Plain texts: grey text in a dark box
//! (colour 0x040404, alpha 0x60) centred at x 0x100, 420 px wrap width, its centre 100 px above the screen's bottom (54 with the
//! 16:9 byte), kept 20 px above the bottom.
//!
//! The native player keeps the game thread parked in the movie hook, so the window draws the entries itself with the engine's UI
//! text (`Prim`s through `Gpu::ui_gpu`, drawn after the movie layer): the game's font is not used [I: same words, positions and
//! timing; the PC UI face]. The script is copied out of RAM once in the hook (rac2/movie.rs `movie_call`).

use rac_engine::ui::font::FontSet;
use rac_engine::ui::prim::{Align, FontRole, Prim, Rect, Rgba, TextRun};
use rac_engine::ui::theme::{DESIGN_H, DESIGN_W};

/// How a line is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
	/// A dialogue line in the dark box near the bottom (only with Subtitles on).
	Boxed,
	/// "**": a title card at the top left, fading.
	Title,
	/// "*": centred on the screen, fading.
	Centre,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
	pub start: i32,
	pub end: i32,
	pub kind: Kind,
	/// The text without its leading '*', '\n' at the font's newline byte.
	pub text: String,
}

/// One movie's lines in the game's language, with the screen facts the positions hang from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Script {
	pub lines: Vec<Line>,
	/// The 16:9 byte (`addrs::WIDE`).
	pub wide: bool,
	/// The screen height word (`addrs::SCREEN_H`).
	pub screen_h: i32,
}

/// Entries read at most (a script ends far earlier; a bad pointer stops here).
const MAX_ENTRIES: usize = 512;
/// Bytes of one text read at most.
const MAX_TEXT: usize = 512;

/// The script's text column of a language word.
pub fn column(lang: u32) -> usize {
	if (2..=5).contains(&lang) { (lang - 1) as usize } else { 0 }
}

fn i16_at(ram: &[u8], a: usize) -> Option<i16> {
	ram.get(a..a + 2).map(|b| i16::from_le_bytes([b[0], b[1]]))
}

/// The font bytes of a text as a string: printable bytes through the font table, 0x01 / '\n' as newlines, other control bytes left out.
fn decode(raw: &[u8]) -> String {
	raw.iter().filter_map(|&b| if b == 0x01 || b == b'\n' { Some('\n') } else { rac_formats::font_bytes::font_char(b) }).collect()
}

/// Reads the script at guest address `base` (0 = none) for language word `lang`; `on` = the Subtitles byte.
pub fn read(ram: &[u8], base: u32, lang: u32, on: bool, wide: bool, screen_h: i32) -> Script {
	let mut s = Script { lines: Vec::new(), wide, screen_h: if screen_h > 0 { screen_h } else { 448 } };
	if base == 0 {
		return s;
	}
	let (base, col) = ((base & 0x01ff_ffff) as usize, column(lang));
	for k in 0..MAX_ENTRIES {
		let e = base + k * 16;
		let (Some(start), Some(end), Some(off)) = (i16_at(ram, e), i16_at(ram, e + 2), i16_at(ram, e + 4 + col * 2)) else { break };
		if start < 0 {
			break;
		}
		let t = base as i64 + off as i64;
		let raw: Vec<u8> = ram.get(t.max(0) as usize..).unwrap_or(&[]).iter().take(MAX_TEXT).take_while(|&&b| b != 0).copied().collect();
		let stars = raw.iter().take(2).take_while(|&&b| b == b'*').count();
		if stars == 0 && !on {
			continue;
		}
		let kind = [Kind::Boxed, Kind::Centre, Kind::Title][stars];
		s.lines.push(Line { start: start as i32, end: end as i32, kind, text: decode(&raw[stars..]) });
	}
	s
}

/// The display-frame count of the movie's `frames`-th shown picture (25 a second; the first is count 0) at `rate` fields a second.
pub fn count_of(frames: u64, rate: f32) -> i32 {
	(frames.saturating_sub(1) as f64 * rate as f64 / 25.0) as i32
}

/// A faded line's opacity at `count` (retail: alpha (edge distance + 1) << 27 within 17 frames of an edge, 0x80 = opaque).
fn fade(l: &Line, count: i32) -> f32 {
	let edge = if l.end - count < 0x12 { l.end - count + 1 } else if count - l.start < 0x12 { count - l.start + 1 } else { 16 };
	(edge as f32 * 8.0 / 128.0).clamp(0.0, 1.0)
}

/// Text size and line step in design units (retail: 18-pixel lines on a 448-line screen).
const PX: f32 = 26.0;
const STEP: f32 = PX * 1.25;

/// Splits `text` into lines no wider than `room` (words kept whole; the font's newlines kept).
fn wrap(text: &str, room: f32, width: impl Fn(&str) -> f32) -> Vec<String> {
	let mut out = Vec::new();
	for para in text.split('\n') {
		let mut cur = String::new();
		for w in para.split(' ').filter(|w| !w.is_empty()) {
			let next = if cur.is_empty() { w.to_string() } else { format!("{cur} {w}") };
			if !cur.is_empty() && width(&next) > room {
				out.push(std::mem::replace(&mut cur, w.to_string()));
			} else {
				cur = next;
			}
		}
		out.push(cur);
	}
	out.retain(|l| !l.is_empty());
	out
}

impl Script {
	/// The lines shown at display-frame `count`.
	pub fn at(&self, count: i32) -> impl Iterator<Item = &Line> {
		self.lines.iter().filter(move |l| l.start <= count && count <= l.end)
	}

	/// Shot runs: does a line's middle display frame fall in (prev, now]? (one movie shot inside every line's window).
	pub fn mid_in(&self, prev: i32, now: i32) -> bool {
		self.lines.iter().any(|l| {
			let mid = (l.start + l.end) / 2;
			prev < mid && mid <= now
		})
	}

	/// The UI draw list of display-frame `count` (empty: nothing shown). The game's screen (512 x `screen_h`, 4:3, or 16:9 with the
	/// 16:9 byte) is mapped onto the design area's centre.
	pub fn prims(&self, count: i32) -> Vec<Prim> {
		self.prims_with(count, |t| FontSet::get().width(FontRole::Medium, PX, t))
	}

	fn prims_with(&self, count: i32, width: impl Fn(&str) -> f32) -> Vec<Prim> {
		let sw = if self.wide { DESIGN_W } else { DESIGN_H * 4.0 / 3.0 };
		let (ox, k) = ((DESIGN_W - sw) * 0.5, sw / 512.0);
		let h = self.screen_h as f32;
		let y_of = |y: f32| y / h * DESIGN_H;
		let mut out = Vec::new();
		for l in self.at(count) {
			let a = if l.kind == Kind::Boxed { 1.0 } else { fade(l, count) };
			let colour = if l.kind == Kind::Boxed { Rgba::rgb(0xe8e8e8) } else { Rgba::rgb(0xffffff).tint(a) };
			let lines = wrap(&l.text, 420.0 * k, &width);
			let (x, align, y0) = match l.kind {
				Kind::Boxed => {
					let wmax = lines.iter().map(|s| width(s)).fold(0.0f32, f32::max);
					let bh = STEP * lines.len() as f32;
					let mut cy = y_of(h - if self.wide { 54.0 } else { 100.0 } - 4.0);
					cy = cy.min(y_of(h - 20.0) - bh * 0.5 - 5.0);
					let r = Rect::new(ox + 256.0 * k - wmax * 0.5 - 10.0 * k, cy - bh * 0.5 - 5.0, wmax + 20.0 * k, bh + 10.0);
					out.push(Prim::RRect { rect: r, radius: 0.0, fill: Rgba::rgba(4, 4, 4, 0.75), border: None });
					(ox + 256.0 * k, Align::Center, cy - bh * 0.5 + STEP * 0.5)
				}
				Kind::Title => (ox + 0x20 as f32 * k, Align::Left, y_of(if self.wide { 0x18 as f32 } else { 0x40 as f32 })),
				Kind::Centre => (ox + 256.0 * k, Align::Center, y_of(0xd0 as f32) - STEP * (lines.len() as f32 - 1.0) * 0.5),
			};
			for (i, s) in lines.into_iter().enumerate() {
				out.push(Prim::Text(TextRun { x, y: y0 + STEP * i as f32, role: FontRole::Medium, px: PX, colour, align, max_w: 0.0, track: 0.0, text: s }));
			}
		}
		out
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// A script at 0x100 of a small RAM: entries (start, end, offsets per column), then the texts.
	fn ram_with(entries: &[(i16, i16, [i16; 5])], texts: &[(usize, &[u8])]) -> Vec<u8> {
		let mut ram = vec![0u8; 0x1000];
		for (k, (s, e, offs)) in entries.iter().enumerate() {
			let at = 0x100 + k * 16;
			ram[at..at + 2].copy_from_slice(&s.to_le_bytes());
			ram[at + 2..at + 4].copy_from_slice(&e.to_le_bytes());
			for (c, o) in offs.iter().enumerate() {
				ram[at + 4 + c * 2..at + 6 + c * 2].copy_from_slice(&o.to_le_bytes());
			}
		}
		let end = 0x100 + entries.len() * 16;
		ram[end..end + 2].copy_from_slice(&(-1i16).to_le_bytes());
		for (off, t) in texts {
			ram[0x100 + off..0x100 + off + t.len()].copy_from_slice(t);
		}
		ram
	}

	#[test]
	fn reads_the_language_column_and_the_subtitles_byte() {
		let ram = ram_with(&[(10, 60, [0x200, 0x210, 0x220, 0x230, 0x240]), (70, 120, [0x300; 5])], &[(0x200, b"Hello\x01there"), (0x210, b"Bonjour \x8c"), (0x300, b"**Planet")]);
		let s = read(&ram, 0x100, 2, true, false, 448);
		assert_eq!(s.lines.len(), 2);
		assert_eq!(s.lines[0], Line { start: 10, end: 60, kind: Kind::Boxed, text: "Bonjour \u{e9}".into() });
		assert_eq!(s.lines[1].kind, Kind::Title);
		assert_eq!(s.lines[1].text, "Planet");
		assert_eq!(read(&ram, 0x100, 0, true, false, 448).lines[0].text, "Hello\nthere", "English, the font's newline");
		let off = read(&ram, 0x100, 0, false, false, 448);
		assert_eq!(off.lines.len(), 1, "Subtitles off: only the forced '*' texts");
		assert!(read(&ram, 0, 0, true, false, 448).lines.is_empty(), "no script");
		assert_eq!((column(0), column(1), column(3), column(5), column(9)), (0, 0, 2, 4, 0));
	}

	#[test]
	fn lines_show_in_their_frames_and_titles_fade() {
		let ram = ram_with(&[(10, 60, [0x200; 5]), (70, 120, [0x300; 5])], &[(0x200, b"Hi"), (0x300, b"*Card")]);
		let s = read(&ram, 0x100, 0, true, false, 448);
		let w = |t: &str| t.len() as f32 * 10.0;
		assert!(s.prims_with(9, w).is_empty());
		let p = s.prims_with(10, w);
		assert!(matches!(&p[0], Prim::RRect { .. }) && matches!(&p[1], Prim::Text(t) if t.text == "Hi"));
		assert!(s.prims_with(65, w).is_empty());
		let alpha = |c: i32| match &s.prims_with(c, w)[0] {
			Prim::Text(t) => t.colour.a,
			_ => panic!("text"),
		};
		assert!(alpha(70) < 20 && alpha(95) == 255 && alpha(120) < 20, "{} {} {}", alpha(70), alpha(95), alpha(120));
		assert_eq!((count_of(1, 50.0), count_of(26, 50.0), count_of(26, 60000.0 / 1001.0)), (0, 50, 59));
	}

	#[test]
	fn wraps_at_the_box_width() {
		let l = wrap("aa bb cc\ndd", 50.0, |t| t.len() as f32 * 10.0);
		assert_eq!(l, vec!["aa bb", "cc", "dd"]);
	}
}
