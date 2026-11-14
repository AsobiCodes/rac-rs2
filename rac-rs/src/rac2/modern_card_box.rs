//! [PC] [b57 NG, ledger R10] The title's memory-card message boxes with the modern menu. Retail opens them through its generic dialog
//! (boot FUN_00286180(type, arg): [0x1bacc0] = type; type 1 = card message with its sub-state [0x1a74a0]; draw 0x286338): they are no menu
//! page (G+0x14 stays the title root), so the modern root stayed drawn and frozen: it stripped Circle (`modern_screen::title_pad`) and
//! the box was never shown [C dumps b57\ng\find*, decomp SCES_516.07.c 37875..38281]. Here the box is read from RAM, drawn as a modern
//! dialog over the title root with the game's own text, and its buttons press Circle / Cross / Triangle (the glyph byte of the label
//! text: 0x10 Cross, 0x11 Circle, 0x12 Triangle). A dialog type without a builder = the classic picture (`active`, no modern root).
//! Nothing is written to game RAM; with the classic menu style or refused UI pipes the pad passes bit for bit.
use crate::modern_load::{split_lines, text_bytes, CIRCLE_BIT, CROSS_BIT, TRIANGLE_BIT};
use crate::modern_pages::{self, Look};
use crate::modern_screen::{self as scr, Facts, Kind, Quit, Texts};
use rac_engine::ui::keycaps::{self, Cap};
use rac_engine::ui::kit::page::{Dialog, DialogButton, PageScreen};
use rac_engine::ui::prim::Symbol;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

const TYPE: u32 = 0x1bacc0;
const STATE: u32 = 0x1a74a0;
const DRAWN: u32 = 0x1bacdc;

/// What retail draws for the dialog of this field (pure, `plan`).
#[derive(Clone, Debug, PartialEq)]
pub enum Plan {
	/// No dialog, or one retail draws nothing for yet.
	Off,
	/// A dialog without a modern builder: retail's own picture.
	Classic,
	/// Text ids (joined by a blank line) and button label ids.
	Show { msgs: Vec<u32>, labels: Vec<u32> },
}

/// The words the dialog code reads.
#[derive(Clone, Copy, Debug, Default)]
pub struct Card {
	pub ty: u32,
	pub state: u32,
	pub drawn: u32,
	pub hold: u32,
	pub a4: u32,
	pub c4: u32,
	pub d00: u32,
	pub e7c14: u32,
	pub e1a3c: u32,
	pub s8d38: u32,
	pub s8d3c: u32,
	pub f29a0: u32,
}

fn rd(ram: &[u8], a: u32) -> u32 {
	let a = (a & 0x1ff_fffc) as usize;
	ram.get(a..a + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

impl Card {
	pub fn read(ram: &[u8]) -> Card {
		Card { ty: rd(ram, TYPE), state: rd(ram, STATE), drawn: rd(ram, DRAWN), hold: rd(ram, 0x1bacfc), a4: rd(ram, 0x1a74a4), c4: rd(ram, 0x1bacc4), d00: rd(ram, 0x1bad00), e7c14: rd(ram, 0x1a7c14), e1a3c: rd(ram, 0x1b1a3c), s8d38: rd(ram, 0x1a8d38), s8d3c: rd(ram, 0x1a8d3c), f29a0: rd(ram, 0x1f29a0) }
	}
}

const CONTINUE: u32 = 0x1137;
const YES_X: u32 = 0x113b;
const YES_O: u32 = 0x113c;
const NO_T: u32 = 0x1138;
const BACK_T: u32 = 0xbf9;

fn show(msgs: &[u32], labels: &[u32]) -> Plan {
	Plan::Show { msgs: msgs.to_vec(), labels: labels.to_vec() }
}

/// Text and buttons of the card message (type 1) by its sub-state: retail's switch at 0x286a00..0x286f1c [C decomp].
fn card_plan(c: &Card) -> Plan {
	if c.e7c14 != 0 || (c.a4 & 0x600 != 0 && c.e1a3c != 0) {
		return show(&[0xc15], &[CONTINUE]);
	}
	let go = if c.c4 != 0 { vec![] } else { vec![CONTINUE] };
	match c.state {
		2 => show(&[0xbf6], &go),
		3 | 4 if c.s8d38 != 0 && c.f29a0 == 0 => show(&[0xbfd], &[YES_X, NO_T]),
		3 | 4 if c.s8d3c != 0 => show(&[0xbfd], &[YES_X]),
		3 | 4 => show(&[if c.f29a0 == 0 { 0xbff } else { 0xbf7 }], &[BACK_T]),
		5 => show(&[0xbf7], &[BACK_T]),
		6 => show(&[0xc05], &[YES_O, NO_T]),
		7 | 8 => show(&[0xc0f], &[]),
		10 | 11 => show(&[0xc11], &[]),
		12 if c.f29a0 != 0 => show(&[0xbf7], &[BACK_T]),
		12 | 13 => show(&[0xc01], &[YES_O, NO_T]),
		14 | 15 => show(&[0xc10], &[]),
		0x11 => show(&[0xc12], &[CONTINUE]),
		0x12 => show(&[0xc14], &[CONTINUE]),
		0x13 if c.f29a0 != 0 => show(&[0xbf7], &[BACK_T]),
		0x13 if c.s8d38 != 0 => show(&[0xbfa, 0xbfc], &[YES_X, NO_T]),
		0x13 => show(&[0xbfa, 0xbfc], &[if c.s8d3c != 0 { YES_O } else { YES_X }]),
		0x14 => show(&[0xc13], &[CONTINUE]),
		0x15 => show(&[0xc14], &[CONTINUE]),
		0x17 => show(&[0xc07], &[YES_X, NO_T]),
		0x18 => show(&[0xc03, 0xbfc], &[YES_X, NO_T]),
		_ => Plan::Off,
	}
}

/// The dialog of this field (pure). Types 4 / 5: autosave off / save failed; nothing is drawn before the dialog's counter is 1.
pub fn plan(c: &Card) -> Plan {
	if c.ty == 0 || c.drawn < 1 || c.hold != 0 {
		return Plan::Off;
	}
	match c.ty {
		1 => card_plan(c),
		4 => show(&[0xbf6], &if c.c4 != 0 { vec![] } else { vec![CONTINUE] }),
		5 => show(&[if c.d00 == 0 { 0xc14 } else { 0xc15 }], &if c.c4 != 0 { vec![] } else { vec![CONTINUE] }),
		_ => Plan::Classic,
	}
}

/// The pad bit of a label's glyph byte.
pub fn glyph_bit(g: u8) -> u16 {
	match g {
		0x10 => CROSS_BIT,
		0x11 => CIRCLE_BIT,
		0x12 => TRIANGLE_BIT,
		_ => 0,
	}
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Btn {
	pub bit: u16,
	pub text: String,
}

/// A button from its label's bytes (glyph byte, then the word).
pub fn button(bytes: &[u8]) -> Option<Btn> {
	let bit = glyph_bit(*bytes.first()?);
	// The other languages put a colon after the glyph byte (German: glyph, colon, Ja): it belongs to the glyph, not to the word.
	(bit != 0).then(|| Btn { bit, text: scr::decode(&bytes[1..]).trim_start_matches(':').trim().to_string() })
}

/// The box the window draws, with the title root behind it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CardBox {
	/// The box's identity (opening count, sub-state, buttons): a request made for another box is dropped.
	pub key: u32,
	pub lines: Vec<String>,
	pub buttons: Vec<Btn>,
	pub rows: Vec<String>,
	pub cur: usize,
}

/// The button Enter presses (the first) and the one Esc / a right click presses (Triangle, else a sole button).
pub fn confirm_bit(b: &CardBox) -> Option<u16> {
	b.buttons.first().map(|x| x.bit)
}
pub fn cancel_bit(b: &CardBox) -> Option<u16> {
	b.buttons.iter().find(|x| x.bit == TRIANGLE_BIT).or(if b.buttons.len() == 1 { b.buttons.first() } else { None }).map(|x| x.bit)
}

/// The modern page: the title root, inert, with the dialog.
pub fn screen(b: &CardBox, look: Look) -> PageScreen {
	let f = Facts { kind: Kind::Title, cur: b.cur, rows: b.rows.clone(), site: String::new(), bolts: 0 };
	let mut s = modern_pages::title(&f, Quit::default(), look);
	s.menu.iter_mut().for_each(|m| m.live = false);
	let symbol = |bit| match bit {
		CROSS_BIT => Symbol::Cross,
		CIRCLE_BIT => Symbol::Circle,
		_ => Symbol::Triangle,
	};
	let buttons = b.buttons.iter().enumerate().map(|(i, x)| DialogButton { symbol: symbol(x.bit), key: keycaps::name(if i == 0 { Cap::Enter } else { Cap::Esc }).to_string(), text: x.text.clone() }).collect();
	s.dialog = Some(Dialog { lines: b.lines.clone(), buttons });
	s
}

#[derive(Default)]
struct Game {
	texts: Option<Texts>,
	retry: u32,
	opened: u32,
	up: bool,
	seq: u32,
	/// A pulse under way: (button, field of 4, box key).
	pulse: Option<(u16, u8, u32)>,
}

static GAME: Mutex<Option<Game>> = Mutex::new(None);
static BOX: Mutex<Option<CardBox>> = Mutex::new(None);
static ACTIVE: AtomicBool = AtomicBool::new(false);
static READY: AtomicBool = AtomicBool::new(false);
static REQ: Mutex<Option<(u16, u32)>> = Mutex::new(None);
static SEQ: AtomicU32 = AtomicU32::new(0);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Window thread, each display frame: the UI pipes can draw.
pub fn set_ready(r: bool) {
	READY.store(r, Ordering::Relaxed);
}

/// The box of the newest field (window reads).
pub fn facts() -> Option<CardBox> {
	lock(&BOX).clone()
}

/// A title dialog is up (modern box or classic fallback): the modern roots and slot pages stand aside this field.
pub fn active() -> bool {
	ACTIVE.load(Ordering::Relaxed)
}

/// Window thread: press `bit` on the box `key` (dropped when the box is another one by then).
pub fn request(bit: u16, key: u32) {
	*lock(&REQ) = Some((bit, key));
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

fn build(ram: &[u8], t: &Texts, msgs: &[u32], labels: &[u32]) -> Option<(Vec<String>, Vec<Btn>)> {
	let mut lines = Vec::new();
	for (i, &id) in msgs.iter().enumerate() {
		if i > 0 {
			lines.push(String::new());
		}
		lines.extend(split_lines(text_bytes(ram, t, id, 700)?));
	}
	let buttons = labels.iter().filter_map(|&id| text_bytes(ram, t, id, 64).and_then(button)).collect();
	Some((lines, buttons))
}

/// Game thread, once per field BEFORE the other modern filters: reads the dialog (sets `active` and the box).
pub fn pre(ram: &[u8]) {
	let mut gg = lock(&GAME);
	let g = gg.get_or_insert_with(Game::default);
	let title = rd(ram, scr::TITLE_STATE) == 4 && rd(ram, scr::MANAGER) == 0;
	let p = if READY.load(Ordering::Relaxed) && crate::modern_open::modern() && !rac_engine::ui::is_open() && title { plan(&Card::read(ram)) } else { Plan::Off };
	let shown = match &p {
		Plan::Show { msgs, labels } => texts(ram, g).and_then(|t| build(ram, &t, msgs, labels).map(|(l, b)| (l, b, t))),
		_ => None,
	};
	let Some((lines, buttons, t)) = shown else {
		ACTIVE.store(matches!(p, Plan::Classic | Plan::Show { .. }), Ordering::Relaxed);
		*lock(&BOX) = None;
		g.up = false;
		g.pulse = None;
		return;
	};
	if !g.up {
		g.opened += 1;
	}
	g.up = true;
	let bits = buttons.iter().fold(0u32, |a, b| a | u32::from(b.bit));
	let key = (g.opened << 20) | (rd(ram, STATE) & 0xff) << 12 | bits;
	let rows = scr::TITLE_IDS.iter().map(|&id| t.get(ram, id).unwrap_or_default()).collect();
	let cur = (rd(ram, scr::TITLE_CURSOR) as usize).min(scr::TITLE_IDS.len() - 1);
	ACTIVE.store(true, Ordering::Relaxed);
	*lock(&BOX) = Some(CardBox { key, lines, buttons, rows, cur });
}

/// Game thread, once per field AFTER the other filters: a pending window request becomes a 2-field press (2 released), so it is a
/// new edge for retail. Without a box the pad is returned unchanged.
pub fn post(b: u16, s: [u8; 4]) -> (u16, [u8; 4]) {
	let mut gg = lock(&GAME);
	let g = gg.get_or_insert_with(Game::default);
	let Some(bx) = lock(&BOX).clone() else {
		g.seq = SEQ.load(Ordering::Relaxed);
		return (b, s);
	};
	let seq = SEQ.load(Ordering::Relaxed);
	if seq != g.seq {
		g.seq = seq;
		if let Some((bit, key)) = lock(&REQ).take() {
			g.pulse = (key == bx.key && bx.buttons.iter().any(|x| x.bit == bit)).then_some((bit, 0, key));
		}
	}
	let Some((bit, ph, key)) = g.pulse else { return (b, s) };
	if key != bx.key {
		g.pulse = None;
		return (b, s);
	}
	g.pulse = (ph < 3).then_some((bit, ph + 1, key));
	((b & !crate::modern_open::MENU_BTNS) | if ph < 2 { bit } else { 0 }, s)
}

#[cfg(test)]
#[path = "modern_card_box_tests.rs"]
mod tests;
