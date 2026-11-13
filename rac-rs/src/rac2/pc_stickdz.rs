//! [PC] The Stick dead zone row (parity row 45, lane inp; R&C1 `rac1/retail/pcinput.rs` `left_stick` is the model, same maths). High is
//! retail's rule untouched. R&C2's game reads each axis with its own dead zone of about 67 of 127 counts [C, measured 2026-11-11 on level 13: a
//! fed stick moves Ratchet from byte 195 (13000 raw), not from 193; R&C1's is 47, so R&C1's maths with 47 never moved him here],
//! which sits above the host's gate; Medium / Low lower the host's radial gate to just above the game's, so Ratchet moves at once,
//! slowly, and the push grows into the retail curve (identical from 55 % of the throw on). The left stick only; the key is
//! `stickdz=small|minimal` in pc_options.txt (read live, so the menu row takes effect on the next pad poll).
use crate::modern_rows::row;
use rac_engine::pcopts::PcOptions;
use rac_engine::ui::{Cost, RowKind, UiRow, T5};

const NAMES: [&str; 3] = ["retail", "small", "minimal"];
/// PCSX2's analog sensitivity times the byte range: a push of fraction m gives 1.33 * 127 * m counts.
const COUNTS_PER_UNIT: f32 = 1.33 * 127.0;
/// A count safely past the game's own per-axis dead zone (about 67 of 127, see above): 72.
const LO: f32 = 72.0 / COUNTS_PER_UNIT;
/// From this much throw (largest axis) on, the push is retail's curve unchanged.
const FULL_FROM: f32 = 0.55;
/// The host's noise gate per mode (fraction of the full throw): High (retail), Medium and Low.
const GATE: [f32; 3] = [0.0, 0.12, 0.06];

const DEAD_ZONES: &[T5] = &[crate::modern_rows::HIGH, crate::modern_rows::MEDIUM, crate::modern_rows::LOW];
pub const S_CONTROLLER: T5 = ["Controller", "Manette", "Controller", "Mando", "Controller"];

pub static ROWS: [UiRow; 1] = [row(
	"stickdz",
	RowKind::Choice,
	["Stick dead zone", "Zone morte du stick", "Stick-Totzone", "Zona muerta del stick", "Zona morta dello stick"],
	[
		"How far a stick must move before the game notices it. High ignores small drifts of a worn stick.",
		"De combien le stick doit bouger avant que le jeu le remarque. \u{c9}lev\u{e9}e ignore les petites d\u{e9}rives d'un stick us\u{e9}.",
		"Wie weit sich ein Stick bewegen muss, bevor das Spiel ihn bemerkt. Hoch ignoriert kleines Driften eines abgenutzten Sticks.",
		"Cu\u{e1}nto debe moverse un stick para que el juego lo note. Alta ignora la deriva leve de un stick gastado.",
		"Quanto deve muoversi uno stick prima che il gioco lo noti. Alta ignora le piccole derive di uno stick consumato.",
	],
	DEAD_ZONES,
	0,
	Cost::None,
	None,
)];

/// The row value in `o`: 0 High (retail), 1 Medium, 2 Low.
pub fn mode(o: &PcOptions) -> i32 {
	o.extra_get("stickdz").map_or(0, |v| NAMES.iter().position(|n| v.trim().eq_ignore_ascii_case(n)).unwrap_or(0)) as i32
}

/// Writes the row into `o`.
pub fn apply(o: &mut PcOptions, v: i32) {
	let m = v.clamp(0, 2) as usize;
	o.extra_set("stickdz", (m != 0).then(|| NAMES[m].to_string()));
}

/// `rac_engine::input::pad::set_left_stick` hook: the left stick of an XInput report `(x, y)` and the radial gate (retail's) in, the
/// vector and the gate to give the byte conversion out.
pub fn left_stick(x: i16, y: i16, retail_gate: f32) -> ((i16, i16), f32) {
	left_stick_mode(x, y, retail_gate, mode(&crate::options::get()) as u8)
}

fn left_stick_mode(x: i16, y: i16, retail_gate: f32, mode: u8) -> ((i16, i16), f32) {
	let g = GATE[mode.min(2) as usize];
	if g == 0.0 {
		return ((x, y), retail_gate);
	}
	let (fx, fy) = (x as f32 / 32767.0, y as f32 / 32767.0);
	if (fx * fx + fy * fy).sqrt() <= g {
		return ((0, 0), 0.0);
	}
	let m = fx.abs().max(fy.abs());
	let k = lift(m, g) / m;
	let s = |v: f32| (v * k * 32767.0).round().clamp(-32767.0, 32767.0) as i16;
	((s(fx), s(fy)), 0.0)
}

/// A push `m` past the gate `g` as the game should see it: from `LO` at the gate, linear up to retail's value at `FULL_FROM`.
fn lift(m: f32, g: f32) -> f32 {
	if m >= FULL_FROM {
		m
	} else {
		LO + (FULL_FROM - LO) * ((m - g).max(0.0) / (FULL_FROM - g))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn high_is_retail_untouched() {
		assert_eq!(left_stick_mode(1234, -20000, 7849.0, 0), ((1234, -20000), 7849.0));
	}

	/// Inside the gate the stick is centred; a lifted push clears the game's own dead zone (about 67 counts per axis) and never lowers one.
	#[test]
	fn deadzone_row_effect() {
		for mode in [1u8, 2] {
			let g = GATE[mode as usize];
			assert_eq!(left_stick_mode((g * 32767.0 * 0.9) as i16, 0, 7849.0, mode), ((0, 0), 0.0), "inside the gate: centred");
			let ((x, _), gate) = left_stick_mode((g * 32767.0 * 1.2) as i16, 0, 7849.0, mode);
			assert_eq!(gate, 0.0);
			let byte = 0x80 as f32 + x as f32 / 32767.0 * 1.33 * 127.0;
			assert!(byte - 128.0 > 67.0, "mode {mode}: the game sees a push past its dead zone ({byte})");
			assert_eq!(left_stick_mode(20000, 0, 7849.0, mode), ((20000, 0), 0.0), "from 55 % on the retail curve");
		}
		// Retail (High) swallows a push the Low mode passes: 8 % of the throw is inside the retail gate (7849 / 32767 = 24 %).
		let p = (0.08 * 32767.0) as i16;
		assert_eq!(left_stick_mode(p, 0, 7849.0, 0), ((p, 0), 7849.0));
		assert!(left_stick_mode(p, 0, 7849.0, 2).0 .0 > 0);
	}

	#[test]
	fn the_key_round_trips() {
		let mut o = PcOptions::parse("");
		assert_eq!(mode(&o), 0);
		apply(&mut o, 2);
		assert_eq!((mode(&o), o.extra_get("stickdz")), (2, Some("minimal")));
		apply(&mut o, 0);
		assert!(o.extra_get("stickdz").is_none());
	}
}
