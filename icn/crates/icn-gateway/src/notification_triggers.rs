//! Notification Triggers
//!
//! Listens to various events and queues appropriate notifications.
//! This bridges the event system with the notification queue.

use std::sync::Arc;

use icn_ledger::{LedgerEvent, SharedEventEmitter};
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use crate::notification_queue::{
    balance_change_notification, payment_received_notification, payment_sent_notification,
    security_alert_notification, NotificationQueue, NotificationType, QueuedNotification,
};

/// Triggers notifications based on ledger events
pub struct LedgerNotificationTrigger {
    /// The shared event emitter from the ledger
    ledger_emitter: SharedEventEmitter,
    /// The notification queue
    queue: Arc<NotificationQueue>,
    /// Default coop ID
    default_coop_id: String,
}

impl LedgerNotificationTrigger {
    /// Create a new ledger notification trigger
    pub fn new(
        ledger_emitter: SharedEventEmitter,
        queue: Arc<NotificationQueue>,
        default_coop_id: String,
    ) -> Self {
        Self {
            ledger_emitter,
            queue,
            default_coop_id,
        }
    }

    /// Start the trigger, listening for ledger events
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut receiver = self.ledger_emitter.subscribe();
            info!("Ledger notification trigger started");

            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        if let Err(e) = self.handle_event(event).await {
                            warn!("Failed to queue notification: {}", e);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(count)) => {
                        warn!(
                            "Ledger notification trigger lagged, missed {} events",
                            count
                        );
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("Ledger event emitter closed, trigger shutting down");
                        break;
                    }
                }
            }
        })
    }

    /// Handle a single ledger event
    async fn handle_event(&self, event: LedgerEvent) -> Result<(), String> {
        match event {
            LedgerEvent::TransactionCreated(tx) => {
                let coop_id = tx.domain_id.unwrap_or_else(|| self.default_coop_id.clone());

                // Create notifications for each transfer
                for transfer in tx.transfers {
                    // Notify recipient
                    let notif = payment_received_notification(
                        &transfer.to,
                        &coop_id,
                        transfer.amount,
                        &transfer.currency,
                        &transfer.from,
                        &tx.entry_hash,
                    );
                    self.queue.queue(notif).await?;

                    // Notify sender
                    let notif = payment_sent_notification(
                        &transfer.from,
                        &coop_id,
                        transfer.amount,
                        &transfer.currency,
                        &transfer.to,
                        &tx.entry_hash,
                    );
                    self.queue.queue(notif).await?;
                }

                debug!(
                    entry_hash = %tx.entry_hash,
                    "Queued payment notifications"
                );
            }

            LedgerEvent::BalanceChanged(bc) => {
                let coop_id = bc.domain_id.unwrap_or_else(|| self.default_coop_id.clone());

                // Only notify on significant balance changes (not tiny amounts)
                if bc.change.abs() >= 1 {
                    let notif = balance_change_notification(
                        &bc.account,
                        &coop_id,
                        &bc.currency,
                        bc.old_balance,
                        bc.new_balance,
                        bc.change,
                    );
                    self.queue.queue(notif).await?;
                    debug!(account = %bc.account, "Queued balance change notification");
                }
            }

            LedgerEvent::MemberFrozen(mf) => {
                let coop_id = self.default_coop_id.clone();

                // Notify the frozen member
                let notif = security_alert_notification(
                    &mf.member,
                    &coop_id,
                    "account_frozen",
                    &format!("Your account has been frozen. Reason: {}", &mf.reason),
                );
                self.queue.queue(notif).await?;
                debug!(member = %mf.member, "Queued account frozen notification");
            }

            LedgerEvent::MemberUnfrozen(mu) => {
                let coop_id = self.default_coop_id.clone();

                // Notify the unfrozen member
                let notif = QueuedNotification::new(
                    &mu.member,
                    &coop_id,
                    "Account Restored",
                    format!("Your account has been unfrozen. Reason: {}", &mu.reason),
                    NotificationType::SecurityAlert,
                );
                self.queue.queue(notif).await?;
                debug!(member = %mu.member, "Queued account unfrozen notification");
            }

            LedgerEvent::ForkDetected(fd) => {
                // Fork detection is a system-wide alert
                // In a real system, this would notify administrators
                warn!(
                    parent_hash = %fd.parent_hash,
                    conflicts = fd.conflicting_entries.len(),
                    "Fork detected - would notify administrators"
                );
            }

            LedgerEvent::CrossCurrencyTransfer(cct) => {
                let coop_id = cct
                    .domain_id
                    .unwrap_or_else(|| self.default_coop_id.clone());

                // Notify recipient of cross-currency payment
                let notif = QueuedNotification::new(
                    &cct.to,
                    &coop_id,
                    "Cross-Currency Payment Received",
                    format!(
                        "You received {} {} (converted from {} {}) from {}",
                        cct.net_target_amount,
                        &cct.to_currency,
                        cct.source_amount,
                        &cct.from_currency,
                        &cct.from
                    ),
                    NotificationType::PaymentReceived,
                );
                self.queue.queue(notif).await?;

                // Notify sender
                let notif = QueuedNotification::new(
                    &cct.from,
                    &coop_id,
                    "Cross-Currency Payment Sent",
                    format!(
                        "You sent {} {} (converted to {} {}) to {}",
                        cct.source_amount,
                        &cct.from_currency,
                        cct.net_target_amount,
                        &cct.to_currency,
                        &cct.to
                    ),
                    NotificationType::PaymentSent,
                );
                self.queue.queue(notif).await?;

                debug!(
                    entry_hash = %cct.entry_hash,
                    "Queued cross-currency payment notifications"
                );
            }

            // Skip other events for notification purposes
            LedgerEvent::TransactionConfirmed(_) => {}
            LedgerEvent::BatchBalanceChanged(_) => {}
            LedgerEvent::ForkResolved(_) => {}
            LedgerEvent::RollbackPerformed(_) => {}
        }

        Ok(())
    }
}

/// Triggers notifications based on governance events
pub struct GovernanceNotificationTrigger {
    /// The notification queue
    queue: Arc<NotificationQueue>,
}

impl GovernanceNotificationTrigger {
    /// Create a new governance notification trigger
    pub fn new(queue: Arc<NotificationQueue>) -> Self {
        Self { queue }
    }

    /// Queue notification for proposal created
    pub async fn on_proposal_created(
        &self,
        recipient: &str,
        coop_id: &str,
        proposal_id: &str,
        title: &str,
        proposer: &str,
    ) -> Result<String, String> {
        let notif = crate::notification_queue::proposal_created_notification(
            recipient,
            coop_id,
            proposal_id,
            title,
            proposer,
        );
        self.queue.queue(notif).await
    }

    /// Queue notification for amendment proposed
    pub async fn on_amendment_proposed(
        &self,
        recipient: &str,
        coop_id: &str,
        amendment_id: &str,
        title: &str,
        proposer: &str,
    ) -> Result<String, String> {
        let notif = QueuedNotification::new(
            recipient,
            coop_id,
            "Amendment Proposed",
            format!("{} proposed by {}", title, truncate_did(proposer)),
            NotificationType::AmendmentProposed,
        )
        .with_data(serde_json::json!({
            "type": "amendment_proposed",
            "amendment_id": amendment_id,
            "title": title,
            "proposer": proposer,
        }));
        self.queue.queue(notif).await
    }

    /// Queue notification for appeal filed
    pub async fn on_appeal_filed(
        &self,
        recipient: &str,
        coop_id: &str,
        appeal_id: &str,
        subject: &str,
        appellant: &str,
    ) -> Result<String, String> {
        let notif = QueuedNotification::new(
            recipient,
            coop_id,
            "Appeal Filed",
            format!(
                "Appeal regarding {} filed by {}",
                subject,
                truncate_did(appellant)
            ),
            NotificationType::AppealFiled,
        )
        .with_data(serde_json::json!({
            "type": "appeal_filed",
            "appeal_id": appeal_id,
            "subject": subject,
            "appellant": appellant,
        }));
        self.queue.queue(notif).await
    }

    /// Queue notification for appeal decision
    pub async fn on_appeal_decided(
        &self,
        appellant: &str,
        coop_id: &str,
        appeal_id: &str,
        decision: &str,
    ) -> Result<String, String> {
        let notif = QueuedNotification::new(
            appellant,
            coop_id,
            "Appeal Decision",
            format!("Your appeal has been {decision}"),
            NotificationType::AppealDecision,
        )
        .with_data(serde_json::json!({
            "type": "appeal_decision",
            "appeal_id": appeal_id,
            "decision": decision,
        }))
        .with_email(); // Important decisions should go to email too
        self.queue.queue(notif).await
    }

    /// Queue notification for membership status change
    pub async fn on_membership_changed(
        &self,
        member: &str,
        coop_id: &str,
        status: &str,
        reason: Option<&str>,
    ) -> Result<String, String> {
        let body = match reason {
            Some(r) => format!("Your membership status is now: {status}. Reason: {r}"),
            None => format!("Your membership status is now: {status}"),
        };

        let notif = QueuedNotification::new(
            member,
            coop_id,
            "Membership Update",
            body,
            NotificationType::MembershipChange,
        )
        .with_data(serde_json::json!({
            "type": "membership_changed",
            "status": status,
            "reason": reason,
        }));
        self.queue.queue(notif).await
    }

    /// Queue notification when deliberation starts on a proposal
    pub async fn on_deliberation_started(
        &self,
        recipient: &str,
        coop_id: &str,
        proposal_id: &str,
        proposal_title: &str,
        ends_at: u64,
    ) -> Result<String, String> {
        let days_remaining = (ends_at.saturating_sub(current_timestamp())) / 86400;
        let body = format!(
            "📣 New proposal open for discussion: \"{proposal_title}\"\nDeliberation ends in {days_remaining} days. Join the conversation!"
        );

        let notif = QueuedNotification::new(
            recipient,
            coop_id,
            "Deliberation Started",
            body,
            NotificationType::DeliberationStarted,
        )
        .with_data(serde_json::json!({
            "type": "deliberation_started",
            "proposal_id": proposal_id,
            "proposal_title": proposal_title,
            "ends_at": ends_at,
        }));
        self.queue.queue(notif).await
    }

    /// Queue notification when deliberation is ending soon
    pub async fn on_deliberation_ending_soon(
        &self,
        recipient: &str,
        coop_id: &str,
        proposal_id: &str,
        proposal_title: &str,
        hours_remaining: u64,
    ) -> Result<String, String> {
        let body = format!(
            "⏰ Deliberation ending soon: \"{proposal_title}\"\nOnly {hours_remaining} hours left to comment before voting begins."
        );

        let notif = QueuedNotification::new(
            recipient,
            coop_id,
            "Deliberation Ending Soon",
            body,
            NotificationType::DeliberationEnding,
        )
        .with_data(serde_json::json!({
            "type": "deliberation_ending",
            "proposal_id": proposal_id,
            "proposal_title": proposal_title,
            "hours_remaining": hours_remaining,
        }));
        self.queue.queue(notif).await
    }

    /// Queue notification when a comment is added to a proposal
    pub async fn on_comment_added(
        &self,
        recipient: &str,
        coop_id: &str,
        proposal_id: &str,
        comment_id: &str,
        commenter: &str,
    ) -> Result<String, String> {
        let body = format!(
            "{} commented on a proposal you're following",
            truncate_did(commenter)
        );

        let notif = QueuedNotification::new(
            recipient,
            coop_id,
            "New Comment",
            body,
            NotificationType::CommentAdded,
        )
        .with_data(serde_json::json!({
            "type": "comment_added",
            "proposal_id": proposal_id,
            "comment_id": comment_id,
            "commenter": commenter,
        }));
        self.queue.queue(notif).await
    }

    /// Queue notification when someone replies to your comment
    pub async fn on_comment_reply(
        &self,
        recipient: &str,
        coop_id: &str,
        proposal_id: &str,
        comment_id: &str,
        parent_comment_id: &str,
        replier: &str,
    ) -> Result<String, String> {
        let body = format!("{} replied to your comment", truncate_did(replier));

        let notif = QueuedNotification::new(
            recipient,
            coop_id,
            "New Reply",
            body,
            NotificationType::CommentReply,
        )
        .with_data(serde_json::json!({
            "type": "comment_reply",
            "proposal_id": proposal_id,
            "comment_id": comment_id,
            "parent_comment_id": parent_comment_id,
            "replier": replier,
        }));
        self.queue.queue(notif).await
    }

    /// Queue notification when someone reacts to your comment
    pub async fn on_comment_reaction(
        &self,
        recipient: &str,
        coop_id: &str,
        proposal_id: &str,
        comment_id: &str,
        reactor: &str,
        emoji: &str,
    ) -> Result<String, String> {
        let body = format!(
            "{} reacted {} to your comment",
            truncate_did(reactor),
            emoji
        );

        let notif = QueuedNotification::new(
            recipient,
            coop_id,
            "New Reaction",
            body,
            NotificationType::CommentReaction,
        )
        .with_data(serde_json::json!({
            "type": "comment_reaction",
            "proposal_id": proposal_id,
            "comment_id": comment_id,
            "reactor": reactor,
            "emoji": emoji,
        }));
        self.queue.queue(notif).await
    }
}

/// Get current timestamp in seconds
fn current_timestamp() -> u64 {
    icn_time::current_timestamp_secs()
}

/// Truncate DID for display
fn truncate_did(did: &str) -> String {
    if did.len() > 20 {
        format!("{}...{}", &did[..12], &did[did.len() - 6..])
    } else {
        did.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_ledger::create_shared_emitter;

    #[tokio::test]
    async fn test_ledger_trigger_handles_transaction() {
        let emitter = create_shared_emitter();
        let (queue, mut receiver) = NotificationQueue::new();
        let queue = Arc::new(queue);

        let trigger =
            LedgerNotificationTrigger::new(emitter.clone(), queue.clone(), "test-coop".to_string());
        let _handle = trigger.start();

        // Give trigger time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Emit transaction event
        emitter.emit(LedgerEvent::TransactionCreated(
            icn_ledger::TransactionCreated {
                entry_hash: "hash123".to_string(),
                author: "did:icn:alice".to_string(),
                transfers: vec![icn_ledger::Transfer {
                    from: "did:icn:alice".to_string(),
                    to: "did:icn:bob".to_string(),
                    amount: 10,
                    currency: "hours".to_string(),
                }],
                timestamp: 1234567890,
                domain_id: Some("test-coop".to_string()),
            },
        ));

        // Wait for notifications to be queued
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Should receive notifications for both sender and recipient
        let notif1 = receiver.recv().await;
        assert!(notif1.is_some());

        let notif2 = receiver.recv().await;
        assert!(notif2.is_some());
    }

    #[tokio::test]
    async fn test_governance_trigger_proposal() {
        let (queue, mut receiver) = NotificationQueue::new();
        let queue = Arc::new(queue);
        let trigger = GovernanceNotificationTrigger::new(queue);

        trigger
            .on_proposal_created(
                "did:icn:alice",
                "test-coop",
                "prop-1",
                "Budget Proposal",
                "did:icn:bob",
            )
            .await
            .unwrap();

        let notif = receiver.recv().await.unwrap();
        assert_eq!(notif.recipient, "did:icn:alice");
        assert!(notif.title.contains("Proposal"));
    }
}
