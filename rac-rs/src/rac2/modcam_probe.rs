//! Modern camera wall probe for R&C2 (scratch_rc2_cam, stage B2): the retail segment test (Aranos 0x2dec98, one twin per level) run
//! from the camera hand with everything it writes put back, so the game cannot tell the probe ran.
//!
//! * Finder (`find`, `SegCache`): the function is located per level by its code words (a loose mask drops the relocated parts: jump
//!   targets and 16-bit immediates; 8 anchor words, then an FNV-1a hash over 1031 loose words), because the level overlay moves it
//!   (entries 0x2d41b0..0x31e400 [C, 26 of 26 levels]). The result block `res` and the event ring come from the `lui`/`addiu` pairs
//!   inside it. Spec and proof: handoff_stageB.md section 3.
//! * `Wall`: the restore set, saved before and written back after the call (all of it one snapshot taken at the same moment, so
//!   overlaps cannot disagree): the result block + slot buffers + event ring (one span, res .. ring + 0x1000 = res + 0x5280), the
//!   dedup key table at res[8] (0x40), every moby's query stamp (+0xa0) and touch byte (+0xa8), the 16 KB scratchpad, VU0 data memory,
//!   a stack window below the caller's sp, the SPR DMA channel registers + D_STAT, and in `Regs` all registers (+ pc / npc).
//! * `RetailProbe`: `Probe::seg(a, b)` for `modcam_pose::Cam::step`: arguments on the stack, `Cpu::call(entry)`, hit = v0 & 1, the
//!   contact point = 3 floats at res + 0x20, then everything restored.
//! * Diag `RAC2_CAM_DIAG=1` (ENV.md): for the first `DIAG_PROBES` probes the whole 32 MB RAM is copied before the call; after the
//!   restore RAM, scratchpad, DMA window, VU0 data memory and registers are compared with the copy and the residual is logged (it must be 0).
//!
//! World units are metres (the hero at Aranos 247.0, 190.7, 49.9; the contact point of a wall 253.9, 198.3, 49.9 [C]); the eye of the
//! RENDER basis (addrs.rs) is x1024, the game's segment test is not. `RetailProbe::unit` scales if a check ever shows otherwise.

use crate::modcam_pose::{Probe, V3};
use ee_oracle::cpu::Cpu;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::OnceLock;

// ---------------------------------------------------------------------------------------------------------------- finder

/// What identifies the function in RAM: where to look, the first 8 loose words, the count and hash of the loose words, and where
/// the two address-building `lui`/`addiu` pairs sit (word index of the `lui`).
pub struct Spec {
	pub lo: u32,
	pub hi: u32,
	pub anchor: [u32; 8],
	pub words: usize,
	pub hash: u64,
	pub res_at: usize,
	pub ring_at: usize,
}

/// The segment test [C: 26 of 26 level dumps, exactly one match each]. `res_at` = +0xc, `ring_at` = +0xf9c.
pub const SEG: Spec = Spec {
	lo: 0x280000,
	hi: 0x360000,
	anchor: [0x7220_0000, 0xd8bd_0000, 0x4a20_0000, 0x3c0f_0000, 0x25ef_0000, 0x4a20_0000, 0x3c01_0000, 0],
	words: 1031,
	hash: 0x56e4_fe9d_6c2e_2f1f,
	res_at: 3,
	ring_at: 0xf9c / 4,
};

/// Bytes of the event ring: 64 entries of 0x40, at res + 0x4280 [C: 7 levels].
pub const RING_BYTES: usize = 0x1000;

/// The relocation-blind form of a code word: jumps keep the opcode, branches (PC-relative) stay whole, everything else with a 16-bit
/// immediate keeps opcode and registers (addresses, offsets and constants move per level).
pub fn loose(w: u32) -> u32 {
	match w >> 26 {
		2 | 3 => w & 0xfc00_0000,
		1 | 4 | 5 | 6 | 7 | 0x14 | 0x15 | 0x16 | 0x17 => w,
		op if op >= 8 => w & 0xffff_0000,
		_ => w,
	}
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x100_0000_01b3;

/// FNV-1a 64 over the 32-bit loose words (`h = (h ^ w) * P`).
pub fn hash_loose(words: impl Iterator<Item = u32>) -> u64 {
	words.fold(FNV_OFFSET, |h, w| (h ^ loose(w) as u64).wrapping_mul(FNV_PRIME))
}

/// The segment test as found in one level's RAM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegFn {
	pub entry: u32,
	/// The result block.
	pub res: u32,
	/// The event ring (inside the span `res .. res + span()`).
	pub ring: u32,
	/// How many places of the scan range matched (1 expected; the first is used).
	pub matches: u32,
}

impl SegFn {
	/// Bytes from `res` to the end of the event ring: the one block that is saved and restored.
	pub fn span(&self) -> usize {
		(self.ring - self.res) as usize + RING_BYTES
	}
}

fn rd32(b: &[u8], a: usize) -> Option<u32> {
	b.get(a..a.checked_add(4)?).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn rd32s(b: &[u8], a: u32) -> Option<u32> {
	rd32(b, (a & 0x01ff_ffff) as usize)
}

/// `lui r, hi` followed by `addiu r, r, lo` (ops 0xf / 9): the address they build.
pub fn lui_addiu(hi: u32, lo: u32) -> Option<u32> {
	(hi >> 26 == 0xf && lo >> 26 == 9).then(|| ((hi & 0xffff) << 16).wrapping_add(lo as u16 as i16 as i32 as u32))
}

/// The first 8 loose words at `a` equal the anchor.
fn head_ok(ram: &[u8], a: usize, s: &Spec) -> bool {
	s.anchor.iter().enumerate().all(|(i, w)| rd32(ram, a + i * 4).is_some_and(|x| loose(x) == *w))
}

fn match_at(ram: &[u8], a: usize, s: &Spec) -> Option<SegFn> {
	if !head_ok(ram, a, s) {
		return None;
	}
	let body = ram.get(a..a + s.words * 4)?;
	if hash_loose(body.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))) != s.hash {
		return None;
	}
	let w = |i: usize| rd32(ram, a + i * 4);
	let res = lui_addiu(w(s.res_at)?, w(s.res_at + 1)?)?;
	let ring = lui_addiu(w(s.ring_at)?, w(s.ring_at + 1)?)?;
	let ok = ring >= res && ring - res <= 0x1_0000 && (res as usize) < ram.len() && (ring as usize) + RING_BYTES <= ram.len();
	ok.then_some(SegFn { entry: a as u32, res, ring, matches: 1 })
}

/// Scans `spec.lo .. spec.hi` (step 4) for the function: first word, then the 8 anchor words, then the hash.
pub fn find_with(ram: &[u8], s: &Spec) -> Option<SegFn> {
	let last = (s.hi as usize).saturating_sub(4).min(ram.len().checked_sub(s.words * 4)?);
	let mut found: Option<SegFn> = None;
	let mut n = 0;
	let mut a = (s.lo as usize + 3) & !3;
	while a <= last {
		if rd32(ram, a).is_some_and(|x| loose(x) == s.anchor[0]) {
			if let Some(f) = match_at(ram, a, s) {
				n += 1;
				found.get_or_insert(f);
			}
		}
		a += 4;
	}
	found.map(|f| SegFn { matches: n, ..f })
}

/// `find_with` for the R&C2 segment test.
pub fn find(ram: &[u8]) -> Option<SegFn> {
	find_with(ram, &SEG)
}

/// Calls between two scans while a level has no function yet (a load in progress).
pub const RETRY_EVERY: u32 = 30;

/// The exact words behind a found function's addresses: the `lui` / `addiu` pairs of the result block and the ring. The loose head
/// check cannot see them change (an overlay with the same code shape and other data addresses at the same key), these four words can.
fn address_words(ram: &[u8], entry: u32, s: &Spec) -> Option<[u32; 4]> {
	let w = |i: usize| rd32(ram, entry as usize + i * 4);
	Some([w(s.res_at)?, w(s.res_at + 1)?, w(s.ring_at)?, w(s.ring_at + 1)?])
}

/// The finder's result per level: scans when the level key changes, drops the entry when its code changed under it (a new overlay at
/// the same key: the loose head words or the exact address words differ), and retries every `RETRY_EVERY` calls while none was found.
pub struct SegCache {
	spec: &'static Spec,
	key: Option<u32>,
	seg: Option<SegFn>,
	/// `address_words` of `seg` when it was found.
	sig: Option<[u32; 4]>,
	misses: u32,
}

impl Default for SegCache {
	fn default() -> SegCache {
		SegCache::with_spec(&SEG)
	}
}

impl SegCache {
	pub fn with_spec(spec: &'static Spec) -> SegCache {
		SegCache { spec, key: None, seg: None, sig: None, misses: 0 }
	}

	/// The function of the level `key` (any value that changes with the level: the level word, the entry of the camera class).
	pub fn get(&mut self, ram: &[u8], key: u32) -> Option<SegFn> {
		if self.key == Some(key) {
			match self.seg {
				Some(f) if head_ok(ram, f.entry as usize, self.spec) && address_words(ram, f.entry, self.spec) == self.sig => return self.seg,
				Some(_) => self.seg = None,
				None => {
					self.misses += 1;
					if self.misses % RETRY_EVERY != 0 {
						return None;
					}
				}
			}
		}
		self.key = Some(key);
		self.misses = 0;
		self.seg = find_with(ram, self.spec);
		self.sig = self.seg.and_then(|f| address_words(ram, f.entry, self.spec));
		if let Some(f) = self.seg {
			eprintln!("rac2 cam probe: segment test {:#x} result block {:#x} ring {:#x} ({} match{})", f.entry, f.res, f.ring, f.matches, if f.matches == 1 { "" } else { "es" });
		}
		self.seg
	}
}

// -------------------------------------------------------------------------------------------------------------- registers

/// Every register a nested retail call may change, and where the PC was.
#[derive(Clone, PartialEq)]
pub struct Regs {
	r: [u128; 32],
	hi: u128,
	lo: u128,
	sa: u32,
	pc: u32,
	npc: u32,
	f: [u32; 32],
	facc: u32,
	fcr31: u32,
	vf: [[u32; 4]; 32],
	vacc: [u32; 4],
	vi: [u32; 32],
}

impl Regs {
	pub fn save(c: &Cpu) -> Regs {
		Regs { r: c.r, hi: c.hi, lo: c.lo, sa: c.sa, pc: c.pc, npc: c.npc, f: c.f, facc: c.facc, fcr31: c.fcr31, vf: c.vf, vacc: c.vacc, vi: c.vi }
	}
	pub fn restore(&self, c: &mut Cpu) {
		(c.r, c.hi, c.lo, c.sa, c.pc, c.npc) = (self.r, self.hi, self.lo, self.sa, self.pc, self.npc);
		(c.f, c.facc, c.fcr31, c.vf, c.vacc, c.vi) = (self.f, self.facc, self.fcr31, self.vf, self.vacc, self.vi);
	}
}

// ------------------------------------------------------------------------------------------------------------------------ clock

/// Status.IE and Status.EIE: the oracle takes an interrupt only with both set (`Cpu::irq_taken`).
pub const IRQ_ENABLE: u32 = 0x1_0001;

/// The machine's instruction clock and event state. The probe is not part of the game's timeline (pattern of R&C1's `water_path.rs`
/// `Clock` / `Hw`): its instructions are not counted (`icount` / `mem.now` are put back), and nothing that is due by the clock may
/// fire inside it. `hold` pushes the event state out of reach (the cold-boot field timer, a pending VIF1/DMA stop that would mature
/// `dma_delay` instructions after its kick, the poll limit) and clears Status.IE / EIE, so neither a matured stop nor an INT0 / INT1
/// handler runs mid-probe, and the completion bit of a pending DMA cannot be raised inside the probe only to be rewound by the
/// `Wall` restore (the possible hang). `release` gives everything back unchanged: a stop that was pending stays pending and
/// matures at the instruction it would have without the probe; a line that was asserted is taken at the next block entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clock {
	icount: u64,
	now: u64,
	limit: u64,
	due: u64,
	tick: u64,
	dma_due: u64,
	pend: Option<[u32; 5]>,
	dirty: bool,
	status: u32,
	cause: u32,
}

impl Clock {
	/// The current state, nothing changed.
	pub fn read(c: &Cpu) -> Clock {
		let m = &c.mem;
		Clock { icount: c.icount, now: m.now, limit: m.limit, due: m.due, tick: m.tick, dma_due: m.dma_due, pend: m.vif1_pend, dirty: m.irq_dirty, status: c.cp0[12], cause: c.cp0[13] }
	}

	/// The state to give back with `release`; until then no event is due and no interrupt can be taken.
	pub fn hold(c: &mut Cpu) -> Clock {
		let held = Clock::read(c);
		let m = &mut c.mem;
		(m.tick, m.due, m.limit, m.irq_dirty) = (u64::MAX, u64::MAX, u64::MAX, false);
		c.cp0[12] &= !IRQ_ENABLE;
		held
	}

	/// Puts the held state back (the Cause register too: `try_irq` rewrites its IP bits).
	pub fn release(&self, c: &mut Cpu) {
		(c.icount, c.cp0[12], c.cp0[13]) = (self.icount, self.status, self.cause);
		let m = &mut c.mem;
		(m.now, m.limit, m.due, m.tick, m.dma_due, m.vif1_pend, m.irq_dirty) = (self.now, self.limit, self.due, self.tick, self.dma_due, self.pend, self.dirty);
	}
}

// ---------------------------------------------------------------------------------------------------------------- restore set

/// Dedup key table: 8 entries of {mesh id, query id}.
pub const KEY_BYTES: usize = 0x40;
/// The slot buffers: 8 x 0x800 from res[4] (inside the span in every level checked; saved separately if a level says otherwise).
pub const SLOT_BYTES: usize = 0x4000;
/// The stack window is `STACK_BELOW` under the argument buffer and `STACK_ABOVE` over it.
pub const STACK_BELOW: u32 = 0x1000;
pub const STACK_ABOVE: u32 = 0x40;
/// Moby fields the call writes: the query stamp (word) and the touch index (byte).
pub const MOBY_STAMP: usize = 0xa0;
pub const MOBY_TOUCH: usize = 0xa8;
pub const MOBY_STRIDE: u32 = 0x100;
pub const MAX_MOBYS: u32 = 8192;
/// Hardware window (`Mem::hw`): the SPR channels 0x1000d000.. (the mesh loader's DMA registers) and D_STAT (a kick sets its bit).
pub const DMA_LO: usize = 0xd000;
pub const DMA_BYTES: usize = 0x500;
pub const D_STAT: usize = 0xe010;

/// What one call needs to know of the machine to build the restore set.
#[derive(Clone, Copy, Debug)]
pub struct Plan {
	pub res: u32,
	pub span: usize,
	/// Lowest address of the stack window and its length.
	pub stack: u32,
	pub stack_len: usize,
}

/// The argument buffer and the lowest address of the stack window for a caller stack pointer, or None when `sp` is no RAM stack (the
/// scratchpad, an unaligned or wild value) or the window leaves RAM. The buffer holds the two vec4 (32 bytes); the callee's `sp` is
/// `buf - 0x40`.
pub fn stack_plan(sp: u32, ram_len: usize) -> Option<(u32, u32)> {
	if sp & 3 != 0 || sp >= 0xc000_0000 || (sp & 0x1fff_ffff) as usize >= ram_len {
		return None;
	}
	let buf = sp.wrapping_sub(0x200) & !0xf;
	let lo = buf.wrapping_sub(STACK_BELOW);
	let off = (lo & 0x1fff_ffff) as usize;
	(off >= 0x1000 && off + (STACK_BELOW + STACK_ABOVE) as usize <= ram_len).then_some((buf, lo))
}

fn grab(dst: &mut Vec<u8>, at: &mut usize, src: &[u8], off: Option<usize>, n: usize) {
	match off.filter(|o| o.checked_add(n).is_some_and(|e| e <= src.len())) {
		Some(o) => {
			dst.clear();
			dst.extend_from_slice(&src[o..o + n]);
			*at = o;
		}
		None => {
			dst.clear();
			*at = 0;
		}
	}
}

/// The restore buffers (reused across probes) and the moby list. `save` then `restore` leaves RAM, the scratchpad and the hardware
/// window bit-identical to before `save`, however the call in between scribbled in them.
#[derive(Default)]
pub struct Wall {
	/// Moby record offsets (RAM offsets, masked) of the current table.
	pub mobys: Vec<u32>,
	/// The camera helper moby(s) hidden from the line test (`modcam_marker`), found when the probe is built.
	pub markers: Vec<crate::modcam_marker::Held>,
	res_at: usize,
	res: Vec<u8>,
	key_at: usize,
	key: Vec<u8>,
	slot_at: usize,
	slot: Vec<u8>,
	stack_at: usize,
	stack: Vec<u8>,
	spr: Vec<u8>,
	/// The whole hardware register page (`Mem::hw`: D_STAT, the channel registers, INTC).
	hw: Vec<u8>,
	stamps: Vec<(u32, u32, u8)>,
	vu0d: Vec<u8>,
	diag: Diag,
}

impl Wall {
	/// Reads the moby table (start, end, spawned-end words at `table`) into `mobys` (every 0x100 record from the start to the later
	/// end); returns the start (the hero, the first record) or None for an implausible table.
	pub fn collect_mobys(&mut self, ram: &[u8], table: u32) -> Option<u32> {
		self.mobys.clear();
		let m = |a| rd32s(ram, table + a).map(|w| w & 0x01ff_ffff);
		let (s, e) = (m(0)?, m(4)?.max(m(8)?));
		if s == 0 || e <= s || (e - s) / MOBY_STRIDE > MAX_MOBYS || e as usize > ram.len() {
			return None;
		}
		self.mobys.extend((s..e).step_by(MOBY_STRIDE as usize).filter(|a| *a as usize + MOBY_TOUCH < ram.len()));
		Some(s)
	}

	/// Snapshots the restore set; false (nothing to restore) when the result block or the stack window is outside `ram`.
	pub fn save(&mut self, ram: &[u8], spr: &[u8], hw: &[u8], p: &Plan) -> bool {
		let r = (p.res & 0x01ff_ffff) as usize;
		let st = (p.stack & 0x01ff_ffff) as usize;
		if r.checked_add(p.span).is_none_or(|e| e > ram.len()) || st + p.stack_len > ram.len() {
			return false;
		}
		grab(&mut self.res, &mut self.res_at, ram, Some(r), p.span);
		grab(&mut self.stack, &mut self.stack_at, ram, Some(st), p.stack_len);
		let ptr = |a: usize| rd32(ram, r + a).filter(|w| *w != 0).map(|w| (w & 0x01ff_ffff) as usize);
		grab(&mut self.key, &mut self.key_at, ram, ptr(8), KEY_BYTES);
		let slot = ptr(4).filter(|o| !(*o >= r && o + SLOT_BYTES <= r + p.span));
		grab(&mut self.slot, &mut self.slot_at, ram, slot, SLOT_BYTES);
		self.stamps.clear();
		self.stamps.extend(self.mobys.iter().filter_map(|m| Some((*m, rd32(ram, *m as usize + MOBY_STAMP)?, *ram.get(*m as usize + MOBY_TOUCH)?))));
		self.spr.clear();
		self.spr.extend_from_slice(spr);
		self.hw.clear();
		self.hw.extend_from_slice(hw);
		true
	}

	/// Writes the snapshot back.
	pub fn restore(&self, ram: &mut [u8], spr: &mut [u8], hw: &mut [u8]) {
		for (at, b) in [(self.res_at, &self.res), (self.key_at, &self.key), (self.slot_at, &self.slot), (self.stack_at, &self.stack)] {
			ram[at..at + b.len()].copy_from_slice(b);
		}
		for (m, stamp, touch) in &self.stamps {
			let m = *m as usize;
			ram[m + MOBY_STAMP..m + MOBY_STAMP + 4].copy_from_slice(&stamp.to_le_bytes());
			ram[m + MOBY_TOUCH] = *touch;
		}
		spr.copy_from_slice(&self.spr);
		if hw.len() == self.hw.len() {
			hw.copy_from_slice(&self.hw);
		}
	}

	/// VU0 data memory (`Mem::vu0d`, 4 KB): not written by the segment test's own code, but its callees might run a VU0 program.
	pub fn save_vu0(&mut self, vu0d: &[u8]) {
		self.vu0d.clear();
		self.vu0d.extend_from_slice(vu0d);
	}

	pub fn restore_vu0(&self, vu0d: &mut [u8]) {
		if vu0d.len() == self.vu0d.len() {
			vu0d.copy_from_slice(&self.vu0d);
		}
	}

	/// Bytes the restore set holds (tests, logs).
	pub fn saved_bytes(&self) -> usize {
		self.res.len() + self.key.len() + self.slot.len() + self.stack.len() + self.spr.len() + self.hw.len() + self.stamps.len() * 5
	}
}

// ------------------------------------------------------------------------------------------------------------------ diff

/// Differing bytes between two images and the first ranges (gaps under 16 bytes merge).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Diff {
	pub total: usize,
	pub ranges: Vec<(usize, usize)>,
}

pub fn diff(a: &[u8], b: &[u8], max_ranges: usize) -> Diff {
	let mut d = Diff::default();
	let mut open: Option<(usize, usize)> = None;
	for (ci, (ca, cb)) in a.chunks(4096).zip(b.chunks(4096)).enumerate() {
		if ca == cb {
			continue;
		}
		for (i, (x, y)) in ca.iter().zip(cb).enumerate() {
			if x == y {
				continue;
			}
			let p = ci * 4096 + i;
			d.total += 1;
			match open {
				Some((s, e)) if p < e + 16 => open = Some((s, p + 1)),
				Some(r) => {
					if d.ranges.len() < max_ranges {
						d.ranges.push(r);
					}
					open = Some((p, p + 1));
				}
				None => open = Some((p, p + 1)),
			}
		}
	}
	if let Some(r) = open.filter(|_| d.ranges.len() < max_ranges) {
		d.ranges.push(r);
	}
	d
}

/// Distance from `p` to the segment `a`-`b`.
pub fn point_seg_dist(a: V3, b: V3, p: V3) -> f32 {
	let (ab, ap) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [p[0] - a[0], p[1] - a[1], p[2] - a[2]]);
	let l2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
	let t = if l2 > 0.0 { ((ap[0] * ab[0] + ap[1] * ab[1] + ap[2] * ab[2]) / l2).clamp(0.0, 1.0) } else { 0.0 };
	let d = [ap[0] - ab[0] * t, ap[1] - ab[1] * t, ap[2] - ab[2] * t];
	(d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

// ------------------------------------------------------------------------------------------------------------------- diag

/// Probes that get the full RAM comparison (a 32 MB copy each).
pub const DIAG_PROBES: u32 = 300;

/// `RAC2_CAM_DIAG=1` [once].
pub fn diag_on() -> bool {
	static D: OnceLock<bool> = OnceLock::new();
	*D.get_or_init(|| std::env::var("RAC2_CAM_DIAG").is_ok_and(|v| v == "1"))
}

static DIAG_N: AtomicU32 = AtomicU32::new(0);
static DIAG_BAD: AtomicU64 = AtomicU64::new(0);
static DIAG_OFF: AtomicU32 = AtomicU32::new(0);
static DIAG_HITS: AtomicU32 = AtomicU32::new(0);

/// Total residual bytes (RAM + scratchpad + hardware page + VU0 data, +1 per differing register set or clock field) over the probes
/// compared so far; 0 is the proof.
pub fn diag_residual() -> u64 {
	DIAG_BAD.load(Relaxed)
}

/// The pre-call copy of everything the comparison covers.
#[derive(Default)]
struct Diag {
	ram: Vec<u8>,
	spr: Vec<u8>,
	hw: Vec<u8>,
	vu0d: Vec<u8>,
	vu0m: Vec<u8>,
	regs: Option<Regs>,
	clock: Option<Clock>,
	on: bool,
}

impl Diag {
	fn begin(&mut self, c: &Cpu) {
		self.on = diag_on() && DIAG_N.fetch_add(1, Relaxed) < DIAG_PROBES;
		if self.on {
			self.ram.clear();
			self.ram.extend_from_slice(&c.mem.ram);
			self.spr.clear();
			self.spr.extend_from_slice(&c.mem.spr);
			self.hw.clear();
			self.hw.extend_from_slice(&c.mem.hw);
			self.vu0d.clear();
			self.vu0d.extend_from_slice(&c.mem.vu0d);
			self.vu0m.clear();
			self.vu0m.extend_from_slice(&c.mem.vu0m);
			self.regs = Some(Regs::save(c));
			self.clock = Some(Clock::read(c));
		}
	}

	/// Raw writes of the call (before the restore): what the restore set has to cover.
	fn wrote(&self, c: &Cpu) -> Diff {
		diff(&self.ram, &c.mem.ram, 12)
	}

	/// After the restore: logs and counts the residual.
	fn end(&self, c: &Cpu, a: V3, b: V3, hit: Option<V3>, wrote: &Diff) {
		let ram = diff(&self.ram, &c.mem.ram, 16);
		let spr = diff(&self.spr, &c.mem.spr, 4).total;
		let hw = diff(&self.hw, &c.mem.hw, 4).total;
		let vu0d = diff(&self.vu0d, &c.mem.vu0d, 4).total;
		let vu0m = diff(&self.vu0m, &c.mem.vu0m, 4).total;
		let regs = self.regs.as_ref().is_some_and(|r| *r == Regs::save(c));
		let clock = self.clock.as_ref().is_some_and(|k| *k == Clock::read(c));
		let bad = (ram.total + spr + hw + vu0d) as u64 + u64::from(!regs) + u64::from(!clock);
		DIAG_BAD.fetch_add(bad, Relaxed);
		let n = DIAG_N.load(Relaxed).min(DIAG_PROBES);
		let off = hit.map(|p| point_seg_dist(a, b, p));
		if let Some(d) = off {
			DIAG_HITS.fetch_add(1, Relaxed);
			if d > 0.5 {
				DIAG_OFF.fetch_add(1, Relaxed);
			}
		}
		if n <= 12 || bad != 0 {
			let r = |d: &Diff| d.ranges.iter().map(|(s, e)| format!("{s:#x}+{:#x}", e - s)).collect::<Vec<_>>().join(" ");
			eprintln!("rac2 cam diag #{n}: {a:.2?} -> {b:.2?} hit {hit:.2?} (off segment {off:.3?}); retail wrote {} B in [{}]; RESIDUAL ram {} spr {spr} hw {hw} vu0d {vu0d} regs {} clock {} (info: vu0m {vu0m}){}", wrote.total, r(wrote), ram.total, if regs { "same" } else { "DIFFER" }, if clock { "same" } else { "DIFFER" }, if bad == 0 { String::new() } else { format!(" first at [{}]", r(&ram)) });
		}
		if n == DIAG_PROBES {
			eprintln!("rac2 cam diag SUMMARY: {n} probes, residual {} bytes, {} hits, {} hits more than 0.5 m off their segment", diag_residual(), DIAG_HITS.load(Relaxed), DIAG_OFF.load(Relaxed));
		}
	}
}

// ------------------------------------------------------------------------------------------------------------------ probe

/// The camera line mask the wall tests pass (a2): R&C1's 0xb4 [I], no R&C2 caller found to confirm.
pub const CAM_MASK: u32 = 0xb4;

/// `Probe` over the retail segment test of the running machine. Build one per hand call (the moby list is read at construction), pass
/// it to `Cam::step`, drop it; `c` is usable again after that.
pub struct RetailProbe<'a> {
	c: &'a mut Cpu,
	seg: SegFn,
	wall: &'a mut Wall,
	/// The ignored moby (a3): the hero, the first record of the table.
	hero: u32,
	/// Game units per metre the segment test works in (1.0 [C]).
	pub unit: f32,
	/// The line mask (a2).
	pub mask: u32,
	pub calls: u32,
	pub hits: u32,
	/// Hits on a moby riding on the eye that were skipped (the helper).
	pub rides: u32,
	/// The moby (RAM address, 0 = terrain) and its class id (+0xaa) of every final hit of this probe, for the log (first 8).
	pub hit_mobys: Vec<(u32, u16)>,
}

/// The two words before the finder's entry that retail's callers run (`jal entry-8`): `pcpyld` and `lqc2 vf28, 0(a0)`, the START
/// vector. The recompiler left them out of the function (as with the follow camera's), so a call at `entry` would test from a stale
/// vf28 [C Aranos 0x2dec90: 130 callers `jal 0x2dec90`, none `jal 0x2dec98`; the first probe run hit a wall 9 m off its segment].
pub const PROLOGUE: [u32; 2] = [0x7200_8389, 0xd89c_0000];

/// Whether the two words before `entry` are the prologue (`PROLOGUE`).
pub fn prologue_ok(ram: &[u8], entry: u32) -> bool {
	let at = (entry.wrapping_sub(8) & 0x01ff_ffff) as usize;
	at + 8 <= ram.len() && PROLOGUE.iter().enumerate().all(|(k, &w)| rd32(ram, at + 4 * k) == Some(w))
}

impl<'a> RetailProbe<'a> {
	/// `table` = the address of the moby table's start word (`Addrs::moby`); None when the table is implausible or the segment test
	/// has not the prologue words before it.
	pub fn try_new(c: &'a mut Cpu, seg: SegFn, wall: &'a mut Wall, table: u32) -> Option<RetailProbe<'a>> {
		if !prologue_ok(&c.mem.ram, seg.entry) {
			return None;
		}
		let hero = wall.collect_mobys(&c.mem.ram, table)?;
		wall.markers = if crate::modcam_marker::enabled() { crate::modcam_marker::find(&c.mem.ram, &wall.mobys) } else { Vec::new() };
		Some(RetailProbe { c, seg, wall, hero, unit: 1.0, mask: CAM_MASK, calls: 0, hits: 0, rides: 0, hit_mobys: Vec::new() })
	}

	/// The moby the tests ignore is the character the player controls (`modcam_who`; Clank in his sections) instead of the table's
	/// first record; for Ratchet both are the same record.
	pub fn ignoring(mut self, moby: u32) -> RetailProbe<'a> {
		if moby != 0 {
			self.hero = moby;
		}
		self
	}

	/// One run of the segment test; `skip` = mobys it must not test: their stamp (+0xa0) is set to the id this query will use, and the
	/// test skips a moby whose stamp equals the id (the broadphase already saw it) [I from R&C1 stamps.rs; the run proves it]. The
	/// contact (None = no hit) and the moby it says it hit (res+0x18, 0 = terrain); None = could not run.
	fn cast_once(&mut self, a: V3, b: V3, skip: &[u32]) -> Option<(Option<V3>, u32)> {
		let u = self.unit;
		let (a, b) = (a.map(|x| x * u), b.map(|x| x * u));
		if a.iter().chain(&b).any(|x| !x.is_finite()) {
			return None;
		}
		let sp = self.c.g(29) as u32;
		let (buf, stack) = stack_plan(sp, self.c.mem.ram.len())?;
		let plan = Plan { res: self.seg.res, span: self.seg.span(), stack, stack_len: (STACK_BELOW + STACK_ABOVE) as usize };
		let regs = Regs::save(self.c);
		if !self.wall.save(&self.c.mem.ram, &self.c.mem.spr, &self.c.mem.hw, &plan) {
			return None;
		}
		self.wall.save_vu0(&self.c.mem.vu0d);
		self.wall.diag.begin(self.c);
		// From here the clock and the event state are held (see `Clock`): the segment test kicks the SPR DMA, and a pending VIF1/DMA
		// stop or an interrupt line must not fire inside it.
		let clock = Clock::hold(self.c);
		let o = (buf & 0x01ff_ffff) as usize;
		for (k, v) in [a[0], a[1], a[2], 1.0, b[0], b[1], b[2], 1.0].into_iter().enumerate() {
			self.c.mem.ram[o + k * 4..o + k * 4 + 4].copy_from_slice(&v.to_le_bytes());
		}
		// a0 / a1 = the vec4 ends, a2 = mask, a3 = the ignored moby, t0 = 0 (no collision-event record); the callee's sp under the buffer.
		for (reg, v) in [(29, buf.wrapping_sub(0x40)), (4, buf), (5, buf.wrapping_add(0x10)), (6, self.mask), (7, self.hero), (8, 0)] {
			self.c.s(reg, v as i32 as i64 as u64);
		}
		let id = u32::from_le_bytes(self.c.mem.ram[self.seg.res as usize + 0x10..self.seg.res as usize + 0x14].try_into().unwrap()).wrapping_add(1);
		for &m in skip.iter().filter(|m| self.wall.mobys.binary_search(m).is_ok()) {
			// Only a moby of the table: `Wall::restore` puts back the stamps of those (and only those).
			self.c.mem.wr32(m + MOBY_STAMP as u32, id);
		}
		// The camera's own helper moby rides on the eye: it is no wall (`modcam_marker`). Hidden for this call only.
		crate::modcam_marker::hide(&mut self.c.mem.ram, &self.wall.markers);
		self.c.call(self.seg.entry - 8);
		crate::modcam_marker::show(&mut self.c.mem.ram, &self.wall.markers);
		let hit = self.c.g(2) as u32 & 1 != 0 && self.c.stop.is_none();
		let p = (self.seg.res as usize + 0x20).min(self.c.mem.ram.len() - 12);
		let f = |k: usize| f32::from_le_bytes(self.c.mem.ram[p + k * 4..p + k * 4 + 4].try_into().unwrap());
		let contact = [f(0), f(1), f(2)];
		let moby = u32::from_le_bytes(self.c.mem.ram[p - 8..p - 4].try_into().unwrap()) & 0x01ff_ffff;
		if trace_on() && self.calls < 40 {
			self.trace(a, b, hit);
		}
		let wrote = self.wall.diag.on.then(|| self.wall.diag.wrote(self.c));
		self.wall.restore(&mut self.c.mem.ram, &mut self.c.mem.spr, &mut self.c.mem.hw);
		self.wall.restore_vu0(&mut self.c.mem.vu0d);
		regs.restore(self.c);
		clock.release(self.c);
		if let Some(w) = wrote {
			self.wall.diag.end(self.c, a, b, hit.then_some(contact), &w);
		}
		self.calls += 1;
		Some(((hit && contact.iter().all(|x| x.is_finite())).then(|| contact.map(|x| x / u)), moby))
	}

	/// The camera's hidden helper moby rides on the eye (R&C1: class 1007, `cam_pulse.md`): a boom test that sees it pulls in, it
	/// follows, the boom releases into it again, the camera pulses. A hit on a moby within `RIDER` of the segment's end is that
	/// helper (or something else sitting on the eye): the test is run again with it skipped, up to `RIDES` times; when the skips run
	/// out the LAST hit stands (a real moby at the end of the segment is a wall, not dropped). R&C1 excludes its helper by slot
	/// address; R&C2's slot is known now (`modcam_marker.rs`: `basis - 0x465c`, 26/26 dumps) and that helper is hidden from the test, so this distance rule is only a fallback for other riders (a hoverboard or gadget sitting on the eye).
	fn cast(&mut self, a: V3, b: V3) -> Option<V3> {
		let mut skip: Vec<u32> = Vec::new();
		let (mut out, mut last) = (None, 0);
		while let Some((p, moby)) = self.cast_once(a, b, &skip) {
			(out, last) = (p, moby);
			if p.is_none() || skip.len() as u32 >= RIDES || !self.skippable(moby, b) {
				break;
			}
			self.rides += 1;
			skip.push(moby);
		}
		self.hits += u32::from(out.is_some());
		if out.is_some() && self.hit_mobys.len() < 8 {
			let class = self.c.mem.ram.get(last as usize + 0xaa..last as usize + 0xac).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]));
			self.hit_mobys.push((last, class));
		}
		out
	}

	/// A hit on `moby` may be skipped: a moby of the table (its stamp can be set and put back), not the hero, near the segment end.
	fn skippable(&self, moby: u32, end: V3) -> bool {
		moby != 0 && moby != self.hero && self.wall.mobys.binary_search(&moby).is_ok() && self.rides_on(moby, end)
	}

	/// The moby (RAM address) is within `RIDER` metres of the point.
	fn rides_on(&self, moby: u32, at: V3) -> bool {
		let o = moby as usize + 0x10;
		let Some(w) = self.c.mem.ram.get(o..o + 12) else { return false };
		let pos = [0, 4, 8].map(|k| f32::from_le_bytes(w[k..k + 4].try_into().unwrap()) / self.unit);
		pos.iter().all(|x| x.is_finite()) && (0..3).map(|k| (pos[k] - at[k]) * (pos[k] - at[k])).sum::<f32>() < RIDER * RIDER
	}
}

/// A moby this close (m) to the end of the camera's line is the camera's own helper, not a wall.
pub const RIDER: f32 = 1.0;
/// How many such riders one test skips before it gives up (and reports no hit).
pub const RIDES: u32 = 3;

impl RetailProbe<'_> {
	/// How many helper mobys are hidden from the line tests (`modcam_marker`), for the log.
	pub fn markers(&self) -> usize {
		self.wall.markers.len()
	}

	/// `RAC2_CAM_PROBE_TRACE=1`: one line per probe (the first 40): ends, v0, v1, the result block's first 0x40 bytes (before the restore).
	fn trace(&self, a: V3, b: V3, hit: bool) {
		let r = self.seg.res as usize;
		let w = |k: usize| u32::from_le_bytes(self.c.mem.ram[r + k * 4..r + k * 4 + 4].try_into().unwrap());
		let words: Vec<String> = (0..16).map(|k| format!("{:08x}", w(k))).collect();
		let f = |k: usize| f32::from_bits(w(k));
		// The moby the test says it hit (res+0x18): its first 0x30 bytes.
		let m = (w(6) & 0x01ff_ffff) as usize;
		let moby: Vec<String> = if m != 0 && m + 0x30 <= self.c.mem.ram.len() { (0..12).map(|k| format!("{:08x}", u32::from_le_bytes(self.c.mem.ram[m + k * 4..m + k * 4 + 4].try_into().unwrap()))).collect() } else { Vec::new() };
		eprintln!("rac2 cam trace moby {m:#x} (hero {:#x}): {}", self.hero, moby.join(" "));
		eprintln!(
			"rac2 cam trace #{}: {a:.2?} -> {b:.2?} mask {:#x} hit {hit} v0 {:#x} v1 {:#x} res[+0x20..] {:.2} {:.2} {:.2} {:.2} words {}",
			self.calls, self.mask, self.c.g(2) as u32, self.c.g(3) as u32, f(8), f(9), f(10), f(11), words.join(" ")
		);
	}
}

/// `RAC2_CAM_PROBE_TRACE=1` [once].
fn trace_on() -> bool {
	static T: OnceLock<bool> = OnceLock::new();
	*T.get_or_init(|| std::env::var("RAC2_CAM_PROBE_TRACE").is_ok_and(|v| v == "1"))
}

impl Probe for RetailProbe<'_> {
	fn seg(&mut self, a: V3, b: V3) -> Option<V3> {
		self.cast(a, b)
	}
}
