//! [PC] Mouse hover and click in R&C2's RETAIL (Classic style) Gadgetron vendor (parity row 48; the pause root's twin is
//! `pc_menu_hover.rs`, the Modern vendor page owns the mouse itself: `host/screens_vendor.rs`). The item under the pointer on the
//! bottom strip becomes the selected one by Left / Right pulses on the pad (the same `modern_vendor::Pulse` walk the Modern page
//! uses; nothing is written to the game), closed loop on the game's own row cursor (`modern_vendor::read`, found by code shape per
//! level). A click is the Menu layer's Cross: it is held back while the walk runs and pressed on arrival, so it buys the item
//! under the pointer; a click on the Exit plate is Triangle instead.
//!
//! Picture facts [C] (fractions of the retail picture; our window shots of the Oozla vendor, state q1794424713041_5861, cursor 3
//! and 5, agree): the strip is centred on the CURRENT item (cell k at x = 0.5 + (k - cur) * 0.1094, 7 cells -3..+3, the outer ones
//! faded), y 0.76..0.94; the Buy plate is x 0.025..0.27, the Exit plate x 0.734..0.975, both y 0.367..0.456.
use crate::modern_vendor::{self as v, Ask, Pulse};
use std::sync::atomic::{AtomicBool, Ordering};

const CROSS: u16 = 0x40;
const TRIANGLE: u16 = 0x10;
const PAD_LR: u16 = 0x8000 | 0x2000;
const PITCH: f32 = 0.1094;
const REACH: i32 = 3;
const STRIP_Y: (f32, f32) = (0.76, 0.94);
const EXIT_X: (f32, f32) = (0.734, 0.975);
const PLATE_Y: (f32, f32) = (0.367, 0.456);
const YES_X: (f32, f32) = (0.025, 0.27);
/// [b48 Y4] The Ship Shack's plates [C measured on `b48/y4/runs/shack/shots/w00130.png` and `shack2/shots/w00260.png`, 1024 x 768]: both on
/// the right (x 805..975 px), Buy / Yes on the upper line (y 352..386), Exit / No on the lower (y 387..420). Its strip is the vendor's.
const SHACK_X: (f32, f32) = (800.0 / 1024.0, 980.0 / 1024.0);
const SHACK_BUY_Y: (f32, f32) = (350.0 / 768.0, 386.0 / 768.0);
const SHACK_EXIT_Y: (f32, f32) = (386.0 / 768.0, 424.0 / 768.0);
/// The Ship Shack takes a step every 12 fields and the part of a Cross only once the cursor rested 26 fields (modern_shack.rs).
const SHACK_PERIOD: u8 = 12;
const SETTLE: u32 = 26;

/// The left mouse button is down (window thread; only the window's own run uses it).
static LEFT: AtomicBool = AtomicBool::new(false);

/// Window: the left button went down / up.
pub fn left(down: bool) {
	LEFT.store(down, Ordering::Relaxed);
}

/// [b47 X4] The left button is down now (the window's press, or its tap latch: a short click lasts TAP_FIELDS fields).
pub fn left_now() -> bool {
	LEFT.load(Ordering::Relaxed)
}

/// The strip cell under (fx, fy) as an offset from the current item (-3..=3).
pub fn strip_offset(fx: f32, fy: f32) -> Option<i32> {
	if !(STRIP_Y.0..=STRIP_Y.1).contains(&fy) {
		return None;
	}
	let k = ((fx - 0.5) / PITCH).round();
	((fx - 0.5 - k * PITCH).abs() <= PITCH * 0.5 && k.abs() <= REACH as f32).then_some(k as i32)
}

/// The Exit plate (a click there is Triangle).
pub fn on_exit(fx: f32, fy: f32) -> bool {
	(EXIT_X.0..=EXIT_X.1).contains(&fx) && (PLATE_Y.0..=PLATE_Y.1).contains(&fy)
}

/// [b48 Y4] The Ship Shack's Buy / Yes plate and Exit / No plate.
pub fn shack_buy(fx: f32, fy: f32) -> bool {
	(SHACK_X.0..=SHACK_X.1).contains(&fx) && (SHACK_BUY_Y.0..SHACK_BUY_Y.1).contains(&fy)
}

pub fn shack_exit(fx: f32, fy: f32) -> bool {
	(SHACK_X.0..=SHACK_X.1).contains(&fx) && (SHACK_EXIT_Y.0..=SHACK_EXIT_Y.1).contains(&fy)
}

/// The weapon-mod vendor (level 3's Platinum shop and its twins): the same screen words as the Gadgetron shop (found by the same
/// code shape, `modern_vendor_find`) but its rows are 0x10 bytes {kind, item, ?, text id} ended by item 0, and none of them is a
/// priced weapon, so `modern_vendor::read` gives nothing there [C: RAM of state q1794424782355_9259 at field 60: cur 2 at
/// block+0x80, five rows from block+0x140, item 0x1a = Blitz Gun at the selected third cell]. (cursor, rows, a prompt is up).
fn mod_view(ram: &[u8], ) -> Option<(usize, usize, bool)> {
	let rd = |a: u32| ram.get(a as usize..a as usize + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]));
	let level = rd(crate::modern_screen::LEVEL)?;
	if rd(if level == 1 { v::MODE_L1 } else { v::MODE })? != v::VENDOR_MODE {
		return None;
	}
	let w = crate::modern_vendor_find::words(ram, level)?;
	let n = (0..v::MAX_ROWS as u32).take_while(|k| rd(w.rows + k * 0x10 + 4).is_some_and(|i| i != 0 && i < 0x80)).count();
	let cur = rd(w.cur)? as usize;
	(n > 0 && cur < n).then(|| (cur, n, rd(w.stage).unwrap_or(0) != 0))
}

/// The Buy plate (browse) / Yes plate (the "Purchase?" prompt): a click there is Cross.
pub fn on_yes(fx: f32, fy: f32) -> bool {
	(YES_X.0..=YES_X.1).contains(&fx) && (PLATE_Y.0..=PLATE_Y.1).contains(&fy)
}

#[derive(Default)]
pub struct VHover {
	/// Pointer reader (window pointer or RAC2_MOUSE_SCRIPT), shared with the pause root's.
	ptr: crate::pc_menu_hover::Hover,
	last: Option<(f32, f32)>,
	pulse: Pulse,
	/// [b48 Y4] Ship Shack: the cursor of the last field and the fields it rested, a click held back until it rested, a Cross to give.
	last_cur: usize,
	still: u32,
	hold: bool,
	fire: u8,
}

impl VHover {
	/// One field. `b` = the buttons so far; returns them with the walk's Left / Right pulses and the Cross / Triangle of a click.
	pub fn field(&mut self, ram: &[u8], field: u64, live: bool, b: u16) -> u16 {
		// The Modern vendor / Ship Shack pages (modern_vendor.rs, modern_shack.rs) drive their own mouse: never two drivers on one screen.
		let modern = v::shop().is_some() || crate::modern_shack::shop().is_some();
		let shack = if modern { None } else { crate::modern_shack::read(ram, &|_| None, &|_| None) }; // [b48 Y4] the Classic Ship Shack
		let shop = if modern {
			None
		} else if let Some(sh) = &shack {
			Some((sh.cur, sh.entries.len(), sh.prompt()))
		} else {
			v::read(ram, &|_| None).map(|s| (s.cur, s.rows.len(), s.prompt())).or_else(|| mod_view(ram))
		};
		let shack = shack.is_some();
		let Some((cur, n, prompt)) = shop else {
			(self.pulse, self.last) = (Pulse::default(), None);
			(self.last_cur, self.still, self.hold, self.fire) = (usize::MAX, 0, false, 0);
			return b;
		};
		if shack {
			self.pulse = std::mem::take(&mut self.pulse).slow(SHACK_PERIOD);
			self.still = if cur == self.last_cur { self.still.saturating_add(1) } else { 0 };
			self.last_cur = cur;
		}
		let (exit_at, yes_at): (fn(f32, f32) -> bool, fn(f32, f32) -> bool) = if shack { (shack_exit, shack_buy) } else { (on_exit, on_yes) };
		let moved = self.ptr.moved(field, live);
		// A scripted pointer reports every field: only a NEW position is a move (the strip scrolls under a fixed pointer).
		if let Some(p) = moved.filter(|p| self.last != Some(*p)) {
			self.last = Some(p);
			let off = (!prompt).then(|| strip_offset(p.0, p.1)).flatten();
			if let Some(t) = off.and_then(|o| usize::try_from(cur as i32 + o).ok()).filter(|&t| t < n && t != cur) {
				self.pulse.ask(Ask::Row(t, false)); // the same target keeps its walk
				self.hold = false; // [b48 R1] a click held back was for the cell the pointer has left
			}
		}
		if b & PAD_LR != 0 {
			(self.pulse, self.hold, self.fire) = (Pulse::default(), false, 0); // the player's own Left / Right win
			return b;
		}
		let mut b = b;
		let mouse = live && LEFT.load(Ordering::Relaxed) || std::env::var_os("RAC2_MOUSE_SCRIPT").is_some();
		if b & CROSS != 0 && mouse && self.last.is_some_and(|(x, y)| exit_at(x, y)) {
			return (b & !CROSS) | TRIANGLE;
		}
		// [b48 Y4] The Ship Shack's browse: a click counts on a strip cell or the Buy plate only; elsewhere it is nothing.
		if shack && !prompt && b & CROSS != 0 && mouse && !self.last.is_some_and(|(x, y)| strip_offset(x, y).is_some() || shack_buy(x, y)) {
			return b & !CROSS;
		}
		// On the "Purchase?" prompt a mouse click buys only on the Yes plate (the No plate = Exit's place is Triangle above).
		if prompt && b & CROSS != 0 && mouse && !self.last.is_some_and(|(x, y)| yes_at(x, y)) {
			return b & !CROSS;
		}
		// [b48 Y4] Retail's buy latch trails the cursor by ~20 fields: a mouse Cross before it rested is held back and given once it has.
		if shack && !prompt && !self.pulse.busy() {
			if mouse && b & CROSS != 0 && self.still < SETTLE {
				self.hold = true;
				b &= !CROSS;
			}
			// [b48 R1] Given once the cursor has rested, whether or not the button is still down (a short click ends before that).
			if self.hold && self.still >= SETTLE {
				(self.hold, self.fire) = (false, 2);
			}
		}
		if prompt {
			self.hold = false;
		}
		if self.fire > 0 {
			self.fire -= 1;
			b |= CROSS;
		}
		if self.pulse.busy() && b & CROSS != 0 && !mouse {
			// [b45 P3] B27: a pad / keyboard Cross buys the item retail shows selected: the hover walk ends, the Cross goes through.
			self.pulse = Pulse::default();
			return b;
		}
		if self.pulse.busy() {
			if b & CROSS != 0 && self.pulse.target().is_some() {
				b &= !CROSS;
				self.pulse.ask(Ask::Press(v::CROSS_B)); // a click during the walk buys on arrival at the hovered row, never on the way
			}
			b |= self.pulse.step_gated(cur, n, !shack || self.still >= SETTLE);
			if std::env::var_os("RAC2_HOVER_TRACE").is_some() && b & PAD_LR != 0 {
				eprintln!("pc_vendor_hover: field {field} row {} -> {:?} pad {:#x}", cur, self.pulse.target(), b);
			}
		}
		b
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn strip_and_plates() {
		assert_eq!(strip_offset(0.5, 0.85), Some(0));
		assert_eq!(strip_offset(0.5 + PITCH, 0.85), Some(1));
		assert_eq!(strip_offset(0.5 - 3.0 * PITCH, 0.85), Some(-3));
		assert_eq!(strip_offset(0.5 + 4.0 * PITCH, 0.85), None, "beyond the visible cells");
		assert_eq!(strip_offset(0.5, 0.5), None, "not on the strip");
		assert!(on_exit(0.85, 0.41));
		assert!(!on_exit(0.15, 0.41), "the Buy plate");
		assert!(on_yes(0.15, 0.41) && !on_yes(0.85, 0.41));
		// [b48 Y4] the Ship Shack's plates (px 890,370 = Buy / Yes, 890,402 = Exit / No)
		assert!(shack_buy(890.0 / 1024.0, 370.0 / 768.0) && !shack_exit(890.0 / 1024.0, 370.0 / 768.0));
		assert!(shack_exit(890.0 / 1024.0, 402.0 / 768.0) && !shack_buy(890.0 / 1024.0, 402.0 / 768.0));
		assert!(!shack_buy(500.0 / 1024.0, 370.0 / 768.0), "the picture");
	}
}
