//! The follow camera's hidden helper moby ("marker", lane scratch_rc2_bounce, 2026-10-31): hidden from the Modern camera's line tests.
//!
//! [C] While the current camera record's class is 0 (follow), retail keeps a helper moby of class 0x3ef (1007) at the camera eye:
//! the select function's sync step (Aranos 0x2d84a8, twin of R&C1's `cam_helper_sync` FUN_001ddcd0) spawns it with
//! `0x385360(0x1b8e00)` (0x311fb0(0x3ef)) and keeps its slot in the camera block (Aranos 0x1b8f24 = basis - 0x465c, 26 of 26 level
//! dumps: the slot holds a class 0x3ef moby, `tools\survey_marker.py`). The moby has a collision pointer (`+0x98`, R&C2's shifted twin
//! of R&C1's `+0x94`), so the segment test (Aranos 0x2dec98) counts it as a wall: its moby loop reads `lw s4, 0x98(t8)` and
//! skips the moby when that is 0 (listing: `runs\segdis.txt` 0x2df4f8..0x2df550, `beqz s4`).
//!
//! The Modern boom is a line from the pivot toward the wanted eye. The marker sits on the eye the camera has NOW, which is short of
//! the wanted eye whenever a wall pulled the boom in: the line meets the marker first, the boom snaps in to it, the marker follows the
//! eye, the boom releases into it again: the camera bounces in and out (R&C1 had the same, decomp/notes/camera.md "Follow-camera
//! marker moby and line tests": `modcam.rs seg_raw` clears `+0x94` for the call and puts it back; retail's own tests pass the marker
//! as the ignored moby). The distance rule in `modcam_probe` (a moby within 1 m of the line's END) only catches the marker when the
//! boom is at full length.
//!
//! `find` picks the markers out of the moby list, `hide` zeroes their collision pointers for ONE segment test, `show` writes them
//! back right after (nothing else runs in between), so the game cannot tell.

/// Moby class id (u16 at `CLASS_AT`) of the camera helper.
pub const CLASS: u16 = 0x3ef;
pub const CLASS_AT: usize = 0xaa;
/// The collision pointer the segment test reads; 0 = not tested.
pub const COLLISION: usize = 0x98;

/// `RAC2_CAM_MARKER=0` [once]: leaves the helper in the line tests (the old behaviour; A/B tests of the bounce). Default on.
pub fn enabled() -> bool {
	static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
	*ON.get_or_init(|| std::env::var("RAC2_CAM_MARKER").map_or(true, |v| v != "0"))
}

/// A hidden-able marker: the moby (RAM offset) and its collision pointer as the game has it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Held {
	pub moby: u32,
	pub col: u32,
}

fn rd32(ram: &[u8], a: usize) -> Option<u32> {
	ram.get(a..a.checked_add(4)?).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// The mobys of class `CLASS` that have a collision pointer (the live camera marker; a second class-0x3ef moby of the dumps has none and
/// is not tested anyway).
pub fn find(ram: &[u8], mobys: &[u32]) -> Vec<Held> {
	mobys
		.iter()
		.filter_map(|&m| {
			let at = m as usize;
			let class = ram.get(at + CLASS_AT..at + CLASS_AT + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))?;
			let col = rd32(ram, at + COLLISION)?;
			(class == CLASS && col != 0).then_some(Held { moby: m, col })
		})
		.collect()
}

/// Zeroes the collision pointers (the segment test then skips these mobys).
pub fn hide(ram: &mut [u8], held: &[Held]) {
	for h in held {
		let at = h.moby as usize + COLLISION;
		if let Some(d) = ram.get_mut(at..at + 4) {
			d.copy_from_slice(&0u32.to_le_bytes());
		}
	}
}

/// Puts the collision pointers back.
pub fn show(ram: &mut [u8], held: &[Held]) {
	for h in held {
		let at = h.moby as usize + COLLISION;
		if let Some(d) = ram.get_mut(at..at + 4) {
			d.copy_from_slice(&h.col.to_le_bytes());
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn ram_with(mobys: &[(u32, u16, u32)]) -> Vec<u8> {
		let mut ram = vec![0u8; 0x4000];
		for &(m, class, col) in mobys {
			ram[m as usize + CLASS_AT..m as usize + CLASS_AT + 2].copy_from_slice(&class.to_le_bytes());
			ram[m as usize + COLLISION..m as usize + COLLISION + 4].copy_from_slice(&col.to_le_bytes());
		}
		ram
	}

	#[test]
	fn only_a_helper_with_a_collision_pointer_is_found() {
		let ram = ram_with(&[(0x100, 0, 0x55), (0x200, CLASS, 0x0123_4560), (0x300, CLASS, 0), (0x400, 7, 0)]);
		assert_eq!(find(&ram, &[0x100, 0x200, 0x300, 0x400]), vec![Held { moby: 0x200, col: 0x0123_4560 }]);
		assert!(find(&ram, &[0x1_0000_0000u64 as u32 + 0x3ffe]).is_empty(), "a record at the end of RAM is no marker");
	}

	#[test]
	fn hide_zeroes_the_pointer_and_show_gives_back_exactly_what_was_there() {
		let mut ram = ram_with(&[(0x100, 0, 0x55), (0x200, CLASS, 0x0123_4560)]);
		let before = ram.clone();
		let held = find(&ram, &[0x100, 0x200]);
		hide(&mut ram, &held);
		assert_eq!(rd32(&ram, 0x200 + COLLISION), Some(0), "the segment test skips a moby whose +0x98 is 0");
		assert_eq!(rd32(&ram, 0x100 + COLLISION), Some(0x55), "other mobys are untouched");
		assert_ne!(ram, before);
		show(&mut ram, &held);
		assert_eq!(ram, before);
	}

	#[test]
	fn a_real_level_dump_has_one_marker_with_a_collision_pointer() {
		// Aranos at field 2000 (the research dump, when present; moby table 0x1964480..0x199a680): the marker the slot basis - 0x465c points to.
		let Ok(ram) = std::fs::read("E:/ratchet/rac2/runs/ram2000.bin") else {
			eprintln!("research dump E:/ratchet/rac2/runs/ram2000.bin not present: skipped");
			return;
		};
		let mobys: Vec<u32> = (0x196_4480u32..0x199_a680).step_by(0x100).collect();
		let held = find(&ram, &mobys);
		assert_eq!(held.len(), 1, "{held:x?}");
		let slot = rd32(&ram, (crate::addrs::CAM_BASIS - 0x465c) as usize).map(|w| w & 0x01ff_ffff);
		assert_eq!(slot, Some(held[0].moby), "the camera block's slot names it");
	}
}
