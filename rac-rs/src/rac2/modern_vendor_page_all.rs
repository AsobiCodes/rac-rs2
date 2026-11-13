//! [PC] The Buy all ammo row of R&C2's modern vendor page and its detail card (lane scr), R&C1's form (rac1/modern_menu/vendor.rs
//! `buy_all_row`, `buyall_detail`): a row above the stock (what a run buys now, or why it buys nothing, with the Square lead while
//! it would buy), selected while it has the focus (`Shop::all_focus`, keys `modern_vendor_buyall::nav`), and then its own card: the
//! ammo it refills, the rounds and the bolts, a button that starts it. The run itself: `modern_vendor_hand.rs`.
use super::*;
use crate::modern_vendor_buyall::refill;

/// The row at the top of the list; returns where the stock rows start.
pub(super) fn row(b: &mut Built, f: &Facts, m: &dyn TextMeasure) -> f32 {
	let s = f.shop;
	let out = &mut b.prims;
	let rect = Rect::new(LIST.x + 12.0, LIST.y + 12.0, LIST.w - 24.0, ROW_H);
	if b.buyall {
		rrect(out, rect, 10.0, ACCENT.tint(0.13), Some((1.5, ACCENT.tint(0.6))));
	}
	let fill_up = refill(s);
	let ready = !s.prompt() && !fill_up.rows.is_empty();
	let ink = if ready { TEAL } else { TEXT_DISABLED };
	let icon = Rect::new(rect.x + 14.0, rect.cy() - 20.0, 40.0, 40.0);
	rrect(out, icon, 20.0, ink.tint(0.14), None);
	rrect(out, Rect::new(icon.cx() - 8.0, icon.cy() - 1.5, 16.0, 3.0), 1.5, ink, None);
	rrect(out, Rect::new(icon.cx() - 1.5, icon.cy() - 8.0, 3.0, 16.0), 1.5, ink, None);
	let hungry = s.rows.iter().any(|r| r.ammo && !r.full());
	let sub = match (hungry, fill_up.rounds) {
		(false, _) => word(&AMMO_FULL, f.lang).to_string(),
		(true, 0) => word(&NOT_ENOUGH, f.lang).to_string(),
		_ => format!("{} +{} \u{b7} {} {}", word(&AMMO, f.lang), fill_up.rounds, commas(fill_up.cost), bolts_word(f, fill_up.cost)),
	};
	let lw = if ready { lead_w(f, Symbol::Square, m) } else { 0.0 };
	if ready {
		lead(out, f, Symbol::Square, rect.right() - 14.0 - lw, rect.cy(), m);
	}
	let max_w = rect.right() - 14.0 - lw - 12.0 - (rect.x + 68.0);
	text(out, rect.x + 68.0, rect.y + 20.0, FontRole::SemiBold, 18.0, if ready { TEXT } else { TEXT_MUTED }, Align::Left, max_w, 0.0, word(&BUY_ALL, f.lang));
	text(out, rect.x + 68.0, rect.y + 41.0, FontRole::Regular, 14.0, TEXT_MUTED, Align::Left, max_w, 0.0, &sub);
	let line = rect.bottom() + 6.0;
	rrect(out, Rect::new(rect.x + 8.0, line, rect.w - 16.0, 1.0), 0.0, HAIRLINE, None);
	b.all_row = (!s.prompt() && !s.buying).then_some(rect);
	line + 7.0
}

/// `s` (Regular 14) over at most two lines of `w`, broken between words.
fn two_lines(m: &dyn TextMeasure, s: &str, w: f32) -> Vec<String> {
	let fits = |t: &str| run_width(m, FontRole::Regular, 14.0, t, 0.0) <= w;
	let words: Vec<&str> = s.split(' ').collect();
	let mut k = 1;
	while k < words.len() && fits(&words[..k + 1].join(" ")) {
		k += 1;
	}
	[words[..k].join(" "), words[k..].join(" ")].into_iter().filter(|l| !l.is_empty()).collect()
}

/// The "Refills:" lines in two lines of `w`: every name when they fit, else the first names and "+N more", at worst the count alone
/// (R&C1 `refill_lines`; an R&C2 shop has up to 18 rows).
pub fn refill_lines(m: &dyn TextMeasure, lang: usize, names: &[&str], w: f32) -> Vec<String> {
	let n = names.len();
	let fits = |l: &[String]| l.iter().all(|s| run_width(m, FontRole::Regular, 14.0, s, 0.0) <= w);
	for k in (1..=n).rev() {
		let more = if k < n { format!(", {}", fill(&REFILLS_MORE, lang, &(n - k).to_string())) } else { String::new() };
		let l = two_lines(m, &fill(&REFILLS, lang, &format!("{}{more}", names[..k].join(", "))), w);
		if fits(&l) {
			return l;
		}
	}
	two_lines(m, &fill(&REFILLS, lang, &fill(&REFILLS_MORE, lang, &n.to_string())), w)
}

/// The card while the row has the focus.
pub(super) fn detail(b: &mut Built, f: &Facts, m: &dyn TextMeasure) {
	let s = f.shop;
	let (x, w) = (DETAIL.x + 22.0, DETAIL.w - 44.0);
	let title = word(&BUY_ALL, f.lang);
	let mut px = 34.0;
	while px > 24.0 && run_width(m, FontRole::Condensed, px, title, 0.0) > w {
		px -= 2.0;
	}
	text(&mut b.prims, x, DETAIL.y + 38.0, FontRole::Condensed, px, WHITE, Align::Left, w, 0.0, title);
	let fill_up = refill(s);
	let hungry = s.rows.iter().any(|r| r.ammo && !r.full());
	let lines: Vec<(String, Rgba)> = if !hungry {
		vec![(word(&AMMO_FULL, f.lang).to_string(), TEXT_DISABLED)]
	} else if fill_up.rows.is_empty() {
		vec![(word(&NOT_ENOUGH, f.lang).to_string(), RED)]
	} else {
		let names: Vec<&str> = fill_up.rows.iter().map(|&i| s.rows[i].name.as_str()).collect();
		refill_lines(m, f.lang, &names, w).into_iter().map(|t| (t, TEXT_BODY)).collect()
	};
	for (k, (t, colour)) in lines.iter().enumerate() {
		text(&mut b.prims, x, DETAIL.y + 76.0 + 22.0 * k as f32, FontRole::Regular, 14.0, *colour, Align::Left, w, 0.0, t);
	}
	let btn = Rect::new(x, DETAIL.bottom() - 22.0 - 46.0, w, 46.0);
	let (label, price) = if fill_up.rounds > 0 {
		(format!("{} +{}", word(&AMMO, f.lang), fill_up.rounds), commas(fill_up.cost))
	} else {
		(word(&TOTAL, f.lang).to_string(), "\u{2014}".to_string())
	};
	price_line(&mut b.prims, btn, &label, &price, false);
	let ready = !fill_up.rows.is_empty() && !s.buying;
	let say = if !fill_up.rows.is_empty() { &BUY_ALL } else if hungry { &NOT_ENOUGH } else { &AMMO_FULL };
	button(b, f, m, btn, word(say, f.lang), ready);
}
