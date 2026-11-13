//! [PC] R&C2's item pictures for the modern screens (lane scr; R&C1's twin rac1/host/ui_icons.rs + modern_menu/icons.rs): the level's
//! HUD set (primary sections 4..9, the same header + five banks as R&C1, read by `rac_formats::hud`) holds sheets 0xea60 + n.
//! Probe result (scr-5): the sheet ids are there, but R&C1's sprite decoding gives noise on R&C2 data (rac2/notes_modern_screens.md
//! 7 (I)); nothing is registered yet, the vendor keeps its ring.

#[cfg(test)]
mod tests {
	fn iso() -> Option<String> {
		let p = std::env::var("RAC2_ISO").unwrap_or_else(|_| "E:/Ratchet & Clank 2 (Europe, Australia) (En,Fr,De,Es,It)/Ratchet & Clank 2 (Europe, Australia) (En,Fr,De,Es,It).iso".into());
		std::path::Path::new(&p).exists().then_some(p)
	}

	/// Probe: writes every 0xea60.. sheet's frames of level 2 as PPM into RAC2_ICON_DUMP (when set).
	#[test]
	fn probe_item_sheets() {
		let (Some(iso), Ok(dir)) = (iso(), std::env::var("RAC2_ICON_DUMP")) else { return };
		let l = rac_formats::rc2::level::load_level(&iso, 2).unwrap();
		let h = rac_formats::hud::hud_sprites(&l, false).unwrap();
		let mut ids: Vec<(u16, usize)> = h.icons.iter().map(|i| (i.id, i.count)).collect();
		ids.sort();
		eprintln!("icons {}: {:x?}", ids.len(), ids);
		for i in h.icons.iter().filter(|i| (0xea60..0xeb00).contains(&i.id)) {
			for f in 0..i.count {
				let Some(sp) = h.frames.get(i.first + f).and_then(|x| x.as_ref()) else { continue };
				let t = &sp.tex;
				let mut out = format!("P6\n{} {}\n255\n", t.w, t.h).into_bytes();
				for p in &t.rgba {
					let a = p[3] as u32;
					out.extend([(p[0] as u32 * a / 255) as u8, (p[1] as u32 * a / 255) as u8, (p[2] as u32 * a / 255) as u8]);
				}
				std::fs::write(format!("{dir}/i{:04x}_{f}.ppm", i.id), out).unwrap();
			}
		}
	}
}
