//! Event broadcasting for real-time updates

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::coop::CoopId;

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
    MemberRemoved {
        coop_id: String,
        did: String,
    },
    /// A member's role was updated
    RoleUpdated {
        coop_id: String,
        did: String,
        new_role: String,
    },
    /// Cooperative settings were updated
    SettingsUpdated {
        coop_id: String,
    },
}

/// Event broadcaster manages subscribers and sends events
pub struct EventBroadcaster {
    /// Subscribers per cooperative (coop_id -> list of subscriber channels)
    subscribers: Arc<RwLock<HashMap<CoopId, Vec<tokio::sync::mpsc::UnboundedSender<GatewayEvent>>>>>,
}

impl EventBroadcaster {
    /// Create a new event broadcaster
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Subscribe to events for a cooperative
    pub async fn subscribe(&self, coop_id: &str) -> tokio::sync::mpsc::UnboundedReceiver<GatewayEvent> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        let mut subscribers = self.subscribers.write().await;
        subscribers
            .entry(coop_id.to_string())
            .or_insert_with(Vec::new)
            .push(tx);

        rx
    }

    /// Broadcast an event to all subscribers of a cooperative
    pub async fn broadcast(&self, coop_id: &str, event: GatewayEvent) {
        // First try read-only send
        {
            let subscribers = self.subscribers.read().await;
            if let Some(subs) = subscribers.get(coop_id) {
                let mut any_closed = false;
                for sub in subs {
                    if sub.send(event.clone()).is_err() {
                        any_closed = true;
                    }
                }

                // If no channels were closed, we're done
                if !any_closed {
                    return;
                }
            } else {
                return; // No subscribers
            }
        }

        // If we detected closed channels, acquire write lock and clean up
        let mut subscribers = self.subscribers.write().await;
        if let Some(subs) = subscribers.get_mut(coop_id) {
            subs.retain(|sub| !sub.is_closed());

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

        let mut rx = broadcaster.subscribe("test-coop").await;

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

        match received.unwrap() {
            GatewayEvent::PaymentCreated { coop_id, amount, .. } => {
                assert_eq!(coop_id, "test-coop");
                assert_eq!(amount, 10);
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let broadcaster = EventBroadcaster::new();

        let mut rx1 = broadcaster.subscribe("test-coop").await;
        let mut rx2 = broadcaster.subscribe("test-coop").await;

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

        let rx = broadcaster.subscribe("test-coop").await;
        drop(rx); // Close the channel

        broadcaster.cleanup("test-coop").await;

        let subscribers = broadcaster.subscribers.read().await;
        assert!(!subscribers.contains_key("test-coop"));
    }
}
