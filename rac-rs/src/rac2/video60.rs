//! [PC] The "60 Hz mode" row (user 2026-11-11, "Yes, add both"): runs R&C2 PAL in the game's own 60 Hz (NTSC) mode.
//! HIDDEN AND INERT (2026-11-11, agent b60i): lib.rs does not call `register` (no Hand bound), the row is in no menu, the pc_options
//! key is ignored and oracle/src/rc2/cold_hooks.txt has no rule for these entries: nothing is written, retail 50 Hz. Why [C runs
//! scratch_rc2_s144/runs/p60e/v60.err (title switch, b60h) and the boot-time run below]: with the PAL flag 0 the first level hangs
//! at field 1397 (rec_r2l0_08 f_318438 reads [a0+0x5c] = 0x3ed55555, a float, as a pointer) whether the flag is switched at the
//! title or set before the boot init, so the NTSC mode itself fails on the PAL disc's level code/data [I]. Kept as the research
//! state for a later fix.
//!
//! THE GAME'S MODES [C rac2/notes_menus.md item 5 "PAL FLAG"; boot ELF + level overlays]: the word `addrs::PAL_FLAG` (0x1a7c18)
//! picks every 50/60 Hz difference of the code: the video reset FUN_002848c0 (SetGsCrt PAL omode 3 / NTSC omode 2), the frame
//! buffers + display rectangle FUN_0027b928, the frame-time scale of the main loop FUN_0026ec50, the projection FUN_0027af28, the
//! sound/movie scales FUN_00337aa0, and in every level overlay the timing set (dt 1/60 + scale 1.0 for 0, dt 1/50 + scale 1.2 for
//! 1) that the overlay re-syncs from the word on its own. The PAL disc has the word 1 in its data and in the boot (FUN_002914e0
//! 0x29163c); no retail code writes 0 to it on PAL, so retail PAL never reaches 60 Hz. The hidden "Video mode" dialog (type 0xb)
//! toggles a different word, `addrs::VIDEO_60` (0x1a7c40) = 480p progressive (omode 0x50), which FUN_002848c0 clears while the PAL
//! flag is set. PC DEVIATION: this row sets the PAL flag to 0 = the state the same code has on an NTSC console (timing included),
//! and runs the game's own resets for it.
//!
//! BOOT TIME (an NTSC console has the flag 0 from its disc's data): the row is read once, on entry of the game's boot init
//! FUN_002914e0 (cold-hook rule `ret 2914e0 hand ..` in oracle/src/rc2/cold_hooks.txt; the boot runs interpreted). Row On = the flag
//! is 0 before the init's own video reset FUN_002848c0 (SetGsCrt omode 2; the oracle's field clock follows, hle.rs) and frame
//! buffers / display rectangle FUN_0027b928 run, so everything the boot sizes is the NTSC console's. The init's later store of 1
//! (0x29163c, and its copy in each level overlay) is undone at the callee's entry (`after_init_store`). A row change applies at the
//! next start of the game (row note). An earlier version switched at the title menu: buffers sized at boot for PAL then hung the
//! first level (field 1396, rec_r2l0_08 f_318438 read a float as a pointer) [C b60h].
//! The flag is in the card's Options block (save table entry {0x1a7c18, 0x28} [C 0x256138]): the save writer runs with the disc's 1
//! (`save`, the card stays a retail PAL save), the loader's value goes back to 0 (`load`), and `guard` catches any other write.
//! Off (the default) = no write anywhere: retail 50 Hz.
use ee_oracle::cpu::Cpu;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// The game's boot init FUN_002914e0 [C boot ELF, resident]: entry, end of the hashed words (its first 12, up to the init's first
/// video reset call), FNV hash of those words (`ee_oracle::rec::hash`; [C] SCES_516.07 = RAM of fg1/0000).
pub const BOOT_INIT: (u32, u32, u64) = (0x2914e0, 0x291510, 0x1a26_565e_2688_463e);

/// The game init's PAL-flag store `li v0,1 ... jal F; sw v0,-0x7458(gp)` (boot FUN_002914e0 0x29163c, and the copy of that init in
/// every level's code overlay, run when the game code starts): F = FUN_00299550 (the memory card names from the disc id), the
/// same 0x170 bytes everywhere [C fg1 states 0000..0581]. Its entries per overlay (`ret ENTRY hand 27bdffe0 0080302d` rules in
/// oracle/src/rc2/cold_hooks.txt list the same); levels 13..21 / 23 had no state to read [?]: there `guard` alone puts it back.
pub const STORE_CALLEE: [u32; 18] = [
	0x299550, 0x30ba80, 0x3012a8, 0x31e0f8, 0x30fbc0, 0x30bca8, 0x30cd90, 0x32c3c8, 0x323da0, 0x34afc0, 0x309d28, 0x301c68, 0x318008,
	0x307d18, 0x310f68, 0x3264a8, 0x31ddb8, 0x30f4e0,
];
pub const STORE_CALLEE_LEN: u32 = 0x170;
pub const STORE_CALLEE_HASH: u64 = 0x9e03_9cdc_541c_6a31;

const fn store_hook(i: usize) -> ee_oracle::rec::Hand {
	let e = STORE_CALLEE[i];
	ee_oracle::rec::Hand { entry: e, end: e + STORE_CALLEE_LEN, hash: STORE_CALLEE_HASH, est: 0, f: after_init_store }
}
/// FUN_0029bb00, the save loader (writes the save table's blocks into RAM, the Options block 0x1a7c18..0x1a7c40 among them; card
/// loads FUN_00299898 and the level/default load FUN_002996c0 call it) [C boot ELF, resident]: entry, end, FNV hash.
pub const LOADER: (u32, u32, u64) = (0x29bb00, 0x29bd78, 0x1290_03c4_08e8_4ff6);
/// FUN_0029b930, the save writer (copies the save table's blocks out of RAM: the card image FUN_0029b738 and the in-RAM image
/// 0x1a2c28) [C boot ELF, resident]: entry, end, FNV hash.
pub const SAVER: (u32, u32, u64) = (0x29b930, 0x29ba48, 0xacda_88b1_8f10_b3ce);

static HOOKS: [ee_oracle::rec::Hand; 21] = [
	ee_oracle::rec::Hand { entry: BOOT_INIT.0, end: BOOT_INIT.1, hash: BOOT_INIT.2, est: 0, f: boot_init },
	ee_oracle::rec::Hand { entry: LOADER.0, end: LOADER.1, hash: LOADER.2, est: 0, f: load },
	ee_oracle::rec::Hand { entry: SAVER.0, end: SAVER.1, hash: SAVER.2, est: 0, f: save },
	store_hook(0), store_hook(1), store_hook(2), store_hook(3), store_hook(4), store_hook(5), store_hook(6), store_hook(7),
	store_hook(8), store_hook(9), store_hook(10), store_hook(11), store_hook(12), store_hook(13), store_hook(14), store_hook(15),
	store_hook(16), store_hook(17),
];

/// True when the row put the game in 60 Hz at this boot (for the whole run).
static IN_60: AtomicBool = AtomicBool::new(false);

/// Registers the hooks (before the recompiled table is built). NOT CALLED while the row is hidden; each Hand is inert while Off.
pub fn register() {
	ee_oracle::rec::register_hand(&HOOKS);
}

/// `RAC2_VIDEO60=1|0` (only read once `register` is called again; the row and its key are gone while hidden).
fn wanted() -> bool {
	static ENV: OnceLock<Option<bool>> = OnceLock::new();
	ENV.get_or_init(|| std::env::var("RAC2_VIDEO60").ok().map(|v| matches!(v.trim(), "1" | "on")))
		.unwrap_or(false)
}

/// The PAL flag the boot gives the game for a row value: None = leave the disc's 1 (retail), Some(0) = the NTSC console's value.
pub fn boot_flag(want: bool) -> Option<u32> {
	want.then_some(0)
}

/// Entry of the boot init: with the row On the flag is 0 before any of the init's readers. False: the init runs as retail's.
fn boot_init(c: &mut Cpu) -> bool {
	crate::scan_timing::timed("hook_boot_init", || boot_init_in(c))
}

fn boot_init_in(c: &mut Cpu) -> bool {
	let Some(flag) = boot_flag(wanted()) else { return false };
	c.mem.wr32(crate::addrs::PAL_FLAG, flag);
	IN_60.store(true, Ordering::Relaxed);
	eprintln!("rac2: 60 Hz mode row On: the game boots in its NTSC (60 Hz) mode");
	false
}

/// Entry of FUN_00299550 (or its overlay copy), i.e. right after the game init's store of 1 into the PAL flag: while the row's 60 Hz
/// is in force the flag goes back to 0 before the rest of the init reads it. False: the function runs as retail's.
fn after_init_store(c: &mut Cpu) -> bool {
	crate::scan_timing::timed("hook_after_init_store", || after_init_store_in(c))
}

fn after_init_store_in(c: &mut Cpu) -> bool {
	if IN_60.load(Ordering::Relaxed) && c.mem.rd32(crate::addrs::PAL_FLAG) != 0 {
		c.mem.wr32(crate::addrs::PAL_FLAG, 0);
		eprintln!("rac2: 60 Hz mode row: the game init's PAL flag store undone at {:#x}", c.pc);
	}
	false
}

/// FUN_0029bb00 (the save loader) while the row's 60 Hz is in force: runs the retail loader, then a PAL flag it loaded from the save
/// goes back to 0 before any reader runs. True = handled (v0 = the loader's result). Otherwise false: the loader runs as retail's.
fn load(c: &mut Cpu) -> bool {
	crate::scan_timing::timed("hook_load", || load_in(c))
}

fn load_in(c: &mut Cpu) -> bool {
	static BUSY: AtomicBool = AtomicBool::new(false);
	if !IN_60.load(Ordering::Relaxed) || BUSY.swap(true, Ordering::Relaxed) {
		return false;
	}
	c.call(LOADER.0);
	BUSY.store(false, Ordering::Relaxed);
	if c.mem.rd32(crate::addrs::PAL_FLAG) != 0 {
		c.mem.wr32(crate::addrs::PAL_FLAG, 0);
		eprintln!("rac2: 60 Hz mode row: a loaded save's PAL flag set to 60 Hz again");
	}
	true
}

/// FUN_0029b930 (the save writer) while the row's 60 Hz is in force: the retail writer runs with the PAL flag 1 (the disc's value), so
/// a card save stays a retail PAL save (a real PAL PS2 or PCSX2 loads it at 50 Hz), then the flag goes back to 0. True = handled
/// (v0 = the writer's result). Otherwise false: the writer runs as retail's.
fn save(c: &mut Cpu) -> bool {
	crate::scan_timing::timed("hook_save", || save_in(c))
}

fn save_in(c: &mut Cpu) -> bool {
	static BUSY: AtomicBool = AtomicBool::new(false);
	if !IN_60.load(Ordering::Relaxed) || BUSY.swap(true, Ordering::Relaxed) {
		return false;
	}
	c.mem.wr32(crate::addrs::PAL_FLAG, 1);
	c.call(SAVER.0);
	c.mem.wr32(crate::addrs::PAL_FLAG, 0);
	BUSY.store(false, Ordering::Relaxed);
	true
}

/// Game thread, every field: while the row's 60 Hz is in force, a PAL flag written back by the game (a loaded save's Options
/// block) is set to 0 again. Inert while 50 Hz is in force.
pub fn guard(m: &mut ee_oracle::Machine) {
	if IN_60.load(Ordering::Relaxed) && m.rd32(crate::addrs::PAL_FLAG) != 0 {
		m.cpu.mem.wr32(crate::addrs::PAL_FLAG, 0);
		eprintln!("rac2: 60 Hz mode row: the game's PAL flag came back (a loaded save), set to 60 Hz again");
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn boot_flag_only_with_the_row() {
		assert_eq!(boot_flag(false), None, "Off = retail, nothing written");
		assert_eq!(boot_flag(true), Some(0));
	}

	/// The hooked functions in the disc's boot ELF (the dev PC's copy of the extracted ELF): the hashes the hand binds
	/// with, the flag's data value (1 = PAL), the init's store and the reset's PAL-flag read.
	#[test]
	fn hashes_match_the_boot_elf() {
		let Ok(elf) = std::fs::read("D:/ratchet/rac2/extracted/SCES_516.07") else { return };
		let ph = u32::from_le_bytes(elf[0x1c..0x20].try_into().unwrap()) as usize;
		let n = u16::from_le_bytes(elf[0x2c..0x2e].try_into().unwrap()) as usize;
		let word = |a: u32| -> u32 {
			for i in 0..n {
				let s = |k: usize| u32::from_le_bytes(elf[ph + 32 * i + 4 * k..ph + 32 * i + 4 * k + 4].try_into().unwrap());
				let (off, va, sz) = (s(1), s(3), s(4));
				if a >= va && a < va + sz {
					let o = (off + a - va) as usize;
					return u32::from_le_bytes(elf[o..o + 4].try_into().unwrap());
				}
			}
			panic!("{a:#x} not in the ELF");
		};
		let fnv = |lo: u32, hi: u32| (lo..hi).step_by(4).fold(0xcbf2_9ce4_8422_2325u64, |h, a| (h ^ word(a) as u64).wrapping_mul(0x100_0000_01b3));
		assert_eq!(fnv(BOOT_INIT.0, BOOT_INIT.1), BOOT_INIT.2);
		assert_eq!(word(0x291510), 0x0c0a_1230, "jal FUN_002848c0: the init's video reset comes after the hashed words");
		assert_eq!(fnv(LOADER.0, LOADER.1), LOADER.2);
		assert_eq!(fnv(SAVER.0, SAVER.1), SAVER.2);
		assert_eq!(fnv(STORE_CALLEE[0], STORE_CALLEE[0] + STORE_CALLEE_LEN), STORE_CALLEE_HASH);
		assert_eq!((word(0x291638), word(0x29163c)), (0x0c00_0000 | (STORE_CALLEE[0] >> 2), 0xaf82_8ba8), "jal FUN_00299550; sw v0,-0x7458(gp)");
		assert_eq!(word(crate::addrs::PAL_FLAG), 1, "the PAL disc's flag");
		assert_eq!(word(0x2848d4), 0x8c63_7c18, "lw v1, 0x7c18(v1): the reset reads the PAL flag");
	}
}
