//! One R&C2 level from the disc image (G/LEVELn.WAD), in the shape of the shared `Level` (from lane rc2's `rc2/world/level.rs`).
//!
//! LEVELn.WAD [C, Aranos LEVEL0]: header 0x60 bytes {size, level, 0, 3 [?], then from +0x10 (sector, sectors) pairs}: primary,
//! sound bank, gameplay WAD, occlusion map. Primary: 0x80-byte header of (offset, size) pairs: overlay code, asset header (= R&C1
//! core index: same tables at the same offsets), GS RAM, HUD header, five HUD banks, assets (WAD, = R&C1 core data; index +0x8c its
//! decompressed size), and a last WAD block. The fourth pair is the occlusion map (`occlusion::OccMap::from_block`) [C: its
//! tie count equals the number of tie placements on levels 0 and 1].

use crate::bytes::{span, u32_at, R};
use crate::gamelayout::Game;
use crate::gameplay::DirLight;
use crate::level::{decoded, Level, SECTION_NAMES};
use crate::mesh::Mesh;

pub use super::iso9660::read_file;

/// Level `id` from the disc image at `iso` (G/LEVEL<id>.WAD).
pub fn load_level(iso: &str, id: usize) -> R<Level> {
	Ok(read_level(iso, id)?.0)
}

/// Level `id` and its terrain chunks (`chunk_blocks`): chunk 0 first (= the head of the level's core data), none for a level
/// with one terrain block.
pub fn load_level_chunks(iso: &str, id: usize) -> R<(Level, Vec<Vec<u8>>)> {
	load_level_chunk_parts(iso, id).map(|(l, t, _)| (l, t))
}

/// `load_level_chunks` plus each chunk's collision block (`chunk_collisions`, same order).
pub fn load_level_chunk_parts(iso: &str, id: usize) -> R<(Level, Vec<Vec<u8>>, Vec<Vec<u8>>)> {
	let (l, raw) = read_level(iso, id)?;
	let chunks = chunk_blocks(&raw).unwrap_or_else(|e| {
		eprintln!("rc2: level {id}: terrain chunk table: {e}; the level keeps its first chunk");
		Vec::new()
	});
	let colls = if chunks.is_empty() { Vec::new() } else { chunk_collisions(&raw) };
	Ok((l, chunks, colls))
}

/// The collision block of each terrain chunk [C, lane gfxb 2026-11-11, gfxb tools/chunkcoll2.py: all 19 chunks of levels 1 2 4 7
/// 8 11 19 20 decode with 0 bad cells; level 4 chunk 1's tree is byte for byte the tree at `res+0` in RAM state fg1/0200]: the
/// chunk's 0x10-byte prefix is {terrain stream offset 0x10, collision stream offset, 0, 0} (block-relative), and the second WAD
/// stream decompresses to the R&C1 wrapper {tree offset 0x40, hero-groups offset, ..} + tree (`collision::collision_block`).
/// Chunk 0's equals the core's own. A chunk without one gives an empty block (its plants then find no ground).
pub fn chunk_collisions(raw: &[u8]) -> Vec<Vec<u8>> {
	let mut out = Vec::new();
	for k in 4..7 {
		let (Ok(s), Ok(n)) = (u32_at(raw, 0x10 + k * 8), u32_at(raw, 0x14 + k * 8)) else { break };
		if n == 0 {
			continue;
		}
		let (s, n) = (s as usize * 0x800, n as usize * 0x800);
		let one = || -> R<Vec<u8>> {
			let sec = span(raw, s, n.min(raw.len().saturating_sub(s)))?;
			let c = u32_at(sec, 4)? as usize;
			decoded(span(sec, c, sec.len().saturating_sub(c))?)
		};
		out.push(one().unwrap_or_default());
	}
	out
}

/// The terrain chunks of a LEVELn.WAD [C: levels 1 2 4 7 8 11 19 20, 2026-11-03]. Header pairs 4..6 (the (sector, sectors) pairs
/// at +0x30 / +0x38 / +0x40) hold up to three chunks, each a 0x10-byte prefix {0x10, collision stream offset, 0, 0} (corrected 2026-11-11: was {0x10, size, 0, 0}; `chunk_collisions`) and a WAD stream of one terrain
/// block image: {0x40, fragment count, 12.0, 0x7cb} at +0, the fragment headers (0x40 bytes each) from +0x40, then the rest of
/// the block (collision, vertex data), 0x2b9290 bytes on level 4. Chunk 0 is byte for byte the head of the core data. The game
/// keeps ONE chunk resident at the core base (the word at `addrs.core`) and streams the next one over it when Ratchet crosses
/// into its area [C: RAM of level 4 holds chunk 1 at its core base while the core's head is chunk 0; levels 1, 2, 7, 8, 11
/// likewise at their own core bases].
///
/// A chunk that does not decode is logged and kept as an EMPTY block (so the others keep their numbers; `chunk_sig` of it is None, which
/// turns chunk streaming off for the level): the level still loads with the core's own terrain.
pub fn chunk_blocks(raw: &[u8]) -> R<Vec<Vec<u8>>> {
	let mut out = Vec::new();
	for k in 4..7 {
		let (s, n) = (u32_at(raw, 0x10 + k * 8)? as usize, u32_at(raw, 0x14 + k * 8)? as usize);
		if n == 0 {
			continue;
		}
		let one = || -> R<Vec<u8>> {
			let sec = span(raw, s * 0x800, (n * 0x800).min(raw.len().saturating_sub(s * 0x800)))?;
			decoded(span(sec, 0x10, sec.len().saturating_sub(0x10))?)
		};
		out.push(one().unwrap_or_else(|e| {
			eprintln!("rc2: terrain chunk {} (header pair {k}) does not decode: {e}; the level keeps its first chunk", out.len());
			Vec::new()
		}));
	}
	Ok(out)
}

/// What tells a resident terrain block from the others: its fragment count and its first fragment's sphere (16 bytes at +0x40),
/// which the game leaves as the file has them [C: RAM of level 4 against both chunks]. None when the block has no such header.
pub fn chunk_sig(block: &[u8]) -> Option<[u8; 20]> {
	let mut sig = [0u8; 20];
	sig[..4].copy_from_slice(block.get(4..8)?);
	sig[4..].copy_from_slice(block.get(0x40..0x50)?);
	(block.get(..4)? == 0x40u32.to_le_bytes()).then_some(sig)
}

fn read_level(iso: &str, id: usize) -> R<(Level, Vec<u8>)> {
	let raw = read_file(iso, &format!("G/LEVEL{id}.WAD"))?;
	let sec = |k: usize| -> R<&[u8]> {
		let (s, n) = (u32_at(&raw, 0x10 + k * 8)? as usize, u32_at(&raw, 0x14 + k * 8)? as usize);
		// The last block may end inside its last sector: the file is not padded to whole sectors [C: LEVEL10.WAD gameplay
		// sectors 0x1a63 + 0x11f end 0x1d bytes past the file's 0xdc0fe3].
		span(&raw, s * 0x800, (n * 0x800).min(raw.len().saturating_sub(s * 0x800)))
	};
	// Header pairs from +0x10 [C]: (sector, sectors) of the primary, the sound bank, gameplay and the occlusion map.
	let (primary, sound, gameplay) = (sec(0)?, sec(1)?, sec(2)?);
	let part = |k: usize| -> R<&[u8]> { span(primary, u32_at(primary, k * 8)? as usize, u32_at(primary, k * 8 + 4)? as usize) };
	let mut sections: Vec<Option<Vec<u8>>> = vec![None; SECTION_NAMES.len()];
	sections[0] = Some(part(0)?.to_vec());
	sections[1] = Some(sound.to_vec());
	let index = part(1)?.to_vec();
	sections[3] = Some(part(2)?.to_vec());
	for k in 0..6 {
		sections[4 + k] = Some(part(3 + k)?.to_vec());
	}
	let core = decoded(part(9)?)?;
	if core.len() != u32_at(&index, 0x8c)? as usize {
		return Err("core data size differs from its index".into());
	}
	let gameplay = decoded(gameplay)?;
	let occ_block = sec(3).ok().map(|b| b.to_vec());
	Ok((Level::from_parts(Game::Rc2, id, sections, index, core, gameplay, occ_block)?, raw))
}

/// The level's terrain as plain meshes (one per fragment, LOD 0) with the level's directional lights baked into the vertex
/// colours, as lane rc2's world drew it: `terrain_lit(.., Some(lights))`, and the unlit `terrain` when that fails. The R&C2 pack
/// calls this for the terrain; `terrain::terrain_lods` is the shared fragment-LOD form (no baked lights without `lights`).
pub fn terrain_meshes(level: &Level) -> R<Vec<Mesh>> {
	terrain_meshes_of(level, level.terrain_block()?)
}

/// The same for another terrain block of the level (a chunk, `chunk_blocks`).
pub fn terrain_meshes_of(level: &Level, block: &[u8]) -> R<Vec<Mesh>> {
	let lights: Vec<DirLight> = super::gameplay::lights(&level.gameplay).unwrap_or_default();
	crate::terrain::terrain_lit(block, 0, Some(&lights)).or_else(|_| crate::terrain::terrain(block, 0))
}
