"""Entry point: python3 -m icn_control [--report | --json | --tui]."""

from __future__ import annotations

import argparse
import json
import sys

from . import __version__
from .reconcile import build_views, load_catalog
from .snapshot import Collector
from .util import Paths


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(prog="icn-control", description="ICN Development Control Center")
    g = ap.add_mutually_exclusive_group()
    g.add_argument("--report", action="store_true", help="print a plain-text report and exit")
    g.add_argument("--json", action="store_true", help="print reconciled views as JSON and exit")
    ap.add_argument("--no-github", action="store_true", help="skip GitHub (report shows it as unavailable)")
    ap.add_argument("--no-git", action="store_true", help="skip the per-worktree git status sweep")
    ap.add_argument("--here", action="store_true", help=argparse.SUPPRESS)  # launcher flag; the UI always runs here
    ap.add_argument("--version", action="version", version=__version__)
    a = ap.parse_args(argv)

    paths = Paths()
    catalog = load_catalog()
    if a.report or a.json:
        c = Collector(paths, catalog, use_cache=False)
        tiers = ["fast", "records"] + ([] if a.no_git else ["git"]) + ([] if a.no_github else ["github"])
        c.run_once(tiers)
        snap = c.snapshot()
        views = build_views(snap, paths, catalog)
        if a.json:
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
