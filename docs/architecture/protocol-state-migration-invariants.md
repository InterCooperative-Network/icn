# Protocol state migration invariants

**Status:** current · **Scope:** `icn-net` — persisted replay state, durable signing sequence

Persisted protocol-security state has a schema *and* a semantics. This document states the
invariants for changing the second one. It is the design record for issue #2517 and the migration
half of the #2504 restart/rejoin chain; the steady-state invariants are in
[replay-state-restart-invariants.md](replay-state-restart-invariants.md) and the narrative history
is in [restart-rejoin-investigation-2504.md](restart-rejoin-investigation-2504.md).

## 1. The governing invariant

> **Correct steady-state protocol semantics and safe migration from previously persisted or
> distributed semantics are separate invariants.** Establishing the first does not establish the
> second, and a faithful restore of a legacy value is still wrong.

> **A persisted replay high-water is interpretable only together with BOTH the receiver replay
> semantics and the authenticated sender sequence regime that produced it.** Neither alone is
> enough. A current receiver that records a number learned from an unproven sender, and labels it
> current because *it* is current, has laundered a legacy value into current state — and nothing
> downstream can tell the difference afterwards.

Formally the meaning of a stored `max_seq` is a pair, not a scalar:

```text
meaning(max_seq) = ReceiverReplayRegime  ×  SenderSequenceRegime
```

The first version of the #2517 fix recorded only the left-hand factor. §8 is the record of why that
was insufficient and what closes it.

#2514 established that a receiver restores exactly the highest sequence it accepted. That is
correct and it is not sufficient. `15915` restored faithfully is still `15915` written by a regime
that no longer exists.

The distinction that matters:

| | question | detected by |
|---|---|---|
| **Schema version** | can this binary *parse* the bytes? | deserialisation failing |
| **Semantic version** | can this binary *believe* the parsed value? | nothing, unless recorded |

A value can parse perfectly and mean nothing. Every replay-state entry written before #2517 is in
exactly that position: `MaxSeqEntry` never changed shape, so no parse ever failed, while the
meaning of `max_seq` changed twice — once at #2510 (the sender's counter became durable, so
pre-#2510 high-waters belong to an incarnation that no longer exists) and once at #2514 (restart
stopped inflating the stored value by a fixed gap).

> **Invariant.** Persisted state must identify the semantic regime under which it was produced
> whenever a future version cannot safely interpret old values identically.

## 1a. Known old semantics and unknown semantics are not the same problem

Recording the regime is only half of it. What a reader does with a regime it is not currently
implementing splits sharply in two, and collapsing them is the mistake this section exists to
prevent.

> **Invariant.** Automatic migration is permitted only when the implementation contains an explicit
> migration from the identified source regime.

> **Invariant.** Unknown semantic versions fail closed, with no deadline. Time passing cannot make
> unknown state semantics safe to reinterpret.

The asymmetry is not squeamishness, it is what the bounded hold is actually built on. Retiring a
legacy high-water after `future_skew + max_past_age` is safe **because we know what regime produced
it** and can therefore bound how long anything produced under it stays dangerous. That argument is
unavailable for a regime we have no migration for: a newer binary may have changed sequence
interpretation, replay-window semantics, epoch handling, what persistence means, or freshness
assumptions themselves. Waiting out a horizon derived from *our* assumptions proves nothing about
state written under *different* ones.

Concretely, the receiver enumerates rather than testing `!= current`:

```text
version == LEGACY (0)      -> known regime, explicit migration exists
                           -> bounded fail-closed hold (validity horizon)
                           -> retire legacy value, rebuild under current semantics

version == CURRENT (1)     -> exact #2514 restore

anything else              -> no migration exists; meaning cannot be established
                           -> fail closed INDEFINITELY, no deadline to reach
                           -> never auto-graduates into current semantics
                           -> never overwritten or restamped by refused traffic
                           -> resolved by upgrading the binary or repairing the state
```

A regime introduced later gets its own arm **and its own migration**. Until it has one it falls to
the catch-all and fails closed, which is the safe direction in which to forget something.

In the implementation the indefinite case carries no expiry *field* — `PeerHold::UnsupportedVersion`
has no `until` — so the property is structural rather than a matter of picking a large enough
constant. There is nothing that can expire, and a reordering of the check cannot accidentally give
it a deadline.

It is reported as a distinct typed error, `ReplayStateUnsupportedVersion`, classified with
`ReplayStateNotDurable`, `ReplayStateUnreadable`, and `ReplayStateLegacy` as a **local** condition:
never a replay attack, never severity 1.0, never a ban, never reputation damage. It is logged at
`error` rather than `warn` because it is the one local fault that does not resolve itself — an
operator has to act, and a countdown would be a lie.

**Receiver and sender agree here.** The sender already refuses to open a watermark whose regime it
does not implement, for the same reason: a sender that guesses reissues sequences its peers have
already accepted. Neither side waits out an unknown.

## 2. Why this is one problem, not two

#2517 presents as two defects — an upgraded *sender* resuming below what peers remember, and an
upgraded *receiver* restoring an obsolete floor. They are one defect seen from two ends.

The sender-side symptom exists **only because a receiver treats a legacy number as a
current-semantic high-water.** A sender emitting sequence 12901 is doing nothing wrong; the fault
is entirely in the receiver's willingness to compare it against 15915. Correct the receiver's
interpretation and the sender needs no migration: no jump, no handshake, no peer polling, no epoch.

This is why the fix is receiver-side. It also disposes of the offline-peer problem: legacy
detection reads the receiver's *own* persisted state, so a peer that was offline for the entire
migration still detects and migrates its own state whenever it next boots. There is no
online-during-upgrade requirement and no window to miss.

## 3. Rejected alternatives

Recorded so they are not re-litigated.

**Recover the sender's legacy high-water locally.** Impossible. The pre-#2510 counter was
`Arc::new(AtomicU64::new(0))` in `init_send_callback.rs`, process-local and never persisted. No
durable artifact exists. The apparent exception is an accident of #2506: while nodes admitted their
own DID as a remote peer they recorded `replay_max_seq:<own-did>` about themselves. That artifact
is frozen now that #2506 is fixed, absent on clean nodes, and is a bug's side effect rather than a
record anything undertook to keep.

**Ask peers for the highest sequence they remember.** A malicious or compromised peer reports
`u64::MAX` and pins the sender out of the federation permanently. Bounding the damage needs a
quorum model that does not exist, and it still fails for peers that were offline during the
exchange and return later holding a higher legacy value.

**Start the new counter at a large constant above plausible legacy usage.** The old counter was
`fetch_add(1)` per outbound message with no protocol-level bound on uptime or rate, so no constant
provably dominates it. This is the invented-future-sequence-space reasoning that produced #2514;
"nobody could have sent that many" is not a proof.

**Introduce explicit sequence epochs — `(sender DID, epoch, sequence)`.** This is the option most
likely to look right, and it does not work. The only party with the problem is a receiver still
running legacy code, and a legacy receiver cannot interpret an epoch field — it goes on comparing
raw sequences exactly as before. Epochs therefore fix nothing that receiver-side migration does not,
while costing a wire-format change, a downgrade-attack surface, and a mixed-version compatibility
matrix. Rejected on the merits, not on effort.

## 4. The mechanism

`MaxSeqEntry` gains `#[serde(default)] semantic_version: u32`. The absence of the key *is* the
signal: an entry written before the field existed deserialises to `0`, the legacy regime. Old
binaries ignore the unknown key, so the change is additive in both directions.

On load, an entry carrying the **legacy** regime does **not** become the floor. It enters the
fail-closed hold #2514 already built for unreadable state — bounded by
`envelope_validity_horizon()` — reported as the typed `ReplayStateLegacy`, which callers must not
score as misbehaviour. `max_seq` and `floor_seq` stay at zero; freshness, not the floor, carries
replay rejection for the duration. An entry from any regime without an explicit migration takes the
indefinite path in §1a instead.

> **Invariant.** Discarding a legacy high-water is safe only for as long as the peer is refused.
> The hold is `future_skew + max_past_age` — the interval after which any envelope captured under
> the old regime fails `verify_age`.

At 300 s clock skew that is **600 s**, deterministic, versus the ~3600 s `max_peer_age_secs` expiry
that currently ends these incidents by luck. Migration completes when the hold expires; the first
subsequent acceptance persists a current-version entry, and later restarts take the ordinary #2514
exact-restore path.

**Unknown-forward is emphatically not this path** — see §1a. A regime with no explicit migration is
held with no deadline, reported as `ReplayStateUnsupportedVersion`, and never graduates into current
semantics however long it waits. The bounded hold here is licensed by knowing what the legacy regime
meant; that licence does not extend to a regime we cannot name.

### Crash safety and idempotence

There is no separate promotion step to crash between: the version is stamped by the ordinary
`persist_max_seq` write that any acceptance already performs.

- Crash before the hold expires → next boot re-reads a legacy entry, re-enters the hold. Fail-closed
  and idempotent; repeated restarts never lower a floor or extend one cumulatively.
- Crash after the first post-migration acceptance → the entry is already current-version, so the
  next boot takes the #2514 path. Migration is one-way.
- An entry held as unsupported is never written to at all, because the hold returns before any
  acceptance can persist. A refused peer therefore cannot restamp state a newer binary owns —
  which would be a silent downgrade — and restarting accumulates no progress toward accepting it.

## 5. Sender-side versioning

The sender needs no migration, but its watermark is stamped anyway so the next semantic change has
a hook and an unknown regime fails closed rather than being guessed at.

The version lives at a **separate key** from the watermark. `outgoing_signing_seq_reserved_end` is
eight raw big-endian bytes that a pre-#2517 binary parses with a fixed-width `try_into`, so
widening it in place would make a rollback fail to start on a store this binary had touched. A
separate key is additive in both directions.

On open: no version key → stamp the current one and leave the watermark **exactly as found** (it is
monotonic by construction under both regimes; resetting it would recreate #2510). Current version →
nothing to do. Anything else → refuse to open, because a sender that guesses wrong reissues
sequences its peers have already accepted.

## 6. Mixed-version compatibility

Two axes, so the matrix is over both. "Established" means this receiver has durable provenance that
the peer's previous namespace was retired (§9.3).

| previous sender regime | receiver state | new authenticated sender regime | expected |
|---|---|---|---|
| legacy | legacy replay (v0) | durable-v1 | bounded migration, then a second bounded migration on the sender axis |
| legacy | current receiver, legacy-tagged high-water | durable-v1 | bounded migration |
| legacy | current receiver | remains legacy | continues, high-water stays legacy-tagged |
| durable-v1 | established durable-v1 | durable-v1 | normal steady state (#2514 exact restore) |
| durable-v1 | established durable-v1 | missing capability | **downgrade → fail closed**, state retained |
| — | transition in progress | receiver restarts | full hold restarts; never shortened |
| — | transition in progress | sender disconnects | no promotion |
| — | transition in progress | sender returns legacy | no promotion |
| unknown future | any | any | fail closed, no deadline |
| none (clean install) | no history | durable-v1 | **one bounded hold**, then established — see §9 |
| durable-v1 | established, high-water aged out by `cleanup()` | durable-v1 | resumes immediately; provenance survived (§9.3) |

> **Supported upgrade order: none is required.** Each node migrates its own state at its own next
> restart, independently of what any peer is running. What migration *does* cost is a bounded
> transition — see the two-hold entry in row 1, and §9.2 on why clean installs are not exempt.

**Why the sender-first order costs two holds.** After the receiver-state migration completes, the
sender axis returns to unproven rather than shortcutting to durable. The shortcut is only valid if
the sender upgraded *before* the first hold began; if it upgraded during, its last legacy envelope
was created at some `X` after the hold started and stays valid until `X + skew + max_age`, past the
hold's end. A receiver cannot date a peer's upgrade, so it assumes the worse case. Pinned by
`sender_first_upgrade_costs_two_sequential_holds_and_no_sender_restart`.

### The bound this does not fix

A node still running legacy code holds its legacy high-water **in memory** and runs no migration
logic — no shipped code reaches it. Its behaviour is unchanged until it restarts onto a binary that
has this fix, and its in-memory state still expires only at `max_peer_age_secs`.

This is a mixed-version property, not a migration failure, and it is not closable from this side:
fixing it would require either a wire change a legacy receiver cannot parse, or an unsound sender
jump. **Migration fixes each node at its own upgrade. It cannot fix a node that has not upgraded.**

## 7. Operational note: bounded logs cannot prove absence

`kubectl logs --since=X` returns nothing — not an error — when the retained buffer does not cover
`X`. During the #2517 investigation a node's buffer rotated roughly every eight minutes at the
volume it was producing, so a four-hour query returned empty and briefly read as "the defect
healed." Cumulative counters showed thousands of events over the same window.

For any historical claim about live behaviour, prefer cumulative/monotonic metrics and direct
inspection of persisted state. Note also that a restart resets process-scoped counters: evidence
held only in a running process is destroyed by the event most likely to be under investigation.

## 8. The sender sequence regime (the second axis)

### 8.1 Why receiver-only versioning was insufficient

Receiver-side semantic versioning makes `max_seq` interpretable *with respect to our own code*. It
says nothing about whose numbering the value came from. The gap is reachable in ordinary operation:

1. receiver A upgrades and correctly migrates its legacy replay state;
2. sender B has **not** upgraded and is still emitting ephemeral sequences;
3. A accepts B's traffic — compatibility during a rolling upgrade requires it;
4. A records a high-water and stamps it *current*, because A is current;
5. B later upgrades; its durable counter starts near 1;
6. A rejects B against a bound belonging to a process that no longer exists.

Step 6 is #2517, recreated — and now **invisible to the migration built to catch it**, because
nothing about the entry looks legacy any more. The canonical regression is
`receiver_first_upgrade_migrates_the_sender_regime_end_to_end`.

> **Invariant.** A receiver must never convert legacy ephemeral sequence history into a
> durable-current replay boundary. The regime recorded with an accepted sequence is the regime the
> *window is established in*, never the regime the receiving binary implements.

### 8.2 The signal: an authenticated capability

`CapabilityFlags::DURABLE_SIGNING_SEQUENCE` asserts four things about the sender, not one:

1. the signing sequence is persisted per-DID, not process-local;
2. it survives process restart, crash included;
3. it is monotonically increasing;
4. it never reuses a sequence that may already have been emitted.

It is advertised unconditionally by every binary implementing #2510, is not operator-configurable,
and is not feature-gated — a build that could advertise it selectively could claim a namespace
property its own storage layer does not provide.

**Nothing may be substituted for it.** Not the software version string (unverified, and release
naming has no protocol meaning), not sequence magnitude (a long-lived ephemeral counter looks
exactly like a durable one), not observed monotonicity (an ephemeral counter is monotonic too,
right up until it restarts), not the Kubernetes image, not uptime, and not `GRACEFUL_RESTART`
(which is about snapshots and was already advertised by binaries with the ephemeral counter).

### 8.3 Missing capability means `LegacyOrUnproven`, never `DurableV1`

> **Invariant.** Absence of the capability is treated as unproven. It is never treated as durable.

Three different peers land in "missing", and only one is dangerous:

- a genuinely pre-#2510 ephemeral sender;
- a #2510-era durable sender built before the capability existed;
- a peer whose capabilities we simply do not hold.

They are treated identically because nothing available to the receiver distinguishes them. The
middle case pays a bounded one-time hold it does not strictly need; that is the accepted cost, and
it is documented rather than optimised away. In particular, entries written by the intermediate
receiver-only-versioning build carry `semantic_version = 1` and no sender axis, so they read as
unproven — which is correct, because that build recorded numbers from senders it never asked about.

### 8.4 Attribution depends on #2520

The capability is usable as regime evidence **only** because Hello claims are bound to the
certificate on the live QUIC connection. Production Hello attribution requires all three of:

1. `message.from == binding_info.did`;
2. the DID's key validates the BindingInfo signature;
3. the BindingInfo certificate hash equals the certificate on the **current** connection.

(1) and (2) alone prove only that the DID authenticated *some* certificate at some point. Every node
publishes its BindingInfo in every Hello, so that pair is replayable by anyone who has ever spoken
to the peer. (3) ties the claim to the live session. Without it, an unrelated attacker could assert
`DURABLE_SIGNING_SEQUENCE` on B's behalf, force B's replay namespace to be retired, and then own
the empty namespace that replaced it.

Pinned by `forged_hello_does_not_corrupt_established_peer_state`, which asserts in both directions:
an authenticated peer's advertised regime **is** recorded, and an unauthenticated peer cannot change
it either way.

## 9. Absence of local history is not absence of a legacy namespace

This is the design gate that a first draft of §8 failed, and it is the subtlest part of #2517.

An early version established the durable regime immediately when the receiver held no replay
history for the peer, reasoning that there was "no old namespace to retire". **That reasoning is
unsound.** `NoHistory` proves exactly one thing:

> *this receiver* currently holds no numeric high-water for this DID.

It does **not** prove that the DID never emitted envelopes under the legacy ephemeral namespace.
A receiver can lack history because it just joined, never spoke to the peer, had its replay store
repaired, or had the window removed by ordinary inactive-peer `cleanup()`. In every one of those
cases the sender may have switched namespaces seconds ago, and envelopes from its previous
namespace remain valid for the full freshness horizon.

### 9.1 A current capability cannot classify a historical envelope

`DURABLE_SIGNING_SEQUENCE` proves the authenticated peer is **currently** using durable semantics.
It does not prove that *this particular envelope* was created under them. A `SignedEnvelope` carries
`from`, a signed sequence, a timestamp, and a signature — and **no sequence-regime marker**.

During the validity window the observables genuinely overlap:

| | captured, signed just **before** B upgraded | legitimate, signed just **after** |
|---|---|---|
| B's signature | valid | valid |
| freshness | passes | passes |
| sequence | arbitrary, incomparable | arbitrary, incomparable |
| B's current connection | advertises `DurableV1` | advertises `DurableV1` |

There is no observable that separates them. This is an information-theoretic limit of the current
wire format, not a coding gap, and it is stated here so no future change assumes otherwise.

> **Invariant.** Current connection capabilities describe the sender *now*. They must never
> retroactively relabel historical signed envelopes.

Left unaddressed the consequence is severe and *worse* than an ordinary first-contact replay window:
a captured legacy sequence (say 15915) is unboundedly above the sender's fresh durable counter, so
accepting it both delivers a replay **and** poisons the durable namespace with a bound the
legitimate sender cannot reach for a very long time — while being tagged current on both axes, so
no migration can ever fire to clear it. Pinned by
`a_captured_legacy_envelope_must_not_poison_a_fresh_durable_namespace`.

By contrast a replayed *durable-namespace* envelope can only carry a sequence the sender actually
emitted, so it sets a floor at or below the sender's live counter and self-corrects. **Only the
cross-namespace case needs the hold.** That asymmetry is why the retirement hold is sufficient
rather than merely helpful.

### 9.2 Consequence: no unproven→durable transition is ever immediate

`LegacyOrUnproven` therefore covers both "known legacy" and "never established", because the two are
behaviourally identical: in neither case do we hold proof that the peer's legacy namespace was
retired. There is deliberately no third, more permissive state.

> **Invariant.** A receiver may establish `DurableV1` replay semantics for a peer only after the
> current connection proves the durable regime, all still-valid traffic from the previous namespace
> has been retired by the horizon, and that transition has been made durable and crash-safe.

**Cost, stated plainly.** Every (receiver, sender) pair pays one retirement hold, once — including a
brand-new federation in which no legacy namespace ever existed, because nothing lets a receiver
prove that. This is not a zero-interruption upgrade and must not be described as one. Removing the
cost would require the envelope itself to name its namespace (an authenticated sequence epoch),
which is a wire change with mixed-version impact and is deliberately **not** taken here.

### 9.3 Provenance outlives the numeric high-water

`cleanup()` deletes an inactive peer's high-water after `max_peer_age_secs`. If the established
regime lived only in that entry, routine garbage collection would manufacture the §9 precondition on
a receiver that had already done the work — re-imposing the hold every quiet hour, and, if absence
were ever read as permission, far worse.

> **Invariant.** Established sender-regime provenance is persisted separately from the numeric
> replay window and is **not** removed by inactive-peer cleanup. Forgetting a high-water must never
> be laundered into "there was never a legacy namespace".

`replay_sender_regime:<did>` is written only at state transitions, so the common path costs no extra
write; it is a few bytes per DID ever seen, bounded by federation size. Pinned by
`established_regime_survives_replay_state_cleanup` and
`cleanup_of_an_inactive_peer_does_not_prove_the_legacy_namespace_never_existed`.

### 9.4 SignedEnvelope is self-authenticating, not connection-bound

Traced during the #2517 design gate and recorded because it constrains what any future fix may
assume:

- the `MessagePayload::Signed` dispatch does **not** require `message.from == envelope.from`;
- it does **not** compare the TLS-authenticated connection identity to `envelope.from`;
- consequently an authenticated peer can deliver a third-party envelope, and
  `peer_connections[envelope.from]` is the **latest known direct connection for that DID**, not the
  connection that delivered this envelope. It may not exist at all.

No in-tree relay currently does this, so it is a latent structural property rather than an exercised
feature. It does not weaken the capability's authenticity — #2520 still guarantees the recorded
capability is genuinely B's — but it does mean connection metadata and envelope creation regime are
explicitly different concepts, which is the same conclusion §9.1 reaches from the temporal side.

## 10. The sender-regime state machine

```text
                    observed: LegacyOrUnproven        observed: DurableV1
LegacyOrUnproven    steady; high-water tagged LEGACY  -> TransitionToDurableV1 (hold)
TransitionToDurable no promotion; stay held           hold until horizon, then promote
DurableV1           DOWNGRADE -> fail closed, keep    steady; #2514 exact restore
Unknown(v)          fail closed, no deadline          fail closed, no deadline
```

**Promotion requires current evidence, not merely elapsed time.** At the end of the horizon the
receiver still requires the message in hand to carry a durable-v1 attribution. Because attribution
is derived per-message from the peer's authenticated connection, it is current by construction: a
peer that disconnected, returned without the capability, or was rolled back supplies no evidence and
is not promoted. Pinned by
`transition_does_not_promote_when_the_peer_returns_without_the_capability`.

**Downgrade fails closed and preserves state.** Once `DurableV1` is established, a later
authenticated connection omitting the capability must not erase the durable high-water — that would
make replay-state reset reachable by downgrade. The legitimate cause is an operator rolling a peer
back onto a pre-capability binary after migration completed; the answer is to roll it forward, and
the receiver deliberately cannot distinguish an honest rollback from an induced one. Pinned by
`a_stale_legacy_connection_cannot_downgrade_established_durable_state`.

**Unknown sender regimes fail closed indefinitely**, on the same principle as §1a and on both the
high-water tag and the provenance record: a known regime can have a bounded migration because its
meaning is known; an unknown one cannot be reinterpreted by waiting.

### 10.1 The retirement horizon

Derived, never a literal:

```text
horizon = maximum permitted future clock skew + maximum permitted past age
        = max_clock_skew + max_clock_skew          (verify_age is symmetric)
        = 300s + 300s = 600s                        at the production configuration
```

An envelope accepted just before time `R` passed freshness then, so its timestamp `T <= R + skew`;
it remains valid until `now > T + max_age`, i.e. until `R + skew + max_age`. A hold of a single
`max_age` would end while such an envelope is at its *freshest*, which is why the naive
single-interval arithmetic is wrong. If the two tolerances are ever configured separately, this must
be recomputed from both, not left at `2 * max_clock_skew`.

### 10.2 Crash semantics

The transition is persisted **without a deadline**, and a receiver restarting mid-transition
restarts the **full** monotonic hold rather than resuming a remembered one.

This is deliberate. A persisted deadline would have to be a wall-clock time, and a clock jump or
rollback could then *shorten* a security hold. Restarting the full hold can only lengthen the
migration — the safe direction in which to be wrong. Every deadline in the module is a
receiver-local elapsed duration on a monotonic clock; none is derived from a sender timestamp or
from `SystemTime`.

Promotion writes the reset numeric namespace first and the provenance record — the authority — last,
so a crash in between leaves provenance still saying "transition" and the restart repeats the hold
rather than accepting under a namespace it never finished proving.

## 11. Capability lying: the honest threat model

A peer controls what it advertises about its own implementation. That is acceptable, and it is worth
being precise about why rather than waving at it.

- **A third party cannot lie about B.** #2520 binds Hello claims to the current connection
  certificate, so capabilities recorded against B prove B authenticated *this* connection.
- **B can lie about B.** A peer holding B's DID key can advertise `DURABLE_SIGNING_SEQUENCE` while
  running an ephemeral counter.

The additional security consequence of that lie is bounded and specific: it lets B cause a receiver
to retire B's own legacy replay bound early and start a fresh namespace for B. It does **not** grant
anything against a third party's state. And an adversary holding B's DID key can already sign
arbitrary fresh envelopes as B — sequences of its choosing, passing every check — so the capability
grants it nothing it did not already have.

> The capability's role is to let **honest, protocol-conforming** peers distinguish sequence
> namespaces, and to remove the captured-message ambiguity that arises during migration. It is not,
> and is not relied upon as, a defence against a peer that already controls its own DID key.
