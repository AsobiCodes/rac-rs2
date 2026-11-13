//! [PC] [b45 P1] Window-side system features R&C2 lacked (sweep E H3, M1, L2), the same engine calls as R&C1's
//! `host/handler.rs`: keep-awake (a gamepad player's screen saver), the live Display mode (windowed / borderless fullscreen), the
//! window icon.

use std::sync::Mutex;
use winit::window::{Fullscreen, Icon};

/// The logo, 64x64 raw RGBA (assets/icon/rac-rs-64.rgba), as R&C1's window uses it.
pub fn icon() -> Option<Icon> {
	Icon::from_rgba(include_bytes!("../../../assets/icon/rac-rs-64.rgba").to_vec(), 64, 64).ok()
}

/// A hidden agent test window (RAC_TEST_WINDOW=hidden, or the smoke check's RAC_RETAIL_SMOKE): it never goes fullscreen.
pub fn test_window() -> bool {
	std::env::var_os("RAC_TEST_WINDOW").is_some_and(|v| v == "hidden") || std::env::var_os("RAC_RETAIL_SMOKE").is_some()
}

/// Display mode: the `set_fullscreen` argument when the wanted mode (`want`, true = fullscreen) differs from the window's (`is`):
/// borderless fullscreen on the current monitor (no video mode change), or None = back to the window.
pub fn display_change(want: bool, is: bool) -> Option<Option<Fullscreen>> {
	(want != is).then(|| want.then(|| Fullscreen::Borderless(None)))
}

/// Real gamepad input (a button or stick moved since the last poll) resets the idle timers: a pad does not count as activity for
/// Windows (engine/awake.rs, throttled to once a second). Scripted pads never reach here.
pub fn pad_activity(now: &rac_engine::input::pad::PadState) {
	static PREV: Mutex<Option<rac_engine::input::pad::PadState>> = Mutex::new(None);
	let Ok(mut p) = PREV.lock() else { return };
	if p.as_ref().is_some_and(|prev| rac_engine::input::pad::pad_activity(prev, now)) {
		rac_engine::awake::pad_input();
	}
	*p = Some(*now);
}

#[cfg(test)]
mod tests {
	use super::display_change;

	#[test]
	fn display_mode_changes_only_when_it_differs() {
		assert!(display_change(true, true).is_none());
		assert!(display_change(false, false).is_none());
		assert!(matches!(display_change(true, false), Some(Some(_))));
		assert!(matches!(display_change(false, true), Some(None)));
	}
}
