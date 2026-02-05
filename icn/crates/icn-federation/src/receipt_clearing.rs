//! Receipt Clearing for Cross-Scope Settlement (Epic 4, Issue #941)
//!
//! Handles `ExecutionReceipt`s that cross cooperative boundaries (Federation/Commons scope).
//! Local/Cell/Org scoped receipts are settled directly by `icn-ledger::SettlementEngine`.
//! Federation and Commons scoped receipts are batched here and flushed to the existing
//! `ClearingManager` infrastructure as `CrossCoopTransfer`s.
//!
//! # Data Flow
//!
//! ```text
//! ExecutionReceipt (Federation/Commons scope)
//!       │
//!       ▼
//! ReceiptClearingManager
//!       │
//!       ├─ [Federation] → ClearingManager → CrossCoopTransfer
//!       └─ [Commons]    → stub (TODO: Epic 6 commons credit pool)
//! ```

use std::collections::VecDeque;
use std::sync::RwLock;

use icn_kernel_api::ScopeLevel;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::clearing::CrossCoopTransfer;
use crate::clearing_manager::ClearingManager;
use crate::error::{FederationError, Result};
use crate::types::current_timestamp;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A receipt queued for cross-cooperative clearing.
///
/// This is a DTO — it carries the fields needed for clearing without
/// depending on `icn-compute::ExecutionReceipt`. The caller (gateway/service)
/// converts `ExecutionReceipt` → `ClearingReceipt` at the boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClearingReceipt {
    /// Blake3 hash of the original receipt's signing payload
    pub receipt_hash: [u8; 32],

    /// DID of the executor (credit recipient)
    pub executor_did: String,

    /// Cooperative ID of the executor
    pub executor_coop: String,

    /// DID of the submitter (debit source)
    pub submitter_did: String,

    /// Cooperative ID of the submitter
    pub submitter_coop: String,

    /// Amount to settle (in the agreed currency)
    pub cost: u64,

    /// Currency for the transfer
    pub currency: String,

    /// Scope level — must be Federation or Commons
    pub scope: ScopeLevel,

    /// When the original receipt was created
    pub created_at: u64,

    /// Whether this receipt has been disputed
    pub disputed: bool,

    /// Reason for dispute, if any
    pub dispute_reason: Option<String>,
}

/// Configuration for batch clearing behavior.
#[derive(Debug, Clone)]
pub struct BatchClearingConfig {
    /// Maximum number of receipts in a batch before auto-flush
    pub max_batch_size: usize,

    /// Maximum age (seconds) of the oldest receipt before auto-flush
    pub max_batch_age_secs: u64,
}

impl Default for BatchClearingConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 100,
            max_batch_age_secs: 3600, // 1 hour
        }
    }
}

/// Manages batching and flushing of cross-scope execution receipts.
///
/// Federation-scoped receipts are converted to `CrossCoopTransfer`s and
/// proposed through the existing `ClearingManager`. Commons-scoped receipts
/// are logged and stubbed pending Epic 6 (#925).
///
/// # Flush lifecycle
///
/// This manager does **not** auto-flush. Callers must periodically:
/// 1. Check [`should_flush()`](Self::should_flush) (batch size or age exceeded)
/// 2. Call [`flush_to_clearing()`](Self::flush_to_clearing) to drain pending receipts
///
/// A background task in the actor layer (future work) should poll on a timer.
pub struct ReceiptClearingManager {
    /// Pending receipts awaiting flush
    pending: RwLock<VecDeque<ClearingReceipt>>,

    /// Timestamp when the current batch started (first receipt arrival)
    batch_started_at: RwLock<Option<u64>>,

    /// Batch configuration
    config: BatchClearingConfig,
}

impl ReceiptClearingManager {
    /// Create a new manager with the given batch configuration.
    pub fn new(config: BatchClearingConfig) -> Self {
        Self {
            pending: RwLock::new(VecDeque::new()),
            batch_started_at: RwLock::new(None),
            config,
        }
    }

    /// Submit a receipt for cross-scope clearing.
    ///
    /// Only Federation and Commons scoped receipts are accepted.
    /// Returns an error if the scope is Local, Cell, or Org (those go to
    /// `icn-ledger::SettlementEngine` instead).
    pub fn submit_receipt(&self, receipt: ClearingReceipt) -> Result<()> {
        match receipt.scope {
            ScopeLevel::Federation | ScopeLevel::Commons => {}
            other => {
                return Err(FederationError::InvalidTransfer(format!(
                    "receipt scope {other:?} must be settled locally, not via clearing"
                )));
            }
        }

        if receipt.executor_coop == receipt.submitter_coop {
            return Err(FederationError::InvalidTransfer(
                "cross-scope clearing requires different cooperatives; \
                 same-coop receipts should use local settlement"
                    .into(),
            ));
        }

        if receipt.cost == 0 {
            return Err(FederationError::InvalidTransfer(
                "receipt cost must be > 0".into(),
            ));
        }

        let mut pending = self.pending.write().unwrap_or_else(|poisoned| {
            warn!("Pending receipts lock poisoned, recovering");
            poisoned.into_inner()
        });

        // Enforce batch size cap to prevent unbounded memory growth
        if pending.len() >= self.config.max_batch_size {
            return Err(FederationError::InvalidState(format!(
                "clearing batch full ({} receipts); flush before submitting more",
                self.config.max_batch_size
            )));
        }

        // Reject duplicate receipt hashes
        if pending
            .iter()
            .any(|r| r.receipt_hash == receipt.receipt_hash)
        {
            return Err(FederationError::InvalidTransfer(format!(
                "duplicate receipt hash {}",
                hex::encode(receipt.receipt_hash)
            )));
        }

        // Start batch timer on first receipt
        let mut batch_start = self.batch_started_at.write().unwrap_or_else(|poisoned| {
            warn!("Batch timer lock poisoned, recovering");
            poisoned.into_inner()
        });
        if batch_start.is_none() {
            *batch_start = Some(current_timestamp());
        }
        drop(batch_start);

        debug!(
            receipt_hash = hex::encode(receipt.receipt_hash),
            scope = ?receipt.scope,
            "queued receipt for clearing"
        );

        pending.push_back(receipt);
        Ok(())
    }

    /// Dispute a pending receipt. Either the submitter or executor may dispute.
    ///
    /// - **Submitter**: disputes if they believe the work wasn't performed correctly
    /// - **Executor**: disputes if they believe the receipt is fraudulent or incorrect
    ///
    /// Disputed receipts are skipped during flush.
    pub fn dispute_receipt(
        &self,
        receipt_hash: &[u8; 32],
        disputer: &str,
        reason: String,
    ) -> Result<()> {
        let mut pending = self.pending.write().unwrap_or_else(|poisoned| {
            warn!("Pending receipts lock poisoned, recovering");
            poisoned.into_inner()
        });

        let receipt = pending
            .iter_mut()
            .find(|r| r.receipt_hash == *receipt_hash)
            .ok_or_else(|| FederationError::TransferNotFound(hex::encode(receipt_hash)))?;

        // Either party to the receipt can dispute
        if receipt.submitter_did != disputer && receipt.executor_did != disputer {
            return Err(FederationError::Unauthorized(format!(
                "only submitter {} or executor {} can dispute, not {}",
                receipt.submitter_did, receipt.executor_did, disputer
            )));
        }

        if receipt.disputed {
            return Err(FederationError::InvalidState(
                "receipt already disputed".into(),
            ));
        }

        receipt.disputed = true;
        receipt.dispute_reason = Some(reason.clone());

        info!(
            receipt_hash = hex::encode(receipt_hash),
            disputer, reason, "receipt disputed"
        );

        Ok(())
    }

    /// Check whether the batch should be flushed.
    ///
    /// Returns `true` if:
    /// - The pending count reaches `max_batch_size`, OR
    /// - The oldest receipt has been pending longer than `max_batch_age_secs`
    pub fn should_flush(&self) -> bool {
        let pending = self.pending.read().unwrap_or_else(|poisoned| {
            warn!("Pending receipts lock poisoned, recovering");
            poisoned.into_inner()
        });

        if pending.len() >= self.config.max_batch_size {
            return true;
        }

        let batch_start = self.batch_started_at.read().unwrap_or_else(|poisoned| {
            warn!("Batch timer lock poisoned, recovering");
            poisoned.into_inner()
        });

        if let Some(started) = *batch_start {
            let age = current_timestamp().saturating_sub(started);
            if age >= self.config.max_batch_age_secs && !pending.is_empty() {
                return true;
            }
        }

        false
    }

    /// Flush pending receipts to the clearing infrastructure.
    ///
    /// - **Federation** receipts → `ClearingManager::propose_transfer()`
    /// - **Commons** receipts → logged + skipped (TODO: Epic 6 #925)
    /// - **Disputed** receipts → skipped
    ///
    /// Returns the number of receipts successfully flushed.
    pub fn flush_to_clearing(&self, clearing_manager: &ClearingManager) -> Result<FlushReport> {
        let mut pending = self.pending.write().unwrap_or_else(|poisoned| {
            warn!("Pending receipts lock poisoned, recovering");
            poisoned.into_inner()
        });

        let total = pending.len();
        let mut federation_flushed: usize = 0;
        let mut commons_deferred: usize = 0;
        let mut disputed_skipped: usize = 0;
        let mut errors: Vec<String> = Vec::new();

        let receipts: Vec<ClearingReceipt> = pending.drain(..).collect();
        drop(pending);

        // Reset batch timer
        let mut batch_start = self.batch_started_at.write().unwrap_or_else(|poisoned| {
            warn!("Batch timer lock poisoned, recovering");
            poisoned.into_inner()
        });
        *batch_start = None;
        drop(batch_start);

        for receipt in receipts {
            if receipt.disputed {
                disputed_skipped += 1;
                debug!(
                    receipt_hash = hex::encode(receipt.receipt_hash),
                    "skipping disputed receipt"
                );
                continue;
            }

            match receipt.scope {
                ScopeLevel::Federation => {
                    let receipt_hex = hex::encode(receipt.receipt_hash);
                    let result: Result<()> = (|| {
                        let amount = i64::try_from(receipt.cost).map_err(|_| {
                            FederationError::InvalidTransfer(format!(
                                "receipt {receipt_hex} cost {} overflows i64",
                                receipt.cost
                            ))
                        })?;

                        let transfer = CrossCoopTransfer::new(
                            receipt_hex.clone(),
                            receipt.submitter_coop.clone(),
                            receipt.submitter_did.parse().map_err(|e: anyhow::Error| {
                                FederationError::InvalidDidFormat(e.to_string())
                            })?,
                            receipt.executor_coop.clone(),
                            receipt.executor_did.parse().map_err(|e: anyhow::Error| {
                                FederationError::InvalidDidFormat(e.to_string())
                            })?,
                            amount,
                            receipt.currency.clone(),
                            amount,
                            receipt.currency.clone(),
                        );

                        clearing_manager.propose_transfer(transfer)?;
                        Ok(())
                    })();

                    match result {
                        Ok(()) => {
                            federation_flushed += 1;
                            debug!(
                                receipt_hash = receipt_hex,
                                "flushed federation receipt to clearing"
                            );
                        }
                        Err(e) => {
                            errors.push(format!("receipt {receipt_hex}: {e}"));
                            warn!(
                                receipt_hash = receipt_hex,
                                error = %e,
                                "failed to flush federation receipt"
                            );
                        }
                    }
                }
                ScopeLevel::Commons => {
                    // TODO(Epic 6, #925): Route to commons credit pool once implemented.
                    // For now, log and count as deferred.
                    commons_deferred += 1;
                    info!(
                        receipt_hash = hex::encode(receipt.receipt_hash),
                        cost = receipt.cost,
                        currency = receipt.currency,
                        "commons receipt deferred — commons credit pool not yet implemented (Epic 6 #925)"
                    );
                }
                _ => {
                    // Should not reach here due to submit_receipt validation,
                    // but handle defensively.
                    errors.push(format!(
                        "receipt {}: unexpected scope {:?}",
                        hex::encode(receipt.receipt_hash),
                        receipt.scope
                    ));
                }
            }
        }

        let report = FlushReport {
            total,
            federation_flushed,
            commons_deferred,
            disputed_skipped,
            errors,
        };

        info!(
            total = report.total,
            flushed = report.federation_flushed,
            deferred = report.commons_deferred,
            disputed = report.disputed_skipped,
            errors = report.errors.len(),
            "batch flush complete"
        );

        Ok(report)
    }

    /// Number of receipts currently pending.
    pub fn pending_count(&self) -> usize {
        self.pending
            .read()
            .unwrap_or_else(|poisoned| {
                warn!("Pending receipts lock poisoned, recovering");
                poisoned.into_inner()
            })
            .len()
    }

    /// Number of disputed receipts currently pending.
    pub fn disputed_count(&self) -> usize {
        self.pending
            .read()
            .unwrap_or_else(|poisoned| {
                warn!("Pending receipts lock poisoned, recovering");
                poisoned.into_inner()
            })
            .iter()
            .filter(|r| r.disputed)
            .count()
    }
}

/// Report returned from a flush operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlushReport {
    /// Total receipts in the batch
    pub total: usize,

    /// Federation receipts successfully proposed as transfers
    pub federation_flushed: usize,

    /// Commons receipts deferred (pending Epic 6)
    pub commons_deferred: usize,

    /// Disputed receipts skipped
    pub disputed_skipped: usize,

    /// Error messages for failed receipts
    pub errors: Vec<String>,
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_store::{SledStore, Store};
    use std::sync::Arc;

    fn test_did() -> String {
        KeyPair::generate().unwrap().did().to_string()
    }

    fn make_receipt(scope: ScopeLevel) -> ClearingReceipt {
        ClearingReceipt {
            receipt_hash: [0xAA; 32],
            executor_did: test_did(),
            executor_coop: "tech-coop".into(),
            submitter_did: test_did(),
            submitter_coop: "food-coop".into(),
            cost: 500,
            currency: "credits".into(),
            scope,
            created_at: current_timestamp(),
            disputed: false,
            dispute_reason: None,
        }
    }

    fn make_clearing_manager() -> ClearingManager {
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        ClearingManager::new(store, "food-coop".to_string()).unwrap()
    }

    // ---- submit_receipt ----

    #[test]
    fn test_submit_federation_receipt() {
        let mgr = ReceiptClearingManager::new(BatchClearingConfig::default());
        let receipt = make_receipt(ScopeLevel::Federation);
        mgr.submit_receipt(receipt).unwrap();
        assert_eq!(mgr.pending_count(), 1);
    }

    #[test]
    fn test_submit_commons_receipt() {
        let mgr = ReceiptClearingManager::new(BatchClearingConfig::default());
        let receipt = make_receipt(ScopeLevel::Commons);
        mgr.submit_receipt(receipt).unwrap();
        assert_eq!(mgr.pending_count(), 1);
    }

    #[test]
    fn test_submit_local_scope_rejected() {
        let mgr = ReceiptClearingManager::new(BatchClearingConfig::default());
        let receipt = make_receipt(ScopeLevel::Local);
        let err = mgr.submit_receipt(receipt).unwrap_err();
        assert!(err.to_string().contains("settled locally"));
    }

    #[test]
    fn test_submit_cell_scope_rejected() {
        let mgr = ReceiptClearingManager::new(BatchClearingConfig::default());
        let receipt = make_receipt(ScopeLevel::Cell);
        let err = mgr.submit_receipt(receipt).unwrap_err();
        assert!(err.to_string().contains("settled locally"));
    }

    #[test]
    fn test_submit_org_scope_rejected() {
        let mgr = ReceiptClearingManager::new(BatchClearingConfig::default());
        let receipt = make_receipt(ScopeLevel::Org);
        let err = mgr.submit_receipt(receipt).unwrap_err();
        assert!(err.to_string().contains("settled locally"));
    }

    #[test]
    fn test_submit_same_coop_rejected() {
        let mgr = ReceiptClearingManager::new(BatchClearingConfig::default());
        let mut receipt = make_receipt(ScopeLevel::Federation);
        receipt.executor_coop = "same-coop".into();
        receipt.submitter_coop = "same-coop".into();
        let err = mgr.submit_receipt(receipt).unwrap_err();
        assert!(err.to_string().contains("different cooperatives"));
    }

    #[test]
    fn test_submit_zero_cost_rejected() {
        let mgr = ReceiptClearingManager::new(BatchClearingConfig::default());
        let mut receipt = make_receipt(ScopeLevel::Federation);
        receipt.cost = 0;
        let err = mgr.submit_receipt(receipt).unwrap_err();
        assert!(err.to_string().contains("cost must be > 0"));
    }

    // ---- dispute_receipt ----

    #[test]
    fn test_dispute_receipt() {
        let mgr = ReceiptClearingManager::new(BatchClearingConfig::default());
        let receipt = make_receipt(ScopeLevel::Federation);
        let submitter = receipt.submitter_did.clone();
        let hash = receipt.receipt_hash;
        mgr.submit_receipt(receipt).unwrap();

        mgr.dispute_receipt(&hash, &submitter, "wrong amount".into())
            .unwrap();
        assert_eq!(mgr.disputed_count(), 1);
    }

    #[test]
    fn test_dispute_by_executor_accepted() {
        let mgr = ReceiptClearingManager::new(BatchClearingConfig::default());
        let receipt = make_receipt(ScopeLevel::Federation);
        let executor = receipt.executor_did.clone();
        let hash = receipt.receipt_hash;
        mgr.submit_receipt(receipt).unwrap();

        mgr.dispute_receipt(&hash, &executor, "incorrect metering".into())
            .unwrap();
        assert_eq!(mgr.disputed_count(), 1);
    }

    #[test]
    fn test_dispute_by_third_party_rejected() {
        let mgr = ReceiptClearingManager::new(BatchClearingConfig::default());
        let receipt = make_receipt(ScopeLevel::Federation);
        let hash = receipt.receipt_hash;
        mgr.submit_receipt(receipt).unwrap();

        let err = mgr
            .dispute_receipt(&hash, "did:icn:intruder", "fraud".into())
            .unwrap_err();
        assert!(err.to_string().contains("only submitter"));
        assert!(err.to_string().contains("or executor"));
    }

    #[test]
    fn test_dispute_nonexistent_receipt() {
        let mgr = ReceiptClearingManager::new(BatchClearingConfig::default());
        let hash = [0xFF; 32];
        let err = mgr
            .dispute_receipt(&hash, "anyone", "reason".into())
            .unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_dispute_already_disputed() {
        let mgr = ReceiptClearingManager::new(BatchClearingConfig::default());
        let receipt = make_receipt(ScopeLevel::Federation);
        let submitter = receipt.submitter_did.clone();
        let hash = receipt.receipt_hash;
        mgr.submit_receipt(receipt).unwrap();

        mgr.dispute_receipt(&hash, &submitter, "reason 1".into())
            .unwrap();
        let err = mgr
            .dispute_receipt(&hash, &submitter, "reason 2".into())
            .unwrap_err();
        assert!(err.to_string().contains("already disputed"));
    }

    // ---- should_flush ----

    #[test]
    fn test_should_flush_empty() {
        let mgr = ReceiptClearingManager::new(BatchClearingConfig::default());
        assert!(!mgr.should_flush());
    }

    #[test]
    fn test_should_flush_max_batch_size() {
        let config = BatchClearingConfig {
            max_batch_size: 3,
            max_batch_age_secs: 9999,
        };
        let mgr = ReceiptClearingManager::new(config);

        for i in 0..3 {
            let mut receipt = make_receipt(ScopeLevel::Federation);
            receipt.receipt_hash = [i as u8; 32];
            mgr.submit_receipt(receipt).unwrap();
        }

        assert!(mgr.should_flush());
    }

    #[test]
    fn test_should_flush_below_threshold() {
        let config = BatchClearingConfig {
            max_batch_size: 10,
            max_batch_age_secs: 9999,
        };
        let mgr = ReceiptClearingManager::new(config);
        mgr.submit_receipt(make_receipt(ScopeLevel::Federation))
            .unwrap();
        assert!(!mgr.should_flush());
    }

    // ---- flush_to_clearing ----

    #[test]
    fn test_flush_disputed_receipt_skipped() {
        let mgr = ReceiptClearingManager::new(BatchClearingConfig::default());
        let clearing = make_clearing_manager();

        let receipt = make_receipt(ScopeLevel::Federation);
        let submitter = receipt.submitter_did.clone();
        let hash = receipt.receipt_hash;
        mgr.submit_receipt(receipt).unwrap();

        mgr.dispute_receipt(&hash, &submitter, "wrong".into())
            .unwrap();

        let report = mgr.flush_to_clearing(&clearing).unwrap();
        assert_eq!(report.disputed_skipped, 1);
        assert_eq!(report.federation_flushed, 0);
        assert_eq!(mgr.pending_count(), 0);
    }

    #[test]
    fn test_flush_commons_receipt_deferred() {
        let mgr = ReceiptClearingManager::new(BatchClearingConfig::default());
        let clearing = make_clearing_manager();

        mgr.submit_receipt(make_receipt(ScopeLevel::Commons))
            .unwrap();

        let report = mgr.flush_to_clearing(&clearing).unwrap();
        assert_eq!(report.commons_deferred, 1);
        assert_eq!(report.federation_flushed, 0);
        assert_eq!(mgr.pending_count(), 0);
    }

    #[test]
    fn test_flush_federation_creates_transfer() {
        use crate::clearing::BilateralClearingAgreement;
        use icn_identity::KeyPair;

        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let clearing = ClearingManager::new(store, "food-coop".to_string()).unwrap();

        // Set up a bilateral agreement between the two coops
        let executor_kp = KeyPair::generate().unwrap();
        let submitter_kp = KeyPair::generate().unwrap();

        let agreement = BilateralClearingAgreement::new(
            "food-tech-agreement".to_string(),
            "food-coop".to_string(),
            submitter_kp.did().clone(),
            "tech-coop".to_string(),
            executor_kp.did().clone(),
        )
        .with_rate("credits", "credits", 1.0);
        clearing.create_agreement(agreement).unwrap();

        // Submit a federation receipt
        let mgr = ReceiptClearingManager::new(BatchClearingConfig::default());
        let mut receipt = ClearingReceipt {
            receipt_hash: [0xBB; 32],
            executor_did: executor_kp.did().to_string(),
            executor_coop: "tech-coop".into(),
            submitter_did: submitter_kp.did().to_string(),
            submitter_coop: "food-coop".into(),
            cost: 250,
            currency: "credits".into(),
            scope: ScopeLevel::Federation,
            created_at: current_timestamp(),
            disputed: false,
            dispute_reason: None,
        };
        // Use unique hash
        receipt.receipt_hash = [0xCC; 32];
        mgr.submit_receipt(receipt).unwrap();

        let report = mgr.flush_to_clearing(&clearing).unwrap();
        assert_eq!(report.federation_flushed, 1);
        assert_eq!(report.errors.len(), 0);
        assert_eq!(mgr.pending_count(), 0);

        // Verify the transfer was proposed in the clearing manager
        let position = clearing.calculate_position("food-tech-agreement").unwrap();
        assert_eq!(position.pending_transfers.len(), 1);
        assert_eq!(position.pending_transfers[0].source_amount, 250);
    }

    #[test]
    fn test_flush_mixed_batch() {
        use crate::clearing::BilateralClearingAgreement;
        use icn_identity::KeyPair;

        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let clearing = ClearingManager::new(store, "food-coop".to_string()).unwrap();

        let executor_kp = KeyPair::generate().unwrap();
        let submitter_kp = KeyPair::generate().unwrap();

        let agreement = BilateralClearingAgreement::new(
            "food-tech-agreement".to_string(),
            "food-coop".to_string(),
            submitter_kp.did().clone(),
            "tech-coop".to_string(),
            executor_kp.did().clone(),
        )
        .with_rate("credits", "credits", 1.0);
        clearing.create_agreement(agreement).unwrap();

        let mgr = ReceiptClearingManager::new(BatchClearingConfig::default());

        // 1: Federation (should flush)
        let mut r1 = ClearingReceipt {
            receipt_hash: [0x01; 32],
            executor_did: executor_kp.did().to_string(),
            executor_coop: "tech-coop".into(),
            submitter_did: submitter_kp.did().to_string(),
            submitter_coop: "food-coop".into(),
            cost: 100,
            currency: "credits".into(),
            scope: ScopeLevel::Federation,
            created_at: current_timestamp(),
            disputed: false,
            dispute_reason: None,
        };
        mgr.submit_receipt(r1.clone()).unwrap();

        // 2: Commons (should defer)
        let mut r2 = r1.clone();
        r2.receipt_hash = [0x02; 32];
        r2.scope = ScopeLevel::Commons;
        mgr.submit_receipt(r2).unwrap();

        // 3: Federation but disputed (should skip)
        r1.receipt_hash = [0x03; 32];
        let submitter = r1.submitter_did.clone();
        mgr.submit_receipt(r1).unwrap();
        mgr.dispute_receipt(&[0x03; 32], &submitter, "bad".into())
            .unwrap();

        assert_eq!(mgr.pending_count(), 3);

        let report = mgr.flush_to_clearing(&clearing).unwrap();
        assert_eq!(report.total, 3);
        assert_eq!(report.federation_flushed, 1);
        assert_eq!(report.commons_deferred, 1);
        assert_eq!(report.disputed_skipped, 1);
        assert_eq!(report.errors.len(), 0);
        assert_eq!(mgr.pending_count(), 0);
    }

    #[test]
    fn test_flush_resets_batch_timer() {
        let mgr = ReceiptClearingManager::new(BatchClearingConfig::default());
        let clearing = make_clearing_manager();

        mgr.submit_receipt(make_receipt(ScopeLevel::Commons))
            .unwrap();

        // Batch timer should be set
        let timer = mgr.batch_started_at.read().unwrap();
        assert!(timer.is_some());
        drop(timer);

        mgr.flush_to_clearing(&clearing).unwrap();

        // Batch timer should be reset
        let timer = mgr.batch_started_at.read().unwrap();
        assert!(timer.is_none());
    }

    #[test]
    fn test_pending_and_disputed_counts() {
        let mgr = ReceiptClearingManager::new(BatchClearingConfig::default());

        let r1 = make_receipt(ScopeLevel::Federation);
        let submitter = r1.submitter_did.clone();
        let hash = r1.receipt_hash;
        mgr.submit_receipt(r1).unwrap();

        let mut r2 = make_receipt(ScopeLevel::Federation);
        r2.receipt_hash = [0xBB; 32];
        mgr.submit_receipt(r2).unwrap();

        assert_eq!(mgr.pending_count(), 2);
        assert_eq!(mgr.disputed_count(), 0);

        mgr.dispute_receipt(&hash, &submitter, "test".into())
            .unwrap();
        assert_eq!(mgr.pending_count(), 2);
        assert_eq!(mgr.disputed_count(), 1);
    }

    #[test]
    fn test_clearing_receipt_serde_roundtrip() {
        let receipt = make_receipt(ScopeLevel::Federation);
        let json = serde_json::to_string(&receipt).unwrap();
        let deserialized: ClearingReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(receipt, deserialized);
    }

    #[test]
    fn test_flush_cost_overflow_rejected() {
        use crate::clearing::BilateralClearingAgreement;
        use icn_identity::KeyPair;

        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let clearing = ClearingManager::new(store, "food-coop".to_string()).unwrap();

        let executor_kp = KeyPair::generate().unwrap();
        let submitter_kp = KeyPair::generate().unwrap();

        let agreement = BilateralClearingAgreement::new(
            "food-tech-agreement".to_string(),
            "food-coop".to_string(),
            submitter_kp.did().clone(),
            "tech-coop".to_string(),
            executor_kp.did().clone(),
        )
        .with_rate("credits", "credits", 1.0);
        clearing.create_agreement(agreement).unwrap();

        let mgr = ReceiptClearingManager::new(BatchClearingConfig::default());
        let receipt = ClearingReceipt {
            receipt_hash: [0xDD; 32],
            executor_did: executor_kp.did().to_string(),
            executor_coop: "tech-coop".into(),
            submitter_did: submitter_kp.did().to_string(),
            submitter_coop: "food-coop".into(),
            cost: u64::MAX, // overflows i64
            currency: "credits".into(),
            scope: ScopeLevel::Federation,
            created_at: current_timestamp(),
            disputed: false,
            dispute_reason: None,
        };
        mgr.submit_receipt(receipt).unwrap();

        let report = mgr.flush_to_clearing(&clearing).unwrap();
        assert_eq!(report.federation_flushed, 0);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("overflows i64"));
    }

    // ---- submit_receipt guards ----

    #[test]
    fn test_submit_duplicate_hash_rejected() {
        let mgr = ReceiptClearingManager::new(BatchClearingConfig::default());
        let receipt = make_receipt(ScopeLevel::Federation);
        let receipt2 = receipt.clone();
        mgr.submit_receipt(receipt).unwrap();

        let err = mgr.submit_receipt(receipt2).unwrap_err();
        assert!(err.to_string().contains("duplicate receipt hash"));
    }

    #[test]
    fn test_submit_batch_full_rejected() {
        let config = BatchClearingConfig {
            max_batch_size: 3,
            max_batch_age_secs: 9999,
        };
        let mgr = ReceiptClearingManager::new(config);

        for i in 0..3u8 {
            let mut receipt = make_receipt(ScopeLevel::Federation);
            receipt.receipt_hash = [i; 32];
            mgr.submit_receipt(receipt).unwrap();
        }

        let mut receipt = make_receipt(ScopeLevel::Federation);
        receipt.receipt_hash = [0xFF; 32];
        let err = mgr.submit_receipt(receipt).unwrap_err();
        assert!(err.to_string().contains("batch full"));
    }
}
