//! [PC] R&C2's modern Galactic Map page, a pure builder (lane scrmap): `Map` (modern_map.rs) -> a draw list in the 1280x720 design
//! space plus what each part does when clicked. The look of the modern vendor (modern_vendor_page.rs): the destinations as a list on
//! the left, on the right the star chart (the retail marker table 0x264ec0, rows = levels) over the selected planet's card with its
//! travel button, the hint bar at the bottom. Left out against retail: the planet picture (a cropped retail overlay).
use crate::modern_map::{Entry, Map};
use crate::modern_vendor_page::{first_row, word, LEAVE};
use rac_engine::ui::input::{Device, GlyphSet};
use rac_engine::ui::kit::text::{run_width, upper};
use rac_engine::ui::kit::TextMeasure;
use rac_engine::ui::prim::{Align, Border, FontRole, Prim, Rect, Rgba, Symbol, TextRun};
use rac_engine::ui::theme::*;
use rac_engine::ui::T5;

pub const LIST: Rect = Rect::new(48.0, 116.0, 440.0, 540.0);
pub const DETAIL: Rect = Rect::new(512.0, 116.0, 720.0, 540.0);
const ROW_H: f32 = 60.0;
const ROW_GAP: f32 = 4.0;
const VISIBLE: usize = 7;
const TEAL: Rgba = SYMBOL_TRIANGLE;

pub const HEADER: T5 = ["Galactic Map", "Carte galactique", "Galaxiekarte", "Mapa gal\u{e1}ctico", "Mappa galattica"];
pub const TITLE: T5 = ["Choose a destination", "Choisir une destination", "Reiseziel w\u{e4}hlen", "Elige un destino", "Scegli la destinazione"];
pub const HERE: T5 = ["Here", "Ici", "Hier", "Aqu\u{ed}", "Qui"];
pub const TRAVEL: T5 = ["Travel", "Voyager", "Reisen", "Viajar", "Viaggia"];
pub const CLOSE_MAP: T5 = ["Close map", "Fermer la carte", "Karte schlie\u{df}en", "Cerrar mapa", "Chiudi mappa"];
pub const YOU_ARE_HERE: T5 = ["You are here", "Vous \u{ea}tes ici", "Du bist hier", "Est\u{e1}s aqu\u{ed}", "Sei qui"];

/// Every table, for the font-coverage test.
pub fn all() -> [&'static T5; 6] {
	[&HEADER, &TITLE, &HERE, &TRAVEL, &CLOSE_MAP, &YOU_ARE_HERE]
}

/// The retail marker table 0x264ec0 [C notes_vendor_map 5.2]: row = level, (x, y, size).
pub const MARKERS: [(i32, i32, f32); 21] = [
	(-99, -73, 16.0), (15, -109, 12.0), (58, -118, 21.2), (104, -118, 10.4), (137, -122, 21.6), (170, -79, 12.0), (163, -43, 14.0),
	(185, -36, 16.7), (148, -1, 16.0), (114, 24, 12.0), (62, 8, 14.0), (6, -20, 18.0), (-10, -52, 16.0), (10, -89, 12.0), (35, -98, 14.0),
	(61, -82, 18.0), (103, -80, 16.0), (92, -17, 12.0), (92, -39, 14.0), (45, -19, 18.0), (22, -58, 27.7),
];

/// [PC] Markers of the levels past retail's table (it has rows 0..20 only and draws no marker for the rest [C notes_vendor_map 5.2:
/// the marker code guards `< 0x15`]): placed by hand in free spots of the chart, the two orbits next to their planets (sweep B11).
/// Rows: level 21 Dantopia, 22 Dobbo Orbit (by Dobbo), 23 Damosel Orbit (by Damosel), 24 Ship Shack, 25 Wupash, 26 Jamming Array.
pub const MARKERS_PC: [(i32, i32, f32); 6] = [(-55, 10, 10.0), (138, 26, 10.0), (75, -40, 10.0), (-65, -25, 12.0), (190, -100, 10.0), (185, 15, 10.0)];

/// Everything `build` reads.
pub struct Facts<'a> {
	pub map: &'a Map,
	/// The entry the mouse chose a moment ago (shown at once, before the game's cursor follows).
	pub focus: Option<usize>,
	/// The first row the list showed last frame.
	pub scroll: usize,
	pub lang: usize,
	pub device: Device,
	pub glyphs: GlyphSet,
	/// [b46 W5] The galaxy painting is under the page (`modern_map_art`): no blurred backdrop, markers pulse.
	pub art: bool,
	/// Display clock, seconds (`modern_map_art::clock_s`): the markers' pulses.
	pub t: f64,
	/// [b48 Y3] B12: the game picture's aspect while the retail overlay crop of the level picture is drawn (None = no crop this frame).
	pub pic: Option<f32>,
}

/// What a click at a point does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
	None,
	Row(usize),
	Go,
	Hint(Symbol),
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Built {
	pub prims: Vec<Prim>,
	pub rows: Vec<(usize, Rect)>,
	/// The chart's planet dots.
	pub dots: Vec<(usize, Rect)>,
	pub go: Option<Rect>,
	pub hints: Vec<(Symbol, Rect)>,
	pub sel: usize,
	pub first: usize,
	pub count: usize,
	/// [b48 Y3] The card's hole for the planet picture (design space) and the overlay crop shown in it (u0, v0, u1, v1).
	pub hole: Option<Rect>,
	pub uv: [f32; 4],
}

/// [b48 Y3] B12: the planet picture in the retail Galactic Map screen = the strip bottom right [C bank 0349 Screenshot, about
/// x 205..600 y 283..365 of 640x480; [I] that the overlay spans the picture 1:1]. Fractions of the overlay.
pub const PIC_SRC: [f32; 4] = [0.32, 0.59, 0.94, 0.76];
/// Where the picture may show in the card: between the planet text and the Travel button.
const PIC_AREA: Rect = Rect::new(DETAIL.x + 22.0, DETAIL.y + 336.0, DETAIL.w - 44.0, 104.0);

/// The hole for the picture in a game picture of aspect `aspect` (the crop keeps its shape).
pub fn pic_hole(aspect: f32) -> Rect {
	let (w, h) = ((PIC_SRC[2] - PIC_SRC[0]) * 512.0, (PIC_SRC[3] - PIC_SRC[1]) * 448.0);
	crate::modern_grid_page::contain(PIC_AREA, w * aspect, h * 512.0 / 448.0)
}

impl Built {
	pub fn hit(&self, x: f32, y: f32) -> Hit {
		if self.go.is_some_and(|r| r.contains(x, y)) {
			return Hit::Go;
		}
		if let Some(r) = self.rows.iter().find(|r| r.1.contains(x, y)) {
			return Hit::Row(r.0);
		}
		if let Some(d) = self.dots.iter().find(|d| d.1.contains(x, y)) {
			return Hit::Row(d.0);
		}
		self.hints.iter().find(|h| h.1.contains(x, y)).map_or(Hit::None, |h| Hit::Hint(h.0))
	}
}

#[allow(clippy::too_many_arguments)]
fn text(out: &mut Vec<Prim>, x: f32, y: f32, role: FontRole, px: f32, colour: Rgba, align: Align, max_w: f32, track: f32, s: &str) {
	out.push(Prim::Text(TextRun { x, y, role, px, colour, align, max_w, track, text: s.to_string() }));
}

fn rrect(out: &mut Vec<Prim>, rect: Rect, radius: f32, fill: Rgba, border: Option<(f32, Rgba)>) {
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

fn header(out: &mut Vec<Prim>, f: &Facts) {
	text(out, 48.0, 44.0, FontRole::SemiBold, 13.0, TEXT_MUTED, Align::Left, 400.0, 0.18, &upper(word(&HEADER, f.lang)));
	text(out, 48.0, 80.0, FontRole::Condensed, 34.0, TEXT, Align::Left, 700.0, 0.04, &upper(word(&TITLE, f.lang)));
}

fn list(b: &mut Built, f: &Facts, m: &dyn TextMeasure) {
	rrect(&mut b.prims, LIST, 16.0, PANEL, Some((1.0, HAIRLINE)));
	let n = f.map.entries.len();
	b.first = first_row(f.scroll, b.sel, n).min(n.saturating_sub(VISIBLE));
	let out = &mut b.prims;
	for (k, i) in (b.first..n.min(b.first + VISIBLE)).enumerate() {
		let e = &f.map.entries[i];
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
		let mut right = rect.right() - 14.0;
		if e.level == f.map.here {
			let word = word(&HERE, f.lang);
			let w = run_width(m, FontRole::SemiBold, 13.0, word, 0.0) + 20.0;
			let chip = Rect::new(right - w, rect.cy() - 13.0, w, 26.0);
			rrect(out, chip, 13.0, TEAL.tint(0.14), None);
			text(out, chip.cx(), chip.cy(), FontRole::SemiBold, 13.0, TEAL, Align::Center, 0.0, 0.0, word);
			right = chip.x - 12.0;
		}
		let max_w = right - (rect.x + 68.0);
		if e.loc.is_empty() {
			text(out, rect.x + 68.0, rect.cy(), FontRole::SemiBold, 18.0, TEXT, Align::Left, max_w, 0.0, &e.planet);
		} else {
			text(out, rect.x + 68.0, rect.y + 20.0, FontRole::SemiBold, 18.0, TEXT, Align::Left, max_w, 0.0, &e.planet);
			text(out, rect.x + 68.0, rect.y + 41.0, FontRole::Regular, 14.0, TEXT_MUTED, Align::Left, max_w, 0.0, &e.loc);
		}
		b.rows.push((i, rect));
	}
}

/// The chart position of a level's marker: the retail table's centre (x + size / 2), fitted into the chart; levels past the table
/// use `MARKERS_PC`; an unknown level sits in the middle.
pub fn marker_pos(level: u32, chart: Rect) -> (f32, f32, f32) {
	let row = MARKERS.get(level as usize).or_else(|| MARKERS_PC.get((level as usize).wrapping_sub(MARKERS.len())));
	let Some(&(x, y, size)) = row else { return (chart.cx(), chart.cy(), 16.0) };
	let (cx, cy) = (x as f32 + size * 0.5, y as f32 + size * 0.5);
	let (x0, x1, y0, y1) = (-99.0, 196.0, -118.0, 34.0);
	let px = chart.x + 36.0 + (cx - x0) / (x1 - x0) * (chart.w - 72.0);
	let py = chart.y + 28.0 + (cy - y0) / (y1 - y0) * (chart.h - 56.0);
	(px, py, size)
}

fn chart(b: &mut Built, f: &Facts) {
	let c = Rect::new(DETAIL.x + 22.0, DETAIL.y + 22.0, DETAIL.w - 44.0, 216.0); // [b48 Y3] was 286: room for the planet picture
	let out = &mut b.prims;
	rrect(out, c, 12.0, Rgba::rgba(255, 255, 255, 0.03), Some((1.0, HAIRLINE)));
	for d in [264.0, 176.0, 84.0] {
		let d = d * c.h / 286.0; // [b48 R2] the rings shrink with the chart, so the outer one stays inside it
		rrect(out, Rect::new(c.cx() - d * 1.15, c.cy() - d * 0.5, d * 2.3, d), d * 0.5, Rgba::rgba(0, 0, 0, 0.0), Some((1.2, ACCENT.tint(0.22))));
	}
	let (sx, sy, _) = f.map.entries.get(b.sel).map_or((c.cx(), c.cy(), 0.0), |e| marker_pos(e.level, c));
	rrect(out, Rect::new(c.x + 8.0, sy - 0.5, c.w - 16.0, 1.0), 0.0, ACCENT.tint(0.30), None);
	rrect(out, Rect::new(sx - 0.5, c.y + 8.0, 1.0, c.h - 16.0), 0.0, ACCENT.tint(0.30), None);
	for (i, e) in f.map.entries.iter().enumerate() {
		let (px, py, size) = marker_pos(e.level, c);
		let r = (size * 0.42).clamp(5.0, 11.0);
		let (colour, on) = if i == b.sel { (ACCENT, true) } else if e.level == f.map.here { (TEAL, false) } else { (TEXT_BODY.tint(0.7), false) };
		if on {
			let k = if f.art { 0.75 + 0.25 * crate::modern_map_art::pulse(f.t, 2.4, 0.0) } else { 1.0 }; // [b46 W5] slow marker pulse
			rrect(out, Rect::new(px - r - 6.0, py - r - 6.0, 2.0 * r + 12.0, 2.0 * r + 12.0), r + 6.0, ACCENT.tint(0.16 * k), Some((1.5, ACCENT.tint(0.7 * k))));
		}
		rrect(out, Rect::new(px - r, py - r, 2.0 * r, 2.0 * r), r, colour, None);
		b.dots.push((i, Rect::new(px - r - 6.0, py - r - 6.0, 2.0 * r + 12.0, 2.0 * r + 12.0)));
	}
}

fn card(b: &mut Built, f: &Facts, m: &dyn TextMeasure, e: &Entry) {
	let (x, w) = (DETAIL.x + 22.0, DETAIL.w - 44.0);
	let out = &mut b.prims;
	text(out, x, DETAIL.y + 268.0, FontRole::Condensed, 34.0, WHITE, Align::Left, w, 0.0, &e.planet);
	if !e.loc.is_empty() {
		text(out, x, DETAIL.y + 300.0, FontRole::Regular, 16.0, TEXT_MUTED, Align::Left, w, 0.0, &e.loc);
	}
	let btn = Rect::new(x, DETAIL.bottom() - 22.0 - 46.0, w, 46.0);
	let here = e.level == f.map.here;
	rrect(out, btn, 10.0, ACCENT, None);
	let say = word(if here { &CLOSE_MAP } else { &TRAVEL }, f.lang);
	let tw = run_width(m, FontRole::SemiBold, 17.0, say, 0.0);
	let lw = lead_w(f, Symbol::Cross, m);
	let x0 = btn.cx() - (lw + 10.0 + tw) * 0.5;
	lead(out, f, Symbol::Cross, x0, btn.cy(), m);
	text(out, x0 + lw + 10.0, btn.cy(), FontRole::SemiBold, 17.0, INK_ON_ACCENT, Align::Left, btn.w - 24.0 - 10.0 - lw, 0.0, say);
	if here {
		text(out, x, btn.y - 20.0, FontRole::Regular, 14.0, TEAL, Align::Left, w, 0.0, word(&YOU_ARE_HERE, f.lang));
	}
	b.go = Some(btn);
}

fn hints(b: &mut Built, f: &Facts, m: &dyn TextMeasure) {
	let here = f.map.entries.get(b.sel).is_some_and(|e| e.level == f.map.here);
	let items = [(Symbol::Cross, word(if here { &CLOSE_MAP } else { &TRAVEL }, f.lang)), (Symbol::Triangle, word(&LEAVE, f.lang))];
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

/// The page of `f`; None while the list is empty.
pub fn build(f: &Facts, m: &dyn TextMeasure) -> Option<Built> {
	let n = f.map.entries.len();
	if n == 0 {
		return None;
	}
	let sel = f.focus.unwrap_or(f.map.cur).min(n - 1);
	let mut b = Built { sel, count: n, ..Built::default() };
	if !f.art {
		b.prims.push(Prim::Backdrop { blur_px: BLUR_PEEK, scrim: Rgba::rgba(4, 6, 12, 0.72) });
	}
	header(&mut b.prims, f);
	list(&mut b, f, m);
	match f.pic {
		Some(a) if a > 0.0 => {
			let hole = pic_hole(a);
			b.prims.extend(crate::modern_grid_page::holed_panel(DETAIL, hole, 16.0, PANEL, Border { width: 1.0, colour: HAIRLINE }));
			b.hole = Some(hole);
			b.uv = PIC_SRC;
		}
		_ => rrect(&mut b.prims, DETAIL, 16.0, PANEL, Some((1.0, HAIRLINE))),
	}
	chart(&mut b, f);
	card(&mut b, f, m, &f.map.entries[sel]);
	hints(&mut b, f, m);
	Some(b)
}
