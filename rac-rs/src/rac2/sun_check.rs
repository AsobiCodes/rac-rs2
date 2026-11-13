//! [PC] Parity wave 1 data checks on the disc (ignored tests; RAC2_ISO = the R&C2 PAL disc). Nothing here is drawn.
//! - `key_lights`: rows 10 + 12 (sun shadows, sun rays): every level's light 0 as the engine reads it (engine/viewer/sunshadow.rs
//!   `key_light` + `strength`: the brighter of the pair, travel direction = `DirLight.dir`, shadows only from above the horizon and
//!   for a light brighter than luma 0.1). Writes `runs/w1/ident/key_lights.txt`.
//! - same test: rows 29a + 28 (fur, metal) identity dumps for wave 3: per level every texture's key, size, the metal look's hash
//!   (`metal_texture_hash`) and the fur hash (`furspec::texhash`), armor textures (keys from `ARMOR_TEXTURE_BASE`) marked.
//!   Writes `runs/w1/ident/textures_LL.tsv`.

use rac_formats::gameplay::DirLight;
use std::fmt::Write as _;

/// The output folder: `<lane>/runs/w1/ident` (this crate's manifest is `<lane>/rac-rs/crates/rac2`).
fn out_dir() -> std::path::PathBuf {
	std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../runs/w1/ident"))
}

/// One light pair as the engine's sun reads it: (travel direction, luma) of the brighter half, None without a direction.
fn key_of(l: &DirLight) -> Option<([f32; 3], f32)> {
	let luma = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
	(0..2)
		.filter(|&k| l.dir[k].iter().map(|x| x * x).sum::<f32>() > 1e-6)
		.map(|k| {
			let d = l.dir[k];
			let n = d.iter().map(|x| x * x).sum::<f32>().sqrt();
			(d.map(|x| x / n), luma(l.colour[k]))
		})
		.max_by(|a, b| a.1.total_cmp(&b.1))
}

/// The engine's shadow strength rule without its constant (0 = no shadows, 1 = full).
fn strength(dir: [f32; 3], luma: f32) -> f32 {
	((luma - 0.1) / 0.4).clamp(0.0, 1.0) * ((-dir[2]).clamp(0.0, 1.0) / 0.15).min(1.0)
}

#[test]
fn key_light_rule_matches_the_engine() {
	let lit = DirLight { colour: [[1.0; 3], [0.2; 3]], dir: [[0.0, 0.6, -0.8], [0.0, 0.0, 1.0]], wrap: [0.0; 2] };
	let (d, l) = key_of(&lit).unwrap();
	assert!((d[2] + 0.8).abs() < 1e-5 && l > 0.9);
	assert!(strength(d, l) > 0.99, "a bright light from above casts");
	assert_eq!(strength([0.0, 0.0, 1.0], 1.0), 0.0, "from below: none");
	assert_eq!(strength([0.0, 0.0, -1.0], 0.05), 0.0, "dim: none");
}

#[test]
#[ignore = "reads the R&C2 disc (RAC2_ISO); writes runs/w1/ident"]
fn key_lights_and_identity_dumps() {
	let iso = std::env::var("RAC2_ISO").expect("RAC2_ISO");
	let dir = out_dir();
	std::fs::create_dir_all(&dir).unwrap();
	let mut sum = String::from("level\tlights\tkey_dir(x,y,z)\tluma\tstrength\tverdict\n");
	for id in 0..32usize {
		let Ok(l) = rac_formats::rc2::level::load_level(&iso, id) else { continue };
		let lights = rac_formats::rc2::gameplay::lights(&l.gameplay).unwrap_or_default();
		let (kd, kl, ks) = match lights.first().and_then(key_of) {
			Some((d, lu)) => (format!("{:.3},{:.3},{:.3}", d[0], d[1], d[2]), format!("{lu:.3}"), strength(d, lu)),
			None => ("-".into(), "-".into(), 0.0),
		};
		let verdict = if ks >= 0.5 { "sun" } else if ks > 0.0 { "weak" } else { "none" };
		let _ = writeln!(sum, "{id}\t{}\t{kd}\t{kl}\t{ks:.3}\t{verdict}", lights.len());
		let Ok(w) = crate::world::World::load(&iso, id, None) else { continue };
		let mut keys: Vec<_> = w.textures.keys().copied().collect();
		keys.sort_unstable();
		let mut t = String::from("key\tw\th\tmetal_hash\tfur_hash\tarmor\n");
		for k in keys {
			let x = &w.textures[&k];
			let armor = k >> 16 == 1 && k & 0xffff >= u32::from(rac_formats::rc2::armor::ARMOR_TEXTURE_BASE);
			let _ = writeln!(t, "{k:08x}\t{}\t{}\t{:016x}\t{:016x}\t{}", x.w, x.h, rac_engine::viewer::metal_texture_hash(x), rac_engine::furspec::texhash(x.w, x.h, &x.rgba), armor as u8);
		}
		std::fs::write(dir.join(format!("textures_{id:02}.tsv")), t).unwrap();
	}
	std::fs::write(dir.join("key_lights.txt"), &sum).unwrap();
	eprint!("{sum}");
}
