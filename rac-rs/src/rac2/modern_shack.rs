//! [PC] R&C2's modern Ship Shack (lane shack; twin of `modern_vendor.rs`): the game thread reads the shop from RAM once per field
//! (`read`) and turns the window's clicks into pad pulses (`filter`); the page is `modern_shack_page.rs`, drawn by
//! host/screens_ship.rs. The game stays in charge: choosing a part, the "Purchase?" prompt and leaving are pad presses into the
//! retail shop; nothing here writes Raritanium or the owned mask.
//!
//! Facts [C] (rac2/notes_modern_screens.md 8.3, level 24, PAL): game mode 0x1a8f80 = 5, cursor 0x22f140, stage 0x22f144 (0 browse,
//! 2 prompt), Raritanium 0x1a7a84, owned mask u16 0x1a7b78, part table 0x2a0ad0 (12 rows x 0x14 bytes {name, desc, group, own bit,
//! id}), costs u32 array 0x2a1390 (part row r >= 1 costs cost[r + 1], row 0 costs cost[0]).
use crate::modern_screen::Texts;
use crate::modern_shack_page::{self as page, Entry, Pick, Shack};
use crate::modern_vendor::{browse_pad, Ask, Pulse, MODE, VENDOR_MODE};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

pub const SHACK_LEVEL: u32 = 24;
pub const CUR: u32 = 0x22f140;
pub const STAGE: u32 = 0x22f144;
pub const RARITANIUM: u32 = 0x1a7a84;
pub const MASK: u32 = 0x1a7b78;
pub const PARTS: u32 = 0x2a0ad0;
pub const COSTS: u32 = 0x2a1390;
const ROW_SIZE: u32 = 0x14;
const ROWS: usize = 12;
/// Fields per walk step: retail's shop takes a step every 12 fields at most (see `Pulse::slow`).
const SHACK_PERIOD: u8 = 12;
/// Fields the cursor must rest before a Cross: retail takes the part to buy from a latch that follows the cursor ~20 fields late (a
/// Cross 12-18 fields after a walk bought the old row for the new row's price; 24 and more is right).
const SETTLE: u32 = 26;

fn rd(ram: &[u8], a: u32) -> u32 {
	let a = (a & 0x1ff_fffc) as usize;
	ram.get(a..a + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Text ids of the wing pairs (Standard, Hi-Lift, Heavy Ordnance) [C level-24 text bank: 0x10d6 / 0xffc / 0xffd]; the parts' name ids
/// are the first word of their table rows.
pub const WING_TEXT: [u32; 3] = [0x10d6, 0xffc, 0xffd];

/// The shop of this field (None: not the ship shop, or words that do not hold). Names come from the game's own text bank (`text`),
/// the English ones are only the backstop.

pub fn read(ram: &[u8], text: &dyn Fn(u32) -> Option<String>, desc: &dyn Fn(u32) -> Option<String>) -> Option<Shack> {
	if rd(ram, crate::modern_screen::LEVEL) != SHACK_LEVEL || rd(ram, MODE) != VENDOR_MODE {
		return None;
	}
	// The table must be the Ship Shack's: Triple Boost (own bit 0x10, id 0x11) at row 0, 15 / 45 Raritanium at both ends of the parts.
	if rd(ram, PARTS + 12) != 0x10 || rd(ram, PARTS + 16) != 0x11 || rd(ram, COSTS) != 15 || rd(ram, COSTS + 4 * 12) != 45 {
		return None;
	}
	let rows: Vec<(u32, u32)> = (0..ROWS as u32).map(|r| (rd(ram, PARTS + r * ROW_SIZE + 8), rd(ram, PARTS + r * ROW_SIZE + 12))).collect();
	let mask = rd(ram, MASK) & 0xffff;
	let entries: Vec<Entry> = page::strip(&rows, mask)
		.into_iter()
		.map(|pick| match pick {
			Pick::Part(r) => Entry {
				pick,
				name: text(rd(ram, PARTS + r as u32 * ROW_SIZE)).filter(|n| !n.is_empty()).unwrap_or_else(|| page::PART_NAMES[r].to_string()),
				cost: rd(ram, COSTS + 4 * if r == 0 { 0 } else { r as u32 + 1 }),
				desc: desc(rd(ram, PARTS + r as u32 * ROW_SIZE + 4)).unwrap_or_default(),
			},
			Pick::Wing(w) => Entry {
				pick,
				name: text(WING_TEXT[w]).filter(|n| !n.is_empty()).unwrap_or_else(|| page::WING_NAMES[w].to_string()),
				cost: Some(rd(ram, COSTS + 4 * (13 + w as u32))).filter(|c| (1..1000).contains(c)).unwrap_or(page::WING_COST), // [b46 W5] U11: costs[13..15] = 5 5 5 [C 0434]
				desc: String::new(),
			},
		})
		.collect();
	let cur = rd(ram, CUR) as usize;
	if entries.is_empty() || cur >= entries.len() {
		return None;
	}
	Some(Shack { entries, cur, stage: rd(ram, STAGE), rar: rd(ram, RARITANIUM), walk: None })
}

#[derive(Default)]
struct Game {
	pulse: Pulse,
	prev: u16,
	texts: Option<Texts>,
	retry: u32,
	/// Fields the shop has been open (for `debug_asks`).
	open: u64,
	/// The cursor of the last field and the fields it has rested there.
	last_cur: usize,
	still: u32,
	/// A pad Cross is held back until the cursor has rested (`hold_cross`).
	hold: bool,
	/// Last field's prompt state and the fields since it flipped (`modern_vendor::ask_ok`).
	was_prompt: bool,
	since_flip: u32,
}

impl Game {
	/// No shop this field: forget the walk and the held Cross (a Cross held when the player left must not fire at the next visit).
	fn closed(&mut self, b: u16) {
		(self.pulse, self.prev) = (Pulse::default(), b);
		(self.hold, self.still, self.last_cur, self.open) = (false, 0, usize::MAX, 0);
		(self.was_prompt, self.since_flip) = (false, 0);
	}
}

/// Modern mode: retail's buy latch trails the cursor, so a pad / keyboard Cross soon after a move would buy the row the card no longer
/// shows. A Cross pressed (rising edge) before the cursor rested `SETTLE` fields is taken away and given back as one press once it has.
/// Returns (buttons for the game, fire the held Cross now).
fn hold_cross(hold: &mut bool, was: u16, out: u16, still: u32, prompt: bool) -> (u16, bool) {
	let cross = crate::modern_vendor::CROSS_B;
	if prompt {
		*hold = false;
		return (out, false);
	}
	if out & cross != 0 && was & cross == 0 && still < SETTLE {
		*hold = true;
	}
	if !*hold {
		return (out, false);
	}
	if still >= SETTLE {
		*hold = false;
		return (out & !cross, true);
	}
	(out & !cross, false)
}

/// Debug: RAC2_SHACK_ASK="N:ROW:BUY,N:pHEX,..." sends the click a window would, at the N-th field the shop is open (ROW: strip index,
/// BUY 0/1 = click with a buy; pHEX = a pad press, e.g. p40 = Cross). Lets a hidden test run drive the window path.
fn debug_asks() -> &'static [(u64, Ask)] {
	static ASKS: std::sync::OnceLock<Vec<(u64, Ask)>> = std::sync::OnceLock::new();
	ASKS.get_or_init(|| {
		let v = std::env::var("RAC2_SHACK_ASK").unwrap_or_default();
		v.split(',')
			.filter_map(|e| {
				let mut p = e.split(':');
				let n = p.next()?.parse().ok()?;
				let a = p.next()?;
				let ask = match a.strip_prefix('p') {
					Some(h) => Ask::Press(u16::from_str_radix(h, 16).ok()?),
					None => Ask::Row(a.parse().ok()?, p.next()? == "1"),
				};
				Some((n, ask))
			})
			.collect()
	})
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

static GAME: Mutex<Option<Game>> = Mutex::new(None);
static SHOP: Mutex<Option<Shack>> = Mutex::new(None);
static ASK: Mutex<Option<Ask>> = Mutex::new(None);
static READY: AtomicBool = AtomicBool::new(false);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn set_ready(ok: bool) {
	READY.store(ok, Ordering::Relaxed);
}

pub fn shop() -> Option<Shack> {
	lock(&SHOP).clone()
}

/// Window thread: a click's request (the newest wins).
pub fn ask(a: Ask) {
	*lock(&ASK) = Some(a);
}

/// Game thread, once per field after `modern_vendor::filter`: the buttons the game gets. Without a modern Ship Shack the pad is
/// returned unchanged, bit for bit.
pub fn filter(ram: &[u8], b: u16, s: [u8; 4]) -> (u16, [u8; 4]) {
	let mut gg = lock(&GAME);
	let g = gg.get_or_insert_with(Game::default);
	let on = READY.load(Ordering::Relaxed) && crate::modern_open::modern() && !rac_engine::ui::is_open();
	let t = if on { texts(ram, g) } else { None };
	let Some(shop) = on.then(|| read(ram, &|id| t.and_then(|t| t.get(ram, id)), &|id| t.and_then(|t| t.get_n(ram, id, 400)))).flatten() else {
		*lock(&SHOP) = None;
		*lock(&ASK) = None;
		g.closed(b);
		return (b, s);
	};
	let mut shop = shop;
	g.pulse = g.pulse.slow(SHACK_PERIOD);
	if shop.prompt() != g.was_prompt {
		(g.was_prompt, g.since_flip) = (shop.prompt(), 0);
	}
	g.since_flip = g.since_flip.saturating_add(1);
	g.still = if shop.cur == g.last_cur { g.still.saturating_add(1) } else { 0 };
	g.last_cur = shop.cur;
	let mut out = browse_pad(b, shop.prompt());
	let was = std::mem::replace(&mut g.prev, out);
	if !g.pulse.busy() {
		let (o, fire) = hold_cross(&mut g.hold, was, out, g.still, shop.prompt());
		out = o;
		if fire {
			g.pulse.ask(Ask::Press(crate::modern_vendor::CROSS_B));
		}
	}
	// One walk (sweep B23): a row request walks there, a Cross rides on the walk and lands only on its target; a request built
	// against the other layout (list vs prompt) or a confirm right after the switch is dropped (B3's rule).
	if let Some(a) = lock(&ASK).take().filter(|&a| crate::modern_vendor::ask_ok(a, shop.prompt(), g.since_flip)) {
		g.pulse.ask(a);
	}
	if g.pulse.busy() {
		out = (out & !crate::modern_open::MENU_BTNS) | g.pulse.step_gated(shop.cur, shop.entries.len(), g.still >= SETTLE);
	}
	g.open += 1;
	if let Some((_, a)) = debug_asks().iter().find(|(n, _)| *n == g.open) {
		g.pulse.ask(*a);
	}
	shop.walk = g.pulse.target();
	*lock(&SHOP) = Some(shop);
	(out, s)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn ram() -> Vec<u8> {
		let mut r = vec![0u8; 0x300000];
		let mut w = |a: u32, v: u32| r[a as usize..a as usize + 4].copy_from_slice(&v.to_le_bytes());
		w(crate::modern_screen::LEVEL, 24);
		w(MODE, 5);
		let rows: [(u32, u32); 12] = [
			(0x30, 0x10), (0x40, 0x40), (0x180, 0x80), (0x180, 0x100), (0x200, 0), (0x200, 0x200),
			(0xc00, 0x400), (0xc00, 0x800), (0xc000, 0x4000), (0xc000, 0x8000), (0x1000, 0x1000), (0x2000, 0x2000),
		];
		for (i, (g, b)) in rows.iter().enumerate() {
			w(PARTS + i as u32 * ROW_SIZE + 8, *g);
			w(PARTS + i as u32 * ROW_SIZE + 12, *b);
		}
		w(PARTS + 16, 0x11);
		for (i, c) in [15u32, 30, 30, 10, 20, 10, 20, 10, 20, 20, 40, 60, 45].iter().enumerate() {
			w(COSTS + 4 * i as u32, *c);
		}
		w(MASK, 0x4240);
		w(RARITANIUM, 30);
		w(CUR, 2);
		r
	}

	#[test]
	fn reads_the_pal_shop() {
		let s = read(&ram(), &|_| None, &|_| None).unwrap();
		assert_eq!((s.entries.len(), s.cur, s.stage, s.rar), (8, 2, 0, 30));
		let e = &s.entries[2];
		assert_eq!((e.name.as_str(), e.cost), ("Torpedo Launcher", 10), "row 6 costs cost[7]");
		assert_eq!(s.entries[0].cost, 15);
		assert_eq!(s.entries[5].cost, 45);
		assert_eq!(s.entries[6].cost, 5);
	}

	#[test]
	fn names_come_from_the_text_bank() {
		let t = |id: u32| match id {
			0xfec => Some("Lance-torpilles".to_string()),
			0xffc => Some("Ailes".to_string()),
			_ => None,
		};
		let mut r = ram();
		r[(PARTS + 6 * ROW_SIZE) as usize..][..4].copy_from_slice(&0xfecu32.to_le_bytes());
		let s = read(&r, &t, &|_| None).unwrap();
		assert_eq!(s.entries[2].name, "Lance-torpilles");
		assert_eq!(s.entries[6].name, "Ailes");
		assert_eq!(s.entries[7].name, "Heavy Ordnance Wings", "no text: the English backstop");
	}

	#[test]
	fn a_walk_step_takes_twelve_fields() {
		let mut p = Pulse::new(Ask::Row(4, false)).slow(SHACK_PERIOD);
		let mut cur = 2;
		let mut fields = 0;
		let mut steps = 0;
		while p.busy() && fields < 200 {
			if p.step(cur, 8) != 0 && fields % 12 == 0 {
				cur += 1;
				steps += 1;
			}
			fields += 1;
		}
		assert_eq!((cur, steps), (4, 2));
		assert_eq!(fields, 25, "two steps of 12 fields, then the field that finds the target reached");
	}

	#[test]
	fn a_cross_waits_for_the_cursor_to_rest() {
		let mut p = Pulse::new(Ask::Row(3, true)).slow(SHACK_PERIOD);
		assert_ne!(p.step_gated(2, 8, true), 0, "walk press");
		for _ in 0..11 {
			p.step_gated(3, 8, false);
		}
		for _ in 0..30 {
			assert_eq!(p.step_gated(3, 8, false), 0, "no Cross while the cursor has not rested");
		}
		assert!(p.busy());
		assert_ne!(p.step_gated(3, 8, true), 0, "Cross once it has");
	}

	#[test]
	fn a_pad_cross_after_a_move_waits_for_the_cursor() {
		let c = crate::modern_vendor::CROSS_B;
		let mut hold = false;
		// Cross pressed 2 fields after a move: taken away, kept while the cursor is not at rest.
		assert_eq!(hold_cross(&mut hold, 0, c, 2, false), (0, false));
		assert_eq!(hold_cross(&mut hold, c, c, 10, false), (0, false));
		assert_eq!(hold_cross(&mut hold, 0, 0, SETTLE - 1, false), (0, false), "released early: still pending");
		assert_eq!(hold_cross(&mut hold, 0, 0, SETTLE, false), (0, true), "given back once rested");
		assert!(!hold);
		// A Cross on a rested cursor passes through; the prompt's Cross is never held.
		assert_eq!(hold_cross(&mut hold, 0, c, SETTLE, false), (c, false));
		assert_eq!(hold_cross(&mut hold, 0, c, 0, true), (c, false));
	}

	#[test]
	fn leaving_the_shop_drops_a_held_cross() {
		let c = crate::modern_vendor::CROSS_B;
		let mut g = Game::default();
		assert_eq!(hold_cross(&mut g.hold, 0, c, 2, false), (0, false));
		(g.last_cur, g.still) = (3, 20);
		assert!(g.hold, "Cross held: the cursor had not rested");
		g.closed(0x40);
		assert!(!g.hold && g.still == 0 && g.last_cur == usize::MAX && g.prev == 0x40 && !g.pulse.busy());
		// Next visit: once the cursor rests, nothing fires.
		assert_eq!(hold_cross(&mut g.hold, 0, 0, SETTLE, false), (0, false));
	}

	#[test]
	fn other_levels_and_modes_read_nothing() {
		let mut r = ram();
		r[crate::modern_screen::LEVEL as usize] = 2;
		assert!(read(&r, &|_| None, &|_| None).is_none());
		let mut r = ram();
		r[MODE as usize] = 0;
		assert!(read(&r, &|_| None, &|_| None).is_none());
	}
}
