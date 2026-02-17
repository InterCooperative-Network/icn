//! Decision Executor — idempotent, persistent wrapper around EffectDispatcher.
//!
//! The `DecisionExecutor` sits between the governance event subscription and the
//! `EffectDispatcher`. It provides:
//!
//! 1. **Idempotency**: If a decision_hash has already been executed (status = Confirmed),
//!    the executor returns early. This prevents double-execution from replayed events.
//!
//! 2. **Execution logging**: Every decision's execution status is persisted to sled.
//!    This survives restarts and enables audit.
//!
//! 3. **Saga-ready structure**: The `ExecutionRecord` status machine
//!    (Pending → Executing → Confirmed/Failed) is the foundation for future
//!    saga patterns (retry, compensation, dead-letter routing).
//!
//! # Architecture
//!
//! ```text
//! [App Layer]
//! GovernanceEvent → translate_payload_to_effects() → Vec<KernelEffect>
//!                                                          |
//!                                                          v
//! [DecisionExecutor]  ←── idempotency check (sled)
//!       |   record Pending
//!       |   record Executing
//!       v
//! [EffectDispatcher]  ←── existing execution path
//!       |
//!       v
//! [DecisionExecutor]  ←── record Confirmed/Failed
//! ```

use std::sync::Arc;

use anyhow::Result;
use tracing::{debug, error, info, warn};

use icn_kernel_api::effects::{EffectResult, KernelEffect};
use icn_kernel_api::execution::{ExecutionRecord, ExecutionStatus, ExecutionStore};

use super::effect_dispatcher::EffectDispatcher;

/// Maximum number of automatic retries before marking permanently failed.
const MAX_RETRIES: u32 = 3;

/// Wraps `EffectDispatcher` with persistent idempotency and execution logging.
pub struct DecisionExecutor {
    dispatcher: Arc<EffectDispatcher>,
    store: Arc<dyn ExecutionStore>,
}

impl DecisionExecutor {
    /// Create a new DecisionExecutor.
    pub fn new(dispatcher: Arc<EffectDispatcher>, store: Arc<dyn ExecutionStore>) -> Self {
        Self { dispatcher, store }
    }

    /// Execute effects for a governance decision, with idempotency.
    ///
    /// # Arguments
    /// * `effects` - Pre-translated kernel effects
    /// * `decision_receipt_id` - Receipt ID for audit linkage
    /// * `decision_hash` - Canonical decision hash (idempotency key)
    /// * `proposal_id` - The originating proposal ID
    ///
    /// # Returns
    /// Effect results, or an empty vec if already executed.
    pub async fn execute(
        &self,
        effects: Vec<KernelEffect>,
        decision_receipt_id: &str,
        decision_hash: &str,
        proposal_id: &str,
    ) -> Result<Vec<EffectResult>> {
        // 1. Idempotency check
        if let Some(existing) = self.store.get(decision_hash)? {
            if existing.is_terminal() {
                debug!(
                    decision_hash = %decision_hash,
                    status = ?existing.status,
                    "Decision already executed (idempotency check), skipping"
                );
                return Ok(vec![]);
            }

            // If it's in Executing state, another execution may be in progress
            // or a prior attempt crashed. For the skeleton, we log and proceed.
            if existing.status == ExecutionStatus::Executing {
                warn!(
                    decision_hash = %decision_hash,
                    "Decision in Executing state (possible crash recovery), re-executing"
                );
            }

            // If it's in Failed state, check retry limit
            if existing.status == ExecutionStatus::Failed && existing.retries >= MAX_RETRIES {
                warn!(
                    decision_hash = %decision_hash,
                    retries = existing.retries,
                    "Decision exceeded max retries, marking permanently failed"
                );
                let mut record = existing;
                record.mark_permanently_failed("Max retries exceeded");
                self.store.put(&record)?;
                return Ok(vec![]);
            }
        }

        // 2. Record Pending
        let mut record =
            ExecutionRecord::new_pending(decision_hash, proposal_id, decision_receipt_id);
        self.store.put(&record)?;

        // 3. Transition to Executing
        record.mark_executing();
        self.store.put(&record)?;

        info!(
            decision_hash = %decision_hash,
            proposal_id = %proposal_id,
            effect_count = effects.len(),
            "Executing governance decision"
        );

        // 4. Delegate to EffectDispatcher
        let results = match self
            .dispatcher
            .execute_effects(effects, decision_receipt_id)
            .await
        {
            Ok(results) => results,
            Err(e) => {
                error!(
                    decision_hash = %decision_hash,
                    error = %e,
                    "Effect dispatcher returned error"
                );
                record.mark_failed(e.to_string());
                self.store.put(&record)?;
                return Err(e);
            }
        };

        // 5. Check results and record outcome
        let all_success = results.iter().all(|r| r.success);
        if all_success {
            let state_change_hashes: Vec<String> = results
                .iter()
                .filter_map(|r| r.state_change_hash.clone())
                .collect();
            // Note: ledger_entry_ids will be populated when treasury executor
            // is wired to return entry IDs in EffectResult.
            record.mark_confirmed(vec![], state_change_hashes);
            self.store.put(&record)?;

            info!(
                decision_hash = %decision_hash,
                result_count = results.len(),
                "Decision execution confirmed"
            );
        } else {
            let failures: Vec<String> = results
                .iter()
                .filter(|r| !r.success)
                .map(|r| r.message.clone())
                .collect();
            let error_msg = failures.join("; ");

            warn!(
                decision_hash = %decision_hash,
                failures = ?failures,
                "Decision execution had failures"
            );

            record.mark_failed(error_msg);
            self.store.put(&record)?;
        }

        Ok(results)
    }

    /// Query execution status for a decision hash.
    pub fn get_status(&self, decision_hash: &str) -> Result<Option<ExecutionRecord>> {
        self.store.get(decision_hash)
    }

    /// List all records with a given status.
    pub fn list_by_status(&self, status: ExecutionStatus) -> Result<Vec<ExecutionRecord>> {
        self.store.list_by_status(status)
    }
}

/// Create an effect executor callback that routes through the DecisionExecutor.
///
/// This replaces `create_effect_executor_callback` from `effect_dispatcher.rs`
/// when the DecisionExecutor is wired. The callback extracts decision_hash
/// from the effects (if present) and delegates to DecisionExecutor::execute.
pub fn create_decision_executor_callback(
    executor: Arc<DecisionExecutor>,
) -> Arc<dyn Fn(Vec<KernelEffect>, String) + Send + Sync> {
    Arc::new(move |effects, decision_receipt_id| {
        if effects.is_empty() {
            debug!(
                receipt_id = %decision_receipt_id,
                "No effects to execute (empty batch)"
            );
            return;
        }

        let executor = executor.clone();
        let effect_count = effects.len();

        // Extract decision_hash from first effect that carries one.
        // Treasury and some other effects embed decision_hash.
        let decision_hash =
            extract_decision_hash(&effects).unwrap_or_else(|| decision_receipt_id.clone());

        // Use receipt_id as proposal_id fallback — the app layer should
        // pass a richer context once the bridge is fully wired.
        let proposal_id = decision_receipt_id.clone();

        tokio::spawn(async move {
            match executor
                .execute(effects, &decision_receipt_id, &decision_hash, &proposal_id)
                .await
            {
                Ok(results) => {
                    let success_count = results.iter().filter(|r| r.success).count();
                    info!(
                        receipt_id = %decision_receipt_id,
                        decision_hash = %decision_hash,
                        total = results.len(),
                        success = success_count,
                        "Decision execution complete"
                    );
                }
                Err(e) => {
                    error!(
                        receipt_id = %decision_receipt_id,
                        decision_hash = %decision_hash,
                        effect_count = effect_count,
                        error = %e,
                        "Decision execution failed"
                    );
                }
            }
        });
    })
}

/// Extract `decision_hash` from the first effect that carries one.
fn extract_decision_hash(effects: &[KernelEffect]) -> Option<String> {
    use icn_kernel_api::effects::TreasuryEffect;
    for effect in effects {
        if let KernelEffect::Treasury(
            TreasuryEffect::Spend { decision_hash, .. }
            | TreasuryEffect::CreateBudget { decision_hash, .. }
            | TreasuryEffect::ReleaseEscrow { decision_hash, .. },
        ) = effect
        {
            if !decision_hash.is_empty() {
                return Some(decision_hash.clone());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_kernel_api::effects::TreasuryEffect;
    use icn_kernel_api::execution::ExecutionRecord;
    use std::collections::HashMap;
    use std::sync::RwLock;

    /// In-memory execution store for testing.
    struct MemoryExecutionStore {
        records: RwLock<HashMap<String, ExecutionRecord>>,
    }

    impl MemoryExecutionStore {
        fn new() -> Self {
            Self {
                records: RwLock::new(HashMap::new()),
            }
        }
    }

    impl ExecutionStore for MemoryExecutionStore {
        fn get(&self, decision_hash: &str) -> Result<Option<ExecutionRecord>> {
            Ok(self
                .records
                .read()
                .map_err(|e| anyhow::anyhow!("lock: {e}"))?
                .get(decision_hash)
                .cloned())
        }

        fn put(&self, record: &ExecutionRecord) -> Result<()> {
            self.records
                .write()
                .map_err(|e| anyhow::anyhow!("lock: {e}"))?
                .insert(record.decision_hash.clone(), record.clone());
            Ok(())
        }

        fn list_by_status(&self, status: ExecutionStatus) -> Result<Vec<ExecutionRecord>> {
            Ok(self
                .records
                .read()
                .map_err(|e| anyhow::anyhow!("lock: {e}"))?
                .values()
                .filter(|r| r.status == status)
                .cloned()
                .collect())
        }

        fn count_by_status(&self) -> Result<HashMap<ExecutionStatus, usize>> {
            let mut counts = HashMap::new();
            for record in self
                .records
                .read()
                .map_err(|e| anyhow::anyhow!("lock: {e}"))?
                .values()
            {
                *counts.entry(record.status).or_insert(0) += 1;
            }
            Ok(counts)
        }
    }

    /// Helper: create a DecisionExecutor with in-memory stores and no ledger.
    fn make_test_executor() -> (Arc<DecisionExecutor>, Arc<MemoryExecutionStore>) {
        use icn_kernel_api::protocol_params::*;

        // Minimal param store stub
        struct StubParamStore;
        impl ProtocolParameterStore for StubParamStore {
            fn get(&self, _: &str) -> Result<Option<ProtocolParameter>> {
                Ok(None)
            }
            fn get_effective(
                &self,
                _: &str,
                _: Option<&str>,
                _: Option<&str>,
            ) -> Result<Option<ProtocolParameter>> {
                Ok(None)
            }
            fn set(
                &self,
                _: ProtocolParameter,
                _: Option<String>,
                _: Option<String>,
            ) -> Result<()> {
                Ok(())
            }
            fn list(&self) -> Result<Vec<ProtocolParameter>> {
                Ok(vec![])
            }
            fn list_by_category(&self, _: &str) -> Result<Vec<ProtocolParameter>> {
                Ok(vec![])
            }
            fn get_history(&self, _: &str) -> Result<Vec<ParameterChange>> {
                Ok(vec![])
            }
            fn get_history_paginated(
                &self,
                _: &str,
                _: usize,
                _: usize,
            ) -> Result<(Vec<ParameterChange>, usize)> {
                Ok((vec![], 0))
            }
            fn prune_history(&self, _: &str, _: usize) -> Result<usize> {
                Ok(0)
            }
            fn delete(&self, _: &str) -> Result<()> {
                Ok(())
            }
            fn exists(&self, _: &str) -> Result<bool> {
                Ok(false)
            }
            fn count(&self) -> Result<usize> {
                Ok(0)
            }
            fn total_history_count(&self) -> Result<usize> {
                Ok(0)
            }
            fn validate(
                &self,
                _: &str,
                _: &ParameterValue,
            ) -> std::result::Result<(), ParameterValidationError> {
                Ok(())
            }
            fn list_scoped_parameters(&self) -> Result<Vec<ProtocolParameter>> {
                Ok(vec![])
            }
            fn delete_scoped_parameter(&self, _: &str, _: &ParameterScope) -> Result<bool> {
                Ok(false)
            }
            fn add_pending_change(&self, _: PendingParameterChange) -> Result<()> {
                Ok(())
            }
            fn get_pending_change(
                &self,
                _: &PendingChangeId,
            ) -> Result<Option<PendingParameterChange>> {
                Ok(None)
            }
            fn list_pending_changes(&self) -> Result<Vec<PendingParameterChange>> {
                Ok(vec![])
            }
            fn list_pending_changes_for_parameter(
                &self,
                _: &str,
            ) -> Result<Vec<PendingParameterChange>> {
                Ok(vec![])
            }
            fn get_changes_due_before(&self, _: u64) -> Result<Vec<PendingParameterChange>> {
                Ok(vec![])
            }
            fn update_pending_change(&self, _: PendingParameterChange) -> Result<()> {
                Ok(())
            }
            fn cancel_pending_change(&self, _: &PendingChangeId, _: &str) -> Result<()> {
                Ok(())
            }
            fn count_pending_changes(&self) -> Result<usize> {
                Ok(0)
            }
        }

        let param_store = Arc::new(StubParamStore);
        let kernel_executor =
            Arc::new(super::super::governance_executor::KernelGovernanceExecutor::new(param_store));
        let dispatcher = Arc::new(EffectDispatcher::new(kernel_executor));
        let exec_store = Arc::new(MemoryExecutionStore::new());

        let decision_executor = Arc::new(DecisionExecutor::new(dispatcher, exec_store.clone()));

        (decision_executor, exec_store)
    }

    #[tokio::test]
    async fn test_execute_records_status() {
        let (executor, store) = make_test_executor();

        let effects = vec![KernelEffect::NoOp {
            reason: "test".to_string(),
        }];

        let results = executor
            .execute(effects, "receipt-1", "hash-1", "proposal-1")
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].success);

        // Verify record was persisted with Confirmed status
        let record = store.get("hash-1").unwrap().unwrap();
        assert_eq!(record.status, ExecutionStatus::Confirmed);
        assert_eq!(record.proposal_id, "proposal-1");
        assert!(record.finished_at.is_some());
    }

    #[tokio::test]
    async fn test_idempotency_skips_confirmed() {
        let (executor, store) = make_test_executor();

        // First execution
        let effects = vec![KernelEffect::NoOp {
            reason: "first".to_string(),
        }];
        let results1 = executor
            .execute(effects, "receipt-1", "hash-1", "proposal-1")
            .await
            .unwrap();
        assert_eq!(results1.len(), 1);

        // Second execution with same decision_hash — should be skipped
        let effects2 = vec![KernelEffect::NoOp {
            reason: "duplicate".to_string(),
        }];
        let results2 = executor
            .execute(effects2, "receipt-1", "hash-1", "proposal-1")
            .await
            .unwrap();
        assert!(
            results2.is_empty(),
            "Duplicate execution should return empty"
        );

        // Status should still be Confirmed
        let record = store.get("hash-1").unwrap().unwrap();
        assert_eq!(record.status, ExecutionStatus::Confirmed);
    }

    #[tokio::test]
    async fn test_idempotency_skips_permanently_failed() {
        let (executor, store) = make_test_executor();

        // Manually insert a permanently failed record
        let mut record = ExecutionRecord::new_pending("hash-pf", "p-1", "r-1");
        record.mark_permanently_failed("Non-recoverable");
        store.put(&record).unwrap();

        // Attempt execution — should be skipped
        let effects = vec![KernelEffect::NoOp {
            reason: "retry".to_string(),
        }];
        let results = executor
            .execute(effects, "r-1", "hash-pf", "p-1")
            .await
            .unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_treasury_effect_with_decision_hash() {
        let (executor, store) = make_test_executor();

        let effects = vec![KernelEffect::Treasury(TreasuryEffect::Spend {
            treasury_did: "did:icn:treasury".to_string(),
            recipient_did: "did:icn:alice".to_string(),
            amount: 500,
            currency: "HOURS".to_string(),
            memo: "Test".to_string(),
            budget_id: None,
            decision_receipt_id: "receipt-t1".to_string(),
            decision_hash: "sha256:treasury-hash-1".to_string(),
        })];

        let results = executor
            .execute(
                effects,
                "receipt-t1",
                "sha256:treasury-hash-1",
                "proposal-t1",
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].success);

        let record = store.get("sha256:treasury-hash-1").unwrap().unwrap();
        assert_eq!(record.status, ExecutionStatus::Confirmed);
    }

    #[tokio::test]
    async fn test_extract_decision_hash_from_effects() {
        let effects = vec![
            KernelEffect::NoOp {
                reason: "first".to_string(),
            },
            KernelEffect::Treasury(TreasuryEffect::Spend {
                treasury_did: "t".to_string(),
                recipient_did: "r".to_string(),
                amount: 100,
                currency: "ICN".to_string(),
                memo: "m".to_string(),
                budget_id: None,
                decision_receipt_id: "r1".to_string(),
                decision_hash: "extracted-hash".to_string(),
            }),
        ];

        assert_eq!(
            extract_decision_hash(&effects),
            Some("extracted-hash".to_string())
        );

        // No treasury effects → None
        let no_hash = vec![KernelEffect::NoOp {
            reason: "only".to_string(),
        }];
        assert_eq!(extract_decision_hash(&no_hash), None);
    }

    #[tokio::test]
    async fn test_different_decisions_execute_independently() {
        let (executor, store) = make_test_executor();

        // Decision A
        executor
            .execute(
                vec![KernelEffect::NoOp { reason: "a".into() }],
                "r-a",
                "hash-a",
                "p-a",
            )
            .await
            .unwrap();

        // Decision B
        executor
            .execute(
                vec![KernelEffect::NoOp { reason: "b".into() }],
                "r-b",
                "hash-b",
                "p-b",
            )
            .await
            .unwrap();

        // Both should be independently confirmed
        let a = store.get("hash-a").unwrap().unwrap();
        let b = store.get("hash-b").unwrap().unwrap();
        assert_eq!(a.status, ExecutionStatus::Confirmed);
        assert_eq!(b.status, ExecutionStatus::Confirmed);
        assert_ne!(a.proposal_id, b.proposal_id);
    }
}
