//! [PC] R&C2's modern Options list, the pure builder (build 45 P4B; game side `modern_optlist.rs`, window side
//! `host/screens_optlist.rs`): R&C1's look of its pause Options tab (rac1/modern_menu/pause.rs `list_parts`: the pause shell with the
//! Options tab selected and the list as a rows panel), at the title the rows panel alone. Retail rows carry the game's own texts; the
//! PC rows R&C1's words (tab names of `modern_menu.rs`, Quit to Desktop and its question of `modern_pages.rs`).
use crate::modern_optlist::{Facts, Pc, Row, TAB};
use crate::modern_pages::{self, word, Look};
use crate::modern_screen::{Facts as ScreenFacts, Kind};
use rac_engine::ui::kit::page::{PageRow, PageScreen, Panel};
use rac_engine::ui::T5;

pub const DISPLAY: T5 = ["Display", "Affichage", "Anzeige", "Pantalla", "Schermo"];
pub const GRAPHICS: T5 = ["Graphics", "Graphismes", "Grafik", "Gr\u{e1}ficos", "Grafica"];
pub const CAMERA: T5 = ["Camera", "Cam\u{e9}ra", "Kamera", "C\u{e1}mara", "Telecamera"];

/// The words of row `r`.
pub fn label(f: &Facts, r: Row, lang: usize) -> String {
	match r {
		Row::Retail(k) => f.retail.get(k).cloned().unwrap_or_default(),
		Row::Pc(Pc::Display) => word(&DISPLAY, lang).to_string(),
		Row::Pc(Pc::Graphics) => word(&GRAPHICS, lang).to_string(),
		Row::Pc(Pc::Camera) => word(&CAMERA, lang).to_string(),
		Row::Pc(Pc::Quit) => word(&modern_pages::QUIT, lang).to_string(),
	}
}

/// The page: pause = the shell with the Options tab, title = no shell; the rows panel; the Quit question over it while asked.
pub fn build(f: &Facts, look: Look) -> PageScreen {
	let rows = match &f.sound {
		Some(sd) => crate::modern_optlist_sound::rows(sd), // [b46 W4] the Sound page
		None => f.rows.iter().enumerate().map(|(i, &r)| PageRow { label: label(f, r, look.lang), selected: i == f.sel, ..Default::default() }).collect(),
	};
	let mut s = match f.kind {
		Kind::Pause => modern_pages::pause(&ScreenFacts { kind: Kind::Pause, cur: TAB, rows: f.tabs.clone(), site: f.site.clone(), bolts: f.bolts }, look),
		Kind::Title => PageScreen { device: look.device, glyphs: look.glyphs, ..Default::default() },
	};
	if f.ask {
		s.menu.iter_mut().for_each(|m| m.live = false);
	}
	s.panel = Panel::Rows { caption: f.caption.clone(), rows };
	s.hints = modern_pages::hints_for(look); // [b46 W4] Back shows Triangle on a pad
	s.dialog = f.ask.then(|| modern_pages::quit_dialog(look.lang));
	if let Some(body) = &f.boxed {
		// [b47 X3] retail's Quit Game box
		s.menu.iter_mut().for_each(|m| m.live = false);
		s.dialog = Some(crate::modern_optlist_quit::dialog(Some(body), look.lang));
	}
	s
}
