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
# BOTH halves must be present. The old form was `grep -q on_signal && ! grep -q "on_signal()"`,
# which is satisfied when everything is there AND when everything is gone — deleting the `trap`
# line entirely left it green, so it could not detect the very regression it names.
trap_line=$(grep -c '^[[:space:]]*trap on_signal' "$WAIT")
handler_def=$(grep -c '^[[:space:]]*on_signal()' "$WAIT")
if [ "$trap_line" -ge 1 ] && [ "$handler_def" -ge 1 ]; then
  ok "the signal handler is both DEFINED and TRAPPED (${trap_line} trap, ${handler_def} def)"
else
  bad "signal handling is not wired" \
      "trap lines=$trap_line handler definitions=$handler_def (both must be >= 1)"
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
# A 2s-interval poll burns ~0 ticks over the sample window; anything above this is spinning.
# TIGHTENED FROM 10. A mutant admitting `.05` (a 20 Hz poll) burned 6-9 ticks and scored one
# under the old limit — passing while spinning, and latently flaky besides. That is verbatim the
# failure icn-wait's own comment names: "scored just under the test's threshold rather than
# being correct". Measured baseline for every accepted interval is 0.
BUSY_TICK_LIMIT=3
cpu_ticks_of() {  # $1 = pid -> utime+stime in clock ticks, or DEAD if it is not running
  # DEAD, not 0. A corpse has no /proc entry, so `|| echo 0` reported the same "0 ticks" as a
  # politely-sleeping waiter — and every assertion below reads low ticks as success. A mutant
  # that made icn-wait EXIT IMMEDIATELY, performing no wait at all, therefore passed all eight
  # busy-loop assertions. The measurement must be able to say "there was nothing to measure".
  awk '{print $14 + $15}' "/proc/$1/stat" 2>/dev/null || echo DEAD
}
busy_ticks() {    # $1 = poll interval -> cpu ticks burned by icn-wait over ~3s, or DEAD
  ICN_WAIT_POLL_INTERVAL="$1" "$WAIT" file "$TMP/never-poll-$1.log" --timeout 6 >/dev/null 2>&1 &
  local w=$! ticks
  sleep 3
  # PRECONDITION: it must still be waiting. Without this the whole measurement is unfalsifiable.
  ticks=$(cpu_ticks_of "$w")
  kill -TERM "$w" 2>/dev/null; wait "$w" 2>/dev/null
  echo "${ticks:-DEAD}"
}

# One place decides whether a busy-tick sample means "polled politely" or "never waited".
assert_polite() {  # $1 = label suffix, $2 = sample
  if [ "$2" = "DEAD" ]; then
    bad "ICN_WAIT_POLL_INTERVAL=$1 does not busy-loop" \
        "the waiter was NOT RUNNING when sampled — it never waited, so 'low cpu' proves nothing"
  elif [ "$2" -lt "$BUSY_TICK_LIMIT" ]; then
    ok "ICN_WAIT_POLL_INTERVAL=$1 does not busy-loop (${2} cpu ticks in 3s, still waiting)"
  else
    bad "ICN_WAIT_POLL_INTERVAL=$1 busy-loops" "${2} cpu ticks in 3s"
  fi
}
for badval in 0 00 0.0 .0 0.001 0.0001 .05 abc; do
  assert_polite "$badval" "$(busy_ticks "$badval")"
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


# ── 9. bounds that only matter where the deadline clamp cannot reach ────────
echo "9. poll-interval bounds, independently of the deadline"

# THE CAP ITSELF. Both earlier cap tests pass `--timeout 3`, so poll_sleep()'s deadline clamp
# alone satisfies them — deleting the 60s cap left the whole suite green. The cap is the only
# thing that matters when there IS no deadline: unbounded, `ICN_WAIT_POLL_INTERVAL=999999`
# slept for 11.5 days, so a condition that was met would be noticed 11.5 days late.
ICN_WAIT_POLL_INTERVAL=999999 "$WAIT" file "$TMP/never-appears" --timeout 0 --allow-unbounded \
  >/dev/null 2>&1 &
wp=$!
# Poll for the sleeper rather than sampling once after a fixed delay: under heavy machine load
# the child may not exist yet at 1.5s, and "no sleep child found" would be reported as a cap
# failure when the cap is fine. The ASSERTION is about the sleep DURATION, so waiting longer to
# observe it costs nothing; a genuinely absent sleeper still fails after the bound.
sleeper=""
for _ in $(seq 1 20); do
  sleep 0.5
  sleeper=$(pgrep -P "$wp" -x sleep 2>/dev/null | head -1)
  [ -n "$sleeper" ] && break
done
slept=""
[ -n "$sleeper" ] && slept=$(tr '\0' ' ' < "/proc/$sleeper/cmdline" 2>/dev/null | awk '{print $2}')
kill -TERM "$wp" 2>/dev/null; wait "$wp" 2>/dev/null
if [ -n "$slept" ] && awk -v s="$slept" 'BEGIN{exit !(s<=60)}'; then
  ok "an unbounded wait still caps the poll interval (slept ${slept}s, not 999999s)"
else
  bad "the poll interval is uncapped without a deadline" "child slept for '${slept:-<no sleep child found>}'"
fi

# THE LOWER BOUND, AS A PROPERTY. It used to be a list of spellings anchored at the first
# character (`0.0*|.0*`), so one extra leading zero walked past it and busy-spun.
for spelling in 00.001 000.0001 0000.00001 00.0; do
  # --timeout 8, sampled at 3s. Section 5's busy_ticks() uses the same 6/3 ratio for the same
  # reason: sampling at 2.5s against a 3s bound left 0.5s of headroom, so under machine load the
  # waiter had already exited and the (correct) liveness precondition reported "NOT RUNNING".
  # The assertion is about CPU burn while waiting, so a longer bound costs nothing.
  ICN_WAIT_POLL_INTERVAL="$spelling" "$WAIT" file "$TMP/never-appears" --timeout 8 >/dev/null 2>&1 &
  wp=$!
  sleep 3
  ticks=$(cpu_ticks_of "$wp")
  kill -TERM "$wp" 2>/dev/null; wait "$wp" 2>/dev/null
  assert_polite "$spelling" "$ticks"
done


# ── 10. zero-padded numbers must not mean something else ───────────────────
echo "10. leading zeros"

# Validators are character classes, so they accept leading zeros — and the value was then read
# three inconsistent ways: `[ x -gt y ]` DECIMAL, `$(( x ))` OCTAL, `[ a = b ]` STRING. That gap
# turned `--timeout 08` into a silently UNBOUNDED wait (the arithmetic errored, DEADLINE stayed
# 0, expired() never fired) and let `pid 0$$` past the self-wait refusal.

# 08 is invalid octal: it used to die in arithmetic and leave the wait unbounded.
start=$(date +%s)
timeout 20 "$WAIT" cmd --timeout 08 -- sleep 300 >/dev/null 2>&1
rc=$?; elapsed=$(( $(date +%s) - start ))
if [ "$rc" -eq 124 ] && [ "$elapsed" -le 12 ]; then
  ok "--timeout 08 is bounded (rc 124 after ${elapsed}s), not silently unbounded"
else
  bad "--timeout 08 did not bound the wait" "rc=$rc elapsed=${elapsed}s (expected rc=124 within ~8s)"
fi

# 010 is valid octal for 8 — it must mean TEN.
start=$(date +%s)
timeout 25 "$WAIT" file "$TMP/never-appears" --timeout 010 >/dev/null 2>&1
rc=$?; elapsed=$(( $(date +%s) - start ))
if [ "$rc" -eq 1 ] && [ "$elapsed" -ge 9 ] && [ "$elapsed" -le 14 ]; then
  ok "--timeout 010 waits TEN seconds, not octal eight (${elapsed}s)"
else
  bad "--timeout 010 was misread" "rc=$rc elapsed=${elapsed}s (expected rc=1 at ~10s)"
fi

# ...and a padded zero is still zero, so it still demands the explicit flag.
timeout 8 "$WAIT" file "$TMP/never-appears" --timeout 00 >/dev/null 2>&1
check "--timeout 00 still requires --allow-unbounded (exit 2)" 2 $?

# A padded pid is the SAME pid: string comparison let it through both guards.
timeout 10 bash -c "exec '$WAIT' pid 0\$\$ --timeout 4" >/dev/null 2>&1
check "a zero-padded self pid is still refused (exit 3)" 3 $?

timeout 10 bash -c "exec '$WAIT' pid \$\$ --timeout 4" >/dev/null 2>&1
check "  ...and the unpadded control is refused too" 3 $?

# A real, live, padded pid must still be WAITED on — the guard must not reject everything.
sleep 6 &
lp=$!
timeout 10 "$WAIT" pid "0$lp" --timeout 2 >/dev/null 2>&1
check "a zero-padded live pid is still waited on (exit 1 = timed out)" 1 $?
kill "$lp" 2>/dev/null; wait "$lp" 2>/dev/null


# ── 11. digits that are not ASCII digits ───────────────────────────────────
echo "11. locale-collating digit classes"

# `[0-9]` in a bash `case` is a LOCALE-COLLATING RANGE. Under a full UTF-8 locale it admits
# ARABIC-INDIC ٣, FULLWIDTH ３ and EXTENDED-ARABIC ۳. They passed validation, reached
# `$((10#...))`, errored, and left TIMEOUT empty: every downstream guard no-oped, DEADLINE
# stayed 0, expired() could never fire, and the wait was SILENTLY UNBOUNDED. Measured before the
# fix: `--timeout ٣` ran until an external timeout killed it at 12s, against a 3s control.
# The classes are spelled out as [0123456789] now, so there is no collation and no hole.
#
# THE LOCALE IS THE TEST, AND IT CANNOT BE ASSUMED. `[0-9]` only collates non-ASCII digits under
# a full UTF-8 locale; under C / C.UTF-8 / POSIX it rejects them anyway, so these cases pass
# whether the defect is present or not. Measured with the pre-fix `[0-9]` restored:
# en_US.UTF-8 -> 3 failures (caught), C.UTF-8 -> 0 failures (SURVIVES), C -> 0, unset -> 0.
# ubuntu-latest defaults to C.UTF-8 — a surviving row — so a section written against the ambient
# locale is inert exactly where it gates the merge.
#
# So the locale is DISCOVERED BEHAVIOURALLY and, failing that, BUILT. A locale NAME is not
# evidence: what this section needs is a locale in which `[0-9]` demonstrably matches U+0663, so
# every candidate is probed with a real bash before it is trusted. If none qualifies the section
# fails loudly rather than passing without power.
probe_collates() {  # $1 = LOCPATH ('' for the system one), $2 = locale name
  # Checks ALL THREE digits the cases below actually use, not just one. Collation tables are
  # per-locale; a locale that admits ٣ but not ３ would satisfy a one-digit probe and then
  # fail case 2 for a reason that has nothing to do with icn-wait. The assertion has to be
  # exactly the property the cases depend on.
  [ "$(LOCPATH="$1" LC_ALL="$2" bash -c '
        for d in ٣ ３ ۳; do
          case "$d" in [0-9]) ;; *) echo no; exit ;; esac
        done
        echo yes' 2>/dev/null)" = "yes" ]
}

COLLATING_LOCALE=""
COLLATING_LOCPATH=""

# (a) Prefer a locale the machine already has. C/POSIX are skipped by name only to avoid probing
#     the ones that definitionally cannot collate; everything else still has to prove it.
for cand in $(locale -a 2>/dev/null); do
  case "$cand" in C|C.*|POSIX) continue ;; esac
  if probe_collates "" "$cand"; then COLLATING_LOCALE="$cand"; break; fi
done

# (b) Otherwise BUILD one into the sandbox. `localedef` ships in libc-bin (Priority: required),
#     and the en_US/UTF-8 source files ship in `locales`, which is present on the ubuntu-latest
#     runner image — so this needs no root, no apt and no network. LOCPATH makes it visible to
#     the child shell without touching the system locale set.
if [ -z "$COLLATING_LOCALE" ] && command -v localedef >/dev/null 2>&1; then
  mkdir -p "$TMP/loc"
  # localedef's EXIT CODE is deliberately ignored: it warns (and can exit non-zero) about things
  # that do not stop the locale being usable. The behavioural probe is the authority here, as it
  # is for branch (a) — what matters is whether `[0-9]` collates, not what a tool reported.
  localedef -i en_US -f UTF-8 "$TMP/loc/en_US.UTF-8" >/dev/null 2>&1
  if probe_collates "$TMP/loc" "en_US.UTF-8"; then
    COLLATING_LOCALE="en_US.UTF-8"
    COLLATING_LOCPATH="$TMP/loc"
  fi
fi

if [ -z "$COLLATING_LOCALE" ]; then
  # Do NOT silently pass. Without a collating locale these cases cannot tell the fixed helper
  # from the defective one, and a green run would misreport that as proof.
  bad "no locale found or built in which [0-9] collates" \
      "section 11 cannot see the defect it exists for (locale -a: $(locale -a 2>/dev/null | tr '\n' ' '))"
else
  ok "verified [0-9] admits all three non-ASCII digits under ${COLLATING_LOCALE}${COLLATING_LOCPATH:+ (built in-sandbox)} — this section can see the defect"
fi

# Every case below runs under that verified locale, NOT the ambient one.
for digit in "$(printf '٣')" "$(printf '３')" "$(printf '۳')"; do
  LOCPATH="$COLLATING_LOCPATH" LC_ALL="${COLLATING_LOCALE:-C}" \
    timeout 12 "$WAIT" file "$TMP/never-appears" --timeout "$digit" >/dev/null 2>&1
  rc=$?
  if [ "$rc" -eq 2 ]; then
    ok "a non-ASCII digit --timeout is REJECTED (exit 2), not silently unbounded"
  else
    bad "a non-ASCII digit --timeout was accepted" "rc=$rc (expected 2)"
  fi
done

LOCPATH="$COLLATING_LOCPATH" LC_ALL="${COLLATING_LOCALE:-C}" \
  timeout 12 "$WAIT" pid "$(printf '٣')" --timeout 3 >/dev/null 2>&1
check "a non-ASCII digit pid is rejected (exit 2)" 2 $?

LOCPATH="$COLLATING_LOCPATH" LC_ALL="${COLLATING_LOCALE:-C}" \
  timeout 12 "$WAIT" file "$TMP/never-appears" --source-pid "$(printf '٣')" --timeout 3 >/dev/null 2>&1
check "a non-ASCII digit --source-pid is rejected (exit 2)" 2 $?

# ...and the ASCII control still behaves, so the guard is not simply refusing everything.
# It runs under the SAME verified locale as the cases above — the point is that ASCII digits keep
# working in the very locale where the collating range goes wrong, not merely in the ambient one.
start=$(date +%s)
LOCPATH="$COLLATING_LOCPATH" LC_ALL="${COLLATING_LOCALE:-C}" \
  timeout 12 "$WAIT" file "$TMP/never-appears" --timeout 3 >/dev/null 2>&1
rc=$?; elapsed=$(( $(date +%s) - start ))
if [ "$rc" -eq 1 ] && [ "$elapsed" -ge 2 ] && [ "$elapsed" -le 6 ]; then
  ok "ASCII --timeout 3 still waits and bounds itself (${elapsed}s)"
else
  bad "the ASCII control regressed" "rc=$rc elapsed=${elapsed}s"
fi

echo
printf 'passed: %d  failed: %d\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
