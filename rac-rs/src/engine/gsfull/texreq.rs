//! R&C2: textures the R&C2 pack asks the software GS for: classes decoded from RAM (rac2 `scene::Scene::ram_classes`) carry the
//! GS TEX0 words the game wrote into their packets instead of level texture slots, so their texels are read from VRAM.
//! The renderer asks (`want`), the GS stage reads when it first draws with that TEX0 (`at_draw`; textures the game uploads
//! just before a draw and overwrites later in the field, like Ratchet's head at TBP 0) or, for one not drawn within a few
//! fields, after a field (`serve`); the renderer picks them up (`get`).
use super::gs::Gs;
use super::gs_mem::{clut_read, luts, rd_px, unpack_col};
use crate::level::Texture;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// TEX0 CLD bits (61..63): ignored when matching a draw's TEX0 with a wanted / held word.
const CLD: u64 = 7u64 << 61;

/// A wanted TEX0 word.
#[derive(Clone, Copy, Debug)]
struct Want {
    t: u64,
    /// Fields it has waited (counted from GS field `from` on).
    age: u32,
    /// A re-read of a texture already held: dropped undrawn instead of read after the field.
    refresh: bool,
    /// The VU1 program whose PATH1 draws read it (`ANY_PROG`: any draw).
    prog: u8,
    /// The program the asker named (`Have::prog` of the read; `prog` until the want moves).
    pin: u8,
    /// Another VU1 program's PATH1 draw used this word while it waited (`NO_PROG`: none): an undrawn want moves to that program.
    seen: u8,
    /// The first GS field (the host's field number, `gs_field`) whose draws may serve it (0: any).
    from: u64,
}
static WANT: Mutex<Vec<Want>> = Mutex::new(Vec::new());
/// `want_prog`'s "any draw".
pub const ANY_PROG: u8 = 0xff;
const NO_PROG: u8 = 0xfe;
static ANY: AtomicBool = AtomicBool::new(false);
/// One bit per wanted / held word (`bit`): a draw or skipped-run TEX0 whose bit is clear is neither, without a lock.
static WMASK: AtomicU64 = AtomicU64::new(0);
static HMASK: AtomicU64 = AtomicU64::new(0);

fn bit(t: u64) -> u64 {
    1u64 << ((t & !CLD).wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 58)
}

/// WANT changed: the per-draw quick checks follow it.
fn publish(w: &[Want]) {
    ANY.store(!w.is_empty(), Ordering::Relaxed);
    WMASK.store(w.iter().fold(0, |m, e| m | bit(e.t)), Ordering::Relaxed);
}

/// May this TEX0 be wanted or held (false: certainly neither)?
#[inline]
pub fn maybe(t: u64) -> bool {
    (WMASK.load(Ordering::Relaxed) | HMASK.load(Ordering::Relaxed)) & bit(t) != 0
}

/// The GS field being drawn (the host's field number; 0 = not told: every want is due).
static GS_AT: AtomicU64 = AtomicU64::new(0);

/// The GS stage starts drawing the field the host numbered `f` (`want_from`'s clock).
pub fn gs_field(f: u64) {
    GS_AT.store(f, Ordering::Relaxed);
}

/// May a draw of the GS field being drawn serve a want (or confirm a renewed texture) from GS field `from` on?
fn gs_reached(from: u64) -> bool {
    let at = GS_AT.load(Ordering::Relaxed);
    from == 0 || at == 0 || at >= from
}

/// A texture read from VRAM: its picture, how many times its texels changed on a refresh and the field it was read.
struct Have {
    tex: Arc<Texture>,
    ver: u32,
    at: u32,
    /// The field its texels last changed (or first read).
    changed: u32,
    /// Quick confirming re-reads still to do: a first read (or a change) can catch VRAM before the game's upload to the span or its
    /// palette finished, or while another texture still sat there (scattered-colour noise); it is read again within a few fields.
    verify: u8,
    /// The GS upload counter when it was read (`Gs::texreq_stamp`): a confirming re-read is skipped while nothing was uploaded into its
    /// texture's or palette's blocks since.
    read_gen: u64,
    /// Undrawn drops in a row (a scheduled re-read that no draw used) and the field before which it is not scheduled again.
    misses: u8,
    hold: u32,
    /// Which read filled it last (texcheck log): 0 first read at a draw, 1 first read after a field, 2 confirming re-read, 3 age refresh.
    src: u8,
    /// The VU1 program whose draws read it (`want_prog`; moves to the program that really draws it, `Want::seen`).
    prog: u8,
    /// `raw_hash` of the VRAM bytes it was decoded from: a re-read of equal bytes skips the decode.
    raw: u64,
    /// Renewed (`want_from`: a class switched to this held word): the renderer gets nothing (keeps its previous picture) until a read
    /// at or after this GS field (0: none).
    need: u64,
    /// A read found texels the game uploaded (or copied) into the texture's own blocks lately (`uploaded_since`; kept once seen): R&C2
    /// props.rs `reads_screen` tells a texture the game put into a frame buffer's blocks for its draw from a screen read.
    up: bool,
    /// [b50 V7b] Its texels came only from a read AFTER a field (a program want no draw of its program served): VRAM after the field
    /// holds what the game's texture ring parked there, not what the GS draws with that word [C: b50\handoff_V7.md H1, user's level-14
    /// save: 99.4-100 % of the texels differ]. Held but not shown (`get_versioned` None) until a read at a draw or mark clears it.
    after: bool,
    /// [b51 G3] Refresh wants of this `after` word dropped undrawn so far (`drop_undrawn`); `show_after` lets the pack show it at N.
    after_miss: u8,
}
/// Fields after a read before a confirming re-read is due, and how many confirmations a first read or a change needs.
const VERIFY_FIELDS: u32 = 2;
const VERIFY_READS: u8 = 3;
static HAVE: Mutex<Option<HashMap<u64, Have>>> = Mutex::new(None);
/// The GS upload counters are kept (the first `want` switches them on).
static TRACK: AtomicBool = AtomicBool::new(false);
/// Version base of new entries: a texture dropped by `prune` and read again must not look unchanged to a renderer that cached it.
static EPOCH: AtomicU32 = AtomicU32::new(0);
/// Re-reads scheduled per field (at most), of which refreshes keep up to REFRESH_QUOTA when both kinds are due.
const MAX_SCHED: usize = 4;
const REFRESH_QUOTA: usize = 2;
/// Undrawn drops after which a held texture is forgotten (~35 s of fields with the back-off: a previous level's textures).
const PRUNE_MISSES: u8 = 12;
/// Fields served so far (the refresh clock).
static FIELD: AtomicU32 = AtomicU32::new(0);
/// A held texture is read again (when a draw uses its TEX0) after this many fields: the game reuses VRAM blocks, so a TEX0
/// word read once ("first draw wins") showed black or foreign texels late in a session.
const REFRESH_FIELDS: u32 = 30;
/// A TEX0 whose texels just changed is not read again for this many fields (an animated or alternating texture would churn the
/// renderer's slot every refresh); refreshing never stops for good.
const MIN_CHANGE_GAP: u32 = 60;
/// Fields a want waits for a draw of its program before it moves to another program that drew the word, or is read after the field.
const WANT_FIELDS: u32 = 3;

/// Ask for the texture of a TEX0 word (read at the end of the next field the GS draws).
pub fn want(tex0: u64) {
    want_prog(tex0, ANY_PROG);
}

/// Ask for the texture of a TEX0 word that VU1 program `prog` draws with: it is read only at that program's PATH1 draws (or its skipped
/// runs' marks in overlay mode). The game reuses a TEX0 word within one field for other texels drawn by other paths [C: texcheck Aranos
/// field 0..600: TEX0 80059145dd30b1d2 drawn by a PATH2 packet over other texels than the moby program's draw of the same word]. A want
/// no draw of `prog` serves within WANT_FIELDS fields while another program's PATH1 draws use the word moves to that program.
pub fn want_prog(tex0: u64, prog: u8) {
    want_from(tex0, prog, 0);
}

/// `want_prog` for a word a class switched to at the host's field `from - 1`: the game thread runs ahead of the GS stage, so only draws of
/// GS field `from` on show the class's texels behind it. A held word is renewed: the renderer gets nothing for it (keeps its previous
/// picture, as for a first read) until it was read again at such a draw.
pub fn want_from(tex0: u64, prog: u8, from: u64) {
    TRACK.store(true, Ordering::Relaxed);
    if renew(tex0, from) && from == 0 {
        // [b50 V7b] Held from an after-field read only: read at its next draw (a refresh want: never read after a field again).
        if awaits_draw(tex0) {
            queue_draw(&[(tex0, prog)]);
        }
        return;
    }
    if let Ok(mut w) = WANT.lock() {
        if let Some(e) = w.iter_mut().find(|e| e.t == tex0 && e.pin == prog) {
            if from > e.from {
                (e.from, e.age) = (from, 0);
            }
            e.refresh = false;
            return;
        }
        w.push(Want { t: tex0, age: 0, refresh: false, prog, pin: prog, seen: NO_PROG, from });
        if std::env::var_os("RAC2_TEXLOG").is_some() {
            eprintln!("texreq: want {tex0:x} prog {prog} from {from}");
        }
        publish(&w);
    }
}

/// Does the GS keep its upload counters for `Gs::texreq_untouched` (a texture was asked for)?
#[inline]
pub fn tracking() -> bool {
    TRACK.load(Ordering::Relaxed)
}

/// Is this TEX0 wanted (by any program; CLD ignored)?
pub fn wanted(tex0: u64) -> bool {
    WMASK.load(Ordering::Relaxed) & bit(tex0) != 0 && WANT.lock().is_ok_and(|w| w.iter().any(|e| (e.t ^ tex0) & !CLD == 0))
}

/// Does a draw of program `prog` (ANY_PROG: not a VU1 program's draw) serve a want of program `want`?
fn serves(want: u8, prog: u8) -> bool {
    want == ANY_PROG || want == prog
}

/// A skipped 3D draw of VU1 program `prog` (HUD overlay mode, vif Out::texmarks) would draw with this TEX0 here: the uploads so far are
/// applied and a wanted texture is read as at a draw.
pub fn mark(g: &mut Gs, tex0: u64, prog: u8) {
    probe::at(g, tex0, prog); // [b50 V7b]
    if !wanted(tex0) {
        // A held texture the game uploaded new texels over since its read (a streamed moby texture): read at this draw, not one field
        // later through `schedule_touched`.
        let stale = HAVE.lock().is_ok_and(|h| h.as_ref().and_then(|m| m.get(&tex0)).is_some_and(|x| serves(x.prog, prog) && active(tex0, x) && g.texreq_uploaded(tex0, x.read_gen)));
        if !stale {
            return;
        }
        queue(vec![(tex0, prog)]);
    }
    take(g, [tex0, tex0], prog);
}

/// A held texture asked for again (a class switched back to this TEX0 word): its back-off after undrawn re-reads ends, so an upload
/// over it since its read is re-read at its next draw (`schedule_touched`) instead of after up to 8 * REFRESH_FIELDS; with `from` (a
/// class switched to it) it is renewed (`Have::need`). True if held.
fn renew(tex0: u64, from: u64) -> bool {
    let Ok(mut h) = HAVE.lock() else { return false };
    let Some(x) = h.as_mut().and_then(|m| m.get_mut(&tex0)) else { return false };
    (x.misses, x.hold) = (0, 0);
    if from > 0 {
        x.need = x.need.max(from);
    }
    true
}

/// [b50 V7b] Is this TEX0 held only from a read after a field (`Have::after`: not shown until a read at a draw)?
fn awaits_draw(tex0: u64) -> bool {
    HAVE.lock().is_ok_and(|h| h.as_ref().and_then(|m| m.get(&tex0)).is_some_and(|x| x.after))
}

/// [b50 V7b] Queues held words for a read at their next draw of their program as refresh wants (dropped undrawn after WANT_FIELDS
/// fields, never read after a field), unless a want of the word waits already. Not counted as upload re-reads (`TOUCHED`).
fn queue_draw(due: &[(u64, u8)]) {
    if due.is_empty() {
        return;
    }
    if let Ok(mut w) = WANT.lock() {
        let n = w.len();
        for &(t, prog) in due {
            if !w.iter().any(|e| e.t == t) {
                w.push(Want { t, age: 0, refresh: true, prog, pin: prog, seen: NO_PROG, from: 0 });
            }
        }
        if w.len() != n {
            publish(&w);
        }
    }
}

/// Is this TEX0 held (and in use) for draws of program `prog` (overlay marks: a held one is re-read at a mark when its blocks got an
/// upload)?
pub fn held_for(tex0: u64, prog: u8) -> bool {
    HMASK.load(Ordering::Relaxed) & bit(tex0) != 0 && HAVE.lock().is_ok_and(|h| h.as_ref().is_some_and(|m| m.get(&tex0).is_some_and(|x| serves(x.prog, prog) && active(tex0, x))))
}

/// Is a held texture re-read when the game uploads over it: one asked for by any draw (water, chrome) always, a program's one (a RAM
/// class's) while a class uses its TEX0 word (`set_in_use`): the streamed moby textures leave many words behind that no class draws
/// any more (on Oozla ~12 uploads over held textures a field).
fn active(t: u64, x: &Have) -> bool {
    x.prog == ANY_PROG || in_use(t)
}

/// Any texture held (overlay: skipped runs are scanned for held TEX0s too)?
pub fn any_held() -> bool {
    HMASK.load(Ordering::Relaxed) != 0
}

pub fn get(tex0: u64) -> Option<Arc<Texture>> {
    get_versioned(tex0).map(|x| x.0)
}

/// The texture and its version, read together (None while renewed and not read again yet: `Have::need`).
pub fn get_versioned(tex0: u64) -> Option<(Arc<Texture>, u32)> {
    HAVE.lock().ok()?.as_ref()?.get(&tex0).filter(|h| h.need == 0 && !h.after).map(|h| (h.tex.clone(), h.ver)) // [b50 V7b] after
}

/// `get_versioned` once the read is confirmed (`Have::verify` 0: the confirming re-reads are done) and not renewed: for a renderer that
/// copies the picture once and keeps it (R&C2's chrome environment map), which must not keep a first read or a change that caught VRAM
/// mid-upload or with other data in the blocks (it keeps its previous picture meanwhile).
pub fn get_confirmed(tex0: u64) -> Option<(Arc<Texture>, u32)> {
    HAVE.lock().ok()?.as_ref()?.get(&tex0).filter(|h| h.need == 0 && h.verify == 0 && !h.after).map(|h| (h.tex.clone(), h.ver)) // [b50 V7b] after
}

/// [b51 G3] Review b50 M1: a word held only from an after-field read (`Have::after`) is hidden until a draw reads it. A class the pack
/// DRAWS whose word no draw of its program ever serves (so `miss` counted `n` dropped refresh wants) must not stay invisible: the pack
/// asks (`tex_guard`) and the after-field texels are shown, as build 49 did. True when it was released by this call.
pub fn show_after(tex0: u64, n: u8) -> bool {
    HAVE.lock().ok().and_then(|mut h| h.as_mut().and_then(|m| m.get_mut(&tex0)).map(|x| release_after(x, n))).unwrap_or(false)
}

fn release_after(x: &mut Have, n: u8) -> bool {
    if x.after && x.after_miss >= n {
        (x.after, x.after_miss) = (false, 0);
        true
    } else {
        false
    }
}

/// Is this TEX0 held (read at least once, not forgotten by `prune` or `level_reset`)?
pub fn is_held(tex0: u64) -> bool {
    HAVE.lock().is_ok_and(|h| h.as_ref().is_some_and(|m| m.contains_key(&tex0)))
}

/// How often the texels of this TEX0 changed on a refresh (0 = as first read): a renderer that cached the picture rebuilds
/// when it differs from the number it saw.
pub fn version(tex0: u64) -> u32 {
    HAVE.lock().ok().and_then(|h| h.as_ref().and_then(|m| m.get(&tex0).map(|x| x.ver))).unwrap_or(0)
}

/// A new level: every held texture and want is forgotten (a word of the previous level names other texels now; the versions of
/// textures read again start from a new base).
pub fn level_reset() {
    probe::level_started(FIELD.load(Ordering::Relaxed)); // [b52 H1] review b51 m1
    if let Ok(mut w) = WANT.lock() {
        w.clear();
        publish(&w);
    }
    if let Ok(mut h) = HAVE.lock() {
        *h = None;
        HMASK.store(0, Ordering::Relaxed);
        EPOCH.fetch_add(1 << 20, Ordering::Relaxed);
    }
    super::texreq_screen::clear();
    super::texreq_mips::clear();
}

/// Stores freshly read textures (read when the GS upload counter was `stamp`): a refresh that reads other texels than the held ones
/// bumps the version.
/// (TEX0, texels or None when the VRAM bytes equal the held ones' (`raw_hash`), program, raw hash, upload stamp, mip levels changed).
type Read = (u64, Option<Arc<Texture>>, u8, u64, u64, bool);

fn store(out: Vec<Read>, after_field: bool) {
    let now = FIELD.load(Ordering::Relaxed);
    if let Ok(mut h) = HAVE.lock() {
        let m = h.get_or_insert_with(HashMap::new);
        for (t, x, prog, raw, stamp, levels) in out {
            match m.get_mut(&t) {
                Some(o) => {
                    o.src = if o.verify > 0 { 2 } else { 3 };
                    o.raw = raw;
                    if prog != ANY_PROG || o.prog == ANY_PROG {
                        o.prog = prog;
                    }
                    if o.need != 0 && gs_reached(o.need) {
                        o.need = 0;
                    }
                    if let Some(x) = x.filter(|x| levels || o.tex.rgba != x.rgba || o.tex.w != x.w || o.tex.h != x.h) {
                        if std::env::var_os("RAC2_TEXLOG").is_some() {
                            eprintln!("texreq: {t:x} changed (version {})", o.ver + 1);
                        }
                        o.tex = x;
                        o.ver += 1;
                        o.changed = now;
                        // A change found while still confirming keeps counting down (an animated or alternating texture is not
                        // confirmed forever, nor rebuilt every second field); one found on a settled texture starts the confirming.
                        o.verify = if o.verify > 0 { o.verify - 1 } else { VERIFY_READS };
                    } else {
                        o.verify = o.verify.saturating_sub(1);
                    }
                    o.at = now;
                    o.read_gen = stamp;
                    o.misses = 0;
                    o.hold = 0;
                    if !after_field {
                        o.after = false; // [b50 V7b] read at a draw / mark: the GS's own bytes
                        o.after_miss = 0;
                    }
                }
                None => {
                    let Some(x) = x else { continue };
                    let ver = EPOCH.load(Ordering::Relaxed);
                    // [b50 V7b] A program want's first read after a field is held, not shown (`Have::after`); ANY_PROG wants as before.
                    let after = after_field && prog != ANY_PROG;
                    m.insert(t, Have { tex: x, ver, at: now, changed: now, verify: VERIFY_READS, read_gen: stamp, misses: 0, hold: 0, src: u8::from(after_field), prog, raw, need: 0, up: false, after, after_miss: 0 });
                    HMASK.fetch_or(bit(t), Ordering::Relaxed);
                }
            }
        }
    }
}

/// PSMT8H / PSMT4HL / PSMT4HH: index texels in the top byte of 32-bit pixels (the alpha plane under a PSMCT24 frame buffer).
fn alpha_plane(tex0: u64) -> bool {
    matches!((tex0 >> 20) & 0x3f, 0x1b | 0x24 | 0x2c)
}

/// The upload stamp of the last read of a held, confirmed texture (None: not held, still confirming or renewed).
fn settled_gen(tex0: u64) -> Option<u64> {
    HAVE.lock().ok()?.as_ref()?.get(&tex0).filter(|h| h.verify == 0 && h.need == 0 && !h.after).map(|h| h.read_gen) // [b50 V7b] after
}

/// Turns the changed reads (`Read` texels Some) of settled alpha-plane textures whose texels lie in a render target (`in_fb`: the
/// first texel block) and got no upload or copy into their texels or palette since the held read (`uploaded(tex0, since)`) into
/// reads of unchanged texels: `store` keeps the held picture (the new bytes' hash is kept, so the same bytes are not decoded again;
/// an upload makes the next read count). R&C2's chrome environment map (TEX0 0x80058305ddb08b00: 128 x 128 PSMT8H at TBP 0xb00,
/// frame buffer 0's top byte) is uploaded once per level and never changes in a level [C: lane tex d1c, 61702 fields, one texel hash];
/// retail's PSMCT24 frames leave its byte alone, so other bytes there without an upload are VRAM the port changed or read at a wrong
/// moment (user build 43, Dobbo: helmet / Clank as speckle noise) [I]. Returns the words kept.
fn keep_settled(out: &mut [Read], in_fb: impl Fn(u32) -> bool, uploaded: impl Fn(u64, u64) -> bool) -> Vec<u64> {
    let mut kept = Vec::new();
    for r in out.iter_mut().filter(|r| r.1.is_some() && alpha_plane(r.0) && in_fb(tex_spans(r.0)[0].0)) {
        if settled_gen(r.0).is_some_and(|since| !uploaded(r.0, since)) {
            (r.1, r.5) = (None, false);
            kept.push(r.0);
        }
    }
    kept
}

/// Changed alpha-plane reads refused so far (`guard`; the first few are logged).
static KEPT: AtomicU32 = AtomicU32::new(0);

/// `keep_settled` against the GS's render targets (`in_target`) and upload counters; logs the first refusals (RAC2_TEXLOG: all).
fn guard(g: &Gs, out: &mut [Read]) {
    let kept = keep_settled(out, in_target, |t, since| g.texreq_uploaded(t, since));
    for t in kept {
        let n = KEPT.fetch_add(1, Ordering::Relaxed);
        if n < 4 || std::env::var_os("RAC2_TEXLOG").is_some() {
            eprintln!("texreq: kept {t:x}: its alpha-plane texels in a frame buffer changed with no upload (field {}, FRAME_1 {:x} FRAME_2 {:x} path {} prog {})", FIELD.load(Ordering::Relaxed), g.r[0x4c], g.r[0x4d], g.path, g.prog);
        }
    }
}

/// Is field `now` at or after `t` (wrapping)?
fn reached(now: u32, t: u32) -> bool {
    (now.wrapping_sub(t) as i32) >= 0
}

/// The held textures to queue for a re-read at their next draw (`now` = the field): confirmations first-read textures still need, and
/// age refreshes, each oldest read first, at most MAX_SCHED together; refreshes keep REFRESH_QUOTA of them when both are due. A texture
/// whose last scheduled re-read no draw used is held back (`miss`), so stale ones cannot pin the slots.
fn pick_due(m: &HashMap<u64, Have>, now: u32) -> Vec<u64> {
    let (mut verify, mut refresh) = (Vec::new(), Vec::new());
    for (&t, x) in m {
        if !reached(now, x.hold) {
            continue;
        }
        if x.verify > 0 && now.wrapping_sub(x.at) >= VERIFY_FIELDS {
            verify.push((x.at, t));
        } else if now.wrapping_sub(x.at) >= REFRESH_FIELDS && now.wrapping_sub(x.changed) >= MIN_CHANGE_GAP {
            refresh.push((x.at, t));
        }
    }
    verify.sort_unstable();
    refresh.sort_unstable();
    let vq = verify.len().min(MAX_SCHED - refresh.len().min(REFRESH_QUOTA));
    let rq = refresh.len().min(MAX_SCHED - vq);
    verify.iter().take(vq).chain(refresh.iter().take(rq)).map(|e| e.1).collect()
}

/// A scheduled re-read of this held texture was dropped undrawn: wait longer before the next one.
fn miss(x: &mut Have, now: u32) {
    x.misses = x.misses.saturating_add(1);
    x.hold = now.wrapping_add(REFRESH_FIELDS * u32::from(x.misses.min(8)));
}

/// Forgets textures whose re-reads were dropped undrawn PRUNE_MISSES times in a row (not drawn for a long time). True if any.
/// [b50 V7b] A word a pack class uses now (`used`: `set_in_use`) is kept: nothing asked for a forgotten word again, so the pack kept
/// the last texels it got for good (b50\handoff_V7.md H3); it is forgotten once no class uses it.
fn prune(m: &mut HashMap<u64, Have>, used: impl Fn(u64) -> bool) -> bool {
    let n = m.len();
    m.retain(|&t, x| x.misses < PRUNE_MISSES || used(t));
    let any = m.len() != n;
    if any {
        EPOCH.fetch_add(1 << 20, Ordering::Relaxed);
        HMASK.store(m.keys().fold(0, |b, &t| b | bit(t)), Ordering::Relaxed);
    }
    any
}

/// Held textures whose VRAM blocks (texels or palette) got an upload since they were read (`uploaded`: TEX0, entry), at field `now`;
/// ones backing off after undrawn re-reads (`hold`) wait. The game streams moby textures through a VRAM cache and reuses a TEX0 word for
/// other texels a few fields later [C: texcheck, Aranos TBP 0x3200..0x33ff]: such a texture is read again at its next draw instead of
/// after the REFRESH_FIELDS age refresh (kept as a backstop).
fn pick_touched(m: &HashMap<u64, Have>, now: u32, uploaded: impl Fn(u64, &Have) -> bool) -> Vec<(u64, u8)> {
    m.iter().filter(|e| reached(now, e.1.hold) && uploaded(*e.0, e.1)).map(|e| (*e.0, e.1.prog)).collect()
}

/// Re-reads queued because their blocks got an upload (texcheck log).
static TOUCHED: AtomicU32 = AtomicU32::new(0);

pub(crate) fn touched_count() -> u32 {
    TOUCHED.load(Ordering::Relaxed)
}

/// Once per field (before the age refresh): queues the held textures the game uploaded over for a re-read at their next draw (the
/// class-use set is read once per pass, the cheap tests first).
fn schedule_touched(g: &Gs) {
    let now = FIELD.load(Ordering::Relaxed);
    let due = match HAVE.lock() {
        Ok(h) => match h.as_ref() {
            Some(m) => {
                let u = IN_USE.lock();
                let used = |t: u64| u.as_ref().ok().and_then(|u| u.as_ref().map(|s| s.contains(&(t & !CLD)))).unwrap_or(true);
                pick_touched(m, now, |t, x| (x.prog == ANY_PROG || used(t)) && g.texreq_uploaded(t, x.read_gen))
            }
            None => return,
        },
        Err(_) => return,
    };
    queue(due);
}

/// Held textures with the program whose draws read them.
fn with_prog(m: &HashMap<u64, Have>, due: Vec<u64>) -> Vec<(u64, u8)> {
    due.into_iter().filter_map(|t| m.get(&t).map(|x| (t, x.prog))).collect()
}

/// A held texture whose mip levels moved (texreq_mips::set_bases): read again at its next draw of program `prog` (the mark that follows).
pub fn reread(tex0: u64, prog: u8) {
    queue(vec![(tex0, prog)]);
}

/// Queues held textures for a re-read at their next draw (refreshes: dropped, not read after the field, when no draw uses them).
fn queue(due: Vec<(u64, u8)>) {
    if due.is_empty() {
        return;
    }
    if let Ok(mut w) = WANT.lock() {
        let n = w.len();
        for (t, prog) in due {
            if !w.iter().any(|e| e.t == t) {
                w.push(Want { t, age: 0, refresh: true, prog, pin: prog, seen: NO_PROG, from: 0 });
                TOUCHED.fetch_add(1, Ordering::Relaxed);
            }
        }
        if w.len() != n {
            publish(&w);
        }
    }
}

/// Once per field: queues held textures for a re-read at their next draw.
fn schedule_refresh() {
    let now = FIELD.load(Ordering::Relaxed);
    let due: Vec<(u64, u8)> = match HAVE.lock() {
        Ok(h) => h.as_ref().map_or(Vec::new(), |m| with_prog(m, pick_due(m, now))),
        Err(_) => return,
    };
    queue(due);
}

/// A held texture for the texcheck detector (RAC2_TEXCHECK): the key it is held under (the class's TEX0 word; CLD ignored when matching),
/// its texels, version, which read filled it (`Have::src`), the field of that read and of its last change.
pub(crate) struct Held {
    pub key: u64,
    pub tex: Arc<Texture>,
    pub ver: u32,
    pub src: u8,
    pub at: u32,
    pub changed: u32,
}

pub(crate) fn held(tex0: u64) -> Option<Held> {
    let h = HAVE.lock().ok()?;
    let m = h.as_ref()?;
    let (&key, x) = m.get_key_value(&tex0).or_else(|| m.iter().find(|e| (e.0 ^ tex0) & !CLD == 0))?;
    Some(Held { key, tex: x.tex.clone(), ver: x.ver, src: x.src, at: x.at, changed: x.changed })
}

/// The TEX0 words the pack's classes use now (None until the pack says): only these program-wanted textures are re-read on uploads
/// (texcheck: a mismatch of another word is a stale key no class draws).
static IN_USE: Mutex<Option<std::collections::HashSet<u64>>> = Mutex::new(None);

/// The pack's classes now use these TEX0 words (CLD ignored).
pub fn set_in_use(words: impl Iterator<Item = u64>) {
    if let Ok(mut u) = IN_USE.lock() {
        *u = Some(words.map(|t| t & !CLD).collect());
    }
}

/// Does a class use this TEX0 word now (true when the pack never said)?
pub(crate) fn in_use(tex0: u64) -> bool {
    IN_USE.lock().ok().and_then(|u| u.as_ref().map(|s| s.contains(&(tex0 & !CLD)))).unwrap_or(true)
}

/// Does a class use this TEX0 word now (false while the pack never said; RAC2_TEXPROG)?
pub(crate) fn class_word(tex0: u64) -> bool {
    IN_USE.lock().ok().is_some_and(|u| u.as_ref().is_some_and(|s| s.contains(&(tex0 & !CLD))))
}

/// Program wants no draw of their program served (read after the field, or a re-read dropped undrawn), and wants moved to the program
/// that really drew their word (`Want::seen`).
static UNDRAWN: AtomicU32 = AtomicU32::new(0);
static MOVED: AtomicU32 = AtomicU32::new(0);

pub(crate) fn undrawn_count() -> (u32, u32) {
    (UNDRAWN.load(Ordering::Relaxed), MOVED.load(Ordering::Relaxed))
}

/// Fields served so far (the texture-request clock).
pub fn field() -> u32 {
    FIELD.load(Ordering::Relaxed)
}

/// Is any texture wanted (the GS's per-draw check)?
#[inline]
pub fn pending() -> bool {
    ANY.load(Ordering::Relaxed)
}

/// The GS is about to draw with these TEX0 words (TEX0_1, TEX0_2) and its uploads for them are applied: read any wanted
/// one now (a want of one VU1 program only at a PATH1 draw of that program). CLD (bits 61..63) is ignored when matching.
pub fn at_draw(g: &mut Gs, regs: [u64; 2]) {
    let prog = if g.path == 1 { g.prog } else { ANY_PROG };
    probe::at(g, regs[0], prog); // [b50 V7b]
    let m = WMASK.load(Ordering::Relaxed);
    if m & (bit(regs[0]) | bit(regs[1])) == 0 {
        return;
    }
    take(g, regs, prog);
}

/// Takes the wants of these TEX0 words that a draw of program `prog` serves out of `w` (due ones only: `gs_reached`); a PATH1 draw of
/// another program is noted on the want (`Want::seen`).
fn take_wants(w: &mut Vec<Want>, regs: [u64; 2], prog: u8) -> Vec<(u64, u8)> {
    let mut hit = Vec::new();
    w.retain_mut(|e| {
        if !regs.iter().any(|&r| (r ^ e.t) & !CLD == 0) || !gs_reached(e.from) {
            return true;
        }
        if serves(e.prog, prog) {
            hit.push((e.t, e.pin));
            return false;
        }
        if prog != ANY_PROG {
            e.seen = prog;
        }
        true
    });
    hit
}

/// Reads the wanted textures of these TEX0 words that a draw of program `prog` serves.
fn take(g: &mut Gs, regs: [u64; 2], prog: u8) {
    let hit = match WANT.lock() {
        Ok(mut w) => {
            let hit = take_wants(&mut w, regs, prog);
            if !hit.is_empty() {
                publish(&w);
            }
            hit
        }
        Err(_) => return,
    };
    // A confirming re-read of a texture nothing was uploaded over since its last read would return the same texels: counted as done
    // without the raster drain (`g.vram()` waits for every raster thread).
    let hit: Vec<(u64, u8)> = hit.into_iter().filter(|&(t, _)| !confirmed_unchanged(g, t)).collect();
    if hit.is_empty() {
        return;
    }
    if std::env::var_os("RAC2_TEXLOG").is_some() {
        eprintln!("texreq: at draw {hit:x?} path {} prog {} hud {}", g.path, g.prog, g.hud);
    }
    // The draw's own uploads are applied; the palette's (a separate upload) and anything queued before it too.
    g.flush_all();
    let stamps: Vec<u64> = hit.iter().map(|&(t, _)| g.texreq_stamp(t)).collect();
    let ups = uploads(g, hit.iter().map(|e| e.0));
    let vram = g.vram().as_ptr();
    // SAFETY: as in `serve`.
    let mut out: Vec<Read> = hit.into_iter().zip(stamps).map(|((t, p), s)| unsafe { read_new(vram, t, p, s) }).collect();
    guard(g, &mut out);
    if let Ok(d) = std::env::var("RAC2_TEXDUMP") {
        for (t, x, _, _, _, _) in &out {
            let Some(x) = x else { continue };
            let rgb = x.rgba.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
            let _ = std::fs::write(format!("{d}/tex_{t:016x}.png"), crate::render::png(&crate::render::Image { w: x.w, h: x.h, rgb }));
            let rgb = x.rgba.iter().flat_map(|p| [p[3], p[3], p[3]]).collect();
            let _ = std::fs::write(format!("{d}/alpha_{t:016x}.png"), crate::render::png(&crate::render::Image { w: x.w, h: x.h, rgb }));
            for (n, l) in super::texreq_mips::chain(*t).iter().flat_map(|c| c.iter()).enumerate() {
                let rgb = l.rgba.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
                let _ = std::fs::write(format!("{d}/tex_{t:016x}_l{}.png", n + 1), crate::render::png(&crate::render::Image { w: l.w, h: l.h, rgb }));
            }
        }
    }
    let ups = fresh_ups(ups, &out); // [b46 U5]
    store(out, false);
    mark_up(&ups);
}

/// A held texture that is still confirming and has seen no upload (or render) over its texture or palette since it was read: its
/// confirmations are done. True = no read needed (never for a renewed one: it needs a read to be shown again).
fn confirmed_unchanged(g: &Gs, t: u64) -> bool {
    let now = FIELD.load(Ordering::Relaxed);
    let Ok(mut h) = HAVE.lock() else { return false };
    let Some(x) = h.as_mut().and_then(|m| m.get_mut(&t)) else { return false };
    if x.verify == 0 || x.need != 0 || x.after || !g.texreq_untouched(t, x.read_gen) {
        // [b50 V7b] `after`: a read at a draw is what shows it.
        return false;
    }
    (x.verify, x.at, x.misses, x.hold) = (0, now, 0, 0);
    true
}

/// Ages the wants by one field (due ones only): a program want that waited WANT_FIELDS fields while another program's PATH1 draws
/// used its word is served by that program's next draw instead (a class drawn by another VU1 program than the one asked for is never
/// left unread; the read keeps the asker's program, `pin`) [C: RAC2_TEXPROG over one state of 16 levels: every class word drawn by
/// programs 3 / 7 / 8 was also drawn by 11 in the same fields (aliases), so this only serves classes off screen]; the rest that waited
/// that long leave, as (first reads to read after the field, refreshes dropped undrawn).
fn age_wants(w: &mut Vec<Want>) -> (Vec<(u64, u8)>, Vec<u64>) {
    for e in w.iter_mut().filter(|e| gs_reached(e.from)) {
        e.age += 1;
        if e.age >= WANT_FIELDS && e.prog != ANY_PROG && e.seen != NO_PROG {
            if std::env::var_os("RAC2_TEXLOG").is_some() {
                eprintln!("texreq: {:x} undrawn by prog {}: moves to prog {}", e.t, e.prog, e.seen);
            }
            (e.prog, e.seen, e.age) = (e.seen, NO_PROG, 0);
            MOVED.fetch_add(1, Ordering::Relaxed);
        }
    }
    let old: Vec<(u64, u8)> = w.iter().filter(|e| e.age >= WANT_FIELDS && !e.refresh).map(|e| (e.t, e.pin)).collect();
    let dropped: Vec<u64> = w.iter().filter(|e| e.age >= WANT_FIELDS && e.refresh).map(|e| e.t).collect();
    let n = w.iter().filter(|e| e.age >= WANT_FIELDS && e.prog != ANY_PROG).count() as u32;
    if n > 0 {
        UNDRAWN.fetch_add(n, Ordering::Relaxed);
    }
    w.retain(|e| e.age < WANT_FIELDS);
    (old, dropped)
}

/// The GS render targets seen lately (FRAME_1 at the end of a field, blocks [lo, hi) of its scissored rows), newest last: with the
/// CRTC's shown buffers, the frame buffers a class's screen-read TEX0 samples (R&C2 props.rs `reads_screen`; the native picture has
/// no 3D frame buffer, so texels there are stale VRAM). Context 2 is not a frame buffer: R&C2 leaves FRAME_2 inside its texture area
/// (blocks 0x3160.. / 0x32a0.., where the level's own 128 x 128 PSMT8 class textures sit: states 0373, 0472, round 3 batch 27c) [C].
static TARGETS: Mutex<Vec<(u32, u32)>> = Mutex::new(Vec::new());
const TARGETS_KEEP: usize = 4;

/// Notes context 1's render target (`serve`, once per field).
fn note_targets(g: &Gs) {
    let Ok(mut v) = TARGETS.lock() else { return };
    let Some(r) = target_span(g.r[0x4c], g.r[0x40]) else { return };
    if v.contains(&r) {
        return;
    }
    if std::env::var_os("RAC2_CRTCLOG").is_some() {
        eprintln!("crtc: render target ctx 1 blocks {:#x}..{:#x}", r.0, r.1);
    }
    if v.len() == TARGETS_KEEP {
        v.remove(0);
    }
    v.push(r);
}

/// FRAME_n and SCISSOR_n as the blocks [lo, hi) of a PSMCT32 render target (a page = 64 x 32 pixels = 32 blocks); None for FBW 0.
fn target_span(frame: u64, scissor: u64) -> Option<(u32, u32)> {
    let (fbp, fbw) = ((frame & 0x1ff) as u32 * 32, ((frame >> 16) & 0x3f) as u32);
    let rows = ((scissor >> 48) & 0x7ff) as u32 + 1;
    (fbw != 0).then(|| (fbp, fbp + fbw * rows.div_ceil(32) * 32))
}

/// Is block `tbp` inside a render target seen lately (`note_targets`)?
pub fn in_target(tbp: u32) -> bool {
    TARGETS.lock().is_ok_and(|v| v.iter().any(|&(lo, hi)| (lo..hi).contains(&tbp)))
}

/// Read wanted textures that no draw used for WANT_FIELDS fields from VRAM as it is after a field (GS stage thread).
pub fn serve(g: &mut Gs) {
    let f = FIELD.fetch_add(1, Ordering::Relaxed) + 1;
    note_targets(g);
    note_end(g.texreq_gen());
    probe::arm(f); // [b50 V7b]
    if super::texcheck::prog_on() {
        super::texcheck::prog_field(f);
    }
    if TRACK.load(Ordering::Relaxed) {
        schedule_touched(g);
    }
    schedule_refresh();
    let (want, dropped) = match WANT.lock() {
        Ok(mut w) if !w.is_empty() => {
            let r = age_wants(&mut w);
            publish(&w);
            r
        }
        _ => return,
    };
    drop_undrawn(&dropped);
    if want.is_empty() {
        return;
    }
    if std::env::var_os("RAC2_TEXLOG").is_some() {
        eprintln!("texreq: undrawn, read after the field {want:x?}");
    }
    let stamps: Vec<u64> = want.iter().map(|&(t, _)| g.texreq_stamp(t)).collect();
    let ups = uploads(g, want.iter().map(|e| e.0));
    let vram = g.vram().as_ptr();
    let mut out = Vec::new();
    for ((t, p), s) in want.into_iter().zip(stamps) {
        // SAFETY: `vram` is the GS's VRAM_SIZE bytes; `Gs::vram` finished every pending write.
        out.push(unsafe { read_new(vram, t, p, s) });
    }
    guard(g, &mut out);
    let ups = fresh_ups(ups, &out); // [b46 U5]
    let read: Vec<(u64, u8)> = out.iter().filter(|r| r.2 != ANY_PROG).map(|r| (r.0, r.2)).collect(); // [b50 V7b]
    store(out, true);
    mark_up(&ups);
    // [b50 V7b] The words now held only from this after-field read are read again at their next draw.
    let due: Vec<(u64, u8)> = read.into_iter().filter(|e| awaits_draw(e.0)).collect();
    queue_draw(&due);
}

/// The GS upload counter at the end of each of the last LATE_FIELDS fields served, oldest first (`serve`).
static ENDS: Mutex<[u64; LATE_FIELDS]> = Mutex::new([0; LATE_FIELDS]);
/// Uploads this many fields back (the field before and the one being drawn) count as "lately": the game uploads a texture it draws
/// from a frame buffer's blocks in the field it draws it (the next picture drawn there overwrites it).
const LATE_FIELDS: usize = 2;

/// `serve`: a field ended with the upload counter at `up_gen`.
fn note_end(up_gen: u64) {
    if let Ok(mut e) = ENDS.lock() {
        e.rotate_left(1);
        e[LATE_FIELDS - 1] = up_gen;
    }
}

/// Was the last upload into a texture's blocks (counter `last`, 0 = none counted) after the field-end counter `late`?
fn uploaded_since(last: u64, late: u64) -> bool {
    last > late
}

/// Per TEX0 about to be read: did the game upload (or copy) texels into its own blocks lately (`uploaded_since` the end of the field
/// LATE_FIELDS fields back; the palette not counted)?
fn uploads(g: &Gs, words: impl Iterator<Item = u64>) -> Vec<(u64, bool)> {
    let late = ENDS.lock().map_or(0, |e| e[0]);
    words.map(|t| (t, uploaded_since(g.texreq_tex_upload(t), late))).collect()
}

/// Notes on the held textures just read whether their texels were uploaded lately (`Have::up`): the latest read that took texels decides.
/// [b46 U5] Was `|=` (kept once seen): one upload into block 0 by any texture made every later read of a TEX0 there "uploaded", so a
/// frame buffer read through a CLUT drew as noise for ever (level 14 helper robots, handoff_U5 fact 1).
fn mark_up(ups: &[(u64, bool)]) {
    let Ok(mut h) = HAVE.lock() else { return };
    let Some(m) = h.as_mut() else { return };
    for &(t, up) in ups {
        if let Some(x) = m.get_mut(&t) {
            x.up = up;
        }
    }
}

/// [b46 U5] The upload notes of the reads that took texels (`Read` texels Some: a first read or changed bytes): a re-read of the very
/// bytes held (or one `guard` kept) still shows the texels of the read that took them, so its note stays.
fn fresh_ups(ups: Vec<(u64, bool)>, out: &[Read]) -> Vec<(u64, bool)> {
    ups.into_iter().filter(|u| out.iter().any(|r| r.0 == u.0 && r.1.is_some())).collect()
}

/// Were the held texels of this TEX0 uploaded by the game lately when read (`Have::up`)? None while not held (or renewed and not read
/// again: `get_versioned` gives nothing either).
pub fn uploaded_lately(tex0: u64) -> Option<bool> {
    HAVE.lock().ok()?.as_ref()?.get(&tex0).filter(|h| h.need == 0 && !h.after).map(|h| h.up) // [b50 V7b] after
}

/// Scheduled re-reads that no draw used: the held textures wait longer (stale ones rotate to the back instead of taking the slots of
/// the fresh ones every field), and long-unused ones are forgotten.
fn drop_undrawn(list: &[u64]) {
    if list.is_empty() {
        return;
    }
    let now = FIELD.load(Ordering::Relaxed);
    if let Ok(mut h) = HAVE.lock() {
        if let Some(m) = h.as_mut() {
            for t in list {
                if let Some(x) = m.get_mut(t) {
                    miss(x, now);
                    x.after_miss = x.after_miss.saturating_add(u8::from(x.after)); // [b51 G3]
                }
            }
            // [b50 V7b] Lock order HAVE -> IN_USE (as `schedule_touched`); a pack that never said keeps the old rule.
            let u = IN_USE.lock();
            prune(m, |t| u.as_ref().ok().and_then(|u| u.as_ref().map(|s| s.contains(&(t & !CLD)))).unwrap_or(false));
        }
    }
}

/// The VRAM block spans [lo, hi) a TEX0 reads (`tex_spans`): its texels and, for an indexed format, its palette (no heap per call).
pub(crate) struct Spans {
    s: [(u32, u32); 2],
    n: usize,
}

impl std::ops::Deref for Spans {
    type Target = [(u32, u32)];
    fn deref(&self) -> &[(u32, u32)] {
        &self.s[..self.n]
    }
}

/// The VRAM block spans [lo, hi) a TEX0 reads: its texels and, for an indexed format, its palette.
pub(crate) fn tex_spans(tex0: u64) -> Spans {
    let f = |sh: u32, bits: u32| ((tex0 >> sh) & ((1 << bits) - 1)) as u32;
    let (tbp, tbw, psm) = (f(0, 14), f(14, 6), f(20, 6));
    let (w, h) = (1u32 << f(26, 4).min(10), 1u32 << f(30, 4).min(10));
    let mut s = Spans { s: [super::gs_mem::span(psm, tbp, tbw, 0, 0, w, h), (0, 0)], n: 1 };
    if matches!(psm, 0x13 | 0x14 | 0x1b | 0x24 | 0x2c) {
        let cbp = f(37, 14);
        (s.s[1], s.n) = ((cbp, cbp + 4), 2);
    }
    s
}

/// Hash of the VRAM bytes a TEX0 decodes from (`tex_spans`): equal bytes give equal texels.
/// SAFETY: `m` points at VRAM_SIZE bytes.
unsafe fn raw_hash(m: *const u8, t: u64) -> u64 {
    use std::hash::Hasher;
    let v = std::slice::from_raw_parts(m, super::gs_mem::VRAM_SIZE);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    // The mip levels the pack registered (texreq_mips) count too: a level the game re-uploads is a change.
    for w in std::iter::once(t).chain(super::texreq_mips::level_words(t)) {
        for &(lo, hi) in tex_spans(w).iter() {
            let (a, b) = ((lo as usize * 256).min(v.len()), (hi as usize * 256).min(v.len()));
            h.write(&v[a..b.max(a)]);
            h.write_u64((lo as u64) << 32 | hi as u64);
        }
    }
    h.finish()
}

/// A read of `t` for `store` (upload stamp `stamp`): decoded only when its VRAM bytes differ from the held texture's (the game
/// re-uploads a streamed texture every field; most re-reads find the same bytes). SAFETY: as `read`.
unsafe fn read_new(m: *const u8, t: u64, prog: u8, stamp: u64) -> Read {
    let raw = raw_hash(m, t);
    // SAFETY: as this fn's.
    unsafe { super::texreq_screen::keep_clut(m, t) };
    let same = HAVE.lock().is_ok_and(|h| h.as_ref().and_then(|x| x.get(&t)).is_some_and(|x| x.raw == raw));
    // SAFETY: as this fn's. A level 1..MXL that changed is a change of the texture even when level 0 kept its texels.
    let levels = !same && unsafe { super::texreq_mips::read_chain(m, t) };
    (t, (!same).then(|| Arc::new(read(m, t))), prog, raw, stamp, levels)
}

/// RGBA of a TEX0 (PSMCT32, PSMT8 / T8H and PSMT4 / T4HL / T4HH with a CT32 / CT16 CLUT; other formats white), PS2 alpha 0x80 = 255. TCC 0
/// (texture alpha unused) reads opaque.
pub(crate) unsafe fn read(m: *const u8, t: u64) -> Texture {
    let f = |sh: u32, bits: u32| ((t >> sh) & ((1 << bits) - 1)) as u32;
    let (tbp, tbw, psm) = (f(0, 14), f(14, 6), f(20, 6));
    let (w, h) = (1u32 << f(26, 4).min(10), 1u32 << f(30, 4).min(10));
    let (tcc, cbp, cpsm, csa) = (f(34, 1), f(37, 14), f(51, 4), f(56, 5));
    let t4 = matches!(psm, 0x14 | 0x24 | 0x2c);
    let mut clut = [[0u8; 4]; 512];
    let texa = 0x80 | 0x80 << 32;
    if matches!(psm, 0x13 | 0x14 | 0x1b | 0x24 | 0x2c) {
        clut_read(m, &mut clut, cbp, if cpsm & 2 != 0 { 0x02 } else { 0x00 }, t4, csa as usize * 16, texa);
    }
    let l = luts();
    let mut rgba = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let c = match psm {
                0x00 => unpack_col(0, rd_px(m, l, 0, tbp, tbw, x, y), texa),
                0x13 | 0x1b => clut[rd_px(m, l, psm, tbp, tbw, x, y) as usize & 0xff],
                0x14 | 0x24 | 0x2c => clut[csa as usize * 16 + (rd_px(m, l, psm, tbp, tbw, x, y) as usize & 0xf)],
                _ => [255, 255, 255, 0x80],
            };
            let a = if tcc == 0 { 255 } else { (c[3] as u32 * 2).min(255) as u8 };
            rgba.push([c[0], c[1], c[2], a]);
        }
    }
    // Debug: RAC2_TEXSOLID=TEX0hex:RRGGBB,.. paints those textures one colour (which faces use which texture).
    static SOLID: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    if let Some(v) = SOLID.get_or_init(|| std::env::var("RAC2_TEXSOLID").ok()) {
        for e in v.split(',') {
            if let Some((k, c)) = e.split_once(':') {
                if u64::from_str_radix(k, 16).ok() == Some(t) {
                    let c = u32::from_str_radix(c, 16).unwrap_or(0xff0000);
                    rgba.iter_mut().for_each(|p| *p = [(c >> 16) as u8, (c >> 8) as u8, c as u8, 255]);
                }
            }
        }
    }
    Texture { w: w as usize, h: h as usize, rgba }
}

/// [b50 V7b] The VRAM-side sampled self-check (a child module: it reads the held textures).
#[path = "texreq_probe.rs"]
mod probe;

#[cfg(test)]
#[path = "texreq_tests.rs"]
mod tests;
