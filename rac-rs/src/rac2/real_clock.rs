//! [PC] The console clock for R&C2: which runs give the game the host's real date and time, and which keep the fixed clock.
//!
//! A real PS2 has a clock (CDVD RTC in Japan time, plus the time zone and summer time of its settings) and the game stores what it
//! reads in every save: the Load / Save Game slots show it as day / month / year (slot record +0x15..+0x17, BCD; modern_load.rs).
//! Without the real clock the oracle answers sceCdReadClock with zeros and the slots show a wrong date (95/00/00). The oracle
//! (ee_oracle::clock) can hand the game the real clock; this file decides when, with the rule of R&C1's
//! rac1/retail/console_clock.rs: a replay, a hash run, a picture check or a savestate test keeps the fixed (zero) clock it always
//! had, so its RAM never depends on the day; only a player's session gets the real one.
//!
//! The rule, in `real_wanted`:
//! - `RAC_REAL_CLOCK=1` forces the real clock, `RAC_REAL_CLOCK=0` the fixed one.
//! - Otherwise the clock is fixed when the run is scripted (`init`'s argument: --shots, --window-shots, --replay, --pad, --stick) or
//!   any of `DETERMINISTIC_RUNS` is set (scripted input, savestate / level start, RAM hash or dump runs, test clock and window).
//!   RAC2_STATE alone does not count when RAC2_PARENT_PID is set too: that is an F9 load's child (states.rs `restart`), which
//!   carries on the player's session (and keeps its parent's arguments, so a scripted parent's child stays fixed).
//! - Otherwise (a player starting the game) the clock is real.

/// The switches that make an R&C2 run deterministic: with one of them set the game keeps the fixed clock.
const DETERMINISTIC_RUNS: [&str; 17] = [
	"RAC2_STATE",
	"RAC2_LEVEL",
	"RAC2_INPUT_FEED",
	"RAC2_MOUSE_SCRIPT",
	"RAC2_CAM_FEED",
	"RAC2_FREECAM_SCRIPT",
	"RAC2_DEBUGKEYS_FEED",
	"RAC2_QSAVE_AT",
	"RAC2_RAMHASH",
	"RAC2_RAMDUMP",
	"RAC_TEST_CLOCK",
	"RAC_TEST_UNPACED",
	"RAC_TEST_WINDOW",
	"RAC_TEST_STATE",
	"RAC_TEST_PAD",
	"RAC_TEST_FIELDS",
	"RAC_SHOT_FRAMES",
];

/// The variable an F9 load's child process gets (states.rs PARENT_ENV): its RAC2_STATE is the player's own quick save.
const F9_CHILD: &str = "RAC2_PARENT_PID";

/// True when the run should get the host's real clock (see the module doc), for the environment `var` reads and `names` lists.
fn real_wanted(var: impl Fn(&str) -> Option<String>, names: &[String], scripted: bool) -> bool {
	match var("RAC_REAL_CLOCK").as_deref() {
		Some("1") => return true,
		Some("0") => return false,
		_ => {}
	}
	let f9_child = names.iter().any(|k| k == F9_CHILD);
	!scripted && !names.iter().any(|k| DETERMINISTIC_RUNS.contains(&k.as_str()) && !(f9_child && k == "RAC2_STATE"))
}

/// Called once in `main` before the machine is created (the F9 child's RAC2_PARENT_PID is still set then): switches the
/// oracle's console clock on for a player's session. `scripted`: the command line scripts the run.
pub fn init(scripted: bool) {
	let names: Vec<String> = std::env::vars_os().map(|(k, _)| k.to_string_lossy().into_owned()).collect();
	let real = real_wanted(|k| std::env::var(k).ok(), &names, scripted);
	ee_oracle::clock::set_real(real);
	eprintln!("rac2-play: console clock {}", if real { "real (host date and time)" } else { "fixed (test, replay, scripted run or RAC_REAL_CLOCK=0)" });
}

#[cfg(test)]
mod tests {
	use super::*;

	fn run_s(env: &[(&str, &str)], scripted: bool) -> bool {
		let names: Vec<String> = env.iter().map(|(k, _)| k.to_string()).collect();
		real_wanted(|k| env.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string()), &names, scripted)
	}

	fn run(env: &[(&str, &str)]) -> bool {
		run_s(env, false)
	}

	/// A player's run (no test variable, no script on the command line) gets the real clock.
	#[test]
	fn real_clock_normal_play_is_real() {
		assert!(run(&[]));
		assert!(run(&[("RAC_PLAYER_DIR", "x"), ("RAC2_SAVE", "card.mcd"), ("RAC_MENU_STYLE", "classic")]));
	}

	/// Every scripted, replayed, savestate, hash and test-clock run keeps the fixed clock, so RAM hashes and pictures never depend on the day.
	#[test]
	fn real_clock_tests_and_replays_keep_the_fixed_clock() {
		for k in DETERMINISTIC_RUNS {
			assert!(!run(&[(k, "1")]), "{k}");
		}
		assert!(!run_s(&[], true));
		// what bisect\st.ps1 and the gate set, with their usual company
		assert!(!run(&[("RAC2_NOSOUND", "1"), ("RAC2_STATE", "x"), ("RAC_TEST_WINDOW", "hidden"), ("RAC_PLAYER_DIR", "p")]));
		assert!(!run_s(&[("RAC2_NOSOUND", "1"), ("RAC_PLAYER_DIR", "p")], true));
	}

	/// An F9 load's child carries on a player's session (real clock), unless its command line or another switch scripts it.
	#[test]
	fn real_clock_f9_child_stays_real() {
		assert!(run(&[("RAC2_STATE", "q"), (F9_CHILD, "123")]));
		assert!(!run_s(&[("RAC2_STATE", "q"), (F9_CHILD, "123")], true));
		assert!(!run(&[("RAC2_STATE", "q"), (F9_CHILD, "123"), ("RAC2_INPUT_FEED", "f")]));
	}

	/// Variables that are not about a deterministic run (sound, logs, HUD, menu style, player dir) leave the real clock on.
	#[test]
	fn real_clock_other_variables_leave_the_real_clock() {
		for k in ["RAC2_NOSOUND", "RAC2_LOG", "RAC2_TEXLOG", "RAC_HUD", "RAC_FMV", "RAC_MENU_STYLE", "RAC2_WIDE", "RAC_TEST_GAME_VERSION", "RAC2_REPLAY_DIR"] {
			assert!(run(&[(k, "1")]), "{k}");
		}
	}

	/// RAC_REAL_CLOCK decides both ways, over the test variables and the command line.
	#[test]
	fn real_clock_the_switch_overrides() {
		assert!(run(&[("RAC_REAL_CLOCK", "1"), ("RAC_TEST_CLOCK", "1")]));
		assert!(run_s(&[("RAC_REAL_CLOCK", "1")], true));
		assert!(!run(&[("RAC_REAL_CLOCK", "0")]));
		assert!(run(&[("RAC_REAL_CLOCK", "other")]));
	}
}
