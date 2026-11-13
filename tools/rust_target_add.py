#!/usr/bin/env python3
"""rust_target_add.py - adds the libraries of ONE more build target to an installed Rust toolchain folder, without rustup.

  python -I tools/rust_target_add.py DEST TARGET [--implib=FILE.a ...] PKG.tar.xz [PKG.tar.xz ...]

Called by setup.ps1 step (a) for Ratchet & Clank 2 on Windows: the game exe is built for the target x86_64-pc-windows-gnullvm
(with the pinned GNU-host Rust that setup installs anyway), because on the plain GNU target every use of a per-thread variable
goes through a slow, shared call of the Rust runtime and the game used several times more CPU. The two official packages of that
target (rust-std = the standard library, rust-mingw = the C startup files and import libraries; SHA-256 checked by setup.ps1
against the release manifest) are laid out as rustup would: every file their manifest.in lists goes to the same relative path
under DEST, and all of them must lie under lib/rustlib/TARGET/. That folder is built next to its place (TARGET.part) and renamed
at the end, so it is either complete or absent; an existing one is removed first (files of a hard-linked toolchain are only
unlinked, never written). Uses only the standard library (tarfile reads .tar.xz).
"""
import os
import shutil
import sys
import tarfile


def safe(rel):
    return rel and not rel.startswith('/') and ':' not in rel and '..' not in rel.split('/')


def unpack(pkg, prefix, part):
    comps, man, top = [], {}, ''
    with tarfile.open(pkg, 'r:xz') as t:
        for m in t:
            p = m.name.split('/')
            top = p[0]
            if m.isfile() and len(p) == 2 and p[1] == 'components':
                comps = t.extractfile(m).read().decode('utf-8').split()
            elif m.isfile() and len(p) == 3 and p[2] == 'manifest.in':
                man[p[1]] = t.extractfile(m).read().decode('utf-8')
    name = os.path.basename(pkg)
    if not comps or any(c not in man for c in comps):
        sys.exit('rust_target_add.py: %s has no components / manifest.in' % name)
    n = 0
    for comp in comps:
        files = set()
        for line in man[comp].splitlines():
            kind, _, rel = line.strip().partition(':')
            if kind == 'file' and safe(rel):
                if not rel.startswith(prefix):
                    sys.exit('rust_target_add.py: %s: %s is outside %s' % (name, rel, prefix))
                files.add(rel)
            elif kind == 'dir':
                sys.exit('rust_target_add.py: %s lists a whole folder (%s); not expected in a target package' % (name, rel))
        done = set()
        with tarfile.open(pkg, 'r:xz') as t:
            for m in t:
                p = m.name.split('/')
                if len(p) < 3 or p[0] != top or p[1] != comp or not m.isfile():
                    continue
                rel = '/'.join(p[2:])
                if rel not in files:
                    continue
                dst = os.path.join(part, *rel[len(prefix):].split('/'))
                os.makedirs(os.path.dirname(dst), exist_ok=True)
                with t.extractfile(m) as src, open(dst, 'wb') as out:
                    shutil.copyfileobj(src, out, 1 << 20)
                done.add(rel)
                n += 1
        missing = sorted(files - done)
        if missing:
            sys.exit('rust_target_add.py: %s: %d files of manifest.in are not in the package (first: %s)' % (comp, len(missing), missing[0]))
        print('  %s: %d files' % (comp, len(done)), flush=True)
    return n


def main():
    implibs = [a[len('--implib='):] for a in sys.argv[1:] if a.startswith('--implib=')]
    sys.argv = [a for a in sys.argv if not a.startswith('--implib=')]
    if len(sys.argv) < 4:
        sys.exit(__doc__)
    dest, target = os.path.abspath(sys.argv[1]), sys.argv[2]
    if not safe(target) or '/' in target or '\\' in target:
        sys.exit('rust_target_add.py: bad target name')
    if not os.path.isdir(os.path.join(dest, 'lib', 'rustlib')):
        sys.exit('rust_target_add.py: %s is not a Rust toolchain folder (no lib/rustlib)' % dest)
    final = os.path.join(dest, 'lib', 'rustlib', target)
    part = final + '.part'
    if os.path.isdir(part):
        shutil.rmtree(part)
    os.makedirs(part)
    total = 0
    for pkg in sys.argv[3:]:
        total += unpack(pkg, 'lib/rustlib/%s/' % target, part)
    lib = os.path.join(part, 'lib')
    if not os.path.isfile(os.path.join(lib, 'self-contained', 'crt2.o')) or not any(f.startswith('libstd-') and f.endswith('.rlib') for f in os.listdir(lib)):
        sys.exit('rust_target_add.py: the standard library or the startup files are missing after unpacking')
    for src in implibs:
        # an import library the target's own startup package lacks (a copy: a new file, never a link to the source)
        if not os.path.isfile(src):
            sys.exit('rust_target_add.py: %s is missing' % src)
        shutil.copyfile(src, os.path.join(lib, 'self-contained', os.path.basename(src)))
        total += 1
    if os.path.isdir(final):
        shutil.rmtree(final)
    os.replace(part, final)
    print('  %d files unpacked into %s' % (total, final), flush=True)


if __name__ == '__main__':
    main()
