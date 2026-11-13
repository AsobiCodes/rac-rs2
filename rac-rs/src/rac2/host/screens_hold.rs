//! [PC] The modern pause and title pages stay up through the retail menu's short gaps (lane flk, build 44, user report: "the classic
//! pages sometimes flicker over the modern ones"). A modern page is drawn only while its reader read this field's RAM (game thread,
//! before the field). At a page change, at the close and on retail's transient cursor fields the reader has no facts for a field,
//! while the window still shows a GS picture with the old retail page in it (live play: the picture lags the game by a field or two)
//! -> one frame of the classic page [C live capture of build 43, Dobbo fg1 0357, 2026-11-12: the classic root at every root ->
//! Weapons / Quick Select / Items change and at the close]. Here the window keeps the last modern scene and draws it again, the
//! retail overlay left out and the page's preview hole filled (fx1, screens_preview.rs: at a back the picture already holds the next
//! page, so a crop showed the world for one field [C: fx1 p1s on b44 f1, backs 141, 221, 381, 461, 792, 2026-11-12]), while
//! (a) the retail menu is on a page a modern reader handles (a short None; at most `MAX_HOLD` fields after the last modern draw), or
//! (b) the page changed to one without a modern twin (Options list, Special, Help sub pages) or the menu closed, until the picture
//!     shows the RAM of the change field + 1 (`since` + 2 fields done). A classic-by-design page shows as soon as it settled.
//! (c) [b45 P3, sweep B13] the vendor / Ship Shack (game mode 5) and the Galactic Map (game mode 3) are mode-word pages, not pause
//!     sub pages: while the game mode is one of them they count as handled (a short None of their readers is held), and the mode's
//!     change (open / close) starts a new record like a page change, so the close shows the world only once the picture is past it.
//! Game side: `note` (modern_screen.rs `read`, every field) publishes the menu's page and the game mode word. Classic menu style: no modern page is ever
//! drawn, so nothing is kept and the picture is the one of before, bit for bit.
use super::screens_preview::Preview;
use rac_engine::ui::prim::Prim;
use rac_engine::viewer::targets::{RETAIL_HD_TEX, RETAIL_OV_TEX};
use rac_engine::viewer::Gpu;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

/// The longest gap (fields after the last modern draw) a page is kept over a handled page whose reader has no facts.
pub const MAX_HOLD: u64 = 25;

/// The pages under the pause root a modern reader draws: Weapons, Gadgets, Quick Select, Items, Help.
const SUB_PAGES: [u32; 5] = [crate::modern_grid::PAGE_WEAPONS, crate::modern_grid::PAGE_GADGETS, crate::modern_qs::PAGE_QS, crate::modern_items::PAGE_ITEMS, crate::modern_help::PAGE_HELP];

/// The retail menu as the game thread read it before a field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Menu {
	/// The current page record's id (0 = no menu page up).
	pub page: u32,
	/// The parent record's page id (0 for a root).
	pub parent: u32,
	/// The pause menu's manager word is set (a game runs under it).
	pub up: bool,
	/// The game mode word (0x1a8f80; level 1: 0x1a8fc0): 5 vendor / Ship Shack, 3 Galactic Map.
	pub mode: u32,
	/// The fields done when this page, parent and manager state began (the GS picture of that many fields shows the same RAM).
	pub since: u64,
}

/// This field's record from the previous one and the page read at `at` fields done: `since` stays while page, parent and manager
/// stay. Pure.
#[cfg(test)]
pub fn next(prev: Option<Menu>, page: u32, parent: u32, up: bool, at: u64) -> Menu {
	next_mode(prev, page, parent, up, 0, at)
}

/// `next` with the game mode word (a mode change starts a new record too). Pure.
pub fn next_mode(prev: Option<Menu>, page: u32, parent: u32, up: bool, mode: u32, at: u64) -> Menu {
	match prev {
		Some(m) if (m.page, m.parent, m.up, m.mode) == (page, parent, up, mode) => m,
		_ => Menu { page, parent, up, mode, since: at },
	}
}

/// A mode-word page with a modern twin: the vendor / Ship Shack (mode 5) or the Galactic Map (mode 3 = any page menu, so only its
/// page 0xf without a parent; the Quick Select grid shares id 0xf under a parent). Pure.
fn mode_page(m: &Menu) -> bool {
	m.mode == crate::modern_vendor::VENDOR_MODE || (m.mode == crate::modern_map::MAP_MODE && m.page == crate::modern_map::MAP_PAGE && m.parent == 0)
}

/// A page a modern reader draws: a root (pause or title) or a pause sub page with a modern twin. Pure.
pub fn handled(m: &Menu) -> bool {
	mode_page(m) || (m.page == 2 && m.parent == 0) || (SUB_PAGES.contains(&m.page) && m.parent == 2 && m.up)
}

/// Whether the kept scene is drawn this display frame: `drawn_at` = fields done at the last modern draw, `menu` = the newest record,
/// `done` = the GS fields done of the picture shown. Pure.
pub fn keep(drawn_at: u64, menu: Option<Menu>, done: u64) -> bool {
	let Some(m) = menu else { return false };
	if done > drawn_at + MAX_HOLD {
		return false;
	}
	handled(&m) || done < m.since + 2
}

/// [PC] fx1: the picture of `done` fields does not hold the newest record's page yet (it shows the RAM of the change field + 1 from
/// `since` + 2 on). Pure.
pub fn behind(cur: Option<Menu>, done: u64) -> bool {
	cur.is_some_and(|c| done < c.since + 2)
}

struct Kept {
	prims: Vec<Prim>,
	at: u64,
	/// [PC] fx1: the page's preview hole as drawn (screens_preview.rs).
	preview: Option<Preview>,
}

static KEPT: Mutex<Option<Kept>> = Mutex::new(None);
/// The newest record.
static MENU: Mutex<Option<Menu>> = Mutex::new(None);
/// The previous display frame drew the world (`world_latch`).
static WORLD: AtomicBool = AtomicBool::new(false);
/// [PC] fx1: the GS fields done of the picture this display frame shows (`begin`).
static DONE: AtomicU64 = AtomicU64::new(0);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Game thread, every field (modern_screen.rs `read`, before its gates): the page the retail menu is on (0 = none), its parent's
/// page id, the manager word set. `cold::FIELD` holds the previous field's number then: the RAM read is that of FIELD + 1 fields.
pub(crate) fn note(page: u32, parent: u32, up: bool, mode: u32) {
	let at = ee_oracle::cold::FIELD.load(Ordering::Relaxed) + 1;
	let mut m = lock(&MENU);
	*m = Some(next_mode(*m, page, parent, up, mode, at));
}

/// Window thread, first in the display-frame step: the GS fields done of the picture this frame shows.
pub(super) fn begin(done: u64) {
	DONE.store(done, Ordering::Relaxed);
}

/// Window thread: the picture of this display frame does not hold the retail menu's newest page yet (`behind`).
pub(super) fn picture_behind() -> bool {
	behind(*lock(&MENU), DONE.load(Ordering::Relaxed))
}

/// Window thread: the readers' world test (READY), kept on across ONE display frame without the world, so a single refused world
/// frame does not take a whole field of facts away (the classic page would show for that field).
pub(super) fn world_latch(world: bool) -> bool {
	WORLD.swap(world, Ordering::Relaxed) || world
}

/// Window thread, after the pages' draws: `drew` = a modern page set its scene this frame, `can` = the pages may draw (UI pipes, no
/// Options page, the world), `done` = the GS fields done of the picture this frame shows.
pub(super) fn frame(g: &mut Gpu, drew: bool, can: bool, done: u64) {
	let mut k = lock(&KEPT);
	let preview = super::screens_preview::take();
	let cur = *lock(&MENU);
	if drew {
		match (g.ui_gpu.scene(), k.as_mut()) {
			(Some(p), Some(kept)) => {
				kept.prims.clear();
				kept.prims.extend_from_slice(p);
				(kept.at, kept.preview) = (done, preview);
			}
			(Some(p), None) => *k = Some(Kept { prims: p.to_vec(), at: done, preview }),
			(None, _) => *k = None,
		}
		return;
	}
	let Some(kept) = k.as_ref().filter(|kept| can && keep(kept.at, cur, done)) else {
		*k = None;
		return;
	};
	g.hud.retain(|q| q.tex != RETAIL_OV_TEX && q.tex != RETAIL_HD_TEX);
	match kept.preview {
		// [PC] fx1: the hole filled, never the world: at a back / the close the picture already holds the next page (no preview).
		Some(p) => g.ui_gpu.set_scene(super::screens_switch::held(&kept.prims, Some(p.hole))),
		None => g.ui_gpu.set_scene(kept.prims.clone()),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn menu(page: u32, parent: u32, up: bool, since: u64) -> Option<Menu> {
		Some(Menu { page, parent, up, mode: 0, since })
	}

	#[test]
	fn vendor_and_map_modes_are_held_and_their_close_waits_for_the_picture() {
		let shop = Some(next_mode(None, 0, 0, false, 5, 200));
		assert!(keep(210, shop, 212), "a short None of the vendor reader is held");
		assert!(!keep(210, shop, 211 + MAX_HOLD), "bounded");
		let map = next_mode(None, 0xf, 0, false, 3, 300);
		assert!(keep(305, Some(map), 307));
		let closed = next_mode(Some(map), 0, 0, false, 0, 320);
		assert_eq!(closed.since, 320, "the close is a new record");
		assert!(keep(319, Some(closed), 321) && !keep(319, Some(closed), 322), "kept until the picture is past the close");
		assert!(!handled(&next_mode(None, 0, 0, false, 0, 0)), "plain play");
		assert!(!handled(&next_mode(None, 8, 2, true, 3, 0)), "mode 3 is every page menu: the classic Options list is not held");
	}

	#[test]
	fn next_keeps_since_while_the_page_stays() {
		let a = next(None, 2, 0, true, 40);
		assert_eq!(a.since, 40);
		assert_eq!(next(Some(a), 2, 0, true, 41).since, 40);
		assert_eq!(next(Some(a), 3, 2, true, 61).since, 61, "a page change starts a new record");
		assert_eq!(next(Some(a), 2, 0, false, 70).since, 70, "the manager word too");
	}

	#[test]
	fn handled_pages_are_the_roots_and_the_modern_sub_pages() {
		for p in [3, 4, 5, 6, 7] {
			assert!(handled(&menu(p, 2, true, 0).unwrap()), "page {p}");
			assert!(!handled(&menu(p, 2, false, 0).unwrap()), "page {p} without a game");
			assert!(!handled(&menu(p, 7, true, 0).unwrap()), "page {p} under another parent");
		}
		assert!(handled(&menu(2, 0, true, 0).unwrap()) && handled(&menu(2, 0, false, 0).unwrap()), "pause and title roots");
		for p in [0, 8, 11, 15, 29, 31, 52] {
			assert!(!handled(&menu(p, 2, true, 0).unwrap()), "classic page {p}");
		}
	}

	#[test]
	fn a_short_none_on_a_handled_page_is_held_and_bounded() {
		// root -> Weapons: the grid reader has no facts for the first field.
		assert!(keep(60, menu(3, 2, true, 61), 61));
		assert!(keep(60, menu(3, 2, true, 61), 60 + MAX_HOLD));
		assert!(!keep(60, menu(3, 2, true, 61), 61 + MAX_HOLD), "a reader that never reads lets the classic page show");
	}

	#[test]
	fn a_classic_page_or_the_close_shows_once_the_picture_is_past_the_change() {
		// root -> Options list at 451 (page 8): kept for the pictures of 451 and 452 fields, the list from 453 on.
		assert!(keep(450, menu(8, 2, true, 451), 451));
		assert!(keep(450, menu(8, 2, true, 451), 452));
		assert!(!keep(450, menu(8, 2, true, 451), 453));
		// the close (no page) at 521
		assert!(keep(521, menu(0, 0, false, 521), 522));
		assert!(!keep(521, menu(0, 0, false, 521), 523));
		// a settled classic page long after the last modern draw: never kept
		assert!(!keep(100, menu(8, 2, true, 90), 101));
	}

	#[test]
	fn no_menu_record_never_holds() {
		assert!(!keep(10, None, 11));
	}

	#[test]
	fn the_picture_is_behind_a_new_page_for_two_fields() {
		// root -> Weapons at 101: the pictures of 101 and 102 fields still show the root (no preview in the hole).
		assert!(behind(menu(3, 2, true, 101), 101));
		assert!(behind(menu(3, 2, true, 101), 102));
		assert!(!behind(menu(3, 2, true, 101), 103));
		assert!(!behind(None, 5), "no record: never behind");
	}
}
