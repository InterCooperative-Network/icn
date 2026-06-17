---
id: "0035"
title: "Entity-aware request authorization (require_entity_access) — primitive + treasury observe"
status: "accepted"
date: "2026-06-17"
deciders: ["Matt Faherty"]
tags: ["authz", "gateway", "rfc-0018", "treasury", "meaning-firewall", "security"]
supersedes: []
superseded_by: []
amends: []
implementation_status: "partial"
references:
  - "docs/rfcs/RFC-0018-entity-aware-request-authorization.md (implements first slice)"
  - "ADR-0019-authority-grant-minting-and-mandate-persistence-seam.md (authority basis seam)"
  - "docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md (a token is not a mandate)"
  - "icn/crates/icn-gateway/src/authority.rs (require_entity_access)"
  - "icn/crates/icn-gateway/src/api/treasury.rs (observe-mode wiring)"
  - "GitHub #2061 (spec), #2074 (RFC-0018), #2075/#2077 (self-asserted coop fix)"
---

# ADR-0035: Entity-aware request authorization (require_entity_access) — primitive + treasury observe

## Status

`accepted` (decision). `implementation_status: partial` — the primitive and treasury
*observation* have landed; treasury *enforcement* is a tracked follow-up.

## Context

Gateway request authorization is the flat `require_coop_access` guard
(`middleware.rs`: `claims.coop_id == path_coop_id`) at ~38 call sites — blind to
entity type, membership, and role. PR #2077 closed the self-asserted-`coop_id`
issuance weakness (#2075) issuance-side, but did not make authorization
entity-aware. RFC-0018 (#2074, spec #2061) specifies the move to
`require_entity_access(caller, target, action)`.

The entity model already exists (`icn-entity`: `EntityId`, `Membership`,
`MembershipRole`, `MembershipCapability`) and the gateway already does
membership-based authz in one place — `require_entity_write_access`
(`api/entity.rs`), which resolves `get_members(target)` and checks the caller's
role. The gap is a reusable primitive and a measured migration path that does not
risk breaking live authorization while membership data is still being backfilled.

## Decision

1. **Introduce `require_entity_access(entity_mgr, caller, target, action)`** in
   `authority.rs`, generalizing `require_entity_write_access`. It resolves the
   caller's membership in `target` and checks an action-specific requirement.

2. **Canonical authz subject/target** = `(caller individual EntityId, target
   EntityId)`. The caller `EntityId` is derived **at the guard** from the token's
   `claims.sub` (DID) via `EntityId::from_did` — it is **not** carried as an
   authority claim in the token. Rationale: a stored authority claim would
   recreate the #2075 failure mode (trusting a self-asserted token field) in a
   shinier outfit. Authority is resolved against the membership graph at request
   time. ("Claim vs lookup" — we choose lookup; revisit only with a trusted
   issuance path.)

3. **Action → authority basis (intentionally heterogeneous):**

   | `EntityAction` | Requirement | Why |
   |---|---|---|
   | `ModifyEntity` | role ∈ `{Founder, BoardMember}`, **no active-standing gate** | preserves `require_entity_write_access` behavior *exactly* |
   | `TreasuryWrite` | active membership **with `TreasuryAccess` capability** | capability, not role-name; Founder/BoardMember/Officer hold it via `default_capabilities`, a plain member only if explicitly granted |
   | `TreasuryRead` | active membership (any role) | broad for observe mode |

   `EntityAction` stays minimal (three variants). New variants are added only when
   a real endpoint family needs them.

4. **Treasury is wired in OBSERVE mode, not enforcement.** Each of the 10 treasury
   handlers, *after* the authoritative flat `require_coop_access` has passed and
   the treasury is loaded, computes an `EntityAccessObservation` and records it via
   `entity_authz_observation_total{family,action,result,reason}`. The entity path
   **never denies** in this slice; `require_coop_access` remains the sole enforced
   gate. Observation runs only after flat success (the flat guard early-returns on
   denial), so the metric answers: *of currently-allowed treasury requests, how
   often would the entity path also allow?*

5. **`coop_id → EntityId` projection is fallback-only.** Treasury already stores
   its `EntityId` (`treasury.entity_id()`), so that is the authoritative target.
   `legacy_coop_id_to_entity_id_fallback` (a thin wrapper over
   `EntityId::cooperative`, which rejects rather than normalizes) is used only when
   `entity_id()` is `None`. The canonical, persisted, reversible `coop_id ↔
   EntityId` mapping/backfill is deferred (tracked follow-up).

## Consequences

- **Zero live authorization change.** No flat guard is removed or weakened; the
  entity path is observation-only. The risk of denying legitimate treasury calls
  (e.g. coops not yet entity-registered) is avoided by design.
- The primitive gains one *live* caller immediately: `require_entity_write_access`
  delegates to it (`ModifyEntity`), proving it against the existing entity-write
  path without behavior change.
- The observation metric's `reason` taxonomy (`missing_treasury_entity_id`,
  `no_memberships`, `non_member`, `missing_capability`, …) becomes the evidence
  that gates the future enforcement cutover.
- Heterogeneous per-action bases (role for `ModifyEntity`, capability for
  `TreasuryWrite`) are deliberate; an implementer must not "simplify" them into a
  single role check — that would either weaken treasury or tighten entity-write.
- Meaning Firewall preserved: the gateway translates the institutional action into
  a generic membership/capability requirement; no domain (`trust`/`governance`/
  `ccl`/`coop`/`community`) crate is imported, and the kernel sees nothing.

## Implementation status

`partial`. Proven in this slice:

- `require_entity_access` + `EntityAction` with a 7-case decision matrix
  (`authority.rs` tests): Founder/BoardMember allow; active member reads but cannot
  write; capability-beats-role; non-member deny; suspended deny (treasury);
  `ModifyEntity` role-only preserved.
- `require_entity_write_access` delegates to the primitive; existing entity-write
  tests stay green.
- Treasury observe-mode wiring + an "entity-deny does not deny the request" test.
- `legacy_coop_id_to_entity_id_fallback` reject-not-normalize tests.

Not implemented (tracked follow-ups):

- Treasury enforcement cutover (gated on clean observation metrics + membership
  backfill; possible `TreasuryRead`/`TreasurySensitiveRead` split).
- `entity_id` token claim (only if/when a trusted issuance path exists).
- Canonical persisted `coop_id ↔ EntityId` mapping/backfill.
- Other endpoint families; delegation/federation/community cross-entity authority
  (RFC-0018 Step 5).

## Alternatives Considered

| Alternative | Why rejected |
|-------------|-------------|
| Enforce the entity guard on treasury now (dual-guard, both deny) | Membership data is only populated for entity-registered coops; enforcing would 403 legitimate calls and violate the doctrine's fail-closed-only-on-wired-resolver rule (ABUSE_CASE_HARDENING §4.3). Observe first, enforce on evidence. |
| Carry an `entity_id`/authority claim in the JWT | Recreates the #2075 self-asserted-claim trust problem. Resolve at the guard instead. |
| Single role-based check for all actions | Either weakens treasury (role ≠ treasury authority) or tightens entity-write (breaks behavior parity). Per-action bases are the point. |
| Build the canonical `coop_id ↔ EntityId` backfill now | Unnecessary for treasury (it stores `entity_id`); it is the RFC's heavy gating sub-task and belongs with the enforcement cutover, not the primitive. |
| Lowercase-normalize `coop_id` into a slug | Lossy: `coop_A` and `coop-a` collide → identity soup. Reject non-mappable ids instead. |

---

## Notes

This ADR records the first RFC-0018 implementation slice. The RFC remains the
design of record for the full migration; this ADR fixes the concrete decisions the
primitive and treasury-observe wiring bake in.
