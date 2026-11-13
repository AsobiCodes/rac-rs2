//! Whom the Modern camera follows (lane scratch_rc2_ccam, 2026-11-12; user, build 43: "the camera doesnt follow clank").
//!
//! The hand took the hero as the first record of the moby table (Ratchet's, class 0). In R&C2's Clank sections Ratchet's record
//! stays parked where it was and the player drives Clank's own moby (class 0x57), so the Modern camera stared at a fixed point
//! [C: the user's quick save q60903 (level 14), RAM at fields 20/45/59/90: record 0 (class 0) at 328.30, 158.41, 110.00 the whole
//! walk, record 63 (class 0x57) 328.57, 159.70, 110.00 -> 329.15, 159.91, 109.93 -> 329.94, 160.19, 108.77 (into the lava); retail's
//! camera followed record 63].
//! Retail knows the controlled character by the hero moby pointer 0x18c130 (hero struct base 0x189ea0 + 0x2290, the word before the
//! hero state 0x18c134; the same address in every level) [C notes_gamefacts.md; R&C1's twin is HERO_MOBY 0x1413d0 before
//! HERO_STATE 0x1413d4]. The fg1 bank (581 states): class 0 (Ratchet) in 553, class 0x57 (Clank) in Endako 0144-0149, class 0x27d
//! (Giant Clank, Dobbo orbit, level 22) in 0374-0382, 0 (no hero: loading, ship menus) in 19 [C scan of 0x18c130 -> +0xaa].
//! R&C1 follows the hero struct position, which is Clank's in his sections, with the same pivot and boom, and hands Giant Clank back
//! to retail (`rac1/retail/modcam.rs` `why_retail`, BODY_FORM 2) [C]: the same here, by the class of the controlled moby. A zero or
//! implausible pointer keeps the old rule (the table's first record), so nothing changes where the pointer is not set.

/// The hero moby pointer (EE RAM, every level).
pub const HERO_MOBY: u32 = 0x18c130;
/// Moby record offsets: position (f32 x 3), slot byte (0 = the hero's / a live record), class id (u16).
const POS: u32 = 0x10;
const SLOT: u32 = 0x20;
const CLASS: u32 = 0xaa;
/// Class ids of the bodies the player drives on foot [C fg1 scan above].
pub const RATCHET: u16 = 0;
pub const CLANK: u16 = 0x57;

/// The body the player drives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Body {
	Ratchet,
	Clank,
	/// Anything else (Giant Clank 0x27d, a vehicle): retail's camera, as R&C1 does for Giant Clank.
	Other(u16),
}

impl Body {
	pub fn of(class: u16) -> Body {
		match class {
			RATCHET => Body::Ratchet,
			CLANK => Body::Clank,
			c => Body::Other(c),
		}
	}
	/// The Modern camera follows this body (Ratchet, Clank).
	pub fn on_foot(self) -> bool {
		!matches!(self, Body::Other(_))
	}
	/// The strafe aim (raised pivot) exists for Ratchet only (R&C1: the aim mode needs body form 0).
	pub fn aims(self) -> bool {
		self == Body::Ratchet
	}
}

/// The character the camera follows this field: its moby (RAM offset), body and position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Who {
	pub moby: u32,
	pub body: Body,
	pub pos: [f32; 3],
}

fn rd32(ram: &[u8], a: u32) -> Option<u32> {
	let a = a as usize;
	ram.get(a..a + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The record at `moby` as a followed character: in RAM, slot byte 0, a finite position (None otherwise).
pub fn at(ram: &[u8], moby: u32) -> Option<Who> {
	let m = moby & 0x01ff_ffff;
	if m == 0 || m as usize + 0x100 > ram.len() || ram[(m + SLOT) as usize] != 0 {
		return None;
	}
	let pos = [0, 4, 8].map(|k| f32::from_bits(rd32(ram, m + POS + k).unwrap_or(u32::MAX)));
	if !pos.iter().all(|x| x.is_finite()) {
		return None;
	}
	let class = u16::from_le_bytes([ram[(m + CLASS) as usize], ram[(m + CLASS + 1) as usize]]);
	Some(Who { moby: m, body: Body::of(class), pos })
}

/// The controlled character: the moby at `HERO_MOBY` when it is a plausible record, else the first record of the moby table whose
/// start word is at `table` (the rule before this lane; that record is Ratchet's).
pub fn find(ram: &[u8], table: u32) -> Option<Who> {
	rd32(ram, HERO_MOBY).and_then(|p| at(ram, p)).or_else(|| rd32(ram, table).and_then(|first| at(ram, first)).map(|w| Who { body: Body::Ratchet, ..w }))
}

/// The controlled moby changed since the last field (both known): the camera's target jumps from one character to the other.
pub fn switched(last: Option<u32>, now: u32) -> bool {
	last.is_some_and(|l| l != now)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn put(ram: &mut [u8], a: u32, v: u32) {
		ram[a as usize..a as usize + 4].copy_from_slice(&v.to_le_bytes());
	}

	/// A RAM with the moby table word at 0x1b251c -> Ratchet's record 0x1c2c680, Clank's record 0x1c30580 (level 14 layout [C]).
	fn ram() -> Vec<u8> {
		let mut r = vec![0u8; 32 << 20];
		put(&mut r, 0x1b251c, 0x1c2c680);
		for (m, p, class) in [(0x1c2c680u32, [328.30f32, 158.41, 110.00], 0u16), (0x1c30580, [328.57, 159.70, 110.00], 0x57)] {
			for (k, v) in p.iter().enumerate() {
				put(&mut r, m + POS + 4 * k as u32, v.to_bits());
			}
			r[(m + CLASS) as usize..(m + CLASS + 2) as usize].copy_from_slice(&class.to_le_bytes());
		}
		r
	}

	#[test]
	fn clank_is_followed_when_the_hero_pointer_names_him() {
		let mut r = ram();
		put(&mut r, HERO_MOBY, 0x1c30580);
		let w = find(&r, 0x1b251c).unwrap();
		assert_eq!((w.moby, w.body, w.pos), (0x1c30580, Body::Clank, [328.57, 159.70, 110.00]));
	}

	#[test]
	fn ratchet_through_the_pointer_is_the_first_record_as_before() {
		let mut r = ram();
		put(&mut r, HERO_MOBY, 0x1c2c680);
		let w = find(&r, 0x1b251c).unwrap();
		assert_eq!((w.moby, w.body), (0x1c2c680, Body::Ratchet));
	}

	#[test]
	fn no_pointer_or_a_bad_one_keeps_the_first_record() {
		let mut r = ram();
		for p in [0u32, 0x1fff_fff0, 0x1c2c780] {
			put(&mut r, HERO_MOBY, p);
			if p == 0x1c2c780 {
				r[(0x1c2c780 + SLOT) as usize] = 1; // an unused slot
			}
			assert_eq!(find(&r, 0x1b251c).map(|w| w.moby), Some(0x1c2c680), "pointer {p:#x}");
		}
	}

	#[test]
	fn giant_clank_and_vehicles_are_not_on_foot() {
		assert!(Body::of(0).on_foot() && Body::of(0x57).on_foot());
		assert_eq!(Body::of(0x27d), Body::Other(0x27d));
		assert!(!Body::of(0x27d).on_foot());
		assert!(Body::Ratchet.aims() && !Body::Clank.aims());
	}

	#[test]
	fn a_nan_position_is_no_target() {
		let mut r = ram();
		put(&mut r, HERO_MOBY, 0x1c30580);
		put(&mut r, 0x1c30580 + POS, f32::NAN.to_bits());
		assert_eq!(find(&r, 0x1b251c).map(|w| w.moby), Some(0x1c2c680), "a NaN Clank falls back to the table");
	}

	#[test]
	fn a_switch_needs_two_known_mobys() {
		assert!(!switched(None, 5));
		assert!(!switched(Some(5), 5));
		assert!(switched(Some(5), 6));
	}
}
