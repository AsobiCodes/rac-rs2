//! The grabbed-backdrop area (game-neutral; the pack names the numbers through `ovl_pipe::FieldIn::grab`).
//!
//! Some games freeze the picture behind a pause or title menu the way a real PS2 does: the EE downloads the finished picture from the
//! GS into main memory (GS -> EE, VIF1 DIR = 1), and every field it (1) copies the area to the shown buffer with a full-screen sprite,
//! (2) clears the area (a draw buffer), (3) uploads the RAM image back into the area (IMAGE tags), then (4) draws the dimming sprite
//! and the menu INTO THE AREA; the next field's copy shows them. This GS never makes the download (the oracle does not emulate it),
//! so the RAM image holds old junk and the upload shows it as noise. Two cures, both only while the window shows the native picture
//! (`FieldIn::grab` is None otherwise, and for every game that never sets it):
//! - the GS drops the IMAGE uploads that land in the area (`Gs::trx_start`): the area keeps what the GS drew there (a black clear at
//!   the title), so the title menu is black behind its buttons;
//! - in an overlay field (the world is the native picture, the GS draws only the HUD) everything before the first upload is the
//!   previous frame's copy and clear, not HUD (`OvlField::hud_from`): the overlay is then the dimming sprite and the menu, drawn
//!   into the area, and the frozen native world shows under them.

/// The grabbed area: GS local memory blocks `base .. base + blocks` (a draw buffer; DBP units) and the pixel format (BITBLTBUF PSM) of the
/// grab's uploads: other uploads into the area (a movie's frames) are real pictures and stay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grab {
    pub base: u32,
    pub blocks: u32,
    pub psm: u32,
}

impl Grab {
    /// An upload whose BITBLTBUF DBP / PSM are `dbp` / `psm` is the grab's: it lands in the area in the grab's format.
    pub fn hit(&self, dbp: u32, psm: u32) -> bool {
        psm == self.psm && dbp >= self.base && dbp < self.base + self.blocks
    }

    /// The field's first qword (index into the field's GS qwords) of an IMAGE upload into the area, from `(dbp, psm, qword)` of every IMAGE
    /// packet of the field (`water::GifRegs::img`). None: the field does not upload into the area (no frozen screen).
    pub fn hud_from(&self, images: &[(u32, u32, u32)]) -> Option<u32> {
        images.iter().find(|i| self.hit(i.0, i.1)).map(|i| i.2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: Grab = Grab { base: 0x1e00, blocks: 0xe00, psm: 1 };

    #[test]
    fn hit_covers_exactly_the_area_in_the_grab_format() {
        assert!(!G.hit(0x1dff, 1));
        assert!(G.hit(0x1e00, 1));
        assert!(G.hit(0x1e00 + 0x40 * 55, 1));
        assert!(G.hit(0x2bff, 1));
        assert!(!G.hit(0x2c00, 1));
        assert!(!G.hit(0x31d2, 1));
        // a movie frame (another format) into the area is not the grab
        assert!(!G.hit(0x1e40, 0));
    }

    #[test]
    fn hud_from_is_the_first_upload_into_the_area() {
        assert_eq!(G.hud_from(&[(0x31d2, 0, 10), (0x31d6, 1, 20)]), None);
        assert_eq!(G.hud_from(&[]), None);
        assert_eq!(G.hud_from(&[(0x1e00, 0, 5), (0x31d2, 0, 10), (0x1e00, 1, 300), (0x1e40, 1, 1100)]), Some(300));
        assert_eq!(G.hud_from(&[(0x2bc0, 1, 7)]), Some(7));
    }
}
