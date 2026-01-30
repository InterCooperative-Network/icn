//! Settlement engine for converting verified execution receipts into journal entries.
//!
//! This module provides the `SettlementEngine` which accepts `SettlementRequest` DTOs
//! (converted from `ExecutionReceipt` by the caller) and creates balanced double-entry
//! journal entries in the ledger.
//!
//! # Scope routing
//!
//! - **Local / Cell / Org**: Settle directly via `JournalEntryBuilder`.
//! - **Federation / Commons**: Rejected — caller must route to `ReceiptClearingManager`
//!   in `icn-federation`.
//!
//! # Deduplication
//!
//! Each receipt is deduplicated by `sha256(task_hash || executor_did_bytes)`. The engine
//! tracks settled receipts in an in-memory `HashSet` (sled persistence is future work).
//!
//! # Design
//!
//! `SettlementRequest` is a DTO — it does NOT depend on `icn-compute`. The caller is
//! responsible for verifying receipt signatures before constructing the request and
//! setting `executor_verified = true`.

use crate::entry::JournalEntryBuilder;
use crate::error::LedgerError;
use crate::types::JournalEntry;
use icn_identity::Did;
use icn_kernel_api::scope::ScopeLevel;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::RwLock;

/// A settlement request DTO.
///
/// Constructed by the caller from a verified `ExecutionReceipt`. The caller must:
/// 1. Verify all receipt signatures (executor, submitter ack, optionally attester).
/// 2. Set `executor_verified = true` after verification.
/// 3. Parse DID strings into `Did` values.
///
/// The engine will reject requests where `executor_verified` is false.
#[derive(Debug, Clone)]
pub struct SettlementRequest {
    /// Hash of the compute task (from ExecutionReceipt.task_hash)
    pub task_hash: [u8; 32],

    /// DID of the executor who performed the work
    pub executor: Did,

    /// DID of the submitter who requested the work
    pub submitter: Did,

    /// Scope level of the original task
    pub scope: ScopeLevel,

    /// Cost in the settlement currency (from ExecutionReceipt.cost)
    pub cost: u64,

    /// Currency for settlement (e.g., "credits")
    pub currency: String,

    /// Whether the caller has verified the executor's signature.
    /// The engine rejects requests where this is false.
    pub executor_verified: bool,
}

/// Settlement engine for converting verified receipts into journal entries.
///
/// Handles deduplication and scope validation, then delegates to
/// `JournalEntryBuilder` for balanced double-entry creation.
pub struct SettlementEngine {
    /// In-memory dedup set of settled receipt keys.
    /// Each key is `sha256(task_hash || executor_did_bytes)`.
    settled: RwLock<HashSet<[u8; 32]>>,
}

impl SettlementEngine {
    /// Create a new settlement engine with an empty dedup set.
    pub fn new() -> Self {
        SettlementEngine {
            settled: RwLock::new(HashSet::new()),
        }
    }

    /// Settle a verified receipt, creating a balanced journal entry.
    ///
    /// # Errors
    ///
    /// - `LedgerError::InvalidEntry` if `executor_verified` is false
    /// - `LedgerError::InvalidEntry` if scope is Federation or Commons
    /// - `LedgerError::DuplicateEntry` if this receipt has already been settled
    /// - `LedgerError::InvalidEntry` if cost is zero
    /// - Propagates `JournalEntryBuilder::build()` errors
    pub fn settle_receipt(&self, request: &SettlementRequest) -> Result<JournalEntry, LedgerError> {
        // 1. Reject unverified receipts
        if !request.executor_verified {
            return Err(LedgerError::InvalidEntry(
                "receipt executor signature not verified".to_string(),
            ));
        }

        // 2. Reject federation/commons scope — must use clearing
        match request.scope {
            ScopeLevel::Federation => {
                return Err(LedgerError::InvalidEntry(
                    "federation-scope receipts must use cross-cooperative clearing".to_string(),
                ));
            }
            ScopeLevel::Commons => {
                return Err(LedgerError::InvalidEntry(
                    "commons-scope receipts must use cross-cooperative clearing".to_string(),
                ));
            }
            ScopeLevel::Local | ScopeLevel::Cell | ScopeLevel::Org => {}
        }

        // 3. Reject zero-cost receipts
        if request.cost == 0 {
            return Err(LedgerError::InvalidEntry(
                "receipt cost must be non-zero".to_string(),
            ));
        }

        // 4. Check deduplication
        let dedup_key = Self::dedup_key(&request.task_hash, &request.executor);
        {
            let settled = self
                .settled
                .read()
                .map_err(|_| LedgerError::Internal("settlement lock poisoned".to_string()))?;
            if settled.contains(&dedup_key) {
                return Err(LedgerError::DuplicateEntry(format!(
                    "receipt already settled: task={} executor={}",
                    hex::encode(request.task_hash),
                    request.executor
                )));
            }
        }

        // 5. Build balanced journal entry: debit submitter, credit executor
        let cost_i64 = i64::try_from(request.cost).map_err(|_| {
            LedgerError::ArithmeticOverflow(format!(
                "receipt cost {} exceeds i64::MAX",
                request.cost
            ))
        })?;

        let entry = JournalEntryBuilder::new(request.submitter.clone())
            .debit(
                request.submitter.clone(),
                request.currency.clone(),
                cost_i64,
            )
            .credit(request.executor.clone(), request.currency.clone(), cost_i64)
            .build()
            .map_err(|e| LedgerError::InvalidEntry(e.to_string()))?;

        // 6. Record dedup key
        {
            let mut settled = self
                .settled
                .write()
                .map_err(|_| LedgerError::Internal("settlement lock poisoned".to_string()))?;
            settled.insert(dedup_key);
        }

        Ok(entry)
    }

    /// Check whether a receipt has already been settled.
    pub fn is_settled(&self, task_hash: &[u8; 32], executor: &Did) -> bool {
        let key = Self::dedup_key(task_hash, executor);
        self.settled
            .read()
            .map(|s| s.contains(&key))
            .unwrap_or(false)
    }

    /// Number of settled receipts tracked.
    pub fn settled_count(&self) -> usize {
        self.settled.read().map(|s| s.len()).unwrap_or(0)
    }

    /// Compute deduplication key: `sha256(task_hash || executor_did_string_bytes)`.
    fn dedup_key(task_hash: &[u8; 32], executor: &Did) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(task_hash);
        hasher.update(executor.to_string().as_bytes());
        let result = hasher.finalize();
        let mut key = [0u8; 32];
        key.copy_from_slice(&result);
        key
    }
}

impl Default for SettlementEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn make_request(scope: ScopeLevel, cost: u64, verified: bool) -> SettlementRequest {
        let executor_kp = KeyPair::generate().unwrap();
        let submitter_kp = KeyPair::generate().unwrap();
        SettlementRequest {
            task_hash: [0xAA; 32],
            executor: executor_kp.did().clone(),
            submitter: submitter_kp.did().clone(),
            scope,
            cost,
            currency: "credits".to_string(),
            executor_verified: verified,
        }
    }

    fn make_request_with_keys(
        executor: &KeyPair,
        submitter: &KeyPair,
        task_hash: [u8; 32],
    ) -> SettlementRequest {
        SettlementRequest {
            task_hash,
            executor: executor.did().clone(),
            submitter: submitter.did().clone(),
            scope: ScopeLevel::Local,
            cost: 100,
            currency: "credits".to_string(),
            executor_verified: true,
        }
    }

    #[test]
    fn test_settle_creates_balanced_entry() {
        let engine = SettlementEngine::new();
        let executor_kp = KeyPair::generate().unwrap();
        let submitter_kp = KeyPair::generate().unwrap();

        let request = SettlementRequest {
            task_hash: [0xBB; 32],
            executor: executor_kp.did().clone(),
            submitter: submitter_kp.did().clone(),
            scope: ScopeLevel::Local,
            cost: 500,
            currency: "credits".to_string(),
            executor_verified: true,
        };

        let entry = engine.settle_receipt(&request).unwrap();

        // Should have exactly 2 account deltas: one debit, one credit
        assert_eq!(entry.accounts.len(), 2);

        let debit = &entry.accounts[0];
        assert_eq!(debit.account_id, *submitter_kp.did());
        assert_eq!(debit.debit, Some(500));
        assert_eq!(debit.credit, None);
        assert_eq!(debit.currency, "credits");

        let credit = &entry.accounts[1];
        assert_eq!(credit.account_id, *executor_kp.did());
        assert_eq!(credit.credit, Some(500));
        assert_eq!(credit.debit, None);
        assert_eq!(credit.currency, "credits");

        // Author should be the submitter
        assert_eq!(entry.author, *submitter_kp.did());
    }

    #[test]
    fn test_settle_deduplication() {
        let engine = SettlementEngine::new();
        let executor_kp = KeyPair::generate().unwrap();
        let submitter_kp = KeyPair::generate().unwrap();

        let request = make_request_with_keys(&executor_kp, &submitter_kp, [0xCC; 32]);

        // First settlement succeeds
        let result = engine.settle_receipt(&request);
        assert!(result.is_ok());

        // Second attempt is rejected as duplicate
        let result = engine.settle_receipt(&request);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("already settled"), "got: {err}");
    }

    #[test]
    fn test_settle_federation_scope_rejected() {
        let engine = SettlementEngine::new();
        let request = make_request(ScopeLevel::Federation, 100, true);

        let result = engine.settle_receipt(&request);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("cross-cooperative clearing"), "got: {err}");
    }

    #[test]
    fn test_settle_commons_scope_rejected() {
        let engine = SettlementEngine::new();
        let request = make_request(ScopeLevel::Commons, 100, true);

        let result = engine.settle_receipt(&request);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("cross-cooperative clearing"), "got: {err}");
    }

    #[test]
    fn test_settle_local_scope_accepted() {
        let engine = SettlementEngine::new();
        let request = make_request(ScopeLevel::Local, 100, true);
        assert!(engine.settle_receipt(&request).is_ok());
    }

    #[test]
    fn test_settle_cell_scope_accepted() {
        let engine = SettlementEngine::new();
        let request = make_request(ScopeLevel::Cell, 100, true);
        assert!(engine.settle_receipt(&request).is_ok());
    }

    #[test]
    fn test_settle_org_scope_accepted() {
        let engine = SettlementEngine::new();
        let request = make_request(ScopeLevel::Org, 100, true);
        assert!(engine.settle_receipt(&request).is_ok());
    }

    #[test]
    fn test_settle_unverified_rejected() {
        let engine = SettlementEngine::new();
        let request = make_request(ScopeLevel::Local, 100, false);

        let result = engine.settle_receipt(&request);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not verified"), "got: {err}");
    }

    #[test]
    fn test_settle_zero_cost_rejected() {
        let engine = SettlementEngine::new();
        let request = make_request(ScopeLevel::Local, 0, true);

        let result = engine.settle_receipt(&request);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("non-zero"), "got: {err}");
    }

    #[test]
    fn test_is_settled() {
        let engine = SettlementEngine::new();
        let executor_kp = KeyPair::generate().unwrap();
        let submitter_kp = KeyPair::generate().unwrap();
        let task_hash = [0xDD; 32];

        // Not settled yet
        assert!(!engine.is_settled(&task_hash, executor_kp.did()));

        // Settle it
        let request = make_request_with_keys(&executor_kp, &submitter_kp, task_hash);
        engine.settle_receipt(&request).unwrap();

        // Now it is settled
        assert!(engine.is_settled(&task_hash, executor_kp.did()));
    }

    #[test]
    fn test_different_executor_same_task_not_duplicate() {
        let engine = SettlementEngine::new();
        let executor_a = KeyPair::generate().unwrap();
        let executor_b = KeyPair::generate().unwrap();
        let submitter = KeyPair::generate().unwrap();
        let task_hash = [0xEE; 32];

        let request_a = make_request_with_keys(&executor_a, &submitter, task_hash);
        let request_b = make_request_with_keys(&executor_b, &submitter, task_hash);

        // Both should succeed — different executors
        assert!(engine.settle_receipt(&request_a).is_ok());
        assert!(engine.settle_receipt(&request_b).is_ok());
        assert_eq!(engine.settled_count(), 2);
    }

    #[test]
    fn test_same_executor_different_task_not_duplicate() {
        let engine = SettlementEngine::new();
        let executor = KeyPair::generate().unwrap();
        let submitter = KeyPair::generate().unwrap();

        let request_a = make_request_with_keys(&executor, &submitter, [0x11; 32]);
        let request_b = make_request_with_keys(&executor, &submitter, [0x22; 32]);

        // Both should succeed — different tasks
        assert!(engine.settle_receipt(&request_a).is_ok());
        assert!(engine.settle_receipt(&request_b).is_ok());
        assert_eq!(engine.settled_count(), 2);
    }

    #[test]
    fn test_settled_count() {
        let engine = SettlementEngine::new();
        assert_eq!(engine.settled_count(), 0);

        let request = make_request(ScopeLevel::Local, 50, true);
        engine.settle_receipt(&request).unwrap();
        assert_eq!(engine.settled_count(), 1);
    }

    #[test]
    fn test_dedup_key_deterministic() {
        let kp = KeyPair::generate().unwrap();
        let task_hash = [0xFF; 32];

        let key1 = SettlementEngine::dedup_key(&task_hash, kp.did());
        let key2 = SettlementEngine::dedup_key(&task_hash, kp.did());
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_dedup_key_changes_on_different_executor() {
        let kp_a = KeyPair::generate().unwrap();
        let kp_b = KeyPair::generate().unwrap();
        let task_hash = [0xFF; 32];

        let key_a = SettlementEngine::dedup_key(&task_hash, kp_a.did());
        let key_b = SettlementEngine::dedup_key(&task_hash, kp_b.did());
        assert_ne!(key_a, key_b);
    }

    #[test]
    fn test_dedup_key_changes_on_different_task() {
        let kp = KeyPair::generate().unwrap();

        let key_a = SettlementEngine::dedup_key(&[0x11; 32], kp.did());
        let key_b = SettlementEngine::dedup_key(&[0x22; 32], kp.did());
        assert_ne!(key_a, key_b);
    }
}
