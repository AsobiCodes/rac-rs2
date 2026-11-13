//! [PC] HUD position (4:3 / Corners) and HUD scale (75..150 %) of R&C2 (parity row 27; pc_options keys `hudpos=corners`, `hudscale=N`, rows
//! in rac2/modern_rows_hud.rs). The HUD is cut by ELEMENT: the nanotech box (top left) and the bolt / weapon box (top right) are the corner
//! boxes, the weapon-level meter in the middle is not; the boxes are drawn anchored to the window's corners at the chosen scale and the
//! meter stays centred where the game draws it (hud_steps.rs does the drawing; the weapon-level meter's frame has a pointed tip under the
//! boxes' tails, so the boxes are stepped pieces, not rectangles). R&C2's picture is the game's own shape (16:9 with its widescreen row, 4:3
//! without). There is no HUD guard as R&C1's (retail::HudGuard): the frame's own pixels are measured instead (`note_frame`, `segs_of`): per
//! row of the HUD's top part the occupied column runs are found; runs in the outer parts of the picture are the boxes, the rest is the
//! middle. A picture whose boxes touch the middle element, go on far below the top, or whose outer columns are covered (a space-flight HUD, a
//! fade, a letterbox, a menu box that reaches into a corner) keeps the picture where the game puts it, so nothing is cut in two. Only over the
//! native world (host/frame.rs).
use crate::hud_steps::{quads as step_quads, Seg};
use rac_engine::gsfull::ovl_pipe::Frame;
use rac_engine::overlay::Quad;
use rac_engine::pcopts::PcOptions;
use rac_engine::viewer::targets::{RETAIL_HD_TEX, RETAIL_OV_TEX};
use std::sync::Mutex;

pub const KEY_POS: &str = "hudpos";
pub const KEY_SCALE: &str = "hudscale";

/// HUD position Corners (`hudpos=corners`; 4:3 = no key).
pub fn corners(o: &PcOptions) -> bool {
	o.extra_get(KEY_POS).is_some_and(|v| v.trim().eq_ignore_ascii_case("corners"))
}

/// The scale in percent: 75 .. 150 in steps of 5, 100 without a key.
pub fn scale_pct(o: &PcOptions) -> u32 {
	let v: u32 = o.extra_get(KEY_SCALE).and_then(|v| v.trim().parse().ok()).unwrap_or(100);
	(v.clamp(75, 150) + 2) / 5 * 5
}

/// The row value `id` has in `o` (None = not one of these rows).
pub fn value(o: &PcOptions, id: &str) -> Option<i32> {
	match id {
		KEY_POS => Some(corners(o) as i32),
		KEY_SCALE => Some(scale_pct(o) as i32),
		_ => None,
	}
}

/// Writes row `id` = `v` into `o` (the defaults write no key); false = not one of these rows.
pub fn apply(o: &mut PcOptions, id: &str, v: i32) -> bool {
	match id {
		KEY_POS => o.extra_set(KEY_POS, (v != 0).then(|| "corners".to_string())),
		KEY_SCALE => {
			let p = (v.clamp(75, 150) + 2) / 5 * 5;
			o.extra_set(KEY_SCALE, (p != 100).then(|| p.to_string()));
		}
		_ => return false,
	}
	true
}

/// What the newest picture says: its box segments (None = keep the picture in place) and the sizes of the two textures.
#[derive(Clone, Default)]
struct Info {
	segs: Option<Vec<Seg>>,
	ov: (u32, u32),
	hd: Option<(u32, u32)>,
}

static INFO: Mutex<Info> = Mutex::new(Info { segs: None, ov: (0, 0), hd: None });

/// Pixels of padding between a box and its cell's inner edge, and the width of the picture's outer columns that must be clear.
const BAND: usize = 2;
/// Columns closer than this (fraction of the picture's width) belong to one HUD element.
const RUN_GAP: f32 = 0.008;
/// The boxes' rows are looked for in this much of the picture's height; a box that goes on to the lower part of it is no corner box.
const SCAN: f32 = 0.20;
/// The lowest the boxes may end (fraction of the picture's height).
const MAX_H: f32 = 0.17;
/// A segment is at most this tall (fraction of the picture's height): the steps stay fine.
const SEG_H: f32 = 0.03;

/// One row of the picture: the left box piece's inner edge `l` and the right one's `r` (columns, from the outer edge; 0 = none) and the
/// room beside them: `lcap` = first column of the next run to the right of the left box, `rcap` = columns the right box's piece may take
/// before the run to its left.
struct RowInfo {
	l: usize,
	r: usize,
	lcap: usize,
	rcap: usize,
	/// The middle element's runs in this row (the next row's `row_info` keeps what touches them in the middle).
	mid: Vec<(usize, usize)>,
}

/// The column runs (start, end) of row `y`: columns closer than `gap` are one run.
fn runs_of(w: usize, y: usize, gap: usize, opaque: &dyn Fn(usize) -> bool) -> Vec<(usize, usize)> {
	let mut runs: Vec<(usize, usize)> = Vec::new();
	for x in 0..w {
		if !opaque(y * w + x) {
			continue;
		}
		match runs.last_mut() {
			Some(r) if x - r.1 <= gap => r.1 = x,
			_ => runs.push((x, x)),
		}
	}
	runs
}

/// Classifies the runs of one row; None = a box runs into the middle element in this row. `prev_mid`: the middle element's runs of the
/// row above: a run that touches one of them (columns within `gap`) is the middle element too, even off the centre (the weapon-level meter's
/// bottom frame ends in short pieces under its tips, which a side-only test took for box pieces and moved to the corners).
fn row_info(w: usize, runs: &[(usize, usize)], prev_mid: &[(usize, usize)], gap: usize) -> Option<RowInfo> {
	let wf = w as f32;
	let (mut l, mut lcap, mut r, mut rcap) = (0usize, w, 0usize, 0usize);
	let mut left_open = true;
	let mut mid = Vec::new();
	for &(s, e) in runs {
		let (sf, ef) = (s as f32, e as f32);
		let under_mid = prev_mid.iter().any(|&(ps, pe)| s <= pe + gap && e + gap >= ps);
		if ef <= 0.45 * wf && left_open && !under_mid {
			l = e + 1 + BAND;
		} else if sf >= 0.55 * wf && !under_mid {
			if r == 0 {
				r = w - s.saturating_sub(BAND);
			}
		} else {
			if sf < 0.12 * wf || ef > 0.88 * wf {
				return None; // a box runs into the middle element
			}
			left_open = false;
			if lcap == w {
				lcap = s;
			}
			rcap = rcap.max(e + 1);
			mid.push((s, e));
		}
	}
	// The next run to the right of the left box (a middle element, or the right box if there is no middle one) bounds the left piece.
	if lcap == w {
		if let Some(&(s, _)) = runs.iter().find(|&&(s, _)| s as f32 >= 0.55 * wf) {
			lcap = s;
		}
	}
	let rcap = if r > 0 && rcap == 0 { runs.iter().rev().find(|&&(_, e)| e as f32 <= 0.45 * wf).map_or(0, |&(_, e)| e + 1) } else { rcap };
	Some(RowInfo { l, r, lcap, rcap, mid })
}

/// Cuts the picture's HUD into the corner boxes' segments (see the module doc): None when the picture is no plain gameplay HUD.
fn segs_of(w: usize, h: usize, opaque: &dyn Fn(usize) -> bool) -> Option<Vec<Seg>> {
	if w < 64 || h < 64 {
		return None;
	}
	// The picture's outer columns, all rows (a fade or a letterbox is not clear).
	if !(0..h).all(|y| (0..BAND).all(|x| !opaque(y * w + x) && !opaque(y * w + w - 1 - x))) {
		return None;
	}
	let gap = ((RUN_GAP * w as f32) as usize).max(2);
	let scan = ((SCAN * h as f32) as usize).min(h);
	let mut rows: Vec<RowInfo> = Vec::with_capacity(scan);
	for y in 0..scan {
		let prev = rows.last().map_or(&[][..], |r: &RowInfo| &r.mid[..]);
		let ri = row_info(w, &runs_of(w, y, gap, opaque), prev, gap)?;
		rows.push(ri);
	}
	let last = rows.iter().rposition(|r| r.l > 0 || r.r > 0)?;
	if last + 1 >= scan || (last + 1) as f32 > MAX_H * h as f32 {
		return None; // the boxes go on below the top, or nothing is there
	}
	// Greedy segments: as many rows as keep every piece clear of the middle element's run in every row of the segment.
	let maxh = ((SEG_H * h as f32) as usize).max(1);
	let (mut segs, mut y0) = (Vec::new(), 0usize);
	while y0 <= last {
		let (mut l, mut r, mut lcap, mut rcap) = (0usize, 0usize, w, 0usize);
		let mut y1 = y0;
		while y1 <= last && y1 - y0 < maxh {
			let ri = &rows[y1];
			let (nl, nr, nlcap, nrcap) = (l.max(ri.l), r.max(ri.r), lcap.min(ri.lcap), rcap.max(ri.rcap));
			if nl > nlcap || (nr > 0 && w - nr < nrcap) {
				break;
			}
			(l, r, lcap, rcap) = (nl, nr, nlcap, nrcap);
			y1 += 1;
		}
		if y1 == y0 {
			return None; // one row alone has the box touching the middle
		}
		segs.push(Seg { v0: y0 as f32 / h as f32, v1: y1 as f32 / h as f32, l: l as f32 / w as f32, r: r as f32 / w as f32 });
		y0 = y1;
	}
	Some(segs)
}

/// A column-occupancy string of the top rows, for RAC2_HUDLOG=1 (calibrates the guard).
fn occupancy(w: usize, h: usize, opaque: &dyn Fn(usize) -> bool) -> String {
	let rows = ((0.20 * h as f32) as usize).min(h);
	(0..64).map(|b| if (0..rows).any(|y| (b * w / 64..(b + 1) * w / 64).any(|x| opaque(y * w + x))) { '#' } else { '.' }).collect()
}

/// Window thread, for every new GS picture (host/frame.rs): the segments of this picture's HUD. With the sharp HUD the HUD is in `f.hd`
/// (alpha = how much of the picture below shows), else in `f.rgba` (premultiplied coverage alpha).
pub fn note_frame(f: &Frame) {
	if !active(&crate::options::get()) {
		return;
	}
	let (w, h) = (f.w as usize, f.h as usize);
	let ov_opaque = |i: usize| f.rgba.get(i * 4 + 3).is_some_and(|&a| a > 8);
	let (segs, hd, occ) = match &f.hd {
		Some(hd) => {
			let op = |i: usize| hd.rgba.get(i).is_some_and(|p| p[3] < 247);
			(segs_of(hd.w as usize, hd.rows as usize, &op), Some((hd.w, hd.rows)), occupancy(hd.w as usize, hd.rows as usize, &op))
		}
		None => (segs_of(w, h, &ov_opaque), None, occupancy(w, h, &ov_opaque)),
	};
	if std::env::var_os("RAC2_HUDLOG").is_some() {
		eprintln!("hud_layout: field {} segs {segs:?} cols {occ}", f.field);
	}
	*INFO.lock().unwrap_or_else(|e| e.into_inner()) = Info { segs, ov: (f.w, f.h), hd };
}

/// Does the layout need the cell drawing at all (Corners, or a scale other than 100 %).
fn active(o: &PcOptions) -> bool {
	corners(o) || scale_pct(o) != 100
}

/// The overlay's and the sharp HUD's quads for the picture `rect` (x, y, w, h in window pixels) of shape `aspect` in a window `sw` x `sh`:
/// the boxes at the chosen position and scale. None = the picture as the game draws it (default options, a picture whose HUD runs the boxes
/// into the middle, a window narrower than the picture).
pub fn quads(sw: f32, sh: f32, aspect: f32, rect: [f32; 4], o: &PcOptions) -> Option<(Vec<Quad>, Vec<Quad>)> {
	if !active(o) {
		return None;
	}
	let info = INFO.lock().unwrap_or_else(|e| e.into_inner()).clone();
	let segs = info.segs.as_deref()?;
	let (c, s) = (corners(o), scale_pct(o) as f32 / 100.0);
	let Some((hw, hh)) = info.hd else {
		// Original HUD: the HUD is in the overlay itself.
		return Some((step_quads(RETAIL_OV_TEX, sw, sh, aspect, info.ov.0, info.ov.1, c, s, segs)?, Vec::new()));
	};
	// Sharp HUD: the HUD is the sharp picture; the overlay holds only the effects (fire, glows), which are not HUD and stay in place.
	let hd = step_quads(RETAIL_HD_TEX, sw, sh, aspect, hw, hh, c, s, segs)?;
	let plain = Quad { tex: RETAIL_OV_TEX, rect, uv: [0.0, 0.0, 1.0, 1.0], colour: [1.0; 4], angle: 0.0, corners: None };
	Some((vec![plain], hd))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_rows_round_trip_and_the_defaults_write_no_key() {
		let mut o = PcOptions::default();
		assert_eq!((value(&o, KEY_POS), value(&o, KEY_SCALE)), (Some(0), Some(100)));
		assert!(apply(&mut o, KEY_POS, 1) && apply(&mut o, KEY_SCALE, 125));
		assert_eq!((o.extra_get(KEY_POS), o.extra_get(KEY_SCALE)), (Some("corners"), Some("125")));
		assert_eq!((value(&o, KEY_POS), value(&o, KEY_SCALE)), (Some(1), Some(125)));
		assert!(apply(&mut o, KEY_POS, 0) && apply(&mut o, KEY_SCALE, 100));
		assert_eq!((o.extra_get(KEY_POS), o.extra_get(KEY_SCALE)), (None, None));
		assert!(!apply(&mut o, "hud", 1) && value(&o, "hud").is_none());
		// A hand-edited file snaps into the row's range.
		assert_eq!(scale_pct(&PcOptions::parse("hudscale=300\n")), 150);
		assert_eq!(scale_pct(&PcOptions::parse("hudscale=33\n")), 75);
		assert_eq!(scale_pct(&PcOptions::parse("hudscale=x\n")), 100);
	}

	const W: usize = 512;
	const H: usize = 288;
	fn rect(x0: usize, x1: usize, y0: usize, y1: usize) -> impl Fn(usize) -> bool {
		move |i: usize| (i % W >= x0 && i % W < x1 && i / W >= y0 && i / W < y1)
	}

	/// The combat HUD: nanotech box, the meter between with a tip under the box's tail, bolt box.
	fn combat(i: usize) -> bool {
		rect(10, 150, 0, 18)(i) || rect(10, 60, 18, 30)(i) || rect(160, 350, 0, 18)(i) || rect(120, 350, 18, 30)(i) || rect(370, 500, 0, 30)(i)
	}

	#[test]
	fn the_boxes_are_cut_from_the_meter_by_rows() {
		let segs = segs_of(W, H, &combat).expect("combat HUD");
		let at = |y: usize| *segs.iter().find(|s| (s.v0 * H as f32).round() as usize <= y && y < (s.v1 * H as f32).round() as usize).unwrap();
		let top = at(5);
		assert!((top.l * W as f32 - 152.0).abs() < 1.5 && (top.r * W as f32 - 144.0).abs() < 1.5, "{top:?}");
		// Where the meter's tip is (rows 18 .. 30) the left piece is the narrow lower box only.
		let low = at(25);
		assert!((low.l * W as f32 - 62.0).abs() < 1.5 && low.l * (W as f32) < 120.0, "{low:?}");
		assert!(segs.last().unwrap().v1 * (H as f32) <= 32.0 && segs[0].v0 == 0.0);
		assert!(segs.windows(2).all(|p| p[0].v1 == p[1].v0), "contiguous");
	}

	/// The meter's bottom frame ends in short pieces left and right of the centre below its tip rows (st23/0188): they stay with the
	/// meter, not with the boxes (they were the two thin lines beside the meter with Corners).
	#[test]
	fn the_meters_bottom_pieces_stay_in_the_middle() {
		let pic = |i: usize| combat(i) || rect(10, 90, 30, 33)(i) || rect(160, 200, 30, 33)(i) || rect(320, 360, 30, 33)(i);
		let segs = segs_of(W, H, &pic).expect("combat HUD with the meter's bottom pieces");
		let low = *segs.iter().find(|s| (s.v0 * H as f32).round() as usize <= 31 && 31 < (s.v1 * H as f32).round() as usize).unwrap();
		assert!((low.l * W as f32 - 92.0).abs() < 1.5, "left piece = the box's tail only: {low:?}");
		assert!(low.r * (W as f32) < (W - 360) as f32, "right piece clear of the meter piece: {low:?}");
	}

	#[test]
	fn pictures_that_would_tear_are_kept_in_place() {
		assert!(segs_of(W, H, &|i| combat(i) || rect(130, 400, 0, 20)(i)).is_none(), "the meter runs into the boxes");
		assert!(segs_of(W, H, &|i| combat(i) || rect(10, 100, 30, 80)(i)).is_none(), "a stack of boxes goes on below");
		assert!(segs_of(W, H, &|i| i / W < 20).is_none(), "a letterbox bar covers the outer columns");
		assert!(segs_of(32, 32, &|_| false).is_none(), "a tiny picture is not trusted");
		assert!(segs_of(W, H, &|_| false).is_none(), "no HUD, nothing to move");
	}

	#[test]
	fn default_options_use_no_layout() {
		assert!(quads(1920.0, 1080.0, 16.0 / 9.0, [0.0, 0.0, 1920.0, 1080.0], &PcOptions::default()).is_none());
	}
}
