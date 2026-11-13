//! The R&C2 window (engine split stage E5, 2026-10-27; from lane rc2's window.rs, whose own renderer and GS pipe it replaces):
//! the game runs on a thread of its own at the PAL field rate (`host/game.rs`), its fields go through the engine's GS threads
//! in HUD overlay mode (gsfull/ovl_pipe.rs), and the engine renderer draws the level from the disc's data with the game's
//! camera, mobies, metal, tie colours and shadows from RAM, the game's HUD and effects from the GS over it (`host/frame.rs`).
//! Without a 3D camera or before the level's world is decoded (menus, loads) the GS's whole picture shows. Both are shown in
//! the middle of the window at the game's picture shape with black bars: 16:9 with the game's own Options row "16:9
//! (widescreen)", on from the start (lib.rs `main`: the game draws and culls for the wide view itself), or 4:3 when the
//! player turns it off (`scene::View::aspect`). Input: the
//! engine's keyboard bindings and gamepads (engine/input), left / middle mouse button = Circle / Triangle; in retail menus the Menu
//! layer (kbm_table.rs, pc_input.rs). F1 flips to the
//! emulated picture and back (RAC2_PICTURE=gs starts with it).
//!
//! Window shots (`--window-shots DIR`, test runs): the game and the window step in lockstep (each field is drawn before the
//! next one runs; a level's world is decoded before the field after the game asked for it), unpaced and silent, so the
//! pictures of a replay are the same run to run: DIR/wNNNNN.png at the fields `--every` / `--at` / the last one ask for, each
//! with a `field N: WxH hash H` line on stderr.

mod cursor;
mod debug_route;
mod frame;
pub(crate) mod framelog;
mod gs_sync;
mod livecap;
mod menu;
mod screens;
mod screens_ship;
mod screens_grid;
mod screens_load; // [PC] ld lane
mod screens_items;
mod screens_qs;
mod screens_switch;
mod screens_preview; // [PC] fx1: the pages' preview hole never shows the world
mod screens_vendor;
mod screens_map;
mod screens_optlist; // [b45 P4B]
pub(crate) mod screens_hold;
mod freeze;
mod display_res; // [b45 P2] Display > Resolution, window side
mod game;
pub(crate) mod sys_window; // [b45 P1] keep-awake, live display mode, icon
pub(crate) mod sys_crash; // [b45 P1] panic hook, crash.log, message box
mod movie;
mod ov_hold;
mod ov_fxlog;
mod ov_blend;
mod ov_layers;
mod ov_fire;
mod ov_lava; // [b47 X2] heat haze over the lava sheet

use crate::scene::Scene;
use crate::shots::Opts;
use crate::world::World;
use rac_engine::gsfull::ovl_pipe::GsPipe;
use rac_engine::viewer::Gpu;
use rac_engine::viewer::pace::Pace;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

/// A number one thread raises and another waits for (lockstep).
#[derive(Default)]
struct Gate {
	v: Mutex<u64>,
	cv: Condvar,
}

impl Gate {
	fn set(&self, v: u64) {
		if let Ok(mut g) = self.v.lock() {
			*g = v;
		}
		self.cv.notify_all();
	}

	/// Waits until `ok` holds; false when `quit` was set meanwhile.
	fn wait(&self, ok: impl Fn(u64) -> bool, quit: &AtomicBool) -> bool {
		let Ok(mut g) = self.v.lock() else { return false };
		while !ok(*g) {
			if quit.load(Ordering::Relaxed) {
				return false;
			}
			g = match self.cv.wait_timeout(g, Duration::from_millis(100)) {
				Ok(r) => r.0,
				Err(_) => return false,
			};
		}
		true
	}
}

/// What the window and the game thread share.
#[derive(Default)]
struct Shared {
	/// The window's pad report: buttons << 32 | sticks (rx ry lx ly).
	pad: AtomicU64,
	quit: AtomicBool,
	/// The game thread stopped (its error is printed there, or the run's fields are done).
	over: AtomicBool,
	/// The game's camera, level and mobies of the newest field.
	view: Mutex<Scene>,
	/// The cameras of the last few fields by field number (the GS picture of a field is placed with its own camera, not the newest).
	views: Mutex<ov_layers::Views>,
	/// A decoded world the window has not taken yet.
	world: Mutex<Option<World>>,
	/// The number of requests the world loader is through (decoded or failed; counts the ones a newer request replaced).
	loaded: Gate,
	/// The window draws the engine's world: the GS runs no world VU1 programs and draws only the HUD and effects, as an overlay.
	overlay: AtomicBool,
	/// The window shows the native picture (set at start, flipped by F1): the game thread then names the grabbed-backdrop area (`addrs::GRAB`).
	native: AtomicBool,
	/// Lockstep: the newest field (counted from 1) the window drew.
	drawn: Gate,
	/// F5: the game thread saves the machine at the end of its next field (states.rs).
	save_req: AtomicBool,
	/// The newest field has a 3D camera (gameplay, not a menu or load): the Modern camera's cursor grab with `follow_ran_recently` (host/cursor.rs).
	playing: AtomicBool,
}

struct App {
	gpu: Option<Gpu>,
	stage: frame::Stage,
	gs: Arc<GsPipe>,
	shared: Arc<Shared>,
	held: HashSet<KeyCode>,
	/// Left, middle, right button (Circle, Triangle, R2: the strafe aim, in every camera mode).
	mouse: [bool; 5],
	/// The window has the keyboard focus.
	focused: bool,
	/// `RAC_TEST_WINDOW=hidden`: an agent's off-screen test window (never grabs the cursor).
	test_window: bool,
	/// The Modern camera's cursor grab (host/cursor.rs).
	cursor: cursor::Cursor,
	/// The engine's world on (F1 flips).
	native: bool,
	/// The newest GS picture is a HUD overlay.
	ov_overlay: bool,
	/// The game's picture shape (`View::aspect`; 4:3 until the first 3D view).
	aspect: f32,
	/// Window shots (lockstep).
	shots: Option<frame::Shots>,
	/// Frame smoothing (smooth.rs), and in window shots the newest field drawn and how many of its frames (RAC_TEST_SUB).
	smooth: crate::smooth::Smooth,
	sub_at: (u64, u32),
	/// The world under a frozen-screen menu (host/freeze.rs).
	freeze: freeze::Freeze,
	/// The last live effects plane, kept under a frozen-screen menu (host/ov_hold.rs).
	ov_hold: ov_hold::Held,
	/// The world layers of the newest overlay frame in the engine (host/ov_layers.rs) and the frame's, until the camera is read.
	ov_layers: ov_layers::Layers,
	layers_new: Option<(bool, bool, u64, Vec<rac_engine::gsfull::layer_tris::LayerTri>)>,
	/// The camera the newest live (not frozen) overlay picture was drawn with: its effects are warped from it (None: use the newest).
	ov_view: Option<crate::scene::View>,
	pace: Pace,
	/// The disc image (movies stream from it), the movie playing (host/movie.rs) and its sound output.
	iso: String,
	playing: Option<movie::Playing>,
	movie_audio: Option<rac_engine::audio::Audio>,
	/// The scene drawn: the one of the newest GS picture (host/gs_sync.rs).
	sync: gs_sync::SceneSync,
	/// Test: RAC2_LIVECAP (host/livecap.rs).
	livecap: Option<livecap::LiveCap>,
	/// The mouse cursor in window pixels (the modern Options page's clicks, host/menu.rs).
	ui_pos: Option<(f32, f32)>,
	/// The window is borderless fullscreen (the Display mode follows `display=` live, host/sys_window.rs).
	fullscreen: bool,
}

impl App {
	/// The quick save message (states.rs `note`: queued / waiting for the card / saved / failed) in the window title for a few
	/// seconds, then the usual title; the player has no console.
	fn show_save_note(&mut self) {
		static SHOWN: Mutex<String> = Mutex::new(String::new());
		let Some(g) = &self.gpu else { return };
		// [PC] Debug mode adds its readout and events (host/debug_route.rs).
		let want = debug_route::title_note().map_or_else(|| title(self.native).to_string(), |m| format!("Ratchet & Clank 2 (rac2-play) - {m}"));
		if let Ok(mut shown) = SHOWN.lock() {
			if *shown != want && !(shown.is_empty() && want == title(self.native)) {
				g.window.set_title(&want);
			}
			*shown = want;
		}
	}

	/// [b48 Y5] `gate` = the field a field-locked feed run (pc_devfeed.rs, `f` entries) is asked for: that run polls once per field, for
	/// that field; without `f` entries `gate` is None and the poll is the old one (newest field, every redraw).
	fn poll_input(&self, gate: Option<u64>) {
		let lockstep_feed = crate::pc_devfeed::lockstep_locked(); // [b50 V6]
		if crate::pc_devfeed::locked() && gate.is_none() && !lockstep_feed {
			return; // already answered this field
		}
		let field = gate.unwrap_or_else(|| self.shared.view.lock().map_or(0, |v| v.field));
		// [b50 V6] A window-shots run has no gate: its redraw is the field's poll (view.field = the field the game runs next), once per field.
		if gate.is_none() && lockstep_feed && !crate::pc_devfeed::first_poll(field) {
			return;
		}
		self.poll_input_at(field);
		if let Some(g) = gate {
			crate::pc_devfeed::answer(g);
		}
	}

	fn poll_input_at(&self, field: u64) {
		use rac_engine::input::kbm::{self, Input, Mouse};
		// [PC] Dev: RAC2_INPUT_FEED scripts keys, the left stick, raw mouse motion and captures (pc_devfeed.rs).
		crate::pc_devfeed::poll(field);
		// [b46 U1] Bug B: the feed's `mouse:` motion takes the real mouse's route (it reached only the cursor's visibility before).
		if let Some((dx, dy)) = crate::pc_devfeed::motion(field) {
			self.mouse_motion(dx, dy, true, !rac_engine::ui::is_open() && self.grab_rule(false));
		}
		debug_route::feed(field); // [PC] Debug mode's test feed (RAC2_DEVTOOLS_FEED)
		let fed = crate::pc_devfeed::held(field);
		let mut taps = crate::pc_taps::TAPS.lock().unwrap_or_else(|e| e.into_inner()); // [b45 P6]
		taps.poll(field as u64);
		// [b46 U1] Bug A sweep: the Classic vendor tells a click's Cross from a key's by the left button (pc_vendor_hover.rs `filter`:
		// Exit plate = Triangle, a click mid-walk buys on arrival). A short click lives on in the latch after the release: still a click.
		crate::pc_vendor_hover::left(self.mouse[0] || taps.alive(Input::Mouse(Mouse::Left)));
		let raw = |i: Input| fed.contains(&i) || match i {
			Input::Key(k) => self.held.contains(&k),
			Input::Mouse(Mouse::Left) => self.mouse[0],
			Input::Mouse(Mouse::Middle) => self.mouse[1],
			Input::Mouse(Mouse::Right) => self.mouse[2],
			Input::Mouse(Mouse::Back) => self.mouse[3],
			Input::Mouse(Mouse::Forward) => self.mouse[4],
			Input::Wheel(w) => crate::pc_controls::wheel_active(w),
			_ => false,
		} || taps.alive(i); // [b45 P6] C5: a press between two polls counts for TAP_FIELDS fields (pc_taps.rs)
		// [PC] A retail menu screen (pc_input.rs): the Menu layer of R&C2's table (kbm_table.rs); a key held across the switch waits.
		let menu = crate::pc_input::menu_ctx();
		let Ok(mut latch) = crate::kbm_table::LATCH.lock() else { return };
		let all: Vec<Input> = self.held.iter().map(|&k| Input::Key(k)).chain([Mouse::Left, Mouse::Middle, Mouse::Right, Mouse::Back, Mouse::Forward].map(Input::Mouse)).chain([kbm::Wheel::Up, kbm::Wheel::Down].map(Input::Wheel)).collect();
		latch.update(menu.is_some(), &raw, &all);
		let down = |i: Input| raw(i) && latch.counts(i);
		let o = kbm::eval(kbm::table(), &kbm::binds(), &down, &kbm::Ctx { contexts: crate::pc_input::contexts(), menu }); // [b45 P6] C6: the ship fight context
		static PC_PREV: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0); // [b45 P6]
		let z = crate::pc_taps::zoom_edge(PC_PREV.swap(o.pc, Ordering::Relaxed), o.pc); // Zoom on a key / button: one notch per press
		if z != 0.0 && self.cursor.grabbed() && menu.is_none() && !crate::pc_input::owns_motion() {
			crate::modcam_ui::ui_zoom(z);
		}
		let (mut b, mut s) = (o.buttons, o.sticks);
		// The camera keys' share of the right stick (keyboard only, before the gamepad is merged in): modcam_ui.rs.
		if crate::modcam_ui::modern() {
			crate::modcam_ui::ui_keys([(s[0] as i16 - 0x80) as i8, (s[1] as i16 - 0x80) as i8]);
		}
		// The controller: the buttons follow the player's remap (the generic table's actions, identity by default), the sticks stay as they are.
		let xs = crate::pc_devfeed::stick(field).map(|(lx, ly)| (rac_engine::input::pad::map_xinput(&rac_engine::input::pad::XRaw { lx, ly, ..Default::default() }), 0));
		let scripted_pad = xs.is_some();
		let pad = xs.or_else(rac_engine::input::pad::read);
		crate::modern_open::note_real_pad(pad.as_ref().map(|(p, _)| p)); // [b46 U6] the Options page's pad
		debug_route::pad(pad.as_ref().map(|(p, _)| p)); // [PC] Debug mode's free camera / planet list buttons
		if let Some((p, phys)) = pad {
			if !scripted_pad {
				sys_window::pad_activity(&p); // [b45 P1] a real pad keeps the screen awake (engine/awake.rs)
			}
			crate::pc_prompts::note_pad(&p); // [PC] real pad input: the prompts show the pad's glyphs (pc_prompts.rs)
			// [b45 P6] C16: the player's controller layout in play, the standard one in menus; a pad capture on the Controls tab binds the
			// next button and the game gets none meanwhile. A Zoom action on a controller button zooms one notch per press.
			static PHYS_PREV: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
			static PAD_PC: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
			let mut prev = PHYS_PREV.load(Ordering::Relaxed);
			let capturing = crate::pad_binds::poll_capture(phys, &mut prev);
			PHYS_PREV.store(prev, Ordering::Relaxed);
			let pe = kbm::eval_pad(kbm::table(), &crate::pad_binds::game_binds(menu.is_some()), phys, None);
			let zp = crate::pc_taps::zoom_edge(PAD_PC.swap(pe.pc, Ordering::Relaxed), pe.pc);
			if zp != 0.0 && self.cursor.grabbed() && menu.is_none() && !crate::pc_input::owns_motion() {
				crate::modcam_ui::ui_zoom(zp);
			}
			let buttons = if capturing { 0 } else { pe.buttons };
			(b, s) = rac_engine::input::pad::merge((b, s), &rac_engine::input::pad::PadState { buttons, sticks: p.sticks });
		}
		self.shared.pad.store((b as u64) << 32 | u32::from_le_bytes(s) as u64, Ordering::Relaxed);
	}

	/// The newest field has a 3D camera and no movie plays.
	fn in_play(&self) -> bool {
		self.shared.playing.load(Ordering::Relaxed) && self.playing.is_none()
	}

	/// [b46 U1] Raw mouse motion (real: `device_event`; the dev feed's `mouse:` entries: `poll_input`). `focused` / `held` = the window
	/// has the focus / holds the cursor. The feed passes the hold RULE (`grab_rule` without the test-window and focus terms), so a hidden
	/// run drives the ship, the Quick Select wheel, the Modern camera and first person exactly as a held real mouse does.
	fn mouse_motion(&self, dx: f64, dy: f64, focused: bool, held: bool) {
		rac_engine::cursor_vis::raw_motion(dx, dy);
		// [PC] The free camera looks with the mouse (Debug mode, pc_freecam.rs): the Modern camera does not turn meanwhile.
		if focused && crate::pc_freecam::mouse(dx as f32, dy as f32) {
			return;
		}
		// [PC] The Quick Select wheel and the ship fights take the mouse (pc_input.rs); the camera waits meanwhile.
		if focused {
			crate::pc_input::motion(dx as f32, dy as f32);
			crate::pc_prompts::note_motion(dx, dy);
		}
		if held && !crate::pc_input::owns_motion() {
			crate::modcam_ui::ui_mouse(dx as f32, dy as f32);
		}
	}

	/// [b46 U1] The cursor hold rule of `update_cursor` (host/cursor.rs `want_grab`). `real` = this window's focus, test-window and
	/// window-shot terms; false = the game side of the rule alone (the dev feed's hidden runs: focused, no test window, no shots).
	fn grab_rule(&self, real: bool) -> bool {
		let (focused, test_window, shots) = if real { (self.focused, self.test_window, self.shots.is_some()) } else { (true, false, false) };
		cursor::want_grab(crate::modcam_ui::modern(), focused, self.in_play(), crate::modcam_hand::follow_ran_recently(crate::modcam_ui::current().0), crate::pc_input::owns_motion() || crate::modcam_fp::holds() /* [b45 P5] first person looks with the mouse, Classic too; [b49 U5b] from the look mode on */, test_window, shots)
	}

	/// Holds and hides the cursor while the Modern camera plays, lets it go otherwise (host/cursor.rs).
	fn update_cursor(&mut self) {
		// The modern Options page (host/menu.rs) needs the cursor.
		// [PC] The free camera (Debug mode, pc_freecam.rs) looks with the mouse.
		let free = crate::pc_freecam::on() && self.focused && self.shots.is_none() && !self.test_window;
		let rule = self.grab_rule(true);
		RULE.store(rule, Ordering::Relaxed); // [b50 V3] read by `device_event` (review finding 8)
		let want = !rac_engine::ui::is_open() && (free || rule); // [b46 U1] the rule shared with the dev feed
		let was = self.cursor.grabbed();
		if let Some(g) = &self.gpu {
			self.cursor.update(&g.window, want);
		}
		self.log_hold(want);
		if was && !self.cursor.grabbed() {
			crate::modcam_ui::ui_release();
		}
		// [PC] The OS cursor shows only while the mouse is the device in use (engine/cursor_vis.rs; parity row 50).
		if let Some(g) = &self.gpu {
			rac_engine::cursor_vis::apply(&g.window, self.cursor.grabbed(), false);
		}
	}

	/// [b49 U5b / b50 V3] Find-out for the live first-person mouse (the user's log.txt): one line whenever a term of the hold changes, and
	/// two more while the first-person key (pad L1) is held (3 and 40 redraws after it went down), so a line carries the state of the
	/// transition. Names every gate: the `want` terms, the OS grab, the first-person chain (`fp_gate`: the first gate that says no, with
	/// the hero camera word, the camera class and the game mode word), and the raw mouse events seen / not held. At most 300 lines.
	fn log_hold(&self, want: bool) {
		use std::sync::atomic::AtomicU32;
		static LAST: AtomicU32 = AtomicU32::new(u32::MAX);
		static LINES: AtomicU32 = AtomicU32::new(0);
		static L1_REDRAWS: AtomicU32 = AtomicU32::new(0);
		let l1 = (self.shared.pad.load(Ordering::Relaxed) >> 32) & 0x04 != 0;
		let n = if l1 { L1_REDRAWS.fetch_add(1, Ordering::Relaxed) + 1 } else { L1_REDRAWS.swap(0, Ordering::Relaxed) & 0 };
		let field = crate::modcam_ui::current().0;
		let t = [want, self.cursor.grabbed(), crate::modcam_ui::modern(), crate::modcam_hand::follow_ran_recently(field), crate::modcam_fp::live(), crate::modcam_fp::holds(), crate::pc_input::owns_motion(), self.in_play(), rac_engine::ui::is_open(), self.focused];
		let bits = t.iter().fold(crate::modcam_fp::gate_code() << 12 | (l1 as u32) << 11, |a, &b| a << 1 | b as u32);
		let changed = LAST.swap(bits, Ordering::Relaxed) != bits;
		if (changed || n == 3 || n == 40) && LINES.fetch_add(1, Ordering::Relaxed) < 300 {
			let held = if n > 0 { format!(" (l1 held {n} redraws)") } else { String::new() };
			eprintln!(
				"rac2: cursor hold field {field}: want {} grabbed {} modern {} following {} fp_live {} fp_holds {} owns {} in_play {} ui_open {} focused {} l1 {l1}{held} {} raw_events {} not_held {}",
				t[0], t[1], t[2], t[3], t[4], t[5], t[6], t[7], t[8], t[9],
				crate::modcam_fp::gate_text(),
				RAW_SEEN.load(Ordering::Relaxed),
				RAW_DROPPED.load(Ordering::Relaxed)
			);
		}
	}

	/// [b45 P1] Display mode follows `display=` live (R&C1: frame_start.rs): borderless fullscreen on the current monitor, or back to
	/// the window. A hidden test window only logs.
	fn sync_display_mode(&mut self) {
		let want = crate::options::get().fullscreen_on();
		let Some(f) = sys_window::display_change(want, self.fullscreen) else { return };
		self.fullscreen = want;
		let skip = sys_window::test_window();
		eprintln!("rac2-play: display mode {}: set_fullscreen({f:?}){}", if want { "Fullscreen" } else { "Windowed" }, if skip { " skipped (test window)" } else { "" });
		if let (false, Some(g)) = (skip, &self.gpu) {
			g.window.set_fullscreen(f);
		}
	}

	/// The one way out (the window's close, a menu's Quit to Desktop): the game thread and the movie stop, then the engine's quit
	/// path (engine/quit.rs: achievements flush, capture guard, the loop's exit).
	fn quit(&mut self, el: &ActiveEventLoop) {
		self.shared.quit.store(true, Ordering::Relaxed);
		crate::movie::abort();
		rac_engine::quit::quit_app(|| el.exit());
	}
}

impl ApplicationHandler for App {
	fn resumed(&mut self, el: &ActiveEventLoop) {
		if self.gpu.is_some() {
			return;
		}
		let mut attrs = Window::default_attributes().with_title(title(self.native)).with_inner_size(winit::dpi::LogicalSize::new(1024, 768)).with_window_icon(sys_window::icon()); // [b45 P1] the logo as window / taskbar icon
		// [b45 P2] a saved windowed size is the new window's size.
		if let (false, Some(sz)) = (sys_window::test_window(), display_res::start_size(el)) {
			attrs = attrs.with_inner_size(sz);
		}
		// Agent test runs: off-screen, not activated, not in the taskbar.
		if std::env::var_os("RAC_TEST_WINDOW").is_some_and(|v| v == "hidden") {
			attrs = attrs.with_active(false).with_position(winit::dpi::PhysicalPosition::new(-32000, -32000));
			// RAC_TEST_SIZE=WIDTHxHEIGHT (pixels): the test window's size (the sharp HUD needs a window at least 2 pixels per GS row tall).
			if let Some((w, h)) = std::env::var("RAC_TEST_SIZE").ok().and_then(|v| v.split_once('x').and_then(|(a, b)| Some((a.trim().parse::<u32>().ok()?, b.trim().parse::<u32>().ok()?)))) {
				attrs = attrs.with_inner_size(winit::dpi::PhysicalSize::new(w, h));
			}
			#[cfg(windows)]
			{
				use winit::platform::windows::WindowAttributesExtWindows;
				attrs = attrs.with_skip_taskbar(true);
			}
		} else if crate::options::get().fullscreen_on() {
			// [PC] pc_options `display=fullscreen`: borderless fullscreen on the current monitor (as R&C1's Display mode row).
			attrs = attrs.with_fullscreen(Some(winit::window::Fullscreen::Borderless(None)));
		}
		let window = Arc::new(el.create_window(attrs).expect("window"));
		self.focused = window.has_focus();
		self.fullscreen = window.fullscreen().is_some();
		// [b45 P1] Keep-awake follows the window's focus and minimise state (engine/awake.rs), as R&C1's handler.
		rac_engine::runlog::set_window(&window);
		rac_engine::awake::focus(self.focused);
		rac_engine::awake::minimise(window.inner_size().width == 0 || window.inner_size().height == 0);
		let mut g = Gpu::new(window.clone());
		rac_engine::viewer::gpu::renderer::start_done(); // [PC] the DX12 FXC start-up wait is over (renderer.rs)
		frame::Stage::empty(&mut g);
		self.gpu = Some(g);
		window.request_redraw();
	}

	fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
		// [PC] Keys, mouse buttons, the wheel and clear pointer movement make the prompts name the keys (pc_prompts.rs).
		if self.focused {
			crate::pc_prompts::note_window_event(&event);
		}
		rac_engine::cursor_vis::window_event(&event, self.cursor.grabbed());
		// [PC] Debug mode's planet list takes every key press while open, before every page (host/debug_route.rs).
		if debug_route::list_first(&event) {
			return;
		}
		// [PC] The modern Options page's keys and mouse first (host/menu.rs): what it takes never reaches the game.
		// [PC] The modern pause / title pages (host/screens.rs) before it.
		if self.screen_event(&event) || self.menu_event(&event) {
			return;
		}
		// [b46 U1] Bug A: the tap latch sees only what goes on to the game (build 45 latched before the pages: a click the PC Options
		// page took came back as the menu's Confirm and acted twice). Not gated on focus, as `held` / `mouse` below: a scripted `ui`
		// click (pc_devfeed) in the hidden test window takes the same route as a real one.
		crate::pc_taps::note_event(&event, false);
		match event {
			WindowEvent::CloseRequested => self.quit(el),
			WindowEvent::Resized(s) => {
				if let Some(g) = &mut self.gpu {
					g.resize(s.width, s.height);
				}
				rac_engine::awake::minimise(s.width == 0 || s.height == 0); // [b45 P1]
			}
			WindowEvent::Focused(f) => {
				self.focused = f;
				rac_engine::awake::focus(f); // [b45 P1] a gamepad does not count as activity for Windows: held while focused
				if !f {
					self.held.clear();
					self.mouse = [false; 5];
					crate::modcam_ui::ui_release();
				}
			}
			// [PC] The free pointer over the retail picture: a retail menu row under it is selected (pc_menu_hover.rs).
			WindowEvent::CursorMoved { position, .. } => {
				if let (Some(g), true, false) = (self.gpu.as_ref(), self.focused, self.cursor.grabbed()) {
					let [x, y, w, h] = frame::picture_rect(g.config.width as f32, g.config.height as f32, self.aspect);
					crate::pc_menu_hover::pointer((position.x as f32 - x) / w.max(1.0), (position.y as f32 - y) / h.max(1.0));
				}
			}
			WindowEvent::MouseWheel { delta, .. } => {
				let lines = match delta {
					MouseScrollDelta::LineDelta(_, y) => y,
					MouseScrollDelta::PixelDelta(p) => p.y as f32 / 80.0,
				};
				// [PC] The free camera's speed (Debug mode, pc_freecam.rs).
				if self.focused && crate::pc_freecam::wheel(lines) {
					return;
				}
				// [PC] The wheel steps a retail menu's selection (pc_input.rs); in play it is the Modern camera's zoom. A notch bound
				// on the Controls tab presses only its action.
				// [b45 P6] C6/C12: only a pad/stick action on the wheel taps it; the zoom follows the Zoom rows (default: the wheel) and
				// never while the Quick Select wheel / a ship fight owns the mouse (Quick Select then drives only itself).
				let (bound, zoom) = crate::pc_taps::wheel_route(&rac_engine::input::kbm::binds(), lines);
				if self.focused && bound {
					crate::pc_controls::note_wheel(lines);
				}
				if self.focused && !bound {
					crate::pc_input::wheel(lines);
				}
				if zoom != 0.0 && self.cursor.grabbed() && crate::pc_input::menu_ctx().is_none() && !crate::pc_input::owns_motion() {
					crate::modcam_ui::ui_zoom(zoom);
				}
			}
			WindowEvent::KeyboardInput { event, .. } => {
				if let PhysicalKey::Code(k) = event.physical_key {
					// [PC] Debug mode (host/debug_route.rs): the free camera (F2 and its keys), the split view (F10 split, F11 Hide
					// HUD, F12 labels, [ ] Home \ the divider), F3 draw distance, F6 / F7 / F8 cheats. A taken release still lets go.
					if debug_route::key(k, event.state == ElementState::Pressed, event.repeat) {
						if event.state == ElementState::Released {
							self.held.remove(&k);
						}
						return;
					}
					if k == KeyCode::F1&& event.state == ElementState::Pressed && !event.repeat {
						self.native = !self.native;
						self.shared.native.store(self.native, Ordering::Relaxed);
						if let Some(g) = &self.gpu {
							g.window.set_title(title(self.native));
						}
						eprintln!("rac2-play: {} picture (F1 flips)", if self.native { "native" } else { "emulated" });
					}
					// F5 quick save, F9 quick load (states.rs; a restart from the newest save).
					if k == KeyCode::F5 && event.state == ElementState::Pressed && !event.repeat {
						self.shared.save_req.store(true, Ordering::Relaxed);
					}
					if k == KeyCode::F9 && event.state == ElementState::Pressed && !event.repeat && crate::states::restart_into_newest() {
						rac_engine::gscap_guard::quit();
						self.shared.quit.store(true, Ordering::Relaxed);
						crate::movie::abort();
						el.exit();
					}
					if event.state == ElementState::Pressed {
						self.held.insert(k);
					} else {
						self.held.remove(&k);
					}
				}
			}
			WindowEvent::MouseInput { state, button, .. } => {
				let i = match button {
					MouseButton::Left => 0,
					MouseButton::Middle => 1,
					MouseButton::Right => 2,
					MouseButton::Back => 3,
					MouseButton::Forward => 4,
					_ => return,
				};
				self.mouse[i] = state == ElementState::Pressed;
				if i == 0 && self.mouse[0] {
					crate::pc_vendor_hover::left(true); // [b46 U1] the release is taken in poll_input, when the click's latch ends too
				}
			}
			WindowEvent::RedrawRequested => {
				// pc_options.txt changed (twice a second; options.rs) and an asked quit (engine/quit.rs: a menu's Quit to Desktop).
				crate::options::poll();
				if rac_engine::quit::take() {
					self.quit(el);
					return;
				}
				crate::pc_controls::tick();
				// [PC] nav lane, dev: RAC2_INPUT_FEED `ui` / `click` entries through the real window-event path (pc_devfeed.rs).
				let gate = crate::pc_devfeed::pending(); // [b48 Y5] the field a locked feed run asks for
				let field = gate.unwrap_or_else(|| self.shared.view.lock().map_or(0, |v| v.field));
				if let Some(id) = self.gpu.as_ref().map(|g| g.window.id()).filter(|_| gate.is_some() || !crate::pc_devfeed::locked() || crate::pc_devfeed::lockstep_locked()) {
					for ev in crate::pc_devfeed::ui_events(field) {
						self.window_event(el, id, ev);
					}
				}
				// [b45 P1] A changed render scale / upscaler / aspect box applies before anything reads the frame size (R&C1: handler.rs).
				if let Some(g) = &mut self.gpu {
					g.rscale_sync();
				}
				self.sync_display_mode();
				self.display_res_frame(); // [b45 P2] Resolution row (host/display_res.rs)
				self.poll_input(gate); // [b48 Y5]
				self.update_cursor();
				self.show_save_note();
				if self.shared.over.load(Ordering::Relaxed) || self.gs.dead.load(Ordering::Relaxed) {
					el.exit();
					return;
				}
				self.redraw();
			}
			_ => {}
		}
	}

	/// Raw mouse motion while the cursor is held: the Modern camera's mouse look (modcam_ui.rs).
	fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
		if let DeviceEvent::MouseMotion { delta } = event {
			// [b49 U5] The hold RULE counts too, not only the OS grab's result: the user's live first person lost every mouse count
			// while hidden runs (which go by the rule) turned fine, so a grab that failed or lags must not mute the mouse.
			// [b50 V3] Review finding 8: the rule is computed once per redraw in `update_cursor` (`RULE`), not per raw event.
			let held = self.cursor.grabbed() || (!rac_engine::ui::is_open() && RULE.load(Ordering::Relaxed));
			RAW_SEEN.fetch_add(1, Ordering::Relaxed);
			if !held {
				RAW_DROPPED.fetch_add(1, Ordering::Relaxed);
			}
			self.mouse_motion(delta.0, delta.1, self.focused, held); // [b46 U1] one route, the dev feed's too
		}
	}
}

/// [b50 V3] The cursor hold rule, computed once per redraw (`update_cursor`) for `device_event`; raw mouse events seen / not held.
static RULE: AtomicBool = AtomicBool::new(false);
static RAW_SEEN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static RAW_DROPPED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn title(native: bool) -> &'static str {
	if native {
		"Ratchet & Clank 2 (rac2-play) - native picture - F1: emulated picture"
	} else {
		"Ratchet & Clank 2 (rac2-play) - emulated picture - F1: native picture"
	}
}

/// The world decodes on its own thread: level 0 (Aranos) while the game boots, then whichever level the game loads next, and the
/// same level again when the game streams another terrain chunk in (`chunks`; the game thread sends each request, only the
/// newest is decoded).
fn world_loader(iso: String, shared: Arc<Shared>) -> std::sync::mpsc::Sender<crate::chunks::Req> {
	let (level_tx, level_rx) = std::sync::mpsc::channel::<crate::chunks::Req>();
	let _ = level_tx.send(crate::chunks::Req { level: 0, sig: None });
	let _ = std::thread::Builder::new().name("rac2-world".into()).spawn(move || {
		let mut done = 0u64;
		while let Ok(mut req) = level_rx.recv() {
			done += 1;
			while let Ok(newer) = level_rx.try_recv() {
				req = newer;
				done += 1;
			}
			let t0 = std::time::Instant::now();
			match World::load(&iso, req.level as usize, req.sig) {
				Ok(w) => {
					eprintln!("rac2: world decoded: level {} in {:.0} ms (world thread)", req.level, t0.elapsed().as_secs_f64() * 1e3);
					*shared.world.lock().unwrap() = Some(w);
				}
				Err(e) => eprintln!("rac2-play: world {}: {e}", req.level),
			}
			shared.loaded.set(done);
		}
	});
	level_tx
}

/// The window. `shots`: window-shot run into that folder (lockstep, `o` gives the fields, pictures, pad).
pub fn run(iso: &str, o: Opts, shots: Option<String>) -> Result<(), String> {
	framelog::init();
	rac_engine::pack::set(&crate::pack_bind::RAC2);
	let lockstep = shots.is_some();
	let rec_pad = match (&o.replay, &shots) {
		(Some(r), Some(d)) => {
			std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
			crate::shots::load_replay(r, d)?
		}
		// [PC] Test (round 3 batch 28): a LIVE replay, paced in real time like the player's session (GS pipe, smoothing, texture serving
		// on their real clocks); the card copy goes to RAC2_REPLAY_DIR (default REC_live), never into the recording.
		(Some(r), None) => crate::shots::load_replay(r, &live_replay_dir(r)?)?,
		_ => Vec::new(),
	};
	// [PC] The recording's PC actions (pc_rec.rs): with pc.bin its pad is final and those actions replay as made.
	if let Some(r) = &o.replay {
		match crate::pc_rec::load(r) {
			Some(n) => eprintln!("replay: {n} PC actions (pc.bin): the recorded pad is final"),
			None => eprintln!("replay: no pc.bin (an older recording): the pad filters run again as before"),
		}
	}
	if let Some(d) = &shots {
		std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
	}
	let replay = o.replay.is_some();
	crate::modern_open::set_driven(lockstep || replay || crate::pc_devfeed::scripted()); // [b46 U6]
	crate::pc_devfeed::set_lockstep(lockstep); // [b50 V6]
	let fields = if replay { o.fields.min(rec_pad.len() as u64) } else { o.fields };
	// The sharp HUD (engine gscap/rc2.rs): the GPU capture shares the window's device, so it learns of the window first and opens in the background.
	// [b45 P1] A played session starts the capture also with the HUD row on Original (its drawing is gated live by `set_hd_target`
	// in frame.rs), so Original -> Sharp in the menu works at once, as in R&C1. Tests and RAC_HUD=original keep the old rule.
	let sharp = crate::options::hud_sharp() || (!lockstep && !replay && std::env::var("RAC_HUD").map_or(true, |v| !v.eq_ignore_ascii_case("original")) && std::env::var_os("RAC_TEST_WINDOW").is_none());
	if sharp {
		rac_engine::gscap::gpu_expect_window();
		rac_engine::gscap::gpu_prestart(|| {});
	}
	// The ship's glass and the glows are drawn by the engine (host/ov_layers.rs), not into the GS overlay.
	rac_engine::gsfull::layer_tris::enable(ov_layers::enabled());
	rac_engine::gsfull::layer_tris::enable_fx(ov_layers::fx_enabled());
	let start_state = crate::states::start_dir();
	if let Some(d) = start_state.as_deref() {
		crate::states::load_cam(d);
	}
	let (tx, gs) = rac_engine::gsfull::ovl_pipe::start_from(sharp, start_state.as_deref().unwrap_or("")).ok_or("gsfull start (a state dir needs GS.bin, vu1Memory.bin and vu1MicroMem.bin)")?;
	let shared = Arc::new(Shared::default());
	shared.pad.store(u32::from_le_bytes([0x80; 4]) as u64, Ordering::Relaxed);
	crate::modern_grid_icons::set_iso(iso);
	let level_tx = world_loader(iso.to_string(), shared.clone());
	let native = std::env::var("RAC2_PICTURE").map_or(true, |v| v != "gs");
	eprintln!("rac2-play: {} picture; F1 flips between the native and the emulated picture", if native { "native" } else { "emulated" });
	shared.native.store(native, Ordering::Relaxed);
	let shot_set = shots.map(|d| frame::Shots::new(d, &o, fields));
	crate::movie::reset();
	// A replay (live too) skips movies at once like a lockstep run: the player's skips were window presses, not in pad.bin.
	crate::movie::set_serviced(movie::serviced(lockstep || replay));
	let r = game::Run { iso: iso.to_string(), o, rec_pad, fields: run_fields(lockstep, replay, fields), lockstep, state: start_state, gs: gs.clone() };
	let sh = shared.clone();
	let game = std::thread::Builder::new().name("ee".into()).stack_size(256 << 20).spawn(move || game::game(r, tx, sh, level_tx)).map_err(|e| e.to_string())?;
	let el = EventLoop::new().map_err(|e| e.to_string())?;
	el.set_control_flow(ControlFlow::Poll);
	rac_engine::input::pad::set_left_stick(crate::pc_stickdz::left_stick);
	let mut app = App { gpu: None, stage: frame::Stage::default(), gs, shared: shared.clone(), held: HashSet::new(), mouse: [false; 5], focused: false, test_window: std::env::var_os("RAC_TEST_WINDOW").is_some_and(|v| v == "hidden"), cursor: cursor::Cursor::default(), native, ov_overlay: false, aspect: 4.0 / 3.0, shots: shot_set, smooth: Default::default(), sub_at: (0, 0), freeze: Default::default(), ov_hold: Default::default(), ov_layers: Default::default(), layers_new: None, ov_view: None, pace: Pace::new(), iso: iso.to_string(), playing: None, movie_audio: None, sync: Default::default(), livecap: livecap::LiveCap::from_env(), ui_pos: None, fullscreen: false };
	el.run_app(&mut app).map_err(|e| e.to_string())?;
	rac_engine::gscap_guard::quit();
	shared.quit.store(true, Ordering::Relaxed);
	// A game thread waiting in the movie hook is let go.
	crate::movie::abort();
	drop(app);
	let _ = game.join();
	// A quick save still being written (F5, then close the window) is finished first.
	crate::states::wait_for_save();
	Ok(())
}

/// The fields the game thread runs: lockstep and replays (live too) the run's count, a played session until the window closes (0).
fn run_fields(lockstep: bool, replay: bool, fields: u64) -> u64 {
	if lockstep || replay { fields } else { 0 }
}

/// A live replay's own folder (the card copy the replay saves into): RAC2_REPLAY_DIR, else REC_live next to the recording.
fn live_replay_dir(rec: &str) -> Result<String, String> {
	let d = std::env::var("RAC2_REPLAY_DIR").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| format!("{}_live", rec.trim_end_matches(['/', '\\'])));
	std::fs::create_dir_all(&d).map_err(|e| format!("{d}: {e}"))?;
	Ok(d)
}

#[cfg(test)]
mod live_replay_tests {
	#[test]
	fn a_live_replay_runs_the_recorded_fields_and_a_session_runs_on() {
		assert_eq!(super::run_fields(false, true, 10560), 10560, "live replay: stops at the recording's end");
		assert_eq!(super::run_fields(true, false, 1500), 1500, "window shots");
		assert_eq!(super::run_fields(false, false, 1500), 0, "a played session runs until the window closes");
	}
}
