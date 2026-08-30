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
print(int(m.group(1)) + len(mod.direct_hook_targets(settings, root)))
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
python3 -c "
import importlib.util, json, pathlib, sys
spec = importlib.util.spec_from_file_location('g', sys.argv[1])
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
root = pathlib.Path(sys.argv[2])
s = json.loads((root / '.claude/settings.json').read_text(encoding='utf-8'))
sys.exit(0 if len(m.direct_hook_targets(s, root)) == 9 else 1)
" "$GATE" "$FIX_BAD"
if [ $? -eq 0 ]; then
  ok "a malformed hook command does not abort the derivation walk"
else
  bad "a malformed hook command truncated the derived target list" ""
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
