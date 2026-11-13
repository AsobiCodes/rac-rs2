//! [PC] R&C2's Metal rows (parity wave 3 row 3): R&C1's Metal toggle and Metal reflections slider (texts from
//! rac1/modern_menu/rows_effects.rs), on the identities of rac2/metal_materials.rs. Off by default as R&C1's; the slider's 75 % is
//! R&C1's approved default (`metal_option::REFLECT_DEFAULT`). Keys `metal=on|off` and `metal_reflect=0..100` (engine pcopts).
use crate::modern_rows::{row, slider, t, CH, LIGHT, NO, Q4};
use rac_engine::viewer::metal_detail::{self, Detail};
use rac_engine::pcopts::PcOptions;
use rac_engine::ui::{Cost, Needs, NeedsRule, UiRow};

/// The Graphics page's Effects section: Metal, Metal reflections, then Fire and Smoke (rac2/modern_rows_fire.rs).
pub static EFFECTS: [UiRow; 4] = [
	// [b45 P2] R&C1's row (rac1/modern_menu/rows_effects.rs): Off / Low / Medium / High = Metal off / on with the floor reflection detail
	// (engine viewer/metal_detail.rs, key `metal_detail`; default High = a quarter of the render size per axis).
	row(
		"metal",
		CH,
		["Metal", "M\u{e9}tal", "Metall", "Metal", "Metallo"],
		[
			"Shiny metal: reflections of the sky and the scene, and sharp highlights. Low to High = sharper floor reflections, slower.",
			"M\u{e9}tal brillant : reflets du ciel et de la sc\u{e8}ne, et brillances nettes. De Faible \u{e0} \u{c9}lev\u{e9} = reflets du sol plus nets, plus lent.",
			"Gl\u{e4}nzendes Metall: Spiegelungen von Himmel und Szene und scharfe Glanzlichter. Niedrig bis Hoch = sch\u{e4}rfere Bodenspiegelungen, langsamer.",
			"Metal brillante: reflejos del cielo y de la escena, y brillos n\u{ed}tidos. De Baja a Alta = reflejos del suelo m\u{e1}s n\u{ed}tidos, m\u{e1}s lento.",
			"Metallo lucido: riflessi del cielo e della scena, e luci nitide. Da Bassa ad Alta = riflessi del pavimento pi\u{f9} nitidi, pi\u{f9} lento.",
		],
		Q4,
		0,
		// R&C1's measured level (rac1/modern_menu/costs.rs METAL).
		Cost::High,
		LIGHT,
	),
	row(
		"metal_reflect",
		slider(0, 100, 5, "%"),
		["Metal reflections", "Reflets du m\u{e9}tal", "Metallspiegelung", "Reflejos del metal", "Riflessi del metallo"],
		[
			"How strongly metal mirrors the sky and the scene. 100% = the full shiny look; lower = more matte; 0% = no reflections. Highlights from lights stay.",
			"Intensit\u{e9} des reflets du ciel et de la sc\u{e8}ne sur le m\u{e9}tal. 100 % = tout l'effet brillant ; moins = plus mat ; 0 % = aucun reflet. Les brillances des lumi\u{e8}res restent.",
			"Wie stark Metall Himmel und Szene spiegelt. 100 % = voller Glanz; weniger = matter; 0 % = keine Spiegelung. Glanzlichter von Lampen bleiben.",
			"Cu\u{e1}nto refleja el metal el cielo y la escena. 100 % = todo el brillo; menos = m\u{e1}s mate; 0 % = sin reflejos. Los brillos de las luces se mantienen.",
			"Quanto il metallo riflette cielo e scena. 100% = tutto l'effetto lucido; meno = pi\u{f9} opaco; 0% = nessun riflesso. Le luci nitide restano.",
		],
		NO,
		75,
		Cost::None,
		Some(Needs { row: "metal", min: 1, max: i32::MAX }),
	),
	crate::modern_rows_fire::FIRE,
	crate::modern_rows_fire::SMOKE,
];

/// Metal reflections is dimmed while Metal is Off (R&C1's rule).
pub const NEEDS_METAL: NeedsRule = NeedsRule {
	row: "metal",
	set_to: 3,
	for_row: "",
	fix: Some(saved_ui_value), // restores the saved Low / Medium / High, not always High (R&C1's rule)
	banner: t(""),
	detail: [
		"Applies while Metal is not Off. Metal is {} now.",
		"S'applique quand le m\u{e9}tal est sur Oui. Il est sur {} pour l'instant.",
		"Gilt, solange Metall an ist. Metall steht auf {}.",
		"Se aplica con el metal en S\u{ed}. Ahora est\u{e1} en {}.",
		"Vale con il metallo su S\u{ec}. Ora \u{e8} su {}.",
	],
	button: ["Switch Metal On", "Activer le m\u{e9}tal", "Metall einschalten", "Activar el metal", "Attiva il metallo"],
};

/// R&C1's Metal reflection detail default (rac1 retail/pcmetal.rs `DETAIL_DEFAULT`): High = the planar floor reflection at 1/4 of the
/// render size per axis (the engine's own default is the costlier Full).
pub const DETAIL_DEFAULT: Detail = Detail::High;

/// The Low..High row value the saved detail gives (1 Low, 2 Medium, 3 High): what "Switch Metal On" restores.
pub fn saved_ui_value() -> i32 {
	metal_detail::get().ui().clamp(0, 2) + 1
}

/// The file's Metal choice (no key = Off, as R&C1's `pcmetal::init`).
pub fn metal_on(o: &PcOptions) -> bool {
	o.extra_get("metal").is_some_and(|v| v.trim().eq_ignore_ascii_case("on"))
}

/// The file's Metal reflections, snapped to the slider's steps of 5 (no key = 75).
pub fn reflect(o: &PcOptions) -> u8 {
	o.metal_reflect.map_or(rac_engine::metal_option::REFLECT_DEFAULT, |v| v.min(100) / 5 * 5)
}

/// The engine's live Metal statics from the file (rac2/options.rs `apply`).
pub fn init(o: &PcOptions) {
	metal_detail::init(o, DETAIL_DEFAULT);
	rac_engine::metal_option::set(metal_on(o));
	rac_engine::metal_option::set_reflect(reflect(o));
}

/// `get` / `set` of the two rows (modern_menu.rs); None / false = not a Metal row.
pub fn value(o: &PcOptions, id: &str) -> Option<i32> {
	match id {
		"metal" => Some(if metal_on(o) { saved_ui_value() } else { 0 }),
		"metal_reflect" => Some(reflect(o) as i32),
		_ => None,
	}
}

pub fn apply(o: &mut PcOptions, id: &str, v: i32) -> bool {
	match id {
		"metal" => {
			if v > 0 {
				metal_detail::set_from_ui(o, Detail::from_ui(v.clamp(1, 3) - 1));
			}
			o.extra_set("metal", Some(if v > 0 { "on" } else { "off" }.to_string()));
		}
		"metal_reflect" => o.metal_reflect = Some((v.clamp(0, 100) / 5 * 5) as u8),
		_ => return false,
	}
	true
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn new_file_is_off_at_75_and_rows_round_trip() {
		let mut o = PcOptions::parse("");
		assert_eq!((value(&o, "metal"), value(&o, "metal_reflect")), (Some(0), Some(75)));
		assert!(apply(&mut o, "metal", 1) && apply(&mut o, "metal_reflect", 42));
		assert_eq!((value(&o, "metal"), value(&o, "metal_reflect")), (Some(1), Some(40)), "Low = on with the Low detail");
		assert_eq!(o.extra_get("metal_detail"), Some("low"));
		assert!(apply(&mut o, "metal", 0) && o.extra_get("metal") == Some("off") && o.extra_get("metal_detail") == Some("low"), "Off keeps the detail key");
		assert!(!apply(&mut o, "fog", 1) && value(&o, "fog").is_none());
		let o = PcOptions::parse("metal=on\nmetal_reflect=100\n");
		assert!(metal_on(&o) && reflect(&o) == 100);
	}
}
