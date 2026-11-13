//! [PC] The Debug section of the modern Options page's Controls tab (parity row 71; R&C1 `rac1/retail/pcctl.rs` SEC_DEBUG, the
//! Controls page's last section): row 1 "Debug mode" Off / On (pc_options `debug=on`, options.rs `debug_mode`; moved here from the
//! System section), rows 2..6 name the keys it switches on, with R&C1's texts and R&C2's keys (pc_devtools.rs, pc_freecam.rs,
//! pc_debug.rs). The key rows are text-only Bind rows (the key in the binding box, like the rows above it; `get` sets the text):
//! Enter / Left / Backspace on them do nothing, nothing is read or written.
use crate::modern_rows::{row, t, NO, TOG};
use rac_engine::ui::{live, Cost, RowKind, UiRow, T5};

pub const S_DEBUG: T5 = t("Debug");

const DESC_MODE: T5 = [
	"Developer keys: F4 planet warp, F2 free camera, F3 draw distance, F7 god mode, F6 infinite ammo, F8 10000 more bolts, F10 split view, F11 hide the HUD, F12 labels. Warping can skip story progress: save first.",
	"Touches de d\u{e9}veloppement : F4 saut de niveau, F2 cam\u{e9}ra libre, F3 distance d'affichage, F7 invincible, F6 munitions infinies, F8 10000 boulons de plus, F10 \u{e9}cran partag\u{e9}, F11 masquer l'ATH, F12 \u{e9}tiquettes. Un saut peut sauter l'histoire : sauvegardez d'abord.",
	"Entwicklertasten: F4 Planeten-Warp, F2 freie Kamera, F3 Sichtweite, F7 unverwundbar, F6 unbegrenzte Munition, F8 10000 Schrauben mehr, F10 geteilte Ansicht, F11 HUD ausblenden, F12 Beschriftungen. Ein Warp kann Story-Fortschritt \u{fc}berspringen: vorher speichern.",
	"Teclas de desarrollo: F4 salto de planeta, F2 c\u{e1}mara libre, F3 distancia de dibujado, F7 invencible, F6 munici\u{f3}n infinita, F8 10000 tornillos m\u{e1}s, F10 vista dividida, F11 ocultar el HUD, F12 etiquetas. Un salto puede saltarse la historia: guarda antes.",
	"Tasti sviluppatore: F4 salto a pianeta, F2 camera libera, F3 distanza visiva, F7 invincibile, F6 munizioni infinite, F8 altri 10000 bulloni, F10 vista divisa, F11 nascondi l'HUD, F12 etichette. Un salto pu\u{f2} saltare la storia: salva prima.",
];
const DESC_KEYS: T5 = [
	"With Debug mode on, this key does it (keyboard). It cannot be changed.",
	"Avec le mode d\u{e9}bogage, cette touche le fait (clavier). Elle ne peut pas \u{ea}tre chang\u{e9}e.",
	"Mit dem Debug-Modus macht diese Taste das (Tastatur). Sie kann nicht ge\u{e4}ndert werden.",
	"Con el modo depuraci\u{f3}n, esta tecla lo hace (teclado). No se puede cambiar.",
	"Con la modalit\u{e0} debug, questo tasto lo fa (tastiera). Non si pu\u{f2} cambiare.",
];

/// The keys of ROWS[1..] (R&C2's: R&C1's F1 / F5 are R&C2's picture flip / quick save).
const KEYS: [&str; 5] = ["F4", "F2", "F7", "F6", "F8"];

const fn key(id: &'static str, label: T5) -> UiRow {
	row(id, RowKind::Bind, label, DESC_KEYS, NO, 0, Cost::None, None)
}

pub static ROWS: [UiRow; 6] = [
	row("debug", TOG, ["Debug mode", "Mode d\u{e9}bogage", "Debug-Modus", "Modo depuraci\u{f3}n", "Modalit\u{e0} debug"], DESC_MODE, NO, 0, Cost::None, None),
	key("dbgkey_warp", ["Planet warp", "Saut de niveau", "Planeten-Warp", "Salto de planeta", "Salto a pianeta"]),
	key("dbgkey_free", ["Free camera", "Cam\u{e9}ra libre", "Freie Kamera", "C\u{e1}mara libre", "Camera libera"]),
	key("dbgkey_god", ["God mode", "Invincible", "Unverwundbar", "Invencible", "Invincibile"]),
	key("dbgkey_ammo", ["Infinite ammo", "Munitions infinies", "Unbegrenzte Munition", "Munici\u{f3}n infinita", "Munizioni infinite"]),
	key("dbgkey_bolts", ["10000 more bolts", "10000 boulons de plus", "10000 Schrauben mehr", "10000 tornillos m\u{e1}s", "Altri 10000 bulloni"]),
];

/// A key row (text only).
fn is_key(id: &str) -> bool {
	id.starts_with("dbgkey_")
}

/// The row value of a key row (always its one value); None for every other id.
pub fn value(id: &str) -> Option<i32> {
	is_key(id).then_some(0)
}

/// `Pack::ui` `get` of a key row: its key as the binding text, value 0 (None for every other id).
pub fn get(id: &str) -> Option<i32> {
	let i = ROWS[1..].iter().position(|r| r.id == id)?;
	live::set(ROWS[1 + i].id, 0, KEYS[i].to_string());
	Some(0)
}

/// A key row: nothing to write (true = it was one).
pub fn apply(id: &str) -> bool {
	is_key(id)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_debug_section_is_the_mode_row_and_five_text_only_key_rows() {
		assert_eq!(ROWS[0].id, "debug");
		assert!(ROWS[1..].iter().all(|r| is_key(r.id) && r.kind == RowKind::Bind && value(r.id) == Some(0) && apply(r.id)));
		assert_eq!((get("dbgkey_warp"), get("dbgkey_bolts"), get("debug")), (Some(0), Some(0), None));
		assert_eq!((value("debug"), apply("debug")), (None, false), "the mode row is a real key (modern_menu.rs)");
	}
}
