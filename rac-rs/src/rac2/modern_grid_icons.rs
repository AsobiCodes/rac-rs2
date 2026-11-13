//! [PC] The item icons of the modern Weapons / Gadgets cells (lane pages; decoder: `modern_icons_decode.rs`, lane icons). Once per
//! level (`want`, from the game thread) a worker reads the level's HUD sheets (`load_level` reads the whole WAD: not on the game
//! thread), takes frame 0 of every item sheet 0xea60.. (32 x 32 menu pictures; bigger sheets are portraits) and packs them into the
//! engine's picture atlas (`rac_engine::ui::images`, nothing else of R&C2 uses it) under `BASE + icon id - 0xea60`. The sheets are
//! grey (palette ~ 102..127 = half intensity: the GS doubles at draw), so the pictures are stored doubled and the page tints them
//! (retail: cyan in the grid, orange when selected). `icon_for` is None until the level's set is registered, or when the level's HUD
//! set lacks the sheet: the cell then shows the item's name.
use rac_engine::ui::images::{self, ImageSet, Pic};
use std::sync::Mutex;

/// A registered item picture: its id in the atlas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Icon(pub u32);

pub const BASE: u32 = 0x3000;
const FIRST: u32 = 0xea60;
const LAST: u32 = 0xeb10;
/// The weapon mod icons: R&C2 HUD sheet 0xea98, frame 0 = Shock, 1 = Acid, 2 = Lock On (retail draws them with `FUN_00338600(sprite, 0xea98, frame)`).
pub const MOD_SHEET: u32 = 0xea98;
const MOD_BASE: u32 = BASE + (LAST - FIRST);
/// The collectibles' icons: sheet 0xeaa2, frame 0 Platinum Bolt, 1 Crystals, 2 Moonstones, 3 Raritanium (the table at 0x1aa910).
pub const COLLECT_SHEET: u32 = 0xeaa2;
const COLLECT_BASE: u32 = MOD_BASE + 8;
/// Pictures larger than this are portraits, not items.
const MAX_SIDE: usize = 64;

static ISO: Mutex<String> = Mutex::new(String::new());
/// The level a worker is loading or has loaded.
static LEVEL: Mutex<Option<u32>> = Mutex::new(None);
/// The last level whose load failed and its failed tries: a failed load is tried again on the next `want`, `TRIES` times in all.
/// Lock order: LEVEL, then FAILED.
static FAILED: Mutex<(u32, u32)> = Mutex::new((u32::MAX, 0));
const TRIES: u32 = 3;

pub fn set_iso(iso: &str) {
	*ISO.lock().unwrap_or_else(|e| e.into_inner()) = iso.to_string();
}

fn may_try(failed: (u32, u32), level: u32) -> bool {
	failed.0 != level || failed.1 < TRIES
}

/// A worker's load of `level` failed: count it, and let the next `want(level)` try again while that level is still the wanted one.
fn failed(level: u32) {
	let mut l = LEVEL.lock().unwrap_or_else(|e| e.into_inner());
	let mut f = FAILED.lock().unwrap_or_else(|e| e.into_inner());
	*f = (level, if f.0 == level { f.1 + 1 } else { 1 });
	if *l == Some(level) {
		*l = None;
	}
}

/// Starts the worker for `level` unless it was started already.
pub fn want(level: u32) {
	{
		let mut l = LEVEL.lock().unwrap_or_else(|e| e.into_inner());
		if *l == Some(level) || !may_try(*FAILED.lock().unwrap_or_else(|e| e.into_inner()), level) {
			return;
		}
		*l = Some(level);
	}
	let iso = ISO.lock().unwrap_or_else(|e| e.into_inner()).clone();
	if iso.is_empty() {
		return;
	}
	let _ = std::thread::Builder::new().name("grid-icons".into()).spawn(move || match load(&iso, level) {
		Ok(n) => eprintln!("rac2 grid icons: level {level}: {n} pictures"),
		Err(e) => {
			eprintln!("rac2 grid icons: level {level}: {e}");
			failed(level);
		}
	});
}

/// Reads and registers the pictures of a level; the count.
fn load(iso: &str, level: u32) -> Result<usize, String> {
	let l = rac_formats::rc2::level::load_level(iso, level as usize).map_err(|e| e.to_string())?;
	let set = crate::modern_icons_decode::IconSet::from_level(&l)?;
	let owned: Vec<(u32, u32, u32, Vec<u8>)> = set
		.ids()
		.into_iter()
		.filter(|&id| (FIRST..LAST).contains(&(id as u32)))
		.filter_map(|id| {
			let img = set.icon_for(id)?;
			(img.w <= MAX_SIDE && img.h <= MAX_SIDE).then(|| (BASE + id as u32 - FIRST, img.w as u32, img.h as u32, doubled(img.rgba)))
		})
		.collect();
	let mut owned = owned;
	// The extra frames: the weapon mod icons (sheet 0xea98, frames 0..3) and the collectibles (sheet 0xeaa2, frames 0..4).
	for (sheet, first, n) in [(MOD_SHEET, MOD_BASE, 3usize), (COLLECT_SHEET, COLLECT_BASE, 4)] {
		for k in 0..n {
			if let Some(img) = set.frame(sheet as u16, k).filter(|i| i.w <= MAX_SIDE && i.h <= MAX_SIDE) {
				owned.push((first + k as u32, img.w as u32, img.h as u32, doubled(img.rgba)));
			}
		}
	}
	let pics: Vec<Pic> = owned.iter().map(|(id, w, h, b)| Pic { id: *id, w: *w, h: *h, rgba: b }).collect();
	let packed = ImageSet::pack(&pics);
	// A worker for an older level that finishes after a newer `want` must not replace the newer level's set (LEVEL held: no
	// `want` in between).
	let wanted = LEVEL.lock().unwrap_or_else(|e| e.into_inner());
	if *wanted != Some(level) {
		eprintln!("rac2 grid icons: level {level}: dropped, level {:?} is wanted now", *wanted);
		return Ok(0);
	}
	if !packed.is_empty() {
		images::set(packed);
	}
	Ok(owned.len())
}

/// The colour channels doubled (clamped): the GS's 128 = 1.0.
pub fn doubled(mut rgba: Vec<u8>) -> Vec<u8> {
	for p in rgba.chunks_exact_mut(4) {
		for c in &mut p[..3] {
			*c = (*c as u32 * 2).min(255) as u8;
		}
	}
	rgba
}

/// The picture of the sheet `icon_id` (the id the cell tables give: 0xea60 + n); None = not registered (yet).
pub fn icon_for(icon_id: u32) -> Option<Icon> {
	if !(FIRST..LAST).contains(&icon_id) {
		return None;
	}
	let id = BASE + icon_id - FIRST;
	images::has(id).then_some(Icon(id))
}

/// The icon of weapon mod frame `frame` (0 Shock, 1 Acid, 2 Lock On).
pub fn mod_icon(frame: usize) -> Option<Icon> {
	let id = MOD_BASE + frame as u32;
	(frame < 3 && images::has(id)).then_some(Icon(id))
}

/// The icon of collectible `k` (0..3: Platinum Bolt, Crystals, Moonstones, Raritanium).
pub fn collect_icon(k: usize) -> Option<Icon> {
	let id = COLLECT_BASE + k as u32;
	(k < 4 && images::has(id)).then_some(Icon(id))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn colours_double_and_clamp_but_alpha_stays() {
		assert_eq!(doubled(vec![102, 127, 200, 77]), vec![204, 254, 255, 77]);
	}

	#[test]
	fn unknown_ids_have_no_icon() {
		assert_eq!(icon_for(0x10), None);
		assert_eq!(icon_for(0xeb10), None);
	}

	#[test]
	fn a_failed_load_is_tried_again_a_few_times() {
		*LEVEL.lock().unwrap() = Some(999);
		failed(999);
		assert_eq!(*LEVEL.lock().unwrap(), None, "the next want(999) starts a worker again");
		assert!(may_try(*FAILED.lock().unwrap(), 999));
		assert!(may_try((999, TRIES - 1), 999) && !may_try((999, TRIES), 999) && may_try((999, TRIES), 998));
		// A failure of a level that is no longer wanted leaves the wanted one alone.
		*LEVEL.lock().unwrap() = Some(998);
		failed(999);
		assert_eq!(*LEVEL.lock().unwrap(), Some(998));
		*LEVEL.lock().unwrap() = None;
	}
}
