"""Readers for existing host coordination records. All strictly read-only.

- icn-ops session registry (SQLite, opened with mode=ro; no migration, no new DB)
- icn-session launcher records (~/.local/state/icn-session/*.json)
- ~/icn-dev/state/active-worktrees.yaml and file-locks.yaml (host allocation/lock maps)
- build admission reservations (~/icn-dev/state/admit/reservations)
- disk-guard and MCP-live status files
- handoff documents
"""

from __future__ import annotations

import glob
import json
import os
import re
import sqlite3
from .util import Paths, parse_iso

try:  # PyYAML is optional; without it the YAML-backed records show as unavailable.
    import yaml  # type: ignore
except ImportError:  # pragma: no cover - exercised by test via monkeypatch
    yaml = None


class SourceUnavailable(Exception):
    """A source exists conceptually but cannot be read right now."""


# --------------------------------------------------------------------------- registry
def registry_sessions(db_path: str) -> dict:
    """Read the icn-ops session registry read-only. Raises SourceUnavailable if absent."""
    if not os.path.exists(db_path):
        raise SourceUnavailable(f"registry database not found at {db_path}")
    uri = f"file:{db_path}?mode=ro"
    con = sqlite3.connect(uri, uri=True, timeout=2.0)
    try:
        con.row_factory = sqlite3.Row
        cols = {r[1] for r in con.execute("PRAGMA table_info(sessions)")}
        if not cols:
            raise SourceUnavailable("registry has no sessions table")
        want = ["id", "repo", "worktree", "task_description", "started_at", "last_heartbeat",
                "worktree_path", "worktree_name", "provider_session_id", "branch_at_registration",
                "task_ref", "pr_ref", "provider", "agent_pid", "current_activity", "last_progress",
                "progress_count", "worktree_generation", "host"]
        sel = ", ".join(c if c in cols else f"NULL AS {c}" for c in want)
        rows = [dict(r) for r in con.execute(f"SELECT {sel} FROM sessions ORDER BY last_heartbeat DESC")]
        claims = []
        if con.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name='file_claims'").fetchone():
            claims = [dict(r) for r in con.execute("SELECT file_path, session_id, claimed_at FROM file_claims")]
        for r in rows:
            r["last_heartbeat_ts"] = parse_iso(r.get("last_heartbeat"))
            r["started_ts"] = parse_iso(r.get("started_at"))
            r["claims"] = sorted(c["file_path"] for c in claims if c["session_id"] == r["id"])
        return {"db": db_path, "sessions": rows, "claim_count": len(claims)}
    finally:
        con.close()


# --------------------------------------------------------------------------- icn-session
def icn_session_records(state_dir: str) -> list[dict]:
    if not os.path.isdir(state_dir):
        raise SourceUnavailable(f"icn-session state dir not found: {state_dir}")
    out = []
    for f in sorted(glob.glob(os.path.join(state_dir, "*.json"))):
        try:
            with open(f) as fh:
                d = json.load(fh)
        except (OSError, ValueError):
            out.append({"file": f, "unreadable": True, "name": os.path.basename(f)[:-5]})
            continue
        if isinstance(d, dict):
            d["file"] = f
            d["updated_ts"] = parse_iso(d.get("updated_at"))
            out.append(d)
    return out


# --------------------------------------------------------------------------- YAML records
def _load_yaml(path: str):
    if yaml is None:
        raise SourceUnavailable("PyYAML not installed; cannot read " + os.path.basename(path))
    if not os.path.exists(path):
        raise SourceUnavailable(f"{path} not found")
    with open(path) as f:
        return yaml.safe_load(f)


HANDOFF_REF = re.compile(r"(~/icn-dev/(?:handoffs|state|private)/[^\s\"'`),;]+)")


def active_worktrees(paths: Paths) -> dict:
    """Rows of active-worktrees.yaml keyed by absolute path. Text is historical claim, not truth."""
    path = os.path.join(paths.state, "active-worktrees.yaml")
    d = _load_yaml(path) or {}
    rows = {}
    for w in d.get("worktrees") or []:
        if not isinstance(w, dict) or not w.get("path"):
            continue
        p = w["path"]
        absp = p if os.path.isabs(p) else os.path.join(paths.icn_dev, p)
        text = " ".join(str(w.get(k) or "") for k in ("task", "status"))
        rows[os.path.normpath(absp)] = {
            "repo": w.get("repo"), "branch": w.get("branch"), "task": str(w.get("task") or ""),
            "status": str(w.get("status") or ""), "created": str(w.get("created") or ""),
            "refs": sorted(set(HANDOFF_REF.findall(text))),
        }
    return {"file": path, "last_updated": str(d.get("last_updated") or ""), "rows": rows,
            "mtime": os.path.getmtime(path)}


def file_locks(paths: Paths) -> dict:
    path = os.path.join(paths.state, "file-locks.yaml")
    d = _load_yaml(path) or {}
    locks = []
    for lk in d.get("locks") or []:
        if not isinstance(lk, dict):
            continue
        locks.append({"repo": lk.get("repo"), "owner": str(lk.get("owner") or ""),
                      "status": str(lk.get("status") or "unknown"),
                      "files": [str(x) for x in (lk.get("files") or [])],
                      "notes": str(lk.get("notes") or "")[:400]})
    return {"file": path, "last_updated": str(d.get("last_updated") or ""), "locks": locks,
            "mtime": os.path.getmtime(path)}


def open_lock_conflicts(locks: list[dict]) -> list[dict]:
    """Files claimed by more than one *open* lock owner in the same repo."""
    by_file: dict = {}
    for lk in locks:
        if lk["status"] != "open":
            continue
        for f in lk["files"]:
            by_file.setdefault((lk.get("repo"), f), set()).add(lk["owner"])
    return [{"repo": r, "file": f, "owners": sorted(o)} for (r, f), o in sorted(by_file.items(), key=lambda x: (str(x[0][0]), x[0][1])) if len(o) > 1]


# --------------------------------------------------------------------------- build admission
def build_reservations(paths: Paths) -> dict:
    d = os.path.join(paths.state, "admit")
    if not os.path.isdir(d):
        raise SourceUnavailable("build admission state dir not found")
    res = []
    rdir = os.path.join(d, "reservations")
    for f in sorted(glob.glob(os.path.join(rdir, "*"))):
        if os.path.basename(f).startswith(".") or os.path.islink(f) or not os.path.isfile(f):
            continue
        rec = {}
        try:
            with open(f) as fh:
                for line in fh:
                    if "=" in line:
                        k, v = line.rstrip("\n").split("=", 1)
                        rec[k] = v
        except OSError:
            continue
        rec["id"] = os.path.basename(f)
        res.append(rec)
    mode = None
    try:
        with open(os.path.join(d, "mode")) as fh:
            mode = fh.readline().strip() or None
    except OSError:
        # best-effort: an unreadable optional record leaves this field empty, shown as unknown
        pass
    last = []
    try:
        with open(os.path.join(d, "admit.log"), "rb") as fh:
            fh.seek(0, 2)
            size = fh.tell()
            fh.seek(max(0, size - 4096))
            last = fh.read().decode(errors="replace").splitlines()[-6:]
    except OSError:
        # best-effort: an unreadable optional record leaves this field empty, shown as unknown
        pass
    return {"reservations": res, "mode": mode, "recent_log": last}


def json_status_file(path: str) -> dict:
    if not os.path.exists(path):
        raise SourceUnavailable(f"{path} not found")
    with open(path) as f:
        d = json.load(f)
    return {"file": path, "mtime": os.path.getmtime(path), "data": d}


# --------------------------------------------------------------------------- handoffs
SECTION_KEYS = {
    "done": re.compile(r"(?i)\b(done|completed?|results?|what changed|delivered|landed|verified)\b"),
    "open": re.compile(r"(?i)\b(unfinished|remaining|open|not done|gaps?|limitations?|follow[- ]?ups?|blockers?)\b"),
    "decision": re.compile(r"(?i)\b(decisions?|maintainer|authori[sz]ation|choose|choice)\b"),
    "next": re.compile(r"(?i)\b(next|resume|smallest)\b"),
}


def list_handoffs(paths: Paths, limit: int = 60) -> list[dict]:
    """Recent handoff documents: ~/icn-dev/handoffs/*.md plus HANDOFF.md in state dirs."""
    cands = glob.glob(os.path.join(paths.handoffs, "*.md"))
    cands += glob.glob(os.path.join(paths.state, "*", "HANDOFF.md"))
    cands += glob.glob(os.path.join(paths.state, "*", "*handoff*.md"))
    out = []
    for f in set(cands):
        try:
            st = os.stat(f)
        except OSError:
            continue
        out.append({"path": f, "mtime": st.st_mtime, "size": st.st_size, "title": _title(f)})
    out.sort(key=lambda x: -x["mtime"])
    return out[:limit]


def _title(path: str) -> str:
    try:
        with open(path, errors="replace") as f:
            for i, line in enumerate(f):
                if line.startswith("# "):
                    return line[2:].strip()[:140]
                if i > 20:
                    break
    except OSError:
        # best-effort: an unreadable optional record leaves this field empty, shown as unknown
        pass
    return os.path.basename(path)


def summarize_handoff(path: str, max_lines: int = 8) -> dict:
    """Pull headings that look like done / open / decision / next, with their first lines.

    This is navigation, not interpretation: the original file is always one key away.
    """
    try:
        with open(path, errors="replace") as f:
            text = f.read(400_000)
    except OSError as exc:
        raise SourceUnavailable(str(exc))
    sections: dict = {k: [] for k in SECTION_KEYS}
    cur_key, cur_head = None, None
    for line in text.splitlines():
        m = re.match(r"^(#{2,4})\s+(.*)", line)
        if m:
            head = m.group(2).strip()
            cur_key = next((k for k, rx in SECTION_KEYS.items() if rx.search(head)), None)
            cur_head = head
            if cur_key:
                sections[cur_key].append({"heading": head, "lines": []})
            continue
        if cur_key and line.strip() and cur_head:
            blk = sections[cur_key][-1]
            if len(blk["lines"]) < max_lines:
                blk["lines"].append(line.rstrip()[:220])
    return {"path": path, "title": _title(path), "sections": sections}
