---
id: "0087"
title: "N1 authority-fact persistence boundary"
status: "accepted"
date: "2026-10-23"
accepted_date: "2026-10-23"
deciders: ["Matt Faherty"]
tags: ["identity", "n1", "authority-log", "persistence", "durability", "api-boundary"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "implemented on the #2800 branch (not yet merged); no daemon integration"
references:
  - "icn#2799 (N1-D: local restart durability)"
  - "icn#2800 (the implementing PR)"
  - "icn#2694 (N1 ladder)"
  - "docs/architecture/HUMAN_IDENTITY_ARCHITECTURE.md (§9.1, §9.2, §9.2.1, §9.2.2: the N1 durable layer)"
  - "icn/crates/icn-identity/src/authority_log/body.rs (the canonical body encoding the record value carries)"
  - "icn/crates/icn-identity/src/authority_log/fact_store.rs"
  - "icn/crates/icn-core/src/authority_facts.rs"
---

# ADR-0087: N1 authority-fact persistence boundary

## Status

**`accepted` (2026-10-23).** Drafted with #2800. The maintainer (Matt Faherty,
the decider) approved its direction and substantive decisions on 2026-10-23,
with three documentation clarifications that are applied in this text:
- §1: the four-item surface is the persistence boundary, not every export of
  the module;
- §3: duplicate writes are semantically idempotent, not skipped;
- §5: `put_fact` does not validate admission.

The approval was given as a written instruction in #2800's delivery session.
ADR-0018 requires no separate signature.

Decision status and implementation status are separate axes (ADR-0018). The
implementation is on #2800's branch and is not merged, and nothing in the
daemon uses it yet.

## Context

N1's durable layer is two grow-only sets — admitted authority bodies and their
witnesses — from which `derive` computes authority as a pure function
(HUMAN_IDENTITY_ARCHITECTURE §9.2). N1-D (#2799) makes those sets survive a
restart of the process that holds them.

That needs a boundary between two crates. `icn-identity` is kernel-class and
carries no storage dependency, so it cannot own a backend. `icn-core` owns the
runtime's storage, so it cannot be trusted to decide what a stored record
means. Whatever crosses between them becomes a compatibility surface: other
crates can depend on it, and records written through it outlive the code that
wrote them.

Review of #2800 found that the first draft exported more than that boundary
needs (an in-memory test store, a key-length constant, a method that turns a
record into an admitted event without going through `rehydrate`), had no
recorded decision behind any of it, and acknowledged writes that had not been
made durable.

## Decision

### 1. The persistence boundary's public surface is four items

For the N1-D persistence boundary, `icn_identity::authority_log` exports
exactly these four items. The module exports other things — bodies,
construction, derivation, the in-memory `AuthorityStore` — which this ADR
neither adds nor governs.

| Item | Why it is public |
|---|---|
| `trait AuthorityFactStore { put_fact, load_facts }` | the contract a medium implements; `icn-core` implements it |
| `PersistedFact` (opaque) with `from_event`, `key`, `value`, `from_key_value` | the record a medium moves; a writer builds it from an admitted event, a backend round-trips its bytes |
| `FactStoreError` | the one error type both sides speak |
| `fn rehydrate(&dyn AuthorityFactStore) -> Result<AuthorityStore, FactStoreError>` | the only way back from records to an admitted store |

Everything else of the persistence module is crate-private. The conversion from a record to an admitted
event is reachable only through `rehydrate`. That narrows the surface; it is
not what makes storage safe. Admission is enforced at `AuthorityStore::ingest`,
which re-checks every body whatever its source, so storage cannot widen what
is admitted either way. An in-memory store is test code, not API.

`icn_core::authority_facts` exports the adapter (`SledAuthorityFactStore`), its
namespace prefix (`AUTHORITY_FACT_PREFIX = "authority/fact/"`), and the
`DurableStore` durability trait (§4).

### 2. The record encoding

```text
key   = event_id (32 bytes) || witness signature (64 bytes)
value = canonical_bytes(body)
```

The key is the witness element itself. The value is the body's canonical
encoding (`authority_log/body.rs`), which opens with the length-prefixed
`icn.authority-log` domain separator and the protocol version. The record adds no framing of its own.
The adapter stores a record under its namespace prefix followed by the key
verbatim, and never re-encodes the key.

### 3. Idempotence and duplicates

A record's key is its identity, so writing a record that is already present is
**semantically idempotent**: the held set is unchanged, and the write is not an
error. That describes the outcome, not a skipped operation. The `icn-core`
adapter still performs the physical write and the durability barrier on every
call, duplicate or not. Two valid signatures over one body are two records, which
rehydrate to one body and two witnesses. Choosing between them would be a
content tiebreak the durable lattice forbids (§9.2.1).

### 4. The durability acknowledgment

`put_fact` returning `Ok` is the acknowledgment. It means the record has
crossed the implementation's persistence barrier and survives a restart of the
process. An implementation that cannot make that true must not return `Ok`. On
`Err` the record may or may not be held. It was not acknowledged, and writing
it again is safe (§3).

The `icn-core` adapter writes, then calls the backend's durability barrier,
and acknowledges only if both succeed. `Store::flush` cannot be that barrier,
because it defaults to a no-op that any implementation inherits silently. So
the adapter accepts only `Arc<dyn DurableStore>`, where
`DurableStore: Store` has one required method, `sync_durable`, with **no
default**. `SledStore` implements it with its flush (how far that reaches is
in §9); nothing else in production implements it.

A backend becomes eligible only through an explicit `impl` that writes its
barrier out. A no-op barrier can exist only where someone wrote one, in code a
reviewer reads. The constructor's type enforces this at compile time: a plain
`Arc<dyn Store>` is rejected.

This is a claim made visible, not a proof. Nothing can check that an
implementation's barrier really does what it claims. The trait is not
sealed, because tests in other crates need fault-injecting implementations.

The cost is one flush per fact. N1 events are rare, bounded per subject and
written at authority transitions. A batched write that flushes once per batch
is the right optimization when a caller shows it needs one.

### 5. Rehydration and admission

`rehydrate` re-admits every record through `admissible_bytes`, the same
state-independent gate a live event passes, then checks that the key's digest
matches the decoded body, then ingests. Admission proves only that some key
signed these bytes. A well-formed body signed by an unauthorized key is
admitted, stored and rehydrated like any other, and is excluded by `derive`,
exactly as it is live. Storage neither drops nor promotes it.

**`put_fact` does not validate admission.** It persists whatever record it is
handed. A runtime producer must admit a fact through `AuthorityStore::ingest`,
or `admissible`, before persisting it.

Rehydration validates every loaded record again, independently, and fails
closed (§7). So a record that should never have been written cannot become
admitted state. It can, however, make every later rehydration refuse. That is
why validation on the producer side is required, not optional.

### 6. Ordering

None is promised and none is needed. Bodies order by their canonical bytes, so
the rebuilt sets do not depend on the order a backend returns rows. A backend
may return records in any order.

### 7. Corruption fails closed

`rehydrate` stops at the first record it cannot admit and returns the error:
- a key of the wrong length;
- a value that does not decode, or carries an unknown protocol version;
- a key that names a different body than its value encodes;
- a row outside the namespace.

There is no "skipped" or "repaired" outcome. A partially rehydrated store is a
different history from the one persisted, and returning it would let one bad
row silently change derived authority. What a caller does with the refusal is
the caller's decision (see the follow-on daemon design in #2800). It must not
be "start empty".

### 8. Versioning and evolution

The version lives in the body encoding. A record written by a future encoding
is refused as inadmissible, not guessed at. A change to the record layout
(key shape or value framing) is a breaking change to persisted data. It must
use a new namespace prefix or an explicit migration, and must never reinterpret
existing rows in place. Adding a method to `AuthorityFactStore` is a breaking
change for implementors and needs this ADR amended.

### 9. What this does not claim

- No claim that a fact which never reached the store, or which vanished from
  the medium, is detectable. Frontier and completeness proofs are separate.
- No power-loss proof, and no claim of power-loss safety from sled.
  The tests show acknowledged facts survive the writing process being killed.
  In sled 0.34 on Linux, `flush` waits with `sync_file_range`. That does not
  flush the drive's write cache or sync file metadata; sled issues a full
  `fsync` only from its periodic flusher and on close. So the
  acknowledgment is crash-of-process safe, and only best-effort against power
  loss.
- No daemon integration. Nothing in the runtime constructs this store yet.

## Consequences

- `icn-identity` gains a small, deliberate, documented storage contract and no
  storage dependency.
- Every acknowledged fact costs a flush.
- A new backend must opt in to `DurableStore`. Someone wiring an in-memory
  store into the adapter "for now" gets a compile error, not a silent loss of
  durability.
- The persisted record layout becomes a compatibility commitment from the
  first write on a real node (§8).

## Implementation status

Implemented by #2800, which covers §1–§7:
- the narrowed exports;
- the flush-then-acknowledge adapter;
- the `DurableStore` trait with its required barrier;
- fail-closed rehydration.

The evidence is in #2800's tests:
- the contract against an in-memory store;
- a real sled store closed and reopened;
- a deterministic fault-injection backend proving acknowledgment only after a
  successful flush, failure propagation, and safe retry;
- a child process SIGKILLed after acknowledging, with its facts present on
  reopen.

The decision is accepted. The implementation counts as landed only once #2800
merges. Daemon wiring is separate follow-on work, and no producer exists yet
to do the admission §5 requires before persisting.

## Alternatives Considered

| Alternative | Why rejected |
|---|---|
| Keep the first draft's wider exports (in-memory store, key-length constant, public record-to-event conversion) | Freezes test scaffolding and internals as API. The conversion was not a safety hole, because `AuthorityStore::ingest` re-checks admission whatever a body's source. But it was a second public route to the same place, with nothing to gain. |
| Let callers call `Store::flush` themselves | Makes durability depend on every caller remembering it. The acknowledgment should mean one thing. |
| Accept any `Store` and trust its `flush` | The trait's default `flush` is a no-op, so an in-memory or test store would acknowledge durability it does not provide. |
| A plain marker trait `DurableStore: Store {}` over `Store::flush` | A first draft of this PR. A marker still lets an implementation claim durability while inheriting the no-op `flush`. A required method without a default does not. |
| A sealed trait | Tests in other crates (fault injection) could not implement it, so the failure paths would go untested. |
| Add an `is_durable()` method to `Store` and check it at runtime | Widens a shared trait for one consumer, and moves the failure to run time. |
| Skip unreadable records during rehydration | Silently changes the derived authority. A refusal is the only outcome that cannot be mistaken for a smaller history. |
