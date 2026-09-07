#!/usr/bin/env python3
"""Executable consumer + regression gate for checkout-path interpolation in the
ICN operational scripts (icn#2722).

#2688 closed this defect class on the `--json` payload of `what-matters-now.sh`
by passing values through the environment. The *same class* survived elsewhere:
several operational scripts spliced the checkout path into **Python source**
inside single-quoted string literals, e.g.

    d = json.load(open('${SPRINT_FILE}'))      # ${REPO_ROOT}/ops/state/sprint/...
    d = json.load(open('${REGISTRY}'))         # ${REPO_ROOT}/ops/state/truth/...

`REPO_ROOT` is the checkout directory. A single quote is a legal POSIX path
component, so a checkout path containing one closes the literal early and the
remainder is parsed as Python.

Where the call is wrapped in `2>/dev/null || echo "<fallback>"` the failure is
silent: the preflight reported `unresolved (sprint owner unreadable)` for a file
it can read perfectly well and dropped the `required checks` line -- the same "a
control states a conclusion its evidence path never produced" family as #2723.

The operating rule this file enforces:

    the program is code; paths, names and registry locations are DATA.
    data reaches Python through argv/env/stdin, never as syntax.

Scope. Three operational scripts are in scope, all of which derive a path from
the checkout root:

    ops/scripts/what-matters-now.sh      (5 sites)
    ops/scripts/drift-check.sh           (4 sites)
    ops/scripts/setup-skill-symlinks.sh  (1 site)

Demo/rehearsal scripts under scripts/ are deliberately NOT covered: their
interpolated values are author-written Python fragments or script-generated
quote-free constants, a different provenance. The two that splice a
token-derived DID are recorded as separate bounded debt, not silently cleared.

Run: python3 scripts/tests/test_what_matters_now_sprint_path.py
"""
import json
import pathlib
import re
import shutil
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
OPS = ROOT / "ops" / "scripts"
SCRIPT = OPS / "what-matters-now.sh"
DRIFT = OPS / "drift-check.sh"
SYMLINKS = OPS / "setup-skill-symlinks.sh"

# Every operational script whose Python source must stay free of shell values.
IN_SCOPE = [SCRIPT, DRIFT, SYMLINKS]

failures = []


def check(desc, cond):
    print(("  ok   " if cond else "  FAIL ") + desc)
    if not cond:
        failures.append(desc)


# ── fixture ──────────────────────────────────────────────────────────────────


STATE_FILES = [
    "ops/state/sprint/current.json",
    "ops/state/truth/sources.json",
    "ops/state/truth/policy.json",
    "ops/state/truth/agents.json",
    "ops/state/truth/skills.json",
    "ops/state/config/repo-map.json",
]


def git(*args):
    """Run a git fixture command FAIL-CLOSED.

    A fixture that half-built used to be indistinguishable from a passing test:
    the scripts under test bail early on a non-repo, so the interesting code was
    never reached and the run still looked green. Setup failures must be fatal,
    never evidence.
    """
    subprocess.run(args, capture_output=True, check=True)


def build_checkout(parent, dirname, scripts=None):
    """Materialise a minimal but real checkout at `parent/dirname`.

    The directory name is the whole point of the test, so it is passed verbatim.

    The `.claude/agents` and `../.claude/skills` trees below are NOT decoration.
    `drift-check.sh` guards two of the four sites this change fixes behind
    `[[ -d "${PROJECT_SKILLS}" ]]` (:93) and `[[ -d "${AGENTS_DIR}" ]]` (:290).
    Without these trees both guards take the skip branch, the `${REGISTRY}` and
    `${AGENTS_FILE}` reads never execute, and an assertion about them would pass
    against the pre-fix spelling too -- a control whose evidence path never runs,
    which is the exact defect family this file exists to forbid.
    """
    root = pathlib.Path(parent) / dirname
    (root / "ops" / "scripts").mkdir(parents=True)
    for s in (scripts or IN_SCOPE):
        shutil.copy2(s, root / "ops" / "scripts" / s.name)
    for rel in STATE_FILES:
        src = ROOT / rel
        # Fail closed, for the same reason git() does: a thinner fixture must
        # never silently become weaker evidence.
        if not src.exists():
            raise SystemExit("fixture: required state file missing: %s" % rel)
        dst = root / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)

    canonical_skills = root / "ops" / "automation" / "skills"
    canonical_skills.mkdir(parents=True, exist_ok=True)

    # Drive the agents-registry read (drift-check.sh:291): one name that IS in the
    # registry and one that is not, so both branches of the comparison execute.
    agents_dir = root / ".claude" / "agents"
    agents_dir.mkdir(parents=True)
    registry_agents = sorted(
        a["name"] for a in json.loads(
            (ROOT / "ops" / "state" / "truth" / "agents.json").read_text())["agents"])
    for name in registry_agents[:3]:
        (agents_dir / ("%s.md" % name)).write_text("# %s\n" % name)
    (agents_dir / "unregistered-extra.md").write_text("# unregistered-extra\n")

    # Drive the symlink-registry read (drift-check.sh:95): PROJECT_SKILLS is
    # ${REPO_ROOT}/../.claude/skills, i.e. inside `parent` -- never the real one.
    project_skills = pathlib.Path(parent) / ".claude" / "skills"
    project_skills.mkdir(parents=True, exist_ok=True)
    registry_skills = [e["name"] for e in json.loads(
        (ROOT / "ops" / "state" / "truth" / "skills.json").read_text()
    )["skills"]["ops_automation_canonical"]]
    if registry_skills:
        # one correct symlink and one plain directory, so both arms are exercised
        (canonical_skills / registry_skills[0]).mkdir(parents=True, exist_ok=True)
        link = project_skills / registry_skills[0]
        if not link.exists():
            link.symlink_to(canonical_skills / registry_skills[0])
        if len(registry_skills) > 1:
            (project_skills / registry_skills[1]).mkdir(parents=True, exist_ok=True)

    git("git", "init", "-q", str(root))
    git("git", "-C", str(root), "config", "user.email", "t@example.invalid")
    git("git", "-C", str(root), "config", "user.name", "t")
    git("git", "-C", str(root), "add", "-A")
    git("git", "-C", str(root), "commit", "-qm", "init")

    # Pin the isolation the whole behavioural section rests on. If REPO_ROOT
    # resolution ever escaped the fixture, `plain` and `quoted` would resolve to
    # the SAME real root and every check below would still pass -- green for a
    # reason that has nothing to do with the quote.
    top = subprocess.run(["git", "-C", str(root), "rev-parse", "--show-toplevel"],
                         capture_output=True, text=True, check=True).stdout.strip()
    if pathlib.Path(top).resolve() != root.resolve():
        raise SystemExit("fixture: git toplevel %r escaped the fixture root %r"
                         % (top, str(root)))
    return root


def run_script(checkout, name, *args, cwd=None):
    return subprocess.run(
        ["bash", str(checkout / "ops" / "scripts" / name), *args],
        capture_output=True, text=True, cwd=cwd or str(checkout),
    )


def sprint_line(human_output):
    lines = human_output.splitlines()
    for i, ln in enumerate(lines):
        if "Sprint cadence" in ln and i + 1 < len(lines):
            return lines[i + 1].strip()
    return ""


PLAIN_DIR = "plain-repo"
QUOTED_DIR = "it's-a-repo"   # the only difference that may change any behaviour


# ── behavioural: what-matters-now.sh from a quoted checkout path ─────────────


print("--- what-matters-now.sh: report does not depend on a ' in the path ---")

sprint_doc = json.loads((ROOT / "ops" / "state" / "sprint" / "current.json").read_text())
if sprint_doc.get("cadence") == "dormant" or sprint_doc.get("active_sprint") is None:
    expected_sprint = "no active sprint (cadence dormant)"
else:
    expected_sprint = "Sprint %s (%s)" % (sprint_doc.get("active_sprint"),
                                          sprint_doc.get("status", "?"))

tmp = tempfile.mkdtemp()
try:
    plain = build_checkout(tmp, PLAIN_DIR)
    quoted = build_checkout(tmp, QUOTED_DIR)

    plain_run = run_script(plain, SCRIPT.name)
    quoted_run = run_script(quoted, SCRIPT.name)

    check("baseline: a plain path reports the sprint cadence correctly (%r)"
          % sprint_line(plain_run.stdout),
          sprint_line(plain_run.stdout) == expected_sprint)

    check("a checkout path containing ' reports the SAME sprint cadence (%r)"
          % sprint_line(quoted_run.stdout),
          sprint_line(quoted_run.stdout) == expected_sprint)

    check("a checkout path containing ' does not claim the file is unreadable",
          "unreadable" not in sprint_line(quoted_run.stdout))

    # POLICY_CHECKS (the ${REPO_ROOT} site) drops this line entirely pre-fix.
    check("the 'required checks' count is reported from a quoted path too",
          "required checks" in quoted_run.stdout)

    qjson = run_script(quoted, SCRIPT.name, "--json")
    try:
        payload = json.loads(qjson.stdout)
        check("--json is valid from a quoted checkout path", True)
        check("--json sprint.status is truthful from a quoted path (%r)"
              % payload.get("sprint", {}).get("status"),
              payload.get("sprint", {}).get("status") == sprint_doc.get("status", "?"))
    except ValueError as exc:
        check("--json is valid from a quoted checkout path (%s)" % exc, False)

    # ── behavioural: drift-check.sh from a quoted checkout path ──────────────

    print()
    print("--- drift-check.sh: registry/policy/agents reads survive a quoted path ---")

    d_plain = run_script(plain, DRIFT.name, "--verbose")
    d_quoted = run_script(quoted, DRIFT.name, "--verbose")
    d_out = d_quoted.stdout + d_quoted.stderr

    # skills.json -> scan scope (sites at :27 and :89)
    check("skill scan scope is still derived from the registry on a quoted path",
          "could not derive skill scan scope" not in d_out)

    # policy.json -> required-check count (site at :260)
    expected_checks = len(json.loads(
        (ROOT / "ops" / "state" / "truth" / "policy.json").read_text()
    )["merge"]["required_checks"])
    check("policy.json required-check count reads correctly on a quoted path "
          "(expect %d)" % expected_checks,
          ("policy.json has %d required checks" % expected_checks) in d_out)
    check("the quoted path does not trigger the 'fewer than 11 required checks' fail",
          "fewer than 11 required checks" not in d_out)

    # agents.json -> registered agent names (site at :291). Reading it wrongly
    # yields an EMPTY name set, which turns every "Agent registered" into
    # "Agent not in registry" -- so assert the positive line, not just equality.
    registry_agents = sorted(
        a["name"] for a in json.loads(
            (ROOT / "ops" / "state" / "truth" / "agents.json").read_text())["agents"])
    check("agents registry resolves registered names on a quoted path (%r)"
          % registry_agents[:3],
          all(("Agent registered: %s" % n) in d_out for n in registry_agents[:3]))
    check("the agents read still detects a genuinely unregistered agent",
          "Agent not in registry: unregistered-extra" in d_out)

    # skills.json -> symlink names (site at :95). An empty read means the loop
    # body never runs, so the plain-directory drift below would go unreported.
    check("the symlink-registry read reports the machine-local plain directory",
          "plain directory" in d_out or "NOT SYMLINK" in d_out)

    check("drift-check.sh reaches the same overall verdict on both paths "
          "(plain exit=%d, quoted exit=%d)" % (d_plain.returncode, d_quoted.returncode),
          d_plain.returncode == d_quoted.returncode)

    # ── behavioural: setup-skill-symlinks.sh from a quoted checkout path ─────

    print()
    print("--- setup-skill-symlinks.sh --check: registry loads on a quoted path ---")

    # SKILLS_DIR is ${REPO_ROOT}/../.claude/skills, i.e. INSIDE this temp dir --
    # the developer's real machine-local .claude/skills is never touched. --check
    # never creates anything either.
    registered = [e["name"] for e in json.loads(
        (ROOT / "ops" / "state" / "truth" / "skills.json").read_text()
    )["skills"]["ops_automation_canonical"]]

    s_quoted = run_script(quoted, SYMLINKS.name, "--check")
    s_out = s_quoted.stdout + s_quoted.stderr

    # Pre-fix the json.load dies, SKILLS is empty, and the script exits on its
    # own "no ops-automation skills registered" error without ever naming one.
    check("the registry is not reported empty on a quoted path",
          "no ops-automation skills registered" not in s_out)
    check("every registered skill name is still enumerated on a quoted path (%r)"
          % registered,
          all(n in s_out for n in registered) and len(registered) > 0)
    # Isolation is pinned by build_checkout()'s toplevel assertion, which proves
    # REPO_ROOT resolves inside the fixture -- so SKILLS_DIR
    # (${REPO_ROOT}/../.claude/skills) is the fixture's, not the developer's. An
    # earlier `not (...__probe__).exists()` line here was decoration: nothing ever
    # created __probe__, so the assertion could not fail.
    check("--check is read-only: it reported problems without exiting 0 (exit=%d)"
          % s_quoted.returncode, s_quoted.returncode != 0)
finally:
    shutil.rmtree(tmp, ignore_errors=True)


# ── the argv change did not break the genuine "unreadable" fallback ──────────


PRE_FIX_INLINE = (
    'STATUS=$(python3 -c "import json; '
    "d=json.load(open('${SPRINT_FILE}')); print(d.get('status','?'))\" "
    '2>/dev/null || echo "?")'
)
FIXED_INLINE = (
    'STATUS=$(python3 -c "import json,sys; '
    "d=json.load(open(sys.argv[1])); print(d.get('status','?'))\" "
    '"${SPRINT_FILE}" 2>/dev/null || echo "?")'
)


def status_of(form, sprint_file):
    return subprocess.run(
        ["bash", "-c",
         "set -uo pipefail\nSPRINT_FILE=%s\n" % json.dumps(str(sprint_file))
         + form + '\necho "RESULT=${STATUS}"'],
        capture_output=True, text=True,
    ).stdout


print()
print("--- the || fallback still fires for a genuinely unreadable file ---")

fb = tempfile.mkdtemp()
try:
    good = pathlib.Path(fb) / "good.json"
    good.write_text('{"status": "closed"}')
    bad = pathlib.Path(fb) / "corrupt.json"
    bad.write_text("{not valid json")

    check("fixed form reports the real status on a readable file",
          "RESULT=closed" in status_of(FIXED_INLINE, good))
    check("fixed form still falls back to '?' when the file is missing",
          "RESULT=?" in status_of(FIXED_INLINE, pathlib.Path(fb) / "absent.json"))
    check("fixed form still falls back to '?' on corrupt JSON",
          "RESULT=?" in status_of(FIXED_INLINE, bad))
finally:
    shutil.rmtree(fb, ignore_errors=True)


# ── MUST-FAIL controls ───────────────────────────────────────────────────────


print()
print("--- MUST-FAIL controls (a gate that cannot fail is not a gate) ---")

# The benign control isolates THE QUOTE as the only variable: identical, readable
# JSON content, reached by two paths that differ in one character class. An
# earlier version pointed at a non-existent file, so plain FileNotFoundError
# produced the same fallback and the control proved nothing (live review).
iso = tempfile.mkdtemp()
try:
    plain_dir = pathlib.Path(iso) / "plain"
    quote_dir = pathlib.Path(iso) / "it's"
    plain_dir.mkdir()
    quote_dir.mkdir()
    for d in (plain_dir, quote_dir):
        (d / "current.json").write_text('{"status": "closed"}')

    pre_plain = status_of(PRE_FIX_INLINE, plain_dir / "current.json")
    pre_quote = status_of(PRE_FIX_INLINE, quote_dir / "current.json")
    fix_plain = status_of(FIXED_INLINE, plain_dir / "current.json")
    fix_quote = status_of(FIXED_INLINE, quote_dir / "current.json")

    check("CONTROL: both fixture files are readable and identical in content",
          (plain_dir / "current.json").read_text()
          == (quote_dir / "current.json").read_text())
    check("CONTROL: the pre-fix form SUCCEEDS at a plain path (%r)"
          % pre_plain.strip(), "RESULT=closed" in pre_plain)
    check("CONTROL: the pre-fix form MISREPORTS the same readable file under a "
          "path containing ' (%r)" % pre_quote.strip(), "RESULT=?" in pre_quote)
    check("CONTROL: the fixed form succeeds at BOTH paths",
          "RESULT=closed" in fix_plain and "RESULT=closed" in fix_quote)
finally:
    shutil.rmtree(iso, ignore_errors=True)

# The ceiling of the class: the interpolated value reaches Python as SYNTAX, so a
# crafted path component does not merely break parsing -- it runs. This control is
# what entitles the PR to say so; without it that sentence would be a conclusion
# whose evidence path no longer executes, which is the very defect this file is
# about. (It was briefly deleted while addressing an "unused variable" comment;
# the minimal response was to drop the binding, not the proof.)
# Inert payload: writes a marker file. Every character is legal in a POSIX path.
ceil_dir = tempfile.mkdtemp()
try:
    marker = pathlib.Path(ceil_dir) / "ICN2722_MARKER"
    crafted = ("/x'+__import__('pathlib').Path('%s').write_text('x')*0*0+'y"
               "/current.json" % marker)
    subprocess.run(
        ["bash", "-c",
         "set -uo pipefail\nSPRINT_FILE=%s\n" % json.dumps(crafted) + PRE_FIX_INLINE],
        capture_output=True, text=True,
    )
    check("CONTROL: the pre-fix form EXECUTES code embedded in the checkout path",
          marker.exists())

    # And the fixed form must NOT execute it -- the path is data, so it is only
    # ever a filename that does not exist.
    marker2 = pathlib.Path(ceil_dir) / "ICN2722_MARKER_2"
    crafted2 = ("/x'+__import__('pathlib').Path('%s').write_text('x')*0*0+'y"
                "/current.json" % marker2)
    subprocess.run(
        ["bash", "-c",
         "set -uo pipefail\nSPRINT_FILE=%s\n" % json.dumps(crafted2) + FIXED_INLINE],
        capture_output=True, text=True,
    )
    check("CONTROL: the fixed form does NOT execute it (path stays data)",
          not marker2.exists())
finally:
    shutil.rmtree(ceil_dir, ignore_errors=True)


# ── structural: no shell value reaches Python as syntax, in any in-scope script


print()
print("--- the defect class cannot return (structural, all in-scope scripts) ---")

# The three shapes in which the shell can rewrite Python SOURCE. Anything not
# matched here cannot interpolate: a single-quoted `-c '...'` body and a
# `<<'EOF'` heredoc do not expand at all, and `script.py "$path"` passes argv --
# which is the safe form. Scanning single-quoted bodies for `${` would in fact be
# wrong: there a `${x}` is a literal string, not an injection.
# `python3?` so a `python -c` spelling is not silently skipped; `<<-?` so the
# dash-heredoc form is caught. The expansion pattern deliberately admits every
# shape the shell substitutes -- `${VAR}`, `${VAR:-default}`, `${ARR[0]}`, `$VAR`,
# `$1`, `$@`, `$(...)` -- because the earlier `${NAME}`-only spelling silently
# passed `open('${SPRINT_FILE:-/dev/null}')` and `open('$1')`.
RE_C_DQ = re.compile(r'python3? -c "((?:[^"\\]|\\.)*)"', re.S)
RE_HEREDOC = re.compile(
    r"python3? [-\w./]*\s*<<-?(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\1\n(.*?)\n\s*\2\b", re.S)
RE_EXPANSION = re.compile(r"\$\{[^}]*\}|\$\(|\$[A-Za-z_0-9@*#?-]")


def python_sources(text):
    """(shape, body) for every Python source the shell can rewrite."""
    out = [("python3 -c \"...\"", m.group(1)) for m in RE_C_DQ.finditer(text)]
    for m in RE_HEREDOC.finditer(text):
        if m.group(1) == "":          # unquoted delimiter -> shell expands body
            out.append(("python3 <<%s (unquoted)" % m.group(2), m.group(3)))
    return out


total_sources = 0
for script in IN_SCOPE:
    text = script.read_text(encoding="utf-8")
    srcs = python_sources(text)
    total_sources += len(srcs)
    offenders = []
    for shape, body in srcs:
        found = RE_EXPANSION.findall(body)
        if found:
            offenders.append("%s -> %s" % (shape, ", ".join(sorted(set(found)))))
    check("%s: no shell value reaches Python as syntax (%d source block(s))"
          % (script.name, len(srcs)),
          not offenders)
    if offenders:
        for o in offenders:
            print("         offender: " + o)

check("the structural scan actually found Python source to inspect (%d blocks)"
      % total_sources, total_sources >= 10)

for script in IN_SCOPE:
    check("%s: no `open('${...}')` spelling remains" % script.name,
          not re.search(r"open\(\s*['\"]\$\{", script.read_text(encoding="utf-8")))


# ── MUST-FAIL: the structural gate rejects BOTH generations of the defect ────


print()
print("--- MUST-FAIL: the structural gate rejects the pre-fix spellings ---")

GEN1 = (            # what-matters-now.sh, the originally reported generation
    'SPRINT_STATUS=$(python3 -c "import json; '
    "d=json.load(open('${SPRINT_FILE}')); print(d.get('status','?'))\" "
    '2>/dev/null || echo "?")'
)
GEN2 = (            # drift-check.sh / setup-skill-symlinks.sh, found by review
    'mapfile -t SKILL_TREES < <(python3 -c "\n'
    "import json,sys\n"
    "try: d=json.load(open('${REGISTRY}'))\n"
    "except Exception: sys.exit(0)\n"
    '" 2>/dev/null || true)'
)
GEN3 = (            # an expanding heredoc, the shape #2688 removed
    "python3 - <<EOF\nimport json\nd = json.load(open('${REPO_ROOT}/x.json'))\nEOF\n"
)

for label, sample in (("gen-1 inline (icn#2722 as filed)", GEN1),
                      ("gen-2 multiline (found by live review)", GEN2),
                      ("gen-3 expanding heredoc", GEN3)):
    srcs = python_sources(sample)
    flagged = any(RE_EXPANSION.findall(body) for _, body in srcs)
    check("CONTROL: the structural gate flags %s" % label, bool(srcs) and flagged)

check("CONTROL: the gate does NOT flag the safe argv form",
      not any(RE_EXPANSION.findall(b) for _, b in python_sources(
          'python3 -c "import json,sys; json.load(open(sys.argv[1]))" "${REGISTRY}"')))
check("CONTROL: the gate does NOT flag a quoted heredoc",
      not python_sources("python3 - <<'EOF'\nimport os\nprint(os.environ['X'])\nEOF\n"))


# ── summary ──────────────────────────────────────────────────────────────────


print()
if failures:
    print("FAILED (%d):" % len(failures))
    for f in failures:
        print("  - " + f)
    raise SystemExit(1)
print("all checks passed")
