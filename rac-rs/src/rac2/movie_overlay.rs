//! The movie player copies that live in the level overlays (lane scratch_rc2_hang, 2026-10-31).
//!
//! Besides the boot copy (FUN_00351140, `movie.rs`) EVERY level overlay carries its own copy of the player head [C] (all 27 levels,
//! found in the disc images by `tools/cover_hands.py`): Aranos (level 0) at 0x44a310, levels 1, 7, 21, 22, 24, 26 at 0x44d928,
//! 0x445e70, 0x44e450, 0x44ad68, 0x428e50, 0x43c998, the other 20 above 0x450000 (level 2 at 0x476318, level 11 at 0x4ad1a8, ...).
//! The head is the same function (same signature: a0 lba, a1 bytes, a2/a3 -> 0x1b2748/0x1b274c decode buffers, t0 audio track, t1 flag; v0 = 1
//! when the pad skipped it); it calls an overlay init (0x44a7c0: libmpeg init, CreateThread of the MPEG decode thread 0x44ce38,
//! StartThread, AddIntcHandler) and an overlay loop (0x44a400) that waits for that thread. The oracle has no scheduler, so the thread
//! never ran and the end of Aranos waited forever on a black screen (the user's R&C2 session of 2026-10-31). The same hook as the
//! boot copy (`movie::movie_call`) now answers these copies: the window plays the PSS (the same stream the retail thread decodes)
//! and the game returns v0 = 0 (played) or 1 (skipped).
//!
//! The copies differ only in immediates and `jal` targets (the overlays sit at different addresses), so the hook is a relocatable
//! hand (`ee_oracle::rec::register_reloc_hand`): `Table::build` binds it to every recompiled function of the sets in use whose
//! 0xec code bytes equal the template with the masked fields removed. `SPEC` is written by `tools/mkspec.py` of the lane
//! (differing bits of the first 7 copies, widened to the immediate / target field); `tools/cover_hands.py` checks that it matches in
//! all 27 level images and that each match is a generated function with exactly that entry and length. A hand binds only to a
//! compiled function, so the R&C2 generator's function lists must cover the whole code overlay: they stopped at 0x450000 until
//! 2026-10-31 (`tools/mk_bounds_rc2_hi.py`), and 20 of the 27 copies were interpreted and never hooked.
use ee_oracle::hs_reloc::{Reloc, RelocHand};
use std::sync::OnceLock;

/// Length of the player head (59 instructions) in every copy.
pub const LEN: u32 = 0xec;

/// `hs_reloc` spec text: name, entry of the template copy (Aranos), length, masked-code hash, masked fields (byte offset, bits).
pub const SPEC: &str = "reloc movie_player_overlay entry 0x44a310 len 0xec
mhash 0x2e7a30947928c6f1
mask 0x8 0xffff
mask 0x3c 0xffff
mask 0x44 0xffff
mask 0x48 0x3ffffff
mask 0x58 0x3ffffff
mask 0x7c 0x3ffffff
mask 0x88 0xffff
mask 0xa4 0x3ffffff
mask 0xb0 0x3ffffff
mask 0xd8 0xffff
mask 0xe0 0xffff
";

pub fn reloc() -> &'static Reloc {
	static R: OnceLock<Reloc> = OnceLock::new();
	R.get_or_init(|| Reloc::parse(SPEC).expect("movie_overlay::SPEC parses"))
}

static HAND: RelocHand = RelocHand { name: "rc2_movie_overlay", reloc, est: 0, f: crate::movie::movie_call };

/// Registers the hook with the oracle's table (idempotent; called by `movie::register`).
pub fn register() {
	ee_oracle::rec::register_reloc_hand(&HAND);
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Entry of the template copy (Aranos, level 0).
	const BASE: u32 = 0x44a310;

	/// The 59 words of the player head in Aranos (level 0, 0x44a310): the template `SPEC` was written from. It is the function the
	/// boot ELF has at 0x351140 (`movie.rs`), linked at another address (other buffers, other callees).
	const TEMPLATE: [u32; 59] = [
		0x27bdffd0, 0x3c01001b, 0xac262748, 0xffb20010, 0xffb30018, 0x0080902d,
		0x00a0982d, 0xffb00000, 0xffb10008, 0x3c04001b, 0x3c050038, 0xffb40020,
		0x0100882d, 0x0120802d, 0xffbf0028, 0x2484eb58, 0x3c01001b, 0xac27274c,
		0x0c0a91d4, 0x34a5cac0, 0x0000a02d, 0x0200202d, 0x0c0bc31c, 0x0080282d,
		0x0c046ac4, 0x00000000, 0x0040202d, 0x0c046aac, 0x24050001, 0x0240202d,
		0x0260282d, 0x0c1129f0, 0x0220302d, 0x1040000a, 0x8f8436dc, 0x3c02000d,
		0x3c06000d, 0x34429048, 0x0080282d, 0x34c69040, 0x00822021, 0x0c112900,
		0x00a63021, 0x0040a02d, 0x0c112a92, 0x00000000, 0x0280102d, 0xdfb00000,
		0xdfb10008, 0xdfb20010, 0xdfb30018, 0xdfb40020, 0xdfbf0028, 0x3c01001b,
		0xac202748, 0x3c01001b, 0xac20274c, 0x03e00008, 0x27bd0030,
	];

	fn hash_of(w: &[u32], base: u32) -> u64 {
		reloc().masked_hash_by(base, |a| w[((a - base) / 4) as usize])
	}

	#[test]
	fn spec_parses() {
		let r = reloc();
		assert_eq!((r.entry, r.len, r.name.as_str()), (0x44a310, LEN, "movie_player_overlay"));
		assert_eq!(r.mhash, 0x2e7a_3094_7928_c6f1);
		assert_eq!(TEMPLATE.len() as u32 * 4, LEN);
	}

	/// The template hashes to the spec's masked hash. A copy in another overlay differs in exactly the masked fields (immediates
	/// and `jal` targets): giving every masked field other bits still matches. One changed unmasked word (a different function of
	/// the same size) or a changed opcode in a masked word does not.
	#[test]
	fn template_matches_and_only_masked_fields_may_differ() {
		let r = reloc();
		assert_eq!(hash_of(&TEMPLATE, BASE), r.mhash);
		let mut moved = TEMPLATE;
		for l in SPEC.lines().filter(|l| l.starts_with("mask ")) {
			let f: Vec<u32> = l.split_whitespace().skip(1).map(|x| u32::from_str_radix(x.trim_start_matches("0x"), 16).unwrap()).collect();
			moved[(f[0] / 4) as usize] ^= f[1] & 0x5555_5555;
		}
		assert_ne!(moved, TEMPLATE, "the masks changed something");
		assert_eq!(hash_of(&moved, BASE), r.mhash, "relocated fields are ignored");
		let mut x = TEMPLATE;
		x[10] ^= 0x0100_0000; // `lui a1, 0x38` becomes another register
		assert_ne!(hash_of(&x, BASE), r.mhash, "an unmasked word changed");
		let mut y = TEMPLATE;
		y[2] ^= 0x0400_0000; // the masked `sw a2, 0x2748(at)` becomes another opcode
		assert_ne!(hash_of(&y, BASE), r.mhash, "an opcode changed");
	}
}
