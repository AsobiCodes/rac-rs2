//! R&C2 debug detector (RAC2_TEXCHECK=1): do the texels the native renderer holds for a GS texture (`texreq`, the RAM classes' TEX0
//! words) equal what the emulated GS draws with that TEX0? At every textured draw (the real GS draw in the emulated picture; in the
//! native picture's overlay mode the world programs are skipped, so each skipped run is also run on a copy of VU1 and its GIF output is
//! parsed for the textured prims' TEX0s, `truth_run`) the texture is decoded from GS memory as it is at that point of the stream, with
//! the palette the GS's CLUT buffer really holds (`ClutTrack`), and compared with the held one. Logs every new mismatch (field, TEX0,
//! version, which read filled it, % texels that differ), TEX0s drawn with two different contents in one field ("ALIAS"), and a
//! summary every 100 fields. Off: no cost.
use super::gs::Gs;
use super::texreq;
use super::vif::Out;
use super::vu1::Vu1;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

pub fn on() -> bool {
    static C: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *C.get_or_init(|| std::env::var_os("RAC2_TEXCHECK").is_some_and(|v| v != "0"))
}

/// CBP + CPSM bits of a TEX0.
const PAL_MASK: u64 = 0x3ffff << 37;
const CLD_MASK: u64 = 7 << 61;

/// Per decoded TEX0: the upload stamp, held version and result of the last decode (reused while nothing touched its blocks).
struct Last {
    stamp: u64,
    ver: u32,
    hash: u64,
    diff: u32,
}

#[derive(Default)]
struct St {
    last: HashMap<u64, Last>,
    /// TEX0 -> decoded hash drawn this field.
    seen: HashMap<u64, u64>,
    field: u32,
    /// (TEX0, version, drawn hash) mismatches already logged.
    logged: HashSet<(u64, u32, u64)>,
    checks: u64,
    bad: u64,
    bad_keys: HashSet<u64>,
    alias: u64,
    field_bad: u32,
    /// Mismatches of TEX0s a native class uses now (`texreq::in_use`): the ones the picture shows.
    bad_used: u64,
}

static ST: Mutex<Option<St>> = Mutex::new(None);

const SRC: [&str; 4] = ["first-at-draw", "first-after-field", "verify", "refresh"];

fn hash(t: &crate::level::Texture) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (t.w, t.h).hash(&mut h);
    t.rgba.hash(&mut h);
    h.finish()
}

/// The GS is about to draw a textured prim with `tex0` (`how`: 0 a real GS draw, 1 a skipped overlay-mode run's draw); `eff` is the same
/// TEX0 with the CBP / CPSM of the palette the GS's CLUT buffer really holds for it (`ClutTrack`).
pub fn check(g: &mut Gs, tex0: u64, eff: u64, how: u8) {
    let Some(h) = texreq::held(tex0) else { return };
    let field = texreq::field();
    let Ok(mut lk) = ST.lock() else { return };
    let st = lk.get_or_insert_with(St::default);
    if st.field != field {
        end_field(st, field);
    }
    st.checks += 1;
    let dk = h.key & !(PAL_MASK | CLD_MASK) | eff & PAL_MASK;
    let reuse = st.last.get(&dk).is_some_and(|l| l.ver == h.ver && g.texreq_untouched(dk, l.stamp));
    let (hs, diff) = if reuse {
        let l = &st.last[&dk];
        (l.hash, l.diff)
    } else {
        g.flush_all();
        let stamp = g.texreq_gen();
        // SAFETY: `Gs::vram` finished every pending write; VRAM_SIZE bytes.
        let t = unsafe { texreq::read(g.vram().as_ptr(), dk) };
        let diff = if t.w != h.tex.w || t.h != h.tex.h { u32::MAX } else { t.rgba.iter().zip(&h.tex.rgba).filter(|(a, b)| a != b).count() as u32 };
        let hs = hash(&t);
        if diff != 0 {
            dump(&t, &h, field, hs);
        }
        st.last.insert(dk, Last { stamp, ver: h.ver, hash: hs, diff });
        (hs, diff)
    };
    let mode = if how == 0 { "gs" } else { "ovl" };
    if let Some(&o) = st.seen.get(&h.key) {
        if o != hs {
            st.alias += 1;
            if st.logged.insert((h.key, u32::MAX, hs)) {
                eprintln!("texcheck: f{field} ALIAS t={:x} drawn with two contents in one field ({mode})", h.key);
            }
        }
    } else {
        st.seen.insert(h.key, hs);
    }
    if diff == 0 {
        return;
    }
    let used = texreq::in_use(h.key);
    st.bad += 1;
    st.bad_used += u64::from(used);
    st.field_bad += 1;
    st.bad_keys.insert(h.key);
    if st.logged.insert((h.key, h.ver, hs)) {
        let n = (h.tex.w * h.tex.h) as u32;
        let pct = if diff == u32::MAX { 100.0 } else { diff as f64 * 100.0 / n.max(1) as f64 };
        let (tw, th) = (1u32 << ((tex0 >> 26) & 15), 1u32 << ((tex0 >> 30) & 15));
        let cnt = if diff == u32::MAX { "size".to_string() } else { format!("{diff}/{n}") };
        let pal = if dk & PAL_MASK != h.key & PAL_MASK { format!(" CLUT-BUFFER cbp {:x} (TEX0 cbp {:x})", (dk >> 37) & 0x3fff, (h.key >> 37) & 0x3fff) } else { String::new() };
        eprintln!(
            "texcheck: f{field} MISMATCH t={:x} psm {:x} {tw}x{th} ver {} src {} read f{} changed f{} differ {pct:.1}% ({cnt}) {mode}{pal}{}",
            h.key,
            (tex0 >> 20) & 0x3f,
            h.ver,
            SRC[h.src as usize & 3],
            h.at,
            h.changed,
            if used { " IN-USE" } else { " stale-key" },
        );
    }
}

/// Debug (RAC2_TEXCHECK_DUMP=dir): the drawn and the held texels of a mismatch as PNGs (at most 300 drawn ones).
fn dump(t: &crate::level::Texture, h: &texreq::Held, field: u32, hs: u64) {
    static DIR: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let Some(d) = DIR.get_or_init(|| std::env::var("RAC2_TEXCHECK_DUMP").ok()) else { return };
    if N.fetch_add(1, std::sync::atomic::Ordering::Relaxed) >= 300 {
        return;
    }
    let png = |x: &crate::level::Texture, p: String| {
        let rgb = x.rgba.iter().flat_map(|c| [c[0], c[1], c[2]]).collect();
        let _ = std::fs::write(p, crate::render::png(&crate::render::Image { w: x.w, h: x.h, rgb }));
    };
    png(t, format!("{d}/drawn_{:016x}_f{field:05}_{:08x}.png", h.key, hs as u32));
    let hp = format!("{d}/held_{:016x}_v{}.png", h.key, h.ver);
    if !std::path::Path::new(&hp).exists() {
        png(&h.tex, hp);
    }
}

fn end_field(st: &mut St, field: u32) {
    if st.field_bad > 0 {
        eprintln!("texcheck: field {} had {} mismatching draws", st.field, st.field_bad);
    }
    if field / 100 != st.field / 100 {
        eprintln!("texcheck: SUMMARY to f{field}: checks {} mismatching {} (in use {}) keys {} alias {} upload re-reads {}", st.checks, st.bad, st.bad_used, st.bad_keys.len(), st.alias, texreq::touched_count());
    }
    st.seen.clear();
    st.field = field;
    st.field_bad = 0;
}

/// Debug (RAC2_TEXPROG=1): which GS path / VU1 program draws the TEX0 words the pack's classes use (`texreq::class_word`), counted at
/// every textured draw; a summary every 100 fields (run with RAC2_PICTURE=gs so every program really draws).
pub fn prog_on() -> bool {
    static C: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *C.get_or_init(|| std::env::var_os("RAC2_TEXPROG").is_some_and(|v| v != "0"))
}

#[derive(Default)]
struct ProgSt {
    /// (path, prog) -> (draws, words).
    by: std::collections::BTreeMap<(u8, u8), (u64, HashSet<u64>)>,
    /// word -> PATH1 programs that drew it.
    progs: HashMap<u64, std::collections::BTreeSet<u8>>,
    field: u32,
}

static PROG: Mutex<Option<ProgSt>> = Mutex::new(None);

/// A textured draw of `tex0` by `path` / `prog` (RAC2_TEXPROG).
pub fn prog_draw(path: u8, prog: u8, tex0: u64) {
    let t = tex0 & !CLD_MASK;
    if !texreq::class_word(t) {
        return;
    }
    let Ok(mut s) = PROG.lock() else { return };
    let s = s.get_or_insert_with(ProgSt::default);
    let e = s.by.entry((path, if path == 1 { prog } else { 0xff })).or_default();
    e.0 += 1;
    e.1.insert(t);
    if path == 1 {
        s.progs.entry(t).or_default().insert(prog);
    }
}

/// Field end (RAC2_TEXPROG): the summary every 100 fields.
pub fn prog_field(field: u32) {
    let Ok(mut s) = PROG.lock() else { return };
    let s = s.get_or_insert_with(ProgSt::default);
    if field / 100 != s.field / 100 {
        for (&(p, g), (n, w)) in &s.by {
            eprintln!("texprog: to f{field} path {p} prog {g}: {n} draws of {} class words", w.len());
        }
        let multi: Vec<_> = s.progs.iter().filter(|e| e.1.len() > 1).take(8).collect();
        let (undrawn, moved) = texreq::undrawn_count();
        eprintln!("texprog: to f{field} words by >1 PATH1 program: {} {multi:x?}; program wants undrawn by their program {undrawn}, moved to the drawing program {moved}", s.progs.values().filter(|p| p.len() > 1).count());
    }
    s.field = field;
}

/// Which TEX0 loaded each 16-entry slot of the GS CLUT buffer (CLD rules as `Gs::clut_load`): a draw whose TEX0 does not load the
/// palette samples the slot's palette, which may come from another CBP than its own.
pub struct ClutTrack {
    slot: [u64; 32],
    cbp: [u32; 2],
}

impl Default for ClutTrack {
    fn default() -> Self {
        ClutTrack { slot: [u64::MAX; 32], cbp: [u32::MAX; 2] }
    }
}

fn is_t4(psm: u32) -> bool {
    matches!(psm, 0x14 | 0x24 | 0x2c)
}

impl ClutTrack {
    /// A TEX0 write.
    pub fn tex0(&mut self, t: u64) {
        let psm = ((t >> 20) & 0x3f) as u32;
        if !is_t4(psm) && !matches!(psm, 0x13 | 0x1b) {
            return;
        }
        let (cbp, cpsm, csa, cld) = (((t >> 37) & 0x3fff) as u32, ((t >> 51) & 0xf) as u32, ((t >> 56) & 0x1f) as usize, (t >> 61) & 7);
        let load = match cld {
            1..=3 => true,
            4 => cbp != self.cbp[0],
            5 => cbp != self.cbp[1],
            _ => false,
        };
        match cld {
            2 | 4 => self.cbp[0] = cbp,
            3 | 5 => self.cbp[1] = cbp,
            _ => {}
        }
        if !load {
            return;
        }
        if is_t4(psm) {
            self.slot[if cpsm & 2 != 0 { csa } else { csa & 15 }] = t;
        } else {
            self.slot[..16].iter_mut().for_each(|s| *s = t);
        }
    }

    /// `t` with the CBP / CPSM of the palette the buffer holds for it (unchanged when unknown, or for an 8-bit texture whose slots
    /// came from different loads).
    pub fn effective(&self, t: u64) -> u64 {
        let psm = ((t >> 20) & 0x3f) as u32;
        let s = if is_t4(psm) {
            let csa = ((t >> 56) & 0x1f) as usize;
            self.slot[if (t >> 51) & 2 != 0 { csa } else { csa & 15 }]
        } else if matches!(psm, 0x13 | 0x1b) && self.slot[..16].iter().all(|&x| x == self.slot[0]) {
            self.slot[0]
        } else {
            u64::MAX
        };
        if s == u64::MAX { t } else { t & !PAL_MASK | s & PAL_MASK }
    }
}

static GS_CLUT: Mutex<Option<ClutTrack>> = Mutex::new(None);
static VIF_CLUT: Mutex<Option<ClutTrack>> = Mutex::new(None);

/// The software GS took a TEX0 write (gs.rs `clut_load`, RAC2_TEXCHECK only).
pub fn gs_tex0(t: u64) {
    if let Ok(mut c) = GS_CLUT.lock() {
        c.get_or_insert_with(ClutTrack::default).tex0(t);
    }
}

/// A real GS draw with `t`: its palette-effective TEX0.
pub fn gs_effective(t: u64) -> u64 {
    GS_CLUT.lock().ok().and_then(|c| c.as_ref().map(|c| c.effective(t))).unwrap_or(t)
}

/// Overlay mode skips a world program run: it is run on a copy of VU1 (registers and data memory restored after) and the TEX0s of its
/// textured prims are returned, in draw order, each with its palette-effective TEX0 (`ClutTrack`, kept across runs).
pub fn truth_run(vu: &mut Vu1, addr: usize, top: u32, itop: u32, prog: u8) -> Vec<(u64, u64)> {
    let (data, vf, vi, acc, tpc, st) = (vu.data.clone(), vu.vf, vu.vi, vu.acc, vu.tpc, vu.st.clone());
    let mut out = Out { prog, ..Default::default() };
    let t0 = std::time::Instant::now();
    let _ = vu.run(addr, top, itop, &mut out);
    let ms = t0.elapsed().as_millis();
    if ms > 20 {
        eprintln!("texcheck: slow truth run prog {prog} at {addr:#x}: {ms} ms, {} qwords", out.qs.len());
    }
    vu.data.copy_from_slice(&data);
    (vu.vf, vu.vi, vu.acc, vu.tpc, vu.st) = (vf, vi, acc, tpc, st);
    vu.last_err = None;
    let Ok(mut c) = VIF_CLUT.lock() else { return Vec::new() };
    textured_draws(&out.qs, c.get_or_insert_with(ClutTrack::default))
}

/// A register write seen by `textured_draws`.
fn reg_write(addr: u8, data: u64, prim: &mut u64, tex: &mut [u64; 2], clut: &mut ClutTrack, r: &mut Vec<(u64, u64)>) {
    match addr {
        0 => *prim = data & 0x7ff,
        6 | 7 => {
            tex[(addr - 6) as usize] = data;
            clut.tex0(data);
        }
        // XYZF2 / XYZ2 kick a vertex; XYZF3 / XYZ3 too.
        4 | 5 | 0xc | 0xd => {
            if *prim & 0x10 != 0 {
                let t = tex[((*prim >> 9) & 1) as usize];
                let e = (t, clut.effective(t));
                if r.last() != Some(&e) {
                    r.push(e);
                }
            }
        }
        _ => {}
    }
}

/// The TEX0 (of the prim's context) of each textured prim's vertex kicks in a GIF stream (consecutive repeats once), with its
/// palette-effective TEX0.
pub fn textured_draws(qs: &[u128], clut: &mut ClutTrack) -> Vec<(u64, u64)> {
    let mut r = Vec::new();
    let (mut prim, mut tex) = (0u64, [0u64; 2]);
    let mut i = 0;
    while i < qs.len() {
        let tag = qs[i];
        i += 1;
        let nloop = (tag & 0x7fff) as usize;
        let flg = ((tag >> 58) & 3) as u8;
        let nreg = match ((tag >> 60) & 15) as usize {
            0 => 16,
            n => n,
        };
        if (tag >> 46) & 1 != 0 && flg != 2 {
            prim = ((tag >> 47) & 0x7ff) as u64;
        }
        let reg = |k: usize| ((tag >> (64 + 4 * k)) & 15) as u8;
        match flg {
            0 => {
                for _ in 0..nloop {
                    for k in 0..nreg {
                        let Some(&q) = qs.get(i) else { return r };
                        i += 1;
                        let desc = reg(k);
                        let addr = if desc == 0xe { ((q >> 64) & 0xff) as u8 } else { desc };
                        reg_write(addr, q as u64, &mut prim, &mut tex, clut, &mut r);
                    }
                }
            }
            1 => {
                let n = nloop * nreg;
                for k in 0..n {
                    let Some(&q) = qs.get(i + k / 2) else { return r };
                    let a = reg(k % nreg);
                    if a != 0xe {
                        reg_write(a, (q >> (64 * (k % 2))) as u64, &mut prim, &mut tex, clut, &mut r);
                    }
                }
                i += n.div_ceil(2);
            }
            _ => i += nloop,
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(nloop: u128, eop: bool, pre_prim: Option<u128>, flg: u128, regs: &[u8]) -> u128 {
        let mut t = nloop | (eop as u128) << 15 | flg << 58 | ((regs.len() as u128) & 15) << 60;
        if let Some(p) = pre_prim {
            t |= 1 << 46 | p << 47;
        }
        for (k, &r) in regs.iter().enumerate() {
            t |= (r as u128) << (64 + 4 * k);
        }
        t
    }

    fn ad(addr: u8, data: u64) -> u128 {
        data as u128 | (addr as u128) << 64
    }

    /// A+D TEX0_1 / TEX0_2, a textured triangle in context 1 and one in context 2; an IMAGE and an untextured prim are skipped.
    #[test]
    fn textured_draws_finds_each_prims_tex0() {
        let qs = vec![
            tag(3, false, None, 0, &[0xe]),
            ad(6, 0x1111),
            ad(7, 0x2222),
            ad(0, 0x13),
            tag(1, false, None, 0, &[0x5]),
            0,
            tag(2, false, None, 2, &[]),
            1,
            2,
            tag(1, false, Some(0x13 | 0x200), 0, &[0x2, 0x5]),
            0,
            0,
            tag(1, true, Some(0x3), 0, &[0x5]),
            0,
        ];
        assert_eq!(textured_draws(&qs, &mut ClutTrack::default()), vec![(0x1111, 0x1111), (0x2222, 0x2222)]);
    }

    #[test]
    fn reglist_kicks_count() {
        let qs = vec![tag(1, true, Some(0x13), 1, &[0x6, 0x5]), 0x7777];
        assert_eq!(textured_draws(&qs, &mut ClutTrack::default()), vec![(0x7777, 0x7777)]);
    }

    /// An 8-bit TEX0 with CLD 0 samples the palette the last loading TEX0 put in the buffer.
    #[test]
    fn a_non_loading_tex0_uses_the_buffers_palette() {
        let t8 = |cbp: u64, cld: u64| 0x13u64 << 20 | cbp << 37 | cld << 61;
        let mut c = ClutTrack::default();
        c.tex0(t8(0x100, 1));
        assert_eq!(c.effective(t8(0x200, 0)) & PAL_MASK, t8(0x100, 0) & PAL_MASK);
        c.tex0(t8(0x200, 1));
        assert_eq!(c.effective(t8(0x200, 0)), t8(0x200, 0));
        // A 4-bit load at CSA 3 overwrites one slot: the 8-bit palette is mixed (unknown), the 4-bit one is known.
        let t4 = 0x14u64 << 20 | 0x300u64 << 37 | 3 << 56 | 1 << 61;
        c.tex0(t4);
        assert_eq!(c.effective(t8(0x200, 0)), t8(0x200, 0));
        assert_eq!(c.effective(t4 & !(CLD_MASK | PAL_MASK)) & PAL_MASK, 0x300u64 << 37);
    }
}
