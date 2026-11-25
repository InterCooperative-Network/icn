"""Logical development activities: one piece of work with all of its sessions, worktrees, PRs,
reviews and handoffs, derived from evidence on every refresh.

Grouping is by explicit links only, each recorded with its evidence:
  worktree <-> open PR      the worktree's branch is the PR head (same repository; forks excluded)
  session  <-> worktree     the agent's working directory is inside the worktree
  session  <-> PR           registered pr_ref, branch match, or an explicit PR number (labelled inferred)
  worktree <-> PR           an explicit PR number in a review checkout's name/branch (review-pr-N, review/pr-N)
Similar-looking names alone never join two pieces of work.

Status is derived, not stored. The only persisted state is a user-local, display-only overlay of
"paused"/"archived" marks (activity-marks.json); it deletes nothing and is not a registry.
"""

from __future__ import annotations

import json
import os
import re
import tempfile
import time
from typing import Optional

from .sessionctl import session_actions
from .util import AWAITING_MAINTAINER, BLOCKED, LIVE, STALE, Paths, ago

ACTIVE, PAUSED, COMPLETED, ARCHIVED, UNRESOLVED = "Active", "Paused", "Completed", "Archived", "Unresolved"
CLASSES = (ACTIVE, PAUSED, COMPLETED, ARCHIVED, UNRESOLVED)
NOW_CLASSES = (ACTIVE, PAUSED)
REVIEW_REF = re.compile(r"(?i)(?:^|[^a-z0-9])(?:pr|review)[-_/]?(?:pr[-_]?)?(\d{2,6})(?:\D|$)")
RECENT_PR_S = 24 * 3600          # an open PR that moved in the last day counts as underway
DORMANT_S = 30 * 86400           # paused work untouched this long leaves the default view (still listed)
FRESH_WT_S = 3 * 86400


class _UF:
    def __init__(self):
        self.p = {}

    def find(self, x):
        self.p.setdefault(x, x)
        while self.p[x] != x:
            self.p[x] = self.p[self.p[x]]
            x = self.p[x]
        return x

    def union(self, a, b):
        ra, rb = self.find(a), self.find(b)
        if ra != rb:
            self.p[max(ra, rb)] = min(ra, rb)


def _skey(s: dict) -> str:
    return f"sess:{s.get('kind')}:{s.get('pid') or ''}:{s.get('tmux') or ''}:{s.get('name')}:{s.get('cwd') or ''}"


def build(views: dict, history: Optional[dict], marks: dict, paths: Paths, catalog: dict,
          pr_observed: frozenset = frozenset(), now: Optional[float] = None) -> list[dict]:
    now = now or time.time()
    prs = views.get("prs", [])
    sessions = views.get("sessions", [])
    worktrees = views.get("worktrees", [])
    handoffs = views.get("handoffs", [])
    uf = _UF()
    edges: list = []

    def link(a, b, why):
        uf.union(a, b)
        edges.append((a, why))   # filed under the FINAL group after every union (roots move as groups merge)

    pr_by_key = {f"pr:{p['repo']}#{p['number']}": p for p in prs}
    wt_by_key = {f"wt:{w['path']}": w for w in worktrees}
    for k in pr_by_key:
        uf.find(k)
    for k, w in wt_by_key.items():
        uf.find(k)
        if w.get("pr"):
            link(k, f"pr:{w['repo']}#{w['pr']['number']}", f"worktree {w['name']} is the head branch of PR #{w['pr']['number']}")
        else:
            m = REVIEW_REF.search(f"{w['name']} {w.get('branch') or ''}")
            pk = f"pr:{w['repo']}#{m.group(1)}" if m else None
            if pk in pr_by_key:
                link(k, pk, f"review checkout {w['name']} names PR #{m.group(1)}")
    sess_by_key = {}
    for s in sessions:
        if s.get("kind") in ("service", "control-center"):
            continue  # a service's working directory is not ownership; the dashboard is not work
        if s.get("kind") == "tmux" and not s.get("worktree"):
            continue  # an empty shell outside any worktree is not a piece of work
        k = _skey(s)
        sess_by_key[k] = s
        uf.find(k)
        if s.get("worktree") and f"wt:{s['worktree']}" in wt_by_key:
            link(k, f"wt:{s['worktree']}", f"session {s['name']} runs in worktree {os.path.basename(s['worktree'])}")
        elif s.get("cwd"):
            link(k, f"dir:{s['cwd']}", f"session {s['name']} runs in {s['cwd']}")
        if s.get("pr"):
            pk = f"pr:{s['pr']['repo']}#{s['pr']['number']}"
            if pk in pr_by_key:
                link(k, pk, f"session {s['name']} -> PR #{s['pr']['number']} ({s['pr'].get('how')})")

    groups: dict = {}
    for node in list(uf.p):
        groups.setdefault(uf.find(node), []).append(node)
    links: dict = {}
    for node, why in edges:
        links.setdefault(uf.find(node), []).append(why)

    hist = history or {}
    out = []
    for root, nodes in groups.items():
        a_prs = [pr_by_key[n] for n in nodes if n in pr_by_key]
        a_wts = [wt_by_key[n] for n in nodes if n in wt_by_key]
        a_sess = [sess_by_key[n] for n in nodes if n in sess_by_key]
        dirs = sorted(n[4:] for n in nodes if n.startswith("dir:"))
        if not (a_prs or a_wts or a_sess):
            continue
        anchor = (sorted(n for n in nodes if n.startswith("pr:")) or sorted(n for n in nodes if n.startswith("wt:"))
                  or sorted(n for n in nodes if n.startswith("dir:")) or sorted(nodes))[0]
        merged, closed = [], []
        for w in a_wts:
            h = (hist.get(w["repo"]) or {})
            merged += [dict(x, repo=w["repo"], branch=w.get("branch")) for x in (h.get("merged") or {}).get(w.get("branch") or "", [])]
            closed += [dict(x, repo=w["repo"], branch=w.get("branch")) for x in (h.get("closed") or {}).get(w.get("branch") or "", [])]
        act = {"id": anchor, "prs": a_prs, "worktrees": a_wts, "sessions": a_sess, "dirs": dirs,
               "merged_prs": merged, "closed_prs": closed, "links": sorted(set(links.get(root, []))),
               # repositories of this activity's worktrees whose open-PR list was not observed (unknown, not "none")
               "pr_unknown": sorted({w["repo"] for w in a_wts if w["repo"] not in pr_observed})}
        _describe(act, catalog)
        act["handoffs"] = _handoffs_for(act, handoffs)
        act["evidence_dirs"] = sorted({d for p in a_prs for d in p.get("evidence_dirs") or []})
        act["ledgers"] = sorted({f"{p['repo']}#{p['lifecycle']['ledger']}" for p in a_prs if (p.get("lifecycle") or {}).get("ledger")})
        _classify(act, marks.get(anchor), history is not None, now)
        act["next"] = _next_action(act, sessions, paths)
        out.append(act)
    order = {ACTIVE: 0, PAUSED: 1, UNRESOLVED: 2, COMPLETED: 3, ARCHIVED: 4}
    out.sort(key=lambda a: (not a["attention"], order[a["status"]], -(a["last_activity"] or 0), a["title"]))
    return out


def _describe(act: dict, catalog: dict) -> None:
    prs, wts, ss = act["prs"], act["worktrees"], act["sessions"]
    if prs:
        p = sorted(prs, key=lambda x: x["number"])[0]
        title, program = f"{p['repo']}#{p['number']} {p['title']}", p.get("program")
    elif wts:
        w = sorted(wts, key=lambda x: x["path"])[0]
        task = ((w.get("row") or {}).get("task") or "").strip()
        title = f"{w['repo']}/{w['name']}" + (f" — {task[:90]}" if task else "")
        program = w.get("program")
    else:
        s = ss[0]
        title = (s.get("task") and f"{s['name']} — {s['task'][:90]}") or s["name"]
        program = s.get("program")
    act["title"] = title
    act["program"] = program or "unclassified"
    act["program_name"] = next((p["name"] for p in catalog.get("programs", []) if p["id"] == act["program"]), act["program"])
    times = [s.get("last_activity") for s in ss] + [p.get("updated_ts") for p in prs] + \
            [(w.get("status") or {}).get("mtime") for w in wts]
    act["last_activity"] = max([t for t in times if t] or [0]) or None


def _handoffs_for(act: dict, handoffs: list) -> list:
    out = []
    nums = {str(p["number"]) for p in act["prs"]} | {str(x["number"]) for x in act["merged_prs"]}
    names = {w["name"] for w in act["worktrees"] if len(w["name"]) >= 8}
    for s in act["sessions"]:
        out += s.get("handoffs") or []
        out += s.get("evidence") or []
    for h in handoffs:
        base = os.path.basename(h["path"])
        if any(re.search(rf"(?<!\d){n}(?!\d)", base) for n in nums) or any(n in base for n in names):
            out.append(h["path"])
    for w in act["worktrees"]:
        for r in (w.get("row") or {}).get("refs") or []:
            p = os.path.expanduser(r)
            if os.path.isfile(p):
                out.append(p)
    seen, res = set(), []
    for p in out:
        if p not in seen:
            seen.add(p)
            res.append(p)
    return res


def _resumable(sessions: list) -> list:
    """Launcher records whose saved conversation can still be resumed. The one predicate for both the
    Paused classification and the retirement protection: a STALE record (another agent is live in its
    worktree) is excluded, and that live agent is itself a protection."""
    return [s for s in sessions if s.get("kind") == "launcher-record" and s.get("status") != STALE]


def _protections(act: dict) -> list:
    """Reasons no physical asset of this activity may be retired (shown on every activity)."""
    prot = []
    for repo in act.get("pr_unknown", []):
        prot.append(f"open-PR state unknown for {repo}")
    for w in act["worktrees"]:
        st = w.get("status") or {}
        if st.get("dirty"):
            prot.append(f"{w['name']}: uncommitted changes")
        if st.get("local_only_commits"):
            prot.append(f"{w['name']}: {st['local_only_commits']} commit(s) on no remote-tracking ref")
        if w.get("locks"):
            prot.append(f"{w['name']}: open file locks")
        if st.get("error") or st.get("local_only_commits") is None and st.get("exists"):
            prot.append(f"{w['name']}: git state not fully known")
    for p in act["prs"]:
        prot.append(f"open PR {p['repo']}#{p['number']}")
    for s in act["sessions"]:
        if s.get("kind") == "agent":
            prot.append(f"live agent {s['name']}")
    for s in _resumable(act["sessions"]):
        # Retiring the worktree would remove the directory its resume depends on.
        prot.append(f"resumable conversation {s['name']}")
    return prot


def _classify(act: dict, mark: Optional[dict], history_known: bool, now: float) -> None:
    live = [s for s in act["sessions"] if s.get("kind") == "agent" and s.get("status") == LIVE]
    resumable = _resumable(act["sessions"])
    prs, wts = act["prs"], act["worktrees"]
    dirty = any((w.get("status") or {}).get("dirty") for w in wts)
    local_only = any((w.get("status") or {}).get("local_only_commits") for w in wts)
    waiting = [p for p in prs if p.get("status") in (AWAITING_MAINTAINER, BLOCKED)]
    idle_agents = [s for s in live if (s.get("state_label") or "").startswith("IDLE")]
    why = []
    if live:
        status = ACTIVE
        why.append(f"{len(live)} live agent process(es)")
    elif waiting:
        status = ACTIVE
        why.append("open PR needs a maintainer decision or is blocked")
    elif prs:
        recent = [p for p in prs if now - (p.get("updated_ts") or 0) < RECENT_PR_S]
        status = ACTIVE if recent else PAUSED
        why.append("open PR updated in the last 24 hours" if recent else
                   f"open PR, last updated {ago(max(p.get('updated_ts') or 0 for p in prs))}, no live agent")
    elif act.get("pr_unknown"):
        status = UNRESOLVED
        why.append("open-PR state unknown for " + ", ".join(act["pr_unknown"]) + "; cannot tell finished from unfinished")
    elif not history_known and wts:
        status = UNRESOLVED
        why.append("PR history unavailable; cannot tell finished from unfinished")
    elif act["merged_prs"]:
        if dirty:
            status = UNRESOLVED
            why.append("PR merged, but the worktree still has uncommitted changes")
        else:
            status = COMPLETED
            why.append(f"PR #{act['merged_prs'][0]['number']} merged")
    elif act["closed_prs"]:
        status = UNRESOLVED if (dirty or local_only) else ARCHIVED
        why.append(f"PR #{act['closed_prs'][0]['number']} closed without merge"
                   + ("; local changes remain" if (dirty or local_only) else " (superseded)"))
    elif dirty:
        status = PAUSED
        why.append("uncommitted work preserved in the worktree, no live agent")
    elif resumable:
        status = PAUSED
        why.append("a saved conversation can be resumed")
    elif local_only:
        status = UNRESOLVED
        why.append("commits not on any remote-tracking ref and no PR found (unpublished, or merged under another branch)")
    elif wts and all(now - ((w.get("status") or {}).get("mtime") or 0) < FRESH_WT_S for w in wts):
        status = PAUSED
        why.append("recently created worktree, nothing committed yet")
    elif wts:
        status = ARCHIVED
        why.append("clean worktree with nothing unique and no PR (reference / historical checkout)")
    else:
        status = ARCHIVED if not act["sessions"] or all(s.get("status") == STALE for s in act["sessions"]) else UNRESOLVED
        why.append("only session records, no worktree or PR")
    act["derived_status"] = status
    act["mark"] = mark
    if mark and mark.get("mark") in ("archived", "paused"):
        want = ARCHIVED if mark["mark"] == "archived" else PAUSED
        if live:
            why.append(f"marked {mark['mark']} in the dashboard, but an agent is live: shown as Active")
        else:
            status = want
            why.append(f"marked {mark['mark']} in the dashboard on {mark.get('at', '?')} (display only)")
    act["status"] = status
    act["why"] = why
    act["attention"] = bool(waiting or idle_agents) and status in (ACTIVE, PAUSED)
    act["attention_reasons"] = [f"{p['repo']}#{p['number']}: {p.get('status_reason')}" for p in waiting] + \
        [f"agent {s['name']} is idle at its prompt ({ago(s.get('last_activity'))})" for s in idle_agents]
    act["protections"] = _protections(act)
    act["retirable"] = status in (COMPLETED, ARCHIVED) and not act["protections"] and bool(act["worktrees"])


def _next_action(act: dict, sessions: list, paths: Paths) -> str:
    live_tmux = [s for s in act["sessions"] if s.get("kind") == "agent" and s.get("tmux")]
    if act["attention_reasons"]:
        return act["attention_reasons"][0]
    if live_tmux:
        return f"Join '{live_tmux[0]['name']}' to check its progress."
    if any(s.get("kind") == "agent" for s in act["sessions"]):
        return "An agent is running outside tmux; continue in its own terminal."
    for s in act["sessions"]:
        if any(a["id"] == "resume" and a["enabled"] for a in session_actions(s, sessions, paths)):
            return f"Resume saved conversation '{s['name']}'."
    if act["status"] == ACTIVE and act["prs"]:
        p = act["prs"][0]
        return f"{p['repo']}#{p['number']}: {p.get('status_reason')}"
    if act["status"] == PAUSED and act["prs"]:
        return f"Continue {act['prs'][0]['repo']}#{act['prs'][0]['number']} (open {ago(act['prs'][0].get('updated_ts'))} since last update)."
    if act["status"] == PAUSED and act["worktrees"]:
        return f"Open Claude Code in {act['worktrees'][0]['name']} to continue (c), or mark it archived."
    if act["retirable"]:
        return "Finished and unprotected: request retirement approval (x) if the checkout is no longer needed."
    if act["status"] == UNRESOLVED:
        return "Needs a human look: " + "; ".join(act["why"])
    return "Nothing to do; kept for reference."


# --------------------------------------------------------------------------- display-only marks
def marks_path(paths: Paths) -> str:
    return os.path.join(paths.control_state, "activity-marks.json")


def load_marks(paths: Paths) -> dict:
    try:
        with open(marks_path(paths)) as f:
            d = json.load(f)
        m = d.get("marks") if isinstance(d, dict) else None
        return {k: v for k, v in (m or {}).items() if isinstance(v, dict) and v.get("mark") in ("archived", "paused")}
    except (OSError, ValueError):
        return {}


def set_mark(paths: Paths, activity_id: str, mark: Optional[str], note: str = "") -> dict:
    """Set or clear a display mark. Atomic, 0600. Changes nothing outside this file."""
    if mark not in (None, "archived", "paused"):
        raise ValueError("mark must be archived, paused or None")
    marks = load_marks(paths)
    if mark is None:
        marks.pop(activity_id, None)
    else:
        marks[activity_id] = {"mark": mark, "at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "note": note[:200]}
    os.makedirs(paths.control_state, mode=0o700, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=paths.control_state, prefix=".marks.")
    with os.fdopen(fd, "w") as f:
        json.dump({"schema": "icn-control.activity-marks.v1", "canonical": False,
                   "note": "Display-only overlay for the ICN control center. Deletes nothing; not project truth.",
                   "marks": marks}, f, indent=1)
    os.chmod(tmp, 0o600)
    os.replace(tmp, marks_path(paths))
    return marks


# --------------------------------------------------------------------------- retirement requests
def retirement_request(act: dict, request_dir: str) -> str:
    """Append a reviewable retirement request (never executed here). Returns the file path."""
    os.makedirs(request_dir, exist_ok=True)
    path = os.path.join(request_dir, "retirement-requests.md")
    lines = [f"\n## {time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())} — {act['title']}",
             f"- activity: `{act['id']}` — status {act['status']} ({'; '.join(act['why'])})",
             f"- protections: {'none observed' if not act['protections'] else '; '.join(act['protections'])}"]
    for w in act["worktrees"]:
        lines.append(f"- [ ] worktree `{w['path']}` branch `{w.get('branch')}` head `{(w.get('head') or '')[:12]}`")
        lines.append(f"      verify then: `wt-clean {w['repo']}` (dry run) and, if it prints select={w['name']}@<sha>:"
                     f" `wt-clean {w['repo']} --apply --only {w['name']}@<sha>`")
    lines.append("- The branch ref, the conversation transcripts and every registry record are kept either way.")
    with open(path, "a") as f:
        f.write("\n".join(lines) + "\n")
    return path


VIEWS = ("now", "open", "archive")


def in_view(act: dict, view: str, now: Optional[float] = None) -> bool:
    """now: Active + needs-you + Paused touched in 30 days. open: Active + all Paused. archive: the rest."""
    now = now or time.time()
    if view == "archive":
        return act["status"] in (COMPLETED, ARCHIVED, UNRESOLVED)
    if view == "open":
        return act["status"] in NOW_CLASSES
    return act["status"] == ACTIVE or act["attention"] or (
        act["status"] == PAUSED and now - (act["last_activity"] or 0) < DORMANT_S)


def summary(acts: list) -> dict:
    c = {k: 0 for k in CLASSES}
    for a in acts:
        c[a["status"]] += 1
    c["attention"] = sum(1 for a in acts if a["attention"])
    c["total"] = len(acts)
    return c
