use anyhow::Result;
use serde::{Deserialize, Serialize};
use sled::Db;
use std::time::{SystemTime, UNIX_EPOCH};

/// Device platform type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    /// Apple iOS device
    Ios,
    /// Android device
    Android,
    /// Web browser
    Web,
}

/// Registered device for push notifications
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RegisteredDevice {
    /// Device identifier (DID)
    pub did: String,
    /// FCM device token
    pub device_token: String,
    /// Platform type
    pub platform: Platform,
    /// Registration timestamp
    pub registered_at: u64,
}

/// In-app notification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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

/// Persistent store for notifications and devices
#[derive(Clone)]
pub struct NotificationStore {
    db: Db,
}

impl NotificationStore {
    /// Create a new notification store with the given database
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    // --- Device Management ---

    /// Register a device
    pub fn register_device(&self, did: &str, device_token: &str, platform: Platform) -> Result<()> {
        let timestamp = icn_time::current_timestamp_secs();

        let device = RegisteredDevice {
            did: did.to_string(),
            device_token: device_token.to_string(),
            platform,
            registered_at: timestamp,
        };

        // Store by token (primary key)
        let key = format!("device:{device_token}");
        self.db
            .insert(key.as_bytes(), serde_json::to_vec(&device)?)?;

        // Update DID index (idx_device_owner:{did}:{token})
        let idx_key = format!("idx_device_owner:{did}:{device_token}");
        self.db.insert(idx_key.as_bytes(), &[])?;

        Ok(())
    }

    /// Unregister a device
    pub fn unregister_device(&self, device_token: &str) -> Result<()> {
        let key = format!("device:{device_token}");
        if let Some(data) = self.db.remove(key.as_bytes())? {
            // Remove index if device existed
            if let Ok(device) = serde_json::from_slice::<RegisteredDevice>(&data) {
                let idx_key = format!("idx_device_owner:{}:{}", device.did, device_token);
                self.db.remove(idx_key.as_bytes())?;
            }
        }
        Ok(())
    }

    /// Get all device tokens for a DID
    pub fn get_device_tokens(&self, did: &str) -> Result<Vec<String>> {
        let prefix = format!("idx_device_owner:{did}:");
        let mut tokens = Vec::new();

        for item in self.db.scan_prefix(prefix.as_bytes()) {
            let (key, _) = item?;
            let key_str = std::str::from_utf8(&key)?;
            if let Some(token) = key_str.strip_prefix(&prefix) {
                tokens.push(token.to_string());
            }
        }

        Ok(tokens)
    }

    // --- In-App Notifications ---

    /// Add an in-app notification
    pub fn add_notification(&self, notification: InAppNotification) -> Result<()> {
        // Store by ID (notif:{id})
        let key = format!("notif:{}", notification.id);
        self.db
            .insert(key.as_bytes(), serde_json::to_vec(&notification)?)?;

        // Index by recipient (idx_notif_recipient:{recipient}:{created_at}:{id})
        // Using created_at in key enables chronological sorting
        let idx_key = format!(
            "idx_notif_recipient:{}:{}:{}",
            notification.recipient, notification.created_at, notification.id
        );
        self.db.insert(idx_key.as_bytes(), &[])?;

        Ok(())
    }

    /// Get notifications for a user
    pub fn get_notifications(
        &self,
        recipient: &str,
        unread_only: bool,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<(Vec<InAppNotification>, usize)> {
        // First get all matching keys
        let prefix = format!("idx_notif_recipient:{recipient}:");
        let mut notifications = Vec::new();

        // Scan in reverse to get newest first
        for item in self.db.scan_prefix(prefix.as_bytes()).rev() {
            let (key, _) = item?;
            let key_str = std::str::from_utf8(&key)?;
            // Extract key components to find ID
            // Format: idx_notif_recipient:{recipient}:{created_at}:{id}
            let parts: Vec<&str> = key_str.split(':').collect();
            if let Some(id) = parts.last() {
                if let Some(data) = self.db.get(format!("notif:{id}").as_bytes())? {
                    if let Ok(notif) = serde_json::from_slice::<InAppNotification>(&data) {
                        if !unread_only || !notif.read {
                            notifications.push(notif);
                        }
                    }
                }
            }
        }

        let total = notifications.len();
        let limit = limit.unwrap_or(50);
        let offset = offset.unwrap_or(0);

        let paged = notifications.into_iter().skip(offset).take(limit).collect();

        Ok((paged, total))
    }

    /// Get unread count
    pub fn get_unread_count(&self, recipient: &str) -> Result<usize> {
        let prefix = format!("idx_notif_recipient:{recipient}:");
        let mut count = 0;

        for item in self.db.scan_prefix(prefix.as_bytes()) {
            let (key, _) = item?;
            let key_str = std::str::from_utf8(&key)?;
            let parts: Vec<&str> = key_str.split(':').collect();
            if let Some(id) = parts.last() {
                if let Some(data) = self.db.get(format!("notif:{id}").as_bytes())? {
                    if let Ok(notif) = serde_json::from_slice::<InAppNotification>(&data) {
                        if !notif.read {
                            count += 1;
                        }
                    }
                }
            }
        }
        Ok(count)
    }

    /// Mark notification as read
    pub fn mark_read(&self, recipient: &str, notification_id: &str) -> Result<bool> {
        let key = format!("notif:{notification_id}");
        if let Some(data) = self.db.get(key.as_bytes())? {
            let mut notif: InAppNotification = serde_json::from_slice(&data)?;

            // Verify recipient matches
            if notif.recipient != recipient {
                return Ok(false);
            }

            if !notif.read {
                notif.read = true;
                notif.read_at = Some(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs());
                self.db
                    .insert(key.as_bytes(), serde_json::to_vec(&notif)?)?;
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Mark all as read
    pub fn mark_all_read(&self, recipient: &str) -> Result<usize> {
        let prefix = format!("idx_notif_recipient:{recipient}:");
        let mut count = 0;

        for item in self.db.scan_prefix(prefix.as_bytes()) {
            let (key, _) = item?;
            let key_str = std::str::from_utf8(&key)?;
            let parts: Vec<&str> = key_str.split(':').collect();
            if let Some(id) = parts.last() {
                let notif_key = format!("notif:{id}");
                if let Some(data) = self.db.get(notif_key.as_bytes())? {
                    if let Ok(mut notif) = serde_json::from_slice::<InAppNotification>(&data) {
                        if !notif.read {
                            notif.read = true;
                            notif.read_at =
                                Some(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs());
                            self.db
                                .insert(notif_key.as_bytes(), serde_json::to_vec(&notif)?)?;
                            count += 1;
                        }
                    }
                }
            }
        }
        Ok(count)
    }

    /// Delete notification
    pub fn delete_notification(&self, recipient: &str, notification_id: &str) -> Result<bool> {
        let key = format!("notif:{notification_id}");
        if let Some(data) = self.db.get(key.as_bytes())? {
            let notif: InAppNotification = serde_json::from_slice(&data)?;

            if notif.recipient != recipient {
                return Ok(false);
            }

            // Remove main record
            self.db.remove(key.as_bytes())?;

            // Remove index
            let idx_key = format!(
                "idx_notif_recipient:{}:{}:{}",
                notif.recipient, notif.created_at, notif.id
            );
            self.db.remove(idx_key.as_bytes())?;

            Ok(true)
        } else {
            Ok(false)
        }
    }

    // --- Delivery Logs ---

    /// Add a delivery log entry
    pub fn add_delivery_log(&self, entry: DeliveryLogEntry) -> Result<()> {
        // key: log:{notification_id}:{timestamp}
        // This allows multiple log entries per notification, ordered by time
        let key = format!("log:{}:{}", entry.notification_id, entry.timestamp);
        self.db
            .insert(key.as_bytes(), serde_json::to_vec(&entry)?)?;
        Ok(())
    }

    /// Get delivery logs for a notification
    pub fn get_delivery_logs(&self, notification_id: &str) -> Result<Vec<DeliveryLogEntry>> {
        let prefix = format!("log:{notification_id}:");
        let mut logs = Vec::new();

        for item in self.db.scan_prefix(prefix.as_bytes()) {
            let (_, value) = item?;
            if let Ok(entry) = serde_json::from_slice::<DeliveryLogEntry>(&value) {
                logs.push(entry);
            }
        }

        Ok(logs)
    }
}

/// Delivery log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DeliveryLogEntry {
    /// Notification ID
    pub notification_id: String,
    /// Channel (push, email, in_app)
    pub channel: String,
    /// Status (delivered, failed, abandoned)
    pub status: String,
    /// Timestamp
    pub timestamp: u64,
    /// Optional details/error message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sled::Config;

    fn temp_db() -> Db {
        Config::new().temporary(true).open().unwrap()
    }

    #[test]
    fn test_device_registration() -> Result<()> {
        let db = temp_db();
        let store = NotificationStore::new(db);
        let did = "did:icn:test";

        store.register_device(did, "token1", Platform::Android)?;
        store.register_device(did, "token2", Platform::Ios)?;

        let tokens = store.get_device_tokens(did)?;
        assert_eq!(tokens.len(), 2);
        assert!(tokens.contains(&"token1".to_string()));
        assert!(tokens.contains(&"token2".to_string()));

        store.unregister_device("token1")?;
        let tokens = store.get_device_tokens(did)?;
        assert_eq!(tokens.len(), 1);
        assert!(tokens.contains(&"token2".to_string()));

        Ok(())
    }

    #[test]
    fn test_in_app_notifications() -> Result<()> {
        let db = temp_db();
        let store = NotificationStore::new(db);
        let recipient = "did:icn:alice";

        // Add notification
        let notif = InAppNotification {
            id: "1".to_string(),
            recipient: recipient.to_string(),
            coop_id: "coop1".to_string(),
            title: "Test".to_string(),
            body: "Body".to_string(),
            data: None,
            notification_type: "System".to_string(),
            created_at: 1000,
            read: false,
            read_at: None,
        };
        store.add_notification(notif.clone())?;

        // Get unread count
        assert_eq!(store.get_unread_count(recipient)?, 1);

        // Get list
        let (list, total) = store.get_notifications(recipient, false, None, None)?;
        assert_eq!(total, 1);
        assert_eq!(list[0].id, "1");

        // Mark read
        assert!(store.mark_read(recipient, "1")?);
        assert_eq!(store.get_unread_count(recipient)?, 0);

        // Delete
        assert!(store.delete_notification(recipient, "1")?);
        let (list, _) = store.get_notifications(recipient, false, None, None)?;
        assert_eq!(list.len(), 0);

        Ok(())
    }

    #[test]
    fn test_delivery_logs() -> Result<()> {
        let db = temp_db();
        let store = NotificationStore::new(db);
        let id = "notif-123";

        // Add logs
        let entry1 = DeliveryLogEntry {
            notification_id: id.to_string(),
            channel: "email".to_string(),
            status: "pending".to_string(),
            timestamp: 1000,
            details: None,
        };
        store.add_delivery_log(entry1)?;

        let entry2 = DeliveryLogEntry {
            notification_id: id.to_string(),
            channel: "email".to_string(),
            status: "delivered".to_string(),
            timestamp: 1001,
            details: Some("Sent via SMTP".to_string()),
        };
        store.add_delivery_log(entry2)?;

        // Retrieve logs
        let logs = store.get_delivery_logs(id)?;
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].status, "pending");
        assert_eq!(logs[1].status, "delivered");
        assert_eq!(logs[1].details, Some("Sent via SMTP".to_string()));

        Ok(())
    }
}
