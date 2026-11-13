//! Unit tests of modcam_probe.rs: the finder (spec words against a synthetic code image and, when the research dumps exist, the real
//! level RAM), the per-level cache, the restore-set bookkeeping (`Wall`), the stack plan and the diff. The retail call itself needs a
//! running machine: its proof is the in-game `RAC2_CAM_DIAG=1` residual (ENV.md).

use crate::modcam_probe::*;

const WORDS: usize = 96;
const CODE_AT: usize = 0x2000;

fn xorshift(s: &mut u32) -> u32 {
	*s ^= *s << 13;
	*s ^= *s >> 17;
	*s ^= *s << 5;
	*s
}

/// Pseudo-random code words with the anchor at the start, a `lui`/`addiu` pair at word 3 (result block) and at WORDS - 8 (ring), and
/// a few known shapes in between.
fn template() -> Vec<u32> {
	let mut s = 0x1234_5678;
	let mut w: Vec<u32> = (0..WORDS).map(|_| xorshift(&mut s)).collect();
	w[..8].copy_from_slice(&[0x7220_8b89, 0xd8bd_0010, 0x4a20_0718, 0x3c0f_001c, 0x25ef_f240, 0x4a20_0758, 0x3c01_4480, 0]);
	w[10] = 0x0c0b_7afc; // jal
	w[11] = 0x1060_feff; // beqz
	w[12] = 0x8ded_0010; // lw
	w[13] = 0x0043_1821; // addu (nothing of it is loosened)
	w[WORDS - 8] = 0x3c04_001c; // lui a0
	w[WORDS - 7] = 0x2484_34c0; // addiu a0
	w
}

fn set_pair(w: &mut [u32], at: usize, addr: u32) {
	let lo = addr as u16;
	let hi = ((addr >> 16) as u16).wrapping_add(lo >> 15);
	w[at] = (w[at] & 0xffff_0000) | hi as u32;
	w[at + 1] = (w[at + 1] & 0xffff_0000) | lo as u32;
}

/// The same code linked elsewhere: jump targets and immediates differ, the result block and ring addresses are `res` / `ring`.
fn relocate(t: &[u32], salt: u32, res: u32, ring: u32) -> Vec<u32> {
	let mut o: Vec<u32> = t
		.iter()
		.enumerate()
		.map(|(i, &w)| match w >> 26 {
			2 | 3 => (w & 0xfc00_0000) | ((w ^ salt.wrapping_mul(i as u32 + 1)) & 0x03ff_ffff),
			1 | 4..=7 | 0x14..=0x17 => w,
			op if op >= 8 => (w & 0xffff_0000) | ((w ^ salt.wrapping_mul(i as u32 + 7)) & 0xffff),
			_ => w,
		})
		.collect();
	set_pair(&mut o, 3, res);
	set_pair(&mut o, WORDS - 8, ring);
	o
}

fn spec_for(t: &[u32]) -> Spec {
	let mut anchor = [0; 8];
	for (a, w) in anchor.iter_mut().zip(t) {
		*a = loose(*w);
	}
	Spec { lo: 0x1000, hi: 0x9000, anchor, words: t.len(), hash: hash_loose(t.iter().copied()), res_at: 3, ring_at: t.len() - 8 }
}

fn put(ram: &mut [u8], at: usize, w: &[u32]) {
	for (i, x) in w.iter().enumerate() {
		ram[at + i * 4..at + i * 4 + 4].copy_from_slice(&x.to_le_bytes());
	}
}

fn w32(ram: &mut [u8], at: usize, v: u32) {
	put(ram, at, &[v]);
}

#[test]
fn loose_drops_the_relocated_parts() {
	assert_eq!(loose(0x0c0b_7afc), 0x0c00_0000, "jal keeps the opcode only");
	assert_eq!(loose(0x0800_0123), 0x0800_0000, "j likewise");
	assert_eq!(loose(0x1060_feff), 0x1060_feff, "branches are PC-relative: whole");
	assert_eq!(loose(0x0461_0035), 0x0461_0035, "REGIMM branches: whole");
	assert_eq!(loose(0x3c0f_001c), 0x3c0f_0000, "lui keeps op and register");
	assert_eq!(loose(0x25ef_f240), 0x25ef_0000, "addiu likewise");
	assert_eq!(loose(0x7220_8b89), 0x7220_0000, "MMI / COP2 words keep the top 16 bits");
	assert_eq!(loose(0x4bdd_e0ef), 0x4bdd_0000);
	assert_eq!(loose(0x0003_7080), 0x0003_7080, "SPECIAL (op 0) stays whole");
}

#[test]
fn hash_known_answers() {
	// Computed with the research script (handoff_stageB.md section 3) from the same rule.
	assert_eq!(hash_loose([0x7220_8b89, 0xd8bd_0000, 0x0c0b_7afc, 0x3c0f_001c, 0x1060_feff, 0x0003_7080].into_iter()), 0x5f3e_2e51_f8c2_cb1a);
	assert_eq!(hash_loose(std::iter::empty()), 0xcbf2_9ce4_8422_2325);
	assert_eq!(hash_loose([0x0c0b_7afc].into_iter()), hash_loose([0x0c00_0001].into_iter()), "a jump target does not matter");
	assert_ne!(hash_loose([0x0003_7080].into_iter()), hash_loose([0x0003_7081].into_iter()));
}

#[test]
fn spec_constants_are_the_handoffs() {
	assert_eq!((SEG.lo, SEG.hi, SEG.words, SEG.hash), (0x280000, 0x360000, 1031, 0x56e4_fe9d_6c2e_2f1f));
	assert_eq!((SEG.res_at * 4, SEG.ring_at * 4), (0xc, 0xf9c));
	assert_eq!(SEG.anchor, [0x7220_0000, 0xd8bd_0000, 0x4a20_0000, 0x3c0f_0000, 0x25ef_0000, 0x4a20_0000, 0x3c01_0000, 0]);
}

#[test]
fn lui_addiu_decodes_with_sign_extension() {
	assert_eq!(lui_addiu(0x3c0f_001c, 0x25ef_f240), Some(0x1b_f240), "Aranos: lui 0x1c, addiu -0xdc0");
	assert_eq!(lui_addiu(0x3c04_001c, 0x2484_34c0), Some(0x1c_34c0));
	assert_eq!(lui_addiu(0x3c04_001c, 0x3484_34c0), None, "ori is no addiu");
	assert_eq!(lui_addiu(0x2404_001c, 0x2484_34c0), None, "addiu is no lui");
}

#[test]
fn finder_matches_relocated_copies() {
	let t = template();
	let sp = spec_for(&t);
	let mut ram = vec![0u8; 0x20000];
	put(&mut ram, CODE_AT, &relocate(&t, 0x1111_1111, 0x4000, 0x8280));
	assert_eq!(find_with(&ram, &sp), Some(SegFn { entry: CODE_AT as u32, res: 0x4000, ring: 0x8280, matches: 1 }));

	// The same function linked at a second place (stale code of an older overlay): the lower one is returned, both are counted.
	put(&mut ram, 0x5000, &relocate(&t, 0x2222_2222, 0x6000, 0x6000 + 0x4280 - 0x2000));
	let f = find_with(&ram, &sp).unwrap();
	assert_eq!((f.entry, f.res, f.ring, f.matches), (CODE_AT as u32, 0x4000, 0x8280, 2));
}

#[test]
fn finder_ignores_immediates_but_not_other_words() {
	let t = template();
	let sp = spec_for(&t);
	let mut ram = vec![0u8; 0x20000];
	put(&mut ram, CODE_AT, &relocate(&t, 1, 0x4000, 0x8280));
	assert!(find_with(&ram, &sp).is_some());
	// lw immediate (word 12) changed: still the function.
	let at = CODE_AT + 12 * 4;
	w32(&mut ram, at, 0x8ded_0abc);
	assert!(find_with(&ram, &sp).is_some(), "an immediate is loose");
	// addu (word 13) changed: another function.
	w32(&mut ram, CODE_AT + 13 * 4, 0x0043_1822);
	assert!(find_with(&ram, &sp).is_none(), "a non-loose word decides");
}

#[test]
fn finder_rejects_bad_places_and_bad_blocks() {
	let t = template();
	let sp = spec_for(&t);
	let mut ram = vec![0u8; 0x20000];
	// Outside the scan range.
	put(&mut ram, 0xa000, &relocate(&t, 1, 0x4000, 0x8280));
	assert!(find_with(&ram, &sp).is_none());
	// Ring below the result block.
	let mut ram = vec![0u8; 0x20000];
	put(&mut ram, CODE_AT, &relocate(&t, 1, 0x8280, 0x4000));
	assert!(find_with(&ram, &sp).is_none());
	// Ring block running past RAM.
	let mut ram = vec![0u8; 0x20000];
	put(&mut ram, CODE_AT, &relocate(&t, 1, 0x4000, 0x1_f800));
	assert!(find_with(&ram, &sp).is_none());
	// RAM too small for the code to fit: no panic.
	assert!(find_with(&[0u8; 64], &sp).is_none());
}

#[test]
fn cache_scans_per_level_and_detects_changed_code() {
	let t = template();
	let sp: &'static Spec = Box::leak(Box::new(spec_for(&t)));
	let mut ram = vec![0u8; 0x20000];
	let code = relocate(&t, 5, 0x4000, 0x8280);
	put(&mut ram, CODE_AT, &code);
	let mut c = SegCache::with_spec(sp);
	let f = c.get(&ram, 7).unwrap();
	assert_eq!(c.get(&ram, 7), Some(f), "cached");
	// The overlay is replaced under the same key: the entry is dropped and the scan finds nothing.
	put(&mut ram, CODE_AT, &vec![0; WORDS]);
	assert_eq!(c.get(&ram, 7), None);
	// The function appears elsewhere: found only at the next retry (every RETRY_EVERY calls), at once for a new key.
	put(&mut ram, 0x6000, &relocate(&t, 9, 0x4400, 0x8680));
	for _ in 1..RETRY_EVERY {
		assert_eq!(c.get(&ram, 7), None);
	}
	let g = c.get(&ram, 7).unwrap();
	assert_eq!((g.entry, g.res), (0x6000, 0x4400));
	assert_eq!(c.get(&ram, 8).map(|g| g.entry), Some(0x6000), "a new level key scans at once");
}

#[test]
fn cache_drops_an_entry_whose_address_words_changed() {
	// Another overlay with the same code shape at the same key and entry, but other data addresses (the `lui` / `addiu` pairs of the
	// result block and the ring): the loose head check passes, the exact words do not.
	let t = template();
	let sp: &'static Spec = Box::leak(Box::new(spec_for(&t)));
	let mut ram = vec![0u8; 0x20000];
	put(&mut ram, CODE_AT, &relocate(&t, 5, 0x4000, 0x8280));
	let mut c = SegCache::with_spec(sp);
	assert_eq!(c.get(&ram, 7).map(|f| f.res), Some(0x4000));
	put(&mut ram, CODE_AT, &relocate(&t, 5, 0x5000, 0x9280));
	// Same salt: only the two address pairs differ. The cached entry must not be served; the rescan finds the new one.
	let first = c.get(&ram, 7);
	assert_ne!(first.map(|f| f.res), Some(0x4000), "the stale result block must not be served");
	let found = first.or_else(|| (1..=RETRY_EVERY).find_map(|_| c.get(&ram, 7)));
	assert_eq!(found.map(|f| (f.res, f.ring)), Some((0x5000, 0x9280)));
}

#[test]
fn clock_holds_the_events_off_and_gives_everything_back() {
	use ee_oracle::{cpu::Cpu, mem::Mem};
	let mut c = Cpu::new(Mem::new());
	c.icount = 1_000_000;
	c.mem.now = 999_990;
	c.cp0[12] = 0x1000_ff13; // IE and EIE set (and a few more bits)
	c.cp0[13] = 0x0000_0c00;
	c.mem.tick = 1_200_000;
	c.mem.dma_due = 1_000_500;
	c.mem.vif1_pend = Some([1, 2, 3, 4, 5]);
	c.mem.set_due(1_000_500);
	c.mem.mark_dirty();
	c.mem.clear_dirty();
	let before = Clock::read(&c);
	let status = c.cp0[12];
	let held = Clock::hold(&mut c);
	assert_eq!(held, before, "hold returns the state it found");
	assert_eq!((c.mem.due, c.mem.tick, c.mem.limit, c.mem.irq_dirty), (u64::MAX, u64::MAX, u64::MAX, false), "nothing is due by the clock");
	assert_eq!(c.cp0[12] & IRQ_ENABLE, 0, "no interrupt can be taken");
	assert_eq!(c.cp0[12], status & !IRQ_ENABLE, "other Status bits stay");
	// The probe runs: the clock moves, a kick marks the lines dirty, the stop is replaced, Cause is rewritten.
	c.icount += 5_000;
	c.mem.now = c.icount;
	c.mem.mark_dirty();
	c.mem.vif1_pend = None;
	c.mem.dma_due = 7;
	c.cp0[13] = 0;
	c.cp0[12] |= 2;
	held.release(&mut c);
	assert_eq!(Clock::read(&c), before, "icount, now, limit, due, tick, dma_due, the pending stop, dirty, Status and Cause are back");
	assert!(c.mem.limit_ok(), "the poll limit follows dirty / due again");
	assert_eq!(c.mem.vif1_pend, Some([1, 2, 3, 4, 5]), "a pending DMA stop stays pending");
}

fn real_dump(path: &str) -> Option<Vec<u8>> {
	std::fs::read(path).ok().filter(|r| r.len() == 0x200_0000)
}

#[test]
fn real_aranos_dump_has_the_function() {
	let Some(ram) = real_dump(r"E:\ratchet\rac2\runs\ram2000.bin") else {
		eprintln!("research dump not present: skipped");
		return;
	};
	assert_eq!(find(&ram), Some(SegFn { entry: 0x2dec98, res: 0x1bf240, ring: 0x1c34c0, matches: 1 }));
	assert!(prologue_ok(&ram, 0x2dec98), "the start vector's load sits 8 bytes before the finder's entry");
	assert!(!prologue_ok(&ram, 0x2deca0));
	let mut w = Wall::default();
	assert_eq!(w.collect_mobys(&ram, 0x1b1e9c), Some(0x196_4480), "hero = first record");
	assert!(w.mobys.len() > 400 && w.mobys.len() < 1000, "{}", w.mobys.len());
}

#[test]
fn real_level_dumps_have_exactly_one_function() {
	let mut seen = 0;
	for lv in 1..=26 {
		let Some(ram) = real_dump(&format!(r"E:\ratchet\rac2\runs\survey\lv{lv}\ram02400.bin")) else {
			eprintln!("level {lv}: research dump not present: skipped");
			continue;
		};
		let f = find(&ram).unwrap_or_else(|| panic!("level {lv}: not found"));
		assert_eq!((f.matches, f.ring - f.res), (1, 0x4280), "level {lv}: {f:?}");
		assert!(prologue_ok(&ram, f.entry), "level {lv}: no `pcpyld; lqc2 vf28, 0(a0)` before {:#x}", f.entry);
		seen += 1;
	}
	eprintln!("{seen} level dumps checked");
}

// ------------------------------------------------------------------------------------------------------------ restore set

struct World {
	ram: Vec<u8>,
	spr: Vec<u8>,
	hw: Vec<u8>,
}

const RES: u32 = 0x10000;
const KEY: usize = 0x20000;
const SP: u32 = 0x3f000;

fn world() -> World {
	let mut s = 0x9e37_79b9;
	let mut fill = |n: usize| (0..n).map(|_| (xorshift(&mut s) >> 8) as u8).collect::<Vec<u8>>();
	let mut w = World { ram: fill(0x40000), spr: fill(0x4000), hw: fill(0x10000) };
	// Moby table at 0x100: start (with a KSEG bit), end, spawned end: 5 records of 0x100 from 0x30000.
	w32(&mut w.ram, 0x100, 0x8003_0000);
	w32(&mut w.ram, 0x104, 0x0003_0300);
	w32(&mut w.ram, 0x108, 0x0003_0500);
	// Result block: slot buffer base inside the span, key table pointer.
	w32(&mut w.ram, RES as usize + 4, RES + 0x80);
	w32(&mut w.ram, RES as usize + 8, KEY as u32);
	w
}

fn plan(w: &World) -> Plan {
	let (_, lo) = stack_plan(SP, w.ram.len()).unwrap();
	Plan { res: RES, span: 0x5280, stack: lo, stack_len: (STACK_BELOW + STACK_ABOVE) as usize }
}

fn flip(b: &mut [u8], at: usize, n: usize) {
	for x in &mut b[at..at + n] {
		*x ^= 0xff;
	}
}

/// What a call may scribble in: every region of the restore set, plus one byte outside each RAM region (which must stay as scribbled).
fn scribble(w: &mut World, p: &Plan, mobys: &[u32]) -> Vec<usize> {
	flip(&mut w.ram, RES as usize, p.span);
	flip(&mut w.ram, KEY, KEY_BYTES);
	flip(&mut w.ram, (p.stack & 0x01ff_ffff) as usize, p.stack_len);
	for m in mobys {
		flip(&mut w.ram, *m as usize + MOBY_STAMP, 4);
		flip(&mut w.ram, *m as usize + MOBY_TOUCH, 1);
	}
	flip(&mut w.spr, 0, 0x4000);
	flip(&mut w.hw, 0, 0x10000);
	let outside = [RES as usize + p.span, KEY + KEY_BYTES, KEY - 1, (p.stack & 0x01ff_ffff) as usize - 1, mobys[0] as usize + MOBY_STAMP + 4, mobys[0] as usize + MOBY_TOUCH + 1];
	for o in outside {
		w.ram[o] ^= 0xff;
	}
	outside.to_vec()
}

#[test]
fn wall_restores_exactly_the_set() {
	let orig = world();
	let mut w = World { ram: orig.ram.clone(), spr: orig.spr.clone(), hw: orig.hw.clone() };
	let mut wall = Wall::default();
	assert_eq!(wall.collect_mobys(&w.ram, 0x100), Some(0x30000));
	assert_eq!(wall.mobys, vec![0x30000, 0x30100, 0x30200, 0x30300, 0x30400], "start to the spawned end");
	let p = plan(&w);
	assert!(wall.save(&w.ram, &w.spr, &w.hw, &p));
	let mobys = wall.mobys.clone();
	let outside = scribble(&mut w, &p, &mobys);
	assert_ne!(w.ram, orig.ram);
	wall.restore(&mut w.ram, &mut w.spr, &mut w.hw);

	assert_eq!(w.spr, orig.spr, "scratchpad");
	// The only differences left are the scribbles outside the set.
	let mut want = orig.ram.clone();
	for o in &outside {
		want[*o] ^= 0xff;
	}
	assert_eq!(diff(&w.ram, &want, 4), Diff::default(), "RAM");
	assert_eq!(w.hw, orig.hw, "the whole hardware register page comes back (D_STAT, INTC, every channel)");
	assert!(wall.saved_bytes() > 0x5280 + 0x4000);
}

#[test]
fn wall_saves_a_slot_buffer_outside_the_span_and_skips_bad_pointers() {
	let mut w = world();
	w32(&mut w.ram, RES as usize + 4, 0x28000); // slot buffer elsewhere
	w32(&mut w.ram, RES as usize + 8, 0); // no key table
	let orig = w.ram.clone();
	let mut wall = Wall::default();
	wall.collect_mobys(&w.ram, 0x100).unwrap();
	let p = plan(&w);
	assert!(wall.save(&w.ram, &w.spr, &w.hw, &p));
	flip(&mut w.ram, 0x28000, SLOT_BYTES);
	flip(&mut w.ram, 0x30, 8); // not in the set
	wall.restore(&mut w.ram, &mut w.spr, &mut w.hw);
	let d = diff(&w.ram, &orig, 4);
	assert_eq!(d.ranges, vec![(0x30, 0x38)], "the slot buffer came back, the bystander did not");

	// A key pointer past RAM is skipped, not a panic.
	w32(&mut w.ram, RES as usize + 8, 0x01ff_fff0);
	assert!(wall.save(&w.ram, &w.spr, &w.hw, &p));
	wall.restore(&mut w.ram, &mut w.spr, &mut w.hw);
}

#[test]
fn wall_restores_vu0_data() {
	let mut vu0d: Vec<u8> = (0..0x1000).map(|i| (i * 7) as u8).collect();
	let orig = vu0d.clone();
	let mut wall = Wall::default();
	wall.save_vu0(&vu0d);
	flip(&mut vu0d, 0, 0x1000);
	wall.restore_vu0(&mut vu0d);
	assert_eq!(vu0d, orig);
	let mut other = vec![0u8; 16];
	wall.restore_vu0(&mut other);
	assert_eq!(other, vec![0u8; 16], "a memory of another size is left alone");
}

#[test]
fn wall_save_refuses_a_block_outside_ram() {
	let w = world();
	let mut wall = Wall::default();
	let mut p = plan(&w);
	p.res = 0x3_f000;
	assert!(!wall.save(&w.ram, &w.spr, &w.hw, &p), "res + span past the end");
	let mut p = plan(&w);
	p.stack = 0x3_ffe0;
	assert!(!wall.save(&w.ram, &w.spr, &w.hw, &p), "stack window past the end");
}

#[test]
fn moby_table_reading_is_strict() {
	let mut w = world();
	let mut wall = Wall::default();
	assert_eq!(wall.collect_mobys(&w.ram, 0x100).map(|_| wall.mobys.len()), Some(5));
	// Spawned end below the end: the end wins.
	w32(&mut w.ram, 0x108, 0x0003_0100);
	wall.collect_mobys(&w.ram, 0x100).unwrap();
	assert_eq!(wall.mobys.len(), 3);
	// Implausible tables: empty, reversed, enormous, past RAM.
	for (s, e) in [(0u32, 0x30300u32), (0x30300, 0x30000), (0x1000, 0x1000 + 0x100 * (MAX_MOBYS + 1)), (0x3ff00, 0x40200)] {
		w32(&mut w.ram, 0x100, s);
		w32(&mut w.ram, 0x104, e);
		w32(&mut w.ram, 0x108, e);
		assert_eq!(wall.collect_mobys(&w.ram, 0x100), None, "{s:#x}..{e:#x}");
		assert!(wall.mobys.is_empty());
	}
	assert_eq!(wall.collect_mobys(&w.ram, 0x3_fffe), None, "table words past RAM");
}

#[test]
fn stack_plan_accepts_only_a_ram_stack() {
	let n = 0x40000;
	let (buf, lo) = stack_plan(SP, n).unwrap();
	assert_eq!(buf, (SP - 0x200) & !0xf);
	assert_eq!(lo, buf - STACK_BELOW);
	assert!(stack_plan(0x8003_f000, n).is_some(), "KSEG0 stack");
	assert_eq!(stack_plan(0x7000_3fc0, n), None, "scratchpad stack");
	assert_eq!(stack_plan(SP + 2, n), None, "unaligned");
	assert_eq!(stack_plan(0x1000, n), None, "too low for the window");
	assert_eq!(stack_plan(0x4_0000, n), None, "past RAM");
	assert_eq!(stack_plan(0xe000_0000, n), None, "not a RAM segment");
}

// -------------------------------------------------------------------------------------------------------------- pure parts

#[test]
fn diff_merges_close_bytes_and_caps_ranges() {
	let a = vec![0u8; 0x3000];
	let mut b = a.clone();
	for i in [5, 9, 40, 0x1000, 0x2fff] {
		b[i] = 1;
	}
	let d = diff(&a, &b, 8);
	assert_eq!(d.total, 5);
	assert_eq!(d.ranges, vec![(5, 10), (40, 41), (0x1000, 0x1001), (0x2fff, 0x3000)], "5 and 9 merge (gap under 16), 40 does not");
	let d = diff(&a, &b, 2);
	assert_eq!((d.total, d.ranges.len()), (5, 2), "ranges capped, the total is not");
	assert_eq!(diff(&a, &a, 4), Diff::default());
}

#[test]
fn point_segment_distance() {
	let (a, b) = ([0.0, 0.0, 0.0], [10.0, 0.0, 0.0]);
	assert!(point_seg_dist(a, b, [4.0, 0.0, 0.0]) < 1e-6);
	assert!((point_seg_dist(a, b, [4.0, 3.0, 0.0]) - 3.0).abs() < 1e-6);
	assert!((point_seg_dist(a, b, [13.0, 4.0, 0.0]) - 5.0).abs() < 1e-6, "beyond the end: distance to the end point");
	assert!((point_seg_dist(a, a, [3.0, 4.0, 0.0]) - 5.0).abs() < 1e-6, "a degenerate segment");
}

#[test]
fn span_is_res_to_the_end_of_the_ring() {
	let f = SegFn { entry: 0x2dec98, res: 0x1bf240, ring: 0x1c34c0, matches: 1 };
	assert_eq!(f.span(), 0x5280);
}
