//! The Buy all ammo hand against retail's own purchase (states on this PC; each test skips without its files).
use super::*;
use ee_oracle::Machine;

const L2_STATE: &str = "E:/ratchet/rac2/states/fg1/0125";
/// The level-2 shop open in browse (stage 0) [C: scratch_rc2_scr vmod run, field 100].
const L2_SHOP: &str = "E:/ratchet/scratch_rc2_scr/runs/vmod/ram00100.bin";
const L1_STATE: &str = "E:/ratchet/scratch_rc2_b42/bisect/qs_l1";

fn have(p: &str) -> bool {
	std::path::Path::new(p).exists()
}

fn machine(dir: &str, ram: Option<&str>) -> Machine {
	let mut m = Machine::load_resume(dir);
	if let Some(f) = ram {
		let b = std::fs::read(f).expect("ram dump");
		m.cpu.mem.ram[..b.len()].copy_from_slice(&b);
	}
	m
}

#[test]
fn spec_matches_every_level_copy_once() {
	let r = reloc();
	for (f, at) in [
		("E:/ratchet/scratch_rc2_b42/bisect/qs_l1/eeMemory.bin", 0x36d250u32),
		("E:/ratchet/rac2/states/fg1/0125/eeMemory.bin", 0x37ea40),
		("E:/ratchet/rac2/states/fg1/0187/eeMemory.bin", 0x39ac38),
		("E:/ratchet/rac2/states/fg1/0260/eeMemory.bin", 0x3b8670),
		("E:/ratchet/rac2/states/fg1/0473/eeMemory.bin", 0x38b9a0),
		("E:/ratchet/rac2/states/fg1/0555/eeMemory.bin", 0x37c8c8),
	] {
		let Ok(ram) = std::fs::read(f) else { continue };
		let rd = |a: u32| u32::from_le_bytes(ram[a as usize..a as usize + 4].try_into().unwrap());
		let hits: Vec<u32> = (0x280000u32..0x400000).step_by(4).filter(|&a| rd(a) == 0x27bd_fff0 && r.masked_hash_by(a, rd) == r.mhash).collect();
		assert_eq!(hits, vec![at], "{f}");
	}
}

#[test]
fn vendor_decodes_on_level_1_and_2() {
	if have(L1_STATE) {
		let m = machine(L1_STATE, None);
		let v = Vendor::at(&m.cpu, 0x36d250).expect("level 1 vendor");
		assert_eq!((v.block, v.e), (0x22ff00, 0x36c008));
	}
	if have(L2_STATE) {
		let m = machine(L2_STATE, None);
		let v = Vendor::at(&m.cpu, 0x37ea40).expect("level 2 vendor");
		assert_eq!((v.block, v.e, v.table), (0x230fc0, 0x37d7f8, 0x264af0));
		assert_eq!((v.add, v.rebuild, v.cue, v.post), (0x2d7960, 0x37b480, 0x3692d8, 0x383a48));
	}
}

/// Bolts and the ammo words.
fn purse(m: &mut Machine) -> (u32, Vec<u32>) {
	(m.cpu.mem.rd32(crate::modern_screen::BOLTS), (0..64).map(|i| m.cpu.mem.rd32(AMMO + 4 * i)).collect())
}

/// Retail's own purchase of every ammo row, row by row: the prompt (stage 1 sets the quantity at its cap), then the purchase
/// (`*param_1` = 1, stage 4), each by calling E as the dispatcher does.
fn by_hand(m: &mut Machine, v: &Vendor) {
	let gp = m.cpu.g(28) as u32;
	for k in 0..24u32 {
		let row = v.block + 0x140 + k * ROW_STRIDE;
		let item = m.cpu.mem.rd32(row);
		if item == 0 {
			break;
		}
		if m.cpu.mem.rd32(row + 4) != 1 {
			continue;
		}
		let rec = v.table + m.cpu.mem.rd8(SLOT_MAP + item) as u32 * REC_SIZE;
		let (unit, max) = (m.cpu.mem.rd16(rec + 0x88) as u32, m.cpu.mem.rd16(rec + 0x8e) as u32);
		let owned = m.cpu.mem.rd32(AMMO + 4 * item);
		let bolts = m.cpu.mem.rd32(crate::modern_screen::BOLTS);
		if unit == 0 || owned >= max || bolts / unit == 0 {
			continue;
		}
		m.cpu.mem.wr32(v.block + 0x80, k);
		for stage in [1u32, 4] {
			if stage == 4 {
				m.cpu.mem.wr32(gp + 0x4cc0, 1);
			}
			m.cpu.mem.wr32(v.block + 0x84, stage);
			m.cpu.s(4, sx(gp + 0x4cc0));
			m.cpu.s(5, sx(gp + 0x4cc4));
			m.cpu.call(v.e);
		}
		let q = m.cpu.mem.rd32(crate::modern_vendor::L2.qty);
		assert_eq!(q, (max - owned).min(bolts / unit), "row {k}: the prompt opened at its cap");
	}
}

fn compare(setup: impl Fn(&mut Machine, &Vendor)) -> ((u32, Vec<u32>), (u32, Vec<u32>), (u32, Vec<u32>)) {
	let mut a = machine(L2_STATE, Some(L2_SHOP));
	let v = Vendor::at(&a.cpu, 0x37ea40).expect("level 2 vendor");
	assert_eq!(a.cpu.mem.rd32(v.block + 0x84), 0, "the dump is in browse");
	setup(&mut a, &v);
	let before = purse(&mut a);
	let mut b = machine(L2_STATE, Some(L2_SHOP));
	setup(&mut b, &v);
	buy_all(&mut a.cpu, &v);
	by_hand(&mut b, &v);
	(before, purse(&mut a), purse(&mut b))
}

#[test]
fn buy_all_equals_retail_row_by_row() {
	if !have(L2_STATE) || !have(L2_SHOP) {
		return;
	}
	let (before, all, hand) = compare(|_, _| {});
	assert_eq!(all, hand, "rich");
	assert!(all.0 < before.0 && all.1 != before.1, "something was bought");
	eprintln!("rich: bolts {} -> {}", before.0, all.0);
	let (before, all, hand) = compare(|m, _| m.cpu.mem.wr32(crate::modern_screen::BOLTS, 12));
	assert_eq!(all, hand, "poor");
	eprintln!("poor: bolts {} -> {}", before.0, all.0);
	let (before, all, hand) = compare(|m, v| {
		for k in 0..24u32 {
			let row = v.block + 0x140 + k * ROW_STRIDE;
			let item = m.cpu.mem.rd32(row);
			if item == 0 {
				break;
			}
			let rec = v.table + m.cpu.mem.rd8(SLOT_MAP + item) as u32 * REC_SIZE;
			let max = m.cpu.mem.rd16(rec + 0x8e) as u32;
			if m.cpu.mem.rd32(row + 4) == 1 {
				m.cpu.mem.wr32(AMMO + 4 * item, max);
			}
		}
	});
	assert_eq!(all, hand, "all full");
	assert_eq!(all, before, "all full: nothing changes");
}

#[test]
fn no_request_no_change() {
	if !have(L2_STATE) || !have(L2_SHOP) {
		return;
	}
	let _l = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
	let mut a = machine(L2_STATE, Some(L2_SHOP));
	let before = a.cpu.mem.ram.clone();
	cancel();
	a.cpu.pc = 0x37ea40;
	assert!(!tick(&mut a.cpu));
	assert!(a.cpu.mem.ram == before, "no request: no RAM write");
}
