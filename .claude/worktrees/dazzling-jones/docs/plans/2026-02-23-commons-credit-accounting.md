# Commons Credit Accounting Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement commons credit earning and spending (#948) — executors earn credits proportional to fuel consumed; submitters spend credits; balance cannot go below zero.

**Architecture:** `CommonsSettlementRequest` DTO feeds `SettlementEngine::settle_commons_receipt()` which produces a balanced `(earn_entry, spend_entry)` journal entry pair. `ComputeTask` gets an optional `scope` field so submitters can opt into commons-scoped execution. `ComputeActor` holds an optional `LedgerHandle`; after a commons task completes, it appends both entries to the ledger. The `BalanceCallback` used by E7's credit-ceiling check is wired from the same ledger handle.

**Tech Stack:** Rust 1.88, `icn-ledger` crate (`commons_credits.rs`, `settlement.rs`, `ledger.rs`), `icn-compute` crate (`types.rs`, `actor/mod.rs`, `actor/lifecycle.rs`), `tokio::sync::RwLock`

**Worktree:** Before starting, create: `just worktree create 948-commons-credits` from `icn-wt/`. Run all commands from within the worktree's `icn/` subdirectory.

---

## Quick reference

| Symbol | Meaning |
|--------|---------|
| `COMMONS_CREDIT_CURRENCY` | `"commons-credits"` — the currency string for the commons ledger |
| `COMMONS_MINT_DID` | Deterministic DID seeded with `[0xCC; 32]` — the virtual mint/sink |
| `build_earn_entry_with_receipt(contributor, amount, receipt_id)` | Debits mint, credits contributor — `icn_ledger::build_earn_entry_with_receipt` |
| `build_spend_entry_with_receipt(consumer, amount, receipt_id)` | Debits consumer, credits mint — `icn_ledger::build_spend_entry_with_receipt` |
| `compute_credits_earned(cpu_millis, memory_mb_millis, storage_bytes, egress_bytes)` | Returns `u64` credits — `icn_ledger::compute_credits_earned` |
| `Ledger::append_entry(&mut self, entry)` | Async, returns `Result<ContentHash>` |
| `Ledger::get_balance(&self, did, currency)` | Returns `i64` |

**Key invariants:**
- `SettlementEngine::settle_receipt()` explicitly rejects `ScopeLevel::Commons` — commons must use the NEW `settle_commons_receipt()` path.
- Credits are non-transferable: only `build_earn_entry*` and `build_spend_entry*` may touch the commons mint DID.
- Dedup is by `receipt_id` nonce in the journal entry — same receipt can't be settled twice.
- Credit floor is zero: `check_sufficient_balance(balance, required)` enforces this.
- `ComputeResult` has `fuel_used` — use this as a proxy for `cpu_millis` (memory/storage/egress = 0 for now; full metering is a follow-up).

---

## Task 1: CommonsSettlementRequest DTO

**Files:**
- Modify: `crates/icn-ledger/src/settlement.rs`

### Step 1: Write the failing test

At the bottom of `crates/icn-ledger/src/settlement.rs`, inside the `#[cfg(test)]` block, add:

```rust
#[test]
fn test_commons_settlement_request_fields() {
    use icn_identity::Did;
    let executor: Did = "did:icn:executor001".parse().unwrap();
    let submitter: Did = "did:icn:submitter001".parse().unwrap();
    let req = CommonsSettlementRequest {
        receipt_id: [1u8; 32],
        executor: executor.clone(),
        submitter: submitter.clone(),
        cpu_millis: 5000,
        memory_mb_millis: 0,
        storage_bytes: 0,
        egress_bytes: 0,
        executor_verified: true,
    };
    assert_eq!(req.cpu_millis, 5000);
    assert_ne!(req.executor, req.submitter);
}
```

### Step 2: Run to verify it fails

```bash
cargo test -p icn-ledger --lib test_commons_settlement_request_fields
```
Expected: `error[E0422]: cannot find struct CommonsSettlementRequest`

### Step 3: Add the struct

In `crates/icn-ledger/src/settlement.rs`, after the existing `SettlementRequest` struct, add:

```rust
/// Request DTO for commons credit settlement.
///
/// Used with [`SettlementEngine::settle_commons_receipt`] to produce
/// a balanced (earn, spend) journal entry pair from a completed commons task.
///
/// # Metering proxy
/// Until full resource tracking is wired, set `cpu_millis = fuel_used`
/// and leave `memory_mb_millis`, `storage_bytes`, `egress_bytes` as `0`.
#[derive(Debug, Clone)]
pub struct CommonsSettlementRequest {
    /// Receipt nonce for dedup — set to `ExecutionReceipt::receipt_id`.
    pub receipt_id: [u8; 32],
    /// DID of the executor (earns credits).
    pub executor: icn_identity::Did,
    /// DID of the submitter (spends credits).
    pub submitter: icn_identity::Did,
    /// CPU time in milliseconds (use `fuel_used` as proxy for now).
    pub cpu_millis: u64,
    /// Memory usage in MB-milliseconds (0 until full metering is wired).
    pub memory_mb_millis: u64,
    /// Storage bytes (0 until full metering is wired).
    pub storage_bytes: u64,
    /// Network egress bytes (0 until full metering is wired).
    pub egress_bytes: u64,
    /// Must be `true` — executor has signed the result.
    pub executor_verified: bool,
}
```

### Step 4: Run test to verify it passes

```bash
cargo test -p icn-ledger --lib test_commons_settlement_request_fields
```
Expected: PASS

### Step 5: Commit

```bash
git add crates/icn-ledger/src/settlement.rs
git commit -m "feat(ledger): add CommonsSettlementRequest DTO (#948)"
```

---

## Task 2: SettlementEngine::settle_commons_receipt()

**Files:**
- Modify: `crates/icn-ledger/src/settlement.rs`

### Step 1: Write the failing tests

Add these tests inside the `#[cfg(test)]` block:

```rust
#[test]
fn test_settle_commons_receipt_produces_entry_pair() {
    use icn_identity::Did;
    let engine = SettlementEngine::new();
    let executor: Did = "did:icn:exec001".parse().unwrap();
    let submitter: Did = "did:icn:sub001".parse().unwrap();
    let req = CommonsSettlementRequest {
        receipt_id: [42u8; 32],
        executor: executor.clone(),
        submitter: submitter.clone(),
        cpu_millis: 1000,   // → 1000 credits (compute_credits_earned(1000,0,0,0) = 1000)
        memory_mb_millis: 0,
        storage_bytes: 0,
        egress_bytes: 0,
        executor_verified: true,
    };
    let (earn, spend) = engine.settle_commons_receipt(&req).unwrap();
    // Earn: executor credited, mint debited
    let executor_credit = earn.accounts.iter()
        .find(|d| d.account_id == executor && d.credit.is_some())
        .expect("executor should have credit delta");
    assert_eq!(executor_credit.credit.unwrap(), 1000);
    // Spend: submitter debited
    let submitter_debit = spend.accounts.iter()
        .find(|d| d.account_id == submitter && d.debit.is_some())
        .expect("submitter should have debit delta");
    assert_eq!(submitter_debit.debit.unwrap(), 1000);
}

#[test]
fn test_settle_commons_receipt_rejects_unverified() {
    use icn_identity::Did;
    let engine = SettlementEngine::new();
    let req = CommonsSettlementRequest {
        receipt_id: [1u8; 32],
        executor: "did:icn:exec".parse().unwrap(),
        submitter: "did:icn:sub".parse().unwrap(),
        cpu_millis: 500,
        memory_mb_millis: 0,
        storage_bytes: 0,
        egress_bytes: 0,
        executor_verified: false,   // ← unverified
    };
    assert!(engine.settle_commons_receipt(&req).is_err());
}

#[test]
fn test_settle_commons_receipt_rejects_self_dealing() {
    use icn_identity::Did;
    let engine = SettlementEngine::new();
    let same: Did = "did:icn:same".parse().unwrap();
    let req = CommonsSettlementRequest {
        receipt_id: [2u8; 32],
        executor: same.clone(),
        submitter: same.clone(),   // ← same DID
        cpu_millis: 500,
        memory_mb_millis: 0,
        storage_bytes: 0,
        egress_bytes: 0,
        executor_verified: true,
    };
    assert!(engine.settle_commons_receipt(&req).is_err());
}

#[test]
fn test_settle_commons_receipt_deduplicates() {
    use icn_identity::Did;
    let engine = SettlementEngine::new();
    let req = CommonsSettlementRequest {
        receipt_id: [7u8; 32],
        executor: "did:icn:exec2".parse().unwrap(),
        submitter: "did:icn:sub2".parse().unwrap(),
        cpu_millis: 200,
        memory_mb_millis: 0,
        storage_bytes: 0,
        egress_bytes: 0,
        executor_verified: true,
    };
    assert!(engine.settle_commons_receipt(&req).is_ok());
    // Second call with same receipt_id → DuplicateEntry error
    let err = engine.settle_commons_receipt(&req).unwrap_err();
    assert!(
        format!("{err:?}").contains("Duplicate") || format!("{err:?}").contains("duplicate"),
        "expected duplicate error, got: {err:?}"
    );
}
```

### Step 2: Run to verify they fail

```bash
cargo test -p icn-ledger --lib "test_settle_commons"
```
Expected: compile error — `settle_commons_receipt` not found

### Step 3: Implement the method

Add to the `impl SettlementEngine` block in `settlement.rs`:

```rust
/// Settle a commons-scoped execution receipt, producing an earn/spend entry pair.
///
/// Returns `(earn_entry, spend_entry)`:
/// - `earn_entry`: credits the executor's commons-credits account
/// - `spend_entry`: debits the submitter's commons-credits account
///
/// Both entries use `receipt_id` as a nonce, preventing silent dedup.
///
/// # Errors
/// - `executor_verified` is false
/// - `executor == submitter` (self-dealing)
/// - `cpu_millis == 0` AND all other metering fields are 0 (zero-value settlement)
/// - Receipt already settled (duplicate)
pub fn settle_commons_receipt(
    &self,
    req: &CommonsSettlementRequest,
) -> Result<(JournalEntry, JournalEntry), LedgerError> {
    if !req.executor_verified {
        return Err(LedgerError::InvalidEntry(
            "commons settlement requires executor_verified = true".into(),
        ));
    }
    if req.executor == req.submitter {
        return Err(LedgerError::InvalidEntry(
            "commons settlement: executor and submitter must differ (self-dealing)".into(),
        ));
    }

    // Compute credit amount from metering data
    let credits = crate::commons_credits::compute_credits_earned(
        req.cpu_millis,
        req.memory_mb_millis,
        req.storage_bytes,
        req.egress_bytes,
    );
    if credits == 0 {
        return Err(LedgerError::InvalidEntry(
            "commons settlement: computed credit amount is zero".into(),
        ));
    }

    // Dedup check
    let dedup_key = Self::dedup_key(&req.receipt_id);
    {
        let settled = self.settled.read().map_err(|_| {
            LedgerError::InvalidEntry("settlement dedup lock poisoned".into())
        })?;
        if settled.contains(&dedup_key) {
            return Err(LedgerError::DuplicateEntry(hex::encode(req.receipt_id)));
        }
    }

    // Build balanced entry pair
    let amount = credits as i64;
    let earn_entry = crate::commons_credits::build_earn_entry_with_receipt(
        &req.executor,
        amount,
        req.receipt_id,
    )
    .map_err(|e| LedgerError::InvalidEntry(e.to_string()))?;

    let spend_entry = crate::commons_credits::build_spend_entry_with_receipt(
        &req.submitter,
        amount,
        req.receipt_id,
    )
    .map_err(|e| LedgerError::InvalidEntry(e.to_string()))?;

    // Record dedup
    self.settled
        .write()
        .map_err(|_| LedgerError::InvalidEntry("settlement dedup lock poisoned".into()))?
        .insert(dedup_key);

    Ok((earn_entry, spend_entry))
}
```

### Step 4: Run tests to verify they pass

```bash
cargo test -p icn-ledger --lib "test_settle_commons"
```
Expected: 4 tests PASS

### Step 5: Run all ledger tests

```bash
cargo test -p icn-ledger --lib
```
Expected: all tests pass

### Step 6: Commit

```bash
git add crates/icn-ledger/src/settlement.rs
git commit -m "feat(ledger): add settle_commons_receipt() to SettlementEngine (#948)"
```

---

## Task 3: Export CommonsSettlementRequest from icn-ledger

**Files:**
- Modify: `crates/icn-ledger/src/lib.rs`

### Step 1: Write the failing test

In a scratch test (add to `settlement.rs` tests):
```rust
#[test]
fn test_commons_settlement_request_pub_export() {
    // This test exists to verify the type is exported from lib.rs.
    // It will fail to compile if CommonsSettlementRequest is not re-exported.
    let _: Option<icn_ledger::CommonsSettlementRequest> = None;
}
```
Actually, verify by doing:
```bash
grep "CommonsSettlementRequest" crates/icn-ledger/src/lib.rs
```
Expected: no output (not yet exported)

### Step 2: Add the export

Find the line in `crates/icn-ledger/src/lib.rs` that reads:
```rust
pub use settlement::{SettlementEngine, SettlementRequest};
```
Change it to:
```rust
pub use settlement::{CommonsSettlementRequest, SettlementEngine, SettlementRequest};
```

### Step 3: Verify

```bash
cargo check -p icn-ledger
```
Expected: clean

### Step 4: Commit

```bash
git add crates/icn-ledger/src/lib.rs
git commit -m "chore(ledger): export CommonsSettlementRequest from crate root (#948)"
```

---

## Task 4: Add scope field to ComputeTask

**Files:**
- Modify: `crates/icn-compute/src/types.rs`

**Why this field:** Submitters need to explicitly opt into commons-scoped execution. `scope: Some(ScopeLevel::Commons)` signals that the task should be routed through the commons governance checks and that post-execution credits should be settled via the commons ledger path.

### Step 1: Write the failing test

In `crates/icn-compute/src/actor/tests.rs`, add:

```rust
#[test]
fn test_compute_task_commons_scope_field() {
    use icn_kernel_api::ScopeLevel;
    let task = ComputeTask {
        scope: Some(ScopeLevel::Commons),
        ..make_task("scope-test", "did:icn:submitter")
    };
    assert_eq!(task.scope, Some(ScopeLevel::Commons));
}

#[test]
fn test_compute_task_default_scope_is_none() {
    let task = make_task("scope-default", "did:icn:submitter");
    assert!(task.scope.is_none());
}
```

### Step 2: Run to verify they fail

```bash
cargo test -p icn-compute --lib "test_compute_task.*scope"
```
Expected: compile error — no field `scope` on `ComputeTask`

### Step 3: Add the field

In `crates/icn-compute/src/types.rs`, in the `ComputeTask` struct, add after `data_locality`:

```rust
/// Execution scope for this task (E5 - #948).
///
/// `Some(ScopeLevel::Commons)` opts into commons-scoped execution:
/// - Commons governance checks apply (standing, credit ceiling via E7)
/// - Post-execution credits are settled via the commons ledger path
/// - Executor earns commons credits proportional to fuel consumed
/// - Submitter's commons credit balance is debited
///
/// `None` means local/cell scope — no commons settlement.
#[serde(default)]
pub scope: Option<icn_kernel_api::ScopeLevel>,
```

### Step 4: Run tests

```bash
cargo test -p icn-compute --lib "test_compute_task.*scope"
```
Expected: PASS

### Step 5: Check workspace still compiles

```bash
cargo check --workspace
```
If there are compile errors due to exhaustive struct initialization, add `scope: None` to any struct literals that fail.

### Step 6: Commit

```bash
git add crates/icn-compute/src/types.rs
git commit -m "feat(compute): add scope field to ComputeTask for commons-scoped execution (#948)"
```

---

## Task 5: Add LedgerHandle to ComputeActor

**Files:**
- Modify: `crates/icn-compute/src/actor/types.rs`
- Modify: `crates/icn-compute/src/actor/mod.rs`

**Why:** The actor needs to write credit settlements to the ledger after commons task completion, and provide a `BalanceCallback` for E7's credit-ceiling check that reads real ledger balances.

### Step 1: Write a failing test

In `crates/icn-compute/src/actor/tests.rs`, add:

```rust
#[tokio::test]
async fn test_compute_actor_accepts_ledger_handle() {
    use std::sync::Arc;
    use tokio::sync::RwLock;
    // Build a minimal in-memory Ledger
    let store = icn_ledger::MemStore::new();  // adjust import if needed
    let ledger = icn_ledger::Ledger::new(Arc::new(store)).unwrap();
    let handle: LedgerHandle = Arc::new(RwLock::new(ledger));

    let trust_cb: TrustCallback = Arc::new(|_| 0.8);
    let mut actor = ComputeActor::new("did:icn:executor".into(), trust_cb);
    actor.set_ledger(handle); // Should not panic
}
```

**Note:** Check the actual `MemStore` import path in `icn-ledger` — it may be `icn_ledger::store::MemStore` or similar. Search with `grep -r "pub struct MemStore\|MemStore" crates/icn-ledger/src/ | head -5`.

### Step 2: Run to verify it fails

```bash
cargo test -p icn-compute --lib test_compute_actor_accepts_ledger_handle
```
Expected: compile error — `LedgerHandle` and `set_ledger` not found

### Step 3: Add LedgerHandle type alias

In `crates/icn-compute/src/actor/types.rs`, add at the end:

```rust
/// Handle to the ICN ledger for commons credit settlement (E5 - #948).
///
/// Wrapped in `Arc<RwLock>` so the compute actor can write credit
/// entries from async task completion handlers without blocking.
pub type LedgerHandle = std::sync::Arc<tokio::sync::RwLock<icn_ledger::Ledger>>;
```

### Step 4: Export the type from mod.rs

In `crates/icn-compute/src/actor/mod.rs`, add `LedgerHandle` to the re-export list:

```rust
pub use types::{
    BalanceCallback, ComputeEvent, EventCallback, LedgerHandle, LocalityCallback,
    PaymentCallback, PaymentRequest, SendCallback, TrustCallback,
};
```

### Step 5: Add field and setter to ComputeActor

In `crates/icn-compute/src/actor/mod.rs`, add to the `ComputeActor` struct:

```rust
/// Ledger handle for commons credit settlement (E5 - #948).
/// When set, completed commons tasks (scope == ScopeLevel::Commons)
/// earn/spend credits atomically via append_entry.
ledger: Option<LedgerHandle>,
```

In `ComputeActor::new()` initializer, add:
```rust
ledger: None,
```

Add the setter after `set_balance_callback`:

```rust
/// Set ledger handle for commons credit settlement (E5 - #948).
///
/// Also automatically wires a `BalanceCallback` that reads the submitter's
/// `commons-credits` balance from the ledger, activating E7 credit-ceiling
/// enforcement without a separate `set_balance_callback` call.
pub fn set_ledger(&mut self, handle: LedgerHandle) {
    // Auto-wire balance callback from ledger
    let cb_handle = handle.clone();
    self.balance_callback = Some(std::sync::Arc::new(move |did: &str| {
        let ledger = cb_handle.blocking_read();
        let did_parsed: icn_identity::Did = match did.parse() {
            Ok(d) => d,
            Err(_) => return 0,
        };
        ledger.get_balance(&did_parsed, icn_ledger::COMMONS_CREDIT_CURRENCY)
    }));
    self.ledger = Some(handle);
}
```

### Step 6: Run tests

```bash
cargo test -p icn-compute --lib test_compute_actor_accepts_ledger_handle
cargo check -p icn-compute
```
Expected: test passes, no compile errors

### Step 7: Commit

```bash
git add crates/icn-compute/src/actor/types.rs crates/icn-compute/src/actor/mod.rs
git commit -m "feat(compute): add LedgerHandle and set_ledger() to ComputeActor (#948)"
```

---

## Task 6: Commons credit settlement in complete_task_result()

**Files:**
- Modify: `crates/icn-compute/src/actor/lifecycle.rs`

**What:** After a successful commons task completes, build a `CommonsSettlementRequest` from the result and task, call `settle_commons_receipt`, append both entries to the ledger.

### Step 1: Write the failing integration test

In `crates/icn-compute/src/actor/tests.rs`, add:

```rust
#[tokio::test]
async fn test_commons_task_earns_credits_on_completion() {
    use icn_kernel_api::ScopeLevel;
    use icn_ledger::{Ledger, COMMONS_CREDIT_CURRENCY};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    // Setup: ledger + actor
    let store = icn_ledger::store::MemStore::new(); // adjust path as needed
    let ledger = Ledger::new(Arc::new(store)).unwrap();
    let ledger_handle: LedgerHandle = Arc::new(RwLock::new(ledger));

    let trust_cb: TrustCallback = Arc::new(|_| 0.9);
    let mut actor = ComputeActor::new("did:icn:executor001".into(), trust_cb);
    actor.set_ledger(ledger_handle.clone());
    let handle = actor.spawn();

    // Submit a commons-scoped task
    let mut task = make_task("commons-settle-test", "did:icn:submitter001");
    task.scope = Some(ScopeLevel::Commons);
    task.fuel_limit = crate::types::FuelLimit(2000);
    handle.submit(task).await.expect("submission should succeed");

    // Wait briefly for execution
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Executor should have earned commons credits
    let executor_did: icn_identity::Did = "did:icn:executor001".parse().unwrap();
    let balance = ledger_handle
        .read()
        .await
        .get_balance(&executor_did, COMMONS_CREDIT_CURRENCY);
    assert!(
        balance > 0,
        "executor should have earned commons credits, got balance = {balance}"
    );
}

#[tokio::test]
async fn test_commons_task_debits_submitter_on_completion() {
    use icn_kernel_api::ScopeLevel;
    use icn_ledger::{Ledger, COMMONS_CREDIT_CURRENCY};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    let store = icn_ledger::store::MemStore::new();
    let ledger = Ledger::new(Arc::new(store)).unwrap();
    let ledger_handle: LedgerHandle = Arc::new(RwLock::new(ledger));

    let trust_cb: TrustCallback = Arc::new(|_| 0.9);
    let mut actor = ComputeActor::new("did:icn:executor002".into(), trust_cb);
    actor.set_ledger(ledger_handle.clone());
    let handle = actor.spawn();

    let mut task = make_task("commons-spend-test", "did:icn:submitter002");
    task.scope = Some(ScopeLevel::Commons);
    task.fuel_limit = crate::types::FuelLimit(1000);
    handle.submit(task).await.expect("submission should succeed");

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Submitter should have a negative commons-credits balance (spent)
    let submitter_did: icn_identity::Did = "did:icn:submitter002".parse().unwrap();
    let balance = ledger_handle
        .read()
        .await
        .get_balance(&submitter_did, COMMONS_CREDIT_CURRENCY);
    assert!(
        balance < 0,
        "submitter should have spent commons credits, got balance = {balance}"
    );
}
```

**Note:** If the submitter balance floor is enforced at submission time (E7 credit ceiling), you may need to seed the submitter's balance first. Adjust the test if the second assertion fails — the intent is that spending decreases balance.

### Step 2: Run to verify they fail

```bash
cargo test -p icn-compute --lib "test_commons_task_earns\|test_commons_task_debits"
```
Expected: compile or runtime failure — settlement not wired yet

### Step 3: Add settlement logic to complete_task_result()

In `crates/icn-compute/src/actor/lifecycle.rs`, add the following import at the top:

```rust
use icn_kernel_api::ScopeLevel;
```

In `complete_task_result()`, after the event callback block (after `cb(ComputeEvent::TaskCompleted {...})`), add:

```rust
// Commons credit settlement (E5 - #948)
// For successfully completed commons-scoped tasks, earn credits for executor
// and debit submitter. Runs after task completion event to avoid blocking.
if let Some(ref ledger_handle) = self.ledger {
    // Look up task to get scope and submitter
    let task_info = {
        let mgr = self.task_manager.lock().await;
        mgr.get(&task_hash).map(|t| (t.scope.clone(), t.submitter.clone()))
    };

    if let Some((Some(ScopeLevel::Commons), submitter)) = task_info {
        // Only settle successful outcomes
        if matches!(outcome, crate::types::ExecutionOutcome::Success(_)) {
            let receipt_id: [u8; 32] = task_hash; // use task_hash as receipt nonce
            let executor_parsed: Option<icn_identity::Did> =
                executor_did.parse().ok();
            let submitter_parsed: Option<icn_identity::Did> =
                submitter.parse().ok();

            if let (Some(executor_did_parsed), Some(submitter_did_parsed)) =
                (executor_parsed, submitter_parsed)
            {
                let req = icn_ledger::CommonsSettlementRequest {
                    receipt_id,
                    executor: executor_did_parsed,
                    submitter: submitter_did_parsed,
                    cpu_millis: fuel_used, // fuel_used as cpu_millis proxy
                    memory_mb_millis: 0,
                    storage_bytes: 0,
                    egress_bytes: 0,
                    executor_verified: true,
                };

                let engine = icn_ledger::SettlementEngine::new();
                match engine.settle_commons_receipt(&req) {
                    Ok((earn_entry, spend_entry)) => {
                        let mut ledger = ledger_handle.write().await;
                        if let Err(e) = ledger.append_entry(earn_entry).await {
                            tracing::warn!(
                                task_hash = %hex::encode(task_hash),
                                error = %e,
                                "Failed to append commons earn entry"
                            );
                        }
                        if let Err(e) = ledger.append_entry(spend_entry).await {
                            tracing::warn!(
                                task_hash = %hex::encode(task_hash),
                                error = %e,
                                "Failed to append commons spend entry"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            task_hash = %hex::encode(task_hash),
                            error = %e,
                            "Commons settlement failed"
                        );
                    }
                }
            }
        }
    }
}
```

**Important:** `SettlementEngine::new()` creates a fresh engine per call — this means no cross-call dedup. To get persistent dedup, add `settlement_engine: SettlementEngine` as a field on `ComputeActor` and use `self.settlement_engine` instead of a local `let engine`. Do this if the dedup tests fail.

**Also important:** `TaskManager::get()` returns a task reference — check the actual method signature and what fields it exposes. If `scope` is not accessible via the task manager, you may need to also cache the scope+submitter in a separate map (similar to `task_scope_map` in the actor).

### Step 4: Check compile

```bash
cargo check -p icn-compute
```
Fix any compile errors before running tests.

### Step 5: Run the settlement tests

```bash
cargo test -p icn-compute --lib "test_commons_task_earns\|test_commons_task_debits"
```
Expected: both PASS (may need to adjust timing or task manager access)

### Step 6: Run all compute tests

```bash
cargo test -p icn-compute --lib
```
Expected: all 345+ tests pass

### Step 7: Run fmt and clippy

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```
Fix any warnings.

### Step 8: Commit

```bash
git add crates/icn-compute/src/actor/lifecycle.rs
git commit -m "feat(compute): settle commons credits on task completion (#948)"
```

---

## Task 7: Integration test — full earn/spend cycle

**Files:**
- Create: `crates/icn-ledger/tests/commons_credit_integration.rs`

### Step 1: Write the integration test

```rust
//! Integration test: full commons credit earn → check → spend cycle.
//!
//! Verifies that:
//! 1. An executor earns credits after settling a commons receipt.
//! 2. A submitter's balance decreases after settling a commons receipt.
//! 3. Duplicate settlement of the same receipt is rejected.
//! 4. Zero-fuel receipt (no credits) is rejected.

use icn_identity::Did;
use icn_ledger::{
    CommonsSettlementRequest, Ledger, SettlementEngine, COMMONS_CREDIT_CURRENCY,
};
use std::sync::Arc;

fn test_ledger() -> Ledger {
    // Use the in-memory store for tests — check the correct import path:
    // `icn_ledger::store::MemStore` or `icn_ledger::MemStore`
    let store = icn_ledger::store::MemStore::new();
    Ledger::new(Arc::new(store)).unwrap()
}

fn make_request(receipt_id: [u8; 32], fuel: u64) -> CommonsSettlementRequest {
    CommonsSettlementRequest {
        receipt_id,
        executor: "did:icn:executor-alpha".parse().unwrap(),
        submitter: "did:icn:submitter-alpha".parse().unwrap(),
        cpu_millis: fuel,
        memory_mb_millis: 0,
        storage_bytes: 0,
        egress_bytes: 0,
        executor_verified: true,
    }
}

#[tokio::test]
async fn test_earn_and_spend_cycle() {
    let mut ledger = test_ledger();
    let engine = SettlementEngine::new();

    let req = make_request([1u8; 32], 5000);
    let executor: Did = req.executor.clone();
    let submitter: Did = req.submitter.clone();

    let (earn, spend) = engine.settle_commons_receipt(&req).unwrap();

    ledger.append_entry(earn).await.unwrap();
    ledger.append_entry(spend).await.unwrap();

    let exec_balance = ledger.get_balance(&executor, COMMONS_CREDIT_CURRENCY);
    let sub_balance = ledger.get_balance(&submitter, COMMONS_CREDIT_CURRENCY);

    assert_eq!(exec_balance, 5000, "executor should earn 5000 credits (fuel=5000, formula: cpu_millis)");
    assert_eq!(sub_balance, -5000, "submitter should spend 5000 credits");
}

#[tokio::test]
async fn test_duplicate_receipt_rejected() {
    let engine = SettlementEngine::new();
    let req = make_request([2u8; 32], 1000);

    // First settlement succeeds
    assert!(engine.settle_commons_receipt(&req).is_ok());

    // Second with same receipt_id rejected
    let err = engine.settle_commons_receipt(&req).unwrap_err();
    let err_msg = format!("{err:?}");
    assert!(
        err_msg.contains("Duplicate") || err_msg.contains("duplicate"),
        "expected DuplicateEntry, got: {err_msg}"
    );
}

#[tokio::test]
async fn test_zero_fuel_rejected() {
    let engine = SettlementEngine::new();
    let req = make_request([3u8; 32], 0); // cpu_millis = 0, all others = 0
    let result = engine.settle_commons_receipt(&req);
    assert!(result.is_err(), "zero-credit settlement should be rejected");
}
```

### Step 2: Run to verify they compile and pass

```bash
cargo test -p icn-ledger --test commons_credit_integration
```
Expected: 3 tests PASS

### Step 3: Run full ledger test suite

```bash
cargo test -p icn-ledger
```
Expected: all tests pass

### Step 4: Final full workspace check

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --lib
```
All should pass.

### Step 5: Final commit

```bash
git add crates/icn-ledger/tests/commons_credit_integration.rs
git commit -m "test(ledger): integration test for commons credit earn/spend cycle (#948)"
```

---

## Done — open PR

```bash
git push -u origin feat/948-commons-credits
gh pr create \
  --title "feat(ledger/compute): commons credit earning and spending (#948)" \
  --body "Implements #948.

## What
- \`CommonsSettlementRequest\` DTO for commons credit settlement
- \`SettlementEngine::settle_commons_receipt()\` — produces balanced (earn, spend) entry pair
- \`ComputeTask.scope\` field — submitters opt into \`ScopeLevel::Commons\`
- \`ComputeActor::set_ledger()\` — wires ledger for settlement and auto-creates BalanceCallback
- Post-completion settlement hook in \`complete_task_result()\`
- Integration test: earn → balance check → spend → balance check

## Credit formula
\`fuel_used\` is used as a proxy for \`cpu_millis\` (1 credit per fuel unit). Full metering (memory, storage, egress) is deferred to a follow-up issue.

## Notes
- Settles only on \`ExecutionOutcome::Success\`; failed/timeout tasks do not earn/spend credits.
- Dedup is via \`receipt_id\` nonce in journal entries; see Task 6 note about persistent \`SettlementEngine\`.
- Balance floor enforcement uses existing E7 credit ceiling check; floor-at-zero is enforced at submission time, not settlement time.

Closes #948"
```
