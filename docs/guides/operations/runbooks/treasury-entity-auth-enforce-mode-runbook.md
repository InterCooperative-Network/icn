# Treasury Entity-Auth Enforce-Mode Runbook

Operator runbook for rehearsing and verifying the treasury entity-auth gate in
**enforcement** mode (`ICN_TREASURY_ENTITY_AUTH_MODE=enforce-trusted-resolver`) **before any
real enablement**.

## Status & scope

- **Status:** operational / forward-use runbook.
- **Not default-enabled.** The shipped default is `ObserveOnly`; nothing in this runbook turns
  enforcement on by default.
- **Not a production enablement authorization.** This document tells an operator how to
  rehearse the mode safely; it does not authorize enabling it on any real/partner deployment.
- **Applies only to the treasury route family** (the `treasury` API handlers in
  `icn-gateway`). It does not describe any other route family.
- **Requires #2254 or later** (the route-level consumption of the gate decision). On code
  before #2254 the env var only selects the *measurement* mode and never denies.
- **`#2082` remains open** — the canonical persisted `coop_id ↔ EntityId` mapping/backfill is
  not complete, and this runbook does not change that.

## 1. Purpose

`ICN_TREASURY_ENTITY_AUTH_MODE=enforce-trusted-resolver` makes the treasury routes **consume
the entity-auth gate decision inline** on the request path:

- The default `ObserveOnly` preserves the flat `require_coop_access` route behavior exactly —
  the entity-aware observation runs as a detached, fire-and-forget task and can never deny.
- Explicit enforce mode returns **403** **only** when `decide_treasury_gate(...)` returns
  `WouldDeny`. When it returns `ProceedUnchanged`, the route proceeds exactly as the flat
  guard decided.
- Entity membership is the authority signal. A resolver / `CoopEntityMap` mapping is a
  *target-evidence / trust qualifier* only — never authority on its own.

This runbook tells an operator how to **rehearse and verify** the mode in a controlled
environment before considering any real enablement.

## 2. Preconditions (before any rehearsal)

- Running code includes **#2254 or later** (confirm the deployed commit).
- Operator understands this is **treasury-only** and changes only the treasury route family.
- **Test/dev environment only**, unless a separate, explicit authorization exists for anything
  beyond that.
- A **known trusted resolver / provenance source** is available (a provenance-aware
  `CoopEntityMap` wired through the gateway), or the operator accepts that without one the
  gate has no trusted basis and enforcement will fail closed.
- Test data covers, at minimum:
  - a **verified trusted member-allow** path (trusted basis + membership target verified +
    affirmative member);
  - a **non-member deny** path;
  - a **resolver-only / mapping-only** path (a mapping exists but membership evidence does not
    verify the resolved target);
  - an **`UnknownLegacy` / unprovenanced** path;
  - an **observation-error** path (e.g. an unavailable `EntityManager`), if feasible.
- A **rollback path is known** (see §8).
- **Logs/metrics collection is available** to observe the gate decisions.
- **No private partner data** is used unless a separate approval exists.

## 3. How to enable for a rehearsal

Set the env var for the rehearsal process only (shell/session or the dev/test service unit —
**not** a production deploy manifest):

```bash
ICN_TREASURY_ENTITY_AUTH_MODE=enforce-trusted-resolver
```

Roll back by unsetting:

```bash
unset ICN_TREASURY_ENTITY_AUTH_MODE
```

or by selecting observe-only explicitly:

```bash
ICN_TREASURY_ENTITY_AUTH_MODE=observe-only
```

> **Do not add this env var to production deploy manifests.** Keep it scoped to the rehearsal
> process. Restart the relevant service after changing the value so it is re-read.

## 4. Expected behavior matrix (env value → mode)

The parser is fail-safe: only the explicit enforce tokens select enforcement; everything else
resolves to `ObserveOnly`. Matching trims whitespace and is case-insensitive.

| Env value | Mode | Route effect |
|---|---|---|
| (missing / unset) | `ObserveOnly` | no new denial |
| empty / whitespace-only | `ObserveOnly` | no new denial |
| `observe` / `observe-only` / `observe_only` | `ObserveOnly` | no new denial |
| `enforce-trusted-resolver` / `enforce_trusted_resolver` | `EnforceTrustedResolver` | deny only on `WouldDeny` |
| anything else (typo, bare `enforce`, `enforcetrustedresolver`, `true`, `1`, `on`) | `ObserveOnly` | no new denial (logs a `warn!`) |

There is **no fuzzy match toward enforcement** — an unrecognized value never enables denial.

## 5. Treasury route behavior matrix (under each mode)

| Case | `EnforceTrustedResolver` | `ObserveOnly` (default) |
|---|---|---|
| Trusted basis + verified membership target + member allow | **proceed** | unchanged |
| Non-member against trusted target | **403** (`NotMember`) | unchanged |
| Resolver mapping alone, membership evidence does not verify the resolved target | **403** (`ResolverOnlyTargetUnverified` / `AgreeTargetUnverified`) | unchanged |
| `UnknownLegacy` / unprovenanced / gossip-originated basis | **403** (`UntrustedResolution`) | unchanged |
| Legacy/resolver `EntityId` disagreement (migration collision) | **403** (`ResolverConflict`) | unchanged |
| Indeterminate membership (data gap) | **403** (`IndeterminateMembership`) | unchanged |
| Observation / resolver error | **403** (`ObservationError`) | unchanged |

Under `ObserveOnly`, **every** case above leaves route allow/deny **byte-identical** to the
flat `require_coop_access` guard — the gate is measured, never enforced. Under enforcement, the
route denies **exactly** when `decide_treasury_gate` returns `WouldDeny`; the resolver mapping
is only a trust qualifier + target verifier, never authority.

## 6. Verification checklist

- Confirm the **current mode** from logs/observation output (the gate logs under the
  `entity_authz_gate` / `entity_authz_observe` targets).
- Run the **allow-path smoke** (verified trusted member) → expect the route to proceed.
- Run the **deny-path smoke** (non-member / untrusted basis) → expect **403**.
- Confirm denied requests return **403 before any treasury mutation** (no budget/deposit/spend
  side effect on a denied request).
- Confirm a **successful** route still emits/records the normal observation evidence.
- Confirm a **denied** route leaves **no treasury mutation**.
- Confirm **rollback to `ObserveOnly`** restores the baseline (no new denials).
- Record, in your rehearsal notes: commit SHA, config value, test-data class, result, and
  operator identity.

## 7. Rollback

- `unset ICN_TREASURY_ENTITY_AUTH_MODE` (or set `observe-only`).
- Restart the relevant service if needed so the value is re-read.
- Re-run the baseline allow path.
- Confirm **no new denials** after rollback.
- Preserve logs/evidence from the failed rehearsal for follow-up.

## 8. Prohibited use

- Do **not** enable in production.
- Do **not** enable for partner / private data without separate, explicit approval.
- Do **not** use this runbook to claim `#2082` is complete.
- Do **not** treat resolver mappings as authority.
- Do **not** bypass the flat-guard assumptions without separate review.
- Do **not** extend this pattern to other route families without comparable target-aware
  evidence.

## 9. Nonclaims

- This does not complete `#2082`.
- This does not complete entity-aware authorization.
- This does not retire the flat `require_coop_access` baseline by default.
- This does not make resolver mappings authority.
- This does not trust `UnknownLegacy`, gossip-originated rows, or unprovenanced mappings.
- This does not issue positive `entity_id` / `entity_type` token claims.
- This does not make ICN production-ready, pilot-ready, organizer-ready, member-ready, or
  live-federated.
- This does not complete Phase 2.
- This does not complete `#2041`.
- This does not claim `#2113` completion.
- This does not change CodeQL setup or alert state.

## 10. Follow-ups

- Create an actual **rehearsal evidence template** if one is not already present (commit SHA /
  config value / test-data class / result / operator).
- Decide whether to add **non-env route tests via dependency injection** in a later code PR
  (the current tests are env-free pure-decision tests; no env-var route tests exist, to avoid
  global `std::env` races).
- Extend the consume-the-decision pattern to other route families **only** when each has
  comparable target-aware evidence.
- `#2082` mapping/backfill closure remains a separate track.

## Related

- `icn/crates/icn-gateway/src/authority.rs` — `parse_treasury_entity_auth_mode`,
  `active_treasury_entity_auth_mode`, `decide_treasury_gate`,
  `treasury_gate_enforcement_denial`.
- `icn/crates/icn-gateway/src/api/treasury.rs` — `observe_treasury` /
  `observe_treasury_by_did` (mode-branching route helpers).
- [Security Incident](./04-security-incident.md) · [Troubleshooting](./05-troubleshooting.md)
