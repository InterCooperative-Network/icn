# `CreateTreasury` — Treasury `entity_id` Trust Semantics

**Status**: draft — design / audit (trust-semantics definition, not implementation)
**Truth class**: descriptive
**Canonical**: no — current implementation truth lives in [docs/STATE.md](../STATE.md) and [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md)
**Last Reviewed**: 2026-07-01
**Source basis**: read against `main` @ `d37d7d43` (#2267). Code anchors/line numbers were verified at that commit — re-verify before relying on exact numbers.
**Related**: [coop-id-entity-resolver.md](coop-id-entity-resolver.md) (resolver seam) · [ADR-0084](../adr/ADR-0084-controlled-treasury-entity-id-backfill-apply-contract.md) (controlled apply contract) · [ADR-0035](../adr/ADR-0035-entity-aware-request-authorization.md) · [RFC-0018](../rfcs/RFC-0018-entity-aware-request-authorization.md) · issues #2082 (this lane), #2081 (blocked, untouched), #2080 (separate, untouched)

> **This document defines trust semantics for a path that must not be casually mutated.**
> It answers what the `CreateTreasury` message path is, what authority it carries today,
> why it must not populate treasury `entity_id` by projection, and what single future
> implementation slice would be safe. It changes **no** code behavior. A
> `coop_id ↔ EntityId` mapping binds an identity target only — it grants **zero**
> membership, role, capability, mandate, standing, or permission.

## 1. Current behavior (verified at `d37d7d43`)

`CreateTreasury` exists twice, as near-identical duplicates on two parallel actor paths:

| Path | Message | Handler |
|---|---|---|
| `icn/crates/icn-coop/src/actor.rs` | `CoopMessage::CreateTreasury` (~L226, dispatch ~L409) | `handle_create_treasury` (~L732) |
| `icn/apps/membership/src/coop_core/actor.rs` | `CoopMessage::CreateTreasury` (~L122, dispatch ~L288) | `handle_create_treasury` (~L484) |

Both handlers do exactly this:

1. Verify the cooperative exists in the store (`get_cooperative`).
2. Reject if `coop.treasury_did` is already set (`"already has a treasury"`).
3. Derive the treasury DID deterministically from the flat `coop_id`
   (`derive_treasury_did` / `derive_treasury_anchor`).
4. Assign the treasury to the cooperative and, if a `TreasuryManager` is wired,
   register the treasury in the ledger via the **plain** `register_treasury(...)` —
   i.e. **`entity_id: None`**, with a default settlement unit and the first listed
   member as `created_by`.
5. Save and announce.

What the handlers do **not** do:

- **No `CoopEntityMap` interaction of any kind** — no read, no bind, no provenance.
- **No lifecycle-state gate** — only "exists" and "no treasury yet"; a merely
  proposed (non-activated) cooperative qualifies.
- **No authority gate at the actor layer** — the caller is whatever holds the actor
  handle. There is **no production caller today**: no gateway route, RPC method, or
  binary invokes `create_treasury` (verified by repo-wide search at `d37d7d43`);
  it is reachable only through the actor handle API and is exercised by tests.
- **No overlap with activation**: since #2266, cooperative activation creates the
  treasury itself (register `entity_id: None` → commit activation → record the
  `Activation`-provenance map binding **last** → populate the treasury `entity_id`
  from that recorded binding; when **no map is wired**, activation instead uses the
  pure reject-not-normalize projection — `icn-coop/src/actor.rs` ~L587-591 — because
  there is no map to disagree with). A coop that went through activation therefore already
  has a treasury, and `CreateTreasury` is **rejected** for it (tests pin this on both
  paths). `CreateTreasury` consequently fires only for cooperatives that have **not**
  been activated — exactly the population for which **no trusted binding exists**.

### 1.1 Which institutional act is this?

None, currently. Activation is the institutional act that owns treasury creation and
the only act that may record `Activation` provenance ("only the node that
authoritatively activated may write `Activation`" — the trust assertion documented at
`icn-coop/src/actor.rs` ~L79). `CreateTreasury` as wired today is **library/test
scaffolding**: a handle-level surface with no authority basis, no governance receipt,
and no accountable origin for any identity claim it might make. That is precisely why
it must stay `entity_id: None` until this document's semantics are implemented.

### 1.2 The sibling gap (out of scope here, recorded for #2082)

`apps/membership` `coop_core` has **no** `CoopEntityMap` integration at all — its
activation path does not record an `Activation` binding either (verified: zero
matches for `CoopEntityMap` / `bind_coop_entity_map` / `register_treasury_with_entity`
under `icn/apps/membership/` at `d37d7d43`). That is #2082 gap 12b, a separate
activation-parity rung; this document only notes that any `CreateTreasury` slice must
either land on **both** actor copies or explicitly sequence them.

## 2. Problem

A treasury row's `entity_id` is authorization-relevant under the (off-by-default)
`enforce-trusted-resolver` mode: it is the membership *target* the entity-auth gate
verifies against. Populating it from an unaccountable source would let an unwired,
authority-free scaffolding path mint the identity target that a future enforcement
mode trusts. Two failure shapes matter:

- **Projection without provenance.** `project_coop_id` is pure and reject-not-
  normalize, but a projection performed by `CreateTreasury` carries no accountable
  origin. #2266's review (P2#1) established the rule for the **map-wired** activation
  path: treasury `entity_id` may be populated **only from the successfully recorded
  binding**, never from a bare projection racing ahead of the bind. (The **no-map**
  activation path does populate from pure projection — `icn-coop/src/actor.rs`
  ~L587-591 — which is tolerable *there* only because activation is the authoritative
  institutional act and there is no map for the projection to disagree with.)
  `CreateTreasury` is not that act and owns no binding, so this document deliberately
  holds it to the **stricter** rule: no projection fallback at all — see §5.
- **Silent trust creation.** If `CreateTreasury` wrote a map binding itself, that
  binding's provenance would be a lie — the path is not activation, not an operator
  backfill, not a surrogate allocation, and holds no governance receipt. Every
  trusted `CoopEntityBindingProvenance` variant records a concrete accountable
  origin this path does not have.

## 3. Non-goals

- No change to `CreateTreasury` behavior in this document or its PR.
- No treasury mutation, no map mutation, no provenance write.
- No new authority gate design for treasury creation itself (who may create a
  treasury is an institutional-act question for a future governance slice; this
  document only pins the *identity-target* semantics).
- No gateway enforcement, route-default, or token-issuance change (#2081 and #2080
  remain untouched and blocked/separate respectively).
- No #2082 closure.

## 4. Trust sources available / not available to this path

| Source | Available? | Why / why not |
|---|---|---|
| `Activation` provenance | **No** | `CreateTreasury` is not activation and must never impersonate it. |
| `OperatorBackfill` provenance | **No** | It is not an operator-run backfill command. |
| `Surrogate` provenance | **No** | It performs no surrogate allocation. |
| `GovernanceReceipt` provenance | **No** (today) | No governance decision flows into this path. |
| **Reading an existing trusted binding** | **Yes** | `binding_for_coop` + `is_trusted_for_resolution()` + reverse-index agreement + cooperative well-formedness — the same fail-closed discipline the ADR-0084 planner (`WouldPopulate`) and #2266 activation-populate already use. |

The only legitimate trust relationship this path can ever have with the map is
**read-only consumption of a binding somebody accountable already recorded.**

## 5. Options considered

1. **Reject projectable `coop_id`s without a trusted binding.** Rejected: couples
   treasury *existence* to mapping state, which is authority-adjacent creep — the
   mapping would start gating what an institution can do, violating zero-authority.
   Also changes behavior on a surface with no production caller, for no safety gain
   (`entity_id` stays `None` either way).
2. **Require an existing trusted map binding as a creation precondition.** Rejected
   for the same coupling reason: a legacy cooperative without a binding could no
   longer receive a treasury at all.
3. **Create a pending/untrusted `entity_id` target.** Rejected: violates the
   fail-closed doctrine. ADR-0084 §3 forbids trusting `UnknownLegacy`; a
   "provisional" treasury `entity_id` is indistinguishable from a trusted one to
   every consumer that reads the row. There is no such thing as a safely-untrusted
   populated target.
4. **Remain `entity_id: None`; let the evidence-bearing backfill path handle it.**
   **Current state, and the correct default.** The ADR-0084 planner/report/apply
   chain already covers rows created this way, fail-closed.

**Recommended future slice (the only mutation later allowed):** option 4 **plus
read-only trusted-binding consultation** — at `CreateTreasury` time, if (and only
if) a trusted, structurally consistent binding already exists for the byte-exact
`coop_id` (trusted provenance via `is_trusted_for_resolution()`, reverse index
agrees byte-for-byte, target is a well-formed cooperative `EntityId`), populate the
treasury's `entity_id` from that binding; otherwise `entity_id: None` exactly as
today. The map is **never written**, no provenance is recorded, and a
missing/untrusted/ambiguous/malformed binding degrades silently to today's
behavior. This mirrors #2266's populate-from-recorded-binding and ADR-0084's
re-verification discipline, and leaves rows without bindings to the existing
backfill chain.

**Registration path constraint (coop_id preservation).** The implementation MUST
NOT use `TreasuryManager::register_treasury_with_entity` for this: that API derives
the treasury row's `coop_id` from `entity_id.identifier()`
(`icn-ledger/src/treasury.rs` ~L400-408), so for a trusted **surrogate** (or any
binding where `entity_id.identifier() != coop_id`) it would file the treasury under
the surrogate slug instead of the original legacy `coop_id` — breaking byte-for-byte
preservation and `get_treasury_by_coop`/reverse-audit consistency. The slice must
instead use the **two-step path activation already uses**: plain
`register_treasury(...)` with the byte-exact original `coop_id`, then populate
`entity_id` from the trusted binding through the existing fail-closed populate seam
(the `populate_entity_id_at_activation`-class primitive: byte-for-byte `coop_id`
re-check + entity-uniqueness guard), or an equivalent new `coop_id`-preserving
registration helper with the same checks.

Note the deliberate divergence from activation: the slice has **no no-map projection
fallback**. Activation may project when no map is wired because it is the
authoritative institutional act; `CreateTreasury` is not, so with no map — or no
trusted binding — the result is `entity_id: None`, full stop (required test 5).

## 6. Required tests before any implementation PR may mutate `entity_id`

An implementation PR is acceptable only with all of the following, on **both** actor
copies (or with the membership copy explicitly sequenced as a follow-up in the same
lane):

1. Trusted binding (each of `Activation`, `OperatorBackfill`, `Surrogate`,
   `GovernanceReceipt`) → treasury registered with that binding's `EntityId`;
   `coop_id` preserved byte-for-byte. **Must include a surrogate case where
   `entity_id.identifier() != coop_id`**: the treasury row keeps the original
   legacy `coop_id` (and `get_treasury_by_coop(legacy_id)` still resolves), with
   `entity_id` set to the surrogate — proving the registration path is
   `coop_id`-preserving (see §5's registration-path constraint).
2. `UnknownLegacy` / missing provenance → `entity_id: None` (fail-closed, no error).
3. Reverse-index mismatch or one-sided binding → `entity_id: None`.
4. Non-cooperative or malformed target → `entity_id: None`.
5. No map configured → `entity_id: None` (no store created).
6. **No map write in any branch** (a write-through test double must prove no
   `bind_*` call occurs).
7. Post-activation `CreateTreasury` rejection unchanged.
8. Entity-id uniqueness conflict (binding's `EntityId` already owned by another
   treasury) → fail-closed error, no partial write (the #2265 `EntityIdConflict`
   discipline).

## 7. Explicit non-claims

This document and its PR claim **none** of the following: any code behavior change;
any treasury, map, or provenance mutation; any enforcement or default route-outcome
change; any positive `entity_id`/`entity_type` token issuance; #2081 progress or
readiness; #2080 progress; #2082 closure; mapping-as-authority (a binding never
grants membership, role, capability, mandate, standing, or permission); trust of
`UnknownLegacy` (it remains untrusted unless a future evidence-bearing workflow
repairs it); production, pilot, member, organizer, or live-federation readiness;
Phase 2 completion.

## 8. Validation

- Frontmatter + `docs/registry.toml` entry + `docs/INDEX.md` link added per
  [docs-control-map — Adding a new doc](../reference/project-index/docs-control-map.md).
- `python3 docs/scripts/doc_control_check.py` run locally before commit.
- Vocabulary: settlement/allocation language only; no prohibited terms.
