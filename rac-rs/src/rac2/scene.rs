//! One field of R&C2's render state read from RAM (the game thread reads it after each field, the host draws it with the engine):
//! the render camera (`View`, as an engine `Camera`), every moby the game drew (`Moby`: class key, placement, pose, light,
//! shadow caster), the classes that are not in the level file decoded from RAM and the metal packets' environment map
//! (`Resolver`). From lane rc2's native/scene.rs (engine split stage E5).

use crate::addrs::{self, Addrs};
use crate::props::{class_id, part_id};
use rac_engine::viewer::Camera;
use rac_formats::rc2::mobies::Part;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

/// The VU1 program that draws mobies [C: Aranos runs {2, 5, 6, 7, 11}, 11 = mobies; engine gsfull/overlay.rs]: a RAM class's textures
/// are read from VRAM only at its draws (the game draws the same TEX0 word over other texels by other paths in the same field).
const MOBY_PROG: u8 = 11;

/// A class decoded from RAM and the GS TEX0 word of each of its texture slots (`rc2::mobies::moby_class_ram`).
pub type RamClass = Arc<(rac_formats::mobies::MobyClass, Vec<u64>)>;

/// Class keys (core offsets) each loaded world draws from the level file, by level (`world::load` fills it): the game thread
/// decodes every other live moby's class from RAM.
pub static KNOWN: Mutex<Vec<(usize, Arc<HashSet<u32>>)>> = Mutex::new(Vec::new());

fn known(level: i32) -> Option<Arc<HashSet<u32>>> {
	KNOWN.lock().ok()?.iter().find(|k| k.0 as i32 == level).map(|k| k.1.clone())
}

/// [b54] What the level file says each level class draws with (`world::load` fills it next to `KNOWN`): a level class decoded
/// from RAM (a record the game spawned, a part) takes its textures from the level file, not from GS memory, where the game's VRAM
/// ring puts another class's texels under the same TEX0 word a few fields later (`rc2::mobies::FileTex`).
#[derive(Default)]
pub struct LevelTex {
	/// Core offset of each AD texture slot of the level classes -> the class-local texture index the file holds there.
	pub slots: HashMap<u32, u8>,
	/// Class key -> the class's level-table remap bytes (index -> level moby texture); only classes whose every slot has a texture.
	pub remap: HashMap<u32, [u8; 16]>,
}

pub static LEVEL_TEX: Mutex<Vec<(usize, Arc<LevelTex>)>> = Mutex::new(Vec::new());

fn level_tex(level: i32) -> Option<Arc<LevelTex>> {
	LEVEL_TEX.lock().ok()?.iter().find(|k| k.0 as i32 == level).map(|k| k.1.clone())
}

/// [b54] Debug: RAC2_FILETEX=0 reads every RAM class's textures from GS memory again (the builds before 54).
fn file_tex_on() -> bool {
	static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
	*ON.get_or_init(|| std::env::var_os("RAC2_FILETEX").is_none_or(|v| v != "0"))
}

/// [b54] A TEX0 in one of the alpha-plane formats (PSMT8H, PSMT4HL, PSMT4HH): the textures the game loads at run time and keeps
/// parked for good (Ratchet's armor, the weapons [C: Aranos hero 0x10bc4c0 and weapon 0x1f0c000 words, all PSM 0x1b; the VRAM
/// ring's words are PSMT8 0x13]). A level class the game re-pointed at such a texture is not drawn with its level-file textures.
fn parked_format(tex0: u64) -> bool {
	matches!(tex0 >> 20 & 0x3f, 0x1b | 0x24 | 0x2c)
}

/// The game's render camera of one field (`addrs` CAM_*).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
	pub eye: [f32; 3],
	/// Rows = world axes; columns = view right, down, forward.
	pub basis: [[f32; 3]; 3],
	/// Focal lengths in screen pixels.
	pub fx: f32,
	pub fy: f32,
	/// Half the drawn screen in pixels (256, 224).
	pub half: [f32; 2],
}

impl View {
	pub fn read(m: &ee_oracle::Machine, a: &Addrs) -> Option<View> {
		let b = a.basis;
		if m.rdf(b + 0x3c) != 1024.0 {
			return None;
		}
		let eye = m.rdv3(b + 0x30).map(|v| v / 1024.0);
		let basis = [m.rdv3(b), m.rdv3(b + 0x10), m.rdv3(b + 0x20)];
		let (fx, fy) = (m.rdf(a.proj) * addrs::CAM_PROJ_UNIT, m.rdf(a.proj + 0x14) * addrs::CAM_PROJ_UNIT);
		let half = [m.rdf(a.half), m.rdf(a.half + 4)];
		let ok = eye.iter().chain(basis.iter().flatten()).all(|x| x.is_finite()) && fx > 1.0 && fy > 1.0 && half[0] > 1.0 && half[1] > 1.0;
		ok.then_some(View { eye, basis, fx, fy, half })
	}

	/// The picture's shape the game draws for: its horizontal over its vertical field of view, 4:3, or 16:9 with the game's own
	/// Options "16:9 (widescreen)" row (the anamorphic picture; the game culls to that view), snapped to the nearer one when within
	/// 2 %.
	pub fn aspect(&self) -> f32 {
		let a = (self.half[0] * self.fy) / (self.half[1] * self.fx);
		[4.0 / 3.0, 16.0 / 9.0].into_iter().find(|s| (a / s - 1.0).abs() < 0.02).unwrap_or(a.clamp(1.0, 2.5))
	}

	/// The engine camera of this view in a window of shape `window` (width / height): forward = basis column 2, up = minus column
	/// 1 (the view's down), the vertical field of view from `fy` (the horizontal one follows the window: a wider window shows
	/// more at the sides); a window narrower than the game's picture keeps the game's horizontal field of view.
	pub fn camera(&self, window: f32) -> Camera {
		let col = |j: usize| [self.basis[0][j], self.basis[1][j], self.basis[2][j]];
		let f = col(2);
		let d = col(1);
		let l = (f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt().max(1e-9);
		let f = f.map(|x| x / l);
		// A window narrower than the game's picture shows that picture across its width with bars above and below: the field of
		// view of the whole window height is then wider than the game's by the ratio of the shapes.
		let fit = (self.aspect() / window).max(1.0);
		Camera { pos: self.eye, yaw: f[1].atan2(f[0]), pitch: f[2].clamp(-1.0, 1.0).asin(), tan_half: self.half[1] / self.fy * fit, up: d.map(|x| -x) }
	}
}

/// One moby the game drew last field.
#[derive(Clone)]
pub struct Moby {
	/// Class key: the class header address minus the core base (wrapping).
	pub key: u32,
	/// Record address (RAM).
	pub addr: u32,
	pub pos: [f32; 3],
	/// Draw basis rows (+0xc0, +0xd0, +0xe0), unit length.
	pub rows: [[f32; 3]; 3],
	/// Moby scale (+0x2c): class scale times the instance factor.
	pub scale: f32,
	/// Part mask (+0x7c) as `rc2::mobies::part_mask` keeps it: `0x8000 | 1 << k` = a piece (only part k of the class is drawn), a
	/// positive mask = the intact moby (base packets + every set part), 0 = the whole base mesh (no parts).
	pub mask: u16,
	/// Joint skinning matrices of an animated moby (`ram_anim::pose_from_ram`).
	pub pose: Option<Arc<Vec<[f32; 12]>>>,
	/// Ambient bytes (+0x3c) and light word (+0x38: slot A, slot B << 8, blend << 16) [C: R&C1's layout; Aranos hero +0x3c 0x3f3f3f,
	/// level mobies 0x4c4f4e, +0x38 0].
	pub light: ([u8; 3], u32),
	/// The colour override: moby +0x78 (R = low byte, 0x80 = 1.0, byte 3 = draw alpha) while flag +0x34 & 0x10 is set [C: Aranos-class hacking
	/// minigame grid lines (classes 0xafa / 0xafc, state 0096): the game pulses +0x78 = 0xfe0000XX and the emulated picture's vertex colours are (XX, 0, 0)].
	/// Drawn in this colour instead of the lit one: only the class's second-to-last packet, one-joint classes ([C] README format facts, `props::class_tint`); not R&C1's colour_start rule.
	pub tint: Option<u32>,
	/// A shadow caster: drawn after the volumes and the darken (retail draws its casters on the deferred list after the volumes,
	/// so they are never darkened themselves [C: R&C1 shadows.md "Frame position"]).
	pub late: bool,
	/// [PC] Not drawn by the game this field (`+0x31` = 0): why, as `moby_cull` re-ran its tests; read only while the render thread
	/// wants it (Culling Off, the side restore). None = the game drew it.
	pub cut: Option<crate::moby_cull::Cut>,
}

/// What the engine draws of one field.
#[derive(Clone, Default)]
pub struct Scene {
	/// The fields run when it was read (the window's frame smoothing tells the fields apart by it; 0 = not set).
	pub field: u64,
	pub view: Option<View>,
	/// The game's level word (`ee_oracle::rc2::LEVEL`) at the time of the read: the world the picture belongs to.
	pub level: i32,
	/// The terrain chunk resident at the core base (`chunks`; None = no terrain header there).
	pub chunk: Option<crate::chunks::Sig>,
	pub mobies: Vec<Moby>,
	/// Classes of live mobies that are not in the level file (boot data, run-time buffers such as weapons), decoded from RAM,
	/// by engine class id (`props::class_id` of the key; the parts of a class by `props::part_id`): every one seen in this level
	/// so far.
	pub ram_classes: Arc<Vec<(u32, RamClass)>>,
	/// GS TEX0 of the metal packets' environment map (read from the hero's metal packet in RAM; gsfull::texreq).
	pub chrome: Option<u64>,
	/// The game's shadow records this field (`shadows::read_records`; empty while the game has the pass gated off).
	pub shadows: Vec<rac_engine::shadowgeom::RecCaster>,
	/// [PC] The caster moby record of each of `shadows` (0 = unknown); frame smoothing blends records of the same caster (smooth.rs).
	pub shadow_owners: Vec<u32>,
	/// Where the hero and the thing he holds are: spheres (x, y, z, radius) in world metres (`hero_group`). Retail draws his glows
	/// before him, so a glow centred inside him is hidden by his body (native layers are depth-tested against him instead).
	pub hero_group: Vec<[f32; 4]>,
	/// [PC] The game's camera-under-water flag this field (under.rs; the underwater look, parity row 15).
	pub under: bool,
	/// [PC] The live fog this field was drawn with (under.rs `fog_lagged`: camera zone fog, or the underwater constants while `under`); None = the level's static fog.
	pub fog_live: Option<crate::under::Fog>,
	/// [PC] The mobies the game drew this field, except Ratchet: (record, position, bounding radius in metres = sphere word +0xc / 1024) for the
	/// water wakes (fx_movers.rs; R&C1 hands the same, rac1/retail/snapshot.rs `moby_fx`).
	pub movers: Vec<(u32, [f32; 3], f32)>,
	/// [PC] A starship fight is on this field: hero state 0x18c134 == 49 and group 0x18c13c == 9 [C: pc_input.rs `ship`]. Space, no sun to
	/// cast from: `Gpu::ship_fight` (R&C1 sets it from `shipmouse::in_ship_fight`).
	pub ship_fight: bool,
	/// [PC] The pause menu's root page is current in RAM after this field (modern_screen.rs `pause_up`, read only): the game
	/// re-points its camera from that field on, before the GS flags the frozen run (host/freeze.rs holds the world from before it).
	pub paused: bool,
}

/// The RAM addresses of the running level's render state (`Addrs`), found again whenever the level word changes or the camera
/// block stops looking like one, and the RAM class decodes.
#[derive(Default)]
pub struct Resolver {
	level: i32,
	addrs: Option<Addrs>,
	/// RAM class decodes by key.
	ram: HashMap<u32, RamEntry>,
	ram_list: Arc<Vec<(u32, RamClass)>>,
	chrome: Option<u64>,
	/// The hero class header `chrome` was read from (`moby_at`).
	chrome_hdr: u32,
	/// Units whose TEX0 word walk failed (logged once).
	words_bad: HashSet<u32>,
	/// The host's number of the field being read (texreq: a class's new TEX0 word is read only at GS fields after it).
	pub field: u64,
	/// [b50 V2] `class_stamp::stamp` per class header, computed once per field: (field, stamp).
	stamps: HashMap<u32, (u64, u64)>,
	/// [b54] The level file's texture slots of this level's classes (`LEVEL_TEX`; None until the world is loaded).
	file_tex: Option<Arc<LevelTex>>,
	/// [b54] The level core's RAM base of the field being read (a slot's RAM address minus it = its core offset).
	base: u32,
	/// [b54] Units whose decode with the level file's textures failed (logged once).
	file_bad: HashSet<u32>,
}

impl Resolver {
	pub fn get(&mut self, m: &ee_oracle::Machine, level: i32) -> Option<Addrs> {
		if level != self.level {
			self.level = level;
			self.addrs = None;
			self.ram.clear();
			self.stamps.clear();
			self.words_bad.clear();
			self.file_bad.clear();
			self.file_tex = None;
			self.ram_list = Arc::default();
			crate::gsfull::texreq::set_in_use(std::iter::empty());
			crate::gsfull::texreq::level_reset();
			(self.chrome, self.chrome_hdr) = (None, 0);
		}
		if let Some(a) = self.addrs {
			if m.rdf(a.basis + 0x3c) == 1024.0 && !crate::space_flight::stale(&m.cpu.mem.ram, &a) { // [b47 X5] found again when the flight starts / ends
				return Some(a);
			}
			self.addrs = None;
		}
		// [b47 X5] In the planet-to-planet flight: the flight's own moby table (space_flight.rs `addrs`: the ship).
		self.addrs = crate::scan_timing::timed("resolver_addrs", || crate::space_flight::addrs(&m.cpu.mem.ram, Addrs::find(&m.cpu.mem.ram)));
		if let Some(a) = self.addrs {
			eprintln!("rac2: level {level} render state at camera {:x}, moby table {:x}, core {:x}", a.basis, a.moby, a.core);
		}
		self.addrs
	}

	/// Decode the RAM class at `hdr` once (again when its header words change: a run-time buffer reused by another class).
	fn ram_class(&mut self, m: &ee_oracle::Machine, hdr: u32, key: u32) {
		self.ram_unit(m, hdr, key, class_id(key), None);
	}

	/// The parts of the class at `hdr` whose bits are set in `mask`, each decoded as a class of its own under `part_id` (the same
	/// rules as `ram_class`: once, again when the header changes or a TEX0 base is still 0).
	fn ram_parts(&mut self, m: &ee_oracle::Machine, hdr: u32, key: u32, mask: u16) {
		let parts = rac_formats::rc2::mobies::parts(&m.cpu.mem.ram[..32 << 20], hdr as usize);
		for p in &parts {
			if mask >> p.bit & 1 != 0 {
				self.ram_unit(m, hdr, key, part_id(key, p.bit), Some(*p));
			}
		}
		// [b49 U2b] A part this class does not have (any more): the class at a reused buffer (the held weapon) was another weapon's,
		// and that weapon's part kept drawing under the same part id.
		for bit in (0..15u32).filter(|&b| mask >> b & 1 != 0 && !parts.iter().any(|p| p.bit == b)) {
			self.blank_unit(part_id(key, bit));
		}
	}

	/// [b49 U2b] The unit `id` decoded before is no longer a class that exists: it draws nothing, and decodes again when it returns.
	fn blank_unit(&mut self, id: u32) {
		let Some(e) = self.ram.get_mut(&id) else { return };
		let Some(old) = e.class.as_ref().filter(|c| !c.0.mesh.faces.is_empty() || !c.0.metal.is_empty()).cloned() else { return };
		let blank = Arc::new(blanked(&old));
		(e.class, e.sig, e.words) = (Some(blank.clone()), u64::MAX, Vec::new());
		let l: Vec<(u32, RamClass)> = self.ram_list.iter().map(|x| if x.0 == id { (id, blank.clone()) } else { x.clone() }).collect();
		crate::gsfull::texreq::set_in_use(l.iter().flat_map(|e| e.1 .1.iter().copied()));
		self.ram_list = Arc::new(l);
	}

	/// One decode unit of the class at `hdr`: the base class (`part` None) or one part, stored under its engine class `id`. A built
	/// class is decoded again when the TEX0 words in its packets change: the game streams moby textures through a VRAM cache and
	/// rewrites a class's TEX0 (TBP0 / CBP) when its texture moves, so the same TEX0 word later holds another class's texels [C:
	/// texcheck, Aranos TBP 0x3200..0x33ff PSMT8]. [b54] A level class (`ckey` = its class key, in `LevelTex::remap`) is decoded
	/// with the level file's textures instead (`rc2::mobies::moby_class_ram_file`): no TEX0 of it is read from GS memory, so the
	/// ring's moves cannot put another class's texels on it. Not while one of its slots names a parked run-time texture
	/// (`parked_format`), and not when a slot is not the level file's: those decode with the TEX0 words as before.
	fn ram_unit(&mut self, m: &ee_oracle::Machine, hdr: u32, ckey: u32, id: u32, part: Option<Part>) {
		let key = id;
		let ram = &m.cpu.mem.ram[..32 << 20];
		let sig = ((m.rd32(hdr + 4) as u64) << 32 | m.rd32(hdr + 0x24) as u64) ^ part.map_or(0, |p| (p.first as u64) << 40 | (p.count as u64) << 48);
		// [b49 U2b] The class's contents too: the held weapon is one buffer / one class id for every weapon, and two weapons can share both header words and the TEX0 list.
		let field = self.field;
		let stamp = match self.stamps.get(&hdr) {
			Some(&(f, s)) if f == field => s,
			_ => {
				let s = crate::class_stamp::stamp(ram, hdr);
				self.stamps.insert(hdr, (field, s));
				s
			}
		};
		if let Some(e) = self.ram.get_mut(&key).filter(|e| e.sig == sig) {
			// [b50 V2] Other contents under the same header words: decode again once the same new stamp was seen in two fields
			// (a stamp that still changes, or flips back, is a load in progress or a toggle: not decoded).
			let (same, pending) = crate::class_stamp::debounce(e.stamp, e.pending, stamp, field);
			e.pending = pending;
			match ram_step(same, words_unchanged(ram, &e.words), e.wait) {
				RamStep::Skip => return,
				RamStep::Wait => {
					e.wait -= 1;
					return;
				}
				RamStep::Decode => {}
			}
		}
		let tex_words = rac_formats::rc2::mobies::tex0_words(ram, hdr as usize, part.map(|p| (p.first, p.count)));
		// [b54] The level file's textures for a level class.
		let from_file = match (&tex_words, self.file_tex.as_ref().and_then(|l| l.remap.get(&ckey).map(|r| (l.clone(), *r)))) {
			(Ok(w), Some((l, remap))) if !w.iter().any(|&(_, t)| parked_format(t)) => {
				let base = self.base;
				let slot = |at: usize| l.slots.get(&(at as u32).wrapping_sub(base)).map(|&i| i as u32);
				match rac_formats::rc2::mobies::moby_class_ram_file(ram, hdr as usize, 0, part.map(|p| (p.first, p.count)), &rac_formats::rc2::mobies::FileTex { slot: &slot, remap: &remap }) {
					Ok(c) => Some(c),
					Err(e) => {
						if self.file_bad.insert(key) && log_tex0() {
							eprintln!("rac2: RAM class at {hdr:#x}{}: no level-file textures ({e}); its TEX0 words are read", part.map_or(String::new(), |p| format!(" part {}", p.bit)));
						}
						None
					}
				}
			}
			_ => None,
		};
		let file = from_file.is_some();
		let decoded = match (from_file, part) {
			(Some(c), _) => Ok((c, Vec::new())),
			(None, None) => rac_formats::rc2::mobies::moby_class_ram(ram, hdr as usize, 0),
			(None, Some(p)) => rac_formats::rc2::mobies::moby_class_ram_part(ram, hdr as usize, 0, p.first, p.count),
		};
		// A failed word walk must not read as "unchanged" for ever (the batch-12 bug again): log it once per unit and decode again on
		// the countdown.
		let (words, words_err) = match tex_words {
			// The slots' mip registers are watched too (the game can move a class's levels 1..MXL without rewriting its TEX0).
			Ok(mut w) => {
				w.extend(rac_formats::rc2::mobies::mip_words(ram, hdr as usize, part.map(|p| (p.first, p.count))).unwrap_or_default());
				(w, false)
			}
			Err(e) => {
				if self.words_bad.insert(key) {
					eprintln!("rac2: RAM class at {hdr:#x}: TEX0 words not readable ({e}); decoding again every {RECHECK} calls");
				}
				(Vec::new(), true)
			}
		};
		// [b46 U5] A slot word at TBP0 0 as the game wrote it (before `resident` renames mip-only words): the RECHECK countdown below.
		let mut tbp0_zero = false;
		let c = match decoded {
			Ok(mut c) => {
				// A word the class did not use before is read only at GS fields after this one (the game thread runs ahead; an earlier
				// draw of the word shows its previous owner's texels); a held one is renewed (texreq::want_from).
				let old: Vec<u64> = self.ram.get(&key).filter(|e| e.sig == sig).and_then(|e| e.class.as_ref()).map(|o| o.1.clone()).unwrap_or_default();
				let from = self.field + 1;
				tbp0_zero = c.1.iter().any(|&t| t & 0x3fff == 0);
				// Each slot's TEX1 / MIPTBP1 / MIPTBP2: texreq reads the levels the GS samples with them (gsfull::texreq_mips).
				// [b54] Not for a class with the level file's textures: none of its words is read from GS memory.
				let mips = if file { Vec::new() } else { rac_formats::rc2::mobies::slot_mips(ram, hdr as usize, part.map(|p| (p.first, p.count))).unwrap_or_default() };
				for (t, mm) in mips {
					crate::gsfull::texreq_mips::set(t, [mm.tex1, mm.mip1, mm.mip2]);
					// [b46 U5] A mip-only word (TBP0 0: level 0 not in VRAM, the word shared by every class the VRAM ring parks there) is
					// this class's texture as its first resident level's word with the levels after it (gsfull::texreq_resident): no
					// level 0 decoded from the frame buffer at block 0 (noise / vanishing when close), no other class's levels.
					if let Some((w, r)) = crate::gsfull::texreq_resident::resident(t, &[mm.tex1, mm.mip1, mm.mip2]) {
						crate::gsfull::texreq_mips::set(w, r);
						c.1.iter_mut().filter(|x| **x == t).for_each(|x| *x = w);
					}
				}
				c.1.iter().for_each(|&t| crate::gsfull::texreq::want_from(t, MOBY_PROG, if old.contains(&t) { 0 } else { from }));
				Some(Arc::new(c))
			}
			Err(e) => {
				eprintln!("rac2: RAM class at {hdr:#x}{}: {e}", part.map_or(String::new(), |p| format!(" part {}", p.bit)));
				None
			}
		};
		// The game writes a class's texture bases (TEX0 TBP0) after uploading its textures, without touching the header words of
		// `sig`: a class decoded before that has TBP0 0 [C: Aranos 0x129ed80 TEX0 80059585dd308000 early, 80059585dd30b1d2
		// later]. Such a class is decoded again every 25 calls until its bases are set (or at once when its TEX0 words change).
		// [b46 U5] Retail streams a mip-only class's level 0 in when close (TBP0 set, `resident` then keeps the word): checked as before.
		let wait = if words_err || tbp0_zero { RECHECK } else { 0 };
		// Decoded again with the same TEX0s: keep the built one (no GPU rebuild).
		if let (Some(e), Some(n)) = (self.ram.get_mut(&key).filter(|e| e.sig == sig), &c) {
			if e.stamp == stamp && e.class.as_ref().is_some_and(|o| keep_built(&o.1, &n.1)) { // [b49 U2b] other contents: never keep the old build
				(e.wait, e.words) = (wait, words);
				return;
			}
			if log_tex0() {
				eprintln!("texcheck: f{} class TEX0 changed hdr {hdr:#x} id {id:x}: {:x?} -> {:x?}", crate::gsfull::texreq::field(), e.class.as_ref().map(|o| o.1.clone()).unwrap_or_default(), n.1);
			}
		}
		if let Some(c) = &c {
			if std::env::var_os("RAC2_LOG").is_some() {
				eprintln!("rac2: RAM class at {hdr:#x}{}: {} triangles, {}TEX0 {:x?}", part.map_or(String::new(), |p| format!(" part {}", p.bit)), c.0.mesh.faces.len(), if file { "level-file textures, " } else { "" }, c.1);
			}
			let mut l: Vec<(u32, RamClass)> = self.ram_list.iter().filter(|e| e.0 != key).cloned().collect();
			l.push((key, c.clone()));
			crate::gsfull::texreq::set_in_use(l.iter().flat_map(|e| e.1 .1.iter().copied()));
			self.ram_list = Arc::new(l);
		}
		self.ram.insert(key, RamEntry { sig, class: c, wait, words, stamp, pending: None });
	}
}

/// [b49 U2b] `c` without faces, metal packets and textures (a part that no longer exists).
fn blanked(c: &(rac_formats::mobies::MobyClass, Vec<u64>)) -> (rac_formats::mobies::MobyClass, Vec<u64>) {
	let mut m = c.0.clone();
	m.mesh.faces.clear();
	m.metal.clear();
	m.flat.clear();
	(m, Vec::new())
}

/// Calls between two decodes of a class whose TEX0 bases are still 0 or whose TEX0 words could not be read.
const RECHECK: u32 = 25;

/// What `Resolver::ram_unit` does with a known unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RamStep {
	/// Nothing changed: return at once.
	Skip,
	/// Nothing changed, the countdown runs: count down and return.
	Wait,
	/// Decode again.
	Decode,
}

/// The decision of `ram_unit` for a unit decoded before: `same_sig` = its header words are the same, `words_same` = its TEX0
/// words in RAM are the same as at the decode, `wait` = its countdown (0 = settled; nonzero while a TEX0 base is 0 or the words
/// were unreadable).
fn ram_step(same_sig: bool, words_same: bool, wait: u32) -> RamStep {
	match (same_sig && words_same, wait) {
		(true, 0) => RamStep::Skip,
		(true, w) if w > 1 => RamStep::Wait,
		_ => RamStep::Decode,
	}
}

/// A unit decoded again keeps its built class (no GPU rebuild) when its TEX0 list is the same.
fn keep_built(old: &[u64], new: &[u64]) -> bool {
	old == new
}

/// One RAM class decode (`Resolver::ram_unit`).
struct RamEntry {
	/// Header words +4 and +0x24 at decode time (and the part range): another class in a reused run-time buffer.
	sig: u64,
	/// The class, None when it did not decode.
	class: Option<RamClass>,
	/// Calls left before decoding again: nonzero while a TEX0 still has texture base 0.
	wait: u32,
	/// The TEX0 words in its packets at decode time (`rc2::mobies::tex0_words`: RAM address, word).
	words: Vec<(u32, u64)>,
	/// [b49 U2b] `class_stamp::stamp` at decode time.
	stamp: u64,
	/// [b50 V2] A different stamp seen but not yet stable: (stamp, field first seen) (`class_stamp::debounce`).
	pending: Option<(u64, u64)>,
}

/// Do the TEX0 words of a decoded class still hold what they held at its decode?
fn words_unchanged(ram: &[u8], words: &[(u32, u64)]) -> bool {
	words.iter().all(|&(a, w)| ram.get(a as usize..a as usize + 8).is_some_and(|b| u64::from_le_bytes(b.try_into().unwrap_or_default()) == w))
}

/// Log class TEX0 changes (RAC2_TEXCHECK or RAC2_LOG)?
fn log_tex0() -> bool {
	static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
	*ON.get_or_init(|| std::env::var_os("RAC2_TEXCHECK").is_some_and(|v| v != "0") || std::env::var_os("RAC2_LOG").is_some())
}

impl Scene {
	pub fn read(m: &ee_oracle::Machine, res: &mut Resolver) -> Scene {
		let level = m.rd32(ee_oracle::rc2::LEVEL) as i32;
		let none = Scene { level, ..Scene::default() };
		let Some(addrs) = res.get(m, level) else { return none };
		let Some(view) = View::read(m, &addrs) else { return none };
		// The level's GS Z offset for the layers' and the overlay's depth (engine `layer_tris::set_far_word`).
		rac_engine::gsfull::layer_tris::set_far_word(m.rdf(addrs.proj - addrs::PROJ_FAR_BELOW));
		let base = m.rd32(addrs.core) & 0x01ff_ffff;
		res.base = base;
		// [b54] The level file's texture slots, once the world is loaded: the units decoded before it are decoded again.
		if res.file_tex.is_none() && file_tex_on() {
			if let Some(l) = level_tex(level) {
				eprintln!("rac2: level {level}: {} classes draw with their level-file textures when read from RAM", l.remap.len());
				res.ram.clear();
				res.file_tex = Some(l);
			}
		}
		let chunk = crate::chunks::ram_sig(&m.cpu.mem.ram, base);
		// Moby list words: first record, end of the level's own mobies, end of the records the game spawns at run time (weapons,
		// effects, Ratchet's attachments) [C: Aranos 0x1964480 / 0x1981880 / 0x199a680, level 1 alike].
		let (mut a, end) = (m.rd32(addrs.moby) & 0x01ff_ffff, m.rd32(addrs.moby + 8) & 0x01ff_ffff);
		let own_end = m.rd32(addrs.moby + 4) & 0x01ff_ffff;
		let mut mobies = Vec::new();
		let mut spawned = Vec::new();
		let mut movers = Vec::new();
		let hero_rec = a;
		let hero = (a < end).then(|| m.rdv3(a + 0x10));
		let known = known(level);
		// [PC] The mobies the game left out, re-tested only while the render thread wants them (moby_cull.rs `WANT`).
		let want = crate::moby_cull::want();
		let dv = if want != 0 { crate::moby_cull::DrawView::read(m, &addrs) } else { None };
		let pick = Pick { dv: dv.as_ref(), want };
		// The shadow pass: the caster list (records - 0x180, {moby, fade, straddle} entries) and the records [C].
		let sh = addrs.shadow.filter(|s| m.rd32(s.gate) != 0);
		let casters: Vec<u32> = sh.map(|s| (0..48).map(|i| m.rd32(s.records - 0x180 + 8 * i) & 0x01ff_ffff).take_while(|&a| a != 0).collect()).unwrap_or_default();
		let (shadow_owners, shadows): (Vec<u32>, Vec<_>) = sh.map(|s| crate::shadows::read_indexed(m, s.records)).unwrap_or_default().into_iter().map(|(i, rc)| (casters.get(i).copied().unwrap_or(0), rc)).unzip();
		if base != 0 && a < end && end - a < 0x40_0000 {
			while a < end {
				if let Some(x) = moby_at(m, res, a, base, a >= own_end || known.as_ref().is_some_and(|k| !k.contains(&m.rd32(a + 0x24).wrapping_sub(base))), &pick) {
					let drawn = x.cut.is_none();
					if a >= own_end && drawn {
						spawned.push(x.pos);
					}
					if drawn && a != hero_rec {
						movers.push((a, x.pos, m.rdf(a + 0xc) / 1024.0));
					}
					mobies.push(Moby { late: drawn && casters.contains(&a), ..x });
				}
				a += addrs::MOBY_SIZE;
			}
		}
		Scene { view: Some(view), level, chunk, mobies, ram_classes: res.ram_list.clone(), chrome: res.chrome, shadows, shadow_owners, field: 0, hero_group: hero_group(hero, &spawned), under: crate::under::flag(&m.cpu.mem.ram, level), fog_live: crate::under::fog_lagged(&m.cpu.mem.ram, level), movers, ship_fight: m.rd32(0x18c134) == 49 && m.rd32(0x18c13c) == 9, paused: false }
	}
}

/// Debug: RAC2_HIDE=0xCLASS,.. leaves the mobies of these class ids (+0xaa) out of the native picture (bisecting a bad draw).
fn hide_class(cid: u32) -> bool {
	static HIDE: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
	HIDE.get_or_init(|| std::env::var("RAC2_HIDE").unwrap_or_default().split(',').filter_map(|x| u32::from_str_radix(x.trim().trim_start_matches("0x"), 16).ok()).collect()).contains(&cid)
}

/// [b53] Class ids (+0xaa) whose every prim retail draws with vertex RGBA (0, 0, 0, 0): GS modulate gives black at alpha 0, the
/// alpha test (GEQUAL 0x60, AFAIL RGB only) fails and the blend 0x8044 keeps the frame, so nothing shows. Native lights the class
/// like any moby (ambient 0x404040) and draws an opaque textured box. 0xbfe = the (un-upgraded) Lancer's bolt: two moving records
/// with explicit-matrix rows (+0x34 0x0100), 24 faces, TEX0 8005f845dd30b320 [C: fg1 0011 Circle 5-100 field 25, RAC2_MOBYLOG key
/// 0xbbbe00; RAC2_PICTURE=gs RAC_GS_PRIMLOG=8005f845dd30b320: 48 prims per field 23..27, one vertex colour set [0,0,0,0], at 0x7c
/// alpha 0x8044 test 0x5360b; the GS picture shows no box, native the red/orange box on the beam]. Same retail zero as the beam
/// helper 0xc89 (`zero_basis`, runs/b33/lancer_rgba.md), whose rows are zero; 0xbfe's rows are not, so it is named here.
fn retail_unseen(cid: u32) -> bool {
	matches!(cid, 0xbfe)
}

/// Debug: RAC2_RAMCLASS=0xCLASS,.. reads those classes from RAM even where the level file has them (texture slot checks).
fn ram_forced(cid: u32) -> bool {
	static L: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
	L.get_or_init(|| std::env::var("RAC2_RAMCLASS").unwrap_or_default().split(',').filter_map(|x| u32::from_str_radix(x.trim().trim_start_matches("0x"), 16).ok()).collect()).contains(&cid)
}

/// A glow centred within this many metres of Ratchet's root (his body) is behind his draw order in retail.
const HERO_RADIUS: f32 = 1.1;
/// ... or within this many of a moby the game spawned at run time right next to him (the weapon in his hand).
const HELD_RADIUS: f32 = 0.7;
/// A spawned moby counts as held when it is this close to Ratchet's root (m).
const HELD_REACH: f32 = 1.6;

/// The spheres of Ratchet's body and of the run-time mobies close to him (his held weapon): `hero` = the first moby record's position.
fn hero_group(hero: Option<[f32; 3]>, spawned: &[[f32; 3]]) -> Vec<[f32; 4]> {
	let Some(h) = hero.filter(|h| h.iter().all(|x| x.is_finite())) else { return Vec::new() };
	let near = |p: &[f32; 3]| (0..3).map(|k| (p[k] - h[k]).powi(2)).sum::<f32>() <= HELD_REACH * HELD_REACH;
	let mut out = vec![[h[0], h[1], h[2], HERO_RADIUS]];
	out.extend(spawned.iter().filter(|p| near(p)).take(8).map(|p| [p[0], p[1], p[2], HELD_RADIUS]));
	out
}

/// [PC] Which of the mobies the game left out `moby_at` keeps: the field's draw view and `moby_cull::WANT`.
struct Pick<'a> {
	dv: Option<&'a crate::moby_cull::DrawView>,
	want: u8,
}

impl Pick<'_> {
	/// The cut of a moby the game did not draw, when it is wanted: every one with Culling Off, the side-only ones for the side restore.
	fn cut(&self, m: &ee_oracle::Machine, a: u32, slot: u8) -> Option<crate::moby_cull::Cut> {
		use crate::moby_cull::{FAR, OFF, SIDE};
		// [I] Slots 0x80.. are not live level mobies (the handoff's research: only slots under 0x80 were compared with the game's verdict).
		let c = self.dv.filter(|_| slot < 0x80)?.cut(m, a)?;
		(self.want & OFF != 0 || (self.want & SIDE != 0 && c.side_only) || (self.want & FAR != 0 && c.far_ok)).then_some(c)
	}
}

/// The moby record at `a` when the game drew it last field, or a moby it left out that `pick` keeps (`Moby::cut`) (`from_ram`: its
/// class is read from RAM, not the level file).
fn moby_at(m: &ee_oracle::Machine, res: &mut Resolver, a: u32, base: u32, from_ram: bool, pick: &Pick) -> Option<Moby> {
	let slot = m.rd8(a + 0x20);
	let hdr = m.rd32(a + 0x24) & 0x01ff_ffff;
	if hdr == 0 || slot >= 0xfd {
		return None;
	}
	// +0x31: the game drew the moby last frame (R&C1's `drawn`) [C: Aranos field 2640: 17 of 462 live mobies, the hidden helpers
	// that ride on Ratchet (classes 0x259, 0x25f) at 0].
	let cut = if m.rd8(a + 0x31) == 0 { Some(pick.cut(m, a, slot)?) } else { None };
	let key = hdr.wrapping_sub(base);
	if hide_class(m.rd32(a + 0xa8) >> 16) || retail_unseen(m.rd32(a + 0xa8) >> 16) {
		return None;
	}
	let from_ram = from_ram || ram_forced(m.rd32(a + 0xa8) >> 16);
	// Records spawned at run time (past the level's own) always come from RAM: Ratchet's helmet (0x50a) and boot pieces (0xa73)
	// have level-file entries whose texture slots name the wrong level textures; the game drew them with TEX0 words it wrote into
	// their packets [C: Aranos field 2500].
	if from_ram {
		res.ram_class(m, hdr, key);
	}
	// Part mask (+0x7c): a piece or an intact moby with limbs on is drawn from the class's part packets, which the base class
	// does not hold; their classes are decoded from RAM whether the base class comes from the level file or from RAM.
	let mask = rac_formats::rc2::mobies::part_mask(&m.cpu.mem.ram[..32 << 20], hdr as usize, m.rd32(a + 0x7c) as u16);
	if mask != 0 {
		res.ram_parts(m, hdr, key, mask);
	}
	// Class id 0 (+0xaa) = Ratchet: his helmet's metal packet carries the environment map's TEX0, read again whenever his class
	// header moves (not once per level: a stale word would name other VRAM).
	if m.rd32(a + 0xa8) >> 16 == 0 && (res.chrome.is_none() || res.chrome_hdr != hdr) {
		let t = metal_tex0(m, hdr);
		if t.is_some() {
			res.chrome_hdr = hdr;
		}
		if let Some(t) = t.filter(|&t| res.chrome != Some(t)) {
			eprintln!("rac2: metal environment map TEX0 {t:#x}");
			res.chrome = Some(t);
			crate::gsfull::texreq::want(t);
		}
	}
	let pos = m.rdv3(a + 0x10);
	let rows = [m.rdv3(a + 0xc0), m.rdv3(a + 0xd0), m.rdv3(a + 0xe0)];
	let scale = m.rdf(a + 0x2c);
	if !pos.iter().chain(rows.iter().flatten()).all(|x| x.is_finite()) || !scale.is_finite() || zero_basis(&rows) {
		return None;
	}
	let pose = crate::ram_anim::pose_from_ram(&m.cpu.mem.ram, a).ok().filter(|p| !p.is_empty() && p.iter().flatten().all(|x| x.is_finite()));
	if std::env::var_os("RAC2_MOBYLOG").is_some() {
		eprintln!("rac2: moby {a:#x} class {:#x} key {key:#x} hdr {hdr:#x} at {pos:?} joints {}", m.rd32(a + 0xa8) >> 16, pose.as_ref().map_or(0, |p| p.len()));
	}
	let amb = m.rd32(a + 0x3c);
	Some(Moby { key, addr: a, pos, rows, scale, mask, pose: pose.map(Arc::new), light: ([amb as u8, (amb >> 8) as u8, (amb >> 16) as u8], m.rd32(a + 0x38)), tint: (m.rd32(a + 0x34) & 0x10 != 0).then(|| m.rd32(a + 0x78)), late: false, cut })
}

/// The first GS TEX0 in the AD data of a RAM class's metal packets (header +6 count, +7 first index; the game writes the
/// environment map's TEX0 there at load) [C: Aranos hero 0x10bc4c0 metal packet 0: 0x80058305ddb08b00, 128x128 T8H at TBP 0xb00].
fn metal_tex0(m: &ee_oracle::Machine, hdr: u32) -> Option<u64> {
	let table = m.rd32(hdr) & 0x01ff_ffff;
	let (count, first) = (m.rd8(hdr + 6) as u32, m.rd8(hdr + 7) as u32);
	for k in 0..count {
		let e = table + (first + k) * 16;
		let (list, qw) = (m.rd32(e) & 0x01ff_ffff, m.rd32(e + 4) & 0xffff);
		let (mut a, end) = (list, list + qw * 16);
		while a < end && a < 0x0200_0000 {
			let c = m.rd32(a);
			let cmd = (c >> 24) & 0x7f;
			if cmd & 0x60 != 0x60 {
				a += 4;
				continue;
			}
			let (vn, vl) = ((cmd >> 2) & 3, cmd & 3);
			let n = match (c >> 16) & 0xff {
				0 => 256,
				n => n,
			};
			let bytes = ([4, 2, 1, 0][vl as usize] * (vn + 1) * n + 3) & !3;
			if cmd & 0xf == 0xc {
				for q in (0..bytes).step_by(16) {
					let at = a + 4 + q;
					if m.rd32(at + 8) & 0xff == 6 {
						return Some(m.rd32(at) as u64 | (m.rd32(at + 4) as u64) << 32);
					}
				}
			}
			a += 4 + bytes;
		}
	}
	None
}

/// A moby whose matrix rows (+0xc0, +0xd0, +0xe0) are all zero: retail's moby program puts every vertex on the moby origin (zero-area
/// prims, the GS draws nothing) [C runs/b32/g2.err raw 1830-1850, runs/b33/lancer_rgba.md: the Lancer's class 0xc89 beam helper,
/// the only class with zero rows in the logged records]; native would draw it unrotated at full size (props.rs `placed` keeps zero
/// rows, the dark box on the Lancer beam). Such a moby is not drawn, whatever its class.
fn zero_basis(rows: &[[f32; 3]; 3]) -> bool {
	rows.iter().all(|r| r.iter().map(|x| x * x).sum::<f32>() < 1e-12)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn zero_basis_only_for_all_zero_rows() {
		assert!(zero_basis(&[[0.0; 3]; 3]));
		assert!(zero_basis(&[[0.0, -0.0, 0.0], [0.0; 3], [1e-9, 0.0, 0.0]]));
		assert!(!zero_basis(&[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]));
		// One live row (a flattened moby) still draws.
		assert!(!zero_basis(&[[0.0; 3], [0.0; 3], [0.0, 0.0, 0.5]]));
	}

	#[test]
	fn the_lancer_bolt_retail_draws_black_is_not_drawn() {
		assert!(retail_unseen(0xbfe));
		// Ratchet, the beam helper (hidden by `zero_basis` instead) and the Heavy Lancer shot (props.rs `level0_screen`) are not named.
		assert!(!retail_unseen(0) && !retail_unseen(0xc89) && !retail_unseen(0xa26));
	}

	/// The decision table of `ram_unit` for a unit decoded before: header words same / TEX0 words same / countdown.
	#[test]
	fn ram_step_table() {
		use RamStep::*;
		// (same_sig, words_same, wait) -> step
		let table = [
			(true, true, 0, Skip),
			(true, true, 1, Decode),
			(true, true, 2, Wait),
			(true, true, RECHECK, Wait),
			(true, false, 0, Decode),
			(true, false, 1, Decode),
			(true, false, RECHECK, Decode),
			(false, true, 0, Decode),
			(false, true, RECHECK, Decode),
			(false, false, 0, Decode),
		];
		for (sig, words, wait, want) in table {
			assert_eq!(ram_step(sig, words, wait), want, "same_sig {sig} words_same {words} wait {wait}");
		}
	}

	/// A countdown runs down to a decode, then restarts: an unreadable word list is decoded again every RECHECK calls, never
	/// "unchanged" for ever.
	#[test]
	fn an_unreadable_word_list_is_decoded_again_on_the_countdown() {
		let mut wait = RECHECK;
		let mut decodes = 0;
		for _ in 0..3 * RECHECK {
			match ram_step(true, true, wait) {
				RamStep::Wait => wait -= 1,
				RamStep::Decode => {
					decodes += 1;
					wait = RECHECK;
				}
				RamStep::Skip => panic!("a unit with a countdown never skips"),
			}
		}
		assert!(decodes >= 2, "decoded {decodes} times");
	}

	#[test]
	fn a_decode_with_the_same_tex0s_keeps_the_built_class() {
		assert!(keep_built(&[1, 2], &[1, 2]));
		assert!(!keep_built(&[1, 2], &[1, 3]));
		assert!(!keep_built(&[1], &[1, 2]));
	}

	#[test]
	fn words_unchanged_sees_a_rewritten_word_and_a_cut_off_address() {
		let mut ram = vec![0u8; 64];
		ram[16..24].copy_from_slice(&0x1234u64.to_le_bytes());
		assert!(words_unchanged(&ram, &[(16, 0x1234)]));
		assert!(words_unchanged(&ram, &[]));
		ram[16] = 0x35;
		assert!(!words_unchanged(&ram, &[(16, 0x1234)]));
		assert!(!words_unchanged(&ram, &[(60, 0)]), "an address past the end is not unchanged");
	}
}
