use super::*;

fn card(ty: u32, state: u32) -> Card {
	Card { ty, state, drawn: 5, ..Card::default() }
}

#[test]
fn the_create_save_question_asks_circle_yes_triangle_no() {
	// The user's blank card (b57\ng find runs): type 1, state 0xd.
	assert_eq!(plan(&card(1, 0xd)), Plan::Show { msgs: vec![0xc01], labels: vec![YES_O, NO_T] });
	// After No: "continue anyway?" with Cross Yes / Triangle No.
	assert_eq!(plan(&card(1, 0x18)), Plan::Show { msgs: vec![0xc03, 0xbfc], labels: vec![YES_X, NO_T] });
}

#[test]
fn no_dialog_or_not_drawn_yet_is_off_and_other_types_are_classic() {
	assert_eq!(plan(&card(0, 0xd)), Plan::Off);
	assert_eq!(plan(&Card { drawn: 0, ..card(1, 0xd) }), Plan::Off);
	assert_eq!(plan(&Card { hold: 1, ..card(1, 0xd) }), Plan::Off);
	assert_eq!(plan(&card(0xb, 0)), Plan::Classic);
	assert_eq!(plan(&card(9, 0)), Plan::Classic);
}

#[test]
fn messages_without_a_question_have_no_buttons_and_failures_continue() {
	assert_eq!(plan(&card(1, 0xe)), Plan::Show { msgs: vec![0xc10], labels: vec![] });
	assert_eq!(plan(&card(1, 0x12)), Plan::Show { msgs: vec![0xc14], labels: vec![CONTINUE] });
	assert_eq!(plan(&Card { c4: 3, ..card(4, 0) }), Plan::Show { msgs: vec![0xbf6], labels: vec![] });
	assert_eq!(plan(&card(1, 0x99)), Plan::Off);
}

#[test]
fn glyph_bytes_name_the_pad_buttons() {
	assert_eq!(button(b"\x11 Yes\0"), Some(Btn { bit: CIRCLE_BIT, text: "Yes".into() }));
	assert_eq!(button(b"\x12 No\0"), Some(Btn { bit: TRIANGLE_BIT, text: "No".into() }));
	assert_eq!(button(b"\x10 Continue\0").map(|b| b.bit), Some(CROSS_BIT));
	assert_eq!(button(b"Yes\0"), None);
	assert_eq!(button(b"\x10: Ja\0"), Some(Btn { bit: CROSS_BIT, text: "Ja".into() }));
}

#[test]
fn enter_confirms_the_first_button_and_esc_cancels_with_triangle() {
	let yes_no = CardBox { buttons: vec![Btn { bit: CIRCLE_BIT, text: "Yes".into() }, Btn { bit: TRIANGLE_BIT, text: "No".into() }], ..CardBox::default() };
	assert_eq!((confirm_bit(&yes_no), cancel_bit(&yes_no)), (Some(CIRCLE_BIT), Some(TRIANGLE_BIT)));
	let one = CardBox { buttons: vec![Btn { bit: CROSS_BIT, text: "Continue".into() }], ..CardBox::default() };
	assert_eq!((confirm_bit(&one), cancel_bit(&one)), (Some(CROSS_BIT), Some(CROSS_BIT)));
	let none = CardBox::default();
	assert_eq!((confirm_bit(&none), cancel_bit(&none)), (None, None));
}
