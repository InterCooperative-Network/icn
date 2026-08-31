#!/usr/bin/env bash
# The adoption gate's own regression suite.
#
# WHY THIS EXISTS
#   check-agent-runtime-adoption.py is the thing that proves the runtime is actually WIRED.
#   Round 8 defeated it three different ways while it reported "25 check(s) passed, 0
#   failure(s)":
#     - prefixing every hook with `true ` (lifecycle tracking entirely off) — the matcher used
#       endswith(), so a command that merely ENDED with the hook path satisfied it;
#     - deleting .cursor/mcp.json — the adapter check `continue`d on a missing file, silently
#       dropping to 24 checks AND still printing that Cursor declares the ops MCP;
#     - moving ops/mcp/dist away — the strongest check (registry write-through) vanished.
#   A gate that cannot fail is not a gate. Each case below mutates a fixture and asserts the
#   gate FAILS; the baseline case asserts it still passes, so these are not simply "always red".

set -uo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GATE="$REPO_ROOT/scripts/check-agent-runtime-adoption.py"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { printf '  ok    %s\n' "$1"; PASS=$((PASS+1)); }
bad() { printf '  FAIL  %s\n     %s\n' "$1" "${2:-}"; FAIL=$((FAIL+1)); }

# A fixture is a copy of only what the gate reads.
make_fixture() {
  local dst="$1"
  mkdir -p "$dst"
  for p in .claude .mcp.json .cursor .codex .opencode ops/scripts ops/state scripts docs; do
    if [ -e "$REPO_ROOT/$p" ]; then
      mkdir -p "$dst/$(dirname "$p")"
      cp -a "$REPO_ROOT/$p" "$dst/$(dirname "$p")/"
    fi
  done
  mkdir -p "$dst/ops/mcp"
  [ -d "$REPO_ROOT/ops/mcp/dist" ] && cp -a "$REPO_ROOT/ops/mcp/dist" "$dst/ops/mcp/"
  # Symlinked, not copied: the compiled CLI requires better-sqlite3 at runtime, and without it
  # the registry write-through probe fails for a reason that has nothing to do with adoption.
  [ -d "$REPO_ROOT/ops/mcp/node_modules" ] && \
    ln -s "$REPO_ROOT/ops/mcp/node_modules" "$dst/ops/mcp/node_modules"
  # The fixture must be a real Git worktree. The registry write-through probe registers a
  # session, and lane identity is Git-derived — a non-repo cwd is now correctly REFUSED, so a
  # plain directory would make the strongest check fail for the wrong reason.
  git -C "$dst" init -q -b main .
  git -C "$dst" -c user.email=fixture@local -c user.name=fixture \
      commit -q --allow-empty -m "adoption gate fixture"
  return 0
}

run_gate() { timeout 120 python3 "$GATE" --repo-root "$1" >"$2" 2>&1; echo $?; }

echo "adoption-gate regression"

FIX="$TMP/base"
make_fixture "$FIX"
rc=$(run_gate "$FIX" "$TMP/base.log")
if [ "$rc" -eq 0 ]; then
  ok "an unmodified checkout passes ($(grep -o '[0-9]* check(s) passed' "$TMP/base.log" | head -1))"
else
  bad "baseline fixture does not pass — every case below would be meaningless" "$(tail -3 "$TMP/base.log")"
fi
BASE_COUNT=$(grep -oE '^agent-runtime adoption: [0-9]+' "$TMP/base.log" | grep -oE '[0-9]+' | head -1)

# ── 1. a hook that does not RUN the wrapper ────────────────────────────────
F="$TMP/neutered"; make_fixture "$F"
python3 - "$F" <<'PY'
import json, sys
p = sys.argv[1] + "/.claude/settings.json"
d = json.load(open(p))
def walk(o):
    if isinstance(o, dict):
        for k, v in list(o.items()):
            if k == "command" and isinstance(v, str) and "session-lifecycle.sh" in v:
                o[k] = "true " + v
            else:
                walk(v)
    elif isinstance(o, list):
        for i in o: walk(i)
walk(d)
json.dump(d, open(p, "w"), indent=2)
PY
rc=$(run_gate "$F" "$TMP/neutered.log")
if [ "$rc" -ne 0 ] && grep -q "does not invoke" "$TMP/neutered.log"; then
  ok "\`true <wrapper>\` is rejected: the hook must RUN the wrapper, not merely mention it"
else
  bad "a no-op hook still passed the gate" "rc=$rc $(grep -c 'does not invoke' "$TMP/neutered.log") match(es)"
fi

# ── 1b. a hook path that does not EXIST ────────────────────────────────────
# `endswith()` matching accepted any string ending in the hook's name, so pointing every
# command at `.claude/hooks/DISABLED/session-lifecycle.sh` — which exits 127 — left the gate at
# 25/0 with lifecycle tracking entirely off.
F="$TMP/disabled"; make_fixture "$F"
python3 - "$F" <<'PY'
import json, sys
p = sys.argv[1] + "/.claude/settings.json"
d = json.load(open(p))
def walk(o):
    if isinstance(o, dict):
        for k, v in list(o.items()):
            if k == "command" and isinstance(v, str) and "session-lifecycle.sh" in v:
                o[k] = v.replace("/.claude/hooks/", "/.claude/hooks/DISABLED/")
            else:
                walk(v)
    elif isinstance(o, list):
        for i in o: walk(i)
walk(d)
json.dump(d, open(p, "w"), indent=2)
PY
rc=$(run_gate "$F" "$TMP/disabled.log")
if [ "$rc" -ne 0 ] && grep -q "does not invoke" "$TMP/disabled.log"; then
  ok "a hook path that does not exist is rejected, not string-matched"
else
  bad "a nonexistent hook path passed the gate" "rc=$rc"
fi

# ── 1c. a hook whose stdin is redirected away ──────────────────────────────
# Everything after argv0 was ignored, so ` </dev/null` left the gate green while the hook got
# no payload and answered "DEGRADED — hook payload unparseable" on every event.
F="$TMP/nostdin"; make_fixture "$F"
python3 - "$F" <<'PY'
import json, sys
p = sys.argv[1] + "/.claude/settings.json"
d = json.load(open(p))
def walk(o):
    if isinstance(o, dict):
        for k, v in list(o.items()):
            if k == "command" and isinstance(v, str) and "session-lifecycle.sh" in v:
                o[k] = v + " </dev/null"
            else:
                walk(v)
    elif isinstance(o, list):
        for i in o: walk(i)
walk(d)
json.dump(d, open(p, "w"), indent=2)
PY
rc=$(run_gate "$F" "$TMP/nostdin.log")
if [ "$rc" -ne 0 ] && grep -q "does not invoke" "$TMP/nostdin.log"; then
  ok "a redirection that starves the hook of its payload is rejected"
else
  bad "a stdin-redirected hook passed the gate" "rc=$rc"
fi

# ── 1d. a $VAR the shell would NOT expand ──────────────────────────────────
# `shlex` strips quoting before the gate substitutes $CLAUDE_PROJECT_DIR, so a SINGLE-quoted
# reference — which a shell leaves literal, so the hook exits 127 and tracking is entirely off —
# resolved to the real hook path and passed 25/0.
F="$TMP/literaldollar"; make_fixture "$F"
python3 - "$F" <<'PY'
import json, sys
p = sys.argv[1] + "/.claude/settings.json"
d = json.load(open(p))
def walk(o):
    if isinstance(o, dict):
        for k, v in list(o.items()):
            if k == "command" and isinstance(v, str) and "session-lifecycle.sh" in v:
                o[k] = v.replace('"$CLAUDE_PROJECT_DIR"', "'$CLAUDE_PROJECT_DIR'")
            else:
                walk(v)
    elif isinstance(o, list):
        for i in o: walk(i)
walk(d)
json.dump(d, open(p, "w"), indent=2)
PY
rc=$(run_gate "$F" "$TMP/literaldollar.log")
if [ "$rc" -ne 0 ] && grep -q "does not invoke" "$TMP/literaldollar.log"; then
  ok "a single-quoted \$CLAUDE_PROJECT_DIR is rejected — the shell would not expand it"
else
  bad "a literal-\$ hook command passed the gate" "rc=$rc"
fi

# ── 1e. an apostrophe must not desynchronise the quoting model ─────────────
# The first version of the masker tracked ONLY single quotes, so an ordinary apostrophe inside a
# double-quoted word — `NOTE="agent's lane"` — toggled its state and left a genuinely
# single-quoted $CLAUDE_PROJECT_DIR later on the line unmasked. The gate reported 25/0 while
# `bash -c` on the identical string exited 127. Case 1d's single spelling did not catch it.
F="$TMP/quotedesync"; make_fixture "$F"
python3 - "$F" <<'PY'
import json, sys
p = sys.argv[1] + "/.claude/settings.json"
d = json.load(open(p))
def walk(o):
    if isinstance(o, dict):
        for k, v in list(o.items()):
            if k == "command" and isinstance(v, str) and "session-lifecycle.sh" in v:
                o[k] = "NOTE=\"agent's lane\" " + v.replace('"$CLAUDE_PROJECT_DIR"', "'$CLAUDE_PROJECT_DIR'")
            else:
                walk(v)
    elif isinstance(o, list):
        for i in o: walk(i)
walk(d)
json.dump(d, open(p, "w"), indent=2)
PY
rc=$(run_gate "$F" "$TMP/quotedesync.log")
if [ "$rc" -ne 0 ] && grep -q "does not invoke" "$TMP/quotedesync.log"; then
  ok "an apostrophe in a \"...\" word cannot desynchronise the quoting model"
else
  bad "a desynchronised quote tracker let a literal-\$ hook through" "rc=$rc"
fi

# ...and the CONTROL: the same apostrophe with a properly DOUBLE-quoted $VAR must still pass,
# or the fix would simply be rejecting anything containing an apostrophe.
F="$TMP/quoteok"; make_fixture "$F"
python3 - "$F" <<'PY'
import json, sys
p = sys.argv[1] + "/.claude/settings.json"
d = json.load(open(p))
def walk(o):
    if isinstance(o, dict):
        for k, v in list(o.items()):
            if k == "command" and isinstance(v, str) and "session-lifecycle.sh" in v:
                o[k] = "NOTE=\"agent's lane\" " + v
            else:
                walk(v)
    elif isinstance(o, list):
        for i in o: walk(i)
walk(d)
json.dump(d, open(p, "w"), indent=2)
PY
rc=$(run_gate "$F" "$TMP/quoteok.log")
if [ "$rc" -eq 0 ]; then
  ok "  ...and a legitimately double-quoted command with an apostrophe still passes"
else
  bad "the quoting model now falsely rejects a runnable command" "rc=$rc: $(grep 'does not invoke' "$TMP/quoteok.log" | head -1)"
fi

# ── 1f. the ESCAPE half of the quoting model ───────────────────────────────
# Cases 1d/1e pin the SINGLE-QUOTE half. Nothing pinned the escape branch, and three mutants of
# it — swapping it below the in_double branch, adding `and not in_double`, and dropping the
# LITERAL_DOLLAR mask — all left the suite at 15/0 while making the gate ACCEPT a command
# `bash -c` exits 127 on. Fail-OPEN, in the half of the model that had no assertion.
#
# The spelling matters. `"\$VAR"` — escaped INSIDE double quotes — is rejected by every one of
# those mutants too, but for the wrong reason: the stray backslash makes the path unresolvable,
# so it would pass while proving nothing. The UNQUOTED `\$VAR` is the one that discriminates
# (it kills the dropped-mask mutant); 1g below kills the other two.
F="$TMP/escapedollar"; make_fixture "$F"
python3 - "$F" <<'PY'
import json, sys
p = sys.argv[1] + "/.claude/settings.json"
d = json.load(open(p))
def walk(o):
    if isinstance(o, dict):
        for k, v in list(o.items()):
            if k == "command" and isinstance(v, str) and "session-lifecycle.sh" in v:
                # \$VAR unquoted — the backslash makes the $ LITERAL, so the shell looks for a
                # directory actually named $CLAUDE_PROJECT_DIR and fails with 127.
                o[k] = v.replace('"$CLAUDE_PROJECT_DIR"', '\\$CLAUDE_PROJECT_DIR')
            else:
                walk(v)
    elif isinstance(o, list):
        for i in o: walk(i)
walk(d)
json.dump(d, open(p, "w"), indent=2)
PY
rc=$(run_gate "$F" "$TMP/escapedollar.log")
if [ "$rc" -ne 0 ] && grep -q "does not invoke" "$TMP/escapedollar.log"; then
  ok "an unquoted escaped \$ is literal, and is rejected"
else
  bad "an escaped-\$ hook command passed the gate" "rc=$rc"
fi

# ...and the CONTROL: a backslash-escaped DOUBLE QUOTE inside a double-quoted word is ordinary,
# and must not make the gate reject a command the shell runs perfectly well.
F="$TMP/escapeok"; make_fixture "$F"
python3 - "$F" <<'PY'
import json, sys
p = sys.argv[1] + "/.claude/settings.json"
d = json.load(open(p))
def walk(o):
    if isinstance(o, dict):
        for k, v in list(o.items()):
            if k == "command" and isinstance(v, str) and "session-lifecycle.sh" in v:
                o[k] = 'NOTE="a\\"b" ' + v
            else:
                walk(v)
    elif isinstance(o, list):
        for i in o: walk(i)
walk(d)
json.dump(d, open(p, "w"), indent=2)
PY
rc=$(run_gate "$F" "$TMP/escapeok.log")
if [ "$rc" -eq 0 ]; then
  ok "  ...and an escaped quote in a \"...\" word still passes"
else
  bad "the quoting model now falsely rejects a runnable command" "rc=$rc"
fi

# ── 1g. an escaped quote must not desynchronise the model either ───────────
# Same shape as 1e, but the desync vector is a BACKSLASH-ESCAPED double quote rather than an
# apostrophe. It reaches the escape/in_double interaction that 1e does not.
F="$TMP/escapedesync"; make_fixture "$F"
python3 - "$F" <<'PY'
import json, sys
p = sys.argv[1] + "/.claude/settings.json"
d = json.load(open(p))
def walk(o):
    if isinstance(o, dict):
        for k, v in list(o.items()):
            if k == "command" and isinstance(v, str) and "session-lifecycle.sh" in v:
                o[k] = 'NOTE="a\\"b" ' + v.replace('"$CLAUDE_PROJECT_DIR"', "'$CLAUDE_PROJECT_DIR'")
            else:
                walk(v)
    elif isinstance(o, list):
        for i in o: walk(i)
walk(d)
json.dump(d, open(p, "w"), indent=2)
PY
rc=$(run_gate "$F" "$TMP/escapedesync.log")
if [ "$rc" -ne 0 ] && grep -q "does not invoke" "$TMP/escapedesync.log"; then
  ok "an escaped quote cannot desynchronise the model either"
else
  bad "an escaped-quote desync let a literal-\$ hook through" "rc=$rc"
fi

# ── 2. a provider adapter the gate CLAIMS coverage for ─────────────────────
F="$TMP/nocursor"; make_fixture "$F"; rm -f "$F/.cursor/mcp.json"
rc=$(run_gate "$F" "$TMP/nocursor.log")
if [ "$rc" -ne 0 ] && grep -q "missing: .cursor/mcp.json" "$TMP/nocursor.log"; then
  ok "a deleted .cursor/mcp.json FAILS instead of silently dropping a check"
else
  bad "a missing adapter was skipped silently" "rc=$rc"
fi

# ── 3. the strongest check must not be able to vanish ──────────────────────
F="$TMP/nodist"; make_fixture "$F"; rm -rf "$F/ops/mcp/dist"
rc=$(run_gate "$F" "$TMP/nodist.log")
if [ "$rc" -ne 0 ] && grep -qE "[0-9]+ checks ran, expected exactly [0-9]+" "$TMP/nodist.log"; then
  ok "an unbuilt ops/mcp/dist trips the check-count floor rather than reporting green"
else
  bad "a reduced check count still reported success" "rc=$rc"
fi

# ── 4. the count itself is asserted, not decorative ────────────────────────
# PARSED, not grepped: `grep -q "EXPECTED_CHECKS = 25"` is satisfied by a COMMENT, so setting
# the constant to 24 and adding `# EXPECTED_CHECKS = 25` left this case green.
# The expectation is now fixed + derived (icn#2691): the hook-executable checks come from
# .claude/settings.json, so their count legitimately changes when a hook is added. The fixed
# part stays exact; this asserts the SUM, which is the property that actually matters.
DECLARED=$(python3 -c '
import importlib.util, json, pathlib, re, sys
src = open(sys.argv[1]).read()
m = re.search(r"^EXPECTED_STATIC_CHECKS\s*=\s*(\d+)\s*$", src, re.M)
if not m:
    print(""); sys.exit()
spec = importlib.util.spec_from_file_location("gate", sys.argv[1])
mod = importlib.util.module_from_spec(spec); spec.loader.exec_module(mod)
root = pathlib.Path(sys.argv[2])
settings = json.loads((root / ".claude/settings.json").read_text(encoding="utf-8"))
print(int(m.group(1)) + len(mod.direct_hook_targets(settings, root)[0]))
' "$GATE" "$FIX")
if [ -n "$BASE_COUNT" ] && [ "$DECLARED" = "$BASE_COUNT" ]; then
  ok "EXPECTED_STATIC_CHECKS + derived hook checks ($BASE_COUNT) matches a complete run"
else
  bad "the expected check count does not match the observed count" "declared=${DECLARED:-<unparseable>} observed=$BASE_COUNT"
fi

# ── 5. the prose the gate prints must match what it verified ───────────────
# It printed ".cursor adapter: declares the ops MCP in .cursor/mcp.json" on a run where that
# file had been deleted. Coverage prose is a claim like any other.
# The COVERAGE prose itself, not the failure line. `grep -q "cursor"` matched
# `missing: .cursor/mcp.json` — the failure — making this a tautological duplicate of case 2,
# and deleting the ".cursor adapter" entry from COVERAGE entirely left it green.
# COVERAGE prose only prints with --verbose (or on failure), so the clean-run half of this
# assertion has to ask for it explicitly. Asserting it against a non-verbose log would compare
# against an empty section and pass for the wrong reason.
timeout 120 python3 "$GATE" --repo-root "$FIX" --verbose >"$TMP/base-verbose.log" 2>&1
if grep -q "declares the ops MCP in .cursor/mcp.json" "$TMP/base-verbose.log" && grep -q "missing: .cursor/mcp.json" "$TMP/nocursor.log"; then
  ok "the .cursor coverage claim is printed on a clean run AND fails when it stops being true"
else
  bad "coverage prose asserted something the run did not verify" \
      "clean-run prose present: $(grep -c 'declares the ops MCP in .cursor/mcp.json' "$TMP/base-verbose.log"); failure present: $(grep -c 'missing: .cursor/mcp.json' "$TMP/nocursor.log")"
fi

# ── 6. the WRAPPER's degraded envelope ─────────────────────────────────────
#
# ops/scripts/icn-agent-session emits a hand-written JSON envelope when it cannot reach the
# CLI. Nothing gated it — not vitest, not the adoption gate, not the capability suite — so
# deleting `contention` and `live_agent_pids` from it passed every suite in the repository.
# It also interpolated a filesystem path straight into JSON, so a root containing a quote, a
# backslash or a tab produced exit 3 plus unparseable output: the fail-safe status with garbage
# attached, which is the precise failure the block exists to prevent.
echo
echo "wrapper degraded envelope"

wrapper_case() {
  local label="$1" dirname="$2"
  # The exact directory name must survive escaping and decoding intact.
  local marker="$dirname"
  local d="$TMP/wrap/$dirname"
  mkdir -p "$d/ops/scripts" "$d/ops/mcp"          # ops/mcp exists, dist/ deliberately does not
  cp -a "$REPO_ROOT/ops/scripts/icn-agent-session" "$d/ops/scripts/"
  local out
  out=$(CLAUDE_PROJECT_DIR="$d" timeout 20 "$d/ops/scripts/icn-agent-session" \
          classify --worktree-id /nonexistent 2>/dev/null)
  local rc=$?
  local verdict
  verdict=$(printf '%s' "$out" | python3 -c '
import sys, json
REQUIRED = ["state","reason","session_id","heartbeat_age_min","progress_age_min",
            "progress_count","contention","branch_changed","live_branch","live_agent_pids"]
raw = sys.stdin.read()
try:
    d = json.loads(raw)
except Exception as e:
    print("UNPARSEABLE: %s" % str(e)[:60]); raise SystemExit(0)
missing = [k for k in REQUIRED if k not in d]
marker = sys.argv[1] if len(sys.argv) > 1 else ""
if missing: print("MISSING: %s" % ",".join(missing))
elif not isinstance(d.get("live_agent_pids"), list): print("live_agent_pids is not a list")
elif not isinstance(d.get("contention"), dict): print("contention is not an object")
# THE REASON MUST ROUND-TRIP THE PATH, not merely parse. Checking parseability alone let a
# BROKEN backslash escape through: `has\backslash` becomes `\b`, which is a LEGAL JSON escape,
# so json.loads succeeded while silently decoding it to U+0008 — "hasackslash". The envelope
# parsed, the keys were all present, and the reason was corrupt.
elif marker and marker not in d.get("reason", ""):
    print("REASON CORRUPTED: %r not found in %r" % (marker, d.get("reason", "")[:120]))
else: print("OK")
' "$marker")
  if [ "$verdict" = "OK" ] && [ "$rc" -eq 3 ]; then
    ok "$label"
  else
    bad "$label" "rc=$rc verdict=$verdict"
  fi
}

wrapper_case "the degraded envelope is complete and parseable (exit 3)" "plain"
wrapper_case "...and survives a double quote in the repo path" 'has"quote'
wrapper_case "...and a backslash" 'has\backslash'
wrapper_case "...and a tab" "$(printf 'has\ttab')"

# ── 6. a hook invoked directly must be executable (icn#2691) ───────────────
# hook-health.sh was committed 100644 while settings.json ran it AS the command, so it exited
# 126 at every session start. The gate did not look: its executable list was hardcoded and
# omitted it. The list is derived from settings.json now; these prove the derivation both
# catches a real regression and does not over-reach.

FIX_X="$TMP/hookexec"
make_fixture "$FIX_X"
chmod -x "$FIX_X/.claude/hooks/hook-health.sh"
rc=$(run_gate "$FIX_X" "$TMP/hookexec.log")
if [ "$rc" -ne 0 ] && grep -q "not executable: .claude/hooks/hook-health.sh" "$TMP/hookexec.log"; then
  ok "a directly-invoked hook without the executable bit fails the gate"
else
  bad "a non-executable direct hook did not fail the gate" "$(tail -3 "$TMP/hookexec.log")"
fi

# The .py hooks run through python3 and are correctly 100644. Rejecting them would be a false
# positive that pressures someone into chmod-ing files that do not need it.
FIX_PY="$TMP/pyhooks"
make_fixture "$FIX_PY"
chmod -x "$FIX_PY"/.claude/hooks/*.py 2>/dev/null || true
rc=$(run_gate "$FIX_PY" "$TMP/pyhooks.log")
if [ "$rc" -eq 0 ]; then
  ok "hooks invoked through an interpreter are not required to be executable"
else
  bad "non-executable .py hooks were wrongly rejected" "$(tail -3 "$TMP/pyhooks.log")"
fi

# A configured hook whose FILE is gone must be reported missing, not quietly dropped from the
# derived list. Filtering on existence shrank the list and the expected count together, so the
# gate stayed green while settings.json invoked a command that was not there.
FIX_DEL="$TMP/delhook"
make_fixture "$FIX_DEL"
rm -f "$FIX_DEL/.claude/hooks/firewall-guard.sh"
rc=$(run_gate "$FIX_DEL" "$TMP/delhook.log")
if [ "$rc" -ne 0 ] && grep -q "missing: .claude/hooks/firewall-guard.sh" "$TMP/delhook.log"; then
  ok "deleting a hook settings.json still invokes is reported missing"
else
  bad "a deleted but still-configured hook was silently dropped" "$(tail -3 "$TMP/delhook.log")"
fi

# One unparseable command must not stop the walk and take the rest of the hooks with it.
FIX_BAD="$TMP/badcmd"
make_fixture "$FIX_BAD"
python3 - "$FIX_BAD" <<'PYBAD'
import json, pathlib, sys
p = pathlib.Path(sys.argv[1]) / ".claude/settings.json"
d = json.loads(p.read_text(encoding="utf-8"))
first = next(iter(d["hooks"]))
d["hooks"][first].insert(0, {"matcher": "*", "hooks": [
    {"type": "command", "command": 'unclosed "quote'}]})
p.write_text(json.dumps(d, indent=2), encoding="utf-8")
PYBAD
# Compare the target SET before and after, never a pinned count. Hardcoding the
# repository's current hook count made this assertion fail the moment a legitimate hook was
# added -- turning the required drift workflow red, and directly contradicting the next case,
# which asserts that additions must NOT trip the count.
python3 -c "
import importlib.util, json, pathlib, sys
spec = importlib.util.spec_from_file_location('g', sys.argv[1])
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
clean, dirty = pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3])
load = lambda r: json.loads((r / '.claude/settings.json').read_text(encoding='utf-8'))
before = set(m.direct_hook_targets(load(clean), clean)[0])
after = set(m.direct_hook_targets(load(dirty), dirty)[0])
sys.exit(0 if before and before == after else 1)
" "$GATE" "$FIX" "$FIX_BAD"
if [ $? -eq 0 ]; then
  ok "a malformed hook command does not abort the derivation walk"
else
  bad "a malformed hook command truncated the derived target list" ""
fi

# Two legal direct-invocation spellings that the derivation used to mishandle. `_invokes_hook`
# in the same file already normalised both, so these were siblings disagreeing.
python3 - "$GATE" <<'PYSHAPES'
import importlib.util, pathlib, sys
spec = importlib.util.spec_from_file_location("g", sys.argv[1])
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
root = pathlib.Path(".").resolve()
K = m.HookCommandKind
want = ".claude/hooks/hook-health.sh"
H = '"$CLAUDE_PROJECT_DIR"/.claude/hooks/hook-health.sh'
# Assignment prefixes and both project-dir spellings stay supported: `_invokes_hook` in the
# same module already treats them as direct invocations.
cases = {
    'MODE=health %s' % H: (K.DIRECT, want),
    'A=1 B=2 "${CLAUDE_PROJECT_DIR}"/.claude/hooks/hook-health.sh': (K.DIRECT, want),
    '"${CLAUDE_PROJECT_DIR}"/.claude/hooks/hook-health.sh': (K.DIRECT, want),
    H: (K.DIRECT, want),
    'echo hi': (K.NON_HOOK, None),
    'python3 "$CLAUDE_PROJECT_DIR"/.claude/hooks/pre-tool-guard.py': (K.INTERPRETED, None),
}
bad = [c for c, exp in cases.items() if m.classify_hook_command(c, root)[:2] != exp]
sys.exit(0 if not bad else 1)
PYSHAPES
if [ $? -eq 0 ]; then
  ok "assignment prefixes and both project-dir spellings classify as direct"
else
  bad "a legal direct-invocation spelling was mis-resolved" ""
fi

# End-to-end: rewriting a hook with an env-assignment prefix must not make its executable
# check disappear. The derived list and the expected count read the same source, so a dropped
# target hides itself in both.
FIX_ASSIGN="$TMP/assignhook"
make_fixture "$FIX_ASSIGN"
python3 - "$FIX_ASSIGN" <<'PYASSIGN'
import json, pathlib, sys
p = pathlib.Path(sys.argv[1]) / ".claude/settings.json"
d = json.loads(p.read_text(encoding="utf-8"))
def walk(n):
    if isinstance(n, dict):
        c = n.get("command")
        if n.get("type") == "command" and isinstance(c, str) and "hook-health.sh" in c:
            n["command"] = "MODE=health " + c
        for v in n.values(): walk(v)
    elif isinstance(n, list):
        for v in n: walk(v)
walk(d.get("hooks", {}))
p.write_text(json.dumps(d, indent=2), encoding="utf-8")
PYASSIGN
chmod -x "$FIX_ASSIGN/.claude/hooks/hook-health.sh"
rc=$(run_gate "$FIX_ASSIGN" "$TMP/assignhook.log")
if [ "$rc" -ne 0 ] && grep -q "not executable: .claude/hooks/hook-health.sh" "$TMP/assignhook.log"; then
  ok "an env-assignment prefix does not hide a hook from the executable check"
else
  bad "an assignment-prefixed hook escaped the executable check" "$(tail -3 "$TMP/assignhook.log")"
fi

# Launcher prefixes and absolute interpreters: two opposite failures. A launcher-prefixed
# hook dropped out of the derived set (fail-open); an absolute external interpreter became a
# bogus repo-relative path and was reported missing (fail-closed on a correct config).
python3 - "$GATE" <<'PYLAUNCH'
import importlib.util, pathlib, sys
spec = importlib.util.spec_from_file_location("g", sys.argv[1])
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
root = pathlib.Path(".").resolve()
K = m.HookCommandKind
H = '"$CLAUDE_PROJECT_DIR"/.claude/hooks/hook-health.sh'
PY = '"$CLAUDE_PROJECT_DIR"/.claude/hooks/pre-tool-guard.py'
cases = {
    # The three shapes live settings.json actually uses.
    H: K.DIRECT,
    'python3 %s' % PY: K.INTERPRETED,
    # An ABSOLUTE interpreter is no longer exempt. Nothing uses the spelling, and defending it
    # meant trusting a basename: `/tmp/python3` symlinked to `/usr/bin/env` was certified as
    # "runs the interpreter" while the hook it launches left the derived set.
    '/usr/bin/python3 %s' % PY: K.UNCLASSIFIED,
    '/tmp/python3 %s' % H: K.UNCLASSIFIED,
    # The real echo carries `||` INSIDE a command substitution. Quoting is respected, so this
    # is one simple command -- a substring search for operators would have got it wrong.
    'echo "branch: $(git branch --show-current 2>/dev/null || echo detached)"': K.NON_HOOK,
    # Top-level composition: argv0 is not the only program, and a later one may be a hook.
    'true && %s' % H: K.UNCLASSIFIED,
    'echo hello; %s' % H: K.UNCLASSIFIED,
    'echo foo | %s' % H: K.UNCLASSIFIED,
    # Launcher support was REMOVED: nothing used it, and `command -v` only prints while
    # `env -0` refuses to run a command, so one shared flag set certified both as execution.
    'command -v %s' % H: K.UNCLASSIFIED,
    'env -0 %s' % H: K.UNCLASSIFIED,
    'nohup -i %s' % H: K.UNCLASSIFIED,
    'env %s' % H: K.UNCLASSIFIED,
    'env -i %s' % H: K.UNCLASSIFIED,
    # An external absolute program is NOT harmless: `/usr/bin/env <hook>` runs the hook and
    # returns 126 when it is not executable; `/bin/sh -c …` can run anything. No name exempts
    # a path that lands outside the tree, and a repo file named python3 does not escape either:
    # the interpreter vocabulary applies to BARE names only, which is what a shell looks up.
    '/usr/bin/env %s' % H: K.UNCLASSIFIED,
    '/usr/bin/nohup %s' % H: K.UNCLASSIFIED,
    '/bin/sh -c "anything"': K.UNCLASSIFIED,
    '/opt/vendor/wrapper %s' % H: K.UNCLASSIFIED,
    # `true` and `:` are gone from the non-hook vocabulary; nothing live uses them.
    'true': K.UNCLASSIFIED,
    'curl https://example.invalid': K.UNCLASSIFIED,
}
bad = [c for c, exp in cases.items() if m.classify_hook_command(c, root)[0] != exp]
sys.exit(0 if not bad else 1)
PYLAUNCH
if [ $? -eq 0 ]; then
  ok "the supported command language is the three live shapes; everything else is UNCLASSIFIED"
else
  bad "a hook command form was mis-classified" ""
fi


# CONTAINMENT, decided physically and BEFORE any name-based exemption. Two opposite fail-open
# holes, both of which left the target out of the derived set -- and the expected check count
# reads that same list, so each loss concealed itself.
python3 - "$GATE" <<'PYCONTAIN'
import importlib.util, os, pathlib, sys
spec = importlib.util.spec_from_file_location("g", sys.argv[1])
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
root = pathlib.Path(".").resolve()
K = m.HookCommandKind
H = '"$CLAUDE_PROJECT_DIR"/.claude/hooks/hook-health.sh'
# Enough `..` to leave THIS root from inside .claude/, so the escape lands on a real,
# executable /usr/bin/env rather than on nothing -- otherwise the old code would have been
# caught by `missing:` and the hole would look self-limiting.
up = "../" * len(root.parts[1:])
esc = '"$CLAUDE_PROJECT_DIR"/.claude/../%susr/bin/env %s' % (up, H)
assert (root / (".claude/../%susr/bin/env" % up)).resolve() == pathlib.Path("/usr/bin/env")
assert os.access("/usr/bin/env", os.X_OK), "fixture assumption: /usr/bin/env is executable"
cases = {
    # Interior traversal has no LEADING `..`, so a lexical containment test saw a repository
    # path and the executable check then certified a program outside the tree.
    esc: (K.UNCLASSIFIED, None),
    '"$CLAUDE_PROJECT_DIR"/.claude/../../../usr/bin/env %s' % H: (K.UNCLASSIFIED, None),
    # A repository file that IS argv0 needs the executable bit whatever it is NAMED. The
    # basename exemptions used to fire first and drop it.
    '"$CLAUDE_PROJECT_DIR"/.claude/hooks/python3': (K.DIRECT, ".claude/hooks/python3"),
    '"$CLAUDE_PROJECT_DIR"/.claude/hooks/python': (K.DIRECT, ".claude/hooks/python"),
    '"$CLAUDE_PROJECT_DIR"/.claude/hooks/echo': (K.DIRECT, ".claude/hooks/echo"),
    # Traversal that stays inside is legal, and the target is reported RESOLVED: the caller
    # joins it to the root, so handing back the raw spelling hands back the traversal too.
    '"$CLAUDE_PROJECT_DIR"/.claude/hooks/../hooks/hook-health.sh':
        (K.DIRECT, ".claude/hooks/hook-health.sh"),
    # A BARE name keeps shell semantics -- PATH lookup, never the repository -- so the
    # interpreter and non-hook vocabularies apply to it, and ONLY to it.
    'python3 "$CLAUDE_PROJECT_DIR"/.claude/hooks/pre-tool-guard.py': (K.INTERPRETED, None),
    '/usr/bin/python3 "$CLAUDE_PROJECT_DIR"/.claude/hooks/pre-tool-guard.py': (K.UNCLASSIFIED, None),
    'echo hi': (K.NON_HOOK, None),
    H: (K.DIRECT, ".claude/hooks/hook-health.sh"),
}
bad = [c for c, exp in cases.items() if m.classify_hook_command(c, root)[:2] != exp]
sys.exit(0 if not bad else 1)
PYCONTAIN
if [ $? -eq 0 ]; then
  ok "containment is physical and precedes the name exemptions"
else
  bad "a path command escaped the repository or was exempted by its name" ""
fi

# One quoting model, three consumers. Masking tracked quotes properly while comment stripping
# was an unconditional split, and the lexer never saw a newline at all.
python3 - "$GATE" <<'PYQUOTE'
import importlib.util, pathlib, sys
spec = importlib.util.spec_from_file_location("g", sys.argv[1])
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
root = pathlib.Path(".").resolve()
K = m.HookCommandKind
H = '"$CLAUDE_PROJECT_DIR"/.claude/hooks/hook-health.sh'
PY = '"$CLAUDE_PROJECT_DIR"/.claude/hooks/pre-tool-guard.py'
cases = {
    # A LITERAL NEWLINE is a bash command separator. shlex reads it as ordinary whitespace, so
    # argv0 was `echo`, the entry classified NON_HOOK, and the hook on the second line left the
    # derived set -- while bash runs it and returns 126 when it is not executable.
    'echo hi\n%s' % H: K.UNCLASSIFIED,
    '%s\necho done' % H: K.UNCLASSIFIED,
    # A comment ends at its LINE, not at the end of the string, so this still hides a command.
    'echo hi # note\n%s' % H: K.UNCLASSIFIED,
    # ...but a newline INSIDE quotes is data, not a separator.
    'echo "a\nb"': K.NON_HOOK,
    # A quoted `#` is part of the argument. The unconditional split cut here, leaving an
    # unmatched quote that came back as "unparseable" -- the required workflow RED on a command
    # the shell runs perfectly well. That is a false rejection, not a missed detection.
    'echo "ticket #123"': K.NON_HOOK,
    "echo 'ticket #123'": K.NON_HOOK,
    'echo ticket\\#123': K.NON_HOOK,
    # A real comment is still a comment.
    'echo hi # a note': K.NON_HOOK,
    '%s # runs the health check' % H: K.DIRECT,
    # ...and shlex must not strip comments a SECOND time, by a different rule. Its commenter
    # fires mid-word, but bash treats `#` inside a word as part of it, so `<hook>#missing` was
    # truncated to the real hook and its bit checked while Claude runs the suffixed path and
    # exits 127. The target must keep the suffix so the `missing:` branch fails on it.
    '%s#missing' % H: K.DIRECT,
    '"$CLAUDE_PROJECT_DIR"/.claude/hooks/a#b.sh': K.DIRECT,
    # A bare name is resolved through PATH, and an assignment can BE the PATH:
    # `PATH=/tmp:$PATH python3 <hook>` runs whatever /tmp/python3 is. A name exemption needs
    # nothing local to have changed what the name resolves to.
    'PATH=/tmp:$PATH python3 %s' % PY: K.UNCLASSIFIED,
    'FOO=1 echo hi': K.UNCLASSIFIED,
    # ...but an assignment in front of a repo PATH stays supported: a path is not looked up.
    'MODE=health %s' % H: K.DIRECT,
    # A COMMAND SUBSTITUTION is executable shell wherever it appears, including inside the
    # quoted argument of an exempt command. `echo "$(<hook>)"` RUNS the hook, and the outer
    # echo returns 0 whatever the hook does -- so a mode-0644 file reported permission denied
    # while the gate stayed green and the target never entered the derived set.
    'echo "$(%s)"' % H: K.UNCLASSIFIED,
    'echo "`%s`"' % H: K.UNCLASSIFIED,
    'echo $(%s)' % H: K.UNCLASSIFIED,
    # ...but only one NAMING A REPOSITORY PATH. The live command carries a substitution and
    # must stay classifiable, which is the whole reason this is not a blanket refusal.
    'echo "branch: $(git branch --show-current 2>/dev/null || echo detached)"': K.NON_HOOK,
    # OPERATORS ARE READ BEFORE QUOTING IS DISCARDED. shlex strips quote provenance, so an
    # argument made entirely of punctuation came back indistinguishable from a real operator
    # and the gate went RED on a command bash runs normally.
    'echo ";"': K.NON_HOOK,
    "echo '&&'": K.NON_HOOK,
    'echo "|"': K.NON_HOOK,
    'echo \\;': K.NON_HOOK,
    '%s ";"' % H: K.DIRECT,
    # A SUBSTITUTION BODY HAS ITS OWN QUOTING CONTEXT. Counting parentheses against the OUTER
    # states closed the body at a quoted `)`, so the scan found no repository path and the
    # entry fell back to NON_HOOK while bash still runs the hook.
    'echo "$(echo \')\'; %s)"' % H: K.UNCLASSIFIED,
    'echo "$(echo \'(\'; %s)"' % H: K.UNCLASSIFIED,
    # The BACKTICK scanner had the same defect one round later: it closed on the first
    # backtick `find` reached, so an escaped or quoted one truncated the body and the hook
    # inside it was never seen. Its delimiter is located with shell-aware state now.
    'echo "`printf \'\\`\'; %s`"' % H: K.UNCLASSIFIED,
    'echo "`printf \'x`y\'; %s`"' % H: K.UNCLASSIFIED,
    'echo "`%s`"' % H: K.UNCLASSIFIED,
    'echo "`date`"': K.NON_HOOK,
    # OPERATOR TEXT INSIDE A COMMENT IS NOT COMPOSITION. Scanning the raw command made the
    # gate red on a hook carrying an explanatory comment -- bash never sees those characters.
    '%s # use && fallback' % H: K.DIRECT,
    '%s # x | y' % H: K.DIRECT,
    '%s # step 1; step 2' % H: K.DIRECT,
}
bad = [c for c, exp in cases.items() if m.classify_hook_command(c, root)[0] != exp]
sys.exit(0 if not bad else 1)
PYQUOTE
if [ $? -eq 0 ]; then
  ok "newlines separate commands, and quoted hashes are not comments"
else
  bad "a separator was missed or a quoted hash truncated a valid command" ""
fi

# End-to-end: the newline spelling must take the gate red, not quietly drop a hook.
FIX_NL="$TMP/newlinehook"
make_fixture "$FIX_NL"
python3 - "$FIX_NL" <<'PYNL'
import json, pathlib, sys
p = pathlib.Path(sys.argv[1]) / ".claude/settings.json"
d = json.loads(p.read_text(encoding="utf-8"))
def walk(n):
    if isinstance(n, dict):
        c = n.get("command")
        if n.get("type") == "command" and isinstance(c, str) and "hook-health.sh" in c:
            n["command"] = "echo starting\n" + c
        for v in n.values(): walk(v)
    elif isinstance(n, list):
        for v in n: walk(v)
walk(d.get("hooks", {}))
p.write_text(json.dumps(d, indent=2), encoding="utf-8")
PYNL
chmod -x "$FIX_NL/.claude/hooks/hook-health.sh"
rc=$(run_gate "$FIX_NL" "$TMP/newlinehook.log")
if [ "$rc" -ne 0 ] && grep -q "literal newline" "$TMP/newlinehook.log"; then
  ok "a newline-separated hook command fails the gate instead of vanishing behind echo"
else
  bad "a hook hidden after a newline escaped the executable check" "$(tail -3 "$TMP/newlinehook.log")"
fi

# End-to-end the other way: a quoted hash must NOT make a working configuration fail.
FIX_HASH="$TMP/hashhook"
make_fixture "$FIX_HASH"
python3 - "$FIX_HASH" <<'PYHASH'
import json, pathlib, sys
p = pathlib.Path(sys.argv[1]) / ".claude/settings.json"
d = json.loads(p.read_text(encoding="utf-8"))
done = []
def walk(n):
    if isinstance(n, dict):
        c = n.get("command")
        if n.get("type") == "command" and isinstance(c, str) and c.startswith("echo") and not done:
            n["command"] = 'echo "see ticket #123"'
            done.append(1)
        for v in n.values(): walk(v)
    elif isinstance(n, list):
        for v in n: walk(v)
walk(d.get("hooks", {}))
assert done, "fixture assumption: settings.json has an echo hook"
p.write_text(json.dumps(d, indent=2), encoding="utf-8")
PYHASH
rc=$(run_gate "$FIX_HASH" "$TMP/hashhook.log")
if [ "$rc" -eq 0 ]; then
  ok "a quoted hash in a hook command does not fail the gate"
else
  bad "a valid command containing a quoted hash was rejected" "$(tail -3 "$TMP/hashhook.log")"
fi

# End-to-end: a `#` suffix on the path must not be lexed away, or the gate checks a different
# file from the one Claude runs.
FIX_SUF="$TMP/suffixhook"
make_fixture "$FIX_SUF"
python3 - "$FIX_SUF" <<'PYSUF'
import json, pathlib, sys
p = pathlib.Path(sys.argv[1]) / ".claude/settings.json"
d = json.loads(p.read_text(encoding="utf-8"))
def walk(n):
    if isinstance(n, dict):
        c = n.get("command")
        if n.get("type") == "command" and isinstance(c, str) and c.endswith("hook-health.sh"):
            n["command"] = c + "#missing"
        for v in n.values(): walk(v)
    elif isinstance(n, list):
        for v in n: walk(v)
walk(d.get("hooks", {}))
p.write_text(json.dumps(d, indent=2), encoding="utf-8")
PYSUF
rc=$(run_gate "$FIX_SUF" "$TMP/suffixhook.log")
if [ "$rc" -ne 0 ] && grep -q "missing: .claude/hooks/hook-health.sh#missing" "$TMP/suffixhook.log"; then
  ok "a hash suffix on a hook path is checked as part of the path, not lexed away"
else
  bad "a hash-suffixed hook path was checked as the unsuffixed file" "$(tail -3 "$TMP/suffixhook.log")"
fi

# End-to-end: an assignment that can change PATH lookup must take the gate red, not exempt the
# interpreter and drop the hook.
FIX_PATH="$TMP/pathhook"
make_fixture "$FIX_PATH"
python3 - "$FIX_PATH" <<'PYPATH'
import json, pathlib, sys
p = pathlib.Path(sys.argv[1]) / ".claude/settings.json"
d = json.loads(p.read_text(encoding="utf-8"))
done = []
def walk(n):
    if isinstance(n, dict):
        c = n.get("command")
        if n.get("type") == "command" and isinstance(c, str) and c.startswith("python3 ") and not done:
            n["command"] = "PATH=/tmp:$PATH " + c
            done.append(1)
        for v in n.values(): walk(v)
    elif isinstance(n, list):
        for v in n: walk(v)
walk(d.get("hooks", {}))
assert done, "fixture assumption: settings.json invokes at least one python3 hook"
p.write_text(json.dumps(d, indent=2), encoding="utf-8")
PYPATH
rc=$(run_gate "$FIX_PATH" "$TMP/pathhook.log")
if [ "$rc" -ne 0 ] && grep -q "command-local assignment" "$TMP/pathhook.log"; then
  ok "an interpreter behind a PATH-changing assignment is not exempted"
else
  bad "a PATH-overridden interpreter kept its name exemption" "$(tail -3 "$TMP/pathhook.log")"
fi

# End-to-end, the fail-open half: a hook run from inside a substitution must not hide behind
# the outer echo, whose exit status says nothing about it.
FIX_SUB="$TMP/subhook"
make_fixture "$FIX_SUB"
python3 - "$FIX_SUB" <<'PYSUB'
import json, pathlib, sys
p = pathlib.Path(sys.argv[1]) / ".claude/settings.json"
d = json.loads(p.read_text(encoding="utf-8"))
def walk(n):
    if isinstance(n, dict):
        c = n.get("command")
        if n.get("type") == "command" and isinstance(c, str) and c.endswith("hook-health.sh"):
            n["command"] = 'echo "$(%s)"' % c
        for v in n.values(): walk(v)
    elif isinstance(n, list):
        for v in n: walk(v)
walk(d.get("hooks", {}))
p.write_text(json.dumps(d, indent=2), encoding="utf-8")
PYSUB
chmod -x "$FIX_SUB/.claude/hooks/hook-health.sh"
rc=$(run_gate "$FIX_SUB" "$TMP/subhook.log")
if [ "$rc" -ne 0 ] && grep -q "command substitution runs" "$TMP/subhook.log"; then
  ok "a hook invoked inside a command substitution does not hide behind the outer echo"
else
  bad "a substituted hook escaped the executable check" "$(tail -3 "$TMP/subhook.log")"
fi

# End-to-end, the false-rejection half: a quoted operator is data and must leave the gate green.
FIX_QOP="$TMP/quotedop"
make_fixture "$FIX_QOP"
python3 - "$FIX_QOP" <<'PYQOP'
import json, pathlib, sys
p = pathlib.Path(sys.argv[1]) / ".claude/settings.json"
d = json.loads(p.read_text(encoding="utf-8"))
done = []
def walk(n):
    if isinstance(n, dict):
        c = n.get("command")
        if n.get("type") == "command" and isinstance(c, str) and c.startswith("echo") and not done:
            n["command"] = 'echo ";"'
            done.append(1)
        for v in n.values(): walk(v)
    elif isinstance(n, list):
        for v in n: walk(v)
walk(d.get("hooks", {}))
assert done, "fixture assumption: settings.json has an echo hook"
p.write_text(json.dumps(d, indent=2), encoding="utf-8")
PYQOP
rc=$(run_gate "$FIX_QOP" "$TMP/quotedop.log")
if [ "$rc" -eq 0 ]; then
  ok "an argument made of quoted punctuation is data, not a composition"
else
  bad "a quoted operator argument was mistaken for a composition" "$(tail -3 "$TMP/quotedop.log")"
fi

# End-to-end: a hook carrying an explanatory comment with punctuation in it must stay green.
FIX_CMT="$TMP/commenthook"
make_fixture "$FIX_CMT"
python3 - "$FIX_CMT" <<'PYCMT'
import json, pathlib, sys
p = pathlib.Path(sys.argv[1]) / ".claude/settings.json"
d = json.loads(p.read_text(encoding="utf-8"))
def walk(n):
    if isinstance(n, dict):
        c = n.get("command")
        if n.get("type") == "command" and isinstance(c, str) and c.endswith("hook-health.sh"):
            n["command"] = c + " # health check; see docs && runbook"
        for v in n.values(): walk(v)
    elif isinstance(n, list):
        for v in n: walk(v)
walk(d.get("hooks", {}))
p.write_text(json.dumps(d, indent=2), encoding="utf-8")
PYCMT
rc=$(run_gate "$FIX_CMT" "$TMP/commenthook.log")
if [ "$rc" -eq 0 ]; then
  ok "operator text inside a comment is not read as composition"
else
  bad "a comment containing punctuation was read as a composition" "$(tail -3 "$TMP/commenthook.log")"
fi

# End-to-end: a quoted `)` must not close the substitution early and let the hook vanish.
FIX_PAREN="$TMP/parenhook"
make_fixture "$FIX_PAREN"
python3 - "$FIX_PAREN" <<'PYPAREN'
import json, pathlib, sys
p = pathlib.Path(sys.argv[1]) / ".claude/settings.json"
d = json.loads(p.read_text(encoding="utf-8"))
def walk(n):
    if isinstance(n, dict):
        c = n.get("command")
        if n.get("type") == "command" and isinstance(c, str) and c.endswith("hook-health.sh"):
            n["command"] = 'echo "$(echo \')\'; %s)"' % c
        for v in n.values(): walk(v)
    elif isinstance(n, list):
        for v in n: walk(v)
walk(d.get("hooks", {}))
p.write_text(json.dumps(d, indent=2), encoding="utf-8")
PYPAREN
chmod -x "$FIX_PAREN/.claude/hooks/hook-health.sh"
rc=$(run_gate "$FIX_PAREN" "$TMP/parenhook.log")
if [ "$rc" -ne 0 ] && grep -q "command substitution runs" "$TMP/parenhook.log"; then
  ok "a quoted parenthesis does not close the substitution early"
else
  bad "a hook hid behind a quoted parenthesis in a substitution" "$(tail -3 "$TMP/parenhook.log")"
fi

# End-to-end: an escaped backtick must not close the substitution early either.
FIX_TICK="$TMP/tickhook"
make_fixture "$FIX_TICK"
python3 - "$FIX_TICK" <<'PYTICK'
import json, pathlib, sys
p = pathlib.Path(sys.argv[1]) / ".claude/settings.json"
d = json.loads(p.read_text(encoding="utf-8"))
def walk(n):
    if isinstance(n, dict):
        c = n.get("command")
        if n.get("type") == "command" and isinstance(c, str) and c.endswith("hook-health.sh"):
            n["command"] = 'echo "`printf \'\\`\'; %s`"' % c
        for v in n.values(): walk(v)
    elif isinstance(n, list):
        for v in n: walk(v)
walk(d.get("hooks", {}))
p.write_text(json.dumps(d, indent=2), encoding="utf-8")
PYTICK
chmod -x "$FIX_TICK/.claude/hooks/hook-health.sh"
rc=$(run_gate "$FIX_TICK" "$TMP/tickhook.log")
if [ "$rc" -ne 0 ] && grep -q "command substitution runs" "$TMP/tickhook.log"; then
  ok "an escaped backtick does not close the substitution early"
else
  bad "a hook hid behind an escaped backtick" "$(tail -3 "$TMP/tickhook.log")"
fi

# End-to-end, both halves. A dropped target hides in the derived list AND in the expected
# count, so only a full run proves the gate goes red.
FIX_ESC="$TMP/escapehook"
make_fixture "$FIX_ESC"
python3 - "$FIX_ESC" <<'PYESC'
import json, pathlib, sys
root = pathlib.Path(sys.argv[1]).resolve()
up = "../" * len(root.parts[1:])
esc = '"$CLAUDE_PROJECT_DIR"/.claude/../%susr/bin/env ' % up
p = root / ".claude/settings.json"
d = json.loads(p.read_text(encoding="utf-8"))
def walk(n):
    if isinstance(n, dict):
        c = n.get("command")
        if n.get("type") == "command" and isinstance(c, str) and "hook-health.sh" in c:
            n["command"] = esc + c
        for v in n.values(): walk(v)
    elif isinstance(n, list):
        for v in n: walk(v)
walk(d.get("hooks", {}))
p.write_text(json.dumps(d, indent=2), encoding="utf-8")
PYESC
rc=$(run_gate "$FIX_ESC" "$TMP/escapehook.log")
if [ "$rc" -ne 0 ] && grep -q "outside the repository" "$TMP/escapehook.log"; then
  ok "a hook path traversing out of the repository fails the gate instead of certifying /usr/bin/env"
else
  bad "an escaping hook path was accepted as a repository executable" "$(tail -3 "$TMP/escapehook.log")"
fi

FIX_PYNAME="$TMP/pynamehook"
make_fixture "$FIX_PYNAME"
mv "$FIX_PYNAME/.claude/hooks/hook-health.sh" "$FIX_PYNAME/.claude/hooks/python3"
python3 - "$FIX_PYNAME" <<'PYNAME'
import json, pathlib, sys
p = pathlib.Path(sys.argv[1]) / ".claude/settings.json"
d = json.loads(p.read_text(encoding="utf-8"))
def walk(n):
    if isinstance(n, dict):
        c = n.get("command")
        if n.get("type") == "command" and isinstance(c, str) and "hook-health.sh" in c:
            n["command"] = c.replace("hook-health.sh", "python3")
        for v in n.values(): walk(v)
    elif isinstance(n, list):
        for v in n: walk(v)
walk(d.get("hooks", {}))
p.write_text(json.dumps(d, indent=2), encoding="utf-8")
PYNAME
chmod -x "$FIX_PYNAME/.claude/hooks/python3"
rc=$(run_gate "$FIX_PYNAME" "$TMP/pynamehook.log")
if [ "$rc" -ne 0 ] && grep -q "not executable: .claude/hooks/python3" "$TMP/pynamehook.log"; then
  ok "a repository hook named python3 is still checked for its executable bit"
else
  bad "a repository hook was exempted because of its basename" "$(tail -3 "$TMP/pynamehook.log")"
fi


# An unclassifiable configured hook must FAIL the gate, not vanish from it. This is the
# self-concealing shape: an unparsed form used to return None, the target left the derived
# set, and the expected count shrank to match, so the gate stayed green.
FIX_UNC="$TMP/unclassified"
make_fixture "$FIX_UNC"
python3 - "$FIX_UNC" <<'PYUNC'
import json, pathlib, sys
p = pathlib.Path(sys.argv[1]) / ".claude/settings.json"
d = json.loads(p.read_text(encoding="utf-8"))
def walk(n):
    if isinstance(n, dict):
        c = n.get("command")
        if n.get("type") == "command" and isinstance(c, str) and "hook-health.sh" in c:
            n["command"] = 'true && ' + c
        for v in n.values(): walk(v)
    elif isinstance(n, list):
        for v in n: walk(v)
walk(d.get("hooks", {}))
p.write_text(json.dumps(d, indent=2), encoding="utf-8")
PYUNC
rc=$(run_gate "$FIX_UNC" "$TMP/unclassified.log")
if [ "$rc" -ne 0 ] && grep -q "cannot be classified" "$TMP/unclassified.log"; then
  ok "a compound hook command fails the gate and is named"
else
  bad "a compound hook command did not fail the gate" "$(tail -3 "$TMP/unclassified.log")"
fi

# End-to-end: an absolute launcher must FAIL rather than quietly removing the hook from the
# derived set. Bash runs the hook through /usr/bin/env and returns 126 when it is not
# executable, so classifying the entry as a harmless external command hid a real breakage.
FIX_ABS="$TMP/abslauncher"
make_fixture "$FIX_ABS"
python3 - "$FIX_ABS" <<'PYABS'
import json, pathlib, sys
p = pathlib.Path(sys.argv[1]) / ".claude/settings.json"
d = json.loads(p.read_text(encoding="utf-8"))
def walk(n):
    if isinstance(n, dict):
        c = n.get("command")
        if n.get("type") == "command" and isinstance(c, str) and "hook-health.sh" in c:
            n["command"] = "/usr/bin/env " + c
        for v in n.values(): walk(v)
    elif isinstance(n, list):
        for v in n: walk(v)
walk(d.get("hooks", {}))
p.write_text(json.dumps(d, indent=2), encoding="utf-8")
PYABS
rc=$(run_gate "$FIX_ABS" "$TMP/abslauncher.log")
if [ "$rc" -ne 0 ] && grep -q "cannot be classified" "$TMP/abslauncher.log"; then
  ok "an absolute launcher fails the gate and is named"
else
  bad "an absolute launcher did not fail the gate" "$(tail -3 "$TMP/abslauncher.log")"
fi

# Adding a hook must not trip the exact-count assertion.
FIX_ADD="$TMP/addhook"
make_fixture "$FIX_ADD"
python3 - "$FIX_ADD" <<'PYADD'
import json, os, pathlib, sys
root = pathlib.Path(sys.argv[1])
new = root / ".claude/hooks/extra-guard.sh"
new.write_text("#!/usr/bin/env bash\nexit 0\n", encoding="utf-8")
os.chmod(new, 0o755)
p = root / ".claude/settings.json"
d = json.loads(p.read_text(encoding="utf-8"))
d["hooks"].setdefault("PreToolUse", []).append(
    {"matcher": "Bash", "hooks": [{"type": "command",
     "command": '"$CLAUDE_PROJECT_DIR"/.claude/hooks/extra-guard.sh'}]})
p.write_text(json.dumps(d, indent=2), encoding="utf-8")
PYADD
rc=$(run_gate "$FIX_ADD" "$TMP/addhook.log")
if [ "$rc" -eq 0 ]; then
  ok "adding an executable hook does not trip the exact-count assertion"
else
  bad "adding a hook broke the count assertion" "$(tail -3 "$TMP/addhook.log")"
fi

echo
printf 'passed: %d  failed: %d\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
