//! [PC] The one cursor walk of R&C2's modern screens (build 45, P3). The modern pages do not own a selection: retail's own menu
//! cursor stays the truth, and a mouse request walks it step by step with pad pulses (two fields pressed, then released, so every
//! step is a new edge). The rules every page relies on (sweep B2 / B8 / B23: a Cross that landed on an in-between row):
//! - the confirm press of a walk (`to(target, then)`) is sent ONLY when retail's cursor, read from RAM this field, IS on the target;
//! - a walk that does not arrive within its step cap (default: the list's length + 2) gives up WITHOUT the press;
//! - a new target restarts the walk; the same target keeps its progress (a click on the row a hover is walking to only adds the press);
//! - a confirm asked while a walk runs (`confirm`) rides on that walk: it lands on the walk's target on arrival, never on the way;
//! - one walk at a time: a lone press (`press`, e.g. Triangle) ends the walk, and nothing else is sent until its pulse is over.
//! Pure: no RAM, no globals; the pages call `step` once per field with the cursor they read.

/// Fields of one pulse when no period is set (2 pressed, 2 released).
const PERIOD: u8 = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Walk {
	target: Option<usize>,
	/// The press due on arrival at `target` (0 = only walk).
	then: u16,
	/// A lone press (not tied to a row).
	press: u16,
	phase: u8,
	held: u16,
	/// Walk steps sent for `target`.
	steps: u16,
	/// Step cap (0 = the list's length + 2).
	cap: u16,
	/// Fields per pulse (0 = `PERIOD`).
	period: u8,
	/// The buttons that move the cursor one row back / forward.
	back: u16,
	fwd: u16,
	/// Presses that wait for `ready` (a page that takes a confirm only once its cursor rested).
	gated: u16,
}

impl Walk {
	/// A walk whose cursor moves back with `back` and forward with `fwd` (Up / Down for a list, Left / Right for a strip).
	pub const fn new(back: u16, fwd: u16) -> Walk {
		Walk { target: None, then: 0, press: 0, phase: 0, held: 0, steps: 0, cap: 0, period: 0, back, fwd, gated: 0 }
	}

	/// Fields per pulse (the Ship Shack takes one step per 12 fields).
	pub const fn period(mut self, fields: u8) -> Walk {
		self.period = fields;
		self
	}

	/// A fixed step cap (instead of the list's length + 2).
	pub const fn cap(mut self, steps: u16) -> Walk {
		self.cap = steps;
		self
	}

	/// Presses in `bits` (as a lone press or on arrival) wait while `step`'s `ready` is false.
	pub const fn gate(mut self, bits: u16) -> Walk {
		self.gated = bits;
		self
	}

	/// Walk retail's cursor to `target`, then press `then` there (0 = only walk). The same target keeps the walk's progress and only
	/// adds a non-zero `then`; another target restarts the walk and drops a queued lone press.
	pub fn to(&mut self, target: usize, then: u16) {
		if self.target == Some(target) {
			if then != 0 {
				self.then = then;
			}
			return;
		}
		(self.target, self.then, self.steps, self.press) = (Some(target), then, 0, 0);
	}

	/// A lone press (Triangle, Left / Right in a prompt): ends any walk (its arrival press too).
	pub fn press(&mut self, bit: u16) {
		(self.target, self.then, self.steps, self.press) = (None, 0, 0, bit);
	}

	/// A confirm (Cross) for the row the page shows: rides on a running walk (sent only on arrival at its target), else a lone press.
	pub fn confirm(&mut self, bit: u16) {
		if self.target.is_some() {
			self.then = bit;
		} else {
			self.press(bit);
		}
	}

	/// Drops the walk and any queued press (a pulse already held is let out, so no button sticks).
	pub fn cancel(&mut self) {
		(self.target, self.then, self.steps, self.press) = (None, 0, 0, 0);
	}

	/// Something is queued or a pulse is on (the page masks the player's own menu buttons meanwhile).
	pub fn busy(&self) -> bool {
		self.target.is_some() || self.press != 0 || self.phase != 0
	}

	/// The row the walk is on its way to.
	pub fn target(&self) -> Option<usize> {
		self.target
	}

	/// The press due on arrival (0 = none).
	pub fn then(&self) -> u16 {
		self.then
	}

	/// One field: the buttons the game gets from the walk. `cur` = retail's cursor read this field, `n` = rows, `ready` = the page
	/// takes a gated press now.
	pub fn step(&mut self, cur: usize, n: usize, ready: bool) -> u16 {
		if self.phase == 0 {
			self.held = self.next(cur, n, ready);
			if self.held == 0 {
				return 0;
			}
		}
		let out = if self.phase < 2 { self.held } else { 0 };
		self.phase = (self.phase + 1) % if self.period == 0 { PERIOD } else { self.period };
		out
	}

	fn next(&mut self, cur: usize, n: usize, ready: bool) -> u16 {
		if self.press != 0 {
			if self.press & self.gated != 0 && !ready {
				return 0;
			}
			return std::mem::take(&mut self.press);
		}
		let Some(t) = self.target.filter(|&t| t < n) else {
			self.cancel();
			return 0;
		};
		let cap = if self.cap == 0 { n as u16 + 2 } else { self.cap };
		if t != cur {
			if self.steps < cap {
				self.steps += 1;
				return if t > cur { self.fwd } else { self.back };
			}
			self.cancel(); // gave up: no press on a row that is not the target
			return 0;
		}
		if self.then & self.gated != 0 && !ready {
			return 0;
		}
		let then = self.then;
		self.cancel();
		then
	}
}

/// Window side: how long a mouse choice shows before the game thread has taken it (R&C1's `title::GRACE_MS`).
pub const GRACE_MS: u64 = 300;
/// Window side: after a switch between a list and its prompt, clicks only hover for this long (R&C1's `vendor_host::SETTLE_MS`).
pub const SETTLE_MS: u64 = 350;

/// Milliseconds of wall time since the first call (the window's clock for `Focus` / `Settle`; time based, so the same at any
/// refresh rate: sweep B4 / B24).
pub fn now_ms() -> u64 {
	static T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
	T0.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64
}

/// Window side: the row the mouse chose. It shows at once and stays shown while the game's walk is on its way to it (or for
/// `GRACE_MS` before the game thread took the request); it ends when retail's cursor is there, or the walk ended elsewhere.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Focus(Option<(usize, u64)>);

impl Focus {
	pub fn set(&mut self, row: usize, now: u64) {
		self.0 = Some((row, now));
	}

	pub fn clear(&mut self) {
		self.0 = None;
	}

	/// The chosen row, held or not.
	pub fn row(&self) -> Option<usize> {
		self.0.map(|f| f.0)
	}

	/// This frame's shown focus (`cur` = retail's cursor, `walk` = the game's walk target); forgets it once it is over.
	pub fn shown(&mut self, cur: usize, walk: Option<usize>, now: u64) -> Option<usize> {
		let keep = self.0.filter(|&(r, at)| r != cur && (walk == Some(r) || now.saturating_sub(at) < GRACE_MS));
		self.0 = keep;
		keep.map(|f| f.0)
	}
}

/// Window side: the list <-> prompt switch clock (R&C1's `flip_at`): within `SETTLE_MS` of a switch a click presses nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Settle {
	prompt: Option<bool>,
	at: Option<u64>,
}

impl Settle {
	/// Every drawn frame: the page's prompt state.
	pub fn see(&mut self, prompt: bool, now: u64) {
		if self.prompt.is_some_and(|p| p != prompt) {
			self.at = Some(now);
		}
		self.prompt = Some(prompt);
	}

	pub fn settling(&self, now: u64) -> bool {
		self.at.is_some_and(|at| now.saturating_sub(at) < SETTLE_MS)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn focus_holds_while_walking_and_settle_after_a_switch() {
		let mut f = Focus::default();
		f.set(5, 1000);
		assert_eq!(f.shown(2, None, 1100), Some(5), "grace before the game took it");
		assert_eq!(f.shown(3, Some(5), 9000), Some(5), "held while the walk runs, however long (any refresh rate)");
		assert_eq!(f.shown(3, None, 9001), None, "the walk gave up: the game's row shows");
		f.set(4, 0);
		assert_eq!(f.shown(4, Some(4), 10), None, "arrived");
		let mut s = Settle::default();
		s.see(false, 0);
		assert!(!s.settling(10), "opening is no switch");
		s.see(true, 100);
		assert!(s.settling(300) && !s.settling(450));
	}
	const B: u16 = 1;
	const F: u16 = 2;
	const X: u16 = 0x40;
	const T: u16 = 0x10;

	fn run(w: &mut Walk, cur: usize, n: usize, fields: usize) -> Vec<u16> {
		(0..fields).map(|_| w.step(cur, n, true)).collect()
	}

	#[test]
	fn arrives_then_presses() {
		let mut w = Walk::new(B, F);
		w.to(2, X);
		assert_eq!(run(&mut w, 0, 5, 4), vec![F, F, 0, 0]);
		assert_eq!(run(&mut w, 1, 5, 4), vec![F, F, 0, 0]);
		assert_eq!(run(&mut w, 2, 5, 4), vec![X, X, 0, 0], "the press only once the cursor IS on the target");
		assert!(!w.busy());
		w.to(0, 0);
		assert_eq!(run(&mut w, 2, 5, 1), vec![B]);
	}

	#[test]
	fn gives_up_without_press() {
		let mut w = Walk::new(B, F);
		w.to(3, X);
		let seq = run(&mut w, 0, 4, 100);
		assert_eq!(seq.iter().filter(|&&b| b == F).count(), 2 * 6, "n + 2 = 6 steps of 2 fields");
		assert!(!seq.contains(&X) && !w.busy());
		let mut w = Walk::new(B, F).cap(2);
		w.to(3, X);
		let seq = run(&mut w, 0, 4, 100);
		assert_eq!(seq.iter().filter(|&&b| b == F).count(), 4, "fixed cap of 2 steps");
		assert!(!seq.contains(&X));
	}

	#[test]
	fn target_change_restarts_and_same_target_keeps() {
		let mut w = Walk::new(B, F);
		w.to(4, 0);
		run(&mut w, 0, 6, 4);
		w.to(4, X); // a click on the row the hover walks to: only adds the press
		assert_eq!(w.target(), Some(4));
		assert_eq!(w.then(), X);
		w.to(1, 0); // the mouse moved on: the press is gone
		assert_eq!(w.then(), 0);
		assert_eq!(run(&mut w, 3, 6, 1), vec![B]);
	}

	#[test]
	fn confirm_rides_on_the_walk_and_press_ends_it() {
		let mut w = Walk::new(B, F);
		w.to(3, 0);
		w.confirm(X);
		assert_eq!(run(&mut w, 1, 6, 4), vec![F, F, 0, 0], "no Cross on the in-between row");
		assert_eq!(run(&mut w, 2, 6, 4), vec![F, F, 0, 0]);
		assert_eq!(run(&mut w, 3, 6, 2), vec![X, X]);
		let mut w = Walk::new(B, F);
		w.confirm(X);
		assert_eq!(run(&mut w, 1, 6, 2), vec![X, X], "no walk: a lone press");
		let mut w = Walk::new(B, F);
		w.to(3, X);
		w.press(T);
		assert_eq!(run(&mut w, 1, 6, 8), vec![T, T, 0, 0, 0, 0, 0, 0], "Triangle ends the walk and its press");
	}

	#[test]
	fn gated_press_waits_for_ready_and_period() {
		let mut w = Walk::new(B, F).period(12).gate(X);
		w.to(1, X);
		assert_eq!(w.step(0, 3, false), F);
		for _ in 0..11 {
			w.step(0, 3, false);
		}
		assert_eq!(w.step(1, 3, false), 0, "arrived, not rested");
		assert!(w.busy());
		assert_eq!(w.step(1, 3, true), X);
		let mut w = Walk::new(B, F);
		w.to(9, X);
		assert_eq!(run(&mut w, 0, 4, 4), vec![0; 4], "a target past the list is dropped");
		assert!(!w.busy());
	}
}
