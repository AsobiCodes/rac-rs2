//! [PC] The world install without the freeze (stutter class 1, round 3 lane stut2): the level's scene (`build_scene`: the batches
//! of terrain, ties and shrubs, the texture uploads) is built on a thread of its own with the engine's `SceneCtx` (the way R&C1's
//! level prefetch does it, rac1/host/app.rs), not on the render thread, which only swaps the finished scene in (`Stage::install`).
//! Until then the redraw shows what it showed before (the old level's world, or the GS's whole picture: the game's own load screen
//! or the loading level's frames), as it already did while the world decoded. Window shots (lockstep) build inline, so a field's
//! picture never depends on a thread's timing.
use super::*;
use rac_engine::viewer::gpu::SceneCtx;
use rac_engine::viewer::Scene as GpuScene;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::time::Instant;

/// A scene build in flight, and the newest world that arrived meanwhile (only the newest is built after it).
pub(super) struct Job {
	rx: Receiver<Built>,
	waiting: Option<World>,
	/// Finished builds dropped in a row for a newer world (review A5: capped, so worlds arriving faster than a build still install).
	dropped: u32,
}

/// Finished builds dropped in a row before one is installed anyway (the newer world is built right after it).
const MAX_DROPS: u32 = 1;

/// A world with the engine scene built from it.
pub(super) struct Built {
	scene: Option<GpuScene>,
	world: World,
	build_ms: f64,
}

/// Frees `t` on a short-lived thread (an old scene's buffers, a skipped world): not on the render thread.
fn drop_later<T: Send + 'static>(t: T) {
	let _ = std::thread::Builder::new().name("rac2-drop".into()).spawn(move || drop(t));
}

/// The scene of `w` (what `Gpu::load_scene` builds for it: no prop slots yet, they grow with the game's moby list). Runs on the
/// thread that owns `ctx`: the texture pack's replacements for this level are taken on it too.
fn build(ctx: SceneCtx, mut w: World) -> Built {
	let t0 = Instant::now();
	rac_engine::texpack::prepared::adopt(w.repl.take());
	crate::fur::use_level(w.level);
	let mut scene = ctx.build_scene(&w.terrain, &w.shrubs, &w.ties, &w.textures, w.sky.as_ref(), None, None, usize::MAX, |_| Some(Vec::new()), &w.classes.meshes, &HashMap::new(), Occlusion::new(None, None), Vec::new(), 0.0, &[], &[], &[]);
	if let Some(sc) = scene.as_mut() {
		ctx.tag_ice(sc, w.level); // the first draw's once-per-level ice marks, here instead of on the render thread
	}
	// Texture packs: the plant planner's grass colours from the HD ground textures (decodes pack files: here, not in `install`;
	// after the scene, which keeps the original textures).
	rac_engine::texpack::extra_grass::apply(w.level, &mut w.textures, &mut Default::default());
	Built { scene, world: w, build_ms: t0.elapsed().as_secs_f64() * 1e3 }
}

fn start(g: &Gpu, w: World) -> Option<Job> {
	let ctx = SceneCtx { paced: true, ..g.ctx() };
	let (tx, rx) = channel();
	match rac_engine::viewer::bgpool::spawn("rac2-scene", 64 << 20, move || {
		rac_engine::viewer::gpu::pace_texture_uploads(); // this thread's texture uploads pause now and then (the render thread's submit)
		let _ = tx.send(build(ctx, w));
	}) {
		Ok(_) => Some(Job { rx, waiting: None, dropped: 0 }),
		Err(e) => {
			eprintln!("rac2: scene build thread: {e}");
			None
		}
	}
}

/// What a redraw does with the build in flight: nothing (still running), install it, or drop it for the newer world waiting.
#[derive(Debug, PartialEq)]
enum Next {
	Idle,
	Install,
	Rebuild,
}

/// `dropped`: finished builds already dropped in a row; at `MAX_DROPS` the finished one installs even with a newer world waiting.
fn next(finished: bool, newer_waiting: bool, dropped: u32) -> Next {
	match (finished, newer_waiting) {
		(false, _) => Next::Idle,
		(true, true) if dropped < MAX_DROPS => Next::Rebuild,
		(true, _) => Next::Install,
	}
}

impl Stage {
	/// Every redraw: `new` is the world the loader just finished (if any). Builds it off the render thread (inline with `sync`),
	/// installs the finished build. At most one build runs; a world that arrives meanwhile replaces any older one waiting.
	pub(super) fn world_step(&mut self, g: &mut Gpu, new: Option<World>, sync: bool) {
		if sync {
			if let Some(w) = new {
				let b = build(g.ctx(), w);
				self.install(g, b);
			}
			return;
		}
		if let Some(w) = new {
			match &mut self.job {
				Some(j) => j.waiting = Some(w),
				None => self.job = start(g, w),
			}
		}
		let done = match self.job.as_ref().map(|j| j.rx.try_recv()) {
			Some(Ok(b)) => Some(b),
			Some(Err(TryRecvError::Disconnected)) => {
				eprintln!("rac2: scene build thread ended without a scene");
				let waiting = self.job.take().and_then(|j| j.waiting);
				self.job = waiting.and_then(|w| start(g, w));
				None
			}
			_ => None,
		};
		let Some(b) = done else { return };
		let (waiting, dropped) = self.job.take().map_or((None, 0), |j| (j.waiting, j.dropped));
		match (next(true, waiting.is_some(), dropped), waiting) {
			// A newer world is already decoded: this one would be replaced at once.
			(Next::Rebuild, Some(w)) => {
				self.job = start(g, w);
				if let Some(j) = self.job.as_mut() {
					j.dropped = dropped + 1;
				}
				drop_later(b);
			}
			// Installed anyway after `MAX_DROPS` drops in a row; the newer world is built next.
			(_, Some(w)) => {
				self.install(g, b);
				self.job = start(g, w);
			}
			_ => self.install(g, b),
		}
	}

	/// The render thread's half of a world change: the finished scene into the engine, the level's lights, an empty prop pool
	/// (it grows with the game's moby list, `Classes::sync`), the tie colour capture started, the plants planned.
	fn install(&mut self, g: &mut Gpu, b: Built) {
		let t0 = Instant::now();
		let Built { scene, world, build_ms } = b;
		let World { level, terrain, chunk, shrubs, ties, tie_cols, textures, classes, lighting, coll, .. } = world;
		crate::fur::use_level(level); // [PC] fur: this level's Ratchet class is the fur prop (fur.rs)
		drop_later(std::mem::replace(&mut g.scene, scene));
		g.lighting = Some(lighting);
		g.level = level;
		tie_cols.start();
		self.water.clear(g);
		crate::veg::plan(g, coll.as_deref(), terrain, shrubs, textures, ties, &classes.meshes, level); // [PC] Vegetation (veg.rs)
		let job = self.job.take();
		*self = Stage { level: Some(level), classes, tie_cols, coll, job, ..Default::default() };
		// The render thread's hitch of a world change (a level, or the game streaming another terrain chunk in).
		eprintln!("rac2: world installed: level {level} chunk {chunk}, {:.1} ms on the render thread (scene build {build_ms:.1} ms) at log t {:.0}", t0.elapsed().as_secs_f64() * 1e3, super::super::framelog::now_ms());
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn is_send<T: Send>() {}

	/// The scene build moves a world and the engine's scene context to its thread and the finished scene back.
	#[test]
	fn build_types_cross_threads() {
		is_send::<World>();
		is_send::<Built>();
		is_send::<SceneCtx>();
	}

	#[test]
	fn a_newer_world_replaces_a_finished_one() {
		assert_eq!(next(false, true, 0), Next::Idle);
		assert_eq!(next(false, false, 0), Next::Idle);
		assert_eq!(next(true, false, 0), Next::Install);
		assert_eq!(next(true, true, 0), Next::Rebuild);
	}

	/// Review A5: worlds arriving faster than a build lasts still install one (every second build at worst).
	#[test]
	fn drops_in_a_row_are_capped() {
		assert_eq!(next(true, true, MAX_DROPS), Next::Install);
		assert_eq!(next(false, true, MAX_DROPS), Next::Idle);
		let (mut dropped, mut installs) = (0, 0);
		for _ in 0..10 {
			match next(true, true, dropped) {
				Next::Rebuild => dropped += 1,
				_ => {
					installs += 1;
					dropped = 0;
				}
			}
		}
		assert!(installs >= 5, "{installs}");
	}
}
