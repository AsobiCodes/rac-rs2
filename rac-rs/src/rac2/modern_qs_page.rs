//! [PC] R&C2's modern Quick Select page, a pure builder (lane pages2; game side `modern_qs.rs`, window side `host/screens_qs.rs`).
//! R&C1's approved look (rac1/modern_menu/pause_quick.rs): the pause shell with the Quick Select tab selected, the weapons grid on
//! the left (a dot on the items that already sit in the ring, an ammo bar), on the right the ring of eight slots (slot 0 at the top,
//! clockwise, the game's own order) with the grid item's name in the middle, L1 / R1 turn arrows, the Hand gadgets grid under it and
//! an accent Equip button. Words are the game's own texts. No retail 3D preview on this page (no hole).
use crate::modern_grid::Cell;
use crate::modern_grid_page::{DETAIL_PANEL, GRID_PANEL};
use crate::modern_pages::{self, Look};
use crate::modern_qs::{Facts, HAND_COLS, WEAPON_COLS};
use crate::modern_screen::{Facts as ScreenFacts, Kind};
use rac_engine::ui::kit::data::{Hint, HintKind};
use rac_engine::ui::kit::page::PageScreen;
use rac_engine::ui::prim::{Align, Border, FontRole, Prim, Rect, Rgba, TextRun};
use rac_engine::ui::theme::*;

pub const BUTTON: Rect = Rect::new(672.0, 598.0, 260.0, 44.0);
const PAD: f32 = 20.0;
const CYAN: Rgba = Rgba::rgb(0x7FD3E6);
const CELL_H: f32 = 72.0;
const RING_AT: (f32, f32) = (940.0, 286.0);
const RING_R: f32 = 98.0;
const SLOT: f32 = 54.0;
const ARROW: f32 = 44.0;
const HAND_Y: f32 = 478.0;
const HAND_H: f32 = 52.0;
#[allow(clippy::too_many_arguments)]
fn text(out: &mut Vec<Prim>, x: f32, y: f32, role: FontRole, px: f32, colour: Rgba, align: Align, max_w: f32, s: &str) {
	out.push(Prim::Text(TextRun { x, y, role, px, colour, align, max_w, track: 0.0, text: s.to_string() }));
}

fn fill(out: &mut Vec<Prim>, rect: Rect, radius: f32, fill: Rgba, border: Option<(f32, Rgba)>) {
	out.push(Prim::RRect { rect, radius, fill, border: border.map(|(width, colour)| Border { width, colour }) });
}

pub const EQUIP: [&str; 5] = ["Equip", "\u{c9}quiper", "Ausr\u{fc}sten", "Equipar", "Equipaggia"];

/// Where a click or hover on the page lands.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Built {
	pub prims: Vec<Prim>,
	/// (group, cell index) and its rect (holes and the cells of nothing left out).
	pub cells: Vec<((usize, usize), Rect)>,
	pub slots: [Rect; 8],
	/// The L1 (turn back) and R1 (turn forward) arrows.
	pub prev: Rect,
	pub next: Rect,
	pub button: Option<Rect>,
}

impl Built {
	pub fn cell_at(&self, x: f32, y: f32) -> Option<(usize, usize)> {
		self.cells.iter().find(|c| c.1.contains(x, y)).map(|c| c.0)
	}

	pub fn slot_at(&self, x: f32, y: f32) -> Option<u32> {
		self.slots.iter().position(|r| r.contains(x, y)).map(|i| i as u32)
	}

	pub fn on_button(&self, x: f32, y: f32) -> bool {
		self.button.is_some_and(|b| b.contains(x, y))
	}
}

/// Where slot `i` sits: 0 at the top, clockwise.
pub fn slot_centre(i: usize) -> (f32, f32) {
	let a = i as f32 * std::f32::consts::FRAC_PI_4 - std::f32::consts::FRAC_PI_2;
	(RING_AT.0 + a.cos() * RING_R, RING_AT.1 + a.sin() * RING_R)
}

/// The pause shell with the Quick Select tab selected; hints Equip and Back.
pub fn shell(f: &Facts, look: Look) -> PageScreen {
	let sf = ScreenFacts { kind: Kind::Pause, cur: 2, rows: f.tabs.clone(), site: f.site.clone(), bolts: f.bolts };
	let mut s = modern_pages::pause(&sf, look);
	let eq = equip_word(f, look.lang);
	s.hints = vec![Hint { kind: HintKind::Change, text: eq }, Hint { kind: modern_pages::back_kind(look.device), text: modern_pages::word(&modern_pages::BACK, look.lang).to_string() }]; // [b46 U6]
	s
}

fn equip_word(f: &Facts, lang: usize) -> String {
	if f.equip.is_empty() { EQUIP[lang.min(4)].to_string() } else { f.equip.clone() }
}

fn pic(out: &mut Vec<Prim>, icon: u32, bx: Rect, ink: Rgba) {
	match crate::modern_grid_icons::icon_for(icon) {
		Some(i) => out.push(Prim::Image { rect: bx, id: i.0, radius: 0.0, tint: ink }),
		None => {
			fill(out, bx, bx.w / 2.0, ink.tint(0.16), Some((1.5, ink.tint(0.55))));
			let s = bx.w * 0.42;
			fill(out, Rect::new(bx.cx() - s / 2.0, bx.cy() - s / 2.0, s, s), s * 0.28, ink.tint(0.9), None);
		}
	}
}

fn draw_cell(out: &mut Vec<Prim>, f: &Facts, c: &Cell, rect: Rect, selected: bool, circle: f32) {
	if selected {
		fill(out, rect, 12.0, ACCENT.tint(0.13), Some((1.5, ACCENT.tint(0.6))));
	} else {
		fill(out, rect, 12.0, WHITE.tint(0.03), None);
	}
	if !c.owned {
		return;
	}
	let content = circle + 2.0 + 13.0;
	let top = rect.y + (rect.h - content) * 0.5;
	pic(out, c.icon, Rect::new(rect.cx() - circle / 2.0, top, circle, circle), if selected { ACCENT } else { CYAN });
	text(out, rect.cx(), top + circle + 10.0, FontRole::Medium, 11.0, if selected { WHITE } else { TEXT_BODY }, Align::Center, rect.w - 6.0, &c.name);
	if let Some((n, max)) = c.ammo {
		let (w, y) = (rect.w - 24.0, rect.bottom() - 6.0);
		fill(out, Rect::new(rect.x + 12.0, y, w, 3.0), 1.5, BAR_OFF, None);
		fill(out, Rect::new(rect.x + 12.0, y, w * (n as f32 / max.max(1) as f32).clamp(0.0, 1.0), 3.0), 1.5, ACCENT, None);
	}
	if f.in_ring(c.item) {
		fill(out, Rect::new(rect.right() - 15.0, rect.y + 7.0, 8.0, 8.0), 4.0, ACCENT, None);
	}
}

/// A ring item's (held, max) ammo: the weapons grid's cell of the same item (gadgets and unknown items: None).
pub fn ring_ammo(f: &Facts, item: u32) -> Option<(u32, u32)> {
	f.weapons.iter().find(|c| c.item == item && c.owned).and_then(|c| c.ammo)
}

fn ring(out: &mut Vec<Prim>, f: &Facts) -> ([Rect; 8], Rect, Rect) {
	fill(out, DETAIL_PANEL, 16.0, PANEL, Some((1.0, HAIRLINE)));
	fill(out, Rect::new(RING_AT.0 - RING_R, RING_AT.1 - RING_R, 2.0 * RING_R, 2.0 * RING_R), RING_R, WHITE.tint(0.0), Some((2.0, HAIRLINE)));
	let mut slots = [Rect::new(0.0, 0.0, 0.0, 0.0); 8];
	for (i, hit) in slots.iter_mut().enumerate() {
		let (sx, sy) = slot_centre(i);
		let disc = Rect::new(sx - SLOT / 2.0, sy - SLOT / 2.0, SLOT, SLOT);
		*hit = disc;
		let cur = i as u32 == f.ring_target();
		if cur {
			fill(out, Rect::new(disc.x - 5.0, disc.y - 5.0, SLOT + 10.0, SLOT + 10.0), (SLOT + 10.0) / 2.0, ACCENT.tint(0.16), Some((2.0, ACCENT)));
		} else {
			fill(out, disc, SLOT / 2.0, WHITE.tint(0.05), Some((1.0, HAIRLINE)));
		}
		if let Some(s) = f.ring.get(i).filter(|s| s.item != 0) {
			let d = SLOT * 0.66;
			pic(out, s.icon, Rect::new(sx - d / 2.0, sy - d / 2.0, d, d), WHITE);
			// The slot's ammo bar (retail draws one under each ring weapon), from the weapons grid's cell of that item.
			if let Some((n, max)) = ring_ammo(f, s.item) {
				let (w, y) = (SLOT * 0.7, disc.bottom() + 4.0);
				fill(out, Rect::new(sx - w / 2.0, y, w, 3.0), 1.5, BAR_OFF, None);
				fill(out, Rect::new(sx - w / 2.0, y, w * (n as f32 / max.max(1) as f32).clamp(0.0, 1.0), 3.0), 1.5, ACCENT, None);
			}
		}
	}
	let y = RING_AT.1 - ARROW / 2.0;
	let (prev, next) = (Rect::new(DETAIL_PANEL.x + 24.0, y, ARROW, ARROW), Rect::new(DETAIL_PANEL.right() - 24.0 - ARROW, y, ARROW, ARROW));
	for (r, g) in [(prev, "<"), (next, ">")] {
		fill(out, r, ARROW / 2.0, WHITE.tint(0.05), Some((1.0, HAIRLINE)));
		text(out, r.cx(), r.cy() + 1.0, FontRole::Condensed, 26.0, TEXT_BODY, Align::Center, 0.0, g);
	}
	// The middle: the grid's item (the one Equip puts into the lit slot).
	if let Some(c) = f.cell(f.sel.0, f.sel.1).filter(|c| c.owned) {
		text(out, RING_AT.0, RING_AT.1 - 4.0, FontRole::Condensed, 24.0, WHITE, Align::Center, 2.0 * (RING_R - SLOT) - 6.0, &c.name);
		if let Some((n, max)) = c.ammo {
			text(out, RING_AT.0, RING_AT.1 + 24.0, FontRole::SemiBold, 15.0, TEXT_DIM, Align::Center, 0.0, &format!("{n} / {max}"));
		}
	}
	(slots, prev, next)
}

/// The page body. `lang` picks the fallback Equip word.
pub fn build(f: &Facts, lang: usize) -> Built {
	build_with(f, lang, None)
}

/// [b46 W7] B6 rest: `build` with the hint device and glyph set, so the Equip button leads with the Cross glyph / Enter keycap (as the grid pages).
pub fn build_look(f: &Facts, m: &dyn rac_engine::ui::kit::TextMeasure, look: &crate::modern_pages::Look) -> Built {
	build_with(f, look.lang, Some((m, look)))
}

fn build_with(f: &Facts, lang: usize, lead: Option<(&dyn rac_engine::ui::kit::TextMeasure, &crate::modern_pages::Look)>) -> Built {
	let mut out = Vec::new();
	fill(&mut out, GRID_PANEL, 16.0, PANEL, Some((1.0, HAIRLINE)));
	let inner = GRID_PANEL.inset(PAD);
	let title = f.tabs.get(2).cloned().unwrap_or_default().to_uppercase();
	text(&mut out, inner.x, inner.y + 10.0, FontRole::Condensed, 15.0, TEXT_SECTION, Align::Left, 0.0, &title);
	let mut cells = Vec::new();
	let (cw, top) = (inner.w / WEAPON_COLS as f32, inner.y + 34.0);
	for (k, c) in f.weapons.iter().enumerate().filter(|(_, c)| c.item != 0) {
		let r = Rect::new(inner.x + (k % WEAPON_COLS) as f32 * cw, top + (k / WEAPON_COLS) as f32 * CELL_H, cw, CELL_H).inset(3.0);
		draw_cell(&mut out, f, c, r, f.sel == (0, k), 34.0);
		if c.owned { // [b45 P4A] B14: unowned cells are drawn, not live
			cells.push(((0, k), r));
		}
	}
	let (slots, prev, next) = ring(&mut out, f);
	let hin = DETAIL_PANEL.inset(PAD);
	text(&mut out, hin.x, HAND_Y - 12.0, FontRole::Condensed, 14.0, TEXT_SECTION, Align::Left, 0.0, &f.hand_label.to_uppercase());
	let hw = hin.w / HAND_COLS as f32;
	for (k, c) in f.hand.iter().enumerate().filter(|(_, c)| c.item != 0) {
		let r = Rect::new(hin.x + (k % HAND_COLS) as f32 * hw, HAND_Y + (k / HAND_COLS) as f32 * HAND_H, hw, HAND_H).inset(3.0);
		draw_cell(&mut out, f, c, r, f.sel == (1, k), 22.0);
		if c.owned {
			cells.push(((1, k), r));
		}
	}
	let button = f.cell(f.sel.0, f.sel.1).is_some_and(|c| c.owned).then(|| {
		match lead {
			Some((m, look)) => crate::modern_grid_page::draw_button_at(&mut out, m, look, &equip_word(f, lang), BUTTON),
			None => {
				fill(&mut out, BUTTON, 12.0, ACCENT, None);
				text(&mut out, BUTTON.cx(), BUTTON.cy(), FontRole::SemiBold, 17.0, INK_ON_ACCENT, Align::Center, BUTTON.w - 24.0, &equip_word(f, lang));
			}
		}
		BUTTON
	});
	Built { prims: out, cells, slots, prev, next, button }
}
