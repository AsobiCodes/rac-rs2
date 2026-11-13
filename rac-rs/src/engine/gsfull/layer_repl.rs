//! [PC] Texture packs for the effect and world-layer triangles (the software GS's native layers, `gs_layers.rs`): the textures of
//! sparks, smoke, flames and shots are read from local memory as RGBA and handed to the native draw, so a pack's image goes in right
//! there. The name is the PCSX2 name of the texture as local memory holds it (`gsname::name_for_draw`, the same one the GPU capture
//! gives its HUD prims), the image comes from the HUD registry (`hudrepl`, decoded on loader threads). While the file is still decoding
//! the caller keeps the original and does not cache it, so the next field asks again. Without a pack the first line returns.
use crate::level::Texture;
use crate::texpack::gsname;
use crate::texpack::hudrepl::{self, Count, HudImg, State};

/// What the pack has for a layer texture.
pub enum Repl {
	/// Nothing (no pack, no file, a format that is no texture, a region of a bigger one): the original stays.
	None,
	/// The file is still decoding (or decodes again): the original is drawn now and must not be cached.
	Pending,
	/// The pack's image.
	Done(Texture),
}

/// The engine texture of a decoded replacement (RGBA8, straight alpha on the engine's scale).
pub fn texture_of(img: &HudImg) -> Texture {
	Texture { w: img.w as usize, h: img.h as usize, rgba: img.rgba.chunks_exact(4).map(|p| [p[0], p[1], p[2], p[3]]).collect() }
}

/// After `Slot::rearm`: `Pending` when the slot was queued again, else the original stays (`None`).
fn pending_if(rearmed: bool) -> Repl {
	if rearmed {
		Repl::Pending
	} else {
		Repl::None
	}
}

/// The replacement of the texture `tex0` names in `vram` (4 MB of local memory with the uploads flushed).
pub fn replace(vram: &[u8], tex0: u64, texa: u64, texclut: u64) -> Repl {
	if !hudrepl::enabled() {
		return Repl::None;
	}
	let Some(n) = gsname::name_for_draw(vram, tex0, 0, texa, texclut, false) else { return Repl::None };
	let (tw, th) = (1u32 << ((tex0 >> 26) & 15).min(10), 1u32 << ((tex0 >> 30) & 15).min(10));
	if n.x != 0 || n.y != 0 || n.w != tw || n.h != th {
		return Repl::None;
	}
	hudrepl::count(Count::Prims);
	let slot = hudrepl::lookup(&n.name);
	match slot.state() {
		State::Ready => match slot.image() {
			Some(img) => {
				hudrepl::count(Count::Replaced);
				Repl::Done(texture_of(&img))
			}
			None => Repl::Pending,
		},
		State::Pending => Repl::Pending,
		// Pending (not cached) only when the slot really went back to the loaders; an OverBudget slot the budget still has no room
		// for keeps the original, cacheable (review A6: no VRAM read + name hash of that texture every field).
		State::Released | State::Dropped | State::OverBudget => pending_if(slot.rearm()),
		_ => {
			hudrepl::count(Count::Missing);
			Repl::None
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn an_image_becomes_a_texture() {
		let img = HudImg { w: 2, h: 1, rgba: vec![1, 2, 3, 4, 5, 6, 7, 8] };
		let t = texture_of(&img);
		assert_eq!((t.w, t.h, t.rgba), (2, 1, vec![[1, 2, 3, 4], [5, 6, 7, 8]]));
	}

	#[test]
	fn without_a_pack_the_original_stays() {
		assert!(matches!(replace(&[], 0, 0, 0), Repl::None));
	}

	#[test]
	fn only_a_real_rearm_is_pending() {
		assert!(matches!(pending_if(true), Repl::Pending));
		assert!(matches!(pending_if(false), Repl::None));
	}
}
