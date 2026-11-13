//! [PC] fx1 (build 44): the preview hole of the modern Weapons / Gadgets / Items / Help pages never shows the world. The hole is a
//! crop of the retail overlay picture (Ratchet with the weapon, the item, the Help row's icon), and the GS picture lags the readers'
//! facts: on a page's first field the picture still holds the page before it (the root: nothing there, so the world showed through
//! the hole for one field at every page open and every tab click) [C: combined check A on b44 m5, chk_a\runs\p1s opens 102, 182,
//! 342, 422, 482 and tab clicks 505..765, 2026-11-12]. Until the picture holds the page (`screens_hold::behind`) the hole is filled
//! with retail's own empty-preview colour (screens_switch.rs `EMPTY_PREVIEW`, what retail itself shows there for the next fields
//! while its picture loads). The frame's hole is noted for the hold (screens_hold.rs `frame`), which redraws a kept page with the
//! hole filled (the retail overlay is left out then: at a back the picture already holds the next page).
use rac_engine::overlay::Quad;
use rac_engine::ui::kit::Scale;
use rac_engine::ui::prim::{Prim, Rect};
use rac_engine::viewer::targets::{RETAIL_HD_TEX, RETAIL_OV_TEX};
use rac_engine::viewer::Gpu;
use std::sync::Mutex;

/// A page's preview as drawn this display frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preview {
	/// The hole (design units).
	pub hole: Rect,
}

static FRAME: Mutex<Option<Preview>> = Mutex::new(None);

fn lock() -> std::sync::MutexGuard<'static, Option<Preview>> {
	FRAME.lock().unwrap_or_else(|e| e.into_inner())
}

/// The preview a page drew this display frame (once: the hold takes it after the pages' draws).
pub(super) fn take() -> Option<Preview> {
	lock().take()
}

/// The scene of a page with a preview: the page's own, its hole filled unless the crop shows (`live`). Pure.
pub fn scene(prims: Vec<Prim>, hole: Rect, live: bool) -> Vec<Prim> {
	if live {
		prims
	} else {
		super::screens_switch::held(&prims, Some(hole))
	}
}

/// A page with a preview hole draws: its scene (valid for the next draw only), the retail overlay left out, the crop `uv` drawn into
/// `hole` once the picture holds the page.
pub(super) fn draw(g: &mut Gpu, prims: Vec<Prim>, hole: Rect, uv: [f32; 4]) {
	let live = !super::screens_hold::picture_behind();
	*lock() = Some(Preview { hole });
	g.ui_gpu.set_scene(scene(prims, hole, live));
	g.hud.retain(|q| q.tex != RETAIL_OV_TEX && q.tex != RETAIL_HD_TEX);
	if live {
		crop(g, hole, uv);
	}
}

/// The crop's quads into the hole: the overlay picture, and the sharp HUD's copy when there is one.
fn crop(g: &mut Gpu, hole: Rect, uv: [f32; 4]) {
	let hs = super::screens::hud_size(g);
	let r = Scale::fit(hs.0, hs.1).to_window(hole);
	g.hud.push(Quad { tex: RETAIL_OV_TEX, rect: [r.x, r.y, r.w, r.h], uv, colour: [1.0; 4], angle: 0.0, corners: None });
	if g.has_retail_hd() {
		g.hud.push(Quad { tex: RETAIL_HD_TEX, rect: [r.x, r.y, r.w, r.h], uv, colour: [1.0; 4], angle: 0.0, corners: None });
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use super::super::screens_switch::EMPTY_PREVIEW;

	#[test]
	fn a_page_behind_its_picture_fills_the_hole() {
		let hole = Rect::new(600.0, 140.0, 500.0, 300.0);
		let live = scene(vec![Prim::ClipPop], hole, true);
		assert_eq!(live.len(), 1, "the crop shows: the scene is the page's own");
		let filled = scene(vec![Prim::ClipPop], hole, false);
		assert_eq!(filled.len(), 2);
		assert!(matches!(&filled[1], Prim::RRect { rect, fill, .. } if *rect == hole && *fill == EMPTY_PREVIEW && fill.a == 255), "filled last, opaque");
	}
}
