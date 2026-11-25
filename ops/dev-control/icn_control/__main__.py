"""Entry point: python3 -m icn_control [--report | --json | --tui]."""

from __future__ import annotations

import argparse
import json
import sys

from . import __version__
from .reconcile import build_views, load_catalog
from .snapshot import Collector
from .util import Paths


def inventory(views: dict) -> dict:
    """Counts only (no titles or paths): what the dashboard sees and how it is consolidated."""
    import collections
    import time
    from .activities import VIEWS, in_view, summary
    from .picker import PICKER_KINDS, scope_sessions
    acts = views.get("activities", [])
    sessions = views.get("sessions", [])
    return {"captured_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "dashboard_version": __version__,
            "sessions_total": len(sessions), "session_kinds": dict(collections.Counter(s["kind"] for s in sessions)),
            "picker_rows_all": sum(1 for s in sessions if s["kind"] in PICKER_KINDS),
            "picker_rows_default": sum(1 for s in scope_sessions(sessions, acts) if s["kind"] in PICKER_KINDS),
            "worktrees_total": len(views.get("worktrees", [])), "open_prs": len(views.get("prs", [])),
            "activities": summary(acts), "view_sizes": {v: sum(1 for a in acts if in_view(a, v)) for v in VIEWS},
            "retirable_activities": sum(1 for a in acts if a["retirable"]),
            "multi_item_activities": sum(1 for a in acts if len(a["sessions"]) + len(a["worktrees"]) + len(a["prs"]) > 1)}


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(prog="icn-control", description="ICN Development Control Center")
    g = ap.add_mutually_exclusive_group()
    g.add_argument("--report", action="store_true", help="print a plain-text report and exit")
    g.add_argument("--json", action="store_true", help="print reconciled views as JSON and exit")
    g.add_argument("--inventory", action="store_true", help="print counts of sessions, worktrees and activities (JSON)")
    ap.add_argument("--no-github", action="store_true", help="skip GitHub (report shows it as unavailable)")
    ap.add_argument("--no-git", action="store_true", help="skip the per-worktree git status sweep")
    ap.add_argument("--here", action="store_true", help=argparse.SUPPRESS)  # launcher flag; the UI always runs here
    ap.add_argument("--version", action="version", version=__version__)
    a = ap.parse_args(argv)

    paths = Paths()
    catalog = load_catalog()
    if a.report or a.json or a.inventory:
        c = Collector(paths, catalog, use_cache=False)
        tiers = ["fast", "records"] + ([] if a.no_git else ["git"]) + ([] if a.no_github else ["github", "history"])
        c.run_once(tiers)
        snap = c.snapshot()
        views = build_views(snap, paths, catalog)
        if a.inventory:
            json.dump(inventory(views), sys.stdout, indent=1)
            sys.stdout.write("\n")
        elif a.json:
            json.dump({"views": views, "sources": {k: {"source": o.source, "ok": o.ok, "observed_at": o.observed_at,
                                                         "error": o.error} for k, o in snap.items() if not k.startswith("_")}},
                      sys.stdout, default=str, indent=1)
        else:
            from .report import render
            from .util import clean
            sys.stdout.write(clean(render(snap, views)))
        return 0
    from .tui import run_tui
    return run_tui(paths, catalog)


if __name__ == "__main__":
    sys.exit(main())
