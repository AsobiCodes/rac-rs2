//! [PC] The Button glyphs option of R&C2 (parity row 46; pc_options key `glyphs=playstation|letters|switch`, the row of the modern Options
//! page, rac2/modern_rows_hud.rs). The ten button cells of the game's three font sheets (256x128 PSMT8, 24x16 cells at v 96, u 0 24 48 72 96
//! 120 144 168 208 232: the same cells as R&C1's, [C] rac2/notes_menus.md 3) get pictures of OUR making (a grey disc / pill / dark disc with a
//! letter in a built-in 5x7 pixel font, drawn from code, no artwork): Letters = A bottom, B right, X left, Y top, bumpers "LB" / "RB", triggers
//! "LT" / "RT", sticks "L3" / "R3"; Switch swaps the letters (A right, B bottom, X top, Y left). PlayStation (default) = retail's cells, nothing
//! touched. Where: the engine's GS capture asks `font_patch` (pack_bind.rs) for every 256x128 PSMT8 transfer (gscap/font.rs); the sheet is
//! recognised by its texels against the level's fonts (`rac_formats::hud::level_fonts_raw` on the level's index, same table as R&C1's: the R&C2
//! level's asset header is R&C1's core index [C rac2 formats level.rs]), which the world loader hands over per level (`note_level`); the pictures
//! are quantised into the font's own 256-colour palette with the sheet's dithered background. The modern menus' own glyph drawing takes the style
//! from `style()` (modern_menu.rs `glyphs`). The keyboard key names of pc_prompts.rs replace the glyph codes in the text, so a keyboard player
//! never sees these cells; with a pad they show the button pictures.
use rac_engine::pcopts::PcOptions;
use rac_formats::level::Level;
use std::sync::{Arc, Mutex};

/// The names the options file uses, in the row's value order.
pub const NAMES: [&str; 3] = ["playstation", "letters", "switch"];

pub const CELL_W: usize = 24;
pub const CELL_H: usize = 16;
/// The art sits in the 16-wide window at x 2 of its cell (where retail's own glyphs are); the rest is the font's background.
const ART_X: usize = 2;

/// The ten button cells of the sheet: (retail glyph code, u, v) in the standard layout (Cross, Square, Triangle, Circle, L1, L2, R1, R2,
/// right stick, left stick).
pub const CELLS: [(u8, usize, usize); 10] = [
	(0x10, 0, 96),
	(0x13, 24, 96),
	(0x12, 48, 96),
	(0x11, 72, 96),
	(0x14, 96, 96),
	(0x16, 120, 96),
	(0x15, 144, 96),
	(0x17, 168, 96),
	(0x18, 208, 96),
	(0x19, 232, 96),
];

/// The style of an options file: 0 PlayStation, 1 Letters, 2 Letters (Switch layout); a missing or unknown value is 0.
pub fn style_of(o: &PcOptions) -> u8 {
	o.extra_get("glyphs").and_then(|v| NAMES.iter().position(|n| v.trim().eq_ignore_ascii_case(n))).unwrap_or(0) as u8
}

/// The style in force (live from the options in force).
pub fn style() -> u8 {
	style_of(&crate::options::get())
}

/// Writes the row into the options (PlayStation = no key).
pub fn set_style(o: &mut PcOptions, v: i32) {
	let v = v.clamp(0, 2) as usize;
	o.extra_set("glyphs", (v != 0).then(|| NAMES[v].to_string()));
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Pic {
	Disc(u8),
	Bumper([u8; 2]),
	Trigger([u8; 2]),
	Stick([u8; 2]),
}

/// The picture of cell `k` (the CELLS order) in `style` (1 Letters, 2 Switch): the face cells show the letter of the button in the
/// standard layout: Cross = bottom, Circle = right, Square = left, Triangle = top.
fn pic_of(k: usize, style: u8) -> Pic {
	let face = if style == 2 { *b"BAYX" } else { *b"ABXY" }; // bottom, right, left, top
	match k {
		0 => Pic::Disc(face[0]),
		3 => Pic::Disc(face[1]),
		1 => Pic::Disc(face[2]),
		2 => Pic::Disc(face[3]),
		4 | 6 => Pic::Bumper(if k == 4 { *b"LB" } else { *b"RB" }),
		5 | 7 => Pic::Trigger(if k == 5 { *b"LT" } else { *b"RT" }),
		8 => Pic::Stick(*b"R3"),
		_ => Pic::Stick(*b"L3"),
	}
}

fn glyph5x7(c: u8) -> [u8; 7] {
	match c {
		b'A' => [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
		b'B' => [0x1e, 0x11, 0x11, 0x1e, 0x11, 0x11, 0x1e],
		b'X' => [0x11, 0x11, 0x0a, 0x04, 0x0a, 0x11, 0x11],
		b'Y' => [0x11, 0x11, 0x0a, 0x04, 0x04, 0x04, 0x04],
		b'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f],
		b'R' => [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11],
		b'T' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
		b'3' => [0x0e, 0x11, 0x01, 0x06, 0x01, 0x11, 0x0e],
		_ => [0; 7],
	}
}

/// Signed distance to a rounded box (centre `c`, half size `h`, corner radius `r`).
fn rounded_box(px: f32, py: f32, c: (f32, f32), h: (f32, f32), r: f32) -> f32 {
	let (qx, qy) = ((px - c.0).abs() - (h.0 - r), (py - c.1).abs() - (h.1 - r));
	(qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r
}

const FILL: [u8; 4] = [228, 228, 228, 255];
const FILL_TRIGGER: [u8; 4] = [196, 196, 196, 255];
const RIM: [u8; 4] = [96, 96, 96, 255];
const INK: [u8; 4] = [16, 16, 16, 255];
const DARK: [u8; 4] = [64, 64, 64, 255];
const DARK_RIM: [u8; 4] = [210, 210, 210, 255];
const LIGHT: [u8; 4] = [244, 244, 244, 255];

/// A 24x16 RGBA cell (alpha 0 = the font's background shows).
fn render(pic: Pic) -> Vec<[u8; 4]> {
	let (shape, fill, rim, ink, text): ((f32, f32), [u8; 4], [u8; 4], [u8; 4], Vec<u8>) = match pic {
		Pic::Disc(c) => ((7.75, 7.75), FILL, RIM, INK, vec![c]),
		Pic::Bumper(t) => ((8.0, 5.5), FILL, RIM, INK, t.to_vec()),
		Pic::Trigger(t) => ((8.0, 7.5), FILL_TRIGGER, RIM, INK, t.to_vec()),
		Pic::Stick(t) => ((7.75, 7.75), DARK, DARK_RIM, LIGHT, t.to_vec()),
	};
	let radius = match pic {
		Pic::Disc(_) | Pic::Stick(_) => 7.75,
		Pic::Bumper(_) => 5.5,
		Pic::Trigger(_) => 4.0,
	};
	let centre = (ART_X as f32 + 8.0, 8.0 + if matches!(pic, Pic::Bumper(_) | Pic::Trigger(_)) { 0.5 } else { 0.0 });
	let mut out = vec![[0u8; 4]; CELL_W * CELL_H];
	for y in 0..CELL_H {
		for x in 0..CELL_W {
			let d = rounded_box(x as f32 + 0.5, y as f32 + 0.5, centre, shape, radius);
			if d <= 0.0 {
				out[y * CELL_W + x] = if d > -1.0 { rim } else { fill };
			}
		}
	}
	let w = text.len() * 5 + (text.len() - 1);
	let x0 = (centre.0 - w as f32 / 2.0 + 0.5).floor() as usize;
	let y0 = (centre.1 - 3.5).floor() as usize;
	for (i, &c) in text.iter().enumerate() {
		for (row, bits) in glyph5x7(c).iter().enumerate() {
			for col in 0..5 {
				if bits >> (4 - col) & 1 != 0 {
					out[(y0 + row) * CELL_W + x0 + i * 6 + col] = ink;
				}
			}
		}
	}
	out
}

/// The level's three font sheets (index bytes) and palettes.
struct Patch {
	sheets: Vec<Vec<u8>>,
	palettes: Vec<Vec<[u8; 4]>>,
	qwords: Vec<Vec<u128>>,
}

impl Patch {
	/// The sheet (0..3) whose texels start `data` (the upload).
	fn find(&self, data: &[u128]) -> Option<usize> {
		self.qwords.iter().position(|q| data.len() >= q.len() && data[..q.len()] == q[..])
	}
}

/// The fonts of the levels the world loader has handed over (a level's sheets are told apart by content; a few levels' fonts are equal).
static PATCHES: Mutex<Vec<Arc<Patch>>> = Mutex::new(Vec::new());
/// The cells of the last (patch, sheet, style) asked for. The patch is held (compared by pointer): an evicted patch's address cannot
/// be reused by a new one while the cache names it (review A7).
static CACHE: Mutex<Option<(Arc<Patch>, usize, u8, Arc<Vec<Vec<u8>>>)>> = Mutex::new(None);
/// At most this many levels' fonts are kept.
const KEEP: usize = 40;

/// The world loader (world.rs): level `l`'s font sheets become recognisable. A level whose sheets are already known adds nothing.
pub fn note_level(l: &Level) {
	let Ok(fonts) = rac_formats::hud::level_fonts_raw(l) else { return };
	let (sheets, palettes): (Vec<Vec<u8>>, Vec<Vec<[u8; 4]>>) = fonts.into_iter().unzip();
	let qwords = sheets.iter().map(|s| s.chunks(16).map(|c| c.iter().enumerate().fold(0u128, |w, (i, b)| w | (*b as u128) << (8 * i))).collect()).collect();
	let mut g = PATCHES.lock().unwrap_or_else(|e| e.into_inner());
	if g.iter().any(|p| p.sheets == sheets) {
		return;
	}
	if g.len() >= KEEP {
		g.remove(0);
	}
	g.push(Arc::new(Patch { sheets, palettes, qwords }));
}

/// The sheet's own background texels under the art (its dither: two palette indices, a checker of 4x4 blocks).
fn background(sheet: &[u8], u: usize, v: usize) -> [u8; 2] {
	let (a, b) = (sheet[v * 256 + u + 18], sheet[v * 256 + u + 20]);
	if ((u + 18) / 4 + v / 4) % 2 == 0 {
		[a, b]
	} else {
		[b, a]
	}
}

fn nearest(px: [u8; 4], palette: &[[u8; 4]]) -> u8 {
	let mut best = (u32::MAX, 0u8);
	for (i, c) in palette.iter().enumerate().take(256) {
		let d: u32 = (0..4).map(|k| (px[k] as i32 - c[k] as i32).unsigned_abs()).sum();
		if d < best.0 {
			best = (d, i as u8);
		}
	}
	best.1
}

/// The ten cells' index bytes (24x16 each) of sheet `which` of `patch` in `style`.
fn cells(patch: &Patch, which: usize, style: u8) -> Vec<Vec<u8>> {
	let (sheet, palette) = (&patch.sheets[which], &patch.palettes[which]);
	CELLS
		.iter()
		.enumerate()
		.map(|(k, &(_, u, v))| {
			let bg = background(sheet, u, v);
			render(pic_of(k, style))
				.iter()
				.enumerate()
				.map(|(i, px)| if px[3] == 0 { bg[((u + i % CELL_W) / 4 + (v + i / CELL_W) / 4) % 2] } else { nearest(*px, palette) })
				.collect()
		})
		.collect()
}

/// Writes `cells` into the sheet's qwords.
fn patch_qwords(data: &mut [u128], cells: &[Vec<u8>]) {
	for (k, cell) in cells.iter().enumerate() {
		let (u, v) = (CELLS[k].1, CELLS[k].2);
		for y in 0..CELL_H {
			for x in 0..CELL_W {
				let i = (v + y) * 256 + u + x;
				let b = 8 * (i % 16);
				data[i / 16] = (data[i / 16] & !(0xffu128 << b)) | ((cell[y * CELL_W + x] as u128) << b);
			}
		}
	}
}

/// `Pack::font_patch` (the engine's GS capture, gscap/font.rs): a 256x128 PSMT8 upload that is one of the known font sheets gets the
/// cells of the chosen style; the sheet as sent is returned when it patched and `want_orig` is set. PlayStation: None, nothing touched.
pub fn font_patch(data: &mut [u128], want_orig: bool) -> Option<Vec<u128>> {
	let style = style();
	if style == 0 {
		return None;
	}
	let patches: Vec<Arc<Patch>> = PATCHES.lock().unwrap_or_else(|e| e.into_inner()).clone();
	let (patch, which) = patches.iter().find_map(|p| p.find(data).map(|w| (p, w)))?;
	let cells = {
		let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
		match c.as_ref().filter(|e| Arc::ptr_eq(&e.0, patch) && (e.1, e.2) == (which, style)) {
			Some(e) => e.3.clone(),
			None => {
				let made = Arc::new(cells(patch, which, style));
				*c = Some((patch.clone(), which, style, made.clone()));
				made
			}
		}
	};
	let orig = want_orig.then(|| data.to_vec());
	patch_qwords(data, &cells);
	orig
}

#[cfg(test)]
mod tests {
	use super::*;

	fn sheet() -> Patch {
		let sheet: Vec<u8> = (0..256 * 128).map(|i| ((i / 4) % 7) as u8).collect();
		let palette: Vec<[u8; 4]> = (0..256).map(|i| [i as u8, i as u8, i as u8, 255]).collect();
		let qwords = vec![sheet.chunks(16).map(|c| c.iter().enumerate().fold(0u128, |w, (i, b)| w | (*b as u128) << (8 * i))).collect()];
		Patch { sheets: vec![sheet], palettes: vec![palette], qwords }
	}

	#[test]
	fn the_style_comes_from_the_options_file() {
		assert_eq!(style_of(&PcOptions::parse("")), 0);
		assert_eq!(style_of(&PcOptions::parse("glyphs=letters\n")), 1);
		assert_eq!(style_of(&PcOptions::parse("glyphs=Switch\n")), 2);
		assert_eq!(style_of(&PcOptions::parse("glyphs=bogus\n")), 0);
		let mut o = PcOptions::default();
		set_style(&mut o, 2);
		assert_eq!(o.extra_get("glyphs"), Some("switch"));
		set_style(&mut o, 0);
		assert_eq!(o.extra_get("glyphs"), None, "PlayStation = no key");
	}

	#[test]
	fn switch_swaps_the_face_letters_and_every_cell_has_art() {
		assert_eq!((pic_of(0, 1), pic_of(3, 1), pic_of(1, 1), pic_of(2, 1)), (Pic::Disc(b'A'), Pic::Disc(b'B'), Pic::Disc(b'X'), Pic::Disc(b'Y')));
		assert_eq!((pic_of(0, 2), pic_of(3, 2), pic_of(1, 2), pic_of(2, 2)), (Pic::Disc(b'B'), Pic::Disc(b'A'), Pic::Disc(b'Y'), Pic::Disc(b'X')));
		for k in 0..CELLS.len() {
			assert!(render(pic_of(k, 1)).iter().filter(|p| p[3] != 0).count() > 60, "cell {k} draws a picture");
		}
	}

	#[test]
	fn patching_changes_only_the_ten_cells() {
		let p = sheet();
		let mut data = p.qwords[0].clone();
		let orig = data.clone();
		assert_eq!(p.find(&data), Some(0));
		patch_qwords(&mut data, &cells(&p, 0, 1));
		assert_ne!(data, orig);
		// A texel outside the cells' rows is unchanged.
		for i in 0..96 * 256 / 16 {
			assert_eq!(data[i], orig[i], "row block {i} (above v 96) untouched");
		}
		// Every changed texel is inside a 24x16 cell of CELLS.
		for i in 0..256 * 128 {
			let (b, o) = ((data[i / 16] >> (8 * (i % 16))) as u8, (orig[i / 16] >> (8 * (i % 16))) as u8);
			if b != o {
				let (x, y) = (i % 256, i / 256);
				assert!(CELLS.iter().any(|&(_, u, v)| x >= u && x < u + CELL_W && y >= v && y < v + CELL_H), "({x},{y})");
			}
		}
	}
}
