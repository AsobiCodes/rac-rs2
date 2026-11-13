//! [PC] The Graphics tab's Resolution section of R&C2's modern Options page (parity wave 2 B row 8): Upscaler (Off / FSR / DLSS) and
//! Upscale quality, R&C1's rows (rac1/modern_menu/rows_upscale.rs) on the engine's choice (`rac_engine::viewer::upscale_choice`,
//! pc_options keys `upscaler` / `upscale_q`, read at the start by options.rs `apply`). An upscaler this PC cannot run (no DLLs next to
//! the exe, no RTX GPU, the runtime refused) is not offered (engine ui/live.rs `offer`). Checked on R&C2 (par-w2e, runs/w2e/upx.sh):
//! every presented frame of the Lancer fight with frame smoothing on, DLSS 4.5 and FSR 3.1: the GS effects plane and the HUD are drawn
//! at the window size after the upscaler, so they do not shimmer; no A-B-A flicker, no ghosting on Ratchet or the shots.
use crate::modern_rows::{row, t, CH, OFF};
use rac_engine::pcopts::PcOptions;
use rac_engine::ui::{Cost, Needs, NeedsRule, UiRow, T5};
use rac_engine::viewer::upscale_choice;

const KIND_VALUES: &[T5] = &[OFF, t("FSR 3 / 4"), t("DLSS")];
/// The share of the window each side is drawn at; with DLSS the engine names the first one "DLAA (100%)".
const Q_VALUES: &[T5] = &[
	["Native AA (100%)", "AA natif (100 %)", "Natives AA (100 %)", "AA nativo (100 %)", "AA nativo (100%)"],
	["Quality (67%)", "Qualit\u{e9} (67 %)", "Qualit\u{e4}t (67 %)", "Calidad (67 %)", "Qualit\u{e0} (67%)"],
	["Balanced (59%)", "\u{c9}quilibr\u{e9} (59 %)", "Ausgewogen (59 %)", "Equilibrado (59 %)", "Bilanciato (59%)"],
	["Performance (50%)", "Performance (50 %)", "Leistung (50 %)", "Rendimiento (50 %)", "Prestazioni (50%)"],
	["Ultra performance (33%)", "Ultra performance (33 %)", "Ultra-Leistung (33 %)", "Ultra rendimiento (33 %)", "Ultra prestazioni (33%)"],
];

pub static UPSCALING: [UiRow; 3] = [
	row(
		"upscaler",
		CH,
		["Upscaler", "Mise \u{e0} l'\u{e9}chelle", "Hochskalierung", "Escalado", "Upscaling"],
		[
			// The last sentence is the in-app credit NVIDIA's DLSS SDK licence asks for (as R&C1's row).
			"Draws the game smaller, then scales it up to the window: faster and nearly as sharp. With Off, Render scale applies. Upscaling by AMD FidelityFX Super Resolution (FSR) and NVIDIA DLSS.",
			"Dessine le jeu plus petit, puis l'agrandit \u{e0} la fen\u{ea}tre : plus rapide et presque aussi net. Avec Non, la taille du rendu s'applique. Mise \u{e0} l'\u{e9}chelle par AMD FidelityFX Super Resolution (FSR) et NVIDIA DLSS.",
			"Zeichnet das Spiel kleiner und skaliert es dann auf das Fenster hoch: schneller und fast so scharf. Bei Aus gilt die Render-Skalierung. Hochskalierung durch AMD FidelityFX Super Resolution (FSR) und NVIDIA DLSS.",
			"Dibuja el juego m\u{e1}s peque\u{f1}o y luego lo ampl\u{ed}a a la ventana: m\u{e1}s r\u{e1}pido y casi igual de n\u{ed}tido. Con No se aplica la escala de render. Escalado de AMD FidelityFX Super Resolution (FSR) y NVIDIA DLSS.",
			"Disegna il gioco pi\u{f9} piccolo, poi lo ingrandisce alla finestra: pi\u{f9} veloce e quasi altrettanto nitido. Con No vale la scala di rendering. Upscaling di AMD FidelityFX Super Resolution (FSR) e NVIDIA DLSS.",
		],
		KIND_VALUES,
		0,
		Cost::Low,
		None,
	),
	row(
		"upscale_q",
		CH,
		["Upscale quality", "Qualit\u{e9} de mise \u{e0} l'\u{e9}chelle", "Skalierungsqualit\u{e4}t", "Calidad de escalado", "Qualit\u{e0} upscaling"],
		[
			"The size the game is drawn at before the upscaler, in percent of the window. Lower is faster and softer.",
			"Taille du rendu avant mise \u{e0} l'\u{e9}chelle, en % de la fen\u{ea}tre. Plus bas : plus rapide, plus doux.",
			"Die Gr\u{f6}\u{df}e, in der das Spiel vor dem Hochskalieren gezeichnet wird, in Prozent des Fensters. Weniger ist schneller und weicher.",
			"El tama\u{f1}o al que se dibuja el juego antes del escalado, en porcentaje de la ventana. Menos es m\u{e1}s r\u{e1}pido y m\u{e1}s suave.",
			"La dimensione a cui si disegna il gioco prima dell'upscaling, in percentuale della finestra. Meno \u{e8} pi\u{f9} veloce e pi\u{f9} morbido.",
		],
		Q_VALUES,
		upscale_choice::DEFAULT_Q as i32,
		Cost::High,
		Some(Needs { row: "upscaler", min: 1, max: i32::MAX }),
	),
	crate::modern_rows_display::FRAMEGEN, // [b45 P2] follows the Upscaler (engine fgen_follow)
];

/// Upscale quality is dimmed while the Upscaler is Off (no banner: Off is the default, not a problem to point at).
pub const NEEDS_UPSCALER: NeedsRule = NeedsRule {
	row: "upscaler",
	set_to: upscale_choice::FFX_VALUE,
	for_row: "",
	fix: None,
	banner: t(""),
	detail: [
		"Applies while an upscaler runs. Upscaler is {} now.",
		"S'applique avec une mise \u{e0} l'\u{e9}chelle active. Elle est sur {} pour l'instant.",
		"Gilt, solange hochskaliert wird. Die Hochskalierung steht auf {}.",
		"Se aplica mientras hay escalado. Ahora est\u{e1} en {}.",
		"Vale mentre l'upscaling \u{e8} attivo. Ora \u{e8} su {}.",
	],
	button: ["Switch the Upscaler on", "Activer la mise \u{e0} l'\u{e9}chelle", "Hochskalierung einschalten", "Activar el escalado", "Attiva l'upscaling"],
};

/// The row value (the engine's choice in force), None = not one of these rows.
pub fn value(id: &str) -> Option<i32> {
	match id {
		"upscaler" => Some(upscale_choice::kind() as i32),
		"upscale_q" => Some(upscale_choice::quality() as i32),
		_ => None,
	}
}

/// Writes the row into the engine's choice and the options file; false = not one of these rows.
pub fn apply(o: &mut PcOptions, id: &str, v: i32) -> bool {
	match id {
		"upscaler" => {
			upscale_choice::set_kind(o, v);
		}
		"upscale_q" => {
			upscale_choice::set_quality(o, v);
		}
		_ => return false,
	}
	true
}

#[cfg(test)]
mod tests {
	use super::*;
	use upscale_choice::{DLSS_VALUE, FFX_VALUE, KEY, KEY_Q, KINDS};

	#[test]
	fn the_rows_follow_the_engine_choice_and_credit_both_sdks() {
		assert_eq!((UPSCALING[0].id, UPSCALING[1].id, UPSCALING[2].id), (KEY, KEY_Q, "framegen"));
		assert_eq!(UPSCALING[0].values.len(), KINDS.len());
		assert!(UPSCALING[0].values[FFX_VALUE as usize].iter().all(|s| s.starts_with("FSR")));
		assert!(UPSCALING[0].values[DLSS_VALUE as usize].iter().all(|s| *s == "DLSS"));
		assert!(UPSCALING[0].desc.iter().all(|s| s.contains("NVIDIA DLSS") && s.contains("AMD FidelityFX Super Resolution")));
		assert_eq!(value("draw"), None);
		assert!(!apply(&mut PcOptions::default(), "draw", 1));
	}
}
