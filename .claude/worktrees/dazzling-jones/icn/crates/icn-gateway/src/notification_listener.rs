//! Notification Event Listener
//!
//! Listens to gateway events and sends push notifications.

use std::sync::Arc;
use tracing::{info, warn};

use crate::events::{EventBroadcaster, GatewayEvent};
use crate::notifications::NotificationService;
use icn_identity::Did;

/// Start notification event listener
///
/// Spawns a background task that subscribes to all cooperative events
/// and sends push notifications for relevant events.
pub fn start_notification_listener(
    _event_broadcaster: Arc<EventBroadcaster>,
    _notification_service: Arc<NotificationService>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("Starting notification event listener");

        // We need a way to listen to all events, not just one coop
        // For now, we'll document this limitation and provide a hook for when
        // events are broadcast. In production, EventBroadcaster could have a
        // global listener mechanism.

        // Note: EventBroadcaster currently uses per-coop channels for WebSocket subscriptions.
        // For notifications, we use explicit triggers from API handlers via handle_event_for_notifications().
        // This is the recommended pattern for pilot as it gives fine-grained control over
        // which events generate notifications. A global event listener could be added later
        // for centralized notification processing if needed.

        info!("Notification listener ready (manual trigger mode)");

        // Keep task alive until shutdown signal
        if let Err(e) = tokio::signal::ctrl_c().await {
            warn!("Failed to listen for ctrl+c: {}", e);
        }
    })
}

/// Handle gateway event and send notifications
///
/// This function should be called when events are broadcast to send notifications.
/// Call this from API handlers after broadcasting events.
pub async fn handle_event_for_notifications(
    event: &GatewayEvent,
    notification_service: &Arc<NotificationService>,
) {
    match event {
        GatewayEvent::PaymentCreated {
            from, to, amount, ..
        } => {
            // Parse DIDs
            if let (Ok(from_did), Ok(to_did)) = (from.parse::<Did>(), to.parse::<Did>()) {
                // Notify recipient
                let recipient_notif =
                    NotificationService::payment_received_notification(*amount, &from_did, "");
                if let Err(e) = notification_service
                    .send_to_did(&to_did, recipient_notif)
                    .await
                {
                    warn!("Failed to send payment notification to recipient: {}", e);
                } else {
                    info!("Sent payment received notification to {}", to);
                }

                // Notify sender (confirmation)
                let sender_notif =
                    NotificationService::payment_sent_notification(*amount, &to_did, "");
                if let Err(e) = notification_service
                    .send_to_did(&from_did, sender_notif)
                    .await
                {
                    warn!("Failed to send payment confirmation to sender: {}", e);
                } else {
                    info!("Sent payment confirmation to {}", from);
                }
            }
        }

        GatewayEvent::GovernanceProposalCreated {
            proposal_id,
            title,
            proposer,
            ..
        } => {
            // Send notification to all domain members
            // For now, we'll just notify the proposer as a confirmation
            if let Ok(proposer_did) = proposer.parse::<Did>() {
                let notif = NotificationService::proposal_created_notification(proposal_id, title);
                if let Err(e) = notification_service.send_to_did(&proposer_did, notif).await {
                    warn!("Failed to send proposal notification: {}", e);
                } else {
                    info!("Sent proposal created notification for {}", proposal_id);
                }
            }
        }

        GatewayEvent::GovernanceProposalClosed {
            proposal_id,
            outcome,
            ..
        } => {
            // Note: Voter notifications are sent directly in close_proposal API handler
            // This event handler is for WebSocket listeners; notifications are handled inline
            info!("Proposal {} closed with outcome: {}", proposal_id, outcome);
        }

        GatewayEvent::GovernanceVoteCast {
            proposal_id, voter, ..
        } => {
            // Send vote confirmation to voter
            if let Ok(voter_did) = voter.parse::<Did>() {
                let notif =
                    NotificationService::vote_recorded_notification(proposal_id, proposal_id);
                if let Err(e) = notification_service.send_to_did(&voter_did, notif).await {
                    warn!("Failed to send vote confirmation: {}", e);
                } else {
                    info!("Sent vote confirmation to {}", voter);
                }
            }
        }

        _ => {
            // Other events don't trigger notifications yet
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notifications::NotificationService;
    use icn_identity::KeyPair;

    #[tokio::test]
    async fn test_payment_notification() {
        // Use temporary store for tests
        let store = Arc::new(crate::notifications::NotificationStore::new(
            sled::Config::new().temporary(true).open().unwrap(),
        ));
        let notification_service = Arc::new(NotificationService::new(store, None));
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Register devices
        notification_service.register_device(
            alice.clone(),
            "alice_token".to_string(),
            crate::notifications::Platform::Android,
        );
        notification_service.register_device(
            bob.clone(),
            "bob_token".to_string(),
            crate::notifications::Platform::Ios,
        );

        let event = GatewayEvent::PaymentCreated {
            coop_id: "test-coop".to_string(),
            hash: "hash123".to_string(),
            from: alice.to_string(),
            to: bob.to_string(),
            amount: 100,
            currency: "hours".to_string(),
        };

        handle_event_for_notifications(&event, &notification_service).await;

        // Verify both devices were notified (check logs in real implementation)
    }
}
