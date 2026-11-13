//! [PC] Button prompts that name the keyboard keys (parity row 47, R&C1 rac1/retail/prompts.rs; the naming is the engine's
//! engine/input/glyph_text.rs, shared with R&C1).
//!
//! Retail [C, rac2/notes_menus.md 3/4]: R&C2's strings hold the same one-byte glyph codes as R&C1's (0x10 Cross .. 0x19 left stick)
//! and every string comes from one text lookup per level overlay (level 6 0x3351f0: get-string(a0 = text id) -> char*; it calls the
//! index finder (jal, level 6 0x335180) and answers [gp+OFF] + 16 * index, or the 'missing text' literal). The draw, the width
//! measure and the wrap all take that pointer, so a lookup that answers a copy with the glyphs spelled out makes retail lay out and
//! draw the key names.
//!
//! THE HAND [C, runs/tl/mhash.py over 26 RAM dumps: levels 0..9, 11..26 and the title]: the lookup is 23 words (0x5c bytes, the
//! recompiled span is the same in every level crate) that differ between levels only in the `jal` target (+0xc) and three gp
//! offsets (+0x1c, +0x3c, +0x48); `SPEC` is the level-6 copy with those masked, and it matches exactly once per level (one per
//! dump; level 10 had no dump: it binds there only if its copy is the same code). A relocatable hand (`register_reloc_hand`, as
//! modcam_hand.rs) is installed at every match. Registered hands never bind into the front end, so the title menu keeps its glyphs.
//! With a controller (or `keyprompts=off`) the hand declines and retail's lookup runs untouched. With the keyboard it runs the same
//! instructions (stack stores, the finder call, the result registers) and, for a string that holds a glyph, answers a pool copy.
//!
//! THE POOL [C]: 0xA0000..0xA8000 + its directory 0xA8000..0xB0000 (in game RAM, so quick saves keep it: see `place`): below the ELF image, zero in all 30 dumps above (cold boot, title, 26
//! levels, menus; runs/tl/zeros.py: 0x96000..0x100000 is zero in every one) and untouched over the canary run
//! (`RAC2_PROMPT_CANARY=1`: 0xA0000..0xB0000 filled with 0xA5 and checked every field against 0xA5 or the bytes this file wrote). A slot is written only when its bytes differ.
//!
//! Device (R&C1's rule, except the start): the controller (retail glyphs) until the first key, mouse button, wheel or clear mouse
//! movement, which make it the keyboard; real pad input makes it the controller again; the game thread takes the verdict once per field (`begin_field`). In a retail menu (`pc_input::menu_ctx`) a short line
//! that starts with a glyph (a legend: "\x12 Exit", "\x10 Buy", "\x14/\x15 to view other planets") names the MENU keys: confirm
//! Enter, back Esc, the other bits the Menu layer's key (Q / E for L1 / R1); sentences keep the play keys.
//! Dev: `RAC2_PROMPT_DEV=from-to:kb|pad;...` forces the device per field; `RAC2_PROMPT_LOG=1` logs each answered string.
use ee_oracle::cpu::Cpu;
use ee_oracle::hs_reloc::{Reloc, RelocHand};
use ee_oracle::mem::Mem;
use rac_engine::input::glyph_text::{self as gt, Device, MoveSum};
use rac_engine::input::kbm::{self, MenuCtx, MenuOut};
use std::borrow::Cow;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// The text lookup of level 6 (0x3351f0, 0x5c bytes) with the per-level fields masked (runs/tl/mhash.py).
const SPEC: &str = "reloc text_lookup entry 0x3351f0 len 0x5c\nmhash 0x45f8144a751aed17\nmask 0xc 0x3ffffff\nmask 0x1c 0xffff\nmask 0x3c 0xffff\nmask 0x48 0xffff\n";
/// Instructions of the lookup itself on the found path (the finder counts its own).
const EST: u64 = 16;

pub const POOL: u32 = 0xA0000;
/// Longest spelled text answered.
const TEXT_MAX: usize = 0x3ff;
/// The canary range (pool + margin).
const CANARY: std::ops::Range<u32> = 0xA0000..0xB0000;
/// A legend line in a menu: starts with a glyph and is at most this long.
const LEGEND_MAX: usize = 40;

/// Retail glyphs until the first key / mouse input (a keyboard player presses a key at the title already; runs without any input,
/// scripted pads included, stay byte-identical to retail).
static DEVICE: Device = Device::starting(false);
static FIELD: AtomicU32 = AtomicU32::new(0);

/// [b45 P4A] True when the player's last real input was the keyboard or mouse (the modern pages' hint-bar device, live).
pub fn keyboard_in_use() -> bool {
	DEVICE.keyboard_now()
}

fn reloc() -> &'static Reloc {
	static R: OnceLock<Reloc> = OnceLock::new();
	R.get_or_init(|| Reloc::parse(SPEC).expect("the text lookup spec parses"))
}

/// `keyprompts` (pc_options, default on): key names in the prompts while the keyboard is in use.
pub fn enabled() -> bool {
	static ON: OnceLock<bool> = OnceLock::new();
	*ON.get_or_init(|| !crate::options::get().extra_get("keyprompts").is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "off" | "0" | "false")))
}

/// Registers the hand (lib.rs `register`, before the recompiled table is built).
pub fn register() {
	static HAND: RelocHand = RelocHand { name: "text_lookup", reloc, est: EST, f: lookup };
	if enabled() {
		ee_oracle::rec::register_reloc_hand(&HAND);
	}
}

// ---- device (window thread notes, game thread takes) ---------------------------------------------------------------

fn moved(dist: f32) {
	static M: Mutex<Option<(Instant, MoveSum)>> = Mutex::new(None);
	let Ok(mut g) = M.lock() else { return };
	let (t0, sum) = g.get_or_insert_with(|| (Instant::now(), MoveSum::default()));
	if sum.add(t0.elapsed().as_millis() as u64 + 1000, dist) {
		DEVICE.note_keyboard();
	}
}

/// Raw mouse motion.
pub fn note_motion(dx: f64, dy: f64) {
	moved((dx * dx + dy * dy).sqrt() as f32);
}

/// Window events: key / mouse button presses and the wheel are the keyboard, cursor movement once it is clear.
pub fn note_window_event(e: &winit::event::WindowEvent) {
	use winit::event::{ElementState, WindowEvent};
	// [b46 U1] Bug A: the tap latch moved to host.rs `window_event`, after the page hosts (a press a page took is not latched).
	match e {
		WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => DEVICE.note_keyboard(),
		WindowEvent::MouseInput { state: ElementState::Pressed, .. } | WindowEvent::MouseWheel { .. } => DEVICE.note_keyboard(),
		WindowEvent::CursorMoved { position, .. } => {
			static LAST: Mutex<Option<(Instant, f64, f64)>> = Mutex::new(None);
			let Ok(mut l) = LAST.lock() else { return };
			let now = Instant::now();
			let prev = l.replace((now, position.x, position.y));
			if let Some((_, x, y)) = prev.filter(|p| now.duration_since(p.0).as_millis() as u64 <= gt::MOVE_GAP_MS) {
				moved(((position.x - x).powi(2) + (position.y - y).powi(2)).sqrt() as f32);
			}
		}
		_ => {}
	}
}

/// The controller's state of this poll (host.rs poll_input): real input makes it the device.
pub fn note_pad(now: &rac_engine::input::pad::PadState) {
	static PREV: Mutex<Option<rac_engine::input::pad::PadState>> = Mutex::new(None);
	let Ok(mut p) = PREV.lock() else { return };
	if p.as_ref().is_some_and(|prev| rac_engine::input::pad::pad_activity(prev, now)) {
		DEVICE.note_pad();
		rac_engine::cursor_vis::pad(); // [PC] a pad player's cursor stays hidden (engine/cursor_vis.rs)
	}
	*p = Some(*now);
}

fn dev_script(field: u32) -> Option<bool> {
	static S: OnceLock<String> = OnceLock::new();
	let s = S.get_or_init(|| std::env::var("RAC2_PROMPT_DEV").unwrap_or_default());
	if s.is_empty() {
		None
	} else {
		gt::scripted(s, field)
	}
}

/// Game thread, before each field: the device of this field's texts; the canary check when asked.
pub fn begin_field(mem: &mut Mem, field: u32, live: bool) {
	if !enabled() {
		return;
	}
	FIELD.store(field, Ordering::Relaxed);
	// [PC] A replay with pc.bin: the session's device of this field (pc_rec.rs), which a recorded session notes.
	let forced = crate::pc_rec::device_now().or_else(|| dev_script(field)).or(if live { None } else { Some(false) });
	if let Some(kb) = DEVICE.begin_field(forced) {
		eprintln!("rac2-play: prompts: {} prompts from field {field}", if kb { "keyboard" } else { "gamepad" });
	}
	let kb = DEVICE.keyboard_now();
	PADGEN.store(crate::pad_glyphs::stamp(), Ordering::Relaxed); // [b46 W6]
	crate::pc_rec::note_device(kb);
	sync(mem, style(kb, kb && crate::modcam_ui::modern()));
	canary(mem, field);
}

/// What the canary range must hold: 0xA5, except the bytes `place` wrote (canary runs only).
static EXPECT: Mutex<Option<Vec<u8>>> = Mutex::new(None);

/// Dev proof that nothing else writes the pool range (`RAC2_PROMPT_CANARY=1`): the whole range (pool included) is filled with
/// 0xA5 once and checked every field against 0xA5 or the bytes this file wrote; changed bytes are logged (first 8, then a count
/// every 200 fields).
fn canary(mem: &mut Mem, field: u32) {
	static ON: OnceLock<bool> = OnceLock::new();
	if !*ON.get_or_init(|| std::env::var("RAC2_PROMPT_CANARY").is_ok_and(|v| v == "1")) {
		return;
	}
	static BAD: Mutex<u32> = Mutex::new(0);
	let mut ex = EXPECT.lock().unwrap_or_else(|e| e.into_inner());
	let Some(e) = ex.as_mut() else {
		for a in CANARY {
			mem.wr8(a, 0xa5);
		}
		*ex = Some(vec![0xa5; CANARY.len()]);
		eprintln!("rac2-play: canary armed {:#x}..{:#x} at field {field}", CANARY.start, CANARY.end);
		return;
	};
	let mut bad = BAD.lock().unwrap_or_else(|e| e.into_inner());
	for a in CANARY {
		let want = e[(a - CANARY.start) as usize];
		if mem.rd8(a) != want {
			*bad += 1;
			if *bad <= 8 {
				eprintln!("rac2-play: canary: byte {a:#x} = {:#04x} (want {want:#04x}) at field {field}", mem.rd8(a));
			}
			mem.wr8(a, want);
		}
	}
	if field % 200 == 0 {
		eprintln!("rac2-play: canary: {} changed bytes so far, field {field}", *bad);
	}
}

// ---- names ---------------------------------------------------------------------------------------------------------

/// The Menu-layer key of glyph `code` on a menu screen: confirm Enter, back Esc, other bits the first key of the Menu layer that
/// presses them (L1 / R1: Q / E); None = the play key stands.
fn menu_name(code: u8, m: &MenuCtx) -> Option<Cow<'static, [u8]>> {
	let bit = gt::glyph_bit(code)?;
	if bit == m.confirm {
		return Some(Cow::Borrowed(b"Enter"));
	}
	if bit == m.back {
		return Some(Cow::Borrowed(b"Esc"));
	}
	let k = kbm::table().menu.iter().find(|k| k.out == MenuOut::Pad(bit) && matches!(k.input, kbm::Input::Key(_)))?;
	Some(Cow::Owned(kbm::names::short_name(k.input).into_bytes()))
}

/// `s` with its glyphs spelled out (`menu`: the retail menu on screen; `modern`: the Modern camera, the right stick is the mouse).
pub fn substitute(s: &[u8], menu: Option<MenuCtx>, modern: bool) -> Option<Vec<u8>> {
	let legend = !s.is_empty() && (gt::CROSS..=gt::LSTICK).contains(&s[0]) && s.len() <= LEGEND_MAX;
	match menu.filter(|_| legend) {
		Some(m) => gt::spell(s, None, |b| menu_name(b, &m).or_else(|| gt::key_name(b, modern))),
		None => gt::spell(s, None, |b| gt::key_name(b, modern)),
	}
}

// ---- the pool ------------------------------------------------------------------------------------------------------

// Retail CACHES looked-up pointers [C, canary run gcan: at the Oozla level load 59 lookups in one field (ship / map menu texts),
// none later]: so an entry keeps its address for good and never gets another text (no slot reuse), and a style change (device,
// Modern camera) rewrites every entry IN PLACE (`restyle`: the original glyph text for the pad, the spelled copy for the
// keyboard), so cached texts follow the device too. Entries are keyed by the original bytes and the menu kind, not by pointer, so
// one text shared by many levels takes one entry. A full pool (never seen: ~150 glyph texts in the game [I]) answers retail's own
// pointer (glyphs).
//
// THE TABLE LIVES IN GAME RAM (directory `DIR`, original texts next to their entries), not on the host: a quick save stores it with
// the pointers retail cached, a quick load / any state load brings both back together, so no cached pointer can ever get another
// text (a host table would start fresh after a load and hand a cached address a new text). The header's style word says which
// device's texts the entries hold; `sync` (every field) restyles when it differs from today's (a state saved with the keyboard,
// loaded with the pad). A RAM without the magic (cold boot, PCSX2 states, pad-only play) is an empty table; the pad never writes it.

const POOL_END: u32 = 0xA8000;
/// Directory: magic, count, next free pool byte, style; then `REC`-byte records (at, cap, orig_at, orig_len, menu lo, menu hi).
const DIR: u32 = 0xA8000;
const DIR_END: u32 = 0xB0000;
const MAGIC: u32 = 0x3150_4350; // "PCP1"
const REC: u32 = 32;
const MAX_RECS: u32 = (DIR_END - DIR - 0x10) / REC;
/// Room an entry keeps for longer key names after a rebind.
const SPARE: usize = 32;

/// Style word: 0 pad (glyph text), 1 keyboard, 3 keyboard with the Modern camera.
fn style(keyboard: bool, modern: bool) -> u32 {
	// [b45 P6] The bindings' hash rides in bits 8..32 (keyboard only): a rebind changes the style word, so `sync` restyles the cached entries.
	if keyboard {
		1 | (modern as u32) << 1 | binds_gen() << 8
	} else {
		// [b46 W6] C16: a controller remap shows the glyphs of the buttons now pressed (pad_glyphs.rs); 0 = retail's texts
		match PADGEN.load(Ordering::Relaxed) {
			0 => 0,
			g => 4 | g << 8,
		}
	}
}

/// [b46 W6] The remap stamp of the controller layout (pad_glyphs::stamp), taken once per field.
static PADGEN: AtomicU32 = AtomicU32::new(0);

/// A 24-bit hash of the live key and mouse bindings (C9: the prompts follow a rebind at once, as R&C1 spells at draw time).
pub fn binds_gen() -> u32 {
	use std::hash::{Hash, Hasher};
	let mut h = std::collections::hash_map::DefaultHasher::new();
	rac_engine::input::kbm::binds().0.hash(&mut h);
	(h.finish() as u32) & 0x00ff_ffff
}

fn menu_word(m: Option<MenuCtx>) -> u64 {
	m.map_or(0, |m| 1 << 63 | (m.sticks as u64) << 48 | (m.alt as u64) << 32 | (m.back as u64) << 16 | m.confirm as u64)
}

fn word_menu(w: u64) -> Option<MenuCtx> {
	(w >> 63 == 1).then(|| MenuCtx { confirm: w as u16, back: (w >> 16) as u16, alt: (w >> 32) as u16, sticks: (w >> 48) & 1 == 1 })
}

struct Rec {
	at: u32,
	cap: usize,
	orig: Vec<u8>,
	menu: Option<MenuCtx>,
}

fn read_bytes(mem: &mut Mem, at: u32, n: usize) -> Vec<u8> {
	(0..n as u32).map(|i| mem.rd8(at + i)).collect()
}

/// The records in RAM; None when the directory is missing or not sane (then nothing is answered or rewritten).
fn records(mem: &mut Mem) -> Option<Vec<Rec>> {
	if mem.rd32(DIR) != MAGIC {
		return None;
	}
	let n = mem.rd32(DIR + 4);
	if n > MAX_RECS {
		return None;
	}
	let mut v = Vec::with_capacity(n as usize);
	for i in 0..n {
		let r = DIR + 0x10 + REC * i;
		let (at, cap, oat, olen) = (mem.rd32(r), mem.rd32(r + 4), mem.rd32(r + 8), mem.rd32(r + 12));
		let menu = word_menu(mem.rd32(r + 16) as u64 | (mem.rd32(r + 20) as u64) << 32);
		let ok = |a: u32, l: u32| a >= POOL && a.checked_add(l).is_some_and(|e| e <= POOL_END);
		if !ok(at, cap) || !ok(oat, olen + 1) || cap == 0 {
			return None;
		}
		v.push(Rec { at, cap: cap as usize, orig: read_bytes(mem, oat, olen as usize), menu });
	}
	Some(v)
}

/// Per field: the entries hold the texts of `want` (restyled in place when a state load or a device change left another style).
fn sync(mem: &mut Mem, want: u32) {
	if mem.rd32(DIR) == MAGIC && mem.rd32(DIR + 12) != want {
		restyle(mem, want);
	}
}

/// The pool address for `orig` on this kind of screen with `spelled` (style `st`) written there; None when it does not fit (pool
/// full, or a rebind made it longer than the entry's room).
fn place(mem: &mut Mem, orig: &[u8], menu: Option<MenuCtx>, spelled: &[u8], st: u32) -> Option<u32> {
	if mem.rd32(DIR) != MAGIC {
		put32(mem, DIR + 4, 0);
		put32(mem, DIR + 8, POOL);
		put32(mem, DIR + 12, st);
		put32(mem, DIR, MAGIC);
	}
	let recs = records(mem)?;
	let mw = menu_word(menu);
	if let Some(r) = recs.iter().find(|r| r.orig == orig && menu_word(r.menu) == mw) {
		if spelled.len() >= r.cap {
			return None;
		}
		write_str(mem, r.at, spelled);
		return Some(r.at);
	}
	let (n, next) = (recs.len() as u32, mem.rd32(DIR + 8));
	let cap = (spelled.len().max(orig.len()) + 1 + SPARE + 15) & !15;
	let ocap = (orig.len() + 1 + 15) & !15;
	if n >= MAX_RECS || !(POOL..=POOL_END).contains(&next) || next as usize + cap + ocap > POOL_END as usize {
		return None;
	}
	let (at, oat) = (next, next + cap as u32);
	write_str(mem, oat, orig);
	write_str(mem, at, spelled);
	let r = DIR + 0x10 + REC * n;
	for (k, w) in [at, cap as u32, oat, orig.len() as u32, mw as u32, (mw >> 32) as u32].into_iter().enumerate() {
		put32(mem, r + 4 * k as u32, w);
	}
	put32(mem, DIR + 8, oat + ocap as u32);
	put32(mem, DIR + 4, n + 1);
	Some(at)
}

/// Every entry gets the text of style `st` in place (retail may hold its pointer).
fn restyle(mem: &mut Mem, st: u32) {
	let Some(recs) = records(mem) else { return };
	for e in &recs {
		let mut kb = (st & 1 != 0).then(|| substitute(&e.orig, e.menu, st & 2 != 0)).flatten().filter(|n| n.len() < e.cap);
		if st & 5 == 4 && e.menu.is_none() {
			kb = crate::pad_glyphs::remap(&e.orig, &crate::pad_glyphs::table()).filter(|n| n.len() < e.cap); // [b46 W6] C16
		}
		write_str(mem, e.at, kb.as_deref().unwrap_or(&e.orig));
	}
	put32(mem, DIR + 12, st);
}

fn put32(mem: &mut Mem, at: u32, w: u32) {
	for (i, b) in w.to_le_bytes().into_iter().enumerate() {
		put8(mem, at + i as u32, b);
	}
}

/// One byte into the pool range (the canary's expected bytes follow).
fn put8(mem: &mut Mem, at: u32, b: u8) {
	mem.wr8(at, b);
	if let Some(e) = EXPECT.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
		if CANARY.contains(&at) {
			e[(at - CANARY.start) as usize] = b;
		}
	}
}

/// Writes `s` + NUL at `at` when its bytes differ.
fn write_str(mem: &mut Mem, at: u32, s: &[u8]) {
	if !(0..=s.len()).all(|i| mem.rd8(at + i as u32) == s.get(i).copied().unwrap_or(0)) {
		for (i, b) in s.iter().chain(&[0]).enumerate() {
			put8(mem, at + i as u32, *b);
		}
	}
}

// ---- the hand ------------------------------------------------------------------------------------------------------

fn sx(v: u32) -> u64 {
	v as i32 as i64 as u64
}

/// The lookup hand (c.pc = the copy's entry). Declines (retail runs) unless the keyboard is the device; otherwise runs retail's
/// instructions: addiu sp,-0x10 / sd s0,0(sp) / sd ra,8(sp) / jal FIND (move s0,a0) / move v1,v0 / bltz v1 (lw v0,OFF(gp)) /
/// found: sll v1,4 / addu v1,v0 / lw v0,(v1); not found: v0 = [gp+OFF3] for id 0x9c40, else gp+OFF2 / ld s0 / ld ra / addiu sp.
pub fn lookup(c: &mut Cpu) -> bool {
	crate::scan_timing::timed("hook_lookup", || lookup_in(c))
}

fn lookup_in(c: &mut Cpu) -> bool {
	if !DEVICE.keyboard_now() && PADGEN.load(Ordering::Relaxed) == 0 {
		return false;
	}
	let pc = c.pc;
	let w = |c: &mut Cpu, o: u32| c.mem.rd32(pc + o);
	let (wj, wl, wa, wn) = (w(c, 0xc), w(c, 0x1c), w(c, 0x3c), w(c, 0x48));
	let find = ((wj & 0x03ff_ffff) << 2) | (pc.wrapping_add(0x10) & 0xf000_0000);
	let (off, off2, off3) = (wl as u16 as i16 as i32 as u32, wa as u16 as i16 as i32 as u32, wn as u16 as i16 as i32 as u32);
	let (ra, s0, sp0, a0) = (c.g(31), c.g(16), c.g(29), c.g(4));
	let sp = (sp0 as u32).wrapping_sub(0x10);
	c.s(29, sx(sp));
	c.mem.wr64(sp, s0);
	c.mem.wr64(sp + 8, ra);
	c.s(16, a0);
	// The finder is a leaf (level 6 0x335180: no stack, ra only returns), so `call`'s own return address changes nothing it writes.
	c.call(find);
	let idx = c.g(2);
	c.s(3, idx);
	let gp = c.g(28) as u32;
	let base = c.mem.rd32(gp.wrapping_add(off));
	c.s(2, sx(base));
	let mut ptr = 0;
	if (idx as i64) < 0 {
		let v = if a0 == 0x9c40 { c.mem.rd32(gp.wrapping_add(off3)) } else { gp.wrapping_add(off2) };
		c.s(2, sx(v));
	} else {
		let v1 = ((idx as u32) << 4).wrapping_add(base);
		c.s(3, sx(v1));
		ptr = c.mem.rd32(v1);
		c.s(2, sx(ptr));
	}
	// ld s0 / ld ra / jr ra / addiu sp,0x10 (s0 and ra come back from the stack words written above)
	c.s(16, s0);
	c.s(31, ra);
	c.s(29, sp0);
	if (0x100000..0x200_0000).contains(&ptr) {
		answer(c, ptr);
	}
	true
}

/// A found string: the pool copy with its glyphs spelled out, when it holds one.
fn answer(c: &mut Cpu, ptr: u32) {
	let mut s = Vec::with_capacity(128);
	while s.len() < 0x300 {
		let b = c.mem.rd8(ptr + s.len() as u32);
		if b == 0 {
			break;
		}
		s.push(b);
	}
	let (menu, modern) = (crate::pc_input::menu_ctx(), crate::modcam_ui::modern());
	let kb = DEVICE.keyboard_now();
	if !kb && menu.is_some() {
		return; // [b46 W6] C16: menus read the standard layout: retail's glyphs stay
	}
	let spelled = if kb { substitute(&s, menu, modern) } else { crate::pad_glyphs::remap(&s, &crate::pad_glyphs::table()) };
	let Some(n) = spelled.filter(|n| n.len() < TEXT_MAX) else { return };
	let Some(at) = place(&mut c.mem, &s, menu, &n, style(kb, modern)) else { return };
	if log_on() {
		eprintln!("rac2-play: prompts: field {} {:#x} -> {:#x} {:?}", FIELD.load(Ordering::Relaxed), ptr, at, String::from_utf8_lossy(&n));
	}
	c.s(2, sx(at));
}

fn log_on() -> bool {
	static ON: OnceLock<bool> = OnceLock::new();
	*ON.get_or_init(|| std::env::var("RAC2_PROMPT_LOG").is_ok_and(|v| v == "1"))
}

#[cfg(test)]
#[path = "pc_prompts_tests.rs"]
mod tests;
