use super::*;

#[test]
fn modern_tabs_switch_sends_back_once_then_cross_at_the_root() {
	let mut s = Switch::new(2);
	assert_eq!(s.on_sub(0), (true, false)); // Weapons up, Quick Select asked: Triangle once
	assert_eq!(s.on_sub(0), (false, false));
	s.age += RESEND; // [b45 P4A] B28: a Triangle the page did not take is sent again
	assert_eq!(s.on_sub(0), (true, false));
	assert!(s.on_root()); // root: cursor + Cross
	assert!(!s.on_root()); // not again before RESEND fields
	s.age += RESEND;
	assert!(s.on_root());
	assert_eq!(s.on_sub(2), (false, true)); // the asked page ends it
}

#[test]
fn modern_tabs_switch_lives_through_retail_page_gaps() {
	let mut s = Switch::new(5);
	assert!(!s.gone(true)); // Weapons up, no Cross yet: keep
	assert!(!s.gone(false)); // Triangle: one field with no page
	assert!(s.on_root());
	assert!(!s.gone(false)); // Cross: one field with no page
	assert!(s.gone(true)); // a page after the Cross (Options: classic): over
}

#[test]
fn modern_tabs_switch_dropped_when_the_pause_closes() {
	let mut s = Switch::new(3);
	assert!(!s.gone(false));
	assert!(!s.gone(false));
	assert!(s.gone(false)); // three fields with no page: Start closed the pause
}
