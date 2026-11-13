//! [PC] R&C2's modern vendor page, a pure builder (lane scr): `Shop` (modern_vendor.rs) -> a draw list in the 1280x720 design space
//! plus what each part does when clicked. R&C1's approved look (rac1/modern_menu/vendor.rs, `MOCKUP_vendor.dc.html`): the stock on
//! the left, the selected item's card on the right, the machine free in the middle, the buy prompt as a box over a dimmed world, the
//! hint bar at the bottom. Buy all ammo is the list's first row with its own card, as R&C1's (`modern_vendor_page_all.rs`). Left
//! out against R&C1: the vendor portrait hole and the item icons (R&C2's HUD sheets are not mapped [?]): a ring stands in. Words: R&C1's vendor words (`T5`, same review note).
//! The selected item's 3D model: retail draws it into its overlay picture (the moby program inside the centre panel, gsfull/overlay.rs
//! `MENU_PROG`), so the card leaves a HOLE under its title and the host draws ONE crop of the overlay into it (`MODEL_SRC`), as the
//! Weapons page does with Ratchet (modern_grid_page.rs `hole_for`).
use crate::modern_vendor::{Row, Shop};
use rac_engine::ui::input::{Device, GlyphSet};
use rac_engine::ui::kit::text::{run_width, upper};
use rac_engine::ui::kit::TextMeasure;
use rac_engine::ui::prim::{Align, Border, FontRole, Prim, Rect, Rgba, Symbol, TextRun};
use rac_engine::ui::theme::*;
use rac_engine::ui::T5;

pub const LIST: Rect = Rect::new(48.0, 116.0, 520.0, 540.0);
pub const DETAIL: Rect = Rect::new(852.0, 116.0, 380.0, 540.0);
/// The item model in the 512 x 448 overlay picture: the inside of retail's centre panel around the turning model, above its "Cost" /
/// "You're maxed out!" line (measured on the classic shop, levels 1 and 2: panel x 288..750, model x 425..600, y 240..365 of the 1024
/// x 576 picture), tuned on the modern card (Oozla Minirocket Tube, Endako Chopper): rows up to 114 hold the panel's top frame and
/// retail clips the model at row 115 too (classic Oozla shop, 2026-11-12), the price line starts near row 225 [C pictures].
pub const MODEL_SRC: [f32; 4] = [196.0, 115.0, 120.0, 105.0];
/// Where the model may show in the card: under the title and the ammo bar, above the price line.
const MODEL_AREA: Rect = Rect::new(DETAIL.x + 22.0, DETAIL.y + 112.0, DETAIL.w - 44.0, 290.0);
const ROW_H: f32 = 60.0;
const ROW_GAP: f32 = 4.0;
const VISIBLE: usize = 7;
const RED: Rgba = SYMBOL_CIRCLE;
const TEAL: Rgba = SYMBOL_TRIANGLE;

// R&C1's words (rac1/modern_menu/vendor_text.rs, title.rs BOLTS).
pub const HEADER: T5 = ["Gadgetron Vendor", "Vendeur Gadgetron", "Gadgetron-H\u{e4}ndler", "Vendedor Gadgetron", "Venditore Gadgetron"];
pub const TITLE: T5 = ["Weapons and ammo", "Armes et munitions", "Waffen und Munition", "Armas y munici\u{f3}n", "Armi e munizioni"];
pub const NEW: T5 = ["New", "Nouveau", "Neu", "Nuevo", "Nuovo"];
pub const OWNED: T5 = ["Owned", "Poss\u{e9}d\u{e9}", "Im Besitz", "Tienes", "Posseduto"];
pub const FULL: T5 = ["Full", "Plein", "Voll", "Lleno", "Pieno"];
pub const NEED_BOLTS: T5 = ["Need bolts", "Boulons requis", "Zu wenig Bolzen", "Faltan tuercas", "Servono bulloni"];
pub const AMMO: T5 = ["Ammo", "Munitions", "Munition", "Munici\u{f3}n", "Munizioni"];
pub const BOLTS: T5 = ["Bolts", "Boulons", "Bolzen", "Tuercas", "Bulloni"];
pub const BOLT: T5 = ["Bolt", "Boulon", "Bolzen", "Tuerca", "Bullone"];
pub const EACH: T5 = ["each", "pi\u{e8}ce", "pro St\u{fc}ck", "c/u", "cadauna"];
pub const PRICE: T5 = ["Price", "Prix", "Preis", "Precio", "Prezzo"];
pub const FILL_UP: T5 = ["Fill up {} ammo", "Remplir {} munitions", "{} Munition auff\u{fc}llen", "Rellenar {} de munici\u{f3}n", "Riempi {} munizioni"];
pub const AMMO_FULL: T5 = ["Ammo is full", "Munitions pleines", "Munition voll", "Munici\u{f3}n llena", "Munizioni piene"];
pub const BUY_AMMO: T5 = ["Buy ammo", "Acheter des munitions", "Munition kaufen", "Comprar munici\u{f3}n", "Compra munizioni"];
pub const BUY_WEAPON: T5 = ["Buy weapon", "Acheter l'arme", "Waffe kaufen", "Comprar arma", "Compra arma"];
pub const NOT_ENOUGH: T5 = ["Not enough bolts", "Pas assez de boulons", "Nicht genug Bolzen", "Faltan tuercas", "Bulloni insufficienti"];
pub const ASK_WEAPON: T5 = ["Buy {}?", "Acheter {} ?", "{} kaufen?", "\u{bf}Comprar {}?", "Comprare {}?"];
pub const ASK_AMMO: T5 = ["Buy {} ammo", "Acheter munitions {}", "Munition f\u{fc}r {} kaufen", "Comprar munici\u{f3}n de {}", "Compra munizioni {}"];
pub const HOW_MANY: T5 = ["How many", "Combien", "Wie viele", "Cu\u{e1}ntas", "Quante"];
pub const TOTAL: T5 = ["Total", "Total", "Gesamt", "Total", "Totale"];
pub const BUY: T5 = ["Buy", "Acheter", "Kaufen", "Comprar", "Acquista"];
pub const CANCEL: T5 = ["Cancel", "Annuler", "Abbrechen", "Cancelar", "Annulla"];
pub const LEAVE: T5 = ["Leave", "Quitter", "Verlassen", "Salir", "Esci"];
pub const BUY_ALL: T5 = ["Buy all ammo", "Acheter toutes les munitions", "Alle Munition kaufen", "Comprar toda la munici\u{f3}n", "Compra tutte le munizioni"];
pub const REFILLS: T5 = ["Refills: {}", "Recharge : {}", "F\u{fc}llt auf: {}", "Recarga: {}", "Ricarica: {}"];
pub const REFILLS_MORE: T5 = ["+{} more", "+{} autres", "+{} weitere", "+{} m\u{e1}s", "+{} altri"];

/// Every table, for the font-coverage test.
pub fn all() -> [&'static T5; 26] {
	[&HEADER, &TITLE, &NEW, &OWNED, &FULL, &NEED_BOLTS, &AMMO, &BOLTS, &BOLT, &EACH, &PRICE, &FILL_UP, &AMMO_FULL, &BUY_AMMO, &BUY_WEAPON, &NOT_ENOUGH, &ASK_WEAPON, &ASK_AMMO, &HOW_MANY, &TOTAL, &BUY, &CANCEL, &LEAVE, &BUY_ALL, &REFILLS, &REFILLS_MORE]
}

pub fn word(t: &T5, lang: usize) -> &'static str {
	t[lang.min(4)]
}

fn fill(t: &T5, lang: usize, x: &str) -> String {
	word(t, lang).replace("{}", x)
}

/// A number with thousands commas.
pub fn commas(n: u32) -> String {
	let s = n.to_string();
	let mut out = String::new();
	for (i, c) in s.chars().enumerate() {
		if i > 0 && (s.len() - i) % 3 == 0 {
			out.push(',');
		}
		out.push(c);
	}
	out
}

/// Everything `build` reads.
pub struct Facts<'a> {
	pub shop: &'a Shop,
	/// The row the mouse chose a moment ago (shown at once, before the game's cursor follows).
	pub focus: Option<usize>,
	/// The first row the list showed last frame.
	pub scroll: usize,
	pub lang: usize,
	pub device: Device,
	pub glyphs: GlyphSet,
	/// The game picture's aspect (width / height) while the overlay crop of the item model is drawn; None = no model hole.
	pub model: Option<f32>,
}

/// What a click at a point does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
	None,
	Row(usize),
	Buy,
	Less,
	More,
	Confirm,
	Cancel,
	Hint(Symbol),
	/// The Buy all ammo row.
	AllRow,
	/// The Buy all ammo card's button.
	BuyAll,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Built {
	pub prims: Vec<Prim>,
	pub rows: Vec<(usize, Rect)>,
	pub buy: Option<Rect>,
	pub less: Option<Rect>,
	pub more: Option<Rect>,
	pub confirm: Option<Rect>,
	pub cancel: Option<Rect>,
	pub hints: Vec<(Symbol, Rect)>,
	pub prompt: bool,
	pub sel: usize,
	pub first: usize,
	/// Rows of the whole list.
	pub count: usize,
	/// The Buy all ammo row (while the list browses).
	pub all_row: Option<Rect>,
	/// The Buy all ammo row has the focus: drawn selected, its own card.
	pub buyall: bool,
	/// The card's hole for the item model (design space) and the overlay crop shown in it (u0, v0, u1, v1).
	pub hole: Option<Rect>,
	pub uv: [f32; 4],
}

/// The hole for the item model in a game picture of aspect `aspect`: the crop keeps its shape (modern_grid_page.rs `hole_for`).
pub fn model_hole(aspect: f32) -> Rect {
	crate::modern_grid_page::contain(MODEL_AREA, MODEL_SRC[2] * aspect, MODEL_SRC[3] * 512.0 / 448.0)
}

impl Built {
	pub fn hit(&self, x: f32, y: f32) -> Hit {
		let on = |r: &Option<Rect>| r.is_some_and(|r| r.contains(x, y));
		let hint = || self.hints.iter().find(|h| h.1.contains(x, y)).map_or(Hit::None, |h| Hit::Hint(h.0));
		if self.prompt {
			return [(self.confirm, Hit::Confirm), (self.cancel, Hit::Cancel), (self.less, Hit::Less), (self.more, Hit::More)].into_iter().find(|(r, _)| on(r)).map_or_else(hint, |(_, h)| h);
		}
		if on(&self.buy) {
			return if self.buyall { Hit::BuyAll } else { Hit::Buy };
		}
		if on(&self.all_row) {
			return Hit::AllRow;
		}
		self.rows.iter().find(|r| r.1.contains(x, y)).map_or_else(hint, |r| Hit::Row(r.0))
	}
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn text(out: &mut Vec<Prim>, x: f32, y: f32, role: FontRole, px: f32, colour: Rgba, align: Align, max_w: f32, track: f32, s: &str) {
	out.push(Prim::Text(TextRun { x, y, role, px, colour, align, max_w, track, text: s.to_string() }));
}

pub(crate) fn rrect(out: &mut Vec<Prim>, rect: Rect, radius: f32, fill: Rgba, border: Option<(f32, Rgba)>) {
	out.push(Prim::RRect { rect, radius, fill, border: border.map(|(width, colour)| Border { width, colour }) });
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

fn bolts_word(f: &Facts, n: u32) -> &'static str {
	if n == 1 { word(&BOLT, f.lang) } else { word(&BOLTS, f.lang) }
}

fn header(out: &mut Vec<Prim>, f: &Facts, m: &dyn TextMeasure) {
	text(out, 48.0, 44.0, FontRole::SemiBold, 13.0, TEXT_MUTED, Align::Left, 400.0, 0.18, &upper(word(&HEADER, f.lang)));
	text(out, 48.0, 80.0, FontRole::Condensed, 34.0, TEXT, Align::Left, 700.0, 0.04, &upper(word(&TITLE, f.lang)));
	let n = commas(f.shop.bolts);
	let w = 16.0 + 20.0 + 8.0 + run_width(m, FontRole::Condensed, 24.0, &n, 0.0) + 16.0;
	let r = Rect::new(1232.0 - w, 58.0, w, 44.0);
	rrect(out, r, 10.0, PANEL, Some((1.0, HAIRLINE)));
	rrect(out, Rect::new(r.x + 16.0, r.cy() - 10.0, 20.0, 20.0), 10.0, Rgba::rgba(0, 0, 0, 0.0), Some((2.0, TEXT_BODY)));
	rrect(out, Rect::new(r.x + 22.0, r.cy() - 4.0, 8.0, 8.0), 4.0, TEXT_BODY, None);
	text(out, r.x + 44.0, r.cy(), FontRole::Condensed, 24.0, TEXT, Align::Left, 0.0, 0.0, &n);
}

/// A chip: its word, text colour and background.
fn chip_of(f: &Facts, r: &Row) -> (&'static str, Rgba, Rgba) {
	let bolts = f.shop.bolts;
	if r.ammo {
		if r.full() {
			(word(&FULL, f.lang), TEXT_SECTION, Rgba::rgba(255, 255, 255, 0.06))
		} else if !r.can_buy(bolts) {
			(word(&NEED_BOLTS, f.lang), RED, RED.tint(0.12))
		} else {
			(word(&OWNED, f.lang), TEAL, TEAL.tint(0.14))
		}
	} else if r.can_buy(bolts) {
		(word(&NEW, f.lang), ACCENT, ACCENT.tint(0.16))
	} else {
		(word(&NEED_BOLTS, f.lang), RED, RED.tint(0.12))
	}
}

fn sub_of(f: &Facts, r: &Row) -> String {
	if r.ammo {
		format!("{} {} / {} \u{b7} {} {} {}", word(&AMMO, f.lang), r.owned, r.max, commas(r.price), bolts_word(f, r.price), word(&EACH, f.lang))
	} else {
		format!("{} {}", commas(r.price), bolts_word(f, r.price))
	}
}

/// The first row shown: last frame's window moved as little as keeps row `sel` of `n` in view (R&C1 `scroll_first`).
pub fn first_row(prev: usize, sel: usize, n: usize) -> usize {
	let first = prev.min(n.saturating_sub(VISIBLE));
	if sel < first {
		sel
	} else if sel >= first + VISIBLE {
		sel + 1 - VISIBLE
	} else {
		first
	}
}

fn list(b: &mut Built, f: &Facts, m: &dyn TextMeasure) {
	rrect(&mut b.prims, LIST, 16.0, PANEL, Some((1.0, HAIRLINE)));
	let top = all::row(b, f, m);
	let n = f.shop.rows.len();
	// Scrolled to keep the selection in view; to the top while Buy all ammo has the focus (R&C1).
	b.first = if b.buyall { 0 } else { first_row(f.scroll, b.sel, n) };
	let out = &mut b.prims;
	for (k, i) in (b.first..n.min(b.first + VISIBLE)).enumerate() {
		let r = &f.shop.rows[i];
		let rect = Rect::new(LIST.x + 12.0, top + k as f32 * (ROW_H + ROW_GAP), LIST.w - 24.0, ROW_H);
		let on = !b.buyall && i == b.sel;
		if on {
			rrect(out, rect, 10.0, ACCENT.tint(0.13), Some((1.5, ACCENT.tint(0.6))));
		}
		let ink = if on { ACCENT } else { TEXT_BODY };
		let icon = Rect::new(rect.x + 14.0, rect.cy() - 20.0, 40.0, 40.0);
		rrect(out, icon, 20.0, Rgba::rgba(255, 255, 255, 0.07), None);
		rrect(out, icon.inset(9.0), 11.0, Rgba::rgba(0, 0, 0, 0.0), Some((2.2, ink)));
		rrect(out, icon.inset(15.5), 4.5, ink, None);
		let (word, colour, fill) = chip_of(f, r);
		let w = run_width(m, FontRole::SemiBold, 13.0, word, 0.0) + 20.0;
		let chip = Rect::new(rect.right() - 14.0 - w, rect.cy() - 13.0, w, 26.0);
		rrect(out, chip, 13.0, fill, None);
		text(out, chip.cx(), chip.cy(), FontRole::SemiBold, 13.0, colour, Align::Center, 0.0, 0.0, word);
		let max_w = chip.x - 12.0 - (rect.x + 68.0);
		text(out, rect.x + 68.0, rect.y + 20.0, FontRole::SemiBold, 18.0, TEXT, Align::Left, max_w, 0.0, &r.name);
		text(out, rect.x + 68.0, rect.y + 41.0, FontRole::Regular, 14.0, TEXT_MUTED, Align::Left, max_w, 0.0, &sub_of(f, r));
		b.rows.push((i, rect));
	}
}

fn detail(b: &mut Built, f: &Facts, m: &dyn TextMeasure) {
	if b.buyall {
		rrect(&mut b.prims, DETAIL, 16.0, PANEL, Some((1.0, HAIRLINE)));
		return all::detail(b, f, m);
	}
	match f.model.filter(|a| *a > 0.0 && f.shop.rows.get(b.sel).is_some()) {
		Some(a) => {
			let hole = model_hole(a);
			b.prims.extend(crate::modern_grid_page::holed_panel(DETAIL, hole, 16.0, PANEL, Border { width: 1.0, colour: HAIRLINE }));
			b.hole = Some(hole);
			b.uv = crate::modern_grid_page::uv_of(MODEL_SRC);
		}
		None => rrect(&mut b.prims, DETAIL, 16.0, PANEL, Some((1.0, HAIRLINE))),
	}
	let Some(r) = f.shop.rows.get(b.sel) else { return };
	let (x, w) = (DETAIL.x + 22.0, DETAIL.w - 44.0);
	text(&mut b.prims, x, DETAIL.y + 38.0, FontRole::Condensed, 34.0, WHITE, Align::Left, w, 0.0, &r.name);
	if r.ammo {
		let y = DETAIL.y + 90.0;
		let out = &mut b.prims;
		text(out, x, y, FontRole::Regular, 14.0, TEXT_MUTED, Align::Left, 70.0, 0.0, word(&AMMO, f.lang));
		let bar = Rect::new(x + 78.0, y - 4.0, w - 78.0 - 76.0, 8.0);
		rrect(out, bar, 4.0, BAR_OFF, None);
		let frac = if r.max == 0 { 0.0 } else { (r.owned as f32 / r.max as f32).clamp(0.0, 1.0) };
		if frac > 0.0 {
			rrect(out, Rect::new(bar.x, bar.y, (bar.w * frac).max(8.0), 8.0), 4.0, ACCENT, None);
		}
		text(out, x + w, y, FontRole::SemiBold, 16.0, TEXT, Align::Right, 72.0, 0.0, &format!("{} / {}", r.owned, r.max));
	}
	buy_button(b, f, m, r);
}

fn buy_button(b: &mut Built, f: &Facts, m: &dyn TextMeasure, r: &Row) {
	let (x, w) = (DETAIL.x + 22.0, DETAIL.w - 44.0);
	let btn = Rect::new(x, DETAIL.bottom() - 22.0 - 46.0, w, 46.0);
	let can = r.can_buy(f.shop.bolts);
	let (label, price, red) = if r.ammo {
		let missing = r.max.saturating_sub(r.owned);
		if r.full() { (word(&AMMO_FULL, f.lang).to_string(), "\u{2014}".to_string(), false) } else { (fill(&FILL_UP, f.lang, &missing.to_string()), commas(missing * r.price), !can) }
	} else {
		(word(&PRICE, f.lang).to_string(), commas(r.price), !can)
	};
	price_line(&mut b.prims, btn, &label, &price, red);
	let say = match (r.ammo, can, r.full()) {
		(true, _, true) => word(&AMMO_FULL, f.lang),
		(_, false, _) => word(&NOT_ENOUGH, f.lang),
		(true, _, _) => word(&BUY_AMMO, f.lang),
		_ => word(&BUY_WEAPON, f.lang),
	};
	button(b, f, m, btn, say, can);
}

/// The price line above the detail card's button `btn`: its label on the left, the price on the right.
fn price_line(out: &mut Vec<Prim>, btn: Rect, label: &str, price: &str, red: bool) {
	let y = btn.y - 12.0 - 18.0;
	text(out, btn.x, y, FontRole::Regular, 14.0, TEXT_MUTED, Align::Left, 190.0, 0.0, label);
	text(out, btn.right(), y, FontRole::Condensed, 28.0, if red { RED } else { WHITE }, Align::Right, 130.0, 0.0, price);
}

/// The detail card's button: live (the accent with the Cross lead; a click is Buy) or disabled with the reason.
fn button(b: &mut Built, f: &Facts, m: &dyn TextMeasure, btn: Rect, say: &str, can: bool) {
	let out = &mut b.prims;
	if can {
		rrect(out, btn, 10.0, ACCENT, None);
		let tw = run_width(m, FontRole::SemiBold, 17.0, say, 0.0);
		let lw = lead_w(f, Symbol::Cross, m);
		let x0 = btn.cx() - (lw + 10.0 + tw) * 0.5;
		lead(out, f, Symbol::Cross, x0, btn.cy(), m);
		text(out, x0 + lw + 10.0, btn.cy(), FontRole::SemiBold, 17.0, INK_ON_ACCENT, Align::Left, btn.w - 24.0 - 10.0 - lw, 0.0, say);
		b.buy = Some(btn);
	} else {
		rrect(out, btn, 10.0, BUTTON_BG, None);
		text(out, btn.cx(), btn.cy(), FontRole::SemiBold, 17.0, TEXT_DISABLED, Align::Center, btn.w - 24.0, 0.0, say);
	}
}

fn hints(b: &mut Built, f: &Facts, m: &dyn TextMeasure) {
	// Buy, Leave (Cancel under the prompt); Buy all ammo is the list's first row (R&C1).
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
	let Some(r) = f.shop.rows.get(b.sel) else { return };
	let out = &mut b.prims;
	out.push(Prim::Backdrop { blur_px: BLUR_PEEK, scrim: Rgba::rgba(4, 6, 12, 0.62) });
	let d = Rect::new(400.0, 210.0, 480.0, if r.ammo { 258.0 } else { 206.0 });
	rrect(out, d, 16.0, Rgba::rgba(14, 20, 34, 0.97), Some((1.0, Rgba::rgba(255, 255, 255, 0.10))));
	let (x, w) = (d.x + 26.0, d.w - 52.0);
	text(out, x, d.y + 44.0, FontRole::Condensed, 30.0, TEXT, Align::Left, w, 0.0, &fill(if r.ammo { &ASK_AMMO } else { &ASK_WEAPON }, f.lang, &r.name));
	let mut y = d.y + 78.0;
	let qty = f.shop.qty.max(1);
	if r.ammo {
		text(out, x, y + 18.0, FontRole::Regular, 16.0, TEXT_MUTED, Align::Left, 200.0, 0.0, word(&HOW_MANY, f.lang));
		let more = Rect::new(x + w - 36.0, y, 36.0, 36.0);
		let less = Rect::new(more.x - 110.0, y, 36.0, 36.0);
		for (rect, right) in [(less, false), (more, true)] {
			rrect(out, rect, 8.0, CHIP_BG, None);
			out.push(Prim::Chevron { rect: Rect::new(rect.cx() - 7.0, rect.cy() - 7.0, 14.0, 14.0), right, colour: CHEVRON });
		}
		text(out, less.right() + 37.0, y + 18.0, FontRole::Condensed, 28.0, WHITE, Align::Center, 54.0, 0.0, &qty.to_string());
		(b.less, b.more) = (Some(less), Some(more));
		y += 52.0;
	}
	rrect(out, Rect::new(x, y, w, 1.0), 0.0, HAIRLINE, None);
	let total = if r.ammo { qty.saturating_mul(r.price) } else { r.price };
	text(out, x, y + 24.0, FontRole::Regular, 15.0, TEXT_MUTED, Align::Left, 120.0, 0.0, word(&TOTAL, f.lang));
	text(out, x + w, y + 24.0, FontRole::Condensed, 28.0, WHITE, Align::Right, w - 130.0, 0.0, &format!("{} {}", commas(total), bolts_word(f, total)));
	let by = d.bottom() - 72.0;
	let half = (w - 12.0) * 0.5;
	let (ok, no) = (Rect::new(x, by, half, 46.0), Rect::new(x + half + 12.0, by, half, 46.0));
	rrect(out, ok, 10.0, ACCENT, None);
	rrect(out, no, 10.0, CHIP_BG, None);
	text(out, ok.cx(), ok.cy(), FontRole::SemiBold, 17.0, INK_ON_ACCENT, Align::Center, half - 16.0, 0.0, word(&BUY, f.lang));
	text(out, no.cx(), no.cy(), FontRole::SemiBold, 17.0, TEXT, Align::Center, half - 16.0, 0.0, word(&CANCEL, f.lang));
	(b.confirm, b.cancel) = (Some(ok), Some(no));
}

/// The page of `f`; None while the list is empty.
pub fn build(f: &Facts, m: &dyn TextMeasure) -> Option<Built> {
	let s = f.shop;
	if s.rows.is_empty() {
		return None;
	}
	let sel = if s.prompt() { s.cur } else { f.focus.unwrap_or(s.cur) }.min(s.rows.len() - 1);
	let mut b = Built { prompt: s.prompt(), sel, count: s.rows.len(), buyall: s.all_focus && !s.prompt(), ..Built::default() };
	header(&mut b.prims, f, m);
	list(&mut b, f, m);
	detail(&mut b, f, m);
	hints(&mut b, f, m);
	if b.prompt {
		prompt(&mut b, f);
	}
	Some(b)
}

#[path = "modern_vendor_page_all.rs"]
mod all;
