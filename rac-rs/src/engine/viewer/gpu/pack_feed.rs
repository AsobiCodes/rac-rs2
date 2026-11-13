//! What a game pack feeds the loaded scene while the game runs (engine split stage E5, 2026-10-27). R&C2 decodes the classes its
//! game builds in RAM (weapons, armor, effects) and reads their textures from GS memory (gsfull::texreq) only once the game has
//! uploaded them: their face keys get texture slots here, so `append_props` / `regrow_props` can then place those classes. And
//! R&C2's ties take the colours the game sends each field (rac2/tiecol.rs) instead of colours baked at load.

use super::*;

impl Gpu {
	/// Gives each face key of `keys` whose draw slot (`mobies::SLOT_MASK`: texture, CLAMP wrap, moby TEX1 K code) the scene has
	/// none for yet its slot, uploaded as the scene build does (`SceneCtx::batch`; textures by `KEY_MASK` key): the bind group, its
	/// moby draw word and see-through flag. Slots added here are not in the bindless array (ties and shrubs only) and take no
	/// texture-pack replacement. Returns the number of slots added (0 without a scene).
	pub fn add_texture_slots(&mut self, keys: &[u32], textures: &HashMap<u32, Texture>) -> usize {
		let Some(mut scene) = self.scene.take() else { return 0 };
		let ctx = self.ctx();
		let mut n = 0;
		for &key in keys {
			let wrap = (key >> crate::shrubs::WRAP_SHIFT) & 3;
			let moby = super::k_slot(key);
			let kcode = if moby { key >> crate::mobies::K_CODE_SHIFT } else { 0 };
			let slot_key = key & (crate::mobies::KEY_MASK | 3 << crate::shrubs::WRAP_SHIFT) | kcode << crate::mobies::K_CODE_SHIFT;
			if scene.tex_slots.contains_key(&slot_key) {
				continue;
			}
			let tex = key & crate::mobies::KEY_MASK;
			let t = textures.get(&tex);
			let chain = moby_chain(slot_key, t).filter(|_| kcode != 0);
			scene.bind_groups.push(ctx.upload_slot(tex, t, chain.as_ref().map(|c| c.as_slice()), wrap, false));
			scene.tex_slots.insert(slot_key, scene.bind_groups.len() - 1);
			scene.slot_words.push(moby_word(slot_key, t) | crate::viewer::metalfx::texture_flag(t));
			let one: HashMap<u32, usize> = [(slot_key, 0)].into_iter().collect();
			scene.clear_tex.push(clear_slots(&one, 1, textures)[0]);
			n += 1;
		}
		self.scene = Some(scene);
		n
	}

	/// The texels of texture key `tex` (`KEY_MASK` key) changed (R&C2's GS textures, refreshed from VRAM): every slot the scene
	/// has for it is uploaded again in place (bind group, moby draw word, see-through flag), so the key keeps its one slot.
	/// Returns the number of slots replaced (0 without a scene or a slot).
	pub fn replace_texture_slots(&mut self, tex: u32, textures: &HashMap<u32, Texture>) -> usize {
		let Some(mut scene) = self.scene.take() else { return 0 };
		let ctx = self.ctx();
		let t = textures.get(&tex);
		let slots: Vec<(u32, usize)> = scene.tex_slots.iter().filter(|(k, _)| **k & crate::mobies::KEY_MASK == tex).map(|(&k, &i)| (k, i)).collect();
		// What every slot of this texture shares is decided once (the metal hash and the see-through count scan all texels): the
		// flag, the clear bit, and the bind group per (wrap, has a chain) - the same texels in the same sampler.
		let flag = crate::viewer::metalfx::texture_flag(t);
		let clear = slots.first().map(|&(k, _)| clear_slots(&[(k, 0)].into_iter().collect(), 1, textures)[0]);
		let mut binds: Vec<((u32, bool), wgpu::BindGroup)> = Vec::new();
		for &(slot_key, i) in &slots {
			let wrap = (slot_key >> crate::shrubs::WRAP_SHIFT) & 3;
			let kcode = slot_key >> crate::mobies::K_CODE_SHIFT;
			let chain = moby_chain(slot_key, t).filter(|_| kcode != 0);
			let id = (wrap, chain.is_some());
			let bind = match binds.iter().find(|b| b.0 == id) {
				Some(b) => b.1.clone(),
				None => {
					let b = ctx.upload_slot(tex, t, chain.as_ref().map(|c| c.as_slice()), wrap, false);
					binds.push((id, b.clone()));
					b
				}
			};
			scene.bind_groups[i] = bind;
			scene.slot_words[i] = moby_word_for(slot_key, chain.as_deref()) | flag;
			scene.clear_tex[i] = clear.unwrap_or(false);
		}
		self.scene = Some(scene);
		slots.len()
	}

	/// Replaces tie `at`'s slot colours (`LitTie::cols`, same length; 128 = 1.0): the drawn LOD is recoloured from them in
	/// `cull_ties`, as after `relight`. False without a scene, for an unknown tie or a length that differs.
	pub fn set_tie_colours(&mut self, at: usize, cols: &[[f32; 4]]) -> bool {
		let Some(scene) = &mut self.scene else { return false };
		let Some(c) = scene.tie_cols.get_mut(at).filter(|c| c.len() == cols.len()) else { return false };
		c.copy_from_slice(cols);
		scene.tie_stale[at] = [true; 3];
		if let Some(g) = &mut scene.tie_gpu {
			g.caps_dirty.push(at);
		}
		true
	}
}
