//! [PC] R&C2's keyboard + mouse table (parity row 48, R&C1 `kbm_actions.rs` MENU + `retail/menu_keys.rs`): the engine's generic
//! 24 functions (engine/input/kbm/generic.rs, unchanged) plus a Menu layer that is on only while a retail menu screen is up
//! (`pc_input::menu_ctx`: the page menu, game mode 3, and the vendor, mode 5). In a menu the keyboard works like R&C1's modern
//! menu scheme: arrows / WASD move the selection (d-pad), Enter / Space confirm, Esc / Backspace go back, Q / E are L1 / R1
//! (R&C2's "\x14/\x15 to view other planets" pages); the left mouse button confirms and the right one goes back. What confirm and
//! back press is the screen's (`MenuCtx`, `pc_input.rs`). Outside menus nothing changes: the layer's keys are the play keys again.
//! Nothing is written to the game: the result is pad bits only.
//!
//! A key whose meaning changes when a menu opens or closes and that is held across the switch counts for nothing until it is
//! released (`Latch`), so the Esc that opened the pause menu does not also go back in it.
use rac_engine::input::kbm::{ActionDef, Input, Layer, MenuKey, MenuOut, Mouse, Output, Table, Wheel};
use std::sync::Mutex;
use winit::keyboard::KeyCode as K;

const fn mk(input: Input, out: MenuOut, unless_sticks: bool, stick_shadow: bool) -> MenuKey {
	MenuKey { input, out, unless_sticks, stick_shadow }
}
const fn key(k: K) -> Input {
	Input::Key(k)
}

/// The Menu layer (R&C1's keys; the mouse buttons added: R&C2's left button is Circle in play, its right one R2 (host.rs)).
pub static MENU: [MenuKey; 16] = [
	mk(key(K::Space), MenuOut::Confirm, false, false),
	mk(key(K::Enter), MenuOut::Confirm, false, false),
	mk(key(K::Escape), MenuOut::Back, false, false),
	mk(key(K::Backspace), MenuOut::Back, false, false),
	mk(key(K::KeyQ), MenuOut::Pad(4), false, true),
	mk(key(K::KeyE), MenuOut::Pad(8), false, true),
	mk(key(K::ArrowUp), MenuOut::Pad(0x1000), false, false),
	mk(key(K::ArrowDown), MenuOut::Pad(0x4000), false, false),
	mk(key(K::ArrowLeft), MenuOut::Pad(0x8000), false, false),
	mk(key(K::ArrowRight), MenuOut::Pad(0x2000), false, false),
	mk(key(K::KeyW), MenuOut::Pad(0x1000), true, false),
	mk(key(K::KeyS), MenuOut::Pad(0x4000), true, false),
	mk(key(K::KeyA), MenuOut::Pad(0x8000), true, false),
	mk(key(K::KeyD), MenuOut::Pad(0x2000), true, false),
	mk(Input::Mouse(Mouse::Left), MenuOut::Alt, false, false), // [b46 W7] the click's own bit in a Classic page menu (pc_input::menu_ctx)
	mk(Input::Mouse(Mouse::Right), MenuOut::Back, false, false),
];

/// PC action ids (`Output::Pc`, R&C1 `kbm_actions.rs`): the camera zoom, one notch per press (the wheel zooms by its delta).
pub const ZOOM_IN: u8 = 1;
pub const ZOOM_OUT: u8 = 2;
/// The context bit of a starship fight (`pc_input::ship_fight`): there "Ship fire" is the fire button (Circle, as retail).
pub const SHIP: u8 = 0;
/// Sections: 0 actions, 1 D-pad, 2 movement, 3 camera, 4 the PC rows.
pub const PC_SECTION: u8 = 4;

/// No controller slot (the stick directions).
const NO_PAD: u8 = 255;
/// `pad`: the controller button of the default (`binds::PHYS` index: A 0, B 1, X 2, Y 3, LB 4, RB 5, Start 6, Back 7, L3 8, R3 9,
/// D-pad up / down / left / right 10..13, LT 14, RT 15).
const fn a(id: &'static str, section: u8, output: Output, d0: Option<Input>, d1: Option<Input>, keep_one: bool, pad: u8) -> ActionDef {
	let ok = pad != NO_PAD;
	ActionDef { id, section, layer: Layer::Game, output, defaults: [d0, d1], keep_one, pad_ok: ok, pad_defaults: [if ok { Some(pad) } else { None }, None], pad_follows: 0 }
}
const fn pc(id: &'static str, output: Output, d0: Option<Input>) -> ActionDef {
	a(id, PC_SECTION, output, d0, None, false, NO_PAD)
}
const fn some(k: K) -> Option<Input> {
	Some(Input::Key(k))
}
const fn mouse(m: Mouse) -> Option<Input> {
	Some(Input::Mouse(m))
}

/// R&C2's actions (lane inp; the rows of the Controls tab, `pc_controls.rs` words them): the same 24 PS2 functions and defaults as
/// the generic table (Space, F, LMB / G, T / MMB, ...), named for the game (Jump, Wrench, Fire, Quick Select). The right mouse
/// button is R2's second key: the strafe aim retail has on that button (host.rs used to add it by hand). Sections: 0 actions,
/// 1 D-pad, 2 movement, 3 camera.
pub static ACTIONS: [ActionDef; 28] = [
	a("jump", 0, Output::Pad(0x40), some(K::Space), None, false, 0),
	a("wrench", 0, Output::Pad(0x80), some(K::KeyF), None, false, 2),
	a("fire", 0, Output::Pad(0x20), mouse(Mouse::Left), some(K::KeyG), false, 1),
	a("quick_select", 0, Output::Pad(0x10), some(K::KeyT), mouse(Mouse::Middle), false, 3),
	a("l1", 0, Output::Pad(4), some(K::KeyC), None, false, 4),
	a("r1", 0, Output::Pad(8), some(K::ControlLeft), None, false, 5),
	a("l2", 0, Output::Pad(1), some(K::KeyV), None, false, 14),
	a("r2", 0, Output::Pad(2), some(K::KeyB), None, false, 15),
	a("l3", 0, Output::Pad(0x200), some(K::ShiftLeft), None, false, 8),
	a("r3", 0, Output::Pad(0x400), some(K::KeyR), None, false, 9),
	a("pause", 0, Output::Pad(0x800), some(K::Escape), some(K::Enter), true, 6),
	a("select", 0, Output::Pad(0x100), some(K::KeyM), None, false, 7),
	a("up", 1, Output::Pad(0x1000), some(K::ArrowUp), None, false, 10),
	a("down", 1, Output::Pad(0x4000), some(K::ArrowDown), None, false, 11),
	a("left", 1, Output::Pad(0x8000), some(K::ArrowLeft), None, false, 12),
	a("right", 1, Output::Pad(0x2000), some(K::ArrowRight), None, false, 13),
	a("forward", 2, Output::Stick(3, -1), some(K::KeyW), None, false, NO_PAD),
	a("back", 2, Output::Stick(3, 1), some(K::KeyS), None, false, NO_PAD),
	a("strafe_left", 2, Output::Stick(2, -1), some(K::KeyA), None, false, NO_PAD),
	a("strafe_right", 2, Output::Stick(2, 1), some(K::KeyD), None, false, NO_PAD),
	a("camera_up", 3, Output::Stick(1, -1), some(K::KeyX), None, false, NO_PAD),
	a("camera_down", 3, Output::Stick(1, 1), some(K::KeyZ), None, false, NO_PAD),
	a("camera_left", 3, Output::Stick(0, -1), some(K::KeyE), None, false, NO_PAD),
	a("camera_right", 3, Output::Stick(0, 1), some(K::KeyQ), None, false, NO_PAD),
	// [b45 P6] The rows R&C1 has and R&C2 lacked (C6). Aim = R2, the strafe aim retail has on that button (the right mouse button
	// moved here from R2's second slot: same pad, but now it can be moved or switched off); Zoom = the Modern camera's zoom
	// (default the wheel, as before); Ship fire = Cross in a starship fight (context SHIP; [b46 W6] [C] the PCSX2 recording fullgame1 vb 67400..76600, level 25: Cross 0x4000 is pressed in ~3400 of the pad changes, Square ~210, Circle 20), the left button as before.
	a("aim", PC_SECTION, Output::Pad(2), mouse(Mouse::Right), None, false, NO_PAD),
	pc("zoom_in", Output::Pc(ZOOM_IN), Some(Input::Wheel(Wheel::Up))),
	pc("zoom_out", Output::Pc(ZOOM_OUT), Some(Input::Wheel(Wheel::Down))),
	ActionDef { id: "ship_fire", section: PC_SECTION, layer: Layer::Context(SHIP), output: Output::Pad(0x40), defaults: [mouse(Mouse::Left), None], keep_one: false, pad_ok: false, pad_defaults: [None, None], pad_follows: 0 },
];

/// R&C2's table (pack_bind.rs `kbm`).
pub static TABLE: Table = Table { actions: &ACTIONS, menu: &MENU };

/// Inputs whose pad meaning differs between a menu and play (the layer's keys that are not the same pad bit in both).
fn switches(i: Input) -> bool {
	matches!(i, Input::Key(K::Space | K::Enter | K::NumpadEnter | K::Escape | K::Backspace | K::KeyQ | K::KeyE | K::KeyW | K::KeyA | K::KeyS | K::KeyD) | Input::Mouse(_))
}

/// The layer last seen and the switching inputs held across its last change (they count for nothing until released).
pub struct Latch {
	menu: bool,
	stale: Vec<Input>,
}

impl Latch {
	pub const fn new() -> Latch {
		Latch { menu: false, stale: Vec::new() }
	}

	/// Called once per poll with the layer now and the raw `held`; returns whether input `i` counts as held.
	pub fn update(&mut self, menu: bool, held: &dyn Fn(Input) -> bool, all: &[Input]) {
		if menu != self.menu {
			self.menu = menu;
			self.stale = all.iter().copied().filter(|&i| switches(i) && held(i)).collect();
		}
		self.stale.retain(|&i| held(i));
	}

	pub fn counts(&self, i: Input) -> bool {
		!self.stale.contains(&i)
	}
}

/// The window's latch (host.rs `poll_input`).
pub static LATCH: Mutex<Latch> = Mutex::new(Latch::new());

#[cfg(test)]
mod tests {
	use super::*;
	use rac_engine::input::kbm::{eval, Binds, Ctx, MenuCtx};

	fn binds() -> Binds {
		Binds::defaults(&TABLE)
	}

	fn run(held: &[Input], menu: Option<MenuCtx>) -> u16 {
		eval(&TABLE, &binds(), &|i| held.contains(&i), &Ctx { contexts: 0, menu }).buttons
	}

	// alt = confirm: what pc_input::menu_ctx gives outside a Classic page menu (the left click is the Menu layer's Alt since [b46 W7])
	const PAUSE: MenuCtx = MenuCtx { confirm: 0x40, back: 0x10, alt: 0x40, sticks: false };

	#[test]
	fn play_keys_are_the_generic_table_and_the_menu_layer_takes_over_in_menus() {
		let enter = [key(K::Enter)];
		assert_eq!(run(&enter, None), 0x800, "play: Enter is Start");
		assert_eq!(run(&enter, Some(PAUSE)), 0x40, "menu: Enter confirms");
		assert_eq!(run(&[key(K::Escape)], Some(PAUSE)), 0x10, "menu: Esc goes back");
		assert_eq!(run(&[key(K::KeyW)], Some(PAUSE)), 0x1000, "menu: W is up");
		assert_eq!(run(&[Input::Mouse(Mouse::Left)], None), 0x20, "play: left button is Circle");
		assert_eq!(run(&[Input::Mouse(Mouse::Left)], Some(PAUSE)), 0x40, "menu: left button confirms");
		assert_eq!(run(&[Input::Mouse(Mouse::Right)], Some(PAUSE)), 0x10);
		assert_eq!(run(&[Input::Mouse(Mouse::Right)], None), 2, "play: the right button is R2");
		assert_eq!(run(&[Input::Mouse(Mouse::Right)], None) & 2, 2);
		// Same functions and defaults as the generic PS2 table (only the names and R2's second key differ).
		// (R&C2's table has its own ids and order, so every action is matched by what it does.)
		let generic = rac_engine::input::kbm::generic::ACTIONS;
		assert_eq!(TABLE.actions.iter().filter(|a| a.section < PC_SECTION).count(), generic.len());
		for a in TABLE.actions.iter().filter(|a| a.section < PC_SECTION) {
			assert!(generic.iter().any(|g| (a.output, a.keep_one, a.pad_defaults, a.defaults[0]) == (g.output, g.keep_one, g.pad_defaults, g.defaults[0])), "{}", a.id);
		}
	}

	#[test]
	fn the_pc_rows_default_to_what_the_hard_wired_inputs_did() {
		let ship = Ctx { contexts: 1 << SHIP, menu: None };
		let held = |h: &'static [Input]| move |i: Input| h.contains(&i);
		let lmb: &'static [Input] = &[Input::Mouse(Mouse::Left)];
		assert_eq!(eval(&TABLE, &binds(), &held(lmb), &ship).buttons, 0x40, "ship fight: the left button fires with Cross (b46 W6)");
		let up: &'static [Input] = &[Input::Wheel(Wheel::Up)];
		let o = eval(&TABLE, &binds(), &held(up), &Ctx::default());
		assert_eq!((o.buttons, o.pc), (0, 1 << ZOOM_IN), "the wheel up is Zoom in and presses no pad button");
		// Moving Aim off the right button frees it; the pad bit follows the new key.
		let mut b = binds();
		let ai = TABLE.index("aim").unwrap();
		b.assign(ai, 0, key(K::KeyN));
		let n: &'static [Input] = &[Input::Key(K::KeyN)];
		assert_eq!(eval(&TABLE, &b, &held(n), &Ctx::default()).buttons, 2);
		let rmb: &'static [Input] = &[Input::Mouse(Mouse::Right)];
		assert_eq!(eval(&TABLE, &b, &held(rmb), &Ctx::default()).buttons, 0);
	}

	#[test]
	fn a_switching_key_held_across_the_change_counts_only_after_release() {
		let mut l = Latch::new();
		let all = [key(K::Escape), key(K::ArrowUp)];
		let esc_up = |i: Input| i == key(K::Escape) || i == key(K::ArrowUp);
		l.update(false, &esc_up, &all);
		l.update(true, &esc_up, &all);
		assert!(!l.counts(key(K::Escape)), "the Esc that opened the menu");
		assert!(l.counts(key(K::ArrowUp)), "same bit in both layers");
		l.update(true, &|_| false, &all);
		assert!(l.counts(key(K::Escape)), "released");
	}
}
