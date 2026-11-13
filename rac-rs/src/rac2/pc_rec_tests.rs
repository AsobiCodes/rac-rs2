use super::*;

fn file(ev: &[(u32, Ev)]) -> Vec<u8> {
	let mut b = MAGIC.to_vec();
	for (f, e) in ev {
		b.extend(encode(*f, e));
	}
	b
}

fn sample() -> Vec<(u32, Ev)> {
	vec![
		(0, Ev::Device(true)),
		(3, Ev::Write(Stage::Jump, 0x1a_8f84, 5u32.to_le_bytes().to_vec())),
		(3, Ev::Write(Stage::Debug, 0x1a_7a80, 10_000u32.to_le_bytes().to_vec())),
		(3, Ev::Vendor),
		(3, Ev::Write(Stage::Jump, 0x1a_8f88, 1u32.to_le_bytes().to_vec())),
		(7, Ev::Movie(0)),
		(7, Ev::Movie(1)),
		(9, Ev::Write(Stage::After, 0x1a_7c1e, vec![1])),
		(12, Ev::Device(false)),
	]
}

#[test]
fn every_kind_round_trips_in_order() {
	let ev = sample();
	assert_eq!(decode(&file(&ev)), Some(ev));
	assert_eq!(encode(3, &Ev::Vendor), vec![3, 0, 0, 0, 4, 0], "field LE, kind, len, no payload");
	assert_eq!(encode(1, &Ev::Write(Stage::After, 0x10, vec![7])), vec![1, 0, 0, 0, 3, 5, 0x10, 0, 0, 0, 7]);
}

#[test]
fn not_a_pc_bin_a_cut_record_and_unknown_kinds() {
	assert_eq!(decode(b"RC2PC\x02\0\0"), None, "another magic");
	assert_eq!(decode(&[]), None);
	let mut b = file(&sample());
	b.extend_from_slice(&[13, 0, 0, 0, 1, 8, 0x10]); // a killed session: the last record is cut
	assert_eq!(decode(&b), Some(sample()));
	let mut u = file(&[(1, Ev::Vendor)]);
	u.extend_from_slice(&[2, 0, 0, 0, 99, 2, 1, 2]); // a kind this build does not know: skipped
	u.extend(encode(4, &Ev::Movie(1)));
	assert_eq!(decode(&u), Some(vec![(1, Ev::Vendor), (4, Ev::Movie(1))]));
	assert_eq!(decode(&[MAGIC.to_vec(), vec![0, 0, 0, 0, 1, 2, 1, 2]].concat()), Some(vec![]), "a write without its address");
}

#[test]
fn a_replay_gives_each_field_its_own_actions() {
	let mut p = Play::new(sample());
	p.begin(0);
	assert!(p.device() && !p.vendor() && p.writes(Stage::Jump).is_empty());
	p.begin(3);
	assert_eq!(p.writes(Stage::Jump), vec![(0x1a_8f84, 5u32.to_le_bytes().to_vec()), (0x1a_8f88, 1u32.to_le_bytes().to_vec())], "in the recorded order");
	assert_eq!(p.writes(Stage::Debug), vec![(0x1a_7a80, 10_000u32.to_le_bytes().to_vec())]);
	assert!(p.vendor() && p.writes(Stage::After).is_empty() && p.movie_next().is_none());
	p.begin(4);
	assert!(!p.vendor() && p.writes(Stage::Jump).is_empty(), "nothing carries over");
	p.begin(7);
	assert_eq!((p.movie_next(), p.movie_next(), p.movie_next()), (Some(0), Some(1), None), "two movies in one field, in order");
	p.begin(9);
	assert_eq!(p.writes(Stage::After), vec![(0x1a_7c1e, vec![1])]);
	assert!(p.device(), "the device holds until the next change");
	p.begin(12);
	assert!(!p.device());
	p.begin(500);
	assert!(!p.device() && !p.vendor());
}

#[test]
fn skipped_fields_still_bring_the_device() {
	let mut p = Play::new(vec![(2, Ev::Device(true)), (5, Ev::Device(false)), (6, Ev::Device(true))]);
	p.begin(4);
	assert!(p.device());
	p.begin(10);
	assert!(p.device(), "the last change up to the field wins");
}

#[test]
fn a_session_file_replays_its_writes_into_ram() {
	let d = std::env::temp_dir().join(format!("rac2_pc_rec_{}", std::process::id()));
	std::fs::create_dir_all(&d).unwrap();
	std::fs::write(d.join("pc.bin"), file(&sample())).unwrap();
	let ev = decode(&std::fs::read(d.join("pc.bin")).unwrap()).unwrap();
	let mut p = Play::new(ev);
	p.begin(3);
	let mut ram = vec![0u8; 0x20_0000];
	for (a, b) in p.writes(Stage::Jump) {
		ram[a as usize..a as usize + b.len()].copy_from_slice(&b);
	}
	assert_eq!(&ram[0x1a_8f84..0x1a_8f8c], &[5, 0, 0, 0, 1, 0, 0, 0]);
	let _ = std::fs::remove_dir_all(&d);
}
