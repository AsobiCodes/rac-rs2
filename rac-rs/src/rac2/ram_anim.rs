//! R&C2 moby posing straight from a RAM image (EE RAM, 32 MB, absolute addresses): skeleton from the class header, the two
//! animation frames the engine holds in the moby record, blend phase, joint chain, skinning matrices.
//! Layout, evidence and open points: rac2/notes_anim.md. The maths is rac-formats' `anim.rs` (same engine; its `skin` and
//! `skin_metal` skin with these matrices); what is R&C2 specific is where the state lives (moby +0x40.. / +0x58 / +0x5c) and the
//! compressed frame format (`decompress_frame`). From lane rc2's world/anim.rs.
//!
//! Matrices are affine 3x4 row-major `[r00 r01 r02 tx, r10 .. ty, r20 .. tz]`, column-vector convention. `pose_from_ram`
//! returns one per joint `World_j * InvBind_j` with the translation scaled to game units, so they act directly on
//! `MobyClass.mesh.positions` (model space), exactly like rac-rs `anim::skin`'s input.

use rac_formats::bytes::{f32_at, span, u16_at, u32_at, R};

type Mat = [[f32; 4]; 3];
const IDENTITY: Mat = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];

fn mul(a: &Mat, b: &Mat) -> Mat {
	let mut o = [[0.0; 4]; 3];
	for i in 0..3 {
		for j in 0..4 {
			o[i][j] = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j] + if j == 3 { a[i][3] } else { 0.0 };
		}
	}
	o
}

fn flat(m: &Mat) -> [f32; 12] {
	[m[0][0], m[0][1], m[0][2], m[0][3], m[1][0], m[1][1], m[1][2], m[1][3], m[2][0], m[2][1], m[2][2], m[2][3]]
}

fn quat_mat(q: [f32; 4]) -> [[f32; 3]; 3] {
	let [x, y, z, w] = q;
	[
		[1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w)],
		[2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w)],
		[2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y)],
	]
}

fn normalize4(q: [f32; 4]) -> [f32; 4] {
	let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
	if n > 0.0 { q.map(|x| x / n) } else { [0.0, 0.0, 0.0, 1.0] }
}

/// Rotation channel (R&C1 0x268a80 / Eudora 0x2469d0 rules, carried over [I] for R&C2): t == 0 keeps frame A raw; a
/// same-sequence consecutive step (`direct`) lerps without normalisation; otherwise the sign of a.b picks sum or difference
/// and the result is normalised.
fn blend_rotation(qa: [f32; 4], qb: [f32; 4], t: f32, direct: bool) -> [f32; 4] {
	if t.to_bits() == 0 {
		return qa;
	}
	if direct {
		return [0, 1, 2, 3].map(|i| qa[i] * (1.0 - t) + qb[i] * t);
	}
	let qb = if qa.iter().zip(&qb).map(|(x, y)| x * y).sum::<f32>() < 0.0 { qb.map(|x| -x) } else { qb };
	normalize4([0, 1, 2, 3].map(|i| qa[i] * (1.0 - t) + qb[i] * t))
}

/// Per class skeleton: parents, bind translations (raw s16 units), inverse bind matrices, raw -> game unit factor.
#[derive(Clone)]
pub struct Rig {
	pub parents: Vec<Option<usize>>,
	pub bind_translation: Vec<[f32; 3]>,
	pub inverse_bind: Vec<Mat>,
	pub unit: f32,
}

/// Class header fields (absolute RAM pointers): +8 joint count, +0xc sequence count, +0x14 skeleton table (64 bytes per
/// joint), +0x18 bone data (16 bytes per joint), +0x24 scale, +0x48 sequence pointers.
pub fn rig_from_ram(ram: &[u8], class: u32) -> R<Rig> {
	let c = class as usize;
	let joints = span(ram, c + 8, 1)?[0] as usize;
	let skel = u32_at(ram, c + 0x14)? as usize;
	let bones = u32_at(ram, c + 0x18)? as usize;
	let unit = f32_at(ram, c + 0x24)? / 1024.0;
	let (mut parents, mut bind, mut inv) = (Vec::new(), Vec::new(), Vec::new());
	for j in 0..joints {
		let b = span(ram, bones + 16 * j, 16)?;
		bind.push([f32_at(b, 0)?, f32_at(b, 4)?, f32_at(b, 8)?]);
		let (parent, marker) = (u16_at(b, 12)? as usize / 64, u16_at(b, 14)?);
		parents.push(if marker == 0 || parent >= j { None } else { Some(parent) });
		let m = span(ram, skel + 64 * j, 64)?;
		let f = |i: usize| f32_at(m, i * 4);
		// [R^T | c]: qword k of the entry is column k of R (mat[r][k] = lane r of qword k), qword 3 lanes xyz = c.
		let mut mat = IDENTITY;
		for r in 0..3 {
			for k in 0..3 {
				mat[r][k] = f(k * 4 + r)?;
			}
			mat[r][3] = f(12 + r)?;
		}
		inv.push(mat);
	}
	Ok(Rig { parents, bind_translation: bind, inverse_bind: inv, unit })
}

/// One decoded frame record (header 16 bytes + body: joints x 8 rotation, scale records, translation records).
#[derive(Clone, Debug)]
pub struct Frame {
	pub speed: f32,
	pub time: u16,
	/// (x, y, z, w) as `s16 / 32768`, w sign-corrected (stored negated), not normalised.
	pub rotation: Vec<[f32; 4]>,
	/// Scale records with flags bit 0x80 (inherited by children).
	pub scale: Vec<(usize, [f32; 3])>,
	/// Scale records with bit 0x80 clear (applied to the world matrix after the chain; children do not inherit).
	pub post_scale: Vec<(usize, [f32; 3])>,
	/// Local translation records in raw units.
	pub translation: Vec<(usize, [f32; 3])>,
}

/// Parse an uncompressed frame record at absolute address `at`.
pub fn parse_frame(ram: &[u8], at: usize, joints: usize) -> R<Frame> {
	let h = span(ram, at, 16)?;
	let speed = f32_at(h, 0)?;
	let time = u16_at(h, 4)?;
	let size = u16_at(h, 6)? as usize * 16;
	let (scale_off, scale_n, trans_off, trans_n) = (u16_at(h, 8)? as usize, u16_at(h, 10)? as usize, u16_at(h, 12)? as usize, u16_at(h, 14)? as usize);
	let body = span(ram, at + 16, size)?;
	let mut rotation = Vec::with_capacity(joints);
	for j in 0..joints.min(scale_off / 8) {
		let b = span(body, j * 8, 8)?;
		let s = |i: usize| i16::from_le_bytes([b[i * 2], b[i * 2 + 1]]) as f32 / 32768.0;
		rotation.push([s(0), s(1), s(2), -s(3)]);
	}
	rotation.resize(joints, [0.0, 0.0, 0.0, 1.0]);
	let (mut scale, mut post_scale) = (Vec::new(), Vec::new());
	for i in 0..scale_n {
		let b = span(body, scale_off + i * 8, 8)?;
		let s = |k: usize| u16::from_le_bytes([b[k * 2], b[k * 2 + 1]]) as f32 / 4096.0;
		(if b[7] & 0x80 != 0 { &mut scale } else { &mut post_scale }).push((b[6] as usize, [s(0), s(1), s(2)]));
	}
	let mut translation = Vec::new();
	for i in 0..trans_n {
		let b = span(body, trans_off + i * 8, 8)?;
		let s = |k: usize| i16::from_le_bytes([b[k * 2], b[k * 2 + 1]]) as f32;
		translation.push((b[6] as usize, [s(0), s(1), s(2)]));
	}
	Ok(Frame { speed, time, rotation, scale, post_scale, translation })
}

/// The retail frame decompressor FUN_002a1b10(table, frame word, out, joints) [C], returned as the bytes it writes into a
/// frame buffer (parse them with `parse_frame` after placing them in a RAM image, or use `frame_from_word`).
/// `seq` = sequence header address; the frame word is `*(seq + 0x1c + frame * 4)` (top nibble 0xf = compressed).
/// Table = `seq + frame_count * 4 + 0x1c + u8(seq + 0x13) * 4`: u32 stream layout word, then per joint 3 x u16 base values
/// (low 2 bits = how the frame's data changes the value), then u8 scale count, u8 translation count, then the 8-byte base
/// records of the scale then translation sparse lists.
pub fn decompress_frame(ram: &[u8], seq: usize, word: u32, joints: usize) -> R<Vec<u8>> {
	let frames = span(ram, seq + 0x10, 1)?[0] as usize;
	let tbl = seq + frames * 4 + 0x1c + span(ram, seq + 0x13, 1)?[0] as usize * 4;
	let hdr = u32_at(ram, tbl)?;
	let d = (word & 0x0fff_ffff) as usize;
	let ticks = u16_at(ram, d)?;
	let time = u16_at(ram, d + 2)?;
	let ns = span(ram, tbl + joints * 6 + 4, 1)?[0] as usize;
	let nt = span(ram, tbl + joints * 6 + 5, 1)?[0] as usize;
	let size_qw = (joints + ns + nt + 1) >> 1;
	let mut out = vec![0u8; 16 + size_qw * 16];
	let speed = if ticks != 0 { 1.0f32 / ticks as f32 } else { 0.0 };
	out[0..4].copy_from_slice(&speed.to_le_bytes());
	out[4..6].copy_from_slice(&time.to_le_bytes());
	out[6..8].copy_from_slice(&(size_qw as u16).to_le_bytes());
	out[8..10].copy_from_slice(&((joints * 8) as u16).to_le_bytes());
	out[10..12].copy_from_slice(&(ns as u16).to_le_bytes());
	out[12..14].copy_from_slice(&(((joints + ns) * 8) as u16).to_le_bytes());
	out[14..16].copy_from_slice(&(nt as u16).to_le_bytes());
	// Streams: 10-bit signed fields (3 per u32), u16 literals, signed bytes (x4), control/sign bytes.
	let mut p10 = d + 4;
	let mut shift = 22i32;
	let lit_base = d + 4 + (hdr & 0x3ff) as usize;
	let mut lit = lit_base;
	let mut sb = d + 4 + ((hdr >> 10) & 0x7ff) as usize;
	let mut cb = d + (hdr >> 21) as usize + 4;
	let mut value = |base: u16, p10: &mut usize, shift: &mut i32, lit: &mut usize, sb: &mut usize| -> R<u16> {
		Ok(match base & 3 {
			1 => {
				let v = span(ram, *sb, 1)?[0] as i8 as i32;
				*sb += 1;
				base.wrapping_add((v * 4) as u16)
			}
			2 => {
				let w = u32_at(ram, *p10)? as i32;
				let f = (w << (*shift & 31)) >> 22;
				*shift -= 10;
				if *shift < 1 {
					*shift = 22;
					*p10 += 4;
				}
				base.wrapping_add((f as i16 as i32 * 4) as u16)
			}
			3 => {
				let v = u16_at(ram, *lit)?;
				*lit += 2;
				v
			}
			_ => base,
		})
	};
	let mut ox = 16usize;
	for j in 0..joints {
		let e = span(ram, tbl + 4 + j * 6, 6)?;
		for k in 0..3 {
			let v = value(u16::from_le_bytes([e[k * 2], e[k * 2 + 1]]), &mut p10, &mut shift, &mut lit, &mut sb)?;
			out[ox + k * 2..ox + k * 2 + 2].copy_from_slice(&v.to_le_bytes());
		}
		ox += 8;
	}
	// w from x, y, z (VU: 1 - x^2 - y^2 - z^2, max 0, sqrt, FTOI15 truncation, min 0x7fff), sign bit from the control stream.
	let mut bits = span(ram, cb, 1)?[0] as u32;
	cb += 1;
	let mut left = 8i32;
	for j in 0..joints {
		let o = 16 + j * 8;
		let c = |k: usize| i16::from_le_bytes([out[o + k * 2], out[o + k * 2 + 1]]) as f32 / 32768.0;
		let (x, y, z) = (c(0), c(1), c(2));
		let w2 = (((1.0 - x * x) - y * y) - z * z).max(0.0);
		let mut w = ((w2.sqrt() * 32768.0) as i32).min(0x7fff);
		if left < 1 {
			bits = span(ram, cb, 1)?[0] as u32;
			cb += 1;
			left = 7;
		} else {
			left -= 1;
		}
		if bits & 1 != 0 {
			w = -w;
		}
		bits >>= 1;
		out[o + 6..o + 8].copy_from_slice(&(w as i16).to_le_bytes());
	}
	// Sparse scale then translation records: 8-byte base record (3 x u16 + joint/flags u16), control byte with 2 bits per
	// component: 0 base, 1 base + i8 * 4, 2 literal s16.
	let mut rec = tbl + joints * 6 + 6;
	for _ in 0..ns + nt {
		let b = span(ram, rec, 8)?;
		rec += 8;
		out[ox + 6..ox + 8].copy_from_slice(&b[6..8]);
		let ctl = span(ram, cb, 1)?[0];
		cb += 1;
		for k in 0..3 {
			let base = u16::from_le_bytes([b[k * 2], b[k * 2 + 1]]);
			let v = match (ctl >> (k * 2)) & 3 {
				1 => {
					let v = span(ram, cb, 1)?[0] as i8 as i32;
					cb += 1;
					base.wrapping_add((v * 4) as u16)
				}
				2 => {
					let v = u16_at(ram, cb)?;
					cb += 2;
					v
				}
				_ => base,
			};
			out[ox + k * 2..ox + k * 2 + 2].copy_from_slice(&v.to_le_bytes());
		}
		ox += 8;
	}
	Ok(out)
}

/// A frame from a stored frame word (as in the sequence's frame table, or a moby pointer): compressed ones are expanded.
pub fn frame_from_word(ram: &[u8], seq: usize, word: u32, joints: usize) -> R<Frame> {
	if word >> 28 == 0 {
		return parse_frame(ram, word as usize, joints);
	}
	let bytes = decompress_frame(ram, seq, word, joints)?;
	parse_frame(&bytes, 0, joints)
}

/// What the moby record holds [C: FUN_0029fd50 / FUN_0029fef0 / FUN_0029fe10]: frame indices +0x40 (A) / +0x41 (B), sequence
/// ids +0x42 (A's; 0xff = A is a snapshot buffer) / +0x43 (B's), blend phase f32 +0x44, frame pointers +0x58 (A) / +0x5c (B).
#[derive(Clone, Copy, Debug)]
pub struct MobyAnim {
	pub class: u32,
	pub frame_a: u8,
	pub frame_b: u8,
	pub seq_a: u8,
	pub seq_b: u8,
	pub t: f32,
	pub ptr_a: u32,
	pub ptr_b: u32,
}

pub fn moby_anim(ram: &[u8], moby: u32) -> R<MobyAnim> {
	let m = moby as usize;
	let b = span(ram, m + 0x40, 4)?;
	Ok(MobyAnim {
		class: u32_at(ram, m + 0x24)?,
		frame_a: b[0],
		frame_b: b[1],
		seq_a: b[2],
		seq_b: b[3],
		t: f32_at(ram, m + 0x44)?,
		ptr_a: u32_at(ram, m + 0x58)?,
		ptr_b: u32_at(ram, m + 0x5c)?,
	})
}

/// Sequence header address of sequence `seq` of a class (class +0x48 table of absolute pointers).
pub fn sequence_addr(ram: &[u8], class: u32, seq: usize) -> R<usize> {
	let n = span(ram, class as usize + 0xc, 1)?[0] as usize;
	if seq >= n {
		return Err(format!("sequence {seq} >= count {n}"));
	}
	let a = u32_at(ram, class as usize + 0x48 + seq * 4)? as usize;
	if a == 0 { Err(format!("sequence {seq} has no data")) } else { Ok(a) }
}

/// Local joint state (rotation, scale, raw translation) of the blend of frame A and B at `t`, then the joint override
/// records `fx` of that joint in chain order (rac-rs `local_fx_parts`, retail FUN_00268a80 rules [I for R&C2]).
fn locals(rig: &Rig, a: &Frame, b: &Frame, t: f32, direct: bool, fx: &[JointFx]) -> Vec<Mat> {
	let n = rig.parents.len();
	let mut sa = vec![[1.0f32; 3]; n];
	let mut sb = sa.clone();
	let mut ta = rig.bind_translation.clone();
	let mut tb = ta.clone();
	for &(j, s) in &a.scale { if j < n { sa[j] = s; } }
	for &(j, s) in &b.scale { if j < n { sb[j] = s; } }
	for &(j, v) in &a.translation { if j < n { ta[j] = v; } }
	for &(j, v) in &b.translation { if j < n { tb[j] = v; } }
	(0..n)
		.map(|j| {
				let mut q = blend_rotation(a.rotation[j], b.rotation[j], t, direct);
			let mut s = [0, 1, 2].map(|i| sa[j][i] + (sb[j][i] - sa[j][i]) * t);
			let mut tr = [0, 1, 2].map(|i| ta[j][i] + (tb[j][i] - ta[j][i]) * t);
			for f in fx.iter().filter(|f| f.joint == j) {
				let [x, y, z, w] = q;
				let [rx, ry, rz, rw] = f.rot;
				match f.blend {
					None => {
						q = [rw * x + rx * w + ry * z - rz * y, rw * y - rx * z + ry * w + rz * x, rw * z + rx * y - ry * x + rz * w, rw * w - rx * x - ry * y - rz * z];
						s = [0, 1, 2].map(|i| s[i] * f.scale[i]);
						tr = [0, 1, 2].map(|i| tr[i] + f.off[i]);
					}
					Some(wt) => {
						let u = 1.0 - wt;
						let av = [x * u, y * u, z * u, w * u];
						let bv = [rx * wt, ry * wt, rz * wt, rw * wt];
						let d = ((av[1] * bv[1] + av[0] * bv[0]) + av[2] * bv[2]) + av[3] * bv[3];
						let v = if d.is_sign_negative() { [0, 1, 2, 3].map(|i| av[i] - bv[i]) } else { [0, 1, 2, 3].map(|i| av[i] + bv[i]) };
						let nn = ((v[0] * v[0] + v[1] * v[1]) + v[2] * v[2]) + v[3] * v[3];
						if nn > 0.0 {
							let inv = 1.0 / nn.sqrt();
							q = v.map(|c| c * inv);
						}
						s = [0, 1, 2].map(|i| s[i] * u + f.scale[i] * wt);
						tr = [0, 1, 2].map(|i| tr[i] * u + f.off[i] * wt);
					}
				}
			}
			let r = quat_mat(q);
			let mut m = IDENTITY;
			for i in 0..3 {
				for c in 0..3 {
					m[i][c] = r[i][c] * s[c];
				}
				m[i][3] = tr[i];
			}
			m
		})
		.collect()
}

/// The non-inherited scale pass (R&C1 0x2438a8 rules, carried over [I]): order of visit, last record wins per joint.
fn post_scale_list(a: &Frame, b: &Frame, t: f32) -> Vec<(usize, [f32; 3])> {
	let raw = t.to_bits() == 0;
	let mut order: Vec<usize> = a.post_scale.iter().map(|e| e.0).collect();
	if !raw {
		let mut seen_b: Vec<usize> = Vec::new();
		for &(j, _) in &b.post_scale {
			if !seen_b.contains(&j) {
				seen_b.push(j);
				order.push(j);
			}
		}
	}
	let last = |f: &Frame, j: usize| f.post_scale.iter().rev().find(|e| e.0 == j).map_or([1.0; 3], |e| e.1);
	let mut out = Vec::new();
	let mut i = 0;
	while i < order.len() {
		let j = order[i];
		let sa = last(a, j);
		out.push((j, if raw { sa } else { let sb = last(b, j); [0, 1, 2].map(|k| sa[k] * (1.0 - t) + sb[k] * t) }));
		i = order.iter().rposition(|&x| x == j).unwrap() + 1;
	}
	out
}

/// Skinning matrices of frames `a`..`b` at phase `t`: `World_j * InvBind_j`, translation in game units.
pub fn pose_frames(rig: &Rig, a: &Frame, b: &Frame, t: f32, direct: bool) -> Vec<[f32; 12]> {
	pose_frames_fx(rig, a, b, t, direct, &[])
}

/// `pose_frames` with joint override records.
pub fn pose_frames_fx(rig: &Rig, a: &Frame, b: &Frame, t: f32, direct: bool, fx: &[JointFx]) -> Vec<[f32; 12]> {
	let local = locals(rig, a, b, t, direct, fx);
	let mut world: Vec<Mat> = Vec::with_capacity(local.len());
	for (j, l) in local.iter().enumerate() {
		world.push(match rig.parents[j] {
			Some(p) => mul(&world[p], l),
			None => *l,
		});
	}
	for (j, s) in post_scale_list(a, b, t) {
		if let Some(w) = world.get_mut(j) {
			for r in 0..3 {
				for c in 0..3 {
					w[r][c] *= s[c];
				}
			}
		}
	}
	world
		.iter()
		.zip(&rig.inverse_bind)
		.map(|(w, inv)| {
			let mut m = mul(w, inv);
			for r in 0..3 {
				m[r][3] *= rig.unit;
			}
			flat(&m)
		})
		.collect()
}

/// World matrix of a joint in model space (game units) from a `pose_frames` result: `skin * bind_world`. Needs the joint's
/// bind world matrix = inverse of `[R^T | c]`; cheap form: posed joint origin = skin * bind origin.
pub fn bind_world(rig: &Rig, joint: usize) -> [f32; 12] {
	let m = &rig.inverse_bind[joint];
	let a = |r: usize, c: usize| m[r][c];
	let cof = |r: usize, c: usize| {
		let (r1, r2, c1, c2) = ((r + 1) % 3, (r + 2) % 3, (c + 1) % 3, (c + 2) % 3);
		a(r1, c1) * a(r2, c2) - a(r1, c2) * a(r2, c1)
	};
	let det = a(0, 0) * cof(0, 0) + a(0, 1) * cof(0, 1) + a(0, 2) * cof(0, 2);
	let inv_det = if det.abs() > 1e-12 { 1.0 / det } else { 0.0 };
	let mut o = IDENTITY;
	for r in 0..3 {
		for c in 0..3 {
			o[r][c] = cof(c, r) * inv_det;
		}
	}
	for r in 0..3 {
		o[r][3] = -(o[r][0] * m[0][3] + o[r][1] * m[1][3] + o[r][2] * m[2][3]) * rig.unit;
	}
	flat(&o)
}

/// Posed joint frame (3x4, model space, game units) for joint `j`.
pub fn joint_world(rig: &Rig, mats: &[[f32; 12]], j: usize) -> [f32; 12] {
	let s = mats[j];
	let s: Mat = [[s[0], s[1], s[2], s[3]], [s[4], s[5], s[6], s[7]], [s[8], s[9], s[10], s[11]]];
	let b = bind_world(rig, j);
	let b: Mat = [[b[0], b[1], b[2], b[3]], [b[4], b[5], b[6], b[7]], [b[8], b[9], b[10], b[11]]];
	flat(&mul(&s, &b))
}

/// The moby's current pose from RAM: frames A and B at the pointers the engine keeps in the record (already decompressed
/// into a frame buffer for compressed sequences, e.g. the hero), blend phase +0x44. Returns one matrix per joint of the
/// class (empty for classes without joints).
///
/// The moby's weighted layers (+0x50: the arms holding a weapon) are blended over the base frame and its joint override
/// records (+0x54: head look-at, hidden parts) applied, as rac-rs `Animated::list_frame` + `pose_frames_fx` (notes_anim.md
/// "Layers"). Without them the held gun and the helmet, which the game places at the real hand / head joints, float off the
/// drawn Ratchet.
pub fn pose_from_ram(ram: &[u8], moby: u32) -> Result<Vec<[f32; 12]>, String> {
	let (rig, a, b, t, direct) = moby_frames(ram, moby)?;
	let n = rig.parents.len();
	let fx = joint_records(ram, moby);
	let layers = layer_list(ram, moby, n);
	if layers.is_empty() {
		return Ok(pose_frames_fx(&rig, &a, &b, t, direct, &fx));
	}
	// rac-rs `list_frame`: the base blend (normalised nlerp), each layer's own FROM -> TO blend mixed in over its joints.
	let mut m = blend_frames(&rig, &a, &b, t, |qa, qb| blend_rotation(qa, qb, t, false));
	m.post_scale = post_scale_list(&a, &b, t);
	for l in &layers {
		let lay = blend_frames(&rig, &l.from, &l.to, l.t, |qa, qb| blend_rotation(qa, qb, l.t, l.consecutive));
		mix_layer(&mut m, &lay, l.weight, &l.joints, n);
	}
	Ok(pose_frames_fx(&rig, &m, &m, 0.0, false, &fx))
}

/// One joint override record of the moby's +0x54 chain (R&C1 +0x64; 0xb0-byte records, next +8) [C: layout = R&C1's on
/// level 1 field 2675: hero records joint 18 / 7 rotations, joints 94 / 100 scale 0.01]: `rot` = the conjugate of the
/// quaternion at +0x10 (rac-rs `JointFx.rot`), `scale` +0x20, `off` +0x30 (raw units); `blend` None = mode 0 (+3 == 0:
/// multiply / scale / add), Some(+0xc) = mode 1 (weighted).
#[derive(Clone, Copy, Debug)]
pub struct JointFx {
	pub joint: usize,
	pub rot: [f32; 4],
	pub scale: [f32; 3],
	pub off: [f32; 3],
	pub blend: Option<f32>,
}

/// The +0x54 chain in chain order (at most 64 records).
pub fn joint_records(ram: &[u8], moby: u32) -> Vec<JointFx> {
	let w = |a: u32| u32_at(ram, a as usize).unwrap_or(0);
	let f = |a: u32| f32_at(ram, a as usize).unwrap_or(0.0);
	let mut v = Vec::new();
	let mut r = w(moby + 0x54) & 0x01ff_ffff;
	while r != 0 && v.len() < 64 && (r as usize) + 0x40 <= ram.len() {
		let rec = JointFx {
			joint: ((w(r + 4) & 0xffff) / 0x40) as usize,
			rot: [-f(r + 0x10), -f(r + 0x14), -f(r + 0x18), f(r + 0x1c)],
			scale: [f(r + 0x20), f(r + 0x24), f(r + 0x28)],
			off: [f(r + 0x30), f(r + 0x34), f(r + 0x38)],
			blend: (ram[r as usize + 3] != 0).then(|| f(r + 0xc)),
		};
		if rec.rot.iter().chain(&rec.scale).chain(&rec.off).all(|x| x.is_finite()) {
			v.push(rec);
		}
		r = w(r + 8) & 0x01ff_ffff;
	}
	v
}

/// One weighted layer object of the moby's +0x50 chain (R&C1 +0x60, next +0x1c, at most 6) [C: layout = R&C1's on level 1
/// field 2675: hero jtab 12, 20 joints 52..71, weight 1.0]: +2 joint count, +8 weight, +0xc joint list (bytes), +0x20 / +0x21
/// FROM / TO frame, +0x22 / +0x23 FROM / TO sequence, +0x24 t; +0x38 / +0x3c the FROM / TO frames as built (plain frame
/// records, also for frames taken from a hold set), read directly.
pub struct Layer {
	pub from: Frame,
	pub to: Frame,
	pub t: f32,
	pub consecutive: bool,
	pub weight: f32,
	pub joints: Vec<usize>,
}

/// The layers with weight, joints and both frames.
pub fn layer_list(ram: &[u8], moby: u32, joints: usize) -> Vec<Layer> {
	let w = |a: u32| u32_at(ram, a as usize).unwrap_or(0);
	let mut v = Vec::new();
	let mut o = w(moby + 0x50) & 0x01ff_ffff;
	let mut k = 0;
	while o != 0 && k < 6 && (o as usize) + 0x40 <= ram.len() {
		k += 1;
		let weight = f32_at(ram, o as usize + 8).unwrap_or(0.0);
		let n = u16_at(ram, o as usize + 2).unwrap_or(0).min(128) as usize;
		let list = (w(o + 0xc) & 0x01ff_ffff) as usize;
		let (fa, fb) = (w(o + 0x38) & 0x01ff_ffff, w(o + 0x3c) & 0x01ff_ffff);
		if weight != 0.0 && weight.is_finite() && n > 0 && fa != 0 && fb != 0 {
			if let (Ok(from), Ok(to), Ok(js)) = (parse_frame(ram, fa as usize, joints), parse_frame(ram, fb as usize, joints), span(ram, list, n)) {
				let b = &ram[o as usize..];
				let t = f32_at(ram, o as usize + 0x24).unwrap_or(0.0);
				v.push(Layer {
					from,
					to,
					t: if t.is_finite() { t } else { 0.0 },
					consecutive: b[0x22] == b[0x23] && b[0x20].wrapping_add(1) == b[0x21],
					weight,
					joints: js.iter().map(|&j| j as usize).collect(),
				});
			}
		}
		o = w(o + 0x1c) & 0x01ff_ffff;
	}
	v
}

/// Every joint of `a` / `b` (scale 1 and the bind translation where a frame has no record) blended: rotation by `rot`,
/// scale and translation linearly (rac-rs `blend_frames`; no post-scale records).
fn blend_frames(rig: &Rig, a: &Frame, b: &Frame, t: f32, rot: impl Fn([f32; 4], [f32; 4]) -> [f32; 4]) -> Frame {
	let n = rig.parents.len();
	let (mut sa, mut ta) = (vec![[1.0f32; 3]; n], rig.bind_translation.clone());
	let (mut sb, mut tb) = (sa.clone(), ta.clone());
	for &(j, s) in &a.scale { if j < n { sa[j] = s; } }
	for &(j, s) in &b.scale { if j < n { sb[j] = s; } }
	for &(j, v) in &a.translation { if j < n { ta[j] = v; } }
	for &(j, v) in &b.translation { if j < n { tb[j] = v; } }
	let raw = t.to_bits() == 0;
	let mix = |x: f32, y: f32| if raw { x } else { x * (1.0 - t) + y * t };
	let q = |f: &Frame, j: usize| f.rotation.get(j).copied().unwrap_or([0.0, 0.0, 0.0, 1.0]);
	Frame {
		speed: a.speed,
		time: a.time,
		rotation: (0..n).map(|j| rot(q(a, j), q(b, j))).collect(),
		scale: (0..n).map(|j| (j, [0, 1, 2].map(|k| mix(sa[j][k], sb[j][k])))).collect(),
		post_scale: Vec::new(),
		translation: (0..n).map(|j| (j, [0, 1, 2].map(|k| mix(ta[j][k], tb[j][k])))).collect(),
	}
}

/// One weighted layer over `joints` (rac-rs `mix_layer`): rotation nlerp toward the layer (shortest way), scale and
/// translation linear.
fn mix_layer(m: &mut Frame, lay: &Frame, w: f32, joints: &[usize], n: usize) {
	for &j in joints.iter().filter(|&&j| j < n) {
		let (bq, lq) = (m.rotation[j], lay.rotation[j]);
		let sg = if bq.iter().zip(&lq).map(|(x, y)| x * y).sum::<f32>() < 0.0 { -1.0 } else { 1.0 };
		m.rotation[j] = normalize4([0, 1, 2, 3].map(|i| bq[i] * (1.0 - w) + sg * w * lq[i]));
		let mix = |x: [f32; 3], y: [f32; 3]| [0, 1, 2].map(|i| x[i] * (1.0 - w) + y[i] * w);
		m.scale[j].1 = mix(m.scale[j].1, lay.scale[j].1);
		m.translation[j].1 = mix(m.translation[j].1, lay.translation[j].1);
	}
}

/// `pose_from_ram` pieces: rig, frame A, frame B, phase, same-sequence-step flag (retail's 0x400 test:
/// `((B idx - A idx) & 0xfffe) + (A seq - B seq) == 0`, [I] carried from R&C1).
pub fn moby_frames(ram: &[u8], moby: u32) -> R<(Rig, Frame, Frame, f32, bool)> {
	let st = moby_anim(ram, moby)?;
	if st.class == 0 {
		return Err("moby has no class".into());
	}
	let rig = rig_from_ram(ram, st.class)?;
	let joints = rig.parents.len();
	let get = |ptr: u32, seq: u8, idx: u8| -> R<Frame> {
		if ptr >> 28 == 0 && ptr != 0 {
			return parse_frame(ram, ptr as usize, joints);
		}
		let s = sequence_addr(ram, st.class, seq as usize)?;
		let w = u32_at(ram, s + 0x1c + idx as usize * 4)?;
		frame_from_word(ram, s, w, joints)
	};
	let a = get(st.ptr_a, st.seq_a, st.frame_a)?;
	let b = get(st.ptr_b, st.seq_b, st.frame_b)?;
	let d = ((st.frame_b as u32).wrapping_sub(st.frame_a as u32) & 0xfffe).wrapping_add((st.seq_a as u32).wrapping_sub(st.seq_b as u32));
	Ok((rig, a, b, st.t.clamp(0.0, 1.0), d == 0))
}

/// Pose of frame `frame_a` -> `frame_b` of sequence `seq` of the class at `class` (decompressing as needed), phase `t`.
pub fn pose_sequence(ram: &[u8], class: u32, seq: usize, frame_a: usize, frame_b: usize, t: f32) -> R<Vec<[f32; 12]>> {
	let rig = rig_from_ram(ram, class)?;
	let joints = rig.parents.len();
	let s = sequence_addr(ram, class, seq)?;
	let n = span(ram, s + 0x10, 1)?[0] as usize;
	if frame_a >= n || frame_b >= n {
		return Err(format!("frame {frame_a}/{frame_b} >= {n}"));
	}
	let word = |f: usize| u32_at(ram, s + 0x1c + f * 4);
	let a = frame_from_word(ram, s, word(frame_a)?, joints)?;
	let b = frame_from_word(ram, s, word(frame_b)?, joints)?;
	Ok(pose_frames(&rig, &a, &b, t, frame_b == frame_a + 1))
}

