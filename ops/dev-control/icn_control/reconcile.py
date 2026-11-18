"""Reconcile observations from independent owners into views the maintainer can read.

Rules that keep this honest:
- A process is "live" only if observed in the process table now (pid + start time).
- A registry row, launcher record, worktree directory or tmux session is NOT a live agent.
- Missing data yields UNKNOWN, never success.
- Every derived claim carries the sources it was derived from.
"""

from __future__ import annotations

import json
import os
import re
import time
from typing import Optional

from . import github as gh
from .gitwt import toplevel_for
from .procs import AGENT_COMMS, ancestors, file_activity, same_process, start_epoch, transcript_activity
from .snapshot import merge_policy
from .util import (AWAITING_MAINTAINER, BLOCKED, IDLE, IN_PROGRESS, LIVE, REGISTERED_NOT_RUNNING,
                   STALE, UNKNOWN, WARNING, Obs, Paths, ago, short)

REGISTRY_STALE_S = 2 * 3600
WORKING_WINDOW_S = 5 * 60
PR_DIR = re.compile(r"(?i)(?:^|[^a-z0-9])(?:pr|review)-?(\d{3,6})(?:\D|$)")


def _data(snap: dict, key: str, default=None):
    o: Optional[Obs] = snap.get(key)
    return o.data if (o is not None and o.ok and o.data is not None) else default


# --------------------------------------------------------------------------- programs
def load_catalog(path: Optional[str] = None) -> dict:
    path = path or os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "programs.json")
    with open(path) as f:
        return json.load(f)


def _kw_match(text: str, kw: str) -> bool:
    return re.search(r"(?<![a-z0-9])" + re.escape(kw.lower()) + r"(?![a-z0-9])", text) is not None


def classify(repo: Optional[str], text: str, catalog: dict) -> tuple[str, str]:
    """(program_id, rule description). Deterministic: repo rules, then keyword priority, then default."""
    progs = {p["id"]: p for p in catalog["programs"]}
    for p in catalog["programs"]:
        if repo and repo in (p.get("repos") or []):
            return p["id"], f"repo '{repo}'"
    low = f"{repo or ''} {text or ''}".lower()
    for pid in catalog.get("keyword_priority", []):
        for kw in progs[pid].get("keywords") or []:
            if _kw_match(low, kw):
                return pid, f"keyword '{kw}'"
    for p in catalog["programs"]:
        if repo and repo in (p.get("default_for_repos") or []):
            return p["id"], f"default for repo '{repo}'"
    return "unclassified", "no rule matched"


# --------------------------------------------------------------------------- PRs
def pr_views(snap: dict, paths: Paths, catalog: dict) -> list[dict]:
    gh_obs = snap.get("github_prs")
    if gh_obs is None or not gh_obs.ok:
        return []
    required = _data(snap, "required_checks", {})
    ready_when = (merge_policy() or {}).get("ready_when")
    out = []
    for repo, prs in (gh_obs.data or {}).items():
        req_list = required.get(repo) if isinstance(required, dict) else None
        if isinstance(req_list, dict):
            req_list = req_list.get("checks")
        for pr in prs:
            req = gh.required_check_status(pr, req_list)
            ready = gh.merge_readiness(pr, req, ready_when)
            evidence = review_evidence_dirs(paths, pr["number"])
            v = dict(pr)
            v["required"] = req
            v["readiness"] = ready
            v["evidence_dirs"] = evidence
            v["program"], v["program_rule"] = classify(repo, f"{pr['title']} {pr['branch']}", catalog)
            v["status"], v["status_reason"] = pr_status(v)
            out.append(v)
    out.sort(key=lambda p: -(p.get("updated_ts") or 0))
    return out


def review_evidence_dirs(paths: Paths, number: int) -> list[str]:
    try:
        names = os.listdir(paths.state)
    except OSError:
        return []
    out = []
    for n in names:
        m = PR_DIR.search(n)
        if m and int(m.group(1)) == number and os.path.isdir(os.path.join(paths.state, n)):
            out.append(os.path.join(paths.state, n))
    return sorted(out)


def latest_review_generation(pr: dict) -> Optional[dict]:
    """Most recent review that declares a FULL/DELTA generation or a findings line."""
    for r in reversed(pr.get("reviews") or []):
        if r.get("generation") or r.get("findings"):
            return r
    return None


def pr_status(pr: dict) -> tuple[str, str]:
    if pr.get("lifecycle_untrusted"):
        return UNKNOWN, (f"lifecycle block ignored: PR author association is {pr.get('author_association') or 'unknown'},"
                         " not a repository member")
    lc = pr.get("lifecycle") or {}
    state = lc.get("state")
    rv = latest_review_generation(pr)
    if state == "FROZEN":
        match = gh.freeze_matches_head(pr)
        if match is False:
            return STALE, "lifecycle says FROZEN but the head moved after freeze"
        led = f", follow-up ledger #{lc['ledger']}" if lc.get("ledger") else ""
        return AWAITING_MAINTAINER, f"FROZEN at {short(lc.get('freeze_head'))}{led}: maintainer delivery decision"
    if pr["required"]["verdict"] == "FAILING":
        idle_days = (time.time() - (pr.get("updated_ts") or 0)) / 86400
        why = "required CI failing: " + ", ".join(pr["required"]["failing"][:4])
        if idle_days > 30:
            return STALE, f"{why}; untouched for {int(idle_days)}d"
        return BLOCKED, why
    if rv and state in (None, "IMPLEMENTING", "REVIEWING"):
        f = rv.get("findings") or {}
        at = short(rv.get("commit"))
        stale_note = "" if (rv.get("commit") and pr.get("head") and rv["commit"] == pr["head"]) else " (review is on an older head)"
        counts = (f"{f.get('blocker', '?')} BLOCKER / {f.get('follow_up', '?')} FOLLOW_UP" if f else "findings not parsed")
        return AWAITING_MAINTAINER if f.get("blocker") == 0 else BLOCKED, \
            f"{rv.get('generation') or 'review'} at {at}{stale_note}: {counts}; lifecycle block still {state or 'absent'} - author disposition pending"
    if state in ("FIXING", "VERIFYING", "REVIEWING", "IMPLEMENTING"):
        return IN_PROGRESS, f"lifecycle {state}"
    if state == "MERGING":
        return AWAITING_MAINTAINER, "lifecycle MERGING"
    if pr.get("draft"):
        return IN_PROGRESS if (time.time() - (pr.get("updated_ts") or 0)) < 14 * 86400 else STALE, \
            "draft, no lifecycle block" + ("" if (time.time() - (pr.get("updated_ts") or 0)) < 14 * 86400 else ", untouched >14d")
    return UNKNOWN, "open, no lifecycle block"


def branch_pr(prs: list, repo: Optional[str], branch: Optional[str]) -> Optional[dict]:
    """The PR whose head is this local branch. Fork PRs never match by name (a fork can reuse any
    branch name); associating one needs an explicit registry pr_ref."""
    if not branch:
        return None
    return next((p for p in prs if p["repo"] == repo and p["branch"] == branch and p.get("cross_repo") is False), None)


# --------------------------------------------------------------------------- sessions
def session_views(snap: dict, paths: Paths) -> list[dict]:
    now = time.time()
    procs = _data(snap, "processes", []) or []
    by_pid = {p["pid"]: p for p in procs}
    tm = _data(snap, "tmux", {}) or {}
    panes = tm.get("panes") or []
    pane_by_pid = {p["pane_pid"]: p for p in panes if p.get("pane_pid")}
    tmux_sessions = {s["name"]: s for s in tm.get("sessions") or []}
    csf = {c["pid"]: c for c in (_data(snap, "claude_sessions", []) or []) if c.get("process_alive")}
    reg = (_data(snap, "registry", {}) or {}).get("sessions") or []
    recs = _data(snap, "icn_session", []) or []
    wts = (_data(snap, "worktrees", {}) or {}).get("worktrees") or []
    rows = (_data(snap, "active_worktrees", {}) or {}).get("rows") or {}
    locks = (_data(snap, "file_locks", {}) or {}).get("locks") or []
    prs = snap.get("_pr_views") or []
    handoffs = _data(snap, "handoffs", []) or []

    views, used_reg, used_rec, tmux_with_agent = [], set(), set(), set()

    agents = [p for p in procs if p["comm"] in AGENT_COMMS
              and not any(by_pid.get(a, {}).get("comm") in AGENT_COMMS for a in ancestors(p["pid"], by_pid))]
    for p in agents:
        c = csf.get(p["pid"]) or {}
        cwd = c.get("cwd") or p.get("cwd")
        tmux_name = (c.get("tmux") or "").split(":")[0] or None
        if not tmux_name:
            for a in [p["pid"]] + ancestors(p["pid"], by_pid):
                if a in pane_by_pid:
                    tmux_name = pane_by_pid[a]["session"]
                    break
        if tmux_name:
            tmux_with_agent.add(tmux_name)
        wt = toplevel_for(cwd, wts)
        r = next((x for x in reg if c.get("sessionId") and x.get("provider_session_id") == c.get("sessionId")), None)
        if r:
            used_reg.add(r["id"])
        rec = next((x for x in recs if c.get("sessionId") and x.get("claude_session_id") == c.get("sessionId")), None)
        if rec is None and wt:
            rec = next((x for x in recs if x.get("worktree") == wt["path"] and
                        same_process(x.get("fg_pid"), x.get("fg_start"), paths.proc)), None)
        if rec is None and tmux_name:
            # icn-session names its tmux session after the record; the session is the recorded one.
            rec = next((x for x in recs if x.get("launch_mode") == "tmux" and x.get("name") == tmux_name), None)
        if rec:
            used_rec.add(rec.get("file"))
        status = c.get("status")
        st_ts = (c.get("statusUpdatedAt") or 0) / 1000 or None
        # Activity evidence is per agent type: Claude's own status file and transcript, or the
        # transcript files the process holds open. Never another tool's transcripts.
        tr_ts = transcript_activity(paths, cwd, c.get("sessionId")) if (p["comm"] == "claude" and c) else None
        fd_ts = file_activity(p.get("activity_files") or [])
        last = max([t for t in (st_ts, tr_ts, fd_ts) if t] or [0]) or None
        service = p.get("agent_role") == "service"
        if service:
            label, vocab = f"{p['comm'].upper()} SERVICE PROCESS (app-server/proxy; its threads are not observable here)", LIVE
        elif status == "busy" or (status is None and last and now - last < WORKING_WINDOW_S):
            label, vocab = "WORKING", LIVE
        elif status == "idle":
            label, vocab = "IDLE AT PROMPT (waiting for input or finished)", LIVE
        elif last:
            label, vocab = f"RUNNING, last wrote {ago(last)}", LIVE
        else:
            label, vocab = "RUNNING (activity unknown)", LIVE
        v = _base_view(kind="service" if service else "agent",
                       name=c.get("name") or p.get("agent_name") or (rec or {}).get("name") or friendly_name(p, cwd, wt, service),
                       cwd=cwd, wt=wt, rows=rows, prs=prs, locks=locks, handoffs=handoffs, reg=r)
        if service:
            resume = "Service process, not a task agent. Its working directory is only where it was started."
        elif tmux_name:
            resume = "OCCUPIED - an agent is running here. Do not start another; attach to watch or talk to it."
        else:
            resume = "OCCUPIED - running in a terminal outside tmux; it cannot be attached from here."
        v.update({"pid": p["pid"], "pid_start": p.get("start"), "agent": p["comm"], "tmux": tmux_name,
                  "tty": tty_name(p.get("tty_nr")),
                  "tmux_attached": (tmux_sessions.get(tmux_name) or {}).get("attached") if tmux_name else None,
                  "conversation": c.get("sessionId"), "state_label": label, "status": vocab,
                  "last_activity": last, "started": start_epoch(p.get("start"), paths.proc),
                  "icn_session_record": (rec or {}).get("name"), "resume": resume,
                  "sources": ["process table (pid+start)"] + (["~/.claude/sessions"] if c else [])
                  + (["tmux"] if tmux_name else []) + (["icn-ops registry"] if r else [])})
        views.append(v)

    for name, s in tmux_sessions.items():
        if name in tmux_with_agent:
            continue
        pane = next((p for p in panes if p["session"] == name), {})
        cwd = pane.get("path")
        wt = toplevel_for(cwd, wts)
        if s.get("is_control_center"):
            kind, label, vocab = "control-center", "THIS CONTROL CENTER", LIVE
            resume = "This dashboard's own tmux session."
        else:
            age = now - (s.get("activity") or 0)
            kind = "tmux"
            label = "TMUX OPEN, NO AGENT PROCESS"
            vocab = STALE if age > 86400 else IDLE
            resume = "Shell only. Attaching is safe; no agent is running in it."
        v = _base_view(kind=kind, name=name, cwd=cwd, wt=wt, rows=rows, prs=prs, locks=locks, handoffs=handoffs, reg=None)
        v.update({"pid": None, "tmux": name, "tmux_attached": s.get("attached"), "state_label": label,
                  "status": vocab, "last_activity": s.get("activity"), "resume": resume,
                  "sources": ["tmux"]})
        views.append(v)

    live_cwds = {v["cwd"] for v in views if v["kind"] == "agent"}
    live_wts = {v["worktree"] for v in views if v["kind"] == "agent" and v.get("worktree")}
    for rec in recs:
        if rec.get("file") in used_rec or rec.get("unreadable"):
            continue
        wtp = rec.get("worktree")
        alive = same_process(rec.get("fg_pid"), rec.get("fg_start"), paths.proc)
        if rec.get("launch_mode") == "tmux" and rec.get("name") in tmux_sessions:
            continue  # represented by its tmux row
        wt = toplevel_for(wtp, wts)
        v = _base_view(kind="launcher-record", name=rec.get("name") or "?", cwd=wtp, wt=wt, rows=rows,
                       prs=prs, locks=locks, handoffs=handoffs, reg=None)
        if alive:
            label, vocab, resume = "RUNNING (recorded foreground pid)", LIVE, "OCCUPIED - running in a foreground terminal."
        elif wtp in live_cwds or wtp in live_wts:
            label, vocab = "RECORD OUTDATED - another agent is live in this worktree", STALE
            resume = "Do not resume: an agent is already running in this worktree."
        else:
            label, vocab = "STOPPED (launcher record only)", REGISTERED_NOT_RUNNING
            purpose = rec.get("purpose") or rec.get("name")
            resume = (f"Safe to resume (no live process found): icn-session open {purpose}"
                      if rec.get("claude_session_id") else "No recorded conversation to resume.")
        v.update({"pid": None, "tmux": None, "state_label": label, "status": vocab, "agent": "claude",
                  "record": {k: rec.get(k) for k in ("name", "purpose", "repo", "worktree", "claude_session_id",
                                                     "launch_mode", "fg_pid", "fg_start", "file", "updated_at")},
                  "last_activity": rec.get("updated_ts"), "conversation": rec.get("claude_session_id"),
                  "resume": resume, "sources": ["icn-session record"]})
        views.append(v)

    for r in reg:
        if r["id"] in used_reg:
            continue
        hb = r.get("last_heartbeat_ts")
        wt = toplevel_for(r.get("worktree_path"), wts)
        stale = hb is None or now - hb > REGISTRY_STALE_S
        v = _base_view(kind="registry", name=r.get("worktree_name") or r["id"][:8], cwd=r.get("worktree_path"),
                       wt=wt, rows=rows, prs=prs, locks=locks, handoffs=handoffs, reg=r)
        v.update({"pid": None, "tmux": None,
                  "state_label": "REGISTERED, NO MATCHING PROCESS" + (" - POSSIBLY STALE" if stale else ""),
                  "status": STALE if stale else REGISTERED_NOT_RUNNING, "last_activity": hb,
                  "conversation": r.get("provider_session_id"), "agent": r.get("provider"),
                  "resume": "Registry entry only; no agent process observed. Verify before resuming.",
                  "sources": ["icn-ops registry"]})
        views.append(v)

    order = {LIVE: 0, IDLE: 1, REGISTERED_NOT_RUNNING: 2, STALE: 3, UNKNOWN: 4}
    views.sort(key=lambda v: (order.get(v["status"], 5), v["kind"] != "agent", -(v.get("last_activity") or 0)))
    return views


def tty_name(tty_nr) -> Optional[str]:
    """Decode /proc stat tty_nr into /dev/pts/N (Unix98 pty majors 136-143); None if not a pty."""
    try:
        n = int(tty_nr)
    except (TypeError, ValueError):
        return None
    major, minor = (n >> 8) & 0xFFF, (n & 0xFF) | ((n >> 12) & 0xFFF00)
    return f"/dev/pts/{minor + (major - 136) * 256}" if 136 <= major <= 143 and n else None


def friendly_name(p: dict, cwd: Optional[str], wt: Optional[dict], service: bool) -> str:
    """A recognisable label: the worktree or directory name plus the tool. PIDs stay in the details."""
    base = os.path.basename((wt or {}).get("path") or (cwd or "").rstrip("/")) or "unknown directory"
    return f"{base} ({p['comm']}{' service' if service else ''})"


def _base_view(kind, name, cwd, wt, rows, prs, locks, handoffs, reg) -> dict:
    wt_path = wt["path"] if wt else None
    row = rows.get(os.path.normpath(wt_path)) if wt_path else None
    if row is None and cwd:
        row = rows.get(os.path.normpath(cwd))
    branch = wt.get("branch") if wt else None
    repo = wt.get("repo") if wt else None
    pr, pr_how = None, None
    ref = (reg or {}).get("pr_ref") or (reg or {}).get("task_ref")
    if ref:
        # An explicit registration wins over every inference (icn-ops register_session pr_ref/task_ref).
        m = re.match(r"^\s*(?:([\w.-]+)#)?#?(\d{1,6})\s*$", str(ref))
        if m:
            want_repo = m.group(1)
            pr = next((p for p in prs if p["number"] == int(m.group(2)) and (want_repo in (None, p["repo"]))), None)
            pr_how = "registered pr_ref/task_ref" if pr else None
    if pr is None and branch:
        pr = branch_pr(prs, repo, branch)
        pr_how = "branch match (same-repository PR)" if pr else None
    if pr is None and cwd:
        m = PR_DIR.search(os.path.basename(cwd.rstrip("/")))
        if m:
            pr = next((p for p in prs if p["number"] == int(m.group(1))), None)
            pr_how = "inferred from directory name" if pr else None
    if pr is None and cwd and os.path.isfile(os.path.join(cwd, "HANDOFF.md")):
        try:
            with open(os.path.join(cwd, "HANDOFF.md"), errors="replace") as f:
                head = f.read(4096)
        except OSError:
            head = ""
        for m in re.finditer(r"#(\d{3,6})", head):
            pr = next((p for p in prs if p["number"] == int(m.group(1))), None)
            if pr:
                pr_how = "inferred from first PR reference in HANDOFF.md"
                break
    task = None
    task_src = None
    if reg and reg.get("task_description"):
        task, task_src = reg["task_description"], "icn-ops registry"
    elif row and row.get("task"):
        task, task_src = row["task"], "active-worktrees.yaml (historical claim)"
    elif pr:
        task, task_src = pr["title"], f"PR #{pr['number']} title"
    owner_key = os.path.basename(wt_path) if wt_path else None
    claimed = []
    if reg:
        claimed += [f"{c} (registry claim)" for c in reg.get("claims") or []]
    if owner_key:
        for lk in locks:
            if lk["status"] == "open" and lk["owner"] == owner_key:
                claimed += [f"{f} (file-locks.yaml)" for f in lk["files"]]
    hand = []
    if cwd and os.path.isfile(os.path.join(cwd, "HANDOFF.md")):
        hand.append(os.path.join(cwd, "HANDOFF.md"))
    if row:
        hand += [os.path.expanduser(x) for x in row.get("refs") or [] if "handoff" in x.lower() or x.endswith(".md")]
    if owner_key:
        hand += [h["path"] for h in handoffs if owner_key in os.path.basename(h["path"])]
    if pr:
        hand += [h["path"] for h in handoffs if re.search(rf"(?<!\d){pr['number']}(?!\d)", os.path.basename(h["path"]))]
    seen, hand_u = set(), []
    for h in hand:
        if h not in seen and os.path.exists(h):
            seen.add(h)
            hand_u.append(h)
    evidence = []
    if cwd and wt_path is None and os.path.isdir(cwd):
        # A non-worktree working directory (e.g. a state/evidence dir): list its top-level records.
        try:
            evidence = sorted(os.path.join(cwd, n) for n in os.listdir(cwd)
                              if n.endswith(".md") and os.path.join(cwd, n) not in seen)[:8]
        except OSError:
            evidence = []
    return {"kind": kind, "name": name, "cwd": cwd, "worktree": wt_path, "repo": repo, "branch": branch,
            "head": wt.get("head") if wt else None, "task": task, "task_source": task_src,
            "pr": {"repo": pr["repo"], "number": pr["number"], "title": pr["title"], "how": pr_how,
                   "status": pr.get("status"), "url": pr.get("url")} if pr else None,
            "claims": claimed, "handoffs": hand_u, "evidence": evidence, "registry_id": (reg or {}).get("id"),
            "registry_heartbeat": (reg or {}).get("last_heartbeat_ts"),
            "registry_activity": (reg or {}).get("current_activity")}


# --------------------------------------------------------------------------- worktrees
def open_pr_observed(snap: dict) -> frozenset:
    """Repository keys whose open-PR list this snapshot observed live and completely.

    The single source for "this worktree has no open PR": the worktree verdicts and the activity
    classifier both use it. A repository absent here is UNKNOWN (its fetch failed or was truncated, it
    has no GitHub remote, or only cached data remains); absence of an observation is never an
    observation of no PRs. A successful empty list is present here, with no PRs.
    """
    o = snap.get("github_prs")
    if o is None or not o.ok or o.from_cache or not isinstance(o.data, dict):
        return frozenset()
    return frozenset(k for k, v in o.data.items() if isinstance(v, list))


def worktree_views(snap: dict, sessions: list[dict], catalog: dict) -> list[dict]:
    wobs = snap.get("worktrees")
    if wobs is None or not wobs.ok:
        return []
    rows = (_data(snap, "active_worktrees", {}) or {}).get("rows") or {}
    locks = (_data(snap, "file_locks", {}) or {}).get("locks") or []
    prs = snap.get("_pr_views") or []
    merged = _data(snap, "github_merged", []) or []
    observed = open_pr_observed(snap)
    out = []
    for w in wobs.data["worktrees"]:
        st = w.get("status") or {}
        name = os.path.basename(w["path"])
        keep, cand = [], []
        live = [s for s in sessions if s.get("worktree") == w["path"] and s["status"] in (LIVE, IDLE)]
        if any(s["kind"] == "agent" for s in live):
            keep.append("live agent process in it")
        elif any(s["kind"] == "service" for s in live):
            keep.append("a running service process has its working directory here")
        elif live:
            keep.append("open tmux session in it")
        if st.get("exists") is False:
            cand.append("directory missing (git still lists it; 'git worktree prune' territory)")
        if st.get("error"):
            keep.append("git status failed: " + st["error"][:80])
        if st.get("dirty"):
            keep.append(f"uncommitted changes ({st.get('modified', 0)} modified, {st.get('untracked', 0)} untracked)")
        if st.get("local_only_commits"):
            keep.append(f"{st['local_only_commits']} commit(s) on no remote-tracking ref "
                        "(unpublished, or squash-merged with the branch deleted; verify before removal)")
        elif st.get("exists") and st.get("local_only_commits") is None and not st.get("error"):
            keep.append("unpushed-commit count unknown")
        pr = branch_pr(prs, w["repo"], w.get("branch"))
        if pr:
            keep.append(f"open PR #{pr['number']}")
        open_locks = [lk for lk in locks if lk["status"] == "open" and lk["owner"] == name]
        if open_locks:
            keep.append(f"{sum(len(lk['files']) for lk in open_locks)} file(s) in open locks")
        if w.get("locked"):
            keep.append("git worktree lock")
        if any(s["kind"] in ("registry", "launcher-record") and s.get("worktree") == w["path"] for s in sessions):
            keep.append("session record points here")
        mpr = branch_pr(merged, w["repo"], w.get("branch"))
        if mpr:
            cand.append(f"PR #{mpr['number']} merged {ago(mpr.get('merged_ts'))}")
        if w["repo"] not in observed:
            keep.append(f"open-PR state unknown for {w['repo']}")
        row = rows.get(os.path.normpath(w["path"]))
        program, rule = classify(w["repo"], f"{w.get('branch') or ''} {name} {(row or {}).get('task', '')}", catalog)
        if keep:
            verdict = "KEEP"
        elif cand:
            verdict = "CLEANUP CANDIDATE (verify; never automatic)"
        else:
            verdict = "NO KEEP SIGNAL OBSERVED (not a deletion recommendation)"
        out.append({"repo": w["repo"], "path": w["path"], "name": name, "branch": w.get("branch"),
                    "detached": bool(w.get("detached")), "head": w.get("head"), "status": st,
                    "pr": {"number": pr["number"], "status": pr["status"]} if pr else None,
                    "merged_pr": mpr, "locks": open_locks, "row": row, "keep_reasons": keep,
                    "candidate_reasons": cand, "verdict": verdict, "program": program, "program_rule": rule,
                    "live_sessions": [s["name"] for s in live]})
    out.sort(key=lambda x: (x["verdict"] != "KEEP", not x["live_sessions"], x["repo"], -((x["status"] or {}).get("mtime") or 0)))
    return out


# --------------------------------------------------------------------------- attention & overview
def attention(snap: dict, prs: list[dict], sessions: list[dict], worktrees: list[dict]) -> list[dict]:
    """Things that plausibly need the maintainer. Each item names its source."""
    items = []
    now = time.time()
    for p in prs:
        if p["status"] == AWAITING_MAINTAINER:
            items.append({"sev": 2, "status": AWAITING_MAINTAINER, "title": f"{p['repo']}#{p['number']}: {p['title'][:70]}",
                          "detail": p["status_reason"], "source": "GitHub PR body lifecycle block + reviews"})
        elif p["status"] == BLOCKED:
            items.append({"sev": 2, "status": BLOCKED, "title": f"{p['repo']}#{p['number']}: {p['title'][:70]}",
                          "detail": p["status_reason"], "source": "GitHub"})
        elif p["status"] == STALE and (p.get("lifecycle") or {}).get("state"):
            items.append({"sev": 1, "status": STALE, "title": f"{p['repo']}#{p['number']}", "detail": p["status_reason"],
                          "source": "GitHub lifecycle block vs head"})
    for s in sessions:
        if s["kind"] == "agent" and s["state_label"].startswith("IDLE"):
            items.append({"sev": 1, "status": IDLE, "title": f"Agent '{s['name']}' is idle at its prompt",
                          "detail": f"last activity {ago(s.get('last_activity'))}; may be waiting for you"
                                    + (f" (tmux {s['tmux']})" if s.get("tmux") else ""),
                          "source": "~/.claude/sessions status + process table"})
        if s["kind"] == "registry" and s["status"] == STALE:
            items.append({"sev": 0, "status": STALE, "title": f"Registry entry '{s['name']}' has no running process",
                          "detail": f"heartbeat {ago(s.get('last_activity'))}", "source": "icn-ops registry"})
    orphan_dirty = [w for w in worktrees if (w["status"] or {}).get("dirty") and not w["live_sessions"]]
    if orphan_dirty:
        names = ", ".join(f"{w['repo']}/{w['name']}" for w in orphan_dirty[:5])
        items.append({"sev": 0, "status": UNKNOWN,
                      "title": f"{len(orphan_dirty)} worktree(s) hold uncommitted work with no live session",
                      "detail": names + (" ..." if len(orphan_dirty) > 5 else "") + " (see Worktrees screen; never auto-cleaned)",
                      "source": "git status"})
    items.extend(health_warnings(snap, now))
    for key in ("github_prs", "worktrees", "processes", "tmux", "registry"):
        o = snap.get(key)
        if o is None:
            items.append({"sev": 1, "status": UNKNOWN, "title": f"Source not yet observed: {key}", "detail": "", "source": key})
        elif not o.ok:
            items.append({"sev": 2, "status": UNKNOWN, "title": f"Source unavailable: {o.source}",
                          "detail": o.error or "", "source": key})
        elif o.from_cache:
            items.append({"sev": 1, "status": STALE, "title": f"Showing cached {o.source}",
                          "detail": f"observed {ago(o.observed_at)}; live refresh pending", "source": "local cache"})
    items.sort(key=lambda i: -i["sev"])
    return items


def health_warnings(snap: dict, now: float) -> list[dict]:
    out = []
    for fs in _data(snap, "fs", []) or []:
        if fs.get("error"):
            out.append({"sev": 2, "status": UNKNOWN, "title": f"Cannot stat {fs['mount']}", "detail": fs["error"], "source": "statvfs"})
            continue
        if fs["mount"] == "/mnt/build" and not fs.get("is_mountpoint"):
            out.append({"sev": 2, "status": BLOCKED, "title": "/mnt/build is not mounted", "detail": "builds would land on /",
                        "source": "statvfs"})
        if fs.get("used_pct") is not None and fs["used_pct"] >= 80:
            out.append({"sev": 2 if fs["used_pct"] >= 90 else 1, "status": BLOCKED if fs["used_pct"] >= 90 else WARNING,
                        "title": f"{fs['mount']} {fs['used_pct']}% used", "detail": f"{fs['free_gib']} GiB free",
                        "source": "statvfs"})
    dg = _data(snap, "disk_guard")
    if dg:
        lvl = (dg["data"] or {}).get("level")
        age = now - dg["mtime"]
        if lvl and lvl not in ("OK",):
            out.append({"sev": 1, "status": WARNING, "title": f"Disk guard level {lvl}", "detail": f"report written {ago(dg['mtime'])}",
                        "source": dg["file"]})
        if age > 6 * 3600:
            out.append({"sev": 1, "status": STALE, "title": "Disk guard report is old", "detail": ago(dg["mtime"]), "source": dg["file"]})
    ml = _data(snap, "mcp_live")
    if ml:
        d = ml["data"] or {}
        if d.get("state") not in (None, "ok", "current", "live", "success"):
            out.append({"sev": 1, "status": BLOCKED, "title": f"icn-ops MCP live updater: {d.get('state')}",
                        "detail": f"live {short(d.get('live_commit'))} vs origin/main {short(d.get('origin_main'))}",
                        "source": ml["file"]})
    mem = _data(snap, "mem")
    if mem and mem.get("available_gib") is not None and mem.get("total_gib") and mem["available_gib"] < 0.1 * mem["total_gib"]:
        out.append({"sev": 2, "status": BLOCKED, "title": "Low memory", "detail": f"{mem['available_gib']} GiB available",
                    "source": "/proc/meminfo"})
    for r in (_data(snap, "reservations", {}) or {}).get("reservations") or []:
        try:
            if now > int(r.get("expires", 0)):
                out.append({"sev": 1, "status": STALE, "title": f"Build reservation past TTL: lane {r.get('lane')}",
                            "detail": f"id {r['id']}", "source": "state/admit/reservations"})
        except ValueError:
            # best-effort: a malformed reservation field is skipped; the guard owns validation
            pass
    return out


def next_steps(prs: list[dict], sessions: list[dict], attention_items: list[dict]) -> list[str]:
    """A short, rule-based 'what should happen next'. Each line says why."""
    steps = []
    for p in prs:
        if p["status"] == AWAITING_MAINTAINER and (p.get("lifecycle") or {}).get("state") == "FROZEN":
            steps.append(f"Decide delivery of {p['repo']}#{p['number']} (frozen{', still draft' if p['draft'] else ''}, "
                         f"required CI {p['required']['verdict'].lower()}): "
                         f"{'mark ready and ' if p['draft'] else ''}authorize the merge, or send it back. Ledger: "
                         f"{'#' + str(p['lifecycle']['ledger']) if p['lifecycle'].get('ledger') else 'none'}.")
    for p in prs:
        if p["status"] in (AWAITING_MAINTAINER, BLOCKED) and (p.get("lifecycle") or {}).get("state") != "FROZEN" \
                and latest_review_generation(p):
            steps.append(f"Have the author of {p['repo']}#{p['number']} disposition the review findings, "
                         f"then update its lifecycle block (currently {(p.get('lifecycle') or {}).get('state') or 'absent'}).")
    idle = [s for s in sessions if s["kind"] == "agent" and s["state_label"].startswith("IDLE")]
    if idle:
        steps.append("Check idle agents (" + ", ".join(s["name"] for s in idle[:4]) + "): they may be waiting for an answer.")
    blk = [a for a in attention_items if a["status"] == BLOCKED and "CI" not in a.get("detail", "")]
    for a in blk[:2]:
        steps.append(f"Infrastructure: {a['title']} ({a['detail']}).")
    if not steps:
        steps.append("No source-backed next step derived. Review the PR and session screens.")
    return steps[:8]


def program_views(catalog: dict, prs: list[dict], sessions: list[dict], worktrees: list[dict],
                  handoffs: list[dict], issues: dict) -> list[dict]:
    out = []
    for p in catalog["programs"]:
        pid = p["id"]
        pp = [x for x in prs if x["program"] == pid]
        ww = [w for w in worktrees if w["program"] == pid]
        wpaths = {w["path"] for w in ww}
        ss = [s for s in sessions if s.get("worktree") in wpaths or (s.get("pr") and any(
            x["number"] == s["pr"]["number"] and x["repo"] == s["pr"]["repo"] for x in pp))]
        hk = [k.lower() for k in p.get("handoff_keywords") or []]
        hh = [h for h in handoffs if any(_kw_match(os.path.basename(h["path"]).lower().replace("_", "-"), k) for k in hk)][:12]
        tracking = []
        for t in p.get("tracking_issues") or []:
            key = f"{t['repo']}#{t['number']}"
            tracking.append({"ref": key, "why": t.get("why"), "live": (issues or {}).get(key)})
        waiting = [x for x in pp if x["status"] == AWAITING_MAINTAINER]
        blocked = [x for x in pp if x["status"] == BLOCKED]
        live = [s for s in ss if s["status"] == LIVE and s["kind"] == "agent"]
        if waiting:
            nxt = f"{waiting[0]['repo']}#{waiting[0]['number']}: {waiting[0]['status_reason']}"
        elif blocked:
            nxt = f"Unblock {blocked[0]['repo']}#{blocked[0]['number']}: {blocked[0]['status_reason']}"
        elif live:
            nxt = f"Agent '{live[0]['name']}' is active; check its progress."
        else:
            nxt = "No source-backed next action. Review the open work listed here."
        out.append({"id": pid, "name": p["name"], "summary": p["summary"], "prs": pp, "worktrees": ww,
                    "sessions": ss, "handoffs": hh, "tracking": tracking, "waiting": waiting, "blocked": blocked,
                    "live_agents": live, "next": nxt,
                    "owner": "unassigned (no registered owner source)" if not p.get("owner_source") else p["owner_source"],
                    "goals_source": p.get("goals_source")})
    return out


def build_views(snap: dict, paths: Paths, catalog: dict) -> dict:
    prs = pr_views(snap, paths, catalog)
    snap["_pr_views"] = prs
    sessions = session_views(snap, paths)
    worktrees = worktree_views(snap, sessions, catalog)
    by_path = {w["path"]: w for w in worktrees}
    for s in sessions:
        pr = next((p for p in prs if s.get("pr") and p["number"] == s["pr"]["number"] and p["repo"] == s["pr"]["repo"]), None)
        if pr:
            s["program"], s["program_rule"] = pr["program"], f"via PR #{pr['number']}"
        elif s.get("worktree") in by_path:
            s["program"], s["program_rule"] = by_path[s["worktree"]]["program"], "via worktree"
        else:
            s["program"], s["program_rule"] = classify(s.get("repo"), f"{s.get('name')} {s.get('cwd') or ''}", catalog)
    att = attention(snap, prs, sessions, worktrees)
    handoffs = _data(snap, "handoffs", []) or []
    progs = program_views(catalog, prs, sessions, worktrees, handoffs, _data(snap, "issues", {}) or {})
    from . import activities as actmod
    hist_obs = snap.get("github_history")
    history = hist_obs.data if (hist_obs is not None and hist_obs.ok) else None
    acts = actmod.build({"prs": prs, "sessions": sessions, "worktrees": worktrees,
                         "handoffs": _data(snap, "handoffs", []) or []},
                        history, actmod.load_marks(paths), paths, catalog, pr_observed=open_pr_observed(snap))
    return {"activities": acts, "prs": prs, "sessions": sessions, "worktrees": worktrees, "attention": att,
            "next": next_steps(prs, sessions, att), "programs": progs,
            "merged": _data(snap, "github_merged", []) or [], "handoffs": handoffs}
