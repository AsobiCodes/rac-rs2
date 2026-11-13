//! [PC] Debug mode's window routing (parity row 71; R&C1's viewer handler + devtools.rs): the planet list, readout and notice
//! (pc_devtools.rs), the free camera (pc_freecam.rs), the cheats (pc_debug.rs) and the split view (engine splitview.rs) get their
//! keys, gamepad, title and overlay draw here. Debug mode off: every function is a no-op (each tool checks `options::debug_mode`
//! itself), so no key is taken, nothing is drawn and the game's pad is untouched.
use crate::scene::Scene;
use rac_engine::input::pad::PadState;
use rac_engine::viewer::targets::{RETAIL_HD_TEX, RETAIL_OV_TEX};
use rac_engine::viewer::Gpu;
use winit::event::{ElementState, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

/// Before every other handler (the modern pages too): the open planet list takes every key press, F4 opens it.
pub(super) fn list_first(event: &WindowEvent) -> bool {
	match event {
		WindowEvent::KeyboardInput { event, .. } => match event.physical_key {
			PhysicalKey::Code(k) => crate::pc_devtools::list_key(k, event.state == ElementState::Pressed, event.repeat),
			_ => false,
		},
		_ => false,
	}
}

/// A key event for the Debug mode tools: the free camera first (F2, and its keys while it flies, so its `[` `]` speed beats the
/// split view's divider), the split view (F10 / F11 / F12, `[` `]` Home `\`), then F3 / F6 / F7 / F8 (pc_debug.rs). True = taken.
pub(super) fn key(k: KeyCode, pressed: bool, repeat: bool) -> bool {
	rac_engine::diag::key(k, pressed, repeat); // [b51 G5] RAC_DIAG: the pack's mark key (Insert) marks the moment (never takes the key)
	crate::pc_freecam::key(k, pressed, repeat) || rac_engine::splitview::key(k, pressed, repeat) || (pressed && !repeat && crate::pc_debug::key(k))
}

/// Every poll: the test feed's keys of game field `field` (RAC2_DEVTOOLS_FEED, pc_devtools.rs) down the window's own path, each
/// pressed then released.
pub(super) fn feed(field: u64) {
	for k in crate::pc_devtools::feed(field) {
		if !crate::pc_devtools::list_key(k, true, false) {
			key(k, true, false);
		}
		key(k, false, false);
	}
}

/// Every poll: the real gamepad (none = idle) for the free camera (L3 + R3, Start, Select, Triangle, sticks) and the planet list
/// (D-pad, Cross, Triangle / Circle).
pub(super) fn pad(p: Option<&PadState>) {
	if !crate::options::debug_mode() {
		return;
	}
	let (b, s) = p.map_or((0, [0x80; 4]), |p| (p.buttons, p.sticks));
	let b = b | crate::pc_devtools::fed_pad(); // test feed `pad:` tokens (RAC2_DEVTOOLS_FEED)
	crate::pc_freecam::pad(b, s);
	crate::pc_devtools::pad(b);
}

/// The window title's note: the quick save message first, else the Debug mode readout with the cheats' and the free camera's
/// newest events (all None with Debug mode off and nothing switched on: the usual title).
pub(super) fn title_note() -> Option<String> {
	crate::states::note_now().or_else(|| {
		let parts: Vec<String> = [crate::pc_devtools::title(), crate::pc_debug::note_now(), crate::pc_freecam::note_now()].into_iter().flatten().collect();
		(!parts.is_empty()).then(|| parts.join(" | "))
	})
}

/// Right after the HUD list is built (`world`: the engine draws this field's 3D world, no movie): the frame's gameplay verdict (also
/// no pause, no retail menu) and Ratchet's position for the readout and the warp, F11 Hide HUD (the game's HUD quads leave, in
/// gameplay only), then the planet list, the notice, the readout line and the free camera's help on top (render pixels).
pub(super) fn frame(g: &mut Gpu, world: bool, scene: &Scene) {
	if !crate::options::debug_mode() {
		return;
	}
	let playing = world && !scene.paused && crate::pc_input::menu_ctx().is_none();
	crate::pc_devtools::set_frame(playing, scene.hero_group.first().map(|h| [h[0], h[1], h[2]]));
	// F11 Hide HUD as R&C1's strip_hud: the whole 2D layer in gameplay (the game's HUD with its texts and boxes: one picture, not
	// split by element; the readout too). With the sharp HUD the HUD is the sharp picture and the overlay keeps only the effects
	// (fire, glows: gsfull/overlay.rs is_fx), which stay; without it the overlay holds both and goes whole.
	let clean = rac_engine::splitview::hidden() && playing;
	if clean {
		let sharp = g.has_retail_hd();
		g.hud.retain(|q| q.tex != RETAIL_HD_TEX && (sharp || q.tex != RETAIL_OV_TEX));
	}
	crate::pc_devtools::draw(&mut g.hud, g.config.width as f32, g.config.height as f32, clean);
}
