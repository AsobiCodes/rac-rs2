//! [PC] R&C2's PC options (engine split stage E6): `<player dir>/pc_options.txt` (engine/pcopts.rs; the launcher's Settings page
//! writes it, R&C2's own menus have no PC rows yet). LIVE since parity wave 1 (row 52): the window polls the file twice a second
//! (`poll`) and a change is in force from the next frame, like R&C1's rows; `change` is the writer a menu uses (wave 2).
//! The Graphics verdict keeps the rows whose engine code needs nothing more from the game: Lighting (Enhanced: glow, contrast,
//! sharpen, smooth texture detail unless texfilter=classic), ambient occlusion, global illumination, draw distance, frame
//! smoothing, dynamic (sun) shadows (on with Enhanced, wave 2B), volumetric fog (on with Enhanced, wave 3); sun rays from the key light (High with Enhanced). The
//! rows that need R&C2 facts or research stay Classic; Vegetation (veg.rs) is read, volumetric clouds only from the file's `clouds=`
//! key and the sea waves on unless `waves=off` (rac2/planet_sky.rs, parity rows 14 / 19); Culling (BATCH A CULL: Off also draws the mobies the
//! game left out, rac2/moby_cull.rs) is read. Also read
//! here: the HUD row (`hud=sharp|original`), Display mode, the renderer (read by the engine itself), the key / button remaps
//! (`keys`, `pad`), FPS limit (`fps=`), Render scale (`renderscale=`), Aspect ratio (`aspect=`), Debug mode (`debug=on`: the
//! split view's keys F10 / F11 / F12), the boot logo skip (`intro=skip`) and the language (`lang=`).

use rac_engine::gfx::Graphics;
use rac_engine::pcopts::{self, Lang, PcOptions};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant, SystemTime};

/// The options in force and the file stamp (modified time, length) they were read from.
struct Live {
	opts: Arc<PcOptions>,
	stamp: Option<(SystemTime, u64)>,
	path: PathBuf,
}

static LIVE: RwLock<Option<Live>> = RwLock::new(None);
/// Serialises loads and writes (the first `get` of two threads, `poll` against `change`).
static LOAD: Mutex<()> = Mutex::new(());
/// When `poll` last looked at the file.
static POLLED: Mutex<Option<Instant>> = Mutex::new(None);
const POLL_EVERY: Duration = Duration::from_millis(500);

fn stamp_of(path: &Path) -> Option<(SystemTime, u64)> {
	let m = std::fs::metadata(path).ok()?;
	Some((m.modified().ok()?, m.len()))
}

/// The player's options in force (read on first use).
pub fn get() -> Arc<PcOptions> {
	if let Some(l) = LIVE.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
		return l.opts.clone();
	}
	let _g = LOAD.lock().unwrap_or_else(|e| e.into_inner());
	if let Some(l) = LIVE.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
		return l.opts.clone();
	}
	install(pcopts::file(), None)
}

/// Reads `path` and makes it the options in force (`old` = the ones it replaces, None at the start); LOAD held.
fn install(path: PathBuf, old: Option<&PcOptions>) -> Arc<PcOptions> {
	let stamp = stamp_of(&path);
	let o = Arc::new(PcOptions::load(&path));
	apply(&o, old);
	*LIVE.write().unwrap_or_else(|e| e.into_inner()) = Some(Live { opts: o.clone(), stamp, path });
	o
}

/// The engine statics the options drive (at the start and after every change).
fn apply(o: &PcOptions, old: Option<&PcOptions>) {
	// Smooth texture detail (engine/texfilter.rs): with Lighting Enhanced unless texfilter=classic.
	rac_engine::texfilter::set(o.texfilter.unwrap_or(true));
	// Key and button remaps (engine/input/binds.rs; R&C1 writes them from its Controls page, R&C2 has no Controls rows yet): at the
	// start and when a bind key changed only, since kbm::init reloads the whole table (unsaved menu edits too); R&C1 calls it once.
	if binds_changed(old, o) {
		rac_engine::input::binds::init_kbm(o.extra_get("kbm"), o.extra_get("keys"), o.extra_get("pad"), o.extra_get("gamepad"));
	}
	// Aspect ratio (engine viewer/aspect_box.rs): Auto = the window as before; a shape = the whole picture in a box of it, the
	// game's own 4:3 / 16:9 picture centred inside (host/frame.rs `picture_rect` reads the Gpu's size, which is the box).
	rac_engine::viewer::aspect_box::init(o);
	// Metal and Metal reflections (engine metal_option; rac2/modern_rows_metal.rs): live from the file.
	crate::modern_rows_metal::init(o);
	// Fire and Smoke (engine fire_option / smoke_option; rac2/modern_rows_fire.rs, host/ov_fire.rs): live from the file.
	crate::modern_rows_fire::init(o);
	// Underwater look (launcher key `underwater`, no menu row, as R&C1): rac2/under.rs feeds the camera flag.
	rac_engine::viewer::under_choice::init(o);
	// [b45 P2] Brightness / Contrast follow the file (engine picture_adjust); Resolution and Frame Generation are read once at the start
	// (fgen_choice::init resets "started": the menu sets its own choice; resolution::init resets the pending size).
	let pic = |p: &PcOptions| (p.extra_get("brightness").map(str::to_owned), p.extra_get("contrast").map(str::to_owned));
	if old.is_none_or(|p| pic(p) != pic(o)) {
		rac_engine::viewer::picture_adjust::init(o);
	}
	if old.is_none() {
		rac_engine::viewer::fgen_choice::init(o);
		rac_engine::viewer::resolution::init(o);
	}
	// Upscaler and Upscale quality (engine viewer/upscale/choice.rs; modern_rows_upscale.rs): from the file at the start and when the
	// file's keys changed outside the menu (the menu sets the engine's choice itself).
	let up = |p: &PcOptions| (p.extra_get("upscaler").map(str::to_owned), p.extra_get("upscale_q").map(str::to_owned));
	if old.is_none_or(|p| up(p) != up(o)) {
		rac_engine::viewer::upscale_choice::init(o);
	}
	// Debug mode switched On by the player: the keys notice (pc_devtools.rs). Off: every tool and cheat goes (split view, Hide HUD,
	// planet list, free camera, draw distance A/B, god, ammo; R&C1 devtools.rs set_menu / debugkeys.rs menu_off).
	if old.is_some_and(|p| !p.debug_on()) && o.debug_on() {
		crate::pc_devtools::announce();
	}
	if old.is_some_and(|p| p.debug_on()) && !o.debug_on() {
		rac_engine::splitview::menu_off();
		crate::pc_devtools::menu_off();
		crate::pc_debug::menu_off();
		crate::pc_freecam::menu_off();
	}
}

/// The pc_options keys `kbm::init` reads.
const BIND_KEYS: [&str; 4] = ["kbm", "keys", "pad", "gamepad"];

/// Must the key / button binds be (re)loaded: the first apply (`old` None) or one of `BIND_KEYS` changed.
fn binds_changed(old: Option<&PcOptions>, o: &PcOptions) -> bool {
	old.is_none_or(|p| BIND_KEYS.iter().any(|k| p.extra_get(k) != o.extra_get(k)))
}

/// Window thread, every redraw: the file is looked at twice a second; a change (the launcher, a hand edit, wave 2's menu in another
/// process) is read and in force from the next frame. True = re-read.
pub fn poll() -> bool {
	{
		let mut p = POLLED.lock().unwrap_or_else(|e| e.into_inner());
		if p.is_some_and(|t| t.elapsed() < POLL_EVERY) {
			return false;
		}
		*p = Some(Instant::now());
	}
	reread_if_changed()
}

fn reread_if_changed() -> bool {
	let _g = LOAD.lock().unwrap_or_else(|e| e.into_inner());
	let (path, stamp, old) = match LIVE.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
		Some(l) => (l.path.clone(), l.stamp, l.opts.clone()),
		None => return false,
	};
	if stamp_of(&path) == stamp {
		return false;
	}
	let o = install(path, Some(&old));
	if *o != *old {
		eprintln!("rac2: PC options re-read (pc_options.txt changed)");
	}
	true
}

/// [b45 P2] The crash filter's writer of `renderer=dx12` (engine rtss_crash.rs `Dx12Writer`; R&C1: renderer_switch::set_dx12): goes through
/// `change`, so the cached copy takes it too and a later save cannot write Vulkan back. `wait` false: the lock is tried for 200 ms at
/// most (the crash filter runs on the crashed thread, which may hold it itself); None = busy, the caller writes the file directly.
pub fn set_dx12(wait: bool) -> Option<std::io::Result<()>> {
	if !wait {
		let free = (0..20).any(|n| {
			let ok = LOAD.try_lock().is_ok() || LOAD.is_poisoned();
			if !ok && n < 19 {
				std::thread::sleep(std::time::Duration::from_millis(10));
			}
			ok
		});
		if !free {
			return None;
		}
	}
	Some(change(|o| o.renderer = Some(true)))
}

/// A writer (wave 2's menu, the language watch): `f` changes a copy, which is saved and in force at once.
pub fn change(f: impl FnOnce(&mut PcOptions)) -> std::io::Result<()> {
	let cur = get();
	let _g = LOAD.lock().unwrap_or_else(|e| e.into_inner());
	let mut o = (*cur).clone();
	f(&mut o);
	let path = LIVE.read().unwrap_or_else(|e| e.into_inner()).as_ref().map_or_else(pcopts::file, |l| l.path.clone());
	let saved = o.store(&path);
	let o = Arc::new(o);
	apply(&o, Some(&cur));
	*LIVE.write().unwrap_or_else(|e| e.into_inner()) = Some(Live { opts: o, stamp: stamp_of(&path), path });
	saved
}

/// [PC] pc_options `hud=sharp|original` (default sharp; `RAC_HUD=original` wins over the file): the HUD and menus over the world are
/// drawn a second time at the window's resolution by the GPU capture (engine gscap/rc2.rs); original = the 512-wide picture of the game.
pub fn hud_sharp() -> bool {
	static ENV_ORIGINAL: OnceLock<bool> = OnceLock::new();
	get().hud_sharp() && !*ENV_ORIGINAL.get_or_init(|| std::env::var("RAC_HUD").is_ok_and(|v| v.eq_ignore_ascii_case("original")))
}

/// The pack's Graphics verdict (`Pack::graphics`, every frame).
pub fn graphics() -> Graphics {
	verdict(&get())
}

pub(crate) fn verdict(o: &PcOptions) -> Graphics {
	let r = o.graphics_rows();
	// Sun rays (row 12, lane gfxd): from the level's key light (Looks::rays_from_key); High with Lighting Enhanced as R&C1's row. Volumetric fog
	// (wave 3 row 1, wave 4.1): High with Lighting Enhanced as R&C1's row; the volume keeps the retail fog's shape (rac2/looks.rs
	// `retail_fog_shape`: in wave 3 it painted R&C2's sky in the retail fog colour) and space skies get no air (rac2/planet_fog.rs). Sun shadows
	// (wave 2A/2B): only the mobys on retail's shadow caster list cast (rac2/looks.rs; the painted world does not follow R&C2's
	// key light), so they are on (High) with Lighting Enhanced unless the file says otherwise.
	let shadows_q = o.shadows.map_or(if r[0] != 0 { 3 } else { 0 }, |v| v.min(3));
	let sun_q = o.sun_rays.map_or(if r[0] != 0 { 3 } else { 0 }, |v| v.min(3));
	// Water Enhanced (wave 3 row 2): R&C2's water batches are the engine's fogged water quads (rac2/water.rs), which the Enhanced
	// water pass takes (viewer/gpu/draw/pass_water.rs `pass_world_quads`, "fogged quads"); High with Lighting Enhanced as R&C1's row
	// (par-w3b: reflections on Oozla and Dobbo, A-B-A flicker 0 live), explicit Classic kept.
	let water_q = if r[0] != 0 { o.water.map_or(3, |v| v.min(3)) } else { 0 };
	// Particles Enhanced (wave 4.2 row 16): R&C2's effect-program prims are the engine's layer runs (rac2/host/ov_layers.rs), which
	// the soft particle pass, the glow and the effect lights take (engine gpu/layer_fx.rs); High with Lighting Enhanced as R&C1's row.
	let particles_q = if r[0] != 0 { o.particles.map_or(3, |v| v.min(3)) } else { 0 };
	Graphics {
		lighting: r[0],
		ao: r[1] != 0,
		ao_q: r[1],
		shadows: shadows_q != 0,
		shadows_q,
		sun_rays: sun_q != 0,
		sun_q,
		smoothing: r[7] != 0,
		gi_q: r[10],
		draw: pcopts::draw_level(r[12]),
		fog_q: o.fog.map_or(if r[0] != 0 { 3 } else { 0 }, |v| v.min(3)),
		water: water_q != 0,
		water_q,
		particles: particles_q != 0,
		particles_q,
		// [PC] Culling (rac2/moby_cull.rs, R&C1's row): On (the default) = the game's own moby verdict.
		culling: o.culling.unwrap_or(true),
		// [PC] Vegetation (veg.rs, R&C1's row: Moderate by default, drawn only with Lighting Enhanced).
		veg: r[8].min(2),
		// [PC] Fur (fur.rs, R&C1's rows: High by default, drawn only with Lighting Enhanced; style Soft).
		fur_q: r[15].min(3),
		fur_long: r[16] != 0,
		// [PC] Waves (R&C1's launcher key `waves`, no menu row as in R&C1): the geometric sea waves of Water Medium / High, On by
		// default. R&C2's big water quads are sea draws of the water pass (waterfx/draws.rs `sea`: Oozla, Dobbo).
		waves: o.waves.unwrap_or(true),
		// [PC] Volumetric clouds: only from the file's `clouds=` key (taste per planet, rac2/planet_sky.rs), Classic by default.
		clouds_q: o.clouds.map_or(0, |v| v.min(3)),
		..Graphics::CLASSIC
	}
}

/// [PC] Debug mode (`Pack::debug_mode`): `debug=on` in the file, or RAC_DEV_TOOLS=1 / RAC_DEBUG_KEYS=1 (as R&C1's). On: the split
/// view's keys (F10 split, F11 hide HUD, F12 labels, [ ] Home \ the divider; engine/splitview.rs).
pub fn debug_mode() -> bool {
	static ENV: OnceLock<bool> = OnceLock::new();
	*ENV.get_or_init(|| ["RAC_DEV_TOOLS", "RAC_DEBUG_KEYS"].iter().any(|k| std::env::var(k).is_ok_and(|v| v == "1"))) || get().debug_on()
}

/// [PC] FPS limit (`Pack::fps_setting`): `fps=30|60|120|144|off` (no key = the display's refresh rate).
pub fn fps_setting() -> u32 {
	get().fps_limit()
}

/// [PC] Render scale (`Pack::render_scale_pct`): `renderscale=50|75|100|125|150|200` (no key = 100).
pub fn render_scale_pct() -> u32 {
	get().render_scale()
}

/// [PC] Pack `reset_sizes` (engine gpu_errors.rs: lasting wgpu errors): Render scale 100 % and Aspect ratio Auto, saved and in force
/// from the next frame. Always possible (the file is read on first use).
pub fn reset_sizes() -> bool {
	let r = change(|o| {
		o.extra_set("renderscale", None);
		rac_engine::viewer::aspect_box::set_from_ui(o, 0);
	});
	if let Err(e) = r {
		eprintln!("rac2: PC options: cannot save the size reset: {e}");
	}
	true
}

/// [PC] `intro=skip`: the boot logo movies end at once (movie.rs; missing = play, retail's).
pub fn intro_skip() -> bool {
	get().intro_skip()
}

// ---- language (row 57) --------------------------------------------------------------------------------------------------------

/// The game's language word [C rac2/movie.rs: the PSS audio track the movie player gets is this word: 0 English, 2 French,
/// 3 German, 4 Spanish, 5 Italian; the same numbering as R&C1's word, `Lang::word`].
#[allow(dead_code)]
pub const LANG_WORD: u32 = 0x1a7c3c;

/// Before `Machine::cold`: the cold boot reports the remembered language (`lang=`) as the console's (`ee_oracle::cold::set_language`),
/// unless COLD_LANG forces one (tests and tools). None = the console default (English).
pub fn cold_language() {
	if let Some((l, from)) = pcopts::cold_lang(std::env::var_os("COLD_LANG").is_none(), get().lang) {
		ee_oracle::cold::set_language(l.osd());
		eprintln!("rac2: PC options: cold boot in the {from} language ({})", l.code());
	}
}

/// The word as last seen by `watch_language` (u32::MAX: nothing seen yet).
static SEEN_WORD: Mutex<u32> = Mutex::new(u32::MAX);

/// The first field the game thread watches the language at (R&C1's `watch_language` waits 4 too). The boot's one write of the word
/// (FUN_002914e0, from the console language the cold boot reports: `cold_language`) is the console's choice, never a player's: a
/// remembered or Setup language makes that write equal the file (no write), and an unset one is English = the word's 0.
pub const LANG_WATCH_FROM: u64 = 4;

/// Game thread, every field from `LANG_WATCH_FROM` (host/game.rs): the game's language byte (`addrs::LANG`). A change after the
/// first look (the player picked a language on the title's Options page, FUN_002d6730 case 9) is remembered in the file as
/// `lang=`, so the next cold boot starts in it (parity row 57).
pub fn watch_language(word: u32, field: u64) {
	let prev = std::mem::replace(&mut *SEEN_WORD.lock().unwrap_or_else(|e| e.into_inner()), word);
	if prev == u32::MAX {
		eprintln!("rac2: language word {word} at field {field} (first look)");
	}
	if let Some(l) = remembered_change(prev, word, get().lang) {
		eprintln!("rac2: PC options: language {} remembered", l.code());
		if let Err(e) = change(|o| o.lang = Some(l)) {
			eprintln!("rac2: PC options: cannot write the language: {e}");
		}
	}
}

/// The language to write when the word went `prev` -> `now` (`file` = what the file holds): only a real change of a valid word.
fn remembered_change(prev: u32, now: u32, file: Option<Lang>) -> Option<Lang> {
	if prev == u32::MAX || prev == now {
		return None;
	}
	Lang::from_word(now).filter(|l| file != Some(*l))
}

#[cfg(test)]
#[path = "options_tests.rs"]
mod tests;
