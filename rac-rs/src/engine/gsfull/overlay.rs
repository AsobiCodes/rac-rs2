//! R&C2 HUD overlay mode (rac-rs/src/rc2 window and `--shots --native`): rac-rs draws the 3D world natively (rc2/native),
//! so the VU1 world programs do not run; the GS draws the HUD and the effect programs `progs` still runs into a cleared,
//! see-through buffer (gs_overlay.rs) that is laid over the native picture (`crtc::present_overlay`). Along the way the
//! VIF1 stream hands the native picture the water surface input (water.rs) and the textures it asked for (texreq.rs).
//! Pipe / Vif1 / Gs keep R&C1's path unchanged while `Vif1::ovl` is None and `Gs::set_overlay` was never called.
use super::crtc::Crtc;
use super::gs::Gs;
use super::pipe::{Field, Pipe};
use super::vif::Out;
use super::vu1::Vu1;
use super::water::{GifRegs, WaterBatch};
use ee_oracle::gs::Job;

/// Most qwords one effect-program run may emit in overlay mode (normal particle runs: a few thousand).
const FX_MAX_QW: usize = 100_000;

/// VU1 programs (bit n = program id n) that still run in HUD overlay mode: the effects the native picture does not draw yet.
/// Program 2 = sprites / particles [C: Aranos runs {2, 5, 6, 7, 11}; 5, 6 terrain, 7 ties, 11 mobies], program 10 = the
/// water surface [C: level 1 field 2700 RAC2_ONLYPROG=10 draws the swamp plane] (not on by default: in overlay mode its
/// draws come out as garbage texture blocks and fail the stale GS Z test). RAC2_FX_PROGS="2,10" overrides.
pub fn progs() -> u32 {
    static M: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *M.get_or_init(|| match std::env::var("RAC2_FX_PROGS") {
        Ok(v) => v.split(',').filter_map(|x| x.trim().parse::<u32>().ok()).filter(|&n| n < 16).fold(0, |m, n| m | 1 << n),
        Err(_) => 1 << 2,
    })
}

/// The VU1 program that draws a menu's own 3D (the vendor's weapon model: the moby program, id 11 [C: runs6_s1.err]); the other world
/// programs stay skipped in a frozen-screen menu field.
const MENU_PROG: u8 = 11;

/// The effect program drawn natively from its input records (fx_native.rs).
const FX_PROG: u8 = 2;

/// The overlay state of the VIF1 front end (Vif1::ovl).
#[derive(Default)]
pub struct VifOverlay {
    /// GS registers as the GIF packets so far left them, and the qwords of `Out::qs` already fed to it.
    gif: GifRegs,
    parsed: usize,
    /// The water batches (VU1 program 10 inputs) of this field.
    water: Vec<WaterBatch>,
    /// VU1 data memory at the last skipped run scanned for wanted TEX0s (only qwords written since count).
    texseen: Vec<[u32; 4]>,
    /// (qword, TEX0, VU1 program) where a skipped VU1 run would have drawn with a TEX0 the native picture wants (texreq::mark at
    /// that point of the stream), for the current job.
    /// The 4th field: the MIPTBP1 / MIPTBP2 words of the TEX0's AD block in VU1 memory (texreq_mips::set_bases), when it has them.
    texmarks: Vec<(u32, u64, u8, Option<[u64; 2]>)>,
    /// (qword, TEX0) of the textured prims a skipped run would have drawn (texcheck detector, RAC2_TEXCHECK only).
    texchk: Vec<(u32, u64, u64)>,
    /// The grabbed-backdrop area of this field (grab.rs; None = no frozen-screen cure): once the field has uploaded into it, its 3D draws
    /// belong to the menu and run (`menu_3d`).
    grab: Option<super::grab::Grab>,
    /// This field's effect program records (fx_native.rs) when the native draw of the effects is on.
    fxw: Vec<super::fx_native::FxRec>,
}

impl VifOverlay {
    /// The field is past its first IMAGE upload into the grabbed area, i.e. it is a frozen-screen menu from here on (grab.rs): the menu's own 3D
    /// (the vendor's weapon model, drawn by the moby program, `MENU_PROG`, after the panels) is not the world the native picture replaces, so it
    /// must run. Asked for that program only.
    fn menu_3d(&mut self, out: &Out) -> bool {
        let Some(grab) = self.grab else { return false };
        for &q in &out.qs[self.parsed.min(out.qs.len())..] {
            self.gif.feed(q);
        }
        self.parsed = out.qs.len();
        self.gif.img.iter().any(|i| grab.hit(i.0, i.1))
    }

    /// One MSCAL / MSCNT in overlay mode (Vif1::exec): the water input is captured, the world programs are skipped (an
    /// empty PATH1 segment still marks where the 3D draw was, so the PATH2 / PATH3 draws after the last one read as the
    /// HUD), the effect programs run with a cap on their output.
    pub(super) fn exec(&mut self, addr: usize, top: u32, itop: u32, vu: &mut Vu1, out: &mut Out) {
        if out.prog == super::water::WATER_PROG {
            for &q in &out.qs[self.parsed.min(out.qs.len())..] {
                self.gif.feed(q);
            }
            self.parsed = out.qs.len();
            if let Some(b) = super::water::capture(&vu.data, top as usize, &self.gif) {
                // Its texture is uploaded just before and overwritten later in the field: a wanted one is read here.
                if super::texreq::wanted(b.tex0) && !self.texmarks.iter().any(|m| m.1 == b.tex0) {
                    self.texmarks.push((out.qs.len() as u32, b.tex0, out.prog, None));
                }
                self.water.push(b);
            }
        }
        if (progs() >> (out.prog & 15)) & 1 == 0 && !(out.prog == MENU_PROG && self.menu_3d(out)) {
            out.seg(1);
            // The moby program draws the RAM classes' textures (the only ones texreq serves for mobies).
            if super::texcheck::on() && out.prog == MENU_PROG {
                let q = out.qs.len() as u32;
                self.texchk.extend(super::texcheck::truth_run(vu, addr, top, itop, out.prog).into_iter().map(|(t, e)| (q, t, e)));
            }
            self.scan_tex(vu, out);
            return;
        }
        if out.prog == FX_PROG && addr != 0 && super::layer_tris::fx_on() {
            // The records the run is about to consume (its input buffer toggles between qword 0 and 0x80 words: (vi15 + 0x200) & vi14,
            // as the program's own first instructions do; vi14 = 0x3ff), before it overwrites its input with its output.
            let mask = if vu.vi[14] & 0x3ff == 0 { 0x3ff } else { vu.vi[14] & 0x3ff };
            let base = ((vu.vi[15].wrapping_add(0x200)) & mask) as usize;
            super::fx_native::capture(vu, base, &mut self.fxw);
        }
        // (a run that errors leaves the packets it already sent; the field goes on)
        let (nq, ns) = (out.qs.len(), out.segs.len());
        if vu.run(addr, top, itop, out).is_err() {
            out.stats.run_errs += 1;
        }
        // An effect program that emits a runaway count (4 M qwords in one run on a few level 1 fields, a bogus particle
        // count [?]) would take seconds to draw: its output of that run is dropped.
        if out.qs.len() - nq > FX_MAX_QW {
            out.qs.truncate(nq);
            out.segs.truncate(ns);
            out.seg(1);
        }
    }

    /// A skipped run's TEX0s (A+D TEX0_1 / TEX0_2 qwords it uploaded) that the native picture wants: textures the game
    /// uploads just before the draw and overwrites later in the field (Ratchet's head at TBP 0) are read at this point of
    /// the stream instead of after the field. The first skipped run after textures became wanted only takes the snapshot
    /// (older memory would match TEX0s of earlier draws, whose texels are gone).
    /// A texreq mark of TEX0 `t` (with level bases `bases`) at the current qword of `out`, when the word is wanted or held for this
    /// program and not marked yet in this run. Cheap tests first (the word's wanted/held bit, this run's marks), the locks last.
    fn mark_tex(marks: &mut Vec<(u32, u64, u8, Option<[u64; 2]>)>, t: u64, bases: Option<[u64; 2]>, out: &Out) {
        if super::texreq::maybe(t) && !marks.iter().any(|m| m.1 == t && m.2 == out.prog) && (super::texreq::wanted(t) || super::texreq::held_for(t, out.prog)) {
            marks.push((out.qs.len() as u32, t, out.prog, bases));
        }
    }

    fn scan_tex(&mut self, vu: &Vu1, out: &Out) {
        if !super::texreq::pending() && !super::texreq::any_held() {
            self.texseen.clear();
        } else if self.texseen.len() != vu.data.len() {
            self.texseen = vu.data.clone();
        } else {
            for (i, (q, old)) in vu.data.iter().zip(&self.texseen).enumerate() {
                if q == old {
                    continue;
                }
                // A TEX0 qword, or a MIPTBP qword right after one (the game moves a class's levels 1..MXL in the VRAM ring without
                // rewriting its TEX0 [C: crate TEX0 ..b29b at Oozla field 7900, MIPTBP1 0x32ac in the draw's AD block]).
                let at = match q[2] & 0xff {
                    6 | 7 => i,
                    0x34..=0x37 => match (1..=2).map(|k| i.wrapping_sub(k)).find(|&j| vu.data.get(j).is_some_and(|d| matches!(d[2] & 0xff, 6 | 7))) {
                        Some(j) => j,
                        None => continue,
                    },
                    _ => continue,
                };
                let d = vu.data[at];
                let raw = d[0] as u64 | (d[1] as u64) << 32;
                let bases = ad_bases(&vu.data, at);
                // [b47 X5] (b46 review minors 4, 6) The draw's own word is marked when it is wanted or held: a level-file class that
                // asks for a mip-only word as it is keeps its reads (b46 renamed every draw of a registered word).
                Self::mark_tex(&mut self.texmarks, raw, bases, out);
                // [b46 U5] A mip-only word (TBP0 0, level bases set) is ALSO known by its first resident level, from this draw's own
                // bases (texreq_resident): classes sharing the word are different textures, and its level 0 is not in VRAM. The mip
                // registers' lock only for a word that may be registered (`maybe_regs`, an atomic mask).
                if raw & 0x3fff == 0 && bases.is_some() && super::texreq_mips::maybe_regs(raw) {
                    let tex1 = super::texreq_mips::regs(raw).map(|r| r[0]);
                    let (w, b) = super::texreq_resident::draw_word(raw, tex1, bases);
                    if w != raw {
                        Self::mark_tex(&mut self.texmarks, w, b, out);
                    }
                }
            }
            self.texseen.copy_from_slice(&vu.data);
        }
    }
}

/// The MIPTBP1 / MIPTBP2 data of the AD block whose TEX0 qword is `data[at]` (the qwords after it, register 0x34/0x35 and 0x36/0x37);
/// None without a MIPTBP1. A block without MIPTBP2 gives 0 for it (as the slot's RAM block: `rc2::mobies::block_mip`).
fn ad_bases(data: &[[u32; 4]], at: usize) -> Option<[u64; 2]> {
    let mut b = [None, None];
    for d in data.iter().skip(at + 1).take(2) {
        let w = d[0] as u64 | (d[1] as u64) << 32;
        match d[2] & 0xff {
            0x34 | 0x35 => b[0] = Some(w),
            0x36 | 0x37 => b[1] = Some(w),
            _ => {}
        }
    }
    Some([b[0]?, b[1].unwrap_or(0)])
}

/// One overlay field: the GS qwords and the texture marks (field qword, TEX0) for `raster`.
#[derive(Default)]
pub struct OvlField {
    pub f: Field,
    pub texmarks: Vec<(u32, u64, u8, Option<[u64; 2]>)>,
    /// The texcheck detector's draw points (RAC2_TEXCHECK; empty otherwise).
    pub texchk: Vec<(u32, u64, u64)>,
    /// (BITBLTBUF DBP, PSM, qword index) of every IMAGE upload of the field (overlay fields only; empty otherwise).
    pub images: Vec<(u32, u32, u32)>,
    /// The first qword that counts as HUD (grab.rs `Grab::hud_from`, set by ovl_pipe.rs): None = the usual rule (`is_hud`).
    pub hud_from: Option<u32>,
    /// The effect program's records of the field, in the order the program emitted them (fx_native.rs; empty unless enabled).
    pub fxw: Vec<super::fx_native::FxRec>,
    /// [b46 U4] R&C2's ship flight between planets (game mode 6): the cosmos (a nebula of Gouraud triangles and two palettised texture
    /// meshes, 256 x 256 and 512 x 256) is drawn by the EE itself over PATH2 BEFORE the (empty here) PATH1 ship draw, so the usual rule drops
    /// it. From this qword on (the first 8-bit texture upload of the field) the field's draws count as overlay pixels. None = the usual rule.
    pub space_from: Option<u32>,
}

impl OvlField {
    /// Does the HUD rule change at qword `k` (`hud_from`, `space_from`)?
    pub fn flips_at(&self, k: usize) -> bool {
        self.hud_from == Some(k as u32) || self.space_from == Some(k as u32)
    }

    /// [b46 fix round] Is qword `k` one of a flight field's cosmos draws: from `space_from` up to the first PATH1 segment after it
    /// (the ship, drawn natively)? What follows that segment is the flight's HUD (in front, as always). The cosmos is the lowest layer of
    /// the picture: the software GS draws it at the farthest depth (`Gs::set_space`), the GPU capture leaves it alone (gscap/rc2.rs).
    pub fn space_at(&self, k: usize) -> bool {
        let Some(s) = self.space_from else { return false };
        let end = self.f.segs.iter().find(|g| g.1 == 1 && g.0 >= s).map_or(usize::MAX, |g| g.0 as usize);
        (s as usize..end).contains(&k)
    }
}

impl Pipe {
    /// One field's jobs (EE field `field`) in overlay mode (`overlay` true) or the normal one; the water batches go to water::WATER.
    pub fn process_rc2(&mut self, jobs: Vec<Job>, crtc: Crtc, overlay: bool, grab: Option<super::grab::Grab>, field: u64) -> OvlField {
        if !overlay {
            self.vif.ovl = None;
            return OvlField { f: self.process(jobs, crtc), texmarks: Vec::new(), ..Default::default() };
        }
        let mut o = self.vif.ovl.take().unwrap_or_default();
        o.gif.img.clear();
        o.fxw.clear();
        o.grab = grab;
        let mut f = Field { crtc, ..Default::default() };
        let mut texmarks = Vec::new();
        let mut texchk = Vec::new();
        f.job_dispfb = jobs
            .iter()
            .rev()
            .find_map(|j| match j {
                Job::Vif1(_, _, d) | Job::Gif(_, d) => Some(*d),
            })
            .unwrap_or(0);
        for j in jobs {
            match j {
                Job::Vif1(w, marks, _) => {
                    self.out.qs.clear();
                    self.out.segs.clear();
                    o.texmarks.clear();
                    o.texchk.clear();
                    o.parsed = 0;
                    o.gif.n = f.qs.len() as u32;
                    self.vif.ovl = Some(o);
                    self.vif.feed(&w, &marks, &mut self.vu, &mut self.out);
                    ee_oracle::gs::recycle_words(w);
                    o = self.vif.ovl.take().unwrap_or_default();
                    for &q in &self.out.qs[o.parsed.min(self.out.qs.len())..] {
                        o.gif.feed(q);
                    }
                    let base = f.qs.len() as u32;
                    f.qs.extend_from_slice(&self.out.qs);
                    f.segs.extend(self.out.segs.iter().map(|&(s, p, g)| (base + s, p, g)));
                    texmarks.extend(o.texmarks.iter().map(|&(q, t, p, b)| (base + q, t, p, b)));
                    texchk.extend(o.texchk.iter().map(|&(q, t, e)| (base + q, t, e)));
                }
                Job::Gif(qs, _) => {
                    o.gif.n = f.qs.len() as u32;
                    qs.iter().for_each(|&q| o.gif.feed(q));
                    f.segs.push((f.qs.len() as u32, 3, 0));
                    f.qs.extend(qs);
                }
            }
        }
        f.vu_errors = self.vu.st.errors;
        super::water::publish(field, std::mem::take(&mut o.water));
        // RAC_GSFULL_QDUMP=<file>: the field's GS qwords (debug, as `Pipe::process`; overwritten each overlay field).
        if let Some(p) = std::env::var_os("RAC_GSFULL_QDUMP") {
            let b: Vec<u8> = f.qs.iter().flat_map(|x| x.to_le_bytes()).collect();
            let _ = std::fs::write(p, b);
        }
        let images = std::mem::take(&mut o.gif.img);
        let fxw = std::mem::take(&mut o.fxw);
        self.vif.ovl = Some(o);
        OvlField { f, texmarks, texchk, images, hud_from: None, fxw, space_from: None }
    }
}

/// A segment (start qword, path, VU1 program) of an effect program the overlay still draws (`progs`).
pub fn is_fx(s: &(u32, u8, u8)) -> bool {
    s.1 == 1 && (progs() >> (s.2 & 15)) & 1 != 0
}

/// Index of the last PATH1 segment that is not an effect program (the 3D draw the native picture replaces), -1 when none.
pub fn last_world_seg(segs: &[(u32, u8, u8)]) -> i64 {
    segs.iter().rposition(|s| s.1 == 1 && !is_fx(s)).map(|x| x as i64).unwrap_or(-1)
}

/// Segment `i` (after `last_world_seg` `last1`) draws the HUD (everything after the 3D draw) or an effect: its overlay pixels count.
pub fn is_hud(i: usize, seg: &(u32, u8, u8), last1: i64) -> bool {
    (i as i64) > last1 || is_fx(seg)
}

/// Whether qword `k` of segment `i` (after `last_world_seg` `last1`) draws the HUD: `is_hud`, except in a field with a grabbed backdrop
/// (`OvlField::hud_from`, a frozen-screen menu: the pause menu, the title, the vendor): what precedes its first upload into the area is the
/// frozen picture's copy and clear (grab.rs), and EVERYTHING from there on is the menu, whatever `last1` says: the vendor draws its weapon
/// model (four PATH1 draws of the moby program) AFTER all its panels, which made `last1` pass the panels and dropped the whole menu [C: level 1
/// vendor, field 2560: segment 0 = the 51 k qwords of the menu, then 4 empty PATH1 prog 11 segments].
pub fn hud_at(of: &OvlField, i: usize, seg: &(u32, u8, u8), last1: i64, k: usize) -> bool {
    // [b46 U4] A flight field (`OvlField::space_from`): the cosmos' own draws are the picture's lowest layer, drawn by the game itself.
    if of.space_from.is_some_and(|s| k >= s as usize) {
        return true;
    }
    match of.hud_from {
        Some(h) => is_fx(seg) || k >= h as usize,
        None => is_hud(i, seg, last1),
    }
}

/// Feeds a field to the GS. An overlay field draws the HUD into a cleared, see-through buffer (`Gs::set_overlay`); the 3D
/// is not run at all. The effect programs (`progs`) also run: their PATH1 segments are drawn into the overlay like the
/// HUD, and do not count as the 3D draw that precedes it. A normal field is `pipe::raster` (overlay mode off). `fxw`: the field's effect
/// records (`OvlField::fxw`, taken out of it by the caller so they move into the GS without a copy).
pub fn raster(g: &mut Gs, of: &OvlField, overlay: bool, fxw: Vec<super::fx_native::FxRec>) {
    let f = &of.f;
    let shown = f.crtc.shown().map_or((512, 448), |s| (s.4, s.5));
    g.set_overlay(overlay, shown);
    g.set_fx_world(if overlay { fxw } else { Vec::new() });
    if !overlay {
        g.set_fx(false);
        g.set_space(false);
        super::pipe::raster(g, f);
        return;
    }
    let last1 = last_world_seg(&f.segs);
    segs_log(of, last1);
    let (mut mk, mut ck) = (0, 0);
    for (i, seg) in f.segs.iter().enumerate() {
        g.set_fx(is_fx(seg));
        g.set_seg(seg.1, seg.2, hud_at(of, i, seg, last1, seg.0 as usize));
        g.set_space(of.space_at(seg.0 as usize));
        let end = f.segs.get(i + 1).map(|s| s.0 as usize).unwrap_or(f.qs.len());
        for k in seg.0 as usize..end {
            if of.flips_at(k) && k > seg.0 as usize { // [b46 U4]
                g.set_seg(seg.1, seg.2, hud_at(of, i, seg, last1, k));
                g.set_space(of.space_at(k));
            }
            while let Some(&(_, t, p, b)) = of.texmarks.get(mk).filter(|m| m.0 as usize <= k) {
                mark(g, t, p, b);
                mk += 1;
            }
            while let Some(&(_, t, e)) = of.texchk.get(ck).filter(|m| m.0 as usize <= k) {
                super::texcheck::check(g, t, e, 1);
                ck += 1;
            }
            g.gif(f.qs[k]);
        }
    }
    for &(_, t, p, b) in &of.texmarks[mk.min(of.texmarks.len())..] {
        mark(g, t, p, b);
    }
}

/// A texture mark with its draw's level bases: the levels are read from where this draw's GS samples them (a moved chain is read
/// again at this mark), then `texreq::mark`.
fn mark(g: &mut Gs, t: u64, p: u8, bases: Option<[u64; 2]>) {
    if bases.is_some_and(|b| super::texreq_mips::set_bases(t, b)) {
        super::texreq::reread(t, p);
    }
    super::texreq::mark(g, t, p);
}

/// Debug (RAC2_OVLSEGS=N): the segments of every overlay field from the N-th on (the vendor menu).
fn segs_log(of: &OvlField, last1: i64) {
    static FROM: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let Some(from) = *FROM.get_or_init(|| std::env::var("RAC2_OVLSEGS").ok().and_then(|v| v.trim().parse().ok())) else {
        return;
    };
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if n < from {
        return;
    }
    let f = &of.f;
    eprintln!("ovlsegs call {n} qw {} last1 {last1} hud_from {:?} images {:?}", f.qs.len(), of.hud_from, of.images.iter().map(|i| (format!("{:#x}", i.0), i.1, i.2)).collect::<Vec<_>>());
    eprintln!("ovlsegs segs {:?}", f.segs);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The vendor menu (level 1, field 2560): its panels are segment 0 (PATH2), the weapon model is four PATH1 prog 11 draws after them, so
    /// `last1` is behind every panel; from the first upload into the grabbed area on, all of it is HUD, and what precedes it is not.
    #[test]
    fn a_frozen_menu_is_all_hud_after_its_first_upload_whatever_the_last_3d_draw() {
        let segs = [(0, 2, 11), (51328, 1, 11), (51328, 2, 11), (51330, 1, 11)];
        let last1 = last_world_seg(&segs);
        assert_eq!(last1, 3);
        assert!(!is_hud(0, &segs[0], last1), "the usual rule drops the menu");
        let mut of = OvlField::default();
        of.hud_from = Some(85);
        assert!(!hud_at(&of, 0, &segs[0], last1, 10), "the frozen picture's copy and clear");
        assert!(hud_at(&of, 0, &segs[0], last1, 85));
        assert!(hud_at(&of, 0, &segs[0], last1, 51000));
        assert!(hud_at(&of, 3, &segs[3], last1, 51330), "the weapon model");
        of.hud_from = None;
        assert!(hud_at(&of, 3, &segs[3], 2, 51330) && !hud_at(&of, 0, &segs[0], 2, 51000), "no grab: the usual rule");
    }

    /// [b46 U4] The flight (bank 0013): segment 0 is the cosmos over PATH2, the PATH1 run after it is empty; the usual rule drops the cosmos,
    /// `space_from` (first PSM 19 upload, qword 185) keeps everything from there on and flips the rule at that qword.
    #[test]
    fn the_flight_cosmos_counts_from_its_first_texture_upload() {
        let segs = [(0, 2, 11), (13800, 1, 11), (13800, 2, 11)];
        let last1 = last_world_seg(&segs);
        let mut of = OvlField::default();
        assert!(!hud_at(&of, 0, &segs[0], last1, 12600), "the usual rule drops the cosmos");
        of.space_from = Some(185);
        assert!(!hud_at(&of, 0, &segs[0], last1, 100) && of.flips_at(185) && !of.flips_at(186));
        assert!(hud_at(&of, 0, &segs[0], last1, 185) && hud_at(&of, 0, &segs[0], last1, 12600));
        // [b46 fix round] The cosmos (far layer) ends at the ship's PATH1 segment; the HUD after it is in front as always.
        assert!(!of.space_at(184) && of.space_at(185), "no PATH1 segment after it: the cosmos runs to the field's end");
        of.f.segs = segs.to_vec();
        assert!(!of.space_at(184) && of.space_at(185) && of.space_at(13799) && !of.space_at(13800));
        of.space_from = None;
        assert!(!of.space_at(185));
    }

    /// The menu's 3D runs only in a field that has uploaded into the grabbed area.
    #[test]
    fn the_menu_3d_needs_an_upload_into_the_grabbed_area() {
        let g = super::super::grab::Grab { base: 0x1e00, blocks: 0xe00, psm: 1 };
        let mut o = VifOverlay::default();
        let out = Out::default();
        assert!(!o.menu_3d(&out), "no grab: never");
        o.grab = Some(g);
        assert!(!o.menu_3d(&out), "a grab but no upload yet");
        o.gif.img.push((0x1e40, 1, 5));
        assert!(o.menu_3d(&out));
    }
}
