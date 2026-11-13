//! [PC] Controller remap on the Controls tab (b45 P6, sweep C16; R&C1 `modern_menu/pause_keys.rs` `view_pad`, engine `kbm` pad slots,
//! design `V051_PAD_DESIGN.md`). A "Show" row at the top of the tab switches the bind rows between the keyboard + mouse view
//! (`pc_controls.rs`) and the controller view: each row then names the (up to two) controller buttons of its action, Enter / a click
//! waits for the next controller button (`kbm::Cap::Pad`, polled in host.rs `poll_input`), Backspace clears one. Only actions with a
//! controller slot (`ActionDef::pad_ok`: the 16 buttons) can be bound; the sticks and the PC rows say "Keyboard only". The default is
//! the PS2-identical layout, and menus ALWAYS read the default layout (`menu_binds`), so a remap cannot lock the player out of a menu.
//! Saved by `pc_controls::tick` as the options file's `gamepad=` line (engine `kbm::take_changes`).
use crate::modern_rows::row;
use rac_engine::input::binds::PHYS;
use rac_engine::input::kbm::{self, Binds, Cap};
use rac_engine::ui::{Cost, RowKind, UiRow, T5};
use std::sync::atomic::{AtomicU8, Ordering};

/// 0 keyboard + mouse, 1 controller (the Controls tab's Show row).
static VIEW: AtomicU8 = AtomicU8::new(0);

pub fn view_pad() -> bool {
	VIEW.load(Ordering::Relaxed) == 1
}

pub fn set_view(v: i32) {
	VIEW.store(v.clamp(0, 1) as u8, Ordering::Relaxed);
}

const OPTIONS: &[T5] = &[
	["Keyboard and mouse", "Clavier et souris", "Tastatur und Maus", "Teclado y rat\u{f3}n", "Tastiera e mouse"],
	["Controller", "Manette", "Controller", "Mando", "Controller"],
];
const VIEW_DESC: T5 = [
	"Which device the rows below show and change. The controller keeps the PS2 layout unless you change it; menus always use the standard layout.",
	"Quel p\u{e9}riph\u{e9}rique les lignes ci-dessous montrent et modifient. La manette garde la disposition PS2 sauf changement ; les menus utilisent toujours la disposition standard.",
	"Welches Ger\u{e4}t die Zeilen unten zeigen und \u{e4}ndern. Der Controller beh\u{e4}lt das PS2-Layout, bis du es \u{e4}nderst; Men\u{fc}s nutzen immer das Standardlayout.",
	"Qu\u{e9} dispositivo muestran y cambian las filas de abajo. El mando conserva la distribuci\u{f3}n de PS2 salvo cambio; los men\u{fa}s usan siempre la est\u{e1}ndar.",
	"Quale dispositivo mostrano e cambiano le righe sotto. Il controller mantiene il layout PS2 salvo modifiche; i menu usano sempre quello standard.",
];
pub static VIEW_ROWS: [UiRow; 1] = [row("bind_view", RowKind::Choice, ["Show", "Afficher", "Anzeigen", "Mostrar", "Mostra"], VIEW_DESC, OPTIONS, 0, Cost::None, None)];
pub const S_VIEW: T5 = ["View", "Vue", "Ansicht", "Vista", "Vista"];

const RESET_PAD_LABEL: T5 = ["Reset controller", "R\u{e9}initialiser la manette", "Controller zur\u{fc}cksetzen", "Restaurar mando", "Ripristina controller"];
const RESET_PAD_DESC: T5 = [
	"Puts every controller button back to the PS2 layout.",
	"Remet tous les boutons de la manette sur la disposition PS2.",
	"Setzt alle Controllerkn\u{f6}pfe auf das PS2-Layout zur\u{fc}ck.",
	"Devuelve todos los botones del mando a la distribuci\u{f3}n de PS2.",
	"Riporta tutti i pulsanti del controller al layout PS2.",
];
pub const RESET_PAD: UiRow = row("bind_reset_pad", RowKind::Bind, RESET_PAD_LABEL, RESET_PAD_DESC, &[], 0, Cost::None, None);

const PRESS_PAD: T5 = ["Press a button", "Appuyez sur un bouton", "Knopf dr\u{fc}cken", "Pulsa un bot\u{f3}n", "Premi un pulsante"];
const KEYBOARD_ONLY: T5 = ["Keyboard only", "Clavier seul", "Nur Tastatur", "Solo teclado", "Solo tastiera"];
const REFUSED: T5 = [
	"Pause must keep one button",
	"Pause doit garder un bouton",
	"Pause muss einen Knopf behalten",
	"Pausa debe conservar un bot\u{f3}n",
	"Pausa deve mantenere un pulsante",
];

/// A controller button as the row shows it: the Xbox name and the PS2 name ("A (Cross)").
pub fn button_name(p: u8) -> String {
	let (_, _, x, ps) = PHYS[p as usize % PHYS.len()];
	if x == ps { x.to_string() } else { format!("{x} ({ps})") }
}

fn n(p: Option<u8>) -> String {
	p.map_or("-".to_string(), button_name)
}

/// The text of action `a`'s row in the controller view; `waiting`/`refused`: the capture prompt / refusal is up.
pub fn text(a: usize, lang: usize, refused: bool) -> String {
	let l = lang.min(4);
	let t = kbm::table();
	if a >= t.actions.len() || !t.actions[a].pad_ok {
		return KEYBOARD_ONLY[l].to_string();
	}
	if matches!(kbm::capture_state(), Cap::Pad(x, _) if x == a) {
		return PRESS_PAD[l].to_string();
	}
	if refused {
		return REFUSED[l].to_string();
	}
	let b = kbm::binds();
	format!("{} / {}", n(b.1[a][0]), n(b.1[a][1]))
}

/// `Pack::ui` `set` of a Bind row in the controller view: 1 / 2 wait for a button for slot 1 / 2, 3 clears. Returns Some(refused)
/// when handled (a refusal = the clear was not allowed).
pub fn act(a: usize, v: i32) -> Option<bool> {
	let t = kbm::table();
	if a >= t.actions.len() || !t.actions[a].pad_ok {
		return Some(false);
	}
	match v {
		1 | 2 => {
			kbm::capture_begin(Cap::Pad(a, v as usize - 1));
			Some(false)
		}
		3 => {
			let b = kbm::binds();
			let slot = if b.1[a][1].is_some() { 1 } else { 0 };
			Some(!kbm::clear_pad(a, slot))
		}
		_ => Some(false),
	}
}

/// The layout the controller gets this field: the player's remap in play, the standard one in menus (R&C1 `prompts.rs`).
pub fn game_binds(menu: bool) -> Binds {
	if menu { Binds::defaults(kbm::table()) } else { kbm::binds() }
}

/// Window thread, every poll with the controller's physical buttons: a pad capture binds the first button that went down since
/// the last poll. Returns true while a pad capture waits (the game then gets no controller buttons).
pub fn poll_capture(phys: u32, prev: &mut u32) -> bool {
	let fresh = phys & !*prev;
	*prev = phys;
	if matches!(kbm::capture_state(), Cap::Pad(..)) {
		kbm::capture_poll(&[], fresh);
		return true;
	}
	false
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn menus_read_the_standard_layout_whatever_the_remap() {
		kbm::ensure_table(&crate::kbm_table::TABLE);
		kbm::reset_pad();
		let a = kbm::table().index("jump").unwrap();
		kbm::assign_pad(a, 0, 3); // Jump on Y
		assert_eq!(kbm::binds().1[a][0], Some(3));
		assert_eq!(game_binds(true).1[a][0], Some(0), "menus: the default (A = Cross)");
		assert_eq!(game_binds(false).1[a][0], Some(3));
		let phys_a = PHYS[0].1;
		assert_eq!(kbm::eval_pad(kbm::table(), &game_binds(true), phys_a, None).buttons, 0x40);
		assert_eq!(kbm::eval_pad(kbm::table(), &game_binds(false), phys_a, None).buttons, 0);
		kbm::reset_pad();
	}

	#[test]
	fn a_pad_capture_binds_the_first_new_button_and_only_pad_actions_can_be_bound() {
		kbm::ensure_table(&crate::kbm_table::TABLE);
		kbm::reset_pad();
		let a = kbm::table().index("wrench").unwrap();
		assert_eq!(act(a, 1), Some(false));
		let mut prev = PHYS[0].1; // A held since before
		assert!(poll_capture(PHYS[0].1, &mut prev));
		assert!(poll_capture(PHYS[0].1 | PHYS[3].1, &mut prev), "second poll: Y went down");
		assert!(!kbm::capturing());
		assert_eq!(kbm::binds().1[a][0], Some(3));
		assert_eq!(text(a, 0, false), "Y (Triangle) / -");
		let f = kbm::table().index("forward").unwrap();
		assert_eq!(text(f, 0, false), KEYBOARD_ONLY[0]);
		kbm::reset_pad();
	}
}
