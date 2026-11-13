//! Per-level shop words of the Gadgetron vendor, found by code words (lane scr): every level's overlay has its own copy of the
//! quantity prompt (boot ELF 0x2f9898, `FUN_002f9898`), moved and linked against that level's data. The copy is found by its loose
//! code words (`modcam_probe::loose`: 8 anchor words, then an FNV-1a hash over 120 words) and the addresses come out of it:
//! - word 1 `lui` + word 3 `addiu` = the vendor block: current row +0x80, stage +0x84, the row list +0x140;
//! - word 27 `lui` + word 30 `addiu` = the item record table (0xe0 per record);
//! - word 42 `sw v1, imm(gp)` = the quantity word, at `GP + imm` (gp 0x1af070 on every level [C regs.bin, bank 0125 0187 0260 0473 0555]).
//! [C: one match each in bank 0051 0125 (level 2: entry 0x37d7f8), 0187 (L4 0x3999f0), 0260 (L6 0x3b7428), 0473 (L11 0x38a758),
//! 0555 (L12 0x37b680); level 2 gives exactly the words read before from the replay dumps.] Tool: scratch_rc2_scr\tools\vqfind.py.

use crate::modcam_probe::{hash_loose, loose, lui_addiu};
use crate::modern_vendor::Words;
use std::sync::Mutex;

/// The game's gp register (the same on every level [C regs.bin of 5 bank states]).
pub const GP: u32 = 0x1af070;
const LO: usize = 0x280000;
const HI: usize = 0x400000;
const ANCHOR: [u32; 8] = [0x27bd_0000, 0x3c02_0000, 0xffb0_0000, 0x2447_0000, 0xffb5_0000, 0x0080_802d, 0xffb1_0000, 0x0040_a82d];
const WORDS_N: usize = 120;
const HASH: u64 = 0x7939_211e_07c6_3256;
/// Word indexes of the address-building instructions.
const AT_BASE: (usize, usize) = (1, 3);
const AT_TABLE: (usize, usize) = (27, 30);
const AT_QTY: usize = 42;
/// Calls between two scans while a level has none (an overlay still loading).
const RETRY_EVERY: u32 = 30;

fn rd32(b: &[u8], a: usize) -> Option<u32> {
	b.get(a..a.checked_add(4)?).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// The address words of a copy at `a` (the exact words, so a new overlay at the same place is seen).
fn sig(ram: &[u8], a: usize) -> Option<[u32; 5]> {
	let w = |i: usize| rd32(ram, a + i * 4);
	Some([w(AT_BASE.0)?, w(AT_BASE.1)?, w(AT_TABLE.0)?, w(AT_TABLE.1)?, w(AT_QTY)?])
}

/// The words a copy at `a` builds (None: not the prompt, or addresses outside RAM).
pub fn decode(ram: &[u8], a: usize, level: u32) -> Option<Words> {
	if !ANCHOR.iter().enumerate().all(|(i, w)| rd32(ram, a + i * 4).is_some_and(|x| loose(x) == *w)) {
		return None;
	}
	let body = ram.get(a..a + WORDS_N * 4)?;
	if hash_loose(body.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))) != HASH {
		return None;
	}
	let [b0, b1, t0, t1, q] = sig(ram, a)?;
	let base = lui_addiu(b0, b1)?;
	let table = lui_addiu(t0, t1)?;
	// sw v1, imm(gp): op 0x2b, base register 28.
	if q >> 26 != 0x2b || (q >> 21) & 31 != 28 {
		return None;
	}
	let qty = GP.wrapping_add(q as u16 as i16 as i32 as u32);
	let w = Words { level, cur: base + 0x80, stage: base + 0x84, qty, rows: base + 0x140, table };
	let inside = |x: u32| (x as usize) < ram.len() && x >= 0x10_0000;
	(inside(w.cur) && inside(w.rows) && inside(w.qty) && inside(w.table)).then_some(w)
}

/// Scans the overlay range for the prompt (the first match).
pub fn find(ram: &[u8], level: u32) -> Option<(usize, Words)> {
	let last = HI.min(ram.len().checked_sub(WORDS_N * 4)?);
	(LO..last).step_by(4).find_map(|a| (rd32(ram, a).is_some_and(|x| loose(x) == ANCHOR[0])).then(|| decode(ram, a, level)).flatten().map(|w| (a, w)))
}

#[derive(Default)]
struct Cache {
	level: Option<u32>,
	hit: Option<(usize, Words, [u32; 5])>,
	misses: u32,
}

static CACHE: Mutex<Cache> = Mutex::new(Cache { level: None, hit: None, misses: 0 });

/// The shop words of `level`: kept while the code under them is unchanged, scanned again on a level change or a new overlay.
pub fn words(ram: &[u8], level: u32) -> Option<Words> {
	let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
	if c.level == Some(level) {
		match c.hit {
			Some((a, w, s)) if sig(ram, a) == Some(s) && rd32(ram, a).is_some_and(|x| loose(x) == ANCHOR[0]) => return Some(w),
			Some(_) => c.hit = None,
			None => {
				c.misses += 1;
				if c.misses % RETRY_EVERY != 0 {
					return None;
				}
			}
		}
	}
	c.level = Some(level);
	c.misses = 0;
	c.hit = find(ram, level).and_then(|(a, w)| Some((a, w, sig(ram, a)?)));
	if let Some((a, w, _)) = c.hit {
		eprintln!("rac2 vendor: level {} quantity prompt {:#x}: block {:#x} rows {:#x} table {:#x} qty {:#x}", level, a, w.cur - 0x80, w.rows, w.table, w.qty);
	}
	c.hit.map(|h| h.1)
}
