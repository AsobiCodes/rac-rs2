//! [PC] R&C2's modern Options rows (parity wave 2 B): only the rows whose engine code works in R&C2 after waves 1 and 2A (rac2/options.rs
//! `verdict`, `fps_setting`, `render_scale_pct`, the aspect box, the HUD, the Modern camera, Debug mode). The five-language texts of the
//! rows R&C1 has too are R&C1's (rac1/modern_menu, copied by runs/w2d/gen_rows.py); Smooth texture detail and Debug mode are R&C2 rows.
//! Row ids are the ones `modern_menu.rs` `get` / `set` know.
use rac_engine::ui::{Cost, Needs, RowKind, UiRow, LANGS, T5};

/// Speed impact levels. Dynamic shadows MEASURED on R&C2 (wave 2A: 2560x1440, Lancer scene, about -8 % = Medium); the others are
/// R&C1's measured levels (rac1/modern_menu/costs.rs) for the same engine passes.
mod cost {
	use rac_engine::ui::Cost;
	pub const RSCALE: Cost = Cost::High;
	pub const SMOOTHING: Cost = Cost::Low;
	pub const LIGHTING: Cost = Cost::High;
	pub const AO: Cost = Cost::Low;
	pub const SHADOWS: Cost = Cost::Medium;
	pub const GI: Cost = Cost::High; // R&C1 re-measured 2026-11-06 (fire lights + fire shadows), rac1/modern_menu/costs.rs
	pub const FOG: Cost = Cost::Low;
	pub const WATER: Cost = Cost::High; // R&C1 rac1/modern_menu/costs.rs WATER
	pub const PARTICLES: Cost = Cost::Low; // R&C1 rac1/modern_menu/costs.rs PARTICLES
	pub const DRAW: Cost = Cost::Medium;
	pub const VEGETATION: Cost = Cost::High; // R&C1 rac1/modern_menu/costs.rs VEGETATION
	pub const FUR: Cost = Cost::Low; // R&C1 rac1/modern_menu/costs.rs FUR
	pub const SUNRAYS: Cost = Cost::Low; // R&C1 rac1/modern_menu/costs.rs SUNRAYS
}

pub const fn t(s: &'static str) -> T5 {
	[s; LANGS]
}

pub const NO: &[T5] = &[];

/// Off (or Classic) / Low / Medium / High, the effect rows' levels.
pub const OFF: T5 = ["Off", "Non", "Aus", "No", "No"];
pub const ON: T5 = ["On", "Oui", "Ein", "S\u{ed}", "S\u{ec}"];
pub const CLASSIC: T5 = ["Classic", "Classique", "Klassisch", "Cl\u{e1}sico", "Classico"];
pub const LOW: T5 = ["Low", "Faible", "Niedrig", "Baja", "Bassa"];
pub const MEDIUM: T5 = ["Medium", "Moyen", "Mittel", "Media", "Media"];
pub const HIGH: T5 = ["High", "\u{c9}lev\u{e9}", "Hoch", "Alta", "Alta"];
pub const Q4: &[T5] = &[OFF, LOW, MEDIUM, HIGH];
pub const QC: &[T5] = &[CLASSIC, LOW, MEDIUM, HIGH];
pub const LMH: &[T5] = &[LOW, MEDIUM, HIGH];

pub const CH: RowKind = RowKind::Choice;
pub const TOG: RowKind = RowKind::Toggle;

/// Rows that show nothing until Lighting is Enhanced (DECISIONS 1, PLAN section 8).
pub const LIGHT: Option<Needs> = Some(Needs { row: "lighting", min: 1, max: i32::MAX });
/// Rows of the improved water (Waves, Underwater look): nothing at Water Classic; Water itself needs Lighting, so they dim with
/// either (a chain, `Model::blocked`).

#[allow(clippy::too_many_arguments)]
pub const fn row(id: &'static str, kind: RowKind, label: T5, desc: T5, values: &'static [T5], def: i32, cost: Cost, needs: Option<Needs>) -> UiRow {
	UiRow { id, kind, label, desc, values, def, cost, needs, note: None }
}

/// `r` with the extra line `note` under its description.
const fn with_note(r: UiRow, note: T5) -> UiRow {
	UiRow { note: Some(note), ..r }
}

/// A slider row from `min` to `max` in `step`s with `unit` after the number.
pub const fn slider(min: i32, max: i32, step: i32, unit: &'static str) -> RowKind {
	RowKind::Slider { min, max, step, unit }
}

const LIGHT_VALUES: &[T5] = &[CLASSIC, ["Enhanced", "Am\u{e9}lior\u{e9}", "Verbessert", "Mejorado", "Migliorato"]];
const FPS_VALUES: &[T5] = &[["Display", "\u{c9}cran", "Anzeige", "Pantalla", "Schermo"], t("30"), t("60"), t("120"), t("144"), OFF];
const RS_VALUES: &[T5] = &[t("50%"), t("75%"), t("100%"), t("125%"), t("150%"), t("200%")];
const HUD_VALUES: &[T5] = &[["Sharp", "Net", "Scharf", "N\u{ed}tido", "Nitido"], ["Original", "Original", "Original", "Original", "Originale"]];
/// `rac_engine::viewer::aspect_box::ALL` order.
const ASPECT_VALUES: &[T5] = &[t("Auto"), t("4:3"), t("16:9"), t("16:10"), t("21:9"), t("32:9")];
/// Classic in its feminine form for Spanish and Italian (la c\u{e1}mara, la telecamera).
const CAMERAS: &[T5] = &[["Classic", "Classique", "Klassisch", "Cl\u{e1}sica", "Classica"], ["Static", "Fixe", "Statisch", "Est\u{e1}tica", "Statica"], ["Smooth", "Fluide", "Sanft", "Suave", "Fluida"]];
const FUR_STYLES: &[T5] = &[["Soft", "Doux", "Weich", "Suave", "Morbido"], ["Fluffy", "Duveteux", "Flauschig", "Esponjoso", "Soffice"]];
const VEG_VALUES: &[T5] = &[OFF, ["Moderate", "Mod\u{e9}r\u{e9}e", "M\u{e4}\u{df}ig", "Moderada", "Moderata"], ["Lush", "Luxuriante", "\u{dc}ppig", "Frondosa", "Rigogliosa"]];
const STYLES: &[T5] = &[["Modern", "Moderne", "Modern", "Moderno", "Moderno"], CLASSIC];

pub static QUALITY: [UiRow; 3] = [
	row(
		"preset",
		RowKind::Preset,
		["Preset", "Pr\u{e9}r\u{e9}glage", "Voreinstellung", "Preajuste", "Preset"],
		[
			"Low, Medium and High set the graphics rows in one go. Change any row yourself and the preset shows Custom.",
			"Faible, Moyen et \u{c9}lev\u{e9} r\u{e8}glent toutes les lignes d'un coup. Changez une ligne et le pr\u{e9}r\u{e9}glage devient Personnalis\u{e9}.",
			"Niedrig, Mittel und Hoch stellen alle Grafikzeilen auf einmal ein. \u{c4}nderst du eine Zeile, zeigt die Voreinstellung Eigene.",
			"Baja, Media y Alta ajustan todas las filas de golpe. Si cambias una fila, el preajuste pasa a Personal.",
			"Bassa, Media e Alta impostano tutte le righe in un colpo. Se cambi una riga, il preset diventa Personale.",
		],
		NO,
		0,
		Cost::None,
		None,
	),
	row(
		"lighting",
		CH,
		["Lighting", "\u{c9}clairage", "Beleuchtung", "Iluminaci\u{f3}n", "Illuminazione"],
		[
			"Classic is the original PS2 lighting. Enhanced is the new PC lighting. The effects below need it.",
			"Classique est l'\u{e9}clairage PS2 d'origine. Am\u{e9}lior\u{e9} est le nouvel \u{e9}clairage PC. Les effets ci-dessous en ont besoin.",
			"Klassisch ist die originale PS2-Beleuchtung. Verbessert ist die neue PC-Beleuchtung. Die Effekte darunter brauchen sie.",
			"Cl\u{e1}sico es la iluminaci\u{f3}n original de PS2. Mejorado es la nueva iluminaci\u{f3}n de PC. Los efectos de abajo la necesitan.",
			"Classico \u{e8} l'illuminazione PS2 originale. Migliorato \u{e8} la nuova illuminazione PC. Gli effetti qui sotto ne hanno bisogno.",
		],
		LIGHT_VALUES,
		0,
		cost::LIGHTING,
		None,
	),
	row(
		"texfilter",
		TOG,
		["Smooth texture detail", "D\u{e9}tail de texture liss\u{e9}", "Glatte Texturdetails", "Detalle de textura suave", "Dettaglio texture morbido"],
		[
			"Smooths the blocky look of close-up textures. Off keeps the original sharp texture pixels.",
			"Adoucit l'aspect pix\u{e9}lis\u{e9} des textures de pr\u{e8}s. Non garde les pixels d'origine.",
			"Gl\u{e4}ttet den pixeligen Look naher Texturen. Aus beh\u{e4}lt die originalen Texturpixel.",
			"Suaviza el aspecto pixelado de las texturas de cerca. No mantiene los p\u{ed}xeles originales.",
			"Ammorbidisce l'aspetto squadrato delle texture da vicino. No mantiene i pixel originali.",
		],
		NO,
		1,
		Cost::Low,
		LIGHT,
	),
];

pub static SHADING: [UiRow; 10] = [
	row(
		"ao",
		CH,
		["Ambient occlusion", "Occlusion ambiante", "Umgebungsverdeckung", "Oclusi\u{f3}n ambiental", "Occlusione ambientale"],
		[
			"Soft shadows in corners, creases and under objects.",
			"Ombres douces dans les coins, les creux et sous les objets.",
			"Weiche Schatten in Ecken, Falten und unter Objekten.",
			"Sombras suaves en rincones, pliegues y bajo los objetos.",
			"Ombre morbide negli angoli, nelle pieghe e sotto gli oggetti.",
		],
		Q4,
		3,
		cost::AO,
		LIGHT,
	),
	row(
		"shadows",
		CH,
		["Dynamic shadows", "Ombres dynamiques", "Dynamische Schatten", "Sombras din\u{e1}micas", "Ombre dinamiche"],
		[
			"Real-time shadows cast by the level and the characters.",
			"Ombres en temps r\u{e9}el projet\u{e9}es par le niveau et les personnages.",
			"Echtzeit-Schatten von Level und Figuren.",
			"Sombras en tiempo real del nivel y de los personajes.",
			"Ombre in tempo reale proiettate dal livello e dai personaggi.",
		],
		Q4,
		3,
		cost::SHADOWS,
		LIGHT,
	),
	row(
		"gi",
		CH,
		["Global illumination", "Illumination globale", "Globale Beleuchtung", "Iluminaci\u{f3}n global", "Illuminazione globale"],
		[
			"Light bounces off walls and floors; fire and weapon shots light up what is nearby.",
			"La lumi\u{e8}re rebondit sur les murs et les sols ; le feu et les tirs \u{e9}clairent les alentours.",
			"Licht prallt von W\u{e4}nden und B\u{f6}den ab; Feuer und Sch\u{fc}sse erhellen die Umgebung.",
			"La luz rebota en paredes y suelos; el fuego y los disparos iluminan lo que hay cerca.",
			"La luce rimbalza su muri e pavimenti; fuoco e colpi illuminano ci\u{f2} che \u{e8} vicino.",
		],
		Q4,
		0,
		cost::GI,
		LIGHT,
	),
	// Wave 3 row 2: the improved water (R&C1's row; R&C2's water batches are the engine's fogged water quads, rac2/water.rs).
	row(
		"water",
		CH,
		["Water", "Eau", "Wasser", "Agua", "Acqua"],
		[
			"Classic is the original water. Low to High use the improved water with reflections.",
			"Classique est l'eau d'origine. Faible \u{e0} \u{c9}lev\u{e9} utilisent l'eau am\u{e9}lior\u{e9}e avec reflets.",
			"Klassisch ist das Originalwasser. Niedrig bis Hoch nutzen das verbesserte Wasser mit Spiegelungen.",
			"Cl\u{e1}sico es el agua original. De Baja a Alta usan el agua mejorada con reflejos.",
			"Classico \u{e8} l'acqua originale. Da Bassa ad Alta usano l'acqua migliorata con riflessi.",
		],
		QC,
		3,
		cost::WATER,
		LIGHT,
	),
	// Wave 3 row 1 / wave 4.1: R&C1's fog row; the volume keeps the retail fog's shape (rac2/looks.rs `retail_fog_shape`: sky clear, air from the near distance), no air under space skies.
	row(
		"fog",
		CH,
		["Volumetric fog", "Brouillard volum\u{e9}trique", "Volumetrischer Nebel", "Niebla volum\u{e9}trica", "Nebbia volumetrica"],
		[
			"Classic is the original fog. Low to High give the fog depth, so light shines through it.",
			"Classique est le brouillard d'origine. Faible \u{e0} \u{c9}lev\u{e9} lui donnent de la profondeur, la lumi\u{e8}re le traverse.",
			"Klassisch ist der Originalnebel. Niedrig bis Hoch geben ihm Tiefe, sodass Licht hindurchscheint.",
			"Cl\u{e1}sico es la niebla original. De Baja a Alta le dan profundidad y la luz la atraviesa.",
			"Classico \u{e8} la nebbia originale. Da Bassa ad Alta le danno profondit\u{e0} e la luce la attraversa.",
		],
		QC,
		3,
		cost::FOG,
		LIGHT,
	),
	// Wave 4.2: R&C1's particles row; R&C2's effect-program prims (rac2/host/ov_layers.rs) take the soft pass, the glow and the
	// effect lights (engine gpu/layer_fx.rs `lit`); the no-depth ones stay over the world (partfx.rs `over`).
	row(
		"particles",
		CH,
		["Particles", "Particules", "Partikel", "Part\u{ed}culas", "Particelle"],
		[
			"Classic keeps the original sparks and explosions. Low to High use the improved effects.",
			"Classique garde les \u{e9}tincelles et explosions d'origine. Faible \u{e0} \u{c9}lev\u{e9} utilisent les effets am\u{e9}lior\u{e9}s.",
			"Klassisch beh\u{e4}lt die originalen Funken und Explosionen. Niedrig bis Hoch nutzen die verbesserten Effekte.",
			"Cl\u{e1}sico mantiene las chispas y explosiones originales. De Baja a Alta usan los efectos mejorados.",
			"Classico mantiene le scintille e le esplosioni originali. Da Bassa ad Alta usano gli effetti migliorati.",
		],
		QC,
		3,
		cost::PARTICLES,
		LIGHT,
	),
	// [PC] R&C1's Vegetation row and texts (rac1/modern_menu/rows_graphics.rs); the engine's plant planner over R&C2's collision (veg.rs).
	row(
		"vegetation",
		CH,
		["Vegetation", "V\u{e9}g\u{e9}tation", "Vegetation", "Vegetaci\u{f3}n", "Vegetazione"],
		[
			"Extra grass and plants on the planets. Off is the original.",
			"Herbe et plantes en plus sur les plan\u{e8}tes. Avec Non, c'est l'original.",
			"Zus\u{e4}tzliches Gras und Pflanzen auf den Planeten. Aus ist das Original.",
			"Hierba y plantas extra en los planetas. Con No se mantiene el original.",
			"Erba e piante in pi\u{f9} sui pianeti. Con No resta l'originale.",
		],
		VEG_VALUES,
		1,
		cost::VEGETATION,
		LIGHT,
	),
	// [PC] R&C1's Fur and Fur style rows and texts (rac1/modern_menu/rows_effects.rs); Ratchet's prop gets the shells (fur.rs).
	row(
		"fur",
		CH,
		["Fur", "Fourrure", "Fell", "Pelo", "Pelo"],
		[
			"Fur shading for Ratchet. Off is the original look.",
			"Ombrage de la fourrure de Ratchet. Avec Non, c'est l'aspect d'origine.",
			"Fell-Schattierung f\u{fc}r Ratchet. Aus ist der Originallook.",
			"Sombreado del pelo de Ratchet. Con No se mantiene el aspecto original.",
			"Ombreggiatura del pelo di Ratchet. Con No resta l'aspetto originale.",
		],
		Q4,
		3,
		cost::FUR,
		LIGHT,
	),
	row(
		"furstyle",
		CH,
		["Fur style", "Style de fourrure", "Fellstil", "Estilo de pelo", "Stile del pelo"],
		[
			"Two looks for the fur: Soft or Fluffy.",
			"Deux aspects pour la fourrure : Doux ou Duveteux.",
			"Zwei Looks f\u{fc}r das Fell: Weich oder Flauschig.",
			"Dos aspectos para el pelo: Suave o Esponjoso.",
			"Due aspetti per il pelo: Morbido o Soffice.",
		],
		FUR_STYLES,
		0,
		Cost::None,
		LIGHT,
	),
	// Row 12: R&C1's row; the rays come from the level's key light (Looks::rays_from_key, engine prepare.rs).
	row(
		"sunrays",
		CH,
		["Sun rays", "Rayons de soleil", "Sonnenstrahlen", "Rayos de sol", "Raggi di sole"],
		[
			"Shafts of light around the sun and bright skies.",
			"Faisceaux de lumi\u{e8}re autour du soleil et des ciels clairs.",
			"Lichtstrahlen um die Sonne und an hellem Himmel.",
			"Haces de luz alrededor del sol y de los cielos brillantes.",
			"Fasci di luce attorno al sole e ai cieli luminosi.",
		],
		Q4,
		3,
		cost::SUNRAYS,
		LIGHT,
	),
];

pub static WORLD: [UiRow; 3] = [
	row(
		"draw",
		CH,
		["Draw distance", "Distance d'affichage", "Sichtweite", "Distancia visual", "Distanza visiva"],
		[
			"How far into the distance the level is drawn.",
			"Jusqu'o\u{f9} le niveau est dessin\u{e9} au loin.",
			"Wie weit der Level in die Ferne gezeichnet wird.",
			"Hasta qu\u{e9} distancia se dibuja el nivel.",
			"Fino a che distanza viene disegnato il livello.",
		],
		LMH,
		1,
		cost::DRAW,
		None,
	),
	// [PC] Culling (BATCH A CULL, rac2/moby_cull.rs): Off also draws the mobies the game left out (camera tests, draw distance). R&C1's
	// row and texts (rac1/modern_menu/rows_graphics.rs).
	row(
		"culling",
		TOG,
		["Culling", "Masquage", "Culling", "Descarte", "Culling"],
		[
			"Skips drawing what the camera cannot see. On is faster and looks the same.",
			"Ne dessine pas ce que la cam\u{e9}ra ne voit pas. Avec Oui, c'est plus rapide et identique \u{e0} l'\u{e9}cran.",
			"\u{dc}berspringt, was die Kamera nicht sieht. Ein ist schneller und sieht gleich aus.",
			"Omite lo que la c\u{e1}mara no ve. Con S\u{ed} es m\u{e1}s r\u{e1}pido y se ve igual.",
			"Salta ci\u{f2} che la telecamera non vede. Con S\u{ec} \u{e8} pi\u{f9} veloce e l'immagine non cambia.",
		],
		NO,
		1,
		Cost::None,
		None,
	),
	// [b45 P2] R&C1's clouds row (rac1 rows_graphics.rs); R&C2's default stays Classic (options.rs `clouds_q`, the look per planet:
	// planet_sky.rs), so it is not in the presets (High would make a fresh file read Custom).
	row(
		"clouds",
		CH,
		["Volumetric clouds", "Nuages volum\u{e9}triques", "Volumetrische Wolken", "Nubes volum\u{e9}tricas", "Nuvole volumetriche"],
		[
			"Classic is the original sky. Low to High add 3D clouds.",
			"Classique est le ciel d'origine. Faible \u{e0} \u{c9}lev\u{e9} ajoutent des nuages en 3D.",
			"Klassisch ist der Originalhimmel. Niedrig bis Hoch f\u{fc}gen 3D-Wolken hinzu.",
			"Cl\u{e1}sico es el cielo original. De Baja a Alta a\u{f1}aden nubes en 3D.",
			"Classico \u{e8} il cielo originale. Da Bassa ad Alta aggiungono nuvole 3D.",
		],
		QC,
		0,
		Cost::Medium,
		LIGHT,
	),
];

pub static SCREEN: [UiRow; 6] = [
	crate::modern_rows_display::DISPLAYMODE, // [b45 P2] window side: host/display_res.rs + host.rs sync_display_mode
	crate::modern_rows_display::RESOLUTION,
	row(
		"aspect",
		CH,
		["Aspect ratio", "Format d'image", "Seitenverh\u{e4}ltnis", "Relaci\u{f3}n de aspecto", "Proporzioni"],
		[
			"Auto fills the window. Another shape draws the game in the largest box of that shape, with black bars around it (ReShade is off while they show). 4:3 is the shape of the original PS2 picture.",
			"Auto remplit la fen\u{ea}tre. Un autre format dessine le jeu dans le plus grand cadre de ce format, avec des bandes noires autour (ReShade est d\u{e9}sactiv\u{e9} tant qu'elles sont affich\u{e9}es). 4:3 est le format de l'image PS2 d'origine.",
			"Auto f\u{fc}llt das Fenster. Ein anderes Format zeichnet das Spiel im gr\u{f6}\u{df}ten Rahmen dieser Form, mit schwarzen Balken drumherum (solange sie zu sehen sind, ist ReShade aus). 4:3 ist das Format des Original-PS2-Bilds.",
			"Auto llena la ventana. Otro formato dibuja el juego en el mayor recuadro de esa forma, con barras negras alrededor (ReShade se desactiva mientras se ven). 4:3 es el formato de la imagen original de PS2.",
			"Auto riempie la finestra. Un altro formato disegna il gioco nel riquadro pi\u{f9} grande di quella forma, con bande nere intorno (ReShade \u{e8} disattivato finch\u{e9} sono visibili). 4:3 \u{e8} il formato dell'immagine PS2 originale.",
		],
		ASPECT_VALUES,
		0,
		Cost::None,
		None,
	),
	row(
		"rscale",
		CH,
		["Render scale", "Taille du rendu", "Render-Skalierung", "Escala de render", "Scala di rendering"],
		[
			"Draws the picture at this size of the window, then fits it to the window. Below 100% runs faster, above 100% looks sharper.",
			"Dessine l'image \u{e0} cette taille de la fen\u{ea}tre, puis l'ajuste. Sous 100 %, c'est plus rapide; au-dessus, plus net.",
			"Zeichnet das Bild in dieser Gr\u{f6}\u{df}e des Fensters und passt es dann an. Unter 100 % l\u{e4}uft es schneller, dar\u{fc}ber sch\u{e4}rfer.",
			"Dibuja la imagen a este tama\u{f1}o de la ventana y la ajusta. Por debajo de 100 % va m\u{e1}s r\u{e1}pido, por encima se ve m\u{e1}s n\u{ed}tido.",
			"Disegna l'immagine a questa dimensione della finestra e poi la adatta. Sotto il 100% \u{e8} pi\u{f9} veloce, sopra \u{e8} pi\u{f9} nitida.",
		],
		RS_VALUES,
		2,
		cost::RSCALE,
		None,
	),
	row(
		"fps",
		CH,
		["FPS limit", "Limite FPS", "FPS-Limit", "L\u{ed}mite de FPS", "Limite FPS"],
		[
			"The most frames a second the game draws. Display matches your monitor, Off removes the limit.",
			"Le maximum d'images par seconde. \u{c9}cran suit votre moniteur, Non retire la limite.",
			"Die meisten Bilder pro Sekunde, die das Spiel zeichnet. Anzeige folgt deinem Monitor, Aus hebt das Limit auf.",
			"El m\u{e1}ximo de fotogramas por segundo. Pantalla sigue tu monitor, No quita el l\u{ed}mite.",
			"Il massimo di fotogrammi al secondo. Schermo segue il monitor, No toglie il limite.",
		],
		FPS_VALUES,
		0,
		Cost::None,
		None,
	),
	row(
		"smoothing",
		TOG,
		["Frame smoothing", "Lissage des images", "Bildgl\u{e4}ttung", "Suavizado de im\u{e1}genes", "Fluidit\u{e0} immagini"],
		[
			"Draws in-between frames so motion looks smooth on fast monitors. The game itself still runs at its original speed.",
			"Dessine des images interm\u{e9}diaires pour un mouvement fluide sur les \u{e9}crans rapides. Le jeu garde sa vitesse d'origine.",
			"Zeichnet Zwischenbilder, damit Bewegung auf schnellen Monitoren fl\u{fc}ssig wirkt. Das Spiel l\u{e4}uft weiter mit Originaltempo.",
			"Dibuja fotogramas intermedios para que el movimiento sea fluido en monitores r\u{e1}pidos. El juego sigue a su velocidad original.",
			"Disegna fotogrammi intermedi per un movimento fluido sui monitor veloci. Il gioco gira alla velocit\u{e0} originale.",
		],
		NO,
		1,
		cost::SMOOTHING,
		None,
	),
];

pub static HUD: [UiRow; 3] = [
	row(
		"hud",
		CH,
		["HUD", "HUD", "HUD", "HUD", "HUD"],
		[
			"Sharp draws the HUD at your screen resolution. Original keeps the PS2 look.",
			"Net dessine le HUD \u{e0} la r\u{e9}solution de l'\u{e9}cran. Original garde l'aspect PS2.",
			"Scharf zeichnet das HUD in deiner Bildschirmaufl\u{f6}sung. Original beh\u{e4}lt den PS2-Look.",
			"N\u{ed}tido dibuja el HUD a la resoluci\u{f3}n de tu pantalla. Original mantiene el aspecto de PS2.",
			"Nitido disegna l'HUD alla risoluzione dello schermo. Originale mantiene l'aspetto PS2.",
		],
		HUD_VALUES,
		0,
		Cost::None,
		None,
	),
	crate::modern_rows_hud::HUDPOS, // [PC] v0.1 lane hud (modern_rows_hud.rs)
	crate::modern_rows_hud::HUDSCALE,
];

/// Debug mode moved to the Controls tab's Debug section (pc_debug_rows.rs, as R&C1's Controls page).
pub static SYSTEM: [UiRow; 3] = [
	row(
		"menustyle",
		CH,
		["Menu style", "Style des menus", "Men\u{fc}stil", "Estilo de men\u{fa}s", "Stile dei menu"],
		[
			"Modern is this menu. Classic brings back the original-style option pages.",
			"Moderne est ce menu. Classique ram\u{e8}ne les pages d'options dans le style d'origine.",
			"Modern ist dieses Men\u{fc}. Klassisch bringt die Optionsseiten im Originalstil zur\u{fc}ck.",
			"Moderno es este men\u{fa}. Cl\u{e1}sico recupera las p\u{e1}ginas de opciones al estilo original.",
			"Moderno \u{e8} questo menu. Classico riporta le pagine delle opzioni in stile originale.",
		],
		STYLES,
		0,
		Cost::None,
		None,
	),
	crate::modern_rows_hud::GLYPHS,
	crate::modern_rows_hud::INTRO,
];

/// Classic and back take effect at the next start (the Modern camera's hand is put in at the boot, rac2/lib.rs `register`).
pub static CAMERA: [UiRow; 2] = [
	with_note(
		row(
		"camera",
		CH,
		["Camera", "Cam\u{e9}ra", "Kamera", "C\u{e1}mara", "Telecamera"],
		[
			"Classic is the original camera and controls. Static is the mouse camera. Smooth is the mouse camera with a gentle follow.",
			"Classique est la cam\u{e9}ra et les commandes d'origine. Fixe est la cam\u{e9}ra \u{e0} la souris. Fluide ajoute un suivi doux.",
			"Klassisch ist die originale Kamera samt Steuerung. Statisch ist die Maus-Kamera. Sanft ist die Maus-Kamera mit weichem Nachf\u{fc}hren.",
			"Cl\u{e1}sica es la c\u{e1}mara y los controles originales. Est\u{e1}tica es la c\u{e1}mara con rat\u{f3}n. Suave a\u{f1}ade un seguimiento progresivo.",
			"Classica \u{e8} la telecamera con i comandi originali. Statica \u{e8} la telecamera col mouse. Fluida aggiunge un inseguimento dolce.",
		],
		CAMERAS,
		2,
		Cost::None,
		None,
		),
		[
			"Switching to or from Classic takes effect the next time the game starts.",
			"Passer \u{e0} ou depuis Classique prend effet au prochain lancement du jeu.",
			"Der Wechsel zu oder von Klassisch wirkt ab dem n\u{e4}chsten Spielstart.",
			"Cambiar a o desde Cl\u{e1}sica se aplica la pr\u{f3}xima vez que se inicie el juego.",
			"Il passaggio a o da Classica ha effetto al prossimo avvio del gioco.",
		],
	),
	row(
		"camspeed",
		slider(10, 200, 10, ""),
		["Camera speed", "Vitesse de la cam\u{e9}ra", "Kameratempo", "Velocidad de c\u{e1}mara", "Velocit\u{e0} telecamera"],
		[
			"How fast the stick turns the camera. 50 is the standard speed.",
			"Vitesse de rotation de la cam\u{e9}ra au stick. 50 est la vitesse normale.",
			"Wie schnell der Stick die Kamera dreht. 50 ist das normale Tempo.",
			"Con qu\u{e9} rapidez gira la c\u{e1}mara el stick. 50 es la velocidad normal.",
			"Quanto velocemente lo stick ruota la telecamera. 50 \u{e8} la velocit\u{e0} normale.",
		],
		NO,
		50,
		Cost::None,
		None,
	),
];

pub static MOUSE: [UiRow; 1] = [
	row(
	"mousespeed",
	slider(10, 200, 10, ""),
	["Mouse speed", "Vitesse de la souris", "Mausgeschwindigkeit", "Velocidad del rat\u{f3}n", "Velocit\u{e0} del mouse"],
	[
		"How fast the mouse turns the camera. Separate from the stick speed.",
		"Vitesse de rotation de la cam\u{e9}ra \u{e0} la souris. S\u{e9}par\u{e9}e de celle du stick.",
		"Wie schnell die Maus die Kamera dreht. Getrennt vom Stick-Tempo.",
		"Con qu\u{e9} rapidez gira la c\u{e1}mara el rat\u{f3}n. Aparte de la velocidad del stick.",
		"Quanto velocemente il mouse ruota la telecamera. Separata dalla velocit\u{e0} dello stick.",
	],
	NO,
	50,
	Cost::None,
	None,
),
];
