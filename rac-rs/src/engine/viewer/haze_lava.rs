//! [PC] Heat haze (heathaze.rs): lava boxes a game pack hands in (b47 X2), for lava the engine draws as no ocean (R&C2 level 14's
//! lava is a GS layer sheet, rac2/host/ov_lava.rs). `gpu/draw/pass_post.rs` `haze_lava` chains them after the ocean boxes, so they
//! get the same hot layer, the same Fire row gate (Medium / High, Lighting Enhanced) and the same `MAX_LAVA` nearest pick. A game
//! that never calls `set` (R&C1) has none: its haze, picture and RAM are unchanged.
//! Split view: the boxes are only read while `heathaze::wanted` allows the haze (its `splitview::fx` gate), so the Classic pass has none.

use std::sync::Mutex;

static BOXES: Mutex<Vec<[f32; 6]>> = Mutex::new(Vec::new());

/// This field's lava boxes (min x, y, z, max x, y, z; z up, the top is the lava surface). An empty list = none.
pub fn set(boxes: Vec<[f32; 6]>) {
	if let Ok(mut b) = BOXES.lock() {
		*b = boxes;
	}
}

/// The boxes last set (empty when no pack sets any).
pub(crate) fn boxes() -> Vec<[f32; 6]> {
	BOXES.lock().map(|b| b.clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
	#[test]
	fn the_boxes_set_are_the_boxes_read() {
		super::set(vec![[0.0, 1.0, 2.0, 3.0, 4.0, 5.0]]);
		assert_eq!(super::boxes(), vec![[0.0, 1.0, 2.0, 3.0, 4.0, 5.0]]);
		super::set(Vec::new());
		assert!(super::boxes().is_empty());
	}
}
