//! [PC] R&C2's modern PAUSE and TITLE screens, GAME-THREAD side (lane scr, R&C1's `modern_menu/pause.rs` + `title.rs` + `title_quit.rs`
//! twins): which retail screen is up (`read`), the facts the page shows (the game's own texts, cursor, bolts, planet), and the pad
//! the game gets while the modern page is drawn (`filter`). The page itself is drawn by `host/screens.rs` (window thread) from
//! `modern_pages.rs` (pure builders). The retail menu keeps running underneath: choosing a row sends the game D-pad / Cross pulses
//! (`Drive`), so retail's code does the work; nothing is written to game RAM. Classic menu style, refused UI pipes or no world drawn
//! behind the pause: `filter` returns the pad bit for bit and nothing is drawn.
//!
//! Facts [C] (rac2/notes_modern_screens.md 2nd pass, RAM of runs\screens\ta + fg1 0262 / 0110, checked 2026-11-11):
//! - the menu globals block G (`modern_open::find`; G = 0x1f2840 at the title, per level in game): G+0x14 = the current page record,
//!   record +0xe4 page id (2 = the root of both the pause and the title menu), +0xe0 parent (0 for both roots).
//! - the menu manager word 0x1a90cc: 0x1fb8000 in game, 0 at the title. Pause root cursor = [0x1a90cc] + 0x39160 + 0x4ac (0..6:
//!   Weapons Gadgets Quick Select Items Help Options Special, wraps); title cursor 0x1ff6ae8 (0 New Game, 1 Load, 2 Options, wraps).
//! - G+0 at the title: 2 during the logos, 4 once the menu panels show [I: meaning; values C].
//! - Up 0x1000 / Down 0x4000 on the new-press word move both cursors; Cross 0x40 picks; Triangle 0x10 backs out of the pause, Start
//!   0x800 closes it (handler 0x38a540).
//! - Texts: the game's text records (16 bytes: +0 string pointer, +4 id) sit in one table that moves per level; found by the pair of
//!   records 0xb62 "Weapons" / 0xb63 next to each other (`Texts`). Planet names 0xb45 + level for levels 0..=23 [C names, order I],
//!   0x1b23 Wupash Nebula (25), 0x1b22 Jamming Array (26).
use crate::modern_open;
use rac_engine::input::binds::{BIT, CIRCLE, CROSS, DPAD, TRIANGLE};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

pub const UP: u16 = BIT[DPAD];
pub const DOWN: u16 = BIT[DPAD + 1];
pub const LEFT: u16 = BIT[DPAD + 2];
pub const RIGHT: u16 = BIT[DPAD + 3];
pub const START: u16 = 0x800;
/// Menu manager word and the pause cursor's offset in its object [C].
pub const MANAGER: u32 = 0x1a90cc;
pub const PAUSE_CURSOR_OFF: u32 = 0x39160 + 0x4ac;
pub const TITLE_CURSOR: u32 = 0x1ff6ae8;
/// The title's state word (= G at the title): 4 once the panels show.
pub const TITLE_STATE: u32 = 0x1f2840;
pub const BOLTS: u32 = 0x1a7a80;
pub const LEVEL: u32 = 0x1a7a78;
/// The language word (0 English, 2 French, 3 German, 4 Spanish, 5 Italian) [C rac2/notes_gamefacts.md].
pub const LANG_WORD: u32 = 0x1a7c3c;
/// The pause root's rows (text ids) and the title root's.
pub const PAUSE_IDS: [u32; 7] = [0xb62, 0xb63, 0xb64, 0xb65, 0xb66, 0xb67, 0xbc4];
pub const TITLE_IDS: [u32; 3] = [0xbb0, 0xba6, 0xba3];
/// The title's virtual fourth entry, Quit to Desktop (host-owned, R&C1 `title_quit::ROW`).
pub const QUIT_ROW: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Kind {
	#[default]
	Pause,
	Title,
}

/// What the modern page shows this field.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Facts {
	pub kind: Kind,
	/// The game's cursor row.
	pub cur: usize,
	/// The root's row words, the game's own texts.
	pub rows: Vec<String>,
	pub site: String,
	pub bolts: i32,
}

/// The menu manager of this field for the pause pages: [0x1a90cc] (`MANAGER`), or where that word is 0 outside the title, the
/// word the level's own code names (`Snap::manager`). Levels 1 and 25 keep it 0x40 higher, at 0x1a910c [C: RAM of fg1 0025 with the
/// pause up on level 1, 2026-11-12: [0x1a90cc] = 0, [0x1a910c] = 0x1fb8000, cursor offsets as on the other levels]; with the
/// constant alone every modern pause page stayed classic there (user report, Oozla). At the title [0x1a90cc] is 0 and the code's
/// word is not (the title's own manager, 0x1fb8000 at 0x1a8dac [C fg1 0000]), also during the logos: the title's G (= 0x1f2840, no
/// level's G is there [C fg1 levels 1-12, 22, 24-26]) and the title state word keep it out, so `gate` still sees the title.
pub fn manager(ram: &[u8], s: &modern_open::Snap) -> u32 {
	let m = rd(ram, MANAGER);
	if m != 0 || s.g == TITLE_STATE || rd(ram, TITLE_STATE) == 4 {
		m
	} else {
		s.manager
	}
}

/// Which root is up: page id / parent of the current record, the manager word, the title state word. Pure.
pub fn gate(page_id: u32, parent: u32, manager: u32, title_state: u32) -> Option<Kind> {
	if page_id != 2 || parent != 0 {
		return None;
	}
	if manager != 0 {
		return Some(Kind::Pause);
	}
	(title_state == 4).then_some(Kind::Title)
}

/// [PC] Whether the pause root is the current page in `ram` (read only; either menu style). The game shows it from the field it
/// re-points its camera at [C: fg1 0357 RAM dumps, Esc at 300: page 2 / parent 0 from field 302 to 330, the camera's re-point at
/// 302, the GS's frozen run from 304]: host/freeze.rs holds the world from that field on.
pub fn pause_up(ram: &[u8]) -> bool {
	modern_open::snap_now(ram).is_some_and(|s| matches!(gate(s.page_id, rd(ram, s.page + 0xe0), manager(ram, &s), rd(ram, TITLE_STATE)), Some(Kind::Pause)))
}

fn rd(ram: &[u8], a: u32) -> u32 {
	let a = (a & 0x1ff_fffc) as usize;
	ram.get(a..a + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// A game string as the PC font shows it (`rac_formats::font_bytes`; control bytes and button glyphs dropped), trimmed.
pub fn decode(bytes: &[u8]) -> String {
	let s: String = bytes.iter().take_while(|&&b| b != 0).filter_map(|&b| if (0x20..0x7f).contains(&b) { Some(b as char) } else { rac_formats::font_bytes::font_char(b) }).collect();
	s.trim().to_string()
}

/// The game's text table: first record and record count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Texts {
	pub start: u32,
	pub n: u32,
	anchor: u32,
	/// The language byte the table was chosen for.
	lang: u8,
}

fn rec_ok(ram: &[u8], a: u32) -> bool {
	let p = rd(ram, a);
	(0x10_0000..0x200_0000).contains(&p) && rd(ram, a + 4) < 0x1_0000
}

impl Texts {
	fn anchor_ok(ram: &[u8], a: u32) -> bool {
		rd(ram, a + 4) == 0xb62 && rd(ram, a + 20) == 0xb63 && rec_ok(ram, a)
	}

	/// The text table column of the language word: 0 English, 1 French, 2 German, 3 Spanish, 4 Italian (0 or > 5 = English) [C: boot
	/// ELF script column rule, rac2/notes_menus.md "LANGUAGE TEXT TABLES"].
	pub fn column(lang: u8) -> usize {
		if (2..=5).contains(&lang) { lang as usize - 1 } else { 0 }
	}

	/// Scans RAM for the anchor pair and walks out to the table's ends. [b49 Z3] At the title all five languages' tables are in RAM
	/// (English, French, German, Spanish, Italian, in this order of address): the one of the language word's column is taken (the
	/// first would be English in every language). In a level RAM holds the selected language's table alone: that one.
	/// [b50 V4] The full-RAM scan runs only when needed: the anchors found last time are checked first, and a miss is remembered
	/// until the language, level or menu mode changes (`Finder`, one per thread).
	pub fn find(ram: &[u8]) -> Option<Texts> {
		FINDER.with(|f| f.borrow_mut().find(ram))
	}

	/// The anchors in address order, up to `want` of them: the scan stops at the want-th.
	fn scan(ram: &[u8], want: usize) -> Vec<u32> {
		let top = (ram.len() as u32).min(0x200_0000).saturating_sub(32);
		(0x10_0000..top).step_by(4).filter(|&a| Texts::anchor_ok(ram, a)).take(want).collect()
	}

	/// The table around `anchor`, for the language `lang`.
	fn around(ram: &[u8], anchor: u32, lang: u8) -> Texts {
		let (mut s, mut e) = (anchor, anchor);
		while s >= 16 && rec_ok(ram, s - 16) {
			s -= 16;
		}
		while rec_ok(ram, e + 16) {
			e += 16;
		}
		Texts { start: s, n: (e - s) / 16 + 1, anchor, lang }
	}

	/// [b50 V4] Still the table of the current language word (a language change at the title moves the column).
	pub fn still(&self, ram: &[u8]) -> bool {
		Texts::anchor_ok(ram, self.anchor) && lang_of(ram) == self.lang
	}

	/// The text of `id` (None: no record).
	pub fn get(&self, ram: &[u8], id: u32) -> Option<String> {
		self.get_n(ram, id, 120)
	}

	/// `get` for long texts (descriptions): reads up to `n` bytes of the record's string.
	pub fn get_n(&self, ram: &[u8], id: u32, n: usize) -> Option<String> {
		let a = (0..self.n).map(|k| self.start + 16 * k).find(|&a| rd(ram, a + 4) == id)?;
		let p = (rd(ram, a) & 0x1ff_ffff) as usize;
		ram.get(p..(p + n).min(ram.len())).map(decode)
	}
}

fn lang_of(ram: &[u8]) -> u8 {
	ram.get(LANG_WORD as usize).copied().unwrap_or(0)
}

/// [b50 V4] What `Texts::find` remembers between calls (it is called by every page reader on every cache miss).
#[derive(Default)]
pub struct Finder {
	/// Anchors of the last scan, in address order; `complete` = the scan ran to the end of RAM (no more exist).
	anchors: Vec<u32>,
	complete: bool,
	/// (language, level, title state, manager) of the last scan that found nothing, and the calls answered from it since.
	miss: Option<[u32; 4]>,
	skipped: u32,
	/// Full-RAM scans made (tests).
	pub scans: u32,
}

/// Calls answered from a remembered miss before the RAM is scanned again anyway (a table can appear without any key changing).
const MISS_RECHECK: u32 = 300;

impl Finder {
	fn key(ram: &[u8]) -> [u32; 4] {
		[lang_of(ram) as u32, rd(ram, LEVEL), rd(ram, TITLE_STATE), rd(ram, MANAGER)]
	}

	pub fn find(&mut self, ram: &[u8]) -> Option<Texts> {
		let lang = lang_of(ram);
		let col = Texts::column(lang);
		// The known place first: every anchor of the last scan must still be one.
		if !self.anchors.is_empty() && self.anchors.iter().all(|&a| Texts::anchor_ok(ram, a)) && (self.complete || self.anchors.len() > col) {
			let a = *self.anchors.get(col).unwrap_or(&self.anchors[0]);
			return Some(Texts::around(ram, a, lang));
		}
		let key = Finder::key(ram);
		if self.miss == Some(key) && self.skipped < MISS_RECHECK {
			self.skipped += 1;
			return None;
		}
		self.scans += 1;
		self.anchors = Texts::scan(ram, col + 1);
		self.complete = self.anchors.len() < col + 1;
		self.skipped = 0;
		if self.anchors.is_empty() {
			self.miss = Some(key);
			return None;
		}
		self.miss = None;
		let a = *self.anchors.get(col).unwrap_or(&self.anchors[0]);
		Some(Texts::around(ram, a, lang))
	}
}

thread_local! {
	static FINDER: std::cell::RefCell<Finder> = std::cell::RefCell::new(Finder::default());
}

/// The planet name's text id of a level (None: no name known).
pub fn site_id(level: u32) -> Option<u32> {
	match level {
		0..=23 => Some(0xb45 + level),
		25 => Some(0x1b23),
		26 => Some(0x1b22),
		_ => None,
	}
}

#[derive(Default)]
struct Game {
	texts: Option<Texts>,
	/// Fields until the next table scan after a failed one.
	retry: u32,
	drive: Drive,
	prev: u16,
	/// The request sequence `drive` was made for.
	seq: u32,
	/// [PC] nav lane: the pad this filter returned last field (the edge rule of `filter`).
	last: u16,
}

/// The pulses that bring the game's cursor to a row (and pick it): two fields pressed, two released, so every press is a new edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Drive {
	pub target: Option<usize>,
	pub confirm: bool,
	pub back: bool,
	pub phase: u8,
	held: u16,
	/// [PC] nav lane: the game's cursor word (0 = presses only): a move is one field (modern_jump.rs `list_preset`).
	pub at: u32,
	/// [PC] nav lane: a tab switch (modern_tabs.rs): write the cursor to the target and press Cross in the same field.
	pub direct: bool,
	/// [b45 P3] B21: walk presses sent for `target`; after `n + 2` the walk gives up (no Cross, the player's buttons free again).
	pub walked: u8,
}

impl Drive {
	/// One field: the menu buttons to press. `cur` = the game's row, `n` = rows (the cursor wraps).
	pub fn step(&mut self, cur: usize, n: usize) -> u16 {
		if self.phase % 4 == 0 {
			self.held = self.next(cur, n);
		}
		if self.held == 0 && self.phase % 4 == 0 {
			return 0;
		}
		let out = if self.phase % 4 < 2 { self.held } else { 0 };
		self.phase = (self.phase + 1) % 4;
		if self.phase == 0 && self.held & (BIT[CROSS] | BIT[TRIANGLE]) != 0 {
			(self.confirm, self.back, self.target) = (false, false, None);
		}
		out
	}

	fn next(&mut self, cur: usize, n: usize) -> u16 {
		if self.back {
			return BIT[TRIANGLE];
		}
		let Some(t) = self.target.filter(|&t| t < n) else { return 0 };
		if t != cur && self.at != 0 {
			if self.direct && self.confirm {
				crate::modern_jump::queue(self.at, t as u32);
				return BIT[CROSS];
			}
			if let Some((pre, p)) = crate::modern_jump::list_preset(n, cur, t) {
				if pre != cur {
					crate::modern_jump::queue(self.at, pre as u32);
				}
				return p;
			}
		}
		if t != cur {
			if self.walked as usize >= n + 2 {
				// A row retail's cursor never stops on: give the walk up without a press (sweep B21).
				(self.target, self.confirm, self.walked) = (None, false, 0);
				return 0;
			}
			self.walked = self.walked.saturating_add(1);
			let fwd = (t + n - cur) % n;
			return if fwd <= n - fwd { DOWN } else { UP };
		}
		self.walked = 0;
		if self.confirm {
			return BIT[CROSS];
		}
		self.target = None;
		0
	}

	pub fn busy(&self) -> bool {
		self.target.is_some() || self.back || self.phase != 0
	}
}

/// The pause root's pad: Left / Right walk the rows like Up / Down (the tab bar runs across), Circle is kept from the game (a
/// mouse click is the game's Circle in R&C2's keyboard table; the root ignores it [C handler 0x38a540], this keeps it so).
pub fn pause_pad(b: u16) -> u16 {
	let mut o = b & !(LEFT | RIGHT | BIT[CIRCLE]);
	if b & LEFT != 0 {
		o |= UP;
	}
	if b & RIGHT != 0 {
		o |= DOWN;
	}
	o
}

/// The title's Quit to Desktop entry (R&C1 `title_quit`): focus and question.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Quit {
	pub focus: bool,
	pub ask: bool,
}

/// What a title field's new presses `new` do to `q` with the game's cursor on `cur`; returns the pad the game gets (`b` = held) and
/// whether Yes was chosen. Down from Options focuses the entry; Up leaves it; Cross asks; Circle / Start = Yes, Triangle = No.
pub fn title_pad(q: &mut Quit, cur: usize, b: u16, new: u16) -> (u16, bool) {
	if q.ask {
		if new & BIT[CIRCLE] != 0 { // [b45 P4A] B20: Start no longer answers Yes (R&C1: Circle only)
			return (0, true);
		}
		if new & BIT[TRIANGLE] != 0 {
			q.ask = false;
		}
		return (0, false);
	}
	if q.focus {
		if new & UP != 0 {
			q.focus = false;
		} else if new & BIT[CROSS] != 0 {
			q.ask = true;
		}
		return (0, false);
	}
	if cur == TITLE_IDS.len() - 1 && new & DOWN != 0 {
		q.focus = true;
		return (b & !DOWN, false);
	}
	(b & !BIT[CIRCLE], false)
}

static GAME: Mutex<Option<Game>> = Mutex::new(None);
/// The facts of the newest field (window reads).
static FACTS: Mutex<Option<Facts>> = Mutex::new(None);
static QUIT: Mutex<Quit> = Mutex::new(Quit { focus: false, ask: false });
/// The window draws the page (UI pipes fine; for the pause also the world behind it).
static READY_PAUSE: AtomicBool = AtomicBool::new(false);
static READY_TITLE: AtomicBool = AtomicBool::new(false);
/// The window's request: row, pick, back (`request`), with a sequence number.
static REQ: Mutex<Option<(usize, bool, bool)>> = Mutex::new(None);
static SEQ: AtomicU32 = AtomicU32::new(0);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Window thread, each display frame: whether it can draw the pause page (UI pipes and the world) and the title page (UI pipes).
pub fn set_ready(pause: bool, title: bool) {
	READY_PAUSE.store(pause, Ordering::Relaxed);
	READY_TITLE.store(title, Ordering::Relaxed);
}

pub fn facts() -> Option<Facts> {
	lock(&FACTS).clone()
}

pub fn quit_state() -> Quit {
	*lock(&QUIT)
}

/// Window thread: go to `row` (and pick it); `back` = Triangle. The title's Quit row is the host's own (focus / ask, no pulse).
pub fn request(row: usize, pick: bool, back: bool) {
	let title = lock(&FACTS).as_ref().is_some_and(|f| f.kind == Kind::Title);
	if title {
		let mut q = lock(&QUIT);
		if q.ask {
			return;
		}
		q.focus = row == QUIT_ROW && !back;
		if q.focus {
			q.ask = pick;
			return;
		}
	}
	*lock(&REQ) = Some((row, pick, back));
	SEQ.fetch_add(1, Ordering::Relaxed);
}

/// Window thread: the Quit question's answer.
pub fn answer(yes: bool) {
	let mut q = lock(&QUIT);
	if !q.ask {
		return;
	}
	if yes {
		rac_engine::quit::request();
	} else {
		q.ask = false;
	}
}

/// The facts of this field, or None when no modern root is up.
fn read(ram: &[u8], g: &mut Game) -> Option<Facts> {
	let s = modern_open::snap_now(ram);
	let mgr = s.as_ref().map_or(0, |s| manager(ram, s));
	// [PC] The page the menu is on, for the window's hold over retail's short gaps (host/screens_hold.rs); no RAM write.
	let parent = |p: u32| if p == 0 { 0 } else { rd(ram, p + 0xe4) };
	// [b45 P3] B13: with the game mode word, so the vendor / Ship Shack / Galactic Map pages are held over short gaps too.
	let mode = rd(ram, if rd(ram, LEVEL) == 1 { crate::modern_vendor::MODE_L1 } else { crate::modern_vendor::MODE });
	crate::host::screens_hold::note(s.as_ref().map_or(0, |s| s.page_id), s.as_ref().map_or(0, |s| parent(rd(ram, s.page + 0xe0))), mgr != 0, mode);
	let s = s?;
	let page = s.page;
	let kind = gate(s.page_id, rd(ram, page + 0xe0), mgr, rd(ram, TITLE_STATE))?;
	let ready = match kind {
		Kind::Pause => READY_PAUSE.load(Ordering::Relaxed),
		Kind::Title => READY_TITLE.load(Ordering::Relaxed),
	};
	if !ready || !modern_open::modern() {
		return None;
	}
	let texts = texts(ram, g)?;
	let (ids, cur): (&[u32], u32) = match kind {
		Kind::Pause => (&PAUSE_IDS, rd(ram, mgr.wrapping_add(PAUSE_CURSOR_OFF))),
		Kind::Title => (&TITLE_IDS, rd(ram, TITLE_CURSOR)),
	};
	let rows = ids.iter().map(|&id| texts.get(ram, id).filter(|t| !t.is_empty())).collect::<Option<Vec<_>>>()?;
	if cur as usize >= rows.len() {
		return None;
	}
	let level = rd(ram, LEVEL);
	let site = site_id(level).and_then(|id| texts.get(ram, id)).unwrap_or_default();
	Some(Facts { kind, cur: cur as usize, rows, site, bolts: rd(ram, BOLTS) as i32 })
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

/// Game thread, once per field after the Options door (`modern_open::filter`): the buttons and sticks the game gets.
pub fn filter(ram: &[u8], b: u16, s: [u8; 4]) -> (u16, [u8; 4]) {
	let mut gg = lock(&GAME);
	let g = gg.get_or_insert_with(Game::default);
	let new = b & !g.prev;
	g.prev = b;
	let f = if rac_engine::ui::is_open() { None } else { read(ram, g) };
	let Some(f) = f else {
		*lock(&FACTS) = None;
		*lock(&QUIT) = Quit::default();
		(g.drive, g.seq, g.last) = (Drive::default(), SEQ.load(Ordering::Relaxed), b);
		crate::modern_tabs::root_gone(modern_open::page_id_now(ram).is_some()); // [PC] nav lane
		return (b, s);
	};
	let seq = SEQ.load(Ordering::Relaxed);
	if seq != g.seq {
		g.seq = seq;
		if let Some((row, pick, back)) = lock(&REQ).take() {
			g.drive = Drive { target: Some(row), confirm: pick, back, at: cursor_at(ram, f.kind), ..Drive::default() };
		}
	}
	// [PC] nav lane: a tab clicked on a sub page (modern_tabs.rs): the root cursor written to it and Cross, in one field.
	if f.kind == Kind::Pause && !g.drive.busy() {
		if let Some(tab) = crate::modern_tabs::take_root() {
			g.drive = Drive { target: Some(tab), confirm: true, at: cursor_at(ram, f.kind), direct: true, ..Drive::default() };
		}
	}
	let mut out = match f.kind {
		Kind::Pause => pause_pad(b),
		Kind::Title => {
			let mut q = lock(&QUIT);
			let (o, yes) = title_pad(&mut q, f.cur, b, new);
			if yes {
				rac_engine::quit::request();
			}
			o
		}
	};
	if g.drive.busy() {
		// [PC] nav lane: a press held last field is no new edge (a new request mid-pulse): wait one field.
		let w = if g.drive.phase == 0 && g.last & modern_open::MENU_BTNS != 0 { 0 } else { g.drive.step(f.cur, f.rows.len()) };
		out = (out & !modern_open::MENU_BTNS) | w;
	}
	g.last = out;
	*lock(&FACTS) = Some(f);
	(out, s)
}

/// [PC] nav lane: the root's cursor word for one-field moves; 0 on the title (three rows: never more than one press away).
fn cursor_at(ram: &[u8], kind: Kind) -> u32 {
	if kind == Kind::Pause {
		crate::modern_jump::at(crate::modern_jump::manager(ram), PAUSE_CURSOR_OFF)
	} else {
		0
	}
}

#[cfg(test)]
#[path = "modern_screen_tests.rs"]
mod tests;
