//! Notification Queue System
//!
//! Provides reliable notification delivery with:
//! - Multi-channel support (push, email, in-app)
//! - Retry logic with exponential backoff
//! - Delivery tracking and status
//! - Event-triggered notifications

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tracing::debug;

/// Maximum retry attempts before giving up
const MAX_RETRIES: u32 = 5;

/// Base delay for exponential backoff (milliseconds)
const BASE_BACKOFF_MS: u64 = 1000;

/// Maximum backoff delay (30 seconds)
const MAX_BACKOFF_MS: u64 = 30000;

/// Notification channels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationChannel {
    /// Firebase Cloud Messaging push notifications
    Push,
    /// Email notifications
    Email,
    /// In-app notification center
    InApp,
}

/// Priority levels for notifications
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum NotificationPriority {
    /// Urgent - deliver immediately
    High,
    /// Normal delivery
    #[default]
    Normal,
    /// Low priority - can be batched
    Low,
}

/// Notification delivery status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    /// Queued for delivery
    Pending,
    /// Currently being processed
    Processing,
    /// Successfully delivered
    Delivered,
    /// Delivery failed, will retry
    Failed { error: String, retries: u32 },
    /// Permanently failed after max retries
    Abandoned { error: String },
}

/// A notification entry in the queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedNotification {
    /// Unique notification ID
    pub id: String,
    /// Target recipient DID
    pub recipient: String,
    /// Cooperative context
    pub coop_id: String,
    /// Notification title
    pub title: String,
    /// Notification body
    pub body: String,
    /// Target channels
    pub channels: Vec<NotificationChannel>,
    /// Priority level
    pub priority: NotificationPriority,
    /// Custom data payload
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    /// Notification type for categorization
    pub notification_type: NotificationType,
    /// Creation timestamp
    pub created_at: u64,
    /// Delivery status per channel (not serialized - runtime state only)
    #[serde(skip)]
    pub status: DashMap<NotificationChannel, DeliveryStatus>,
    /// Scheduled delivery time (None = immediate)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_at: Option<u64>,
}

/// Types of notifications for categorization and preferences
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationType {
    /// Payment received
    PaymentReceived,
    /// Payment sent confirmation
    PaymentSent,
    /// New proposal created
    ProposalCreated,
    /// Proposal closing soon
    ProposalClosing,
    /// Vote recorded
    VoteRecorded,
    /// Proposal outcome
    ProposalResult,
    /// Membership status change
    MembershipChange,
    /// Amendment proposed
    AmendmentProposed,
    /// Amendment result
    AmendmentResult,
    /// Appeal filed
    AppealFiled,
    /// Appeal decision
    AppealDecision,
    /// Balance change
    BalanceChange,
    /// Security alert (frozen account, fork detected, etc.)
    SecurityAlert,
    /// General system notification
    System,
}

impl QueuedNotification {
    /// Create a new queued notification
    pub fn new(
        recipient: impl Into<String>,
        coop_id: impl Into<String>,
        title: impl Into<String>,
        body: impl Into<String>,
        notification_type: NotificationType,
    ) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        let now = current_timestamp();

        Self {
            id,
            recipient: recipient.into(),
            coop_id: coop_id.into(),
            title: title.into(),
            body: body.into(),
            channels: vec![NotificationChannel::Push, NotificationChannel::InApp],
            priority: NotificationPriority::Normal,
            data: None,
            notification_type,
            created_at: now,
            status: DashMap::new(),
            scheduled_at: None,
        }
    }

    /// Add custom data
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: NotificationPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Set channels
    pub fn with_channels(mut self, channels: Vec<NotificationChannel>) -> Self {
        self.channels = channels;
        self
    }

    /// Schedule for later delivery
    pub fn scheduled_at(mut self, timestamp: u64) -> Self {
        self.scheduled_at = Some(timestamp);
        self
    }

    /// Include email channel
    pub fn with_email(mut self) -> Self {
        if !self.channels.contains(&NotificationChannel::Email) {
            self.channels.push(NotificationChannel::Email);
        }
        self
    }

    /// Check if notification is ready for delivery
    pub fn is_ready(&self) -> bool {
        match self.scheduled_at {
            Some(scheduled) => current_timestamp() >= scheduled,
            None => true,
        }
    }

    /// Get overall delivery status
    pub fn overall_status(&self) -> DeliveryStatus {
        let mut all_delivered = true;
        let mut any_pending = false;
        let mut last_error = None;

        for entry in self.status.iter() {
            match entry.value() {
                DeliveryStatus::Delivered => {}
                DeliveryStatus::Pending | DeliveryStatus::Processing => {
                    any_pending = true;
                    all_delivered = false;
                }
                DeliveryStatus::Failed { error, retries } => {
                    all_delivered = false;
                    last_error = Some((error.clone(), *retries));
                }
                DeliveryStatus::Abandoned { error } => {
                    all_delivered = false;
                    last_error = Some((error.clone(), MAX_RETRIES));
                }
            }
        }

        if all_delivered && !self.status.is_empty() {
            DeliveryStatus::Delivered
        } else if any_pending {
            DeliveryStatus::Pending
        } else if let Some((error, retries)) = last_error {
            if retries >= MAX_RETRIES {
                DeliveryStatus::Abandoned { error }
            } else {
                DeliveryStatus::Failed { error, retries }
            }
        } else {
            DeliveryStatus::Pending
        }
    }
}

/// Notification queue manager
pub struct NotificationQueue {
    /// Pending notifications
    pending: Arc<DashMap<String, QueuedNotification>>,
    /// Channel for submitting notifications
    sender: mpsc::Sender<QueuedNotification>,
    /// Notification statistics
    stats: Arc<NotificationStats>,
}

/// Statistics for notification delivery
pub struct NotificationStats {
    pub queued: AtomicU64,
    pub delivered: AtomicU64,
    pub failed: AtomicU64,
    pub retried: AtomicU64,
}

impl Default for NotificationStats {
    fn default() -> Self {
        Self {
            queued: AtomicU64::new(0),
            delivered: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            retried: AtomicU64::new(0),
        }
    }
}

impl NotificationQueue {
    /// Create a new notification queue
    ///
    /// Returns the queue and a receiver for processing notifications.
    pub fn new() -> (Self, mpsc::Receiver<QueuedNotification>) {
        let (sender, receiver) = mpsc::channel(1000);
        let queue = Self {
            pending: Arc::new(DashMap::new()),
            sender,
            stats: Arc::new(NotificationStats::default()),
        };
        (queue, receiver)
    }

    /// Queue a notification for delivery
    pub async fn queue(&self, notification: QueuedNotification) -> Result<String, String> {
        let id = notification.id.clone();

        // Initialize status for each channel
        for channel in &notification.channels {
            notification
                .status
                .insert(*channel, DeliveryStatus::Pending);
        }

        // Store in pending map
        self.pending.insert(id.clone(), notification.clone());
        self.stats.queued.fetch_add(1, Ordering::Relaxed);

        // Send to processing channel
        self.sender
            .send(notification)
            .await
            .map_err(|e| format!("Failed to queue notification: {e}"))?;

        debug!(notification_id = %id, "Notification queued");
        Ok(id)
    }

    /// Queue a simple notification
    pub async fn queue_simple(
        &self,
        recipient: &str,
        coop_id: &str,
        title: &str,
        body: &str,
        notification_type: NotificationType,
    ) -> Result<String, String> {
        let notification =
            QueuedNotification::new(recipient, coop_id, title, body, notification_type);
        self.queue(notification).await
    }

    /// Get notification by ID
    pub fn get(&self, id: &str) -> Option<QueuedNotification> {
        self.pending.get(id).map(|n| n.clone())
    }

    /// Update notification status
    pub fn update_status(&self, id: &str, channel: NotificationChannel, status: DeliveryStatus) {
        if let Some(notification) = self.pending.get(id) {
            notification.status.insert(channel, status);
        }
    }

    /// Mark channel as delivered
    pub fn mark_delivered(&self, id: &str, channel: NotificationChannel) {
        self.update_status(id, channel, DeliveryStatus::Delivered);
        self.stats.delivered.fetch_add(1, Ordering::Relaxed);
    }

    /// Mark channel as failed
    pub fn mark_failed(&self, id: &str, channel: NotificationChannel, error: &str, retries: u32) {
        if retries >= MAX_RETRIES {
            self.update_status(
                id,
                channel,
                DeliveryStatus::Abandoned {
                    error: error.to_string(),
                },
            );
            self.stats.failed.fetch_add(1, Ordering::Relaxed);
        } else {
            self.update_status(
                id,
                channel,
                DeliveryStatus::Failed {
                    error: error.to_string(),
                    retries,
                },
            );
            self.stats.retried.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Remove completed notification
    pub fn remove(&self, id: &str) {
        self.pending.remove(id);
    }

    /// Get queue statistics
    pub fn get_stats(&self) -> NotificationStatsSnapshot {
        NotificationStatsSnapshot {
            queued: self.stats.queued.load(Ordering::Relaxed),
            delivered: self.stats.delivered.load(Ordering::Relaxed),
            failed: self.stats.failed.load(Ordering::Relaxed),
            retried: self.stats.retried.load(Ordering::Relaxed),
            pending_count: self.pending.len(),
        }
    }

    /// Get pending notifications for a recipient
    pub fn get_pending_for_recipient(&self, recipient: &str) -> Vec<QueuedNotification> {
        self.pending
            .iter()
            .filter(|entry| entry.value().recipient == recipient)
            .map(|entry| entry.value().clone())
            .collect()
    }
}

/// Snapshot of notification statistics
#[derive(Debug, Clone, Serialize)]
pub struct NotificationStatsSnapshot {
    pub queued: u64,
    pub delivered: u64,
    pub failed: u64,
    pub retried: u64,
    pub pending_count: usize,
}

/// Calculate exponential backoff delay
pub fn calculate_backoff(retries: u32) -> Duration {
    let delay_ms = BASE_BACKOFF_MS * 2u64.pow(retries);
    let capped_delay = delay_ms.min(MAX_BACKOFF_MS);
    Duration::from_millis(capped_delay)
}

/// Get current Unix timestamp
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time before UNIX epoch")
        .as_secs()
}

// ============================================================================
// Notification Factory Functions
// ============================================================================

/// Create payment received notification
pub fn payment_received_notification(
    recipient: &str,
    coop_id: &str,
    amount: i64,
    currency: &str,
    from: &str,
    tx_hash: &str,
) -> QueuedNotification {
    QueuedNotification::new(
        recipient,
        coop_id,
        "Payment Received",
        format!(
            "You received {} {} from {}",
            amount,
            currency,
            truncate_did(from)
        ),
        NotificationType::PaymentReceived,
    )
    .with_data(serde_json::json!({
        "type": "payment_received",
        "amount": amount,
        "currency": currency,
        "from": from,
        "tx_hash": tx_hash,
    }))
    .with_priority(NotificationPriority::High)
}

/// Create payment sent confirmation
pub fn payment_sent_notification(
    sender: &str,
    coop_id: &str,
    amount: i64,
    currency: &str,
    to: &str,
    tx_hash: &str,
) -> QueuedNotification {
    QueuedNotification::new(
        sender,
        coop_id,
        "Payment Sent",
        format!("You sent {} {} to {}", amount, currency, truncate_did(to)),
        NotificationType::PaymentSent,
    )
    .with_data(serde_json::json!({
        "type": "payment_sent",
        "amount": amount,
        "currency": currency,
        "to": to,
        "tx_hash": tx_hash,
    }))
}

/// Create balance change notification
pub fn balance_change_notification(
    account: &str,
    coop_id: &str,
    currency: &str,
    old_balance: i64,
    new_balance: i64,
    change: i64,
) -> QueuedNotification {
    let direction = if change > 0 { "increased" } else { "decreased" };
    QueuedNotification::new(
        account,
        coop_id,
        "Balance Update",
        format!(
            "Your {} balance {} by {} (now: {})",
            currency,
            direction,
            change.abs(),
            new_balance
        ),
        NotificationType::BalanceChange,
    )
    .with_data(serde_json::json!({
        "type": "balance_change",
        "currency": currency,
        "old_balance": old_balance,
        "new_balance": new_balance,
        "change": change,
    }))
}

/// Create proposal notification
pub fn proposal_created_notification(
    recipient: &str,
    coop_id: &str,
    proposal_id: &str,
    title: &str,
    proposer: &str,
) -> QueuedNotification {
    QueuedNotification::new(
        recipient,
        coop_id,
        "New Proposal",
        format!("{} by {}", title, truncate_did(proposer)),
        NotificationType::ProposalCreated,
    )
    .with_data(serde_json::json!({
        "type": "proposal_created",
        "proposal_id": proposal_id,
        "title": title,
        "proposer": proposer,
    }))
}

/// Create security alert notification
pub fn security_alert_notification(
    recipient: &str,
    coop_id: &str,
    alert_type: &str,
    message: &str,
) -> QueuedNotification {
    QueuedNotification::new(
        recipient,
        coop_id,
        "Security Alert",
        message.to_string(),
        NotificationType::SecurityAlert,
    )
    .with_data(serde_json::json!({
        "type": "security_alert",
        "alert_type": alert_type,
    }))
    .with_priority(NotificationPriority::High)
    .with_email() // Security alerts should also go via email
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

    #[test]
    fn test_queued_notification_creation() {
        let notif = QueuedNotification::new(
            "did:icn:alice",
            "test-coop",
            "Test Title",
            "Test Body",
            NotificationType::System,
        );

        assert!(!notif.id.is_empty());
        assert_eq!(notif.recipient, "did:icn:alice");
        assert_eq!(notif.channels.len(), 2); // Push + InApp by default
    }

    #[test]
    fn test_notification_with_email() {
        let notif = QueuedNotification::new(
            "did:icn:alice",
            "test-coop",
            "Test",
            "Body",
            NotificationType::System,
        )
        .with_email();

        assert_eq!(notif.channels.len(), 3);
        assert!(notif.channels.contains(&NotificationChannel::Email));
    }

    #[test]
    fn test_calculate_backoff() {
        assert_eq!(calculate_backoff(0), Duration::from_millis(1000));
        assert_eq!(calculate_backoff(1), Duration::from_millis(2000));
        assert_eq!(calculate_backoff(2), Duration::from_millis(4000));
        assert_eq!(calculate_backoff(3), Duration::from_millis(8000));
        assert_eq!(calculate_backoff(4), Duration::from_millis(16000));
        assert_eq!(calculate_backoff(5), Duration::from_millis(30000)); // Capped
        assert_eq!(calculate_backoff(10), Duration::from_millis(30000)); // Still capped
    }

    #[test]
    fn test_notification_ready_check() {
        let notif = QueuedNotification::new(
            "did:icn:alice",
            "test-coop",
            "Test",
            "Body",
            NotificationType::System,
        );
        assert!(notif.is_ready()); // Immediate delivery

        let scheduled = notif.scheduled_at(current_timestamp() + 3600); // 1 hour in future
        assert!(!scheduled.is_ready());
    }

    #[tokio::test]
    async fn test_queue_notification() {
        let (queue, mut receiver) = NotificationQueue::new();

        let notif = QueuedNotification::new(
            "did:icn:alice",
            "test-coop",
            "Test",
            "Body",
            NotificationType::System,
        );

        let id = queue.queue(notif).await.unwrap();
        assert!(!id.is_empty());

        // Should receive notification
        let received = receiver.recv().await.unwrap();
        assert_eq!(received.id, id);
    }

    #[test]
    fn test_payment_notification_factory() {
        let notif = payment_received_notification(
            "did:icn:bob",
            "coop-1",
            100,
            "hours",
            "did:icn:alice",
            "hash123",
        );

        assert_eq!(notif.notification_type, NotificationType::PaymentReceived);
        assert_eq!(notif.priority, NotificationPriority::High);
        assert!(notif.title.contains("Payment Received"));
    }

    #[test]
    fn test_security_alert_includes_email() {
        let notif = security_alert_notification(
            "did:icn:alice",
            "coop-1",
            "account_frozen",
            "Your account has been frozen",
        );

        assert!(notif.channels.contains(&NotificationChannel::Email));
        assert_eq!(notif.priority, NotificationPriority::High);
    }
}
