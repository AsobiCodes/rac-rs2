//! Movies in the R&C2 window (engine split stage E6): the game's movie player waits in its hook (rac2/movie.rs) while the window
//! plays the movie with the engine's player (engine/fmv: the disc stream, MPEG-2 video, the language's ADPCM track on a sound
//! stream of the window's own) over the whole window (the engine's movie layer, `Gpu::movie`, black around the picture) with its
//! subtitle lines (rac2/movie_subs.rs) in the engine's UI pass, polls
//! retail's skip rule (`movie::Skipper`) with the window's pad each redraw, and lets the game go on with "played" or "skipped".
//! Window shots (lockstep) skip every movie at once as before (the replays were recorded so) unless RAC_FMV=1: then the movie
//! plays on the wall clock and every 25th frame of it is saved as DIR/mI_NNNNN.png (I = the run's movie number).

use super::App;
use crate::movie::{self, Skipper};
use rac_engine::fmv::player::Player;
use std::sync::atomic::Ordering;

pub(super) struct Playing {
	player: Player,
	skipper: Skipper,
	/// The run's movie number (window shots name its frames by it) and its frames shown so far.
	index: u32,
	frames: u64,
	/// The movie's subtitle lines (rac2/movie_subs.rs), drawn by the engine's UI pass over the movie layer.
	subs: Option<crate::movie_subs::Script>,
}

/// The window plays the game's movies: always, unless RAC_FMV=0; in window shots only with RAC_FMV=1.
pub(super) fn serviced(lockstep: bool) -> bool {
	!lockstep || std::env::var("RAC_FMV").is_ok_and(|v| v == "1")
}

/// Debug (replays): RAC2_FMV_SKIP_AT=N presses Start (with the shoulder buttons, which retail's story rule wants too) once the
/// movie has shown N frames, as a player does. A lockstep replay runs no field during a movie, so it cannot press a button.
fn skip_at() -> Option<u64> {
	static N: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
	*N.get_or_init(|| std::env::var("RAC2_FMV_SKIP_AT").ok().and_then(|v| v.trim().parse().ok()))
}

/// Debug (skipcut proof): RAC2_FMV_START_AT=N presses Start ALONE from movie frame N on, as a player pressing only Start.
fn start_at() -> Option<u64> {
	static N: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
	*N.get_or_init(|| std::env::var("RAC2_FMV_START_AT").ok().and_then(|v| v.trim().parse().ok()))
}

impl App {
	/// Before the redraw's draw: starts the movie the game asked for, uploads its frame that is due and ends it when it played
	/// out or the pad skipped it. True while a movie is up (the engine draws it over the frame).
	pub(super) fn movie_step(&mut self) -> bool {
		let Some(g) = self.gpu.as_mut() else { return false };
		if let Some(r) = movie::take_request() {
			if self.movie_audio.is_none() && self.shots.is_none() && std::env::var_os("RAC2_NOSOUND").is_none() {
				self.movie_audio = rac_engine::audio::Audio::new();
			}
			eprintln!("rac2-play: movie at sector {} ({} bytes, track {}, skip {:?})", r.lba, r.bytes, r.track, r.skip);
			self.playing = Some(Playing { player: Player::start(&self.iso, &r.movie(), r.lang(), self.movie_audio.as_mut()), skipper: Skipper::new(r.skip), index: movie::counts().0, frames: 0, subs: movie::take_script() });
		}
		let Some(p) = self.playing.as_mut() else { return false };
		let mut shown = false;
		if let Some(f) = p.player.poll() {
			g.movie.upload(&g.device, &g.queue, &f);
			p.frames += 1;
			shown = true;
		}
		let mut held = (self.shared.pad.load(Ordering::Relaxed) >> 32) as u16;
		if skip_at().is_some_and(|n| p.frames >= n) {
			held |= 0x80f;
		}
		if start_at().is_some_and(|n| p.frames >= n) {
			held |= 0x800;
		}
		// [PC] Start skips cutscenes (R&C1's row, `skipcut`, default On): Start stands for Start + L1 R1 L2 R2, the combo retail wants
		// for the story movies after the first planet (movie.rs `Skip::Combo`); Off = retail's rule.
		if crate::options::get().skipcut_on() && held & 0x800 != 0 {
			held |= 0xf;
		}
		let skip = p.skipper.poll(held);
		if skip || p.player.finished {
			eprintln!("rac2-play: movie {} after {} frames", if skip { "skipped" } else { "ended" }, p.frames);
			self.playing = None;
			g.movie.clear();
			movie::finish(skip);
			return false;
		}
		// The subtitle lines of this picture's display-frame count (the UI scene is valid for one draw: set again for the window's).
		let rate = rac_engine::field::field_rate();
		let subs = p.subs.as_ref().map(|s| s.prims(crate::movie_subs::count_of(p.frames, rate))).filter(|v| !v.is_empty());
		// Shot runs also save the frame in the middle of every subtitle line's window (a proof sees each line once).
		let mid = shown && p.subs.as_ref().is_some_and(|s| s.mid_in(crate::movie_subs::count_of(p.frames.saturating_sub(1), rate), crate::movie_subs::count_of(p.frames, rate)));
		if let (Some(s), true) = (self.shots.as_mut(), shown && (p.frames % 25 == 1 || mid)) {
			let (i, n) = (p.index, p.frames);
			if let Some(v) = subs.clone() {
				g.ui_gpu.set_scene(v);
			}
			if let Some(img) = g.draw(&super::frame::IDLE, true) {
				s.save_movie(i, n, &img);
			}
		}
		if let Some(v) = subs {
			g.ui_gpu.set_scene(v);
		}
		true
	}
}
