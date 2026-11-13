//! [b50 V9] [PC] RAC_DIAG (engine/diag.rs): R&C2's facts for the diagnostic record (`Pack::diag_facts`): the level, whether the
//! game shows a menu / movie / pause, the hero's position and the camera mode. The window publishes them once per redraw while
//! RAC_DIAG is on (host/frame.rs `redraw`, a copy of the scene it already holds); the engine reads them once per second. No RAM is
//! read or written here.

use crate::scene::Scene;
use rac_engine::diag::Facts;
use std::sync::Mutex;

static FACTS: Mutex<Option<Facts>> = Mutex::new(None);

/// What a row's facts are made of (a plain copy, so the mapping is testable without a window).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inputs {
	/// The scene's level word and whether it has a 3D camera (no camera = title, menus, loading, a movie's black).
	pub level: i32,
	pub has_view: bool,
	pub paused: bool,
	/// A movie plays in the window (movie.rs).
	pub movie: bool,
	/// The hero's world position (the first sphere of `Scene::hero_group`).
	pub hero: Option<[f32; 3]>,
	pub modern_camera: bool,
	/// First person is up (modcam_fp.rs `live`).
	pub first_person: bool,
}

/// The inputs of one scene.
pub fn inputs_of(s: &Scene, movie: bool, modern_camera: bool, first_person: bool) -> Inputs {
	Inputs { level: s.level, has_view: s.view.is_some(), paused: s.paused, movie, hero: s.hero_group.first().map(|h| [h[0], h[1], h[2]]), modern_camera, first_person }
}

/// The facts of a row. "menu" (the engine's front-end state word) is every second without a 3D game camera and every second
/// a movie plays (the engine has no separate "movie" word: ENGINE NEEDED in handoff_V9.md); the level is named only while a 3D
/// world is up (the level word of a title screen is stale); the hero and the camera only while the game is played or paused.
pub fn facts_of(i: &Inputs) -> Facts {
	let menu = !i.has_view || i.movie;
	let known = !menu &&(0..crate::pc_devtools::NAMES.len() as i32).contains(&i.level);
	Facts {
		level: if known { crate::pc_devtools::level_name(i.level as usize) } else { "" },
		level_id: if known { i.level } else { -1 },
		paused: i.paused && !menu,
		menu,
		movie: i.movie,
		hero: i.hero.filter(|_| !menu && i.hero.is_some_and(|h| h.iter().all(|x| x.is_finite()))),
		camera: if menu { "" } else { camera_name(i.modern_camera, i.first_person) },
	}
}

/// The camera mode's word: first person (retail's free look) over the Camera row's Modern / Classic.
pub fn camera_name(modern: bool, first_person: bool) -> &'static str {
	if first_person {
		"first_person"
	} else if modern {
		"modern"
	} else {
		"classic"
	}
}

/// The window's facts of this redraw (host/frame.rs, only while RAC_DIAG is on).
pub fn publish(s: &Scene) {
	let f = facts_of(&inputs_of(s, crate::movie::playing(), crate::modcam_ui::modern(), crate::modcam_fp::live()));
	*FACTS.lock().unwrap_or_else(|e| e.into_inner()) = Some(f);
}

/// `Pack::diag_facts`.
pub fn get() -> Facts {
	FACTS.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default()
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::scene::View;

	fn view() -> View {
		View { eye: [0.0; 3], basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], fx: 500.0, fy: 500.0, half: [256.0, 224.0] }
	}

	fn scene(level: i32, with_view: bool, paused: bool) -> Scene {
		Scene { level, view: with_view.then(view), paused, hero_group: vec![[1.5, -2.0, 30.25, 0.6], [9.0, 9.0, 9.0, 0.2]], ..Scene::default() }
	}

	#[test]
	fn play_gives_level_hero_and_camera() {
		let f = facts_of(&inputs_of(&scene(3, true, false), false, true, false));
		assert_eq!((f.level, f.level_id, f.menu, f.paused, f.camera), ("ENDAKO (MEGAPOLIS)", 3, false, false, "modern"));
		assert_eq!(f.hero, Some([1.5, -2.0, 30.25]));
		assert_eq!(rac_engine::diag::stats::state(&f, 0), "play");
		assert_eq!(facts_of(&inputs_of(&scene(3, true, false), false, false, false)).camera, "classic");
		assert_eq!(facts_of(&inputs_of(&scene(3, true, false), false, true, true)).camera, "first_person");
	}

	#[test]
	fn pause_keeps_the_world_facts() {
		let f = facts_of(&inputs_of(&scene(0, true, true), false, true, false));
		assert!(f.paused && !f.menu);
		assert_eq!((f.level_id, f.hero.is_some()), (0, true));
		assert_eq!(rac_engine::diag::stats::state(&f, 0), "paused");
	}

	#[test]
	fn no_camera_or_a_movie_is_the_front_end() {
		for (view, movie) in [(false, false), (true, true), (false, true)] {
			let f = facts_of(&inputs_of(&scene(5, view, true), movie, true, false));
			assert!(f.menu && !f.paused, "view {view} movie {movie}");
			assert_eq!((f.level, f.level_id, f.hero, f.camera), ("", -1, None, ""));
			assert_eq!(rac_engine::diag::stats::state(&f, 0), if movie { "movie" } else { "menu" });
			assert_eq!(f.movie, movie);
		}
	}

	#[test]
	fn a_stale_level_word_or_a_bad_hero_is_dropped() {
		let f = facts_of(&inputs_of(&scene(99, true, false), false, true, false));
		assert_eq!((f.level, f.level_id), ("", -1));
		let mut s = scene(1, true, false);
		s.hero_group = vec![[f32::NAN, 0.0, 0.0, 0.6]];
		assert_eq!(facts_of(&inputs_of(&s, false, true, false)).hero, None);
		s.hero_group.clear();
		assert_eq!(facts_of(&inputs_of(&s, false, true, false)).hero, None);
	}
}
