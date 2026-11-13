//! The R&C2 pack's answers to the engine renderer (engine/pack.rs; engine split stage E5): the Graphics verdict of the player's
//! pc_options.txt (stage E6, `options.rs`; live since parity wave 1, with Debug mode, FPS limit and Render scale), a game session runs (the prop batch keeps room for the mobies the game adds), no PC
//! extensions, the GS capture's device hooks (engine gscap: the sharp HUD of stage E6 shares the window's device), no shop or world art, the retail looks but for the Dynamic shadows (looks.rs).

use rac_engine::pack::{ModsRule, Pack};

pub static RAC2: Pack = Pack {
	kbm: &crate::kbm_table::TABLE, // [PC] generic actions + R&C2 Menu layer
	graphics: crate::options::graphics,
	debug_mode: crate::options::debug_mode,
	running: || true,
	pc_ext: || true, // engine reads it only in vegfx (the Vegetation row's planner gate)
	veg_for_load: || crate::options::graphics().veg,
	ui_sharp: |_| false,
	capture_features: rac_engine::gscap::gpu_window_features,
	capture_window: rac_engine::gscap::gpu_window,
	capture_submit_lock: rac_engine::gscap::gpu_submit_lock,
	prompt_class: u32::MAX,
	portrait_class: u32::MAX,
	mesh_fallback: (u32::MAX, u32::MAX),
	moby_limit: 0,
	fx_clamped: |_| false,
	fx_glass: |_| false,
	world_art: |_| None,
	looks: &crate::looks::LOOKS,
	metal_textures: crate::metal_materials::METAL_TEXTURES,
	metal_subdued_textures: crate::metal_materials::SUBDUED,
	metal_subdued_classes: &[],
	// Fire, Smoke and weapon lights on the effect pool's sprites (rac2/fx_rows.rs, host/ov_fire.rs).
	smoke: crate::fx_rows::SMOKE,
	smoke_quads: |_| &[],
	shot_lights: crate::fx_rows::SHOTS,
	fire_rows: crate::fx_rows::fire_rows,
	fire_mobys: |_| &[],
	// The projection block's near and far words: 32.0 and 1048576.0 (1024 m; R&C1 745472.0) [C: savestates fullgame1/0013
	// level 1 RAM 0x1bd3a0, 0452 level 10 0x1bcc60]; the GS z of a view depth d is 524288 / d - 480 (gsfull overlay_depth), 32
	// at the far plane. The far word is per level (3072000.0 on Aranos and level 14, 4194304.0 on some): the GS depth decode follows
	// it (addrs::PROJ_FAR_BELOW, engine layer_tris::z_offset_of); this clip far stays 1024 m.
	far: (1024.0, 1048576.0),
	// As the launcher's R&C2 entry: <install>/rac2/player.
	player: "rac2/player",
	// <install>/rac2/mods beside <install>/rac2/player (the launcher's layout): PCSX2 texture packs of SCES-51607 (texpack.rs).
	mods: ModsRule { game: "rac2", beside_player: true },
	font_patch: Some(crate::glyph_cells::font_patch), // [PC] Button glyphs (glyph_cells.rs)
	dx12_writer: Some(crate::options::set_dx12), // [b45 P2]
	fps_setting: crate::options::fps_setting,
	note_refresh: |_| {},
	render_scale_pct: crate::options::render_scale_pct,
	// Render scale and Aspect ratio (parity wave 1, from the file): a lasting wgpu error puts them back to their defaults.
	reset_sizes: Some(rac_engine::pack::SizeReset { names: "Render scale and Aspect ratio", reset: crate::options::reset_sizes }),
	is_usa: || false,
	fur: crate::fur::spec,
	fur_prop: crate::fur::hero_class,
	achievements: None,
	// The modern Options page (parity wave 2 B): Cross on the Camera row of the game's Options list (modern_open.rs).
	// The window side: rac2/host/menu.rs; the ticks play on the game thread's mixer (host/game.rs).
	ui: Some(&crate::modern_menu::PACK),
	ui_sound: rac_engine::ui::sound::request,
	hud_z: true,
	diag_facts: crate::diag_facts::get, // [b50 V9]
	diag_mark_key: winit::keyboard::KeyCode::Insert, // [b52 H3] was F10 (also Debug mode's split view); F9 is the quick load
	diag_detail: true, // [b51 G5] a block for every frame over 12 ms, the time outside the redraw, extra columns
	scene_msaa_default: 1, // [b45 P2] R&C1's SCENE_MSAA_DEFAULT (off while DLSS / FSR runs, else 4x)
};
