//! The Modern camera's orbit frame for magnetic walls (lane scratch_rc2_mag, 2026-11-11): pure maths plus the RAM lookup of the hero's up.
//!
//! The yaw is measured from `a` toward `b`, the pitch toward `u` (the hero's up), `b = u x a`. On flat ground (`u` = world z, `a` = world x)
//! it IS the world frame and every formula is the plain `modcam_pose::rows` / `eye` (bit-exact with the camera before the frame existed).
//! With the Magneboots on a wall the game's smoothed hero up turns away from z: the mouse then orbits Ratchet's own up and the view
//! rolls with it, as retail's follow camera does (the world turns around an upright hero: the camera record's up row carries the roll).
//!
//! Where the hero's up comes from [C, R&C2 PAL, state bank fg1]: the camera pre-update (Aranos 0x2da298-like, one copy per level overlay,
//! the twin of R&C1's FUN_0020e570) loads `s6 = 0x18a160` (the gravity direction word in the boot data: DOWN, (0,0,-1) on flat ground) and
//! `s5 = <camera globals>`, scales the gravity by -1 and springs `s5 + 0x20` (the smoothed hero up, k 0.015 d 0.2) toward it. The base
//! differs per level (0x1b8c20 level 26, 0x1b9560 level 2, ...), so `find_up` takes it from the `lui s7 / addiu s5` pair next to the
//! `addiu s6, v0, -0x5ea0` that names the gravity word. Level 26 states 0115-0123 hold the rolled views (0120: gravity (0.61,-0.79,0), up
//! (-0.62,0.78,0.05)).

use crate::modcam_pose::{V3, Rows, add3, cross3, dot3, len3, rows, scale3, sub, unit3};
use rac_engine::fmath::Fm;

/// The gravity word (boot data) the pre-update scales by -1: `addiu s6, v0, -0x5ea0` after `lui v0, 0x19`.
const GRAVITY: u32 = 0x18a160;
const ADDIU_S6_GRAVITY: u32 = 0x2456_a160;
/// Offset of the smoothed hero up in the camera globals (`s5 + 0x20`).
const UP_OFF: u32 = 0x20;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
	pub a: V3,
	pub b: V3,
	pub u: V3,
}

pub const FLAT: Frame = Frame { a: [1.0, 0.0, 0.0], b: [0.0, 1.0, 0.0], u: [0.0, 0.0, 1.0] };

impl Frame {
	pub fn flat(&self) -> bool {
		*self == FLAT
	}

	/// The frame for hero up `u`, carrying the reference axis `a` of the last field (parallel transport: `a` projected onto the new
	/// tangent plane, so the camera turns by the least angle as the wall curves). Returns the frame and the yaw to add (the turn of the
	/// carried axis against world x when `u` came back to world z, folded into the yaw so the heading stays).
	pub fn carry(a: V3, u: V3) -> (Frame, f32) {
		if u == [0.0, 0.0, 1.0] {
			return (FLAT, if a == FLAT.a { 0.0 } else { a[1].fatan2(a[0]) });
		}
		let mut p = sub(a, scale3(u, dot3(a, u)));
		if len3(p) < 1e-3 {
			// a was along u: any axis across it (world x, else world y).
			let t = if u[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
			p = sub(t, scale3(u, dot3(t, u)));
		}
		let a = unit3(p);
		(Frame { a, b: cross3(u, a), u }, 0.0)
	}

	/// Camera rows for yaw / pitch in this frame (`rows` on flat ground): row 0 forward, row 1 left, row 2 up.
	pub fn rows(&self, yaw: f32, pitch: f32) -> Rows {
		if self.flat() {
			return rows(yaw, pitch);
		}
		let (sy, cy) = yaw.fsin_cos();
		let (sp, cp) = pitch.fsin_cos();
		let comb = |x: f32, y: f32, z: f32| [self.a[0] * x + self.b[0] * y + self.u[0] * z, self.a[1] * x + self.b[1] * y + self.u[1] * z, self.a[2] * x + self.b[2] * y + self.u[2] * z];
		[comb(cy * cp, sy * cp, -sp), comb(-sy, cy, 0.0), comb(cy * sp, sy * sp, cp)]
	}

	/// The eye for a pivot, yaw / pitch and boom length.
	pub fn eye(&self, pivot: V3, yaw: f32, pitch: f32, len: f32) -> V3 {
		let f = self.rows(yaw, pitch)[0];
		[pivot[0] - f[0] * len, pivot[1] - f[1] * len, pivot[2] - f[2] * len]
	}

	/// `p` moved `k` along hero up.
	pub fn lift(&self, p: V3, k: f32) -> V3 {
		if self.flat() {
			[p[0], p[1], p[2] + k]
		} else {
			add3(p, scale3(self.u, k))
		}
	}

	/// The part of `v` along hero up (`v[2]` on flat ground).
	pub fn along(&self, v: V3) -> f32 {
		if self.flat() { v[2] } else { dot3(v, self.u) }
	}

	/// `v` without its part along hero up.
	pub fn across(&self, v: V3) -> V3 {
		if self.flat() { [v[0], v[1], 0.0] } else { sub(v, scale3(self.u, dot3(v, self.u))) }
	}

	/// `(x, y)` of `v` in the frame's tangent plane (world x, y on flat ground).
	pub fn xy(&self, v: V3) -> (f32, f32) {
		if self.flat() { (v[0], v[1]) } else { (dot3(v, self.a), dot3(v, self.b)) }
	}
}

/// A hero up as the camera uses it: unit length (else world z: the level loads with zeros), snapped to exactly z within 1e-4 so flat
/// ground runs the unchanged formulas.
pub fn clean_up(u: V3) -> V3 {
	let l = len3(u);
	if !l.is_finite() || (l - 1.0).abs() > 0.1 {
		return [0.0, 0.0, 1.0];
	}
	let u = [u[0] / l, u[1] / l, u[2] / l];
	if len3(sub(u, [0.0, 0.0, 1.0])) < 1e-4 { [0.0, 0.0, 1.0] } else { u }
}

/// The address of the smoothed hero up in a RAM image (None while the level's overlay is not loaded): the camera globals base from the
/// `lui s7, hi; addiu s5, s7, lo` within 8 words of the gravity `addiu s6, v0, -0x5ea0`, plus `UP_OFF`. The gravity word itself must
/// hold a unit vector (the check that the code is live and the base plausible).
pub fn find_up(ram: &[u8]) -> Option<u32> {
	let w = |a: u32| -> Option<u32> { ram.get(a as usize..a as usize + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])) };
	let f = |a: u32| w(a).map(f32::from_bits);
	let g = [f(GRAVITY)?, f(GRAVITY + 4)?, f(GRAVITY + 8)?];
	if (len3(g) - 1.0).abs() > 0.1 {
		return None;
	}
	let (lo, hi) = (0x200000u32, (ram.len() as u32).min(0x480000));
	let at = (lo..hi).step_by(4).find(|&a| w(a) == Some(ADDIU_S6_GRAVITY))?;
	let near = (at - 32..at + 48).step_by(4);
	let hi_imm = near.clone().filter_map(w).find(|x| x >> 16 == 0x3c17).map(|x| x & 0xffff)?;
	let lo_imm = near.filter_map(w).find(|x| x >> 16 == 0x26f5).map(|x| x as u16 as i16 as i32)?;
	let base = (hi_imm << 16).wrapping_add(lo_imm as u32);
	let up = base.wrapping_add(UP_OFF);
	(up as usize + 12 <= ram.len() && up >= 0x100000).then_some(up)
}

/// Reads the hero up at `addr` (`find_up`), cleaned.
pub fn read_up(ram: &[u8], addr: u32) -> V3 {
	let f = |k: u32| {
		let a = (addr + k) as usize;
		ram.get(a..a + 4).map_or(0.0, |b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
	};
	clean_up([f(0), f(4), f(8)])
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn flat_is_the_world_frame() {
		let (fr, turn) = Frame::carry(FLAT.a, [0.0, 0.0, 1.0]);
		assert!(fr.flat() && turn == 0.0);
		for (y, p) in [(0.3f32, 0.2f32), (-2.0, 1.0)] {
			assert_eq!(fr.rows(y, p), rows(y, p));
		}
		assert_eq!(fr.lift([1.0, 2.0, 3.0], 0.9), [1.0, 2.0, 3.9]);
	}

	#[test]
	fn a_wall_frame_is_orthonormal_with_the_wall_normal_as_up() {
		// Level 26 state 0120: smoothed up (-0.62, 0.78, 0.05).
		let u = clean_up([-0.62, 0.78, 0.05]);
		let (fr, _) = Frame::carry(FLAT.a, u);
		assert!(!fr.flat());
		let r = fr.rows(0.4, 0.3);
		for i in 0..3 {
			assert!((len3(r[i]) - 1.0).abs() < 1e-5);
		}
		assert!(dot3(r[0], r[1]).abs() < 1e-5 && dot3(r[0], r[2]).abs() < 1e-5);
		// Pitch 0: forward lies in the wall plane; the up row is hero up.
		let r0 = fr.rows(0.4, 0.0);
		assert!(dot3(r0[0], u).abs() < 1e-5 && len3(sub(r0[2], u)) < 1e-5);
		// The eye is behind the pivot along forward.
		let e = fr.eye([1.0, 1.0, 1.0], 0.4, 0.3, 5.0);
		assert!(len3(sub(add3(e, scale3(r[0], 5.0)), [1.0, 1.0, 1.0])) < 1e-4);
	}

	#[test]
	fn the_frame_returns_to_world_and_keeps_the_heading() {
		let (wall, _) = Frame::carry(FLAT.a, clean_up([0.0, 0.7, 0.714]));
		let (back, turn) = Frame::carry(wall.a, [0.0, 0.0, 1.0]);
		assert!(back.flat());
		assert!(turn.abs() < 1e-5, "a tilt about x keeps world x as the carried axis: turn {turn}");
	}

	#[test]
	fn bad_ups_are_world_z() {
		assert_eq!(clean_up([0.0; 3]), [0.0, 0.0, 1.0]);
		assert_eq!(clean_up([f32::NAN, 0.0, 0.0]), [0.0, 0.0, 1.0]);
		assert_eq!(clean_up([0.00001, 0.0, 1.0]), [0.0, 0.0, 1.0]);
	}

	#[test]
	fn find_up_reads_the_base_from_the_code_next_to_the_gravity_word() {
		let mut ram = vec![0u8; 0x400000];
		let put = |ram: &mut Vec<u8>, a: usize, v: u32| ram[a..a + 4].copy_from_slice(&v.to_le_bytes());
		put(&mut ram, 0x18a160 + 8, (-1.0f32).to_bits());
		// lui v0,0x19 / sd / ... / addiu s6,v0,-0x5ea0 ... lui s7,0x1c / addiu s5,s7,-0x73e0 (the Aranos-like copy at level 26's base 0x1b8c20).
		put(&mut ram, 0x2da29c, 0x3c02_0019);
		put(&mut ram, 0x2da2ac, ADDIU_S6_GRAVITY);
		put(&mut ram, 0x2da2b8, 0x3c17_001c);
		put(&mut ram, 0x2da2c8, 0x26f5_8c20);
		assert_eq!(find_up(&ram), Some(0x1b8c20 + 0x20));
		put(&mut ram, 0x1b8c40 + 0, 0.6f32.to_bits());
		put(&mut ram, 0x1b8c40 + 4, 0.0f32.to_bits());
		put(&mut ram, 0x1b8c40 + 8, 0.8f32.to_bits());
		let u = read_up(&ram, 0x1b8c40);
		assert!((u[0] - 0.6).abs() < 1e-6 && (u[2] - 0.8).abs() < 1e-6);
	}
}
