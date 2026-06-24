# Entity-Aware Authorization Control Map

**Status**: draft — design / control map (migration orientation, not implementation)
**Truth class**: descriptive
**Canonical**: no — current implementation truth lives in [docs/STATE.md](../STATE.md) and [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md)
**Last Reviewed**: 2026-06-24
**Source basis**: read against `main` @ `e7f3fc02` (#2181). Counts re-verified with `rg`; re-verify before relying on exact numbers.
**Related**: [RFC-0018](../rfcs/RFC-0018-entity-aware-request-authorization.md) · [ADR-0035](../adr/ADR-0035-entity-aware-request-authorization.md) · [ABUSE_CASE_HARDENING_STRATEGY.md](../architecture/ABUSE_CASE_HARDENING_STRATEGY.md) · issues #2061, #2080, #1868

> **This document is a control map for an in-progress migration, not a description of a finished
> system. It explains where authorization is enforced today, what is observe-only, and what is not yet
> wired. It does not authorize any behavior change.**

---

## Purpose

ICN's request authorization currently spans four moving parts: a flat `coop_id` guard, an entity-aware
guard, optional token claims, and a trusted-issuance seam. Each is at a different stage of a single
migration, and the surface is easy to misread — both as "more enforced than it is" and as "broken."
This map fixes one durable picture of:

- what each authorization input actually proves today,
- the two coexisting authorization regimes (flat vs entity-aware),
- the observe-mode pattern being used to migrate safely,
- the fail-closed trusted-issuance seam,
- the single missing **`coop_id → EntityId` resolver** that gates further progress, and
- how `#2061`, `#2080`, and `#1868` relate.

It makes the migration surface explicit **before** any enforcement or code change.

## Current authorization reality

A gateway request is authorized today by two things: an authenticated bearer token (the caller proves
key control of a DID) and a per-route guard. On the **bulk** of mutation routes that guard is a flat
`coop_id` string match. On entity routes it is an entity-aware membership check. On treasury routes the
entity-aware check runs **alongside** the flat guard but only as an observation. That is the whole
picture; everything below is detail.

The current system is **not broken** because it still uses flat `coop_id` guards. Those guards are the
current enforced safety baseline. The risk is treating flat `coop_id` as the *final* authority model.

## DID, token, `coop_id`, `EntityId`: what each proves

| Input | Where | Enforced today? | What it proves |
|---|---|---|---|
| `sub` (DID) | `TokenClaims.sub` (`icn-gateway/src/auth.rs`) | yes (authentication) | **key control** of a DID. Nothing institutional. |
| `coop_id` | `TokenClaims.coop_id` | **yes — the sole authZ gate on most routes** | a flat namespace string. `require_coop_access` checks `claims.coop_id == <route coop_id>`. |
| `scopes` | `TokenClaims.scopes` | yes | technical/capability permission (`ledger:write`, `governance:write`, …). **Not** institutional authority. |
| `entity_id` / `entity_type` | `TokenClaims.entity_id?` / `entity_type?` | **no — non-enforcing context** | a typed `EntityId` context, settable only by a *trusted* issuance path. The dev / self-asserted path never sets it; the flat guard ignores it. |
| `EntityId` + membership | `icn-entity` (`entity.rs`, `membership.rs`, `registry.rs`) | enforced **only on entity routes** | the typed institutional model: entity kind, membership, role, standing, capability. |

The load-bearing distinctions this map preserves:

```
A DID proves key control.
A DID is not membership.
A token is not voting power.
A capability scope is not a mandate.
A flat coop_id match is not entity-aware authorization.
Mapping is not authority.
```

Entity-aware authorization, even once enforced, remains **subordinate to the mandate/authority spine**
where an institutional *effect* is involved (governance acts gate separately; see the operating model).

## Regime A — flat `coop_id` authorization

- Guard: `require_coop_access(req, coop_id)` in `icn-gateway/src/middleware.rs`.
- Mechanism: string equality between the token's `coop_id` claim and the route's `coop_id`.
- Status: **authoritative** — this is the enforced gate on the bulk of mutation routes.
- Blind to: entity kind, membership graph, hierarchy, delegation, role, and standing.
- Deliberately ignores the optional `entity_id` claim (a regression test pins this: a token's
  `entity_id` must not influence the flat decision).

This is the safety baseline. It is coarse but real, and it must remain in force until a typed
replacement is both available and measured.

## Regime B — entity-aware authorization

- Guard: `require_entity_access(entity_mgr, caller, target, action)` in `icn-gateway/src/authority.rs`
  (RFC-0018 / ADR-0035).
- Mechanism: resolve the caller's membership in the target `EntityId`, then check an `EntityAction`
  against role / active-standing / capability. The current action triad is `ModifyEntity`,
  `TreasuryRead`, `TreasuryWrite`.
- Status:
  - **Enforced** on entity routes (`icn-gateway/src/api/entity.rs`, via `require_entity_write_access`).
  - **Observe-only** on treasury routes (see next section).
  - Absent on all other route families.
- This is the migration target: typed, membership-grounded authorization that the kernel never has to
  understand (meaning stays in the app; the kernel sees only the allow/deny).

## Observe-mode migration pattern

Treasury routes are the migration frontier. There, the entity-aware decision is computed **alongside**
the authoritative flat guard and recorded — never acted on:

- `observe_treasury_entity_access(...)` in `icn-gateway/src/authority.rs` returns an
  `EntityAccessObservation` (`AgreesAllow` / `EntityDeny(reason)` / `Indeterminate(reason)` / `Error`).
- The observation is **data, not a `Result`** the caller propagates — it is structurally incapable of
  denying a request. It emits a metric and a log line; the flat guard remains the sole enforced gate.
- It records exactly the signal a future enforcement flip needs: where the entity-aware path *would*
  deny (`NonMember` / `InactiveMember` / `MissingCapability`) or *cannot decide* (data gaps such as a
  missing `entity_id` or unregistered entity).

The migration discipline this encodes:

```
Keep the flat guard as the enforced baseline.
Compute the entity-aware decision in observe mode and measure divergence.
Move one route family at a time to enforcement — and only after trusted
EntityId resolution and trusted token issuance exist.
```

## Trusted issuance seam

Whether a token may carry typed entity context is a separate, fail-closed boundary:

- `TokenAuthoritySource` (trait) in `icn-gateway/src/token_authority.rs` answers, by value, whether a
  subject DID may receive a token binding a `coop_id`/`EntityId` with given scopes
  (`IssuanceAuthorityDecision::{Allow{basis}, Deny{reason}}`).
- The only source shipped is `DenyUntilWired`: it **denies every request** and reads none of its inputs
  — no DID, `coop_id`, `EntityId`, or scope can produce an allow. Denial is a decision value, not an
  error, so a caller can never mistake an unwired/unhealthy source for an allow.
- `IssuanceAuthorityBasis` names the future positive sources (`Membership`, `Standing`, `Bootstrap`,
  `AuthorityGrant`); only a test-only basis is ever constructed today. **No** production issuance path
  (`/auth/verify`, invites, sessions, SDIS enrollment, bootstrap) consults a `TokenAuthoritySource`
  yet, and `entity_id` minting remains a raw primitive.

Self-asserted `coop_id` issuance at `/auth/verify` is **fail-closed** (RFC-0018, #2075): DID key control
alone never authorizes a cooperative.

## The `coop_id → EntityId` resolver keystone

All three seams above — route enforcement, trusted issuance, and the token `entity_id` claim — converge
on the same missing piece: a **trusted, governed resolver from the legacy flat `coop_id` namespace to
the typed `EntityId` model**.

Today the only bridge is `legacy_coop_id_to_entity_id_fallback` in `icn-gateway/src/entity_map.rs`, a
best-effort *reject-not-normalize* projection used only inside observe-mode. It is not a governed
authority source.

```
The coop_id → EntityId resolver is the keystone because it joins the legacy route
namespace to the typed institutional entity model. Without it:
  - #2061 cannot safely flip treasury observe → enforce (nor add observe elsewhere), and
  - #2080 cannot safely mint positive entity-scoped authority.
```

It is the keystone precisely because both the enforcement side and the issuance side are designed to
read from the *same* membership-grounded source once it exists.

## Route-family enforcement map

Mechanically counted at `e7f3fc02` (`rg` over `icn-gateway/src`); treat as a snapshot, not a contract.

| Route family (file) | Flat `require_coop_access` sites | Entity-aware (Regime B) | Net state |
|---|---|---|---|
| `api/entity.rs` | 0 | **enforced** (`require_entity_write_access` → `require_entity_access`, `ModifyEntity`) | migrated |
| `api/treasury.rs` | 10 (enforced) | **observe-only** ×2 (`TreasuryRead` / `TreasuryWrite`) | migration frontier |
| `api/ledger.rs` | 6 | none | flat-only |
| `api/coops.rs` | 6 | none | flat-only |
| `api/registry.rs` | 2 | none | flat-only |
| `api/invites.rs` | 2 | none | flat-only |
| `api/escrow.rs` | 2 | none | flat-only |
| `api/recurring_settlements.rs` | 1 | none | flat-only |
| `middleware.rs` | guard definition + helpers + tests | — | the guard itself |

## Relationship to #2061, #2080, and #1868

- **#2061 — entity-aware request authorization (route side).** Migrate route families off the flat
  `coop_id` guard onto entity-aware checks. The next safe rung is to extend the **observe** pattern
  (already proven on treasury) to a flat-only family and measure divergence *before* any enforcement
  flip. Blocked on the resolver keystone for the enforce step.
- **#2080 — trusted positive token issuance (issuance side).** Replace `DenyUntilWired` with a positive
  `TokenAuthoritySource` (likely `Membership`-backed) plus a first-admin bootstrap path. Blocked on the
  same resolver; until then, fail-closed `DenyUntilWired` is the correct shipped state.
- **#1868 — decompose broad `governance:write` (scope side).** Orthogonal but complementary: breaking a
  single broad scope into per-action capabilities / mandate-bundles shrinks the blast radius of any one
  token. It interacts with this map only in that scopes are technical permission, not authority.

These are three sides of one migration: **routes** (who is checked), **issuance** (what a token may
claim), and **scopes** (how coarse the permission is). The resolver is the joint that lets the route
and issuance sides converge on one membership-grounded authority source.

## Migration principles

1. The flat `coop_id` guard is the **current enforced safety baseline**; do not remove it ahead of a
   measured, typed replacement.
2. Frame entity-aware authorization as a **migration target**, not a finished state.
3. Migrate by **observe → measure → enforce**, one route family at a time.
4. Trusted issuance stays **fail-closed** (`DenyUntilWired`) until a governed source is wired.
5. Entity-aware authorization remains **subordinate to the mandate/authority spine** for institutional
   effects.
6. Land the **`coop_id → EntityId` resolver** as the shared keystone before flipping enforcement or
   minting positive entity-scoped authority.

## Non-claims

This document explicitly does **not** claim:

- that entity-aware authorization is fully enforced (it is enforced only on entity routes; observe-only
  on treasury; absent elsewhere);
- that trusted positive token issuance is live (only `DenyUntilWired` ships);
- that flat `coop_id` guards are removed (they remain the enforced baseline);
- that the `coop_id → EntityId` resolver exists as a governed authority source;
- production readiness, live federation, formal NYCN pilot status, or complete API coverage.

This is a control map for migration, not an implementation.

## Out of scope

This document covers the authorization-migration surface only. It does not address route/OpenAPI
inventory (#2112), governance-scope decomposition implementation (#1868), or any non-auth findings; and
it intentionally introduces no runtime change.

## Validation

Docs-only change. The applicable gate is the documentation control plane:

```bash
python3 docs/scripts/doc_control_check.py --repo . --registry docs/registry.toml --strict
```

Not applicable to this doc: `compliance_linter.py` scans gateway `api/**`, `models.rs`, and SDK/UI only
(not `docs/`); `lint-arch.py` targets `docs/ARCHITECTURE.md` only. No Rust build/test is required for a
docs-only change.

## Source references (verified at `e7f3fc02`)

- `icn/crates/icn-gateway/src/middleware.rs` — `require_coop_access` (flat guard) + tests.
- `icn/crates/icn-gateway/src/authority.rs` — `require_entity_access`, `EntityAction`, observe-mode
  (`observe_treasury_entity_access`, `EntityAccessObservation`, `DenyReason`, `IndeterminateReason`).
- `icn/crates/icn-gateway/src/token_authority.rs` — `TokenAuthoritySource`, `DenyUntilWired`,
  `IssuanceAuthorityDecision` / `IssuanceAuthorityBasis` / `IssuanceDenialReason`.
- `icn/crates/icn-gateway/src/auth.rs` — `TokenClaims` (incl. non-enforcing `entity_id` / `entity_type`).
- `icn/crates/icn-gateway/src/entity_map.rs` — `legacy_coop_id_to_entity_id_fallback` (bridge).
- `icn/crates/icn-gateway/src/api/{entity,treasury,ledger,coops,registry,invites,escrow,recurring_settlements}.rs` — route guards.
- `icn/crates/icn-entity/src/{entity,membership,registry}.rs` — typed entity / membership model.
