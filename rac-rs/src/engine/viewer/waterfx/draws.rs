//! One water mesh for the water pass (`WaterDraw`) and the position target's format.

use super::*;

pub(super) const WPOS_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;

/// One water draw of the level pass.
pub(in crate::viewer) struct WaterDraw {
	/// Vertex buffer (`Vertex` layout), index buffer (u32), index count.
	pub vb: wgpu::Buffer,
	pub ib: wgpu::Buffer,
	pub n: u32,
	/// The first index (`range`: a slice of a shared batch).
	pub first: u32,
	/// Its flat plane, if any (`mirror::plane_of`).
	pub plane: super::mirror::Plane,
	/// Its slot in this frame's flow table (`WaterFx::strip_flows`), 0 = still water.
	pub flow: u32,
	/// What drew it (the water log, RAC_WATER_LOG).
	pub kind: &'static str,
	/// [PC] Its box (min x, y, z, max x, y, z) for the wakes and the churn (`Wakes`); none for a slice of a shared batch.
	pub bounds: Option<[f32; 6]>,
}

/// [PC] Heat haze (heathaze.rs): the box (min x, y, z, max x, y, z) of an ocean draw's points `pos` when the level's ocean liquid is
/// lava (the pack's liquid table, as `WaterDraw::liquid` reads it), else None. Taken from the ocean draws themselves, so the haze does
/// not need Water Enhanced (review S4). Lava drawn as level terrain (`terrain_liquid`, `WaterDraw::range`) has no box: no haze.
pub(in crate::viewer) fn lava_ocean_box(level: usize, pos: impl Iterator<Item = [f32; 3]>) -> Option<[f32; 6]> {
	if liquid_slot(level) != Some(super::levels::LAVA_SLOT) {
		return None;
	}
	let mut b = [f32::MAX, f32::MAX, f32::MAX, f32::MIN, f32::MIN, f32::MIN];
	for p in pos {
		for k in 0..3 {
			b[k] = b[k].min(p[k]);
			b[k + 3] = b[k + 3].max(p[k]);
		}
	}
	(b[0] <= b[3]).then_some(b)
}

/// [PC] The quads (4 points each) cut into runs of consecutive quads at one height (`Looks::split_water_quads`): a run ends at a
/// quad whose height range lies more than `RUN_GAP` from the run's. One flat plane per run: lumped into one draw, a small dense
/// mesh 2.3 units over R&C2's Dobbo sea (293 points against the sea's 140) won `plane_of`'s vertex count whenever the tide split
/// the sea's points over two bins, and the sea flipped between the geometric waves and a flat mirror (user, test build 42).
pub(in crate::viewer) fn height_runs(pos: &[[f32; 3]]) -> Vec<std::ops::Range<usize>> {
	const RUN_GAP: f32 = 0.5;
	let mut runs: Vec<(std::ops::Range<usize>, f32, f32)> = Vec::new();
	for (q, c) in pos.chunks_exact(4).enumerate() {
		let (lo, hi) = c.iter().fold((f32::MAX, f32::MIN), |(l, h), p| (l.min(p[2]), h.max(p[2])));
		match runs.last_mut() {
			Some(r) if lo <= r.2 + RUN_GAP && hi >= r.1 - RUN_GAP => (r.0.end, r.1, r.2) = (q + 1, r.1.min(lo), r.2.max(hi)),
			_ => runs.push((q..q + 1, lo, hi)),
		}
	}
	runs.into_iter().map(|r| r.0).collect()
}

impl WaterDraw {
	pub(in crate::viewer) fn new(vb: wgpu::Buffer, ib: wgpu::Buffer, n: u32, pos: impl Iterator<Item = [f32; 3]> + Clone) -> WaterDraw {
		let mut b = [f32::MAX, f32::MAX, f32::MAX, f32::MIN, f32::MIN, f32::MIN];
		for p in pos.clone() {
			for k in 0..3 {
				b[k] = b[k].min(p[k]);
				b[k + 3] = b[k + 3].max(p[k]);
			}
		}
		WaterDraw { vb, ib, n, first: 0, plane: super::mirror::plane_of(pos), flow: 0, kind: "", bounds: (b[0] <= b[3]).then_some(b) }
	}

	/// Indices `first..first + n` of a shared batch (`terrain_liquid`): its slot, no mirror plane.
	pub(in crate::viewer) fn range(vb: wgpu::Buffer, ib: wgpu::Buffer, first: u32, n: u32, slot: u32) -> WaterDraw {
		WaterDraw { vb, ib, n, first, plane: None, flow: slot, kind: "", bounds: None }
	}

	/// [PC] Open sea (`swell`): flat water whose box spans at least `SEA_SIZE` (the oceans and Pokitaru's sea, not a pool).
	pub(super) fn sea(&self) -> bool {
		self.plane.is_some_and(|b| (b[3] - b[1]).max(b[4] - b[2]) >= SEA_SIZE) && self.flow == 0 && self.kind != "strip"
	}

	/// [PC] A lake (`waves`): flat still water, spanning at least `LAKE_SIZE` but no open sea.
	pub(super) fn lake(&self) -> bool {
		self.plane.is_some_and(|b| {
			let span = (b[3] - b[1]).max(b[4] - b[2]);
			span >= LAKE_SIZE && span < SEA_SIZE
		}) && self.flow == 0 && self.kind != "strip"
	}

	/// [PC] Its box for the wakes (`Wakes`): water only (not lava, mud or sludge), its flat plane's height when it has one.
	pub(super) fn wake_box(&self) -> Option<[f32; 6]> {
		if self.flow >= LAST_FLOW && self.flow != OPAQUE_SLOT && self.flow != COLD_SLOT {
			return None;
		}
		match (self.plane, self.bounds) {
			(Some(p), _) => Some([p[1], p[2], p[0], p[3], p[4], p[0]]),
			(None, b) => b,
		}
	}

	pub(in crate::viewer) fn kind(mut self, kind: &'static str) -> WaterDraw {
		self.kind = kind;
		self
	}

	/// A flowing river group (`WaterFx::quad_flows`): its flow slot, and no mirror plane (the river slopes).
	pub(in crate::viewer) fn flowing(mut self, slot: u32) -> WaterDraw {
		self.flow = slot;
		self.plane = None;
		self
	}

	/// [PC] Water retail draws milky (Pokitaru's waterfall ponds, `pond_fx_tex`): `OPAQUE_SLOT` (its own colour, no clear
	/// shallows), keeping its mirror plane.
	pub(in crate::viewer) fn opaque(mut self) -> WaterDraw {
		self.flow = OPAQUE_SLOT;
		self
	}

	/// An ocean that is no water (`liquid_slot`: Gaspar's lava, Aridia's mud): its slot, and no mirror plane.
	pub(in crate::viewer) fn liquid(mut self, level: usize) -> WaterDraw {
		if let Some(slot) = liquid_slot(level) {
			self.flow = slot;
			self.plane = None;
		}
		self
	}
}

#[cfg(test)]
mod tests {
	use super::height_runs;

	fn quad(z: f32) -> [[f32; 3]; 4] {
		[[0.0, 0.0, z], [1.0, 0.0, z], [1.0, 1.0, z], [0.0, 1.0, z + 0.05]]
	}

	fn runs(zs: &[f32]) -> Vec<std::ops::Range<usize>> {
		height_runs(&zs.iter().flat_map(|&z| quad(z)).collect::<Vec<_>>())
	}

	/// Dobbo's case (test build 42): sea quads at 408.3, a small mesh 2 units higher, more sea: three runs, each its own plane.
	#[test]
	fn height_runs_cut_at_a_height_gap() {
		assert_eq!(runs(&[408.27, 408.37, 408.3, 410.3, 410.6, 408.33]), vec![0..3, 3..5, 5..6]);
		assert_eq!(runs(&[0.0, 0.3, 0.6, 0.9, 1.2]), vec![0..5], "a swell that climbs in small steps stays one run");
		assert_eq!(runs(&[]), Vec::<std::ops::Range<usize>>::new());
		let mut odd: Vec<[f32; 3]> = quad(5.0).to_vec();
		odd.push([0.0, 0.0, 9.0]);
		assert_eq!(height_runs(&odd), vec![0..1], "a stray point past the last quad is no quad");
	}
}
