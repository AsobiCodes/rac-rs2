//! [PC] [b46 W5] B7: the key cap a hint bar shows for a pad button on the vendor, Galactic Map and Ship Shack pages. Cross and Triangle
//! are the Menu layer's Enter and Esc (kbm_table.rs MENU: confirm / back are fixed there); Circle and Square have no Menu key, so the
//! cap names the live PLAY key of that button (`glyph_text::key_name`: follows a rebind, mouse buttons included), the shipped G / F
//! when the binding table has none.
use rac_engine::input::glyph_text as gt;
use rac_engine::ui::keycaps::{self, Cap};
use rac_engine::ui::prim::Symbol;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// [b47 X3] (review minor 8) The Circle / Square caps are looked up at most every 250 ms, not on every call of every frame
/// (`lead_w` + `lead` per hint per frame): a rebind shows within a quarter second.
static CACHE: Mutex<Option<(Instant, [String; 2])>> = Mutex::new(None);
const REFRESH: Duration = Duration::from_millis(250);

fn live(code: u8, default: &str) -> String {
	gt::key_name(code, false).and_then(|n| String::from_utf8(n.into_owned()).ok()).filter(|s| !s.is_empty()).unwrap_or_else(|| default.to_string())
}

/// The cap text of `sym` for the keyboard hint.
pub fn cap(sym: Symbol) -> String {
	let i = match sym {
		Symbol::Cross => return keycaps::name(Cap::Enter).to_string(),
		Symbol::Triangle => return keycaps::name(Cap::Esc).to_string(),
		Symbol::Circle => 0,
		Symbol::Square => 1,
	};
	let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
	if c.as_ref().is_none_or(|(t, _)| t.elapsed() >= REFRESH) {
		*c = Some((Instant::now(), [live(gt::CIRCLE, "G"), live(gt::SQUARE, "F")]));
	}
	c.as_ref().map_or_else(String::new, |(_, v)| v[i].clone())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn confirm_and_back_are_the_menu_keys_and_the_others_never_empty() {
		assert_eq!(cap(Symbol::Cross), keycaps::name(Cap::Enter));
		assert_eq!(cap(Symbol::Triangle), keycaps::name(Cap::Esc));
		assert!(!cap(Symbol::Circle).is_empty() && !cap(Symbol::Square).is_empty());
	}
}
