//! [PC] "Buy all ammo" in ONE go for R&C2's modern vendor, as R&C1's (rac1/retail/vendor.rs `buy_all`): a relocatable hand on the
//! vendor's per-frame dispatcher that, on a pending request in browse, buys every ammo row in list order with retail's own calls.
//! No request = it declines at once and retail's dispatcher runs untouched (no RAM write).
//!
//! THE DISPATCHER [C, level 2 0x37ea40, 41 words; one copy per level: L1 0x36d250, L2 0x37ea40, L4 0x39ac38, L6 0x3b8670,
//! L11 0x38b9a0, L12 0x37c8c8 (states qs_l1 0125 0187 0260 0473 0555)]: `lui v0` (+0x4) + `addiu v0` (+0xc) = the vendor block B;
//! stage = B+0x84; stage 0 returns at once; else by B+0x50 (0 = the weapon vendor) it calls the quantity / purchase function E (jal at
//! +0x30, a0 = gp+0x4cc0, a1 = gp+0x4cc4). `SPEC` masks the per-level fields (gp imm16 at +0x28/+0x2c/.., the jal targets).
//!
//! RETAIL'S PURCHASE OF ONE AMMO ROW [C, E stage 4, level 2 0x37dd40..0x37e048; the same offsets in all six copies above]: ammo row
//! (rows B+0x140 stride 0x18 {item, kind 1}), rec = table + slot_map[item] * 0xe0, unit = rec+0x88 (u16), max = rec+0x8e (u16);
//! ret = ADD_AMMO(item, q) (jal E+0x5c8: ammo[item] += q clamped to max, answers the overflow); bolts (gp-0x75f0) -= unit * (q - ret);
//! REBUILD(0) (E+0x60c); TEXT(0xbdc) (E+0x614) -> HEADER (E+0x61c); CUE(7, 0x11, B+0x28) (E+0x82c); stage = 0; POST (E+0x84c).
//! Bolts short: TEXT(0xbdd) -> HEADER, CUE(0, ..). The prompt's cap (stage 1) is q = min(max - ammo, bolts / unit) (`Row::most`).
//! Here: per ammo row q = that cap with the bolts left, ADD_AMMO + the bolts + REBUILD(0); then ONE header line, ONE cue and POST,
//! as one press of retail's Buy would end. Bolts short for a row = it is skipped (later, cheaper rows still buy), as R&C1.
use crate::modern_vendor::{AMMO, REC_SIZE, ROW_STRIDE, SLOT_MAP};
use ee_oracle::cpu::Cpu;
use ee_oracle::hs_reloc::{Reloc, RelocHand};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// The level-2 dispatcher (0x37ea40, 0xa4 bytes) with the per-level fields masked (hash: FNV-1a 64 over the masked words; one match
/// per level in the six states named above).
const SPEC: &str = "reloc vendor_tick entry 0x37ea40 len 0xa4\nmhash 0x77338ed675ef1959\nmask 0x4 0xffff\nmask 0xc 0xffff\nmask 0x28 0xffff\nmask 0x2c 0xffff\nmask 0x30 0x3ffffff\nmask 0x48 0xffff\nmask 0x4c 0xffff\nmask 0x50 0x3ffffff\nmask 0x68 0xffff\nmask 0x6c 0xffff\nmask 0x70 0x3ffffff\nmask 0x88 0xffff\nmask 0x8c 0xffff\nmask 0x90 0x3ffffff\n";
const EST: u64 = 8;

/// Offsets in E of the calls and their delay-slot words (checked before use: any other code = nothing is done).
const AT_ADD: u32 = 0x5c8;
const AT_REBUILD: u32 = 0x60c;
const AT_TEXT: u32 = 0x614;
const AT_HEADER: u32 = 0x61c;
const AT_CUE: u32 = 0x82c;
const AT_POST: u32 = 0x84c;
const CHECKS: [(u32, u32); 4] = [(0x610, 0xaf85_8a10), (0x618, 0x2404_0bdc), (0x818, 0x2404_0bdd), (0x830, 0x2405_0011)];
/// Text ids of the header line: bought / not enough bolts.
const ID_THANKS: u64 = 0xbdc;
const ID_NO_BOLTS: u64 = 0xbdd;
/// The bolts word: gp - 0x75f0 (retail's store; 0x1a7a80 on every level).
const BOLTS_GP: u32 = 0x75f0;
const MAX_ROWS: u32 = crate::modern_vendor::MAX_ROWS as u32;

static PENDING: AtomicBool = AtomicBool::new(false);
/// Tests that touch `PENDING` (here and in modern_vendor_tests.rs) run one at a time.
#[cfg(test)]
pub(crate) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn reloc() -> &'static Reloc {
	static R: OnceLock<Reloc> = OnceLock::new();
	R.get_or_init(|| Reloc::parse(SPEC).expect("the vendor dispatcher spec parses"))
}

/// Registers the hand (lib.rs `register`, before the recompiled table is built).
pub fn register() {
	static HAND: RelocHand = RelocHand { name: "vendor_tick", reloc, est: EST, f: tick };
	ee_oracle::rec::register_reloc_hand(&HAND);
}

/// Game thread (modern_vendor.rs `drive`): buy all the ammo at the dispatcher's next call (in browse only; else it is dropped).
pub fn request() {
	PENDING.store(true, Ordering::Relaxed);
}

/// A request not yet taken by the game.
pub fn pending() -> bool {
	PENDING.load(Ordering::Relaxed)
}

/// Drops a request (the shop closed).
pub fn cancel() {
	PENDING.store(false, Ordering::Relaxed);
}

/// [PC] Game thread, right before the field runs (host/game.rs): a replay with pc.bin has the request pending exactly as the
/// session had it; a recorded session notes a pending one (pc_rec.rs). Otherwise nothing.
pub fn rec_sync() {
	match crate::pc_rec::vendor_now() {
		Some(p) => PENDING.store(p, Ordering::Relaxed),
		None if pending() => crate::pc_rec::note(crate::pc_rec::Ev::Vendor),
		None => {}
	}
}

/// The hand: always declines (retail's dispatcher runs after it); with a request in browse it first buys all the ammo.
pub fn tick(c: &mut Cpu) -> bool {
	if PENDING.swap(false, Ordering::Relaxed) {
		let pc = c.pc;
		if let Some(v) = Vendor::at(c, pc) {
			if c.mem.rd32(v.block + 0x84) == 0 && c.mem.rd32(v.block + 0x50) == 0 {
				buy_all(c, &v);
			}
		}
	}
	false
}

/// One level's vendor: the block, E's callees and the record table.
pub struct Vendor {
	pub block: u32,
	pub e: u32,
	pub table: u32,
	add: u32,
	rebuild: u32,
	text: u32,
	header: u32,
	cue: u32,
	post: u32,
}

fn jal_target(w: u32, at: u32) -> Option<u32> {
	(w >> 26 == 3).then(|| (at.wrapping_add(4) & 0xf000_0000) | ((w & 0x03ff_ffff) << 2))
}

fn lui_addiu(hi: u32, lo: u32) -> Option<u32> {
	(hi >> 26 == 0x0f && lo >> 26 == 0x09).then(|| ((hi & 0xffff) << 16).wrapping_add(lo as u16 as i16 as i32 as u32))
}

impl Vendor {
	/// The vendor of the dispatcher copy at `d` (None: code that is not the one decoded above).
	pub fn at(c: &Cpu, d: u32) -> Option<Vendor> {
		let ram = &c.mem.ram;
		let w = |a: u32| ram.get(a as usize..a as usize + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
		let block = lui_addiu(w(d + 0x4), w(d + 0xc))?;
		let e = jal_target(w(d + 0x30), d + 0x30)?;
		if CHECKS.iter().any(|&(o, x)| w(e + o) != x) {
			return None;
		}
		let words = crate::modern_vendor_find::decode(ram, e as usize, 0)?;
		if words.cur != block + 0x80 {
			return None;
		}
		let call = |o: u32| jal_target(w(e + o), e + o);
		Some(Vendor { block, e, table: words.table, add: call(AT_ADD)?, rebuild: call(AT_REBUILD)?, text: call(AT_TEXT)?, header: call(AT_HEADER)?, cue: call(AT_CUE)?, post: call(AT_POST)? })
	}
}

struct Regs {
	r: [u128; 32],
	hi: u128,
	lo: u128,
	sa: u32,
	f: [u32; 32],
	facc: u32,
	fcr31: u32,
	vf: [[u32; 4]; 32],
	vacc: [u32; 4],
	vi: [u32; 32],
}

impl Regs {
	fn save(c: &Cpu) -> Regs {
		Regs { r: c.r, hi: c.hi, lo: c.lo, sa: c.sa, f: c.f, facc: c.facc, fcr31: c.fcr31, vf: c.vf, vacc: c.vacc, vi: c.vi }
	}
	fn restore(&self, c: &mut Cpu) {
		(c.r, c.hi, c.lo, c.sa, c.f, c.facc, c.fcr31, c.vf, c.vacc, c.vi) = (self.r, self.hi, self.lo, self.sa, self.f, self.facc, self.fcr31, self.vf, self.vacc, self.vi);
	}
}

fn sx(v: u32) -> u64 {
	v as i32 as i64 as u64
}

/// Every ammo row in list order at retail's prompt cap, then one header line, one cue and retail's post-purchase call. Registers
/// come back as they were. Returns (rows bought, a row the bolts were short for).
pub fn buy_all(c: &mut Cpu, v: &Vendor) -> (u32, bool) {
	let regs = Regs::save(c);
	let gp = c.g(28) as u32;
	let bolts_at = gp.wrapping_sub(BOLTS_GP);
	let (mut bought, mut broke) = (0u32, false);
	// [b46 W5] U3: the ammo items are listed BEFORE the first purchase and bought by item id, so a rebuild that reorders or drops
	// rows (it runs after every row) can never make the walk skip one. Same order and result as before while the rows stay put.
	let mut items = Vec::new();
	for k in 0..MAX_ROWS {
		let row = v.block + 0x140 + k * ROW_STRIDE;
		let item = c.mem.rd32(row);
		if item == 0 {
			break;
		}
		if c.mem.rd32(row + 4) == 1 && item < 0x80 {
			items.push(item);
		}
	}
	for item in items {
		let rec = v.table + c.mem.rd8(SLOT_MAP + item) as u32 * REC_SIZE;
		let (unit, max) = (c.mem.rd16(rec + 0x88) as i32, c.mem.rd16(rec + 0x8e) as i32);
		let owned = c.mem.rd32(AMMO + 4 * item) as i32;
		if unit <= 0 || owned >= max {
			continue;
		}
		let q = (max - owned).min(c.mem.rd32(bolts_at) as i32 / unit);
		if q <= 0 {
			broke = true;
			continue;
		}
		c.s(4, item as u64);
		c.s(5, q as u64);
		c.call(v.add);
		let got = q - c.g(2) as i32;
		let left = (c.mem.rd32(bolts_at) as i32).wrapping_sub(unit.wrapping_mul(got));
		c.mem.wr32(bolts_at, left as u32);
		c.s(4, 0);
		c.call(v.rebuild);
		bought += 1;
	}
	if bought > 0 || broke {
		c.s(4, if bought > 0 { ID_THANKS } else { ID_NO_BOLTS });
		c.call(v.text);
		let t = c.g(2);
		c.s(4, t);
		c.call(v.header);
	}
	c.s(4, if bought > 0 { 7 } else { 0 });
	c.s(5, 0x11);
	let vp = sx(c.mem.rd32(v.block + 0x28));
	c.s(6, vp);
	c.call(v.cue);
	c.mem.wr32(v.block + 0x84, 0);
	c.call(v.post);
	regs.restore(c);
	(bought, broke)
}

#[cfg(test)]
#[path = "modern_vendor_hand_tests.rs"]
mod tests;
