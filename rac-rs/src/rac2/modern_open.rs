//! [PC] The modern Options page's door in R&C2 (parity wave 2 B; R&C1's twin is rac1/retail/modern_swallow.rs): Cross on the Camera
//! row of the retail Options list (Pause -> Options) opens the engine's modern page on its Camera tab instead of the retail Camera
//! page; the press is swallowed (the game never sees it), and Triangle / Esc on the modern page returns to the retail Options list.
//! Open with either menu style (Classic = retail pause / title / vendor only); off with the UI pipes refused, or while the page
//! is already up: then the pad is returned unchanged, bit for bit.
//!
//! The menu facts [C] (par-w2c/par-w2d RAM dumps of Pause -> Options on levels 26 (st17/0110) and 23 (st23/0329), disassembly of
//! the Options page handler, descriptor word 0):
//! - The menu globals block G moves per level (0x1fea40 on 26, 0x1ff9c0 on 23): it is found by the shape of the Options handler's
//!   Triangle branch, `andi v0,a0,0x10; beqz; lui v0,HI; addiu v0,v0,LO; lw v1,0x14(v0); lw v1,0xe0(v1)` (`find`). G+0x14 = the
//!   current menu page record (0 with no menu up); record +0xe0 = parent page, +0xe4 = page id (pause list 2, Options 8).
//! - The menu list object L = *B + 0x3c210, B found by `lui v0,HI; lw v0,LO(v0); .. lui a0,3; ori a0,a0,0xc210` (0x1a90cc on both
//!   levels). L+0x31c = list depth (the Options list is depth 1), the row index at L+0x1b8 + 4 x depth, the row table at L+0x310 +
//!   4 x depth for depth 0 and 1 (0x318 deeper; the item getter 0x42ee80 on level 26). Row records are 0x14 bytes: +4 text id
//!   (Camera 0xba9), +0xc the page the row opens (the Camera page: id 29, parent = the Options record).
//! - L keeps its last row after the menu closes (stale): the page id gates it.
//!
//! `decide` is the pure core (no RAM) so the tests need no game; `filter` is the game thread's call once per field.
use rac_engine::input::binds::{BIT, CROSS};
use rac_engine::ui::menu_host;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

/// Page ids [C]: the Options list, and the Camera page its Camera row opens.
pub const OPTIONS_PAGE: u32 = 8;
pub const CAMERA_PAGE: u32 = 29;
/// The Camera row's text id [C].
pub const CAMERA_TEXT: u32 = 0xba9;
/// The face buttons, Select, Start and the d-pad: what a swallowed press clears.
pub const MENU_BTNS: u16 = 0xf9f0;

/// Where the menu facts live on the level in RAM (`find`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Locs {
	/// The menu globals block G.
	pub g: u32,
	/// The word holding the menu list object's base (L = *base + 0x3c210).
	pub base: u32,
	/// Where G's code shape was found (checked again before the next use: a new level moves it).
	pub at: u32,
}

/// The menu as one field sees it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Snap {
	pub page_id: u32,
	pub depth: u32,
	/// The current row's text id and the page id it opens (0 = none) and that page's parent record.
	pub text: u32,
	pub target_id: u32,
	pub target_parent: u32,
	/// The current page record.
	pub page: u32,
	/// The menu manager word at `Locs::base` (0x1a90cc on most levels, 0x1a910c on levels 1 and 25: the level's code names it).
	pub manager: u32,
	/// Where that word is (`Locs::base`): the level's block of menu globals moves with it (Quick Select's ring pointer, 0x1a91a0 /
	/// 0x1a91e0 on level 1 [C RAM 2026-11-12]).
	pub base: u32,
	/// The menu globals block G (`Locs::g`; 0x1f2840 at the title, elsewhere per level).
	pub g: u32,
}

fn rd(ram: &[u8], a: u32) -> u32 {
	let a = (a & 0x1ff_fffc) as usize;
	ram.get(a..a + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn imm(w: u32) -> i32 {
	(w & 0xffff) as u16 as i16 as i32
}

/// Whether G's code shape starts at `a`.
fn g_at(ram: &[u8], a: u32) -> Option<u32> {
	let w = |k: u32| rd(ram, a + k);
	let ok = w(0) == 0x3082_0010 && w(4) >> 16 == 0x1040 && w(8) >> 16 == 0x3c02 && w(12) >> 16 == 0x2442 && w(16) == 0x8c43_0014 && w(20) == 0x8c63_00e0;
	ok.then(|| (((w(8) & 0xffff) << 16) as i32 + imm(w(12))) as u32)
}

fn base_at(ram: &[u8], a: u32) -> Option<u32> {
	let w = |k: u32| rd(ram, a + k);
	if w(0) >> 16 != 0x3c02 || w(4) >> 16 != 0x8c42 {
		return None;
	}
	(8..28).step_by(4).any(|k| w(k) == 0x3c04_0003 && w(k + 4) == 0x3484_c210).then(|| (((w(0) & 0xffff) << 16) as i32 + imm(w(4))) as u32)
}

/// Scans the level's code (0x200000..0x600000) for both shapes.
pub fn find(ram: &[u8]) -> Option<Locs> {
	let (mut g, mut base) = (None, None);
	for a in (0x20_0000u32..0x60_0000).step_by(4) {
		if g.is_none() {
			g = g_at(ram, a).map(|v| (v, a));
		}
		if base.is_none() {
			base = base_at(ram, a);
		}
		if g.is_some() && base.is_some() {
			break;
		}
	}
	let (g, at) = g?;
	Some(Locs { g, base: base?, at })
}

/// The menu of this field (None: no menu page is current).
pub fn snap(ram: &[u8], l: &Locs) -> Option<Snap> {
	let page = rd(ram, l.g + 0x14);
	if page == 0 {
		return None;
	}
	let list = rd(ram, l.base).wrapping_add(0x3c210);
	let depth = rd(ram, list + 0x31c);
	let idx = rd(ram, list + 0x1b8 + 4 * depth.min(15));
	let table = rd(ram, list + 0x310 + 4 * depth.min(2));
	let item = table.wrapping_add(idx.min(255) * 0x14);
	let target = rd(ram, item + 0xc);
	let (target_id, target_parent) = if target == 0 { (0, 0) } else { (rd(ram, target + 0xe4), rd(ram, target + 0xe0)) };
	Some(Snap { page_id: rd(ram, page + 0xe4), depth, text: rd(ram, item + 4), target_id, target_parent, page, manager: rd(ram, l.base), base: l.base, g: l.g })
}

/// Whether the press `pad` on this menu opens the modern page: the Options list current, its cursor on the Camera row, Cross down,
/// the Modern menu style, the UI pipes working.
pub fn decide(s: Option<&Snap>, pad: u16, modern: bool, gpu_ok: bool) -> bool {
	if !modern || !gpu_ok || pad & BIT[CROSS] == 0 {
		return false;
	}
	let Some(s) = s else { return false };
	s.page_id == OPTIONS_PAGE && s.depth == 1 && s.text == CAMERA_TEXT && s.target_id == CAMERA_PAGE && s.target_parent == s.page
}

/// The open request for the window thread (the tab id).
static REQUEST: Mutex<Option<&'static str>> = Mutex::new(None);
static LOCS: Mutex<Option<Locs>> = Mutex::new(None);
/// The window's verdict on the UI pipes (`ui::gpu::refused()`), set each display frame.
static GPU_REFUSED: AtomicBool = AtomicBool::new(false);
/// The retail Options list is still the current page (the modern page lives while it is).
static OPTIONS_UP: AtomicBool = AtomicBool::new(true);
/// The game thread's raw pad of the newest field (buttons << 32 | sticks), for the page's pad input.
static PAD: AtomicU64 = AtomicU64::new(0x8080_8080);

/// A left click in the window since the last field (window thread sets it, `filter` takes it).
static CLICK: AtomicBool = AtomicBool::new(false);

/// Window thread: a left mouse press (cursor free, page down): the next field's door treats it as Cross on the Camera row.
pub fn note_click() {
	CLICK.store(true, Ordering::Relaxed);
}

pub fn set_gpu_refused(r: bool) {
	GPU_REFUSED.store(r, Ordering::Relaxed);
}

// [b45 P4B] The visible PC rows of the modern Options list (modern_optlist.rs: Display, Graphics, title Camera) open the page on
// their tab; `front` = from the title's Options (R&C1 rac1/modern_menu/host.rs `open(o.front, ..)`). Game thread.
static FRONT: AtomicBool = AtomicBool::new(false);
pub fn request_tab(tab: &'static str, front: bool) {
	FRONT.store(front, Ordering::Relaxed);
	if let Ok(mut r) = REQUEST.lock() {
		r.get_or_insert(tab);
	}
	OPTIONS_UP.store(true, Ordering::Relaxed);
}
/// [b45 P4B] Window thread, at the open: whether the request came from the title (read once; the door's own requests are not).
pub fn take_front() -> bool {
	FRONT.swap(false, Ordering::Relaxed)
}

/// Window thread: the pending open request.
pub fn take_open() -> Option<&'static str> {
	REQUEST.lock().ok().and_then(|mut r| r.take())
}

pub fn options_up() -> bool {
	OPTIONS_UP.load(Ordering::Relaxed)
}

// [b46 U6] Item 1: the PC Options page's device follows the REAL controller. The engine page marks the pad as the device on every
// button edge of the pad it is handed, and the field's merged pad holds the keys and clicks too (kbm_table), so a key turned into
// Cross flipped the hint bar to pad glyphs. A played session hands the page only the connected controller's report (R&C1's rule,
// rac1/host/handler/frame_ui.rs pad_peek); scripted runs (dev feed, window shots, replays) keep the merged pad, their only input.
static REAL_PAD: AtomicU64 = AtomicU64::new(0x8080_8080);
static DRIVEN: AtomicBool = AtomicBool::new(false);

/// Window thread, every poll: the connected controller's report in the standard layout (None = no controller: idle).
pub fn note_real_pad(p: Option<&rac_engine::input::pad::PadState>) {
	let v = p.map_or(0x8080_8080, |p| (p.buttons as u64) << 32 | u32::from_le_bytes(p.sticks) as u64);
	REAL_PAD.store(v, Ordering::Relaxed);
}

/// Scripted input (dev feed, window shots, a replay): the page reads the merged pad.
pub fn set_driven(on: bool) {
	DRIVEN.store(on, Ordering::Relaxed);
}

/// The pad the Options page reads (window thread, host/menu.rs).
pub fn page_pad() -> (u16, [u8; 4]) {
	if DRIVEN.load(Ordering::Relaxed) {
		return pad();
	}
	let p = REAL_PAD.load(Ordering::Relaxed);
	((p >> 32) as u16, (p as u32).to_le_bytes())
}

/// The newest field's raw pad (buttons, sticks rx ry lx ly).
pub fn pad() -> (u16, [u8; 4]) {
	let p = PAD.load(Ordering::Relaxed);
	((p >> 32) as u16, (p as u32).to_le_bytes())
}

/// The menu style: Modern unless `menustyle=classic` in pc_options.txt or RAC_MENU_STYLE=classic.
pub fn modern() -> bool {
	if let Ok(v) = std::env::var("RAC_MENU_STYLE") {
		return !v.trim().eq_ignore_ascii_case("classic");
	}
	!crate::options::get().extra_get("menustyle").is_some_and(|v| v.trim().eq_ignore_ascii_case("classic"))
}

/// [b47 X3] (W6 U4) Calls left to skip after a failed `find`: the 4 MB scan runs once per ~16 calls while no menu page record
/// exists, not on every call (a menu opens at most a few calls later).
// [b48 Y3] Review minor 2: counted in game FIELDS (TICK is advanced once per field by `filter`), not in calls: one failed find
// blanks the readers for BACKOFF_FIELDS fields at most, however many callers ask in a field. BACKOFF_UNTIL = first tick that scans again.
static TICK: AtomicU64 = AtomicU64::new(1);
static BACKOFF_UNTIL: AtomicU64 = AtomicU64::new(0);
const BACKOFF_FIELDS: u64 = 3;

pub(crate) fn snap_now(ram: &[u8]) -> Option<Snap> {
	let mut l = LOCS.lock().unwrap_or_else(|e| e.into_inner());
	if !l.is_some_and(|x| g_at(ram, x.at) == Some(x.g)) {
		let t = TICK.load(Ordering::Relaxed);
		if t < BACKOFF_UNTIL.load(Ordering::Relaxed) {
			*l = None;
			return None;
		}
		*l = find(ram);
		if l.is_none() {
			BACKOFF_UNTIL.store(t + BACKOFF_FIELDS, Ordering::Relaxed);
		}
	}
	snap(ram, l.as_ref()?)
}

/// The current menu page id ([G+0x14] + 0xe4; None with no menu page up). Used by pc_menu_hover.rs.
pub fn page_id_now(ram: &[u8]) -> Option<u32> {
	snap_now(ram).map(|s| s.page_id)
}

/// Game thread, once per field before `set_pad`: the buttons and sticks the game gets. While the modern page is up (or a button
/// held when it closed is still down) the game gets a neutral pad; a Cross that opens the page is cleared with the other menu
/// buttons; everything else is returned unchanged.
pub fn filter(ram: &[u8], b: u16, s: [u8; 4]) -> (u16, [u8; 4]) {
	PAD.store((b as u64) << 32 | u32::from_le_bytes(s) as u64, Ordering::Relaxed);
	TICK.fetch_add(1, Ordering::Relaxed); // [b48 Y3] one game field
	let (b, s) = crate::modern_optlist::filter(ram, b, s); // [b45 P4B] the modern Options list (its Cross on Camera still opens the door)
	if menu_host::pad_filter(b) {
		if rac_engine::ui::is_open() {
			OPTIONS_UP.store(snap_now(ram).is_some_and(|x| x.page_id == OPTIONS_PAGE), Ordering::Relaxed);
		}
		return (0, [0x80; 4]);
	}
	// A request the window has not opened yet: the press stays away from the game until it has (the game thread can run ahead).
	if REQUEST.lock().is_ok_and(|r| r.is_some()) {
		return (b & !MENU_BTNS, s);
	}
	// [b47 X4] U5: a click is no Cross for the door any more (it opened the PC page from anywhere while retail's cursor sat on
	// Camera): the Classic mouse (pc_menu_hover.rs) presses Cross only on the row under the pointer, and `door_after` below opens
	// the door for that Cross.
	CLICK.store(false, Ordering::Relaxed);
	let door = b;
	if door & BIT[CROSS] == 0 || rac_engine::pack::get().ui.is_none() {
		return (b, s);
	}
	// The door stays open with the Classic menu style too: R&C2 has no classic PC option pages (R&C1 keeps its PC rows in the
	// classic pages), so Classic means only the retail pause / title / vendor screens (modern_screen.rs).
	let gpu_ok = !GPU_REFUSED.load(Ordering::Relaxed);
	if !gpu_ok {
		return (b, s);
	}
	if decide(snap_now(ram).as_ref(), door, true, gpu_ok) {
		if let Ok(mut r) = REQUEST.lock() {
			r.get_or_insert("camera");
		}
		OPTIONS_UP.store(true, Ordering::Relaxed);
		return (b & !MENU_BTNS, s);
	}
	(b, s)
}

/// [b47 X4] The door for a Cross the Classic mouse added AFTER `filter` ran this field (pc_input.rs: a click on the Camera row, or the
/// walk's press on arrival): the same decision as `filter`'s, so the click opens the PC page and never retail's Camera page.
pub fn door_after(ram: &[u8], b: u16) -> u16 {
	if b & BIT[CROSS] == 0 || rac_engine::pack::get().ui.is_none() || GPU_REFUSED.load(Ordering::Relaxed) {
		return b;
	}
	if decide(snap_now(ram).as_ref(), b, true, true) {
		if let Ok(mut r) = REQUEST.lock() {
			r.get_or_insert("camera");
		}
		OPTIONS_UP.store(true, Ordering::Relaxed);
		return b & !MENU_BTNS;
	}
	b
}

#[cfg(test)]
#[path = "modern_open_tests.rs"]
mod tests;
