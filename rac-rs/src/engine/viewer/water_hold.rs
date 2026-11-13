//! [PC] The Enhanced water's clock held while the game shows a frozen picture (R&C2: the pause and title menus over the frame the game
//! grabbed, rac2/host/freeze.rs). The native world under such a menu is the held scene of the field before the freeze, standing still,
//! as retail's grabbed picture does; the water's waves, ripples, flows, wakes and rings (all on `WaterFx::clock`) stand still with it
//! and go on from where they stood when the run ends (no jump: the held stretch is taken off the clock). The host sets the flag every
//! redraw; never set for R&C1 (default off: the clock is the wall clock as before, bit for bit).
//! Split view: not an effect of its own; it only holds the clock the Enhanced water passes read, and they gate themselves.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static HOLD: AtomicBool = AtomicBool::new(false);
static CLOCK: Mutex<Hold> = Mutex::new(Hold { since: None, off: 0.0 });

/// The next draws hold the water's clock (`on`), or let it run again.
pub fn set(on: bool) {
	HOLD.store(on, Ordering::Relaxed);
}

/// The water's clock for the running clock `raw` (seconds): `raw` less the held stretches, standing while held.
pub(crate) fn clock(raw: f32) -> f32 {
	CLOCK.lock().map_or(raw, |mut c| c.at(raw, HOLD.load(Ordering::Relaxed)))
}

/// The held clock: the running clock's reading when the hold began, and the seconds held so far (taken off).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Hold {
	since: Option<f32>,
	off: f32,
}

impl Hold {
	fn at(&mut self, raw: f32, hold: bool) -> f32 {
		match (hold, self.since) {
			(true, None) => self.since = Some(raw),
			(false, Some(s)) => {
				// A test clock may read a little back between two fields: never a negative stretch.
				self.off += (raw - s).max(0.0);
				self.since = None;
			}
			_ => {}
		}
		self.since.unwrap_or(raw) - self.off
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn never_held_is_the_running_clock() {
		let mut h = Hold { since: None, off: 0.0 };
		for t in [0.0f32, 0.016, 1.5, 3599.9, 12345.678] {
			assert_eq!(h.at(t, false).to_bits(), t.to_bits());
		}
	}

	#[test]
	fn a_hold_stands_and_the_clock_goes_on_without_a_jump() {
		let mut h = Hold { since: None, off: 0.0 };
		assert_eq!(h.at(10.0, false), 10.0);
		// held from 10.5: stands there however long the menu is up
		assert_eq!(h.at(10.5, true), 10.5);
		assert_eq!(h.at(11.0, true), 10.5);
		assert_eq!(h.at(70.0, true), 10.5);
		// released at 70.25: on from 10.5 + what ran since the release
		assert_eq!(h.at(70.25, false), 10.5);
		assert_eq!(h.at(71.0, false), 11.25);
		// a second hold adds its own stretch
		assert_eq!(h.at(72.0, true), 12.25);
		assert_eq!(h.at(80.0, false), 12.25);
		assert_eq!(h.at(80.5, false), 12.75);
	}

	#[test]
	fn a_clock_read_back_at_the_release_takes_nothing_off() {
		let mut h = Hold { since: None, off: 0.0 };
		h.at(5.0, true);
		assert_eq!(h.at(4.98, false), 4.98);
		assert_eq!(h.off, 0.0);
	}
}
