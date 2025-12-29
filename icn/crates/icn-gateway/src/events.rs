//! Event broadcasting for real-time updates
//!
//! This module provides WebSocket event broadcasting with:
//! - Bounded channels to prevent memory exhaustion from slow clients
//! - Automatic slow client detection and disconnection
//! - Sequence numbers for event ordering
//! - Backfill support for reconnection

use icn_obs::metrics::gateway as metrics;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::RwLock;
use tracing::warn;

use crate::coop::CoopId;

/// Configuration for WebSocket connections and event broadcasting
///
/// All fields have sensible defaults via `Default::default()`.
#[derive(Debug, Clone)]
pub struct WebSocketConfig {
    /// Maximum global WebSocket connections (default: 10,000)
    pub max_connections_global: u64,
    /// Maximum subscribers per cooperative (default: 1,000)
    pub max_subscribers_per_coop: usize,
    /// Maximum events kept for backfill per channel (default: 100)
    pub max_backfill_events: usize,
    /// Channel capacity per subscriber - events buffer (default: 5,000)
    pub channel_capacity: usize,
    /// Heartbeat ping interval in seconds (default: 30)
    pub heartbeat_interval_secs: u64,
    /// Heartbeat timeout in seconds (default: 60)
    pub heartbeat_timeout_secs: u64,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            max_connections_global: 10_000,
            max_subscribers_per_coop: 1_000,
            max_backfill_events: 100,
            channel_capacity: 5_000,
            heartbeat_interval_secs: 30,
            heartbeat_timeout_secs: 60,
        }
    }
}

impl WebSocketConfig {
    /// Create a config with custom values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set channel capacity (for testing with small values)
    pub fn with_channel_capacity(mut self, capacity: usize) -> Self {
        self.channel_capacity = capacity;
        self
    }

    /// Set max backfill events
    pub fn with_max_backfill(mut self, max_events: usize) -> Self {
        self.max_backfill_events = max_events;
        self
    }

    /// Set max subscribers per cooperative
    pub fn with_max_subscribers(mut self, max_subs: usize) -> Self {
        self.max_subscribers_per_coop = max_subs;
        self
    }
}

/// Global sequence counter for events
static EVENT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Counter for slow client disconnections (for metrics)
static SLOW_CLIENT_DISCONNECTS: AtomicU64 = AtomicU64::new(0);

/// Event types that can be broadcast to WebSocket clients
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum GatewayEvent {
    /// A new payment was created
    PaymentCreated {
        coop_id: String,
        hash: String,
        from: String,
        to: String,
        amount: i64,
        currency: String,
    },
    /// A member was added to a cooperative
    MemberAdded {
        coop_id: String,
        did: String,
        role: String,
    },
    /// A member was removed from a cooperative
    MemberRemoved { coop_id: String, did: String },
    /// A member's role was updated
    RoleUpdated {
        coop_id: String,
        did: String,
        new_role: String,
    },
    /// Cooperative settings were updated
    SettingsUpdated { coop_id: String },
    /// A governance domain was created
    GovernanceDomainCreated {
        domain_id: String,
        name: String,
        creator: String,
    },
    /// A governance proposal was created
    GovernanceProposalCreated {
        proposal_id: String,
        domain_id: String,
        proposer: String,
        title: String,
        payload_type: String, // "text", "budget", "membership", "config_change"
    },
    /// A governance proposal was opened for voting
    GovernanceProposalOpened {
        proposal_id: String,
        domain_id: String,
        closes_at: u64, // Unix timestamp
    },
    /// A governance proposal was closed
    GovernanceProposalClosed {
        proposal_id: String,
        domain_id: String,
        outcome: String, // "accepted", "rejected", "no_quorum"
    },
    /// A vote was cast on a proposal
    GovernanceVoteCast {
        proposal_id: String,
        domain_id: String,
        voter: String,
        choice: String, // "for", "against", "abstain"
    },
    /// A compute task was submitted
    ComputeTaskSubmitted {
        task_id: String,
        task_hash: String,
        submitter: String,
        fuel_limit: u64,
    },
    /// A compute task was claimed by an executor
    ComputeTaskClaimed { task_hash: String, executor: String },
    /// A compute task completed
    ComputeTaskCompleted {
        task_hash: String,
        executor: String,
        outcome: String, // "success", "failed", "out_of_fuel", "timeout"
        fuel_used: u64,
        duration_ms: u64,
    },
    /// A compute task was cancelled
    ComputeTaskCancelled {
        task_hash: String,
        submitter: String,
        reason: String,
    },
    /// A trust attestation was created
    TrustAttested {
        from: String,
        to: String,
        score: f64,
        memo: Option<String>,
    },
    /// A trust attestation was revoked
    TrustRevoked { from: String, to: String },
    /// A transaction was completed
    TransactionCompleted {
        coop_id: String,
        hash: String,
        from: String,
        to: String,
        amount: i64,
        currency: String,
    },

    // === Ledger Events (real-time balance tracking) ===
    /// An account balance changed
    BalanceChanged {
        coop_id: String,
        account: String,
        currency: String,
        old_balance: i64,
        new_balance: i64,
        change: i64,
        entry_hash: String,
    },

    /// Multiple balance changes in a batch (from one transaction)
    BatchBalanceChanged {
        coop_id: String,
        entry_hash: String,
        changes: Vec<BalanceChangeDetail>,
    },

    /// A member was frozen
    LedgerMemberFrozen {
        coop_id: String,
        member: String,
        reason: String,
        frozen_by: Option<String>,
        proposal_id: Option<String>,
        duration_seconds: Option<u64>,
    },

    /// A member was unfrozen
    LedgerMemberUnfrozen {
        coop_id: String,
        member: String,
        reason: String,
        unfrozen_by: Option<String>,
        proposal_id: Option<String>,
    },

    /// A fork was detected in the ledger
    LedgerForkDetected {
        coop_id: String,
        parent_hash: String,
        conflicting_entries: Vec<String>,
    },

    /// A fork was resolved in the ledger
    LedgerForkResolved {
        coop_id: String,
        parent_hash: String,
        kept_entry: String,
        quarantined_entries: Vec<String>,
        resolution_strategy: String,
    },

    /// A ledger rollback was performed
    LedgerRollback {
        coop_id: String,
        target_hash: String,
        archived_count: usize,
        reason: String,
    },
}

/// Detail of a single balance change (used in BatchBalanceChanged)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceChangeDetail {
    pub account: String,
    pub currency: String,
    pub old_balance: i64,
    pub new_balance: i64,
    pub change: i64,
}

/// Event with sequence number for backfill support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequencedEvent {
    /// Sequence number for ordering and backfill
    pub seq: u64,
    /// The event payload
    #[serde(flatten)]
    pub event: GatewayEvent,
}

/// A subscriber with its bounded channel and tracking info
struct Subscriber {
    /// Bounded channel sender for events
    tx: tokio::sync::mpsc::Sender<SequencedEvent>,
    /// Unique identifier for this subscriber (for logging)
    id: u64,
}

impl Subscriber {
    fn new(tx: tokio::sync::mpsc::Sender<SequencedEvent>, id: u64) -> Self {
        Self { tx, id }
    }

    /// Check if the channel is closed
    fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }

    /// Try to send an event, returning whether it succeeded
    #[allow(clippy::result_large_err)] // TrySendError contains the event for retry
    fn try_send(&self, event: SequencedEvent) -> Result<(), TrySendError<SequencedEvent>> {
        self.tx.try_send(event)
    }
}

/// Global counter for subscriber IDs (for debugging/logging)
static SUBSCRIBER_ID: AtomicU64 = AtomicU64::new(1);

/// Event broadcaster manages subscribers and sends events
///
/// Uses bounded channels to prevent memory exhaustion from slow clients.
/// Slow clients that can't consume events fast enough will be automatically
/// disconnected when their channel buffer fills up.
pub struct EventBroadcaster {
    /// Subscribers per cooperative (coop_id -> list of subscribers with bounded channels)
    subscribers: Arc<RwLock<HashMap<CoopId, Vec<Subscriber>>>>,
    /// Recent events for backfill (coop_id -> ring buffer of recent events)
    backfill_buffer: Arc<RwLock<HashMap<CoopId, VecDeque<SequencedEvent>>>>,
    /// Configuration for WebSocket connections
    config: WebSocketConfig,
}

impl EventBroadcaster {
    /// Create a new event broadcaster with default configuration
    pub fn new() -> Self {
        Self::with_config(WebSocketConfig::default())
    }

    /// Create a new event broadcaster with custom channel capacity (convenience method)
    pub fn with_capacity(channel_capacity: usize) -> Self {
        Self::with_config(WebSocketConfig::default().with_channel_capacity(channel_capacity))
    }

    /// Create a new event broadcaster with custom configuration
    pub fn with_config(config: WebSocketConfig) -> Self {
        Self {
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            backfill_buffer: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Get the current configuration
    pub fn config(&self) -> &WebSocketConfig {
        &self.config
    }

    /// Subscribe to events for a cooperative
    ///
    /// Returns a bounded receiver channel. If the client can't consume events
    /// fast enough and the channel fills up, the client will be automatically
    /// disconnected on the next broadcast.
    ///
    /// Returns None if the subscriber limit has been reached for this cooperative.
    pub async fn subscribe(
        &self,
        coop_id: &str,
    ) -> Option<tokio::sync::mpsc::Receiver<SequencedEvent>> {
        let (tx, rx) = tokio::sync::mpsc::channel(self.config.channel_capacity);
        let subscriber_id = SUBSCRIBER_ID.fetch_add(1, Ordering::Relaxed);
        let subscriber = Subscriber::new(tx, subscriber_id);

        let mut subscribers = self.subscribers.write().await;
        let subs = subscribers
            .entry(coop_id.to_string())
            .or_insert_with(Vec::new);

        // Check subscriber limit to prevent DoS via unlimited WebSocket connections
        if subs.len() >= self.config.max_subscribers_per_coop {
            metrics::websocket_subscriber_limit_rejections_inc(coop_id);
            return None;
        }

        subs.push(subscriber);

        // Update subscriber count metric
        metrics::websocket_subscribers_per_coop_set(coop_id, subs.len() as u64);

        Some(rx)
    }

    /// Get the number of slow client disconnections (for metrics)
    pub fn slow_client_disconnects() -> u64 {
        SLOW_CLIENT_DISCONNECTS.load(Ordering::Relaxed)
    }

    /// Get events after a given sequence number for backfill
    pub async fn get_backfill(&self, coop_id: &str, after_seq: u64) -> Vec<SequencedEvent> {
        metrics::websocket_backfill_requests_inc(coop_id);
        let buffer = self.backfill_buffer.read().await;
        if let Some(events) = buffer.get(coop_id) {
            events
                .iter()
                .filter(|e| e.seq > after_seq)
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get the current sequence number (for initial connection)
    pub fn current_sequence(&self) -> u64 {
        EVENT_SEQUENCE.load(Ordering::Relaxed)
    }

    /// Broadcast an event to all subscribers of a cooperative
    ///
    /// Uses bounded channels with backpressure handling:
    /// - If a client's channel is full (slow client), the client is marked for removal
    /// - Slow clients are logged and counted in metrics
    /// - This prevents memory exhaustion from slow consumers
    pub async fn broadcast(&self, coop_id: &str, event: GatewayEvent) {
        // Record broadcast metric
        metrics::websocket_events_broadcast_inc(coop_id);

        // Assign sequence number
        let seq = EVENT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let sequenced = SequencedEvent { seq, event };

        // Store in backfill buffer
        {
            let mut buffer = self.backfill_buffer.write().await;
            let events = buffer
                .entry(coop_id.to_string())
                .or_insert_with(VecDeque::new);
            events.push_back(sequenced.clone());
            // Trim to max size
            while events.len() > self.config.max_backfill_events {
                events.pop_front();
            }
        }

        // Track which subscribers need removal (slow clients or closed channels)
        let mut needs_cleanup = false;
        let mut slow_client_ids = Vec::new();

        // First try read-only send with try_send for backpressure handling
        {
            let subscribers = self.subscribers.read().await;
            if let Some(subs) = subscribers.get(coop_id) {
                for sub in subs {
                    match sub.try_send(sequenced.clone()) {
                        Ok(()) => {
                            // Successfully sent
                        }
                        Err(TrySendError::Full(_)) => {
                            // Channel is full - slow client detected
                            warn!(
                                subscriber_id = sub.id,
                                coop_id = %coop_id,
                                seq = seq,
                                "Slow client detected: channel full, will disconnect"
                            );
                            slow_client_ids.push(sub.id);
                            SLOW_CLIENT_DISCONNECTS.fetch_add(1, Ordering::Relaxed);
                            metrics::websocket_slow_client_disconnects_inc();
                            needs_cleanup = true;
                        }
                        Err(TrySendError::Closed(_)) => {
                            // Channel closed normally
                            needs_cleanup = true;
                        }
                    }
                }

                // If no channels need cleanup, we're done
                if !needs_cleanup {
                    return;
                }
            } else {
                return; // No subscribers
            }
        }

        // If we detected issues, acquire write lock and clean up
        let mut subscribers = self.subscribers.write().await;
        if let Some(subs) = subscribers.get_mut(coop_id) {
            // Remove closed channels and slow clients
            subs.retain(|sub| {
                if sub.is_closed() {
                    return false;
                }
                if slow_client_ids.contains(&sub.id) {
                    return false;
                }
                true
            });

            // Update subscriber count metric after cleanup
            metrics::websocket_subscribers_per_coop_set(coop_id, subs.len() as u64);

            // Remove empty subscriber lists
            if subs.is_empty() {
                subscribers.remove(coop_id);
            }
        }
    }

    /// Clean up closed subscriber channels
    pub async fn cleanup(&self, coop_id: &str) {
        let mut subscribers = self.subscribers.write().await;

        if let Some(subs) = subscribers.get_mut(coop_id) {
            subs.retain(|sub| !sub.is_closed());

            // Update subscriber count metric after cleanup
            metrics::websocket_subscribers_per_coop_set(coop_id, subs.len() as u64);

            // Remove empty subscriber lists
            if subs.is_empty() {
                subscribers.remove(coop_id);
            }
        }
    }
}

impl Default for EventBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_subscribe_and_broadcast() {
        let broadcaster = EventBroadcaster::new();

        let mut rx = broadcaster
            .subscribe("test-coop")
            .await
            .expect("Should subscribe successfully");

        let event = GatewayEvent::PaymentCreated {
            coop_id: "test-coop".to_string(),
            hash: "abc123".to_string(),
            from: "did:icn:alice".to_string(),
            to: "did:icn:bob".to_string(),
            amount: 10,
            currency: "hours".to_string(),
        };

        broadcaster.broadcast("test-coop", event.clone()).await;

        let received = rx.recv().await;
        assert!(received.is_some());

        let sequenced = received.unwrap();
        assert!(sequenced.seq > 0);
        match sequenced.event {
            GatewayEvent::PaymentCreated {
                coop_id, amount, ..
            } => {
                assert_eq!(coop_id, "test-coop");
                assert_eq!(amount, 10);
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[tokio::test]
    async fn test_backfill() {
        let broadcaster = EventBroadcaster::new();

        // Broadcast 3 events
        for i in 0..3 {
            let event = GatewayEvent::PaymentCreated {
                coop_id: "test-coop".to_string(),
                hash: format!("hash{i}"),
                from: "did:icn:alice".to_string(),
                to: "did:icn:bob".to_string(),
                amount: i as i64,
                currency: "hours".to_string(),
            };
            broadcaster.broadcast("test-coop", event).await;
        }

        // Get all events (after seq 0)
        let backfill = broadcaster.get_backfill("test-coop", 0).await;
        assert_eq!(backfill.len(), 3);

        // Get events after first
        let first_seq = backfill[0].seq;
        let partial = broadcaster.get_backfill("test-coop", first_seq).await;
        assert_eq!(partial.len(), 2);
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let broadcaster = EventBroadcaster::new();

        let mut rx1 = broadcaster
            .subscribe("test-coop")
            .await
            .expect("Should subscribe successfully");
        let mut rx2 = broadcaster
            .subscribe("test-coop")
            .await
            .expect("Should subscribe successfully");

        let event = GatewayEvent::MemberAdded {
            coop_id: "test-coop".to_string(),
            did: "did:icn:alice".to_string(),
            role: "Member".to_string(),
        };

        broadcaster.broadcast("test-coop", event).await;

        assert!(rx1.recv().await.is_some());
        assert!(rx2.recv().await.is_some());
    }

    #[tokio::test]
    async fn test_cleanup_closed_channels() {
        let broadcaster = EventBroadcaster::new();

        let rx = broadcaster
            .subscribe("test-coop")
            .await
            .expect("Should subscribe successfully");
        drop(rx); // Close the channel

        broadcaster.cleanup("test-coop").await;

        let subscribers = broadcaster.subscribers.read().await;
        assert!(!subscribers.contains_key("test-coop"));
    }

    #[tokio::test]
    async fn test_slow_client_detection() {
        // Create a broadcaster with small channel capacity for testing
        let broadcaster = EventBroadcaster::with_capacity(3);

        // Subscribe but don't consume events
        let _rx = broadcaster
            .subscribe("test-coop")
            .await
            .expect("Should subscribe successfully");

        // Verify subscriber count before
        {
            let subscribers = broadcaster.subscribers.read().await;
            assert_eq!(subscribers.get("test-coop").map(|s| s.len()), Some(1));
        }

        // Broadcast more events than channel capacity (slow client scenario)
        for i in 0..5 {
            let event = GatewayEvent::PaymentCreated {
                coop_id: "test-coop".to_string(),
                hash: format!("hash{i}"),
                from: "did:icn:alice".to_string(),
                to: "did:icn:bob".to_string(),
                amount: i as i64,
                currency: "hours".to_string(),
            };
            broadcaster.broadcast("test-coop", event).await;
        }

        // After channel fills up, slow client should be disconnected
        // The subscriber should have been removed due to channel full
        let subscribers = broadcaster.subscribers.read().await;
        // Slow client was removed because channel filled up
        assert!(
            !subscribers.contains_key("test-coop")
                || subscribers
                    .get("test-coop")
                    .map(|s| s.is_empty())
                    .unwrap_or(true)
        );

        // Verify slow client disconnect counter incremented
        let disconnects = EventBroadcaster::slow_client_disconnects();
        assert!(
            disconnects > 0,
            "Should have recorded slow client disconnect"
        );
    }

    #[tokio::test]
    async fn test_subscriber_limit() {
        // Use small limit for testing
        let config = WebSocketConfig::default().with_max_subscribers(5);
        let broadcaster = EventBroadcaster::with_config(config);
        let mut receivers = Vec::new();

        // Subscribe up to the limit
        for _ in 0..5 {
            let rx = broadcaster.subscribe("test-coop").await;
            assert!(rx.is_some(), "Should allow subscription up to limit");
            receivers.push(rx);
        }

        // Next subscription should be rejected
        let rx_over_limit = broadcaster.subscribe("test-coop").await;
        assert!(
            rx_over_limit.is_none(),
            "Should reject subscription over limit"
        );

        // Verify config was applied correctly
        assert_eq!(broadcaster.config().max_subscribers_per_coop, 5);
    }
}
