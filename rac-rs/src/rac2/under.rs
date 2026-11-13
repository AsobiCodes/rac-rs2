//! [PC] R&C2's camera-under-water flag for the engine's underwater look (parity row 15; engine viewer/underwater.rs, under_detect.rs).
//!
//! The game keeps a word that is 1 while the camera is under water (R&C1 DAT_00167414's twin): written by the hero-state code (group
//! 0x11 = under water), the exit-water code and the camera water probe; read by the fog swap that switches to the underwater fog
//! [C: rac2/notes_gamefacts.md "Swimming and underwater"]. Its address is overlay data and moves per level; it is found by the code
//! that reads it: `lui v0,X; addiu sp,sp,-0x10; lw v1,off(v0); beqz v1,..; sd ra,0(sp)` -> flag = (X << 16) + sext(off) [C: exactly
//! one hit in each of the 581 fg1 bank states, value always 0 or 1; rac2/tools/uwscan.py].
//! The swim states (0x32-0x34 group 0x11 under, 0x35/0x36 group 0x12 surface) set this flag through the same code, so the flag alone
//! is the retail decision; R&C1's extra "hero swims under a sticky surface" case needs R&C2's surface height word, not known [?]:
//! the engine gets no surface (`under_surf` None).
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};

/// Scanned RAM range (the overlay code lives here in every level).
const SCAN: std::ops::Range<usize> = 0x10_0000..0x46_0000;
/// The level the cached address belongs to (i32::MIN = none) and the address (0 = not found).
static LEVEL: AtomicI32 = AtomicI32::new(i32::MIN);
static ADDR: AtomicU32 = AtomicU32::new(0);
/// The code word address of the hit (re-checked each read: the overlay may still be loading when the level word changes).
static CODE: AtomicU32 = AtomicU32::new(0);
/// Reads since the last scan that found nothing: the scan walks 3.5 MB, so a level without the reader (title, menus, loads) is
/// scanned again only every `RETRY` reads (the overlay may still be loading), as fx_pool.rs does.
static MISSES: AtomicU32 = AtomicU32::new(0);
const RETRY: u32 = 60;

fn word(ram: &[u8], a: usize) -> u32 {
	ram.get(a..a + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The flag address of the code at `a`, if the five words there are the reader's prologue.
fn at(ram: &[u8], a: usize) -> Option<u32> {
	let w = |k: usize| word(ram, a + 4 * k);
	let (lui, ld) = (w(0), w(2));
	(lui >> 16 == 0x3c02 && w(1) == 0x27bd_fff0 && ld >> 16 == 0x8c43 && w(3) >> 16 == 0x1060 && w(4) == 0xffbf_0000)
		.then(|| ((lui & 0xffff) << 16).wrapping_add(ld as u16 as i16 as i32 as u32))
}

/// The first reader prologue in the scanned range: (code address, flag address).
pub fn find(ram: &[u8]) -> Option<(u32, u32)> {
	SCAN.step_by(4).find_map(|a| at(ram, a).map(|f| (a as u32, f)))
}

/// The camera-under-water flag of level `level` in `ram` (found once per level; false while not found).
pub fn flag(ram: &[u8], level: i32) -> bool {
	let code = CODE.load(Ordering::Relaxed);
	let retry = code == 0 && MISSES.fetch_add(1, Ordering::Relaxed) % RETRY == 0;
	if LEVEL.load(Ordering::Relaxed) != level || retry || (code != 0 && at(ram, code as usize).is_none()) {
		let hit = crate::scan_timing::timed("under", || find(ram));
		LEVEL.store(level, Ordering::Relaxed);
		CODE.store(hit.map_or(0, |h| h.0), Ordering::Relaxed);
		ADDR.store(hit.map_or(0, |h| h.1), Ordering::Relaxed);
		if let Some((c, f)) = hit {
			eprintln!("rac2: level {level} camera-under-water flag at {f:#x} (reader {c:#x})");
		}
	}
	let a = ADDR.load(Ordering::Relaxed);
	a != 0 && word(ram, (a & 0x01ff_ffff) as usize) == 1
}

/// [PC] The live fog the game's fog-swap function writes: (colour bytes, [near m, far m, near intensity, far intensity]).
pub type Fog = ([u8; 3], [f32; 4]);

/// The fog block of the swap function whose prologue is at `code` [C: rac2/tools + scratch_rc2_b42/b45/p7/fogcheck.py on 25 bank states]:
/// `lui v0,hi` at +0x1c and `addiu v0,v0,lo` at +0x28 give the level's data base B, the live block is F = B + 0x218 (R&C1's
/// SwapWords layout): near = f32(F)/1024 m, far = f32(F+4)/1024 m, intensities f32(F+0x10), f32(F+0x14), colour bytes u32(F+0x18/0x1c/0x20).
/// None when the words there are not that code or the values are not a sane fog (NaN, far <= near, far beyond 1000 m).
pub fn fog_at(ram: &[u8], code: usize) -> Option<Fog> {
	let (lui, add) = (word(ram, code + 0x1c), word(ram, code + 0x28));
	if lui >> 16 != 0x3c02 || add >> 16 != 0x2442 {
		return None;
	}
	let f = (((lui & 0xffff) << 16).wrapping_add(add as u16 as i16 as i32 as u32) & 0x01ff_ffff) as usize + 0x218;
	let fl = |a: usize| f32::from_bits(word(ram, a));
	let fog = [fl(f) / 1024.0, fl(f + 4) / 1024.0, fl(f + 0x10), fl(f + 0x14)];
	let sane = fog.iter().all(|v| v.is_finite()) && fog[0] >= 0.0 && fog[1] > fog[0] && fog[1] <= 1000.0 && fog[2] >= 0.0 && fog[3] >= 0.0;
	sane.then(|| ([word(ram, f + 0x18) as u8, word(ram, f + 0x1c) as u8, word(ram, f + 0x20) as u8], fog))
}

/// The live fog block as of now (after the game's end-of-frame swap of this field), found through the reader `flag` located for
/// this level (so call `flag` first, as `Scene::read` does). Flag clear = camera zone fog, flag set = the underwater constants: one block.
pub fn fog_block(ram: &[u8]) -> Option<Fog> {
	match CODE.load(Ordering::Relaxed) {
		0 => None,
		c => fog_at(ram, c as usize),
	}
}

/// The fog block the draw of this field used: the swap runs last in the game's frame function, so a block read after the field is
/// the NEXT field's, and the draw takes the previous read (R&C1 `fx_queue.rs::lag_swap`, decomp/notes/water.md NOV-28.2).
/// Another level (or no block) resets the lag: that field has no live fog (the level's static fog stays).
pub fn fog_lagged(ram: &[u8], level: i32) -> Option<Fog> {
	static LAG: std::sync::Mutex<Option<(i32, Fog)>> = std::sync::Mutex::new(None);
	let now = fog_block(ram);
	let mut lag = LAG.lock().unwrap_or_else(|e| e.into_inner());
	let prev = lag.filter(|p| p.0 == level).map(|p| p.1);
	*lag = now.map(|f| (level, f));
	prev
}

#[cfg(test)]
mod tests {
	use super::*;

	fn put(ram: &mut [u8], a: usize, w: u32) {
		ram[a..a + 4].copy_from_slice(&w.to_le_bytes());
	}

	#[test]
	fn the_reader_prologue_gives_the_flag_address() {
		let mut ram = vec![0u8; 0x48_0000];
		// Level 4's reader [C: state 0221 flag 0x1b9a00]: lui v0,0x1c; lw v1,-0x6600(v0).
		for (k, w) in [0x3c02_001c, 0x27bd_fff0, 0x8c43_9a00, 0x1060_0005, 0xffbf_0000].into_iter().enumerate() {
			put(&mut ram, 0x30_0000 + 4 * k, w);
		}
		assert_eq!(find(&ram), Some((0x30_0000, 0x1b_9a00)));
		assert!(!flag(&ram, 4));
		put(&mut ram, 0x1b_9a00, 1);
		assert!(flag(&ram, 4));
		// The overlay changed under the same level word: found again (here: gone).
		put(&mut ram, 0x30_0000, 0);
		assert!(!flag(&ram, 4));
		// Not found now: the 3.5 MB scan is skipped for the next reads (the reader coming back is seen at the retry).
		put(&mut ram, 0x30_0000, 0x3c02_001c);
		assert!(!flag(&ram, 4));
		assert_eq!(CODE.load(Ordering::Relaxed), 0);
	}

	/// Fog-block values of the bank states named in the P7 handoff (skipped when the bank is not on this machine).
	#[test]
	fn the_fog_block_reads_the_bank_states() {
		let fog = |st: &str| -> Option<Fog> {
			let ram = std::fs::read(format!("E:/ratchet/rac2/states/fg1/{st}/eeMemory.bin")).ok()?;
			let (code, _) = find(&ram)?;
			fog_at(&ram, code as usize)
		};
		let near = |f: Fog| (f.1[0].round(), f.1[1].round());
		if let Some(f) = fog("0144") {
			assert_eq!(near(f), (80.0, 225.0)); // level 3
		}
		if let Some(f) = fog("0168") {
			assert_eq!(near(f), (0.0, 512.0)); // level 3, another fog zone
		}
		if let Some(f) = fog("0192") {
			assert_eq!(near(f), (50.0, 140.0)); // level 4
			assert_eq!(f.0, [0x42, 0x5c, 0x63]);
			assert!((f.1[2] - 255.0).abs() < 0.5 && (f.1[3] - 140.2).abs() < 0.2);
		}
	}

	#[test]
	fn the_fog_block_follows_the_swap_function_words_and_rejects_junk() {
		let mut ram = vec![0u8; 0x48_0000];
		// lui v0,0x1c ; addiu v0,v0,-0x23c0 -> B = 0x1bdc40 (level 4), F = B + 0x218.
		put(&mut ram, 0x30_001c, 0x3c02_001c);
		put(&mut ram, 0x30_0028, 0x2442_dc40);
		let f = 0x1b_dc40 + 0x218;
		assert_eq!(fog_at(&ram, 0x30_0000), None); // all zero: far <= near
		for (a, v) in [(f, 50.0 * 1024.0), (f + 4, 140.0 * 1024.0), (f + 0x10, 255.0), (f + 0x14, 140.0)] {
			put(&mut ram, a, f32::to_bits(v));
		}
		for (k, c) in [0x42u32, 0x5c, 0x63].into_iter().enumerate() {
			put(&mut ram, f + 0x18 + 4 * k, c);
		}
		assert_eq!(fog_at(&ram, 0x30_0000), Some(([0x42, 0x5c, 0x63], [50.0, 140.0, 255.0, 140.0])));
		put(&mut ram, f + 4, f32::NAN.to_bits());
		assert_eq!(fog_at(&ram, 0x30_0000), None);
		put(&mut ram, 0x30_001c, 0);
		assert_eq!(fog_at(&ram, 0x30_0000), None);
	}
}
