//! [PC] The window side of R&C2's modern Options page (parity wave 2 B; the engine's `ui::menu_host` owns the page, the door is
//! rac2/modern_open.rs): the display-frame step (open on the door's request, the pad, the draw list, the retail menu overlay hidden
//! behind the page while the engine draws the world) and the window's keys, mouse and focus loss while the page is up. Nothing runs
//! on a frame without the page but two flag reads.
use super::App;
use crate::modern_open;
use rac_engine::ui::{self, gpu as ui_gpu, menu_host, MouseButton as UiButton, MouseEvent};
use rac_engine::viewer::targets::{RETAIL_HD_TEX, RETAIL_OV_TEX};
use rac_engine::viewer::Gpu;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::PhysicalKey;

/// Display-frame step, after the HUD list is built and before the draw. `world`: the engine draws the world this frame (then the
/// game's menu overlay is left out behind the page, so its blur shows the world alone; without the world the GS picture stays).
pub(super) fn step(g: &mut Gpu, world: bool) {
	let refused = ui_gpu::refused();
	modern_open::set_gpu_refused(refused);
	let was_open = ui::is_open();
	let request = modern_open::take_open();
	if !was_open && request.is_none() {
		return;
	}
	// The game thread's pad of the newest field (the player's, or a replay's): the page reads it like the game would.
	let (b, s) = modern_open::page_pad(); // [b46 U6] the real controller only in a played session
	let pad = Some(menu_host::pad_sample(b, s));
	if !was_open {
		// A request while the UI pipes are refused opens nothing: the retail Camera page is the fallback (the door is shut then too).
		if let Some(tab) = request.filter(|_| !refused) {
			menu_host::open(modern_open::take_front(), tab, pad); // [b45 P4B] A2: title rows open it as R&C1's title (front)
		}
	}
	let Some(prims) = menu_host::step(g.win_size(), pad, modern_open::options_up() && !refused) else { return };
	g.ui_gpu.set_scene(prims); // valid for the next draw only: every frame while the page is up
	// [b46 W4] over the title (peek, "View game") the classic Options list is part of the GS picture: left out as R&C1 hides its title menu
	if (world && menu_host::hides_retail()) || menu_host::hides_title_menu() {
		g.hud.retain(|q| q.tex != RETAIL_OV_TEX && q.tex != RETAIL_HD_TEX);
	}
}

fn map_button(b: MouseButton) -> Option<UiButton> {
	match b {
		MouseButton::Left => Some(UiButton::Left),
		MouseButton::Right => Some(UiButton::Right),
		MouseButton::Middle => Some(UiButton::Middle),
		_ => None,
	}
}

impl App {
	/// Called first in `window_event`. True = the event is the page's alone (the game never sees it). Presses go to the page while it
	/// is up; key and button RELEASES always go on, so nothing stays held in the game. A left click with the page down and the cursor
	/// free is noted for the door (Cross on the Camera row of the Options list; anywhere else it stays the game's Circle).
	pub(super) fn menu_event(&mut self, event: &WindowEvent) -> bool {
		// [PC] A Controls-tab capture takes the next key / button / notch (pc_controls.rs).
		if crate::pc_controls::event(event) {
			return true;
		}
		if let WindowEvent::CursorMoved { position, .. } = event {
			self.ui_pos = Some((position.x as f32, position.y as f32));
		}
		if !ui::is_open() {
			if let WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } = event {
				if !self.cursor.grabbed() {
					modern_open::note_click();
				}
			}
			return false;
		}
		match event {
			WindowEvent::KeyboardInput { event, .. } => match event.physical_key {
				PhysicalKey::Code(k) => menu_host::key(k, event.state == ElementState::Pressed, event.repeat),
				_ => false,
			},
			WindowEvent::CursorMoved { position, .. } => menu_host::page_mouse(MouseEvent::Move { x: position.x as f32, y: position.y as f32 }),
			WindowEvent::MouseInput { state, button, .. } => {
				// [PC] fx1: the right button is Back wherever the cursor is, also before the window saw a cursor move (fx1 run xr).
				let Some(b) = map_button(*button) else { return false };
				let Some((x, y)) = self.ui_pos.or((b == UiButton::Right).then_some((0.0, 0.0))) else { return false };
				if *state == ElementState::Pressed {
					menu_host::page_mouse(MouseEvent::Down { button: b, x, y })
				} else {
					menu_host::page_mouse(MouseEvent::Up { button: b });
					false
				}
			}
			WindowEvent::MouseWheel { delta, .. } => menu_host::page_mouse(MouseEvent::Wheel {
				lines: match delta {
					MouseScrollDelta::LineDelta(_, y) => *y,
					MouseScrollDelta::PixelDelta(p) => p.y as f32 / 120.0,
				},
			}),
			WindowEvent::Focused(false) => {
				menu_host::lose_focus();
				false
			}
			_ => false,
		}
	}
}
