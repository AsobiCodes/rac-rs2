//! [PC] R&C2's modern Galactic Map (lane scrmap): the game thread reads the map from RAM once per field (`read`) and turns the
//! window's mouse into pad pulses (`filter`); the page is `modern_map_page.rs`, drawn by host/screens_map.rs. The game stays in
//! charge: the cursor, the travel and leaving are pad presses into the retail map page; nothing here writes the game's RAM.
//!
//! Facts [C] (rac2/notes_modern_screens.md 8.2, RAM dumps of user state q1794424659374_3213, level 24):
//! - The map is a page of the page menu: game mode 0x1a8f80 (level 1: 0x1a8fc0) == 3, the current page record (`modern_open::snap_now`)
//!   has id 0xf and parent 0 (the Quick Select / Gadgets grids use id 0xf too, with a parent).
//! - The selected planet is the LEVEL NUMBER at page +0x180; the list order is the u32 array 0x1399c8 (boot, level independent),
//!   ended by a zero entry. Down / Up (new press) move the cursor by one, clamped; Cross travels (or closes on the current level),
//!   Triangle closes.
//! - Planet name = text `modern_screen::site_id(level)` (0xb45 + level; 25 Wupash 0x1b23, 26 Jamming Array 0x1b22), location line =
//!   0xb2b + level (levels 0..24). [b45 P3, sweep B10] Level 24 (bank state 0434, English) holds the English table with absolute
//!   pointers (found by `Texts::find`) next to German / Spanish / Italian tables in another form ({count, size} header + 16-byte
//!   records {offset from the header, id, -1, 0}; not read here). `english` is the backstop for every level (0434's English table).
use crate::modern_screen::{Texts, DOWN, LEVEL, UP};
use crate::modern_walk::Walk;
use rac_engine::input::binds::{BIT, CROSS, TRIANGLE};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

pub const MODE: u32 = 0x1a8f80;
pub const MODE_L1: u32 = 0x1a8fc0;
pub const MAP_MODE: u32 = 3;
pub const ORDER: u32 = 0x1399c8;
pub const MAP_PAGE: u32 = 0xf;
pub const SEL_OFF: u32 = 0x180;
pub const MAX_ENTRIES: usize = 24;
pub const CROSS_B: u16 = BIT[CROSS];
pub const TRIANGLE_B: u16 = BIT[TRIANGLE];

/// One destination of the list.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Entry {
	pub level: u32,
	pub planet: String,
	pub loc: String,
}

/// The map as one field sees it.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Map {
	pub entries: Vec<Entry>,
	/// The selected entry (the game's cursor).
	pub cur: usize,
	/// The level the player is on now.
	pub here: u32,
	/// The entry the mouse's walk is on its way to (the page keeps it shown until retail's cursor is there).
	pub walk: Option<usize>,
}

fn rd(ram: &[u8], a: u32) -> u32 {
	let a = (a & 0x1ff_fffc) as usize;
	ram.get(a..a + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// English planet and location names by level [C: the English text table of bank state fg1 0434 (level 24), ids 0xb45 + level /
/// 0xb2b + level, 25 / 26 by `site_id`; the same in 0580].
pub fn english(level: u32) -> Option<(&'static str, &'static str)> {
	Some(match level {
		0 | 14 => ("Aranos", "Flying Lab"),
		1 => ("Oozla", "Megacorp Outlet"),
		2 => ("Maktar Nebula", "Maktar Resort"),
		3 => ("Endako", "Megapolis"),
		4 => ("Barlow", "Vukovar Canyon"),
		5 => ("Feltzin System", "Thug Rendezvous"),
		6 => ("Notak", "Canal City"),
		7 => ("Siberius", "Frozen Base"),
		8 => ("Tabora", "Mining Area"),
		9 => ("Dobbo", "Testing Facility"),
		10 => ("Hrugis Cloud", "Deep Space Disposal"),
		11 => ("Joba", "Megacorp Games"),
		12 => ("Todano", "Megacorp Armory"),
		13 => ("Boldan", "Silver City"),
		15 => ("Gorn", "Thugs-4-Less Fleet"),
		16 => ("Snivelak", "Thug HQ"),
		17 => ("Smolg", "Distribution Facility"),
		18 => ("Damosel", "Allgon City"),
		19 => ("Grelbin", "Tundor Wastes"),
		20 => ("Yeedil", "Protopet Factory"),
		21 => ("Dantopia", "Insomniac Museum"),
		22 => ("Dobbo Orbit", "Testing Facility Moon"),
		23 => ("Damosel Orbit", "Testing Facility Moon"),
		24 => ("Ship Shack", "Slim Cognito's"),
		25 => ("Wupash Nebula", ""),
		26 => ("Jamming Array", ""),
		_ => return None,
	})
}

/// The map of this field (None: no map page is current, or words that do not hold). `name` = the text of an id.
pub fn read(ram: &[u8], name: &dyn Fn(u32) -> Option<String>) -> Option<Map> {
	let here = rd(ram, LEVEL);
	if rd(ram, if here == 1 { MODE_L1 } else { MODE }) != MAP_MODE {
		return None;
	}
	let s = crate::modern_open::snap_now(ram)?;
	if s.page_id != MAP_PAGE || rd(ram, s.page + 0xe0) != 0 {
		return None;
	}
	read_with(ram, s.page, here, name)
}

/// `read` once the page is known to be the map.
pub fn read_with(ram: &[u8], page: u32, here: u32, name: &dyn Fn(u32) -> Option<String>) -> Option<Map> {
	let mut levels = Vec::new();
	for k in 0..MAX_ENTRIES as u32 {
		let l = rd(ram, ORDER + 4 * k);
		if l == 0 || l >= 64 {
			break;
		}
		levels.push(l);
	}
	let sel = rd(ram, page + SEL_OFF);
	let cur = levels.iter().position(|&l| l == sel)?;
	let text = |id: u32| name(id).filter(|t| !t.is_empty());
	let entries = levels
		.iter()
		.map(|&level| {
			let e = english(level);
			Entry {
				level,
				planet: crate::modern_screen::site_id(level).and_then(text).or_else(|| e.map(|e| e.0.to_string())).unwrap_or_else(|| format!("Planet {level}")),
				loc: (level <= 24).then(|| text(0xb2b + level)).flatten().or_else(|| e.map(|e| e.1.to_string())).unwrap_or_default(),
			}
		})
		.collect();
	Some(Map { entries, cur, here, walk: None })
}

/// A window request: walk to an entry (and travel / pick it), or press one button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
	Row(usize, bool),
	Press(u16),
}

/// The pulses of a request over the one walk (`modern_walk::Walk`: Up / Down along the list, two fields pressed, two released). The
/// travel Cross is sent only when retail's cursor IS on the planet asked for; a Cross asked while a walk runs rides on it (sweep B8:
/// before, a Press replaced the walk and the ship flew to the in-between planet).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pulse(pub Walk);

impl Default for Pulse {
	fn default() -> Pulse {
		Pulse(Walk::new(UP, DOWN))
	}
}

impl Pulse {
	pub fn new(a: Ask) -> Pulse {
		let mut p = Pulse::default();
		p.ask(a);
		p
	}

	/// Takes a window request into the running walk: an entry walks there (and travels on arrival); Cross rides on a running walk;
	/// any other press ends the walk.
	pub fn ask(&mut self, a: Ask) {
		match a {
			Ask::Row(r, go) => self.0.to(r, if go { CROSS_B } else { 0 }),
			Ask::Press(CROSS_B) => self.0.confirm(CROSS_B),
			Ask::Press(p) => self.0.press(p),
		}
	}

	pub fn busy(&self) -> bool {
		self.0.busy()
	}

	/// One field: the game's buttons (the cursor clamps at both ends, it does not wrap).
	pub fn step(&mut self, cur: usize, n: usize) -> u16 {
		self.0.step(cur, n, true)
	}
}

#[derive(Default)]
struct Game {
	texts: Option<Texts>,
	retry: u32,
	pulse: Pulse,
}

static GAME: Mutex<Option<Game>> = Mutex::new(None);
static MAP: Mutex<Option<Map>> = Mutex::new(None);
static ASK: Mutex<Option<Ask>> = Mutex::new(None);
/// The window can draw the page (UI pipes fine, the world drawn behind it).
static READY: AtomicBool = AtomicBool::new(false);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn set_ready(ok: bool) {
	READY.store(ok, Ordering::Relaxed);
}

pub fn map() -> Option<Map> {
	lock(&MAP).clone()
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

/// Game thread, once per field after `modern_vendor::filter`: the buttons the game gets. Without a modern map the pad is returned
/// unchanged, bit for bit; with one the pad still reaches the game (its own Up / Down / Cross / Triangle) plus the window's pulses.
pub fn filter(ram: &[u8], b: u16, s: [u8; 4]) -> (u16, [u8; 4]) {
	let mut gg = lock(&GAME);
	let g = gg.get_or_insert_with(Game::default);
	let on = READY.load(Ordering::Relaxed) && crate::modern_open::modern() && !rac_engine::ui::is_open();
	let map = if on {
		let t = texts(ram, g);
		read(ram, &|id| t.and_then(|t| t.get(ram, id)))
	} else {
		None
	};
	let Some(map) = map else {
		*lock(&MAP) = None;
		*lock(&ASK) = None;
		g.pulse = Pulse::default();
		return (b, s);
	};
	if let Some(a) = lock(&ASK).take() {
		g.pulse.ask(a);
	}
	let mut out = b;
	if g.pulse.busy() {
		out = (out & !crate::modern_open::MENU_BTNS) | g.pulse.step(map.cur, map.entries.len());
	}
	let mut map = map;
	map.walk = g.pulse.0.target();
	*lock(&MAP) = Some(map);
	(out, s)
}

#[cfg(test)]
#[path = "modern_map_tests.rs"]
mod tests;
