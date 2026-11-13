//! [b51 G5] [PC] RAC_DIAG, the part of a frame that is NOT the redraw: the wait between two redraws (window loop), the tail of the
//! redraw function after the profiled frame ended (pace wait), and the game thread's time per field. Only games whose pack asks
//! (`Pack::diag_detail`) call the hooks or get the columns; a pack without it keeps its record byte-identical.
//!
//! Hooks (any thread, relaxed atomics, no allocation): `note_tail` (host, after the frame-cap wait), `game_field` (game thread, per
//! field). `frame` in diag.rs takes them once per display frame and keeps a per-second `Ext`.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};

/// Frame blocks per second and per file (detail mode); the rest are counted in the row's `spikes` column.
pub const BLOCKS_PER_SECOND: u32 = 8;
pub const BLOCKS_PER_FILE: u32 = 1500;

/// Detail mode's block rule: a frame interval over max(12 ms, 2x the last non-load second's median).
pub fn block_limit(median_ms: f32) -> f32 {
	(2.0 * median_ms).max(12.0)
}

static TAIL_BITS: AtomicU32 = AtomicU32::new(0);
static G_FIELDS: AtomicU64 = AtomicU64::new(0);
static G_FRAME_US: AtomicU64 = AtomicU64::new(0);
static G_HOST_US: AtomicU64 = AtomicU64::new(0);
static G_SEND_US: AtomicU64 = AtomicU64::new(0);
static G_FRAME_MAX_US: AtomicU64 = AtomicU64::new(0);
static G_SEND_MAX_US: AtomicU64 = AtomicU64::new(0);

/// The redraw function's tail after the profiled frame ended (take_corner, shot bookkeeping, the frame-cap wait), in ms: belongs
/// to the interval that ends with the NEXT frame.
pub fn note_tail(ms: f32) {
	if super::on() {
		TAIL_BITS.store(ms.max(0.0).to_bits(), Relaxed);
	}
}

/// One game field ran (game thread): ms in `Machine::frame`, ms of other per-field host work, ms blocked in the send to the GS pipe.
pub fn game_field(frame: f32, host: f32, send: f32) {
	if super::on() {
		let us = |ms: f32| (ms.max(0.0) * 1000.0) as u64;
		G_FIELDS.fetch_add(1, Relaxed);
		G_FRAME_US.fetch_add(us(frame), Relaxed);
		G_HOST_US.fetch_add(us(host), Relaxed);
		G_SEND_US.fetch_add(us(send), Relaxed);
		G_FRAME_MAX_US.fetch_max(us(frame), Relaxed);
		G_SEND_MAX_US.fetch_max(us(send), Relaxed);
	}
}

/// The game thread's fields since the last display frame (ms).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Game {
	pub fields: u64,
	pub frame_sum: f32,
	pub frame_max: f32,
	pub host_sum: f32,
	pub send_sum: f32,
	pub send_max: f32,
}

/// What one display frame's interval held besides the redraw.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Outside {
	/// Frame end of the previous redraw to the start of this one.
	pub gap: f32,
	/// Of the gap: the previous redraw's tail (pace wait).
	pub tail: f32,
	pub game: Game,
}

impl Outside {
	/// The window loop's own wait: the gap minus the tail.
	pub fn wait(&self) -> f32 {
		(self.gap - self.tail).max(0.0)
	}
}

/// Takes the hooks' values (once per display frame, render thread).
pub fn take(gap_ms: f32) -> Outside {
	let ms = |a: &AtomicU64| a.swap(0, Relaxed) as f32 / 1000.0;
	let fields = G_FIELDS.swap(0, Relaxed);
	let game = Game { fields, frame_sum: ms(&G_FRAME_US), frame_max: ms(&G_FRAME_MAX_US), host_sum: ms(&G_HOST_US), send_sum: ms(&G_SEND_US), send_max: ms(&G_SEND_MAX_US) };
	Outside { gap: gap_ms.max(0.0), tail: f32::from_bits(TAIL_BITS.swap(0, Relaxed)).min(gap_ms.max(0.0)), game }
}

/// Where a frame's time was: the biggest of the redraw, the window loop's wait and the tail (pace), in plain words.
pub fn where_text(redraw: f32, o: &Outside) -> String {
	let (w, t) = (o.wait(), o.tail);
	let (name, ms) = if redraw >= w && redraw >= t {
		("inside the redraw", redraw)
	} else if w >= t {
		("window loop wait between redraws", w)
	} else {
		("pace wait / redraw tail", t)
	};
	format!("{name} {ms:.1} ms")
}

/// The block's line for what is outside the redraw.
pub fn block_line(redraw: f32, o: &Outside) -> String {
	let g = &o.game;
	let game = if g.fields == 0 {
		"game thread: no field ran".to_string()
	} else {
		let n = g.fields as f32;
		format!("game thread {} fields: field avg {:.2} worst {:.2}, host avg {:.2}, send-blocked avg {:.2} worst {:.2}", g.fields, g.frame_sum / n, g.frame_max, g.host_sum / n, g.send_sum / n, g.send_max)
	};
	format!("  where: {}\n  outside redraw: between redraws {:.1} ms = window loop wait {:.1} + pace/tail {:.1}; {game}\n", where_text(redraw, o), o.gap, o.wait(), o.tail)
}

/// One second of `Outside` (detail mode's extra perf.csv columns).
#[derive(Clone, Debug, Default)]
pub struct Ext {
	pub frames: u32,
	pub wait_sum: f32,
	pub wait_max: f32,
	pub tail_sum: f32,
	pub tail_max: f32,
	pub fields: u64,
	pub frame_sum: f32,
	pub frame_max: f32,
	pub host_sum: f32,
	pub send_sum: f32,
	pub send_max: f32,
	/// Blocks written in full this second.
	pub blocks: u32,
}

impl Ext {
	pub fn add(&mut self, o: &Outside) {
		self.frames += 1;
		self.wait_sum += o.wait();
		self.wait_max = self.wait_max.max(o.wait());
		self.tail_sum += o.tail;
		self.tail_max = self.tail_max.max(o.tail);
		let g = &o.game;
		self.fields += g.fields;
		self.frame_sum += g.frame_sum;
		self.frame_max = self.frame_max.max(g.frame_max);
		self.host_sum += g.host_sum;
		self.send_sum += g.send_sum;
		self.send_max = self.send_max.max(g.send_max);
	}
}

/// The extra header columns (appended after the engine's own).
pub fn header() -> &'static str {
	",loop_wait_avg_ms,loop_wait_max_ms,tail_avg_ms,tail_max_ms,game_fields,game_field_avg_ms,game_field_max_ms,game_host_avg_ms,game_send_avg_ms,game_send_max_ms,blocks"
}

/// The extra columns of a second (same order as `header`).
pub fn row(e: &Ext) -> String {
	let nf = e.frames.max(1) as f32;
	let ng = e.fields.max(1) as f32;
	format!(",{:.2},{:.2},{:.2},{:.2},{},{:.2},{:.2},{:.2},{:.2},{:.2},{}", e.wait_sum / nf, e.wait_max, e.tail_sum / nf, e.tail_max, e.fields, e.frame_sum / ng, e.frame_max, e.host_sum / ng, e.send_sum / ng, e.send_max, e.blocks)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_block_rule_is_12_ms_or_twice_the_median() {
		assert_eq!(block_limit(0.0), 12.0);
		assert_eq!(block_limit(6.4), 12.8);
		assert_eq!(block_limit(16.0), 32.0);
	}

	#[test]
	fn the_window_loop_wait_is_the_gap_minus_the_tail() {
		let o = Outside { gap: 20.0, tail: 4.0, game: Game::default() };
		assert_eq!(o.wait(), 16.0);
		assert_eq!(Outside { gap: 2.0, tail: 5.0, game: Game::default() }.wait(), 0.0);
	}

	#[test]
	fn where_names_the_biggest_part() {
		let o = Outside { gap: 20.0, tail: 3.0, game: Game::default() };
		assert!(where_text(4.0, &o).starts_with("window loop wait"));
		assert!(where_text(25.0, &o).starts_with("inside the redraw"));
		assert!(where_text(4.0, &Outside { gap: 12.0, tail: 11.0, game: Game::default() }).starts_with("pace wait"));
	}

	#[test]
	fn the_row_has_as_many_columns_as_the_header() {
		let mut e = Ext::default();
		e.add(&Outside { gap: 20.0, tail: 4.0, game: Game { fields: 2, frame_sum: 6.0, frame_max: 4.0, host_sum: 1.0, send_sum: 0.5, send_max: 0.4 } });
		e.add(&Outside { gap: 10.0, tail: 4.0, game: Game::default() });
		assert_eq!(row(&e).split(',').count(), header().split(',').count());
		assert!(row(&e).starts_with(",11.00,16.00,4.00,4.00,2,3.00,4.00,0.50,0.25,0.40,"));
		assert_eq!(row(&Ext::default()).split(',').count(), header().split(',').count());
	}

	#[test]
	fn the_block_line_names_the_game_thread() {
		let o = Outside { gap: 18.0, tail: 2.0, game: Game { fields: 3, frame_sum: 9.0, frame_max: 5.0, host_sum: 1.5, send_sum: 0.0, send_max: 0.0 } };
		let l = block_line(4.0, &o);
		assert!(l.contains("where: window loop wait between redraws 16.0 ms"));
		assert!(l.contains("game thread 3 fields: field avg 3.00 worst 5.00"));
		assert!(block_line(4.0, &Outside::default()).contains("no field ran"));
	}
}
