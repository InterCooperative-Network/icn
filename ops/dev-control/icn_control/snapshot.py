"""Collect observations in tiers and keep a non-authoritative local cache.

Tiers (default intervals, seconds):
  fast    20   processes, tmux, Claude session files, registry, launcher records, host health
  records 60   host YAML records, handoff index
  git     300  git worktree list + status of every worktree (no fetch, no optional locks)
  github  300  open PRs, recent merges, required checks, tracking issues

The cache exists only so a restart can show *something* immediately. Anything loaded
from it is flagged from_cache and rendered as STALE until a live observation replaces it.
"""

from __future__ import annotations

import json
import os
import tempfile
import threading
import time
from typing import Callable, Optional

from . import github as gh
from . import gitwt, health, procs, records
from .util import Obs, Paths, observe

OWNER = "InterCooperative-Network"
CACHE_VERSION = 1
TIERS = {"fast": 20, "records": 60, "git": 300, "github": 300, "history": 3600}
CACHED_KEYS = ("github_prs", "github_merged", "required_checks", "issues", "worktrees", "github_history")


def repo_remotes(paths: Paths) -> dict:
    """GitHub repo name for each local bare store, from the store's own origin URL."""
    out = {}
    for b in gitwt.bare_stores(paths):
        key = gitwt.repo_key(b)
        try:
            with open(os.path.join(b, "config")) as f:
                cfg = f.read()
        except OSError:
            continue
        import re
        m = re.search(r'\[remote "origin"\][^\[]*?url\s*=\s*(\S+)', cfg, re.S)
        if not m:
            continue
        mm = re.search(r"github\.com[:/]([^/]+)/([^/\s]+?)(?:\.git)?$", m.group(1))
        if mm and mm.group(1) == OWNER:
            out[key] = mm.group(2)
    return out


def collect_fast(paths: Paths) -> dict:
    snap = {}
    snap["processes"] = observe("process table", procs.scan_processes, paths.proc)
    snap["tmux"] = observe("tmux", procs.tmux_state)
    snap["claude_sessions"] = observe("~/.claude/sessions", procs.claude_session_files, paths)
    snap["registry"] = observe("icn-ops registry (read-only)", records.registry_sessions, paths.registry_db())
    snap["icn_session"] = observe("icn-session records", records.icn_session_records, paths.icn_session_state)
    snap["fs"] = observe("filesystems", health.filesystems)
    snap["mem"] = observe("memory", health.memory, paths.proc)
    snap["load"] = observe("load", health.load, paths.proc)
    snap["reservations"] = observe("build admission", records.build_reservations, paths)
    snap["disk_guard"] = observe("disk guard status", records.json_status_file,
                                 os.path.join(paths.state, "disk-guard-status.json"))
    snap["mcp_live"] = observe("icn-ops MCP live status", records.json_status_file,
                               os.path.join(paths.state, "mcp-live-status.json"))
    if snap["processes"].ok:
        snap["builds"] = Obs("build processes", snap["processes"].observed_at, True,
                             procs.build_processes(snap["processes"].data))
    return snap


def collect_records(paths: Paths) -> dict:
    return {
        "active_worktrees": observe("active-worktrees.yaml", records.active_worktrees, paths),
        "file_locks": observe("file-locks.yaml", records.file_locks, paths),
        "handoffs": observe("handoff index", records.list_handoffs, paths),
    }


def collect_git(paths: Paths) -> dict:
    return {"worktrees": observe("git worktrees", gitwt.collect, paths)}


def collect_history(paths: Paths) -> dict:
    """Merged / closed PR history keyed by local repo key, for classifying old worktrees. Hourly."""
    remotes = repo_remotes(paths)

    def hist():
        out, errs = {}, []
        for key, name in sorted(remotes.items()):
            try:
                out[key] = gh.fetch_pr_history(OWNER, name)
            except gh.GhUnavailable as exc:
                errs.append(f"{name}: {exc}")
        if errs:
            raise gh.GhUnavailable("; ".join(errs)[:400])  # partial history could misclassify: all or nothing
        return out
    return {"github_history": observe("GitHub PR history (merged/closed)", hist)}


def collect_github(paths: Paths, catalog: Optional[dict] = None) -> dict:
    remotes = repo_remotes(paths)
    snap = {}

    def prs():
        out, errs = {}, []
        for key, name in sorted(remotes.items()):
            try:
                out[key] = gh.fetch_open_prs(OWNER, name)
            except gh.GhUnavailable as exc:
                errs.append(f"{name}: {exc}")
        if errs and not out:
            raise gh.GhUnavailable("; ".join(errs))
        if errs:
            out["_errors"] = errs  # partial result is flagged, not silently dropped
        return out

    o = observe("GitHub open PRs", prs)
    if o.ok and isinstance(o.data, dict) and "_errors" in o.data:
        o.error = "partial: " + "; ".join(o.data.pop("_errors"))
    snap["github_prs"] = o
    snap["github_merged"] = observe("GitHub merged PRs (7d)", gh.fetch_recent_merged, OWNER,
                                    [remotes[k] for k in sorted(remotes)], 7)

    def required():
        policy = _policy_required()
        out = {}
        for key, name in remotes.items():
            live = gh.branch_required_checks(OWNER, name)
            if live is not None:
                out[key] = {"checks": live, "source": "live branch protection"}
            elif key == "icn" and policy:
                out[key] = {"checks": policy, "source": "ops/state/truth/policy.json (branch protection not readable)"}
            else:
                out[key] = {"checks": None, "source": "unknown"}
        return out

    snap["required_checks"] = observe("required checks", required)

    def issues():
        out = {}
        for p in (catalog or {}).get("programs", []):
            for t in p.get("tracking_issues") or []:
                name = remotes.get(t["repo"], t["repo"])
                try:
                    out[f"{t['repo']}#{t['number']}"] = gh.fetch_issue(OWNER, name, t["number"])
                except gh.GhUnavailable as exc:
                    out[f"{t['repo']}#{t['number']}"] = {"error": str(exc)}
        return out

    snap["issues"] = observe("tracking issues", issues)
    return snap


def merge_policy() -> Optional[dict]:
    """The `merge` object of ops/state/truth/policy.json (the merge-policy owner), or None if unreadable."""
    here = os.path.dirname(os.path.abspath(__file__))
    cands = [os.path.join(here, "..", "..", "..", "ops", "state", "truth", "policy.json"),  # repo checkout
             os.path.join(here, "..", "policy.json")]  # installed release (snapshot taken at install)
    if os.environ.get("ICN_CONTROL_REPO"):
        cands.insert(0, os.path.join(os.environ["ICN_CONTROL_REPO"], "ops", "state", "truth", "policy.json"))
    for p in cands:
        try:
            with open(p) as f:
                merge = json.load(f)["merge"]
        except (OSError, KeyError, ValueError, TypeError):
            continue
        if isinstance(merge, dict):
            return merge
    return None


def _policy_required() -> Optional[list]:
    return (merge_policy() or {}).get("required_checks")


# --------------------------------------------------------------------------- cache
def cache_path(paths: Paths) -> str:
    return os.path.join(paths.control_state, "last-snapshot.json")


def save_cache(paths: Paths, snap: dict) -> None:
    """Atomic, 0600, user-private. Only slow/remote sources are cached."""
    os.makedirs(paths.control_state, mode=0o700, exist_ok=True)
    data = {"version": CACHE_VERSION, "saved_at": time.time(),
            "obs": {k: snap[k].to_json() for k in CACHED_KEYS if k in snap and snap[k].ok and not snap[k].from_cache}}
    if not data["obs"]:
        return
    try:
        old = load_cache_raw(paths)
        for k, v in (old.get("obs") or {}).items():
            data["obs"].setdefault(k, v)
    except Exception:
        # best-effort: the cache is a convenience; failure to merge/save/clean it is non-fatal
        pass
    fd, tmp = tempfile.mkstemp(dir=paths.control_state, prefix=".snap.")
    try:
        with os.fdopen(fd, "w") as f:
            json.dump(data, f)
        os.chmod(tmp, 0o600)
        os.replace(tmp, cache_path(paths))
    except Exception:
        try:
            os.unlink(tmp)
        except OSError:
            # best-effort: the cache is a convenience; failure to merge/save/clean it is non-fatal
            pass
        raise


def load_cache_raw(paths: Paths) -> dict:
    with open(cache_path(paths)) as f:
        d = json.load(f)
    if d.get("version") != CACHE_VERSION:
        return {}
    return d


def load_cache(paths: Paths) -> dict:
    try:
        d = load_cache_raw(paths)
    except (OSError, ValueError):
        return {}
    return {k: Obs.from_json(v) for k, v in (d.get("obs") or {}).items()}


# --------------------------------------------------------------------------- background refresher
class Collector:
    """Owns the current snapshot; refreshes tiers on a background thread."""

    def __init__(self, paths: Paths, catalog: dict, intervals: Optional[dict] = None,
                 on_update: Optional[Callable[[], None]] = None, use_cache: bool = True):
        self.paths, self.catalog = paths, catalog
        self.intervals = dict(TIERS, **(intervals or {}))
        self.snap: dict = load_cache(paths) if use_cache else {}
        self.lock = threading.Lock()
        self.last_run = {t: 0.0 for t in TIERS}
        self.running = {t: False for t in TIERS}
        self.force = threading.Event()
        self.stop = threading.Event()
        self.on_update = on_update
        self.version = 0
        self._forced: set = set()

    def _run_tier(self, tier: str) -> None:
        fn = {"fast": collect_fast, "records": collect_records, "git": collect_git,
              "github": lambda p: collect_github(p, self.catalog), "history": collect_history}[tier]
        self.running[tier] = True
        try:
            res = fn(self.paths)
        finally:
            self.running[tier] = False
        with self.lock:
            for k, o in res.items():
                prev = self.snap.get(k)
                if not o.ok and prev is not None and prev.ok:
                    # Keep the last good data visible, explicitly marked stale, plus the failure.
                    stale = Obs(prev.source, prev.observed_at, True, prev.data, error=f"refresh failed: {o.error}",
                                from_cache=True, duration_s=prev.duration_s)
                    self.snap[k] = stale
                else:
                    self.snap[k] = o
            self.last_run[tier] = time.time()
            self.version += 1
        if tier in ("github", "git", "history"):
            try:
                save_cache(self.paths, self.snap)
            except Exception:
                # best-effort: the cache is a convenience; failure to merge/save/clean it is non-fatal
                pass
        if self.on_update:
            self.on_update()

    def refresh_all(self) -> None:
        self._forced = set(TIERS)
        self.force.set()

    def run_once(self, tiers=("fast", "records", "git", "github")) -> None:
        threads = [threading.Thread(target=self._run_tier, args=(t,), daemon=True) for t in tiers]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

    def loop(self) -> None:
        workers: dict = {}
        while not self.stop.is_set():
            now = time.time()
            forced = self._forced
            self._forced = set()
            for tier, iv in self.intervals.items():
                due = tier in forced or now - self.last_run[tier] >= iv
                busy = workers.get(tier) is not None and workers[tier].is_alive()
                if due and not busy:
                    th = threading.Thread(target=self._run_tier, args=(tier,), daemon=True)
                    workers[tier] = th
                    self.last_run[tier] = now  # avoid re-launch while running
                    th.start()
            self.force.wait(1.0)
            self.force.clear()

    def start(self) -> threading.Thread:
        th = threading.Thread(target=self.loop, daemon=True)
        th.start()
        return th

    def snapshot(self) -> dict:
        with self.lock:
            return dict(self.snap)
