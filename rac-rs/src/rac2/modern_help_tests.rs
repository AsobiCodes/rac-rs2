use super::*;
use crate::modern_help_page as page;
use rac_engine::ui::font::FontMeasure;
use rac_engine::ui::prim::Prim;

fn facts(cur: usize) -> Facts {
	Facts {
		rows: (0..7).map(|k| format!("Row {k}")).collect(),
		descs: (0..7).map(|k| format!("Description of row {k}. It is long enough to need a second line in the panel, maybe more.")).collect(),
		cur,
		tabs: (0..7).map(|k| format!("t{k}")).collect(),
		..Default::default()
	}
}

#[test]
fn gate_page_7_under_the_root() {
	assert!(gate(7, 2, 0x1fb8000));
	assert!(!gate(7, 2, 0) && !gate(7, 3, 1) && !gate(5, 2, 1) && !gate(52, 7, 1));
}

#[test]
fn rows_and_descriptions_line_up() {
	assert_eq!(NAMES.len(), DESCS.len());
	assert_eq!(NAMES[2], 0xb72); // Controls
	assert_eq!(DESCS[1], NAMES[1] + 1); // Help Log
}

#[test]
fn hit_rects_and_crop() {
	let b = page::build(&facts(2), &FontMeasure, 16.0 / 9.0);
	assert_eq!(b.rows.len(), 7);
	for (i, r) in b.rows.iter().enumerate() {
		assert_eq!(b.row_at(r.cx(), r.cy()), Some(i));
	}
	assert_eq!(b.row_at(5.0, 5.0), None);
	// the crop is inside the 512 x 448 overlay for every row and the hole inside the detail panel
	for k in 0..7 {
		let s = page::icon_src(k);
		assert!(s[0] + s[2] <= 512.0 && s[1] + s[3] <= 448.0, "row {k}");
	}
	assert!(b.uv.iter().all(|v| (0.0..=1.0).contains(v)));
	assert!(b.hole.w > 0.0 && b.hole.x >= 648.0 && b.hole.right() <= 1232.0);
	assert!(b.prims.iter().any(|p| matches!(p, Prim::Text(t) if t.text == "Row 2" && t.px > 30.0)));
}

#[test]
fn list_drive_is_the_roots() {
	let mut d = Drive::default();
	(d.target, d.confirm) = (Some(2), true);
	let seq: Vec<u16> = (0..12).map(|_| d.step(0, 7)).collect();
	assert_eq!(&seq[..2], &[crate::modern_screen::DOWN; 2]);
}
