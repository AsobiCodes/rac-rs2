//! R&C2 effect program (VU1 program 2: sprites, particles, ground rings, trails, rain / spark lines) drawn natively from its INPUT
//! (round 3 batch 7; issues 4 / 5 / 6 / 7 / 9). Microcode: `rac2/decomp/vu1/prog2.s`, facts in `rac2/notes_fx_prog2.md`.
//!
//! The program is still run (its output keeps the VU1 state right and is the fallback), but before it runs `capture` reads the
//! particle records it is about to consume from VU1 data memory and rebuilds each record's corners in WORLD space: the 3D point, and
//! for a camera-facing sprite the corner's pixel offset from it. The software GS then meets the program's triangles / lines, matches
//! each to its record by the corners' screen position (`find`) and hands the engine world triangles (`gs_layers.rs fx_capture`);
//! the pack places them with the camera of the field (a sprite: centre + pixel offset at the centre's depth) and the engine draws
//! them at the window's resolution, at the smoothed camera, depth-tested against the world, with the game's texture / blend / fog.
//! A prim that matches no record (bogus data, behind the camera) stays in the overlay, so nothing is removed without being drawn.
//! Game-neutral machinery; only the R&C2 pack switches it on (`layer_tris::enable_fx`).
use super::vu1::Vu1;

/// A corner: a world point plus a screen-space offset in GS pixels (right, down) from it, applied at the point's view depth.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WVert {
	pub p: [f32; 3],
	pub off: [f32; 2],
}

/// One particle record: `n` corners (4: a triangle strip, 2: a line) in the program's output order, and the GS x / y (12.4 fixed
/// point with the program's 2048 offset) the microcode computes for each.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FxRec {
	pub n: u8,
	pub v: [WVert; 4],
	pub xy: [[i32; 2]; 4],
}

/// Records one `capture` may hand out (a runaway packet is not captured).
pub const MAX_RECS: usize = 3000;

struct Regs {
	m: [[f32; 4]; 4],
	k: f32,
	off: [f32; 2],
	s: [f32; 2],
}

fn f(w: u32) -> f32 {
	f32::from_bits(w)
}

impl Regs {
	/// The constants the program's first run left in the vector registers (vf28..31 view-projection, vf25.x, vf26.xy, vf27.xy).
	fn of(vu: &Vu1) -> Regs {
		let r = |i: usize| vu.vf[i].map(f);
		Regs { m: [r(28), r(29), r(30), r(31)], k: r(25)[0], off: [r(26)[0], r(26)[1]], s: [r(27)[0], r(27)[1]] }
	}

	/// vf31 + vf28 x + vf29 y + vf30 z.
	fn clip(&self, p: [f32; 3]) -> [f32; 4] {
		std::array::from_fn(|i| self.m[3][i] + self.m[0][i] * p[0] + self.m[1][i] * p[1] + self.m[2][i] * p[2])
	}

	/// The GS x / y (12.4) of a world point plus a pixel offset; None behind the camera.
	fn xy(&self, p: [f32; 3], off: [f32; 2]) -> Option<[i32; 2]> {
		let c = self.clip(p);
		// (R&C2 builds its projection with a negative k and a negative clip.w in front of the camera: the sign of k / w is what counts.)
		let q = self.k / c[3];
		if c.iter().any(|x| !x.is_finite()) || !(q > 1e-9) {
			return None;
		}
		let fx = |o: f32, base: f32, c: f32| (((base + o) + c * q) * 16.0) as i32;
		Some([fx(off[0], self.off[0], c[0]), fx(off[1], self.off[1], c[1])])
	}
}

/// Reads the records of the packet at VU1 data qword `base` (the program's input buffer) and appends their corners to `out`.
/// Returns the number appended; 0 and `out` untouched when the packet does not parse to its own length (the record sizes must sum
/// to the length the first record's third qword names, as the program's loop does).
pub fn capture(vu: &Vu1, base: usize, out: &mut Vec<FxRec>) -> usize {
	let t0 = timing().then(std::time::Instant::now);
	let n = capture_recs(vu, base, out);
	if let Some(t0) = t0 {
		CAP_NS.fetch_add(t0.elapsed().as_nanos() as u64, std::sync::atomic::Ordering::Relaxed);
	}
	n
}

/// RAC2_LAYERTIME=1: nanoseconds `capture` took since the last `take_cap_ns` (the VIF thread adds, the GS thread's field log reads).
static CAP_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn timing() -> bool {
	static C: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
	*C.get_or_init(|| std::env::var_os("RAC2_LAYERTIME").is_some_and(|v| v != "0"))
}

/// The time `capture` has taken since the last call (ns; 0 unless RAC2_LAYERTIME is on).
pub fn take_cap_ns() -> u64 {
	CAP_NS.swap(0, std::sync::atomic::Ordering::Relaxed)
}

fn capture_recs(vu: &Vu1, base: usize, out: &mut Vec<FxRec>) -> usize {
	let rg = Regs::of(vu);
	let rd = |q: usize| vu.data[(base + q) & 0x3ff];
	let len = (rd(2)[3] & 0xffff) as usize;
	let (start, mut i, mut guard) = (out.len(), 0usize, 0);
	loop {
		let q0 = rd(i);
		let mode = (q0[3] & 0xffff) as u16 as i16;
		let p = [f(q0[0]), f(q0[1]), f(q0[2])];
		let v3 = |q: [u32; 4]| [f(q[0]), f(q[1]), f(q[2])];
		let (size, rec) = match mode {
			0 => (4, sprite(&rg, p, rd(i + 2))),
			-2 => (6, ribbon(&rg, p, rd(i + 1), v3(rd(i + 2)))),
			m if m < 0 => (4, line(&rg, p, v3(rd(i + 2)))),
			_ => (5, ground(&rg, p, rd(i + 2))),
		};
		out.extend(rec);
		i += size;
		guard += 1;
		if i == len {
			break;
		}
		if i > len || guard > MAX_RECS || out.len() - start > MAX_RECS {
			out.truncate(start);
			return 0;
		}
	}
	out.len() - start
}

fn rec(rg: &Regs, v: [WVert; 4], n: u8) -> Option<FxRec> {
	let mut xy = [[0; 2]; 4];
	for (j, w) in v.iter().take(n as usize).enumerate() {
		xy[j] = rg.xy(w.p, w.off)?;
	}
	Some(FxRec { n, v, xy })
}

/// Mode 0: a camera-facing sprite; record qw2 = (a, b, ..): corners at (a s.x, b s.y), (-b, s.y a), (b, -s.y a), (-a s.x, -b s.y) pixels.
fn sprite(rg: &Regs, p: [f32; 3], q2: [u32; 4]) -> Option<FxRec> {
	let (a, b) = (f(q2[0]), f(q2[1]));
	let offs = [[a * rg.s[0], b * rg.s[1]], [-b, rg.s[1] * a], [b, -(rg.s[1] * a)], [-(a * rg.s[0]), -(b * rg.s[1])]];
	rec(rg, offs.map(|off| WVert { p, off }), 4)
}

/// Mode > 0: a ring / splat lying in the world XY plane at p.z, turned by the vector (d.x, d.y) = record qw2.xy.
fn ground(rg: &Regs, p: [f32; 3], q2: [u32; 4]) -> Option<FxRec> {
	let (dx, dy) = (f(q2[0]), f(q2[1]));
	let c = |x: f32, y: f32| WVert { p: [x, y, p[2]], off: [0.0; 2] };
	rec(rg, [c(p[0] + dx, p[1] + dy), c(p[0] - dy, p[1] + dx), c(p[0] + dy, p[1] - dx), c(p[0] - dx, p[1] - dy)], 4)
}

/// Mode -2: a ribbon segment: P +- d, then Q +- d * s with Q = qw1.xyz, s = qw1.w, d = qw2.xyz.
fn ribbon(rg: &Regs, p: [f32; 3], q1: [u32; 4], d: [f32; 3]) -> Option<FxRec> {
	let (q, s) = ([f(q1[0]), f(q1[1]), f(q1[2])], f(q1[3]));
	let at = |c: [f32; 3], k: f32, sign: f32| WVert { p: std::array::from_fn(|i| c[i] + sign * d[i] * k), off: [0.0; 2] };
	rec(rg, [at(p, 1.0, 1.0), at(p, 1.0, -1.0), at(q, s, 1.0), at(q, s, -1.0)], 4)
}

/// Other negative modes: a line from p to qw2.xyz (one GS pixel wide, its colours per end).
fn line(rg: &Regs, p: [f32; 3], b: [f32; 3]) -> Option<FxRec> {
	let w = |p| WVert { p, off: [0.0; 2] };
	rec(rg, [w(p), w(b), WVert::default(), WVert::default()], 2)
}

/// Prims one record yields: a 4-corner record is a triangle strip of 2, a line is 1.
pub fn prims_of(r: &FxRec) -> u8 {
	if r.n == 2 { 1 } else { 2 }
}

/// Which corner of `r` a GS vertex (x, y in 12.4) is: within a quarter pixel of the program's own number.
fn corner(r: &FxRec, x: i32, y: i32, skip: &[usize]) -> Option<usize> {
	(0..r.n as usize).find(|&j| !skip.contains(&j) && (r.xy[j][0] - x).abs() <= 4 && (r.xy[j][1] - y).abs() <= 4)
}

/// The record a primitive of `pts` (GS x / y of its 3, or 2 for a line, vertices) belongs to, searched forward from `*cur` (the
/// records are in the order the program emits them). `used[i]` counts the prims record `i` has yielded so far (`prims_of` is all it
/// has): a record that is used up is passed over, so a burst of identical records (a spark / debris burst spawned at one point with
/// one size) maps 1:1 in order and never stalls the rest of the field. Records before a match are done with (the program emits a
/// record's prims one after the other), so `*cur` follows the match. Returns the record and the corner of each vertex.
pub fn find(recs: &[FxRec], used: &mut [u8], cur: &mut usize, pts: &[[i32; 2]]) -> Option<(usize, [usize; 3])> {
	const AHEAD: usize = 24;
	let want_line = pts.len() == 2;
	for idx in *cur..recs.len().min(*cur + AHEAD) {
		let r = &recs[idx];
		if (r.n == 2) != want_line || used.get(idx).is_none_or(|&u| u >= prims_of(r)) {
			continue;
		}
		let (mut got, mut taken, mut n) = ([0usize; 3], [usize::MAX; 3], 0);
		let ok = pts.iter().enumerate().all(|(k, p)| match corner(r, p[0], p[1], &taken[..n]) {
			Some(c) => {
				got[k] = c;
				taken[n] = c;
				n += 1;
				true
			}
			None => false,
		});
		if ok {
			used[idx] += 1;
			*cur = if used[idx] >= prims_of(r) { idx + 1 } else { idx };
			return Some((idx, got));
		}
	}
	None
}

#[cfg(test)]
mod tests {
	use super::*;

	fn regs() -> Regs {
		// Camera at the origin looking along +y; screen x = 2048 + 400 x / y, screen y = 2048 - 400 z / y. clip.w = y.
		Regs { m: [[400.0, 0.0, 0.0, 0.0], [0.0, 0.0, 1.0, 1.0], [0.0, -400.0, 0.0, 0.0], [0.0; 4]], k: 1.0, off: [2048.0, 2048.0], s: [1.0, 1.0625] }
	}

	#[test]
	fn a_point_projects_like_the_microcode() {
		let r = regs();
		// 2 m right, 10 m ahead, 1 m up: x = 2048 + 80 px, y = 2048 - 40 px (12.4: x16).
		assert_eq!(r.xy([2.0, 10.0, 1.0], [0.0; 2]), Some([(2048 + 80) * 16, (2048 - 40) * 16]));
		assert_eq!(r.xy([0.0, -3.0, 0.0], [0.0; 2]), None, "behind the camera");
	}

	#[test]
	fn a_sprite_has_pixel_offset_corners_and_a_ground_ring_world_corners() {
		let r = regs();
		let s = sprite(&r, [0.0, 10.0, 0.0], [4.0f32.to_bits(), 2.0f32.to_bits(), 0, 0]).unwrap();
		assert_eq!(s.n, 4);
		assert_eq!(s.v[0].off, [4.0, 2.0 * 1.0625]);
		assert_eq!(s.v[1].off, [-2.0, 1.0625 * 4.0]);
		assert_eq!(s.xy[0], [(2048 + 4) * 16, ((2048.0 + 2.125) * 16.0) as i32]);
		let g = ground(&r, [0.0, 10.0, 0.0], [1.0f32.to_bits(), 0, 0, 0]).unwrap();
		assert_eq!(g.v[0].p, [1.0, 10.0, 0.0]);
		assert_eq!(g.v[1].p, [0.0, 11.0, 0.0], "(x - d.y, y + d.x)");
		assert_eq!(g.v[3].p, [-1.0, 10.0, 0.0]);
	}

	#[test]
	fn a_packet_must_sum_to_its_length_and_a_vertex_finds_its_record() {
		let mut vu = Vu1::new(&[], &[]);
		vu.vf[25] = [1.0f32.to_bits(), 0, 0, 0];
		vu.vf[26] = [2048.0f32.to_bits(), 2048.0f32.to_bits(), 0, 0];
		vu.vf[27] = [1.0f32.to_bits(), 1.0f32.to_bits(), 0, 0];
		vu.vf[28] = [400.0f32.to_bits(), 0, 0, 0];
		vu.vf[29] = [0, 0, 1.0f32.to_bits(), 1.0f32.to_bits()];
		vu.vf[30] = [0, (-400.0f32).to_bits(), 0, 0];
		let put = |vu: &mut Vu1, q: usize, w: [f32; 4], mode: Option<u32>| vu.data[q] = [w[0].to_bits(), w[1].to_bits(), w[2].to_bits(), mode.unwrap_or(w[3].to_bits())];
		// Two sprites (4 qw each): the first record's qw2.w = 8 = the packet length.
		put(&mut vu, 0, [0.0, 10.0, 0.0, 0.0], Some(0));
		put(&mut vu, 2, [3.0, 3.0, 0.0, 0.0], Some(8));
		put(&mut vu, 4, [1.0, 20.0, 0.0, 0.0], Some(0));
		put(&mut vu, 6, [3.0, 3.0, 0.0, 0.0], Some(0));
		let mut out = Vec::new();
		assert_eq!(capture(&vu, 0, &mut out), 2);
		let (mut cur, p, mut used) = (0, out[1].xy[2], vec![0u8; out.len()]);
		let (idx, c) = find(&out, &mut used, &mut cur, &[out[1].xy[0], out[1].xy[1], [p[0] + 3, p[1] - 2]]).unwrap();
		assert_eq!((idx, c), (1, [0, 1, 2]));
		assert_eq!((cur, used[1]), (1, 1), "the strip's second triangle may still use this record");
		// A length that the records do not add up to is refused.
		put(&mut vu, 2, [3.0, 3.0, 0.0, 0.0], Some(7));
		let mut out = Vec::new();
		assert_eq!(capture(&vu, 0, &mut out), 0);
		assert!(out.is_empty());
	}

	#[test]
	fn a_burst_of_identical_records_maps_one_to_one_and_never_stalls() {
		let rec = |x: i32| FxRec { n: 4, v: [WVert::default(); 4], xy: [[x, 0], [x + 160, 0], [x, 160], [x + 160, 160]] };
		// 40 identical records, then a distinct one far past the 24-record window of a cursor stuck on the first.
		let mut recs: Vec<FxRec> = (0..40).map(|_| rec(1000)).collect();
		recs.push(rec(5000));
		let (mut used, mut cur) = (vec![0u8; recs.len()], 0);
		let tris = [[[1000, 0], [1160, 0], [1000, 160]], [[1160, 0], [1000, 160], [1160, 160]]];
		for want in 0..40 {
			for t in &tris {
				assert_eq!(find(&recs, &mut used, &mut cur, t).map(|r| r.0), Some(want), "the {want}th identical record");
			}
		}
		let d = [[5000, 0], [5160, 0], [5000, 160]];
		assert_eq!(find(&recs, &mut used, &mut cur, &d).map(|r| r.0), Some(40), "the record behind the burst is still found");
		// A line record yields one prim.
		let line = FxRec { n: 2, v: [WVert::default(); 4], xy: [[16, 16], [320, 320], [0; 2], [0; 2]] };
		let (mut used, mut cur) = (vec![0u8; 1], 0);
		assert!(find(&[line.clone()], &mut used, &mut cur, &[[16, 16], [320, 320]]).is_some());
		assert!(find(&[line], &mut used, &mut cur, &[[16, 16], [320, 320]]).is_none(), "used up");
	}
}
