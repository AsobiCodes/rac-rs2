//! [PC] Debug level travel (parity row 71, R&C1's F1 warp list; R&C2's F1 is the picture flip, so here F4 opens the planet list,
//! pc_devtools.rs). Debug mode only (pc_debug.rs gate); nothing is written until the list's Enter.
//!
//! How retail travels [C] (rac2/HANDOFF.md "Planet travel"): the level's own copy of the request function (ELF FUN_002895c8)
//! writes a target level word and a pending flag; the level main loop then exits and the loader loads the target. Writing the two
//! words is the same request (Aranos test: target then flag in one field -> level 1 loads with its arrival scene).
//! FIND RULE [C: one hit per level in the dumps of levels 0 (Aranos), 1, 6, 25 and three more bank states]: the request function
//! starts `li v0,1; addiu sp,-16; sw v0,F(gp); sd ra,0(sp); li v0,-1; bne a0,v0,..; sw a0,T(gp)`; flag = gp + F, target = gp + T
//! with gp = 0x1af070 (the ELF's own copy writes gp-0x643c = 0x1a8c34 and the Aranos copy gp-0x611c = 0x1a8f54: both give that gp).
//! The flag is the same word in every level seen (0x1a8f54; level 1 0x1a8f94: mode word - 0x2c), the target word moves per level.

/// The game's gp register value (see above) [C].
pub const GP: u32 = 0x1af070;
/// The current level word [C rac2/notes_vendor_map.md].
pub const LEVEL: u32 = 0x1a7a78;
/// Level files on the disc (0..=26) [C: rac2/notes_metal.md lists 27 levels].
pub const LEVELS: u32 = 27;

fn rd(ram: &[u8], a: usize) -> u32 {
	ram.get(a..a + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn gp_rel(w: u32) -> u32 {
	GP.wrapping_add((w & 0xffff) as u16 as i16 as i32 as u32)
}

/// The (pending flag, target level) words of the loaded level's travel request, found by the code shape above.
pub fn find(ram: &[u8]) -> Option<(u32, u32)> {
	let end = ram.len().min(0x100_0000).saturating_sub(28);
	(0x10_0000..end).step_by(4).find_map(|a| {
		let w = |k: usize| rd(ram, a + k);
		let ok = w(0) == 0x2402_0001
			&& w(4) == 0x27bd_fff0
			&& w(8) >> 16 == 0xaf82
			&& w(12) == 0xffbf_0000
			&& w(16) == 0x2402_ffff
			&& w(20) >> 16 == 0x1482
			&& w(24) >> 16 == 0xaf84;
		ok.then(|| (gp_rel(w(8)), gp_rel(w(24))))
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn finds_the_aranos_request() {
		// The Aranos copy's first 7 words (rac2/runs/s23/ram02540.bin at 0x2f5960).
		let code = [0x2402_0001u32, 0x27bd_fff0, 0xaf82_9ee4, 0xffbf_0000, 0x2402_ffff, 0x1482_000f, 0xaf84_2950];
		let mut ram = vec![0u8; 0x40_0000];
		for (i, c) in code.iter().enumerate() {
			ram[0x2f_5960 + 4 * i..][..4].copy_from_slice(&c.to_le_bytes());
		}
		assert_eq!(find(&ram), Some((0x1a_8f54, 0x1b_19c0)));
		assert_eq!(find(&vec![0u8; 0x40_0000]), None);
	}
}
