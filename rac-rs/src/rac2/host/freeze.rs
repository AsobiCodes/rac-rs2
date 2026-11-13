//! The world under a frozen-screen menu (engine `gsfull/grab.rs`): the title and pause menus show the picture the game froze when the
//! menu opened. The native world is drawn from the game's RAM every redraw, and while the pause menu is up the game re-points its
//! camera (the pause field's view is the sky, not the scene that was frozen [C: window shots, fields 6790 / 6800]), so the window keeps
//! the last scenes it saw and, while the GS reports a frozen run (`GsPipe::frozen_from`: the field the run began at), shows the newest
//! one from before that field.

use crate::scene::Scene;
use std::collections::VecDeque;

/// Recent scenes by field (the window sees them a little after the GS thread flags the freeze), and the one held for the current run.
#[derive(Default)]
pub(super) struct Freeze {
	hist: VecDeque<Scene>,
	held: Option<(u64, Scene)>,
	/// The no-world flag of the newest live GS picture (`no_world`).
	live_no_world: bool,
	/// [b46 U4] How many live pictures in a row had no world (a minigame's black lasts many; the vendor / map open's one grab field is one).
	nw_run: u32,
}

/// Scenes kept: more than the fields the window can lag behind the GS thread.
const KEEP: usize = 12;

/// The game's camera is re-pointed up to this many fields before the first field that uploads the grabbed picture [C: field 6800 of the
/// user's session: a sky view, first upload field 6801], so the held scene is that much older.
const LEAD: u64 = 3;

/// [b46 U4] Live no-world pictures in a row that make a minigame (kept through a pause over it).
const NW_MIN: u32 = 3;

/// The held key of a hold the pause root began (no GS run flagged yet).
const EARLY: u64 = u64::MAX;

impl Freeze {
	/// The scene to draw: `newest` normally; during a frozen run (`frozen_from` != 0, the first field that uploads the grabbed picture)
	/// the newest scene `LEAD` fields older than that field, kept until the run ends.
	/// [PC] A scene with the pause root up (`Scene::paused`, read from RAM after its field) holds too, from the field the game re-points
	/// its camera at, two fields before the GS flags the run (one field of the re-pointed, black world otherwise): the newest scene from
	/// before it, kept when the GS's run takes over.
	pub(super) fn scene(&mut self, newest: Scene, frozen_from: u64) -> Scene {
		if frozen_from == 0 && !newest.paused {
			self.held = None;
			if self.hist.back().is_none_or(|s| s.field != newest.field) {
				self.hist.push_back(newest.clone());
				if self.hist.len() > KEEP {
					self.hist.pop_front();
				}
			}
			return newest;
		}
		let key = if frozen_from != 0 { frozen_from } else { EARLY };
		match &mut self.held {
			Some(h) if h.0 == key => {}
			// The run the pause root began (or the root still up after the GS's run ended): the same scene.
			Some(h) if h.0 == EARLY || key == EARLY => h.0 = key,
			_ => {
				// [b46 U4] Vendor / Ship Shack / Galactic Map open: the game draws ONE no-world field (a grab, no 3D) right before the
				// frozen run [C: u43328 fields 72-74, segs only program 2] -> the flag it latched would hide the held world (black backdrop).
				// A hold that starts after fewer than `NW_MIN` no-world pictures is not over a minigame: the held world is whole.
				if self.nw_run < NW_MIN {
					self.live_no_world = false;
				}
				let before = if key == EARLY { self.hist.iter().rev().find(|s| s.field < newest.field) } else { self.hist.iter().rev().find(|s| s.field + LEAD <= key) };
				let before = before.cloned();
				eprintln!("rac2-play: frozen screen from field {}: showing the scene of field {} (newest {})", if key == EARLY { newest.field } else { key }, before.as_ref().map_or(0, |s| s.field), newest.field);
				self.held = Some((key, before.unwrap_or_else(|| newest.clone())));
			}
		}
		self.held.as_ref().map_or(newest, |h| h.1.clone())
	}

	/// The last `scene` call gave the held scene (a frozen run is on): the water under it is that scene's own (rac2/water.rs `held`),
	/// and the Enhanced water's clock stands (engine viewer/water_hold.rs).
	pub(super) fn holding(&self) -> bool {
		self.held.is_some()
	}

	/// [PC] The engine's no-world flag (engine viewer/no_world.rs) after `scene`: the newest GS picture's (`pic`; None = no new
	/// picture this redraw) while live, the last live picture's while a scene is held. The pause field's own picture draws no world
	/// [C: fg1 0357 field 302, the held scene's terrain and sea left out, only its mobies drawn], yet the held scene is drawn whole;
	/// a pause over a minigame keeps the minigame's black.
	pub(super) fn no_world(&mut self, pic: Option<bool>) -> bool {
		if let Some(p) = pic.filter(|_| !self.holding()) {
			self.live_no_world = p;
			self.nw_run = if p { self.nw_run + 1 } else { 0 };
		}
		self.live_no_world
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn at(field: u64) -> Scene {
		Scene { field, ..Default::default() }
	}

	#[test]
	fn shows_the_newest_scene_from_before_the_freeze_until_it_ends() {
		let mut f = Freeze::default();
		for n in 1..=20 {
			assert_eq!(f.scene(at(n), 0).field, n);
		}
		assert!(!f.holding());
		// the GS thread flags a run that began at field 18 once the window is at 21
		assert_eq!(f.scene(at(21), 18).field, 15);
		assert!(f.holding());
		// the history rolls on (no new scenes are stored during the run), the held scene stays
		for n in 22..60 {
			assert_eq!(f.scene(at(n), 18).field, 15);
		}
		// the run ends: live again; a new run holds a newer scene
		for n in 60..70 {
			assert_eq!(f.scene(at(n), 0).field, n);
		}
		assert!(!f.holding());
		assert_eq!(f.scene(at(71), 68).field, 65);
	}

	#[test]
	fn the_pause_root_holds_from_its_field_before_the_gs_run() {
		let paused = |field: u64| Scene { field, paused: true, ..Default::default() };
		let mut f = Freeze::default();
		for n in 290..=301 {
			assert_eq!(f.scene(at(n), 0).field, n);
		}
		// fg1 0357: the root is up from 302 (the camera's re-point), the GS flags the run that began at 304 two fields later
		assert_eq!(f.scene(paused(302), 0).field, 301);
		assert!(f.holding());
		assert_eq!(f.scene(paused(303), 0).field, 301);
		for n in 304..331 {
			assert_eq!(f.scene(paused(n), 304).field, 301);
		}
		// the root closes at 331 while the GS still shows the grabbed picture; then live with no step back
		for n in 331..334 {
			assert_eq!(f.scene(at(n), 304).field, 301);
		}
		assert_eq!(f.scene(at(334), 0).field, 334);
		assert!(!f.holding());
		// the GS's run ends while the root is still up: the same scene stays held until both are gone
		assert_eq!(f.scene(paused(340), 0).field, 334);
		assert_eq!(f.scene(paused(345), 343).field, 334);
		assert_eq!(f.scene(paused(350), 0).field, 334);
		assert_eq!(f.scene(at(351), 0).field, 351);
	}

	#[test]
	fn a_held_scene_keeps_the_no_world_flag_of_the_last_live_picture() {
		let paused = |field: u64| Scene { field, paused: true, ..Default::default() };
		let mut f = Freeze::default();
		f.scene(at(300), 0);
		assert!(!f.no_world(Some(false)));
		assert!(!f.no_world(None));
		// the pause field's picture has no world segments: the held world is drawn whole
		f.scene(paused(302), 0);
		assert!(!f.no_world(Some(true)));
		f.scene(paused(305), 304);
		assert!(!f.no_world(Some(true)));
		// live again: the pictures' own flag (a minigame's black), kept through a pause over it
		f.scene(at(340), 0);
		assert!(f.no_world(Some(true)));
		f.scene(at(341), 0);
		assert!(f.no_world(Some(true)));
		f.scene(at(342), 0);
		assert!(f.no_world(Some(true)));
		f.scene(paused(343), 0);
		assert!(f.no_world(Some(false)));
		f.scene(at(350), 0);
		assert!(!f.no_world(Some(false)));
	}

	#[test]
	fn one_no_world_grab_field_before_a_vendor_hold_does_not_hide_the_world() {
		let mut f = Freeze::default();
		f.scene(at(72), 0);
		assert!(!f.no_world(Some(false)));
		f.scene(at(73), 0);
		assert!(f.no_world(Some(true)), "the grab field itself");
		f.scene(at(75), 76);
		assert!(!f.no_world(Some(false)), "the held world is whole");
		assert!(!f.no_world(None));
	}

	#[test]
	fn without_history_the_newest_scene_is_held() {
		let mut f = Freeze::default();
		assert_eq!(f.scene(at(30), 25).field, 30);
	}
}
