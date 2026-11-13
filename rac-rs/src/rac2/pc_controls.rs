//! [PC] R&C2's Controls tab (parity row 43, lane inp; R&C1 `modern_menu/pause_keys.rs` for the idea): one row per action of R&C2's
//! table (`kbm_table.rs ACTIONS`), the value text is the binding ("Space / G", "LMB / G", "-" for an empty slot). Enter, a click or
//! Right on a row captures its first slot, Left the second one, Backspace (or the pad's Square) clears the last used slot; the row
//! "Reset keyboard and mouse" puts every default back. A capture takes the next key, mouse button (left, right, middle, the two
//! side buttons) or wheel notch (`event`, window thread); Esc, or 8 seconds of nothing, cancels it. Rows are `RowKind::Bind`
//! (engine/ui): the engine reports 1 / 2 / 3 through `Pack::ui` `set` (`act`), `get` answers 0 and `value` keeps the text fresh.
//! Changes are saved by `tick` (every redraw) as the options file's `kbm=` line (engine `kbm::take_changes`); the old `keys=` and
//! `pad=` lines are dropped then, since `kbm=` / `gamepad=` replace them. The wheel: a notch bound to an action presses it for
//! the tap latch's TAP_FIELDS fields (`note_wheel` -> pc_taps.rs, host.rs `poll_input`; [b46 W6] C5: was a 70 ms window).
use crate::modern_rows::row;
use rac_engine::input::kbm::{self, names, Cap, Input, Mouse, Wheel};
use rac_engine::ui::{live, Cost, RowKind, UiRow, T5};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::PhysicalKey;

/// A wheel notch counts as a press for this long (the game reads the pad once per 20 ms field).
/// A capture waits this long.
const CAPTURE_SECS: u64 = 8;

// Not retail text (the table's own words; native-speaker review wanted, as R&C1's Controls tab).
const PRESS_KEY: T5 = ["Press a key/button", "Touche ou bouton", "Taste/Maustaste", "Tecla o bot\u{f3}n", "Tasto o pulsante"];
const REFUSED: T5 = [
	"Pause must keep one key or button",
	"Pause doit garder une touche ou un bouton",
	"Pause muss eine Taste oder einen Knopf behalten",
	"Pausa debe conservar una tecla o bot\u{f3}n",
	"Pausa deve mantenere un tasto o pulsante",
];
const DESC: T5 = [
	"Enter or click: set the first key or mouse button. Left: set the second one. Backspace: clear it. Esc cancels.",
	"Entr\u{e9}e ou clic : premi\u{e8}re touche ou bouton souris. Gauche : la deuxi\u{e8}me. Retour arri\u{e8}re : effacer. \u{c9}chap annule.",
	"Enter oder Klick: erste Taste oder Maustaste. Links: die zweite. R\u{fc}cktaste: l\u{f6}schen. Esc bricht ab.",
	"Intro o clic: primera tecla o bot\u{f3}n del rat\u{f3}n. Izquierda: la segunda. Retroceso: borrar. Esc cancela.",
	"Invio o clic: primo tasto o pulsante del mouse. Sinistra: il secondo. Backspace: cancella. Esc annulla.",
];
const RESET_LABEL: T5 = ["Reset keyboard and mouse", "R\u{e9}initialiser clavier et souris", "Tastatur und Maus zur\u{fc}cksetzen", "Restaurar teclado y rat\u{f3}n", "Ripristina tastiera e mouse"];
const RESET_DESC: T5 = [
	"Puts every key and mouse button back to the defaults.",
	"Remet toutes les touches et tous les boutons de la souris par d\u{e9}faut.",
	"Setzt alle Tasten und Maustasten auf die Standardwerte zur\u{fc}ck.",
	"Devuelve todas las teclas y botones del rat\u{f3}n a su valor original.",
	"Riporta tutti i tasti e i pulsanti del mouse ai valori predefiniti.",
];
const fn w(s: &'static str) -> T5 {
	[s, s, s, s, s]
}

/// The row labels, in `kbm_table::ACTIONS` order. The pad's own names where the game's meaning of a button is not known [I]
/// (L1, R1, L2, L3, R3, Select); the right mouse button is R2, the strafe aim.
pub static LABELS: [T5; 28] = [
	["Jump", "Saut", "Springen", "Saltar", "Salto"],
	["Wrench", "Cl\u{e9}", "Schl\u{fc}ssel", "Llave", "Chiave"],
	["Fire", "Tir", "Feuer", "Disparar", "Fuoco"],
	["Quick Select", "S\u{e9}lection rapide", "Schnellwahl", "Selecci\u{f3}n r\u{e1}pida", "Selezione rapida"],
	w("L1"),
	w("R1"),
	w("L2"),
	["R2 (strafe aim)", "R2 (vis\u{e9}e)", "R2 (Zielen)", "R2 (apuntar)", "R2 (mira)"],
	w("L3"),
	w("R3"),
	["Pause", "Pause", "Pause", "Pausa", "Pausa"],
	w("Select"),
	["D-pad up", "Croix haut", "Steuerkreuz hoch", "Cruceta arriba", "Croce su"],
	["D-pad down", "Croix bas", "Steuerkreuz runter", "Cruceta abajo", "Croce gi\u{f9}"],
	["D-pad left", "Croix gauche", "Steuerkreuz links", "Cruceta izquierda", "Croce sinistra"],
	["D-pad right", "Croix droite", "Steuerkreuz rechts", "Cruceta derecha", "Croce destra"],
	["Move forward", "Avancer", "Vorw\u{e4}rts", "Avanzar", "Avanti"],
	["Move back", "Reculer", "R\u{fc}ckw\u{e4}rts", "Retroceder", "Indietro"],
	["Move left", "Aller \u{e0} gauche", "Nach links", "Mover a la izquierda", "Muovi a sinistra"],
	["Move right", "Aller \u{e0} droite", "Nach rechts", "Mover a la derecha", "Muovi a destra"],
	["Camera up", "Cam\u{e9}ra haut", "Kamera hoch", "C\u{e1}mara arriba", "Telecamera su"],
	["Camera down", "Cam\u{e9}ra bas", "Kamera runter", "C\u{e1}mara abajo", "Telecamera gi\u{f9}"],
	["Camera left", "Cam\u{e9}ra gauche", "Kamera links", "C\u{e1}mara izquierda", "Telecamera sinistra"],
	["Camera right", "Cam\u{e9}ra droite", "Kamera rechts", "C\u{e1}mara derecha", "Telecamera destra"],
	["Aim (strafe, R2)", "Viser (R2)", "Zielen (R2)", "Apuntar (R2)", "Mira (R2)"],
	["Zoom in (PC)", "Zoom + (PC)", "Zoom + (PC)", "Zoom + (PC)", "Zoom + (PC)"],
	["Zoom out (PC)", "Zoom - (PC)", "Zoom - (PC)", "Zoom - (PC)", "Zoom - (PC)"],
	["Ship fire", "Tir vaisseau", "Schiffsfeuer", "Disparo nave", "Fuoco nave"],
];

const fn bind(id: &'static str, i: usize) -> UiRow {
	row(id, RowKind::Bind, LABELS[i], DESC, &[], 0, Cost::None, None)
}

pub static ACTIONS: [UiRow; 12] = [
	bind("bind_jump", 0),
	bind("bind_wrench", 1),
	bind("bind_fire", 2),
	bind("bind_quick_select", 3),
	bind("bind_l1", 4),
	bind("bind_r1", 5),
	bind("bind_l2", 6),
	bind("bind_r2", 7),
	bind("bind_l3", 8),
	bind("bind_r3", 9),
	bind("bind_pause", 10),
	bind("bind_select", 11),
];
pub static DPAD: [UiRow; 4] = [bind("bind_up", 12), bind("bind_down", 13), bind("bind_left", 14), bind("bind_right", 15)];
pub static MOVE: [UiRow; 4] = [bind("bind_forward", 16), bind("bind_back", 17), bind("bind_strafe_left", 18), bind("bind_strafe_right", 19)];
pub static CAMERA: [UiRow; 4] = [bind("bind_camera_up", 20), bind("bind_camera_down", 21), bind("bind_camera_left", 22), bind("bind_camera_right", 23)];
pub static PC: [UiRow; 4] = [bind("bind_aim", 24), bind("bind_zoom_in", 25), bind("bind_zoom_out", 26), bind("bind_ship_fire", 27)];
pub static RESET: [UiRow; 2] = [row("bind_reset", RowKind::Bind, RESET_LABEL, RESET_DESC, &[], 0, Cost::None, None), crate::pad_binds::RESET_PAD];

pub const T_CONTROLS: T5 = ["Controls", "Commandes", "Steuerung", "Controles", "Comandi"];
pub const S_ACTIONS: T5 = ["Actions", "Actions", "Aktionen", "Acciones", "Azioni"];
pub const S_DPAD: T5 = ["D-pad", "Croix", "Steuerkreuz", "Cruceta", "Croce"];
pub const S_MOVE: T5 = ["Movement", "Mouvement", "Bewegung", "Movimiento", "Movimento"];
pub const S_CAMERA: T5 = ["Camera", "Cam\u{e9}ra", "Kamera", "C\u{e1}mara", "Telecamera"];
pub const S_PC: T5 = ["PC extras", "Extras PC", "PC-Extras", "Extras de PC", "Extra PC"];
pub const S_RESET: T5 = ["Reset", "R\u{e9}initialiser", "Zur\u{fc}cksetzen", "Restaurar", "Ripristina"];

/// When the capture began, the action whose Pause refusal shows, and the wheel notches' time.
struct State {
	cap_since: Option<Instant>,
	refused: Option<(usize, Instant)>,
}
static STATE: Mutex<State> = Mutex::new(State { cap_since: None, refused: None });

fn lang() -> usize {
	crate::options::get().lang.map_or(0, |l| [rac_engine::pcopts::Lang::En, rac_engine::pcopts::Lang::Fr, rac_engine::pcopts::Lang::De, rac_engine::pcopts::Lang::Es, rac_engine::pcopts::Lang::It].iter().position(|&x| x == l).unwrap_or(0))
}

/// The action a row id names ("bind_jump" -> index of "jump").
fn action_of(id: &str) -> Option<usize> {
	id.strip_prefix("bind_").and_then(|a| kbm::table().index(a))
}

/// The text of action `a`'s row: the capture prompt while it waits, a refusal for a few seconds, else "first / second".
pub fn text(a: usize) -> String {
	let l = lang();
	if crate::pad_binds::view_pad() {
		let refused = STATE.lock().unwrap_or_else(|e| e.into_inner()).refused.is_some_and(|(x, t)| x == a && t.elapsed() < Duration::from_millis(2500));
		return crate::pad_binds::text(a, l, refused); // [b45 P6] C16: the controller view
	}
	if matches!(kbm::capture_state(), Cap::Kbm(x, _) if x == a) {
		return PRESS_KEY[l.min(4)].to_string();
	}
	if STATE.lock().unwrap_or_else(|e| e.into_inner()).refused.is_some_and(|(x, t)| x == a && t.elapsed() < Duration::from_millis(2500)) {
		return REFUSED[l.min(4)].to_string();
	}
	let b = kbm::binds();
	let n = |i: Option<Input>| i.map_or("-".to_string(), names::short_name);
	format!("{} / {}", n(b.0[a][0]), n(b.0[a][1]))
}

/// `Pack::ui` `get` for a Bind row: refreshes the row's text, the value is always 0.
pub fn value(id: &str) -> Option<i32> {
	if id == "bind_view" {
		return Some(crate::pad_binds::view_pad() as i32); // [b45 P6] C16
	}
	let r = crate::modern_menu::menu().find(id)?;
	if r.kind != RowKind::Bind {
		return None;
	}
	live::set(r.id, 0, action_of(id).map_or_else(String::new, text));
	Some(0)
}

/// `Pack::ui` `set` for a Bind row: 1 / 2 capture slot 1 / 2, 3 clear, "bind_reset" 1 resets everything. True = it was a Bind row.
pub fn act(id: &str, v: i32) -> bool {
	if id == "bind_view" {
		crate::pad_binds::set_view(v);
		return true;
	}
	if id == "bind_reset_pad" {
		if v == 1 {
			kbm::reset_pad();
		}
		return true;
	}
	if id == "bind_reset" {
		if v == 1 {
			kbm::reset_kbm();
		}
		return true;
	}
	let Some(a) = action_of(id) else { return false };
	if crate::pad_binds::view_pad() {
		if crate::pad_binds::act(a, v) == Some(true) {
			STATE.lock().unwrap_or_else(|e| e.into_inner()).refused = Some((a, Instant::now()));
		}
		if matches!(v, 1 | 2) && kbm::capturing() {
			STATE.lock().unwrap_or_else(|e| e.into_inner()).cap_since = Some(Instant::now());
		}
		return true;
	}
	match v {
		1 | 2 => {
			kbm::capture_begin(Cap::Kbm(a, v as usize - 1));
			STATE.lock().unwrap_or_else(|e| e.into_inner()).cap_since = Some(Instant::now());
		}
		3 => {
			let b = kbm::binds();
			let slot = if b.0[a][1].is_some() { 1 } else { 0 };
			if !kbm::clear(a, slot) {
				STATE.lock().unwrap_or_else(|e| e.into_inner()).refused = Some((a, Instant::now()));
			}
		}
		_ => {}
	}
	true
}

/// The input a window event is, when it is a fresh press / notch (None for releases, repeats and other events).
fn pressed(ev: &WindowEvent) -> Option<Input> {
	Some(match ev {
		WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed && !event.repeat => match event.physical_key {
			PhysicalKey::Code(k) => Input::Key(k),
			_ => return None,
		},
		WindowEvent::MouseInput { state: ElementState::Pressed, button, .. } => Input::Mouse(match button {
			MouseButton::Left => Mouse::Left,
			MouseButton::Right => Mouse::Right,
			MouseButton::Middle => Mouse::Middle,
			MouseButton::Back => Mouse::Back,
			MouseButton::Forward => Mouse::Forward,
			MouseButton::Other(_) => return None,
		}),
		WindowEvent::MouseWheel { delta, .. } => {
			let y = match delta {
				MouseScrollDelta::LineDelta(_, y) => *y,
				MouseScrollDelta::PixelDelta(p) => p.y as f32 / 120.0,
			};
			if y == 0.0 {
				return None;
			}
			Input::Wheel(if y > 0.0 { Wheel::Up } else { Wheel::Down })
		}
		_ => return None,
	})
}

/// Window thread, first in the menu route (host/menu.rs `menu_event`): while a capture waits, the press or notch binds (Esc
/// cancels) and the event is the capture's alone; releases go on so nothing stays held. True = taken.
pub fn event(ev: &WindowEvent) -> bool {
	if !kbm::capturing() {
		return false;
	}
	match pressed(ev) {
		Some(i) => {
			kbm::capture_poll(&[i], 0);
			true
		}
		None => matches!(ev, WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed),
	}
}

/// Window thread, every redraw: the capture's time limit and the save of a changed binding.
pub fn tick() {
	if kbm::capturing() {
		let late = STATE.lock().unwrap_or_else(|e| e.into_inner()).cap_since.is_some_and(|t| t.elapsed() > Duration::from_secs(CAPTURE_SECS));
		if late {
			kbm::capture_cancel();
		}
	}
	if let Some((k, g)) = kbm::take_changes() {
		let r = crate::options::change(|o| {
			o.extra_set("kbm", k);
			o.extra_set("gamepad", g);
			o.extra_set("keys", None);
			o.extra_set("pad", None);
		});
		if let Err(e) = r {
			eprintln!("rac2: PC options: cannot save the key bindings: {e}");
		}
	}
}

/// A wheel notch happened (host.rs `MouseWheel`).
pub fn note_wheel(lines: f32) {
	// [b46 W6] C5: a notch is a tap like a key press (R&C1 press_wheel): held TAP_FIELDS fields from the first poll after it.
	if lines != 0.0 {
		if let Ok(mut t) = crate::pc_taps::TAPS.lock() {
			t.press(Input::Wheel(if lines > 0.0 { Wheel::Up } else { Wheel::Down }));
		}
	}
}

/// A wheel notch of this direction (`lines` > 0 = up) is bound to an action on the Controls tab: it presses that action only, it
/// neither steps a retail menu nor zooms the camera.
pub fn wheel_bound(lines: f32) -> bool {
	crate::pc_taps::wheel_route(&kbm::binds(), lines).0
}

fn bound_in(b: &kbm::Binds, w: Wheel) -> bool {
	b.0.iter().flatten().any(|i| *i == Some(Input::Wheel(w)))
}

/// A wheel notch has no held state (it is a tap: `note_wheel`, pc_taps.rs `alive`).
pub fn wheel_active(_w: Wheel) -> bool {
	false
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn every_row_names_an_action_and_the_labels_match() {
		rac_engine::input::kbm::ensure_table(&crate::kbm_table::TABLE);
		let t = kbm::table();
		let rows = ACTIONS.iter().chain(&DPAD).chain(&MOVE).chain(&CAMERA).chain(&PC);
		let mut n = 0;
		for r in rows {
			assert!(action_of(r.id).is_some(), "{} names no action of the table", r.id);
			assert_eq!(r.kind, RowKind::Bind);
			n += 1;
		}
		assert_eq!(n, t.actions.len(), "one row per action");
		assert_eq!(LABELS.len(), t.actions.len());
	}

	#[test]
	fn a_captured_key_rebinds_the_action_and_is_saved_as_the_kbm_line() {
		rac_engine::input::kbm::ensure_table(&crate::kbm_table::TABLE);
		kbm::reset_kbm();
		let _ = kbm::take_changes();
		let a = action_of("bind_jump").unwrap();
		assert!(act("bind_jump", 1));
		assert!(kbm::capturing());
		assert_eq!(text(a), PRESS_KEY[0]);
		kbm::capture_poll(&[Input::Key(winit::keyboard::KeyCode::KeyN)], 0);
		assert!(!kbm::capturing());
		assert_eq!(kbm::binds().0[a][0], Some(Input::Key(winit::keyboard::KeyCode::KeyN)));
		assert!(text(a).starts_with("N / "), "{}", text(a));
		let (k, _) = kbm::take_changes().expect("a change is queued for the options file");
		assert!(k.is_some_and(|s| s.contains("jump")));
		// Clear, then Reset puts the default back.
		assert!(act("bind_jump", 3));
		assert!(act("bind_reset", 1));
		assert_eq!(kbm::binds().0[a][0], Some(Input::Key(winit::keyboard::KeyCode::Space)));
	}

	#[test]
	fn a_bound_wheel_notch_is_found_per_direction() {
		let mut b = kbm::Binds::defaults(&crate::kbm_table::TABLE);
		assert!(!wheel_bound(1.0) && !wheel_bound(-1.0), "the default wheel is Zoom: menus step, the camera zooms");
		b.0[3][1] = Some(Input::Wheel(Wheel::Up));
		assert!(bound_in(&b, Wheel::Up) && bound_in(&b, Wheel::Down), "up: the pad action and Zoom in; down: Zoom out (a PC row)");
		assert!(crate::pc_taps::wheel_route(&b, 1.0).0, "a pad action on the wheel taps only itself");
		assert!(!crate::pc_taps::wheel_route(&b, -1.0).0, "the other direction is still only Zoom out");
	}
}
