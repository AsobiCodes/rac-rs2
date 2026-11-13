//! [PC] Frame smoothing of the native effect layers (rain streaks, splashes, sparks, glows: `ov_layers.rs`; round 3 batch 25, ISSUES
//! B23-A5). Each overlay field's triangles are placed in the world with that field's own camera and were drawn as they are under the
//! smoothed camera (smooth.rs), so a falling rain streak stood still for a field and then jumped, while the world around it moved on
//! every redraw [C: runs/b25/strip_streak.png top row]. Here the two newest consecutive fields' triangles are blended per corner to
//! the smoothed camera's time, as the program-10 water is (`crate::water::blend_at`, the same time rule): a triangle blends with its
//! partner in the older field (`pairs`); any other triangle (a new particle, one that jumped) is the newest field's as it is.
//! Smoothing off, a camera cut or no older field: the newest field's triangles as they are (pictures without smoothing unchanged).

use rac_engine::viewer::gpu::layer_fx::WorldTri;
use std::collections::HashMap;

/// The farthest (m) a corner may move in one field and still blend (a streak re-used for a new drop elsewhere snaps).
const SNAP: f32 = 2.0;
/// A partner by rank must be about the nearest triangle of its group in the older field (centre distance at most this factor of the
/// nearest one's plus `SLACK` m): two splashes standing a metre apart are not blended into each other [C: runs/b25 n28, a static
/// splash slid ~65 px between fields with rank pairing alone].
const NEAREST: f32 = 1.25;
const SLACK: f32 = 0.05;

/// RAC2_LAYERBLEND is set (read once): one line per redraw (camera time, fields, triangles blended).
fn log_on() -> bool {
	static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
	*ON.get_or_init(|| std::env::var_os("RAC2_LAYERBLEND").is_some_and(|v| v != "0"))
}

type Field = (u64, Vec<WorldTri>);

/// The layers of the two newest consecutive fields, by the field of the camera they were placed with.
#[derive(Default)]
pub(super) struct History {
	older: Option<Field>,
	newest: Option<Field>,
	/// For each of the newest field's triangles, its partner among the older field's (made once per field).
	pairs: Vec<Option<usize>>,
	/// The engine holds blended triangles (the newest must be handed back when the blend stops).
	blended: bool,
}

impl History {
	/// A new field's triangles (`key`: the field of the camera that placed them); the older one is kept only when it is the field before.
	pub(super) fn push(&mut self, key: u64, tris: Vec<WorldTri>) {
		self.older = self.newest.take().filter(|f| f.0 + 1 == key);
		self.pairs = self.older.as_ref().map(|a| pairs(&a.1, &tris)).unwrap_or_default();
		self.newest = Some((key, tris));
		self.blended = false;
	}

	/// No layers (a field without a camera, a whole-picture field).
	pub(super) fn clear(&mut self) {
		*self = Self::default();
	}

	/// The triangles to hand the engine for a redraw at the smoothed camera `blended` (smooth.rs `Smooth::blended`): Some = new ones,
	/// None = the engine's are right as they are.
	pub(super) fn at(&mut self, blended: Option<(u64, f32)>) -> Option<Vec<WorldTri>> {
		let mixed = blended.and_then(|(f, t)| self.blend(f, t));
		if log_on() {
			let n = |f: &Option<Field>| f.as_ref().map(|f| (f.0, f.1.len()));
			eprintln!("layer blend: smoothed {blended:?} fields {:?} {:?} paired {} blended {}", n(&self.older), n(&self.newest), self.pairs.iter().flatten().count(), mixed.is_some());
		}
		match mixed {
			Some(v) => {
				self.blended = true;
				Some(v)
			}
			None if self.blended => {
				self.blended = false;
				self.newest.as_ref().map(|l| l.1.iter().map(|t| copy(t, t.pos)).collect())
			}
			None => None,
		}
	}

	/// [PC] A copy of the newest field's triangles as they are (host/ov_fire.rs re-hands them at every redraw while it runs).
	pub(super) fn newest(&self) -> Option<Vec<WorldTri>> {
		self.newest.as_ref().map(|l| l.1.iter().map(|t| copy(t, t.pos)).collect())
	}

	/// The newest field's triangles at camera time `field + t`, `u` fields after the older field (0..2: up to a field past the newest,
	/// whose layers are a field behind the newest camera as the water is). None: no older field, or the time is before it.
	fn blend(&self, field: u64, t: f32) -> Option<Vec<WorldTri>> {
		let (Some((ka, a)), Some((_, b))) = (&self.older, &self.newest) else { return None };
		if *ka > field {
			return None;
		}
		let u = ((field - ka) as f32 + t).clamp(0.0, 2.0);
		let mix = |p: &WorldTri, q: &WorldTri| std::array::from_fn(|c| std::array::from_fn(|x| p.pos[c][x] + (q.pos[c][x] - p.pos[c][x]) * u));
		Some(b.iter().zip(&self.pairs).map(|(q, p)| copy(q, p.map_or(q.pos, |i| mix(&a[i], q)))).collect())
	}
}

/// Each of `b`'s triangles' partner in `a`: in its group (texture, blend, depth test) the triangle of rank r in `b` is paired with
/// rank r - k in `a`, k = 0 (particles added at the end) or the count difference (added at the front), whichever pairs more; a pair
/// holds when no corner moved past `SNAP` and the partner is about the nearest of the group (`NEAREST`) [C: Oozla rain, runs/b25
/// n28.err "layer group": the front ranks of a group stay 0.01..0.46 m apart field to field while its count changes by 2..10].
fn pairs(a: &[WorldTri], b: &[WorldTri]) -> Vec<Option<usize>> {
	type Group = (u64, bool, bool, bool);
	let group = |t: &WorldTri| (t.id, t.additive, t.depth, t.replace);
	let mut older: HashMap<Group, Vec<usize>> = HashMap::new();
	a.iter().enumerate().for_each(|(i, t)| older.entry(group(t)).or_default().push(i));
	let mut newer: HashMap<Group, Vec<usize>> = HashMap::new();
	b.iter().enumerate().for_each(|(i, t)| newer.entry(group(t)).or_default().push(i));
	let centre = |t: &WorldTri| -> [f32; 3] { std::array::from_fn(|k| (t.pos[0][k] + t.pos[1][k] + t.pos[2][k]) / 3.0) };
	let dist = |p: [f32; 3], q: [f32; 3]| (0..3).map(|k| (p[k] - q[k]).powi(2)).sum::<f32>().sqrt();
	let mut out = vec![None; b.len()];
	for (g, idx) in &newer {
		let Some(o) = older.get(g) else { continue };
		let olds: Vec<[f32; 3]> = o.iter().map(|&i| centre(&a[i])).collect();
		let by_x = ByX::new(&olds); // [b49 U3] the nearest-partner test was O(n) per triangle: an explosion's 2000+ sparks froze the frame
		// The partner must be about the nearest older triangle, its own quad's other half left out (a streak quad's other triangle
		// lies nearer than the partner once the streak falls more than half its length).
		let ok = |r: usize, j: usize| {
			let (t, p) = (&b[idx[r]], &a[o[j]]);
			if !(0..3).all(|c| dist(t.pos[c], p.pos[c]) <= SNAP) {
				return false;
			}
			let ct = centre(t);
			let dj = dist(ct, olds[j]);
			// [b50 V1] Only whether `dj` passes is asked, so the search stops at the first nearest that already passes.
			let elig = |s: usize| o[s] == o[j] || !shares(&a[o[s]], p); // [b49 U3]
			if !dj.is_finite() {
				return dj <= by_x.nearest(ct, f32::MAX, elig) * NEAREST + SLACK;
			}
			// [b51 G4] It passes unless an eligible point is nearer than the floor (`any_closer`: stops at the first one).
			!by_x.any_closer(ct, pass_floor(dj), elig)
		};
		// [b50 V1] The partners of each rank are made once per shift (they were made 2 - 3 times: count(front), count(0), the pairing).
		let partner = |k: isize, r: usize| usize::try_from(r as isize - k).ok().filter(|&j| j < o.len() && ok(r, j));
		let part = |k: isize| -> Vec<Option<usize>> { (0..idx.len()).map(|r| partner(k, r)).collect() };
		let count = |v: &[Option<usize>]| v.iter().flatten().count();
		let front = idx.len() as isize - o.len() as isize;
		let mut chosen = part(0);
		// [b51 G4] A shift cannot pair more than every rank: when the plain one did, the other is not made.
		if front != 0 && count(&chosen) < idx.len() {
			let f = part(front);
			if count(&f) > count(&chosen) {
				chosen = f;
			}
		}
		for (r, &i) in idx.iter().enumerate() {
			out[i] = chosen[r].map(|j| o[j]);
		}
		// The two halves of a quad move together or not at all.
		for w in idx.windows(2) {
			if shares(&b[w[0]], &b[w[1]]) && out[w[0]].is_some() != out[w[1]].is_some() {
				(out[w[0]], out[w[1]]) = (None, None);
			}
		}
	}
	out
}

/// [b49 U3] Points sorted by x: the exact nearest eligible point of a query without scanning them all. The search walks outwards from
/// the query's x on both sides and stops where the x gap alone is past the best distance found, so a dense burst (best distance tiny)
/// looks at a thin slab. The result is the same minimum the plain scan gave (`nearest_brute` in the tests).
struct ByX<'a> {
	pts: &'a [[f32; 3]],
	ord: Vec<usize>,
	xs: Vec<f32>,
}

impl<'a> ByX<'a> {
	fn new(pts: &'a [[f32; 3]]) -> Self {
		// [b50 V1] A centre with a non-finite x never won the plain scan (its distance is NaN / infinite); a NaN sorted to the front
		// broke the partition of `nearest` (review B49 finding 7).
		let mut ord: Vec<usize> = (0..pts.len()).filter(|&i| pts[i][0].is_finite()).collect();
		ord.sort_by(|&p, &q| pts[p][0].total_cmp(&pts[q][0]));
		let xs = ord.iter().map(|&i| pts[i][0]).collect();
		ByX { pts, ord, xs }
	}

	/// [b51 G4] Is there a point `s` with `ok(s)` strictly nearer to `q` than `cap`? The same answer as `self.nearest(q, cap, ok) < cap`,
	/// but it stops at the first such point (the pass test only needs that bool; `nearest` kept walking to narrow the true minimum, and
	/// the profile of the Zodiac burst put 18.5 % of the render thread's samples into that walk), looks at the distance before the
	/// eligibility test (`ok` compares triangles), and walks both sides outwards together so a near point is met early.
	fn any_closer(&self, q: [f32; 3], cap: f32, ok: impl Fn(usize) -> bool) -> bool {
		let d2 = |s: usize| (0..3).map(|k| (self.pts[s][k] - q[k]).powi(2)).sum::<f32>();
		let start = self.xs.partition_point(|&x| x < q[0]);
		let within = |gap: f32| gap <= cap * 1.0001 + 1e-6;
		let (mut hi, mut lo) = (start, start);
		loop {
			let up = hi < self.ord.len() && within(self.xs[hi] - q[0]);
			let down = lo > 0 && within(q[0] - self.xs[lo - 1]);
			if !up && !down {
				return false;
			}
			if up {
				let s = self.ord[hi];
				hi += 1;
				// The same value `nearest` compares (sqrt of the same sum), so a tie at `cap` gives the same bool.
				if d2(s).sqrt() < cap && ok(s) {
					return true;
				}
			}
			if down {
				lo -= 1;
				let s = self.ord[lo];
				if d2(s).sqrt() < cap && ok(s) {
					return true;
				}
			}
		}
	}

	/// The least distance from `q` to a point `s` with `ok(s)`, or `cap` when none is nearer (f32::MAX with `cap` f32::MAX: none).
	fn nearest(&self, q: [f32; 3], cap: f32, ok: impl Fn(usize) -> bool) -> f32 {
		let d = |s: usize| (0..3).map(|k| (self.pts[s][k] - q[k]).powi(2)).sum::<f32>().sqrt();
		let mut best = cap;
		let start = self.xs.partition_point(|&x| x < q[0]);
		// The slack keeps a last-bit rounding of `d` from ending the walk early.
		let within = |gap: f32, best: f32| gap <= best * 1.0001 + 1e-6;
		let mut i = start;
		while i < self.ord.len() && within(self.xs[i] - q[0], best) {
			if ok(self.ord[i]) {
				best = best.min(d(self.ord[i]));
			}
			i += 1;
		}
		let mut i = start;
		while i > 0 && within(q[0] - self.xs[i - 1], best) {
			i -= 1;
			if ok(self.ord[i]) {
				best = best.min(d(self.ord[i]));
			}
		}
		best
	}
}

/// [b50 V1] A `nearest` value from which `dj <= nearest * NEAREST + SLACK` holds (the formula is monotone in `nearest`, so any nearest
/// at or above this passes too). Not finite `dj`: no cap (the plain search).
fn pass_floor(dj: f32) -> f32 {
	if !dj.is_finite() {
		return f32::MAX;
	}
	let mut c = ((dj - SLACK) / NEAREST).max(0.0);
	while !(dj <= c * NEAREST + SLACK) {
		c = f32::from_bits(c.to_bits() + 1);
	}
	c
}

/// Do two triangles share a corner (two halves of one quad)?
fn shares(p: &WorldTri, q: &WorldTri) -> bool {
	p.pos.iter().any(|c| q.pos.contains(c))
}

/// `t` at the corners `pos`.
fn copy(t: &WorldTri, pos: [[f32; 3]; 3]) -> WorldTri {
	WorldTri { pos, uv: t.uv, col: t.col, id: t.id, tex: t.tex.clone(), additive: t.additive, depth: t.depth, effect: t.effect, replace: t.replace, zwrite: t.zwrite, alpha: t.alpha }
}

/// A copy of a triangle list (`WorldTri` is not `Clone`; ov_layers.rs keeps the list last handed while Fire is on).
pub(super) fn dup(v: &[WorldTri]) -> Vec<WorldTri> {
	v.iter().map(|t| copy(t, t.pos)).collect()
}

#[cfg(test)]
mod tests {
	use super::*;

	fn tri(id: u64, x: f32) -> WorldTri {
		let tex = std::sync::Arc::new(rac_formats::level::Texture { w: 1, h: 1, rgba: vec![[255; 4]] });
		WorldTri { pos: [[x, 0.0, 0.0], [x, 1.0, 0.0], [x, 0.0, 1.0]], uv: [[0.0; 2]; 3], col: [[1.0; 4]; 3], id, tex, additive: false, depth: true, effect: false, replace: false, zwrite: false, alpha: Default::default() }
	}

	/// [b49 U3] The plain scan the sorted search replaced.
	fn nearest_brute(pts: &[[f32; 3]], q: [f32; 3], ok: impl Fn(usize) -> bool) -> f32 {
		(0..pts.len()).filter(|&s| ok(s)).map(|s| (0..3).map(|k| (pts[s][k] - q[k]).powi(2)).sum::<f32>().sqrt()).fold(f32::MAX, f32::min)
	}

	#[test]
	fn the_sorted_nearest_search_equals_the_plain_scan() {
		let mut seed = 12345u32;
		let mut rnd = move || {
			seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
			(seed >> 8) as f32 / (1u32 << 24) as f32
		};
		// A dense burst plus a few far points, 2500 of them: every query agrees, with and without exclusions.
		let mut pts: Vec<[f32; 3]> = (0..2500).map(|_| [rnd() * 2.0, rnd() * 2.0, rnd() * 2.0]).collect();
		pts.extend((0..20).map(|_| [rnd() * 80.0, rnd() * 80.0, rnd() * 80.0]));
		let by = ByX::new(&pts);
		for n in 0..300 {
			let q = if n % 3 == 0 { [rnd() * 90.0, rnd() * 90.0, rnd() * 90.0] } else { pts[(n * 7) % pts.len()] };
			let skip = (n * 11) % pts.len();
			let ok = |s: usize| s != skip && s % 5 != n % 5;
			assert_eq!(by.nearest(q, f32::MAX, ok), nearest_brute(&pts, q, ok), "query {n}");
			// [b50 V1] The capped search decides the pass test as the plain minimum does, for near and far partners.
			for dj in [0.0f32, 0.03, 0.06, 0.2, 1.0, 5.0, 40.0] {
				let pass = |nearest: f32| dj <= nearest * NEAREST + SLACK;
				assert_eq!(pass(by.nearest(q, pass_floor(dj), ok)), pass(nearest_brute(&pts, q, ok)), "query {n} dj {dj}");
				// [b51 G4] The early-exit test gives the same bool, also at the exact floor and for tiny / huge caps.
				let cap = pass_floor(dj);
				assert_eq!(pass(f32::MAX) && !by.any_closer(q, cap, ok), pass(nearest_brute(&pts, q, ok)), "any_closer query {n} dj {dj}");
				for c in [0.0f32, 1e-4, cap, 0.7, 3.0, 200.0] {
					assert_eq!(by.any_closer(q, c, ok), nearest_brute(&pts, q, ok) < c, "any_closer query {n} cap {c}");
				}
			}
		}
		assert_eq!(by.nearest([0.0; 3], f32::MAX, |_| false), f32::MAX);
	}

	#[test]
	fn a_nan_centre_is_ignored_by_the_sorted_search() {
		// Review B49 finding 7: a NaN x (negative NaN sorts first) must not end the walk.
		let pts = [[f32::NAN, 0.0, 0.0], [5.0, 0.0, 0.0], [7.0, 0.0, 0.0], [f32::INFINITY, 0.0, 0.0]];
		let by = ByX::new(&pts);
		assert_eq!(by.nearest([0.0; 3], f32::MAX, |_| true), 5.0);
		assert_eq!(by.nearest([0.0; 3], f32::MAX, |_| true), nearest_brute(&pts, [0.0; 3], |_| true));
	}

	fn xs(v: Vec<WorldTri>) -> Vec<f32> {
		v.iter().map(|t| t.pos[0][0]).collect()
	}

	#[test]
	fn a_streak_moves_between_fields_and_a_new_one_snaps() {
		let mut h = History::default();
		h.push(10, vec![tri(1, 0.0), tri(2, 5.0)]);
		h.push(11, vec![tri(1, 1.0), tri(2, 9.0), tri(3, 7.0)]);
		// Camera 10 + 1/2: half way; the 4 m jump (past SNAP) and the new texture group are the newest field's.
		assert_eq!(h.at(Some((10, 0.5))).map(xs), Some(vec![0.5, 9.0, 7.0]));
		// A field past the newer one at most; smoothing off after a blend hands the newest back once.
		assert_eq!(h.at(Some((11, 0.5))).map(xs), Some(vec![1.5, 9.0, 7.0]));
		assert_eq!(h.at(None).map(xs), Some(vec![1.0, 9.0, 7.0]));
		assert!(h.at(None).is_none());
		// Before the older field, or the fields not consecutive: no blend.
		assert!(h.at(Some((9, 0.5))).is_none());
		h.push(13, vec![tri(1, 2.0)]);
		assert!(h.at(Some((13, 0.5))).is_none());
	}

	#[test]
	fn particles_added_at_either_end_keep_their_partners() {
		let p = |a: &[WorldTri], b: &[WorldTri]| pairs(a, b);
		// Added at the end: ranks pair as they are; the new one has none.
		assert_eq!(p(&[tri(1, 0.0), tri(1, 10.0)], &[tri(1, 1.0), tri(1, 11.0), tri(1, 30.0)]), vec![Some(0), Some(1), None]);
		// Added at the front: ranks shift by the count difference.
		assert_eq!(p(&[tri(1, 0.0), tri(1, 10.0)], &[tri(1, 30.0), tri(1, 1.0), tri(1, 11.0)]), vec![None, Some(0), Some(1)]);
	}

	#[test]
	fn a_falling_quad_keeps_both_halves() {
		// A 1 m streak quad (two triangles sharing an edge) falls 0.8 m: each half's nearest older triangle is the other half.
		let half = |z: f32, up: bool| {
			let mut t = tri(1, 0.0);
			t.pos = if up { [[0.0, 0.0, z], [0.1, 0.0, z], [0.0, 0.0, z + 1.0]] } else { [[0.1, 0.0, z], [0.1, 0.0, z + 1.0], [0.0, 0.0, z + 1.0]] };
			t
		};
		let p = pairs(&[half(0.0, true), half(0.0, false)], &[half(-0.8, true), half(-0.8, false)]);
		assert_eq!(p, vec![Some(0), Some(1)]);
	}

	#[test]
	fn a_standing_splash_is_not_blended_into_its_neighbour() {
		// Rank 0 of the newer field stands where rank 1 of the older one stood (1 m from its rank partner): no pair.
		assert_eq!(pairs(&[tri(1, 0.0), tri(1, 1.0)], &[tri(1, 1.0), tri(1, 5.0)]), vec![None, None]);
	}
}
