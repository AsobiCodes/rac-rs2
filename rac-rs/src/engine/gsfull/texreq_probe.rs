//! [b50 V7b] texreq's VRAM-side self-check: a held texture that is shown (confirmed, not renewed, not after-field only) and that a
//! pack class uses now is compared, a few times a second, with the VRAM bytes at a real draw (or overlay mark) of its word. One log
//! line per word (at most MAX_LINES per run) when the shown texels are stale while texreq thinks they are right:
//! `texreq: texture stale TEX0 <t> field <f> cache texreq-vram: <why>`.
//! The prefix is the engine's own module name: the engine's `Pack` has no game-name hook, and the engine names no game.
//! Cost: one atomic load per draw / mark while nothing is armed; once per SAMPLE_FIELDS fields one map pass (arm) and, at the armed
//! word's next draw, one VRAM drain plus one hash of its bytes (as a texreq read without the decode).
use super::*;
use std::sync::atomic::AtomicU8;

/// Fields between two samples (4 a second at 60 fields).
const SAMPLE_FIELDS: u32 = 15;
/// [b51 G3] No sample before this field of the texreq clock: right after a state load / level start the first reads are of VRAM the
/// load still rearranges (all 35 noisy tour runs logged at fields 15..90 with right pictures, tour\out_b50).
const WARMUP_FIELDS: u32 = 150;
/// [b52 H1] The texreq field of the last level load (`level_reset`; 0 = the exe's start): the warm-up counts from it (review b51 m1).
static LEVEL_FIELD: AtomicU32 = AtomicU32::new(0);
/// Log lines per run at most.
const MAX_LINES: u32 = 32;
/// The armed word (0: none) and the program whose draw checks it.
static ARMED: AtomicU64 = AtomicU64::new(0);
static ARMED_PROG: AtomicU8 = AtomicU8::new(ANY_PROG);
static LINES: AtomicU32 = AtomicU32::new(0);
static LOGGED: Mutex<Vec<u64>> = Mutex::new(Vec::new());

/// The k-th (wrapping) held texture that may be checked: a program's (a pack class's) word in use now (`used`), shown (`need` 0, not
/// `after`) and confirmed. Its word and program.
fn pick(m: &HashMap<u64, Have>, used: impl Fn(u64) -> bool, k: usize) -> Option<(u64, u8)> {
    let ok = |e: &(&u64, &Have)| e.1.prog != ANY_PROG && e.1.need == 0 && !e.1.after && e.1.verify == 0 && used(*e.0);
    let n = m.iter().filter(ok).count();
    if n == 0 {
        return None;
    }
    m.iter().filter(ok).nth(k % n).map(|e| (*e.0, e.1.prog))
}

/// [b52 H1] `texreq::level_reset` at texreq field `f`: no sample for WARMUP_FIELDS fields from here.
pub(super) fn level_started(f: u32) {
    LEVEL_FIELD.store(f, Ordering::Relaxed);
}

/// [b52 H1] Is field `f` inside the warm-up that started at field `start` (wrapping field clock)?
fn warming(f: u32, start: u32) -> bool {
    f.wrapping_sub(start) < WARMUP_FIELDS
}

/// `serve`, field `f`: every SAMPLE_FIELDS fields one word is armed (a word no draw used since the last arming is replaced).
pub(super) fn arm(f: u32) {
    if f % SAMPLE_FIELDS != 0 || warming(f, LEVEL_FIELD.load(Ordering::Relaxed)) {
        return;
    }
    let Ok(h) = HAVE.lock() else { return };
    let Some(m) = h.as_ref() else { return };
    // Lock order HAVE -> IN_USE (as `schedule_touched`). A pack that never said which words its classes use: nothing is checked.
    let Ok(u) = IN_USE.lock() else { return };
    let Some(s) = u.as_ref() else { return };
    if let Some((t, p)) = pick(m, |t| s.contains(&(t & !CLD)), (f / SAMPLE_FIELDS) as usize) {
        ARMED_PROG.store(p, Ordering::Relaxed);
        ARMED.store(t, Ordering::Relaxed);
    }
}

/// What the check needs of a held texture: the hash of the bytes it was read from, its upload stamp, the field of its read and
/// whether its re-reads back off now (`hold` not reached).
struct State {
    raw: u64,
    stamp: u64,
    at: u32,
    held_back: bool,
}

fn state(t: u64, now: u32) -> Option<State> {
    let h = HAVE.lock().ok()?;
    let x = h.as_ref()?.get(&t).filter(|x| x.need == 0 && !x.after && x.verify == 0)?;
    Some(State { raw: x.raw, stamp: x.read_gen, at: x.at, held_back: !reached(now, x.hold) })
}

/// A draw (or overlay mark) of program `prog` with TEX0 `t`, its uploads applied: the armed word is checked here.
pub(super) fn at(g: &mut Gs, t: u64, prog: u8) {
    let a = ARMED.load(Ordering::Relaxed);
    if a == 0 || (a ^ t) & !CLD != 0 || !serves(ARMED_PROG.load(Ordering::Relaxed), prog) {
        return;
    }
    ARMED.store(0, Ordering::Relaxed);
    let now = FIELD.load(Ordering::Relaxed);
    let Some(s) = state(a, now) else { return };
    let why = if g.texreq_untouched(a, s.stamp) {
        "no upload since its read, other bytes at this draw (a VRAM write the upload counters missed)"
    } else if s.held_back && g.texreq_uploaded(a, s.stamp) && !in_target(tex_spans(a)[0].0) {
        "uploaded over since its read while its re-reads back off (the old texels are shown until a re-read)"
    } else {
        // Uploaded since the read and not backing off: `schedule_touched` re-reads it at this or the next draw.
        return;
    };
    g.flush_all();
    // SAFETY: `Gs::vram` is the GS's VRAM_SIZE bytes with every pending write finished (as in `serve`).
    let raw = unsafe { raw_hash(g.vram().as_ptr(), a) };
    if raw != s.raw {
        log(a, now, &format!("{why} (prog {prog}, read at field {})", s.at));
    }
}

fn log(t: u64, now: u32, what: &str) {
    let Ok(mut l) = LOGGED.lock() else { return };
    if l.contains(&t) || LINES.fetch_add(1, Ordering::Relaxed) >= MAX_LINES {
        return;
    }
    l.push(t);
    eprintln!("texreq: texture stale TEX0 {t:016x} field {now} cache texreq-vram: {what}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn have(prog: u8, verify: u8, need: u64, after: bool) -> Have {
        let tex = Arc::new(Texture { w: 1, h: 1, rgba: vec![[0; 4]] });
        Have { tex, ver: 0, at: 0, changed: 0, verify, read_gen: 0, misses: 0, hold: 0, src: 0, prog, raw: 0, need, up: false, after, after_miss: 0 }
    }

    /// [b52 H1] The warm-up counts from the last level load, not from the exe's start.
    #[test]
    fn the_warm_up_counts_from_the_last_level_load() {
        assert!(warming(149, 0) && !warming(150, 0));
        assert!(warming(1000 + 149, 1000) && !warming(1000 + 150, 1000));
        assert!(!warming(5, u32::MAX - 400), "wrapping clock far past the start");
    }

    /// Only shown, confirmed program words a class uses now are checked; the pick rotates over them.
    #[test]
    fn the_probe_checks_only_shown_class_words_in_rotation() {
        let mut m = HashMap::new();
        m.insert(1, have(11, 0, 0, false));
        m.insert(2, have(11, 0, 0, false));
        m.insert(3, have(ANY_PROG, 0, 0, false)); // water / chrome: not a class word
        m.insert(4, have(11, 2, 0, false)); // still confirming
        m.insert(5, have(11, 0, 9, false)); // renewed
        m.insert(6, have(11, 0, 0, true)); // after-field only
        m.insert(7, have(11, 0, 0, false)); // no class uses it
        let used = |t: u64| t != 7;
        let mut got: Vec<u64> = (0..4).filter_map(|k| pick(&m, used, k).map(|e| e.0)).collect();
        got.sort_unstable();
        assert_eq!(got, vec![1, 1, 2, 2], "both checkable words, in turn");
        assert!(pick(&m, |_| false, 0).is_none());
    }
}
