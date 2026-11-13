//! The depth-tested "world layer" test of the overlay routing, shared by the software GS (`gs_overlay.rs` `GsOvl::world_layer`) and the
//! GPU capture (`gscap/rc2.rs` `rc2_world_layer`): the two must never disagree about which prims are layers. Game-neutral: it only
//! reads GS registers.
//!
//! A world layer is a draw of the 3D part that the EE sends itself (VIF DIRECT or GIF, not a VU1 program's output: `path != 1`), tests
//! depth against the world drawn before it (TEST.ZTE = 1, ZTST GEQUAL / GREATER) and draws a colour of its own. The native picture
//! does not draw it, so the overlay does, with the depth of its pixels kept (`Ctx::ozfx`): the window then hides what a wall covers.
//! Whether the segment counts as HUD (`overlay::is_hud`: everything after the last non-effect PATH1 segment) does NOT matter: the EE
//! sends its lamp glows (R&C2 Aranos: ALPHA 0x8000000048, TEST 0x5380b) after the last PATH1 segment, so they sit in the "HUD" part,
//! and with depth 0 ("always in front") they showed through walls [C: lane glow, RAC_GSPIX 372,299 field 2500, census RAC2_DRAWLOG].
//! Genuine HUD draws use TEST 0x30000 (ZTE = 1, ZTST ALWAYS), which this test rejects.

/// GS TEST_1/2 bit ZTE.
const TEST_ZTE: u64 = 1 << 16;

/// Is the prim being drawn a depth-tested world layer? `path`: the GIF path of the segment; `at`: the prim attributes (bit 4 TME, bit
/// 9 context); `test`, `tex0`, `frame`: TEST, TEX0 and FRAME of the prim's context. Not a layer: PATH1 (a VU1 program's own draw);
/// no Z test or a Z test that always passes (HUD); an alpha test that never passes and writes no colour for the failing pixels
/// (ATST NEVER with AFAIL KEEP / ZB_ONLY); a draw that reads the frame it draws into (the shadow passes: TEX0 TBP = FBP, they came out
/// as black blobs); RGB masked off (FBMSK). ATST NEVER with AFAIL FB_ONLY / RGB_ONLY (TEST 0x53001: Z-tested glows of another level,
/// colour only, no Z write [C: census over the whole recording, overlay field 4023]) does draw its colour, so it is a layer.
pub fn is_world_layer(path: u8, at: u64, test: u64, tex0: u64, frame: u64) -> bool {
    let own_frame = at & 0x10 != 0 && (tex0 & 0x3fff) == (frame & 0x1ff) * 32;
    let draws_nothing = test & 1 != 0 && (test >> 1) & 7 == 0 && (test >> 12) & 1 == 0;
    path != 1 && test & TEST_ZTE != 0 && (test >> 17) & 3 >= 2 && !draws_nothing && !own_frame && (frame >> 32) & 0xff_ffff != 0xff_ffff
}

/// [b46 U5c] The routing test both call sites use: `is_world_layer` and not a dst-alpha pass (`is_dst_alpha_pass`, `alpha` = the prim
/// context's ALPHA). A Z-tested dst-alpha pass is the thruster heat haze of R&C2's flying glass platforms: TEX0 0x6202133c0 (the
/// frame's 256 x 256 PSMCT16 down-copy at TBP 0x33c0), ALPHA 0x8000000054, TEST 0x53001 (ZTE, GEQUAL) [C: b46 u5 run plat_pix, state
/// u75237 field 50, RAC_GSPIX 245,330 + RAC2_OVLKIND: "ovlkind layer ... alpha 0x8000000054 tex0 0x6202133c0", overlay pixel
/// 0x7f000000 -> 0x7f11110d with texel 0]. As a layer it was drawn into the overlay plane, which does not hold the frame copy, so the
/// texel was 0 and the plane got an opaque near-black box; the haze only bends the picture behind it, so not drawing it is retail's
/// look minus the shimmer. Without layer status the HUD part skips it (`overlay_skip`) and the 3D part never draws into the overlay.
pub fn is_layer_draw(path: u8, at: u64, test: u64, tex0: u64, frame: u64, alpha: u64) -> bool {
    is_world_layer(path, at, test, tex0, frame) && !is_dst_alpha_pass(at, alpha)
}

/// A blend that only makes sense against a frame buffer whose alpha holds the picture's coverage: ALPHA `(A - B) * C >> 7 + D` with the
/// result Cd (A == B, D = Cd: a pass that only writes the destination alpha, the "mask" half of the hover-board thruster flames) or with
/// C = Ad ((Cs - Cd) * Ad + Cd, ALPHA 0x54: the "colour" half, which samples the frame's 256 x 256 down-copy). The overlay plane's alpha
/// is the see-through flag of a plane that is empty where the HUD draws nothing, so such a draw gave a black box on the native picture
/// [C: aranos 0200 level 4 board, F1 RAC_GSPIX 300,428: pair of TEX0 0x800640a599307209 ALPHA 0x4a + TEX0 0x620213220 ALPHA 0x54,
/// the second reads the previous field's frame copy, which the overlay GS does not hold]. ABE off: not a blend. R&C2 overlay only.
pub fn is_dst_alpha_pass(at: u64, alpha: u64) -> bool {
    if at & 0x40 == 0 {
        return false;
    }
    let (a, b, c, d) = (alpha & 3, (alpha >> 2) & 3, (alpha >> 4) & 3, (alpha >> 6) & 3);
    (a == b && d == 1) || (c == 1 && a == 0 && b == 1 && d == 1)
}

/// [b50 V5] An untextured SUBTRACTIVE blend in the HUD part, ALPHA `(0 - Cs) * C >> 7 + Cd` (A = zero, B = Cs, D = Cd): a screen
/// filter that darkens the 3D picture under it. R&C2's Visibomb missile view washes the frame green with two full-screen sprites,
/// colour [80, 0, 80, 128] and [255, 255, 255, 15], ALPHA 0x1f00000042, TEST 0x33001, into the HUD buffer [C: b50 v5, crafted state
/// b48\y1\st\visi, RAC_GSPIX 120,300 fields 123 vs 150; RAC2_HUDLOG: every overlay pixel opaque from field 125]. The overlay plane
/// does not hold the 3D picture (it is empty where the HUD draws nothing), so the subtraction left opaque black over the whole native
/// view. Not drawing it is retail's view minus the green wash and the scanlines. Textured subtractive draws (HUD art) stay.
/// R&C2 overlay only.
pub fn is_frame_filter(at: u64, alpha: u64) -> bool {
    if at & 0x40 == 0 || at & 0x10 != 0 {
        return false; // ABE off: not a blend; TME on: a textured HUD draw
    }
    let (a, b, d) = (alpha & 3, (alpha >> 2) & 3, (alpha >> 6) & 3);
    a == 2 && b == 0 && d == 1
}

#[cfg(test)]
mod tests {
    use super::is_world_layer;

    const AT_TME: u64 = 0x10;
    /// FRAME with FBP 0x3c (blocks of 32 words = 0x780), PSM 0, FBMSK 0.
    const FRAME: u64 = 0x3c;

    #[test]
    fn lamp_glow_is_a_layer_also_in_the_hud_part() {
        // Aranos glow: TEST 0x5380b (ATE, ATST GEQUAL, ZTE, ZTST GEQUAL), sent by PATH2/3, textured from another buffer.
        for path in [0u8, 2, 3] {
            assert!(is_world_layer(path, AT_TME, 0x5380b, 0x3000, FRAME), "path {path}");
        }
    }

    #[test]
    fn level6_tower_windows_are_layers() {
        assert!(is_world_layer(2, AT_TME, 0x5360b, 0x6a5, FRAME));
    }

    #[test]
    fn hud_draws_are_not_layers() {
        // HUD draws: TEST 0x30000 = ZTE 1, ZTST ALWAYS.
        assert!(!is_world_layer(2, AT_TME, 0x30000, 0x3000, FRAME));
        // No depth test at all.
        assert!(!is_world_layer(2, AT_TME, 0x0, 0x3000, FRAME));
        assert!(!is_world_layer(2, AT_TME, 0x5380b & !0x10000, 0x3000, FRAME));
        // ZTST NEVER (0) and ALWAYS (1) are below GEQUAL (2).
        assert!(!is_world_layer(2, AT_TME, 0x10000, 0x3000, FRAME));
        assert!(!is_world_layer(2, AT_TME, 0x30000, 0x3000, FRAME));
    }

    #[test]
    fn vu1_programs_shadow_passes_and_masked_draws_are_not_layers() {
        // PATH1 is a VU1 program's draw (an effect, not a layer).
        assert!(!is_world_layer(1, AT_TME, 0x5380b, 0x3000, FRAME));
        // Alpha test NEVER with AFAIL KEEP (0x50001) / ZB_ONLY (0x52001): the prim draws no colour.
        assert!(!is_world_layer(2, AT_TME, 0x50001, 0x3000, FRAME));
        assert!(!is_world_layer(2, AT_TME, 0x52001, 0x3000, FRAME));
        // AFAIL RGB_ONLY (0x53001) / FB_ONLY (0x51001) still draws its colour: a layer, unless RGB is masked (the shadow passes
        // of Aranos: FBMSK 0xffffff) or it samples its own frame.
        assert!(is_world_layer(2, AT_TME, 0x53001, 0x3000, FRAME));
        assert!(!is_world_layer(2, AT_TME, 0x51001, 0x3000, FRAME | (0xff_ffffu64 << 32)));
        assert!(!is_world_layer(2, AT_TME, 0x51001, 0x3c * 32, FRAME));
        // Samples the frame it draws into (TEX0 TBP == FBP * 32 words), with texturing on.
        assert!(!is_world_layer(2, AT_TME, 0x5380b, 0x3c * 32, FRAME));
        // ... but the same TBP without texturing is a plain coloured draw.
        assert!(is_world_layer(2, 0, 0x5380b, 0x3c * 32, FRAME));
        // RGB masked off.
        assert!(!is_world_layer(2, AT_TME, 0x5380b, 0x3000, FRAME | (0xff_ffffu64 << 32)));
        // Only alpha masked: still a layer.
        assert!(is_world_layer(2, AT_TME, 0x5380b, 0x3000, FRAME | (0xff00_0000u64 << 32)));
    }

    #[test]
    fn dst_alpha_passes_are_told_from_plain_blends() {
        use super::is_dst_alpha_pass;
        assert!(is_dst_alpha_pass(0x40, 0x4a), "the alpha mask pass (result Cd)");
        assert!(is_dst_alpha_pass(0x40, 0x54), "(Cs - Cd) * Ad + Cd");
        assert!(!is_dst_alpha_pass(0x40, 0x44), "source over");
        assert!(!is_dst_alpha_pass(0x40, 0x48), "additive");
        assert!(!is_dst_alpha_pass(0x00, 0x4a), "ABE off");
        // [b50 V5] the Visibomb view's green wash: untextured, subtractive
        use super::is_frame_filter;
        assert!(is_frame_filter(0x146, 0x1f_0000_0042), "Cd - Cs * As, untextured");
        assert!(!is_frame_filter(0x106, 0x1f_0000_0042), "ABE off");
        assert!(!is_frame_filter(0x156, 0x1f_0000_0042), "textured: HUD art");
        assert!(!is_frame_filter(0x146, 0x44), "source over");
        assert!(!is_frame_filter(0x146, 0x48), "additive");
    }

    // [b46 U5c] The glass platform's heat haze (TEST 0x53001, ALPHA 0x54 / mask 0x4a, AT 0x7c) is a world layer by its depth test but
    // not a layer draw; the layers that work today (glows ALPHA 0x48 / 0x44 / 0x88 / 0x64, tower windows) stay layer draws.
    #[test]
    fn depth_tested_dst_alpha_passes_are_not_layer_draws() {
        use super::is_layer_draw;
        const AT: u64 = 0x7c; // TME, ABE, ...
        assert!(is_world_layer(2, AT, 0x53001, 0x6202133c0, FRAME));
        assert!(!is_layer_draw(2, AT, 0x53001, 0x6202133c0, FRAME, 0x8000000054), "frame-copy colour pass");
        assert!(!is_layer_draw(2, AT, 0x53001, 0x8006728599307398, FRAME, 0x4a), "alpha mask pass");
        for alpha in [0x8000000048u64, 0x8000000044, 0x8000000088, 0x2000000064] {
            assert!(is_layer_draw(2, AT, 0x5380b, 0x3000, FRAME, alpha), "alpha {alpha:#x}");
        }
        assert!(is_layer_draw(2, AT_TME, 0x5360b, 0x6a5, FRAME, 0x44));
        // Without ABE the ALPHA register does not matter.
        assert!(is_layer_draw(2, AT_TME, 0x53001, 0x3000, FRAME, 0x54));
    }
}
