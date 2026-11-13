use super::*;
use ee_oracle::cpu::Cpu;
use ee_oracle::mem::Mem;

/// Level 6: the index finder 0x335180 (28 words) then the lookup 0x3351f0 (23 words) [C, survey/lv6 dump].
const LV6: [u32; 51] = [
	0x3c020028, 0x2406ffff, 0x0040402d, 0x24427b48, 0x8c43002c, 0x18600013, 0x0000282d, 0x3c03001b, 0x8c632580, 0x8c620004, 0x14440003, 0x0060382d, 0x1000000c, 0x0000302d, 0x25037b48, 0x24a50001,
	0x8c62002c, 0x00a2102a, 0x10400006, 0x00051100, 0x00471021, 0x8c430004, 0x1464fff8, 0x25037b48, 0x00a0302d, 0x03e00008, 0x00c0102d, 0x00000000, 0x27bdfff0, 0xffb00000, 0xffbf0008, 0x0c0cd460,
	0x0080802d, 0x0040182d, 0x04600005, 0x8f823510, 0x00031900, 0x00621821, 0x10000008, 0x8c620000, 0x34029c40, 0x12020004, 0xdfb00000, 0x2782a088, 0x10000004, 0xdfbf0008, 0x8f82a078, 0xdfb00000,
	0xdfbf0008, 0x03e00008, 0x27bd0010,
];
/// Aranos (level 0) 0x2f5cc8: the same lookup with another finder and gp offset.
const LV0: [u32; 23] = [
	0x27bdfff0, 0xffb00000, 0xffbf0008, 0x0c0bd716, 0x0080802d, 0x0040182d, 0x04600005, 0x8f822b10, 0x00031900, 0x00621821, 0x10000008, 0x8c620000, 0x34029c40, 0x12020004, 0xdfb00000, 0x2782a088,
	0x10000004, 0xdfbf0008, 0x8f82a078, 0xdfb00000, 0xdfbf0008, 0x03e00008, 0x27bd0010,
];
const ENTRY: u32 = 0x3351f0;
const GP: u32 = 0x1af070;
const RECS: u32 = 0x1c579c0;

fn put(m: &mut Mem, at: u32, w: &[u32]) {
	for (i, &x) in w.iter().enumerate() {
		m.wr32(at + 4 * i as u32, x);
	}
}

/// A level-6 machine: code, the bank (count, table pointer, [gp+0x3510]) and three strings.
fn machine() -> Cpu {
	let mut m = Mem::new();
	put(&mut m, 0x335180, &LV6);
	let texts: [(u32, &[u8]); 3] = [(0xb60, b"\x12 Exit"), (0xb61, b"Press \x15 for help"), (0xb62, b"Plain")];
	m.wr32(0x287b48 + 0x2c, texts.len() as u32);
	m.wr32(0x1b2580, RECS);
	m.wr32(GP + 0x3510, RECS);
	m.wr32(GP.wrapping_sub(0x5f88), 0x1d0000);
	for (i, (id, s)) in texts.iter().enumerate() {
		let at = 0x1c60000 + 0x100 * i as u32;
		m.wr32(RECS + 16 * i as u32, at);
		m.wr32(RECS + 16 * i as u32 + 4, *id);
		for (k, &b) in s.iter().enumerate() {
			m.wr8(at + k as u32, b);
		}
	}
	let mut c = Cpu::new(m);
	c.s(28, GP as u64);
	c.s(29, 0x1ff0000);
	c.s(31, 0x123456);
	c.s(16, 0x7777);
	c
}

/// Retail's own function (interpreted) and the hand on the same id: registers and stack words.
fn both(id: u32) -> ([u64; 6], [u64; 6], Cpu) {
	let regs = |c: &mut Cpu| [c.g(2), c.g(3), c.g(16), c.g(29), c.g(31), c.mem.rd32(0x1ff0000 - 0x10) as u64 | (c.mem.rd32(0x1ff0000 - 8) as u64) << 32];
	let mut a = machine();
	a.s(4, id as u64);
	a.call(ENTRY);
	// `call` enters with its sentinel in ra, which retail stores and restores: the hand gets the same ra.
	let ra_ = regs(&mut a);
	let mut b = machine();
	b.s(31, a.g(31));
	b.s(4, id as u64);
	b.pc = ENTRY;
	assert!(lookup(&mut b));
	(ra_, regs(&mut b), b)
}

#[test]
fn the_spec_matches_every_level_copy_and_nothing_else() {
	let r = reloc();
	assert_eq!((r.entry, r.len), (ENTRY, 0x5c));
	let mut m = Mem::new();
	put(&mut m, 0x335180, &LV6);
	put(&mut m, 0x2f5cc8, &LV0);
	assert!(r.matches(&m, ENTRY));
	assert!(r.matches(&m, 0x2f5cc8), "Aranos' copy (other jal and gp offset)");
	assert!(!r.matches(&m, 0x335180));
	// The research dumps when present: exactly one copy per level (runs/tl/mhash.py did the same over 26 dumps).
	for lv in [2, 6, 11, 25] {
		let Ok(ram) = std::fs::read(format!("E:/ratchet/rac2/runs/survey/lv{lv}/ram02400.bin")) else { continue };
		let mut m = Mem::new();
		let n = ram.len().min(m.ram.len());
		m.ram[..n].copy_from_slice(&ram[..n]);
		let hits: Vec<u32> = (0x100000u32..0x600000).step_by(4).filter(|&a| m.rd32(a) == 0x27bd_fff0 && r.matches(&m, a)).collect();
		assert_eq!(hits.len(), 1, "level {lv}: {hits:x?}");
	}
}

#[test]
fn the_hand_runs_retails_lookup_and_answers_a_spelled_copy() {
	rac_engine::input::kbm::ensure_table(&crate::kbm_table::TABLE);
	// A controller: the hand declines, retail runs.
	DEVICE.begin_field(Some(false));
	let mut c = machine();
	c.pc = ENTRY;
	c.s(4, 0xb60);
	assert!(!lookup(&mut c));
	DEVICE.begin_field(Some(true));
	// No glyph, a missing id and the 0x9c40 case: exactly retail's registers and stack words.
	for id in [0xb62, 0x999, 0x9c40] {
		let (a, b, _) = both(id);
		assert_eq!(a, b, "id {id:#x}");
	}
	// A glyph string: the same registers but v0, which points at the pool copy.
	let (a, b, mut c) = both(0xb61);
	assert_eq!(a[1..], b[1..]);
	assert_eq!(a[0], 0x1c60100);
	assert!((POOL..POOL_END).contains(&(b[0] as u32)));
	let mut got = vec![];
	while c.mem.rd8(b[0] as u32 + got.len() as u32) != 0 {
		got.push(c.mem.rd8(b[0] as u32 + got.len() as u32));
	}
	assert_eq!(Some(got.clone()), substitute(b"Press \x15 for help", None, false));
	assert!(!gt::has_glyph(&got), "{:?}", String::from_utf8_lossy(&got));
	// Asked again: the same slot, nothing rewritten.
	let (_, b2, _) = both(0xb61);
	assert_eq!(b2[0], b[0]);
	// Another text (same machine) never shares its address; a switch to the pad puts the original glyph text back in place
	// (retail may cache the pointer), the keyboard spells it again.
	let at = b[0] as u32;
	c.pc = ENTRY;
	c.s(4, 0xb60);
	assert!(lookup(&mut c));
	assert_ne!(c.g(2) as u32, at);
	assert_eq!(text(&mut c.mem, at), got);
	restyle(&mut c.mem, style(false, false));
	assert_eq!(text(&mut c.mem, at), b"Press  for help");
	restyle(&mut c.mem, style(true, false));
	assert_eq!(text(&mut c.mem, at), got);
}

fn text(m: &mut Mem, at: u32) -> Vec<u8> {
	(0..).map(|i| m.rd8(at + i)).take_while(|&x| x != 0).collect()
}

/// RAM 0xA0000..0xB0000 of `from` into `to` (what a quick save stores and a quick load restores).
fn copy_pool(from: &mut Mem, to: &mut Mem) {
	for a in POOL..DIR_END {
		to.wr8(a, from.rd8(a));
	}
}

#[test]
fn a_quick_load_keeps_every_cached_pointer_on_its_text() {
	let (kb, pad) = (style(true, false), style(false, false));
	let mut m = Mem::new();
	let sp = |s: &[u8]| substitute(s, None, false).unwrap();
	let a = place(&mut m, b" Exit", None, &sp(b" Exit"), kb).unwrap();
	// Quick save here (retail holds pointer `a`), then more texts and a quick load into a fresh process (no host state at all).
	let mut saved = Mem::new();
	copy_pool(&mut m, &mut saved);
	let b = place(&mut m, b"Press  for help", None, &sp(b"Press  for help"), kb).unwrap();
	assert_ne!(a, b);
	let mut loaded = Mem::new();
	copy_pool(&mut saved, &mut loaded);
	// The same text again: the same address; a new text: a new address, never `a`.
	assert_eq!(place(&mut loaded, b" Exit", None, &sp(b" Exit"), kb), Some(a));
	let c = place(&mut loaded, b" Help", None, &sp(b" Help"), kb).unwrap();
	assert_ne!(c, a);
	assert_eq!(text(&mut loaded, a), sp(b" Exit"));
	// A menu legend is another entry than the same bytes in play.
	let mc = MenuCtx { confirm: 0x40, back: 0x10, alt: 0, sticks: false };
	let d = place(&mut loaded, b" Exit", Some(mc), b"Esc Exit", kb).unwrap();
	assert!(d != a && d != c);
	// Saved with the keyboard, loaded with the pad: the per-field sync puts the glyph texts back; with the keyboard again, spelled.
	sync(&mut loaded, pad);
	assert_eq!(text(&mut loaded, a), b" Exit");
	assert_eq!(text(&mut loaded, d), b" Exit");
	sync(&mut loaded, kb);
	assert_eq!(text(&mut loaded, a), sp(b" Exit"));
	assert_eq!(text(&mut loaded, d), b"Esc Exit");
	// A RAM without the directory (cold boot, PCSX2 state, pad-only play): sync writes nothing.
	let mut cold = Mem::new();
	sync(&mut cold, pad);
	sync(&mut cold, kb);
	assert!((POOL..DIR_END).all(|x| cold.rd8(x) == 0));
	// A full pool answers retail's pointer (None), never an address in use.
	let long = vec![b'x'; 900];
	let mut n = 0;
	while place(&mut loaded, &[&long[..], &[0x10, n as u8 + 1]].concat(), None, &long, kb).is_some() {
		n += 1;
	}
	assert!(n > 5 && n < 40, "{n}");
	assert_eq!(place(&mut loaded, b" Exit", None, &sp(b" Exit"), kb), Some(a));
}

#[test]
fn menu_legends_name_the_menu_keys_and_sentences_the_play_keys() {
	rac_engine::input::kbm::ensure_table(&crate::kbm_table::TABLE);
	let m = MenuCtx { confirm: 0x40, back: 0x10, alt: 0, sticks: false };
	assert_eq!(substitute(b"\x12 Exit", Some(m), false).unwrap(), b"Esc Exit");
	assert_eq!(substitute(b"\x10 Buy", Some(m), false).unwrap(), b"Enter Buy");
	assert_eq!(substitute(b"\x14/\x15 to view other planets", Some(m), false).unwrap(), b"Q/E to view other planets");
	// Outside menus and in sentences: the play keys (the generic table's).
	let play = substitute(b"\x12 Exit", None, false).unwrap();
	assert_ne!(play, b"Esc Exit");
	assert!(!gt::has_glyph(&play));
	assert_eq!(substitute(b"Use \x18 to look", None, true).unwrap(), b"Use Mouse to look");
	assert_eq!(substitute(b"Plain", Some(m), false), None);
}
