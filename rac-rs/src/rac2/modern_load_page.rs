//! [PC] R&C2's modern save-slot pages, a pure builder (lane ld; game side `modern_load.rs`, window side `host/screens_load.rs`).
//! R&C1's approved title slot cards (rac1/modern_menu/title.rs `cards`): at the title the left column of root buttons with the
//! page's entry selected (inert), in the pause the shell with the Options tab; the cards panel (one card per save: "N  Planet",
//! Bolts / Play time / Saved, an empty slot dimmed with the game's "EMPTY"; the title's Save Game page adds retail's "Continue
//! without saving" row) and, under it, the selected save's picture: ONE crop of the retail overlay (the level picture retail draws
//! for its cursor's slot) shown through a hole in the scrim, with the planet and site names beside it. Words are the game's texts
//! plus R&C1's PC words for the stat labels and hints.
use crate::modern_grid_page::{fill, text, uv_of};
use crate::modern_load::{Facts, Mode, Question, OPTIONS_TAB};
use crate::modern_pages::{self, Look, BACK, NO, QUIT, SELECT, YES};
use crate::modern_screen::{Facts as ScreenFacts, Kind};
use rac_engine::ui::keycaps::{self, Cap};
use rac_engine::ui::kit::data::{Hint, HintKind};
use rac_engine::ui::kit::page::{Dialog, DialogButton, MenuButton, PageRow, PageScreen, Panel, Stat, HINT_Y};
use rac_engine::ui::prim::{Align, FontRole, Prim, Rect, Rgba, Symbol};
use rac_engine::ui::theme::*;
use rac_engine::ui::T5;

// R&C1's words (rac1/modern_menu/title.rs).
pub const BOLTS: T5 = ["Bolts", "Boulons", "Bolzen", "Tuercas", "Bulloni"];
pub const PLAY_TIME: T5 = ["Play time", "Temps de jeu", "Spielzeit", "Tiempo de juego", "Tempo di gioco"];
pub const SAVED: T5 = ["Saved", "Sauvegard\u{e9}", "Gespeichert", "Guardado", "Salvato"];

/// The selected save's picture in the 512 x 448 retail overlay (retail window x 345..957, y 192..365 of 1024 x 768 [C ex1
/// w00700], one pixel inside its frame), and the aspect it is shown with: as on a 4:3 TV.
pub const PICTURE: [f32; 4] = [173.0, 113.0, 305.0, 99.0];
const TV_ASPECT: f32 = 4.0 / 3.0;
const OV_W: f32 = 512.0;
const OV_H: f32 = 448.0;
const GAP: f32 = 16.0;
/// The tallest the picture gets, and the room kept above the hint bar.
const PIC_H: f32 = 150.0;
const ABOVE_HINTS: f32 = 22.0;

/// The page drawn: the kit page and how many cards it has (the extra row, when there, is item `cards`).
#[derive(Clone, Debug, PartialEq)]
pub struct Built {
	pub screen: PageScreen,
	pub cards: usize,
	pub extra: bool,
}

fn word(t: &T5, lang: usize) -> &'static str {
	modern_pages::word(t, lang)
}

fn cards(f: &Facts, sel: usize, lang: usize) -> Vec<PageRow> {
	let st = |l: &T5, v: &str| Stat { label: word(l, lang).to_string(), value: v.to_string() };
	f.slots
		.iter()
		.enumerate()
		.map(|(i, s)| match s {
			Some(s) => PageRow { label: format!("{}  {}", i + 1, s.planet), stats: vec![st(&BOLTS, &s.bolts), st(&PLAY_TIME, &s.time), st(&SAVED, &s.date)], selected: i == sel, ..Default::default() },
			None => PageRow { label: format!("{}  {}", i + 1, f.empty), selected: i == sel, dim: true, ..Default::default() },
		})
		.collect()
}

/// Retail's message box as the kit's dialog: the game's lines; a question gets Circle = Yes (Enter) and Triangle = No (Esc), as
/// R&C1's Overwrite page; "Saving ..." no buttons.
pub fn dialog(q: &Question, lang: usize) -> Dialog {
	let b = |symbol, cap, t: &T5| DialogButton { symbol, key: keycaps::name(cap).to_string(), text: word(t, lang).to_string() };
	let buttons = if q.yes_no { vec![b(Symbol::Circle, Cap::Enter, &YES), b(Symbol::Triangle, Cap::Esc, &NO)] } else { Vec::new() };
	Dialog { lines: q.lines.clone(), buttons }
}

/// The page with card `sel` selected (`modern_load::shown`) and retail's box, when up, as a dialog over it.
pub fn build(f: &Facts, sel: usize, look: Look) -> Built {
	let extra = f.no_save.as_ref().map(|t| PageRow { label: t.clone(), ..Default::default() });
	let panel = Panel::Cards { caption: f.caption.clone(), side: String::new(), cards: cards(f, sel, look.lang), extra: extra.clone() };
	let hints = vec![Hint { kind: HintKind::Change, text: word(&SELECT, look.lang).to_string() }, Hint { kind: crate::modern_pages::back_kind(look.device), text: word(&BACK, look.lang).to_string() }]; // [b46 U6]
	let screen = if f.title {
		// The title root's buttons with the page's entry selected and inert (R&C1 title.rs `build`: New Game for Save, Load).
		let entry = if f.mode == Mode::Save { 0 } else { 1 };
		let mut menu: Vec<MenuButton> = f.rows.iter().enumerate().map(|(i, t)| MenuButton { text: t.clone(), selected: i == entry, live: false }).collect();
		menu.push(MenuButton { text: word(&QUIT, look.lang).to_string(), selected: false, live: false });
		PageScreen { menu, panel, hints, device: look.device, glyphs: look.glyphs, ..Default::default() }
	} else {
		let sf = ScreenFacts { kind: Kind::Pause, cur: OPTIONS_TAB, rows: f.rows.clone(), site: f.site.clone(), bolts: f.bolts };
		let mut s = modern_pages::pause(&sf, look);
		s.menu.iter_mut().for_each(|m| m.live = false);
		s.panel = panel;
		s.hints = hints;
		s
	};
	let screen = PageScreen { dialog: f.ask.as_ref().map(|q| dialog(q, look.lang)), ..screen };
	Built { screen, cards: f.slots.len(), extra: extra.is_some() }
}

/// Where the picture goes under the cards panel `panel` (design space): left-aligned with the panel, as tall as the room above the
/// hint bar allows (at most `PIC_H`), with the crop's aspect on a 4:3 TV.
pub fn hole(panel: Rect) -> Rect {
	let aspect = (PICTURE[2] * TV_ASPECT) / (PICTURE[3] * OV_W / OV_H);
	let top = panel.bottom() + GAP;
	let h = (HINT_Y - ABOVE_HINTS - top).min(PIC_H).min(panel.w * 0.6 / aspect).max(0.0);
	Rect::new(panel.x, top, h * aspect, h)
}

/// The overlay crop the hole shows (u0, v0, u1, v1).
pub fn uv() -> [f32; 4] {
	uv_of(PICTURE)
}

/// The picture of retail's cursor slot (None: no panel, an empty slot, no room, retail's box up: it covers that spot of the
/// retail picture, or retail draws no picture this field: the switch field or its picture not loaded yet, `Facts::pic`): the hole
/// and the prims around it (a thin frame, the planet and the site beside it).
pub fn picture(f: &Facts, panel: Option<Rect>) -> Option<(Rect, Vec<Prim>)> {
	if f.ask.is_some() || !f.pic {
		return None;
	}
	let s = f.slots.get(f.cur)?.as_ref()?;
	let h = hole(panel?);
	if h.h < 24.0 {
		return None;
	}
	let mut out = Vec::new();
	fill(&mut out, Rect::new(h.x - 2.0, h.y - 2.0, h.w + 4.0, h.h + 4.0), 6.0, Rgba::rgba(0, 0, 0, 0.0), Some((1.0, PILL_BORDER)));
	let x = h.right() + 24.0;
	text(&mut out, x, h.y + h.h * 0.5 - 14.0, FontRole::SemiBold, 22.0, TEXT, Align::Left, 0.0, &s.planet);
	text(&mut out, x, h.y + h.h * 0.5 + 14.0, FontRole::Regular, 17.0, TEXT_MUTED, Align::Left, 0.0, &s.site);
	Some((h, out))
}

/// `r` minus `hole`: up to four rects.
pub fn minus(r: Rect, hole: Rect) -> Vec<Rect> {
	let (x0, x1) = (r.x.max(hole.x), r.right().min(hole.right()));
	let (y0, y1) = (r.y.max(hole.y), r.bottom().min(hole.bottom()));
	if x0 >= x1 || y0 >= y1 {
		return vec![r];
	}
	[Rect::new(r.x, r.y, r.w, y0 - r.y), Rect::new(r.x, y1, r.w, r.bottom() - y1), Rect::new(r.x, y0, x0 - r.x, y1 - y0), Rect::new(x1, y0, r.right() - x1, y1 - y0)]
		.into_iter()
		.filter(|p| p.w > 0.0 && p.h > 0.0)
		.collect()
}

/// A full-screen scrim of the kit (the title's left gradient strips, the pause shell's veil): colour (5, 8, 15), 10000 units tall.
fn is_scrim(rect: &Rect, fill: &Rgba) -> bool {
	rect.h >= 9000.0 && (fill.r, fill.g, fill.b) == (5, 8, 15)
}

/// The page's draw list with the picture's hole cut out of every scrim (the retail crop drawn under the UI shows through
/// undarkened) and the picture's own prims added.
/// Retail's fade to black over the page (`Facts::fade`; 0 = none): one black rect over everything, as R&C1's title page draws the
/// front end's fade (front_modern.rs).
pub fn with_fade(mut prims: Vec<Prim>, fade: f32) -> Vec<Prim> {
	if fade > 0.0 {
		prims.push(Prim::RRect { rect: Rect::new(-5000.0, -5000.0, 10000.0, 10000.0), radius: 0.0, fill: Rgba::rgba(0, 0, 0, fade.min(1.0)), border: None });
	}
	prims
}

pub fn with_picture(prims: Vec<Prim>, pic: Option<&(Rect, Vec<Prim>)>) -> Vec<Prim> {
	let Some((hole, extra)) = pic else { return prims };
	let mut out = Vec::with_capacity(prims.len() + extra.len() + 16);
	for p in prims {
		match p {
			Prim::RRect { rect, radius, fill, border } if is_scrim(&rect, &fill) => {
				out.extend(minus(rect, *hole).into_iter().map(|rect| Prim::RRect { rect, radius, fill, border: border.clone() }));
			}
			p => out.push(p),
		}
	}
	out.extend(extra.iter().cloned());
	out
}
