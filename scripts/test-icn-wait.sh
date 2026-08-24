#!/usr/bin/env bash
# Regression tests for ops/scripts/icn-wait.
#
# Every non-terminating pattern recovered from the icn-dev process table during the #2644
# lifecycle investigation is pinned here, in two halves:
#
#   NEGATIVE CONTROL — reproduce the original defective loop and assert it does NOT finish.
#                      Without this, a "the helper terminates" test proves nothing: it could
#                      pass because the condition was trivially true, not because the bug is fixed.
#   POSITIVE         — assert icn-wait, given the SAME inputs, terminates with a correct verdict.
#
# Usage: bash scripts/test-icn-wait.sh

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WAIT="$ROOT/ops/scripts/icn-wait"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"; pkill -P $$ 2>/dev/null || true' EXIT

PASS=0; FAIL=0
ok()   { PASS=$((PASS+1)); printf '  ok    %s\n' "$1"; }
bad()  { FAIL=$((FAIL+1)); printf '  FAIL  %s\n     -> %s\n' "$1" "${2:-}" >&2; }
check(){ # check <name> <expected-exit> <actual-exit>
  if [ "$2" = "$3" ]; then ok "$1"; else bad "$1" "expected exit $2, got $3"; fi
}

echo "icn-wait regression tests"
echo

# ── 1. The self-matching pgrep defect (PIDs 296141, 2843900 on icn-dev) ──────
echo "1. self-matching 'pgrep -f' waits"

# A marker unique to this run. The recovered patterns ("scratchpad/mutate.py",
# "scratchpad/run_mutations.sh") are deliberately NOT used literally: on a machine where a real
# process still matches them the wait would be legitimate, and the test would measure the VM
# rather than the helper. The marker reproduces the exact SHAPE of the defect hermetically —
# a pattern that appears in the waiting process's own command line.
MARKER="scratchpad/selftest-$$-$RANDOM.py"

# NEGATIVE CONTROL: the original loop, verbatim in shape. It must still hang, proving the
# pathology is real and that our timeout harness can observe it. Nothing else on the machine
# matches MARKER, so the ONLY thing it can be matching is itself.
timeout 6 bash -c "until ! pgrep -f \"$MARKER\" >/dev/null 2>&1; do sleep 1; done" >/dev/null 2>&1
rc=$?
if [ "$rc" -eq 124 ]; then
  ok "naive 'until ! pgrep -f PATTERN' never terminates when only itself matches (defect reproduced)"
else
  bad "naive self-matching loop should hang" "exited $rc instead of timing out"
fi

# Confirm the mechanism rather than just the symptom: a pgrep for MARKER, issued from a shell
# whose own argv contains MARKER, finds that shell.
selfmatch="$(bash -c "pgrep -f \"$MARKER\" | wc -l" 2>/dev/null)"
if [ "${selfmatch:-0}" -ge 1 ]; then
  ok "mechanism confirmed: 'pgrep -f' matches the shell that names the pattern"
else
  bad "expected the observing shell to self-match" "matched $selfmatch processes"
fi

# POSITIVE: icn-wait given the identical pattern — which is likewise present in its own argv,
# and in its parent `timeout`'s argv — terminates immediately, because the observer and its
# whole process tree are excluded.
timeout 20 "$WAIT" match "$MARKER" --timeout 10 --quiet >/dev/null 2>&1
check "icn-wait match excludes the observer and returns" 0 $?

# ...and it must NOT be excluding everything: a genuine third-party match is still waited for.
setsid bash -c "exec -a \"holder-$MARKER\" sleep 12" >/dev/null 2>&1 &
sleep 1
start=$(date +%s)
timeout 20 "$WAIT" match "$MARKER" --timeout 15 --quiet >/dev/null 2>&1
rc=$?; waited=$(( $(date +%s) - start ))
if [ "$rc" -eq 0 ] && [ "$waited" -ge 2 ]; then
  ok "icn-wait match genuinely waits for a foreign match (${waited}s), then returns"
else
  bad "foreign match should be waited for" "exit $rc after ${waited}s"
fi

# An unusable pattern must FAIL, not report the wait as satisfied. pgrep exits 2 on a bad regex
# while printing nothing, which is indistinguishable from "no matches" — so this used to exit 0,
# telling the caller the thing it was waiting for had finished.
timeout 20 "$WAIT" match 'foo(' --timeout 5 >/dev/null 2>&1
check "invalid match pattern fails fast (exit 3), never reports success" 3 $?

# A real foreign process, waited for exactly by PID.
sleep 9 & foreign=$!
( sleep 2; kill "$foreign" 2>/dev/null ) &
timeout 20 "$WAIT" pid "$foreign" --timeout 15 --quiet >/dev/null 2>&1
check "icn-wait pid waits for a real foreign process, then returns" 0 $?
wait "$foreign" 2>/dev/null

echo

# ── 2. The unbounded-sentinel defect (PID 220945 on icn-dev) ─────────────────
echo "2. sentinel-file waits"

# NOTE ON THE PROPERTY UNDER TEST. Unlike the self-matching pgrep above, a sentinel wait is not
# logically impossible — a file can legitimately arrive later. The defect is that the unbounded,
# error-swallowing form cannot distinguish a working producer from a dead one, a deleted
# directory, or a sentinel that will never come. This control pins the WORST case (a directory
# nothing will ever create), where the wait is in fact unsatisfiable AND cannot say so.
timeout 6 bash -c '
  until grep -q "^EXIT=" /nonexistent-dir-xyz/full-test-3.log 2>/dev/null; do sleep 1; done
' >/dev/null 2>&1
rc=$?
if [ "$rc" -eq 124 ]; then
  ok "naive 'until grep -q PATTERN missing-file' spins with no way to report failure (defect reproduced)"
else
  bad "naive sentinel loop should spin" "exited $rc instead of timing out"
fi

# POSITIVE: unreachable sentinel is a distinct, immediate failure — not a wait.
timeout 10 "$WAIT" file /nonexistent-dir-xyz/full-test-3.log --pattern '^EXIT=' --timeout 30 >/dev/null 2>&1
check "missing sentinel DIRECTORY fails fast as unreachable (exit 3)" 3 $?

# A sentinel that could appear but does not: bounded, and the timeout is observable.
start=$(date +%s)
timeout 20 "$WAIT" file "$TMP/never.log" --pattern '^EXIT=' --timeout 3 >"$TMP/to.err" 2>&1
rc=$?; elapsed=$(( $(date +%s) - start ))
check "absent-but-possible sentinel times out (exit 1)" 1 $rc
if [ "$elapsed" -lt 15 ]; then ok "timeout is bounded (${elapsed}s)"; else bad "timeout not bounded" "${elapsed}s"; fi
if grep -q "TIMEOUT" "$TMP/to.err"; then ok "timeout path is observable on stderr"; else bad "timeout not reported" "$(cat "$TMP/to.err")"; fi

# A sentinel whose producer has died can never be satisfied.
sleep 0.1 & dead=$!; wait "$dead" 2>/dev/null
timeout 10 "$WAIT" file "$TMP/from-dead.log" --source-pid "$dead" --timeout 30 >/dev/null 2>&1
check "dead producer makes the sentinel unreachable (exit 3)" 3 $?

# The happy path still works.
( sleep 1; echo "EXIT=0" > "$TMP/done.log" ) &
timeout 20 "$WAIT" file "$TMP/done.log" --pattern '^EXIT=' --timeout 15 >/dev/null 2>&1
check "sentinel that does appear is detected" 0 $?

echo

# ── 3. Direct PID / command waits ────────────────────────────────────────────
echo "3. exact waits (the forms agents should reach for first)"

timeout 20 "$WAIT" cmd --timeout 15 -- true >/dev/null 2>&1
check "cmd wait completes and propagates success" 0 $?

timeout 20 "$WAIT" cmd --timeout 15 -- false >/dev/null 2>&1
check "cmd wait propagates the child's failure exit code" 1 $?

timeout 20 "$WAIT" cmd --timeout 2 -- sleep 30 >/dev/null 2>&1
check "cmd wait enforces its own timeout on a slow child" 1 $?

timeout 10 "$WAIT" pid "$$" --timeout 5 >/dev/null 2>&1
check "refuses to wait on itself (exit 3)" 3 $?

parent="$(ps -o ppid= -p $$ | tr -d ' ')"
timeout 10 "$WAIT" pid "$parent" --timeout 5 >/dev/null 2>&1
check "refuses to wait on an ancestor (exit 3)" 3 $?

timeout 10 "$WAIT" pid 999999999 --timeout 5 >/dev/null 2>&1
check "already-gone PID returns immediately" 0 $?

echo

# ── 4. Boundedness is the default, not an option ─────────────────────────────
echo "4. unbounded waiting requires explicit opt-in"

timeout 10 "$WAIT" file "$TMP/never2.log" --timeout 0 >/dev/null 2>&1
check "--timeout 0 without --allow-unbounded is a usage error (exit 2)" 2 $?

timeout 6 "$WAIT" file "$TMP/never3.log" --timeout 0 --allow-unbounded >/dev/null 2>&1
check "--allow-unbounded is honoured (hangs until killed)" 124 $?

timeout 10 "$WAIT" file "$TMP/x" --timeout abc >/dev/null 2>&1
check "non-numeric timeout is rejected (exit 2)" 2 $?

echo

# ── 5. the anti-hang tool must never hang in its own argument parser ─────────
echo "5. the anti-hang tool must not hang"

# `shift 2` with one argument left FAILS and shifts nothing, so the parser spun at 100% CPU
# forever. Realistic trigger: `--timeout $T` where $T is unset.
timeout 8 "$WAIT" pid 2 --timeout >/dev/null 2>&1
check "a dangling option value is a usage error, not an infinite loop" 2 $?

timeout 8 "$WAIT" file /tmp/x --pattern >/dev/null 2>&1
check "same for a dangling --pattern" 2 $?

# An oversized value made the `-gt 0` test error out (no set -e), leaving DEADLINE=0 so
# expired() was never true — a silently UNBOUNDED wait from a flag that looks like a bound.
timeout 8 "$WAIT" pid 2 --timeout 99999999999999999999 >/dev/null 2>&1
check "an oversized --timeout is rejected, not silently unbounded" 2 $?

timeout 8 "$WAIT" pid 2 --timeout 9223372036854775807 >/dev/null 2>&1
check "an int64-overflowing --timeout is rejected" 2 $?

echo

# ── 6. signals must stop the wait AND reap the child ─────────────────────────
echo "6. signal handling"

# A previous edit deleted the on_signal() body and left `trap on_signal` in place. Bash printed
# "on_signal: command not found" and RESUMED the loop, so SIGTERM neither stopped icn-wait nor
# reaped the child — and because the child runs under setsid, killing our process group missed
# it too. Verify BOTH halves, finding the child by ppid rather than by pattern (a pgrep here
# would match this script's own command line — the very defect under test).
"$WAIT" cmd --timeout 120 -- sleep 247 >/dev/null 2>&1 &
waiter=$!
sleep 1.5
child="$(ps -eo pid,ppid --no-headers | awk -v p="$waiter" '$2==p {print $1}' | head -1)"
kill -TERM "$waiter" 2>/dev/null
sleep 3
if kill -0 "$waiter" 2>/dev/null; then
  bad "SIGTERM must stop icn-wait" "still running"
  kill -9 "$waiter" 2>/dev/null
else
  ok "SIGTERM stops the wait"
fi
if [ -n "$child" ] && kill -0 "$child" 2>/dev/null; then
  bad "SIGTERM must reap the child" "pid $child survived"
  kill -9 "$child" 2>/dev/null
else
  ok "SIGTERM reaps the setsid-detached child"
fi
if grep -q "on_signal" "$WAIT" && ! grep -q "on_signal()" "$WAIT"; then
  bad "trap references an undefined handler" "on_signal is trapped but never defined"
else
  ok "the trapped handler is actually defined"
fi

# An unvalidated poll interval busy-looped at ~100% CPU firing sleep errors.
start=$(date +%s)
ICN_WAIT_POLL_INTERVAL=0 timeout 8 "$WAIT" file "$TMP/never-poll.log" --timeout 3 >/dev/null 2>"$TMP/poll.err"
elapsed=$(( $(date +%s) - start ))
# The TIMEOUT line on stderr is expected; a busy-loop shows up as hundreds of `sleep` errors.
# `grep -c` prints 0 AND exits 1 when there are no matches, so `|| echo 0` appended a second
# zero and the numeric test then errored on "0\n0".
sleep_errs=$(grep -c "sleep" "$TMP/poll.err" 2>/dev/null | head -1)
sleep_errs=${sleep_errs:-0}
if [ "$elapsed" -ge 2 ] && [ "$sleep_errs" -eq 0 ]; then
  ok "an invalid ICN_WAIT_POLL_INTERVAL falls back instead of busy-looping"
else
  bad "invalid poll interval" "elapsed=${elapsed}s sleep-errors=${sleep_errs}"
fi

echo
printf 'passed: %d  failed: %d\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
