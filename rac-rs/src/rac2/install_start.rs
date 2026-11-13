//! [PC] The no-argument start of an installed R&C2 (what Play.cmd / the launcher / the desktop shortcut do): the install root is the
//! first folder from the exe's folder upwards that holds `generated/play.cfg` (written by setup.ps1 -Game rac2, setup.sh); its
//! `iso=<path>` is the player's R&C2 disc image. Sets RAC2_ISO and RAC_PLAYER_DIR (`<root>/player`: card, pc_options.txt, log) unless
//! the caller already set them. Dev and test runs set RAC2_ISO themselves and never reach the file. Runs first thing in `main`, before
//! any thread starts (`set_var` is only sound then).

use std::path::Path;

/// Err is the text for the console (and the run log): the install has no usable play.cfg.
pub fn apply() -> Result<(), String> {
	if std::env::var_os("RAC2_ISO").is_some_and(|v| !v.is_empty()) {
		return Ok(());
	}
	let Ok(root) = rac_engine::install_root() else { return Ok(()) };
	let cfg = std::fs::read(root.join("generated").join("play.cfg")).map_err(|e| format!("cannot read generated/play.cfg ({e})"))?;
	let iso = parse_iso(&String::from_utf8_lossy(&cfg)).ok_or("generated/play.cfg has no disc image path (run the set-up again)")?;
	let iso = if Path::new(&iso).is_relative() { root.join(&iso).to_string_lossy().into_owned() } else { iso };
	// SAFETY: called first in main, before any thread starts.
	unsafe {
		std::env::set_var("RAC2_ISO", &iso);
		if std::env::var_os("RAC_PLAYER_DIR").is_none() {
			std::env::set_var("RAC_PLAYER_DIR", root.join("player"));
		}
	}
	Ok(())
}

/// `iso=<path>` of a play.cfg text (BOM and spaces ignored).
fn parse_iso(cfg: &str) -> Option<String> {
	cfg.trim_start_matches('\u{feff}').lines().filter_map(|l| l.split_once('=')).find(|(k, _)| k.trim() == "iso").map(|(_, v)| v.trim().to_string()).filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
	use super::parse_iso;

	#[test]
	fn reads_the_iso_line() {
		assert_eq!(parse_iso("\u{feff}iso=E:\\Games\\rc2.iso\nlang=de\n").as_deref(), Some("E:\\Games\\rc2.iso"));
		assert_eq!(parse_iso("lang=de\n"), None);
		assert_eq!(parse_iso("iso=\n"), None);
	}
}
