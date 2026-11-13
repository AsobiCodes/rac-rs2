//! The particle pipelines of `Gpu::new` (`part_pipelines`): alpha, additive, fogged (water), then the R&C2 effect prims whose GS TEST has
//! no depth test (layer_fx.rs `NODEPTH`): alpha, additive. Index = `additive as usize + 3 * no_depth as usize`, 2 = the fogged one.
//! Then R&C2's replace-blend layers (layer_fx.rs `REPLACE`, drawn in the level pass; viewer/layer_replace.wgsl): `REPLACE_PIPE` the
//! pixels passing the GS alpha test with depth writes, + 1 without, + 2 the failing ones' colour (no depth), + 3 their depth alone.

/// The first replace-blend layer pipeline (GS ALPHA 0x88 = Cs * As, the destination not read): see the module doc for the next three.
pub(super) const REPLACE_PIPE: usize = 5;

/// GS ALPHA (Cs - 0) * As >> 7 + 0: the colour scaled by its alpha, nothing of the destination (alpha as the other layers: over).
pub(super) const REPLACE_BLEND: wgpu::BlendState = wgpu::BlendState {
	color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::Zero, operation: wgpu::BlendOperation::Add },
	alpha: wgpu::BlendComponent::OVER,
};

/// The destination kept as it is (GS AFAIL ZB_ONLY: a failing pixel writes its depth, not its colour).
const KEEP_COLOUR: wgpu::BlendState = wgpu::BlendState {
	color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
	alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
};

/// `make(vs, fs, blend, depth_write, compare)` builds one pipeline of the main shader.
pub(super) fn part_pipelines<P>(make: impl Fn(&str, &str, Option<wgpu::BlendState>, bool, wgpu::CompareFunction) -> P, additive: wgpu::BlendState) -> [P; 9] {
	let ztest = crate::gs_state::ztest();
	[
		make("vs", "fs_part", Some(wgpu::BlendState::ALPHA_BLENDING), false, ztest),
		make("vs", "fs_part", Some(additive), false, ztest),
		// Water (prog10 fogs per vertex like tfrag, lighting_fog.md): alpha blend with the scene fog.
		make("vs", "fs_part_fog", Some(wgpu::BlendState::ALPHA_BLENDING), false, ztest),
		make("vs", "fs_part", Some(wgpu::BlendState::ALPHA_BLENDING), false, wgpu::CompareFunction::Always),
		make("vs", "fs_part", Some(additive), false, wgpu::CompareFunction::Always),
		make("vs_layer", "fs_layer_pass", Some(REPLACE_BLEND), true, ztest),
		make("vs_layer", "fs_layer_pass", Some(REPLACE_BLEND), false, ztest),
		make("vs_layer", "fs_layer_fail", Some(REPLACE_BLEND), false, ztest),
		make("vs_layer", "fs_layer_fail", Some(KEEP_COLOUR), true, ztest),
	]
}

impl super::Gpu {
	/// The pipeline of a particle run: the soft one with the particles' soft pass (no-depth layer runs: `partfx.over`), else `part_pipelines` (no-depth effect prims `layer_fx::no_depth(key)` take 3..5).
	pub(super) fn part_pipeline(&self, part_on: bool, additive: bool, key: u32) -> &wgpu::RenderPipeline {
		if part_on {
			// [PC] R&C2's no-depth effect prims stay over the world (partfx.rs `over`); R&C1 never has layer keys.
			&[&self.partfx.soft, &self.partfx.over][super::layer_fx::no_depth(key) as usize][additive as usize]
		} else {
			&self.part_pipelines[additive as usize + 3 * super::layer_fx::no_depth(key) as usize]
		}
	}
}
