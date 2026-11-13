//! [PC] R&C2's modern Options menu (parity wave 2 B): tabs Display, Graphics and Camera (the rows of `modern_rows.rs`), the presets, the
//! Lighting rule, the Menu style notice and the fixed words (R&C1's texts), and the `Pack::ui` hooks. Every row is a pc_options.txt key:
//! `get` reads the options in force (rac2/options.rs `get`), `set` writes through `options::change` (saved, in force from the next frame).
//! Opened by Cross on the Camera row of the game's Options list (modern_open.rs).
use crate::modern_rows::*;
use rac_engine::pcopts::{self, PcOptions, FPS_LIST, RS_LIST};
use rac_engine::ui::{Notice, NeedsRule, UiChrome, UiMenu, UiPack, UiPresetDef, UiSection, UiTab};

static DISPLAY_SECS: [UiSection; 7] = [
	UiSection { title: ["Screen", "\u{c9}cran", "Bildschirm", "Pantalla", "Schermo"], rows: &SCREEN },
	// [PC] The PAL disc's hidden 60 Hz mode and movie subtitles (modern_rows_video.rs).
	UiSection { title: ["Video", "Vid\u{e9}o", "Video", "V\u{ed}deo", "Video"], rows: &crate::modern_rows_video::VIDEO },
	UiSection { title: ["Picture", "Image", "Bild", "Imagen", "Immagine"], rows: &crate::modern_rows_display::PICTURE }, // [b45 P2]
	UiSection { title: t("HUD"), rows: &HUD },
	UiSection { title: ["System", "Syst\u{e8}me", "System", "Sistema", "Sistema"], rows: &SYSTEM },
	UiSection { title: ["Start", "D\u{e9}marrage", "Start", "Inicio", "Avvio"], rows: &crate::modern_rows_input::SKIPCUT }, // [b45 P2] R&C1's Start section
	UiSection { title: ["Renderer", "Rendu", "Renderer", "Motor gr\u{e1}fico", "Renderer"], rows: &crate::modern_rows_display::RENDERER }, // [b45 P2] next start
];
static GRAPHICS_SECS: [UiSection; 5] = [
	UiSection { title: ["Quality", "Qualit\u{e9}", "Qualit\u{e4}t", "Calidad", "Qualit\u{e0}"], rows: &QUALITY },
	UiSection { title: ["Resolution", "R\u{e9}solution", "Aufl\u{f6}sung", "Resoluci\u{f3}n", "Risoluzione"], rows: &crate::modern_rows_upscale::UPSCALING },
	UiSection { title: ["Lighting and shadows", "Lumi\u{e8}re et ombres", "Licht und Schatten", "Luz y sombras", "Luce e ombre"], rows: &SHADING },
	UiSection { title: ["World", "Monde", "Welt", "Mundo", "Mondo"], rows: &WORLD },
	UiSection { title: ["Effects", "Effets", "Effekte", "Efectos", "Effetti"], rows: &crate::modern_rows_metal::EFFECTS },
];
static CAMERA_SECS: [UiSection; 6] = [
	UiSection { title: ["Camera", "Cam\u{e9}ra", "Kamera", "C\u{e1}mara", "Telecamera"], rows: &CAMERA },
	UiSection { title: ["Mouse", "Souris", "Maus", "Rat\u{f3}n", "Mouse"], rows: &MOUSE },
	UiSection { title: ["Aiming", "Vis\u{e9}e", "Zielen", "Punter\u{ed}a", "Mira"], rows: &crate::modern_rows_input::AIMING }, // [b46 W6] C11
	// [b45 P2] mouse invert + Quick Select mouse, retail's Left / Right and Up / Down, then the stick dead zone (R&C1's Controller order).
	UiSection { title: ["Mouse axes", "Axes souris", "Mausachsen", "Ejes del rat\u{f3}n", "Assi mouse"], rows: &crate::modern_rows_input::MOUSE_INVERT },
	UiSection { title: crate::pc_stickdz::S_CONTROLLER, rows: &crate::modern_rows_input::CONTROLLER },
	UiSection { title: ["Stick", "Stick", "Stick", "Stick", "Stick"], rows: &crate::pc_stickdz::ROWS },
];
static CONTROLS_SECS: [UiSection; 8] = [
	UiSection { title: crate::pad_binds::S_VIEW, rows: &crate::pad_binds::VIEW_ROWS }, // [b45 P6] C16
	UiSection { title: crate::pc_controls::S_ACTIONS, rows: &crate::pc_controls::ACTIONS },
	UiSection { title: crate::pc_controls::S_DPAD, rows: &crate::pc_controls::DPAD },
	UiSection { title: crate::pc_controls::S_MOVE, rows: &crate::pc_controls::MOVE },
	UiSection { title: crate::pc_controls::S_CAMERA, rows: &crate::pc_controls::CAMERA },
	UiSection { title: crate::pc_controls::S_PC, rows: &crate::pc_controls::PC }, // [b45 P6]
	UiSection { title: crate::pc_controls::S_RESET, rows: &crate::pc_controls::RESET },
	UiSection { title: crate::pc_debug_rows::S_DEBUG, rows: &crate::pc_debug_rows::ROWS },
];
/// "camera" is the tab modern_open.rs opens.
static TABS: [UiTab; 4] = [
	UiTab { id: "display", name: ["Display", "Affichage", "Anzeige", "Pantalla", "Schermo"], sections: &DISPLAY_SECS },
	UiTab { id: "graphics", name: ["Graphics", "Graphismes", "Grafik", "Gr\u{e1}ficos", "Grafica"], sections: &GRAPHICS_SECS },
	UiTab { id: "camera", name: ["Camera", "Cam\u{e9}ra", "Kamera", "C\u{e1}mara", "Telecamera"], sections: &CAMERA_SECS },
	UiTab { id: "controls", name: crate::pc_controls::T_CONTROLS, sections: &CONTROLS_SECS },
];

/// R&C1's preset rule on the rows R&C2 has (rac1/modern_menu/tabs.rs: the presets leave Lighting Classic; the effect rows are
/// set for when the player picks Enhanced).
const LOW: [(&str, i32); 17] = [("hud", 1), ("fire", 0), ("smoke", 0), ("texfilter", 1), ("lighting", 0), ("draw", 0), ("smoothing", 0), ("sunrays", 1), ("ao", 1), ("shadows", 1), ("gi", 0), ("water", 1), ("fog", 0), ("particles", 1), ("culling", 1), ("vegetation", 0), ("fur", 0)];
const MEDIUM: [(&str, i32); 17] = [("hud", 0), ("fire", 1), ("smoke", 1), ("texfilter", 1), ("lighting", 0), ("draw", 0), ("smoothing", 1), ("sunrays", 2), ("ao", 2), ("shadows", 2), ("gi", 0), ("water", 2), ("fog", 1), ("particles", 2), ("culling", 1), ("vegetation", 1), ("fur", 1)];
const HIGH: [(&str, i32); 17] = [("hud", 0), ("fire", 3), ("smoke", 3), ("texfilter", 1), ("lighting", 0), ("draw", 1), ("smoothing", 1), ("sunrays", 3), ("ao", 3), ("shadows", 3), ("gi", 0), ("water", 3), ("fog", 3), ("particles", 3), ("culling", 1), ("vegetation", 1), ("fur", 3)];
pub static PRESETS: [UiPresetDef; 3] = [
	UiPresetDef { name: LOW_T, sets: &LOW },
	UiPresetDef { name: MEDIUM_T, sets: &MEDIUM },
	UiPresetDef { name: HIGH_T, sets: &HIGH },
];
const LOW_T: rac_engine::ui::T5 = crate::modern_rows::LOW;
const MEDIUM_T: rac_engine::ui::T5 = crate::modern_rows::MEDIUM;
const HIGH_T: rac_engine::ui::T5 = crate::modern_rows::HIGH;
/// Every preset sets the texture filter Smooth (as R&C1's preset_also; here it is a row, so it is in the presets).
static PRESET_ALSO: [(&str, i32); 0] = [];

static NEEDS: [NeedsRule; 4] = [NeedsRule {
	row: "lighting",
	set_to: 1,
	for_row: "",
	fix: None,
	banner: ["Most effects need Enhanced lighting", "La plupart des effets demandent l'\u{e9}clairage am\u{e9}lior\u{e9}", "Die meisten Effekte brauchen verbesserte Beleuchtung", "La mayor\u{ed}a de los efectos necesitan iluminaci\u{f3}n mejorada", "Quasi tutti gli effetti richiedono l'illuminazione migliorata"],
	detail: ["This only shows with Lighting set to Enhanced. Lighting is {} now.", "Visible seulement avec l'\u{e9}clairage am\u{e9}lior\u{e9}. L'\u{e9}clairage est {} pour l'instant.", "Das ist nur mit verbesserter Beleuchtung zu sehen. Die Beleuchtung steht auf {}.", "Solo se ve con la iluminaci\u{f3}n mejorada. Ahora est\u{e1} en {}.", "Si vede solo con l'illuminazione migliorata. Ora \u{e8} {}."],
	button: ["Switch Lighting to Enhanced", "Passer \u{e0} l'\u{e9}clairage am\u{e9}lior\u{e9}", "Auf verbesserte Beleuchtung umstellen", "Cambiar a iluminaci\u{f3}n mejorada", "Passa all'illuminazione migliorata"],
}, crate::modern_rows_upscale::NEEDS_UPSCALER, crate::modern_rows_metal::NEEDS_METAL, crate::modern_rows_display::NEEDS_RULE];
static NOTICES: [Notice; 1] = [Notice { row: "menustyle", value: 1, text: ["Original pause, title and shop screens; these options stay on the Camera row", "\u{c9}crans de pause, titre et boutique d'origine ; ces options restent sur la ligne Cam\u{e9}ra", "Originale Pause-, Titel- und Shop-Bildschirme; diese Optionen bleiben in der Zeile Kamera", "Pantallas originales de pausa, t\u{ed}tulo y tienda; estas opciones siguen en la fila C\u{e1}mara", "Schermate originali di pausa, titolo e negozio; queste opzioni restano sulla riga Telecamera"] }];

const CHROME: UiChrome = UiChrome {
	title: ["Paused - Options", "Pause - Options", "Pause - Optionen", "Pausa - Opciones", "Pausa - Opzioni"],
	title_front: ["Options", "Options", "Optionen", "Opciones", "Opzioni"],
	autosave: ["Saved automatically", "Enregistr\u{e9} automatiquement", "Automatisch gespeichert", "Se guarda solo", "Salvataggio automatico"],
	now: ["Now", "Actuel", "Jetzt", "Ahora", "Ora"],
	default: ["Default", "Par d\u{e9}faut", "Standard", "Original", "Predefinito"],
	speed_impact: ["Speed impact", "Impact sur la vitesse", "Leistungsbedarf", "Impacto en velocidad", "Impatto sulla velocit\u{e0}"],
	cost_names: [["Low", "Faible", "Gering", "Bajo", "Basso"], ["Medium", "Moyen", "Mittel", "Medio", "Medio"], ["High", "\u{c9}lev\u{e9}", "Hoch", "Alto", "Alto"]],
	peek_note: ["Look at the game behind", "Regarder le jeu derri\u{e8}re", "Das Spiel dahinter ansehen", "Mirar el juego de fondo", "Guarda il gioco dietro"],
	view_game: ["View game", "Voir le jeu", "Spiel ansehen", "Ver juego", "Vedi gioco"],
	back_to_menu: ["Back to the menu", "Retour au menu", "Zur\u{fc}ck zum Men\u{fc}", "Volver al men\u{fa}", "Torna al menu"],
	change: ["Change", "Modifier", "\u{c4}ndern", "Cambiar", "Cambia"],
	back: ["Back", "Retour", "Zur\u{fc}ck", "Atr\u{e1}s", "Indietro"],
	reset: ["Reset to default", "Valeur par d\u{e9}faut", "Zur\u{fc}cksetzen", "Restablecer", "Ripristina"],
	switch_tab: ["Switch tab", "Changer d'onglet", "Tab wechseln", "Cambiar pesta\u{f1}a", "Cambia scheda"],
	off_on: [OFF, ON],
	custom: ["Custom", "Personnalis\u{e9}", "Eigene", "Personal", "Personale"],
};

static MENU: UiMenu = UiMenu { tabs: &TABS, presets: &PRESETS, preset_reset: 2, preset_also: &PRESET_ALSO, needs: &NEEDS, notices: &NOTICES, chrome: CHROME };

pub fn menu() -> &'static UiMenu {
	&MENU
}

fn lang() -> usize {
	match crate::options::get().lang {
		Some(l) => [pcopts::Lang::En, pcopts::Lang::Fr, pcopts::Lang::De, pcopts::Lang::Es, pcopts::Lang::It].iter().position(|&x| x == l).unwrap_or(0),
		None => 0,
	}
}

fn pos<T: PartialEq>(list: &[T], v: T) -> i32 {
	list.iter().position(|x| *x == v).unwrap_or(0) as i32
}

/// The row value `id` has in `o` (None = no such row).
pub fn value(o: &PcOptions, id: &str) -> Option<i32> {
	if let Some(v) = crate::modern_rows_input::value(o, id)
		.or_else(|| crate::modern_rows_display::value(o, id))
		.or_else(|| crate::modern_rows_upscale::value(id))
		.or_else(|| crate::modern_rows_metal::value(o, id))
		.or_else(|| crate::modern_rows_video::value(o, id))
		.or_else(|| crate::modern_rows_hud::value(o, id))
		.or_else(|| crate::modern_rows_fire::value(o, id))
		.or_else(|| crate::pc_debug_rows::value(id))
	{
		return Some(v);
	}
	let lit = o.lighting_or_default();
	Some(match id {
		"stickdz" => crate::pc_stickdz::mode(o),
		"lighting" => lit as i32,
		"texfilter" => o.texfilter.unwrap_or(true) as i32,
		"ao" => o.ao_level() as i32,
		// No key = High, the row's default (as R&C1's): `options::verdict` turns it on only with Lighting Enhanced, and with Classic the
		// row is inert anyway (the sun pass needs Enhanced), so a new file reads as the High preset, not Custom.
		"shadows" => o.shadows.map_or(3, |v| v.min(3)) as i32,
		"sunrays" => o.sun_rays.map_or(3, |v| v.min(3)) as i32,
		"gi" => o.gi.unwrap_or(0).min(3) as i32,
		"water" => o.water.map_or(3, |v| v.min(3)) as i32,
		"fog" => o.fog.map_or(3, |v| v.min(3)) as i32,
		"particles" => o.particles.map_or(3, |v| v.min(3)) as i32,
		"clouds" => o.clouds.map_or(0, |v| v.min(3)) as i32, // [b45 P2] no key = Classic (options.rs `clouds_q`)
		"draw" => o.draw.unwrap_or(1).min(2) as i32,
		"smoothing" => o.smoothing.unwrap_or(true) as i32,
		"culling" => o.culling.unwrap_or(true) as i32,
		"vegetation" => o.vegetation.unwrap_or(1).min(2) as i32,
		"fur" => o.fur.unwrap_or(3).min(3) as i32,
		"furstyle" => o.furstyle.unwrap_or(false) as i32,
		"aspect" => rac_engine::viewer::aspect_box::current() as i32,
		"rscale" => pos(&RS_LIST.map(|r| r as u32), o.render_scale()),
		"fps" => pos(&FPS_LIST, o.fps_limit()),
		"hud" => !o.hud_sharp() as i32,
		"debug" => o.debug_on() as i32,
		"menustyle" => o.extra_get("menustyle").is_some_and(|v| v.trim().eq_ignore_ascii_case("classic")) as i32,
		"camera" => o.camera_byte() as i32,
		"camspeed" => o.camspeed.map_or(50, pcopts::snap_camspeed) as i32,
		"mousespeed" => o.mousespeed.map_or(50, pcopts::snap_camspeed) as i32,
		_ => return None,
	})
}

/// Writes row `id` = `v` into `o`; false = no such row.
pub fn apply(o: &mut PcOptions, id: &str, v: i32) -> bool {
	if crate::modern_rows_input::apply(o, id, v) || crate::modern_rows_display::apply(o, id, v) || crate::modern_rows_metal::apply(o, id, v) || crate::modern_rows_video::apply(o, id, v) || crate::modern_rows_fire::apply(o, id, v) || crate::modern_rows_hud::apply(o, id, v) || crate::pc_debug_rows::apply(id) {
		return true;
	}
	let lvl = v.clamp(0, 3) as u8;
	match id {
		"stickdz" => crate::pc_stickdz::apply(o, v),
		"lighting" => o.lighting = Some(lvl.min(1)),
		"texfilter" => o.texfilter = Some(v != 0),
		"ao" => o.ao = Some(lvl),
		"shadows" => o.shadows = Some(lvl),
		"sunrays" => o.sun_rays = Some(lvl),
		"gi" => o.gi = Some(lvl),
		"water" => o.water = Some(lvl),
		"fog" => o.fog = Some(lvl),
		"particles" => o.particles = Some(lvl),
		"clouds" => o.clouds = Some(lvl),
		"draw" => o.draw = Some(lvl.min(2)),
		"smoothing" => o.smoothing = Some(v != 0),
		// Off writes the marker R&C1's file needs (pcopts `to_text`: culling=off + culling_off=kept while Off).
		"culling" => o.culling = Some(v != 0),
		"vegetation" => o.vegetation = Some(lvl.min(2)),
		"fur" => o.fur = Some(lvl),
		"furstyle" => o.furstyle = Some(v != 0),
		"aspect" => {
			rac_engine::viewer::aspect_box::set_from_ui(o, v);
		}
		"rscale" => {
			let r = RS_LIST[v.clamp(0, RS_LIST.len() as i32 - 1) as usize];
			o.extra_set("renderscale", (r != 100).then(|| r.to_string()));
		}
		"fps" => {
			let f = FPS_LIST[v.clamp(0, FPS_LIST.len() as i32 - 1) as usize];
			o.extra_set("fps", match f {
				0 => None,
				u32::MAX => Some("off".to_string()),
				n => Some(n.to_string()),
			});
		}
		"hud" => o.hud = Some(v == 0),
		"debug" => o.debug = Some(v != 0),
		"menustyle" => o.extra_set("menustyle", (v != 0).then(|| "classic".to_string())),
		"camera" => o.set_camera_byte(v.clamp(0, 2) as u8),
		"camspeed" => o.camspeed = Some(pcopts::snap_camspeed(v.clamp(0, 255) as u8)),
		"mousespeed" => o.mousespeed = Some(pcopts::snap_camspeed(v.clamp(0, 255) as u8)),
		_ => return crate::modern_rows_upscale::apply(o, id, v),
	}
	true
}

fn get(id: &str) -> Option<i32> {
	if id.starts_with("bind_") {
		return crate::pc_controls::value(id);
	}
	if id.starts_with("dbgkey_") {
		return crate::pc_debug_rows::get(id);
	}
	value(&crate::options::get(), id)
}

fn set(id: &str, v: i32) {
	// The Debug section's key rows only show text (pc_debug_rows.rs): nothing is saved.
	if crate::pc_controls::act(id, v) || crate::pc_debug_rows::apply(id) {
		return;
	}
	if value(&crate::options::get(), id) == Some(v) {
		return;
	}
	if let Err(e) = crate::options::change(|o| {
		apply(o, id, v);
	}) {
		eprintln!("rac2: PC options: cannot save {id}: {e}");
	}
}

/// The Button glyphs row (glyph_cells.rs): the style the modern menus draw their button pictures in.
fn glyphs() -> u8 {
	crate::glyph_cells::style()
}

/// `Pack::ui` for R&C2 (pack_bind.rs).
pub static PACK: UiPack = UiPack { menu, get, set, lang, glyphs };

#[cfg(test)]
#[path = "modern_menu_tests.rs"]
mod tests;
