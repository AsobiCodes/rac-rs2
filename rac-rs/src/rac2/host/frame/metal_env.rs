//! [PC] What the engine's metal draw was given (host/frame.rs `Stage::metal`): the classes with metal packets and the chrome / glass
//! environment map, a copy of the GS texture the metal packets' TEX0 names (gsfull::texreq). The copy follows texreq's newest
//! CONFIRMED read of that TEX0 (every new version, not one snapshot per class count), so a read that caught the frame buffer's alpha
//! plane mid-upload or drawn over (the speckled helmet, visor and Clank of build 43) is never kept: the draw keeps its previous picture
//! until texreq's confirming re-reads settle, and takes the settled picture when they do.

use rac_formats::level::Texture;
use std::sync::Arc;

/// The metal draw's inputs as last given.
#[derive(Default)]
pub(super) struct Given {
	/// The environment map's TEX0, its texreq version and the picture (kept for reloads of the packets alone).
	env: Option<(u64, u32, Arc<Texture>)>,
	/// The ids of the classes with metal packets, sorted.
	classes: Vec<u32>,
}

/// What `Stage::metal` gives the engine this field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step {
	/// Nothing changed (or nothing confirmed to give yet).
	Keep,
	/// Load the packets with the confirmed picture just read (`true`) or with the picture already given (`false`: the classes changed).
	Load(bool),
}

/// [b49 U2] The classes' change key: per class (sorted by id) its id and a fingerprint of its metal packets (counts and a few
/// sampled vertices). The held weapon's class id never changes (its buffer's address), so ids alone kept the previous weapon's metal
/// packets loaded after a switch.
pub(super) fn keys<'a>(classes: impl Iterator<Item = (u32, &'a [rac_formats::mobies::MetalPacket])>) -> Vec<u32> {
	let mut v: Vec<(u32, u32)> = classes.map(|(id, p)| (id, fingerprint(p))).collect();
	v.sort_unstable();
	v.into_iter().flat_map(|(id, f)| [id, f]).collect()
}

fn fingerprint(packets: &[rac_formats::mobies::MetalPacket]) -> u32 {
	let mix = |h: u32, x: u32| (h ^ x).wrapping_mul(0x0100_0193);
	let mut h = mix(0x811c_9dc5, packets.len() as u32);
	for p in packets {
		h = mix(mix(mix(h, p.positions.len() as u32), p.faces.len() as u32), p.skin.len() as u32);
		let n = p.positions.len();
		for i in [0, n / 3, n / 2, n.saturating_sub(1)].into_iter().filter(|&i| i < n) {
			h = p.positions[i].iter().fold(h, |h, x| mix(h, x.to_bits()));
		}
		if let Some(f) = p.faces.last() {
			h = f.1.iter().fold(mix(h, f.0 as u32), |h, &x| mix(h, x));
		}
	}
	h
}

/// The pure decision: `confirmed` = texreq's confirmed (TEX0, version) of the environment map now (None: not read, or still
/// confirming); `classes` = the sorted ids of the classes with metal packets now.
pub(super) fn step(given_env: Option<(u64, u32)>, given_classes: &[u32], confirmed: Option<(u64, u32)>, classes: &[u32]) -> Step {
	if confirmed.is_some() && confirmed != given_env {
		return Step::Load(true);
	}
	if given_env.is_some() && classes != given_classes {
		return Step::Load(false);
	}
	Step::Keep
}

impl Given {
	/// `step` for these inputs.
	pub(super) fn step(&self, confirmed: Option<(u64, u32)>, classes: &[u32]) -> Step {
		step(self.env.as_ref().map(|e| (e.0, e.1)), &self.classes, confirmed, classes)
	}

	/// The picture to give: the confirmed one just read (`fresh`, stored as given) or the one given before.
	pub(super) fn picture(&mut self, fresh: Option<(u64, u32, Arc<Texture>)>, classes: Vec<u32>) -> Option<Arc<Texture>> {
		if let Some(f) = fresh {
			self.env = Some(f);
		}
		self.classes = classes;
		self.env.as_ref().map(|e| e.2.clone())
	}

	/// The number of classes with metal packets given (log).
	pub(super) fn classes_len(&self) -> usize {
		self.classes.len() / 2
	}
}

/// RAC2_METALLOG=<dir> (debug): the directory for the environment map's pictures, None when off.
fn log_dir() -> Option<&'static str> {
	static DIR: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
	DIR.get_or_init(|| std::env::var("RAC2_METALLOG").ok()).as_deref()
}

/// FNV-1a of the texels (log).
fn hash(t: &Texture) -> u64 {
	t.rgba.iter().fold(0xcbf2_9ce4_8422_2325, |h, p| (h ^ u64::from(u32::from_le_bytes(*p))).wrapping_mul(0x100_0000_01b3))
}

fn write_png(path: String, t: &Texture) {
	let rgb = t.rgba.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
	let _ = std::fs::write(path, rac_engine::render::png(&rac_engine::render::Image { w: t.w, h: t.h, rgb }));
}

/// RAC2_METALLOG: every new texreq version of the environment map (confirmed or still confirming), logged and written as a PNG.
pub(super) fn log_held(t0: u64, confirmed: bool) {
	use rac_engine::gsfull::texreq;
	static SEEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);
	let Some(d) = log_dir() else { return };
	let v = u64::from(texreq::version(t0)) << 1 | u64::from(confirmed);
	if SEEN.swap(v, std::sync::atomic::Ordering::Relaxed) == v {
		return;
	}
	let Some(t) = texreq::get_versioned(t0) else { return };
	let (f, h) = (texreq::field(), hash(&t.0));
	eprintln!("metallog: f{f} held t0 {t0:016x} ver {} confirmed {confirmed} hash {h:016x}", t.1);
	write_png(format!("{d}/held_f{f:06}_v{}_{h:016x}.png", t.1), &t.0);
}

/// RAC2_METALLOG: each picture given to the metal draw.
pub(super) fn log_load(t0: u64, t: &Texture, classes: usize) {
	let Some(d) = log_dir() else { return };
	let (f, h) = (rac_engine::gsfull::texreq::field(), hash(t));
	eprintln!("metallog: f{f} load t0 {t0:016x} classes {classes} hash {h:016x}");
	write_png(format!("{d}/env_f{f:06}_{h:016x}.png"), t);
}

#[cfg(test)]
mod tests {
	use super::*;

	const T: u64 = 0x8005_8305_ddb0_8b00;

	fn tex(v: u8) -> Arc<Texture> {
		Arc::new(Texture { w: 1, h: 1, rgba: vec![[v; 4]] })
	}

	/// [b49 U2] The held weapon: the same class id, another weapon's metal packets -> another key (a reload); the same packets -> the same key.
	#[test]
	fn same_id_other_metal_packets_change_the_key() {
		use rac_formats::mobies::MetalPacket;
		let pk = |n: usize, x: f32| MetalPacket { positions: (0..n).map(|i| [x + i as f32, 0.0, 1.0]).collect(), faces: vec![(-3, [0, 1, 2])], ..Default::default() };
		let (a, b, a2) = (vec![pk(6, 0.0), pk(4, 1.0)], vec![pk(5, 0.0), pk(4, 1.0)], vec![pk(6, 0.0), pk(4, 1.0)]);
		let key = |p: &Vec<MetalPacket>| keys([(7u32, &p[..])].into_iter());
		assert_ne!(key(&a), key(&b));
		assert_eq!(key(&a), key(&a2));
		assert_ne!(key(&a), keys([(8u32, &a[..])].into_iter()));
	}

	#[test]
	fn nothing_until_confirmed() {
		// Classes arrive before the first confirmed read: nothing to give (the draw has no picture yet).
		assert_eq!(step(None, &[], None, &[3, 7]), Step::Keep);
		assert_eq!(step(None, &[], Some((T, 5)), &[3, 7]), Step::Load(true));
	}

	#[test]
	fn follows_every_confirmed_version() {
		// Build 43 copied the picture once per class count: a later version (texreq re-read the map) was never given.
		assert_eq!(step(Some((T, 5)), &[3, 7], Some((T, 5)), &[3, 7]), Step::Keep);
		assert_eq!(step(Some((T, 5)), &[3, 7], Some((T, 6)), &[3, 7]), Step::Load(true));
		// A version base after a prune / level reset is a new picture too.
		assert_eq!(step(Some((T, 5)), &[3, 7], Some((T, 1 << 20)), &[3, 7]), Step::Load(true));
	}

	#[test]
	fn unconfirmed_change_keeps_the_previous_picture() {
		// texreq is confirming a changed read (or forgot the map): None, the given picture stays.
		assert_eq!(step(Some((T, 5)), &[3, 7], None, &[3, 7]), Step::Keep);
		// New classes with metal packets meanwhile: their packets load with the previous picture, not the unconfirmed one.
		assert_eq!(step(Some((T, 5)), &[3, 7], None, &[3, 7, 9]), Step::Load(false));
	}

	#[test]
	fn class_set_not_count() {
		// One class left, another arrived: same count, other packets (build 43 keyed on the count and missed it).
		assert_eq!(step(Some((T, 5)), &[3, 7], Some((T, 5)), &[3, 9]), Step::Load(false));
	}

	#[test]
	fn picture_keeps_the_given_one() {
		let mut g = Given::default();
		assert!(g.picture(None, vec![3]).is_none());
		assert_eq!(g.picture(Some((T, 5, tex(1))), vec![3]).unwrap().rgba, vec![[1; 4]]);
		assert_eq!(g.step(None, &[3, 9]), Step::Load(false));
		assert_eq!(g.picture(None, vec![3, 9]).unwrap().rgba, vec![[1; 4]]);
		assert_eq!(g.step(Some((T, 5)), &[3, 9]), Step::Keep);
		assert_eq!(g.step(Some((T, 6)), &[3, 9]), Step::Load(true));
	}
}
