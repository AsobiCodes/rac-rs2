//! [PC] Debug mode's on-screen tools (parity row 71; R&C1's twin is `retail/devtools.rs`, same look and keys where R&C2 leaves them
//! free). Inert unless Debug mode is on (`options::debug_mode`: Options -> Controls -> Debug, `debug=on`, RAC_DEV_TOOLS=1 /
//! RAC_DEBUG_KEYS=1): off, no key, button or mouse event is taken, nothing is drawn and the game's pad is untouched.
//!
//! - Planet list: F4 (R&C1's F1; R&C2's F1 is the picture flip) opens a list of the 27 levels over the picture; Up / Down (W / S,
//!   PgUp / PgDn, Home / End; on a gamepad the D-pad) pick, Enter / Space (Cross) goes, Esc / Backspace / F4 (Triangle / Circle) closes.
//!   The list takes every key press while it is open and the game gets a neutral pad. The warp goes out through retail's own level
//!   request (pc_warp.rs: the level's request function's two words, written by pc_debug.rs before the next field), only in gameplay
//!   (a 3D field, no menu); picking the planet you are on restarts it (retail reloads the level).
//! - Readout: the window title and a top-left line get the level, Ratchet's position and, with the free camera, its pose.
//! - Notice: switching Debug mode On in the menu puts up the keys and the warp warning for 15 s (R&C1's NOTICE, R&C2's keys).
//! - F3 draw distance A/B (R&C1 debugkeys): Low (retail's distances) / the Draw distance row's level, window side only.
//! Test: RAC2_DEVTOOLS_FEED=`FIELD:NAME,NAME;...` presses those keys (options-file names: F4, ArrowDown, Enter, KeyP ...) through
//! the window's own Debug-mode key path at that game field (host.rs `debug_key`); `FIELD:notice` puts the notice up; `FIELD:pad:L3+R3`
//! holds those gamepad buttons for that field (Debug tools only, never the game's pad: host/debug_route.rs `pad`).
use rac_engine::overlay::{rect, text, text_width, Quad};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use winit::keyboard::KeyCode as K;

const LEVELS: usize = crate::pc_warp::LEVELS as usize;
/// How long an event stays in the title / on screen, and the notice of a fresh Debug mode On.
const NOTE_FOR: Duration = Duration::from_secs(4);
const NOTICE_FOR: Duration = Duration::from_secs(15);

/// The level names by level word [C: the 27 level files in rac2/notes_metal.md order; the places of 1-13 and 24 are the PAL map
/// screen's (modern_map.rs `english`)]. The overlay font has capitals only.
pub const NAMES: [&str; LEVELS] = [
	"ARANOS (GADGETRON HQ)",
	"OOZLA (MEGACORP OUTLET)",
	"MAKTAR NEBULA (MAKTAR RESORT)",
	"ENDAKO (MEGAPOLIS)",
	"BARLOW (VUKOVAR CANYON)",
	"FELTZIN SYSTEM (THUG RENDEZVOUS)",
	"NOTAK (CANAL CITY)",
	"SIBERIUS (FROZEN BASE)",
	"TABORA (MINING AREA)",
	"DOBBO (TESTING FACILITY)",
	"HRUGIS CLOUD (DEEP SPACE DISPOSAL)",
	"JOBA (MEGACORP GAMES)",
	"TODANO (MEGACORP ARMORY)",
	"BOLDAN (SILVER CITY)",
	"ARANOS (SECOND VISIT)",
	"GORN",
	"SNIVELAK",
	"SMOLG",
	"DAMOSEL",
	"GRELBIN",
	"YEEDIL",
	"INSOMNIAC MUSEUM",
	"DOBBO ORBIT",
	"DAMOSEL ORBIT",
	"SHIP SHACK (SLIM COGNITO)",
	"WUPASH NEBULA",
	"JAMMING ARRAY",
];

/// The notice drawn when the player switches Debug mode On: R&C1's lines with R&C2's keys, then the warning.
pub const NOTICE: [&str; 13] = [
	"DEBUG MODE ON",
	"F4  PLANET LIST / WARP (ENTER GOES, ESC CLOSES)",
	"F2  FREE CAMERA (WASD, MOUSE, P FREEZE, R RESET, PAD L3 + R3)",
	"F3  DRAW DISTANCE: LOW / YOUR SETTING",
	"F7  IMMORTALITY ON / OFF",
	"F6  INFINITE AMMO ON / OFF",
	"F8  +10000 BOLTS",
	"F10 SPLIT VIEW CLASSIC / ENHANCED (BRACKET KEYS MOVE, SHIFT FAST, HOME CENTRE, BACKSLASH AUTO WIPE)",
	"F11 HIDE HUD ON / OFF (CLEAN RECORDINGS)",
	"F12 SPLIT VIEW LABELS ON / OFF",
	"F1 PICTURE, F5 QUICK SAVE, F9 QUICK LOAD STAY (R&C1: F1 LIST, F5 IMMORTALITY)",
	"WARNING: WARPING CAN SKIP STORY PROGRESS.",
	"SAVE YOUR GAME FIRST. OFF: OPTIONS - CONTROLS - DEBUG.",
];

pub const HELP_PICK: &str = "UP/DOWN PGUP/PGDN PICK - ENTER WARP - ESC CLOSE";

/// PS2 pad bits (pressed = 1).
const PAD_TRI: u16 = 0x10;
const PAD_CIRCLE: u16 = 0x20;
const PAD_CROSS: u16 = 0x40;
const PAD_UP: u16 = 0x1000;
const PAD_DOWN: u16 = 0x4000;

struct Ui {
	/// The planet list is open, with this row under the cursor.
	pick: Option<usize>,
	/// A warp asked for and not yet handed to a field.
	warp: Option<u32>,
	note: Option<(String, Instant)>,
	/// Debug mode was just switched On by the player: when.
	notice: Option<Instant>,
	/// The gamepad buttons of the last poll (edges).
	pad_prev: u16,
	/// Ratchet's position of the newest drawn frame.
	hero: Option<[f32; 3]>,
	/// Test feed: (field, key names) and the last field fed.
	feed: Option<Vec<(u64, String)>>,
	fed: u64,
}

static UI: Mutex<Ui> = Mutex::new(Ui { pick: None, warp: None, note: None, notice: None, pad_prev: 0, hero: None, feed: None, fed: u64::MAX });
/// The list is open (the game thread's pad mask reads it without the lock).
static LIST: AtomicBool = AtomicBool::new(false);
/// The level word the game is on (set by the game thread before every field in Debug mode).
static CURRENT: AtomicU32 = AtomicU32::new(0);
/// The window's verdict of the newest frame: gameplay (a 3D field, no movie, no menu): a warp is taken only then.
static PLAYING: AtomicBool = AtomicBool::new(false);
/// F3: the draw distance is Low (retail's) instead of the Draw distance row's level.
static FAR_LOW: AtomicBool = AtomicBool::new(false);
/// The pad buttons that closed the list (or warped), kept from the game until released.
static HELD: AtomicU32 = AtomicU32::new(0);

fn ui() -> std::sync::MutexGuard<'static, Ui> {
	UI.lock().unwrap_or_else(|e| e.into_inner())
}

fn on() -> bool {
	crate::options::debug_mode()
}

fn note(u: &mut Ui, s: String) {
	eprintln!("rac2 devtools: {s}");
	u.note = Some((s, Instant::now()));
}

pub fn level_name(l: usize) -> &'static str {
	NAMES.get(l).copied().unwrap_or("?")
}

/// The list's cursor after key `k` (None = the key closes it, Some(Some(n)) = warp to n, Some(None) = stay open).
fn list_step(row: usize, k: K, repeat: bool) -> Result<Option<usize>, ()> {
	let n = LEVELS;
	Ok(Some(match k {
		K::ArrowUp | K::KeyW => (row + n - 1) % n,
		K::ArrowDown | K::KeyS => (row + 1) % n,
		K::PageUp => row.saturating_sub(5),
		K::PageDown => (row + 5).min(n - 1),
		K::Home => 0,
		K::End => n - 1,
		K::Enter | K::NumpadEnter | K::Space if !repeat => return Ok(None),
		K::Escape | K::Backspace | K::F4 if !repeat => return Err(()),
		_ => row,
	}))
}

fn close(u: &mut Ui) {
	u.pick = None;
	LIST.store(false, Ordering::Relaxed);
}

fn ask_warp(u: &mut Ui, row: usize) {
	close(u);
	u.warp = Some(row as u32);
	note(u, format!("WARP TO {row:02} {} ASKED", level_name(row)));
}

/// Window thread: a key event, before every other handler (the list owns the keyboard while open). True = taken. Releases pass.
pub fn list_key(k: K, pressed: bool, repeat: bool) -> bool {
	if !pressed || !on() {
		return false;
	}
	let mut u = ui();
	if let Some(row) = u.pick {
		match list_step(row, k, repeat) {
			Ok(Some(r)) => u.pick = Some(r),
			Ok(None) => ask_warp(&mut u, row),
			Err(()) => close(&mut u),
		}
		return true;
	}
	if k == K::F4 {
		if !repeat {
			u.pick = Some((CURRENT.load(Ordering::Relaxed) as usize).min(LEVELS - 1));
			LIST.store(true, Ordering::Relaxed);
		}
		return true;
	}
	false
}

/// Window thread: F3, the draw distance A/B (only reached in Debug mode, pc_debug.rs `key`).
pub fn far_toggle() {
	let low = !FAR_LOW.fetch_xor(true, Ordering::Relaxed);
	note(&mut ui(), format!("DRAW DISTANCE {}", if low { "LOW (PS2)" } else { "YOUR SETTING" }));
}

/// The far draw of this frame: the Draw distance row's level `q`, or Low while F3 holds it (Debug mode only).
pub fn far(q: u8) -> Option<rac_engine::viewer::fardraw::Level> {
	if FAR_LOW.load(Ordering::Relaxed) && on() {
		return None;
	}
	rac_engine::viewer::fardraw::Level::from_q(q)
}

/// Window thread, every poll with a controller: the D-pad / Cross / Triangle / Circle drive the list while it is open.
pub fn pad(buttons: u16) {
	if !on() {
		return;
	}
	let mut u = ui();
	let edge = buttons & !u.pad_prev;
	u.pad_prev = buttons;
	HELD.fetch_and(buttons as u32, Ordering::Relaxed);
	let Some(row) = u.pick else { return };
	match pad_step(row, edge) {
		Ok(Some(r)) => u.pick = Some(r),
		Ok(None) => ask_warp(&mut u, row),
		Err(()) => close(&mut u),
	}
	if u.pick.is_none() {
		HELD.store(buttons as u32, Ordering::Relaxed);
	}
}

/// The list's cursor after the pad's newly pressed buttons `edge` (as `list_step`: Err = close, Ok(None) = warp).
fn pad_step(row: usize, edge: u16) -> Result<Option<usize>, ()> {
	let n = LEVELS;
	Ok(Some(if edge & PAD_UP != 0 {
		(row + n - 1) % n
	} else if edge & PAD_DOWN != 0 {
		(row + 1) % n
	} else if edge & PAD_CROSS != 0 {
		return Ok(None);
	} else if edge & (PAD_TRI | PAD_CIRCLE) != 0 {
		return Err(());
	} else {
		row
	}))
}

/// Game thread: the pad bits the game must not see: all while the list is open, then the buttons that closed it until released.
pub fn pad_mask() -> u16 {
	if !on() {
		return 0;
	}
	if LIST.load(Ordering::Relaxed) { 0xffff } else { HELD.load(Ordering::Relaxed) as u16 }
}

/// Game thread: the pad the game gets (a neutral one while the list is open; Debug mode off = untouched).
pub fn filter(b: u16, s: [u8; 4]) -> (u16, [u8; 4]) {
	match pad_mask() {
		0 => (b, s),
		0xffff => (0, [0x80; 4]),
		m => (b & !m, s),
	}
}

/// Game thread, before a field in Debug mode: the level the game is on (the list opens there, the readout names it).
pub fn set_level(level: u32) {
	CURRENT.store(level, Ordering::Relaxed);
}

/// Window thread, every frame in Debug mode: gameplay or not, and Ratchet's position.
pub fn set_frame(playing: bool, hero: Option<[f32; 3]>) {
	if !on() {
		return;
	}
	PLAYING.store(playing, Ordering::Relaxed);
	ui().hero = hero;
}

/// Game thread: the warp asked for, once, if the game can take it now (gameplay); else it is dropped with a note.
pub fn take_warp() -> Option<u32> {
	let mut u = ui();
	let to = u.warp.take()?;
	let from = CURRENT.load(Ordering::Relaxed);
	if PLAYING.load(Ordering::Relaxed) {
		note(&mut u, format!("WARP {from:02} {} -> {to:02} {}", level_name(from as usize), level_name(to as usize)));
		Some(to)
	} else {
		note(&mut u, "WARP DROPPED: ONLY IN GAMEPLAY".into());
		None
	}
}

/// Game thread: the warp request code was not found in this level's RAM.
pub fn warp_failed() {
	note(&mut ui(), "WARP: REQUEST CODE NOT FOUND".into());
}

/// Debug mode was switched On by the player (options.rs): the notice goes up.
pub fn announce() {
	ui().notice = Some(Instant::now());
	eprintln!("rac2 devtools: Debug mode On (F4 planet list / warp, F2 free camera, F3 draw distance A/B, F7 immortality, F6 infinite ammo, F8 +10000 bolts, F10 split view, F11 hide HUD, F12 split labels)");
}

/// Debug mode was switched Off (options.rs): every tool is put away, so a later On starts from nothing.
pub fn menu_off() {
	let mut u = ui();
	close(&mut u);
	u.warp = None;
	u.note = None;
	u.notice = None;
	FAR_LOW.store(false, Ordering::Relaxed);
	HELD.store(0, Ordering::Relaxed);
	eprintln!("rac2 devtools: Debug mode Off");
}

/// The list is open (the modern pages step aside; host.rs).
pub fn list_open() -> bool {
	LIST.load(Ordering::Relaxed) && on()
}

/// The readout: level, Ratchet's position, the free camera's pose and state, the last event.
fn readout(u: &Ui) -> String {
	let l = CURRENT.load(Ordering::Relaxed) as usize;
	let mut s = format!("L{l:02} {}", level_name(l));
	if let Some(h) = u.hero {
		s += &format!(" - HERO {:.1} {:.1} {:.1}", h[0], h[1], h[2]);
	}
	if let Some(fc) = crate::pc_freecam::status() {
		s += " - ";
		s += &fc;
	}
	if let Some((t, at)) = &u.note {
		if at.elapsed() < NOTE_FOR {
			s += " - ";
			s += t;
		}
	}
	s
}

/// The window title's part (None with Debug mode off).
pub fn title() -> Option<String> {
	if !on() {
		return None;
	}
	let u = ui();
	let s = readout(&u);
	Some(match u.pick {
		Some(row) => format!("DEV {s} - WARP TO > {row:02} {} <", level_name(row)),
		None => format!("DEV {s}"),
	})
}

/// The text size (at most `size`) that keeps the widest of `lines` inside `avail` pixels: a narrow window shrinks a long line
/// instead of cutting it off at the right edge.
fn fit<'a>(size: f32, lines: impl IntoIterator<Item = &'a str>, avail: f32) -> f32 {
	let widest = lines.into_iter().map(|l| text_width(l, 1.0)).fold(0.0, f32::max);
	if widest > 0.0 && widest * size > avail { (avail / widest).max(1.0) } else { size }
}

/// The Debug mode notice: a dark panel at the top left with the keys, the warning in red (render pixels).
fn draw_notice(out: &mut Vec<Quad>, k: f32, y0: f32, w: f32) {
	let size = fit(13.0 * k, NOTICE, w - 30.0 * k);
	let lh = size * 1.45;
	let w = NOTICE.iter().map(|l| text_width(l, size)).fold(0.0, f32::max) + 24.0 * k;
	rect(out, [6.0 * k, y0, w, NOTICE.len() as f32 * lh + 12.0 * k], [0.0, 0.0, 0.0, 0.75]);
	for (i, l) in NOTICE.iter().enumerate() {
		let colour = if i == 0 { [1.0, 0.8, 0.3, 1.0] } else if i >= NOTICE.len() - 2 { [1.0, 0.55, 0.45, 1.0] } else { [0.92, 0.92, 0.92, 1.0] };
		text(out, l, 12.0 * k, y0 + 6.0 * k + i as f32 * lh, size, colour);
	}
}

fn draw_list(out: &mut Vec<Quad>, w: f32, h: f32, k: f32, row: usize) {
	let size = 16.0 * k;
	let line_h = size * 1.35;
	let rows = LEVELS as f32 + 2.5;
	let pw = text_width(HELP_PICK, size * 0.8).max(NAMES.iter().map(|n| text_width(&format!("00  {n}  (HERE)"), size)).fold(0.0, f32::max)) + 32.0 * k;
	let ph = rows * line_h + 24.0 * k;
	let (x0, y0) = ((w - pw) / 2.0, ((h - ph) / 2.0).max(4.0));
	rect(out, [x0, y0, pw, ph], [0.0, 0.0, 0.0, 0.75]);
	text(out, "WARP TO PLANET (RETAIL LEVEL LOAD)", x0 + 16.0 * k, y0 + 12.0 * k, size, [1.0, 0.8, 0.3, 1.0]);
	let cur = CURRENT.load(Ordering::Relaxed) as usize;
	for l in 0..LEVELS {
		let y = y0 + 12.0 * k + (l as f32 + 1.4) * line_h;
		if l == row {
			rect(out, [x0 + 8.0 * k, y - 3.0 * k, pw - 16.0 * k, line_h], [0.25, 0.45, 0.9, 0.8]);
		}
		let t = format!("{l:02}  {}{}", level_name(l), if l == cur { "  (HERE)" } else { "" });
		text(out, &t, x0 + 16.0 * k, y, size, if l == row { [1.0, 1.0, 1.0, 1.0] } else { [0.8, 0.8, 0.8, 1.0] });
	}
	text(out, HELP_PICK, x0 + 16.0 * k, y0 + 12.0 * k + (LEVELS as f32 + 1.6) * line_h, size * 0.8, [0.7, 0.7, 0.7, 1.0]);
}

/// The on-screen part (render pixels `w` x `h`), drawn over the HUD: the notice, the readout line while a tool is up or an event is
/// fresh, the free camera's help line, the planet list. `clean` (F11 Hide HUD in gameplay): only the planet list while it is open
/// (R&C1's strip_hud clears the whole 2D layer in gameplay, its readout too; the list stays here since it owns the input).
pub fn draw(out: &mut Vec<Quad>, w: f32, h: f32, clean: bool) {
	if !on() {
		return;
	}
	let u = ui();
	let k = (h / 720.0).max(0.6);
	let size = 16.0 * k;
	if clean {
		if let Some(row) = u.pick {
			draw_list(out, w, h, k, row);
		}
		return;
	}
	if u.notice.is_some_and(|at| at.elapsed() < NOTICE_FOR) {
		draw_notice(out, k, 8.0 * k + size + 12.0 * k, w);
	}
	let free = crate::pc_freecam::on();
	let shown = free || u.pick.is_some() || u.note.as_ref().is_some_and(|(_, at)| at.elapsed() < NOTE_FOR);
	if shown {
		let line = readout(&u);
		let rs = fit(size, [line.as_str()], w - 24.0 * k);
		rect(out, [6.0 * k, 6.0 * k, text_width(&line, rs) + 12.0 * k, rs + 8.0 * k], [0.0, 0.0, 0.0, 0.55]);
		text(out, &line, 12.0 * k, 10.0 * k, rs, [1.0, 1.0, 0.6, 1.0]);
	}
	if free && u.pick.is_none() {
		let hs = fit(size * 0.75, [crate::pc_freecam::HELP], w - 24.0 * k);
		text(out, crate::pc_freecam::HELP, 12.0 * k, h - 12.0 * k - hs, hs, [0.85, 0.85, 0.85, 1.0]);
	}
	if let Some(row) = u.pick {
		draw_list(out, w, h, k, row);
	}
}

/// A feed key name: F1..F12 (not bindable, so not in the options-file names), else an options-file name.
fn feed_key(name: &str) -> Option<K> {
	const F: [K; 12] = [K::F1, K::F2, K::F3, K::F4, K::F5, K::F6, K::F7, K::F8, K::F9, K::F10, K::F11, K::F12];
	let name = name.trim();
	if let Some(n) = name.strip_prefix('F').and_then(|n| n.parse::<usize>().ok()) {
		return F.get(n.wrapping_sub(1)).copied();
	}
	match rac_engine::input::kbm::names::from_name(name) {
		Some(rac_engine::input::kbm::Input::Key(k)) => Some(k),
		_ => None,
	}
}

/// A feed pad token's buttons (`pad:L3+R3`, PadState bits), None for an unknown name.
fn pad_bits(v: &str) -> Option<u16> {
	const N: [&str; 16] = ["L2", "R2", "L1", "R1", "TRIANGLE", "CIRCLE", "CROSS", "SQUARE", "SELECT", "L3", "R3", "START", "UP", "RIGHT", "DOWN", "LEFT"];
	v.split('+').try_fold(0u16, |b, n| N.iter().position(|x| x.eq_ignore_ascii_case(n.trim())).map(|i| b | 1 << i))
}

/// The feed's pad buttons held during the current field (`pad:` tokens; debug_route::pad adds them to the real pad), 0 = none.
static FED_PAD: AtomicU32 = AtomicU32::new(0);

pub fn fed_pad() -> u16 {
	FED_PAD.load(Ordering::Relaxed) as u16
}

fn parse_feed(v: &str) -> Vec<(u64, String)> {
	v.split(';').filter_map(|e| e.split_once(':').and_then(|(f, a)| Some((f.trim().parse().ok()?, a.trim().to_string())))).collect()
}

/// Window thread, every poll: the test feed's key presses of game field `field` (once per field), for host.rs to send through
/// its Debug-mode key path. `notice` entries put the notice up.
pub fn feed(field: u64) -> Vec<K> {
	if !on() {
		return Vec::new();
	}
	let mut u = ui();
	let feed = u.feed.get_or_insert_with(|| parse_feed(&std::env::var("RAC2_DEVTOOLS_FEED").unwrap_or_default())).clone();
	if feed.is_empty() || u.fed == field {
		return Vec::new();
	}
	u.fed = field;
	FED_PAD.store(0, Ordering::Relaxed);
	let mut keys = Vec::new();
	for (_, a) in feed.iter().filter(|e| e.0 == field) {
		for name in a.split(',') {
			if name.trim() == "notice" {
				u.notice = Some(Instant::now());
			} else if let Some(b) = name.trim().strip_prefix("pad:").and_then(pad_bits) {
				FED_PAD.fetch_or(b as u32, Ordering::Relaxed);
			} else if let Some(k) = feed_key(name) {
				keys.push(k);
			} else {
				eprintln!("rac2 devtools: feed: unknown key {name}");
			}
		}
		eprintln!("rac2 devtools: field {field}: feed {a}");
	}
	keys
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_list_steps_wraps_goes_and_closes() {
		assert_eq!(list_step(0, K::ArrowUp, false), Ok(Some(LEVELS - 1)));
		assert_eq!(list_step(LEVELS - 1, K::KeyS, true), Ok(Some(0)));
		assert_eq!(list_step(3, K::PageUp, false), Ok(Some(0)));
		assert_eq!(list_step(24, K::PageDown, false), Ok(Some(LEVELS - 1)));
		assert_eq!(list_step(9, K::Enter, false), Ok(None));
		assert_eq!(list_step(9, K::Enter, true), Ok(Some(9)), "a held Enter does not warp");
		assert_eq!(list_step(9, K::Escape, false), Err(()));
		assert_eq!(list_step(9, K::F4, false), Err(()));
		assert_eq!(list_step(9, K::KeyQ, false), Ok(Some(9)), "other keys are taken but do nothing");
	}

	#[test]
	fn every_level_has_a_name_in_capitals_and_the_notice_names_the_r_and_c2_keys() {
		assert_eq!(NAMES.len(), 27);
		assert!(NAMES.iter().all(|n| !n.is_empty() && n.to_uppercase() == *n));
		assert_eq!((level_name(1), level_name(99)), ("OOZLA (MEGACORP OUTLET)", "?"));
		assert!(NOTICE.iter().all(|l| l.to_uppercase() == *l));
		assert!(NOTICE[1].starts_with("F4") && NOTICE[2].starts_with("F2") && NOTICE[4].starts_with("F7"));
		assert!(NOTICE[NOTICE.len() - 2].starts_with("WARNING"));
	}

	#[test]
	fn the_pad_steps_goes_and_closes_the_list() {
		assert_eq!(pad_step(0, PAD_UP), Ok(Some(LEVELS - 1)));
		assert_eq!(pad_step(LEVELS - 1, PAD_DOWN), Ok(Some(0)));
		assert_eq!(pad_step(4, PAD_CROSS), Ok(None));
		assert_eq!((pad_step(4, PAD_TRI), pad_step(4, PAD_CIRCLE)), (Err(()), Err(())));
		assert_eq!(pad_step(4, 0), Ok(Some(4)), "no new press");
	}

	#[test]
	fn the_feed_parses_fields_and_key_names() {
		assert_eq!(parse_feed("40:F4;60:ArrowDown,Enter;x;7"), vec![(40, "F4".to_string()), (60, "ArrowDown,Enter".to_string())]);
		assert!(parse_feed("").is_empty());
		assert_eq!([feed_key("F4"), feed_key("F11"), feed_key("F13"), feed_key("ArrowDown")], [Some(K::F4), Some(K::F11), None, Some(K::ArrowDown)]);
		assert_eq!([pad_bits("L3+R3"), pad_bits("start"), pad_bits("Cross+Up"), pad_bits("L3+X")], [Some(0x600), Some(0x800), Some(0x1040), None]);
	}

	/// The engine overlay font's characters (engine/overlay.rs `glyph`): anything else is drawn as a gap.
	const FONT: &str = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ .,!?:-+/\\\"()%&";

	#[test]
	fn a_long_line_shrinks_to_fit_a_narrow_window() {
		let w1 = text_width(NOTICE[7], 1.0);
		assert_eq!(fit(14.0, NOTICE, w1 * 20.0), 14.0, "room enough: unchanged");
		assert!((fit(14.0, NOTICE, w1 * 10.0) - 10.0).abs() < 1e-4, "the widest line fills the room");
	}

	#[test]
	fn every_on_screen_text_uses_only_the_overlay_font() {
		let texts = NOTICE.iter().chain(NAMES.iter()).chain([HELP_PICK, crate::pc_freecam::HELP].iter());
		for t in texts {
			assert!(t.chars().all(|c| FONT.contains(c)), "a character the overlay font lacks in {t:?}");
		}
	}
}
