//! [PC] R&C2's modern Help page (pause page 7), GAME-THREAD side (lane pages2; twin of `modern_grid.rs`, R&C1's
//! `modern_menu/pause_help.rs`): the rows, the cursor and the descriptions read from RAM (`read`) and the pad the game gets while the
//! page is drawn (`filter`). The page itself is `modern_help_page.rs`, drawn by `host/screens_qs.rs`. The retail page keeps running
//! underneath: the player's pad reaches it untouched, the window's mouse becomes Up / Down / Cross / Triangle pulses (`Drive`,
//! the pause root's), nothing is written to game RAM. Choosing a row opens the game's own sub page (page ids 52, 21, ... in the
//! retail look; the gate only matches page 7), Back returns here.
//!
//! Facts [C] (RAM dumps of fg1 0262 + Down x4 + Cross, level 6, 2026-11-11):
//! - page id 7, parent = the pause root (id 2). List cursor = [0x1a90cc] + 0x3c3c8 (0..6 in row order, wraps).
//! - The 7 rows are fixed retail data (the row table sits in level data at stride 0x14: {name text id, 3, widget pointer, 0, 1.0f},
//!   0x293ffc on level 6): Monsterpedia 0x10de, Help Log 0xc31, Controls 0xb72, Moves 0xb73, Weapons 0xb62, Gadgets 0xb63,
//!   Ship 0x13dc. The description of row k is a text id of the 7-word table [0x10df, 0xc32, 0x1b4c, 0xb7e, 0xb7f, 0xb80, 0x1b4b]
//!   (level data, 0x2991f0 on level 6; the boot copy at 0x1ae410 has 0xb7d for Controls and is not the one the page uses).
//!   [b49 Z3 U9, C bank scan, one state per level word 0,1,2,3,4,5,6,7,8,9,10,11,12,22,24,25,26] the description table [0x10df, 0xc32, 0x1b4c,
//!   0xb7e, 0xb7f, 0xb80, 0x1b4b] is the same in every level (only its address moves: 0x265010 .. 0x2a7e70) and the 7-row table (0x10de, 0xc31,
//!   0xb72, 0xb73, 0xb62, 0xb63, 0x13dc at stride 0x14) is the same in the five levels checked (0, 4, 24, 10, 12): no level hides a row.
//!   (corrected 2026-11-11: notes 9.1 named the rows Controls, Moves, Purchasing, Switches, Map, Hints, Planets; that was a guess.)
use crate::modern_screen::{site_id, Drive, Texts, BOLTS, LEVEL, PAUSE_IDS};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

pub const PAGE_HELP: u32 = 7;
pub const CURSOR: u32 = 0x3c3c8;
pub const NAMES: [u32; 7] = [0x10de, 0xc31, 0xb72, 0xb73, 0xb62, 0xb63, 0x13dc];
pub const DESCS: [u32; 7] = [0x10df, 0xc32, 0x1b4c, 0xb7e, 0xb7f, 0xb80, 0x1b4b];
pub const T_BACK: u32 = 0xb60;

/// What the modern Help page shows this field.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Facts {
	pub rows: Vec<String>,
	pub descs: Vec<String>,
	pub cur: usize,
	pub tabs: Vec<String>,
	pub site: String,
	pub bolts: i32,
}

fn rd(ram: &[u8], a: u32) -> u32 {
	let a = (a & 0x1ff_fffc) as usize;
	ram.get(a..a + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Is the Help page up? Pure.
pub fn gate(page_id: u32, parent_id: u32, manager: u32) -> bool {
	page_id == PAGE_HELP && parent_id == 2 && manager != 0
}

#[derive(Default)]
struct Game {
	texts: Option<Texts>,
	retry: u32,
	drive: Drive,
	seq: u32,
	/// [PC] nav lane: the pad returned last field (the edge rule).
	last: u16,
}

/// [PC] nav lane: the page's tab in the pause root (modern_tabs.rs).
pub const TAB: usize = 4;

fn read(ram: &[u8], g: &mut Game) -> Option<Facts> {
	let s = crate::modern_open::snap_now(ram)?;
	let manager = crate::modern_screen::manager(ram, &s);
	if !gate(s.page_id, rd(ram, rd(ram, s.page + 0xe0).wrapping_add(0xe4)), manager) || !READY.load(Ordering::Relaxed) || !crate::modern_open::modern() {
		return None;
	}
	if !g.texts.is_some_and(|t| t.still(ram)) {
		g.texts = None;
		if g.retry > 0 {
			g.retry -= 1;
			return None;
		}
		g.texts = Texts::find(ram);
		if g.texts.is_none() {
			g.retry = 50;
		}
	}
	let texts = g.texts?;
	let cur = rd(ram, manager.wrapping_add(CURSOR)) as usize;
	if cur >= NAMES.len() {
		return None;
	}
	let rows = NAMES.iter().map(|&id| texts.get(ram, id).filter(|t| !t.is_empty())).collect::<Option<Vec<_>>>()?;
	let descs = DESCS.iter().map(|&id| texts.get(ram, id).unwrap_or_default()).collect();
	let tabs = PAUSE_IDS.iter().map(|&id| texts.get(ram, id).filter(|t| !t.is_empty())).collect::<Option<Vec<_>>>()?;
	let site = site_id(rd(ram, LEVEL)).and_then(|id| texts.get(ram, id)).unwrap_or_default();
	Some(Facts { rows, descs, cur, tabs, site, bolts: rd(ram, BOLTS) as i32 })
}

static GAME: Mutex<Option<Game>> = Mutex::new(None);
static FACTS: Mutex<Option<Facts>> = Mutex::new(None);
static REQ: Mutex<Option<(usize, bool, bool)>> = Mutex::new(None);
static SEQ: AtomicU32 = AtomicU32::new(0);
static READY: AtomicBool = AtomicBool::new(false);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn set_ready(ok: bool) {
	READY.store(ok, Ordering::Relaxed);
}

pub fn facts() -> Option<Facts> {
	lock(&FACTS).clone()
}

/// [PC] nav lane: the row a mouse request is taking the cursor to (drawn selected at once; modern_jump.rs `Pend`).
static PEND: crate::modern_jump::Pend<usize> = crate::modern_jump::Pend::new();

pub fn pending() -> Option<usize> {
	PEND.get()
}

/// Window thread: walk the game's cursor to `row` and pick it (`pick`); `back` = Triangle.
pub fn request(row: usize, pick: bool, back: bool) {
	*lock(&REQ) = Some((row, pick, back));
	PEND.set((!back).then_some(row));
	SEQ.fetch_add(1, Ordering::Relaxed);
}

/// Game thread, once per field after `modern_qs::filter`: the buttons the game gets. Without a modern Help page the pad is
/// returned unchanged, bit for bit.
pub fn filter(ram: &[u8], b: u16, s: [u8; 4]) -> (u16, [u8; 4]) {
	let mut gg = lock(&GAME);
	let g = gg.get_or_insert_with(Game::default);
	let f = if rac_engine::ui::is_open() { None } else { read(ram, g) };
	let Some(f) = f else {
		*lock(&FACTS) = None;
		(g.drive, g.seq, g.last) = (Drive::default(), SEQ.load(Ordering::Relaxed), b);
		PEND.set(None);
		return (b, s);
	};
	let seq = SEQ.load(Ordering::Relaxed);
	if seq != g.seq {
		g.seq = seq;
		if let Some((row, pick, back)) = lock(&REQ).take() {
			let mut d = Drive::default();
			(d.target, d.confirm, d.back) = (Some(row), pick, back);
			d.at = crate::modern_jump::at(crate::modern_jump::manager(ram), CURSOR); // [PC] nav lane: one-field moves
			g.drive = d;
		}
	}
	// [PC] nav lane: a tab clicked on the tab bar (modern_tabs.rs): back to the root first.
	if crate::modern_tabs::want_back(TAB) {
		let mut d = Drive::default();
		d.back = true;
		g.drive = d;
	}
	let mut out = b;
	if g.drive.busy() {
		let w = if g.drive.phase == 0 && g.last & crate::modern_open::MENU_BTNS != 0 { 0 } else { g.drive.step(f.cur, f.rows.len()) };
		out = (out & !crate::modern_open::MENU_BTNS) | w;
	}
	g.last = out;
	if SEQ.load(Ordering::Relaxed) == g.seq {
		PEND.set(g.drive.target.filter(|_| !g.drive.back));
	}
	*lock(&FACTS) = Some(f);
	(out, s)
}

#[cfg(test)]
#[path = "modern_help_tests.rs"]
mod tests;
