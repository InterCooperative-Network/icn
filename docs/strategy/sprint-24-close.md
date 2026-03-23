# Sprint 24 Close — Commons-Compute Spine

**Date closed:** 2026-03-23
**Scope:** Epic `commons-compute` — trust bootstrap, stale expiry, V1 credit enforcement

---

## What Shipped

| Issue | Description | PR | Tasks |
|-------|-------------|-----|-------|
| #947 | Two-tier trust threshold for unaffiliated vs affiliated pool admission | #1396 | s24-t1, s24-t2 |
| #964 | Stale commons pool participant expiry (`expire_stale`, `StaleParticipantConfig`) | #1396 | s24-t3 |
| #1397 | V1 scheduling-time credit enforcement floor in `handle_submit()` | #1398 | s24-t6 |
| #925 | Parent: commons resource pool + credit accounting substrate | closed as delivered | s24-t4 |

All six sprint tasks (`s24-t1` through `s24-t6`) are done.

---

## Key Implementation Notes

**#947 (s24-t1/t2):** `SybilPolicy.commons_admission_min_trust` = 0.0 for unaffiliated nodes; `is_affiliated: bool` derived from `cell_id.is_some()` at announce time. Two-tier threshold: affiliated nodes require `min_trust_score` (0.1), unaffiliated admitted at 0.0.

**#964 (s24-t3):** `expire_stale(max_age)` piggybacked on each `NodeCapacityAnnounce`. No background timer in V1. `Duration::MAX` is the "disabled" sentinel.

**#1397 (s24-t6):** Enforcement seam is `handle_submit()` in `lifecycle.rs`, after `CommonsPoolPolicy` governance checks, before policy manager. Floor constant `COMMONS_CREDIT_FLOOR = 1` — not a pricing model, a V1 presence check. `balance_callback = None` skips the gate silently (opt-in). `icn-ledger` stays in `[dev-dependencies]`; the inline `balance < 1` check is semantically identical to `check_sufficient_balance(balance, 1)`.

---

## Intentionally Deferred (not scope-crept into Sprint 24)

| Deferred Work | Reason Deferred |
|---------------|----------------|
| Cost-based enforcement — derive required credits from `task.resource_profile` or `task.fuel_limit` | Distinct pricing model; `COMMONS_CREDIT_FLOOR = 1` is the correct V1 sentinel |
| `icn-ledger` promotion from `[dev-dependencies]` to `[dependencies]` in `icn-compute` | Wider change; own PR scope. Enables calling `check_sufficient_balance()` from library code |
| Pre-execution reservation — reserve credits at submit, settle actuals from `ExecutionReceipt` | Prevents concurrent-submitter races; requires reservation → settlement two-phase design |

These are three distinct slices. None blocked V1 from being useful. New issues should be filed when prioritized — do not silently extend Sprint 24.

---

## CI Lesson (worth remembering)

`test_connection_flapping` in `icn-net` failed on first CI run with a QUIC handshake error (`aborted by peer: the application or application protocol caused the connection to be closed during the handshake`). This is the same failure class as the ~20 sibling tests already marked `#[ignore = "Flaky in CI: QUIC stream failures in containerized environment"]`.

**Correct response:** rerun once. It passed. No code churn.

**Rule:** For QUIC/container/network failures in `icn-net` integration tests — rerun once before treating as a code regression. Only investigate if it fails on the second run. Only `--admin` bypass if the failure is clearly queue-stalled (required checks `pending / 0s` for >1hr with the runner healthy).

---

## Next Engineering Move

Open a new issue for the highest-priority deferred follow-on. Recommended: **cost-based enforcement** — it is the next semantic upgrade after the flat floor, and the mechanism (`COMMONS_CREDIT_FLOOR` → `compute_credits_required(&task)`) is named and documented in `docs/design/economics/commons-credit-enforcement-v1.md`.
