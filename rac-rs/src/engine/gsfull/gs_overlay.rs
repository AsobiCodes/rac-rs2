//! R&C2 HUD overlay mode of the software GS (`overlay.rs` drives it): while rac-rs draws the 3D world natively, the GS draws
//! only the HUD (and the effect programs `overlay::progs` still runs) into a cleared, see-through buffer; the window lays
//! that over the native picture. Off (`GsOvl::on` false) for every R&C1 field. Child of `gs.rs` (Gs's private fields).
use super::super::gs_mem::{a32, luts, span};
use super::super::gs_raster::Ctx;
use super::super::grab::Grab;
use super::super::ovl_vote::PLANE;
use super::Gs;

/// Overlay state of a field (Gs::ovl).
#[derive(Default)]
pub(in crate::gsfull) struct GsOvl {
    /// HUD overlay field (overlay::raster): on, the shown size, the buffer the HUD draws into this field (FBP blocks, FBW)
    /// and last field's.
    pub(in crate::gsfull) on: bool,
    size: (u32, u32),
    target: Option<(u32, u32)>,
    /// The part of the overlay buffer cleared this field (w, h): the HUD context's SCISSOR within the shown size.
    clip: (u32, u32),
    last: Option<u32>,
    /// The segment being fed is an effect program's (overlay::raster): its overlay pixels are depth-tested (Ctx::ozfx).
    pub(super) fx: bool,
    /// The prim being drawn is a world layer (`world_layer`): drawn into the overlay like an effect.
    pub(super) layer: bool,
    /// [b46 fix round] The qwords fed next are a flight field's cosmos (overlay::raster, `OvlField::space_at`): the picture's lowest
    /// layer. Its overlay pixels are kept apart from the HUD's (`Ctx::ozfar`, `ovl_vote::SPACE`: drawn under the whole native scene), and this GS draws them also
    /// when the GPU capture draws the HUD (`skip_hud` leaves `ozfx` draws alone).
    pub(super) space: bool,
    /// The world layers captured this field for the native draw (`gs_layers.rs`).
    pub(super) layers: super::layers::Layers,
    /// The GPU capture draws this field's HUD (`set_hud_skip`; gscap/rc2.rs): the HUD prims are not drawn here, only the effects.
    pub(in crate::gsfull) skip_hud: bool,
    /// Effect Z per overlay pixel (1024 x 1024; allocated by the first overlay field): the depth of the effect that coloured the pixel
    /// most (`ovl_vote.rs`), then the same size of vote weights.
    z: Vec<u32>,
    /// See-through byte per overlay pixel (1024 x 1024, 255 = all of the native picture; `overlay_px` keeps it, `overlay_cover`
    /// reads it). Its own plane, not the buffer's top byte: the game keeps data there (Boldan: Ratchet's and his ship's T8H
    /// indices at TBP 0..0x6ff, under a 512 x 512 HUD buffer at FBP 0), which a clear to 0xff000000 turned into one colour.
    t: Vec<u8>,
    /// The grabbed-backdrop area (grab.rs; `set_grab`): None = off.
    pub(super) grab: Option<Grab>,
    /// The IMAGE transfer in progress lands in the area: its data is dropped (`Gs::image`; reset by every TRXDIR write).
    pub(super) trx_drop: bool,
    /// The depth planes were cleared for this field's frozen-screen menu (`menu_z_reset`).
    z_reset: bool,
}

impl Gs {
    /// Path, VU1 program and HUD flag of the data fed next (overlay fields: the overlay context depends on the HUD flag).
    pub fn set_seg(&mut self, path: u8, prog: u8, hud: bool) {
        if hud != self.hud {
            self.ctx_dirty = true;
        }
        (self.path, self.prog, self.hud) = (path, prog, hud);
    }

    /// Starts a field in HUD overlay mode (shown size w x h) or not. The game draws the HUD into its back buffer (Aranos:
    /// FBP 0x1e00 blocks) and copies that to the shown one at the next field's start [C: rac2 shots RAC2_FBLOG field 2540],
    /// so the overlay is the buffer of the first HUD draw, cleared just before it; last field's buffer gets no scene draws.
    pub fn set_overlay(&mut self, on: bool, size: (u32, u32)) {
        if on != self.ovl.on {
            self.ctx_dirty = true;
        }
        self.ovl.last = if on { self.ovl.target.map(|o| o.0) } else { None };
        (self.ovl.on, self.ovl.size, self.ovl.target) = (on, size, None);
        self.ovl.z_reset = false;
    }

    /// A frozen-screen menu field starts (its first upload into the grabbed area, `trx_start`): the GS depth planes still hold the last
    /// world that was drawn here (the live fields skip the world programs, so nothing refreshed them), and the menu's own 3D (the vendor's
    /// weapon model) would be depth-tested against that stale world: parts of it vanish. Cleared, once per field, behind the raster.
    /// Also before the first world layer the overlay draws in a field (gs.rs `draw`): the same stale world hid it.
    pub(super) fn menu_z_reset(&mut self) {
        if !self.ovl.on || self.ovl.z_reset {
            return;
        }
        self.ovl.z_reset = true;
        let t0 = super::layers::fx_log().then(std::time::Instant::now);
        self.finish();
        self.zplanes.iter_mut().for_each(|p| p.fill(0));
        if let Some(t0) = t0 {
            let ms = t0.elapsed().as_secs_f64() * 1e3;
            eprintln!("zreset field {}: {} depth planes cleared, {ms:.3} ms with the raster finish", self.ovl.layers.field_no, self.zplanes.len());
        }
    }

    /// The grabbed-backdrop area of the fields fed next (None: off): the IMAGE uploads that land in it are dropped (grab.rs).
    pub fn set_grab(&mut self, grab: Option<Grab>) {
        self.ovl.grab = grab;
    }

    /// The segment fed next is an effect program's (its overlay pixels are depth-tested against the native picture).
    pub fn set_fx(&mut self, fx: bool) {
        if fx != self.ovl.fx {
            self.ctx_dirty = true;
        }
        self.ovl.fx = fx;
    }

    /// [b46 fix round] The qwords fed next are (not) a flight field's cosmos (`GsOvl::space`).
    pub fn set_space(&mut self, space: bool) {
        if space != self.ovl.space {
            self.ctx_dirty = true;
        }
        self.ovl.space = space;
    }

    /// The HUD of the overlay fields fed next is drawn by the GPU capture at the window's resolution (gscap/rc2.rs): this GS then
    /// draws only the effect programs' and the world layers' overlay pixels (nothing is drawn twice).
    pub fn set_hud_skip(&mut self, on: bool) {
        self.ovl.skip_hud = on;
    }

    /// The overlay's depth for the w x h shown rectangle: per pixel the view depth (metres) of the effect that coloured it most
    /// (`ovl_vote.rs`; one depth per pixel), 0 where the HUD drew or nothing did (always in front). GS Z = 2^24 Q - C with Q = 1 / (32 depth) for
    /// every R&C2 VU1 program [C: level 1 field 2700 vertices of programs 2-11, RAC2_ZQLOG: C = 480], so depth = 524288 / (Z + C); C is per
    /// level (480 / 142.76 / 96: `layer_tris::z_offset_of`).
    pub fn overlay_depth(&mut self, w: u32, h: u32) -> Vec<f32> {
        self.finish();
        let mut out = vec![0f32; (w * h) as usize];
        if self.ovl.z.is_empty() {
            return out;
        }
        let c = crate::gsfull::layer_tris::z_offset();
        for y in 0..h.min(1024) {
            for x in 0..w.min(1024) {
                let z = self.ovl.z[(y * 1024 + x) as usize];
                if z == super::super::ovl_vote::SPACE {
                    out[(y * w + x) as usize] = super::super::ovl_vote::SPACE_M; // [b46] the flight's cosmos: under the whole scene
                } else if z != 0 && z != u32::MAX {
                    out[(y * w + x) as usize] = crate::gsfull::layer_tris::depth_with(z, c);
                }
            }
        }
        out
    }

    /// The buffer (FBP blocks, FBW) the HUD went into this field (None: no HUD draw).
    pub fn overlay_target(&self) -> Option<(u32, u32)> {
        self.ovl.target
    }

    /// The cleared part of the overlay buffer (w, h); outside it the buffer holds other data, not HUD.
    pub fn overlay_clip(&self) -> (u32, u32) {
        self.ovl.clip
    }

    /// Clears the shown rectangle (FBP blocks, FBW, DBX, DBY, w, h) to black (rgb only: the top byte is the game's data, R&C2 keeps
    /// T8H texture indices there [C: Boldan, RAC2_OVLLOG + RAC2_TEXDUMP]), fully see-through (plane `t` 255).
    fn overlay_clear(&mut self, (fbp, fbw, x0, y0, w, h): (u32, u32, u32, u32, u32, u32)) {
        let (lo, hi) = span(0, fbp, fbw, x0, y0, w, h);
        self.flush(lo, hi);
        self.finish();
        if self.ovl.t.is_empty() {
            self.ovl.t = vec![255; PLANE];
        }
        let l = luts();
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                let a = a32(l, fbp, fbw, x, y);
                self.vram[a..a + 3].fill(0);
                // The depth plane is 1024 x 1024 and the weight plane follows it at `PLANE`: bound the row as well as the column
                // (the z buffer holds two planes, so a row >= 1024 would pass `get_mut` and index the weights out of range).
                if x < 1024 && y < 1024 {
                    let i = (y * 1024 + x) as usize;
                    self.ovl.t[i] = 255;
                    if !self.ovl.z.is_empty() {
                        self.ovl.z[i] = 0;
                        self.ovl.z[PLANE + i] = 0;
                    }
                }
            }
        }
    }

    /// The overlay picture's alpha (`crtc::present_overlay`): rgba holds the w x h HUD buffer read from (0, 0); each pixel's alpha
    /// becomes 255 - its see-through byte (plane `t`; 255 = see-through outside the plane or before the first overlay field).
    pub fn overlay_cover(&self, rgba: &mut [u8], w: u32) {
        for (i, p) in rgba.chunks_exact_mut(4).enumerate() {
            let (x, y) = (i as u32 % w.max(1), i as u32 / w.max(1));
            let t = if x < 1024 && y < 1024 { self.ovl.t.get((y * 1024 + x) as usize).copied().unwrap_or(255) } else { 255 };
            p[3] = 255 - t;
        }
    }

    /// make_ctx in an overlay field: the first HUD draw picks (and clears) the overlay buffer; draws into it get Ctx::ovl.
    pub(super) fn overlay_ctx(&mut self, cx: &mut Ctx, fbp: u32, fbw: u32, fpsm: u32, sc: u64) {
        let hud = self.hud || self.ovl.layer;
        if hud && fpsm <= 1 && self.ovl.target.is_none() {
            self.ovl.target = Some((fbp, fbw));
            // Only the drawing area (SCISSOR): the game keeps CLUTs right after its 512 x 448 HUD buffer, which a clear of
            // the 512-line shown size wiped [C: Aranos FBP 0x1e00, Ratchet's T8H CLUTs at CBP 0x2c00..0x2cac; Ratchet
            // black in the window].
            let (w, h) = self.ovl.size;
            let (w, h) = (w.min(((sc >> 16) & 0x7ff) as u32 + 1), h.min(((sc >> 48) & 0x7ff) as u32 + 1));
            self.ovl.clip = (w, h);
            if std::env::var_os("RAC2_OVLLOG").is_some() {
                eprintln!("overlay: fbp {fbp:#x} fbw {fbw} shown {:?} scissor {sc:#x} clear {w}x{h}", self.ovl.size);
            }
            self.overlay_clear((fbp, fbw, 0, 0, w, h));
        }
        cx.ovl = hud && fpsm <= 1 && self.ovl.target.map(|o| o.0) == Some(fbp);
        if cx.ovl {
            if self.ovl.z.is_empty() {
                self.ovl.z = vec![0; 2 * PLANE];
            }
            (cx.oz, cx.ozfx) = (self.ovl.z.as_mut_ptr() as usize, self.ovl.fx || self.ovl.layer || self.ovl.space);
            cx.ozfar = self.ovl.space;
            // The clear above allocated the see-through plane (the target is always cleared before its first draw).
            cx.ot = if self.ovl.t.is_empty() { 0 } else { self.ovl.t.as_mut_ptr() as usize };
        }
    }

    /// Debug inventory (RAC2_OVLKIND=1): every distinct kind of prim that reaches the overlay is logged once (`kind` fx = an effect
    /// program's, layer = a world layer the EE sends itself, hud; GIF path, VU1 program, prim type, TEST, ALPHA, TEX0, first vertex).
    pub(super) fn kind_log(&self, ty: u32, at: u64, v0: (u16, u16, u32)) {
        static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if !*ON.get_or_init(|| std::env::var_os("RAC2_OVLKIND").is_some()) {
            return;
        }
        static SEEN: std::sync::Mutex<Vec<(u8, u8, u8, u32, u64, u64, u64)>> = std::sync::Mutex::new(Vec::new());
        let c = ((at >> 9) & 1) as usize;
        let kind = if self.ovl.layer { 2 } else if self.ovl.fx { 1 } else { 0 };
        // The texture slot changes every field for an animated one: key on the TEX0 without its base pointer.
        let key = (kind, self.path, self.prog, ty, self.r[0x47 + c], self.r[0x42 + c], self.r[0x06 + c] & !0x3fff);
        let Ok(mut seen) = SEEN.lock() else { return };
        if seen.contains(&key) {
            return;
        }
        seen.push(key);
        let name = ["hud", "fx", "layer"][kind as usize];
        eprintln!("ovlkind {name} path {} prog {} ty {ty} test {:#x} alpha {:#x} tex0 {:#x} at {at:#x} v0 ({},{}) z {}", self.path, self.prog, key.4, key.5, self.r[0x06 + c], v0.0 / 16, v0.1 / 16, v0.2);
    }

    /// draw in an overlay field: true = skip this prim. A world layer (`world_layer`) is drawn into the overlay as an effect;
    /// the rest of what is drawn before the HUD (the sky, the world clears) is not needed.
    pub(super) fn overlay_draw(&mut self, at: u64) -> bool {
        let layer = self.world_layer(at);
        if layer != self.ovl.layer {
            self.ovl.layer = layer;
            self.ctx_dirty = true;
        }
        !layer && self.overlay_skip(at)
    }

    /// One of the game's own world layers that the native picture does not draw (`zlayer::is_world_layer`, shared with the GPU
    /// capture): a prim that the EE sends itself (not a VU1 program's output: VIF DIRECT or GIF), tests depth against the world
    /// drawn before it (ZTE, ZTST GEQUAL / GREATER) and draws a colour of its own, also in the HUD part of the field (the lamp
    /// glows come after the last PATH1 segment). Level 6's lit tower windows: white quads (TEST 0x5360b, ALPHA 0x44, TEX0
    /// 0x800646a59930728c) laid over the tie bodies by VIF DIRECT, still drawn with every VU1 program skipped [C: level 6 field
    /// 3000, emulated picture; RAC_GSPIX 174,107]. Not the shadow passes (alpha test NEVER, sampling the frame they draw into [C:
    /// same field, RAC_GSPIX 335,425]) or a draw with RGB masked off: as overlay pixels they came out as black blobs.
    fn world_layer(&self, at: u64) -> bool {
        let c = ((at >> 9) & 1) as usize;
        // [b46 U5c] not a dst-alpha pass (glass platform heat haze: black box in the plane; zlayer.rs is_layer_draw)
        crate::gsfull::zlayer::is_layer_draw(self.path, at, self.r[0x47 + c], self.r[0x06 + c], self.r[0x4c + c], self.r[0x42 + c])
    }

    /// What is drawn into the shown buffer before the HUD is not needed, and a HUD-part draw that samples the HUD buffer
    /// itself is a screen effect over the 3D picture (Aranos: a full-screen sprite reading FBP 0x1e00), which the native
    /// picture does not have yet [P].
    fn overlay_skip(&self, at: u64) -> bool {
        let c = ((at >> 9) & 1) as usize;
        if let Some(fb) = self.ovl.last.filter(|_| !self.hud) {
            if (self.r[0x4c + c] & 0x1ff) as u32 * 32 == fb {
                return true;
            }
        }
        if self.hud && crate::gsfull::zlayer::is_dst_alpha_pass(at, self.r[0x42 + c]) {
            return true; // the hover-board flame's alpha mask / frame-copy blend: black boxes in the plane (zlayer.rs)
        }
        if self.hud && crate::gsfull::zlayer::is_frame_filter(at, self.r[0x42 + c]) {
            return true; // [b50 V5] the Visibomb view's subtractive screen filter: opaque black over the native view (zlayer.rs)
        }
        self.hud && at & 0x10 != 0 && (self.r[0x06 + c] & 0x3fff) as u32 == (self.r[0x4c + c] & 0x1ff) as u32 * 32
    }
}

#[cfg(test)]
mod tests {
    use super::super::VRAM_SIZE;
    use super::*;

    /// Review B3/4 R5: the depth buffer holds two planes, so a cleared row >= 1024 used to pass `get_mut` and index the weight plane
    /// out of range (a garbage CRTC / SCISSOR height). Rows below 1024 are cleared in both planes, the rest is left alone, no panic.
    #[test]
    fn overlay_clear_stays_inside_the_planes_for_tall_rectangles() {
        let mut g = Gs::new(vec![0u8; VRAM_SIZE], 0);
        g.ovl.z = vec![7; 2 * PLANE];
        g.overlay_clear((0, 16, 0, 1020, 16, 12));
        let at = |g: &Gs, plane: usize, y: usize, x: usize| g.ovl.z[plane + y * 1024 + x];
        for y in 1020..1024 {
            assert_eq!((at(&g, 0, y, 3), at(&g, PLANE, y, 3)), (0, 0), "row {y} is cleared in both planes");
        }
        assert_eq!((at(&g, 0, 1019, 3), at(&g, PLANE, 1019, 3)), (7, 7), "the row above the rectangle is kept");
        assert_eq!((at(&g, 0, 0, 3), at(&g, PLANE, 0, 3)), (7, 7), "row 1024 did not wrap onto row 0");
        // An overlay buffer that was never allocated is no error either.
        let mut g = Gs::new(vec![0u8; VRAM_SIZE], 0);
        g.overlay_clear((0, 16, 0, 1020, 16, 12));
        assert!(g.ovl.z.is_empty());
    }

    /// Batch 38 (Boldan: teal Ratchet + ship after the travel): the game keeps T8H texture indices in the top byte of a frame buffer
    /// that one overlay field used as its HUD buffer (FBP 0, 512 x 512). The clear blacks only rgb, keeps that byte, and the see-through
    /// state (255) lives in the plane that `overlay_cover` reads.
    #[test]
    fn overlay_clear_keeps_the_games_top_byte() {
        let mut g = Gs::new(vec![0u8; VRAM_SIZE], 0);
        let l = luts();
        for (x, y) in [(0, 0), (37, 5), (511, 447)] {
            let a = a32(l, 0, 8, x, y);
            g.vram[a..a + 4].copy_from_slice(&0x5a12_3456u32.to_le_bytes());
        }
        g.overlay_clear((0, 8, 0, 0, 512, 448));
        for (x, y) in [(0, 0), (37, 5), (511, 447)] {
            let a = a32(l, 0, 8, x, y);
            assert_eq!(u32::from_le_bytes(g.vram[a..a + 4].try_into().unwrap()), 0x5a00_0000, "({x},{y}): rgb black, T8H index kept");
        }
        let mut rgba = vec![9u8; 4 * 4];
        g.overlay_cover(&mut rgba, 2);
        assert!(rgba.chunks_exact(4).all(|p| p[3] == 0), "a cleared pixel is fully see-through");
        g.ovl.t[1024 + 1] = 64;
        g.overlay_cover(&mut rgba, 2);
        assert_eq!((rgba[3], rgba[15]), (0, 191), "alpha = 255 - the plane's byte at (x, y)");
        // No overlay field yet: everything shows the native picture.
        let g = Gs::new(vec![0u8; VRAM_SIZE], 0);
        let mut rgba = vec![9u8; 8];
        g.overlay_cover(&mut rgba, 2);
        assert_eq!((rgba[3], rgba[7]), (0, 0));
    }
}
