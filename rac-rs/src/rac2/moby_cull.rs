//! [PC] R&C2's moby cull verdict, re-run on the PC side (BATCH A CULL; R&C1's rac1/retail/moby_state.rs `DrawView::culls` and
//! props_sync.rs `SideOnly::inside_lag`): the game draws only the mobies whose `+0x31` it set, after the tests of rac2/notes_cull.md
//! (occlusion mask, hidden flags, draw distance, near plane, the side planes of its own 4:3 / 16:9 view). Two PC uses:
//! - Culling Off (Graphics -> World -> Culling): every moby the game left out for a camera test or its draw distance is drawn too
//!   (the engine's `culled_src` draws a `cast` prop in the main pass only with Culling Off, and `open_alphas` drops the distance fade).
//! - Side restore: a moby the game left out ONLY at a side plane is drawn when it is inside the planes of the view rac-rs draws: a
//!   window wider than the game's picture (the Aspect ratio row's box, host/frame.rs `fill`), or a smoothing-blended camera that
//!   trails the newest field's (the pop-in at the screen corners). Retail's own verdict in RAM is never changed.
//!
//! The game thread computes `Cut` only while `WANT` asks for it (the render thread sets it every redraw): with Culling On, no wide
//! box and no blending (lockstep shots) nothing is read and the scene is the one before.

use crate::addrs::{self, Addrs};
use crate::scene::View;
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};

/// What the render thread wants this field: `SIDE` = side-only mobies (wide box or blended frames), `OFF` = every culled moby.
pub static WANT: AtomicU8 = AtomicU8::new(0);
pub const SIDE: u8 = 1;
pub const OFF: u8 = 2;
/// Draw distance Medium / High (row 5): the mobies the game left out ONLY for their draw distance, but that lie within `scale` times
/// that distance (up to `cap` m), are drawn too (R&C1's fardraw.rs scale / cap, set by `set_far`).
pub const FAR: u8 = 4;
static FAR_SCALE: AtomicU32 = AtomicU32::new(0);
static FAR_CAP: AtomicU32 = AtomicU32::new(0);

/// The far draw of this redraw: (scale, cap in m), None = Low (the game's own draw distances).
pub fn set_far(f: Option<(f32, f32)>) {
	let (s, c) = f.unwrap_or((0.0, 0.0));
	FAR_SCALE.store(s.to_bits(), Ordering::Relaxed);
	FAR_CAP.store(c.to_bits(), Ordering::Relaxed);
}

fn far() -> Option<(f32, f32)> {
	let s = f32::from_bits(FAR_SCALE.load(Ordering::Relaxed));
	(s > 1.0).then(|| (s, f32::from_bits(FAR_CAP.load(Ordering::Relaxed))))
}

pub fn want() -> u8 {
	WANT.load(Ordering::Relaxed)
}

pub fn set_want(w: u8) {
	WANT.store(w, Ordering::Relaxed);
}

/// The draw-distance cap in metres [I]: R&C1's global is 500 at level setup (moby_state.rs `DrawView::cap`); R&C2's word (the
/// kernel's `pminw` source) is not found yet (notes_cull.md T5-1 OPEN). A lower live cap only makes more mobies `Far`.
const CAP: i32 = 500;
/// The near plane of the moby draw (1/1024 m) [C: notes_cull.md, `32 - (w + z)`].
const NEAR: f32 = 32.0;
/// Growth of the sphere radius for the side restore (1/1024 m): a blended frame's mobies lie between two fields while the cut is
/// the newest field's (R&C1's `ViewLag::grow` covers the same).
const GROW: f32 = 256.0;
/// Offset of the 128-byte occlusion mask from the camera basis [C: every level, 22 fg1 states; notes_cull.md T5-3/T5-4].
const MASK_OFF: u32 = 0x1b00;

/// Why the game did not draw a moby that is still in the world (R&C1's `Culled`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
	/// Only a camera test (occlusion mask, near or side plane).
	View,
	/// Its draw distance (+0x32 under the cap), maybe a camera test too.
	Far,
}

/// The cut of one moby the game left out: why, whether a side plane alone rejected it, and its bounding sphere in the world
/// (centre and radius, 1/1024 m).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cut {
	pub why: Why,
	pub side_only: bool,
	/// A `Far` cut that the far draw (`set_far`) brings back: the scaled draw distance holds and no other test rejects it.
	pub far_ok: bool,
	pub sphere: [f32; 4],
}

/// The camera side of the game's moby draw: eye (1/1024 m), the view rows, the side tangents and their `sqrt(1 + t^2)`, the
/// occlusion mask.
pub struct DrawView {
	eye: [f32; 3],
	rows: [[f32; 3]; 3],
	t: [f32; 2],
	s: [f32; 2],
	mask: [u8; 128],
}

/// The tests of one moby, true = rejects (R&C1's `Culls`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Tests {
	occ: bool,
	hidden: bool,
	dist: bool,
	never: bool,
	near: bool,
	side: bool,
}

impl Tests {
	/// R&C1's `Culls::why`: a moby with no draw distance is hidden by design; hidden flags keep it out whatever Culling says.
	fn cut(&self) -> Option<(Why, bool)> {
		if self.never || self.hidden {
			return None;
		}
		let why = if self.dist {
			Why::Far
		} else if self.occ || self.near || self.side {
			Why::View
		} else {
			return None;
		};
		Some((why, self.side && !(self.occ || self.dist || self.near)))
	}
}

impl DrawView {
	pub fn read(m: &ee_oracle::Machine, a: &Addrs) -> Option<DrawView> {
		let b = a.basis;
		let t = [m.rdf(a.half) / (m.rdf(a.proj) * addrs::CAM_PROJ_UNIT), m.rdf(a.half + 4) / (m.rdf(a.proj + 0x14) * addrs::CAM_PROJ_UNIT)];
		let ok = t.iter().all(|x| x.is_finite() && *x > 0.0);
		ok.then(|| DrawView {
			eye: m.rdv3(b + 0x30),
			rows: [m.rdv3(b), m.rdv3(b + 0x10), m.rdv3(b + 0x20)],
			t,
			s: t.map(|x| (1.0 + x * x).sqrt()),
			mask: std::array::from_fn(|i| m.rd8(b + MASK_OFF + i as u32)),
		})
	}

	/// The cut of the moby record at `a` that the game did not draw; None = drawn by design rules (hidden, no draw distance) or by
	/// none of the tests.
	pub fn cut(&self, m: &ee_oracle::Machine, a: u32) -> Option<Cut> {
		let c = m.rdv3(a);
		let w = m.rdf(a + 0xc);
		if !c.iter().all(|x| x.is_finite()) || !w.is_finite() {
			return None;
		}
		let draw = m.rd16(a + 0x32) as i16 as i32;
		let tests = self.tests(c, w, draw, m.rd16(a + 0x34), m.rd8(a + 0x36), m.rd8(a + 0x37));
		let (why, side_only) = tests.cut()?;
		let far_ok = why == Why::Far && far().is_some_and(|(scale, cap)| self.tests_at(c, w, ((draw as f32 * scale).min(cap) * 1024.0).floor(), m.rd16(a + 0x34), m.rd8(a + 0x36), m.rd8(a + 0x37)).cut().is_none());
		Some(Cut { why, side_only, far_ok, sphere: [c[0], c[1], c[2], w] })
	}

	fn tests(&self, c: [f32; 3], w: f32, draw: i32, flags: u16, groups: u8, cell: u8) -> Tests {
		self.tests_at(c, w, (draw.min(CAP) << 10) as f32, flags, groups, cell)
	}

	/// The tests with the draw limit `limit` in 1/1024 m (the game's: `draw.min(CAP) << 10`).
	fn tests_at(&self, c: [f32; 3], w: f32, limit: f32, flags: u16, groups: u8, cell: u8) -> Tests {
		let d: [f32; 3] = std::array::from_fn(|k| c[k] - self.eye[k]);
		let dot = |k: usize| (d[0] * self.rows[0][k] + d[1] * self.rows[1][k]) + d[2] * self.rows[2][k];
		let (vx, vy, vz) = (dot(0), dot(1), dot(2));
		Tests {
			occ: groups & self.mask[cell as usize & 0x7f] == 0,
			hidden: flags & 0x81 != 0,
			dist: ((limit - w) - (vz - w)).is_sign_negative(),
			never: limit <= 0.0,
			near: !(NEAR - (w + vz)).is_sign_negative(),
			side: (self.t[0] * vz - (vx.abs() - self.s[0] * w)).is_sign_negative() || (self.t[1] * vz - (vy.abs() - self.s[1] * w)).is_sign_negative(),
		}
	}
}

/// The planes of the view rac-rs draws this redraw: eye (m), basis (rows = world axes, columns = right, down, forward), the
/// horizontal and vertical side tangents.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Planes {
	pub eye: [f32; 3],
	pub basis: [[f32; 3]; 3],
	pub tan_h: f32,
	pub tan_v: f32,
}

impl Planes {
	/// The drawn view in a window `window` wide (width / height); `fill` = the window's sides are shown (no bars): the horizontal
	/// tangent follows the window, else the game's own picture.
	pub fn of(v: &View, window: f32, fill: bool) -> Planes {
		let fit = (v.aspect() / window).max(1.0);
		let tan_v = v.half[1] / v.fy * fit;
		let tan_h = if fill { tan_v * window } else { v.half[0] / v.fx };
		Planes { eye: v.eye, basis: v.basis, tan_h, tan_v }
	}

	/// The sphere (1/1024 m), grown by `GROW`, inside both side planes and past the near plane (R&C1's `SideOnly::inside`).
	pub fn inside(&self, sphere: [f32; 4]) -> bool {
		let d: [f32; 3] = std::array::from_fn(|k| sphere[k] - self.eye[k] * 1024.0);
		let dot = |k: usize| d[0] * self.basis[0][k] + d[1] * self.basis[1][k] + d[2] * self.basis[2][k];
		let (vx, vy, vz) = (dot(0), dot(1), dot(2));
		let w = sphere[3] + GROW;
		let (sx, sy) = ((1.0 + self.tan_h * self.tan_h).sqrt(), (1.0 + self.tan_v * self.tan_v).sqrt());
		w + vz > NEAR && !(self.tan_h * vz - (vx.abs() - sx * w)).is_sign_negative() && !(self.tan_v * vz - (vy.abs() - sy * w)).is_sign_negative()
	}
}

/// What this redraw restores (host/frame.rs): Culling Off, and the drawn view's planes when the side restore is on.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Restore {
	pub culling_off: bool,
	/// Draw distance Medium / High: a `far_ok` cut is shown.
	pub far: bool,
	pub side: Option<Planes>,
}

impl Restore {
	/// (shown, cast) of a moby the game left out: Culling Off casts it (the engine draws a cast prop in the main pass then, R&C1's
	/// View and Far alike); a side-only cut inside the drawn view is shown.
	pub fn verdict(&self, cut: &Cut) -> (bool, bool) {
		let shown = (cut.side_only && self.side.is_some_and(|p| p.inside(cut.sphere))) || (self.far && cut.far_ok);
		(shown, !shown && self.culling_off)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn view() -> DrawView {
		// Looking down world +y: columns right = +x, down = -z, forward = +y. 4:3 tangents of notes_cull.md.
		let t = [0.62, 0.35];
		DrawView { eye: [0.0; 3], rows: [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]], t, s: t.map(|x: f32| (1.0 + x * x).sqrt()), mask: [0xff; 128] }
	}

	const M: f32 = 1024.0;

	#[test]
	fn the_cut_names_why() {
		let v = view();
		let t = |c: [f32; 3], draw: i32| v.tests(c.map(|x| x * M), 1.0 * M, draw, 0, 1, 0).cut();
		assert_eq!(t([0.0, 20.0, 0.0], 64), None, "in front, inside every plane: drawn");
		assert_eq!(t([30.0, 20.0, 0.0], 64), Some((Why::View, true)), "beside the 4:3 plane only");
		assert_eq!(t([0.0, 100.0, 0.0], 64), Some((Why::Far, false)), "past its draw distance");
		assert_eq!(t([0.0, -20.0, 0.0], 64), Some((Why::View, false)), "behind the camera: near and side");
		assert_eq!(t([0.0, 100.0, 0.0], 0), None, "no draw distance: hidden by design");
		assert_eq!(v.tests([0.0, 20.0 * M, 0.0], M, 64, 0x80, 1, 0).cut(), None, "hidden flag");
		let mut occ = view();
		occ.mask[5] = 0;
		assert_eq!(occ.tests([0.0, 20.0 * M, 0.0], M, 64, 0, 1, 5).cut(), Some((Why::View, false)), "occlusion mask");
		assert_eq!(occ.tests([30.0 * M, 20.0 * M, 0.0], M, 64, 0, 1, 5).cut(), Some((Why::View, false)), "occluded and beside: not side only");
	}

	#[test]
	fn the_far_draw_brings_back_a_moby_past_only_its_draw_distance() {
		let v = view();
		let at = |y: f32, draw: f32| v.tests_at([0.0, y * M, 0.0], M, draw * M, 0, 1, 0).cut();
		assert_eq!(at(100.0, 64.0), Some((Why::Far, false)), "past 64 m: the game leaves it out");
		assert_eq!(at(100.0, (64.0f32 * 4.0).min(720.0)), None, "Medium (x4): inside 256 m, drawn");
		assert_eq!(at(300.0, 64.0 * 4.0), Some((Why::Far, false)), "past even the scaled distance");
		assert_eq!(at(300.0, (64.0f32 * 12.0).min(2000.0)), None, "High (x12): 768 m");
		assert_eq!(v.tests_at([0.0, -100.0 * M, 0.0], M, 256.0 * M, 0, 1, 0).cut().map(|c| c.0), Some(Why::View), "behind the camera stays out");
		let cut = Cut { why: Why::Far, side_only: false, far_ok: true, sphere: [0.0; 4] };
		assert_eq!(Restore { far: true, ..Restore::default() }.verdict(&cut), (true, false));
		assert_eq!(Restore::default().verdict(&cut), (false, false), "Low: as the game");
	}

	fn planes(fill: bool) -> Planes {
		let v = View { eye: [0.0; 3], basis: [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]], fx: 256.0 / 0.62, fy: 224.0 / 0.35, half: [256.0, 224.0] };
		Planes::of(&v, 21.0 / 9.0, fill)
	}

	#[test]
	fn a_side_cut_shows_only_inside_the_drawn_planes() {
		let cut = |x: f32, side_only: bool| Cut { why: Why::View, side_only, far_ok: false, sphere: [x * M, 20.0 * M, 0.0, M] };
		let wide = Restore { culling_off: false, far: false, side: Some(planes(true)) };
		// tan_v 0.35 x 21:9 = 0.82: x = 15 m at 20 m is outside 4:3 (0.62 x 20 + 1.2 = 13.6) but inside the wide view.
		assert_eq!(wide.verdict(&cut(15.0, true)), (true, false));
		assert_eq!(wide.verdict(&cut(40.0, true)), (false, false), "outside the wide view too");
		assert_eq!(wide.verdict(&cut(15.0, false)), (false, false), "not side only: never shown");
		let game = Restore { culling_off: false, far: false, side: Some(planes(false)) };
		assert_eq!(game.verdict(&cut(15.0, true)), (false, false), "the game's own planes keep it out");
		assert_eq!(Restore::default().verdict(&cut(15.0, true)), (false, false), "no restore: as the game");
		let off = Restore { culling_off: true, far: false, side: None };
		assert_eq!(off.verdict(&cut(40.0, false)), (false, true), "Culling Off casts it");
		assert_eq!(Restore { culling_off: true, far: false, side: Some(planes(true)) }.verdict(&cut(15.0, true)), (true, false));
	}

	#[test]
	fn the_grow_keeps_an_edge_moby() {
		// Just past the game's own plane by less than GROW: a blended frame between two fields keeps it.
		let p = planes(false);
		let edge = 0.62 * 20.0 + (1.0f32 + 0.62 * 0.62).sqrt() * 1.0 + 0.1;
		assert!(p.inside([edge * M, 20.0 * M, 0.0, M]));
		assert!(!p.inside([(edge + 1.0) * M, 20.0 * M, 0.0, M]));
	}
}
