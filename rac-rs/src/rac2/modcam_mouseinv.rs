//! The mouse's own invert rows, apply side (build 45 P5, sweep C7; R&C1's `rac1/retail/pcmouseinv.rs`): pc_options keys `mouse_ud` /
//! `mouse_lr` = `normal` | `inverted` (absent = Like camera, R&C1's file format, written by the Options rows of package P2). Like camera
//! = the mouse follows retail's Reversed camera options (addrs.rs OPT_LR / OPT_UD), exactly as before; Normal = right looks right, up
//! looks up; Inverted = as Reversed. Only the mouse: the stick keeps following retail's options. Read live from the options store, so
//! a row change applies on the next field. Used by the Modern camera (`modcam_hand`), the first-person mouse look (`modcam_fp`) and the
//! ship mouse's vertical axis (`pc_input.rs` `ship`).

pub const LIKE: u8 = 0;
pub const NORMAL: u8 = 1;
pub const INVERTED: u8 = 2;

/// The options-file keys of axis 0 (up/down) and 1 (left/right): R&C1's names.
const KEYS: [&str; 2] = ["mouse_ud", "mouse_lr"];

fn mode_of(v: Option<&str>) -> u8 {
	match v.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
		Some("normal") => NORMAL,
		Some("inverted") => INVERTED,
		_ => LIKE,
	}
}

/// The mode of axis `a` (0 up/down, 1 left/right), live from the options store.
pub fn mode(a: usize) -> u8 {
	mode_of(crate::options::get().extra_get(KEYS[a.min(1)]))
}

/// The mouse's turn sign of one axis for the mode `m` and retail's sign `retail` (-1.0 Reversed, 1.0 Normal).
pub fn sign_for(m: u8, retail: f32) -> f32 {
	match m {
		NORMAL => 1.0,
		INVERTED => -1.0,
		_ => retail,
	}
}

/// The mouse's (left/right, up/down) signs from retail's own (-1.0 Reversed, 1.0 Normal).
pub fn signs(lr: f32, ud: f32) -> (f32, f32) {
	(sign_for(mode(1), lr), sign_for(mode(0), ud))
}

/// The ship mouse's vertical axis is inverted. R&C2 has no ship-flight Y option that is known [?], so Like camera keeps the ship
/// mouse as it always was (up = climb) and only Inverted flips it.
pub fn ship_ud_inverted() -> bool {
	sign_for(mode(0), 1.0) < 0.0
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_file_words_are_r_and_c1s() {
		assert_eq!(mode_of(None), LIKE);
		assert_eq!(mode_of(Some("Normal")), NORMAL);
		assert_eq!(mode_of(Some(" inverted ")), INVERTED);
		assert_eq!(mode_of(Some("sideways")), LIKE);
	}

	#[test]
	fn like_camera_follows_retail_the_others_do_not() {
		for r in [-1.0, 1.0] {
			assert_eq!(sign_for(LIKE, r), r);
			assert_eq!(sign_for(NORMAL, r), 1.0);
			assert_eq!(sign_for(INVERTED, r), -1.0);
		}
	}
}
