//! [PC] The window side of R&C2's modern pause and title pages (lane scr; game side `modern_screen.rs`, builders `modern_pages.rs`):
//! the display-frame step (page drawn over the game, the retail menu picture left out behind it) and the window's mouse and keys
//! while a page is up. Hover moves the game's cursor to the row under the mouse, a click picks it, a right click goes back; Enter /
//! Esc answer the title's Quit question. Keys and the pad otherwise reach the game as always (its own menu code moves the cursor).
//! Nothing runs without a page but a flag read and a lock.
use super::App;
use crate::modern_pages::{self, Look};
use crate::modern_screen::{self as scr, Facts, Kind};
use rac_engine::ui::font::FontMeasure;
use rac_engine::ui::kit::page::{PageHit, PageLayout, PageScreen};
use rac_engine::ui::kit::{page_draw, Scale};
use rac_engine::ui::{self, gpu as ui_gpu};
use rac_engine::viewer::targets::{RETAIL_HD_TEX, RETAIL_OV_TEX};
use rac_engine::viewer::Gpu;
use std::sync::Mutex;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

struct Shown {
	screen: PageScreen,
	layout: PageLayout,
	win: (u32, u32),
	/// The row the mouse last hovered (a hover asks the game once per row).
	hover: Option<usize>,
}

static SHOWN: Mutex<Option<Shown>> = Mutex::new(None);

fn lock() -> std::sync::MutexGuard<'static, Option<Shown>> {
	SHOWN.lock().unwrap_or_else(|e| e.into_inner())
}

/// The HUD list's pixel space: the render size (`config`; the engine's prepare.rs maps it to clip space), NOT the window. With an
/// upscaler (DLSS / FSR) or a Render scale below 100 % window pixels put an overlay crop off (too far right and down, too big: user
/// screenshot 2026-11-12, Weapons page with DLSS Quality); R&C1 does the same (rac1/host/handler/frame_ui.rs). Mouse hits stay in
/// window pixels (`g.win_size()`). Uniform scale, so the design rects still fit.
pub(super) fn hud_size(g: &Gpu) -> (u32, u32) {
	(g.config.width, g.config.height)
}

pub(super) fn look() -> Look {
	let Some(p) = rac_engine::pack::get().ui else { return Look::default() };
	Look { lang: (p.lang)().min(4), device: if crate::pc_prompts::keyboard_in_use() { ui::Device::Keyboard } else { ui::Device::Pad }, glyphs: ui::GlyphSet::from_index((p.glyphs)()) }
}

/// The modern pages in `step`'s draw order. A page whose draw does not run this frame keeps its last window state unless cleared,
/// and its `event` (run before the Options page's and the game's) would swallow every click: F1, load frames, Options up.
const PAGES: [fn(); 7] = [
	super::screens_vendor::clear,
	super::screens_ship::clear,
	super::screens_map::clear,
	super::screens_grid::clear,
	super::screens_items::clear,
	super::screens_qs::clear,
	super::screens_load::clear, // [PC] ld lane
];

/// The slot pages' place in `PAGES`: the only pages drawn without the world (at the title).
const LOAD: usize = 6;

fn clear_pages(from: usize) {
	PAGES[from..].iter().for_each(|c| c());
}

/// The pages `pages` clears before its draws (pure; `ok` = they may draw): all when they may not; without the world all but the slot
/// pages, which may still draw (the title) and clear themselves when they do not (screens_load.rs `draw`). [PC] fx2: a clear every
/// title frame dropped the hover's aim before each draw, so the highlight walked card by card behind retail's cursor (check A2,
/// d1f 700..705).
fn cleared(ok: bool, world: bool) -> std::ops::Range<usize> {
	match (ok, world) {
		(false, _) => 0..PAGES.len(),
		(true, false) => 0..LOAD,
		(true, true) => 0..0,
	}
}

/// Display-frame step after the Options page's (`menu::step`). `world`: the engine draws the world this frame; `done`: the GS fields
/// done of the picture shown (host/frame.rs `redraw`).
pub(super) fn step(g: &mut Gpu, world: bool, aspect: f32, done: u64) {
	let ok = !ui_gpu::refused() && !ui::is_open();
	// [PC] Debug mode's planet list is up: the modern page steps aside (pc_devtools.rs; R&C1 pause.rs Gate::list_open). fx1: only the
	// drawing does; the readers' READY and the kept scene stay, so the page is back on the first frame after the list closes (check A
	// run dl: one classic Weapons field 169 when READY dropped with the list).
	let show = ok && !crate::pc_devtools::list_open();
	super::screens_hold::begin(done); // [PC] fx1: the picture's fields for the preview holes (screens_preview.rs)
	// The readers' READY stays on across one display frame without the world (screens_hold.rs `world_latch`).
	let ready = ok && super::screens_hold::world_latch(world);
	scr::set_ready(ready, ok);
	// The modern vendor (screens_vendor.rs) is the picture while a shop is up; no root page can be up then.
	crate::modern_vendor::set_ready(ready);
	crate::modern_shack::set_ready(ready);
	crate::modern_grid::set_ready(ready);
	super::screens_qs::set_ready(ready);
	crate::modern_map::set_ready(ready);
	let drew = pages(g, ok, show, world, aspect, done);
	// [PC] The last modern page kept over retail's short gaps (page changes, the close, transient cursor fields: screens_hold.rs);
	// left as it is while the planet list is up.
	if show || !ok {
		super::screens_hold::frame(g, drew, ok && world, done);
	}
}

/// The modern pages in draw order; true = one drew this frame. `ok`: the readers may read; `show`: the pages may draw; `done`: the
/// GS fields done of the picture shown.
fn pages(g: &mut Gpu, ok: bool, show: bool, world: bool, aspect: f32, done: u64) -> bool {
	let ok_read = ok;
	let ok = show;
	PAGES[cleared(ok, world)].iter().for_each(|c| c());
	// [PC] ld lane: the slot pages' world record, every display frame (in GS fields: screens_load.rs `world_seen`).
	let slot_world = super::screens_load::world_seen(world, done);
	if ok && world && super::screens_vendor::draw(g) {
		clear_pages(1);
		*lock() = None;
		return true;
	}
	if ok && world && super::screens_ship::draw(g) {
		clear_pages(2);
		*lock() = None;
		return true;
	}
	// The modern Galactic Map (screens_map.rs) is the picture while the map page is up.
	if ok && world && super::screens_map::draw(g) {
		clear_pages(3);
		*lock() = None;
		return true;
	}
	// The modern Weapons / Gadgets pages (screens_grid.rs; it clears the Items page itself when the grid is up).
	if ok && world && super::screens_grid::draw(g, aspect) {
		clear_pages(5);
		*lock() = None;
		return true;
	}
	// The modern Quick Select / Help pages (screens_qs.rs).
	if ok && world && super::screens_qs::draw(g, aspect) {
		*lock() = None;
		return true;
	}
	// [PC] ld lane: the modern Load / Save Game slot pages (screens_load.rs), on the title and in the pause menu.
	crate::modern_load::set_ready(ok_read, ok_read && slot_world);
	if ok && super::screens_load::draw(g, slot_world) {
		*lock() = None;
		return true;
	}
	if super::screens_optlist::draw(g, ok_read, ok, world) { // [b45 P4B] the modern Options list (pause and title)
		*lock() = None;
		return true;
	}
	let facts = if ok { scr::facts() } else { None };
	let Some(f) = facts.filter(|f| f.kind == Kind::Title || world) else {
		*lock() = None;
		return false;
	};
	draw(g, &f);
	true
}

fn draw(g: &mut Gpu, f: &Facts) {
	let screen = modern_pages::build(f, scr::quit_state(), look());
	let (layout, prims) = page_draw::frame(&screen, &FontMeasure);
	g.ui_gpu.set_scene(prims); // valid for the next draw only: every frame while the page is up
	g.hud.retain(|q| q.tex != RETAIL_OV_TEX && q.tex != RETAIL_HD_TEX);
	let mut s = lock();
	let hover = s.as_ref().and_then(|s| s.hover);
	*s = Some(Shown { screen, layout, win: g.win_size(), hover });
}

/// The page's widget under a window point.
fn hit(s: &Shown, x: f32, y: f32) -> PageHit {
	let (dx, dy) = Scale::fit(s.win.0, s.win.1).to_design(x, y);
	s.layout.hit(&s.screen, dx, dy)
}

impl App {
	/// Called first in `window_event` (before the Options page's). True = the page took the event (the game never sees it).
	pub(super) fn screen_event(&mut self, event: &WindowEvent) -> bool {
		// [b45 P4A] U10: while a tab switch runs no page is up; a mouse press then would reach the game as its Circle. Swallowed.
		if crate::modern_tabs::holding() && matches!(event, WindowEvent::MouseInput { state: ElementState::Pressed, .. }) {
			return true;
		}
		if super::screens_vendor::event(self.ui_pos, event) || super::screens_ship::event(self.ui_pos, event) || super::screens_grid::event(self.ui_pos, event) || super::screens_qs::event(self.ui_pos, event) {
			return true;
		}
		if super::screens_load::event(self.ui_pos, event) {
			return true; // [PC] ld lane
		}
		if super::screens_optlist::event(self.ui_pos, event) { // [b45 P4B]
			return true;
		}
		if super::screens_map::event(self.ui_pos, event) {
			return true;
		}
		let mut g = lock();
		let Some(s) = g.as_mut() else { return false };
		match event {
			WindowEvent::CursorMoved { position, .. } => {
				let PageHit::Menu(i) = hit(s, position.x as f32, position.y as f32) else {
					s.hover = None;
					return false;
				};
				if s.hover != Some(i) && s.screen.menu.get(i).is_some_and(|m| m.live) {
					s.hover = Some(i);
					scr::request(i, false, false);
				}
				false
			}
			WindowEvent::MouseInput { state: ElementState::Pressed, button, .. } => {
				let Some((x, y)) = self.ui_pos else { return true };
				match (button, hit(s, x, y)) {
					(MouseButton::Left, PageHit::Menu(i)) if s.screen.menu.get(i).is_some_and(|m| m.live) => scr::request(i, true, false),
					(MouseButton::Left, PageHit::Button(k)) => scr::answer(k == 0),
					(MouseButton::Right, _) if s.screen.dialog.is_some() => scr::answer(false),
					(MouseButton::Right, _) if s.screen.shell.is_some() => scr::request(0, false, true),
					_ => {}
				}
				true // a click on the page never reaches the game (the left button is the game's Circle)
			}
			WindowEvent::KeyboardInput { event, .. } if s.screen.dialog.is_some() && event.state == ElementState::Pressed => match event.physical_key {
				PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter) => {
					scr::answer(true);
					true
				}
				PhysicalKey::Code(KeyCode::Escape | KeyCode::Backspace) => {
					scr::answer(false);
					true
				}
				_ => false,
			},
			_ => false,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_slot_pages_stay_without_the_world_while_they_may_draw() {
		assert_eq!(LOAD, PAGES.len() - 1, "the slot pages come last");
		assert_eq!(cleared(false, true), 0..PAGES.len(), "the pages may not draw: every one forgets its window state");
		assert_eq!(cleared(false, false), 0..PAGES.len());
		// The title (no world): the hover and the aim of the slot cards survive to the next draw (check A2: the bar walked).
		assert_eq!(cleared(true, false), 0..LOAD);
		assert!(!cleared(true, false).contains(&LOAD));
		assert!(cleared(true, true).is_empty(), "with the world each page clears below the one that drew");
	}
}
