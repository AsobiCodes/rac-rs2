//! [PC] build 45 P2: the Display and Graphics rows R&C2 lacked (sweep A7, A8, A9, E M2, E M3), R&C1's rows and texts
//! (rac1/modern_menu/rows_picture.rs, rows_display.rs, rows_system.rs, rows_fgen.rs, rows_upscale.rs): Resolution and Display mode (live
//! window side: host/display_res.rs), Brightness and Contrast (engine picture_adjust), Renderer (key `renderer`, next start) and
//! Frame Generation (engine fgen_choice / fgen_follow; follows the Upscaler). The engine statics are set by options.rs `apply`.
use crate::modern_rows::{row, slider, t, CH, NO};
use rac_engine::ui::{Cost, Needs, NeedsRule, UiRow, T5};

const WINDOWED: &[T5] = &[["Windowed", "Fen\u{ea}tr\u{e9}", "Fenster", "Ventana", "Finestra"], ["Fullscreen", "Plein \u{e9}cran", "Vollbild", "Pantalla completa", "Schermo intero"]];
const RENDERERS: &[T5] = &[t("Vulkan"), t("DirectX 12")];
/// Frame Generation's values in `fgen_choice::KINDS` order.
pub const FG_VALUES: &[T5] = &[crate::modern_rows::OFF, t("FSR 2x"), t("DLSS 2x"), t("DLSS 3x"), t("DLSS 4x")];

/// Desktop, then `resolution::SLOTS - 1` places for the monitor's sizes (their static name is never shown: the engine names or
/// withdraws every one of them at start-up).
const RES_VALUES: &[rac_engine::ui::T5] = &[
	["Desktop", "Bureau", "Desktop", "Escritorio", "Desktop"],
	t("-"), t("-"), t("-"), t("-"), t("-"), t("-"), t("-"), t("-"), t("-"), t("-"), t("-"), t("-"),
	t("-"), t("-"), t("-"), t("-"), t("-"), t("-"), t("-"), t("-"), t("-"), t("-"), t("-"),
];

pub const RESOLUTION: UiRow = row(
	"resolution",
	CH,
	["Resolution", "R\u{e9}solution", "Aufl\u{f6}sung", "Resoluci\u{f3}n", "Risoluzione"],
	[
		"Desktop: in a window, the window keeps the size you give it; in fullscreen, the screen's own size. In a window, another size sets the window to it (a size bigger than the screen's free area acts as Desktop). In fullscreen the game is drawn at that size and scaled to the screen; Frame Generation pauses then.",
		"Bureau : en fen\u{ea}tre, la fen\u{ea}tre garde la taille que vous lui donnez ; en plein \u{e9}cran, la taille de l'\u{e9}cran. En fen\u{ea}tre, une autre taille r\u{e8}gle la fen\u{ea}tre (une taille plus grande que l'espace libre de l'\u{e9}cran agit comme Bureau). En plein \u{e9}cran, le jeu est dessin\u{e9} \u{e0} cette taille puis ajust\u{e9} \u{e0} l'\u{e9}cran ; la G\u{e9}n\u{e9}ration d'images est alors en pause.",
		"Desktop: im Fenster beh\u{e4}lt das Fenster die Gr\u{f6}\u{df}e, die du ihm gibst; im Vollbild die Gr\u{f6}\u{df}e des Bildschirms. Im Fenster stellt eine andere Gr\u{f6}\u{df}e das Fenster darauf ein (eine Gr\u{f6}\u{df}e \u{fc}ber der freien Bildschirmfl\u{e4}che wirkt wie Desktop). Im Vollbild wird das Spiel in dieser Gr\u{f6}\u{df}e gezeichnet und auf den Bildschirm skaliert; die Frame-Generierung pausiert dann.",
		"Escritorio: en ventana, la ventana mantiene el tama\u{f1}o que le des; en pantalla completa, el tama\u{f1}o de la pantalla. En ventana, otro tama\u{f1}o ajusta la ventana a \u{e9}l (un tama\u{f1}o mayor que el \u{e1}rea libre de la pantalla act\u{fa}a como Escritorio). En pantalla completa el juego se dibuja a ese tama\u{f1}o y se escala a la pantalla; la Generaci\u{f3}n de fotogramas queda en pausa.",
		"Desktop: in finestra, la finestra mantiene la dimensione che le dai; a schermo intero, la dimensione dello schermo. In finestra, un'altra dimensione imposta la finestra (una dimensione pi\u{f9} grande dell'area libera dello schermo vale come Desktop). A schermo intero il gioco \u{e8} disegnato a quella dimensione e scalato sullo schermo; la Generazione di frame \u{e8} in pausa.",
	],
	RES_VALUES,
	0,
	Cost::None,
	None,
);

pub static PICTURE: [UiRow; 2] = [
	row(
		"brightness",
		slider(0, 100, 5, ""),
		["Brightness", "Luminosit\u{e9}", "Helligkeit", "Brillo", "Luminosit\u{e0}"],
		[
			"Lifts or darkens the shadows and mid-tones of the game picture. Black and white stay as they are; the HUD and menus are not changed. 50 is the original picture.",
			"\u{c9}claircit ou assombrit les ombres et les tons moyens de l'image du jeu. Le noir et le blanc restent tels quels; le HUD et les menus ne changent pas. 50 est l'image d'origine.",
			"Hellt die Schatten und Mittelt\u{f6}ne des Spielbilds auf oder dunkelt sie ab. Schwarz und Wei\u{df} bleiben; HUD und Men\u{fc}s bleiben unver\u{e4}ndert. 50 ist das Originalbild.",
			"Aclara u oscurece las sombras y los tonos medios de la imagen del juego. El negro y el blanco no cambian; el HUD y los men\u{fa}s tampoco. 50 es la imagen original.",
			"Schiarisce o scurisce le ombre e i mezzitoni dell'immagine di gioco. Il nero e il bianco restano uguali; HUD e menu non cambiano. 50 \u{e8} l'immagine originale.",
		],
		NO,
		50,
		Cost::None,
		None,
	),
	row(
		"contrast",
		slider(0, 100, 5, ""),
		["Contrast", "Contraste", "Kontrast", "Contraste", "Contrasto"],
		[
			"Spreads the game picture's tones apart (above 50) or brings them closer to grey (below 50). The HUD and menus are not changed. 50 is the original picture.",
			"\u{c9}carte les tons de l'image du jeu (au-dessus de 50) ou les rapproche du gris (en dessous). Le HUD et les menus ne changent pas. 50 est l'image d'origine.",
			"Zieht die T\u{f6}ne des Spielbilds auseinander (\u{fc}ber 50) oder r\u{fc}ckt sie n\u{e4}her an Grau (unter 50). HUD und Men\u{fc}s bleiben unver\u{e4}ndert. 50 ist das Originalbild.",
			"Separa los tonos de la imagen del juego (por encima de 50) o los acerca al gris (por debajo). El HUD y los men\u{fa}s no cambian. 50 es la imagen original.",
			"Allontana i toni dell'immagine di gioco (sopra 50) o li avvicina al grigio (sotto 50). HUD e menu non cambiano. 50 \u{e8} l'immagine originale.",
		],
		NO,
		50,
		Cost::None,
		None,
	),
];


/// Display mode (Screen section, first row).
pub const DISPLAYMODE: UiRow = row(
		"displaymode",
		CH,
		["Display mode", "Mode d'affichage", "Anzeigemodus", "Modo de pantalla", "Modalit\u{e0} schermo"],
		[
			"Play in a window or on the whole screen.",
			"Jouer dans une fen\u{ea}tre ou sur tout l'\u{e9}cran.",
			"Im Fenster oder auf dem ganzen Bildschirm spielen.",
			"Juega en una ventana o en toda la pantalla.",
			"Gioca in una finestra o a schermo intero.",
		],
		WINDOWED,
		0,
		Cost::None,
		None,
	);

/// The Renderer section (Display tab). The change is stored at once and used from the next start.
pub static RENDERER: [UiRow; 1] = [UiRow {
	note: Some([
		"Applies after a restart",
		"S'applique apr\u{e8}s un red\u{e9}marrage",
		"Gilt nach einem Neustart",
		"Se aplica al reiniciar",
		"Si applica dopo il riavvio",
	]),
	..row(
		"renderer",
		CH,
		["Renderer", "Rendu", "Renderer", "Motor gr\u{e1}fico", "Renderer"],
		[
			"The graphics system the game draws with. Vulkan is the default. Try DirectX 12 if the game does not start or stutters.",
			"Le syst\u{e8}me graphique utilis\u{e9} par le jeu. Vulkan est celui par d\u{e9}faut. Essayez DirectX 12 si le jeu ne d\u{e9}marre pas ou saccade.",
			"Das Grafiksystem, mit dem das Spiel zeichnet. Vulkan ist der Standard. Probiere DirectX 12, wenn das Spiel nicht startet oder ruckelt.",
			"El sistema gr\u{e1}fico con el que dibuja el juego. Vulkan es el normal. Prueba DirectX 12 si el juego no arranca o va a tirones.",
			"Il sistema grafico con cui disegna il gioco. Vulkan \u{e8} quello normale. Prova DirectX 12 se il gioco non parte o scatta.",
		],
		RENDERERS,
		0,
		Cost::None,
		None,
	)
}];


/// Hidden while the Upscaler is Off (its rule: `NEEDS_RULE`).
pub const NEEDS: Needs = Needs { row: "upscaler", min: 1, max: i32::MAX };

/// The row's box while the Upscaler is Off (review 2): the button sets the Upscaler of the frame generation's maker (engine
/// follow.rs `fix_upscaler`: DLSS for DLSS 2x..4x, FSR for FSR 2x; nothing chosen: the maker this run started, else the first
/// upscaler this PC runs), and names it (`{}`), so the chosen frame generation comes back.
pub const NEEDS_RULE: NeedsRule = NeedsRule {
	row: "upscaler",
	set_to: rac_engine::viewer::upscale_choice::FFX_VALUE,
	for_row: "framegen",
	fix: Some(rac_engine::viewer::fgen_follow::fix_upscaler),
	banner: [""; 5],
	detail: [
		"Needs the Upscaler of its maker. Upscaler is {} now.",
		"Exige la mise \u{e0} l'\u{e9}chelle de son fabricant. Elle est sur {}.",
		"Braucht die Hochskalierung des Herstellers. Sie steht auf {}.",
		"Necesita el escalado de su fabricante. Ahora est\u{e1} en {}.",
		"Richiede l'upscaling del suo produttore. Ora \u{e8} su {}.",
	],
	// `{}` = the Upscaler value the button sets (engine model_edit.rs `fix_text`).
	button: [
		"Switch the Upscaler to {}",
		"Passer la mise \u{e0} l'\u{e9}chelle sur {}",
		"Hochskalierung auf {} stellen",
		"Cambiar el escalado a {}",
		"Imposta l'upscaling su {}",
	],
};

/// What it does, that it follows the Upscaler, the pause under black bars.
pub const DESC: T5 = [
	"Adds generated frames for smoother motion; a little more input lag. Follows the Upscaler (DLSS or FSR). Paused with black bars (Aspect ratio) or a non-Desktop fullscreen Resolution.",
	"Ajoute des images g\u{e9}n\u{e9}r\u{e9}es, plus fluide ; l\u{e9}ger retard. Suit la mise \u{e0} l'\u{e9}chelle (DLSS ou FSR). En pause avec bandes noires (Format d'image) ou plein \u{e9}cran hors Bureau.",
	"Mehr Bilder, fl\u{fc}ssiger; etwas Verz\u{f6}gerung. Folgt der Hochskalierung (DLSS oder FSR). Pause bei schwarzen Balken oder Vollbild au\u{df}er Desktop.",
	"M\u{e1}s fluidez, algo de retraso. Sigue al escalado (DLSS o FSR). Pausa con franjas negras o pantalla completa fuera de Escritorio.",
	"Frame generati: pi\u{f9} fluido, un po' di ritardo. Segue l'upscaling (DLSS o FSR). In pausa con bande nere (Proporzioni) o schermo intero non Desktop.",
];


/// Frame Generation (Graphics -> Resolution section, after Upscale quality).
pub const FRAMEGEN: UiRow = UiRow {
		note: Some([
			"Applies after a restart",
			"S'applique apr\u{e8}s un red\u{e9}marrage",
			"Gilt nach einem Neustart",
			"Se aplica al reiniciar",
			"Si applica dopo il riavvio",
		]),
		..row(
			"framegen",
			CH,
			["Frame Generation", "G\u{e9}n\u{e9}ration d'images", "Frame-Generierung", "Generaci\u{f3}n de fotogramas", "Generazione di frame"],
			DESC, // lane fglive: follows the Upscaler (rows_fgen.rs)
			FG_VALUES,
			0,
			Cost::None,
			Some(NEEDS), // lane fglive: hidden while the Upscaler is Off
		)
	};

/// `get` of these rows (modern_menu.rs `value`); None = not one of them.
pub fn value(o: &rac_engine::pcopts::PcOptions, id: &str) -> Option<i32> {
	use rac_engine::viewer::{fgen_follow, picture_adjust, resolution};
	Some(match id {
		"brightness" => picture_adjust::brightness() as i32,
		"contrast" => picture_adjust::contrast() as i32,
		"resolution" => resolution::index(),
		"displaymode" => o.fullscreen_on() as i32,
		"renderer" => o.renderer.unwrap_or(false) as i32,
		"framegen" => fgen_follow::shown_now() as i32,
		_ => return None,
	})
}

/// `set` of these rows (engine setters as R&C1's ui_bridge_more.rs); false = not one of them.
pub fn apply(o: &mut rac_engine::pcopts::PcOptions, id: &str, v: i32) -> bool {
	use rac_engine::viewer::{fgen_choice, picture_adjust, resolution};
	match id {
		"brightness" | "contrast" => {
			picture_adjust::set_from_ui(o, id == "contrast", v);
		}
		"resolution" => {
			resolution::set_from_ui(o, v);
		}
		"displaymode" => o.fullscreen = Some(v == 1),
		"renderer" => o.renderer = Some(v != 0),
		"framegen" => {
			fgen_choice::set(o, v);
		}
		_ => return false,
	}
	true
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn one_value_per_engine_slot_and_rows_are_r_and_c_ones() {
		assert_eq!(RES_VALUES.len(), rac_engine::viewer::resolution::SLOTS);
		assert_eq!(FG_VALUES.len(), rac_engine::viewer::fgen_choice::KINDS.len());
		assert_eq!(PICTURE.each_ref().map(|r| r.id), ["brightness", "contrast"]);
		assert_eq!((DISPLAYMODE.id, RESOLUTION.id, RENDERER[0].id, FRAMEGEN.id), ("displaymode", "resolution", "renderer", "framegen"));
		assert_eq!((PICTURE[0].def, PICTURE[1].def), (50, 50));
		let mut o = rac_engine::pcopts::PcOptions::parse("");
		assert_eq!((value(&o, "displaymode"), value(&o, "renderer")), (Some(0), Some(0)));
		assert!(apply(&mut o, "displaymode", 1) && apply(&mut o, "renderer", 1));
		assert_eq!((value(&o, "displaymode"), value(&o, "renderer")), (Some(1), Some(1)));
		assert!(!apply(&mut o, "fog", 1));
	}
}
