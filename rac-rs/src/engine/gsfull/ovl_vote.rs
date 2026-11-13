//! Which effect's depth an overlay pixel keeps (R&C2 overlay mode, `gs_raster::overlay_px`). The overlay holds ONE depth per pixel,
//! but a pixel can be drawn by several effects at different depths: a faint glow in the room and a fire behind the wall. Keeping the
//! NEAREST depth (what this did before) let the far fire pass the window's wall test because of the near glow (fire seen through a
//! wall, R&C2 Aranos, 2026-10-31 screenshots). Now the pixel keeps the depth of the effect that contributed most to its colour:
//! a running vote over the draws of the field (Boyer-Moore: draws within ~6 % of the kept depth add to its weight, a draw at another
//! depth takes weight off it and takes the pixel over when it outweighs it). The weight lives in the second plane of the overlay
//! depth buffer (`PLANE` u32 after the depth plane). The GS never draws retail's world in overlay mode (the world programs are
//! skipped, `overlay.rs`), so there is no GS Z buffer of the world to test against [C: `VifOverlay::exec`].
//! Known trade-off of one depth per pixel (review B3/4 N2): a faint near effect over a dominant far one that is behind a wall is dropped in
//! the shared pixels (the whole pixel takes the far depth); the real fix is a second overlay layer (near / far split). Also known (N1): the
//! kept depth of one effect ratchets nearer by up to 1/16 per draw (`z.max`), so a long particle column can end at its near end's depth.

/// Element offset of the weight plane in `GsOvl::z` (the depth plane is 1024 x 1024 u32).
pub const PLANE: usize = 1024 * 1024;

/// [b46 fix round] The depth word of a pixel the flight's cosmos coloured (`Ctx::ozfar`): no GS Z, the picture's lowest layer (u32::MAX = HUD,
/// in front). The window draws these pixels first, under the whole native scene (`viewer/gpu/ov_depth.rs` backdrop).
pub const SPACE: u32 = u32::MAX - 1;
/// `SPACE` in the depth plane handed to the window (metres; no effect is that far).
pub const SPACE_M: f32 = 1.0e9;

/// How much a draw changed an overlay pixel: the summed byte differences of the packed pixel (premultiplied rgb and the see-through
/// byte), 0 = it left the pixel as it was.
pub fn contribution(old: u32, new: u32) -> u32 {
    (0..4).map(|k| ((old >> (8 * k)) & 0xff).abs_diff((new >> (8 * k)) & 0xff)).sum()
}

/// The (GS Z, weight) a pixel keeps after a draw of GS Z `nz` that contributed `c`; `z` 0 = no effect yet. GS Z grows towards the eye.
pub fn vote(z: u32, w: u32, nz: u32, c: u32) -> (u32, u32) {
    if c == 0 {
        return (z, w);
    }
    if z == 0 || w == 0 {
        return (nz, c);
    }
    // Same effect: depths within 1/16 of each other (view depth = 524288 / (Z + 480)).
    let (a, b) = (z as u64 + 480, nz as u64 + 480);
    if a.abs_diff(b) * 16 <= a.max(b) {
        (z.max(nz), w.saturating_add(c))
    } else if c > w {
        (nz, c - w)
    } else {
        (z, w - c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A near faint glow does not decide the depth of a pixel a far fire mostly colours: the fire stays behind its wall.
    #[test]
    fn dominant_effect_keeps_the_pixel() {
        let (near, far) = (2_000_000u32, 400_000u32);
        let (mut z, mut w) = (0, 0);
        (z, w) = vote(z, w, far, 300);
        (z, w) = vote(z, w, near, 20);
        assert_eq!(z, far);
        (z, w) = vote(z, w, far, 100);
        assert_eq!((z, w), (far, 380));
    }

    /// The order of draws does not matter for a clear winner.
    #[test]
    fn order_free_for_a_clear_winner() {
        let (near, far) = (2_000_000u32, 400_000u32);
        let (z, w) = [(near, 20), (far, 300), (near, 20)].iter().fold((0, 0), |(z, w), &(nz, c)| vote(z, w, nz, c));
        assert_eq!(z, far);
        assert!(w > 0);
    }

    /// Draws at about one depth add up, and the nearest of them is kept.
    #[test]
    fn same_depth_accumulates() {
        let (z, w) = vote(1_000_000, 50, 1_020_000, 70);
        assert_eq!((z, w), (1_020_000, 120));
        // A draw that changed nothing leaves the vote alone.
        assert_eq!(vote(z, w, 5, 0), (z, w));
    }

    #[test]
    fn contribution_counts_every_byte() {
        assert_eq!(contribution(0xff00_0000, 0xff00_0000), 0);
        assert_eq!(contribution(0xff00_0000, 0xf010_0305), 15 + 16 + 3 + 5);
    }
}
