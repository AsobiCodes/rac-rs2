//! [b49 U2b] A content stamp of a RAM moby class: the held weapon is ALWAYS the class at the same buffer (header 0x178590, one class
//! id), only its contents change with the weapon. `scene::Resolver::ram_unit` decided "same class" by two header words and the TEX0
//! list, so a weapon that shares both with the one held before (the gloves) kept the old weapon's mesh, or kept an old part of it.
//! The stamp hashes the class's packet table and a sample of each packet's VIF lists, so a different weapon never reads as the same
//! class; it is a sampled hash (cheap per field), identical for identical contents.

const MASK: u32 = 0x01ff_ffff;

fn mix(h: u64, x: u64) -> u64 {
	(h ^ x).wrapping_mul(0x0000_0100_0000_01b3).rotate_left(23)
}

fn u32_at(ram: &[u8], a: usize) -> Option<u32> {
	ram.get(a..a + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn u16_at(ram: &[u8], a: usize) -> Option<u16> {
	ram.get(a..a + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

/// Hash of `qw` quad words at `at`, 16 bytes of every 32 (the lists are static class data; the AD TEX0 words the game
/// rewrites are watched on their own by `words_unchanged`, a change in a sample only decodes the class again).
fn list_hash(ram: &[u8], at: usize, qw: usize, mut h: u64) -> u64 {
	let len = qw.min(4096) * 16;
	if at == 0 || at.checked_add(len).is_none_or(|e| e > ram.len()) {
		return mix(h, 0xdead);
	}
	h = mix(h, len as u64);
	let mut o = 0;
	while o < len {
		h = mix(mix(h, u64::from_le_bytes(ram[at + o..at + o + 8].try_into().unwrap_or_default())), u64::from_le_bytes(ram[at + o + 8..at + o + 16].try_into().unwrap_or_default()));
		o += 32;
	}
	// The last quad word too (a list that differs only in its tail).
	if len >= 16 {
		h = mix(mix(h, u64::from_le_bytes(ram[at + len - 16..at + len - 8].try_into().unwrap_or_default())), u64::from_le_bytes(ram[at + len - 8..at + len].try_into().unwrap_or_default()));
	}
	h
}

/// The stamp of the class at `hdr` (RAM address): header counts, the packet table (16-byte rows: list pointer + quad words, second
/// list pointer + quad words) and a sample of every list.
pub fn stamp(ram: &[u8], hdr: u32) -> u64 {
	let hdr = hdr as usize;
	let mut h = 0xcbf2_9ce4_8422_2325u64;
	let Some(t) = u32_at(ram, hdr).map(|p| (p & MASK) as usize) else { return h };
	let (high, metal, first_metal) = (ram.get(hdr + 4).copied().unwrap_or(0) as usize, ram.get(hdr + 6).copied().unwrap_or(0) as usize, ram.get(hdr + 7).copied().unwrap_or(0) as usize);
	h = mix(mix(mix(h, high as u64), metal as u64), first_metal as u64);
	h = ram.get(hdr + 8..hdr + 0x48).map_or(h, |b| b.chunks(8).fold(h, |h, c| mix(h, u64::from_le_bytes(c.try_into().unwrap_or_default()))));
	let n = high.max(first_metal + metal).min(96);
	for k in 0..n {
		let e = t + k * 16;
		let Some(row) = ram.get(e..e + 16) else { break };
		h = row.chunks(8).fold(h, |h, c| mix(h, u64::from_le_bytes(c.try_into().unwrap_or_default())));
		for f in [0usize, 8] {
			let p = u32_at(ram, e + f).map_or(0, |p| (p & MASK) as usize);
			h = list_hash(ram, p, u16_at(ram, e + f + 4).unwrap_or(0) as usize, h);
		}
	}
	h
}

/// [b50 V2] Debounce of a changed stamp. `stored` = the stamp at the last decode, `pending` = a different stamp seen before as
/// (stamp, field first seen), `stamp` = this call's, `field` = this call's field. Returns `(same, pending)`: `same` true = treat the
/// class as unchanged (do not decode for the stamp); false = decode (the same new stamp was seen in two different fields). A stamp
/// equal to the stored one clears the pending one (A, B, A never decodes); a stamp that differs from the pending one restarts it
/// (a model still streaming in never decodes). Cost bound: at most one decode per stable new stamp.
pub fn debounce(stored: u64, pending: Option<(u64, u64)>, stamp: u64, field: u64) -> (bool, Option<(u64, u64)>) {
	if stamp == stored {
		return (true, None);
	}
	match pending {
		Some((s, f)) if s == stamp => (field <= f, pending),
		_ => (true, Some((stamp, field))),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// One stamp per field fed to a unit (a decode makes the stored stamp the new one); returns the indexes that decoded.
	fn run(initial: u64, seq: &[u64]) -> Vec<usize> {
		let (mut stored, mut pending, mut out) = (initial, None, vec![]);
		for (i, &s) in seq.iter().enumerate() {
			let (same, p) = debounce(stored, pending, s, i as u64 + 1);
			pending = p;
			if !same {
				stored = s;
				pending = None;
				out.push(i);
			}
		}
		out
	}

	#[test]
	fn steady_never_decodes() {
		assert!(run(5, &[5; 20]).is_empty());
	}

	#[test]
	fn half_loaded_decodes_once_two_fields_after_it_settles() {
		let mut seq: Vec<u64> = (100..110).collect(); // 10 fields of a changing stamp
		seq.extend([7, 7, 7, 7]);
		assert_eq!(run(5, &seq), vec![11]); // 7 first seen at index 10, decoded at the next field
	}

	#[test]
	fn a_toggle_never_decodes_until_it_stays() {
		let mut seq: Vec<u64> = (0..12).map(|i| if i % 2 == 0 { 8 } else { 5 }).collect();
		assert!(run(5, &seq).is_empty());
		seq.extend([8, 8, 8]);
		assert_eq!(run(5, &seq), vec![13]);
		assert!(run(5, &[8, 5, 8, 5, 8, 5, 8]).is_empty());
	}

	#[test]
	fn many_calls_in_one_field_do_not_confirm() {
		let (_, p) = debounce(5, None, 9, 3);
		assert!(debounce(5, p, 9, 3).0, "same field: still unconfirmed");
		assert!(!debounce(5, p, 9, 4).0);
	}

	#[test]
	fn a_stamp_equal_to_the_stored_one_clears_the_pending_one() {
		assert_eq!(debounce(5, Some((9, 1)), 5, 2), (true, None));
	}

	/// A tiny class: header at 0x100 with a table at 0x200 of one packet whose list sits at 0x300.
	fn class(fill: u8) -> Vec<u8> {
		let mut r = vec![0u8; 0x1000];
		r[0x100..0x104].copy_from_slice(&0x200u32.to_le_bytes());
		r[0x104] = 1;
		r[0x200..0x204].copy_from_slice(&0x300u32.to_le_bytes());
		r[0x204..0x206].copy_from_slice(&4u16.to_le_bytes());
		for b in &mut r[0x300..0x340] {
			*b = fill;
		}
		r
	}

	#[test]
	fn same_contents_same_stamp() {
		assert_eq!(stamp(&class(7), 0x100), stamp(&class(7), 0x100));
	}

	#[test]
	fn other_packet_contents_other_stamp_with_the_same_header_words() {
		let (a, mut b) = (class(7), class(7));
		b[0x328] ^= 0x55; // a different weapon's vertex data, same header
		assert_eq!(a[0x104..0x108], b[0x104..0x108]);
		assert_ne!(stamp(&a, 0x100), stamp(&b, 0x100));
	}

	#[test]
	fn a_tail_only_difference_changes_the_stamp() {
		let (a, mut b) = (class(7), class(7));
		b[0x33c] ^= 1;
		assert_ne!(stamp(&a, 0x100), stamp(&b, 0x100));
	}

	#[test]
	fn pointers_outside_ram_do_not_panic() {
		let mut r = class(1);
		r[0x200..0x204].copy_from_slice(&0x01ff_fff0u32.to_le_bytes());
		let _ = stamp(&r, 0x100);
		let _ = stamp(&r, 0x2000);
	}
}
