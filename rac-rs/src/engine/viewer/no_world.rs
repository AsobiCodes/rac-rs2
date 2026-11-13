//! [R&C2] A field in which the game itself draws no world (a minigame such as the sphere puzzle: the sky, terrain, ties and shrubs
//! are not sent, only a moby or two and the HUD over a black clear). The host sets the flag from the GS picture of the field
//! (`gsfull::ovl_pipe::Frame::no_world`); the draw passes then leave the sky and the level out, so the native picture shows the
//! black the game cleared to instead of the level's sky around the origin. Never set for R&C1 (default false).
use std::sync::atomic::{AtomicBool, Ordering};

static NO_WORLD: AtomicBool = AtomicBool::new(false);

/// The next draws leave the level (sky, terrain, ties, shrubs, ocean, strips, pools) out.
pub fn set(on: bool) {
	NO_WORLD.store(on, Ordering::Relaxed);
}

/// Is the level left out (`set`)?
pub fn on() -> bool {
	NO_WORLD.load(Ordering::Relaxed)
}

/// The draw groups (`draw::dskip` names) that are the level and not the game's mobies or HUD.
pub fn hides(name: &str) -> bool {
	on() && matches!(name, "sky" | "world" | "tfrags" | "ties" | "shrub" | "bb" | "veg" | "ocean" | "oceang" | "quads" | "strips" | "pools")
}

/// The VU1 programs of a field's segments that draw the level (R&C2: 3 sky, 5 and 6 terrain, 7 ties, 8 shrubs, 10 water
/// [C: RAC2_OVLSEGS on states 0555 and 0453 of fullgame1 list 3, 5, 6, 7, 8; the sphere minigame 0561 and the hacking game 0096
/// list only 2 and 11]). A field whose segments hold none of them draws no world.
pub fn field_draws_world(segs: &[(u32, u8, u8)]) -> bool {
	segs.iter().any(|s| matches!(s.2 & 15, 3 | 5..=8 | 10))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_minigame_fields_have_no_world_and_the_gameplay_ones_have() {
		assert!(!field_draws_world(&[(0, 2, 11), (155, 2, 11), (1450, 1, 11), (1450, 2, 11)]));
		assert!(!field_draws_world(&[(0, 2, 2), (155, 2, 11), (1780, 1, 11), (7146, 2, 2)]));
		assert!(field_draws_world(&[(0, 2, 11), (30384, 1, 5), (30384, 1, 6), (47794, 1, 3)]));
		assert!(!field_draws_world(&[]));
	}

	#[test]
	fn only_the_level_groups_are_hidden() {
		set(true);
		assert!(hides("sky") && hides("ties") && !hides("props") && !hides("abe"));
		set(false);
		assert!(!hides("sky"));
	}
}
