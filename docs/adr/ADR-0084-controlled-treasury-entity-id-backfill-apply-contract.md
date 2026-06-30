---
id: "0084"
title: "Controlled treasury entity-id backfill apply contract"
status: "proposed"
date: "2026-06-30"
deciders: ["Matt Faherty"]
tags: ["authz", "treasury", "entity", "rfc-0018", "backfill", "meaning-firewall", "security"]
supersedes: []
superseded_by: []
amends: ["0035"]
implementation_status: "proposed"
references:
  - "ADR-0035-entity-aware-request-authorization.md (amends — adds the apply-side safety contract for the entity_id this gate reads)"
  - "docs/rfcs/RFC-0018-entity-aware-request-authorization.md (identifier-reconciliation lane)"
  - "icn/crates/icn-ledger/src/treasury_entity_backfill.rs (read-only planner, #2258)"
  - "icn/crates/icn-entity/src/coop_entity_map.rs (CoopEntityBindingProvenance::is_trusted_for_resolution)"
  - "icn/bins/icnctl/src/main.rs (treasury entity-backfill-report, #2262)"
  - "GitHub #2082 (mapping/backfill lane), #2258 (planner), #2262 (read-only report), #2081 (treasury enforcement cutover — deferred)"
---

# ADR-0084: Controlled treasury entity-id backfill apply contract

> **Note on ADR id:** numbered `0084` (next free issued id) because `ADR-0036`
> and the `0021`–`0082` tranche are reserved candidate ids in
> `ops/coordination/adr_candidates.yaml` (e.g. `0036` = "Federation Agreement
> Support"). ADR-0083 skipped past the same reserved block for the same reason.
> This is **not** the reserved federation-agreement ADR.

## Status

`proposed` — design/decision artifact only. **No implementation.** This ADR
specifies the safety contract that any future *mutating* treasury `entity_id`
backfill apply path MUST satisfy, written **before** that apply code, so the
contract constrains the implementation rather than being reconstructed after it.
It amends ADR-0035 (which landed `require_entity_access` plus treasury
*observe* mode) on the apply side; it does not change ADR-0035's decision.
`implementation_status: proposed` — nothing here is built.

## Context

The #2082 lane establishes a canonical, persisted, reversible `coop_id ↔ EntityId`
mapping/backfill so that entity-aware authorization (ADR-0035, RFC-0018) can be
applied to coop-namespaced endpoints whose treasuries carry no stored `entity_id`.
Two read-only rungs have landed:

- **#2258** — `TreasuryManager::plan_entity_id_backfill` classifies, fail-closed,
  which legacy treasuries (`entity_id: None`) *could* be populated from a trusted,
  non-ambiguous binding. It writes nothing.
- **#2262** — `icnctl treasury entity-backfill-report [--json]` surfaces that
  planner against real persisted state, never collapsing persisted treasuries to
  `total: 0`, and creating no store.

The next step in the lane would be a *mutating* apply that populates
`treasury.entity_id`. That step is the first one that is **not**
authorization-neutral, so its safety contract must be fixed first. This ADR is
that contract. It does not authorize writing any apply code.

## Decision

A future controlled apply path MAY populate `treasury.entity_id` for eligible
legacy treasuries **only** under the contract below. Until a later, separately
reviewed implementation PR adopts this contract, no apply exists and the default
posture stays read-only.

### 1. Why apply is not authorization-neutral

`treasury.entity_id()` is read by the treasury entity-auth gate
(`require_entity_access`, ADR-0035). Under the observe-only mode that is the
**default** (`ICN_TREASURY_ENTITY_AUTH_MODE` unset / `observe-only`), the gate
records an observation and changes no route outcome, so populating `entity_id` is
invisible to callers.

The enforce mode (`ICN_TREASURY_ENTITY_AUTH_MODE=enforce-trusted-resolver`) is
**already reachable** in any dev/test process that sets the env var — it landed
with #2254, and `icn-gateway/src/api/treasury.rs` branches on
`active_treasury_entity_auth_mode()` and, under
`TreasuryEntityAuthMode::EnforceTrustedResolver`, denies a `WouldDeny` outcome with
HTTP 403 via `treasury_gate_enforcement_denial` (the enforce-mode runbook, #2255,
documents this). So enforce mode is **not the default and not the production #2081
cutover** (that cutover stays deferred) — but it is *reachable today*, not
*unreachable*.

Under enforce mode the stored `entity_id` becomes the membership/authority
*target* the gate evaluates against. Changing a treasury from `entity_id: None` to
`Some(entity_id)` therefore changes *which entity's* membership graph an enforced
request is checked against — and an apply could already trigger that change in any
process running the enforce-mode env var. That is an authorization-relevant
mutation, so apply must be explicit, controlled, reviewable, auditable, and gated —
never a silent or implicit side effect of a report or any other command.

### 2. Authority doctrine (unchanged, restated as a hard invariant)

A `coop_id ↔ EntityId` mapping grants **zero** authority. It is a reversible name
binding that identifies *which entity* an identifier denotes; it never says what a
caller may do. A trusted binding is a **target verifier**, not a permission.
Authorization continues to derive from membership, roles, capabilities, mandates,
charters, governance decisions, receipts, and execution evidence. Apply populates
an identity target; it grants nothing.

### 3. Preconditions for any future apply

- A report / dry-run MUST be available and is the default; apply is never the
  default action.
- Apply MUST operate **only** on `WouldPopulate` rows from
  `TreasuryManager::plan_entity_id_backfill`. Every fail-closed class the planner
  already encodes MUST be skipped, never coerced:
  `SkippedAlreadyHasEntityId`, `SkippedNoMapping`, `SkippedUntrustedProvenance`,
  `SkippedNonCooperativeEntity`, `SkippedAmbiguousBinding`, `SkippedStorageError`.
- Apply MUST require **trusted provenance** as defined by the canonical predicate
  `CoopEntityBindingProvenance::is_trusted_for_resolution()`
  (`icn/crates/icn-entity/src/coop_entity_map.rs`): `Activation`,
  `OperatorBackfill`, `Surrogate`, `GovernanceReceipt { receipt_id }`.
- Apply MUST **never** trust `UnknownLegacy` (pre-provenance, unrecorded, or an
  unrecognized future variant — also the `serde(other)` fail-closed catch-all).
- Apply MUST NOT create a missing store as a side effect (mirrors #2262).
- Apply MUST NOT normalize a `coop_id` (no `coop_A → coop-a`); the original
  `coop_id` is preserved byte-for-byte, exactly as bound.

### 4. Operator confirmation contract

- The default of any apply-capable command MUST be read-only / dry-run / report.
- A mutation MUST require an explicit `--apply` flag.
- When the plan contains any `would_populate > 0`, apply SHOULD require a second,
  unambiguous confirmation (a distinct flag or an exact confirmation phrase) so a
  mutation cannot be a single-flag accident.
- Human output MUST state plainly that a mutation will occur / occurred and how
  many rows it affects; JSON output MUST be machine-auditable.
- Apply MUST emit enough plan detail (the same shape as the report, before and
  after, or an explicit per-row result list) to compare intended vs applied.
- No hidden mutation on a missing store; an absent store is reported, not created.

### 5. Apply transaction and failure contract

- Apply mutates **only** the `entity_id` field of eligible treasury rows. It MUST
  NOT mutate the `coop_id ↔ EntityId` map in this rung, and MUST NOT touch
  membership, roles, capabilities, auth configuration, or enforcement mode.
- Apply MUST be **idempotent**: a re-run over already-populated rows is a no-op
  (those rows classify `SkippedAlreadyHasEntityId`).
- Partial-failure behavior MUST be explicit. Either all eligible writes are
  transactional (all-or-nothing), or per-row results are reported with clear
  partial-success semantics that name exactly which rows changed.
- Storage errors MUST fail closed.

### 6. Reversibility, rollback, and audit expectations

- Apply output MUST include a per-row before/after (`coop_id`, `treasury_did`,
  `entity_id: None → Some(...)`, provenance, result).
- A durable audit artifact SHOULD be produced if a repo-native pattern fits at
  implementation time; this ADR does not mandate a specific schema.
- Rollback is **out of scope** for this contract and MUST NOT be implicit or
  automatic. Because the binding is reversible and the pre-state (`None`) is
  recorded in the apply output, a future rollback can be specified separately; it
  is not authorized here.

### 7. Relationship to #2081

The #2081 treasury enforcement cutover remains **blocked** and is untouched by
this ADR. This contract does not flip enforcement, does not make existing
observations clean, and does not assert that the mapping/observation evidence is
sufficient for enforcement. Enforcement may be *considered* only after a
controlled apply built to this contract exists, has produced evidence, and its
observation metrics are understood — a later, separate decision.

## Non-goals

- No code. No `--apply`. No mutation. No new schema (a later implementation PR may
  justify one on its own merits).
- No enforcement-default change; no gateway behavior change; no #2081 cutover.
- Does not close #2082 (the lane stays open).
- No token / payment / wallet / balance / currency framing; ICN is democratic
  institutional infrastructure / a Coordination OS, and this is an identity-target
  binding for settlement-constraint authorization, not an economic instrument.
- No production-readiness, pilot-readiness, live-federation, or
  security-completion claim.

## Validation checklist for the later implementation PR

When apply is built to this contract, it MUST demonstrate:

- report/dry-run available and is the default; dry-run writes nothing;
- apply writes only `WouldPopulate` rows;
- idempotent re-run is a no-op;
- no store creation on a missing store;
- untrusted (`UnknownLegacy`) provenance skipped;
- ambiguous binding skipped;
- already-populated skipped;
- non-cooperative entity skipped;
- storage error fails closed;
- `coop_id` preserved byte-for-byte (no normalization);
- enforcement mode unchanged; #2081 untouched.

## Consequences

- The mutating rung now has a fixed safety envelope before any code exists, so the
  implementation PR is a conformance exercise, not a design negotiation.
- The fail-closed planner taxonomy (#2258) and the read-only report (#2262) are
  promoted from convenience to **contract**: apply is defined entirely in terms of
  the planner's `WouldPopulate` set and the canonical provenance-trust predicate.
- The doctrine that mapping grants zero authority is preserved end to end: apply
  binds an identity target, never a permission, and enforcement stays a separate,
  later, deferred decision (#2081).
- Cost: one more decision rung before mutation. Accepted deliberately — populating
  an authorization-relevant field warrants a written contract first.
