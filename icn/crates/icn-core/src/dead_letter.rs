//! Dead Letter Queue for failed operations
//!
//! Provides persistent storage for operations that failed and need manual
//! reconciliation or automated retry. This ensures no failed operations
//! are silently lost.
//!
//! # Usage
//!
//! ```rust,ignore
//! use icn_core::dead_letter::{DeadLetterQueue, FailedOperation, FailureType};
//!
//! let dlq = DeadLetterQueue::new(store);
//!
//! // Record a failed operation
//! dlq.enqueue(FailedOperation {
//!     id: "proposal-123".to_string(),
//!     failure_type: FailureType::AuditTrailWrite,
//!     context: serde_json::json!({
//!         "proposal_id": "123",
//!         "ledger_hash": "abc...",
//!     }),
//!     error_message: "Storage write failed".to_string(),
//!     timestamp: SystemTime::now(),
//!     retry_count: 0,
//! })?;
//!
//! // Query failed operations
//! let pending = dlq.list_pending()?;
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::SystemTime;

/// Type of failure that caused the operation to be queued
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureType {
    /// Audit trail write failed after successful ledger entry
    AuditTrailWrite,
    /// Audit trail serialization failed
    AuditTrailSerialize,
    /// Idempotency check failed (couldn't verify if already executed)
    IdempotencyCheckFailed,
    /// Ledger entry append failed
    LedgerAppendFailed,
    /// Governance execution failed
    GovernanceExecutionFailed,
    /// Gossip publication failed
    GossipPublishFailed,
    /// Generic storage failure
    StorageFailure,
    /// Other failure type
    Other(String),
}

impl std::fmt::Display for FailureType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FailureType::AuditTrailWrite => write!(f, "audit_trail_write"),
            FailureType::AuditTrailSerialize => write!(f, "audit_trail_serialize"),
            FailureType::IdempotencyCheckFailed => write!(f, "idempotency_check_failed"),
            FailureType::LedgerAppendFailed => write!(f, "ledger_append_failed"),
            FailureType::GovernanceExecutionFailed => write!(f, "governance_execution_failed"),
            FailureType::GossipPublishFailed => write!(f, "gossip_publish_failed"),
            FailureType::StorageFailure => write!(f, "storage_failure"),
            FailureType::Other(s) => write!(f, "other:{}", s),
        }
    }
}

/// Status of a dead-letter queue entry
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryStatus {
    /// Pending - not yet processed
    Pending,
    /// Being retried
    Retrying,
    /// Successfully resolved
    Resolved,
    /// Permanently failed (manual intervention required)
    PermanentlyFailed,
}

/// A failed operation stored in the dead-letter queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedOperation {
    /// Unique identifier for this failed operation
    pub id: String,
    /// Type of failure
    pub failure_type: FailureType,
    /// JSON context with all relevant information for retry/debugging
    pub context: serde_json::Value,
    /// Human-readable error message
    pub error_message: String,
    /// When the failure occurred
    pub timestamp: SystemTime,
    /// Number of retry attempts
    pub retry_count: u32,
    /// Current status
    pub status: EntryStatus,
    /// When last retry was attempted (if any)
    pub last_retry: Option<SystemTime>,
    /// Resolution notes (if resolved)
    pub resolution_notes: Option<String>,
}

impl FailedOperation {
    /// Create a new failed operation entry
    pub fn new(
        id: impl Into<String>,
        failure_type: FailureType,
        context: serde_json::Value,
        error_message: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            failure_type,
            context,
            error_message: error_message.into(),
            timestamp: SystemTime::now(),
            retry_count: 0,
            status: EntryStatus::Pending,
            last_retry: None,
            resolution_notes: None,
        }
    }

    /// Create from governance audit trail failure
    pub fn audit_trail_failure(
        proposal_id: &str,
        ledger_hash: &str,
        amount: i64,
        currency: &str,
        recipient: &str,
        error: &str,
    ) -> Self {
        Self::new(
            format!("audit:{}", proposal_id),
            FailureType::AuditTrailWrite,
            serde_json::json!({
                "proposal_id": proposal_id,
                "ledger_hash": ledger_hash,
                "amount": amount,
                "currency": currency,
                "recipient": recipient,
            }),
            error,
        )
    }

    /// Create from idempotency check failure
    pub fn idempotency_check_failure(proposal_id: &str, error: &str) -> Self {
        Self::new(
            format!("idem:{}", proposal_id),
            FailureType::IdempotencyCheckFailed,
            serde_json::json!({
                "proposal_id": proposal_id,
            }),
            error,
        )
    }

    /// Mark as being retried
    pub fn mark_retrying(&mut self) {
        self.status = EntryStatus::Retrying;
        self.retry_count += 1;
        self.last_retry = Some(SystemTime::now());
    }

    /// Mark as resolved
    pub fn mark_resolved(&mut self, notes: impl Into<String>) {
        self.status = EntryStatus::Resolved;
        self.resolution_notes = Some(notes.into());
    }

    /// Mark as permanently failed
    pub fn mark_permanently_failed(&mut self, notes: impl Into<String>) {
        self.status = EntryStatus::PermanentlyFailed;
        self.resolution_notes = Some(notes.into());
    }
}

/// Dead Letter Queue for storing and managing failed operations
pub struct DeadLetterQueue<S> {
    store: Arc<S>,
}

impl<S> DeadLetterQueue<S>
where
    S: icn_store::Store,
{
    /// Key prefix for dead-letter queue entries
    const PREFIX: &'static [u8] = b"dlq:";

    /// Create a new dead-letter queue
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    /// Generate storage key for an entry
    fn entry_key(id: &str) -> Vec<u8> {
        let mut key = Self::PREFIX.to_vec();
        key.extend_from_slice(id.as_bytes());
        key
    }

    /// Enqueue a failed operation
    pub fn enqueue(&self, operation: FailedOperation) -> Result<()> {
        let key = Self::entry_key(&operation.id);
        let value =
            serde_json::to_vec(&operation).context("Failed to serialize failed operation")?;

        self.store
            .put(&key, &value)
            .context("Failed to store failed operation")?;

        // Update metrics
        icn_obs::metrics::core::dead_letter_enqueued_inc(&operation.failure_type.to_string());

        tracing::warn!(
            id = %operation.id,
            failure_type = %operation.failure_type,
            error = %operation.error_message,
            "Operation added to dead-letter queue"
        );

        Ok(())
    }

    /// Get a specific entry by ID
    pub fn get(&self, id: &str) -> Result<Option<FailedOperation>> {
        let key = Self::entry_key(id);
        match self.store.get(&key)? {
            Some(data) => {
                let operation: FailedOperation = serde_json::from_slice(&data)
                    .context("Failed to deserialize failed operation")?;
                Ok(Some(operation))
            }
            None => Ok(None),
        }
    }

    /// Update an existing entry
    pub fn update(&self, operation: &FailedOperation) -> Result<()> {
        let key = Self::entry_key(&operation.id);
        let value =
            serde_json::to_vec(operation).context("Failed to serialize failed operation")?;

        self.store
            .put(&key, &value)
            .context("Failed to update failed operation")?;

        Ok(())
    }

    /// Remove an entry (after resolution)
    pub fn remove(&self, id: &str) -> Result<()> {
        let key = Self::entry_key(id);
        self.store
            .delete(&key)
            .context("Failed to remove failed operation")?;

        icn_obs::metrics::core::dead_letter_resolved_inc();

        Ok(())
    }

    /// List all entries with a given status
    pub fn list_by_status(&self, status: EntryStatus) -> Result<Vec<FailedOperation>> {
        let entries = self.store.scan(Self::PREFIX)?;
        let mut result = Vec::new();

        for (_key, value) in entries {
            let operation: FailedOperation =
                serde_json::from_slice(&value).context("Failed to deserialize failed operation")?;
            if operation.status == status {
                result.push(operation);
            }
        }

        // Sort by timestamp (oldest first)
        result.sort_by_key(|op| op.timestamp);
        Ok(result)
    }

    /// List all pending entries
    pub fn list_pending(&self) -> Result<Vec<FailedOperation>> {
        self.list_by_status(EntryStatus::Pending)
    }

    /// List all entries (for admin review)
    pub fn list_all(&self) -> Result<Vec<FailedOperation>> {
        let entries = self.store.scan(Self::PREFIX)?;
        let mut result = Vec::new();

        for (_key, value) in entries {
            let operation: FailedOperation =
                serde_json::from_slice(&value).context("Failed to deserialize failed operation")?;
            result.push(operation);
        }

        // Sort by timestamp (oldest first)
        result.sort_by_key(|op| op.timestamp);
        Ok(result)
    }

    /// Count entries by status
    pub fn count_by_status(&self) -> Result<std::collections::HashMap<String, usize>> {
        let entries = self.store.scan(Self::PREFIX)?;
        let mut counts = std::collections::HashMap::new();

        for (_key, value) in entries {
            let operation: FailedOperation =
                serde_json::from_slice(&value).context("Failed to deserialize failed operation")?;
            let status_str = format!("{:?}", operation.status);
            *counts.entry(status_str).or_insert(0) += 1;
        }

        Ok(counts)
    }

    /// Get entries that are ready for retry (pending or failed retries with backoff)
    pub fn get_retryable(&self, max_retries: u32) -> Result<Vec<FailedOperation>> {
        let pending = self.list_pending()?;

        // Filter to entries under retry limit
        let retryable: Vec<_> = pending
            .into_iter()
            .filter(|op| op.retry_count < max_retries)
            .collect();

        Ok(retryable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_store::SledStore;

    fn test_store() -> Arc<SledStore> {
        Arc::new(SledStore::temporary().unwrap())
    }

    #[test]
    fn test_enqueue_and_get() {
        let store = test_store();
        let dlq = DeadLetterQueue::new(store);

        let op = FailedOperation::new(
            "test-op-1",
            FailureType::AuditTrailWrite,
            serde_json::json!({"test": "data"}),
            "Test error",
        );

        dlq.enqueue(op.clone()).unwrap();

        let retrieved = dlq.get("test-op-1").unwrap().unwrap();
        assert_eq!(retrieved.id, "test-op-1");
        assert_eq!(retrieved.failure_type, FailureType::AuditTrailWrite);
        assert_eq!(retrieved.error_message, "Test error");
        assert_eq!(retrieved.status, EntryStatus::Pending);
    }

    #[test]
    fn test_list_pending() {
        let store = test_store();
        let dlq = DeadLetterQueue::new(store);

        // Add some operations
        for i in 0..3 {
            let op = FailedOperation::new(
                format!("op-{}", i),
                FailureType::StorageFailure,
                serde_json::json!({}),
                "Error",
            );
            dlq.enqueue(op).unwrap();
        }

        let pending = dlq.list_pending().unwrap();
        assert_eq!(pending.len(), 3);
    }

    #[test]
    fn test_update_status() {
        let store = test_store();
        let dlq = DeadLetterQueue::new(store);

        let mut op = FailedOperation::new(
            "test-op",
            FailureType::LedgerAppendFailed,
            serde_json::json!({}),
            "Error",
        );
        dlq.enqueue(op.clone()).unwrap();

        // Mark as resolved
        op.mark_resolved("Fixed manually");
        dlq.update(&op).unwrap();

        let retrieved = dlq.get("test-op").unwrap().unwrap();
        assert_eq!(retrieved.status, EntryStatus::Resolved);
        assert_eq!(
            retrieved.resolution_notes,
            Some("Fixed manually".to_string())
        );
    }

    #[test]
    fn test_remove() {
        let store = test_store();
        let dlq = DeadLetterQueue::new(store);

        let op = FailedOperation::new(
            "to-remove",
            FailureType::Other("test".to_string()),
            serde_json::json!({}),
            "Error",
        );
        dlq.enqueue(op).unwrap();

        assert!(dlq.get("to-remove").unwrap().is_some());

        dlq.remove("to-remove").unwrap();

        assert!(dlq.get("to-remove").unwrap().is_none());
    }

    #[test]
    fn test_audit_trail_failure_helper() {
        let op = FailedOperation::audit_trail_failure(
            "prop-123",
            "abc123",
            1000,
            "USD",
            "did:icn:recipient",
            "Storage full",
        );

        assert_eq!(op.id, "audit:prop-123");
        assert_eq!(op.failure_type, FailureType::AuditTrailWrite);
        assert_eq!(op.context["proposal_id"], "prop-123");
        assert_eq!(op.context["amount"], 1000);
    }

    #[test]
    fn test_retry_tracking() {
        let store = test_store();
        let dlq = DeadLetterQueue::new(store);

        let mut op = FailedOperation::new(
            "retry-test",
            FailureType::GossipPublishFailed,
            serde_json::json!({}),
            "Network error",
        );
        dlq.enqueue(op.clone()).unwrap();

        // Simulate retry
        op.mark_retrying();
        assert_eq!(op.retry_count, 1);
        assert_eq!(op.status, EntryStatus::Retrying);
        assert!(op.last_retry.is_some());

        dlq.update(&op).unwrap();

        let retrieved = dlq.get("retry-test").unwrap().unwrap();
        assert_eq!(retrieved.retry_count, 1);
    }

    #[test]
    fn test_get_retryable() {
        let store = test_store();
        let dlq = DeadLetterQueue::new(store);

        // Add operation with 0 retries
        let op1 = FailedOperation::new(
            "op1",
            FailureType::StorageFailure,
            serde_json::json!({}),
            "E",
        );
        dlq.enqueue(op1).unwrap();

        // Add operation with max retries
        let mut op2 = FailedOperation::new(
            "op2",
            FailureType::StorageFailure,
            serde_json::json!({}),
            "E",
        );
        op2.retry_count = 5;
        dlq.enqueue(op2).unwrap();

        let retryable = dlq.get_retryable(3).unwrap();
        assert_eq!(retryable.len(), 1);
        assert_eq!(retryable[0].id, "op1");
    }
}
