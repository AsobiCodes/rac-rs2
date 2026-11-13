//! Terrain chunks of the levels that have them (batch 14, #20): the native world draws the chunk the game has resident.
//!
//! [C, 2026-11-03] LEVELn.WAD of levels 1 2 4 7 8 11 19 20 holds up to three terrain chunks (header pairs 4..6,
//! `rac_formats::rc2::level::chunk_blocks`). The game keeps ONE of them at the core base (the RAM word at `addrs.core`, the
//! head of the core data) and streams the next over it when Ratchet crosses into its area; chunk 0 is the core's own head. The
//! hoverboard race (level 4, state 0200) is in chunk 1: the core file's terrain (860 fragments, a distant city) is not what the
//! game draws there, the tunnel (1135 fragments) is. The game thread reads which chunk is resident (`ram_sig`), tells the world
//! loader (`Req`), and the loader decodes that chunk's terrain (`world.rs`); the engine's scene is rebuilt like for a new level.

use rac_formats::rc2::level::chunk_sig;
use std::sync::Mutex;

/// A chunk's identity: its fragment count and its first fragment's sphere (`chunk_sig`).
pub type Sig = [u8; 20];

/// One job of the world loader: level `level`, and the chunk the game had resident when it asked (None = the first).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Req {
	pub level: u32,
	pub sig: Option<Sig>,
}

/// The chunk signatures of each level the loader has decoded (empty: a level with one terrain block).
static SIGS: Mutex<Vec<(usize, Vec<Sig>)>> = Mutex::new(Vec::new());

/// Record level `level`'s chunk signatures (`World::load`).
pub fn register(level: usize, sigs: Vec<Sig>) {
	if let Ok(mut s) = SIGS.lock() {
		s.retain(|e| e.0 != level);
		s.push((level, sigs));
	}
}

/// The chunk of `sigs` that `sig` names; the first when it names none (or is None).
pub fn pick(sigs: &[Sig], sig: Option<Sig>) -> usize {
	sig.and_then(|s| sigs.iter().position(|&c| c == s)).unwrap_or(0)
}

/// The signature of the terrain block resident at the core base `base` of the RAM `ram` (None: no such header there).
pub fn ram_sig(ram: &[u8], base: u32) -> Option<Sig> {
	let a = (base & 0x01ff_ffff) as usize;
	chunk_sig(ram.get(a..a + 0x50)?)
}

/// True when the game has another chunk of `level` resident (`ram`) than the one the loader was last asked for (`asked`): the
/// chunks are known (the level's world was decoded once), `ram` is one of them and not the asked one. A signature that is none
/// of the level's chunks (the old level's terrain while a new level loads, a half-streamed block) asks for nothing.
pub fn wanted(level: usize, asked: Option<Sig>, ram: Option<Sig>) -> bool {
	let Some(r) = ram else { return false };
	let Ok(s) = SIGS.lock() else { return false };
	let Some((_, sigs)) = s.iter().find(|e| e.0 == level) else { return false };
	sigs.contains(&r) && pick(sigs, Some(r)) != pick(sigs, asked)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn sig(n: u8) -> Sig {
		let mut s = [n; 20];
		s[0] = 7;
		s
	}

	#[test]
	fn pick_follows_the_signature_and_defaults_to_the_first() {
		let sigs = [sig(1), sig(2), sig(3)];
		assert_eq!(pick(&sigs, Some(sig(3))), 2);
		assert_eq!(pick(&sigs, Some(sig(9))), 0);
		assert_eq!(pick(&sigs, None), 0);
		assert_eq!(pick(&[], Some(sig(1))), 0);
	}

	#[test]
	fn wanted_only_for_a_known_other_chunk() {
		register(990, vec![sig(1), sig(2)]);
		assert!(wanted(990, None, Some(sig(2))), "chunk 1 resident, chunk 0 asked");
		assert!(wanted(990, Some(sig(1)), Some(sig(2))));
		assert!(!wanted(990, Some(sig(2)), Some(sig(2))), "already asked");
		assert!(!wanted(990, None, Some(sig(1))), "chunk 0 is the default");
		assert!(!wanted(990, None, Some(sig(9))), "not a chunk of this level");
		assert!(!wanted(990, None, None));
		assert!(!wanted(991, None, Some(sig(2))), "level not decoded yet");
		register(992, Vec::new());
		assert!(!wanted(992, None, Some(sig(2))), "a level without chunks");
	}

	#[test]
	fn ram_sig_reads_count_and_first_sphere_at_the_core_base() {
		let mut ram = vec![0u8; 0x2000];
		ram[0x1000..0x1004].copy_from_slice(&0x40u32.to_le_bytes());
		ram[0x1004..0x1008].copy_from_slice(&1135u32.to_le_bytes());
		ram[0x1040..0x1050].copy_from_slice(&[9; 16]);
		let s = ram_sig(&ram, 0x8000_1000).unwrap();
		assert_eq!(&s[..4], &1135u32.to_le_bytes());
		assert_eq!(&s[4..], &[9; 16]);
		assert_eq!(ram_sig(&ram, 0x800), None, "no 0x40 table offset word there");
		assert_eq!(ram_sig(&ram, 0x1ff0), None, "past the RAM");
	}

	/// The R&C2 PAL disc (`RAC2_ISO`, else the dev PC's copy); None (the test returns) when there is none.
	fn iso() -> Option<String> {
		let p = std::env::var("RAC2_ISO").unwrap_or_else(|_| "E:/Ratchet & Clank 2 (Europe, Australia) (En,Fr,De,Es,It)/Ratchet & Clank 2 (Europe, Australia) (En,Fr,De,Es,It).iso".into());
		std::path::Path::new(&p).is_file().then_some(p)
	}

	/// Level 4 (the hoverboard race) [C: 2026-11-03]: two chunks, chunk 0 is the core's head (860 fragments, the distant city) and
	/// chunk 1 the tunnel the game has resident in state 0200 (1135 fragments, count + sphere equal to its RAM header); levels
	/// 0 and 3 have none.
	#[test]
	fn level_4_has_two_chunks_and_chunk_0_is_the_core_head() {
		let Some(iso) = iso() else { return };
		let (l, blocks) = rac_formats::rc2::level::load_level_chunks(&iso, 4).unwrap();
		assert_eq!(blocks.len(), 2);
		let counts: Vec<u32> = blocks.iter().map(|b| u32::from_le_bytes(b[4..8].try_into().unwrap())).collect();
		assert_eq!(counts, [860, 1135]);
		assert_eq!(&l.terrain_block().unwrap()[..0x2b9290], &blocks[0][..0x2b9290], "chunk 0 = the core's head");
		let sigs: Vec<Sig> = blocks.iter().map(|b| chunk_sig(b).unwrap()).collect();
		assert_ne!(sigs[0], sigs[1]);
		assert_eq!(sigs[1][4..8], [0x1c, 0x59, 0x2b, 0x49], "the sphere the game's RAM block starts with");
		assert_eq!(pick(&sigs, Some(sigs[1])), 1);
		let t = rac_formats::rc2::level::terrain_meshes_of(&l, &blocks[1]).unwrap();
		assert!(t.len() > 800 && t.len() <= 1135, "{}", t.len());
		assert!(rac_formats::rc2::level::load_level_chunks(&iso, 0).unwrap().1.is_empty());
		assert!(rac_formats::rc2::level::load_level_chunks(&iso, 3).unwrap().1.is_empty());
	}
}
