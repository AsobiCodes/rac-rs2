//! The effects under a frozen-screen menu (round 3 batch 4, issues 3 / 14 / 15: "in the pause menu the glass is not visible at all").
//! Everything the native world does not draw itself (ship glass, glows, pickups' sprites, fire) reaches the picture through the GS
//! overlay's effects plane. A pause or title menu is drawn by the game over the picture it froze (engine `gsfull/grab.rs`): the world
//! is held by `freeze.rs`, but the field's effects plane is empty (the game runs no 3D then; with the sharp HUD the menu is in
//! `Frame::hd`, not in `rgba`), so the glass and the glows vanished while the world stayed. Here the last live effects plane (texels and
//! depth) is kept and shown again under the menu until the run ends: the retail frozen picture has them too.
//! Not used when the sharp HUD is off (`hd` None): the menu is then in the plane itself.

use super::frame;
use rac_engine::gsfull::ovl_pipe::Frame;
use rac_engine::viewer::Gpu;

/// The last live overlay plane: size, texels (the engine's transmission alpha) and the effects' view depth.
struct Plane {
	w: u32,
	h: u32,
	px: Vec<[u8; 4]>,
	depth: Vec<f32>,
}

#[derive(Default)]
pub(super) struct Held {
	plane: Option<Plane>,
}

/// Is this frame a menu over a frozen picture whose effects plane the host can replace by the held one?
pub(super) fn holds(f: &Frame, have: bool) -> bool {
	have && f.frozen && f.overlay && f.hd.is_some()
}

impl Held {
	/// Hands the frame's picture to the engine: a frozen menu field shows the held effects plane (and its own sharp HUD), any other
	/// field its own plane, which becomes the held one when it is a live overlay field with a sharp HUD.
	pub(super) fn show(&mut self, g: &mut Gpu, f: &Frame) {
		if holds(f, self.plane.is_some()) {
			let p = self.plane.as_ref().unwrap();
			g.set_retail_overlay(p.w, p.h, &p.px, Default::default(), false);
			g.set_retail_overlay_depth(p.w, p.h, Some(&p.depth[..]).filter(|d| !d.is_empty()));
			if let Some(h) = &f.hd {
				g.set_retail_hd(h.w, h.rows, &h.rgba);
			}
			return;
		}
		let px = frame::px(f);
		g.set_retail_overlay(f.w, f.h, &px, Default::default(), false);
		g.set_retail_overlay_depth(f.w, f.h, Some(&f.depth[..]).filter(|d| f.overlay && !d.is_empty()));
		match &f.hd {
			Some(h) => g.set_retail_hd(h.w, h.rows, &h.rgba),
			None => g.clear_retail_hd(),
		}
		if f.overlay && f.hd.is_some() && !f.frozen {
			self.plane = Some(Plane { w: f.w, h: f.h, px, depth: f.depth.clone() });
		} else if !f.overlay {
			self.plane = None;
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn frame(overlay: bool, frozen: bool, hd: bool) -> Frame {
		Frame { field: 0, w: 2, h: 2, rgba: vec![0; 16], overlay, depth: Vec::new(), hd: hd.then(|| rac_engine::gscap::Hd { w: 1, rows: 1, rgba: vec![[0; 4]] }), frozen, no_world: false, layers: Vec::new() }
	}

	#[test]
	fn only_a_frozen_overlay_field_with_a_sharp_hud_takes_the_held_plane() {
		assert!(holds(&frame(true, true, true), true));
		assert!(!holds(&frame(true, true, true), false), "nothing held yet");
		assert!(!holds(&frame(true, false, true), true), "a live field shows its own effects");
		assert!(!holds(&frame(true, true, false), true), "without the sharp HUD the menu is in the plane");
		assert!(!holds(&frame(false, true, true), true), "a whole picture (title, load) is not an overlay");
	}
}
