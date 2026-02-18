# Execution Bridge Specification

**Status**: Authoritative Design Document
**Last Updated**: 2026-02-17
**Purpose**: Specifies how finalized governance decisions become executed economic actions.
**Audience**: Implementors wiring the governance-to-economics pipeline
**Depends On**: [economics/model-validation.md](economics/model-validation.md), [governance/model-validation.md](governance/model-validation.md)

---

## 1. Overview

The execution bridge converts governance decisions into economic state changes.
The provenance chain is:

```
GovernanceDecisionReceipt          (icn-governance)
  ├─ decision_hash: [u8; 32]       (canonical, cross-node)
  │
  ▼
AllocationReceipt                  (icn-kernel-api/src/receipts.rs)
  ├─ decision_hash                 (links back to governance)
  ├─ intents: Vec<SettlementIntent>
  │
  ▼
SettlementIntent                   (icn-kernel-api/src/economics.rs)
  ├─ decision_hash                 (same anchor)
  ├─ from, to, amount, unit
  ├─ asset: AssetType
  │
  ▼
JournalEntry                       (icn-ledger/src/types.rs)
  ├─ accounts: Vec<AccountDelta>   (balanced double-entry)
  ├─ parent_hash                   (Merkle-DAG link)
```

Two parallel paths currently exist:

| Path | Trigger | Executor | Status |
|------|---------|----------|--------|
| **Effect path** | Governance tally closes | `KernelGovernanceExecutor` | WIRED (treasury spend, protocol params, federation, membership, control) |
| **Receipt path** | Compute task completes | `SettlementEngine` | SCHEMA (engine tested, not called from runtime) |

---

## 2. Effect Path (Governance Decisions)

### 2.1 Event Source

Governance proposals that pass tally emit `KernelEffect` variants:

```
KernelEffect::Treasury(TreasuryEffect)
KernelEffect::Protocol(ProtocolEffect)
KernelEffect::Federation(FederationEffect)
KernelEffect::Control(ControlEffect)
KernelEffect::Membership(MembershipEffect)
KernelEffect::NoOp { reason }
```

**File**: `icn-kernel-api/src/effects.rs`
**Executor**: `icn-core/src/supervisor/governance_executor.rs`

### 2.2 Treasury Effect Flow

```
TreasuryEffect::Spend { treasury_did, recipient_did, amount, currency, memo, decision_hash }
  │
  ▼  treasury_effect_to_operation()  [governance_executor.rs:722]
  │
TreasuryOperation { treasury_id, operation_type, amount, currency, recipient, memo, decision_hash }
  │
  ▼  KernelTreasuryExecutor::execute_treasury_operation()  [governance_executor.rs:234]
  │
  ├── IF ledger service configured AND decision_hash present:
  │     TreasuryEntryRequest → LedgerService::submit_treasury_entry()
  │     → JournalEntry created in ledger
  │
  └── ELSE: placeholder success (no ledger mutation)
```

**Current state**: The LedgerService path is wired. When the governance executor has a ledger service injected (production mode), treasury spend effects create real journal entries.

### 2.3 Supported Treasury Effects

| Effect | Maps To | Wired? |
|--------|---------|--------|
| `TreasuryEffect::Spend` | `Spend` operation | YES - creates journal entry via LedgerService |
| `TreasuryEffect::CreateBudget` | `Allocate` operation | YES - creates journal entry |
| `TreasuryEffect::Allocate` | `Allocate` operation | YES (no decision_hash provenance) |
| `TreasuryEffect::Transfer` | `Spend` operation | YES (no decision_hash provenance) |
| Other variants | `Reserve` placeholder | NO - returns unmapped placeholder |

### 2.4 Protocol Effect Flow

```
ProtocolEffect::SetParameter { parameter_name, old_value_hash, new_value_json, effective_at }
  │
  ▼  protocol_effect_to_change()  [governance_executor.rs:804]
  │
ProtocolChange { parameter_name, old_value, new_value, effective_at }
  │
  ▼  KernelProtocolExecutor::apply_protocol_change()  [governance_executor.rs:370]
  │
  ├── Read current value from ProtocolParameterStore
  ├── Optimistic concurrency check (old_value matches current)
  ├── Parse new_value to match parameter type
  ├── Write updated parameter (durable via Sled)
  └── Return state_change_hash for provenance
```

**Concurrency guard**: If the parameter changed between proposal creation and tally, execution fails with "has changed since proposal". This prevents stale writes.

### 2.5 Unsupported Effects (Pilot V1)

| Effect | Reason | Alternative |
|--------|--------|-------------|
| `ProtocolEffect::Upgrade` | Requires migration coordination | Not available |
| `ProtocolEffect::SetSchedulingPolicy` | Works via EventBus side-channel | EventBus subscription |
| `FederationOperationType::LeaveFederation` | Not implemented with durable state | Manual |
| `FederationOperationType::EstablishClearing` | Not implemented with durable state | Manual |

---

## 3. Receipt Path (Compute Settlement)

### 3.1 SettlementEngine

**File**: `icn-ledger/src/settlement.rs`

Converts verified `ExecutionReceipt` (from compute layer) into `JournalEntry`:

```
SettlementRequest {
    receipt_hash: [u8; 32],    // opaque compute-layer hash
    executor: Did,              // earns credit
    submitter: Did,             // pays debit
    attester: Option<Did>,      // governance/audit
    scope: ScopeLevel,
    amount: i64,
    currency: String,
    executor_verified: bool,    // MUST be true
}
```

### 3.2 Invariants

1. **Verification required**: `executor_verified` must be `true` (caller verifies signatures)
2. **Scope routing**: Local/Cell/Org settle directly. Federation/Commons MUST use clearing.
3. **Positive amounts**: `amount > 0`
4. **No self-dealing**: `executor != submitter`
5. **Dedup**: `sha256("icn-ledger:settlement:v1:" || receipt_hash)` tracked in `HashSet`

### 3.3 Current Gap

The SettlementEngine is fully tested (14 tests) but **never called from the runtime**.
No code path converts an `ExecutionReceipt` into a `SettlementRequest` and invokes `settle_receipt()`.

---

## 4. Allocation Receipt Path (Budget Decisions)

### 4.1 AllocationReceipt

**File**: `icn-kernel-api/src/receipts.rs`

Links governance budget decisions to settlement intents:

```
AllocationReceipt {
    decision_hash: Hash,                    // links to GovernanceDecisionReceipt
    intents: Vec<SettlementIntent>,         // one or more economic actions
    scope: ScopeLevel,
    provenance: ProvenanceAnchors,          // upstream hash chain
}
```

### 4.2 SettlementIntent

**File**: `icn-kernel-api/src/economics.rs`

```
SettlementIntent {
    decision_receipt_id: String,
    decision_hash: Hash,
    asset: AssetType,                       // Fungible, Consumable, Service, Claim, etc.
    from: String,                           // source DID
    to: String,                             // destination DID
    amount: u64,
    unit: String,
    scope: ScopeLevel,
    memo: Option<String>,                   // excluded from canonical hash
    provenance: ProvenanceAnchors,
}
```

### 4.3 Builder

**File**: `icn-ledger/src/allocations.rs`

```rust
create_budget_allocation(
    decision_hash, proposal_id, treasury_did, recipient_did,
    amount, currency, purpose,
) -> AllocationReceipt
```

### 4.4 Current Gap

`create_budget_allocation()` produces an `AllocationReceipt` with `SettlementIntent`,
but nothing converts the intent's `(from, to, amount, unit)` into a `JournalEntry`
and appends it to the ledger. The conversion function does not exist yet.

---

## 5. Idempotency Rules

### 5.1 Effect Path

| Layer | Idempotency Mechanism |
|-------|----------------------|
| Governance tally | Each proposal executes effects exactly once on tally close |
| Protocol parameters | Optimistic concurrency (old_value check) prevents stale writes |
| Treasury entries | `decision_receipt_id` included in journal entry metadata |

**Gap**: No dedup guard on `TreasuryEntryRequest`. If the same decision is replayed (e.g., gossip redelivery), the same journal entry could be created twice.

### 5.2 Receipt Path

| Layer | Idempotency Mechanism |
|-------|----------------------|
| SettlementEngine | `sha256(DEDUP_PREFIX \|\| receipt_hash)` in `HashSet<[u8; 32]>` |

**Gap**: Dedup set is **in-memory only**. Node restart loses dedup state. Must persist to sled.

### 5.3 Allocation Path

No idempotency mechanism exists. The `AllocationReceipt.canonical_hash()` could serve as a dedup key, but no code checks it.

---

## 6. Canonical Hash Rules

All receipts implement `CanonicalReceipt` trait:

```rust
trait CanonicalReceipt {
    fn canonical_hash(&self) -> Hash;       // deterministic, cross-node equal
    fn receipt_id(&self) -> &ReceiptId;     // node-local, excluded from hash
    fn provenance(&self) -> &ProvenanceAnchors;
}
```

### Hash computation

- Serialization: `bincode::serialize()` (deterministic byte order)
- Hash function: `blake3::hash()` -> `[u8; 32]`
- Helper: `compute_canonical_hash<T: Serialize>(value: &T) -> Hash`

### Excluded from canonical hash

| Type | Excluded Fields |
|------|----------------|
| `SettlementIntent` | `intent_id` (node-local), `memo` |
| `AllocationReceipt` | `receipt_id`, `signature` |

### Order independence

`AllocationReceipt::canonical_hash()` sorts intent hashes before hashing.
Two nodes adding the same intents in different order produce the same canonical hash.

---

## 7. Failure Modes and Recovery

### 7.1 Effect Execution Failures

| Failure | Handling | Recovery |
|---------|----------|----------|
| Parameter concurrent modification | `ExecutionOutcome::Failed` with reason | Re-propose with current value |
| Treasury ledger submission error | `anyhow::Error` propagated | Governance records failure in tally |
| Federation service not configured | `ExecutionOutcome::Failed` | Configure service, re-execute |
| Membership service not configured | `ExecutionOutcome::Failed` | Configure service, re-execute |
| Unknown treasury effect variant | Maps to placeholder Reserve(0) | Add explicit mapping |

### 7.2 Settlement Failures

| Failure | Error Type | Recovery |
|---------|-----------|----------|
| Unverified receipt | `LedgerError::InvalidEntry` | Caller must verify signatures first |
| Federation/Commons scope | `LedgerError::InvalidEntry` | Route to clearing manager |
| Non-positive amount | `LedgerError::InvalidEntry` | Fix upstream |
| Self-dealing | `LedgerError::InvalidEntry` | Different executor/submitter required |
| Duplicate receipt | `LedgerError::DuplicateEntry` | Already settled, safe to ignore |
| Lock poisoned | `LedgerError::Internal` | Node restart |

### 7.3 Retry Policy

- **Effect path**: No automatic retry. Failed effects are recorded in governance tally results. Re-execution requires a new governance proposal.
- **Receipt path**: Caller may retry after transient failures (lock contention). Dedup prevents double-settlement on retry.
- **Allocation path**: Not yet implemented. Should follow receipt path pattern (dedup on canonical_hash).

---

## 8. Golden-Path Test Inventory

### 8.1 Effect Path Tests

| Test | File | What It Proves |
|------|------|---------------|
| `test_treasury_executor_placeholder` | `governance_executor.rs:1459` | Treasury effect produces Success outcome |
| `test_protocol_executor_apply_change` | `governance_executor.rs:1486` | Protocol param update writes to store |
| `test_protocol_executor_concurrent_modification` | `governance_executor.rs:1519` | Stale write detected and rejected |
| `test_protocol_upgrade_returns_explicit_failure` | `governance_executor.rs:1663` | Unsupported effects fail honestly |
| `test_protocol_set_scheduling_policy_returns_explicit_failure` | `governance_executor.rs:1694` | Pilot V1 limitations documented in tests |

### 8.2 Settlement Path Tests

| Test | File | What It Proves |
|------|------|---------------|
| `test_settle_creates_balanced_entry` | `settlement.rs:263` | Debit submitter + credit executor, balanced |
| `test_settle_deduplication` | `settlement.rs:301` | Same receipt_hash rejected on second attempt |
| `test_settle_federation_scope_rejected` | `settlement.rs:320` | Federation scope routed to clearing |
| `test_settle_self_dealing_rejected` | `settlement.rs:396` | executor == submitter blocked |
| `test_dedup_key_domain_separated` | `settlement.rs:476` | Dedup prefix prevents cross-feature collision |

### 8.3 Allocation Receipt Tests

| Test | File | What It Proves |
|------|------|---------------|
| `test_creates_valid_receipt_with_one_intent` | `allocations.rs:80` | Builder produces valid receipt |
| `test_decision_hash_propagates_to_intent` | `allocations.rs:89` | Provenance chain integrity |
| `test_canonical_hash_is_stable` | `allocations.rs:122` | Deterministic hashing |
| `test_allocation_receipt_canonical_hash_order_independent` | `receipts.rs:586` | Intent order does not affect hash |

### 8.4 Missing Golden-Path Tests

| Scenario | Why It Matters |
|----------|---------------|
| End-to-end governance proposal -> treasury journal entry | Proves the full pipeline works |
| AllocationReceipt -> JournalEntry conversion | Proves budget decisions become ledger mutations |
| SettlementEngine called from supervisor/gateway | Proves compute receipts settle |
| Dedup survives node restart | Proves persistence of settlement dedup |
| Concurrent settlement of same receipt from two gossip paths | Proves dedup under concurrency |

---

## 9. Minimum Viable Fixes for Pilot

### Fix 1: Settlement dedup persistence

**Problem**: `SettlementEngine.settled` is `RwLock<HashSet<[u8; 32]>>` (in-memory).
**Fix**: Add sled persistence with key prefix `settlement:dedup:`.
**Effort**: ~30 lines. Read settled set on startup, write on each settle.
**File**: `icn-ledger/src/settlement.rs`

### Fix 2: AllocationReceipt -> JournalEntry conversion

**Problem**: No function converts `SettlementIntent` fields into `JournalEntryBuilder` calls.
**Fix**: Add `settle_allocation(receipt: &AllocationReceipt, ledger: &mut Ledger)` that iterates intents, builds entries, and appends to ledger with dedup on `intent.canonical_hash()`.
**Effort**: ~50 lines.
**File**: New function in `icn-ledger/src/settlement.rs`

### Fix 3: Treasury effect dedup guard

**Problem**: `submit_treasury_entry()` has no idempotency check. Replayed governance decisions could create duplicate entries.
**Fix**: Track `decision_receipt_id` in a dedup set (same pattern as SettlementEngine).
**Effort**: ~20 lines.
**File**: `icn-core/src/supervisor/governance_executor.rs` or the LedgerService implementation

### Fix 4: Wire SettlementEngine into compute receipt handler

**Problem**: No runtime code converts `ExecutionReceipt` to `SettlementRequest`.
**Fix**: Add handler in compute receipt processing that calls `settle_receipt()`.
**Effort**: ~40 lines.
**Files**: `icn-core/src/supervisor/` (new compute settlement handler)

### Fix 5: End-to-end integration test

**Problem**: No test proves governance decision -> ledger entry across the full stack.
**Fix**: Integration test using `TestNode` that creates a treasury, submits a governance proposal, tallies it, and verifies the resulting journal entry.
**Effort**: ~100 lines.
**File**: `icn-core/tests/` (new integration test)

---

## 10. Decision Executor (New Component)

The Decision Executor is the missing layer that wraps the `EffectDispatcher` with persistent idempotency, saga state tracking, and execution receipts.

### 10.1 Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│  APP LAYER (icn-governance)                                         │
│  1. ProposalState::Accepted                                         │
│  2. GovernanceDecisionReceipt generated                             │
│  3. translate_payload_to_effects(payload) -> Vec<KernelEffect>      │
│  4. decision_hash computed from (proposal_id, payload, tally)       │
└───────────────────────────────┬─────────────────────────────────────┘
                                │ Vec<KernelEffect> + decision_hash
                                ▼
┌─────────────────────────────────────────────────────────────────────┐
│  DECISION EXECUTOR (new, in icn-core supervisor)                    │
│  5. Check execution_log[decision_hash]                              │
│     ├── Confirmed → SKIP (idempotent)                               │
│     ├── Failed (retries < max) → RETRY                              │
│     └── Not found → CREATE (status=Pending)                         │
│  6. Set status = Executing                                          │
│  7. EffectDispatcher.execute_effects(effects, receipt_id)           │
│  8. Collect Vec<EffectResult>                                       │
│     ├── All success → status=Confirmed, store entry IDs             │
│     └── Any failure → status=Failed, store error, increment retry   │
│  9. Generate ExecutionReceipt                                       │
└───────────────────────────────┬─────────────────────────────────────┘
                                │ KernelEffect per effect
                                ▼
┌─────────────────────────────────────────────────────────────────────┐
│  KERNEL LAYER (existing KernelGovernanceExecutor)                    │
│  10. Execute: Treasury→LedgerService, Protocol→ParamStore, etc.     │
│  11. Return EffectResult { success, message, state_change_hash }    │
└─────────────────────────────────────────────────────────────────────┘
```

### 10.2 ExecutionRecord Schema

```rust
pub struct ExecutionRecord {
    /// SHA-256 of (proposal_id || payload_bytes || tally_bytes).
    /// ONLY idempotency key. Survives retries, rebuilds, federation replay.
    pub decision_hash: [u8; 32],
    /// Governance proposal ID (human reference, not dedup).
    pub proposal_id: String,
    /// Current execution status.
    pub status: ExecutionStatus,
    /// When execution was first attempted.
    pub started_at: u64,
    /// When execution reached terminal state.
    pub finished_at: Option<u64>,
    /// Ledger entry IDs produced (empty until Confirmed).
    pub ledger_entry_ids: Vec<String>,
    /// State change hashes from each effect.
    pub state_change_hashes: Vec<String>,
    /// Error if Failed.
    pub error: Option<String>,
    /// Retry count.
    pub retries: u32,
    /// Links to GovernanceDecisionReceipt.
    pub decision_receipt_id: String,
}

pub enum ExecutionStatus {
    Pending,           // Record created, not yet started
    Executing,         // Effects being applied
    Confirmed,         // All succeeded. INVARIANT: never re-execute.
    Failed,            // May retry (retries < max)
    PermanentlyFailed, // Max retries exceeded. Manual intervention.
}
```

### 10.3 Idempotency Key: decision_hash

**Why decision_hash, not proposal_id**: Proposals can be re-submitted. Federation nodes may assign different local IDs. The decision_hash captures the exact combination of proposal + payload + vote outcome.

**Why not timestamp**: Non-deterministic. Two nodes processing the same decision must agree on the key.

**Computation**:
```rust
fn decision_hash(proposal_id: &str, payload: &[u8], tally: &[u8]) -> [u8; 32] {
    sha256(b"icn:decision:v1:" || proposal_id || b":" || payload || b":" || tally)
}
```

### 10.4 Retry Policy

| Category | Example | Retry? | Max |
|----------|---------|--------|-----|
| Transient | sled timeout, lock contention | Yes | 3 |
| Business logic | Insufficient funds | No | - |
| Invariant | Already confirmed | No (skip) | - |
| Config | Service not wired | No | - |

Backoff: 1s, 2s, 4s (exponential, base 1s).

### 10.5 Component Placement

| Component | Crate | Rationale |
|-----------|-------|-----------|
| `ExecutionRecord`, `ExecutionStatus` | `icn-kernel-api` | Shared types |
| `ExecutionLog` trait + sled impl | `icn-core` | Persistence near supervisor |
| `DecisionExecutor` | `icn-core` | Supervisor background task |
| Effect translation | `icn-governance` | Meaning firewall: app translates |

### 10.6 Receipt Chain

```
GovernanceDecisionReceipt        (existing, icn-governance)
  ├── decision_hash ─────────────┐
  ├── vote_tally                 │
  └── attestations               │ SAME KEY
                                 │
ExecutionReceipt                 │ (new, execution bridge)
  ├── decision_hash ─────────────┘
  ├── execution_hash
  ├── ledger_entry_ids ──────────┐
  └── executed_at                │ BACK-LINK
                                 │
JournalEntry                     │ (existing, icn-ledger)
  ├── id ────────────────────────┘
  ├── decision_receipt_id
  └── decision_hash
```

---

## 11. Implementation Sequence

### Phase 1: Skeleton (no behavior)

**Branch**: `feat/execution-bridge-skeleton`

1. `ExecutionRecord` + `ExecutionStatus` types in `icn-kernel-api`
2. `ExecutionLog` trait + sled impl in `icn-core`
3. `DecisionExecutor` struct with `process_decision()` stub
4. Wire into supervisor lifecycle (runs, does nothing)
5. Unit tests for record serialization + log CRUD

### Phase 2: Escrow Release Slice

**Branch**: `feat/executor-escrow-release`

1. Idempotency check in `process_decision()`
2. Status transitions: Pending -> Executing -> Confirmed/Failed
3. Wire `TreasuryEffect::Spend` through decision executor
4. `ExecutionReceipt` generation
5. Three integration tests:
   - Golden path: governance approve -> ledger entry created -> receipt generated
   - Failure path: insufficient funds -> escrow stays locked -> no partial state
   - Idempotency: replayed decision -> no double-disbursement

### Phase 3: Budget Enforcement Slice

**Branch**: `feat/executor-budget-enforcement`

1. New `TreasuryEffect::CreateBudget` variant (or extend existing)
2. Wire to `BudgetManager::create_budget()`
3. Budget validation in `Ledger::append_entry()`
4. Integration test: approved budget -> spending constrained

### Phase 4: Generalize

1. Effect translation for remaining `ProposalPayload` variants
2. `SurplusAllocation`, `ShareRedemption`, `DisputeResolution` effects
3. Federation effect execution beyond logging

---

## 12. Sequence Diagram: Full Golden Path

```
Governance        Effect           Treasury        Ledger
Tally             Executor         Executor        Service
  │                  │                │               │
  │  KernelEffect::  │                │               │
  │  Treasury(Spend) │                │               │
  ├─────────────────>│                │               │
  │                  │  TreasuryOp    │               │
  │                  ├───────────────>│               │
  │                  │                │  TreasuryEntry │
  │                  │                │  Request       │
  │                  │                ├──────────────>│
  │                  │                │               │  build JournalEntry
  │                  │                │               │  append to ledger
  │                  │                │               │  dedup check
  │                  │                │  entry_hash   │
  │                  │                │<──────────────┤
  │                  │  Success       │               │
  │                  │<───────────────┤               │
  │  EffectResult    │                │               │
  │<─────────────────┤                │               │
  │  (success=true)  │                │               │
```

---

## Document History

- 2026-02-17: Initial creation from codebase audit and architecture analysis
