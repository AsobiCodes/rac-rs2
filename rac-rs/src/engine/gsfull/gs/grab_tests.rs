//! The grabbed-backdrop drop in `Gs::trx_start` / `image` (grab.rs): uploads into the area never reach local memory.
use super::*;
use crate::gsfull::grab::Grab;

/// One A+D register write through the GIF.
fn ad(g: &mut Gs, addr: u64, v: u64) {
    g.gif(1u128 | 1 << 15 | 1 << 60 | 0xe << 64);
    g.gif(v as u128 | (addr as u128) << 64);
}

/// A 4 x 4 host -> local upload of 0x11223344 pixels in format `psm` (0 = CT32, 1 = CT24) at block `dbp` (TRXDIR 0, then 4 qwords).
fn upload(g: &mut Gs, dbp: u64, psm: u64) {
    ad(g, 0x50, dbp << 32 | 1 << 48 | psm << 56);
    ad(g, 0x51, 0);
    ad(g, 0x52, 4 | 4 << 32);
    ad(g, 0x53, 0);
    g.gif(4u128 | 1 << 15 | 2 << 58);
    for _ in 0..4 {
        g.gif(0x1122_3344_1122_3344_1122_3344_1122_3344);
    }
}

#[test]
fn uploads_into_the_grabbed_area_are_dropped() {
    let mut g = Gs::new(vec![0u8; VRAM_SIZE], 0);
    g.lazy = false;
    g.set_grab(Some(Grab { base: 0x1e00, blocks: 0xe00, psm: 0 }));
    upload(&mut g, 0x1e40, 0);
    upload(&mut g, 0x31d2, 0);
    assert_eq!(g.rd_px(0, 0x1e40, 1, 0, 0), 0, "inside the area, the grab's format: dropped");
    assert_eq!(g.rd_px(0, 0x31d2, 1, 0, 0), 0x1122_3344, "outside the area: kept");
    // another format into the area (a movie frame) is kept
    upload(&mut g, 0x1e80, 1);
    assert_ne!(g.rd_px(1, 0x1e80, 1, 0, 0), 0, "inside the area, another format: kept");
    // the drop ends with its transfer; no grab: the same upload lands
    g.set_grab(None);
    upload(&mut g, 0x1e40, 0);
    assert_eq!(g.rd_px(0, 0x1e40, 1, 0, 0), 0x1122_3344, "grab off: kept");
}
