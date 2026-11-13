//! Screen-spot textures (game-neutral; R&C2 rac2/screenspot.rs feeds it, round 3 batch 27e): an index texture (PSMT8 / PSMT4) whose
//! texels lie in a frame buffer makes the real GS sample the picture there, its PSMCT32 bytes read as index bytes through the
//! texture's own CLUT. The native picture has no frame buffer in GS memory (the blocks hold stale bytes), so the pack hands a
//! substitute picture packed as the frame buffer's PSMCT32 bytes (`pack_ct32`) and decodes the texture over it (`read_over`) with the
//! CLUT kept from texreq's last read of that TEX0 (`keep_clut`).

use super::gs_mem::{a32, luts, VRAM_SIZE};
use crate::level::Texture;
use std::collections::HashMap;
use std::sync::Mutex;

/// A picture as GS memory bytes: blocks `first ..` (256 bytes each), PSMCT32 swizzled; `seq` numbers the pictures.
#[derive(Clone, Debug, Default)]
pub struct Screen {
	pub first: u32,
	pub bytes: Vec<u8>,
	pub seq: u64,
}

/// Packs a `w` x `h` PSMCT32 picture (`px(x, y)` = R, G, B, A bytes) as the bytes of a frame buffer at block `fbp`, `fbw` pages wide.
pub fn pack_ct32(fbp: u32, fbw: u32, w: u32, h: u32, seq: u64, px: impl Fn(u32, u32) -> [u8; 4]) -> Screen {
	let (lo, hi) = super::gs_mem::span(0, fbp, fbw, 0, 0, w.max(1), h.max(1));
	let mut bytes = vec![0u8; (hi - lo) as usize * 256];
	let l = luts();
	for y in 0..h {
		for x in 0..w {
			let a = a32(l, fbp, fbw, x, y) - lo as usize * 256;
			bytes[a..a + 4].copy_from_slice(&px(x, y));
		}
	}
	Screen { first: lo, bytes, seq }
}

/// The CLUT bytes (4 blocks from CBP) of the index TEX0s texreq read, by TEX0.
static CLUTS: Mutex<Option<HashMap<u64, (u32, Vec<u8>)>>> = Mutex::new(None);

fn index_format(psm: u32) -> bool {
	matches!(psm, 0x13 | 0x14 | 0x1b | 0x24 | 0x2c)
}

/// texreq read TEX0 `t` from VRAM `m`: keeps its CLUT's bytes. SAFETY: `m` points at VRAM_SIZE bytes.
pub(super) unsafe fn keep_clut(m: *const u8, t: u64) {
	if !index_format((t >> 20 & 0x3f) as u32) {
		return;
	}
	let cbp = (t >> 37 & 0x3fff) as u32;
	let a = cbp as usize * 256;
	if a + 1024 > VRAM_SIZE {
		return;
	}
	// SAFETY: `a + 1024 <= VRAM_SIZE` and `m` points at VRAM_SIZE bytes.
	let b = unsafe { std::slice::from_raw_parts(m.add(a), 1024) }.to_vec();
	if let Ok(mut c) = CLUTS.lock() {
		c.get_or_insert_with(HashMap::new).insert(t, (cbp, b));
	}
}

/// A new level: the kept CLUTs go with texreq's textures.
pub(super) fn clear() {
	if let Ok(mut c) = CLUTS.lock() {
		*c = None;
	}
}

/// The texture of index TEX0 `t` sampled from `screen` (blocks it does not cover read 0), through the CLUT texreq last read for it.
/// None until texreq has read `t` once.
pub fn read_over(t: u64, screen: Option<&Screen>) -> Option<Texture> {
	let (cbp, clut) = CLUTS.lock().ok()?.as_ref()?.get(&t)?.clone();
	Some(decode(t, cbp, &clut, screen))
}

/// Decodes `t` from a scratch GS memory that holds only `clut` at `cbp` and `screen`.
fn decode(t: u64, cbp: u32, clut: &[u8], screen: Option<&Screen>) -> Texture {
	static SCRATCH: Mutex<Vec<u8>> = Mutex::new(Vec::new());
	let mut m = SCRATCH.lock().unwrap_or_else(|e| e.into_inner());
	if m.len() != VRAM_SIZE {
		*m = vec![0; VRAM_SIZE];
	}
	let put = |m: &mut Vec<u8>, at: usize, b: &[u8]| {
		let n = b.len().min(VRAM_SIZE.saturating_sub(at));
		m[at..at + n].copy_from_slice(&b[..n]);
	};
	if let Some(s) = screen {
		put(&mut m, s.first as usize * 256, &s.bytes);
	}
	put(&mut m, cbp as usize * 256, clut);
	// SAFETY: `m` holds VRAM_SIZE bytes.
	let tex = unsafe { super::texreq::read(m.as_ptr(), t) };
	// Back to zeros for the next decode.
	if let Some(s) = screen {
		let at = s.first as usize * 256;
		m[at..(at + s.bytes.len()).min(VRAM_SIZE)].fill(0);
	}
	let at = cbp as usize * 256;
	m[at..(at + clut.len()).min(VRAM_SIZE)].fill(0);
	tex
}

#[cfg(test)]
mod tests {
	use super::*;

	/// 128 x 32 picture packed at block 0, 8 pages wide (PAL 512): blocks 0..64 (pages 0 and 1), and every pixel reads back.
	#[test]
	fn a_picture_packs_into_the_frame_buffer_pages_it_covers() {
		let px = |x: u32, y: u32| [x as u8, y as u8, (x ^ y) as u8, 0x80];
		let s = pack_ct32(0, 8, 128, 32, 7, px);
		assert_eq!((s.first, s.bytes.len(), s.seq), (0, 64 * 256, 7));
		let mut m = vec![0u8; VRAM_SIZE];
		m[..s.bytes.len()].copy_from_slice(&s.bytes);
		let l = luts();
		for (x, y) in [(0, 0), (7, 7), (8, 0), (63, 31), (64, 0), (127, 31)] {
			// SAFETY: `m` holds VRAM_SIZE bytes.
			let v = unsafe { super::super::gs_mem::rd_px(m.as_ptr(), l, 0, 0, 8, x, y) };
			assert_eq!(v.to_le_bytes(), px(x, y), "pixel ({x}, {y})");
		}
	}

	/// An 8-bit texture at TBP 0 over a packed picture: its 16 x 16 texels are exactly the bytes of the frame's first block (8 x 8
	/// pixels), each through the texture's CLUT as the GS reads it (`clut_read`).
	#[test]
	fn a_screen_spot_reads_the_frame_bytes_as_indices_through_its_clut() {
		let px = |x: u32, y: u32| [(x * 8 + y) as u8, 100 + x as u8, 200 + y as u8, 0x80];
		let s = pack_ct32(0, 8, 64, 32, 1, px);
		// CLUT bytes: entry slot i holds red i (the GS's CSM1 order permutes the slots; `clut_read` gives the real table).
		let cbp = 0x3100u32;
		let clut: Vec<u8> = (0..256u32).flat_map(|i| [i as u8, 0, 0, 0x80]).collect();
		let mut m = vec![0u8; VRAM_SIZE];
		m[cbp as usize * 256..][..1024].copy_from_slice(&clut);
		let mut tab = [[0u8; 4]; 512];
		// SAFETY: `m` holds VRAM_SIZE bytes.
		unsafe { super::super::gs_mem::clut_read(m.as_ptr(), &mut tab, cbp, 0, false, 0, 0x80 | 0x80 << 32) };
		// TEX0: TBP 0, TBW 1, PSMT8, 16 x 16, TCC 1, CBP 0x3100, CPSM CT32, CSM1.
		let t: u64 = 1 << 14 | 0x13 << 20 | 4 << 26 | 4 << 30 | 1 << 34 | (cbp as u64) << 37;
		let tex = decode(t, cbp, &clut, Some(&s));
		assert_eq!((tex.w, tex.h), (16, 16));
		let mut got: Vec<u8> = tex.rgba.iter().map(|p| p[0]).collect();
		let mut want: Vec<u8> = (0..8).flat_map(|y| (0..8).flat_map(move |x| px(x, y))).map(|b| tab[b as usize][0]).collect();
		got.sort_unstable();
		want.sort_unstable();
		assert_eq!(got, want);
		// Without a picture every texel is index 0; the scratch memory is clean again after each decode.
		let tex = decode(t, cbp, &clut, None);
		assert!(tex.rgba.iter().all(|p| p[0] == tab[0][0]));
		assert!(read_over(0xdead_0000_0000_0000 | t, None).is_none(), "no CLUT kept: not read yet");
	}
}
