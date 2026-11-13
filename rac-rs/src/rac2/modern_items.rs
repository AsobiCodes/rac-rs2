//! [PC] R&C2's modern ITEMS page, GAME-THREAD side (lane pages; twin of `modern_grid.rs`, which dispatches here for page id 6): the
//! cells and the cursor read from RAM (`read`), and the pad the game gets while the page is drawn (`filter`). The page itself is
//! `modern_items_page.rs`, drawn by host/screens_items.rs. Like the other modern screens the retail page keeps running underneath: the
//! player's pad reaches it untouched, the window's mouse becomes D-pad / Cross / Triangle pulses (`Walk`), nothing is written to game
//! RAM. Classic style, refused UI pipes or no world: `filter` returns the pad bit for bit.
//!
//! Facts [C] (notes_modern_screens_pages.md 8/9, RAM dumps fg1 0397 level 9 + user_2017 level 13; navigation run runs/o_nav):
//! - page id 6, parent = the pause root (2). Cursor = [0x1a90cc] + 0x3a558 {1, column, row, ITEM ID, GROUP}; group 0 Head (2 x 1: 47,
//!   48), 1 Armor (1 cell, item 6, shown as the worn armor: slot map 0x1395e8 + 6 -> record 56..59), 2 Items (4 x 2: 21 7 50 5 / 38 51
//!   4 8), 3 Collectibles (2 x 2: id = row * 2 + column: Platinum Bolt, Crystals, Moonstones, Raritanium; name text 0xad2 - id,
//!   description text 0xfd6 + id). Column / row restart in every group.
//! - Cell tables (10-byte entries as the Weapons table): Items = Weapons table + 0x4170, Armor = the entry run 6,56,57,58,59 in the
//!   per-level block 0x1aa000..0x1ac000, Head = Armor - 0x20. Item description text id = u16 at item record + 0x42, 0 or absent = none (ids 0xf10..0x10dd: Commando 0xf10, Megacorp Helmet 0x10dc, Biker Helmet 0x10dd; record = item
//!   table + slot * 0xe0, as for names at +8): Electrolyzer 0xfd3, Infiltrator 0xfd4, Levitator 0xfd5, Mapper 0xfd2, Box Breaker 0xfda;
//!   Hydro-Pack and the armor upgrades 56..59 have none. Raritanium counter u32 0x1a7a84 ("Found"); the Platinum Bolt Found / Used / Remain: see `platinum`.
//!   [b49 Z3 U8, C boot ELF decomp, Items page draw ~line 147355, format "%d" 0x1ae010] Crystals = u16 at 0x1a7aa2 (`DAT_001a7aa0._2_2_`),
//!   Moonstones = u32 at 0x1a7aa4 (`DAT_001a7aa0._4_4_`): the widths of `CRYSTALS` / `MOONSTONES` below are retail's. No bank state holds a
//!   Moonstone (all 581 read 0 at 0x1a7aa4); Crystals are 0..11 there.
//! - Down: Head -> Items row 0 -> row 1 -> Head; Up the reverse; Right from Items column 3 -> Collectibles (column 0, same row 0);
//!   Right in Collectibles wraps; Left from Collectibles column 0 -> Items (3, 1); Left from Items column 0 wraps to column 3; Down in
//!   Collectibles wraps; Up from Collectibles row 0 -> Items (3, 1); Right from Head column 1 -> Armor [notes 9.2; Armor moves [I]].
use crate::modern_grid as mg;
use crate::modern_screen::{decode, site_id, Texts, BOLTS, DOWN, LEFT, LEVEL, PAUSE_IDS, RIGHT, UP};
use rac_engine::input::binds::{BIT, CROSS, TRIANGLE};
use std::sync::Mutex;

pub const PAGE_ITEMS: u32 = 6;
const CURSOR: u32 = 0x3a558;
const ITEMS_FROM_WEAPONS: u32 = 0x4170;
const ARMOR_TABLE_LO: u32 = 0x1a_a000;
const ARMOR_TABLE_HI: u32 = 0x1a_c000;
const RARITANIUM: u32 = 0x1a7a84;
const CRYSTALS: u32 = 0x1a7aa2;
const MOONSTONES: u32 = 0x1a7aa4;
const PLAT_FLAGS: u32 = 0x19b2f8;
const PLAT_USED: u32 = 0x1396c8;
const PLAT_NAME_MODE: u32 = 0x1a7c3c;
const T_COLLECT_NAME: u32 = 0xad2;
const T_COLLECT_DESC: u32 = 0xfd6;
pub const HEAD: [u32; 2] = [47, 48];
pub const ARMOR: [u32; 5] = [6, 56, 57, 58, 59];
pub const ITEMS: [u32; 8] = [21, 7, 50, 5, 38, 51, 4, 8];
/// Columns per group, in group-word order.
pub const COLS: [usize; 4] = [2, 1, 4, 2];
pub const GROUP_LEN: [usize; 4] = [2, 1, 8, 4];

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ICell {
	/// The item id (collectibles: 0..3).
	pub item: u32,
	pub owned: bool,
	pub name: String,
	pub desc: String,
	/// The icon sheet id (0xea60 + n); 0 = none.
	pub icon: u32,
	/// Collectibles: the counter lines the retail panel prints, (label, value): Found / Used / Remain for the Platinum Bolt, Found for
	/// the others (`FUN_003469e8`, texts 0xbcc 0xbcd 0xbce).
	pub lines: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Facts {
	/// Head, Armor, Items, Collectibles.
	pub groups: Vec<Vec<ICell>>,
	/// The game's cursor: (group, cell index in it).
	pub sel: (usize, usize),
	pub tabs: Vec<String>,
	pub site: String,
	pub bolts: i32,
	pub level: u32,
}

impl Facts {
	pub fn cell(&self, g: usize, k: usize) -> Option<&ICell> {
		self.groups.get(g)?.get(k)
	}
}

/// Is the Items page up: page id, the parent record's id, the menu manager word. Pure.
pub fn gate(page_id: u32, parent_id: u32, manager: u32) -> bool {
	page_id == PAGE_ITEMS && parent_id == 2 && manager != 0
}

/// The press that moves the game's cursor from `at` (group, column, row) in direction `dir` (the retail wrapping, see the header). Pure.
pub fn step(at: (usize, usize, usize), dir: u16) -> (usize, usize, usize) {
	let (g, c, r) = at;
	match (g, dir) {
		(0, DOWN) => (2, c.min(3), 0),
		(0, UP) => (2, c.min(3), 1),
		(0, RIGHT) => if c == 0 { (0, 1, 0) } else { (1, 0, 0) },
		(0, LEFT) => if c == 1 { (0, 0, 0) } else { (1, 0, 0) },
		(1, DOWN) => (2, 3, 0),
		(1, UP) => (2, 3, 1),
		(1, RIGHT) => (0, 0, 0),
		(1, LEFT) => (0, 1, 0),
		// Items (FUN_003460a8): Down from row 1 = Head (cols 0 1), wraps in column 2, Collectibles (0,0) from column 3; Up from row 0 =
		// Head (cols 0 1), wraps in column 2, Armor from column 3; Right from column 3 = Collectibles (0,0).
		(2, DOWN) => match (r, c) {
			(0, _) => (2, c, 1),
			(_, 0 | 1) => (0, c, 0),
			(_, 2) => (2, 2, 0),
			_ => (3, 0, 0),
		},
		(2, UP) => match (r, c) {
			(1, _) => (2, c, 0),
			(_, 0 | 1) => (0, c, 0),
			(_, 2) => (2, 2, 1),
			_ => (1, 0, 0),
		},
		(2, RIGHT) => if c < 3 { (2, c + 1, r) } else { (3, 0, 0) },
		(2, LEFT) => (2, (c + 3) % 4, r),
		(3, DOWN) => (3, c, 1 - r),
		// Collectibles (FUN_00346210): Up from row 0 wraps to row 1 in column 1, leaves to Items (3,1) from column 0.
		(3, UP) => if r == 1 { (3, c, 0) } else if c == 0 { (2, 3, 1) } else { (3, c, 1) },
		(3, RIGHT) => (3, (c + 1) % 2, r),
		(3, LEFT) => if c == 1 { (3, 0, r) } else { (2, 3, 1) },
		_ => at,
	}
}

fn pos(g: usize, k: usize) -> (usize, usize, usize) {
	(g, k % COLS[g.min(3)], k / COLS[g.min(3)])
}

/// The first press of a shortest route (over `step`) from cell `cur` to cell `to`, None = there already or no route.
pub fn press_toward(cur: (usize, usize), to: (usize, usize)) -> Option<u16> {
	if cur == to || cur.0 > 3 || to.0 > 3 {
		return None;
	}
	let goal = pos(to.0, to.1);
	let mut seen = vec![pos(cur.0, cur.1)];
	let mut queue: Vec<((usize, usize, usize), u16)> = [UP, DOWN, LEFT, RIGHT].iter().map(|&d| (step(seen[0], d), d)).collect();
	let mut i = 0;
	while i < queue.len() {
		let (p, first) = queue[i];
		i += 1;
		if seen.contains(&p) {
			continue;
		}
		if p == goal {
			return Some(first);
		}
		seen.push(p);
		queue.extend([UP, DOWN, LEFT, RIGHT].iter().map(|&d| (step(p, d), first)));
	}
	None
}

/// Where the cell tables are in this level's RAM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Tables {
	head: u32,
	armor: u32,
	items: u32,
}

fn find_tables(ram: &[u8]) -> Option<Tables> {
	let w = mg::Tables::find(ram)?.weapons;
	let items = w + ITEMS_FROM_WEAPONS;
	let armor = (ARMOR_TABLE_LO..ARMOR_TABLE_HI).step_by(2).find(|&a| mg::table_at(ram, a, &ARMOR))?;
	let t = Tables { head: armor.checked_sub(0x20)?, armor, items };
	t.still(ram).then_some(t)
}

impl Tables {
	fn still(&self, ram: &[u8]) -> bool {
		mg::table_at(ram, self.items, &ITEMS) && mg::table_at(ram, self.armor, &ARMOR) && mg::table_at(ram, self.head, &HEAD)
	}
}

/// The text of `id` in full (descriptions are longer than `Texts::get` reads); line breaks become spaces.
pub(crate) fn long_text(ram: &[u8], t: &Texts, id: u32) -> String {
	text_in(ram, t.start, t.n, id)
}

fn text_in(ram: &[u8], start: u32, n: u32, id: u32) -> String {
	let Some(a) = (0..n).map(|k| start + 16 * k).find(|&a| mg::rd(ram, a + 4) == id) else { return String::new() };
	let p = (mg::rd(ram, a) & 0x1ff_ffff) as usize;
	let mut b: Vec<u8> = ram.get(p..(p + 700).min(ram.len())).unwrap_or(&[]).to_vec();
	for x in b.iter_mut() {
		if *x == 1 || *x == 0x0a {
			*x = b' ';
		}
	}
	decode(&b)
}

#[derive(Default)]
struct Game {
	texts: Option<Texts>,
	tables: Option<Tables>,
	items: Option<u32>,
	retry: u32,
	walk: Walk,
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
		if !self.items.is_some_and(|a| mg::item_tab_ok(ram, a)) {
			self.items = None;
		}
		if self.texts.is_none() || self.tables.is_none() || self.items.is_none() {
			if self.retry > 0 {
				self.retry -= 1;
				return None;
			}
			self.texts = self.texts.or_else(|| Texts::find(ram));
			self.tables = self.tables.or_else(|| find_tables(ram));
			self.items = self.items.or_else(|| mg::find_items(ram));
			if self.texts.is_none() || self.tables.is_none() || self.items.is_none() {
				self.retry = 50;
			}
		}
		Some((self.texts?, self.tables?, self.items?))
	}
}

fn item_cell(ram: &[u8], items: u32, texts: &Texts, item: u32, icon: u32) -> ICell {
	let slot = mg::rd8(ram, crate::modern_vendor::SLOT_MAP + item);
	let rec = items + if slot == 0 { item } else { slot } * mg::REC_SIZE;
	let desc_id = mg::rd16(ram, rec + 0x42);
	let desc = if desc_id != 0 { long_text(ram, texts, desc_id) } else { String::new() };
	ICell { item, owned: mg::rd8(ram, mg::OWNED + item) != 0, name: texts.get(ram, mg::rd16(ram, rec + 8)).unwrap_or_default(), desc, icon, lines: Vec::new() }
}

fn build_groups(ram: &[u8], texts: &Texts, t: &Tables, items: u32) -> Vec<Vec<ICell>> {
	let at = |base: u32, k: usize| mg::rd16(ram, base + 10 * k as u32);
	let head = HEAD.iter().enumerate().map(|(k, &i)| item_cell(ram, items, texts, i, at(t.head, k))).collect();
	// The armor cell shows the worn armor: the slot map's record, its own icon.
	let worn = mg::rd8(ram, crate::modern_vendor::SLOT_MAP + 6);
	let k = ARMOR.iter().position(|&a| a == worn).unwrap_or(0);
	let armor = vec![item_cell(ram, items, texts, 6, at(t.armor, k))];
	let grid = ITEMS.iter().enumerate().map(|(k, &i)| item_cell(ram, items, texts, i, at(t.items, k))).collect();
	let collect = (0..4u32).map(|id| collect_cell(ram, texts, id)).collect();
	vec![head, armor, grid, collect]
}

/// Platinum Bolts found: the non-zero bytes of the 112 flag bytes at 0x19b2f8, at most 40 (`FUN_002b1930`); used = byte 0x1396c8
/// (`FUN_002b18d8`); remaining = found - used, clamped 0..40 (`FUN_002b18e8`).
pub fn platinum(ram: &[u8]) -> (u32, u32, u32) {
	let found = (0..112u32).filter(|&k| mg::rd8(ram, PLAT_FLAGS + k) != 0).count().min(40) as u32;
	let used = mg::rd8(ram, PLAT_USED);
	(found, used, found.saturating_sub(used).min(40))
}

fn collect_cell(ram: &[u8], texts: &Texts, id: u32) -> ICell {
	let label = |t: u32| texts.get(ram, t).unwrap_or_default();
	let n = |v: u32| v.to_string();
	let lines = match id {
		0 => {
			let (f, u, r) = platinum(ram);
			vec![(label(0xbcc), n(f)), (label(0xbcd), n(u)), (label(0xbce), n(r))]
		}
		1 => vec![(label(0xbcc), n(mg::rd16(ram, CRYSTALS)))],
		2 => vec![(label(0xbcc), n(mg::rd(ram, MOONSTONES)))],
		_ => vec![(label(0xbcc), n(mg::rd(ram, RARITANIUM)))],
	};
	// The Platinum Bolt's name is the plural text 0xad3 while byte 0x1a7c3c is 2.
	let name_id = if id == 0 && mg::rd8(ram, PLAT_NAME_MODE) == 2 { 0xad3 } else { T_COLLECT_NAME - id };
	ICell { item: id, owned: true, name: texts.get(ram, name_id).unwrap_or_default(), desc: long_text(ram, texts, T_COLLECT_DESC + id), icon: 0, lines }
}

/// The page of this field, or None (not the Items page, or facts that do not hold: the page then stays the game's own).
fn read(ram: &[u8], g: &mut Game) -> Option<Facts> {
	let s = crate::modern_open::snap_now(ram)?;
	let manager = crate::modern_screen::manager(ram, &s);
	if !gate(s.page_id, mg::rd(ram, mg::rd(ram, s.page + 0xe0).wrapping_add(0xe4)), manager) || !mg::ready() || !crate::modern_open::modern() {
		return None;
	}
	let (texts, tables, items) = g.locate(ram)?;
	let level = mg::rd(ram, LEVEL);
	crate::modern_grid_icons::want(level);
	let m = |o: u32| mg::rd(ram, manager.wrapping_add(CURSOR + o));
	let (live, col, row, id, group) = (m(0), m(4) as usize, m(8) as usize, m(12), m(16) as usize);
	if live != 1 || group > 3 {
		return None;
	}
	let k = row * COLS[group] + col;
	let groups = build_groups(ram, &texts, &tables, items);
	if col >= COLS[group] || k >= GROUP_LEN[group] || groups[group][k].item != id {
		return None;
	}
	let tabs = PAUSE_IDS.iter().map(|&id| texts.get(ram, id).filter(|t| !t.is_empty())).collect::<Option<Vec<_>>>()?;
	let site = site_id(level).and_then(|id| texts.get(ram, id)).unwrap_or_default();
	Some(Facts { groups, sel: (group, k), tabs, site, bolts: mg::rd(ram, BOLTS) as i32, level })
}

/// The pulses that bring the game's cursor to a cell (and select it / go back): two fields pressed, two released. The game's cursor is
/// read back before every press, so its own wrapping decides.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Walk {
	pub target: Option<(usize, usize)>,
	pub confirm: bool,
	pub back: bool,
	phase: u8,
	held: u16,
	sent: u8,
	/// [PC] nav lane: the cursor words ([manager] + `CURSOR`; 0 = presses only): any move is one field (modern_jump.rs).
	pub at: u32,
	/// [PC] nav lane: the item id of every cell, groups in order (the cursor's item word of a written cell).
	pub cells: [u32; 15],
}

const MAX_PRESSES: u8 = 14;
/// [PC] nav lane: the page's tab in the pause root (modern_tabs.rs; modern_items_page.rs draws the same).
pub const TAB: usize = 3;

impl Walk {
	pub fn step(&mut self, sel: (usize, usize)) -> u16 {
		if self.phase == 0 {
			self.held = self.next(sel);
			if self.held == 0 {
				return 0;
			}
		}
		let out = if self.phase < 2 { self.held } else { 0 };
		self.phase = (self.phase + 1) % 4;
		out
	}

	fn next(&mut self, sel: (usize, usize)) -> u16 {
		if std::mem::take(&mut self.back) {
			self.target = None;
			return BIT[TRIANGLE];
		}
		let Some(to) = self.target else { return 0 };
		if let Some(p) = press_toward(sel, to).filter(|_| self.sent < MAX_PRESSES) {
			self.sent += 1;
			return self.jump(sel, to).unwrap_or(p);
		}
		self.target = None;
		self.sent = 0;
		if std::mem::take(&mut self.confirm) && sel == to {
			return BIT[CROSS];
		}
		self.confirm = false;
		0
	}

	pub fn busy(&self) -> bool {
		self.target.is_some() || self.back || self.phase != 0
	}

	/// [PC] nav lane: ONE press from a cell next to `to` over retail's wrapping (modern_jump.rs `items_preset`), the cursor words
	/// {column, row, item, group} written to that cell first (`cells`: the page's item ids). None = press as before.
	fn jump(&self, sel: (usize, usize), to: (usize, usize)) -> Option<u16> {
		if self.at == 0 {
			return None;
		}
		let ((g, k), p) = crate::modern_jump::items_preset(sel, to)?;
		if (g, k) != sel {
			let item = *self.cells.get(GROUP_LEN[..g].iter().sum::<usize>() + k)?;
			for (o, v) in [(4, (k % COLS[g]) as u32), (8, (k / COLS[g]) as u32), (12, item), (16, g as u32)] {
				crate::modern_jump::queue(self.at.wrapping_add(o), v);
			}
		}
		Some(p)
	}
}

static GAME: Mutex<Option<Game>> = Mutex::new(None);
static FACTS: Mutex<Option<Facts>> = Mutex::new(None);
static REQ: Mutex<Option<((usize, usize), bool, bool)>> = Mutex::new(None);
static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn facts() -> Option<Facts> {
	lock(&FACTS).clone()
}

/// [PC] nav lane: the cell a mouse request is taking the cursor to (drawn selected at once; modern_jump.rs `Pend`).
static PEND: crate::modern_jump::Pend<(usize, usize)> = crate::modern_jump::Pend::new();

pub fn pending() -> Option<(usize, usize)> {
	PEND.get()
}

/// Window thread: walk the game's cursor to (group, cell) and press Cross there (`pick`); `back` = Triangle.
pub fn request(to: (usize, usize), pick: bool, back: bool) {
	*lock(&REQ) = Some((to, pick, back));
	PEND.set((!back).then_some(to));
	SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Game thread, once per field (called by `modern_grid::filter` when it has no page of its own): the buttons the game gets. Without the
/// modern Items page the pad is returned unchanged, bit for bit.
pub fn filter(ram: &[u8], b: u16, s: [u8; 4]) -> (u16, [u8; 4]) {
	let mut gg = lock(&GAME);
	let g = gg.get_or_insert_with(Game::default);
	let f = if rac_engine::ui::is_open() { None } else { read(ram, g) };
	let Some(f) = f else {
		*lock(&FACTS) = None;
		(g.walk, g.seq, g.last) = (Walk::default(), SEQ.load(std::sync::atomic::Ordering::Relaxed), b);
		PEND.set(None);
		return (b, s);
	};
	let seq = SEQ.load(std::sync::atomic::Ordering::Relaxed);
	if seq != g.seq {
		g.seq = seq;
		if let Some((to, confirm, back)) = lock(&REQ).take() {
			g.walk = Walk { target: Some(to), confirm, back, at: crate::modern_jump::at(crate::modern_jump::manager(ram), CURSOR), ..Walk::default() };
		}
	}
	// [PC] nav lane: a tab clicked on the tab bar (modern_tabs.rs): back to the root first.
	if crate::modern_tabs::want_back(TAB) {
		g.walk = Walk { back: true, ..Walk::default() };
	}
	let flat = f.groups.iter().flatten().map(|c| c.item);
	g.walk.cells.iter_mut().zip(flat).for_each(|(d, i)| *d = i);
	let mut out = b;
	if g.walk.busy() {
		// [PC] nav lane: a press held last field is no new edge (a new request mid-pulse): wait one field.
		let w = if g.walk.phase == 0 && g.last & crate::modern_open::MENU_BTNS != 0 { 0 } else { g.walk.step(f.sel) };
		out = (out & !crate::modern_open::MENU_BTNS) | w;
	}
	g.last = out;
	if SEQ.load(std::sync::atomic::Ordering::Relaxed) == g.seq {
		PEND.set(g.walk.target.filter(|_| !g.walk.back));
	}
	*lock(&FACTS) = Some(f);
	(out, s)
}

#[cfg(test)]
#[path = "modern_items_tests.rs"]
mod tests;
