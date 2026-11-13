//! [PC] nav lane (build 44): the picture while a tab click switches pages (modern_tabs.rs). Retail walks through its pause root:
//! Triangle, one field with no page current, the root for one field, Cross, again one field with no page, the asked page
//! [C: RAM dumps, modern_tabs.rs `GAP`]. Drawn as it comes that is the bare world, the root's lone tab bar, the bare world again
//! (3 frames of blink per click). While the switch runs, the last sub page's scene is drawn instead until the asked page is up.
//! Its preview hole would show the world then (the retail overlay holds the root, no preview): it is filled with retail's own
//! empty-preview colour, the look of every preview change on these pages [C: window shots hov_new 170-172, rgb 2,27,40].
//! Next to lane flk's frame-level hold (host/screens_hold.rs, retail's page gaps without a click): while a switch runs this one
//! draws (screens_qs.rs `draw` returns true), so the frame counts as a modern draw and that hold stays idle; it takes over only
//! once `modern_tabs::holding()` ended and still no page draws.
use rac_engine::ui::prim::{Prim, Rect, Rgba};
use rac_engine::viewer::targets::{RETAIL_HD_TEX, RETAIL_OV_TEX};
use rac_engine::viewer::Gpu;
use std::sync::Mutex;

/// Retail's preview panel while its picture loads (dark blue, opaque).
pub const EMPTY_PREVIEW: Rgba = Rgba::rgb(0x021b28);

static HELD: Mutex<Option<Vec<Prim>>> = Mutex::new(None);

fn lock() -> std::sync::MutexGuard<'static, Option<Vec<Prim>>> {
	HELD.lock().unwrap_or_else(|e| e.into_inner())
}

/// The scene drawn while a switch runs: the page's own, its preview hole (design units) filled. Pure.
pub fn held(prims: &[Prim], hole: Option<Rect>) -> Vec<Prim> {
	let mut v = prims.to_vec();
	if let Some(rect) = hole {
		v.push(Prim::RRect { rect, radius: 0.0, fill: EMPTY_PREVIEW, border: None });
	}
	v
}

/// A sub page's draw: its scene and preview hole, kept only while a switch runs (no copy otherwise).
pub(super) fn keep(prims: &[Prim], hole: Option<Rect>) {
	let mut h = lock();
	*h = crate::modern_tabs::holding().then(|| held(prims, hole));
}

/// A display frame where no sub page drew: true = the kept scene was drawn (a switch runs).
pub(super) fn draw(g: &mut Gpu) -> bool {
	let mut h = lock();
	if !crate::modern_tabs::holding() {
		*h = None;
		return false;
	}
	let Some(p) = h.clone() else { return false };
	g.ui_gpu.set_scene(p); // valid for the next draw only
	g.hud.retain(|q| q.tex != RETAIL_OV_TEX && q.tex != RETAIL_HD_TEX);
	true
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn modern_switch_fills_the_preview_hole_last() {
		let base = [Prim::ClipPop];
		let r = Rect::new(10.0, 20.0, 30.0, 40.0);
		let v = held(&base, Some(r));
		assert_eq!(v.len(), 2);
		assert!(matches!(v[0], Prim::ClipPop));
		assert!(matches!(&v[1], Prim::RRect { rect, fill, border: None, .. } if *rect == r && *fill == EMPTY_PREVIEW && fill.a == 255));
		assert_eq!(held(&base, None).len(), 1, "a page without a hole is kept as it is");
	}
}
