//! Compute event integration for WebSocket broadcasting
//!
//! This module provides utilities for forwarding compute events from the
//! compute actor to the gateway's EventBroadcaster for WebSocket delivery.

use crate::events::{EventBroadcaster, GatewayEvent};
use icn_compute::ComputeEvent;
use std::sync::Arc;

/// Forward a compute event to the appropriate cooperative's WebSocket subscribers
///
/// This function converts ComputeEvent types to GatewayEvent types and broadcasts
/// them to the "compute" channel. In a full daemon-gateway integration, this would
/// be called by the compute actor's event callback.
///
/// # Arguments
/// * `broadcaster` - The EventBroadcaster instance
/// * `event` - The compute event to forward
///
/// # Example Integration Pattern
/// ```rust,ignore
/// // In supervisor.rs when setting up compute actor:
/// let broadcaster = Arc::new(EventBroadcaster::new());
/// let broadcaster_clone = broadcaster.clone();
///
/// let compute_event_callback: icn_compute::EventCallback = Arc::new(move |event| {
///     let b = broadcaster_clone.clone();
///     tokio::spawn(async move {
///         forward_compute_event(&b, event).await;
///     });
/// });
/// compute_actor.set_event_callback(compute_event_callback);
/// ```
pub async fn forward_compute_event(broadcaster: &EventBroadcaster, event: ComputeEvent) {
    match event {
        ComputeEvent::TaskClaimed {
            task_hash,
            executor,
        } => {
            broadcaster
                .broadcast(
                    "compute",
                    GatewayEvent::ComputeTaskClaimed {
                        task_hash,
                        executor,
                    },
                )
                .await;
        }
        ComputeEvent::TaskCompleted {
            task_hash,
            executor,
            outcome,
            fuel_used,
            duration_ms,
        } => {
            broadcaster
                .broadcast(
                    "compute",
                    GatewayEvent::ComputeTaskCompleted {
                        task_hash,
                        executor,
                        outcome,
                        fuel_used,
                        duration_ms,
                    },
                )
                .await;
        }
        // Resource changes are internal monitoring events, not broadcast to clients
        ComputeEvent::ResourcesChanged { .. } => {}
    }
}

/// Create an event callback that forwards events to the EventBroadcaster
///
/// This is a convenience function for creating the callback closure.
///
/// # Arguments
/// * `broadcaster` - The EventBroadcaster to forward events to
///
/// # Returns
/// An EventCallback that can be passed to `ComputeActor::set_event_callback()`
pub fn create_forwarding_callback(
    broadcaster: Arc<EventBroadcaster>,
) -> icn_compute::EventCallback {
    Arc::new(move |event| {
        let b = broadcaster.clone();
        tokio::spawn(async move {
            forward_compute_event(&b, event).await;
        });
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_forward_task_claimed() {
        let broadcaster = EventBroadcaster::new();
        let mut rx = broadcaster
            .subscribe("compute")
            .await
            .expect("Should subscribe");

        let event = ComputeEvent::TaskClaimed {
            task_hash: "abc123".to_string(),
            executor: "did:icn:executor".to_string(),
        };

        forward_compute_event(&broadcaster, event).await;

        let received = rx.recv().await.expect("Should receive event");
        match received.event {
            GatewayEvent::ComputeTaskClaimed {
                task_hash,
                executor,
            } => {
                assert_eq!(task_hash, "abc123");
                assert_eq!(executor, "did:icn:executor");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[tokio::test]
    async fn test_forward_task_completed() {
        let broadcaster = EventBroadcaster::new();
        let mut rx = broadcaster
            .subscribe("compute")
            .await
            .expect("Should subscribe");

        let event = ComputeEvent::TaskCompleted {
            task_hash: "def456".to_string(),
            executor: "did:icn:executor".to_string(),
            outcome: "success".to_string(),
            fuel_used: 5000,
            duration_ms: 250,
        };

        forward_compute_event(&broadcaster, event).await;

        let received = rx.recv().await.expect("Should receive event");
        match received.event {
            GatewayEvent::ComputeTaskCompleted {
                task_hash,
                outcome,
                fuel_used,
                ..
            } => {
                assert_eq!(task_hash, "def456");
                assert_eq!(outcome, "success");
                assert_eq!(fuel_used, 5000);
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[tokio::test]
    async fn test_create_forwarding_callback() {
        let broadcaster = Arc::new(EventBroadcaster::new());
        let mut rx = broadcaster
            .subscribe("compute")
            .await
            .expect("Should subscribe");

        let callback = create_forwarding_callback(broadcaster);

        // Trigger callback
        callback(ComputeEvent::TaskClaimed {
            task_hash: "test123".to_string(),
            executor: "did:icn:test".to_string(),
        });

        // Give async task time to run
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let received = rx.recv().await.expect("Should receive event");
        match received.event {
            GatewayEvent::ComputeTaskClaimed { task_hash, .. } => {
                assert_eq!(task_hash, "test123");
            }
            _ => panic!("Wrong event type"),
        }
    }
}
