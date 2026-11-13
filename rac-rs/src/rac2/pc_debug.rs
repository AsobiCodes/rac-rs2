//! [PC] Debug keys for playtests (parity row 71, R&C1 `retail/debugkeys.rs`), keyboard only, inert unless Debug mode is on
//! (`options::debug_mode`: `debug=on`, RAC_DEV_TOOLS=1 or RAC_DEBUG_KEYS=1). Off, nothing is written (retail bit-exact).
//! R&C1's keys where R&C2 leaves them free: F6 infinite ammo and F8 +10000 bolts as R&C1; R&C1's F5 immortality is on F7 here
//! (R&C2's F5 / F9 are quick save / load; R&C2 has no F7 modern-camera key):
//! - F7 immortality toggle: Ratchet's health (0x18c36c) is set to his max (nanotech capacity 0x1a7a88) before every field while
//!   he is alive (health 0 = dead: left alone, the death plays as retail).
//! - F6 infinite ammo toggle (as R&C1's: every weapon full): each owned weapon's ammo word (0x139708 + 4 * slot) is set to its
//!   max before every field. The max is retail's own clamp [C: the vendor's ADD_AMMO, modern_vendor_hand.rs]: u16 at +0x8e of the
//!   item record table + map[slot] * 0xe0 (map bytes 0x1395e8; the table found by content, `modern_grid::find_items`, once per
//!   level); owned = byte 0x1a7b80 + slot; slots with max 0 (gadgets, no ammo) are left alone. Without the table (not found in
//!   this level's RAM) it falls back to "never less than you had" (each word put back to the highest value seen since the toggle)
//!   and the note says so.
//! - F8 +10000 bolts (0x1a7a80, once per press; capped at 9999999 [I]: R&C1's HUD limit; R&C2's counter showed 6 digits).
//! All addresses [C] rac2/notes_gamefacts.md (fixed boot data in every level). The window thread records the presses; the game
//! thread applies them between two fields (`pre_field`). The title shows the state ("GOD ON", "AMMO ON") and the last event for
//! 3 s. Test: RAC2_DEBUGKEYS_FEED=`FIELD:LETTERS;...` presses at that game field (g = F7, a = F6, b = F8; also with Debug mode off
//! is ignored) plus `hN` (health = N after that field's top-up) for a proof run; each top-up is logged.
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const HEALTH: u32 = 0x18c36c;
const MAX_HEALTH: u32 = 0x1a7a88;
const BOLTS: u32 = 0x1a7a80;
const AMMO: u32 = 0x139708;
const SLOTS: usize = 61;
const BOLT_ADD: i32 = 10_000;
const BOLT_CAP: i32 = 9_999_999;
const NOTE_FOR: Duration = Duration::from_secs(3);

const P_GOD: u32 = 1;
const P_AMMO: u32 = 2;
const P_BOLTS: u32 = 4;
static PENDING: AtomicU32 = AtomicU32::new(0);

struct St {
	god: bool,
	ammo: Option<[u32; SLOTS]>,
	note: Option<(String, Instant)>,
	feed: Option<Vec<(u64, String)>>,
	/// The level word the ammo tops belong to (a level change / load re-takes them).
	lvl: u32,
	/// The item record table (max ammo) of this level, and fields until the next search after a failed one.
	items: Option<u32>,
	retry: u32,
}

static ST: Mutex<St> = Mutex::new(St { god: false, ammo: None, note: None, feed: None, lvl: 0, items: None, retry: 0 });

/// Window: a key press; true = taken (Debug mode on and one of F3 / F6 / F7 / F8: F3 draw distance A/B (pc_devtools.rs), F6 ammo,
/// F7 god, F8 bolts). F4 (planet list) is pc_devtools.rs `list_key`, ahead of every handler.
pub fn key(k: winit::keyboard::KeyCode) -> bool {
	use winit::keyboard::KeyCode as K;
	if !crate::options::debug_mode() {
		return false;
	}
	let p = match k {
		K::F6 => P_AMMO,
		K::F7 => P_GOD,
		K::F8 => P_BOLTS,
		K::F3 => {
			crate::pc_devtools::far_toggle();
			return true;
		}
		_ => return false,
	};
	PENDING.fetch_or(p, Ordering::Relaxed);
	true
}

/// Window title: the state and the newest event (None = nothing to show).
pub fn note_now() -> Option<String> {
	let s = ST.lock().ok()?;
	let mut parts: Vec<String> = Vec::new();
	if s.god {
		parts.push("GOD ON".into());
	}
	if s.ammo.is_some() {
		parts.push("AMMO ON".into());
	}
	if let Some((n, t)) = &s.note {
		if t.elapsed() < NOTE_FOR {
			parts.push(n.clone());
		}
	}
	(!parts.is_empty()).then(|| parts.join(" | "))
}

fn feed_at(s: &mut St, field: u64) -> (u32, Option<u32>) {
	let feed = s.feed.get_or_insert_with(|| {
		std::env::var("RAC2_DEBUGKEYS_FEED").map(|v| v.split(';').filter_map(|e| e.split_once(':').and_then(|(f, l)| Some((f.trim().parse().ok()?, l.trim().to_string())))).collect()).unwrap_or_default()
	});
	let (mut p, mut h) = (0, None);
	for (_, letters) in feed.iter().filter(|e| e.0 == field) {
		p |= letters.chars().map(|c| match c {
			'g' => P_GOD,
			'a' => P_AMMO,
			'b' => P_BOLTS,
			_ => 0,
		}).fold(0, |a, b| a | b);
		h = letters.split('h').nth(1).and_then(|n| n.trim().parse().ok()).or(h);
	}
	(p, h)
}

/// Game thread, before `Machine::frame`: the presses since the last field, then the top-ups.
pub fn pre_field(m: &mut ee_oracle::Machine, field: u64) {
	// [PC] A replay with pc.bin: the writes the keys made in the session, not the window's keys (pc_rec.rs).
	if crate::pc_rec::replaying() {
		crate::pc_rec::apply_mem(crate::pc_rec::Stage::Debug, &mut m.cpu.mem);
		return;
	}
	if !crate::options::debug_mode() {
		return;
	}
	let Ok(mut s) = ST.lock() else { return };
	let (fed, poke) = feed_at(&mut s, field);
	let p = PENDING.swap(0, Ordering::Relaxed) | fed;
	let log = s.feed.as_ref().is_some_and(|f| !f.is_empty());
	if p & P_GOD != 0 {
		s.god = !s.god;
		s.note = Some((format!("GOD {}", if s.god { "ON" } else { "OFF" }), Instant::now()));
	}
	if p & P_AMMO != 0 {
		s.ammo = match s.ammo {
			Some(_) => None,
			None => Some(std::array::from_fn(|i| m.rd32(AMMO + 4 * i as u32))),
		};
		s.retry = 0;
		let full = s.ammo.is_some() && find_table(&m.cpu.mem.ram, &mut s);
		s.note = Some((match (s.ammo.is_some(), full) {
			(false, _) => "AMMO OFF".into(),
			(true, true) => "AMMO ON (FULL)".into(),
			(true, false) => "AMMO ON (NEVER LESS: MAX TABLE NOT FOUND)".into(),
		}, Instant::now()));
	}
	if p & P_BOLTS != 0 {
		let b = (m.rd32(BOLTS) as i32).saturating_add(BOLT_ADD).min(BOLT_CAP);
		wr(m, BOLTS, b as u32);
		s.note = Some(("+10000 BOLTS".into(), Instant::now()));
		eprintln!("debug keys: field {field}: bolts {b}");
	}
	let lvl = m.rd32(crate::pc_warp::LEVEL);
	crate::pc_devtools::set_level(lvl);
	warp(m, &mut s, field);
	if s.god {
		let (h, max) = (m.rd32(HEALTH) as i32, m.rd32(MAX_HEALTH) as i32);
		if h > 0 && h < max {
			wr(m, HEALTH, max as u32);
			if log {
				eprintln!("debug keys: field {field}: health {h} -> {max}");
			}
		}
	}
	if lvl != std::mem::replace(&mut s.lvl, lvl) && s.ammo.is_some() {
		// A level change or a load: the old maxima would refill the new save's ammo.
		s.ammo = Some(std::array::from_fn(|i| m.rd32(AMMO + 4 * i as u32)));
	}
	ammo_top(m, &mut s, field, log);
	if let Some(h) = poke {
		wr(m, HEALTH, h);
		eprintln!("debug keys: field {field}: test poke health {h}");
	}
}

/// Every game-RAM write of the Debug tools goes through here (game thread, inside `pre_field`), noted in a recorded session so a
/// replay makes the same writes (pc_rec.rs).
fn wr(m: &mut ee_oracle::Machine, a: u32, v: u32) {
	m.cpu.mem.wr32(a, v);
	crate::pc_rec::note_write(crate::pc_rec::Stage::Debug, a, &v.to_le_bytes());
}

/// The item record table is still where it was, else searched again (at most every 60 fields while missing). True = known.
fn find_table(ram: &[u8], s: &mut St) -> bool {
	if s.items.is_some_and(|a| !crate::modern_grid::item_tab_ok(ram, a)) {
		s.items = None;
	}
	if s.items.is_none() {
		if s.retry == 0 {
			s.items = crate::modern_grid::find_items(ram);
			s.retry = 60;
		} else {
			s.retry -= 1;
		}
	}
	s.items.is_some()
}

fn rd16(ram: &[u8], a: u32) -> u32 {
	ram.get(a as usize..a as usize + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]) as u32)
}

/// Slot `i`'s max ammo: Some(max) for an owned weapon with ammo, None for anything else (not owned, a gadget: max 0, or the record
/// of another owned slot's upgrade: fg1 0355 owns slot 60 and maps slot 30 to record 60; the ammo stays in slot 30's word and
/// slot 60's stays 0 [C: fg1 0355 / 0357 RAM]).
fn slot_max(ram: &[u8], items: u32, i: u32) -> Option<u32> {
	use crate::modern_grid::OWNED;
	use crate::modern_vendor::SLOT_MAP;
	let byte = |a: u32| ram.get(a as usize).copied().unwrap_or(0) as u32;
	let map = byte(SLOT_MAP + i);
	let max = rd16(ram, items + if map == 0 { i } else { map } * crate::modern_vendor::REC_SIZE + 0x8e);
	let upgrade_of_another = (0..SLOTS as u32).any(|k| k != i && byte(OWNED + k) != 0 && byte(SLOT_MAP + k) == i);
	(byte(OWNED + i) != 0 && max > 0 && !upgrade_of_another).then_some(max)
}

/// The ammo word to write for one slot (None = leave it): up to `max` (the table's), else back to the highest seen (`top`).
fn ammo_fix(now: u32, top: &mut u32, max: Option<u32>) -> Option<u32> {
	if let Some(m) = max {
		return (now < m).then_some(m);
	}
	if now > *top {
		*top = now;
	}
	(now < *top).then_some(*top)
}

/// F6 while on: every owned weapon to its max (the table), or never less than seen (no table).
fn ammo_top(m: &mut ee_oracle::Machine, s: &mut St, field: u64, log: bool) {
	if s.ammo.is_none() {
		return;
	}
	let table = if find_table(&m.cpu.mem.ram, s) { s.items } else { None };
	let Some(top) = s.ammo.as_mut() else { return };
	for (i, t) in top.iter_mut().enumerate() {
		let a = AMMO + 4 * i as u32;
		let max = match table {
			Some(tab) => match slot_max(&m.cpu.mem.ram, tab, i as u32) {
				Some(mx) => Some(mx),
				None => continue,
			},
			None => None,
		};
		let now = m.rd32(a);
		if let Some(v) = ammo_fix(now, t, max) {
			wr(m, a, v);
			if log {
				eprintln!("debug keys: field {field}: ammo slot {i} {now} -> {v}{}", if max.is_some() { " (max)" } else { "" });
			}
		}
	}
}

/// The planet list's warp (pc_devtools.rs), if the window asked for one and the game is in gameplay: retail's own level request
/// (pc_warp.rs: the request function's target and flag words); picking the level the game is on restarts it.
fn warp(m: &mut ee_oracle::Machine, s: &mut St, field: u64) {
	let Some(to) = crate::pc_devtools::take_warp() else { return };
	match crate::pc_warp::find(&m.cpu.mem.ram) {
		Some((flag, target)) => {
			wr(m, target, to);
			wr(m, flag, 1);
			s.note = Some((format!("WARP LEVEL {to}"), Instant::now()));
			eprintln!("debug keys: field {field}: warp to level {to} (flag {flag:#x}, target {target:#x})");
		}
		None => {
			crate::pc_devtools::warp_failed();
			eprintln!("debug keys: field {field}: warp request code not found");
		}
	}
}

/// Debug mode was switched Off (options.rs): god, ammo, the pending presses and the last event go, as R&C1's `menu_off`.
pub fn menu_off() {
	PENDING.store(0, Ordering::Relaxed);
	let Ok(mut s) = ST.lock() else { return };
	s.god = false;
	s.ammo = None;
	s.note = None;
}

#[cfg(test)]
mod tests {
	#[test]
	fn ammo_goes_to_the_max_or_never_below_the_highest_seen() {
		use super::ammo_fix;
		let mut top = 5;
		assert_eq!(ammo_fix(3, &mut top, Some(8)), Some(8), "a weapon with a max is filled");
		assert_eq!(ammo_fix(8, &mut top, Some(8)), None, "full: nothing written");
		assert_eq!(ammo_fix(3, &mut top, None), Some(5), "no table: back to the highest seen");
		assert_eq!(ammo_fix(9, &mut top, None), None);
		assert_eq!((top, ammo_fix(7, &mut top, None)), (9, Some(9)));
	}

	#[test]
	fn slot_max_reads_the_mapped_record_of_an_owned_weapon() {
		use crate::modern_vendor::{REC_SIZE, SLOT_MAP};
		let mut ram = vec![0u8; 0x40_0000];
		let items = 0x26_c870u32;
		// Slot 26 maps to record 68 (max 40), owned; slot 23 owned, record 23 max 8; slot 58 owned, max 0 (no ammo); slot 12 not owned.
		ram[(SLOT_MAP + 26) as usize] = 68;
		ram[(SLOT_MAP + 23) as usize] = 23;
		for (slot, rec, max, owned) in [(26u32, 68u32, 40u16, 1u8), (23, 23, 8, 1), (58, 58, 0, 1), (12, 12, 40, 0)] {
			ram[(crate::modern_grid::OWNED + slot) as usize] = owned;
			ram[(items + rec * REC_SIZE + 0x8e) as usize..][..2].copy_from_slice(&max.to_le_bytes());
		}
		assert_eq!([26, 23, 58, 12].map(|i| super::slot_max(&ram, items, i)), [Some(40), Some(8), None, None]);
		// Slot 30 owned, upgraded to record 60 (max 300); slot 60 owned too (map 60): only slot 30's word is the weapon's ammo.
		for slot in [30u32, 60] {
			ram[(crate::modern_grid::OWNED + slot) as usize] = 1;
			ram[(SLOT_MAP + slot) as usize] = 60;
		}
		ram[(items + 60 * REC_SIZE + 0x8e) as usize..][..2].copy_from_slice(&300u16.to_le_bytes());
		assert_eq!([30, 60].map(|i| super::slot_max(&ram, items, i)), [Some(300), None]);
	}

	#[test]
	fn the_feed_parses_presses_and_pokes() {
		let mut s = super::St { god: false, ammo: None, note: None, lvl: 0, items: None, retry: 0, feed: Some(vec![(5, "gab".into()), (9, "h2".into())]) };
		assert_eq!(super::feed_at(&mut s, 5), (7, None));
		assert_eq!(super::feed_at(&mut s, 9), (0, Some(2)));
		assert_eq!(super::feed_at(&mut s, 6), (0, None));
	}
}
