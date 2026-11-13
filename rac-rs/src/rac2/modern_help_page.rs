//! [PC] R&C2's modern Help page, a pure builder (lane pages2; game side `modern_help.rs`, window side `host/screens_qs.rs`). R&C1's
//! look (rac1/modern_menu/pause_help.rs): the pause shell with the Help tab selected, the game's own seven Help rows as a list on the
//! left, on the right the row's name, the game's own one-line description and the picture retail draws for the row: the row's icon,
//! cropped out of the retail overlay picture into a hole of the detail panel (the host draws the crop). Words are the game's own texts.
use crate::modern_grid_page::{contain, holed_panel, uv_of, DETAIL_PANEL, GRID_PANEL};
use crate::modern_help::Facts;
use crate::modern_pages::{self, Look};
use crate::modern_screen::{Facts as ScreenFacts, Kind};
use rac_engine::ui::kit::data::{Hint, HintKind};
use rac_engine::ui::kit::page::PageScreen;
use rac_engine::ui::kit::TextMeasure;
use rac_engine::ui::prim::{Align, Border, FontRole, Prim, Rect, Rgba, TextRun};
use rac_engine::ui::theme::*;

const PAD: f32 = 20.0;
const ROW_H: f32 = 58.0;
/// The picture hole's box in the detail panel; the icon is fitted inside it.
const VIEW: Rect = Rect::new(672.0, 140.0, 120.0, 120.0);
/// A row's icon in the 512 x 448 overlay picture: x, y of row 0's top, the row pitch, w, h [C measured on the classic page,
/// 1600 x 900 window: icon plates x 695..830, row centres y 170 + 71.8 k].
const ICON: [f32; 5] = [229.0, 68.0, 35.8, 31.0, 32.0];
const DESC_LINES: usize = 7;

/// The overlay crop (x, y, w, h) of row `k`'s icon.
pub fn icon_src(k: usize) -> [f32; 4] {
	[ICON[0], ICON[1] + ICON[2] * k as f32, ICON[3], ICON[4]]
}

/// Where a hover or click lands, and the preview hole.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Built {
	pub prims: Vec<Prim>,
	pub rows: Vec<Rect>,
	pub hole: Rect,
	pub uv: [f32; 4],
}

impl Built {
	pub fn row_at(&self, x: f32, y: f32) -> Option<usize> {
		self.rows.iter().position(|r| r.contains(x, y))
	}
}

/// The pause shell with the Help tab selected; hints Open and Back.
pub fn shell(f: &Facts, look: Look) -> PageScreen {
	let sf = ScreenFacts { kind: Kind::Pause, cur: 4, rows: f.tabs.clone(), site: f.site.clone(), bolts: f.bolts };
	let mut s = modern_pages::pause(&sf, look);
	s.hints = vec![Hint { kind: HintKind::Change, text: modern_pages::word(&OPEN, look.lang).to_string() }, Hint { kind: modern_pages::back_kind(look.device), text: modern_pages::word(&modern_pages::BACK, look.lang).to_string() }]; // [b46 U6]
	s
}

pub const OPEN: [&str; 5] = ["Select", "Choisir", "Ausw\u{e4}hlen", "Elegir", "Seleziona"];

#[allow(clippy::too_many_arguments)]
fn text(out: &mut Vec<Prim>, x: f32, y: f32, role: FontRole, px: f32, colour: Rgba, align: Align, max_w: f32, s: &str) {
	out.push(Prim::Text(TextRun { x, y, role, px, colour, align, max_w, track: 0.0, text: s.to_string() }));
}

fn fill(out: &mut Vec<Prim>, rect: Rect, radius: f32, fill: Rgba, border: Option<(f32, Rgba)>) {
	out.push(Prim::RRect { rect, radius, fill, border: border.map(|(width, colour)| Border { width, colour }) });
}

/// The page body. `aspect` = the game picture's aspect (the overlay fills the picture rect, so a crop of w x h overlay pixels
/// shows with aspect (w * aspect) : (h * 512 / 448)).
pub fn build(f: &Facts, m: &dyn TextMeasure, aspect: f32) -> Built {
	let mut out = Vec::new();
	fill(&mut out, GRID_PANEL, 16.0, PANEL, Some((1.0, HAIRLINE)));
	let inner = GRID_PANEL.inset(PAD);
	let title = f.tabs.get(4).cloned().unwrap_or_default().to_uppercase();
	text(&mut out, inner.x, inner.y + 10.0, FontRole::Condensed, 15.0, TEXT_SECTION, Align::Left, 0.0, &title);
	let mut rows = Vec::new();
	for (i, label) in f.rows.iter().enumerate() {
		let r = Rect::new(inner.x, inner.y + 34.0 + i as f32 * (ROW_H + 4.0), inner.w, ROW_H);
		if i == f.cur {
			fill(&mut out, r, 12.0, ACCENT.tint(0.13), Some((1.5, ACCENT.tint(0.6))));
		}
		text(&mut out, r.x + 20.0, r.cy(), FontRole::Medium, 20.0, if i == f.cur { WHITE } else { TEXT_BODY }, Align::Left, r.w - 40.0, label);
		rows.push(r);
	}
	let src = icon_src(f.cur);
	let hole = contain(VIEW, src[2] * aspect, src[3] * 512.0 / 448.0);
	out.extend(holed_panel(DETAIL_PANEL, hole, 16.0, PANEL, Border { width: 1.0, colour: HAIRLINE }));
	let x = VIEW.right() + 24.0;
	let w = DETAIL_PANEL.right() - PAD - x;
	text(&mut out, x, VIEW.cy(), FontRole::Condensed, 38.0, WHITE, Align::Left, w, f.rows.get(f.cur).map_or("", |s| s));
	if let Some(desc) = f.descs.get(f.cur).filter(|d| !d.is_empty()) {
		let dw = DETAIL_PANEL.w - 2.0 * PAD;
		for (i, line) in rac_engine::ui::kit::text::wrap(m, FontRole::Regular, 17.0, desc, dw).iter().take(DESC_LINES).enumerate() {
			text(&mut out, VIEW.x, VIEW.bottom() + 40.0 + i as f32 * 26.0, FontRole::Regular, 17.0, TEXT_BODY, Align::Left, dw, line);
		}
	}
	fill(&mut out, hole, 0.0, WHITE.tint(0.0), Some((1.0, HAIRLINE)));
	Built { prims: out, rows, hole, uv: uv_of(src) }
}
