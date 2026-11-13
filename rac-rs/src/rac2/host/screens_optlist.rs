//! [PC] The window side of R&C2's modern Options list (build 45 P4B; game side `modern_optlist.rs`, builder
//! `modern_optlist_page.rs`): the display-frame draw (the page over the world in the pause, over the GS picture at the title; the
//! retail menu picture left out behind it) and the mouse and the question's keys while it is up. Hover = the row under the pointer
//! (a retail row: retail's cursor walks there; a PC row: focus), click = pick it (Cross only on arrival), right click = Back,
//! a tab click = that pause tab (modern_tabs.rs). Keys and the pad otherwise reach the game's list as always.
use super::screens::look;
use crate::modern_optlist::{self as ol, Req, TAB};
use crate::modern_optlist_page as page;
use crate::modern_screen::Kind;
use crate::modern_tabs::{self, Click};
use rac_engine::ui::font::FontMeasure;
use rac_engine::ui::kit::data::HintKind;
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
	hover: Option<usize>,
}

static SHOWN: Mutex<Option<Shown>> = Mutex::new(None);

fn lock() -> std::sync::MutexGuard<'static, Option<Shown>> {
	SHOWN.lock().unwrap_or_else(|e| e.into_inner())
}

/// Display-frame step (host/screens.rs `pages`): `read` = the readers may read (UI pipes fine, no engine page), `show` = the page
/// may draw, `world` = the engine draws the world. True = the list drew.
pub(super) fn draw(g: &mut Gpu, read: bool, show: bool, world: bool) -> bool {
	ol::set_ready(read && world, read);
	let f = if show { ol::facts() } else { None };
	let Some(f) = f.filter(|f| f.kind == Kind::Title || world) else {
		*lock() = None;
		return false;
	};
	let screen = page::build(&f, look());
	let (layout, prims) = page_draw::frame(&screen, &FontMeasure);
	g.ui_gpu.set_scene(prims); // valid for the next draw only: every frame while the page is up
	g.hud.retain(|q| q.tex != RETAIL_OV_TEX && q.tex != RETAIL_HD_TEX);
	let mut s = lock();
	let hover = s.as_ref().and_then(|s| s.hover);
	*s = Some(Shown { screen, layout, win: g.win_size(), hover });
	true
}

/// What a left click at design point (x, y) asks. Pure.
fn left_click(screen: &PageScreen, layout: &PageLayout, x: f32, y: f32) -> Option<Req> {
	if screen.shell.is_some() && screen.dialog.is_none() {
		match modern_tabs::click(layout, screen, TAB, x, y) {
			Click::Tab => return None,
			Click::Back => return Some(Req::Back),
			Click::None => {}
		}
	}
	match layout.hit(screen, x, y) {
		PageHit::Item(i) => Some(Req::Row(i, true)),
		PageHit::Button(k) => Some(Req::Answer(k == 0)),
		PageHit::Hint(HintKind::Back | HintKind::Peek) => Some(if screen.dialog.is_some() { Req::Answer(false) } else { Req::Back }),
		_ => None,
	}
}

/// Window event while the list is shown (before the Options page's and the game's). True = the list took it.
pub(super) fn event(ui_pos: Option<(f32, f32)>, event: &WindowEvent) -> bool {
	let mut g = lock();
	let Some(s) = g.as_mut() else { return false };
	let design = |s: &Shown, x: f32, y: f32| Scale::fit(s.win.0, s.win.1).to_design(x, y);
	match event {
		WindowEvent::CursorMoved { position, .. } => {
			let (x, y) = design(s, position.x as f32, position.y as f32);
			match s.layout.hit(&s.screen, x, y) {
				PageHit::Item(i) if s.screen.dialog.is_none() => {
					if s.hover != Some(i) {
						s.hover = Some(i);
						ol::request(Req::Row(i, false));
					}
				}
				_ => s.hover = None,
			}
			false
		}
		WindowEvent::MouseInput { state: ElementState::Pressed, button, .. } => {
			let Some((x, y)) = ui_pos else { return true };
			let (x, y) = design(s, x, y);
			let req = match button {
				MouseButton::Left => left_click(&s.screen, &s.layout, x, y),
				MouseButton::Right if s.screen.dialog.is_some() => Some(Req::Answer(false)),
				MouseButton::Right => Some(Req::Back),
				_ => None,
			};
			if let Some(r) = req {
				ol::request(r);
			}
			true // a click on the page never reaches the game (the left button is the game's Circle)
		}
		WindowEvent::KeyboardInput { event, .. } if s.screen.dialog.is_some() && event.state == ElementState::Pressed => match event.physical_key {
			PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter) => {
				ol::request(Req::Answer(true));
				true
			}
			PhysicalKey::Code(KeyCode::Escape | KeyCode::Backspace) => {
				ol::request(Req::Answer(false));
				true
			}
			_ => false,
		},
		_ => false,
	}
}
