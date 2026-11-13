//! [PC] Parity wave 2A (sun shadows): which world geometry stands between a point on the ground and the sun (ignored test;
//! RAC2_ISO = the R&C2 PAL disc; RAC2_RAY="level x y z dx dy dz" = a point and the light's travel direction as RAC_SUN_LOG prints
//! it). Casts rays from a grid around the point towards the light and lists the first terrain mesh / tie / shrub each ray hits.
//! Nothing here is drawn.

use rac_formats::mesh::Mesh;

/// Distance along `d` from `o` to triangle `abc` (Moller-Trumbore, both faces), None without a hit in front.
fn hit(o: [f32; 3], d: [f32; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> Option<(f32, f32, f32)> {
	let sub = |x: [f32; 3], y: [f32; 3]| [x[0] - y[0], x[1] - y[1], x[2] - y[2]];
	let cross = |x: [f32; 3], y: [f32; 3]| [x[1] * y[2] - x[2] * y[1], x[2] * y[0] - x[0] * y[2], x[0] * y[1] - x[1] * y[0]];
	let dot = |x: [f32; 3], y: [f32; 3]| x[0] * y[0] + x[1] * y[1] + x[2] * y[2];
	let (e1, e2) = (sub(b, a), sub(c, a));
	let p = cross(d, e2);
	let det = dot(e1, p);
	if det.abs() < 1e-9 {
		return None;
	}
	let t = sub(o, a);
	let u = dot(t, p) / det;
	let q = cross(t, e1);
	let v = dot(d, q) / det;
	let s = dot(e2, q) / det;
	(u >= 0.0 && v >= 0.0 && u + v <= 1.0 && s > 0.05).then_some((s, u, v))
}

/// First hit of the ray in `m`: (distance, face index, barycentric u, v).
fn first(m: &Mesh, o: [f32; 3], d: [f32; 3]) -> Option<(f32, usize, f32, f32)> {
	let p = &m.positions;
	m.faces
		.iter()
		.enumerate()
		.filter_map(|(i, (_, f))| hit(o, d, p[f[0] as usize], p[f[1] as usize], p[f[2] as usize]).map(|(s, u, v)| (s, i, u, v)))
		.min_by(|a, b| a.0.total_cmp(&b.0))
}

/// Texel alpha at the hit (`textures` keyed by the face's texture word; 255 when the texture is unknown).
fn alpha(m: &Mesh, tex: &std::collections::HashMap<u32, rac_formats::level::Texture>, f: usize, u: f32, v: f32) -> u8 {
	let (t, ix) = m.faces[f];
	let Some(x) = tex.get(&t) else { return 255 };
	if m.uvs.len() != m.positions.len() {
		return 255;
	}
	let [a, b, c] = ix.map(|k| m.uvs[k as usize]);
	let w = 1.0 - u - v;
	let uv = [a[0] * w + b[0] * u + c[0] * v, a[1] * w + b[1] * u + c[1] * v];
	let px = ((uv[0].rem_euclid(1.0)) * x.w as f32) as usize % x.w;
	let py = ((uv[1].rem_euclid(1.0)) * x.h as f32) as usize % x.h;
	x.rgba[py * x.w + px][3]
}

fn bbox(m: &Mesh) -> ([f32; 3], [f32; 3]) {
	let mut lo = [f32::MAX; 3];
	let mut hi = [f32::MIN; 3];
	for v in &m.positions {
		for k in 0..3 {
			lo[k] = lo[k].min(v[k]);
			hi[k] = hi[k].max(v[k]);
		}
	}
	(lo, hi)
}

#[test]
#[ignore = "reads the R&C2 disc (RAC2_ISO, RAC2_RAY)"]
fn what_stands_between_the_ground_and_the_sun() {
	let iso = std::env::var("RAC2_ISO").expect("RAC2_ISO");
	let r: Vec<f32> = std::env::var("RAC2_RAY").expect("RAC2_RAY").split_whitespace().map(|x| x.parse().unwrap()).collect();
	let level = r[0] as usize;
	let w = crate::world::World::load(&iso, level, None).unwrap();
	let n = (r[4] * r[4] + r[5] * r[5] + r[6] * r[6]).sqrt();
	let to_sun = [-r[4] / n, -r[5] / n, -r[6] / n];
	eprintln!("level {level} terrain meshes {} ties {} shrubs {}", w.terrain.len(), w.ties.len(), w.shrubs.len());
	// A grid over the ground in front of the point; the ground height from a terrain ray straight down.
	let mut blockers: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
	let (mut pts, mut lit, mut cut_only) = (0, 0, 0);
	let step = r.get(7).copied().unwrap_or(5.0);
	for gy in 0..24 {
		for gx in -12..12 {
			let top = [r[1] + gx as f32 * step, r[2] + gy as f32 * step, r[3] + 60.0];
			let Some(g) = w.terrain.iter().filter_map(|m| first(m, top, [0.0, 0.0, -1.0]).map(|h| h.0)).min_by(|a, b| a.total_cmp(b)) else { continue };
			let o = [top[0], top[1], top[2] - g + 0.2];
			pts += 1;
			let mut hits: Vec<(f32, String, u8)> = Vec::new();
			for (i, m) in w.terrain.iter().enumerate() {
				if let Some((s, f, u, v)) = first(m, o, to_sun) {
					hits.push((s, format!("terrain {i}"), alpha(m, &w.textures, f, u, v)));
				}
			}
			for (i, t) in w.ties.iter().enumerate() {
				if let Some((s, f, u, v)) = first(&t.lods[0], o, to_sun) {
					let (lo, hi) = bbox(&t.lods[0]);
					hits.push((s, format!("tie {i} box {:?}..{:?}", lo.map(|x| x.round()), hi.map(|x| x.round())), alpha(&t.lods[0], &w.textures, f, u, v)));
				}
			}
			for (i, sh) in w.shrubs.iter().enumerate() {
				if let Some((s, f, u, v)) = first(&sh.mesh, o, to_sun) {
					hits.push((s, format!("shrub {i} class {}", sh.class_id), alpha(&sh.mesh, &w.textures, f, u, v)));
				}
			}
			hits.sort_by(|a, b| a.0.total_cmp(&b.0));
			// The first hit whose texel survives the tie cut (127) blocks the light.
			eprintln!("pt {} {} {} {}", o[0], o[1], o[2], if hits.iter().any(|h| h.2 >= 127) { 'B' } else { 'L' });
			match hits.iter().find(|h| h.2 >= 127) {
				Some(h) => {
					let e = blockers.entry(h.1.clone()).or_default();
					e.0 += 1;
					e.1 = e.1.max(h.0 as usize);
				}
				None if hits.is_empty() => lit += 1,
				None => cut_only += 1,
			}
		}
	}
	eprintln!("ground points {pts}: lit {lit}, behind see-through texels only {cut_only}, blocked {}", pts - lit - cut_only);
	let mut v: Vec<_> = blockers.into_iter().collect();
	v.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
	for (k, (n, d)) in v.iter().take(25) {
		eprintln!("  {n:3} points (up to {d} m away): {k}");
	}
}

/// Any blocker of the ray from `o` along `d` (unit) in the casters (mesh, bounding sphere): sphere test first, then the triangles.
fn blocked(casters: &[(&Mesh, [f32; 4])], o: [f32; 3], d: [f32; 3]) -> bool {
	casters.iter().any(|(m, s)| {
		let c = [s[0] - o[0], s[1] - o[1], s[2] - o[2]];
		let t = c[0] * d[0] + c[1] * d[1] + c[2] * d[2];
		let dd = c[0] * c[0] + c[1] * c[1] + c[2] * c[2] - t * t;
		(t > -s[3]) && dd <= s[3] * s[3] && first(m, o, d).is_some()
	})
}

fn sphere(m: &Mesh) -> [f32; 4] {
	let (lo, hi) = bbox(m);
	let c = [0, 1, 2].map(|k| (lo[k] + hi[k]) * 0.5);
	let r = [0, 1, 2].map(|k| (hi[k] - lo[k]) * 0.5);
	[c[0], c[1], c[2], (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt()]
}

/// Which light direction the level's painted (baked) terrain colours agree with: for terrain vertices within RAC2_RAY's radius of
/// its point, per candidate direction (and the level's own light rows) the mean baked luma of the vertices the ties / shrubs /
/// terrain block from that direction vs the ones they do not. A painted sun shows as a direction whose blocked vertices are clearly
/// darker; no such direction = the level paints no sun shadows.
#[test]
#[ignore = "reads the R&C2 disc (RAC2_ISO, RAC2_RAY=\"level x y z radius\")"]
fn which_sun_the_painted_colours_agree_with() {
	let iso = std::env::var("RAC2_ISO").expect("RAC2_ISO");
	let r: Vec<f32> = std::env::var("RAC2_RAY").expect("RAC2_RAY").split_whitespace().map(|x| x.parse().unwrap()).collect();
	let level = r[0] as usize;
	let rad = r.get(4).copied().unwrap_or(60.0);
	let w = crate::world::World::load(&iso, level, None).unwrap();
	let l = rac_formats::rc2::level::load_level(&iso, level).unwrap();
	let lights = rac_formats::rc2::gameplay::lights(&l.gameplay).unwrap_or_default();
	let near = |c: [f32; 3], s: f32| (c[0] - r[1]).hypot(c[1] - r[2]) < rad + s + 60.0;
	let mut casters: Vec<(&Mesh, [f32; 4])> = Vec::new();
	casters.extend(w.terrain.iter().map(|m| (m, sphere(m))).filter(|(_, s)| near([s[0], s[1], s[2]], s[3])));
	casters.extend(w.ties.iter().map(|t| (&t.lods[0], sphere(&t.lods[0]))).filter(|(_, s)| near([s[0], s[1], s[2]], s[3])));
	casters.extend(w.shrubs.iter().map(|t| (&t.mesh, sphere(&t.mesh))).filter(|(_, s)| near([s[0], s[1], s[2]], s[3])));
	let mut pts: Vec<([f32; 3], f32)> = Vec::new();
	for m in &w.terrain {
		for (k, p) in m.positions.iter().enumerate() {
			if (p[0] - r[1]).hypot(p[1] - r[2]) < rad && m.colours.len() == m.positions.len() {
				let c = m.colours[k];
				pts.push(([p[0], p[1], p[2] + 0.3], (0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]) / 128.0));
			}
		}
	}
	let stride = (pts.len() / 1500).max(1);
	let pts: Vec<_> = pts.into_iter().step_by(stride).collect();
	eprintln!("level {level}: {} terrain points, {} casters, lights {}", pts.len(), casters.len(), lights.len());
	let score = |to: [f32; 3]| {
		let n = (to[0] * to[0] + to[1] * to[1] + to[2] * to[2]).sqrt();
		let d = to.map(|x| x / n);
		let (mut nb, mut lb, mut nu, mut lu) = (0usize, 0.0f32, 0usize, 0.0f32);
		for (p, y) in &pts {
			if blocked(&casters, *p, d) {
				nb += 1;
				lb += y;
			} else {
				nu += 1;
				lu += y;
			}
		}
		let (mb, mu) = (lb / nb.max(1) as f32, lu / nu.max(1) as f32);
		(nb as f32 / pts.len().max(1) as f32, mb, mu, if nb > 10 && nu > 10 { (mu - mb) / mu.max(1e-3) } else { 0.0 })
	};
	let mut rows = Vec::new();
	for el in [25.0f32, 35.0, 45.0, 55.0, 65.0, 75.0, 89.0] {
		for az in (0..24).map(|k| k as f32 * 15.0) {
			let (e, a) = (el.to_radians(), az.to_radians());
			let to = [e.cos() * a.cos(), e.cos() * a.sin(), e.sin()];
			let (b, mb, mu, s) = score(to);
			rows.push((s, format!("el {el:2} az {az:3}: blocked {b:.2} luma blocked {mb:.3} open {mu:.3} contrast {s:.3}")));
		}
	}
	rows.sort_by(|a, b| b.0.total_cmp(&a.0));
	for (_, t) in rows.iter().take(8) {
		eprintln!("best  {t}");
	}
	for (_, t) in rows.iter().rev().take(3) {
		eprintln!("worst {t}");
	}
	for (i, li) in lights.iter().enumerate() {
		for k in 0..2 {
			let d = li.dir[k];
			if d.iter().map(|x| x * x).sum::<f32>() < 1e-6 {
				continue;
			}
			let to = d.map(|x| -x);
			let (b, mb, mu, s) = score(to);
			let el = (to[2] / (to[0] * to[0] + to[1] * to[1] + to[2] * to[2]).sqrt()).asin().to_degrees();
			let az = to[1].atan2(to[0]).to_degrees().rem_euclid(360.0);
			eprintln!("light {i}{} el {el:.0} az {az:.0}: blocked {b:.2} luma blocked {mb:.3} open {mu:.3} contrast {s:.3}", if k == 0 { 'a' } else { 'b' });
		}
	}
}

/// (blocked fraction, mean luma blocked, mean luma open, contrast (open - blocked) / open) of `pts` towards `to`.
fn agree(casters: &[(&Mesh, [f32; 4])], pts: &[([f32; 3], f32)], to: [f32; 3]) -> (f32, f32, f32, f32) {
	let n = (to[0] * to[0] + to[1] * to[1] + to[2] * to[2]).sqrt();
	let d = to.map(|x| x / n);
	let (mut nb, mut lb, mut nu, mut lu) = (0usize, 0.0f32, 0usize, 0.0f32);
	for (p, y) in pts {
		if blocked(casters, *p, d) {
			nb += 1;
			lb += y;
		} else {
			nu += 1;
			lu += y;
		}
	}
	let (mb, mu) = (lb / nb.max(1) as f32, lu / nu.max(1) as f32);
	(nb as f32 / pts.len().max(1) as f32, mb, mu, if nb > 10 && nu > 10 { (mu - mb) / mu.max(1e-3) } else { 0.0 })
}

/// Every level: does its painted terrain agree with the key light the engine's sun takes (light 0's brighter half, raised to 35
/// degrees as `sunshadow::raise` does)? Per level: the key light's blocked fraction and contrast, the straight-up contrast (what
/// any overhead cover paints: the baseline) and the best of 48 directions. Writes runs/w2a/painted_sun.txt.
#[test]
#[ignore = "reads the R&C2 disc (RAC2_ISO); writes runs/w2a/painted_sun.txt"]
fn painted_sun_per_level() {
	use std::fmt::Write as _;
	let iso = std::env::var("RAC2_ISO").expect("RAC2_ISO");
	let mut out = String::from("level\tpts\tkey_el\tkey_az\tkey_blocked\tkey_contrast\tup_contrast\tbest_el\tbest_az\tbest_blocked\tbest_contrast\n");
	for level in 0..27usize {
		let Ok(w) = crate::world::World::load(&iso, level, None) else { continue };
		let Ok(l) = rac_formats::rc2::level::load_level(&iso, level) else { continue };
		let lights = rac_formats::rc2::gameplay::lights(&l.gameplay).unwrap_or_default();
		let luma = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
		let key = lights.first().and_then(|li| (0..2).filter(|&k| li.dir[k].iter().map(|x| x * x).sum::<f32>() > 1e-6).max_by(|&a, &b| luma(li.colour[a]).total_cmp(&luma(li.colour[b]))).map(|k| li.dir[k]));
		let mut casters: Vec<(&Mesh, [f32; 4])> = Vec::new();
		casters.extend(w.terrain.iter().map(|m| (m, sphere(m))));
		casters.extend(w.ties.iter().map(|t| (&t.lods[0], sphere(&t.lods[0]))));
		casters.extend(w.shrubs.iter().map(|t| (&t.mesh, sphere(&t.mesh))));
		let mut pts: Vec<([f32; 3], f32)> = Vec::new();
		for m in w.terrain.iter().filter(|m| m.colours.len() == m.positions.len()) {
			for (p, c) in m.positions.iter().zip(&m.colours) {
				pts.push(([p[0], p[1], p[2] + 0.3], (0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]) / 128.0));
			}
		}
		let stride = (pts.len() / 1200).max(1);
		let pts: Vec<_> = pts.into_iter().step_by(stride).collect();
		let dir_of = |el: f32, az: f32| {
			let (e, a) = (el.to_radians(), az.to_radians());
			[e.cos() * a.cos(), e.cos() * a.sin(), e.sin()]
		};
		let (kel, kaz, kb, kc) = match key {
			Some(d) => {
				let to = d.map(|x| -x);
				let n = (to[0] * to[0] + to[1] * to[1] + to[2] * to[2]).sqrt();
				let el = (to[2] / n).asin().to_degrees().max(35.0);
				let az = to[1].atan2(to[0]).to_degrees().rem_euclid(360.0);
				let (b, _, _, c) = if to[2] > 0.0 { agree(&casters, &pts, dir_of(el, az)) } else { (0.0, 0.0, 0.0, 0.0) };
				(el, az, b, c)
			}
			None => (0.0, 0.0, 0.0, 0.0),
		};
		let (_, _, _, up) = agree(&casters, &pts, [0.0, 0.0, 1.0]);
		let mut best = (f32::MIN, 0.0, 0.0, 0.0);
		for el in [35.0f32, 50.0, 65.0, 80.0] {
			for k in 0..12 {
				let az = k as f32 * 30.0;
				let (b, _, _, c) = agree(&casters, &pts, dir_of(el, az));
				if c > best.0 {
					best = (c, el, az, b);
				}
			}
		}
		let _ = writeln!(out, "{level}\t{}\t{kel:.0}\t{kaz:.0}\t{kb:.2}\t{kc:.3}\t{up:.3}\t{:.0}\t{:.0}\t{:.2}\t{:.3}", pts.len(), best.1, best.2, best.3, best.0);
		eprint!("{}", out.lines().last().unwrap().to_string() + "\n");
	}
	let dir = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../runs/w2a"));
	std::fs::create_dir_all(&dir).unwrap();
	std::fs::write(dir.join("painted_sun.txt"), &out).unwrap();
}
