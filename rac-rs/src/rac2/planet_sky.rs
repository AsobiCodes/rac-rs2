//! [PC] R&C2's sky looks (parity rows 19 clouds and 20 stars; R&C1: rac1/planet_looks.rs `twinkle`, rac1/planet_looks/clouds.rs).
//!
//! Stars: the engine's starfield (engine/viewer/starfield.rs, Lighting Enhanced) finds R&C2's star textures by itself [C: the
//! detector run over every R&C2 sky texture (scratch_rc2_gfxc runs/star_detect.py = a port of `detect` over par runs/w3/sky): sky
//! texture 0x40000 (0x40003 on level 2) is a star texture on levels 0, 2, 3, 5, 6, 7, 9-14, 16, 17, 19, 22-26 (352 to 429 stars);
//! the game log prints `starfield: texture 0x40000 slot N: 352 stars` on Siberius]. Only the twinkle is R&C2's: as R&C1's space
//! levels, the space skies (`planet_fog::SPACE_SKY`) dip softly (no air up there), the planets fully.
//!
//! Clouds: no default. The volumetric cloud sky (engine/viewer/volclouds) is taste per planet (R&C1's table took several rounds with
//! the user), so R&C2 starts it only with the file's `clouds=` key (rac2/options.rs `verdict`; no menu row): the fair-weather draft
//! below on the daylight planets whose painted sky is clouds over blue [C: par runs/w3/sky_sheet.png], every retail sky shell kept
//! (`sky_mask` 0: the clouds go over the painted sky, no landmark drops), no own sun disc. The dark, dusk, night and storm skies
//! (Oozla, Notak, Dobbo, Joba, Boldan, Gorn, Snivelak, Grelbin, Yeedil) and space keep the retail sky.

use rac_engine::looks::CloudLook;

/// The twinkle dip depth: the space skies softly (R&C1's 0.2 for its space levels), the planets fully (0.35, the engine default).
pub fn twinkle(level: usize) -> f32 {
	if crate::planet_fog::SPACE_SKY.contains(&level) {
		0.2
	} else {
		0.35
	}
}

/// The daylight planets with painted clouds over blue: Aranos, Endako, Barlow, Siberius, Tabora, Todano, Aranos 2, Smolg,
/// Damosel, the museum, Dobbo orbit.
const DAY_CLOUDS: [usize; 11] = [0, 3, 4, 7, 8, 12, 14, 17, 18, 21, 22];

/// The cloud look of a level (only drawn when the file asks for clouds): R&C1's fair-weather default on `DAY_CLOUDS`.
pub fn clouds(level: usize) -> Option<CloudLook> {
	DAY_CLOUDS.contains(&level).then_some(FAIR)
}

/// R&C1's fair-weather look (rac1/planet_looks/clouds.rs `fair`).
const FAIR: CloudLook = CloudLook {
	cu: 0.6,
	sc: 0.08,
	ac: 0.1,
	ci: 0.15,
	cc: 0.0,
	tower: 0.45,
	base: 1400.0,
	top: 5200.0,
	mid: 6400.0,
	high: 9500.0,
	density: 1.0,
	haze: 26000.0,
	wind: 40.0,
	heading: 0.6,
	sun: 2.0,
	hue: 0.35,
	dusk: 0.0,
	dusk_amb: 4.0,
	amb: 0.9,
	size: 1.5,
	farcov: 0.18,
	pale: 0.0,
	expo: 1.0,
	soft: 0.0,
	tint: None,
	form: 0.0,
	rim: 1.0,
	lining: 1.0,
	under: 1.0,
	feather: 0.0,
};

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn space_skies_twinkle_softly_and_day_planets_take_clouds() {
		for lv in [2, 5, 10, 23, 24, 25, 26] {
			assert_eq!(twinkle(lv), 0.2, "level {lv}");
			assert!(clouds(lv).is_none(), "space level {lv} keeps its sky");
		}
		for lv in [0, 1, 7, 15, 19] {
			assert_eq!(twinkle(lv), 0.35, "level {lv}");
		}
		for lv in [1, 6, 9, 11, 13, 15, 16, 19, 20] {
			assert!(clouds(lv).is_none(), "dark / dusk / storm level {lv} keeps its sky");
		}
		let k = clouds(7).expect("Siberius");
		assert!(k.top > k.base && k.mid > k.top && k.high > k.mid);
		let l = &crate::looks::LOOKS;
		assert_eq!(((l.twinkle)(26), (l.sky_mask)(7), (l.sun_disc)(7), (l.star)(7)), (0.2, 0, false, None), "LOOKS takes them; every shell kept");
	}
}
