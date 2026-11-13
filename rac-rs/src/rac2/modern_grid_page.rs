//! [PC] R&C2's modern Weapons and Gadgets pages, a pure builder (lane pages; game side `modern_grid.rs`, window side
//! `host/screens_grid.rs`). R&C1's approved look (rac1/modern_menu/pause_grid.rs): the pause shell (tab bar, planet, bolts) with the
//! page's tab selected, the cell grid on the left, the detail panel on the right with a HOLE where the retail 3D preview of Ratchet
//! shows through (the host draws one crop of the retail overlay picture into it), an accent Equip button and the hint bar. Words are
//! the game's own texts. Unowned cells are blank; owned cells show the item's icon (tinted: cyan, accent when selected) or, until the
//! level's icons are registered, a ring, and the item's name under it.
use crate::modern_grid::{Cell, Facts, Page};
use crate::modern_pages::{self, Look};
use crate::modern_screen::{Facts as ScreenFacts, Kind};
use rac_engine::ui::kit::data::{Hint, HintKind};
use rac_engine::ui::kit::page::PageScreen;
use rac_engine::ui::kit::shell::SHELL_SCRIM;
use rac_engine::ui::input::Device;
use rac_engine::ui::keycaps::{self, Cap};
use rac_engine::ui::kit::text::{run_width, wrap};
use rac_engine::ui::kit::TextMeasure;
use rac_engine::ui::prim::{Align, Border, FontRole, Prim, Rect, Rgba, Symbol, TextRun};
use rac_engine::ui::theme::*;
use rac_engine::ui::T5;

pub const GRID_PANEL: Rect = Rect::new(48.0, 116.0, 576.0, 540.0);
pub const DETAIL_PANEL: Rect = Rect::new(648.0, 116.0, 584.0, 540.0);
pub const VIEWPORT: Rect = Rect::new(672.0, 136.0, 536.0, 220.0);
pub const BUTTON: Rect = Rect::new(672.0, 592.0, 260.0, 44.0);
const PAD: f32 = 20.0;
const LABEL_W: f32 = 108.0;
const GROUP_GAP: f32 = 12.0;
const CELL_H: f32 = 118.0;
const GADGET_CELL_H: f32 = 78.0;
const GADGET_COLS: usize = 3;
/// The retail hero panel in the 512 x 448 overlay picture: the 3D preview of Ratchet with the weapon / the worn gadgets
/// (R&C2: x 255..468, y 29..188 [C], notes_modern_screens_pages.md 8).
pub const HERO: [f32; 4] = [252.0, 33.5, 216.0, 154.0];
/// The Gadgets page's preview sits lower in the retail picture (measured on the classic page, level 6: blue area x 775..1470, y 118..442 of 1600 x 900).
pub const HERO_GADGETS: [f32; 4] = [250.0, 61.0, 218.0, 154.0];

pub fn hero(page: Page) -> [f32; 4] {
	if page == Page::Gadgets { HERO_GADGETS } else { HERO }
}
const OV_W: f32 = 512.0;
const OV_H: f32 = 448.0;
pub(crate) const CYAN: Rgba = Rgba::rgb(0x7FD3E6);

pub const EQUIP: T5 = ["Equip", "\u{c9}quiper", "Ausr\u{fc}sten", "Equipar", "Equipaggia"];
pub const NO_AMMO: T5 = ["Uses no ammo", "Sans munitions", "Keine Munition", "Sin munici\u{f3}n", "Senza munizioni"];

/// Where a click or hover on the page lands.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Built {
	pub prims: Vec<Prim>,
	/// (group, cell) and its rect, owned or not (holes left out).
	pub cells: Vec<((usize, usize), Rect)>,
	pub button: Option<Rect>,
	/// Where the retail 3D preview shows (design space), and the overlay crop it shows (u0, v0, u1, v1).
	pub hole: Rect,
	pub uv: [f32; 4],
}

impl Built {
	pub fn cell_at(&self, x: f32, y: f32) -> Option<(usize, usize)> {
		self.cells.iter().find(|c| c.1.contains(x, y)).map(|c| c.0)
	}

	pub fn on_button(&self, x: f32, y: f32) -> bool {
		self.button.is_some_and(|b| b.contains(x, y))
	}
}

/// `r` shrunk to the largest rect of aspect w : h inside it, centred.
pub fn contain(r: Rect, w: f32, h: f32) -> Rect {
	if w <= 0.0 || h <= 0.0 {
		return r;
	}
	let s = (r.w / w).min(r.h / h);
	let (cw, ch) = (w * s, h * s);
	Rect::new(r.x + (r.w - cw) * 0.5, r.y + (r.h - ch) * 0.5, cw, ch)
}

/// The hole for a picture of game aspect `aspect`: the overlay fills the picture rect, so a crop of w x h overlay pixels shows with
/// aspect (w * aspect) : (h * 512 / 448).
pub fn hole_for(page: Page, aspect: f32) -> Rect {
	let h = hero(page);
	contain(VIEWPORT, h[2] * aspect, h[3] * OV_W / OV_H)
}

pub fn uv_of(src: [f32; 4]) -> [f32; 4] {
	[src[0] / OV_W, src[1] / OV_H, (src[0] + src[2]) / OV_W, (src[1] + src[3]) / OV_H]
}

pub(crate) fn word(t: &T5, lang: usize) -> &'static str {
	t[lang.min(4)]
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn text(out: &mut Vec<Prim>, x: f32, y: f32, role: FontRole, px: f32, colour: Rgba, align: Align, max_w: f32, s: &str) {
	out.push(Prim::Text(TextRun { x, y, role, px, colour, align, max_w, track: 0.0, text: s.to_string() }));
}

pub(crate) fn fill(out: &mut Vec<Prim>, rect: Rect, radius: f32, fill: Rgba, border: Option<(f32, Rgba)>) {
	out.push(Prim::RRect { rect, radius, fill, border: border.map(|(width, colour)| Border { width, colour }) });
}

/// The pause shell of the page: the tab bar with this page's tab selected, "PAUSED", planet and bolts; hints Equip and Back.
pub fn shell(f: &Facts, look: Look) -> PageScreen {
	let sf = ScreenFacts { kind: Kind::Pause, cur: f.page.tab(), rows: f.tabs.clone(), site: f.site.clone(), bolts: f.bolts };
	let mut s = modern_pages::pause(&sf, look);
	let equip = if f.equip.is_empty() { word(&EQUIP, look.lang).to_string() } else { f.equip.clone() };
	s.hints = vec![Hint { kind: HintKind::Change, text: equip }, Hint { kind: modern_pages::back_kind(look.device), text: word(&modern_pages::BACK, look.lang).to_string() }]; // [b46 U6] the button that really backs out
	s
}

/// The shell's draw list with its giant scrim replaced by four strips around the hole (the preview shows through).
pub fn with_grid(shell: Vec<Prim>, b: &Built) -> Vec<Prim> {
	let mut out = Vec::with_capacity(shell.len() + b.prims.len() + 8);
	let mut done = false;
	for p in shell {
		match &p {
			Prim::RRect { rect, fill, .. } if !done && rect.w >= 10000.0 && *fill == SHELL_SCRIM => {
				done = true;
				out.extend(holed_scrim(b.hole));
			}
			_ => out.push(p),
		}
	}
	out.extend(b.prims.iter().cloned());
	out
}

/// The shell's scrim with a hole: four strips covering what the giant scrim covered (-5000..5000) minus `hole`.
pub fn holed_scrim(hole: Rect) -> Vec<Prim> {
	const B: f32 = 5000.0;
	let strips = [
		Rect::new(-B, -B, 2.0 * B, hole.y + B),
		Rect::new(-B, hole.bottom(), 2.0 * B, B - hole.bottom()),
		Rect::new(-B, hole.y, hole.x + B, hole.h),
		Rect::new(hole.right(), hole.y, B - hole.right(), hole.h),
	];
	strips.into_iter().filter(|r| r.w > 0.0 && r.h > 0.0).map(|rect| Prim::RRect { rect, radius: 0.0, fill: SHELL_SCRIM, border: None }).collect()
}

/// A rounded panel with a rectangular hole: the fill as four strips inside a rounded clip, then the border alone.
pub fn holed_panel(rect: Rect, hole: Rect, radius: f32, fill: Rgba, border: Border) -> Vec<Prim> {
	let strips = [
		Rect::new(rect.x, rect.y, rect.w, hole.y - rect.y),
		Rect::new(rect.x, hole.bottom(), rect.w, rect.bottom() - hole.bottom()),
		Rect::new(rect.x, hole.y, hole.x - rect.x, hole.h),
		Rect::new(hole.right(), hole.y, rect.right() - hole.right(), hole.h),
	];
	let mut out = vec![Prim::ClipPush { rect, radius }];
	out.extend(strips.into_iter().filter(|r| r.w > 0.0 && r.h > 0.0).map(|rect| Prim::RRect { rect, radius: 0.0, fill, border: None }));
	out.push(Prim::ClipPop);
	out.push(Prim::RRect { rect, radius, fill: fill.tint(0.0), border: Some(border) });
	out
}

/// One placed cell: (group, index in it), its rect and the icon box size.
struct Place {
	at: (usize, usize),
	rect: Rect,
	circle: f32,
}

/// Where every cell (holes left out) and each group's label go.
fn places(f: &Facts) -> (Vec<Place>, Vec<Rect>) {
	let inner = GRID_PANEL.inset(PAD);
	let top = inner.y + 34.0;
	let (mut out, mut labels) = (Vec::new(), Vec::new());
	if f.page == Page::Gadgets {
		let rows: usize = f.groups.iter().map(|g| g.rows()).sum::<usize>().max(1);
		let gaps = f.groups.len().saturating_sub(1) as f32 * GROUP_GAP;
		let ch = GADGET_CELL_H.min(((inner.bottom() - top - gaps) / rows as f32).max(1.0));
		let mut y = top;
		for (gi, g) in f.groups.iter().enumerate() {
			let cols = g.cols.max(1);
			let cw = (inner.w - LABEL_W) / cols.max(GADGET_COLS) as f32;
			labels.push(Rect::new(inner.x, y, LABEL_W - 8.0, ch));
			for (k, c) in g.cells.iter().enumerate().filter(|(_, c)| c.item != 0) {
				let (r, col) = ((k / cols) as f32, (k % cols) as f32);
				out.push(Place { at: (gi, k), rect: Rect::new(inner.x + LABEL_W + col * cw, y + r * ch, cw, ch).inset(3.0), circle: 36.0 });
			}
			y += g.rows() as f32 * ch + GROUP_GAP;
		}
		return (out, labels);
	}
	for (gi, g) in f.groups.iter().enumerate() {
		let cols = g.cols.max(1);
		let ch = CELL_H.min((inner.bottom() - top) / g.rows().max(1) as f32);
		let cw = inner.w / cols as f32;
		for (k, c) in g.cells.iter().enumerate().filter(|(_, c)| c.item != 0) {
			out.push(Place { at: (gi, k), rect: Rect::new(inner.x + (k % cols) as f32 * cw, top + (k / cols) as f32 * ch, cw, ch).inset(3.0), circle: 40.0 });
		}
	}
	(out, labels)
}

/// The icon: the item's picture tinted, or a ring while it is not registered.
fn icon(out: &mut Vec<Prim>, c: &Cell, bx: Rect, ink: Rgba) {
	match crate::modern_grid_icons::icon_for(c.icon) {
		Some(i) => out.push(Prim::Image { rect: bx, id: i.0, radius: 0.0, tint: ink }),
		None => {
			fill(out, bx, bx.w / 2.0, ink.tint(0.16), Some((1.5, ink.tint(0.55))));
			let s = bx.w * 0.42;
			fill(out, Rect::new(bx.cx() - s / 2.0, bx.cy() - s / 2.0, s, s), s * 0.28, ink.tint(0.9), None);
		}
	}
}

fn draw_cell(out: &mut Vec<Prim>, p: &Place, c: &Cell, selected: bool) {
	if selected {
		fill(out, p.rect, 12.0, ACCENT.tint(0.13), Some((1.5, ACCENT.tint(0.6))));
	} else {
		fill(out, p.rect, 12.0, WHITE.tint(0.03), None);
	}
	if !c.owned {
		return;
	}
	if c.extra.empty {
		// Retail: out of ammo turns the cell background dark red (0x60202080).
		fill(out, p.rect, 12.0, Rgba::rgb(0xE04040).tint(0.22), None);
	}
	let content = p.circle + 4.0 + 14.0;
	let top = p.rect.y + (p.rect.h - content) * 0.5;
	let bx = Rect::new(p.rect.cx() - p.circle / 2.0, top, p.circle, p.circle);
	// Retail tints the icon by the weapon's tier (cyan, orange, yellow, blue); an untiered icon is cyan, white when selected.
	let ink = match c.extra.tint.filter(|_| c.extra.tier > 0) {
		Some([r, g, b]) => Rgba::rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32),
		None if selected => WHITE,
		None => CYAN,
	};
	icon(out, c, bx, ink);
	if let Some(bar) = c.extra.bar {
		let track = Rect::new(p.rect.x + 14.0, p.rect.bottom() - 9.0, p.rect.w - 28.0, 4.0);
		fill(out, track, 2.0, WHITE.tint(0.12), None);
		let col = if bar.magenta { Rgba::rgb(0xD8_00_66) } else { Rgba::rgb(0xFF_5A_1E) };
		fill(out, Rect::new(track.x, track.y, (track.w * bar.permille as f32 / 1000.0).max(2.0), track.h), 2.0, col, None);
	}
	text(out, p.rect.cx(), top + p.circle + 12.0, FontRole::Medium, 12.0, if selected { WHITE } else { TEXT_BODY }, Align::Center, p.rect.w - 6.0, &c.name);
}

pub(crate) fn draw_button(out: &mut Vec<Prim>, m: &dyn TextMeasure, look: &Look, label: &str) {
	draw_button_at(out, m, look, label, BUTTON);
}

/// [b46 W7] B6 rest: the same Equip button with its Cross glyph / Enter keycap lead at another rect (the Quick Select page's).
pub(crate) fn draw_button_at(out: &mut Vec<Prim>, m: &dyn TextMeasure, look: &Look, label: &str, button: Rect) {
	fill(out, button, 12.0, ACCENT, None);
	// The lead (R&C1 pause_grid draw_button): the Enter keycap with the keyboard, the Cross glyph in the player's set with a pad.
	let keyboard = look.device == Device::Keyboard;
	let cap = keycaps::name(Cap::Enter);
	let lw = if keyboard { (run_width(m, FontRole::SemiBold, 13.0, cap, 0.0) + 14.0).max(24.0) } else { 24.0 };
	let tw = run_width(m, FontRole::SemiBold, 17.0, label, 0.0).min(button.w - 24.0 - 10.0 - lw);
	let x0 = button.cx() - (lw + 10.0 + tw) * 0.5;
	let rect = Rect::new(x0, button.cy() - 12.0, lw, 24.0);
	if keyboard {
		fill(out, rect, 6.0, CAP_BG, Some((1.0, CAP_BORDER)));
		text(out, rect.cx(), rect.cy(), FontRole::SemiBold, 13.0, TEXT, Align::Center, 0.0, cap);
	} else {
		out.push(Prim::Glyph { rect, kind: look.glyphs.glyph(Symbol::Cross), colour: symbol_colour(Symbol::Cross) });
	}
	text(out, x0 + lw + 10.0, button.cy(), FontRole::SemiBold, 17.0, INK_ON_ACCENT, Align::Left, button.w - 24.0 - 10.0 - lw, label);
}

/// The retail MOD rows under the ammo line: icon (sheet 0xea98 frame), the mod's name and its description text.
fn mod_rows(out: &mut Vec<Prim>, m: &dyn TextMeasure, c: &Cell, x: f32, mut y: f32, w: f32) {
	for md in &c.extra.mods {
		let bx = Rect::new(x, y, 30.0, 30.0);
		match crate::modern_grid_icons::mod_icon(md.frame) {
			Some(i) => out.push(Prim::Image { rect: bx, id: i.0, radius: 0.0, tint: CYAN }),
			None => fill(out, bx, 15.0, CYAN.tint(0.2), Some((1.5, CYAN.tint(0.6)))),
		}
		text(out, x + 42.0, y + 15.0, FontRole::SemiBold, 17.0, WHITE, Align::Left, w - 42.0, &md.name);
		let mut ly = y + 42.0;
		for line in wrap(m, FontRole::Regular, 13.0, &md.desc, w).iter().take(2) {
			text(out, x, ly, FontRole::Regular, 13.0, TEXT_DIM, Align::Left, w, line);
			ly += 16.0;
		}
		y += 64.0;
	}
}

fn detail(out: &mut Vec<Prim>, m: &dyn TextMeasure, f: &Facts, c: &Cell) {
	let (x, w) = (VIEWPORT.x, VIEWPORT.w);
	let name = if c.owned { c.name.as_str() } else { "?" };
	text(out, x, VIEWPORT.bottom() + 33.0, FontRole::Condensed, 38.0, if c.owned { WHITE } else { TEXT_DISABLED }, Align::Left, w, name);
	if f.page != Page::Weapons || !c.owned {
		return;
	}
	let y = VIEWPORT.bottom() + 74.0;
	match c.ammo {
		Some((n, max)) => {
			fill(out, Rect::new(x, y + 14.0, w, 8.0), 4.0, BAR_OFF, None);
			fill(out, Rect::new(x, y + 14.0, w * (n as f32 / max.max(1) as f32).clamp(0.0, 1.0), 8.0), 4.0, ACCENT, None);
			text(out, x + w, y, FontRole::SemiBold, 18.0, TEXT, Align::Right, 0.0, &format!("{n} / {max}"));
		}
		None => text(out, x, y, FontRole::Regular, 16.0, TEXT_DIM, Align::Left, w, if f.no_ammo.is_empty() { NO_AMMO[0] } else { &f.no_ammo }),
	}
	mod_rows(out, m, c, x, y + 34.0, w);
}

/// The page body (grid panel, detail panel with the hole, Equip button). `aspect` = the game picture's aspect; `_m` is kept for the
/// text measuring callers share with the other builders.
pub fn build(f: &Facts, m: &dyn TextMeasure, aspect: f32, lang: usize) -> Built {
	build_look(f, m, aspect, &Look { lang, ..Look::default() })
}

/// [b45 P4A] B6: `build` with the hint device and glyph set (`look`), so the Equip button leads with the Cross glyph / Enter keycap.
pub fn build_look(f: &Facts, m: &dyn TextMeasure, aspect: f32, look: &Look) -> Built {
	let lang = look.lang;
	let (placed, labels) = places(f);
	let mut out = Vec::new();
	fill(&mut out, GRID_PANEL, 16.0, PANEL, Some((1.0, HAIRLINE)));
	let inner = GRID_PANEL.inset(PAD);
	let title = f.tabs.get(f.page.tab()).cloned().unwrap_or_default().to_uppercase();
	text(&mut out, inner.x, inner.y + 10.0, FontRole::Condensed, 15.0, TEXT_SECTION, Align::Left, 0.0, &title);
	for (r, g) in labels.iter().zip(&f.groups) {
		text(&mut out, r.x, r.cy(), FontRole::Condensed, 16.0, TEXT_MUTED, Align::Left, r.w, &g.label);
	}
	let mut cells = Vec::new();
	for p in &placed {
		let Some(c) = f.cell(p.at.0, p.at.1) else { continue };
		draw_cell(&mut out, p, c, p.at == f.sel);
		if c.owned { // [b45 P4A] B5: unowned cells are drawn but cannot be hovered or clicked
			cells.push((p.at, p.rect));
		}
	}
	let hole = hole_for(f.page, aspect);
	out.extend(holed_panel(DETAIL_PANEL, hole, 16.0, PANEL, Border { width: 1.0, colour: HAIRLINE }));
	let chosen = f.cell(f.sel.0, f.sel.1);
	if let Some(c) = chosen {
		detail(&mut out, m, f, c);
	}
	let button = chosen.is_some_and(|c| c.owned).then(|| {
		let label = if f.equip.is_empty() { word(&EQUIP, lang).to_string() } else { f.equip.clone() };
		draw_button(&mut out, m, look, &label);
		BUTTON
	});
	Built { prims: out, cells, button, hole, uv: uv_of(hero(f.page)) }
}
