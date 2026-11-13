//! R&C2's moby shadow records read from RAM as the engine's shadow casters (`shadowgeom::RecCaster`; the engine builds and draws
//! the volumes, viewer/shadowvol.rs). R&C2 keeps R&C1's rings (sphere ring and capsule ring decode word for word [C: Aranos
//! 0x35c160 = Novalis 0x29b8b8 83/83 words, 0x35c2b4 ~ 0x29ba08 361/403]) but clips them between two planes along the caster's up
//! axis instead of two world heights, so a shadow lies on slopes and walls too. From lane rc2's world/shadows.rs.
//!
//! Record (Aranos 0x1fb6a0, built each drawn frame by 0x35bae0) [C]: {prim count, straddle (list entry +6), d (moby +0x70 through
//! VU0), h (moby +0x74)}, the moby's up axis (rotation row +0xe0), the light (slot 0x1b1ed0 + (+0xbd) * 16), then the prims as in
//! R&C1: type 0 sphere (0x20 B: {0, n, 0, 0, centre xyz, radius}), type 1 capsule (0x30 B: {1, n0, n1, 0, p0 xyz r0, p1 xyz r1}).
//! A zero prim count ends the records.
//!
//! Driver 0x35b908 [C]: per record shadow_basis(1000, light) (0x35b398); plane P0 = (up, -d - h), P1 = (up, h - d); per prim the
//! ring (0x35b4a8), then the clip 0x35c900: top_i moved along (bottom_i - top_i) onto P0, bottom_i onto P1. The faces (0x35b500
//! tags 2 and 1) and the darken (0x35b738, ALPHA FIX 0x20: colour x 0.75) are R&C1's.

use rac_engine::shadowgeom::{RecCaster, RecPrim};

/// Reads the records at `records`; stops at a zero or implausible prim count, an unknown prim type or the build's end guard
/// records + 0x1f80.
pub fn read_records(m: &ee_oracle::Machine, records: u32) -> Vec<RecCaster> {
	read_indexed(m, records).into_iter().map(|e| e.1).collect()
}

/// `read_records` with each record's index in the game's caster list (record i is list entry i; a record that fails the sanity test is
/// left out, so the indices can skip): the frame smoothing blends a record with the same caster's record of the field before it
/// ([b45 P7] smooth.rs `blend_shadows`; R&C1 interp.rs blends them the same way).
pub fn read_indexed(m: &ee_oracle::Machine, records: u32) -> Vec<(usize, RecCaster)> {
	let end = records + 0x1f80;
	let mut out = Vec::new();
	let mut r = records;
	'rec: for idx in 0..48 {
		let n = m.rd32(r);
		if n == 0 || n > 64 || r + 0x30 > end {
			break;
		}
		let (d, h) = (m.rdf(r + 8), m.rdf(r + 12));
		let mut rc = RecCaster { straddle: m.rd32(r + 4) != 0, band: [d - h, d + h], up: m.rdv3(r + 0x10), light: m.rdv3(r + 0x20), prims: Vec::new() };
		r += 0x30;
		for _ in 0..n {
			match m.rd32(r) {
				0 if r + 0x20 <= end => {
					rc.prims.push(RecPrim::Sphere { c: m.rdv3(r + 0x10), r: m.rdf(r + 0x1c), n: m.rd32(r + 4) });
					r += 0x20;
				}
				1 if r + 0x30 <= end => {
					rc.prims.push(RecPrim::Capsule { p0: m.rdv3(r + 0x10), r0: m.rdf(r + 0x1c), p1: m.rdv3(r + 0x20), r1: m.rdf(r + 0x2c), n0: m.rd32(r + 4), n1: m.rd32(r + 8) });
					r += 0x30;
				}
				_ => break 'rec,
			}
		}
		let l = rc.light;
		let ok = [d, h].iter().chain(rc.up.iter()).chain(l.iter()).all(|x| x.is_finite()) && l[0] * l[0] + l[1] * l[1] + l[2] * l[2] > 1e-6;
		if ok {
			out.push((idx, rc));
		}
	}
	out
}

#[cfg(test)]
mod tests {
	use rac_engine::shadowgeom::{record_volumes, RecCaster, RecPrim};

	type V3 = [f32; 3];

	/// One sphere above flat ground (up z, slab 0.8..1.2, the game's slot-0 light (0.14 cos a, 0.14 sin a, -0.99)): the volume
	/// darkens the ground under it (one more front than back face on a ray from above to the ground) and nothing beside it; a
	/// tilted up axis (a slope) still darkens the point under the sphere on that slope.
	#[test]
	fn sphere_over_ground_and_slope() {
		let sub = |a: V3, b: V3| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
		let dot = |a: V3, b: V3| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
		let cross = |a: V3, b: V3| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
		let eye = [0.0, -0.3, 10.0];
		let hits = |tris: &[[V3; 3]], g: V3| {
			tris.iter()
				.filter(|t| {
					let (e1, e2) = (sub(t[1], t[0]), sub(t[2], t[0]));
					let d = sub(g, eye);
					let p = cross(d, e2);
					let det = dot(e1, p);
					if det.abs() < 1e-9 {
						return false;
					}
					let s = sub(eye, t[0]);
					let u = dot(s, p) / det;
					let q = cross(s, e1);
					let v = dot(d, q) / det;
					let k = dot(e2, q) / det;
					u >= 0.0 && v >= 0.0 && u + v <= 1.0 && k > 0.0 && k < 1.0
				})
				.count() as i32
		};
		for (up, ground) in [([0.0, 0.0, 1.0], [0.05, 0.05, 1.0]), ([0.0, 0.1736, 0.9848], [0.05, 0.05, 1.0])] {
			let rc = RecCaster { straddle: false, band: [0.8, 1.2], up, light: [0.12, 0.07, -0.99], prims: vec![RecPrim::Sphere { c: [0.0, 0.0, 3.0], r: 0.5, n: 8 }] };
			let vols = record_volumes(&[rc], eye, &|_| false);
			assert_eq!(vols.len(), 1);
			// The ground point on the slab's mid plane under the sphere.
			let g = { let k = (1.0 - dot(up, ground)) / up[2]; [ground[0], ground[1], ground[2] + k] };
			let count = |g: V3| hits(&vols[0].inc, g) - hits(&vols[0].dec, g);
			assert_eq!(count(g), 1, "up {up:?}");
			assert_eq!(count([2.0, 0.0, 1.0]), 0, "up {up:?}");
		}
	}
}
