# `apps/membership` `coop_core` — Map-Parity Contract (#2082 gap 12b)

**Status**: draft — design / audit (parity contract, not implementation)
**Truth class**: descriptive
**Canonical**: no — current implementation truth lives in [docs/STATE.md](../STATE.md) and [docs/PHASE_PROGRESS.md](../PHASE_PROGRESS.md)
**Last Reviewed**: 2026-07-02
**Source basis**: read against `main` @ `480f294a` (#2271). Code anchors were verified at that commit — re-verify before relying on exact numbers.
**Related**: [create-treasury-entity-id-semantics.md](create-treasury-entity-id-semantics.md) (the 12a contract this extends) · [coop-id-entity-resolver.md](coop-id-entity-resolver.md) · [ADR-0084](../adr/ADR-0084-controlled-treasury-entity-id-backfill-apply-contract.md) · issues #2082 (this lane), #2081 (blocked, untouched), #2080 (separate, untouched)

> **This document defines the last #2082 structural gap (12b) and the decision it
> actually requires.** It audits whether the `apps/membership` `coop_core` actor is a
> live path at all, what parity with `icn-coop` would mean, and whether parity or
> deprecation is the right spend. It changes **no** code behavior. A
> `coop_id ↔ EntityId` mapping binds an identity target only — it grants **zero**
> membership, role, capability, mandate, standing, or permission.

## 1. Liveness audit (verified at `480f294a`)

**The membership `coop_core` actor has no production caller.** Verified:

- The only workspace dependent of `icn-membership-app` is `icn-core` — and only in
  **`[dev-dependencies]`** (`icn/crates/icn-core/Cargo.toml` ~L69, under the
  dev-dependencies section).
- The only Rust consumer is
  `icn/crates/icn-core/tests/vertical_slice_integration.rs` (spawns
  `icn_membership_app::coop_core::actor::CoopActor` as its coop fixture).
- `icn-core/src/**`, the daemon (`icnd`), and the supervisor contain **zero**
  references. No gateway route, RPC method, or binary reaches this actor.

**Classification: a test-harness fixture**, not a live, duplicated, or transitional
production path. It is a near-copy of `icn-coop`'s actor frozen at a pre-#2104
level of the #2082 lane.

## 2. Divergence from `icn-coop` (the actual gap)

| Behavior | `icn-coop` (current) | membership `coop_core` |
|---|---|---|
| Activation-time map binding (`Activation` provenance) | ✅ #2104/#2193 (`bind_coop_entity_map_activation`) | ❌ none |
| Activation-time treasury `entity_id` population | ✅ #2266 (recorded-binding first; projection only when no map) | ❌ none — plain `register_treasury`, `entity_id: None` (~L328-370) |
| `CreateTreasury` trusted-binding consultation | ✅ #2271 (read-only; no projection fallback) | ❌ none (~L484+; plain `register_treasury`) |
| `icn-entity` dependency / `CoopEntityMap` integration | ✅ | ❌ none at all (`Cargo.toml` verified) |
| Post-activation `CreateTreasury` rejection | ✅ | ✅ (same guard; tests pin it) |

Consequence: the **vertical-slice integration test exercises stale #2082 semantics**
— cooperatives activated through the fixture produce treasuries with no identity
target and no `Activation` binding, silently diverging from what the production
(`icn-coop`) path now does.

## 3. Non-goals

- No code change in this document or its PR.
- No new authority gate design (who may create/activate is out of scope).
- No gateway enforcement, route-default, or token-issuance change (#2081/#2080
  remain untouched, blocked and separate respectively).
- No #2082 closure.

## 4. Dependency boundary

Adding `icn-entity` to `icn-membership-app` is *permissible* — apps are
domain-specific (the meaning firewall constrains kernel crates, not apps), and the
app already pulls `icn-entity` transitively through `icn-ledger`. It is not
*architecturally blocked*; it is a question of whether the spend is justified for
a dev-dependency fixture. That is the real decision below.

## 5. The decision: parity vs. deprecate/redirect

**Option A — full parity implementation.** Add `icn-entity`, a
`CoopEntityMapHandle` field + spawn variant, activation binding
(`Activation`-provenance, local-authoritative only), activation-time populate
(recorded-binding first), and `CreateTreasury` consultation (12a semantics
verbatim), plus the full test matrix (§7). Cost: duplicating ~4 merged rungs of
identity-mapping logic into a second copy whose only caller is one integration
test — and every future #2082-lane change pays the duplication tax again.

**Option B — deprecate/redirect (recommended).** Migrate
`vertical_slice_integration.rs` to spawn `icn_coop::CoopActor` (already a
workspace crate; the fixture usage is constructor + handle calls, which the
`icn-coop` handle also provides), then freeze or remove the `coop_core` duplicate.
The vertical-slice test then exercises the *real* production path — including the
#2104/#2266/#2271 semantics — instead of a stale copy. Cost: a test-migration PR
plus whatever `coop_core` types the membership app's own modules still use
(audit required: `types.rs`/`store.rs` may be consumed by membership's non-actor
code even if the actor is not).

**Option C — labeled fixture.** Keep the duplicate, add a prominent module-doc
caveat that it is a fixture without #2082 parity. Cost: the vertical-slice test
keeps validating divergent semantics; the "gap" never closes, only gets a sign.

**Recommendation: Option B.** Parity (A) would wire identity-target mapping into a
test fixture — effort that produces no production behavior and a standing
duplication tax. Deprecation (B) closes gap 12b *by removing the parallel path*,
makes the flagship integration test cover the true semantics, and is the only
option after which #2082's structural scope is genuinely complete. Option C is
acceptable only as a stopgap if (B)'s type-usage audit finds heavy internal
coupling. **The choice between A and B is Matt's call**; this document defines
both contracts so either next PR is mechanical.

## 6. Required parity slices (only if Option A is chosen)

Strict order, one PR each, mirroring the merged lane:

1. **Activation-time map binding** — mirror `bind_coop_entity_map_activation`:
   local authoritative activation records `Activation` provenance; the gossip
   mirror stays unprovenanced; a bind failure never fails activation.
2. **Activation-time treasury `entity_id` population** — mirror #2266's ordering
   (register `None` → commit → bind LAST → populate from the recorded binding;
   pure projection only when no map is wired).
3. **`CreateTreasury` trusted-binding consultation** — the 12a contract verbatim
   ([create-treasury-entity-id-semantics.md](create-treasury-entity-id-semantics.md)):
   read-only consult, coop_id-preserving two-step, **no**
   `register_treasury_with_entity`, **no** projection fallback, `UnknownLegacy`
   and all structurally-unsafe states stay `entity_id: None`, map never written.

## 7. Required test matrix before any Option-A implementation PR

The 12a matrix, re-run against the membership actor: each trusted provenance
populates (incl. a divergent-surrogate case proving legacy `coop_id` preservation
and no row under the surrogate slug); `UnknownLegacy`/missing provenance, reverse
mismatch, malformed/non-cooperative target → `None`; no-map → `None` (no
projection fallback at creation); post-activation rejection unchanged; duplicate
entity target → fail-closed `EntityIdConflict`, no partial write; plus read-only
test doubles that panic on any `bind_*` call (no-map-write proof), and the
activation-ordering tests from #2266.

For **Option B**, the required tests are instead: the migrated vertical-slice
test passes against `icn_coop::CoopActor` with byte-identical assertions (plus
the now-reachable entity_id assertions), and a build proving nothing else
consumed the removed/frozen actor.

## 8. Out of scope / explicit non-claims

This document and its PR claim **none** of the following: any code behavior
change; any map, treasury, or provenance mutation; any gateway/default
enforcement or route-outcome change; any positive token issuance; #2081 progress
(remains blocked); #2080 progress (remains separate); #2082 closure (requires
explicit authorization); trust of `UnknownLegacy` (untrusted unless a future
evidence-bearing workflow repairs it); mapping-as-authority; production, pilot,
member, organizer, or live-federation readiness; Phase 2 completion.

## 9. Validation

- Frontmatter + `docs/registry.toml` entry + `docs/INDEX.md` link added per
  [docs-control-map — Adding a new doc](../reference/project-index/docs-control-map.md).
- `python3 docs/scripts/doc_control_check.py` run locally before commit.
- Vocabulary: settlement/allocation language only; no prohibited terms.
