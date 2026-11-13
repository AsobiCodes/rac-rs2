//! [b46 U4] R&C2's ship flight between planets (game mode 6): the star field and milky way behind the ship.
//!
//! Facts [C: bank 0013, RAC_GSFULL_QDUMP of field 100, parsed offline]: the game draws the cosmos itself over PATH2, as the first
//! qwords of the field, and no VU1 program draws it: (1) 16x16 + 256x256 PSMT8 texture uploads (CLUT 0x2d2b, texels 0x2d2f) and 16x16 +
//! 512x256 (0x2e2f / 0x2e33), (2) 195 Gouraud alpha-blended triangle vertices (the nebula tint), (3) 204 vertices textured with the
//! 256x256 sheet (the stars) and 60 with the 512x256 sheet (the milky way), all in screen space with the PS2's 2048-pixel offset;
//! then one PATH1 moby-program run (empty here: the native picture draws the ship) and the HUD. The usual overlay rule treats a PATH2
//! draw before the last PATH1 run as the world the native picture replaces and drops it; the native picture has nothing of the flight
//! (its scene read follows the departure level's records and finds junk), so the field was black. The engine now keeps these draws
//! (`OvlField::space_from`), drawn by the game's own packets: nothing is invented.
//! [b47 X5] The ship: the flight's own moby table (`addrs`, `flight_table`), so the ship's classes draw as the landed ship does.

use crate::addrs::{Addrs, ShadowAddrs};
use std::sync::atomic::{AtomicU32, Ordering};

/// Is the game in the warp flight (mode word 6, found by `pc_input::find_mode`'s rule)?
pub fn in_flight(ram: &[u8]) -> bool {
	mode(ram) == Some(6)
}

fn mode(ram: &[u8]) -> Option<u32> {
	crate::pc_input::find_mode(ram).and_then(|x| ram.get(x..x + 4)).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// [b47 R3] Is the flight's moby table the one to use: in the flight, and under a pause or a box opened in it (mode 3 / 4 with the
/// flight's table still picked), so the ship does not leave the picture behind the menu and nothing is found again twice per pause.
fn flying(ram: &[u8]) -> bool {
	match mode(ram) {
		Some(6) => true,
		Some(3 | 4) => TABLE.load(Ordering::Relaxed) != 0,
		_ => false,
	}
}

// [b47 X5] The ship in the flight. Facts [C: fg1 banks 0013, 0126, 0186, 0305 (mode 6) and 0043 (level 25, mode 6), offline]: the
// flight's render state sits where `Addrs::find` looks (camera basis 0x1bd580 / 0x1bed40 / 0x1be140 / 0x1be040 / 0x1bdc80, the eye on
// the fixed flight path), and its moby table is the usual {start, own end, spawned end} triple (0x1b1e9c = {0x954c40, 0x954d40, 0x958c40}
// in 0013): the first record is Ratchet PARKED (class 0, slot byte 0xfd = not live), then the ship's records (classes 0x10d4 x2,
// 0x10d7, 0x10da x2, 0x10de x2, 0x10e5 x2 and 0xd54, all at the ship's position, slot 0, drawn byte 1). `Addrs::find` wants a live
// Ratchet (slot 0) first, so it chose a junk table in 0013 (records at 0x80.. = "mobies 20 drawn 0 ... not decoded") and none at all in
// 0126 / 0186 / 0305 / 0043 (no view: the black whole-GS picture). The same ship classes and the 0xd54 helper sit in the levels' own
// tables next to the landed ship (bank 0014), drawn there by the usual RAM class path.

/// The moby table word picked for the flight by `addrs` (0 = none): `stale` finds the cached render state again when the flight
/// starts or ends (the level word does not change at the arrival: it names the target during the flight).
static TABLE: AtomicU32 = AtomicU32::new(0);

fn rd(ram: &[u8], a: u32) -> Option<u32> {
	ram.get(a as usize..a as usize + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn rdf(ram: &[u8], a: u32) -> Option<f32> {
	rd(ram, a).map(f32::from_bits)
}

/// The camera basis block by `Addrs::find`'s rule (+0x3c = 1024.0, half screen 256 x 224 at -0x80, three unit rows, projection scale
/// 3..60 at -0x1c0; the last candidate wins).
fn basis(ram: &[u8]) -> Option<u32> {
	let unit = |p: u32| (0..3).all(|r| (0..3).map(|k| rdf(ram, p + r * 16 + k * 4)).collect::<Option<Vec<f32>>>().is_some_and(|v| (v.iter().map(|x| x * x).sum::<f32>().sqrt() - 1.0).abs() < 0.02));
	(0x1a0000u32..0x200000).step_by(16).filter(|&p| p >= 0x1c0 && rd(ram, p + 0x3c) == Some(0x4480_0000) && rdf(ram, p - 0x80) == Some(256.0) && rdf(ram, p - 0x7c) == Some(224.0) && unit(p) && rdf(ram, p - 0x1c0).is_some_and(|s| (3.0..60.0).contains(&s))).last()
}

/// The flight's moby table word near `basis` (the window `Addrs::find` scans): the table shape of `Addrs::find`, a parked Ratchet
/// first (class 0, slot 0xfd, class header above the core base) and at least one live record; the one with the most live records.
pub fn flight_table(ram: &[u8], basis: u32) -> Option<u32> {
	let base = rd(ram, basis + 0x1c18)? & 0x01ff_ffff;
	let hdr_ok = |r: u32| rd(ram, r + 0x24).is_some_and(|h| (h & 0x01ff_ffff) > base && (h & 0x01ff_ffff) < 0x0200_0000);
	let slot = |r: u32| ram.get(r as usize + 0x20).copied();
	let mut best: Option<(u32, u32)> = None;
	for o in (basis.checked_sub(0xd000)?..basis - 0x8000).step_by(4) {
		let (Some(s), Some(e), Some(e2)) = (rd(ram, o), rd(ram, o + 4), rd(ram, o + 8)) else { continue };
		let (s, e, e2) = (s & 0x01ff_ffff, e & 0x01ff_ffff, e2 & 0x01ff_ffff);
		let shape = s % 16 == 0 && e > s && e2 >= e && (e - s) % 0x100 == 0 && (0x100..=0x100000).contains(&(e - s)) && e2 - s <= 0x200000 && (e2 as usize) <= ram.len();
		if !shape || slot(s) != Some(0xfd) || rd(ram, s + 0xa8).map(|x| x >> 16) != Some(0) || !hdr_ok(s) {
			continue;
		}
		let live = (s..e2).step_by(0x100).take_while(|&r| slot(r).is_some_and(|x| x != 0xff)).filter(|&r| slot(r).is_some_and(|x| x < 0xfd) && hdr_ok(r)).count() as u32;
		if live >= 1 && best.is_none_or(|b| live > b.1) {
			best = Some((o, live));
		}
	}
	best.map(|b| b.0)
}

/// The render state to use: `found` (`Addrs::find`) outside the flight; in the flight the same camera with the flight's moby table
/// (`flight_table`), built from the camera basis alone when `Addrs::find` found no table.
pub fn addrs(ram: &[u8], found: Option<Addrs>) -> Option<Addrs> {
	if !flying(ram) {
		TABLE.store(0, Ordering::Relaxed);
		return found;
	}
	let Some(b) = found.map(|a| a.basis).or_else(|| basis(ram)) else { return found };
	let Some(t) = flight_table(ram, b) else {
		TABLE.store(found.map_or(0, |a| a.moby), Ordering::Relaxed);
		return found;
	};
	TABLE.store(t, Ordering::Relaxed);
	eprintln!("rac2: flight moby table {t:x} (camera {b:x})");
	Some(found.map_or_else(|| Addrs { basis: b, proj: b - 0x1c0, half: b - 0x80, core: b + 0x1c18, moby: t, shadow: ShadowAddrs::find(ram) }, |a| Addrs { moby: t, ..a }))
}

/// Must the cached render state `a` be found again: the flight started (its table is not the flight's) or ended (it still is)?
pub fn stale(ram: &[u8], a: &Addrs) -> bool {
	let t = TABLE.load(Ordering::Relaxed);
	if flying(ram) {
		return t != a.moby;
	}
	if t != 0 {
		TABLE.store(0, Ordering::Relaxed);
		return t == a.moby;
	}
	false
}

#[cfg(test)]
mod tests {
	use super::*;

	fn put(ram: &mut [u8], a: usize, v: u32) {
		ram[a..a + 4].copy_from_slice(&v.to_le_bytes());
	}

	#[test]
	fn mode_six_is_the_flight_and_other_modes_are_not() {
		let mut ram = vec![0u8; 0x1b0000];
		// bank 0013's words: mode 6, previous mode 0, then -2 [C].
		put(&mut ram, 0x1a8f80, 6);
		put(&mut ram, 0x1a8f84, 0);
		put(&mut ram, 0x1a8f88, -2i32 as u32);
		assert!(in_flight(&ram));
		put(&mut ram, 0x1a8f80, 0);
		assert!(!in_flight(&ram));
		assert!(!in_flight(&[]));
	}

	/// Bank 0013's layout in small: camera basis 0x1bd580, the flight table 0x1b1e9c (parked Ratchet, one ship record) and a table
	/// with a live Ratchet first whose records are junk (the one `Addrs::find` picked).
	fn flight_ram() -> Vec<u8> {
		let mut ram = vec![0u8; 0x1c0000];
		let b = 0x1bd580usize;
		for (k, v) in [(0x3c, 1024.0f32), (0, 1.0), (0x14, 1.0), (0x28, 1.0)] {
			put(&mut ram, b + k, v.to_bits());
		}
		put(&mut ram, b - 0x80, 256f32.to_bits());
		put(&mut ram, b - 0x7c, 224f32.to_bits());
		put(&mut ram, b - 0x1c0, 10f32.to_bits());
		put(&mut ram, b + 0x1c18, 0x8000);
		let table = |ram: &mut Vec<u8>, o: usize, s: usize, hero_slot: u8| {
			put(ram, o, s as u32);
			put(ram, o + 4, s as u32 + 0x100);
			put(ram, o + 8, s as u32 + 0x300);
			ram[s + 0x20] = hero_slot;
			put(ram, s + 0x24, 0x9000);
			ram[s + 0x120] = 0;
			put(ram, s + 0x124, 0x9100);
			put(ram, s + 0x1a8, 0x10d4 << 16);
			ram[s + 0x220] = 0xff;
		};
		table(&mut ram, 0x1b1e9c, 0x40000, 0xfd);
		table(&mut ram, 0x1b0a8c, 0x50000, 0);
		ram
	}

	#[test]
	fn the_flight_table_is_the_one_with_ratchet_parked() {
		let mut ram = flight_ram();
		assert_eq!(basis(&ram), Some(0x1bd580));
		assert_eq!(flight_table(&ram, 0x1bd580), Some(0x1b1e9c));
		// Not in the flight: `Addrs::find`'s answer stays (None here); in the flight the table is the flight's, built from the camera.
		assert_eq!(addrs(&ram, None), None);
		put(&mut ram, 0x1a8f80, 6);
		put(&mut ram, 0x1a8f88, -2i32 as u32);
		let a = addrs(&ram, None).unwrap();
		assert_eq!((a.basis, a.moby, a.core, a.proj, a.half), (0x1bd580, 0x1b1e9c, 0x1bd580 + 0x1c18, 0x1bd580 - 0x1c0, 0x1bd580 - 0x80));
		assert!(!stale(&ram, &a));
		// The arrival: mode 0 with the flight's table cached = found again once.
		put(&mut ram, 0x1a8f80, 0);
		assert!(stale(&ram, &a));
		assert!(!stale(&ram, &a));
		// [b47 R3] A pause in the flight (mode 3, previous 6) keeps the flight's table; a pause on the ground has none.
		put(&mut ram, 0x1a8f80, 6);
		let a = addrs(&ram, None).unwrap();
		put(&mut ram, 0x1a8f80, 3);
		assert!(!stale(&ram, &a) && addrs(&ram, None).map(|x| x.moby) == Some(a.moby));
		put(&mut ram, 0x1a8f80, 0);
		assert!(stale(&ram, &a));
		put(&mut ram, 0x1a8f80, 3);
		assert!(!stale(&ram, &a) && addrs(&ram, None).is_none(), "a pause on the ground");
		// No parked Ratchet: no flight table.
		ram[0x40020] = 0;
		assert_eq!(flight_table(&ram, 0x1bd580), None);
	}
}
