//! [PC] The window side of R&C2's modern Ship Shack page (lane shack; game side `modern_shack.rs`, builder `modern_shack_page.rs`):
//! the display-frame draw (page over the world, the retail overlay left out behind it so the ship preview shows) and the mouse while
//! the page is up. Hover walks the game's cursor to the row under the mouse, a click on a row opens its "Purchase?" prompt, the
//! buttons and hints press the pad button they stand for, a right click is Triangle, the wheel walks the list. Keys and the pad reach
//! the game as always.
use crate::modern_shack::{self as v};
use crate::modern_shack_page::{self as page, Built, Facts, Hit};
use crate::modern_vendor::{Ask, CROSS_B, TRIANGLE_B};
use crate::modern_walk::{now_ms, Focus, Settle};
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
	/// The row the mouse chose (shown while the game's walk is on its way to it: `modern_walk::Focus`, time based; sweep B24).
	focus: Focus,
	/// The list <-> prompt switch clock (clicks only hover right after it; sweep B3 / B13).
	settle: Settle,
}

static SHOWN: Mutex<Option<Shown>> = Mutex::new(None);

fn lock() -> std::sync::MutexGuard<'static, Option<Shown>> {
	SHOWN.lock().unwrap_or_else(|e| e.into_inner())
}

/// Not drawn this frame (F1, load frames, a page above it): forget the window state, so `event` swallows no clicks.
pub(super) fn clear() {
	*lock() = None;
}

/// Draws the page when the game thread read a Ship Shack this field; false (and nothing drawn) otherwise.
pub(super) fn draw(g: &mut Gpu) -> bool {
	let Some(shop) = v::shop() else {
		*lock() = None;
		return false;
	};
	let mut s = lock();
	let now = now_ms();
	let (mut focus, mut settle, scroll) = s.as_ref().map_or((Focus::default(), Settle::default(), 0), |s| (s.focus, s.settle, s.built.first));
	settle.see(shop.prompt(), now);
	let shown = if shop.prompt() { None } else { focus.shown(shop.cur, shop.walk, now) };
	if shown.is_none() {
		focus.clear();
	}
	let look = super::screens::look();
	let f = Facts { shop: &shop, focus: shown, scroll, lang: look.lang, device: look.device, glyphs: look.glyphs };
	let Some(built) = page::build(&f, &FontMeasure) else {
		*s = None;
		return false;
	};
	g.ui_gpu.set_scene(built.prims.clone()); // valid for the next draw only: every frame while the page is up
	place_ship(g);
	*s = Some(Shown { built, win: g.win_size(), focus, settle });
	true
}

/// The retail overlay is left out but for the ship: its picture region (`page::SHIP`) is cut out of the overlay quad and drawn in the
/// page's window (`page::hole`) over a dark backdrop, so the ship shows and turns as in Classic.
fn place_ship(g: &mut Gpu) {
	let ov: Vec<_> = g.hud.iter().filter(|q| q.tex == RETAIL_OV_TEX).cloned().collect();
	g.hud.retain(|q| q.tex != RETAIL_OV_TEX && q.tex != RETAIL_HD_TEX);
	let hs = super::screens::hud_size(g);
	let (w, (fx, fy, fw, fh)) = (Scale::fit(hs.0, hs.1).to_window(page::hole()), page::SHIP);
	rac_engine::overlay::rect(&mut g.hud, [w.x, w.y, w.w, w.h], [0.04, 0.06, 0.10, 1.0]);
	for q in ov {
		let [u0, v0, u1, v1] = q.uv;
		let uv = [u0 + (u1 - u0) * fx, v0 + (v1 - v0) * fy, u0 + (u1 - u0) * (fx + fw), v0 + (v1 - v0) * (fy + fh)];
		g.hud.push(rac_engine::overlay::Quad { rect: [w.x, w.y, w.w, w.h], uv, ..q });
	}
}

/// What a click on `h` asks of the game (R&C1's `vendor_host::click`): a row click selects, and opens "Purchase?" only on the row
/// the page shows (the hovered one); Buy / the Cross hint open it for the SHOWN row (the walk goes there first, the Cross lands only
/// on arrival: sweep B23); within `SETTLE_MS` of a list <-> prompt switch a click only hovers a row (sweep B3).
pub fn click(h: Hit, shown: usize, prompt: bool, settling: bool) -> Option<Ask> {
	Some(match h {
		Hit::Row(i) => Ask::Row(i, i == shown && !settling && !prompt),
		_ if settling => return None,
		Hit::Buy | Hit::Hint(Symbol::Cross) if !prompt => Ask::Row(shown, true),
		Hit::Buy | Hit::Confirm | Hit::Hint(Symbol::Cross) => Ask::Press(CROSS_B),
		Hit::Cancel | Hit::Hint(Symbol::Triangle) => Ask::Press(TRIANGLE_B),
		Hit::None | Hit::Hint(_) => return None,
	})
}

/// One wheel move (`y` > 0 = up): the list walks one row (clamped, no wrap) from the row shown selected; none under the prompt.
pub fn wheel(b: &Built, focus: Option<usize>, y: f32) -> Option<(Ask, usize)> {
	if y == 0.0 || b.prompt {
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
				if !s.built.prompt && s.focus.row() != Some(i) && i != s.built.sel {
					s.focus.set(i, now_ms());
					v::ask(Ask::Row(i, false));
				}
			}
			false
		}
		WindowEvent::MouseInput { state: ElementState::Pressed, button, .. } => {
			let Some((x, y)) = ui_pos else { return true };
			let ask = match button {
				MouseButton::Left => {
					let now = now_ms();
					let a = click(at(x, y), s.focus.row().unwrap_or(s.built.sel), s.built.prompt, s.settle.settling(now));
					if let Some(Ask::Row(i, _)) = a {
						s.focus.set(i, now);
					}
					a
				}
				MouseButton::Right => Some(Ask::Press(TRIANGLE_B)),
				_ => None,
			};
			if let Some(a) = ask {
				v::ask(a);
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
				v::ask(a);
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
	fn ship_wheel_walks_the_list_and_clicks_map_to_buttons() {
		let b = Built { sel: 2, count: 4, ..Built::default() };
		assert_eq!(wheel(&b, None, -1.0), Some((Ask::Row(3, false), 3)));
		assert_eq!(wheel(&b, Some(3), -1.0), None, "clamped at the last row");
		assert_eq!(wheel(&b, Some(0), 1.0), None, "no wrap");
		let p = Built { prompt: true, ..b };
		assert_eq!(wheel(&p, None, 1.0), None, "no wheel under the prompt");
		assert_eq!(click(Hit::Row(1), 1, false, false), Some(Ask::Row(1, true)), "the shown (hovered) row: Purchase?");
		assert_eq!(click(Hit::Row(1), 2, false, false), Some(Ask::Row(1, false)), "another row: select only");
		assert_eq!(click(Hit::Row(1), 1, false, true), Some(Ask::Row(1, false)), "settling: hover only");
		assert_eq!(click(Hit::Buy, 3, false, false), Some(Ask::Row(3, true)), "Buy = the shown row, Cross on arrival");
		assert_eq!(click(Hit::Confirm, 3, true, false), Some(Ask::Press(CROSS_B)));
		assert_eq!(click(Hit::Confirm, 3, true, true), None, "right after the prompt opened");
		assert_eq!(click(Hit::Cancel, 0, true, false), Some(Ask::Press(TRIANGLE_B)));
	}
}
