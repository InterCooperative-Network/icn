#!/usr/bin/env python3
"""Executable consumer + regression gate for `what-matters-now.sh --drift` (icn#2723).

`Agent Tooling Drift Check` is a *required* context, and this script is what it
runs. Under `set -euo pipefail` the drift count was kept with `((DRIFT_ERRORS++))`,
and an arithmetic *command* whose value is zero exits 1. Post-increment evaluates
to the value **before** the increment, so the very first drift (0 -> 1) aborted the
script: the `DRIFT DETECTED: N problem(s)` report was unreachable exactly when it
had something to say, findings could not aggregate, and `exit ${DRIFT_ERRORS}`
could only ever run with `0`.

Measured on the pre-fix script: one missing canonical truth file produced exit 1
with **completely empty stdout and stderr** -- the truth-file loop counts without
printing, so a required check went red and said nothing at all.

The controls below fail against the pre-fix form. A gate that cannot fail is not
a gate.
"""

import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_REL = "ops/scripts/what-matters-now.sh"

FAILURES = []


def check(desc, cond):
    print(("  ok   " if cond else "  FAIL ") + desc)
    if not cond:
        FAILURES.append(desc)


def git(cwd, *args):
    subprocess.run(
        ["git", "-c", "user.email=t@t", "-c", "user.name=t", *args],
        cwd=cwd,
        check=True,
        capture_output=True,
    )


def make_fixture(tmp):
    """A minimal repo the script will accept: a git checkout carrying `ops/`.

    The real `ops/` tree is copied rather than synthesised, so the fixture cannot
    drift from what the script actually inspects.
    """
    root = Path(tmp) / "repo"
    root.mkdir()
    git(root, "init", "-q", ".")
    shutil.copytree(REPO_ROOT / "ops", root / "ops", symlinks=True)
    git(root, "add", "-A")
    git(root, "commit", "-q", "-m", "fixture")
    return root


def run_drift(root):
    proc = subprocess.run(
        ["bash", SCRIPT_REL, "--drift"],
        cwd=root,
        capture_output=True,
        text=True,
    )
    return proc.returncode, proc.stdout + proc.stderr


print("--- a clean tree reports clean and exits 0 ---")
with tempfile.TemporaryDirectory() as tmp:
    root = make_fixture(tmp)
    rc, out = run_drift(root)
    check("clean tree exits 0 (rc=%d)" % rc, rc == 0)
    check("clean tree says so (%r)" % out.strip()[:60], "no drift detected" in out)

print()
print("--- one drift is reported, not silently aborted ---")
with tempfile.TemporaryDirectory() as tmp:
    root = make_fixture(tmp)
    (root / "ops/state/truth/agents.json").unlink()
    rc, out = run_drift(root)
    check("one drift exits non-zero (rc=%d)" % rc, rc != 0)
    # This is the assertion the pre-fix script could not satisfy: it exited 1
    # with no output whatsoever.
    check("the DRIFT DETECTED report is reached (output=%r)" % out.strip()[:80],
          "DRIFT DETECTED" in out)
    check("the report counts exactly one problem",
          re.search(r"DRIFT DETECTED: 1 problem", out) is not None)
    check("the report names the file that is missing, not just that some are",
          "ops/state/truth/agents.json" in out)

print()
print("--- findings aggregate; they did not before, because the first aborted ---")
with tempfile.TemporaryDirectory() as tmp:
    root = make_fixture(tmp)
    (root / "ops/state/truth/agents.json").unlink()
    (root / "ops/state/truth/skills.json").unlink()
    rc, out = run_drift(root)
    check("two drifts still exit non-zero (rc=%d)" % rc, rc != 0)
    check("the count aggregates to 2 (output=%r)" % out.strip()[:80],
          re.search(r"DRIFT DETECTED: 2 problem", out) is not None)
    check("both missing files are named",
          "ops/state/truth/agents.json" in out and "ops/state/truth/skills.json" in out)

print()
print("--- no counter in this script can abort under `set -e` ---")
script = (REPO_ROOT / SCRIPT_REL).read_text()

# `((VAR++))` / `((VAR--))` as a *command* is the hazard: its exit status is 1
# whenever the expression evaluates to 0. Assignment forms are safe.
UNSAFE = re.compile(r"^\s*\(\(\s*[A-Za-z_][A-Za-z0-9_]*\s*(\+\+|--)\s*\)\)", re.M)
UNSAFE_PREFIX = re.compile(r"^\s*\(\(\s*(\+\+|--)\s*[A-Za-z_][A-Za-z0-9_]*\s*\)\)", re.M)

check("the script sets -e (otherwise this whole class is moot)",
      re.search(r"^set -[a-z]*e", script, re.M) is not None)
check("no `((VAR++))` / `((VAR--))` command form remains (found %d)"
      % len(UNSAFE.findall(script)), not UNSAFE.search(script))
check("no `((++VAR))` / `((--VAR))` command form either (found %d)"
      % len(UNSAFE_PREFIX.findall(script)), not UNSAFE_PREFIX.search(script))
check("the drift counter is incremented by assignment",
      re.search(r"DRIFT_ERRORS=\$\(\(\s*DRIFT_ERRORS\s*\+\s*1\s*\)\)", script) is not None)

print()
print("--- MUST-FAIL controls (a gate that cannot fail is not a gate) ---")
PRE_FIX = "    ((DRIFT_ERRORS++))\n"
check("CONTROL: the matcher recognises the pre-fix increment",
      UNSAFE.search(PRE_FIX) is not None)
check("CONTROL: the matcher recognises a pre-increment variant",
      UNSAFE_PREFIX.search("  ((++N))\n") is not None)
check("CONTROL: the matcher does NOT flag the assignment form",
      UNSAFE.search("  N=$(( N + 1 ))\n") is None)

# The shell semantics this whole issue rests on, asserted rather than assumed.
probe = subprocess.run(
    ["bash", "-c", 'set -euo pipefail; N=0; ((N++)); echo "reached"'],
    capture_output=True, text=True,
)
check("CONTROL: `((N++))` with N=0 really does abort under `set -e` "
      "(rc=%d, stdout=%r)" % (probe.returncode, probe.stdout.strip()),
      probe.returncode != 0 and "reached" not in probe.stdout)

safe_probe = subprocess.run(
    ["bash", "-c", 'set -euo pipefail; N=0; N=$(( N + 1 )); echo "reached $N"'],
    capture_output=True, text=True,
)
check("CONTROL: the assignment form does not abort (rc=%d, stdout=%r)"
      % (safe_probe.returncode, safe_probe.stdout.strip()),
      safe_probe.returncode == 0 and "reached 1" in safe_probe.stdout)

print()
if FAILURES:
    print("test_what_matters_now_drift: %d check(s) FAILED" % len(FAILURES))
    for f in FAILURES:
        print("  - " + f)
    sys.exit(1)
print("test_what_matters_now_drift: all checks passed")
