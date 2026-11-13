//! [PC] The rows of parity wave v0.1 lane hud in R&C2's modern Options page, with R&C1's names and values: HUD position (4:3 / Corners) and
//! HUD scale (hud_layout.rs), Button glyphs (glyph_cells.rs), Skip intro (movie.rs `intro_skipped`; pc_options key `intro=skip`). The
//! Aspect ratio row (Auto, 4:3, 16:9, 16:10, 21:9, 32:9) is `modern_rows::SCREEN`.
use crate::modern_rows::{row, slider, t, CH, NO, TOG};
use rac_engine::pcopts::PcOptions;
use rac_engine::ui::{Cost, T5, UiRow};

const POS_VALUES: &[T5] = &[t("4:3"), ["Corners", "Coins", "Ecken", "Esquinas", "Angoli"]];
const GLYPH_SETS: &[T5] = &[t("PlayStation"), ["Letters", "Lettres", "Buchstaben", "Letras", "Lettere"], ["Letters (Switch)", "Lettres (Switch)", "Buchst. (Switch)", "Letras (Switch)", "Lettere (Switch)"]];

pub const HUDPOS: UiRow = row(
	"hudpos",
	CH,
	["HUD position", "Position du HUD", "HUD-Position", "Posici\u{f3}n del HUD", "Posizione HUD"],
	[
		"4:3 keeps the HUD where the game has it. Corners moves the top boxes (nanotech, bolts and ammo) to the screen corners on wide screens.",
		"4:3 garde le HUD o\u{f9} le jeu le met. Coins place les bo\u{ee}tes du haut (nanotech, boulons, munitions) aux coins des \u{e9}crans larges.",
		"4:3 l\u{e4}sst das HUD, wo das Spiel es hat. Ecken setzt die oberen Felder (Nanotech, Bolzen, Munition) auf Breitbild in die Ecken.",
		"4:3 deja el HUD donde lo pone el juego. Esquinas lleva las cajas de arriba (nanotecnolog\u{ed}a, tornillos y munici\u{f3}n) a las esquinas en pantallas anchas.",
		"4:3 lascia l'HUD dove lo mette il gioco. Angoli sposta i riquadri in alto (nanotech, bulloni e munizioni) negli angoli sugli schermi larghi.",
	],
	POS_VALUES,
	0,
	Cost::None,
	None,
);

pub const HUDSCALE: UiRow = row(
	"hudscale",
	slider(75, 150, 5, "%"),
	["HUD scale", "Taille du HUD", "HUD-Skalierung", "Escala del HUD", "Scala HUD"],
	[
		"Size of the top HUD boxes. 100% is the original size.",
		"Taille des bo\u{ee}tes du haut du HUD. 100 % est la taille d'origine.",
		"Gr\u{f6}\u{df}e der oberen HUD-Felder. 100 % ist die Originalgr\u{f6}\u{df}e.",
		"Tama\u{f1}o de las cajas superiores del HUD. 100 % es el tama\u{f1}o original.",
		"Dimensione dei riquadri in alto dell'HUD. 100% \u{e8} la dimensione originale.",
	],
	NO,
	100,
	Cost::None,
	None,
);

pub const GLYPHS: UiRow = row(
	"glyphs",
	CH,
	["Button glyphs", "Symboles des boutons", "Tastensymbole", "Iconos de botones", "Simboli dei tasti"],
	[
		"The button pictures shown in prompts and menus. Letters shows A B X Y, the Switch set swaps the letters like a Switch pad.",
		"Les images de boutons affich\u{e9}es dans les messages et les menus. Lettres montre A B X Y, le style Switch \u{e9}change les lettres comme une manette Switch.",
		"Die Tastenbilder in Hinweisen und Men\u{fc}s. Buchstaben zeigt A B X Y, der Switch-Satz tauscht die Buchstaben wie ein Switch-Controller.",
		"Las im\u{e1}genes de botones en avisos y men\u{fa}s. Letras muestra A B X Y, el estilo Switch intercambia las letras como un mando de Switch.",
		"Le immagini dei tasti nei messaggi e nei menu. Lettere mostra A B X Y, il set Switch scambia le lettere come un controller Switch.",
	],
	GLYPH_SETS,
	0,
	Cost::None,
	None,
);

pub const INTRO: UiRow = row(
	"intro",
	TOG,
	["Skip intro", "Passer l'intro", "Intro \u{fc}berspringen", "Saltar la intro", "Salta l'intro"],
	[
		"Skips the logos when the game starts.",
		"Passe les logos au d\u{e9}marrage du jeu.",
		"\u{dc}berspringt beim Spielstart die Logos.",
		"Salta los logotipos al iniciar el juego.",
		"Salta i loghi all'avvio del gioco.",
	],
	NO,
	0,
	Cost::None,
	None,
);

/// The row value `id` has in `o` (None = not one of these rows).
pub fn value(o: &PcOptions, id: &str) -> Option<i32> {
	crate::hud_layout::value(o, id).or(match id {
		"glyphs" => Some(crate::glyph_cells::style_of(o) as i32),
		"intro" => Some(o.intro_skip() as i32),
		_ => None,
	})
}

/// Writes row `id` = `v` into `o`; false = not one of these rows.
pub fn apply(o: &mut PcOptions, id: &str, v: i32) -> bool {
	if crate::hud_layout::apply(o, id, v) {
		return true;
	}
	match id {
		"glyphs" => crate::glyph_cells::set_style(o, v),
		"intro" => o.intro = Some(v != 0),
		_ => return false,
	}
	true
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn rows_follow_the_options_and_default_to_retail() {
		let mut o = PcOptions::default();
		for (id, def) in [("hudpos", 0), ("hudscale", 100), ("glyphs", 0), ("intro", 0)] {
			assert_eq!(value(&o, id), Some(def), "{id}");
		}
		assert!(apply(&mut o, "glyphs", 2) && apply(&mut o, "intro", 1));
		assert_eq!((value(&o, "glyphs"), value(&o, "intro")), (Some(2), Some(1)));
		assert!(PcOptions::parse(&o.to_text()).intro_skip(), "intro survives the file");
		assert_eq!(crate::glyph_cells::style_of(&PcOptions::parse(&o.to_text())), 2);
		assert!(!apply(&mut o, "fps", 1));
		assert_eq!([HUDPOS.def, HUDSCALE.def, GLYPHS.def, INTRO.def], [0, 100, 0, 0]);
	}
}
