//! [b50 V7] R2 "textures are still artifacting": the RAM classes' GS textures (gsfull::texreq, keyed by the TEX0 word) are kept
//! fresh for the classes the game DRAWS, and a cheap self-check names a stale texture in the player's log.
//!
//! The hole (proven on rc2_b49_1.exe, b50\handoff_V7.md): texreq reads a wanted word after the field when no draw used it for 3
//! fields (an off-screen class: VRAM there holds whatever the game's texture ring parked, 99.9 % foreign texels in the user's
//! level-14 save), backs off its re-reads while the class stays undrawn (up to 240 fields) and FORGETS it after 12 undrawn re-reads
//! (`prune`, ~36 s). Nothing asked for a forgotten word again (scene.rs wants a class's words only when it decodes), so the pack kept
//! drawing the last texels it got - foreign ones after an after-field read - for good, while the class sat unchanged in RAM.
//! The fix: each field the words of the classes that are drawn now (`Moby::cut` None) and were not drawn the field before are asked
//! for again (`texreq::want_prog`: a held word's back-off ends, so the game's upload before the draw is re-read at that draw; a
//! forgotten word is read again at its next draw), and a few drawn classes per field in rotation (a class drawn for good is checked
//! too). The self-check (one `rac2: texture stale` line per word and cache, RAC_RUN_LOG lands it in player\rac2\log.txt):
//! - `texreq-unheld`: a drawn class's word texreq has not held for UNHELD_FIELDS fields in a row (asked for, never read).
//! - `pack-copy`: the pack's copy of a key differs from texreq's texels of the same version (sampled texels, one key per few fields).
//! - `texreq-churn`: a drawn class's word changed version in CHURN_MIN of the last 8 fields (two textures under one TEX0 word in one
//!   field, or a texture the game animates in place).
//! - `pack-key` (props.rs `ram_key`): the 16-bit key counter of a level ran out (two words would share one key).

use crate::scene::{RamClass, Scene};
use rac_formats::level::Texture;
use std::collections::{HashMap, HashSet};

/// The VU1 program of the moby draws (scene.rs `MOBY_PROG`): the program whose draws texreq reads a class's words at.
const MOBY_PROG: u8 = 11;
/// Drawn classes whose words are asked for again per field in rotation.
const SWEEP: usize = 4;
/// [b51 G3] Dropped undrawn refresh wants of a drawn class's after-field word before it is shown anyway (review b50 M1).
const SHOW_AFTER_MISSES: u8 = 3;
/// [b51 review M1] Fields in a row a class must be drawn with its word unshown before the after-field texels are released
/// (more than 3 x texreq's WANT_FIELDS: a draw of the class had its chances to read the word).
const RELEASE_FIELDS: u32 = 12;
/// Fields a drawn class's word may stay unheld before the self-check names it.
const UNHELD_FIELDS: u32 = 120;
/// Changed versions in the last 8 fields that count as churn.
const CHURN_MIN: u32 = 6;
/// Fields between two pack-copy checks, and texels compared per check.
const COPY_EVERY: u32 = 4;
const COPY_SAMPLES: usize = 256;

#[derive(Default)]
pub struct Guard {
	/// Class ids drawn the field before, and the buffers of this field (swapped, never reallocated per call).
	drawn: HashSet<u32>,
	now: HashSet<u32>,
	words: HashSet<u64>,
	list: Vec<usize>,
	/// [b51 G3] The game field the last pass ran for: `sync` runs per REDRAW (100+ a second), the guard once per game field.
	last_field: Option<u64>,
	/// Rotation cursor over the drawn classes (`SWEEP`) and over the pack's keys (`COPY_EVERY`).
	sweep: usize,
	copy: usize,
	tick: u32,
	/// Per drawn word: fields in a row unheld, the last version seen and the churn bits (bit 0 = this field changed).
	unheld: HashMap<u64, u32>,
	seen: HashMap<u64, (u32, u8)>,
	/// (word, cache) already logged.
	logged: HashSet<(u64, &'static str)>,
}

impl Guard {
	/// Once per GAME field after the pack refreshed its GS textures (props.rs `Classes::sync`, called every redraw: a redraw of the
	/// same field returns at once). `ram_tex`: TEX0 -> (key, version) of the pack, `tex`: its texels by key, `screen`: the pack blanks
	/// this word's level 0 (props.rs `level0_screen`: not compared).
	pub fn field(&mut self, scene: &Scene, ram_tex: &HashMap<u64, (u32, u32)>, tex: &HashMap<u32, Texture>, screen: impl Fn(u64) -> bool) {
		use crate::gsfull::texreq;
		if self.last_field == Some(scene.field) {
			return;
		}
		self.last_field = Some(scene.field);
		self.now.clear();
		self.now.extend(scene.mobies.iter().filter(|m| m.cut.is_none()).flat_map(|m| crate::props::draw_ids(m.key, m.mask)));
		self.list.clear();
		let now = &self.now;
		self.list.extend(scene.ram_classes.iter().enumerate().filter(|(_, e)| now.contains(&e.0) && !e.1 .1.is_empty()).map(|(i, _)| i));
		let n = self.list.len();
		for (i, &ix) in self.list.iter().enumerate() {
			let e = &scene.ram_classes[ix];
			let swept = n > 0 && (i + n - self.sweep % n) % n < SWEEP;
			if swept || !self.drawn.contains(&e.0) {
				e.1 .1.iter().for_each(|&t| texreq::want_prog(t, MOBY_PROG));
			}
		}
		self.sweep = self.sweep.wrapping_add(SWEEP);
		self.words.clear();
		self.words.extend(self.list.iter().flat_map(|&ix| scene.ram_classes[ix].1 .1.iter().copied()));
		let f = texreq::field();
		let words = std::mem::take(&mut self.words);
		for &t in &words {
			// [b51 review M2] Held = shown by texreq, decided from texreq alone (the pack's own copy no longer counts: a word it had
			// keyed once never reached `show_after` again, and its class froze on its last mesh).
			let held = texreq::maybe(t) && texreq::get_versioned(t).is_some();
			let c = self.unheld.entry(t).or_insert(0);
			// A paused game (pause menu up) does not count: nothing is drawn, so nothing is read.
			*c = if scene.paused { *c } else { unheld_step(*c, held) };
			// [b51 G3] review b50 M1: only ever read after a field, but the class is drawn: show those texels. [b51 review M1] Only
			// once the class was DRAWN unshown for RELEASE_FIELDS fields in a row (`unheld` is dropped while it is not drawn): the
			// dropped wants also pile up off-screen, and a class coming back into view gets its read at a draw first.
			if release_due(*c, held) && texreq::show_after(t, SHOW_AFTER_MISSES) {
				*c = 0;
			}
			// The pack still draws texels it got earlier for this word: not named (the log stays quiet for it).
			if *c == UNHELD_FIELDS && !ram_tex.contains_key(&t) {
				self.log(t, "texreq-unheld", scene.level, f, &format!("drawn class word asked for, not read for {UNHELD_FIELDS} fields"));
			}
			if let Some(&(key, ver)) = ram_tex.get(&t) {
				let s = self.seen.entry(t).or_insert((ver, 0));
				let bits = churn_step(s.1, s.0 != ver);
				*s = (ver, bits);
				if churning(bits) {
					self.log(t, "texreq-churn", scene.level, f, &format!("key {key:#x}: texels changed in {} of the last 8 fields", bits.count_ones()));
				}
			}
		}
		self.unheld.retain(|t, _| words.contains(t));
		self.seen.retain(|t, _| words.contains(t));
		self.words = words;
		std::mem::swap(&mut self.drawn, &mut self.now);
		self.tick = self.tick.wrapping_add(1);
		if self.tick % COPY_EVERY == 0 && !ram_tex.is_empty() {
			self.copy = (self.copy + 1) % ram_tex.len();
			if let Some((&t, &(key, ver))) = ram_tex.iter().nth(self.copy) {
				let held = texreq::get_versioned(t).filter(|x| x.1 == ver);
				if let (Some((x, _)), Some(mine)) = (held, tex.get(&key).filter(|_| !screen(t))) {
					if texels_differ(&x, mine, COPY_SAMPLES) {
						self.log(t, "pack-copy", scene.level, f, &format!("key {key:#x} version {ver}: the pack's texels differ from texreq's"));
					}
				}
			}
		}
	}

	fn log(&mut self, t: u64, cache: &'static str, level: i32, field: u32, what: &str) {
		if self.logged.insert((t, cache)) {
			eprintln!("rac2: texture stale TEX0 {t:016x} level {level} field {field} cache {cache}: {what}");
		}
	}
}

/// Fields in a row a drawn word was not held (`held`: texreq holds it now).
fn unheld_step(count: u32, held: bool) -> u32 {
	if held {
		0
	} else {
		count.saturating_add(1)
	}
}

/// [b51 review M1] May a drawn word's after-field texels be released now (`count`: fields in a row drawn and not held)?
fn release_due(count: u32, held: bool) -> bool {
	!held && count >= RELEASE_FIELDS
}

/// The churn bits after a field (bit 0 = this field's version changed), 8 fields kept.
fn churn_step(bits: u8, changed: bool) -> u8 {
	bits << 1 | u8::from(changed)
}

fn churning(bits: u8) -> bool {
	bits.count_ones() >= CHURN_MIN
}

/// Do two textures differ (size, or any of `samples` texels spread over the picture)?
fn texels_differ(a: &Texture, b: &Texture, samples: usize) -> bool {
	if a.w != b.w || a.h != b.h || a.rgba.len() != b.rgba.len() {
		return true;
	}
	let n = a.rgba.len();
	if n == 0 {
		return false;
	}
	let step = (n / samples.max(1)).max(1);
	// An odd stride offset per sample so a pattern with the step's period is not missed.
	(0..n).step_by(step).any(|i| {
		let j = (i + (i / step) % 7) % n;
		a.rgba[j] != b.rgba[j]
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	fn tex(w: usize, h: usize, f: impl Fn(usize) -> [u8; 4]) -> Texture {
		Texture { w, h, rgba: (0..w * h).map(f).collect() }
	}

	#[test]
	fn unheld_counts_up_and_resets_on_a_read() {
		let mut c = 0;
		for _ in 0..5 {
			c = unheld_step(c, false);
		}
		assert_eq!(c, 5);
		assert_eq!(unheld_step(c, true), 0);
		assert_eq!(unheld_step(u32::MAX, false), u32::MAX);
	}

	#[test]
	fn after_texels_are_released_only_after_drawn_unshown_fields() {
		// A class that just came back into view (count restarts at 1) is not released, whatever piled up off-screen.
		let mut c = 0;
		for _ in 0..RELEASE_FIELDS - 1 {
			c = unheld_step(c, false);
			assert!(!release_due(c, false));
		}
		c = unheld_step(c, false);
		assert!(release_due(c, false));
		assert!(!release_due(c, true), "a held word needs no release");
	}

	#[test]
	fn churn_needs_most_of_the_last_eight_fields() {
		let mut b = 0;
		for k in 0..8 {
			b = churn_step(b, k % 2 == 0);
		}
		assert!(!churning(b), "every second field is an animation, not churn");
		for _ in 0..6 {
			b = churn_step(b, true);
		}
		assert!(churning(b));
		for _ in 0..8 {
			b = churn_step(b, false);
		}
		assert!(!churning(b));
	}

	#[test]
	fn sampled_compare_sees_a_changed_picture_and_not_an_equal_one() {
		let a = tex(64, 64, |i| [i as u8, 0, 0, 255]);
		let same = tex(64, 64, |i| [i as u8, 0, 0, 255]);
		assert!(!texels_differ(&a, &same, 256));
		let other = tex(64, 64, |i| [(i * 7) as u8, 3, 0, 255]);
		assert!(texels_differ(&a, &other, 256));
		assert!(texels_differ(&a, &tex(32, 32, |_| [0; 4]), 256));
		// One changed 8 x 8 block (a partly uploaded texture) is still found.
		let block = tex(64, 64, |i| if (i % 64) < 8 && i / 64 < 8 { [9, 9, 9, 9] } else { [i as u8, 0, 0, 255] });
		assert!(texels_differ(&a, &block, 256));
	}
}
