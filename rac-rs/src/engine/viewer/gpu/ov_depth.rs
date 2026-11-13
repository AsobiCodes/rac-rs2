//! The GS overlay's effect depth (engine split stage E5, 2026-10-27): a game whose HUD overlay also carries the game's own
//! effects (R&C2: water, particles, drawn by the GS in overlay mode, engine/gsfull/gs_overlay.rs) gives each overlay pixel the
//! view depth of the effect that drew it (0 = HUD, always in front). The overlay quad (`RETAIL_OV_TEX`) is then drawn with that
//! depth against the world's depth buffer, so an effect behind a wall or a moby is left out. Same blend as the plain overlay
//! (premultiplied colour plus transmission). From lane rc2's window.rs `fs_overlay` (there a discard against the linearised
//! native depth, here the depth test).
//! Edges: the colour is a bilinear sample, so a pixel next to an effect texel takes some of its colour; it gets that effect's
//! depth (`fx_depth`: 1/z interpolated over the effect texels of the 2 x 2, so a wall cuts a glow along a smooth line, not in
//! GS-texel steps; the nearest depth of the texels that add colour (bilinear weight > 0.02) where their depths differ a lot).
//! With the depth of its own texel only (a HUD or empty texel: in front), an effect behind the world
//! showed as thin lines along its lower and right borders [C: R&C2 level 12 field 4000, a world layer at 230 m behind a
//! cliff face at 24 m, debug picture 2026-10-27].
//! One depth per pixel (`gsfull/ovl_vote.rs` picks it): a faint near effect that overlaps a dominant far one behind a wall is dropped
//! in the shared pixels, because the whole pixel takes the far depth. Known trade-off; the real fix is a second overlay layer.

use super::*;

const HEAD: &str = "
@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(1) @binding(0) var fxd: texture_2d<f32>;
// nf[0] = (near, far, warp on, 0); nf[1..4] = the rows of the warp matrix; nf[4] = (kx, ky, 0, 0): the view's tangents across the picture; nf[5] = the newest eye minus the shown eye in the shown view's axes.
@group(1) @binding(1) var<uniform> nf: array<vec4<f32>, 6>;
struct V { @builtin(position) p: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs(@location(0) p: vec3<f32>, @location(1) uv: vec2<f32>) -> V {
	var o: V;
	o.p = vec4<f32>(p.xy, 0.0, 1.0);
	o.uv = uv;
	return o;
}
";

/// [b46 fix round] Depths from here on are the backdrop marker (`gsfull::ovl_vote::SPACE_M` = 1e9).
const SPACE_MIN: f32 = 1.0e8;

/// The edge-aware effect depth (its own const so the test below can run exactly this text on the GPU).
const FX_DEPTH: &str = "
// The effect depth of the pixel: 1/z interpolated bilinearly over the effect texels (z > 0) of the 2 x 2 the colour's bilinear
// sample blends, weights renormalised to the effect texels, so the line where a wall cuts a glow, a shield or a flame follows
// the surface smoothly and not in GS-texel steps. Where the texels' depths differ a lot (the edge between two effects, e.g. a
// near glow over a far flame) the interpolation would invent a depth in between: take the nearest depth of the effect texels that
// add colour (bilinear weight > MIN_W) then, so no effect that contributes to the colour is dropped, and a texel that adds ~nothing
// (a 1 % glow at 5 m next to 200 m / 300 m fire) cannot pull the whole pixel in front of a wall (review B3/4 R4). Only when no
// effect texel passes the weight (the pixel is almost all HUD / empty) the nearest of all four is used (0: none is an effect).
const EDGE: f32 = 0.2;
const MIN_W: f32 = 0.02;
fn fx_depth(uv: vec2<f32>, s: vec2<f32>) -> f32 {
	let p = uv * s - vec2<f32>(0.5);
	let t = floor(p);
	let f = p - t;
	var wsum = 0.0;
	var isum = 0.0;
	var imin = 1.0e9;
	var imax = 0.0;
	var fd = 0.0;
	var fd_any = 0.0;
	for (var k = 0; k < 4; k++) {
		let o = vec2<f32>(f32(k & 1), f32(k >> 1));
		let q = clamp(t + o, vec2<f32>(0.0), s - vec2<f32>(1.0));
		let z = textureLoad(fxd, vec2<i32>(q), 0).r;
		if (z > 0.0) {
			if (fd_any == 0.0 || z < fd_any) {
				fd_any = z;
			}
			let w = mix(1.0 - f.x, f.x, o.x) * mix(1.0 - f.y, f.y, o.y);
			if (w > MIN_W) {
				if (fd == 0.0 || z < fd) {
					fd = z;
				}
				let iz = 1.0 / z;
				wsum += w;
				isum += w * iz;
				imin = min(imin, iz);
				imax = max(imax, iz);
			}
		}
	}
	if (wsum <= 0.0) {
		return fd_any;
	}
	if (imax - imin > EDGE * imax) {
		return fd;
	}
	return wsum / isum;
}
";

/// The smooth reconstruction of the effect texels (round 3 batch 4, issues 3 / 14 / 15): the GS draws its effects (glass, glows, flames,
/// particles) at 512 x 448 with no edge smoothing, so a bilinear fetch shows every polygon edge as a stair of 3 to 6 window pixels.
/// A Mitchell-Netravali filter (B = C = 1/3, 4 x 4 texels, weights sum to 1) turns the stairs into smooth ramps; only effect texels
/// use it (the HUD without the sharp picture stays a plain bilinear fetch). The result is clamped (the filter's lobes dip below 0).
const FX_CUBIC: &str = "
fn mitchell(x: f32) -> f32 {
	let a = abs(x);
	if (a < 1.0) {
		return (7.0 * a * a * a - 12.0 * a * a + 16.0 / 3.0) / 6.0;
	}
	if (a < 2.0) {
		return (-7.0 / 3.0 * a * a * a + 12.0 * a * a - 20.0 * a + 32.0 / 3.0) / 6.0;
	}
	return 0.0;
}
fn fx_cubic(uv: vec2<f32>) -> vec4<f32> {
	let sz = vec2<f32>(textureDimensions(tex));
	let p = uv * sz - vec2<f32>(0.5);
	let t = floor(p);
	let f = p - t;
	var acc = vec4<f32>(0.0);
	for (var j = -1; j <= 2; j++) {
		for (var i = -1; i <= 2; i++) {
			let q = clamp(vec2<i32>(t) + vec2<i32>(i, j), vec2<i32>(0), vec2<i32>(sz) - vec2<i32>(1));
			acc += textureLoad(tex, q, 0) * (mitchell(f.x - f32(i)) * mitchell(f.y - f32(j)));
		}
	}
	return clamp(acc, vec4<f32>(0.0), vec4<f32>(1.0));
}
";

const TAIL: &str = "
struct F { @location(0) c: vec4<f32>, @builtin(frag_depth) d: f32 };
@fragment fn fs(v: V) -> F {
	let s = vec2<f32>(textureDimensions(fxd));
	var c = textureSampleLevel(tex, samp, v.uv, 0.0);
	var fd = fx_depth(v.uv, s);
	if (fd > 0.0) {
		c = fx_cubic(v.uv);
	}
	// Frame smoothing draws the world between two fields but the GS picture is the newest field's: the effect texels (depth > 0) are
	// fetched along the rotation between the two cameras, so they stay on the world; HUD and empty texels stay where they are. A HUD
	// pixel (no effect depth, not the clear colour rgb 0 / alpha 1) is kept as it is: a warped effect must not paint over it.
	let hud = fd == 0.0 && (max(max(c.r, c.g), c.b) > 0.004 || c.a < 0.996);
	if (nf[0].z > 0.5 && !hud) {
		let ds = vec3<f32>((v.uv.x - 0.5) * nf[4].x, (v.uv.y - 0.5) * nf[4].y, 1.0);
		var dn = vec3<f32>(dot(nf[1].xyz, ds), dot(nf[2].xyz, ds), dot(nf[3].xyz, ds));
		if (dn.z > 0.05) {
			// The eye moved too: the effect texel found along the rotation sits at a depth, i.e. at a point of the world; seen from the
			// shown eye that point lands off this pixel by `e` (parallax); fetch from `e` further back along the picture (one step).
			let z0 = fx_depth(clamp(vec2<f32>(dn.x / dn.z / nf[4].x + 0.5, dn.y / dn.z / nf[4].y + 0.5), vec2<f32>(0.0), vec2<f32>(1.0)), s);
			if (z0 > 0.0) {
				let pn = vec3<f32>(dn.x / dn.z * z0, dn.y / dn.z * z0, z0);
				let ps = vec3<f32>(dot(vec3<f32>(nf[1].x, nf[2].x, nf[3].x), pn), dot(vec3<f32>(nf[1].y, nf[2].y, nf[3].y), pn), dot(vec3<f32>(nf[1].z, nf[2].z, nf[3].z), pn)) + nf[5].xyz;
				if (ps.z > 0.05) {
					let e = clamp(ps.xy / ps.z - ds.xy, vec2<f32>(-0.5), vec2<f32>(0.5));
					let d2 = vec3<f32>(ds.xy - e, 1.0);
					dn = vec3<f32>(dot(nf[1].xyz, d2), dot(nf[2].xyz, d2), dot(nf[3].xyz, d2));
				}
			}
		}
		if (dn.z > 0.05) {
			let u = vec2<f32>(dn.x / dn.z / nf[4].x + 0.5, dn.y / dn.z / nf[4].y + 0.5);
			let inside = u.x >= 0.0 && u.x <= 1.0 && u.y >= 0.0 && u.y <= 1.0;
			let fw = fx_depth(clamp(u, vec2<f32>(0.0), vec2<f32>(1.0)), s);
			if (inside && fw > 0.0) {
				c = fx_cubic(u);
				fd = fw;
			} else if (fd > 0.0) {
				c = vec4<f32>(0.0, 0.0, 0.0, 1.0);
				fd = 0.0;
			}
		}
	}
	// [b46 fix round] A backdrop texel (the flight's cosmos, depth >= SPACE_MIN) is the picture's lowest layer: the backdrop draw (first in
	// the level pass, under the whole scene: the far plane's depth) takes only those, this draw (with the HUD) everything else.
	if ((fd >= SPACE_MIN) != BACKDROP) {
		discard;
	}
	var d = 0.0;
	if (BACKDROP) {
		d = 1.0;
	} else if (fd > 0.0) {
		// In front of its own depth by 1.2 % + 0.15 m: the GS and this renderer round the same surface differently, and a glow, lamp
		// halo or flame lying flush on a wall passed the test pixel by pixel (scattered red pixels, stair-step cut-outs; round 3 #6, #7).
		let z = max(fd / 1.012 - 0.15, nf[0].x);
		d = clamp(nf[0].y * (z - nf[0].x) / (z * (nf[0].y - nf[0].x)), 0.0, 1.0);
	}
	return F(c, d);
}
";

fn shader_src(backdrop: bool) -> String {
	format!("const SPACE_MIN: f32 = {SPACE_MIN:?};\nconst BACKDROP: bool = {backdrop};\n{HEAD}{FX_DEPTH}{FX_CUBIC}{TAIL}")
}

pub(in crate::viewer) struct OvDepth {
	pipeline: wgpu::RenderPipeline,
	/// [b46 fix round] The same draw for the backdrop texels only (shader const BACKDROP), first in the level pass.
	backdrop: wgpu::RenderPipeline,
	/// This frame's depth holds backdrop texels (`set_retail_overlay_depth`).
	space: bool,
	layout: wgpu::BindGroupLayout,
	/// (near, far, 0, 0) of this frame's projection.
	ubuf: wgpu::Buffer,
	/// The depth texture (width, rows).
	tex: Option<(u32, u32, wgpu::Texture)>,
	/// Group 1 of the overlay draw (`tex`, `ubuf`).
	bind: Option<wgpu::BindGroup>,
	/// The overlay of this frame has a depth (else the plain overlay draw).
	on: bool,
	/// The warp of the effect texels (`set_retail_overlay_warp`): matrix rows and the tangents, none = the overlay as it is.
	warp: Option<[f32; 20]>,
}

impl OvDepth {
	/// This frame's overlay has backdrop texels.
	pub(in crate::viewer) fn space(&self) -> bool {
		self.on && self.space
	}

	fn new(device: &wgpu::Device, format: wgpu::TextureFormat, depth_format: wgpu::TextureFormat, tex_layout: &wgpu::BindGroupLayout) -> OvDepth {
		let entry = |binding, ty| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT, ty, count: None };
		let plain = wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false };
		let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
			label: Some("overlay depth"),
			entries: &[entry(0, plain), entry(1, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None })],
		});
		let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("overlay depth"), bind_group_layouts: &[Some(tex_layout), Some(&layout)], immediate_size: 0 });
		// As hud_pipeline[3]: out = Cs + As * Cd.
		let blend = wgpu::BlendState {
			color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::SrcAlpha, operation: wgpu::BlendOperation::Add },
			alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
		};
		let pipe = |backdrop: bool| { let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("overlay depth"), source: wgpu::ShaderSource::Wgsl(shader_src(backdrop).into()) }); device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
			label: Some("overlay depth"),
			layout: Some(&pl),
			vertex: wgpu::VertexState {
				module: &module,
				entry_point: Some("vs"),
				compilation_options: Default::default(),
				buffers: &[Some(wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2] })],
			},
			fragment: Some(wgpu::FragmentState { module: &module, entry_point: Some("fs"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format, blend: Some(blend), write_mask: wgpu::ColorWrites::ALL })] }),
			primitive: wgpu::PrimitiveState::default(),
			depth_stencil: Some(wgpu::DepthStencilState { format: depth_format, depth_write_enabled: Some(false), depth_compare: Some(wgpu::CompareFunction::LessEqual), stencil: Default::default(), bias: Default::default() }),
			multisample: wgpu::MultisampleState { count: msaa(), ..Default::default() },
			multiview_mask: None,
			cache: None,
		}) };
		let (pipeline, backdrop) = (pipe(false), pipe(true));
		let ubuf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("overlay depth range"), size: 96, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
		OvDepth { pipeline, backdrop, space: false, layout, ubuf, tex: None, bind: None, on: false, warp: None }
	}
}

fn r32(device: &wgpu::Device, w: u32, rows: u32) -> wgpu::Texture {
	device.create_texture(&wgpu::TextureDescriptor {
		label: Some("overlay depth"),
		size: wgpu::Extent3d { width: w, height: rows, depth_or_array_layers: 1 },
		mip_level_count: 1,
		sample_count: 1,
		dimension: wgpu::TextureDimension::D2,
		format: wgpu::TextureFormat::R32Float,
		usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
		view_formats: &[],
	})
}

impl Gpu {
	/// The effect depth of the overlay `set_retail_overlay` set (`w` x `rows`, view depth per pixel, 0 = HUD), or None: the
	/// overlay is drawn over everything.
	pub fn set_retail_overlay_depth(&mut self, w: u32, rows: u32, depth: Option<&[f32]>) {
		let Some(d) = depth.filter(|d| d.len() == (w * rows) as usize) else {
			if let Some(o) = &mut self.ov_depth {
				(o.on, o.space) = (false, false);
			}
			return;
		};
		let o = self.ov_depth.get_or_insert_with(|| OvDepth::new(&self.device, self.config.format, self.depth_format, &self.tex_layout));
		if !o.tex.as_ref().is_some_and(|t| t.0 == w && t.1 == rows) {
			o.tex = Some((w, rows, r32(&self.device, w, rows)));
			o.bind = None;
		}
		let t = &o.tex.as_ref().unwrap().2;
		if o.bind.is_none() {
			let view = t.create_view(&Default::default());
			let entries = [(0, wgpu::BindingResource::TextureView(&view)), (1, o.ubuf.as_entire_binding())];
			let entries = entries.map(|(binding, resource)| wgpu::BindGroupEntry { binding, resource });
			o.bind = Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("overlay depth"), layout: &o.layout, entries: &entries }));
		}
		self.queue.write_texture(t.as_image_copy(), bytemuck::cast_slice(d), wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(rows) }, wgpu::Extent3d { width: w, height: rows, depth_or_array_layers: 1 });
		(o.on, o.space) = (true, d.iter().any(|&z| z >= SPACE_MIN));
	}

	/// R&C2 frame smoothing: the effect texels of this frame's overlay are fetched along `warp` (20 floats: three matrix rows of four, then
	/// the picture's tangents kx, ky, then the eye shift in the shown view's axes; `rac2/ov_warp.rs`), or not at all with None. Call every frame after `set_retail_overlay_depth`.
	pub fn set_retail_overlay_warp(&mut self, warp: Option<[f32; 20]>) {
		if let Some(o) = &mut self.ov_depth {
			o.warp = warp;
		}
	}

	/// The overlay quad's draw with the effect depth (pass_hud, `RETAIL_OV_TEX`): false when this frame's overlay has none. Leaves
	/// bind group 0 as the HUD draws had it.
	pub(super) fn draw_overlay_depth(&self, pass: &mut wgpu::RenderPass<'_>, ov: &wgpu::BindGroup, range: std::ops::Range<u32>) -> bool {
		self.draw_overlay_part(pass, ov, range, false)
	}

	/// [b46 fix round] The overlay quad's backdrop texels (R&C2's flight: the cosmos the game draws itself, opaque over the whole screen),
	/// first in the level pass at the far plane's depth: every native draw (the ship, its see-through flame and glass, which write no
	/// depth) lands on top. False: this frame has none.
	pub(super) fn draw_overlay_backdrop(&self, pass: &mut wgpu::RenderPass<'_>, ov: &wgpu::BindGroup, range: std::ops::Range<u32>) -> bool {
		self.ov_depth.as_ref().is_some_and(|o| o.space) && self.draw_overlay_part(pass, ov, range, true)
	}

	fn draw_overlay_part(&self, pass: &mut wgpu::RenderPass<'_>, ov: &wgpu::BindGroup, range: std::ops::Range<u32>, backdrop: bool) -> bool {
		let Some((o, b)) = self.ov_depth.as_ref().filter(|o| o.on).and_then(|o| Some((o, o.bind.as_ref()?))) else { return false };
		let mut u = [0.0f32; 24];
		u[0] = RETAIL_NEAR;
		u[1] = self.world_far();
		if let Some(w) = o.warp {
			u[2] = 1.0;
			u[4..24].copy_from_slice(&w);
		}
		self.queue.write_buffer(&o.ubuf, 0, bytemuck::cast_slice(&u));
		pass.set_pipeline(if backdrop { &o.backdrop } else { &o.pipeline });
		pass.set_bind_group(0, ov, &[]);
		pass.set_bind_group(1, b, &[]);
		pass.draw_indexed(range, 0, 0..1);
		pass.set_bind_group(0, &self.ubind, &[]);
		true
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn validate(name: &str, src: &str) {
		let m = naga::front::wgsl::parse_str(src).unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(src)));
		let mut v = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all());
		v.validate(&m).unwrap_or_else(|e| panic!("{name} validates: {e:?}"));
	}

	/// The shader compiles and validates (naga).
	#[test]
	fn shaders_validate() {
		validate("overlay depth", &shader_src(false));
		validate("overlay backdrop", &shader_src(true));
	}

	fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
		let mut desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
		desc.backends = wgpu::Backends::VULKAN;
		let inst = wgpu::Instance::new(desc);
		let ad = pollster::block_on(inst.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
		pollster::block_on(ad.request_device(&wgpu::DeviceDescriptor::default())).ok()
	}

	/// Runs the real `FX_DEPTH` text in a one-thread compute shader over a `w` x `h` effect-depth texture at `uv`.
	fn run_fx_depth(dev: &(wgpu::Device, wgpu::Queue), texels: &[f32], (w, h): (u32, u32), uv: [f32; 2]) -> f32 {
		let (d, q) = dev;
		let src = format!(
			"@group(0) @binding(0) var fxd: texture_2d<f32>;\n@group(0) @binding(1) var<storage, read_write> outv: array<f32>;\n{FX_DEPTH}\n@compute @workgroup_size(1) fn main() {{ let s = vec2<f32>(textureDimensions(fxd)); outv[0] = fx_depth(vec2<f32>({:?}, {:?}), s); }}\n",
			uv[0], uv[1]
		);
		validate("fx_depth harness", &src);
		let tex = r32(d, w, h);
		q.write_texture(tex.as_image_copy(), bytemuck::cast_slice(texels), wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) }, wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
		let view = tex.create_view(&Default::default());
		let buf = |usage| d.create_buffer(&wgpu::BufferDescriptor { label: None, size: 16, usage, mapped_at_creation: false });
		let out = buf(wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC);
		let stage = buf(wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ);
		let entry = |binding, ty| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::COMPUTE, ty, count: None };
		let bgl = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
			label: None,
			entries: &[
				entry(0, wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }),
				entry(1, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: false }, has_dynamic_offset: false, min_binding_size: None }),
			],
		});
		let bg = d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &bgl, entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }, wgpu::BindGroupEntry { binding: 1, resource: out.as_entire_binding() }] });
		let module = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(src.into()) });
		let pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&bgl)], immediate_size: 0 });
		let pipe = d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: None, layout: Some(&pl), module: &module, entry_point: Some("main"), compilation_options: Default::default(), cache: None });
		let mut enc = d.create_command_encoder(&Default::default());
		{
			let mut pass = enc.begin_compute_pass(&Default::default());
			pass.set_pipeline(&pipe);
			pass.set_bind_group(0, &bg, &[]);
			pass.dispatch_workgroups(1, 1, 1);
		}
		enc.copy_buffer_to_buffer(&out, 0, &stage, 0, 16);
		q.submit([enc.finish()]);
		stage.slice(..).map_async(wgpu::MapMode::Read, |r| r.unwrap());
		d.poll(wgpu::PollType::wait_indefinitely()).unwrap();
		let v = f32::from_le_bytes(stage.slice(..).get_mapped_range().unwrap()[0..4].try_into().unwrap());
		stage.unmap();
		v
	}

	/// The uv whose bilinear footprint starts at texel (0, 0) with fractions `fx`, `fy` in a `w` x `h` texture.
	fn uv_at(fx: f32, fy: f32, (w, h): (u32, u32)) -> [f32; 2] {
		[(fx + 0.5) / w as f32, (fy + 0.5) / h as f32]
	}

	/// Review B3/4 R4. Texels A (0,0) fire at 200 m, B (1,0) fire at 300 m, C (0,1) a glow at 5 m, D (1,1) empty. At fx 0.5, fy 0.01
	/// the weights are A 0.495, B 0.495, C 0.005: the 200 m / 300 m fire differ by more than the edge limit, so the pixel takes the
	/// nearest depth. It must be A's 200 m (the fire), not C's 5 m (a glow adding 0.5 % of the colour): at 5 m the pixel would pass the
	/// depth test against a 24 m wall and show 99 % far fire through it.
	#[test]
	fn a_faint_near_texel_does_not_pull_the_edge_depth_in_front_of_a_wall() {
		let Some(dev) = device() else {
			eprintln!("no Vulkan adapter: skipped");
			return;
		};
		let size = (2, 2);
		let z = run_fx_depth(&dev, &[200.0, 300.0, 5.0, 0.0], size, uv_at(0.5, 0.01, size));
		assert!((z - 200.0).abs() < 1e-3, "the edge depth is the nearest texel that adds colour: {z}");
		// The overlay fragment's depth is its depth / 1.012 - 0.15 against the wall at 24 m: the fire is behind it, hidden.
		let wall = 24.0;
		assert!(z / 1.012 - 0.15 > wall, "the fire is hidden by the wall");
		// The near glow is not dropped when it does add colour (weight 0.2): the nearest of the weighted texels is the glow.
		let z = run_fx_depth(&dev, &[200.0, 300.0, 5.0, 0.0], size, uv_at(0.5, 0.2, size));
		assert!((z - 5.0).abs() < 1e-3, "a texel that adds colour still counts: {z}");
		// Only a faint effect texel in the footprint (the pixel is almost all empty): the all-texel fallback keeps its depth.
		let z = run_fx_depth(&dev, &[0.0, 0.0, 5.0, 0.0], size, uv_at(0.5, 0.01, size));
		assert!((z - 5.0).abs() < 1e-3, "fallback to the faint texel: {z}");
		// No effect at all: 0 (HUD).
		assert_eq!(run_fx_depth(&dev, &[0.0; 4], size, uv_at(0.3, 0.3, size)), 0.0);
	}

	/// The smooth case is unchanged: two effect texels of one surface (10 m, 10.5 m) interpolate 1/z by their weights.
	#[test]
	fn texels_of_one_surface_still_interpolate_smoothly() {
		let Some(dev) = device() else {
			eprintln!("no Vulkan adapter: skipped");
			return;
		};
		let size = (2, 2);
		let z = run_fx_depth(&dev, &[10.0, 10.5, 10.0, 10.5], size, uv_at(0.5, 0.5, size));
		let want = 1.0 / (0.5 / 10.0 + 0.5 / 10.5);
		assert!((z - want).abs() < 1e-3, "{z} vs {want}");
	}
}
