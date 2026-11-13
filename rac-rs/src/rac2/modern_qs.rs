//! [PC] R&C2's modern Quick Select page (pause page 5), GAME-THREAD side (lane pages2; twin of `modern_grid.rs`, R&C1's
//! `modern_menu/pause_quick.rs` + `pause_host/grid.rs`): the page's facts read from RAM (`read`) and the pad the game gets while
//! the page is drawn (`filter`). The page itself is `modern_qs_page.rs` (pure builder), drawn by `host/screens_qs.rs`. As with the
//! other modern screens the retail page keeps running underneath: the player's pad reaches it untouched, the window's mouse becomes
//! D-pad / L1 / R1 / Cross / Triangle pulses (`QWalk`), nothing is written to game RAM. Classic menu style, refused UI pipes or no
//! world behind the pause: `filter` returns the pad bit for bit and nothing is drawn.
//!
//! Facts [C] (RAM dumps of fg1 0277, level 6, 2026-11-11; notes_modern_screens.md section 8 of this lane):
//! - page id 5 (record at [G+0x14], +0xe4), parent = the pause root (id 2).
//! - Grid cursor C = [0x1a90cc] + 0x3ad80: {+0 widget pointer, +4 column, +8 row, +0xc GROUP (0 = the weapons grid, 1 = the Hand
//!   gadgets grid), +0x10 ITEM id}. Weapons: 4 columns x 6 rows, cell = the Weapons table (`modern_grid::Tables`); Down / Up and
//!   Left / Right wrap. Right from the weapons column 3 (row 5 tried) enters the Hand gadgets grid (3 columns x 2 rows, the Hand
//!   table with its hole at cell 3), at (column 1, row 1); Left from its column 0 goes back; Down / Up inside it wrap over its 2 rows.
//! - Ring cursor word [0x1a90cc] + 0x3ad3c (0..7): L1 -1, R1 +1 (wraps). Cross advances it by one, clears the item from any other
//!   ring cell and writes it into the cell (notes 9.2).
//! - Ring cells: array pointer in the boot word 0x1a91a0, 8 cells of 0x1c bytes, +0 icon id (0xea60 + n, 0 empty), +0x18 ITEM id.
//!   Slots hold weapons and hand gadgets (level 6: 30, 42, 36 Dynamo, 46 Tractor Beam, 26, 23, 13 Swingshot, 24).
use crate::modern_grid::{find_items, press_toward, Cell, Tables, OWNED};
use crate::modern_screen::{decode, site_id, Texts, BOLTS, DOWN, LEFT, LEVEL, PAUSE_IDS, RIGHT};
use rac_engine::input::binds::{BIT, CROSS, L1, R1, TRIANGLE};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

pub const PAGE_QS: u32 = 5;
pub const CURSOR: u32 = 0x3ad80;
pub const RING_CURSOR: u32 = 0x3ad3c;
pub const RING_PTR: u32 = 0x1a91a0;
const RING_STRIDE: u32 = 0x1c;
const REC_SIZE: u32 = 0xe0;
pub const WEAPON_COLS: usize = 4;
pub const HAND_COLS: usize = 3;
/// The cell tables by item id (0 = a hole), cell order (the same lists `modern_grid` finds).
const WEAPONS: [u32; 24] = [30, 42, 22, 24, 23, 41, 26, 45, 31, 29, 37, 27, 28, 25, 32, 16, 18, 12, 53, 14, 17, 43, 44, 9];
const HAND: [u32; 6] = [13, 36, 39, 0, 46, 55];
pub const T_HAND: u32 = 0xb68;
pub const T_EQUIP: u32 = 0xb6c;
const MENU_BTNS: u16 = crate::modern_open::MENU_BTNS | BIT[L1] | BIT[R1];

/// One of the ring's eight slots.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Slot {
	pub icon: u32,
	pub item: u32,
	pub name: String,
}

/// What the modern Quick Select page shows this field.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Facts {
	/// The weapons grid (4 x 6) and the Hand gadgets grid (3 x 2, cell 3 a hole).
	pub weapons: Vec<Cell>,
	pub hand: Vec<Cell>,
	pub hand_label: String,
	/// The game's grid cursor: (group 0 weapons / 1 hand, cell index in the group).
	pub sel: (usize, usize),
	pub ring: Vec<Slot>,
	pub ring_cursor: u32,
	pub tabs: Vec<String>,
	pub site: String,
	pub bolts: i32,
	pub equip: String,
}

impl Facts {
	pub fn cell(&self, g: usize, k: usize) -> Option<&Cell> {
		if g == 0 { self.weapons.get(k) } else { self.hand.get(k) }
	}

	/// The slot Cross fills (the retail highlight): one past the ring cursor.
	pub fn ring_target(&self) -> u32 {
		(self.ring_cursor + 1) % 8
	}

	/// Is this item sitting in one of the ring's slots?
	pub fn in_ring(&self, item: u32) -> bool {
		item != 0 && self.ring.iter().any(|s| s.item == item)
	}
}

fn rd(ram: &[u8], a: u32) -> u32 {
	let a = (a & 0x1ff_fffc) as usize;
	ram.get(a..a + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn rd16(ram: &[u8], a: u32) -> u32 {
	let a = (a & 0x1ff_fffe) as usize;
	ram.get(a..a + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]) as u32)
}

fn rd8(ram: &[u8], a: u32) -> u32 {
	ram.get((a & 0x1ff_ffff) as usize).map_or(0, |&b| b as u32)
}

/// Is the Quick Select page up? Page id, the parent record's id, the menu manager word (0 = no game). Pure.
pub fn gate(page_id: u32, parent_id: u32, manager: u32) -> bool {
	page_id == PAGE_QS && parent_id == 2 && manager != 0
}

/// The grid cursor words as (group, column, row, item) -> (group, cell index) when they agree with the cell tables; None otherwise.
pub fn sel_of(widget: u32, col: u32, row: u32, group: u32, item: u32) -> Option<(usize, usize)> {
	if widget == 0 {
		return None;
	}
	let (col, row) = (col as usize, row as usize);
	match group {
		0 if col < WEAPON_COLS && row < WEAPONS.len() / WEAPON_COLS => {
			let k = row * WEAPON_COLS + col;
			(WEAPONS[k] == item).then_some((0, k))
		}
		1 if col < HAND_COLS && row < HAND.len() / HAND_COLS => {
			let k = row * HAND_COLS + col;
			(HAND[k] == item && item != 0).then_some((1, k))
		}
		_ => None,
	}
}

#[derive(Default)]
struct Game {
	texts: Option<Texts>,
	tables: Option<Tables>,
	items: Option<u32>,
	retry: u32,
	walk: QWalk,
	seq: u32,
	/// [PC] nav lane: the pad returned last field (the edge rule of `filter`).
	last: u16,
}

impl Game {
	fn locate(&mut self, ram: &[u8]) -> Option<(Texts, Tables, u32)> {
		if !self.texts.is_some_and(|t| t.still(ram)) {
			self.texts = None;
		}
		if !self.tables.is_some_and(|t| t.still(ram)) {
			self.tables = None;
		}
		if !self.items.is_some_and(|a| item_ok(ram, a)) {
			self.items = None;
		}
		if self.texts.is_none() || self.tables.is_none() || self.items.is_none() {
			if self.retry > 0 {
				self.retry -= 1;
				return None;
			}
			self.texts = self.texts.or_else(|| Texts::find(ram));
			self.tables = self.tables.or_else(|| Tables::find(ram));
			self.items = self.items.or_else(|| find_items(ram));
			if self.texts.is_none() || self.tables.is_none() || self.items.is_none() {
				self.retry = 50;
			}
		}
		Some((self.texts?, self.tables?, self.items?))
	}
}

fn item_ok(ram: &[u8], a: u32) -> bool {
	rd16(ram, a + 22 * REC_SIZE + 8) == 0xa78 && rd(ram, a + 22 * REC_SIZE + 0x80) == 5000 && rd16(ram, a + 26 * REC_SIZE + 8) == 0xa7a && rd16(ram, a + 39 * REC_SIZE + 8) == 0xa96
}

fn rec_of(ram: &[u8], items: u32, item: u32) -> u32 {
	let slot = rd8(ram, crate::modern_vendor::SLOT_MAP + item);
	items + if slot == 0 { item } else { slot } * REC_SIZE
}

fn name_of(ram: &[u8], items: u32, texts: &Texts, item: u32) -> String {
	if item == 0 {
		return String::new();
	}
	texts.get(ram, rd16(ram, rec_of(ram, items, item) + 8)).unwrap_or_default()
}

fn cell_of(ram: &[u8], items: u32, texts: &Texts, weapon: bool, item: u32, icon: u32) -> Cell {
	if item == 0 {
		return Cell::default();
	}
	let owned = rd8(ram, OWNED + item) != 0;
	let max = rd16(ram, rec_of(ram, items, item) + 0x8e);
	let ammo = (weapon && owned && max > 0).then(|| (rd(ram, crate::modern_vendor::AMMO + 4 * item), max));
	Cell { item, owned, name: name_of(ram, items, texts, item), ammo, icon, extra: Default::default() }
}

/// Where the ring's array pointer is this field: `RING_PTR` moved as far as the level's menu manager word moved from 0x1a90cc
/// (`Snap::base`; level 1 keeps both 0x40 higher [C RAM of fg1 0025 with Quick Select up, 2026-11-12: 0x1a91a0 = 0, 0x1a91e0 = the
/// ring]); with the fixed word the page stayed classic on Oozla.
pub fn ring_ptr_at(base: u32) -> u32 {
	RING_PTR.wrapping_add(base.wrapping_sub(crate::modern_screen::MANAGER))
}

/// The ring's eight slots (None: the array pointer does not look like RAM).
fn ring_of(ram: &[u8], items: u32, texts: &Texts, at: u32) -> Option<Vec<Slot>> {
	let p = rd(ram, at);
	if !(0x10_0000..0x1ff_0000).contains(&p) {
		return None;
	}
	Some((0..8).map(|k| {
		let a = p + RING_STRIDE * k;
		let item = rd(ram, a + 0x18);
		Slot { icon: rd16(ram, a), item, name: name_of(ram, items, texts, item) }
	}).collect())
}

/// The page of this field, or None (not up, or facts that do not hold: the page then stays the game's own).
fn read(ram: &[u8], g: &mut Game) -> Option<Facts> {
	let s = crate::modern_open::snap_now(ram)?;
	let manager = crate::modern_screen::manager(ram, &s);
	if !gate(s.page_id, rd(ram, rd(ram, s.page + 0xe0).wrapping_add(0xe4)), manager) || !READY.load(Ordering::Relaxed) || !crate::modern_open::modern() {
		return None;
	}
	let (texts, tables, items) = g.locate(ram)?;
	crate::modern_grid_icons::want(rd(ram, LEVEL));
	let c = manager.wrapping_add(CURSOR);
	let sel = sel_of(rd(ram, c), rd(ram, c + 4), rd(ram, c + 8), rd(ram, c + 12), rd(ram, c + 16))?;
	let ring_cursor = rd(ram, manager.wrapping_add(RING_CURSOR));
	if ring_cursor > 7 {
		return None;
	}
	let ring = ring_of(ram, items, &texts, ring_ptr_at(s.base))?;
	let icon = |base: u32, k: usize, it: u32| if it == 0 { 0 } else { rd16(ram, base + 10 * k as u32) };
	let weapons = WEAPONS.iter().enumerate().map(|(k, &i)| cell_of(ram, items, &texts, true, i, icon(tables.weapons, k, i))).collect();
	let hand = HAND.iter().enumerate().map(|(k, &i)| cell_of(ram, items, &texts, false, i, icon(tables.gadgets[0], k, i))).collect();
	let tabs = PAUSE_IDS.iter().map(|&id| texts.get(ram, id).filter(|t| !t.is_empty())).collect::<Option<Vec<_>>>()?;
	let site = site_id(rd(ram, LEVEL)).and_then(|id| texts.get(ram, id)).unwrap_or_default();
	let equip = texts.get(ram, T_EQUIP).map(|t| decode(t.as_bytes())).unwrap_or_default();
	Some(Facts { weapons, hand, hand_label: texts.get(ram, T_HAND).unwrap_or_default(), sel, ring, ring_cursor, tabs, site, bolts: rd(ram, BOLTS) as i32, equip })
}

/// The next D-pad press that brings the grid cursor from `cur` toward `to` (group, cell). Inside a group: column, then row (the
/// game wraps, the walk does not use it). Weapons -> Hand: down to row 5, right to column 3, Right again enters the Hand grid;
/// Hand -> weapons: Left (out of column 0).
pub fn toward(cur: (usize, usize), to: (usize, usize)) -> Option<u16> {
	if cur == to {
		return None;
	}
	if cur.0 == to.0 {
		return press_toward(&[WEAPON_COLS, HAND_COLS], cur, to);
	}
	if cur.0 == 0 {
		return Some(if cur.1 / WEAPON_COLS < 5 { DOWN } else { RIGHT });
	}
	Some(LEFT)
}

/// How to turn the ring cursor from `cursor` to `slot`: (true = R1, how many presses), the short way round (a tie goes to R1).
pub fn ring_steps(cursor: u32, slot: u32) -> (bool, u32) {
	let d = (slot % 8 + 8 - cursor % 8) % 8;
	if d <= 4 { (true, d) } else { (false, 8 - d) }
}

/// The window's request: walk the grid cursor to `grid`, turn the ring to `ring`, press Cross (`pick`) or Triangle (`back`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Req {
	pub grid: Option<(usize, usize)>,
	/// The ring SLOT Cross should fill (the highlighted one).
	pub ring: Option<u32>,
	pub pick: bool,
	pub back: bool,
}

/// The pulses of one request: two fields pressed, two released, so every press is a new edge. The game's cursors are read back
/// before every press, so its own wrapping decides.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QWalk {
	pub req: Req,
	phase: u8,
	held: u16,
	sent: u8,
	/// [PC] nav lane: the grid cursor words ([manager] + `CURSOR`; 0 = presses only): a move is one field (modern_jump.rs).
	pub at: u32,
}

const MAX_PRESSES: u8 = 24;
/// [PC] nav lane: the page's tab in the pause root (modern_tabs.rs; modern_qs_page.rs draws the same).
pub const TAB: usize = 2;

impl QWalk {
	pub fn step(&mut self, f: &Facts) -> u16 {
		if self.phase == 0 {
			self.held = self.next(f);
			if self.held == 0 {
				return 0;
			}
		}
		let out = if self.phase < 2 { self.held } else { 0 };
		self.phase = (self.phase + 1) % 4;
		out
	}

	fn next(&mut self, f: &Facts) -> u16 {
		let r = &mut self.req;
		if std::mem::take(&mut r.back) {
			*r = Req::default();
			return BIT[TRIANGLE];
		}
		if let Some(slot) = r.ring {
			let (fwd, n) = ring_steps(f.ring_cursor, (slot + 7) % 8); // Cross fills slot cursor + 1
			if n > 0 && self.sent < MAX_PRESSES {
				self.sent += 1;
				return if fwd { BIT[R1] } else { BIT[L1] };
			}
			r.ring = None;
		}
		if let Some(to) = r.grid {
			if let Some(p) = toward(f.sel, to).filter(|_| self.sent < MAX_PRESSES) {
				self.sent += 1;
				return jump(self.at, f.sel, to).unwrap_or(p);
			}
			r.grid = None;
			if std::mem::take(&mut r.pick) && f.sel == to {
				self.sent = 0;
				return BIT[CROSS];
			}
		}
		if std::mem::take(&mut r.pick) {
			self.sent = 0;
			return BIT[CROSS];
		}
		self.sent = 0;
		0
	}

	pub fn busy(&self) -> bool {
		self.req != Req::default() || self.phase != 0
	}
}

/// [PC] nav lane: ONE press toward `to` from the cell next to it (modern_jump.rs `grid_preset`), the cursor words {column, row,
/// group, item} at `at` written to that cell first. Another group: Right from the weapons' last cell (3, 5) enters the Hand grid,
/// Left from the Hand's first cell leaves it (`toward`); the next press is then a jump inside the group. None = press as before.
fn jump(at: u32, cur: (usize, usize), to: (usize, usize)) -> Option<u16> {
	let group = |g: usize| -> (usize, &'static [u32]) { if g == 0 { (WEAPON_COLS, &WEAPONS[..]) } else { (HAND_COLS, &HAND[..]) } };
	if at == 0 || to.0 > 1 || cur.0 > 1 {
		return None;
	}
	let ((g, k), p) = if cur.0 == to.0 {
		let (cols, items) = group(to.0);
		let (k, p) = crate::modern_jump::grid_preset(cols, items.len(), Some(cur.1), to.1, |k| items[k] != 0)?;
		((to.0, k), p)
	} else if cur.0 == 0 {
		((0, WEAPONS.len() - 1), RIGHT)
	} else {
		((1, 0), LEFT)
	};
	if (g, k) != cur {
		let (cols, items) = group(g);
		for (o, v) in [(4, (k % cols) as u32), (8, (k / cols) as u32), (12, g as u32), (16, items[k])] {
			crate::modern_jump::queue(at.wrapping_add(o), v);
		}
	}
	Some(p)
}

static GAME: Mutex<Option<Game>> = Mutex::new(None);
static FACTS: Mutex<Option<Facts>> = Mutex::new(None);
static REQ: Mutex<Option<Req>> = Mutex::new(None);
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

/// [PC] nav lane: the cell a mouse request is taking the grid cursor to (drawn selected at once; modern_jump.rs `Pend`).
static PEND: crate::modern_jump::Pend<(usize, usize)> = crate::modern_jump::Pend::new();

pub fn pending() -> Option<(usize, usize)> {
	PEND.get()
}

/// Window thread: a mouse request (see `Req`).
pub fn request(r: Req) {
	*lock(&REQ) = Some(r);
	PEND.set(r.grid.filter(|_| !r.back));
	SEQ.fetch_add(1, Ordering::Relaxed);
}

/// Game thread, once per field after `modern_grid::filter`: the buttons the game gets. Without a modern Quick Select page the pad is
/// returned unchanged, bit for bit.
pub fn filter(ram: &[u8], b: u16, s: [u8; 4]) -> (u16, [u8; 4]) {
	let mut gg = lock(&GAME);
	let g = gg.get_or_insert_with(Game::default);
	let f = if rac_engine::ui::is_open() { None } else { read(ram, g) };
	let Some(f) = f else {
		*lock(&FACTS) = None;
		(g.walk, g.seq, g.last) = (QWalk::default(), SEQ.load(Ordering::Relaxed), b);
		PEND.set(None);
		return (b, s);
	};
	let seq = SEQ.load(Ordering::Relaxed);
	if seq != g.seq {
		g.seq = seq;
		if let Some(req) = lock(&REQ).take() {
			g.walk = QWalk { req, at: crate::modern_jump::at(crate::modern_jump::manager(ram), CURSOR), ..QWalk::default() };
		}
	}
	// [PC] nav lane: a tab clicked on the tab bar (modern_tabs.rs): back to the root first.
	if crate::modern_tabs::want_back(TAB) {
		g.walk = QWalk { req: Req { back: true, ..Req::default() }, ..QWalk::default() };
	}
	let mut out = b;
	if g.walk.busy() {
		// [PC] nav lane: a press held last field is no new edge (a new request mid-pulse): wait one field.
		let w = if g.walk.phase == 0 && g.last & MENU_BTNS != 0 { 0 } else { g.walk.step(&f) };
		out = (out & !MENU_BTNS) | w;
	}
	g.last = out;
	if SEQ.load(Ordering::Relaxed) == g.seq {
		PEND.set(g.walk.req.grid);
	}
	*lock(&FACTS) = Some(f);
	(out, s)
}

/// [b45 P4A] B18: how long (ms) a requested ring slot stands in for retail's cursor while the walk to it still runs.
pub const ARROW_PENDING_MS: u64 = 1200;

/// The ring slot a click on an arrow asks for: one step from the slot the previous click asked for (`pending` = slot and the ms it
/// was asked) while that walk can still be running, else from retail's ring target `base`; so two fast clicks turn the ring twice.
pub fn arrow_target(pending: Option<(u32, u64)>, base: u32, fwd: bool, now: u64) -> u32 {
	let from = match pending {
		Some((slot, at)) if now.saturating_sub(at) < ARROW_PENDING_MS => slot,
		_ => base,
	};
	if fwd { (from + 1) % 8 } else { (from + 7) % 8 }
}

#[cfg(test)]
#[path = "modern_qs_tests.rs"]
mod tests;
