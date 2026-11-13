//! [PC] [b45 P1] What happens when R&C2 stops: the panic hook (a line in the run log and in `<player dir>/crash.log`, as R&C1's
//! main.rs does), the message box for a failed start (missing disc, broken play.cfg, boot error; a double-clicked game has no
//! console), and the Linux panic window. Same engine calls as R&C1's main.rs (`crash_log`, `message_box`, the hook).

use rac_engine::{omni, pcopts, runlog};
use std::io::IsTerminal;

/// Appends one line to `<player dir>/crash.log`.
pub fn crash_log(text: &str) {
	use std::io::Write;
	let dir = pcopts::player_dir();
	let _ = std::fs::create_dir_all(&dir);
	let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
	if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("crash.log")) {
		let _ = writeln!(f, "[unix time {secs}] {text}");
	}
}

/// The panic hook: a panic caught on the spot (texture-pack decoder; texpack/guard.rs) is one log line; any other panic goes into
/// the run log with the calls that led to it and into crash.log. On Linux a panic of the main thread also opens a window.
pub fn install_panic_hook() {
	#[cfg(not(windows))]
	let prev = std::panic::take_hook();
	std::panic::set_hook(Box::new(move |info| {
		#[cfg(not(windows))]
		prev(info);
		if rac_engine::texpack::guard::take_expected() {
			eprintln!("{}: {info}", rac_engine::texpack::guard::expected_note());
			return;
		}
		let thread = std::thread::current();
		let text = format!("panic in thread {}: {info}", thread.name().unwrap_or("?"));
		omni::flush_log();
		eprintln!("{text}");
		static TRACED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
		if TRACED.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < 4 {
			runlog::note_calls();
		}
		crash_log(&text);
		#[cfg(not(windows))]
		if thread.name() == Some("main") && std::env::var_os("RAC_TEST_WINDOW").is_none() && !std::io::stderr().is_terminal() {
			rac_engine::dialog_linux::show("Ratchet & Clank 2 (rac2-play)", &format!("The game stopped: {info}\n\nMore details are in player/crash.log."), rac_engine::dialog_linux::Kind::Error);
		}
	}));
}

/// Whether a failure should open a box: only for a player's session with no console (a double-click), never in test runs.
pub fn box_wanted() -> bool {
	!std::io::stderr().is_terminal() && std::env::var_os("RAC_TEST_WINDOW").is_none() && std::env::var_os("RAC_RETAIL_SMOKE").is_none()
}

/// A plain message box (no console needed); on Linux a zenity / kdialog / xmessage window.
pub fn message_box(text: &str) {
	#[cfg(windows)]
	{
		rac_engine::runlog_policy::note_message(text);
		#[link(name = "user32")]
		unsafe extern "system" {
			fn MessageBoxW(hwnd: *mut std::ffi::c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
		}
		let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
		// MB_OK | MB_ICONINFORMATION
		unsafe { MessageBoxW(std::ptr::null_mut(), wide(text).as_ptr(), wide("Ratchet & Clank 2 (rac2-play)").as_ptr(), 0x40) };
	}
	#[cfg(not(windows))]
	{
		if !runlog::active() {
			eprintln!("{text}");
		}
		rac_engine::dialog_linux::show("Ratchet & Clank 2 (rac2-play)", text, rac_engine::dialog_linux::Kind::Error);
	}
}

/// A failed start or run: the text goes to the run log, crash.log and, for a player with no console, a box.
pub fn report_failure(text: &str) {
	eprintln!("rac2-play: {text}");
	crash_log(&format!("error: {text}"));
	if box_wanted() {
		message_box(&format!("The game stopped: {text}\n\nDetails are in player/crash.log."));
	}
}
