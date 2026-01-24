//! Ledger event types for real-time notifications
//!
//! This module defines events emitted by the ledger during transaction processing.
//! These events can be consumed by:
//! - WebSocket subscriptions for real-time updates
//! - Notification systems for push/email alerts
//! - Analytics and monitoring systems
//! - Event-sourced state synchronization

use crate::types::ContentHash;
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;

/// Events emitted by the ledger during transaction processing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type", content = "payload")]
pub enum LedgerEvent {
    /// A new transaction has been created and appended to the ledger
    TransactionCreated(TransactionCreated),

    /// A transaction has been confirmed (included in consensus/sync)
    TransactionConfirmed(TransactionConfirmed),

    /// An account's balance has changed
    BalanceChanged(BalanceChanged),

    /// Multiple balance changes in a single transaction (batch update)
    BatchBalanceChanged(BatchBalanceChanged),

    /// A member's account has been frozen
    MemberFrozen(MemberFrozen),

    /// A member's account has been unfrozen
    MemberUnfrozen(MemberUnfrozen),

    /// A fork was detected in the ledger
    ForkDetected(ForkDetected),

    /// A fork was resolved
    ForkResolved(ForkResolved),

    /// A ledger rollback occurred
    RollbackPerformed(RollbackPerformed),

    /// A cross-currency transfer was executed
    CrossCurrencyTransfer(CrossCurrencyTransfer),

    /// Resource access was transferred to a new holder
    ResourceAccessTransferred(ResourceAccessTransferred),
}

/// Event: A new transaction was created
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionCreated {
    /// Hash of the journal entry
    pub entry_hash: String,

    /// Author who created the transaction
    pub author: String,

    /// List of transfers in this transaction
    pub transfers: Vec<Transfer>,

    /// Unix timestamp when created
    pub timestamp: u64,

    /// Domain/cooperative this transaction belongs to (if known)
    pub domain_id: Option<String>,
}

/// A single transfer within a transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transfer {
    /// Sender DID (who is debited)
    pub from: String,

    /// Receiver DID (who is credited)
    pub to: String,

    /// Amount transferred
    pub amount: i64,

    /// Currency symbol
    pub currency: String,
}

/// Event: A transaction was confirmed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionConfirmed {
    /// Hash of the journal entry
    pub entry_hash: String,

    /// Number of confirmations/acknowledgements
    pub confirmations: u32,

    /// Unix timestamp when confirmed
    pub confirmed_at: u64,
}

/// Event: An account balance changed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceChanged {
    /// Account DID whose balance changed
    pub account: String,

    /// Currency that changed
    pub currency: String,

    /// Previous balance
    pub old_balance: i64,

    /// New balance
    pub new_balance: i64,

    /// Change amount (positive = credit, negative = debit)
    pub change: i64,

    /// Entry hash that caused this change
    pub entry_hash: String,

    /// Unix timestamp
    pub timestamp: u64,

    /// Domain/cooperative this belongs to (if known)
    pub domain_id: Option<String>,
}

/// Event: Multiple balance changes in a batch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchBalanceChanged {
    /// Entry hash that caused these changes
    pub entry_hash: String,

    /// All balance changes
    pub changes: Vec<BalanceChanged>,

    /// Unix timestamp
    pub timestamp: u64,
}

/// Event: A member was frozen
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberFrozen {
    /// DID of the frozen member
    pub member: String,

    /// Reason for freezing
    pub reason: String,

    /// Who initiated the freeze (if known)
    pub frozen_by: Option<String>,

    /// Associated governance proposal (if any)
    pub proposal_id: Option<String>,

    /// Duration in seconds (None = indefinite)
    pub duration_seconds: Option<u64>,

    /// Unix timestamp when frozen
    pub frozen_at: u64,
}

/// Event: A member was unfrozen
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberUnfrozen {
    /// DID of the unfrozen member
    pub member: String,

    /// Reason for unfreezing
    pub reason: String,

    /// Who initiated the unfreeze (if known)
    pub unfrozen_by: Option<String>,

    /// Associated governance proposal (if any)
    pub proposal_id: Option<String>,

    /// Unix timestamp when unfrozen
    pub unfrozen_at: u64,
}

/// Event: A fork was detected
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkDetected {
    /// Common parent hash where fork occurred
    pub parent_hash: String,

    /// Conflicting entry hashes
    pub conflicting_entries: Vec<String>,

    /// Unix timestamp when detected
    pub detected_at: u64,
}

/// Event: A fork was resolved
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkResolved {
    /// Common parent hash where fork was
    pub parent_hash: String,

    /// Entry hash that was kept
    pub kept_entry: String,

    /// Entry hashes that were quarantined
    pub quarantined_entries: Vec<String>,

    /// Resolution strategy used
    pub resolution_strategy: String,

    /// Unix timestamp when resolved
    pub resolved_at: u64,
}

/// Event: A rollback was performed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackPerformed {
    /// Entry hash rolled back to
    pub target_hash: String,

    /// Number of entries archived
    pub archived_count: usize,

    /// Reason for rollback
    pub reason: String,

    /// Unix timestamp when performed
    pub performed_at: u64,
}

/// Event: A cross-currency transfer was executed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossCurrencyTransfer {
    /// Hash of the journal entry
    pub entry_hash: String,

    /// Sender DID
    pub from: String,

    /// Recipient DID
    pub to: String,

    /// Amount debited from sender
    pub source_amount: i64,

    /// Source currency
    pub from_currency: String,

    /// Gross amount before fees
    pub gross_target_amount: i64,

    /// Fee amount (sent to treasury)
    pub fee_amount: i64,

    /// Net amount credited to recipient
    pub net_target_amount: i64,

    /// Target currency
    pub to_currency: String,

    /// Exchange rate used
    pub rate: f64,

    /// When the rate was fetched
    pub rate_timestamp: u64,

    /// Sources that provided the rate
    pub rate_sources: Vec<String>,

    /// Unix timestamp of the transfer
    pub timestamp: u64,

    /// Domain/cooperative this belongs to (if known)
    pub domain_id: Option<String>,
}

/// Event: Resource access was transferred to a new holder
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceAccessTransferred {
    /// Resource ID
    pub resource_id: String,

    /// Previous holder entity ID
    pub from_holder: String,

    /// New holder entity ID
    pub to_holder: String,

    /// Optional transfer price (None = free transfer)
    pub price: Option<i64>,

    /// Access model (UseAccess or Stewardship)
    pub access_model: String,

    /// Unix timestamp when transferred
    pub transferred_at: u64,

    /// Cooperative ID that owns/manages this resource (None for backward compatibility)
    pub coop_id: Option<String>,
}

/// Event emitter for broadcasting ledger events
///
/// Uses a broadcast channel to allow multiple subscribers to receive events.
#[derive(Clone)]
pub struct LedgerEventEmitter {
    sender: broadcast::Sender<LedgerEvent>,
}

impl Default for LedgerEventEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl LedgerEventEmitter {
    /// Create a new event emitter with default capacity (1024 events)
    pub fn new() -> Self {
        Self::with_capacity(1024)
    }

    /// Create a new event emitter with specified capacity
    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        LedgerEventEmitter { sender }
    }

    /// Emit an event to all subscribers
    ///
    /// Returns the number of subscribers that received the event.
    /// If no subscribers, returns 0 (events are dropped).
    pub fn emit(&self, event: LedgerEvent) -> usize {
        self.sender.send(event).unwrap_or_default()
    }

    /// Subscribe to ledger events
    ///
    /// Returns a receiver that will receive all future events.
    pub fn subscribe(&self) -> broadcast::Receiver<LedgerEvent> {
        self.sender.subscribe()
    }

    /// Get the number of active subscribers
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }

    // === Convenience methods for emitting specific events ===

    /// Emit a TransactionCreated event
    pub fn emit_transaction_created(
        &self,
        entry_hash: ContentHash,
        author: &Did,
        transfers: Vec<Transfer>,
        timestamp: u64,
        domain_id: Option<String>,
    ) -> usize {
        self.emit(LedgerEvent::TransactionCreated(TransactionCreated {
            entry_hash: entry_hash.to_hex(),
            author: author.to_string(),
            transfers,
            timestamp,
            domain_id,
        }))
    }

    /// Emit a BalanceChanged event
    pub fn emit_balance_changed(
        &self,
        account: &Did,
        currency: &str,
        old_balance: i64,
        new_balance: i64,
        entry_hash: &ContentHash,
        timestamp: u64,
        domain_id: Option<String>,
    ) -> usize {
        self.emit(LedgerEvent::BalanceChanged(BalanceChanged {
            account: account.to_string(),
            currency: currency.to_string(),
            old_balance,
            new_balance,
            change: new_balance - old_balance,
            entry_hash: entry_hash.to_hex(),
            timestamp,
            domain_id,
        }))
    }

    /// Emit a BatchBalanceChanged event
    pub fn emit_batch_balance_changed(
        &self,
        entry_hash: &ContentHash,
        changes: Vec<BalanceChanged>,
        timestamp: u64,
    ) -> usize {
        self.emit(LedgerEvent::BatchBalanceChanged(BatchBalanceChanged {
            entry_hash: entry_hash.to_hex(),
            changes,
            timestamp,
        }))
    }

    /// Emit a MemberFrozen event
    pub fn emit_member_frozen(
        &self,
        member: &Did,
        reason: String,
        frozen_by: Option<&Did>,
        proposal_id: Option<String>,
        duration_seconds: Option<u64>,
        frozen_at: u64,
    ) -> usize {
        self.emit(LedgerEvent::MemberFrozen(MemberFrozen {
            member: member.to_string(),
            reason,
            frozen_by: frozen_by.map(|d| d.to_string()),
            proposal_id,
            duration_seconds,
            frozen_at,
        }))
    }

    /// Emit a MemberUnfrozen event
    pub fn emit_member_unfrozen(
        &self,
        member: &Did,
        reason: String,
        unfrozen_by: Option<&Did>,
        proposal_id: Option<String>,
        unfrozen_at: u64,
    ) -> usize {
        self.emit(LedgerEvent::MemberUnfrozen(MemberUnfrozen {
            member: member.to_string(),
            reason,
            unfrozen_by: unfrozen_by.map(|d| d.to_string()),
            proposal_id,
            unfrozen_at,
        }))
    }

    /// Emit a ForkDetected event
    pub fn emit_fork_detected(
        &self,
        parent_hash: &ContentHash,
        conflicting_entries: Vec<ContentHash>,
        detected_at: u64,
    ) -> usize {
        self.emit(LedgerEvent::ForkDetected(ForkDetected {
            parent_hash: parent_hash.to_hex(),
            conflicting_entries: conflicting_entries.iter().map(|h| h.to_hex()).collect(),
            detected_at,
        }))
    }

    /// Emit a ForkResolved event
    pub fn emit_fork_resolved(
        &self,
        parent_hash: &ContentHash,
        kept_entry: &ContentHash,
        quarantined_entries: Vec<ContentHash>,
        resolution_strategy: &str,
        resolved_at: u64,
    ) -> usize {
        self.emit(LedgerEvent::ForkResolved(ForkResolved {
            parent_hash: parent_hash.to_hex(),
            kept_entry: kept_entry.to_hex(),
            quarantined_entries: quarantined_entries.iter().map(|h| h.to_hex()).collect(),
            resolution_strategy: resolution_strategy.to_string(),
            resolved_at,
        }))
    }

    /// Emit a RollbackPerformed event
    pub fn emit_rollback_performed(
        &self,
        target_hash: &ContentHash,
        archived_count: usize,
        reason: &str,
        performed_at: u64,
    ) -> usize {
        self.emit(LedgerEvent::RollbackPerformed(RollbackPerformed {
            target_hash: target_hash.to_hex(),
            archived_count,
            reason: reason.to_string(),
            performed_at,
        }))
    }

    /// Emit a CrossCurrencyTransfer event
    #[allow(clippy::too_many_arguments)]
    pub fn emit_cross_currency_transfer(
        &self,
        entry_hash: &ContentHash,
        from: &Did,
        to: &Did,
        source_amount: i64,
        from_currency: &str,
        gross_target_amount: i64,
        fee_amount: i64,
        net_target_amount: i64,
        to_currency: &str,
        rate: f64,
        rate_timestamp: u64,
        rate_sources: Vec<String>,
        timestamp: u64,
        domain_id: Option<String>,
    ) -> usize {
        self.emit(LedgerEvent::CrossCurrencyTransfer(CrossCurrencyTransfer {
            entry_hash: entry_hash.to_hex(),
            from: from.to_string(),
            to: to.to_string(),
            source_amount,
            from_currency: from_currency.to_string(),
            gross_target_amount,
            fee_amount,
            net_target_amount,
            to_currency: to_currency.to_string(),
            rate,
            rate_timestamp,
            rate_sources,
            timestamp,
            domain_id,
        }))
    }

    /// Emit a ResourceAccessTransferred event
    pub fn emit_resource_access_transferred(
        &self,
        resource_id: String,
        from_holder: String,
        to_holder: String,
        price: Option<i64>,
        access_model: String,
        transferred_at: u64,
        coop_id: Option<String>,
    ) -> usize {
        self.emit(LedgerEvent::ResourceAccessTransferred(
            ResourceAccessTransferred {
                resource_id,
                from_holder,
                to_holder,
                price,
                access_model,
                transferred_at,
                coop_id,
            },
        ))
    }
}

/// Shared event emitter that can be passed to multiple components
pub type SharedEventEmitter = Arc<LedgerEventEmitter>;

/// Create a new shared event emitter
pub fn create_shared_emitter() -> SharedEventEmitter {
    Arc::new(LedgerEventEmitter::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_event_emission() {
        let emitter = LedgerEventEmitter::new();
        let mut receiver = emitter.subscribe();

        // Emit an event
        let hash = ContentHash::from_bytes([1u8; 32]);
        emitter.emit(LedgerEvent::TransactionCreated(TransactionCreated {
            entry_hash: hash.to_hex(),
            author: "did:icn:alice".to_string(),
            transfers: vec![Transfer {
                from: "did:icn:alice".to_string(),
                to: "did:icn:bob".to_string(),
                amount: 10,
                currency: "hours".to_string(),
            }],
            timestamp: 1234567890,
            domain_id: Some("food-coop".to_string()),
        }));

        // Receive the event
        let event = tokio::time::timeout(Duration::from_secs(1), receiver.recv())
            .await
            .expect("timeout")
            .expect("receive error");

        match event {
            LedgerEvent::TransactionCreated(tx) => {
                assert_eq!(tx.author, "did:icn:alice");
                assert_eq!(tx.transfers.len(), 1);
                assert_eq!(tx.transfers[0].amount, 10);
            }
            _ => panic!("Expected TransactionCreated event"),
        }
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let emitter = LedgerEventEmitter::new();
        let mut receiver1 = emitter.subscribe();
        let mut receiver2 = emitter.subscribe();

        assert_eq!(emitter.subscriber_count(), 2);

        // Emit an event
        let count = emitter.emit(LedgerEvent::BalanceChanged(BalanceChanged {
            account: "did:icn:alice".to_string(),
            currency: "hours".to_string(),
            old_balance: 0,
            new_balance: 10,
            change: 10,
            entry_hash: "abc123".to_string(),
            timestamp: 1234567890,
            domain_id: None,
        }));

        assert_eq!(count, 2); // Both subscribers received it

        // Both should receive
        let e1 = receiver1.recv().await.unwrap();
        let e2 = receiver2.recv().await.unwrap();

        match (e1, e2) {
            (LedgerEvent::BalanceChanged(b1), LedgerEvent::BalanceChanged(b2)) => {
                assert_eq!(b1.new_balance, 10);
                assert_eq!(b2.new_balance, 10);
            }
            _ => panic!("Expected BalanceChanged events"),
        }
    }

    #[test]
    fn test_event_serialization() {
        let event = LedgerEvent::TransactionCreated(TransactionCreated {
            entry_hash: "abc123".to_string(),
            author: "did:icn:alice".to_string(),
            transfers: vec![Transfer {
                from: "did:icn:alice".to_string(),
                to: "did:icn:bob".to_string(),
                amount: 100,
                currency: "USD".to_string(),
            }],
            timestamp: 1234567890,
            domain_id: Some("my-coop".to_string()),
        });

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("TransactionCreated"));
        assert!(json.contains("did:icn:alice"));

        // Deserialize back
        let parsed: LedgerEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            LedgerEvent::TransactionCreated(tx) => {
                assert_eq!(tx.author, "did:icn:alice");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_no_subscribers() {
        let emitter = LedgerEventEmitter::new();
        // No subscribers yet

        let count = emitter.emit(LedgerEvent::BalanceChanged(BalanceChanged {
            account: "did:icn:test".to_string(),
            currency: "test".to_string(),
            old_balance: 0,
            new_balance: 0,
            change: 0,
            entry_hash: "test".to_string(),
            timestamp: 0,
            domain_id: None,
        }));

        assert_eq!(count, 0); // No subscribers, event dropped
    }
}
