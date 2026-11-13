
// [PC] Particles Enhanced, R&C2's no-depth effect prims (partfx.rs `over`, gpu/layer_fx.rs `no_depth`): `fs_part_soft` /
// `fs_part_soft_add` without the soft fade, as the game draws them over the world without the depth test.
@fragment fn fs_part_over(i: VOut) -> SoftOut {
	let c = textureSample(tex, samp, i.uv) * i.c;
	var o: SoftOut;
	o.c = c;
	o.glow = vec4<f32>(0.0, 0.0, 0.0, c.a);
	return o;
}

@fragment fn fs_part_over_add(i: VOut) -> SoftOut {
	let c = textureSample(tex, samp, i.uv) * i.c;
	var o: SoftOut;
	o.c = c;
	o.glow = vec4<f32>(c.rgb * c.a * soft.p.w, max(c.r, max(c.g, c.b)) * c.a);
	return o;
}
