//! R&C2 moby classes the game built in EE RAM outside the level file (engine split stage E5, from lane rc2's `rc2/world/mobies.rs`).

use crate::bytes::{span, u16_at, u32_at, R};
use crate::gamelayout::Game;
use crate::mobies::{decode_class, vif_unpacks, MobyClass};

/// R&C2: a class header the game built in EE RAM outside the level file (a class in the boot data, or one loaded at run
/// time into the buffers at 0x1f00000+) [C: Aranos RAM 0x178590, 0x1f0c000; their pointers are absolute RAM addresses].
/// Decoded from a copy of RAM from the header on with the high and metal packet pointers made relative to it; collision
/// and shadow data are left out; metal packets get chrome slot -2. The game wrote GS TEX0 words over the AD texture slots (AD block +0x20):
/// faces get slot keys `GADGET_TEXTURE_BASE + i` and the returned list holds TEX0 i (at most 16 per class).
pub fn moby_class_ram(ram: &[u8], hdr: usize, id: u32) -> R<(MobyClass, Vec<u64>)> {
	class_from(span(ram, hdr, ram.len().saturating_sub(hdr))?.to_vec(), hdr, id, None, None)
}

/// [b54] The level file's texture slots of a level class decoded from RAM. The game streams a level class's textures through a
/// VRAM ring at the mip level its distance needs and rewrites the TEX0 words in the class's packets whenever a texture moves, so
/// the texels under a TEX0 word are another class's a few fields later [C: Aranos class 0x1419a80, one slot at TBP 0x32e6 32x32,
/// 0x3326 / 0x336b / 0x343f / 0x335b 64x64 within 500 fields, scratch_rc2_rel/b54/tc1]. The level file has what each slot draws:
/// `slot(at)` = the class-local texture index the file holds in the AD texture slot at RAM address `at` (where the game wrote the
/// TEX0), `remap` = the class's 16 level-table bytes (index -> level moby texture, the keys `mobies::moby_class_for` gives).
pub struct FileTex<'a> {
	pub slot: &'a dyn Fn(usize) -> Option<u32>,
	pub remap: &'a [u8],
}

/// [b54] `moby_class_ram` / `moby_class_ram_part` (`part` = first packet, count) of a level class with the level file's textures:
/// the geometry is RAM's, every textured face carries its level texture key and no TEX0 is read from GS memory. An error when a
/// slot is not one of the level file's (the caller then decodes with the TEX0 words as before).
pub fn moby_class_ram_file(ram: &[u8], hdr: usize, id: u32, part: Option<(usize, usize)>, file: &FileTex) -> R<MobyClass> {
	let rest = ram.len().saturating_sub(hdr);
	let mut last: R<MobyClass> = Err("empty class".into());
	for win in [1 << 18, 1 << 20, 1 << 22, usize::MAX] {
		let len = rest.min(win);
		last = span(ram, hdr, len).and_then(|s| class_from(s.to_vec(), hdr, id, part, Some(file))).map(|c| c.0);
		if last.is_ok() || len == rest {
			break;
		}
	}
	last
}

/// One part of a class (`parts`): the packets `first .. first + count` of the class packet table drawn as a mesh of their own.
/// Same class header (skeleton, scale, joint count), no metal packets and no collision; the TEX0 list is the part's own. The packet
/// state the base decode carries from one packet to the next (matrix slots, vertex cache) starts empty here: a part is drawn by
/// itself (a piece), so its packets load what they need [C: retail draws a piece without the base packets].
pub fn moby_class_ram_part(ram: &[u8], hdr: usize, id: u32, first: usize, count: usize) -> R<(MobyClass, Vec<u64>)> {
	// A class blob is a few hundred KB, not the 32 MB of RAM from its header on: try a small copy first, then bigger ones.
	let rest = ram.len().saturating_sub(hdr);
	let mut last: R<(MobyClass, Vec<u64>)> = Err("empty part".into());
	for win in [1 << 18, 1 << 20, 1 << 22, usize::MAX] {
		let len = rest.min(win);
		last = span(ram, hdr, len).and_then(|s| class_from(s.to_vec(), hdr, id, Some((first, count)), None));
		if last.is_ok() || len == rest {
			break;
		}
	}
	last
}

/// R&C2 part table of the class at `hdr` (class header +0x2c = its offset in 16-byte rows from the header; `T`): `T + 2` is the
/// u16 mask of the parts the class has, part `k`'s entry at `T + 4 + 4k` = [near start, near count, far start, far count] into the
/// class packet table (pointer at class +0), k < 15. None when the class has no part table [C: SCES_516.07.c FUN_002a6890 area,
/// scratch_rc2_pic/handoff2.md].
fn part_table(ram: &[u8], hdr: usize) -> Option<usize> {
	let t = hdr + *ram.get(hdr + 0x2c)? as usize * 0x10;
	(*ram.get(hdr + 0x2c)? != 0 && t + 0x40 <= ram.len()).then_some(t)
}

/// One part of a class: its bit in a moby's part mask and its near-LOD packet range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Part {
	pub bit: u32,
	pub first: usize,
	pub count: usize,
}

/// The parts of the class at `hdr` that have near packets (empty without a part table).
pub fn parts(ram: &[u8], hdr: usize) -> Vec<Part> {
	let Some(t) = part_table(ram, hdr) else { return Vec::new() };
	let exist = u16::from_le_bytes([ram[t + 2], ram[t + 3]]) & 0x7fff;
	(0..15u32).filter(|k| exist >> k & 1 != 0).filter_map(|k| {
		let e = t + 4 + 4 * k as usize;
		(ram[e + 1] != 0).then_some(Part { bit: k, first: ram[e] as usize, count: ram[e + 1] as usize })
	}).collect()
}

/// A moby's part mask (record +0x7c) as the draw uses it: `0x8000 | 1 << k` is a piece (only part `k`), a positive mask is the
/// intact moby (the base packets and every part whose bit is set). A piece keeps its bits unfiltered (retail draws part
/// `max(raw & 0x7fff, 1)` without the T+2 filter, SCES_516.07.c 59937-59947: a part the class has no entry for draws nothing, and
/// a bare 0x8000 means part 0, so a piece never falls back to the whole model); only positive masks are filtered by T+2 (the
/// parts the class has). 0 for a class without parts and for a positive mask that names none (those draw the whole class).
pub fn part_mask(ram: &[u8], hdr: usize, raw: u16) -> u16 {
	let Some(t) = part_table(ram, hdr) else { return 0 };
	if raw & 0x8000 != 0 {
		return 0x8000 | (raw & 0x7fff).max(1);
	}
	raw & u16::from_le_bytes([ram[t + 2], ram[t + 3]]) & 0x7fff
}

/// The RAM addresses of the TEX0 words the game wrote into the AD texture slots of the class at `hdr` (the near packets, or the
/// packets `first .. first + count` of `part`), with each word as it is now; untextured slots (-1) are left out. The game streams
/// moby textures through a VRAM cache and rewrites these words when a class's texture moves (scene.rs rechecks them).
pub fn tex0_words(ram: &[u8], hdr: usize, part: Option<(usize, usize)>) -> R<Vec<(u32, u64)>> {
	let abs = |v: u32| (v & 0x01ff_ffff) as usize;
	let mut table = abs(u32_at(ram, hdr)?);
	let mut high = *ram.get(hdr + 4).ok_or("class header cut off")? as usize;
	if let Some((first, count)) = part {
		(table, high) = (table + first * 16, count);
	}
	Ok(ad_tex0s(ram, table, high, abs)?.into_iter().map(|(a, t)| (a as u32, t)).collect())
}

/// [b54] The textured AD slots of a level-file class block (its pointers are relative to the block): the near packets' and every
/// part's, as (offset in the block, class-local texture index). The game loads the block unchanged at core base + class offset, so
/// a slot's RAM address minus the core base is the class offset plus this offset (`FileTex::slot`).
pub fn file_slots(block: &[u8]) -> R<Vec<(u32, u32)>> {
	let mut out: Vec<(u32, u32)> = tex0_words(block, 0, None)?.into_iter().map(|(a, t)| (a, t as u32)).collect();
	for p in parts(block, 0) {
		out.extend(tex0_words(block, 0, Some((p.first, p.count)))?.into_iter().map(|(a, t)| (a, t as u32)));
	}
	Ok(out)
}

/// The GS mip state the game wrote next to a TEX0 in an AD texture slot: TEX1 (MXL, MMIN, L, K, MTBA) and the MIPTBP1 / MIPTBP2 base
/// pointers of levels 1..6 (0 when the slot has no such register) [C: R&C2 round 3 batch 34, crate class 0x1353540 RAM at field 7900:
/// block = TEX1 data 0xff8700000128 at +0x00 (its register field reads 0x41a00004, not 0x14; the GS draws with that TEX1), CLAMP_1
/// (reg 8) +0x10, TEX0_1 (reg 6) +0x20, MIPTBP1_1 (reg 0x34) 0x6c1706e73072ac +0x30; no MIPTBP2 (runs/b34/g6dump, runs/b32/g6.err)].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SlotMip {
	pub tex1: u64,
	pub mip1: u64,
	pub mip2: u64,
}

/// `tex0_words`' slots with the mip registers of each slot's AD block (TEX1 = the data of qword +0x00; MIPTBP1_1/2 = register
/// 0x34/0x35, MIPTBP2_1/2 = 0x36/0x37 in any qword), by TEX0 word (first slot of each word).
pub fn slot_mips(ram: &[u8], hdr: usize, part: Option<(usize, usize)>) -> R<Vec<(u64, SlotMip)>> {
	let mut out: Vec<(u64, SlotMip)> = Vec::new();
	for (at, t) in tex0_words(ram, hdr, part)? {
		if out.iter().any(|e| e.0 == t) {
			continue;
		}
		out.push((t, block_mip(ram, at as usize - 0x20)?));
	}
	Ok(out)
}

/// The RAM addresses and words of the mip registers (`slot_mips`) of every textured slot: watched next to `tex0_words` (the game can
/// move a class's levels without rewriting its TEX0).
pub fn mip_words(ram: &[u8], hdr: usize, part: Option<(usize, usize)>) -> R<Vec<(u32, u64)>> {
	let mut out = Vec::new();
	for (at, _) in tex0_words(ram, hdr, part)? {
		let blk = at as usize - 0x20;
		for q in 0..4 {
			if q == 0 || matches!(*b_at(ram, blk + q * 16 + 8)?, 0x34 | 0x35 | 0x36 | 0x37) {
				let d = u32_at(ram, blk + q * 16)? as u64 | (u32_at(ram, blk + q * 16 + 4)? as u64) << 32;
				out.push(((blk + q * 16) as u32, d));
			}
		}
	}
	Ok(out)
}

fn b_at(b: &[u8], at: usize) -> R<&u8> {
	b.get(at).ok_or_else(|| "AD block cut off".into())
}

/// The mip registers of the AD block at `blk` (4 qwords: data u64, register byte at +8).
fn block_mip(b: &[u8], blk: usize) -> R<SlotMip> {
	let mut m = SlotMip::default();
	for q in 0..4 {
		let d = u32_at(b, blk + q * 16)? as u64 | (u32_at(b, blk + q * 16 + 4)? as u64) << 32;
		match *b_at(b, blk + q * 16 + 8)? {
			_ if q == 0 => m.tex1 = d,
			0x34 | 0x35 => m.mip1 = d,
			0x36 | 0x37 => m.mip2 = d,
			_ => {}
		}
	}
	Ok(m)
}

/// The textured slots of a class's near packets in order (`table` = byte offset of the packet table in `b`, `high` packets; `list`
/// maps an entry's packet-list word to a byte offset in `b`): (byte offset, TEX0) of each slot of the AD data (VIF unpack 2), one
/// TEX0 every 0x40 bytes at +0x20, untextured slots (-1) left out. `class_from` and `tex0_words` both walk with it, so the RAM words
/// the recheck watches are exactly the slots the decode read.
fn ad_tex0s(b: &[u8], table: usize, high: usize, list: impl Fn(u32) -> usize) -> R<Vec<(usize, u64)>> {
	let mut out = Vec::new();
	for k in 0..high {
		let (l, qw) = (list(u32_at(b, table + k * 16)?), u16_at(b, table + k * 16 + 4)? as usize);
		let Some(u) = vif_unpacks(span(b, l, qw * 16)?)?.get(2).map(|u| (u.2.as_ptr() as usize - b.as_ptr() as usize, u.2.len())) else {
			continue;
		};
		for j in 0..u.1 / 0x40 {
			let at = u.0 + j * 0x40 + 0x20;
			if u32_at(b, at)? as i32 == -1 {
				continue;
			}
			out.push((at, u32_at(b, at)? as u64 | (u32_at(b, at + 4)? as u64) << 32));
		}
	}
	Ok(out)
}

/// The decode of `moby_class_ram` / `moby_class_ram_part` on a copy `b` of RAM from the class header `hdr` on (`part` = the packet
/// range of one part, `None` = the whole near LOD). [b54] With `file` every textured slot gets the level file's texture index back
/// and the faces their level texture keys (`FileTex`); the TEX0 list is then empty.
fn class_from(mut b: Vec<u8>, hdr: usize, id: u32, part: Option<(usize, usize)>, file: Option<&FileTex>) -> R<(MobyClass, Vec<u64>)> {
	let rel = |v: u32| -> R<u32> {
		let p = (v & 0x01ff_ffff) as usize;
		p.checked_sub(hdr).map(|x| x as u32).ok_or_else(|| format!("class pointer {p:#x} below the header {hdr:#x}"))
	};
	let put = |b: &mut Vec<u8>, at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
	if b.len() < 0x48 {
		return Err("class header cut off".into());
	}
	let mut table = rel(u32_at(&b, 0)?)? as usize;
	if let Some((first, count)) = part {
		// The part's packets as the whole table: no far LOD, no metal, no colour override.
		(table, b[4], b[5], b[6], b[7], b[0xa]) = (table + first * 16, count as u8, 0, 0, 0, 0xff);
	}
	put(&mut b, 0, table as u32);
	put(&mut b, 0x10, 0);
	b[0xf] = 0;
	let (high, metal, first_metal) = (b[4] as usize, b[6] as usize, b[7] as usize);
	// Metal packets (index + AD unpacks): pointers made relative, their TEX0 (the environment map) back to slot -2.
	let mut metal_ok = true;
	for k in first_metal..first_metal + metal {
		let mut one = || -> R<()> {
			for f in [0, 8] {
				let at = table + k * 16 + f;
				let v = rel(u32_at(&b, at)?)?;
				put(&mut b, at, v);
			}
			let (list, qw) = (u32_at(&b, table + k * 16)? as usize, u16_at(&b, table + k * 16 + 4)? as usize);
			if let Some(u) = vif_unpacks(span(&b, list, qw * 16)?)?.get(1) {
				let (o, n) = (u.2.as_ptr() as usize - b.as_ptr() as usize, u.2.len());
				for j in 0..n / 0x40 {
					let at = o + j * 0x40 + 0x20;
					if (u32_at(&b, at)? as i32) >= -1 || u32_at(&b, at + 4)? != 0xffff_ffff {
						put(&mut b, at, -2i32 as u32);
						put(&mut b, at + 4, 0xffff_ffff);
					}
				}
			}
			Ok(())
		};
		if one().is_err() {
			metal_ok = false;
			break;
		}
	}
	if !metal_ok {
		b[6] = 0;
	}
	for k in 0..high {
		for f in [0, 8] {
			let at = table + k * 16 + f;
			let v = rel(u32_at(&b, at)?)?;
			put(&mut b, at, v);
		}
	}
	// Each textured slot's TEX0 becomes its index in the class's TEX0 list (untextured, -1, stays as it is).
	let mut tex0: Vec<u64> = Vec::new();
	for (at, t) in ad_tex0s(&b, table, high, |v| v as usize)? {
		if let Some(f) = file {
			// [b54] The level file's index of this slot (its RAM address = header + offset in the copy).
			let i = (f.slot)(hdr + at).filter(|&i| i < 16 && f.remap.get(i as usize).is_some_and(|&r| r != 0xff)).ok_or_else(|| format!("slot at {:#x} is not a level-file texture slot", hdr + at))?;
			put(&mut b, at, i);
			continue;
		}
		let i = match tex0.iter().position(|&x| x == t) {
			Some(i) => i,
			None => {
				tex0.push(t);
				tex0.len() - 1
			}
		};
		if i >= 16 {
			return Err("more than 16 textures".into());
		}
		put(&mut b, at, i as u32);
	}
	// A class whose metal packets do not decode is kept without them.
	let remap = file.map(|f| f.remap);
	match decode_class(Game::Rc2, &b, remap, id) {
		Ok(c) => Ok((c, tex0)),
		Err(_) if b[6] != 0 => {
			b[6] = 0;
			Ok((decode_class(Game::Rc2, &b, remap, id)?, tex0))
		}
		Err(e) => Err(e),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// RAM with a class header at `hdr` whose part table (class +0x2c = 8 rows, T = hdr + 0x80, past the 0x48-byte header) has the
	/// parts of `exist`, each entry `[near start, near count, far start, far count]` from `entries` (part number, bytes).
	fn ram_with_parts(hdr: usize, exist: u16, entries: &[(usize, [u8; 4])]) -> Vec<u8> {
		let mut r = vec![0u8; hdr + 0x200];
		r[hdr + 0x2c] = 8;
		let t = hdr + 0x80;
		r[t + 2..t + 4].copy_from_slice(&exist.to_le_bytes());
		for (k, e) in entries {
			r[t + 4 + 4 * k..t + 8 + 4 * k].copy_from_slice(e);
		}
		r
	}

	/// The part table of Aranos' window glass (class 0x43e, `RAC2_CLASSDUMP`): one base packet, parts 0..3 = packets 1..4.
	#[test]
	fn glass_parts() {
		let r = ram_with_parts(0x40, 0xf, &[(0, [1, 1, 0, 0]), (1, [2, 1, 0, 0]), (2, [3, 1, 0, 0]), (3, [4, 1, 0, 0])]);
		let p = parts(&r, 0x40);
		assert_eq!(p.iter().map(|p| (p.bit, p.first, p.count)).collect::<Vec<_>>(), vec![(0, 1, 1), (1, 2, 1), (2, 3, 1), (3, 4, 1)]);
	}

	/// A part with a far range only (no near packets) is left out; a class of 11 parts keeps near starts after its base packets.
	#[test]
	fn parts_need_near_packets() {
		let r = ram_with_parts(0x40, 0x7ff, &[(0, [49, 4, 53, 1]), (1, [54, 2, 56, 1]), (2, [0, 0, 57, 2]), (10, [88, 2, 0, 0])]);
		let p = parts(&r, 0x40);
		assert_eq!(p.iter().map(|p| (p.bit, p.first, p.count)).collect::<Vec<_>>(), vec![(0, 49, 4), (1, 54, 2), (10, 88, 2)]);
	}

	/// No part table (+0x2c = 0), or a table running off the end of RAM: no parts, mask 0 (the class draws whole).
	#[test]
	fn no_table_no_parts() {
		let mut r = ram_with_parts(0x40, 0xf, &[(0, [1, 1, 0, 0])]);
		r[0x40 + 0x2c] = 0;
		assert!(parts(&r, 0x40).is_empty());
		assert_eq!(part_mask(&r, 0x40, 0x8001), 0);
		assert_eq!(part_mask(&r, 0x40, 0x0003), 0);
		let r = ram_with_parts(0x40, 0xf, &[]);
		assert!(parts(&r[..0x40 + 0x30], 0x40).is_empty());
		assert!(parts(&[], 0).is_empty());
	}

	/// `+0x7c`: a piece keeps its bits and the 0x8000 flag unfiltered (a bare 0x8000 is part 0), an intact moby its attached
	/// parts; bits of parts the class lacks go from a positive mask, and a positive mask that names none is 0.
	#[test]
	fn mask_keeps_existing_parts() {
		let r = ram_with_parts(0x40, 0b1011, &[(0, [1, 1, 0, 0])]);
		assert_eq!(part_mask(&r, 0x40, 0x8008), 0x8008);
		assert_eq!(part_mask(&r, 0x40, 0x8004), 0x8004, "part 2 has no entry in parts(): it draws nothing, never the whole model");
		assert_eq!(part_mask(&r, 0x40, 0x8000), 0x8001, "a bare 0x8000 is part 0");
		assert_eq!(part_mask(&r, 0x40, 0x000f), 0b1011);
		assert_eq!(part_mask(&r, 0x40, 0x8000 | 0x000f), 0x800f);
		assert_eq!(part_mask(&r, 0x40, 0), 0);
		assert_eq!(part_mask(&r, 0x40, 0x0010), 0);
	}

	/// A part decode on bytes that cannot hold a class is an error, never a panic (every window size is tried).
	#[test]
	fn part_decode_errors_cleanly() {
		let r = ram_with_parts(0x40, 0xf, &[(0, [1, 1, 0, 0])]);
		assert!(moby_class_ram_part(&r, 0x40, 0, 1, 1).is_err());
		assert!(moby_class_ram_part(&r, 0x40, 0, 255, 255).is_err());
		assert!(moby_class_ram_part(&[], 0, 0, 0, 1).is_err());
	}

	/// RAM with a class header at 0x40 (+0 packet table, +4 near packet count) whose packets each carry three VIF unpacks, the third
	/// (the AD data) holding one 0x40-byte slot per TEX0 of `packets` (-1 = untextured). Packet lists at 0x400 + 0x200 * k, table at
	/// 0x100, pointers with the 0x10000000 uncached bit the game uses.
	fn ram_with_slots(packets: &[&[u64]]) -> Vec<u8> {
		let mut r = vec![0u8; 0x400 + 0x200 * packets.len()];
		let put = |r: &mut Vec<u8>, at: usize, v: u32| r[at..at + 4].copy_from_slice(&v.to_le_bytes());
		put(&mut r, 0x40, 0x1000_0100);
		r[0x44] = packets.len() as u8;
		for (k, slots) in packets.iter().enumerate() {
			let l = 0x400 + 0x200 * k;
			put(&mut r, 0x100 + 16 * k, 0x1000_0000 | l as u32);
			// unpack V4-32 of 1 qword twice, then the AD data: 4 qwords per slot.
			let mut at = l;
			for n in [1usize, 1, 4 * slots.len()] {
				put(&mut r, at, 0x6c00_0000 | (n as u32) << 16);
				at += 4 + 16 * n;
			}
			let qw = (at - l).div_ceil(16);
			r[0x100 + 16 * k + 4..0x100 + 16 * k + 6].copy_from_slice(&(qw as u16).to_le_bytes());
			let ad = l + 4 + 16 + 4 + 16 + 4;
			for (j, &t) in slots.iter().enumerate() {
				put(&mut r, ad + j * 0x40 + 0x20, t as u32);
				put(&mut r, ad + j * 0x40 + 0x24, (t >> 32) as u32);
			}
		}
		r
	}

	/// `tex0_words` returns the TEX0 slots the decode reads (`ad_tex0s`, shared with `class_from`), in packet and slot order, with
	/// their RAM addresses; untextured slots are left out and a part reads only its packet range.
	#[test]
	fn tex0_words_are_the_decoded_slots() {
		let (a, b, c) = (0x8005_cfa5_dd30_b1d2u64, 0x8005_cfa5_dd30_b1d3u64, 0x8005_9145_dd30_b222u64);
		let r = ram_with_slots(&[&[a, 0xffff_ffff_ffff_ffff, b], &[c, a]]);
		let w = tex0_words(&r, 0x40, None).unwrap();
		assert_eq!(w.iter().map(|e| e.1).collect::<Vec<_>>(), vec![a, b, c, a]);
		for &(at, t) in &w {
			assert_eq!(u32_at(&r, at as usize).unwrap() as u64 | (u32_at(&r, at as usize + 4).unwrap() as u64) << 32, t, "the address holds the word");
		}
		// The decode's own walk (pointers already made relative in class_from: here absolute) gives the same slots.
		let walk = ad_tex0s(&r, 0x100, 2, |v| (v & 0x01ff_ffff) as usize).unwrap();
		assert_eq!(walk.iter().map(|e| (e.0 as u32, e.1)).collect::<Vec<_>>(), w);
		let p = tex0_words(&r, 0x40, Some((1, 1))).unwrap();
		assert_eq!(p.iter().map(|e| e.1).collect::<Vec<_>>(), vec![c, a], "part range: packet 1 only");
		assert!(tex0_words(&r[..0x44], 0x40, None).is_err(), "a cut-off header is an error, not an empty list");
	}

	/// [b54] `file_slots` walks a level-file block (header at 0, relative pointers) like `tex0_words` walks RAM: the same slots, as
	/// block offsets with the file's texture index; and a RAM class whose slot the level file does not have is an error (the caller
	/// falls back to the TEX0 words), never a panic.
	#[test]
	fn file_slots_are_the_ram_slots_by_block_offset() {
		let (a, b, c) = (0x8005_cfa5_dd30_b1d2u64, 0x8005_cfa5_dd30_b1d3u64, 0x8005_9145_dd30_b222u64);
		let r = ram_with_slots(&[&[a, 0xffff_ffff_ffff_ffff, b], &[c, a]]);
		let ram_words = tex0_words(&r, 0x40, None).unwrap();
		// The same class as a file block: header at 0, pointers relative to it, each slot holding its class-local index.
		let mut f = r[0x40..].to_vec();
		let put = |f: &mut Vec<u8>, at: usize, v: u32| f[at..at + 4].copy_from_slice(&v.to_le_bytes());
		put(&mut f, 0, 0x100 - 0x40);
		for k in 0..2 {
			put(&mut f, 0xc0 + 16 * k, (0x400 + 0x200 * k - 0x40) as u32);
		}
		for (n, &(at, _)) in ram_words.iter().enumerate() {
			put(&mut f, at as usize - 0x40, [3, 1, 0, 3][n]);
			put(&mut f, at as usize - 0x40 + 4, 0);
		}
		let s = file_slots(&f).unwrap();
		assert_eq!(s, ram_words.iter().zip([3, 1, 0, 3]).map(|(w, i)| (w.0 - 0x40, i)).collect::<Vec<_>>());
		let none = |_: usize| -> Option<u32> { None };
		assert!(moby_class_ram_file(&r, 0x40, 0, None, &FileTex { slot: &none, remap: &[0; 16] }).is_err());
		let unmapped = |_: usize| -> Option<u32> { Some(2) };
		let mut remap = [0u8; 16];
		remap[2] = 0xff;
		assert!(moby_class_ram_file(&r, 0x40, 0, None, &FileTex { slot: &unmapped, remap: &remap }).is_err(), "a slot without a level texture");
		assert!(moby_class_ram_file(&[], 0, 0, Some((0, 1)), &FileTex { slot: &none, remap: &[0; 16] }).is_err());
	}

	/// `slot_mips` reads TEX1 / MIPTBP1 / MIPTBP2 from the TEX0's AD block by register byte, whatever qword holds them; one entry per
	/// distinct TEX0 word; a block without them gives zeros.
	#[test]
	fn slot_mips_reads_the_block_registers() {
		let (a, b) = (0x8005_cc65_dd30_b29bu64, 0x8005_9145_dd30_b222u64);
		let mut r = ram_with_slots(&[&[a, b], &[a]]);
		let w = tex0_words(&r, 0x40, None).unwrap();
		let blk = w[0].0 as usize - 0x20;
		let put64 = |r: &mut Vec<u8>, at: usize, v: u64| r[at..at + 8].copy_from_slice(&v.to_le_bytes());
		put64(&mut r, blk, 0x0000_0fb0_0000_0108);
		put64(&mut r, blk + 8, 0x41a0_0004);
		put64(&mut r, blk + 0x10, 0x0000_0000_0123_4567);
		put64(&mut r, blk + 0x18, 0x34);
		put64(&mut r, blk + 0x30, 0x89ab);
		put64(&mut r, blk + 0x38, 0x36);
		let m = slot_mips(&r, 0x40, None).unwrap();
		assert_eq!(m.len(), 2, "one entry per distinct word");
		assert_eq!(m[0], (a, SlotMip { tex1: 0x0000_0fb0_0000_0108, mip1: 0x0123_4567, mip2: 0x89ab }));
		assert_eq!(m[1].1, SlotMip::default(), "a zero block: no TEX1, no bases");
		let w = mip_words(&r, 0x40, None).unwrap();
		assert_eq!(&w[..3], &[(blk as u32, 0x0000_0fb0_0000_0108), (blk as u32 + 0x10, 0x0123_4567), (blk as u32 + 0x30, 0x89ab)], "slot 0's registers");
		assert_eq!(w.len(), 3 + 2, "the other slots: their TEX1 qword only");
	}
}
