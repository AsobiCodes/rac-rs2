//! R&C2's mobies as engine props (engine split stage E5): the class meshes the engine draws (the level file's classes from
//! `world`, and the classes the game builds in RAM with their textures read from GS memory), the prop slot pool that grows with
//! the game's moby list, and each field's `Prop` list with the animated mobies' skinned vertices.

use crate::scene::{Moby, RamClass, Scene};
use rac_engine::prop::{Kind, Prop, Sched};
use rac_engine::viewer::{Gpu, Posed};
use rac_formats::level::Texture;
use rac_formats::mesh::Mesh;
use rac_formats::mobies::{MobyClass, FLAT_SLOT, GADGET_TEXTURE_BASE};
use std::collections::HashMap;
use std::sync::Arc;

/// Engine class ids of R&C2 classes: the class key (header address minus core base) in the low 30 bits.
pub const CLASS_BASE: u32 = 0x4000_0000;

pub fn class_id(key: u32) -> u32 {
	CLASS_BASE | key & 0x3fff_ffff
}

/// Engine class id of part `bit` (`rc2::mobies::Part`) of the class at `key`: bit 31 set (a `class_id` never has it), the part
/// number in bits 26..30 and the key's low 26 bits (a normal key is below 0x0200_0000, the boot outlier 0xff9xxxxx keeps bits 24
/// and 25, so no two classes share a part id).
pub fn part_id(key: u32, bit: u32) -> u32 {
	0x8000_0000 | (bit & 0xf) << 26 | key & 0x03ff_ffff
}

/// Is this engine class id a part (`part_id`)?
pub fn is_part(id: u32) -> bool {
	id & 0x8000_0000 != 0
}

/// A class TEX0 whose texture base (TBP0, bits 0..13) the game has not written yet [C: Aranos glass 0x1504200 TEX0
/// 80061be555304000 until field ~6030, then 80061be555307850]. The retail GS draws it with whatever its frame buffer holds at
/// block 0 (a faint pale shard, `runs/a1_gs`); the native picture has no 3D frame buffer there and would read black, so a part
/// class (a piece) waits for its base instead of drawing black slabs (a whole class keeps its old behaviour: Ratchet's own
/// textures really sit at TBP 0).
pub fn base_unset(tex0: u64) -> bool {
	tex0 & 0x3fff == 0 && !mip_only(tex0)
}

/// A TEX0 at TBP0 0 whose class draws only its mip levels (a level 1..MXL base set, a TEX1 K the engine samples with, `k_code`): the
/// VRAM ring keeps a far class's top levels and leaves the level 0 (and 1) base at 0, and the GS LOD picks the top level [C: Aranos
/// state q..5919 RAC_GS_PRIMLOG_FRAME=30, class 0xea3: TEX0 80060265dd308000 / 80059805dd308000 / 8005bc4599304000 drawn with TEX1 MXL
/// 2 MMIN 4 K 0xf92 (-6.9), MIPTBP1 level 1 base 0, level 2 base 0x3024 / 0x3029 / 0x3028, Q < 0.0025: level 2 for every prim; body
/// and armour parts alike]. Neither a screen read nor an unset base: the class draws with its chain (`ram_chain`).
fn mip_only(tex0: u64) -> bool {
	use crate::gsfull::texreq_mips::{level_tex0, mxl, regs};
	tex0 & 0x3fff == 0 && k_code(tex0) != 0 && regs(tex0).is_some_and(|r| (1..=mxl(r[0])).any(|n| level_tex0(tex0, &r, n) & 0x3fff != 0))
}

/// A class TEX0 that reads one of the game's frame buffers as an 8-bit / 4-bit index texture through a CLUT: a screen-read effect (the
/// Heavy Lancer shot's two passes, class 0xa26: TBP0 = 0, PSMT8, 128 x 128), or a class whose texture base the game has not written yet
/// (TBP0 0 = the shown buffer's first block: the Lancer's 16 x 16 PSMT8 class 0xc89 at the moment it is fired, round 3 batch 27). The
/// retail GS samples the last picture there; the native picture has no 3D frame buffer, so its texels were stale VRAM: from a PCSX2
/// state the old PCSX2 frame (dark noise), in a session started from boot the boot / loading art, drawn as hazard-striped boxes [C:
/// runs/b8_trailtex.png, runs/b27/log w49885 (rec_user9 replay: class 0xc89 hdr 0x1786040 TEX0 8006200511304000)]. Such faces are drawn
/// from the native picture's corner packed as the frame buffer (screenspot.rs, round 3 batch 27e), whatever their size. The frame buffers are the video mode's own (`note_shown`: the CRTC's shown buffers; texreq
/// `in_target`: the GS render target of context 1; FRAME_2 sits in the texture area). PSMT8H / PSMT4H* (Ratchet's own textures live in the alpha plane at TBP 0..)
/// are not screen reads. The game also puts small textures it draws into a frame buffer's blocks (crate debris, vendor previews at TBP 0,
/// round 3 batch 27c: F1 draws them): those it uploaded there for the draw (texreq `uploaded_lately`) are real textures; until the texels
/// are read the answer is false (the class waits for the texture like any other).
pub fn reads_screen(tex0: u64) -> bool {
	screen_read(screen_spot(tex0), crate::gsfull::texreq::uploaded_lately(tex0))
}

/// An index texture inside a frame buffer (`reads_screen` before the upload test); a `mip_only` class draws its levels instead.
fn screen_spot(tex0: u64) -> bool {
	frame_spot(tex0) && !mip_only(tex0)
}

/// An index texture inside a frame buffer, whatever its levels.
fn frame_spot(tex0: u64) -> bool {
	let tbp = (tex0 & 0x3fff) as u32;
	index_texture(tex0) && (in_shown(tbp as u64) || crate::gsfull::texreq::in_target(tbp))
}

/// A `mip_only` TEX0 whose level 0 is a screen read (a frame buffer the game did not upload to lately): the GS still samples level 0
/// wherever LOD = log2(1/Q) + K < 0.5 (a near draw: the Heavy Lancer shot TEX0 8005f565dd308000 has mip registers too). Its level 0
/// is drawn see-through (`gs_level0`), so the shader's GS level choice (texfilter.wgsl `gs_level`) keeps only the texels of levels
/// 1..MXL and a near draw shows nothing, as the screen-read drop rule does for the whole TEX0.
fn level0_screen(tex0: u64) -> bool {
	mip_only(tex0) && screen_read(frame_spot(tex0), crate::gsfull::texreq::uploaded_lately(tex0))
}

/// Which levels 1..MXL of a `level0_screen` TEX0 are screen reads too (their MIPTBP base inside a frame buffer, as level 1 of the
/// green Thug 0xea3: base 0 [C: Aranos q..5919 RAC_GS_PRIMLOG_FRAME=30, MIPTBP1 level 1 base 0, level 2 base 0x3024]); those draw
/// see-through as level 0 does (`ram_chain`). None for any other TEX0.
fn level_screen(t0: u64) -> impl Fn(u32) -> bool {
	let r = crate::gsfull::texreq_mips::regs(t0).filter(|_| level0_screen(t0));
	move |n| r.is_some_and(|r| frame_spot(crate::gsfull::texreq_mips::level_tex0(t0, &r, n)))
}

/// The level 0 texels the engine draws for GS texture `t0` read as `t`: see-through for a `level0_screen` TEX0, `t` itself otherwise.
fn gs_level0(t0: u64, t: &Texture) -> Texture {
	let rgba = if level0_screen(t0) { vec![[0; 4]; t.rgba.len()] } else { t.rgba.clone() };
	Texture { w: t.w, h: t.h, rgba }
}

/// A texture in a frame buffer (`spot`) reads the screen when its texels were read and the game had not uploaded them lately (`uploaded`;
/// None: not read yet).
fn screen_read(spot: bool, uploaded: Option<bool>) -> bool {
	spot && uploaded == Some(false)
}

/// An 8-bit / 4-bit index texture (PSMT8, PSMT4), the formats a screen read uses (not the alpha-plane PSMT8H / PSMT4H*).
fn index_texture(tex0: u64) -> bool {
	matches!(tex0 >> 20 & 0x3f, 0x13 | 0x14)
}

/// The frame buffers the CRTC showed lately (start block, blocks), newest last: in every video mode (PAL 512 x 512, 60 Hz 512 x 448,
/// progressive, the card's screen options) the buffers the game shows are the ones its screen reads sample.
static SHOWN: std::sync::Mutex<Vec<(u64, u64)>> = std::sync::Mutex::new(Vec::new());
const SHOWN_KEEP: usize = 4;

/// Game thread, after each field: the shown buffer (`Crtc::shown`: FBP in blocks, FBW, DBX, DBY, w, h); its rows up to DBY + h.
pub fn note_shown(shown: Option<(u32, u32, u32, u32, u32, u32)>) {
	let Some((fbp, fbw, _, dby, _, h)) = shown else { return };
	crate::screenspot::note_shown(shown.unwrap_or_default());
	// PSMCT32: a 64-pixel-wide page column of one row is 64 * 4 bytes = one block per FBW unit and row.
	let r = (fbp as u64, fbw as u64 * (dby + h) as u64);
	let Ok(mut v) = SHOWN.lock() else { return };
	if v.last() == Some(&r) {
		return;
	}
	static LOG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
	if *LOG.get_or_init(|| std::env::var_os("RAC2_CRTCLOG").is_some()) {
		eprintln!("crtc: shown buffer at block {:#x}, {:#x} blocks (FBW {fbw}, rows {})", r.0, r.1, dby + h);
	}
	v.retain(|x| *x != r);
	if v.len() == SHOWN_KEEP {
		v.remove(0);
	}
	v.push(r);
}

fn in_shown(tbp: u64) -> bool {
	SHOWN.lock().is_ok_and(|v| v.iter().any(|&(a, n)| (a..a + n).contains(&tbp)))
}

/// RAC2_CRTCLOG: per class and TEX0 in a frame buffer, once its texels were read: a screen read (drawn from the picture) or kept as uploaded.
fn log_screen_reads(key: u32, words: &[u64], scr: &[bool]) {
	static LOG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
	if !*LOG.get_or_init(|| std::env::var_os("RAC2_CRTCLOG").is_some()) {
		return;
	}
	static SEEN: std::sync::Mutex<Option<std::collections::HashSet<(u32, u64)>>> = std::sync::Mutex::new(None);
	for (&t, &r) in words.iter().zip(scr) {
		let up = crate::gsfull::texreq::uploaded_lately(t);
		if !screen_spot(t) || up.is_none() || !SEEN.lock().is_ok_and(|mut s| s.get_or_insert_with(Default::default).insert((key, t))) {
			continue;
		}
		eprintln!("screen read: class {key:#x} TEX0 {t:016x} {}", if r { "from the picture (not uploaded lately)" } else { "kept (uploaded lately)" });
	}
}

/// RAC2_CRTCLOG: a RAM class still waiting for a texture from texreq (once per class and TEX0; the level file's class draws meanwhile).
fn log_waiting(key: u32, t: u64) {
	static SEEN: std::sync::Mutex<Option<std::collections::HashSet<(u32, u64)>>> = std::sync::Mutex::new(None);
	if std::env::var_os("RAC2_CRTCLOG").is_some() && SEEN.lock().is_ok_and(|mut s| s.get_or_insert_with(Default::default).insert((key, t))) {
		eprintln!("ram class {key:#x} waits for TEX0 {t:016x} (screen spot {}, uploaded lately {:?})", screen_spot(t), crate::gsfull::texreq::uploaded_lately(t));
	}
}

/// The engine classes drawn for a moby of class `key` with part mask `mask` (`Moby::mask`): the base class unless the moby is a
/// piece (bit 15), then the part of every set bit.
pub fn draw_ids(key: u32, mask: u16) -> impl Iterator<Item = u32> {
	let base = (mask & 0x8000 == 0).then(|| class_id(key));
	base.into_iter().chain((0..15u32).filter(move |k| mask >> k & 1 != 0).map(move |k| part_id(key, k)))
}

/// Texture key group of the textures read from GS memory (`texture_key` groups 0..4 are the level's and the sky's).
const RAM_GROUP: u32 = 0x20;
/// The 1x1 white texture of untextured faces (`FLAT_SLOT`; texel 255 = the GS modulate's 1.0, alpha 0x80 = 255 as texreq reads).
pub const WHITE_KEY: u32 = RAM_GROUP << 16;
/// Screen spots drawn from the native picture's corner (round 3 batch 27e, exe31e) instead of dropped (build 31's drop rule). OFF
/// [C, runs/b28/n31e]: over any 32-bit frame the Lancer's class 0xc89 CLUT gives the same yellow/black stripes F1 never shows.
const SCREEN_FROM_PICTURE: bool = false;

/// Texture key group of the screen-spot textures (`Classes::screen_key`).
const SCREEN_GROUP: u32 = 0x21;

/// The classes the engine can draw, by engine class id, and the pool of prop slots.
#[derive(Default)]
pub struct Classes {
	/// Engine meshes (`load_scene` / `append_props` / `regrow_props` take them as `prop_meshes`).
	pub meshes: HashMap<u32, Mesh>,
	/// The decoded classes (skin, metal packets, class scale).
	pub decoded: HashMap<u32, Arc<MobyClass>>,
	/// RAM classes built: engine class id -> the `RamClass` they were built from (Arc address).
	ram_built: HashMap<u32, usize>,
	/// [b51 G3] Classes whose build waited for a texture: (Arc address, texreq field of the attempt). `take_ram_classes` runs every redraw
	/// and a waiting class cost 0.3 ms each redraw (a Vec + texreq locks per face, build 50's `after` words): retried once per texreq field.
	ram_wait: HashMap<u32, (usize, u32)>,
	/// Texture key and last seen texreq version of the GS textures by TEX0: one key per TEX0 for good (`refresh_ram_textures`
	/// replaces the key's texels when they change).
	ram_tex: HashMap<u64, (u32, u32)>,
	/// Texture key and picture number (screenspot.rs `seq`) of the screen-spot TEX0s (`reads_screen`): drawn from the native picture's
	/// corner packed as the frame buffer, re-decoded when a newer picture arrives (`refresh_screen_textures`).
	scr_tex: HashMap<u64, (u32, u64)>,
	/// The classes' textures by key: the level's moby textures (`world`), Ratchet's armor, the GS ones (`RAM_GROUP`).
	pub tex: HashMap<u32, Texture>,
	/// The prop slots' classes in slot order (the engine's), and per class its slots.
	pub slots: Vec<u32>,
	by_class: HashMap<u32, Vec<usize>>,
	/// The slot list changed since the engine last built it (`sync`).
	stale: bool,
	/// Classes the game re-pointed at other GS textures with the same geometry: only their face texture keys changed (`sync`
	/// lays those slots out again, `Gpu::retexture_props`, instead of rebuilding every slot).
	retex: Vec<u32>,
	/// [b51 G2] Classes whose vertices moved with the same faces, keys and vertex count (the game rewrites them in RAM every few
	/// fields): only their slots' vertices are written again (`sync`, `Gpu::reshape_props`), no rebuild of every slot.
	reshape: Vec<u32>,
	/// [b50 V7] The drawn classes' GS textures asked for again + the stale-texture self-check (tex_guard.rs).
	guard: crate::tex_guard::Guard,
}

/// Each field's draw input for the engine: one prop per slot, the skinned vertices by slot, per slot the moby's bounding sphere
/// (centre and radius in 1/1024 m, `sphere`).
pub struct Frame {
	pub props: Vec<Prop>,
	pub fx_posed: HashMap<usize, std::sync::Arc<Posed>>,
	pub centres: Vec<[f32; 3]>,
	pub radii: Vec<Option<f32>>,
	/// Slots of shadow casters (drawn after the volumes).
	pub late: Vec<usize>,
	/// Per slot: the moby's colour override and the first vertex it covers (`PROP_TINTS`), see `Moby::tint`.
	pub tints: Vec<Option<([f32; 3], usize)>>,
	/// Per slot: the end (exclusive) of the tinted vertex range (`PROP_TINT_ENDS`), `u32::MAX` where there is no tint.
	pub tint_ends: Vec<u32>,
}

/// The colour override of a moby (`Moby::tint`: +0x78 when +0x34 & 0x10) on its class: (rgb, first vertex, end vertex). [C] The boot code's copy lands in the
/// colour array of the second-to-last packet of the LOD (README); a class under 2 packets has none, so no tint.
/// The end is the mesh end (`u32::MAX`), i.e. packets high-2 AND high-1 take the colour: [C picture, batch 20] F1 draws the whole hacking grid (state 0096) red,
/// and a tint limited to packet high-2 (review S1, `[second_last_start, last_start)`) left it grey. Review S1's other point (a class under 2 packets paints the
/// whole mesh) stays fixed. OPEN [?]: whether the last packet really keeps its baked colour in classes where F1 differs (edge specks in 0096).
/// [I] Only classes with at most one joint take it (0xafc, the arcs, has 0 joints and is tinted in its packet 4 of 6 only); Clank (19 joints) is not tinted in F1.
fn class_tint(tint: Option<u32>, c: &MobyClass) -> Option<([f32; 3], usize, u32)> {
	let w = tint.filter(|w| c.joints <= 1 && c.last_start > c.second_last_start)?;
	// [I, batch 22] Tried: packet high-2 only, and high-2 to the mesh end; the sphere minigame (0xb28) then shows a tinted wedge F1 does not have, so the
	// colour reaches the LAST packet only (its dot and side lights are blue in F1, the rest keeps the baked colours).
	Some((tail_rgb(w), c.last_start, u32::MAX))
}

/// Registers the mip chain of GS texture `t0` (key `tk`, level 0 `l0`) with the engine: level 0 and the levels 1..MXL texreq read
/// with it (gsfull::texreq_mips, R&C2 batch 34: the game's VRAM ring keeps only the levels a far class samples, its level 0 blocks
/// hold other textures [C: crate TEX0 ..b29b at Oozla 7900]); none when the TEX0 has no mip registers or no levels were read.
fn ram_chain(tk: u32, t0: u64, l0: &Texture) {
	let screen = level_screen(t0);
	let chain = crate::gsfull::texreq_mips::chain(t0).filter(|_| k_code(t0) != 0).map(|c| {
		let level = |n: usize, m: &Texture| Texture { w: m.w, h: m.h, rgba: if screen(n as u32 + 1) { vec![[0; 4]; m.rgba.len()] } else { m.rgba.clone() } };
		std::iter::once(Texture { w: l0.w, h: l0.h, rgba: l0.rgba.clone() }).chain(c.iter().take(3).enumerate().map(|(n, m)| level(n, m))).collect()
	});
	if std::env::var_os("RAC2_TEXLOG").is_some() {
		eprintln!("rac2: mip chain key {tk:#x} TEX0 {t0:016x}: {:?} levels, K code {}", chain.as_ref().map(|c: &Vec<Texture>| c.len()), k_code(t0));
	}
	rac_engine::viewer::gpu::register_runtime_chain(tk, chain);
}

/// Do two meshes give the engine the same slot data (vertices, uvs, colours, normals, faces with their texture keys, strip
/// phases)? Meshes with runtime-relight data (`lit`, world meshes only) are never taken as equal.
fn same_mesh(a: &rac_formats::mesh::Mesh, b: &rac_formats::mesh::Mesh) -> bool {
	a.lit.is_none() && b.lit.is_none() && a.positions == b.positions && a.uvs == b.uvs && a.faces == b.faces && a.colours == b.colours && a.strip_phase == b.strip_phase && a.normals == b.normals
}

/// Do two meshes differ at most in the texture keys of their faces (same vertices, uvs, colours, normals, strip phases and per
/// face the same vertex indices)?
fn same_geometry(a: &rac_formats::mesh::Mesh, b: &rac_formats::mesh::Mesh) -> bool {
	a.lit.is_none() && b.lit.is_none() && a.positions == b.positions && a.uvs == b.uvs && a.colours == b.colours && a.strip_phase == b.strip_phase && a.normals == b.normals && a.faces.len() == b.faces.len() && a.faces.iter().zip(&b.faces).all(|(x, y)| x.1 == y.1)
}

/// [b52 H4] Do two meshes differ at most in the vertex values and the texture keys of their faces (same face vertex indices,
/// strip phases and array lengths)?
fn same_layout(a: &rac_formats::mesh::Mesh, b: &rac_formats::mesh::Mesh) -> bool {
	a.lit.is_none() && b.lit.is_none() && a.strip_phase == b.strip_phase && a.positions.len() == b.positions.len() && a.uvs.len() == b.uvs.len() && a.colours.len() == b.colours.len() && a.normals.len() == b.normals.len() && a.faces.len() == b.faces.len() && a.faces.iter().zip(&b.faces).all(|(x, y)| x.1 == y.1)
}

/// [b51 G2] Do two meshes differ at most in the vertex values (same faces with keys, strip phases and array lengths)?
fn same_topology(a: &rac_formats::mesh::Mesh, b: &rac_formats::mesh::Mesh) -> bool {
	a.lit.is_none() && b.lit.is_none() && a.faces == b.faces && a.strip_phase == b.strip_phase && a.positions.len() == b.positions.len() && a.uvs.len() == b.uvs.len() && a.colours.len() == b.colours.len() && a.normals.len() == b.normals.len()
}

/// The face-key TEX1 K code of GS texture `t0` (rac_formats::mobies::K_CODE_SHIFT; 0 without mip registers, L != 0 or a full K table).
fn k_code(t0: u64) -> u32 {
	let Some(r) = crate::gsfull::texreq_mips::regs(t0) else { return 0 };
	let (k, l) = crate::gsfull::texreq_mips::k_l(r[0]);
	if l != 0 {
		return 0;
	}
	rac_formats::mobies::intern_k(k)
}

impl Classes {
	/// A level-file class (`world::load`).
	pub fn add_level_class(&mut self, key: u32, c: MobyClass) {
		let id = class_id(key);
		self.meshes.insert(id, c.mesh.clone());
		self.decoded.insert(id, Arc::new(c));
	}

	/// The RAM classes of `scene` not built yet (or rebuilt from another class at the same key), once every texture they use has
	/// arrived from GS memory (gsfull::texreq); until then the level file's class at that key, if any, keeps drawing. True when
	/// a class mesh changed (the slots of that class must be rebuilt).
	pub fn take_ram_classes(&mut self, list: &[(u32, RamClass)]) -> bool {
		let mut changed = false;
		let tfield = crate::gsfull::texreq::field(); // [b51 G3]
		for (key, rc) in list {
			let ptr = Arc::as_ptr(rc) as usize;
			if self.ram_built.get(key) == Some(&ptr) || self.ram_wait.get(key) == Some(&(ptr, tfield)) {
				continue;
			}
			let slot_of = |k: u32| (k & 0xffff).wrapping_sub(GADGET_TEXTURE_BASE) as usize;
			// [b54] A face of a level class decoded with the level file's textures (scene.rs `LevelTex`) carries a level moby texture
			// key (below GADGET_TEXTURE_BASE): its texture is the level's (`tex`), nothing is read from GS memory for it.
			let level_key = |k: u32| (k & 0xffff) < GADGET_TEXTURE_BASE;
			if is_part(*key) && rc.1.iter().any(|&t| base_unset(t)) {
				continue;
			}
			// Decided once per pass (texreq may finish a read meanwhile): the faces skipped while waiting are the ones dropped.
			let scr: Vec<bool> = rc.1.iter().map(|&t| reads_screen(t)).collect();
			log_screen_reads(*key, &rc.1, &scr);
			let mut ready = true;
			for &(k, _) in &rc.0.mesh.faces {
				if k & 0xffff == FLAT_SLOT || level_key(k) {
					continue;
				}
				if scr.get(slot_of(k)).copied().unwrap_or(false) {
					if SCREEN_FROM_PICTURE {
						ready &= self.screen_key(rc.1[slot_of(k)]).is_some();
					}
					continue;
				}
				match rc.1.get(slot_of(k)).and_then(|&t| crate::gsfull::texreq::get_versioned(t).map(|x| (t, x))) {
					Some((t0, (t, ver))) => {
						let tk = self.ram_key(t0, ver);
						if !self.tex.contains_key(&tk) {
							let l0 = gs_level0(t0, &t);
							ram_chain(tk, t0, &l0);
							self.tex.insert(tk, l0);
						}
					}
					None => {
						ready = false;
						log_waiting(*key, rc.1[slot_of(k)]);
					}
				}
			}
			if !ready {
				self.ram_wait.insert(*key, (ptr, tfield)); // [b51 G3]
				continue;
			}
			self.ram_wait.remove(key);
			let mut c = rc.0.clone();
			// The drop rule (build 31): a screen spot's faces are not drawn (see SCREEN_FROM_PICTURE).
			if !SCREEN_FROM_PICTURE {
				c.mesh.faces.retain(|f| f.0 & 0xffff == FLAT_SLOT || level_key(f.0) || !scr.get(slot_of(f.0)).copied().unwrap_or(false));
			}
			for f in &mut c.mesh.faces {
				// The cull and CLAMP wrap bits (24..28) stay; the texture is the GS one (a screen spot's: the picture's, `screen_key`).
				let k = if f.0 & 0xffff == FLAT_SLOT {
					WHITE_KEY
				} else if level_key(f.0) {
					// [b54] The level texture's key as it is (white if the level has no such texture: a face key without a texture
					// makes the engine refuse the whole scene).
					let lk = f.0 & rac_formats::mobies::KEY_MASK;
					if self.tex.contains_key(&lk) { lk } else { WHITE_KEY }
				} else {
					let t = rc.1[slot_of(f.0)];
					let spot = scr.get(slot_of(f.0)).copied().unwrap_or(false).then(|| self.scr_tex.get(&t).map(|e| e.0)).flatten();
					// A GS texture with mip levels draws with its TEX1 K code (the GS level choice, `ram_chain`).
					spot.or_else(|| self.ram_tex.get(&t).map(|e| e.0 | k_code(t) << rac_formats::mobies::K_CODE_SHIFT)).unwrap_or(WHITE_KEY)
				};
				f.0 = f.0 & 0xf << 24 | k;
			}
			let id = *key;
			// A class the game rebuilt with the very same mesh (its RAM copy was written again) draws as before: the engine's slots
			// keep their vertices and indices (no full rebuild of every slot, `regrow_props`).
			let same = self.meshes.get(&id).is_some_and(|old| same_mesh(old, &c.mesh));
			// Same geometry with other texture keys: only those slots' layout is made again (`sync`).
			let retex = !same && self.meshes.get(&id).is_some_and(|old| same_geometry(old, &c.mesh));
			// [b51 G2] Moved vertices only (same faces, keys, vertex count): written in place (`reshape`), not a rebuild of every slot.
			let reshape = !same && !retex && self.meshes.get(&id).is_some_and(|old| same_topology(old, &c.mesh));
			// [b52 H4] Other texture keys AND moved vertices (same face vertex indices, strip phases, array lengths): the retexture lays
			// the draws out again (it never reads the vertices), the reshape then writes the vertices; no rebuild of every slot.
			let retex_reshape = !same && !retex && !reshape && self.meshes.get(&id).is_some_and(|old| same_layout(old, &c.mesh));
			self.meshes.insert(id, c.mesh.clone());
			self.decoded.insert(id, Arc::new(c));
			self.ram_built.insert(*key, ptr);
			if std::env::var_os("RAC2_CRTCLOG").is_some() {
				eprintln!("ram class {key:#x} built: {} faces ({} screen-spot TEX0s dropped)", self.meshes[key].faces.len(), scr.iter().filter(|&&s| s).count());
				for &t in &rc.1 {
					eprintln!("  TEX0 {t:016x} screen spot {} uploaded lately {:?}", screen_spot(t), crate::gsfull::texreq::uploaded_lately(t));
				}
			}
			if retex_reshape && self.by_class.contains_key(&id) {
				if !self.retex.contains(&id) {
					self.retex.push(id);
				}
				if !self.reshape.contains(&id) {
					self.reshape.push(id);
				}
			} else if reshape && self.by_class.contains_key(&id) {
				if !self.reshape.contains(&id) {
					self.reshape.push(id);
				}
			} else if retex && self.by_class.contains_key(&id) {
				if !self.retex.contains(&id) {
					self.retex.push(id);
				}
			} else {
				changed |= !same && self.by_class.contains_key(&id);
			}
		}
		self.tex.entry(WHITE_KEY).or_insert_with(|| Texture { w: 1, h: 1, rgba: vec![[255; 4]] });
		changed
	}

	/// The GS textures whose texels changed in GS memory since the key was made (gsfull::texreq refreshes them): the key keeps
	/// its one entry in `tex`, with the new texels, and the engine uploads its slots again in place. Returns the count.
	pub fn refresh_ram_textures(&mut self, gpu: &mut Gpu) -> usize {
		let mut changed = self.changed_ram_textures(crate::gsfull::texreq::get_versioned);
		if SCREEN_FROM_PICTURE {
			changed.extend(self.refresh_screen_textures());
		}
		if std::env::var_os("RAC2_TEXLOG").is_some() {
			static CALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
			if CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 1000 == 0 {
				eprintln!("rac2: texture store: {} GS keys, {} textures, {} prop slots, {} changed now", self.ram_tex.len(), self.tex.len(), self.slots.len(), changed.len());
			}
		}
		for &k in &changed {
			gpu.replace_texture_slots(k, &self.tex);
		}
		changed.len()
	}

	/// The key of screen-spot TEX0 `t` (its texture decoded over the newest picture, screenspot.rs): made once per TEX0 in SCREEN_GROUP.
	/// None until texreq has read `t` (its CLUT).
	fn screen_key(&mut self, t: u64) -> Option<u32> {
		if let Some(e) = self.scr_tex.get(&t) {
			return Some(e.0);
		}
		let (tex, seq) = crate::screenspot::texture(t)?;
		let k = SCREEN_GROUP << 16 | (self.scr_tex.len() as u32 + 1).min(0xffff);
		self.scr_tex.insert(t, (k, seq));
		self.tex.insert(k, tex);
		Some(k)
	}

	/// The screen-spot textures decoded again over a newer picture (`screenspot::seq`); their keys.
	fn refresh_screen_textures(&mut self) -> Vec<u32> {
		crate::screenspot::set_active(!self.scr_tex.is_empty());
		let now = crate::screenspot::seq();
		let mut out = Vec::new();
		for (&t, e) in self.scr_tex.iter_mut().filter(|e| e.1 .1 != now) {
			let Some((tex, seq)) = crate::screenspot::texture(t) else { continue };
			e.1 = seq;
			self.tex.insert(e.0, tex);
			out.push(e.0);
		}
		out
	}

	/// The key of a GS texture: made once per TEX0 (the first call), the same key for every later version.
	fn ram_key(&mut self, t0: u64, ver: u32) -> u32 {
		let n = self.ram_tex.len() as u32 + 1;
		// [b50 V7] From the 65535th word of a level visit every new word would share key 0x20ffff (one picture for all of them).
		if n == 0xffff && !self.ram_tex.contains_key(&t0) {
			eprintln!("rac2: texture stale TEX0 {t0:016x} field {} cache pack-key: 65535 GS texture keys in this level, new words share one key", crate::gsfull::texreq::field());
		}
		self.ram_tex.entry(t0).or_insert((RAM_GROUP << 16 | n.min(0xffff), ver)).0
	}

	/// Puts the texels `latest` now holds for the keys that changed into `tex`; the changed keys.
	fn changed_ram_textures(&mut self, latest: impl Fn(u64) -> Option<(Arc<Texture>, u32)>) -> Vec<u32> {
		let mut out = Vec::new();
		for (t0, e) in self.ram_tex.iter_mut() {
			let Some((t, ver)) = latest(*t0).filter(|x| x.1 != e.1) else { continue };
			e.1 = ver;
			let l0 = gs_level0(*t0, &t);
			ram_chain(e.0, *t0, &l0);
			self.tex.insert(e.0, l0);
			out.push(e.0);
		}
		out
	}

	/// The distinct face keys of `classes`' meshes.
	fn face_keys(&self, classes: &[u32]) -> Vec<u32> {
		let mut seen = std::collections::HashSet::new();
		classes.iter().filter(|c| seen.insert(**c)).filter_map(|c| self.meshes.get(c)).flat_map(|m| m.faces.iter().map(|f| f.0)).collect::<std::collections::BTreeSet<u32>>().into_iter().collect()
	}

	/// Test log: the face keys of the slot classes whose texture (by `KEY_MASK` key) is not among the class textures.
	pub fn missing_textures(&self) -> Vec<u32> {
		self.face_keys(&self.slots).into_iter().filter(|k| !self.tex.contains_key(&(k & rac_formats::mobies::KEY_MASK))).collect()
	}

	/// The slot order of a new scene: one slot per drawn moby of `scene` (with a mesh), grouped by class, plus room.
	pub fn initial_slots(&mut self, scene: &Scene) -> Vec<u32> {
		self.slots.clear();
		self.by_class.clear();
		self.grow(scene);
		self.stale = false;
		self.slots.clone()
	}

	/// More slots for the classes `scene` draws more mobies of than the pool has; the new classes, in slot order.
	fn grow(&mut self, scene: &Scene) -> Vec<u32> {
		let mut need: HashMap<u32, usize> = HashMap::new();
		for m in &scene.mobies {
			for id in draw_ids(m.key, m.mask) {
				if self.meshes.get(&id).is_some_and(|x| !x.faces.is_empty()) {
					*need.entry(id).or_default() += 1;
				}
			}
		}
		let mut add: Vec<(u32, usize)> = need.into_iter().filter_map(|(c, n)| {
			let have = self.by_class.get(&c).map_or(0, |v| v.len());
			// Room for a few more of the same class (spawned effects come and go in bursts).
			(n > have).then(|| (c, n + n / 4 + 1 - have))
		}).collect();
		add.sort_unstable();
		let mut new = Vec::new();
		for (c, k) in add {
			for _ in 0..k {
				self.by_class.entry(c).or_default().push(self.slots.len());
				self.slots.push(c);
				new.push(c);
			}
		}
		new
	}

	/// Brings the engine's prop slots up to this field's moby list: new slots appended (`append_props`), all slots rebuilt
	/// (`regrow_props`) when a class mesh changed or the append could not be done. False when the engine has no scene to grow.
	pub fn sync(&mut self, gpu: &mut Gpu, scene: &Scene) -> bool {
		let at = self.slots.len();
		crate::host::framelog::sub(0);
		self.refresh_ram_textures(gpu);
		self.guard.field(scene, &self.ram_tex, &self.tex, level0_screen); // [b50 V7]
		crate::host::framelog::sub(7);
		let new = self.grow(scene);
		let retex = std::mem::take(&mut self.retex);
		let reshape = std::mem::take(&mut self.reshape); // [b51 G2]
		crate::host::framelog::sub(8);
		if self.stale || !new.is_empty() || !retex.is_empty() {
			let which = if self.stale { self.slots.clone() } else { [new.as_slice(), retex.as_slice()].concat() };
			let keys = self.face_keys(&which);
			gpu.add_texture_slots(&keys, &self.tex);
		}
		crate::host::framelog::sub(9);
		// Classes with only new texture keys: their slots are laid out again before any append (the append reads the class faces).
		if !self.stale && !retex.is_empty() && !gpu.retexture_props(&retex, &self.meshes) {
			self.stale = true;
		}
		if !self.stale && !new.is_empty() {
			let order: Vec<usize> = (0..self.slots.len()).collect();
			self.stale = !gpu.append_props(at, &new, &self.meshes, &order);
		} else if !new.is_empty() {
			self.stale = true;
		}
		// [b51 G2] Classes whose vertices moved: only their slots' vertices are written (a full regrow below writes them anyway).
		if !self.stale && !reshape.is_empty() && !gpu.reshape_props(&reshape, &self.meshes) {
			self.stale = true;
		}
		crate::host::framelog::sub(10);
		if self.stale {
			if !gpu.regrow_props(&self.slots, &self.meshes) {
				return false;
			}
			self.stale = false;
		}
		crate::host::framelog::sub(11);
		if let (Some(m), Some(h)) = (scene.mobies.first(), scene.hero_group.first()) {
			if m.pos == [h[0], h[1], h[2]] && self.by_class.contains_key(&class_id(m.key)) {
				crate::fur::follow_hero(class_id(m.key)); // the first moby record is Ratchet (scene.rs `hero_group`)
			}
		}
		gpu.fur_prop_follow(&self.meshes, &self.tex); // [PC] fur on Ratchet follows the class mesh the slots draw (furfx/prop.rs)
		true
	}

	/// Test log: the moby classes of `scene` this field does not draw, each with why (no slot, not decoded).
	pub fn undrawn(&self, scene: &Scene) -> Vec<(u32, &'static str)> {
		let mut out: Vec<(u32, &'static str)> = Vec::new();
		for m in &scene.mobies {
			for id in draw_ids(m.key, m.mask) {
				let why = match (self.by_class.contains_key(&id), self.decoded.contains_key(&id)) {
					(_, false) => "not decoded",
					(false, true) => "no slot",
					_ => continue,
				};
				if !out.iter().any(|o| o.0 == id) {
					out.push((id, why));
				}
			}
		}
		out
	}

	/// The engine must rebuild every slot (a class mesh changed).
	pub fn mark_stale(&mut self) {
		self.stale = true;
	}

	/// This field's props: each drawn moby in a slot of its class, the other slots hidden. A moby the game left out (`Moby::cut`)
	/// takes a slot only when `restore` shows it (side restore) or casts it (Culling Off: the engine then draws it in the main pass).
	pub fn frame(&self, scene: &Scene, restore: &crate::moby_cull::Restore) -> Frame {
		let mut props: Vec<Prop> = self.slots.iter().map(|&c| hidden(c)).collect();
		let (mut centres, mut radii) = (vec![[0.0; 3]; props.len()], vec![None; props.len()]);
		let (mut fx_posed, mut late) = (HashMap::new(), Vec::new());
		let (mut tints, mut tint_ends) = (vec![None; props.len()], vec![u32::MAX; props.len()]);
		let mut used: HashMap<u32, usize> = HashMap::new();
		for m in &scene.mobies {
			let (shown, cast) = m.cut.as_ref().map_or((true, false), |c| restore.verdict(c));
			if !shown && !cast {
				continue;
			}
			// A moby with a part mask takes one slot per class it draws (`draw_ids`).
			for id in draw_ids(m.key, m.mask) {
				let Some(slots) = self.by_class.get(&id) else { continue };
				let k = used.entry(id).or_default();
				let Some(&si) = slots.get(*k) else { continue };
				*k += 1;
				let Some(c) = self.decoded.get(&id) else { continue };
				props[si] = Prop { shown, cast, ..placed(m, c, id) };
				(centres[si], radii[si]) = sphere(m, c);
				if let Some((rgb, start, end)) = class_tint(m.tint, c) {
					(tints[si], tint_ends[si]) = (Some((rgb, start)), end);
				}
				if let Some(p) = &m.pose {
					if !c.skin.is_empty() {
						let (mut ps, mut ns) = rac_formats::anim::skin(c, p);
						if !c.metal.is_empty() {
							let (mp, mn) = rac_formats::anim::skin_metal(c, p);
							ps.extend(mp);
							ns.extend(mn);
						}
						fx_posed.insert(si, std::sync::Arc::new((ps, ns)));
					}
				}
				if m.late {
					late.push(si);
				}
			}
		}
		Frame { props, fx_posed, centres, radii, late, tints, tint_ends }
	}
}

/// A drawn moby's bounding sphere in the world, in the engine's units (1/1024 m, as R&C1's `fade_centres` / `fade_radii`): the
/// class sphere's centre turned by the moby's axes and scaled by its instance factor, from its position, and its radius.
fn sphere(m: &Moby, c: &MobyClass) -> ([f32; 3], Option<f32>) {
	let k = m.scale / c.scale.max(1e-6);
	let axis = |v: [f32; 3]| {
		let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
		if l > 0.0 { v.map(|x| x / l * k) } else { v }
	};
	let r = [axis(m.rows[0]), axis(m.rows[1]), axis(m.rows[2])];
	let b = c.bsphere;
	let centre = std::array::from_fn(|a| (m.pos[a] + r[0][a] * b[0] + r[1][a] * b[1] + r[2][a] * b[2]) * 1024.0);
	(centre, Some(b[3] * k * 1024.0))
}

/// A colour override word as shader RGB (byte / 128, R = low byte; byte 3 is the draw alpha).
fn tail_rgb(w: u32) -> [f32; 3] {
	[0, 8, 16].map(|s| (w >> s & 0xff) as f32 / 128.0)
}

fn hidden(class_id: u32) -> Prop {
	Prop { class_id, pos: [0.0; 3], rot: [0.0; 3], mirror: false, order: 0, shown: false, retail_drawn: false, cast: false, anim: rac_formats::anim::PropAnim::new(0), scale: 1.0, tint: None, light: None, index: None, sched: Sched::default(), kind: Kind::Scenery }
}

/// A drawn moby as a prop: Euler angles of its draw basis (a left-handed basis, R&C1's mirrored mobies, as `mirror`), the
/// instance factor of its scale (the class mesh carries the class scale), its ambient and light word.
fn placed(m: &Moby, c: &MobyClass, id: u32) -> Prop {
	let n = |v: [f32; 3]| {
		let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
		if l > 0.0 { v.map(|x| x / l) } else { v }
	};
	let (r0, mut r1, r2) = (n(m.rows[0]), n(m.rows[1]), n(m.rows[2]));
	let det = r0[0] * (r1[1] * r2[2] - r1[2] * r2[1]) - r0[1] * (r1[0] * r2[2] - r1[2] * r2[0]) + r0[2] * (r1[0] * r2[1] - r1[1] * r2[0]);
	let mirror = det < -0.5;
	if mirror {
		r1 = r1.map(|x| -x);
	}
	let rot = rac_engine::viewer::euler_from_rows(r0, r1, r2);
	let scale = if c.scale != 0.0 { m.scale / c.scale } else { 1.0 };
	Prop { class_id: id, pos: m.pos, rot, mirror, order: 0, shown: true, retail_drawn: false, cast: false, anim: rac_formats::anim::PropAnim::new(0), scale, tint: None, light: Some(m.light), index: None, sched: Sched::default(), kind: Kind::Scenery }
}

#[cfg(test)]
mod tests {
	use super::*;

	/// [b51 G2] Moved vertices with the same faces are a reshape; other faces, keys or vertex counts are not.
	#[test]
	fn moved_vertices_are_the_same_topology() {
		let mut a = rac_formats::mesh::Mesh::new(String::new());
		a.positions = vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
		a.faces = vec![(5, [0, 1, 2])];
		let mut b = a.clone();
		b.positions[1] = [2.0, 0.0, 0.0];
		assert!(same_topology(&a, &b) && !same_mesh(&a, &b));
		let mut c = a.clone();
		c.faces[0].0 = 6;
		assert!(!same_topology(&a, &c));
		let mut d = a.clone();
		d.positions.push([1.0; 3]);
		assert!(!same_topology(&a, &d));
	}

	/// [b52 H4] Moved vertices with other texture keys (same face indices) are a layout match, not a topology match.
	#[test]
	fn moved_vertices_with_other_keys_are_the_same_layout() {
		let mut a = rac_formats::mesh::Mesh::new(String::new());
		a.positions = vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
		a.faces = vec![(5, [0, 1, 2])];
		let mut b = a.clone();
		b.positions[1] = [2.0, 0.0, 0.0];
		b.faces[0].0 = 6;
		assert!(same_layout(&a, &b) && !same_topology(&a, &b) && !same_geometry(&a, &b));
		let mut c = b.clone();
		c.faces[0].1 = [0, 2, 1];
		assert!(!same_layout(&a, &c));
		let mut d = b.clone();
		d.positions.push([1.0; 3]);
		assert!(!same_layout(&a, &d));
	}

	fn ids(key: u32, mask: u16) -> Vec<u32> {
		draw_ids(key, mask).collect()
	}

	/// Screen reads are told by the frame buffers the CRTC shows, not by the texture's size [C: rec_user9 49885, class 0xc89 16 x 16].
	#[test]
	fn a_screen_read_texture_is_told_from_an_uploaded_one() {
		// PAL 512 x 512 (the user's card): one shown buffer at block 0, 0x1000 blocks (round 3 batch 26, RAC2_CRTCLOG).
		note_shown(Some((0, 8, 0, 0, 512, 512)));
		// The Heavy Lancer shot (class 0xa26): PSMT8, TBP 0, 128 x 128; the Lancer's class 0xc89 before its base is written: 16 x 16.
		assert!(screen_spot(0x8005f565dd308000) && screen_spot(0x8005f665dd308000));
		assert!(screen_spot(0x8006200511304000), "16 x 16 PSMT8 at TBP 0");
		assert!(screen_spot(0x8005b76555304000), "32 x 32 PSMT8 at TBP 0");
		assert!(!screen_spot(0x8005800621b10000), "PSMT8H alpha-plane texture");
		assert!(!screen_spot(0x8005f565dd301000), "TBP 0x1000 is past the shown buffer");
		// Another video mode: a shown buffer at block 0x3000, 512 x 512 (FBW 8): its reads are caught too.
		note_shown(Some((0x3000, 8, 0, 0, 512, 512)));
		assert!(screen_spot(0x8005f565dd303000) && screen_spot(0x8005f565dd303e00) && !screen_spot(0x8005f565dd302d00));
		SHOWN.lock().unwrap().clear();
	}

	/// In a frame buffer: a slot the game uploaded texels into for the draw (debris, vendor previews) is kept; a never-uploaded or
	/// frame-written one (boot art / an old picture at block 0: the Lancer's 0xc89) is dropped; an unread one waits (not dropped yet).
	#[test]
	fn an_uploaded_slot_in_a_frame_buffer_is_kept() {
		assert!(!screen_read(true, Some(true)), "uploaded lately: a real texture");
		assert!(screen_read(true, Some(false)), "not uploaded lately: the screen");
		assert!(!screen_read(true, None), "not read yet: waits");
		assert!(!screen_read(false, Some(false)), "outside the frame buffers");
		// Not held by texreq: never a screen read before its read.
		assert!(!reads_screen(0x8006200511304000));
	}

	#[test]
	fn unset_texture_base_is_detected() {
		assert!(base_unset(0x80061be555304000));
		assert!(!base_unset(0x80061be555307850));
		assert!(is_part(part_id(0xcb4200, 0)) && !is_part(class_id(0xcb4200)));
	}

	/// A far class's TEX0 at TBP0 0 with its levels 1..2 at MIPTBP1 (the GS samples only those) is neither unset nor a screen read
	/// [C: Aranos q..5919, class 0xea3 armour TEX0 80060265dd308000].
	#[test]
	fn a_mip_only_texture_at_tbp_0_is_set() {
		let t = 0x80060265dd308000;
		assert!(base_unset(t) && !mip_only(t), "no mip registers: unset");
		crate::gsfull::texreq_mips::set(t, [0xff9200000128, 0x6c170702404000, 0]);
		assert!(mip_only(t) && !base_unset(t) && !screen_spot(t));
		crate::gsfull::texreq_mips::set(t, [0xff9200000128, 0x4000 | 1 << 34, 0]);
		assert!(!mip_only(t), "no level base: still unset");
		crate::gsfull::texreq_mips::set(t, [0x128 & !0x1c, 0x6c170702404000, 0]);
		assert!(base_unset(t), "MXL 0: no levels");
	}

	/// The levels of a `mip_only` TEX0 draw, its level 0 (a screen read) never does: see-through texels at LOD < 0.5 [C: the Heavy
	/// Lancer shot TEX0 8005f565dd308000 sits at TBP0 0 with mip registers too; build 41 drew none of it].
	#[test]
	fn a_mip_only_screen_spot_draws_no_level_0() {
		let t = 0x8005f965dd308000; // the Lancer shot's format at another CLUT: the mip register table is shared by parallel tests
		let l = Texture { w: 2, h: 1, rgba: vec![[9; 4]; 2] };
		crate::gsfull::texreq_mips::set(t, [0xff9200000128, 0x6c170702404000, 0]);
		assert!(mip_only(t) && !screen_spot(t));
		assert!(!screen_read(false, Some(false)) && screen_read(true, Some(false)) && !screen_read(true, Some(true)));
		assert_eq!(gs_level0(0x80061be555307850, &l).rgba, l.rgba, "a texture outside the frame buffers keeps its level 0");
	}

	/// Mask 0 (no parts): the whole base class, as before the part masks.
	#[test]
	fn plain_moby_draws_its_class() {
		assert_eq!(ids(0xcb4200, 0), vec![class_id(0xcb4200)]);
	}

	/// A piece (0x8000 | 1 << k) draws only part k's packets, never the base [C: window glass 0x8001..0x8008, limbs 0x8008..0x8400].
	#[test]
	fn piece_draws_only_its_part() {
		assert_eq!(ids(0xcb4200, 0x8001), vec![part_id(0xcb4200, 0)]);
		assert_eq!(ids(0xad16c0, 0x8400), vec![part_id(0xad16c0, 10)]);
		assert_eq!(ids(0xad16c0, 0x8000 | 0b101), vec![part_id(0xad16c0, 0), part_id(0xad16c0, 2)]);
	}

	/// An intact moby with a positive mask draws the base class and every attached part.
	#[test]
	fn intact_moby_draws_base_and_attached_parts() {
		assert_eq!(ids(0xad16c0, 0b110), vec![class_id(0xad16c0), part_id(0xad16c0, 1), part_id(0xad16c0, 2)]);
	}

	/// Class ids and part ids never collide: normal keys, the boot outlier 0xff9xxxxx, every part number.
	#[test]
	fn ids_are_distinct() {
		let mut all = std::collections::HashSet::new();
		let keys = [0x0, 0x928590, 0xcb4200, 0x01ff_fff0, 0xff92_8590, 0xff00_0000];
		for key in keys {
			assert!(all.insert(class_id(key)), "class {key:#x}");
			assert_eq!(class_id(key) & 0x8000_0000, 0);
			for k in 0..15 {
				assert!(all.insert(part_id(key, k)), "part {k} of {key:#x}");
				assert_ne!(part_id(key, k) & 0x8000_0000, 0);
			}
		}
	}

	fn class(faces: usize) -> MobyClass {
		let mut mesh = Mesh::new("t".into());
		mesh.positions = vec![[0.0; 3]; 3];
		mesh.faces = vec![(FLAT_SLOT, [0, 1, 2]); faces];
		MobyClass { mesh, scale: 1.0, skin: Vec::new(), normals: Vec::new(), flat: Vec::new(), bsphere: [0.0; 4], colour_start: 0, joints: 1, second_last_start: 0, last_start: 0, glow: 0, collision: None, metal: Vec::new(), kind: 0, shadow_prims: Vec::new() }
	}

	/// The colour override starts at the last packet and runs to the mesh end (hacking grid red, sphere minigame dot blue as in F1); a
	/// class under 2 packets or with more than one joint gets none.
	#[test]
	fn class_tint_starts_at_the_last_packet() {
		let mut c = class(1);
		(c.second_last_start, c.last_start) = (8, 12);
		let w = 0xfe00_0080;
		assert_eq!(class_tint(Some(w), &c), Some(([1.0, 0.0, 0.0], 12, u32::MAX)), "the last packet to the mesh end");
		c.joints = 0;
		assert_eq!(class_tint(Some(w), &c), Some(([1.0, 0.0, 0.0], 12, u32::MAX)));
		c.joints = 1;
		assert_eq!(class_tint(None, &c), None);
		c.joints = 19;
		assert_eq!(class_tint(Some(w), &c), None);
		// One packet (high < 2): mobies.rs leaves both starts 0.
		(c.joints, c.second_last_start, c.last_start) = (1, 0, 0);
		assert_eq!(class_tint(Some(w), &c), None);
	}

	fn moby(key: u32, mask: u16, addr: u32) -> Moby {
		Moby { key, addr, pos: [0.0; 3], rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], scale: 1.0, mask, pose: None, light: ([0; 3], 0), tint: None, late: false, cut: None }
	}

	/// The slots of a scene: a moby takes one slot per class it draws, in the pool of that class; frame() shows each of them with its
	/// own class id (three pieces of two parts, one intact moby with two limbs on, one plain moby of another class).
	#[test]
	fn frame_gives_each_drawn_part_a_slot() {
		let (k, other) = (0xad16c0, 0x95cec0);
		let mut c = Classes::default();
		for id in [class_id(k), part_id(k, 1), part_id(k, 2), class_id(other)] {
			c.decoded.insert(id, Arc::new(class(1)));
			c.meshes.insert(id, class(1).mesh);
		}
		// Part 3 has no mesh (not decoded yet): a piece of it draws nothing.
		let scene = Scene { mobies: vec![moby(k, 0x8002, 1), moby(k, 0x8004, 2), moby(k, 0x8002, 3), moby(k, 0b110, 4), moby(other, 0, 5), moby(k, 0x8008, 6)], ..Scene::default() };
		let slots = c.initial_slots(&scene);
		let per = |id: u32| slots.iter().filter(|&&s| s == id).count();
		assert!(per(part_id(k, 1)) >= 3 && per(part_id(k, 2)) >= 2 && per(class_id(k)) >= 1 && per(class_id(other)) >= 1);
		assert_eq!(per(part_id(k, 3)), 0);
		let f = c.frame(&scene, &Default::default());
		let mut shown: Vec<u32> = f.props.iter().filter(|p| p.shown).map(|p| p.class_id).collect();
		shown.sort_unstable();
		let mut want = vec![part_id(k, 1), part_id(k, 2), part_id(k, 1), class_id(k), part_id(k, 1), part_id(k, 2), class_id(other)];
		want.sort_unstable();
		assert_eq!(shown, want);
		assert!(c.undrawn(&scene).iter().all(|u| u.0 == part_id(k, 3)), "only the part without a mesh is reported");
	}

	/// A moby the game left out (`Moby::cut`) takes no slot with Culling On and no restore; Culling Off casts it.
	#[test]
	fn a_cut_moby_shows_only_when_restored() {
		use crate::moby_cull::{Cut, Restore, Why};
		let k = 0xad16c0;
		let mut c = Classes::default();
		c.decoded.insert(class_id(k), Arc::new(class(1)));
		c.meshes.insert(class_id(k), class(1).mesh);
		let cut = Cut { why: Why::Far, side_only: false, far_ok: false, sphere: [0.0, 0.0, 0.0, 1024.0] };
		let scene = Scene { mobies: vec![moby(k, 0, 1), Moby { cut: Some(cut), ..moby(k, 0, 2) }], ..Scene::default() };
		c.initial_slots(&scene);
		let f = c.frame(&scene, &Restore::default());
		assert_eq!((f.props.iter().filter(|p| p.shown).count(), f.props.iter().filter(|p| p.cast).count()), (1, 0));
		let f = c.frame(&scene, &Restore { culling_off: true, far: false, side: None });
		assert_eq!((f.props.iter().filter(|p| p.shown).count(), f.props.iter().filter(|p| p.cast).count()), (1, 1));
	}
}

#[cfg(test)]
mod ram_tex_tests {
	use super::*;

	#[test]
	fn many_versions_of_one_tex0_keep_one_key_and_one_texture() {
		let mut c = Classes::default();
		let k = c.ram_key(0xabc, 0);
		c.tex.insert(k, Texture { w: 1, h: 1, rgba: vec![[0; 4]] });
		for v in 1..500u32 {
			assert_eq!(c.ram_key(0xabc, v), k);
			let ch = c.changed_ram_textures(|_| Some((Arc::new(Texture { w: 1, h: 1, rgba: vec![[v as u8, 0, 0, 255]] }), v)));
			assert_eq!(ch, vec![k]);
			assert_eq!(c.tex.len(), 1);
			assert_eq!(c.tex[&k].rgba[0][0], v as u8);
		}
		assert_eq!(c.changed_ram_textures(|_| Some((Arc::new(Texture { w: 1, h: 1, rgba: vec![] }), 499))), Vec::<u32>::new());
		assert_ne!(c.ram_key(0xdef, 0), k);
	}

	/// A class rebuilt with the same mesh is no change for the engine's slots; any differing vertex, face key or colour is.
	#[test]
	fn a_rebuilt_class_with_the_same_mesh_is_not_a_change() {
		let mut a = Mesh::new("a".into());
		a.positions = vec![[0.0, 1.0, 2.0]; 3];
		a.uvs = vec![[0.5; 2]; 3];
		a.faces = vec![(7, [0, 1, 2])];
		a.strip_phase = vec![0];
		let mut b = a.clone();
		b.name = "other name".into();
		assert!(same_mesh(&a, &b));
		b.faces[0].0 = 8;
		assert!(!same_mesh(&a, &b));
		b = a.clone();
		b.positions[2][1] = 1.5;
		assert!(!same_mesh(&a, &b));
		b = a.clone();
		b.colours = vec![[1.0; 4]; 3];
		assert!(!same_mesh(&a, &b));
	}
}
