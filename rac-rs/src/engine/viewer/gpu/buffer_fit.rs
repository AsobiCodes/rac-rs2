//! Buffer sizes against the device limit (R&C2 batch 13, the 0200 crash: a level's tie vertices were 342 MB, over wgpu's 256 MiB
//! default `max_buffer_size`, and `create_buffer` panicked). Three layers, none changes a picture that already fits:
//! - `want`: the device asks for the adapter's own `max_buffer_size` (up to 1 GiB; every Vulkan / DX12 adapter reports at least 2 GiB),
//!   so a big level simply fits;
//! - `fit_prefix`: a batch of meshes that still does not fit keeps its leading meshes that do (the rest are empty, logged once);
//! - `clamp_size` / `clamp_write`: whatever buffer creation or write is still over the limit is cut to it instead of a validation panic.

/// The `max_buffer_size` to request: the adapter's, at most 1 GiB, never below wgpu's default.
pub(super) fn want(adapter: &wgpu::Adapter) -> u64 {
	want_of(adapter.limits().max_buffer_size)
}

fn want_of(adapter_max: u64) -> u64 {
	adapter_max.min(1 << 30).max(wgpu::Limits::default().max_buffer_size)
}

/// How many leading meshes of `counts` (vertices, indices) fit in one vertex buffer of `vertex_bytes` per vertex and one index
/// buffer (4 bytes per index), each at most `limit` bytes.
pub(super) fn fit_prefix(counts: &[(usize, usize)], vertex_bytes: u64, limit: u64) -> usize {
	let (mut v, mut i) = (0u64, 0u64);
	for (n, &(nv, ni)) in counts.iter().enumerate() {
		v += nv as u64 * vertex_bytes;
		i += ni as u64 * 4;
		if v > limit || i > limit {
			return n;
		}
	}
	counts.len()
}

/// `size` cut to the device limit (rounded down to 4); says so once per size.
pub(in crate::viewer) fn clamp_size(device: &wgpu::Device, label: &str, size: u64) -> u64 {
	let limit = device.limits().max_buffer_size & !3;
	if size <= limit {
		return size;
	}
	static SAID: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<u64>>> = std::sync::OnceLock::new();
	if SAID.get_or_init(Default::default).lock().map_or(true, |mut s| s.insert(size)) {
		eprintln!("gpu: buffer '{label}' needs {size} bytes, over the device limit {limit}: cut to the limit");
	}
	limit
}

/// The part of `data` written at `off` that lies inside a buffer of `size` bytes.
pub(super) fn clamp_write(size: u64, off: u64, data: &[u8]) -> &[u8] {
	let room = size.saturating_sub(off) as usize;
	&data[..data.len().min(room)]
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn fit_keeps_leading_meshes_under_both_limits() {
		let c = [(10, 30), (10, 30), (10, 30)];
		assert_eq!(fit_prefix(&c, 64, 10_000), 3);
		assert_eq!(fit_prefix(&c, 64, 1_280), 2); // 2 x 640 vertex bytes
		assert_eq!(fit_prefix(&c, 64, 639), 0);
		assert_eq!(fit_prefix(&c, 1, 239), 1); // indices: 30 x 4 = 120 per mesh
		assert_eq!(fit_prefix(&[], 64, 0), 0);
	}

	#[test]
	fn the_0200_tie_batch_is_cut_under_the_default_limit() {
		let limit = wgpu::Limits::default().max_buffer_size;
		let counts = vec![(463usize, 1200usize); 11_538]; // 342 MB of 64-byte vertices
		let n = fit_prefix(&counts, 64, limit);
		assert!(n > 0 && n < counts.len());
		assert!(n as u64 * 463 * 64 <= limit && (n as u64 + 1) * 463 * 64 > limit);
	}

	#[test]
	fn want_is_between_the_default_and_one_gib() {
		let d = wgpu::Limits::default().max_buffer_size;
		assert_eq!(want_of(0), d);
		assert_eq!(want_of(u64::MAX), 1 << 30);
		assert_eq!(want_of(512 << 20), 512 << 20);
	}

	#[test]
	fn writes_stay_inside_the_buffer() {
		let d = [1u8; 16];
		assert_eq!(clamp_write(16, 0, &d).len(), 16);
		assert_eq!(clamp_write(10, 4, &d).len(), 6);
		assert_eq!(clamp_write(10, 12, &d).len(), 0);
	}
}
