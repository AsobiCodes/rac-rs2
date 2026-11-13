//! [PC] RAC2_FRAMELOG=path (debug aid for stutter hunts, no retail counterpart): one CSV row per window redraw with the wall time
//! since the previous redraw began and the time of each stage of `App::redraw` (host/frame.rs). Off = one relaxed load per mark.
//! The s0..s11 columns are the render thread's ON-CPU milliseconds (thread cycle counter, so a busy PC does not inflate them).
//! Columns: t_ms (since the first redraw), dt (since the previous redraw began), then ms in: pick (movie, GS picture, world
//! install, scene pick), field (native world), menus (layers, modern pages, HUD), draw (`Gpu::draw`), pace (frame cap wait).
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

const N: usize = 5;
const NS: usize = 12;

static ON: AtomicBool = AtomicBool::new(false);
static ST: OnceLock<Mutex<St>> = OnceLock::new();

struct St {
	out: std::io::BufWriter<std::fs::File>,
	t0: Instant,
	last_begin: Option<Instant>,
	mark: Instant,
	ph: [f32; N],
	sub: [f32; NS],
	sub_mark: u64,
	dt: f32,
	rows: u32,
}

pub(super) fn init() {
	if std::env::var_os("RAC2_FRAMELOG").is_some() {
		let _ = per_ms();
	}
	rac_engine::viewer::profile::init_from_env(); // RAC_FRAME_CSV / RAC_FRAME_PROFILE: the engine's span profiler (this, the render thread)
	if let Some(p) = std::env::var("RAC2_FRAMELOG").ok().filter(|p| !p.is_empty()) {
		if let Ok(f) = std::fs::File::create(p) {
			let now = Instant::now();
			let mut out = std::io::BufWriter::new(f);
			let _ = writeln!(out, "t_ms,dt,pick,field,menus,draw,pace,s0,s1,s2,s3,s4,s5,s6,s7,s8,s9,s10,s11");
			let _ = ST.set(Mutex::new(St { out, t0: now, last_begin: None, mark: now, ph: [0.0; N], sub: [0.0; NS], sub_mark: cycles(), dt: 0.0, rows: 0 }));
			ON.store(true, Ordering::Relaxed);
		}
	}
}

// [b50 V9] RAC_DIAG / RAC_FRAME_PROFILE / RAC_FPS_METER: this window's redraw mapped onto the engine profiler's phases (perf.csv
// cpu_* columns, spikes.txt). The game's field runs on its OWN thread, so cpu_ticks / cpu_audio stay 0 here; its cost comes as the
// spans game_field_avg / game_field_worst / game_host_avg (spikes.txt). Phase map (render thread):
//   input  = pick: movie poll, GS picture take, world install, scene pick      (stage 0)
//   hero   = classes sync, metal + volumes (sub slots 0, 1)
//   props  = props, water, vegetation (sub slots 2, 5..11; the rest of stage 1)
//   upload = props / volumes / tie colours handed to the engine (sub slots 3, 4)
//   post   = the effect layers (ov_layers.redraw, water_only)
//   hud    = menus, modern pages, HUD (stage 2)
//   cull / acquire / encode / present = Gpu::draw's own laps (engine); end = after the draw, before the frame-cap wait.
// Off: every call below is one relaxed atomic load (profile::lap) or one OnceLock read (diag::on).
use rac_engine::viewer::profile::{self, Phase};
use std::cell::Cell;
use std::sync::atomic::AtomicU64;

/// The game thread's newest field seen by the window, and the field the last redraw counted up to.
static NEWEST: AtomicU64 = AtomicU64::new(0);
static SEEN: AtomicU64 = AtomicU64::new(0);
const STAGE_SPANS: [&str; 4] = ["pick", "native_world", "layers_menus_hud", "draw"];

thread_local! {
	static STAGE_AT: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// The game fields run since the last redraw (`seen` = 0: the first redraw counts none; capped, a load jumps the counter).
fn field_delta(newest: u64, seen: u64) -> u32 {
	if seen == 0 { 0 } else { newest.saturating_sub(seen).min(1000) as u32 }
}

/// The game thread's newest field, as the window sees it this redraw.
pub(super) fn note_field(f: u64) {
	NEWEST.store(f, Ordering::Relaxed);
}

/// The effect layers are drawn (host/frame.rs): closes the profiler's "post" phase.
pub(super) fn layers_done() {
	profile::lap(Phase::Post);
}

/// Which profiler phase a `sub` slot of `Stage::field` (and props.rs) closes.
fn sub_phase(i: usize) -> Phase {
	match i {
		0 | 1 => Phase::Hero,
		3 | 4 => Phase::Upload,
		_ => Phase::Props,
	}
}

/// The stage `i` just ended: closes its profiler phase, adds its plain-word span; after the draw (stage 3) the frame ends.
fn profile_stage(i: usize) {
	if i < 4 && rac_engine::diag::on() {
		let now = Instant::now();
		if let Some(t) = STAGE_AT.with(|c| c.replace(Some(now))) {
			profile::span_add(STAGE_SPANS[i], (now - t).as_secs_f32() * 1000.0);
		}
	}
	match i {
		0 => profile::lap(Phase::Input),
		1 => profile::lap(Phase::Props),
		2 => profile::lap(Phase::Hud),
		3 => {
			let ticks = field_delta(NEWEST.load(Ordering::Relaxed), SEEN.swap(NEWEST.load(Ordering::Relaxed), Ordering::Relaxed));
			profile::frame_end(ticks); // after the draw, before the frame-cap wait (stage 4): the wait is "between frames", not cpu_end
		}
		// [b51 G5] The redraw function's tail after the profiled frame ended (take_corner, the frame-cap wait): part of the NEXT
		// frame's interval, so the record names it ("pace/tail") instead of calling it "outside the redraw".
		4 if rac_engine::diag::on() => {
			if let Some(t) = STAGE_AT.with(|c| c.get()) {
				rac_engine::diag::note_tail((Instant::now() - t).as_secs_f32() * 1000.0);
			}
		}
		_ => {}
	}
}

pub(super) fn begin() {
	profile::frame_start();
	if rac_engine::diag::on() {
		STAGE_AT.with(|c| c.set(Some(Instant::now())));
	}
	if !ON.load(Ordering::Relaxed) {
		return;
	}
	if let Some(m) = ST.get() {
		let mut s = m.lock().unwrap();
		let now = Instant::now();
		s.dt = s.last_begin.map_or(0.0, |l| (now - l).as_secs_f32() * 1000.0);
		s.last_begin = Some(now);
		s.mark = now;
	}
}

/// Ends stage `i` (0 = pick .. 4 = pace); the last stage writes the row.
pub(super) fn stage(i: usize) {
	profile_stage(i); // [b50 V9] (the frame ended after the draw, stage 3, not after the pace wait)
	if !ON.load(Ordering::Relaxed) {
		return;
	}
	if let Some(m) = ST.get() {
		let mut s = m.lock().unwrap();
		let now = Instant::now();
		s.ph[i] = (now - s.mark).as_secs_f32() * 1000.0;
		s.mark = now;
		if i == N - 1 {
			let t = (now - s.t0).as_secs_f32() * 1000.0;
			let (dt, p) = (s.dt, s.ph);
			let q = s.sub;
			let _ = write!(s.out, "{t:.1},{dt:.2},{:.2},{:.2},{:.2},{:.2},{:.2}", p[0], p[1], p[2], p[3], p[4]);
			for x in q {
				let _ = write!(s.out, ",{x:.2}");
			}
			let _ = writeln!(s.out);
			s.sub = [0.0; NS];
			s.rows += 1;
			if s.rows % 120 == 0 {
				let _ = s.out.flush();
			}
		}
	}
}

static GST: OnceLock<Mutex<(std::io::BufWriter<std::fs::File>, u32)>> = OnceLock::new();

/// The game thread's side (host/game.rs): one row per field in `<path>.game`: field, wall ms since start, ms in `Machine::frame`,
/// ms in the other per-field host work, ms blocked in the send to the GS pipe, ms asleep for the field clock.
pub(super) fn game_row(field: u64, t_ms: f32, frame: f32, host: f32, send: f32, sleep: f32) {
	if rac_engine::diag::on() {
		// [b51 G5] RAC_DIAG: the game thread's field / host / send-blocked time (diag/outside.rs: the block's "game thread" line and
		// the game_* columns). [b52 H3] Not added to the redraw's ticks phase any more: other-thread time is not redraw time.
		rac_engine::diag::game_field(frame, host, send);
	}
	if !ON.load(Ordering::Relaxed) {
		return;
	}
	let m = GST.get_or_init(|| {
		let p = std::env::var("RAC2_FRAMELOG").unwrap_or_default() + ".game";
		let mut w = std::io::BufWriter::new(std::fs::File::create(p).expect("framelog game"));
		let _ = writeln!(w, "field,t_ms,frame,host,send,sleep");
		Mutex::new((w, 0))
	});
	let mut g = m.lock().unwrap();
	let _ = writeln!(g.0, "{field},{t_ms:.1},{frame:.2},{host:.2},{send:.2},{sleep:.2}");
	g.1 += 1;
	if g.1 % 100 == 0 {
		let _ = g.0.flush();
	}
}

/// Inside a stage: restart the sub-clock (`sub_start`) and add the time since the last mark to slot `i` (0..8). The slots of
/// `Stage::field` (host/frame.rs): 0 classes sync, 1 metal + volumes, 2 props, 3 tie colours, 4 water, 5 vegetation.
pub(crate) fn sub_start() {
	if ON.load(Ordering::Relaxed) {
		if let Some(m) = ST.get() {
			m.lock().unwrap().sub_mark = cycles();
		}
	}
}

pub(crate) fn sub(i: usize) {
	profile::lap(sub_phase(i)); // [b50 V9] (a no-op off the window thread or with the profiler off)
	if ON.load(Ordering::Relaxed) {
		if let Some(m) = ST.get() {
			let mut s = m.lock().unwrap();
			let c = cycles();
			s.sub[i] += (c.wrapping_sub(s.sub_mark) as f64 / per_ms()) as f32;
			s.sub_mark = c;
		}
	}
}

#[cfg(windows)]
fn cycles() -> u64 {
	unsafe extern "system" {
		fn GetCurrentThread() -> *mut std::ffi::c_void;
		fn QueryThreadCycleTime(thread: *mut std::ffi::c_void, cycles: *mut u64) -> i32;
	}
	let mut c = 0u64;
	// SAFETY: the pseudo handle of the calling thread and a valid out pointer.
	unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut c) };
	c
}

#[cfg(not(windows))]
fn cycles() -> u64 {
	0
}

/// Thread cycles per millisecond (one 20 ms spin at the first call).
fn per_ms() -> f64 {
	static K: OnceLock<f64> = OnceLock::new();
	*K.get_or_init(|| {
		let (t, c) = (Instant::now(), cycles());
		while t.elapsed() < std::time::Duration::from_millis(20) {
			std::hint::spin_loop();
		}
		(cycles().wrapping_sub(c) as f64 / (t.elapsed().as_secs_f64() * 1000.0)).max(1.0)
	})
}

/// Milliseconds on the log's clock (the t_ms column); 0 when RAC2_FRAMELOG is off.
pub(super) fn now_ms() -> f64 {
	ST.get().map_or(0.0, |m| (Instant::now() - m.lock().unwrap().t0).as_secs_f64() * 1e3)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_field_delta_counts_game_steps_between_redraws() {
		assert_eq!(field_delta(500, 0), 0, "the first redraw counts none");
		assert_eq!(field_delta(503, 500), 3);
		assert_eq!(field_delta(500, 500), 0);
		assert_eq!(field_delta(10, 500), 0, "a counter that went back (a load) counts none");
		assert_eq!(field_delta(90_000, 500), 1000, "a jump is capped");
	}

	#[test]
	fn every_sub_slot_closes_a_phase() {
		assert_eq!(sub_phase(0), Phase::Hero);
		assert_eq!(sub_phase(1), Phase::Hero);
		assert_eq!(sub_phase(3), Phase::Upload);
		assert_eq!(sub_phase(4), Phase::Upload);
		for i in [2, 5, 6, 7, 8, 9, 10, 11] {
			assert_eq!(sub_phase(i), Phase::Props);
		}
	}
}
