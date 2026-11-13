//! Quick save for a machine started from a savestate dir (R&C2 `RAC2_STATE`, F5): the inverse of `load_raw`. Writes the files
//! `p2s.py` writes (eeMemory.bin, Scratchpad.bin, vu0/vu1 memories, eeHwRegs.bin, iopMemory.bin, regs.bin, GS.bin) so that
//! `Machine::load` starts from it. Call between fields (the game thread parked at the vblank wait). New file 2026-11-02
//! (R&C2 round 3 batch 10); nothing in R&C1's paths calls it.
use crate::Machine;
use std::io::Write;
use std::path::Path;

/// regs.bin (p2s.py layout, 1476 bytes) of `c`; `None` when the machine is not parked on a plain instruction boundary.
pub fn regs_bin(c: &crate::cpu::Cpu) -> Option<Vec<u8>> {
    // A pc in a branch delay slot (npc != pc + 4) is fine: dyn.bin carries npc; a PCSX2-format reader resumes at pc alone.
    let mut b = Vec::with_capacity(1476);
    for r in &c.r {
        b.extend_from_slice(&r.to_le_bytes());
    }
    b.extend_from_slice(&c.hi.to_le_bytes());
    b.extend_from_slice(&c.lo.to_le_bytes());
    for w in &c.cp0 {
        b.extend_from_slice(&w.to_le_bytes());
    }
    for w in [c.sa, c.pc, 0] {
        b.extend_from_slice(&w.to_le_bytes());
    }
    for w in &c.f {
        b.extend_from_slice(&w.to_le_bytes());
    }
    b.extend_from_slice(&c.fcr31.to_le_bytes());
    b.extend_from_slice(&c.facc.to_le_bytes());
    for v in &c.vf {
        for w in v {
            b.extend_from_slice(&w.to_le_bytes());
        }
    }
    for w in &c.vacc {
        b.extend_from_slice(&w.to_le_bytes());
    }
    for w in &c.vi {
        b.extend_from_slice(&w.to_le_bytes());
    }
    debug_assert_eq!(b.len(), 1476);
    Some(b)
}

/// GS.bin of PCSX2's freeze version 9: 425 header bytes, the 4 MB local memory, 84 tail bytes (`Gs::load_vram` reads only the middle).
pub fn gs_bin(vram: &[u8]) -> Vec<u8> {
    let mut b = vec![0u8; 425];
    b.extend_from_slice(&vram[..4 << 20]);
    b.extend_from_slice(&[0u8; 84]);
    b
}

/// The disc image `Machine::load` reads (`load_raw`): set by a host that knows it (R&C2: its `--iso`); else ORACLE_ISO, else the default.
pub static ISO: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

impl Machine {
    /// `Machine::load` for a host that resumes a state (R&C2 `RAC2_STATE`, quick saves). A PCSX2 state is caught anywhere in
    /// the game's vblank wait (R&C2: pc 0x11b1a0, after the INTC_STAT check), where the oracle's vsync arming (the game's
    /// SetVSyncFlag, made before the state) is not known: armed, the next pass over the wait raises the vblank. A quick save
    /// is parked AT the wait (pc == vblank_pc, after the vblank was raised: INTC_STAT in the saved hw regs has the bit), where
    /// it stays unarmed so the first field continues exactly as the saved machine would have.
    pub fn load_resume(dir: &str) -> Machine {
        let mut m = Machine::load(dir);
        // A state session behaves like the cold boot for the memory card: the save file, else a freshly formatted card in port 0 and
        // nothing in port 1 (without it the game saw "no card" and showed "Save failed / check Memory Card" at its first autosave).
        // Before dyn.bin: its card refresh builds the cards by this rule.
        if crate::ver::rc2() {
            m.cpu.mc.blank_default = true;
        }
        // dyn.bin (quick saves): the machine's own state outside RAM (statedyn.rs), cold-boot kernel included.
        let mut dyn_applied = false;
        if let Ok(x) = std::fs::read(format!("{dir}/dyn.bin")) {
            match crate::statedyn::read(&mut m.cpu, &x) {
                Ok(()) => dyn_applied = true,
                Err(e) => eprintln!("state {dir}: {e}; resuming without the dynamic state"),
            }
        }
        // extra.bin (quick saves of build 13..18: no dyn.bin): the oracle's clocks, which a PCSX2 dir has no place for.
        let mut quick_save = dyn_applied;
        if !dyn_applied {
            if let Ok(x) = std::fs::read(format!("{dir}/extra.bin")) {
                if x.len() >= 20 {
                    quick_save = true;
                    m.cpu.icount = u64::from_le_bytes(x[0..8].try_into().unwrap());
                    m.cpu.mem.now = u64::from_le_bytes(x[8..16].try_into().unwrap());
                    m.cpu.mem.rcnt1 = u32::from_le_bytes(x[16..20].try_into().unwrap());
                }
            }
        }
        if crate::ver::rc2() {
            if !dyn_applied {
                let n = crate::rc2::hle::seed_binds(&m.cpu);
                if std::env::var_os("RAC2_LOG").is_some() {
                    eprintln!("state: {n} RPC clients found in RAM");
                }
            }
        }
        if !quick_save {
            m.cpu.vsync_armed = true;
        }
        // The field timer, as on a cold boot (`cold::install`): a game loop that never reaches the vblank wait still gets its
        // field tick (`cold::tick`) after one field's worth of instructions. R&C2's level loader busy-waits for the 989snd CD
        // channel, whose completion (`snd_cd_finish`) comes with that tick: without the timer a state run hung there for good
        // (batch 11: replay from fullgame1 0010, pc 0x3308a0 loop, SND_CD_BUSY 1). A wait that is reached in time re-arms it
        // (`vblank_hook`), so fields that end at the wait are unchanged. dyn.bin carries the timer as it was.
        if crate::ver::rc2() && !dyn_applied {
            let now = m.cpu.icount;
            m.cpu.mem.rearm_tick(now);
        }
        m
    }

    /// A copy of everything `save_state` writes except the host's GS pipe state, taken on the game thread between fields (a few ms);
    /// `StateImage::write` then does the slow part on another thread.
    pub fn snapshot_state(&self) -> Result<StateImage, String> {
        let c = &*self.cpu;
        let regs = regs_bin(c).ok_or("the machine is not on an instruction boundary")?;
        let mut x = Vec::with_capacity(20);
        x.extend_from_slice(&c.icount.to_le_bytes());
        x.extend_from_slice(&c.mem.now.to_le_bytes());
        x.extend_from_slice(&c.mem.rcnt1.to_le_bytes());
        Ok(StateImage {
            files: vec![
                ("eeMemory.bin", c.mem.ram[..32 << 20].to_vec()),
                ("Scratchpad.bin", c.mem.spr.clone()),
                ("vu0Memory.bin", c.mem.vu0d.clone()),
                ("vu0MicroMem.bin", c.mem.vu0m.clone()),
                ("eeHwRegs.bin", c.mem.hw.clone()),
                ("iopMemory.bin", c.iop.clone()),
                ("regs.bin", regs),
                ("extra.bin", x),
                ("dyn.bin", crate::statedyn::write(c)),
            ],
            vu1: (c.mem.vu1d.clone(), c.mem.vu1m.clone()),
        })
    }

    /// Writes the machine as a savestate dir (`snapshot_state` + `StateImage::write`; test and tool use).
    pub fn save_state(&self, dir: &Path, vram: Option<&[u8]>, vu1: Option<(&[u8], &[u8])>) -> Result<(), String> {
        let img = self.snapshot_state()?;
        img.write(dir, vram.unwrap_or(&vec![0u8; 4 << 20]), vu1.map(|v| (v.0.to_vec(), v.1.to_vec())))
    }
}

/// A machine's state in memory, ready to be written (`Machine::snapshot_state`).
pub struct StateImage {
    files: Vec<(&'static str, Vec<u8>)>,
    vu1: (Vec<u8>, Vec<u8>),
}

impl StateImage {
    /// The saved RAM (for the hash a save prints).
    pub fn ram(&self) -> &[u8] {
        &self.files[0].1
    }

    /// Writes the dir. `vram` is the GS local memory (4 MB) and `vu1` (data, micro) the host's VU1 when the host's GS pipe ran it,
    /// else the machine's. Files go to `dir.tmp` and the dir is renamed into place, so a half-written save never replaces a good one.
    pub fn write(self, dir: &Path, vram: &[u8], vu1: Option<(Vec<u8>, Vec<u8>)>) -> Result<(), String> {
        let tmp = dir.with_extension("tmp");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        let put = |n: &str, d: &[u8]| -> Result<(), String> {
            let mut f = std::fs::File::create(tmp.join(n)).map_err(|e| format!("{n}: {e}"))?;
            f.write_all(d).map_err(|e| format!("{n}: {e}"))
        };
        for (n, d) in &self.files {
            put(n, d)?;
        }
        let (vd, vm) = vu1.unwrap_or(self.vu1);
        put("vu1Memory.bin", &vd)?;
        put("vu1MicroMem.bin", &vm)?;
        put("GS.bin", &gs_bin(vram))?;
        let _ = std::fs::remove_dir_all(dir);
        std::fs::rename(&tmp, dir).map_err(|e| e.to_string())
    }
}
