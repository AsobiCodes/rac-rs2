//! [PC] Vegetation over a terrain drawn as plain meshes (no terrain fragments; R&C2, rac2/veg.rs): the same planner, gates and
//! draw as `Gpu::plan_veg`, with each terrain mesh standing in for a fragment (its baked vertex colours as the lit colours). The
//! plan is made on a worker thread and arrives through `veg_follow`, so a level or terrain-chunk change does not stall the
//! render thread; a Moderate <-> Lush replan reads the kept meshes again (`owned`). A game with terrain fragments (R&C1) never
//! calls `plan_veg_plain`, so `PLAIN` stays empty there and `owned` is `TfOwned::of`.

use super::*;
use std::sync::{Arc, Mutex};

/// The plain terrain of the last `plan_veg_plain` (the scene id it belongs to, the meshes).
static PLAIN: Mutex<Option<((usize, u32), Arc<Vec<Mesh>>)>> = Mutex::new(None);

impl TfOwned {
	/// Plain terrain meshes as the planner's fragments: one per mesh, every vertex its own lit-colour record (`Mesh::colours`, PS2
	/// bytes as the fragments' lit colours; an unlit mesh falls back to white, as a fragment without lit colours).
	fn plain(meshes: &[Mesh], id: (usize, u32)) -> TfOwned {
		TfOwned {
			near: meshes.to_vec(),
			vert_pos: meshes.iter().map(|m| (0..m.positions.len() as u32).collect()).collect(),
			cols: meshes.iter().map(|m| m.colours.clone()).collect(),
			vranges: meshes.iter().map(|_| [0..0, 0..0, 0..0]).collect(),
			verts: Vec::new(),
			id,
		}
	}
}

/// What a replan (`veg_follow`) reads: the kept plain terrain while the scene on screen has no fragments and is the one it was
/// kept for, else the scene's fragments.
pub(super) fn owned(scene: &Scene) -> TfOwned {
	let id = (scene.tfrag_set.len(), scene.radius.to_bits());
	if scene.tfrag_set.is_empty() {
		if let Some((pid, m)) = PLAIN.lock().unwrap().as_ref() {
			if *pid == id {
				return TfOwned::plain(m, id);
			}
		}
	}
	TfOwned::of(scene)
}

impl Gpu {
	/// [PC] `plan_veg` for a terrain of plain meshes (`terrain`: the meshes `load_scene` got; the scene must have no fragments).
	/// No terrain, shrub or tie mip chains: the level's texture table stands in (as the planner does where a chain is missing).
	/// `prop_meshes`: the moby class models (their footprints, `VegFx::actors`). The host calls `veg_follow` every frame.
	pub fn plan_veg_plain(&mut self, world: &World, terrain: Arc<Vec<Mesh>>, shrub_set: Vec<shrubs::ShrubInstance>, textures: HashMap<u32, Texture>, tie_set: Vec<ties::LitTie>, prop_meshes: &HashMap<u32, Mesh>, level: usize) {
		self.veg.plan = None;
		self.veg.job = None;
		self.veg.src = None;
		self.veg.trail.clear();
		*PLAIN.lock().unwrap() = None;
		let c = cfg();
		if c.env == Some(0) || (c.env.is_none() && !((crate::pack::get().running)() && (crate::pack::get().pc_ext)())) {
			return;
		}
		let on = c.env.unwrap_or_else(crate::pack::get().veg_for_load).max(1);
		let Some(scene) = self.scene.as_ref() else { return };
		if terrain.is_empty() || !scene.tfrag_set.is_empty() {
			return;
		}
		let id = (0, scene.radius.to_bits());
		let foot: HashMap<u32, f32> = prop_meshes.iter().filter(|(_, x)| !x.faces.is_empty()).map(|(&k, x)| (k, x.positions.iter().map(|v| v[0].hypot(v[1])).fold(0.0, f32::max))).collect();
		*PLAIN.lock().unwrap() = Some((id, terrain.clone()));
		let src = Arc::new(VegSrc { shrub_set, textures, terrain_chains: Vec::new(), shrub_chains: Vec::new(), tie_set, tie_chains: Vec::new(), foot, n_mobies: 0, level });
		self.veg.src = Some(src.clone());
		let world = world.clone();
		let (tx, rx) = std::sync::mpsc::channel();
		let started = crate::viewer::bgpool::spawn("veg plan", 16 << 20, move || {
			// Background work: below-normal priority, so a 4-core PC keeps its game and render threads first.
			crate::hot::idle();
			let owned = TfOwned::plain(&terrain, id);
			let _ = tx.send(plan_cpu(&owned.view(), &world, &src.as_in(), on));
		});
		if started.is_ok() {
			self.veg.job = Some((rx, on));
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_plain_mesh_is_one_fragment_with_its_own_colours() {
		let mut m = Mesh::new("t".into());
		(m.positions, m.colours, m.faces, m.uvs) = (vec![[0.0; 3]; 3], vec![[64.0, 128.0, 32.0, 128.0]; 3], vec![(5, [0, 1, 2])], vec![[0.0; 2]; 3]);
		let o = TfOwned::plain(&[m], (0, 7));
		let v = o.view();
		assert_eq!(v.set.len(), 1);
		assert_eq!(v.set[0].vert_pos, &[0, 1, 2]);
		assert_eq!(v.cols[0][1], [64.0, 128.0, 32.0, 128.0]);
		assert_eq!(v.vranges.len(), 1);
		assert_eq!(v.id, (0, 7));
	}
}
