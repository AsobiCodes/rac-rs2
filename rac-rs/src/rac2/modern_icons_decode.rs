//! [PC] R&C2's item / weapon / gadget pictures for the modern screens (lane icons, 2026-11-11).
//!
//! Where they live [C]: every level's primary section holds the HUD set (header = primary part 3, five banks = parts 4..8; the same
//! header layout as R&C1, `rac_formats::hud`). The difference to R&C1: the five banks are WAD-compressed (magic "WAD"), so R&C1's
//! reader saw compressed bytes and drew noise (the old "swizzle" hypothesis was wrong: after `wad::decompress` the textures are
//! plain linear PSMT8 with a CSM1 palette, exactly R&C1's format). `rac_formats::hud::hud_sprites_wad` does that.
//! An icon id 0xea60 + n names a sheet of 1..n frames (frame 0 = the menu picture, 32x32; the colour comes from the palette,
//! most are grey and the game tints them when it draws). The id is NOT the item id: the pause cell tables give (icon id, item id)
//! per cell (`notes_modern_screens_pages.md` 8; the weapons grid is `WEAPON_CELLS`).

use rac_formats::hud::{hud_sprites_wad, Hud};
use rac_formats::level::Level;

/// One decoded picture, straight-alpha RGBA8 (alpha = the palette alpha doubled and clamped, as the GS blends it).
#[derive(Clone, Debug, PartialEq)]
pub struct IconImage {
	pub w: usize,
	pub h: usize,
	pub rgba: Vec<u8>,
}

/// All HUD sheets of one level, by icon id.
pub struct IconSet {
	hud: Hud,
}

/// Weapons grid, cell order (4 columns x 6 rows): (icon id, item id) [C: level 6 0x28cbb8, notes 8].
pub const WEAPON_CELLS: [(u16, u16); 24] = [
	(0xea64, 30), (0xea65, 42), (0xea66, 22), (0xea6b, 24), (0xea68, 23), (0xea6a, 41), (0xea67, 26), (0xea74, 45),
	(0xea6d, 31), (0xea69, 29), (0xea73, 37), (0xea70, 27), (0xea71, 28), (0xea72, 25), (0xea6e, 32), (0xea75, 16),
	(0xea7a, 18), (0xea79, 12), (0xea7c, 53), (0xea7d, 14), (0xea7b, 17), (0xea76, 43), (0xea6c, 44), (0xea7e, 9),
];

/// The weapons grid's icon id of an item id (None: not a weapon-grid item; gadget icons come from their own cell tables).
pub fn weapon_icon_id(item_id: u16) -> Option<u16> {
	WEAPON_CELLS.iter().find(|c| c.1 == item_id).map(|c| c.0)
}

impl IconSet {
	/// Reads the HUD set of a loaded level (`rac_formats::rc2::level::load_level`).
	pub fn from_level(level: &Level) -> Result<IconSet, String> {
		Ok(IconSet { hud: hud_sprites_wad(level, false).map_err(|e| e.to_string())? })
	}

	/// Frame `frame` of sheet `icon_id` (0xea60 + n); None when the sheet or the frame is not in this level's HUD set.
	pub fn frame(&self, icon_id: u16, frame: usize) -> Option<IconImage> {
		let i = self.hud.icons.iter().find(|i| i.id == icon_id)?;
		if frame >= i.count {
			return None;
		}
		let t = &self.hud.frames.get(i.first + frame)?.as_ref()?.tex;
		Some(IconImage { w: t.w, h: t.h, rgba: t.rgba.iter().flatten().copied().collect() })
	}

	/// The menu picture (frame 0) of icon id `icon_id`.
	pub fn icon_for(&self, icon_id: u16) -> Option<IconImage> {
		self.frame(icon_id, 0)
	}

	/// Every sheet id the level carries, ascending.
	pub fn ids(&self) -> Vec<u16> {
		let mut v: Vec<u16> = self.hud.icons.iter().map(|i| i.id).collect();
		v.sort_unstable();
		v
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn iso() -> Option<String> {
		let p = std::env::var("RAC2_ISO").unwrap_or_else(|_| "E:/Ratchet & Clank 2 (Europe, Australia) (En,Fr,De,Es,It)/Ratchet & Clank 2 (Europe, Australia) (En,Fr,De,Es,It).iso".into());
		std::path::Path::new(&p).exists().then_some(p)
	}

	fn px(i: &IconImage, x: usize, y: usize) -> [u8; 4] {
		let o = (y * i.w + x) * 4;
		[i.rgba[o], i.rgba[o + 1], i.rgba[o + 2], i.rgba[o + 3]]
	}

	#[test]
	fn weapon_cells_map() {
		assert_eq!(weapon_icon_id(30), Some(0xea64));
		assert_eq!(weapon_icon_id(9), Some(0xea7e));
		assert_eq!(weapon_icon_id(1), None);
	}

	/// Known pixels of level 2's sheets (values read from the decoded pictures, compared by eye with PCSX2's pause menu).
	#[test]
	fn known_pixels() {
		let Some(iso) = iso() else { return };
		let set = IconSet::from_level(&rac_formats::rc2::level::load_level(&iso, 2).unwrap()).unwrap();
		assert!(set.ids().len() >= 89);
		let gear = set.icon_for(0xea64).unwrap();
		assert_eq!((gear.w, gear.h, gear.rgba.len()), (32, 32, 32 * 32 * 4));
		assert_eq!(px(&gear, 0, 0), [102, 102, 102, 0]);
		assert_eq!(px(&gear, 16, 16), [127, 127, 127, 255]);
		let eye = set.icon_for(0xea65).unwrap();
		assert_eq!(px(&eye, 16, 2), [126, 126, 126, 240]);
		let bolt = set.icon_for(0xea62).unwrap();
		assert_eq!(px(&bolt, 0, 0), [243, 192, 125, 255]);
		assert!(set.frame(0xea62, 2).is_none());
		assert!(set.icon_for(0xffff).is_none());
	}

	/// Every weapon-grid icon exists as a 32x32 picture with some opaque pixels.
	#[test]
	fn all_weapon_icons_decode() {
		let Some(iso) = iso() else { return };
		let set = IconSet::from_level(&rac_formats::rc2::level::load_level(&iso, 2).unwrap()).unwrap();
		for (icon, _) in WEAPON_CELLS {
			let i = set.icon_for(icon).unwrap_or_else(|| panic!("icon {icon:x} missing"));
			assert_eq!((i.w, i.h), (32, 32), "{icon:x}");
			assert!(i.rgba.chunks(4).filter(|p| p[3] > 200).count() > 40, "{icon:x} looks empty");
		}
	}
}
