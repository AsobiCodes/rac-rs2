//! [PC] R&C2's modern Weapons and Gadgets pages, GAME-THREAD side (lane pages; R&C1's twin is rac1/modern_menu/pause_grid.rs +
//! pause_host/grid.rs): which grid page is up, the cells and the cursor read from RAM (`read`), and the pad the game gets while the
//! page is drawn (`filter`). The page itself is `modern_grid_page.rs` (pure builder), drawn by host/screens_grid.rs. Like the
//! other modern screens the retail page keeps running underneath: the player's pad reaches it untouched, the window's mouse
//! becomes D-pad / Cross / Triangle pulses (`Walk`), nothing is written to game RAM. Classic menu style, refused UI pipes or no
//! world behind the pause: `filter` returns the pad bit for bit and nothing is drawn.
//!
//! Facts [C] (notes_modern_screens_pages.md 8, RAM dumps of fg1 0279 / 0262 / 0110 / 0390, levels 6, 9, 26):
//! - page ids (record at [G+0x14], +0xe4): 3 = Weapons, 4 = Gadgets, parent = the pause root (id 2). 15 is not read here.
//! - Weapons cursor = [0x1a90cc] + 0x39b90: {1, column 0..3, row 0..5}; flat cell = row * 4 + column.
//! - Gadgets cursor = [0x1a90cc] + 0x3a098: {1, column, row, ITEM ID}; the id names the cell, so the group is found by it.
//! - Cell tables are resident level data whenever a level is loaded (also with the pause closed): 10-byte entries {u16 icon
//!   0xea60 + n, 0, 0, u16 ITEM ID at +6, 0}, in cell order. Weapons: 24 entries (4 x 6). Gadgets: three tables, Hand Items (3 x 2,
//!   entry 3 = no cell), Back Packs (2 x 1), Foot Items (3 x 1); the same on levels 6, 9 and 26, but at other addresses, so they
//!   are found by their item lists (`Tables::find`). The retail Down / Up from one group goes to the next / previous group.
//! - Item record table (0xe0 per record, `ItemTab::find`): +8 u16 name text id, +0x8e u16 max ammo. Owned byte 0x1a7b80 + item,
//!   ammo u32 0x139708 + 4 * item, slot -> record map byte 0x1395e8 + item (item 30 maps to 60 once the Lancer is upgraded: the
//!   cell's name is the mapped record's, "Heavy Lancer").
//! - Texts: 0xb68 Hand Items, 0xb69 Back Packs, 0xb6b Foot Items, 0xb6c Equip (with a button glyph), 0xbca Uses no ammo.
use crate::modern_screen::{decode, site_id, Texts, BOLTS, DOWN, LEFT, LEVEL, PAUSE_IDS, RIGHT, UP};
use rac_engine::input::binds::{BIT, CROSS, TRIANGLE};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

pub const PAGE_WEAPONS: u32 = 3;
pub const PAGE_GADGETS: u32 = 4;
pub const WEAPONS_CURSOR: u32 = 0x39b90;
pub const GADGETS_CURSOR: u32 = 0x3a098;
pub const OWNED: u32 = 0x1a7b80;
pub(crate) const REC_SIZE: u32 = 0xe0;
const T_GROUPS: [u32; 3] = [0xb68, 0xb69, 0xb6b];
pub const T_EQUIP: u32 = 0xb6c;
pub const T_NO_AMMO: u32 = 0xbca;

/// Cell tables by item id (0 = no cell), cell order.
const WEAPONS: [u32; 24] = [30, 42, 22, 24, 23, 41, 26, 45, 31, 29, 37, 27, 28, 25, 32, 16, 18, 12, 53, 14, 17, 43, 44, 9];
const HAND: [u32; 6] = [13, 36, 39, 0, 46, 55];
const BACK: [u32; 2] = [2, 3];
const FOOT: [u32; 3] = [20, 19, 54];
/// The Gadgets groups: item list and column count.
pub(crate) const GADGETS: [(&[u32], usize); 3] = [(&HAND, 3), (&BACK, 2), (&FOOT, 3)];
pub const WEAPON_COLS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Page {
	#[default]
	Weapons,
	Gadgets,
}

impl Page {
	/// The page's tab in the pause root (`PAUSE_IDS` index).
	pub fn tab(self) -> usize {
		match self {
			Page::Weapons => 0,
			Page::Gadgets => 1,
		}
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Cell {
	/// The item id; 0 = no cell (a hole in the group's shape).
	pub item: u32,
	pub owned: bool,
	pub name: String,
	/// Weapons: (held, max); None = no ammo.
	pub ammo: Option<(u32, u32)>,
	/// The icon sheet id (0xea60 + n) from the cell table; 0 = none.
	pub icon: u32,
	/// Weapons: tint, bar, empty flag and mod rows read from the retail draw rules (`modern_grid_extra.rs`).
	pub extra: crate::modern_grid_extra::Extra,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Group {
	pub label: String,
	pub cols: usize,
	pub cells: Vec<Cell>,
}

impl Group {
	pub fn rows(&self) -> usize {
		self.cells.len().div_ceil(self.cols.max(1))
	}
}

/// What the modern grid page shows this field.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Facts {
	pub page: Page,
	pub groups: Vec<Group>,
	/// The game's cursor: (group, cell index in it).
	pub sel: (usize, usize),
	/// The pause root's tab words, the planet and the bolts (the shell).
	pub tabs: Vec<String>,
	pub site: String,
	pub bolts: i32,
	/// The game's own "Equip" word.
	pub equip: String,
	pub no_ammo: String,
	pub level: u32,
}

impl Facts {
	pub fn cell(&self, g: usize, k: usize) -> Option<&Cell> {
		self.groups.get(g)?.cells.get(k)
	}
}

pub(crate) fn rd(ram: &[u8], a: u32) -> u32 {
	let a = (a & 0x1ff_fffc) as usize;
	ram.get(a..a + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

pub(crate) fn rd16(ram: &[u8], a: u32) -> u32 {
	let a = (a & 0x1ff_fffe) as usize;
	ram.get(a..a + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]) as u32)
}

pub(crate) fn rd8(ram: &[u8], a: u32) -> u32 {
	ram.get((a & 0x1ff_ffff) as usize).map_or(0, |&b| b as u32)
}

/// Which grid page is up: page id, the parent record's id, the menu manager word (0 = no game). Pure.
pub fn gate(page_id: u32, parent_id: u32, manager: u32) -> Option<Page> {
	if parent_id != 2 || manager == 0 {
		return None;
	}
	match page_id {
		PAGE_WEAPONS => Some(Page::Weapons),
		PAGE_GADGETS => Some(Page::Gadgets),
		_ => None,
	}
}

/// Does the table at `a` hold exactly `items` (entries {icon, 0, 0, item, 0}; a 0 item is an empty entry)?
pub(crate) fn table_at(ram: &[u8], a: u32, items: &[u32]) -> bool {
	items.iter().enumerate().all(|(k, &it)| {
		let e = a + 10 * k as u32;
		let icon = rd16(ram, e);
		let shape = rd16(ram, e + 2) == 0 && rd16(ram, e + 4) == 0 && rd16(ram, e + 8) == 0;
		shape && rd16(ram, e + 6) == it && if it == 0 { icon == 0 } else { (0xea60..0xeb10).contains(&icon) }
	})
}

/// Where the cell tables are in this level's RAM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tables {
	pub weapons: u32,
	pub gadgets: [u32; 3],
}

impl Tables {
	/// Scans RAM for the four item lists (2-byte aligned: the entries are 10 bytes).
	pub fn find(ram: &[u8]) -> Option<Tables> {
		let (mut w, mut g) = (None, [None; 3]);
		let top = (ram.len() as u32).min(0x200_0000).saturating_sub(250);
		let mut a = 0x10_0000u32;
		while a < top {
			let it = rd16(ram, a + 6);
			if it == WEAPONS[0] && w.is_none() && table_at(ram, a, &WEAPONS) {
				w = Some(a);
			}
			for (i, (items, _)) in GADGETS.iter().enumerate() {
				if it == items[0] && g[i].is_none() && table_at(ram, a, items) {
					g[i] = Some(a);
				}
			}
			if w.is_some() && g.iter().all(|x| x.is_some()) {
				break;
			}
			a += 2;
		}
		Some(Tables { weapons: w?, gadgets: [g[0]?, g[1]?, g[2]?] })
	}

	pub fn still(&self, ram: &[u8]) -> bool {
		table_at(ram, self.weapons, &WEAPONS) && self.gadgets.iter().zip(GADGETS).all(|(&a, (items, _))| table_at(ram, a, items))
	}
}

/// The item record table (0xe0 per record): found by records 22 (price 5000, name 0xa78), 26 (0xa7a) and 39 (0xa96).
pub fn find_items(ram: &[u8]) -> Option<u32> {
	let top = (ram.len() as u32).min(0x50_0000);
	(0x10_0000..top.saturating_sub(40 * REC_SIZE)).step_by(4).find(|&a| item_tab_ok(ram, a))
}

pub(crate) fn item_tab_ok(ram: &[u8], a: u32) -> bool {
	rd16(ram, a + 22 * REC_SIZE + 8) == 0xa78 && rd(ram, a + 22 * REC_SIZE + 0x80) == 5000 && rd16(ram, a + 26 * REC_SIZE + 8) == 0xa7a && rd16(ram, a + 39 * REC_SIZE + 8) == 0xa96
}

#[derive(Default)]
struct Game {
	texts: Option<Texts>,
	tables: Option<Tables>,
	items: Option<u32>,
	/// Fields until the next scan after a failed one.
	retry: u32,
	walk: Walk,
	prev: u16,
	seq: u32,
	/// [PC] nav lane: the pad returned last field (the edge rule of `filter`).
	last: u16,
}

impl Game {
	/// The scans' results, kept while they hold.
	fn locate(&mut self, ram: &[u8]) -> Option<(Texts, Tables, u32)> {
		if !self.texts.is_some_and(|t| t.still(ram)) {
			self.texts = None;
		}
		if !self.tables.is_some_and(|t| t.still(ram)) {
			self.tables = None;
		}
		if !self.items.is_some_and(|a| item_tab_ok(ram, a)) {
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

fn cell_of(ram: &[u8], items: u32, texts: &Texts, page: Page, item: u32, icon: u32) -> Cell {
	if item == 0 {
		return Cell::default();
	}
	let slot = rd8(ram, crate::modern_vendor::SLOT_MAP + item);
	let rec = items + if slot == 0 { item } else { slot } * REC_SIZE;
	let owned = rd8(ram, OWNED + item) != 0;
	let name = texts.get(ram, rd16(ram, rec + 8)).unwrap_or_default();
	let max = rd16(ram, rec + 0x8e);
	let ammo = (page == Page::Weapons && owned && max > 0).then(|| (rd(ram, crate::modern_vendor::AMMO + 4 * item), max));
	let extra = if page == Page::Weapons { crate::modern_grid_extra::extra(ram, items, texts, item, rec, owned) } else { Default::default() };
	// Retail draws the MAPPED record's icon (+0x3c) in the weapon grid; the cell table's id is the fallback.
	let icon = if page == Page::Weapons && owned && extra.icon != 0 { extra.icon } else { icon };
	Cell { item, owned, name, ammo, icon, extra }
}

/// The page of this field, or None (no grid page up, or facts that do not hold: the page then stays the game's own).
fn read(ram: &[u8], g: &mut Game) -> Option<Facts> {
	let s = crate::modern_open::snap_now(ram)?;
	let manager = crate::modern_screen::manager(ram, &s);
	let page = gate(s.page_id, rd(ram, rd(ram, s.page + 0xe0).wrapping_add(0xe4)), manager)?;
	if !READY.load(Ordering::Relaxed) || !crate::modern_open::modern() {
		return None;
	}
	let (texts, tables, items) = g.locate(ram)?;
	let level = rd(ram, LEVEL);
	crate::modern_grid_icons::want(level);
	let cell = |base: u32, k: usize, it| cell_of(ram, items, &texts, page, it, if it == 0 { 0 } else { rd16(ram, base + 10 * k as u32) });
	let (groups, sel) = match page {
		Page::Weapons => {
			let c = rd(ram, manager.wrapping_add(WEAPONS_CURSOR));
			let (col, row) = (rd(ram, manager.wrapping_add(WEAPONS_CURSOR + 4)) as usize, rd(ram, manager.wrapping_add(WEAPONS_CURSOR + 8)) as usize);
			if c != 1 || col >= WEAPON_COLS || row >= WEAPONS.len() / WEAPON_COLS {
				return None;
			}
			(vec![Group { label: String::new(), cols: WEAPON_COLS, cells: WEAPONS.iter().enumerate().map(|(k, &i)| cell(tables.weapons, k, i)).collect() }], (0, row * WEAPON_COLS + col))
		}
		Page::Gadgets => {
			let c = rd(ram, manager.wrapping_add(GADGETS_CURSOR));
			let id = rd(ram, manager.wrapping_add(GADGETS_CURSOR + 12));
			let at = GADGETS.iter().enumerate().find_map(|(gi, (its, _))| its.iter().position(|&i| i == id && id != 0).map(|k| (gi, k)))?;
			if c != 1 {
				return None;
			}
			let groups = GADGETS.iter().enumerate().map(|(gi, (its, cols))| Group { label: texts.get(ram, T_GROUPS[gi]).unwrap_or_default(), cols: *cols, cells: its.iter().enumerate().map(|(k, &i)| cell(tables.gadgets[gi], k, i)).collect() });
			(groups.collect(), at)
		}
	};
	let tabs = PAUSE_IDS.iter().map(|&id| texts.get(ram, id).filter(|t| !t.is_empty())).collect::<Option<Vec<_>>>()?;
	let site = site_id(rd(ram, LEVEL)).and_then(|id| texts.get(ram, id)).unwrap_or_default();
	let equip = texts.get(ram, T_EQUIP).map(|t| decode(t.as_bytes())).unwrap_or_default();
	let no_ammo = texts.get(ram, T_NO_AMMO).unwrap_or_default();
	Some(Facts { page, groups, sel, tabs, site, bolts: rd(ram, BOLTS) as i32, equip, no_ammo, level })
}

/// The pulses that bring the game's cursor to a cell (and equip it / go back): two fields pressed, two released, so every press is
/// a new edge. The game's cursor is read back before every press, so its own wrapping and group jumps decide.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Walk {
	pub target: Option<(usize, usize)>,
	pub confirm: bool,
	pub back: bool,
	phase: u8,
	held: u16,
	/// Presses sent for this target (the walk gives up after `MAX_PRESSES`: a press the game did not take).
	sent: u8,
	/// [PC] nav lane: the page's cursor words ([manager] + `WEAPONS_CURSOR` / `GADGETS_CURSOR`; 0 = presses only): a move inside a
	/// group is one field (modern_jump.rs).
	pub at: u32,
}

const MAX_PRESSES: u8 = 14;

/// The next press from `cur` toward `to` (group, cell) in `groups` (columns per group): groups first (Down = the next group), then the
/// column, then the row.
pub fn press_toward(cols: &[usize], cur: (usize, usize), to: (usize, usize)) -> Option<u16> {
	if cur == to {
		return None;
	}
	if cur.0 != to.0 {
		return Some(if to.0 > cur.0 { DOWN } else { UP });
	}
	let c = cols.get(cur.0).copied().unwrap_or(1).max(1);
	let ((cc, cr), (tc, tr)) = ((cur.1 % c, cur.1 / c), (to.1 % c, to.1 / c));
	if cc != tc {
		return Some(if tc > cc { RIGHT } else { LEFT });
	}
	Some(if tr > cr { DOWN } else { UP })
}

impl Walk {
	/// One field: the menu buttons to press for the game's cursor `cur` in `f`'s shape.
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
		if std::mem::take(&mut self.back) {
			self.target = None;
			return BIT[TRIANGLE];
		}
		let Some(to) = self.target else { return 0 };
		let cols: Vec<usize> = f.groups.iter().map(|g| g.cols).collect();
		if let Some(p) = press_toward(&cols, f.sel, to).filter(|_| self.sent < MAX_PRESSES) {
			self.sent += 1;
			return self.jump(f, to).unwrap_or(p);
		}
		self.target = None;
		self.sent = 0;
		if std::mem::take(&mut self.confirm) && f.sel == to {
			return BIT[CROSS];
		}
		self.confirm = false;
		0
	}

	/// [PC] nav lane: ONE press from the cell next to `to` (modern_jump.rs `grid_preset`), the cursor words written to that cell
	/// first (Gadgets: also its item id); None = press as before (no cursor word, another group: the group word is not known).
	fn jump(&self, f: &Facts, to: (usize, usize)) -> Option<u16> {
		if self.at == 0 || f.sel.0 != to.0 {
			return None;
		}
		let g = f.groups.get(to.0)?;
		let (pre, p) = crate::modern_jump::grid_preset(g.cols, g.cells.len(), Some(f.sel.1), to.1, |k| g.cells[k].item != 0)?;
		if pre != f.sel.1 {
			let c = g.cols.max(1);
			crate::modern_jump::queue(self.at.wrapping_add(4), (pre % c) as u32);
			crate::modern_jump::queue(self.at.wrapping_add(8), (pre / c) as u32);
			if f.page == Page::Gadgets {
				crate::modern_jump::queue(self.at.wrapping_add(12), g.cells[pre].item);
			}
		}
		Some(p)
	}

	pub fn busy(&self) -> bool {
		self.target.is_some() || self.back || self.phase != 0
	}
}

static GAME: Mutex<Option<Game>> = Mutex::new(None);
static FACTS: Mutex<Option<Facts>> = Mutex::new(None);
/// The window's request: (group, cell), equip, back, with a sequence number.
static REQ: Mutex<Option<((usize, usize), bool, bool)>> = Mutex::new(None);
static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// The window can draw the page (UI pipes fine, the world drawn behind it).
static READY: AtomicBool = AtomicBool::new(false);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn ready() -> bool {
	READY.load(Ordering::Relaxed)
}

pub fn set_ready(ok: bool) {
	READY.store(ok, Ordering::Relaxed);
}

pub fn facts() -> Option<Facts> {
	lock(&FACTS).clone()
}

/// [PC] nav lane: the cell a mouse request is taking the cursor to (drawn selected at once; modern_jump.rs `Pend`).
static PEND: crate::modern_jump::Pend<(usize, usize)> = crate::modern_jump::Pend::new();

pub fn pending() -> Option<(usize, usize)> {
	PEND.get()
}

/// Window thread: walk the game's cursor to (group, cell) and equip it (`pick`); `back` = Triangle.
pub fn request(to: (usize, usize), pick: bool, back: bool) {
	*lock(&REQ) = Some((to, pick, back));
	PEND.set((!back).then_some(to));
	SEQ.fetch_add(1, Ordering::Relaxed);
}

/// Game thread, once per field after `modern_vendor::filter`: the buttons the game gets. Without a modern grid page the pad is
/// returned unchanged, bit for bit.
pub fn filter(ram: &[u8], b: u16, s: [u8; 4]) -> (u16, [u8; 4]) {
	let mut gg = lock(&GAME);
	let g = gg.get_or_insert_with(Game::default);
	g.prev = b;
	let f = if rac_engine::ui::is_open() { None } else { read(ram, g) };
	let Some(f) = f else {
		*lock(&FACTS) = None;
		(g.walk, g.seq, g.last) = (Walk::default(), SEQ.load(Ordering::Relaxed), b);
		PEND.set(None);
		drop(gg);
		return crate::modern_items::filter(ram, b, s); // the Items page (modern_items.rs); the pad unchanged without it
	};
	let seq = SEQ.load(Ordering::Relaxed);
	if seq != g.seq {
		g.seq = seq;
		if let Some((to, confirm, back)) = lock(&REQ).take() {
			let off = if f.page == Page::Weapons { WEAPONS_CURSOR } else { GADGETS_CURSOR };
			g.walk = Walk { target: Some(to), confirm, back, at: crate::modern_jump::at(crate::modern_jump::manager(ram), off), ..Walk::default() };
		}
	}
	// [PC] nav lane: a tab clicked on the tab bar (modern_tabs.rs): back to the root first.
	if crate::modern_tabs::want_back(f.page.tab()) {
		g.walk = Walk { back: true, ..Walk::default() };
	}
	let mut out = b;
	if g.walk.busy() {
		// [PC] nav lane: a press held last field is no new edge (a new request mid-pulse): wait one field.
		let w = if g.walk.phase == 0 && g.last & crate::modern_open::MENU_BTNS != 0 { 0 } else { g.walk.step(&f) };
		out = (out & !crate::modern_open::MENU_BTNS) | w;
	}
	g.last = out;
	if SEQ.load(Ordering::Relaxed) == g.seq {
		PEND.set(g.walk.target.filter(|_| !g.walk.back));
	}
	*lock(&FACTS) = Some(f);
	(out, s)
}

#[cfg(test)]
#[path = "modern_grid_tests.rs"]
mod tests;
