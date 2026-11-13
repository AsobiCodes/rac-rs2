use super::*;
use crate::modern_pages::{self, Look};
use rac_engine::ui::kit::page::Panel;

#[test]
fn gate_tells_the_pause_root_from_the_title_root() {
	assert_eq!(gate(2, 0, 0x1fb_8000, 0), Some(Kind::Pause));
	assert_eq!(gate(2, 0, 0, 4), Some(Kind::Title));
	assert_eq!(gate(2, 0, 0, 2), None, "title logos: no panels yet");
	assert_eq!(gate(8, 0x28bf88, 0x1fb_8000, 0), None, "a sub page stays classic");
	assert_eq!(gate(2, 0x259458, 0, 4), None, "a child record with id 2 is not the root");
}

#[test]
fn drive_walks_the_short_way_round_and_picks() {
	let mut d = Drive { target: Some(5), confirm: true, ..Drive::default() };
	// From row 1 of 7: forward 4, back 3 -> Up.
	let seq: Vec<u16> = (0..4).map(|_| d.step(1, 7)).collect();
	assert_eq!(seq, vec![UP, UP, 0, 0], "two fields pressed, two released");
	let seq: Vec<u16> = (0..4).map(|_| d.step(5, 7)).collect();
	assert_eq!(seq, vec![BIT[CROSS], BIT[CROSS], 0, 0]);
	assert!(!d.busy(), "done after the pick");
	assert_eq!(d.step(5, 7), 0);
	let mut d = Drive { target: Some(2), ..Drive::default() };
	assert_eq!(d.step(1, 3), DOWN);
	let mut d = Drive { target: Some(2), ..Drive::default() };
	assert_eq!(d.step(2, 3), 0);
	assert!(!d.busy(), "already there, no pick: nothing pressed");
	let mut d = Drive { back: true, ..Drive::default() };
	assert_eq!(d.step(0, 7), BIT[TRIANGLE]);
}

#[test]
fn drive_gives_up_on_a_row_the_cursor_never_reaches() {
	// [b45 P3] B21: n + 2 walk presses, then no more presses and no Cross; the page is free again.
	let mut d = Drive { target: Some(3), confirm: true, ..Drive::default() };
	let seq: Vec<u16> = (0..200).map(|_| d.step(1, 5)).collect();
	assert_eq!(seq.iter().filter(|&&b| b == DOWN).count(), 2 * 7);
	assert!(!seq.contains(&BIT[CROSS]) && !d.busy());
}

#[test]
fn pause_pad_turns_left_right_into_up_down_and_keeps_circle() {
	assert_eq!(pause_pad(LEFT), UP);
	assert_eq!(pause_pad(RIGHT | BIT[CROSS]), DOWN | BIT[CROSS]);
	assert_eq!(pause_pad(BIT[CIRCLE] | START), START);
	assert_eq!(pause_pad(0), 0);
}

#[test]
fn title_quit_entry_follows_r_and_c1() {
	let mut q = Quit::default();
	assert_eq!(title_pad(&mut q, 1, DOWN, DOWN), (DOWN, false), "Down on Load is the game's");
	assert_eq!(title_pad(&mut q, 2, DOWN, DOWN), (0, false), "Down on Options focuses Quit, not the wrap");
	assert!(q.focus);
	assert_eq!(title_pad(&mut q, 2, BIT[CROSS], BIT[CROSS]), (0, false));
	assert!(q.ask);
	assert_eq!(title_pad(&mut q, 2, BIT[CROSS], BIT[CROSS]), (0, false), "Cross never answers");
	assert_eq!(title_pad(&mut q, 2, BIT[TRIANGLE], BIT[TRIANGLE]), (0, false));
	assert!(!q.ask && q.focus, "No: back on the entry");
	q.ask = true;
	assert_eq!(title_pad(&mut q, 2, BIT[CIRCLE], BIT[CIRCLE]), (0, true), "Circle = Yes");
	let mut q = Quit { focus: true, ask: false };
	title_pad(&mut q, 2, UP, UP);
	assert!(!q.focus, "Up leaves the entry");
}

#[test]
fn site_ids_and_decode() {
	assert_eq!(site_id(0), Some(0xb45));
	assert_eq!(site_id(6), Some(0xb4b));
	assert_eq!(site_id(26), Some(0x1b22));
	assert_eq!(site_id(24), None);
	assert_eq!(decode(b" Quick Select\0junk"), "Quick Select");
	assert_eq!(decode(b"Op\x01tions\0"), "Options", "control bytes dropped");
}

/// A RAM image with a little text table: records at 0x100100, strings at 0x100800.
fn ram_with(texts: &[(u32, &str)]) -> Vec<u8> {
	let mut ram = vec![0u8; 0x10_2000];
	for (k, (id, s)) in texts.iter().enumerate() {
		let (r, p) = (0x10_0100 + 16 * k, 0x10_0800 + 64 * k);
		ram[r..r + 4].copy_from_slice(&(p as u32).to_le_bytes());
		ram[r + 4..r + 8].copy_from_slice(&id.to_le_bytes());
		ram[p..p + s.len()].copy_from_slice(s.as_bytes());
	}
	ram
}

#[test]
fn texts_find_the_table_by_its_anchor_pair() {
	let ram = ram_with(&[(0xa46, "#"), (0xb62, "Weapons"), (0xb63, "Gadgets"), (0xbc4, "Special")]);
	let t = Texts::find(&ram).expect("table");
	assert_eq!((t.start, t.n), (0x10_0100, 4));
	assert_eq!(t.get(&ram, 0xbc4).as_deref(), Some("Special"));
	assert_eq!(t.get(&ram, 0xa46).as_deref(), Some("#"));
	assert_eq!(t.get(&ram, 0xb67), None);
	assert!(t.still(&ram));
}

/// The real bank states, when this PC has them: the pause root of fg1 0262 (level 6) reads as R&C2's seven rows.
#[test]
fn bank_pause_state_reads_its_rows() {
	let Ok(ram) = std::fs::read("E:/ratchet/rac2/states/fg1/0262/eeMemory.bin") else { return };
	let t = Texts::find(&ram).expect("text table in 0262");
	let rows: Vec<String> = PAUSE_IDS.iter().map(|&id| t.get(&ram, id).unwrap_or_default()).collect();
	assert_eq!(rows, ["Weapons", "Gadgets", "Quick Select", "Items", "Help", "Options", "Special"]);
	assert_eq!(t.get(&ram, site_id(rd(&ram, LEVEL)).unwrap()).as_deref(), Some("Notak"));
	let s = modern_open::find(&ram).and_then(|l| modern_open::snap(&ram, &l)).expect("menu up");
	assert_eq!(gate(s.page_id, rd(&ram, s.page + 0xe0), rd(&ram, MANAGER), rd(&ram, TITLE_STATE)), Some(Kind::Pause));
	assert_eq!(rd(&ram, rd(&ram, MANAGER) + PAUSE_CURSOR_OFF), 0);
}

#[test]
fn pages_look_like_r_and_c1s_roots() {
	let f = Facts { kind: Kind::Pause, cur: 2, rows: PAUSE_IDS.iter().map(|i| format!("r{i:x}")).collect(), site: "Notak".into(), bolts: 21670 };
	let p = modern_pages::build(&f, Quit::default(), Look::default());
	assert_eq!(p.menu.len(), 7);
	assert!(p.menu[2].selected && p.menu.iter().filter(|m| m.selected).count() == 1);
	let sh = p.shell.as_ref().expect("pause shell");
	assert_eq!((sh.label.as_str(), sh.site.as_str(), sh.bolts.as_str()), ("Paused", "Notak", "21670"));
	assert_eq!(p.panel, Panel::None);
	let t = Facts { kind: Kind::Title, cur: 1, rows: vec!["New Game".into(), "Load".into(), "Options".into()], ..Facts::default() };
	let p = modern_pages::build(&t, Quit::default(), Look { lang: 2, ..Look::default() });
	assert_eq!(p.menu.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(), ["New Game", "Load", "Options", "Zum Desktop beenden"]);
	assert!(p.menu[1].selected && p.shell.is_none() && p.dialog.is_none());
	let p = modern_pages::build(&t, Quit { focus: true, ask: true }, Look::default());
	assert!(p.menu[3].selected && p.menu.iter().all(|m| !m.live));
	assert_eq!(p.dialog.as_ref().map(|d| d.buttons.len()), Some(2));
}

/// `manager`: the constant word where it is set, the title kept as the title, else the word the level's code names.
#[test]
fn manager_keeps_the_constant_and_the_title() {
	let mut ram = vec![0u8; 0x200_0000];
	let s = modern_open::Snap { manager: 0x1fb_8000, ..Default::default() };
	assert_eq!(manager(&ram, &s), 0x1fb_8000, "constant word empty, no title: the level's own word");
	let logos = modern_open::Snap { g: TITLE_STATE, ..s };
	assert_eq!(manager(&ram, &logos), 0, "the title's G (logos, state 2): never the pause");
	ram[TITLE_STATE as usize..TITLE_STATE as usize + 4].copy_from_slice(&4u32.to_le_bytes());
	assert_eq!(manager(&ram, &s), 0, "at the title the gate must still see the title");
	ram[MANAGER as usize..MANAGER as usize + 4].copy_from_slice(&0x1fb_9000u32.to_le_bytes());
	assert_eq!(manager(&ram, &s), 0x1fb_9000, "the constant word wins where it is set");
}

/// Level 1 (Oozla) keeps the menu manager at 0x1a910c: RAM of fg1 0025 with the pause root up (dumped 2026-11-12, the user's report
/// "the new menu is missing") reads as the pause root through `manager`; with the constant alone it read as no menu.
#[test]
fn level_1_pause_reads_through_the_moved_manager() {
	let Ok(ram) = std::fs::read("E:/ratchet/scratch_rc2_b42/bisect/dump_p1/ram00070.bin") else { return };
	let s = modern_open::find(&ram).and_then(|l| modern_open::snap(&ram, &l)).expect("menu up");
	assert_eq!(rd(&ram, MANAGER), 0);
	assert_eq!(gate(s.page_id, rd(&ram, s.page + 0xe0), rd(&ram, MANAGER), rd(&ram, TITLE_STATE)), None, "the old read");
	let m = manager(&ram, &s);
	assert_eq!(m, 0x1fb_8000);
	assert_eq!(gate(s.page_id, rd(&ram, s.page + 0xe0), m, rd(&ram, TITLE_STATE)), Some(Kind::Pause));
	assert_eq!(rd(&ram, m + PAUSE_CURSOR_OFF), 0, "cursor on Weapons, as the picture shows");
}

/// The bank's title state (fg1 0000): the code-found manager word is set there, the title must still never read as the pause.
#[test]
fn bank_title_state_is_never_the_pause() {
	let Ok(ram) = std::fs::read("E:/ratchet/rac2/states/fg1/0000/eeMemory.bin") else { return };
	let Some(s) = modern_open::find(&ram).and_then(|l| modern_open::snap(&ram, &l)) else { return };
	assert_eq!(s.g, TITLE_STATE);
	assert_ne!(s.manager, 0, "the title's own manager word is set: the trap this guards");
	assert_eq!(manager(&ram, &s), 0);
	assert_ne!(gate(s.page_id, rd(&ram, s.page + 0xe0), manager(&ram, &s), rd(&ram, TITLE_STATE)), Some(Kind::Pause));
}

/// [b49 Z3] The title holds all five languages' tables (English, French, German, Spanish, Italian by address): the language word's
/// column is taken; a lone table is taken whatever the word says.
#[test]
fn texts_find_takes_the_language_column_when_several_tables_exist() {
	let mut ram = vec![0u8; 0x1b_0000];
	for (t, word) in ["Weapons", "Armes", "Waffen", "Armas", "Armi"].iter().enumerate() {
		for (k, id) in [0xb62u32, 0xb63].iter().enumerate() {
			let (r, p) = (0x10_0100 + 0x1000 * t + 16 * k, 0x10_0800 + 0x1000 * t + 64 * k);
			ram[r..r + 4].copy_from_slice(&(p as u32).to_le_bytes());
			ram[r + 4..r + 8].copy_from_slice(&id.to_le_bytes());
			let s = if k == 0 { *word } else { "Gadgets" };
			ram[p..p + s.len()].copy_from_slice(s.as_bytes());
		}
	}
	for (lang, want) in [(0u8, "Weapons"), (2, "Armes"), (3, "Waffen"), (4, "Armas"), (5, "Armi"), (9, "Weapons")] {
		ram[0x1a7c3c] = lang;
		let t = Texts::find(&ram).expect("table");
		assert_eq!(t.get(&ram, 0xb62).as_deref(), Some(want), "lang {lang}");
	}
	let mut one = ram.clone();
	one[0x10_1000..0x10_6000].fill(0);
	one[0x1a7c3c] = 3;
	assert_eq!(Texts::find(&one).unwrap().get(&one, 0xb62).as_deref(), Some("Weapons"));
}

fn put_table(ram: &mut [u8], base: usize, word: &str) {
	for (k, id) in [0xb62u32, 0xb63].iter().enumerate() {
		let (r, p) = (base + 16 * k, base + 0x700 + 64 * k);
		ram[r..r + 4].copy_from_slice(&(p as u32).to_le_bytes());
		ram[r + 4..r + 8].copy_from_slice(&id.to_le_bytes());
		let s = if k == 0 { word } else { "Gadgets" };
		ram[p..p + s.len()].copy_from_slice(s.as_bytes());
	}
}

/// [b50 V4] A table found for one language is not `still` once the language word changes (review finding 5).
#[test]
fn texts_are_not_still_after_a_language_change() {
	let mut ram = vec![0u8; 0x1b_0000];
	for (t, w) in ["Weapons", "Armes", "Waffen"].iter().enumerate() {
		put_table(&mut ram, 0x10_0100 + 0x1000 * t, w);
	}
	let t = Texts::find(&ram).expect("table");
	assert_eq!(t.get(&ram, 0xb62).as_deref(), Some("Weapons"));
	assert!(t.still(&ram));
	ram[0x1a7c3c] = 3;
	assert!(!t.still(&ram), "the language word changed");
	let t2 = Texts::find(&ram).expect("table");
	assert_eq!(t2.get(&ram, 0xb62).as_deref(), Some("Waffen"));
	assert!(t2.still(&ram));
}

/// [b50 V4] A miss is remembered until language / level / mode change (or MISS_RECHECK calls); a found table is re-used without a scan.
#[test]
fn the_finder_scans_once_for_a_miss_and_once_for_a_hit() {
	let mut f = Finder::default();
	let mut ram = vec![0u8; 0x1b_0000];
	for _ in 0..50 {
		assert!(f.find(&ram).is_none());
	}
	assert_eq!(f.scans, 1, "50 misses, one scan");
	ram[0x1a7a78] = 5; // level changes: scan again
	assert!(f.find(&ram).is_none());
	assert_eq!(f.scans, 2);
	put_table(&mut ram, 0x10_0100, "Weapons");
	ram[0x1a7a78] = 6;
	assert!(f.find(&ram).is_some());
	assert_eq!(f.scans, 3);
	for _ in 0..50 {
		assert!(f.find(&ram).is_some());
	}
	assert_eq!(f.scans, 3, "the known place answers");
	ram[0x10_0104] = 0; // the table is gone
	assert!(f.find(&ram).is_none());
	assert_eq!(f.scans, 4);
	for _ in 0..=MISS_RECHECK {
		let _ = f.find(&ram);
	}
	assert_eq!(f.scans, 5, "the memo expires after MISS_RECHECK calls");
}
