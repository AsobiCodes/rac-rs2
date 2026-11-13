//! Capture of the world layers for the native draw (`layer_tris.rs`). Child of `gs.rs` (Gs's private fields).
use super::super::fx_native::{self, FxRec, WVert};
use super::super::layer_tris::{self, LayerTri, LayerVert, MAX_TEX, MAX_TRIS};
use super::super::texreq;
use super::{Gs, Vtx};
use crate::level::Texture;
use std::sync::Arc;

/// One texture read for the layers: its TEX0 (without CLD), the VRAM blocks it and its CLUT occupy, the upload counter at the read
/// (any later upload into a span makes the entry stale), and the texels with their hash.
struct Entry {
    key: u64,
    spans: [(u32, u32); 2],
    read_gen: u64,
    /// `up_gen` at the last freshness check (nothing was uploaded since, so the entry is still fresh), and the field it was last used in.
    checked: u64,
    last: u64,
    tex: Arc<Texture>,
    id: u64,
}

/// A texture asked for by triangles of this field and not read yet (`resolve`): its TEX0, its blocks, the upload counter when it was
/// asked for, and the triangles (indices into `Layers::tris`) waiting for its texels.
struct Pending {
    key: u64,
    /// TEXA / TEXCLUT as they were when the texture was asked for (the pack's name, `layer_repl.rs`).
    texa: u64,
    texclut: u64,
    spans: [(u32, u32); 2],
    req_gen: u64,
    tris: Vec<usize>,
}

/// What the layers hold: this field's triangles and the textures read for them. The texture cache lives across fields: an entry stays
/// valid until something is uploaded into the VRAM blocks of its texture or palette (an animated texture moves its TBP every field).
#[derive(Default)]
pub(in crate::gsfull) struct Layers {
    tris: Vec<LayerTri>,
    cache: Vec<Entry>,
    /// Textures whose texels are read at the end of the field (or when something is uploaded over them), when the raster has finished:
    /// the triangles that use them hold a blank texture until then.
    pending: Vec<Pending>,
    blank: Option<Arc<Texture>>,
    /// Per 256-byte VRAM block: the upload counter value of the last upload into it.
    blk_gen: Vec<u64>,
    up_gen: u64,
    pub(super) field_no: u64,
    /// The field is a frozen-screen one (pause / title menu): its prims stay in the overlay.
    frozen: bool,
    stats: Stats,
    /// This field's effect program records (`fx_native.rs`), the search cursor into them, and what became of the program's prims.
    fxw: Vec<FxRec>,
    fx_cur: usize,
    /// Prims each record of `fxw` has yielded (`fx_native::find`).
    fx_used: Vec<u8>,
    fx: FxStats,
    /// `cache` index of a TEX0 key, as last found (checked against the entry before it is trusted: the cache is reordered).
    idx: std::collections::HashMap<u64, usize>,
}

/// RAC2_FXLOG=1: the effect program's prims of a field: drawn natively, left in the overlay (no record / no blend we reproduce).
#[derive(Default)]
struct FxStats {
    hit: u32,
    no_rec: u32,
    no_mode: u32,
    /// Prims of a GS type the native draw does not handle (a sprite, a point): they stay in the overlay.
    no_type: u32,
    /// Prims drawn without the depth test (ZTE off or ZTST ALWAYS), with the depth test.
    no_depth: u32,
    tested: u32,
    /// World-layer prims (not effect-program ones) that stayed in the overlay: a blend / cut-out the native draw does not reproduce.
    layer_kept: u32,
}

/// RAC2_LAYERTIME=1: what one field's capture cost.
#[derive(Default)]
struct Stats {
    ns: u64,
    /// The share of `ns` spent in `fx_capture` (record search + capture), and the prims it handled.
    fx_ns: u64,
    reads: u32,
    hits: u32,
    fog_min: u8,
    fog_n: u32,
}

fn env_on(cell: &'static std::sync::OnceLock<bool>, name: &str) -> bool {
    *cell.get_or_init(|| std::env::var_os(name).is_some_and(|v| v != "0"))
}

/// RAC2_LAYERTIME=1 logs the capture's cost per field; RAC2_LAYER_LEGACY=1 reads every texture every field behind a raster sync (the
/// first version), to compare the two. Both read once.
fn timing() -> bool {
    static C: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    env_on(&C, "RAC2_LAYERTIME")
}

pub(super) fn fx_log() -> bool {
    static C: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    env_on(&C, "RAC2_FXLOG")
}

fn legacy() -> bool {
    static C: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    env_on(&C, "RAC2_LAYER_LEGACY")
}

const NBLK: u32 = 16384;

/// Where a layer's texels are: already read, or in `Layers::pending` (index).
enum Src {
    Ready(Arc<Texture>, u64),
    Pending(usize),
}

impl Layers {
    fn note_upload(&mut self, lo: u32, hi: u32) {
        if self.blk_gen.is_empty() {
            self.blk_gen = vec![0; NBLK as usize];
        }
        self.up_gen += 1;
        for b in lo..hi.min(NBLK) {
            self.blk_gen[b as usize] = self.up_gen;
        }
    }

    /// The upload counter of the last upload (or copy) into blocks [lo, hi); 0 when none was counted.
    fn last_upload(&self, lo: u32, hi: u32) -> u64 {
        if self.blk_gen.is_empty() {
            return 0;
        }
        self.blk_gen[lo.min(NBLK) as usize..hi.min(NBLK) as usize].iter().copied().max().unwrap_or(0)
    }

    /// Nothing was uploaded into the texture's or the palette's blocks since the entry was read.
    fn fresh(&mut self, i: usize) -> bool {
        let e = &self.cache[i];
        if e.checked == self.up_gen || self.blk_gen.is_empty() {
            return true;
        }
        let ok = e.spans.iter().all(|&(lo, hi)| self.blk_gen[lo as usize..hi.min(NBLK) as usize].iter().all(|&g| g <= e.read_gen));
        if ok {
            self.cache[i].checked = self.up_gen;
        }
        ok
    }
}

/// The GS blends the native layer draw reproduces (ALPHA `(A - B) * C >> 7 + D`, C = As): every other one stays in the overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Blend {
    /// (Cs, Cd, As, Cd) 0x44: source over.
    Over,
    /// (Cs, 0, As, Cd) 0x48: additive.
    Add,
    /// (Cs, 0, As, 0) 0x88: Cs * As, the destination is not read (the level-14 lava sheet).
    Replace,
}

/// How a prim blends and shades, when the native draw can reproduce it: (blend, decal). None: the overlay keeps drawing it.
fn mode(at: u64, test: u64, alpha: u64, tex0: u64) -> Option<(Blend, bool)> {
    let (ate, atst, afail) = (test & 1 != 0, (test >> 1) & 7, (test >> 12) & 3);
    let (abe, tme) = (at & 0x40 != 0, at & 0x10 != 0);
    if !tme || !abe {
        return None;
    }
    // ALPHA (A - B) * C >> 7 + D with C = As: A = Cs; B = Cd (source over, D = Cd) or 0 (additive, D = Cd; replace, D = 0).
    let (a, b, c, d) = (alpha & 3, (alpha >> 2) & 3, (alpha >> 4) & 3, (alpha >> 6) & 3);
    let blend = match (a, b, c, d) {
        (0, 1, 0, 1) => Blend::Over,
        (0, 2, 0, 1) => Blend::Add,
        (0, 2, 0, 2) => Blend::Replace,
        _ => return None,
    };
    // Over / add need no alpha-test cut-out: ATE off, ATST ALWAYS, or AFAIL RGB_ONLY (the glows: the failing pixels still write their
    // colour, and these layers write no depth, so nothing is cut). A replace layer takes any alpha test (`AlphaTest`, its own draws).
    if blend != Blend::Replace && ate && atst != 1 && afail != 3 {
        return None;
    }
    let (psm, tfx) = ((tex0 >> 20) & 0x3f, (tex0 >> 35) & 3);
    if !matches!(psm, 0x00 | 0x13 | 0x14 | 0x1b | 0x24 | 0x2c) || tfx > 1 {
        return None;
    }
    Some((blend, tfx == 1))
}

/// `mode` for a prim of the effect program: also an untextured one (rain / spark lines), blended. Effect prims are placed by their
/// records and drawn in the particle pass: over or additive only (a replace one stays in the overlay).
fn fx_mode(at: u64, test: u64, alpha: u64, tex0: u64) -> Option<(Blend, bool)> {
    if at & 0x10 != 0 {
        return mode(at, test, alpha, tex0).filter(|m| m.0 != Blend::Replace);
    }
    let (ate, atst, afail) = (test & 1 != 0, (test >> 1) & 7, (test >> 12) & 3);
    if at & 0x40 == 0 || (ate && atst != 1 && afail != 3) {
        return None;
    }
    let (a, b, c, d) = (alpha & 3, (alpha >> 2) & 3, (alpha >> 4) & 3, (alpha >> 6) & 3);
    match (a, b, c, d) {
        (0, 1, 0, 1) => Some((Blend::Over, false)),
        (0, 2, 0, 1) => Some((Blend::Add, false)),
        _ => None,
    }
}

/// Does a prim with this TEST register depth-test? ZTE off or ZTST ALWAYS (1): no. (NEVER draws nothing on the GS; here it is tested.)
fn depth_tested(test: u64) -> bool {
    (test >> 16) & 1 != 0 && (test >> 17) & 3 != 1
}

impl Gs {
    /// The effect program's records of the field about to be fed (`fx_native::capture`, collected by the VIF1 front end).
    pub fn set_fx_world(&mut self, recs: Vec<FxRec>) {
        let l = &mut self.ovl.layers;
        l.fx_used.clear();
        l.fx_used.resize(recs.len(), 0);
        l.fxw = recs;
        l.fx_cur = 0;
    }

    /// A prim of the effect program (overlay field): matched to its record and captured with world corners (true = not drawn here), or
    /// false: the overlay draws it as before.
    pub(super) fn fx_capture(&mut self, ty: u32, vs: &[Vtx; 3], at: u64) -> bool {
        let n = match ty {
            1 | 2 => 2,
            3..=5 => 3,
            _ => {
                self.ovl.layers.fx.no_type += 1;
                return false;
            }
        };
        let l = &self.ovl.layers;
        if !layer_tris::fx_on() || l.frozen || l.fxw.is_empty() || l.tris.len() + 2 > MAX_TRIS {
            return false;
        }
        let t0 = timing().then(std::time::Instant::now);
        let pts = [[vs[0].x as i32, vs[0].y as i32], [vs[1].x as i32, vs[1].y as i32], [vs[2].x as i32, vs[2].y as i32]];
        let l = &mut self.ovl.layers;
        let Some((idx, c)) = fx_native::find(&l.fxw, &mut l.fx_used, &mut l.fx_cur, &pts[..n]) else {
            l.fx.no_rec += 1;
            return false;
        };
        let w = l.fxw[idx].v;
        let ok = if n == 3 { self.capture(ty, vs, at, Some([w[c[0]], w[c[1]], w[c[2]]])) } else { self.fx_line(vs, at, [w[c[0]], w[c[1]]]) };
        let st = &mut self.ovl.layers;
        if ok {
            st.fx.hit += 1;
        } else {
            st.fx.no_mode += 1;
        }
        if let Some(t0) = t0 {
            st.stats.fx_ns += t0.elapsed().as_nanos() as u64;
        }
        ok
    }

    /// A line (one GS pixel wide) as a quad of two triangles: each end gets the pixel offset +- half a pixel across the line.
    fn fx_line(&mut self, vs: &[Vtx; 3], at: u64, w: [WVert; 2]) -> bool {
        let (dx, dy) = ((vs[1].x as f32 - vs[0].x as f32) / 16.0, (vs[1].y as f32 - vs[0].y as f32) / 16.0);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-3 {
            return false;
        }
        let n = [-dy / len * 0.5, dx / len * 0.5];
        let corner = |e: usize, s: f32| WVert { p: w[e].p, off: [n[0] * s, n[1] * s] };
        let (a0, a1, b0, b1) = (corner(0, 1.0), corner(0, -1.0), corner(1, 1.0), corner(1, -1.0));
        let (v0, v1) = (vs[0], vs[1]);
        self.capture(2, &[v0, v0, v1], at, Some([a0, a1, b0])) && self.capture(2, &[v0, v1, v1], at, Some([a1, b0, b1]))
    }

    /// An upload into blocks [lo, hi) started (`trx_start`): the cached layer textures there are stale.
    pub(super) fn layers_note_upload(&mut self, lo: u32, hi: u32) {
        let layers = layer_tris::on();
        if layers {
            // Triangles waiting for texels that this upload overwrites read them first.
            self.resolve(Some((lo, hi)));
        }
        if layers || texreq::tracking() {
            self.ovl.layers.note_upload(lo, hi);
        }
    }

    /// The upload counter (`texreq` stamps its reads with it; counted while layers or texture requests are on).
    pub fn texreq_gen(&self) -> u64 {
        self.ovl.layers.up_gen
    }

    /// The upload stamp of a `texreq` read of `tex0` now: one less while a transfer into its blocks is still receiving data (IMAGE data
    /// split around a draw), so the rest of that upload counts as "uploaded since" and is read again at the next draw.
    pub fn texreq_stamp(&self, tex0: u64) -> u64 {
        let g = self.ovl.layers.up_gen;
        match self.trx_open {
            Some((lo, hi, _)) if texreq::tex_spans(tex0).iter().any(|s| s.0 < hi && lo < s.1) => g.saturating_sub(1),
            _ => g,
        }
    }

    /// The upload counter of the last upload (or copy) into the texels of `tex0` (its palette not counted); 0 when none was counted.
    pub fn texreq_tex_upload(&self, tex0: u64) -> u64 {
        let (lo, hi) = texreq::tex_spans(tex0)[0];
        self.ovl.layers.last_upload(lo, hi)
    }

    /// Nothing was uploaded into the blocks of the texture `tex0` or its palette since the counter was `since`, and the texture is no
    /// render target: reading it again returns the same texels.
    pub fn texreq_untouched(&self, tex0: u64, since: u64) -> bool {
        let spans = texreq::tex_spans(tex0);
        if (0..2).any(|c| {
            let fb = self.fb_span(c);
            spans.iter().any(|s| s.0 < fb.1 && fb.0 < s.1)
        }) {
            return false;
        }
        !self.texreq_uploaded(tex0, since)
    }

    /// Something was uploaded (or copied) into the blocks of the texture `tex0` or its palette since the counter was `since` (render
    /// targets not counted): the game put other texels behind the TEX0 word (texreq re-reads it at its next draw).
    pub fn texreq_uploaded(&self, tex0: u64, since: u64) -> bool {
        let l = &self.ovl.layers;
        if l.blk_gen.is_empty() {
            return l.up_gen != since;
        }
        texreq::tex_spans(tex0).iter().any(|&(lo, hi)| l.blk_gen[(lo.min(NBLK)) as usize..hi.min(NBLK) as usize].iter().any(|&g| g > since))
    }

    /// The field about to be fed is a frozen-screen one (pause / title menu over the picture the game froze): its layers are not
    /// captured, the overlay draws them as it always did (the pack keeps the last native layers under the menu).
    pub fn set_layers_frozen(&mut self, frozen: bool) {
        self.ovl.layers.frozen = frozen;
    }

    /// The texture `tex0` for a layer: its texels from the cache (`Ready`), or a read that waits for the end of the field (`Pending`:
    /// the raster is finished then, so the read needs no mid-field sync). None: too many textures this field.
    fn layer_texture(&mut self, tex0: u64) -> Option<Src> {
        let key = tex0 & !(7u64 << 61);
        let now = self.ovl.layers.field_no;
        let l = &mut self.ovl.layers;
        let hint = l.idx.get(&key).copied().filter(|&i| l.cache.get(i).is_some_and(|e| e.key == key));
        if let Some(i) = hint.or_else(|| (0..l.cache.len()).find(|&i| l.cache[i].key == key)) {
            l.idx.insert(key, i);
            if l.fresh(i) {
                l.cache[i].last = now;
                l.stats.hits += 1;
                return Some(Src::Ready(l.cache[i].tex.clone(), l.cache[i].id));
            }
            l.cache.swap_remove(i);
        }
        if let Some(i) = l.pending.iter().position(|p| p.key == key) {
            return Some(Src::Pending(i));
        }
        let (w, h) = (1usize << ((tex0 >> 26) & 15).min(10), 1usize << ((tex0 >> 30) & 15).min(10));
        if l.cache.iter().filter(|e| e.last == now).count() + l.pending.len() >= MAX_TEX || w * h > 512 * 512 {
            return None;
        }
        let tex_span = self.cur_tex.map_or((0, NBLK), |s| (s.0, s.1.min(NBLK)));
        let cbp = ((tex0 >> 37) & 0x3fff) as u32;
        let clut_span = (cbp.min(NBLK), (cbp + 4).min(NBLK));
        // The draw flushed the texture's uploads; the palette's too.
        if self.any_pending(clut_span.0, clut_span.1) {
            self.flush(clut_span.0, clut_span.1);
        }
        // A texture drawn into by the prims queued so far must be read after them (and is never cached).
        let target = (0..2).any(|c| {
            let f = self.fb_span(c);
            tex_span.0 < f.1 && f.0 < tex_span.1
        });
        if target || legacy() {
            let (texa, texclut) = (self.r[0x3b], self.r[0x1c]);
            let (t, packed) = self.pack_texture(key, texa, texclut);
            let t = Arc::new(t);
            let id = layer_tris::texture_id(&t);
            let l = &mut self.ovl.layers;
            l.stats.reads += 1;
            if !target && !packed {
                let g = l.up_gen;
                l.cache.push(Entry { key, spans: [tex_span, clut_span], read_gen: g, checked: g, last: now, tex: t.clone(), id });
            }
            return Some(Src::Ready(t, id));
        }
        let l = &mut self.ovl.layers;
        l.pending.push(Pending { key, texa: self.r[0x3b], texclut: self.r[0x1c], spans: [tex_span, clut_span], req_gen: l.up_gen, tris: Vec::new() });
        Some(Src::Pending(l.pending.len() - 1))
    }

    /// The texels of `key` as local memory holds them, or the texture pack's image for them. The flag: the pack's file is still
    /// decoding, so this read is not kept in the cache (the next field asks again).
    fn pack_texture(&mut self, key: u64, texa: u64, texclut: u64) -> (Texture, bool) {
        let ptr = self.vram().as_ptr();
        let r = super::super::layer_repl::replace(&self.vram, key, texa, texclut);
        match r {
            super::super::layer_repl::Repl::Done(t) => (t, false),
            super::super::layer_repl::Repl::Pending => (unsafe { texreq::read(ptr, key) }, true),
            super::super::layer_repl::Repl::None => (unsafe { texreq::read(ptr, key) }, false),
        }
    }

    /// Reads the pending textures (all, or those whose blocks overlap [lo, hi)) from local memory, after the queued draws and uploads,
    /// gives them to their triangles and the cache.
    fn resolve(&mut self, only: Option<(u32, u32)>) {
        let hit = |p: &Pending| only.is_none_or(|(lo, hi)| p.spans.iter().any(|s| s.0 < hi && lo < s.1));
        if !self.ovl.layers.pending.iter().any(hit) {
            return;
        }
        self.finish();
        let now = self.ovl.layers.field_no;
        let all = std::mem::take(&mut self.ovl.layers.pending);
        let (take, keep): (Vec<Pending>, Vec<Pending>) = all.into_iter().partition(hit);
        self.ovl.layers.pending = keep;
        for p in take {
            let (t, packed) = self.pack_texture(p.key, p.texa, p.texclut);
            let t = Arc::new(t);
            let id = layer_tris::texture_id(&t);
            let l = &mut self.ovl.layers;
            l.stats.reads += 1;
            for &i in &p.tris {
                l.tris[i].tex = t.clone();
                l.tris[i].id = id;
            }
            if !packed {
                l.cache.push(Entry { key: p.key, spans: p.spans, read_gen: p.req_gen, checked: p.req_gen, last: now, tex: t, id });
            }
        }
    }

    /// A world-layer triangle of an overlay field: captured for the native draw (true = do not draw it here), or false when it is
    /// something the native draw does not reproduce (the overlay draws it as before): so nothing is ever removed without being drawn.
    pub(super) fn layer_capture(&mut self, ty: u32, vs: &[Vtx; 3], at: u64) -> bool {
        if !layer_tris::on() || !(3..=5).contains(&ty) || self.ovl.layers.tris.len() >= MAX_TRIS || self.ovl.layers.frozen {
            return false;
        }
        // The pack places a triangle by its depth words: one it could not place stays in the overlay.
        if vs.iter().any(|v| v.z == 0 || v.z >= 0x00ff_0000) {
            return false;
        }
        let t0 = timing().then(std::time::Instant::now);
        let r = self.capture(ty, vs, at, None);
        self.ovl.layers.fx.layer_kept += !r as u32;
        if let Some(t0) = t0 {
            self.ovl.layers.stats.ns += t0.elapsed().as_nanos() as u64;
        }
        r
    }

    fn capture(&mut self, _ty: u32, vs: &[Vtx; 3], at: u64, world: Option<[WVert; 3]>) -> bool {
        let c = ((at >> 9) & 1) as usize;
        let tex0 = self.r[0x06 + c];
        let m = if world.is_some() { fx_mode(at, self.r[0x47 + c], self.r[0x42 + c], tex0) } else { mode(at, self.r[0x47 + c], self.r[0x42 + c], tex0) };
        let Some((blend, decal)) = m else { return false };
        let (additive, replace) = (blend == Blend::Add, blend == Blend::Replace);
        // ZBUF ZMSK off: the prim writes depth (a replace sheet keeps what is drawn after it behind it hidden).
        let zwrite = replace && (self.r[0x4e + c] >> 32) & 1 == 0;
        // Its alpha test decides which pixels write colour and depth (`pass_layer_replace`); over / add layers pass every pixel.
        let alpha = if replace { layer_tris::AlphaTest::of(self.r[0x47 + c]) } else { layer_tris::AlphaTest::ALWAYS };
        let fst = at & 0x100 != 0;
        // The texture's size is in its TEX0 (what the read returns).
        let (w, h) = ((1u32 << ((tex0 >> 26) & 15).min(10)) as f32, (1u32 << ((tex0 >> 30) & 15).min(10)) as f32);
        let o = self.r[0x18 + c];
        let (ox, oy) = ((o & 0xffff) as f32, ((o >> 32) & 0xffff) as f32);
        let vert = |v: &Vtx| {
            let uv = if fst { [v.u as f32 / 16.0 / w, v.v as f32 / 16.0 / h] } else if v.q != 0.0 { [v.s / v.q, v.t / v.q] } else { [0.0; 2] };
            LayerVert { px: (v.x as f32 - ox) / 16.0, py: (v.y as f32 - oy) / 16.0, z: v.z, rgba: v.rgba, uv, f: if at & 0x20 != 0 { v.f } else { 255 } }
        };
        let v = [vert(&vs[0]), vert(&vs[1]), vert(&vs[2])];
        if v.iter().any(|p| !p.uv[0].is_finite() || !p.uv[1].is_finite()) {
            return false;
        }
        let src = if at & 0x10 != 0 { self.layer_texture(tex0) } else { Some(Src::Ready(self.ovl.layers.blank.get_or_insert_with(|| Arc::new(Texture { w: 1, h: 1, rgba: vec![[255; 4]] })).clone(), 0)) };
        let Some(src) = src else { return false };
        if timing() && at & 0x20 != 0 {
            let st = &mut self.ovl.layers.stats;
            let m = vs.iter().map(|v| v.f).min().unwrap_or(255);
            st.fog_min = if st.fog_n == 0 { m } else { st.fog_min.min(m) };
            st.fog_n += (m < 255) as u32;
        }
        let l = &mut self.ovl.layers;
        let (tex, id) = match &src {
            Src::Ready(t, id) => (t.clone(), *id),
            Src::Pending(_) => (l.blank.get_or_insert_with(|| Arc::new(Texture { w: 1, h: 1, rgba: vec![[255; 4]] })).clone(), 0),
        };
        if let Src::Pending(p) = src {
            l.pending[p].tris.push(l.tris.len());
        }
        // An effect prim honours its own depth test; a world layer is always tested.
        let depth = world.is_none() || depth_tested(self.r[0x47 + c]);
        if world.is_some() {
            l.fx.tested += depth as u32;
            l.fx.no_depth += !depth as u32;
        }
        l.tris.push(LayerTri { v, tex, id, additive, decal, world, depth, untextured: at & 0x10 == 0, replace, zwrite, alpha });
        true
    }

    /// The triangles captured since the last call (one field).
    pub fn take_layers(&mut self) -> Vec<LayerTri> {
        let t0 = timing().then(std::time::Instant::now);
        self.resolve(None);
        let l = &mut self.ovl.layers;
        if let Some(t0) = t0 {
            l.stats.ns += t0.elapsed().as_nanos() as u64;
        }
        l.field_no += 1;
        if legacy() {
            l.cache.clear();
        } else {
            let now = l.field_no;
            l.cache.retain(|e| now - e.last < 8);
            l.idx.clear();
        }
        let n = l.tris.len();
        let fx = std::mem::take(&mut l.fx);
        if fx_log() && (fx.hit + fx.no_rec + fx.no_mode + fx.no_type + fx.layer_kept > 0 || !l.fxw.is_empty()) {
            eprintln!("fxnative field {}: {} records, {} prims native ({} depth-tested, {} not), {} without a record, {} with a blend the layers do not reproduce, {} of a type not handled; {} world-layer prims left in the overlay", l.field_no, l.fxw.len(), fx.hit, fx.tested, fx.no_depth, fx.no_rec, fx.no_mode, fx.no_type, fx.layer_kept);
        }
        l.fxw.clear();
        if timing() && n > 0 {
            let st = std::mem::take(&mut l.stats);
            let cap = fx_native::take_cap_ns();
            eprintln!("layertime field {}: {} tris, {:.3} ms, {} texture reads, {} cache hits, fog: {} tris with F < 255 (min F {}), fogcol {:#x}; fx: {:.3} ms find+capture, {:.3} ms records (VIF thread)", l.field_no, n, st.ns as f64 / 1e6, st.reads, st.hits, st.fog_n, st.fog_min, self.r[0x3d], st.fx_ns as f64 / 1e6, cap as f64 / 1e6);
        } else {
            l.stats = Stats::default();
        }
        std::mem::take(&mut self.ovl.layers.tris)
    }
}

#[cfg(test)]
mod tests {
    use super::{Blend, Entry, Layers, fx_mode, mode};
    use crate::level::Texture;
    use std::sync::Arc;

    #[test]
    fn the_canopy_and_lamp_glows_are_reproducible() {
        let tme_abe = 0x10 | 0x40;
        // Canopy glass: TEST 0x5380b ALPHA 0x44, T8 texture, modulate.
        assert_eq!(mode(tme_abe, 0x5380b, 0x44, 0x13 << 20), Some((Blend::Over, false)));
        // Additive glow (ALPHA 0x48), decal.
        assert_eq!(mode(tme_abe, 0x53001, 0x48, (0x00 << 20) | (1 << 35)), Some((Blend::Add, true)));
    }

    #[test]
    fn the_blend_table_takes_over_additive_and_replace_and_nothing_else() {
        let tme_abe = 0x10 | 0x40;
        // Level-14 lava: PRIM 0x7c, TEST 0x5380b, ALPHA 0x8000000088 (FIX 0x80 is not read: C = As), TEX0 PSMT8 modulate.
        let lava = 0x8006_5985_dd30_b2d0u64;
        assert_eq!(mode(0x7c, 0x5380b, 0x80_0000_0088, lava), Some((Blend::Replace, false)));
        // Level 6 windows 0x44 and the Aranos glows 0x48 keep their kinds.
        assert_eq!(mode(tme_abe, 0x5360b, 0x44, 0x8006_46a5_9930_728c), Some((Blend::Over, false)));
        assert_eq!(mode(tme_abe, 0x5380b, 0x80_0000_0048, 0x13 << 20), Some((Blend::Add, false)));
        // Every (A, B, C, D) but the three: none (the overlay draws them). C = Ad / FIX, B = Cs, D = Cs, A = Cd / 0.
        for alpha in 0..0x100u64 {
            let want = match alpha {
                0x44 => Some(Blend::Over),
                0x48 => Some(Blend::Add),
                0x88 => Some(Blend::Replace),
                _ => None,
            };
            assert_eq!(mode(tme_abe, 0x5380b, alpha, 0x13 << 20).map(|m| m.0), want, "ALPHA {alpha:#x}");
        }
        // The effect program's prims are over / additive only: a replace one stays in the overlay.
        assert_eq!(fx_mode(tme_abe, 0x5380b, 0x88, 0x13 << 20), None);
        assert_eq!(fx_mode(0x40, 0x5380b, 0x88, 0), None);
        assert_eq!(fx_mode(tme_abe, 0x5380b, 0x48, 0x13 << 20), Some((Blend::Add, false)));
    }

    #[test]
    fn a_replace_layer_takes_every_alpha_test_and_decodes_it() {
        use super::layer_tris::AlphaTest;
        let tme_abe = 0x10 | 0x40;
        // Every AFAIL (KEEP, FB_ONLY, ZB_ONLY, RGB_ONLY) of an alpha-tested replace prim is drawn natively; over / add keep RGB_ONLY only.
        for afail in 0..4u64 {
            let test = (0x5380b & !0x3000) | afail << 12;
            assert_eq!(mode(tme_abe, test, 0x88, 0x13 << 20).map(|m| m.0), Some(Blend::Replace), "AFAIL {afail}");
            assert_eq!(mode(tme_abe, test, 0x44, 0x13 << 20).is_some(), afail == 3, "over, AFAIL {afail}");
        }
        // The lava's TEST 0x5380b: ATE, ATST GEQUAL, AREF 0x80, AFAIL RGB_ONLY (ZTE, ZTST GEQUAL above bit 16 are not read here).
        let lava = AlphaTest::of(0x5380b);
        assert_eq!(lava, AlphaTest { atst: 5, aref: 0x80, afail: 3 });
        assert!(lava.passes(0x80) && lava.passes(0xff) && !lava.passes(0x7f));
        // ATE off: every pixel passes, whatever ATST / AREF / AFAIL say.
        assert_eq!(AlphaTest::of(0x5380a), AlphaTest::ALWAYS);
        assert_eq!(AlphaTest::of(0x30000), AlphaTest::ALWAYS);
        // ATST: NEVER, ALWAYS, LESS, LEQUAL, EQUAL, GEQUAL, GREATER, NOTEQUAL against AREF 0x40.
        let want = [[false, false, false], [true, true, true], [true, false, false], [true, true, false], [false, true, false], [false, true, true], [false, false, true], [true, false, true]];
        for (atst, w) in want.iter().enumerate() {
            let t = AlphaTest::of(1 | (atst as u64) << 1 | 0x40 << 4);
            assert_eq!(t.aref, 0x40);
            assert_eq!([t.passes(0x3f), t.passes(0x40), t.passes(0x41)], *w, "ATST {atst}");
        }
        assert_eq!(AlphaTest { atst: 5, aref: 0x80, afail: 3 }.word(), 5 | 0x80 << 3);
    }

    #[test]
    fn the_replace_draws_follow_afail_and_zmsk() {
        use super::layer_tris::AlphaTest;
        let t = |atst, afail| AlphaTest { atst, aref: 0x80, afail };
        // Depth-writing prim (the lava): passing pixels colour + depth; failing ones per AFAIL in a second draw.
        assert_eq!(t(5, 3).sides(true), ((true, true), Some((true, false))), "RGB_ONLY: colour, no depth");
        assert_eq!(t(5, 1).sides(true), ((true, true), Some((true, false))), "FB_ONLY: colour, no depth");
        assert_eq!(t(5, 2).sides(true), ((true, true), Some((false, true))), "ZB_ONLY: depth only");
        assert_eq!(t(5, 0).sides(true), ((true, true), None), "KEEP: nothing");
        // No depth writes (ZMSK on): RGB_ONLY / FB_ONLY failing pixels write what passing ones do -> one draw, no test.
        assert_eq!(t(5, 3).sides(false), ((true, false), None));
        assert_eq!(t(5, 3).first_draw(false), AlphaTest::ALWAYS);
        assert_eq!(t(5, 2).sides(false), ((true, false), None), "ZB_ONLY without depth writes: a failing pixel writes nothing");
        assert_eq!(t(5, 2).first_draw(false), t(5, 2));
        assert_eq!(t(5, 3).first_draw(true), t(5, 3), "the first draw keeps the test when the sides differ");
        // ATST ALWAYS / ATE off: one draw.
        assert_eq!(AlphaTest::ALWAYS.sides(true), ((true, true), None));
        assert_eq!(t(1, 0).sides(true), ((true, true), None));
        // NEVER: the first draw keeps nothing, the failing side draws every pixel.
        assert_eq!(t(0, 3).sides(true), ((true, true), Some((true, false))));
    }

    #[test]
    fn cut_outs_and_unknown_blends_stay_in_the_overlay() {
        let tme_abe = 0x10 | 0x40;
        // ATST GEQUAL with AFAIL KEEP cuts pixels out.
        assert_eq!(mode(tme_abe, 0x1380b & !0x3000, 0x44, 0x13 << 20), None);
        // Fixed alpha (C = 2), no blend, no texture, highlight.
        assert_eq!(mode(tme_abe, 0x5380b, 0x64, 0x13 << 20), None);
        assert_eq!(mode(0x10, 0x5380b, 0x44, 0x13 << 20), None);
        assert_eq!(mode(0x40, 0x5380b, 0x44, 0x13 << 20), None);
        assert_eq!(mode(tme_abe, 0x5380b, 0x44, (0x13 << 20) | (2 << 35)), None);
    }

    #[test]
    fn an_upload_into_the_texture_or_its_palette_makes_the_cached_texels_stale() {
        let mut l = Layers::default();
        let t = Arc::new(Texture { w: 1, h: 1, rgba: vec![[0; 4]] });
        l.note_upload(0, 4); // before the read: harmless
        l.cache.push(Entry { key: 1, spans: [(100, 108), (200, 204)], read_gen: l.up_gen, checked: l.up_gen, last: 0, tex: t, id: 1 });
        assert!(l.fresh(0));
        l.note_upload(300, 310); // elsewhere
        assert!(l.fresh(0), "an upload elsewhere keeps the entry");
        l.note_upload(107, 109); // the last texture block
        assert!(!l.fresh(0), "an upload into the texture");
        l.cache[0].read_gen = l.up_gen;
        l.cache[0].checked = l.up_gen;
        assert!(l.fresh(0));
        l.note_upload(203, 204); // the palette
        assert!(!l.fresh(0), "an upload into the palette");
    }

    #[test]
    fn the_last_upload_into_a_span_is_its_newest_block_stamp() {
        let mut l = Layers::default();
        assert_eq!(l.last_upload(0, 4), 0, "nothing counted yet");
        l.note_upload(2, 3);
        let g = l.up_gen;
        l.note_upload(10, 20);
        assert_eq!(l.last_upload(0, 4), g, "the upload into block 2");
        assert_eq!(l.last_upload(4, 10), 0, "never uploaded");
        assert_eq!(l.last_upload(0, 16), l.up_gen);
    }
}
