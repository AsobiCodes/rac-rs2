//! [PC] R&C2's modern Gadgetron vendor (lane scr; R&C1's twins: rac1/retail/shop_view.rs facts, rac1/modern_menu/vendor_host.rs):
//! the game thread reads the shop from RAM once per field (`read`) and turns the window's clicks into pad pulses (`filter`); the
//! page itself is `modern_vendor_page.rs`, drawn by host/screens.rs. The game stays in charge: choosing a row, the quantity, buying
//! and leaving are pad presses into the retail shop; nothing here writes bolts or ammo. Buy all ammo is the one request that is not
//! a press: `modern_vendor_hand.rs` buys every ammo row in one go with retail's own purchase calls, as R&C1 does.
//!
//! Facts [C] (rac2/notes_vendor_map.md 1.2, 1.3, 5.4; RAM dumps runs/vendmap/r0125 of bank state 0125, lane scr probe):
//! - Game mode word 0x1a8f80 (level 1: 0x1a8fc0) = 5 while a shop is up (levels 2,3,4,6,11,12 Gadgetron; 24 = the ship shop).
//! - Per level (`WORDS`, only level 2 known): current row, stage (0 browse, 2 the quantity / "Purchase?" prompt), quantity on the
//!   prompt, the row list (0x18-byte rows {slot, kind 1 = ammo, text id [?: not the name, 0xc05 is a card message], 0xcdb, 0, 0},
//!   ended by a zero row) and the item record table (0xe0 bytes per record: +0x8 u16 name text id (26 Blitz Gun 0xa7a, 22 Chopper
//!   0xa78, lane scr dump r0125), +0x80 weapon price, +0x88 u16 bolts per round, +0x8e u16 max ammo).
//! - Globals: slot -> record byte map 0x1395e8 (slot 30 -> 60 after the Lancer's upgrade), ammo held 0x139708 + 4 x slot, bolts
//!   0x1a7a80, level 0x1a7a78.
//! Every level's cur / stage / qty / rows / table words come from `modern_vendor_find` (the level's copy of the quantity prompt, by
//! code words); a level where it is not found keeps the retail shop.
use crate::modern_screen::{Texts, DOWN, LEFT, RIGHT, UP};
use crate::modern_vendor_buyall::{nav, Nav};
use crate::modern_vendor_hand as hand;
use crate::modern_walk::Walk;
use rac_engine::input::binds::{BIT, CROSS, SQUARE, TRIANGLE};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

pub const MODE: u32 = 0x1a8f80;
pub const MODE_L1: u32 = 0x1a8fc0;
pub const VENDOR_MODE: u32 = 5;
pub const SLOT_MAP: u32 = 0x1395e8;
pub const AMMO: u32 = 0x139708;
pub const ROW_STRIDE: u32 = 0x18;
pub const ROW_MARK: u32 = 0xcdb;
pub const REC_SIZE: u32 = 0xe0;
/// Rows read at most (the level-12 list has 18 [C bank 0555]).
pub const MAX_ROWS: usize = 24;
/// The table check: record 22 (the Chopper) costs 5000 bolts on every level [C data, boot ELF record].
const CHECK_ITEM: u32 = 22;
const CHECK_PRICE: u32 = 5000;

/// One level's shop words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Words {
	pub level: u32,
	pub cur: u32,
	pub stage: u32,
	pub qty: u32,
	pub rows: u32,
	pub table: u32,
}

/// Level 2's words [C: bank 0125 replay dumps; `modern_vendor_find` gives the same]. Every level's words come from the finder; this
/// one is kept for the tests.
pub const L2: Words = Words { level: 2, cur: 0x231040, stage: 0x231044, qty: 0x1b3d48, rows: 0x231100, table: 0x264af0 };

/// One row of the shop's list.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Row {
	pub slot: u32,
	pub ammo: bool,
	pub name: String,
	/// Weapon: its price. Ammo: bolts per round.
	pub price: u32,
	pub owned: u32,
	pub max: u32,
}

impl Row {
	pub fn full(&self) -> bool {
		self.ammo && self.owned >= self.max
	}

	/// The most rounds the prompt allows: `min(max - owned, bolts / unit)` (retail's clamp [C code 111190..]).
	pub fn most(&self, bolts: u32) -> u32 {
		if !self.ammo || self.price == 0 {
			return 0;
		}
		self.max.saturating_sub(self.owned).min(bolts / self.price)
	}

	pub fn can_buy(&self, bolts: u32) -> bool {
		if self.ammo { self.most(bolts) > 0 } else { bolts >= self.price }
	}
}

/// The shop as one field sees it.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Shop {
	pub rows: Vec<Row>,
	pub cur: usize,
	pub stage: u32,
	pub qty: u32,
	pub bolts: u32,
	/// Buy all ammo is asked and the game has not bought it yet (`modern_vendor_hand.rs`; one field at most).
	pub buying: bool,
	/// The Buy all ammo row above the list has the focus (R&C1's list row; `modern_vendor_buyall::nav`).
	pub all_focus: bool,
	/// The row the mouse's walk is on its way to (the page keeps that row shown until retail's cursor is there).
	pub walk: Option<usize>,
}

impl Shop {
	pub fn prompt(&self) -> bool {
		self.stage != 0
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

/// The shop of this field (None: no shop up, no quantity prompt found in the level, or words that do not hold). `name` = the text
/// of an id.
pub fn read(ram: &[u8], name: &dyn Fn(u32) -> Option<String>) -> Option<Shop> {
	let level = rd(ram, crate::modern_screen::LEVEL);
	let mode = rd(ram, if level == 1 { MODE_L1 } else { MODE });
	if mode != VENDOR_MODE {
		return None;
	}
	read_with(ram, &crate::modern_vendor_find::words(ram, level)?, name)
}

/// `read` with known words. The list ends at a zero row; the weapon rows carry 0xcdb at +0xc, the ammo rows of weapons found
/// elsewhere (items 12, 14, 17, 18 after them [C bank 0260 0473 0555]) carry 0 or 0xe1 there, so +0xc is not checked.
pub fn read_with(ram: &[u8], w: &Words, name: &dyn Fn(u32) -> Option<String>) -> Option<Shop> {
	if rd(ram, w.table + CHECK_ITEM * REC_SIZE + 0x80) != CHECK_PRICE {
		return None;
	}
	let mut rows = Vec::new();
	for k in 0..MAX_ROWS as u32 {
		let a = w.rows + k * ROW_STRIDE;
		let slot = rd(ram, a);
		if slot == 0 {
			break;
		}
		if rd(ram, a + 4) > 1 || slot >= 0x80 {
			return None;
		}
		let ammo = rd(ram, a + 4) == 1;
		let rec = w.table + rd8(ram, SLOT_MAP + slot) * REC_SIZE;
		let price = if ammo { rd16(ram, rec + 0x88) } else { rd(ram, rec + 0x80) };
		let name = name(rd16(ram, rec + 8)).unwrap_or_default();
		rows.push(Row { slot, ammo, name, price, owned: rd(ram, AMMO + 4 * slot), max: rd16(ram, rec + 0x8e) });
	}
	let cur = rd(ram, w.cur) as usize;
	if rows.is_empty() || cur >= rows.len() {
		return None;
	}
	Some(Shop { rows, cur, stage: rd(ram, w.stage), qty: rd(ram, w.qty), bolts: rd(ram, crate::modern_screen::BOLTS), buying: false, all_focus: false, walk: None })
}

/// The browse list's pad: the page's list runs down, the game's strip runs across, so Up / Down walk it like Left / Right [C probe];
/// the prompt keeps every button (Left / Right = fewer / more).
pub fn browse_pad(b: u16, prompt: bool) -> u16 {
	if prompt {
		return b;
	}
	let mut o = b & !(UP | DOWN);
	if b & UP != 0 {
		o |= LEFT;
	}
	if b & DOWN != 0 {
		o |= RIGHT;
	}
	o
}

/// A window request: walk to a row (and buy it; takes the focus off Buy all ammo), press one button, Buy all ammo, or put the focus
/// on / off the Buy all ammo row (off walks the game's cursor back to row 0, the row under it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
	Row(usize, bool),
	Press(u16),
	BuyAll,
	AllFocus(bool),
}

/// The pulses of a request over the one walk (`modern_walk::Walk`: Left / Right along the game's strip, two fields pressed, two
/// released). A Cross is sent only when retail's cursor IS on the row asked for; a Cross asked while a walk runs rides on it
/// (sweep B2: before, a Press replaced the walk and the Cross landed on the in-between row).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pulse(pub Walk);

impl Default for Pulse {
	fn default() -> Pulse {
		Pulse(Walk::new(LEFT, RIGHT).gate(BIT[CROSS]))
	}
}

impl Pulse {
	/// Fields per press (the Ship Shack takes one step per 12 fields [C runs: 5 taps at period 12 = 5 steps, at period 10 = 4, at
	/// period 4 = 2]).
	pub fn slow(self, period: u8) -> Pulse {
		Pulse(self.0.period(period))
	}

	pub fn new(a: Ask) -> Pulse {
		let mut p = Pulse::default();
		p.ask(a);
		p
	}

	/// Takes a window request into the running walk: a row walks there (and buys on arrival); Cross rides on a running walk; any
	/// other press ends the walk.
	pub fn ask(&mut self, a: Ask) {
		match a {
			Ask::Row(r, buy) => self.0.to(r, if buy { BIT[CROSS] } else { 0 }),
			Ask::Press(p) if p == BIT[CROSS] => self.0.confirm(p),
			Ask::Press(p) => self.0.press(p),
			Ask::BuyAll | Ask::AllFocus(_) => {}
		}
	}

	pub fn busy(&self) -> bool {
		self.0.busy()
	}

	/// The row the walk is on its way to.
	pub fn target(&self) -> Option<usize> {
		self.0.target()
	}

	/// One field: the game's buttons (Left / Right walk the strip; it does NOT wrap at row 0 [C ball2 run: Left pulses on row 0 left
	/// the cursor there]).
	pub fn step(&mut self, cur: usize, n: usize) -> u16 {
		self.0.step(cur, n, true)
	}

	/// `step`, but a Cross waits while `ready` is false (the Ship Shack takes the part of a Cross only once the cursor has rested for
	/// about 24 fields; before that it buys the row the cursor was on before the walk [C runs]).
	pub fn step_gated(&mut self, cur: usize, n: usize, ready: bool) -> u16 {
		self.0.step(cur, n, ready)
	}
}

pub const CROSS_B: u16 = BIT[CROSS];
pub const TRIANGLE_B: u16 = BIT[TRIANGLE];
/// Square in the browse list = Buy all ammo (R&C1's key; the retail browse reads Left / Right / Cross / Triangle [C notes 1.3]).
pub const SQUARE_B: u16 = BIT[SQUARE];

#[derive(Default)]
struct Game {
	texts: Option<Texts>,
	retry: u32,
	pulse: Pulse,
	/// Last field's buttons after `browse_pad` (new presses).
	prev: u16,
	/// The keys on the Buy all ammo row.
	nav: Nav,
	/// Last field's prompt state and the fields since it flipped (list <-> prompt).
	was_prompt: bool,
	since_flip: u32,
}

/// Fields after a switch between the list and the prompt in which a mouse confirm is dropped (R&C1's `SETTLE_MS` 350 ms; sweep B3:
/// a double click or a click right after the prompt closed confirmed a purchase the player did not see).
pub const SETTLE_FIELDS: u32 = 18;

/// May a window request reach the game this field? A row request only on the list (it was built against the list's layout); a Cross
/// not within `SETTLE_FIELDS` of a list <-> prompt switch.
pub fn ask_ok(a: Ask, prompt: bool, since_flip: u32) -> bool {
	let settling = since_flip < SETTLE_FIELDS;
	match a {
		Ask::Row(_, buy) => !prompt && !(buy && settling),
		Ask::Press(p) => !(p == CROSS_B && settling),
		Ask::BuyAll | Ask::AllFocus(_) => true,
	}
}

static GAME: Mutex<Option<Game>> = Mutex::new(None);
static SHOP: Mutex<Option<Shop>> = Mutex::new(None);
static ASK: Mutex<Option<Ask>> = Mutex::new(None);
/// The window can draw the page (UI pipes fine, the world drawn behind it).
static READY: AtomicBool = AtomicBool::new(false);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn set_ready(ok: bool) {
	READY.store(ok, Ordering::Relaxed);
}

pub fn shop() -> Option<Shop> {
	lock(&SHOP).clone()
}

/// Window thread: a click's request (the newest wins).
pub fn ask(a: Ask) {
	*lock(&ASK) = Some(a);
}

fn texts(ram: &[u8], g: &mut Game) -> Option<Texts> {
	if g.texts.is_some_and(|t| t.still(ram)) {
		return g.texts;
	}
	if g.retry > 0 {
		g.retry -= 1;
		return None;
	}
	g.texts = Texts::find(ram);
	if g.texts.is_none() {
		g.retry = 50;
	}
	g.texts
}

/// Game thread, once per field after `modern_screen::filter`: the buttons the game gets. Without a modern shop the pad is returned
/// unchanged, bit for bit.
pub fn filter(ram: &[u8], b: u16, s: [u8; 4]) -> (u16, [u8; 4]) {
	let mut gg = lock(&GAME);
	let g = gg.get_or_insert_with(Game::default);
	let on = READY.load(Ordering::Relaxed) && crate::modern_open::modern() && !rac_engine::ui::is_open();
	let shop = if on {
		let t = texts(ram, g);
		read(ram, &|id| t.and_then(|t| t.get(ram, id)))
	} else {
		None
	};
	let Some(shop) = shop else {
		*lock(&SHOP) = None;
		*lock(&ASK) = None;
		(g.pulse, g.prev, g.nav, g.was_prompt, g.since_flip) = (Pulse::default(), b, Nav::default(), false, 0);
		hand::cancel();
		return (b, s);
	};
	let (out, shop) = drive(g, shop, b);
	*lock(&SHOP) = Some(shop);
	(out, s)
}

/// The buttons of one field with a modern shop up: the window's request, the Buy all ammo row's keys, Square (Buy all ammo: one
/// request to the hand, bought at the game's next shop frame), the pulses.
fn drive(g: &mut Game, mut shop: Shop, b: u16) -> (u16, Shop) {
	let mut out = browse_pad(b, shop.prompt());
	let new = out & !g.prev;
	g.prev = out;
	let mut ask = lock(&ASK).take();
	if shop.prompt() != g.was_prompt {
		(g.was_prompt, g.since_flip) = (shop.prompt(), 0);
	}
	g.since_flip = g.since_flip.saturating_add(1);
	if ask.is_some_and(|a| !ask_ok(a, shop.prompt(), g.since_flip)) {
		ask = None;
	}
	let live = !shop.prompt() && !hand::pending() && !g.pulse.busy();
	let k = nav(&mut g.nav, live, shop.cur, out, new);
	out = k.pad;
	if (new & SQUARE_B != 0 && !shop.prompt()) || k.start {
		ask = Some(Ask::BuyAll);
	} else if k.ask.is_some() {
		ask = k.ask;
	}
	match ask {
		Some(Ask::BuyAll) => {
			// Browse only, no walk in flight, something to fill: the hand buys it all at the dispatcher's next call.
			if !shop.prompt() && !g.pulse.busy() && !crate::modern_vendor_buyall::wanted(&shop).is_empty() {
				hand::request();
			}
		}
		Some(Ask::AllFocus(on)) => {
			g.nav.focus = on && !shop.prompt();
			if !on && shop.cur != 0 && !hand::pending() {
				g.pulse.ask(Ask::Row(0, false)); // back to the row under it
			}
		}
		Some(a) => {
			if matches!(a, Ask::Row(..)) {
				g.nav.focus = false;
			}
			g.pulse.ask(a);
		}
		None => {}
	}
	if !shop.prompt() {
		out &= !SQUARE_B;
	}
	if hand::pending() {
		// The game buys at its next shop frame: no menu press reaches it in between.
		out &= !crate::modern_open::MENU_BTNS;
	}
	if g.pulse.busy() {
		out = (out & !crate::modern_open::MENU_BTNS) | g.pulse.step(shop.cur, shop.rows.len());
	}
	shop.buying = hand::pending();
	shop.all_focus = g.nav.focus;
	shop.walk = g.pulse.target();
	(out, shop)
}

#[cfg(test)]
#[path = "modern_vendor_tests.rs"]
mod tests;
