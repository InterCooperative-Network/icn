#!/usr/bin/env python3
"""Audit a package archive BEFORE extracting it.

    python3 -I audit_archive.py <archive.tar.gz> <expected-top-dir>

Standard library only. Exit 0 only if the archive is safe to extract: one top-level directory,
only regular files and directories, no absolute or '..' or backslash paths, no links or devices,
no setuid/setgid/sticky bit, no group/world-writable mode, neutral ownership, no duplicates.
Reads the archive in memory-bounded fashion and never extracts anything.
"""
import sys
import tarfile

archive, top = sys.argv[1], sys.argv[2]
bad = []
with tarfile.open(archive, "r:gz") as t:
    members = t.getmembers()
    names = [m.name.rstrip("/") for m in members]
    for m in members:
        n = m.name.rstrip("/")
        if n.startswith("/") or ".." in n.split("/") or "\\" in n:
            bad.append(("unsafe path", n))
        if not (n == top or n.startswith(top + "/")):
            bad.append(("outside the single top directory", n))
        if m.issym() or m.islnk() or m.isdev() or m.isfifo():
            bad.append(("link or device", n))
        if not (m.isreg() or m.isdir()):
            bad.append(("neither file nor directory", n))
        if m.mode & 0o7022:
            bad.append((f"unsafe mode {m.mode:o}", n))
        if m.uid or m.gid or m.uname or m.gname:
            bad.append(("non-neutral owner", n))
    if len(set(names)) != len(names):
        bad.append(("duplicate entry", ""))
print(f"entries: {len(members)} | regular: {sum(m.isreg() for m in members)} | dirs: {sum(m.isdir() for m in members)}")
print("archive audit:", "CLEAN" if not bad else bad)
sys.exit(1 if bad else 0)
