//! [PC] [b46 W6] C16 rest: the button glyphs of the prompts follow a controller remap (Controls tab, Controller view, `gamepad=`).
//! A retail prompt holds one glyph byte per button (0x10 Cross .. 0x17 R2, glyph_text.rs); with a remap the byte is exchanged for the
//! glyph of the physical button that now does that action (same length, so retail's layout is untouched). Only prompts shown in play
//! follow (menus read the default layout: pad_binds::game_binds(true)), see pc_prompts.rs `answer` / `restyle`.
use rac_engine::input::glyph_text as gt;
use rac_engine::input::kbm::{self, Binds, Output};

const N: usize = (gt::R2 - gt::CROSS) as usize + 1;

/// The glyph byte a physical controller button (`PHYS` index) wears: the one of the action it drives in the default layout.
fn glyph_of_phys(t: &kbm::Table, defaults: &Binds, p: u8) -> Option<u8> {
	let a = (0..t.actions.len()).find(|&a| t.actions[a].pad_ok && defaults.1[a].contains(&Some(p)))?;
	let Output::Pad(bit) = t.actions[a].output else { return None };
	(gt::CROSS..=gt::R2).find(|&c| gt::glyph_bit(c) == Some(bit))
}

/// glyph byte -> glyph byte under `binds` (identity for a glyph whose action is unbound or whose button wears no glyph).
pub fn table_for(t: &kbm::Table, binds: &Binds) -> [u8; N] {
	let defaults = Binds::defaults(t);
	let mut out = [0u8; N];
	for (i, o) in out.iter_mut().enumerate() {
		let c = gt::CROSS + i as u8;
		*o = (|| {
			let a = t.game_action(Output::Pad(gt::glyph_bit(c)?))?;
			let p = binds.1[a][0].or(binds.1[a][1])?;
			glyph_of_phys(t, &defaults, p)
		})()
		.unwrap_or(c);
	}
	out
}

/// The live table (the player's controller layout).
pub fn table() -> [u8; N] {
	table_for(kbm::table(), &kbm::binds())
}

/// True when no glyph changes.
pub fn is_identity(t: &[u8; N]) -> bool {
	t.iter().enumerate().all(|(i, &c)| c == gt::CROSS + i as u8)
}

/// A 24-bit stamp of the live table: 0 = the standard layout (the pad keeps retail's texts), else non-zero (never 0).
pub fn stamp() -> u32 {
	stamp_of(&table())
}

/// `stamp` of a table.
fn stamp_of(t: &[u8; N]) -> u32 {
	if is_identity(t) {
		return 0;
	}
	let h = t.iter().fold(0x811c_9dc5u32, |h, &b| (h ^ b as u32).wrapping_mul(0x0100_0193));
	(h & 0x00ff_ffff) | 1
}

/// `s` with its glyph bytes exchanged under `t`; None when nothing changes.
pub fn remap(s: &[u8], t: &[u8; N]) -> Option<Vec<u8>> {
	let o: Vec<u8> = s.iter().map(|&b| if (gt::CROSS..=gt::R2).contains(&b) { t[(b - gt::CROSS) as usize] } else { b }).collect();
	(o != s).then_some(o)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn default_layout_changes_nothing_and_a_remap_swaps_the_glyph() {
		kbm::ensure_table(&crate::kbm_table::TABLE);
		// A local layout, not the live one: the other tests change the live binds while this one runs.
		let kt = kbm::table();
		let mut b = Binds::defaults(kt);
		let t = table_for(kt, &b);
		assert!(is_identity(&t));
		assert_eq!(stamp_of(&t), 0);
		let a = kt.index("jump").unwrap();
		(b.1[a][0], b.1[a][1]) = (Some(3), None); // Jump (Cross) now on Y, which wears the Triangle glyph
		let t = table_for(kt, &b);
		assert_eq!(t[(gt::CROSS - gt::CROSS) as usize], gt::TRIANGLE);
		assert_ne!(stamp_of(&t), 0);
		assert_eq!(remap(&[b'A', gt::CROSS, b'B'], &t), Some(vec![b'A', gt::TRIANGLE, b'B']));
		assert_eq!(remap(b"none", &t), None);
	}
}
