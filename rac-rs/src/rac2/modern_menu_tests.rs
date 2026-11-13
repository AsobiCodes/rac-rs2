//! Tests of R&C2's modern menu table (modern_menu.rs): the table is valid, every row reads and writes its pc_options key, the presets
//! name only rows.
use super::*;

#[test]
fn the_table_is_valid() {
	assert_eq!(menu().validate(), Vec::<String>::new());
	assert_eq!(TABS.iter().map(|t| t.id).collect::<Vec<_>>(), ["display", "graphics", "camera", "controls"]);
}

#[test]
fn every_row_has_a_value_and_its_default_on_a_new_file() {
	let o = PcOptions::new();
	for (_, _, r) in menu().rows() {
		// aspect: an engine static other tests move (aspect_box::current).
		// upscaler / upscale_q: engine statics too (upscale_choice; modern_rows_upscale.rs and the engine's own tests).
		if r.kind == rac_engine::ui::RowKind::Preset || r.kind == rac_engine::ui::RowKind::Bind || engine_static(r.id) || host_view(r.id) {
			continue;
		}
		assert_eq!(value(&o, r.id), Some(r.def), "row {} on an empty file", r.id);
	}
	assert_eq!(value(&o, "fog"), Some(3), "the fog row (wave 4.1), High by default as R&C1");
	assert_eq!(value(&o, "particles"), Some(3), "the particles row (wave 4.2), High by default as R&C1");
	assert_eq!(value(&o, "clouds"), Some(0), "the clouds row (b45): Classic on a new file, the sky stays retail's");
	assert_eq!(value(&o, "no_such_row"), None, "rows R&C2 does not show are unknown");
}

/// Rows whose value lives in an engine static that other tests move while these run (aspect_box, upscale_choice, resolution,
/// picture_adjust, fgen_follow): checked by their own tests (modern_rows_display.rs, modern_rows_upscale.rs, the engine's).
fn engine_static(id: &str) -> bool {
	matches!(id, "aspect" | "resolution" | "brightness" | "contrast" | "framegen") || id.starts_with("upscale")
}

/// Rows that are no setting: the Controls tab's Show row (b45 P6, keyboard / controller view) is a switch of the page kept in a host
/// static, read and set through `pc_controls::value` / `act` (the `Pack::ui` hooks), never through the options file.
fn host_view(id: &str) -> bool {
	id == "bind_view"
}

#[test]
fn every_value_of_every_row_round_trips_through_the_file() {
	for (_, _, r) in menu().rows() {
		let vals: Vec<i32> = match r.kind {
			rac_engine::ui::RowKind::Choice => (0..r.values.len() as i32).collect(),
			rac_engine::ui::RowKind::Toggle => vec![0, 1],
			rac_engine::ui::RowKind::Slider { min, max, step, .. } => (min..=max).step_by(step as usize).collect(),
			rac_engine::ui::RowKind::Preset | rac_engine::ui::RowKind::Bind => continue,
		};
		if host_view(r.id) {
			assert!(crate::pc_controls::value(r.id).is_some(), "row {}: the page's own switch", r.id);
			continue;
		}
		for v in vals {
			let mut o = PcOptions::new();
			assert!(apply(&mut o, r.id, v), "row {}", r.id);
			// Through the file text, as `options::change` saves it and the next start reads it.
			let o = PcOptions::parse(&o.to_text());
			if engine_static(r.id) {
				continue; // these values live in engine statics, checked by their own tests
			}
			assert_eq!(value(&o, r.id), Some(v), "row {} value {v}", r.id);
		}
	}
}

#[test]
fn shadows_read_high_on_a_new_file_and_the_verdict_needs_enhanced() {
	let mut o = PcOptions::new();
	assert_eq!(value(&o, "shadows"), Some(3), "a new file shows High (inert with Classic)");
	assert!(!crate::options::verdict(&o).shadows, "Classic lighting: no sun pass");
	apply(&mut o, "lighting", 1);
	assert_eq!(value(&o, "shadows"), Some(3), "Enhanced: High");
	assert_eq!(crate::options::verdict(&o).shadows_q, 3, "Enhanced: the verdict is High too");
	apply(&mut o, "shadows", 0);
	assert_eq!(value(&o, "shadows"), Some(0), "an explicit Off stays");
}

#[test]
fn a_new_file_reads_as_the_high_preset() {
	let o = PcOptions::new();
	for (id, v) in HIGH {
		assert_eq!(value(&o, id), Some(v), "row {id}: a new file must match the High preset (else the menu shows Custom)");
	}
}

#[test]
fn presets_name_only_rows_and_send_the_texture_filter() {
	let ids: Vec<&str> = menu().rows().map(|(_, _, r)| r.id).collect();
	for p in menu().presets {
		for (id, _) in p.sets {
			assert!(ids.contains(id), "preset row {id}");
		}
	}
	assert!(menu().presets.iter().all(|p| p.sets.contains(&("texfilter", 1))));
}

#[test]
fn the_camera_row_says_classic_needs_a_restart() {
	let cam = menu().rows().find(|(_, _, r)| r.id == "camera").unwrap().2;
	assert!(cam.note.is_some());
}
