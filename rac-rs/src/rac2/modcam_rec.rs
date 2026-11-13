//! The Modern camera's input in session recordings (round 3 batch 26): pad.bin keeps the pad only, so a session with mouse look
//! desynced on `--replay` (the camera hand turned differently, and Ratchet walks by the camera). Each field's `Input` (the mouse
//! counts, RMB / strafe, the wheel, the camera keys: what `modcam_ui::take_ui` gave the camera hand) goes to cam.bin next to pad.bin
//! (8 bytes a field); a replay with a cam.bin gives the camera hand those inputs back field by field instead of the window's.
//! Recordings without cam.bin replay as before.

use crate::modcam_pose::Input;
use std::sync::Mutex;

/// Bytes per field in cam.bin.
pub const REC: usize = 8;

static REPLAY: Mutex<Option<Vec<Input>>> = Mutex::new(None);

pub fn encode(i: &Input) -> [u8; REC] {
	let [x0, x1] = i.look[0].to_le_bytes();
	let [y0, y1] = i.look[1].to_le_bytes();
	[x0, x1, y0, y1, i.aim as u8 | (i.strafe as u8) << 1, i.zoom as u8, i.keys[0] as u8, i.keys[1] as u8]
}

pub fn decode(b: &[u8]) -> Input {
	Input { look: [i16::from_le_bytes([b[0], b[1]]), i16::from_le_bytes([b[2], b[3]])], aim: b[4] & 1 != 0, strafe: b[4] & 2 != 0, zoom: b[5] as i8, keys: [b[6] as i8, b[7] as i8] }
}

pub fn decode_all(b: &[u8]) -> Vec<Input> {
	b.chunks_exact(REC).map(decode).collect()
}

/// `--replay REC`: REC/cam.bin, if the recording has one (the number of fields in it; 0 = none, the window's input as before).
pub fn load(rec: &str) -> usize {
	let v = std::fs::read(format!("{rec}/cam.bin")).map(|b| decode_all(&b)).unwrap_or_default();
	let n = v.len();
	if let Ok(mut r) = REPLAY.lock() {
		*r = (n > 0).then_some(v);
	}
	n
}

/// The recorded input of field `n` while a replay with a cam.bin runs.
pub fn replayed(n: u32) -> Option<Input> {
	REPLAY.lock().ok()?.as_ref()?.get(n as usize).copied()
}

/// The session recording: field `field`'s camera input to cam.bin.
pub fn record(w: &mut Option<std::io::BufWriter<std::fs::File>>, field: u64, i: &Input) {
	use std::io::Write;
	if let Some(w) = w.as_mut() {
		let _ = w.write_all(&encode(i));
		if field % 250 == 0 {
			let _ = w.flush();
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn record_then_replay_gives_the_same_input_per_field() {
		let fields = [
			Input::default(),
			Input { look: [-321, 17], aim: true, strafe: true, zoom: -2, keys: [127, -127] },
			Input { look: [32000, -32000], aim: false, strafe: true, zoom: 5, keys: [0, 64] },
		];
		let path = std::env::temp_dir().join(format!("rac2_cam_{}", std::process::id()));
		std::fs::create_dir_all(&path).unwrap();
		let mut w = Some(std::io::BufWriter::new(std::fs::File::create(path.join("cam.bin")).unwrap()));
		for (k, i) in fields.iter().enumerate() {
			record(&mut w, k as u64, i);
		}
		drop(w);
		assert_eq!(load(path.to_str().unwrap()), fields.len());
		for (k, i) in fields.iter().enumerate() {
			assert_eq!(replayed(k as u32), Some(*i), "field {k}");
		}
		assert_eq!(replayed(fields.len() as u32), None, "past the recording");
		// A recording without cam.bin: none.
		std::fs::remove_file(path.join("cam.bin")).unwrap();
		assert_eq!(load(path.to_str().unwrap()), 0);
		assert_eq!(replayed(0), None);
		let _ = std::fs::remove_dir_all(&path);
	}
}
