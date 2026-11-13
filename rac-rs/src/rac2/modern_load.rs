//! [PC] R&C2's modern save-slot pages, GAME-THREAD side (lane ld, build 44; R&C1's modern New Game / Load Game slot cards,
//! rac1/modern_menu/title.rs `cards` + title_slots.rs): which slot page is up, the four save slots it lists (retail's own slot
//! table) and the pad the game gets while the modern page is drawn (`filter`). Window side `host/screens_load.rs`, builder
//! `modern_load_page.rs`. Retail's page keeps running underneath: a card chosen with the mouse sends the game Up / Down pulses
//! (its cursor stops at both ends, no wrap) and then Cross; nothing is written to game RAM. Retail's message box over a Save page
//! (overwrite?, saving) is drawn as a modern dialog over the cards (R&C1 pause.rs `Page::Overwrite`); its Yes / No send Circle /
//! Triangle. Classic menu style or refused UI pipes: `filter` returns the pad bit for bit and nothing is drawn.
//!
//! Facts [C] (lane ld runs bisect\runs\ex1, ex2, ng1 on rc2_b43.exe with the user's card of 4 saves; retail code 0x2de710..,
//! 2026-11-12):
//! - At the title both slot pages have page id 31 (record +0xe4) with the title root (id 2) as parent: Load, and New Game's "Save
//!   Game" page (Circle = Continue without saving); they differ by the record word +0xf0: 0x1ab0d8 Load, 0x1ab108 Save (title runs:
//!   Load record 0x260b58, Save 0x260c88). In the pause the Options list (id 8) opens Save = page id 30 and Load = page id 31, other
//!   records with other +0xf0 words (Oozla: 0x1ab3c8 Save, 0x1ab398 Load: the level overlay's twins of the title's pair) [C RAM
//!   bisect\dumps\pz\ram02620.bin, Options items 0xba5 Save / 0xba6 Load]: there the page id tells them apart (`slot_mode`).
//! - The cursor: [record +0xe8] (the page descriptor) +0x40, 0..3; Down stops at 3 (ex2 shots 640..940; pause Save the same).
//! - Retail's message box over a Save page is a page of its own: id 26, parent = the Save page (title record 0x260ea8, pause
//!   0x288260; no Load page has one) [C both dumps]. Its kind is G+0xe4 (title 0x1f2924; draw 0x2dc908): 0..2 = the overwrite
//!   question, text 0xc0a ("A Ratchet and Clank 2 save game already exists ... Do you wish to overwrite this?", 0x01 = new line),
//!   3 = "Saving Data ..." 0xc0d; other kinds print "(error)" [C 0x2dc94c..]. Its pad handler 0x2dc870 answers Circle (0x20: Yes,
//!   G+0xe4 = 0, record +0x12c = 1) and Triangle (0x10: No) and goes back to the parent; nothing else.
//! - Slot table 0x139490, 4 records of 0x1c: +0 level (-1 = an empty slot: text 0xe39 "EMPTY", 0x2de814), +4 bolts, +0xc play time
//!   in 1/60 s (hours = t / 216000, "%02d:%02d"), +0x15 / +0x16 / +0x17 day / month / year in BCD, printed "%02x/%02x/%02x"
//!   (0x2debb4; R&C2's fixed console clock made every save "95/00/00": real_clock.rs).
//! - Texts: 0xbae Load Game, 0xbaf Save Game, 0xe39 EMPTY, 0xe55 Continue without saving, 0xb44 "Planet", 0xb45 + level the planet
//!   (modern_screen::site_id), 0xb2b + level the site of the save (levels 0..=24: 0xb2c Megacorp Outlet = Oozla, 0xb36 Megacorp
//!   Games = Joba).
use crate::modern_open;
use crate::modern_screen::{self as scr, Texts, DOWN, UP};
use rac_engine::input::binds::{BIT, CIRCLE, CROSS, TRIANGLE};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

pub const PAGE_ID: u32 = 31;
/// The pause's Save page (its Load page is `PAGE_ID`), the page ids of the title root and the pause Options list (their parents).
pub const PAUSE_SAVE_ID: u32 = 30;
pub const TITLE_ROOT_ID: u32 = 2;
pub const OPTIONS_ID: u32 = modern_open::OPTIONS_PAGE;
/// Retail's message box page over a Save page and the word (G + this) holding its kind.
pub const BOX_ID: u32 = 26;
pub const BOX_KIND: u32 = 0xe4;
/// G+0, the menu's state: `UP_STATE` = a page up (G+0x14), `SWITCHING` = the one field of a page change (G+0x14 = 0, G+0x18 = the
/// next page; retail still draws the old page), 0 = closed (at the title the last page stays current and fades out for `FADE`
/// fields) [C bisect\dumps\dbox 600..800 (title, the box opening at 701 and closing at 761, the close at 788); pause Save: 4].
pub const UP_STATE: u32 = 4;
pub const SWITCHING: u32 = 3;
pub const NEXT_PAGE: u32 = 0x18;
/// Fields of the title's fade after it closes (runs\ld2_yes 788..791, black from 792) [C shots].
pub const FADE: u32 = 4;
/// The title's close after a LOAD [C runs\ld5_mousedbg: dumps 886..918, every-field shots, code 0x2ed7e0 / 0x2eba30]: the Load page
/// closes (G+0 4 -> 0, field 891) and its "Loading Data" picture stays on screen while retail loads the save (it draws no frame:
/// 0x1b1598 stays); then the level change begins (`LEVEL_CHANGE` 0 -> 1, stored at 0x2ed800 with the new level in 0x1a7a78; field
/// 898), retail fades the page out over `EXIT_FADE` frames, one per field (shots 899..902 at 4/5..1/5 brightness, black from 903),
/// shows black and then its loading screen (the halfword `LOAD_SCREEN`: -1 from 0x2eba30, 0, 1, 2 .. from field 916, picture 917).
/// The modern page stays over all of it with retail's fade drawn over it (R&C1: the title page lives while the front end does,
/// front_modern.rs `fr.fade`) and lets go at the loading screen. G+0x14 keeps the Load page long after (field 1050).
pub const LEVEL_CHANGE: u32 = 0x1a7304;
pub const LOAD_SCREEN: u32 = 0x152ca2;
pub const EXIT_FADE: u32 = 5;
/// A guard only: a load close that never reaches the loading screen lets go after this many fields (the d1 load took 25).
pub const EXIT_CAP: u32 = 600;
/// The pause's close after a LOAD from its Load page [C runs\ld6_pzlcl (classic) / ld6_pzl (modern), fg1 0357 + the user card, Cross
/// on slot 1 at 440; dumps every field 448..460, 546..556; G 0x1ff8c0]: the card message (G+0x138, text 0xc0e) is on 442..550; retail
/// draws its last frame at 550 (the picture of 550, 551 and 552 is the same, bit for bit), clears the message at 551 with the page
/// still up (G+0 4), and closes the menu at 552 together with the level change (G+0 4 -> 0, `LEVEL_CHANGE` 0 -> 1); black from 553
/// (no fade), its loading screen from 605 (`LOAD_SCREEN` 0, 1 .. from about 570). So the message stays one field after it clears
/// (`linger`) and the page stays up to the field the level change began (`pause_exit`). While it loads, retail draws its page
/// without the world (454, 456) and the world without its page (455, 457): the window draws the page over those gaps
/// (`world_step`). 0x1b1598 stays 0 in the pause (the title's frame counter only). The gap is counted in GS fields, never in display
/// frames: the window redraws one field's picture any number of times (fx2 [C RAC2_FRAMELOG of q1 on b44 f3]: 51 redraws of field
/// 454 and 8 of field 455 without the world, the frozen-screen hold showing the old level's scene; a count of display frames dropped
/// the page at 456 = retail's classic page, and READY with it, so 457 showed the world without a page).
pub const WORLD_GAP: u32 = 6;
/// G+0x138 = 1 while the slot page shows a card message, G+0x13c = its text (0xc0d Saving Data, 0xc0e Loading), drawn in the info
/// panel instead of the level picture (draw 0x2db764: flag off = text 0xc11; set at 0x2dd908 / 0x2de0a8 / 0x2de218) [C dumps dbox 787].
pub const MSG_ON: u32 = 0x138;
pub const MSG_TEXT: u32 = 0x13c;
/// The slot page's level-picture widget, one of the page record's widget words `WIDGETS` (title record 0x260c88 +0x100 =
/// 0x25db30, pause Oozla 0x287428 +0x100 = 0x287780; its code is in boot at the title and in the level overlay in the pause, so it is
/// found by its picture table +0x30 = `PIC_TABLE`) [C dumps dbox 600..788, pz 2620]. +0x44 its state: the draw (0x2db6c8) shows a
/// picture only from 2 (`slti 2`; < 4 buffer +0x48, else +0x4c), below it the info panel's empty well (text 0xc11) = the dark rect
/// in the picture hole; -1 after a page change until the loader (0x2db048) has the picture (dbox 701..786; pz 2620 = 2); +0x58 < 0
/// = not drawn.
pub const PIC_TABLE: u32 = 0x14c9d0;
pub const PIC_STATE: u32 = 0x44;
pub const PIC_HIDE: u32 = 0x58;
const WIDGETS: std::ops::Range<u32> = 0xf4..0x110;
pub const ID_OVERWRITE: u32 = 0xc0a;
pub const ID_SAVING: u32 = 0xc0d;
pub const LOAD_KIND: u32 = 0x1ab0d8;
pub const SAVE_KIND: u32 = 0x1ab108;
pub const SLOTS: u32 = 0x139490;
pub const SLOT_SIZE: u32 = 0x1c;
pub const N_SLOTS: usize = 4;
pub const ID_LOAD: u32 = 0xbae;
pub const ID_SAVE: u32 = 0xbaf;
pub const ID_EMPTY: u32 = 0xe39;
pub const ID_NO_SAVE: u32 = 0xe55;
pub const ID_PLANET: u32 = 0xb44;
pub const ID_SITE0: u32 = 0xb2b;
/// The pause tab the slot pages belong to (Options).
pub const OPTIONS_TAB: usize = 5;
/// How long (ms) a clicked or hovered card shows selected before retail's cursor must have arrived (R&C1 title_slots `AIM_MS`).
pub const AIM_MS: u64 = 1000;
/// The most Up / Down steps one request walks (4 slots: 3 are enough; a dropped step must not walk forever).
const MAX_STEPS: u8 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
	#[default]
	Load,
	Save,
}

/// The page kind of a slot page's record word +0xf0.
pub fn mode_of(kind: u32) -> Option<Mode> {
	match kind {
		LOAD_KIND => Some(Mode::Load),
		SAVE_KIND => Some(Mode::Save),
		_ => None,
	}
}

/// Which slot page a menu record is: at the title id 31 under the root with the Load / Save word (`mode_of`), in the pause id 30
/// (Save) or 31 (Load) under the Options list. `parent_id` = the parent record's page id (None: no parent).
pub fn slot_mode(title: bool, id: u32, parent_id: Option<u32>, kind: u32) -> Option<Mode> {
	match (title, id, parent_id) {
		(true, PAGE_ID, Some(TITLE_ROOT_ID)) => mode_of(kind),
		(false, PAUSE_SAVE_ID, Some(OPTIONS_ID)) => Some(Mode::Save),
		(false, PAGE_ID, Some(OPTIONS_ID)) => Some(Mode::Load),
		_ => None,
	}
}

/// What one field shows (`see`): this page record, the last facts kept (the title's close fade), or no slot page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum See {
	Page(u32),
	Hold,
	Gone,
}

/// The page record a field shows from G's state word, current and next page (pure): the current page while one is up, the next one
/// in the one-field switch (retail's picture still has the old page: the modern page shows the new one a field early instead of a
/// classic frame), Hold after the close with the last page current (`keep` decides how long: the title's fade, the pause's load).
pub fn see(state: u32, cur: u32, next: u32) -> See {
	match state {
		UP_STATE if cur != 0 => See::Page(cur),
		SWITCHING if cur == 0 && next != 0 => See::Page(next),
		0 if cur != 0 => See::Hold,
		_ => See::Gone,
	}
}

/// The pause's close with its Load page current (pure; see `WORLD_GAP`): whether the page stays this field. `msg` = the kept facts
/// show the card message (retail closed after its "Loading" message), `held` = fields held since the close, `change_at` = the
/// `held` count the level change began at. Retail's picture holds the page up to the field the level change began (black after); a
/// close without the message and without the level change (Start) lets go at once, as before; `FADE` fields is a guard.
pub fn pause_exit(msg: bool, held: u32, change_at: Option<u32>) -> bool {
	match change_at {
		Some(at) => held <= at,
		None => msg && held < FADE,
	}
}

/// The card message of a Load page one field after it cleared (pure; see `WORLD_GAP`): retail's last frame still shows it then.
/// `prev` = the message of the previous field's facts of the same page kind, `carried` = it was already kept once. Returns the
/// message to show and whether it is a kept one.
pub fn linger(mode: Mode, ask: Option<Question>, prev: Option<&Question>, carried: bool) -> (Option<Question>, bool) {
	match (ask, prev) {
		(Some(q), _) => (Some(q), false),
		(None, Some(p)) if mode == Mode::Load && !carried && !p.yes_no => (Some(p.clone()), true),
		_ => (None, false),
	}
}

/// One display frame of the window's world record for the pause page (pure; see `WORLD_GAP`): `last` = the GS fields done of the
/// newest frame that drew the world (None = never), `world` = this frame draws it, `done` = the GS fields done of this frame's
/// picture. Returns the new record and whether the page may draw: the world in a picture at most `WORLD_GAP` fields older, however
/// many frames showed none. Without the world the page goes over black (retail's own picture then); never without a world before
/// it (renderer off: the classic page as before).
pub fn world_step(last: Option<u64>, world: bool, done: u64) -> (Option<u64>, bool) {
	let last = if world { Some(done) } else { last };
	(last, last.is_some_and(|at| done.saturating_sub(at) <= u64::from(WORLD_GAP)))
}

/// The title's close after a load (pure; `LEVEL_CHANGE`): the black over the held Load page this field (0 = none, 1 = black) or
/// None = let go. `held` = fields held since the close, `change_at` = the field of that count the level change began (None: retail
/// still loads the save), `load_screen` = the `LOAD_SCREEN` halfword (0xffff until retail's loading screen draws).
pub fn load_exit(held: u32, change_at: Option<u32>, load_screen: u16) -> Option<f32> {
	match change_at {
		_ if held > EXIT_CAP => None,
		None => Some(0.0),
		Some(_) if load_screen != 0xffff => None,
		Some(at) => Some((held.saturating_sub(at) as f32 / EXIT_FADE as f32).min(1.0)),
	}
}

/// Whether retail draws the slot page's level picture this field (pure): not in the one-field switch (retail still draws the old
/// page) and the page's picture widget (its state and hide words; None = the page has none) holds a picture (draw 0x2db6c8: state
/// >= 2, hide >= 0). Otherwise retail shows its empty well, so the modern page shows no picture.
pub fn pic_shown(switching: bool, widget: Option<(u32, u32)>) -> bool {
	!switching && widget.is_some_and(|(state, hide)| state as i32 >= 2 && hide as i32 >= 0)
}

/// The page record's level-picture widget (`PIC_TABLE`): its state and hide words; None = the page has none.
fn pic_widget(ram: &[u8], page: u32) -> Option<(u32, u32)> {
	let w = WIDGETS.step_by(4).map(|o| rd(ram, page + o)).find(|&x| (0x10_0000..0x200_0000).contains(&x) && rd(ram, x + 0x30) == PIC_TABLE)?;
	Some((rd(ram, w + PIC_STATE), rd(ram, w + PIC_HIDE)))
}

/// Retail's message box over a Save page: what it says (the game's text, one line per 0x01) and whether it asks (Circle = Yes,
/// Triangle = No).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Question {
	pub lines: Vec<String>,
	pub yes_no: bool,
}

/// The box kind (G + `BOX_KIND`): its text id and whether it asks. Kinds 0..2 = overwrite?, 3 = saving; others print "(error)"
/// in retail and still take Circle / Triangle (no text: the question without lines).
pub fn box_text(kind: u32) -> (Option<u32>, bool) {
	match kind {
		0..=2 => (Some(ID_OVERWRITE), true),
		3 => (Some(ID_SAVING), false),
		_ => (None, true),
	}
}

/// What the dialog over the cards says (pure): retail's box (its kind, `box_text`) or the card message of the info panel (G+0x13c
/// while G+0x138 is set: no buttons; retail draws it where the level picture goes); None = no dialog.
pub fn ask_of(box_kind: Option<u32>, msg: Option<u32>) -> Option<(Option<u32>, bool)> {
	match (box_kind, msg) {
		(Some(k), _) => Some(box_text(k)),
		(None, Some(id)) => Some((Some(id), false)),
		_ => None,
	}
}

/// A game string's lines: up to the NUL, split at each 0x01 (two in a row = an empty line between paragraphs), as the PC font
/// shows them.
pub fn split_lines(bytes: &[u8]) -> Vec<String> {
	let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
	bytes[..end].split(|&b| b == 1).map(scr::decode).collect()
}

/// The raw bytes of text `id` (`Texts::get` without the decoding, which drops the 0x01 line breaks).
fn text_bytes<'a>(ram: &'a [u8], t: &Texts, id: u32, n: usize) -> Option<&'a [u8]> {
	let a = (0..t.n).map(|k| t.start + 16 * k).find(|&a| rd(ram, a + 4) == id)?;
	let p = (rd(ram, a) & 0x1ff_ffff) as usize;
	ram.get(p..(p + n).min(ram.len()))
}

/// One save slot as retail's table holds it (None from `raw` = an empty slot).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Raw {
	pub level: u32,
	pub bolts: u32,
	pub time: u32,
	/// Day, month, year (BCD).
	pub date: [u8; 3],
}

fn rd(ram: &[u8], a: u32) -> u32 {
	let a = (a & 0x1ff_fffc) as usize;
	ram.get(a..a + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn rd16(ram: &[u8], a: u32) -> u16 {
	let a = (a & 0x1ff_fffe) as usize;
	ram.get(a..a + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
}

/// Slot `i` of retail's table; None = empty (level word -1).
pub fn raw(ram: &[u8], i: usize) -> Option<Raw> {
	let a = SLOTS + SLOT_SIZE * i as u32;
	let level = rd(ram, a);
	if level == u32::MAX {
		return None;
	}
	let d = rd(ram, a + 0x14).to_le_bytes();
	Some(Raw { level, bolts: rd(ram, a + 4), time: rd(ram, a + 0xc), date: [d[1], d[2], d[3]] })
}

/// Play time as retail prints it: hours and minutes of 1/60 s ticks.
pub fn clock(t: u32) -> String {
	format!("{:02}:{:02}", t / 216_000, (t / 3600) % 60)
}

/// The save date as retail prints it: the BCD bytes as hex, day / month / year.
pub fn date(d: [u8; 3]) -> String {
	format!("{:02x}/{:02x}/{:02x}", d[0], d[1], d[2])
}

/// Bolts with thousands commas, as retail prints them in English (see `bolts_text_for`).
pub fn bolts_text(b: u32) -> String {
	bolts_text_for(b, 0)
}

/// [b49 Z3 U7] A slot's bolts as the retail draw function 0x2de710 prints them [C: boot ELF decomp, 0x2de710]: plain "%d" (format
/// 0x1abbc8) of the count clamped to 9,999,999, then one separator glyph after every 3 digits from the right, drawn only when the
/// number has more than 3 digits: "," for English / unknown language words, "." for German (3), Spanish (4) and Italian (5), none for
/// French (2). Below 1,000 there is no separator ("500"), from 1,000,000 on there are two ("1,000,000").
pub fn bolts_text_for(b: u32, lang: u8) -> String {
	let s = b.min(9_999_999).to_string();
	let sep = match lang {
		2 => None,
		3..=5 => Some('.'),
		_ => Some(','),
	};
	let mut out = String::new();
	for (i, c) in s.chars().enumerate() {
		if let (true, Some(sep)) = (i > 0 && (s.len() - i) % 3 == 0, sep) {
			out.push(sep);
		}
		out.push(c);
	}
	out
}

/// A slot as the page shows it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Slot {
	/// "Planet Oozla" and "Megacorp Outlet".
	pub planet: String,
	pub site: String,
	pub bolts: String,
	pub time: String,
	pub date: String,
}

/// What the modern slot page shows this field.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Facts {
	pub mode: Mode,
	/// At the title (else in the pause, Options tab).
	pub title: bool,
	/// Retail's slot cursor.
	pub cur: usize,
	pub caption: String,
	pub slots: Vec<Option<Slot>>,
	pub empty: String,
	/// The title's Save Game page: retail's "Continue without saving" (Circle).
	pub no_save: Option<String>,
	/// The title root's rows (title) or the pause tabs (pause), the game's texts.
	pub rows: Vec<String>,
	/// The pause shell's planet and bolts.
	pub site: String,
	pub bolts: i32,
	/// Retail's message box over the page (page `BOX_ID`), drawn as a modern dialog over the cards.
	pub ask: Option<Question>,
	/// Retail draws the cursor slot's level picture this field (`pic_shown`); else no picture hole: the switch field (retail's
	/// picture still holds the old page, e.g. the box after Yes) or its picture not loaded yet (the empty well).
	pub pic: bool,
	/// Retail's fade to black over the page after a title load (`load_exit`): 0 = none, 1 = black.
	pub fade: f32,
}

/// The pulses that take retail's cursor to a slot (no wrap) and then press one button (Cross, Circle, Triangle or none): two fields
/// pressed, two released, so every press is a new edge (as `modern_screen::Drive`, whose rows wrap).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Walk {
	pub target: Option<usize>,
	pub press: u16,
	phase: u8,
	held: u16,
	steps: u8,
	/// The step just begun is not the last one: its move sound is hushed (B17, R&C1 `quiet_steps`).
	quiet: bool,
}

impl Walk {
	pub fn new(target: Option<usize>, press: u16) -> Walk {
		Walk { target, press, ..Walk::default() }
	}

	/// One field: the buttons to press with retail's cursor on `cur`.
	pub fn step(&mut self, cur: usize) -> u16 {
		if self.phase == 0 {
			self.held = self.next(cur);
			if self.held == 0 {
				return 0;
			}
		}
		let out = if self.phase < 2 { self.held } else { 0 };
		self.phase = (self.phase + 1) % 4;
		if self.phase == 0 && self.held & (UP | DOWN) == 0 {
			*self = Walk::default();
		}
		out
	}

	fn next(&mut self, cur: usize) -> u16 {
		if let Some(t) = self.target {
			if t != cur {
				if self.steps < MAX_STEPS {
					self.steps += 1;
					self.quiet = t.abs_diff(cur) > 1;
					return if t > cur { DOWN } else { UP };
				}
				// [b45] B16: the cursor never arrived (a press was lost): the request is dropped, its button is never pressed.
				*self = Walk::default();
				return 0;
			}
			self.target = None;
		}
		std::mem::take(&mut self.press)
	}

	/// [b45 P4A] B17: true once per step that is not the walk's last (the caller hushes that field's move sound).
	pub fn take_quiet(&mut self) -> bool {
		std::mem::take(&mut self.quiet)
	}

	pub fn busy(&self) -> bool {
		self.target.is_some() || self.press != 0 || self.phase != 0
	}
}

/// The card to draw selected: a clicked or hovered card (`aim` = card, ms) at once until retail's cursor arrives or `AIM_MS` pass,
/// else retail's cursor; and the aim to keep (R&C1 title_slots `shown`: the bar never crawls card by card).
pub fn shown(aim: Option<(usize, u64)>, cur: usize, now: u64) -> (usize, Option<(usize, u64)>) {
	match aim {
		Some((card, at)) if card != cur && now.saturating_sub(at) < AIM_MS => (card, aim),
		_ => (cur, None),
	}
}

/// The button the window asks for a click on card `i` while RETAIL's cursor is on card `cur` (not the card drawn selected, B15):
/// Cross only on the card retail's cursor is on (the second click loads or saves, R&C1), none on another (the walk selects it).
pub fn click_press(i: usize, cur: usize) -> u16 {
	if i == cur { BIT[CROSS] } else { 0 }
}

/// Whether a click on card `i` does anything: not when the walk is still on its way to that very card (`aim`, R&C1 title_slots
/// `click`: a click on the card the steps are walking to does nothing).
pub fn click_acts(i: usize, cur: usize, aim: Option<(usize, u64)>) -> bool {
	i == cur || aim.is_none_or(|(a, _)| a != i)
}

/// Whether a window request still fits the game's field (pure): one made on retail's Yes/No box (`on_box`) acts only while the box
/// asks, one made on the page only while it does not. The window draws a field late: a click on the page as the box opens (or on
/// the box as it closes) must never answer the box or act on the page.
pub fn fits(on_box: bool, ask: Option<&Question>) -> bool {
	on_box == ask.is_some_and(|q| q.yes_no)
}

pub const CROSS_BIT: u16 = BIT[CROSS];
pub const CIRCLE_BIT: u16 = BIT[CIRCLE];
pub const TRIANGLE_BIT: u16 = BIT[TRIANGLE];

#[derive(Default)]
struct Game {
	texts: Option<Texts>,
	retry: u32,
	walk: Walk,
	/// The walk was asked on retail's Yes/No box (`fits`).
	walk_box: bool,
	seq: u32,
	/// G of the last field with a menu page: the switch field has no current page, so `modern_open::snap_now` has none (used once).
	last_g: Option<u32>,
	/// Fields the last facts were kept (`See::Hold` or a switch to a page that is not a slot page).
	held: u32,
	/// This field is `See::Hold` (the menu closed with the page current), and the `held` count the level change began at.
	closed: bool,
	change_at: Option<u32>,
	/// The card message of the last facts was kept one field after it cleared (`linger`).
	carried: bool,
}

static GAME: Mutex<Option<Game>> = Mutex::new(None);
static FACTS: Mutex<Option<Facts>> = Mutex::new(None);
static READY_TITLE: AtomicBool = AtomicBool::new(false);
static READY_GAME: AtomicBool = AtomicBool::new(false);
static REQ: Mutex<Option<(Option<usize>, u16, bool)>> = Mutex::new(None);
static SEQ: AtomicU32 = AtomicU32::new(0);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Window thread, each display frame: whether it can draw the page at the title (UI pipes) and in the pause (UI pipes and world).
pub fn set_ready(title: bool, game: bool) {
	READY_TITLE.store(title, Ordering::Relaxed);
	READY_GAME.store(game, Ordering::Relaxed);
}

pub fn facts() -> Option<Facts> {
	lock(&FACTS).clone()
}

/// Window thread: walk retail's cursor to `slot` (None = stay) and then press `press` (0 = none); `on_box` = asked on retail's
/// Yes/No box (`fits`).
pub fn request(slot: Option<usize>, press: u16, on_box: bool) {
	*lock(&REQ) = Some((slot, press, on_box));
	SEQ.fetch_add(1, Ordering::Relaxed);
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

/// A game text without its leading button glyph ("\x11 Continue without saving").
fn plain(s: String) -> String {
	s.trim_start_matches(|c: char| !c.is_alphanumeric()).to_string()
}

fn slot(ram: &[u8], t: &Texts, r: Raw) -> Slot {
	let name = scr::site_id(r.level).and_then(|id| t.get(ram, id)).unwrap_or_default();
	let planet = match t.get(ram, ID_PLANET) {
		Some(p) if !p.is_empty() && !name.is_empty() => format!("{p} {name}"),
		_ => name,
	};
	let site = (r.level <= 24).then(|| t.get(ram, ID_SITE0 + r.level)).flatten().unwrap_or_default();
	Slot { planet, site, bolts: bolts_text_for(r.bolts, ram.get(scr::LANG_WORD as usize).copied().unwrap_or(0)), time: clock(r.time), date: date(r.date) }
}

/// The facts of this field: Some(Some) = a modern slot page up, Some(None) = keep the last facts (`See::Hold`, or the switch field
/// to a page that is not a slot page: no classic frame in between), None = no slot page.
fn read(ram: &[u8], g: &mut Game) -> Option<Option<Facts>> {
	g.closed = false;
	let gw = match modern_open::snap_now(ram) {
		Some(s) => *g.last_g.insert(s.g),
		None => g.last_g.take()?,
	};
	let title = gw == scr::TITLE_STATE;
	let cur_page = rd(ram, gw + 0x14);
	let at = match see(rd(ram, gw), cur_page, rd(ram, gw + NEXT_PAGE)) {
		See::Page(p) => p,
		See::Hold => {
			g.closed = true;
			return Some(None);
		}
		See::Gone => return None,
	};
	let other = if cur_page == 0 { Some(None) } else { None };
	// Retail's message box is the current page while it is up: the slot page is its parent.
	let (page, box_kind) = if rd(ram, at + 0xe4) == BOX_ID { (rd(ram, at + 0xe0), Some(rd(ram, gw + BOX_KIND))) } else { (at, None) };
	if page == 0 {
		return other;
	}
	let parent = rd(ram, page + 0xe0);
	let parent_id = (parent != 0).then(|| rd(ram, parent + 0xe4));
	let Some(mode) = slot_mode(title, rd(ram, page + 0xe4), parent_id, rd(ram, page + 0xf0)) else { return other };
	if box_kind.is_some() && mode != Mode::Save {
		return other;
	}
	let ready = if title { &READY_TITLE } else { &READY_GAME };
	if !ready.load(Ordering::Relaxed) || !modern_open::modern() {
		return None;
	}
	let cur = rd(ram, rd(ram, page + 0xe8).wrapping_add(0x40)) as usize;
	if cur >= N_SLOTS {
		return None;
	}
	let t = texts(ram, g)?;
	let msg = (rd(ram, gw + MSG_ON) != 0).then(|| rd(ram, gw + MSG_TEXT));
	let ask = ask_of(box_kind, msg).map(|(id, yes_no)| {
		let lines = id.and_then(|id| text_bytes(ram, &t, id, 400)).map(split_lines).unwrap_or_default();
		Question { lines, yes_no }
	});
	let caption = t.get(ram, if mode == Mode::Load { ID_LOAD } else { ID_SAVE }).unwrap_or_default();
	let slots = (0..N_SLOTS).map(|i| raw(ram, i).map(|r| slot(ram, &t, r))).collect();
	let empty = t.get(ram, ID_EMPTY).unwrap_or_default();
	let no_save = (title && mode == Mode::Save).then(|| t.get(ram, ID_NO_SAVE).map(plain)).flatten();
	let ids: &[u32] = if title { &scr::TITLE_IDS } else { &scr::PAUSE_IDS };
	let rows = ids.iter().map(|&id| t.get(ram, id).unwrap_or_default()).collect();
	let site = if title { String::new() } else { scr::site_id(rd(ram, scr::LEVEL)).and_then(|id| t.get(ram, id)).unwrap_or_default() };
	let pic = pic_shown(rd(ram, gw) == SWITCHING, pic_widget(ram, page));
	Some(Some(Facts { mode, title, cur, caption, slots, empty, no_save, rows, site, bolts: rd(ram, scr::BOLTS) as i32, ask, pic, fade: 0.0 }))
}

/// A field without a slot page to read (`read` gave Some(None)): whether the last facts stay drawn. The title's close after a load
/// follows retail's exit (`load_exit`, with its fade written into the facts), the pause's close after a load up to the level change
/// (`pause_exit`), any other pause close not at all; every other hold (the title's close after a save, a switch to a page that is
/// not a slot page) lasts at most `FADE` fields.
fn keep(ram: &[u8], g: &mut Game) -> bool {
	let mut facts = lock(&FACTS);
	let Some(f) = facts.as_mut() else { return false };
	if g.closed && f.mode == Mode::Load && g.change_at.is_none() && rd(ram, LEVEL_CHANGE) != 0 {
		g.change_at = Some(g.held);
	}
	let fade = match (g.closed, f.title, f.mode) {
		(true, true, Mode::Load) => load_exit(g.held, g.change_at, rd16(ram, LOAD_SCREEN)),
		// The pause's close: only after a load (`pause_exit`); otherwise nothing kept, as before.
		(true, false, Mode::Load) => pause_exit(f.ask.as_ref().is_some_and(|q| !q.yes_no), g.held, g.change_at).then_some(0.0),
		(true, false, Mode::Save) => None,
		_ => (g.held < FADE).then_some(0.0),
	};
	fade.inspect(|&x| f.fade = x).is_some()
}

/// Game thread, once per field after `modern_screen::filter`: the buttons the game gets. Without a modern slot page the pad is
/// returned unchanged, bit for bit; with one, only a pending mouse request changes it.
pub fn filter(ram: &[u8], b: u16, s: [u8; 4]) -> (u16, [u8; 4]) {
	let mut gg = lock(&GAME);
	let g = gg.get_or_insert_with(Game::default);
	let f = match if rac_engine::ui::is_open() { None } else { read(ram, g) } {
		Some(Some(mut f)) => {
			(g.held, g.change_at) = (0, None);
			let last = lock(&FACTS).as_ref().filter(|p| (p.mode, p.title) == (f.mode, f.title)).and_then(|p| p.ask.clone());
			(f.ask, g.carried) = linger(f.mode, f.ask.take(), last.as_ref(), g.carried);
			f
		}
		// The last facts stay drawn (`keep`); the pad passes untouched and no click asked meanwhile acts.
		Some(None) if keep(ram, g) => {
			g.held += 1;
			g.walk = Walk::default();
			g.seq = SEQ.load(Ordering::Relaxed);
			return (b, s);
		}
		_ => {
			if lock(&FACTS).take().is_some() || g.walk.busy() {
				g.walk = Walk::default();
			}
			(g.held, g.change_at, g.carried) = (0, None, false);
			g.seq = SEQ.load(Ordering::Relaxed);
			return (b, s);
		}
	};
	let seq = SEQ.load(Ordering::Relaxed);
	if seq != g.seq {
		g.seq = seq;
		match lock(&REQ).take() {
			Some((slot, press, on_box)) if fits(on_box, f.ask.as_ref()) => (g.walk, g.walk_box) = (Walk::new(slot, press), on_box),
			// Made for another state of the box: dropped, and a walk under way stops (no button the player did not give).
			Some(_) => g.walk = Walk::default(),
			None => {}
		}
	}
	if g.walk.busy() && !fits(g.walk_box, f.ask.as_ref()) {
		g.walk = Walk::default(); // the box opened or closed under the walk: it stops there
	}
	let mut out = b;
	if g.walk.busy() {
		out = (b & !modern_open::MENU_BTNS) | g.walk.step(f.cur);
		if g.walk.take_quiet() {
			crate::modern_tabs::hush(2); // [b45 P4A] B17: only the last step of a walk is heard
		}
	}
	*lock(&FACTS) = Some(f);
	(out, s)
}

#[cfg(test)]
#[path = "modern_load_tests.rs"]
mod tests;
