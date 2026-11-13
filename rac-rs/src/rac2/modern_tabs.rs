//! [PC] Tab-bar clicks on R&C2's modern pause sub pages (lane nav, build 44; user report b43: "a click on another tab of the tab bar
//! does nothing on a sub page"). The window (host/screens_grid.rs, screens_qs.rs, screens_items.rs) asks for a tab; the sub page's
//! filter sends Triangle (retail goes back to the pause root), then the root's filter writes the root cursor to the tab and presses
//! Cross in the same field (no walk over the rows, no move sound). The click is heard once: the Triangle's back sound is hushed
//! (`hush` / `hushed`, R&C1's snd.rs `hush_batch`: only bank voice starts are dropped, stream voices start as always). Nothing runs
//! without a click: no RAM write, no change to the pad or the sound. R&C1 has no key tab switch on sub pages either (child_tabs.rs).
use rac_engine::spumix::Batch;
use rac_engine::ui::kit::data::HintKind;
use rac_engine::ui::kit::page::{PageHit, PageLayout, PageScreen};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

/// A switch older than this many fields is dropped (no menu took it).
const MAX_AGE: u32 = 120;
/// Fields between two Crosses at the root while it stays up (a press it did not take).
const RESEND: u32 = 8;
/// Retail's page change leaves ONE field with no page current ([G+0x14] = 0, [G+0x18] = the next page) [C: RAM dumps of fields
/// 119-126 / 129-132 / 189-193, Weapons -> root -> Quick Select by click and by keys, fg1 0357, 2026-11-12]; more fields with no page
/// = the pause closed (Start mid-switch): the switch is dropped, so a later pause does not open the tab.
const GAP: u8 = 2;
/// Fields the window keeps the last sub page on screen while a switch runs (host/screens_switch.rs); a switch takes 4-5 fields
/// [measured: click -> asked page shown 4 fields, Options 5].
const HOLD: u32 = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Switch {
	pub tab: usize,
	age: u32,
	/// The field age of the last Triangle sent on the sub page (resent after `RESEND` fields while it stays up, B28).
	back: Option<u32>,
	/// The field age of the last Cross at the root.
	cross: Option<u32>,
	/// Fields in a row with no menu page current.
	idle: u8,
}

impl Switch {
	pub fn new(tab: usize) -> Switch {
		Switch { tab, ..Switch::default() }
	}

	/// A sub page `own` is up: true = send Triangle now. Pure.
	pub fn on_sub(&mut self, own: usize) -> (bool, bool) {
		if own == self.tab {
			return (false, true);
		}
		let send = self.back.is_none_or(|b| self.age >= b + RESEND);
		if send {
			self.back = Some(self.age);
		}
		(send, false)
	}

	/// The root is up: true = write the cursor and press Cross now. Pure.
	pub fn on_root(&mut self) -> bool {
		if self.cross.is_some_and(|c| self.age < c + RESEND) {
			return false;
		}
		self.cross = Some(self.age);
		true
	}

	/// No root is up; `current` = some other menu page is (else retail is between pages or the pause closed). True = the switch is
	/// over: a page after the Cross (the Options tab opens no modern page), or more than `GAP` fields with no page. Pure.
	pub fn gone(&mut self, current: bool) -> bool {
		if current {
			self.idle = 0;
			return self.cross.is_some();
		}
		self.idle += 1;
		self.idle > GAP
	}
}

static SW: Mutex<Option<Switch>> = Mutex::new(None);
static HUSH: AtomicU32 = AtomicU32::new(0);

fn lock() -> std::sync::MutexGuard<'static, Option<Switch>> {
	SW.lock().unwrap_or_else(|e| e.into_inner())
}

/// Window thread: a click on tab `tab` of a sub page's tab bar.
pub fn request(tab: usize) {
	*lock() = Some(Switch::new(tab));
}

/// What a left click did on a sub page's shell (the tab bar and the hint bar).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
	/// Not on the shell: the page's own cells and buttons.
	None,
	/// A tab (another one asks a switch; the page's own tab does nothing).
	Tab,
	/// The Back hint: the page sends Triangle.
	Back,
}

/// Window thread: a left click at design point (`x`, `y`) on a sub page whose shell is `screen` laid out as `layout`, `own` = its tab.
pub fn click(layout: &PageLayout, screen: &PageScreen, own: usize, x: f32, y: f32) -> Click {
	match layout.hit(screen, x, y) {
		PageHit::Menu(i) if screen.menu.get(i).is_some_and(|m| m.live) => {
			if i != own {
				request(i);
			}
			Click::Tab
		}
		PageHit::Hint(HintKind::Back | HintKind::Peek) => Click::Back, // [b46 U6] Peek = the pad's Triangle Back (modern_pages::back_kind)
		_ => Click::None,
	}
}

/// Game thread, a sub page's filter (its tab `own`): true = press Triangle now (once per switch; its sound is hushed). The page of
/// the asked tab ends the switch.
pub fn want_back(own: usize) -> bool {
	let mut g = lock();
	let Some(s) = g.as_mut() else { return false };
	let (send, done) = s.on_sub(own);
	if done {
		*g = None;
	}
	if send {
		hush(2);
	}
	send
}

/// Game thread, the pause root's filter: the tab to open now (write the root cursor + Cross), None = nothing to do.
pub fn take_root() -> Option<usize> {
	let mut g = lock();
	let s = g.as_mut()?;
	s.on_root().then_some(s.tab)
}

/// Game thread, the root's filter when no root is up (`current` = another menu page is): `Switch::gone`.
pub fn root_gone(current: bool) {
	let mut g = lock();
	if g.as_mut().is_some_and(|s| s.gone(current)) {
		*g = None;
	}
}

/// Window thread: a switch runs and is young enough for the window to keep the last sub page on screen (host/screens_switch.rs).
pub fn holding() -> bool {
	lock().is_some_and(|s| s.age <= HOLD)
}

/// Game thread, once per field (modern_jump.rs `apply`): ages the switch, drops a stale one.
pub fn tick() {
	let mut g = lock();
	if let Some(s) = g.as_mut() {
		s.age += 1;
		if s.age > MAX_AGE {
			*g = None;
		}
	}
}

/// Game thread: this field and the next `fields - 1` start no bank voice (a pulse sent only to walk retail's menu).
pub fn hush(fields: u32) {
	HUSH.store(fields, Ordering::Relaxed);
}

/// Game thread, each field's sound batch right after `spumix::drain` (host/game.rs): a hushed field's batch keys on no bank voice;
/// stream voices still start (their key-on follows a `StreamSrc` in the same batch; a dropped one silences the music, R&C1 #74).
pub fn hushed(mut b: Batch) -> Batch {
	use ee_oracle::snd989::voices::Reg;
	if HUSH.load(Ordering::Relaxed) == 0 {
		return b;
	}
	HUSH.fetch_sub(1, Ordering::Relaxed);
	let mut stream = [0u32; 2]; // per core: voices that got a stream this batch
	for e in b.ev.iter().filter(|e| e.w.reg == Reg::StreamSrc) {
		let v = e.w.index as usize;
		if v < 48 {
			stream[v / 24] |= 1 << (v % 24);
		}
	}
	b.ev.retain_mut(|e| {
		if e.w.reg != Reg::Kon {
			return true;
		}
		e.w.value &= stream.get(e.w.index as usize).copied().unwrap_or(0);
		e.w.value != 0
	});
	b
}

#[cfg(test)]
#[path = "modern_tabs_tests.rs"]
mod tests;
