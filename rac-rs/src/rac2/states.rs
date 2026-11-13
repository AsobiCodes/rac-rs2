//! Savestates for rac2-play (round 3 batch 10): start from a state dir, quick save (F5) and quick load (F9).
//!
//! RAC2_STATE=<dir> starts the game from a PCSX2 savestate dir (`python oracle/p2s.py X.p2s DIR`: eeMemory.bin, iopMemory.bin,
//! regs.bin, GS.bin ...) instead of the cold boot; `RAC2_STATE=last` takes the newest quick save. The machine is the oracle's savestate
//! machine (`Machine::load_resume`: the game's kernel and IOP state live in the state's RAM; the oracle answers the IOP's RPCs), so a
//! state is also saved by a session that started from the disc (dyn.bin carries the cold-boot kernel, batch 15). F5 saves the machine at the end of the current field into
//! `<player dir>/states/q<unix millis>_<field>` in the same format (GS local memory and VU1 from the GS pipe after that field, `ovl_snap.rs`) on a
//! thread of its own; the newest 3 are kept. F9 starts a new process from the newest one. dyn.bin (oracle statedyn.rs) holds what the machine keeps outside RAM: cold kernel, field timer, sound driver model, RPC binds, card server.
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// The writer thread of the save in flight (one at a time: a second F5 meanwhile is refused, see `save`).
static WRITER: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);

/// The newest quick save outcome the player should see (queued / waiting for the card / saved / failed) and when it was said.
static NOTE: Mutex<Option<(String, std::time::Instant)>> = Mutex::new(None);

/// Seconds a quick save message stays in the window title.
const NOTE_SECS: u64 = 6;

/// Says `msg` in the log and in the window title for a few seconds (host.rs `RedrawRequested`): the F5 outcomes must be visible, the
/// player has no console.
pub fn note(msg: impl Into<String>) {
	let msg = msg.into();
	eprintln!("rac2-play: {msg}");
	if let Ok(mut n) = NOTE.lock() {
		*n = Some((msg, std::time::Instant::now()));
	}
}

/// The quick save message still to show in the window title (None after `NOTE_SECS`).
pub fn note_now() -> Option<String> {
	NOTE.lock().ok()?.as_ref().filter(|(_, t)| t.elapsed().as_secs() < NOTE_SECS).map(|(m, _)| m.clone())
}

/// The sort key of a quick save dir name `q<unix millis>_<field>` (an older `q<unix seconds>` sorts before every new one).
fn stamp(name: &str) -> Option<u64> {
	let t = name.strip_prefix('q')?;
	match t.split_once('_') {
		Some((m, f)) => f.parse::<u64>().ok().and(m.parse().ok()),
		None => t.parse().ok(),
	}
}

/// True while a save is being written (its thread has not ended).
fn saving() -> bool {
	WRITER.lock().is_ok_and(|w| w.as_ref().is_some_and(|h| !h.is_finished()))
}

/// Waits for the save in flight (the writer waits at most 10 s for the GS pipe, then writes a few files): F9 and the end of the
/// run call this so the newest save is complete on disk. Nothing to wait for: returns at once.
pub fn wait_for_save() {
	let h = WRITER.lock().ok().and_then(|mut w| w.take());
	if let Some(h) = h {
		eprintln!("rac2-play: quick save: waiting for the save in flight");
		let _ = h.join();
	}
}

/// `<player dir>/states`: the quick saves.
pub fn states_dir() -> PathBuf {
	rac_engine::pcopts::player_dir().join("states")
}

/// The newest complete quick save (`q<unix millis>_<field>` with a regs.bin), if any.
pub fn newest() -> Option<PathBuf> {
	let mut best: Option<(u64, PathBuf)> = None;
	for e in std::fs::read_dir(states_dir()).ok()?.flatten() {
		let n = e.file_name().to_string_lossy().into_owned();
		let Some(t) = stamp(&n) else { continue };
		if missing_file(&e.path()).is_none() && best.as_ref().is_none_or(|b| t > b.0) {
			best = Some((t, e.path()));
		}
	}
	best.map(|b| b.1)
}

/// The state of a `--replay REC` that recorded one (rec_state.rs `replay_state`); wins over RAC2_STATE.
static REPLAY_START: OnceLock<String> = OnceLock::new();

/// `--replay REC` with `REC/state`: the replay resumes from it (lib.rs `main`, before anything reads `start_dir`).
pub fn set_start(dir: String) {
	let _ = REPLAY_START.set(dir);
}

/// RAC2_STATE: the state dir the game starts from (None: cold boot).
pub fn start_dir() -> Option<String> {
	static D: OnceLock<Option<String>> = OnceLock::new();
	D.get_or_init(|| {
		if let Some(d) = REPLAY_START.get() {
			return Some(d.clone());
		}
		let v = std::env::var("RAC2_STATE").ok().filter(|v| !v.trim().is_empty())?;
		let v = v.trim();
		if v.eq_ignore_ascii_case("last") {
			let p = newest().map(|p| p.to_string_lossy().into_owned());
			if p.is_none() {
				eprintln!("rac2-play: RAC2_STATE=last: no quick save in {} yet (F5 saves one); starting normally", states_dir().display());
			}
			p
		} else if let Some(missing) = missing_file(std::path::Path::new(v)) {
			eprintln!("rac2-play: RAC2_STATE={v}: not a complete savestate dir ({missing} is missing; convert a .p2s with `python oracle/p2s.py X.p2s DIR`); starting normally");
			None
		} else {
			Some(v.to_string())
		}
	})
	.clone()
}

/// The files `Machine::load_resume` and the GS pipe read from a state dir; the first one missing, if any.
pub(crate) fn missing_file(dir: &std::path::Path) -> Option<&'static str> {
	["eeMemory.bin", "Scratchpad.bin", "vu0Memory.bin", "vu0MicroMem.bin", "eeHwRegs.bin", "iopMemory.bin", "regs.bin"]
		.into_iter()
		.find(|n| !dir.join(n).is_file())
}

/// Fields (about 5 s) a quick save waits for the memory card server before it gives up: the card on disk is incomplete while the game
/// has a file open, and a save taken then would load a corrupted slot.
const SAVE_WAIT_MAX: u32 = 300;

/// Delays a quick save (F5 / RAC2_QSAVE_AT) while a memory card file is open: the game's autosave spans several fields (Open, Write, Write,
/// Close) and the card on disk is only complete at Close; a save taken in between would load against the stale card and mix an old file
/// with the new writes (a corrupted slot). `poll` once per field at the field end; true = save now.
#[derive(Default)]
pub struct SaveGate {
	pending: bool,
	waited: u32,
}

impl SaveGate {
	pub fn poll(&mut self, request: bool, card_busy: bool, field: u64) -> bool {
		self.pending |= request;
		if !self.pending {
			return false;
		}
		if card_busy && self.waited < SAVE_WAIT_MAX {
			if self.waited == 0 {
				note(format!("quick save waiting for the memory card (field {field})"));
			}
			self.waited += 1;
			return false;
		}
		let busy = card_busy;
		(self.pending, self.waited) = (false, 0);
		if busy {
			note(format!("could not save: memory card busy (field {field})"));
			return false;
		}
		true
	}
}

/// [tests] RAC2_QSAVE_AT=N: a quick save at the end of field N (the keyboard-free way to F5), then the run goes on.
pub fn save_at() -> Option<u64> {
	static F: OnceLock<Option<u64>> = OnceLock::new();
	*F.get_or_init(|| std::env::var("RAC2_QSAVE_AT").ok().and_then(|v| v.trim().parse().ok()))
}

/// Deletes all but the newest `keep` quick saves and leftovers of interrupted writes (a `.tmp` dir touched in the last minute may
/// be a writer at work and stays).
fn prune(keep: usize) {
	let Ok(rd) = std::fs::read_dir(states_dir()) else { return };
	let mut v: Vec<(u64, PathBuf)> = Vec::new();
	for e in rd.flatten() {
		match stamp(&e.file_name().to_string_lossy()) {
			Some(t) => v.push((t, e.path())),
			None if e.path().extension().is_some_and(|x| x == "tmp") => {
				let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|a| a.as_secs() > 60);
				if old {
					let _ = std::fs::remove_dir_all(e.path());
				}
			}
			None => {}
		}
	}
	v.sort();
	let n = v.len();
	for (_, p) in v.into_iter().take(n.saturating_sub(keep)) {
		let _ = std::fs::remove_dir_all(p);
	}
}

/// A hash of EE RAM (`boot::ram_hash`'s).
fn hash(ram: &[u8]) -> u64 {
	ram.chunks_exact(8).fold(0xcbf29ce484222325u64, |h, c| (h ^ u64::from_le_bytes(c.try_into().unwrap())).wrapping_mul(0x100000001b3))
}

/// Game thread, at the end of `field` (before it is sent to the GS pipe): copies the machine and asks the pipe for its share; a thread
/// waits for it and writes the dir. False when this session cannot save right now (a save in flight, or the machine could not be copied).
pub fn save(m: &ee_oracle::Machine, field: u64, gs: &std::sync::Arc<rac_engine::gsfull::ovl_pipe::GsPipe>) -> bool {
	if saving() {
		note("quick save: still saving the last one");
		return false;
	}
	let img = match m.snapshot_state() {
		Ok(i) => i,
		Err(e) => {
			note(format!("quick save failed: {e}"));
			return false;
		}
	};
	let h = hash(img.ram());
	let cam = crate::modcam_hand::export();
	gs.snap.request(field);
	note(format!("quick save queued (field {field})"));
	let millis = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
	let dir = states_dir().join(format!("q{millis}_{field}"));
	let gs = gs.clone();
	let h = std::thread::Builder::new().name("qsave".into()).spawn(move || {
		let _ = std::fs::create_dir_all(states_dir());
		let r = match gs.snap.take(std::time::Duration::from_secs(10)) {
			Some((vram, vu1)) => img.write(&dir, &vram, Some(vu1)).inspect(|_| write_cam(&dir, cam)),
			None => Err("the GS pipe did not hand over its state".into()),
		};
		match r {
			Ok(()) => {
				prune(3);
				note(format!("quick save saved: field {field} -> {} (ram hash {h:016x})", dir.display()));
			}
			Err(e) => note(format!("quick save failed: {e}")),
		}
	});
	if let (Ok(h), Ok(mut w)) = (h, WRITER.lock()) {
		*w = Some(h);
	}
	true
}

/// F9: starts this exe again from the newest quick save (same arguments). True: started; the caller then ends this process.
pub fn restart_into_newest() -> bool {
	wait_for_save();
	let Some(dir) = newest() else {
		eprintln!("rac2-play: quick load: no quick save in {} (F5 saves one)", states_dir().display());
		return false;
	};
	let Ok(exe) = std::env::current_exe() else { return false };
	match std::process::Command::new(exe).args(std::env::args().skip(1)).env("RAC2_STATE", &dir).env(PARENT_ENV, std::process::id().to_string()).spawn() {
		Ok(_) => {
			eprintln!("rac2-play: quick load: restarting from {}", dir.display());
			true
		}
		Err(e) => {
			eprintln!("rac2-play: quick load: {e}");
			false
		}
	}
}

/// The Modern camera's file next to dyn.bin: one line "yaw pitch boom" (f32 bits in hex, exact). Host side only (the game never reads it);
/// a state without it (a PCSX2 conversion, an older save) simply seeds from retail's pose.
const CAM_FILE: &str = "modcam.txt";

fn write_cam(dir: &std::path::Path, cam: Option<Vec<u32>>) {
	let Some(c) = cam else { return };
	let line: Vec<String> = c.iter().map(|w| format!("{w:08x}")).collect();
	let _ = std::fs::write(dir.join(CAM_FILE), line.join(" ") + "
");
}

/// The words of the camera line; None for anything but 3 (the old file) or `WORDS` hex words.
fn parse_cam(text: &str) -> Option<Vec<u32>> {
	let v: Option<Vec<u32>> = text.split_whitespace().map(|w| u32::from_str_radix(w, 16).ok()).collect();
	v.filter(|v| v.len() == 3 || v.len() == crate::modcam_pose::WORDS)
}

/// A start from a state dir (`start_dir`): hands its saved Modern camera to the camera hand (no-op when the dir has none).
pub fn load_cam(dir: &str) {
	if let Some(c) = std::fs::read_to_string(std::path::Path::new(dir).join(CAM_FILE)).ok().and_then(|t| parse_cam(&t)) {
		crate::modcam_hand::import(&c);
		let f = |i: usize| f32::from_bits(c[i]);
		eprintln!("rac2-play: quick load: Modern camera yaw {:.2} pitch {:.2} boom {:.1} restored ({} words)", f(0), f(1), f(2), c.len());
	}
}

/// F9 hands its process id to the child through this variable; the child waits for it to end before it touches anything.
const PARENT_ENV: &str = "RAC2_PARENT_PID";

/// How long a restarted child waits for its parent to exit (the parent leaves as soon as F9 spawned it: seconds at most).
const PARENT_WAIT_MS: u32 = 20_000;

/// The child of an F9 restart waits here, first thing in `main`, until the process that spawned it has exited (`RAC2_PARENT_PID`), so
/// the two never both own the window, the sound device or `card.mcd`. Gives up after 20 s (a parent that hangs must not lock the game
/// out). No variable (a normal start): returns at once. Returns true when it had to wait.
pub fn wait_for_parent() -> bool {
	let Some(pid) = std::env::var(PARENT_ENV).ok().and_then(|v| v.trim().parse::<u32>().ok()).filter(|&p| p != 0 && p != std::process::id()) else {
		return false;
	};
	// Not inherited by what this process starts in turn (a later F9 sets its own).
	unsafe { std::env::remove_var(PARENT_ENV) };
	let t = std::time::Instant::now();
	let waited = wait_pid_exit(pid, PARENT_WAIT_MS);
	eprintln!("rac2-play: quick load: waited {} ms for the old process {pid} ({})", t.elapsed().as_millis(), if waited { "exited" } else { "timeout" });
	true
}

#[cfg(windows)]
fn wait_pid_exit(pid: u32, ms: u32) -> bool {
	unsafe extern "system" {
		fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut core::ffi::c_void;
		fn WaitForSingleObject(h: *mut core::ffi::c_void, ms: u32) -> u32;
		fn CloseHandle(h: *mut core::ffi::c_void) -> i32;
	}
	const SYNCHRONIZE: u32 = 0x0010_0000;
	// SAFETY: plain kernel32 calls on a handle this function opens and closes.
	unsafe {
		let h = OpenProcess(SYNCHRONIZE, 0, pid);
		if h.is_null() {
			return true; // already gone (or not ours to wait on)
		}
		let r = WaitForSingleObject(h, ms);
		CloseHandle(h);
		r == 0 // WAIT_OBJECT_0
	}
}

#[cfg(not(windows))]
fn wait_pid_exit(pid: u32, ms: u32) -> bool {
	let t = std::time::Instant::now();
	while std::path::Path::new(&format!("/proc/{pid}")).exists() {
		if t.elapsed().as_millis() as u32 >= ms {
			return false;
		}
		std::thread::sleep(std::time::Duration::from_millis(20));
	}
	true
}

#[cfg(test)]
mod tests {
	use super::*;

	/// A quick save requested while a card file is open waits for its Close (one save, at the first idle field); without a request nothing saves.
	#[test]
	fn save_gate_waits_for_the_card() {
		let mut g = SaveGate::default();
		assert!(!g.poll(false, false, 1));
		assert!(!g.poll(true, true, 2), "card open: wait");
		assert!(!g.poll(false, true, 3), "still open: the request is kept");
		assert!(g.poll(false, false, 4), "closed: save now");
		assert!(!g.poll(false, false, 5), "once");
		assert!(g.poll(true, false, 6), "idle card: immediately");
		// A card that never closes does not block the gate forever: after about 5 s the request is dropped (no save of a half-written card).
		assert!(!g.poll(true, true, 7));
		for _ in 0..=SAVE_WAIT_MAX {
			assert!(!g.poll(false, true, 8), "a busy card never saves");
		}
		assert!(!g.poll(false, false, 9), "the request is gone");
		assert!(g.poll(true, false, 10), "the next F5 works");
	}

	#[test]
	fn save_dir_names_sort_by_millis_and_old_names_come_first() {
		assert_eq!(stamp("q1730000000123_4567"), Some(1730000000123));
		assert_eq!(stamp("q1730000000"), Some(1730000000), "a build 13 save (unix seconds)");
		assert!(stamp("q1730000000").unwrap() < stamp("q1730000000123_1").unwrap());
		assert_eq!(stamp("q1730000000123_4567.tmp"), None);
		assert_eq!(stamp("forcefield"), None);
	}

	#[test]
	fn an_incomplete_state_dir_names_its_missing_file() {
		let d = std::env::temp_dir().join(format!("rac2_states_test_{}", std::process::id()));
		let _ = std::fs::create_dir_all(&d);
		assert_eq!(missing_file(&d), Some("eeMemory.bin"));
		std::fs::write(d.join("regs.bin"), b"x").unwrap();
		assert_eq!(missing_file(&d), Some("eeMemory.bin"), "regs.bin alone is not a state");
		let _ = std::fs::remove_dir_all(&d);
	}

	#[test]
	fn the_camera_line_round_trips_exactly() {
		let c: Vec<u32> = (0..crate::modcam_pose::WORDS as u32).map(|i| (i as f32 * 1.37 - 5.0).to_bits()).collect();
		let line: Vec<String> = c.iter().map(|w| format!("{w:08x}")).collect();
		assert_eq!(parse_cam(&(line.join(" ") + "
")), Some(c.clone()));
		assert_eq!(parse_cam("3f800000 3f800000 40000000").map(|v| v.len()), Some(3), "the old 3-word file");
		assert_eq!(parse_cam("zz 1 2"), None);
		assert_eq!(parse_cam("3f800000 3f800000"), None);
	}
}
