//! texreq unit tests (moved out of texreq.rs; size limit).
use super::*;

fn tex(c: u8) -> Arc<Texture> {
    Arc::new(Texture { w: 1, h: 1, rgba: vec![[c, 0, 0, 255]] })
}

#[test]
fn a_reread_with_other_texels_bumps_the_version() {
    let t = 0xdead_0001u64;
    store(vec![(t, Some(tex(1)), ANY_PROG, 0, 0, false)], false);
    let v0 = version(t);
    store(vec![(t, Some(tex(1)), ANY_PROG, 0, 0, false)], false);
    assert_eq!(version(t), v0, "same texels: no new version");
    store(vec![(t, Some(tex(2)), ANY_PROG, 0, 0, false)], false);
    assert_eq!(version(t), v0 + 1);
    assert_eq!(get(t).unwrap().rgba[0][0], 2);
}

fn verify_left(t: u64) -> u8 {
    HAVE.lock().unwrap().as_ref().unwrap().get(&t).unwrap().verify
}

/// A first read needs VERIFY_READS confirming re-reads with unchanged texels; a change while confirming keeps counting down (an
/// animated texture is not confirmed for ever); a change on a settled texture starts the confirming again.
#[test]
fn a_first_read_and_a_change_need_confirming_rereads() {
    let t = 0xdead_0002u64;
    store(vec![(t, Some(tex(5)), ANY_PROG, 0, 0, false)], false);
    assert_eq!(verify_left(t), VERIFY_READS, "a first read is unconfirmed");
    store(vec![(t, Some(tex(5)), ANY_PROG, 0, 0, false)], false);
    assert_eq!(verify_left(t), VERIFY_READS - 1);
    store(vec![(t, Some(tex(6)), ANY_PROG, 0, 0, false)], false);
    assert_eq!(verify_left(t), VERIFY_READS - 2, "a change while confirming counts down too");
    for _ in 0..VERIFY_READS + 2 {
        store(vec![(t, Some(tex(6)), ANY_PROG, 0, 0, false)], false);
    }
    assert_eq!(verify_left(t), 0, "confirmed, and the count saturates");
    store(vec![(t, Some(tex(7)), ANY_PROG, 0, 0, false)], false);
    assert_eq!(verify_left(t), VERIFY_READS, "a change on a settled texture: confirm again");
}

/// `get_confirmed` gives nothing for a first read or a change still confirming (R&C2's chrome environment map: a copy kept by the
/// renderer), the settled picture and its version once the confirming re-reads are done; `is_held` follows the hold.
#[test]
fn confirmed_reads_only() {
    let t = 0xdead_0120u64;
    assert!(!is_held(t) && get_confirmed(t).is_none());
    store(vec![(t, Some(tex(5)), ANY_PROG, 0, 0, false)], false);
    assert!(is_held(t));
    assert!(get_confirmed(t).is_none(), "a first read is unconfirmed");
    for _ in 0..VERIFY_READS {
        store(vec![(t, Some(tex(5)), ANY_PROG, 0, 0, false)], false);
    }
    let (x, v) = get_confirmed(t).unwrap();
    assert_eq!((x.rgba[0][0], v), (5, version(t)));
    store(vec![(t, Some(tex(9)), ANY_PROG, 0, 0, false)], false);
    assert!(get_confirmed(t).is_none(), "a change on a settled texture: unconfirmed again");
    assert_eq!(get(t).unwrap().rgba[0][0], 9, "get still gives the unconfirmed read");
    for _ in 0..VERIFY_READS {
        store(vec![(t, Some(tex(9)), ANY_PROG, 0, 0, false)], false);
    }
    assert_eq!(get_confirmed(t).map(|c| (c.0.rgba[0][0], c.1)), Some((9, v + 1)));
}

/// An alternating texture ends its confirming after VERIFY_READS reads, and is then left to the MIN_CHANGE_GAP refresh.
#[test]
fn an_alternating_texture_stops_being_reread_every_second_field() {
    let t = 0xdead_0003u64;
    for i in 0..VERIFY_READS as usize + 1 {
        store(vec![(t, Some(tex(10 + (i % 2) as u8)), ANY_PROG, 0, 0, false)], false);
    }
    assert_eq!(verify_left(t), 0);
    let mut m = HashMap::new();
    m.insert(t, Have { tex: tex(1), ver: 0, at: 100, changed: 100, verify: 0, read_gen: 0, misses: 0, hold: 0, src: 0, prog: ANY_PROG, raw: 0, need: 0, up: false, after: false, after_miss: 0 });
    for now in 102..160 {
        assert!(pick_due(&m, now).is_empty(), "field {now}");
    }
    assert_eq!(pick_due(&m, 160), vec![t], "refreshed after the change gap");
}

fn have(at: u32, changed: u32, verify: u8) -> Have {
    Have { tex: tex(1), ver: 0, at, changed, verify, read_gen: 0, misses: 0, hold: 0, src: 0, prog: ANY_PROG, raw: 0, need: 0, up: false, after: false, after_miss: 0 }
}

/// Stale entries whose re-reads nobody draws (a previous level's textures) must not take all slots for ever.
#[test]
fn stale_textures_do_not_starve_a_fresh_one() {
    let mut m: HashMap<u64, Have> = (0..6).map(|i| (i, have(i as u32, 0, VERIFY_READS))).collect();
    m.insert(100, have(50, 50, VERIFY_READS));
    let mut first = None;
    for now in 60..100u32 {
        let due = pick_due(&m, now);
        assert!(due.len() <= MAX_SCHED);
        if due.contains(&100) {
            first = Some(now);
            break;
        }
        // Nobody draws them: dropped undrawn.
        due.iter().for_each(|t| miss(m.get_mut(t).unwrap(), now));
    }
    assert!(first.is_some_and(|f| f <= 62), "the fresh texture is scheduled within a few fields: {first:?}");
}

#[test]
fn confirmations_have_their_own_quota_ahead_of_refreshes() {
    let mut m: HashMap<u64, Have> = HashMap::new();
    for i in 0..5 {
        m.insert(i, have(0, 0, 0)); // age refreshes, oldest
        m.insert(10 + i, have(90, 90, VERIFY_READS)); // confirmations
    }
    let due = pick_due(&m, 200);
    assert_eq!(due.len(), MAX_SCHED);
    assert_eq!(due.iter().filter(|&&t| t >= 10).count(), 2);
    assert_eq!(due.iter().filter(|&&t| t < 10).count(), 2, "refreshes keep their quota");
    m.retain(|&t, _| t >= 10);
    assert_eq!(pick_due(&m, 200).len(), MAX_SCHED, "confirmations alone use every slot");
}

/// A class switching back to a held TEX0 word asks for it again: the undrawn back-off ends (so an upload since its read is re-read at
/// once) and no first read is queued.
#[test]
fn asking_again_for_a_held_texture_ends_its_back_off() {
    let t = 0xdead_0010u64;
    store(vec![(t, Some(tex(1)), 11, 0, 0, false)], false);
    if let Some(x) = HAVE.lock().unwrap().as_mut().unwrap().get_mut(&t) {
        (x.misses, x.hold) = (3, 500);
    }
    want_prog(t, 11);
    let (m, h) = HAVE.lock().unwrap().as_ref().unwrap().get(&t).map(|x| (x.misses, x.hold)).unwrap();
    assert_eq!((m, h), (0, 0));
    assert!(!WANT.lock().unwrap().iter().any(|e| e.t == t), "held: no first read");
    assert!(held_for(t, 11) && !held_for(t, 8));
}

/// A re-read whose VRAM bytes equal the held texture's (no decode, texels None) confirms it: same version, one confirmation done,
/// new stamp; a first read without texels stores nothing.
#[test]
fn a_reread_of_equal_bytes_confirms_without_a_decode() {
    let t = 0xdead_0011u64;
    store(vec![(t, Some(tex(4)), ANY_PROG, 77, 1, false)], false);
    let v0 = version(t);
    store(vec![(t, None, ANY_PROG, 77, 9, false)], false);
    let h = HAVE.lock().unwrap();
    let x = h.as_ref().unwrap().get(&t).unwrap();
    assert_eq!((x.ver, x.verify, x.read_gen, x.raw, x.tex.rgba[0][0]), (v0, VERIFY_READS - 1, 9, 77, 4));
    drop(h);
    store(vec![(0xdead_0012, None, ANY_PROG, 1, 0, false)], false);
    assert!(get(0xdead_0012).is_none());
}

/// A want of one VU1 program is served only by that program's draws; a plain want by any draw.
#[test]
fn a_program_want_is_served_only_by_that_program() {
    assert!(serves(ANY_PROG, 8) && serves(ANY_PROG, ANY_PROG));
    assert!(serves(11, 11));
    assert!(!serves(11, 8) && !serves(11, ANY_PROG), "a PATH2 / other-program draw of the same TEX0 word holds other texels");
}

/// A held texture whose blocks got an upload since its read is queued at once (no 30-field wait); one backing off after undrawn
/// re-reads waits; untouched ones are not read again.
#[test]
fn an_upload_over_a_held_texture_queues_a_reread_at_once() {
    let mut m: HashMap<u64, Have> = HashMap::new();
    m.insert(1, Have { read_gen: 5, ..have(100, 100, 0) });
    m.insert(2, Have { read_gen: 9, ..have(100, 100, 0) });
    m.insert(3, Have { read_gen: 5, hold: 200, ..have(100, 100, 0) });
    // Uploads after stamp 7 over textures 1 and 3.
    let up = |t: u64, x: &Have| t != 2 && x.read_gen < 7;
    assert_eq!(pick_touched(&m, 101, up), vec![(1, ANY_PROG)], "touched one field after its read, the backing-off one waits");
    let mut later = pick_touched(&m, 200, up);
    later.sort_unstable();
    assert_eq!(later, vec![(1, ANY_PROG), (3, ANY_PROG)]);
    assert!(pick_touched(&m, 101, |_, _| false).is_empty());
}

#[test]
fn long_undrawn_textures_are_forgotten_and_come_back_with_a_new_version() {
    let mut m: HashMap<u64, Have> = HashMap::new();
    m.insert(1, have(0, 0, 0));
    m.insert(2, have(0, 0, 0));
    for now in 0..PRUNE_MISSES as u32 {
        miss(m.get_mut(&1).unwrap(), now);
    }
    assert!(m[&1].hold >= REFRESH_FIELDS * 8 - 8 * 4 && !reached(100, m[&1].hold), "the back-off grows");
    let e0 = EPOCH.load(Ordering::Relaxed);
    assert!(prune(&mut m, |_| false));
    assert!(m.contains_key(&2) && !m.contains_key(&1));
    assert!(EPOCH.load(Ordering::Relaxed) > e0, "a re-read texture gets a new version base");
}

/// [b50 V7b] H3: a word a pack class uses now is not forgotten however long it stays undrawn; once no class uses it, it is.
#[test]
fn prune_keeps_a_word_a_class_uses() {
    let mut m: HashMap<u64, Have> = HashMap::new();
    m.insert(1, Have { prog: 11, ..have(0, 0, 0) });
    m.insert(2, Have { prog: 11, ..have(0, 0, 0) });
    for now in 0..PRUNE_MISSES as u32 + 4 {
        miss(m.get_mut(&1).unwrap(), now);
        miss(m.get_mut(&2).unwrap(), now);
    }
    // Word 1 in use (CLD bits ignored by the caller's set: the closure gets the held key).
    assert!(prune(&mut m, |t| t == 1));
    assert!(m.contains_key(&1) && !m.contains_key(&2), "the used word stays, the unused one goes");
    assert!(!prune(&mut m, |t| t == 1), "nothing more to forget");
    assert!(prune(&mut m, |_| false) && m.is_empty(), "no class uses it any more: forgotten");
}

/// [b50 V7b] H1: a program want's FIRST read after a field is held but not shown; asking for it again queues a read at a draw (a refresh
/// want, never read after a field); a later after-field read keeps it hidden; a read at a draw shows it (equal bytes: the held texels).
/// A plain (any-draw) want's after-field read is shown as before.
#[test]
fn a_first_read_after_the_field_waits_for_a_draw() {
    let t = 0xdead_0130u64;
    store(vec![(t, Some(tex(8)), 11, 55, 0, false)], true);
    assert!(is_held(t) && awaits_draw(t));
    assert!(get_versioned(t).is_none() && get_confirmed(t).is_none() && uploaded_lately(t).is_none(), "not shown");
    want_prog(t, 11);
    let w: Vec<Want> = WANT.lock().unwrap().iter().filter(|e| e.t == t).copied().collect();
    assert_eq!(w.len(), 1, "one read queued");
    assert!(w[0].refresh && w[0].prog == 11, "read only at a draw of its program");
    want_prog(t, 11);
    assert_eq!(WANT.lock().unwrap().iter().filter(|e| e.t == t).count(), 1, "asking again queues nothing more");
    WANT.lock().unwrap().retain(|e| e.t != t);
    store(vec![(t, Some(tex(9)), 11, 56, 0, false)], true);
    assert!(get_versioned(t).is_none(), "another after-field read: still hidden");
    store(vec![(t, None, 11, 56, 0, false)], false);
    assert!(!awaits_draw(t));
    assert_eq!(get_versioned(t).map(|x| x.0.rgba[0][0]), Some(9), "a read at a draw shows it");
    let p = 0xdead_0131u64;
    store(vec![(p, Some(tex(3)), ANY_PROG, 1, 0, false)], true);
    assert!(!awaits_draw(p) && get_versioned(p).is_some(), "an any-draw want: as before");
}

/// S2: a class switched to a held word renews it: the renderer gets nothing until a read at or after the switch's GS field (an
/// earlier GS field's draw of that word shows the previous owner's texels); the want is not served before that field.
#[test]
fn a_renewed_texture_is_hidden_until_a_read_from_its_field() {
    let t = 0xdead_0020u64;
    store(vec![(t, Some(tex(1)), 11, 0, 0, false)], false);
    assert!(get_versioned(t).is_some());
    want_from(t, 11, 1_000_005);
    assert!(get_versioned(t).is_none(), "renewed: the renderer keeps its previous picture");
    let mut w = WANT.lock().unwrap().iter().filter(|e| e.t == t).copied().collect::<Vec<_>>();
    assert_eq!(w.len(), 1, "a read is queued");
    gs_field(1_000_004);
    assert!(take_wants(&mut w, [t, t], 11).is_empty(), "a draw before the switch's field does not serve it");
    store(vec![(t, Some(tex(2)), 11, 0, 0, false)], false);
    assert!(get_versioned(t).is_none(), "a read before the field does not confirm it");
    gs_field(1_000_005);
    assert_eq!(take_wants(&mut w, [t, t], 11), vec![(t, 11)]);
    store(vec![(t, Some(tex(3)), 11, 0, 0, false)], false);
    assert_eq!(get_versioned(t).map(|x| x.0.rgba[0][0]), Some(3));
    gs_field(0);
}

fn want_of(t: u64, prog: u8, refresh: bool) -> Want {
    Want { t, age: 0, refresh, prog, pin: prog, seen: NO_PROG, from: 0 }
}

/// S1: a program want is served by its program's PATH1 draws only; one its program does not draw for WANT_FIELDS fields while another
/// program's PATH1 draws use the word moves to that program (never to PATH2/3) and keeps the asker's program for the read.
#[test]
fn an_undrawn_program_want_moves_to_the_program_that_draws_it() {
    let t = 0x1234_5678u64;
    let mut w = vec![want_of(t, 11, false)];
    assert!(take_wants(&mut w, [t, t], ANY_PROG).is_empty(), "PATH2 never serves a program want");
    assert_eq!(w[0].seen, NO_PROG, "nor counts as the drawer");
    assert!(take_wants(&mut w, [t | CLD, t], 3).is_empty());
    assert_eq!(w[0].seen, 3);
    for _ in 0..WANT_FIELDS - 1 {
        assert_eq!(age_wants(&mut w), (vec![], vec![]));
    }
    assert_eq!(age_wants(&mut w), (vec![], vec![]), "moved, not read after the field");
    assert_eq!((w[0].prog, w[0].pin, w[0].age), (3, 11, 0));
    assert_eq!(take_wants(&mut w, [t, t], 3), vec![(t, 11)]);
    assert!(w.is_empty());
    // Nobody else drew it: a first read is read after the field, a refresh dropped.
    let mut w = vec![want_of(t, 11, false), want_of(t + 1, 11, true)];
    for _ in 0..WANT_FIELDS - 1 {
        age_wants(&mut w);
    }
    assert_eq!(age_wants(&mut w), (vec![(t, 11)], vec![t + 1]));
    assert!(w.is_empty());
}

/// The quick checks: CLD does not change a word's bit; spans have no heap and name the palette of an indexed format only.
#[test]
fn word_bits_ignore_cld_and_spans_follow_the_format() {
    let t8 = 0x8005_cfa5_dd30_b1d2u64;
    assert_eq!(bit(t8), bit(t8 & !CLD));
    assert_eq!(tex_spans(t8).len(), 2);
    assert_eq!(tex_spans(t8)[1].0, ((t8 >> 37) & 0x3fff) as u32);
    assert_eq!(tex_spans(t8 & !(0x3fu64 << 20)).len(), 1, "PSMCT32: texels only");
}

/// A render target's blocks from FRAME and SCISSOR: PAL 512 x 512 at FBP 0x80 (block 0x1000), FBW 8, rows 0..511.
#[test]
fn render_target_span_from_frame_and_scissor() {
    let frame = 0x80 | (8 << 16);
    let scissor = 511u64 << 48 | 511u64 << 16;
    assert_eq!(target_span(frame, scissor), Some((0x1000, 0x2000)));
    assert_eq!(target_span(0x80, scissor), None, "FBW 0: no target");
    // 448 rows: 14 page rows of 8 pages.
    assert_eq!(target_span(frame & !0x1ff, 447u64 << 48), Some((0, 0xe00)));
}

/// `Have::up`: a texture whose own blocks got an upload in the field before or the one being drawn counts as uploaded (the latest read
/// that took texels decides, [b46 U5]); one whose blocks were last written before that (boot art at block 0, an old frame) does not.
#[test]
fn an_upload_lately_is_told_from_old_texels() {
    assert!(!uploaded_since(0, 0), "never uploaded");
    assert!(uploaded_since(12, 10) && !uploaded_since(10, 10) && !uploaded_since(3, 10));
    let t = 0xdead_0030u64;
    assert_eq!(uploaded_lately(t), None, "not held: unknown");
    store(vec![(t, Some(tex(1)), 11, 0, 0, false)], false);
    mark_up(&[(t, false)]);
    assert_eq!(uploaded_lately(t), Some(false));
    mark_up(&[(t, true)]);
    assert_eq!(uploaded_lately(t), Some(true));
    mark_up(&[(t, false)]);
    assert_eq!(uploaded_lately(t), Some(false), "[b46 U5] not kept once seen: the latest read decides");
    // A re-read of the held bytes (texels None) keeps the note of the read that took them.
    let (same, new): (Read, Read) = ((t, None, 11, 0, 0, false), (t, Some(tex(2)), 11, 0, 0, false));
    assert!(fresh_ups(vec![(t, true)], &[same]).is_empty());
    assert_eq!(fresh_ups(vec![(t, true)], &[new]), vec![(t, true)]);
}

/// The alpha-plane guard (`keep_settled`): a settled PSMT8H texture in a frame buffer (R&C2's chrome environment map) keeps its
/// texels through a changed read with no upload since its read, and ALWAYS takes a read after an upload into its texels or palette;
/// a first read, a still-confirming one, other formats, textures outside a frame buffer and unchanged reads are never held back.
#[test]
fn alpha_plane_texels_change_only_with_an_upload() {
    let env = 0x8005_8305_ddb0_8b00u64 | 1 << 56; // PSMT8H, TBP 0xb00 (CSA 1: a key of its own)
    assert!(alpha_plane(env) && alpha_plane(env & !(0x3f << 20) | 0x24 << 20) && alpha_plane(env & !(0x3f << 20) | 0x2c << 20));
    let settle = |t: u64, c: u8| {
        for _ in 0..=VERIFY_READS {
            store(vec![(t, Some(tex(c)), ANY_PROG, u64::from(c), 5, false)], false);
        }
    };
    let read = |t: u64, c: u8| (t, Some(tex(c)), ANY_PROG, 100 + u64::from(c), 7, false);
    let in_fb = |tbp: u32| tbp < 0x1180; // frame buffer 0: 640 x 448 PSMCT32
    let never = |_: u64, _: u64| false;
    settle(env, 5);
    let v = version(env);
    let mut out = vec![read(env, 9)];
    assert_eq!(keep_settled(&mut out, in_fb, never), vec![env], "no upload since the read: kept");
    store(out, false);
    assert_eq!((version(env), get(env).unwrap().rgba[0][0]), (v, 5));
    assert!(get_confirmed(env).is_some_and(|c| c.1 == v), "the held picture stays confirmed");
    // An upload into its texels or palette after the held read (stamp 7 now): the new texels are taken.
    let mut out = vec![read(env, 9)];
    assert!(keep_settled(&mut out, in_fb, |t, since| t == env && since == 7).is_empty(), "a real upload is never blocked");
    store(out, false);
    assert_eq!((version(env), get(env).unwrap().rgba[0][0]), (v + 1, 9));
    // Unchanged bytes (no texels in the read) are not counted.
    let mut out = vec![(env, None, ANY_PROG, 109, 8, false)];
    assert!(keep_settled(&mut out, in_fb, never).is_empty());
    // Never held, or still confirming: the read counts.
    let first = env | 2 << 56;
    assert!(keep_settled(&mut [read(first, 3)], in_fb, never).is_empty(), "not held");
    store(vec![read(first, 3)], false);
    assert!(keep_settled(&mut [read(first, 4)], in_fb, never).is_empty(), "still confirming");
    // PSMT8 in a frame buffer, PSMT8H outside one: never held back.
    let t8 = env & !(0x3f << 20) | 0x13 << 20 | 3 << 56;
    let far = env & !0x3fff | 0x2000 | 4 << 56;
    settle(t8, 1);
    settle(far, 1);
    assert!(keep_settled(&mut [read(t8, 2), read(far, 2)], in_fb, never).is_empty());
}

/// [b51 G3] Review b50 M1: an `after` word is shown once N refresh wants were dropped undrawn while the pack still draws its class; a
/// word that is not `after`, or too few drops, is left alone.
#[test]
fn an_after_word_is_released_after_n_dropped_refresh_wants() {
    let mut x = Have { after: true, prog: 11, ..have(0, 0, 0) };
    for _ in 0..2 {
        x.after_miss += 1;
    }
    assert!(!release_after(&mut x, 3) && x.after, "two drops are not enough");
    x.after_miss += 1;
    assert!(release_after(&mut x, 3) && !x.after && x.after_miss == 0);
    assert!(!release_after(&mut x, 0), "a shown word is not 'released' again");
}
