//! [PC] perf (lane stut): a retail class re-pointed at other GS textures. The game rebuilds a class with the same geometry and
//! only other face texture keys (R&C2: a busy scene did it ~60 times in 2 minutes, and `regrow_props` rebuilt all ~300 slots
//! in ~26 ms each time). Nothing on the vertex side depends on the keys, so the vertices, ranges, radii and the slots'
//! attribute stay; only what the keys group (draws, indices, runs, clippable faces of the class's slots) is made again, the
//! very values `regrow_props` gives.

use super::clip::PropFace;
use super::props_append::{class_prop_faces, faces_at, meshes_known};
use super::*;

/// What a retexture changes in the props layout.
pub(super) struct Retex {
	pub(super) draws: Vec<Draw>,
	pub(super) slot_runs: Vec<Vec<(u32, u32, u32)>>,
	pub(super) indices: Vec<u32>,
	/// The clippable faces of every slot whose class is in `changed` (slot, faces).
	pub(super) faces: Vec<(usize, Vec<PropFace>)>,
}

/// The layout for the slot order `order` with the class faces `cf` (already holding the new keys of `changed`).
pub(super) fn retexture(cf: &HashMap<u32, ClassFaces>, tex_slots: &HashMap<u32, usize>, classes: &[u32], ranges: &[std::ops::Range<usize>], order: &[usize], changed: &[u32]) -> Retex {
	let (draws, slot_runs, indices) = prop_layout(cf, tex_slots, classes, ranges, order);
	let templates: HashMap<u32, Vec<PropFace>> = changed.iter().map(|&c| (c, class_prop_faces(&cf[&c], tex_slots))).collect();
	let faces = classes.iter().enumerate().filter_map(|(s, c)| templates.get(c).map(|t| (s, faces_at(ranges[s].start as u32, t)))).collect();
	Retex { draws, slot_runs, indices, faces }
}

impl Gpu {
	/// [PC] The prop slots of the classes in `changed` drawn with the new face texture keys of `prop_meshes` (same geometry as
	/// before: `same_geometry`) without a rebuild of every slot. The result is what `regrow_props` would build (the vertex
	/// layout aside). False (scene untouched) when there is no scene, a changed class has no mesh or class faces, is one of
	/// the pack's fallback classes or the purchase prompt, or a face uses a texture the scene never uploaded; the caller then
	/// regrows.
	pub fn retexture_props(&mut self, changed: &[u32], prop_meshes: &HashMap<u32, Mesh>) -> bool {
		let (queue, Some(scene)) = (&self.queue, self.scene.as_mut()) else { return false };
		let pk = crate::pack::get();
		let (first, end) = pk.mesh_fallback;
		let changed: Vec<u32> = changed.iter().copied().filter(|c| scene.props_class.contains(c)).collect();
		if changed.is_empty() {
			return true;
		}
		let n = scene.props_class.len();
		let ok = changed.iter().all(|c| *c != pk.prompt_class && !(first..end).contains(c) && prop_meshes.get(c).is_some_and(|m| m.positions.len() > 0)) && scene.props_order.len() == n && scene.props_ranges.len() == n && scene.prop_faces.len() == n && scene.props_class.iter().all(|c| scene.class_faces.contains_key(c) || changed.contains(c));
		if !ok {
			return false;
		}
		let meshes: Vec<&Mesh> = changed.iter().map(|c| &prop_meshes[c]).collect();
		if !meshes_known(&meshes, &scene.tex_slots) {
			return false;
		}
		let old: Vec<Option<ClassFaces>> = changed.iter().map(|c| scene.class_faces.remove(c)).collect();
		for (&c, m) in changed.iter().zip(&meshes) {
			scene.class_faces.insert(c, ClassFaces::of(m));
		}
		let r = retexture(&scene.class_faces, &scene.tex_slots, &scene.props_class, &scene.props_ranges, &scene.props_order, &changed);
		if r.indices.len() * 4 > scene.props.ibuf.size() as usize {
			for (&c, o) in changed.iter().zip(old) {
				match o {
					Some(o) => scene.class_faces.insert(c, o),
					None => scene.class_faces.remove(&c),
				};
			}
			return false;
		}
		queue.write_buffer(&scene.props.ibuf, 0, bytemuck::cast_slice(&r.indices));
		scene.props.draws = r.draws;
		scene.slot_runs = r.slot_runs;
		for (s, f) in r.faces {
			scene.prop_faces[s] = f;
		}
		// As after a rebuild: nothing is masked for the clipper, the per-frame state starts over.
		scene.props_inst = Vec::new();
		scene.props_pose = Vec::new();
		scene.props_shown = false;
		scene.props_alpha = false;
		scene.props_drawn = Vec::new();
		scene.props_vis = Vec::new();
		scene.props_late = Vec::new();
		scene.clip_masked = Vec::new();
		scene.clip_view = None;
		scene.clip_queue = Vec::new();
		true
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn rnd(s: &mut u64) -> u64 {
		*s ^= *s << 13;
		*s ^= *s >> 7;
		*s ^= *s << 17;
		*s
	}

	fn mesh(seed: &mut u64, keys: &[u32]) -> Mesh {
		let n = 6 + (rnd(seed) % 6) as usize;
		let nf = 4 + (rnd(seed) % 8) as usize;
		let faces: Vec<(u32, [u32; 3])> = (0..nf).map(|i| (keys[(rnd(seed) % keys.len() as u64) as usize], [(i % n) as u32, ((i + 1) % n) as u32, ((i + 3) % n) as u32])).collect();
		let strip_phase = (0..nf).map(|i| if i % 3 == 0 { crate::mesh::NO_PHASE } else { (i % 2) as u8 }).collect();
		Mesh { name: String::new(), positions: (0..n).map(|i| [i as f32, 0.5 * i as f32, 2.0]).collect(), uvs: (0..n).map(|i| [i as f32 * 0.1, 0.0]).collect(), faces, colours: Vec::new(), lit: None, strip_phase, normals: Vec::new() }
	}

	/// A class re-pointed at other texture keys: the incremental layout equals the whole rebuild's (`Gpu::batch` grouping +
	/// `clip::replay_faces` + `shadowvol::slot_runs`, i.e. `regrow_props`) on a synthetic pool, with the changed classes in
	/// several slots and the other classes untouched.
	#[test]
	fn a_retextured_class_lays_out_like_a_regrow() {
		let mut seed = 0x1234_5678_9abc_def1u64;
		let mut tex_slots: HashMap<u32, usize> = HashMap::new();
		for k in 0..16u32 {
			for cull in [0, 1 << 24, 2 << 24] {
				tex_slots.insert((k | cull) & crate::mobies::SLOT_MASK, k as usize);
			}
		}
		for _ in 0..200 {
			let keys_old = [1u32, 2 | 1 << 24, 5];
			let keys_new = [7u32, 9 | 2 << 24, 3, 12];
			let mut old: HashMap<u32, Mesh> = (1..=5u32).map(|c| (c, mesh(&mut seed, &keys_old))).collect();
			let classes: Vec<u32> = (0..12).map(|_| 1 + (rnd(&mut seed) % 5) as u32).collect();
			let mut changed = vec![1 + (rnd(&mut seed) % 5) as u32, 1 + (rnd(&mut seed) % 5) as u32];
			changed.sort_unstable();
			changed.dedup();
			// The new meshes: same geometry, other keys.
			let mut new = old.clone();
			for c in &changed {
				let m = new.get_mut(c).unwrap();
				for f in &mut m.faces {
					f.0 = keys_new[(rnd(&mut seed) % 4) as usize];
				}
			}
			let ranges_of = |ms: &HashMap<u32, Mesh>| {
				let mut at = 0;
				classes.iter().map(|c| { let r = at..at + ms[c].positions.len(); at = r.end; r }).collect::<Vec<_>>()
			};
			let ranges = ranges_of(&new);
			let order: Vec<usize> = (0..classes.len()).collect();
			let mut cf: HashMap<u32, ClassFaces> = old.iter().map(|(&c, m)| (c, ClassFaces::of(m))).collect();
			for c in &changed {
				cf.insert(*c, ClassFaces::of(&new[c]));
			}
			let r = retexture(&cf, &tex_slots, &classes, &ranges, &order, &changed);
			// Reference: the whole rebuild from the new meshes.
			let ms: Vec<&Mesh> = classes.iter().map(|c| &new[c]).collect();
			let mut by_tex: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
			let mut base = 0u32;
			for m in &ms {
				for &(k, f) in &m.faces {
					by_tex.entry(k).or_default().extend(f.map(|i| i + base));
				}
				base += m.positions.len() as u32;
			}
			let mut ref_idx = Vec::new();
			let mut ref_draws = Vec::new();
			for (k, idx) in by_tex {
				ref_draws.push(Draw { inst: 0, cull: ((k >> 24) & 3) as u8, texture: tex_slots[&(k & crate::mobies::SLOT_MASK)], start: ref_idx.len() as u32, count: idx.len() as u32 });
				ref_idx.extend(idx);
			}
			assert_eq!(r.indices, ref_idx);
			assert!(same_draws(&r.draws, &ref_draws));
			assert_eq!(r.slot_runs, super::super::super::shadowvol::slot_runs(&ms));
			let mut faces = super::super::super::clip::replay_faces(&ms, &tex_slots);
			for (fs, runs) in faces.iter_mut().zip(&r.slot_runs) {
				for f in fs.iter_mut() {
					let k = runs.partition_point(|x| x.1 + x.2 <= f.slot);
					f.run = k as u32;
					f.slot -= runs[k].1;
				}
			}
			for (s, f) in &r.faces {
				assert!(f.len() == faces[*s].len() && f.iter().zip(&faces[*s]).all(|(a, b)| (a.slot, a.run, a.idx, a.phase, a.tex, a.cull) == (b.slot, b.run, b.idx, b.phase, b.tex, b.cull)), "slot {s}");
			}
			assert_eq!(r.faces.len(), classes.iter().filter(|c| changed.contains(c)).count());
			old.clear();
		}
	}
}
