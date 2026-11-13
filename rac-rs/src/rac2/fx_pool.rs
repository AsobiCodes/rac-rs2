//! [PC] R&C2's effect pool (R&C1's twin) read from RAM for the Fire, Smoke and weapon-light rows (parity rows 30-32; engine firefx,
//! volsmoke, shotlight). The picture of every effect stays the captured VU1 program-2 record (rac2/host/ov_layers.rs); the pool only
//! says WHAT a record is: its effect type (slot byte +0) and the slot's own size, colour and fade byte, matched to the record by the
//! world point (record qw0 xyz = slot +0x10; host/ov_fire.rs).
//!
//! Facts [C: rac2/notes_fx_pool.md sections 1-2, 59/59 states found, slots decoded in 3 levels]:
//! - Everything the pool code addresses is overlay data and moves per level; gp is 0x1af070 in every level. Found by the allocator's
//!   prologue, the first hit in 0x100000..0x460000: `lw t8,X(gp)` (0x8f98XXXX), `lui t7,0x20` (0x3c0f0020), `addiu t7,t7,L`
//!   (0x25efLLLL), `lw t9,X+4(gp)` (0x8f99 with X+4). Base pointer = gp+X-4, high-water index = gp+X+4, occupancy bitmap = 0x200000 +
//!   sext(L) (one bit per slot).
//! - 0x800 slots x 0x40 bytes, R&C1's layout byte for byte: +0 type, +1 flags (bit 7 parked, bits 0-1 mode: 0 point sprite, 1 ground
//!   quad, 2 line, 3 ribbon), +2 texture, +3 ALPHA (0x44 alpha-over, else additive), +4 RGBA (R low, 0x80 = 1.0), +8 rotation, +9
//!   near/far fade byte, +0xc f32 size, +0x10 xyz world point.
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

const GP: u32 = 0x1a_f070;
const SCAN: std::ops::Range<usize> = 0x10_0000..0x46_0000;
const SLOTS: u32 = 0x800;

/// One live point sprite of the pool (mode 0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
	pub index: u16,
	pub ty: u8,
	pub tex: u8,
	pub alpha_op: u8,
	pub rot: u8,
	pub b9: u8,
	pub rgba: u32,
	pub size: f32,
	pub pos: [f32; 3],
}

fn word(ram: &[u8], a: u32) -> u32 {
	let a = (a & 0x01ff_ffff) as usize;
	ram.get(a..a + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The pool words of the allocator prologue at `a`: (gp offset X of the next-free index, bitmap address).
fn at(ram: &[u8], a: u32) -> Option<(u32, u32)> {
	let w = |k: u32| word(ram, a + 4 * k);
	let (lw, lui, addiu, lw2) = (w(0), w(1), w(2), w(3));
	let x = lw & 0xffff;
	(lw >> 16 == 0x8f98 && lui == 0x3c0f_0020 && addiu >> 16 == 0x25ef && lw2 >> 16 == 0x8f99 && lw2 & 0xffff == (x + 4) & 0xffff)
		.then(|| (x as u16 as i16 as i32 as u32, 0x20_0000u32.wrapping_add(addiu as u16 as i16 as i32 as u32)))
}

/// The first allocator prologue: (code address, X, bitmap).
pub fn find(ram: &[u8]) -> Option<(u32, u32, u32)> {
	SCAN.step_by(4).find_map(|a| at(ram, a as u32).map(|(x, bm)| (a as u32, x, bm)))
}

/// The level the cached prologue belongs to and its code address (0 = none).
static LEVEL: AtomicI32 = AtomicI32::new(i32::MIN);
static CODE: AtomicU32 = AtomicU32::new(0);
static MISSES: AtomicU32 = AtomicU32::new(0);
const RETRY: u32 = 60;

/// The live point sprites of level `level`'s pool in `ram` (empty while not found).
pub fn read(ram: &[u8], level: i32) -> Vec<Slot> {
	let code = CODE.load(Ordering::Relaxed);
	// Not found in this level: scan again only every `RETRY` reads (the scan walks 3.5 MB; the overlay may still be loading).
	let retry = code == 0 && MISSES.fetch_add(1, Ordering::Relaxed) % RETRY == 0;
	if LEVEL.load(Ordering::Relaxed) != level || retry || (code != 0 && at(ram, code).is_none()) {
		let hit = find(ram);
		LEVEL.store(level, Ordering::Relaxed);
		CODE.store(hit.map_or(0, |h| h.0), Ordering::Relaxed);
		if let Some((c, x, bm)) = hit {
			eprintln!("rac2: level {level} effect pool: alloc prologue {c:#x}, base word {:#x}, bitmap {bm:#x}", GP.wrapping_add(x).wrapping_sub(4));
		}
	}
	let code = CODE.load(Ordering::Relaxed);
	let Some((x, bm)) = (code != 0).then(|| at(ram, code)).flatten() else { return Vec::new() };
	slots(ram, word(ram, GP.wrapping_add(x).wrapping_sub(4)), word(ram, GP.wrapping_add(x).wrapping_add(4)), bm)
}

/// The mode-0 slots of the pool at `base` up to the high-water index `hw`, by the occupancy bitmap `bm`.
fn slots(ram: &[u8], base: u32, hw: u32, bm: u32) -> Vec<Slot> {
	let base = base & 0x01ff_ffff;
	if base == 0 || (hw as i32) < 0 {
		return Vec::new();
	}
	let byte = |a: u32| ram.get((a & 0x01ff_ffff) as usize).copied().unwrap_or(0);
	let f = |a: u32| f32::from_bits(word(ram, a));
	let mut out = Vec::new();
	for i in 0..=hw.min(SLOTS - 1) {
		let s = base + 0x40 * i;
		let fl = byte(s + 1);
		if byte(bm + (i >> 3)) >> (i & 7) & 1 == 0 || fl & 0x80 != 0 || fl & 3 != 0 {
			continue;
		}
		let pos = [f(s + 0x10), f(s + 0x14), f(s + 0x18)];
		if !pos.iter().all(|v| v.is_finite()) {
			continue;
		}
		out.push(Slot { index: i as u16, ty: byte(s), tex: byte(s + 2), alpha_op: byte(s + 3), rot: byte(s + 8), b9: byte(s + 9), rgba: word(ram, s + 4), size: f(s + 0xc), pos });
	}
	out
}

/// The render thread wants the pool (Fire / Smoke / weapon lights can use it: host/ov_fire.rs); the game thread reads it only then.
static WANT: AtomicBool = AtomicBool::new(false);

pub fn set_wanted(on: bool) {
	WANT.store(on, Ordering::Relaxed);
}

/// [b47 X1] Does the render thread want the pool (fx_shots.rs reads the shots under the same switch)?
pub fn wanted() -> bool {
	WANT.load(Ordering::Relaxed)
}

/// The pool of the last fields, by field (game thread -> render thread).
static FIELDS: Mutex<VecDeque<(u64, Arc<Vec<Slot>>)>> = Mutex::new(VecDeque::new());
const KEEP: usize = 8;

/// Game thread, after field `field` (host/game.rs): keeps that field's pool while the render thread wants it.
pub fn note(field: u64, ram: &[u8], level: i32) {
	if !WANT.load(Ordering::Relaxed) {
		return;
	}
	let slots = Arc::new(read(ram, level));
	let mut q = FIELDS.lock().unwrap_or_else(|e| e.into_inner());
	q.retain(|f| f.0 < field);
	q.push_back((field, slots));
	while q.len() > KEEP {
		q.pop_front();
	}
}

/// The pool kept for `field`.
pub fn of_field(field: u64) -> Option<Arc<Vec<Slot>>> {
	FIELDS.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|f| f.0 == field).map(|f| f.1.clone())
}

#[cfg(test)]
mod tests {
	use super::*;

	fn put(ram: &mut [u8], a: u32, w: u32) {
		ram[a as usize..a as usize + 4].copy_from_slice(&w.to_le_bytes());
	}

	#[test]
	fn the_prologue_finds_the_pool_and_only_live_point_sprites_are_read() {
		let mut ram = vec![0u8; 0x200_0000];
		// st21's alloc [C: gp+0x3724 next-free, base word gp+0x3720 = 0x1b2790, bitmap 0x1fe880 = 0x200000 - 0x1780].
		for (k, w) in [0x8f98_3724u32, 0x3c0f_0020, 0x25ef_e880, 0x8f99_3728].into_iter().enumerate() {
			put(&mut ram, 0x32_7800 + 4 * k as u32, w);
		}
		assert_eq!(find(&ram), Some((0x32_7800, 0x3724, 0x1f_e880)));
		put(&mut ram, 0x1b_2790, 0x01c2_e100);
		put(&mut ram, 0x1b_2798, 3); // high-water
		let base = 0x1c2_e100;
		for i in 0..4u32 {
			let s = base + 0x40 * i;
			ram[s as usize] = 4 + i as u8;
			ram[s as usize + 1] = [0x40, 0x41, 0xc0, 0x40][i as usize];
			put(&mut ram, s + 4, 0x3bff_7f7f);
			put(&mut ram, s + 0xc, 63000.0f32.to_bits());
			put(&mut ram, s + 0x10, (1.0 + i as f32).to_bits());
		}
		ram[0x1f_e880] = 0b0000_0111; // slot 3 not occupied
		let got = read(&ram, 7);
		// Slot 1 is a ground quad (mode 1), slot 2 parked, slot 3 free: only slot 0.
		assert_eq!(got.len(), 1);
		assert_eq!((got[0].index, got[0].ty, got[0].rgba, got[0].size, got[0].pos), (0, 4, 0x3bff_7f7f, 63000.0, [1.0, 0.0, 0.0]));
	}

	#[test]
	fn fields_are_kept_only_while_wanted_and_the_newest_eight() {
		let ram = vec![0u8; 0x48_0000];
		set_wanted(false);
		note(1, &ram, 0);
		assert!(of_field(1).is_none());
		set_wanted(true);
		for f in 10..20 {
			note(f, &ram, 0);
		}
		assert!(of_field(11).is_none() && of_field(12).is_some() && of_field(19).is_some());
		set_wanted(false);
	}
}
