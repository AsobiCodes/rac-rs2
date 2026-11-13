//! R&C2 world layers the engine draws natively (round 3 batch 4, issues 3 / 6 / 7 / 9 / 14 / 15). The EE sends some depth-tested
//! 3D triangles itself (`zlayer::is_world_layer`: the ship's canopy glass, lamp / nanotech / bolt glows). They used to be drawn by the
//! software GS into the 512 x 448 overlay at the newest field (blocky edges, a lag behind the smoothed world, gone in the pause
//! menu). With `enable(true)` the GS captures them here instead (`gs_layers.rs`), as screen triangles with their texels; the pack
//! unprojects them with the newest view and hands them to the engine's particle draw (`viewer/gpu/layer_fx.rs`), which draws them
//! at the window's resolution, at the smoothed camera, with real depth. Game-neutral: only the R&C2 pack switches it on.
use crate::level::Texture;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

static ON: AtomicBool = AtomicBool::new(false);

/// Capture world layers instead of drawing them into the overlay (default off: R&C1 never calls this).
pub fn enable(on: bool) {
	ON.store(on, Ordering::Relaxed);
}

pub fn on() -> bool {
	ON.load(Ordering::Relaxed)
}

static FX_ON: AtomicBool = AtomicBool::new(false);

/// Also draw the effect program (VU1 program 2) natively from its input records (`fx_native.rs`); needs `enable(true)` too. Default off.
pub fn enable_fx(on: bool) {
	FX_ON.store(on, Ordering::Relaxed);
}

pub fn fx_on() -> bool {
	FX_ON.load(Ordering::Relaxed) && on()
}

static CAMS: std::sync::Mutex<std::collections::VecDeque<(u64, bool)>> = std::sync::Mutex::new(std::collections::VecDeque::new());

/// The pack tells whether the pictures of fields from `field` on will have a 3D camera to place their effects with (the game thread
/// publishes it ahead of the GS). Without one the effects are no use to the native draw, so the GS keeps them in the overlay.
pub fn note_camera(field: u64, has: bool) {
	if let Ok(mut c) = CAMS.lock() {
		if c.len() >= 32 {
			c.pop_front();
		}
		c.push_back((field, has));
	}
}

/// Does the picture of `field` have a camera (the newest note at or before it)? True when nothing is known.
pub fn has_camera(field: u64) -> bool {
	CAMS.lock().ok().and_then(|c| c.iter().rev().find(|e| e.0 <= field).map(|e| e.1)).unwrap_or(true)
}

/// Most triangles and distinct textures one field may hand out; the rest is drawn into the overlay as before.
pub const MAX_TRIS: usize = 4096;
pub const MAX_TEX: usize = 64;

/// One vertex: GS screen position in pixels of the overlay buffer (the game's projection centre is its `half`), the GS depth word,
/// the vertex colour (128 = 1.0) and the texture coordinate in 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerVert {
	pub px: f32,
	pub py: f32,
	pub z: u32,
	pub rgba: [u8; 4],
	pub uv: [f32; 2],
	/// The GS fog factor F (255 = no fog; also 255 when the prim has fog off): the GS blends the textured colour towards FOGCOL by
	/// 1 - F / 255. The pack scales the colour by F / 255 (the FOGCOL term is under 3 / 255 in R&C2's levels [C: Oozla F >= 238, FOGCOL 1e280a]).
	pub f: u8,
}

/// One captured triangle. `id`: hash of the texels (an animated texture moves its VRAM base every field; the renderer keys on the
/// texels, so a texture is uploaded once and the store does not grow).
#[derive(Clone)]
pub struct LayerTri {
	pub v: [LayerVert; 3],
	pub tex: Arc<Texture>,
	pub id: u64,
	/// ALPHA is the additive blend (Cs * As + Cd), else source-alpha over (Cs * As + Cd * (1 - As)).
	pub additive: bool,
	/// TEX0 TFX decal: the texel is the colour, the vertex colour is not applied.
	pub decal: bool,
	/// The corners in the world (effect program records, `fx_native.rs`): when set the pack places the triangle by these and not by
	/// the screen positions / depth words of `v`.
	pub world: Option<[crate::gsfull::fx_native::WVert; 3]>,
	/// Depth-tested against the world. False for an effect prim whose GS TEST has no depth test (ZTE off or ZTST ALWAYS): the pack
	/// draws it over the world. World layers (`world` None) are always tested.
	pub depth: bool,
	/// The prim had no texture (rain / spark lines): its colour is the vertex colour as it is (rgb / 255), not a texture modulation (/ 128).
	pub untextured: bool,
	/// ALPHA (Cs - 0) * As >> 7 + 0 (0x88, `additive` false): the colour scaled by its alpha replaces what is there, the destination is
	/// not read (R&C2 level 14's lava sheet [C: RAC_GSPIX 225,252 q60903]). Drawn in the level pass before the other layers.
	pub replace: bool,
	/// With `replace`: the prim writes depth (ZBUF ZMSK off), as the GS does, so what is drawn after it behind the sheet stays hidden.
	pub zwrite: bool,
	/// With `replace`: the prim's GS alpha test (TEST ATE / ATST / AREF / AFAIL), applied by the native draw; ALWAYS otherwise.
	pub alpha: AlphaTest,
}

/// A GS alpha test (TEST bits 0..13): pixels whose alpha As fails `As ATST AREF` write only what AFAIL lets through (KEEP 0:
/// nothing, FB_ONLY 1: the colour, ZB_ONLY 2: the depth, RGB_ONLY 3: the colour without its alpha); passing pixels write both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AlphaTest {
	/// ATST: 0 NEVER, 1 ALWAYS, 2 LESS, 3 LEQUAL, 4 EQUAL, 5 GEQUAL, 6 GREATER, 7 NOTEQUAL.
	pub atst: u8,
	pub aref: u8,
	pub afail: u8,
}

impl Default for AlphaTest {
	fn default() -> Self {
		Self::ALWAYS
	}
}

impl AlphaTest {
	/// Every pixel passes (ATE off).
	pub const ALWAYS: AlphaTest = AlphaTest { atst: 1, aref: 0, afail: 0 };

	/// The alpha test of a TEST register: ALWAYS when ATE is off.
	pub fn of(test: u64) -> Self {
		if test & 1 == 0 {
			return Self::ALWAYS;
		}
		Self { atst: ((test >> 1) & 7) as u8, aref: ((test >> 4) & 0xff) as u8, afail: ((test >> 12) & 3) as u8 }
	}

	/// Does a pixel of alpha `a` (the GS's As: texel * vertex alpha >> 7) pass?
	pub fn passes(self, a: u8) -> bool {
		match self.atst {
			0 => false,
			1 => true,
			2 => a < self.aref,
			3 => a <= self.aref,
			4 => a == self.aref,
			5 => a >= self.aref,
			6 => a > self.aref,
			_ => a != self.aref,
		}
	}

	/// The draws that reproduce it for a prim that writes depth (`zwrite`) or not: (writes colour, writes depth) of the pixels that pass,
	/// then of those that fail (None: a failing pixel writes nothing, or exactly what a passing one writes: one draw, no test).
	pub fn sides(self, zwrite: bool) -> ((bool, bool), Option<(bool, bool)>) {
		let pass = (true, zwrite);
		let fail = (matches!(self.afail, 1 | 3), zwrite && self.afail == 2);
		(pass, (self.atst != 1 && fail != pass && fail != (false, false)).then_some(fail))
	}

	/// The test the pixels of the first draw (`sides`.0) take: ALWAYS when a failing pixel writes what a passing one does.
	pub fn first_draw(self, zwrite: bool) -> AlphaTest {
		let fail = (matches!(self.afail, 1 | 3), zwrite && self.afail == 2);
		if fail == (true, zwrite) { Self::ALWAYS } else { self }
	}

	/// The word the shader reads (viewer/layer_replace.wgsl `layer_alpha`): ATST | AREF << 3.
	pub fn word(self) -> u32 {
		self.atst as u32 | (self.aref as u32) << 3
	}
}

/// The running level's GS Z offset C (`depth_with`; f32 bits), 480 until the pack names a far word (R&C1 never does).
static Z_OFF: AtomicU32 = AtomicU32::new(0x43f0_0000);

/// The GS Z offset C of a level whose projection block holds the far word `far` (the far plane in 1/1024 m): the VU1 programs' Z is
/// 32 at the far plane on every level, so C = 2^24 / (32 far / 1024) - 32 = 2^29 / far - 32: 480 for 1048576 (1024 m, most levels),
/// 142.76 for 3072000 (Aranos, level 14), 96 for 4194304 [C: per state, the projection matrix's z column (proj + 0x68, + 0x78) puts the
/// far plane at the same Z (-8388079.5 + the programs' constant) on all of fg1 0000..0576 and q60903; level 14 measured Z = 2^24 Q -
/// 147 / - 146 (RAC_GSPIX q60903: lava z 60616 q 0.003621738, ties z 52087 q 0.0031133543); the matrix gives -143.0 +- 1].
pub fn z_offset_of(far: f32) -> f32 {
	536_870_912.0 / far - 32.0
}

/// The pack names the running level's far word (`z_offset_of`); a word out of 65536..2^27 (no level loaded) keeps the last offset.
pub fn set_far_word(far: f32) {
	if (65536.0..134_217_728.0).contains(&far) {
		Z_OFF.store(z_offset_of(far).to_bits(), Ordering::Relaxed);
	}
}

/// The running level's GS Z offset C (`z_offset_of`).
pub fn z_offset() -> f32 {
	f32::from_bits(Z_OFF.load(Ordering::Relaxed))
}

/// View depth in metres of a GS depth word with the Z offset `c`: Z = 2^24 Q - C with Q = 1 / (32 depth) for every R&C2 VU1 program
/// [C: gs_overlay.rs; C per level: `z_offset_of`].
pub fn depth_with(z: u32, c: f32) -> f32 {
	524288.0 / (z as f32 + c)
}

/// View depth in metres of a GS depth word of the running level.
pub fn depth_m(z: u32) -> f32 {
	depth_with(z, z_offset())
}

/// FNV-1a over the texels and the size.
pub fn texture_id(t: &Texture) -> u64 {
	let mut h = 0xcbf2_9ce4_8422_2325u64 ^ ((t.w as u64) << 20) ^ t.h as u64;
	for p in &t.rgba {
		h = (h ^ u32::from_le_bytes(*p) as u64).wrapping_mul(0x100_0000_01b3);
	}
	h
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn texels_decide_the_id_not_the_vram_base() {
		let a = Texture { w: 2, h: 1, rgba: vec![[1, 2, 3, 255], [4, 5, 6, 255]] };
		let b = Texture { w: 2, h: 1, rgba: vec![[1, 2, 3, 255], [4, 5, 6, 255]] };
		let c = Texture { w: 2, h: 1, rgba: vec![[1, 2, 3, 255], [4, 5, 7, 255]] };
		assert_eq!(texture_id(&a), texture_id(&b));
		assert_ne!(texture_id(&a), texture_id(&c));
	}

	#[test]
	fn depth_matches_the_overlay_depth_plane() {
		// Q = 1 / (32 * 10 m): Z = 2^24 / 320 - 480.
		let z = (16777216.0f32 / 320.0 - 480.0) as u32;
		assert!((depth_m(z) - 10.0).abs() < 0.01);
	}

	#[test]
	fn the_z_offset_follows_the_level_s_far_word() {
		assert_eq!(f32::from_bits(0x43f0_0000), 480.0, "the default is the 1024 m levels' offset");
		assert_eq!(z_offset_of(1_048_576.0), 480.0, "1024 m: exactly the old constant");
		assert_eq!(z_offset_of(4_194_304.0), 96.0);
		let c = z_offset_of(3_072_000.0);
		assert!((c - 142.76).abs() < 0.01, "{c}");
		// Level 14 (q60903, RAC_GSPIX): the GS Z and Q of the lava and of a tie; depth = 1 / (32 Q).
		for (z, q) in [(60616u32, 0.003621738f32), (52087, 0.0031133543)] {
			let (got, want) = (depth_with(z, c), 1.0 / (32.0 * q));
			assert!((got / want - 1.0).abs() < 1e-4, "z {z}: {got} m, Q says {want} m");
			assert!(depth_with(z, 480.0) < want * 0.995, "the 1024 m constant puts it > 0.5 % too near");
		}
		// The far plane is Z 32 on every level.
		for far in [1_048_576.0f32, 3_072_000.0, 4_194_304.0] {
			assert!((depth_with(32, z_offset_of(far)) - far / 1024.0).abs() < 0.05, "{far}");
		}
	}
}
