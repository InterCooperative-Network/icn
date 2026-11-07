---
Status: normative
Canonical: no
Last Reviewed: 2026-11-06
---

# Governance human actor model (`HumanActorModel`) — the legacy-rail firewall

**Slice:** rung 4 of the [#2694](https://github.com/InterCooperative-Network/icn/issues/2694) semantic
convergence ladder · **Architecture owner:** #2602 (GEN) · **Builds on:**
[GEN-A](GEN_SUBJECT_CONTEXT_GENESIS.md), [GEN-B](GEN_SUBJECT_RECOGNITION.md) ·
**Code:** `icn/crates/icn-governance/src/actor_model.rs`, `…/domain.rs`, `…/resolver.rs`,
`icn/apps/governance/src/actor.rs`, `…/manager.rs` ·
**Tests:** `icn/crates/icn-governance/tests/actor_model_firewall.rs`,
`icn/apps/governance/tests/actor_model_firewall.rs`, `actor_model_firewall_tests` in `manager.rs`

This is a **safety boundary**, not a voting system. On the supported, sequential write paths it
refuses a governance domain that has been explicitly configured for context-scoped human Subjects
silently continuing to run on the legacy device-Principal membership, electorate and voting
machinery. The no-downgrade rule is enforced atomically at the typed store (see "Monotonic persistence"
below); the rest is an enforced boundary on the supported paths only, not a guarantee against raw store
writers, stale caches or older binaries.

## 1. The setting

```text
HumanActorModel::LegacyPrincipalV0   default; humans act as device Principals (`Did`s in the
                                     domain's membership source)
HumanActorModel::ContextSubjectV1    explicit opt-in; every legacy Principal-rail operation refuses
```

It is a field of `GovernanceDomain` (not of `GovernanceConfig`). `GovernanceConfig` is replaced
wholesale by config updates; a sibling field cannot be changed by one.

Persisted as `human_actor_model`, `snake_case` (`legacy_principal_v0`, `context_subject_v1`).

## 2. Why legacy is the default, and how that is kept honest

- A record without the field loads as `LegacyPrincipalV0` and behaves exactly as before.
- A legacy domain serializes **without** the field, so its bytes (store rows, gossip
  `DomainCreated`/`DomainUpdated`) are identical to the pre-field encoding. Nothing is rewritten on
  load; there is no implicit migration.
- `null`, an empty string or any unknown value is a deserialization **error**. It is never read as
  either model.
- Subject mode is never inferred from a domain name, a Subject identifier, a device Principal, a
  GEN-B `recognition_hash`, or a membership record.

## 3. The mixed-state invariant

A `ContextSubjectV1` domain carries **no legacy membership source**. `MembershipSource` has no
"none" variant (adding one would break every exhaustive match across crates), so the single inert
shape is `StaticList(vec![])`: it names nobody and resolves no electorate. A non-empty
`StaticList` and any `TrustThreshold` (which always resolves an electorate from the trust graph)
are refused.

Enforced at:

| Boundary | Behaviour |
|---|---|
| `GovernanceDomain::with_human_actor_model` | the only setter; returns `Err` for a mixed domain and refuses leaving Subject mode (no downgrade) |
| `GovernanceDomain::update_config` | returns `Err` and changes nothing if the new config would be mixed |
| `Deserialize` (`try_from`) | a mixed record cannot be loaded from any store or message |

There is **no conversion of membership**, and no supported in-place switch for a persisted domain: the setter is a library builder on an owned value, and `CreateDomain` refuses to overwrite a stored Subject-mode domain on **both** write paths: `GovernanceActor` (refusal op `domain_replacement`) and the standalone `GovernanceManager::create_domain`, which consults the persisted record rather than trusting its in-memory cache (hydration falls back to an empty cache if the store cannot list), and refuses if that record cannot be read. A domain
with legacy members cannot become Subject-mode; unsafe transitions are refused, not repaired. No
migration framework exists in this slice.

**Monotonic persistence (no Subject → legacy).** `GovernanceStateStore::save_domain` never replaces a stored `ContextSubjectV1` record with one that is not. It reads the current bytes, validates the replacement against them, and writes with `Store::compare_and_put` against **exactly the bytes it read**; a writer that lands in between turns into a conflict that is re-read and re-validated (bounded retries; exhaustion, an unreadable stored record, or a backend without atomic compare-and-put all fail closed without writing). This holds for any writer that goes through the typed store, including a manager and an actor sharing one store. It does **not** cover: raw `Store` writes, deletes or `import_snapshot` (a raw delete followed by a save is a create); binaries that predate the `human_actor_model` field (see the downgrade warning); or any rule other than mode monotonicity. In particular the manager's and actor's `CreateDomain`/delegation pre-checks remain read-then-write: they refuse early, but only `save_domain` is the atomic line, and a stale writer replacing one Subject-mode record with another Subject-mode record is allowed by the store.

**Raw access to the governance store (#2864 A2).** The typed check above binds callers of the typed store; a holder of an unrestricted `dyn Store` (or `sled::Db`) for the same database can still write any key. What is true today: (1) inspection found no supported production interface that lets a caller name a raw key (a code audit at the time of writing, not a proof). The `gov:domain:*` key constructor is private to `state_store.rs`, no generic key/value RPC, HTTP or CLI route or command exists, `SledStore::import_snapshot` has test callers only, and `icnctl` backup/restore is file-level under `DataDirLock` (it replaces whole data directories, which can also restore an older snapshot; that is a rollback concern, A5, not a key write). (2) The governance crates no longer hand out the raw handle for the daemon's governance store: `SledGovernanceStateStore::as_store()` is test-only and `init_governance_actor` no longer returns the store (the supervisor's unused copy is gone); both are guarded by `compile_fail` doctests. Inside the process that handle is held by the actor's typed store and its replication-sequence counter (own `gov:repl:seq:` prefix). A caller that wants to read persisted state opens its own store and passes it to `init_governance_actor_with_store`. **Not guaranteed:** protection against any code that already holds another unrestricted handle. The standalone manager's `gateway_store` is one `sled::Db` shared by many components that can write any key, and only convention (nothing else names `gov:domain:`) keeps them off it; raw `put`, `delete`, `compare_and_put` and `import_snapshot` are key-blind public APIs; `SledGovernanceStateStore::new` accepts whatever handle the caller already has; and there is no on-disk protection (no Subject marker or separate keyspace, A6). `apps/governance/tests/raw_store_authority_limits.rs` pins these bypasses on synthetic stores so they are not mistaken for protection.

**Single-writer storage is no longer what the mode invariant rests on, but it is still an assumed layout.** By the documented layout the daemon's governance actor persists to its own store (`store/governance/`, opened at `apps/governance/src/init.rs`), while the gateway opens `{data_dir}/gateway_store` and builds a standalone `GovernanceManager::new_with_sled` on it only when no actor handle is connected (`icn-gateway/src/server.rs`); an actor-backed manager does not use that store for domains. `GovernanceManager::with_domain_store` has no non-test caller. Cross-process sharing is additionally blocked by `DataDirLock` and sled's directory lock, which bind only parties that take them. Ownership is not enforced by the type system. **Manager cache coherence:** when a domain store is attached, the standalone manager's authorization-sensitive decisions (membership edit, vote, close, delegation scope) resolve the domain from the persisted record in preference to its in-memory cache, so a cache that went stale cannot authorize a legacy operation on a domain that is now Subject-mode; an unreadable persisted record fails every such decision closed. A membership edit is applied to a copy of that domain and the cache is replaced only after the copy has been persisted, so a refused or failed save (Subject-mode, conflict, retry exhaustion, unsupported backend, storage error) leaves the cache unchanged; concurrent in-process edits serialize on the `domains` write lock, which is held across the read, the save and the cache swap. This narrows but does not close the cross-writer window: the persisted record can still change between the read and the next step. For a membership edit `save_domain` remains the atomic line. A vote or a close has **no** atomic line: the decision is taken against the persisted record at the moment it is read, and nothing re-checks it before the vote row or the proposal state change, so a concurrent typed writer making the domain Subject-mode in that instant is not excluded (it is outside the supported single-writer layout). Read-only views (`get_domain`, `list_domains`) still serve the cache, and an actor-backed manager has no domain cache of its own. Those residuals, older-binary rollback, and the raw-writer bypass above must be resolved or explicitly accepted **before any real Subject-mode activation**.

## 4. What a Subject-mode domain refuses (fail-closed, before any mutation)

| Operation | Refused at |
|---|---|
| Add/remove a legacy member | `GovernanceActor` `UpdateMembership`; `GovernanceManager::update_domain_membership` (in-memory) |
| Replace config with legacy membership | `GovernanceDomain::update_config`, reached by `UpdateDomainConfig` |
| Cast a Principal-keyed vote | `GovernanceActor` `CastVote`; `GovernanceManager::cast_vote` (in-memory) |
| Resolve a Principal electorate | every `MembershipResolver` (`resolve_members`, `is_member`, async variants, composite) |
| Close a proposal (with or without an eligibility filter) | `GovernanceActor` `CloseProposal`; `GovernanceManager::close_proposal*` (in-memory) |
| Veto or force-close a proposal | `GovernanceActor` `VetoProposal`, `ForceCloseProposal` |
| Create a domain- or proposal-scoped vote delegation | `GovernanceActor` `create_delegation`; `GovernanceManager::create_delegation` (cached domain/proposal first; on a cache miss it consults the persisted proposal and domain records and refuses if either cannot be read; an unknown proposal id keeps the legacy behaviour) |
| Create a domain over a stored Subject-mode domain (replacement) | `GovernanceActor` `CreateDomain`; `GovernanceManager::create_domain` (checks the persisted record; an unreadable record is refused) |

"Before any mutation" is checked, not asserted: the tests snapshot every record the actor can write
(domains, proposals, votes, delegations, proofs) and require byte-equal snapshots across each
refusal. The closure guards run before the scheduler-cancel side effect.

A forced `Accept` is refused because it is an electorate-free override that would otherwise produce an
apparently legitimate decision (empty tally, no votes) for a domain that has no electorate.

## 5. Recognition is not authorization

A GEN-B `SubjectRecognitionV1` (including the non-production `BootstrapRehearsalV1` basis, and
whether or not `is_production_grade()` is consulted) is a verifiable fact. It is **not** a
membership credential, an electorate entry, a vote, or an authorization. This slice adds no
code path that accepts a `recognition_hash` or a `SubjectId` as authority; a test constructs a valid
recognition for the very domain under test and shows the legacy rail still refuses and state is
unchanged. No production recognition issuer exists.

## 6. Status

| Item | Status |
|---|---|
| `HumanActorModel`, default, serde compatibility, mixed-state invariant | **Implemented and tested** (library) |
| Refusal on the actor and in-memory manager paths listed in §4 | **Implemented and tested** |
| Selecting `ContextSubjectV1` through RPC, gateway, CLI or `CreateDomain` | **Not exposed.** Library constructor only; no consumer needs it yet |
| Subject-based membership, electorate, vote, tally, closure | **Deferred.** A Subject-mode domain is refusal-only |
| Production recognition issuer / institutional admission decision | **Deferred** (GEN, #2602) |
| Any production readiness claim | **None** |

Not guarded by this slice, deliberately: proposal creation and the draft/deliberation/open
lifecycle (a Subject-mode domain can accumulate proposals that can never close); blanket-scoped
delegations (they name no domain and are inert because closure refuses); read paths and display
of membership; replication ingest (observe-only in this build).

## 7. What must exist before Subject voting can be enabled

Subject membership roster and admission policy (an institutional decision, not a recognition);
a frozen voting-process snapshot (G1-A); a Subject-keyed vote action and ballot slot (N5-A);
Subject vote-set hashing and deterministic tally; a Subject decision receipt. Each is its own
increment; none is implied by this boundary.

## 8. Compatibility

**Downgrade warning.** A binary that predates this field ignores `human_actor_model`, so it would load a Subject-mode record as a legacy domain with an empty list and could re-save it without the field. Subject-mode records must not be handled by pre-rung-4 binaries. This is harmless while no production surface can create one.

No change to GEN-A derivation or context-kind tags, GEN-B encoding or domain-separation tags, N1
authority-event encoding, device Principal semantics, signed-byte layouts, kernel API, or federation
wire contracts. `GovernanceDomain::update_config` now returns `Result` (callers updated).
