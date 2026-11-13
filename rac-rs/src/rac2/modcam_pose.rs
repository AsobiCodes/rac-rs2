//! The Modern camera's pose maths for R&C2 (lane scratch_rc2_cam, stage A, 2026-10-31): pure and game-neutral. It is R&C1's
//! `retail/modcam.rs` + `modcam_turn.rs` copied (R&C1's files stay untouched; rac2 never names rac1) and cut to what R&C2's hook needs:
//! the view rows for a yaw / pitch, the eye = pivot - forward x length, the mouse / right stick / invert / speed turn, the Static
//! camera (plain orbit with wheel zoom, wall pull-in and the rise over the hero) and the Smooth camera (the pivot on critically
//! damped springs, lazy recentre and default elevation, look-up on slopes, boom breathing with speed and open space, a ramped stick),
//! the seed from retail's pose and the blend back to it. What R&C1 has on top and is left out here: the orbit frame for magnetic walls
//! (world z is up), the drop probe, the lock-on framing and the Shoulder variant of the aim (the Raised lift is `modcam_shoulder.rs`,
//! used in `step`; R2 strafes natively in R&C2: retail turns Ratchet to the camera heading), the hero-state gates (the caller decides:
//! `PoseIn::retail_drives`, see `modcam_blend.rs`).
//!
//! Nothing here touches the game. The caller (stage B: modcam.rs, the hand on the follow class update) fills a `PoseIn` from RAM and
//! the window's input, passes a `Probe` (the game's own segment test, which has to restore everything it touches), and writes the
//! returned `Pose` into the camera record. Conventions (as R&C1): metres; yaw grows to the LEFT, pitch grows DOWN (`rows`); row 0 =
//! forward, row 1 = left, row 2 = up; the mouse turns the view the way it moves (right = look right), `look` is x right, y down.
//! Classic = this module is not used at all (the caller registers nothing), so retail stays bit-exact.

use crate::modcam_shoulder::{AIM_CEIL, AIM_LIFT, Lift};
use crate::modcam_frame::{FLAT, Frame};
use crate::modcam_look::{LookOff, view_of};
use rac_engine::fmath::Fm;

mod snap;
pub use snap::WORDS;

pub type V3 = [f32; 3];
pub type Rows = [V3; 3];

/// Height of the orbit pivot above the hero's position (m).
pub const PIVOT: f32 = 0.9;
/// Boom length limits (m) and pitch limits (rad, + looks down).
pub const ZOOM: (f32, f32) = (1.5, 14.0);
pub const PITCH: (f32, f32) = (-0.5, 1.3);
/// Right stick: pitch step and yaw rate per field at full deflection (rad), mouse count -> rad.
pub const TILT_STEP: f32 = 0.0305;
pub const ROTATE_RATE: f32 = 0.022_689_3;
pub const MOUSE: f32 = 0.003;
/// One wheel step (m of boom).
pub const WHEEL_STEP: f32 = 0.5;
/// Static: share of the way the pivot covers toward the hero per field; boom release outward per field (m); shortest boom (m).
const FOLLOW: f32 = 0.35;
const RELEASE: f32 = 0.1;
const MIN_LEN: f32 = 0.2;
/// Below this boom length (m) the camera rises over the hero; the rise ends once the boom has RISE_BAND more room; its spring.
const RISE_MIN: f32 = 1.8;
const RISE_BAND: f32 = 0.5;
const RISE_W: f32 = 0.25;
/// Fields to blend back to the retail camera.
pub const BLEND: u32 = 20;
/// A hero jump longer than this (m) in one field (respawn, teleport) re-seeds the camera from retail's pose.
pub const JUMP: f32 = 8.0;
/// Time (s) the boom takes to ease to the distance the player set (the seed starts from retail's boom).
const DIST_TAU: f32 = 0.6;
const UP: V3 = [0.0, 0.0, 1.0];

/// One field's camera input of the player (the window's, or the scripted feed).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Input {
	/// Mouse counts since the last field (x right, y down).
	pub look: [i16; 2],
	/// Right mouse button held (= R2, the strafe aim); `modcam_ui::with_aim` adds the R2 bit to the game's pad from this one only.
	pub aim: bool,
	/// The strafe is held: L2 or R2 in the pad as the game gets it (keyboard, gamepad, RMB, replay, feed). The camera's aim state
	/// (the lift, no recentre) follows this; it never changes what the game gets.
	pub strafe: bool,
	/// Wheel steps (positive = zoom in).
	pub zoom: i8,
	/// The camera keys' share of the right stick (Q/E, Z/X), offset from 0x80, 0 = not held (`camera_sticks`).
	pub keys: [i8; 2],
}

impl Input {
	pub fn is_zero(&self) -> bool {
		*self == Input::default()
	}
}

// ---- vectors ---------------------------------------------------------------------------------------------------------------

pub fn add3(a: V3, b: V3) -> V3 {
	[a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn sub(a: V3, b: V3) -> V3 {
	[a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn scale3(a: V3, k: f32) -> V3 {
	[a[0] * k, a[1] * k, a[2] * k]
}
pub fn dot3(a: V3, b: V3) -> f32 {
	a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn cross3(a: V3, b: V3) -> V3 {
	[a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
pub fn len3(a: V3) -> f32 {
	dot3(a, a).sqrt()
}
pub fn unit3(a: V3) -> V3 {
	let l = len3(a);
	if l < 1e-9 { [1.0, 0.0, 0.0] } else { scale3(a, 1.0 / l) }
}

/// An angle wrapped to -pi..pi.
pub fn wrap(a: f32) -> f32 {
	let t = std::f32::consts::TAU;
	let mut a = a % t;
	if a > std::f32::consts::PI {
		a -= t;
	} else if a < -std::f32::consts::PI {
		a += t;
	}
	a
}

/// Camera rows for a view yaw / pitch (pitch > 0 looks down): row 0 forward, row 1 left, row 2 up.
pub fn rows(yaw: f32, pitch: f32) -> Rows {
	let (sy, cy) = yaw.fsin_cos();
	let (sp, cp) = pitch.fsin_cos();
	[[cy * cp, sy * cp, -sp], [-sy, cy, 0.0], [cy * sp, sy * sp, cp]]
}

/// The eye for a pivot, view yaw / pitch and boom length: behind the pivot along the view direction.
pub fn eye(pivot: V3, yaw: f32, pitch: f32, len: f32) -> V3 {
	let f = rows(yaw, pitch)[0];
	[pivot[0] - f[0] * len, pivot[1] - f[1] * len, pivot[2] - f[2] * len]
}

/// Yaw / pitch of camera rows (the inverse of `rows` for an unrolled view).
pub fn angles(r: &Rows) -> (f32, f32) {
	(r[0][1].fatan2(r[0][0]), (-r[0][2]).clamp(-1.0, 1.0).fasin())
}

// ---- turn (R&C1 modcam_turn.rs) --------------------------------------------------------------------------------------------

/// Flips one stick byte around the centre 0x80 (0x01 <-> 0xff; 0x00 -> 0xff, the PS2 range has no 0x100).
fn flip(b: u8) -> u8 {
	(256 - b as i32).clamp(1, 255) as u8
}

/// The right stick bytes [rx, ry] the game gets with the Modern camera on, so that the controller's stick has the mouse's sign
/// (retail's follow camera reads it the other way round): `merged` = keyboard and controller merged by the larger deflection,
/// `keys` = the keys' share (offset from 0x80, 0 = no key held), `class4` = retail runs its free look, `lr_reversed` = the
/// Left/Right option is Reversed. An axis the keys drive is not touched (the keys keep their old direction).
pub fn camera_sticks(merged: [u8; 2], keys: [i8; 2], class4: bool, lr_reversed: bool) -> [u8; 2] {
	let from_keys = |i: usize| keys[i] != 0 && merged[i] as i16 - 128 == keys[i] as i16;
	let rx = if from_keys(0) || (class4 && !lr_reversed) { merged[0] } else { flip(merged[0]) };
	let ry = if from_keys(1) { merged[1] } else { flip(merged[1]) };
	[rx, ry]
}

/// The right stick as the turn maths takes it, in `[-1, 1]` (PS2 bytes: right and down are high), with the game's own dead zone
/// (`d = |byte - 0x7f|`, 0 up to 47, else `(d - 48) / 76` up to 1); the keys' bytes 0x01 / 0xff give exactly -1 / 1.
pub fn stick_axes(merged: [u8; 2]) -> [f32; 2] {
	merged.map(|b| {
		let d = (b as i32 - 0x7f).abs();
		let v = if d <= 0x2f { 0.0 } else { ((d - 0x30) as f32 / 0x4c as f32).min(1.0) };
		if b < 0x7f { -v } else { v }
	})
}

/// The per-field gains of the turn: the mouse (counts -> radians, `sens`, the Mouse speed row's factor), the stick (yaw rate, pitch
/// step, the Camera speed row's factor) and the Reversed options (-1.0 Reversed, 1.0 Normal) per axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gains {
	pub mouse: f32,
	pub sens: f32,
	pub mouse_k: f32,
	pub rot: f32,
	pub tilt: f32,
	pub stick_k: f32,
	pub lr: f32,
	pub ud: f32,
	/// The mouse's own signs (`modcam_mouseinv::signs`, R&C1's `mouse_lr` / `mouse_ud`); equal to `lr` / `ud` = Like camera.
	pub mouse_lr: f32,
	pub mouse_ud: f32,
}

impl Gains {
	/// The standard gains: `mouse_k` / `stick_k` = Mouse speed / Camera speed percent / 50 (`scale`), `lr_rev` / `ud_rev` = Reversed.
	/// The mouse follows the Reversed options (Like camera); `with_mouse_signs` sets its own.
	pub fn new(mouse_k: f32, stick_k: f32, lr_rev: bool, ud_rev: bool) -> Gains {
		let (lr, ud) = (if lr_rev { -1.0 } else { 1.0 }, if ud_rev { -1.0 } else { 1.0 });
		Gains { mouse: MOUSE, sens: 1.0, mouse_k, rot: ROTATE_RATE, tilt: TILT_STEP, stick_k, lr, ud, mouse_lr: lr, mouse_ud: ud }
	}

	/// The mouse's own (left/right, up/down) signs (+-1.0).
	pub fn with_mouse_signs(self, (mouse_lr, mouse_ud): (f32, f32)) -> Gains {
		Gains { mouse_lr, mouse_ud, ..self }
	}
}

/// A speed row in percent (10..200, 50 = the default feel) as a factor.
pub fn scale(percent: Option<u8>) -> f32 {
	percent.unwrap_or(50) as f32 / 50.0
}

/// The field's turn (yaw, pitch) in radians from the mouse counts `look` and the stick `s` (`stick_axes`, after the deadzone and the
/// Smooth ramp).
pub fn turn(look: [i16; 2], s: [f32; 2], g: &Gains) -> (f32, f32) {
	// The mouse's share is flipped by `mouse * retail` (both +-1, R&C1 modcam_turn.rs): exactly 1.0 for Like camera.
	let (flip_x, flip_y) = (g.mouse_lr * g.lr, g.mouse_ud * g.ud);
	let dyaw = (-(look[0] as f32) * g.mouse * g.sens * g.mouse_k * flip_x + s[0] * g.rot * g.stick_k) * g.lr;
	let dpitch = (look[1] as f32 * g.mouse * g.sens * g.mouse_k * flip_y - s[1] * g.tilt * g.stick_k) * g.ud;
	(dyaw, dpitch)
}

// ---- springs ---------------------------------------------------------------------------------------------------------------

fn sstep(x: f32) -> f32 {
	let x = x.clamp(0.0, 1.0);
	x * x * (3.0 - 2.0 * x)
}

/// The share of the way to a goal one field (`dt` s) covers with a first-order lag of `tau` seconds.
pub fn k_of(dt: f32, tau: f32) -> f32 {
	if tau <= 1e-4 { 1.0 } else { 1.0 - (-dt / tau).fexp() }
}

/// One field of a critically damped spring on the offset `y` (velocity `v`) from its goal, smoothing time `ts` (w = 2 / ts, the exact
/// solution, so no overshoot and no step-size trouble): (y, v) afterwards.
pub fn crit(dt: f32, y: f32, v: f32, ts: f32) -> (f32, f32) {
	let w = 2.0 / ts.max(0.02);
	let e = (-w * dt).fexp();
	let t = v + w * y;
	((y + t * dt) * e, (v - w * t * dt) * e)
}

/// The Smooth camera's constants (R&C1 `Tune`, the lock-on framing and drop probe left out).
#[derive(Clone, Copy, Debug)]
pub struct Tune {
	/// Pivot spring smoothing times (s): horizontal, vertical.
	pub h_time: f32,
	pub v_time: f32,
	/// Share of a jump's rise the pivot follows before landing (falls below the takeoff follow fully).
	pub air_follow: f32,
	/// Leash (m): the pivot never lags the hero's chest by more than this sideways, with the hero above it, or with it above him.
	pub leash_h: f32,
	pub leash_up: f32,
	pub leash_dn: f32,
	/// Smoothing of the hero velocity estimate (s).
	pub vel_time: f32,
	/// Recentre: idle seconds before it starts, ease-in time, rate (1/s) base and extra at 90 deg sideways, the highest turn rate
	/// (rad/s), the lowest run speed (m/s), the most it swings per episode (rad).
	pub rc_delay: f32,
	pub rc_ramp: f32,
	pub rc_base: f32,
	pub rc_side: f32,
	pub rc_max: f32,
	pub rc_speed: f32,
	pub rc_budget: f32,
	/// Default elevation (rad, + looks down), the dead zone around it that is left alone (rad), its ease rate (1/s), the lowest speed.
	pub pitch_def: f32,
	pub pitch_dz: f32,
	pub pitch_k: f32,
	pub pitch_speed: f32,
	/// Look-up on slopes (rad per unit slope, most), look-down while falling (rad), bias in / out times (s).
	pub bias_up_k: f32,
	pub bias_up_max: f32,
	pub bias_fall: f32,
	pub bias_in: f32,
	pub bias_out: f32,
	/// Boom length: extra share at run speed, factor in tight / open space, smoothing (s), probe length (m), ease-out time (s).
	pub dist_run: f32,
	pub dist_open_min: f32,
	pub dist_open_max: f32,
	pub dist_tau: f32,
	pub open_ray: f32,
	pub boom_out: f32,
	/// Right stick ramp: rise and fall rate (full deflection per second).
	pub ramp_up: f32,
	pub ramp_dn: f32,
	/// The margin kept off a surface (m) by the wall tests.
	pub safe_margin: f32,
}

impl Default for Tune {
	fn default() -> Tune {
		Tune {
			h_time: 0.10,
			v_time: 0.28,
			air_follow: 0.35,
			leash_h: 1.6,
			leash_up: 1.5,
			leash_dn: 1.0,
			vel_time: 0.12,
			rc_delay: 1.2,
			rc_ramp: 0.5,
			rc_base: 0.35,
			rc_side: 1.6,
			rc_max: 0.9,
			rc_speed: 3.0,
			rc_budget: 1.3,
			pitch_def: 0.33,
			pitch_dz: 0.10,
			pitch_k: 0.6,
			pitch_speed: 1.5,
			bias_up_k: 0.30,
			bias_up_max: 0.12,
			bias_fall: 0.22,
			bias_in: 0.35,
			bias_out: 0.5,
			dist_run: 0.10,
			dist_open_min: 0.82,
			dist_open_max: 1.10,
			dist_tau: 0.6,
			open_ray: 7.0,
			boom_out: 0.2,
			ramp_up: 4.0,
			ramp_dn: 12.0,
			safe_margin: 0.2,
		}
	}
}

impl Tune {
	/// The defaults with `RAC_TUNE="mc_h_time=0.12,mc_rc_delay=2"` applied (engine devtune, read once; names are `mc_` + the field).
	pub fn from_env() -> Tune {
		let d = Tune::default();
		let f = |n: &str, v: f32| rac_engine::devtune::f32(&format!("mc_{n}"), v);
		Tune {
			h_time: f("h_time", d.h_time),
			v_time: f("v_time", d.v_time),
			air_follow: f("air_follow", d.air_follow),
			leash_h: f("leash_h", d.leash_h),
			leash_up: f("leash_up", d.leash_up),
			leash_dn: f("leash_dn", d.leash_dn),
			vel_time: f("vel_time", d.vel_time),
			rc_delay: f("rc_delay", d.rc_delay),
			rc_ramp: f("rc_ramp", d.rc_ramp),
			rc_base: f("rc_base", d.rc_base),
			rc_side: f("rc_side", d.rc_side),
			rc_max: f("rc_max", d.rc_max),
			rc_speed: f("rc_speed", d.rc_speed),
			rc_budget: f("rc_budget", d.rc_budget),
			pitch_def: f("pitch_def", d.pitch_def),
			pitch_dz: f("pitch_dz", d.pitch_dz),
			pitch_k: f("pitch_k", d.pitch_k),
			pitch_speed: f("pitch_speed", d.pitch_speed),
			bias_up_k: f("bias_up_k", d.bias_up_k),
			bias_up_max: f("bias_up_max", d.bias_up_max),
			bias_fall: f("bias_fall", d.bias_fall),
			bias_in: f("bias_in", d.bias_in),
			bias_out: f("bias_out", d.bias_out),
			dist_run: f("dist_run", d.dist_run),
			dist_open_min: f("dist_open_min", d.dist_open_min),
			dist_open_max: f("dist_open_max", d.dist_open_max),
			dist_tau: f("dist_tau", d.dist_tau),
			open_ray: f("open_ray", d.open_ray),
			boom_out: f("boom_out", d.boom_out),
			ramp_up: f("ramp_up", d.ramp_up),
			ramp_dn: f("ramp_dn", d.ramp_dn),
			safe_margin: f("safe_margin", d.safe_margin),
		}
	}
}

/// The Smooth layer's per-camera state.
#[derive(Clone, Copy, Debug)]
struct Dy {
	/// The field `track` last ran in (a gap re-seeds the velocity) and the hero position then.
	field: u32,
	hero_prev: V3,
	/// Smoothed hero velocity (m/s, world) and the pivot spring's velocity.
	hv: V3,
	pv: V3,
	/// Fields without camera input.
	idle: u32,
	/// Fields in a row the hero was airborne; `is_air` = at least 2 (debounce).
	air: u32,
	is_air: bool,
	/// At the last grounded field: the hero goal's and the pivot's height (the takeoff reference).
	ref_h: f32,
	ref_p: f32,
	/// Horizontal speed and vertical speed (m/s, smoothed).
	speed: f32,
	vz: f32,
	/// Recentre turned so far this episode (rad).
	rc_used: f32,
	/// Pitch bias (rad, + looks down).
	bias: f32,
	/// Free share (0..1) of the left, right and up probes, the boom length factor, the boom's ease-out velocity.
	open_s: [f32; 3],
	dist_f: f32,
	len_v: f32,
	/// The ramped right stick.
	rs: [f32; 2],
	/// The pivot the camera orbits this field: the spring's pivot clipped at walls and floors between it and the hero's chest.
	pivc: V3,
}

impl Dy {
	const ZERO: Dy = Dy { field: 0, hero_prev: [0.0; 3], hv: [0.0; 3], pv: [0.0; 3], idle: 0, air: 0, is_air: false, ref_h: 0.0, ref_p: 0.0, speed: 0.0, vz: 0.0, rc_used: 0.0, bias: 0.0, open_s: [1.0; 3], dist_f: 1.0, len_v: 0.0, rs: [0.0; 2], pivc: [0.0; 3] };
}

// ---- the camera --------------------------------------------------------------------------------------------------------------

/// The game's segment test: the first contact of the segment `a` -> `b` with the level, if any. The implementation must leave the
/// game exactly as it was (stage B: save / restore of everything the retail routine touches).
pub trait Probe {
	fn seg(&mut self, a: V3, b: V3) -> Option<V3>;
}

/// A probe that never hits (open space; tests).
pub struct NoWalls;

impl Probe for NoWalls {
	fn seg(&mut self, _: V3, _: V3) -> Option<V3> {
		None
	}
}

/// A camera pose: the eye and the rows (0 forward, 1 left, 2 up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
	pub eye: V3,
	pub rows: Rows,
}

/// What stays the same for a camera.
#[derive(Clone, Copy, Debug)]
pub struct Config {
	/// Seconds a field is (PAL 0.02).
	pub dt: f32,
	/// The boom length (m) the player starts with, after the seed; the wheel changes it from there.
	pub dist: f32,
	/// The Smooth camera's boom breathes with the open space around the pivot, from three segment tests per three fields (off: speed only).
	pub open_probe: bool,
	pub tune: Tune,
}

impl Config {
	pub fn new(dt: f32, dist: f32) -> Config {
		Config { dt, dist, open_probe: false, tune: Tune::default() }
	}
}

/// What one field gives the camera.
#[derive(Clone, Copy, Debug)]
pub struct PoseIn {
	/// A counter that grows by one per game field (a gap means the camera did not run: Smooth re-seeds its velocity).
	pub field: u32,
	/// The hero's position (m, at his feet; the pivot is `PIVOT` above).
	pub hero: V3,
	/// The hero is in a jump / fall without ground contact (Smooth: look-down while falling, the pivot follows part of a jump).
	pub airborne: bool,
	pub input: Input,
	/// The right stick bytes as the game gets them (after `camera_sticks`).
	pub rstick: [u8; 2],
	pub gains: Gains,
	/// Smooth (true) or Static (false); a change re-seeds the springs.
	pub smooth: bool,
	/// Retail's pose this field (what the follow class update left in the record).
	pub retail: Pose,
	/// Retail drives this field (a gated hero state): the pose blends back to `retail` over `BLEND` fields, then `step` gives none.
	pub retail_drives: bool,
	/// The hero's smoothed up (`modcam_frame::read_up`): world z on flat ground, away from it on a magnetic wall.
	pub up: V3,
}

impl PoseIn {
	/// An idle field: no input, a centred stick, Static, speeds at 1.0, retail not driving.
	pub fn idle(field: u32, hero: V3, retail: Pose) -> PoseIn {
		PoseIn { field, hero, airborne: false, input: Input::default(), rstick: [0x80; 2], gains: Gains::new(1.0, 1.0, false, false), smooth: false, retail, retail_drives: false, up: UP }
	}
}

/// The Modern camera of one game.
pub struct Cam {
	cfg: Config,
	field: u32,
	live: bool,
	seen: bool,
	yaw: f32,
	pitch: f32,
	/// The boom length the player set (the wheel) and the one in force after the seed's ease.
	dist_goal: f32,
	dist: f32,
	/// The boom length in force (after the wall tests).
	len: f32,
	/// Extra pitch while a wall behind the hero leaves the boom too short, its held goal and its spring velocity.
	rise: f32,
	rise_goal: f32,
	rise_v: f32,
	pivot: V3,
	/// Last pose written (eye, yaw, pitch), its rows, and fields of blend-out left.
	last: (V3, f32, f32),
	last_rows: Rows,
	blend: u32,
	hero_last: V3,
	/// The orbit frame of the last field (world frame on flat ground).
	fr: Frame,
	aiming: bool,
	/// The strafe-aim lift of the pivot (`modcam_shoulder.rs`).
	lift: Lift,
	dy: Dy,
	dyn_was: bool,
	/// Trace of the last wall test: base reach, safe reach, rise goal.
	tr: [f32; 3],
	/// (yaw, pitch, boom goal) a savestate carried: the next seed uses it instead of retail's pose (`preset`).
	preset: Option<(f32, f32, f32)>,
	/// The whole camera a savestate carried (`snap.rs`): laid over the next seed.
	full: Option<Vec<u32>>,
	/// The take-over's view offset (`modcam_look.rs`): the first pose after a seed shows retail's view, then the orbit's.
	look: LookOff,
	/// Poses written since the last seed that took retail's pose (`fresh`); a seed from a saved camera starts past it.
	age: u32,
	/// A seed now is a cold start (`set_cold`): no look offset, the orbit's rows at once.
	cold: bool,
	/// The field of the last seed since `reset` (`seeded`).
	seeded: Option<u32>,
}

impl Cam {
	pub fn new(cfg: Config) -> Cam {
		Cam {
			cfg,
			field: 0,
			live: false,
			seen: false,
			yaw: 0.0,
			pitch: 0.35,
			dist_goal: cfg.dist,
			dist: cfg.dist,
			len: cfg.dist,
			rise: 0.0,
			rise_goal: 0.0,
			rise_v: 0.0,
			pivot: [0.0; 3],
			last: ([0.0; 3], 0.0, 0.0),
			last_rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], UP],
			blend: 0,
			hero_last: [0.0; 3],
			fr: FLAT,
			aiming: false,
			lift: Lift::ZERO,
			dy: Dy::ZERO,
			dyn_was: false,
			tr: [0.0; 3],
			preset: None,
			full: None,
			look: LookOff::NONE,
			age: 0,
			cold: false,
			seeded: None,
		}
	}

	/// The caller's word on the next seed: a cold start (the level's first seed: a load, the end of a level's intro; `modcam_hand`
	/// `COLD`) seeds with the orbit's rows at once, as build 43 did, so the game sees the same camera as there [C fg1 0357, no input:
	/// the look fade's rows in fields 2-13 changed retail's per-field rand() count from field 2 on, and with it Ratchet's idle
	/// animations]; else the take-over fades from retail's view (`modcam_look`).
	pub fn set_cold(&mut self, cold: bool) {
		self.cold = cold;
	}

	/// The field of the last seed since `reset` (any kind: retail's pose, a saved camera).
	pub fn seeded(&self) -> Option<u32> {
		self.seeded
	}

	/// A quick load: the next seed starts from this yaw, pitch and boom (a `state()` of the saved game) instead of the pose retail
	/// has after the load, which turns the camera behind the hero. Survives `reset`; a non-finite value is ignored.
	fn preset(&mut self, v: [f32; 3]) {
		if v.iter().all(|x| x.is_finite()) {
			self.preset = Some((v[0], v[1].clamp(PITCH.0, PITCH.1), v[2].clamp(ZOOM.0, ZOOM.1)));
		}
	}

	/// A new level / a load: forget everything (the next field seeds from retail's pose, no blend).
	pub fn reset(&mut self) {
		self.live = false;
		self.seen = false;
		self.blend = 0;
		self.aiming = false;
		self.lift.reset();
		self.look.clear();
		self.seeded = None;
	}

	/// Retail ran another camera class meanwhile (the follow update did not run): the next field seeds again.
	pub fn end(&mut self) {
		self.live = false;
		self.blend = 0;
		self.aiming = false;
		self.lift.reset();
		self.look.clear();
	}

	/// The followed character changed (`modcam_who`: Ratchet <-> Clank): its position is the new reference, so the step to it is no
	/// teleport re-seed (`JUMP`); the caller hands back to retail's pose for the change (`PoseIn::retail_drives`), which blends.
	pub fn retarget(&mut self, hero: V3) {
		self.hero_last = hero;
	}

	/// The blend back to retail's pose is running (`PoseIn::retail_drives` started it): the caller keeps handing back until it is over.
	pub fn blending(&self) -> bool {
		self.blend > 0
	}

	/// The only pose written since the seed is the seed's own (retail's pose of that field): nothing Modern was on screen yet, so a
	/// retail rewrite now is retail placing its camera, not a hand-back (`modcam_blend::Why::Placed`).
	pub fn fresh(&self) -> bool {
		self.live && self.age == 1
	}

	/// The strafe-aim lift in force (m).
	pub fn lift(&self) -> f32 {
		self.lift.value()
	}

	pub fn live(&self) -> bool {
		self.live
	}
	pub fn yaw(&self) -> f32 {
		self.yaw
	}
	pub fn pitch(&self) -> f32 {
		self.pitch
	}
	pub fn len(&self) -> f32 {
		self.len
	}
	pub fn pivot(&self) -> V3 {
		self.pivot
	}
	pub fn aiming(&self) -> bool {
		self.aiming
	}
	/// The last wall test: base reach, safe reach, rise goal (m, m, rad).
	pub fn trace(&self) -> [f32; 3] {
		self.tr
	}

	/// One field. `Some(pose)` = write it into the camera record; `None` = leave retail's pose.
	pub fn step(&mut self, i: &PoseIn, p: &mut dyn Probe) -> Option<Pose> {
		self.field = i.field;
		let jumped = self.seen && len3(sub(i.hero, self.hero_last)) > JUMP;
		self.seen = true;
		self.hero_last = i.hero;
		if jumped {
			self.blend = 0;
			self.live = false;
		}
		if i.retail_drives {
			self.aiming = false;
			self.lift.reset();
			return self.hand_back(&i.retail);
		}
		// Orbit frame from the hero's smoothed up (flat ground: the world frame, nothing below changes).
		let (fr, turn) = Frame::carry(if self.live { self.fr.a } else { FLAT.a }, i.up);
		self.yaw = wrap(self.yaw + turn);
		self.fr = fr;
		let piv_goal = fr.lift(i.hero, PIVOT);
		if !self.live {
			self.seed(i, piv_goal);
		}
		let smooth = i.smooth;
		if smooth != self.dyn_was {
			// Static <-> Smooth flipped: the springs start afresh from the pose on screen (the pivot is shared, so the eye does not
			// jump); leaving Smooth keeps the look-up / look-down bias it had in the pitch.
			if !smooth && self.dy.bias != 0.0 {
				self.pitch = (self.pitch + self.dy.bias).clamp(PITCH.0, PITCH.1);
			}
			self.reseed_dy(i.hero);
			self.dyn_was = smooth;
		}
		self.turn_input(i, piv_goal);
		self.move_pivot(i, piv_goal);
		let (pivot, goal_len) = self.boom_goal(smooth, p);
		let pivot = self.lifted(pivot, p);
		let pose = self.boom(pivot, goal_len, smooth, p);
		self.age = self.age.saturating_add(1);
		Some(pose)
	}

	/// The strafe aim raises the pivot (`modcam_shoulder.rs`): the ceiling over it is the game's own segment test straight up, run only
	/// while the lift exists. The wall pull-in and the rise logic of `boom` then work from the lifted pivot.
	fn lifted(&mut self, pivot: V3, p: &mut dyn Probe) -> V3 {
		let fr = self.fr;
		let ceiling = if self.lift.wants_ceiling(self.aiming) {
			p.seg(pivot, fr.lift(pivot, AIM_LIFT + AIM_CEIL)).map(|h| fr.along(sub(h, pivot)))
		} else {
			None
		};
		let up = self.lift.step(self.aiming, ceiling);
		fr.lift(pivot, up)
	}

	fn reseed_dy(&mut self, hero: V3) {
		self.dy = Dy::ZERO;
		self.dy.hero_prev = hero;
		self.dy.field = self.field.wrapping_sub(1);
	}

	/// Seeds yaw, pitch and boom from the pose on screen (retail's, or the blend-out in progress): no snap.
	fn seed(&mut self, i: &PoseIn, piv_goal: V3) {
		let (e, shown) = if self.blend > 0 { (self.last.0, Some((self.last.1, self.last.2))) } else { (i.retail.eye, view_of(&i.retail.rows)) };
		// The view on screen versus the orbit's (R&C1 `LookOff`): only in the world frame (a tilted frame's angles are not world angles),
		// and not on a cold start (`set_cold`).
		self.look.seed(shown.filter(|_| self.fr.flat() && !self.cold));
		self.seeded = Some(i.field);
		let d = sub(e, piv_goal);
		let l = len3(d).max(MIN_LEN);
		let (dx, dy) = self.fr.xy(d);
		self.yaw = (-dy).fatan2(-dx);
		self.pitch = (self.fr.along(d) / l).clamp(-1.0, 1.0).fasin().clamp(PITCH.0, PITCH.1);
		self.len = l.min(ZOOM.1);
		self.dist = self.len.clamp(ZOOM.0, ZOOM.1);
		self.age = 0;
		if let Some((y, p, g)) = self.preset.take() {
			(self.yaw, self.pitch, self.dist_goal, self.dist, self.len) = (y, p, g, g, g);
			self.look.clear();
			self.age = 2;
		}
		self.pivot = piv_goal;
		(self.rise, self.rise_v, self.rise_goal) = (0.0, 0.0, 0.0);
		self.lift.reset();
		self.live = true;
		self.blend = 0;
		self.reseed_dy(i.hero);
		self.dyn_was = i.smooth;
		if let Some(w) = self.full.take() {
			self.apply_full(&w);
			self.look.clear();
			self.age = 2;
		}
	}

	/// Retail drives: the blend from the last modern pose to retail's (smoothstep over `BLEND` fields), then none.
	fn hand_back(&mut self, retail: &Pose) -> Option<Pose> {
		self.look.clear();
		if self.live {
			self.live = false;
			self.blend = BLEND;
		}
		if self.blend == 0 {
			return None;
		}
		self.blend -= 1;
		let t = sstep(1.0 - self.blend as f32 / BLEND as f32);
		let (e0, y0, p0) = self.last;
		let e = [e0[0] + (retail.eye[0] - e0[0]) * t, e0[1] + (retail.eye[1] - e0[1]) * t, e0[2] + (retail.eye[2] - e0[2]) * t];
		let rr = retail.rows;
		if self.fr.flat() && rr[1][2].abs() < 1e-4 {
			let (ry, rp) = angles(&rr);
			let (y, p) = (y0 + wrap(ry - y0) * t, p0 + (rp - p0) * t);
			self.last_rows = rows(y, p);
			self.last = (e, y, p);
		} else {
			// A rolled retail view: blend the rows themselves and keep them orthonormal.
			let l = self.last_rows;
			let mix = |i: usize| add3(l[i], scale3(sub(rr[i], l[i]), t));
			let f = unit3(mix(0));
			let m2 = mix(2);
			let u = unit3(sub(m2, scale3(f, dot3(m2, f))));
			self.last_rows = [f, cross3(u, f), u];
			self.last = (e, f[1].fatan2(f[0]), (-f[2]).clamp(-1.0, 1.0).fasin());
		}
		Some(Pose { eye: e, rows: self.last_rows })
	}

	/// Mouse, right stick and wheel: yaw, pitch and the boom the player set; Smooth also tracks the hero and recentres.
	fn turn_input(&mut self, i: &PoseIn, piv_goal: V3) {
		let (t, dt) = (self.cfg.tune, self.cfg.dt);
		let inp = i.input;
		let rs = stick_axes(i.rstick);
		// Smooth: the mouse stays direct, the right stick ramps up (keys Q/E Z/X ramp too).
		let (sx, sy) = if i.smooth {
			self.dy.rs = [ramp1(&t, dt, self.dy.rs[0], rs[0]), ramp1(&t, dt, self.dy.rs[1], rs[1])];
			(self.dy.rs[0], self.dy.rs[1])
		} else {
			(rs[0], rs[1])
		};
		let (dyaw, dpitch) = turn(inp.look, [sx, sy], &i.gains);
		self.dist_goal = (self.dist_goal - inp.zoom as f32 * WHEEL_STEP).clamp(ZOOM.0, ZOOM.1);
		self.dist += (self.dist_goal - self.dist) * k_of(dt, DIST_TAU);
		self.dist = self.dist.clamp(ZOOM.0, ZOOM.1);
		// TODO [?]: not gated by the hero state (no R&C2 state word known yet, see modcam_shoulder.rs): L2/R2 lifts in every follow state.
		// Aim is a ground feature: on a magnetic wall the strafe's facing words are world-z [?], so no aim lift there (as R&C1 stows the guns).
		self.aiming = inp.strafe && self.fr.flat();
		self.yaw = wrap(self.yaw + dyaw);
		self.pitch = (self.pitch + dpitch).clamp(PITCH.0, PITCH.1);
		if i.smooth {
			let used = inp.look != [0, 0] || rs != [0.0, 0.0] || inp.zoom != 0 || inp.strafe;
			self.track(i.hero, piv_goal, i.airborne, used);
			if !self.aiming {
				self.auto();
				self.pitch = self.pitch.clamp(PITCH.0, PITCH.1);
			}
		}
	}

	/// The pivot follows the hero: Smooth on springs (not while aiming: the exponential follow keeps that feel), Static exponential.
	fn move_pivot(&mut self, i: &PoseIn, piv_goal: V3) {
		if i.smooth && !self.aiming {
			self.follow_spring(piv_goal);
		} else {
			let old = self.pivot;
			for k in 0..3 {
				self.pivot[k] += (piv_goal[k] - self.pivot[k]) * FOLLOW;
			}
			if i.smooth {
				// The aim follow is exponential; the spring takes over from the velocity it left.
				self.dy.pv = scale3(sub(self.pivot, old), 1.0 / self.cfg.dt);
			}
		}
		// Big hero jumps (respawn, teleport): follow at once.
		if len3(sub(piv_goal, self.pivot)) > 8.0 {
			self.pivot = piv_goal;
			self.dy.pv = [0.0; 3];
		}
	}

	/// Every field, before the camera moves: the hero's velocity, jump / fall state, the idle time (`input` = the camera was used).
	fn track(&mut self, hero: V3, hero_goal: V3, airborne: bool, input: bool) {
		let (t, dt, field) = (self.cfg.tune, self.cfg.dt, self.field);
		let fr = self.fr;
		let piv_h = fr.along(self.pivot);
		let d = &mut self.dy;
		let dp = sub(hero, d.hero_prev);
		let gap = field.wrapping_sub(d.field) != 1 || len3(dp) > JUMP;
		d.field = field;
		d.hero_prev = hero;
		if gap {
			d.hv = [0.0; 3];
		} else {
			let a = k_of(dt, t.vel_time);
			for k in 0..3 {
				d.hv[k] += (dp[k] / dt - d.hv[k]) * a;
			}
		}
		let (vx, vy) = fr.xy(d.hv);
		d.speed = vx.fhypot(vy);
		d.vz = fr.along(d.hv);
		d.air = if airborne { d.air.saturating_add(1) } else { 0 };
		d.is_air = d.air >= 2;
		if d.air == 0 {
			d.ref_h = fr.along(hero_goal);
			d.ref_p = piv_h;
		}
		d.idle = if input { 0 } else { d.idle.saturating_add(1) };
		if input || d.speed < t.rc_speed * 0.5 {
			d.rc_used = 0.0;
		}
	}

	/// The pivot follows the hero goal (chest) through critically damped springs: fast sideways, slower vertically; while a jump is in
	/// the air the vertical goal follows only `air_follow` of the rise above the takeoff (falls below it follow fully); a leash keeps
	/// the hero within `leash_*` of the pivot.
	fn follow_spring(&mut self, hero_goal: V3) {
		let (t, dt) = (self.cfg.tune, self.cfg.dt);
		let fr = self.fr;
		let d = &mut self.dy;
		let hh = fr.along(hero_goal);
		let mut goal = hero_goal;
		if d.is_air {
			let off = hh - d.ref_h;
			let gh = d.ref_p + if off > 0.0 { off * t.air_follow } else { off };
			goal = fr.lift(goal, gh - hh);
		}
		let y = sub(self.pivot, goal);
		let (yv, vv) = (fr.along(y), fr.along(d.pv));
		let (yh, vh) = (fr.across(y), fr.across(d.pv));
		let (mut nyh, mut nvh) = ([0.0f32; 3], [0.0f32; 3]);
		for k in 0..3 {
			if fr.flat() && k == 2 {
				break;
			}
			(nyh[k], nvh[k]) = crit(dt, yh[k], vh[k], t.h_time);
		}
		let (nyv, nvv) = crit(dt, yv, vv, t.v_time);
		let mut pv = fr.lift(nvh, nvv);
		// Leash against the real hero (not the partial jump goal): never lose him.
		let e = add3(sub(goal, hero_goal), fr.lift(nyh, nyv));
		let ev = fr.along(e);
		let mut eh = fr.across(e);
		let mut ev2 = ev;
		let hvv = fr.along(d.hv);
		let lh = len3(eh);
		if lh > t.leash_h {
			eh = scale3(eh, t.leash_h / lh);
			pv = fr.lift(fr.across(d.hv), fr.along(pv));
		}
		if ev > t.leash_dn || ev < -t.leash_up {
			ev2 = ev.clamp(-t.leash_up, t.leash_dn);
			pv = fr.lift(fr.across(pv), hvv);
		}
		self.pivot = add3(hero_goal, fr.lift(eh, ev2));
		d.pv = pv;
	}

	/// Lazy recentre and default elevation (no camera input for `rc_delay` s, the hero running, no aim): the yaw eases toward the run
	/// direction, faster the more sideways he runs, not at all toward the camera, at most `rc_budget` per episode; the pitch eases
	/// toward `pitch_def`.
	fn auto(&mut self) {
		let (t, dt) = (self.cfg.tune, self.cfg.dt);
		let idle_s = self.dy.idle as f32 * dt;
		let sp = self.dy.speed;
		if idle_s < t.rc_delay {
			return;
		}
		let ease = sstep((idle_s - t.rc_delay) / t.rc_ramp);
		if sp >= t.rc_speed && self.dy.rc_used < t.rc_budget {
			let (vx, vy) = self.fr.xy(self.dy.hv);
			let ang = wrap(vy.fatan2(vx) - self.yaw);
			// Running toward the camera (more than about 110 deg off the view direction): no swing.
			let gate = sstep((ang.fcos() + 0.5) / 0.3);
			// The swing tapers off over the last 0.4 rad of its budget (no sudden stop).
			let taper = sstep((t.rc_budget - self.dy.rc_used) / 0.4);
			let rate = (t.rc_base + t.rc_side * ang.fsin().abs()) * gate * ease * sstep((sp - t.rc_speed) / 2.0) * taper;
			let lim = t.rc_max * dt;
			let turn = (ang * (1.0 - (-rate * dt).fexp())).clamp(-lim, lim);
			self.yaw = wrap(self.yaw + turn);
			self.dy.rc_used += turn.abs();
		}
		if sp >= t.pitch_speed {
			// Only the part outside the dead zone around the default pulls (a pitch the player chose nearby stays).
			let err = t.pitch_def - self.pitch;
			self.pitch += (err - err.clamp(-t.pitch_dz, t.pitch_dz)) * k_of(dt, 1.0 / t.pitch_k) * ease;
		}
	}

	/// The look-up / look-down bias: a little up on uphill runs, down while falling. Eased, never touches the player's own pitch;
	/// fades fast while aiming.
	fn bias(&mut self) {
		let (t, dt) = (self.cfg.tune, self.cfg.dt);
		let aiming = self.aiming;
		let d = &mut self.dy;
		let mut g = 0.0f32;
		if !aiming {
			if !d.is_air && d.speed > 1.5 && d.vz > 0.0 {
				g -= (d.vz / d.speed * t.bias_up_k).min(t.bias_up_max);
			}
			if d.is_air && d.vz < -3.0 {
				g += t.bias_fall * ((-d.vz - 3.0) / 10.0).clamp(0.0, 1.0);
			}
		}
		let tau = if aiming { 0.12 } else if g.abs() > d.bias.abs() { t.bias_in } else { t.bias_out };
		d.bias += (g - d.bias) * k_of(dt, tau);
	}

	/// The boom length factor: a bit longer at run speed and in open space, shorter in tight spaces; 1.0 (fast) while aiming.
	fn dist_factor(&mut self) {
		let (t, dt) = (self.cfg.tune, self.cfg.dt);
		let s = self.dy.open_s;
		let f_open = if self.cfg.open_probe { t.dist_open_min + (t.dist_open_max - t.dist_open_min) * (0.45 * s[0] + 0.45 * s[1] + 0.10 * s[2]) } else { 1.0 };
		let f_run = 1.0 + t.dist_run * (self.dy.speed / 6.0).clamp(0.0, 1.0);
		let (goal, tau) = if self.aiming { (1.0, 0.2) } else { (f_open * f_run, t.dist_tau) };
		self.dy.dist_f += (goal - self.dy.dist_f) * k_of(dt, tau);
	}

	/// How open the space around the pivot is, one direction per field (left, right, up): the free share of an `open_ray` segment.
	fn open_probe(&mut self, p: &mut dyn Probe) {
		if self.aiming {
			return;
		}
		let o = self.dy.pivc;
		let r = self.fr.rows(self.yaw, self.pitch);
		let k = (self.field % 3) as usize;
		let dir = [r[1], scale3(r[1], -1.0), self.fr.u][k];
		let ray = self.cfg.tune.open_ray;
		self.dy.open_s[k] = match p.seg(o, add3(o, scale3(dir, ray))) {
			Some(h) => (len3(sub(h, o)) / ray).min(1.0),
			None => 1.0,
		};
	}

	/// The pivot the camera orbits and the boom length it wants. Smooth: the spring's pivot clipped at what lies between it and the
	/// hero's chest, the probes, the bias and the boom factor; Static: the pivot and the player's boom.
	fn boom_goal(&mut self, smooth: bool, p: &mut dyn Probe) -> (V3, f32) {
		let (fr, chest) = (self.fr, self.fr.lift(self.hero_last, PIVOT));
		self.dy.pivc = if smooth { clip_line(p, chest, self.pivot, self.cfg.tune.safe_margin) } else { self.pivot };
		if !smooth {
			return (self.dy.pivc, self.dist);
		}
		if self.cfg.open_probe {
			self.open_probe(p);
		}
		self.bias();
		self.dist_factor();
		let goal = (self.dist * self.dy.dist_f).clamp(ZOOM.0, ZOOM.1);
		self.dy.pivc = pull_to_chest(&fr, p, self.dy.pivc, chest, self.yaw, self.pitch, goal);
		(self.dy.pivc, goal)
	}

	/// The wall tests and the eye. Wall: pull in at once to the hit (minus 0.2), release outward RELEASE a field. With a wall close
	/// behind the hero (boom under RISE_MIN) the camera rises over him (pitch up in 0.3 rad steps, the first that gives room) instead
	/// of sitting in his head; the rise ends once the boom has RISE_BAND more room (hysteresis), on a critically damped spring.
	fn boom(&mut self, pivot: V3, goal_len: f32, smooth: bool, p: &mut dyn Probe) -> Pose {
		let (yaw, fr) = (self.yaw, self.fr);
		let base = reach(&fr, p, pivot, yaw, self.pitch, goal_len, smooth);
		let (lo, hi) = (RISE_MIN.min(goal_len), (RISE_MIN + RISE_BAND).min(goal_len));
		let mut rise_goal = self.rise_goal;
		if base >= hi || self.pitch >= PITCH.1 {
			rise_goal = 0.0;
		} else if base < lo {
			rise_goal = 0.0;
			let mut best = base;
			for k in 1..=4 {
				let pp = (self.pitch + 0.3 * k as f32).min(PITCH.1);
				let r = reach(&fr, p, pivot, yaw, pp, goal_len, smooth);
				if r > best + 0.3 {
					best = r;
					rise_goal = pp - self.pitch;
				}
				if r >= RISE_MIN.min(goal_len) || pp >= PITCH.1 {
					break;
				}
			}
		}
		self.rise_goal = rise_goal;
		self.rise_v += RISE_W * RISE_W * (rise_goal - self.rise) - 2.0 * RISE_W * self.rise_v;
		self.rise += self.rise_v;
		let pitch = if smooth { (self.pitch + self.rise + self.dy.bias).clamp(PITCH.0, PITCH.1) } else { (self.pitch + self.rise).min(PITCH.1) };
		let safe = if (pitch - self.pitch).abs() < 1e-4 { base } else { reach(&fr, p, pivot, yaw, pitch, goal_len, smooth) };
		self.len = if safe < self.len {
			self.dy.len_v = 0.0;
			safe
		} else if smooth && !self.aiming {
			// Smooth: after a wall the boom eases back out on a critically damped spring (about 0.5 s, no pop, no jolt).
			let (y, v) = crit(self.cfg.dt, self.len - safe, self.dy.len_v, self.cfg.tune.boom_out);
			self.dy.len_v = v;
			(safe + y).min(safe)
		} else {
			(self.len + RELEASE).min(safe)
		};
		self.tr = [base, safe, rise_goal];
		let e = fr.eye(pivot, yaw, pitch, self.len);
		// After a seed the rows start on retail's view and fade to the orbit's (`modcam_look.rs`); the eye is the orbit's.
		let (ry, rp) = self.look.apply(yaw, pitch);
		self.last_rows = fr.rows(ry, rp);
		self.last = (e, ry, rp);
		Pose { eye: e, rows: self.last_rows }
	}
}

/// A stick axis `tgt` ramped from `r`: up at `ramp_up` full deflections a second, down (or through zero) at `ramp_dn`.
fn ramp1(t: &Tune, dt: f32, r: f32, tgt: f32) -> f32 {
	let rate = if tgt.abs() > r.abs() && tgt * r >= 0.0 { t.ramp_up } else { t.ramp_dn };
	let m = rate * dt;
	r + (tgt - r).clamp(-m, m)
}

/// The segment test from `from` to `to`, extended `margin` beyond `to`: on a contact the point stops `margin` short of it (never past
/// `to`). The extension makes the clip continuous: it starts the moment `to` comes within the margin of a surface, so nothing pops.
/// The Smooth pivot lags the hero (soft follow), so on a climb it ends up under the ledge he stands on; a boom test from there would
/// start inside the rock: the pivot is therefore tested from the hero's chest.
fn clip_line(p: &mut dyn Probe, from: V3, to: V3, margin: f32) -> V3 {
	let d = sub(to, from);
	let m = len3(d);
	if m < 0.05 {
		return to;
	}
	let u = scale3(d, 1.0 / m);
	match p.seg(from, add3(to, scale3(u, margin))) {
		Some(h) => add3(from, scale3(u, (len3(sub(h, from)) - margin).clamp(0.0, m))),
		None => to,
	}
}

/// How far the boom can reach from `pivot` toward the eye for `yaw` / `pitch` (goal length `goal`): Smooth never ends past the contact
/// itself (the floor of MIN_LEN would put the eye in the wall when the contact is closer).
fn reach(fr: &Frame, p: &mut dyn Probe, pivot: V3, yaw: f32, pitch: f32, goal: f32, smooth: bool) -> f32 {
	match p.seg(pivot, fr.eye(pivot, yaw, pitch, goal)) {
		Some(h) if smooth => {
			let l = len3(sub(h, pivot));
			(l - 0.2).max(MIN_LEN.min(0.5 * l))
		}
		Some(h) => (len3(sub(h, pivot)) - 0.2).max(MIN_LEN),
		None => goal,
	}
}

/// The hero must stay in view: the Smooth pivot lags the hero, so on a climb onto a platform it stays beside / under the ledge, where the
/// boom test from the pivot finds nothing and the eye ends under the platform (round 3 issue 2). When the eye's line to the hero's chest
/// meets a surface, the pivot moves toward the chest by the share of the line that is cut off (all the way when the contact is at the
/// chest), so the boom's own wall tests then work from the hero's side of the surface. Open space: the pivot as it is.
pub fn pull_to_chest(fr: &Frame, p: &mut dyn Probe, pivot: V3, chest: V3, yaw: f32, pitch: f32, len: f32) -> V3 {
	if len3(sub(chest, pivot)) < 0.3 {
		return pivot;
	}
	let e = fr.eye(pivot, yaw, pitch, len);
	let total = len3(sub(e, chest));
	if total < 0.05 {
		return pivot;
	}
	match p.seg(chest, e) {
		Some(h) => {
			let t = ((len3(sub(h, chest)) - 0.2).max(0.0) / total).clamp(0.0, 1.0);
			add3(pivot, scale3(sub(chest, pivot), 1.0 - t))
		}
		None => pivot,
	}
}
