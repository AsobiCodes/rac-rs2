//! [PC] The window side of R&C2's modern Galactic Map (lane scrmap; game side `modern_map.rs`, builder `modern_map_page.rs`): the
//! display-frame draw (page over the world, the retail overlay left out behind it) and the mouse while the page is up. Hover (list row
//! or chart dot) walks the game's cursor to the planet (shown at once); a click chooses it and a second click on the same planet (or
//! the Travel button / the Cross hint) travels to the planet the page shows (R&C1's two steps: sweep B9), the Cross only once retail's
//! cursor is there (sweep B8); a right click is Triangle, the wheel walks the list. Keys and the pad reach the game as always.
use crate::modern_map::{self as m, Ask, TRIANGLE_B};
use crate::modern_map_page::{self as page, Built, Facts, Hit};
use crate::modern_walk::{now_ms, Focus};
use rac_engine::overlay::Quad;
use rac_engine::ui::font::FontMeasure;
use rac_engine::ui::kit::Scale;
use rac_engine::ui::prim::Symbol;
use rac_engine::viewer::targets::{RETAIL_HD_TEX, RETAIL_OV_TEX};
use rac_engine::viewer::Gpu;
use std::sync::Mutex;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};

struct Shown {
	built: Built,
	win: (u32, u32),
	/// The entry the mouse chose (shown while the game's walk is on its way to it: `modern_walk::Focus`, time based).
	focus: Focus,
	/// The entry the last left click chose: a second click on it travels.
	armed: Option<usize>,
}

static SHOWN: Mutex<Option<Shown>> = Mutex::new(None);

fn lock() -> std::sync::MutexGuard<'static, Option<Shown>> {
	SHOWN.lock().unwrap_or_else(|e| e.into_inner())
}

/// Not drawn this frame (F1, load frames, a page above it): forget the window state, so `event` swallows no clicks.
pub(super) fn clear() {
	*lock() = None;
}

/// Draws the page when the game thread read a map this field; false (and nothing drawn) otherwise.
pub(super) fn draw(g: &mut Gpu) -> bool {
	let Some(map) = m::map() else {
		*lock() = None;
		crate::modern_map_art::place(g, false);
		return false;
	};
	let mut s = lock();
	let (mut focus, armed, scroll) = s.as_ref().map_or((Focus::default(), None, 0), |s| (s.focus, s.armed, s.built.first));
	let shown = focus.shown(map.cur, map.walk, now_ms());
	if shown.is_none() {
		focus.clear();
	}
	let look = super::screens::look();
	let pic = if super::screens_hold::picture_behind() { None } else { super::screens_vendor::picture_aspect(&g.hud) }; // [b48 Y3] B12
	let f = Facts { map: &map, focus: shown, scroll, lang: look.lang, device: look.device, glyphs: look.glyphs, art: true, t: crate::modern_map_art::clock_s(), pic }; // [b46 W5]
	let Some(built) = page::build(&f, &FontMeasure) else {
		*s = None;
		crate::modern_map_art::place(g, false);
		return false;
	};
	crate::modern_map_art::place(g, true); // [b46 W5] B12: the animated galaxy painting under the page
	g.ui_gpu.set_scene(built.prims.clone()); // valid for the next draw only: every frame while the page is up
	g.hud.retain(|q| q.tex != RETAIL_OV_TEX && q.tex != RETAIL_HD_TEX);
	if let Some(hole) = built.hole {
		// [b48 Y3] B12: the retail overlay's planet picture crop in its hole (HUD quads are in render pixels, screens.rs `hud_size`).
		let hs = super::screens::hud_size(g);
		let r = Scale::fit(hs.0, hs.1).to_window(hole);
		g.hud.push(Quad { tex: RETAIL_OV_TEX, rect: [r.x, r.y, r.w, r.h], uv: built.uv, colour: [1.0; 4], angle: 0.0, corners: None });
		if g.has_retail_hd() {
			g.hud.push(Quad { tex: RETAIL_HD_TEX, rect: [r.x, r.y, r.w, r.h], uv: built.uv, colour: [1.0; 4], angle: 0.0, corners: None });
		}
	}
	*s = Some(Shown { built, win: g.win_size(), focus, armed });
	true
}

/// What a click on `h` asks of the game, and the entry it leaves chosen (`armed`): the first click on a planet (row or chart dot)
/// chooses it, a second click on the same planet travels; Travel / the Cross hint travel to the planet the page SHOWS. A travel is a
/// row request with its Cross, sent only once retail's cursor is on that planet (sweep B8 / B9).
pub fn click(h: Hit, shown: usize, armed: Option<usize>) -> (Option<Ask>, Option<usize>) {
	match h {
		Hit::Row(i) if armed == Some(i) => (Some(Ask::Row(i, true)), None),
		Hit::Row(i) => (Some(Ask::Row(i, false)), Some(i)),
		Hit::Go | Hit::Hint(Symbol::Cross) => (Some(Ask::Row(shown, true)), None),
		Hit::Hint(Symbol::Triangle) => (Some(Ask::Press(TRIANGLE_B)), None),
		Hit::None | Hit::Hint(_) => (None, armed),
	}
}

/// One wheel move (`y` > 0 = up): the list walks one entry (clamped, no wrap) from the entry shown selected.
pub fn wheel(b: &Built, focus: Option<usize>, y: f32) -> Option<(Ask, usize)> {
	if y == 0.0 {
		return None;
	}
	let from = focus.unwrap_or(b.sel);
	let to = if y > 0.0 { from.saturating_sub(1) } else { (from + 1).min(b.count.saturating_sub(1)) };
	(to != from).then_some((Ask::Row(to, false), to))
}

/// The window event while the page is up (`ui_pos` = the cursor). True = the page took it.
pub(super) fn event(ui_pos: Option<(f32, f32)>, event: &WindowEvent) -> bool {
	let mut g = lock();
	let Some(s) = g.as_mut() else { return false };
	let at = |x: f32, y: f32| {
		let (dx, dy) = Scale::fit(s.win.0, s.win.1).to_design(x, y);
		s.built.hit(dx, dy)
	};
	match event {
		WindowEvent::CursorMoved { position, .. } => {
			if let Hit::Row(i) = at(position.x as f32, position.y as f32) {
				if s.focus.row() != Some(i) && i != s.built.sel {
					s.focus.set(i, now_ms());
					s.armed = s.armed.filter(|&a| a == i);
					m::ask(Ask::Row(i, false));
				}
			}
			false
		}
		WindowEvent::MouseInput { state: ElementState::Pressed, button, .. } => {
			let Some((x, y)) = ui_pos else { return true };
			let ask = match button {
				MouseButton::Left => {
					let (a, armed) = click(at(x, y), s.focus.row().unwrap_or(s.built.sel), s.armed);
					s.armed = armed;
					if let Some(Ask::Row(i, _)) = a {
						s.focus.set(i, now_ms());
					}
					a
				}
				MouseButton::Right => Some(Ask::Press(TRIANGLE_B)),
				_ => None,
			};
			if let Some(a) = ask {
				m::ask(a);
			}
			true // a click on the page never reaches the game (the left button is the game's Circle)
		}
		WindowEvent::MouseWheel { delta, .. } => {
			let y = match delta {
				MouseScrollDelta::LineDelta(_, y) => *y,
				MouseScrollDelta::PixelDelta(p) => p.y as f32,
			};
			if let Some((a, to)) = wheel(&s.built, s.focus.row(), y) {
				s.focus.set(to, now_ms());
				s.armed = None;
				m::ask(a);
			}
			true
		}
		_ => false,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn map_click_chooses_then_travels_the_shown_planet() {
		assert_eq!(click(Hit::Row(3), 1, None), (Some(Ask::Row(3, false)), Some(3)), "first click chooses");
		assert_eq!(click(Hit::Row(3), 3, Some(3)), (Some(Ask::Row(3, true)), None), "second click travels");
		assert_eq!(click(Hit::Row(4), 3, Some(3)), (Some(Ask::Row(4, false)), Some(4)), "another planet: chooses it");
		assert_eq!(click(Hit::Go, 5, None), (Some(Ask::Row(5, true)), None), "Travel = the shown planet, Cross on arrival");
		assert_eq!(click(Hit::None, 5, Some(2)), (None, Some(2)));
		assert_eq!(click(Hit::Hint(Symbol::Triangle), 5, Some(2)).0, Some(Ask::Press(TRIANGLE_B)));
	}
}
