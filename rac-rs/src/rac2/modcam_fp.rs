//! [PC] Mouse look in retail's first-person view (build 45 P5, sweep C1; the twin of R&C1's `rac1/retail/modcam/fp_look.rs`).
//! First person is retail's free-look camera, class 4 (L1 look = hero state 1, hero camera mode 4 [C modcam_yield]); the Modern camera's
//! hand runs on the FOLLOW class only, so the mouse did nothing there. R&C2's class-4 update (Aranos 0x42bc00) is R&C1's FUN_002e8590
//! line for line [C 2026-11-12, rc2dis --ram runs/ram2000.bin]: class state = record +0x70, the stick yaw turns the rows about the hero
//! moby's up (hero cell 0x18c130 -> moby +0xe0, `jal 0x2ef9a0` with `addiu a1, 0xe0`), the pitch word is state +0x10 with the hard stop
//! f32 0x3fbba867 (84 deg; R&C1's 0x3fbba866), and the stick pitch is held still while the camera switch blend (u16 `basis - 0x4630`,
//! `lh 0x290(0x1b8cc0)`) runs with the previous hero state (0x18c140, `lw 0x22a0`) = 2.
//!
//! Before each field (`pre_field`, game thread, before `m.frame()`): with mouse counts and the free look current in gameplay, its forward
//! row is turned as retail turns it for the stick: yaw about the hero's up, pitch about up x forward with the pitch word, stopped at
//! 84 deg; rows 1 / 2 rebuilt as the update rebuilds them. Retail's update then runs on that pose in the same field (stick springs, eye,
//! publish), so the pad and keys add to the mouse. Gains and signs are the Modern camera's mouse (`modcam_hand::gains_of`: Mouse speed
//! row, Reversed options, the mouse invert rows); counts are whole per field (`modcam_ui::take_ui`), so a still mouse writes nothing
//! and the turn does not depend on the frame rate. Works with Camera Classic too (R&C1 does): `live()` tells the cursor rule to hold
//! the mouse while first person is up. RAC2_FP_MOUSE=0: off.

use crate::modcam_pose::{V3, cross3, dot3, len3};
use ee_oracle::mem::Mem;
use rac_engine::fmath::Fm;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Retail's hard pitch limit of the free look [C f32 0x3fbba867 at Aranos 0x42c09c].
const PITCH_MAX: f32 = f32::from_bits(0x3fbb_a867);
/// The hero moby cell and the moby's up vector (retail's yaw axis in the free look).
const HERO_CELL: u32 = 0x18c130;
const HERO_UP: u32 = 0xe0;
/// The previous hero state (retail's stick pitch gate reads it).
const HERO_PREV_STATE: u32 = 0x18c140;
/// Free-look record word +0x70: its class state; state +0x10: the pitch word (rad, + = looking down).
const REC_STATE: u32 = 0x70;
const STATE_PITCH: u32 = 0x10;
/// The hero camera mode of the L1 look (`modcam_yield.rs`: 3 rail, 4 look, 7 swing).
const LOOK_MODE: u32 = 4;

/// First person was the camera at the last field start.
static LIVE: AtomicBool = AtomicBool::new(false);
/// [b49 U5b] The hero's camera mode word said "look" (4) at the last field start. The free look's record becomes current a few fields
/// AFTER the mode (find-out runs b49/u5b/runs e1, e2: L1 down at field 20, hero mode 4 from 21, class 4 current from 26), and the
/// follow camera's `RECENT` window is short: this keeps the cursor held (and the mouse counts coming) across that gap.
static LOOKING: AtomicBool = AtomicBool::new(false);

/// RAC2_FP_MOUSE=0 turns the mouse look in first person off (read once).
fn switch() -> bool {
	static S: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
	*S.get_or_init(|| std::env::var("RAC2_FP_MOUSE").map_or(true, |v| v.trim() != "0"))
}

/// First person (retail's free look, class 4) is up in gameplay: the window holds the mouse (also with Camera Classic) and
/// `modcam_ui::take_ui` passes the mouse counts on with Classic.
pub fn live() -> bool {
	switch() && LIVE.load(Ordering::Relaxed)
}

/// [b49 U5b] The cursor rule's term: first person is up, or the hero's look mode has begun and its camera is not current yet.
pub fn holds() -> bool {
	holds_of(switch(), LIVE.load(Ordering::Relaxed), LOOKING.load(Ordering::Relaxed))
}

fn holds_of(on: bool, live: bool, looking: bool) -> bool {
	on && (live || looking)
}

fn rd(ram: &[u8], a: u32) -> u32 {
	rac_formats::bytes::u32_at(ram, a as usize).unwrap_or(0)
}

fn rf(ram: &[u8], a: u32) -> f32 {
	f32::from_bits(rd(ram, a))
}

/// [b50 V3] Why the first-person gate says no (the log line `rac2: cursor hold ... fp_gate`): the first gate in the chain that fails.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Gate {
	/// RAC2_FP_MOUSE=0.
	Off = 0,
	/// The hero's camera mode word is neither "look" (4) nor "weapon ready" (0x51) and first person was not up.
	NotLooking = 1,
	/// The level's camera addresses are not found yet, or the current-camera cell holds no record.
	NoRecord = 2,
	/// The current camera is not the free look (class 4): the switch has not happened yet, or the hero is not in it.
	NotFreeLook = 3,
	/// The game mode word is not found (`pc_input::find_mode`).
	NoMode = 4,
	/// The game mode word is not 0 (pause menu, a page, a cutscene).
	ModeNotPlay = 5,
	/// The free look's class state pointer is 0 or outside RAM.
	BadState = 6,
	/// First person is up.
	Up = 7,
}

/// The free look's record and its class state when it is the current camera in gameplay (game mode 0), or the gate that said no.
fn free_look_gate(ram: &[u8]) -> Result<(u32, u32, u32), Gate> {
	let (rec, class, basis) = crate::modcam_sticks::current_record(ram).ok_or(Gate::NoRecord)?;
	LAST_CLASS.store(class as u16 as u32, Ordering::Relaxed);
	if class != crate::modcam_sticks::FREE_LOOK_CLASS {
		return Err(Gate::NotFreeLook);
	}
	let mode = crate::pc_input::find_mode(ram).ok_or(Gate::NoMode)?;
	if rd(ram, mode as u32) != 0 {
		return Err(Gate::ModeNotPlay);
	}
	let state = rd(ram, rec + REC_STATE) & 0x01ff_ffff;
	if state != 0 && (state + 0x14) as usize <= ram.len() { Ok((rec, state, basis)) } else { Err(Gate::BadState) }
}

/// [b50 V3] What the last field's gates said, for the cursor log: the gate, the hero camera mode word, the camera class (0xffff = no
/// record), the game mode word (0xff = not found).
static WHY: AtomicU32 = AtomicU32::new(0);
static LAST_CLASS: AtomicU32 = AtomicU32::new(0xffff);

fn note_gate(g: Gate, hero_cam: u32, ram: &[u8]) {
	let mode = if g == Gate::NotLooking { 0xff } else { crate::pc_input::find_mode(ram).map_or(0xff, |m| rd(ram, m as u32).min(0xfe)) };
	let class = if g == Gate::NoRecord { 0xff } else { LAST_CLASS.load(Ordering::Relaxed) & 0xff };
	WHY.store(g as u32 | (hero_cam & 0xff) << 8 | class << 16 | mode << 24, Ordering::Relaxed);
}

/// [b50 V3] The packed gate state; `gate_code` (the gate alone) makes the cursor log write a line when it changes.
pub fn gate_bits() -> u32 {
	WHY.load(Ordering::Relaxed)
}

pub fn gate_code() -> u32 {
	gate_bits() & 0xff
}

pub fn gate_text() -> String {
	let w = gate_bits();
	let name = match w & 0xff {
		0 => "off",
		1 => "hero_cam_not_look",
		2 => "no_camera_record",
		3 => "camera_not_free_look",
		4 => "no_mode_word",
		5 => "mode_not_play",
		6 => "bad_state_ptr",
		_ => "up",
	};
	format!("fp_gate {} hero_cam {:#x} cam_class {} game_mode {}", name, w >> 8 & 0xff, w >> 16 & 0xff, w >> 24)
}

/// [b50 V3] Review finding 1: the look mode holds the mouse only in gameplay. The hero's camera word keeps 4 while the pause menu, a
/// Classic page or a box is up (the hero is frozen), and the cursor must go then (`mode` = the game mode word, None = not found).
fn looking_of(hero_cam: u32, mode: Option<u32>) -> bool {
	hero_cam == LOOK_MODE && mode == Some(0)
}

/// Game thread, once per field before `m.frame()`: notes whether first person is up (`live`) and turns its view by the field's
/// mouse counts (`look`, from `modcam_ui::take_ui`). Nothing is written without counts or outside first person.
pub fn pre_field(mem: &mut Mem, look: [i16; 2]) {
	if !switch() {
		WHY.store(0, Ordering::Relaxed);
		return;
	}
	let ram: &[u8] = &mem.ram;
	let hero_cam = rd(ram, crate::modcam_yield::HERO_CAM);
	// [b50 V3] Review finding 1: the mode word is read only when the hero's camera word says "look" (4).
	let mode = (hero_cam == LOOK_MODE).then(|| crate::pc_input::find_mode(ram).map(|m| rd(ram, m as u32))).flatten();
	LOOKING.store(looking_of(hero_cam, mode), Ordering::Relaxed);
	// [b45 review F2] The cheap gate first: the hero's camera mode word says "look" (`modcam_yield::HERO_CAM` = 4 [C]). Without it
	// the level's camera addresses are not even looked up, so a Classic field costs what it did in build 44.
	if !gate_open(hero_cam, LIVE.load(Ordering::Relaxed)) {
		LIVE.store(false, Ordering::Relaxed);
		note_gate(Gate::NotLooking, hero_cam, ram);
		return;
	}
	let (rec, state, basis) = match free_look_gate(ram) {
		Ok(x) => x,
		Err(g) => {
			LIVE.store(false, Ordering::Relaxed);
			note_gate(g, hero_cam, ram);
			return;
		}
	};
	note_gate(Gate::Up, hero_cam, ram);
	LIVE.store(true, Ordering::Relaxed);
	if look == [0, 0] {
		return;
	}
	let hero = rd(ram, HERO_CELL) & 0x01ff_ffff;
	if hero == 0 || (hero + HERO_UP + 12) as usize > ram.len() {
		return;
	}
	let g = crate::modcam_hand::gains_of(ram, &crate::options::get());
	let (dyaw, dpitch) = crate::modcam_pose::turn(look, [0.0; 2], &g);
	// Retail's own gate of the stick pitch: a camera switch blend running and the previous hero state 2.
	let blend = rac_formats::bytes::u16_at(ram, basis.wrapping_sub(crate::modcam_blend::BLEND_BELOW_BASIS) as usize).unwrap_or(0);
	let dpitch = if blend != 0 && rd(ram, HERO_PREV_STATE) == 2 { 0.0 } else { dpitch };
	let fwd = [rf(ram, rec), rf(ram, rec + 4), rf(ram, rec + 8)];
	let up = [rf(ram, hero + HERO_UP), rf(ram, hero + HERO_UP + 4), rf(ram, hero + HERO_UP + 8)];
	let Some((rows, pitch)) = turn_view(fwd, up, rf(ram, state + STATE_PITCH), dyaw, dpitch) else { return };
	for (i, row) in rows.iter().enumerate() {
		for (j, x) in row.iter().enumerate() {
			mem.wr32(rec + 0x10 * i as u32 + 4 * j as u32, x.to_bits());
		}
	}
	mem.wr32(state + STATE_PITCH, pitch.to_bits());
}

/// [b46 U1] Bug B: the cheap gate before the camera lookup. First person is ENTERED through the L1 look (hero state 1, camera mode 4)
/// but does not stay there: the first shot in first person sets hero state 29 with camera mode 0x51 (weapon ready) and the view stays
/// first person (L1 held, class-4 free look current) [C 2026-11-12 find-out run b46\u1\runs\fpfire, state 0357: field 70 state 1 /
/// mode 4, fields 100..150 state 29 / mode 0x51 / previous state 1, shots in first person with the reticle]. Build 45 gated on mode 4
/// alone, so after one shot `live()` went false: no cursor hold, no mouse counts, no turn (the user's screenshot fired: ammo counting).
/// Now mode 4 or 0x51 opens it (0x51 also after a pause in first person: the menu's game mode ends `live`), and once live the class
/// check of `free_look` alone decides until the free look ends (any other state the hero takes while looking). Mode 0x51 in third
/// person (shooting) costs one class read per field (the level's camera addresses are found once per level, `modcam_sticks`).
fn gate_open(hero_cam: u32, was_live: bool) -> bool {
	was_live || hero_cam == LOOK_MODE || hero_cam == WEAPON_MODE
}

/// The hero camera mode of the weapon-ready state 29 (`modcam_yield.rs`; first person keeps it after a shot).
const WEAPON_MODE: u32 = 0x51;

/// The free look's rows and pitch word after a mouse turn (R&C1 `fp_look::turn_view`). `fwd` = the record's row 0, `up` = the hero's
/// up (any length), `pitch` = the pitch word; yaw about up (+ = left), then pitch about up x forward (+ = down; the word follows and
/// stops at +-84 deg, a word already past it is not pulled back). Rows 1 / 2 as retail's update rebuilds them: unit(up x fwd), then
/// fwd x row 1. None for a degenerate up / forward (nothing is written).
pub(crate) fn turn_view(fwd: V3, up: V3, pitch: f32, dyaw: f32, dpitch: f32) -> Option<([V3; 3], f32)> {
	let u = unit(up)?;
	let f = rotate(unit(fwd)?, u, dyaw);
	let p = if dpitch > 0.0 { (pitch + dpitch).min(PITCH_MAX.max(pitch)) } else { (pitch + dpitch).max((-PITCH_MAX).min(pitch)) };
	let f = if p != pitch { unit(rotate(f, unit(cross3(u, f))?, p - pitch))? } else { unit(f)? };
	let left = unit(cross3(u, f))?;
	Some(([f, left, cross3(f, left)], p))
}

/// `v` turned about the unit axis `k` by `a` radians, right hand (Rodrigues; plain multiplies and adds, no fused ones).
fn rotate(v: V3, k: V3, a: f32) -> V3 {
	if a == 0.0 {
		return v;
	}
	let (s, c) = a.fsin_cos();
	let kv = cross3(k, v);
	let d = dot3(k, v) * (1.0 - c);
	[v[0] * c + kv[0] * s + k[0] * d, v[1] * c + kv[1] * s + k[1] * d, v[2] * c + kv[2] * s + k[2] * d]
}

fn unit(v: V3) -> Option<V3> {
	let l = len3(v);
	(l.is_finite() && l > 1e-6).then(|| [v[0] / l, v[1] / l, v[2] / l])
}

#[cfg(test)]
mod tests {
	use super::*;

	fn close(a: V3, b: V3) -> bool {
		(0..3).all(|i| (a[i] - b[i]).abs() < 1e-5)
	}

	/// [b46 U1] Bug B: hero camera modes of one first-person session with a shot (run fpfire): 4, then 0x51 while the view stays
	/// first person. Build 45's gate (`mode == 4`) closed at the shot; the new one stays open, and still opens on nothing but mode 4.
	#[test]
	fn first_person_stays_live_after_a_shot() {
		let modes = [4u32, 0x51, 0x51, 0x51];
		let (mut b45, mut now) = (Vec::new(), Vec::new());
		let mut live = false;
		for m in modes {
			b45.push(m == LOOK_MODE);
			live = gate_open(m, live); // free_look's class check holds in every one of these fields (first person on screen)
			now.push(live);
		}
		assert_eq!(b45, [true, false, false, false], "build 45: the mouse look ended at the first shot");
		assert_eq!(now, [true, true, true, true]);
		assert!(gate_open(0x51, false), "back from a pause in first person after a shot: the class check decides");
		assert!(!gate_open(0, false) && !gate_open(7, false) && !gate_open(3, false), "walking, swinging, grinding: no camera lookup");
	}

	/// [b49 U5b] The cursor rule held the mouse only once the free look's record was current (`live`), a few fields after the hero's
	/// look mode began: the follow camera's window had run out by then and the cursor was let go. Now the look mode alone holds it.
	#[test]
	fn the_look_mode_holds_the_mouse_before_the_camera_switch() {
		assert!(!holds_of(true, false, false), "third person: not this term");
		assert!(holds_of(true, false, true), "mode 4, class 4 not current yet");
		assert!(holds_of(true, true, false), "first person up (also after a shot, mode 0x51)");
		assert!(!holds_of(false, true, true), "RAC2_FP_MOUSE=0");
	}

	/// [b50 V3] Review finding 1: a pause menu (game mode 3) opened in first person leaves the hero's camera word at 4: no hold then.
	#[test]
	fn the_look_hold_ends_with_gameplay() {
		assert!(looking_of(4, Some(0)), "first person in gameplay");
		assert!(!looking_of(4, Some(3)), "pause menu or a Classic page opened in first person");
		assert!(!looking_of(4, Some(4)), "a box / Quick Select");
		assert!(!looking_of(4, None), "mode word not found: no hold");
		assert!(!looking_of(0x51, Some(0)) && !looking_of(0, Some(0)));
	}

	#[test]
	fn the_pitch_stop_is_retails_word() {
		assert_eq!(PITCH_MAX.to_bits(), 0x3fbb_a867);
	}

	#[test]
	fn yaw_turns_about_up_and_keeps_the_frame_orthonormal() {
		let (rows, p) = turn_view([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0, std::f32::consts::FRAC_PI_2, 0.0).unwrap();
		assert!(close(rows[0], [0.0, 1.0, 0.0]), "{rows:?}");
		assert!(close(rows[1], [-1.0, 0.0, 0.0]), "left = up x fwd: {rows:?}");
		assert!(close(rows[2], [0.0, 0.0, 1.0]));
		assert_eq!(p, 0.0);
	}

	#[test]
	fn pitch_follows_the_word_and_stops_at_84_degrees() {
		let (rows, p) = turn_view([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0, 0.0, 0.3).unwrap();
		assert!((p - 0.3).abs() < 1e-6);
		assert!(rows[0][2] < 0.0, "+ pitch looks down: {rows:?}");
		let (_, p) = turn_view([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.4, 0.0, 0.5).unwrap();
		assert_eq!(p, PITCH_MAX);
		let (_, p) = turn_view([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], -1.4, 0.0, -0.5).unwrap();
		assert_eq!(p, -PITCH_MAX);
	}

	#[test]
	fn a_degenerate_frame_writes_nothing() {
		assert!(turn_view([0.0; 3], [0.0, 0.0, 1.0], 0.0, 0.1, 0.0).is_none());
		assert!(turn_view([1.0, 0.0, 0.0], [0.0; 3], 0.0, 0.1, 0.0).is_none());
	}
}
