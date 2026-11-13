//! [b51 G2] A retail class whose vertices moved but whose faces, texture keys and vertex count did not (a pack whose classes the game
//! rewrites in RAM every few fields; each rewrite made `regrow_props` rebuild every slot, 4-35 ms per rewrite). Only the
//! vertices of that class's slots are written again (GPU buffer, CPU copy, radius); the indices, draws, runs and clippable
//! faces stay, because they depend on faces and keys only. Only reachable from a pack that calls it.

use super::props_append::new_slots;
use super::*;

impl Gpu {
	/// Writes the vertices of `prop_meshes[c]` into every prop slot of each class `c` in `changed`. The caller guarantees the
	/// new mesh has the same faces (with keys), strip phases and vertex count as the one the slots were built from. False
	/// (scene untouched for the class that failed; earlier classes are written) when there is no scene, a class is the
	/// purchase prompt or one of the pack's fallback classes, has no mesh, or its vertex count differs from its slots'; the
	/// caller then regrows, which rewrites everything.
	pub fn reshape_props(&mut self, changed: &[u32], prop_meshes: &HashMap<u32, Mesh>) -> bool {
		let (queue, Some(scene)) = (&self.queue, self.scene.as_mut()) else { return false };
		let _s = super::profile::span("ap_reshape");
		let pk = crate::pack::get();
		let (first, end) = pk.mesh_fallback;
		let n = scene.props_class.len();
		if scene.props_ranges.len() != n || scene.props_rad.len() != n {
			return false;
		}
		let vsz = std::mem::size_of::<Vertex>();
		for &c in changed {
			if c == pk.prompt_class || (first..end).contains(&c) {
				return false;
			}
			let Some(mesh) = prop_meshes.get(&c) else { return false };
			let slots: Vec<usize> = (0..n).filter(|&s| scene.props_class[s] == c).collect();
			if slots.is_empty() {
				continue;
			}
			let built = new_slots(&[mesh], 0);
			if slots.iter().any(|&s| scene.props_ranges[s].len() != built.verts.len()) {
				return false;
			}
			for &s in &slots {
				let r = scene.props_ranges[s].clone();
				queue.write_buffer(&scene.props.vbuf, (r.start * vsz) as u64, bytemuck::cast_slice(&built.verts));
				scene.props_model[r].copy_from_slice(&built.verts);
				scene.props_rad[s] = built.rad[0];
			}
		}
		true
	}
}
