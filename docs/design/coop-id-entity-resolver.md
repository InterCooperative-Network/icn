# Governed `coop_id → EntityId` Resolver — Design Seam

**Status**: draft — design / seam definition (migration orientation, not implementation)
**Truth class**: descriptive
**Canonical**: no — current implementation truth lives in [docs/STATE.md](../STATE.md) and [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md)
**Last Reviewed**: 2026-06-25
**Source basis**: seam read against `main` @ `a77c3324` (#2183); §7 implementation-status note re-verified against `main` @ `0ef541c5` (#2197) on 2026-06-25. Code anchors/line numbers were last verified at `a77c3324` — re-verify before relying on exact numbers.
**Related**: [entity-aware-auth-control-map.md](entity-aware-auth-control-map.md) (the keystone this extends) · [RFC-0018](../rfcs/RFC-0018-entity-aware-request-authorization.md) (active) · [ADR-0035](../adr/ADR-0035-entity-aware-request-authorization.md) (accepted, partial) · [ABUSE_CASE_HARDENING_STRATEGY.md](../architecture/ABUSE_CASE_HARDENING_STRATEGY.md) · issues #2061, #2080, #1868, #2082

> **This document defines a seam — a contract, its invariants, failure modes, and migration
> posture — for the governed `coop_id → EntityId` resolver named as the keystone in the
> [entity-aware authorization control map](entity-aware-auth-control-map.md). It implements
> nothing, changes no runtime authorization, and authorizes no behavior change. It exists so
> the resolver is designed once, fail-closed, before any code reads from it.**

---

## Purpose

The [control map](entity-aware-auth-control-map.md) established that three in-flight seams —
route enforcement, trusted token issuance, and the token `entity_id` claim — all converge on a
single missing piece: a **trusted, governed resolver from the legacy flat `coop_id` namespace
to the typed `EntityId` model**. This document specifies that resolver as a seam.

The load-bearing finding that shapes the whole design: the *mapping store already exists*. What
is missing is not a store but a **governed, provenance-aware, fail-closed read path** that the
authorization and issuance code can trust. The resolver is the route/observe-side analogue of
the issuance-side [`TokenAuthoritySource`](#3-authority-and-governance) seam, and is designed to
read the **same** membership-grounded authority once wired.

## What already exists (do not reinvent)

| Piece | Where | What it is | What it is **not** |
|---|---|---|---|
| `CoopEntityMap` (store) | `icn-entity/src/coop_entity_map.rs` (#2082) | canonical, persisted, **reversible** `coop_id ↔ EntityId` name binding; conflict-detecting both directions; `bind_resolved` / `bind_exact` / `bind_projected` / `entity_for_coop` / `coop_for_entity` | **not authority** — its own module doc states a binding "grants zero standing, role, capability, membership, mandate, or permission" |
| `project_coop_id` | `coop_entity_map.rs` | pure, reject-not-normalize projection (valid cooperative slug only) | not a governed source; rejects non-mappable ids |
| `propose_surrogate_entity_id` | `icn-entity/src/coop_entity_surrogate.rs` | deterministic `coop-legacy-<20 hex>` surrogate for non-mappable ids (domain `icn:coop-entity-surrogate:v1`) | not a binding; a *proposal* |
| activation-time wiring | `icn-core/src/supervisor/init_notifications.rs` + `coop_entity_map_wiring` test | binds through the wired map at activation | not consulted by the gateway |
| `legacy_coop_id_to_entity_id_fallback` | `icn-gateway/src/entity_map.rs` | best-effort observe-only projection bridge | explicitly **not** a governed authority source |

The decisive gap: **`icn-gateway` does not consume `CoopEntityMap` at all.** The store lives in
`icn-entity`; the gateway's only bridge is the lossy, observe-only fallback. Nothing carries the
persisted binding into `require_entity_access`, the treasury observation, or token issuance with
provenance and fail-closed authority semantics. That bridge is the resolver seam.

## 1. Problem statement

- **Flat `coop_id` guards are the current enforced baseline.** `require_coop_access`
  (`icn-gateway/src/middleware.rs:138`) is the sole authorization gate on the bulk of mutation
  routes. It is coarse but real, and must remain in force until a measured typed replacement exists.
- **Entity-aware authorization exists conceptually but cannot cut over.** `require_entity_access`
  (`icn-gateway/src/authority.rs:160`) is enforced only on entity routes and observe-only on
  treasury. Extending it requires a **trustworthy** mapping from a request's legacy `coop_id`
  context to a governed `EntityId` — one that carries *where the binding came from*, not just
  *what it denotes*.
- **A DID proves key control, not membership.** `TokenClaims.sub` proves the caller controls a
  DID. It says nothing institutional.
- **Token scope is not authority.** `scopes` (e.g. `governance:write`) is technical/capability
  permission, not a mandate or standing.
- **`coop_id` is not an entity-authority primitive by itself.** A flat namespace string match is
  not entity-aware authorization, and the optional `entity_id` / `entity_type` token claims
  (`icn-gateway/src/auth.rs`) are deliberately **non-enforcing** context that the flat guard ignores.
- **A mapping is not authority.** Even the canonical `CoopEntityMap` binding is a name binding
  only. Resolving `coop_id → EntityId` says *which* entity an identifier denotes — never *what the
  caller may do*. Authority still flows from memberships, standing, charters, mandates, and
  decision receipts.

The resolver's job is to turn an *untrusted* legacy `coop_id` context into a *trusted, typed,
provenance-tagged* `EntityId` resolution — or to fail closed — so the authority layers above it
can reason in the typed model without inheriting the legacy namespace's ambiguity.

## 2. Resolver contract

A single seam, shaped after the by-value, `#[must_use]` decision discipline already used by
`TokenAuthoritySource` (`icn-gateway/src/token_authority.rs:147`).

**Inputs**
- the request's flat `coop_id` context (route `coop_id` and/or `TokenClaims.coop_id`);
- the subject `Did` (`TokenClaims.sub`);
- the optional, non-enforcing `entity_id` / `entity_type` claim, used only as a **cross-check**, never as the source of truth;
- the **purpose** of the call: `Observe`, `Enforce`, or `Issue` (the trust bar rises across these).

**Output** — a by-value resolution decision (no `Result` the caller can paper over), e.g.:
- `Resolved { entity_id, provenance }` — a trusted binding, tagged with *how* it was established;
- `NotMapped` — no binding exists for this `coop_id`;
- `Ambiguous { detail }` — more than one candidate, or a forward/reverse conflict;
- `Untrusted { reason }` — a binding exists but its provenance is not trusted for this purpose (stale, surrogate-only, revoked, subject/type mismatch);
- `Error { detail }` — the backing store could not answer.

**Trusted data sources** — the governed `CoopEntityMap` (`entity_for_coop` / `coop_for_entity`)
plus per-binding provenance metadata, and (in later slices) the same `icn-entity` membership /
`icn-governance` standing surfaces that issuance will read. **Never** client-supplied.

**Required / optional / observe-only during migration** — **observe-only**. During migration the
resolver result is *data*, never a `Result` that denies a request (the same structural property
the treasury `EntityAccessObservation` already has at `authority.rs:263`). It becomes decisive
only at the enforce/issue cutover, and only behind a wired trusted source.

**Ambiguity** is represented explicitly as `Ambiguous`, never silently resolved to a "best"
candidate. **Missing mappings** are `NotMapped`, never a fabricated or projected identity at the
authority layer. **Stale / conflicting** bindings resolve to `Untrusted` / `Ambiguous` and **fail
closed** — they never widen access.

## 3. Authority and governance

- **Who may create, update, or retire a mapping.** Writes go through the governed binding path
  only — `CoopEntityMap::bind_resolved` / `bind_exact` (which already enforce both-direction
  conflict detection and cooperative-type validation, refusing one-sided writes). Permitted
  origins: activation-time binding, operator-run backfill with recorded provenance, and
  (eventually) a binding carrying a governance **decision receipt**. Retirement is an explicit
  governed transition, not a silent overwrite.
- **What evidence / receipt / provenance is required.** Every binding the resolver will *trust*
  must carry a provenance class (e.g. `activation`, `operator-backfill`, `surrogate`,
  `governance-receipt`). The resolver trusts only the provenance classes appropriate to the
  purpose — e.g. a `surrogate`-only binding may be observable but is not, by itself, an authority
  basis for `Enforce` / `Issue`.
- **Why this cannot be client-supplied.** The token `entity_id` claim is non-enforcing context set
  only by a trusted issuance path; a caller asserting its own `coop_id → EntityId` binding is
  self-asserted authority — the exact pattern the `/auth/verify` fail-closed doctrine (#2075,
  RFC-0018) forbids. DID key control alone never authorizes a cooperative.
- **Why this cannot be inferred from token claims alone.** Scope is permission, not authority; the
  `entity_id` claim is a cross-check, not a source. Inference from claims would let a token mint its
  own institutional meaning.
- **Relationship to `TokenAuthoritySource` / `DenyUntilWired`.** The resolver mirrors that seam
  exactly. Issuance asks "may this subject receive a token binding `coop_id`/`entity_id`?" and the
  shipped `DenyUntilWired` (`token_authority.rs:166`) answers `Deny { NoTrustedSourceWired }`,
  reading none of its inputs. The resolver asks "what trusted `EntityId` does this `coop_id`
  context denote, for this purpose?" and its shipped default must **resolve nothing** until wired.
  Both are fail-closed-by-value, and both are designed to read the **same** membership-grounded
  source once it exists — so the route side and the issuance side converge rather than fork. The
  `IssuanceAuthorityBasis::Membership` variant (`token_authority.rs:83`) names that shared source.

## 4. Migration posture

1. **Current baseline — `require_coop_access`.** The flat guard stays the enforced gate. The
   resolver does not touch it.
2. **Observe mode.** The resolver runs *alongside* the flat guard and records, per request, what a
   trusted typed resolution *would* yield vs the flat decision — extending the proven treasury
   observe pattern (`observe_treasury_entity_access`, `authority.rs:301`) from the membership check
   to the *resolution* step. It emits a metric + log line and denies nothing. This measures
   `NotMapped` / `Ambiguous` / `Untrusted` rates on real traffic before any enforcement.
3. **Later enforcing mode.** `require_entity_access` can become decisive for a route family only
   after (a) the resolver's trusted source is wired, and (b) observe-mode divergence for that
   family is understood and acceptable. One route family at a time.
4. **Compatibility with `governance:write` decomposition (#1868).** Orthogonal and complementary:
   decomposing a broad scope into per-action capabilities shrinks blast radius but is a *scope*
   concern, not an *authority-resolution* concern. The resolver neither blocks nor depends on it.

## 5. Fail-closed rules

The resolver never fails open. In `Observe` a non-`Resolved` outcome is recorded and the flat
guard decides; in `Enforce` / `Issue` a non-`Resolved` outcome **denies**.

| Condition | Resolution | Enforce/Issue effect |
|---|---|---|
| No mapping for `coop_id` | `NotMapped` | deny (no typed target → no entity authority) |
| Multiple / forward-reverse-conflicting mappings | `Ambiguous` | deny (never pick a "best" candidate) |
| Revoked / retired entity | `Untrusted { revoked }` | deny |
| Store / backend failure | `Error` | deny (an unreachable source is not an allow) |
| Token subject ↔ binding mismatch | `Untrusted { subject_mismatch }` | deny |
| Entity-type mismatch (binding not cooperative) | rejected at bind (`CoopEntityMapError::InvalidEntityType`); resolver treats any non-cooperative read as `Untrusted` | deny |
| Stale cache or unverifiable provenance | `Untrusted { stale_or_unverifiable }` | deny |

These mirror the existing observation vocabulary (`DenyReason`, `IndeterminateReason` in
`authority.rs`) and the issuance denial reasons (`IssuanceDenialReason`, `token_authority.rs:101`)
so logs/metrics stay coherent across the three seams.

## 6. Non-claims

This document explicitly does **not**:

- implement the resolver, any trait, or any wiring;
- change runtime authorization, gateway behavior, or any route guard;
- issue positive trusted entity claims, or alter token issuance / minting;
- remove or weaken `require_coop_access` (it remains the enforced baseline);
- enforce `require_entity_access` on any new route family;
- complete or close #2061, #2080, #1868, or #2082;
- claim production, pilot, live-federation, NYCN, or complete-API readiness.

It is a seam definition for migration, not an implementation.

## 7. Follow-up implementation slices

Narrow and sequenced; each is its own PR, behind the fail-closed default until explicitly cut over.

> **Implementation status (as of 2026-06-25, `origin/main` `0ef541c5`).** A2a–A2d have landed, all **observe-only / fail-closed**; the canonical current-state record is [docs/STATE.md](../STATE.md) and [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md), not this design doc.
> - **A2a** ✅ — `CoopEntityResolver` trait + fail-closed `UnwiredCoopEntityResolver` default (#2188).
> - **A2b** ✅ — observe-only treasury classification (#2189), no route outcome change.
> - **A2c** ✅ — persisted provenance substrate (#2190) + fail-closed `StoreBackedCoopEntityResolver` source (#2192) + trusted provenance producers for local activation (#2193) and operator backfill (#2194), then wired into treasury observe-mode (#2196, resolver result discarded — no route outcome change). Trusts only `Activation`/`OperatorBackfill`/`Surrogate`/`GovernanceReceipt`; `UnknownLegacy`, gossip/unprovenanced, missing, ambiguous, entity-type-mismatch, and backend-error all fail closed.
> - **A2d** ✅ — treasury **observe → measure → gate** scaffold (#2197): `TreasuryEntityAuthMode { ObserveOnly (default), EnforceTrustedResolver }` + pure `decide_treasury_gate(...)` in `icn-gateway/src/authority.rs`, shipped **observe-only** (active mode `ObserveOnly`; default route outcomes byte-identical to the flat guard). `EnforceTrustedResolver` is **decision-only, wired to no route or config**, and currently **fails closed on every resolution** (`ResolverOnly` → `ResolverOnlyTargetUnverified`; the `Agree` would-allow path → `AgreeTargetUnverified`) because membership is evaluated against `treasury.entity_id()` / the legacy projection, **not** the resolved/agreed target — so no resolution yields `ProceedUnchanged` today.
> - **Not yet landed:** carrying the agreed/resolved `EntityId` target into membership evaluation (the prerequisite before `Agree`/`ResolverOnly` can enforce); per-family enforcement cutover; and **A2e** cutover criteria. **#2082 remains OPEN.** Nothing landed issues positive entity claims, treats any mapping as authority, trusts `UnknownLegacy` or gossip-originated mappings, or claims production / pilot / live-federation / Phase-2 readiness.

- **A2a — resolver trait + fail-closed default.** Define the `CoopEntityResolver` trait and its
  by-value resolution type in the gateway, with a `DenyUntilWired`-equivalent default that resolves
  nothing and reads none of its inputs. No route consumes it yet. (Mirrors the `TokenAuthoritySource`
  PR shape.)
- **A2b — observe-mode wiring + discrepancy logging.** Consult the resolver alongside the flat
  guard on one already-observed family (treasury), record `Resolved`/`NotMapped`/`Ambiguous`/
  `Untrusted` rates. Still denies nothing.
- **A2c — trusted, store-backed source with provenance.** Back the resolver with the governed
  `CoopEntityMap` (#2082) read path. **Prerequisite — provenance must be retrievable first:** the
  current `CoopEntityMap` read API (`entity_for_coop` / `coop_for_entity`) returns only the bound
  `coop_id ↔ EntityId` pair; it carries **no provenance** (activation vs operator-backfill vs
  surrogate vs governance receipt). Because this design makes provenance *decisive* for
  `Enforce` / `Issue` (§3 Authority and governance, §5 fail-closed rules), A2c must **first** add
  per-binding provenance persistence and retrieval — extend the store with a provenance field, or
  add a parallel provenance source keyed by the binding — before a store-backed resolver can be
  trusted. Until provenance is retrievable, a store-backed resolver cannot fail closed *by
  provenance* and would be forced to treat every existing binding as equally (un)authoritative,
  which violates this design's intent. Only after that prerequisite lands does A2c replace the
  lossy `legacy_coop_id_to_entity_id_fallback` bridge for observe.
- **A2d — route-family migration gates.** Per-family, observe → measure → (only then) enforce,
  gated on wired trust and acceptable divergence.
- **A2e — enforcement cutover criteria.** Define and document the explicit, measurable conditions
  under which `require_entity_access` becomes decisive for a family and the flat guard is retired
  for it.

## Relationship to #2061, #2080, #1868, #2082

- **#2061 (route side)** — migrate route families off the flat guard. The resolver is the
  prerequisite for any *enforce* step; A2b/A2d are its observe→enforce rungs.
- **#2080 (issuance side)** — replace `DenyUntilWired` with a positive `TokenAuthoritySource`. Reads
  the **same** membership-grounded source the resolver targets; A2c aligns their trust source.
- **#1868 (scope side)** — decompose `governance:write`. Orthogonal; interacts only in that scopes
  are permission, not authority.
- **#2082 (store side)** — the canonical persisted `coop_id ↔ EntityId` mapping/backfill. The
  resolver is the **governed read/authority layer that consumes #2082's store**; it does not
  duplicate it. A2c is the join.

## Source references (verified at `a77c3324`)

- `icn/crates/icn-gateway/src/middleware.rs:138` — `require_coop_access` (flat baseline).
- `icn/crates/icn-gateway/src/authority.rs:160,263,301` — `require_entity_access`,
  `EntityAccessObservation`, `observe_treasury_entity_access` (observe pattern).
- `icn/crates/icn-gateway/src/token_authority.rs:55,83,101,147,166` — `IssuanceAuthorityDecision`
  / `IssuanceAuthorityBasis` / `IssuanceDenialReason` / `TokenAuthoritySource` / `DenyUntilWired`
  (the issuance-side seam the resolver mirrors).
- `icn/crates/icn-gateway/src/auth.rs` — `TokenClaims` (non-enforcing `entity_id` / `entity_type`).
- `icn/crates/icn-gateway/src/entity_map.rs:33` — `legacy_coop_id_to_entity_id_fallback`
  (observe-only bridge to be replaced).
- `icn/crates/icn-entity/src/coop_entity_map.rs:53,93,127` — `CoopEntityMapError`, `project_coop_id`,
  `CoopEntityMap` trait (the store, #2082).
- `icn/crates/icn-entity/src/coop_entity_surrogate.rs:78` — `propose_surrogate_entity_id`.
- `icn/crates/icn-core/src/supervisor/init_notifications.rs` — activation-time map wiring.

## Validation

Docs-only change. The applicable gate is the documentation control plane:

```bash
python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict
ops/scripts/drift-check.sh
```

No Rust is touched, so no Rust build/test is required. `compliance_linter.py` scans gateway
`api/**` / `models.rs` / SDK/UI (not `docs/`); `lint-arch.py` targets `docs/ARCHITECTURE.md` only —
neither applies to this doc.
