//! Magnetic-wall tests of the Modern camera (modcam_frame.rs + the frame in modcam_pose.rs): the mouse orbits the hero's own up, the view
//! rolls with it, the transitions from and to flat ground are smooth, no aim lift on a wall. (R&C1: modcam/fp_look_tests.rs.)

use crate::modcam_frame::{Frame, FLAT, clean_up};
use crate::modcam_pose::*;

fn cam() -> Cam {
	Cam::new(Config::new(0.02, 7.0))
}

/// Retail's pose on a wall with hero up `u`: behind him along the in-plane axis `a`, a little above (pitch 0.3).
fn retail_on(u: V3, hero: V3) -> Pose {
	let (fr, _) = Frame::carry(FLAT.a, u);
	let pivot = fr.lift(hero, PIVOT);
	Pose { eye: fr.eye(pivot, 0.0, 0.3, 7.0), rows: fr.rows(0.0, 0.3) }
}

fn input(hero: V3, u: V3, field: u32, look: [i16; 2], retail: Pose) -> PoseIn {
	let mut i = PoseIn::idle(field, hero, retail);
	i.up = u;
	i.input.look = look;
	i
}

#[test]
fn on_a_wall_the_mouse_orbits_hero_up_and_the_view_rolls_with_it() {
	let u = clean_up([0.0, 0.6, 0.8]);
	let hero = [10.0, 5.0, 2.0];
	let retail = retail_on(u, hero);
	let mut c = cam();
	let mut last = None;
	for n in 0..40 {
		last = c.step(&input(hero, u, n, [if n < 30 { 12 } else { 0 }, 0], retail), &mut NoWalls);
	}
	let p = last.expect("a pose");
	let pivot = add3(hero, scale3(u, PIVOT));
	// The boom is straight behind the view: pivot = eye + forward x length.
	assert!(len3(sub(add3(p.eye, scale3(p.rows[0], c.len())), pivot)) < 1e-3, "pivot on the view axis");
	// Rows orthonormal; the pitch stays 0.3 about HERO up, so up . hero_up = cos 0.3 however far the mouse turned the yaw.
	for i in 0..3 {
		assert!((len3(p.rows[i]) - 1.0).abs() < 1e-4);
	}
	assert!(dot3(p.rows[0], p.rows[2]).abs() < 1e-4);
	assert!((dot3(p.rows[2], u) - 0.3f32.cos()).abs() < 1e-3, "up row tilts only by the pitch: {}", dot3(p.rows[2], u));
	assert!(dot3(p.rows[2], [0.0, 0.0, 1.0]) < 0.99, "the view is rolled, not world-upright");
	assert!(c.yaw().abs() > 0.1, "the mouse turned the yaw");
}

#[test]
fn a_wall_does_not_make_the_camera_jump_on_the_first_field() {
	let u = clean_up([1.0, 0.0, 0.0]);
	let hero = [0.0, 0.0, 0.0];
	let retail = retail_on(u, hero);
	let mut c = cam();
	let p = c.step(&input(hero, u, 0, [0, 0], retail), &mut NoWalls).unwrap();
	assert!(len3(sub(p.eye, retail.eye)) < 0.05, "seeded from retail's pose: {:?} vs {:?}", p.eye, retail.eye);
	for i in 0..3 {
		assert!(len3(sub(p.rows[i], retail.rows[i])) < 0.02, "row {i}");
	}
}

#[test]
fn walking_onto_a_wall_and_back_turns_the_view_by_small_steps() {
	let hero = [0.0, 0.0, 0.0];
	let mut c = cam();
	let mut prev: Option<Pose> = None;
	let mut worst = 0.0f32;
	// The smoothed up swings from z to y over 60 fields (retail's spring), holds, and swings back.
	for n in 0..240u32 {
		let a = if n < 60 { n as f32 / 60.0 } else if n < 120 { 1.0 } else { (1.0 - (n - 120) as f32 / 60.0).max(0.0) } * 1.2;
		let u = clean_up([0.0, a.sin(), a.cos()]);
		let retail = retail_on(u, hero);
		let p = c.step(&input(hero, u, n, [0, 0], retail), &mut NoWalls).unwrap();
		if let Some(q) = prev {
			for i in 0..3 {
				worst = worst.max(len3(sub(p.rows[i], q.rows[i])));
			}
		}
		prev = Some(p);
	}
	assert!(worst < 0.05, "no row moves more than 0.05 in a field: {worst}");
	// Back on flat ground the pose is a plain world-frame pose again (up row has no roll: left row has z = 0).
	let p = prev.unwrap();
	assert!(p.rows[1][2].abs() < 1e-4, "no roll on flat ground: {:?}", p.rows[1]);
}

#[test]
fn no_aim_lift_on_a_wall_but_on_flat_ground() {
	let hero = [0.0, 0.0, 0.0];
	let wall_up = clean_up([1.0, 0.0, 0.0]);
	for (u, lifts) in [(wall_up, false), ([0.0, 0.0, 1.0], true)] {
		let retail = retail_on(u, hero);
		let mut c = cam();
		for n in 0..60 {
			let mut i = input(hero, u, n, [0, 0], retail);
			i.input.strafe = true;
			c.step(&i, &mut NoWalls);
		}
		assert_eq!(c.lift() > 0.1, lifts, "lift {} on up {u:?}", c.lift());
	}
}
