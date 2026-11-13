//! [PC] R&C2 frame smoothing (engine split stage E6; pc_options `smoothing`, on by default as in R&C1): on a display faster than the
//! game's 50 fields a second each redraw draws the world between the field before the newest and the newest one, at the redraw's
//! place between their arrivals (one field of display latency, the engine's fixed-step scheme, engine/viewer/smooth.rs). Blended:
//! the camera (eye, axes, focal lengths) and each moby seen in both fields (same record and class): position, axes, scale and its
//! skinning matrices. Snaps (the newest field drawn as is): a camera or moby that moved more than `SNAP_DIST` m or turned past
//! `SNAP_TURN` in one field (a cut, a teleport, a respawn), a new moby, another level. Rotations (the moby's axes and each joint's
//! matrix) turn by quaternion slerp, as R&C1's engine smoothing does, so a fast bone keeps its size between two fields. The shadow
//! records are the nearer field's (not blended). The game's HUD and effects (the GS overlay) stay at the newest field. Nothing here reaches the game.

use crate::scene::{Moby, Scene, View};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

/// Farthest move (metres) and least cos of the turn between two fields that still blends.
const SNAP_DIST: f32 = 4.0;
const SNAP_TURN: f32 = 0.7;

#[derive(Default)]
pub struct Smooth {
	/// The newest field seen, its arrival, and the field before it.
	last: Option<Scene>,
	at: Option<Instant>,
	before: Option<Scene>,
	/// The rotation / stretch split of each moby's matrices in those two fields, made once when a field is first blended (not at every
	/// redraw: review B3/4 R6); the newest field's becomes the field before's when the next one arrives.
	last_dec: Cache,
	before_dec: Cache,
	/// The last `scene`'s blend: (the field before the newest, its place 0..1 towards the newest), None = the newest drawn as is
	/// (also across a camera cut). The water (water.rs) follows the camera with it.
	blended: Option<(u64, f32)>,
}

impl Smooth {
	/// The scene this redraw draws: `newest` (the game's latest field) as is when `on` is false, else blended from the field
	/// before it by `at` (window shots: the frame's place in its field) or the time since the newest arrived, in fields (0..1).
	pub fn scene(&mut self, newest: &Scene, on: bool, at: Option<f32>) -> Scene {
		if self.last.as_ref().is_none_or(|l| l.field != newest.field) {
			self.before = self.last.take();
			self.before_dec = std::mem::take(&mut self.last_dec);
			self.last = Some(newest.clone());
			self.at = Some(Instant::now());
		}
		let alpha = at.unwrap_or_else(|| self.at.map_or(1.0, |t| t.elapsed().as_secs_f32() / rac_engine::field::field_dt()).clamp(0.0, 1.0));
		self.blended = None;
		match (&self.before, on) {
			(Some(b), true) if b.level == newest.level && b.field + 1 == newest.field => {
				if let (Some(va), Some(vb)) = (&b.view, &newest.view) {
					self.blended = (near(va.eye, vb.eye) && same_axes(&va.basis, &vb.basis)).then_some((b.field, alpha));
				}
				blend(b, newest, alpha, (&mut self.before_dec, &mut self.last_dec))
			}
			_ => newest.clone(),
		}
	}

	/// The last `scene`'s blend (`blended`).
	pub fn blended(&self) -> Option<(u64, f32)> {
		self.blended
	}
}

/// `a` -> `b` at `t`; `dec` = the decompositions of `a`'s and `b`'s mobies (`Cache`).
fn blend(a: &Scene, b: &Scene, t: f32, dec: (&mut Cache, &mut Cache)) -> Scene {
	let view = match (&a.view, &b.view) {
		(Some(va), Some(vb)) => Some(view(va, vb, t)),
		_ => b.view,
	};
	let (ca, cb) = dec;
	// A moby the game drew in the older field only (it left the view, or was culled, in the newest): the frame between the two fields
	// still shows it at the older field's pose, so it goes at the moment the camera reaches the newest field and not one blend early
	// (pops at the screen edges, round 3 #1). One that is gone from RAM lingers for that one blend only.
	// Not across a camera cut (`view` snapped to `b`): the older field's mobies would show for one frame in the new shot.
	let cut = match (&a.view, &b.view) {
		(Some(va), Some(vb)) => !near(va.eye, vb.eye) || !same_axes(&va.basis, &vb.basis),
		(None, None) => false,
		_ => true,
	};
	let older: Vec<&Moby> = if cut {
		Vec::new()
	} else {
		let now: std::collections::HashSet<u32> = b.mobies.iter().filter(|m| m.cut.is_none()).map(|m| m.addr).collect();
		a.mobies.iter().filter(|m| m.cut.is_none() && !now.contains(&m.addr)).collect()
	};
	// [PC] A moby the game left out in the newest field (`Moby::cut`, moby_cull.rs) that it drew in the older one is that older drawn
	// moby above; the others blend from the older field's entry as the drawn ones do (the cut stays the newest field's).
	let old_drawn: std::collections::HashSet<u32> = older.iter().map(|m| m.addr).collect();
	let was: HashMap<u32, &Moby> = a.mobies.iter().map(|m| (m.addr, m)).collect();
	let mut mobies: Vec<Moby> = b
		.mobies
		.iter()
		.filter(|m| m.cut.is_none() || !old_drawn.contains(&m.addr))
		.map(|m| was.get(&m.addr).filter(|p| p.key == m.key).map_or_else(|| m.clone(), |p| moby(p, m, t, (&mut *ca, &mut *cb))))
		.collect();
	mobies.extend(older.into_iter().cloned());
	// [b45 P7] The shadow records blend by caster moby (smooth_shadows.rs; R&C1 interp.rs): they no longer step at the game's rate.
	let shadows = crate::smooth_shadows::blend(&a.shadows, &a.shadow_owners, &b.shadows, &b.shadow_owners, t);
	Scene { view, mobies, shadows, ..b.clone() }
}

fn view(a: &View, b: &View, t: f32) -> View {
	if !near(a.eye, b.eye) || !same_axes(&a.basis, &b.basis) {
		return *b;
	}
	let basis = blend_rows(&a.basis, &b.basis, t);
	View { eye: lerp3(a.eye, b.eye, t), basis, fx: lerp(a.fx, b.fx, t), fy: lerp(a.fy, b.fy, t), half: b.half }
}

fn moby(a: &Moby, b: &Moby, t: f32, dec: (&mut Cache, &mut Cache)) -> Moby {
	let turned = (0..3).any(|r| dot(a.rows[r], b.rows[r]) < SNAP_TURN * len(a.rows[r]) * len(b.rows[r]));
	if !near(a.pos, b.pos) || turned {
		return b.clone();
	}
	let (da, db) = (dec.0.of(a), dec.1.of(b));
	let pose = match (&a.pose, &b.pose, &da.joints, &db.joints) {
		(Some(pa), Some(pb), Some(ja), Some(jb)) if pa.len() == pb.len() => Some(Arc::new((0..pa.len()).map(|i| blend_joint(&pa[i], &pb[i], &ja[i], &jb[i], t)).collect())),
		_ => b.pose.clone(),
	};
	let rows = unflat(&blend_joint(&flat(&a.rows), &flat(&b.rows), &da.rows, &db.rows, t));
	Moby { pos: lerp3(a.pos, b.pos, t), rows, scale: lerp(a.scale, b.scale, t), pose, ..b.clone() }
}

/// The basis rows of two fields: the rotation turns (not a per-element lerp, which shortens the rows by cos(turn / 2)).
fn blend_rows(a: &[[f32; 3]; 3], b: &[[f32; 3]; 3], t: f32) -> [[f32; 3]; 3] {
	unflat(&blend_affine(&flat(a), &flat(b), t))
}

/// A 3x3 as a 3x4 with no translation, and back.
fn flat(m: &[[f32; 3]; 3]) -> [f32; 12] {
	std::array::from_fn(|i| if i % 4 == 3 { 0.0 } else { m[i / 4][i % 4] })
}

fn unflat(m: &[f32; 12]) -> [[f32; 3]; 3] {
	std::array::from_fn(|r| [m[r * 4], m[r * 4 + 1], m[r * 4 + 2]])
}

type M3 = [[f32; 3]; 3];

/// A 3x4 matrix split once into what its blend needs (review B3/4 R6: the split used to be redone at every redraw, twice per joint):
/// `m3 = sign * R * S` with R a rotation (the unit quaternion `q`) and S the symmetric stretch of the polar decomposition. A mirrored
/// joint (det < 0, the negative scale of left / right symmetric rigs) is split as `-m3 = R * S` with `sign` -1, so it keeps its size
/// too instead of lerping.
#[derive(Clone, Copy, Debug)]
struct Split {
	q: [f32; 4],
	s: M3,
	sign: f32,
}

/// The split of `m`; None for a singular matrix (a hidden joint) or one the iteration cannot settle.
fn split(m: &[f32; 12]) -> Option<Split> {
	let m = m3(m);
	let sign = if det3(&m) < 0.0 { -1.0 } else { 1.0 };
	let (r, s) = polar(&std::array::from_fn(|i| std::array::from_fn(|j| m[i][j] * sign)))?;
	Some(Split { q: rac_engine::viewer::smooth::quat_of(&col_major(&r)), s, sign })
}

/// A moby's decomposed matrices (basis rows, joints) with what they were made from, so a changed scene is noticed.
struct Entry {
	src_rows: M3,
	rows: Option<Split>,
	src_pose: Option<Arc<Vec<[f32; 12]>>>,
	joints: Option<Vec<Option<Split>>>,
}

impl Entry {
	fn of(m: &Moby) -> Entry {
		Entry { src_rows: m.rows, rows: split(&flat(&m.rows)), src_pose: m.pose.clone(), joints: m.pose.as_ref().map(|p| p.iter().map(split).collect()) }
	}

	fn matches(&self, m: &Moby) -> bool {
		self.src_rows == m.rows
			&& match (&self.src_pose, &m.pose) {
				(Some(x), Some(y)) => Arc::ptr_eq(x, y),
				(None, None) => true,
				_ => false,
			}
	}
}

/// The splits of one field's mobies by record address, made when the moby is first blended in that field.
#[derive(Default)]
struct Cache(HashMap<u32, Entry>);

impl Cache {
	fn of(&mut self, m: &Moby) -> &Entry {
		let e = self.0.entry(m.addr).or_insert_with(|| Entry::of(m));
		if !e.matches(m) {
			*e = Entry::of(m);
		}
		e
	}
}

/// Two 3x4 affine matrices (rows of 4: the 3x3 then the translation; a joint's skinning matrix) at `t`, as a joint moves: the
/// rotation turns along the short arc (quaternion slerp), the translation moves in a line and the stretch (a scale in the matrix,
/// the symmetric factor of the polar decomposition `m = R S`) blends linearly. An element-wise lerp of two rotations shortens the
/// bone to cos(turn / 2) at the middle (a 120 degree turn in one field: half size), thin or vanishing limbs and weapons on a fast
/// display. A matrix that is singular has no rotation to speak of, and neither has a pair where only one is mirrored: they lerp.
/// (Uncached: `blend_joint` with the splits made here; the redraw path keeps them per field in `Cache`.)
fn blend_affine(a: &[f32; 12], b: &[f32; 12], t: f32) -> [f32; 12] {
	blend_joint(a, b, &split(a), &split(b), t)
}

/// `blend_affine` with the splits of `a` and `b` given.
fn blend_joint(a: &[f32; 12], b: &[f32; 12], sa: &Option<Split>, sb: &Option<Split>, t: f32) -> [f32; 12] {
	if a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5) {
		return *b;
	}
	let lerped = || std::array::from_fn(|i| lerp(a[i], b[i], t));
	let (Some(sa), Some(sb)) = (sa, sb) else { return lerped() };
	if sa.sign != sb.sign {
		return lerped();
	}
	let r = quat_mat(&rac_engine::viewer::smooth::slerp(sa.q, sb.q, t));
	let s: M3 = std::array::from_fn(|i| std::array::from_fn(|j| lerp(sa.s[i][j], sb.s[i][j], t)));
	let m = mul3(&r, &s);
	let out: [f32; 12] = std::array::from_fn(|i| if i % 4 == 3 { lerp(a[i], b[i], t) } else { sa.sign * m[i / 4][i % 4] });
	if out.iter().all(|x| x.is_finite()) { out } else { lerped() }
}

/// The rotation matrix (rows) of a unit quaternion (x, y, z, w), the convention of `quat_of`.
fn quat_mat(q: &[f32; 4]) -> M3 {
	let [x, y, z, w] = *q;
	[
		[1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w)],
		[2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w)],
		[2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y)],
	]
}

/// A rotation as the column-major 4x4 `quat_of` reads (columns 0..2 = the rotated basis).
fn col_major(r: &M3) -> [f32; 16] {
	std::array::from_fn(|i| match (i / 4, i % 4) {
		(3, 3) => 1.0,
		(c, k) if c < 3 && k < 3 => r[k][c],
		_ => 0.0,
	})
}

fn m3(a: &[f32; 12]) -> M3 {
	std::array::from_fn(|r| [a[r * 4], a[r * 4 + 1], a[r * 4 + 2]])
}

fn mul3(a: &M3, b: &M3) -> M3 {
	std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum()))
}

fn det3(m: &M3) -> f32 {
	m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// The inverse transpose (cofactors over the determinant); None when singular.
fn inv_t(m: &M3) -> Option<M3> {
	let det = det3(m);
	if !(det.abs() > 1e-12) {
		return None;
	}
	let cof = |r: usize, c: usize| {
		let (r1, r2, c1, c2) = ((r + 1) % 3, (r + 2) % 3, (c + 1) % 3, (c + 2) % 3);
		(m[r1][c1] * m[r2][c2] - m[r1][c2] * m[r2][c1]) / det
	};
	Some(std::array::from_fn(|r| std::array::from_fn(|c| cof(r, c))))
}

fn frob(m: &M3) -> f32 {
	m.iter().flatten().map(|x| x * x).sum::<f32>().sqrt()
}

/// Polar decomposition `m = R S`: R the rotation (scaled Newton iteration, one step for a matrix that already is a rotation),
/// S = R^T m the symmetric stretch. None for a singular or mirrored matrix (`split` hands it the negated matrix of a mirrored joint).
fn polar(m: &M3) -> Option<(M3, M3)> {
	if det3(m) <= 0.0 {
		return None;
	}
	let mut r = *m;
	for _ in 0..24 {
		let it = inv_t(&r)?;
		if det3(&r) <= 0.0 {
			return None;
		}
		// Scaling by sqrt(|R^-1| / |R|) keeps a small or large stretch from costing a dozen steps.
		let g = (frob(&it) / frob(&r)).sqrt();
		let next: M3 = std::array::from_fn(|i| std::array::from_fn(|j| 0.5 * (g * r[i][j] + it[i][j] / g)));
		let moved = (0..3).flat_map(|i| (0..3).map(move |j| (i, j))).map(|(i, j)| (next[i][j] - r[i][j]).abs()).fold(0.0, f32::max);
		r = next;
		if moved < 1e-6 {
			break;
		}
	}
	let rt: M3 = std::array::from_fn(|i| std::array::from_fn(|j| r[j][i]));
	let s = mul3(&rt, m);
	r.iter().flatten().chain(s.iter().flatten()).all(|x| x.is_finite()).then_some((r, s))
}

/// The view axes of two fields are close enough to blend (columns: right, down, forward; rows = world axes).
fn same_axes(a: &[[f32; 3]; 3], b: &[[f32; 3]; 3]) -> bool {
	(0..3).all(|j| {
		let (ca, cb) = ([a[0][j], a[1][j], a[2][j]], [b[0][j], b[1][j], b[2][j]]);
		dot(ca, cb) >= SNAP_TURN * len(ca) * len(cb)
	})
}

fn near(a: [f32; 3], b: [f32; 3]) -> bool {
	let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
	dot(d, d) <= SNAP_DIST * SNAP_DIST
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
	a + (b - a) * t
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
	std::array::from_fn(|i| lerp(a[i], b[i], t))
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
	a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn len(a: [f32; 3]) -> f32 {
	dot(a, a).sqrt()
}

#[cfg(test)]
mod tests {
	use super::*;

	fn moby_at(addr: u32, x: f32) -> Moby {
		Moby { key: 7, addr, pos: [x, 0.0, 0.0], rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], scale: 1.0, mask: 0, pose: None, light: ([0; 3], 0), tint: None, late: false, cut: None }
	}

	#[test]
	fn blends_moves_and_snaps_jumps() {
		let a = Scene { field: 1, mobies: vec![moby_at(0x100, 0.0), moby_at(0x200, 0.0)], ..Scene::default() };
		let b = Scene { field: 2, mobies: vec![moby_at(0x100, 1.0), moby_at(0x200, 10.0), moby_at(0x300, 5.0)], ..Scene::default() };
		let s = blend(&a, &b, 0.25, (&mut Cache::default(), &mut Cache::default()));
		let xs: Vec<f32> = s.mobies.iter().map(|m| m.pos[0]).collect();
		// A step blends; a 10 m jump and a new moby draw the newest field.
		assert_eq!(xs, vec![0.25, 10.0, 5.0]);
		let mut sm = Smooth::default();
		assert_eq!(sm.scene(&a, true, Some(0.5)).mobies[0].pos[0], 0.0);
		assert_eq!(sm.scene(&b, true, Some(0.5)).mobies[0].pos[0], 0.5);
		// Off: always the newest field.
		assert_eq!(sm.scene(&b, false, Some(0.5)).mobies[0].pos[0], 1.0);
	}

	#[test]
	fn a_moby_drawn_in_either_field_is_drawn_between_them() {
		let a = Scene { field: 1, mobies: vec![moby_at(0x100, 0.0), moby_at(0x200, 3.0)], ..Scene::default() };
		let b = Scene { field: 2, mobies: vec![moby_at(0x100, 1.0), moby_at(0x300, 5.0)], ..Scene::default() };
		let s = blend(&a, &b, 0.5, (&mut Cache::default(), &mut Cache::default()));
		let mut xs: Vec<(u32, f32)> = s.mobies.iter().map(|m| (m.addr, m.pos[0])).collect();
		xs.sort_by_key(|x| x.0);
		assert_eq!(xs, vec![(0x100, 0.5), (0x200, 3.0), (0x300, 5.0)]);
	}

	/// A moby the newest field left out (`cut`) that the older one drew stays the older drawn one; another left-out moby blends.
	#[test]
	fn a_left_out_moby_drawn_in_the_older_field_stays_drawn() {
		let cut = Some(crate::moby_cull::Cut { why: crate::moby_cull::Why::View, side_only: true, far_ok: false, sphere: [0.0; 4] });
		let a = Scene { field: 1, mobies: vec![moby_at(0x100, 0.0), Moby { cut, ..moby_at(0x200, 0.0) }], ..Scene::default() };
		let b = Scene { field: 2, mobies: vec![Moby { cut, ..moby_at(0x100, 1.0) }, Moby { cut, ..moby_at(0x200, 1.0) }], ..Scene::default() };
		let s = blend(&a, &b, 0.5, (&mut Cache::default(), &mut Cache::default()));
		let mut xs: Vec<(u32, f32, bool)> = s.mobies.iter().map(|m| (m.addr, m.pos[0], m.cut.is_some())).collect();
		xs.sort_by_key(|x| x.0);
		assert_eq!(xs, vec![(0x100, 0.0, false), (0x200, 0.5, true)]);
	}

	#[test]
	fn a_camera_cut_drops_the_older_fields_mobies() {
		let v = |x: f32| Some(View { eye: [x, 0.0, 0.0], basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], fx: 1.0, fy: 1.0, half: [256.0, 224.0] });
		let a = Scene { field: 1, view: v(0.0), mobies: vec![moby_at(0x200, 3.0)], ..Scene::default() };
		let b = Scene { field: 2, view: v(1.0e6), mobies: vec![moby_at(0x300, 5.0)], ..Scene::default() };
		let s = blend(&a, &b, 0.5, (&mut Cache::default(), &mut Cache::default()));
		assert_eq!(s.mobies.iter().map(|m| m.addr).collect::<Vec<_>>(), vec![0x300]);
	}

	/// Rotation about z by `deg`, uniformly scaled by `k`, moved by `t`, as a joint's 3x4 skinning matrix.
	fn joint(deg: f32, k: f32, t: [f32; 3]) -> [f32; 12] {
		let (s, c) = deg.to_radians().sin_cos();
		[c * k, -s * k, 0.0, t[0], s * k, c * k, 0.0, t[1], 0.0, 0.0, k, t[2]]
	}

	fn col_len(m: &[f32; 12], c: usize) -> f32 {
		(0..3).map(|r| m[r * 4 + c] * m[r * 4 + c]).sum::<f32>().sqrt()
	}

	#[test]
	fn a_fast_turning_joint_keeps_its_size() {
		let (a, b) = (joint(0.0, 1.0, [0.0; 3]), joint(120.0, 1.0, [1.0, 0.0, 0.0]));
		let m = blend_affine(&a, &b, 0.5);
		for c in 0..3 {
			assert!((col_len(&m, c) - 1.0).abs() < 1e-4, "column {c} is {}", col_len(&m, c));
		}
		// The 60 degree middle, the translation halfway.
		let mid = joint(60.0, 1.0, [0.5, 0.0, 0.0]);
		assert!(m.iter().zip(&mid).all(|(x, y)| (x - y).abs() < 1e-4), "{m:?} vs {mid:?}");
		// An element-wise lerp would have halved it.
		let lerped: Vec<f32> = a.iter().zip(&b).map(|(x, y)| lerp(*x, *y, 0.5)).collect();
		assert!((lerped[0] * lerped[0] + lerped[4] * lerped[4]).sqrt() < 0.55);
		// The ends are the two fields, the short way round even past 180 degrees.
		assert!(blend_affine(&a, &b, 1.0).iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-4));
		assert!(blend_affine(&a, &b, 0.0).iter().zip(&a).all(|(x, y)| (x - y).abs() < 1e-4));
		let back = blend_affine(&joint(10.0, 1.0, [0.0; 3]), &joint(350.0, 1.0, [0.0; 3]), 0.5);
		assert!((back[0] - 1.0).abs() < 1e-4 && back[1].abs() < 1e-4, "{back:?}");
	}

	#[test]
	fn a_scale_in_the_matrix_stays_and_blends_linearly() {
		let m = blend_affine(&joint(0.0, 2.0, [0.0; 3]), &joint(120.0, 2.0, [0.0; 3]), 0.5);
		for c in 0..3 {
			assert!((col_len(&m, c) - 2.0).abs() < 1e-4, "column {c} is {}", col_len(&m, c));
		}
		let m = blend_affine(&joint(0.0, 1.0, [0.0; 3]), &joint(90.0, 3.0, [0.0; 3]), 0.5);
		assert!((col_len(&m, 0) - 2.0).abs() < 1e-3, "{m:?}");
		// A hidden (zero) joint has no rotation, and a pair where only one is mirrored has no common handedness: they lerp as before.
		let zero = [0.0; 12];
		let h = blend_affine(&zero, &joint(0.0, 1.0, [2.0, 0.0, 0.0]), 0.5);
		assert!((h[0] - 0.5).abs() < 1e-6 && (h[3] - 1.0).abs() < 1e-6);
		let mirror = mirrored(0.0, [0.0; 3]);
		let h = blend_affine(&mirror, &joint(0.0, 1.0, [0.0; 3]), 0.5);
		assert!(h[0].abs() < 1e-6);
	}

	/// A mirrored joint: `joint(deg)` with its first column negated (det -1; the negative scale of a left / right symmetric rig).
	fn mirrored(deg: f32, t: [f32; 3]) -> [f32; 12] {
		let mut m = joint(deg, 1.0, t);
		for r in 0..3 {
			m[r * 4] = -m[r * 4];
		}
		m
	}

	/// Review B3/4 R6: two mirrored joints turn like rigid ones (R and the stretch blend, the sign stays) instead of the element lerp
	/// that shrank them to cos(turn / 2) at the middle.
	#[test]
	fn a_mirrored_joint_keeps_its_size_and_turns() {
		let (a, b) = (mirrored(0.0, [0.0; 3]), mirrored(120.0, [1.0, 0.0, 0.0]));
		let m = blend_affine(&a, &b, 0.5);
		for c in 0..3 {
			assert!((col_len(&m, c) - 1.0).abs() < 1e-4, "column {c} is {}", col_len(&m, c));
		}
		let mid = mirrored(60.0, [0.5, 0.0, 0.0]);
		assert!(m.iter().zip(&mid).all(|(x, y)| (x - y).abs() < 1e-4), "{m:?} vs {mid:?}");
		assert!(det3(&m3(&m)) < 0.0, "still mirrored");
		// The ends are the two fields; the old element lerp would have halved the middle.
		assert!(blend_affine(&a, &b, 1.0).iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-4));
		let (x, y) = (lerp(a[1], b[1], 0.5), lerp(a[5], b[5], 0.5));
		assert!((x * x + y * y).sqrt() < 0.55, "the element lerp halves column 1");
	}

	/// Review B3/4 R6: the split of a moby's joints is made once per field, not at every redraw: the same field pair redrawn at other
	/// places reuses the entries, the field that arrives next reuses the newest one's as the field before's, and a changed scene is noticed.
	#[test]
	fn the_split_is_made_once_per_field() {
		let with = |field: u64, deg: f32| {
			let mut m = moby_at(0x100, 0.0);
			m.pose = Some(Arc::new(vec![joint(deg, 1.0, [0.0; 3]), mirrored(deg, [0.0; 3])]));
			Scene { field, mobies: vec![m], ..Scene::default() }
		};
		let ptr = |c: &Cache| c.0[&0x100].joints.as_ref().unwrap().as_ptr();
		let (a, b, c) = (with(1, 0.0), with(2, 40.0), with(3, 80.0));
		let mut sm = Smooth::default();
		sm.scene(&a, true, Some(0.0));
		let first = sm.scene(&b, true, Some(0.25));
		let (pa, pb) = (ptr(&sm.before_dec), ptr(&sm.last_dec));
		let second = sm.scene(&b, true, Some(0.75));
		assert_eq!((ptr(&sm.before_dec), ptr(&sm.last_dec)), (pa, pb), "a redraw of the same pair splits nothing again");
		// Same split, same blend as the uncached one.
		for (s, t) in [(&first, 0.25), (&second, 0.75)] {
			let want = blend_affine(&a.mobies[0].pose.as_ref().unwrap()[1], &b.mobies[0].pose.as_ref().unwrap()[1], t);
			assert!(s.mobies[0].pose.as_ref().unwrap()[1].iter().zip(&want).all(|(x, y)| (x - y).abs() < 1e-6));
		}
		sm.scene(&c, true, Some(0.5));
		assert_eq!(ptr(&sm.before_dec), pb, "the newest field's split is the next pair's field before");
		// A scene whose matrices changed under the same address and field is split again.
		let mut d = with(3, 70.0);
		d.mobies[0].pose = Some(Arc::new(vec![joint(70.0, 1.0, [0.0; 3]), mirrored(70.0, [0.0; 3])]));
		let _ = sm.scene(&d, true, Some(0.5));
		assert!(sm.last_dec.0[&0x100].src_pose.as_ref().is_some_and(|p| Arc::ptr_eq(p, d.mobies[0].pose.as_ref().unwrap())));
	}

	#[test]
	fn moby_basis_and_pose_turn_without_shrinking() {
		let rot = |deg: f32| {
			let j = joint(deg, 1.0, [0.0; 3]);
			[[j[0], j[1], j[2]], [j[4], j[5], j[6]], [j[8], j[9], j[10]]]
		};
		let mut a = moby_at(0x100, 0.0);
		let mut b = moby_at(0x100, 0.1);
		// 40 degrees in one field is about the most the snap rule lets through (cos 0.7 = 45.6 degrees).
		(a.rows, b.rows) = (rot(0.0), rot(40.0));
		a.pose = Some(Arc::new(vec![joint(0.0, 1.0, [0.0; 3])]));
		b.pose = Some(Arc::new(vec![joint(150.0, 1.0, [0.0; 3])]));
		let m = moby(&a, &b, 0.5, (&mut Cache::default(), &mut Cache::default()));
		for r in m.rows {
			assert!((len(r) - 1.0).abs() < 1e-4);
		}
		let p = m.pose.unwrap();
		assert!((col_len(&p[0], 0) - 1.0).abs() < 1e-4);
	}
}
