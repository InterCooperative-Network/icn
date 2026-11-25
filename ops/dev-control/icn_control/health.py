"""Host health: filesystems, memory, load. Read-only, no external commands."""

from __future__ import annotations

import os

GIB = 1024 ** 3


def filesystems(mounts=("/", "/mnt/build")) -> list[dict]:
    out = []
    for m in mounts:
        try:
            st = os.statvfs(m)
        except OSError as exc:
            out.append({"mount": m, "error": str(exc)})
            continue
        size = st.f_blocks * st.f_frsize
        free = st.f_bavail * st.f_frsize
        used = size - st.f_bfree * st.f_frsize
        out.append({"mount": m, "size_gib": round(size / GIB, 1), "free_gib": round(free / GIB, 1),
                    "used_pct": round(100 * used / size, 1) if size else None,
                    "is_mountpoint": os.path.ismount(m)})
    return out


def memory(proc: str = "/proc") -> dict:
    info = {}
    with open(os.path.join(proc, "meminfo")) as f:
        for line in f:
            k, _, v = line.partition(":")
            parts = v.split()
            if parts and parts[0].isdigit():
                info[k] = int(parts[0]) * 1024
    total, avail = info.get("MemTotal"), info.get("MemAvailable")
    return {"total_gib": round(total / GIB, 1) if total else None,
            "available_gib": round(avail / GIB, 1) if avail else None,
            "swap_used_gib": round((info.get("SwapTotal", 0) - info.get("SwapFree", 0)) / GIB, 1)}


def load(proc: str = "/proc") -> dict:
    with open(os.path.join(proc, "loadavg")) as f:
        a = f.read().split()
    return {"load1": float(a[0]), "load5": float(a[1]), "load15": float(a[2]), "cpus": os.cpu_count()}


def uptime_s(proc: str = "/proc") -> float:
    with open(os.path.join(proc, "uptime")) as f:
        return float(f.read().split()[0])
