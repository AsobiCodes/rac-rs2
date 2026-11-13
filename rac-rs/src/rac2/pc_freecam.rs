//! [PC] Free camera for playtests (parity row 71; R&C1's twin is `retail/devtools.rs`): Debug mode only (`options::debug_mode`).
//! F2 switches it on: the world the engine draws leaves the game's camera and flies freely (the window replaces the scene's
//! `View` right before the native draw, `host/frame.rs`), so NOTHING is written to the game's camera RAM and there is nothing to
//! undo at exit: the game keeps its own camera, Ratchet gets a neutral pad (`filter`) and stands still. P freezes the game
//! (`hold`: the game thread waits), `.` (period) steps one field while frozen, F2 again returns to the game's camera.
//!
//! Keys while on (held keys come from the window, `key`): W / S forward / back along the view, A / D left / right, Space / C up /
//! down, arrows or the mouse look (`mouse`, the cursor is held), Shift fast, `[` / `]` or the wheel slower / faster (steps of x1.5),
//! R back to the game's view. Gamepad (`pad`, as R&C1's devtools.rs): L3 + R3 together toggle it (the game sees neither click until
//! both are released), Start freezes, Select steps, Triangle resets, the left stick flies, the right stick looks, R1 / L1 up /
//! down, R2 fast, L2 slow. The effect layers the GS captured (fire, smoke, HUD)
//! are not warped to the free view (they belong to the game's camera); the game's own culling and level of detail still follow
//! the game's camera, so mobies behind the game's view are drawn only because `moby_cull::OFF` is set while the camera is free.
//! Test: RAC2_FREECAM_SCRIPT=`FIELD:cmd;...` (FIELD = the scene's game field; each runs once): `f` toggle, `p` freeze toggle, `s` one
//! step, `h` a picture of the next frame (fcNNN.png; the script clock = game field + frames drawn while frozen), `oF,R,U` move that many metres forward / right / up, `lYAW,PITCH` look by that many degrees, `cX,Y,Z,YAW,PITCH` set the pose.
use crate::scene::View;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use winit::keyboard::KeyCode as K;

static ON: AtomicBool = AtomicBool::new(false);
static FROZEN: AtomicBool = AtomicBool::new(false);
static STEPS: AtomicU32 = AtomicU32::new(0);
/// Test script `h`: pictures asked for (taken by the window with `take_snap`, the game may be frozen then).
static SNAPS: AtomicU32 = AtomicU32::new(0);
static SNAPS_DONE: AtomicU32 = AtomicU32::new(0);

const BASE_SPEED: f32 = 12.0; // metres per second
const FAST: f32 = 4.0;
const SPEED_STEP: f32 = 1.5;
const TURN: f32 = 1.6; // radians per second with an arrow key held
/// Mouse look (radians per count) and stick look (radians per second at full tilt), the sticks' dead zone (R&C1 devtools.rs).
const MOUSE_RATE: f32 = 0.0025;
const STICK_TURN: f32 = 2.6;
const STICK_DEAD: i32 = 0x18;
const SLOW: f32 = 0.25;
/// PS2 pad bits (pressed = 1).
const PAD_L2: u16 = 0x1;
const PAD_R2: u16 = 0x2;
const PAD_L1: u16 = 0x4;
const PAD_R1: u16 = 0x8;
const PAD_TRI: u16 = 0x10;
const PAD_SELECT: u16 = 0x100;
const PAD_L3: u16 = 0x200;
const PAD_R3: u16 = 0x400;
const PAD_START: u16 = 0x800;
/// The help line drawn at the bottom while the free camera is on (pc_devtools.rs `draw`).
pub const HELP: &str = "WASD MOVE - SPACE/C UP/DOWN - MOUSE/ARROWS LOOK - SHIFT FAST - WHEEL OR BRACKET KEYS SPEED - P FREEZE - . STEP - R RESET - F2 EXIT";
/// Pad buttons the game must not see: the ones held when L3 + R3 toggled the camera, until each is released (Debug mode only).
static PAD_MASK: AtomicU32 = AtomicU32::new(0);
const PITCH_MAX: f32 = 1.5;
const NOTE_FOR: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, PartialEq)]
struct Pose {
	pos: [f32; 3],
	yaw: f32,
	pitch: f32,
}

#[derive(Default)]
struct St {
	held: HashSet<K>,
	/// The pose while on (None = it starts from the game's view on the next frame).
	pose: Option<Pose>,
	speed: i32,
	at: Option<Instant>,
	note: Option<(String, Instant)>,
	/// Test script: (field, command), and how far it ran.
	script: Option<Vec<(u64, String)>>,
	ran: usize,
	/// The game's view of the last frame (for the title).
	last: Option<View>,
	/// Frames drawn while the game was frozen: the test script's clock runs on (field + this), so a frozen script goes on.
	ticks: u64,
	/// Mouse motion since the last frame (counts).
	mouse: (f32, f32),
	/// The real gamepad of the newest poll (buttons, sticks [rx, ry, lx, ly]) and the buttons of the poll before (edges).
	pad: (u16, [u8; 4]),
	pad_prev: u16,
}

static ST: Mutex<Option<St>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut St) -> R) -> R {
	let mut g = ST.lock().unwrap_or_else(|e| e.into_inner());
	f(g.get_or_insert_with(St::default))
}

/// The free camera is on (the game thread and the window read it without a lock).
pub fn on() -> bool {
	ON.load(Ordering::Relaxed)
}

fn set_on(s: &mut St, on: bool) {
	ON.store(on, Ordering::Relaxed);
	if !on {
		FROZEN.store(false, Ordering::Relaxed);
		STEPS.store(0, Ordering::Relaxed);
	}
	s.pose = None;
	s.at = None;
	s.mouse = (0.0, 0.0);
	s.note = Some((if on { "FREE CAMERA ON (F2 off, P freeze)".into() } else { "FREE CAMERA OFF".into() }, Instant::now()));
	eprintln!("pc_freecam: {}", if on { "on" } else { "off" });
}

/// Window: a key event (`pressed`, `repeat`). True = taken (Debug mode on and F2, or the free camera's own keys while it is on).
pub fn key(k: K, pressed: bool, repeat: bool) -> bool {
	if !crate::options::debug_mode() {
		return false;
	}
	with(|s| {
		if k == K::F2 {
			if pressed && !repeat {
				let now = !on();
				set_on(s, now);
			}
			return true;
		}
		if !on() {
			s.held.clear();
			return false;
		}
		if pressed {
			s.held.insert(k);
		} else {
			s.held.remove(&k);
		}
		match k {
			K::KeyP if pressed && !repeat => {
				let f = !FROZEN.fetch_xor(true, Ordering::Relaxed);
				s.note = Some((format!("GAME {}", if f { "FROZEN (. steps)" } else { "RUNNING" }), Instant::now()));
				true
			}
			K::Period if pressed && !repeat => {
				STEPS.fetch_add(1, Ordering::Relaxed);
				true
			}
			K::BracketLeft | K::BracketRight if pressed && !repeat => {
				s.speed = (s.speed + if k == K::BracketRight { 1 } else { -1 }).clamp(-12, 16);
				true
			}
			K::KeyR if pressed && !repeat => {
				s.pose = None;
				true
			}
			_ => matches!(k, K::KeyR | K::KeyW | K::KeyA | K::KeyS | K::KeyD | K::Space | K::KeyC | K::ArrowUp | K::ArrowDown | K::ArrowLeft | K::ArrowRight | K::ShiftLeft | K::ShiftRight),
		}
	})
}

/// Window title note: the newest event (None = nothing to show; the pose and FROZEN are in `status`, pc_devtools.rs readout).
pub fn note_now() -> Option<String> {
	with(|s| s.note.as_ref().filter(|(_, t)| t.elapsed() < NOTE_FOR).map(|(n, _)| n.clone()))
}

/// The readout's part (pc_devtools.rs): None while off, else the pose, speed and freeze.
pub fn status() -> Option<String> {
	if !on() {
		return None;
	}
	with(|s| {
		let v = BASE_SPEED * SPEED_STEP.powi(s.speed);
		let frozen = if FROZEN.load(Ordering::Relaxed) { " FROZEN" } else { "" };
		Some(match s.pose {
			Some(p) => format!("FREECAM {:.1} {:.1} {:.1} YAW {:.0} PITCH {:.0} SPEED {v:.1}{frozen}", p.pos[0], p.pos[1], p.pos[2], p.yaw.to_degrees(), p.pitch.to_degrees()),
			None => format!("FREECAM SPEED {v:.1}{frozen}"),
		})
	})
}

/// Window: raw mouse motion. True = the free camera took it (the Modern camera must not turn).
pub fn mouse(dx: f32, dy: f32) -> bool {
	if !on() {
		return false;
	}
	with(|s| {
		s.mouse.0 += dx;
		s.mouse.1 += dy;
	});
	true
}

/// Window: wheel notches (positive = away = faster). True = the free camera took them.
pub fn wheel(lines: f32) -> bool {
	if !on() {
		return false;
	}
	if lines != 0.0 {
		with(|s| s.speed = (s.speed + lines.signum() as i32).clamp(-12, 16));
	}
	true
}

/// L3 + R3 both down, one of them just pressed: the gamepad's free camera toggle.
fn pad_toggles(b: u16, edge: u16) -> bool {
	b & (PAD_L3 | PAD_R3) == PAD_L3 | PAD_R3 && edge & (PAD_L3 | PAD_R3) != 0
}

/// Window, every poll in Debug mode: the real gamepad (buttons pressed = 1, sticks [rx, ry, lx, ly]; none = 0 / neutral).
pub fn pad(b: u16, sticks: [u8; 4]) {
	if !crate::options::debug_mode() {
		return;
	}
	with(|s| {
		let edge = b & !s.pad_prev;
		s.pad_prev = b;
		s.pad = (b, sticks);
		// A released button leaves the mask.
		let mut mask = PAD_MASK.load(Ordering::Relaxed) as u16 & b;
		if pad_toggles(b, edge) {
			let now = !on();
			set_on(s, now);
			mask = b;
		}
		PAD_MASK.store(mask as u32, Ordering::Relaxed);
		if !on() {
			return;
		}
		if edge & PAD_START != 0 {
			let f = !FROZEN.fetch_xor(true, Ordering::Relaxed);
			s.note = Some((format!("GAME {}", if f { "FROZEN (SELECT steps)" } else { "RUNNING" }), Instant::now()));
		}
		if edge & PAD_SELECT != 0 && FROZEN.load(Ordering::Relaxed) {
			STEPS.fetch_add(1, Ordering::Relaxed);
		}
		if edge & PAD_TRI != 0 {
			s.pose = None;
		}
	});
}

/// Debug mode was switched Off (options.rs): the camera, the freeze, the speed and the pad mask go, as R&C1's `set_menu`.
pub fn menu_off() {
	with(|s| {
		if on() {
			set_on(s, false);
		}
		s.held.clear();
		s.speed = 0;
		s.note = None;
	});
	PAD_MASK.store(0, Ordering::Relaxed);
}

/// A stick axis (0..255, 0x80 = centre) as -1..1 past the dead zone.
fn stick(v: u8) -> f32 {
	let d = v as i32 - 0x80;
	if d.abs() <= STICK_DEAD { 0.0 } else { (d - d.signum() * STICK_DEAD) as f32 / (127 - STICK_DEAD) as f32 }
}

/// Game thread: the pad the game gets (neutral while the free camera is on: Ratchet stands still, the keys are the camera's).
pub fn filter(b: u16, s: [u8; 4]) -> (u16, [u8; 4]) {
	pad_for(b, s, crate::options::debug_mode)
}

fn pad_for(b: u16, s: [u8; 4], debug: impl Fn() -> bool) -> (u16, [u8; 4]) {
	if on() && still_debug(&debug) {
		return (0, [0x80; 4]);
	}
	// The stick clicks that toggled the camera, until released (only ever set in Debug mode).
	let m = PAD_MASK.load(Ordering::Relaxed) as u16;
	if m != 0 && debug() { (b & !m, s) } else { (b, s) }
}

/// Debug mode was switched off (Options page) while the free camera was on: `key` no longer sees F2, so switch the camera off here
/// (neutral pad and freeze released). Only asked while the free camera is on.
fn still_debug(debug: impl Fn() -> bool) -> bool {
	if debug() {
		return true;
	}
	with(|s| set_on(s, false));
	false
}

/// Game thread, before each field: waits while the game is frozen (a step lets one field through).
pub fn hold(quit: &AtomicBool) {
	while on() && FROZEN.load(Ordering::Relaxed) && still_debug(crate::options::debug_mode) && !quit.load(Ordering::Relaxed) {
		if STEPS.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_sub(1)).is_ok() {
			return;
		}
		std::thread::sleep(Duration::from_millis(4));
	}
}

fn pose_of(v: &View) -> Pose {
	let f = [v.basis[0][2], v.basis[1][2], v.basis[2][2]];
	let l = (f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt().max(1e-9);
	let f = f.map(|x| x / l);
	Pose { pos: v.eye, yaw: f[1].atan2(f[0]), pitch: f[2].clamp(-1.0, 1.0).asin() }
}

/// The view of a pose, with the game's lens: columns = right, down, forward.
fn view_of(p: &Pose, base: &View) -> View {
	let (sy, cy) = p.yaw.sin_cos();
	let (sp, cp) = p.pitch.sin_cos();
	let f = [cy * cp, sy * cp, sp];
	let r = [sy, -cy, 0.0];
	// up = right x forward; the view's down is minus it.
	let up = [r[1] * f[2] - r[2] * f[1], r[2] * f[0] - r[0] * f[2], r[0] * f[1] - r[1] * f[0]];
	let d = up.map(|x| -x);
	let mut basis = [[0.0; 3]; 3];
	for i in 0..3 {
		basis[i] = [r[i], d[i], f[i]];
	}
	View { eye: p.pos, basis, ..*base }
}

fn run_script(s: &mut St, field: u64) {
	let script = s.script.take().unwrap_or_else(|| {
		std::env::var("RAC2_FREECAM_SCRIPT").map(|v| v.split(';').filter_map(|e| e.split_once(':').and_then(|(f, c)| Some((f.trim().parse().ok()?, c.trim().to_string())))).collect()).unwrap_or_default()
	});
	run_commands(s, &script, field);
	s.script = Some(script);
}

fn run_commands(s: &mut St, script: &[(u64, String)], field: u64) {
	while s.ran < script.len() && script[s.ran].0 <= field {
		let c = script[s.ran].1.clone();
		if matches!(&c[..1], "o" | "l" | "c") && s.pose.is_none() {
			break; // the pose starts with the next frame
		}
		s.ran += 1;
		let nums = |t: &str| -> Vec<f32> { t.split(',').filter_map(|x| x.trim().parse().ok()).collect() };
		let (head, rest) = c.split_at(1);
		match head {
			"f" => {
				let now = !on();
				set_on(s, now);
			}
			"p" => {
				FROZEN.fetch_xor(true, Ordering::Relaxed);
			}
			"s" => {
				STEPS.fetch_add(1, Ordering::Relaxed);
			}
			"h" => {
				SNAPS.fetch_add(1, Ordering::Relaxed);
			}
			"o" => {
				let n = nums(rest);
				if let (Some(p), &[fw, rt, up]) = (s.pose.as_mut(), n.as_slice()) {
					let (sy, cy) = p.yaw.sin_cos();
					let (sp, cp) = p.pitch.sin_cos();
					for i in 0..3 {
						p.pos[i] += [cy * cp, sy * cp, sp][i] * fw + [sy, -cy, 0.0][i] * rt;
					}
					p.pos[2] += up;
				}
			}
			"l" => {
				let n = nums(rest);
				if let (Some(p), &[y, pi]) = (s.pose.as_mut(), n.as_slice()) {
					p.yaw += y.to_radians();
					p.pitch = (p.pitch + pi.to_radians()).clamp(-PITCH_MAX, PITCH_MAX);
				}
			}
			"c" => {
				let n = nums(rest);
				if let (Some(p), &[x, y, z, yaw, pitch]) = (s.pose.as_mut(), n.as_slice()) {
					*p = Pose { pos: [x, y, z], yaw: yaw.to_radians(), pitch: pitch.to_radians() };
				}
			}
			_ => {}
		}
	}
}

/// Window, once per frame: a picture was asked for by the test script (`h`); true once per request (a lockstep run saves it as
/// fcNNN.png next to its window shots).
pub fn take_snap() -> Option<u32> {
	SNAPS.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_sub(1)).ok().map(|_| SNAPS_DONE.fetch_add(1, Ordering::Relaxed))
}

/// Window, once per frame right before the native draw: the view to draw (the game's, or the free camera's). `field` = the scene's
/// game field (the test script's clock).
pub fn apply(view: Option<View>, field: u64) -> Option<View> {
	with(|s| {
		// The window keeps drawing while the game is frozen: the script's clock goes on with those frames (a frozen game cannot
		// reach its next field, so a script that waited for one would hang a lockstep test).
		s.ticks += (on() && FROZEN.load(Ordering::Relaxed)) as u64;
		let field = field + s.ticks;
		run_script(s, field);
		let base = view?;
		s.last = Some(base);
		if !on() {
			return Some(base);
		}
		let now = Instant::now();
		let dt = s.at.replace(now).map_or(0.0, |t| (now - t).as_secs_f32()).clamp(0.0, 0.1);
		if s.pose.is_none() {
			s.pose = Some(pose_of(&base));
			run_script(s, field);
		}
		let mouse = std::mem::take(&mut s.mouse);
		let (pad, speed) = (s.pad, s.speed);
		let held = &s.held;
		let Some(p) = s.pose.as_mut() else { return Some(base) };
		fly(p, dt, &|k| held.contains(&k), mouse, pad, speed);
		Some(view_of(p, &base))
	})
}

/// One frame of free flight (R&C1 devtools.rs `step`): keys / mouse / pad -> pose. Movement is along the view (forward follows the
/// pitch), up / down along world z.
fn fly(p: &mut Pose, dt: f32, held: &dyn Fn(K) -> bool, mouse: (f32, f32), (pb, ps): (u16, [u8; 4]), speed: i32) {
	let key = |k: K| held(k) as i32 as f32;
	let bit = |m: u16| (pb & m != 0) as i32 as f32;
	// Look: mouse, arrows, right stick ([rx, ry, lx, ly], y grows downward).
	p.yaw -= mouse.0 * MOUSE_RATE + (key(K::ArrowRight) - key(K::ArrowLeft)) * TURN * dt + stick(ps[0]) * STICK_TURN * dt;
	p.pitch -= mouse.1 * MOUSE_RATE + (key(K::ArrowDown) - key(K::ArrowUp)) * TURN * dt + stick(ps[1]) * STICK_TURN * dt;
	p.pitch = p.pitch.clamp(-PITCH_MAX, PITCH_MAX);
	p.yaw = p.yaw.rem_euclid(std::f32::consts::TAU);
	let mut v = BASE_SPEED * SPEED_STEP.powi(speed);
	if key(K::ShiftLeft) + key(K::ShiftRight) + bit(PAD_R2) > 0.0 {
		v *= FAST;
	}
	if pb & PAD_L2 != 0 {
		v *= SLOW;
	}
	let (sy, cy) = p.yaw.sin_cos();
	let (sp, cp) = p.pitch.sin_cos();
	let fw = (key(K::KeyW) - key(K::KeyS) - stick(ps[3])).clamp(-1.0, 1.0);
	let rt = (key(K::KeyD) - key(K::KeyA) + stick(ps[2])).clamp(-1.0, 1.0);
	let up = (key(K::Space) - key(K::KeyC) + bit(PAD_R1) - bit(PAD_L1)).clamp(-1.0, 1.0);
	for i in 0..3 {
		p.pos[i] += ([cy * cp, sy * cp, sp][i] * fw + [sy, -cy, 0.0][i] * rt) * v * dt;
	}
	p.pos[2] += up * v * dt;
}

#[cfg(test)]
mod tests {
	use super::*;

	fn base() -> View {
		View { eye: [1.0, 2.0, 3.0], basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], fx: 500.0, fy: 500.0, half: [256.0, 224.0] }
	}

	#[test]
	fn a_pose_round_trips_through_the_view() {
		let b = base();
		for (yaw, pitch) in [(0.0f32, 0.0f32), (1.0, 0.3), (-2.0, -0.6), (3.0, 0.0)] {
			let p = Pose { pos: [10.0, -4.0, 7.0], yaw, pitch };
			let v = view_of(&p, &b);
			let q = pose_of(&v);
			assert!((q.yaw - yaw).abs() < 1e-4 && (q.pitch - pitch).abs() < 1e-4 && q.pos == p.pos, "{yaw} {pitch}");
			assert_eq!((v.fx, v.half), (b.fx, b.half), "the lens stays the game's");
			// the view's down points to the ground when level: minus world z
			if pitch == 0.0 {
				assert!(v.basis[2][1] < -0.999, "down = -z");
			}
		}
	}

	#[test]
	fn the_pad_is_neutral_only_while_on() {
		ON.store(false, Ordering::Relaxed);
		assert_eq!(filter(0x40, [1, 2, 3, 4]), (0x40, [1, 2, 3, 4]));
		ON.store(true, Ordering::Relaxed);
		assert_eq!(pad_for(0x40, [1, 2, 3, 4], || true), (0, [0x80; 4]));
		// Debug mode switched off while the camera is on: the camera goes off, the pad and the freeze are the game's again.
		FROZEN.store(true, Ordering::Relaxed);
		assert_eq!(pad_for(0x40, [1, 2, 3, 4], || false), (0x40, [1, 2, 3, 4]));
		assert!(!on() && !FROZEN.load(Ordering::Relaxed));
		ON.store(false, Ordering::Relaxed);
		// The stick clicks that switched it off stay away from the game until released, in Debug mode only.
		PAD_MASK.store((PAD_L3 | PAD_R3) as u32, Ordering::Relaxed);
		assert_eq!(pad_for(0x640, [1, 2, 3, 4], || true), (0x40, [1, 2, 3, 4]));
		assert_eq!(pad_for(0x640, [1, 2, 3, 4], || false), (0x640, [1, 2, 3, 4]));
		PAD_MASK.store(0, Ordering::Relaxed);
	}

	#[test]
	fn the_sticks_have_a_dead_zone_and_l3_r3_toggles_on_an_edge() {
		assert_eq!(stick(0x80), 0.0);
		assert_eq!(stick(0x80 + STICK_DEAD as u8), 0.0);
		assert_eq!(stick(0xff), 1.0);
		assert_eq!(stick(0x80 - 127), -1.0);
		assert!(stick(0xc0) > 0.0 && stick(0x40) < 0.0);
		let both = PAD_L3 | PAD_R3;
		assert!(pad_toggles(both, PAD_R3), "the second click");
		assert!(!pad_toggles(both, 0), "held: no new toggle");
		assert!(!pad_toggles(PAD_L3, PAD_L3), "one click alone");
	}

	#[test]
	fn fly_turns_with_the_mouse_and_moves_with_the_left_stick() {
		let none = |_: K| false;
		let mut p = Pose { pos: [0.0; 3], yaw: 0.0, pitch: 0.0 };
		fly(&mut p, 0.0, &none, (100.0, -40.0), (0, [0x80; 4]), 0);
		assert!((p.yaw - (std::f32::consts::TAU - 0.25)).abs() < 1e-5 && (p.pitch - 0.1).abs() < 1e-6, "{p:?}");
		let mut p = Pose { pos: [0.0; 3], yaw: 0.0, pitch: 0.0 };
		// Left stick full up = forward (+x at yaw 0), R1 = up, L2 slow.
		fly(&mut p, 0.1, &none, (0.0, 0.0), (PAD_R1 | PAD_L2, [0x80, 0x80, 0x80, 0x01]), 0);
		let v = BASE_SPEED * SLOW * 0.1;
		assert!((p.pos[0] - v).abs() < 1e-4 && p.pos[1].abs() < 1e-5 && (p.pos[2] - v).abs() < 1e-4, "{p:?}");
	}
}
