use super::*;

#[test]
fn rows_kept_and_left_classic() {
	// A new player: Lighting Classic, smoothing On, draw distance Medium; ambient occlusion High (shown with Enhanced); sun rays
	// Off in R&C2 unless the file names them (wave 1 montages); shadows (wave 2B) and water (wave 3) and fog (wave 4.1) High with Enhanced, explicit Off kept.
	let g = verdict(&PcOptions::parse(""));
	assert_eq!((g.lighting, g.smoothing, g.draw, g.ao_q, g.gi_q), (0, true, 1, 3, 0));
	assert_eq!((g.shadows, g.shadows_q, g.sun_rays, g.sun_q, g.fog_q), (false, 0, false, 0, 0));
	let g = verdict(&PcOptions::parse("lighting=enhanced\n"));
	assert_eq!((g.lighting, g.shadows, g.shadows_q, g.sun_rays, g.fog_q), (1, true, 3, true, 3));
	assert_eq!((g.water, g.water_q), (true, 3), "water: High with Enhanced");
	assert_eq!((g.particles, g.particles_q), (true, 3), "particles: High with Enhanced (wave 4.2)");
	let g = verdict(&PcOptions::parse("lighting=enhanced\nwater=classic\n"));
	assert_eq!((g.water, g.water_q), (false, 0), "explicit Classic kept");
	let g = verdict(&PcOptions::parse("lighting=enhanced
water=medium
"));
	assert_eq!((g.water, g.water_q), (true, 2));
	let g = verdict(&PcOptions::parse("water=high
"));
	assert_eq!((g.water, g.water_q), (false, 0), "water needs Lighting Enhanced");
	let g = verdict(&PcOptions::parse("lighting=enhanced
shadows=off
"));
	assert_eq!((g.shadows, g.shadows_q), (false, 0));
	let g = verdict(&PcOptions::parse("shadows=high\nsunrays=high\nfog=high\n"));
	assert_eq!((g.shadows_q, g.sun_q, g.fog_q), (3, 3, 3));
	let o = PcOptions::parse("lighting=enhanced\nwater=high\nparticles=high\nshadows=medium\nsunrays=low\nfog=low\nvegetation=lush\nculling=off\ngi=medium\nsmoothing=off\ndraw=high\n");
	let g = verdict(&o);
	assert_eq!((g.lighting, g.gi_q, g.smoothing, g.draw), (1, 2, false, 2));
	assert_eq!((g.shadows_q, g.sun_q, g.fog_q), (2, 1, 1));
	// Still Classic in R&C2 (need R&C2 facts or research): clouds; vegetation as named (veg.rs); water (wave 3) and particles (wave 4.2) as named. A
	// `culling=off` without the marker is a v0.4.0 leftover and reads On (pcopts); with it, Off (BATCH A CULL).
	assert!(g.water && g.particles && g.veg == 2 && g.culling);
	assert_eq!(verdict(&PcOptions::new()).veg, 1, "Moderate by default, as R&C1's row (drawn only with Enhanced)");
	assert!(!verdict(&PcOptions::parse("culling=off\nculling_off=kept\n")).culling && verdict(&PcOptions::new()).culling);
	assert_eq!((g.clouds_q, g.water_q, g.particles_q), (0, 3, 3));
	// Parity rows 14 / 19 (lane gfxc): waves On unless `waves=off`; clouds only from the file's key.
	assert!(g.waves && !verdict(&PcOptions::parse("waves=off\n")).waves);
	assert_eq!(verdict(&PcOptions::parse("lighting=enhanced\nclouds=medium\n")).clouds_q, 2);
	let off = verdict(&PcOptions::parse("shadows=off\nsunrays=off\nfog=classic\n"));
	assert!(!off.shadows && !off.sun_rays && off.fog_q == 0);
}

#[test]
fn display_rows_from_the_file() {
	let o = PcOptions::parse("fps=144\nrenderscale=75\n");
	assert_eq!((o.fps_limit(), o.render_scale()), (144, 75));
	let o = PcOptions::parse("");
	assert_eq!((o.fps_limit(), o.render_scale()), (0, 100));
	assert_eq!(PcOptions::parse("fps=off\n").fps_limit(), u32::MAX);
}

#[test]
fn language_change_is_remembered_once() {
	assert_eq!(remembered_change(u32::MAX, 2, None), None, "the first look is the baseline");
	assert_eq!(remembered_change(0, 0, None), None);
	assert_eq!(remembered_change(0, 2, None), Some(Lang::Fr));
	assert_eq!(remembered_change(0, 2, Some(Lang::Fr)), None, "the file already says so");
	assert_eq!(remembered_change(0, 1, None), None, "not a language word");
}

#[test]
fn binds_reload_only_when_a_bind_key_changed() {
	let a = PcOptions::parse("fps=60\nkbm=x\n");
	assert!(binds_changed(None, &a), "the first apply loads the binds");
	assert!(!binds_changed(Some(&a), &PcOptions::parse("fps=144\nkbm=x\n")), "another option: binds kept");
	for k in BIND_KEYS {
		assert!(binds_changed(Some(&a), &PcOptions::parse(&format!("fps=60\nkbm=x\n{k}=y\n"))), "{k}");
	}
	assert!(binds_changed(Some(&a), &PcOptions::parse("fps=60\n")), "kbm removed");
}
