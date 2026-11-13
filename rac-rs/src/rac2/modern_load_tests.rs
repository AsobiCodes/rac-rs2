use super::*;
use crate::modern_load_page::{self as page, Built};
use crate::modern_pages::Look;
use rac_engine::ui::kit::page::{Panel, HINT_Y};
use rac_engine::ui::prim::{Prim, Rect, Rgba};

/// The user's card as retail's slot table held it (run ex1, field 700): Oozla x3 and Joba, the zero clock's "95/00/00".
fn user_ram() -> Vec<u8> {
	let mut ram = vec![0u8; 0x20_0000];
	let recs: [(u32, u32, u32); 4] = [(1, 7541, 0x1c09e), (1, 7783, 0x1441e), (1, 7541, 0x1d57c), (0xb, 61140, 0x149b8f)];
	for (i, (level, bolts, time)) in recs.iter().enumerate() {
		let a = (SLOTS + SLOT_SIZE * i as u32) as usize;
		for (o, v) in [(0, *level), (4, *bolts), (0xc, *time), (0x10, 0x1500_0000), (0x14, 0x0000_9500)] {
			ram[a + o..a + o + 4].copy_from_slice(&u32::to_le_bytes(v));
		}
	}
	ram
}

#[test]
fn the_slot_table_reads_as_retail_prints_it() {
	let mut ram = user_ram();
	let r = raw(&ram, 0).expect("slot 1");
	assert_eq!((r.level, r.bolts), (1, 7541));
	assert_eq!(clock(r.time), "00:31");
	assert_eq!(clock(raw(&ram, 1).unwrap().time), "00:23");
	assert_eq!(clock(raw(&ram, 2).unwrap().time), "00:33");
	assert_eq!(clock(raw(&ram, 3).unwrap().time), "06:15");
	assert_eq!(date(r.date), "95/00/00", "the zero clock's date, as the retail page shows it");
	assert_eq!(date([0x12, 0x11, 0x26]), "12/11/26", "day / month / year, BCD");
	assert_eq!(raw(&ram, 3).unwrap().level, 0xb);
	// level word -1 = an empty slot (retail draws "EMPTY", 0x2de814)
	let a = (SLOTS + SLOT_SIZE * 2) as usize;
	ram[a..a + 4].copy_from_slice(&u32::MAX.to_le_bytes());
	assert_eq!(raw(&ram, 2), None);
}

#[test]
fn bolts_have_thousands_commas() {
	assert_eq!(bolts_text(7541), "7,541");
	assert_eq!(bolts_text(61140), "61,140");
	assert_eq!(bolts_text(999), "999");
	assert_eq!(bolts_text(0), "0");
	assert_eq!(bolts_text(1_234_567), "1,234,567");
	assert_eq!(bolts_text(12_345_678), "9,999,999");
	assert_eq!(bolts_text_for(7541, 3), "7.541");
	assert_eq!(bolts_text_for(1_234_567, 5), "1.234.567");
	assert_eq!(bolts_text_for(7541, 2), "7541");
	assert_eq!(bolts_text_for(500, 3), "500");
}

#[test]
fn only_the_load_and_save_records_are_slot_pages() {
	assert_eq!(mode_of(LOAD_KIND), Some(Mode::Load));
	assert_eq!(mode_of(SAVE_KIND), Some(Mode::Save));
	assert_eq!(mode_of(0), None);
	assert_eq!(mode_of(0x1aa698), None, "the title root's descriptor");
}

/// A slot cursor like retail's: Up / Down move it on a new press and stop at the ends.
fn run_walk(mut w: Walk, mut cur: usize, fields: usize) -> (usize, Vec<u16>) {
	let (mut prev, mut presses) = (0u16, Vec::new());
	for _ in 0..fields {
		let b = w.step(cur);
		let new = b & !prev;
		prev = b;
		if new & DOWN != 0 {
			cur = (cur + 1).min(N_SLOTS - 1);
		}
		if new & UP != 0 {
			cur = cur.saturating_sub(1);
		}
		if new & !(UP | DOWN) != 0 {
			presses.push(new);
		}
	}
	(cur, presses)
}

#[test]
fn a_walk_steps_without_wrapping_then_presses_once() {
	// slot 4 from slot 1: three Downs (the wrapping Drive would press Up once and stall at the top)
	let (cur, presses) = run_walk(Walk::new(Some(3), CROSS_BIT), 0, 40);
	assert_eq!(cur, 3);
	assert_eq!(presses, vec![CROSS_BIT], "Cross once, after the cursor arrived");
	let (cur, presses) = run_walk(Walk::new(Some(0), 0), 3, 40);
	assert_eq!((cur, presses.len()), (0, 0), "a hover walks Up and presses nothing");
	let (cur, presses) = run_walk(Walk::new(None, TRIANGLE_BIT), 2, 10);
	assert_eq!((cur, presses), (2, vec![TRIANGLE_BIT]), "back: Triangle where the cursor is");
	let mut w = Walk::new(Some(2), 0);
	let _ = run_walk(w, 2, 10);
	assert_eq!(w.step(2), 0, "already there: nothing pressed");
	assert!(!w.busy());
}

#[test]
fn a_walk_that_never_arrives_gives_up() {
	// the cursor does not move (a press lost): at most MAX_STEPS Downs, then the request is dropped
	let mut w = Walk::new(Some(3), CROSS_BIT);
	let mut downs = 0;
	let mut prev = 0;
	for _ in 0..200 {
		let b = w.step(0);
		if b & !prev & DOWN != 0 {
			downs += 1;
		}
		prev = b;
	}
	assert_eq!(downs, MAX_STEPS as usize);
	assert!(!w.busy());
	// and no press goes out after the give-up (B16)
	let mut w = Walk::new(Some(3), CROSS_BIT);
	for _ in 0..200 {
		assert_eq!(w.step(0) & CROSS_BIT, 0, "a given-up walk never presses");
	}
}

#[test]
fn the_clicked_card_shows_selected_at_once() {
	assert_eq!(shown(Some((3, 1000)), 0, 1000), (3, Some((3, 1000))));
	assert_eq!(shown(Some((3, 1000)), 1, 1400).0, 3, "not card 2 on the way");
	assert_eq!(shown(Some((3, 1000)), 3, 1450), (3, None), "arrived: retail's cursor again");
	assert_eq!(shown(Some((3, 1000)), 1, 1000 + AIM_MS), (1, None), "a dropped step lets retail's cursor win");
	assert_eq!(shown(None, 2, 5), (2, None));
	assert_eq!(click_press(2, 2), CROSS_BIT, "a click on the selected card takes it");
	assert_eq!(click_press(1, 2), 0, "a click on another card selects it");
	assert!(click_acts(1, 2, None) && !click_acts(3, 2, Some((3, 0))), "a click on the card being walked to does nothing");
}

fn facts(title: bool, mode: Mode) -> Facts {
	let s = |p: &str, b: &str, t: &str| Some(Slot { planet: format!("Planet {p}"), site: "Megacorp Outlet".into(), bolts: b.into(), time: t.into(), date: "95/00/00".into() });
	Facts {
		mode,
		title,
		cur: 1,
		caption: if mode == Mode::Load { "Load Game" } else { "Save Game" }.into(),
		slots: vec![s("Oozla", "7,541", "00:31"), s("Oozla", "7,783", "00:23"), None, s("Joba", "61,140", "06:15")],
		empty: "EMPTY".into(),
		no_save: (title && mode == Mode::Save).then(|| "Continue without saving".to_string()),
		rows: if title { vec!["New Game".into(), "Load".into(), "Options".into()] } else { (0..7).map(|i| format!("Tab {i}")).collect() },
		site: "Dobbo".into(),
		bolts: 1234,
		ask: None,
		pic: true,
		fade: 0.0,
	}
}

#[test]
fn the_picture_waits_for_retails_picture_widget() {
	// dbox 699 / pz 2620: state 2 = shown; dbox 701..786: -1 after the box (the empty well, ld4_yes 762..765)
	assert!(pic_shown(false, Some((2, 0))));
	assert!(pic_shown(false, Some((4, 0))), "the second buffer");
	assert!(!pic_shown(false, Some((u32::MAX, 0))), "reset: the empty well");
	assert!(!pic_shown(false, Some((0, 0))));
	assert!(!pic_shown(false, Some((1, 0))), "the first picture loading");
	assert!(!pic_shown(false, Some((2, u32::MAX))), "hidden");
	assert!(!pic_shown(true, Some((2, 0))), "the switch field: the old page");
	assert!(!pic_shown(false, None), "no picture widget");
	// found among the record's widget words by its picture table (dbox 699: record 0x260c88 +0x100 = 0x25db30)
	let mut ram = vec![0u8; 0x30_0000];
	let put = |ram: &mut Vec<u8>, a: u32, v: u32| ram[a as usize..a as usize + 4].copy_from_slice(&v.to_le_bytes());
	for (o, v) in [(0xf4, 0x260db8), (0xf8, 0x25da38), (0xfc, 0x260e08), (0x100, 0x25db30)] {
		put(&mut ram, 0x260c88 + o, v);
	}
	assert_eq!(pic_widget(&ram, 0x260c88), None, "no picture table: no widget");
	put(&mut ram, 0x25db30 + 0x30, PIC_TABLE);
	put(&mut ram, 0x25db30 + PIC_STATE, 2);
	assert_eq!(pic_widget(&ram, 0x260c88), Some((2, 0)));
	put(&mut ram, 0x25db30 + PIC_STATE, u32::MAX);
	assert_eq!(pic_widget(&ram, 0x260c88), Some((u32::MAX, 0)));
}

#[test]
fn the_title_and_pause_slot_pages_are_told_apart_by_page_id_and_parent() {
	// title (run ld1_box RAM): id 31 under the root (id 2), Load / Save by the record word
	assert_eq!(slot_mode(true, 31, Some(2), LOAD_KIND), Some(Mode::Load));
	assert_eq!(slot_mode(true, 31, Some(2), SAVE_KIND), Some(Mode::Save));
	assert_eq!(slot_mode(true, 31, Some(2), 0x1ab3c8), None, "an unknown record word at the title");
	assert_eq!(slot_mode(true, 26, Some(31), SAVE_KIND), None, "the box itself");
	// pause (pz RAM, Oozla): Options (id 8) -> Save = id 30 (word 0x1ab3c8), Load = id 31 (0x1ab398)
	assert_eq!(slot_mode(false, 30, Some(8), 0x1ab3c8), Some(Mode::Save));
	assert_eq!(slot_mode(false, 31, Some(8), 0x1ab398), Some(Mode::Load));
	assert_eq!(slot_mode(false, 30, Some(2), 0), None, "not under the Options list");
	assert_eq!(slot_mode(false, 31, None, LOAD_KIND), None);
	assert_eq!(slot_mode(false, 29, Some(8), 0), None, "the Camera page");
}

#[test]
fn retail_box_kinds_and_their_lines() {
	assert_eq!(box_text(1), (Some(ID_OVERWRITE), true), "ld1_box: G+0xe4 = 1");
	assert_eq!(box_text(0), (Some(ID_OVERWRITE), true));
	assert_eq!(box_text(3), (Some(ID_SAVING), false), "saving: no answer");
	assert_eq!(box_text(4), (None, true));
	assert_eq!(box_text(u32::MAX), (None, true));
	let raw = b"A Ratchet and Clank 2 save game already exists in the save file you have selected.\x01\x01Do you wish to overwrite this?\0junk";
	assert_eq!(split_lines(raw), vec!["A Ratchet and Clank 2 save game already exists in the save file you have selected.", "", "Do you wish to overwrite this?"]);
	assert_eq!(split_lines(b"Saving Data.\x01\x01Please do not remove"), vec!["Saving Data.", "", "Please do not remove"]);
	assert_eq!(split_lines(b"\0"), vec![""]);
}

#[test]
fn the_card_message_is_a_dialog_without_buttons_and_the_box_wins() {
	assert_eq!(ask_of(None, None), None);
	assert_eq!(ask_of(None, Some(0xc0d)), Some((Some(0xc0d), false)), "dbox 787: Saving Data in the info panel");
	assert_eq!(ask_of(None, Some(0xc0e)), Some((Some(0xc0e), false)), "Loading");
	assert_eq!(ask_of(Some(1), None), Some((Some(ID_OVERWRITE), true)));
	assert_eq!(ask_of(Some(1), Some(0xc0d)), Some((Some(ID_OVERWRITE), true)), "the box is the page up");
}

#[test]
fn page_switches_show_the_next_page_and_the_title_close_holds() {
	// dbox: 700 up on the Save page, 701 switching to the box, 761 back, 788 the title closed (its page fading out)
	assert_eq!(see(UP_STATE, 0x260c88, 0), See::Page(0x260c88));
	assert_eq!(see(SWITCHING, 0, 0x260ea8), See::Page(0x260ea8));
	assert_eq!(see(SWITCHING, 0, 0x260c88), See::Page(0x260c88));
	assert_eq!(see(0, 0x260c88, 0), See::Hold);
	// The pause closed with a page current: Hold, `keep` / `pause_exit` let go at once unless it closed after a load.
	assert_eq!(see(0, 0x287428, 0), See::Hold);
	assert_eq!(see(UP_STATE, 0x287428, 0), See::Page(0x287428), "pz 2620: the pause Save page");
	assert_eq!(see(UP_STATE, 0, 0), See::Gone);
	assert_eq!(see(SWITCHING, 0, 0), See::Gone);
	assert_eq!(see(0, 0, 0), See::Gone);
	assert_eq!(see(1, 0x260c88, 0), See::Gone, "an unknown state");
}

/// One pause load as runs\ld6_pzlcl saw it (G 0x1ff8c0): the card message 442..550, cleared at 551 with the page up, the close and
/// the level change together at 552, black from 553. `msg` per field as RAM has it; the shown message and whether the page stays.
#[test]
fn a_pause_load_keeps_the_loading_page_until_retails_black() {
	let q = Question { lines: vec!["Loading Data.".into()], yes_no: false };
	// 550: the message on; 551: off in RAM with the page up = kept one field (retail's frozen picture still shows it).
	let (a550, c550) = linger(Mode::Load, Some(q.clone()), Some(&q), false);
	assert_eq!((a550.as_ref(), c550), (Some(&q), false));
	let (a551, c551) = linger(Mode::Load, None, a550.as_ref(), c550);
	assert_eq!((a551.as_ref(), c551), (Some(&q), true), "551: the message stays");
	let (a, c) = linger(Mode::Load, None, a551.as_ref(), c551);
	assert_eq!((a, c), (None, false), "kept once only: a page that stays up loses it the next field");
	assert_eq!(linger(Mode::Save, None, Some(&q), false), (None, false), "the Save page's Saving Data ends with retail's (b2 722)");
	let ask = Question { lines: vec![], yes_no: true };
	assert_eq!(linger(Mode::Load, None, Some(&ask), false), (None, false), "a question is never kept");
	// 552: closed (held 0) with the level change begun this field = the page; 553 (held 1): let go = retail's black.
	assert!(pause_exit(true, 0, Some(0)));
	assert!(!pause_exit(true, 1, Some(0)));
	assert!(pause_exit(false, 0, Some(0)), "the close came with the level change: a load even without the kept message");
	// A close after the message whose level change comes later: the page stays up to that field; FADE fields at most without it.
	assert!(pause_exit(true, 2, None) && pause_exit(true, 3, Some(3)) && !pause_exit(true, 4, Some(3)));
	assert!(!pause_exit(true, FADE, None));
	// Start on a Load page (no message, no level change): let go at once, as before.
	assert!(!pause_exit(false, 0, None));
}

#[test]
fn the_pause_page_draws_over_short_world_gaps_only() {
	assert_eq!(world_step(None, true, 300), (Some(300), true), "the world drawn this frame");
	assert_eq!(world_step(Some(300), false, 300 + WORLD_GAP as u64), (Some(300), true), "retail's page-only fields while it loads");
	assert_eq!(world_step(Some(300), false, 301 + WORLD_GAP as u64), (Some(300), false));
	assert_eq!(world_step(None, false, 300), (None, false), "never a world drawn (renderer off): the classic page as before");
	assert_eq!(world_step(Some(300), false, 200), (Some(300), true), "fields done never run back; a lower count is no gap");
}

/// The redraws of fx2's q1 on b44 f3 (RAC2_FRAMELOG rows 2378..2461: GS fields done, world drawn, frames): the page must draw on
/// every one (the old count of display frames dropped it from the 7th no-world redraw of 455 on: 456 classic, 457 without a page).
#[test]
fn many_redraws_of_one_field_without_the_world_never_drop_the_page() {
	let frames: [(u64, bool, u32); 8] = [(453, true, 3), (453, false, 1), (454, false, 53), (455, true, 5), (455, false, 8), (456, false, 12), (457, true, 4), (458, true, 4)];
	let mut last = None;
	for (done, world, n) in frames {
		for k in 0..n {
			let (l, ok) = world_step(last, world, done);
			assert!(ok, "field {done} redraw {k}");
			last = l;
		}
	}
	// Seven fields' pictures without the world (a real gap, not one field redrawn): the classic picture as before.
	assert!(!(459..466).fold((last, true), |(l, _), d| world_step(l, false, d)).1);
}

/// One title load as runs\ld5_mousedbg saw it: the close at field 891, the level change at 898, retail's loading screen at 916.
fn exit_at(field: u32) -> Option<f32> {
	let held = field - 891;
	let change_at = (field >= 898).then_some(898 - 891);
	let load_screen = if field < 898 { 0 } else if field < 916 { 0xffff } else { (field - 916) as u16 };
	load_exit(held, change_at, load_screen)
}

#[test]
fn a_title_load_keeps_the_page_through_retails_exit_to_its_loading_screen() {
	// 891..897: retail loads the save, its "Loading Data" page stays on screen (the classic page showed 895..898 on rc2_ld_5).
	for k in 891..=898 {
		assert_eq!(exit_at(k), Some(0.0), "field {k}: the page, no fade");
	}
	// 899..902: retail's page at 4/5 .. 1/5 brightness, black from 903 (shots).
	assert_eq!(exit_at(899), Some(0.2));
	assert!((exit_at(901).unwrap() - 0.6).abs() < 1e-6);
	for k in 903..916 {
		assert_eq!(exit_at(k), Some(1.0), "field {k}: black, as retail");
	}
	assert_eq!(exit_at(916), None, "retail's loading screen: the page lets go");
	assert_eq!(exit_at(1000), None);
	// The guard: a close that never reaches the loading screen lets go (never the classic page over a stuck hold forever).
	assert_eq!(load_exit(EXIT_CAP, None, 0), Some(0.0));
	assert_eq!(load_exit(EXIT_CAP + 1, None, 0), None);
	assert_eq!(load_exit(EXIT_CAP + 1, Some(10), 0xffff), None);
}

#[test]
fn retails_fade_is_one_black_rect_over_the_whole_page() {
	let b = page::build(&facts(true, Mode::Load), 1, Look::default());
	let (_, prims) = rac_engine::ui::kit::page_draw::frame(&b.screen, &rac_engine::ui::font::FontMeasure);
	let n = prims.len();
	assert_eq!(page::with_fade(prims.clone(), 0.0).len(), n, "no fade: the page as it is");
	let faded = page::with_fade(prims, 0.4);
	assert_eq!(faded.len(), n + 1);
	match faded.last() {
		Some(Prim::RRect { rect, fill, border: None, .. }) => {
			assert!(rect.x <= -1000.0 && rect.w >= 5000.0 && rect.h >= 5000.0, "over everything");
			assert_eq!(*fill, Rgba::rgba(0, 0, 0, 0.4));
		}
		p => panic!("the fade is a black rect last, got {p:?}"),
	}
}

#[test]
fn retail_box_is_a_modern_dialog_over_the_cards() {
	let mut f = facts(true, Mode::Save);
	let panel = Rect::new(456.0, 116.0, 776.0, 420.0);
	assert!(page::build(&f, 1, Look::default()).screen.dialog.is_none());
	f.ask = Some(Question { lines: vec!["A save game already exists.".into(), String::new(), "Overwrite?".into()], yes_no: true });
	let b = page::build(&f, 1, Look::default());
	let d = b.screen.dialog.as_ref().expect("the dialog");
	assert_eq!(d.lines.len(), 3);
	let btn: Vec<_> = d.buttons.iter().map(|b| (b.symbol, b.text.as_str())).collect();
	assert_eq!(btn, vec![(rac_engine::ui::prim::Symbol::Circle, "Yes"), (rac_engine::ui::prim::Symbol::Triangle, "No")]);
	assert_eq!(cards_of(&b).0.len(), 4, "the cards stay behind it");
	assert!(cards_of(&b).0[1].selected);
	assert!(page::picture(&f, Some(panel)).is_none(), "retail's box covers the picture's spot");
	f.ask = Some(Question { lines: vec!["Saving Data.".into()], yes_no: false });
	assert!(page::build(&f, 1, Look::default()).screen.dialog.as_ref().is_some_and(|d| d.buttons.is_empty()), "saving: no buttons");
	let mut p = facts(false, Mode::Save);
	p.ask = Some(Question { lines: vec!["x".into()], yes_no: true });
	let pb = page::build(&p, 1, Look::default());
	assert!(pb.screen.dialog.is_some() && pb.screen.shell.is_some(), "the pause Save page's box over the shell");
}

fn cards_of(b: &Built) -> (&Vec<rac_engine::ui::kit::page::PageRow>, bool) {
	let Panel::Cards { cards, extra, .. } = &b.screen.panel else { panic!("cards panel") };
	(cards, extra.is_some())
}

#[test]
fn the_title_load_page_is_r_and_c1s_slot_cards() {
	let f = facts(true, Mode::Load);
	let b = page::build(&f, 1, Look::default());
	let (cards, extra) = cards_of(&b);
	assert_eq!((cards.len(), extra, b.cards, b.extra), (4, false, 4, false));
	assert_eq!(cards[0].label, "1  Planet Oozla");
	let stats: Vec<_> = cards[3].stats.iter().map(|s| (s.label.as_str(), s.value.as_str())).collect();
	assert_eq!(stats, vec![("Bolts", "61,140"), ("Play time", "06:15"), ("Saved", "95/00/00")]);
	assert!(cards[1].selected && !cards[0].selected);
	assert!(cards[2].dim && cards[2].label == "3  EMPTY", "an empty slot is dimmed with the game's word");
	// the root column: New Game, Load (selected, inert), Options, Quit to Desktop; no shell
	let m: Vec<_> = b.screen.menu.iter().map(|m| (m.text.as_str(), m.selected, m.live)).collect();
	assert_eq!(m, vec![("New Game", false, false), ("Load", true, false), ("Options", false, false), ("Quit to Desktop", false, false)]);
	assert!(b.screen.shell.is_none());
}

#[test]
fn the_title_save_page_has_continue_without_saving_and_the_pause_pages_the_shell() {
	let b = page::build(&facts(true, Mode::Save), 0, Look::default());
	let (_, extra) = cards_of(&b);
	assert!(extra && b.extra);
	assert!(b.screen.menu[0].selected, "New Game led here");
	let p = page::build(&facts(false, Mode::Save), 3, Look::default());
	assert!(!cards_of(&p).1, "no Continue without saving in the pause");
	let sh = p.screen.shell.as_ref().expect("the pause shell");
	assert_eq!((sh.site.as_str(), sh.bolts.as_str()), ("Dobbo", "1234"));
	assert!(p.screen.menu[OPTIONS_TAB].selected && p.screen.menu.iter().all(|m| !m.live), "Options tab selected, tabs inert");
	assert!(cards_of(&p).0[3].selected);
}

#[test]
fn the_picture_sits_under_the_cards_above_the_hints_with_the_tv_aspect() {
	let panel = Rect::new(456.0, 116.0, 776.0, 420.0);
	let h = page::hole(panel);
	assert_eq!((h.x, h.y), (456.0, 552.0));
	assert!(h.bottom() <= HINT_Y - 20.0 && h.h > 100.0, "{h:?}");
	let aspect = (page::PICTURE[2] * 4.0 / 3.0) / (page::PICTURE[3] * 512.0 / 448.0);
	assert!((h.w / h.h - aspect).abs() < 1e-3);
	let uv = page::uv();
	assert!(uv[0] > 0.33 && uv[2] < 0.94 && uv[1] > 0.25 && uv[3] < 0.48, "{uv:?}");
	// retail's cursor slot decides the picture: an empty one shows none
	let mut f = facts(true, Mode::Load);
	assert!(page::picture(&f, Some(panel)).is_some());
	f.pic = false;
	assert!(page::picture(&f, Some(panel)).is_none(), "the switch field or the empty well: retail draws no picture");
	f.pic = true;
	f.cur = 2;
	assert!(page::picture(&f, Some(panel)).is_none());
	assert!(page::picture(&f, None).is_none());
}

#[test]
fn the_scrims_get_a_hole_and_nothing_else_changes() {
	let hole = Rect::new(456.0, 552.0, 400.0, 110.0);
	let scrim = Rgba::rgba(5, 8, 15, 0.5);
	let other = Rgba::rgba(200, 8, 15, 0.5);
	let prims = vec![
		Prim::RRect { rect: Rect::new(450.0, -5000.0, 30.0, 10000.0), radius: 0.0, fill: scrim, border: None },
		Prim::RRect { rect: Rect::new(0.0, -5000.0, 30.0, 10000.0), radius: 0.0, fill: scrim, border: None },
		Prim::RRect { rect: Rect::new(450.0, 500.0, 300.0, 100.0), radius: 0.0, fill: other, border: None },
	];
	let pic = (hole, vec![]);
	let out = page::with_picture(prims.clone(), Some(&pic));
	// the strip over the hole: above, below and the 6 units left of it; the far strip and the panel untouched
	assert_eq!(out.len(), 3 + 2);
	let rects: Vec<Rect> = out.iter().filter_map(|p| if let Prim::RRect { rect, .. } = p { Some(*rect) } else { None }).collect();
	assert!(rects.iter().all(|r| !(r.x < hole.right() && r.right() > hole.x && r.y < hole.bottom() && r.bottom() > hole.y) || r.w == 300.0));
	assert_eq!(page::with_picture(prims.clone(), None), prims, "no picture: the list as built");
	assert_eq!(page::minus(Rect::new(0.0, 0.0, 10.0, 10.0), Rect::new(20.0, 20.0, 5.0, 5.0)).len(), 1);
}

#[test]
fn a_request_acts_only_in_the_box_state_it_was_made_in() {
	let ask = Question { lines: vec!["Overwrite?".into()], yes_no: true };
	let saving = Question { lines: vec!["Saving Data.".into()], yes_no: false };
	assert!(fits(true, Some(&ask)), "Yes / No on the box");
	assert!(!fits(false, Some(&ask)), "a page click as the box opens never answers it");
	assert!(!fits(true, None), "a box click as the box closes never acts on the page");
	assert!(!fits(true, Some(&saving)), "the saving message asks nothing");
	assert!(fits(false, None) && fits(false, Some(&saving)), "page clicks on the page");
}