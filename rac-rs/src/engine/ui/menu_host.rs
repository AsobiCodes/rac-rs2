//! The host of a modern Options page (engine side, game-neutral; moved from rac1/modern_menu/host.rs, wave 2 of lane rc2_par),
//! MAIN THREAD only: it owns the engine's `Ui` between "the game's open hook asked for the page" and "the player left the page".
//! The pack's window handler feeds it keys, mouse and focus loss as they come; the pack's display-frame step feeds it the pad, the
//! repeat clock and the values the worker published, and takes the draw list. While it is up `ui::is_open()` is true: the game gets
//! a neutral pad (`pad_filter`, called once per field by the pack's input), and the button that closed the page stays eaten until it
//! is released, so Circle / Esc never also backs out of the game's own menu behind it.
//!
//! A pack with more page hosts (R&C1: title, pause, shop, map, Galactic Map) routes keys and mouse to them first and calls
//! `page_key` / `page_mouse` / `lose_focus` here for this page; `key` / `mouse` are the whole route for a pack with this page only.
use super::font::FontMeasure;
use super::prim::Prim;
use super::{MouseEvent, Outcome, PadSample, Ui, UiKey};
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;
use winit::keyboard::KeyCode;

static UI: Mutex<Option<Ui>> = Mutex::new(None);
static START: OnceLock<Instant> = OnceLock::new();
/// The game pad's buttons of the last field (`pad_filter`'s input): what is held when the page closes stays eaten.
static LAST_BUTTONS: AtomicU16 = AtomicU16::new(0);
static EAT: AtomicU16 = AtomicU16::new(0);
/// The page was opened from the title (`open`'s `front`).
static FRONT: AtomicBool = AtomicBool::new(false);
/// A Shift key is held (window keys while a modern screen is up): Shift+Tab walks the tabs back.
static SHIFT: AtomicBool = AtomicBool::new(false);

/// Is a Shift key held (as the window's key events last said)?
pub fn shift() -> bool {
	SHIFT.load(Ordering::Relaxed)
}

/// Notes a Shift key's press or release (every window key goes through here first, whichever page host takes it).
pub fn note_shift(code: KeyCode, pressed: bool) {
	if matches!(code, KeyCode::ShiftLeft | KeyCode::ShiftRight) {
		SHIFT.store(pressed, Ordering::Relaxed);
	}
}

/// The UI's own millisecond clock (display rate, not the game's).
fn now_ms() -> u64 {
	START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

fn lock() -> std::sync::MutexGuard<'static, Option<Ui>> {
	UI.lock().unwrap_or_else(|e| e.into_inner())
}

/// A keyboard key as the UI's key (None = not a menu key).
pub fn map_key(code: KeyCode) -> Option<UiKey> {
	Some(match code {
		KeyCode::ArrowUp => UiKey::Up,
		KeyCode::ArrowDown => UiKey::Down,
		KeyCode::ArrowLeft => UiKey::Left,
		KeyCode::ArrowRight => UiKey::Right,
		KeyCode::KeyW => UiKey::W,
		KeyCode::KeyA => UiKey::A,
		KeyCode::KeyS => UiKey::S,
		KeyCode::KeyD => UiKey::D,
		KeyCode::Enter | KeyCode::NumpadEnter => UiKey::Enter,
		KeyCode::Space => UiKey::Space,
		KeyCode::Escape => UiKey::Esc,
		KeyCode::Backspace => UiKey::Backspace,
		KeyCode::KeyQ => UiKey::Q,
		KeyCode::KeyE => UiKey::E,
		KeyCode::Tab => UiKey::Tab,
		KeyCode::ShiftLeft | KeyCode::ShiftRight => UiKey::Shift,
		KeyCode::PageUp => UiKey::PageUp,
		KeyCode::PageDown => UiKey::PageDown,
		KeyCode::Home => UiKey::Home,
		KeyCode::End => UiKey::End,
		_ => return None,
	})
}

pub fn pad_sample(buttons: u16, sticks: [u8; 4]) -> PadSample {
	PadSample { buttons, lx: sticks[2], ly: sticks[3] }
}

/// Opens the page (main thread) on tab `tab` (a tab id of the pack's `UiMenu`); `front` = from the title's Options. `pad`: the pad
/// now, so a button still down from the press that opened it does not act. False = the pack has no modern menu (nothing opened).
pub fn open(front: bool, tab: &str, pad: Option<PadSample>) -> bool {
	let Some(mut ui) = Ui::from_pack(Box::new(FontMeasure)) else { return false };
	ui.begin(front, Some(tab));
	if let Some(p) = pad {
		ui.hold_pad(p);
	}
	ui.fix_scroll();
	*lock() = Some(ui);
	FRONT.store(front, Ordering::Relaxed);
	super::set_open(true);
	true
}

/// Leaves the page: pending changes go to the game first, the held buttons stay eaten until released.
fn finish(slot: &mut Option<Ui>) {
	if let Some(mut ui) = slot.take() {
		ui.flush_pack();
	}
	EAT.store(LAST_BUTTONS.load(Ordering::Relaxed), Ordering::Relaxed);
	super::set_open(false);
}

/// Closes the page from outside (the menu behind it went away, the UI pipes were refused).
pub fn close() {
	finish(&mut lock());
}

/// One winit key for this page (after `note_shift` and the pack's other page hosts). True = the press is the page's (the game must
/// not see it); a release always goes on to the game so no key sticks. OS key repeats are dropped (the page has its own repeat, and
/// a repeat of the Enter that opened it must not act).
pub fn page_key(code: KeyCode, pressed: bool, repeat: bool) -> bool {
	let mut g = lock();
	let Some(ui) = g.as_mut() else { return false };
	let mapped = map_key(code);
	if repeat {
		return mapped.is_some();
	}
	let Some(k) = mapped else { return false };
	if ui.key(k, pressed, now_ms()) == Outcome::Close {
		finish(&mut g);
	}
	pressed
}

/// One winit key, the whole route of a pack with this page only (`note_shift` + `page_key`).
pub fn key(code: KeyCode, pressed: bool, repeat: bool) -> bool {
	note_shift(code, pressed);
	page_key(code, pressed, repeat)
}

/// A mouse event in window pixels for this page. True = taken (the game's menu behind must not move or click).
pub fn page_mouse(ev: MouseEvent) -> bool {
	let mut g = lock();
	let Some(ui) = g.as_mut() else { return false };
	if ui.mouse(ev, now_ms()) == Outcome::Close {
		finish(&mut g);
	}
	true
}

/// The window lost the focus: forget held keys, buttons and a slider drag (the pack's other page hosts and its own pulses first).
pub fn lose_focus() {
	EAT.store(0, Ordering::Relaxed);
	SHIFT.store(false, Ordering::Relaxed);
	if let Some(ui) = lock().as_mut() {
		ui.lose_focus();
	}
}

/// Display-frame step: the pad, the repeat clock, the values the worker published; `valid` = the game's menu the page was opened
/// from is still up (false closes it). Returns the draw list, or None once the page is not up.
pub fn step(win: (u32, u32), pad: Option<PadSample>, valid: bool) -> Option<Vec<Prim>> {
	let mut g = lock();
	let ui = g.as_mut()?;
	let now = now_ms();
	ui.set_window(win.0, win.1);
	let mut out = ui.tick(now);
	if let Some(p) = pad {
		out = if ui.pad(p, now) == Outcome::Close { Outcome::Close } else { out };
	}
	ui.flush_pack();
	ui.sync_pack();
	if out == Outcome::Close || !valid {
		finish(&mut g);
		return None;
	}
	Some(ui.frame().1)
}

/// The pack's input, once per field with the field's merged pad: true = the game gets a neutral pad (the page is up, or the button
/// that closed it is still down).
pub fn pad_filter(buttons: u16) -> bool {
	LAST_BUTTONS.store(buttons, Ordering::Relaxed);
	let eat = EAT.load(Ordering::Relaxed) & buttons;
	EAT.store(eat, Ordering::Relaxed);
	super::is_open() || eat != 0
}

/// Whether the game's menu overlay is hidden behind the page: over the pause menu yes (the world shows through the blur), over the
/// title no (the title's picture IS the overlay).
pub fn hides_retail() -> bool {
	super::is_open() && !FRONT.load(Ordering::Relaxed)
}

/// The page is up over the title: the pack's native title menu is not drawn, the title scene stays behind the blur.
pub fn hides_title_menu() -> bool {
	super::is_open() && FRONT.load(Ordering::Relaxed)
}

/// The page was opened from the title's Options list (not the pause menu's).
pub fn from_title() -> bool {
	FRONT.load(Ordering::Relaxed)
}

#[cfg(test)]
pub(crate) fn eat_for_test() -> u16 {
	EAT.load(Ordering::Relaxed)
}

/// Test door: the page closes with `buttons` held in the last field (what `finish` does with the eaten mask).
#[cfg(test)]
pub(crate) fn close_for_test(buttons: u16) {
	LAST_BUTTONS.store(buttons, Ordering::Relaxed);
	finish(&mut None);
}

#[cfg(test)]
mod tests {
	use super::*;
	/// Any two distinct pad bits (Circle is 0x20 in the PS2 report).
	const CIRCLE: u16 = 0x20;
	const OTHER: u16 = 0x40;

	/// The button that closed the page stays away from the game until it is released; a neutral pad while the page is up.
	#[test]
	fn eat_keeps_closing_button_from_game() {
		crate::ui::set_open(false);
		close_for_test(CIRCLE);
		assert_eq!(eat_for_test(), CIRCLE);
		assert!(pad_filter(CIRCLE), "still down: eaten");
		assert!(pad_filter(CIRCLE | OTHER), "other buttons are neutral too while one is eaten");
		assert!(!pad_filter(OTHER), "released: the game gets the pad again");
		assert_eq!(eat_for_test(), 0);
		crate::ui::set_open(true);
		assert!(pad_filter(0), "page up: neutral");
		crate::ui::set_open(false);
	}

	#[test]
	fn menu_keys_map() {
		assert_eq!(map_key(KeyCode::NumpadEnter), Some(UiKey::Enter));
		assert_eq!(map_key(KeyCode::F11), None);
		assert!(!hides_retail() || crate::ui::is_open());
	}

	#[test]
	fn shift_noted_and_forgotten() {
		note_shift(KeyCode::ShiftLeft, true);
		assert!(shift());
		note_shift(KeyCode::KeyA, false);
		assert!(shift(), "other keys leave it");
		lose_focus();
		assert!(!shift(), "a focus loss forgets it");
	}
}
