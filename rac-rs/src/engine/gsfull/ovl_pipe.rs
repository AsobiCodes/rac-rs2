//! The GS threads of a game whose world the engine draws (R&C2; from lane rc2's window): like `pipe::start` (a VIF1 + VU1
//! stage, then the software GS and the CRTC read), with the HUD overlay mode per field (overlay.rs): an overlay field runs
//! only the effect programs and the HUD, and hands out the see-through HUD picture and the effects' depth, which the host lays
//! over the engine's world picture.
use crate::gscap::{self, Hd};
use crate::gsfull::crtc::{Crtc, present, present_overlay};
use crate::gsfull::grab::Grab;
use crate::gsfull::gs::Gs;
use crate::gsfull::overlay::{OvlField, raster};
use crate::gsfull::pipe::{Pipe, threads};
use ee_oracle::gs::Job;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Arc, Mutex};

/// One EE field: its number (the host's count, for `GsPipe::done`), its graphics jobs, the CRTC registers at its end, and
/// whether the GS draws it as a HUD overlay.
pub struct FieldIn {
	pub field: u64,
	pub jobs: Vec<Job>,
	pub crtc: Crtc,
	pub overlay: bool,
	/// The grabbed-backdrop area of this field (grab.rs): the pack sets it while the window shows the native picture; None = off.
	pub grab: Option<Grab>,
	/// [b46 U4] The game is in R&C2's ship flight between planets (mode 6): the field's EE-side draws of the cosmos count as overlay
	/// pixels (`OvlField::space_from`). False for every other field and game.
	pub space: bool,
}

/// One presented picture: the shown buffer, or (overlay) the premultiplied HUD and per pixel the effects' view depth in
/// metres (0 = always in front; empty = none).
pub struct Frame {
	/// The EE field this picture was drawn for (`FieldIn::field`; the host keeps the camera of each field by this number).
	pub field: u64,
	pub w: u32,
	pub h: u32,
	pub rgba: Vec<u8>,
	pub overlay: bool,
	pub depth: Vec<f32>,
	/// The HUD again at the window's resolution (the GPU capture, gscap/rc2.rs; RGBA8, alpha = how much of the picture below shows
	/// through): `rgba` then holds the effects only. None: `rgba` has the HUD too.
	pub hd: Option<Hd>,
	/// The field is a frozen-screen one (grab.rs: a pause or title menu over the picture the game froze): the effects plane `rgba` is
	/// empty then (the menu is the HUD), so the host keeps the last live plane under it (rac2/host/frame.rs `Held`).
	pub frozen: bool,
	/// The field runs none of the VU1 programs that draw the level (`viewer::no_world::field_draws_world`) and is not a frozen menu: the game draws
	/// no world in it (a minigame over a black clear), so the native picture must not show the level either.
	pub no_world: bool,
	/// The world layers of the field that the engine draws itself (`layer_tris.rs`; empty unless enabled).
	pub layers: Vec<crate::gsfull::layer_tris::LayerTri>,
}

#[derive(Default)]
pub struct GsPipe {
	slot: Mutex<Option<Frame>>,
	/// The number of the newest field both stages are through (its picture, if it drew one, is in the slot or was taken).
	pub done: AtomicU64,
	/// A thread ended (a panic or the channel closed).
	pub dead: AtomicBool,
	/// The first field of the frozen-screen run the GS is in (grab.rs: overlay fields that upload into the grabbed area), 0 = none.
	pub frozen_from: AtomicU64,
	/// The quick save's share of the pipe (ovl_snap.rs).
	pub snap: crate::gsfull::ovl_snap::Snap,
}

impl GsPipe {
	/// The newest finished picture (each is handed out once).
	pub fn take(&self) -> Option<Frame> {
		self.slot.lock().ok()?.take()
	}
}

/// Marks the shared state dead when a stage's thread ends.
struct Flag(Arc<GsPipe>);
impl Drop for Flag {
	fn drop(&mut self) {
		self.0.dead.store(true, Ordering::SeqCst);
	}
}

/// Starts the two stages (cold boot: blank GS local memory). The sender takes one `FieldIn` per EE field; one with no jobs
/// draws nothing. `sharp`: the GPU capture draws the HUD of overlay fields again at the window's resolution (`Frame::hd`; the
/// caller has called `gscap::gpu_expect_window` before, and `gscap::set_hd_target` follows the window every redraw).
pub fn start(sharp: bool) -> Option<(SyncSender<FieldIn>, Arc<GsPipe>)> {
	start_from(sharp, "")
}

/// `start` for a machine resumed from a savestate dir (rac2/states.rs): GS local memory from its GS.bin, VU1 from its vu1 files. "" = cold boot.
pub fn start_from(sharp: bool, state: &str) -> Option<(SyncSender<FieldIn>, Arc<GsPipe>)> {
	let vram = if state.is_empty() { vec![0u8; crate::gsfull::gs_mem::VRAM_SIZE] } else { Gs::load_vram(&format!("{state}/GS.bin"))? };
	let mut pipe = Pipe::new(state)?;
	let shared = Arc::new(GsPipe::default());
	let (tx, rx) = sync_channel::<FieldIn>(1);
	let (ftx, frx) = sync_channel::<(Option<OvlField>, bool, u64, Option<Grab>)>(1);
	let sh = shared.clone();
	std::thread::Builder::new()
		.name("gsfull-vu1".into())
		.stack_size(64 << 20)
		.spawn(move || {
			let _flag = Flag(sh);
			while let Ok(inp) = rx.recv() {
				// A field with no jobs draws nothing; it still passes the GS stage for `done`.
				let mut f = (!inp.jobs.is_empty()).then(|| pipe.process_rc2(inp.jobs, inp.crtc, inp.overlay, inp.grab, inp.field));
				// [b46 U4] The flight's cosmos: from the first 8-bit (PSM 19) texture upload of the field on, the draws are overlay pixels.
				if let Some(o) = f.as_mut().filter(|o| inp.space && inp.overlay && !crate::viewer::no_world::field_draws_world(&o.f.segs)) {
					o.space_from = o.images.iter().find(|i| i.1 == 19).map(|i| i.2);
				}
				if _flag.0.snap.wants(inp.field) {
					_flag.0.snap.put_vu1(&pipe.vu);
				}
				if ftx.send((f, inp.overlay, inp.field, inp.grab)).is_err() {
					break;
				}
			}
		})
		.ok()?;
	let sh = shared.clone();
	std::thread::Builder::new()
		.name("gsfull-gs".into())
		.stack_size(64 << 20)
		.spawn(move || {
			let _flag = Flag(sh.clone());
			let mut g = Gs::new(vram.clone(), threads());
			// The GPU capture's twin of local memory (gscap/rc2.rs): fed every field once it is attached, dropped when there is no GPU for it.
			let mut cap = sharp.then(|| gscap::Gs::new(vram));
			// Rows of the HUD buffer (its scissor) in the last overlay field: the sharp scale of the next one.
			let mut rows = 448;
			// The sharp scale follows these rows from the first overlay field on (before it, `set_hd_target` assumes the PAL frame's 512).
			if sharp {
				gscap::hd_expect_rows(rows);
			}
			while let Ok((f, overlay, field, grab)) = frx.recv() {
				let Some(mut f) = f else {
					if sh.snap.wants(field) {
						sh.snap.put_vram(g.vram_effective());
					}
					sh.done.store(field, Ordering::SeqCst);
					continue;
				};
				if cap.as_ref().is_some_and(|c| !c.rc2_live()) {
					cap = None;
				}
				// A frozen-screen field (grab.rs): its uploads into the area are dropped, its HUD starts after the first one.
				g.set_grab(grab);
				f.hud_from = grab.and_then(|gr| gr.hud_from(&f.images));
				// (no camera for the field's effects: they stay in the overlay as well, `layer_tris::note_camera`)
				g.set_layers_frozen(f.hud_from.is_some() || !crate::gsfull::layer_tris::has_camera(field));
				if f.hud_from.is_none() {
					sh.frozen_from.store(0, Ordering::SeqCst);
				} else if sh.frozen_from.load(Ordering::SeqCst) == 0 {
					sh.frozen_from.store(field, Ordering::SeqCst);
				}
				let fxw = std::mem::take(&mut f.fxw);
				// texreq's clock: wants of a class that switched words at a later field wait for that field's draws.
				crate::gsfull::texreq::gs_field(field);
				let sharp = overlay && cap.as_ref().is_some_and(|c| c.rc2_hd_ready(rows));
				g.set_hud_skip(sharp);
				// The capture takes the field on its own thread beside the software GS (same qwords; neither changes what the
				// other draws), so its CPU work and the GPU round trip hide behind the software raster. No thread: in turn.
				let r = std::thread::scope(|s| {
					let c = cap.as_mut().map(|c| std::thread::Builder::new().name("gsfull-cap".into()).stack_size(16 << 20).spawn_scoped(s, || c.rc2_field(&f, overlay, rows, sharp)));
					raster(&mut g, &f, overlay, fxw);
					crate::gsfull::texreq::serve(&mut g);
					c.map(|j| j.map(|j| j.join()))
				});
				let hd = match r {
					Some(Ok(Ok(hd))) => hd,
					// The capture panicked: the software GS draws the HUD again from the next field on.
					Some(Ok(Err(_))) => {
						cap = None;
						None
					}
					_ => cap.as_mut().and_then(|c| c.rc2_field(&f, overlay, rows, sharp)),
				};
				let frozen = f.hud_from.is_some();
				let no_world = overlay && !frozen && !crate::viewer::no_world::field_draws_world(&f.f.segs);
				let crtc = f.f.crtc;
				let frame = if overlay {
					let (p, depth) = present_overlay(&mut g, &crtc);
					let (cw, ch) = g.overlay_clip();
					if cw > 0 && ch > 0 {
						// 512 GS pixels wide is what the sharp scale assumes (anything else: no sharp picture).
						rows = if cw == 512 { ch.clamp(64, 512) } else { 0 };
						if rows != 0 {
							gscap::hd_expect_rows(rows);
						}
					}
					Frame { field, w: p.w, h: p.h, rgba: p.rgba, overlay, depth, hd, frozen, no_world, layers: g.take_layers() }
				} else {
					let p = present(&mut g, &crtc);
					Frame { field, w: p.w, h: p.h, rgba: p.rgba, overlay, depth: Vec::new(), hd: None, frozen: false, no_world: false, layers: Vec::new() }
				};
				if let Ok(mut s) = sh.slot.lock() {
					*s = Some(frame);
				}
				if sh.snap.wants(field) {
					sh.snap.put_vram(g.vram_effective());
				}
				sh.done.store(field, Ordering::SeqCst);
			}
		})
		.ok()?;
	Some((tx, shared))
}
