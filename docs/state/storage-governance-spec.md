# Storage is Governance — ICN Storage Specification

**Issue**: [#1131](https://github.com/InterCooperative-Network/icn/issues/1131)
**Status**: Core types implemented; StorageSpec and validate_storage_access() not yet present
**Date**: 2026-03-22

## Thesis

In ICN, storage allocation is a governance act. Who stores what, where, and for how long
is not a configuration detail — it is a policy decision that must be expressed through
the same constraint enforcement architecture as all other resource allocation.

Storage policy has the same structure as any other policy oracle output: apps express
intent in domain terms (membership tier, trust class, data sensitivity), the kernel
enforces placement and access rules without understanding the origin of those rules.

## Implementation

The storage governance layer is implemented in `icn-kernel-api/src/storage.rs`.

### StorageClass

`StorageClass` classifies data by mutability and replication requirements. Three variants:

**`Canonical`** — Immutable, replicated, authoritative. Used for ledger entries,
governance receipts, and trust edges. Cannot be modified after creation. Requires
active replication across the cooperative. Only `Canonical` storage can hold outputs
that mutate ledger or governance state (`can_hold_canonical_output() == true`).

**`ServiceState`** (default) — Mutable, coop-scoped application state. Used for
calendars, inventory, schedules, user preferences. Updated via compare-and-swap.
Not replicated by default. The safe default for general compute task outputs.

**`Blobs`** — Content-addressed binary data. Used for documents, media, WASM modules,
backups. Immutable by reference (addressed by content hash), but not actively replicated.
Locality rules determine placement. `is_immutable()` is true because content addressing
makes mutation semantically impossible; `requires_replication()` is false.

The governance constraint that flows from this: a task that produces `Canonical` outputs
cannot silently store them as `ServiceState`. Any attempt to do so is a violation of the
storage contract, surfaced as `StorageValidationError::CanonicalTaskNonCanonicalStorage`.

### DataLocality

`DataLocality` constrains where data can reside and which tasks can access it. Four
levels forming a strict hierarchy from narrowest to widest:

```
CellLocal (0) < CoopReplicated (1) < FederationMirrored (2) < CommonsPublic (3)
```

**`CellLocal`** (default) — Data must remain in the originating cell. Used for
sensitive local state, cell-specific configuration, data subject to local regulations.

**`CoopReplicated`** — Data replicated across cooperative nodes. Used for shared
coop state, member data, internal services.

**`FederationMirrored`** — Data mirrored for cross-entity interoperability. Used
for federated identity, cross-coop coordination, inter-cooperative agreements.

**`CommonsPublic`** — Data publicly available in the commons. Used for public indices,
shared libraries, open datasets, commons-contributed resources.

The access rule is directional and strict: a task at locality level L can only access
data at locality level >= L. Narrower tasks see everything. Wider tasks cannot
reach into narrower scopes. `DataLocality::can_access(data_locality)` returns true
if and only if `self <= data_locality` (numerically: task level <= data level).

This means `FederationMirrored` tasks cannot read `CellLocal` data — which is the
governance invariant that prevents federation-scope compute from leaking cell-private
state. This is enforced as `StorageValidationError::LocalityAccessViolation` or the
more specific `StorageValidationError::CellLocalFederationViolation`.

### StorageValidationError

Three error variants encode the governance violations the system can detect:

- `CanonicalTaskNonCanonicalStorage` — A canonical task specified non-canonical output
  storage. Records the actual `StorageClass` used for diagnostic messages.
- `LocalityAccessViolation` — A task at one locality level attempted to access data at
  a narrower locality level. Records both the task locality and data locality.
- `CellLocalFederationViolation` — Specialized variant for the specific case of
  `CellLocal` data accessed by a `FederationMirrored` task. This is the highest-risk
  locality violation (federation-scope task reaching into local-only data).

### Implementation Gap (as of Sprint 23)

The task brief for s23-t7 anticipated two additional types:

- `StorageSpec` — A composite struct combining `StorageClass` and `DataLocality` into a
  single constraint that could be attached to compute tasks.
- `validate_storage_access()` — A free function to enforce storage governance at the
  kernel boundary, returning `Result<(), StorageValidationError>`.

Neither is present in the current `storage.rs`. The error variants exist but are not
wired to any enforcement function. This means the error taxonomy is defined but not
yet callable at the kernel boundary.

**Required follow-up**: Implement `StorageSpec` and `validate_storage_access()` to
complete the enforcement path. The error types and semantics are already correct.

## Governance Semantics

Storage decisions in ICN follow the meaning firewall pattern:

- Apps (PolicyOracles) express storage policy in domain terms: a membership tier
  determines whether a task may write `Canonical` outputs; a trust class determines
  whether a task may access `FederationMirrored` data.
- The kernel receives a `StorageClass` and `DataLocality` constraint attached to a
  compute task. It enforces these without understanding that the locality constraint
  came from a trust score or that the class constraint came from a governance rule.
- This ensures storage policy is auditable, composable, and governed — not hardcoded.

The ordering property on `DataLocality` (it implements `Ord`) means the enforcement
rule `task_locality <= data_locality` is a single integer comparison at runtime.
The governance complexity lives entirely in how apps set the task's locality, not in
how the kernel checks it.

## What This Unlocks

Storage governance at the kernel level makes distributed compute viable without
data sovereignty violations: a task dispatched to a federation node cannot access
cell-local data even if the federation node is otherwise trusted. This is a prerequisite
for commons-scope compute tasks that operate on public resources without leaking
coop-internal state.

## Relationship to P0 Completion

This spec closes the documentation gap for issue #1131. The `StorageClass` and
`DataLocality` types were implemented as part of the Meaning Firewall arc (Sprints
19-22). The `StorageSpec` and `validate_storage_access()` function referenced in the
original issue acceptance criteria are not yet implemented — they are a remaining
follow-up. This document provides the written specification that makes the existing
implementation's governance intent legible, and identifies the outstanding gap.

## Sprint 23 Notes

This task (s23-t7) is independent of s23-t5 (CRDT) and s23-t6 (ContainerRuntime).
The `StorageClass` and `DataLocality` implementation is complete with full test coverage
(21 tests covering ordering, access rules, serde roundtrips, and error display).
`StorageSpec` and `validate_storage_access()` remain to be implemented to complete
the acceptance criteria from the original issue.
