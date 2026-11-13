//! [PC] R&C2's modern Ship Shack page, a pure builder (lane shack): `Shack` (the shop as one field sees it) -> a draw list in the
//! 1280x720 design space plus what each part does when clicked. Same look as the modern vendor (`modern_vendor_page.rs`): the stock on
//! the left, the selected part's card on the right, the ship preview free in the middle (the world is drawn behind), the buy prompt as
//! a box over a dimmed world, the hint bar at the bottom. Also the pure strip rule (`strip`): which parts and wings the game offers.
//! Facts [C] rac2/notes_modern_screens.md 8.3. Part names come from the game's text bank (modern_shack.rs); the English ones are a backstop.
use crate::modern_vendor_page::{commas, rrect, text};
use rac_engine::ui::input::{Device, GlyphSet};
use rac_engine::ui::kit::text::{run_width, upper};
use rac_engine::ui::kit::TextMeasure;
use rac_engine::ui::prim::{Align, FontRole, Prim, Rect, Rgba, Symbol};
use rac_engine::ui::theme::*;
use rac_engine::ui::T5;

pub const LIST: Rect = Rect::new(48.0, 116.0, 400.0, 540.0);
pub const DETAIL: Rect = Rect::new(852.0, 116.0, 380.0, 540.0);
/// The ship preview's window in the middle (design space): retail's ship picture is cut out of the overlay into it (host/screens_ship.rs).
pub fn hole() -> Rect {
	let gap = Rect::new(LIST.right() + 16.0, 150.0, DETAIL.x - 16.0 - (LIST.right() + 16.0), 420.0);
	let h = gap.w * (SHIP.3 * 9.0) / (SHIP.2 * 16.0); // the overlay picture is 16 : 9
	Rect::new(gap.x, gap.y + (gap.h - h) * 0.5, gap.w, h)
}

/// The ship's region in retail's overlay picture, as fractions (x0, y0, w, h) of it: the model's grid window without its text [C picture
/// runs/cls/w00059.png]. Aspect w : h = 2.26.
pub const SHIP: (f32, f32, f32, f32) = (0.186, 0.163, 0.464, 0.365);

const ROW_H: f32 = 60.0;
const ROW_GAP: f32 = 4.0;
const VISIBLE: usize = 7;
const RED: Rgba = SYMBOL_CIRCLE;

pub const HEADER: T5 = ["Ship Shack", "Boutique du vaisseau", "Schiffs-Shop", "Tienda de la nave", "Negozio navicella"];
pub const TITLE: T5 = ["Ship upgrades", "Am\u{e9}liorations", "Schiffs-Upgrades", "Mejoras de la nave", "Potenziamenti"];
pub const NEW: T5 = ["New", "Nouveau", "Neu", "Nuevo", "Nuovo"];
pub const SWAP: T5 = ["Swap", "\u{c9}change", "Tausch", "Cambio", "Cambio"];
pub const NEED: T5 = ["Need Raritanium", "Raritanium requis", "Zu wenig Raritanium", "Falta raritanio", "Serve raritanio"];
pub const RARITANIUM: T5 = ["Raritanium", "Raritanium", "Raritanium", "Raritanio", "Raritanio"];
pub const PRICE: T5 = ["Price", "Prix", "Preis", "Precio", "Prezzo"];
pub const BUY: T5 = ["Buy", "Acheter", "Kaufen", "Comprar", "Acquista"];
pub const CANCEL: T5 = ["Cancel", "Annuler", "Abbrechen", "Cancelar", "Annulla"];
pub const LEAVE: T5 = ["Leave", "Quitter", "Verlassen", "Salir", "Esci"];
pub const NOT_ENOUGH: T5 = ["Not enough Raritanium", "Pas assez de Raritanium", "Nicht genug Raritanium", "Falta raritanio", "Raritanio insufficiente"];
pub const ASK: T5 = ["Buy {}?", "Acheter {} ?", "{} kaufen?", "\u{bf}Comprar {}?", "Comprare {}?"];
pub const TOTAL: T5 = ["Total", "Total", "Gesamt", "Total", "Totale"];
pub const PART: T5 = ["Ship part", "Pi\u{e8}ce du vaisseau", "Schiffsteil", "Pieza de la nave", "Parte della navicella"];
pub const WING: T5 = ["Wings (swaps your current pair)", "Ailes (\u{e9}change la paire actuelle)", "Fl\u{fc}gel (tauscht das aktuelle Paar)", "Alas (cambia las actuales)", "Ali (scambia la coppia attuale)"];

/// Every table, for the font-coverage test.
pub fn all() -> [&'static T5; 15] {
	[&HEADER, &TITLE, &NEW, &SWAP, &NEED, &RARITANIUM, &PRICE, &BUY, &CANCEL, &LEAVE, &NOT_ENOUGH, &ASK, &TOTAL, &PART, &WING]
}

pub fn word(t: &T5, lang: usize) -> &'static str {
	t[lang.min(4)]
}

/// English backstop names of the 12 part-table rows (table order) and of the three wing pairs.
pub const PART_NAMES: [&str; 12] = [
	"Triple Boost", "Fusion Laser Cannons", "Electro Mine Launcher", "Mega Mine Launcher", "Standard Missiles", "Fast-Lock Missiles",
	"Torpedo Launcher", "Multi-Torpedo Launcher", "Advanced Shielding", "Ultra-Charge Shielding", "Nuclear Detonation Device", "Hyperspace Warp System",
];
pub const WING_NAMES: [&str; 3] = ["Standard Wings", "Hi-Lift Wings", "Heavy Ordnance Wings"];
/// Price of a wing swap [C costs[13..15]].
pub const WING_COST: u32 = 5;

/// What an entry of the strip is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
	Part(usize),
	Wing(usize),
}

/// The strip the game offers, in order [C notes 8.3]. `rows` = (group mask, own bit) of the part table in table order; `mask` = the owned
/// mask. Rows with the same group mask are one chain; a chain offers its first row whose own bit is non-zero and not owned. After the
/// parts come the two wing pairs that are not the equipped one (equipped = bits 2..3 of the mask).
pub fn strip(rows: &[(u32, u32)], mask: u32) -> Vec<Pick> {
	let mut out = Vec::new();
	let mut r = 0;
	while r < rows.len() {
		let g = rows[r].0;
		let end = (r..rows.len()).find(|&k| rows[k].0 != g).unwrap_or(rows.len());
		if let Some(k) = (r..end).find(|&k| rows[k].1 != 0 && mask & rows[k].1 == 0) {
			out.push(Pick::Part(k));
		}
		r = end;
	}
	let eq = ((mask >> 2) & 3) as usize;
	out.extend((0..3).filter(|&w| w != eq).map(Pick::Wing));
	out
}

/// One entry of the strip as the page shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
	pub pick: Pick,
	pub name: String,
	pub cost: u32,
	/// The part's description paragraph from the text bank (empty: none known).
	pub desc: String,
}

impl Entry {
	pub fn can_buy(&self, rar: u32) -> bool {
		rar >= self.cost
	}

	pub fn wing(&self) -> bool {
		matches!(self.pick, Pick::Wing(_))
	}
}

/// The shop as one field sees it.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Shack {
	pub entries: Vec<Entry>,
	pub cur: usize,
	/// 0 browse, non-zero: the "Purchase?" prompt.
	pub stage: u32,
	pub rar: u32,
	/// The row the mouse's walk is on its way to (the page keeps that row shown until retail's cursor is there). [b45 P3]
	pub walk: Option<usize>,
}

impl Shack {
	pub fn prompt(&self) -> bool {
		self.stage != 0
	}
}

pub struct Facts<'a> {
	pub shop: &'a Shack,
	/// The row the mouse chose a moment ago (shown at once, before the game's cursor follows).
	pub focus: Option<usize>,
	/// The first row the list showed last frame.
	pub scroll: usize,
	pub lang: usize,
	pub device: Device,
	pub glyphs: GlyphSet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
	None,
	Row(usize),
	Buy,
	Confirm,
	Cancel,
	Hint(Symbol),
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Built {
	pub prims: Vec<Prim>,
	pub rows: Vec<(usize, Rect)>,
	pub buy: Option<Rect>,
	pub confirm: Option<Rect>,
	pub cancel: Option<Rect>,
	pub hints: Vec<(Symbol, Rect)>,
	pub prompt: bool,
	pub sel: usize,
	pub first: usize,
	pub count: usize,
}

impl Built {
	pub fn hit(&self, x: f32, y: f32) -> Hit {
		let on = |r: &Option<Rect>| r.is_some_and(|r| r.contains(x, y));
		let hint = || self.hints.iter().find(|h| h.1.contains(x, y)).map_or(Hit::None, |h| Hit::Hint(h.0));
		if self.prompt {
			return [(self.confirm, Hit::Confirm), (self.cancel, Hit::Cancel)].into_iter().find(|(r, _)| on(r)).map_or_else(hint, |(_, h)| h);
		}
		if on(&self.buy) {
			return Hit::Buy;
		}
		self.rows.iter().find(|r| r.1.contains(x, y)).map_or_else(hint, |r| Hit::Row(r.0))
	}
}

fn key_name(sym: Symbol) -> String {
	crate::modern_hint_keys::cap(sym) // [b46 W5] B7: follows a rebound key
}

fn lead_w(f: &Facts, sym: Symbol, m: &dyn TextMeasure) -> f32 {
	if f.device == Device::Keyboard { (run_width(m, FontRole::SemiBold, 13.0, &key_name(sym), 0.0) + 14.0).max(24.0) } else { 24.0 }
}

fn lead(out: &mut Vec<Prim>, f: &Facts, sym: Symbol, x: f32, cy: f32, m: &dyn TextMeasure) -> f32 {
	let w = lead_w(f, sym, m);
	let rect = Rect::new(x, cy - 12.0, w, 24.0);
	if f.device == Device::Keyboard {
		rrect(out, rect, 6.0, CAP_BG, Some((1.0, CAP_BORDER)));
		text(out, rect.cx(), rect.cy(), FontRole::SemiBold, 13.0, TEXT, Align::Center, 0.0, 0.0, &key_name(sym));
	} else {
		out.push(Prim::Glyph { rect, kind: f.glyphs.glyph(sym), colour: symbol_colour(sym) });
	}
	w
}

fn header(out: &mut Vec<Prim>, f: &Facts, m: &dyn TextMeasure) {
	text(out, 48.0, 44.0, FontRole::SemiBold, 13.0, TEXT_MUTED, Align::Left, 400.0, 0.18, &upper(word(&HEADER, f.lang)));
	text(out, 48.0, 80.0, FontRole::Condensed, 34.0, TEXT, Align::Left, 700.0, 0.04, &upper(word(&TITLE, f.lang)));
	let n = commas(f.shop.rar);
	let w = 16.0 + 20.0 + 8.0 + run_width(m, FontRole::Condensed, 24.0, &n, 0.0) + 16.0;
	let r = Rect::new(1232.0 - w, 58.0, w, 44.0);
	rrect(out, r, 10.0, PANEL, Some((1.0, HAIRLINE)));
	rrect(out, Rect::new(r.x + 16.0, r.cy() - 10.0, 20.0, 20.0), 4.0, ACCENT.tint(0.3), Some((2.0, TEXT_BODY)));
	text(out, r.x + 44.0, r.cy(), FontRole::Condensed, 24.0, TEXT, Align::Left, 0.0, 0.0, &n);
}

fn chip_of(f: &Facts, e: &Entry) -> (&'static str, Rgba, Rgba) {
	if !e.can_buy(f.shop.rar) {
		(word(&NEED, f.lang), RED, RED.tint(0.12))
	} else if e.wing() {
		(word(&SWAP, f.lang), SYMBOL_TRIANGLE, SYMBOL_TRIANGLE.tint(0.14))
	} else {
		(word(&NEW, f.lang), ACCENT, ACCENT.tint(0.16))
	}
}

/// The list: scrolled to keep the selection in view (the vendor's rule).
fn list(b: &mut Built, f: &Facts, m: &dyn TextMeasure) {
	rrect(&mut b.prims, LIST, 16.0, PANEL, Some((1.0, HAIRLINE)));
	let n = f.shop.entries.len();
	b.first = crate::modern_vendor_page::first_row(f.scroll, b.sel, n);
	let out = &mut b.prims;
	for (k, i) in (b.first..n.min(b.first + VISIBLE)).enumerate() {
		let e = &f.shop.entries[i];
		let rect = Rect::new(LIST.x + 12.0, LIST.y + 12.0 + k as f32 * (ROW_H + ROW_GAP), LIST.w - 24.0, ROW_H);
		let on = i == b.sel;
		if on {
			rrect(out, rect, 10.0, ACCENT.tint(0.13), Some((1.5, ACCENT.tint(0.6))));
		}
		let ink = if on { ACCENT } else { TEXT_BODY };
		let icon = Rect::new(rect.x + 14.0, rect.cy() - 20.0, 40.0, 40.0);
		rrect(out, icon, 20.0, Rgba::rgba(255, 255, 255, 0.07), None);
		rrect(out, icon.inset(9.0), 11.0, Rgba::rgba(0, 0, 0, 0.0), Some((2.2, ink)));
		rrect(out, icon.inset(15.5), 4.5, ink, None);
		let (word, colour, fill) = chip_of(f, e);
		let w = run_width(m, FontRole::SemiBold, 13.0, word, 0.0) + 20.0;
		let chip = Rect::new(rect.right() - 14.0 - w, rect.y + 31.0, w, 22.0);
		rrect(out, chip, 11.0, fill, None);
		text(out, chip.cx(), chip.cy(), FontRole::SemiBold, 13.0, colour, Align::Center, 0.0, 0.0, word);
		let max_w = rect.right() - 12.0 - (rect.x + 68.0);
		text(out, rect.x + 68.0, rect.y + 20.0, FontRole::SemiBold, 18.0, TEXT, Align::Left, max_w, 0.0, &e.name);
		text(out, rect.x + 68.0, rect.y + 41.0, FontRole::Regular, 14.0, TEXT_MUTED, Align::Left, 100.0, 0.0, &format!("{} {}", commas(e.cost), word_rar(f)));
		b.rows.push((i, rect));
	}
}

fn word_rar(f: &Facts) -> &'static str {
	word(&RARITANIUM, f.lang)
}

fn detail(b: &mut Built, f: &Facts, m: &dyn TextMeasure) {
	rrect(&mut b.prims, DETAIL, 16.0, PANEL, Some((1.0, HAIRLINE)));
	let Some(e) = f.shop.entries.get(b.sel) else { return };
	let (x, w) = (DETAIL.x + 22.0, DETAIL.w - 44.0);
	text(&mut b.prims, x, DETAIL.y + 38.0, FontRole::Condensed, 30.0, WHITE, Align::Left, w, 0.0, &e.name);
	text(&mut b.prims, x, DETAIL.y + 74.0, FontRole::Regular, 14.0, TEXT_MUTED, Align::Left, w, 0.0, word(if e.wing() { &WING } else { &PART }, f.lang));
	let btn = Rect::new(x, DETAIL.bottom() - 22.0 - 46.0, w, 46.0);
	let lines = wrap(&e.desc, w, |s| run_width(m, FontRole::Regular, 15.0, s, 0.0));
	let room = (((btn.y - 30.0 - 22.0) - (DETAIL.y + 112.0)) / 22.0).max(0.0) as usize;
	for (i, l) in lines.iter().take(room).enumerate() {
		text(&mut b.prims, x, DETAIL.y + 112.0 + 22.0 * i as f32, FontRole::Regular, 15.0, TEXT_BODY, Align::Left, w, 0.0, l);
	}
	let can = e.can_buy(f.shop.rar);
	let y = btn.y - 30.0;
	text(&mut b.prims, btn.x, y, FontRole::Regular, 14.0, TEXT_MUTED, Align::Left, 190.0, 0.0, word(&PRICE, f.lang));
	text(&mut b.prims, btn.right(), y, FontRole::Condensed, 28.0, if can { WHITE } else { RED }, Align::Right, 190.0, 0.0, &format!("{} {}", commas(e.cost), word_rar(f)));
	let out = &mut b.prims;
	if can {
		rrect(out, btn, 10.0, ACCENT, None);
		let say = word(&BUY, f.lang);
		let tw = run_width(m, FontRole::SemiBold, 17.0, say, 0.0);
		let lw = lead_w(f, Symbol::Cross, m);
		let x0 = btn.cx() - (lw + 10.0 + tw) * 0.5;
		lead(out, f, Symbol::Cross, x0, btn.cy(), m);
		text(out, x0 + lw + 10.0, btn.cy(), FontRole::SemiBold, 17.0, INK_ON_ACCENT, Align::Left, btn.w - 24.0 - 10.0 - lw, 0.0, say);
		b.buy = Some(btn);
	} else {
		rrect(out, btn, 10.0, BUTTON_BG, None);
		text(out, btn.cx(), btn.cy(), FontRole::SemiBold, 17.0, TEXT_DISABLED, Align::Center, btn.w - 24.0, 0.0, word(&NOT_ENOUGH, f.lang));
	}
}

/// Greedy word wrap of `s` to `room` px (`width` measures a run).
pub fn wrap(s: &str, room: f32, width: impl Fn(&str) -> f32) -> Vec<String> {
	let mut out: Vec<String> = Vec::new();
	let mut cur = String::new();
	for w in s.split_whitespace() {
		let t = if cur.is_empty() { w.to_string() } else { format!("{cur} {w}") };
		if !cur.is_empty() && width(&t) > room {
			out.push(std::mem::take(&mut cur));
			cur = w.to_string();
		} else {
			cur = t;
		}
	}
	if !cur.is_empty() {
		out.push(cur);
	}
	out
}

fn hints(b: &mut Built, f: &Facts, m: &dyn TextMeasure) {
	let items = [(Symbol::Cross, word(&BUY, f.lang)), (Symbol::Triangle, word(if b.prompt { &CANCEL } else { &LEAVE }, f.lang))];
	let widths: Vec<f32> = items.iter().map(|(sym, s)| lead_w(f, *sym, m) + 8.0 + run_width(m, FontRole::Medium, 15.0, s, 0.0)).collect();
	let mut x = 1232.0 - widths.iter().sum::<f32>() - 26.0 * (items.len() - 1) as f32;
	let cy = 688.0;
	for ((sym, s), w) in items.iter().zip(&widths) {
		b.hints.push((*sym, Rect::new(x - 6.0, cy - 16.0, w + 12.0, 32.0)));
		let lw = lead(&mut b.prims, f, *sym, x, cy, m);
		text(&mut b.prims, x + lw + 8.0, cy, FontRole::Medium, 15.0, TEXT_BODY, Align::Left, 0.0, 0.0, s);
		x += w + 26.0;
	}
}

fn prompt(b: &mut Built, f: &Facts) {
	let Some(e) = f.shop.entries.get(b.sel) else { return };
	let out = &mut b.prims;
	out.push(Prim::Backdrop { blur_px: BLUR_PEEK, scrim: Rgba::rgba(4, 6, 12, 0.62) });
	let d = Rect::new(400.0, 210.0, 480.0, 206.0);
	rrect(out, d, 16.0, Rgba::rgba(14, 20, 34, 0.97), Some((1.0, Rgba::rgba(255, 255, 255, 0.10))));
	let (x, w) = (d.x + 26.0, d.w - 52.0);
	text(out, x, d.y + 44.0, FontRole::Condensed, 30.0, TEXT, Align::Left, w, 0.0, &word(&ASK, f.lang).replace("{}", &e.name));
	let y = d.y + 78.0;
	rrect(out, Rect::new(x, y, w, 1.0), 0.0, HAIRLINE, None);
	text(out, x, y + 24.0, FontRole::Regular, 15.0, TEXT_MUTED, Align::Left, 120.0, 0.0, word(&TOTAL, f.lang));
	text(out, x + w, y + 24.0, FontRole::Condensed, 28.0, WHITE, Align::Right, w - 130.0, 0.0, &format!("{} {}", commas(e.cost), word_rar(f)));
	let by = d.bottom() - 72.0;
	let half = (w - 12.0) * 0.5;
	let (ok, no) = (Rect::new(x, by, half, 46.0), Rect::new(x + half + 12.0, by, half, 46.0));
	rrect(out, ok, 10.0, ACCENT, None);
	rrect(out, no, 10.0, CHIP_BG, None);
	text(out, ok.cx(), ok.cy(), FontRole::SemiBold, 17.0, INK_ON_ACCENT, Align::Center, half - 16.0, 0.0, word(&BUY, f.lang));
	text(out, no.cx(), no.cy(), FontRole::SemiBold, 17.0, TEXT, Align::Center, half - 16.0, 0.0, word(&CANCEL, f.lang));
	(b.confirm, b.cancel) = (Some(ok), Some(no));
}

/// The page of `f`; None while the strip is empty.
pub fn build(f: &Facts, m: &dyn TextMeasure) -> Option<Built> {
	let s = f.shop;
	if s.entries.is_empty() {
		return None;
	}
	let sel = if s.prompt() { s.cur } else { f.focus.unwrap_or(s.cur) }.min(s.entries.len() - 1);
	let mut b = Built { prompt: s.prompt(), sel, count: s.entries.len(), ..Built::default() };
	header(&mut b.prims, f, m);
	let h = hole();
	rrect(&mut b.prims, h, 6.0, Rgba::rgba(0, 0, 0, 0.0), Some((1.0, HAIRLINE)));
	list(&mut b, f, m);
	detail(&mut b, f, m);
	hints(&mut b, f, m);
	if b.prompt {
		prompt(&mut b, f);
	}
	Some(b)
}

#[cfg(test)]
mod tests {
	use super::*;

	/// (group, own bit) of the PAL part table [C notes 8.3].
	const ROWS: [(u32, u32); 12] = [
		(0x30, 0x10), (0x40, 0x40), (0x180, 0x80), (0x180, 0x100), (0x200, 0), (0x200, 0x200),
		(0xc00, 0x400), (0xc00, 0x800), (0xc000, 0x4000), (0xc000, 0x8000), (0x1000, 0x1000), (0x2000, 0x2000),
	];

	#[test]
	fn strip_matches_the_save_seen_in_retail() {
		// Owned 0x4240: Triple Boost, Electro Mine, Torpedo, Ultra-Charge, Nuclear, Warp, then the two other wings.
		let s = strip(&ROWS, 0x4240);
		let want = [Pick::Part(0), Pick::Part(2), Pick::Part(6), Pick::Part(9), Pick::Part(10), Pick::Part(11), Pick::Wing(1), Pick::Wing(2)];
		assert_eq!(s, want);
	}

	#[test]
	fn buying_advances_the_chain_and_swaps_the_wings() {
		// Torpedo bought (0x4640): the chain offers the Multi-Torpedo next.
		let s = strip(&ROWS, 0x4640);
		assert!(s.contains(&Pick::Part(7)) && !s.contains(&Pick::Part(6)));
		// Hi-Lift equipped (0x4244): Standard and Heavy are offered.
		let s = strip(&ROWS, 0x4244);
		assert_eq!(&s[s.len() - 2..], &[Pick::Wing(0), Pick::Wing(2)]);
		// A chain whose rows are all owned (or unowned-able) offers nothing.
		let all = strip(&ROWS, 0xffff);
		assert!(all.iter().all(|p| matches!(p, Pick::Wing(_))));
	}

	fn shack(rar: u32, stage: u32, cur: usize) -> Shack {
		let entries = strip(&ROWS, 0x4240).into_iter().map(|p| match p {
			Pick::Part(r) => Entry { pick: p, name: PART_NAMES[r].to_string(), cost: 10 + r as u32, desc: String::new() },
			Pick::Wing(w) => Entry { pick: p, name: WING_NAMES[w].to_string(), cost: WING_COST, desc: String::new() },
		}).collect();
		Shack { entries, cur, stage, rar, walk: None }
	}

	#[test]
	fn page_lists_rows_and_hits() {
		let s = shack(30, 0, 1);
		let f = Facts { shop: &s, focus: None, scroll: 0, lang: 0, device: Device::Keyboard, glyphs: GlyphSet::from_index(0) };
		let b = build(&f, &rac_engine::ui::font::FontMeasure).unwrap();
		assert_eq!(b.rows.len(), 7, "seven rows visible");
		assert_eq!(b.sel, 1);
		let r = b.rows[2].1;
		assert_eq!(b.hit(r.cx(), r.cy()), Hit::Row(2));
		assert!(b.buy.is_some());
		let poor = shack(1, 0, 0);
		let f = Facts { shop: &poor, ..f };
		assert!(build(&f, &rac_engine::ui::font::FontMeasure).unwrap().buy.is_none(), "no buy button without Raritanium");
		let p = shack(30, 2, 0);
		let f = Facts { shop: &p, ..f };
		let b = build(&f, &rac_engine::ui::font::FontMeasure).unwrap();
		let c = b.confirm.unwrap();
		assert_eq!(b.hit(c.cx(), c.cy()), Hit::Confirm);
	}
}
