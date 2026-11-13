use super::*;

const PAUSE_ROWS: [u32; 7] = [0xba4, 0xba5, 0xba6, 0xb72, 0xba8, 0xba9, 0xbac];
const CROSS_B: u16 = BIT[CROSS];

fn pause() -> Vec<Row> {
	layout(&PAUSE_ROWS, Kind::Pause)
}

#[test]
fn the_pause_list_has_r_c1s_pc_rows() {
	let r = pause();
	assert_eq!(r.len(), 10);
	assert_eq!(r[4], Row::Retail(4), "Sound");
	assert_eq!((r[5], r[6]), (Row::Pc(Pc::Display), Row::Pc(Pc::Graphics)), "before Camera");
	assert_eq!(r[7], Row::Retail(5), "retail's Camera row (the door opens the Camera tab)");
	assert_eq!(r[8], Row::Retail(6), "Quit Game");
	assert_eq!(r[9], Row::Pc(Pc::Quit), "Quit to Desktop last");
	let t = layout(&[0xbb1, 0x10fc, 0x1b73, 0xbab], Kind::Title);
	assert_eq!(&t[4..], &[Row::Pc(Pc::Display), Row::Pc(Pc::Graphics), Row::Pc(Pc::Camera)]);
	assert!(!t.contains(&Row::Pc(Pc::Quit)), "the title root has its own Quit to Desktop");
	let no_cam = layout(&[0xba4, 0xbac], Kind::Pause);
	assert_eq!(no_cam, vec![Row::Retail(0), Row::Pc(Pc::Display), Row::Pc(Pc::Graphics), Row::Pc(Pc::Camera), Row::Retail(1), Row::Pc(Pc::Quit)]);
}

#[test]
fn keys_step_over_the_pc_rows_and_never_leak_a_late_edge() {
	let rows = pause();
	let mut n = Nav::default();
	// Sound (retail 4): Down lands on Display, the press is kept from the game while held.
	let o = n.keys(&rows, 4, DOWN, DOWN);
	assert_eq!((o.pad, n.focus), (0, Some(5)));
	assert_eq!(n.keys(&rows, 4, DOWN, 0).pad, 0, "still held: no late edge in retail");
	assert_eq!(n.keys(&rows, 4, 0, 0).pad, 0);
	// Down: Graphics; Down: Camera = retail row 5, retail's cursor is on 4: a walk of one step.
	n.keys(&rows, 4, DOWN, DOWN);
	assert_eq!(n.focus, Some(6));
	n.keys(&rows, 4, 0, 0);
	let o = n.keys(&rows, 4, DOWN, DOWN);
	assert_eq!((o.walk, n.focus, o.pad), (Some(5), None, 0));
	// Up from Camera (retail 5, no focus): Graphics, kept from the game.
	n.keys(&rows, 5, 0, 0);
	let o = n.keys(&rows, 5, UP, UP);
	assert_eq!((o.pad, n.focus), (0, Some(6)));
	n.keys(&rows, 5, 0, 0);
	// Down from Graphics back to Camera: retail's cursor is already there, nothing sent.
	let o = n.keys(&rows, 5, DOWN, DOWN);
	assert_eq!((o.walk, n.focus), (None, None));
	// Between retail rows the pad passes bit for bit.
	n.keys(&rows, 2, 0, 0);
	assert_eq!(n.keys(&rows, 2, DOWN, DOWN).pad, DOWN);
	assert_eq!(n.keys(&rows, 3, CROSS_B, CROSS_B).pad, CROSS_B, "Cross on a retail row is retail's");
}

#[test]
fn pc_rows_act_and_quit_asks_first() {
	let rows = pause();
	let mut n = Nav { focus: Some(5), ..Nav::default() };
	let o = n.keys(&rows, 4, CROSS_B, CROSS_B);
	assert_eq!((o.act, o.pad), (Some(Pc::Display), 0), "Cross on Display opens the Display tab and never reaches retail");
	// Quit Game (retail 6): Down focuses Quit to Desktop, Cross asks, Triangle = No, Circle = Yes.
	let mut n = Nav::default();
	n.keys(&rows, 6, DOWN, DOWN);
	assert_eq!(n.focus, Some(9));
	n.keys(&rows, 6, 0, 0);
	n.keys(&rows, 6, CROSS_B, CROSS_B);
	assert!(n.ask);
	n.keys(&rows, 6, 0, 0);
	let o = n.keys(&rows, 6, BIT[TRIANGLE], BIT[TRIANGLE]);
	assert!(!n.ask && !o.yes && o.pad == 0, "No: the Triangle stays away from the game (no back out)");
	assert_eq!(n.keys(&rows, 6, BIT[TRIANGLE], 0).pad, 0, "held Triangle stays kept");
	n.keys(&rows, 6, 0, 0);
	n.keys(&rows, 6, CROSS_B, CROSS_B);
	n.keys(&rows, 6, 0, 0);
	assert!(n.keys(&rows, 6, BIT[CIRCLE], BIT[CIRCLE]).yes);
	// Triangle on a focused PC row backs out of the list as retail's.
	let mut n = Nav { focus: Some(5), ..Nav::default() };
	assert_eq!(n.keys(&rows, 4, BIT[TRIANGLE], BIT[TRIANGLE]).pad, BIT[TRIANGLE]);
}

#[test]
fn mouse_requests_walk_and_click_only_on_arrival() {
	let rows = pause();
	let (mut n, mut w) = (Nav::default(), Walk::new(UP, DOWN));
	// Click Camera (modern 7 = retail 5) with retail's cursor on 1: Down pulses, Cross only once the cursor reads 5.
	assert_eq!(apply(&mut n, &mut w, &rows, Req::Row(7, true)), (None, false));
	let mut cur = 1;
	let mut crossed_at = None;
	for _ in 0..40 {
		let o = w.step(cur, 7, true);
		if o & CROSS_B != 0 {
			crossed_at = Some(cur);
		}
		if o & DOWN != 0 && cur < 5 {
			cur += 1; // retail moves one row per pulse field pair (a coarse model)
		}
	}
	assert_eq!(crossed_at, Some(5));
	// A click on a PC row: focus and the act, no walk.
	let (mut n, mut w) = (Nav::default(), Walk::new(UP, DOWN));
	assert_eq!(apply(&mut n, &mut w, &rows, Req::Row(6, true)), (Some(Pc::Graphics), false));
	assert!(!w.busy());
	assert_eq!(n.focus, Some(6));
	// Quit to Desktop: asks; the answer decides.
	assert_eq!(apply(&mut n, &mut w, &rows, Req::Row(9, true)), (None, false));
	assert!(n.ask);
	assert_eq!(apply(&mut n, &mut w, &rows, Req::Row(2, true)), (None, false), "rows are inert under the question");
	assert_eq!(apply(&mut n, &mut w, &rows, Req::Answer(false)), (None, false));
	assert!(!n.ask);
	n.ask = true;
	assert_eq!(apply(&mut n, &mut w, &rows, Req::Answer(true)), (None, true));
	// A walk that never arrives gives up without the Cross.
	let (mut n, mut w) = (Nav::default(), Walk::new(UP, DOWN));
	apply(&mut n, &mut w, &rows, Req::Row(7, true));
	assert!((0..200).all(|_| w.step(1, 7, true) & CROSS_B == 0));
}

#[test]
fn the_row_table_reads_as_in_fg1_0001() {
	let mut ram = vec![0u8; 0x40_0000];
	let w = |r: &mut Vec<u8>, a: u32, v: u32| r[a as usize..a as usize + 4].copy_from_slice(&v.to_le_bytes());
	let t = 0x28_9e78;
	for (k, &id) in PAUSE_ROWS.iter().enumerate() {
		w(&mut ram, t + 0x14 * k as u32, 0x3f80_0000);
		w(&mut ram, t + 0x14 * k as u32 + 4, id);
	}
	assert_eq!(row_ids(&ram, t), Some(PAUSE_ROWS.to_vec()));
	w(&mut ram, t + 4, 0x7fff);
	assert_eq!(row_ids(&ram, t), None, "a text id outside the game's table: not a row table");
	assert_eq!(row_ids(&ram, 0), None);
}

// [b49 U1] The game thread's own walk (`Game::default()`, also the reset when no list is up) must carry Up / Down: the user's bug (build 48:
// the selection "rolls back to Sound") was a `Walk::default()` (no buttons) that sent nothing, so a step from a PC row to Camera or a hover
// never moved retail's cursor.
#[test]
fn the_games_list_walk_sends_up_and_down() {
	let mut g = Game::default();
	g.walk.to(5, 0);
	assert_eq!(g.walk.step(4, 7, true), DOWN, "retail's cursor on Sound, target Camera: Down");
	let mut g = Game::default();
	g.walk.to(1, 0);
	assert_eq!(g.walk.step(4, 7, true), UP);
	assert_eq!(list_walk(), Walk::new(UP, DOWN));
}
