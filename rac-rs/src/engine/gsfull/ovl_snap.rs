//! The GS pipe's share of a quick save (R&C2 F5, rac2/states.rs): the 4 MB GS local memory and VU1's memories as they are after one
//! given field. The host asks (`request`) before it sends that field; each stage of the pipe hands over its part when it is through
//! the field, so the pieces match the EE state saved at the end of that field. New file 2026-11-02 (R&C2 round 3 batch 10).
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct Snap {
	want: AtomicU64,
	vram: Mutex<Option<Vec<u8>>>,
	vu1: Mutex<Option<(Vec<u8>, Vec<u8>)>>,
}

impl Snap {
	/// Asks for the state after `field` (>= 1; call before the field is sent to the pipe).
	pub fn request(&self, field: u64) {
		*self.vram.lock().unwrap() = None;
		*self.vu1.lock().unwrap() = None;
		self.want.store(field, Ordering::SeqCst);
	}

	/// True while the pipe is asked for the state after `field`.
	pub fn wants(&self, field: u64) -> bool {
		self.want.load(Ordering::SeqCst) == field
	}

	/// The GS stage: local memory after the field.
	pub fn put_vram(&self, v: Vec<u8>) {
		*self.vram.lock().unwrap() = Some(v);
	}

	/// The VIF1 + VU1 stage: VU1 data and micro memory (bytes, as vu1Memory.bin / vu1MicroMem.bin) after the field.
	pub fn put_vu1(&self, vu: &crate::gsfull::vu1::Vu1) {
		let d: Vec<u8> = vu.data.iter().flat_map(|q| q.iter().flat_map(|w| w.to_le_bytes())).collect();
		let m: Vec<u8> = vu.micro().iter().flat_map(|w| w.to_le_bytes()).collect();
		*self.vu1.lock().unwrap() = Some((d, m));
	}

	/// Waits (up to `max`) for both parts: (local memory, (VU1 data, VU1 micro)). None: the pipe did not deliver (it ended or was behind).
	pub fn take(&self, max: Duration) -> Option<(Vec<u8>, (Vec<u8>, Vec<u8>))> {
		let t = Instant::now();
		loop {
			{
				let (mut a, mut b) = (self.vram.lock().unwrap(), self.vu1.lock().unwrap());
				if a.is_some() && b.is_some() {
					self.want.store(0, Ordering::SeqCst);
					return Some((a.take()?, b.take()?));
				}
			}
			if t.elapsed() > max {
				self.want.store(0, Ordering::SeqCst);
				return None;
			}
			std::thread::sleep(Duration::from_millis(5));
		}
	}
}
