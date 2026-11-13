//! R&C2 (PAL SCES_516.07) RAM addresses of the render state the pack reads each field: the render camera, the core data base,
//! the moby table and the moby shadow-volume pass (from lane rc2's world/rc2.rs; the disc layout facts are in rac-formats'
//! R&C2 layer). The level overlay's data moves per planet, so `Addrs::find` locates it by its shape in RAM.

use rac_formats::bytes::{f32_at, u16_at, u32_at};

/// World-to-view basis [C, Aranos fields 2499 / 2540]: three rows (world x, y, z) whose columns are the view right, down and
/// forward axes, then the eye x 1024 with w = 1024 (the 16-byte rows the moby draw multiplies with).
pub const CAM_BASIS: u32 = 0x1bd580;

/// Projection block: f32 x-scale at +0, y-scale at +0x14; screen pixels = 2048 + scale x 32 x view / depth (the VU1 upload
/// at VU address 0 carries the same numbers times 1/0.012365 [C]).
pub const CAM_PROJ: u32 = 0x1bd3c0;

/// The draw's pixels per projection unit (word 0x1bd3a0 = 32.0 [C]).
pub const CAM_PROJ_UNIT: f32 = 32.0;

/// The far word (the far plane in 1/1024 m) sits this far below the projection block, after the unit word: 1048576.0 on most levels,
/// 3072000.0 on Aranos and level 14, 4194304.0 on some [C: fg1 0000..0576 every 12th, q60903]; it sets the GS Z offset (engine
/// `layer_tris::z_offset_of`).
pub const PROJ_FAR_BELOW: u32 = 0x1c;

/// Half the drawn screen in pixels: 256, 224 [C].
pub const CAM_HALF: u32 = 0x1bd500;

/// The game's Options row "16:9 (widescreen)", byte, 1 = on [C]: set at the boot from the console's TV shape (FUN_002914e0),
/// flipped by the row (FUN_0033e160: then FUN_0027af28 recomputes the projection, vertical FOV x 0.75), kept in saves (the
/// Options block 0x1a7c18..0x1a7c40 is entry 0x26 of the save table at 0x256138).
pub const WIDE: u32 = 0x1a7c39;

/// The progressive-scan word [C rac2/notes_menus.md item 5]: 1 = 480p progressive (SetGsCrt omode 0x50), an NTSC-only mode: the
/// video reset FUN_002848c0 clears it while `PAL_FLAG` is set. The boot clears it (FUN_002914e0); only the hidden Video dialog
/// (type 0xb) toggles it.
pub const VIDEO_60: u32 = 0x1a7c40;

/// The PAL flag [C rac2/notes_menus.md item 5 "PAL FLAG"]: 1 = 50 Hz PAL (the PAL disc's data value and boot write), 0 = the 60 Hz
/// NTSC mode of the same code (video reset, frame buffers, frame-time scales, the level overlays' timing sets). Read by the PC
/// "60 Hz mode" row (video60.rs).
pub const PAL_FLAG: u32 = 0x1a7c18;

/// The language word [C rac2/notes_gamefacts.md]: 0 English, 2 French, 3 German, 4 Spanish, 5 Italian (R&C1's numbering).
pub const LANG: u32 = 0x1a7c3c;

/// The Options > Subtitles byte [C notes_menus.md item 7]: movie subtitles shown while nonzero (texts starting with '*' always).
pub const SUBS_ON: u32 = 0x1a7c1e;

/// The movie's subtitle script pointer (0 = none) and the screen height word the subtitle box hangs from [C notes_menus.md item 7].
pub const SUBS_SCRIPT: u32 = 0x1b888c;
pub const SCREEN_H: u32 = 0x1a73c4;

/// The game's Options words for the camera sticks [C: boot options block, the camera options page FUN_0033e460 toggles them; the follow
/// camera's stick reader (Aranos 0x42a488) reads both with `lw`]: Up/Down at 0x1a7c2c (0 = Y is +stick, else -stick), Left/Right at
/// 0x1a7c30 (0 = X is -stick, else +stick). R&C1's OPT_UD / OPT_LR twins: the same logic, `== 0` is "Reversed" in modcam's terms.
pub const OPT_UD: u32 = 0x1a7c2c;
pub const OPT_LR: u32 = 0x1a7c30;

/// RAM word holding the base address of the level's core data (class blocks: class table offset + base) [C: 0x850000 on
/// Aranos, fields 2499 / 2540; the word beside it holds the same].
pub const CORE_BASE: u32 = 0x1bf198;

/// RAM words: moby table start, end (exclusive) [C: Aranos 0x1964480 / 0x1981880].
pub const MOBY_TABLE: u32 = 0x1b1e9c;

/// Moby record size and fields (R&C1's layout where they agree): +0x10 position, +0x20 slot byte (0xff ends the walk,
/// >= 0xfd unused), +0x24 class header address, +0x2c scale (class header +0x24 x size factor), +0xc0 / 0xd0 / 0xe0
/// rotation rows (model x, y, z axes in world space). [C: Aranos hero 0x1964480]
pub const MOBY_SIZE: u32 = 0x100;

/// The RAM addresses of one level's render state. The level overlay's data is linked at different addresses per planet
/// [C: Aranos camera basis 0x1bd580, level 1 0x1bdc80 (+0x700); moby table 0x1b1e9c / 0x1b255c (+0x6c0)], so they are
/// found by their shape in RAM once the level runs (`Addrs::find`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Addrs {
	/// Camera basis block (CAM_BASIS in Aranos).
	pub basis: u32,
	/// Projection block (basis - 0x1c0 [C: Aranos, level 1]).
	pub proj: u32,
	/// Half screen (basis - 0x80 [C]).
	pub half: u32,
	/// Word holding the core data base (basis + 0x1c18 [C]).
	pub core: u32,
	/// Moby table start word; the end word follows.
	pub moby: u32,
	/// The moby shadow-volume records, when the level overlay has the shadow pass (`ShadowAddrs::find`).
	pub shadow: Option<ShadowAddrs>,
}

/// The moby shadow-volume pass (R&C1's, decomp/notes/shadows.md "BRIDGE-64a"/"BRIDGE-64b"; Aranos: gate fn 0x3138d8, record
/// build 0x35bae0, volume driver 0x35b908 [C]). The caster list (8-byte entries {moby, fade u16, straddle u16}) sits at
/// records - 0x180; the build writes one record per caster each drawn frame, a zero prim count ends them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowAddrs {
	/// The first record (Aranos 0x1fb6a0); the build stops at records + 0x1f80 [C: `addi s3, s5, 0x1f80`].
	pub records: u32,
	/// Word that gates the pass: 0 = no build and no volumes this frame (Aranos 0x1b1ebc) [C].
	pub gate: u32,
}

impl ShadowAddrs {
	/// The record build is found by its `addi s3, s5, 0x1f80` after `lui/addiu s5` (the records) and `lui/addiu s6` (the
	/// light slots); its caller tests the gate word (`lui v0; lw v0, off(v0)`, `bnez v0` within four words) before it calls
	/// the build and the driver [C: Aranos 0x3138dc..0x31397c].
	pub fn find(ram: &[u8]) -> Option<ShadowAddrs> {
		let w = |a: u32| u32_at(ram, a as usize).ok().unwrap_or(0);
		let imm = |hi: u32, lo: u32| (hi << 16).wrapping_add(lo as u16 as i16 as i32 as u32);
		let (lo, hi) = (0x2a0000u32, (ram.len() as u32).min(0x480000));
		let at = (lo..hi).step_by(4).find(|&a| w(a) == 0x22b3_1f80 && w(a - 0x10) >> 16 == 0x3c15 && w(a - 0xc) >> 16 == 0x26b5)?;
		let records = imm(w(at - 0x10) & 0xffff, w(at - 0xc) & 0xffff);
		// The call of the build: a jal to within 0x80 bytes before the signature (Aranos entry 0x35bae0 = at - 0x40).
		let call = (lo..hi).step_by(4).find(|&a| {
			let x = w(a);
			let t = (x & 0x03ff_ffff) << 2;
			x >> 26 == 3 && t < at && t + 0x80 >= at
		})?;
		let gate = (1..64).map(|k| call - 4 * k).find(|&a| w(a) >> 16 == 0x3c02 && w(a + 4) >> 16 == 0x8c42 && (2..6).any(|j| w(a + 4 * j) >> 16 == 0x1440))?;
		Some(ShadowAddrs { records, gate: imm(w(gate) & 0xffff, w(gate + 4) & 0xffff) })
	}
}

impl Addrs {
	/// Aranos' fixed addresses (the constants above).
	pub fn aranos() -> Addrs {
		Addrs { basis: CAM_BASIS, proj: CAM_PROJ, half: CAM_HALF, core: CORE_BASE, moby: MOBY_TABLE, shadow: Some(ShadowAddrs { records: 0x1fb6a0, gate: 0x1b1ebc }) }
	}

	/// Looks for the render state in a RAM image: a 16-aligned block in the overlay's data area whose +0x3c word is 1024.0,
	/// with three unit rows before it, a projection scale 3..60 at basis - 0x1c0 and the half screen 256 x 224 at
	/// basis - 0x80 (the last candidate wins: stale copies sit lower); the moby table is the (start, end, spawned end) word
	/// triple before it with the most live records.
	pub fn find(ram: &[u8]) -> Option<Addrs> {
		let f = |a: u32| f32_at(ram, a as usize).ok();
		let w = |a: u32| u32_at(ram, a as usize).ok();
		let mut found = None;
		let mut p = 0x1a0000u32;
		while p < 0x200000 && (p as usize) + 0x40 <= ram.len() {
			if w(p + 0x3c) == Some(0x4480_0000) && f(p - 0x80) == Some(256.0) && f(p - 0x7c) == Some(224.0) {
				let unit = (0..3).all(|r| {
					let v: Option<Vec<f32>> = (0..3).map(|k| f(p + r * 16 + k * 4)).collect();
					v.is_some_and(|v| (v.iter().map(|x| x * x).sum::<f32>().sqrt() - 1.0).abs() < 0.02)
				});
				let scale = f(p - 0x1c0).is_some_and(|s| (3.0..60.0).contains(&s));
				if unit && scale {
					found = Some(p);
				}
			}
			p += 16;
		}
		let basis = found?;
		let core = basis + 0x1c18;
		let base = w(core)? & 0x01ff_ffff;
		// The table with the most live records (slot < 0xfd, class header above the core base) wins; a table whose first record (the hero)
		// is live beats any whose is not [C: Siberius fg1 0310/0313/0322: 0x1b11b0 = {0x1a00000, 0x1a80000, 0x1b40000} has a zeroed first
		// record and counts ~900 "live" ones on a misaligned walk over the real records; the real table 0x1b22dc (hero 490.0, 227.9, 217.7,
		// the retail eye 489.9, 232.6, 219.7) counts 723. The old pick put the Modern camera at the world origin: a white, fully fogged screen].
		let live_rec = |r: u32| ram[r as usize + 0x20] < 0xfd && w(r + 0x24).is_some_and(|h| (h & 0x01ff_ffff) > base && (h & 0x01ff_ffff) < 0x0200_0000);
		let mut best: Option<(bool, u32, u32)> = None;
		let mut o = basis - 0xd000;
		while o < basis - 0x8000 {
			if let (Some(s), Some(e), Some(e2)) = (w(o), w(o + 4), w(o + 8)) {
				let (s, e, e2) = (s & 0x01ff_ffff, e & 0x01ff_ffff, e2 & 0x01ff_ffff);
				// The first record is Ratchet (class id +0xaa = 0); +8 (end of the run-time spawned records) is at or after +4 [C: level 2's table 0x1b361c holds 0x49300 bytes of
				// level mobies, more than Aranos' 0x40000 bound; the shifted pair 0x1b3620 has +8 below its start].
				if s % 16 == 0 && e > s && e2 >= e && (e - s) % 0x100 == 0 && (0x100..=0x100000).contains(&(e - s)) && e2 - s <= 0x200000 && (e as usize) < ram.len() && ram[s as usize + 0x20] == 0 && u16_at(ram, s as usize + 0xaa).ok() == Some(0) {
					// Live records over the level's own and the spawned ones [C: level 24's table 0x1b179c holds 5 own records and
					// 0x18e00 bytes of spawned ones; level 25 (space combat) 0x2900 bytes of own].
					let live = (s..e2)
						.step_by(0x100)
						.take_while(|r| ram[*r as usize + 0x20] != 0xff)
						.filter(|r| live_rec(*r))
						.count() as u32;
					let first = live_rec(s);
					if live >= 8 && best.is_none_or(|b| (first, live) > (b.0, b.1)) {
						best = Some((first, live, o));
					}
				}
			}
			o += 4;
		}
		let moby = best?.2;
		Some(Addrs { basis, proj: basis - 0x1c0, half: basis - 0x80, core, moby, shadow: ShadowAddrs::find(ram) })
	}
}

/// The GS area of the game's frozen-screen backdrop (engine `gsfull::grab`): the title and pause menus show a picture the EE grabs from
/// the GS into RAM (FUN_002cac90) and uploads back every field (FUN_002fd9e8: 56 tiles of 64 x 64 PSMCT24 = psm 1, 0x40 blocks apart) into the draw
/// buffer FBP 0xf0 (DAT_001a735c 0x1e0000 >> 8 -> block 0x1e00); a sprite at the next field's start copies it into the shown buffer
/// [C: rac2 shots, title field 500, pause field 6800]. The grab is never made here, so the uploads are junk: the engine drops them.
pub const GRAB: rac_engine::gsfull::grab::Grab = rac_engine::gsfull::grab::Grab { base: 0x1e00, blocks: 0xe00, psm: 1 };

#[cfg(test)]
mod tests {
	use super::*;

	fn put(ram: &mut [u8], a: u32, v: u32) {
		ram[a as usize..a as usize + 4].copy_from_slice(&v.to_le_bytes());
	}

	/// Siberius' shape [C: fg1 0310]: a table with a zeroed first record and more "live" records on its walk loses to the table whose
	/// first record is the hero.
	#[test]
	fn a_table_with_a_live_hero_beats_a_bigger_one_without() {
		let mut ram = vec![0u8; 0x0200_0000];
		let basis = 0x1bd9c0u32;
		put(&mut ram, basis + 0x3c, 1024.0f32.to_bits());
		put(&mut ram, basis - 0x80, 256.0f32.to_bits());
		put(&mut ram, basis - 0x7c, 224.0f32.to_bits());
		put(&mut ram, basis - 0x1c0, 10.0f32.to_bits());
		for r in 0..3 {
			put(&mut ram, basis + r * 16 + r * 4, 1.0f32.to_bits());
		}
		put(&mut ram, basis + 0x1c18, 0x80c000);
		// The zeroed-first table: 20 live records after an empty first one.
		let (a, s) = (0x1b11b0u32, 0x1a00000u32);
		put(&mut ram, a, s);
		put(&mut ram, a + 4, s + 0x80000);
		put(&mut ram, a + 8, s + 0x140000);
		for i in 1..=20 {
			put(&mut ram, s + i * 0x100 + 0x24, 0x900000);
		}
		// The real one: 10 live records from the hero on.
		let (b, t) = (0x1b22dcu32, 0x1a9fa40u32);
		put(&mut ram, b, t);
		put(&mut ram, b + 4, t + 0x1000);
		put(&mut ram, b + 8, t + 0x1000);
		for i in 0..10 {
			put(&mut ram, t + i * 0x100 + 0x24, 0x900000);
		}
		ram[(t + 10 * 0x100 + 0x20) as usize] = 0xff;
		let got = Addrs::find(&ram).expect("the render state");
		assert_eq!((got.basis, got.moby), (basis, b));
	}
}
