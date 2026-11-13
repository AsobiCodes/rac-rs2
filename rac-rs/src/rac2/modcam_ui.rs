//! The Modern camera's input on its way from the window to the game thread (lane scratch_rc2_cam, stage A, 2026-10-31; R&C1's
//! `retail/modcam.rs` `ui_*` / `take_ui`, cut to what R&C2 has): the window thread adds raw mouse motion (`ui_mouse`), wheel lines
//! (`ui_zoom`) and the camera keys' share of the right stick (`ui_keys`); the game thread calls `take_ui(field, pad)` once per field
//! before `m.frame()` (host/game.rs) and gets the field's `Input` (whole counts, the rest stays for the next field), ORed with the
//! scripted feed `RAC2_CAM_FEED`. The camera hand (stage B) reads the same value back with `current()` while the field runs.
//! The right mouse button is the R2 bit of the pad (host.rs `poll_input`): the strafe aim retail has natively; `Input::aim` is the R2
//! bit as the game gets it (the window's RMB, a gamepad's R2, a replay's, or the feed), so the camera sees what the game sees.
//!
//! The Modern camera is on unless `camera=classic` is in pc_options.txt (engine/pcopts.rs, default: modern) or `RAC2_CAM=classic`
//! says so (`modern()`); Classic changes nothing in what the game gets (the gate runs with it), only RMB is still R2.

use crate::modcam_pose::Input;
use rac_engine::input::binds::{BIT, L2, R2};
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::Mutex;

/// What the window collected since the last field.
#[derive(Default)]
struct Ui {
	look: [f32; 2],
	zoom: f32,
	keys: [i8; 2],
}

static UI: Mutex<Ui> = Mutex::new(Ui { look: [0.0; 2], zoom: 0.0, keys: [0; 2] });
static FEED: Mutex<Option<Vec<(u32, u32, Input)>>> = Mutex::new(None);
/// The input of the field the game thread runs now (`take_ui`), for the camera hand.
static CUR: Mutex<(u32, Input)> = Mutex::new((0, Input { look: [0; 2], aim: false, strafe: false, zoom: 0, keys: [0; 2] }));

/// The right stick bytes [rx, ry] of the pad the game thread gives the game this field (keyboard and controller merged, or a replay's).
static STICK: AtomicU16 = AtomicU16::new(0x8080);
/// The window drives the game (no replay, no scripted pad: host/game.rs `live`). False: the Classic camera's first person takes no
/// window mouse (`take_ui`).
static WINDOW_INPUT: AtomicBool = AtomicBool::new(true);

/// Game thread, each field before `take_ui`: whether the window's input drives the game (host/game.rs `field_pad`).
pub fn set_window_input(on: bool) {
	WINDOW_INPUT.store(on, Ordering::Relaxed);
}

/// Whether the Modern camera is on: `RAC2_CAM=modern|classic` (read once) wins over pc_options `camera=` (default modern), which
/// is read LIVE (build 45 A4, as R&C1's `modcam::on()`): the Camera row switches Classic <-> Static / Smooth without a restart. The
/// hand stays registered (lib.rs) and blends a live Modern camera back to retail's over `modcam_pose::BLEND` fields when Classic is
/// chosen (`modcam_hand::follow`); Classic chosen with no Modern pose left writes nothing.
pub fn modern() -> bool {
	static ENV: std::sync::OnceLock<Option<bool>> = std::sync::OnceLock::new();
	ENV.get_or_init(|| env_choice(std::env::var("RAC2_CAM").ok().as_deref())).unwrap_or_else(|| crate::options::get().camera_modern())
}

/// `RAC2_CAM`: Some(true) = modern, Some(false) = classic, None = not set (or not understood): the pc_options decide.
fn env_choice(v: Option<&str>) -> Option<bool> {
	match v?.trim().to_ascii_lowercase().as_str() {
		"modern" | "on" | "1" => Some(true),
		"classic" | "off" | "0" => Some(false),
		_ => None,
	}
}

/// Raw mouse motion while the cursor is captured (window thread).
pub fn ui_mouse(dx: f32, dy: f32) {
	if let Ok(mut u) = UI.lock() {
		u.look[0] += dx;
		u.look[1] += dy;
	}
}

/// Wheel lines (positive = away from the user = zoom in).
pub fn ui_zoom(lines: f32) {
	if let Ok(mut u) = UI.lock() {
		u.zoom += lines;
	}
}

/// The camera keys' share of the right stick this frame, offset from 0x80 (0 = none held).
pub fn ui_keys(keys: [i8; 2]) {
	if let Ok(mut u) = UI.lock() {
		u.keys = keys;
	}
}

/// Focus lost or the capture ended: drop the pending motion, wheel and keys.
pub fn ui_release() {
	if let Ok(mut u) = UI.lock() {
		*u = Ui::default();
	}
}

/// `RAC2_CAM_FEED` (tests, shots): `from-to:dx,dy,aim[,zoom];...` (decimal), the mouse counts per field and RMB (= R2) for fields
/// `from..to`, ORed over the window's input.
fn feed() -> Vec<(u32, u32, Input)> {
	parse_feed(&std::env::var("RAC2_CAM_FEED").unwrap_or_default())
}

fn parse_feed(v: &str) -> Vec<(u32, u32, Input)> {
	v.split(';')
		.filter_map(|p| {
			let (r, a) = p.split_once(':')?;
			let (f, t) = r.split_once('-')?;
			let n: Vec<i32> = a.split(',').filter_map(|x| x.trim().parse().ok()).collect();
			let inp = Input { look: [*n.first()? as i16, *n.get(1)? as i16], aim: n.get(2).is_some_and(|&l| l != 0), strafe: n.get(2).is_some_and(|&l| l != 0), zoom: n.get(3).copied().unwrap_or(0) as i8, keys: [0; 2] };
			Some((f.trim().parse().ok()?, t.trim().parse().ok()?, inp))
		})
		.collect()
}

impl Ui {
	/// The input of field `n`: the whole counts collected (the rest stays), the pad's R2 as `aim`, its L2 or R2 as `strafe`, the feed over it.
	fn take(&mut self, n: u32, pad: u16, feed: &[(u32, u32, Input)]) -> Input {
		let x = self.look[0].round().clamp(-32000.0, 32000.0);
		let y = self.look[1].round().clamp(-32000.0, 32000.0);
		self.look[0] -= x;
		self.look[1] -= y;
		let z = self.zoom.trunc().clamp(-100.0, 100.0);
		self.zoom -= z;
		let mut out = Input { look: [x as i16, y as i16], aim: pad & BIT[R2] != 0, strafe: pad & (BIT[L2] | BIT[R2]) != 0, zoom: z as i8, keys: self.keys };
		for &(a, b, i) in feed {
			if (a..b).contains(&n) {
				out.look[0] = out.look[0].saturating_add(i.look[0]);
				out.look[1] = out.look[1].saturating_add(i.look[1]);
				out.aim |= i.aim;
				out.strafe |= i.strafe;
				out.zoom = out.zoom.saturating_add(i.zoom);
			}
		}
		out
	}
}

/// Game thread, once per field before `m.frame()`: the field's input (`pad` = the buttons the game is about to get). With the
/// Classic camera: only the pad's own R2 as `aim`, `strafe` stays false (no feed, no keys; the code ignores the scripted feed), so the game's pad
/// is never touched by this module; the mouse counts only while first person is up (`modcam_fp`).
pub fn take_ui(n: u32, pad: u16) -> Input {
	let out = if modern() {
		let Ok(mut u) = UI.lock() else { return Input::default() };
		let Ok(mut f) = FEED.lock() else { return Input::default() };
		let w = u.take(n, pad, f.get_or_insert_with(feed));
		// A replay with the recording's camera input (modcam_rec.rs): that, not the window's.
		crate::modcam_rec::replayed(n).unwrap_or(w)
	} else if crate::modcam_fp::live() {
		// Classic in first person (R&C1 fp_look works with Classic too): only the mouse counts, for `modcam_fp::pre_field`.
		let Ok(mut u) = UI.lock() else { return Input::default() };
		let window = u.take(n, pad, &[]).look;
		// [b45 review F3] A replay or a scripted pad: the recording's counts (cam.bin) or none, never the window's, so a Classic
		// replay stays bit-exact (as build 44).
		let look = match crate::modcam_rec::replayed(n) {
			Some(i) => i.look,
			None if WINDOW_INPUT.load(Ordering::Relaxed) => window,
			None => [0, 0],
		};
		Input { aim: pad & BIT[R2] != 0, look, ..Input::default() }
	} else {
		Input { aim: pad & BIT[R2] != 0, ..Input::default() }
	};
	if let Ok(mut c) = CUR.lock() {
		*c = (n, out);
	}
	out
}

/// The input `take_ui` gave for the field now running (the field number, the input): for the camera hand inside `m.frame()`.
pub fn current() -> (u32, Input) {
	CUR.lock().map_or((0, Input::default()), |c| *c)
}

/// Game thread, once per field before `take_ui`: the pad's right stick bytes (`m.set_pad`'s `s[0]`, `s[1]`).
pub fn note_stick(s: [u8; 2]) {
	STICK.store(u16::from_le_bytes(s), Ordering::Relaxed);
}

/// The right stick as the Modern camera's turn maths takes it: the controller's share flipped to the mouse's sign, the camera keys
/// (`Input::keys` of the field running now) left alone (`modcam_pose::camera_sticks`; R&C1's `retail/modcam/sticks.rs` does the same
/// to the bytes the game gets, here the game's pad stays untouched and only the camera hand reads this). The Reversed options go to the hand's `Gains`
/// (addrs.rs OPT_LR / OPT_UD); `class4` is false because the hand drives class 0 only.
pub fn rstick() -> [u8; 2] {
	rstick_of(STICK.load(Ordering::Relaxed).to_le_bytes(), current().1.keys)
}

fn rstick_of(merged: [u8; 2], keys: [i8; 2]) -> [u8; 2] {
	crate::modcam_pose::camera_sticks(merged, keys, false, false)
}

/// The buttons the game gets: `pad` with R2 added when the camera input says RMB / the feed aims (a no-op for the window's RMB, which
/// `poll_input` already put in the pad).
pub fn with_aim(pad: u16, i: &Input) -> u16 {
	if i.aim { pad | BIT[R2] } else { pad }
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_controllers_stick_has_the_mouses_sign_and_the_keys_keep_theirs() {
		assert_eq!(rstick_of([0x80, 0x80], [0, 0]), [0x80, 0x80], "centre stays");
		assert_eq!(rstick_of([0xff, 0x00], [0, 0]), [0x01, 0xff], "stick right + up: flipped");
		assert_eq!(rstick_of([0xff, 0x80], [127, 0]), [0xff, 0x80], "the E key drives x: not flipped");
		assert_eq!(rstick_of([0x01, 0x00], [-127, 0]), [0x01, 0xff], "the Q key drives x (kept), the pad's y is flipped");
	}

	#[test]
	fn feed_parses() {
		let f = parse_feed("10-20:5,-3,1;30-31:0,0,0,2;bad;1-2:x");
		assert_eq!(f.len(), 2);
		assert_eq!(f[0], (10, 20, Input { look: [5, -3], aim: true, strafe: true, zoom: 0, keys: [0; 2] }));
		assert_eq!(f[1].2.zoom, 2);
		assert!(parse_feed("").is_empty());
	}

	#[test]
	fn whole_counts_go_out_the_rest_stays() {
		let mut u = Ui::default();
		u.look = [2.6, -0.4];
		u.zoom = 1.5;
		let a = u.take(0, 0, &[]);
		assert_eq!((a.look, a.zoom), ([3, 0], 1));
		// The rest (x -0.4, y -0.4, zoom 0.5) adds to the next field's motion. (y gets +1.0, not +0.9: -0.4f32 + 0.9f32 is
		// 0.49999998, which rounds to 0 where the arithmetic on paper says 0.5.)
		u.look[0] += 0.6;
		u.look[1] += 1.0;
		u.zoom += 0.6;
		let b = u.take(1, 0, &[]);
		assert_eq!((b.look, b.zoom), ([0, 1], 1), "{b:?}");
	}

	#[test]
	fn aim_is_the_pads_r2_and_the_feed_adds_to_the_window() {
		let mut u = Ui::default();
		u.look = [4.0, 0.0];
		let feed = parse_feed("5-7:10,2,1,1");
		assert!(!u.take(4, 0, &feed).aim);
		let a = u.take(5, 0, &feed);
		assert_eq!((a.look, a.aim, a.zoom), ([10, 2], true, 1));
		assert!(u.take(7, 0, &feed).look == [0, 0], "the range ends before field 7");
		assert!(u.take(8, BIT[R2], &feed).aim, "the pad's R2 is the aim");
		// The feed adds to the window's counts.
		let mut v = Ui::default();
		v.look = [4.0, 0.0];
		assert_eq!(v.take(6, 0, &feed).look, [14, 2]);
	}

	#[test]
	fn l2_strafes_for_the_camera_but_the_game_never_gets_an_r2_from_it() {
		let mut u = Ui::default();
		let l2 = u.take(0, BIT[L2], &[]);
		assert!(l2.strafe && !l2.aim, "L2 alone: the camera strafes, no RMB / R2: {l2:?}");
		assert_eq!(with_aim(BIT[L2], &l2), BIT[L2], "the game's pad is not changed");
		let r2 = u.take(1, BIT[R2], &[]);
		assert!(r2.strafe && r2.aim, "R2: both");
		let both = u.take(2, BIT[L2] | BIT[R2], &[]);
		assert!(both.strafe);
		assert!(!u.take(3, 0x40 | 0x1000, &[]).strafe, "other buttons are no strafe");
		// The scripted feed (RMB) is R2: it strafes too.
		let feed = parse_feed("5-6:0,0,1");
		let f = u.take(5, 0, &feed);
		assert!(f.strafe && f.aim);
	}

	#[test]
	fn keys_travel_with_the_input() {
		let mut u = Ui::default();
		u.keys = [127, -127];
		assert_eq!(u.take(0, 0, &[]).keys, [127, -127]);
	}

	#[test]
	fn rmb_through_the_feed_presses_r2() {
		let aim = Input { aim: true, ..Input::default() };
		assert_eq!(with_aim(0, &aim), BIT[R2]);
		assert_eq!(with_aim(0x40, &aim), 0x40 | BIT[R2]);
		assert_eq!(with_aim(0x40, &Input::default()), 0x40, "no aim: the pad is untouched");
	}

	#[test]
	fn classic_leaves_every_pad_value_alone() {
		// Classic (and any field without RMB / feed): `take_ui` gives aim = the pad's own R2, so `with_aim` never changes the pad.
		for pad in 0..=u16::MAX {
			let aim = Input { aim: pad & BIT[R2] != 0, ..Input::default() };
			assert_eq!(with_aim(pad, &aim), pad);
		}
	}

	#[test]
	fn env_choice_reads_the_words() {
		assert_eq!(env_choice(Some("Classic")), Some(false));
		assert_eq!(env_choice(Some("modern")), Some(true));
		assert_eq!(env_choice(Some(" 0 ")), Some(false));
		assert_eq!(env_choice(Some("sideways")), None);
		assert_eq!(env_choice(None), None);
	}
}
