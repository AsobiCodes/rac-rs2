//! [PC] R&C2's Fire and Smoke rows (parity rows 30-31): R&C1's rows (texts from rac1/modern_menu/rows_effects.rs, Off / Low /
//! Medium / High, default High, need Lighting Enhanced) on R&C2's effect pool (rac2/fx_pool.rs, host/ov_fire.rs). Keys `fire` /
//! `smoke` (engine pcopts `graphics_rows` 17 / 18), live in engine fire_option / smoke_option.
use crate::modern_rows::{row, CH, LIGHT, Q4};
use rac_engine::pcopts::PcOptions;
use rac_engine::ui::{Cost, UiRow};

pub const FIRE: UiRow = row(
	"fire",
	CH,
	["Fire", "Feu", "Feuer", "Fuego", "Fuoco"],
	[
		"Realistic flames for weapons, effects and burning buildings. Off is the original fire.",
		"Flammes r\u{e9}alistes pour les armes, les effets et les b\u{e2}timents en feu. Avec Non, c'est le feu d'origine.",
		"Realistische Flammen f\u{fc}r Waffen, Effekte und brennende Geb\u{e4}ude. Aus ist das Originalfeuer.",
		"Llamas realistas para armas, efectos y edificios en llamas. Con No se mantiene el fuego original.",
		"Fiamme realistiche per armi, effetti ed edifici in fiamme. Con No resta il fuoco originale.",
	],
	Q4,
	3,
	// R&C1's measured level (rac1/modern_menu/costs.rs FIRE).
	Cost::Low,
	LIGHT,
);

pub const SMOKE: UiRow = row(
	"smoke",
	CH,
	["Smoke", "Fum\u{e9}e", "Rauch", "Humo", "Fumo"],
	[
		"Volumetric smoke from fires and explosions. Off is the original smoke.",
		"Fum\u{e9}e volum\u{e9}trique des feux et des explosions. Avec Non, c'est la fum\u{e9}e d'origine.",
		"Volumetrischer Rauch von Feuern und Explosionen. Aus ist der Originalrauch.",
		"Humo volum\u{e9}trico de fuegos y explosiones. Con No se mantiene el humo original.",
		"Fumo volumetrico da fuochi ed esplosioni. Con No resta il fumo originale.",
	],
	Q4,
	3,
	// R&C1's measured level (rac1/modern_menu/costs.rs SMOKE).
	Cost::Medium,
	LIGHT,
);

/// The engine's live Fire and Smoke levels from the file (rac2/options.rs `apply`; no key = High, as R&C1's).
pub fn init(o: &PcOptions) {
	let g = o.graphics_rows();
	rac_engine::fire_option::set(g[17]);
	rac_engine::smoke_option::set(g[18]);
}

/// `get` / `set` of the two rows (modern_menu.rs); None / false = not one of them.
pub fn value(o: &PcOptions, id: &str) -> Option<i32> {
	match id {
		"fire" => Some(o.graphics_rows()[17] as i32),
		"smoke" => Some(o.graphics_rows()[18] as i32),
		_ => None,
	}
}

pub fn apply(o: &mut PcOptions, id: &str, v: i32) -> bool {
	let lvl = v.clamp(0, 3) as u8;
	match id {
		"fire" => o.fire = Some(lvl),
		"smoke" => o.smoke = Some(lvl),
		_ => return false,
	}
	true
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn fire_and_smoke_default_high_and_round_trip() {
		let mut o = PcOptions::parse("");
		assert_eq!((value(&o, "fire"), value(&o, "smoke")), (Some(3), Some(3)));
		assert!(apply(&mut o, "fire", 0) && apply(&mut o, "smoke", 9));
		assert_eq!((value(&o, "fire"), value(&o, "smoke")), (Some(0), Some(3)));
		assert!(!apply(&mut o, "metal", 1) && value(&o, "metal").is_none());
		assert_eq!((FIRE.id, SMOKE.id), ("fire", "smoke"));
	}
}
