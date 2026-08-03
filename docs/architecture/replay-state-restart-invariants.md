# Replay state and restart invariants

**Status:** current · **Scope:** `icn-net` — `ReplayGuard`, `SignedEnvelope`, `SigningSequenceCounter`

This document states the durability invariants that make replay protection survive a restart
without rejecting traffic a legitimate sender is actually emitting. It is the design record for
issue #2514 and the security half of the #2504 restart/rejoin chain. The narrative forensic history
lives in [restart-rejoin-investigation-2504.md](restart-rejoin-investigation-2504.md); this file is
the invariant statement.

## 1. Two quantities, not one

Per peer, distinguish:

| | meaning |
|---|---|
| `A` | highest sequence the receiver **actually accepted** before a crash |
| `D` | highest sequence **durably recorded** before that crash |
| `floor` | acceptance floor installed on restart — all sequences `<= floor` rejected |

On restart the receiver can only restore what it durably has, so **`floor = D`**. In general
`D <= A`, and the interval `(D, A]` is the set of sequences that were accepted but whose record did
not survive.

## 2. The two-sided constraint

**Security** requires `floor >= A`. Otherwise a captured message with sequence in `(floor, A]` is
accepted a second time: the Bloom filter is empty after restart, so nothing else rejects it.

**Liveness** requires `floor <= A`. Per the sender invariant (§3) a healthy sender's next emission
is the smallest sequence it has never emitted, which is at most `A + 1`. Any `floor > A` therefore
sits in sequence values the sender has not emitted and will only reach by burning through them.

Therefore:

```
floor == A     — exactly. There is no slack in either direction.
```

**This is the whole of #2514.** The pre-fix code set `floor = D + 1000`. The `+1000` is precisely
the violation of the liveness half. A constant gap cannot be "tuned": any positive value rejects
legitimate traffic, and `0` is correct **only if `D == A`**.

So the real design question is not "how big should the gap be" but **"how do we guarantee
`D == A`"** — or, failing that, what else can cover `(D, A]`.

## 3. The sender invariant (#2510 / #2511)

`SigningSequenceCounter::next_sequence()` reserves a block of `RESERVATION_BLOCK = 1000` and
persists **and flushes** the new `reserved_end` *before* issuing any sequence from that block
(`signing_sequence.rs`). Formally: every emitted sequence `s` satisfies `s < reserved_end` at
emission, and `reserved_end` was durable before emission. A restarted incarnation resumes at the
durable `reserved_end`, hence above every sequence that could have escaped.

```
sender crash  ⇒  sequences may be SKIPPED, never REUSED (monotonic, non-reusing)
```

This is what licenses the liveness half of §2. It does **not** eliminate receiver persistence lag —
these are different crash-consistency problems and must not be conflated.

## 4. Wall clocks cannot close `(D, A]` — an ambiguity result

A tempting cheap fix is to reject envelopes "signed before the restart", using the signed timestamp.
**This does not work, and the reason is worth recording so it is not retried.**

`envelope.timestamp` comes from the **sender's** wall clock. Any restart instant the receiver
records comes from the **receiver's**. These are different clock domains, and ICN deliberately
tolerates skew between them (§5).

Consider a message with sequence in `(D, A]` and the receiver's observable inputs: authenticated
DID, signed sequence, signed timestamp, payload, and a fresh transport session (the old connection
died with the restart, so session freshness carries no information either).

- **Case OLD** — accepted before the crash, lost from durable state, replayed by an attacker after
  restart. If the sender's clock runs *ahead* by `δ`, its timestamp can be **greater** than the
  receiver's restart instant.
- **Case NEW** — the stable sender's next legitimate message, emitted after the restart. If the
  sender's clock runs *behind* by `δ`, its timestamp can be **less** than the receiver's restart
  instant.

For any skew the freshness check tolerates, these two cases produce **overlapping observables**. A
wall-clock timestamp therefore cannot distinguish OLD from NEW. Both failure directions are real and
are pinned by tests (`test_legitimate_traffic_accepted_despite_negative_sender_skew`,
`test_captured_replay_rejected_despite_positive_sender_skew`); the liveness failure appears at as
little as **one second** of skew.

The general form: **bounded clock difference is not the ability to totally order events across
machines.** `verify_age` only ever asks "is this timestamp inside a window around my current time",
which bounded difference does support. "Did this sender-side event happen before this receiver-side
event" is a strictly stronger question that ICN's time contract does not answer.

Consequently the receiver must either **durably know what it accepted**, or use a cryptographic
boundary that actually encodes the distinction. Envelope signatures cover
`sequence || timestamp || payload_type || payload` — no session, epoch, or nonce — so no such
boundary exists today without a wire-format and signature-input change.

## 5. Freshness: what it does and does not promise

`SignedEnvelope::verify_age(max_age_secs)` enforces:

- reject if `now - timestamp > max_age` (too old)
- reject if `timestamp > now + max_age` (too far in the future — the tolerance is symmetric)

`timestamp` is milliseconds since the Unix epoch and **is covered by the signature**, so a replaying
attacker cannot alter it.

**Ordering in production:** signature + age are verified **first**, and only then is `ReplayGuard`
consulted, so a stale envelope never reaches the replay guard.

```
verify signature + verify_age   →   ReplayGuard::check_replay_only
```

**Three separate constants, currently all 300/3600, none derived from each other:**

| quantity | where | value |
|---|---|---|
| freshness bound actually used in production | `handlers/signed.rs` — literal argument | 300 s |
| `ReplayGuard::max_clock_skew` | `actor/mod.rs` — constructor argument | 300 s |
| `ReplayGuard::max_peer_age_secs` | `actor/mod.rs` — constructor argument | 3600 s |

The first two coincide today but are independent literals; production calls `check_replay_only`,
which never consults `max_clock_skew`. Do not treat one as a proxy for the other. The invariant that
matters is stated in §7 and must be maintained deliberately if any of them changes.

## 6. The receiver invariant: durability before acceptance

> **Invariant.** The replay high-water is made durable **before** an acceptance is returned to the
> caller. Therefore `D == A` at every instant, and the floor restored on restart is exactly `A`.

Implementation (`check_replay_only`): on a max-sequence advance, `put` **and `flush`** the
high-water, and only then update in-memory state and return `Ok`. This mirrors what the outbound
side already does for its reservation watermark; the receiving side was the half that never got it.

**Fail closed.** If the high-water cannot be made durable the message is *not* accepted — accepting
it would recreate `(D, A]`. This is surfaced as `ReplayStateNotDurable`, which the signed-message
handler explicitly does **not** score as peer misbehaviour: it is a local storage fault, and banning
a peer for our own disk problem is exactly the false-positive class #2514 was about.

**Consequences:**

- No sequence-space gap. `floor = D = A`.
- Repeated restarts with no traffic leave the floor unchanged — no compounding.
- **No clock comparison anywhere in restart recovery.** Correctness does not depend on clock
  synchronisation between sender and receiver.

### 6.1 Measured cost

Two very different storage profiles, and only the second is deployment evidence.

| | dev VM, local SSD | `atlas-nfs` (the class backing every ICN data PVC) |
|---|---|---|
| samples | 500 | 500 |
| p50 | — | 4 984 µs |
| p95 | — | 6 305 µs |
| p99 | — | 18 882 µs |
| max | — | 26 333 µs |
| mean | 41 µs (`put` alone 5.7 µs) | 5 288 µs |
| sustainable | ~24 000 ops/s | **~189 ops/s** |

The rehearsal cluster's ICN PVCs are **NFS-backed**, not local disk, and an fsync there costs
roughly **130× more** than on the dev VM. The dev-VM figure is not representative and should not be
quoted. Against an observed federation rate of ~15 messages per *minute* per peer (~0.25/s), 189
ops/s is still roughly **750×** headroom — comfortable, but three orders of magnitude less
comfortable than local storage suggests, and the p99/max tail (19–26 ms) is not negligible under the
guard's global write lock.

**`Store::flush()` is whole-database.** `SledStore::flush` delegates to `sled::Db::flush`, which
flushes *all* pending writes across every tree, not just the replay key. Operationally:

- concurrent peers' replay writes amortise into a single sync rather than multiplying it;
- but a replay flush also forces unrelated pending writes (gossip, ledger, receipts) durable, so its
  cost scales with total outstanding write volume, not with the ~50-byte replay record. Under heavy
  unrelated write load the 5 ms p50 can grow.

**Group commit is not justified by these measurements** (750× headroom) and is deliberately not
implemented. It remains the escape hatch if inbound rates approach the ceiling or if the storage
class regresses, at the cost of holding delivery until the batch commits. Correctness is not
negotiable in that trade: the invariant remains *durable replay fact before semantic acceptance*.

## 7. Forgetting replay state

`ReplayGuard::cleanup()` runs periodically and, for any peer with no **accepted** traffic in
`max_peer_age_secs`, removes the in-memory window **and deletes the persisted
`replay_max_seq:<did>` key**.

Two properties matter:

1. **A window that rejects everything never refreshes its own liveness.** `last_update` is written
   only on the accepted path and at window creation; the floor check returns before reaching it.
   This is why, in the #2514 incident, 566 consecutive rejections did not extend the window's life
   by a millisecond and it aged out on a fixed 3600 s timer.
2. **Deleting the state is safe only because of freshness** — and the margin is *not* one freshness
   interval. See the horizon derivation below.

### 7.1 The envelope validity horizon

How long can an envelope the receiver already accepted still pass `verify_age`, counting from the
moment the receiver stops tracking it?

`verify_age` admits timestamp `T` while the receiver's wall clock lies in
`[T - max_age, T + max_age]`. An envelope accepted at receiver time `X` passed freshness *then*, so

```
T <= X + future_skew_tolerance
```

and it remains valid until `now > T + max_past_age`. Worst case `T = X + future_skew_tolerance`:

```
horizon = future_skew_tolerance + max_past_age
```

In the current envelope implementation both tolerances are the same quantity — `verify_age`
compares against `max_age_ms` in *both* directions — so the horizon is `2 × max_age`, i.e.
**600 s** in production, not 300 s.

**A one-interval margin is wrong, and wrong in the worst possible way:** an envelope stamped at the
positive-skew limit reaches age ≈ 0 — its *freshest* point — exactly when a single-`max_age`
quarantine would expire. Pinned by `test_corrupt_state_quarantine_outlasts_maximum_future_skew`;
reducing the horizon to one interval makes that test fail.

> **Invariant.** `max_peer_age_secs > future_skew_tolerance + max_past_age`.

Today that is `3600 > 600`, a 6× margin (not the 12× a single-interval reading suggests). The three
constants are independent literals (§5), so this relation must be maintained deliberately. It is
asserted by `test_peer_age_exceeds_envelope_validity_horizon_in_production_config`.

### 7.2 Clock-security contract

The horizon assumes the receiver's wall clock does not jump **backwards** after a restart. If it
does, `verify_age` itself is compromised — an expired envelope becomes "fresh" again — and no
amount of replay-state bookkeeping can repair that, because freshness is the boundary being
subverted. For peers with intact durable state the sequence floor still rejects everything at or
below `A`, so the exposure is confined to the corrupt-state case (§8), which has no floor.

This is a property of ICN's time contract, not something `ReplayGuard` can solve. Recorded here so
it is not mistaken for a replay-guard defect: **bounded monotonic wall-clock progress on each node
is a prerequisite for signed-envelope freshness**, and therefore for replay-state expiry.

## 8. Storage rollback, corruption, and other degenerate states

- **Corrupt / undeserialisable entry.** The key's existence proves state existed, but its high-water
  is unreadable, so no sequence can be shown to be new. The peer is **quarantined** — all of its
  traffic rejected — for the full envelope validity horizon of §7.1 (`future_skew + max_age`, 600 s
  in production), after which nothing captured before the restart can still pass freshness and
  normal service resumes. Release happens *strictly after* the horizon, not at it: at the horizon
  exactly, a worst-case envelope's age equals `max_age` and `age > max_age` is false, so it is still
  valid for that instant. This is a *receiver-local elapsed duration on the monotonic clock*, not a
  cross-machine timestamp comparison, so §4 does not apply to it. Failing open here would hand an
  attacker a replay window; failing closed forever would turn one corrupt key into a permanent
  peer-level outage.
- **Rollback to an older snapshot.** Durable state moving backwards by means other than a crash is
  **outside** the crash-consistency guarantee. With `D` rolled back below `A`, sequences in `(D, A]`
  become acceptable again and only envelope freshness bounds the exposure. Pinned explicitly by
  `test_storage_rollback_is_outside_the_crash_guarantee` so the boundary is documented rather than
  discovered.
- **Failed `put` or `flush`.** Fail closed, per §6.

## 9. Migration: these invariants describe state created under the corrected protocol

Every invariant above governs replay state **created by this code**. None of them repair state that
was written, or is remembered by a peer, under earlier sequence semantics. That is a separate
invariant and it is not yet satisfied.

The distinction is sharp enough to be worth stating as a rule:

> **Correct steady-state protocol semantics and safe migration from previously persisted or
> distributed semantics are separate invariants.** Establishing the first does not establish the
> second, and a faithful restore of a legacy value is still wrong.

Two known instances, both live-observed:

- **Sender side (#2517).** `SigningSequenceCounter` initialises its durable counter without any
  bridge from the ephemeral counter it replaced, so a first upgrade can resume *below* the
  high-water its peers already recorded. Observed: a sender resumed at `10001` while a long-lived
  peer still remembered `15915` for it, and rejected every legitimate sequence in between.
- **Receiver side (#2514).** §6 restores the durable high-water *exactly*. If that value was
  written by the pre-fix loader — which persisted `stored + 1000` on every load — the exactness is
  faithful to a number that was already inflated. Nothing in §6 detects this.

Both currently "heal" the same way: the peer window ages out under §7 after `max_peer_age_secs` of
no **accepted** traffic, because rejections never refresh liveness. That is recovery by timeout,
which is precisely what the restart/rejoin work exists to eliminate, and it costs thousands of false
misbehaviour and ban events on the way.

A bridge is **not** designed here. The shape of the problem — a persisted record whose meaning
changed without its format changing — suggests versioning the records so a reader can tell
old-regime data from new and treat old-regime entries as untrusted rather than authoritative, which
would cover both instances with one mechanism. That is for #2517 to decide, not this document.

## 10. Connection lifetime vs replay lifetime

Replay state is keyed by peer **DID**, not by connection. Reconnects, stale-connection replacement
(#2505), and transport churn do not reset it. Conversely, the local identity is never admitted as a
remote peer (#2506 / #2513), so a node cannot create replay state for itself.

## Related

- #2504 restart/rejoin umbrella · #2505 stale connection · #2510/#2511 durable sender sequence ·
  #2506/#2513 self-peer · #2514 receiver restart floor
- [restart-rejoin-investigation-2504.md](restart-rejoin-investigation-2504.md) — forensic narrative
- [../security/production-hardening.md](../security/production-hardening.md) — three-layer model
