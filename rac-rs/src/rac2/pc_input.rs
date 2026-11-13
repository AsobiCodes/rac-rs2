//! [PC] Mouse and keyboard in R&C2's retail screens and gameplay (parity rows 48, 49, 51; R&C1 `retail/menu_keys.rs`,
//! `wheel_mouse.rs`, `shipmouse.rs`). The window hands its mouse over (`motion`, `wheel`); the game thread, before each field's
//! `set_pad`, reads the game's mode and turns the mouse into pad bits / stick bytes (`Ctl::field`). Nothing is written to the
//! game: the result is the pad only, exactly like a hand on the pad, so a run without mouse input is bit-identical.
//!
//! Facts used (all [C], rac2/notes_gamefacts.md, notes_menus.md, and the runs in scratch_rc2_ctl/runs, exp.sh):
//! - Game mode word: x in 0x1a8800..0x1a9a00 with word[x+4] == 0, word[x+8] == -2, 0 <= word[x] <= 8 (0x1a8f80 in most levels;
//!   level 1's state 0013 holds it there too). 3 = page menu / pause, 4 = dialog or Quick Select, 5 = vendor.
//! - Quick Select: holding Triangle (0x10) puts mode 4 and the 8-cell wheel up; the LEFT STICK picks the cell (the pad's held
//!   word 0x1383a0 then shows the stick as d-pad bits too) and RELEASING Triangle equips it (run wh_r: stick right -> slot 42 on
//!   release, 0x18c184; stick left / up / down hovered empty cells: no change). Like R&C1's wheel, so the mouse direction becomes
//!   a full left-stick push through the pointed cell (8 cells, cell 0 up, clockwise).
//! - Starship combat (level 25 in fg1, hero state 0x18c134 == 49, group 0x18c13c == 9): the left stick steers; stick left turns
//!   left, stick UP dives (runs sh_l / sh_u vs sh_n). So mouse right / left = stick right / left, mouse up = stick down (climb), a
//!   leaky virtual stick as R&C1's ship mouse.
//!
//! Menus (mode 3 and 5): the window's Menu layer (kbm_table.rs) does the keys and the mouse buttons with the screen's confirm /
//! back from `menu_ctx`; here the mouse wheel steps the selection (Up / Down; the vendor's item row is horizontal: Left / Right,
//! [I] from its screen). Hovering a pause-root row selects it (pc_menu_hover.rs); other pages: not mapped yet.
//!
//! Options: Quick Select mouse = `wheelmouse=on` (R&C1's key, default Off) or RAC_WHEEL_MOUSE=1. Test: RAC2_MOUSE_SCRIPT=
//! `FROM-TO:m:DX,DY;F:w:N` adds DX,DY mouse counts at each field FROM..TO and N wheel notches at field F (+ = up);
//! `FROM-TO:p:FX,FY` puts the free pointer at FX,FY (fractions of the picture, pc_menu_hover.rs).
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::Mutex;

const TRIANGLE: u16 = 0x10;
const CIRCLE: u16 = 0x20;
const UP: u16 = 0x1000;
const DOWN: u16 = 0x4000;
const LEFT: u16 = 0x8000;
const RIGHT: u16 = 0x2000;
const HERO_STATE: usize = 0x18c134;
const HERO_GROUP: usize = 0x18c13c;

static MOTION: Mutex<(f32, f32)> = Mutex::new((0.0, 0.0));
static WHEEL: AtomicI32 = AtomicI32::new(0);
/// The open screen's confirm | back << 16 (0 = no retail menu screen), for the window's Menu layer.
static MENU: AtomicU32 = AtomicU32::new(0);
/// [b47 X4] In a Classic page menu (mode 3) the left click gives the pad nothing (pc_menu_hover.rs reads the button); anywhere else it stays the confirm.
static CLICK_BIT: AtomicBool = AtomicBool::new(false);
/// The Quick Select wheel or a ship fight takes the mouse this field (the Modern camera's mouse look waits).
static OWNS: AtomicBool = AtomicBool::new(false);

/// A starship fight is on this field (`Ctl::ship`): the kbm table's SHIP context (kbm_table.rs `ship_fire`).
static SHIP_ON: AtomicBool = AtomicBool::new(false);

/// Window: the kbm contexts that are on (bit `kbm_table::SHIP` in a starship fight).
pub fn contexts() -> u32 {
	(SHIP_ON.load(Ordering::Relaxed) as u32) << crate::kbm_table::SHIP
}

/// Window: raw mouse counts (only while the window has the focus).
pub fn motion(dx: f32, dy: f32) {
	if let Ok(mut m) = MOTION.lock() {
		m.0 += dx;
		m.1 += dy;
	}
}

/// Window: mouse wheel lines (+ = up).
pub fn wheel(lines: f32) {
	WHEEL.fetch_add(lines.round() as i32, Ordering::Relaxed);
}

/// Window: the Menu layer's screen (None = play keys).
pub fn menu_ctx() -> Option<rac_engine::input::kbm::MenuCtx> {
	let v = MENU.load(Ordering::Relaxed);
	// [b47 X4] alt = the left click (kbm_table.rs): nothing in a Classic page menu (pc_menu_hover.rs reads the button itself and
	// presses a Cross only on a row; build 46 sent it as pad bit 0x0004 = L1, so a controller's L1 and the Q key clicked), else the confirm.
	let alt = if CLICK_BIT.load(Ordering::Relaxed) { 0 } else { v as u16 };
	(v != 0).then(|| rac_engine::input::kbm::MenuCtx { confirm: v as u16, back: (v >> 16) as u16, alt, sticks: false })
}

/// Window: the mouse steers the wheel or the ship now (no camera mouse look, no zoom).
pub fn owns_motion() -> bool {
	OWNS.load(Ordering::Relaxed)
}

fn rd32(ram: &[u8], a: usize) -> Option<u32> {
	ram.get(a..a + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()))
}

/// The game mode word's address. Find rule [C] (all 581 fg1 savestates: exactly one of the three per-level places matches):
/// mode <= 8 and the previous mode <= 8 at +4, then -2 at +8, or 0 at +8 inside a menu (mode = previous mode, not 0).
/// Places: 0x1a8f80 (most levels), 0x1a8fc0 (level 1 and the level-25 state 0043), 0x1a8c60 (boot state 0000).
pub fn find_mode(ram: &[u8]) -> Option<usize> {
	[0x1a8f80, 0x1a8fc0, 0x1a8c60].into_iter().find(|&x| mode_rule(ram, x))
}

fn mode_rule(ram: &[u8], x: usize) -> bool {
	let (Some(m), Some(p), Some(k)) = (rd32(ram, x), rd32(ram, x + 4), rd32(ram, x + 8)) else { return false };
	m <= 8 && p <= 8 && (k == -2i32 as u32 || (k == 0 && m == p && m != 0))
}

/// The confirm / back a retail screen takes: the page menu and the vendor confirm on Cross and go back on Triangle ("\x12 Exit");
/// on the pause root too (runs mr_x: Cross opens Weapons; mr_t: Triangle closes the pause menu, mode 3 -> 0).
pub fn screen(mode: u32) -> u32 {
	match mode {
		3 | 5 => 0x40 | (TRIANGLE as u32) << 16,
		_ => 0,
	}
}

/// The full-length left-stick push (lx, ly) through cell `c` of the wheel (0 up, clockwise).
pub fn cell_stick(c: u8) -> (u8, u8) {
	[(0x80, 0x00), (0xff, 0x00), (0xff, 0x80), (0xff, 0xff), (0x80, 0xff), (0x00, 0xff), (0x00, 0x80), (0x00, 0x00)][(c & 7) as usize]
}

/// The cell the pointer (x right, y down) points at, keeping `prev` within 30 deg of its centre (no flicker on a boundary).
pub fn cell_of(x: f32, y: f32, prev: Option<u8>) -> u8 {
	let deg = x.atan2(-y).to_degrees().rem_euclid(360.0);
	if let Some(p) = prev {
		let d = (deg - p as f32 * 45.0 + 540.0).rem_euclid(360.0) - 180.0;
		if d.abs() <= 30.0 {
			return p;
		}
	}
	((deg + 22.5) / 45.0) as u8 & 7
}

/// Quick Select pointer: counts from the wheel's centre.
const REACH: f32 = 120.0;
const DEAD: f32 = 30.0;
/// Fields after the wheel opened that ignore the mouse (the jiggle of pressing the button).
const ARM: u32 = 3;
/// Ship stick: decay per field, deflection per (scaled) count.
const DECAY: f32 = 0.85;
const GAIN: f32 = 0.012;

/// [b48 R3] A quit box counts when at most this many fields outside modes 3 / 4 passed since the page menu.
/// [b49 Z2] Fact [C, field-exact pad run b49/z2/runs/boxtrace + boxopen on rc2_b48_3, RAM dumps of every field, state 0357]: the mode
/// word reads 3 up to the field before, then ONE field in which `find_mode` finds nothing (mode word 4, previous 3, the -2 / 0 marker
/// at +8 not there yet: field 561), then 4 with the dialog record {9, code, 2} from the next field (562); closing: 4 + record (640),
/// ONE unreadable field (641), then 3 without the record (642). So at most 1 field passes between the menu and the box: 4 keeps a
/// margin (was 30 = half a second, where a held Triangle of the Quick Select right after the pause closed could meet a stale record).
const BOX_AFTER_MENU: u32 = 4;

#[derive(Default)]
pub struct Ctl {
	mode_at: Option<usize>,
	/// The wheel: fields open, pointer, picked cell.
	open: u32,
	ptr: (f32, f32),
	cell: Option<u8>,
	ship: (f32, f32),
	/// Wheel notches waiting and the field of the current step (pressed 2 fields, released 2).
	steps: i32,
	step_at: u32,
	script: Vec<(u64, u64, char, f32, f32)>,
	script_read: bool,
	/// The pause root's row under the pointer (pc_menu_hover.rs).
	hover: crate::pc_menu_hover::Hover,
	/// The retail vendor strip under the pointer (pc_vendor_hover.rs).
	vhover: crate::pc_vendor_hover::VHover,
	/// [b48 Y4] The Classic Yes / No box (pc_menu_mouse_box.rs).
	boxm: crate::pc_menu_mouse_box::BoxMouse,
	/// [b48 R3] Fields since a page menu (mode 3) that were neither mode 3 nor mode 4.
	off_menu: u32,
	/// [b48 Y4] The Classic Sound page (pc_menu_mouse_sound.rs).
	sndm: crate::pc_menu_mouse_sound::SoundMouse,
	/// [b48 Y4] The Classic Galactic Map's Select / Exit plates (pc_menu_mouse_box.rs `map_answer`).
	mapm: crate::pc_menu_mouse_box::BoxMouse,
	/// [b50 V3] The Classic Galactic Map's planet list by mouse (pc_menu_mouse_map.rs).
	maplist: crate::pc_menu_mouse_map::MapList,
	/// [b49 Z2] The Classic Save page's overwrite question (pc_menu_mouse_box.rs `overwrite_answer`).
	ovm: crate::pc_menu_mouse_box::BoxMouse,
}

impl Ctl {
	/// One field: `live` = the pad is the window's (not a replay or a scripted pad; the script below works in any run).
	pub fn field(&mut self, ram: &[u8], field: u64, live: bool, b: u16, s: [u8; 4]) -> (u16, [u8; 4]) {
		let (mut dx, mut dy, mut notches) = self.script_input(field);
		let (mx, my) = MOTION.lock().map(|mut m| std::mem::take(&mut *m)).unwrap_or_default();
		let wn = WHEEL.swap(0, Ordering::Relaxed);
		if live {
			(dx, dy, notches) = (dx + mx, dy + my, notches + wn);
		}
		let mode = self.mode(ram);
		let menu = screen(mode);
		MENU.store(menu, Ordering::Relaxed);
		let classic = !crate::modern_open::modern();
		let own = (mode == 3 || classic && crate::pc_menu_mouse_title::title(ram).is_some()) && classic; // [b47 X4] a Classic page menu or the Classic title's lists read the mouse
		CLICK_BIT.store(mode == 3 && classic, Ordering::Relaxed);
		let scale = crate::options::get().mousespeed.unwrap_or(50) as f32 / 50.0;
		let (b, s, wheel) = self.quick_select(mode, b, s, dx * scale, dy * scale);
		let (s, ship) = self.ship(ram, s, dx * scale, dy * scale);
		OWNS.store(wheel || ship, Ordering::Relaxed);
		SHIP_ON.store(ship, Ordering::Relaxed); // [b45 P6]
		let b = self.menu_wheel(mode, wheel_steps_menu(menu != 0, notches, self.steps, modern_walk), notches, b);
		let page = if mode == 3 { crate::modern_open::page_id_now(ram) } else { None };
		let click = live && crate::pc_vendor_hover::left_now(); // [b47 X4] the window's left button (a pad bit no more)
		// [b48 Y4] The Classic Sound page reads the mouse itself: the pause reader gets `own = false` there (it would add a Cross to a click).
		let sound = if classic && page == Some(crate::pc_menu_mouse_sound::PAGE) { self.mode_at.and_then(|m| rd32(ram, m + 0x14c)).and_then(|mg| crate::modern_optlist_sound::read(ram, mg)) } else { None };
		// [b48 Y4] The Classic Galactic Map (page 0xf, no parent): its click is the Select / Exit plate only (the pause reader's fallback was a Cross anywhere).
		let mapv = if classic && page == Some(crate::modern_map::MAP_PAGE) { crate::modern_map::read(ram, &|_| None) } else { None };
		let map = mapv.is_some();
		// [b49 Z2] The overwrite question over a Classic Save page: a click is Circle (Yes) / Triangle (No) on its words only.
		let over = classic && mode == 3 && page == Some(crate::modern_load::BOX_ID) && crate::pc_menu_mouse_box::overwrite_up(ram);
		let hovered = self.hover.field(ram, self.mode_at, mode, page, field, live, own && sound.is_none() && !map && !over, click, b);
		// [b47 X4] U5: a Cross the mouse added (after the page filters ran) on the Camera row opens the PC page, not retail's Camera page.
		let b = if own && hovered & 0x40 != 0 && b & 0x40 == 0 { crate::modern_open::door_after(ram, hovered) } else { hovered };
		let b = self.sndm.field(sound, self.hover.last(), click, b);
		let b = self.maplist.field(mapv.map(|m| (m.cur, m.entries.len())), self.hover.last(), click, b); // [b50 V3] before the plates: their Cross must stay
		let b = self.mapm.field_with(map, click, self.hover.last(), b, crate::pc_menu_mouse_box::map_answer);
		let b = self.ovm.field_yes(over, click, self.hover.last(), b, crate::pc_menu_mouse_box::overwrite_answer, CIRCLE);
		// [b48 Y4] Classic: the quit box's Yes / No by click (it has no cursor).
		// [b48 R3] Only a box that came up out of a page menu: mode 4 is also the Quick Select. Fields outside modes 3 / 4 are
		// counted (the mode word is unreadable for a field or two while the box opens: build 48's 2nd exe asked for mode 3 the
		// field before and never saw the box, tour run b48_box_no).
		self.off_menu = match mode {
			3 => 0,
			4 => self.off_menu,
			_ => self.off_menu.saturating_add(1),
		};
		let up = classic && self.off_menu <= BOX_AFTER_MENU && self.mode_at.is_some_and(|m| crate::modern_optlist_quit::up(ram, m));
		let b = self.boxm.field(up, click, self.hover.last(), b);
		(self.vhover.field(ram, field, live, b), s)
	}

	fn mode(&mut self, ram: &[u8]) -> u32 {
		if !self.mode_at.is_some_and(|x| mode_rule(ram, x)) {
			self.mode_at = find_mode(ram);
		}
		self.mode_at.and_then(|x| rd32(ram, x)).unwrap_or(0)
	}

	fn quick_select(&mut self, mode: u32, b: u16, mut s: [u8; 4], dx: f32, dy: f32) -> (u16, [u8; 4], bool) {
		if !(wheel_mouse() && mode == 4 && b & TRIANGLE != 0) {
			(self.open, self.ptr, self.cell) = (0, (0.0, 0.0), None);
			return (b, s, false);
		}
		self.open += 1;
		if self.open <= ARM {
			return (b, s, true);
		}
		let (x, y) = (self.ptr.0 + dx, self.ptr.1 + dy);
		let len = (x * x + y * y).sqrt();
		self.ptr = if len > REACH { (x * REACH / len, y * REACH / len) } else { (x, y) };
		if len > DEAD {
			let c = cell_of(self.ptr.0, self.ptr.1, self.cell);
			if self.cell != Some(c) && std::env::var_os("RAC2_MOUSE_SCRIPT").is_some() {
				eprintln!("pc_input: quick select cell {c}");
			}
			self.cell = Some(c);
			(s[2], s[3]) = cell_stick(c);
		}
		(b, s, true)
	}

	fn ship(&mut self, ram: &[u8], mut s: [u8; 4], dx: f32, dy: f32) -> ([u8; 4], bool) {
		let fight = rd32(ram, HERO_STATE) == Some(49) && rd32(ram, HERO_GROUP) == Some(9);
		if !fight {
			self.ship = (0.0, 0.0);
			return (s, false);
		}
		// [b45 P5] The Mouse up/down row (modcam_mouseinv.rs): Inverted flips the ship mouse's vertical axis.
		let dy = if crate::modcam_mouseinv::ship_ud_inverted() { -dy } else { dy };
		self.ship = ((self.ship.0 * DECAY + dx * GAIN).clamp(-1.0, 1.0), (self.ship.1 * DECAY - dy * GAIN).clamp(-1.0, 1.0));
		if self.ship.0.abs() < 1e-3 && self.ship.1.abs() < 1e-3 {
			self.ship = (0.0, 0.0);
			return (s, true);
		}
		// Mouse up (dy < 0) = climb = stick DOWN (ly above 0x80): the -dy above makes up positive, so ly = 0x80 + v.
		let add = |byte: u8, v: f32| (byte as f32 + v * 127.0).round().clamp(0.0, 255.0) as u8;
		s[2] = add(s[2], self.ship.0);
		s[3] = add(s[3], self.ship.1);
		(s, true)
	}

	fn menu_wheel(&mut self, mode: u32, menu: bool, notches: i32, b: u16) -> u16 {
		if !menu {
			(self.steps, self.step_at) = (0, 0);
			return b;
		}
		self.steps = (self.steps + notches).clamp(-4, 4);
		if self.steps == 0 {
			self.step_at = 0;
			return b;
		}
		let dir = self.steps.signum();
		self.step_at += 1;
		let press = self.step_at <= 2;
		if self.step_at >= 4 {
			self.steps -= dir;
			self.step_at = 0;
		}
		let (up, down) = if mode == 5 { (LEFT, RIGHT) } else { (UP, DOWN) };
		if press { b | if dir > 0 { up } else { down } } else { b }
	}

	fn script_input(&mut self, field: u64) -> (f32, f32, i32) {
		if !self.script_read {
			self.script_read = true;
			self.script = std::env::var("RAC2_MOUSE_SCRIPT").map(|v| parse_script(&v)).unwrap_or_default();
		}
		self.script.iter().filter(|e| (e.0..=e.1).contains(&field)).fold((0.0, 0.0, 0), |(x, y, n), e| match e.2 {
			'm' => (x + e.3, y + e.4, n),
			'w' => (x, y, n + e.3 as i32),
			_ => (x, y, n),
		})
	}
}

/// A modern page drives the menu with its own pad pulses (its filter masked the menu buttons before `Ctl::field`): a wheel press
/// OR-ed into that walk would land the cursor on the wrong cell.
fn modern_walk() -> bool {
	crate::modern_grid::facts().is_some()
		|| crate::modern_items::facts().is_some()
		|| crate::modern_qs::facts().is_some()
		|| crate::modern_help::facts().is_some()
		|| crate::modern_map::map().is_some()
		|| crate::modern_shack::shop().is_some()
}

/// The wheel steps the menu: a retail menu is up and no modern page walks it (asked only while the wheel has notches to give).
fn wheel_steps_menu(menu: bool, notches: i32, steps: i32, walk: impl FnOnce() -> bool) -> bool {
	menu && (notches == 0 && steps == 0 || !walk())
}

/// `FROM-TO:m:DX,DY` / `F:w:N` entries, `;`-separated.
pub fn parse_script(v: &str) -> Vec<(u64, u64, char, f32, f32)> {
	v.split(';')
		.filter_map(|e| {
			let mut p = e.trim().splitn(3, ':');
			let (r, k, a) = (p.next()?, p.next()?.chars().next()?, p.next()?);
			let (from, to) = r.split_once('-').map_or((r, r), |x| x);
			let (x, y) = a.split_once(',').unwrap_or((a, "0"));
			Some((from.parse().ok()?, to.parse().ok()?, k, x.trim().parse().ok()?, y.trim().parse().ok()?))
		})
		.collect()
}

/// The Quick Select mouse row (`wheelmouse=on`, R&C1's key; default Off) or RAC_WHEEL_MOUSE=1.
pub fn wheel_mouse() -> bool {
	static ENV: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
	*ENV.get_or_init(|| std::env::var("RAC_WHEEL_MOUSE").is_ok_and(|v| v == "1"))
		|| crate::options::get().extra_get("wheelmouse").is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "on" | "1" | "true"))
}

#[cfg(test)]
#[path = "pc_input_tests.rs"]
mod tests;
