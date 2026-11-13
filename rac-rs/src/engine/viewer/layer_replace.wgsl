// [PC] R&C2's replace-blend world layers (gpu/layer_fx.rs `pass_layer_replace`; GS ALPHA 0x88, the level-14 lava sheet) with the
// prim's GS alpha test (gsfull/layer_tris.rs `AlphaTest`): the vertex normal's x carries ATST | AREF << 3 (`AlphaTest::word`; the
// layers have no normal), handed flat to the fragment in `metal` (unused by these draws). fs_layer_pass keeps the pixels that pass,
// fs_layer_fail those that fail (drawn with what AFAIL writes: the colour without depth, or the depth alone).
@vertex fn vs_layer(@location(0) p: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) n: vec3<f32>, @location(3) c: vec4<f32>, @location(4) l: vec4<f32>) -> VOut {
	var o = vs_main(p, uv, vec3<f32>(0.0), c, l);
	o.metal = u32(n.x);
	return o;
}

// The GS's As of a modulated layer texel: (At * Av) >> 7, clamped to 255. The texel alpha arrives doubled (texreq.rs: 0x80 and up read
// 255), the vertex alpha as Av / 128 (rac2/host/ov_layers.rs; a decal prim has 1.0, so As = At).
fn layer_as(t: f32, v: f32) -> f32 {
	return min(floor(floor(t * 127.5 + 0.5) * round(v * 128.0) / 128.0), 255.0);
}

// GS TEST ATST (w & 7) of As against AREF (w >> 3): NEVER, ALWAYS, LESS, LEQUAL, EQUAL, GEQUAL, GREATER, NOTEQUAL.
fn layer_alpha(a: f32, w: u32) -> bool {
	let r = f32((w >> 3u) & 255u);
	switch (w & 7u) {
		case 0u: { return false; }
		case 1u: { return true; }
		case 2u: { return a < r; }
		case 3u: { return a <= r; }
		case 4u: { return a == r; }
		case 5u: { return a >= r; }
		case 6u: { return a > r; }
		default: { return a != r; }
	}
}

@fragment fn fs_layer_pass(i: VOut) -> @location(0) vec4<f32> {
	let t = textureSample(tex, samp, i.uv);
	let c = t * i.c;
	if (!layer_alpha(layer_as(t.a, i.c.a), i.metal)) {
		discard;
	}
	return vec4<f32>(gs_fog(i, c.rgb), c.a);
}

@fragment fn fs_layer_fail(i: VOut) -> @location(0) vec4<f32> {
	let t = textureSample(tex, samp, i.uv);
	let c = t * i.c;
	if (layer_alpha(layer_as(t.a, i.c.a), i.metal)) {
		discard;
	}
	return vec4<f32>(gs_fog(i, c.rgb), c.a);
}
