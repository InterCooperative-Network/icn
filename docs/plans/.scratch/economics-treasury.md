# Economics / Treasury — Repo State & Gap Analysis

> Generated 2026-02-14 by economics analyst agent

## 1. Current State

### 1.1 Ledger Model

ICN uses **double-entry accounting** where every transaction must balance: `Σ debits == Σ credits` per currency.

| Type | Location | Description |
|------|----------|-------------|
| `JournalEntry` | `icn-ledger/src/types.rs:41` | Core entry: author, accounts[], parents[], nonce, decision provenance |
| `AccountDelta` | `icn-ledger/src/types.rs:103` | Per-account debit/credit in a single currency |
| `ContentHash` | `icn-ledger/src/types.rs:9` | SHA-256 hash for Merkle-DAG addressing |
| `AccountBalances` | `icn-ledger/src/types.rs:209` | Cached balance map per account (HashMap<String, i64>) |
| `Balance` | `icn-ledger/src/types.rs:199` | Single-currency balance |
| `CreditLimit` | `icn-ledger/src/types.rs:255` | Max negative balance allowed per participant |
| `Currency` | `icn-ledger/src/types.rs:166` | Currency definition: symbol, decimals, optional issuer |
| `JournalEntryBuilder` | `icn-ledger/src/entry.rs:9` | Builder with validation (double-entry invariant, positive amounts) |
| `WitnessPolicy` | `icn-ledger/src/types.rs:435` | BFT: None / Counterparty / Quorum / AllParties |
| `WitnessedEntry` | `icn-ledger/src/types.rs:607` | Entry + collected witness signatures |

**Key invariants enforced:**
- Double-entry balance: `validate_double_entry()` at `entry.rs:127`
- Positive amounts: `validate_positive_amounts()` at `entry.rs:153`
- Content hash computation via SHA-256 of canonical serialization
- Merkle-DAG linking via `parents: Vec<ContentHash>`
- Decision provenance: `decision_receipt_id` + `decision_hash` fields on JournalEntry

**Dispute system**: Complete model at `types.rs:332-420` with `DisputeStatus` (Normal→Contested→Resolved/Escalated), `DisputeOutcome` (Upheld/Reversed/Settlement/WriteOff), mediator assignment, and evidence tracking.

**Fork detection**: `QuarantineReason::ForkConflict` at `types.rs:286` handles lost fork resolution from Phase 18 Week 5. `QuarantinedEntry` struct at `types.rs:304` holds conflicting entries pending resolution.

### 1.2 Asset Types

6 asset types defined in `icn-kernel-api/src/economics.rs:17-53`:

| Variant | Description | Used in SettlementIntent→TreasuryEntryRequest? |
|---------|-------------|------------------------------------------------|
| `Fungible` | Standard mutual credit / currency | → `Spend` operation |
| `Consumable { expires_at }` | One-time use (compute hours, vouchers) | → `Spend` operation |
| `Depreciating { schedule }` | Equipment, licenses (Linear/Percentage/Custom) | → `Allocate` operation |
| `Transformable { outputs }` | Raw materials → products | → `Allocate` operation |
| `Service { duration_secs }` | Labor hours, non-storable | → `Spend` operation |
| `Claim { due_by, conditions }` | IOUs, bonds, receivables | → `IssueBond` operation |

**Default**: `AssetType::Fungible` (economics.rs:55-58).

**Conversion mapping** at `services.rs:694-701` deterministically maps AssetType → TreasuryOperationType.

**DepreciationSchedule** (economics.rs:62-78): `Linear { lifespan_secs }`, `Percentage { rate_bps, period_secs }`, `Custom(String)`.

**Hash stability**: All asset types have extensive canonical hash stability tests ensuring cross-node equality (20+ tests).

### 1.3 Settlement System

Two settlement paths:

#### Path 1: Local/Cell/Org Settlement (SettlementEngine)

| Type | Location | Description |
|------|----------|-------------|
| `SettlementEngine` | `icn-ledger/src/settlement.rs:86` | Converts verified receipts → balanced journal entries |
| `SettlementRequest` | `icn-ledger/src/settlement.rs:48` | DTO from compute layer (receipt_hash, executor, submitter, scope, amount) |

**Flow**: `SettlementRequest → SettlementEngine::settle_receipt() → JournalEntry`

**Validations** (settlement.rs:110-186):
1. `executor_verified` must be true
2. Scope must be Local/Cell/Org (Federation/Commons → rejected, must use clearing)
3. Amount must be positive
4. No self-dealing (executor ≠ submitter)
5. Dedup via `sha256("icn-ledger:settlement:v1:" || receipt_hash)` in-memory HashSet

**Creates balanced entry**: debit submitter, credit executor.

**Limitation**: Dedup set is in-memory only (`RwLock<HashSet>`) — NOT persisted to sled. Restart loses dedup state.

#### Path 2: Federation/Commons Settlement (ReceiptClearing)

| Type | Location | Description |
|------|----------|-------------|
| `ReceiptClearingManager` | `icn-federation/src/receipt_clearing.rs:109` | Batches cross-scope receipts |
| `ClearingReceipt` | `icn-federation/src/receipt_clearing.rs:42` | DTO for cross-scope settlement |
| `BatchClearingConfig` | `icn-federation/src/receipt_clearing.rs:78` | max_batch_size=100, max_batch_age=3600s |

**Flow**: `ClearingReceipt → ReceiptClearingManager → ClearingManager → CrossCoopTransfer`

**Federation**: Converts to `CrossCoopTransfer`, routed to bilateral `ClearingManager`.
**Commons**: **STUB** — logged but not processed, pending Epic 6 (#925).

**Validations**: scope must be Federation/Commons, different coops, cost > 0, no duplicate hashes in batch.

### 1.4 Treasury Operations

#### Treasury Struct (`icn-ledger/src/treasury.rs:115`)

```
Treasury {
  treasury_did: Did,              // Cooperative-owned account
  entity_id: Option<EntityId>,    // Type-safe entity reference (new)
  coop_id: String,                // Legacy cooperative identifier
  currency: String,               // Primary currency
  created_at: u64,                // Unix timestamp
  created_by: Did,                // Authorizing DID
  description: Option<String>,
  is_active: bool,
}
```

#### TreasuryManager (`icn-ledger/src/treasury.rs:210`)

Full-featured manager with persistent storage (sled-backed):

| Feature | Status |
|---------|--------|
| Register treasury | ✅ `register_treasury()` at treasury.rs:314 |
| Budget lifecycle | ✅ `create_budget()`, `spend_from_budget()`, `cancel_budget()` etc. in budgets.rs |
| Spending rules | ✅ `add_spending_rule()`, `requires_approval()` in approvals.rs |
| Velocity limits | ✅ `add_velocity_limit()`, `check_velocity_limit()` in approvals.rs |
| Audit trail | ✅ `record_operation()`, `get_audit_trail()` with pagination in audit.rs |
| Labor shares (Razeto) | ✅ `LaborShare`, `CooperativeBond`, `SurplusAllocation` types in labor_shares.rs |
| Sled persistence | ✅ `with_store()` + `load_from_store()` |

#### TreasuryOperationType (`icn-kernel-api/src/services.rs:647`)

7 operation types: `Spend`, `Allocate`, `Transfer`, `CreateBudget`, `DistributeSurplus`, `RedeemShares`, `IssueBond`

#### Governance Treasury Operations (`icn-governance/src/proposal.rs:712`)

`TreasuryProposalOperation` enum with 7 variants:
- `CreateBudget` — create budget from treasury
- `ModifySpendingRule` — change spending thresholds
- `Withdraw` — withdraw with governance approval
- `TransferBetweenBudgets` — move between budget categories
- `CancelBudget` — cancel + optionally return funds
- `ReclaimBudget` — reclaim unspent amounts
- `Spend` — direct treasury disbursement with **nonce-based replay protection** (proposal.rs:807-822)

### 1.5 Allocation → Settlement → Ledger Chain

**The full receipt chain is defined** (`icn-kernel-api/src/receipts.rs:119`):

```
DecisionReceipt → AllocationReceipt → SettlementIntent(s) → TreasuryEntryRequest → JournalEntry
```

| Link | Type | Location | Status |
|------|------|----------|--------|
| Decision → Allocation | `AllocationReceipt.decision_hash` | receipts.rs:127 | ✅ Defined |
| Allocation → Intent | `AllocationReceipt.intents: Vec<SettlementIntent>` | receipts.rs:130 | ✅ Defined |
| Intent → TreasuryEntry | `TreasuryEntryRequest::from_settlement_intent()` | services.rs:690 | ✅ Deterministic conversion |
| Intent+Allocation → TreasuryEntry | `TreasuryEntryRequest::from_settlement_with_allocation()` | services.rs:728 | ✅ Includes allocation hash in memo |
| TreasuryEntry → JournalEntry | `LedgerService::submit_treasury_entry()` | services.rs:566 | ✅ Trait defined, default returns error |
| JournalEntry carries provenance | `decision_receipt_id` + `decision_hash` fields | types.rs:90-99 | ✅ |

**Canonical hash invariants** (receipts.rs:70-95):
1. `canonical_hash()` MUST NOT depend on `receipt_id()` (node-local)
2. `canonical_hash()` MUST be deterministic (same content → same hash)
3. Two nodes with identical inputs MUST compute identical hashes
4. AllocationReceipt sorts intent hashes for order-independence (receipts.rs:160-164)

**CanonicalReceipt trait** at receipts.rs:77 — implemented by `AllocationReceipt`, `SettlementIntent`.

**Hash algorithm**: Blake3 over bincode serialization (`compute_canonical_hash()` at receipts.rs:100).

**Gateway receipt store**: `icn-gateway/src/receipt_store.rs` — sled-backed storage for `AllocationReceipt` and `SettlementIntent` with put/get/list_by_decision operations. Persistence test exists in `icn-core/tests/receipt_persistence_e2e.rs`.

### 1.6 Federation Clearing

| Type | Location | Description |
|------|----------|-------------|
| `ClearingManager` | `icn-federation/src/clearing_manager.rs` | Bilateral clearing engine |
| `BilateralAgreement` | `icn-federation/src/clearing.rs` | Clearing terms between two coops |
| `ClearingTerms` | `icn-federation/src/clearing.rs` | Settlement intervals, currencies, limits |
| `CrossCoopTransfer` | `icn-federation/src/clearing.rs` | Individual transfer in clearing batch |
| `NettingResult` | `icn-federation/src/clearing.rs` | Circular obligation detection result |
| `ReceiptClearingManager` | `icn-federation/src/receipt_clearing.rs:109` | Receipt → clearing bridge |

**Flow**:
1. Cross-scope receipts submitted to `ReceiptClearingManager`
2. Batched by config (max 100 receipts or 1 hour)
3. Flush converts to `CrossCoopTransfer`s
4. `ClearingManager` handles bilateral netting
5. Net positions settled via agreed mechanism

**Netting engine**: Detects circular obligations (A→B→C→A) for multilateral netting.

**Commons**: **STUBBED** — Commons scope receipts are logged but not cleared (pending Epic 6 #925).

### 1.7 Gateway Endpoints

#### Treasury API (`icn-gateway/src/api/treasury.rs`)

| Method | Path | Handler | Description |
|--------|------|---------|-------------|
| GET | `/v1/coops/{coop_id}/treasury` | `get_treasury_status` (line ~280) | Treasury status + balance |
| GET | `/v1/coops/{coop_id}/treasury/balance` | `get_treasury_balance` (line ~344) | Currency balances |
| GET | `/v1/coops/{coop_id}/treasury/budgets` | `list_budgets` (line ~395) | List budgets |
| GET | `/v1/coops/{coop_id}/treasury/budgets/{id}` | `get_budget` (line ~454) | Budget detail |
| GET | `/v1/coops/{coop_id}/treasury/rules` | `list_spending_rules` (line ~506) | Spending rules |
| POST | `/v1/coops/{coop_id}/treasury/proposals/spend` | `propose_spend` (line ~635) | Submit spend proposal |
| POST | `/v1/coops/{coop_id}/treasury/proposals/budget` | `propose_budget` (line ~692) | Submit budget proposal |
| GET | `/v1/coops/{coop_id}/treasury/audit` | `get_audit_trail` (line ~771) | Paginated audit trail |
| POST | `/v1/coops/{coop_id}/treasury/proposals/{id}/execute` | `execute_approved` (line ~893) | Execute approved proposal |

#### Receipts API (`icn-gateway/src/api/receipts.rs`)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/receipts/allocations/{hash}` | Get allocation receipt by hash |
| GET | `/v1/receipts/intents/{hash}` | Get settlement intent by hash |
| GET | `/v1/receipts/decision/{hash}` | Get all receipts for a decision |
| GET | `/v1/receipts/chain/{hash}` | Get full receipt chain (allocation + intents) |

#### Ledger endpoints (general)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/ledger/balances/{did}` | Account balance query |
| POST | `/v1/ledger/transfer` | Basic transfer |
| GET | `/v1/ledger/entries` | Transaction history |

### 1.8 Treasury-Coop Integration

**Current state**: Partially wired.

- `CoopActor` at `icn-coop/src/actor.rs:29` holds `treasury_manager: Option<TreasuryManagerHandle>`
- `TreasuryManagerHandle` = `Arc<RwLock<TreasuryManager>>` at actor.rs:20
- Supervisor creates treasury manager at `icn-core/src/supervisor/actors.rs:72`
- Supervisor passes it to CoopActor via `init_coop.rs:55`
- Treasury creation during coop activation: **EXISTS** but test at actor.rs:1495 shows it registers the treasury in TreasuryManager

**Treasury DID derivation**: `lifecycle.rs:7-26` derives deterministic treasury DID via `blake3("icn:treasury:{coop_id}")` → `did:icn:treasury:{base58}`. Keypair generated via `OsRng` (production) or deterministic (test feature gate). Cooperative struct stores DID string only, never the keypair.

**Double-spend protection**: `CoopStore::check_and_increment_treasury_nonce()` at `store.rs:108-206` uses **sled transaction** for atomic check-and-increment. Treasury spend proposals carry `expected_nonce` which must match stored nonce. `NonceMismatch` aborts if concurrent proposal executed first.

**Capital tracking**: `Cooperative.capital_pool: u64` with `add_capital()/remove_capital()` (saturating arithmetic). Members have `capital_contribution: u64` and `shares: u64`. Membership tiers define `profit_share_weight` for surplus distribution.

**Surplus distribution types**: `AssetDistributionPlan` at `types.rs:126-181` with `BalanceAction` (ReturnToMember/TransferToFederation/DonateToCommons), `DebtAction` (WriteOff/RequireSettlement/Transfer), and `CapitalReturnMethod` (ProRata/Equal/NoReturn).

**Community vs Cooperative**: `icn-community` has NO treasury primitives — only `ResourcePool` (compute/storage/credits as units, not financial). Clear architectural boundary: coops handle value, communities handle capacity.

**Gap**: Coop creation via `icnctl coop create` does NOT automatically derive and create a treasury DID (issue #1139). The `handle_create_treasury()` method exists at `actor.rs:442-511` and registers with TreasuryManager, but it's not called in the standard coop creation flow.

**Governance integration**:
- `GatewayTreasuryManager` at `icn-gateway/src/treasury_mgr.rs:49` wraps daemon's TreasuryManager
- Two modes: standalone (in-memory) or actor-backed (delegates to daemon)
- Treasury proposals flow through governance: `propose_spend()` → creates proposal → voting → `execute_approved()` → treasury effect

## 2. Money Flow Audit

### Scenario A: Simple transfer between members

```
POST /v1/ledger/transfer {from, to, amount, currency}
  → Gateway validates auth (JWT claims)
  → JournalEntryBuilder::new(from)
      .debit(from, currency, amount)
      .credit(to, currency, amount)
      .build()
  → validate_double_entry() ✅
  → validate_positive_amounts() ✅
  → compute_hash() → ContentHash
  → Ledger stores entry
  → Gossip replicates to peers
```

**What works**: Entry creation, validation, Merkle-DAG storage, gossip sync.
**What's missing**:
- ❌ No credit limit enforcement during transfer (CreditLimit type exists but not enforced in transfer path)
- ❌ No "why did this fail?" explainer — errors are generic LedgerError strings
- ⚠️ No transaction receipt returned to caller (just content hash)

### Scenario B: Governance-approved budget spend

```
POST /v1/coops/{id}/treasury/proposals/spend {amount, recipient, memo, nonce}
  → Create governance proposal with TreasuryProposalOperation::Spend
  → Members vote on proposal
  → Proposal reaches quorum → Decision
  → POST /v1/coops/{id}/treasury/proposals/{id}/execute
  → Translate ProposalPayload → KernelEffect::Treasury(TreasuryEffect::Spend{...})
  → KernelGovernanceExecutor::execute_effect()             ← governance_executor.rs:112
  → KernelTreasuryExecutor::execute_treasury_operation()   ← governance_executor.rs:233
  → LedgerService::submit_treasury_entry(TreasuryEntryRequest)  ← governance_executor.rs:265
  → JournalEntry with decision_receipt_id + decision_hash ← entry.rs:83-91
  → ✅ Provenance chain preserved
```

**What works**:
- ✅ Proposal creation, voting, decision
- ✅ Effect translation (TreasuryProposalOperation → KernelEffect → TreasuryOperation)
- ✅ Treasury executor with ledger integration (governance_executor.rs:252-289)
- ✅ Nonce-based replay protection for Spend operations
- ✅ Decision provenance preserved on JournalEntry
- ✅ Audit trail recorded

**What's BROKEN / incomplete**:
- ⚠️ `LedgerService::submit_treasury_entry()` default impl returns `Err("not supported")` (services.rs:570). Must be overridden by concrete implementation — **needs verification that Ledger actor actually implements this**.
- ❌ No AllocationReceipt created in the spend path — Decision goes directly to TreasuryEntryRequest, skipping the AllocationReceipt intermediate step
- ❌ No SettlementIntent created in the governance spend path — the conversion (services.rs:690) exists but isn't called from the governance executor

### Scenario C: Cross-org federation clearing

```
ExecutionReceipt (Federation scope)
  → Caller converts to ClearingReceipt
  → ReceiptClearingManager::submit_receipt()
  → Batch accumulates (max 100 or 1 hour)
  → ReceiptClearingManager::flush_to_clearing()
  → Each receipt → CrossCoopTransfer
  → ClearingManager processes via bilateral agreement
  → NettingEngine detects circular obligations
  → Net positions calculated
  → Settlement via agreed mechanism
```

**What works**:
- ✅ Receipt batching with dedup
- ✅ Scope routing (Local/Cell/Org → SettlementEngine, Federation/Commons → clearing)
- ✅ Bilateral agreement management
- ✅ Netting engine (circular obligation detection)

**What's BROKEN / incomplete**:
- ❌ **No auto-flush** — requires caller to poll `should_flush()` + call `flush_to_clearing()`. No background task.
- ❌ **Commons clearing stubbed** — logged but not processed
- ❌ No receipt chain (no AllocationReceipt in cross-org path)
- ❌ No gateway endpoint for triggering clearing flush
- ❌ No UX for bilateral agreement management

## 3. Gap Analysis

### Critical Gaps (Phase 0 - Legibility)

| # | Gap | User Story Blocked | Missing | Where | Receipt? | Phase |
|---|-----|-------------------|---------|-------|----------|-------|
| G1 | **No "why did this fail?" explainer** | "I transferred 100 hours and got an error. Why?" | Error context enrichment on transfer failure | `icn-gateway/src/api/ledger.rs` | N/A | 0 |
| G2 | **Transaction history lacks receipts** | "Show me my last 10 transactions with provenance" | Transaction list endpoint doesn't return decision provenance | `icn-gateway/src/api/ledger.rs` | ❌ | 0 |
| G3 | **Balance display missing credit limits** | "What's my balance and how much can I spend?" | Credit limit not returned with balance query | `icn-gateway/src/api/ledger.rs` | N/A | 0 |

### Important Gaps (Phase 1 - Functional Treasury)

| # | Gap | User Story Blocked | Missing | Where | Receipt? | Phase |
|---|-----|-------------------|---------|-------|----------|-------|
| G4 | **Treasury not auto-created with coop** | "Create a coop with a treasury" | Treasury DID derivation + creation in coop lifecycle | `icn-coop/src/actor.rs` | N/A | 1 |
| G5 | **AllocationReceipt not in spend path** | "Audit the receipt chain for this spend" | Governance spend skips AllocationReceipt creation | `icn-core/src/supervisor/governance_executor.rs` | ❌ Missing | 1 |
| G6 | **submit_treasury_entry() default is no-op** | "Execute an approved treasury spend" | Concrete Ledger implementation of submit_treasury_entry() | `icn-ledger/src/ledger.rs` | ❌ | 1 |
| G7 | **Surplus distribution not executable** | "Distribute year-end surplus to members" | TreasuryEffect::DistributeSurplus executor not wired to ledger | `governance_executor.rs` | ❌ | 1 |
| G8 | **SettlementEngine dedup not persisted** | "Restart node without duplicate settlements" | In-memory HashSet lost on restart | `icn-ledger/src/settlement.rs:89` | N/A | 1 |

### Federation Gaps (Phase 3)

| # | Gap | User Story Blocked | Missing | Where | Receipt? | Phase |
|---|-----|-------------------|---------|-------|----------|-------|
| G9 | **Commons clearing stubbed** | "Bill compute hours to commons pool" | Commons scope settlement engine | `icn-federation/src/receipt_clearing.rs` | ❌ | 3 |
| G10 | **No auto-flush for clearing** | "Cross-org receipts settle automatically" | Background task for periodic flush | `icn-federation/src/receipt_clearing.rs` | N/A | 3 |
| G11 | **No bilateral agreement UX** | "Set up clearing with partner coop" | Gateway endpoints for agreement CRUD | `icn-gateway/src/api/` | N/A | 3 |

## 4. Phase 0 Tasks (Legibility)

### P0-E1: Transaction history with decision provenance
- **What**: Extend `GET /v1/ledger/entries` to include `decision_receipt_id`, `decision_hash` in response
- **Where**: `icn-gateway/src/api/ledger.rs`
- **Receipt expectation**: Each transaction shows its governance origin if applicable
- **LOC**: ~50

### P0-E2: Balance + credit limit display
- **What**: Extend `GET /v1/ledger/balances/{did}` to include credit limit info
- **Where**: `icn-gateway/src/api/ledger.rs`, need to query `CreditLimit` data
- **LOC**: ~40

### P0-E3: Transfer failure explainer
- **What**: Return structured error responses with machine-readable codes and human explanations
- **Where**: `icn-gateway/src/error.rs` (GatewayError already has machine-readable codes), enrich ledger error mapping
- **Example**: `{ "code": "credit_limit_exceeded", "explanation": "Transfer of 500 hours exceeds your credit limit of 200 hours" }`
- **LOC**: ~80

## 5. Phase 1 Tasks (Functional Treasury)

### P1-E1: Treasury-Coop auto-creation (#1139)
- **What**: Derive treasury DID from coop identity during coop activation
- **Where**: `icn-coop/src/actor.rs`, supervisor init_coop.rs
- **Receipt expectation**: Treasury creation produces audit trail entry
- **Issues**: #1139 [B1]
- **LOC**: ~150

### P1-E2: AllocationReceipt in governance spend path
- **What**: Create AllocationReceipt + SettlementIntents when executing treasury effects
- **Where**: `icn-core/src/supervisor/governance_executor.rs:233-289`
- **Receipt expectation**: Complete chain: Decision → AllocationReceipt → SettlementIntent → JournalEntry
- **Issues**: #1141 [B3]
- **LOC**: ~100

### P1-E3: Concrete submit_treasury_entry() implementation
- **What**: Implement `LedgerService::submit_treasury_entry()` on concrete Ledger actor
- **Where**: `icn-ledger/src/ledger.rs` (wherever Ledger implements LedgerService trait)
- **Receipt expectation**: Returns entry_hash, decision_receipt_id, decision_hash echoed back
- **Issues**: Part of #1092 [PR9a]
- **LOC**: ~80

### P1-E4: Surplus distribution execution
- **What**: Wire `TreasuryEffect::DistributeSurplus` to create N journal entries (one per member)
- **Where**: `icn-core/src/supervisor/governance_executor.rs`
- **Receipt expectation**: Each distribution creates a provenance-linked JournalEntry
- **LOC**: ~120

### P1-E5: Persistent settlement dedup (#992)
- **What**: Persist SettlementEngine dedup set to sled, reload on startup
- **Where**: `icn-ledger/src/settlement.rs`
- **Receipt expectation**: N/A (infrastructure)
- **LOC**: ~60

## 6. Phase 3 Tasks (Federation Economics)

### P3-E1: Commons clearing engine
- **What**: Implement commons credit pool settlement (replace stub)
- **Where**: `icn-federation/src/receipt_clearing.rs` (currently logged + discarded)
- **Receipt expectation**: Commons receipts produce clearing entries with provenance
- **Issues**: Epic 6 #925
- **LOC**: ~200

### P3-E2: Auto-flush background task
- **What**: Actor/task that polls `should_flush()` and triggers `flush_to_clearing()`
- **Where**: `icn-core/src/supervisor/` (new actor or periodic task)
- **LOC**: ~80

### P3-E3: Bilateral agreement management UX
- **What**: Gateway endpoints for CRUD on bilateral clearing agreements
- **Where**: `icn-gateway/src/api/federation.rs` (new)
- **Endpoints**: `POST /v1/federations/{id}/agreements`, `GET /v1/federations/{id}/agreements`, etc.
- **LOC**: ~200

## 7. Relevant Open Issues

| Issue | Title | Labels | Relevance |
|-------|-------|--------|-----------|
| #1139 | [B1] Treasury-Coop Integration | P0, flow:C-treasury | **CRITICAL** — coops can't hold funds |
| #1140 | [B2] Asset Type Foundation | P1, flow:C-treasury | Asset metadata on JournalEntry |
| #1141 | [B3] Allocation Receipt Chain | P1, flow:C-treasury | Receipt chain completeness |
| #1092 | [PR9a] Gateway treasury + governance endpoints | P1, area:gateway | API surface for treasury |
| #1093 | [PR9b] TypeScript SDK treasury methods | P1, area:sdk | SDK coverage |
| #1094 | [PR9c] Flow C demo script | P1, type:doc | Treasury demo |
| #1090 | [PR8c] Fallback atomicity via CoopActor | P1, area:governance | Single-writer for treasury |
| #948 | Commons credit earning/spending | epic:commons-compute | Commons credit flow |
| #992 | Settlement receipt_hash in errors | epic:commons-compute | Error enrichment |
| #957 | Receipt settlement metrics | epic:commons-compute | Observability |
| #965 | Configurable credit formula via CCL | epic:commons-compute | Credit weights |
| #1134 | [E7] Commons compute governance hardening | epic:commons-compute | Governance+compute |
| #1147 | [EPIC] Vertical Slice | type:impl, P0 | End-to-end integration |
| #1145 | [D1] Vertical Slice Integration Test | type:test, P0 | Integration verification |

## 8. Summary: Money Flow Completeness

| Flow | Types ✅ | Conversion ✅ | Execution | Receipt Chain | Gateway |
|------|---------|--------------|-----------|---------------|---------|
| Simple transfer | ✅ | ✅ | ✅ Works | ❌ No provenance | ⚠️ Basic |
| Treasury spend | ✅ | ✅ | ⚠️ Depends on concrete impl | ❌ Skips AllocationReceipt | ✅ Endpoints exist |
| Budget spend | ✅ | ✅ | ⚠️ Same | ❌ Same | ✅ |
| Surplus distribution | ✅ | ✅ | ❌ Not wired | ❌ | ❌ |
| Federation clearing | ✅ | ✅ | ⚠️ No auto-flush | ❌ | ❌ No API |
| Commons clearing | ✅ | ❌ Stub | ❌ Stub | ❌ | ❌ |

**Bottom line**: The **type system and data model are ~90% complete**. All 6 asset types, canonical hashing, receipt chain types, and settlement engines exist. The **execution paths are ~60% complete** — governance → effect → treasury is wired, but the AllocationReceipt intermediate step is skipped and some concrete implementations need verification. The **UX surface is ~40% complete** — treasury endpoints exist but transfer/balance endpoints lack provenance and credit limit data.
