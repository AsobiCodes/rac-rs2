use super::*;
use crate::modern_vendor_hand as hand;
use crate::modern_vendor_page::{self as page, Facts, Hit};
use rac_engine::ui::font::FontMeasure;
use rac_engine::ui::input::{Device, GlyphSet};
use rac_engine::ui::prim::Symbol;

fn put(ram: &mut [u8], a: u32, v: u32) {
	ram[a as usize..a as usize + 4].copy_from_slice(&v.to_le_bytes());
}

fn put16(ram: &mut [u8], a: u32, v: u16) {
	ram[a as usize..a as usize + 2].copy_from_slice(&v.to_le_bytes());
}

/// A made-up level-2 RAM laid out like bank state 0125's vendor (runs/vendmap/r0125 ram00100: rows 30a 42a 22 26a).
fn level2(stage: u32, cur: u32) -> Vec<u8> {
	let mut ram = vec![0u8; 0x200_0000];
	let w = L2;
	put(&mut ram, crate::modern_screen::LEVEL, 2);
	put(&mut ram, MODE, VENDOR_MODE);
	put(&mut ram, crate::modern_screen::BOLTS, 26399);
	put(&mut ram, w.cur, cur);
	put(&mut ram, w.stage, stage);
	put(&mut ram, w.qty, 22);
	for (k, &(slot, kind, text)) in [(30u32, 1u32, 0xc05u32), (42, 1, 0xce3), (22, 0, 0x879), (26, 1, 0x5d1)].iter().enumerate() {
		let a = w.rows + k as u32 * ROW_STRIDE;
		(put(&mut ram, a, slot), put(&mut ram, a + 4, kind), put(&mut ram, a + 8, text), put(&mut ram, a + 0xc, ROW_MARK));
	}
	for s in 0..64u32 {
		ram[(SLOT_MAP + s) as usize] = s as u8;
	}
	ram[(SLOT_MAP + 30) as usize] = 60; // the Lancer's upgrade
	for (id, price, unit, max) in [(60u32, 1000u32, 1u16, 300u16), (42, 1000, 50, 8), (22, 5000, 4, 35), (26, 15000, 5, 40)] {
		let r = w.table + id * REC_SIZE;
		(put(&mut ram, r + 0x80, price), put16(&mut ram, r + 0x88, unit), put16(&mut ram, r + 0x8e, max), put16(&mut ram, r + 8, 0xa00 + id as u16));
	}
	(put(&mut ram, AMMO + 4 * 30, 128), put(&mut ram, AMMO + 4 * 42, 8), put(&mut ram, AMMO + 4 * 26, 18));
	ram
}

fn names(id: u32) -> Option<String> {
	Some(format!("t{id:x}"))
}

#[test]
fn reads_the_level_2_shop_through_the_slot_map() {
	let ram = level2(0, 2);
	let s = read_with(&ram, &L2, &names).expect("shop up");
	assert_eq!((s.cur, s.stage, s.bolts, s.rows.len()), (2, 0, 26399, 4));
	assert_eq!(s.rows[0], Row { slot: 30, ammo: true, name: "ta3c".into(), price: 1, owned: 128, max: 300 }, "slot 30 reads record 60");
	assert_eq!((s.rows[2].ammo, s.rows[2].price), (false, 5000));
	assert!(s.rows[1].full() && !s.rows[1].can_buy(s.bolts));
	assert_eq!(s.rows[3].most(26399), 22);
	assert!(!s.prompt());
}

#[test]
fn no_shop_no_page() {
	let mut ram = level2(0, 2);
	put(&mut ram, MODE, 3);
	assert!(read(&ram, &names).is_none(), "pause menu, not a shop");
	let mut ram = level2(0, 2);
	put(&mut ram, crate::modern_screen::LEVEL, 24);
	assert!(read(&ram, &names).is_none(), "the ship shop / a level without the quantity prompt keeps the retail shop");
	let ram = level2(0, 9);
	assert!(read_with(&ram, &L2, &names).is_none(), "cursor past the list");
	let mut ram = level2(0, 2);
	put(&mut ram, L2.table + 22 * REC_SIZE + 0x80, 1);
	assert!(read_with(&ram, &L2, &names).is_none(), "the table check holds");
}

#[test]
fn browse_pad_turns_up_down_into_the_strip_and_keeps_the_prompt() {
	assert_eq!(browse_pad(UP | CROSS_B, false), LEFT | CROSS_B);
	assert_eq!(browse_pad(DOWN, false), RIGHT);
	assert_eq!(browse_pad(UP | LEFT, true), UP | LEFT);
}

#[test]
fn pulse_walks_the_short_way_and_buys() {
	let mut p = Pulse::new(Ask::Row(0, true));
	let seq: Vec<u16> = (0..4).map(|_| p.step(3, 4)).collect();
	assert_eq!(seq, vec![LEFT, LEFT, 0, 0], "3 -> 0 walks back: the strip does not wrap");
	let seq: Vec<u16> = (0..4).map(|_| p.step(0, 4)).collect();
	assert_eq!(seq, vec![CROSS_B, CROSS_B, 0, 0]);
	assert!(!p.busy());
	// 0 -> 3 walks forward; a cursor that never moves gives the walk up after n + 2 presses, with no Cross.
	let mut p = Pulse::new(Ask::Row(3, true));
	assert_eq!(p.step(0, 4), RIGHT);
	let seq: Vec<u16> = (0..40).map(|_| p.step(0, 4)).collect();
	assert_eq!(seq.iter().filter(|&&b| b == RIGHT).count(), 2 * 6 - 1, "6 walk presses of 2 fields");
	assert!(!seq.contains(&CROSS_B) && !p.busy());
	let mut p = Pulse::new(Ask::Press(TRIANGLE_B));
	let seq: Vec<u16> = (0..5).map(|_| p.step(1, 4)).collect();
	assert_eq!(seq, vec![TRIANGLE_B, TRIANGLE_B, 0, 0, 0]);
}

fn facts(s: &Shop) -> Facts<'_> {
	Facts { shop: s, focus: None, scroll: 0, lang: 0, device: Device::Keyboard, glyphs: GlyphSet::PlayStation, model: None }
}

#[test]
fn page_lists_the_rows_and_the_prompt_takes_the_clicks() {
	let s = read_with(&level2(0, 2), &L2, &names).unwrap();
	let b = page::build(&facts(&s), &FontMeasure).expect("page");
	assert_eq!((b.rows.len(), b.sel, b.prompt), (4, 2, false));
	assert!(b.buy.is_some(), "26399 bolts buy the 5000-bolt weapon");
	let (r, rect) = b.rows[1];
	assert_eq!(b.hit(rect.cx(), rect.cy()), Hit::Row(r));
	let s = read_with(&level2(2, 3), &L2, &names).unwrap();
	let b = page::build(&facts(&s), &FontMeasure).unwrap();
	assert!(b.prompt && b.less.is_some() && b.confirm.is_some());
	let c = b.confirm.unwrap();
	assert_eq!(b.hit(c.cx(), c.cy()), Hit::Confirm);
	let (_, rect) = b.rows[0];
	assert_ne!(b.hit(rect.cx(), rect.cy()), Hit::Row(0), "rows are under the prompt");
	assert_eq!(page::first_row(0, 9, 14), 3);
	assert_eq!(page::commas(1_000_000), "1,000,000");
}

/// The real replay dump, when this PC has it: bank state 0125 with the Blitz Gun ammo prompt up (field 100).
#[test]
fn bank_vendor_dump_reads() {
	let Ok(ram) = std::fs::read("E:/ratchet/rac2/runs/vendmap/r0125/ram00100.bin") else { return };
	let t = Texts::find(&ram);
	let s = read(&ram, &|id| t.and_then(|t| t.get(&ram, id))).expect("shop up in the dump");
	assert_eq!((s.cur, s.stage, s.qty, s.rows.len()), (3, 2, 22, 4));
	assert_eq!((s.rows[3].slot, s.rows[3].ammo, s.rows[3].price, s.rows[3].max), (26, true, 5, 40));
	if t.is_some() {
		let names: Vec<&str> = s.rows.iter().map(|r| r.name.as_str()).collect();
		assert_eq!(names, ["Heavy Lancer", "Gravity Bomb", "Chopper", "Blitz Gun"], "names from the records (slot 30 = record 60)");
	}
}

/// The finder on the bank vendor states of levels 2, 4, 6, 11, 12 (when this PC has them): one prompt each, the same level-2 words as
/// the replay dumps, and the rows run to the zero row (the ammo rows of weapons found elsewhere carry no 0xcdb).
#[test]
fn finder_reads_every_bank_vendor_level() {
	let base = "E:/ratchet/rac2/states/fg1";
	let cases = [("0125", 2u32, 4usize, 2usize), ("0187", 4, 7, 3), ("0260", 6, 12, 9), ("0473", 11, 17, 8), ("0555", 12, 18, 17)];
	for (st, level, n, cur) in cases {
		let Ok(ram) = std::fs::read(format!("{base}/{st}/eeMemory.bin")) else { return };
		let (_, w) = crate::modern_vendor_find::find(&ram, level).expect(st);
		if level == 2 {
			assert_eq!(w, L2, "level 2 = the replay-dump words");
		}
		let s = read(&ram, &names).expect(st);
		assert_eq!((s.rows.len(), s.cur, s.stage), (n, cur, 0), "{st}");
		assert!(s.rows.iter().all(|r| r.max > 0), "{st}: every row reads a record");
	}
}

#[test]
fn finder_needs_the_code() {
	let ram = level2(0, 2);
	assert!(crate::modern_vendor_find::find(&ram, 2).is_none(), "no code, no words");
}

/// Square in the list = ONE request to the hand (`modern_vendor_hand.rs` buys it all at the game's next shop frame; the purchase
/// itself is proven against retail there); nothing to fill or the quantity prompt up = no request.
#[test]
fn square_asks_the_hand_once_and_the_page_lists_it_as_a_row() {
	let _l = hand::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
	hand::cancel();
	let s = read_with(&level2(0, 2), &L2, &names).unwrap();
	let mut g = Game::default();
	let (out, s1) = drive(&mut g, s.clone(), SQUARE_B);
	assert!(s1.buying && hand::pending(), "Square in the list asks the hand");
	assert_eq!(out & SQUARE_B, 0, "the Square press is the page's");
	let (out, _) = drive(&mut g, s.clone(), CROSS_B | TRIANGLE_B);
	assert_eq!(out & crate::modern_open::MENU_BTNS, 0, "until the game has bought, no menu press reaches it");
	hand::cancel();
	let mut full = s.clone();
	(full.rows[0].owned, full.rows[3].owned) = (300, 40);
	let (_, s2) = drive(&mut Game::default(), full, SQUARE_B);
	assert!(!s2.buying && !hand::pending(), "nothing to fill: no request");
	let prompt = read_with(&level2(2, 2), &L2, &names).unwrap();
	let (out, s3) = drive(&mut Game::default(), prompt, SQUARE_B);
	assert!(!s3.buying && !hand::pending(), "the quantity prompt is up: no request");
	assert_eq!(out & SQUARE_B, SQUARE_B, "the prompt keeps its buttons");
	let b = page::build(&facts(&s), &FontMeasure).unwrap();
	assert!(!b.hints.iter().any(|h| h.0 == Symbol::Square), "no hint-bar button: the list row (R&C1)");
	let r = b.all_row.expect("the Buy all ammo row");
	assert!(r.bottom() < b.rows[0].1.y, "above the stock");
	assert_eq!(b.hit(r.cx(), r.cy()), Hit::AllRow);
	assert!(!b.buyall);
}

#[test]
fn up_on_row_0_focuses_buy_all_cross_runs_it_down_gives_it_back() {
	let _l = hand::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
	hand::cancel();
	let s = read_with(&level2(0, 0), &L2, &names).unwrap();
	let mut g = Game::default();
	let (out, s1) = drive(&mut g, s.clone(), UP);
	assert_eq!(out & (LEFT | UP), 0, "the Up press is the page's");
	assert!(s1.all_focus);
	let (out, s1) = drive(&mut g, s.clone(), UP);
	assert_eq!(out, 0, "held: still eaten");
	assert!(s1.all_focus);
	let b = page::build(&facts(&s1), &FontMeasure).unwrap();
	assert!(b.buyall && b.buy.is_some(), "its own card with a live button");
	let btn = b.buy.unwrap();
	assert_eq!(b.hit(btn.cx(), btn.cy()), Hit::BuyAll);
	let (_, s2) = drive(&mut g, s.clone(), 0);
	assert!(s2.all_focus && !s2.buying);
	let (out, s3) = drive(&mut g, s.clone(), CROSS_B);
	assert!(s3.buying && hand::pending(), "Cross on the row asks the hand");
	assert_eq!(out & CROSS_B, 0, "the Cross is the row's, not a buy of the row under it");
	hand::cancel();
	let mut g = Game::default();
	drive(&mut g, s.clone(), UP);
	drive(&mut g, s.clone(), 0);
	let (out, s4) = drive(&mut g, s.clone(), DOWN);
	assert_eq!(out & RIGHT, 0);
	assert!(!s4.all_focus, "Down gives the focus back to row 0");
	let s5 = read_with(&level2(0, 2), &L2, &names).unwrap();
	let mut g = Game::default();
	let (out, s6) = drive(&mut g, s5, UP);
	assert_eq!((out & LEFT, s6.all_focus), (LEFT, false), "Up on row 2 walks the game's list");
}

#[test]
fn refill_spends_the_bolts_row_by_row() {
	let mut s = read_with(&level2(0, 2), &L2, &names).unwrap();
	let f = crate::modern_vendor_buyall::refill(&s);
	assert_eq!((f.rows.clone(), f.rounds, f.cost), (vec![0, 3], 172 + 22, 172 + 110), "Lancer + Blitz Gun: 26117 bolts left");
	s.bolts = 180;
	let f = crate::modern_vendor_buyall::refill(&s);
	assert_eq!((f.rows, f.rounds, f.cost), (vec![0, 3], 172 + 1, 177), "the Lancer first, the rest buys one Blitz round");
}
