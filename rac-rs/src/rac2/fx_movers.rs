//! [PC] [b45 P7] Water Enhanced's rings and wakes (engine viewer/waterfx: `Rings`, `Wakes`): who touches the water. R&C1 hands the engine the
//! hero's feet and the drawn mobys with their bounding radius every field (rac1/retail/snapshot.rs `moby_fx`, `PoolFx::hero` / `movers`);
//! R&C2 handed `None` and nothing, so no ring or wake ever appeared. The feed is the scene's own (scene.rs: Ratchet = the first moby record,
//! `Scene::hero_group`; `Scene::movers`), kept here for the redraw (host/ov_fire.rs reads it while Fire / Smoke / lights run; without them
//! `water_only` hands it alone, so Water has its own gate: the Water option).
use crate::scene::Scene;
use rac_engine::fxpool::PoolFx;
use rac_engine::viewer::{Camera, Gpu};
use std::sync::Mutex;

/// The hero's feet and the movers of one field, with the field count the engine tells fields apart by.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct Feed {
	pub frames: u32,
	pub hero: Option<[f32; 3]>,
	pub movers: Vec<(u32, [f32; 3], f32)>,
}

static LAST: Mutex<Option<Feed>> = Mutex::new(None);

/// What `scene` hands the water: Ratchet's position (the first record) and the mobies the game drew.
pub fn of_scene(scene: &Scene) -> Feed {
	let hero = scene.hero_group.first().map(|h| [h[0], h[1], h[2]]);
	Feed { frames: scene.field as u32, hero, movers: scene.movers.iter().copied().filter(|m| m.1.iter().all(|v| v.is_finite())).collect() }
}

/// Every redraw, before the layers (host/frame.rs): keeps this redraw's scene feed.
pub fn note(scene: &Scene) {
	*LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some(of_scene(scene));
}

fn last() -> Feed {
	LAST.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default()
}

/// Water Enhanced is on (it needs Lighting Enhanced, options.rs `graphics`).
pub fn wanted() -> bool {
	let g = crate::options::graphics();
	g.lighting != 0 && g.water_q != 0
}

/// The last noted feed as the pool feed of the engine (host/ov_fire.rs `step`): `own` outlives the returned value.
pub fn pool_fx<'a>(own: &'a Feed, sprite_slots: &'a [u16], sprite_types: &'a [u8], sprite_texs: &'a [u8]) -> PoolFx<'a> {
	PoolFx {
		sprite_slots,
		sprite_types,
		sprite_texs,
		ribbons: &[],
		lines: &[],
		line_slots: &[],
		frames: own.frames,
		hero: own.hero,
		movers: &own.movers,
		shots: &[],
		blasts: &[],
		blast_gate: Some(&[]),
		flame_lights: &[],
		tick: &[],
		tick_hero: None,
	}
}

/// The newest noted feed (host/ov_fire.rs).
pub fn current() -> Feed {
	last()
}

/// Fire, Smoke and the weapon lights are off but Water is on: the engine gets the hero and the movers alone (no sprites), once per redraw.
/// Returns whether it handed anything (then the caller must not call `Fire::off` state again; the engine holds no sprites anyway).
pub fn water_only(g: &mut Gpu, cam: &Camera) -> bool {
	if !wanted() {
		return false;
	}
	let f = last();
	g.set_particles(&[], &[], &[], &[], cam, Some(pool_fx(&f, &[], &[], &[])));
	true
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_feed_is_ratchet_and_the_drawn_mobies() {
		let s = Scene { field: 7, hero_group: vec![[1.0, 2.0, 3.0, 1.1]], movers: vec![(0x100, [4.0, 5.0, 6.0], 0.5), (0x200, [f32::NAN, 0.0, 0.0], 1.0)], ..Scene::default() };
		let f = of_scene(&s);
		assert_eq!((f.frames, f.hero), (7, Some([1.0, 2.0, 3.0])));
		assert_eq!(f.movers, vec![(0x100, [4.0, 5.0, 6.0], 0.5)]);
		assert_eq!(of_scene(&Scene::default()).hero, None);
	}
}
