#!/usr/bin/env python3
"""Executable consumer + regression gate for `what-matters-now.sh --json` (icn#2638).

`--json` was advertised in the script's own usage header but invoked by no workflow,
skill or hook, so its 100% failure rate looked like silence. It built its payload by
splicing shell values into a Python heredoc, which made every field have to be valid
*Python source*. Two distinct defects followed from that one root cause:

  1. shell `true`/`false` arrived as the bare Python names `true`/`false`
     -> NameError: name 'true' is not defined      (the reported bug, 100% of runs)
  2. a *legal* git branch name containing `"` closed the string early
     -> SyntaxError: unterminated string literal   (reachable, never reported)

The fix passes every value by environment, so a value can no longer be syntax. This
file is the runner that mode never had, plus a structural gate that stops the class
from coming back. Structural, not behavioural: asserting "no interpolation reaches the
Python body" forbids defect 2 without needing a checkout on a hostile branch name.

Run: python3 scripts/tests/test_what_matters_now_json.py
"""
import json
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "ops" / "scripts" / "what-matters-now.sh"

failures = []


def check(desc, cond):
    print(("  ok   " if cond else "  FAIL ") + desc)
    if not cond:
        failures.append(desc)


def json_heredoc_body(text):
    """Return (delimiter, body) for the --json payload heredoc, or (None, None)."""
    m = re.search(r"python3 - <<(['\"]?)([A-Z_][A-Z0-9_]*)\1\n(.*?)\n\2\n", text, re.S)
    if not m:
        return None, None
    return m.group(1), m.group(3)


print("--- the mode runs at all (this is the consumer it never had) ---")

proc = subprocess.run(
    ["bash", str(SCRIPT), "--json"],
    capture_output=True, text=True, cwd=str(ROOT),
)

check("--json emits bytes on stdout", len(proc.stdout) > 0)

payload = None
try:
    payload = json.loads(proc.stdout)
    check("--json stdout is parseable JSON", True)
except ValueError as exc:
    check("--json stdout is parseable JSON (%s; stderr: %s)"
          % (exc, proc.stderr.strip().splitlines()[-1:] or ""), False)

if payload is not None:
    # The documented contract: --json exits with the drift count it reports.
    check("exit status equals the drift_errors it reports (%d == %d)"
          % (proc.returncode, payload["drift_errors"]),
          proc.returncode == payload["drift_errors"])

    # The reported defect, stated as a type assertion. A string "true" would parse
    # fine as JSON and still be wrong, so parseability alone does not cover this.
    for key in ("truth_files_ok", "symlinks_ok"):
        check("%s is a JSON boolean, not a string (%r)" % (key, payload[key]),
              isinstance(payload[key], bool))

    check("drift_errors is an integer (%r)" % (payload["drift_errors"],),
          isinstance(payload["drift_errors"], int)
          and not isinstance(payload["drift_errors"], bool))

    # #2636 added these and --json is their only consumer, so nothing else exercises
    # them end to end.
    check("sprint block is present with its three cadence fields",
          isinstance(payload.get("sprint"), dict)
          and {"summary", "active_sprint", "status"} <= set(payload["sprint"]))

    check("active_sprint is null or a scalar, never the string 'None'",
          payload["sprint"]["active_sprint"] != "None")

    check("current_work_owner routes to the live query, not to a file",
          "live_issue_state" in payload.get("current_work_owner", ""))

    for key in ("repo_root", "workspace_root", "branch", "canonical_truth"):
        check("payload carries %s" % key, isinstance(payload.get(key), str)
              and payload[key] != "")


print()
print("--- the defect class cannot return (structural) ---")

text = SCRIPT.read_text(encoding="utf-8")
delim_quote, body = json_heredoc_body(text)

check("the --json payload heredoc was located", body is not None)

if body is not None:
    check("its delimiter is quoted, so the shell does not expand the body "
          "(delimiter quote=%r)" % (delim_quote,), delim_quote in ("'", '"'))
    check("no ${...} parameter expansion reaches the Python body",
          "${" not in body)
    check("no $(...) command substitution reaches the Python body",
          "$(" not in body)
    check("the two reported booleans are read from the environment, not spliced",
          'flag("WMN_TRUTH_OK")' in body and 'flag("WMN_SYMLINKS_OK")' in body)


print()
print("--- MUST-FAIL controls (a gate that cannot fail is not a gate) ---")

# Reconstruct the real pre-fix form and prove each structural assertion above would
# have rejected it. Without these, the gate could be passing vacuously.
PRE_FIX = (
    'python3 - <<EOF\n'
    'import json\n'
    'data = {\n'
    '  "branch": "${BRANCH}",\n'
    '  "truth_files_ok": $([[ "${TRUTH_OK}" == "true" ]] && echo "true"'
    ' || echo "false"),\n'
    '}\n'
    'EOF\n'
)
pre_quote, pre_body = json_heredoc_body(PRE_FIX)
check("CONTROL: the pre-fix heredoc is still recognised by the matcher",
      pre_body is not None)
check("CONTROL: the unquoted-delimiter assertion rejects the pre-fix form",
      pre_quote == "")
check("CONTROL: the ${...} assertion rejects the pre-fix form",
      pre_body is not None and "${" in pre_body)
check("CONTROL: the $(...) assertion rejects the pre-fix form",
      pre_body is not None and "$(" in pre_body)

# And prove the pre-fix form really did fail, rather than trusting the bug report.
pre_run = subprocess.run(
    ["bash", "-c", 'set -euo pipefail\nBRANCH=main\nTRUTH_OK=true\n' + PRE_FIX],
    capture_output=True, text=True,
)
check("CONTROL: the pre-fix form dies with NameError on the shell boolean",
      pre_run.returncode != 0 and "NameError" in pre_run.stderr
      and "'true'" in pre_run.stderr)

# Defect 2: a branch name git itself accepts. Not hypothetical -- `git check-ref-format`
# permits `"`, so this was reachable on any checkout that used such a branch.
ref_ok = subprocess.run(
    ["git", "check-ref-format", "--branch", 'quote"branch'],
    capture_output=True, text=True,
)
check("CONTROL: git accepts a branch name containing a double quote",
      ref_ok.returncode == 0)

pre_run2 = subprocess.run(
    ["bash", "-c",
     'set -euo pipefail\nBRANCH=\'quote"branch\'\nTRUTH_OK=true\n' + PRE_FIX],
    capture_output=True, text=True,
)
check("CONTROL: the pre-fix form dies with SyntaxError on that legal branch name",
      pre_run2.returncode != 0 and "SyntaxError" in pre_run2.stderr)


print()
print("--- branch is reported truthfully on a detached HEAD (CI's own checkout) ---")

# actions/checkout leaves a detached HEAD on pull_request events. `git branch
# --show-current` exits 0 there and prints nothing, so the script's original
# `|| echo "unknown"` fallback was dead code and --json shipped "branch": "" to every
# consumer. This was invisible until --json got the runner icn#2638 asked for.
tmp = tempfile.mkdtemp()
try:
    def git(*args):
        return subprocess.run(("git", "-C", tmp) + args,
                              capture_output=True, text=True)

    subprocess.run(["git", "init", "-q", tmp], capture_output=True)
    git("config", "user.email", "t@example.invalid")
    git("config", "user.name", "t")
    git("commit", "-q", "--allow-empty", "-m", "one")
    git("checkout", "-q", "--detach", "HEAD")

    probe = git("branch", "--show-current")
    check("CONTROL: on a detached HEAD `git branch --show-current` exits 0 and prints "
          "nothing, so an `|| echo` fallback on it is dead code",
          probe.returncode == 0 and probe.stdout.strip() == "")

    # The whole script cannot run outside a real checkout -- it derives REPO_ROOT from
    # its own location and aborts early under `set -euo pipefail`, which icn#2638 records
    # as a trap that masks defects. So extract the SHIPPED branch-resolution block and
    # execute that verbatim against the detached repo: real code, no re-implementation.
    block = re.search(
        r"^BRANCH=\$\(git -C .*?\n(?:.*?\n)*?^fi$",
        SCRIPT.read_text(encoding="utf-8"), re.M)
    check("the shipped branch-resolution block was located", block is not None)

    if block is not None:
        det = subprocess.run(
            ["bash", "-c",
             'set -euo pipefail\nREPO_ROOT="$1"\n' + block.group(0) + '\nprintf "%s" "$BRANCH"',
             "_", tmp],
            capture_output=True, text=True,
        )
        branch = det.stdout
        check("branch resolution succeeds on a detached HEAD (rc=%d, stderr=%r)"
              % (det.returncode, det.stderr.strip()[:80]), det.returncode == 0)
        check('branch is non-empty on a detached HEAD -- CI emitted "" here -- got %r'
              % (branch,), branch.strip() != "")
        check("branch says it is detached rather than naming a branch that is not "
              "checked out -- got %r" % (branch,), "detached" in branch)

        # Control: the same block on an attached checkout still names the branch.
        subprocess.run(["git", "-C", tmp, "checkout", "-q", "-b", "some-branch"],
                       capture_output=True)
        att = subprocess.run(
            ["bash", "-c",
             'set -euo pipefail\nREPO_ROOT="$1"\n' + block.group(0) + '\nprintf "%s" "$BRANCH"',
             "_", tmp],
            capture_output=True, text=True,
        )
        check("CONTROL: an attached checkout still reports its real branch name "
              "(got %r) -- the fix did not replace branches with 'detached'"
              % (att.stdout,), att.stdout.strip() == "some-branch")
finally:
    shutil.rmtree(tmp, ignore_errors=True)


print()
if failures:
    print("test_what_matters_now_json: %d FAILURE(S)" % len(failures))
    for f in failures:
        print("  - " + f)
    sys.exit(1)
print("test_what_matters_now_json: all checks passed")
