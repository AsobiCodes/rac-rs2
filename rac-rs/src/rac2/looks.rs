//! R&C2's PC looks (`rac_engine::looks::Looks`): the engine's retail defaults (`looks::NONE`) except where R&C2's data says
//! otherwise.
//!
//! Dynamic shadows (parity wave 2A, 2026-11-07): only the mobys cast (`sun_mobys_only`). R&C2's key light (gameplay light 0, the
//! light the engine's sun takes) is the mobys' light, not a sun the world's painted colours follow [C: sun_ray_check.rs
//! `painted_sun_per_level`, runs/w2a/painted_sun.txt: on levels 1, 4, 7, 8, 11-14, 16, 17 the terrain the level's own ties /
//! shrubs / terrain block from the key light is no darker than what any overhead cover paints (straight-up contrast), on 8 and
//! 17 it is lighter]. With the world casting, the trees and roofs between the ground and that light laid one dark slab over
//! ground the level paints lit [C: Oozla st21/0014 (level 1), `what_stands_between_the_ground_and_the_sun`: 172 of 238 ground
//! points in view blocked, by 20+ tree ties 1-44 m up the light; no sky shell, no huge caster; wave 1 mont_sh.png]. The world's
//! shadows are already in its baked colours (2002/2004's painted shadows); the mobys' shadows from their own key light lie on
//! top (`sunshadow.rs` receivers take only painted-lit ground). Only the mobys on retail's own shadow caster list cast (the
//! slots `Stage::field` hands `set_volumes_retail`; a big structure moby such as the 0453 station ring self-shadowed dark).

use rac_engine::looks::{Looks, NONE};

/// Volumetric fog (wave 3 row 1): no air over a space sky (rac2/planet_fog.rs); every sky stays clear of the volume (wave 4.1:
/// retail never fogs the sky, and R&C2's fog colours do not match its skies: Dobbo orbit's sky went olive, par-w3b) and the air
/// starts at the retail fog's near distance (Endako's city washed pale from 0 m: runs/w4 mont_foga_0.png).
pub static LOOKS: Looks = Looks { sun_mobys_only: |_| true, rays_from_key: |_| true, air: crate::planet_fog::air, retail_fog_shape: |_| true, ice_keys: crate::planet_ice::ice_keys, twinkle: crate::planet_sky::twinkle, clouds: crate::planet_sky::clouds, sunless: |l| l == 15 || crate::planet_fog::SPACE_SKY.contains(&l), known_tide, split_water_quads: |_| true, ..NONE };

/// The measured tide (lowest, highest sea plane) per level, which seeds the shore map's tide-free height (engine waterfx/shore_ref.rs)
/// [C: RAC_WATER_CLASS over 600 fields, lane gfxc runs/out/tide_*: Oozla (level 1, fg1 0014) 108.400 .. 108.600, Dobbo (level 9,
/// fg1 0357) 408.169 .. 408.369; both a full cycle, +-0.1 round the mid plane]. Other levels: not measured.
fn known_tide(level: usize) -> Option<(f32, f32)> {
	match level {
		1 => Some((108.400, 108.600)),
		9 => Some((408.169, 408.369)),
		_ => None,
	}
}

#[cfg(test)]
mod tests {
	#[test]
	fn only_mobys_cast_and_everything_else_is_retail() {
		let l = &super::LOOKS;
		assert!((0..32).all(|lv| (l.sun_mobys_only)(lv) && (l.sunless)(lv) == (lv == 15 || crate::planet_fog::SPACE_SKY.contains(&lv)) && !(l.sky_key)(lv)));
		assert!(!(rac_engine::looks::NONE.sun_mobys_only)(1), "the engine default: the world casts (R&C1)");
		assert!((0..32).all(|lv| (l.retail_fog_shape)(lv)), "the fog volume keeps the retail fog's shape (wave 4.1)");
		assert!(!(rac_engine::looks::NONE.retail_fog_shape)(1), "the engine default: the fog marches sky rays from the eye (R&C1)");
		assert_eq!((l.known_tide)(1), Some((108.400, 108.600)), "Oozla's measured tide");
		assert!((l.known_tide)(0).is_none());
	}
}
