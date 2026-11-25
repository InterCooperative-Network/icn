"""Quick session switcher model (pure: no curses, no I/O beyond the action planners)."""

from __future__ import annotations

import os
from typing import Callable, Optional

from .util import LIVE, STALE, ago

PICKER_KINDS = ("agent", "launcher-record", "registry")


def run_state(view: dict, actions: list[dict], paths=None) -> str:
    """Plain-language recovery state. Distinguishes: running in tmux (busy/waiting), running outside tmux,
    stopped and resumable, no launcher record, transcript unavailable, outdated record, stale entry."""
    kind, label = view.get("kind"), view.get("state_label") or ""
    if kind == "agent":
        where = "" if view.get("tmux") else f" · outside tmux{' (' + view['tty'] + ')' if view.get('tty') else ''}"
        if label.startswith("WORKING"):
            return "RUNNING · busy" + where
        if label.startswith("IDLE"):
            return "RUNNING · waiting" + where
        return "RUNNING" + where
    resume = next((a for a in actions if a["id"] == "resume"), {"enabled": False, "reason": ""})
    if resume["enabled"]:
        return "STOPPED · resumable"
    reason = resume.get("reason") or ""
    if label.startswith("RECORD OUTDATED"):
        return "OUTDATED record · agent live here"
    if "No transcript" in reason or "cannot be resumed reliably" in reason:
        return "STOPPED · transcript unavailable"
    if kind == "registry":
        if "Manual route" in reason:
            return "STOPPED · no launcher record"
        return "STALE registry entry" if view.get("status") == STALE else "REGISTERED · no process"
    if view.get("status") == STALE:
        return "STALE record"
    if kind == "launcher-record":
        return "STOPPED · cannot resume"
    return "UNKNOWN"


def can_do(actions: list[dict]) -> str:
    on = [a["id"] for a in actions if a["enabled"] and a["id"] in ("join", "resume")]
    if "join" in on:
        return "join"
    if "resume" in on:
        return "resume"
    return "inspect"


def build_rows(sessions: list[dict], programs: dict, actions_for: Callable[[dict], list[dict]],
               paths=None, titles: Optional[dict] = None) -> list[dict]:
    """One row per Claude/agent conversation. Running first, then most recent activity."""
    rows = []
    for s in sessions:
        if s.get("kind") not in PICKER_KINDS:
            continue
        acts = actions_for(s)
        where = s.get("worktree") or s.get("cwd") or ""
        pr = s.get("pr")
        rows.append({
            "view": s, "actions": acts,
            "name": s.get("name") or "?",
            "task": (titles or {}).get(id(s)) or s.get("task") or (pr or {}).get("title") or "",
            "program": programs.get(s.get("program"), s.get("program") or "unclassified"),
            "repo": s.get("repo") or "-",
            "where": os.path.basename(where.rstrip("/")) or "?",
            "pr": f"{pr['repo']}#{pr['number']}" if pr else "",
            "pr_how": (pr or {}).get("how") or "",
            "state": run_state(s, acts, paths),
            "activity": ago(s.get("last_activity")),
            "last": s.get("last_activity") or 0,
            "can": can_do(acts),
            "live": s.get("kind") == "agent" and s.get("status") == LIVE,
        })
    rows.sort(key=lambda r: (not r["live"], -(r["last"] or 0), r["name"]))
    return rows


def scope_sessions(sessions: list[dict], activities: list[dict], include_all: bool = False) -> list[dict]:
    """The switcher's default scope: every live agent, plus the session records of current work
    (Active / Paused activities). An outdated launcher record for a worktree whose agent is live is
    folded into that agent (it is the same work, not a second session). include_all shows everything."""
    if include_all:
        return list(sessions)
    keep = {id(s) for a in activities if a.get("status") in ("Active", "Paused") for s in a.get("sessions", [])}
    live_wts = {s.get("worktree") for s in sessions if s.get("kind") == "agent" and s.get("worktree")}
    out = []
    for s in sessions:
        if s.get("kind") == "agent":
            out.append(s)
        elif id(s) in keep and not (s.get("kind") == "launcher-record" and s.get("worktree") in live_wts):
            out.append(s)
    return out


def filter_rows(rows: list[dict], text: str) -> list[dict]:
    """Every space-separated word must appear in name, task, program, repo, worktree, PR or state."""
    words = [w for w in (text or "").lower().split() if w]
    if not words:
        return rows
    out = []
    for r in rows:
        hay = " ".join(str(r[k]) for k in ("name", "task", "program", "repo", "where", "pr", "state", "can")).lower()
        if all(w in hay for w in words):
            out.append(r)
    return out


def default_action(row: dict) -> str:
    return {"join": "join", "resume": "resume"}.get(row["can"], "details")
