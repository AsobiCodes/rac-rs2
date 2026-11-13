//! [PC] R&C2's modern pause and title pages, pure builders (lane scr): `Facts` (modern_screen.rs) -> the engine kit's `PageScreen`,
//! the same look as R&C1's (`rac1/modern_menu/pause.rs` root page: the pause shell with the tab bar, "PAUSED", the planet and the
//! bolts chip; `title.rs` root page: the left column of big buttons plus the host's "Quit to Desktop" and its question). The rows
//! are the game's own texts; the words below are R&C1's PC words (not retail text, same native-speaker review note).
//! Only the two ROOT pages are modern: every sub page (Weapons, Gadgets, Quick Select, Items, Help, Options, Special; New Game,
//! Load, title Options) stays the game's classic picture (handoff.md lists the facts each one needs).
use crate::modern_screen::{Facts, Kind, Quit, QUIT_ROW};
use rac_engine::ui::input::{Device, GlyphSet};
use rac_engine::ui::keycaps::{self, Cap};
use rac_engine::ui::kit::data::{Hint, HintKind};
use rac_engine::ui::kit::page::{Dialog, DialogButton, MenuButton, PageScreen, Panel};
use rac_engine::ui::kit::shell::Shell;
use rac_engine::ui::prim::Symbol;
use rac_engine::ui::T5;

// R&C1's words (rac1/modern_menu/pause.rs PAUSED / SITE, title.rs SELECT / BACK, pcext/texts.rs Quit to Desktop).
pub const PAUSED: T5 = ["Paused", "Pause", "Pause", "Pausa", "Pausa"];
pub const SITE: T5 = ["Current site", "Site actuel", "Aktueller Ort", "Sitio actual", "Sito attuale"];
pub const SELECT: T5 = ["Select", "Choisir", "W\u{e4}hlen", "Elegir", "Scegli"];
pub const BACK: T5 = ["Back", "Retour", "Zur\u{fc}ck", "Volver", "Indietro"];
pub const QUIT: T5 = ["Quit to Desktop", "Quitter vers le bureau", "Zum Desktop beenden", "Salir al escritorio", "Esci sul desktop"];
pub const QUIT_ASK: T5 = [
	"Quit to the desktop? Unsaved progress is lost.",
	"Retour au bureau ? Progression non sauvegard\u{e9}e perdue.",
	"Zum Desktop beenden? Nicht Gespeichertes geht verloren.",
	"Salir al escritorio? Se perder\u{e1} el progreso sin guardar.",
	"Uscire al desktop? I progressi non salvati andranno persi.",
];
// R&C1 takes Yes / No from its own text table (0x524f / 0x524b); R&C2's ids for them are not known [?]: PC words.
pub const YES: T5 = ["Yes", "Oui", "Ja", "S\u{ed}", "S\u{ec}"];
pub const NO: T5 = ["No", "Non", "Nein", "No", "No"];

pub fn word(t: &T5, lang: usize) -> &'static str {
	t[lang.min(4)]
}

/// How the page is seen: language, hint device, glyph set.
#[derive(Clone, Copy, Debug, Default)]
pub struct Look {
	pub lang: usize,
	pub device: Device,
	pub glyphs: GlyphSet,
}

/// The hint kind that shows the button retail's lists back out with [b46 W4]: Triangle (0x10 [C notes_menus]), not the kit's Back
/// glyph (Circle on a pad). On a pad the kit's Peek kind draws Triangle; the keyboard keeps Back (Esc). ENGINE NEEDED: a Back kind with a
/// Triangle lead in `engine/ui/kit/layout.rs leads`, then this goes back to HintKind::Back.
pub fn back_kind(device: Device) -> HintKind {
	if matches!(device, Device::Pad) {
		HintKind::Peek
	} else {
		HintKind::Back
	}
}

/// Select + Back hints for `look` (the Back lead is the true button, see `back_kind`).
pub fn hints_for(look: Look) -> Vec<Hint> {
	vec![Hint { kind: HintKind::Change, text: word(&SELECT, look.lang).to_string() }, Hint { kind: back_kind(look.device), text: word(&BACK, look.lang).to_string() }]
}

/// The pause root: the shell (label, planet, bolts), one tab per root row, the game's cursor row selected, no panel.
pub fn pause(f: &Facts, look: Look) -> PageScreen {
	let menu = f.rows.iter().enumerate().map(|(i, t)| MenuButton { text: t.clone(), selected: i == f.cur, live: true }).collect();
	let shell = Shell { label: word(&PAUSED, look.lang).to_string(), site_label: word(&SITE, look.lang).to_string(), site: if f.site.is_empty() { "?".into() } else { f.site.clone() }, bolts: f.bolts.to_string() };
	PageScreen { menu, panel: Panel::None, hints: hints_for(look), device: look.device, glyphs: look.glyphs, shell: Some(shell), ..Default::default() }
}

/// The question's two answers: Circle = Yes (Enter), Triangle = No (Esc), as R&C1's title.
pub fn quit_dialog(lang: usize) -> Dialog {
	let b = |symbol, cap, t: &T5| DialogButton { symbol, key: keycaps::name(cap).to_string(), text: word(t, lang).to_string() };
	Dialog { lines: vec![word(&QUIT_ASK, lang).to_string()], buttons: vec![b(Symbol::Circle, Cap::Enter, &YES), b(Symbol::Triangle, Cap::Esc, &NO)] }
}

/// The title root: New Game, Load, Options (the game's texts) and Quit to Desktop; the question over it while asked.
pub fn title(f: &Facts, q: Quit, look: Look) -> PageScreen {
	let sel = if q.focus || q.ask { QUIT_ROW } else { f.cur };
	let live = !q.ask;
	let mut menu: Vec<MenuButton> = f.rows.iter().enumerate().map(|(i, t)| MenuButton { text: t.clone(), selected: i == sel, live }).collect();
	menu.push(MenuButton { text: word(&QUIT, look.lang).to_string(), selected: sel == QUIT_ROW, live });
	PageScreen { menu, panel: Panel::None, dialog: q.ask.then(|| quit_dialog(look.lang)), hints: hints_for(look), device: look.device, glyphs: look.glyphs, ..Default::default() }
}

pub fn build(f: &Facts, q: Quit, look: Look) -> PageScreen {
	match f.kind {
		Kind::Pause => pause(f, look),
		Kind::Title => title(f, q, look),
	}
}
