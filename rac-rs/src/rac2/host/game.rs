//! The R&C2 window's game thread (host.rs): boots the disc and runs one field per 20 ms (lockstep: one per window redraw), the
//! pad before each from a replay, a scripted pad or the window; the field's sound to the engine's SPU2 mixer, its graphics
//! jobs to the GS threads; after each field the game's camera and moby list for the window and a new level number to the
//! world loader (lane rc2's window.rs `game`, `NativeFeed`).

use super::Shared;
use crate::boot;
use crate::scene::{Resolver, Scene};
use crate::shots::Opts;
use rac_engine::gsfull::crtc::Crtc;
use rac_engine::gsfull::ovl_pipe::FieldIn;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Sender, SyncSender};
use std::time::{Duration, Instant};

/// One field of the game's video mode: 20 ms at 50 Hz, 1001/60000 s in its 60 Hz mode (engine/field.rs follows the oracle's
/// SetGsCrt, R&C1's pace too).
fn field_time() -> Duration {
	Duration::from_secs_f32(rac_engine::field::field_dt())
}

/// What the game thread runs.
pub(super) struct Run {
	pub iso: String,
	pub o: Opts,
	/// A replay's pad per field (empty: none).
	pub rec_pad: Vec<(u16, [u8; 4])>,
	/// Fields to run (0 = until the window closes).
	pub fields: u64,
	pub lockstep: bool,
	/// The savestate dir the game starts from (states.rs), and the GS pipe (a quick save asks it for its share).
	pub state: Option<String>,
	pub gs: Arc<rac_engine::gsfull::ovl_pipe::GsPipe>,
}

impl Run {
	/// The pad of a field: the replay's, else the scripted one (`--pad` / `--stick`), else the window's.
	fn pad(&self, shared: &Shared, field: u64) -> (u16, [u8; 4]) {
		// [b48 Y5] A field-locked feed run (pc_devfeed.rs `f` entries): the window answers this field before the pad is read.
		if !self.lockstep {
			crate::pc_devfeed::gate(field, || shared.quit.load(Ordering::Relaxed));
		}
		if let Some(&p) = self.rec_pad.get(field as usize) {
			// [PC] Test: `--pad` buttons are added to a replay's (a pause press over a recorded scene).
			return (self.o.pad.iter().filter(|q| (q.0..=q.1).contains(&field)).fold(p.0, |b, q| b | q.2), p.1);
		}
		if !self.o.pad.is_empty() || !self.o.stick.is_empty() {
			let b = self.o.pad.iter().filter(|p| (p.0..=p.1).contains(&field)).fold(0u16, |b, p| b | p.2);
			let sticked = self.o.stick.iter().find(|p| (p.0..=p.1).contains(&field));
			let l = sticked.map_or(0x8080, |p| p.2);
			let scripted = (b, [0x80, 0x80, (l >> 8) as u8, l as u8]);
			// [b50 V6] Dev feed: a locked feed (`f` entries) next to `--pad`: the window's pad (the feed's keys / stick) is added. Without
			// `f` entries `locked()` is false and this is the old return, bit for bit.
			if crate::pc_devfeed::locked() {
				let p = shared.pad.load(Ordering::Relaxed);
				return crate::pc_devfeed::merge_pad(scripted, sticked.is_some(), ((p >> 32) as u16, (p as u32).to_le_bytes()));
			}
			return scripted;
		}
		let p = shared.pad.load(Ordering::Relaxed);
		((p >> 32) as u16, (p as u32).to_le_bytes())
	}
}

/// The session recording (boot.rs `start_recording`): the pad of every field.
fn record(rec: &mut Option<std::io::BufWriter<std::fs::File>>, field: u64, b: u16, s: [u8; 4]) {
	use std::io::Write;
	if let Some(w) = rec.as_mut() {
		let _ = w.write_all(&[b as u8, (b >> 8) as u8, s[0], s[1], s[2], s[3]]);
		if field % 250 == 0 {
			let _ = w.flush();
			crate::pc_rec::flush();
		}
	}
}

/// [PC] The pad the game gets this field and the PC side before it: the modern pages' filters, the mouse's cursor writes, the mouse
/// in retail menus, the free camera. A replay with pc.bin (pc_rec.rs): the recorded pad is final (it was made by these); the filters
/// and `ctl` run for their own state only (what the window draws, the prompts' menu keys) and the cursor writes come from the file.
fn field_pad(ram: &mut [u8], ctl: &mut crate::pc_input::Ctl, field: u64, live: bool, b: u16, s: [u8; 4]) -> (u16, [u8; 4]) {
	crate::modcam_ui::set_window_input(live); // [b45 review F3] a replay / scripted pad: Classic first person takes no window mouse
	if crate::pc_rec::replaying() {
		let _ = modern_filters(ram, b, s);
		crate::modern_jump::apply(ram);
		let _ = ctl.field(ram, field, live, b, s);
		return (b, s);
	}
	let (b, s) = modern_filters(ram, b, s);
	crate::modern_jump::apply(ram); // [PC] nav lane: the mouse's cursor writes, only while a request is pending (modern_jump.rs)
	// [PC] The mouse in retail menus, the Quick Select wheel and ship fights (pc_input.rs); pad bits / stick bytes only.
	let (b, s) = ctl.field(ram, field, live, b, s);
	// [PC] A neutral pad while the free camera flies (pc_freecam.rs).
	let (b, s) = crate::pc_freecam::filter(b, s);
	crate::pc_devtools::filter(b, s) // [PC] neutral while Debug mode's planet list is open
}

pub(super) fn game(r: Run, tx: SyncSender<FieldIn>, shared: Arc<Shared>, level_tx: Sender<crate::chunks::Req>) {
	let booted = match &r.state {
		Some(d) => boot::boot_state(&r.iso, d),
		None => boot::boot(&r.iso),
	};
	let (mut m, rx) = match booted {
		Ok(x) => x,
		Err(e) => {
			eprintln!("rac2-play: boot: {e}");
			shared.over.store(true, Ordering::Relaxed);
			return;
		}
	};
	// The retail sound driver's SPU2 writes of each field play on the SPU2 voice mixer (spumix.rs). No device, RAC2_NOSOUND=1
	// or a lockstep run: the game runs silent.
	let mut audio = if r.lockstep || std::env::var_os("RAC2_NOSOUND").is_some() { None } else { rac_engine::audio::Audio::new() };
	if audio.is_some() {
		m.record_sound();
	}
	// Lockstep: Aranos's world (the loader's first job) is in before the first field.
	if r.lockstep && !shared.loaded.wait(|l| l >= 1, &shared.quit) {
		return;
	}
	let (mut next, t0) = (Instant::now(), Instant::now());
	let log = std::env::var_os("RAC2_LOG").is_some();
	let ramhash = std::env::var_os("RAC2_RAMHASH").is_some();
	let mut feed = Feed { level_sent: 0, asked: None, sent: 1, res: Resolver::default(), tx: level_tx };
	let recording = r.rec_pad.is_empty() && (!r.lockstep || crate::pc_rec::rec_lockstep());
	let mut rec = boot::RECDIR.get().filter(|_| recording).and_then(|d| std::fs::File::create(d.join("pad.bin")).ok()).map(std::io::BufWriter::new);
	// The Modern camera's input of each field next to it (modcam_rec.rs): a replay of a mouse-look session stays in sync.
	let mut rec_cam = boot::RECDIR.get().filter(|_| recording).and_then(|d| std::fs::File::create(d.join("cam.bin")).ok()).map(std::io::BufWriter::new);
	boot::RECDIR.get().filter(|_| recording).map(|d| crate::pc_rec::start(d)); // [PC] the PC actions (pc_rec.rs)
	let (mut field, mut qsave) = (0u64, crate::states::SaveGate::default());
	let mut unwind_wait = 0u32; // fields a due unwind has waited for a vsync wait (unwind.rs)
	let (mut ctl, live) = (crate::pc_input::Ctl::default(), r.rec_pad.is_empty() && ((r.o.pad.is_empty() && r.o.stick.is_empty()) || crate::pc_devfeed::locked())); // [b50 V6] a locked feed next to --pad is a live (feed-driven) run
	while !shared.quit.load(Ordering::Relaxed) && (r.fields == 0 || field < r.fields) {
		// [PC] The free camera freezes the game on request (pc_freecam.rs; nothing without Debug mode).
		crate::pc_freecam::hold(&shared.quit);
		let t_loop0 = Instant::now();
		crate::pc_rec::begin_field(field);
		let (b, s) = r.pad(&shared, field);
		let (b, s) = field_pad(&mut m.cpu.mem.ram, &mut ctl, field, live, b, s);
		// The Modern camera's input of this field (modcam_ui.rs; the camera hand reads it back with `current()`); its aim, the
		// right mouse button / the feed, is R2. With the Classic camera and a replay `b` stays exactly what the replay holds.
		crate::modcam_ui::note_stick([s[0], s[1]]);
		let cam = crate::modcam_ui::take_ui(field as u32, b);
		crate::modcam_fp::pre_field(&mut m.cpu.mem, cam.look); // [b45 P5] first person: the mouse turns the view (modcam_fp.rs)
		let b = crate::modcam_ui::with_aim(b, &cam);
		// Retail's camera reads the stick with retail's sign: the controller's share is flipped to the mouse's while it runs
		// (modcam_sticks.rs). The hand got the raw bytes above; the recording keeps the raw bytes too (a replay flips the same way).
		m.set_pad(b, crate::modcam_sticks::game_sticks(&m.cpu.mem.ram, s, cam.keys));
		record(&mut rec, field, b, s);
		crate::modcam_rec::record(&mut rec_cam, field, &cam);
		if let Some(n) = boot::level_override().filter(|_| r.state.is_none()) {
			boot::apply_level_override(&mut m, n);
		}
		if boot::wide_override() {
			boot::apply_wide(&mut m);
		}
		// A movie plays in the window: no field until it ended (rac2/movie.rs `wait_idle`).
		if !crate::movie::wait_idle(&shared.quit) {
			break;
		}
		crate::ramdump::set_field(field);
		crate::pc_debug::pre_field(&mut m, field);
		crate::pc_prompts::begin_field(&mut m.cpu.mem, field as u32, live); // [PC] the prompts' device of this field (pc_prompts.rs)
		crate::modern_vendor_hand::rec_sync(); // [PC] Buy all pending: to pc.bin, or as recorded in a replay (pc_rec.rs)
		let tf = Instant::now();
		if let Err(e) = m.frame() {
			eprintln!("rac2-play: field {field}: {e} (pc {:08x})", m.cpu.pc);
			crate::ramdump::hang_report(&m);
			break;
		}
		crate::scan_timing::field(field as u64, tf.elapsed(), m.cpu.icount);
		field += 1;
		// [PC] Language remembered (parity row 57, R&C1 pcext `watch_language`): a change of the game's word after the boot's own
		// write is saved as `lang=` (options.rs).
		if field > crate::options::LANG_WATCH_FROM {
			crate::options::watch_language(m.rd8(crate::addrs::LANG) as u32, field);
			// [PC] The Subtitles row and the game's Subtitles byte, both ways (subs_sync.rs).
			if live {
				crate::subs_sync::watch(&mut m);
			}
			crate::pc_rec::apply_mem(crate::pc_rec::Stage::After, &mut m.cpu.mem); // a replay: the recorded Subtitles byte (pc_rec.rs)
		}
		// The frame buffers of the video mode the game runs in, for the screen-read test of the RAM classes (props.rs `reads_screen`).
		crate::props::note_shown(Crtc::from_regs(&m.cpu.mem.gs).shown());
		// [b45 P1] Only while the game thread is parked in a vsync wait (no PC hook inside a `Cpu::call`), forced after 500 fields: R&C1's rule (unwind.rs).
		let parked = crate::unwind::parked(m.cpu.pc, m.cpu.vblank_pc, ee_oracle::ver::addrs().poll_head);
		let (due, now) = crate::unwind::step_logged(unwind_wait, field as u32, parked, m.cpu.pc);
		unwind_wait = due;
		if now {
			// The flag unwinds its compiled frames there (RAM unchanged).
			m.cpu.rec_unwind = true;
		}
		crate::ramdump::after_field(&m, field);
		// F5 / RAC2_QSAVE_AT: a quick save of this field (states.rs); the GS pipe is asked before the field is sent.
		if qsave.poll(shared.save_req.swap(false, Ordering::Relaxed) || crate::states::save_at() == Some(field), m.cpu.mc.busy(), field) {
			crate::states::save(&m, field, &r.gs);
		}
		if ramhash && field % 250 == 0 {
			eprintln!("ramhash field {field} {:016x}", boot::ram_hash(&m));
		}
		if let Some(a) = audio.as_mut() {
			let b = crate::modern_tabs::hushed(rac_engine::spumix::drain(&mut m)); // [PC] nav lane: one sound per tab click (modern_tabs.rs)
			if !a.spu_on() {
				a.spu_attach(0, None, true);
			}
			a.spu_push(b.ev, Some(&b.core));
			// [PC] The modern Options page's ticks (engine ui/sound.rs; host/menu.rs).
			rac_engine::ui::sound::drain(a);
		}
		if log && field % 250 == 0 {
			eprintln!("game: field {field} at {:.1}s, last field {:.1} ms, rec {} fns", t0.elapsed().as_secs_f64(), tf.elapsed().as_secs_f64() * 1e3, m.cpu.rec.as_ref().map_or(0, |t| t.len()));
		}
		let t_frame = tf.elapsed();
		let overlay = feed.field(&m, &shared, r.lockstep, field);
		let t_send0 = Instant::now();
		if tx.send(FieldIn { field, jobs: rx.try_iter().collect(), crtc: Crtc::from_regs(&m.cpu.mem.gs), overlay, grab: (shared.native.load(Ordering::Relaxed) && !crate::movie::playing()).then_some(crate::addrs::GRAB), space: crate::space_flight::in_flight(&m.cpu.mem.ram) }).is_err() { // [b46 U4]
			break;
		}
		let t_send = t_send0.elapsed();
		// [b52 H3] A lockstep run writes its `<csv>.game` row and the game_ columns too: the wait for the redraw is the "sleep".
		let tw = Instant::now();
		if r.lockstep && !shared.drawn.wait(|d| d >= field, &shared.quit) {
			break;
		}
		let slept = if r.lockstep { tw.elapsed().as_secs_f32() * 1000.0 } else { pace(&mut next) };
		let total = t_loop0.elapsed().as_secs_f32() * 1000.0;
		let (fr, sn) = (t_frame.as_secs_f32() * 1000.0, t_send.as_secs_f32() * 1000.0);
		super::framelog::game_row(field, t0.elapsed().as_secs_f32() * 1000.0, fr, total - fr - sn - slept, sn, slept);
	}
	crate::pc_rec::flush();
	if r.lockstep {
		eprintln!("done {field} fields in {:.1} s", t0.elapsed().as_secs_f64());
		rac_engine::texpack::discover::finish(); // [PC] DEV RAC_TEXNAMES final write + the texture pack's HUD counters (test runs only)
	}
	shared.over.store(true, Ordering::Relaxed);
}

/// The live game thread's wait for the next field time; returns the ms slept (0 when the field ran late).
fn pace(next: &mut Instant) -> f32 {
	let dt = field_time();
	*next += dt;
	let now = Instant::now();
	if *next > now {
		std::thread::sleep(*next - now);
		return (*next - now).as_secs_f32() * 1000.0;
	}
	if now - *next > dt * 5 {
		// Too far behind (a slow field): run on from now instead of catching up.
		*next = now;
	}
	0.0
}

/// [PC] The modern pages' pad filters, in order (each returns the pad bit for bit while its page is not up).
fn modern_filters(ram: &[u8], b: u16, s: [u8; 4]) -> (u16, [u8; 4]) {
	// [PC] The modern Options page (modern_open.rs): its door on the Camera row, and a neutral pad for the game while it is up.
	let (b, s) = crate::modern_open::filter(ram, b, s);
	// [PC] The modern pause and title roots (modern_screen.rs): pulses for the mouse, Quit to Desktop on the title.
	let (b, s) = crate::modern_screen::filter(ram, b, s);
	let (b, s) = crate::modern_load::filter(ram, b, s); // [PC] ld lane: the modern Load / Save Game pages' mouse pulses
	// [PC] The modern vendor (modern_vendor.rs): the list's Up / Down and the mouse's pulses.
	let (b, s) = crate::modern_vendor::filter(ram, b, s);
	// [PC] The modern Galactic Map (modern_map.rs): the mouse's pulses.
	let (b, s) = crate::modern_map::filter(ram, b, s);
	// [PC] The modern Ship Shack (modern_shack.rs), the same for level 24's shop.
	let (b, s) = crate::modern_shack::filter(ram, b, s);
	// [PC] The modern Weapons / Gadgets / Items pages (modern_grid.rs, modern_items.rs).
	let (b, s) = crate::modern_grid::filter(ram, b, s);
	let (b, s) = crate::modern_qs::filter(ram, b, s);
	let (b, s) = crate::modern_help::filter(ram, b, s);
	(b, s)
}

/// The game thread's side of the engine's world.
struct Feed {
	/// The level the world loader was last asked for (it starts on level 0, `world_loader`).
	level_sent: u32,
	/// The terrain chunk (`chunks::Sig`) the loader was last asked for, and how many requests it has had (the first is its own).
	asked: Option<crate::chunks::Sig>,
	sent: u64,
	res: Resolver,
	tx: Sender<crate::chunks::Req>,
}

impl Feed {
	/// After a field: the game's camera, level and mobies for the window, a new level number to the world loader (the game's
	/// loader wrote it: decoding starts while the level's data loads; lockstep waits for it). True = the next GS field is a
	/// HUD overlay.
	fn field(&mut self, m: &ee_oracle::Machine, shared: &Shared, lockstep: bool, field: u64) -> bool {
		self.res.field = field;
		let scene = Scene { field, paused: crate::modern_screen::pause_up(&m.cpu.mem.ram), ..Scene::read(m, &mut self.res) };
		scene_log(m, &scene, field);
		let new_level = (0..64).contains(&scene.level) && scene.level as u32 != self.level_sent;
		// The same level with another terrain chunk resident (`chunks`): the world is decoded again with that chunk.
		if new_level || (scene.level as u32 == self.level_sent && crate::chunks::wanted(scene.level as usize, self.asked, scene.chunk)) {
			self.level_sent = scene.level as u32;
			self.asked = scene.chunk;
			self.sent += 1;
			let _ = self.tx.send(crate::chunks::Req { level: self.level_sent, sig: scene.chunk });
			if lockstep {
				let want = self.sent;
				shared.loaded.wait(|l| l >= want, &shared.quit);
			}
		}
		let overlay = shared.overlay.load(Ordering::Relaxed) && scene.view.is_some();
		shared.playing.store(scene.view.is_some(), Ordering::Relaxed);
		{
			let mut views = shared.views.lock().unwrap();
			views.push(field, scene.view);
			views.note_hero(field, scene.hero_group.clone());
		}
		// [PC] The effect pool of this field while Fire / Smoke / weapon lights want it (rac2/fx_pool.rs, host/ov_fire.rs).
		crate::fx_pool::note(field, &m.cpu.mem.ram, scene.level);
		crate::fx_shots::note(field, &m.cpu.mem.ram, &scene.mobies); // [b47 X1] the player's shots (explosion gate, rocket lights)
		super::ov_layers::note_camera(field, scene.view.is_some());
		*shared.view.lock().unwrap() = scene;
		overlay
	}
}

/// Debug: RAC2_SCENELOG=FROM-TO prints one line per drawn moby of each field in that range (record, class id, class key, position,
/// light word, shadow caster, joints).
fn scene_log(m: &ee_oracle::Machine, scene: &Scene, field: u64) {
	static RANGE: std::sync::OnceLock<Option<(u64, u64)>> = std::sync::OnceLock::new();
	let range = RANGE.get_or_init(|| {
		let v = std::env::var("RAC2_SCENELOG").ok()?;
		let (a, b) = v.split_once('-')?;
		Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
	});
	if !range.is_some_and(|r| (r.0..=r.1).contains(&field)) {
		return;
	}
	class_dump(m, scene);
	for mb in &scene.mobies {
		let cid = m.rd32(mb.addr + 0xa8) >> 16;
		eprintln!("scenelog {field}: moby {:#x} class {cid:#x} key {:#x} at {:.1?} scale {:.3} light {:x?} late {} tint {:x?} joints {} flags {:#x} mask {:#x}", mb.addr, mb.key, mb.pos, mb.scale, mb.light, mb.late, mb.tint, mb.pose.as_ref().map_or(0, |p| p.len()), m.rd32(mb.addr + 0x28), mb.mask);
		// RAC2_SCENELOG_RAW=0xCLASS,.. (or all): the whole 0x100-byte record as words.
		if raw_class(cid) {
			let w: Vec<String> = (0..64).map(|i| format!("{:08x}", m.rd32(mb.addr + i * 4))).collect();
			eprintln!("scenelog {field}: raw {:#x}: {}", mb.addr, w.join(" "));
		}
	}
}

/// Debug: RAC2_CLASSDUMP=1 (with RAC2_SCENELOG): once per class header, its first 0x48 bytes, the part table (class + byte
/// [+0x2c] * 0x10, 0x130 bytes) and the packet table (high + low + 24 entries of 16 bytes), as words.
fn class_dump(m: &ee_oracle::Machine, scene: &Scene) {
	static DONE: std::sync::Mutex<Vec<u32>> = std::sync::Mutex::new(Vec::new());
	if std::env::var_os("RAC2_CLASSDUMP").is_none() {
		return;
	}
	let words = |a: u32, n: u32| (0..n).map(|i| format!("{:08x}", m.rd32(a + i * 4))).collect::<Vec<_>>().join(" ");
	for mb in &scene.mobies {
		let hdr = m.rd32(mb.addr + 0x24) & 0x01ff_ffff;
		if DONE.lock().unwrap().contains(&hdr) {
			continue;
		}
		DONE.lock().unwrap().push(hdr);
		eprintln!("classdump {hdr:#x} id {:#x}: head {}", m.rd32(mb.addr + 0xa8) >> 16, words(hdr, 0x12));
		let b = m.rd8(hdr + 0x2c) as u32;
		if b != 0 {
			eprintln!("classdump {hdr:#x}: parts at {:#x}: {}", hdr + b * 16, words(hdr + b * 16, 0x4c));
		}
		let (table, n) = (m.rd32(hdr) & 0x01ff_ffff, m.rd8(hdr + 4) as u32 + m.rd8(hdr + 5) as u32 + 24);
		for k in 0..n {
			eprintln!("classdump {hdr:#x}: packet {k}: {}", words(table + k * 16, 4));
		}
	}
}

/// Debug: RAC2_SCENELOG_RAW=0xCLASS,.. (class ids, hex) or `all`: the classes `scene_log` also dumps whole records of.
fn raw_class(cid: u32) -> bool {
	static RAW: std::sync::OnceLock<(bool, Vec<u32>)> = std::sync::OnceLock::new();
	let (all, list) = RAW.get_or_init(|| {
		let v = std::env::var("RAC2_SCENELOG_RAW").unwrap_or_default();
		(v.trim() == "all", v.split(',').filter_map(|x| u32::from_str_radix(x.trim().trim_start_matches("0x"), 16).ok()).collect())
	});
	*all || list.contains(&cid)
}
