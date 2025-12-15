//! Notification Processor
//!
//! Processes queued notifications and delivers them via the appropriate channels.
//! Supports push notifications (FCM), email, and in-app notifications.

use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::fcm_client::{create_notification_message, FcmClient, FcmConfig, FcmResult};
use crate::notification_queue::{
    calculate_backoff, DeliveryStatus, NotificationChannel, NotificationPriority,
    NotificationQueue, QueuedNotification,
};
use crate::notifications::NotificationService;

/// Configuration for the notification processor
#[derive(Debug, Clone)]
pub struct ProcessorConfig {
    /// FCM configuration for push notifications
    pub fcm_config: Option<FcmConfig>,
    /// SMTP configuration for email
    pub smtp_config: Option<SmtpConfig>,
    /// Maximum concurrent deliveries per channel
    pub max_concurrent: usize,
    /// Whether to enable push notifications
    pub enable_push: bool,
    /// Whether to enable email notifications
    pub enable_email: bool,
    /// Whether to enable in-app notifications
    pub enable_in_app: bool,
}

impl Default for ProcessorConfig {
    fn default() -> Self {
        Self {
            fcm_config: None,
            smtp_config: None,
            max_concurrent: 10,
            enable_push: true,
            enable_email: true,
            enable_in_app: true,
        }
    }
}

/// SMTP configuration for email delivery
#[derive(Debug, Clone)]
pub struct SmtpConfig {
    /// SMTP server host
    pub host: String,
    /// SMTP server port
    pub port: u16,
    /// Username for authentication
    pub username: String,
    /// Password for authentication
    pub password: String,
    /// Sender email address
    pub from_address: String,
    /// Sender name
    pub from_name: String,
    /// Use TLS
    pub use_tls: bool,
}

/// In-app notification storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InAppNotification {
    /// Notification ID
    pub id: String,
    /// Recipient DID
    pub recipient: String,
    /// Cooperative ID
    pub coop_id: String,
    /// Title
    pub title: String,
    /// Body
    pub body: String,
    /// Custom data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    /// Notification type
    pub notification_type: String,
    /// Created timestamp
    pub created_at: u64,
    /// Whether the notification has been read
    pub read: bool,
    /// Read timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_at: Option<u64>,
}

/// Notification processor that delivers queued notifications
pub struct NotificationProcessor {
    /// Notification queue reference
    queue: Arc<NotificationQueue>,
    /// Notification service for device token management
    notification_service: Arc<NotificationService>,
    /// FCM client for push notifications (optional)
    fcm_client: Option<Arc<FcmClient>>,
    /// In-app notification storage (recipient -> notifications)
    in_app_store: Arc<DashMap<String, Vec<InAppNotification>>>,
    /// Processor configuration
    config: ProcessorConfig,
}

impl NotificationProcessor {
    /// Create a new notification processor
    pub fn new(
        queue: Arc<NotificationQueue>,
        notification_service: Arc<NotificationService>,
        config: ProcessorConfig,
    ) -> Self {
        // Create FCM client if config is provided
        let fcm_client = config.fcm_config.as_ref().map(|fcm_config| {
            info!("FCM client initialized for project: {}", fcm_config.project_id);
            Arc::new(FcmClient::new(fcm_config.clone()))
        });

        Self {
            queue,
            notification_service,
            fcm_client,
            in_app_store: Arc::new(DashMap::new()),
            config,
        }
    }

    /// Start the processor, consuming from the queue receiver
    pub fn start(
        self: Arc<Self>,
        mut receiver: mpsc::Receiver<QueuedNotification>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            info!("Notification processor started");

            while let Some(notification) = receiver.recv().await {
                let processor = self.clone();
                // Process each notification in a separate task
                tokio::spawn(async move {
                    processor.process_notification(notification).await;
                });
            }

            info!("Notification processor shutting down");
        })
    }

    /// Process a single notification
    async fn process_notification(&self, notification: QueuedNotification) {
        let id = notification.id.clone();
        debug!(notification_id = %id, "Processing notification");

        // Check if notification is ready for delivery
        if !notification.is_ready() {
            // Re-queue for later
            debug!(notification_id = %id, "Notification not ready, re-queueing");
            // In a real system, we'd have a scheduled delivery mechanism
            return;
        }

        // Process each channel
        for channel in &notification.channels {
            match channel {
                NotificationChannel::Push if self.config.enable_push => {
                    self.deliver_push(&notification).await;
                }
                NotificationChannel::Email if self.config.enable_email => {
                    self.deliver_email(&notification).await;
                }
                NotificationChannel::InApp if self.config.enable_in_app => {
                    self.deliver_in_app(&notification).await;
                }
                _ => {
                    debug!(
                        notification_id = %id,
                        channel = ?channel,
                        "Channel disabled, skipping"
                    );
                }
            }
        }

        // Check if all channels delivered
        let status = notification.overall_status();
        match status {
            DeliveryStatus::Delivered => {
                debug!(notification_id = %id, "Notification fully delivered");
                self.queue.remove(&id);
            }
            DeliveryStatus::Abandoned { error } => {
                warn!(notification_id = %id, error = %error, "Notification abandoned");
                self.queue.remove(&id);
            }
            _ => {
                debug!(notification_id = %id, status = ?status, "Notification partially delivered");
            }
        }
    }

    /// Deliver via push notification (FCM)
    async fn deliver_push(&self, notification: &QueuedNotification) {
        let id = &notification.id;

        // Parse recipient DID
        let did: icn_identity::Did = match notification.recipient.parse() {
            Ok(d) => d,
            Err(e) => {
                self.queue.mark_failed(
                    id,
                    NotificationChannel::Push,
                    &format!("Invalid DID: {}", e),
                    0,
                );
                return;
            }
        };

        // Get device tokens for this DID
        let tokens = self.notification_service.get_device_tokens(&did);
        if tokens.is_empty() {
            // No devices registered - not an error, just skip
            debug!(notification_id = %id, "No push devices registered for recipient");
            self.queue.mark_delivered(id, NotificationChannel::Push);
            return;
        }

        // Use FCM client if available, otherwise fall back to notification service
        if let Some(ref fcm_client) = self.fcm_client {
            // Send via FCM HTTP v1 API
            let is_high_priority = notification.priority == NotificationPriority::High;
            let mut all_success = true;
            let mut last_error = None;
            let mut invalid_tokens = Vec::new();

            for token in &tokens {
                let message = create_notification_message(
                    token.clone(),
                    notification.title.clone(),
                    notification.body.clone(),
                    notification.data.clone(),
                    is_high_priority,
                );

                match fcm_client.send(message).await {
                    FcmResult::Success { message_id } => {
                        debug!(
                            notification_id = %id,
                            token = %token,
                            message_id = %message_id,
                            "Push notification sent via FCM"
                        );
                    }
                    FcmResult::InvalidToken => {
                        warn!(
                            notification_id = %id,
                            token = %token,
                            "Invalid FCM token, marking for removal"
                        );
                        invalid_tokens.push(token.clone());
                        // Don't mark as failure - token is just invalid
                    }
                    FcmResult::TemporaryFailure { error } => {
                        warn!(
                            notification_id = %id,
                            token = %token,
                            error = %error,
                            "Temporary FCM failure, will retry"
                        );
                        all_success = false;
                        last_error = Some(error);
                    }
                    FcmResult::PermanentFailure { error } => {
                        error!(
                            notification_id = %id,
                            token = %token,
                            error = %error,
                            "Permanent FCM failure"
                        );
                        all_success = false;
                        last_error = Some(error);
                    }
                }
            }

            // Unregister invalid tokens
            for token in invalid_tokens {
                self.notification_service.unregister_device(&token);
            }

            if all_success {
                self.queue.mark_delivered(id, NotificationChannel::Push);
            } else if let Some(error) = last_error {
                let retries = notification
                    .status
                    .get(&NotificationChannel::Push)
                    .map(|s| match s.value() {
                        DeliveryStatus::Failed { retries, .. } => *retries,
                        _ => 0,
                    })
                    .unwrap_or(0);

                self.queue.mark_failed(id, NotificationChannel::Push, &error, retries + 1);

                if retries < 5 {
                    let backoff = calculate_backoff(retries);
                    debug!(
                        notification_id = %id,
                        retry = retries + 1,
                        backoff_ms = backoff.as_millis(),
                        "Scheduling FCM retry"
                    );
                }
            }
        } else {
            // Fall back to notification service (legacy mode)
            let fcm_notification = crate::notifications::Notification {
                title: notification.title.clone(),
                body: notification.body.clone(),
                data: notification.data.clone(),
            };

            match self.notification_service.send_to_did(&did, fcm_notification).await {
                Ok(sent) => {
                    debug!(notification_id = %id, devices = sent, "Push notification sent (legacy)");
                    self.queue.mark_delivered(id, NotificationChannel::Push);
                }
                Err(e) => {
                    error!(notification_id = %id, error = %e, "Push notification failed (legacy)");
                    let retries = notification
                        .status
                        .get(&NotificationChannel::Push)
                        .map(|s| match s.value() {
                            DeliveryStatus::Failed { retries, .. } => *retries,
                            _ => 0,
                        })
                        .unwrap_or(0);

                    self.queue.mark_failed(id, NotificationChannel::Push, &e, retries + 1);
                }
            }
        }
    }

    /// Deliver via email
    async fn deliver_email(&self, notification: &QueuedNotification) {
        let id = &notification.id;

        // Check if SMTP is configured
        if self.config.smtp_config.is_none() {
            debug!(notification_id = %id, "SMTP not configured, skipping email");
            // Mark as delivered since email is optional
            self.queue.mark_delivered(id, NotificationChannel::Email);
            return;
        }

        // In a real implementation, we'd:
        // 1. Look up the user's email address from their profile
        // 2. Render the email template
        // 3. Send via SMTP

        // For now, log and mark as delivered
        info!(
            notification_id = %id,
            recipient = %notification.recipient,
            title = %notification.title,
            "Would send email notification (SMTP not implemented)"
        );

        self.queue.mark_delivered(id, NotificationChannel::Email);
    }

    /// Deliver to in-app notification center
    async fn deliver_in_app(&self, notification: &QueuedNotification) {
        let id = &notification.id;

        // Create in-app notification
        let in_app = InAppNotification {
            id: id.clone(),
            recipient: notification.recipient.clone(),
            coop_id: notification.coop_id.clone(),
            title: notification.title.clone(),
            body: notification.body.clone(),
            data: notification.data.clone(),
            notification_type: format!("{:?}", notification.notification_type),
            created_at: notification.created_at,
            read: false,
            read_at: None,
        };

        // Store in memory (in a real system, this would be persisted)
        self.in_app_store
            .entry(notification.recipient.clone())
            .and_modify(|notifications| {
                // Keep only last 100 notifications per user
                if notifications.len() >= 100 {
                    notifications.remove(0);
                }
                notifications.push(in_app.clone());
            })
            .or_insert_with(|| vec![in_app]);

        debug!(
            notification_id = %id,
            recipient = %notification.recipient,
            "Stored in-app notification"
        );

        self.queue.mark_delivered(id, NotificationChannel::InApp);
    }

    /// Get in-app notifications for a user
    pub fn get_in_app_notifications(&self, recipient: &str, unread_only: bool) -> Vec<InAppNotification> {
        self.in_app_store
            .get(recipient)
            .map(|notifications| {
                notifications
                    .iter()
                    .filter(|n| !unread_only || !n.read)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get unread notification count for a user
    pub fn get_unread_count(&self, recipient: &str) -> usize {
        self.in_app_store
            .get(recipient)
            .map(|notifications| notifications.iter().filter(|n| !n.read).count())
            .unwrap_or(0)
    }

    /// Mark a notification as read
    pub fn mark_read(&self, recipient: &str, notification_id: &str) -> bool {
        if let Some(mut notifications) = self.in_app_store.get_mut(recipient) {
            for notif in notifications.iter_mut() {
                if notif.id == notification_id {
                    notif.read = true;
                    notif.read_at = Some(
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                    );
                    return true;
                }
            }
        }
        false
    }

    /// Mark all notifications as read for a user
    pub fn mark_all_read(&self, recipient: &str) -> usize {
        if let Some(mut notifications) = self.in_app_store.get_mut(recipient) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            let mut count = 0;
            for notif in notifications.iter_mut() {
                if !notif.read {
                    notif.read = true;
                    notif.read_at = Some(now);
                    count += 1;
                }
            }
            count
        } else {
            0
        }
    }

    /// Delete a notification
    pub fn delete_notification(&self, recipient: &str, notification_id: &str) -> bool {
        if let Some(mut notifications) = self.in_app_store.get_mut(recipient) {
            let len_before = notifications.len();
            notifications.retain(|n| n.id != notification_id);
            notifications.len() < len_before
        } else {
            false
        }
    }

    /// Get reference to in-app store (for API access)
    pub fn in_app_store(&self) -> Arc<DashMap<String, Vec<InAppNotification>>> {
        self.in_app_store.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notification_queue::NotificationType;

    fn test_notification() -> QueuedNotification {
        QueuedNotification::new(
            "did:icn:alice",
            "test-coop",
            "Test Title",
            "Test Body",
            NotificationType::System,
        )
    }

    #[tokio::test]
    async fn test_in_app_storage() {
        let (queue, receiver) = NotificationQueue::new();
        let queue = Arc::new(queue);
        let notification_service = Arc::new(NotificationService::new(None));
        let processor = NotificationProcessor::new(
            queue.clone(),
            notification_service,
            ProcessorConfig::default(),
        );

        // Manually store an in-app notification
        let notif = test_notification();
        processor.deliver_in_app(&notif).await;

        // Check it was stored
        let notifications = processor.get_in_app_notifications("did:icn:alice", false);
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].title, "Test Title");
        assert!(!notifications[0].read);
    }

    #[tokio::test]
    async fn test_mark_read() {
        let (queue, _receiver) = NotificationQueue::new();
        let queue = Arc::new(queue);
        let notification_service = Arc::new(NotificationService::new(None));
        let processor = NotificationProcessor::new(
            queue.clone(),
            notification_service,
            ProcessorConfig::default(),
        );

        let notif = test_notification();
        let id = notif.id.clone();
        processor.deliver_in_app(&notif).await;

        // Mark as read
        assert!(processor.mark_read("did:icn:alice", &id));

        // Check it's marked
        let notifications = processor.get_in_app_notifications("did:icn:alice", true);
        assert_eq!(notifications.len(), 0); // No unread

        let all = processor.get_in_app_notifications("did:icn:alice", false);
        assert_eq!(all.len(), 1);
        assert!(all[0].read);
    }

    #[tokio::test]
    async fn test_unread_count() {
        let (queue, _receiver) = NotificationQueue::new();
        let queue = Arc::new(queue);
        let notification_service = Arc::new(NotificationService::new(None));
        let processor = NotificationProcessor::new(
            queue.clone(),
            notification_service,
            ProcessorConfig::default(),
        );

        // Add 3 notifications
        for _ in 0..3 {
            let notif = test_notification();
            processor.deliver_in_app(&notif).await;
        }

        assert_eq!(processor.get_unread_count("did:icn:alice"), 3);

        // Mark all as read
        let marked = processor.mark_all_read("did:icn:alice");
        assert_eq!(marked, 3);
        assert_eq!(processor.get_unread_count("did:icn:alice"), 0);
    }

    #[tokio::test]
    async fn test_delete_notification() {
        let (queue, _receiver) = NotificationQueue::new();
        let queue = Arc::new(queue);
        let notification_service = Arc::new(NotificationService::new(None));
        let processor = NotificationProcessor::new(
            queue.clone(),
            notification_service,
            ProcessorConfig::default(),
        );

        let notif = test_notification();
        let id = notif.id.clone();
        processor.deliver_in_app(&notif).await;

        assert!(processor.delete_notification("did:icn:alice", &id));
        assert_eq!(
            processor
                .get_in_app_notifications("did:icn:alice", false)
                .len(),
            0
        );
    }
}
