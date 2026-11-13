//! [PC] R&C2's volumetric fog per level (`Looks::air`, parity wave 3 row 1). The engine's volume (engine/viewer/volfog/density.rs
//! `level_params`) is driven by the level's retail fog (rac_formats::rc2::gameplay::fog: weight = (GS fog near - far) / 255) times
//! `air`, so a level with retail fog gets that fog made volumetric and a level without it (weight < 0.04: levels 14, 20, 23, 25)
//! none. `air` 0 = the retail fog stays as it is (no volume) on the levels whose sky is space: in wave 1 the volume laid a grey /
//! brown band over the starfield there (level 26 st17/0110, Hrugis level 10 st17/0453; runs\w1\mont_fog.png).
//!
//! Which levels' sky is space [C: runs/w3/sky_sheet.png = every sky texture of every level from the disc (rac2/planet_check.rs
//! `planet_facts`): the sky shells of levels 2, 5, 10, 23, 24, 25 and 26 hold only stars, planets, nebulae and suns, no cloud or
//! haze layer; every other level's sky has clouds; the PCSX2 screenshots (rac2/play_runs/fullgame1, runs/w3/retail/sheet.png)
//! show 5, 10 and 25 as ship fights in space, 24 the ship shack in space, 26 a station platform under the starfield, 2 an arena
//! under a starry sky]. Level 15 is not in the PCSX2 run and its one sky texture is a cloud, but its world is a ship level's
//! (8 terrain meshes, 218 vertices; space levels 10 / 25: 1 mesh) [C planet_facts.tsv]: undecided, so off there (WAVE3.md rule).

/// The levels whose sky is space (also rac2/planet_sky.rs: the soft star twinkle).
pub const SPACE_SKY: [usize; 7] = [2, 5, 10, 23, 24, 25, 26];

/// The undecided ship-like level 15 gets no air either.
const NO_AIR_TOO: usize = 15;

/// Open-air haze 0..1 on top of the retail fog weight: 1 (the fog follows the retail fog) except where the sky is space.
pub fn air(level: usize) -> f32 {
	if SPACE_SKY.contains(&level) || level == NO_AIR_TOO {
		0.0
	} else {
		1.0
	}
}

#[cfg(test)]
mod tests {
	#[test]
	fn space_skies_keep_the_retail_fog_and_planets_get_air() {
		for lv in [2, 5, 10, 15, 23, 24, 25, 26] {
			assert_eq!(super::air(lv), 0.0, "level {lv}");
		}
		for lv in [0, 1, 3, 4, 6, 7, 8, 9, 11, 12, 13, 16, 17, 18, 19, 21, 22] {
			assert_eq!(super::air(lv), 1.0, "level {lv}");
		}
		assert_eq!((crate::looks::LOOKS.air)(10), 0.0, "LOOKS takes it");
	}
}
