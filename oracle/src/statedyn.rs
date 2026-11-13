//! dyn.bin of a quick save (R&C2 round 3 batch 15): everything a running machine keeps outside RAM, which the PCSX2 file set has no place
//! for. A session that started from a PCSX2 state resumes without it (`Machine::load_resume` rebuilds what it can); with it, a quick save
//! resumes exactly: the cold-boot kernel (a session that started from the disc: handler lists, interrupted contexts, threads,
//! semaphores ...), the field timer, the video mode, the RPC client table, the sound driver model, the memory-card server (open files),
//! GS / VIF / DMA registers and the CPU's own bookkeeping (`Cpu::save_dyn_live`). Layout: magic, video mode, snd_cd_pending, mem.tick,
//! mem.dma_due, RPC binds, kernel flag [+ kernel], the `save_dyn_live` bytes.
//! Version 3 (`RC2DYN03`): magic, `LAYOUT`, that body, FNV-1a of everything before it. Version 2 files (builds 19-22: no layout id, no hash) still
//! load, checked by the trial parse. Whatever the version, `read` parses everything into temporaries (a scratch Cpu takes the `save_dyn_live`
//! bytes first) and touches the machine only when the file parsed to its last byte: a short, old or foreign file is an Err, never a panic.
use crate::cpu::Cpu;
use crate::snapfile::{Rd, Wr};

const MAGIC: &[u8; 8] = b"RC2DYN03";
const MAGIC_V2: &[u8; 8] = b"RC2DYN02";

/// Layout id of the bytes below: BUMP it whenever `Cpu::save_dyn_live`, `Kernel::save`, `Mc::save` or the header of `write` changes
/// (the pin test `layout_is_pinned` fails on any such change; it also guards the fndiff snapshot format that shares `save_dyn_live`).
pub const LAYOUT: u32 = 1;

/// FNV-1a 64 of `b`.
pub fn fnv(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &x| (h ^ x as u64).wrapping_mul(0x0100_0000_01b3))
}

/// The bytes of dyn.bin for `c` (the game thread parked between fields).
pub fn write(c: &Cpu) -> Vec<u8> {
    let mut w = Wr::new();
    w.0.extend_from_slice(MAGIC);
    w.u32(LAYOUT);
    w.bool(crate::ver::ntsc());
    w.bool(c.snd_cd_pending);
    w.u64(c.mem.tick);
    w.u64(c.mem.dma_due);
    let binds = crate::rc2::hle::binds();
    w.u64(binds.len() as u64);
    for (a, b) in binds {
        w.u32(a);
        w.u32(b);
    }
    match &c.cold {
        Some(k) => {
            w.bool(true);
            k.save(&mut w);
        }
        None => w.bool(false),
    }
    let mut d = Wr::new();
    c.save_dyn_live(&mut d);
    w.bytes(&d.0);
    let h = fnv(&w.0);
    w.u64(h);
    w.0
}

/// A dyn.bin parsed into temporaries (nothing of the machine touched yet).
struct Parsed<'a> {
    ntsc: bool,
    cd_pending: bool,
    tick: u64,
    dma_due: u64,
    binds: Vec<(u32, u32)>,
    cold: Option<crate::cold::Kernel>,
    dyn_bytes: &'a [u8],
}

/// The header and the kernel of the body `b` (after magic and layout id). Panics on a short file (`Rd`): the caller catches that.
fn parse(b: &[u8]) -> Parsed<'_> {
    let mut r = Rd::new(b);
    let (ntsc, cd_pending) = (r.bool(), r.bool());
    let (tick, dma_due) = (r.u64(), r.u64());
    let binds: Vec<(u32, u32)> = (0..r.count(8)).map(|_| (r.u32(), r.u32())).collect();
    let cold = r.bool().then(|| {
        let mut k = crate::cold::Kernel::fresh(std::env::var("COLD_HOOKS").ok().as_deref());
        k.load(&mut r);
        k
    });
    let dyn_bytes = r.bytes();
    assert!(r.done(), "dyn.bin: bytes after the dynamic state");
    Parsed { ntsc, cd_pending, tick, dma_due, binds, cold, dyn_bytes }
}

/// The part of dyn.bin that follows magic and (v3) layout id: Err unless the whole file is a version this build wrote.
fn body(b: &[u8]) -> Result<&[u8], String> {
    match b.get(..8) {
        Some(m) if m == MAGIC => {
            let n = b.len();
            if n < 8 + 4 + 8 {
                return Err("dyn.bin: truncated".into());
            }
            let layout = u32::from_le_bytes(b[8..12].try_into().unwrap());
            if layout != LAYOUT {
                return Err(format!("dyn.bin: layout {layout}, this build reads layout {LAYOUT}"));
            }
            if fnv(&b[..n - 8]) != u64::from_le_bytes(b[n - 8..].try_into().unwrap()) {
                return Err("dyn.bin: checksum mismatch (truncated or damaged)".into());
            }
            Ok(&b[12..n - 8])
        }
        Some(m) if m == MAGIC_V2 => Ok(&b[8..]),
        _ => Err("dyn.bin: not a dyn.bin of this program".into()),
    }
}

/// Applies dyn.bin to `c`, which `Machine::load` just built from the same dir (RAM, IOP RAM, stubs, ISO handle). Err: not a dyn.bin
/// this build can read (nothing was changed).
pub fn read(c: &mut Cpu, b: &[u8]) -> Result<(), String> {
    let body = body(b)?;
    // Trial: header, kernel and the dynamic state parsed into temporaries (a scratch Cpu) down to the last byte; a panic of the
    // reader on a short or foreign file is an Err here.
    let trial = std::panic::catch_unwind(|| {
        let p = parse(body);
        let mut r = Rd::new(p.dyn_bytes);
        Cpu::new(crate::mem::Mem::new()).load_dyn(&mut r);
        assert!(r.done(), "dyn.bin: the dynamic state ends early");
    });
    if trial.is_err() {
        return Err("dyn.bin: does not parse (older build or damaged file)".into());
    }
    let p = parse(body);
    if let Some(k) = p.cold {
        c.cold = Some(Box::new(k));
        // The pcs the kernel's rules and vectors must see (`stub_bits`); the field timer it arms is replaced below.
        crate::cold::install(c, true);
    }
    c.load_dyn(&mut Rd::new(p.dyn_bytes));
    c.mem.tick = p.tick;
    c.mem.dma_due = p.dma_due;
    c.snd_cd_pending = p.cd_pending;
    crate::ver::set_ntsc(p.ntsc);
    crate::rc2::hle::set_binds(&p.binds);
    // The host's switches, set again by the host: the sound event recorder.
    c.snd.record = false;
    if let Some(i) = c.snd.inner.as_mut() {
        i.record = false;
    }
    // The save file, not the card of the moment of the save, is the card.
    c.mc.refresh_from_disk();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::Mem;

    /// Pinned: length and FNV of a fresh Cpu's `save_dyn_live` bytes and of a fresh cold kernel's `save` bytes.
    const LIVE_LEN: usize = 133170;
    const LIVE_HASH: u64 = 16048147157655539949;
    const KERN_LEN: usize = 80;
    const KERN_HASH: u64 = 1829144859873806800;

    fn live_bytes() -> Vec<u8> {
        let mut w = Wr::new();
        Cpu::new(Mem::new()).save_dyn_live(&mut w);
        w.0
    }

    fn kernel_bytes() -> Vec<u8> {
        let mut w = Wr::new();
        crate::cold::Kernel::fresh(None).save(&mut w);
        w.0
    }

    /// LAYOUT is tied to the bytes of `save_dyn_live` (a fresh Cpu) and of a fresh cold kernel: this test fails when anyone adds or moves a
    /// field there. Then bump `LAYOUT` (old dyn.bin files are refused instead of misread) and update the pinned numbers.
    #[test]
    fn layout_is_pinned() {
        let (live, kern) = (live_bytes(), kernel_bytes());
        assert_eq!((LAYOUT, live.len(), fnv(&live), kern.len(), fnv(&kern)), (1, LIVE_LEN, LIVE_HASH, KERN_LEN, KERN_HASH), "dyn.bin layout changed: bump statedyn::LAYOUT and re-pin");
    }

    /// A good file loads; a truncated copy, a foreign layout id, flipped bytes and empty input are Errs (no panic, machine untouched).
    #[test]
    fn damaged_files_are_refused_without_a_panic() {
        let src = Cpu::new(Mem::new());
        let good = write(&src);
        assert!(read(&mut Cpu::new(Mem::new()), &good).is_ok(), "round trip");
        for cut in [0, 5, 8, 11, 12, 20, good.len() / 2, good.len() - 9, good.len() - 1] {
            let mut c = Cpu::new(Mem::new());
            assert!(read(&mut c, &good[..cut]).is_err(), "truncated to {cut}");
            assert!(c.cold.is_none());
        }
        let mut other = good.clone();
        other[8] = LAYOUT as u8 + 1;
        assert!(read(&mut Cpu::new(Mem::new()), &other).unwrap_err().contains("layout"));
        let mut flip = good.clone();
        flip[good.len() / 2] ^= 0x55;
        assert!(read(&mut Cpu::new(Mem::new()), &flip).is_err());
        assert!(read(&mut Cpu::new(Mem::new()), b"").is_err());
        assert!(read(&mut Cpu::new(Mem::new()), b"RC2DYN02garbage-from-another-build").is_err(), "a version 2 file of another layout");
    }

    /// A version 2 file (builds 19-22: magic, body, no layout id and no hash) still loads.
    #[test]
    fn version_2_files_still_load() {
        let good = write(&Cpu::new(Mem::new()));
        let mut v2 = MAGIC_V2.to_vec();
        v2.extend_from_slice(&good[12..good.len() - 8]);
        assert!(read(&mut Cpu::new(Mem::new()), &v2).is_ok());
        assert!(read(&mut Cpu::new(Mem::new()), &v2[..v2.len() - 3]).is_err());
    }
}
