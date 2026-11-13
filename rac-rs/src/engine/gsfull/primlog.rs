//! Debug only (R&C2 round 3 batch 30): the GS blend of the prims drawn with one texture base, read in the software GS (gs.rs `kick`).

/// Is either primlog switch (RAC_GS_PRIMLOG / RAC_GS_PRIMLOG_FRAME) set? Read once; gs.rs `kick` checks it before every `prim_log` call.
#[inline]
pub(super) fn on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("RAC_GS_PRIMLOG").is_some() || std::env::var_os("RAC_GS_PRIMLOG_FRAME").is_some())
}

/// Debug: RAC_GS_PRIMLOG=TBP (hex, TEX0 bits 0..14; a whole TEX0 above 0x3fff, then also its first 40 distinct vertex colour sets) prints each GS prim drawn with that texture base once per (AT, ALPHA, TEST, overlay)
/// set (also the CLAMP register and the texel box the prims' coordinates span): the GS blend retail uses for it (R&C2 batch 30: which RAM classes draw additive).
pub(super) fn prim_log(tex0: u64, at: u64, alpha: u64, test: u64, clamp: u64, mip: [u64; 3], ty: u32, vs: &[super::gs::Vtx]) {
    let rgba = [vs[0].rgba, vs[1].rgba, vs[2].rgba];
    use std::sync::{Mutex, OnceLock};
    static TBP: OnceLock<Option<u64>> = OnceLock::new();
    static SEEN: Mutex<Vec<(u64, u64, u64, u64)>> = Mutex::new(Vec::new());
    frame_log(tex0, at, clamp, mip, vs);
    let Some(tbp) = *TBP.get_or_init(|| std::env::var("RAC_GS_PRIMLOG").ok().and_then(|v| u64::from_str_radix(v.trim().trim_start_matches("0x"), 16).ok())) else {
        return;
    };
    if (if tbp > 0x3fff { tex0 != tbp } else { tex0 & 0x3fff != tbp }) || at & 0x10 == 0 {
        return;
    }
    let k = (at, alpha, test, clamp);
    // Texel coordinates of the prim (FST: UV/16, else S/Q, T/Q times the texture size): the box they span, printed when it grows.
    let (tw, th) = ((1u32 << ((tex0 >> 26) & 0xf).min(10)) as f32, (1u32 << ((tex0 >> 30) & 0xf).min(10)) as f32);
    let uv: Vec<(f32, f32)> = vs.iter().take(3).map(|v| if at & 0x100 != 0 { (v.u as f32 / 16.0, v.v as f32 / 16.0) } else { (v.s / v.q * tw, v.t / v.q * th) }).collect();
    // Per picture readback (`frame_mark`): the texel and screen boxes of this frame's prims, printed when the next frame starts.
    static FB: Mutex<(u64, u32, [f32; 4], [u16; 4])> = Mutex::new((0, 0, [f32::MAX, f32::MAX, f32::MIN, f32::MIN], [u16::MAX, u16::MAX, 0, 0]));
    if let Ok(mut fb) = FB.lock() {
        let now = FRAME.load(std::sync::atomic::Ordering::Relaxed);
        if fb.0 != now {
            if fb.1 > 0 {
                eprintln!("primlog frame {} prims {} uv box {:.1?} xy box {:?}", fb.0, fb.1, fb.2, fb.3.map(|x| x >> 4));
            }
            *fb = (now, 0, [f32::MAX, f32::MAX, f32::MIN, f32::MIN], [u16::MAX, u16::MAX, 0, 0]);
        }
        fb.1 += 1;
        for (&(u, v), p) in uv.iter().zip(vs) {
            if u.is_finite() && v.is_finite() {
                (fb.2[0], fb.2[1], fb.2[2], fb.2[3]) = (fb.2[0].min(u), fb.2[1].min(v), fb.2[2].max(u), fb.2[3].max(v));
            }
            (fb.3[0], fb.3[1], fb.3[2], fb.3[3]) = (fb.3[0].min(p.x), fb.3[1].min(p.y), fb.3[2].max(p.x), fb.3[3].max(p.y));
        }
    }
    static BOX: Mutex<[f32; 4]> = Mutex::new([f32::MAX, f32::MAX, f32::MIN, f32::MIN]);
    if let Ok(mut b) = BOX.lock() {
        let o = *b;
        for &(u, v) in &uv {
            (b[0], b[1], b[2], b[3]) = (b[0].min(u), b[1].min(v), b[2].max(u), b[3].max(v));
        }
        if *b != o && uv.iter().all(|p| p.0.is_finite() && p.1.is_finite()) {
            eprintln!("primlog uv box {:.1?} (prim {uv:.1?})", *b);
        }
    }
    if let Ok(mut s) = SEEN.lock() {
        static V: Mutex<Vec<[[u8; 4]; 3]>> = Mutex::new(Vec::new());
        if let (true, Ok(mut v)) = (tbp > 0x3fff, V.lock()) {
            if v.len() < 40 && !v.contains(&rgba) {
                v.push(rgba);
                eprintln!("primlog vertex rgba {rgba:?}");
            }
        }
        if !s.contains(&k) {
            s.push(k);
            eprintln!("primlog tex0 {tex0:016x} prim {ty} at {at:#x} (ABE {}) alpha {alpha:#x} test {test:#x} clamp {clamp:#x}", (at >> 6) & 1);
        }
    }
}

/// Picture readbacks so far (gs.rs `read_fb`): primlog's frame clock.
static FRAME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A picture was read back (one per presented field in the GS picture modes).
pub(super) fn frame_mark() {
    FRAME.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Debug: RAC_GS_PRIMLOG_FRAME=N prints, at the end of each picture frame from N on (`frame_mark` count), every TEX0 its textured prims used with
/// the prim count, CLAMP, screen box (pixels incl. the XYOFFSET) and texel box: which texture draws an object in that frame.
fn frame_log(tex0: u64, at: u64, clamp: u64, mip: [u64; 3], vs: &[super::gs::Vtx]) {
    use std::sync::{Mutex, OnceLock};
    static N: OnceLock<Option<u64>> = OnceLock::new();
    static ACC: Mutex<(u64, Vec<(u64, u64, u32, [u16; 4], [f32; 4], [u64; 3], [f32; 2])>)> = Mutex::new((0, Vec::new()));
    let Some(n) = *N.get_or_init(|| std::env::var("RAC_GS_PRIMLOG_FRAME").ok().and_then(|v| v.trim().parse().ok())) else { return };
    let now = FRAME.load(std::sync::atomic::Ordering::Relaxed);
    let Ok(mut g) = ACC.lock() else { return };
    if now != g.0 {
        let f = g.0;
        for (t, c, k, xy, uv, m, q) in g.1.drain(..) {
            eprintln!("primlog frame {f} tex0 {t:016x} clamp {c:#x} prims {k} xy {:?} uv {uv:.1?} tex1 {:#x} miptbp1 {:#x} miptbp2 {:#x} q {q:.5?}", xy.map(|x| x >> 4), m[0], m[1], m[2]);
        }
        g.0 = now;
    }
    if now < n || at & 0x10 == 0 {
        return;
    }
    let acc = &mut g.1;
    let (tw, th) = ((1u32 << ((tex0 >> 26) & 0xf).min(10)) as f32, (1u32 << ((tex0 >> 30) & 0xf).min(10)) as f32);
    let i = match acc.iter().position(|e| e.0 == tex0 && e.1 == clamp && e.5 == mip) {
        Some(i) => i,
        None => {
            acc.push((tex0, clamp, 0, [u16::MAX, u16::MAX, 0, 0], [f32::MAX, f32::MAX, f32::MIN, f32::MIN], mip, [f32::MAX, f32::MIN]));
            acc.len() - 1
        }
    };
    let e = &mut acc[i];
    e.2 += 1;
    for v in vs.iter().take(3) {
        let (u, w) = if at & 0x100 != 0 { (v.u as f32 / 16.0, v.v as f32 / 16.0) } else { (v.s / v.q * tw, v.t / v.q * th) };
        (e.3[0], e.3[1], e.3[2], e.3[3]) = (e.3[0].min(v.x), e.3[1].min(v.y), e.3[2].max(v.x), e.3[3].max(v.y));
        (e.6[0], e.6[1]) = (e.6[0].min(v.q.abs()), e.6[1].max(v.q.abs()));
        if u.is_finite() && w.is_finite() {
            (e.4[0], e.4[1], e.4[2], e.4[3]) = (e.4[0].min(u), e.4[1].min(w), e.4[2].max(u), e.4[3].max(w));
        }
    }
}
