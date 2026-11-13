//! R&C2 cold boot, session recordings and the small parsers shared by the window and the hidden mode (from lane rc2).
use ee_oracle::Machine;
use ee_oracle::gs::Job;

/// The cold-booted machine (recompiled code on) with its graphics jobs routed to the returned receiver.
pub fn boot(iso: &str) -> Result<(Machine, std::sync::mpsc::Receiver<Job>), String> {
	// [PC] `lang=` in pc_options.txt: the console language the cold boot reports (options.rs; no key = the console default).
	crate::options::cold_language();
	let mut m = Machine::cold(iso, "")?;
	if !ee_oracle::ver::rc2() {
		return Err(format!("{iso}: not the Ratchet & Clank 2 PAL disc (SCES_516.07)"));
	}
	// The recompiled retail code (oracle/rc2gen): a set runs only while all its functions match RAM, so the boot and the menus
	// run on the interpreter until the code they use is in place. ORACLE_REC=0 keeps the interpreter alone.
	crate::register();
	let n = m.enable_rec();
	if std::env::var_os("RAC2_LOG").is_some() {
		eprintln!("rac2-play: rec {}", if n > 0 { format!("on ({n} functions at boot)") } else { "off".into() });
	}
	let (tx, rx) = std::sync::mpsc::channel::<Job>();
	m.cpu.mem.gsq = Some(tx);
	Ok((m, rx))
}

/// The machine resumed from a savestate dir (states.rs; RAC2_STATE) with its graphics jobs routed to the returned receiver: the same
/// recompiled code and hooks as `boot`, but no cold-boot kernel (the state's RAM holds the game's own).
pub fn boot_state(iso: &str, dir: &str) -> Result<(Machine, std::sync::mpsc::Receiver<Job>), String> {
	*ee_oracle::statefile::ISO.lock().unwrap() = Some(iso.to_string());
	let mut m = Machine::load_resume(dir);
	if !ee_oracle::ver::rc2() {
		return Err(format!("{iso}: not the Ratchet & Clank 2 PAL disc (SCES_516.07)"));
	}
	crate::register();
	state_video_mode(&mut m);
	let n = m.enable_rec();
	eprintln!("rac2-play: state {dir} (ram hash {:016x}), rec {}", ram_hash(&m), if n > 0 { format!("on ({n} functions)") } else { "off".into() });
	let (tx, rx) = std::sync::mpsc::channel::<Job>();
	m.cpu.mem.gsq = Some(tx);
	Ok((m, rx))
}

/// A state saved in the game's 60 Hz mode (`addrs::PAL_FLAG` = 0, or the progressive word `addrs::VIDEO_60` = 1) resumes at 59.94
/// fields a second, as the oracle's SetGsCrt does when the game resets the video (oracle hle.rs: omode 2 / 0x50); the sound
/// driver's tick offset first (`video_mode` reads the old mode).
fn state_video_mode(m: &mut Machine) {
	let ntsc = m.rd32(crate::addrs::PAL_FLAG) == 0 || m.rd32(crate::addrs::VIDEO_60) != 0;
	if ntsc != ee_oracle::ver::ntsc() {
		m.cpu.snd.video_mode(ntsc);
		ee_oracle::ver::set_ntsc(ntsc);
		eprintln!("rac2-play: state in the game's {} mode", if ntsc { "60 Hz" } else { "50 Hz" });
	}
}

/// RAC2_LEVEL=N: start level N (G/LEVELn.WAD) instead of Aranos from New Game.
pub fn level_override() -> Option<u32> {
	static L: std::sync::OnceLock<Option<u32>> = std::sync::OnceLock::new();
	*L.get_or_init(|| std::env::var("RAC2_LEVEL").ok().and_then(|v| v.trim().parse().ok()).filter(|&n| n > 0 && n < 27))
}

/// [tests] RAC2_WIDE=1: the game's 16:9 Options row (`addrs::WIDE`) is held on at every field, so a replay whose save was made at
/// 4:3 shows the wide picture from the game's next projection update (FUN_0027af28: level and camera starts). The RAM then
/// differs from the recording's.
pub fn wide_override() -> bool {
	static W: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
	*W.get_or_init(|| std::env::var("RAC2_WIDE").is_ok_and(|v| v == "1"))
}

pub fn apply_wide(m: &mut Machine) {
	if m.rd8(crate::addrs::WIDE) == 0 {
		m.cpu.mem.wr8(crate::addrs::WIDE, 1);
	}
}

/// While the game has begun its New Game load (level word 0) and the loader's destination word is still 0, writes `n` there.
pub fn apply_level_override(m: &mut Machine, n: u32) {
	use ee_oracle::rc2::{LEVEL, NEXT_LEVEL};
	if m.rd32(LEVEL) == 0 && m.rd32(NEXT_LEVEL) == 0 {
		m.cpu.mem.wr32(NEXT_LEVEL, n);
	}
}

/// "a-b:MASK,..." (MASK hex): (first field, last field, value).
pub fn parse_pad(s: &str) -> Vec<(u64, u64, u16)> {
	s.split(',')
		.filter_map(|r| {
			let (range, mask) = r.split_once(':')?;
			let (a, b) = range.split_once('-')?;
			let mask = u16::from_str_radix(mask.trim_start_matches("0x"), 16).ok()?;
			Some((a.parse().ok()?, b.parse().ok()?, mask))
		})
		.collect()
}

/// A hash of EE RAM (RAC2_RAMHASH=1 prints it every 250 fields: the replay check against rac2-play's recordings).
pub fn ram_hash(m: &Machine) -> u64 {
	m.cpu.mem.ram[..32 << 20].chunks_exact(8).fold(0xcbf29ce484222325u64, |h, c| (h ^ u64::from_le_bytes(c.try_into().unwrap())).wrapping_mul(0x100000001b3))
}

/// A recording's pad per field: 6 bytes each (u16 buttons LE, then rx ry lx ly).
pub fn read_pad(path: &str) -> Result<Vec<(u16, [u8; 4])>, String> {
	let b = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
	Ok(b.chunks_exact(6).map(|c| (u16::from_le_bytes([c[0], c[1]]), [c[2], c[3], c[4], c[5]])).collect())
}

/// The session recording folder (`start_recording`).
pub static RECDIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Window sessions are recorded for bug reports (RAC2_NOREC=1: not): recordings/<unix time>/ in the player dir gets a copy of the
/// memory card as the session starts (card.mcd), the TV shape it boots with (screen.txt) and the window's pad per field (pad.bin,
/// `read_pad`'s format, written by window.rs). `--shots DIR --replay REC` plays it back field for field. A session started from a
/// state (`state`: RAC2_STATE, or the restart of an F9 quick load, which so begins a new folder) also gets that state in REC/state
/// (rec_state.rs), and the replay resumes from it. The newest 10 are kept, of them the newest 3 with a state. Same format as the old
/// standalone rac2-play, so its recordings replay here.
pub fn start_recording(card: Option<&str>, state: Option<&str>) {
	if std::env::var_os("RAC2_NOREC").is_some() {
		return;
	}
	let base = rac_engine::pcopts::player_dir().join("recordings");
	let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
	let d = crate::rec_state::new_dir(&base, t);
	if std::fs::create_dir_all(&d).is_err() {
		return;
	}
	if let Some(c) = card {
		let _ = std::fs::copy(c, d.join("card.mcd"));
	}
	if let Some(s) = state {
		if let Err(e) = crate::rec_state::copy_state(std::path::Path::new(s), &d) {
			eprintln!("rac2-play: recording: the start state {s} was not copied ({e}): this recording cannot be replayed");
		}
	}
	// The TV shape the session boots with (lib.rs `main`), for the replay.
	let _ = std::fs::write(d.join("screen.txt"), format!("{}\n", ee_oracle::cold::aspect()));
	// The options the session ran with (camera, key prompts and bindings change the game's RAM): an exact replay runs with them too.
	let _ = std::fs::copy(rac_engine::pcopts::player_dir().join("pc_options.txt"), d.join("pc_options.txt"));
	let _ = RECDIR.set(d);
	crate::rec_state::prune(&base, crate::rec_state::KEEP, crate::rec_state::KEEP_STATES);
}
