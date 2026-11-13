//! Sound measurement without a speaker (lane rc2 sound, 2026-10-31): `RAC2_SNDLOG=FILE` in a `--shots` run writes, per field, every
//! SPU2 key-on of the retail sound driver (voice, the bank handle and sample offset its SSA named, pitch, volumes) and the stream
//! starts, and at the end a table of key-ons per bank handle. `RAC2_SNDWAV=FILE` also writes the mixer's dry output (the
//! engine's `spumix`, no reverb) as a 48 kHz stereo WAV: a run that should be audible has a non-zero peak per second.
//! The driver's state is the same with or without recording (it only keeps the events), so the RAM hash does not change.
use ee_oracle::snd989::streams::StreamEv;
use ee_oracle::snd989::voices::Reg;
use rac_engine::spumix::{self, SpuVoices, VOICES};
use std::collections::BTreeMap;
use std::io::Write;

/// One PAL field of 48 kHz output.
const FIELD_SAMPLES: usize = 960;

pub struct SndLog {
	/// The RAC2_SNDLOG file; a sink when only RAC2_SNDWAV is set (no log file, and no file named NUL on Linux).
	out: Box<dyn Write + Send>,
	/// Per voice: (bank handle, sample offset, pitch, volume L, volume R) as the last writes left them.
	voice: Vec<(u32, u32, u32, u32, u32)>,
	/// (bank handle) -> key-ons, and the distinct (handle, offset) pairs with their counts.
	per_bank: BTreeMap<u32, u32>,
	per_sample: BTreeMap<(u32, u32), u32>,
	streams: u32,
	/// The WAV render (RAC2_SNDWAV): the mixer, the samples so far and the path.
	wav: Option<(SpuVoices, Vec<i16>, String)>,
}

impl SndLog {
	/// Starts the recording when `RAC2_SNDLOG` (or `RAC2_SNDWAV`) is set; asks the machine to keep the driver's events.
	pub fn from_env(m: &mut ee_oracle::Machine) -> Option<SndLog> {
		let path = std::env::var("RAC2_SNDLOG").ok();
		let wav = std::env::var("RAC2_SNDWAV").ok();
		if path.is_none() && wav.is_none() {
			return None;
		}
		let out: Box<dyn Write + Send> = match path.as_deref() {
			Some(p) => Box::new(std::io::BufWriter::new(std::fs::File::create(p).ok()?)),
			None => Box::new(std::io::sink()),
		};
		m.record_sound();
		Some(SndLog {
			out,
			voice: vec![(0, 0, 0, 0, 0); VOICES],
			per_bank: BTreeMap::new(),
			per_sample: BTreeMap::new(),
			streams: 0,
			wav: wav.map(|p| (SpuVoices::new(0, false), Vec::new(), p)),
		})
	}

	/// After a field: log its key-ons, feed the WAV mixer.
	pub fn field(&mut self, m: &mut ee_oracle::Machine, field: u64) {
		let b = spumix::drain(m);
		for e in &b.ev {
			let (idx, v) = (e.w.index as usize, e.w.value);
			match e.w.reg {
				Reg::Ssa if idx < VOICES => {
					self.voice[idx].0 = e.w.bank;
					self.voice[idx].1 = v;
				}
				Reg::Pitch if idx < VOICES => self.voice[idx].2 = v,
				Reg::VolL if idx < VOICES => self.voice[idx].3 = v,
				Reg::VolR if idx < VOICES => self.voice[idx].4 = v,
				Reg::Kon => {
					for bit in (0..24).filter(|bit| v & (1 << bit) != 0) {
						let n = (idx & 1) * 24 + bit;
						let (h, off, pitch, l, r) = self.voice[n];
						let _ = writeln!(self.out, "f{field} t{} kon v{n} bank {h:#x} off {off:#x} pitch {pitch:#x} vol {l:#x}/{r:#x}", e.w.tick);
						*self.per_bank.entry(h).or_default() += 1;
						*self.per_sample.entry((h, off)).or_default() += 1;
					}
				}
				_ => {}
			}
		}
		for s in &b.streams {
			if let StreamEv::Start { tick, voice, lba, held, .. } = s {
				let _ = writeln!(self.out, "f{field} t{tick} stream start v{voice} lba {lba:#x} held {held}");
				self.streams += 1;
			}
		}
		if let Some((mix, pcm, _)) = self.wav.as_mut() {
			mix.sync_core(&b.core, false);
			mix.push(b.ev);
			for _ in 0..FIELD_SAMPLES {
				let f = mix.frame();
				let c = |x: i32| x.clamp(-32768, 32767);
				pcm.push(((c(f.dry[0]) * f.master[0]) >> 15) as i16);
				pcm.push(((c(f.dry[1]) * f.master[1]) >> 15) as i16);
			}
		}
	}

	/// The per-bank table and the WAV file.
	pub fn finish(mut self) {
		let _ = writeln!(self.out, "summary: stream starts {}", self.streams);
		for (h, n) in &self.per_bank {
			let _ = writeln!(self.out, "summary: bank {h:#x} key-ons {n}");
		}
		for ((h, off), n) in &self.per_sample {
			let _ = writeln!(self.out, "summary: bank {h:#x} sample {off:#x} key-ons {n}");
		}
		let _ = self.out.flush();
		if let Some((_, pcm, path)) = self.wav.take() {
			let _ = std::fs::write(&path, wav_bytes(&pcm));
		}
	}
}

/// A 48 kHz 16 bit stereo WAV file.
fn wav_bytes(pcm: &[i16]) -> Vec<u8> {
	let n = (pcm.len() * 2) as u32;
	let mut w = Vec::with_capacity(44 + n as usize);
	w.extend_from_slice(b"RIFF");
	w.extend_from_slice(&(36 + n).to_le_bytes());
	w.extend_from_slice(b"WAVEfmt ");
	w.extend_from_slice(&16u32.to_le_bytes());
	w.extend_from_slice(&1u16.to_le_bytes());
	w.extend_from_slice(&2u16.to_le_bytes());
	w.extend_from_slice(&48000u32.to_le_bytes());
	w.extend_from_slice(&(48000u32 * 4).to_le_bytes());
	w.extend_from_slice(&4u16.to_le_bytes());
	w.extend_from_slice(&16u16.to_le_bytes());
	w.extend_from_slice(b"data");
	w.extend_from_slice(&n.to_le_bytes());
	for s in pcm {
		w.extend_from_slice(&s.to_le_bytes());
	}
	w
}
