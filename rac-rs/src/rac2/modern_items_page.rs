//! [PC] R&C2's modern ITEMS page, a pure builder (lane pages; game side `modern_items.rs`, window side `host/screens_items.rs`). Same look
//! and shell as the Weapons / Gadgets pages (`modern_grid_page.rs`, whose `Built`, `with_grid` and `holed_panel` it reuses): the pause
//! shell with the Items tab selected, the grid panel on the left (Head and Armor on top, the 4 x 2 item grid, the 2 x 2 collectibles at
//! the bottom), the detail panel on the right with a HOLE where the retail 3D preview of the item shows through, the item's name and
//! the game's own description text. No action button: the retail Items page has none. Unowned cells are blank.
use crate::modern_grid_page::{contain, fill, holed_panel, text, uv_of, word, Built, CYAN, DETAIL_PANEL, GRID_PANEL, VIEWPORT};
use crate::modern_items::{Facts, ICell};
use crate::modern_pages::{self, Look};
use crate::modern_screen::{Facts as ScreenFacts, Kind};
use rac_engine::ui::kit::data::{Hint, HintKind};
use rac_engine::ui::kit::page::PageScreen;
use rac_engine::ui::kit::text::wrap;
use rac_engine::ui::kit::TextMeasure;
use rac_engine::ui::prim::{Align, Border, FontRole, Prim, Rect, Rgba};
use rac_engine::ui::theme::*;
use rac_engine::ui::T5;

/// The retail Items-page 3D preview in the 512 x 448 overlay picture (measured on the classic page, level 9, window 1600 x 900: x 776..1476, y 124..444).
pub const HERO: [f32; 4] = [250.0, 63.0, 218.0, 158.0];
const OV_W: f32 = 512.0;
const OV_H: f32 = 448.0;
const PAD: f32 = 20.0;
/// Tab index of Items in the pause root.
const TAB: usize = 3;

/// The hole for a picture of game aspect `aspect` (the overlay fills the picture rect).
pub fn hole_for(aspect: f32) -> Rect {
	contain(VIEWPORT, HERO[2] * aspect, HERO[3] * OV_W / OV_H)
}

/// The pause shell with the Items tab selected; the hint bar has Back only.
pub fn shell(f: &Facts, look: Look) -> PageScreen {
	let sf = ScreenFacts { kind: Kind::Pause, cur: TAB, rows: f.tabs.clone(), site: f.site.clone(), bolts: f.bolts };
	let mut s = modern_pages::pause(&sf, look);
	s.hints = vec![Hint { kind: modern_pages::back_kind(look.device), text: word(&modern_pages::BACK, look.lang).to_string() }]; // [b46 U6]
	s
}

/// Where each group's cells go: (group, cell, rect).
fn places(f: &Facts) -> Vec<((usize, usize), Rect)> {
	let inner = GRID_PANEL.inset(PAD);
	let top = inner.y + 34.0;
	let mut out = Vec::new();
	let w5 = inner.w / 5.0;
	for k in 0..2 {
		out.push(((0, k), Rect::new(inner.x + k as f32 * w5, top, w5, 84.0).inset(3.0)));
	}
	out.push(((1, 0), Rect::new(inner.x + 4.0 * w5, top, w5, 84.0).inset(3.0)));
	let (gy, w4) = (top + 96.0, inner.w / 4.0);
	for k in 0..f.groups.get(2).map_or(0, |g| g.len()) {
		out.push(((2, k), Rect::new(inner.x + (k % 4) as f32 * w4, gy + (k / 4) as f32 * 96.0, w4, 96.0).inset(3.0)));
	}
	let (cy, w2) = (gy + 2.0 * 96.0 + 14.0, inner.w / 2.0);
	for k in 0..4 {
		out.push(((3, k), Rect::new(inner.x + (k % 2) as f32 * w2, cy + (k / 2) as f32 * 76.0, w2, 76.0).inset(3.0)));
	}
	out
}

/// The icon: the item's picture tinted, or a ring while it is not registered.
fn icon(out: &mut Vec<Prim>, c: &ICell, bx: Rect, ink: Rgba) {
	match crate::modern_grid_icons::icon_for(c.icon) {
		Some(i) => out.push(Prim::Image { rect: bx, id: i.0, radius: 0.0, tint: ink }),
		None => {
			fill(out, bx, bx.w / 2.0, ink.tint(0.16), Some((1.5, ink.tint(0.55))));
			let s = bx.w * 0.42;
			fill(out, Rect::new(bx.cx() - s / 2.0, bx.cy() - s / 2.0, s, s), s * 0.28, ink.tint(0.9), None);
		}
	}
}

fn draw_cell(out: &mut Vec<Prim>, at: (usize, usize), r: Rect, c: &ICell, selected: bool, lang: usize) {
	if selected {
		fill(out, r, 12.0, ACCENT.tint(0.13), Some((1.5, ACCENT.tint(0.6))));
	} else {
		fill(out, r, 12.0, WHITE.tint(0.03), None);
	}
	if !c.owned {
		return;
	}
	let ink = if selected { ACCENT } else { CYAN };
	if at.0 == 3 {
		// A collectible: its picture (sheet 0xeaa2, frame = id), the name and the counters the retail panel prints.
		let bx = Rect::new(r.x + 10.0, r.cy() - 20.0, 40.0, 40.0);
		match crate::modern_grid_icons::collect_icon(c.item as usize) {
			Some(i) => out.push(Prim::Image { rect: bx, id: i.0, radius: 0.0, tint: ink }),
			None => fill(out, bx, 20.0, ink.tint(0.16), Some((1.5, ink.tint(0.55)))),
		}
		let tx = r.x + 58.0;
		text(out, tx, r.y + 22.0, FontRole::Medium, 14.0, if selected { WHITE } else { TEXT_BODY }, Align::Left, r.w - 64.0, &c.name);
		let line = c.lines.iter().map(|(l, v)| format!("{l} {v}")).collect::<Vec<_>>().join("   ");
		text(out, tx, r.y + 46.0, FontRole::Regular, 12.0, ink, Align::Left, r.w - 64.0, &line);
		return;
	}
	let circle = if at.0 == 2 { 38.0 } else { 36.0 };
	let top = r.y + (r.h - (circle + 18.0)) * 0.5;
	icon(out, c, Rect::new(r.cx() - circle / 2.0, top, circle, circle), ink);
	text(out, r.cx(), top + circle + 12.0, FontRole::Medium, 12.0, if selected { WHITE } else { TEXT_BODY }, Align::Center, r.w - 6.0, &c.name);
}

fn detail(out: &mut Vec<Prim>, m: &dyn TextMeasure, c: &ICell, lang: usize) {
	let (x, w) = (VIEWPORT.x, VIEWPORT.w);
	text(out, x, VIEWPORT.bottom() + 33.0, FontRole::Condensed, 38.0, if c.owned { WHITE } else { TEXT_DISABLED }, Align::Left, w, if c.owned { &c.name } else { "?" });
	if !c.owned {
		return;
	}
	let mut y = VIEWPORT.bottom() + 74.0;
	for line in wrap(m, FontRole::Regular, 17.0, &c.desc, w).iter().take(9) {
		text(out, x, y, FontRole::Regular, 17.0, TEXT_BODY, Align::Left, w, line);
		y += 26.0;
	}
	let mut ly = DETAIL_PANEL.bottom() - 30.0 - 30.0 * c.lines.len().saturating_sub(1) as f32;
	for (label, value) in &c.lines {
		text(out, x, ly, FontRole::Regular, 18.0, TEXT_DIM, Align::Left, w, label);
		text(out, x + w, ly, FontRole::SemiBold, 20.0, ACCENT, Align::Right, 0.0, value);
		ly += 30.0;
	}
}

/// The page body (grid panel, detail panel with the hole). `aspect` = the game picture's aspect.
pub fn build(f: &Facts, m: &dyn TextMeasure, aspect: f32, lang: usize) -> Built {
	let mut out = Vec::new();
	fill(&mut out, GRID_PANEL, 16.0, PANEL, Some((1.0, HAIRLINE)));
	let inner = GRID_PANEL.inset(PAD);
	let title = f.tabs.get(TAB).cloned().unwrap_or_default().to_uppercase();
	text(&mut out, inner.x, inner.y + 10.0, FontRole::Condensed, 15.0, TEXT_SECTION, Align::Left, 0.0, &title);
	let mut cells = Vec::new();
	for (at, r) in places(f) {
		let Some(c) = f.cell(at.0, at.1) else { continue };
		draw_cell(&mut out, at, r, c, at == f.sel, lang);
		if c.owned { // [b45 P4A] B22: as the grid, unowned cells are not live
			cells.push((at, r));
		}
	}
	let hole = hole_for(aspect);
	out.extend(holed_panel(DETAIL_PANEL, hole, 16.0, PANEL, Border { width: 1.0, colour: HAIRLINE }));
	if let Some(c) = f.cell(f.sel.0, f.sel.1) {
		detail(&mut out, m, c, lang);
	}
	Built { prims: out, cells, button: None, hole, uv: uv_of(HERO) }
}
