use super::*;

fn ram_with_mode(mode: u32) -> Vec<u8> {
	let mut ram = vec![0u8; 0x200_0000];
	ram[0x1a8f80..0x1a8f84].copy_from_slice(&mode.to_le_bytes());
	ram[0x1a8f88..0x1a8f8c].copy_from_slice(&(-2i32).to_le_bytes());
	ram[0x1a8f84..0x1a8f88].copy_from_slice(&mode.to_le_bytes());
	ram
}

#[test]
fn the_mode_word_is_found_by_its_rule_and_menus_confirm_on_cross() {
	let ram = ram_with_mode(3);
	assert_eq!(find_mode(&ram), Some(0x1a8f80), "pause menu: mode 3, previous mode 3 (state 0262)");
	let mut other = vec![0u8; 0x200_0000];
	for (o, v) in [(0, 3u32), (4, 3), (8, 0)] {
		other[0x1a8fc0 + o..0x1a8fc4 + o].copy_from_slice(&v.to_le_bytes());
	}
	assert_eq!(find_mode(&other), Some(0x1a8fc0), "level 1 keeps it at 0x1a8fc0; a menu may hold 0 at +8 (state 0218)");
	assert_eq!(screen(3), 0x40 | 0x10 << 16, "page menu, pause root too");
	assert_eq!(screen(5), 0x40 | 0x10 << 16, "vendor");
	assert_eq!(screen(0), 0);
	assert_eq!(screen(4), 0, "dialogs and the wheel keep the play keys");
}

#[test]
fn the_pointer_picks_eight_cells_clockwise_from_up_and_holds_near_a_boundary() {
	assert_eq!(cell_of(0.0, -50.0, None), 0);
	assert_eq!(cell_of(50.0, -50.0, None), 1);
	assert_eq!(cell_of(50.0, 0.0, None), 2);
	assert_eq!(cell_of(0.0, 50.0, None), 4);
	assert_eq!(cell_of(-50.0, 0.0, None), 6);
	assert_eq!(cell_of(-50.0, -50.0, None), 7);
	// 25 deg right of up: a fresh pick is cell 1, a held cell 0 stays (within 30 deg).
	let (x, y) = (25f32.to_radians().sin() * 50.0, -25f32.to_radians().cos() * 50.0);
	assert_eq!(cell_of(x, y, None), 1);
	assert_eq!(cell_of(x, y, Some(0)), 0);
	assert_eq!(cell_stick(2), (0xff, 0x80), "right: full push");
	assert_eq!(cell_stick(7), (0x00, 0x00), "up-left corner");
}

#[test]
fn nothing_changes_without_mouse_input() {
	let ram = ram_with_mode(0);
	let mut c = Ctl { script_read: true, ..Default::default() };
	for f in 0..10 {
		assert_eq!(c.field(&ram, f, false, 0x40, [1, 2, 3, 4]), (0x40, [1, 2, 3, 4]));
	}
}

#[test]
fn a_wheel_notch_in_a_menu_steps_once_and_a_ship_fight_takes_the_mouse() {
	let mut ram = ram_with_mode(3);
	let mut c = Ctl { script: parse_script("2:w:1;20-22:m:40,-20"), script_read: true, ..Default::default() };
	let downs: Vec<u16> = (0..8).map(|f| c.field(&ram, f, false, 0, [0x80; 4]).0).collect();
	assert_eq!(downs, [0, 0, UP, UP, 0, 0, 0, 0], "one notch up: Up for 2 fields");
	ram[0x1a8f80..0x1a8f84].copy_from_slice(&0u32.to_le_bytes());
	ram[HERO_STATE..HERO_STATE + 4].copy_from_slice(&49u32.to_le_bytes());
	ram[HERO_GROUP..HERO_GROUP + 4].copy_from_slice(&9u32.to_le_bytes());
	let (_, s) = c.field(&ram, 20, false, 0, [0x80; 4]);
	assert!(s[2] > 0x80 && s[3] > 0x80, "mouse right and up: stick right and down (climb) {s:?}");
	let s2 = (0..40).map(|f| c.field(&ram, 30 + f, false, 0, [0x80; 4]).1).last().unwrap();
	assert_eq!(s2, [0x80; 4], "the stick falls back once the mouse stops");
}

#[test]
fn the_script_parses() {
	assert_eq!(parse_script("5-9:m:3,-4; 12:w:-2"), vec![(5, 9, 'm', 3.0, -4.0), (12, 12, 'w', -2.0, 0.0)]);
}

#[test]
fn the_wheel_adds_no_press_while_a_modern_page_walks() {
	assert!(!wheel_steps_menu(true, 1, 0, || true), "a modern walk: no wheel press");
	assert!(!wheel_steps_menu(true, 0, 2, || true), "pending steps dropped too");
	assert!(wheel_steps_menu(true, 1, 0, || false), "retail menu: the wheel steps");
	assert!(wheel_steps_menu(true, 0, 0, || panic!("no wheel: the pages are not asked")));
	assert!(!wheel_steps_menu(false, 1, 0, || false));
}
