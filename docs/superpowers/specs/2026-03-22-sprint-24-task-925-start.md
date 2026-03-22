# Sprint 24 Execution Start — #925 Commons Compute Spine

**Date:** 2026-03-22
**Sprint:** 24 (opening)
**Spine:** #925 → #947 → #964

---

## Sprint State Check

Sprint 23 board: 11/11 done (10 original + p24-pre-2). Status field still reads `active` — needs formal close, but that does not block Sprint 24 from starting. All pre-sprint prerequisites are complete.

No remaining pre-sprint ambiguity.

---

## #925 Current State

**Issue:** `feat(compute): Commons resource pool and contribution accounting`
**GitHub state:** OPEN
**Sub-issues:**

| # | Title | State |
|---|-------|-------|
| #946 | CommonsPool type for aggregate capacity | **CLOSED** |
| #947 | Unaffiliated node participation protocol | **OPEN** |
| #948 | Commons credit earning and spending | **CLOSED** |
| #949 | Commons contribution integration tests | **CLOSED** |

### What already exists (confirmed from repo)

**`icn-compute/src/commons_pool.rs`** — fully implemented:
- `CommonsPool` with `HashMap<String, CommonsParticipant>`
- `SybilPolicy { min_trust_score: 0.1, max_participants: 10_000 }`
- `try_add_participant()` — enforces sybil policy on admission
- `total_commons_capacity()` — weighted aggregate across participants
- `AggregateCapacity { cpu_cores, memory_mb, storage_mb, node_count }`
- `last_announce: Instant` on each participant (stale expiry field present, expiry logic absent — #964)
- Full unit test suite (8 tests, all passing)

**`actor/placement.rs` — `on_capacity_announce()`** — decision matrix implemented:

```
| cell_id | capacity_budget | Behavior                                      |
|---------|-----------------|-----------------------------------------------|
| None    | None            | Unaffiliated. Full commons: commons_share=1.0 |
| None    | Some(b)         | Unaffiliated with explicit budget. Use b.     |
| Some(_) | None            | Affiliated, default budget (0.10 commons).    |
| Some(_) | Some(b)         | Affiliated with explicit budget. Use b.       |
```

Unaffiliated path routes through `try_add_participant()` with trust_score from `(self.trust_callback)(&executor)`.

**`actor/lifecycle.rs`** — commons credit settlement fires correctly when:
- Task outcome = `Success`
- `claimed_task.scope == ScopeLevel::Commons`
- `commons_settlement_callback` is set

**`scheduler.rs` — `DefaultPlacementPolicy::score_task()`** — commons-scoped tasks are routable to nodes with `peer_scope = ScopeLevel::Commons` (what unaffiliated nodes report).

### The precise gap: trust bootstrap for unaffiliated nodes

`on_capacity_announce()` calls `(self.trust_callback)(&executor)` to get `trust_score`. For a node the trust graph has never seen, this returns **0.0**.

`try_add_participant()` enforces `min_trust_score: 0.1`. An unaffiliated node with zero relationship history **cannot pass sybil admission**.

The existing `test_unaffiliated_node_joins_pool` bypasses this by calling `pool.add_participant()` directly with a hardcoded `trust_score: 0.5`. It does not exercise the live path through `on_capacity_announce`.

This is the precise definition of #947 being open: the commons pool cannot admit new unaffiliated nodes in production because they start with 0.0 trust.

---

## Recommended First Slice

**One problem, two parts:**

### Part A — Fix the bootstrap gap (the blocking issue)

Separate the sybil threshold into two tiers in `SybilPolicy`:

```rust
pub struct SybilPolicy {
    /// Minimum trust score to join the commons pool as an unaffiliated node.
    /// Defaults to 0.0 — any node can attempt commons participation.
    /// Set higher to require prior trust relationships.
    pub commons_min_trust: f64,

    /// Minimum trust score for affiliated nodes (with cell_id).
    /// Higher bar: org membership implies accountability.
    /// Default: 0.1
    pub affiliated_min_trust: f64,

    /// Maximum participants (spam guard for both paths).
    pub max_participants: usize,
}
```

Rename the existing `min_trust_score` → `affiliated_min_trust`. Add `commons_min_trust: 0.0`.

In `try_add_participant()`, choose the threshold based on whether `cell_id` was present at admission time — but `CommonsParticipant` doesn't currently carry `cell_id`. The simplest approach is to add an `is_affiliated: bool` field to `CommonsParticipant` and select the threshold accordingly.

Alternatively (simpler, if we accept that commons admission is always lower-bar): use a single `commons_admission_min_trust: f64 = 0.0` that applies to all pool admissions, reserving the existing `min_trust_score` logic for a separate affiliated-only gate.

**Decision to make before implementing**: does trust score affect admission or only scheduling priority?

Architectural intent (from issue): commons is open-participation, trust gates scheduling priority (higher trust = better score, lower queue depth). The admission gate for commons should be near-zero or configurable to zero. Orgs that want a curated commons can set `commons_min_trust` higher; the default cooperative commons doesn't need it.

Concrete change:
- `SybilPolicy`: add `commons_admission_min_trust: f64 = 0.0`
- `CommonsParticipant`: add `is_affiliated: bool`
- `try_add_participant()`: use `commons_admission_min_trust` for non-affiliated, `min_trust_score` for affiliated
- `on_capacity_announce()`: pass `cell_id.is_some()` when building `CommonsParticipant`

### Part B — Integration test through the live path

Write one integration test that goes through `on_capacity_announce` with a real `trust_callback` returning 0.0, verifies the node joins the pool, executes a commons task, and receives settlement. This test currently does not exist.

---

## Acceptance Criteria (for #947 close)

1. Node with `cell_id: None` and trust score 0.0 can join the commons pool with default `SybilPolicy`
2. Node with `cell_id: None` can receive and execute a `ScopeLevel::Commons` task
3. Commons credit settlement fires for that node's contribution
4. Affiliated node with trust score 0.0 is still rejected by the affiliated threshold (default 0.1)
5. `max_participants` still caps the pool for both affiliated and unaffiliated nodes

---

## Coupling to #947 and #964

**#947 → #925 close:** #925 closes when #947 closes. All other sub-issues are done.

**#964 (stale expiry) is independent.** `last_announce: Instant` exists. The implementation is:
```rust
pub fn expire_stale(&mut self, max_age: Duration) -> Vec<String> {
    let stale: Vec<String> = self.participants
        .iter()
        .filter(|(_, p)| p.last_announce.elapsed() > max_age)
        .map(|(did, _)| did.clone())
        .collect();
    for did in &stale {
        self.participants.remove(did);
    }
    stale
}
```

Call `expire_stale()` inside `on_capacity_announce()` before the decision matrix — prune on each announce, no background timer needed initially. Add `commons_pool_expired_total` counter. Wire up `StaleParticipantConfig { max_age: Duration::from_secs(300) }` into the actor.

#964 can ship in the same sprint as #947 or immediately after. No coupling risk.

---

## Risks

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| `trust_callback` is not injectable in test context | Medium | Check if `PlacementActorBuilder` exposes `with_trust_callback()` for test |
| Two-tier threshold changes ripple into `CommonsPoolPolicy` | Low | `CommonsPoolPolicy` deals with credit ceilings, not admission; isolated |
| `is_affiliated` field on `CommonsParticipant` breaks existing tests | Low | It's additive; existing tests construct directly and can default to `false` |
| Stale expiry on `Instant` doesn't survive process restart | Known non-issue | Documented: `last_announce` is ephemeral by design; participants re-announce on join |

---

## Next Concrete Action

**First task:** Add `commons_admission_min_trust: f64 = 0.0` to `SybilPolicy` and `is_affiliated: bool` to `CommonsParticipant`. Update `try_add_participant()` to gate on the appropriate threshold. Update `on_capacity_announce()` to set `is_affiliated = cell_id.is_some()`.

Scope: `crates/icn-compute/src/commons_pool.rs` (the type and admission logic), `crates/icn-compute/src/actor/placement.rs` (the `on_capacity_announce()` call site). No changes to scheduler, lifecycle, or ledger.

Expected diff size: ~40 lines across two files, plus updated tests.

Branch: `feat/947-unaffiliated-trust-bootstrap`

After that: write the `on_capacity_announce` integration test path. After that: implement #964 stale expiry. After both: close #947, close #925, open #964 as the next task.
