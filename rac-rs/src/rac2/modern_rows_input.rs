//! [PC] build 45 P2: the Camera tab's input rows R&C2 lacked (sweep A1, C7, C8, A13), R&C1's rows and texts
//! (rac1/modern_menu/rows_system.rs, retail/pcmouseinv.rs, retail/pcext/ui_bridge_more.rs):
//! - Left / Right and Up / Down (Normal / Reversed): retail's own camera words `addrs::OPT_LR` / `OPT_UD` (0 = Reversed). Retail's Camera
//!   page that held them is replaced by the modern page (modern_open.rs), so these rows are the only way to change them. The words are
//!   part of the save's Options block, so no pc_options key: the row writes the word on the GAME thread (`sync`, from subs_sync::watch)
//!   and shows what the game holds (`MIRROR`), also after a save load.
//! - Mouse up/down and left/right (Like camera / Normal / Inverted): keys `mouse_ud` / `mouse_lr` (`normal|inverted`, Like camera = no
//!   key), as R&C1's; the Modern camera reads them live (P5).
//! - Quick Select mouse (`wheelmouse=on`, default Off) and Start skips cutscenes (`skipcut=on|off`, default on).
use crate::modern_rows::{row, CH, NO, TOG};
use rac_engine::pcopts::PcOptions;
use rac_engine::ui::{Cost, UiRow, T5};
use std::sync::atomic::{AtomicI8, AtomicU8, Ordering};

const NORMAL_REVERSED: &[T5] = &[["Normal", "Normal", "Normal", "Normal", "Normale"], ["Reversed", "Invers\u{e9}", "Invertiert", "Invertido", "Invertito"]];
const MOUSE_MODES: &[T5] = &[
	["Like camera", "Comme cam\u{e9}ra", "Wie Kamera", "Como c\u{e1}mara", "Come camera"],
	["Normal", "Normal", "Normal", "Normal", "Normale"],
	["Inverted", "Invers\u{e9}", "Invertiert", "Invertido", "Invertito"],
];

pub static CONTROLLER: [UiRow; 2] = [
	row(
		"leftright",
		CH,
		["Left / Right", "Gauche / Droite", "Links / Rechts", "Izquierda / Derecha", "Sinistra / Destra"],
		[
			"Turns the camera sideways with the right stick. Reversed swaps the direction.",
			"Tourne la cam\u{e9}ra sur le c\u{f4}t\u{e9} avec le stick droit. Invers\u{e9} change le sens.",
			"Dreht die Kamera mit dem rechten Stick zur Seite. Invertiert kehrt die Richtung um.",
			"Gira la c\u{e1}mara hacia los lados con el stick derecho. Invertido cambia el sentido.",
			"Ruota la telecamera di lato con lo stick destro. Invertito scambia la direzione.",
		],
		NORMAL_REVERSED,
		0,
		Cost::None,
		None,
	),
	row(
		"updown",
		CH,
		["Up / Down", "Haut / Bas", "Oben / Unten", "Arriba / Abajo", "Su / Gi\u{f9}"],
		[
			"Tilts the camera up and down with the right stick. Reversed swaps the direction.",
			"Incline la cam\u{e9}ra vers le haut et le bas avec le stick droit. Invers\u{e9} change le sens.",
			"Neigt die Kamera mit dem rechten Stick nach oben und unten. Invertiert kehrt die Richtung um.",
			"Inclina la c\u{e1}mara hacia arriba y abajo con el stick derecho. Invertido cambia el sentido.",
			"Inclina la telecamera su e gi\u{f9} con lo stick destro. Invertito scambia la direzione.",
		],
		NORMAL_REVERSED,
		0,
		Cost::None,
		None,
	),
];

/// The Mouse section's rows after Mouse speed (mouse_ud, mouse_lr; wheel).
pub static MOUSE_INVERT: [UiRow; 3] = [
	row(
		"mouse_ud",
		CH,
		["Mouse up/down", "Souris haut/bas", "Maus hoch/runter", "Rat\u{f3}n arriba/abajo", "Mouse su/gi\u{f9}"],
		[
			"Which way the mouse tilts the camera. Like camera follows the Up / Down option; Normal and Inverted set it for the mouse alone.",
			"Sens dans lequel la souris incline la cam\u{e9}ra. Comme cam\u{e9}ra suit l'option Haut / Bas ; Normal et Invers\u{e9} la r\u{e8}glent pour la souris seule.",
			"In welche Richtung die Maus die Kamera neigt. Wie Kamera folgt der Option Oben / Unten; Normal und Invertiert gelten nur f\u{fc}r die Maus.",
			"Hacia d\u{f3}nde inclina la c\u{e1}mara el rat\u{f3}n. Como c\u{e1}mara sigue la opci\u{f3}n Arriba / Abajo; Normal e Invertido valen solo para el rat\u{f3}n.",
			"In che direzione il mouse inclina la telecamera. Come camera segue l'opzione Su / Gi\u{f9}; Normale e Invertito valgono solo per il mouse.",
		],
		MOUSE_MODES,
		0,
		Cost::None,
		None,
	),
	row(
		"mouse_lr",
		CH,
		["Mouse left/right", "Souris gauche/droite", "Maus links/rechts", "Rat\u{f3}n izq./der.", "Mouse sin./des."],
		[
			"Which way the mouse turns the camera sideways. Like camera follows the Left / Right option; Normal and Inverted set it for the mouse alone.",
			"Sens dans lequel la souris tourne la cam\u{e9}ra sur le c\u{f4}t\u{e9}. Comme cam\u{e9}ra suit l'option Gauche / Droite ; Normal et Invers\u{e9} la r\u{e8}glent pour la souris seule.",
			"In welche Richtung die Maus die Kamera zur Seite dreht. Wie Kamera folgt der Option Links / Rechts; Normal und Invertiert gelten nur f\u{fc}r die Maus.",
			"Hacia d\u{f3}nde gira la c\u{e1}mara el rat\u{f3}n. Como c\u{e1}mara sigue la opci\u{f3}n Izquierda / Derecha; Normal e Invertido valen solo para el rat\u{f3}n.",
			"In che direzione il mouse ruota la telecamera di lato. Come camera segue l'opzione Sinistra / Destra; Normale e Invertito valgono solo per il mouse.",
		],
		MOUSE_MODES,
		0,
		Cost::None,
		None,
	),
	row(
		"wheel",
		TOG,
		["Quick Select mouse", "Roue par souris", "Schnellwahl per Maus", "Rueda con rat\u{f3}n", "Ruota con mouse"],
		[
			"Opens the Quick Select ring with the mouse and picks a weapon by moving the mouse.",
			"Ouvre la roue de s\u{e9}lection rapide \u{e0} la souris et choisit une arme en la d\u{e9}pla\u{e7}ant.",
			"\u{d6}ffnet den Schnellwahl-Ring mit der Maus und w\u{e4}hlt eine Waffe per Mausbewegung.",
			"Abre la rueda de selecci\u{f3}n r\u{e1}pida con el rat\u{f3}n y elige un arma moviendo el rat\u{f3}n.",
			"Apre la ruota di selezione rapida col mouse e sceglie un'arma muovendo il mouse.",
		],
		NO,
		0,
		Cost::None,
		None,
	),
];

/// The Game-ish "Start skips cutscenes" row (R&C1's Start section; default On = retail's plain Start skip).
pub static SKIPCUT: [UiRow; 1] = [row(
	"skipcut",
	TOG,
	["Start skips cutscenes", "Start passe les sc\u{e8}nes", "Start \u{fc}berspringt Szenen", "Start salta escenas", "Start salta le scene"],
	[
		"Pressing Start during a cutscene skips it. Off plays every cutscene to the end.",
		"Appuyer sur Start pendant une sc\u{e8}ne la passe. Non les joue toutes jusqu'au bout.",
		"Start w\u{e4}hrend einer Zwischensequenz \u{fc}berspringt sie. Aus spielt jede bis zum Ende.",
		"Pulsar Start durante una escena la salta. No las reproduce todas hasta el final.",
		"Premere Start durante una scena la salta. No le riproduce tutte fino alla fine.",
	],
	NO,
	1,
	Cost::None,
	None,
)];

/// [b46 W6] C11: R&C1's "Strafe aiming" row (key `strafe=on|off`, engine PcOptions.strafe_on, default on) with the two values R&C2 has:
/// Raised (the Modern camera lifts and the aim circle shows while strafing: today's feel) and Steady (the camera keeps its pose while
/// strafing, as retail R&C2). R&C1's Shoulder / Off values are not offered (Shoulder is not ported; Off = Steady here, RMB still strafes).
const STRAFES: &[T5] = &[["Raised", "Sur\u{e9}lev\u{e9}e", "Erh\u{f6}ht", "Elevada", "Rialzata"], ["Steady", "Stable", "Ruhig", "Estable", "Stabile"]];
pub static AIMING: [UiRow; 1] = [row(
	"strafe",
	CH,
	["Strafe aiming", "Vis\u{e9}e en d\u{e9}placement", "Zielen beim Strafen", "Apuntar al desplazarse", "Mira in movimento"],
	[
		"Hold the right mouse button to strafe. Raised lifts the camera while you strafe, Steady keeps the camera as it is, like Ratchet & Clank 2.",
		"Maintenez le bouton droit pour viser en vous d\u{e9}pla\u{e7}ant. Sur\u{e9}lev\u{e9}e l\u{e8}ve la cam\u{e9}ra, Stable garde la cam\u{e9}ra telle quelle, comme dans Ratchet & Clank 2.",
		"Halte die rechte Maustaste zum Strafen. Erh\u{f6}ht hebt die Kamera beim Strafen, Ruhig l\u{e4}sst die Kamera unver\u{e4}ndert, wie in Ratchet & Clank 2.",
		"Mant\u{e9}n el bot\u{f3}n derecho del rat\u{f3}n para apuntar al desplazarte. Elevada sube la c\u{e1}mara, Estable deja la c\u{e1}mara tal cual, como en Ratchet & Clank 2.",
		"Tieni premuto il tasto destro del mouse per mirare mentre ti sposti. Rialzata alza la telecamera, Stabile lascia la telecamera com'\u{e8}, come in Ratchet & Clank 2.",
	],
	STRAFES,
	0,
	Cost::None,
	None,
)];

// ---- the retail invert words ---------------------------------------------------------------------------------------------------

/// A pending row choice per axis (0 = Left / Right, 1 = Up / Down): -1 none, else the row value (0 Normal, 1 Reversed).
static PENDING: [AtomicI8; 2] = [AtomicI8::new(-1), AtomicI8::new(-1)];
/// What the game thread last saw in the words (0 Normal, 1 Reversed); a new game holds Normal.
static MIRROR: [AtomicU8; 2] = [AtomicU8::new(0), AtomicU8::new(0)];
const WORDS: [u32; 2] = [crate::addrs::OPT_LR, crate::addrs::OPT_UD];

fn axis(id: &str) -> Option<usize> {
	match id {
		"leftright" => Some(0),
		"updown" => Some(1),
		_ => None,
	}
}

/// The row value of `leftright` / `updown` (None = not one of them): the pending choice, else what the game holds.
pub fn invert_value(id: &str) -> Option<i32> {
	let a = axis(id)?;
	let p = PENDING[a].load(Ordering::Relaxed);
	Some(if p >= 0 { p as i32 } else { MIRROR[a].load(Ordering::Relaxed) as i32 })
}

/// The row was set: the game thread writes the word at the next field (`sync`). False = not one of the two rows.
pub fn invert_set(id: &str, v: i32) -> bool {
	let Some(a) = axis(id) else { return false };
	PENDING[a].store((v != 0) as i8, Ordering::Relaxed);
	true
}

/// The word for a row value: retail's 0 = Reversed, 1 = Normal.
pub fn word_of(reversed: bool) -> u32 {
	if reversed {
		0
	} else {
		1
	}
}

/// Game thread, every live field (subs_sync::watch): a pending row choice is written into the word; otherwise the mirror follows the
/// word (a loaded save brings its own). Nothing is written while the choice equals the word (retail untouched).
pub fn sync(m: &mut ee_oracle::Machine) {
	for a in 0..2 {
		let cur = m.rd32(WORDS[a]);
		let p = PENDING[a].swap(-1, Ordering::Relaxed);
		if p >= 0 && word_of(p != 0) != cur {
			let w = word_of(p != 0);
			for (i, b) in w.to_le_bytes().iter().enumerate() {
				m.cpu.mem.wr8(WORDS[a] + i as u32, *b);
			}
			crate::pc_rec::note_write(crate::pc_rec::Stage::After, WORDS[a], &w.to_le_bytes());
			eprintln!("rac2: {} {} (the game's camera word)", if a == 0 { "Left / Right" } else { "Up / Down" }, if p != 0 { "Reversed" } else { "Normal" });
			MIRROR[a].store(p as u8, Ordering::Relaxed);
		} else {
			MIRROR[a].store((cur == 0) as u8, Ordering::Relaxed);
		}
	}
}

// ---- the file-backed rows ------------------------------------------------------------------------------------------------------

fn mouse_key(id: &str) -> Option<&'static str> {
	match id {
		"mouse_ud" => Some("mouse_ud"),
		"mouse_lr" => Some("mouse_lr"),
		_ => None,
	}
}

/// 0 Like camera, 1 Normal, 2 Inverted (R&C1's pcmouseinv::init: anything unknown = Like camera).
pub fn mouse_mode(o: &PcOptions, key: &str) -> i32 {
	match o.extra_get(key).map(|v| v.trim().to_ascii_lowercase()).as_deref() {
		Some("normal") => 1,
		Some("inverted") => 2,
		_ => 0,
	}
}

/// `get` of these rows (modern_menu.rs `value`).
pub fn value(o: &PcOptions, id: &str) -> Option<i32> {
	if let Some(v) = invert_value(id) {
		return Some(v);
	}
	if let Some(k) = mouse_key(id) {
		return Some(mouse_mode(o, k));
	}
	match id {
		"wheel" => Some(o.extra_get("wheelmouse").is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "on" | "1" | "true")) as i32),
		"skipcut" => Some(o.skipcut_on() as i32),
		"strafe" => Some(!o.strafe_on() as i32), // [b46 W6] C11: 0 Raised (on), 1 Steady (off)
		_ => None,
	}
}

/// `set` of these rows; false = not one of them.
pub fn apply(o: &mut PcOptions, id: &str, v: i32) -> bool {
	if invert_set(id, v) {
		return true;
	}
	if let Some(k) = mouse_key(id) {
		o.extra_set(k, match v {
			1 => Some("normal".to_string()),
			2 => Some("inverted".to_string()),
			_ => None,
		});
		return true;
	}
	match id {
		"wheel" => o.extra_set("wheelmouse", (v != 0).then(|| "on".to_string())),
		"skipcut" => o.skipcut = Some(v != 0),
		"strafe" => o.strafe = Some(v == 0),
		_ => return false,
	}
	true
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn rows_round_trip_and_defaults_are_r_and_c_ones() {
		let mut o = PcOptions::parse("");
		assert_eq!((value(&o, "mouse_ud"), value(&o, "mouse_lr"), value(&o, "wheel"), value(&o, "skipcut")), (Some(0), Some(0), Some(0), Some(1)));
		assert!(apply(&mut o, "mouse_ud", 2) && apply(&mut o, "mouse_lr", 1) && apply(&mut o, "wheel", 1) && apply(&mut o, "skipcut", 0));
		assert_eq!((value(&o, "mouse_ud"), value(&o, "mouse_lr"), value(&o, "wheel"), value(&o, "skipcut")), (Some(2), Some(1), Some(1), Some(0)));
		let o2 = PcOptions::parse(&o.to_text());
		assert_eq!(o2.extra_get("mouse_ud"), Some("inverted"));
		assert!(apply(&mut o, "mouse_ud", 0) && o.extra_get("mouse_ud").is_none(), "Like camera removes the key");
		assert!(!apply(&mut o, "fog", 1) && value(&o, "fog").is_none());
	}

	#[test]
	fn retail_words_are_zero_for_reversed() {
		assert_eq!((word_of(true), word_of(false)), (0, 1));
		assert_eq!(CONTROLLER.each_ref().map(|r| r.id), ["leftright", "updown"]);
		assert_eq!(MOUSE_INVERT.each_ref().map(|r| r.id), ["mouse_ud", "mouse_lr", "wheel"]);
		assert!(invert_set("updown", 1) && invert_value("updown") == Some(1) && PENDING[1].swap(-1, Ordering::Relaxed) == 1);
	}
}
