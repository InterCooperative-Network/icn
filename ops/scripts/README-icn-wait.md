# `icn-wait` — waiting that can actually finish

## The two defects this replaces

Both were recovered live from `icn-dev`'s process table during the `#2644` lifecycle
investigation. Between them they pinned a merged lane's **114 GB** of Cargo build output for up
to 2.8 days.

### 1. The self-matching `pgrep`

```bash
until ! pgrep -f "scratchpad/mutate.py"; do sleep 20; done
```

`pgrep -f` matches **full command lines**. The waiting shell was started as
`zsh -c '... until ! pgrep -f "scratchpad/mutate.py" ...'`, so its own command line *contains
the pattern*. It matches itself. The predicate can never become false.

Verified on the live process: `pgrep -f "scratchpad/mutate.py"` returned exactly one PID — the
waiter.

### 2. The unbounded sentinel wait

```bash
until grep -q "^EXIT=" /tmp/.../full-test-3.log 2>/dev/null; do sleep 20; done
```

**This one is not logically impossible**, and it matters not to overstate it: another process
may legitimately create or update that file later, in which case the loop finishes normally.

Its defect is **indistinguishability**. With stderr swallowed, no bound, and no producer
identity, the loop cannot tell apart four situations:

| Situation | Waiting is |
|---|---|
| the producer is still working | correct |
| the producer died | futile |
| the scratch directory was deleted | futile |
| the sentinel will never arrive | futile |

Three of the four are terminal, and the loop treats all four identically — `2>/dev/null`
collapses *"cannot read this file"* into *"not ready yet"*. So it can spin indefinitely while
appearing active, and can never report why. On `icn-dev` it was the third case: the scratchpad
had been cleaned up and `full-test-3.log` no longer existed.

The fix is not to stop waiting on files — it is to supply a **bound** and **producer evidence**,
which is exactly what `icn-wait file --source-pid --timeout` does.

### Why they were hard to spot

Both spawn a fresh child every few seconds. By every naive metric — recent process start, live
parent, constant activity — they look maximally **alive**. Neither was making progress: the
first could never finish, and the second was waiting on something that was never coming. This
is the distinction the agent runtime draws as *activity ≠ progress*.

Neither pattern appears anywhere in this repository. They are ad-hoc inventions, which is why
the fix is a supported primitive rather than a documentation change.

## Usage

Reach for the exact forms first. If you started the process, you have its PID — use it.

```bash
# BEST: we launch it, so we wait on our own child. No pattern, no ambiguity.
icn-wait cmd --timeout 3600 -- cargo test --workspace

# Exact wait on a PID you were given.
icn-wait pid 12345 --timeout 600

# Sentinel file, with the producer declared so a dead producer fails fast.
icn-wait file /tmp/run.log --pattern '^EXIT=' --source-pid 12345 --timeout 600

# LAST RESORT: pattern matching. Excludes the observer's whole process tree.
icn-wait match 'some-unique-marker' --timeout 600
```


## Guarantees

| Property | How |
|---|---|
| Bounded by default | `--timeout` defaults to 30 min; `--timeout 0` requires `--allow-unbounded`; option values are required and an oversized timeout is refused |
| Cannot wait on itself | this process, its ancestors and its descendants are excluded — and the exclusion is **asserted** before the loop, not assumed |
| Impossible conditions fail fast | missing sentinel directory, directory deleted mid-wait, or `--source-pid` already exited → exit 3 |
| Timeouts are observable | prints what it waited for and for how long |
| Exit codes are meaningful | `0` met · `1` timed out · `2` usage error · `3` condition unreachable |

## Exit codes

```
0  condition met
1  timed out              (bounded wait expired)          — pid / file / match
2  usage error            (bad flags; unbounded without --allow-unbounded)
3  condition unreachable  (can never become true — fail fast, do not spin)

cmd is different, on purpose:
  <n>  the command's own exit status   (0 means the command succeeded)
  124  the wait timed out              (the convention timeout(1) uses)
  143  terminated by a signal
```

`cmd` propagates the command's status because "did `cargo test` pass" is what a caller needs.
Reporting a timeout as 1 made that indistinguishable from a failing test suite, so timeouts use
124 instead.

## Enforcement

- There is deliberately **no** static Bash guard blocking these shapes. One was written and
  removed: it was rewritten four times across four review rounds and broken every time, and its
  final failure was the *natural* phrasing — `while [ "$(pgrep -f X | wc -l)" -gt 0 ]` — while
  simultaneously refusing the canonical `ps aux | grep -v grep` idiom and failing open (taking
  51s against a 5s hook budget) on large inputs. Statically analysing arbitrary shell with
  regexes does not converge. The work is preserved on `ops/agent-wait-guard`.
  What prevents the defect here is the positive path: this tool, the startup context that names
  it, and this document.
- `scripts/test-icn-wait.sh` pins every recovered pattern, each with a **negative control**
  that reproduces the original loop and asserts it still hangs.

See `docs/architecture/AGENT_RUNTIME.md` for the lane lifecycle this feeds.
