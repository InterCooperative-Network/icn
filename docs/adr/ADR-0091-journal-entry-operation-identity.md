---
id: "0091"
title: "Journal entry identity versus economic operation identity"
status: "proposed"
date: "2026-11-11"
deciders: ["ICN maintainers"]
tags: ["ledger", "identity", "idempotency", "settlement", "canonical-encoding"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "proposed. In this tree, executable contract only: icn/crates/icn-ledger/tests/journal_operation_identity.rs (deterministic characterizations + golden vector, passing) and icn/crates/icn-gateway/tests/settlement_operation_identity.rs (control passing; invariant #[ignore]d RED). Draft icn#2879 carries an opt-in implementation of N3 as amended below (Ledger::append_operation, journal-derived index) with no production caller. No production author is migrated."
references:
  - "icn#2871 (journal entry identity and duplicate application), icn#2872 (treasury nonce keying and retry safety), icn#2873 (draft: refuse an already-journaled id)"
  - "ADR-0090 (proposed, not yet in this tree): per-effect identity for governance-authorised treasury effects. This ADR covers every OTHER ledger author and the ledger-side contract both rely on."
  - "docs/architecture/CANONICAL_ENCODING.md (4.1.1 typed_hash domains; 4.1.3 field-wise tagged hashes; Rule 2 frozen bytes)"
  - "icn/crates/icn-ledger/src/hash.rs (compute_entry_hash), src/entry.rs (JournalEntryBuilder::nonce)"
---

# ADR-0091: Journal entry identity versus economic operation identity

## Status

`proposed`. Nothing here is accepted. The tests named in `implementation_status` pin current
behaviour and the target invariant; they do not adopt the decision.

## Context

### What the id is today

`JournalEntry.id = SHA-256(serde_json({timestamp, author, contract_ref, accounts, parents, nonce}))`
(`hash.rs`). It is a **content digest**. `provenance`, `signature` and witness signatures are not
in the preimage. `nonce` is `Option<[u8; 32]>` and is always serialized (`"nonce":null` when
absent), so setting a nonce is a change of content, not of encoding. The preimage is pinned by
`nonce_less_entry_id_golden_vector`, cross-checked with an independent SHA-256.

This hash is not one of the `typed_hash` domains of CANONICAL_ENCODING §4.1.1 (the reserved
`icn-ledger:journal-entry:v1` domain has no code) and is not listed among the §4.1.5 known
exceptions. That documentation gap is noted, not fixed, here.

### Why it collides

Production authors that set no nonce (`origin/main` dfc561f48):

| Author | Site | Stable operation identity available at build time |
|---|---|---|
| Gateway settlement (HTTP, escrow release, recurring) | `icn-gateway` `LedgerManager::create_settlement` | HTTP: none (no idempotency key in the API). System paths: the caller's escrow / schedule record |
| Gateway treasury deposit | `icn-gateway` `TreasuryManager::create_deposit` | none |
| Compute receipt settlement | `icn-ledger` `settlement.rs` `settle_receipt` | yes: the verified `receipt_hash` (already the in-memory dedup key, not in the entry) |
| CCL contract execution | `icn-ccl` `runtime.rs` `apply_ledger_operations` | partial: the contract, but no invocation id; one entry per `Transfer` op in a loop |
| DID recovery migration | `icn-ledger` `transfer_balances_for_recovery` | the recovery record |
| Cross-currency transfer | `icn-ledger` `prepare_cross_currency_transfer[_with_rate]` | none |

Commons credits already set a nonce (the receipt hash). The two entries of one commons settlement
(earn and spend, `settlement.rs`) carry the **same** nonce, so a nonce does not name a single
entry today. Treasury entries set none on `main`; an unpublished local line binds them to the
governance receipt (see "Relation to other work").

Two distinct, legitimate operations of the same shape built in the same millisecond have the same
id (`distinct_operations_without_nonce_in_one_millisecond_share_an_id`), even when their
authorisation proofs differ (`provenance_does_not_separate_same_millisecond_operations`). On
`main` the second silently overwrites the first journal record and applies its delta again.
Measured on `main` dfc561f48 through the real gateway path
(`identical_settlements_are_distinct_durable_entries`, run twice): 200 back-to-back identical
settlement requests all returned `Ok`, yet produced only **44** and **40** distinct entry ids.
Roughly four in five legitimate settlements had no journal record of their own. With
icn#2873 it is refused: consistent, but a legitimate request fails. That is the required-CI
failure on #2873 (`icn-gateway` `budget_integration`). A CCL contract with two identical
`Transfer` ops is the in-process form of the same defect: with #2873 the first op applies and the
second is refused, so the contract is partially applied.

### Why id equality can never give idempotency

The timestamp is in the preimage. A retry of ONE operation built a millisecond later is a
different entry even with the same nonce
(`same_operation_nonce_in_a_later_millisecond_is_a_different_entry`). Refusing duplicate ids
(#2873) catches only same-millisecond duplicates. Retry safety needs a separate, create-only
index keyed by the operation identity. The treasury receipt claim index is the only existing
instance.

## Answers to the identity questions

1. **What the id represents:** the content digest of one journal record. Today it is also the
   only uniqueness key, which is the overload that fails.
2. **Digest, operation identity or idempotency key:** a digest. It should not be the idempotency
   key (point above). Operation identity belongs in `nonce` and an operation index.
3. **Identical content, same id:** yes, always (determinism). Two operations must therefore
   differ in content, which is what an operation nonce provides.
4. **Distinct identical-looking actions:** different operation identities, hence different nonces.
5. **Retries keep identity:** the retry carries the same operation identity; the operation index,
   not the id, recognises it (the retry's id may differ).
6. **What survives restart and replication:** the entry (including its nonce) and the operation
   index. Both are derivable from the journal, so the index can be rebuilt from it.
7. **Replay versus new action:** only by operation identity. The ledger cannot tell them apart
   from content alone when the author set no nonce.
8. **Receipts and custody identifiers:** they are the *sources* of operation identity where they
   exist (governance receipt / ADR-0090 effect id; compute receipt hash). Provenance is not.
9. **Same event through several routes:** same entry, same id, same nonce: one record. Inbound
   ids are currently taken on trust; this ADR does not change that and depends on the separate
   inbound-verification work before the operation index is fed from sync.
10. **Encoding changes that would invalidate history:** any change to the preimage, its field
    order, its JSON serialization, or the hash function. This ADR proposes none.

## Decision (proposed)

**N1. The entry id stays a content digest over the existing preimage.** No change to `hash.rs`,
field order, serialization or hash function. The golden vector pins it.

**N2. Every production ledger author sets `nonce` to an operation discriminator.**
`nonce = BLAKE3(tag || fields)` per CANONICAL_ENCODING §4.1.3, one registered tag per operation
class, each pinned by a golden vector:

| Class | Proposed fields (owner to confirm) |
|---|---|
| governance treasury effect | ADR-0090 effect identity (decided there) |
| compute receipt settlement | `receipt_hash` |
| CCL contract execution | contract execution id, op index (needs an invocation id: gap) |
| DID recovery migration | recovery record id, currency, delta index |
| gateway system settlement | escrow id, or recurring schedule id + cycle |
| gateway HTTP settlement / deposit | coop id, authenticated subject, client idempotency key (API addition, D2) |
| cross-currency transfer | transfer/quote id (gap) |

**N3. Exactly-once is enforced by an operation index, not by id equality.** An author that names
an operation appends through an operation-aware path. One operation id names exactly one journal
entry; an operation that needs several entries derives one id per entry.

- A new operation id: the entry is appended.
- The same operation again (equal in everything the entry id covers except the timestamp): nothing
  is appended and the original entry id is returned, so a lost response is recoverable. This
  answer means the entry is journaled. It does not mean every projection of it was written
  (ADR-0090 I3).
- The same operation id with different content, or an id already carried by more than one entry:
  refused.

The rule binds authors that opt in. It is **not** "any second entry with a claimed nonce is
refused": the commons pair shares one nonce across its two entries and must keep appending through
the plain path until it is migrated to one id per entry. (An earlier draft of this ADR stated the
broader rule; it would have refused the second entry of every commons settlement.)

The index is derivable from the journal, because the operation id is in the entry. It can
therefore be rebuilt on open and needs no persisted format of its own; persisting it is an
optimisation (D3).

**N4. Nonce-less entries stay valid.** Historical entries are never re-hashed or re-keyed. Once
every production author is migrated, a new nonce-less production append may be refused (D4).

**N5. Conflicts on replicated entries quarantine, never apply** (two different entries claiming
one operation). Depends on inbound id verification.

### Alternatives rejected

- **Random or finer-clock nonce.** Gives distinctness only; a retry gets a new value, so it never
  gives idempotency. A process-local random value in an event identity also needs explicit
  authorisation under the Determinism invariant. Acceptable at most as an interim
  distinctness-only measure for the HTTP path, and only if D4 authorises it.
- **Put provenance in the preimage.** Re-hashes every entry type (historical ids move: Rule 2),
  and provenance has its own lifecycle (aliases such as `signature` -> `auth_proof`, forwarded
  copies). It also gives no retry idempotency.
- **Chain entries through `parents`.** `parents` carries fork semantics (`fork_resolution.rs`:
  a shared parent with two children is a fork, resolved by trust score). Concurrent same-author
  settlements would become forks. Activates dormant machinery for a uniqueness problem.
- **Versioned entry identity (move to `icn-ledger:journal-entry:v1` typed_hash).** Correct in the
  long run for canonical-encoding hygiene, but not needed for this defect: `nonce` already exists
  in the preimage. Reserve it for a change that must alter the preimage.

## Compatibility and migration

- Existing ids, proofs and stored records: unchanged (N1, golden vector).
- New nonce-bearing entries: readable by older binaries (the field already exists and is
  deserialized). Older binaries do not consult the operation index, so a retry handled by an
  older binary can still double-apply: forward-only rollout per node, stated in deployment docs.
- The operation index is a new persisted keyspace (versioned key, unknown versions fail closed,
  per protocol-state-migration invariants), rebuildable from the journal.

## Verification requirements

Before acceptance can be claimed as implemented:

- `identical_settlements_are_distinct_durable_entries` passes without `#[ignore]`.
- Same-operation retry in a later millisecond returns the original entry; journal and balances
  unchanged; holds across reopen.
- Two distinct operations in one millisecond: two entries; across reopen `verify_integrity` passes.
- Golden vector unchanged; one new golden vector per new tag; domain-tag audit passes.
- CCL two-identical-ops contract applies both ops (or neither); never one.
- #2873's duplicate-id refusal stays in force and its tests stay green.
- Mutation: drop the nonce at each migrated author; skip the operation-index claim; claim after
  the journal write.

## Human decisions required

- **D1** Accept N1-N3 (operation identity lives in `nonce` + operation index; the id stays a digest).
- **D2** HTTP idempotency: header name, scope (coop, authenticated subject), retention window,
  behaviour on key reuse with a different body. Touches the public API (OpenAPI + SDK).
- **D3** Operation index: derived from the journal on open (needs no format; one extra journal
  scan on open) or persisted (new keyspace, atomicity with the journal record). Also whether an
  entry removed from the journal leaves a permanent refusal for its operation, which only a
  persisted record can give.
- **D4** Interim gateway policy until D2 ships: keep #2873's refusal (identical same-millisecond
  requests fail) or authorise a server-assigned request nonce as distinctness-only.
- **D5** Whether the governance-receipt nonce binding on the unpublished O-6 line is adopted as the
  treasury instance of N2, given it changes future treasury entry ids.

## Relation to other work

- **icn#2873** is the necessary safe half (refuse silent overwrite). It should land with or after
  D4, because without an operation nonce it turns a silent corruption into a visible refusal of
  legitimate same-millisecond requests.
- **ADR-0090** decides the identity of governance-authorised effects; this ADR supplies the
  ledger-side contract those identities feed (N2 row 1, N3). The two do not compete: ADR-0090
  owns the decision commitment, the effect list and the receipt claim state machine; this ADR owns
  "at most one journal entry per operation id". The receipt-bound nonce on the unpublished O-6
  line is a third derivation (SHA-256, receipt-scoped) that matches neither ADR; ADR-0090 I4 rules
  out a receipt-scoped key for one effect of a multi-effect proposal (see D5).
- **icn#2872**: the treasury spend nonce is a sequence, not an operation identity; exactly-once
  for treasury still comes from the receipt claim / ADR-0090 effect id, and the sequence lane
  must be keyed by the debited account (discussed on #2872).
