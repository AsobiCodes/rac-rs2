//! [PC] What the retail WEAPONS page draws besides icon, name and ammo (lane pages, continuation D). Every rule below is read from
//! the retail draw code (boot ELF, Ghidra C of R&C2: `FUN_00349ab8` = the cell grid draw, `FUN_00338c78` = tint and bar colours,
//! `FUN_00349600` = the weapon MOD row, `FUN_00348df8` = the page's update), not from RAM diffs:
//! - Cell icon = u16 at record(slot) + 0x3c (the MAPPED record, so an upgraded weapon shows its own picture); 0xea7e = no tint/bar.
//! - Icon tint = colour table 0x1adb78 indexed by the record's tier byte (+4): cyan, ORANGE, yellow, blue (PS2 RGBA words
//!   0x60f0f0b0, 0x6029a1ff, 0x6040d7d7, 0x60f08040; the special weapons 0xc 0xe 0x11 0x12 0x35 use table 0x1adb88: cyan, yellow).
//!   Item 0x10 with tier > 1 uses the table at tier - 2. 0x80 = 1.0, so a channel of 0xf0 is the full colour here.
//! - Progress bar under the icon = (u32 0x1398e8 + 4 * item) >> 5 over the record's +0x6c, drawn only when `FUN_00338c78` sets
//!   colours: tier 0 and +0x6c > 0 (gold to red), or (flag byte 0x1a7a92 set, upgrade chain deep enough) tier 2 / special tier 1
//!   (magenta). The chain depth is `FUN_00288878`: follow +0x4c to the first record, count +0x4a steps.
//! - Out of ammo (held 0, record +0x88 != 0): the cell background turns dark red (0x60202080).
//! - MOD row (`FUN_00349600`): byte 0x1a7b08 + item: bit 0 = Shock Mod (text 0x10e7, description 0xef4, icon sheet 0xea98 frame 0),
//!   else bit 2 = Acid Mod (0x10e6 / 0xef3, frame 1); bit 0x10 = Lock On Mod (0x10e8 / 0xef5, frame 2) as a second row.
use crate::modern_grid::{rd, rd16, rd8, REC_SIZE};
use crate::modern_screen::Texts;

pub const MOD_FLAGS: u32 = 0x1a7b08;
pub const XP: u32 = 0x1398e8;
pub const NG_FLAG: u32 = 0x1a7a92;
/// Icon id the retail draw treats as "plain" (no tint, no bar).
pub const PLAIN_ICON: u32 = 0xea7e;
const TINTS: [u32; 4] = [0x60f0f0b0, 0x6029a1ff, 0x6040d7d7, 0x60f08040];
const TINTS_SPECIAL: [u32; 4] = [0x60f0f0b0, 0x6040d7d7, 0, 0];

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Mod {
	/// Icon frame in sheet 0xea98: 0 Shock, 1 Acid, 2 Lock On.
	pub frame: usize,
	pub name: String,
	pub desc: String,
}

/// The progress bar under a weapon icon: fill in 1/1000 and which colour pair (false = gold to red, true = magenta).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Bar {
	pub permille: u32,
	pub magenta: bool,
}

/// What the weapons grid adds to a cell.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Extra {
	/// The record's tier byte.
	pub tier: u8,
	/// Retail tint RGB (channel / 0xf0 of the PS2 word, 0..255); None = the page's own colours.
	pub tint: Option<[u8; 3]>,
	pub bar: Option<Bar>,
	/// Held ammo 0 on a weapon that uses ammo.
	pub empty: bool,
	/// The icon id of the mapped record (+0x3c); 0 when unreadable.
	pub icon: u32,
	pub mods: Vec<Mod>,
}

pub fn special(item: u32) -> bool {
	matches!(item, 0xc | 0xe | 0x11 | 0x12 | 0x35)
}

fn word_rgb(w: u32) -> [u8; 3] {
	let ch = |s: u32| ((((w >> s) & 0xff) * 255) / 0xf0).min(255) as u8;
	[ch(0), ch(8), ch(16)]
}

/// The tint word of `item` for a record of `tier` (the retail table choice).
pub fn tint_word(item: u32, tier: u32, tier_of_16: u32) -> u32 {
	let t = |tab: &[u32; 4], i: u32| tab.get(i as usize).copied().unwrap_or(0);
	if item == 0x10 && tier_of_16 > 1 {
		return t(&TINTS, tier_of_16 - 2);
	}
	t(if special(item) { &TINTS_SPECIAL } else { &TINTS }, tier)
}

/// `FUN_00288878`: the number of upgrade steps after the chain's first record.
pub fn chain_depth(ram: &[u8], items: u32, item: u32) -> u32 {
	let rec = |i: u32| items + i * REC_SIZE;
	let mut p = item;
	for _ in 0..64 {
		match rd16(ram, rec(p) + 0x4c) {
			0 => break,
			n => p = n,
		}
	}
	let mut n = 0;
	for _ in 0..64 {
		match rd16(ram, rec(p) + 0x4a) {
			0 => break,
			q => {
				p = q;
				n += 1;
			}
		}
	}
	n
}

/// The bar colours rule of `FUN_00338c78` (None = the retail colours stay at their invisible default).
fn bar_kind(ram: &[u8], items: u32, item: u32, rec: u32, tier: u32) -> Option<bool> {
	if rd(ram, rec) == 0 || chain_depth(ram, items, item) < 1 {
		return None;
	}
	if rd8(ram, NG_FLAG) != 0 {
		let depth = chain_depth(ram, items, item);
		let deep = depth >= 2 || (depth == 1 && special(item));
		if deep {
			return (tier == 2 || (tier == 1 && special(item))).then_some(true);
		}
	}
	(tier == 0 && rd(ram, rec + 0x6c) as i32 >= 1).then_some(false)
}

/// The grid extras of weapon `item` (`rec` = the mapped record's address).
pub fn extra(ram: &[u8], items: u32, texts: &Texts, item: u32, rec: u32, owned: bool) -> Extra {
	let tier = rd8(ram, rec + 4);
	let slot16 = rd8(ram, crate::modern_vendor::SLOT_MAP + 0x10);
	let tier16 = rd8(ram, items + slot16 * REC_SIZE + 4);
	let word = tint_word(item, tier, tier16);
	let icon = rd16(ram, rec + 0x3c);
	let max = rd(ram, rec + 0x6c);
	let bar = bar_kind(ram, items, item, rec, tier).filter(|_| icon != PLAIN_ICON && max > 0).map(|magenta| Bar {
		permille: ((rd(ram, XP + 4 * item) >> 5).min(max) as u64 * 1000 / max as u64) as u32,
		magenta,
	});
	let empty = owned && rd16(ram, rec + 0x88) != 0 && rd(ram, crate::modern_vendor::AMMO + 4 * item) == 0;
	Extra { tier: tier as u8, tint: (word != 0).then(|| word_rgb(word)), bar, empty, icon, mods: if owned { mods(ram, texts, item) } else { Vec::new() } }
}

/// The mod rows of weapon `item` (`FUN_00349600`).
pub fn mods(ram: &[u8], texts: &Texts, item: u32) -> Vec<Mod> {
	let flags = rd8(ram, MOD_FLAGS + item);
	let mut out = Vec::new();
	let mut add = |frame: usize, name: u32, desc: u32| {
		out.push(Mod { frame, name: texts.get(ram, name).unwrap_or_default(), desc: crate::modern_items::long_text(ram, texts, desc) });
	};
	if flags & 1 != 0 {
		add(0, 0x10e7, 0xef4);
	} else if flags & 4 != 0 {
		add(1, 0x10e6, 0xef3);
	}
	if flags & 0x10 != 0 {
		add(2, 0x10e8, 0xef5);
	}
	out
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn tint_table_follows_the_tier_and_the_special_weapons() {
		assert_eq!(tint_word(30, 1, 0), 0x6029a1ff);
		assert_eq!(tint_word(0xc, 1, 0), 0x6040d7d7);
		assert_eq!(tint_word(0xc, 2, 0), 0);
		assert_eq!(tint_word(0x10, 3, 3), 0x6029a1ff);
		assert_eq!(word_rgb(0x6029a1ff), [255, (0xa1u32 * 255 / 0xf0) as u8, (0x29u32 * 255 / 0xf0) as u8]);
	}

	#[test]
	fn chain_depth_counts_upgrade_steps() {
		let mut ram = vec![0u8; 0x1000];
		let w = |ram: &mut Vec<u8>, a: u32, v: u16| ram[a as usize..a as usize + 2].copy_from_slice(&v.to_le_bytes());
		// records of 0xe0 bytes: 1 -> 2 -> 3 through +0x4a, back links through +0x4c.
		w(&mut ram, 0xe0 + 0x4a, 2);
		w(&mut ram, 2 * 0xe0 + 0x4a, 3);
		w(&mut ram, 2 * 0xe0 + 0x4c, 1);
		w(&mut ram, 3 * 0xe0 + 0x4c, 2);
		assert_eq!(chain_depth(&ram, 0, 1), 2);
		assert_eq!(chain_depth(&ram, 0, 3), 2);
		assert_eq!(chain_depth(&ram, 0, 3 + 0), 2);
	}
}
