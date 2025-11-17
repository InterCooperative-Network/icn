//! System-wide event bus for inter-actor communication
//!
//! This module provides a simple event bus for coordinating actions across actors.
//! Key use case: Governance proposals triggering ledger transactions or contract execution.

use icn_governance::{ProposalId, ProposalPayload};
use icn_identity::Did;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// System-wide events that actors can emit and subscribe to
#[derive(Clone, Debug)]
pub enum SystemEvent {
    /// A governance proposal was accepted
    ProposalAccepted {
        proposal_id: ProposalId,
        domain_id: String,
        payload: ProposalPayload,
        decided_at: u64,
    },

    /// A governance proposal was rejected or failed to reach quorum
    ProposalRejected {
        proposal_id: ProposalId,
        domain_id: String,
        decided_at: u64,
    },

    /// A ledger transaction was executed (for contracts listening to ledger changes)
    TransactionExecuted {
        entry_hash: [u8; 32],
        from: Did,
        to: Did,
        amount: i64,
        currency: String,
    },

    /// A contract was executed
    ContractExecuted {
        contract_id: String,
        outcome: serde_json::Value,
    },
}

/// Callback function for event subscribers
pub type EventCallback = Arc<dyn Fn(SystemEvent) + Send + Sync>;

/// Subscription handle that automatically unsubscribes when dropped
///
/// When this handle is dropped, the associated callback is removed from the event bus.
/// This prevents memory leaks when subscriptions are added dynamically.
pub struct SubscriptionHandle {
    id: usize,
    subscribers: Arc<RwLock<Vec<(usize, EventCallback)>>>,
}

impl Drop for SubscriptionHandle {
    fn drop(&mut self) {
        // Attempt to remove this subscription
        // Use try_write() to avoid panicking if the lock is held
        // (this can happen during async runtime shutdown)
        if let Ok(mut subs) = self.subscribers.try_write() {
            let initial_count = subs.len();
            subs.retain(|(id, _)| *id != self.id);
            let removed_count = initial_count - subs.len();
            if removed_count > 0 {
                debug!("Event bus subscriber {} removed (remaining: {})", self.id, subs.len());
            }
        }
        // If we can't get the lock, silently skip cleanup
        // The subscription will remain in memory, but this is better than panicking
    }
}

/// Simple event bus for publishing and subscribing to system events
///
/// This uses a synchronous broadcast pattern where all subscribers are called
/// immediately when an event is emitted. For high-throughput scenarios, consider
/// using an async channel-based approach.
///
/// Subscriptions are tracked via SubscriptionHandle, which automatically unsubscribes
/// when dropped, preventing memory leaks.
#[derive(Clone)]
pub struct EventBus {
    subscribers: Arc<RwLock<Vec<(usize, EventCallback)>>>,
    next_id: Arc<std::sync::atomic::AtomicUsize>,
}

impl EventBus {
    /// Create a new event bus
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(RwLock::new(Vec::new())),
            next_id: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Subscribe to all events with a callback
    ///
    /// The callback will be invoked synchronously for every event.
    /// If the callback needs to perform async work, it should spawn a task.
    ///
    /// Returns a SubscriptionHandle that will automatically unsubscribe when dropped.
    /// This prevents memory leaks if subscriptions are dynamically added and removed.
    pub async fn subscribe(&self, callback: EventCallback) -> SubscriptionHandle {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut subs = self.subscribers.write().await;
        subs.push((id, callback));
        debug!("Event bus subscriber {} added (total: {})", id, subs.len());

        SubscriptionHandle {
            id,
            subscribers: self.subscribers.clone(),
        }
    }

    /// Emit an event to all subscribers
    ///
    /// This is a synchronous operation - all subscribers are called immediately.
    /// Subscribers should spawn tasks for any async work.
    pub async fn emit(&self, event: SystemEvent) {
        let subs = self.subscribers.read().await;
        let event_type = match &event {
            SystemEvent::ProposalAccepted { .. } => "ProposalAccepted",
            SystemEvent::ProposalRejected { .. } => "ProposalRejected",
            SystemEvent::TransactionExecuted { .. } => "TransactionExecuted",
            SystemEvent::ContractExecuted { .. } => "ContractExecuted",
        };

        debug!("Emitting event: {} to {} subscribers", event_type, subs.len());

        for (id, callback) in subs.iter() {
            // Call subscriber, but don't let one subscriber's panic crash everything
            if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                callback(event.clone());
            })) {
                warn!("Event subscriber {} panicked: {:?}", id, e);
            }
        }
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_event_bus_subscribe_and_emit() {
        let bus = EventBus::new();
        let counter = Arc::new(AtomicUsize::new(0));

        // Subscribe to events
        let counter_clone = counter.clone();
        let _handle = bus.subscribe(Arc::new(move |_event| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        }))
        .await;

        // Emit event
        let event = SystemEvent::ProposalAccepted {
            proposal_id: ProposalId("test-proposal".to_string()),
            domain_id: "test-domain".to_string(),
            payload: ProposalPayload::Text {
                body: "Test proposal".to_string(),
            },
            decided_at: 1234567890,
        };

        bus.emit(event).await;

        // Verify callback was called
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_event_bus_multiple_subscribers() {
        let bus = EventBus::new();
        let counter1 = Arc::new(AtomicUsize::new(0));
        let counter2 = Arc::new(AtomicUsize::new(0));

        // Subscribe two callbacks
        let c1 = counter1.clone();
        let _handle1 = bus.subscribe(Arc::new(move |_| {
            c1.fetch_add(1, Ordering::SeqCst);
        }))
        .await;

        let c2 = counter2.clone();
        let _handle2 = bus.subscribe(Arc::new(move |_| {
            c2.fetch_add(10, Ordering::SeqCst);
        }))
        .await;

        // Emit event
        let event = SystemEvent::ProposalRejected {
            proposal_id: ProposalId("rejected".to_string()),
            domain_id: "domain".to_string(),
            decided_at: 9999,
        };

        bus.emit(event).await;

        // Both callbacks should have been called
        assert_eq!(counter1.load(Ordering::SeqCst), 1);
        assert_eq!(counter2.load(Ordering::SeqCst), 10);
    }

    #[tokio::test]
    async fn test_event_bus_clone() {
        let bus1 = EventBus::new();
        let bus2 = bus1.clone();

        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();

        // Subscribe via bus1
        let _handle = bus1.subscribe(Arc::new(move |_| {
            c.fetch_add(1, Ordering::SeqCst);
        }))
        .await;

        // Emit via bus2
        let event = SystemEvent::ContractExecuted {
            contract_id: "contract123".to_string(),
            outcome: serde_json::json!({"status": "success"}),
        };

        bus2.emit(event).await;

        // Subscriber should receive event (same underlying bus)
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_event_bus_unsubscribe_on_drop() {
        let bus = EventBus::new();
        let counter = Arc::new(AtomicUsize::new(0));

        // Subscribe and immediately drop the handle
        {
            let c = counter.clone();
            let _handle = bus.subscribe(Arc::new(move |_| {
                c.fetch_add(1, Ordering::SeqCst);
            }))
            .await;
            // Handle dropped here
        }

        // Emit event
        let event = SystemEvent::ProposalAccepted {
            proposal_id: ProposalId("test".to_string()),
            domain_id: "test".to_string(),
            payload: ProposalPayload::Text { body: "test".to_string() },
            decided_at: 1234567890,
        };

        bus.emit(event).await;

        // Callback should NOT have been called (subscription was dropped)
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }
}
