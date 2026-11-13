//! [PC] Instant mouse moves on R&C2's modern pause pages (lane nav, build 44; user report b43: "input lag in the modern pause pages").
//! The mouse used to walk the game's cursor one D-pad pulse per cell (2 fields pressed + 2 released: 4 fields per cell, 16 for four
//! cells). Now a page's walk writes the game's cursor words to the cell NEXT to the target and presses ONE D-pad edge in the same
//! field: retail makes the last step itself (its move sound, its 3D preview reload, its name / ammo texts), so any distance takes one
//! field. Safe because the cursor state is just those words [C: RAM of the Weapons page across two Right presses (lane nav
//! HANDOFF.md): the only word stepping is the column; besides it only the page's text buffers change]. RAM is written ONLY while a
//! mouse request is pending (a walk queues, `apply` writes before the field runs): without the mouse the game is byte-identical.
//! A walk whose page has no preset for a move (another group, a cell with no neighbour) presses as before, one cell per pulse.
use crate::modern_screen::{DOWN, LEFT, RIGHT, UP};
use std::sync::Mutex;

static WRITES: Mutex<Vec<(u32, u32)>> = Mutex::new(Vec::new());

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
	m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Game thread, from a page's walk while a mouse request is pending: write `v` to the RAM word `addr` before this field runs.
pub fn queue(addr: u32, v: u32) {
	lock(&WRITES).push((addr, v));
}

/// Game thread, once per field right after the pages' filters (host/game.rs): makes the queued writes (none queued = no write) and
/// ages a pending tab switch (modern_tabs.rs).
pub fn apply(ram: &mut [u8]) {
	crate::modern_tabs::tick();
	let mut w = lock(&WRITES);
	// [PC] A replay with pc.bin: the recorded writes, not the window's (pc_rec.rs). A recorded session notes each one.
	if crate::pc_rec::replaying() {
		w.clear();
		crate::pc_rec::apply_ram(crate::pc_rec::Stage::Jump, ram);
		return;
	}
	for (a, v) in w.drain(..) {
		let a = (a & 0x1ff_fffc) as usize;
		if let Some(b) = ram.get_mut(a..a + 4) {
			b.copy_from_slice(&v.to_le_bytes());
			crate::pc_rec::note_write(crate::pc_rec::Stage::Jump, a as u32, &v.to_le_bytes());
		}
	}
}

/// The cell / row a mouse request is taking a page's cursor to: the window draws it selected in the same display frame (`get`),
/// the request sets it, the page's filter keeps it in step with its walk (None once the walk is over: the game's cursor is there).
pub struct Pend<T: Copy>(Mutex<Option<T>>);

impl<T: Copy> Pend<T> {
	pub const fn new() -> Self {
		Pend(Mutex::new(None))
	}

	pub fn get(&self) -> Option<T> {
		*lock(&self.0)
	}

	pub fn set(&self, v: Option<T>) {
		*lock(&self.0) = v;
	}
}

/// The pause pages' menu manager of this field (`modern_screen::manager`), 0 = none: the base of every page's cursor words.
pub fn manager(ram: &[u8]) -> u32 {
	crate::modern_open::snap_now(ram).map_or(0, |s| crate::modern_screen::manager(ram, &s))
}

/// `base + off` when there is a manager, else 0 (= the walk presses only).
pub fn at(base: u32, off: u32) -> u32 {
	if base == 0 { 0 } else { base.wrapping_add(off) }
}

/// The first candidate that is the cursor itself (no write needed), else the first one. Pure.
fn prefer(c: &[(usize, u16)], cur: Option<usize>) -> Option<(usize, u16)> {
	c.iter().find(|x| Some(x.0) == cur).or(c.first()).copied()
}

/// The cell next to `to` in a grid of `cols` columns and `n` cells from which ONE press reaches `to`, and that press: the left
/// neighbour + Right, the right one + Left, the cell above + Down, the cell below + Up (never across a wrap). `ok(k)` = k is a cell
/// (not a hole); `cur` (the game's cell in this group) wins when it is one of them. Pure.
pub fn grid_preset(cols: usize, n: usize, cur: Option<usize>, to: usize, ok: impl Fn(usize) -> bool) -> Option<(usize, u16)> {
	let cols = cols.max(1);
	let (c, r) = (to % cols, to / cols);
	let all = [
		(c > 0).then(|| (to - 1, RIGHT)),
		(c + 1 < cols && to + 1 < n).then(|| (to + 1, LEFT)),
		(r > 0).then(|| (to - cols, DOWN)),
		(to + cols < n).then(|| (to + cols, UP)),
	];
	let c: Vec<(usize, u16)> = all.into_iter().flatten().filter(|&(k, _)| k < n && ok(k)).collect();
	prefer(&c, cur)
}

/// The same for a list of `n` rows (the pause root, Help): the row above + Down, else the row below + Up. Pure.
pub fn list_preset(n: usize, cur: usize, to: usize) -> Option<(usize, u16)> {
	let c: Vec<(usize, u16)> = [(to > 0).then(|| (to.wrapping_sub(1), DOWN)), (to + 1 < n).then(|| (to + 1, UP))].into_iter().flatten().collect();
	prefer(&c, Some(cur))
}

/// The Items page (modern_items.rs `step`, the retail wrapping between its four groups): a cell (group, index) from which ONE press
/// reaches `to`, and the press; the game's cell, then a cell of `to`'s group, then any. Pure.
pub fn items_preset(cur: (usize, usize), to: (usize, usize)) -> Option<((usize, usize), u16)> {
	use crate::modern_items::{step, COLS, GROUP_LEN};
	let pos = |g: usize, k: usize| (g, k % COLS[g], k / COLS[g]);
	if to.0 > 3 || to.1 >= GROUP_LEN[to.0] {
		return None;
	}
	let goal = pos(to.0, to.1);
	let mut c = Vec::new();
	for g in 0..4 {
		for k in 0..GROUP_LEN[g] {
			for d in [RIGHT, LEFT, DOWN, UP] {
				if (g, k) != to && step(pos(g, k), d) == goal {
					c.push(((g, k), d));
				}
			}
		}
	}
	c.iter().find(|x| x.0 == cur).or_else(|| c.iter().find(|x| x.0 .0 == to.0)).or(c.first()).copied()
}

#[cfg(test)]
#[path = "modern_jump_tests.rs"]
mod tests;
