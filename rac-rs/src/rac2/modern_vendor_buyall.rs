//! [PC] "Buy all ammo" for R&C2's modern vendor (lane scr; R&C1's PC row, rac1/modern_menu/vendor.rs): the keys of its row and
//! what a run would buy. The run itself is ONE request: `modern_vendor_hand.rs` buys every ammo row with retail's own purchase calls
//! at the shop's next frame, as R&C1's purchase hand does (it used to walk the cursor to each row and buy it by pad presses).
//! The page lists it as R&C1 does (rac1/modern_menu/vendor_nav.rs): a row above the stock with its own card; Up on row 0 puts the
//! focus on it (retail's cursor stays on row 0), Down gives it back, Cross there starts the run; Square starts it from anywhere in
//! the list (R&C1's key). Left out against R&C1: the browse sound when only the focus moves (R&C2's shop cue is not mapped [?]).
use crate::modern_screen::{LEFT, RIGHT};
use crate::modern_vendor::{Ask, Shop, CROSS_B};

/// The keys' memory on the Buy all ammo row: the focus and the keys eaten until released (retail reads new presses, so a key let
/// through later would count as a fresh press).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Nav {
	pub focus: bool,
	eat: u16,
}

/// One field of `nav`: the pad the game gets, a request (walk back to row 0) and whether Buy all ammo starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Keys {
	pub pad: u16,
	pub ask: Option<Ask>,
	pub start: bool,
}

/// The browse keys on the Buy all ammo row (`pad` = the page's word after `browse_pad`: Left = up the list, Right = down; `new` its new
/// presses; `live` = the list browses, no run and no pulse). The focus moves only while live; eaten keys stay away until released.
pub fn nav(n: &mut Nav, live: bool, cur: usize, pad: u16, new: u16) -> Keys {
	n.eat &= pad;
	let (mut ask, mut start) = (None, false);
	if live && n.focus {
		if new & CROSS_B != 0 {
			start = true;
			n.eat |= CROSS_B;
		}
		if new & RIGHT != 0 {
			n.focus = false;
			n.eat |= RIGHT;
			if cur != 0 {
				ask = Some(Ask::Row(0, false));
			}
		} else if new & LEFT != 0 {
			n.eat |= LEFT; // the top of the list
		}
	} else if live && new & LEFT != 0 && cur == 0 {
		n.focus = true;
		n.eat |= LEFT;
	}
	Keys { pad: pad & !n.eat, ask, start }
}

/// What a run would buy now: the rows that take rounds (in list order, the bolts spent row by row as the run spends them), the rounds
/// and the bolts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Refill {
	pub rows: Vec<usize>,
	pub rounds: u32,
	pub cost: u32,
}

pub fn refill(shop: &Shop) -> Refill {
	let mut f = Refill::default();
	let mut left = shop.bolts;
	for (i, r) in shop.rows.iter().enumerate() {
		let n = r.most(left);
		if r.ammo && n > 0 {
			f.rows.push(i);
			f.rounds += n;
			f.cost += n * r.price;
			left -= n * r.price;
		}
	}
	f
}

/// The ammo rows that can take rounds now.
pub fn wanted(shop: &Shop) -> Vec<usize> {
	shop.rows.iter().enumerate().filter(|(_, r)| r.ammo && r.most(shop.bolts) > 0).map(|(i, _)| i).collect()
}
