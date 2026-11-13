//! Hidden mode (`--shots DIR`): the game field by field on this thread with VIF1 + VU1 + GS in step, pictures to DIR with their
//! hashes on stderr (the same `field N: WxH hash H` lines as the old standalone rac2-play's emulated picture and lane rc2).
use super::boot;
use crate::gsfull::crtc::{Crtc, present};
use crate::gsfull::gs::Gs;
use crate::gsfull::overlay::raster;
use crate::gsfull::pipe::Pipe;
use ee_oracle::gs::Job;

#[derive(Default)]
pub struct Opts {
	pub fields: u64,
	pub every: u64,
	pub at: Vec<u64>,
	pub pad: Vec<(u64, u64, u16)>,
	pub stick: Vec<(u64, u64, u16)>,
	pub replay: Option<String>,
}

/// --replay REC: a window session's recording: its pad per field, a copy of the memory card it started with in `dir` (saves
/// made during the replay go to the copy) and the TV shape it booted with (REC/screen.txt, the OSD screen type; none = 0, 4:3:
/// the recordings made before 2026-10-27). A recording with REC/state resumes from it (rec_state.rs; lib.rs sets the start).
pub fn load_replay(r: &str, dir: &str) -> Result<Vec<(u16, [u8; 4])>, String> {
	let v = boot::read_pad(&format!("{r}/pad.bin"))?;
	let screen = std::fs::read_to_string(format!("{r}/screen.txt")).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
	ee_oracle::cold::set_aspect(screen);
	let card = format!("{dir}/replay_card.mcd");
	let _ = std::fs::remove_file(&card);
	if std::fs::copy(format!("{r}/card.mcd"), &card).is_err() {
		eprintln!("replay: no card.mcd, blank card");
	}
	ee_oracle::mc::set_persist(Some(card));
	let cam = crate::modcam_rec::load(r);
	eprintln!("replay: {} fields recorded, camera input of {cam} fields (cam.bin)", v.len());
	Ok(v)
}

/// An RGBA picture as a PNG (alpha dropped).
pub fn write_png(path: &str, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
	let rgb = rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
	std::fs::write(path, crate::render::png(&crate::render::Image { w: w as usize, h: h as usize, rgb })).map_err(|e| format!("{path}: {e}"))
}

pub fn run(iso: &str, dir: &str, o: &Opts) -> Result<(), String> {
	std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
	let rec_pad = match &o.replay {
		Some(r) => load_replay(r, dir)?,
		None => Vec::new(),
	};
	let fields = if o.replay.is_some() { o.fields.min(rec_pad.len() as u64) } else { o.fields };
	// A start state (a recording's REC/state via lib.rs, or RAC2_STATE): the machine, VU1 and GS memory and the Modern camera from it.
	let state = crate::states::start_dir().unwrap_or_default();
	let (mut m, rx) = if state.is_empty() { boot::boot(iso)? } else { boot::boot_state(iso, &state)? };
	if !state.is_empty() {
		crate::states::load_cam(&state);
	}
	let mut pipe = Pipe::new(&state).ok_or("pipe (a state dir needs vu1Memory.bin and vu1MicroMem.bin)")?;
	let vram = if state.is_empty() { Some(vec![0u8; crate::gsfull::gs_mem::VRAM_SIZE]) } else { Gs::load_vram(&format!("{state}/GS.bin")) };
	let mut g = Gs::new(vram.ok_or("GS.bin of the start state")?, crate::gsfull::pipe::threads());
	let t0 = std::time::Instant::now();
	let ramhash = std::env::var_os("RAC2_RAMHASH").is_some();
	let mut last_hash = 0u64;
	let nogfx = std::env::var_os("RAC2_NOGFX").is_some();
	let mut snd = super::sndlog::SndLog::from_env(&mut m);
	// RAC2_NOGS=1: the GS jobs are dropped unseen (RAM-only and sound runs go several times faster; no pictures).
	let nogs = std::env::var_os("RAC2_NOGS").is_some();
	let mut unwind_wait = 0u32; // [b46 W6] fields a due unwind has waited for a vsync wait (unwind.rs, as host/game.rs)
	for field in 0..fields {
		match rec_pad.get(field as usize) {
			Some(&(b, s)) => m.set_pad(b, s),
			None => {
				let b = o.pad.iter().filter(|p| (p.0..=p.1).contains(&field)).fold(0u16, |b, p| b | p.2);
				let l = o.stick.iter().find(|p| (p.0..=p.1).contains(&field)).map_or(0x8080, |p| p.2);
				m.set_pad(b, [0x80, 0x80, (l >> 8) as u8, l as u8]);
			}
		}
		if let Some(n) = boot::level_override() {
			boot::apply_level_override(&mut m, n);
		}
		if boot::wide_override() {
			boot::apply_wide(&mut m);
		}
		crate::ramdump::set_field(field);
		if let Err(e) = m.frame() {
			eprintln!("field {field}: {e} (pc {:08x})", m.cpu.pc);
			break;
		}
		// [b46 W6] The window's step (host/game.rs): only while parked in a vsync wait, forced after 500 fields; RAM unchanged.
		let parked = crate::unwind::parked(m.cpu.pc, m.cpu.vblank_pc, ee_oracle::ver::addrs().poll_head);
		let (due, now) = crate::unwind::step_logged(unwind_wait, (field + 1) as u32, parked, m.cpu.pc);
		unwind_wait = due;
		if now {
			m.cpu.rec_unwind = true;
		}
		crate::ramdump::after_field(&m, field + 1);
		if let Some(s) = snd.as_mut() {
			s.field(&mut m, field);
		}
		if ramhash && (field + 1) % 250 == 0 {
			eprintln!("ramhash field {} {:016x}", field + 1, boot::ram_hash(&m));
		}
		if nogs {
			rx.try_iter().for_each(drop);
			continue;
		}
		let jobs: Vec<Job> = rx.try_iter().collect();
		let crtc = Crtc::from_regs(&m.cpu.mem.gs);
		let n = jobs.len();
		// RAC2_NOGFX=1: no GS work at all (RAM hunts: the game's RAM does not depend on it); only the last field draws a picture.
		if nogfx && field + 1 != fields {
			continue;
		}
		if n > 0 {
			let f = pipe.process_rc2(jobs, crtc, false, None, 0);
			raster(&mut g, &f, false, Vec::new());
			crate::gsfull::texreq::serve(&mut g);
		}
		if (o.every > 0 && field % o.every == 0) || o.at.contains(&field) || field + 1 == fields {
			let pic = present(&mut g, &crtc);
			let h = crate::gsfull::pipe::hash(&pic.rgba);
			write_png(&format!("{dir}/f{field:05}.png"), pic.w, pic.h, &pic.rgba)?;
			let same = if h == last_hash { " (same)" } else { "" };
			eprintln!("field {field}: {}x{} hash {h:016x}{same} jobs {n} vu1 errors {} ({:.1}s)", pic.w, pic.h, pipe.vu.st.errors, t0.elapsed().as_secs_f64());
			last_hash = h;
		}
	}
	if let Some(s) = snd {
		s.finish();
	}
	eprintln!("done {fields} fields in {:.1} s", t0.elapsed().as_secs_f64());
	Ok(())
}
