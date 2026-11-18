"""Quick session switcher model (pure: no curses, no I/O beyond the action planners)."""

from __future__ import annotations

import os
from typing import Callable

from .util import LIVE, STALE, ago

PICKER_KINDS = ("agent", "launcher-record", "registry")


def run_state(view: dict, actions: list[dict]) -> str:
    """Plain-language state: running/busy, running/waiting, stopped (resumable), not running, unknown."""
    kind, label = view.get("kind"), view.get("state_label") or ""
    if kind == "agent":
        if label.startswith("WORKING"):
            return "RUNNING · busy"
        if label.startswith("IDLE"):
            return "RUNNING · waiting"
        return "RUNNING"
    resumable = any(a["id"] == "resume" and a["enabled"] for a in actions)
    if view.get("status") == STALE:
        return "STALE record"
    if kind == "launcher-record":
        return "STOPPED · resumable" if resumable else "STOPPED"
    if kind == "registry":
        return "NOT RUNNING (registry only)"
    return "UNKNOWN"


def can_do(actions: list[dict]) -> str:
    on = [a["id"] for a in actions if a["enabled"] and a["id"] in ("join", "resume")]
    if "join" in on:
        return "join"
    if "resume" in on:
        return "resume"
    return "inspect"


def build_rows(sessions: list[dict], programs: dict, actions_for: Callable[[dict], list[dict]]) -> list[dict]:
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
            "task": s.get("task") or (pr or {}).get("title") or "",
            "program": programs.get(s.get("program"), s.get("program") or "unclassified"),
            "repo": s.get("repo") or "-",
            "where": os.path.basename(where.rstrip("/")) or "?",
            "pr": f"{pr['repo']}#{pr['number']}" if pr else "",
            "pr_how": (pr or {}).get("how") or "",
            "state": run_state(s, acts),
            "activity": ago(s.get("last_activity")),
            "last": s.get("last_activity") or 0,
            "can": can_do(acts),
            "live": s.get("kind") == "agent" and s.get("status") == LIVE,
        })
    rows.sort(key=lambda r: (not r["live"], -(r["last"] or 0), r["name"]))
    return rows


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
