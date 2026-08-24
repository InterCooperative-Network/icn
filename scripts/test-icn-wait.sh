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
check "cmd propagates the child's failure status (1)" 1 $?

timeout 20 "$WAIT" cmd --timeout 15 -- bash -c 'exit 77' >/dev/null 2>&1
check "cmd propagates an arbitrary child status (77)" 77 $?

# Distinct from ANY child status: "the suite failed" and "the wait expired" were
# indistinguishable while both reported 1, for the form the docs call PREFERRED.
timeout 20 "$WAIT" cmd --timeout 15 -- bash -c 'exit 124' >/dev/null 2>&1
check "cmd propagates 124 from the child too" 124 $?

timeout 20 "$WAIT" cmd --timeout 2 -- sleep 30 >/dev/null 2>&1
check "cmd reports its own timeout as 124, not as a child status" 124 $?

# `$$` here is the TEST's pid, which is icn-wait's ancestor — so this exercised the ancestor
# guard, not the self guard, and deleting the self check left the suite green. Make icn-wait
# report its own pid and wait on that.
timeout 10 "$WAIT" pid "$$" --timeout 5 >/dev/null 2>&1
check "refuses to wait on an ancestor via \$\$ (exit 3)" 3 $?
SELF_ERR="$(timeout 10 bash -c "exec \"$WAIT\" pid \$\$ --timeout 5" 2>&1 >/dev/null)"; SELF_RC=$?
case "$SELF_ERR" in
  *"this very process"*|*"ancestors"*) ok "refuses to wait on itself, and says which case" ;;
  *) bad "self-wait must be refused with a reason" "rc=$SELF_RC err=${SELF_ERR:-<silence>}" ;;
esac

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

# THE NUMERIC CAP, ON ITS OWN. Both assertions above are satisfied by the LENGTH guard
# (`${#TIMEOUT} -gt 9`) without the numeric comparison ever running, so deleting
# `[ "$TIMEOUT" -gt "$MAX_TIMEOUT" ]` left them green while `--timeout 999999999` became an
# effectively unbounded wait. These two values live in the band only the numeric cap can
# reject: 8 and 9 digits, so the length guard passes them, and both exceed 31536000.
timeout 8 "$WAIT" pid 2 --timeout 31536001 >/dev/null 2>&1
check "--timeout just over the 1-year cap is rejected by the NUMERIC cap (8 digits)" 2 $?

timeout 8 "$WAIT" pid 2 --timeout 999999999 >/dev/null 2>&1
check "--timeout 999999999 is rejected by the NUMERIC cap (9 digits, length guard passes it)" 2 $?

# ...and the boundary itself must still be ACCEPTED, or the cap is just an arbitrary refusal.
# Waited against a pid that does not exist, so the condition is met immediately: rc 0 means the
# value was accepted and the wait ran, rc 2 would mean the cap rejected its own boundary.
timeout 8 "$WAIT" pid 4194303 --timeout 31536000 >/dev/null 2>&1
check "--timeout exactly at the cap is ACCEPTED (rc 0 = ran, not 2 = rejected)" 0 $?

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
# PRECONDITION. Without this the whole block passed against a `cmd` that never launched
# anything: an empty $child short-circuits every later check to its ok branch.
if [ -z "$child" ]; then
  bad "precondition: cmd must launch a child before signalling" "no child of pid $waiter"
  kill -9 "$waiter" 2>/dev/null
else
  ok "precondition: cmd launched child $child"
fi
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

# An unvalidated poll interval busy-loops. DETECTING IT NEEDS A FORK-RATE MEASUREMENT, not a
# stderr scan: the round-5 version of this test counted `sleep`-related stderr lines, but
# `sleep 0` is a VALID command that exits 0 and prints nothing — so the test passed against a
# deliberately broken script (measured: 1076 forks in 3s vs 10, and the test said ok). That is
# the second time a test written to close a vacuity finding was itself vacuous, which is why
# this one asserts on the observable pathology instead of a side effect of it.
# Measure the WAITER'S OWN cpu time, not the system-wide fork counter: /proc/stat `processes`
# counts every fork on the box, and measured idle noise of 261-273 per 4s made this assertion
# report three different outcomes across three runs of the UNMODIFIED script. A test that
# measures the machine instead of the tool is worse than no test.
cpu_ticks_of() {  # $1 = pid -> utime+stime in clock ticks
  awk '{print $14 + $15}' "/proc/$1/stat" 2>/dev/null || echo 0
}
busy_ticks() {    # $1 = poll interval -> cpu ticks burned by icn-wait over ~3s
  ICN_WAIT_POLL_INTERVAL="$1" "$WAIT" file "$TMP/never-poll-$1.log" --timeout 6 >/dev/null 2>&1 &
  local w=$! ticks
  sleep 3
  ticks=$(cpu_ticks_of "$w")
  kill -TERM "$w" 2>/dev/null; wait "$w" 2>/dev/null
  echo "${ticks:-0}"
}
BUSY_TICK_LIMIT=10   # a 2s-interval poll burns ~0 ticks; anything above this is spinning
for badval in 0 00 0.0 .0 0.001 0.0001 .05 abc; do
  n=$(busy_ticks "$badval")
  if [ "$n" -lt "$BUSY_TICK_LIMIT" ]; then
    ok "ICN_WAIT_POLL_INTERVAL=$badval does not busy-loop (${n} cpu ticks in 3s)"
  else
    bad "ICN_WAIT_POLL_INTERVAL=$badval busy-loops" "${n} cpu ticks in 3s"
  fi
done

# KILL_GRACE is validated and capped like the timeout, or a `--timeout 2` wait can last days.
cat > "$TMP/stubborn.sh" <<'EOS'
#!/usr/bin/env bash
trap '' TERM
sleep 300
EOS
chmod +x "$TMP/stubborn.sh"
# 999999 is six digits and is caught by the LENGTH check alone, so the numeric cap was never
# exercised — deleting it left the suite green. 9999 passes the length check and can only be
# stopped by the cap itself.
for grace in 999999 9999; do
  start=$(date +%s)
  ICN_WAIT_KILL_GRACE=$grace timeout 120 "$WAIT" cmd --timeout 2 -- "$TMP/stubborn.sh" >/dev/null 2>&1
  elapsed=$(( $(date +%s) - start ))
  # timeout(2) + poll + capped grace(60) + slack. An uncapped value blows straight past this.
  if [ "$elapsed" -lt 90 ]; then
    ok "ICN_WAIT_KILL_GRACE=$grace is capped (${elapsed}s)"
  else
    bad "ICN_WAIT_KILL_GRACE=$grace is uncapped" "${elapsed}s"
  fi
done

# Under job control, setsid FORKS instead of exec'ing, so `$!` was the setsid parent and the
# wait returned success while the real command was still running in a detached session.
rm -f "$TMP/jc.mark"
start=$(date +%s)
env SHELLOPTS=monitor "$WAIT" cmd --timeout 30 -- bash -c "sleep 4; echo alive > $TMP/jc.mark" >/dev/null 2>&1
rc=$?; elapsed=$(( $(date +%s) - start ))
if [ "$rc" -eq 0 ] && [ -f "$TMP/jc.mark" ] && [ "$elapsed" -ge 3 ]; then
  ok "cmd waits for the real command even under job control (${elapsed}s)"
else
  bad "cmd returned before the command finished" "rc=$rc elapsed=${elapsed}s marker=$([ -f "$TMP/jc.mark" ] && echo present || echo absent)"
fi

echo

# ── 8. a PID that is not a process, and a bound that another knob can override ──
echo "8. non-process pids and interacting bounds"

# POSIX gives `kill -0 0` a completely different meaning — it signals the CALLER'S OWN process
# group — so it always succeeds. `icn-wait pid 0` therefore reported the target alive forever:
# a logical deadlock, not a wait. Measured before the fix: exit 1 only when the timeout fired,
# and under --allow-unbounded it never returned at all.
timeout 8 "$WAIT" pid 0 --timeout 3 >/dev/null 2>&1
check "pid 0 is rejected as not-a-process (exit 2), never waited on" 2 $?

timeout 8 "$WAIT" pid -5 --timeout 3 >/dev/null 2>&1
check "a negative pid is rejected (exit 2)" 2 $?

timeout 8 "$WAIT" file "$TMP/never-appears" --source-pid 0 --timeout 3 >/dev/null 2>&1
check "--source-pid 0 is rejected (exit 2), so the dead-producer exit can still fire" 2 $?

# A live, real pid must still be waited on, or the guards above would be indistinguishable from
# a tool that refuses everything.
sleep 5 &
live_pid=$!
timeout 8 "$WAIT" pid "$live_pid" --timeout 2 >/dev/null 2>&1
check "a real live pid is still waited on (exit 1 = timed out, not 2 = rejected)" 1 $?
kill "$live_pid" 2>/dev/null; wait "$live_pid" 2>/dev/null

# ICN_WAIT_POLL_INTERVAL had a lower bound and NO upper bound, so an env var silently overrode
# an explicit one: --timeout 2 with POLL=45 ran for 45 seconds. Each sleep is now clamped to the
# remaining budget, so the deadline holds regardless of the interval.
start=$(date +%s)
ICN_WAIT_POLL_INTERVAL=45 timeout 30 "$WAIT" file "$TMP/never-appears" --timeout 3 >/dev/null 2>&1
rc=$?; elapsed=$(( $(date +%s) - start ))
if [ "$rc" -eq 1 ] && [ "$elapsed" -le 6 ]; then
  ok "a huge ICN_WAIT_POLL_INTERVAL cannot overrun --timeout (${elapsed}s for a 3s bound)"
else
  bad "poll interval overrode the timeout" "rc=$rc elapsed=${elapsed}s (expected rc=1 within ~3s)"
fi

start=$(date +%s)
ICN_WAIT_POLL_INTERVAL=999999 timeout 30 "$WAIT" file "$TMP/never-appears" --timeout 3 >/dev/null 2>&1
rc=$?; elapsed=$(( $(date +%s) - start ))
if [ "$rc" -eq 1 ] && [ "$elapsed" -le 6 ]; then
  ok "an absurd ICN_WAIT_POLL_INTERVAL is capped and still honours --timeout (${elapsed}s)"
else
  bad "absurd poll interval overrode the timeout" "rc=$rc elapsed=${elapsed}s"
fi

echo
printf 'passed: %d  failed: %d\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
