//! [b46 U5] The resident part of a mip-only GS texture. A TEX0 at TBP0 0 whose mip registers name level bases is a texture whose level 0
//! is not in VRAM: the game's VRAM ring keeps only the levels a far class samples and leaves TBP0 at 0 until it streams level 0 in
//! [C: R&C2 level 14 helper robots, RAM classes 0x1598c80 / 0x1680980 TEX0 8005a645dd308000 with TBP0 0 over fields 2..215, while the
//! streamed copy of the same texture draws as 8005a645dd30b56e (b46 handoff_U5 fact 3)]. Its level 0 "texels" are whatever sits at
//! block 0 (the frame buffer): decoded they are noise, drawn see-through the object vanishes when the camera is close.
//! Such a word is also not one texture: every class the ring parks in that slot shares the TEX0 word with its own MIPTBP1 / MIPTBP2
//! [C: gs3 primlog frame 99/100, 8005a645dd308000 drawn with three different MIPTBP1, 80059d45dd308000 with five].
//! `resident` names the texture by its first resident level instead: that level's own TEX0 word (TBP = its MIPTBP base, so classes with
//! other levels get other words) with the mip registers of the levels after it (MXL and TEX1 K shifted by the levels dropped, so the GS
//! level choice picks the same VRAM level for every LOD past the first resident one and the first resident level where the GS would
//! pick a missing finer one). Pure GS register maths: no game is named.
use super::texreq_mips::{level_tex0, mxl, Regs};

/// TEX1 MTBA (levels packed after level 0 at TBP0: nothing resident apart from level 0's own base).
const MTBA: u64 = 0x200;

/// The first resident level of a mip-only `tex0` (TBP0 0, mipmapping TEX1 `r[0]` without MTBA, a level 1..MXL whose MIPTBP base is not
/// 0) as (its TEX0 word, the mip registers of the resident levels after it: MXL = the count of following levels with a base, K moved
/// down by 16 (one level, 1/16 units) per level dropped, MIPTBP1 / MIPTBP2 = those levels' bases and widths). None for any other TEX0.
/// Levels past MXL 3 count only with a MIPTBP2 (as `texreq_mips::level_words`); a level with base 0 after the first resident one ends
/// the chain (the GS would read block 0 there).
pub fn resident(tex0: u64, r: &Regs) -> Option<(u64, Regs)> {
    let mmin = (r[0] >> 6) & 7;
    if tex0 & 0x3fff != 0 || r[0] & MTBA != 0 || mxl(r[0]) == 0 || !(2..=5).contains(&mmin) {
        return None;
    }
    let top = if r[2] == 0 { mxl(r[0]).min(3) } else { mxl(r[0]) };
    let base = |n: u32| level_tex0(tex0, r, n) & 0x3fff;
    let n = (1..=top).find(|&n| base(n) != 0)?;
    // Each following level as a MIPTBP entry: TBP (14 bits) and TBW (6 bits).
    // [b47 X5] (b46 review minor 4) a fixed array, no allocation per draw word (at most 5 levels follow level 1).
    let mut buf = [0u64; 6];
    let mut len = 0;
    for k in (n + 1..=top).take_while(|&k| base(k) != 0) {
        buf[len] = level_tex0(tex0, r, k) & 0xf_ffff;
        len += 1;
    }
    let rest = &buf[..len];
    let pack = |ws: &[u64]| ws.iter().enumerate().fold(0u64, |a, (i, &w)| a | w << (20 * i));
    let (m1, m2) = (pack(&rest[..rest.len().min(3)]), pack(&rest[rest.len().min(3)..]));
    let k = ((r[0] >> 32) & 0xfff) as i32;
    let k = if k >= 0x800 { k - 0x1000 } else { k };
    let k = (k - 16 * n as i32).clamp(-0x800, 0x7ff) as u64 & 0xfff;
    let tex1 = r[0] & !(0x1c | 0xfffu64 << 32) | (rest.len() as u64) << 2 | k << 32;
    Some((level_tex0(tex0, r, n), [tex1, m1, m2]))
}

/// The word a draw of `tex0` with level bases `bases` (its AD block's MIPTBP1 / MIPTBP2, overlay.rs `ad_bases`) is known by and the
/// bases of that word's own levels: the first resident level (`resident`, with the registered TEX1 `tex1`) for a mip-only word, the
/// word itself with its bases otherwise.
pub fn draw_word(tex0: u64, tex1: Option<u64>, bases: Option<[u64; 2]>) -> (u64, Option<[u64; 2]>) {
    match (tex1, bases) {
        (Some(t1), Some(b)) if tex0 & 0x3fff == 0 => match resident(tex0, &[t1, b[0], b[1]]) {
            Some((w, r)) => (w, Some([r[1], r[2]])),
            None => (tex0, bases),
        },
        _ => (tex0, bases),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TEX1 of the level 14 robots and the Aranos Thug: MXL 2, MMIN 4, K 0xf92 (-6.875).
    const TEX1: u64 = 0xff92_0000_0128;
    /// MIPTBP1 with levels 1..3 at these bases (TBW 1).
    fn mip1(b: [u64; 3]) -> u64 {
        b.iter().enumerate().fold(0, |a, (i, &x)| a | (x | 1 << 14) << (20 * i))
    }

    /// Classes sharing one mip-only TEX0 word with their own level bases are different textures; each is named by its level 1 word.
    #[test]
    fn classes_sharing_a_mip_only_word_get_their_own_identity() {
        let t = 0x8005_a645_dd30_8000u64; // the robots' word: TBP0 0
        let (a, ra) = resident(t, &[TEX1, mip1([0x33a6, 0x2d36, 0]), 0]).unwrap();
        let (b, _) = resident(t, &[TEX1, mip1([0x33ca, 0x30c7, 0]), 0]).unwrap();
        assert_ne!(a, b, "other level bases: another texture");
        assert_eq!(a & 0x3fff, 0x33a6, "named by level 1");
        assert_eq!(a, level_tex0(t, &[TEX1, mip1([0x33a6, 0x2d36, 0]), 0], 1));
        // Its own levels: level 2 of the class as its level 1, K one level down (-6.875 - 1 = -7.875 = 0xf82), MXL 1.
        assert_eq!(mxl(ra[0]), 1);
        assert_eq!((ra[0] >> 32) & 0xfff, 0xf82);
        assert_eq!(level_tex0(a, &ra, 1), level_tex0(t, &[TEX1, mip1([0x33a6, 0x2d36, 0]), 0], 2), "the same VRAM level");
        assert_eq!(ra[0] & !(0x1c | 0xfffu64 << 32), TEX1 & !(0x1c | 0xfffu64 << 32), "MMIN, L, MMAG unchanged");
    }

    /// The first resident level is the first one with a base: the Thug's level 1 base is 0 (block 0, the frame buffer), so level 2 is
    /// drawn for every LOD and no chain is left (MXL 0).
    #[test]
    fn the_first_resident_level_skips_level_bases_at_0() {
        let t = 0x8006_0265_dd30_8000u64;
        let r = [TEX1, 0x6c_1707_0240_4000, 0];
        let (w, rw) = resident(t, &r).unwrap();
        assert_eq!(w & 0x3fff, 0x3024);
        assert_eq!(((w >> 26) & 0xf, (w >> 30) & 0xf), (((t >> 26) & 0xf) - 2, ((t >> 30) & 0xf) - 2), "a quarter the size");
        assert_eq!(mxl(rw[0]), 0);
        assert_eq!((rw[1], rw[2]), (0, 0));
        // A base 0 after the first resident level ends the chain.
        let (_, rx) = resident(t, &[TEX1 | 1 << 2, mip1([0x3000, 0, 0x3100]), 0]).unwrap();
        assert_eq!(mxl(rx[0]), 0);
    }

    /// Only mip-only words: a set TBP0, MTBA, no mipmapping or no level base at all keep the word.
    #[test]
    fn other_words_are_not_mip_only() {
        let t = 0x8005_a645_dd30_8000u64;
        let r = [TEX1, mip1([0x33a6, 0x2d36, 0]), 0];
        assert!(resident(t | 0x356e, &r).is_none(), "level 0 streamed in (TBP0 0x356e): the real texture");
        assert!(resident(t, &[TEX1 | MTBA, r[1], 0]).is_none());
        assert!(resident(t, &[TEX1 & !0x1c, r[1], 0]).is_none(), "MXL 0");
        assert!(resident(t, &[TEX1 & !(7 << 6) | 1 << 6, r[1], 0]).is_none(), "MMIN LINEAR");
        assert!(resident(t, &[TEX1, mip1([0, 0, 0]), 0]).is_none(), "no level in VRAM");
        assert_eq!(draw_word(t | 0x356e, Some(TEX1), Some([r[1], 0])), (t | 0x356e, Some([r[1], 0])));
        assert_eq!(draw_word(t, None, Some([r[1], 0])), (t, Some([r[1], 0])), "no registered TEX1");
        let (a, _) = resident(t, &r).unwrap();
        assert_eq!(draw_word(t, Some(TEX1), Some([r[1], 0])).0, a, "a draw's AD bases name the same word as the class's RAM block");
    }

    /// MIPTBP2 levels 4..6 follow MIPTBP1's when it is set; K stays a signed 12-bit value.
    #[test]
    fn levels_past_3_and_k_wrap() {
        let t = 0x8005_a645_dd30_8000u64;
        let tex1 = 0x0010_0000_0000_0000u64 | 0x7f0 << 32 | 6 << 2 | 4 << 6; // K +127/16, MXL 6
        let r = [tex1 & !(0xfff << 32) | 0x810 << 32, mip1([0, 0x3001, 0x3002]), mip1([0x3003, 0x3004, 0x3005])];
        let (w, rw) = resident(t, &r).unwrap();
        assert_eq!(w & 0x3fff, 0x3001);
        assert_eq!(mxl(rw[0]), 4);
        assert_eq!(level_tex0(w, &rw, 4) & 0x3fff, 0x3005);
        assert_eq!((rw[0] >> 32) & 0xfff, 0x800, "-2032 - 32 clamps at -2048");
    }
}
