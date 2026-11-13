//! [PC] R&C2's modern Options list (build 45 P4B, sweep B19 + A2; R&C1's twin is the Options tab of `rac1/modern_menu/pause.rs`
//! `list_parts` with the PC rows of `tabs.rs` / `pcext/quitrow.rs`), GAME-THREAD side: the retail Options list (page 8, list depth 1)
//! of the pause menu AND of the title, read from RAM (`read`), drawn modern by `host/screens_optlist.rs` from
//! `modern_optlist_page.rs`. The retail list keeps running underneath: its own rows (Help Desk, Save, Load, Controls, Sound, Camera,
//! Quit Game; at the title Language, 16:9, Adjust screen, Vibration) are picked by retail's own cursor and buttons (the player's
//! pad passes, the mouse walks the cursor with `modern_walk::Walk`, Cross only on arrival). The PC rows R&C1 shows in its list
//! are host rows here (retail has no record for them, nothing is written to game RAM): Display and Graphics (and Camera at the title)
//! open the engine's PC Options page on that tab (`modern_open::request_tab`, front = the title's), Quit to Desktop (pause) asks the
//! title's question and quits on Yes (`rac_engine::quit::request`, as R&C1's quit row's Circle). The keys reach a PC row like any
//! other: Up / Down step over them (the press that lands on one is kept from the game until released). Classic menu style, refused
//! UI pipes or no world behind the pause: nothing is read and the pad is returned bit for bit.
//!
//! Facts [C] (rac2/notes_menus.md item 5 + RAM of fg1 0001, pause Options up on level 0, 2026-11-12):
//! - page record +0xe4 id 8 = Options (pause and title), its parent the root (id 2, parent 0); pause = menu manager != 0
//!   (`modern_screen::gate`). List object L = [menu manager word] + 0x3c210 (`modern_open::Snap::manager`), depth [L+0x31c] = 1,
//!   cursor [L+0x1bc], row table [L+0x314]: 0x14-byte records {scale float, text id, kind, page record, 0}, ended by a zero record
//!   (0001: 7 rows ba4 ba5 ba6 b72 ba8 ba9 bac, cursor 3; the scale is not always 1.0: the title table has 0.8 / 0.85 rows).
//! - Sub pages the rows open: Help Desk 0x13, Save 0x1e, Load 0x1f, Sound 0x20, Camera 0x1d; Quit Game kind 0xc, no page.
//! - The title's list [C] (corrected 2026-11-12: was [I] "the same list object through the title's manager word"; that word is 0
//!   at the title, so the list stayed classic in the build 45 tour): its own list widget, `TITLE_LIST_CURSOR` / `TITLE_LIST_TABLE`
//!   (`list_of`). A table that does not read as rows leaves the title's classic picture.
use crate::modern_open::{self, MENU_BTNS};
use crate::modern_screen::{self as scr, site_id, Kind, Texts, BOLTS, DOWN, LEVEL, PAUSE_IDS, START, TITLE_STATE, UP};
use crate::modern_optlist_sound as sound; // [b46 W4]
use crate::modern_optlist_quit as quit; // [b47 X3]
use crate::modern_walk::Walk;
use rac_engine::input::binds::{BIT, CIRCLE, CROSS, TRIANGLE};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

pub const OPTIONS_PAGE: u32 = 8;
pub const LIST_OFF: u32 = 0x3c210;
pub const ROW_SIZE: u32 = 0x14;
pub const MAX_ROWS: usize = 12;
/// Text ids [C]: the Camera and Quit Game rows, the page captions (pause "Options" 0xb67, title "Options" 0xba3).
pub const CAMERA_TEXT: u32 = 0xba9;
pub const QUIT_TEXT: u32 = 0xbac;
pub const CAPTION_PAUSE: u32 = 0xb67;
pub const CAPTION_TITLE: u32 = 0xba3;
/// The title's Options list [C RAM of a cold boot with the title Options up, 2026-11-12, tour\find_title]: the title has no menu
/// manager (word 0); its lists are stand-alone list widgets 0xf0 apart. Root: cursor 0x1ff6ae8 (`modern_screen::TITLE_CURSOR`), row
/// table pointer 0x1ff6af0 -> 0x2612e0. Options: cursor 0x1ff69f8, row table pointer 0x1ff6a00 -> 0x261330 (bb1 Language,
/// 10fc 16:9, 1b73 Adjust screen, bab Vibration; each row's page record has the Options page as its parent).
pub const TITLE_LIST_CURSOR: u32 = scr::TITLE_CURSOR - 0xf0;
pub const TITLE_LIST_TABLE: u32 = TITLE_LIST_CURSOR + 8;
/// The pause root's Options tab (modern_tabs.rs).
pub const TAB: usize = 5;
/// What a PC row keeps from the game while it is focused: the D-pad and the face buttons but Triangle (Back still backs out).
const FOCUS_KEEP: u16 = 0xf0f0 & !BIT[TRIANGLE];

/// A host row (no retail record).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pc {
	Display,
	Graphics,
	Camera,
	Quit,
}

impl Pc {
	/// The engine page tab the row opens (None: Quit to Desktop).
	pub fn tab(self) -> Option<&'static str> {
		match self {
			Pc::Display => Some("display"),
			Pc::Graphics => Some("graphics"),
			Pc::Camera => Some("camera"),
			Pc::Quit => None,
		}
	}
}

/// A row of the modern list: retail's row k, or a host row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
	Retail(usize),
	Pc(Pc),
}

/// The modern list over retail's rows `ids` (text ids). Pause: Display and Graphics before retail's Camera row (R&C1's order: Sound,
/// Display, Graphics, Camera, Quit Game), Quit to Desktop last; without a Camera row a PC Camera row goes there too. Title: Display,
/// Graphics, Camera after retail's rows (R&C1's title list ends with its PC rows; the title root has its own Quit to Desktop). Pure.
pub fn layout(ids: &[u32], kind: Kind) -> Vec<Row> {
	let mut v: Vec<Row> = (0..ids.len()).map(Row::Retail).collect();
	match kind {
		Kind::Pause => {
			let cam = ids.iter().position(|&t| t == CAMERA_TEXT);
			let at = cam.or_else(|| ids.iter().position(|&t| t == QUIT_TEXT)).unwrap_or(ids.len());
			let mut pc = vec![Row::Pc(Pc::Display), Row::Pc(Pc::Graphics)];
			if cam.is_none() {
				pc.push(Row::Pc(Pc::Camera));
			}
			v.splice(at..at, pc);
			v.push(Row::Pc(Pc::Quit));
		}
		Kind::Title => v.extend([Row::Pc(Pc::Display), Row::Pc(Pc::Graphics), Row::Pc(Pc::Camera)]),
	}
	v
}

/// What one field's keys did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Out {
	/// The pad the game gets.
	pub pad: u16,
	/// Walk retail's cursor to this row (no press).
	pub walk: Option<usize>,
	/// A PC row chosen.
	pub act: Option<Pc>,
	/// Quit to Desktop answered Yes.
	pub yes: bool,
}

/// The host's part of the list: a focused PC row (an index of `layout`), the Quit question, the presses kept from the game.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Nav {
	pub focus: Option<usize>,
	pub ask: bool,
	/// Held presses kept from the game until released (a press that landed on a PC row is never a late edge in retail).
	swallow: u16,
}

impl Nav {
	/// The selected row of `rows` with retail's cursor on `cur`.
	pub fn sel(&self, rows: &[Row], cur: usize) -> usize {
		self.focus.filter(|&m| m < rows.len()).unwrap_or_else(|| rows.iter().position(|r| *r == Row::Retail(cur)).unwrap_or(0))
	}

	fn go(&mut self, rows: &[Row], cur: usize, t: usize, o: &mut Out) {
		match rows[t] {
			Row::Pc(_) => self.focus = Some(t),
			Row::Retail(j) => {
				self.focus = None;
				if j != cur {
					o.walk = Some(j);
				}
			}
		}
	}

	/// One field's pad: `b` held, `new` new presses, retail's cursor on `cur`. Pure.
	pub fn keys(&mut self, rows: &[Row], cur: usize, b: u16, new: u16) -> Out {
		self.swallow &= b;
		let mut o = Out::default();
		if self.ask {
			if new & BIT[CIRCLE] != 0 {
				o.yes = true;
			} else if new & BIT[TRIANGLE] != 0 {
				self.ask = false;
			}
			self.swallow |= b & MENU_BTNS & !START;
			o.pad = b & !self.swallow;
			return o;
		}
		let m = self.sel(rows, cur);
		let step = if new & DOWN != 0 {
			Some(m + 1).filter(|&t| t < rows.len())
		} else if new & UP != 0 {
			m.checked_sub(1)
		} else {
			None
		};
		if self.focus.is_some() {
			if let Some(t) = step {
				self.go(rows, cur, t, &mut o);
			} else if new & BIT[CROSS] != 0 {
				match rows[m] {
					Row::Pc(Pc::Quit) => self.ask = true,
					Row::Pc(p) => o.act = Some(p),
					Row::Retail(_) => {}
				}
			}
			self.swallow |= b & FOCUS_KEEP;
		} else if let Some(t) = step.filter(|&t| matches!(rows[t], Row::Pc(_))) {
			self.focus = Some(t);
			self.swallow |= new & (UP | DOWN);
		}
		o.pad = b & !self.swallow;
		o
	}
}

/// A window request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Req {
	/// Hover (`false`) or click (`true`) on row m of the layout.
	Row(usize, bool),
	Back,
	Answer(bool),
}

/// Applies a window request to the page state. Retail rows: a walk (Cross on arrival only); PC rows: focus, and on a click the action.
/// Pure.
pub fn apply(nav: &mut Nav, walk: &mut Walk, rows: &[Row], r: Req) -> (Option<Pc>, bool) {
	match r {
		Req::Answer(yes) if nav.ask => {
			nav.ask = yes;
			return (None, yes);
		}
		Req::Answer(_) => {}
		Req::Back if nav.ask => nav.ask = false,
		Req::Back => {
			nav.focus = None;
			walk.press(BIT[TRIANGLE]);
		}
		Req::Row(..) if nav.ask => {}
		Req::Row(m, pick) => match rows.get(m) {
			Some(Row::Pc(p)) => {
				walk.cancel();
				nav.focus = Some(m);
				if pick {
					if *p == Pc::Quit {
						nav.ask = true;
					} else {
						return (Some(*p), false);
					}
				}
			}
			Some(Row::Retail(j)) => {
				nav.focus = None;
				walk.to(*j, if pick { BIT[CROSS] } else { 0 });
			}
			None => {}
		},
	}
	(None, false)
}

/// What the modern list shows this field.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Facts {
	pub kind: Kind,
	pub rows: Vec<Row>,
	/// Retail's row texts (index = retail row).
	pub retail: Vec<String>,
	pub sel: usize,
	pub ask: bool,
	pub caption: String,
	/// The pause root's tab texts (empty at the title).
	pub tabs: Vec<String>,
	pub site: String,
	pub bolts: i32,
	/// [b46 W4] The Sound page is up instead of the list (`modern_optlist_sound.rs`): its rows replace `rows`.
	pub sound: Option<crate::modern_optlist_sound::Sound>,
	/// [b47 X3] Retail's Quit Game box is up: its body text (the box is drawn modern, `modern_optlist_quit.rs`).
	pub boxed: Option<String>,
}

fn rd(ram: &[u8], a: u32) -> u32 {
	let a = (a & 0x1ff_fffc) as usize;
	ram.get(a..a + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The text ids of the row table at `table` (None: it does not read as rows). Pure over RAM.
pub fn row_ids(ram: &[u8], table: u32) -> Option<Vec<u32>> {
	if !(0x10_0000..0x200_0000).contains(&table) {
		return None;
	}
	let mut ids = Vec::new();
	for k in 0..MAX_ROWS as u32 {
		let (scale, id) = (rd(ram, table + k * ROW_SIZE), rd(ram, table + k * ROW_SIZE + 4));
		if scale == 0 || id == 0 {
			break;
		}
		// A float 0.25..=2.0 and a text id of the game's table (0xa46..0x1bca [C notes_menus item 4]).
		if !(0x3e80_0000..=0x4000_0000).contains(&scale) || !(0xa46..0x1c00).contains(&id) {
			return None;
		}
		ids.push(id);
	}
	(!ids.is_empty()).then_some(ids)
}

struct Game {
	texts: Option<Texts>,
	retry: u32,
	nav: Nav,
	walk: Walk,
	prev: u16,
	last: u16,
}

// [b49 U1] `Walk::default()` has no buttons (back = fwd = 0): the list's walk must come from `Walk::new(UP, DOWN)`, or a hover and every
// key step from a PC row onto a retail row sent nothing and the highlight snapped back to retail's own row (Sound).
impl Default for Game {
	fn default() -> Game {
		Game { texts: None, retry: 0, nav: Nav::default(), walk: list_walk(), prev: 0, last: 0 }
	}
}

fn list_walk() -> Walk {
	Walk::new(UP, DOWN)
}

static GAME: Mutex<Option<Game>> = Mutex::new(None);
static FACTS: Mutex<Option<Facts>> = Mutex::new(None);
static REQ: Mutex<Vec<Req>> = Mutex::new(Vec::new());
static READY_PAUSE: AtomicBool = AtomicBool::new(false);
static READY_TITLE: AtomicBool = AtomicBool::new(false);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Window thread, each display frame: it can draw the pause list (UI pipes and the world) / the title list (UI pipes).
pub fn set_ready(pause: bool, title: bool) {
	READY_PAUSE.store(pause, Ordering::Relaxed);
	READY_TITLE.store(title, Ordering::Relaxed);
}

pub fn facts() -> Option<Facts> {
	lock(&FACTS).clone()
}

/// Window thread: a hover, click, back or answer for the next field.
pub fn request(r: Req) {
	let mut q = lock(&REQ);
	if q.len() < 8 {
		q.push(r);
	}
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

/// The Options list's row table and retail's cursor on it. Pause: the manager's list object at depth 1. Title: its own list widget,
/// taken only while the table's first row opens a child of the Options page `s.page` (so another list in that widget is never read).
fn list_of(ram: &[u8], s: &modern_open::Snap, kind: Kind) -> Option<(u32, u32)> {
	match kind {
		Kind::Pause => {
			if s.depth != 1 || s.manager == 0 {
				return None;
			}
			let list = s.manager.wrapping_add(LIST_OFF);
			Some((rd(ram, list + 0x314), rd(ram, list + 0x1bc)))
		}
		Kind::Title => {
			let table = rd(ram, TITLE_LIST_TABLE);
			let first = rd(ram, table.wrapping_add(0xc));
			(first != 0 && rd(ram, first + 0xe0) == s.page).then(|| (table, rd(ram, TITLE_LIST_CURSOR)))
		}
	}
}

/// The list of this field (rows, retail's cursor, texts) or None when no modern Options list is up.
fn read(ram: &[u8], g: &mut Game) -> Option<(Facts, usize)> {
	let s = modern_open::snap_now(ram)?;
	if s.page_id == sound::PAGE {
		return read_sound(ram, g, &s);
	}
	if s.page_id != OPTIONS_PAGE {
		return None;
	}
	let parent = rd(ram, s.page + 0xe0);
	if parent == 0 {
		return None;
	}
	let kind = scr::gate(rd(ram, parent + 0xe4), rd(ram, parent + 0xe0), scr::manager(ram, &s), rd(ram, TITLE_STATE))?;
	let ready = match kind {
		Kind::Pause => READY_PAUSE.load(Ordering::Relaxed),
		Kind::Title => READY_TITLE.load(Ordering::Relaxed),
	};
	if !ready || !modern_open::modern() {
		return None;
	}
	let (table, cur) = list_of(ram, &s, kind)?;
	let ids = row_ids(ram, table)?;
	let cur = cur as usize;
	if cur >= ids.len() {
		return None;
	}
	let t = texts(ram, g)?;
	let retail = ids.iter().map(|&id| t.get(ram, id).filter(|x| !x.is_empty())).collect::<Option<Vec<_>>>()?;
	let caption = t.get(ram, if kind == Kind::Pause { CAPTION_PAUSE } else { CAPTION_TITLE }).unwrap_or_default();
	let tabs = if kind == Kind::Pause { PAUSE_IDS.iter().map(|&id| t.get(ram, id).filter(|x| !x.is_empty())).collect::<Option<Vec<_>>>()? } else { Vec::new() };
	let site = site_id(rd(ram, LEVEL)).and_then(|id| t.get(ram, id)).unwrap_or_default();
	let rows = layout(&ids, kind);
	// [b47 X3] retail's Quit Game box (Pause: the list's Quit Game row, Cross)
	let boxed = (kind == Kind::Pause && crate::pc_input::find_mode(ram).is_some_and(|m| quit::up(ram, m))).then(|| t.get(ram, quit::BODY_ID).unwrap_or_default());
	Some((Facts { kind, rows, retail, sel: 0, ask: false, caption, tabs, site, bolts: rd(ram, BOLTS) as i32, sound: None, boxed }, cur))
}

/// [b46 W4] The Sound page of the pause Options list (record id 0x20 under the Options record under the pause root), read from the
/// widget object's words (`modern_optlist_sound.rs`). Pause only: the title's Options list has no Sound row.
fn read_sound(ram: &[u8], g: &mut Game, s: &modern_open::Snap) -> Option<(Facts, usize)> {
	let opt = rd(ram, s.page + 0xe0);
	let root = if opt == 0 { 0 } else { rd(ram, opt + 0xe0) };
	if root == 0 || rd(ram, opt + 0xe4) != OPTIONS_PAGE {
		return None;
	}
	let kind = scr::gate(rd(ram, root + 0xe4), rd(ram, root + 0xe0), scr::manager(ram, s), rd(ram, TITLE_STATE))?;
	if kind != Kind::Pause || !READY_PAUSE.load(Ordering::Relaxed) || !modern_open::modern() {
		return None;
	}
	let (sfx, music, stereo, cursor) = sound::read(ram, s.manager)?;
	let t = texts(ram, g)?;
	let text = sound::TEXT_IDS.iter().map(|&id| t.get(ram, id).filter(|x| !x.is_empty())).collect::<Option<Vec<_>>>()?;
	let tabs = PAUSE_IDS.iter().map(|&id| t.get(ram, id).filter(|x| !x.is_empty())).collect::<Option<Vec<_>>>()?;
	let site = site_id(rd(ram, LEVEL)).and_then(|id| t.get(ram, id)).unwrap_or_default();
	let caption = t.get(ram, sound::CAPTION).unwrap_or_default();
	let f = Facts { kind, caption, tabs, site, bolts: rd(ram, BOLTS) as i32, sel: cursor, sound: Some(sound::Sound { sfx, music, stereo, cursor, text }), ..Default::default() };
	Some((f, cursor))
}

/// [b46 W4] One field on the Sound page: the pad passes bit for bit; only a Back asked by the mouse (right click, the Back hint) or a
/// tab click is sent, as a Triangle pulse (retail's own back button). Nothing else is ever sent to the game from this page.
fn sound_field(g: &mut Game, b: u16) -> u16 {
	let was_busy = g.walk.busy(); // [b47 R1] before the window's presses: a button held last field is no new edge
	for r in std::mem::take(&mut *lock(&REQ)) {
		if r == Req::Back {
			g.walk.press(BIT[TRIANGLE]);
		}
	}
	if crate::modern_tabs::want_back(TAB) {
		g.walk.press(BIT[TRIANGLE]);
	}
	let mut out = b;
	if g.walk.busy() {
		let w = if !was_busy && g.last & MENU_BTNS != 0 { 0 } else { g.walk.step(0, 3, true) };
		out = (out & !MENU_BTNS) | w;
	}
	g.last = out;
	out
}

/// [b47 X3] One field with retail's Quit Game box up: the pad passes bit for bit (Cross = Yes, Triangle = No are the game's own);
/// an answer asked by the window (button click, Enter / Esc, right click) is sent as that button's pulse.
fn box_field(g: &mut Game, b: u16) -> u16 {
	let was_busy = g.walk.busy(); // [b47 R1] as the list path: before the presses
	for r in std::mem::take(&mut *lock(&REQ)) {
		match r {
			Req::Answer(yes) => g.walk.press(BIT[if yes { CROSS } else { TRIANGLE }]),
			Req::Back => g.walk.press(BIT[TRIANGLE]),
			Req::Row(..) => {}
		}
	}
	let mut out = b;
	if g.walk.busy() {
		let w = if !was_busy && g.last & MENU_BTNS != 0 { 0 } else { g.walk.step(0, 3, true) };
		out = (out & !MENU_BTNS) | w;
	}
	g.nav = Nav::default();
	g.last = out;
	out
}

/// Game thread, once per field (called first in `modern_open::filter`, so a Cross the list lets through on the Camera row still
/// opens the Camera tab): the buttons and sticks the game gets. Without a modern list the pad is returned bit for bit.
pub fn filter(ram: &[u8], b: u16, s: [u8; 4]) -> (u16, [u8; 4]) {
	let mut gg = lock(&GAME);
	let g = gg.get_or_insert_with(Game::default);
	let new = b & !g.prev;
	g.prev = b;
	if rac_engine::ui::is_open() {
		// The PC page is up over the list: the focus is kept for the way back (R&C1 returns to the row).
		*lock(&FACTS) = None;
		lock(&REQ).clear();
		g.last = b;
		return (b, s);
	}
	let Some((mut f, cur)) = read(ram, g) else {
		*lock(&FACTS) = None;
		lock(&REQ).clear();
		(g.nav, g.walk, g.last) = (Nav::default(), list_walk(), b);
		return (b, s);
	};
	if f.sound.is_some() {
		let out = sound_field(g, b);
		*lock(&FACTS) = Some(f);
		return (out, s);
	}
	if f.boxed.is_some() {
		let out = box_field(g, b); // [b47 X3]
		*lock(&FACTS) = Some(f);
		return (out, s);
	}
	let (mut act, mut yes) = (None, false);
	for r in std::mem::take(&mut *lock(&REQ)) {
		let (a, y) = apply(&mut g.nav, &mut g.walk, &f.rows, r);
		act = act.or(a);
		yes |= y;
	}
	// A tab clicked on the tab bar (modern_tabs.rs): back to the root first.
	if f.kind == Kind::Pause && crate::modern_tabs::want_back(TAB) {
		g.nav = Nav::default();
		g.walk.press(BIT[TRIANGLE]);
	}
	let was_busy = g.walk.busy();
	let mut out = b;
	if !was_busy {
		let o = g.nav.keys(&f.rows, cur, b, new);
		out = o.pad;
		if let Some(j) = o.walk {
			g.walk.to(j, 0);
		}
		act = act.or(o.act);
		yes |= o.yes;
	}
	if g.walk.busy() {
		// A press held last field is no new edge: a walk that starts now waits one field.
		let w = if !was_busy && g.last & MENU_BTNS != 0 { 0 } else { g.walk.step(cur, f.retail.len(), true) };
		out = (out & !MENU_BTNS) | w;
	}
	if let Some(tab) = act.and_then(Pc::tab) {
		modern_open::request_tab(tab, f.kind == Kind::Title);
	}
	if yes {
		rac_engine::quit::request();
	}
	g.last = out;
	f.sel = g.nav.sel(&f.rows, g.walk.target().unwrap_or(cur));
	f.ask = g.nav.ask;
	*lock(&FACTS) = Some(f);
	(out, s)
}

#[cfg(test)]
#[path = "modern_optlist_tests.rs"]
mod tests;
