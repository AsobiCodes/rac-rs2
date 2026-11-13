//! [PC] The window side of R&C2's modern vendor page (lane scr; game side `modern_vendor.rs`, builder `modern_vendor_page.rs`): the
//! display-frame draw (page over the world, the retail overlay left out behind it except ONE crop of it, the selected item's 3D
//! model, drawn into the card's hole as screens_grid.rs does with Ratchet) and the mouse while the page is up. Hover walks
//! the game's cursor to the row under the mouse (shown at once), a click on a row buys it, the buttons and hints press the pad
//! button they stand for, a right click is Triangle, the wheel walks the list (browse) or steps the quantity (prompt; up = more),
//! like R&C1's `vendor_host::wheel`. The Buy all ammo row (R&C1's): a hover or the wheel up from row 0 puts the focus on it, a click
//! on it or on its card's button starts the run. Keys and the pad reach the game as always.
use crate::modern_vendor::{self as v, Ask, CROSS_B, TRIANGLE_B};
use crate::modern_vendor_page::{self as page, Built, Facts, Hit};
use crate::modern_screen::{LEFT, RIGHT};
use crate::modern_walk::{now_ms, Focus, Settle};
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
	/// The row the mouse chose (shown while the game's walk is on its way to it: `modern_walk::Focus`, time based; sweep B4).
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

/// Draws the page when the game thread read a shop this field; false (and nothing drawn) otherwise.
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
	let model = picture_aspect(&g.hud);
	let f = Facts { shop: &shop, focus: shown, scroll, lang: look.lang, device: look.device, glyphs: look.glyphs, model };
	let Some(built) = page::build(&f, &FontMeasure) else {
		*s = None;
		return false;
	};
	g.ui_gpu.set_scene(built.prims.clone()); // valid for the next draw only: every frame while the page is up
	g.hud.retain(|q| q.tex != RETAIL_OV_TEX && q.tex != RETAIL_HD_TEX);
	if let Some(hole) = built.hole {
		// HUD quads are in RENDER pixels (the config size), not the window's (screens.rs `hud_size`).
		let hs = super::screens::hud_size(g);
		let r = Scale::fit(hs.0, hs.1).to_window(hole);
		g.hud.push(Quad { tex: RETAIL_OV_TEX, rect: [r.x, r.y, r.w, r.h], uv: built.uv, colour: [1.0; 4], angle: 0.0, corners: None });
		if g.has_retail_hd() {
			g.hud.push(Quad { tex: RETAIL_HD_TEX, rect: [r.x, r.y, r.w, r.h], uv: built.uv, colour: [1.0; 4], angle: 0.0, corners: None });
		}
	}
	*s = Some(Shown { built, win: g.win_size(), focus, settle });
	true
}

/// The game picture's aspect (width / height) as this frame's retail overlay quads show it: the quad covering the most of the
/// overlay (the whole picture, or the centre cell when the HUD layout cut the corners out) maps its uv span to pixels. None = no
/// overlay this frame (nothing to crop).
pub(super) fn picture_aspect(hud: &[Quad]) -> Option<f32> {
	let area = |q: &Quad| (q.uv[2] - q.uv[0]) * (q.uv[3] - q.uv[1]);
	let q = hud.iter().filter(|q| q.tex == RETAIL_OV_TEX && q.uv[2] > q.uv[0] && q.uv[3] > q.uv[1] && q.rect[2] > 0.0 && q.rect[3] > 0.0).max_by(|a, b| area(a).total_cmp(&area(b)))?;
	Some((q.rect[2] / (q.uv[2] - q.uv[0])) / (q.rect[3] / (q.uv[3] - q.uv[1])))
}

/// What a click on `h` asks of the game (R&C1's `vendor_host::click` / `buy`): a row click selects, and buys only the row the page
/// shows (the hovered one; not while Buy all ammo has the focus); Buy / the Cross hint buy the SHOWN row (the walk goes there first,
/// the Cross lands only on arrival: sweep B2) or run Buy all ammo when it has the focus; within `SETTLE_MS` of a list <-> prompt
/// switch a click only hovers a row (sweep B3).
pub fn click(h: Hit, shown: usize, prompt: bool, settling: bool, buyall: bool) -> Option<Ask> {
	Some(match h {
		Hit::Row(i) => Ask::Row(i, i == shown && !settling && !prompt && !buyall),
		_ if settling => return None,
		Hit::AllRow | Hit::BuyAll => Ask::BuyAll,
		Hit::Buy | Hit::Hint(Symbol::Cross) if !prompt && buyall => Ask::BuyAll,
		Hit::Buy | Hit::Hint(Symbol::Cross) if !prompt => Ask::Row(shown, true),
		Hit::Buy | Hit::Confirm | Hit::Hint(Symbol::Cross) => Ask::Press(CROSS_B),
		Hit::Cancel | Hit::Hint(Symbol::Triangle) => Ask::Press(TRIANGLE_B),
		Hit::Less => Ask::Press(LEFT),
		Hit::More => Ask::Press(RIGHT),
		Hit::None | Hit::Hint(_) => return None,
	})
}

/// One wheel move (`y` > 0 = up): the prompt steps the quantity (up = more), the list walks one row (clamped, no wrap) from the row
/// shown selected; the row it chose becomes the shown focus. Up from row 0 goes to the Buy all ammo row, down from it to row 0.
pub fn wheel(b: &Built, focus: Option<usize>, y: f32) -> Option<(Ask, Option<usize>)> {
	if y == 0.0 {
		return None;
	}
	if b.prompt {
		return Some((Ask::Press(if y > 0.0 { RIGHT } else { LEFT }), None));
	}
	if b.buyall {
		return (y < 0.0).then_some((Ask::AllFocus(false), Some(0)));
	}
	let from = focus.unwrap_or(b.sel);
	if y > 0.0 && from == 0 && b.all_row.is_some() {
		return Some((Ask::AllFocus(true), None));
	}
	let to = if y > 0.0 { from.saturating_sub(1) } else { (from + 1).min(b.count.saturating_sub(1)) };
	(to != from).then_some((Ask::Row(to, false), Some(to)))
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
			match at(position.x as f32, position.y as f32) {
				Hit::Row(i) if !s.built.prompt && (s.built.buyall || (s.focus.row() != Some(i) && i != s.built.sel)) => {
					s.focus.set(i, now_ms());
					v::ask(Ask::Row(i, false)); // also takes the focus off Buy all ammo
				}
				Hit::AllRow if !s.built.buyall => {
					s.focus.clear();
					v::ask(Ask::AllFocus(true));
				}
				_ => {}
			}
			false
		}
		WindowEvent::MouseInput { state: ElementState::Pressed, button, .. } => {
			let Some((x, y)) = ui_pos else { return true };
			let ask = match button {
				// The Cross hint on the Buy all ammo row starts the run (Cross there is Buy all ammo, as the keys).
				MouseButton::Left => {
					let now = now_ms();
					let a = click(at(x, y), s.focus.row().unwrap_or(s.built.sel), s.built.prompt, s.settle.settling(now), s.built.buyall);
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
			if let Some((a, focus)) = wheel(&s.built, s.focus.row(), y) {
				if focus.is_some() || a == Ask::AllFocus(true) {
					match focus {
						Some(r) => s.focus.set(r, now_ms()),
						None => s.focus.clear(),
					}
				}
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
	fn vendor_wheel_walks_the_list_and_steps_the_quantity() {
		let b = Built { sel: 2, count: 4, ..Built::default() };
		assert_eq!(wheel(&b, None, -1.0), Some((Ask::Row(3, false), Some(3))), "down = the next row");
		assert_eq!(wheel(&b, Some(3), -1.0), None, "clamped at the last row");
		assert_eq!(wheel(&b, Some(1), 1.0), Some((Ask::Row(0, false), Some(0))), "up from the shown focus");
		assert_eq!(wheel(&b, Some(0), 1.0), None, "no wrap");
		let p = Built { prompt: true, sel: 2, count: 4, ..Built::default() };
		assert_eq!(wheel(&p, None, 1.0), Some((Ask::Press(RIGHT), None)), "up = more");
		assert_eq!(wheel(&p, None, -2.0), Some((Ask::Press(LEFT), None)));
		assert_eq!(wheel(&p, None, 0.0), None);
		let r = rac_engine::ui::prim::Rect::new(0.0, 0.0, 1.0, 1.0);
		let top = Built { sel: 0, count: 4, all_row: Some(r), ..Built::default() };
		assert_eq!(wheel(&top, None, 1.0), Some((Ask::AllFocus(true), None)), "up from row 0 = Buy all ammo");
		let all = Built { buyall: true, all_row: Some(r), ..top.clone() };
		assert_eq!(wheel(&all, None, 1.0), None, "the top");
		assert_eq!(wheel(&all, None, -1.0), Some((Ask::AllFocus(false), Some(0))), "down = row 0");
		assert_eq!(click(Hit::AllRow, 0, false, false, false), Some(Ask::BuyAll));
		assert_eq!(click(Hit::BuyAll, 0, false, false, false), Some(Ask::BuyAll));
	}

	#[test]
	fn vendor_click_selects_then_buys_the_shown_row() {
		assert_eq!(click(Hit::Row(2), 2, false, false, false), Some(Ask::Row(2, true)), "the shown (hovered) row buys");
		assert_eq!(click(Hit::Row(2), 0, false, false, false), Some(Ask::Row(2, false)), "another row: select only");
		assert_eq!(click(Hit::Row(2), 2, false, false, true), Some(Ask::Row(2, false)), "Buy all ammo has the focus: select only");
		assert_eq!(click(Hit::Row(2), 2, false, true, false), Some(Ask::Row(2, false)), "settling: hover only");
		assert_eq!(click(Hit::Buy, 5, false, false, false), Some(Ask::Row(5, true)), "Buy = the shown row, Cross on arrival (B2)");
		assert_eq!(click(Hit::Hint(Symbol::Cross), 5, false, false, true), Some(Ask::BuyAll));
		assert_eq!(click(Hit::Confirm, 5, true, false, false), Some(Ask::Press(CROSS_B)));
		assert_eq!(click(Hit::Confirm, 5, true, true, false), None, "a click right after the prompt opened (B3)");
		assert_eq!(click(Hit::Less, 5, true, false, false), Some(Ask::Press(LEFT)));
	}
}
