"""Git worktree discovery and per-worktree status. Read-only: no fetch, no optional locks.

Identity of a worktree is (bare store, absolute path) as reported by `git worktree list`.
A basename or branch name is never treated as a globally unique id.
"""

from __future__ import annotations

import glob
import os
from concurrent.futures import ThreadPoolExecutor
from typing import Optional

from .util import Paths, run


def bare_stores(paths: Paths) -> list[str]:
    return sorted(glob.glob(os.path.join(paths.repos, "*.git")))


def repo_key(bare: str) -> str:
    return os.path.basename(bare)[:-4] if bare.endswith(".git") else os.path.basename(bare)


def parse_worktree_porcelain(text: str) -> list[dict]:
    out, cur = [], {}
    for line in text.splitlines() + [""]:
        if not line.strip():
            if cur:
                out.append(cur)
            cur = {}
            continue
        k, _, v = line.partition(" ")
        if k == "worktree":
            cur["path"] = v
        elif k == "HEAD":
            cur["head"] = v
        elif k == "branch":
            cur["branch"] = v.removeprefix("refs/heads/")
        elif k == "detached":
            cur["detached"] = True
        elif k == "bare":
            cur["bare"] = True
        elif k == "locked":
            cur["locked"] = v or True
        elif k == "prunable":
            cur["prunable"] = v or True
    return out


def list_worktrees(bare: str) -> list[dict]:
    r = run(["git", "--git-dir", bare, "worktree", "list", "--porcelain"], timeout=20)
    if r.rc != 0:
        raise RuntimeError(f"git worktree list failed for {repo_key(bare)}: {r.err.strip()[:200]}")
    rows = [w for w in parse_worktree_porcelain(r.out) if not w.get("bare")]
    for w in rows:
        w["repo"] = repo_key(bare)
        w["bare"] = bare
    return rows


def remote_freshness(bare: str) -> Optional[float]:
    """When were remote-tracking refs last updated? (FETCH_HEAD mtime, else packed-refs)."""
    for name in ("FETCH_HEAD", "packed-refs"):
        p = os.path.join(bare, name)
        if os.path.exists(p):
            return os.path.getmtime(p)
    return None


def parse_status_v2(text: str) -> dict:
    """Parse `git status --porcelain=v2 --branch` into counts. Paths are not retained."""
    st = {"upstream": None, "ahead": None, "behind": None, "modified": 0, "untracked": 0,
          "conflicted": 0, "oid": None}
    for line in text.splitlines():
        if line.startswith("# branch.oid "):
            st["oid"] = line.split()[2]
        elif line.startswith("# branch.upstream "):
            st["upstream"] = line.split(" ", 2)[2]
        elif line.startswith("# branch.ab "):
            a, b = line.split()[2:4]
            st["ahead"], st["behind"] = int(a), -int(b)
        elif line.startswith(("1 ", "2 ")):
            st["modified"] += 1
        elif line.startswith("u "):
            st["conflicted"] += 1
        elif line.startswith("? "):
            st["untracked"] += 1
    st["dirty"] = (st["modified"] + st["untracked"] + st["conflicted"]) > 0
    return st


def worktree_status(w: dict, timeout: float = 30.0) -> dict:
    p = w["path"]
    if not os.path.isdir(p):
        return {"exists": False}
    r = run(["git", "-C", p, "--no-optional-locks", "status", "--porcelain=v2", "--branch"], timeout=timeout)
    if r.rc != 0:
        return {"exists": True, "error": (r.err.strip() or f"rc={r.rc}")[:200]}
    st = parse_status_v2(r.out)
    st["exists"] = True
    # Commits reachable from HEAD that are on no remote-tracking ref (cached refs; no fetch).
    r2 = run(["git", "-C", p, "rev-list", "--count", "HEAD", "--not", "--remotes"], timeout=timeout)
    st["local_only_commits"] = int(r2.out.strip()) if r2.rc == 0 and r2.out.strip().isdigit() else None
    try:
        st["mtime"] = os.path.getmtime(os.path.join(p, ".git"))
    except OSError:
        st["mtime"] = None
    return st


def collect(paths: Paths, workers: int = 4, with_status: bool = True) -> dict:
    stores = bare_stores(paths)
    if not stores:
        raise RuntimeError(f"no bare stores under {paths.repos}")
    all_wt, errors, fresh = [], [], {}
    for b in stores:
        try:
            all_wt.extend(list_worktrees(b))
        except RuntimeError as exc:
            errors.append(str(exc))
        fresh[repo_key(b)] = remote_freshness(b)
    if with_status:
        with ThreadPoolExecutor(max_workers=workers) as ex:
            for w, st in zip(all_wt, ex.map(worktree_status, all_wt)):
                w["status"] = st
    return {"worktrees": all_wt, "errors": errors, "remote_refs_updated": fresh,
            "stores": [repo_key(b) for b in stores]}


def toplevel_for(path: Optional[str], worktrees: list[dict]) -> Optional[dict]:
    """The worktree containing `path` (longest-prefix match on known worktree paths)."""
    if not path:
        return None
    best = None
    for w in worktrees:
        wp = w["path"].rstrip("/")
        if path == wp or path.startswith(wp + "/"):
            if best is None or len(wp) > len(best["path"]):
                best = w
    return best
