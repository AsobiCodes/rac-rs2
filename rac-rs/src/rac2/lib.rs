//! The Ratchet & Clank 2 pack (engine split stage E5, 2026-10-27; plan: ENGINE_SPLIT.md), PAL SCES_516.07. The retail EE code
//! runs in ee-oracle (its R&C2 profile `oracle/src/rc2`, the recompiled code from `oracle/rc2gen`, linked by the exe
//! rac-rs/exe_rc2 through `set_register`); the engine draws the world from the level files on the disc (rac-formats' R&C2
//! layer) and what this pack reads from RAM each field; gsfull's HUD overlay mode draws the game's own HUD over it. It depends
//! on `rac-engine` and `rac-formats` only (never `rac1`). From lane rc2 (scratch_perf_rc2 S3-S4), whose own window, renderer
//! and GS pipe it replaces.
//!
//! Usage:
//!   rac2-play [ISO]                      window (ISO: else env RAC2_ISO)
//!   rac2-play --shots DIR [opts] [ISO]   hidden: no window; the GS's pictures (PNG) and their hashes
//!   rac2-play --window-shots DIR [opts] [ISO]   the window's pictures (the engine's world + the GS's HUD), lockstep (host.rs)
//!     --fields N   fields to run (default 1500)
//!     --every K    a picture every K fields (default 50; 0 = none) and at the fields listed by --at a,b,c, and the last
//!     --pad "a-b:MASK,..."   buttons held in those field ranges (hex PS2 bits)
//!     --stick "a-b:LXLY,..." left stick in those field ranges (hex bytes, 80 = centre)
//!     --replay REC  play a window session's recording (REC/pad.bin, REC/card.mcd, REC/screen.txt) back field for field
//! Env: RAC2_RAMHASH=1 (a RAM hash every 250 fields), RAC2_LEVEL=N (New Game loads level N), RAC2_SAVE (save file; default
//! card.mcd in the player dir: RAC_PLAYER_DIR, else rac-rs/rac2/player), RAC2_NOREC=1 (no session recording), RAC2_NOSOUND=1, RAC2_LOG=1, RAC_TEST_WINDOW=hidden.

pub mod addrs;
pub mod scan_timing;
pub mod boot;
mod install_start;
mod update_check; // [v0.1 U] the automatic update check (Windows: setup.ps1 -Launch; Linux: the notice)
mod update_notice_linux; // [v0.1 U] duplicate of rac1's until the engine batch shares it
pub mod chunks;
mod host;
mod lxpad;
pub mod movie;
pub mod diag_facts; // [b50 V9] RAC_DIAG facts (Pack::diag_facts)
mod movie_overlay;
pub mod movie_subs;
#[cfg(test)]
mod movie_tests;
pub mod modcam_blend;
pub mod modcam_frame;
pub mod modcam_fp; // [b45 P5]
pub mod modcam_mouseinv; // [b45 P5]
pub mod modcam_death;
pub mod modcam_hand;
pub mod modcam_look;
pub mod modcam_marker;
pub mod modcam_pose;
pub mod modcam_rec;
pub mod modcam_probe;
pub mod modcam_shoulder;
#[cfg(test)]
mod modcam_pose_tests;
#[cfg(test)]
mod modcam_mag_tests;
#[cfg(test)]
mod modcam_probe_tests;
pub mod modcam_sticks;
pub mod modcam_ui;
pub mod modcam_who;
pub mod modcam_yield;
pub mod moby_cull;
pub mod modern_grid;
pub mod modern_walk; // [b45 P3]
pub mod modern_grid_extra;
pub mod modern_grid_icons;
pub mod modern_grid_page;
pub mod modern_items;
pub mod modern_items_page;
pub mod modern_help;
pub mod modern_help_page;
pub mod modern_jump;
pub mod modern_tabs;
pub mod modern_qs;
pub mod modern_qs_page;
pub mod modern_menu;
pub mod modern_open;
pub mod modern_optlist; // [b45 P4B]
pub mod modern_optlist_page; // [b45 P4B]
pub mod modern_optlist_sound; // [b46 W4]
pub mod modern_optlist_quit; // [b47 X3]
pub mod modern_pages;
pub mod modern_screen;
pub mod modern_load; // [PC] ld lane: the modern Load / Save Game slot pages
pub mod modern_card_box; // [b57 NG] the title's memory-card message boxes
pub mod modern_load_page; // [PC] ld lane
pub mod real_clock; // [PC] ld lane: the real date on the saves of a player session
pub mod modern_map;
pub mod modern_map_page;
pub mod modern_map_art; // [b46 W5]
pub mod modern_hint_keys; // [b46 W5]
pub mod modern_shack;
pub mod modern_shack_page;
pub mod modern_vendor;
pub mod modern_vendor_buyall;
pub mod modern_vendor_find;
pub mod modern_vendor_hand;
pub mod modern_vendor_page;
pub mod modern_icons;
pub mod modern_icons_decode;
pub mod modern_rows;
pub mod modern_rows_upscale;
pub mod modern_rows_metal;
pub mod modern_rows_input; // [b45 P2]
pub mod modern_rows_display; // [b45 P2]
pub mod modern_rows_video;
pub mod subs_sync;
pub mod video60;
pub mod modern_rows_fire;
pub mod modern_rows_hud;
pub mod hud_layout;
pub mod hud_steps;
pub mod glyph_cells;
mod metal_materials;
mod looks;
mod smooth_shadows; // [b45 P7]
mod fx_movers; // [b45 P7] water rings and wakes: the hero and the movers
mod planet_fog;
mod planet_ice;
mod planet_sky;
mod options;
pub mod pack_bind;
pub mod pc_debug;
pub mod pc_debug_rows;
pub mod pc_devtools;
pub mod pc_freecam;
pub mod pc_devfeed;
pub mod pc_controls;
pub mod pc_stickdz;
pub mod pc_input;
pub mod pc_taps;
pub mod pad_glyphs; // [b46 W6]
pub mod pad_binds;
pub mod pc_menu_hover;
pub mod pc_menu_mouse; // [b45 P4B]
pub mod pc_menu_mouse_grid; // [b46 W7]
pub mod pc_menu_mouse_items; // [b47 X4]
pub mod pc_menu_mouse_title; // [b47 X4]
pub mod pc_menu_mouse_box; // [b48 Y4]
pub mod pc_menu_mouse_sound; // [b48 Y4]
pub mod pc_menu_mouse_map; // [b50 V3]
pub mod pc_vendor_hover;
pub mod pc_prompts;
pub mod pc_rec;
pub mod pc_warp;
pub mod space_flight; // [b46 U4]
pub mod kbm_table;
pub mod props;
pub mod screenspot;
pub mod ram_anim;
pub mod ramdump;
pub mod scene;
mod class_stamp; // [b49 U2b]
mod tex_guard; // [b50 V7]
pub mod shadows;
pub mod rec_state; // [PC] recordings that start from a state (REC/state)
pub mod shots;
pub mod states;
#[cfg(test)]
mod planet_check;
#[cfg(test)]
mod sun_check;
#[cfg(test)]
mod sun_ray_check;
mod sndlog;
mod ov_warp;
mod smooth;
pub mod texpack;
pub mod tiecol;
pub mod under;
pub mod fx_pool;
pub mod fx_rows;
pub mod fx_shots; // [b47 X1] the player's shots: explosion gate + rocket lights
pub mod unwind;
mod water;
pub mod veg; // [PC] Vegetation row (PARITY 17)
pub mod fur; // [PC] Fur rows (PARITY 28)
pub mod fur_armor; // [b46 W3] plate cuts of the other armors' fur rows
pub mod world;

// The engine's and the formats' modules under their root paths (`crate::gsfull`, `crate::level`, ...), as in rac1.
use rac_engine::{gsfull, render};

use std::process::ExitCode;

static REGISTER: std::sync::OnceLock<fn()> = std::sync::OnceLock::new();

/// The exe passes `ee_game_rc2::register` here before it calls `main`.
pub fn set_register(f: fn()) {
	let _ = REGISTER.set(f);
}

/// Registers the recompiled R&C2 code (the function the exe passed in; none in unit tests: the interpreter runs alone).
pub fn register() {
	if let Some(f) = REGISTER.get() {
		f();
	}
	// The movie player hook (movie.rs): skips every movie at once until the window calls `movie::set_serviced(true)`.
	movie::register();
	// [PC] The 60 Hz mode row is hidden (video60.rs: the NTSC mode hangs the first level): its hooks are not registered.
	// The Modern camera's hand on the follow camera update (modcam_hand.rs). [b45 P5] Always registered, so the Camera row
	// switches Classic <-> Smooth live; with Classic the hand declines (no write, no instruction count): Classic stays bit-exact.
	modcam_hand::register();
	// [PC] Key names in the button prompts while the keyboard is in use (pc_prompts.rs; `keyprompts=off` registers nothing).
	pc_prompts::register();
	modern_vendor_hand::register(); // [PC] the modern shop's Buy all ammo in one go (no request = retail's dispatcher untouched)
}

pub fn main() -> ExitCode {
	rac_engine::pack::set(&pack_bind::RAC2);
	// [PC] [b45 P1] RivaTuner's Vulkan layer off before the first Vulkan call (rtss_layer.rs), and the panic hook (crash.log; host/sys_crash.rs), as R&C1's main.
	rac_engine::rtss_layer::apply();
	host::sys_crash::install_panic_hook();
	// [PC] An installed game started with no arguments finds its disc image in generated/play.cfg (install_start.rs).
	if let Err(e) = install_start::apply() {
		// The run log first (still single-threaded), then the box: the install has no usable play.cfg (R&C1: launch_args).
		let dir = rac_engine::install_root().ok().or_else(|| std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.to_path_buf()))).unwrap_or_default().join("player");
		rac_engine::runlog::start(&dir);
		host::sys_crash::report_failure(&e);
		return ExitCode::from(2);
	}
	// [v0.1 U] A player's start (no arguments, no RAC2_STATE) first asks setup.ps1 -Launch about an update (Windows: exit 10 = the update
	// window took over, leave at once) or starts the notice thread (Linux). After the last set_var, before anything else starts.
	if update_check::at_start() {
		return ExitCode::SUCCESS;
	}
	// [PC] The console reports a 16:9 TV (OSD screen type 1, read at the cold boot), which the boot turns into the game's own
	// Options row "16:9 (widescreen)" (byte 0x1a7c39 from FUN_00131688, set in FUN_002914e0; vertical FOV x 0.75 in FUN_0027af28
	// [C, decomp 2026-10-27]): the game draws and culls for the wide view itself; the player can switch to 4:3 in its Options. A
	// replay boots as it was recorded (shots.rs `load_replay`); env COLD_ASPECT overrides both.
	ee_oracle::cold::set_aspect(1);
	let args: Vec<String> = std::env::args().skip(1).collect();
	let mut iso = std::env::var("RAC2_ISO").unwrap_or_default();
	let mut o = shots::Opts { fields: 1500, every: 50, ..Default::default() };
	let (mut dir, mut wdir) = (None, None);
	let mut i = 0;
	while i < args.len() {
		let v = args.get(i + 1).cloned().unwrap_or_default();
		match args[i].as_str() {
			"--shots" => dir = Some(v),
			"--window-shots" => wdir = Some(v),
			"--fields" => o.fields = v.parse().unwrap_or(o.fields),
			"--every" => o.every = v.parse().unwrap_or(o.every),
			"--at" => o.at = v.split(',').filter_map(|x| x.parse().ok()).collect(),
			"--pad" => o.pad = boot::parse_pad(&v),
			"--stick" => o.stick = boot::parse_pad(&v),
			"--replay" => o.replay = Some(v),
			s => {
				iso = s.to_string();
				i += 1;
				continue;
			}
		}
		i += 2;
	}
	// A recording that starts from a state (REC/state, rec_state.rs) replays from it, in every replay mode (states.rs `start_dir`).
	if let Some(s) = o.replay.as_deref().and_then(rec_state::replay_state) {
		eprintln!("replay: resumes from the recorded state {s}");
		states::set_start(s);
	}
	if !std::path::Path::new(&iso).is_file() {
		host::sys_crash::report_failure(&format!("disc image not found: {iso:?} (pass the R&C2 PAL ISO, or set RAC2_ISO)"));
		return ExitCode::from(2);
	}
	// [PC] ld lane: the host's date in the saves of a player's session; scripted and test runs keep the fixed clock (real_clock.rs).
	real_clock::init(dir.is_some() || wdir.is_some() || o.replay.is_some() || !o.pad.is_empty() || !o.stick.is_empty());
	let r = match (dir, wdir) {
		(Some(d), _) => shots::run(&iso, &d, &o),
		(None, Some(d)) => {
			// [tests] RAC2_REC_LOCKSTEP=1: a feed run records its session like a played one (pc_rec.rs), to replay it.
			// Its memory card is the player dir's card.mcd like a played session's (the replay gets the same card; with none the
			// game would keep the state's card-space word while the replay's blank card rewrites it).
			if o.replay.is_none() && pc_rec::rec_lockstep() {
				let card = rac_engine::pcopts::player_dir().join("card.mcd").to_string_lossy().into_owned();
				boot::start_recording(Some(&card), states::start_dir().as_deref());
				ee_oracle::mc::set_persist(Some(card));
			}
			host::run(&iso, o, Some(d))
		}
		(None, None) => {
			// The player dir (pcopts::player_dir: RAC_PLAYER_DIR, else rac-rs/rac2/player) holds the run log (stderr there when it goes
			// nowhere), pc_options.txt and the game's saves, card.mcd (RAC2_SAVE=path overrides): kept across runs.
			states::wait_for_parent(); // an F9 child: the old process must be gone before the card, the log or the window are touched
			let player = rac_engine::pcopts::player_dir();
			let _ = std::fs::create_dir_all(&player);
			rac_engine::runlog::start(&player);
			let save = std::env::var("RAC2_SAVE").ok().filter(|v| !v.is_empty()).or_else(|| Some(player.join("card.mcd").to_string_lossy().into_owned()));
			// A state-started session (RAC2_STATE, an F9 restart) records with its state (REC/state, rec_state.rs).
			if o.replay.is_none() {
				boot::start_recording(save.as_deref(), states::start_dir().as_deref());
			}
			ee_oracle::mc::set_persist(save);
			host::run(&iso, o, None)
		}
	};
	match r {
		Ok(()) => {
			rac_engine::omni::flush_log();
			rac_engine::runlog::end(); // [b45 P1] the log's "the game ended" line
			ExitCode::SUCCESS
		}
		Err(e) => {
			host::sys_crash::report_failure(&e);
			ExitCode::FAILURE
		}
	}
}
