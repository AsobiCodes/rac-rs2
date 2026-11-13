//! [PC] Fur on Ratchet for R&C2 (PARITY row 28; R&C1's Fur / Fur style rows, look and default): the engine's shell fur
//! (engine viewer/furfx.rs) drawn from Ratchet's prop slot (`Pack::fur_prop`, engine viewer/furfx/prop.rs). The hero is the level's
//! class HERO_CLASS (world.rs `level_classes`), whose engine class id is its core offset: noted per level, picked at `Stage::load`.
//! Body texture [C, scratch_rc2_par/runs/w1/ident textures_LL.tsv]: armor 0's 256x256 texture (key 0x100f0) has the fur hash below in
//! all 27 levels. Mask: R&C1's colour rule plus face rectangles read in the dumped texture (RAC_FUR_DUMP on Todano fg1 0549,
//! scratch_rc2_gfxb/runs/fur/dump/d0_256x256_184c373816d4613f.rgba; pictures runs/fur/tex, tools/furtex.py) [C]: the face sits where
//! R&C1's does (left half of the face top left, centre line x 115: brow x 82..111 y 11..25, eye socket x 84..109 y 26..56, nose
//! x 104..115 y 68..76, mouth line x 77..115 y 84..88), the eyeball at x 159..183 y 107..134. Not in R&C2's texture: R&C1's chest
//! emblem and harness hexagon plates (top left is green cloth and grey hardware, x 216..244 y 144..168 is pale fur), so no plate cuts.
//! Texel rectangles are [x0, y0, x1, y1).

use rac_engine::furspec::{FurMask, FurPart, FurRow, FurSpec, FurZone};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

/// Content hash (`furspec::texhash`) of Ratchet's armor-0 body texture.
pub const BODY_TEXTURE: u64 = 0x184c_3738_16d4_613f;

/// The dark brows (fur, though their colour reads as leather), as R&C1.
static ADD: [[u16; 4]; 1] = [[80, 4, 114, 24]];
/// The eyeball with its orange rim, a band along the mouth line (the lips' shells would cover the teeth), a margin around the nose.
static CUT: [[u16; 4]; 3] = [[156, 104, 184, 138], [76, 81, 116, 90], [101, 67, 116, 79]];
/// The face, R&C1's zones (same face layout): long cheek tufts, short skin-shaded muzzle and nose bridge, NO fur on the eye socket
/// (the shells would grow over the eye), short brows.
static ZONES: [FurZone; 5] = [
	FurZone { at: [20, 32, 78, 110], feather: 12.0, len: 1.45, halo: 1.4, tone: 0.85 },
	FurZone { at: [60, 50, 126, 106], feather: 9.0, len: 0.28, halo: 0.3, tone: 0.0 },
	FurZone { at: [100, 22, 132, 72], feather: 6.0, len: 0.3, halo: 0.3, tone: 0.2 },
	FurZone { at: [80, 20, 112, 60], feather: 5.0, len: 0.0, halo: 0.0, tone: 0.0 },
	FurZone { at: [78, 6, 116, 27], feather: 4.0, len: 0.4, halo: 0.3, tone: 0.5 },
];

/// One armor's body-texture row: R&C1's colour rule + face rectangles, `cut` = the face cuts (+ plate blocks, fur_armor.rs).
const fn row(hash: u64, cut: &'static [[u16; 4]]) -> FurRow {
	FurRow { hash, part: 0, mask: FurMask { hue: [20.0, 75.0], sat: 0.50, val: 0.06, pale: [45.0, 80.0, 0.20, 0.60], erode: 1, add: &ADD, cut, zones: &ZONES } }
}

/// [b46 W3] Armor body textures of G/ARMOR.WAD (item index: hash of the disc texture, b46/w3/armtex.py): 0 green, 1 blue, 2 orange,
/// 3 red, 5 black arms, 7 and 8 plain fur. Armor 4 (Carbonox, all metal) and 6 (purple costume: armor 0's layout with a painted
/// face, never seen worn in a game picture) have no row = no fur (see fur_armor.rs). [b47 X2] The worn armor id is RAM u8 0x1a7aa0
/// (notes_fur.md); the rows key on the body texture, so no id read is needed.
static ROWS: [FurRow; 7] = [
	row(BODY_TEXTURE, &CUT),
	row(0x18f2_0800_5f3a_66b1, &CUT),
	row(0xacd5_db83_808f_4c33, &crate::fur_armor::CUT_A2),
	row(0x2af1_e583_07e0_bbdd, &crate::fur_armor::CUT_A3),
	row(0xaa51_fbd4_7d6a_515f, &CUT),
	row(0x9efb_68a1_c95c_c863, &CUT),
	row(0x7690_90bf_9760_5711, &CUT),
];

/// R&C1's look (rac1/hero_fur.rs PARTS), unchanged.
static PARTS: [FurPart; 1] = [FurPart { name: "body", len: [0.010, 0.022], comb: [0.004, 0.010], density: 7.0, thick: [1.08, 1.2], halo: [1.8, 2.6], tip: [1.14, 1.08, 0.98], root: 0.05, sheen: [0.40, 0.55], occ: 0.5, spec: 0.6, spec_exp: 48.0, lift: 0.4, sharp: 0.45, strand: 0.3 }];

static SPEC: FurSpec = FurSpec { rows: &ROWS, parts: &PARTS };

/// `Pack::fur`.
pub fn spec() -> Option<&'static FurSpec> {
	Some(&SPEC)
}

static NOTED: Mutex<Vec<(usize, u32)>> = Mutex::new(Vec::new());
static HERO: AtomicU32 = AtomicU32::new(u32::MAX);

/// world.rs `level_classes`: level `level`'s hero class has engine class id `id`.
pub fn note_hero(level: usize, id: u32) {
	if let Ok(mut n) = NOTED.lock() {
		n.retain(|e| e.0 != level);
		n.push((level, id));
	}
}

/// frame.rs `Stage::load`, before the scene build: the shown level's hero class becomes the fur prop.
pub fn use_level(level: usize) {
	let id = NOTED.lock().ok().and_then(|n| n.iter().find(|e| e.0 == level).map(|e| e.1)).unwrap_or(u32::MAX);
	HERO.store(id, Ordering::Relaxed);
}

/// props.rs `sync`, every field: the class the game draws Ratchet with (the first moby record's class) [C, runs/g4/fu_on_0200.err:
/// on fg1 0200 the level's noted HERO_CLASS 0x40b98a40 had no prop slot in any frame, so the fur never found him]. Same id as
/// before = nothing changes (the engine's `fur_prop_follow` rebuilds the fur batch only on a change).
pub fn follow_hero(id: u32) {
	HERO.store(id, Ordering::Relaxed);
}

/// `Pack::fur_prop`.
pub fn hero_class() -> u32 {
	HERO.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn fur_hero_follows_the_shown_level() {
		note_hero(901, 0x4000_1234);
		note_hero(902, 0x4000_5678);
		use_level(902);
		assert_eq!(hero_class(), 0x4000_5678);
		use_level(999);
		assert_eq!(hero_class(), u32::MAX, "unknown level: no fur prop");
		let s = spec().unwrap();
		assert!(s.rows.iter().all(|r| (r.part as usize) < s.parts.len()));
	}

	/// [b46 W3] One row per fur armor, all hashes distinct, the plate cuts keep the face cuts first.
	#[test]
	fn every_fur_armor_has_its_row() {
		let rows = &spec().unwrap().rows;
		assert_eq!(rows.len(), 7);
		let mut h: Vec<u64> = rows.iter().map(|r| r.hash).collect();
		h.sort();
		h.dedup();
		assert_eq!(h.len(), 7);
		assert_eq!(rows[2].mask.cut[..3], CUT[..]);
		assert!(rows[2].mask.cut.len() > 3 && rows[3].mask.cut.len() > 3);
	}

	/// Eye, mouth line and nose carry no fur, the eye socket's length is 0, cheeks and the forehead keep theirs.
	#[test]
	fn face_has_no_fur_on_eye_mouth_and_nose() {
		let m = ROWS[0].mask;
		let t = vec![[196u8, 144, 38, 255]; 256 * 256];
		let map = m.map(256, 256, &t);
		let at = |x: usize, y: usize| map[y * 256 + x];
		for (x, y, what) in [(170, 120, "eyeball"), (160, 108, "eye rim"), (182, 133, "eye rim"), (100, 86, "mouth"), (80, 85, "mouth"), (110, 72, "nose")] {
			assert_eq!(at(x, y)[0], 0, "{what} {x},{y}");
		}
		assert_eq!(at(96, 40)[1], 0, "eye socket length");
		assert_eq!(at(48, 70)[0], 255, "cheek is fur");
		assert!((at(48, 70)[1] as f32) > 1.3 * rac_engine::furspec::ZONE_ONE, "cheek tufts");
		assert_eq!(at(200, 230), [255, 128, 128, 255], "body keeps the part's look");
	}
}
