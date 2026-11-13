//! Unit tests of the Modern camera's pose maths (modcam_pose.rs): the rows, the turn, the springs, the seed, the wall pull-in, the
//! blend back to retail, Static <-> Smooth.

use crate::modcam_frame::FLAT;
use crate::modcam_pose::*;

const DT: f32 = 0.02;

fn cam() -> Cam {
	Cam::new(Config::new(DT, 7.0))
}

fn retail_at(hero: V3, yaw: f32, pitch: f32, len: f32) -> Pose {
	let pivot = add3(hero, [0.0, 0.0, PIVOT]);
	Pose { eye: eye(pivot, yaw, pitch, len), rows: rows(yaw, pitch) }
}

/// A wall plane x = `x`: the segment test hits where a segment crosses it.
struct Wall {
	x: f32,
}

impl Probe for Wall {
	fn seg(&mut self, a: V3, b: V3) -> Option<V3> {
		if (a[0] - self.x) * (b[0] - self.x) >= 0.0 {
			return None;
		}
		let t = (self.x - a[0]) / (b[0] - a[0]);
		Some([self.x, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t])
	}
}

fn close(a: f32, b: f32, tol: f32) -> bool {
	(a - b).abs() <= tol
}

fn dist(a: V3, b: V3) -> f32 {
	len3(sub(a, b))
}

#[test]
fn rows_orthonormal_and_read_back() {
	for &(y, p) in &[(0.3f32, 0.2f32), (-2.0, -0.4), (3.0, 1.2)] {
		let r = rows(y, p);
		for i in 0..3 {
			assert!(close(dot3(r[i], r[i]), 1.0, 1e-5));
			for j in 0..i {
				assert!(dot3(r[i], r[j]).abs() < 1e-5);
			}
		}
		// Row 1 is the left of the view: up x forward.
		let c = cross3(r[2], r[0]);
		assert!(dist(c, r[1]) < 1e-5);
		let (ay, ap) = angles(&r);
		assert!(close(ap, p, 1e-5) && wrap(ay - y).abs() < 1e-5, "{ay} {ap} vs {y} {p}");
		// The eye sits behind and (pitch > 0) above the pivot.
		let ey = eye([0.0; 3], y, p, 5.0);
		assert!(close(len3(ey), 5.0, 1e-4) && (ey[2] > 0.0) == (p > 0.0));
	}
}

#[test]
fn the_controller_stick_has_the_mouses_sign() {
	let g = Gains::new(1.0, 1.0, false, false);
	// Mouse right (x +) and up (y -): yaw down = look right, pitch down = look up.
	let (my, mp) = turn([10, -10], [0.0; 2], &g);
	assert!(my < 0.0 && mp < 0.0);
	// The controller's right stick pushed right (rx high) and up (ry low) gets the same signs once the bytes are flipped.
	let s = stick_axes(camera_sticks([0xff, 0x01], [0, 0], false, false));
	let (sy, sp) = turn([0, 0], s, &g);
	assert!(sy < 0.0 && sp < 0.0, "{sy} {sp}");
	// Keys keep their old direction: Q (rx 0xff) turns the view left (yaw up), Z (ry 0xff) tilts it up (pitch down).
	let q = turn([0, 0], stick_axes(camera_sticks([0xff, 0x80], [127, 0], false, false)), &g);
	let z = turn([0, 0], stick_axes(camera_sticks([0x80, 0xff], [0, 127], false, false)), &g);
	assert!(q.0 > 0.0 && z.1 < 0.0, "{q:?} {z:?}");
	// The larger deflection decides: a controller pushed harder than the key is flipped, a full key is left alone.
	assert_eq!(camera_sticks([0xc0, 0x80], [10, 0], false, false)[0], 0x40);
	assert_eq!(camera_sticks([0xff, 0x80], [127, 0], false, false)[0], 0xff);
	// Free look at Normal keeps rx; Reversed flips it. Flipping twice gives the byte back (1..=255).
	assert_eq!(camera_sticks([0xff, 0x00], [0, 0], true, false), [0xff, 0xff]);
	assert_eq!(camera_sticks([0xff, 0x00], [0, 0], true, true), [0x01, 0xff]);
	for b in 1..=255u8 {
		assert_eq!(camera_sticks(camera_sticks([b, b], [0, 0], false, false), [0, 0], false, false), [b, b]);
	}
}

#[test]
fn stick_dead_zone_is_the_games() {
	assert_eq!(stick_axes([0x7f + 47, 0x7f - 47]), [0.0, 0.0]);
	let s = stick_axes([0x7f + 49, 0x7f - 49]);
	assert!(s[0] > 0.0 && s[0] < 0.03 && s[1] < 0.0 && s[1] > -0.03, "{s:?}");
	assert_eq!(stick_axes([0x7f + 124, 0x7f - 124]), [1.0, -1.0]);
	assert_eq!(stick_axes([0x80, 0x7f]), [0.0, 0.0]);
}

#[test]
fn speeds_and_reversed_scale_the_right_part() {
	assert_eq!((scale(None), scale(Some(50)), scale(Some(100)), scale(Some(10))), (1.0, 1.0, 2.0, 0.2));
	let base = turn([10, 10], [0.5, 0.5], &Gains::new(1.0, 1.0, false, false));
	let fast_mouse = turn([10, 10], [0.0; 2], &Gains::new(2.0, 1.0, false, false));
	let mouse = turn([10, 10], [0.0; 2], &Gains::new(1.0, 1.0, false, false));
	assert_eq!(fast_mouse, (mouse.0 * 2.0, mouse.1 * 2.0), "the mouse factor doubles the mouse");
	let st = turn([0, 0], [0.5, 0.5], &Gains::new(1.0, 1.0, false, false));
	assert_eq!(turn([0, 0], [0.5, 0.5], &Gains::new(5.0, 1.0, false, false)), st, "the mouse factor leaves the stick alone");
	assert_eq!(turn([0, 0], [0.5, 0.5], &Gains::new(1.0, 2.0, false, false)), (st.0 * 2.0, st.1 * 2.0), "the stick factor doubles the stick");
	assert!(close(base.0, mouse.0 + st.0, 1e-9) && close(base.1, mouse.1 + st.1, 1e-9), "the two add");
	for (lr, ud) in [(true, false), (false, true), (true, true)] {
		let r = turn([10, -10], [0.3, -0.4], &Gains::new(1.0, 1.0, lr, ud));
		let n = turn([10, -10], [0.3, -0.4], &Gains::new(1.0, 1.0, false, false));
		assert_eq!(r, (if lr { -n.0 } else { n.0 }, if ud { -n.1 } else { n.1 }), "reversed flips per axis");
	}
}

#[test]
fn critical_spring_converges_without_overshoot() {
	let (mut y, mut v) = (1.0f32, 0.0f32);
	for _ in 0..300 {
		(y, v) = crit(DT, y, v, 0.28);
		assert!(y >= -1e-6, "overshoot {y}");
	}
	assert!(y.abs() < 1e-4 && v.abs() < 1e-3);
	assert_eq!(crit(DT, 0.0, 0.0, 0.1), (0.0, 0.0));
	assert_eq!(k_of(DT, 0.0), 1.0);
	assert!(k_of(DT, 0.1) > 0.0 && k_of(DT, 0.1) < k_of(DT, 0.05));
}

#[test]
fn the_first_field_seeds_from_retails_pose() {
	let mut c = cam();
	let hero = [10.0, 20.0, 5.0];
	let r = retail_at(hero, 1.2, 0.4, 10.0);
	let out = c.step(&PoseIn::idle(1, hero, r), &mut NoWalls).expect("modern pose");
	assert!(c.live());
	assert!(close(c.yaw(), 1.2, 1e-4), "yaw {}", c.yaw());
	let (oy, op) = angles(&out.rows);
	assert!(close(oy, 1.2, 1e-4) && close(op, 0.4, 1e-4));
	// The boom starts at retail's 10 m and eases toward the 7 m the camera is set to: no snap.
	assert!(dist(out.eye, r.eye) < 0.2, "eye moved {}", dist(out.eye, r.eye));
	let mut last = out;
	for f in 2..400 {
		last = c.step(&PoseIn::idle(f, hero, r), &mut NoWalls).unwrap();
	}
	assert!(close(len3(sub(last.eye, add3(hero, [0.0, 0.0, PIVOT]))), 7.0, 0.02));
}

#[test]
fn mouse_turns_the_view_and_the_wheel_zooms() {
	let mut c = cam();
	let hero = [0.0; 3];
	let r = retail_at(hero, 0.0, 0.35, 7.0);
	c.step(&PoseIn::idle(1, hero, r), &mut NoWalls);
	let (y0, p0) = (c.yaw(), c.pitch());
	let mut i = PoseIn::idle(2, hero, r);
	i.input.look = [100, 50];
	c.step(&i, &mut NoWalls);
	assert!(close(wrap(c.yaw() - y0), -0.3, 1e-4), "mouse right = look right = yaw down: {}", c.yaw() - y0);
	assert!(close(c.pitch() - p0, 0.15, 1e-4), "mouse down = look down = pitch up: {}", c.pitch() - p0);
	// Mouse speed doubles it.
	let mut d = cam();
	d.step(&PoseIn::idle(1, hero, r), &mut NoWalls);
	let mut j = PoseIn::idle(2, hero, r);
	j.input.look = [100, 0];
	j.gains = Gains::new(2.0, 1.0, false, false);
	d.step(&j, &mut NoWalls);
	assert!(close(wrap(d.yaw() - y0), -0.6, 1e-4));
	// Wheel: zoom in 2 steps = 1 m closer, clamped at both ends.
	let mut f = 3;
	let mut run = |c: &mut Cam, zoom: i8| {
		let mut i = PoseIn::idle(f, hero, r);
		i.input.zoom = zoom;
		f += 1;
		c.step(&i, &mut NoWalls);
		for _ in 0..400 {
			f += 1;
			c.step(&PoseIn::idle(f, hero, r), &mut NoWalls);
		}
		c.len()
	};
	assert!(close(run(&mut c, 2), 6.0, 0.02));
	assert!(close(run(&mut c, 100), ZOOM.0, 0.02));
	assert!(close(run(&mut c, -100), ZOOM.1, 0.02));
	assert!(close(run(&mut c, 20), 4.0, 0.02), "14 m minus ten wheel steps");
}

#[test]
fn a_wall_behind_the_hero_pulls_the_boom_in_at_once_and_lets_it_out_slowly() {
	for wall_x in [-3.0f32, -1.5] {
		let mut c = cam();
		let hero = [0.0; 3];
		let r = retail_at(hero, 0.0, 0.35, 7.0);
		let mut w = Wall { x: wall_x };
		for f in 1..60 {
			let out = c.step(&PoseIn::idle(f, hero, r), &mut w).unwrap();
			assert!(out.eye[0] >= wall_x, "field {f}: the eye ({}) is behind the wall {wall_x}", out.eye[0]);
		}
		assert!(c.len() < 3.2, "len {}", c.len());
		// The wall goes away: Static lets the boom out RELEASE (0.1 m) a field.
		let before = c.len();
		c.step(&PoseIn::idle(100, hero, r), &mut NoWalls);
		assert!(c.len() > before && c.len() <= before + 0.1001, "{} -> {}", before, c.len());
	}
}

#[test]
fn smooth_boom_eases_out_after_a_wall_without_a_pop() {
	let mut c = cam();
	let hero = [0.0; 3];
	let r = retail_at(hero, 0.0, 0.35, 7.0);
	let mut sm = PoseIn::idle(1, hero, r);
	sm.smooth = true;
	let mut w = Wall { x: -3.0 };
	for f in 1..60 {
		sm.field = f;
		c.step(&sm, &mut w);
	}
	let mut prev = c.len();
	for f in 60..300 {
		sm.field = f;
		c.step(&sm, &mut NoWalls);
		assert!(c.len() >= prev - 1e-4 && c.len() - prev < 0.5 && c.len() <= 7.0001, "field {f}: {prev} -> {}", c.len());
		prev = c.len();
	}
	assert!(prev > 6.0, "the boom is out again: {prev}");
}

#[test]
fn retail_driving_blends_back_and_then_leaves_the_pose_alone() {
	let mut c = cam();
	let hero = [0.0; 3];
	let r0 = retail_at(hero, 0.0, 0.35, 7.0);
	let mut i = PoseIn::idle(1, hero, r0);
	i.input.look = [200, 0];
	let mut modern = c.step(&i, &mut NoWalls).unwrap();
	// The seed field shows retail's view (`modcam_look`, R&C1 `LookOff`); the turn shows fully once that offset has faded.
	for f in 0..crate::modcam_look::LOOK_FADE {
		modern = c.step(&PoseIn::idle(1, hero, r0), &mut NoWalls).unwrap();
		let _ = f;
	}
	assert!(wrap(angles(&modern.rows).0).abs() > 0.5);
	// Retail takes over (a gated hero state) with its own pose: BLEND blended poses, ending on retail's.
	let r1 = retail_at(hero, 1.0, 0.2, 5.0);
	let mut last = modern;
	for k in 0..BLEND {
		let mut b = PoseIn::idle(2 + k, hero, r1);
		b.retail_drives = true;
		modern = c.step(&b, &mut NoWalls).expect("blend field");
		assert!(dist(modern.eye, last.eye) < 1.5, "no pop at {k}");
		last = modern;
	}
	assert!(dist(last.eye, r1.eye) < 1e-3, "ends on retail's eye: {}", dist(last.eye, r1.eye));
	assert!(close(wrap(angles(&last.rows).0 - 1.0), 0.0, 1e-3));
	let mut b = PoseIn::idle(40, hero, r1);
	b.retail_drives = true;
	assert!(c.step(&b, &mut NoWalls).is_none(), "after the blend retail's pose stays");
	assert!(!c.live());
	// Back to the follow class: it seeds from retail again.
	assert!(c.step(&PoseIn::idle(41, hero, r1), &mut NoWalls).is_some() && c.live());
	assert!(close(wrap(c.yaw() - 1.0), 0.0, 1e-3));
}

#[test]
fn a_hero_jump_reseeds_from_retails_pose() {
	let mut c = cam();
	let r0 = retail_at([0.0; 3], 0.0, 0.35, 7.0);
	c.step(&PoseIn::idle(1, [0.0; 3], r0), &mut NoWalls);
	assert!(close(c.yaw(), 0.0, 1e-4));
	let far = [50.0, 0.0, 0.0];
	let r1 = retail_at(far, 1.0, 0.3, 8.0);
	let out = c.step(&PoseIn::idle(2, far, r1), &mut NoWalls).unwrap();
	assert!(close(wrap(c.yaw() - 1.0), 0.0, 1e-4), "yaw {}", c.yaw());
	assert!(dist(out.eye, r1.eye) < 0.2);
	// A level change (reset) seeds again too, without a blend.
	c.reset();
	let out = c.step(&PoseIn::idle(3, [0.0; 3], r0), &mut NoWalls).unwrap();
	assert!(close(c.yaw(), 0.0, 1e-4) && dist(out.eye, r0.eye) < 0.2);
}

#[test]
fn smooth_pivot_lags_a_running_hero_inside_the_leash_and_catches_up() {
	let mut c = cam();
	let mut hero = [0.0; 3];
	let r = retail_at(hero, 0.0, 0.35, 7.0);
	let mut i = PoseIn::idle(1, hero, r);
	i.smooth = true;
	c.step(&i, &mut NoWalls);
	let mut max_lag = 0.0f32;
	for f in 2..120 {
		hero[0] += 0.12; // 6 m/s
		i.field = f;
		i.hero = hero;
		c.step(&i, &mut NoWalls);
		let lag = hero[0] - c.pivot()[0];
		assert!(lag < 1.61, "the leash holds: {lag}");
		max_lag = max_lag.max(lag);
	}
	assert!(max_lag > 0.3, "the pivot lags: {max_lag}");
	for f in 120..400 {
		i.field = f;
		c.step(&i, &mut NoWalls);
	}
	assert!(dist(c.pivot(), add3(hero, [0.0, 0.0, PIVOT])) < 0.01);
}

#[test]
fn smooth_recentres_toward_the_run_direction_but_not_toward_the_camera() {
	// The camera looks along +x. Running along +y (sideways) with no camera input swings the view toward +y.
	let run = |dir: [f32; 3], fields: u32| {
		let mut c = cam();
		let mut hero = [0.0; 3];
		let r = retail_at(hero, 0.0, 0.35, 7.0);
		let mut i = PoseIn::idle(1, hero, r);
		i.smooth = true;
		c.step(&i, &mut NoWalls);
		for f in 2..fields {
			hero = add3(hero, scale3(dir, 0.12));
			i.field = f;
			i.hero = hero;
			c.step(&i, &mut NoWalls);
		}
		c
	};
	let side = run([0.0, 1.0, 0.0], 400);
	assert!(side.yaw() > 0.3 && side.yaw() < 1.31 + 0.1, "yaw {}", side.yaw());
	let toward = run([-1.0, 0.0, 0.0], 400);
	assert!(toward.yaw().abs() < 0.01, "running toward the camera does not swing it: {}", toward.yaw());
	// Camera input resets the idle time: with the mouse moving every field there is no recentre (only the player's own turn).
	let mut c = cam();
	let mut hero = [0.0; 3];
	let r = retail_at(hero, 0.0, 0.35, 7.0);
	let mut i = PoseIn::idle(1, hero, r);
	i.smooth = true;
	i.input.look = [1, 0];
	c.step(&i, &mut NoWalls);
	let y0 = c.yaw();
	for f in 2..200 {
		hero[1] += 0.12;
		i.field = f;
		i.hero = hero;
		i.input.look = [0, 1];
		c.step(&i, &mut NoWalls);
	}
	assert!(close(c.yaw(), y0, 1e-4), "no recentre while the mouse is in use");
}

#[test]
fn smooth_ramps_the_right_stick_static_does_not() {
	let hero = [0.0; 3];
	let r = retail_at(hero, 0.0, 0.35, 7.0);
	let first = |smooth: bool| {
		let mut c = cam();
		let mut i = PoseIn::idle(1, hero, r);
		i.smooth = smooth;
		c.step(&i, &mut NoWalls);
		let y0 = c.yaw();
		i.field = 2;
		i.rstick = [0xff, 0x80];
		c.step(&i, &mut NoWalls);
		wrap(c.yaw() - y0)
	};
	let (st, sm) = (first(false), first(true));
	assert!(close(st, ROTATE_RATE, 1e-5), "Static turns at once: {st}");
	assert!(close(sm, ROTATE_RATE * 4.0 * DT, 1e-5), "Smooth ramps: {sm}");
}

#[test]
fn falling_looks_down_in_smooth_only() {
	let run = |smooth: bool| {
		let mut c = cam();
		let mut hero = [0.0, 0.0, 100.0];
		let r = retail_at(hero, 0.0, 0.35, 7.0);
		let mut i = PoseIn::idle(1, hero, r);
		i.smooth = smooth;
		c.step(&i, &mut NoWalls);
		let mut out = None;
		for f in 2..40 {
			hero[2] -= 0.2; // 10 m/s
			i.field = f;
			i.hero = hero;
			i.airborne = true;
			out = c.step(&i, &mut NoWalls);
		}
		angles(&out.unwrap().rows).1 - c.pitch()
	};
	assert!(run(true) > 0.05, "the bias looks down: {}", run(true));
	assert!(run(false).abs() < 1e-6);
}

#[test]
fn switching_the_feel_does_not_move_the_eye() {
	let mut c = cam();
	let hero = [3.0, 4.0, 5.0];
	let r = retail_at(hero, 0.7, 0.3, 7.0);
	let mut i = PoseIn::idle(1, hero, r);
	for f in 1..300 {
		i.field = f;
		c.step(&i, &mut NoWalls);
	}
	i.field = 300;
	let st = c.step(&i, &mut NoWalls).unwrap();
	i.field = 301;
	i.smooth = true;
	let sm = c.step(&i, &mut NoWalls).unwrap();
	assert!(dist(st.eye, sm.eye) < 1e-3, "Static -> Smooth: {}", dist(st.eye, sm.eye));
	i.field = 302;
	i.smooth = false;
	let back = c.step(&i, &mut NoWalls).unwrap();
	assert!(dist(sm.eye, back.eye) < 1e-3, "Smooth -> Static: {}", dist(sm.eye, back.eye));
}

#[test]
fn aiming_follows_the_hero_on_the_plain_follow_and_skips_the_recentre() {
	let mut c = cam();
	let mut hero = [0.0; 3];
	let r = retail_at(hero, 0.0, 0.35, 7.0);
	let mut i = PoseIn::idle(1, hero, r);
	i.smooth = true;
	i.input.strafe = true;
	c.step(&i, &mut NoWalls);
	assert!(c.aiming());
	for f in 2..300 {
		hero[1] += 0.12;
		i.field = f;
		i.hero = hero;
		c.step(&i, &mut NoWalls);
	}
	assert!(c.yaw().abs() < 1e-4, "no recentre while aiming: {}", c.yaw());
	assert!(hero[1] - c.pivot()[1] < 1.0, "the exponential follow keeps close");
}

/// A roof at height `z`: the segment test hits a straight-up segment that crosses it.
struct Roof {
	z: f32,
}

impl Probe for Roof {
	fn seg(&mut self, a: V3, b: V3) -> Option<V3> {
		let up = (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6 && a[2] < self.z && b[2] >= self.z;
		up.then_some([a[0], a[1], self.z])
	}
}

fn run_fields(c: &mut Cam, i: &mut PoseIn, n: u32, p: &mut dyn Probe) -> Pose {
	let mut last = None;
	for _ in 0..n {
		i.field += 1;
		last = c.step(i, p);
	}
	last.expect("a pose")
}

#[test]
fn the_strafe_aim_raises_the_view_and_letting_go_lowers_it_again() {
	let hero = [100.0, 50.0, 10.0];
	let mut c = cam();
	let mut i = PoseIn::idle(0, hero, retail_at(hero, 0.0, 0.3, 7.0));
	let rest = run_fields(&mut c, &mut i, 40, &mut NoWalls);
	i.input.strafe = true;
	let first = run_fields(&mut c, &mut i, 1, &mut NoWalls);
	assert!(first.eye[2] > rest.eye[2] && first.eye[2] < rest.eye[2] + 0.5, "the lift starts smoothly: {}", first.eye[2] - rest.eye[2]);
	let up = run_fields(&mut c, &mut i, 30, &mut NoWalls);
	assert!(close(up.eye[2] - rest.eye[2], crate::modcam_shoulder::AIM_LIFT, 1e-3), "raised by AIM_LIFT: {}", up.eye[2] - rest.eye[2]);
	assert!(close(c.lift(), crate::modcam_shoulder::AIM_LIFT, 1e-6));
	assert_eq!(up.rows, rest.rows, "the view direction is the player's: only the pivot moved");
	assert!(close(up.eye[0], rest.eye[0], 1e-3) && close(up.eye[1], rest.eye[1], 1e-3), "straight up");
	i.input.strafe = false;
	let down = run_fields(&mut c, &mut i, 30, &mut NoWalls);
	assert!(dist(down.eye, rest.eye) < 1e-3 && c.lift() == 0.0, "back where it was: {:?} vs {:?}", down.eye, rest.eye);
}

#[test]
fn a_roof_over_ratchet_limits_the_aim_lift() {
	let hero = [100.0, 50.0, 10.0];
	let mut c = cam();
	let mut i = PoseIn::idle(0, hero, retail_at(hero, 0.0, 0.3, 7.0));
	// The roof is 1.0 m over the pivot (chest + 0.9): 0.4 m stay free, the lift ends at 0.6.
	let mut roof = Roof { z: hero[2] + PIVOT + 1.0 };
	run_fields(&mut c, &mut i, 40, &mut roof);
	i.input.strafe = true;
	run_fields(&mut c, &mut i, 40, &mut roof);
	assert!(close(c.lift(), 0.6, 1e-3), "limited by the roof: {}", c.lift());
}

#[test]
fn a_hand_back_ends_the_aim_lift() {
	let hero = [100.0, 50.0, 10.0];
	let mut c = cam();
	let mut i = PoseIn::idle(0, hero, retail_at(hero, 0.0, 0.3, 7.0));
	i.input.strafe = true;
	run_fields(&mut c, &mut i, 20, &mut NoWalls);
	assert!(c.lift() > 1.0);
	i.retail_drives = true;
	run_fields(&mut c, &mut i, 1, &mut NoWalls);
	assert_eq!((c.lift(), c.aiming()), (0.0, false), "retail drives: no lift, no aim");
	assert!(c.blending());
}

/// A floor at height `z`: any segment that crosses it is hit at the crossing.
struct Floor {
	z: f32,
}

impl Probe for Floor {
	fn seg(&mut self, a: V3, b: V3) -> Option<V3> {
		if (a[2] < self.z) == (b[2] < self.z) {
			return None;
		}
		let t = (self.z - a[2]) / (b[2] - a[2]);
		Some([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, self.z])
	}
}

#[test]
fn the_pivot_moves_to_the_hero_when_the_eye_would_sit_under_his_platform() {
	// The hero stands on a platform (floor z 5); the lagging pivot is still beside the ledge, below the floor, so the boom from it
	// crosses nothing; the eye's line to his chest does.
	let (pivot, chest) = ([0.0, 0.0, 4.0], [0.0, 1.0, 5.9]);
	let moved = pull_to_chest(&FLAT, &mut Floor { z: 5.0 }, pivot, chest, 0.0, -0.3, 6.0);
	assert!(moved[2] > 4.5 && moved[2] <= 5.9 + 1e-4, "pivot pulled up: {moved:?}");
	// Open space, or a pivot at the chest: unchanged.
	assert_eq!(pull_to_chest(&FLAT, &mut NoWalls, pivot, chest, 0.0, -0.3, 6.0), pivot);
	assert_eq!(pull_to_chest(&FLAT, &mut Floor { z: 5.0 }, chest, chest, 0.0, -0.3, 6.0), chest);
}
