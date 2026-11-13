//! [PC] A small corner of the presented picture read back without waiting: a pack asks for a grid of samples of a window region
//! (`Gpu::want_corner`, one frame), the frame's encoder copies only the sampled window rows of that region into a staging buffer and
//! maps it on the frame's own submit (`record`, from `frame_finish`), and a later frame takes the samples (`Gpu::take_corner`). One
//! request is in flight at a time; nothing is copied while no pack asks. R&C2 uses it for the frame-buffer bytes its screen-spot
//! textures sample (rac2 screenspot.rs, round 3 batch 27e): a few rows of a few hundred pixels per field, never the whole frame.

use std::sync::atomic::{AtomicU8, Ordering::Relaxed};
use std::sync::{Arc, Mutex};

/// A grid of `cols` x `rows` samples of the window picture: sample (c, r) at window fraction (x0 + (c + 0.5) / cols * (x1 - x0),
/// y0 + (r + 0.5) / rows * (y1 - y0)), clamped to the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Req {
	pub x0: f32,
	pub x1: f32,
	pub y0: f32,
	pub y1: f32,
	pub cols: u32,
	pub rows: u32,
}

/// The samples of a request, row by row (RGBA, alpha 255).
pub struct Picture {
	pub cols: u32,
	pub rows: u32,
	pub rgba: Vec<[u8; 4]>,
}

const WAITING: u8 = 0;
const READY: u8 = 1;
const FAILED: u8 = 2;

/// The map callback's cell; dropped uncalled (the frame's encoder was never submitted) it reports a failed map.
struct Deliver(Arc<AtomicU8>);

impl Drop for Deliver {
	fn drop(&mut self) {
		let _ = self.0.compare_exchange(WAITING, FAILED, Relaxed, Relaxed);
	}
}

/// A copy in flight: its buffer, the window rows / columns it copied, the row stride, the colour order, the map state.
struct Flight {
	buf: wgpu::Buffer,
	req: Req,
	px0: u32,
	pw: u32,
	win_w: u32,
	stride: u32,
	bgr: bool,
	state: Arc<AtomicU8>,
}

#[derive(Default)]
struct Corner {
	want: Option<Req>,
	flight: Option<Flight>,
	spare: Option<wgpu::Buffer>,
}

static CORNER: Mutex<Option<Corner>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Corner) -> R) -> Option<R> {
	CORNER.lock().ok().map(|mut c| f(c.get_or_insert_with(Corner::default)))
}

/// The window pixel column range [px0, px0 + pw) a request reads, and the window row of each sample row.
pub(super) fn window_rows(r: &Req, w: u32, h: u32) -> (u32, u32, Vec<u32>) {
	let px = |f: f32| ((f.clamp(0.0, 1.0) * w as f32) as u32).min(w.saturating_sub(1));
	let (a, b) = (px(r.x0.min(r.x1)), px(r.x0.max(r.x1)));
	let rows = (0..r.rows.max(1)).map(|i| ((r.y0 + (i as f32 + 0.5) / r.rows.max(1) as f32 * (r.y1 - r.y0)).clamp(0.0, 1.0) * h as f32) as u32).map(|y| y.min(h.saturating_sub(1))).collect();
	(a, b - a + 1, rows)
}

/// Window column of sample column `c` (clamped into [px0, px0 + pw)).
pub(super) fn window_col(r: &Req, c: u32, w: u32, px0: u32, pw: u32) -> u32 {
	let f = (r.x0 + (c as f32 + 0.5) / r.cols.max(1) as f32 * (r.x1 - r.x0)).clamp(0.0, 1.0);
	((f * w as f32) as u32).clamp(px0, px0 + pw - 1)
}

impl super::Gpu {
	/// Ask for a sampled corner of this frame's picture (taken later by `take_corner`). Ignored while one is in flight.
	pub fn want_corner(&mut self, r: Req) {
		with(|c| c.want = Some(r));
	}

	/// The samples of the newest finished corner copy, if one finished. `wait`: (lockstep tests) wait for the copy recorded last.
	pub fn take_corner(&self, wait: bool) -> Option<Picture> {
		let pending = with(|c| c.flight.is_some()).unwrap_or(false);
		if !pending {
			return None;
		}
		if wait {
			let _ = self.queue.sync();
			self.device.poll(wgpu::PollType::wait_indefinitely()).ok();
		} else {
			self.device.poll(wgpu::PollType::Poll).ok();
		}
		with(|c| {
			let s = c.flight.as_ref()?.state.load(Relaxed);
			if s == WAITING {
				return None;
			}
			let f = c.flight.take()?;
			let out = (s == READY).then(|| read(&f)).flatten();
			f.buf.unmap();
			c.spare = Some(f.buf);
			out
		})
		.flatten()
	}

	/// `frame_finish`: records the asked copy of the window image `tex` (`w` x `h`) into `enc`, mapped on its submit.
	pub(super) fn corner_record(&self, enc: &mut wgpu::CommandEncoder, tex: &wgpu::Texture, w: u32, h: u32) {
		let bgr = matches!(self.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
		with(|c| {
			if c.flight.is_some() {
				return;
			}
			let Some(req) = c.want.take() else { return };
			let (px0, pw, rows) = window_rows(&req, w, h);
			// One row per sample row; the stride keeps every row at a 512-byte offset (the copy alignment of every backend).
			let stride = (pw * 4).next_multiple_of(512);
			let size = (stride * rows.len() as u32) as u64;
			let buf = match c.spare.take().filter(|b| b.size() == size) {
				Some(b) => b,
				None => self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("corner"), size, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false }),
			};
			for (i, &y) in rows.iter().enumerate() {
				enc.copy_texture_to_buffer(
					wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin: wgpu::Origin3d { x: px0, y, z: 0 }, aspect: wgpu::TextureAspect::All },
					wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: i as u64 * stride as u64, bytes_per_row: Some(stride), rows_per_image: Some(1) } },
					wgpu::Extent3d { width: pw, height: 1, depth_or_array_layers: 1 },
				);
			}
			let state = Arc::new(AtomicU8::new(WAITING));
			let cell = Deliver(state.clone());
			enc.map_buffer_on_submit(&buf, wgpu::MapMode::Read, .., move |r| cell.0.store(if r.is_ok() { READY } else { FAILED }, Relaxed));
			c.flight = Some(Flight { buf, req, px0, pw, win_w: w, stride, bgr, state });
		});
	}
}

/// The samples of a mapped copy.
fn read(f: &Flight) -> Option<Picture> {
	let data = f.buf.slice(..).get_mapped_range().ok()?;
	let r = &f.req;
	let mut rgba = Vec::with_capacity((r.cols * r.rows.max(1)) as usize);
	for row in 0..r.rows.max(1) as usize {
		for c in 0..r.cols {
			let x = (window_col(r, c, f.win_w, f.px0, f.pw) - f.px0) as usize;
			let p = &data[row * f.stride as usize + x * 4..][..4];
			rgba.push(if f.bgr { [p[2], p[1], p[0], 255] } else { [p[0], p[1], p[2], 255] });
		}
	}
	Some(Picture { cols: r.cols, rows: r.rows.max(1), rgba })
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn rows_and_columns_sample_the_region_inside_the_window() {
		let r = Req { x0: 0.25, x1: 0.5, y0: -0.1, y1: 0.1, cols: 4, rows: 2 };
		let (px0, pw, rows) = window_rows(&r, 400, 100);
		assert_eq!((px0, pw), (100, 101));
		// Rows above the window clamp to its top row.
		assert_eq!(rows, vec![0, 5]);
		let cols: Vec<u32> = (0..4).map(|c| window_col(&r, c, 400, px0, pw)).collect();
		assert_eq!(cols, vec![112, 137, 162, 187]);
		// A region past the right edge stays inside the window.
		let r = Req { x0: 0.5, x1: 1.5, y0: 0.0, y1: 1.0, cols: 2, rows: 1 };
		let (px0, pw, rows) = window_rows(&r, 100, 50);
		assert_eq!((px0, px0 + pw - 1, rows), (50, 99, vec![25]));
		assert_eq!(window_col(&r, 1, 100, px0, pw), 99);
	}
}
