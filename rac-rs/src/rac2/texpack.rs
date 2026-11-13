//! R&C2's PCSX2 texture packs (engine split stage E6, feature row 43): the level file's world textures (terrain, ties, shrubs and
//! the level's moby classes) get the replacements of the enabled packs in `<install>/rac2/mods` (the engine's `texpack`; the folder
//! rule is `Pack::mods`). R&C2 stores its level textures exactly as R&C1 does (the level's asset header has the same four
//! 16-byte tables at the same offsets: raw PSMT8 indices of every mip level plus a CT32 CSM1 CLUT, rac-formats `Level`), so the
//! PCSX2 names are the engine's: `source::level_sources` reads the tables, `hash` hashes the GS block stream of each mip level and
//! the CLUT. [C, 2026-10-27: the names a `--shots` run's GS drew (RAC_TEXNAMES, Aranos, 2700 fields) against the level tables'
//! (test `names_match_the_gs_names`, diag `diag_gs_names_vs_tables`): of the 132 table textures the GS used, 78 were drawn under
//! their own name; the others only at their smaller mips while other textures' data sat in their level-0 blocks (far draws; the
//! level-0 hash is then history-dependent and no PCSX2 pack can name it either), their mip names are the tables'. Most PSMT8 names
//! in a run are the frame buffer read as a texture (TBP 0, screen effects), not textures.] PAL/USA twin names
//! (`assets/texmap.tsv`) are R&C1's: not used here.
//! What is not covered: moby classes the game builds in RAM (weapons, armor, effects; their textures come from GS memory), the
//! water, the HUD and the effects (GS overlay pixels).

use rac_engine::texpack::prepared::{self, Prepared};
use rac_formats::level::{Level, Texture};
use std::collections::HashMap;

/// The replacements of `l`'s textures (`textures` = `l.all_textures()` before anything else changes it), on the world thread; None
/// with no textures mod active. `Stage::load` hands the result to the scene build on the render thread (`prepared::adopt`).
pub fn prepare(l: &Level, textures: &HashMap<u32, Texture>) -> Option<Prepared> {
	let (p, hits) = prepared::prepare(l, textures, &[], false)?;
	eprintln!("rac2: level {}: texture packs: {} of {} textures replaced", l.id, p.len(), hits.textures);
	rac_engine::texpack::extra_grass::remember_prepared(l.id as u32, &p); // [PC] the grass colours of the HD ground (veg.rs `plan`)
	Some(p)
}

#[cfg(test)]
mod tests {
	use super::*;
	use rac_engine::texpack::mods::MipNames;
	use rac_engine::texpack::{names, source};

	/// The R&C2 PAL disc: `RAC2_ISO`, else the dev PC's copy; None (the test returns) when there is none.
	fn iso() -> Option<String> {
		let p = std::env::var("RAC2_ISO").unwrap_or_else(|_| "E:/Ratchet & Clank 2 (Europe, Australia) (En,Fr,De,Es,It)/Ratchet & Clank 2 (Europe, Australia) (En,Fr,De,Es,It).iso".into());
		std::path::Path::new(&p).is_file().then_some(p)
	}

	/// Level 0 (Aranos) table names that the GS drew under the same name (a `--shots` run, RAC_TEXNAMES, 2026-10-27): terrain,
	/// moby and a texture both tables share.
	#[test]
	fn names_match_the_gs_names() {
		let Some(iso) = iso() else { return };
		let l = rac_formats::rc2::level::load_level(&iso, 0).unwrap();
		let (srcs, _) = source::level_sources(&l).unwrap();
		let all = names::level_names(&srcs, MipNames::Both);
		let at = |stem: &str| all.iter().filter(|n| n.name.stem(true) == stem).map(|n| (n.group as usize, n.index, n.level)).collect::<Vec<_>>();
		assert!(at("32fdc2d5bf96e981-2f5693a8dff28ac-00001dd3").contains(&(0x30, 0x1e, 0)));
		assert!(at("24d6c0373c376942-c994d60650062882-00002213").contains(&(0x38, 0x74, 0)));
		let shared = at("c97e601371b145cb-eea57b936a653cad-00001dd3");
		assert!(shared.contains(&(0x30, 65, 0)) && shared.contains(&(0x38, 94, 0)), "{shared:?}");
	}

	/// DEV: RAC2_EXTRA_OUT=<file> RAC2_EXTRA_LEVEL=<n>: writes the PCSX2 names of the level's sky shell textures (and the fx / particle / metal
	/// tables) as `kind	stem	w	h` (a synthetic test pack needs them: the sky is drawn by the scene, never by the GS capture).
	#[test]
	#[ignore = "reads the R&C2 disc (RAC2_ISO) and writes RAC2_EXTRA_OUT"]
	fn dev_extra_names() {
		let (Some(iso), Some(out)) = (iso(), std::env::var_os("RAC2_EXTRA_OUT")) else { return };
		let id = std::env::var("RAC2_EXTRA_LEVEL").ok().and_then(|v| v.parse().ok()).unwrap_or(0usize);
		let l = rac_formats::rc2::level::load_level(&iso, id).unwrap();
		let rows: Vec<String> = rac_engine::texpack::extra::extra_sources(&l).iter().map(|s| format!("{:?}	{}	{}	{}", s.kind, s.name().stem(true), s.w, s.h)).collect();
		std::fs::write(out, rows.join("
")).unwrap();
	}

	/// DIAG (prints only): RAC2_TEXNAMES_TSV=<file> from a `rac2-play --shots` run with RAC_TEXNAMES=<file>, RAC2_TEXNAMES_LEVEL=<n>.
	#[test]
	fn diag_gs_names_vs_tables() {
		let (Some(iso), Some(tsv)) = (iso(), std::env::var_os("RAC2_TEXNAMES_TSV")) else { return };
		let id = std::env::var("RAC2_TEXNAMES_LEVEL").ok().and_then(|v| v.parse().ok()).unwrap_or(0usize);
		let l = rac_formats::rc2::level::load_level(&iso, id).unwrap();
		let (srcs, skipped) = source::level_sources(&l).unwrap();
		let all = names::level_names(&srcs, MipNames::Both);
		let mut by_stem: HashMap<String, Vec<(usize, usize, u8, usize, usize)>> = HashMap::new();
		for n in &all {
			let t = srcs.iter().find(|t| t.group as usize == n.group as usize && t.index == n.index).unwrap();
			by_stem.entry(n.name.stem(true)).or_default().push((n.group as usize, n.index, n.level, t.w, t.h));
		}
		eprintln!("level {id}: {} textures ({skipped} unreadable), {} names ({} distinct)", srcs.len(), all.len(), by_stem.len());
		for g in [0x30usize, 0x38, 0x40, 0x48] {
			let mut dims: Vec<(usize, usize)> = srcs.iter().filter(|t| t.group as usize == g).map(|t| (t.w, t.h)).collect();
			dims.sort();
			dims.dedup();
			eprintln!("  group {g:#x}: {} textures, sizes {dims:?}", srcs.iter().filter(|t| t.group as usize == g).count());
		}
		if let Some(out) = std::env::var_os("RAC2_TABLE_NAMES_OUT") {
			let rows: Vec<String> = by_stem.iter().flat_map(|(k, v)| v.iter().map(move |e| format!("{k}\t{:#x}\t{}\t{}\t{}\t{}", e.0, e.1, e.2, e.3, e.4))).collect();
			std::fs::write(out, rows.join("\n")).unwrap();
		}
		let text = std::fs::read_to_string(tsv).unwrap();
		let (mut t8, mut hit, mut other) = (0, 0, 0);
		let mut miss = Vec::new();
		for line in text.lines().skip(1) {
			let f: Vec<&str> = line.split('\t').collect();
			if f[1] != "0x13" {
				other += 1;
				continue;
			}
			t8 += 1;
			if let Some(v) = by_stem.get(f[0]) {
				hit += 1;
				if hit <= 12 {
					eprintln!("  HIT {} {}x{} draws {} -> {:x?}", f[0], f[2], f[3], f[5], &v[..v.len().min(2)]);
				}
			} else {
				miss.push(format!("{} {}x{} region {} draws {} mxl {}", f[0], f[2], f[3], f[4], f[5], f[7]));
			}
		}
		eprintln!("GS names: {t8} PSMT8 ({hit} in the tables, {} not), {other} other formats", miss.len());
		// Draw-weighted, and which half of a missing name the tables know (texture hash, CLUT hash).
		let part = |s: &str, i: usize| s.split('-').nth(i).unwrap_or("").to_string();
		let texs: std::collections::HashSet<String> = by_stem.keys().map(|k| part(k, 0)).collect();
		let cluts: std::collections::HashSet<String> = by_stem.keys().map(|k| part(k, 1)).collect();
		let (mut d_hit, mut d_all) = (0u64, 0u64);
		let mut rows: Vec<(u64, String)> = Vec::new();
		for line in text.lines().skip(1) {
			let f: Vec<&str> = line.split('\t').collect();
			if f[1] != "0x13" {
				continue;
			}
			let d: u64 = f[5].parse().unwrap_or(0);
			d_all += d;
			if by_stem.contains_key(f[0]) {
				d_hit += d;
			} else {
				rows.push((d, format!("{} {}x{} region {} mxl {} tex {} clut {}", f[0], f[2], f[3], f[4], f[7], texs.contains(&part(f[0], 0)), cluts.contains(&part(f[0], 1)))));
			}
		}
		let (tex_only, clut_only) = (rows.iter().filter(|r| r.1.contains("tex true")).count(), rows.iter().filter(|r| r.1.contains("clut true")).count());
		eprintln!("draws: {d_hit} of {d_all} PSMT8 draws hit; missing names with a known texture hash {tex_only}, a known CLUT {clut_only}");
		rows.sort_by(|a, b| b.0.cmp(&a.0));
		for (d, m) in rows.iter().take(40) {
			eprintln!("  MISS draws {d} {m}");
		}
	}
}
