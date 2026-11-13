//! One R&C2 level's world for the engine (engine split stage E5): the level file decoded by rac-formats' R&C2 layer (terrain lit
//! by the level lights, placed ties, shrubs, sky, the level's moby classes and Ratchet in armor 0), the level lights and fog as the
//! engine's `Lighting`, and the class keys the level file has (`scene::KNOWN`). Loaded on a worker thread; the host hands it to
//! `Gpu::load_scene`. From lane rc2's native/world.rs.

use crate::props::Classes;
use crate::tiecol::TieCols;
use rac_formats::gameplay::{DirLight, Lighting};
use rac_formats::level::{texture_key, Level, Texture, TextureGroup};
use rac_formats::mesh::Mesh;
use rac_formats::rc2::armor::{armor, ARMOR_TEXTURE_BASE, ARMOR_WAD, HERO_CLASS};
use rac_formats::shrubs::ShrubInstance;
use rac_formats::sky::Sky;
use rac_formats::ties::LitTie;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub struct World {
	pub level: usize,
	pub terrain: Vec<Mesh>,
	/// Which terrain chunk `terrain` is (`chunks`; 0 = the core's own, the only one of most levels).
	pub chunk: usize,
	pub shrubs: Vec<ShrubInstance>,
	pub ties: Vec<LitTie>,
	/// The game's tie colours by tie record (`tiecol`).
	pub tie_cols: TieCols,
	pub textures: HashMap<u32, Texture>,
	pub sky: Option<Sky>,
	/// The level's moby classes and their textures (the prop pool of this level starts from them).
	pub classes: Classes,
	pub lighting: Lighting,
	/// The texture packs' replacements of the level's textures (`texpack`), started by `Stage::load` on the render thread.
	pub repl: Option<rac_engine::texpack::prepared::Prepared>,
	/// [PC] The level's collision (veg.rs: the Vegetation row's plant placement), None when it does not decode.
	pub coll: Option<Arc<rac_formats::physics::World>>,
}

impl World {
	/// Level `id`; `want` = the terrain chunk the game has resident (`chunks::ram_sig`), None or unknown = the first.
	pub fn load(iso: &str, id: usize, want: Option<crate::chunks::Sig>) -> Result<World, String> {
		let (l, blocks, colls) = rac_formats::rc2::level::load_level_chunk_parts(iso, id)?;
		crate::glyph_cells::note_level(&l); // [PC] Button glyphs: this level's font sheets are recognisable (glyph_cells.rs)
		let lights = rac_formats::rc2::gameplay::lights(&l.gameplay).unwrap_or_default();
		let sigs: Vec<crate::chunks::Sig> = blocks.iter().map_while(|b| rac_formats::rc2::level::chunk_sig(b)).collect();
		let sigs = if sigs.len() == blocks.len() { sigs } else { Vec::new() };
		let chunk = crate::chunks::pick(&sigs, want);
		// Chunk 0 is the core's own head: the level's terrain as before; another chunk is decoded from its own block.
		let first = || rac_formats::rc2::level::terrain_meshes(&l);
		let (terrain, chunk) = match chunk {
			0 => (first()?, 0),
			k => match rac_formats::rc2::level::terrain_meshes_of(&l, &blocks[k]) {
				Ok(t) => (t, k),
				Err(e) => {
					eprintln!("rac2: level {id}: terrain chunk {k} does not decode: {e}; using chunk 0");
					(first()?, 0)
				}
			},
		};
		crate::chunks::register(id, sigs);
		let mut textures = l.all_textures()?;
		// Texture packs (texpack.rs): named from the level file's own textures, before the armor's join them.
		let repl = crate::texpack::prepare(&l, &textures);
		// Ties: one per tie record (gameplay order: the game's tie colour index), neutral colours until the game's own arrive.
		let placed = rac_formats::rc2::ties::lit_placements(&l, &lights)?;
		let mut maps: HashMap<u32, Arc<Vec<(Vec<u32>, u32)>>> = HashMap::new();
		for e in l.table(0x20, 32)? {
			let (start, cid) = (u32::from_le_bytes(e[0..4].try_into().unwrap()) as usize, u32::from_le_bytes(e[4..8].try_into().unwrap()));
			if let Ok(x) = l.block(start).and_then(|d| rac_formats::ties::tie_colour_maps(l.game, d, &e[16..32])) {
				maps.insert(cid, Arc::new(x));
			}
		}
		let classes_of = rac_formats::rc2::gameplay::tie_placements(&l.gameplay)?.into_iter().map(|p| p.0).collect::<Vec<u32>>();
		let (ties, tie_cols) = TieCols::new(placed, &classes_of, &maps);
		let shrubs = rac_formats::shrubs::lit_instances(&l, &lights).unwrap_or_else(|e| {
			eprintln!("rac2: shrubs: {e}");
			Vec::new()
		});
		let mut sky = rac_formats::sky::sky(&l).unwrap_or_else(|e| {
			eprintln!("rac2: sky: {e}");
			None
		});
		// Texture packs: the sky shells' replacements (PCSX2 names of the sky block's textures; not the title level).
		sky = sky.map(|s| rac_engine::texpack::extra_hd::hd_sky(&l, s));
		// The picture's clear colour (the engine clears to the sky's colour): the level's background colour.
		if let (Some(s), Ok(bg)) = (sky.as_mut(), rac_formats::rc2::gameplay::background(&l.gameplay)) {
			eprintln!("rac2: level {id}: background {bg:?} (sky header {:?})", s.colour);
			s.colour = [bg[0], bg[1], bg[2], 255];
		}
		let classes = level_classes(iso, &l, &mut textures)?;
		let coll = crate::veg::collision(&l, colls.get(chunk).filter(|_| chunk > 0).map(|b| &b[..]));
		eprintln!("rac2: level {id}: terrain chunk {chunk}: {} terrain meshes, {} ties, {} shrubs, {} moby classes", terrain.len(), ties.len(), shrubs.len(), classes.decoded.len());
		Ok(World { level: id, terrain, chunk, shrubs, ties, tie_cols, textures, sky, classes, lighting: lighting(&l, lights), repl, coll })
	}
}

/// The level's moby classes (key: the class's offset in the level's core = its RAM header address minus the core base) and their
/// textures. Ratchet (HERO_CLASS) is the level's class with armor 0's packets and textures: the worn armor is read from RAM (the
/// game loads it at run time) and replaces it once its textures arrive. Records the level's other keys in `scene::KNOWN`.
fn level_classes(iso: &str, l: &Level, textures: &mut HashMap<u32, Texture>) -> Result<Classes, String> {
	// A new level: the TEX1 K codes and run-time mip chains of the previous level's GS textures are void (props.rs `ram_chain`).
	rac_formats::mobies::reset_k_table();
	rac_engine::viewer::gpu::register_runtime_chain(u32::MAX, None);
	let mut classes = Classes::default();
	let (mut keys, mut bad) = (HashSet::new(), 0);
	let mut file_tex = crate::scene::LevelTex::default(); // [b54]
	for e in l.table(0x18, 32)? {
		let (off, cid) = (u32::from_le_bytes(e[0..4].try_into().unwrap()), u32::from_le_bytes(e[4..8].try_into().unwrap()));
		if off == 0 || classes.decoded.contains_key(&crate::props::class_id(off)) {
			continue;
		}
		if cid == HERO_CLASS {
			crate::fur::note_hero(l.id, crate::props::class_id(off)); // [PC] fur (fur.rs)
		}
		let c = if cid == HERO_CLASS {
			rac_formats::rc2::level::read_file(iso, ARMOR_WAD).and_then(|wad| {
				let (b, remap, tex) = armor(&wad, 0, l.block(off as usize)?)?;
				for (i, t) in tex.into_iter().enumerate() {
					textures.insert(texture_key(TextureGroup::Moby, ARMOR_TEXTURE_BASE as u32 + i as u32), t);
				}
				rac_formats::mobies::moby_class_for(l.game, &b, &remap, cid)
			})
		} else {
			keys.insert(off);
			class_tex_log(cid, &e[0x10..0x20], textures);
			l.block(off as usize).and_then(|b| rac_formats::mobies::moby_class_for(l.game, b, &e[0x10..0x20], cid))
		};
		match c {
			Ok(c) => {
				if classtex_on(cid) {
					eprintln!("classtex {cid:#x}: glow {:#x} colour_start {} second_last {} last {} verts {} joints {}", c.glow, c.colour_start, c.second_last_start, c.last_start, c.mesh.positions.len(), c.joints);
					let mut keys = std::collections::BTreeMap::new();
					for f in &c.mesh.faces {
						*keys.entry(f.0).or_insert(0u32) += 1;
					}
					eprintln!("classtex {cid:#x}: face keys {:x?}", keys);
					let pink: Vec<usize> = c.mesh.colours.iter().enumerate().filter(|(_, k)| k[1] < 1.0 / 128.0 && k[0] > 0.3 && k[2] > 0.3).map(|(i, _)| i).collect();
					eprintln!("classtex {cid:#x}: {} colours, {} magenta (first {:?} last {:?}) sample {:?}", c.mesh.colours.len(), pink.len(), pink.first(), pink.last(), c.mesh.colours.iter().step_by(40).take(12).collect::<Vec<_>>());
				}
				// [b54] The class's level-file texture slots, for its records and parts the game thread decodes from RAM (scene.rs `LevelTex`).
				if cid != HERO_CLASS && !RUN_TIME_TEXTURED.contains(&cid) {
					if let Some(slots) = l.block(off as usize).ok().and_then(|b| file_slots(b, &e[0x10..0x20], textures)) {
						file_tex.slots.extend(slots.into_iter().map(|(at, i)| (off + at, i)));
						file_tex.remap.insert(off, e[0x10..0x20].try_into().unwrap());
					}
				}
				classes.add_level_class(off, c)
			}
			Err(err) => {
				bad += 1;
				if cid == HERO_CLASS || std::env::var_os("RAC2_LOG").is_some() {
					eprintln!("rac2: moby class {cid}: {err}");
				}
			}
		}
	}
	if bad > 0 {
		eprintln!("rac2: {bad} moby classes not decoded");
	}
	// Every class of the level table but Ratchet's, decoded or not, is the level file's: only the others are read from RAM.
	eprintln!("rac2: level {}: {} of {} level classes have every texture slot in the level file ({} slots)", l.id, file_tex.remap.len(), keys.len(), file_tex.slots.len());
	if let Ok(mut k) = crate::scene::LEVEL_TEX.lock() {
		k.retain(|e| e.0 != l.id);
		k.push((l.id, Arc::new(file_tex)));
	}
	if let Ok(mut k) = crate::scene::KNOWN.lock() {
		k.retain(|e| e.0 != l.id);
		k.push((l.id, Arc::new(keys)));
	}
	// [C picture, batch 24] Untextured faces (AD slot -1, and R&C2's unmapped slots `moby_flat_unmapped`) carry the FLAT_SLOT key, which no
	// level texture has: without it the engine drew them with no texture bound (magenta, G = 0: the 0052 slot machine frames,
	// class 0xa9c). Same texel as the RAM classes' WHITE_KEY (255 = the GS modulate's 1.0).
	insert_flat_texture(textures);
	classes.tex = textures.iter().filter(|(k, _)| (*k & rac_formats::mobies::KEY_MASK) >> 16 == 1).map(|(k, t)| (*k, Texture { w: t.w, h: t.h, rgba: t.rgba.clone() })).collect();
	Ok(classes)
}

/// [b54] Level classes whose textures the game replaces at run time: Ratchet's helmet (0x50a) and boot pieces (0xa73) have
/// level-file entries whose texture slots name the wrong level textures (scene.rs `moby_at`); they keep their TEX0 words.
const RUN_TIME_TEXTURED: [u32; 2] = [0x50a, 0xa73];

/// [b54] The textured slots of the level class block `b` (near packets and parts) as (offset in the block, class-local index),
/// when every one of them names a level moby texture through `remap` that `textures` has; None otherwise (such a class keeps
/// reading its TEX0 words: an unmapped slot is one the game textures at run time).
fn file_slots(b: &[u8], remap: &[u8], textures: &HashMap<u32, Texture>) -> Option<Vec<(u32, u8)>> {
	let slots = rac_formats::rc2::mobies::file_slots(b).ok()?;
	let ok = |i: u32| remap.get(i as usize).is_some_and(|&r| r != 0xff && textures.contains_key(&texture_key(TextureGroup::Moby, r as u32)));
	slots.iter().all(|&(_, i)| i < 16 && ok(i)).then(|| slots.into_iter().map(|(at, i)| (at, i as u8)).collect())
}

/// The texture of the level classes' untextured faces (`FLAT_SLOT` key): one white texel, like the RAM classes' `WHITE_KEY`.
fn insert_flat_texture(textures: &mut HashMap<u32, Texture>) {
	textures.entry(texture_key(TextureGroup::Moby, rac_formats::mobies::FLAT_SLOT)).or_insert_with(|| Texture { w: 1, h: 1, rgba: vec![[255; 4]] });
}

fn classtex_on(cid: u32) -> bool {
	std::env::var("RAC2_CLASSTEX").is_ok_and(|v| v.split(',').any(|x| u32::from_str_radix(x.trim().trim_start_matches("0x"), 16).ok() == Some(cid)))
}

/// Debug: RAC2_CLASSTEX=0xCLASS,..: a level class's texture remap bytes and each named moby texture's size and mean colour.
fn class_tex_log(cid: u32, remap: &[u8], textures: &HashMap<u32, Texture>) {
	if !classtex_on(cid) {
		return;
	}
	for (i, &r) in remap.iter().enumerate().filter(|(_, r)| **r != 0xff) {
		let t = textures.get(&texture_key(TextureGroup::Moby, r as u32));
		let mean = t.map(|t| {
			let n = t.rgba.len().max(1) as u64;
			let s = t.rgba.iter().fold([0u64; 4], |a, p| std::array::from_fn(|k| a[k] + p[k] as u64));
			s.map(|x| x / n)
		});
		eprintln!("classtex {cid:#x}: slot {i} -> moby texture {r} {:?} mean {mean:?}", t.map(|t| (t.w, t.h)));
	}
}

/// The level lights and fog as the engine's `Lighting` (no light zones or point lights: R&C2's are not decoded [?]).
fn lighting(l: &Level, lights: Vec<DirLight>) -> Lighting {
	let (colour, fog) = rac_formats::rc2::gameplay::fog(&l.gameplay).unwrap_or(([0.0; 3], [0.0, 1e9, 255.0, 255.0]));
	Lighting { fog_colour: colour.map(|c| c as u8), fog, lights, zones: Vec::new(), points: Vec::new(), point_grid: Vec::new() }
}

#[cfg(test)]
mod flat_tests {
	use super::*;

	#[test]
	fn untextured_moby_faces_get_a_white_texture() {
		let mut t = HashMap::new();
		insert_flat_texture(&mut t);
		let k = texture_key(TextureGroup::Moby, rac_formats::mobies::FLAT_SLOT);
		assert_eq!(k, 0x1ffff);
		assert_eq!(t[&k].rgba, vec![[255u8; 4]]);
	}
}
