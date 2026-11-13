//! The sharp HUD's Z test (R&C2 round 3 batch 29, the user's "weapon models are see-through" at the vendor): the menu's 3D model
//! (MENU_PROG 11 prims of a frozen-screen menu) is drawn by the GS with ZTE / ZTST GEQUAL into the HUD buffer, so its far faces lose
//! to the near ones. The sharp HUD (gpu.wgsl `k_hd`) had no Z buffer: every face drew in send order and the inner pods showed
//! through the body. Each overlay prim's record now carries its Z mode (record word 27, `word`; P1 mode keeps words 27..32 for
//! its VRAM Z buffer, `R_ZB + 5` set), and `k_hd` keeps a depth per sharp pixel (the plane after the twin, `HZ`), cleared with the
//! overlay frame (k_hov fill of CUR), tested and written like the GS: ZTE off counts as always, ZMSK keeps the depth.
//! Only for a pack with `Pack::hud_z` (R&C2): otherwise the word stays 0 and `k_hd` draws in send order as before (R&C1 unchanged).

/// The record word: 0 = not an overlay prim (no sharp Z), else 1 | ZTST << 1 | ZMSK << 3 | PSMZ & 3 << 4 (0 Z32, 1 Z24, 2 Z16 / Z16S).
pub(super) fn word(zbuf: u64, test: u64) -> u32 {
    let tst = if test & 0x10000 != 0 { ((test >> 17) & 3) as u32 } else { 1 };
    1 | tst << 1 | (((zbuf >> 32) & 1) as u32) << 3 | (((zbuf >> 24) & 3) as u32) << 4
}

#[cfg(test)]
mod tests {
    use super::word;

    #[test]
    fn the_word_carries_ztst_zmsk_and_the_z_format() {
        // ZTE on, ZTST GEQUAL (2), Z24 (PSMZ 0x31 -> low bits 1), no mask
        assert_eq!(word(0x31 << 24, 0x10000 | 2 << 17), 1 | 2 << 1 | 1 << 4);
        // ZTE off counts as always (1), ZMSK set
        assert_eq!(word(1 << 32, 3 << 17), 1 | 1 << 1 | 1 << 3);
        // GREATER, Z16S (0x3a -> 2)
        assert_eq!(word(0x3a << 24, 0x10000 | 3 << 17), 1 | 3 << 1 | 2 << 4);
        assert_ne!(word(0, 0), 0, "an overlay prim always has a word (0 = no sharp Z)");
    }
}
