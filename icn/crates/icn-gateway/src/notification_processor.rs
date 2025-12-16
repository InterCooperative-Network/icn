//! Notification Processor
//!
//! Processes queued notifications and delivers them via the appropriate channels.
//! Supports push notifications (FCM), email, and in-app notifications.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::email_client::{EmailClient, EmailResult, EmailTemplates, SmtpConfig};
use crate::fcm_client::{create_notification_message, FcmClient, FcmConfig, FcmResult};
use crate::notification_queue::{
    calculate_backoff, DeliveryStatus, NotificationChannel, NotificationPriority,
    NotificationQueue, QueuedNotification,
};
use crate::notifications::{NotificationService, NotificationStore, InAppNotification, DeliveryLogEntry};
use std::time::{SystemTime, UNIX_EPOCH};

/// Configuration for the notification processor
#[derive(Debug, Clone)]
pub struct ProcessorConfig {
    /// FCM configuration for push notifications
    pub fcm_config: Option<FcmConfig>,
    /// SMTP configuration for email
    pub smtp_config: Option<SmtpConfig>,
    /// Base URL for email links (e.g., "https://mycoop.org")
    pub email_base_url: String,
    /// Cooperative name for email branding
    pub email_coop_name: String,
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
            email_base_url: "https://localhost".to_string(),
            email_coop_name: "ICN Cooperative".to_string(),
            max_concurrent: 10,
            enable_push: true,
            enable_email: true,
            enable_in_app: true,
        }
    }
}

impl ProcessorConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        let mut config = Self::default();

        // Load SMTP config
        if let (Ok(host), Ok(port_str), Ok(username), Ok(password), Ok(from_address)) = (
            std::env::var("ICN_SMTP_HOST"),
            std::env::var("ICN_SMTP_PORT"),
            std::env::var("ICN_SMTP_USERNAME"),
            std::env::var("ICN_SMTP_PASSWORD"),
            std::env::var("ICN_SMTP_FROM_ADDRESS"),
        ) {
            if let Ok(port) = port_str.parse::<u16>() {
                let from_name = std::env::var("ICN_SMTP_FROM_NAME")
                    .unwrap_or_else(|_| "ICN Cooperative".to_string());
                
                config.smtp_config = Some(SmtpConfig::new(
                    host,
                    port,
                    username,
                    password,
                    from_address,
                    from_name,
                ));
                info!("SMTP configuration loaded from environment");
            } else {
                warn!("Invalid ICN_SMTP_PORT, SMTP disabled");
            }
        }

        // Load email branding
        if let Ok(url) = std::env::var("ICN_EMAIL_BASE_URL") {
            config.email_base_url = url;
        }
        if let Ok(name) = std::env::var("ICN_EMAIL_COOP_NAME") {
            config.email_coop_name = name;
        }

        // Load FCM config
        if let Ok(json) = std::env::var("ICN_FCM_SERVICE_ACCOUNT_JSON") {
            match FcmConfig::from_service_account_json(&json) {
                Ok(fcm_config) => {
                    config.fcm_config = Some(fcm_config);
                    info!("FCM configuration loaded from environment JSON");
                }
                Err(e) => warn!("Invalid FCM service account JSON: {}", e),
            }
        } else if let Ok(path) = std::env::var("ICN_FCM_SERVICE_ACCOUNT_FILE") {
            // Try loading from file
            match std::fs::read_to_string(&path) {
                Ok(json) => match FcmConfig::from_service_account_json(&json) {
                    Ok(fcm_config) => {
                        config.fcm_config = Some(fcm_config);
                        info!("FCM configuration loaded from file: {}", path);
                    }
                    Err(e) => warn!("Invalid FCM service account JSON in file {}: {}", path, e),
                },
                Err(e) => warn!("Failed to read FCM service account file {}: {}", path, e),
            }
        }

        config
    }
}

/// Notification processor that delivers queued notifications
pub struct NotificationProcessor {
    /// Notification queue reference
    queue: Arc<NotificationQueue>,
    /// Notification store for in-app notifications
    store: Arc<NotificationStore>,
    /// Notification service for device token management
    notification_service: Arc<NotificationService>,
    /// FCM client for push notifications (optional)
    fcm_client: Option<Arc<FcmClient>>,
    /// Email client for email notifications (optional)
    email_client: Option<Arc<EmailClient>>,
    /// Email templates
    email_templates: EmailTemplates,
    /// Processor configuration
    config: ProcessorConfig,
}

impl NotificationProcessor {
    /// Create a new notification processor
    pub fn new(
        queue: Arc<NotificationQueue>,
        store: Arc<NotificationStore>,
        notification_service: Arc<NotificationService>,
        config: ProcessorConfig,
    ) -> Self {
        // Create FCM client if config is provided
        let fcm_client = config.fcm_config.as_ref().map(|fcm_config| {
            info!(
                "FCM client initialized for project: {}",
                fcm_config.project_id
            );
            Arc::new(FcmClient::new(fcm_config.clone()))
        });

        // Create email client if SMTP config is provided
        let email_client = config.smtp_config.as_ref().map(|smtp_config| {
            info!("Email client initialized for SMTP: {}", smtp_config.host);
            Arc::new(EmailClient::new(smtp_config.clone()))
        });

        // Create email templates
        let email_templates = EmailTemplates::new(
            config.email_base_url.clone(),
            config.email_coop_name.clone(),
        );

        Self {
            queue,
            store,
            notification_service,
            fcm_client,
            email_client,
            email_templates,
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

    /// Helper to log delivery status
    fn log_delivery(
        &self,
        notification_id: &str,
        channel: NotificationChannel,
        status: &str,
        details: Option<String>,
    ) {
        if let Ok(timestamp) = SystemTime::now().duration_since(UNIX_EPOCH) {
            let entry = DeliveryLogEntry {
                notification_id: notification_id.to_string(),
                channel: format!("{:?}", channel),
                status: status.to_string(),
                timestamp: timestamp.as_secs(),
                details: details.clone(),
            };
            if let Err(e) = self.store.add_delivery_log(entry) {
                error!(
                    notification_id = %notification_id,
                    error = %e,
                    "Failed to persist delivery log"
                );
            }
        }
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
                    &format!("Invalid DID: {e}"),
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
                        self.log_delivery(id, NotificationChannel::Push, "failed", Some("Invalid token".to_string()));
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
                        last_error = Some(error.clone());
                        self.log_delivery(id, NotificationChannel::Push, "failed", Some(format!("Temporary: {}", error)));
                    }
                    FcmResult::PermanentFailure { error } => {
                        error!(
                            notification_id = %id,
                            token = %token,
                            error = %error,
                            "Permanent FCM failure"
                        );
                        all_success = false;
                        last_error = Some(error.clone());
                        self.log_delivery(id, NotificationChannel::Push, "failed", Some(format!("Permanent: {}", error)));
                    }
                }
            }

            // Unregister invalid tokens
            for token in invalid_tokens {
                self.notification_service.unregister_device(&token);
            }

            if all_success {
                self.log_delivery(id, NotificationChannel::Push, "delivered", None);
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

                self.queue
                    .mark_failed(id, NotificationChannel::Push, &error, retries + 1);

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

            match self
                .notification_service
                .send_to_did(&did, fcm_notification)
                .await
            {
                Ok(sent) => {
                    debug!(notification_id = %id, devices = sent, "Push notification sent (legacy)");
                    self.log_delivery(id, NotificationChannel::Push, "delivered", Some(format!("Legacy send to {} devices", sent)));
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

                    self.queue
                        .mark_failed(id, NotificationChannel::Push, &e, retries + 1);
                }
            }
        }
    }

    /// Deliver via email
    async fn deliver_email(&self, notification: &QueuedNotification) {
        let id = &notification.id;

        // Check if email client is available
        let email_client = match &self.email_client {
            Some(client) => client,
            None => {
                debug!(notification_id = %id, "Email client not configured, skipping email");
                // Mark as delivered since email is optional
                self.queue.mark_delivered(id, NotificationChannel::Email);
                return;
            }
        };

        // Get recipient email from notification data
        // In a full implementation, we'd look this up from user profile
        let recipient_email = notification
            .data
            .as_ref()
            .and_then(|d| d.get("recipient_email"))
            .and_then(|v| v.as_str());

        let recipient_email = match recipient_email {
            Some(email) => email,
            None => {
                debug!(
                    notification_id = %id,
                    recipient = %notification.recipient,
                    "No email address available for recipient, skipping email"
                );
                // Mark as delivered since we can't send without an email
                self.queue.mark_delivered(id, NotificationChannel::Email);
                return;
            }
        };

        // Generate email from template
        let email_message = self.email_templates.generate_email(
            recipient_email,
            notification.notification_type,
            &notification.title,
            &notification.body,
            notification.data.as_ref(),
        );

        // Send the email
        match email_client.send(email_message).await {
            EmailResult::Success { message_id } => {
                info!(
                    notification_id = %id,
                    recipient_email = %recipient_email,
                    message_id = %message_id,
                    "Email notification sent"
                );
                self.log_delivery(id, NotificationChannel::Email, "delivered", Some(format!("Message ID: {}", message_id)));
                self.queue.mark_delivered(id, NotificationChannel::Email);
            }
            EmailResult::InvalidRecipient { error } => {
                warn!(
                    notification_id = %id,
                    recipient_email = %recipient_email,
                    error = %error,
                    "Invalid email recipient"
                );
                // Mark as delivered - nothing more we can do
                self.log_delivery(id, NotificationChannel::Email, "failed", Some(format!("Invalid recipient: {}", error)));
                self.queue.mark_delivered(id, NotificationChannel::Email);
            }
            EmailResult::TemporaryFailure { error } => {
                warn!(
                    notification_id = %id,
                    recipient_email = %recipient_email,
                    error = %error,
                    "Temporary email failure, will retry"
                );
                self.log_delivery(id, NotificationChannel::Email, "failed", Some(format!("Temporary: {}", error)));
                let retries = notification
                    .status
                    .get(&NotificationChannel::Email)
                    .map(|s| match s.value() {
                        DeliveryStatus::Failed { retries, .. } => *retries,
                        _ => 0,
                    })
                    .unwrap_or(0);

                self.queue
                    .mark_failed(id, NotificationChannel::Email, &error, retries + 1);

                if retries < 5 {
                    let backoff = calculate_backoff(retries);
                    debug!(
                        notification_id = %id,
                        retry = retries + 1,
                        backoff_ms = backoff.as_millis(),
                        "Scheduling email retry"
                    );
                }
            }
            EmailResult::PermanentFailure { error } => {
                error!(
                    notification_id = %id,
                    recipient_email = %recipient_email,
                    error = %error,
                    "Permanent email failure"
                );
                // Mark as failed with max retries so it gets abandoned
                self.log_delivery(id, NotificationChannel::Email, "failed", Some(format!("Permanent: {}", error)));
                self.queue
                    .mark_failed(id, NotificationChannel::Email, &error, 999);
            }
        }
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

        // Persist
        if let Err(e) = self.store.add_notification(in_app) {
            error!(
                notification_id = %id,
                error = %e,
                "Failed to persist in-app notification"
            );
            // Don't mark as delivered? Or allow retry?
            // For now, retry
            let retries = notification
                .status
                .get(&NotificationChannel::InApp)
                .map(|s| match s.value() {
                    DeliveryStatus::Failed { retries, .. } => *retries,
                    _ => 0,
                })
                .unwrap_or(0);
                
             self.queue.mark_failed(id, NotificationChannel::InApp, &e.to_string(), retries + 1);
             self.log_delivery(id, NotificationChannel::InApp, "failed", Some(e.to_string()));
             return;
        }

        debug!(
            notification_id = %id,
            recipient = %notification.recipient,
            "Stored in-app notification"
        );

        self.log_delivery(id, NotificationChannel::InApp, "delivered", None);
        self.queue.mark_delivered(id, NotificationChannel::InApp);
    }

    /// Get in-app notifications for a user
    pub fn get_in_app_notifications(
        &self,
        recipient: &str,
        unread_only: bool,
    ) -> Vec<InAppNotification> {
        match self.store.get_notifications(recipient, unread_only, Some(100), None) {
            Ok((notifications, _)) => notifications,
            Err(e) => {
                error!("Failed to get notifications: {}", e);
                Vec::new()
            }
        }
    }

    /// Get unread notification count for a user
    pub fn get_unread_count(&self, recipient: &str) -> usize {
        self.store.get_unread_count(recipient).unwrap_or(0)
    }

    /// Mark a notification as read
    pub fn mark_read(&self, recipient: &str, notification_id: &str) -> bool {
        self.store.mark_read(recipient, notification_id).unwrap_or(false)
    }

    /// Mark all notifications as read for a user
    pub fn mark_all_read(&self, recipient: &str) -> usize {
        self.store.mark_all_read(recipient).unwrap_or(0)
    }

    /// Delete a notification
    pub fn delete_notification(&self, recipient: &str, notification_id: &str) -> Result<bool, anyhow::Error> {
        self.store.delete_notification(recipient, notification_id)
    }

    /// Get delivery logs for a notification
    pub fn get_delivery_logs(&self, notification_id: &str) -> Vec<DeliveryLogEntry> {
        self.store.get_delivery_logs(notification_id).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notification_queue::NotificationType;
    use icn_store::notifications::NotificationStore;
    use sled::Config;

    fn temp_store() -> Arc<NotificationStore> {
        let db = Config::new().temporary(true).open().unwrap();
        Arc::new(NotificationStore::new(db))
    }

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
        let (queue, _receiver) = NotificationQueue::new();
        let queue = Arc::new(queue);
        let store = temp_store();
        // Service also needs a store instance
        let notification_service = Arc::new(NotificationService::new(store.clone(), None));
        let processor = NotificationProcessor::new(
            queue.clone(),
            store.clone(),
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
        let store = temp_store();
        let notification_service = Arc::new(NotificationService::new(store.clone(), None));
        let processor = NotificationProcessor::new(
            queue.clone(),
            store.clone(),
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
        let store = temp_store();
        let notification_service = Arc::new(NotificationService::new(store.clone(), None));
        let processor = NotificationProcessor::new(
            queue.clone(),
            store.clone(),
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
        let store = temp_store();
        let notification_service = Arc::new(NotificationService::new(store.clone(), None));
        let processor = NotificationProcessor::new(
            queue.clone(),
            store.clone(),
            notification_service,
            ProcessorConfig::default(),
        );

        let notif = test_notification();
        let id = notif.id.clone();
        processor.deliver_in_app(&notif).await;

        assert!(processor.delete_notification("did:icn:alice", &id).unwrap());
        assert_eq!(
            processor
                .get_in_app_notifications("did:icn:alice", false)
                .len(),
            0
        );
    }

    #[test]
    fn test_config_from_env() {
        // Set environment variables
        // Use unique var names to avoid conflict with other tests if parallel
        let host_var = "ICN_TEST_SMTP_HOST";
        std::env::set_var("ICN_SMTP_HOST", "smtp.test.com");
        std::env::set_var("ICN_SMTP_PORT", "587");
        std::env::set_var("ICN_SMTP_USERNAME", "user");
        std::env::set_var("ICN_SMTP_PASSWORD", "pass");
        std::env::set_var("ICN_SMTP_FROM_ADDRESS", "test@test.com");
        std::env::set_var("ICN_EMAIL_BASE_URL", "https://test.coop");
        std::env::set_var("ICN_EMAIL_COOP_NAME", "Test Coop");

        let config = ProcessorConfig::from_env();

        assert!(config.smtp_config.is_some());
        let smtp = config.smtp_config.unwrap();
        assert_eq!(smtp.host, "smtp.test.com");
        assert_eq!(smtp.port, 587);
        assert_eq!(smtp.username, "user");
        assert_eq!(config.email_base_url, "https://test.coop");
        assert_eq!(config.email_coop_name, "Test Coop");

        // Clean up
        std::env::remove_var("ICN_SMTP_HOST");
        std::env::remove_var("ICN_SMTP_PORT");
        std::env::remove_var("ICN_SMTP_USERNAME");
        std::env::remove_var("ICN_SMTP_PASSWORD");
        std::env::remove_var("ICN_SMTP_FROM_ADDRESS");
        std::env::remove_var("ICN_EMAIL_BASE_URL");
        std::env::remove_var("ICN_EMAIL_COOP_NAME");
    }

    #[test]
    fn test_fcm_config_from_env() {
        let json = r#"{
            "project_id": "env-project",
            "client_email": "env@test.com",
            "private_key": "-----BEGIN PRIVATE KEY-----\ntest\n-----END PRIVATE KEY-----"
        }"#;

        std::env::set_var("ICN_FCM_SERVICE_ACCOUNT_JSON", json);

        let config = ProcessorConfig::from_env();

        assert!(config.fcm_config.is_some());
        let fcm = config.fcm_config.unwrap();
        assert_eq!(fcm.project_id, "env-project");
        assert_eq!(fcm.service_account_email, "env@test.com");

        std::env::remove_var("ICN_FCM_SERVICE_ACCOUNT_JSON");
    }
}
