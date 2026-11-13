//! Tests of the R&C2 modern page door (modern_open.rs) on a made-up RAM laid out like level 23's dump (runs/w2d/l23/ram00190.bin).
use super::*;

const CROSS_B: u16 = 0x40;
const G: u32 = 0x1ff9c0;
const BASE: u32 = 0x1a90cc;
const LIST_BASE: u32 = 0x1fb8000;
const OPTIONS: u32 = 0x28ec70;
const PAUSE: u32 = 0x28c508;
const TABLE: u32 = 0x294618;
/// The Options rows' text ids and the page ids they open (HelpDesk, Save, Load, Controls, Sound, Camera; Quit opens none).
const ROWS: [(u32, u32); 7] = [(0xba4, 19), (0xba5, 30), (0xba6, 31), (0xb72, 22), (0xba8, 32), (0xba9, 29), (0xbac, 0)];

fn wr(ram: &mut [u8], a: u32, v: u32) {
	ram[a as usize..a as usize + 4].copy_from_slice(&v.to_le_bytes());
}

/// RAM with the code shapes at `code`, the Options list current (`page`), list depth `depth`, row `row`.
fn ram(code: u32, page: u32, depth: u32, row: u32) -> Vec<u8> {
	let mut m = vec![0u8; 0x200_0000];
	// andi v0,a0,0x10; beqz v0,+0xd; lui v0,0x20; addiu v0,v0,-0x640 (G = 0x1ff9c0); lw v1,0x14(v0); lw v1,0xe0(v1)
	for (k, w) in [0x3082_0010u32, 0x1040_000d, 0x3c02_0020, 0x2442_f9c0, 0x8c43_0014, 0x8c63_00e0].into_iter().enumerate() {
		wr(&mut m, code + 4 * k as u32, w);
	}
	// lui v0,0x1b; lw v0,-0x6f34(v0); nop; lui a0,3; ori a0,a0,0xc210
	for (k, w) in [0x3c02_001bu32, 0x8c42_90cc, 0, 0x3c04_0003, 0x3484_c210].into_iter().enumerate() {
		wr(&mut m, code + 0x100 + 4 * k as u32, w);
	}
	wr(&mut m, G + 0x14, page);
	wr(&mut m, PAUSE + 0xe4, 2);
	wr(&mut m, OPTIONS + 0xe0, PAUSE);
	wr(&mut m, OPTIONS + 0xe4, 8);
	wr(&mut m, BASE, LIST_BASE);
	let l = LIST_BASE + 0x3c210;
	wr(&mut m, l + 0x31c, depth);
	wr(&mut m, l + 0x1b8 + 4 * depth, row);
	wr(&mut m, l + 0x314, TABLE);
	for (i, (text, id)) in ROWS.into_iter().enumerate() {
		let item = TABLE + 0x14 * i as u32;
		wr(&mut m, item + 4, text);
		if id != 0 {
			let target = 0x290000 + 0x200 * i as u32;
			wr(&mut m, item + 0xc, target);
			wr(&mut m, target + 0xe0, OPTIONS);
			wr(&mut m, target + 0xe4, id);
		}
	}
	m
}

fn snap_of(m: &[u8]) -> Option<Snap> {
	snap(m, &find(m)?)
}

#[test]
fn finds_the_menu_globals_by_code_shape() {
	let m = ram(0x35f088, OPTIONS, 1, 5);
	assert_eq!(find(&m), Some(Locs { g: G, base: BASE, at: 0x35f088 }));
	assert_eq!(find(&vec![0u8; 0x200_0000]), None, "no shape, no hook");
}

#[test]
fn cross_on_the_camera_row_opens_the_modern_page() {
	let m = ram(0x35f088, OPTIONS, 1, 5);
	let s = snap_of(&m).unwrap();
	assert_eq!((s.page_id, s.depth, s.text, s.target_id), (8, 1, 0xba9, 29));
	assert!(decide(Some(&s), CROSS_B, true, true));
	assert!(decide(Some(&s), CROSS_B | 0x4000, true, true), "a Down riding along");
}

#[test]
fn cross_on_any_other_row_passes_through() {
	for row in [0u32, 1, 2, 3, 4, 6] {
		let m = ram(0x35f088, OPTIONS, 1, row);
		assert!(!decide(snap_of(&m).as_ref(), CROSS_B, true, true), "row {row}");
	}
}

#[test]
fn no_cross_classic_or_refused_pipes_never_open() {
	let m = ram(0x35f088, OPTIONS, 1, 5);
	let s = snap_of(&m);
	assert!(!decide(s.as_ref(), 0, true, true), "no Cross");
	assert!(!decide(s.as_ref(), 0x20, true, true), "Circle");
	assert!(!decide(s.as_ref(), 0x10, true, true), "Triangle");
	assert!(!decide(s.as_ref(), CROSS_B, false, true), "Classic menu style");
	assert!(!decide(s.as_ref(), CROSS_B, true, false), "UI pipes refused");
}

#[test]
fn stale_list_after_the_menu_closed_is_not_a_door() {
	// After Start closes the pause menu, G+0x14 is 0 but the list keeps depth 1 / row 5 [C: runs/w2d/c23 fields 190 / 260].
	let m = ram(0x35f088, 0, 1, 5);
	assert_eq!(snap_of(&m), None);
	assert!(!decide(None, CROSS_B, true, true));
	// The pause list current (id 2) with the stale Options row: not the Options page.
	let m = ram(0x35f088, PAUSE, 1, 5);
	assert!(!decide(snap_of(&m).as_ref(), CROSS_B, true, true));
	// A list deeper than the Options list (a sub page's list).
	let m = ram(0x35f088, OPTIONS, 2, 5);
	assert!(!decide(snap_of(&m).as_ref(), CROSS_B, true, true));
}

#[test]
fn a_moved_level_is_found_again() {
	let a = ram(0x35f088, OPTIONS, 1, 5);
	let b = ram(0x350580, OPTIONS, 1, 5);
	assert_eq!(find(&a).map(|l| l.at), Some(0x35f088));
	assert_eq!(find(&b).map(|l| l.at), Some(0x350580));
	assert_eq!(g_at(&b, 0x35f088), None, "the cached place no longer holds the shape: `snap_now` scans again");
}
