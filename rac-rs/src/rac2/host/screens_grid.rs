//! [PC] The window side of R&C2's modern Weapons and Gadgets pages (lane pages; game side `modern_grid.rs`, builder
//! `modern_grid_page.rs`): the display-frame draw (the page over the world; the retail overlay picture is left out except ONE crop of
//! it, the 3D preview of Ratchet, drawn into the detail panel's hole) and the mouse while the page is up. Hover walks the game's
//! cursor to the cell under the mouse, a left click on a cell or the Equip button equips it (Cross once the cursor is there), a
//! right click is Triangle. Keys and the pad reach the game as always.
use crate::modern_grid as mg;
use crate::modern_grid_page::{self as page, Built};
use crate::modern_tabs::Click;
use rac_engine::ui::font::FontMeasure;
use rac_engine::ui::kit::page::{PageLayout, PageScreen};
use rac_engine::ui::kit::{page_draw, Scale};
use rac_engine::viewer::Gpu;
use std::sync::Mutex;
use winit::event::{ElementState, MouseButton, WindowEvent};

struct Shown {
	built: Built,
	win: (u32, u32),
	sel: (usize, usize),
	/// The cell the mouse last asked for (a hover asks the game once per cell).
	hover: Option<(usize, usize)>,
	/// [PC] nav lane: the shell (tab bar, hints) as drawn, and the page's tab (tab clicks, modern_tabs.rs).
	shell: (PageLayout, PageScreen, usize),
}

static SHOWN: Mutex<Option<Shown>> = Mutex::new(None);

fn lock() -> std::sync::MutexGuard<'static, Option<Shown>> {
	SHOWN.lock().unwrap_or_else(|e| e.into_inner())
}

/// Not drawn this frame (F1, load frames, a page above it): forget the window state, so `event` swallows no clicks.
pub(super) fn clear() {
	*lock() = None;
}

/// Draws the page when the game thread read a grid page this field; false (and nothing drawn) otherwise.
pub(super) fn draw(g: &mut Gpu, aspect: f32) -> bool {
	let Some(mut f) = mg::facts() else {
		*lock() = None;
		return super::screens_items::draw(g, aspect); // the Items page, when that is up
	};
	super::screens_items::clear();
	// [PC] nav lane: the cell a mouse request is taking the cursor to is drawn selected at once (its name, its ammo).
	if let Some(p) = mg::pending().filter(|p| f.cell(p.0, p.1).is_some()) {
		f.sel = p;
	}
	let look = super::screens::look();
	let built = page::build_look(&f, &FontMeasure, aspect, &look);
	let screen = page::shell(&f, look);
	let (layout, shell) = page_draw::frame(&screen, &FontMeasure);
	let scene = page::with_grid(shell, &built);
	super::screens_switch::keep(&scene, Some(built.hole)); // [PC] nav lane: the picture while a tab click switches pages
	// The scene, the retail overlay left out but for the crop into the hole (filled while the picture is behind: screens_preview.rs).
	super::screens_preview::draw(g, scene, built.hole, built.uv);
	let win = g.win_size();
	let mut s = lock();
	let hover = s.as_ref().and_then(|s| s.hover);
	*s = Some(Shown { built, win, sel: f.sel, hover, shell: (layout, screen, f.page.tab()) });
	true
}

/// The window event while the page is up (`ui_pos` = the cursor). True = the page took it.
pub(super) fn event(ui_pos: Option<(f32, f32)>, event: &WindowEvent) -> bool {
	let mut g = lock();
	let Some(s) = g.as_mut() else { return super::screens_items::event(ui_pos, event) };
	let design = |x: f32, y: f32| Scale::fit(s.win.0, s.win.1).to_design(x, y);
	match event {
		WindowEvent::CursorMoved { position, .. } => {
			let (dx, dy) = design(position.x as f32, position.y as f32);
			match s.built.cell_at(dx, dy) {
				Some(c) if s.hover != Some(c) && c != s.sel => {
					s.hover = Some(c);
					mg::request(c, false, false);
				}
				Some(_) => {}
				None => s.hover = None,
			}
			false
		}
		WindowEvent::MouseInput { state: ElementState::Pressed, button, .. } => {
			let Some((x, y)) = ui_pos else { return true };
			let (dx, dy) = design(x, y);
			match button {
				// [PC] nav lane: the tab bar and the Back hint (modern_tabs.rs).
				MouseButton::Left => match crate::modern_tabs::click(&s.shell.0, &s.shell.1, s.shell.2, dx, dy) {
					Click::Tab => {}
					Click::Back => mg::request(s.sel, false, true),
					Click::None => {
						if let Some(c) = s.built.cell_at(dx, dy) {
							mg::request(c, true, false);
						} else if s.built.on_button(dx, dy) {
							mg::request(s.sel, true, false);
						}
					}
				},
				MouseButton::Right => mg::request(s.sel, false, true),
				_ => {}
			}
			true // a click on the page never reaches the game (the left button is the game's Circle)
		}
		_ => false,
	}
}
