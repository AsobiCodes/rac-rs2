//! The right stick the game gets while the Modern camera is on (lane scratch_rc2_cam, review fix 3; R&C1: `rac1/retail/modcam/sticks.rs`).
//! Retail's follow camera reads the stick with retail's sign (its reader, Aranos 0x42a488: Y = -stick unless the Up/Down option word
//! is 0, X = +stick unless the Left/Right word is 0 [C]), so with the Modern camera (mouse sign: up looks up, right looks right) the
//! controller would turn the other way whenever retail's camera runs: free look, other camera records, the hand-back after a cutscene.
//! `game_sticks` flips the controller's share of the bytes the game gets (`modcam_pose::camera_sticks`), only while the current camera
//! record is the follow camera (class 0) or the free look (class 4 [I: its update 0x42bc00 reads the Up/Down word and not Left/Right,
//! like R&C1's class 4]). The camera hand reads the unflipped merged bytes (`modcam_ui::note_stick`) and flips them itself, so nothing
//! is flipped twice. Classic changes nothing. The camera record cell is `basis - 0x4730` [C: Aranos, level 1].

use crate::addrs::{Addrs, OPT_LR};
use rac_formats::bytes::{u16_at, u32_at};
use std::sync::Mutex;

/// Retail's free look camera class [I].
pub const FREE_LOOK_CLASS: i16 = 4;
/// The cell holding the current camera record is this far below the render basis [C].
const CUR_BELOW_BASIS: u32 = 0x4730;

struct Cache {
	level: Option<u32>,
	addrs: Option<Addrs>,
	tries: u32,
}

static CACHE: Mutex<Cache> = Mutex::new(Cache { level: None, addrs: None, tries: 0 });

/// Game thread, once per field before `set_pad`: `sticks` = the pad's [rx, ry, lx, ly] as the game would get them, `keys` = the
/// camera keys' share (`Input::keys`). Unchanged unless the Modern camera is on and retail's follow / free look camera is current.
pub fn game_sticks(ram: &[u8], sticks: [u8; 4], keys: [i8; 2]) -> [u8; 4] {
	if !crate::modcam_ui::modern() {
		return sticks;
	}
	let Some((_, class, _)) = current_record(ram) else { return sticks };
	if class != 0 && class != FREE_LOOK_CLASS {
		return sticks;
	}
	let lr_reversed = u32_at(ram, OPT_LR as usize).is_ok_and(|w| w == 0);
	let [rx, ry] = crate::modcam_pose::camera_sticks([sticks[0], sticks[1]], keys, class == FREE_LOOK_CLASS, lr_reversed);
	[rx, ry, sticks[2], sticks[3]]
}

/// The current camera record of the running level: (record address, its class `+0x86`, the camera basis); None while the level's
/// addresses are not found yet (retried every 30 calls) or the cell holds no record. Game thread, once per field (also `modcam_fp`).
pub fn current_record(ram: &[u8]) -> Option<(u32, i16, u32)> {
	let level = u32_at(ram, ee_oracle::rc2::LEVEL as usize).ok()?;
	let mut g = CACHE.lock().ok()?;
	if g.level != Some(level) {
		*g = Cache { level: Some(level), addrs: None, tries: 0 };
	}
	if g.addrs.is_none() {
		g.tries += 1;
		if g.tries % 30 == 1 {
			g.addrs = crate::scan_timing::timed("sticks_addrs", || Addrs::find(ram));
		}
	}
	let a = g.addrs?;
	let rec = u32_at(ram, a.basis.wrapping_sub(CUR_BELOW_BASIS) as usize).map_or(0, |w| (w & 0x01ff_ffff) as usize);
	if rec == 0 || rec + 0x90 > ram.len() {
		return None;
	}
	let class = u16_at(ram, rec + 0x86).map_or(-1, |c| c as i16);
	Some((rec as u32, class, a.basis))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn classic_or_no_level_leaves_the_bytes_alone() {
		// Without a RAM image of a running level there is nothing to flip (and the Modern default needs pc_options): the bytes pass.
		let s = [0xff, 0x00, 0x80, 0x40];
		assert_eq!(game_sticks(&[], s, [0, 0]), s);
		assert_eq!(game_sticks(&vec![0u8; 0x100], s, [0, 0]), s);
	}
}
