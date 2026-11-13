//! [PC] R&C2's modern Sound page (build 46 W4; the Sound page of the pause Options list, drawn like R&C1's `pause.rs` `sound_parts`:
//! two volume bars and the stereo / mono row). Pure side: the retail words it reads and the rows it builds; the game-thread read is
//! `modern_optlist.rs` `read_sound`, the draw is `modern_optlist_page.rs` / `host/screens_optlist.rs`. Retail keeps running underneath
//! and keeps its own cursor and values: the player's pad passes bit for bit (Left / Right change the bar, Up / Down move the cursor,
//! Cross toggles stereo / mono, Triangle backs out), the page only draws.
//!
//! Facts [C, RAM of a find-out run on fg1 0001 with the Sound page up, 2026-11-12, `scratch_rc2_b42/b46/w4/runs/sound`, and the
//! level-0 code at 0x4380b8 (the page's update, called through the item 0x1ab3a8 callbacks 0x34ebc8 / 0x30f8b8)]:
//! - Sound page record id 0x20, parent = the Options record (id 8, whose parent is the pause root, id 2).
//! - The page's widget object S = [menu manager word] + 0x3db28; cursor [S+0x2d8] = 0 sound effects, 1 music, 2 stereo / mono. Up
//!   (0x1000) = cursor + 2 mod 3, Down (0x4000) = cursor + 1 mod 3.
//! - Values (boot ELF globals, the saved option block): sound effects [0x1a7c28], music [0x1a7c24], both 0..=0x400 (0x233 = the bar
//!   at 55 %); Left (0x8000, held word 0x1383c0) = value - 3 clamped at 0, Right (0x2000) = value + 3 clamped at 0x400, every field the
//!   button is held; stereo flag [0x1a7c20] (1 = STEREO, the picture showed STEREO with 1); Cross (0x40) on row 2 toggles it.
//! - Back = Triangle (0x10, the update's `andi 0x10` path writes the parent page back); Start / Select (0x900) close the menu. Circle (0x20)
//!   is not read by the page.
//! - Texts: caption 0xba8 "Sound", rows 0xe34 "Sound effects volume", 0xe35 "Music volume", 0xe36 "Toggle stereo/mono", values 0xe38
//!   "STEREO" / 0xe37 "MONO".
use rac_engine::ui::kit::page::PageRow;

pub const PAGE: u32 = 0x20;
pub const OBJ_OFF: u32 = 0x3db28;
pub const CURSOR_OFF: u32 = 0x2d8;
pub const SFX: u32 = 0x1a7c28;
pub const MUSIC: u32 = 0x1a7c24;
pub const STEREO: u32 = 0x1a7c20;
pub const VOLUME_MAX: u32 = 0x400;
/// Text ids: the three row labels, MONO, STEREO.
pub const TEXT_IDS: [u32; 5] = [0xe34, 0xe35, 0xe36, 0xe37, 0xe38];
pub const CAPTION: u32 = 0xba8;

/// What the page shows this field.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Sound {
	pub sfx: u32,
	pub music: u32,
	pub stereo: bool,
	pub cursor: usize,
	/// The five texts of `TEXT_IDS`.
	pub text: Vec<String>,
}

fn rd(ram: &[u8], a: u32) -> u32 {
	let a = (a & 0x1ff_fffc) as usize;
	ram.get(a..a + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The retail words of the page (sfx, music, stereo, cursor); None when they are not plausible (a volume above its maximum, a cursor
/// past the third row: another level's layout or a stale page). Pure over RAM.
pub fn read(ram: &[u8], manager: u32) -> Option<(u32, u32, bool, usize)> {
	if manager == 0 {
		return None;
	}
	let (sfx, music, st) = (rd(ram, SFX), rd(ram, MUSIC), rd(ram, STEREO));
	let cursor = rd(ram, manager.wrapping_add(OBJ_OFF + CURSOR_OFF));
	(sfx <= VOLUME_MAX && music <= VOLUME_MAX && st <= 1 && cursor < 3).then_some((sfx, music, st == 1, cursor as usize))
}

/// A volume as a percentage (R&C1's `percent`).
pub fn percent(v: u32) -> String {
	format!("{} %", v.min(VOLUME_MAX) * 100 / VOLUME_MAX)
}

/// The rows of the page: two bars with their percentage and the stereo / mono row; the cursor row selected. Pure.
pub fn rows(s: &Sound) -> Vec<PageRow> {
	let t = |i: usize| s.text.get(i).cloned().unwrap_or_default();
	let bar = |v: u32| Some(v.min(VOLUME_MAX) as f32 / VOLUME_MAX as f32);
	let row = |i: usize, label: String, value: String, bar: Option<f32>| PageRow { label, value, bar, selected: s.cursor == i, ..Default::default() };
	vec![
		row(0, t(0), percent(s.sfx), bar(s.sfx)),
		row(1, t(1), percent(s.music), bar(s.music)),
		row(2, t(2), t(if s.stereo { 4 } else { 3 }), None),
	]
}

#[cfg(test)]
mod tests {
	use super::*;

	fn ram(sfx: u32, music: u32, st: u32, cur: u32) -> Vec<u8> {
		let mut r = vec![0u8; 0x200_0000];
		let mut w = |a: u32, v: u32| r[a as usize..a as usize + 4].copy_from_slice(&v.to_le_bytes());
		w(SFX, sfx);
		w(MUSIC, music);
		w(STEREO, st);
		w(0x1fb8000 + OBJ_OFF + CURSOR_OFF, cur);
		r
	}

	#[test]
	fn reads_the_find_out_run_values() {
		// The run's values at field 160 / 260: sfx 0x400, music 0x233, stereo 1, cursor 0 then 1.
		assert_eq!(read(&ram(0x400, 0x233, 1, 0), 0x1fb8000), Some((0x400, 0x233, true, 0)));
		assert_eq!(read(&ram(0x400, 0x233, 1, 1), 0x1fb8000), Some((0x400, 0x233, true, 1)));
		assert_eq!(read(&ram(0x401, 0, 1, 0), 0x1fb8000), None, "a volume above its maximum is not this page");
		assert_eq!(read(&ram(0, 0, 1, 3), 0x1fb8000), None, "cursor past row 2");
		assert_eq!(read(&ram(0, 0, 1, 0), 0), None, "no manager");
	}

	#[test]
	fn rows_show_bars_and_the_stereo_word() {
		let s = Sound { sfx: 0x400, music: 0x233, stereo: true, cursor: 1, text: ["SFX", "Music", "Toggle", "MONO", "STEREO"].map(String::from).to_vec() };
		let r = rows(&s);
		assert_eq!(r.len(), 3);
		assert_eq!((r[0].value.as_str(), r[0].bar), ("100 %", Some(1.0)));
		assert_eq!(r[1].value, "54 %");
		assert!(r[1].selected && !r[0].selected);
		assert_eq!((r[2].value.as_str(), r[2].bar), ("STEREO", None));
		let m = rows(&Sound { stereo: false, ..s });
		assert_eq!(m[2].value, "MONO");
	}
}
