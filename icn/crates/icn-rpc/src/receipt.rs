//! Receipt tracking for RPC operations
//!
//! This module provides operation receipts that track the outcome of
//! async RPC operations. Receipts allow clients to query the status
//! of operations after they complete.
//!
//! # Architecture
//!
//! - **Receipt**: Immutable record of an operation's outcome
//! - **ReceiptStore**: TTL-bounded LRU cache for receipt lookup
//! - **ReceiptId**: UUID-based unique identifier
//!
//! # Usage
//!
//! ```rust,no_run
//! use icn_rpc::receipt::{Receipt, ReceiptStore, Operation, Outcome};
//! use icn_identity::KeyPair;
//!
//! # async fn example() {
//! let store = ReceiptStore::new(10_000, 86400); // 10k receipts, 24h TTL
//!
//! // Create receipt for completed operation
//! let caller = KeyPair::generate().unwrap().did().clone();
//! let receipt = Receipt::new(
//!     caller,
//!     Operation::ContractDeploy { code_hash: "abc123".to_string() },
//!     Outcome::success(Some("commit_hash".to_string())),
//! );
//!
//! // Store it
//! store.insert(receipt.clone()).await;
//!
//! // Look it up later
//! if let Some(r) = store.get(&receipt.id).await {
//!     println!("Operation succeeded: {:?}", r.outcome);
//! }
//! # }
//! ```

use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Unique receipt identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReceiptId(pub String);

impl ReceiptId {
    /// Generate a new random receipt ID
    pub fn new() -> Self {
        ReceiptId(Uuid::new_v4().to_string())
    }

    /// Create from existing string
    pub fn from_string(s: String) -> Self {
        ReceiptId(s)
    }
}

impl Default for ReceiptId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ReceiptId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Receipt for an RPC operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    /// Unique receipt ID
    pub id: ReceiptId,

    /// When the operation completed (Unix timestamp in seconds)
    pub timestamp: u64,

    /// Who requested the operation
    pub caller: Did,

    /// The operation that was performed
    pub operation: Operation,

    /// Outcome of the operation
    pub outcome: Outcome,

    /// Resource usage metrics
    pub resources: Resources,
}

impl Receipt {
    /// Create a new receipt
    pub fn new(caller: Did, operation: Operation, outcome: Outcome) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Receipt {
            id: ReceiptId::new(),
            timestamp,
            caller,
            operation,
            outcome,
            resources: Resources::default(),
        }
    }

    /// Create a new receipt with resource tracking
    pub fn with_resources(
        caller: Did,
        operation: Operation,
        outcome: Outcome,
        resources: Resources,
    ) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Receipt {
            id: ReceiptId::new(),
            timestamp,
            caller,
            operation,
            outcome,
            resources,
        }
    }
}

/// Type of operation performed
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Operation {
    /// Contract deployment
    ContractDeploy { code_hash: String },

    /// Contract execution
    ContractExecute { code_hash: String, rule: String },

    /// Ledger transfer
    LedgerTransfer {
        from: Did,
        to: Did,
        amount: i128,
    },

    /// Trust edge modification
    TrustEdgeAdd { from: Did, to: Did, score: f32 },
}

/// Outcome of an operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    /// Operation succeeded
    Success {
        /// Optional commit hash or transaction ID
        commit_hash: Option<String>,
    },

    /// Operation failed
    Failure {
        /// Error message
        error: String,
    },
}

impl Outcome {
    /// Create a success outcome
    pub fn success(commit_hash: Option<String>) -> Self {
        Outcome::Success { commit_hash }
    }

    /// Create a failure outcome
    pub fn failure(error: String) -> Self {
        Outcome::Failure { error }
    }

    /// Check if operation succeeded
    pub fn is_success(&self) -> bool {
        matches!(self, Outcome::Success { .. })
    }

    /// Check if operation failed
    pub fn is_failure(&self) -> bool {
        matches!(self, Outcome::Failure { .. })
    }
}

/// Resource usage metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Resources {
    /// Fuel consumed (for contract execution)
    pub fuel_used: u64,

    /// Bytes processed
    pub bytes_processed: usize,

    /// Wall time in milliseconds
    pub wall_time_ms: u64,
}

/// Receipt store with TTL-based eviction
pub struct ReceiptStore {
    /// Receipts by ID
    receipts: Arc<RwLock<HashMap<ReceiptId, Receipt>>>,

    /// Maximum number of receipts to store
    max_size: usize,

    /// TTL in seconds
    ttl_seconds: u64,
}

impl ReceiptStore {
    /// Create a new receipt store
    ///
    /// # Arguments
    ///
    /// * `max_size` - Maximum number of receipts (LRU eviction when exceeded)
    /// * `ttl_seconds` - Time-to-live for receipts (e.g., 86400 = 24 hours)
    pub fn new(max_size: usize, ttl_seconds: u64) -> Self {
        ReceiptStore {
            receipts: Arc::new(RwLock::new(HashMap::new())),
            max_size,
            ttl_seconds,
        }
    }

    /// Insert a receipt into the store
    pub async fn insert(&self, receipt: Receipt) {
        let mut receipts = self.receipts.write().await;

        // Evict expired receipts
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        receipts.retain(|_, r| now - r.timestamp < self.ttl_seconds);

        // Evict oldest if at capacity (simple LRU)
        if receipts.len() >= self.max_size {
            if let Some(oldest_id) = receipts
                .iter()
                .min_by_key(|(_, r)| r.timestamp)
                .map(|(id, _)| id.clone())
            {
                receipts.remove(&oldest_id);
            }
        }

        receipts.insert(receipt.id.clone(), receipt);
    }

    /// Get a receipt by ID
    pub async fn get(&self, id: &ReceiptId) -> Option<Receipt> {
        let receipts = self.receipts.read().await;
        receipts.get(id).cloned()
    }

    /// Get current size of the store
    pub async fn size(&self) -> usize {
        let receipts = self.receipts.read().await;
        receipts.len()
    }

    /// Clear all receipts (for testing)
    #[cfg(test)]
    pub async fn clear(&self) {
        let mut receipts = self.receipts.write().await;
        receipts.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use tokio::time::{sleep, Duration};

    fn create_test_receipt() -> Receipt {
        let caller = KeyPair::generate().unwrap().did().clone();
        Receipt::new(
            caller,
            Operation::ContractDeploy {
                code_hash: "abc123".to_string(),
            },
            Outcome::success(Some("commit_abc".to_string())),
        )
    }

    #[test]
    fn test_receipt_id_generation() {
        let id1 = ReceiptId::new();
        let id2 = ReceiptId::new();

        assert_ne!(id1, id2, "Receipt IDs should be unique");
    }

    #[test]
    fn test_receipt_creation() {
        let caller = KeyPair::generate().unwrap().did().clone();
        let receipt = Receipt::new(
            caller.clone(),
            Operation::ContractDeploy {
                code_hash: "test_hash".to_string(),
            },
            Outcome::success(None),
        );

        assert_eq!(receipt.caller, caller);
        assert!(receipt.outcome.is_success());
        assert!(!receipt.outcome.is_failure());
        assert_eq!(receipt.resources.fuel_used, 0);
    }

    #[test]
    fn test_receipt_with_resources() {
        let caller = KeyPair::generate().unwrap().did().clone();
        let resources = Resources {
            fuel_used: 1000,
            bytes_processed: 2048,
            wall_time_ms: 150,
        };

        let receipt = Receipt::with_resources(
            caller,
            Operation::ContractExecute {
                code_hash: "contract_hash".to_string(),
                rule: "process".to_string(),
            },
            Outcome::success(Some("tx_123".to_string())),
            resources.clone(),
        );

        assert_eq!(receipt.resources.fuel_used, 1000);
        assert_eq!(receipt.resources.bytes_processed, 2048);
        assert_eq!(receipt.resources.wall_time_ms, 150);
    }

    #[test]
    fn test_outcome_types() {
        let success = Outcome::success(Some("hash".to_string()));
        assert!(success.is_success());
        assert!(!success.is_failure());

        let failure = Outcome::failure("Error occurred".to_string());
        assert!(!failure.is_success());
        assert!(failure.is_failure());
    }

    #[tokio::test]
    async fn test_receipt_store_insert_and_get() {
        let store = ReceiptStore::new(100, 3600);
        let receipt = create_test_receipt();
        let id = receipt.id.clone();

        store.insert(receipt.clone()).await;

        let retrieved = store.get(&id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, id);
    }

    #[tokio::test]
    async fn test_receipt_store_size_limit() {
        let store = ReceiptStore::new(5, 3600);

        // Insert 10 receipts
        for _ in 0..10 {
            let receipt = create_test_receipt();
            store.insert(receipt).await;
        }

        // Should only keep 5 (the limit)
        let size = store.size().await;
        assert_eq!(size, 5, "Store should respect size limit");
    }

    #[tokio::test]
    async fn test_receipt_store_ttl_eviction() {
        let store = ReceiptStore::new(100, 1); // 1 second TTL

        let receipt = create_test_receipt();
        let id = receipt.id.clone();

        store.insert(receipt).await;

        // Should be retrievable immediately
        assert!(store.get(&id).await.is_some());

        // Wait for TTL to expire
        sleep(Duration::from_secs(2)).await;

        // Insert another receipt to trigger cleanup
        let new_receipt = create_test_receipt();
        store.insert(new_receipt).await;

        // Old receipt should be gone
        assert!(store.get(&id).await.is_none(), "Expired receipt should be evicted");
    }

    #[tokio::test]
    async fn test_receipt_serialization() {
        let receipt = create_test_receipt();

        // Serialize
        let json = serde_json::to_string(&receipt).unwrap();

        // Deserialize
        let deserialized: Receipt = serde_json::from_str(&json).unwrap();

        assert_eq!(receipt.id, deserialized.id);
        assert_eq!(receipt.caller, deserialized.caller);
    }

    #[tokio::test]
    async fn test_receipt_store_concurrent_access() {
        let store = Arc::new(ReceiptStore::new(1000, 3600));
        let mut handles = vec![];

        // Spawn 10 tasks inserting receipts concurrently
        for _ in 0..10 {
            let store_clone = store.clone();
            handles.push(tokio::spawn(async move {
                for _ in 0..10 {
                    let receipt = create_test_receipt();
                    store_clone.insert(receipt).await;
                }
            }));
        }

        // Wait for all tasks
        for handle in handles {
            handle.await.unwrap();
        }

        // Should have 100 receipts
        let size = store.size().await;
        assert_eq!(size, 100);
    }
}
