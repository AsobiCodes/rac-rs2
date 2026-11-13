//! [PC] Session recordings that replay exactly (lane rcd, build 44). pad.bin (host/game.rs `record`) keeps the pad the game got and
//! cam.bin (modcam_rec.rs) the Modern camera's input, but the PC side also changes the game's state OUTSIDE the pad: the mouse's
//! cursor writes on the modern pages (modern_jump.rs), the modern shop's Buy all (modern_vendor_hand.rs: a request a hand takes
//! inside the field), the debug keys (pc_debug.rs: F6 ammo, F7 god, F8 bolts, F3 / F4 warp), the Subtitles row (subs_sync.rs), the
//! button prompts' device (pc_prompts.rs: the keyboard's texts live in game RAM) and the window's movie verdicts (movie.rs: played to
//! the end or skipped, and the vblank the window's player takes). Each goes to pc.bin next to pad.bin, per field, in the order made.
//!
//! A replay of a recording WITH pc.bin (`replaying`) takes them from the file at the same point of the field instead of from the
//! window, and its recorded pad is final: host/game.rs runs the modern pages' filters and `pc_input::Ctl` for their own state only
//! (what the window draws, the prompts' menu keys) and drops the pad they make. A recording without pc.bin replays as before (it
//! cannot gain the PC actions it never recorded). Runs that neither record nor replay one: nothing here does anything.
//!
//! pc.bin = `MAGIC`, then records `field u32 LE, kind u8, len u8, payload[len]` (field = the game thread's field index, 0-based):
//! kind 1 / 2 / 3 = a RAM write at stage Jump / Debug / After: addr u32 LE + the bytes written (1..=8); kind 4 = Buy all pending as
//! the field runs (no payload); kind 5 = the prompts' device from this field on (1 byte, 1 = keyboard); kind 6 = a movie the window
//! played (1 byte: retail's v0, 0 = played to the end, 1 = skipped by the pad). Unknown kinds are skipped, a cut last record (a
//! killed session) is dropped.
use std::io::Write;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;

pub const MAGIC: [u8; 8] = *b"RC2PC\x01\0\0";

/// Where in the field a recorded RAM write is made (and replayed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
	/// Before the field, after the modern pages' filters (modern_jump.rs `apply`).
	Jump,
	/// Before the field, the debug keys (pc_debug.rs `pre_field`).
	Debug,
	/// After the field (subs_sync.rs `watch`).
	After,
}

/// One PC action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ev {
	Write(Stage, u32, Vec<u8>),
	Vendor,
	Device(bool),
	Movie(u8),
}

impl Ev {
	fn kind(&self) -> u8 {
		match self {
			Ev::Write(Stage::Jump, ..) => 1,
			Ev::Write(Stage::Debug, ..) => 2,
			Ev::Write(Stage::After, ..) => 3,
			Ev::Vendor => 4,
			Ev::Device(_) => 5,
			Ev::Movie(_) => 6,
		}
	}

	fn payload(&self) -> Vec<u8> {
		match self {
			Ev::Write(_, a, b) => a.to_le_bytes().iter().chain(b.iter()).copied().collect(),
			Ev::Vendor => Vec::new(),
			Ev::Device(k) => vec![*k as u8],
			Ev::Movie(v) => vec![*v],
		}
	}

	fn parse(kind: u8, p: &[u8]) -> Option<Ev> {
		let w = |s| (5..=12).contains(&p.len()).then(|| Ev::Write(s, u32::from_le_bytes([p[0], p[1], p[2], p[3]]), p[4..].to_vec()));
		match kind {
			1 => w(Stage::Jump),
			2 => w(Stage::Debug),
			3 => w(Stage::After),
			4 => p.is_empty().then_some(Ev::Vendor),
			5 => (p.len() == 1).then(|| Ev::Device(p[0] != 0)),
			6 => (p.len() == 1).then(|| Ev::Movie(p[0])),
			_ => None,
		}
	}
}

/// One record of pc.bin. Pure.
pub fn encode(field: u32, e: &Ev) -> Vec<u8> {
	let p = e.payload();
	let mut v = field.to_le_bytes().to_vec();
	v.extend_from_slice(&[e.kind(), p.len() as u8]);
	v.extend_from_slice(&p);
	v
}

/// A whole pc.bin in field order; None = no `MAGIC` (not a pc.bin). Pure.
pub fn decode(b: &[u8]) -> Option<Vec<(u32, Ev)>> {
	let mut rest = b.strip_prefix(&MAGIC[..])?;
	let mut out = Vec::new();
	while rest.len() >= 6 {
		let (f, kind, n) = (u32::from_le_bytes([rest[0], rest[1], rest[2], rest[3]]), rest[4], rest[5] as usize);
		let Some(p) = rest.get(6..6 + n) else { break };
		out.extend(Ev::parse(kind, p).map(|e| (f, e)));
		rest = &rest[6 + n..];
	}
	out.sort_by_key(|e| e.0);
	Some(out)
}

/// A replay's events and the field it is in (fields only go up).
#[derive(Default)]
pub struct Play {
	ev: Vec<(u32, Ev)>,
	lo: usize,
	hi: usize,
	movie: usize,
	dev: bool,
}

impl Play {
	pub fn new(ev: Vec<(u32, Ev)>) -> Play {
		Play { ev, ..Play::default() }
	}

	/// Field `f` starts: its events; the device of every event up to it (the last wins).
	pub fn begin(&mut self, f: u32) {
		let n = self.ev.len();
		while self.lo < n && self.ev[self.lo].0 < f {
			self.lo += 1;
		}
		self.hi = self.lo;
		while self.hi < n && self.ev[self.hi].0 == f {
			self.hi += 1;
		}
		self.movie = self.lo;
		if let Some(k) = self.ev[..self.hi].iter().rev().find_map(|e| if let Ev::Device(k) = e.1 { Some(k) } else { None }) {
			self.dev = k;
		}
	}

	fn now(&self) -> &[(u32, Ev)] {
		&self.ev[self.lo..self.hi]
	}

	/// The RAM writes of this field at `s`, in the recorded order.
	pub fn writes(&self, s: Stage) -> Vec<(u32, Vec<u8>)> {
		self.now().iter().filter_map(|e| if let Ev::Write(t, a, b) = &e.1 { (*t == s).then(|| (*a, b.clone())) } else { None }).collect()
	}

	pub fn vendor(&self) -> bool {
		self.now().iter().any(|e| e.1 == Ev::Vendor)
	}

	pub fn device(&self) -> bool {
		self.dev
	}

	/// The next movie verdict of this field (None: the recording had no played movie left here).
	pub fn movie_next(&mut self) -> Option<u8> {
		while self.movie < self.hi {
			self.movie += 1;
			if let Ev::Movie(v) = self.ev[self.movie - 1].1 {
				return Some(v);
			}
		}
		None
	}
}

const OFF: u8 = 0;
const REC: u8 = 1;
const PLAY: u8 = 2;
static MODE: AtomicU8 = AtomicU8::new(OFF);

enum St {
	Off,
	Rec { w: std::io::BufWriter<std::fs::File>, field: u32, dev: bool },
	Play(Play),
}

static ST: Mutex<St> = Mutex::new(St::Off);

fn st() -> std::sync::MutexGuard<'static, St> {
	ST.lock().unwrap_or_else(|e| e.into_inner())
}

/// [tests] RAC2_REC_LOCKSTEP=1: a lockstep window-shots run that is no replay records its session like a played one (player
/// dir/recordings, lib.rs), so a feed run with mouse clicks makes a recording to replay.
pub fn rec_lockstep() -> bool {
	static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
	*ON.get_or_init(|| std::env::var("RAC2_REC_LOCKSTEP").is_ok_and(|v| v == "1"))
}

/// A recorded session (host/game.rs): pc.bin in the recording folder.
pub fn start(dir: &std::path::Path) {
	let Ok(mut f) = std::fs::File::create(dir.join("pc.bin")) else { return };
	if f.write_all(&MAGIC).is_ok() {
		*st() = St::Rec { w: std::io::BufWriter::new(f), field: 0, dev: false };
		MODE.store(REC, Ordering::Relaxed);
	}
}

/// `--replay REC` in the window (host.rs): REC/pc.bin, if the recording has one; the number of actions (None: an old recording).
pub fn load(rec: &str) -> Option<usize> {
	let ev = decode(&std::fs::read(format!("{rec}/pc.bin")).ok()?)?;
	let n = ev.len();
	*st() = St::Play(Play::new(ev));
	MODE.store(PLAY, Ordering::Relaxed);
	Some(n)
}

/// A replay of a recording with pc.bin runs.
pub fn replaying() -> bool {
	MODE.load(Ordering::Relaxed) == PLAY
}

/// Game thread, at the start of each field (before the pad).
pub fn begin_field(field: u64) {
	if MODE.load(Ordering::Relaxed) == OFF {
		return;
	}
	match &mut *st() {
		St::Rec { field: f, .. } => *f = field as u32,
		St::Play(p) => p.begin(field as u32),
		St::Off => {}
	}
}

/// A recorded session: `e` to pc.bin (nothing otherwise).
pub fn note(e: Ev) {
	if MODE.load(Ordering::Relaxed) != REC {
		return;
	}
	if let St::Rec { w, field, .. } = &mut *st() {
		let _ = w.write_all(&encode(*field, &e));
	}
}

/// A PC-side RAM write just made at `s`.
pub fn note_write(s: Stage, addr: u32, bytes: &[u8]) {
	note(Ev::Write(s, addr, bytes.to_vec()));
}

/// The prompts' device of this field (noted when it changed).
pub fn note_device(kb: bool) {
	if MODE.load(Ordering::Relaxed) != REC {
		return;
	}
	if let St::Rec { w, field, dev } = &mut *st() {
		if *dev != kb {
			*dev = kb;
			let _ = w.write_all(&encode(*field, &Ev::Device(kb)));
		}
	}
}

/// Every 250 fields and at the end of the session.
pub fn flush() {
	if let St::Rec { w, .. } = &mut *st() {
		let _ = w.flush();
	}
}

fn with_play<T>(f: impl FnOnce(&mut Play) -> T) -> Option<T> {
	if !replaying() {
		return None;
	}
	match &mut *st() {
		St::Play(p) => Some(f(p)),
		_ => None,
	}
}

/// A replay: this field's recorded writes at `s` into RAM as a slice (modern_jump.rs writes so).
pub fn apply_ram(s: Stage, ram: &mut [u8]) {
	for (a, b) in with_play(|p| p.writes(s)).unwrap_or_default() {
		if let Some(d) = ram.get_mut(a as usize..a as usize + b.len()) {
			d.copy_from_slice(&b);
		}
	}
}

/// A replay: this field's recorded writes at `s` through the memory's own stores (pc_debug.rs / subs_sync.rs write so).
pub fn apply_mem(s: Stage, mem: &mut ee_oracle::mem::Mem) {
	for (a, b) in with_play(|p| p.writes(s)).unwrap_or_default() {
		match b.len() {
			4 => mem.wr32(a, u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
			_ => b.iter().enumerate().for_each(|(i, &x)| mem.wr8(a + i as u32, x)),
		}
	}
}

/// A replay: whether Buy all is pending as this field runs (None: no replay with pc.bin).
pub fn vendor_now() -> Option<bool> {
	with_play(|p| p.vendor())
}

/// A replay: the prompts' device of this field (None: no replay with pc.bin).
pub fn device_now() -> Option<bool> {
	with_play(|p| p.device())
}

/// A replay: the next movie verdict of this field, Some(None) = skipped at once (None: no replay with pc.bin).
pub fn movie_next() -> Option<Option<u8>> {
	with_play(|p| p.movie_next())
}

#[cfg(test)]
#[path = "pc_rec_tests.rs"]
mod tests;
