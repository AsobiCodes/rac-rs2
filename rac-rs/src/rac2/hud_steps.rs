//! [PC] The stepped corner boxes of R&C2's HUD (parity row 27, HUD position Corners / HUD scale; used by hud_layout.rs).
//! R&C2's top boxes are not rectangles that a vertical cut separates from the middle element: the weapon-level meter's frame has a pointed
//! tip that reaches left under the nanotech box's tail (and right under the bolt box's), so no single cell width keeps both whole. The
//! picture's top rows are therefore cut into SEGMENTS (rows `v0 .. v1`, fractions of the picture's height), each with the width `l` of the
//! left box piece and `r` of the right box piece in that row range (fractions of the picture's width, 0 = no box there): the meter's tip
//! rows get a narrower box piece than the tail rows. Every piece is drawn anchored to its window corner (Corners) or its picture corner (4:3)
//! at the chosen scale, all pieces with the same transform, so the box moves whole; everything in the middle of a segment stays where the game
//! draws it (centred); what a piece leaves behind is filled with a stretch of its inner column (HUD free by construction, hud_layout.rs
//! `segs_of`), and the margins beside a 16:9 picture in a wider window with a stretch of its outer column.
use rac_engine::overlay::Quad;

/// Rows `v0 .. v1` (fractions of the picture's height) with a left box piece `l` wide and a right box piece `r` wide (0 = none).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seg {
	pub v0: f32,
	pub v1: f32,
	pub l: f32,
	pub r: f32,
}

/// The quads of texture `tex` (`tw` x `th` texels, the picture `ar` wide / high) in a window `sw` x `sh`: segments `segs` (contiguous from
/// row 0, at least one piece) at `scale`. None = the window is narrower than the picture or the segments are not usable.
pub fn quads(tex: usize, sw: f32, sh: f32, ar: f32, tw: u32, th: u32, corners: bool, scale: f32, segs: &[Seg]) -> Option<Vec<Quad>> {
	let h = segs.last()?.v1;
	let pw = sh * ar;
	let x0 = (sw - pw) * 0.5;
	if sh <= 0.0 || ar <= 0.0 || x0 < -0.5 || !(0.0..=1.0).contains(&h) || segs.iter().any(|s| s.v1 <= s.v0 || s.l + s.r > 0.95) {
		return None;
	}
	let x0 = x0.max(0.0);
	let (lmax, rmax) = segs.iter().fold((0.0f32, 0.0f32), |a, s| (a.0.max(s.l), a.1.max(s.r)));
	let (lmin, rmin) = segs.iter().fold((1.0f32, 1.0f32), |a, s| (if s.l > 0.0 { a.0.min(s.l) } else { a.0 }, if s.r > 0.0 { a.1.min(s.r) } else { a.1 }));
	let s = if lmax + rmax > 0.0 { scale.min(if corners { sw } else { pw } / ((lmax + rmax) * pw)) } else { scale };
	let (ax_l, ax_r) = if corners { (0.0, sw) } else { (x0, x0 + pw) };
	let (htx, hty) = (0.5 / tw.max(1) as f32, 0.5 / th.max(1) as f32);
	let q = |rect: [f32; 4], uv: [f32; 4]| Quad { tex, rect, uv, colour: [1.0; 4], angle: 0.0, corners: None };
	let mut out = Vec::new();
	// The margins beside the picture: the stretched outer columns.
	if x0 > 0.5 {
		out.push(q([0.0, 0.0, x0, sh], [htx, 0.0, htx, 1.0]));
		out.push(q([x0 + pw, 0.0, x0, sh], [1.0 - htx, 0.0, 1.0 - htx, 1.0]));
	}
	// Rows below the boxes: the picture as the game draws it.
	if h < 1.0 {
		out.push(q([x0, h * sh, pw, (1.0 - h) * sh], [0.0, h, 1.0, 1.0]));
	}
	let (mut dest_l, mut dest_r) = (Vec::new(), Vec::new());
	let (mut last_l, mut last_r) = (None, None);
	for g in segs {
		let (y, hh) = (g.v0 * sh, (g.v1 - g.v0) * sh);
		// The middle of the rows, in place.
		let mw = (1.0 - g.l - g.r) * pw;
		if mw > 0.01 {
			out.push(q([x0 + g.l * pw, y, mw, hh], [g.l, g.v0, 1.0 - g.r, g.v1]));
		}
		let (dy, dh) = (g.v0 * sh * s, hh * s);
		if g.l > 0.0 {
			// Beside the piece: from its new inner edge (the narrowest piece's) to the old inner edge, a stretch of its inner column.
			let (from, to) = (ax_l + lmin * pw * s, x0 + g.l * pw);
			if to - from > 0.01 {
				out.push(q([from, y, to - from, hh], [g.l - htx, g.v0, g.l - htx, g.v1]));
			}
			dest_l.push(q([ax_l, dy, g.l * pw * s, dh], [0.0, g.v0, g.l, g.v1]));
			last_l = Some(g);
		}
		if g.r > 0.0 {
			let (from, to) = (x0 + (1.0 - g.r) * pw, ax_r - rmin * pw * s);
			if to - from > 0.01 {
				out.push(q([from, y, to - from, hh], [1.0 - g.r + htx, g.v0, 1.0 - g.r + htx, g.v1]));
			}
			dest_r.push(q([ax_r - g.r * pw * s, dy, g.r * pw * s, dh], [1.0 - g.r, g.v0, 1.0, g.v1]));
			last_r = Some(g);
		}
	}
	// Under a piece that shrank (scale < 1): a stretch of its bottom row down to where the box used to end.
	if let Some(g) = last_l.filter(|_| s < 1.0) {
		out.push(q([ax_l, g.v1 * sh * s, g.l * pw * s, g.v1 * sh * (1.0 - s)], [0.0, g.v1 - hty, g.l, g.v1 - hty]));
	}
	if let Some(g) = last_r.filter(|_| s < 1.0) {
		out.push(q([ax_r - g.r * pw * s, g.v1 * sh * s, g.r * pw * s, g.v1 * sh * (1.0 - s)], [1.0 - g.r, g.v1 - hty, 1.0, g.v1 - hty]));
	}
	out.extend(dest_l);
	out.extend(dest_r);
	Some(out)
}

#[cfg(test)]
mod tests {
	use super::*;

	/// The picture of a 1920x1080 window with a 16:9 picture: a tall left box piece for the first rows, a narrow one below.
	fn segs() -> Vec<Seg> {
		vec![Seg { v0: 0.0, v1: 0.08, l: 0.31, r: 0.30 }, Seg { v0: 0.08, v1: 0.13, l: 0.20, r: 0.30 }]
	}

	fn find(qs: &[Quad], rect_x: f32, rect_y: f32) -> Option<&Quad> {
		qs.iter().find(|q| (q.rect[0] - rect_x).abs() < 0.5 && (q.rect[1] - rect_y).abs() < 0.5)
	}

	#[test]
	fn the_middle_stays_in_place_and_the_pieces_go_to_the_corners() {
		let qs = quads(1, 2560.0, 1080.0, 16.0 / 9.0, 1920, 1080, true, 1.0, &segs()).expect("quads");
		let (pw, x0) = (1920.0, 320.0);
		// The meter's rows: the first segment's middle starts at the left box's end, the second one's further left (the tip stays whole).
		let m1 = find(&qs, x0 + 0.31 * pw, 0.0).expect("middle 1");
		assert_eq!(m1.uv, [0.31, 0.0, 0.70, 0.08]);
		let m2 = find(&qs, x0 + 0.20 * pw, 0.08 * 1080.0).expect("middle 2");
		assert_eq!(m2.uv, [0.20, 0.08, 0.70, 0.13]);
		// The pieces: left at the window's left edge, right at its right edge, same size as in the picture at 100 %.
		let l1 = qs.iter().find(|q| q.rect[0] == 0.0 && (q.rect[2] - 0.31 * pw).abs() < 0.5 && q.uv[0] == 0.0).expect("left piece 1");
		assert_eq!(l1.uv, [0.0, 0.0, 0.31, 0.08]);
		let r1 = qs.iter().find(|q| q.uv == [0.70, 0.0, 1.0, 0.08] && (q.rect[0] + q.rect[2] - 2560.0).abs() < 0.5).expect("right piece 1");
		assert!((r1.rect[2] - 0.30 * pw).abs() < 0.5);
		// A margin stretch exists on both sides.
		assert!(find(&qs, 0.0, 0.0).is_some() && find(&qs, x0 + pw, 0.0).is_some());
	}

	#[test]
	fn the_scale_grows_the_pieces_about_their_corner_and_stops_where_they_would_touch() {
		let qs = quads(1, 2560.0, 1080.0, 16.0 / 9.0, 1920, 1080, true, 1.4, &segs()).expect("quads");
		let l2 = qs.iter().find(|q| q.uv == [0.0, 0.08, 0.20, 0.13]).expect("left piece 2");
		assert!((l2.rect[0]).abs() < 0.5 && (l2.rect[1] - 0.08 * 1080.0 * 1.4).abs() < 0.5 && (l2.rect[2] - 0.20 * 1920.0 * 1.4).abs() < 0.5);
		// Too big to fit side by side: the scale is cut where the two pieces meet.
		let wide = [Seg { v0: 0.0, v1: 0.1, l: 0.45, r: 0.45 }];
		let q3 = quads(1, 1920.0, 1080.0, 16.0 / 9.0, 1920, 1080, true, 1.5, &wide).expect("quads");
		let l = q3.iter().find(|q| q.uv == [0.0, 0.0, 0.45, 0.1]).unwrap();
		assert!(l.rect[2] <= 960.5, "{:?}", l.rect);
	}

	#[test]
	fn a_window_narrower_than_the_picture_or_bad_segments_give_none() {
		assert!(quads(1, 1000.0, 1080.0, 16.0 / 9.0, 1920, 1080, true, 1.0, &segs()).is_none());
		assert!(quads(1, 2560.0, 1080.0, 16.0 / 9.0, 1920, 1080, true, 1.0, &[]).is_none());
		assert!(quads(1, 2560.0, 1080.0, 16.0 / 9.0, 1920, 1080, true, 1.0, &[Seg { v0: 0.1, v1: 0.1, l: 0.2, r: 0.2 }]).is_none());
	}
}
