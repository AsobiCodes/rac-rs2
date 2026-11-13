//! [PC] Fur on a hero that the game draws as a prop slot (R&C2: Ratchet is a CPU-skinned prop, not the GPU marker; PARITY row 28).
//! The pack names the hero's engine class (`Pack::fur_prop`, default `u32::MAX` = none: R&C1 never builds or runs any of this). At
//! scene build the class mesh gets its own one-prop batch (the BRIDGE-36 prompt pattern: same vertex order as its slot in the props
//! batch) with a STORAGE vertex buffer for the welded normals; every `set_props` writes the slot's posed vertices into it with the
//! same `gpuskin::emulate` the CPU mirror uses, so the shells follow the drawn hero (smoothing included); `fur_draw` then draws the
//! shells from that batch exactly as from the marker.

use super::*;
use crate::viewer::gpu::pose_reuse::{slot_pose, PoseJob};
use crate::viewer::gpuskin::{emulate, PoseEntry, PropInst};
use crate::viewer::presenter::GatedQueue;

/// The hero prop's own batch: `batch.vbuf` holds this frame's posed vertices, `model` the model-space ones (weld table).
pub(crate) struct FurProp {
	pub(super) class: u32,
	pub(super) batch: Batch,
	pub(super) model: Vec<Vertex>,
	/// The slot was drawn by the last `set_props` and the buffer holds its vertices.
	pub(super) shown: bool,
	/// `mesh_sig` of the class mesh the batch was built from (`follow` rebuilds when the drawn class mesh changes).
	sig: u64,
}

/// Identity of a class mesh, cheap enough for every field: vertex and face counts plus a sample of the positions.
fn mesh_sig(m: &crate::mesh::Mesh) -> u64 {
	use std::hash::{Hash, Hasher};
	let mut h = std::collections::hash_map::DefaultHasher::new();
	(m.positions.len(), m.faces.len()).hash(&mut h);
	let step = (m.positions.len() / 16).max(1);
	for p in m.positions.iter().step_by(step) {
		p.map(f32::to_bits).hash(&mut h);
	}
	h.finish()
}

impl crate::viewer::Gpu {
	/// [PC] After the pack rebuilt or grew its prop slots: when the pack's hero class (`Pack::fur_prop`, it may change at run time) or
	/// its mesh is no longer the one the fur batch was built from (R&C2 Todano: the disc class mesh at scene build, then the RAM class
	/// mesh the props layout swaps in, 5637 -> 6420 vertices), the fur batch is rebuilt from the mesh the slots now draw, so `pose`
	/// finds the slot again; a scene without a fur batch gets one. Nothing happens while class and mesh stay the
	/// same (one map lookup and a 16-sample hash per call); R&C1 has no fur prop and returns at once. `textures` = the caller's decoded
	/// textures (key & KEY_MASK); the old fur's texels fill keys it lacks. Skipped (old fur kept) when a face texture has no slot yet.
	pub fn fur_prop_follow(&mut self, prop_meshes: &HashMap<u32, crate::mesh::Mesh>, textures: &HashMap<u32, crate::level::Texture>) {
		let class = (crate::pack::get().fur_prop)();
		if class == u32::MAX {
			return; // R&C1: no fur prop
		}
		let Some(scene) = self.scene.as_ref() else { return };
		let fs = scene.marker_fur.as_ref();
		let fp = fs.and_then(|f| f.prop.as_ref());
		if fs.is_some() && fp.is_none() {
			return; // the scene's fur is the marker's: never replaced here
		}
		let Some(m) = prop_meshes.get(&class).filter(|m| !m.positions.is_empty()) else { return };
		let sig = mesh_sig(m);
		if fp.is_some_and(|p| p.class == class && p.sig == sig) || !m.faces.iter().all(|&(t, _)| scene.tex_slots.contains_key(&(t & mobies::SLOT_MASK))) {
			return;
		}
		let copy = |t: &crate::level::Texture| crate::level::Texture { w: t.w, h: t.h, rgba: t.rgba.clone() };
		let mut tex: HashMap<u32, crate::level::Texture> = HashMap::new();
		for &(t, _) in &m.faces {
			let k = t & mobies::SLOT_MASK & mobies::KEY_MASK;
			if !tex.contains_key(&k) {
				if let Some(t) = textures.get(&k).or_else(|| fs.and_then(|fs| fs.draws.iter().flatten().find(|d| d.0 == k)).map(|d| &d.1)) {
					tex.insert(k, copy(t));
				}
			}
		}
		let old = fp.map(|p| (p.class, p.model.len()));
		let old_furs: Vec<crate::level::Texture> = fs.map_or_else(Vec::new, |fs| fur_texels(&fs.draws).into_iter().map(copy).collect());
		let g = self.ctx();
		let scene = self.scene.as_mut().unwrap();
		let mut bind = Vec::new();
		let mut fresh = pending(&g, prop_meshes, &tex, &mut bind, &mut scene.tex_slots);
		debug_assert!(bind.is_empty());
		if let Some(new) = fresh.as_mut() {
			carry_fur_texels(&mut new.draws, &old_furs);
		}
		if std::env::var_os("RAC_FUR_LOG").is_some() {
			eprintln!("furprop: hero class or mesh changed, fur batch (class, vertices) {old:x?} -> {class:#x} {:?}", fresh.as_ref().and_then(|f| f.prop.as_ref()).map(|p| p.model.len()));
		}
		if fresh.is_some() && bind.is_empty() {
			scene.marker_fur = fresh;
		}
	}
}

/// Mean absolute RGB difference per channel under which two same-size textures count as the same picture decoded twice (R&C2 Todano
/// [C]: the disc body texture and the RAM one the props layout swaps in differ by ~10-12 per channel, max 216, on 89 % of texels).
const SAME_PICTURE: f64 = 24.0;

/// [PC] The rebuilt batch's textures come from RAM and need not hash like the fur table's (decoded from the disc): a new draw whose
/// texels match no fur row takes the texels of an old draw that did match, when both are the same picture (same size, mean RGB
/// difference under `SAME_PICTURE`), so the mask is still made from the texture the pack's rectangles were read in.
fn carry_fur_texels(new: &mut [Option<(u32, crate::level::Texture)>], furs: &[crate::level::Texture]) {
	for (_, t) in new.iter_mut().flatten() {
		if is_fur(t) {
			continue;
		}
		if let Some(f) = furs.iter().find(|f| same_picture(f, t)) {
			t.rgba = f.rgba.clone();
		}
	}
}

/// The texels hash to a row of the pack's fur table.
fn is_fur(t: &crate::level::Texture) -> bool {
	spec().is_some_and(|s| s.rows.iter().any(|r| r.hash == crate::furspec::texhash(t.w, t.h, &t.rgba)))
}

/// The textures of a fur scene's draws that are fur.
fn fur_texels(draws: &[Option<(u32, crate::level::Texture)>]) -> Vec<&crate::level::Texture> {
	draws.iter().flatten().map(|d| &d.1).filter(|t| is_fur(t)).collect()
}

/// Same size and mean absolute RGB difference under `SAME_PICTURE`.
fn same_picture(a: &crate::level::Texture, b: &crate::level::Texture) -> bool {
	if a.w != b.w || a.h != b.h || a.rgba.len() != b.rgba.len() || a.rgba.is_empty() {
		return false;
	}
	let sum: u64 = a.rgba.iter().zip(&b.rgba).map(|(p, q)| (0..3).map(|c| (p[c] as i32 - q[c] as i32).unsigned_abs() as u64).sum::<u64>()).sum();
	sum as f64 / (a.rgba.len() * 3) as f64 <= SAME_PICTURE
}

/// Scene build: the fur of the pack's hero prop class, or None (no class named: R&C1; no fur spec; class not in this level).
pub(in crate::viewer) fn pending(g: &crate::viewer::gpu::SceneCtx, prop_meshes: &HashMap<u32, crate::mesh::Mesh>, textures: &HashMap<u32, crate::level::Texture>, bind: &mut Vec<wgpu::BindGroup>, slots: &mut HashMap<u32, usize>) -> Option<FurScene> {
	let class = (crate::pack::get().fur_prop)();
	if class == u32::MAX {
		return None;
	}
	let m = prop_meshes.get(&class).filter(|m| !m.positions.is_empty())?;
	let (b, _, _, model) = g.batch(&[m], None, false, textures, bind, slots);
	let size = ((model.len() * std::mem::size_of::<Vertex>()) as u64).max(4);
	let vbuf = g.device.create_buffer(&wgpu::BufferDescriptor { label: Some("fur prop vertices"), size, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
	g.queue.write_buffer(&vbuf, 0, bytemuck::cast_slice(&model));
	let batch = Batch { vbuf, ..b };
	let mut fs = pick::pending(&batch, &[], 0, slots, textures)?;
	fs.cut = Vec::new(); // a prop has no afterimage tail: every index gets shells
	fs.prop = Some(FurProp { class, batch, model, shown: false, sig: mesh_sig(m) });
	Some(fs)
}

/// Every `set_props` (after the slot records): the hero slot's posed vertices into the fur batch, or `shown` off when the slot is not
/// drawn this frame. `inst` / `pose` / `jobs` = the records `vs_props` reads (pose_reuse.rs `slot_pose`).
pub(in crate::viewer) fn pose(queue: &GatedQueue, scene: &mut Scene, mirror: &[PoseEntry], jobs: Option<&[PoseJob]>, inst: &[PropInst]) {
	let Some(fp) = scene.marker_fur.as_mut().and_then(|f| f.prop.as_mut()) else { return };
	fp.shown = false;
	let g = crate::gfx::graphics();
	if !(g.enhanced() && level(g).0 > 0) {
		return; // Fur off: nothing is drawn, nothing to write
	}
	let n = fp.model.len();
	let found = scene.props_class.iter().enumerate().position(|(si, &c)| c == fp.class && scene.props_vis.get(si) == Some(&true) && scene.props_ranges.get(si).is_some_and(|r| r.len() == n) && si < inst.len());
	if found.is_none() && std::env::var_os("RAC_FUR_LOG").is_some() {
		// [PC] test log (ENV.md): why the hero slot was not found (class slots, their visibility and vertex counts vs the fur batch's).
		let slots: Vec<_> = scene.props_class.iter().enumerate().filter(|e| *e.1 == fp.class).map(|(si, _)| (si, scene.props_vis.get(si).copied(), scene.props_ranges.get(si).map(|r| r.len()), si < inst.len())).collect();
		eprintln!("furprop: class {:#x} n {n} found {found:?} slots {slots:?}", fp.class);
	}
	let Some(si) = found else { return };
	let r = scene.props_ranges[si].clone();
	let (rec, entries) = slot_pose(mirror, jobs, &inst[si]);
	let vs: Vec<Vertex> = scene.props_model[r].iter().enumerate().map(|(j, m0)| emulate(&rec, &entries, m0, j)).collect();
	queue.write_buffer(&fp.batch.vbuf, 0, bytemuck::cast_slice(&vs));
	fp.shown = true;
}

#[cfg(test)]
mod tests {
	use super::same_picture;
	use crate::level::Texture;

	fn tex(w: usize, v: u8) -> Texture {
		Texture { w, h: 1, rgba: vec![[v, v, v, 255]; w] }
	}

	/// The same picture decoded twice (small colour differences) matches; another size or a different picture does not.
	#[test]
	fn same_picture_allows_decode_noise_only() {
		assert!(same_picture(&tex(4, 100), &tex(4, 110)));
		assert!(!same_picture(&tex(4, 100), &tex(4, 160)));
		assert!(!same_picture(&tex(4, 100), &tex(8, 100)));
	}
}
