#!/usr/bin/env python3
"""Executable consumer + regression gate for the checkout-path interpolation in
`what-matters-now.sh` (icn#2722).

`#2688` closed this defect class on the `--json` payload path by passing values
through the environment. The *same class* survived a few lines above it: the
sprint and policy lookups splice the checkout path into **Python source** inside
single-quoted string literals, e.g.

    d = json.load(open('${SPRINT_FILE}'))

`SPRINT_FILE` is `${REPO_ROOT}/ops/state/sprint/current.json`, and `REPO_ROOT`
is the checkout directory, derived from the script's own location. A single
quote is a legal POSIX path component, so a checkout path containing one closes
the literal early and the remainder is parsed as Python. Every site is wrapped
in `2>/dev/null || echo "<fallback>"`, so the failure is silent: the preflight
reports `unresolved (sprint owner unreadable)` for a file it can read perfectly
well, and the `required checks` line vanishes -- the same "a control reports a
conclusion its evidence path never produced" family as #2723.

This file is two things at once:

  1. a behavioural runner that drives the script from a checkout path containing
     a `'` and proves the report is correct there -- the case no other test
     exercises; and
  2. a structural gate that forbids any shell value from reaching Python as
     *syntax* anywhere in the script, so the class cannot come back on a fourth
     site.

Both are backed by MUST-FAIL controls: the pre-fix form is reconstructed and
each assertion is shown to reject it, and the pre-fix form is shown at runtime
to both misreport (benign path) and execute chosen code (the ceiling) when the
checkout path is hostile. A gate that cannot fail is not a gate.

Run: python3 scripts/tests/test_what_matters_now_sprint_path.py
"""
import json
import pathlib
import re
import shutil
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "ops" / "scripts" / "what-matters-now.sh"

failures = []


def check(desc, cond):
    print(("  ok   " if cond else "  FAIL ") + desc)
    if not cond:
        failures.append(desc)


# ── fixture: a self-contained checkout under a chosen directory name ──────────


STATE_FILES = [
    "ops/state/sprint/current.json",
    "ops/state/truth/sources.json",
    "ops/state/truth/policy.json",
    "ops/state/truth/agents.json",
    "ops/state/truth/skills.json",
    "ops/state/config/repo-map.json",
]


def build_checkout(parent, dirname):
    """Materialise a minimal but real checkout at `parent/dirname` and return it.

    The directory name is the whole point of the test, so it is passed in
    verbatim. Only the files the script actually reads are copied; it is then a
    git repo so the early `git` calls under `set -e` do not abort before the
    sprint section is reached.
    """
    root = pathlib.Path(parent) / dirname
    (root / "ops" / "scripts").mkdir(parents=True)
    shutil.copy2(SCRIPT, root / "ops" / "scripts" / SCRIPT.name)
    for rel in STATE_FILES:
        src = ROOT / rel
        if not src.exists():
            continue
        dst = root / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)
    subprocess.run(["git", "init", "-q", str(root)], capture_output=True)
    subprocess.run(["git", "-C", str(root), "config", "user.email", "t@t.invalid"],
                   capture_output=True)
    subprocess.run(["git", "-C", str(root), "config", "user.name", "t"],
                   capture_output=True)
    subprocess.run(["git", "-C", str(root), "add", "-A"], capture_output=True)
    subprocess.run(["git", "-C", str(root), "commit", "-qm", "init"],
                   capture_output=True)
    return root


def run_script(checkout, *args, cwd=None):
    return subprocess.run(
        ["bash", str(checkout / "ops" / "scripts" / SCRIPT.name), *args],
        capture_output=True, text=True, cwd=cwd,
    )


def sprint_line(human_output):
    """The cadence line the script prints under '── Sprint cadence ──'."""
    lines = human_output.splitlines()
    for i, ln in enumerate(lines):
        if "Sprint cadence" in ln and i + 1 < len(lines):
            return lines[i + 1].strip()
    return ""


# ── behavioural: the report is correct from a quoted checkout path ────────────


print("--- the report does not depend on a ' in the checkout path (icn#2722) ---")

expected_sprint = None
try:
    d = json.loads((ROOT / "ops" / "state" / "sprint" / "current.json").read_text())
    if d.get("cadence") == "dormant" or d.get("active_sprint") is None:
        expected_sprint = "no active sprint (cadence dormant)"
    else:
        expected_sprint = "Sprint %s (%s)" % (d.get("active_sprint"), d.get("status", "?"))
except Exception as exc:  # pragma: no cover - fixture integrity
    check("CONTROL: the real sprint file parses (%s)" % exc, False)

tmp = tempfile.mkdtemp()
try:
    clean = build_checkout(tmp, "clean-repo")
    quoted = build_checkout(tmp, "it's-a-repo")

    clean_run = run_script(clean)
    quoted_run = run_script(quoted)

    clean_sprint = sprint_line(clean_run.stdout)
    quoted_sprint = sprint_line(quoted_run.stdout)

    check("baseline: a clean path reports the sprint cadence correctly (%r)"
          % clean_sprint, clean_sprint == expected_sprint)

    # The discriminating assertion. Against current `main` the quoted path yields
    # "unresolved (sprint owner unreadable)" because the `'` breaks the Python
    # literal and the `|| echo` fallback fires; the file is perfectly readable.
    check("a checkout path containing ' reports the SAME sprint cadence (%r)"
          % quoted_sprint, quoted_sprint == expected_sprint)

    check("a checkout path containing ' does not report the file unreadable",
          "unreadable" not in quoted_sprint)

    # The `required checks` line comes from POLICY_CHECKS (site 5, ${REPO_ROOT}
    # spliced directly). It disappears entirely on a quoted path today.
    check("the 'required checks' count is reported from a quoted path too",
          "required checks" in quoted_run.stdout)

    # --json must remain valid from a quoted path: its sprint fields are computed
    # by these same interpolated lookups upstream of #2688's env boundary.
    qjson = run_script(quoted, "--json")
    try:
        payload = json.loads(qjson.stdout)
        check("--json is valid from a quoted checkout path", True)
        check("--json sprint.status is truthful from a quoted path (%r)"
              % payload.get("sprint", {}).get("status"),
              payload.get("sprint", {}).get("status") == d.get("status", "?"))
    except ValueError as exc:
        check("--json is valid from a quoted checkout path (%s)" % exc, False)
finally:
    shutil.rmtree(tmp, ignore_errors=True)


# ── structural: no shell value reaches Python as syntax, anywhere ────────────


print()
print("--- the defect class cannot return (structural, whole file) ---")

text = SCRIPT.read_text(encoding="utf-8")


def inline_python_sources(s):
    """Every `python3 -c "<source>"` body in the script.

    The source is the double-quoted word after `-c`; any path argument the fixed
    form appends comes *after* that closing quote and is therefore not captured,
    which is exactly the property under test.

    Coverage boundary (deliberate). This matches the two invocation shapes that
    can carry a shell value into Python *as syntax*: a double-quoted `-c` body,
    and (below) an unquoted heredoc. The shapes it does not match cannot
    interpolate a shell value: a single-quoted `python3 -c '...'` body and a
    `<<'EOF'` heredoc do not expand `${...}` at all, and `python3 script.py
    "$path"` passes the value as argv — which is the safe form this fix adopts.
    Scanning single-quoted bodies for `${` would in fact be wrong: there a `${x}`
    is a literal string, not an injection. So the gate covers every shape that is
    dangerous and skips the shapes that are safe by construction.
    """
    return re.findall(r'python3 -c "((?:[^"\\]|\\.)*)"', s, re.S)


def interpolating_python_heredocs(s):
    """Bodies of `python3 - <<DELIM ... DELIM` whose delimiter is UNQUOTED.

    A quoted delimiter (`<<'PY'`) does not expand, so those are safe and skipped.
    """
    out = []
    for m in re.finditer(r"python3 - <<(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\1\n(.*?)\n\2\n",
                         s, re.S):
        if m.group(1) == "":  # unquoted delimiter -> shell expands the body
            out.append(m.group(3))
    return out


inline = inline_python_sources(text)
check("at least the sprint/policy python3 -c calls were located (%d)" % len(inline),
      len(inline) >= 3)

bad_inline = [src for src in inline if "${" in src or "$(" in src]
check("no python3 -c source string contains ${...} or $(...) interpolation",
      not bad_inline)

hostile_heredocs = interpolating_python_heredocs(text)
check("no python3 heredoc uses an unquoted (expanding) delimiter",
      not hostile_heredocs)

# Belt and suspenders: the specific pre-fix spelling must be gone from the file.
check("no `open('${...}')` / `open(\"${...}\")` remains anywhere in the script",
      not re.search(r"open\(\s*['\"]\$\{", text))


# ── the argv change did not break the genuine "unreadable" fallback ──────────


print()
print("--- the || fallback still fires for a genuinely unreadable file ---")

# The fix moved the path from source to argv; the `2>/dev/null || echo` fallback
# must still cover a real missing/corrupt file, or the change would trade a false
# "unreadable" for a false "readable". Exercised at the shell level with the exact
# fixed spelling, so it pins the post-fix form, not a paraphrase.
FIXED_INLINE = (
    'SPRINT_STATUS=$(python3 -c "import json,sys; '
    "d=json.load(open(sys.argv[1])); print(d.get('status','?'))\" "
    '"${SPRINT_FILE}" 2>/dev/null || echo "?")'
)


def fixed_status(sprint_file_expr, setup=""):
    return subprocess.run(
        ["bash", "-c",
         "set -uo pipefail\n" + setup
         + 'SPRINT_FILE=%s\n' % sprint_file_expr
         + FIXED_INLINE + '\necho "STATUS=${SPRINT_STATUS}"'],
        capture_output=True, text=True,
    )

missing = fixed_status('"/nonexistent/really-not-here.json"')
check("the fixed argv form still falls back to '?' when the file is missing",
      "STATUS=?" in missing.stdout)

corrupt_dir = tempfile.mkdtemp()
try:
    bad = pathlib.Path(corrupt_dir) / "current.json"
    bad.write_text("{not valid json")
    corrupt = fixed_status('"%s"' % bad)
    check("the fixed argv form still falls back to '?' on corrupt JSON",
          "STATUS=?" in corrupt.stdout)

    good = pathlib.Path(corrupt_dir) / "good.json"
    good.write_text('{"status": "closed"}')
    ok = fixed_status('"%s"' % good)
    check("the fixed argv form reports the real status on a readable file",
          "STATUS=closed" in ok.stdout)
finally:
    shutil.rmtree(corrupt_dir, ignore_errors=True)


# ── MUST-FAIL controls: prove the gate rejects the pre-fix form ──────────────


print()
print("--- MUST-FAIL controls (a gate that cannot fail is not a gate) ---")

PRE_FIX_INLINE = (
    'SPRINT_STATUS=$(python3 -c "import json; '
    "d=json.load(open('${SPRINT_FILE}')); print(d.get('status','?'))\" "
    '2>/dev/null || echo "?")'
)
pre_inline = inline_python_sources(PRE_FIX_INLINE)
check("CONTROL: the matcher still recognises the pre-fix python3 -c form",
      len(pre_inline) == 1)
check("CONTROL: the ${...} assertion rejects the pre-fix inline form",
      pre_inline and "${" in pre_inline[0])

check("CONTROL: the open('${...}') assertion rejects the pre-fix form",
      bool(re.search(r"open\(\s*['\"]\$\{", PRE_FIX_INLINE)))

PRE_FIX_HEREDOC = (
    "python3 - <<EOF\n"
    "import json\n"
    "d = json.load(open('${SPRINT_FILE}'))\n"
    "EOF\n"
)
check("CONTROL: the heredoc matcher flags an unquoted expanding delimiter",
      len(interpolating_python_heredocs(PRE_FIX_HEREDOC)) == 1)

# Runtime proof #1 (benign): the pre-fix form misreports on a path with a `'`.
benign = subprocess.run(
    ["bash", "-c",
     "set -uo pipefail\n"
     "SPRINT_FILE=\"/tmp/it's-a-repo/current.json\"\n" + PRE_FIX_INLINE + "\n"
     'echo "STATUS=${SPRINT_STATUS}"'],
    capture_output=True, text=True,
)
check("CONTROL: the pre-fix form falls back to '?' when the path contains a ' "
      "(silent misreport)",
      "STATUS=?" in benign.stdout)

# Runtime proof #2 (ceiling): a crafted path component executes chosen code.
# All characters below are legal in a POSIX path component.
ceil_dir = tempfile.mkdtemp()
try:
    marker = pathlib.Path(ceil_dir) / "ICN2722_MARKER"
    payload_path = (
        "/x'+__import__('pathlib').Path('%s').write_text('x')*0*0+'y/current.json"
        % marker
    )
    ceil = subprocess.run(
        ["bash", "-c",
         "set -uo pipefail\n"
         "SPRINT_FILE=\"%s\"\n" % payload_path + PRE_FIX_INLINE],
        capture_output=True, text=True,
    )
    check("CONTROL: the pre-fix form EXECUTES code embedded in the checkout path "
          "(the ceiling of this class)", marker.exists())
finally:
    shutil.rmtree(ceil_dir, ignore_errors=True)


# ── summary ──────────────────────────────────────────────────────────────────


print()
if failures:
    print("FAILED (%d):" % len(failures))
    for f in failures:
        print("  - " + f)
    raise SystemExit(1)
print("all checks passed")
