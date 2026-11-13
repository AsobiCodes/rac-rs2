//! [PC] R&C2's Enhanced ice (`Looks::ice_keys`, parity row 21; R&C1: rac1/planet_looks.rs `ice_keys`, Hoven). The engine's ice look
//! (engine/viewer/icefx.rs: a cold gloss following the camera, a fresnel rim, light through thin edges, sparkles; only added
//! light, no retail colour change; Lighting Enhanced only) runs on the tie and shrub textures listed here per level.
//!
//! Dev aid: RAC2_ICE_KEYS="t:2002f,20031;s:30008" replaces the lists of every level (hex bare keys: tie 0x2xxxx, shrub 0x3xxxx),
//! for checking a candidate key in a picture without a rebuild (ENV.md).

use std::sync::OnceLock;

/// The level's ice textures: (tie keys, shrub keys) as bare texture keys (`rac_formats::level::texture_key`, no K / wrap bits).
pub fn ice_keys(level: usize) -> (&'static [u32], &'static [u32]) {
	if let Some(o) = env_keys() {
		return o;
	}
	match level {
		SIBERIUS => (SIBERIUS_TIES, SIBERIUS_SHRUBS),
		GRELBIN => (GRELBIN_TIES, GRELBIN_SHRUBS),
		_ => (&[], &[]),
	}
}

const SIBERIUS: usize = 7;
const GRELBIN: usize = 19;

// The key lists are filled from the texture sheets + ON/OFF pictures (see the notes on each list).
/// Siberius: no tie key lands on ice [C: fg1 0310 field 59 (after the moby table fix in addrs.rs; before it 0310/0311/0313 were
/// white), "t:20003,20004,2000b,20054,20052,20009,20057" = 115 px at thr 4 = noise (122)].
const SIBERIUS_TIES: &[u32] = &[];
/// Siberius shrubs [C: fg1 0310 field 59, lane gfxc runs/out ms1_/ms2_0310]: 0x30004, 0x3000a, 0x3000e = the ice crystal pillars'
/// sides and the frost twigs (21212 px); 0x30005, 0x30002, 0x30001, 0x3000d = the crystal tops and the icy snow banks round their
/// bases (47589 px). Every marked pixel of both groups is ice or snow; which key of a group marks which part is not split [I].
const SIBERIUS_SHRUBS: &[u32] = &[0x30001, 0x30002, 0x30004, 0x30005, 0x3000a, 0x3000d, 0x3000e];
/// Grelbin: no tie key lands on ice [C: travel run RAC2_LEVEL=19 fields 2750..4499, candidate set "t:20006,20007,2000a,2000b,20009,
/// 20004,20005,20036,20021,20008,20029,20022" marks nothing beyond what shrub 0x30005 alone marks (lane gfxc runs/out mgSA_*: 252..666
/// px at thr 4, run noise ~250)]; the ice blocks at the tower base and the crystal shards are not tie/shrub textures of the list.
const GRELBIN_TIES: &[u32] = &[];
/// Shrub 0x30005 = the light-blue cracked ice lumps on the snow plain [C: runs/out mgS_03750 6443 px, mgS_02750 4408 px; looks
/// glossier with the ice look on]. Candidates 0x30004, 0x3000d, 0x3000c add nothing in view.
const GRELBIN_SHRUBS: &[u32] = &[0x30005];

/// RAC2_ICE_KEYS parsed once (None: unset or empty).
fn env_keys() -> Option<(&'static [u32], &'static [u32])> {
	static V: OnceLock<Option<(&'static [u32], &'static [u32])>> = OnceLock::new();
	*V.get_or_init(|| {
		let v = std::env::var("RAC2_ICE_KEYS").ok().filter(|v| !v.trim().is_empty())?;
		let (t, s) = parse_keys(&v);
		Some((&*Box::leak(t.into_boxed_slice()), &*Box::leak(s.into_boxed_slice())))
	})
}

/// "t:2002f,20031;s:30008" -> (ties, shrubs); unknown parts are skipped.
fn parse_keys(v: &str) -> (Vec<u32>, Vec<u32>) {
	let (mut t, mut s) = (Vec::new(), Vec::new());
	for part in v.split(';') {
		let Some((kind, list)) = part.split_once(':') else { continue };
		let keys = list.split(',').filter_map(|k| u32::from_str_radix(k.trim().trim_start_matches("0x"), 16).ok());
		match kind.trim() {
			"t" => t.extend(keys),
			"s" => s.extend(keys),
			_ => {}
		}
	}
	(t, s)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn ice_keys_parse_and_levels() {
		assert_eq!(parse_keys("t:2002f,0x20031;s:30008;x:1"), (vec![0x2002f, 0x20031], vec![0x30008]));
		assert_eq!(parse_keys(""), (vec![], vec![]));
		for lv in (0..32).filter(|&l| l != SIBERIUS && l != GRELBIN) {
			assert_eq!(ice_keys(lv), (&[][..], &[][..]), "level {lv} has no ice");
		}
		for (t, s) in [(SIBERIUS_TIES, SIBERIUS_SHRUBS), (GRELBIN_TIES, GRELBIN_SHRUBS)] {
			assert!(t.iter().all(|k| k >> 16 == 2) && s.iter().all(|k| k >> 16 == 3), "bare tie / shrub keys");
		}
	}

	/// Bare key as icefx.rs `bare` (tie / shrub material bits dropped).
	fn bare(key: u32) -> u32 {
		use rac_formats::{mobies::KEY_MASK, shrubs};
		if shrubs::key_k(key).is_some() { key & KEY_MASK & !shrubs::MATERIAL_BITS } else { key & KEY_MASK }
	}

	/// Texture sheet input: every tie / shrub texture of the icy levels with its face count (`runs/ice/<lv>_<key>_<faces>_WxH.rgba`).
	#[test]
	#[ignore = "reads the R&C2 disc (RAC2_ISO); writes runs/ice/*.rgba"]
	fn ice_texture_dump() {
		let iso = std::env::var("RAC2_ISO").expect("RAC2_ISO");
		let dir = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../runs/ice"));
		std::fs::create_dir_all(&dir).unwrap();
		for id in [SIBERIUS, GRELBIN] {
			let w = crate::world::World::load(&iso, id, None).expect("level");
			let mut faces: std::collections::HashMap<u32, usize> = Default::default();
			for m in w.ties.iter().map(|t| &t.lods[0]).chain(w.shrubs.iter().map(|s| &s.mesh)) {
				for (k, _) in &m.faces {
					*faces.entry(bare(*k)).or_default() += 1;
				}
			}
			for (k, t) in w.textures.iter().filter(|(k, _)| matches!(**k >> 16, 2 | 3)) {
				let px: Vec<u8> = t.rgba.iter().flatten().copied().collect();
				let n = faces.get(k).copied().unwrap_or(0);
				std::fs::write(dir.join(format!("{id:02}_{k:05x}_{n}_{}x{}.rgba", t.w, t.h)), px).unwrap();
			}
		}
	}
}
