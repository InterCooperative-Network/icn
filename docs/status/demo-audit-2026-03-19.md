# Demo Audit — 2026-03-19

**Cluster target:** K3s at 10.8.30.40:30080
**Auditor:** Claude Code (s15-t1)
**Reseed:** `reseed-federation-demo.sh` — seeded 4, skipped 9, failed 0
**Timestamp:** 2026-03-19, sprint 15 phase B

---

## Summary

| Flow | Script | Classification | Governance | Core Feature |
|------|--------|----------------|------------|--------------|
| Flow 1 | `flow-1-governance.sh` | **PROVEN** | ✅ | GovernanceReceipt + Ed25519 |
| Flow 2 | `flow-2-patronage.sh` | **FRAGILE** | ✅ | Ledger 404, receipt chain 400 |
| Flow 3 | `flow-3-federation.sh` | **FRAGILE** | ✅ | Federation API schema mismatch |
| Flow 4 | `flow-4-reporting.sh` | **FRAGILE** | ✅ | Receipt chain 400, federation 400 |

---

## Flow 1 — Harbor Homes Governance

**Script:** `demo/scripts/flow-1-governance.sh`
**Classification: PROVEN**

### Command
```bash
cd /home/ubuntu/projects/icn && bash demo/scripts/flow-1-governance.sh
```

### Result
Exit code: 0. All 10 steps succeeded.

### Evidence
| Step | Action | HTTP | Result |
|------|--------|------|--------|
| 1 | Create governance domain | 200 | `harbor-governance` domain active |
| 2 | Create roof repair proposal | 201 | Proposal ID assigned |
| 3 | Open proposal for voting | 200 | State → Open |
| 4 | Cast vote (For) | 200 | Vote recorded |
| 5 | Request tally | 200 | Tally computed |
| 6 | Close proposal | 200 | State → Accepted |
| 7 | Get GovernanceReceipt | 200 | Ed25519 signature present |
| 8 | Get full governance record | 200 | Complete audit trail |
| 9 | Authorization boundary test | 401 | Write rejected without scope |
| 10 | Query decisions index | 200 | Decision visible in index |

**GovernanceReceipt evidence:**
```json
{
  "decision_hash": "...",
  "signature": { "algorithm": "Ed25519", "value": "..." },
  "timestamp": "...",
  "proposal_id": "..."
}
```

The GovernanceReceipt with Ed25519 signature is working live on K3s. PR #1327 appears merged and deployed.

---

## Flow 2 — BrightWorks Patronage

**Script:** `demo/scripts/flow-2-patronage.sh`
**Classification: FRAGILE**

### Command
```bash
cd /home/ubuntu/projects/icn && bash demo/scripts/flow-2-patronage.sh
```

### Result
Script exits with non-zero. Governance portion proven; ledger and receipt chain broken.

### Evidence
| Step | Action | HTTP | Result |
|------|--------|------|--------|
| 1–7 | Governance (proposal → vote → close → receipt) | 2xx | ✅ PROVEN |
| 8 | `POST /v1/ledger/brightworks-cooperative/payment` | 404 | Route not found |
| 9 | `GET /v1/ledger/brightworks-cooperative/balance/{did}` | 404 | Route not found |
| 10 | `GET /v1/ledger/brightworks-cooperative/history` | 2xx | ✅ History works |
| 11 | `GET /v1/receipts/allocations` (no decision_hash) | 400 | **Bug #1334** |

**Bug #1334 error (step 11):**
```
HTTP 400 — Query deserialize error: missing field `decision_hash`
```

**Root cause:** `ByDecisionQuery.decision_hash` is `String` (required). Actix-web rejects the request when the query param is absent. Field should be `Option<String>` — querying all allocations is a valid operation.

**Ledger 404 root cause:** Routes `/payment` and `/balance/{did}` appear renamed in the deployed binary. The `demo_api_preflight()` function reports `UNKNOWN` binary SHA (health endpoint git_sha not populated), so API surface detection cannot confirm the rename. Routes `/history` and `/position/{did}` appear correct. These 404s are a separate issue from #1334/#1335 and not in scope for Sprint 15 Phase B.

---

## Flow 3 — River City ↔ BrightWorks Federation

**Script:** `demo/scripts/flow-3-federation.sh`
**Classification: FRAGILE**

### Command
```bash
cd /home/ubuntu/projects/icn && bash demo/scripts/flow-3-federation.sh
```

### Result
Script exits with non-zero. Governance PROVEN; all federation API calls broken due to schema mismatch between demo script and deployed gateway API.

### Evidence
| Step | Action | HTTP | Result |
|------|--------|------|--------|
| 3 | River City governance (proposal → vote → close) | 2xx | ✅ PROVEN |
| 4 | BrightWorks governance (proposal → vote → close) | 2xx | ✅ PROVEN |
| 5a | `POST /v1/federation/coops` (register River City) | 400 | **Bug #1335** — schema mismatch |
| 5b | `POST /v1/federation/coops` (register BrightWorks) | 400 | **Bug #1335** — schema mismatch |
| 6a | `POST /v1/federation/coops/{id}/vouch` (River City) | 400 | **Bug #1335** — schema mismatch |
| 6b | `POST /v1/federation/coops/{id}/vouch` (BrightWorks) | 400 | **Bug #1335** — schema mismatch |
| 7 | `POST /v1/federation/clearing` | 400 | **Bug #1335** — missing field `agreement_id` |

**Bug #1335 schema mismatches:**

Step 5 — coop registration:
```json
// Demo sends (wrong):
{"coop_id":"...","name":"...","did":"<DID>"}

// Gateway expects (RegisterCoopRequest):
{"coop_id":"...","name":"...","public_did":"<DID>","gateway_endpoints":[],"capabilities":[]}
```
Fix: rename `did` → `public_did`. `gateway_endpoints` and `capabilities` have `#[serde(default)]` and can be omitted.

Step 6 — vouch:
```json
// Demo sends (wrong):
{"attested_by":"<DID>","attestation":"<text>"}

// Gateway expects (VouchRequest):
{"target_coop_id":"<coop_id>","trust_score":0.85,"expires_in_days":365}
```
Note: the handler uses the URL path `{coop_id}` for the actual target, but `target_coop_id` is still a required field in the body struct.

Step 7 — clearing creation:
```json
// Demo sends (wrong):
{"name":"...","parties":[...],"facilitator":"...","unit":"...","settlement_period_days":90}

// Gateway expects (CreateAgreementRequest):
{"agreement_id":"<client-id>","partner_coop_id":"<coop_id>","partner_did":"<DID>","max_imbalance":1000,"settlement":"monthly"}
```
The gateway API is bilateral (one partner, not a parties list). Fix: generate a client-side `agreement_id`, provide one `partner_coop_id`/`partner_did`, and use `settlement` enum string.

---

## Flow 4 — Finger Lakes CDN Reporting

**Script:** `demo/scripts/flow-4-reporting.sh`
**Classification: FRAGILE**

### Command
```bash
cd /home/ubuntu/projects/icn && bash demo/scripts/flow-4-reporting.sh
```

### Result
Script completes with warnings. Governance reporting PROVEN; receipt chain and federation broken.

### Evidence
| Step | Action | HTTP | Result |
|------|--------|------|--------|
| 1 | Harbor Homes governance records | 200 | ✅ PROVEN |
| 2 | BrightWorks governance records | 200 | ✅ PROVEN |
| 3 | River City governance records | 200 | ✅ PROVEN |
| 4 | Harbor Homes GovernanceReceipt | 200 | ✅ PROVEN (Ed25519 signature) |
| 5 | `GET /v1/receipts/chain` (no decision_hash) | 400 | Bug #1334 |
| 6 | `POST /v1/federation/coops` | 400 | Bug #1335 |
| 7 | `GET /v1/federation/coops` | 400 | Possibly uninitialized |
| 8 | Authorization boundary (write rejected) | 401 | ✅ PROVEN |

---

## Bug Index

| Bug | Issue | Location | Description | Fix |
|-----|-------|----------|-------------|-----|
| #1334 | [#1334](https://github.com/InterCooperative-Network/icn/issues/1334) | `icn-gateway/src/api/receipts.rs:18` | `ByDecisionQuery.decision_hash: String` (required) → 400 when param absent | Change to `Option<String>`, return all allocations when None |
| #1335 | [#1335](https://github.com/InterCooperative-Network/icn/issues/1335) | `demo/scripts/flow-3-federation.sh` | Demo sends wrong field names and body schemas for federation registration, vouch, and clearing | Update script to match deployed API contract |

---

## What Remains Before All 4 Flows Are PROVEN

1. **Fix Bug #1334** (s15-t2) — `decision_hash` optional in allocations/chain endpoints
2. **Fix Bug #1335** (s15-t3) — flow-3 federation schema corrections
3. **Investigate ledger 404s** — `/payment` and `/balance/{did}` routes missing (separate issue, not blocking Phase B)
4. **Re-run Flow 2 and Flow 3** after fixes to confirm PROVEN status

---

## Post-Fix Re-Audit (to be filled after s15-t2 and s15-t3 complete)

| Flow | Pre-fix | Post-fix |
|------|---------|----------|
| Flow 1 | PROVEN | — |
| Flow 2 | FRAGILE | TBD |
| Flow 3 | FRAGILE | TBD |
| Flow 4 | FRAGILE | TBD |
