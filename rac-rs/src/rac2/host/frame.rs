//! One redraw of the R&C2 window (host.rs): the newest GS picture as the engine's retail overlay (the HUD and effects over the
//! world, or the whole picture in menus and loads), a freshly decoded world into the engine (`Stage::load`), this field's
//! mobies, metal, tie colours, shadows and water from the game's RAM (`Stage::field`), the overlay quad and the black bars in the
//! engine's HUD list, the engine's draw; in window-shot runs the picture of each field asked for (`Shots`).

mod metal_env;
mod world_build;

use super::App;
use crate::props::Classes;
use crate::scene::Scene;
use crate::shots::Opts;
use crate::tiecol::TieCols;
use crate::world::World;
use rac_engine::gsfull::ovl_pipe::Frame;
use rac_formats::occlusion::Occlusion;
use rac_engine::viewer::targets::{RETAIL_HD_TEX, RETAIL_OV_TEX};
use rac_engine::viewer::{Camera, Gpu};
use rac_formats::level::{CHROME_KEY, GLASS_KEY, Texture};
use std::collections::HashMap;
use std::sync::atomic::Ordering;

/// The world the engine has: its level, prop classes and tie colours, and what the metal draw was given.
#[derive(Default)]
pub(super) struct Stage {
	level: Option<usize>,
	classes: Classes,
	tie_cols: TieCols,
	/// The environment map and the classes with metal packets the engine's metal draw was given (frame/metal_env.rs).
	metal: metal_env::Given,
	/// The water textures of this level.
	water: crate::water::Water,
	/// Test log of the last field drawn (window shots): the game's mobies, the props drawn, the classes left out.
	pub(super) drawn: (usize, usize, Vec<(u32, &'static str)>),
	/// [PC] The same log's left-out mobies (moby_cull.rs): read this field, and props cast (drawn by Culling Off).
	pub(super) cut: (usize, usize),
	/// [PC] The level's collision (veg.rs): the plants' replans read it.
	coll: Option<std::sync::Arc<rac_formats::physics::World>>,
	/// [PC] The scene build in flight (frame/world_build.rs).
	job: Option<world_build::Job>,
}

impl Stage {
	/// Before the first world: an empty scene, so the engine presents the GS picture of the boot and the menus.
	pub(super) fn empty(g: &mut Gpu) {
		g.load_scene(&[], &[], &[], &HashMap::new(), None, None, None, usize::MAX, &[], &HashMap::new(), &HashMap::new(), Occlusion::new(None, None), Vec::new(), 0.0, &[], &[], &[]);
	}

	/// This field's world from the game's RAM; its camera, or None when the engine's world is not this field's level or the
	/// game has no 3D camera. `fill`: the window's sides are shown (`fills`). `held`: `scene` is the one held under a frozen screen
	/// (host/freeze.rs): its own water (rac2/water.rs).
	fn field(&mut self, g: &mut Gpu, scene: &Scene, blended: Option<(u64, f32)>, fill: bool, held: bool) -> Option<Camera> {
		let view = scene.view.filter(|_| self.level.is_some_and(|l| l as i32 == scene.level))?;
		super::framelog::sub_start();
		if self.classes.take_ram_classes(&scene.ram_classes) {
			self.classes.mark_stale();
		}
		if !self.classes.sync(g, scene) {
			self.log_refused(scene);
			return None;
		}
		super::framelog::sub(0);
		let window = g.config.width as f32 / g.config.height.max(1) as f32;
		let cam = view.camera(window);
		g.set_clip_view(Some((&cam, window)));
		// [b46 U2] The guard-band clipper draws a clipped face as retail's list-order strip, which leaves a wedge of a big polygon
		// undrawn (the Aranos glass floor under the Modern camera: wedges missing toward the lower corners). R&C1 draws the whole
		// polygon (a fan) with its Modern camera (rac1 frame_start.rs); R&C2 never set it. Retail camera: retail's strip.
		rac_engine::viewer::set_complete_polys(crate::modcam_ui::modern());
		self.metal(g, scene.chrome);
		g.ship_fight = scene.ship_fight; // [b45 P7] a starship fight has no sun to cast from (engine sunshadow.rs `casts`)
		// [PC] Underwater look (engine underwater.rs): the game's camera flag (under.rs); no surface height known for R&C2.
		(g.under_flag, g.under_surf) = (rac_engine::viewer::under_detect::camera_under(scene.under, None, cam.pos[2]), None);
		g.fog_live = scene.fog_live; // [b46 W2] the game's live fog block (zone fog / underwater swap), one field late as R&C1's SwapWords
		// [PC] The mobies the game left out (moby_cull.rs): Culling Off draws them all; the side restore shows the side-only ones
		// inside the drawn view, on a blended frame (its camera trails the newest field's) or in a filled wide window.
		let far = crate::pc_devtools::far(crate::options::graphics().draw); // [PC] Debug mode's F3 A/B (pc_devtools.rs)
		let restore = crate::moby_cull::Restore {
			culling_off: !crate::options::graphics().culling,
			far: far.is_some(),
			side: (blended.is_some() || fill).then(|| crate::moby_cull::Planes::of(&view, window, fill)),
		};
		super::framelog::sub(1);
		let f = self.classes.frame(scene, &restore);
		super::framelog::sub(2);
		self.drawn = (scene.mobies.iter().filter(|m| m.cut.is_none()).count(), f.props.iter().filter(|p| p.shown).count(), self.classes.undrawn(scene));
		self.cut = (scene.mobies.iter().filter(|m| m.cut.is_some()).count(), f.props.iter().filter(|p| p.cast).count());
		rac_engine::viewer::PROP_TINTS.with(|t| t.borrow_mut().clone_from(&f.tints));
		rac_engine::viewer::PROP_TINT_ENDS.with(|t| t.borrow_mut().clone_from(&f.tint_ends));
		g.set_props(Some(&f.props), &HashMap::new(), &f.fx_posed, &(), &[], &[], &[], &f.centres, &f.radii);
		super::framelog::sub(3);
		g.set_volumes_retail(&scene.shadows, &f.late, false);
		self.tie_cols.apply(g);
		super::framelog::sub(4);
		self.water.field(g, blended, scene.field, held);
		super::framelog::sub(5);
		crate::veg::field(g, self.coll.as_deref(), scene); // [PC] the plants bend away from Ratchet and the drawn mobies
		super::framelog::sub(6);
		Some(cam)
	}

	/// The engine refused this field's prop slots (`Classes::sync` false), so the redraw shows the GS's whole picture: a flip from
	/// the native world that looks like a flicker of everything. Counted; the first 20 go to stderr, then (RAC2_LOG only) every 200th:
	/// a level that keeps refusing must not fill the player's log (review B3/4 N6).
	fn log_refused(&self, scene: &Scene) {
		static REFUSED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
		static LOG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
		let n = REFUSED.fetch_add(1, Ordering::Relaxed) + 1;
		if n <= 20 || (n % 200 == 0 && *LOG.get_or_init(|| std::env::var_os("RAC2_LOG").is_some())) {
			let missing = self.classes.missing_textures();
			eprintln!(
				"rac2: native world refused #{n} at field {} (level {}): the engine would not build the prop slots (no engine scene, or a face texture it has no slot for); slots {}, mobies {}, face textures not in the class textures {}: {:x?}",
				scene.field,
				scene.level,
				self.classes.slots.len(),
				scene.mobies.len(),
				missing.len(),
				&missing[..missing.len().min(8)]
			);
		}
	}

	/// The metal packets' environment map (gsfull::texreq), given to the engine's metal draw as its chrome and glass (lane rc2 drew
	/// every metal face with it): texreq's newest CONFIRMED read, again at each new confirmed version and whenever the classes with
	/// metal packets change (frame/metal_env.rs; build 43 kept one snapshot per class count, which could be a read of the frame
	/// buffer's alpha plane drawn over: speckled helmet, visor and Clank).
	fn metal(&mut self, g: &mut Gpu, chrome: Option<u64>) {
		use rac_engine::gsfull::texreq;
		let Some(t0) = chrome else { return };
		// Asked again whenever texreq does not hold it (first ask, a level reset, or a prune after long undrawn stretches: pause pages).
		if !texreq::is_held(t0) {
			texreq::want(t0);
		}
		let confirmed = texreq::get_confirmed(t0);
		metal_env::log_held(t0, confirmed.is_some());
		// [b49 U2] The held weapon's class id is the same for every weapon (its buffer's address): the key carries a fingerprint of the
		// metal packets, so a weapon switch reloads them (the old weapon's glass / chrome blades stayed on the new one).
		let mut classes: Vec<u32> = metal_env::keys(self.classes.decoded.iter().filter(|(_, c)| !c.metal.is_empty()).map(|(&id, c)| (id, &c.metal[..])));
		let metal_env::Step::Load(fresh) = self.metal.step(confirmed.as_ref().map(|c| (t0, c.1)), &classes) else { return };
		let fresh = confirmed.filter(|_| fresh).map(|(t, v)| (t0, v, t));
		let Some(t) = self.metal.picture(fresh, classes) else { return };
		metal_env::log_load(t0, &t, self.metal.classes_len());
		let tex = || Texture { w: t.w, h: t.h, rgba: t.rgba.clone() };
		let textures = HashMap::from([(CHROME_KEY, tex()), (GLASS_KEY, tex())]);
		let packets = self.classes.decoded.iter().filter(|(_, c)| !c.metal.is_empty()).map(|(&id, c)| (id, c.metal.clone())).collect();
		g.load_metal(packets, &textures);
	}
}

/// The overlay texels of a GS picture (premultiplied coverage alpha -> transmission).
pub(super) fn px(f: &Frame) -> Vec<[u8; 4]> {
	f.rgba.chunks_exact(4).map(|p| [p[0], p[1], p[2], if f.overlay { 255 - p[3] } else { 0 }]).collect()
}

/// The rectangle of the game's picture in a window `sw` x `sh` at `aspect` (centred, black bars around it).
pub(super) fn picture_rect(sw: f32, sh: f32, aspect: f32) -> [f32; 4] {
	let (w, h) = if sw >= sh * aspect { (sh * aspect, sh) } else { (sw, sw / aspect) };
	[(sw - w) * 0.5, (sh - h) * 0.5, w, h]
}

/// [PC] The world fills the window's width (no black bars at the sides, BATCH A CULL): the Aspect ratio row picks a shape (not
/// Auto) and the frame (the box) is wider than the game's picture `aspect`.
fn fills(g: &Gpu, aspect: f32) -> bool {
	let wide = g.config.width as f32 > g.config.height.max(1) as f32 * aspect * 1.01;
	wide && fills_for(rac_engine::viewer::aspect_box::current(), aspect)
}

/// [b45 P7] Which picture shapes fill a wider window: any with a chosen Aspect ratio; with Auto (R&C1: the window) the widescreen picture
/// (16:9, the game's own widescreen row) fills an ultrawide window, while the 4:3 picture keeps its retail bars.
fn fills_for(r: rac_engine::viewer::aspect_box::Ratio, aspect: f32) -> bool {
	r != rac_engine::viewer::aspect_box::Ratio::Auto || aspect >= 1.7
}

/// The engine's HUD list of this redraw: the overlay quad (`quad`) in the middle of the window at the game's picture shape
/// (`aspect`: 4:3, or 16:9 with the game's widescreen row), black bars around it (not at the sides with `fill`), and black under it
/// too when the world is not drawn.
fn hud(g: &mut Gpu, world: bool, quad: bool, aspect: f32, fill: bool) {
	let (sw, sh) = (g.config.width as f32, g.config.height as f32);
	let [x, y, w, h] = picture_rect(sw, sh, aspect);
	let black = [0.0, 0.0, 0.0, 1.0];
	g.hud.clear();
	if world {
		let sides = if fill { [[0.0; 4]; 2] } else { [[0.0, 0.0, x, sh], [x + w, 0.0, sw - x - w, sh]] };
		for r in [sides[0], sides[1], [x, 0.0, w, y], [x, y + h, w, sh - y - h]] {
			if r[2] > 0.5 && r[3] > 0.5 {
				rac_engine::overlay::rect(&mut g.hud, r, black);
			}
		}
	} else {
		rac_engine::overlay::rect(&mut g.hud, [0.0, 0.0, sw, sh], black);
	}
	// [PC] HUD position / scale (hud_layout.rs): the cells over the native world, else the picture as the game draws it.
	let placed = if world && quad && g.has_retail_overlay() { crate::hud_layout::quads(sw, sh, aspect, [x, y, w, h], &crate::options::get()) } else { None };
	if let Some((ov, hd)) = placed {
		g.hud.extend(ov);
		if g.has_retail_hd() {
			g.hud.extend(hd);
		}
	} else if quad && g.has_retail_overlay() {
		g.hud.push(rac_engine::overlay::Quad { tex: RETAIL_OV_TEX, rect: [x, y, w, h], uv: [0.0, 0.0, 1.0, 1.0], colour: [1.0; 4], angle: 0.0, corners: None });
		if g.has_retail_hd() {
			g.hud.push(rac_engine::overlay::Quad { tex: RETAIL_HD_TEX, rect: [x, y, w, h], uv: [0.0, 0.0, 1.0, 1.0], colour: [1.0; 4], angle: 0.0, corners: None });
		}
	}
}

/// The camera of a redraw without the world (the world is covered).
pub(super) const IDLE: Camera = Camera { pos: [0.0; 3], yaw: 0.0, pitch: 0.0, tan_half: 0.5, up: [0.0, 0.0, 1.0] };

impl App {
	/// One redraw: the newest GS picture, a freshly decoded world, this field's world while the engine's world is the level
	/// the game is in (else the GS's whole picture), the draw; lockstep: the field drawn to the game thread.
	pub(super) fn redraw(&mut self) {
		// A movie the game plays (movie.rs) is drawn over the frame.
		super::framelog::begin();
		self.movie_step();
		let Some(g) = self.gpu.as_mut() else { return };
		rac_engine::gscap_guard::frame();
		// The sharp HUD's scale follows the picture's height in the window (the game's picture at 4:3 or 16:9).
		rac_engine::gscap::set_hd_target(picture_rect(g.config.width as f32, g.config.height as f32, self.aspect)[3] as u32, crate::options::hud_sharp());
		// Read before the picture is taken: the picture is this field's or a newer one.
		let done = self.gs.done.load(Ordering::SeqCst);
		let mut pic_no_world = None;
		if let Some(f) = self.gs.take() {
			pic_no_world = Some(f.no_world);
			crate::hud_layout::note_frame(&f); // [PC] HUD position / scale: this picture's HUD guard (hud_layout.rs)
			self.ov_hold.show(g, &f);
			self.ov_overlay = f.overlay;
			// A field the game draws no world in (a minigame over a black clear): the native draw leaves the level out (engine viewer/no_world.rs).
			rac_engine::viewer::no_world::set(f.no_world);
			if !f.frozen {
				// The camera this live picture was drawn with, kept as long as the frozen run holds it (the views of the fields are
				// kept for a while only).
				self.ov_view = self.shared.views.lock().unwrap().of_picture(f.field);
			}
			self.layers_new = Some((f.overlay, f.frozen, f.field, f.layers));
		}
		let new_world = self.shared.world.lock().unwrap().take();
		self.stage.world_step(g, new_world, self.shots.is_some());
		let newest = self.shared.view.lock().unwrap().clone();
		let game_field = newest.field;
		super::framelog::note_field(game_field); if rac_engine::diag::on() { crate::diag_facts::publish(&newest); } // [b50 V9] steps per second + the RAC_DIAG facts
		// [PC] The scene the newest GS picture was drawn with, not the game's newest (host/gs_sync.rs).
		let (prime, newest) = self.sync.pick(newest, done, super::gs_sync::on());
		if let Some(p) = &prime {
			self.smooth.scene(p, false, None);
		}
		let newest = self.freeze.scene(newest, self.gs.frozen_from.load(Ordering::SeqCst));
		// [PC] Under a frozen screen the held scene's water stands still (engine viewer/water_hold.rs), its world whole (freeze.rs `no_world`).
		let held = self.freeze.holding();
		rac_engine::viewer::water_hold::set(held);
		rac_engine::viewer::no_world::set(self.freeze.no_world(pic_no_world));
		// A GS picture belongs to the camera of its own field, which can be older than the newest (the game runs ahead of the window).
		let views = self.shared.views.lock().unwrap();
		let layers_view = |field: u64| views.of_picture(field).or(newest.view);
		if let Some((overlay, frozen, field, tris)) = self.layers_new.take() {
			// No layer is drawn where the engine draws no world (the camera is the idle one then).
			super::ov_layers::drift_log(field, &tris, &views);
			super::ov_fxlog::log(field, &tris, &views);
			let show = self.native && self.stage.level.is_some_and(|l| l as i32 == newest.level);
			let hero = views.hero_of_picture(field);
			self.ov_layers.update(g, overlay, frozen, field, &tris, layers_view(field).as_ref(), &hero, show);
			// [b47 X2] Heat haze over the lava sheet (ov_lava.rs): the sheet's points of a live overlay field with the world shown.
			let lava_view = layers_view(field).filter(|_| overlay && show && super::ov_layers::enabled());
			self.ov_layers.lava.field(newest.level, field, lava_view.as_ref().map(|v| (&tris[..], v)), overlay && frozen);
		}
		let ov_view = self.ov_view.or(newest.view);
		let live_eyes = self.livecap.as_ref().map(|_| format!("{:?}", [done.saturating_sub(2), done.saturating_sub(1), done].map(|f| (f, views.get(f).map(|v| v.eye)))));
		drop(views);
		// Frame smoothing (smooth.rs): window shots blend only with RAC_TEST_SUB=N, N frames per field at k / N.
		let sub = rac_engine::viewer::test_clock::sub();
		if self.sub_at.0 != done {
			self.sub_at = (done, 0);
		}
		let k = self.sub_at.1;
		let at = self.shots.as_ref().map(|_| k.min(sub - 1) as f32 / sub as f32);
		let blend_on = crate::options::graphics().smoothing && (self.shots.is_none() || sub > 1);
		let mut scene = self.smooth.scene(&newest, blend_on, at);
		// [PC] The free camera (Debug mode, pc_freecam.rs) replaces the drawn view; nothing changes while it is off.
		scene.view = crate::pc_freecam::apply(scene.view, scene.field);
		let free = crate::pc_freecam::on();
		let fill = fills(g, scene.view.map_or(self.aspect, |v| v.aspect()));
		// [PC] What the game thread re-tests of the mobies it left out from the next field (moby_cull.rs): nothing in lockstep shots
		// with the default options (the scene stays as before).
		let culling_off = !crate::options::graphics().culling || free;
		let far = crate::pc_devtools::far(crate::options::graphics().draw); // [PC] Debug mode's F3 A/B (pc_devtools.rs)
		crate::moby_cull::set_far(far.map(|l| (l.scale(), l.cap())));
		crate::moby_cull::set_want(if fill || blend_on { crate::moby_cull::SIDE } else { 0 } | if culling_off { crate::moby_cull::OFF } else { 0 } | if far.is_some() { crate::moby_cull::FAR } else { 0 });
		// The GS picture (the last live one while the screen is frozen) was drawn with `ov_view`: its effects follow the smoothed
		// camera (ov_warp.rs).
		g.set_retail_overlay_warp(match (&ov_view, &scene.view.filter(|_| !free)) {
			(Some(n), Some(v)) => crate::ov_warp::matrix(n, v),
			_ => None,
		});
		let ok = self.native && self.stage.level.is_some_and(|l| l as i32 == scene.level);
		self.shared.overlay.store(ok, Ordering::Relaxed);
		super::framelog::stage(0);
		let cam = if ok { self.stage.field(g, &scene, self.smooth.blended(), fill, held) } else { None };
		super::framelog::stage(1);
		// [PC] The effect layers between the two newest fields at the smoothed camera's time (ov_blend.rs).
		crate::fx_movers::note(&scene); // [b45 P7] the hero and the movers for the water rings and wakes
		self.ov_layers.redraw(g, if ok { self.smooth.blended() } else { None }, cam.as_ref());
		water_only(g, cam.as_ref());
		super::framelog::layers_done(); // [b50 V9] profiler phase "post" = the effect layers
		// [PC] The Enhanced passes (Lighting Enhanced, ambient occlusion, global illumination) run only over the world the engine
		// draws, never over the GS's whole picture.
		g.modern_ok = cam.is_some();
		self.aspect = scene.view.as_ref().map_or(self.aspect, |v| v.aspect());
		hud(g, cam.is_some(), cam.is_none() || self.ov_overlay, self.aspect, fill && cam.is_some());
		super::debug_route::frame(g, cam.is_some() && self.shared.playing.load(Ordering::Relaxed) && self.playing.is_none(), &scene); // [PC] Debug mode
		// [PC] The modern Options page over the pause menu (host/menu.rs).
		super::menu::step(g, cam.is_some());
		// [PC] The modern pause and title pages (host/screens.rs).
		super::screens::step(g, cam.is_some(), self.aspect, done);
		rac_engine::viewer::profile::meter_draw(&mut g.hud, g.config.width as f32, g.config.height as f32); // [b50 V9] RAC_FPS_METER=1 (off: one check)
		let shot = k < sub && self.shots.as_ref().is_some_and(|s| s.want(done, k));
		let live = self.livecap.as_ref().is_some_and(|l| l.want(done));
		// Screen-spot textures (screenspot.rs): the frame buffer's first pages come from the presented picture's corner.
		let (sw, sh) = (g.config.width as f32, g.config.height as f32);
		let [px, py, pw, ph] = picture_rect(sw, sh, self.aspect);
		if let Some(r) = crate::screenspot::request([px / sw, py / sh, pw / sw, ph / sh]) {
			g.want_corner(r);
		}
		super::framelog::stage(2);
		let snap = crate::pc_freecam::take_snap().filter(|_| self.shots.is_some());
		let img = draw_marked(g, cam.as_ref().unwrap_or(&IDLE), shot || live || snap.is_some()); // [b51 G5]
		super::framelog::stage(3);
		if let Some(p) = g.take_corner(self.shots.is_some()) {
			crate::screenspot::note_picture(p);
		}
		// Test: RAC2_LIVECAP (host/livecap.rs), every presented frame of a live session.
		let img = match (self.livecap.as_mut().filter(|_| live), img) {
			(Some(l), Some(i)) => {
				let info = format!("game {game_field} scene {} blended {:?} cam {:?} eyes {}", scene.field, self.smooth.blended(), cam.as_ref().map(|c| c.pos), live_eyes.as_deref().unwrap_or(""));
				l.save(done, i, &info);
				None
			}
			(_, i) => i,
		};
		if let Some(l) = self.livecap.as_mut() {
			l.end_if_done(done);
		}
		// [PC] A frozen-game picture of the test script (pc_freecam.rs `h`).
		if let (Some(s), Some(n), Some(img)) = (self.shots.as_ref(), snap, img.as_ref()) {
			s.save_snap(n, img);
		}
		if let (Some(s), Some(img)) = (self.shots.as_mut().filter(|_| shot), img) {
			s.save(done, k, sub, &img);
			self.log_shot(&scene, cam.as_ref());
		}
		if self.shots.is_some() {
			self.sub_at.1 = (k + 1).min(sub);
			if self.sub_at.1 == sub {
				self.shared.drawn.set(done);
			}
		} else if !rac_engine::viewer::test_clock::unpaced() {
			let g = self.gpu.as_ref().unwrap();
			self.pace.wait(&g.window, g.config.present_mode == wgpu::PresentMode::Fifo, false);
		}
		super::framelog::stage(4);
		self.gpu.as_ref().unwrap().window.request_redraw();
	}

	/// The log lines of a saved window shot: what the scene held and what was drawn.
	fn log_shot(&self, scene: &Scene, cam: Option<&Camera>) {
		let (n, drawn, left) = &self.stage.drawn;
		if self.stage.cut.0 > 0 {
			eprintln!("  cull: left-out mobies read {}, cast {}", self.stage.cut.0, self.stage.cut.1);
		}
		match cam {
			Some(c) => eprintln!("  scene: level {} eye {:?} aspect {:.4} overlay {} mobies {n} drawn {drawn} left out {left:?}", scene.level, c.pos, self.aspect, self.ov_overlay),
			None => eprintln!("  scene: level {} no world (view {}, engine level {:?}) overlay {}", scene.level, scene.view.is_some(), self.stage.level, self.ov_overlay),
		}
	}
}

/// [b45 P7] Water has its own gate: with Fire, Smoke and the weapon lights off the engine still gets the hero and the movers
/// for the water rings and wakes (fx_movers.rs).
/// [b51 G5] The draw, plus the RAC_DIAG mark key's picture (Insert). When a test shot, a live capture or a snapshot takes this
/// frame's picture (`want`), the mark's row is written but its PNG is skipped.
fn draw_marked(g: &mut Gpu, cam: &Camera, want: bool) -> Option<rac_engine::render::Image> {
	let mark = rac_engine::diag::shot_due();
	let img = g.draw(cam, want || mark);
	if mark && !want {
		rac_engine::diag::shot(img);
		return None;
	}
	img
}

fn water_only(g: &mut Gpu, cam: Option<&Camera>) {
	if let Some(c) = cam.filter(|_| !(super::ov_layers::fx_enabled() && super::ov_fire::gate())) {
		crate::fx_movers::water_only(g, c);
	}
}

/// Window shots: the fields to save (counted from 0 as `--shots` does: `field N` = the picture after N + 1 fields ran).
pub(super) struct Shots {
	dir: String,
	every: u64,
	at: Vec<u64>,
	last: u64,
	/// The newest picture saved (field, frame in it) and its hash.
	saved: Option<((u64, u32), u64)>,
	t0: std::time::Instant,
}

impl Shots {
	pub(super) fn new(dir: String, o: &Opts, fields: u64) -> Shots {
		Shots { dir, every: o.every, at: o.at.clone(), last: fields.saturating_sub(1), saved: None, t0: std::time::Instant::now() }
	}

	/// Frame `k` of the picture of `done` fields is one to save.
	fn want(&self, done: u64, k: u32) -> bool {
		let Some(f) = done.checked_sub(1) else { return false };
		self.saved.is_none_or(|s| s.0 != (f, k)) && ((self.every > 0 && f % self.every == 0) || self.at.contains(&f) || f == self.last)
	}

	/// DIR/wNNNNN.png, or with RAC_TEST_SUB=N (`sub`) every frame of the field: DIR/wNNNNN_K.png.
	fn save(&mut self, done: u64, k: u32, sub: u32, img: &rac_engine::render::Image) {
		let f = done - 1;
		let h = rac_engine::gsfull::pipe::hash(&img.rgb);
		let path = if sub > 1 { format!("{}/w{f:05}_{k}.png", self.dir) } else { format!("{}/w{f:05}.png", self.dir) };
		if let Err(e) = std::fs::write(&path, rac_engine::render::png(img)) {
			eprintln!("{path}: {e}");
		}
		let same = if self.saved.is_some_and(|s| s.1 == h) { " (same)" } else { "" };
		let part = if sub > 1 { format!(" frame {k}") } else { String::new() };
		eprintln!("field {f}{part}: {}x{} hash {h:016x}{same} ({:.1}s)", img.w, img.h, self.t0.elapsed().as_secs_f64());
		self.saved = Some(((f, k), h));
	}

	/// DIR/fcNNN.png: a picture the free camera's test script asked for.
	fn save_snap(&self, n: u32, img: &rac_engine::render::Image) {
		let path = format!("{}/fc{n:03}.png", self.dir);
		if let Err(e) = std::fs::write(&path, rac_engine::render::png(img)) {
			eprintln!("{path}: {e}");
		}
		eprintln!("snap {n}: {}x{} hash {:016x}", img.w, img.h, rac_engine::gsfull::pipe::hash(&img.rgb));
	}

	/// Frame `n` of the run's movie `i` (RAC_FMV=1, host/movie.rs): DIR/mI_NNNNN.png.
	pub(super) fn save_movie(&mut self, i: u32, n: u64, img: &rac_engine::render::Image) {
		let path = format!("{}/m{i}_{n:05}.png", self.dir);
		if let Err(e) = std::fs::write(&path, rac_engine::render::png(img)) {
			eprintln!("{path}: {e}");
		}
		eprintln!("movie {i} frame {n}: {}x{} hash {:016x}", img.w, img.h, rac_engine::gsfull::pipe::hash(&img.rgb));
	}
}
