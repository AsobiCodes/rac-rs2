//! R&C2 world layers (the ship's canopy glass, lamp / nanotech / bolt glows) drawn natively (round 3 batch 4). The pack unprojects
//! the triangles the GS captured (`gsfull/layer_tris.rs`) into world space; they go into the particle pass's vertex lists
//! (`part_verts`, one degenerate quad per triangle) under texture keys of their own, so they are drawn at the window's resolution,
//! at the smoothed camera, depth-tested against the world, with the blend the game used. Game-neutral; only a pack that calls
//! `set_layer_tris` has any (R&C1 never does).
use super::*;
use std::sync::Arc;

/// First `fx_tex` / `part_verts` texture key of the layer textures; `SLOTS` keys follow.
pub const LAYER_KEY: u32 = 0x3000_0000;
/// One slot per distinct texture a field may hand out (`layer_tris::MAX_TEX`) plus the blank one of the untextured prims, so no
/// triangle the GS removed from the overlay is ever skipped here.
const SLOTS: usize = crate::gsfull::layer_tris::MAX_TEX + 1;
/// Added to a layer key for the triangles drawn WITHOUT the depth test (an effect prim whose GS TEST has ZTST ALWAYS / ZTE off).
pub const NODEPTH: u32 = 0x100;
/// Added to a layer key for the replace-blend triangles (`WorldTri::replace`, GS ALPHA 0x88: Cs * As, no destination term); with
/// `ZWRITE` they write depth. Drawn in the level pass (`pass_layer_replace`), not in the particle pass.
pub const REPLACE: u32 = 0x200;
pub const ZWRITE: u32 = 0x400;
/// Added to a replace key whose pixels failing the GS alpha test take a second draw (`AlphaTest::sides`): their colour without depth
/// (`FAIL_RGB`: AFAIL FB_ONLY / RGB_ONLY) or their depth alone (`FAIL_Z`: ZB_ONLY).
pub const FAIL_RGB: u32 = 0x800;
pub const FAIL_Z: u32 = 0x1000;
/// The layer keys: `LAYER_KEY` + slot | the flags.
const LAYER_KEYS: std::ops::Range<u32> = LAYER_KEY..LAYER_KEY + 2 * FAIL_Z;

/// Is this `part_verts` key a layer run drawn without the depth test?
pub fn no_depth(key: u32) -> bool {
	LAYER_KEYS.contains(&key) && key & NODEPTH != 0
}

/// A replace-blend layer run (`REPLACE`): Some(writes depth), None for every other key.
pub fn replace(key: u32) -> Option<bool> {
	(LAYER_KEYS.contains(&key) && key & REPLACE != 0).then_some(key & ZWRITE != 0)
}

/// The second draw of a replace run (the pixels failing the alpha test): Some(depth alone) / Some(false) = colour without depth.
pub fn fail_side(key: u32) -> Option<bool> {
	(replace(key).is_some() && key & (FAIL_RGB | FAIL_Z) != 0).then_some(key & FAIL_Z != 0)
}

/// The `fx_tex` key of a `part_verts` key: a layer key without its flags, any other key as it is.
pub fn tex_key(key: u32) -> usize {
	if LAYER_KEYS.contains(&key) { (key & !(NODEPTH | REPLACE | ZWRITE | FAIL_RGB | FAIL_Z)) as usize } else { key as usize }
}

/// The `part_verts` key flags of a layer triangle.
fn flags(t: &WorldTri) -> u32 {
	let depth = if t.depth { 0 } else { NODEPTH };
	if !t.replace {
		return depth;
	}
	let fail = match t.alpha.sides(t.zwrite).1 {
		Some((true, _)) => FAIL_RGB,
		Some((false, _)) => FAIL_Z,
		None => 0,
	};
	depth | REPLACE | if t.zwrite { ZWRITE } else { 0 } | fail
}

/// One triangle in world space: positions, texture coordinates, colours (1.0 = the game's 128), the texture (`id` = its texel hash) and
/// the blend.
pub struct WorldTri {
	pub pos: [[f32; 3]; 3],
	pub uv: [[f32; 2]; 3],
	pub col: [[f32; 4]; 3],
	pub id: u64,
	pub tex: Arc<Texture>,
	pub additive: bool,
	/// Depth-tested against the world (false: drawn over it, the prim's GS TEST had no depth test).
	pub depth: bool,
	/// From an effect-program record (VU1 program 2: sparks, fire, explosions; `LayerTri.world` Some), not a GS world layer (lamp /
	/// bolt glows): only these light the scene with Particles Enhanced (`LayerFx::lit`), as R&C1 keeps its fixed glows out.
	pub effect: bool,
	/// GS ALPHA (Cs - 0) * As + 0 (`layer_tris::LayerTri::replace`): Cs * As replaces the picture (`additive` is false).
	pub replace: bool,
	/// With `replace`: writes depth (the prim's ZBUF has ZMSK off).
	pub zwrite: bool,
	/// With `replace`: the prim's GS alpha test (which pixels write colour / depth); ALWAYS otherwise.
	pub alpha: crate::gsfull::layer_tris::AlphaTest,
}

/// The textures held for the layers: texel hash -> (slot, the call that last used it).
#[derive(Default)]
pub struct LayerFx {
	slots: HashMap<u64, (u32, u64)>,
	tick: u64,
	/// [PC] Particles Enhanced: the additive effect-record triangles of the last call, per layer key (one degenerate quad each), for
	/// the effect lights (draw/passes.rs `pass_fx_lights`; their `part_verts` runs are run 0, which R&C1's lights skip).
	lit: BTreeMap<u32, Vec<Vertex>>,
	/// Per layer key: its texture's mean alpha-weighted colour (partfx.rs `tint_of`), set on upload.
	tints: HashMap<u32, [f32; 3]>,
	/// [PC] Fire / Smoke for a pack whose pool sprites are layer quads (R&C2, rac2/host/ov_fire.rs; empty for R&C1 = no effect): the
	/// quad each sprite of the next `set_particles` draws as (parallel to its sprite list), taken by that call.
	pool_quads: Vec<PoolQuad>,
	/// Per sprite of the last `set_particles` that had `pool_quads`: taken over by the smoke volumes or the fire (`Gpu::pool_taken`).
	taken: Vec<bool>,
}

/// [PC] A pool sprite's own quad (`Gpu::set_pool_quads`): corners in quad order (two triangles 0 1 2 and 0 2 3), uv, colour (1.0 = 128).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PoolQuad {
	pub pos: [[f32; 3]; 4],
	pub uv: [[f32; 2]; 4],
	pub col: [[f32; 4]; 4],
}

impl PoolQuad {
	pub(super) fn verts(&self) -> [Vertex; 4] {
		std::array::from_fn(|k| Vertex { pos: self.pos[k], uv: self.uv[k], normal: [0.0; 3], colour: self.col[k], light: [0.0; 4] })
	}
}

impl LayerFx {
	/// The slot for a texture: its own, or a free one (the least recently used when all are taken, never one used this call).
	fn slot(&mut self, id: u64, held: impl Fn(u32) -> bool) -> Option<(u32, bool)> {
		if let Some(e) = self.slots.get_mut(&id) {
			e.1 = self.tick;
			return Some((e.0, !held(e.0)));
		}
		let used: Vec<u32> = self.slots.values().map(|e| e.0).collect();
		let free = (0..SLOTS as u32).find(|s| !used.contains(s)).or_else(|| {
			let (&old, &(s, at)) = self.slots.iter().min_by_key(|e| e.1.1)?;
			(at != self.tick).then(|| {
				self.slots.remove(&old);
				s
			})
		})?;
		self.slots.insert(id, (free, self.tick));
		Some((free, true))
	}

	/// The effect-record quads that light the scene (`lit`); empty unless a pack hands effect records (R&C1 never does).
	pub(in crate::viewer) fn lit(&self) -> &BTreeMap<u32, Vec<Vertex>> {
		&self.lit
	}

	/// Start of `set_particles` over `n` sprites: the pack's quads for them (empty = draw the sprites as before) and a fresh taken list.
	pub(super) fn begin_pool(&mut self, n: usize) -> Vec<PoolQuad> {
		let q = std::mem::take(&mut self.pool_quads);
		self.taken.clear();
		self.taken.resize(if q.is_empty() { 0 } else { n }, false);
		q
	}

	/// `set_particles`: sprite `i` was taken over (smoke volume or fire); nothing unless the pack handed quads.
	pub(super) fn took(&mut self, i: usize) {
		if let Some(t) = self.taken.get_mut(i) {
			*t = true;
		}
	}

	/// `base` (the pool's particle tints) with the layer keys' tints added, for `partfx::lights` over `lit`.
	pub(in crate::viewer) fn tints_over(&self, base: &HashMap<u32, [f32; 3]>) -> HashMap<u32, [f32; 3]> {
		let mut t = base.clone();
		t.extend(self.tints.iter().map(|(k, v)| (*k, *v)));
		t
	}
}

impl Gpu {
	/// [PC] The `part_verts` texture key a texture (`WorldTri::id`) has after the last `set_layer_tris` (| `NODEPTH` when `depth` is
	/// false), None = not held. A pool sprite whose `tex` is `FX_TEX` + this key draws with that layer texture.
	pub fn layer_key(&self, id: u64, depth: bool) -> Option<u32> {
		let k = LAYER_KEY + self.layer_fx.slots.get(&id)?.0;
		self.fx_tex.contains_key(&(k as usize)).then_some(if depth { k } else { k | NODEPTH })
	}

	/// [PC] The quads the sprites of the next `set_particles` call draw as (parallel to its sprite list; R&C2's effect quads). Never
	/// called by R&C1: its sprites draw as they always did.
	pub fn set_pool_quads(&mut self, quads: Vec<PoolQuad>) {
		self.layer_fx.pool_quads = quads;
	}

	/// [PC] Per sprite of the last `set_particles` that had pool quads: taken over by the smoke volumes or the fire (its quad comes back
	/// only on a frame where they do not draw). Empty after a call without pool quads.
	pub fn pool_taken(&self) -> &[bool] {
		&self.layer_fx.taken
	}

	/// [PC] R&C2's replace-blend layers (`REPLACE` keys; GS ALPHA 0x88, the level-14 lava sheet): drawn in the level pass after the
	/// world (terrain, ties: the lava is sent while the tie program is loaded [C: RAC2_OVLKIND prog 8]) and before the blended mobys,
	/// the shadow volumes and the particles, depth-tested, Cs * As over whatever is there. The prim's GS alpha test splits a run in two
	/// draws (`AlphaTest::sides`): the passing pixels write colour and, with `ZWRITE`, depth; then the failing ones what AFAIL lets
	/// through (`fail_side`). So the shadows fall on the sheet and what is behind it stays hidden, as on the GS. (Inside one run the
	/// failing pixels come after the passing ones, not in the GS's prim order.) R&C1 never has layer keys.
	pub(in crate::viewer) fn pass_layer_replace(&self, pass: &mut wgpu::RenderPass<'_>, warp: bool) {
		let runs: Vec<(u32, &Vec<Vertex>)> = self.part_verts.iter().filter(|((_, _, k), v)| !warp && replace(*k).is_some() && !v.is_empty()).map(|((_, _, k), v)| (*k, v)).collect();
		if runs.is_empty() {
			return;
		}
		let all: Vec<Vertex> = runs.iter().flat_map(|r| r.1.iter().copied()).collect();
		let vb = crate::viewer::framebuf::upload(&self.device, &self.queue, "replace layer vertices", bytemuck::cast_slice(&all), wgpu::BufferUsages::VERTEX);
		let ib = crate::viewer::framebuf::quad_indices(&self.device, &self.queue, "replace layer indices", all.len() as u32 / 4);
		pass.set_bind_group(0, &self.ubind, &[]);
		pass.set_vertex_buffer(0, vb.slice(..));
		pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
		let mut first = 0u32;
		for (key, v) in runs {
			let count = v.len() as u32 / 4 * 6;
			if let (Some(tex), Some(zw)) = (self.fx_tex.get(&tex_key(key)), replace(key)) {
				pass.set_pipeline(&self.part_pipelines[super::part_pipes::REPLACE_PIPE + !zw as usize]);
				pass.set_bind_group(1, tex, &[]);
				pass.draw_indexed(first..first + count, 0, 0..1);
				if let Some(z) = fail_side(key) {
					pass.set_pipeline(&self.part_pipelines[super::part_pipes::REPLACE_PIPE + 2 + z as usize]);
					pass.draw_indexed(first..first + count, 0, 0..1);
				}
			}
			first += count;
		}
	}

	/// Replaces the layer triangles drawn this frame and the next ones (until the next call) by `tris`.
	pub fn set_layer_tris(&mut self, tris: &[WorldTri]) {
		self.part_verts.retain(|(_, _, k), _| !LAYER_KEYS.contains(k));
		self.layer_fx.tick += 1;
		self.layer_fx.lit.clear();
		for t in tris {
			let fx_tex = &self.fx_tex;
			let Some((slot, upload)) = self.layer_fx.slot(t.id, |s| fx_tex.contains_key(&((LAYER_KEY + s) as usize))) else { continue };
			let key = LAYER_KEY + slot;
			if upload {
				let bind = self.upload_texture(Some(&t.tex));
				self.fx_tex.insert(key as usize, bind);
				self.layer_fx.tints.insert(key, crate::viewer::partfx::tint_of(&t.tex));
			}
			let out = self.part_verts.entry((0, t.additive, key | flags(t))).or_default();
			// A replace triangle's normal x carries its alpha test for the shader (viewer/layer_replace.wgsl `vs_layer`).
			let word = if t.replace { t.alpha.first_draw(t.zwrite).word() as f32 } else { 0.0 };
			let v = |i: usize| Vertex { pos: t.pos[i], uv: t.uv[i], normal: [word, 0.0, 0.0], colour: t.col[i], light: [0.0; 4] };
			out.extend([v(0), v(1), v(2), v(2)]);
			if t.effect && t.additive {
				self.layer_fx.lit.entry(key).or_default().extend([v(0), v(1), v(2), v(2)]);
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::gsfull::layer_tris::AlphaTest;

	#[test]
	fn a_texture_keeps_its_slot_and_a_full_store_recycles_the_oldest() {
		let mut f = LayerFx::default();
		f.tick = 1;
		assert_eq!(f.slot(7, |_| true), Some((0, true)), "first use uploads");
		assert_eq!(f.slot(7, |_| true), Some((0, false)), "second use reuses the bind group");
		assert_eq!(f.slot(7, |_| false), Some((0, true)), "a bind group the engine dropped is uploaded again");
		// Fill all slots in call 1, then call 2 recycles the least recently used one for a new texture.
		for id in 100..100 + SLOTS as u64 - 1 {
			assert!(f.slot(id, |_| true).is_some());
		}
		f.tick = 2;
		assert_eq!(f.slot(7, |_| true), Some((0, false)));
		let (s, up) = f.slot(999, |_| true).unwrap();
		assert!(up && s != 0, "slot {s}");
		// A call in which every slot is in use refuses a new texture instead of stealing one.
		let mut g = LayerFx::default();
		g.tick = 1;
		for id in 0..SLOTS as u64 {
			assert!(g.slot(id, |_| true).is_some());
		}
		assert_eq!(g.slot(500, |_| true), None);
	}

	#[test]
	fn every_texture_a_field_may_capture_has_a_slot() {
		// MAX_TEX distinct textures plus the blank one of the untextured prims: none of them is skipped (the GS already removed the prim).
		assert!(SLOTS > crate::gsfull::layer_tris::MAX_TEX);
		assert!(SLOTS as u32 <= NODEPTH, "slot numbers must not reach the NODEPTH flag");
		assert_eq!(tex_key(LAYER_KEY + 5 + NODEPTH), (LAYER_KEY + 5) as usize);
		assert_eq!(tex_key(LAYER_KEY + 5 + REPLACE + ZWRITE), (LAYER_KEY + 5) as usize);
		assert_eq!(tex_key(7), 7);
	}

	#[test]
	fn a_replace_layer_gets_its_own_key_flags() {
		let tex = Arc::new(Texture { w: 1, h: 1, rgba: vec![[255; 4]] });
		let t = |depth, replace, zwrite| WorldTri { pos: [[0.0; 3]; 3], uv: [[0.0; 2]; 3], col: [[1.0; 4]; 3], id: 1, tex: tex.clone(), additive: false, depth, effect: false, replace, zwrite, alpha: AlphaTest::ALWAYS };
		assert_eq!(flags(&t(true, false, false)), 0, "over / additive layers keep their keys");
		assert_eq!(flags(&t(false, false, false)), NODEPTH);
		assert_eq!(flags(&t(true, true, true)), REPLACE | ZWRITE);
		assert_eq!(replace(LAYER_KEY + 3 + REPLACE + ZWRITE), Some(true));
		assert_eq!(replace(LAYER_KEY + 3 + REPLACE), Some(false));
		assert_eq!(replace(LAYER_KEY + 3), None);
		assert_eq!(replace(REPLACE | ZWRITE), None, "a pool texture key is never a layer");
		assert!(!no_depth(LAYER_KEY + 3 + REPLACE + ZWRITE));
	}

	#[test]
	fn a_replace_layer_s_alpha_test_picks_its_second_draw() {
		let tex = Arc::new(Texture { w: 1, h: 1, rgba: vec![[255; 4]] });
		let t = |zwrite, afail| WorldTri { pos: [[0.0; 3]; 3], uv: [[0.0; 2]; 3], col: [[1.0; 4]; 3], id: 1, tex: tex.clone(), additive: false, depth: true, effect: false, replace: true, zwrite, alpha: AlphaTest { atst: 5, aref: 0x80, afail } };
		// The lava (TEST 0x5380b: GEQUAL 0x80, RGB_ONLY; ZMSK off): the failing pixels write their colour without depth.
		let lava = flags(&t(true, 3));
		assert_eq!(lava, REPLACE | ZWRITE | FAIL_RGB);
		assert_eq!((replace(LAYER_KEY + 2 + lava), fail_side(LAYER_KEY + 2 + lava)), (Some(true), Some(false)));
		assert_eq!(flags(&t(true, 1)), REPLACE | ZWRITE | FAIL_RGB, "FB_ONLY");
		assert_eq!(flags(&t(true, 2)), REPLACE | ZWRITE | FAIL_Z, "ZB_ONLY: depth alone");
		assert_eq!(fail_side(LAYER_KEY + 2 + flags(&t(true, 2))), Some(true));
		assert_eq!(flags(&t(true, 0)), REPLACE | ZWRITE, "KEEP: one draw, the failing pixels discarded");
		assert_eq!(flags(&t(false, 3)), REPLACE, "no depth writes: RGB_ONLY writes every pixel alike");
		assert_eq!(fail_side(LAYER_KEY + 2 + REPLACE + ZWRITE), None);
		assert_eq!(fail_side(LAYER_KEY + 2 + FAIL_RGB), None, "only a replace run has a second draw");
		// Every flag stays inside the layer keys and off the texture slot.
		let all = NODEPTH | REPLACE | ZWRITE | FAIL_RGB | FAIL_Z;
		assert!(LAYER_KEYS.contains(&(LAYER_KEY + SLOTS as u32 - 1 + all)));
		assert_eq!(tex_key(LAYER_KEY + 9 + all), (LAYER_KEY + 9) as usize);
		// The shader word of each draw: the lava keeps its test; RGB_ONLY without depth writes needs none.
		assert_eq!(t(true, 3).alpha.first_draw(true).word(), 5 | 0x80 << 3);
		assert_eq!(t(false, 3).alpha.first_draw(false), AlphaTest::ALWAYS);
	}
}
