# Inbound Handshake Cancellation (#2521) — Causal Record

**Status:** fixed on `main`. This is the summary; the issue thread remains authoritative.

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
`Incoming` arrives `t` ms into the window, the handshake gets `100 - t` ms. For a peer
dialling at an arbitrary moment, the effective handshake deadline is therefore
**uniformly distributed over (0, 100 ms]**, not a fixed 100 ms — so the failure rate is
roughly `H/100` for a handshake of duration `H`, and any load that lengthens `H` raises it.

That predicts ~45–55% for the handshake times seen on CI hardware, which is what was
measured (see below).

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
  on a peer that may never finish.

## Evidence

### Deterministic reproducer

`crates/icn-net/tests/accept_handshake_cancellation.rs` pins the boundary by construction
rather than by racing the clock. `tokio::time::timeout` polls its inner future *before*
checking the deadline, so a `Duration::ZERO` budget grants exactly one poll — and one poll
separates the two phases cleanly:

- `Endpoint::accept()` pops an already-queued `Incoming` on its first poll, so the
  cancel-safe phase always survives a zero budget;
- `Connecting::poll` reads a oneshot the freshly-spawned connection driver cannot yet have
  signalled, so the handshake phase never survives one.

On pristine `main` the reproducer was **20/20 deterministic**.

Two earlier drafts of this test produced false confidence and are recorded so they are not
repeated:

- *"1 ms is shorter than any handshake"* — it is not. A warm loopback handshake can
  complete in well under a millisecond, and the test then passes for the wrong reason.
- *Asserting the client's `connect()` failed* — flaky, because the server can complete the
  handshake, let the client's `connect()` resolve `Ok`, and only then discard the
  connection. This is not a test artifact: it is exactly the production symptom, where the
  dialling peer logs a successful certificate exchange and the receiver shows no trace of
  the connection at all. All assertions are made on the server side.

### Real-QUIC regression, same machine, same protocol

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
