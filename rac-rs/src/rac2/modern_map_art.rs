//! [PC] [b46 W5] The galaxy painting under R&C2's modern Galactic Map, in motion (sweep B12; model: R&C1 `modern_menu/ship_anim.rs` +
//! `ship_host.rs:place_art`, engine side `rac_engine::viewer` `ArtAnim` / `set_menu_art_anim`, game-neutral). The painting is the port's
//! own picture (`assets/galaxy/galactic_map.png`, made for this port, not extracted from any disc), so R&C2 shares it. Everything runs on
//! DISPLAY time (wall seconds), independent of the game's steps. `RAC_GMAP_ANIM=0` = the still painting, `RAC_GMAP_T0` shifts the clock
//! (same switches as R&C1, ENV.md). The page draws its own panels over it; the markers' places never move (clicking stays easy).
use rac_engine::viewer::{ArtAnim, Gpu, MENU_ART_ANIM_TEX, MENU_ART_TEX};

/// The painting, PNG.
pub const ART_PNG: &[u8] = include_bytes!("../../assets/galaxy/galactic_map.png");

/// The painting's disc (measured for R&C1's page on this same file): centre as a fraction, semi-axes in painting pixels, tilt (rad).
pub const CENTRE: [f32; 2] = [0.5, 335.0 / 720.0];
pub const AXES: [f32; 2] = [560.0, 215.0];
pub const TILT: f32 = -0.19;
pub const TURN_PERIOD_S: f64 = 480.0;
pub const FEATHER: f32 = 0.45;
pub const CORE_RADIUS: f32 = 0.30;
pub const CORE_AMP: f32 = 0.065;
pub const CORE_PERIOD_S: f64 = 7.0;
pub const TWINKLE: f32 = 0.35;

/// Whether the painting moves (`RAC_GMAP_ANIM=0` = still).
pub fn enabled() -> bool {
	static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
	*ON.get_or_init(|| std::env::var("RAC_GMAP_ANIM").map_or(true, |v| v.trim() != "0"))
}

/// Display time in seconds (+ `RAC_GMAP_T0`); stands still at T0 with `RAC_GMAP_ANIM=0`.
pub fn clock_s() -> f64 {
	static T0: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
	let t0 = *T0.get_or_init(|| std::env::var("RAC_GMAP_T0").ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0.0));
	t0 + if enabled() { crate::modern_walk::now_ms() as f64 / 1000.0 } else { 0.0 }
}

/// The engine's animation of the painting at time `t` seconds.
pub fn anim_at(t: f64) -> ArtAnim {
	let breathe = (t / CORE_PERIOD_S * std::f64::consts::TAU).sin() as f32;
	ArtAnim {
		centre: CENTRE,
		axes: AXES,
		tilt: TILT,
		angle: rac_engine::viewer::turn_angle(t, TURN_PERIOD_S),
		feather: FEATHER,
		core_gain: 1.0 + CORE_AMP * breathe,
		core_radius: CORE_RADIUS,
		time: (t % rac_engine::viewer::TWINKLE_WRAP_S) as f32,
		twinkle: TWINKLE,
	}
}

/// A slow smooth pulse 0..=1 with period `period` s, shifted by `phase` (0..1 of a period).
pub fn pulse(t: f64, period: f64, phase: f64) -> f32 {
	(0.5 - 0.5 * ((t / period + phase) * std::f64::consts::TAU).cos()) as f32
}

fn lum(px: &[u8], w: usize, x: usize, y: usize) -> f32 {
	let i = (y * w + x) * 4;
	px[i].max(px[i + 1]).max(px[i + 2]) as f32 / 255.0
}

fn in_core(x: f32, y: f32, w: usize, h: usize) -> bool {
	let (q0, q1) = (x - CENTRE[0] * w as f32, y - CENTRE[1] * h as f32);
	let (c, s) = (TILT.cos(), TILT.sin());
	let (e0, e1) = (c * q0 + s * q1, -s * q0 + c * q1);
	((e0 / AXES[0]).powi(2) + (e1 / AXES[1]).powi(2)).sqrt() < 0.25
}

/// Is (x, y) a 5 x 5 local brightness maximum (ties broken by position) that is small (the ring 6 px round it under 60 % of it)?
fn is_star_peak(px: &[u8], w: usize, x: usize, y: usize, l: f32) -> bool {
	for dy in -2i32..=2 {
		for dx in -2i32..=2 {
			let n = lum(px, w, (x as i32 + dx) as usize, (y as i32 + dy) as usize);
			if n > l || (n == l && (dy < 0 || (dy == 0 && dx < 0))) {
				return false;
			}
		}
	}
	let ring: f32 = (0..8).map(|k| {
		let a = k as f32 * std::f32::consts::FRAC_PI_4;
		lum(px, w, (x as f32 + 6.0 * a.cos()).round() as usize, (y as f32 + 6.0 * a.sin()).round() as usize)
	}).sum::<f32>() / 8.0;
	ring < 0.6 * l
}

/// Writes the star map into the painting's alpha (255 = not a star, 1..=253 = a star's code), as R&C1's `mark_stars`.
pub fn mark_stars(px: &mut [u8], w: u32, h: u32) {
	let (w, h) = (w as usize, h as usize);
	if px.len() != w * h * 4 {
		return;
	}
	for p in px.chunks_exact_mut(4) {
		p[3] = 255;
	}
	let mut peaks = Vec::new();
	for y in 7..h.saturating_sub(7) {
		for x in 7..w.saturating_sub(7) {
			let l = lum(px, w, x, y);
			if l >= 0.62 && !in_core(x as f32, y as f32, w, h) && is_star_peak(px, w, x, y, l) {
				peaks.push((x, y));
			}
		}
	}
	for (x, y) in peaks {
		let code = (1 + (x.wrapping_mul(73856093) ^ y.wrapping_mul(19349663)) % 253) as u8;
		for dy in -4i32..=4 {
			for dx in -4i32..=4 {
				if dx * dx + dy * dy > 16 {
					continue;
				}
				let (nx, ny) = ((x as i32 + dx) as usize, (y as i32 + dy) as usize);
				if nx < w && ny < h && px[(ny * w + nx) * 4 + 3] == 255 && lum(px, w, nx, ny) >= 0.35 {
					px[(ny * w + nx) * 4 + 3] = code;
				}
			}
		}
	}
}

/// The painting's two HUD quads for a HUD of `win` pixels: black over the whole picture, the painting at the 1280 x 720 design
/// rectangle's place (`Scale::fit`, like the page).
fn quads(win: (u32, u32)) -> [rac_engine::overlay::Quad; 2] {
	let r = rac_engine::ui::kit::Scale::fit(win.0, win.1).to_window(rac_engine::ui::prim::Rect::new(0.0, 0.0, 1280.0, 720.0));
	let quad = |rect: [f32; 4], c: f32| rac_engine::overlay::Quad { tex: MENU_ART_TEX, rect, uv: [0.0, 0.0, 1.0, 1.0], colour: [c, c, c, 1.0], angle: 0.0, corners: None };
	[quad([0.0, 0.0, win.0 as f32, win.1 as f32], 0.0), quad([r.x, r.y, r.w, r.h], 1.0)]
}

/// Lays the painting under the page (first in the HUD); `want` false frees it. Called by `host/screens_map.rs:draw` every frame.
pub fn place(g: &mut Gpu, want: bool) {
	if !want {
		g.set_menu_art_anim(None);
		if g.has_menu_art() {
			g.clear_menu_art();
		}
		return;
	}
	if !g.has_menu_art() {
		static ART: std::sync::OnceLock<Option<(u32, u32, Vec<u8>)>> = std::sync::OnceLock::new();
		let art = ART.get_or_init(|| {
			rac_engine::texpack::png::load(ART_PNG).ok().map(|i| {
				let mut px = i.levels[0].clone();
				mark_stars(&mut px, i.w, i.h);
				(i.w, i.h, px)
			})
		});
		let Some((w, h, px)) = art else { return };
		g.set_menu_art(*w, *h, px);
	}
	let mut q = quads((g.config.width, g.config.height));
	if enabled() {
		q[1].tex = MENU_ART_ANIM_TEX;
		g.set_menu_art_anim(Some(anim_at(clock_s())));
	} else {
		g.set_menu_art_anim(None);
	}
	g.hud.splice(0..0, q);
	g.hud_split = None;
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_real_painting_has_many_stars_and_the_core_has_none() {
		let i = rac_engine::texpack::png::load(ART_PNG).expect("the painting loads");
		let mut px = i.levels[0].clone();
		mark_stars(&mut px, i.w, i.h);
		let n = px.chunks_exact(4).filter(|p| p[3] != 255).count();
		assert!(n > 200 && n < 20000, "star pixels: {n}");
	}

	#[test]
	fn pulse_is_a_smooth_0_to_1_wave() {
		assert!(pulse(0.0, 2.0, 0.0).abs() < 1e-6);
		assert!((pulse(1.0, 2.0, 0.0) - 1.0).abs() < 1e-6);
		assert!(anim_at(100.0).core_gain > 0.9);
	}
}
