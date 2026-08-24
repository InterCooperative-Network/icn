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
DECLARED=$(python3 -c '
import re, sys
src = open(sys.argv[1]).read()
m = re.search(r"^EXPECTED_CHECKS\s*=\s*(\d+)\s*$", src, re.M)
print(m.group(1) if m else "")
' "$GATE")
if [ -n "$BASE_COUNT" ] && [ "$DECLARED" = "$BASE_COUNT" ]; then
  ok "EXPECTED_CHECKS ($BASE_COUNT) matches what a complete run actually performs"
else
  bad "EXPECTED_CHECKS does not match the observed count" "declared=${DECLARED:-<unparseable>} observed=$BASE_COUNT"
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
if missing: print("MISSING: %s" % ",".join(missing))
elif not isinstance(d.get("live_agent_pids"), list): print("live_agent_pids is not a list")
elif not isinstance(d.get("contention"), dict): print("contention is not an object")
else: print("OK")
')
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

echo
printf 'passed: %d  failed: %d\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
