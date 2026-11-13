//! Test: `RAC2_LIVECAP=DIR:FROM-TO` captures every PRESENTED frame of a live (real-time) session while the GS is through fields
//! FROM..=TO (`GsPipe::done`), with the game thread, the GS pipe and frame smoothing on their real clocks (round 3 batch 26: the lockstep
//! window shots never showed the swamp wobble the player saw). Each frame goes to DIR/pNNNNN.ppm (written on a thread of its own, so
//! the window keeps its pace) with one stderr line `livecap N: ...` (time, the GS field, the game's newest field, the scene drawn, the
//! smoothing place, the cameras of the fields around it). After TO the process ends (the frames written first). Several ranges:
//! `DIR:A-B,C-D/K,...`; `/K` keeps only the first frame of every K-th field of that range (a cheap search over a long replay).

use rac_engine::render::Image;
use std::sync::mpsc::{SyncSender, sync_channel};
use std::time::Instant;

pub(super) struct LiveCap {
	dir: String,
	ranges: Vec<(u64, u64, u64)>,
	to: u64,
	/// The GS field of the newest frame saved (a `/K` range keeps one frame per field).
	last: u64,
	n: u64,
	t0: Instant,
	tx: Option<SyncSender<(String, Image)>>,
	writer: Option<std::thread::JoinHandle<()>>,
}

fn parse(v: &str) -> Option<(String, Vec<(u64, u64, u64)>)> {
	let (dir, r) = v.rsplit_once(':')?;
	let mut out = Vec::new();
	for part in r.split(',') {
		let (ab, k) = part.split_once('/').map_or((part, Some(1)), |(x, k)| (x, k.trim().parse().ok()));
		let (a, b) = ab.split_once('-')?;
		out.push((a.trim().parse().ok()?, b.trim().parse().ok()?, k?.max(1)));
	}
	Some((dir.to_string(), out))
}

/// Field `done` is one to capture in `ranges` (`every`: a `/K` range keeps the first frame of every K-th field only, `last` saved).
fn wanted(ranges: &[(u64, u64, u64)], done: u64, last: u64) -> bool {
	ranges.iter().any(|&(a, b, k)| (a..=b).contains(&done) && (k == 1 || ((done - a) % k == 0 && last != done)))
}

impl LiveCap {
	pub(super) fn from_env() -> Option<LiveCap> {
		let (dir, ranges) = parse(&std::env::var("RAC2_LIVECAP").ok()?)?;
		std::fs::create_dir_all(&dir).ok()?;
		// Up to 600 frames wait for the writer (the window blocks beyond that rather than filling the RAM).
		let (tx, rx) = sync_channel::<(String, Image)>(600);
		let writer = std::thread::Builder::new()
			.name("livecap".into())
			.spawn(move || {
				for (path, img) in rx {
					let mut b = format!("P6\n{} {}\n255\n", img.w, img.h).into_bytes();
					b.extend_from_slice(&img.rgb);
					let _ = std::fs::write(&path, b);
				}
			})
			.ok()?;
		eprintln!("livecap: {dir} fields {ranges:?}");
		let to = ranges.iter().map(|r| r.1).max()?;
		Some(LiveCap { dir, ranges, to, last: u64::MAX, n: 0, t0: Instant::now(), tx: Some(tx), writer: Some(writer) })
	}

	/// The frame drawn now (the GS through `done`) is one to capture.
	pub(super) fn want(&self, done: u64) -> bool {
		wanted(&self.ranges, done, self.last)
	}

	pub(super) fn save(&mut self, done: u64, img: Image, info: &str) {
		let path = format!("{}/p{:05}_f{done:05}.ppm", self.dir, self.n);
		self.last = done;
		eprintln!("livecap {}: t {:.2} ms done {done} {info}", self.n, self.t0.elapsed().as_secs_f64() * 1e3);
		self.n += 1;
		if let Some(tx) = &self.tx {
			let _ = tx.send((path, img));
		}
	}

	/// Past TO: the frames are written, then the process ends.
	pub(super) fn end_if_done(&mut self, done: u64) {
		if done <= self.to {
			return;
		}
		drop(self.tx.take());
		if let Some(w) = self.writer.take() {
			let _ = w.join();
		}
		eprintln!("livecap: {} frames, done", self.n);
		std::process::exit(0);
	}
}

#[cfg(test)]
mod tests {
	#[test]
	fn parses_dir_and_range() {
		assert_eq!(super::parse("E:/x/y:100-250"), Some(("E:/x/y".into(), vec![(100, 250, 1)])));
		assert_eq!(super::parse("E:/x/y:1-5,10-20/5"), Some(("E:/x/y".into(), vec![(1, 5, 1), (10, 20, 5)])));
		assert_eq!(super::parse("nope"), None);
	}

	#[test]
	fn a_sparse_range_keeps_one_frame_of_every_kth_field() {
		let r = [(1, 5, 1), (10, 20, 5)];
		assert!(super::wanted(&r, 3, 3), "every frame of a full range");
		assert!(super::wanted(&r, 15, 14) && !super::wanted(&r, 15, 15), "first frame of field 15 only");
		assert!(!super::wanted(&r, 16, 15) && !super::wanted(&r, 7, 0) && !super::wanted(&r, 21, 0));
	}
}
