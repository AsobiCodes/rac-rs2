//! [PC] Vegetation for R&C2 (PARITY row 17, R&C1's Graphics row: Off / Moderate / Lush with Lighting Enhanced): the engine's plant
//! planner and draw (engine viewer/vegfx.rs, plain-mesh terrain `vegfx/plain.rs`), placed with the level's own collision and bent by
//! Ratchet, his trail and the mobies the game draws this field.
//! Collision [C, rac2/notes_collision.md T5-2b]: R&C2's core index +0x14 points at the same wrapper block as R&C1's (tree offset
//! 0x40 in every level), so rac-formats' R&C1 reader decodes it unchanged (27 levels, 0 bad cells). The tree is the core's own
//! (terrain chunk 0); every streamed chunk carries its own as the second WAD stream of its block (`rc2::level::chunk_collisions`).

use rac_engine::viewer::Gpu;
use rac_formats::level::{Level, Texture};
use rac_formats::mesh::Mesh;
use rac_formats::physics::World as Coll;
use rac_formats::shrubs::ShrubInstance;
use rac_formats::ties::LitTie;
use std::collections::HashMap;
use std::sync::Arc;

/// The level's collision as the engine's ray-test world (cells of 4 m, each face visible only through its own cell, as R&C1's).
/// `chunk`: the resident terrain chunk's own collision block (`rc2::level::chunk_collisions`; None = chunk 0, the core's).
pub fn collision(l: &Level, chunk: Option<&[u8]>) -> Option<Arc<Coll>> {
	let faces = match chunk {
		Some(b) => rac_formats::collision::collision_block(b),
		None => rac_formats::collision::collision(l),
	};
	match faces {
		Ok(faces) if !faces.is_empty() => {
			eprintln!("rac2: collision: {} triangles", faces.len());
			Some(Arc::new(Coll::homed(faces.into_iter().map(|f| (f.v, f.kind, Some(f.cell))), 4.0)))
		}
		Ok(_) => None,
		Err(e) => {
			eprintln!("rac2: collision: {e}");
			None
		}
	}
}

/// After `load_scene` (frame.rs `Stage::load`): the plan of this world's plants on a worker thread (none without collision).
/// `textures` already carry a texture pack's grass colours (`extra_grass::apply`, on the scene build thread: world_build.rs `build`).
pub fn plan(g: &mut Gpu, coll: Option<&Coll>, terrain: Vec<Mesh>, shrubs: Vec<ShrubInstance>, textures: HashMap<u32, Texture>, ties: Vec<LitTie>, class_meshes: &HashMap<u32, Mesh>, level: usize) {
	match coll {
		Some(c) => g.plan_veg_plain(c, Arc::new(terrain), shrubs, textures, ties, class_meshes, level),
		None => g.plan_veg_plain(&Coll::new(std::iter::empty(), 4.0), Arc::new(Vec::new()), Vec::new(), HashMap::new(), Vec::new(), &HashMap::new(), level),
	}
}

/// Every field drawn (frame.rs `Stage::field`): Ratchet's position (the first moby record, `Scene::hero_group`) and the mobies the
/// game draws this field (engine class id, position, scale) bend the plants; a ready plan or a row change is taken up.
pub fn field(g: &mut Gpu, coll: Option<&Coll>, scene: &crate::scene::Scene) {
	let hero = scene.hero_group.first().map(|h| [h[0], h[1], h[2]]);
	g.veg.actors(hero, scene.mobies.iter().filter(|m| m.cut.is_none()).map(|m| (crate::props::class_id(m.key), m.pos, m.scale)));
	g.veg_follow(coll);
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn no_collision_plans_nothing_and_keeps_no_terrain() {
		// Without a level the engine has no scene: the plan call returns at once (no thread, no panic).
		let c = Coll::new(std::iter::empty(), 4.0);
		assert!(c.line([0.0, 0.0, 1.0], [0.0, 0.0, -1.0], rac_formats::physics::qmask::TWO_SIDED).is_none());
	}

	/// Level 4 (hoverboard race): chunk 1's own collision [C, lane gfxb tools/chunkcoll2.py] = the tree the game has at `res+0` in
	/// state fg1/0200 (cells x 146..206, z 75..94: the tunnel), not the core's (chunk 0, x 18..106); chunk 0's block = the core's.
	#[test]
	fn chunk_1_collision_is_its_own_tree() {
		let p = std::env::var("RAC2_ISO").unwrap_or_else(|_| "E:/Ratchet & Clank 2 (Europe, Australia) (En,Fr,De,Es,It)/Ratchet & Clank 2 (Europe, Australia) (En,Fr,De,Es,It).iso".into());
		if !std::path::Path::new(&p).is_file() {
			return;
		}
		let (l, _, colls) = rac_formats::rc2::level::load_level_chunk_parts(&p, 4).unwrap();
		assert_eq!(colls.len(), 2);
		let core = rac_formats::collision::collision(&l).unwrap();
		assert_eq!(rac_formats::collision::collision_block(&colls[0]).unwrap().len(), core.len(), "chunk 0 = the core's");
		let f = rac_formats::collision::collision_block(&colls[1]).unwrap();
		assert!(!f.is_empty());
		assert!(f.iter().all(|f| (146..=206).contains(&f.cell[0]) && (75..=94).contains(&f.cell[2])), "chunk 1 cells");
		assert!(core.iter().all(|f| f.cell[0] <= 106));
	}
}
