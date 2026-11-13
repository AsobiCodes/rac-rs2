#!/usr/bin/env python3
"""rc2_stubs.py - writes the generated VU files of the source tree for an R&C2 install: native VU0 programs from the R&C2 disc
(--iso), EMPTY stubs for the rest.

The engine and the oracle compile a few files that R&C1's tools/vugen_iso.py makes from the R&C1 disc (native VU0/VU1 translations,
the one-joint skinning fast path). R&C2 runs VU1 on the interpreter and has no one-joint skinning fast path, and its VU0 prog1
differs: those get stubs. The stubs hold no game data: every program is "not known", so the interpreter / compiled code runs.
Two VU0 programs of R&C2 (prog0, prog9hi) have the same code as R&C1's and are translated from the player's own R&C2 disc image
(see VU0 below); a player of R&C2 need not own R&C1. Writes only files whose content differs (cargo rebuilds by file time).

  python tools/rc2_stubs.py [--tree DIR] [--iso "<the R&C2 .iso>"]     (default tree: the folder above tools/)
"""
import argparse, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

SKIN_EE = '''//! Stub (tools/rc2_stubs.py): no one-joint skinning fast path (R&C2). Do not edit.
#![allow(unused_variables, dead_code, clippy::all)]
use super::*;
pub const PASS_INSNS: u64 = 1;
pub const PASS_STORES: usize = 0;
pub const LOOP_END: u32 = 0;
pub const BLOCKS: [u32; 5] = [0; 5];

pub struct Regs;
impl Regs {
    #[inline(always)]
    pub fn load(_r: &[u128; 32]) -> Regs { Regs }
    #[inline(always)]
    pub fn store(&self, _r: &mut [u128; 32]) {}
}
'''
for _i in range(5):
    SKIN_EE += 'pub fn block_%d(_st: &mut St) -> (bool, u64, u32) { (true, 0, 0) }\n' % _i

SKIN_VU = '''//! Stub (tools/rc2_stubs.py): no native resumes of VU0 prog0 (R&C2). Do not edit.
#![allow(unused_variables, dead_code, clippy::all)]
use crate::vu0aot::V;

pub struct Res {
    pub fu: V,
    pub fm: u32,
    pub park: usize,
    pub pairs: u64,
    pub ok: bool,
}

pub unsafe fn resume(park: usize, _n: usize, w: &mut [V; 32], _dm: *mut u8, _vi: &mut [u32; 32], _acc: &mut V) -> Res {
    Res { fu: w[0], fm: 0, park, pairs: 0, ok: false }
}

pub const VI02_WAITS: [usize; 0] = [];
'''

VU1_GEN = '''// Stub (tools/rc2_stubs.py): no native VU1 programs (R&C2 runs VU1 on the interpreter). Do not edit.
pub(crate) static PROGS: &[super::Native] = &[];
'''


def vu0_stub(name):
    base = {'prog0': 0, 'prog1': 0, 'prog9hi': 0xc80}[name]
    up = name.upper()
    b = '/// Micro address of the first byte of %s.\npub const BASE: usize = %#x;\n\n' % (up, base) if base else ''
    return '''//! Stub (tools/rc2_stubs.py): %s has no native code (R&C2); the interpreter runs it. Do not edit.
#![allow(dead_code, clippy::all)]
use crate::cpu::Cpu;

pub const NODES: usize = 0;

%s/// Empty: never matches micro memory (`vu0_aot_check`).
pub static %s: [u8; 0] = [];

pub fn entry_node(_pc: usize) -> Option<u32> {
    None
}

pub fn park_reg(_pc: usize) -> Option<usize> {
    None
}

impl Cpu {
    pub(crate) fn vu0_%s_run<const COV: bool>(&mut self, _n: u32) -> Result<(u64, u64, usize, bool), String> {
        Err("vu0_%s: no native code".into())
    }
}
''' % (name, b, up, name, name)


# ---- native VU0 programs of R&C2 (--iso) ---------------------------------------------------------------------------------------
# Two VU0 microprograms of R&C2 have the same code as R&C1's: prog0 (the skinned-vertex program, run for every animated model) and
# prog9hi (a sine / rotation-matrix piece). The game logic thread spends most of its time in them: with the interpreter one game
# step takes about 40 ms, with the native translation about 10 ms (measured, 24-thread PC, the 60-per-second budget is 16.7 ms).
# So they are cut out of the player's own disc image here and translated by tools/vu/vu0gen.py, exactly as R&C1's setup does it.
# name -> (size in bytes, file offsets of its 0x800-byte chunks in the boot ELF, SHA-256 of the assembled microcode, micro address)
BOOT_NAMES = ('SCES_516.07',)
VU0 = {
    'prog0': (2624, [0x1010, 0x1818], '47ea7d1c2c42b850a770a7af39932ee552d423ec8f7bc5860186d0b3fa89dde6', 0),
    'prog9hi': (752, [0xf970], 'f92033827dd132fa121e9303eecd4742fe79a39424cba78ddcdb2834c8c42be9', 0xc80),
}
ISO_SEC = 0x800


def boot_elf(iso):
    """The boot ELF via a minimal ISO9660 walk (root directory record); None when the disc has none of BOOT_NAMES."""
    import struct
    with open(iso, 'rb') as f:
        f.seek(16 * ISO_SEC)
        pvd = f.read(ISO_SEC)
        if pvd[1:6] != b'CD001':
            return None
        root_lba, root_len = struct.unpack_from('<I', pvd, 158)[0], struct.unpack_from('<I', pvd, 166)[0]
        f.seek(root_lba * ISO_SEC)
        d = f.read(root_len)
        o = 0
        while o < len(d):
            ln = d[o]
            if ln == 0:
                o = (o // ISO_SEC + 1) * ISO_SEC
                continue
            name = d[o + 33:o + 33 + d[o + 32]].decode('latin1').upper()
            if name.startswith(BOOT_NAMES):
                f.seek(struct.unpack_from('<I', d, o + 2)[0] * ISO_SEC)
                return f.read(struct.unpack_from('<I', d, o + 10)[0])
            o += ln
    return None


def vu0_native(iso, tree):
    """{'oracle/src/vu0_<name>.rs': text} for the programs of VU0 found on the disc with the known code (others: not in the dict)."""
    import hashlib, shutil, subprocess
    vu = os.path.join(HERE, 'vu')
    if not os.path.isfile(os.path.join(vu, 'vu0gen.py')):
        print('rc2_stubs: tools/vu/vu0gen.py is missing, VU0 stays in the interpreter (slow)')
        return {}
    elf = boot_elf(iso)
    if elf is None:
        print('rc2_stubs: no known boot file on the disc image, VU0 stays in the interpreter (slow)')
        return {}
    sys.path.insert(0, vu)
    sys.dont_write_bytecode = True
    import vu_disasm
    stage = os.path.join(tree, 'generated', 'vu_stage')
    shutil.rmtree(stage, ignore_errors=True)
    os.makedirs(stage)
    out = {}
    try:
        for name, (size, offs, want, base) in VU0.items():
            b = bytearray(size)
            for i, o in enumerate(offs):
                lo = i * 0x800
                ln = min(0x800, size - lo)
                b[lo:lo + ln] = elf[o:o + ln]
            b = bytes(b)
            if hashlib.sha256(b).hexdigest() != want:
                print('rc2_stubs: %s on this disc is not the known code, it stays in the interpreter' % name)
                continue
            with open(os.path.join(stage, name + '.bin'), 'wb') as f:
                f.write(b)
            with open(os.path.join(stage, name + '.s'), 'w', encoding='utf-8', newline='\n') as f:
                f.write(vu_disasm.render(b, base)[0])
            rs = os.path.join(stage, 'vu0_%s.rs' % name)
            r = subprocess.run([sys.executable, '-I', '-B', os.path.join(vu, 'vu0gen.py'), rs], cwd=vu, env=dict(os.environ, VU_BINS=stage, VU0_PROG=name))
            if r.returncode or not os.path.isfile(rs):
                sys.exit('rc2_stubs: tools/vu/vu0gen.py failed (%s)' % name)
            out['oracle/src/vu0_%s.rs' % name] = open(rs, 'rb').read().decode('utf-8')
    finally:
        shutil.rmtree(stage, ignore_errors=True)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--tree', default=os.path.join(HERE, '..'))
    ap.add_argument('--iso', help='the R&C2 disc image: translate its VU0 programs prog0 and prog9hi to native code (without it: stubs only)')
    a = ap.parse_args()
    t = os.path.abspath(a.tree)
    files = {
        'oracle/src/vu0_prog0.rs': vu0_stub('prog0'),
        'oracle/src/vu0_prog1.rs': vu0_stub('prog1'),
        'oracle/src/vu0_prog9hi.rs': vu0_stub('prog9hi'),
        'oracle/src/skinff_ee.rs': SKIN_EE,
        'oracle/src/skinff_vu.rs': SKIN_VU,
        'rac-rs/src/engine/gsfull/vu1aot_gen.rs': VU1_GEN,
    }
    native = vu0_native(a.iso, t) if a.iso else {}
    files.update(native)
    n = 0
    for rel, txt in files.items():
        p = os.path.join(t, rel)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        old = open(p, 'rb').read() if os.path.exists(p) else None
        new = txt.encode('utf-8')
        if old != new:
            with open(p, 'wb') as f:
                f.write(new)
            n += 1
    print('rc2_stubs: %d of %d files written; native VU0 programs: %s' % (n, len(files), ', '.join(sorted(k.split('vu0_')[1][:-3] for k in native)) or 'none'))


main()
