//! [PC] Parity wave 3 data checks on the disc (ignored tests; RAC2_ISO = the R&C2 PAL disc): the per-level facts the R&C2 looks
//! (rac2/planet_fog.rs) are read from. Nothing here is drawn. Writes `runs/w3/planet_facts.tsv`.

use std::fmt::Write as _;

fn out_dir() -> std::path::PathBuf {
	std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../runs/w3"))
}

/// Mean luma (0..255) of a texture's opaque-ish texels.
fn mean_luma(t: &rac_formats::level::Texture) -> f32 {
	let n = t.rgba.len().max(1) as f32;
	t.rgba.iter().map(|p| 0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32).sum::<f32>() / n
}

#[test]
#[ignore = "reads the R&C2 disc (RAC2_ISO); writes runs/w3/planet_facts.tsv"]
fn planet_facts() {
	let iso = std::env::var("RAC2_ISO").expect("RAC2_ISO");
	let dir = out_dir();
	std::fs::create_dir_all(dir.join("sky")).unwrap();
	let mut out = String::from("level\tfog_rgb\tnear\tfar\tgs_near\tgs_far\tweight\tbg\tsky_shells\tsky_untex\tsky_colour\tsky_tex_luma\tterrain_meshes\tterrain_verts\tterrain_z\tties\tshrubs\n");
	for id in 0..32usize {
		let Ok(l) = rac_formats::rc2::level::load_level(&iso, id) else { continue };
		let (rgb, fog) = rac_formats::rc2::gameplay::fog(&l.gameplay).unwrap_or(([0.0; 3], [0.0, 0.0, 255.0, 255.0]));
		let bg = rac_formats::rc2::gameplay::background(&l.gameplay).unwrap_or([0; 3]);
		let w = ((fog[2] - fog[3]) / 255.0).clamp(0.0, 1.0);
		let Ok(wo) = crate::world::World::load(&iso, id, None) else {
			let _ = writeln!(out, "{id}\tload failed");
			continue;
		};
		let (shells, untex, scol, sl) = match &wo.sky {
			Some(s) => {
				// The sky textures as raw RGBA (`sky_LL_KEY_WxH.rgba`) for runs/w3/sky_sheet.py.
				for (k, t) in &s.textures {
					let px: Vec<u8> = t.rgba.iter().flatten().copied().collect();
					std::fs::write(dir.join(format!("sky/sky_{id:02}_{k:08x}_{}x{}.rgba", t.w, t.h)), px).unwrap();
				}
				let lum: Vec<f32> = s.textures.values().map(mean_luma).collect();
				let sl = if lum.is_empty() { -1.0 } else { lum.iter().sum::<f32>() / lum.len() as f32 };
				(s.meshes.len(), s.untextured.iter().filter(|u| **u).count(), format!("{:?}", s.colour), sl)
			}
			None => (0, 0, "-".into(), -1.0),
		};
		let verts: usize = wo.terrain.iter().map(|m| m.positions.len()).sum();
		let (zl, zh) = wo.terrain.iter().flat_map(|m| m.positions.iter()).fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p[2]), a.1.max(p[2])));
		let _ = writeln!(
			out,
			"{id}\t{:?}\t{:.0}\t{:.0}\t{:.2}\t{:.2}\t{w:.3}\t{bg:?}\t{shells}\t{untex}\t{scol}\t{sl:.1}\t{}\t{verts}\t{zl:.0}..{zh:.0}\t{}\t{}",
			rgb,
			fog[0],
			fog[1],
			fog[2],
			fog[3],
			wo.terrain.len(),
			wo.ties.len(),
			wo.shrubs.len()
		);
	}
	std::fs::write(dir.join("planet_facts.tsv"), &out).unwrap();
	eprint!("{out}");
}

#[test]
#[ignore = "reads the R&C2 disc (RAC2_ISO); writes runs/w3/metal/*.rgba (the textures of rac2/metal_materials.rs)"]
fn metal_texture_dump() {
	let iso = std::env::var("RAC2_ISO").expect("RAC2_ISO");
	let dir = out_dir().join("metal");
	std::fs::create_dir_all(&dir).unwrap();
	let mut seen = std::collections::HashSet::new();
	for id in 0..32usize {
		let Ok(w) = crate::world::World::load(&iso, id, None) else { continue };
		for (k, t) in &w.textures {
			let h = rac_engine::viewer::metal_texture_hash(t);
			if crate::metal_materials::METAL_TEXTURES.iter().any(|r| r.0 == h) && seen.insert(h) {
				let px: Vec<u8> = t.rgba.iter().flatten().copied().collect();
				std::fs::write(dir.join(format!("m_{h:016x}_{id:02}_{k:08x}_{}x{}.rgba", t.w, t.h)), px).unwrap();
			}
		}
	}
	assert_eq!(seen.len(), crate::metal_materials::METAL_TEXTURES.len());
}
