//! [PC] [b57 NG, ledger R10] Window side of the title's memory-card message boxes (game side `modern_card_box.rs`): the title root with
//! retail's box as a modern dialog. Its buttons, Enter (the first button) and Esc / Backspace / a right click (Triangle) ask the game for
//! the button the label shows; a key repeat never answers; a request made for another box is dropped by the game side (its key).
use crate::modern_card_box as cb;
use rac_engine::ui::font::FontMeasure;
use rac_engine::ui::kit::page::{PageHit, PageLayout, PageScreen};
use rac_engine::ui::kit::{page_draw, Scale};
use rac_engine::viewer::targets::{RETAIL_HD_TEX, RETAIL_OV_TEX};
use rac_engine::viewer::Gpu;
use std::sync::Mutex;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

struct Shown {
	screen: PageScreen,
	layout: PageLayout,
	win: (u32, u32),
	bx: cb::CardBox,
}

static SHOWN: Mutex<Option<Shown>> = Mutex::new(None);

fn lock() -> std::sync::MutexGuard<'static, Option<Shown>> {
	SHOWN.lock().unwrap_or_else(|e| e.into_inner())
}

pub(super) fn clear() {
	*lock() = None;
}

/// Draws the box when the game thread read one this field; false (nothing drawn) otherwise.
pub(super) fn draw(g: &mut Gpu) -> bool {
	let Some(bx) = cb::facts() else {
		clear();
		return false;
	};
	let screen = cb::screen(&bx, super::screens::look());
	let (layout, prims) = page_draw::frame(&screen, &FontMeasure);
	g.ui_gpu.set_scene(prims);
	g.hud.retain(|q| q.tex != RETAIL_OV_TEX && q.tex != RETAIL_HD_TEX);
	*lock() = Some(Shown { screen, layout, win: g.win_size(), bx });
	true
}

/// The window event while the box is up. True = taken (the game never sees a mouse press while the box is up).
pub(super) fn event(ui_pos: Option<(f32, f32)>, event: &WindowEvent) -> bool {
	let g = lock();
	let Some(s) = g.as_ref() else { return false };
	// A box the game changed since the last redraw: its events are still taken (never passed on to the pages below), but ask nothing.
	let live = cb::facts().is_some_and(|f| f.key == s.bx.key);
	let ask = |bit: u16| {
		if live {
			cb::request(bit, s.bx.key);
		}
	};
	match event {
		WindowEvent::MouseInput { state: ElementState::Pressed, button, .. } => {
			let Some((x, y)) = ui_pos else { return true };
			let (dx, dy) = Scale::fit(s.win.0, s.win.1).to_design(x, y);
			match (button, s.layout.hit(&s.screen, dx, dy)) {
				(MouseButton::Left, PageHit::Button(k)) => {
					if let Some(b) = s.bx.buttons.get(k) {
						ask(b.bit);
					}
				}
				(MouseButton::Right, _) => {
					if let Some(bit) = cb::cancel_bit(&s.bx) {
						ask(bit);
					}
				}
				_ => {}
			}
			true
		}
		WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed && !event.repeat => {
			let bit = match event.physical_key {
				PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter) => cb::confirm_bit(&s.bx),
				PhysicalKey::Code(KeyCode::Escape | KeyCode::Backspace) => cb::cancel_bit(&s.bx),
				_ => return false,
			};
			if let Some(bit) = bit {
				ask(bit);
			}
			true
		}
		_ => false,
	}
}
