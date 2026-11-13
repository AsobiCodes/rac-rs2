use super::*;
use crate::modern_map_page::{self as page, Facts, Hit};
use rac_engine::ui::font::FontMeasure;
use rac_engine::ui::input::{Device, GlyphSet};
use rac_engine::ui::prim::Symbol;

fn put(ram: &mut [u8], a: u32, v: u32) {
	ram[a as usize..a as usize + 4].copy_from_slice(&v.to_le_bytes());
}

fn none(_: u32) -> Option<String> {
	None
}

/// A made-up RAM like the level-24 map state (notes 8.2): page at 0x286000, the list order, the Feltzin entry selected.
fn ram(sel: u32) -> Vec<u8> {
	let mut r = vec![0u8; 0x200_0000];
	for (k, l) in [1u32, 2, 3, 4, 5, 6, 24, 7, 8, 9, 11, 10, 12, 13].iter().enumerate() {
		put(&mut r, ORDER + 4 * k as u32, *l);
	}
	put(&mut r, 0x286000 + SEL_OFF, sel);
	r
}

#[test]
fn reads_the_order_the_cursor_and_the_english_fallback() {
	let r = ram(5);
	let m = read_with(&r, 0x286000, 24, &none).expect("map");
	assert_eq!((m.entries.len(), m.cur, m.here), (14, 4, 24));
	assert_eq!((m.entries[4].planet.as_str(), m.entries[4].loc.as_str()), ("Feltzin System", "Thug Rendezvous"));
	assert_eq!(m.entries[6].level, 24, "the Ship Shack sits after Notak");
	let m = read_with(&r, 0x286000, 24, &|id| (id == 0xb45 + 5).then(|| "Feltzin X".to_string())).unwrap();
	assert_eq!(m.entries[4].planet, "Feltzin X", "the game's text wins");
	assert!(read_with(&ram(40), 0x286000, 24, &none).is_none(), "a level not in the list");
}

#[test]
fn every_level_has_a_name_and_the_late_ones_use_their_text_ids() {
	// [b45 P3] B10: no "Planet N" for any level of the game.
	for l in 0..=26 {
		assert!(english(l).is_some_and(|e| !e.0.is_empty()), "level {l}");
	}
	let mut r = ram(25);
	put(&mut r, ORDER + 4 * 14, 25);
	let m = read_with(&r, 0x286000, 24, &|id| (id == 0x1b23).then(|| "Wupash".to_string())).unwrap();
	assert_eq!((m.entries[14].planet.as_str(), m.entries[14].loc.as_str()), ("Wupash", ""), "level 25 = text 0x1b23, no location id");
	let m = read_with(&ram(13), 0x286000, 24, &none).unwrap();
	assert_eq!(m.entries[13].planet, "Boldan");
}

#[test]
fn not_a_map_without_the_mode_and_the_page() {
	let mut r = ram(5);
	put(&mut r, crate::modern_screen::LEVEL, 24);
	assert!(read(&r, &none).is_none(), "mode 0 and no menu page");
}

#[test]
fn pulse_walks_down_and_up_then_travels() {
	let mut p = Pulse::new(Ask::Row(6, true));
	let seq: Vec<u16> = (0..4).map(|_| p.step(4, 14)).collect();
	assert_eq!(seq, vec![DOWN, DOWN, 0, 0]);
	let seq: Vec<u16> = (0..4).map(|_| p.step(5, 14)).collect();
	assert_eq!(seq, vec![DOWN, DOWN, 0, 0]);
	let seq: Vec<u16> = (0..4).map(|_| p.step(6, 14)).collect();
	assert_eq!(seq, vec![CROSS_B, CROSS_B, 0, 0]);
	assert!(!p.busy());
	let mut p = Pulse::new(Ask::Row(0, false));
	assert_eq!(p.step(2, 14), UP);
	let mut p = Pulse::new(Ask::Row(3, true));
	let seq: Vec<u16> = (0..100).map(|_| p.step(0, 14)).collect();
	assert!(!seq.contains(&CROSS_B) && !p.busy(), "a cursor that never moves: the walk gives up without a Cross");
}

fn facts(m: &Map) -> Facts<'_> {
	Facts { map: m, focus: None, scroll: 0, lang: 0, device: Device::Keyboard, glyphs: GlyphSet::PlayStation, art: false, t: 0.0, pic: None }
}

#[test]
fn page_lists_the_planets_and_clicks_hit_them() {
	let m = read_with(&ram(5), 0x286000, 24, &none).unwrap();
	let b = page::build(&facts(&m), &FontMeasure).expect("page");
	assert_eq!((b.sel, b.count, b.dots.len()), (4, 14, 14));
	let (i, rect) = b.rows[2];
	assert_eq!(b.hit(rect.cx(), rect.cy()), Hit::Row(i));
	let go = b.go.unwrap();
	assert_eq!(b.hit(go.cx(), go.cy()), Hit::Go);
	let d = b.dots[5].1;
	assert_eq!(b.hit(d.cx(), d.cy()), Hit::Row(5));
	assert!(b.hints.iter().any(|h| h.0 == Symbol::Triangle));
	let mut m2 = m.clone();
	m2.entries.clear();
	assert!(page::build(&facts(&m2), &FontMeasure).is_none());
}

#[test]
fn texts_have_every_language() {
	for t in page::all() {
		assert!(t.iter().all(|s| !s.is_empty()));
	}
	let c = rac_engine::ui::prim::Rect::new(0.0, 0.0, 600.0, 300.0);
	let (x, y, _) = page::marker_pos(13, c);
	assert!((0.0..=600.0).contains(&x) && (0.0..=300.0).contains(&y));
	assert_eq!(page::marker_pos(27, c).0, c.cx(), "an unknown level sits in the middle");
	// [b45 P3] B11: every level 0..26 has its own spot, none on another's.
	let pts: Vec<(f32, f32)> = (0..=26).map(|l| page::marker_pos(l, c)).map(|(x, y, _)| (x, y)).collect();
	for (i, a) in pts.iter().enumerate() {
		assert!((0.0..=600.0).contains(&a.0) && (0.0..=300.0).contains(&a.1), "level {i} inside the chart");
		for (j, b) in pts.iter().enumerate().skip(i + 1) {
			assert!((a.0 - b.0).hypot(a.1 - b.1) > 4.0, "levels {i} and {j} overlap");
		}
	}
}

// [b48 Y3] B12: the planet picture's hole sits inside the card, under the planet text and above the Travel button, for every aspect.
#[test]
fn picture_hole_fits_the_card_and_keeps_the_crop_shape() {
	let m = read_with(&ram(5), 0x286000, 24, &none).unwrap();
	for a in [4.0 / 3.0, 16.0 / 9.0, 21.0 / 9.0] {
		let h = page::pic_hole(a);
		assert!(h.x >= page::DETAIL.x && h.right() <= page::DETAIL.right(), "aspect {a}: inside the card width");
		let b = page::build(&Facts { pic: Some(a), ..facts(&m) }, &FontMeasure).unwrap();
		assert_eq!(b.hole, Some(h));
		assert!(h.y >= page::DETAIL.y + 330.0 && h.bottom() <= b.go.unwrap().y - 4.0, "aspect {a}: between the text and the button");
		assert!(h.w / h.h > 3.0, "aspect {a}: a wide strip");
	}
	assert!(page::build(&facts(&m), &FontMeasure).unwrap().hole.is_none(), "no overlay this frame: no hole");
}
