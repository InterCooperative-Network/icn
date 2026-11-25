"""Process-table, tmux and Claude Code session observation (read-only).

A process is only considered the same process if its PID *and* its kernel start time
match. A recorded PID alone proves nothing: PIDs are reused.
"""

from __future__ import annotations

import glob
import json
import os
from typing import Optional

from .util import Paths, run

AGENT_COMMS = {"claude", "codex"}
BUILD_COMMS = {"cargo", "rustc", "cargo-nextest", "sccache", "cc1", "cc1plus", "ld", "mold", "npm", "node"}
TMUX_SEP = "|~|"  # tmux octal-escapes control characters in -F output, so use a printable separator


def proc_start_ticks(pid: int, proc: str = "/proc") -> Optional[str]:
    """Field 22 (starttime) of /proc/<pid>/stat, or None if the process is gone."""
    try:
        with open(os.path.join(proc, str(pid), "stat")) as f:
            s = f.read()
    except OSError:
        return None
    # comm may contain spaces/parens; fields after the last ')' start at field 3.
    rest = s[s.rfind(")") + 2:].split()
    return rest[19] if len(rest) > 19 else None


def same_process(pid, start: Optional[str], proc: str = "/proc") -> bool:
    """True only if pid is running AND its start time matches the recorded one."""
    try:
        pid = int(pid)
    except (TypeError, ValueError):
        return False
    if not start:
        return False
    return proc_start_ticks(pid, proc) == str(start)


def _read(path: str) -> Optional[str]:
    try:
        with open(path) as f:
            return f.read()
    except OSError:
        return None


def scan_processes(proc: str = "/proc") -> list[dict]:
    """All processes we can see: pid, ppid, comm, cwd, start ticks, tty nr, elapsed basis.

    Command lines are deliberately NOT collected: they can carry prompts or tokens.
    Only an agent's `--name` value is extracted, and only for agent processes.
    """
    out = []
    for d in os.listdir(proc):
        if not d.isdigit():
            continue
        pid = int(d)
        stat = _read(os.path.join(proc, d, "stat"))
        if not stat:
            continue
        comm = stat[stat.find("(") + 1:stat.rfind(")")]
        rest = stat[stat.rfind(")") + 2:].split()
        if len(rest) < 20:
            continue
        try:
            cwd = os.readlink(os.path.join(proc, d, "cwd"))
        except OSError:
            cwd = None
        rec = {"pid": pid, "ppid": int(rest[1]), "comm": comm, "state": rest[0],
               "tty_nr": int(rest[4]), "start": rest[19], "cwd": cwd}
        if comm in AGENT_COMMS:
            argv = (_read(os.path.join(proc, d, "cmdline")) or "").split("\x00")
            rec["agent_name"] = agent_name(argv)
            rec["agent_role"] = agent_role(argv)
            rec["activity_files"] = _open_transcripts(os.path.join(proc, d, "fd"))
        out.append(rec)
    return out


def agent_role(argv: list[str]) -> str:
    """'service' for app-server/daemon/proxy processes, else 'interactive'. Only fixed tokens are inspected."""
    if argv and "/app-server" in (argv[0] or ""):
        return "service"
    if any(a in ("app-server", "proxy", "mcp-server", "daemon") for a in argv[1:3]):
        return "service"
    return "interactive"


def _open_transcripts(fd_dir: str) -> list[str]:
    """Session transcript files (.jsonl) a process currently holds open; their mtime is activity."""
    out = []
    try:
        for fd in os.listdir(fd_dir):
            try:
                t = os.readlink(os.path.join(fd_dir, fd))
            except OSError:
                continue
            if t.endswith(".jsonl") and ("/.codex/sessions/" in t or "/.claude/projects/" in t):
                out.append(t)
    except OSError:
        # best-effort: a process may exit or deny access mid-scan; absence is not evidence
        pass
    return sorted(set(out))


def file_activity(files: list[str]) -> Optional[float]:
    ts = []
    for f in files or []:
        try:
            ts.append(os.path.getmtime(f))
        except OSError:
            # best-effort: a process may exit or deny access mid-scan; absence is not evidence
            pass
    return max(ts) if ts else None


def agent_name(argv: list[str]) -> Optional[str]:
    for i, a in enumerate(argv):
        if a == "--name" and i + 1 < len(argv):
            return argv[i + 1][:80]
        if a.startswith("--name="):
            return a.split("=", 1)[1][:80]
    return None


def boot_time(proc: str = "/proc") -> Optional[float]:
    s = _read(os.path.join(proc, "stat")) or ""
    for line in s.splitlines():
        if line.startswith("btime "):
            return float(line.split()[1])
    return None


def start_epoch(start_ticks: Optional[str], proc: str = "/proc") -> Optional[float]:
    bt = boot_time(proc)
    if bt is None or not start_ticks:
        return None
    try:
        hz = os.sysconf("SC_CLK_TCK")
    except (ValueError, OSError):
        hz = 100
    return bt + int(start_ticks) / hz


def ancestors(pid: int, by_pid: dict) -> list[int]:
    chain, seen = [], set()
    cur = by_pid.get(pid)
    while cur and cur["ppid"] not in seen and cur["ppid"] > 1:
        seen.add(cur["ppid"])
        chain.append(cur["ppid"])
        cur = by_pid.get(cur["ppid"])
    return chain


def tmux_state() -> dict:
    """tmux sessions and panes. Returns {'available': bool, 'sessions': [...], 'panes': [...]}.

    'no server running' is a definite observation (zero sessions), not an error.
    """
    fmt = TMUX_SEP.join(["#{session_name}", "#{session_attached}", "#{session_activity}",
                         "#{session_created}", "#{@icn_control}", "#{@icn_managed}", "#{@icn_purpose}"])
    r = run(["tmux", "list-sessions", "-F", fmt], timeout=5)
    if r.rc == 127:
        return {"available": False, "error": "tmux not installed", "sessions": [], "panes": []}
    if r.rc != 0:
        if "no server running" in r.err or "No such file" in r.err or "error connecting" in r.err:
            return {"available": True, "sessions": [], "panes": [], "note": "no tmux server running"}
        return {"available": False, "error": r.err.strip()[:200], "sessions": [], "panes": []}
    sessions = []
    for line in r.out.splitlines():
        p = line.split(TMUX_SEP)
        if len(p) < 7:
            continue
        sessions.append({"name": p[0], "attached": int(p[1] or 0), "activity": _int(p[2]),
                         "created": _int(p[3]), "is_control_center": p[4] == "1",
                         "icn_session_managed": p[5] == "1", "icn_session_purpose": p[6] or None})
    pfmt = TMUX_SEP.join(["#{session_name}", "#{pane_pid}", "#{pane_current_path}", "#{pane_tty}",
                          "#{pane_current_command}"])
    r2 = run(["tmux", "list-panes", "-a", "-F", pfmt], timeout=5)
    panes = []
    for line in r2.out.splitlines():
        p = line.split(TMUX_SEP)
        if len(p) < 5:
            continue
        panes.append({"session": p[0], "pane_pid": _int(p[1]), "path": p[2], "tty": p[3], "command": p[4]})
    return {"available": True, "sessions": sessions, "panes": panes}


def _int(s) -> Optional[int]:
    try:
        return int(s)
    except (TypeError, ValueError):
        return None


def claude_session_files(paths: Paths) -> list[dict]:
    """Claude Code's own per-process session files (~/.claude/sessions/<pid>.json).

    Only the .json files are read; any sibling key material is never opened.
    Each record is validated against the live process table (pid + start time).
    """
    out = []
    for f in sorted(glob.glob(os.path.join(paths.claude_home, "sessions", "*.json"))):
        try:
            with open(f) as fh:
                d = json.load(fh)
        except (OSError, ValueError):
            continue
        if not isinstance(d, dict):
            continue
        pid = d.get("pid")
        rec = {k: d.get(k) for k in ("pid", "sessionId", "cwd", "procStart", "name", "status",
                                      "statusUpdatedAt", "updatedAt", "startedAt", "tmux", "kind", "version")}
        rec["file"] = f
        rec["process_alive"] = same_process(pid, d.get("procStart"), paths.proc)
        out.append(rec)
    return out


def transcript_activity(paths: Paths, cwd: Optional[str], session_id: Optional[str] = None) -> Optional[float]:
    """mtime of the Claude transcript for (cwd, session) — the last time the agent wrote."""
    if not cwd:
        return None
    slug = cwd.replace("/", "-").replace(".", "-")
    d = os.path.join(paths.claude_home, "projects", slug)
    try:
        if session_id:
            p = os.path.join(d, f"{session_id}.jsonl")
            return os.path.getmtime(p) if os.path.exists(p) else None
        files = glob.glob(os.path.join(d, "*.jsonl"))
        return max((os.path.getmtime(x) for x in files), default=None)
    except OSError:
        return None


def build_processes(procs: list[dict]) -> list[dict]:
    """Running build-ish processes, grouped by cwd (top-level only: parent isn't a build too)."""
    by_pid = {p["pid"]: p for p in procs}
    out = []
    for p in procs:
        if p["comm"] not in ("cargo", "cargo-nextest", "rustc"):
            continue
        parent = by_pid.get(p["ppid"])
        if parent and parent["comm"] in ("cargo", "cargo-nextest", "rustc"):
            continue
        out.append({"pid": p["pid"], "comm": p["comm"], "cwd": p["cwd"], "start": p["start"]})
    return out
