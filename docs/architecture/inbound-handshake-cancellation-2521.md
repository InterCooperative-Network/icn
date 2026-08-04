# Inbound Handshake Cancellation (#2521) — Causal Record

**Status:** fix under validation on the `fix/2521-quic-accept-handshake-cancellation`
branch. Not merged. This is the summary; the issue thread remains authoritative.

## The invariant

> A shutdown polling deadline may cancel the wait for new inbound work. It must not
> silently become a maximum lifetime for a QUIC/TLS handshake that has already arrived.
> Once an `Incoming` is owned, its handshake lifecycle must be handled explicitly, with
> bounded concurrency and shutdown semantics, rather than by accidental future
> cancellation.

## What was wrong

`NetworkActor::handle_incoming_connections` polled for shutdown by wrapping the accept
step in a 100ms timeout:

```rust
let guard = session_manager.read().await;
tokio::time::timeout(Duration::from_millis(100), guard.accept()).await.ok()
```

`SessionManager::accept()` fuses two operations with opposite cancellation properties:

| phase | call | cancel-safe? | why |
|---|---|---|---|
| wait for new work | `Endpoint::accept()` | **yes** | a pending `Incoming` stays queued on the endpoint; `Accept<'_>` only parks on a `Notify` |
| complete the handshake | `incoming.await` | **no** | `Incoming::into_future` consumes the `Incoming` into a `Connecting`; dropping it drops the last `ConnectionRef`, and quinn calls `implicit_close` |

So the 100ms deadline was a hidden maximum lifetime for every inbound QUIC/TLS handshake.

### Why it presented as a ~50% flake rather than a clean threshold

The budget is *shared between both phases and re-armed each loop iteration*. If the
`Incoming` arrives `t` ms into the window, the handshake gets only `100 - t` ms. The
effective handshake deadline is therefore not a fixed 100 ms but a remainder that varies
from nearly zero up to 100 ms depending on where in the polling cycle the connection
arrives.

Under arrivals that are not synchronised to the polling cycle — which is the normal case,
since peers dial independently of the receiver's loop — this produces probabilistic rather
than threshold failure, at a rate that rises as handshake latency grows and as anything
(CPU load, scheduler contention, slower hardware, WAN latency) lengthens it.

The precise arrival-phase distribution was **not** measured, so no exact failure-rate
formula is claimed here. What was measured is the end-to-end rate on this machine
(37.5%, see below) and the fact that changing only this timeout removes the failure mode
entirely.

### Why the polling existed at all

It was load-bearing, not incidental. The loop held `session_manager.read()` *across* the
accept await, and `NetworkActor::run` shuts down via
`self.session_manager.write().await.stop()`. An unbounded accept would have pinned the
read guard forever and deadlocked shutdown. The timeout bounded the guard's hold time.

This is why "just remove the timeout" is not the fix: it trades a handshake bug for a
shutdown hang.

## The fix

`SessionManager::endpoint_handle()` hands out a cheap clone of the `quinn::Endpoint`
(internally reference-counted), so the accept loop can wait **without holding any
`SessionManager` lock**. With the lock no longer pinned, polling is unnecessary and the
loop waits on `tokio::select!` against the shutdown receiver instead:

1. reserve a handshake slot (bounded concurrency, cancel-safe);
2. clone the endpoint handle, releasing the manager lock immediately;
3. `select!` shutdown against `endpoint.accept()` — the only cancellable phase;
4. move the owned `Incoming` into its own task and drive `incoming.await` there, outside
   every cancellation scope in the loop.

`SessionManager::accept()` is retained for tests that spawn it and never cancel it, and now
carries a `# Cancel safety` warning documenting that it must never be wrapped in a timeout.

### Properties this changes

- **Shutdown got faster, not slower.** It is now signal-driven rather than polled, so the
  worst-case shutdown latency for the accept loop drops from ~100ms to immediate.
- **Head-of-line blocking removed.** The old loop completed one handshake before accepting
  another. A slow peer now cannot stall acceptance, because handshakes run in their own
  tasks.
- **Concurrency is bounded.** `MAX_CONCURRENT_INBOUND_HANDSHAKES = 64` slots are reserved
  *before* an `Incoming` is taken, so bursts back up in quinn's own accept queue — where
  they are subject to endpoint limits and refused cleanly — rather than accumulating
  unbounded tasks. The slot is released when the handshake completes, not when the
  connection closes, so it bounds handshakes rather than connection lifetimes.
- **In-flight handshakes at shutdown are deliberately detached.** `SessionManager::stop`
  closes the endpoint, which fails them promptly; the loop does not block shutdown waiting
  on a peer that may never finish. See "Lifecycle and shutdown" below for which parts of
  this are tested and which are argued.

## Evidence

Four distinct kinds of evidence support this fix. They are not interchangeable, and
conflating them overstates the case, so they are labelled explicitly.

### 1. Regression property (fail-before / pass-after)

`a_slow_but_legitimate_peer_is_still_admitted` is the only test whose **verdict changes**
across the fix. It runs the real production path — a `NetworkActor`, its real accept loop,
a real Hello — and asserts that a legitimate peer whose handshake takes 300 ms is still
admitted, with server-side peer installation as the oracle.

The 300 ms stall is introduced entirely at the test layer, by a client-side
`ServerCertVerifier` that sleeps. That step gates the client's Finished flight, which gates
the server's handshake completion, so it lengthens the server-side handshake without any
production seam and without weakening the server's TLS semantics.

| tree | result |
|---|---|
| pristine `connection.rs` | **0 pass / 6 fail** — fails on its own assertion, naming #2521 |
| fixed `connection.rs` | **6 pass / 0 fail** |

Same test source, unchanged between runs; only `connection.rs` differs. (`session.rs` is
additive and the old loop never calls it, so it was held constant.)

### 2. Deterministic defect witness (passes on *both* trees)

`a_fused_accept_cancelled_after_one_poll_destroys_the_handshake` and its control assert a
*negative property of the fused API* — that `SessionManager::accept()` is cancel-unsafe.
That is true before and after the fix, so this test does **not** change verdict and is not
a RED in the test-driven sense. It is a witness: it demonstrates the mechanism
deterministically and stands as the standing reason the accept loop may not call `accept()`
inside a cancellation scope.

It pins the boundary by construction rather than by racing the clock.
`tokio::time::timeout` polls its inner future *before* checking the deadline, so a
`Duration::ZERO` budget grants exactly one poll — and one poll separates the phases:

- `Endpoint::accept()` pops an already-queued `Incoming` on its first poll, so the
  cancel-safe phase always survives a zero budget;
- `Connecting::poll` reads a oneshot the freshly-spawned connection driver cannot yet have
  signalled, so the handshake phase never survives one.

Measured 20/20 on pristine `main`. That figure is a determinism measurement of the witness,
**not** a RED.

Two earlier drafts of this test produced false confidence and are recorded so they are not
repeated:

- *"1 ms is shorter than any handshake"* — it is not. A warm loopback handshake can
  complete in well under a millisecond, and the test then passes for the wrong reason.
- *Asserting the client's `connect()` failed* — flaky, because the server can complete the
  handshake, let the client's `connect()` resolve `Ok`, and only then discard the
  connection. This is not a test artifact: it is exactly the production symptom, where the
  dialling peer logs a successful certificate exchange and the receiver shows no trace of
  the connection at all. All assertions are made on the server side.

### 3. Mutation kills

Run with a snapshot-based harness that verifies the input tree contains the #2521 fix
before starting, restores only the file each mutation touched, and asserts on exit that the
tree it hands back is byte-identical (sha256) to the tree it was handed.

| id | mutation | killed by |
|---|---|---|
| M1 | skip the current-cert hash comparison | 3 tests incl. `hello_replayed_onto_a_different_current_cert_is_rejected` |
| M2 | fail open when the peer certificate is absent | `hello_without_any_peer_certificate_is_rejected` |
| M3 | verify against a stored cert, not this connection's | `forged_hello_does_not_corrupt_established_peer_state`, `hello_with_matching_current_cert_is_accepted` |
| M4 | omit `message.from == binding_info.did` | `hello_claiming_another_did_with_own_valid_binding_is_rejected` |
| M5 | put the handshake back inside a cancellation scope | `forged_hello_does_not_corrupt_established_peer_state`, `hello_with_matching_current_cert_is_accepted` |

**5/5 killed, 0 survivors, 0 no-ops**, integrity assertion PASS.

Two further mutations check that the new lifecycle tests are not vacuous:

| id | mutation | outcome |
|---|---|---|
| M6 | leak the handshake slot instead of releasing it | KILLED by `more_peers_than_the_handshake_cap_can_all_connect` |
| M7 | hold the read guard across the accept, keeping the shutdown `select!` | **SURVIVED — correctly.** The `select!` wakes the loop and drops the guard, so holding it is not sufficient to deadlock. Informative, not a coverage gap. |
| M7b | the actual naive fix: uninterruptible accept *and* held guard | KILLED by `shutdown_closes_established_connections_promptly` |

M7b is the important one: it is the shutdown deadlock a careless "just delete the timeout"
fix would introduce, and it is caught.

### 4. Bounded A/B on the real-QUIC production path

`cargo test -p icn-net --test hello_current_cert_binding -- --test-threads=1`, 40 serial
runs each (sample size fixed before running):

| tree | result | pass-run elapsed | fail-run elapsed |
|---|---|---|---|
| pristine `8f0159db` | 25 pass / **15 fail** (37.5% failure) | 1.95–2.11 s | 21.93–21.96 s |
| `8f0159db` + this fix | **40 pass / 0 fail** | 1.95–1.98 s | — none |

The distribution is sharply bimodal — ~2 s or ~22 s, nothing between — and the fix removes
the 22 s mode entirely. All 15 baseline failures were the same test,
`hello_with_matching_current_cert_is_accepted`, and all died the same way: a
`wait_for_peer(..., 20s)` timeout, because the connection was destroyed before any Hello
could be exchanged.

CI runs exactly this way — `cargo test --workspace --test '*' -- --test-threads=1`
(`.github/workflows/ci.yml:441`, "serial to avoid port conflicts") — which is why the gate
behaved as a coin flip for any PR touching `icn-net`.

### Lifecycle and shutdown

The fix spawns each owned `Incoming` into a detached task and bounds concurrent handshakes
with a 64-slot semaphore. Those properties are asserted by test and mutation rather than by
comment:

| property | how it is established |
|---|---|
| the wait for new work is cancel-safe | quinn source: `Accept<'_>` parks on a `Notify` and pops from the endpoint's queue, losing nothing on drop. Exercised by `cancelling_the_accept_wait_does_not_destroy_an_inbound_handshake`, which abandons the wait repeatedly on a one-poll budget. |
| an in-flight handshake is not bounded by the poll interval | `a_slow_but_legitimate_peer_is_still_admitted` (300 ms handshake vs the old 100 ms budget), RED on pristine, GREEN on the fix. |
| a completed handshake still reaches the long-lived connection handler | the same test's oracle is peer installation, which only happens inside `handle_connection`. |
| the handshake slot is released on every path | `more_peers_than_the_handshake_cap_can_all_connect` drives 80 peers past the cap of 64; M6 (leak the permit) kills it. |
| shutdown reaches the endpoint and closes it | `shutdown_closes_established_connections_promptly`; M7b (uninterruptible accept holding the read guard) kills it. |
| no shutdown deadlock is introduced | same test plus M7b — this is the failure mode a naive fix would create. |

One property is **reasoned, not directly tested**: that no in-flight handshake task leaks
indefinitely. The argument is that `SessionManager::stop` closes the endpoint, which fails
any outstanding `Connecting`, so each task terminates on its own next poll; and that the
number of such tasks is bounded by the semaphore in the first place. There is no task-count
assertion backing this, and it is recorded here as an argument rather than a proof. Note
also that detached connection-handler tasks are pre-existing behaviour — the fix adds a
bounded handshake stage in front of them, not a new class of unbounded task.

## A note on the validation machinery

The first version of the mutation harness restored every mutation target with
`git checkout -- <file>`. One of those targets, `connection.rs`, held the #2521 fix, which
was still uncommitted at the time. The harness therefore silently replaced the
implementation under test with pristine `HEAD` before the first mutation ran, and M1–M4
were evaluated against the wrong tree. M5 surfaced this as `NO-OP (pattern not found)` —
the expected `incoming.await` was gone because the harness itself had removed it — rather
than as a false kill.

The source was recovered and verified byte-identical (sha256) to the stash object that had
produced the A/B result, so no design drift was introduced. The harness was then rewritten
to snapshot the exact input contents, restore from those snapshots rather than from Git,
refuse to start unless the fix is present, and assert on exit that the tree it returns is
byte-identical to the tree it received. All mutations were re-run against the real fixed
implementation.

The lesson is worth keeping: **validation machinery is part of the proof.** A mutation
harness that can silently substitute `HEAD` for the implementation under test invalidates
its own results even when the individual tests fail in the desired direction. And a NO-OP
is never a kill.

## Hypotheses that were disproved

Recorded so they are not re-litigated:

- **"The 20 s test timeout is too short."** No. The failure occurs *before* the Hello
  security property is exercised at all — no server-side client-cert acceptance, no
  "Accepted connection", no connection handler. The connection is already gone; extending
  the deadline only makes the test wait longer for something that will never arrive.
- **"A 30-second direct-dial timeout."** There is no dial deadline in the production path;
  `SessionManager::connect` dials un-timed. The three `from_secs(30)` values in `icn-net`
  are the TURN refresh interval, the QUIC keep-alive interval, and the mDNS scan interval —
  none is a dial deadline.
- **"mDNS ghost peers / cross-test state accumulation."** This was the original framing of
  #2521, and it is an *amplifier*, not the cause. Accumulated `ServiceDaemon` threads add
  scheduling contention, which lengthens the handshake and so raises the probability of
  crossing the shared 100ms budget — which is why the flake was serial-mode specific. But
  the fix touches no mDNS code and takes the suite to 40/40, and the earlier diagnostic
  that changed *only* the accept timeout from 100ms to 5000ms already took it to 20/20.
  mDNS was never load-bearing.
- **"A #2517/#2519 regression."** No: the defect reproduces on pristine `main` at
  `8f0159db`, and predates both.
- **"Actor mailbox blockage / incomplete actor teardown."** Not implicated: the accept loop
  never reaches the mailbox, and the reproducer needs no actor at all.

## Production impact

Conservatively: this is a **reliability/availability** defect, not a security one.

Inbound federation connection attempts could be silently dropped whenever the QUIC/TLS
handshake outlived the remaining share of the accept loop's 100ms budget — more likely
under CPU load, scheduler contention, slower hardware, WAN latency, or slow certificate
verification. Because the budget is a random share of 100ms rather than a fixed 100ms, some
proportion of inbound connections was being lost even on fast paths. The expected
presentation is intermittent connection/rejoin failure, which would amplify restart and
rejoin instability.

It is **not** an authentication bypass, a confidentiality compromise, or a permanent
outage, and none of those is claimed. The #2520 Hello identity-binding invariant is
unaffected: the failure happens strictly below it, and all four of its mutations remain
killed under this change.

## Follow-ups deliberately not bundled here

- mDNS service advertisement can outlive `Discovery::stop()` until TTL expiry unless
  explicitly unregistered.
- Test teardown signals `NetworkActor` shutdown without awaiting a complete unwind.

Both are real, both are separable, and neither is load-bearing for this defect — the suite
reaches 40/40 without touching either.
