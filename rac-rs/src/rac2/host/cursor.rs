//! The mouse cursor while the Modern camera plays (lane scratch_rc2_cam, stage A 2026-10-31, B1 the same day; the idea of R&C1's
//! `host/handler/frame_start.rs` grab + `cursor_vis.rs`, cut to what R&C2 has): in gameplay with the Modern camera the cursor is held
//! in the window and hidden, so the mouse looks around; it is let go on focus loss, in a movie or menu (no 3D view), in the test window
//! (`RAC_TEST_WINDOW=hidden`) and in window-shot runs (those never touch the cursor).
//!
//! Gameplay = the follow camera is running: `modcam_hand::follow_ran_recently(field)` (no R&C2 pause cell is known [?], and none is
//! needed: the pause menu, the other menus and the cutscenes do not call the follow camera, so the cursor is let go two fields
//! after it stops). With the Classic camera nothing is ever grabbed and the window's cursor is never touched.

use winit::window::{CursorGrabMode, Window};

/// Whether the cursor should be held now. `playing` = a 3D view is on screen and no movie runs; `following` = the follow camera ran
/// in the last fields (`modcam_hand::follow_ran_recently`); `test_window` = `RAC_TEST_WINDOW=hidden`; `shots` = a window-shot run.
/// [b45 P1] `steers` = the mouse steers the game this field whatever the camera mode (a ship fight, the Quick Select wheel:
/// `pc_input::owns_motion`): held in Classic too, as R&C1 holds the cursor whenever the mouse drives gameplay (frame_start.rs).
pub fn want_grab(modern: bool, focused: bool, playing: bool, following: bool, steers: bool, test_window: bool, shots: bool) -> bool {
	focused && playing && ((modern && following) || steers) && !test_window && !shots
}

/// The window's cursor state (window thread only).
#[derive(Default)]
pub struct Cursor {
	grabbed: bool,
}

impl Cursor {
	/// Whether the cursor is held (and hidden) right now.
	pub fn grabbed(&self) -> bool {
		self.grabbed
	}

	/// Moves the window to `want`: grabs `Confined` (else `Locked`) and hides the cursor, or releases and shows it. The window is
	/// touched only when the want changes, so with the Classic camera (never wants) it is never touched.
	pub fn update(&mut self, w: &Window, want: bool) {
		if want == self.grabbed {
			return;
		}
		if want {
			let ok = w.set_cursor_grab(CursorGrabMode::Confined).or_else(|_| w.set_cursor_grab(CursorGrabMode::Locked)).is_ok();
			if ok {
				w.set_cursor_visible(false);
				self.grabbed = true;
			}
		} else {
			// The visibility is engine/cursor_vis.rs's to decide (shown only while the mouse is the device in use).
			let _ = w.set_cursor_grab(CursorGrabMode::None);
			self.grabbed = false;
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn grab_only_in_modern_gameplay_with_the_focus() {
		assert!(want_grab(true, true, true, true, false, false, false));
		assert!(!want_grab(false, true, true, true, false, false, false), "Classic follow-only never grabs");
		assert!(!want_grab(true, false, true, true, false, false, false), "no focus");
		assert!(!want_grab(true, true, false, true, true, false, false), "menu, movie or load");
		assert!(!want_grab(true, true, true, false, false, false, false), "the follow camera is silent: pause menu, cutscene");
		assert!(!want_grab(true, true, true, true, true, true, false), "hidden test window");
		assert!(!want_grab(true, true, true, true, true, false, true), "window-shot run");
		assert!(want_grab(false, true, true, false, true, false, false), "a ship fight steers with the mouse in Classic too");
		assert!(!want_grab(false, false, true, false, true, false, false), "focus loss lets go");
	}
}
