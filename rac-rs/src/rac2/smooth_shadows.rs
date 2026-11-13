//! [PC] [b45 P7] Frame smoothing of the shadow-volume records: a record is blended with the record of the same caster moby in the field
//! before it (R&C1: rac1/interp.rs `blend_shadows`), so a shadow under a gliding moby glides with it instead of stepping at the game's
//! rate. A record without a partner (new caster, other prim shapes, a jump) is the newest field's as it is.
use rac_engine::shadowgeom::{RecCaster, RecPrim};

fn near(a: [f32; 3], b: [f32; 3]) -> bool {
	(0..3).all(|k| (a[k] - b[k]).abs() < 4.0)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
	a + (b - a) * t
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
	[lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t)]
}

/// The prims of `a` -> `b` at `t` (None: other shapes or a jump, so the newest stays).
fn prims(a: &[RecPrim], b: &[RecPrim], t: f32) -> Option<Vec<RecPrim>> {
	if a.len() != b.len() {
		return None;
	}
	a.iter()
		.zip(b)
		.map(|(pa, pb)| match (*pa, *pb) {
			(RecPrim::Sphere { c: ca, r: ra, n: na }, RecPrim::Sphere { c: cb, r: rb, n: nb }) if na == nb && near(ca, cb) => Some(RecPrim::Sphere { c: lerp3(ca, cb, t), r: lerp(ra, rb, t), n: nb }),
			(RecPrim::Capsule { p0: a0, r0: ar0, p1: a1, r1: ar1, n0: an0, n1: an1 }, RecPrim::Capsule { p0: b0, r0: br0, p1: b1, r1: br1, n0, n1 }) if (an0, an1) == (n0, n1) && near(a0, b0) && near(a1, b1) => {
				Some(RecPrim::Capsule { p0: lerp3(a0, b0, t), r0: lerp(ar0, br0, t), p1: lerp3(a1, b1, t), r1: lerp(ar1, br1, t), n0, n1 })
			}
			_ => None,
		})
		.collect()
}

/// `b`'s records (`b_own`: the caster of each) blended toward `a`'s (`a_own`) at `t` (0 = `a`, 1 = `b`).
pub fn blend(a: &[RecCaster], a_own: &[u32], b: &[RecCaster], b_own: &[u32], t: f32) -> Vec<RecCaster> {
	b.iter()
		.enumerate()
		.map(|(i, c)| {
			let owner = b_own.get(i).copied().unwrap_or(0);
			let p = (owner != 0).then(|| a_own.iter().position(|&o| o == owner)).flatten().and_then(|j| a.get(j));
			match p.and_then(|p| prims(&p.prims, &c.prims, t).map(|pr| (p, pr))) {
				Some((p, pr)) => RecCaster {
					prims: pr,
					light: lerp3(p.light, c.light, t),
					band: [lerp(p.band[0], c.band[0], t), lerp(p.band[1], c.band[1], t)],
					..c.clone()
				},
				None => c.clone(),
			}
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;

	fn rec(x: f32) -> RecCaster {
		RecCaster { band: [0.0, 2.0], prims: vec![RecPrim::Sphere { c: [x, 0.0, 0.0], r: 1.0, n: 8 }], light: [0.0, 0.0, -1.0], ..RecCaster::default() }
	}

	#[test]
	fn a_record_follows_its_own_caster_not_its_list_place() {
		// The older field listed caster 0x20 first; the newest lists 0x10 first: each record blends with its own caster's.
		let a = [rec(10.0), rec(0.0)];
		let b = [rec(1.0), rec(11.0)];
		let o = blend(&a, &[0x20, 0x10], &b, &[0x10, 0x20], 0.5);
		let cx = |c: &RecCaster| match c.prims[0] {
			RecPrim::Sphere { c, .. } => c[0],
			_ => f32::NAN,
		};
		assert_eq!((cx(&o[0]), cx(&o[1])), (0.5, 10.5));
		// A new caster, or a jump of more than a few metres, stays as the newest field has it.
		let o = blend(&a, &[0x20, 0x10], &[rec(30.0)], &[0x20], 0.5);
		assert_eq!(cx(&o[0]), 30.0);
		let o = blend(&a, &[0x20, 0x10], &[rec(5.0)], &[0x99], 0.5);
		assert_eq!(cx(&o[0]), 5.0);
	}
}
