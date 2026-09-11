#!/usr/bin/env python3
"""Registering a truth domain without regenerating the spine must fail a gate.

Why this exists
---------------
PR #2773 registered `engineering_leverage` in `ops/state/truth/sources.json` and
did not regenerate `agent-context-spine.json`. Review caught it; no gate did.

Three separate things kept the existing check from binding, and all three were
true at once:

  * `generated-truth.yml` runs `check-agent-context-spine.py`, captures its exit
    status correctly, and then reports drift as a `::warning::` that cannot fail
    the job;
  * that workflow's `paths:` filter does not include `ops/state/truth/**` — the
    very input the spine consumes;
  * `generated-truth` is not a required check.

The repair calls the SAME checker from `ops/scripts/drift-check.sh`, which runs
inside the required Agent Tooling Drift Check. No second generation mechanism.

This witness proves the enforcement binds rather than merely existing: it
registers a synthetic domain, confirms the freshness check FAILS, and restores.
"""
from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

CHECKER = "scripts/check-agent-context-spine.py"
SOURCES = "ops/state/truth/sources.json"


def repo_root() -> Path:
    """Resolve the repository from THIS FILE, not from the caller's cwd.

    `git rev-parse` run in the caller's working directory answers a different
    question: it reports whatever repository the caller happens to be standing
    in. `ops/scripts/drift-check.sh` deliberately derives its own REPO_ROOT and
    is safe to invoke from anywhere, so a checker it calls must be too —
    otherwise `cd /tmp && bash .../drift-check.sh` fails on a healthy tree, or,
    worse, inspects a different checkout. Same resolution as
    scripts/check-agent-context-spine.py.
    """
    try:
        return Path(
            subprocess.run(
                ["git", "rev-parse", "--show-toplevel"],
                cwd=Path(__file__).resolve().parent,
                capture_output=True, text=True, check=True,
            ).stdout.strip()
        )
    except (subprocess.CalledProcessError, OSError):
        return Path(__file__).resolve().parents[2]


def run_checker(root: Path) -> int:
    """Exit status of the freshness checker. Captured directly — never through a pipe."""
    return subprocess.run(
        [sys.executable, CHECKER], cwd=root, capture_output=True, text=True
    ).returncode


def main() -> int:
    root = repo_root()
    failures: list[str] = []

    # Baseline: the committed tree must be fresh, or the witness below proves nothing.
    if run_checker(root) != 0:
        print("test_context_spine_enforcement: FAIL")
        print("  - baseline is already stale; regenerate with "
              "`python3 scripts/generate-agent-context-spine.py --write`")
        return 1

    src = root / SOURCES
    # Restore from a byte copy, not from git: an uncommitted edit in the working
    # tree would make `git checkout --` restore the wrong baseline, which has
    # bitten this repository's mutation work repeatedly.
    with tempfile.TemporaryDirectory() as tmp:
        backup = Path(tmp) / "sources.json"
        shutil.copy2(src, backup)
        try:
            doc = json.loads(src.read_text())
            doc["domains"]["__witness_unregistered_domain__"] = {
                "owner": "docs/ATLAS.md",
                "stability": "slow-changing",
                "description": "Synthetic domain injected by "
                               "test_context_spine_enforcement.py. If you are reading this in a "
                               "committed file, the witness aborted before restoring.",
            }
            src.write_text(json.dumps(doc, indent=2) + "\n")

            rc = run_checker(root)
            if rc == 0:
                failures.append(
                    "registering a new truth domain without regenerating the context spine left "
                    "the freshness check GREEN — the enforcement does not bind"
                )
        finally:
            shutil.copy2(backup, src)

    # Prove the restore semantically, not by hash: re-run the checker.
    if run_checker(root) != 0:
        failures.append(
            "restore did not return the tree to a fresh state — the witness has left the "
            "repository dirty"
        )

    # The enforcement must live on a path that gates merge.
    drift = (root / "ops/scripts/drift-check.sh").read_text()
    if "check-agent-context-spine.py" not in drift:
        failures.append(
            "ops/scripts/drift-check.sh does not run the spine checker; generated-truth.yml only "
            "warns and is not a required check, so nothing would block a stale spine"
        )

    if failures:
        print("test_context_spine_enforcement: FAIL")
        for f in failures:
            print(f"  - {f}")
        return 1
    print(
        "test_context_spine_enforcement: clean "
        "(an unregenerated spine fails the checker; enforcement is on the required drift path)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
