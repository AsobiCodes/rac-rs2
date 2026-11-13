//! R&C2 round 3 batch 34: the mip levels of the textures texreq reads. R&C2 streams moby textures through a VRAM ring and keeps
//! only the levels a class needs: the level 0 blocks of a far class are reused by other textures while the GS samples its
//! levels 1..MXL at MIPTBP1 / MIPTBP2 (TEX1 LOD) [C: crate class 0x1f4 at Oozla field 7900, TEX0 ..b29b level 0 = foreign texels in
//! F1 and native, F1 picture = wood]. The pack names a TEX0's TEX1 / MIPTBP1 / MIPTBP2 (`set`); every read of that TEX0 then also
//! reads its levels (`read_chain`) and the renderer samples them with the GS level choice.
use crate::level::Texture;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// The GS mip registers of a TEX0: TEX1, MIPTBP1, MIPTBP2.
pub type Regs = [u64; 3];

static REGS: Mutex<Option<HashMap<u64, Regs>>> = Mutex::new(None);
/// [b47 X5] (b46 review minor 4) One bit per registered TEX0 (`mbit`), set by `set`, cleared by `clear`: `maybe_regs` tells a word
/// that was never registered without the lock.
static RMASK: AtomicU64 = AtomicU64::new(0);

fn mbit(tex0: u64) -> u64 {
    1 << ((tex0 >> 14 ^ tex0 >> 37) & 63)
}

/// Can `tex0` have registered mip registers (`regs`)? False = surely not (no lock taken).
pub fn maybe_regs(tex0: u64) -> bool {
    RMASK.load(Ordering::Relaxed) & mbit(tex0) != 0
}
/// The levels 1..MXL read with the newest read of a TEX0 (empty: none).
static CHAINS: Mutex<Option<HashMap<u64, Arc<Vec<Texture>>>>> = Mutex::new(None);

/// TEX1 MXL (levels above 0, at most 6).
pub fn mxl(tex1: u64) -> u32 {
    (((tex1 >> 2) & 7) as u32).min(6)
}

/// The pack's mip registers of `tex0` (a TEX1 with MXL 0 or MMIN without mipmapping forgets them). Returns true when they changed.
pub fn set(tex0: u64, r: Regs) -> bool {
    let use_mips = mxl(r[0]) > 0 && (2..=5).contains(&((r[0] >> 6) & 7));
    let Ok(mut g) = REGS.lock() else { return false };
    let m = g.get_or_insert_with(HashMap::new);
    if use_mips {
        RMASK.fetch_or(mbit(tex0), Ordering::Relaxed);
        m.insert(tex0, r) != Some(r)
    } else {
        m.remove(&tex0).is_some()
    }
}

/// The level bases (MIPTBP1, MIPTBP2) a draw of `tex0` really uses (its AD block in VU1 memory at the draw, overlay.rs `ad_bases`)
/// replace the registered ones; TEX1 stays the pack's. The pack registers from the class's RAM block at decode time, and one TEX0
/// word is shared by every class the VRAM ring puts in that slot, so the registered bases can be another time's or another
/// class's [C: Oozla field 7900, crate TEX0 ..b29b registered with MIPTBP1 0x32db (level 1 packed after level 0) while the draw
/// and RAM use 0x32ac]. Nothing for a TEX0 without registered mips. True when the bases changed.
pub fn set_bases(tex0: u64, b: [u64; 2]) -> bool {
    let Ok(mut g) = REGS.lock() else { return false };
    let Some(r) = g.as_mut().and_then(|m| m.get_mut(&tex0)) else { return false };
    if [r[1], r[2]] == b {
        return false;
    }
    (r[1], r[2]) = (b[0], b[1]);
    true
}

/// The registered mip registers of `tex0`.
pub fn regs(tex0: u64) -> Option<Regs> {
    REGS.lock().ok()?.as_ref()?.get(&tex0).copied()
}

/// A new level: every registered TEX0 and chain is forgotten.
pub fn clear() {
    if let Ok(mut g) = REGS.lock() {
        *g = None;
        RMASK.store(0, Ordering::Relaxed);
    }
    if let Ok(mut g) = CHAINS.lock() {
        *g = None;
    }
}

/// The TEX0 word of level `n` (1..=6) of `tex0`: the level's base and width (MIPTBP1 / MIPTBP2, or with TEX1 MTBA the levels packed
/// after level 0 as the GS derives them, gs_raster.rs `level_tbp`), the size halved `n` times (at least 1), the same format and CLUT.
pub fn level_tex0(tex0: u64, r: &Regs, n: u32) -> u64 {
    let f = |sh: u32, bits: u32| ((tex0 >> sh) & ((1 << bits) - 1)) as u32;
    let (tbp, tbw, psm, tw, th) = (f(0, 14), f(14, 6), f(20, 6), f(26, 4).min(10), f(30, 4).min(10));
    let (bp, bw) = if r[0] & 0x200 != 0 {
        let bpp = match psm {
            0x00 | 0x1b | 0x24 | 0x2c => 32,
            0x01 => 24,
            0x02 | 0x0a => 16,
            0x13 => 8,
            _ => 4,
        };
        let (mut bp, mut bw) = (tbp, tbw);
        for k in 0..n {
            let (w, h) = ((1u32 << tw >> k).max(1), (1u32 << th >> k).max(1));
            bp += (w * h * bpp / 8).div_ceil(256);
            bw = (bw / 2).max(1);
        }
        (bp, bw)
    } else {
        let (m, k) = if n <= 3 { (r[1], n - 1) } else { (r[2], n - 4) };
        let w = m >> (20 * k);
        ((w & 0x3fff) as u32, ((w >> 14) & 0x3f) as u32)
    };
    let (lw, lh) = (tw.saturating_sub(n) as u64, th.saturating_sub(n) as u64);
    tex0 & !(0x3fff | 0x3f << 14 | 0xf << 26 | 0xfu64 << 30) | bp as u64 | (bw as u64) << 14 | lw << 26 | lh << 30
}

/// Hash input of the levels of `tex0` (their TEX0 words: texreq hashes each one's VRAM spans), empty without registers.
/// Levels whose base register is missing (MIPTBP2 0 without MTBA: the game sets none for levels 4..6) end the chain.
pub fn level_words(tex0: u64) -> Vec<u64> {
    let Some(r) = regs(tex0) else { return Vec::new() };
    let top = if r[0] & 0x200 == 0 && r[2] == 0 { mxl(r[0]).min(3) } else { mxl(r[0]) };
    (1..=top).map(|n| level_tex0(tex0, &r, n)).collect()
}

/// Reads the levels 1..MXL of `tex0` (none registered: nothing) and keeps them as its chain; true when they differ from the kept ones.
/// SAFETY: `m` points at VRAM_SIZE bytes.
pub(crate) unsafe fn read_chain(m: *const u8, tex0: u64) -> bool {
    let words = level_words(tex0);
    if std::env::var_os("RAC2_TEXLOG").is_some() {
        eprintln!("texreq_mips: read {tex0:016x} regs {:x?} levels {words:016x?}", regs(tex0));
    }
    // SAFETY: as this fn's.
    let levels: Vec<Texture> = words.iter().map(|&t| unsafe { super::texreq::read(m, t) }).collect();
    let Ok(mut g) = CHAINS.lock() else { return false };
    let c = g.get_or_insert_with(HashMap::new);
    let same = c.get(&tex0).is_some_and(|o| o.len() == levels.len() && o.iter().zip(&levels).all(|(a, b)| a.w == b.w && a.h == b.h && a.rgba == b.rgba));
    if levels.is_empty() {
        return c.remove(&tex0).is_some();
    }
    if !same {
        c.insert(tex0, Arc::new(levels));
    }
    !same
}

/// The levels 1..MXL read with the newest read of `tex0` (None: no mips).
pub fn chain(tex0: u64) -> Option<Arc<Vec<Texture>>> {
    CHAINS.lock().ok()?.as_ref()?.get(&tex0).cloned()
}

/// TEX1 K as raw signed 12 bits (1/16 level) and L (LOD = log2(1/|Q|) << L + K).
pub fn k_l(tex1: u64) -> (u32, u32) {
    (((tex1 >> 32) & 0xfff) as u32, ((tex1 >> 19) & 3) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Level words: MIPTBP1 holds levels 1..3 (TBP 14 bits, TBW 6 bits, 20 bits each), MIPTBP2 levels 4..6; the size halves per
    /// level; format, CLUT and the other TEX0 fields stay.
    #[test]
    fn level_tex0_takes_miptbp_and_halves_the_size() {
        let t = 0x8005_cc65_dd30_b29bu64; // TBP 0x329b TBW 2 PSMT8 128x128
        let r = [0x0000_0000_0000_0108u64, 0x3200 | 1 << 14 | (0x3210u64 | 1 << 14) << 20 | 0x3214u64 << 40, 0x3216];
        let l1 = level_tex0(t, &r, 1);
        assert_eq!(l1 & 0x3fff, 0x3200);
        assert_eq!((l1 >> 14) & 0x3f, 1);
        assert_eq!(((l1 >> 26) & 0xf, (l1 >> 30) & 0xf), (6, 6), "64x64");
        assert_eq!(l1 >> 34, t >> 34, "TCC, CLUT, CLD unchanged");
        assert_eq!((l1 >> 20) & 0x3f, 0x13);
        assert_eq!(level_tex0(t, &r, 2) & 0x3fff, 0x3210);
        assert_eq!(level_tex0(t, &r, 3) & 0x3fff, 0x3214);
        assert_eq!(level_tex0(t, &r, 4) & 0x3fff, 0x3216);
    }

    /// TEX1 MTBA: the levels follow level 0 in VRAM (block counts of each level, width halved), as gs_raster.rs `level_tbp`.
    #[test]
    fn level_tex0_mtba_packs_after_level_0() {
        let t = 0x8005_cc65_dd30_b29bu64;
        let r = [0x200 | 3 << 2 | 4 << 6, 0, 0];
        assert_eq!(level_tex0(t, &r, 1) & 0x3fff, 0x329b + 64, "128x128 PSMT8 = 64 blocks");
        assert_eq!(level_tex0(t, &r, 2) & 0x3fff, 0x329b + 64 + 16);
        assert_eq!((level_tex0(t, &r, 1) >> 14) & 0x3f, 1);
    }

    /// Only a mipmapping TEX1 (MXL > 0, MMIN 2..5) is kept.
    #[test]
    fn set_bases_replaces_only_the_bases_of_a_registered_tex0() {
        // Words no other test uses (the registry is process-wide).
        let (t, u) = (0x8005_cc65_dd30_b29bu64, 0x8005_cc65_dd30_b222u64);
        assert!(set(t, [0xff87_0000_0128, 0x6c_1706_e730_72db, 0]));
        assert!(set_bases(t, [0x6c_1706_e730_72ac, 0]), "moved level 1");
        assert_eq!(regs(t), Some([0xff87_0000_0128, 0x6c_1706_e730_72ac, 0]));
        assert!(!set_bases(t, [0x6c_1706_e730_72ac, 0]), "same bases: no change");
        assert_eq!(level_words(t)[0] & 0x3fff, 0x32ac, "level 1 read at the draw's base");
        assert!(!set_bases(u, [1, 2]) && regs(u).is_none(), "no registered mips: nothing");
    }

    #[test]
    fn set_keeps_only_mipmapping_tex1() {
        let t = 0x1234_5678u64;
        assert!(set(t, [3 << 2 | 4 << 6, 1, 2]));
        assert!(regs(t).is_some());
        assert!(!set(t, [3 << 2 | 4 << 6, 1, 2]), "same registers: no change");
        assert!(set(t, [4 << 6, 1, 2]), "MXL 0 forgets them");
        assert!(regs(t).is_none());
        assert!(!set(t, [3 << 2 | 1 << 6, 1, 2]), "MMIN LINEAR: no mipmapping");
    }
}
