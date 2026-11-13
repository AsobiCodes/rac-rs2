//! The Modern camera inside a quick save (batch 17): every number the camera carries from one field to the next (the pivot spring, the
//! Smooth layer's velocities and eases, the wall rise, the aim lift, the last pose), as flat 32-bit words. A quick load that restores
//! only yaw / pitch / boom seeds the springs afresh: the camera then sits ~1 m off where the saved game had it and the game, which
//! reads the camera record (moby culling, debris), runs on a different RAM from the first field. With all of it back the loaded game
//! is field-for-field the saved one (proof: RAM dumps of the field after the save vs the first field after the load).

use super::{Cam, Dy};

/// Words of a full snapshot (the first 3 are the old yaw / pitch / boom goal triple).
pub const WORDS: usize = 64;

struct Wr(Vec<u32>);
impl Wr {
	fn f(&mut self, v: f32) {
		self.0.push(v.to_bits());
	}
	fn v(&mut self, v: &[f32]) {
		v.iter().for_each(|&x| self.f(x));
	}
	fn u(&mut self, v: u32) {
		self.0.push(v);
	}
}

struct Rd<'a>(std::slice::Iter<'a, u32>);
impl Rd<'_> {
	fn f(&mut self) -> f32 {
		f32::from_bits(*self.0.next().unwrap_or(&0))
	}
	fn v<const N: usize>(&mut self) -> [f32; N] {
		let mut o = [0.0; N];
		o.iter_mut().for_each(|x| *x = self.f());
		o
	}
	fn u(&mut self) -> u32 {
		*self.0.next().unwrap_or(&0)
	}
}

fn pack_dy(w: &mut Wr, d: &Dy) {
	w.v(&d.hero_prev);
	w.v(&d.hv);
	w.v(&d.pv);
	w.u(d.idle);
	w.u(d.air);
	w.u(d.is_air as u32);
	w.v(&[d.ref_h, d.ref_p, d.speed, d.vz, d.rc_used, d.bias]);
	w.v(&d.open_s);
	w.v(&[d.dist_f, d.len_v]);
	w.v(&d.rs);
	w.v(&d.pivc);
}

fn unpack_dy(r: &mut Rd, d: &mut Dy) {
	d.hero_prev = r.v();
	d.hv = r.v();
	d.pv = r.v();
	d.idle = r.u();
	d.air = r.u();
	d.is_air = r.u() != 0;
	[d.ref_h, d.ref_p, d.speed, d.vz, d.rc_used, d.bias] = r.v();
	d.open_s = r.v();
	[d.dist_f, d.len_v] = r.v();
	d.rs = r.v();
	d.pivc = r.v();
}

impl Cam {
	/// The whole camera state for a quick save (None while retail drives or before the first seed).
	pub fn snapshot(&self) -> Option<Vec<u32>> {
		if !self.live {
			return None;
		}
		let mut w = Wr(Vec::with_capacity(WORDS));
		w.v(&[self.yaw, self.pitch, self.dist_goal, self.dist, self.len, self.rise, self.rise_goal, self.rise_v]);
		w.v(&self.pivot);
		w.v(&self.last.0);
		w.v(&[self.last.1, self.last.2]);
		self.last_rows.iter().for_each(|r| w.v(r));
		w.v(&self.hero_last);
		w.u(self.aiming as u32);
		w.u(self.dyn_was as u32);
		w.0.extend(self.lift.pack());
		pack_dy(&mut w, &self.dy);
		w.v(&self.tr);
		Some(w.0)
	}

	/// A quick load: the next seed starts from this snapshot. Three words (the old file) only set yaw / pitch / boom.
	pub fn restore(&mut self, words: &[u32]) {
		if words.len() < 3 {
			return;
		}
		self.preset([f32::from_bits(words[0]), f32::from_bits(words[1]), f32::from_bits(words[2])]);
		self.full = (words.len() == WORDS).then(|| words.to_vec());
	}

	/// The seed's last step: lays the snapshot over the freshly seeded state. `field` = this field's counter.
	pub(super) fn apply_full(&mut self, words: &[u32]) {
		let mut r = Rd(words.iter());
		[self.yaw, self.pitch, self.dist_goal, self.dist, self.len, self.rise, self.rise_goal, self.rise_v] = r.v();
		self.pivot = r.v();
		self.last.0 = r.v();
		[self.last.1, self.last.2] = r.v();
		for k in 0..3 {
			self.last_rows[k] = r.v();
		}
		self.hero_last = r.v();
		self.aiming = r.u() != 0;
		self.dyn_was = r.u() != 0;
		self.lift = crate::modcam_shoulder::Lift::unpack([r.u(), r.u(), r.u()]);
		unpack_dy(&mut r, &mut self.dy);
		self.tr = r.v();
		// The counter restarts at a load: the next `track` must see an ordinary next field, not a gap.
		self.dy.field = self.field.wrapping_sub(1);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::modcam_pose::{Config, NoWalls, Pose, PoseIn};

	fn run(c: &mut Cam, from: u32, n: u32) -> Pose {
		let mut last = None;
		for f in from..from + n {
			let retail = Pose { eye: [195.0, 318.0, 128.0], rows: [[0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 1.0]] };
			let mut i = PoseIn::idle(f, [194.2, 323.8, 126.0], retail);
			i.smooth = true;
			last = c.step(&i, &mut NoWalls);
		}
		last.unwrap()
	}

	#[test]
	fn a_restored_camera_goes_on_exactly_like_the_saved_one() {
		let cfg = Config::new(0.02, 7.0);
		let mut a = Cam::new(cfg);
		run(&mut a, 1, 30);
		let words = a.snapshot().unwrap();
		assert_eq!(words.len(), WORDS);
		let next_a = run(&mut a, 31, 30);
		// A new camera (a new process) with the field counter restarted at 1.
		let mut b = Cam::new(cfg);
		b.restore(&words);
		let next_b = run(&mut b, 1, 30);
		assert_eq!(next_a, next_b);
	}

	#[test]
	fn the_old_three_word_file_still_restores_yaw_pitch_and_boom() {
		let mut b = Cam::new(Config::new(0.02, 7.0));
		b.restore(&[1.5f32.to_bits(), 0.3f32.to_bits(), 9.0f32.to_bits()]);
		run(&mut b, 1, 1);
		assert_eq!(b.dist_goal, 9.0);
	}
}
