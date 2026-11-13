//! The cold-boot kernel's dynamic state to bytes and back (quick save of a session that started from the disc; R&C2 round 3
//! batch 15). Everything `Kernel` keeps outside RAM that the game can notice: the INTC / DMAC handler lists, the interrupted-context
//! stack (a save inside a handler), SIF registers, threads, heap end, semaphores, the id counters and the console's OSD word.
//! What is not saved: the hook rules and their debug counters (`Kernel::fresh` rebuilds the rules; trace / memat / stop counters are
//! tools), the syscall statistics (`sys_seen`, `unhandled`, `sema_blocked`, `irq_cnt`: reports only) and `fault_info`.
use crate::cold::{Hnd, Kernel, Saved};
use crate::snapfile::{Rd, Wr};

fn save_hnds(w: &mut Wr, v: &[Hnd]) {
    w.u64(v.len() as u64);
    for h in v {
        for x in [h.id, h.cause, h.func, h.gp, h.arg] {
            w.u32(x);
        }
    }
}

fn load_hnds(r: &mut Rd) -> Vec<Hnd> {
    (0..r.count(20)).map(|_| Hnd { id: r.u32(), cause: r.u32(), func: r.u32(), gp: r.u32(), arg: r.u32() }).collect()
}

fn save_ctx(w: &mut Wr, s: &Saved) {
    for &x in &s.r {
        w.u128(x);
    }
    w.u128(s.hi);
    w.u128(s.lo);
    w.u32(s.sa);
    for &x in &s.f {
        w.u32(x);
    }
    for x in [s.facc, s.fcr31, s.epc, s.cause] {
        w.u32(x);
    }
    w.bool(s.dmac);
    w.u64(s.queue.len() as u64);
    for &(a, b, c) in &s.queue {
        for x in [a, b, c] {
            w.u32(x);
        }
    }
}

fn load_ctx(r: &mut Rd) -> Saved {
    let mut reg = [0u128; 32];
    for x in reg.iter_mut() {
        *x = r.u128();
    }
    let (hi, lo, sa) = (r.u128(), r.u128(), r.u32());
    let mut f = [0u32; 32];
    for x in f.iter_mut() {
        *x = r.u32();
    }
    let (facc, fcr31, epc, cause) = (r.u32(), r.u32(), r.u32(), r.u32());
    let dmac = r.bool();
    let queue = (0..r.count(12)).map(|_| (r.u32(), r.u32(), r.u32())).collect();
    Saved { r: reg, hi, lo, sa, f, facc, fcr31, epc, queue, cause, dmac }
}

impl Kernel {
    /// Writes the dynamic state (see the module comment).
    pub fn save(&self, w: &mut Wr) {
        save_hnds(w, &self.intc);
        save_hnds(w, &self.dmac);
        w.u32(self.next_id);
        w.u64(self.stack.len() as u64);
        for s in &self.stack {
            save_ctx(w, s);
        }
        w.map(&self.sif_reg, |w, k| w.u32(k), |w, v| w.u32(v));
        w.u64(self.threads.len() as u64);
        for &(a, b, c) in &self.threads {
            for x in [a, b, c] {
                w.u32(x);
            }
        }
        w.u32(self.heap_end);
        w.u64(self.frames);
        w.map(&self.sema, |w, k| w.u32(k), |w, v| {
            w.i32(v.0);
            w.i32(v.1)
        });
        w.u32(self.sema_next);
        w.u64(self.next_vb);
        w.u32(self.osd_word);
    }

    /// Inverse of `save`, onto a kernel from `Kernel::fresh`.
    pub fn load(&mut self, r: &mut Rd) {
        self.intc = load_hnds(r);
        self.dmac = load_hnds(r);
        self.next_id = r.u32();
        self.stack = (0..r.count(16 * 35)).map(|_| load_ctx(r)).collect();
        self.sif_reg = r.map(|r| r.u32(), |r| r.u32());
        self.threads = (0..r.count(12)).map(|_| (r.u32(), r.u32(), r.u32())).collect();
        self.heap_end = r.u32();
        self.frames = r.u64();
        self.sema = r.map(|r| r.u32(), |r| (r.i32(), r.i32()));
        self.sema_next = r.u32();
        self.next_vb = r.u64();
        self.osd_word = r.u32();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kernel_state_round_trips() {
        let mut k = Kernel::default();
        k.intc.push(Hnd { id: 3, cause: 2, func: 0x1234, gp: 0x5678, arg: 9 });
        k.dmac.push(Hnd { id: 4, cause: u32::MAX, func: 0x2000, gp: 1, arg: 2 });
        k.next_id = 4;
        k.sif_reg.insert(4, 0x7_0001);
        k.threads.push((0x100000, 0x200000, 0x4000));
        k.heap_end = 0x01f0_0000;
        k.frames = 77;
        k.sema.insert(0x201, (1, 3));
        k.sema_next = 1;
        k.next_vb = 5_900_000;
        k.osd_word = 0x0002_a002;
        let mut s = Saved { r: [0; 32], hi: 5, lo: 6, sa: 7, f: [0; 32], facc: 8, fcr31: 9, epc: 0x11b1a0, queue: vec![(1, 2, 3)], cause: 2, dmac: true };
        s.r[29] = 0xdead_beef_u128 << 64 | 0x01ff_fff0;
        s.f[3] = 0x3f80_0000;
        k.stack.push(s);
        let mut w = Wr::new();
        k.save(&mut w);
        let mut b = Kernel::default();
        let mut r = Rd::new(&w.0);
        b.load(&mut r);
        assert!(r.done(), "the reader consumed every byte");
        let mut w2 = Wr::new();
        b.save(&mut w2);
        assert_eq!(w.0, w2.0, "saving the loaded kernel gives the same bytes");
        assert_eq!((b.intc[0].func, b.intc[0].gp, b.dmac[0].cause, b.next_id, b.heap_end, b.osd_word), (0x1234, 0x5678, u32::MAX, 4, 0x01f0_0000, 0x0002_a002));
        assert_eq!((b.stack[0].epc, b.stack[0].queue.clone(), b.stack[0].r[29] as u32, b.stack[0].f[3]), (0x11b1a0, vec![(1, 2, 3)], 0x01ff_fff0, 0x3f80_0000));
        assert_eq!((b.sema[&0x201], b.sif_reg[&4], b.threads[0].2), ((1, 3), 0x7_0001, 0x4000));
    }
}
