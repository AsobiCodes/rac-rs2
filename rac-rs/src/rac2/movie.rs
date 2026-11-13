//! Native movies for R&C2 (engine split stage E6): the retail movie player is replaced by a window-serviced request.
//!
//! The retail player is FUN_00351140(a0 lba, a1 bytes, a2 buffer A, a3 buffer B, t0 audio track, t1 flag) in the boot .text
//! [C]. It drives the IPU + an MPEG decode thread (0x353c68) the oracle has no scheduler for, so a Hand replaces it. Callers
//! [C]: FUN_002f6f08 (movie list: boot logos id 0xb2, attract list, story movies) and FUN_002ecd08 (planet-end movies).
//! Arguments [C]: a0 = ABSOLUTE disc sector (RC2.HDR MPEG base 2737 + the table's sector offset), a1 = stream bytes,
//! a2/a3 = decode buffers (unused here), t0 = PSS audio track byte (0 English, 2 French, 3 German, 4 Spanish, 5 Italian = the
//! game's language word 0x1a7c3c; 0 forced for the logo/attract lists), t1 = 1 only for movie id 0xbf (table entry 46, 4:3).
//! Return v0 [C]: 0 = played to the end, 1 = skipped by the pad (the caller then ends its list); an open failure returns 0.
//! Data [C]: RC2.HDR (LBA 0x3e9, loaded to 0x14b5c0) MPEG section = 50 records of 16 bytes at 0x14b5c8: {sector, bytes} of a
//! small subtitle script (entries 0..32 only) then {sector, bytes} of the PSS; PSS = MPEG-2 512x416 25 fps, private stream 1
//! audio `FF A1 00 <track>`, tracks 0,2,3,4,5, PS2 ADPCM 48 kHz stereo, interleave 0x20 (the engine's fmv decoder handles it).
//! Every level overlay (all 27) carries its own copy of the same head (Aranos 0x44a310, ...): `movie_overlay.rs` hooks them with this
//! same `movie_call` (the end of Aranos waited forever for the copy's MPEG thread before that).
//! Subtitles: the script at 0x1b888c is read in the hook (`subs_of`, movie_subs.rs) and the window draws its lines over the movie.
//!
//! Wiring (rac2/host.rs, by the window's owner): `register()` (also called by `rac2::register`), `set_serviced(true)` once the
//! window can play movies (else every movie is skipped at once, as the old `ret 351140 1` rule did), `take_request()` after each
//! field, play it with `fmv::player::Player::start` and stop stepping fields (`wait_idle` on the game thread); `finish(skipped)` when it ended or the pad skipped
//! it; `abort()` before the machine is dropped (a game thread waiting in the hook would never exit).
use ee_oracle::cpu::Cpu;
use std::sync::{Condvar, Mutex};

/// The player in the boot ELF: entry, end and the FNV hash of its code words (`ee_oracle::rec::hash`). [C] from SCES_516.07.
pub const ENTRY: u32 = 0x351140;
pub const END: u32 = 0x35122c;
pub const HASH: u64 = 0x8a27_4045_f9b9_034f;

/// What ends a movie early. Retail's loop (FUN_00351230) reads mode word 0x1a74f8, 0x1a7a90, planet word 0x1a7a78 and the pad
/// (0x1383a0 held, 0x1383a4 newly pressed). [C]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skip {
	/// Mode 3 (boot logos): nothing skips.
	Never,
	/// Mode 2 (attract list): a button held while none is newly pressed.
	Any,
	/// Mode 0 past the first planet with 0x1a7a90 high half 0: Start newly pressed while L1+L2+R1+R2 are held.
	Combo,
	/// Everything else: Start newly pressed.
	Start,
}

impl Skip {
	pub fn from_words(mode: i32, dbg: u32, planet: i32) -> Skip {
		match mode {
			3 => Skip::Never,
			2 => Skip::Any,
			0 if planet > 0 && dbg & 0xffff_0000 == 0 => Skip::Combo,
			_ => Skip::Start,
		}
	}
}

/// Retail's skip test for one poll of the pad: all rules need the pad released once first.
pub struct Skipper {
	rule: Skip,
	released: bool,
	prev: u16,
}

impl Skipper {
	pub fn new(rule: Skip) -> Skipper {
		Skipper { rule, released: false, prev: 0 }
	}

	/// `now` = buttons held this poll (PS2 bits: Start 0x800, L2 1, R2 2, L1 4, R1 8). True = skip the movie.
	pub fn poll(&mut self, now: u16) -> bool {
		let new = now & !self.prev;
		self.prev = now;
		if now == 0 {
			self.released = true;
		}
		if !self.released {
			return false;
		}
		match self.rule {
			Skip::Never => false,
			Skip::Any => now != 0 && new == 0,
			Skip::Start => new & 0x800 != 0,
			Skip::Combo => new & 0x800 != 0 && now & 0xf == 0xf,
		}
	}
}

/// One movie the game asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
	/// First disc sector (2048 bytes) of the PSS.
	pub lba: u32,
	pub bytes: u32,
	/// PSS audio track byte (0, 2, 3, 4, 5).
	pub track: u8,
	/// t1: 1 for the 4:3 movie (id 0xbf).
	pub flag: u32,
	pub skip: Skip,
	/// Guest address of the subtitle script (0x1b888c), 0 = none.
	pub subs: u32,
}

impl Request {
	/// The engine player's front-end language index (`fmv::pss::track_for_language` inverse).
	pub fn lang(&self) -> u8 {
		[0u8, 0, 1, 2, 3, 4].get(self.track as usize).copied().unwrap_or(0)
	}

	pub fn movie(&self) -> rac_engine::fmv::Movie {
		rac_engine::fmv::Movie { lba: self.lba, bytes: self.bytes as u64, fps: 25.0 }
	}
}

#[derive(Default)]
struct State {
	serviced: bool,
	abort: bool,
	req: Option<Request>,
	playing: bool,
	/// The window's verdict for v0 (true = skipped).
	skipped_by_pad: bool,
	played: u32,
	skipped: u32,
	/// The movie's subtitle lines (movie_subs.rs), read in the hook with the request.
	script: Option<crate::movie_subs::Script>,
}

static ST: Mutex<State> = Mutex::new(State { serviced: false, abort: false, req: None, playing: false, skipped_by_pad: false, played: 0, skipped: 0, script: None });
static CV: Condvar = Condvar::new();

fn st() -> std::sync::MutexGuard<'static, State> {
	ST.lock().unwrap_or_else(|e| e.into_inner())
}

static HOOKS: [ee_oracle::rec::Hand; 1] = [ee_oracle::rec::Hand { entry: ENTRY, end: END, hash: HASH, est: 0, f: movie_call }];

/// Registers the hooks with the oracle's table (idempotent): the boot copy of the player here, the copies in the level overlays
/// (movie_overlay.rs, a relocatable hand).
pub fn register() {
	ee_oracle::rec::register_hand(&HOOKS);
	crate::movie_overlay::register();
}

/// A new game start: clears an `abort` and any stale request.
pub fn reset() {
	let mut s = st();
	s.abort = false;
	s.playing = false;
	s.req = None;
}

/// The window can play movies (and will call `finish`). RAC_FMV=0 turns it off (every movie is skipped).
pub fn set_serviced(on: bool) {
	let off = std::env::var("RAC_FMV").is_ok_and(|v| v == "0");
	st().serviced = on && !off;
}

/// A movie the game is waiting for, once.
pub fn take_request() -> Option<Request> {
	st().req.take()
}

/// The subtitle lines of the movie `take_request` handed out (once; None: no script).
pub fn take_script() -> Option<crate::movie_subs::Script> {
	st().script.take()
}

/// A movie is playing (the game's picture is the movie's: no frozen-screen handling then, host/game.rs).
pub fn playing() -> bool {
	ST.lock().map_or(false, |s| s.playing)
}

/// True while the game thread waits in the hook (the window must keep presenting movie frames and not step fields).
pub fn waiting() -> bool {
	st().playing
}

/// The game thread, before it steps a field: while the game waits in the hook, wait here until the movie ended (a field stepped
/// now would block in the hook for the whole movie and trip the oracle's 20 s vblank watchdog, `Machine::frame`). False when
/// `quit` was set meanwhile.
pub fn wait_idle(quit: &std::sync::atomic::AtomicBool) -> bool {
	let mut s = st();
	while s.playing && !s.abort {
		if quit.load(std::sync::atomic::Ordering::Relaxed) {
			return false;
		}
		s = CV.wait_timeout(s, std::time::Duration::from_millis(100)).map_or_else(|e| e.into_inner().0, |r| r.0);
	}
	true
}

/// The movie ended (`skipped` false) or the pad skipped it (true): the game thread returns from the player.
pub fn finish(skipped: bool) {
	let mut s = st();
	s.playing = false;
	s.req = None;
	s.skipped_by_pad = skipped;
	s.played += 1;
	CV.notify_all();
}

/// The window is going away: release the game thread and skip every later movie.
pub fn abort() {
	let mut s = st();
	s.abort = true;
	s.playing = false;
	s.req = None;
	s.skipped_by_pad = true;
	CV.notify_all();
}

/// (played through the window, skipped without a window), for tests.
pub fn counts() -> (u32, u32) {
	let s = st();
	(s.played, s.skipped)
}

/// Debug (replays): RAC2_FMV_FROM=N lets the window play only the movies the game asks for from field N on; earlier ones are
/// skipped at once as without a window. A lockstep replay cannot press a button during a movie (no field runs then), so with
/// RAC_FMV=1 the title's attract list would play for ever; this plays just the movie under test. Unset = 0 = no limit.
fn fmv_from() -> u64 {
	static N: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
	*N.get_or_init(|| std::env::var("RAC2_FMV_FROM").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(0))
}

fn queue(r: Request, script: Option<crate::movie_subs::Script>) -> bool {
	let mut s = st();
	if !s.serviced || s.abort || ee_oracle::cold::FIELD.load(std::sync::atomic::Ordering::Relaxed) < fmv_from() {
		s.skipped += 1;
		return false;
	}
	s.req = Some(r);
	s.script = script;
	s.playing = true;
	true
}

fn wait_finish() -> bool {
	let mut s = st();
	while s.playing && !s.abort {
		s = CV.wait(s).unwrap_or_else(|e| e.into_inner());
	}
	s.skipped_by_pad
}

/// Debug (skipcut proof): RAC2_FMV_RULE=combo|start|any|never gives every movie that skip rule (no story-movie savestate in the bank).
fn rule_override() -> Option<Skip> {
	static R: std::sync::OnceLock<Option<Skip>> = std::sync::OnceLock::new();
	*R.get_or_init(|| match std::env::var("RAC2_FMV_RULE").ok()?.trim() {
		"combo" => Some(Skip::Combo),
		"start" => Some(Skip::Start),
		"any" => Some(Skip::Any),
		"never" => Some(Skip::Never),
		_ => None,
	})
}

/// The hook: v0 = 1 when no window services movies (skipped at once, nothing added to the run) or the pad skipped it, 0 when it
/// played to the end. While a window plays the movie the game thread first completes the current field through the vblank
/// callback (as a real PS2 keeps taking vblanks in its player), then waits for `finish`.
pub fn movie_call(c: &mut Cpu) -> bool {
	crate::scan_timing::timed("hook_movie_call", || movie_call_in(c))
}

fn movie_call_in(c: &mut Cpu) -> bool {
	let skip = rule_override().unwrap_or_else(|| Skip::from_words(c.mem.rd32(0x1a74f8) as i32, c.mem.rd32(0x1a7a90), c.mem.rd32(0x1a7a78) as i32));
	if intro_skipped(skip, crate::options::intro_skip()) {
		// [PC] Skip intro (`intro=skip`, parity row 58): the boot logos report "skipped" (v0 1) without playing, as when no window
		// services movies; the game then goes on to the title as after a skip.
		c.s(2, 1);
		return true;
	}
	// [PC] A replay with pc.bin (pc_rec.rs): the window's verdict as the session had it; a movie its window played takes that
	// vblank here too. No verdict recorded: skipped at once, as without a window.
	if let Some(rec) = crate::pc_rec::movie_next() {
		let v0 = rec.map_or(1, |v| {
			window_vblank(c);
			v as u64
		});
		c.s(2, v0);
		return true;
	}
	let r = Request { lba: c.g(4) as u32, bytes: c.g(5) as u32, track: c.g(8) as u8, flag: c.g(9) as u32, skip, subs: c.mem.rd32(0x1b888c) };
	let mut v0 = 1;
	if std::env::var_os("RAC2_LOG").is_some() {
		eprintln!("rac2-play: movie hook: field {} entry {:08x} ra {:08x} sp {:08x} lba {} serviced {} rust stack at {:p}", ee_oracle::cold::FIELD.load(std::sync::atomic::Ordering::Relaxed), c.pc, c.g(31), c.g(29), r.lba, st().serviced, &v0);
	}
	if queue(r, subs_of(c, r.subs)) {
		window_vblank(c);
		v0 = wait_finish() as u64;
		crate::pc_rec::note(crate::pc_rec::Ev::Movie(v0 as u8)); // [PC] the verdict, for a replay (pc_rec.rs)
	}
	c.s(2, v0);
	true
}

/// While the window plays a movie, the game thread first completes the current field through the vblank callback.
fn window_vblank(c: &mut Cpu) {
	if let Some(mut f) = c.vblank_cb.take() {
		f(c);
		if c.vblank_cb.is_none() {
			c.vblank_cb = Some(f);
		}
	}
}

/// The movie's subtitle lines in the game's language with its Subtitles byte (movie_subs.rs); None without a script.
fn subs_of(c: &mut Cpu, base: u32) -> Option<crate::movie_subs::Script> {
	use crate::addrs::{LANG, SCREEN_H, SUBS_ON, WIDE};
	if base == 0 {
		return None;
	}
	let (lang, on, wide) = (c.mem.rd8(LANG) as u32, c.mem.rd8(SUBS_ON) != 0, c.mem.rd8(WIDE) != 0);
	let screen_h = c.mem.rd32(SCREEN_H) as i32;
	let s = crate::movie_subs::read(&c.mem.ram, base, lang, on, wide, screen_h);
	eprintln!("rac2-play: movie subtitles: {} lines (language word {lang}, Subtitles {})", s.lines.len(), if on { "on" } else { "off" });
	// Every line's display-frame window (count from the movie's start; movie frame ~ count * 25 / field rate + 1).
	for l in &s.lines {
		eprintln!("rac2-play:   subtitle {}..{} {:?} {:?}", l.start, l.end, l.kind, l.text.chars().take(48).collect::<String>());
	}
	Some(s)
}

/// [PC] Skip intro: only the boot logos (retail's mode 3, which nothing skips) and only with `intro=skip`.
fn intro_skipped(skip: Skip, intro_skip: bool) -> bool {
	intro_skip && skip == Skip::Never
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn skip_intro_takes_only_the_boot_logos() {
		assert!(intro_skipped(Skip::from_words(3, 0, 0), true));
		assert!(!intro_skipped(Skip::from_words(3, 0, 0), false), "intro=play (no key): retail's logos");
		for mode in [0, 1, 2] {
			assert!(!intro_skipped(Skip::from_words(mode, 0, 1), true), "story and attract movies keep retail's rules");
		}
	}

	/// Retail's skip rules: logos never, attract = a held button once the pad was released, story = Start pressed (+ shoulders).
	#[test]
	fn skip_rules() {
		assert_eq!(Skip::from_words(3, 0, 0), Skip::Never);
		assert_eq!(Skip::from_words(2, 0, 0), Skip::Any);
		assert_eq!(Skip::from_words(0, 0, 1), Skip::Combo);
		assert_eq!(Skip::from_words(0, 0, 0), Skip::Start);
		assert_eq!(Skip::from_words(1, 0, 1), Skip::Start);
		let mut s = Skipper::new(Skip::Start);
		assert!(!s.poll(0x800), "Start held from before the movie");
		assert!(!s.poll(0));
		assert!(s.poll(0x800));
		let mut a = Skipper::new(Skip::Any);
		assert!(!a.poll(0x40) && !a.poll(0));
		assert!(!a.poll(0x40), "first poll of a press is new");
		assert!(a.poll(0x40), "held on the next");
		let mut c = Skipper::new(Skip::Combo);
		assert!(!c.poll(0) && !c.poll(0x800));
		assert!(!c.poll(0) && c.poll(0x80f));
	}

	/// The game thread holds its next field while a movie plays and goes on when it ended; a quitting window releases it.
	#[test]
	fn wait_idle_holds_the_field_until_the_movie_ends() {
		use std::sync::atomic::{AtomicBool, Ordering};
		let quit = AtomicBool::new(false);
		reset();
		assert!(wait_idle(&quit), "no movie: at once");
		st().playing = true;
		let t = std::thread::spawn(|| {
			std::thread::sleep(std::time::Duration::from_millis(150));
			finish(false);
		});
		let t0 = std::time::Instant::now();
		assert!(wait_idle(&quit));
		assert!(t0.elapsed() >= std::time::Duration::from_millis(100), "waited for the end");
		t.join().unwrap();
		st().playing = true;
		quit.store(true, Ordering::Relaxed);
		assert!(!wait_idle(&quit), "quit releases the game thread");
		reset();
	}

	/// The retail words of the boot copy of the player (FUN_00351140, 59 words from the boot ELF `SCES_516.07`): `HASH` is their FNV-1a
	/// and `ENTRY..END` is exactly their length.
	const PLAYER: [u32; 59] = [
		0x27bdffd0, 0x3c01001b, 0xac2623c8, 0xffb20010, 0xffb30018, 0x0080902d,
		0x00a0982d, 0xffb00000, 0xffb10008, 0x3c04001b, 0x3c050038, 0xffb40020,
		0x0100882d, 0x0120802d, 0xffbf0028, 0x2484e858, 0x3c01001b, 0xac2723cc,
		0x0c09bf4a, 0x34a5cac0, 0x0000a02d, 0x0200202d, 0x0c0a1252, 0x0080282d,
		0x0c046ac4, 0x00000000, 0x0040202d, 0x0c046aac, 0x24050001, 0x0240202d,
		0x0260282d, 0x0c0d457c, 0x0220302d, 0x1040000a, 0x8f84335c, 0x3c02000d,
		0x3c06000d, 0x34429048, 0x0080282d, 0x34c69040, 0x00822021, 0x0c0d448c,
		0x00a63021, 0x0040a02d, 0x0c0d461e, 0x00000000, 0x0280102d, 0xdfb00000,
		0xdfb10008, 0xdfb20010, 0xdfb30018, 0xdfb40020, 0xdfbf0028, 0x3c01001b,
		0xac2023c8, 0x3c01001b, 0xac2023cc, 0x03e00008, 0x27bd0030,
	];

	#[test]
	fn hash_constant_is_the_fnv_of_the_player_words() {
		assert_eq!(END - ENTRY, PLAYER.len() as u32 * 4);
		let h = PLAYER.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &w| (h ^ w as u64).wrapping_mul(0x100_0000_01b3));
		assert_eq!(h, HASH);
	}
}
