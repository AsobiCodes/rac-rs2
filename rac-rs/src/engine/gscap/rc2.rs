//! The sharp HUD of R&C2 (engine split stage E6, row 17): the GPU capture draws the HUD of the overlay fields a second time at the
//! window's resolution (the kernel `k_hd` of `draw/gpu.rs`, as R&C1's HUD), fed with the same GIF qwords as the software GS
//! (gsfull/ovl_pipe.rs) and routed like that GS routes them (gsfull/gs_overlay.rs): the HUD prims of an overlay field draw into the
//! overlay plane, the effect programs' prims and the world layers stay with the software GS (they are depth-tested against the
//! native picture there), everything else draws into local memory so later prims can sample it. R&C1's routing (`draw.rs draw_in`:
//! the main buffer learnt from copy sprites, display-buffer pictures, full / load modes) is not used while `Rc2::on` is set.
//! A field that is not an overlay field (the title and menus before the world: the software GS's whole picture) only keeps the
//! GPU's local memory up to date (`no_draw`); a field of an overlay window whose sharp picture is off does the same.
use super::{draw::gpu, hd_target, Gs, Hd, Overlay, Vtx};
use crate::gsfull::overlay::{hud_at, is_fx, last_world_seg, OvlField};

/// R&C2 routing state of the capture (`Gs::rc2`).
#[derive(Default)]
pub(super) struct Rc2 {
    /// R&C2 mode (set by the first `rc2_field`; R&C1 never sets it).
    pub on: bool,
    /// This field is an overlay field (its prims are routed even when it draws nothing: `target` follows the game's HUD buffer).
    pub ovl: bool,
    /// This field's HUD is drawn here (an overlay field, the GPU capture attached, the sharp scale on and for this frame's rows).
    draw_ov: bool,
    /// The segment being fed: a HUD / effect segment, an effect program's, its GIF path (overlay.rs `is_hud`, `is_fx`).
    hud: bool,
    fx: bool,
    path: u8,
    /// [b46 fix round] The qwords being fed are a flight field's cosmos (`OvlField::space_at`): the software GS draws them (far depth).
    space: bool,
    /// The buffer (FBP) the first HUD draw of this field went into, and last field's in blocks (gsfull `GsOvl::target` / `last`).
    target: Option<u32>,
    last: Option<u32>,
    /// [b46 U3] The HUD buffer (FBP) of the newest field that drew a HUD prim. An in-engine cutscene draws no HUD, so `target`
    /// stayed None and `last` with it: the scene's whole effect stack then went into the GPU's local memory, ~900 prims a field
    /// that sample their own target (k_serial, ~850 ms of GPU a field at a 1440p window: the cutscenes ran at ~1 fps, build 45
    /// run b46/u3 c184_hdprof). Gameplay fields skip those draws by `last` (`rc2_skip`); cutscene fields now do the same.
    seen: Option<u32>,
}

impl Gs {
    /// The GPU capture is attached (or still opening), so the host must feed it every field: its local memory has to hold every
    /// upload from the start for the HUD to find its textures when the sharp picture is first drawn.
    pub fn rc2_live(&self) -> bool {
        self.gpu.is_some() || self.gpu_wait != 0
    }

    /// The next overlay field of `rows` rows gets its HUD from `rc2_field` (the GPU capture is attached, the sharp scale is on and
    /// was set for this many rows, the shown picture is the 512 GS pixels wide that scale assumes): the software GS then skips the HUD.
    pub fn rc2_hd_ready(&self, rows: u32) -> bool {
        self.gpu.is_some() && hd_target().0 != 0 && hd_target().2 == rows
    }

    /// Draw routing of an R&C2 field (`draw.rs draw`): the port of gsfull `overlay_draw` / `overlay_ctx` for the capture.
    pub(super) fn rc2_draw(&mut self, ty: u32, vs: &[Vtx]) {
        let at = self.attrs();
        let c = ((at >> 9) & 1) as usize;
        let frame = self.r[0x4c + c];
        let (fbp, fpsm) = ((frame & 0x1ff) as u32, ((frame >> 24) & 0x3f) as u32);
        let layer = self.rc2_world_layer(at);
        if !layer && (self.rc2_skip(at) || (self.rc2.hud && crate::gsfull::zlayer::is_dst_alpha_pass(at, self.r[0x42 + c]))) {
            return; // (the second: hover-board flame passes, zlayer.rs is_dst_alpha_pass)
        }
        if !layer && self.rc2.hud && crate::gsfull::zlayer::is_frame_filter(at, self.r[0x42 + c]) {
            return; // [b50 V5] the Visibomb view's subtractive screen filter (zlayer.rs is_frame_filter)
        }
        let hud = self.rc2.hud || layer;
        if hud && fpsm <= 1 && self.rc2.target.is_none() {
            self.rc2.target = Some(fbp);
        }
        let ov = hud && fpsm <= 1 && self.rc2.target == Some(fbp);
        // An overlay field without the sharp picture only follows the HUD buffer (the software GS draws it all).
        if self.no_draw {
            return;
        }
        if self.tracing() {
            let v1 = vs.get(1).copied().unwrap_or_default();
            eprintln!("gscap f{} rc2 prim ty {ty} at {at:x} fbp {fbp:x} fpsm {fpsm:x} hud {hud} fx {} path {} layer {layer} ov {ov} draw_ov {} tex0 {:x} v0 ({},{}) {:?} v1 ({},{}) {:?}", self.fields, self.rc2.fx, self.rc2.path, self.rc2.draw_ov, self.r[0x06 + c], vs[0].x / 16, vs[0].y / 16, vs[0].rgba, v1.x / 16, v1.y / 16, v1.rgba);
        }
        // The software GS draws the effects and the world layers (it keeps their depth); a HUD prim only when this field has no sharp picture.
        if ov && (self.rc2.fx || self.rc2.space || layer || !self.rc2.draw_ov) {
            return;
        }
        let t0 = self.prof.is_some().then(std::time::Instant::now);
        self.draw_tail(ty, vs, at, ov, false);
        self.prof_add(if ov { "rc2 draw HUD" } else { "rc2 draw local memory" }, t0);
    }

    /// gsfull `GsOvl::world_layer`: a depth-tested prim the EE sends itself (the software GS draws it as an effect). The same
    /// predicate (`zlayer::is_world_layer`), also in the HUD part of the field.
    fn rc2_world_layer(&self, at: u64) -> bool {
        let c = ((at >> 9) & 1) as usize;
        // [b46 U5c] same predicate as gsfull: a dst-alpha pass is not a layer (zlayer.rs is_layer_draw)
        crate::gsfull::zlayer::is_layer_draw(self.rc2.path, at, self.r[0x47 + c], self.r[0x06 + c], self.r[0x4c + c], self.r[0x42 + c])
    }

    /// gsfull `GsOvl::overlay_skip`: what is drawn into last field's HUD buffer before the HUD is not needed, and a HUD-part draw that
    /// samples the HUD buffer itself is a screen effect over the 3D picture that the native picture does not have.
    fn rc2_skip(&self, at: u64) -> bool {
        let c = ((at >> 9) & 1) as usize;
        if let Some(fb) = self.rc2.last.filter(|_| !self.rc2.hud) {
            if (self.r[0x4c + c] & 0x1ff) as u32 * 32 == fb {
                return true;
            }
        }
        let samples_own = at & 0x10 != 0 && (self.r[0x06 + c] & 0x3fff) as u32 == (self.r[0x4c + c] & 0x1ff) as u32 * 32;
        // [b46 U3] Also before the HUD, into a frame-format buffer: a full-buffer read-back of the 3D picture (cutscene screen effects,
        // level 3 scene fg1 0184: ~900 such prims a field, each a k_serial step) only feeds the software GS's picture, never the
        // sharp HUD; it is skipped even when no HUD buffer is known yet (a scene right after a load, `seen` still None).
        samples_own && (self.rc2.hud || ((self.r[0x4c + c] >> 24) & 0x3f) <= 1)
    }

    /// One field of R&C2 (gsfull/ovl_pipe.rs, after the software GS drew it): every GIF qword goes through the capture so its local
    /// memory follows the game's; an overlay field with `sharp` (the caller's `rc2_hd_ready(rows)` from before the software GS drew it:
    /// that GS then skipped the HUD) also draws its HUD into the overlay plane, and the sharp picture comes back (RGBA8, the engine's
    /// transmission alpha). `rows`: the rows of the HUD buffer (its scissor), the scale's.
    pub fn rc2_field(&mut self, of: &OvlField, overlay: bool, rows: u32, sharp: bool) -> Option<Hd> {
        let f = &of.f;
        self.rc2.on = true;
        let rows = rows.clamp(64, 512);
        let draw_ov = overlay && sharp && self.gpu.is_some();
        (self.rc2.draw_ov, self.rc2.ovl) = (draw_ov, overlay);
        self.no_draw = !draw_ov;
        // [b46 U3] A field without a HUD draw keeps the HUD buffer of the field before it (`seen`).
        let hud_buf = self.rc2.target.or(self.rc2.seen);
        // [b47 X5] (b46 review minor 7) A field that is not an overlay field (title, menus, a load) forgets it: after a load the old
        // HUD buffer is not skipped into by the first cutscene / no-HUD fields.
        self.rc2.seen = hud_buf.filter(|_| overlay);
        self.rc2.last = if overlay { hud_buf.map(|t| t * 32) } else { None };
        self.rc2.target = None;
        if draw_ov {
            // A new overlay frame: the plane is cleared, which is also where the sharp scale is taken over (`hd_ov`).
            self.gpu_fill(gpu::CUR);
            self.cur.rows = rows;
        }
        let (last1, t0) = (last_world_seg(&f.segs), self.prof.is_some().then(std::time::Instant::now));
        for (i, seg) in f.segs.iter().enumerate() {
            (self.rc2.fx, self.rc2.path) = (is_fx(seg), seg.1);
            let end = f.segs.get(i + 1).map_or(f.qs.len(), |s| s.0 as usize);
            let lo = (seg.0 as usize).min(end);
            (self.rc2.hud, self.rc2.space) = (hud_at(of, i, seg, last1, lo), of.space_at(lo));
            for k in lo..end {
                if of.flips_at(k) && k > lo { // [b46 U4]
                    (self.rc2.hud, self.rc2.space) = (hud_at(of, i, seg, last1, k), of.space_at(k));
                }
                self.gif(f.qs[k]);
            }
        }
        self.prof_add("rc2 field feed", t0);
        if self.gpu_wait != 0 && self.fields + 1 >= self.gpu_wait {
            self.gpu_attach();
        }
        if draw_ov && self.gpu.is_some() {
            let blank = Overlay { w: self.cur.w, h: self.cur.h, rows, frozen: false, disp: false, px: Vec::new(), rgba: Vec::new(), hd: None, guard: Default::default() };
            self.gpu_publish(gpu::CUR, blank);
        }
        if self.gpu.is_some() {
            self.gpu_end_job(true);
            crate::gscap_guard::field_done();
        }
        self.fields += 1;
        self.frames += draw_ov as u64;
        if self.fields % 100 == 0 {
            self.prof_dump();
        }
        self.done.take()?.hd
    }
}
