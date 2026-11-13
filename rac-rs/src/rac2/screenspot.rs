//! Screen-spot textures (round 3 batch 27e): a RAM class TEX0 that reads a frame buffer as an 8-bit / 4-bit index texture (props.rs
//! `reads_screen`: the Heavy Lancer's 16 x 16 class 0xc89 at TBP 0 when fired, crate debris and small parts with 32 x 32 / 64 x 64 /
//! 128 x 128 TBP-0 slots) makes the PS2 sample the frame buffer's first pages, the picture's PSMCT32 bytes as indices through the class's
//! own CLUT [I: the game never uploads into those blocks, b27d log; F1 draws dark chunks]. The native picture has no frame buffer in GS
//! memory (those blocks hold boot art in a session started from boot: the hazard-striped boxes of build 29), so the source is the
//! native PRESENTED picture: while a class with such a slot exists, every frame asks the engine for a 128 x 32 grid of samples of the
//! window region the frame buffer's first two pages (frame pixels 0..128 x 0..32) are shown in (engine viewer/gpu/corner.rs: an async
//! copy of only those window rows, read one or two frames later), packs it as the frame buffer's PSMCT32 bytes at block 0
//! (engine gsfull/texreq_screen.rs `pack_ct32`) and the props decode the slot over it (`texture`).

use rac_engine::gsfull::texreq_screen::{self, Screen};
use rac_engine::viewer::gpu::corner::{Picture, Req};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// The frame pixels sampled: pages 0 and 1 of a PSMCT32 buffer (a page = 64 x 32 pixels); every screen-spot slot seen reads inside
/// them (TBP 0, 16 x 16 .. 128 x 128 PSMT8 at TBW 1 / 2) [C: b27d "screen read:" lines over 37 states + rec_user9].
const GRID_W: u32 = 128;
const GRID_H: u32 = 32;
/// The frame's alpha byte under the native picture (which has none): the PS2 draws its opaque 3D with alpha 0x80 [I].
const FRAME_ALPHA: u8 = 0x80;

/// The shown buffer (`Crtc::shown`: FBP blocks, FBW, DBX, DBY, w, h), from the game thread.
static SHOWN: Mutex<Option<(u32, u32, u32, u32, u32, u32)>> = Mutex::new(None);
/// The newest packed picture.
static PIC: Mutex<Option<Arc<Screen>>> = Mutex::new(None);
/// A class with a screen-spot slot is built (props.rs): the corner is read every frame.
static ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn note_shown(s: (u32, u32, u32, u32, u32, u32)) {
	if let Ok(mut v) = SHOWN.lock() {
		*v = Some(s);
	}
}

pub fn set_active(on: bool) {
	ACTIVE.store(on, Ordering::Relaxed);
}

/// The sample grid to ask for this frame (None while no screen-spot class exists): the frame pixels 0..GRID_W x 0..GRID_H of the shown
/// buffer, at the window fractions of `rect` (the game's picture in the window: x, y, w, h as fractions of the window).
pub fn request(rect: [f32; 4]) -> Option<Req> {
	if !ACTIVE.load(Ordering::Relaxed) {
		return None;
	}
	let (_, _, dbx, dby, w, h) = (*SHOWN.lock().ok()?)?;
	Some(grid(rect, dbx, dby, w, h))
}

/// The grid over frame pixels 0..GRID_W x 0..GRID_H of a buffer shown from (`dbx`, `dby`), `w` x `h` pixels, in the picture `rect`.
fn grid(rect: [f32; 4], dbx: u32, dby: u32, w: u32, h: u32) -> Req {
	let fx = |x: u32| rect[0] + (x as f32 - dbx as f32) / w.max(1) as f32 * rect[2];
	let fy = |y: u32| rect[1] + (y as f32 - dby as f32) / h.max(1) as f32 * rect[3];
	Req { x0: fx(0), x1: fx(GRID_W), y0: fy(0), y1: fy(GRID_H), cols: GRID_W, rows: GRID_H }
}

/// A finished grid: packed as the frame buffer's bytes at block 0 (the buffer a TBP-0 slot samples), `fbw` of the shown buffer.
pub fn note_picture(p: Picture) {
	let Some((_, fbw, ..)) = SHOWN.lock().ok().and_then(|s| *s) else { return };
	let Ok(mut cur) = PIC.lock() else { return };
	let seq = cur.as_ref().map_or(1, |s| s.seq + 1);
	*cur = Some(Arc::new(pack(&p, fbw, seq)));
}

fn pack(p: &Picture, fbw: u32, seq: u64) -> Screen {
	texreq_screen::pack_ct32(0, fbw.max(2), p.cols, p.rows, seq, |x, y| {
		let c = p.rgba[(y * p.cols + x) as usize];
		[c[0], c[1], c[2], FRAME_ALPHA]
	})
}

/// The newest picture's number (0: none yet).
pub fn seq() -> u64 {
	PIC.lock().ok().and_then(|p| p.as_ref().map(|s| s.seq)).unwrap_or(0)
}

/// The texture of screen-spot TEX0 `t` over the newest picture (index 0 everywhere before the first one) and that picture's number;
/// None until texreq has read `t` (its CLUT).
pub fn texture(t: u64) -> Option<(rac_formats::level::Texture, u64)> {
	let pic = PIC.lock().ok().and_then(|p| p.clone());
	let tex = texreq_screen::read_over(t, pic.as_deref())?;
	Some((tex, pic.map_or(0, |s| s.seq)))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_grid_covers_the_frame_corner_in_the_picture() {
		// A 512 x 512 buffer shown whole in a picture filling the window's middle half across.
		let r = grid([0.25, 0.0, 0.5, 1.0], 0, 0, 512, 512);
		assert_eq!((r.x0, r.x1, r.y0, r.y1, r.cols, r.rows), (0.25, 0.375, 0.0, 0.0625, 128, 32));
		// Shown from row 32: the sampled rows lie above the picture (the corner reads the picture's top row, corner.rs clamps).
		let r = grid([0.0, 0.0, 1.0, 1.0], 0, 32, 512, 448);
		assert!(r.y1 <= 0.0 && r.y0 < 0.0);
	}

	#[test]
	fn a_grid_packs_as_the_first_two_pages_at_block_0() {
		let p = Picture { cols: 128, rows: 32, rgba: (0..128 * 32).map(|i| [i as u8, (i >> 8) as u8, 7, 255]).collect() };
		let s = pack(&p, 8, 3);
		assert_eq!((s.first, s.bytes.len(), s.seq), (0, 64 * 256, 3));
		// Pixel (0, 0) is the first 4 bytes of block 0, with the frame alpha.
		assert_eq!(&s.bytes[..4], &[0, 0, 7, FRAME_ALPHA]);
	}
}
