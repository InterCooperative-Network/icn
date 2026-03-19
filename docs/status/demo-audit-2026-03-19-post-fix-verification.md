# Post-Fix Verification — Flows 2 & 4 — 2026-03-19

**PRs merged:** #1343 (Bug #1334 fix), #1344 (Bug #1335 / Flow 3 fix)
**Deployed:** SHA `41ceac95fbf33741322dd30a8d6b268416738ce2` via `kubectl set image` on all 4 coop namespaces
**Verified:** 2026-03-19, live K3s cluster (10.8.30.40–42)

## Flow 2 — Patronage Demo

| Step | Action | HTTP | Result |
|------|--------|------|--------|
| 1–7  | Governance (proposal create, open, vote, close) | 200/201 | PROVEN |
| 8    | `POST /v1/ledger/brightworks-cooperative/payment` | 404 | FRAGILE (pre-existing: ledger payment endpoint not implemented) |
| 9    | `GET /v1/ledger/brightworks-cooperative/balance/{did}` | 404 | FRAGILE (same) |
| 10   | `GET /v1/ledger/brightworks-cooperative/history` | 200 | PROVEN (empty, endpoint works) |
| **11** | **`GET /v1/receipts/allocations`** | **200** | **BUG #1334 FIXED** |

**Flow 2 classification: FRAGILE** (ledger payment/balance endpoints return 404 — pre-existing, not related to #1334)
**Key win:** Step 11 now returns `HTTP 200 []` instead of `HTTP 400 "missing field decision_hash"`.

## Flow 4 — Multi-Hop Reporting Demo

| Step | Action | HTTP | Result |
|------|--------|------|--------|
| 2    | Harbor Homes capital decision query | 200 | PROVEN |
| 2    | GovernanceReceipt (Harbor Homes) | 200 | PROVEN |
| 3    | BrightWorks patronage decision query | 200 | PROVEN |
| 4    | BrightWorks ledger history | 200 | PROVEN |
| 5    | `GET /v1/receipts/allocations` (FL CDN token) | 400 | FRAGILE (federation:read scope gap — distinct from Bug #1334) |
| 6    | River City equipment-sharing decision | 200 | PROVEN |
| 7    | FL CDN federation status | 200 | PROVEN (initialized: false) |
| 8    | Cross-coop write rejected | 401 | PROVEN (auth boundary enforced) |

**Flow 4 classification: FRAGILE** (federation scope gaps unchanged — deployment config issue, not design gap)

## Bug #1334 Fix — Confirmed

`GET /v1/receipts/allocations` without `?decision_hash=` parameter:
- **Before fix:** HTTP 400 `"Query deserialize error: missing field decision_hash"`
- **After fix:** HTTP 200 `[]` (empty list — no receipts seeded yet, but endpoint works)

Note: Flow 4 step 5 hits a different 400 (scope enforcement for `federation:read` token), not Bug #1334.

## Phase B Summary

| Task | Status | Evidence |
|------|--------|---------|
| s15-t1: Audit all 4 flows | DONE | docs/status/demo-audit-2026-03-19.md |
| s15-t2: Bug #1334 (optional decision_hash) | DONE | PR #1343 merged + live K3s verified |
| s15-t3: Bug #1335 (Flow 3 federation schema) | DONE | PR #1344 merged + steps 5/6/7 verified 2xx |

**Phase B complete. Phase C (receipt chain, s15-t4 through t6) is unblocked.**
