#!/usr/bin/env python3
"""A job's timeout must exceed the budgets nested inside it.

Why this exists
---------------
PR #2762 bounded every job in the repository, and introduced three jobs whose
`timeout-minutes` was BELOW the `timeout-minutes` already declared on their own
steps:

    benchmark.yml  benchmark  job 75  vs  step 90
    benchmark.yml  compare    job 75  vs  steps 60 + 60 (sequential)
    fuzz.yml       fuzz       job 60  vs  3 sequential targets x dispatch duration

A job deadline below its own step budget cancels the step before it can use the
time it was given, and reports a timeout rather than the real result. The two
numbers are written in different places, by different people, at different
times, and nothing related them — so the error was invisible to review until a
reviewer did the arithmetic by hand three times.

This is that arithmetic, executed.
"""
from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import yaml

# A job needs time for checkout, toolchain install and caching on top of the
# work its steps declare. A job budget merely EQUAL to its step budgets is
# already a defect.
SETUP_HEADROOM_MINUTES = 5


def repo_root() -> Path:
    return Path(
        subprocess.run(
            ["git", "rev-parse", "--show-toplevel"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
    )


def main() -> int:
    root = repo_root()
    failures: list[str] = []
    checked = 0

    for wf in sorted((root / ".github/workflows").glob("*.yml")):
        try:
            doc = yaml.safe_load(wf.read_text())
        except yaml.YAMLError as exc:
            failures.append(f"{wf.name}: does not parse: {exc}")
            continue
        if not isinstance(doc, dict):
            continue
        for job_name, job in (doc.get("jobs") or {}).items():
            if not isinstance(job, dict):
                continue
            job_budget = job.get("timeout-minutes")
            if job_budget is None:
                continue
            # Steps run sequentially, so their budgets add.
            step_budgets = [
                s.get("timeout-minutes")
                for s in (job.get("steps") or [])
                if isinstance(s, dict) and s.get("timeout-minutes")
            ]
            if not step_budgets:
                continue
            checked += 1
            required = sum(step_budgets) + SETUP_HEADROOM_MINUTES
            if job_budget < required:
                failures.append(
                    f"{wf.name}:{job_name}: job timeout-minutes={job_budget} but its steps "
                    f"declare {step_budgets} (sum {sum(step_budgets)}) and run sequentially. "
                    f"The job would cancel a step before that step's own budget expired. "
                    f"Need at least {required} (sum + {SETUP_HEADROOM_MINUTES} setup headroom)."
                )

    if failures:
        print("test_workflow_timeout_budgets: FAIL")
        for f in failures:
            print(f"  - {f}")
        return 1
    print(
        f"test_workflow_timeout_budgets: clean "
        f"({checked} job(s) with nested step budgets verified)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
