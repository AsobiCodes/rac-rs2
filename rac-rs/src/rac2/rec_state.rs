//! [PC] Session recordings that start from a savestate (RAC2_STATE, or the restart of an F9 quick load): the recording folder
//! (boot.rs `start_recording`) gets the state it starts from in `REC/state/`, and `--replay REC` resumes from that copy instead of
//! the cold boot (lib.rs `main` hands it to states.rs `set_start`). Without `state/` a replay cold-boots with `card.mcd` as before.
//! Folder rules (`prune`): the newest `KEEP` recordings stay; of them only the newest `KEEP_STATES` that hold a state (~39 MB each
//! when the hard link falls back to a copy) do, an older one with a state is removed whole (it cannot replay without it).
use std::path::{Path, PathBuf};

/// The state copy inside a recording folder.
pub const STATE_DIR: &str = "state";
/// Recordings kept (the newest).
pub const KEEP: usize = 10;
/// Recordings with a state kept (the newest).
pub const KEEP_STATES: usize = 3;

/// A new recording folder name under `base`: `t` (unix seconds), or the next free number when a session of the same second has one
/// (an F9 restart right after the start).
pub fn new_dir(base: &Path, t: u64) -> PathBuf {
	(t..t + 1000).map(|n| base.join(n.to_string())).find(|d| !d.exists()).unwrap_or_else(|| base.join(t.to_string()))
}

/// Copies the state dir `src` into `rec/state` (hard links where the disk allows: a quick save is never written again, and a link
/// costs no space; a copy otherwise). Written as `state.part` first, so a broken copy is never taken for a state.
pub fn copy_state(src: &Path, rec: &Path) -> std::io::Result<()> {
	let part = rec.join(format!("{STATE_DIR}.part"));
	let _ = std::fs::remove_dir_all(&part);
	copy_tree(src, &part)?;
	std::fs::rename(&part, rec.join(STATE_DIR))
}

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
	std::fs::create_dir_all(dst)?;
	for e in std::fs::read_dir(src)? {
		let e = e?;
		let to = dst.join(e.file_name());
		if e.file_type()?.is_dir() {
			copy_tree(&e.path(), &to)?;
		} else if std::fs::hard_link(e.path(), &to).is_err() {
			std::fs::copy(e.path(), &to)?;
		}
	}
	Ok(())
}

/// `--replay REC`: the state the recording starts from (`REC/state` when it is a complete state dir), else None (cold boot).
pub fn replay_state(rec: &str) -> Option<String> {
	let d = Path::new(rec).join(STATE_DIR);
	// The files the GS pipe (shots.rs) reads besides `missing_file`'s list.
	let pipe = ["vu1Memory.bin", "vu1MicroMem.bin", "GS.bin"].iter().all(|n| d.join(n).is_file());
	(crate::states::missing_file(&d).is_none() && pipe).then(|| d.to_string_lossy().into_owned())
}

/// The recordings to remove from (name, holds a state) pairs: all but the newest `keep`, then of the rest those with a state beyond
/// the newest `keep_states`.
pub fn to_remove(mut v: Vec<(u64, bool)>, keep: usize, keep_states: usize) -> Vec<u64> {
	v.sort_unstable_by(|a, b| b.0.cmp(&a.0));
	let mut states = 0;
	let mut out = Vec::new();
	for (i, (n, st)) in v.into_iter().enumerate() {
		let st_over = st && {
			states += 1;
			states > keep_states
		};
		if i >= keep || st_over {
			out.push(n);
		}
	}
	out.sort_unstable();
	out
}

/// Applies `to_remove` to the numbered folders under `base` (anything else there is left alone).
pub fn prune(base: &Path, keep: usize, keep_states: usize) {
	let Ok(rd) = std::fs::read_dir(base) else { return };
	let v: Vec<(u64, bool)> = rd.flatten().filter_map(|e| Some((e.file_name().to_str()?.parse().ok()?, e.path().join(STATE_DIR).is_dir()))).collect();
	for n in to_remove(v, keep, keep_states) {
		let _ = std::fs::remove_dir_all(base.join(n.to_string()));
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_newest_ten_stay_and_only_three_with_a_state() {
		// 12 recordings, 1..=12; those with a state: 2, 5, 8, 10, 11.
		let v: Vec<(u64, bool)> = (1..=12).map(|n| (n, [2, 5, 8, 10, 11].contains(&n))).collect();
		// Beyond the newest 10: 1, 2. States newest first: 11, 10, 8 stay; 5 goes (2 already).
		assert_eq!(to_remove(v, KEEP, KEEP_STATES), vec![1, 2, 5]);
		assert!(to_remove((1..=4).map(|n| (n, false)).collect(), KEEP, KEEP_STATES).is_empty());
	}

	#[test]
	fn a_state_recording_folder_round_trips() {
		let base = std::env::temp_dir().join(format!("rac2_rec_state_test_{}", std::process::id()));
		let _ = std::fs::remove_dir_all(&base);
		let src = base.join("q1_100");
		std::fs::create_dir_all(&src).unwrap();
		for n in ["eeMemory.bin", "Scratchpad.bin", "vu0Memory.bin", "vu0MicroMem.bin", "eeHwRegs.bin", "iopMemory.bin", "regs.bin", "vu1Memory.bin", "vu1MicroMem.bin", "GS.bin", "modcam.txt"] {
			std::fs::write(src.join(n), n.as_bytes()).unwrap();
		}
		let recs = base.join("recordings");
		std::fs::create_dir_all(recs.join("500")).unwrap();
		let d = new_dir(&recs, 500);
		assert_eq!(d, recs.join("501"), "same second: the next number");
		std::fs::create_dir_all(&d).unwrap();
		assert_eq!(replay_state(&d.to_string_lossy()), None, "no state: cold boot");
		copy_state(&src, &d).unwrap();
		assert_eq!(std::fs::read(d.join(STATE_DIR).join("regs.bin")).unwrap(), b"regs.bin");
		assert!(!d.join("state.part").exists());
		assert_eq!(replay_state(&d.to_string_lossy()), Some(d.join(STATE_DIR).to_string_lossy().into_owned()));
		// The quick save may be pruned later: the recording's copy stays.
		std::fs::remove_dir_all(&src).unwrap();
		assert!(d.join(STATE_DIR).join("eeMemory.bin").is_file());
		// prune: 4 more state recordings (502..505) -> 501 and 502 (the oldest with a state) go, 500 (none) stays, a stray name stays.
		for n in 502..=505u64 {
			std::fs::create_dir_all(recs.join(n.to_string()).join(STATE_DIR)).unwrap();
		}
		std::fs::create_dir_all(recs.join("notes")).unwrap();
		prune(&recs, KEEP, KEEP_STATES);
		let mut left: Vec<String> = std::fs::read_dir(&recs).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
		left.sort();
		assert_eq!(left, ["500", "503", "504", "505", "notes"]);
		let _ = std::fs::remove_dir_all(&base);
	}
}
