//! When the game thread unwinds its recompiled frames (both host loops: the window's `host/game.rs` and the hidden `--shots` replay
//! `shots.rs`, which share this one cadence).
//!
//! Compiled guest code nests one Rust frame per retail call, and an interrupt serviced at a block entry runs another guest thread
//! nested inside it; some retail paths never return through those frames, so the oracle's 256 MB `ee` stack fills a little every
//! field and overflowed at field ~18527 of the user's Aranos recording (2026-10-31; [C] cycle of ~420 bytes x ~630K repeats on the
//! stack, same with the classic camera). Every `UNWIND_FIELDS` fields the host sets `Cpu::rec_unwind` while the game thread is parked
//! at the vblank wait; it unwinds its compiled frames there and goes on interpreting. RAM is unchanged (replay: identical RAM hashes
//! with and without, fields 18000..18500). R&C1 does the same: `rac1/retail/worker.rs` UNWIND_FIELDS; [b45 P1] the window loop now also waits for the game to be parked, as R&C1's worker does (`parked`, `step`).

/// Fields between two unwinds.
pub const UNWIND_FIELDS: u64 = 10;

/// Whether the host sets `rec_unwind` after the field that made `done` fields done (1 after the first field).
pub fn unwind_due(done: u64) -> bool {
	done != 0 && done % UNWIND_FIELDS == 0
}

/// A due unwind that has waited this many fields runs anyway: a last resort for a wait no `parked` test knows (R&C1's value).
const UNWIND_FORCE_FIELDS: u32 = 500;

/// [b45 P1] Whether the game thread waits for the vsync, so no PC hook is inside a `Cpu::call` (the Modern camera's probe calls
/// retail code from a hook; unwinding there threw away the hook's Rust frame and the MIPS return went to CALL_RET 0x10: R&C1's
/// stuck map, fixed 2026-11-04): at the vblank wait (`vblank_pc`), or in sceGsSyncV's INTC_STAT poll (`poll_head`, 7 instructions).
pub fn parked(pc: u32, vblank_pc: u32, poll_head: u32) -> bool {
	pc == vblank_pc || pc.wrapping_sub(poll_head) < 0x1c
}

/// The unwind after field number `field` (counted after the increment): `due` = fields a due unwind has waited (0 = none),
/// `parked` = the game thread is in a vsync wait. Returns the new `due` and whether to unwind now (R&C1: worker/unwind.rs `step`).
pub fn step(due: u32, field: u32, parked: bool) -> (u32, bool) {
	let due = if due > 0 {
		due.saturating_add(1)
	} else if field as u64 % UNWIND_FIELDS == 0 {
		1
	} else {
		0
	};
	if due > 0 && (parked || due > UNWIND_FORCE_FIELDS) {
		(0, true)
	} else {
		(due, false)
	}
}

/// `step` for the window loop, with a log line when an unwind had to be forced on a game thread that was not parked (the first 5:
/// "unwind: forced ..."; build 45 review F4: R&C2's `poll_head` is a sentinel, so `parked` is the vblank wait only, and a field loop
/// that ended elsewhere would unwind every 500 fields instead of every 10. No such line in a log = the cadence is the planned one).
pub fn step_logged(due: u32, field: u32, parked: bool, pc: u32) -> (u32, bool) {
	let (due, now) = step(due, field, parked);
	if now && !parked {
		static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
		if N.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < 5 {
			eprintln!("unwind: forced at field {field}, pc {pc:08x} (not parked for {UNWIND_FORCE_FIELDS} fields)");
		}
	}
	(due, now)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parked_waits_and_deferred_unwind() {
		let (vb, ph) = (0x1000, 0x2000);
		assert!(parked(vb, vb, ph) && parked(ph, vb, ph) && parked(ph + 0x18, vb, ph));
		assert!(!parked(ph + 0x1c, vb, ph) && !parked(ph - 4, vb, ph) && !parked(0x10, vb, ph));
		let n = UNWIND_FIELDS as u32;
		assert_eq!(step(0, n, false), (1, false), "due mid-computation: wait");
		assert_eq!(step(1, n + 1, true), (0, true), "the first parked field unwinds");
		assert_eq!(step(0, n + 1, true), (0, false), "not due");
		let (mut due, mut at) = (0, None);
		for f in n..n + 600 {
			let (d, u) = step(due, f, false);
			due = d;
			if u {
				at = Some(f - n);
				break;
			}
		}
		assert_eq!(at, Some(UNWIND_FORCE_FIELDS), "a frame that never parks is unwound after the cap");
	}

	#[test]
	fn unwinds_every_tenth_field() {
		assert!(!unwind_due(0));
		for done in 1..10 {
			assert!(!unwind_due(done), "after {done} fields");
		}
		assert!(unwind_due(10) && unwind_due(20) && unwind_due(18_530));
		assert!(!unwind_due(11) && !unwind_due(19) && !unwind_due(18_531));
		// 20000 fields (the proof replay) unwind 2000 times.
		assert_eq!((1..=20_000u64).filter(|&d| unwind_due(d)).count(), 2000);
	}
}
