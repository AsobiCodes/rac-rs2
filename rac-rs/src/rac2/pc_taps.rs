//! [PC] Tap latch (b45 P6, sweep C5; R&C1 `retail/input.rs` `press_key` / `press_mouse`, TAP_FIELDS = 3) and the PC actions of the
//! kbm table (zoom). The window samples the held keys once per redraw while the game reads the pad once per field, so a key or
//! button pressed and released between two samples never reached the game. A fresh press (window thread, `note_event`) is latched
//! here; the first poll after it stamps the game's field, and from then on the input counts as held for `TAP_FIELDS` fields
//! (`alive`), however short the real press was. A key that is still held is simply held. Auto-repeats and releases are ignored.
use rac_engine::input::kbm::{Input, Mouse};
use std::sync::Mutex;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::keyboard::PhysicalKey;

/// The fewest fields a press is held for (R&C1 `TAP_FIELDS`).
pub const TAP_FIELDS: u64 = 3;

/// Latched presses: the input and the field of the first poll that saw it (None = not polled yet).
#[derive(Default)]
pub struct Taps(Vec<(Input, Option<u64>, Option<u64>)>);

impl Taps {
	pub const fn new() -> Taps {
		Taps(Vec::new())
	}

	/// A fresh press of `i` (window thread).
	/// [b49 Z2] A re-press of an input whose latch was already stamped remembers that stamp: if the next poll comes after the old
	/// latch ran out (a low redraw rate: several fields between polls), that poll shows the input released once (`alive` false), so
	/// the game sees two presses, not one long hold (the second tap used to be swallowed). At one poll per field nothing changes.
	pub fn press(&mut self, i: Input) {
		let prev = self.0.iter().find(|t| t.0 == i).and_then(|t| t.1);
		self.0.retain(|t| t.0 != i);
		self.0.push((i, None, prev));
	}

	/// One poll at game field `field`: new taps get their start field, old ones expire.
	pub fn poll(&mut self, field: u64) {
		for t in &mut self.0 {
			if t.2.take().is_some_and(|old| field > old + TAP_FIELDS) {
				continue; // [b49 Z2] the release between two taps: stamped by the next poll
			}
			t.1.get_or_insert(field);
		}
		self.0.retain(|t| t.1.map_or(true, |s| field < s + TAP_FIELDS));
	}

	/// Whether `i` is still latched (after `poll`).
	pub fn alive(&self, i: Input) -> bool {
		self.0.iter().any(|t| t.0 == i && t.1.is_some())
	}
}

/// The window's latch (host.rs `poll_input`).
pub static TAPS: Mutex<Taps> = Mutex::new(Taps::new());

/// The input a window event presses (None for releases, repeats, wheel and other events).
fn pressed(e: &WindowEvent) -> Option<Input> {
	match e {
		WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed && !event.repeat => match event.physical_key {
			PhysicalKey::Code(k) => Some(Input::Key(k)),
			_ => None,
		},
		WindowEvent::MouseInput { state: ElementState::Pressed, button, .. } => Some(Input::Mouse(match button {
			MouseButton::Left => Mouse::Left,
			MouseButton::Right => Mouse::Right,
			MouseButton::Middle => Mouse::Middle,
			MouseButton::Back => Mouse::Back,
			MouseButton::Forward => Mouse::Forward,
			MouseButton::Other(_) => return None,
		})),
		_ => None,
	}
}

/// Window thread, every window event AFTER the page hosts routed it (host.rs `window_event`). `taken`: a page, a capture or the
/// Debug list took the event (the game must never see it). [b46 U1] Bug A: build 45 latched every press before the routing
/// (pc_prompts.rs `note_window_event`), so a click the PC Options page / a modern screen had already acted on still reached the game
/// for TAP_FIELDS fields as the menu's Confirm (the left button in a menu): the page read that pad and acted a second time (a
/// toggle flipped back). R&C1 latches only in its play handler (rac1/host/handler/play_input.rs `press_mouse`), after the pages.
pub fn note_event(e: &WindowEvent, taken: bool) {
	if let Ok(mut t) = TAPS.lock() {
		note_routed(&mut t, e, taken);
	}
}

/// [b46 U1] The latch decision: only a press that goes on to the game is latched.
pub fn note_routed(t: &mut Taps, e: &WindowEvent, taken: bool) {
	if let (false, Some(i)) = (taken, pressed(e)) {
		t.press(i);
	}
}

/// A wheel notch (`lines` > 0 = up): what it does through the table. `pad`: it is bound to a pad or stick action (it taps that
/// action only, as before); `zoom`: the camera zoom lines when bound to a Zoom action (R&C1 `kbm_wheel`).
pub fn wheel_route(b: &rac_engine::input::kbm::Binds, lines: f32) -> (bool, f32) {
	use rac_engine::input::kbm::{table, Output, Wheel};
	let w = Input::Wheel(if lines > 0.0 { Wheel::Up } else { Wheel::Down });
	let (mut pad, mut zoom) = (false, 0.0f32);
	for (a, def) in table().actions.iter().enumerate().take(b.0.len()) {
		if !b.0[a].iter().flatten().any(|i| *i == w) {
			continue;
		}
		match def.output {
			Output::Pc(id) if id == crate::kbm_table::ZOOM_IN => zoom = lines.abs(),
			Output::Pc(id) if id == crate::kbm_table::ZOOM_OUT => zoom = -lines.abs(),
			Output::Pc(_) => {}
			_ => pad = true,
		}
	}
	(pad, zoom)
}

/// The zoom a rising edge of the PC action bits gives: +1 for Zoom in, -1 for Zoom out (keys and buttons; the wheel zooms by its delta).
pub fn zoom_edge(prev: u32, now: u32) -> f32 {
	let up = |bit: u8| now & (1 << bit) != 0 && prev & (1 << bit) == 0;
	(up(crate::kbm_table::ZOOM_IN) as i32 - up(crate::kbm_table::ZOOM_OUT) as i32) as f32
}

#[cfg(test)]
mod tests {
	use super::*;
	use winit::keyboard::KeyCode as K;

	#[test]
	fn a_tap_between_two_polls_is_held_for_three_fields() {
		let mut t = Taps::new();
		let k = Input::Key(K::Space);
		t.press(k);
		t.poll(100);
		assert!(t.alive(k), "the first poll sees it");
		t.poll(101);
		t.poll(102);
		assert!(t.alive(k), "still latched in the third field");
		t.poll(103);
		assert!(!t.alive(k), "expired after TAP_FIELDS fields");
	}

	#[test]
	fn a_repress_restarts_the_latch_and_other_inputs_are_independent() {
		let mut t = Taps::new();
		let (a, b) = (Input::Key(K::KeyF), Input::Mouse(Mouse::Left));
		t.press(a);
		t.poll(10);
		t.press(b);
		t.poll(12);
		assert!(t.alive(a) && t.alive(b));
		t.press(a);
		t.poll(13);
		t.poll(15);
		assert!(t.alive(a), "restarted at 13");
		assert!(!t.alive(b), "b (started 12) expired at 15");
	}

	/// [b49 Z2] Two taps between slow polls are two presses: a release shows between them.
	#[test]
	fn two_taps_over_slow_polls_are_two_presses() {
		let mut t = Taps::new();
		let k = Input::Key(K::Space);
		t.press(k);
		t.poll(100);
		assert!(t.alive(k));
		t.press(k); // the second tap, between this poll and the next one 12 fields later
		t.poll(112);
		assert!(!t.alive(k), "the release between the two taps");
		t.poll(113);
		assert!(t.alive(k), "the second press");
		t.poll(116);
		assert!(!t.alive(k));
		// at one poll per field a quick re-press stays one continuous hold, as before
		let mut t = Taps::new();
		t.press(k);
		t.poll(10);
		t.press(k);
		t.poll(11);
		assert!(t.alive(k));
	}

	/// [b46 U1] Bug A: one left click on a PC Options toggle acted twice in build 45 (the page's click + the latched Confirm).
	#[test]
	fn a_click_a_page_took_is_not_latched_for_the_game() {
		use rac_engine::input::kbm::{eval, Binds, Ctx, MenuCtx};
		use winit::event::DeviceId;
		rac_engine::input::kbm::ensure_table(&crate::kbm_table::TABLE);
		// alt = confirm, as pc_input::menu_ctx gives it outside a Classic page menu (the left click is the Menu layer's Alt since [b46 W7])
		let pause = MenuCtx { confirm: 0x40, back: 0x10, alt: 0x40, sticks: false };
		let click =WindowEvent::MouseInput { device_id: DeviceId::dummy(), state: ElementState::Pressed, button: MouseButton::Left };
		// What the game (and the page's pad, modern_open::pad) gets in the fields after the click.
		let menu_pad = |t: &Taps| eval(&crate::kbm_table::TABLE, &Binds::defaults(&crate::kbm_table::TABLE), &|i| t.alive(i), &Ctx { contexts: 0, menu: Some(pause) }).buttons;
		// Build 45: latched before the routing, whatever took it -> the page's own click AND a Confirm on the pad = two actions.
		let mut b45 = Taps::new();
		b45.press(pressed(&click).unwrap());
		b45.poll(50);
		assert_eq!(menu_pad(&b45) & 0x40, 0x40, "build 45: the taken click reached the page's pad as Confirm (the second action)");
		// Now: the page took it -> nothing for the game; a click nobody took is still one press (the classic menus, play).
		let mut taken = Taps::new();
		note_routed(&mut taken, &click, true);
		taken.poll(50);
		assert_eq!(menu_pad(&taken), 0, "a click the page took never reaches the game");
		let mut free = Taps::new();
		note_routed(&mut free, &click, false);
		free.poll(50);
		assert_eq!(menu_pad(&free) & 0x40, 0x40, "a click on a retail menu still confirms once");
		let up = WindowEvent::MouseInput { device_id: DeviceId::dummy(), state: ElementState::Released, button: MouseButton::Left };
		let mut rel = Taps::new();
		note_routed(&mut rel, &up, false);
		rel.poll(50);
		assert_eq!(menu_pad(&rel), 0, "a release latches nothing");
	}

	#[test]
	fn zoom_edges_and_wheel_routing() {
		assert_eq!(zoom_edge(0, 1 << crate::kbm_table::ZOOM_IN), 1.0);
		assert_eq!(zoom_edge(1 << crate::kbm_table::ZOOM_IN, 1 << crate::kbm_table::ZOOM_IN), 0.0);
		assert_eq!(zoom_edge(0, 1 << crate::kbm_table::ZOOM_OUT), -1.0);
		rac_engine::input::kbm::ensure_table(&crate::kbm_table::TABLE);
		let b = rac_engine::input::kbm::Binds::defaults(&crate::kbm_table::TABLE);
		assert_eq!(wheel_route(&b, 2.0), (false, 2.0), "default: the wheel up zooms in, taps no pad action");
		assert_eq!(wheel_route(&b, -1.0), (false, -1.0));
	}
}
