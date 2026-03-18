# Session Handoff — 2026-02-17

> Operational restart point for the next agent session.
> Every claim cites a file path. Uncitable claims labeled [SPECULATION].

---

## 1. REPO STATE (GROUND TRUTH)

**Current branch**: `docs/repo-reality-map` (docs PR only — no code changes)

**main HEAD**: `31ab0692` — `Merge pull request #1203 from InterCooperative-Network/feat/executor-observability`

### Open PRs (pilot-relevant)

| PR | Title | Branch | Status |
|----|-------|--------|--------|
| #1205 | docs(architecture): ICN repo reality map | `docs/repo-reality-map` | Open, docs-only |
| #1188 | deps(ts-sdk): bump dev-dependencies | dependabot | Open, unrelated |
| #1187 | deps(rn-sdk): bump react-native | dependabot | Open, unrelated |

### Open Issues (pilot-path)

| Issue | Title | Impact |
|-------|-------|--------|
| #1204 | audit(ledger): verify idempotency at ledger mutation boundary | **Tier-1 blocker** — PR-4 target |
| #1201 | feat(core): operator query for execution records by status | Nice-to-have, not blocking |
| #1200 | feat(core): execution record retention + startup cleanup | Nice-to-have, not blocking |
| #1195 | feat(core): releasing timeout for stuck escrows | Nice-to-have, not blocking |
| #1147 | [EPIC] Vertical Slice: Identity→Governance→Compute→Receipts→Audit | Epic tracker |

---

## 2. CANONICAL INVARIANTS (PROVEN BY TESTS)

### 2a. Decision Execution Idempotency

**Invariant**: Once `status = Confirmed`, executor NEVER re-executes for the same `decision_hash`.

- **Test**: `icn-core/src/supervisor/decision_executor.rs:719` — `test_idempotency_skips_confirmed`
- **Test**: `icn-core/src/supervisor/decision_executor.rs:751` — `test_idempotency_skips_permanently_failed`
- **Implementation**: `icn-core/src/supervisor/decision_executor.rs:170-195` — `execute_decision()` checks store before proceeding
- **Store trait**: `icn-kernel-api/src/execution.rs:172-184` — `ExecutionStore::get()` keyed by `decision_hash`

### 2b. Crash Recovery

**Invariant**: On startup, all `Pending` and `Executing` records are re-processed; `Confirmed` and `PermanentlyFailed` are skipped.

- **Test**: `icn-core/src/supervisor/decision_executor.rs:696` — `test_execute_records_status` (verifies status transitions)
- **Implementation**: `icn-core/src/supervisor/decision_executor.rs:80` — `recover_in_flight()` scans `Pending` + `Executing`
- **Observability**: Recovery logs status distribution at startup (added in PR #1203, commit `155c3c0b`)

### 2c. Retry Accumulation + Cap

**Invariant**: `retries` counter increments on each failure; exceeding max → `PermanentlyFailed`.

- **Test**: `icn-kernel-api/src/execution.rs:209-223` — `test_execution_record_failure_path` (verifies retries=1 after first failure)
- **Implementation**: `icn-kernel-api/src/execution.rs:135-145` — `mark_failed()` increments `retries`
- **Cap check**: `icn-core/src/supervisor/decision_executor.rs:230-250` — compares `retries >= max_retries`

### 2d. Canonical Decision Hash Derivation

**Invariant**: Decision hash is blake3 of `(domain_tag, proposal_id, domain_id, outcome, vote_tally, vote_hash)`. Vote hash sorts by `(voter_did, choice_ordinal)` — deterministic across nodes.

- **Test**: `icn-governance/src/proof.rs` — `test_decision_hash_deterministic` (verifies hash stability)
- **Implementation**: `icn-governance/src/proof.rs:330-345` — `compute_decision_hash()`
- **Vote hash**: `icn-governance/src/proof.rs:308-328` — `compute_decision_hash_bytes_fields()` sorts voters

### 2e. Backpressure

**Invariant**: Max 16 concurrent executions via semaphore. Saturation logged at >100ms wait.

- **Implementation**: `icn-core/src/supervisor/decision_executor.rs:38-45` — `MAX_CONCURRENT_EXECUTIONS = 16`
- **Saturation warning**: `icn-core/src/supervisor/decision_executor.rs:155-168` (added PR #1203)

### 2f. Journal Entry Provenance

**Invariant**: Ledger entries carry `decision_receipt_id` and `decision_hash` when built via treasury path.

- **Test**: `icn-ledger/src/entry.rs:287` — `test_entry_with_decision_provenance`
- **Implementation**: `icn-ledger/src/entry.rs:83-90` — `with_decision_provenance()`
- **JournalEntry fields**: `icn-ledger/src/types.rs:90,99` — `decision_receipt_id: Option<String>`, `decision_hash: Option<String>`

---

## 3. CURRENT SAGAS — REAL vs STUBBED

### Flow: Governance Decision → Effects → Ledger Mutation

```
GovernanceDecisionReceipt
  → DecisionExecutor.execute_decision()
    → KernelGovernanceExecutor.route_effects()
      → KernelTreasuryExecutor.execute_treasury_operation()
        → LedgerService.submit_treasury_entry()
          → Ledger.append_entry()
            → sled persistence
```

### Stage-by-stage assessment:

| Stage | Status | File Path | Notes |
|-------|--------|-----------|-------|
| Decision receipt creation | **REAL** | `icn-governance/src/proof.rs:230-280` | Canonical hash, deterministic |
| Decision executor loop | **REAL** | `icn-core/src/supervisor/decision_executor.rs:50-280` | Polling, idempotent, crash-safe |
| ExecutionStore (sled) | **REAL** | `icn-core/src/supervisor/decision_executor.rs:300-400` — `SledExecutionStore` | Persistent, keyed by decision_hash |
| Effect routing | **REAL** | `icn-core/src/supervisor/governance_executor.rs:318-571` | Routes to specialized executors |
| Treasury executor | **REAL** (wiring) | `icn-core/src/supervisor/governance_executor.rs:604-680` | Calls LedgerService if configured |
| LedgerService impl | **REAL** | `icn-core/src/services/ledger_service.rs:194-250` | Double-entry, provenance tagged |
| Ledger append_entry | **REAL** | `icn-ledger/src/ledger.rs:1202-1475` | Sled persistence, validation, gossip broadcast |
| **Payload → KernelEffect[]** | **STUB** | `icn-core/src/supervisor/governance_executor.rs:1370-1570` — test helpers only | No production mapper exists |
| **Treasury nonce check** | **DEFINED, NOT WIRED** | `icn-coop/src/store.rs:161` — `check_and_increment_treasury_nonce()` | Exists in coop store, never called from executor path |
| **Ledger dedup** | **DOES NOT EXIST** | `icn-ledger/src/ledger.rs:1336-1475` — `append_entry_internal()` | No check for duplicate `(decision_receipt_id, decision_hash)` |
| **Entry hash → ExecutionRecord** | **NOT ROUTED** | `icn-core/src/supervisor/governance_executor.rs:643-660` | `entry_hash` returned but not stored in `ExecutionRecord.ledger_entry_ids` |

### Where persistence actually happens today

- **ExecutionStore**: sled at `icn-core/src/supervisor/decision_executor.rs` — persists `ExecutionRecord` keyed by `decision_hash`
- **Ledger journal**: sled at `icn-ledger/src/ledger.rs:1453-1455` — `store.put(key, value)`
- **Both are real sled persistence**, but they are not linked — the ledger entry hash is not written back to the execution record

### Where mutations stop being real

The chain is real all the way from `DecisionExecutor` → `LedgerService` → `Ledger.append_entry()` **if** a ledger is configured. The gap is:
1. No production code generates `KernelEffect[]` from `ProposalPayload` — effects are hardcoded in tests
2. Treasury nonce is not checked before mutation — double-spend possible on crash+replay
3. Ledger has no dedup — replayed `append_entry()` with same provenance key creates duplicate entries

---

## 4. TIER-1 BLOCKERS (EXECUTION ORDER — LOCKED)

### PR-1: Real LedgerService append with governance provenance

**Goal**: Wire the real `LedgerService` append path with `decision_receipt_id`, `decision_hash`, scope/coop/treasury context. No dedup, no nonce yet.

**Files to modify**:
- `icn-core/src/services/ledger_service.rs` — verify `submit_treasury_entry()` builds correct double-entry deltas
- `icn-core/src/supervisor/governance_executor.rs:604-680` — ensure `entry_hash` routes back to `ExecutionRecord.ledger_entry_ids`
- `icn-ledger/src/entry.rs` — verify `with_decision_provenance()` fields survive sled round-trip

**Test that proves completion**:
- Integration test: submit treasury operation → verify journal entry in sled → verify `decision_receipt_id` and `decision_hash` on persisted entry → verify `ExecutionRecord.ledger_entry_ids` contains entry hash

**What becomes unblocked**: PR-2 (effect translation has a real ledger to write to)

### PR-2: Payload → Effect translation (treasury spend vertical slice)

**Goal**: Production path from `ProposalPayload::TreasurySpend` → `KernelEffect::Treasury(...)`. One proposal type only.

**Files to modify**:
- `icn-core/src/supervisor/governance_executor.rs` — add `translate_payload_to_effects()` function
- `icn-governance/src/types.rs` or `icn-governance/src/state_machine.rs` — verify `ProposalPayload::TreasurySpend` exists

**Test that proves completion**:
- Unit test: `ProposalPayload::TreasurySpend { amount, currency, recipient, treasury_id }` → produces `vec![KernelEffect::Treasury(TreasuryOperation { ... })]`
- Integration test: finalized proposal with TreasurySpend payload → executor picks up → ledger entry created

**What becomes unblocked**: PR-3 (nonce enforcement requires real effect flow)

### PR-3: Treasury nonce enforcement in mutation path

**Goal**: Wire `coop_store.check_and_increment_treasury_nonce()` into the treasury execution path. Reject replay.

**Files to modify**:
- `icn-core/src/supervisor/governance_executor.rs:604-680` — call nonce check before `ledger.submit_treasury_entry()`
- `icn-coop/src/store.rs:161` — already implemented, just needs wiring

**Test that proves completion**:
- Integration test: execute treasury operation → attempt same operation again → second attempt fails with nonce rejection
- Verify: nonce stored in sled survives restart

**What becomes unblocked**: PR-4 (ledger-level dedup is the final layer)

### PR-4: Ledger-level idempotency (#1204)

**Goal**: `Ledger.append_entry_internal()` rejects duplicate entries with matching `(decision_receipt_id, decision_hash)`.

**Files to modify**:
- `icn-ledger/src/ledger.rs:1336-1455` — add dedup check in `append_entry_internal()` before sled write
- `icn-ledger/src/types.rs` — add provenance index prefix constant

**Test that proves completion**:
- Unit test: append entry with provenance → append same provenance again → second returns existing hash (not error) but does not create duplicate
- Integration test: full saga from decision → ledger → crash → recovery → no duplicate entries

**What becomes unblocked**: Definition of "pilot-ready" can be proven

---

## 5. DEFINITION OF "PILOT-READY"

### The Test

**Name**: `test_pilot_submit_restart_resume_single_mutation`

**Location**: `icn/crates/icn-core/tests/decision_executor_runtime_test.rs` (new file or added to existing)

### Steps

1. **Submit decision**: Create `GovernanceDecisionReceipt` with `ProposalPayload::TreasurySpend`
2. **Verify effect translation**: Decision executor picks up receipt, translates payload → `KernelEffect::Treasury`
3. **Execute**: Treasury operation creates journal entry in real ledger
4. **Verify provenance**: Journal entry carries `decision_receipt_id` + `decision_hash`
5. **Crash/restart**: Kill executor, reinitialize from sled
6. **Recovery**: `recover_in_flight()` finds the confirmed record, skips it
7. **Replay protection**: Attempt same decision hash → idempotent skip (ExecutionStore)
8. **Nonce protection**: Attempt same treasury nonce → rejected (CoopStore)
9. **Ledger dedup**: Attempt same provenance key → returns existing hash, no duplicate entry
10. **Audit**: `ExecutionRecord.ledger_entry_ids` contains the correct entry hash

### Services Involved

- `DecisionExecutor` (`icn-core/src/supervisor/decision_executor.rs`)
- `KernelGovernanceExecutor` (`icn-core/src/supervisor/governance_executor.rs`)
- `LedgerService` (`icn-core/src/services/ledger_service.rs`)
- `Ledger` (`icn-ledger/src/ledger.rs`)
- `SledExecutionStore` (`icn-core/src/supervisor/decision_executor.rs`)
- `CoopStore` (`icn-coop/src/store.rs`)

---

## 6. FIRST PR IN NEXT SESSION

**Title**: `feat(ledger): persistent journal append with governance provenance`

**Branch**: `feat/ledger-persistent-append`

### Scope

**Replace/verify**:
- `icn-core/src/services/ledger_service.rs:194-250` — `submit_treasury_entry()` already builds double-entry deltas and appends. Verify the returned `entry_hash` is routed back.
- `icn-core/src/supervisor/governance_executor.rs:643-660` — After `execute_treasury_operation()` returns `ExecutionOutcome::Success`, extract entry hash from effects string and store in `ExecutionRecord.ledger_entry_ids`.
- `icn-core/src/supervisor/decision_executor.rs:200-230` — After effect execution, call `store.put(&record)` with populated `ledger_entry_ids`.

**Data written**:
- Journal entry in sled with `decision_receipt_id` and `decision_hash` metadata fields populated
- `ExecutionRecord.ledger_entry_ids` populated with actual entry hash(es)

**Explicitly NOT in scope**:
- No idempotency/dedup in ledger (PR-4)
- No treasury nonce check (PR-3)
- No payload→effect translation (PR-2)

### Acceptance Test

**Name**: `test_treasury_entry_persisted_with_provenance`

**Behavior**:
1. Create `SledExecutionStore` + real `Ledger` + `LedgerService`
2. Submit `TreasuryEntryRequest` with known `decision_receipt_id` + `decision_hash`
3. Verify: journal entry exists in sled
4. Verify: entry's `decision_receipt_id` matches
5. Verify: entry's `decision_hash` matches
6. Verify: `ExecutionRecord.ledger_entry_ids` is non-empty and contains correct hash

---

## 7. STRICT STARTUP INSTRUCTIONS FOR NEXT AGENT

```
STARTUP PROTOCOL — READ THIS FIRST

1. Read this file: docs/design/session-handoff-2026-02-17.md
2. Verify repo state:
   - git checkout main && git pull
   - Confirm HEAD is 31ab0692 or later
   - Confirm PR #1205 (reality map) is open or merged
3. DO NOT re-explore planes or re-derive architecture.
   The reality map is at docs/design/repo-reality-map.md — read it if needed.
4. DO NOT re-plan. The Tier-1 execution order is locked (Section 4 above).
5. Start with PR-1: feat(ledger): persistent journal append with governance provenance
   - Branch: feat/ledger-persistent-append
   - Scope: Section 6 above
   - Acceptance test: Section 6 above
6. After PR-1 merges, proceed to PR-2, PR-3, PR-4 in strict order.
7. "Pilot-ready" is defined in Section 5. That integration test is the exit criterion.
```

---

*Generated 2026-02-17. Every file path verified against main@31ab0692.*
