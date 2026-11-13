//! [PC] [v0.1 U] The automatic update check of R&C2, as R&C1's `main.rs: update_check` (Windows) and `rac1/update_notice_linux.rs` (Linux).
//!
//! Windows: before the game starts, a hidden PowerShell runs `<install root>\setup.ps1 -Launch -ExePid <pid> -WindowFlag
//! <generated\update_window.flag>` (setup.ps1 reads `generated\play.cfg`, so it asks the R&C2 repository `AsobiCodes/rac-rs2`). Exit code 10
//! = the update window took over: `at_start` returns true and the game must exit so the update can replace its exe. Every other outcome
//! (no setup.ps1, offline, up to date, "Not now", spawn failure, a hang) returns false = start the game. The script writes the flag file
//! when it shows a window: until then it gets 15 s, afterwards the exe waits for the answer without a limit. A flag file (not a pipe)
//! keeps this single-threaded, which `install_start::apply` needs (`set_var`).
//! Linux: the same start rules, then `update_notice_linux::start` (a thread, a notice window that points to the launcher).
//!
//! Only a player's start checks: no arguments (the desktop shortcut, Play.cmd, Steam, the launcher), no RAC2_STATE (an F9 restart or a
//! state start), and an install root with a setup.ps1 (a dev exe outside an install has none). Skipped with RAC_NO_UPDATE_CHECK (Play old
//! version, the launcher) and in the automated test runs (RAC_TEST_WINDOW / RAC_RETAIL_SMOKE) unless RAC_TEST_UPDATE_BASE is set.
//! Test hooks: RAC_TEST_UPDATE_BASE -> `-TestUpdateBase`, RAC_TEST_UPDATE_CHOICE=update|notnow -> `-TestNoShow -TestChoice`,
//! RAC_TEST_UPDATE_SHOT -> `-TestShot` (the same switches as R&C1; ENV.md lists them).
#![cfg_attr(not(windows), allow(dead_code))]

use std::ffi::OsString;
use std::path::Path;

/// The public repository of R&C2 (setup.ps1 `$RepoRaw` for `-Game rac2`; the Linux notice's `REPO_RAW` is this one raw URL).
#[allow(dead_code)] // documents the repo; the code reads it from setup.ps1 (Windows) and update_notice_linux::REPO_RAW (Linux), a test ties them
pub(crate) const REPO: &str = "AsobiCodes/rac-rs2";
/// The longest the script may take before it shows a window (R&C1: 15 s).
const WINDOW_WAIT_SECS: u64 = 15;
/// The exit code of `setup.ps1 -Launch` when its update window took over.
const EXIT_UPDATE_WINDOW: i32 = 10;

/// Does this start skip the check? `has` = is this environment variable set; `n_args` = the arguments after the exe name.
/// A player's start has none; the test and dev runs pass `--fields`, `--window-shots`, an ISO path and so on, or set RAC2_STATE.
pub(crate) fn skip(has: &dyn Fn(&str) -> bool, n_args: usize) -> bool {
	n_args > 0
		|| has("RAC2_STATE")
		|| has("RAC_NO_UPDATE_CHECK")
		|| ((has("RAC_TEST_WINDOW") || has("RAC_RETAIL_SMOKE")) && !has("RAC_TEST_UPDATE_BASE"))
}

/// The arguments of the PowerShell call (after the program name), in the order R&C1 builds them. The three test values are the
/// environment hooks: base (RAC_TEST_UPDATE_BASE), choice (RAC_TEST_UPDATE_CHOICE; only `update` / `notnow` count), shot (RAC_TEST_UPDATE_SHOT).
pub(crate) fn powershell_args(setup: &Path, pid: u32, flag: &Path, base: Option<OsString>, choice: Option<&str>, shot: Option<OsString>) -> Vec<OsString> {
	let mut a: Vec<OsString> = ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"].iter().map(OsString::from).collect();
	a.push(setup.into());
	a.extend(["-Launch", "-ExePid"].iter().map(OsString::from));
	a.push(pid.to_string().into());
	a.push("-WindowFlag".into());
	a.push(flag.into());
	if let Some(b) = base {
		a.push("-TestUpdateBase".into());
		a.push(b);
	}
	if let Some(c) = choice.filter(|c| *c == "update" || *c == "notnow") {
		a.extend(["-TestNoShow", "-TestChoice", c].iter().map(OsString::from));
		if let Some(s) = shot {
			a.push("-TestShot".into());
			a.push(s);
		}
	}
	a
}

/// Called by `main` right after `install_start::apply` (the last `set_var`; nothing else has started). True = the update window took over:
/// return from `main` at once. False = start the game.
pub fn at_start() -> bool {
	let has = |k: &str| std::env::var_os(k).is_some();
	if skip(&has, std::env::args_os().count().saturating_sub(1)) {
		return false;
	}
	let Ok(root) = rac_engine::install_root() else { return false };
	#[cfg(windows)]
	return windows_check(&root);
	#[cfg(target_os = "linux")]
	{
		super::update_notice_linux::start(root);
		return false;
	}
	#[cfg(not(any(windows, target_os = "linux")))]
	{
		let _ = root;
		false
	}
}

/// Windows: runs the hidden PowerShell and waits as the module doc says. Any failure is false.
#[cfg(windows)]
fn windows_check(root: &Path) -> bool {
	use std::os::windows::process::CommandExt;
	use std::process::{Command, Stdio};
	use std::time::{Duration, Instant};
	const CREATE_NO_WINDOW: u32 = 0x0800_0000;
	let setup = root.join("setup.ps1");
	if !setup.is_file() {
		return false;
	}
	let flag = root.join("generated").join("update_window.flag");
	let _ = std::fs::remove_file(&flag);
	let sysroot = std::env::var_os("SystemRoot").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::PathBuf::from(r"C:\Windows"));
	let args = powershell_args(
		&setup,
		std::process::id(),
		&flag,
		std::env::var_os("RAC_TEST_UPDATE_BASE"),
		std::env::var("RAC_TEST_UPDATE_CHOICE").ok().as_deref(),
		std::env::var_os("RAC_TEST_UPDATE_SHOT"),
	);
	let mut cmd = Command::new(sysroot.join(r"System32\WindowsPowerShell\v1.0\powershell.exe"));
	cmd.args(args).current_dir(root).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).creation_flags(CREATE_NO_WINDOW);
	let Ok(mut child) = cmd.spawn() else { return false };
	let t0 = Instant::now();
	loop {
		match child.try_wait() {
			Ok(Some(status)) => return status.code() == Some(EXIT_UPDATE_WINDOW),
			Ok(None) => {}
			Err(_) => return false,
		}
		if t0.elapsed() > Duration::from_secs(WINDOW_WAIT_SECS) && !flag.is_file() {
			let _ = child.kill();
			let _ = child.wait();
			return false;
		}
		std::thread::sleep(Duration::from_millis(40));
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn with(names: &'static [&'static str]) -> impl Fn(&str) -> bool {
		move |k: &str| names.contains(&k)
	}

	#[test]
	fn a_plain_player_start_checks() {
		assert!(!skip(&with(&[]), 0));
		assert!(!skip(&with(&["RAC2_ISO", "RAC_PLAYER_DIR", "RAC_DIAG", "RAC_RUN_LOG"]), 0), "Play.cmd's variables do not skip");
	}

	#[test]
	fn arguments_and_states_are_test_or_dev_runs() {
		assert!(skip(&with(&[]), 1), "an ISO path");
		assert!(skip(&with(&[]), 4), "--fields 600 --window-shots dir");
		assert!(skip(&with(&["RAC2_STATE"]), 0), "an F9 restart or a state start");
	}

	#[test]
	fn the_switches_skip_like_r_and_c_1() {
		assert!(skip(&with(&["RAC_NO_UPDATE_CHECK"]), 0));
		assert!(skip(&with(&["RAC_TEST_WINDOW"]), 0));
		assert!(skip(&with(&["RAC_RETAIL_SMOKE"]), 0));
		assert!(!skip(&with(&["RAC_TEST_WINDOW", "RAC_TEST_UPDATE_BASE"]), 0), "the update test runs in the hidden window");
		assert!(skip(&with(&["RAC_NO_UPDATE_CHECK", "RAC_TEST_UPDATE_BASE"]), 0));
		assert!(skip(&with(&["RAC_TEST_WINDOW", "RAC_TEST_UPDATE_BASE"]), 2), "arguments still skip");
	}

	fn strs(v: Vec<OsString>) -> Vec<String> {
		v.into_iter().map(|s| s.to_string_lossy().into_owned()).collect()
	}

	#[test]
	fn the_plain_call_is_launch_with_pid_and_flag() {
		let a = strs(powershell_args(Path::new("C:/g/setup.ps1"), 4242, Path::new("C:/g/generated/update_window.flag"), None, None, None));
		assert_eq!(
			a,
			["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "C:/g/setup.ps1", "-Launch", "-ExePid", "4242", "-WindowFlag", "C:/g/generated/update_window.flag"]
		);
	}

	#[test]
	fn the_test_hooks_become_switches() {
		let (s, f) = (Path::new("s.ps1"), Path::new("f.flag"));
		let a = strs(powershell_args(s, 1, f, Some("D:/fake".into()), Some("update"), Some("D:/shot.png".into())));
		assert_eq!(a[10..], ["-TestUpdateBase", "D:/fake", "-TestNoShow", "-TestChoice", "update", "-TestShot", "D:/shot.png"]);
		let a = strs(powershell_args(s, 1, f, None, Some("notnow"), None));
		assert_eq!(a[10..], ["-TestNoShow", "-TestChoice", "notnow"]);
		let a = strs(powershell_args(s, 1, f, Some("D:/fake".into()), Some("maybe"), Some("x.png".into())));
		assert_eq!(a[10..], ["-TestUpdateBase", "D:/fake"], "an unknown choice and the shot without a choice are dropped");
		let a = strs(powershell_args(s, 1, f, None, None, Some("x.png".into())));
		assert_eq!(a.len(), 10, "a shot needs a choice");
	}

	#[test]
	fn the_repository_is_the_r_and_c_2_one() {
		assert_eq!(REPO, "AsobiCodes/rac-rs2");
		assert_eq!(super::super::update_notice_linux::REPO_RAW, format!("https://raw.githubusercontent.com/{REPO}/main"));
	}

	/// The exit code that makes the exe leave, and the wait before a window, are R&C1's.
	#[test]
	fn the_exit_code_and_the_wait_are_r_and_c_1s() {
		assert_eq!(EXIT_UPDATE_WINDOW, 10);
		assert_eq!(WINDOW_WAIT_SECS, 15);
	}
}
