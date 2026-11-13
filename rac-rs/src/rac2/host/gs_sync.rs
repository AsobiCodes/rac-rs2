//! [PC] The scene the window draws is the one the newest GS picture was drawn with (round 3 batch 26). The game thread runs ahead of
//! the GS pipe (live play: by a varying part of a field; lockstep: never), and everything native that is built from the GS fields (the
//! water batches, rac2/water.rs; the effect layers, host/ov_layers.rs; the GS picture itself, the HUD and effects) belongs to the camera
//! of the field before its own (`GS_CAMERA_LAG`). Drawn with the game's newest camera instead, the water stood still while the pipe
//! was behind and then jumped (the swamp "wobble" when the camera turns, live only), and a field's mouse turn could not be extrapolated
//! (the turn per field is irregular). So the world is drawn with the scene of field `done - 1` (`GsPipe::done`), frame smoothing
//! blends from its field before (smooth.rs), and the water and layers interpolate between the two newest GS fields at the same place:
//! every presented frame shows each native layer with the camera it was built with. `RAC2_GSSYNC=0`: the newest scene, as before.

use crate::scene::Scene;
use std::collections::VecDeque;

/// Fields between a GS field and the camera it was drawn with (ov_layers.rs `GS_CAMERA_LAG`, water.rs `CAMERA_LAG`).
const LAG: u64 = 1;
/// Scenes kept: the GS pipe holds up to 5 fields between the game thread and the picture (ov_layers.rs `Views::KEEP`).
const KEEP: usize = 8;

#[derive(Default)]
pub(super) struct SceneSync {
	scenes: VecDeque<Scene>,
	/// The field of the last scene given out.
	last: Option<u64>,
}

/// `RAC2_GSSYNC=0` turns the sync off (read once).
pub(super) fn on() -> bool {
	static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
	*ON.get_or_init(|| std::env::var("RAC2_GSSYNC").map_or(true, |v| v.trim() != "0"))
}

impl SceneSync {
	/// The scene to draw while the GS is through field `done`: the newest kept scene not newer than `done - LAG` (none kept: `newest`).
	/// Also the scene of the field before it when the pick skipped one (the pipe caught up by two fields): frame smoothing is given it
	/// first, so it blends from the right field.
	pub(super) fn pick(&mut self, newest: Scene, done: u64, sync: bool) -> (Option<Scene>, Scene) {
		if self.scenes.back().is_none_or(|s| s.field != newest.field) {
			if self.scenes.back().is_some_and(|s| s.field > newest.field) {
				// A new boot / state: the old fields are gone.
				self.scenes.clear();
			}
			if self.scenes.len() == KEEP {
				self.scenes.pop_front();
			}
			self.scenes.push_back(newest.clone());
		}
		if !sync {
			return (None, newest);
		}
		let want = done.saturating_sub(LAG);
		let Some(i) = self.scenes.iter().rposition(|s| s.field <= want) else { return (None, newest) };
		let pick = self.scenes[i].clone();
		let skipped = self.last.is_some_and(|l| pick.field > l + 1);
		let before = (skipped && i > 0 && self.scenes[i - 1].field + 1 == pick.field).then(|| self.scenes[i - 1].clone());
		self.last = Some(pick.field);
		(before, pick)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn sc(field: u64) -> Scene {
		Scene { field, ..Default::default() }
	}

	#[test]
	fn the_scene_follows_the_gs_field() {
		let mut s = SceneSync::default();
		// The game is at field 5, the GS through 4: the picture of 4 was drawn with camera 3.
		for f in 1..=5 {
			s.pick(sc(f), 0, true);
		}
		assert_eq!(s.pick(sc(5), 4, true).1.field, 3);
		// The pipe catches up by two: scene 5 with its field before (4) for the smoothing.
		let (b, p) = s.pick(sc(6), 6, true);
		assert_eq!((b.map(|x| x.field), p.field), (Some(4), 5));
		// Off: the newest.
		assert_eq!(s.pick(sc(7), 6, false).1.field, 7);
		// Nothing kept that old: the newest.
		let mut t = SceneSync::default();
		assert_eq!(t.pick(sc(9), 3, true).1.field, 9);
	}
}
