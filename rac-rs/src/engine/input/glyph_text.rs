//! Button prompts that name the keyboard keys (game-neutral; written after R&C1's rac1/retail/prompts.rs + prompt_names.rs,
//! 2026-11-11, for R&C2. R&C1 keeps its own copy so its output cannot change; the rules are the same). Both PS2 games draw their button glyphs from one-byte codes inside the text strings
//! (0x10 Cross, 0x11 Circle, 0x12 Triangle, 0x13 Square, 0x14 L1, 0x15 R1, 0x16 L2, 0x17 R2, 0x18 right stick, 0x19 left stick: R&C1
//! decomp/notes/button_prompts.md, R&C2 rac2/notes_menus.md 3 [C]). A game answers its text lookup with a copy whose glyph bytes are
//! spelled out as key names (`spell`), so retail's own layout measures, wraps and draws the result.
//!
//! The names come from the live bindings of the installed kbm table (`kbm::table()` / `kbm::binds()`): a glyph's name is the first
//! bound input (slot 1, else slot 2) of the first Game-layer action that sets its pad bit; the sticks join the names of the four
//! direction actions ("forward", "strafe_left", "back", "strafe_right" / "camera_up", ...: the ids of the engine's generic table and
//! of R&C1's). The game installs its table before asking.
//!
//! `Device` is the "keyboard or controller" verdict (R&C1's rule, decided 2026-10-13): keyboard until real pad input; any key, mouse
//! button, wheel or clear mouse movement (`MoveSum`) makes it the keyboard again. The window thread notes, the game thread copies the
//! verdict once per field so a field is consistent.
use super::kbm::{self, names, Output};
use std::borrow::Cow;
use std::sync::atomic::{AtomicU8, Ordering};

pub const CROSS: u8 = 0x10;
pub const CIRCLE: u8 = 0x11;
pub const TRIANGLE: u8 = 0x12;
pub const SQUARE: u8 = 0x13;
pub const L1: u8 = 0x14;
pub const R1: u8 = 0x15;
pub const L2: u8 = 0x16;
pub const R2: u8 = 0x17;
pub const RSTICK: u8 = 0x18;
pub const LSTICK: u8 = 0x19;

/// The DualShock bit of a button glyph (the sticks and other bytes: None).
pub fn glyph_bit(code: u8) -> Option<u16> {
	Some(match code {
		CROSS => 0x40,
		CIRCLE => 0x20,
		TRIANGLE => 0x10,
		SQUARE => 0x80,
		L1 => 4,
		R1 => 8,
		L2 => 1,
		R2 => 2,
		_ => return None,
	})
}

/// True when `s` holds a button glyph byte.
pub fn has_glyph(s: &[u8]) -> bool {
	s.iter().any(|&b| (CROSS..=LSTICK).contains(&b))
}

/// What a prompt says for an action with no binding at all.
pub const NONE: &str = "None";

/// The prompt name of action `a` of the installed table: slot 1, else slot 2, else `NONE`.
pub fn action_name(a: usize) -> String {
	let b = kbm::binds();
	b.0.get(a).and_then(|s| s.iter().flatten().next().copied()).map_or_else(|| NONE.to_string(), names::short_name)
}

/// The name of the button glyph whose pad bit is `bit`: the first Game-layer action that sets it.
pub fn glyph_name(bit: u16) -> Option<String> {
	kbm::table().game_action(Output::Pad(bit)).map(action_name)
}

/// The name of a stick glyph. Under the default keys: "WASD" and "Q/E/Z/X" style (the right stick in the order right, left, down,
/// up as the manual lists them); rebound: the four names joined by "/" in the order forward, left, back, right.
pub fn stick_name(left: bool) -> String {
	let ids = if left { ["forward", "strafe_left", "back", "strafe_right"] } else { ["camera_up", "camera_left", "camera_down", "camera_right"] };
	let t = kbm::table();
	let acts: Vec<usize> = ids.iter().filter_map(|i| t.index(i)).collect();
	if acts.len() != 4 {
		return NONE.to_string();
	}
	let b = kbm::binds();
	let defaults = kbm::Binds::defaults(t);
	let default = acts.iter().all(|&a| b.0[a] == defaults.0[a]);
	let join = |order: [usize; 4], sep: &str| order.iter().map(|&a| action_name(a)).collect::<Vec<_>>().join(sep);
	match (default, left) {
		(true, true) => join([acts[0], acts[1], acts[2], acts[3]], ""),
		(true, false) => join([acts[3], acts[1], acts[2], acts[0]], "/"),
		_ => join([acts[0], acts[1], acts[2], acts[3]], "/"),
	}
}

/// The play key name of a glyph (`modern_camera`: the mouse turns the camera, so the right stick says "Mouse"); None for a byte that
/// is no button glyph.
pub fn key_name(code: u8, modern_camera: bool) -> Option<Cow<'static, [u8]>> {
	match code {
		RSTICK if modern_camera => Some(Cow::Borrowed(&b"Mouse"[..])),
		RSTICK => Some(Cow::Owned(stick_name(false).into_bytes())),
		LSTICK => Some(Cow::Owned(stick_name(true).into_bytes())),
		c => glyph_bit(c).and_then(glyph_name).map(|n| Cow::Owned(n.into_bytes())),
	}
}

/// `s` with every byte that `name` names replaced by that name (`first`: the name of byte 0 instead, R&C1's short legend rule);
/// None when `s` holds no button glyph.
pub fn spell<'a>(s: &[u8], first: Option<&[u8]>, mut name: impl FnMut(u8) -> Option<Cow<'a, [u8]>>) -> Option<Vec<u8>> {
	if !has_glyph(s) {
		return None;
	}
	let mut o = Vec::with_capacity(s.len() + 16);
	for (i, &b) in s.iter().enumerate() {
		if i == 0 {
			if let Some(n) = first {
				o.extend_from_slice(n);
				continue;
			}
		}
		match name(b) {
			Some(k) => o.extend_from_slice(&k),
			None => o.push(b),
		}
	}
	Some(o)
}

/// Mouse movement counts as use of the keyboard side once it adds up to `MOVE_PX` pixels within `MOVE_GAP_MS`.
pub const MOVE_PX: f32 = 40.0;
pub const MOVE_GAP_MS: u64 = 400;

/// Running sum of mouse movement; a pause of `MOVE_GAP_MS` starts it again.
#[derive(Default)]
pub struct MoveSum {
	last_ms: u64,
	sum: f32,
}

impl MoveSum {
	/// `dist` pixels moved at `now_ms`: true once the movement within the last `MOVE_GAP_MS` gaps reaches `MOVE_PX`.
	pub fn add(&mut self, now_ms: u64, dist: f32) -> bool {
		if now_ms.saturating_sub(self.last_ms) > MOVE_GAP_MS {
			self.sum = 0.0;
		}
		self.last_ms = now_ms;
		self.sum += dist;
		if self.sum >= MOVE_PX {
			self.sum = 0.0;
			return true;
		}
		false
	}
}

const KB: u8 = 0;
const PAD: u8 = 1;

/// The device verdict: `host` = what the window thread saw last, `field` = what the current field uses.
pub struct Device {
	host: AtomicU8,
	field: AtomicU8,
}

impl Device {
	/// Starts on the keyboard.
	pub const fn new() -> Device {
		Device::starting(true)
	}
	/// Starts on the keyboard (`keyboard`) or on the controller (the game's own glyphs until the first key or mouse input).
	pub const fn starting(keyboard: bool) -> Device {
		let d = if keyboard { KB } else { PAD };
		Device { host: AtomicU8::new(d), field: AtomicU8::new(d) }
	}
	/// Window thread: a key, mouse button, wheel or clear mouse movement.
	pub fn note_keyboard(&self) {
		self.host.store(KB, Ordering::Relaxed);
	}
	/// Window thread: real pad input.
	pub fn note_pad(&self) {
		self.host.store(PAD, Ordering::Relaxed);
	}
	/// Game thread, before a field: takes the window's verdict (`forced`: a scripted device for test runs, true = keyboard); returns
	/// Some(keyboard) when it changed.
	pub fn begin_field(&self, forced: Option<bool>) -> Option<bool> {
		let d = match forced {
			Some(kb) => {
				let d = if kb { KB } else { PAD };
				self.host.store(d, Ordering::Relaxed);
				d
			}
			None => self.host.load(Ordering::Relaxed),
		};
		(self.field.swap(d, Ordering::Relaxed) != d).then_some(d == KB)
	}
	/// The keyboard is the device of the current field.
	pub fn keyboard_now(&self) -> bool {
		self.field.load(Ordering::Relaxed) == KB
	}
}

impl Default for Device {
	fn default() -> Self {
		Device::new()
	}
}

/// `RAC_PROMPT_DEV`-style script `from-to:kb|pad;...` (fields; `to` empty = open end): the forced device of `field`, last entry wins.
pub fn scripted(spec: &str, field: u32) -> Option<bool> {
	spec.split(';')
		.filter_map(|e| {
			let (r, d) = e.split_once(':')?;
			let (a, z) = r.split_once('-')?;
			let kb = match d.trim() {
				"kb" => true,
				"pad" => false,
				_ => return None,
			};
			Some((a.trim().parse::<u32>().ok()?, z.trim().parse().unwrap_or(u32::MAX), kb))
		})
		.collect::<Vec<_>>()
		.into_iter()
		.rev()
		.find(|(a, z, _)| (*a..*z).contains(&field))
		.map(|x| x.2)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn spell_names_glyphs_and_keeps_the_rest() {
		let n = |b: u8| match b {
			CROSS => Some(Cow::Borrowed(&b"Space"[..])),
			TRIANGLE => Some(Cow::Borrowed(&b"T"[..])),
			_ => None,
		};
		assert_eq!(spell(b"\x12 Exit", None, n).unwrap(), b"T Exit");
		assert_eq!(spell(b"Press \x10 to jump", None, n).unwrap(), b"Press Space to jump");
		assert_eq!(spell(b"\x10 Select", Some(b"Spc"), n).unwrap(), b"Spc Select");
		assert_eq!(spell(b"\x0cTractor Beam\x08", None, n), None);
		// A glyph the namer does not know stays a glyph byte.
		assert_eq!(spell(b"\x15 help", None, n).unwrap(), b"\x15 help");
	}

	#[test]
	fn device_follows_the_window_and_scripts() {
		let d = Device::new();
		assert!(d.keyboard_now());
		d.note_pad();
		assert!(d.keyboard_now(), "the field keeps its device until the next field");
		assert_eq!(d.begin_field(None), Some(false));
		assert!(!d.keyboard_now());
		assert_eq!(d.begin_field(None), None);
		assert_eq!(d.begin_field(Some(true)), Some(true));
		assert_eq!(scripted("0-100:pad;50-:kb", 10), Some(false));
		assert_eq!(scripted("0-100:pad;50-:kb", 70), Some(true));
		assert_eq!(scripted("0-100:pad", 100), None);
		assert_eq!(glyph_bit(CROSS), Some(0x40));
		assert_eq!(glyph_bit(RSTICK), None);
		let p = Device::starting(false);
		assert!(!p.keyboard_now());
		assert_eq!(p.begin_field(None), None, "no input: stays on the controller");
		p.note_keyboard();
		assert_eq!(p.begin_field(None), Some(true));
	}

	#[test]
	fn mouse_movement_must_be_clear() {
		let mut m = MoveSum::default();
		assert!(!m.add(1000, 30.0));
		assert!(m.add(1100, 15.0));
		assert!(!m.add(2000, 39.0), "a pause starts the sum again");
	}
}
