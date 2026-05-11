---
Status: draft
Canonical: no
Last Reviewed: 2026-05-11
---

# Foundational Manual: State Projection and Compaction

This note captures the current drafting doctrine for the ICN Foundational Manual around state projection, canonical facts, snapshotting, and compaction.

It is not a production-readiness claim.

It is not a Phase 2 completion claim.

It is not formal partner authorization.

It is not an implementation issue.

It complements [`FOUNDATIONAL_MANUAL_BASELINE_LOCK.md`](FOUNDATIONAL_MANUAL_BASELINE_LOCK.md) by specifying how a Rust host can derive canonical facts for governance-grade execution without relying on global consensus, a central database, or ambient current state.

## Core doctrine

The Rust host must never compile `canonical_facts` from ambient current state.

It must compile facts from a receipted causal frontier.

For governance-grade execution, state is not a mutable table row.

State is the deterministic projection of an append-only receipt graph up to a known frontier.

Correct doctrine:

> CRDTs are for collaborative deliberation.
>
> ProcessTransitionReceipts are for institutional state.
>
> Merkle roots prove relevant sets.
>
> Causal frontiers locate what was known.
>
> Charters and agreements define what frontier is sufficient.
>
> Snapshots accelerate projection without replacing receipts.
>
> Compaction never destroys auditability.

## Split state doctrine

ICN should not use the same data structure for institutional chatter and institutional spine.

### CRDTs are for deliberation

CRDTs are appropriate for advisory and collaborative surfaces where concurrent edits are legitimate:

- proposal drafts;
- meeting notes;
- comments;
- argument maps;
- preview packet workspaces;
- collaborative educational materials;
- shared working documents.

CRDTs resolve data convergence.

They do not resolve authority.

Once a deliberation artifact becomes part of an institutional process, it must be snapshotted, hashed, and attached to a receipt.

The CRDT is the workspace.

The receipt is the institutional memory.

### Merkle-DAG receipts are for decision

Governance-grade institutional state is a Merkle-DAG of `ProcessTransitionReceipt` records.

Examples:

- `ProcessSessionOpenedReceipt`;
- `DeliberationEntryRecordedReceipt`;
- `DecisionRecordedReceipt`;
- `ActivationCrossedReceipt`;
- `MutationPlanRecordedReceipt`;
- `MutationAppliedReceipt`;
- `EvidencePacketProducedReceipt`;
- `ProcessGateResultReceipt`.

A budget allocation, role activation, final vote tally, emergency authority activation, or settlement record is not merged like a text document.

Reversal is a counter-record, not mutation.

The institutional state at a point in time is the deterministic projection of the receipt graph from genesis or from a trusted checkpoint to the current causal frontier.

## Causal frontier pinning

Before assembling canonical facts, the host pins the exact frontier for the target process.

The frontier should identify:

```text
process_id
target_ref
latest_process_receipt_hash
membership_frontier_hash
standing_context_hash
deliberation_frontier_hash
vote_set_root
notice_receipt_root
privacy_frontier_hash
conflict_frontier_hash
agreement_context_hash
finality_class
```

The host does not claim eternal truth.

It claims:

> This process was evaluated at this known causal boundary.

For a single-institution process, the frontier may be institution-local.

For a federation process, the frontier may require member-entity receipts, federation agreement receipts, reconciliation state, clearing period receipts, and notice receipts across the agreed boundary.

The kernel does not know what that boundary means politically.

The charter or agreement defines the sufficient frontier.

The host enforces the constraint.

## Canonical fact projection

Canonical facts are produced by pure reduction over receipted state.

The host does not query a mutable database and trust the answer.

The host walks or indexes the receipt graph and reduces only the receipts relevant to the process, target, authority context, and frontier.

Example membership-vote projection:

1. Read `ProcessSessionOpenedReceipt` to identify process rules.
2. Read `StandingContextHash` to identify who had standing at the process frontier.
3. Collect valid signed vote receipts causally linked to the process.
4. Exclude revoked, superseded, duplicate, invalid, late, or out-of-scope receipts.
5. Verify notice receipts satisfy the process rule.
6. Verify the liveness/freshness rule in the charter or agreement.
7. Compute counts.
8. Seal the canonical fact snapshot.

The projected fact values may be small:

```text
quorum_required = 0.66
eligible_voters = 42
votes_cast = 31
approvals = 24
rejections = 5
abstentions = 2
notice_delivered = true
sync_constraint_met = true
```

But those values must be backed by roots and receipts:

```text
membership_snapshot_root
eligible_voter_set_root
vote_set_root
notice_receipt_root
state_frontier_id
source_receipt_refs
```

The WASM guest receives bounded facts.

The Rust host keeps provenance.

## Partition and liveness constraints

ICN does not halt the world to obtain global consensus.

That does not mean a partitioned view gets to pretend it is final.

Each process type must define what freshness, liveness, and reconciliation conditions are required before a fact snapshot can seal.

Those rules come from the charter, agreement, scope rule, or emergency authority rule.

Examples:

- A household-local action may require only local finality.
- A cooperative budget decision may require institution finality plus notice receipts.
- A federation clearing decision may require member-entity receipts and federation reconciliation.
- An emergency action may allow local finality with mandatory later review.
- A public commons standard may require broader agreement roots.

If the required frontier is unavailable, stale, forked, or disputed, the host fails closed.

WASM is never invoked for a governance-grade transition when the fact frontier is insufficient.

## Fork handling

A Merkle-DAG can reveal causal divergence.

If two conflicting branches appear, the host does not silently merge them.

The host records the divergence and routes it according to the process rule.

Possible paths:

- reject sealing until the fork is resolved;
- produce a `fork_detected = true` fact for a charter-defined conflict workload;
- trigger a conflict-resolution Action Card;
- open a relational or governance challenge path;
- allow local provisional continuity with explicit non-final receipt status.

The rule is simple:

> Forks are not bugs to hide. They are institutional facts to route.

No fake finality.

No invisible merge of authority-bearing decisions.

No treating eventual consistency as legitimacy.

## Snapshotting doctrine

New nodes should not need to replay an institution's entire history from genesis to evaluate one process gate.

But snapshotting must not become database amnesia.

A snapshot is a receipted checkpoint of a deterministic projection.

It is not a replacement for the receipt graph.

It is not an editable state dump.

It is not a new source of authority.

A valid governance-grade snapshot should include:

```text
snapshot_id
institution_or_scope_id
snapshot_kind
covered_receipt_range
base_frontier_hash
resulting_state_root
included_set_roots
excluded_receipt_policy
projection_function_id
projection_function_version
schema_id
created_by
created_at
authority_context_hash
agreement_context_hash
signature_set
challenge_window
retention_policy
```

The snapshot says:

> Applying this known projection function to these receipts at this frontier produced this state root.

That is the only legitimate shortcut.

## Snapshot kinds

Different state surfaces need different snapshot types.

Candidate snapshot kinds:

### Membership snapshot

Captures members with standing at a frontier.

Used for:

- eligibility;
- quorum;
- representation;
- notice requirements;
- member-facing standing.

### Role and authority snapshot

Captures active roles, grants, mandates, expirations, revocations, and typed scopes at a frontier.

Used for:

- authority resolution;
- execution gating;
- steward review;
- operator/non-operator separation;
- mandate validation.

### Vote set snapshot

Captures valid votes for a process at a frontier.

Used for:

- tallying;
- quorum validation;
- decision receipts;
- challenge review.

### Notice snapshot

Captures delivered, failed, waived, translated, or accessibility-adapted notices.

Used for:

- due process;
- appeal windows;
- meeting validity;
- emergency authority review.

### Deliberation snapshot

Captures the receipted deliberation record or hashed CRDT-derived artifact at a process frontier.

Used for:

- preview/review packets;
- evidence export;
- challenge review;
- institutional learning.

### Agreement frontier snapshot

Captures the governing agreement, compact, charter rule, or federation coordination boundary relevant to the process.

Used for:

- federation reconciliation;
- cross-institution authority;
- shared services;
- clearing;
- finality rules.

## Compaction doctrine

Compaction is not deletion.

Compaction is the replacement of repeated full replay with trusted checkpoints while preserving the ability to audit, challenge, and reconstruct.

The system may compact hot operational state.

It must not compact away legitimacy.

A compacted segment must retain:

- segment root;
- first and last receipt hash;
- receipt count;
- projection function id;
- resulting state root;
- signature set;
- retention rule;
- challenge status;
- archival location or proof of durable preservation.

If an old segment cannot be replayed locally, the node must still be able to verify the segment root and retrieve or request the evidence when needed.

The local hot store can be small.

The audit chain cannot be fake.

## Checkpoint authority

Not every node can mint a governance-grade checkpoint for everyone else.

Checkpoint authority must be scoped.

Possible checkpoint issuers:

- the institution's own cell;
- a federation archive service authorized by agreement;
- a commons archival body with scoped authority;
- a steward review process;
- a quorum of member entities;
- an operator only when authorized by institutional rule and never as political authority.

A checkpoint signed only by an operator key is an operational artifact.

A checkpoint signed under institutional authority is an institutional artifact.

Do not confuse them.

Running the node is not ruling the memory.

## New-node bootstrap

A new node joining a cell should bootstrap from layered checkpoints, not genesis replay.

Safe bootstrap path:

1. Fetch trusted institution identity and charter root.
2. Fetch latest accepted checkpoint manifest for the relevant scope.
3. Verify checkpoint signatures against the charter/agreement authority rule.
4. Verify segment roots and state roots.
5. Fetch hot receipt tail after the checkpoint frontier.
6. Replay only the tail.
7. Recompute state roots.
8. Compare with advertised frontier.
9. Mark local state as usable only after verification.
10. Preserve the ability to request archived receipts for audit or challenge.

A node may be operationally synced before it is governance-authoritative.

That distinction should be explicit.

## Evidence retention and challenge windows

Snapshotting and compaction must respect challenge windows.

No state segment should be compacted beyond practical review while an appeal, challenge, audit, or dispute window remains open.

Retention policy must be tied to:

- rights floor;
- process type;
- privacy class;
- evidence sensitivity;
- legal bridge requirements;
- federation agreement;
- institutional archive policy;
- member export rights.

Privacy matters too.

Some evidence should not be globally replicated forever.

The system must distinguish:

- public receipt hash;
- redacted evidence packet;
- sealed private evidence;
- access-controlled archive;
- deletion or minimization rule;
- counter-record proving deletion/minimization occurred where allowed.

Receipts prove that the process happened.

They do not justify turning every harm process into a permanent public trauma archive.

## Snapshot receipts

Snapshot creation should itself be receipted.

Candidate receipt:

```text
StateSnapshotProducedReceipt
  snapshot_id
  snapshot_kind
  institution_or_scope_id
  covered_receipt_range
  base_frontier_hash
  resulting_state_root
  projection_function_id
  projection_function_version
  schema_id
  produced_by
  authority_context_hash
  agreement_context_hash
  signature_set
  challenge_window
  retention_policy
  record_hash
```

Compaction should also be receipted.

Candidate receipt:

```text
StateSegmentCompactedReceipt
  segment_id
  covered_receipt_range
  segment_root
  checkpoint_snapshot_id
  archival_location_ref
  retention_policy
  compaction_authority
  challenge_status
  record_hash
```

These receipt classes keep operational efficiency from becoming institutional amnesia.

## Relationship to WASM execution

The WASM guest never receives the receipt graph.

The WASM guest never receives the checkpoint authority model.

The WASM guest never receives raw institutional memory.

The Rust host resolves and seals state first.

Then it passes only bounded facts and relevant hashes into the execution envelope.

Execution input should therefore include:

```text
state_resolution_capsule_hash
canonical_fact_snapshot_hash
snapshot_refs
frontier_hash
finality_class
canonical_facts
```

WASM evaluates the static fact capsule.

Rust binds the result to the process transition receipt.

## First implementation slice

Do not begin with universal distributed state compaction.

Begin with one bounded institution-local process.

Recommended first slice:

```text
single-institution proposal decision
membership snapshot
notice receipt root
vote set root
decision receipt
ProcessGateResultReceipt
StateSnapshotProducedReceipt for membership/vote frontier
evidence export
```

Then expand to:

```text
two-institution federation process
federation agreement receipt
member-entity attestations
reconciliation frontier
federation provenance
checkpoint signed by agreement-defined authority
```

Baseline lock is not a theory of all possible state.

Baseline lock is one honest process whose facts can be projected, sealed, executed, receipted, snapshotted, exported, and challenged.

## Closing doctrine

State is not whatever the database says right now.

State is a receipted projection at a causal frontier.

Snapshots are acceleration, not authority.

Compaction is operational hygiene, not amnesia.

The audit chain survives.

The rights floor survives.

The challenge path survives.

The institution can move forward without forgetting what made the movement legitimate.
