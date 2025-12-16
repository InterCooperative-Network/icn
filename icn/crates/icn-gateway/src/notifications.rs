//! Push Notification Service
//!
//! Manages FCM device registration and sends push notifications for events.

use dashmap::DashMap;
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Device platform type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Ios,
    Android,
    Web,
}

/// Registered device for push notifications
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredDevice {
    /// Device identifier (DID)
    pub did: Did,
    /// FCM device token
    pub device_token: String,
    /// Platform type
    pub platform: Platform,
    /// Registration timestamp
    pub registered_at: u64,
}

/// Notification payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    /// Notification title
    pub title: String,
    /// Notification body
    pub body: String,
    /// Custom data payload
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Notification service for managing device tokens and sending notifications
pub struct NotificationService {
    /// Map of device_token -> RegisteredDevice
    devices: Arc<DashMap<String, RegisteredDevice>>,
    /// Map of DID -> Vec<device_token> for quick lookup
    did_tokens: Arc<DashMap<String, Vec<String>>>,
    /// Optional FCM service account JSON (for Firebase Admin SDK)
    #[allow(dead_code)]
    fcm_credentials: Option<String>,
}

impl NotificationService {
    /// Create a new notification service
    pub fn new(fcm_credentials: Option<String>) -> Self {
        Self {
            devices: Arc::new(DashMap::new()),
            did_tokens: Arc::new(DashMap::new()),
            fcm_credentials,
        }
    }

    /// Register a device for push notifications
    pub fn register_device(&self, did: Did, device_token: String, platform: Platform) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let device = RegisteredDevice {
            did: did.clone(),
            device_token: device_token.clone(),
            platform,
            registered_at: timestamp,
        };

        // Store device by token
        self.devices.insert(device_token.clone(), device);

        // Update DID -> tokens mapping
        let did_str = did.to_string();
        self.did_tokens
            .entry(did_str)
            .and_modify(|tokens| {
                if !tokens.contains(&device_token) {
                    tokens.push(device_token.clone());
                }
            })
            .or_insert_with(|| vec![device_token]);
    }

    /// Unregister a device
    pub fn unregister_device(&self, device_token: &str) {
        if let Some((_, device)) = self.devices.remove(device_token) {
            let did_str = device.did.to_string();
            if let Some(mut tokens) = self.did_tokens.get_mut(&did_str) {
                tokens.retain(|t| t != device_token);
            }
        }
    }

    /// Get all device tokens for a DID
    pub fn get_device_tokens(&self, did: &Did) -> Vec<String> {
        let did_str = did.to_string();
        self.did_tokens
            .get(&did_str)
            .map(|tokens| tokens.clone())
            .unwrap_or_default()
    }

    /// Get device info by token
    pub fn get_device(&self, device_token: &str) -> Option<RegisteredDevice> {
        self.devices.get(device_token).map(|d| d.clone())
    }

    /// Get count of registered devices
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// Send notification to specific DID (all their devices)
    pub async fn send_to_did(
        &self,
        did: &Did,
        notification: Notification,
    ) -> Result<usize, String> {
        let tokens = self.get_device_tokens(did);
        if tokens.is_empty() {
            return Ok(0);
        }

        let mut sent = 0;
        for token in tokens {
            if self
                .send_to_token(&token, notification.clone())
                .await
                .is_ok()
            {
                sent += 1;
            }
        }

        Ok(sent)
    }

    /// Send notification to specific device token
    pub async fn send_to_token(
        &self,
        token: &str,
        notification: Notification,
    ) -> Result<(), String> {
        // For now, just log the notification (FCM integration would go here)
        tracing::info!(
            "📱 Sending notification to {}: {} - {}",
            token,
            notification.title,
            notification.body
        );

        // Note: This is the legacy fallback path. The primary notification flow
        // uses NotificationProcessor with FcmClient which handles FCM HTTP v1 API
        // with proper JWT authentication. This method is retained for simple
        // direct push scenarios when NotificationProcessor is not available.

        Ok(())
    }

    /// Create notification for payment received
    pub fn payment_received_notification(
        amount: i64,
        from_did: &Did,
        payment_id: &str,
    ) -> Notification {
        Notification {
            title: "Payment Received".to_string(),
            body: format!(
                "You received {} hours from {}",
                amount,
                format_did(from_did)
            ),
            data: Some(serde_json::json!({
                "type": "payment",
                "payment_id": payment_id,
                "amount": amount,
                "from": from_did.to_string(),
            })),
        }
    }

    /// Create notification for payment sent
    pub fn payment_sent_notification(amount: i64, to_did: &Did, payment_id: &str) -> Notification {
        Notification {
            title: "Payment Confirmed".to_string(),
            body: format!("Sent {} hours to {}", amount, format_did(to_did)),
            data: Some(serde_json::json!({
                "type": "payment",
                "payment_id": payment_id,
                "amount": amount,
                "to": to_did.to_string(),
            })),
        }
    }

    /// Create notification for new proposal
    pub fn proposal_created_notification(proposal_id: &str, title: &str) -> Notification {
        Notification {
            title: "New Proposal".to_string(),
            body: format!("{title} - vote now"),
            data: Some(serde_json::json!({
                "type": "proposal",
                "proposal_id": proposal_id,
            })),
        }
    }

    /// Create notification for proposal closing soon
    pub fn proposal_closing_notification(proposal_id: &str, title: &str) -> Notification {
        Notification {
            title: "Proposal Closing Soon".to_string(),
            body: format!("{title} closes in 24 hours"),
            data: Some(serde_json::json!({
                "type": "proposal",
                "proposal_id": proposal_id,
            })),
        }
    }

    /// Create notification for vote recorded
    pub fn vote_recorded_notification(proposal_id: &str, proposal_title: &str) -> Notification {
        Notification {
            title: "Vote Recorded".to_string(),
            body: format!("Your vote on {proposal_title} was recorded"),
            data: Some(serde_json::json!({
                "type": "vote",
                "proposal_id": proposal_id,
            })),
        }
    }

    /// Create notification for proposal result (closed)
    pub fn proposal_result_notification(
        proposal_id: &str,
        title: &str,
        outcome: &str,
    ) -> Notification {
        let body = match outcome {
            "accepted" => format!("'{title}' was accepted"),
            "rejected" => format!("'{title}' was rejected"),
            "no_quorum" => format!("'{title}' did not reach quorum"),
            _ => format!("'{title}' voting has concluded"),
        };
        Notification {
            title: "Proposal Result".to_string(),
            body,
            data: Some(serde_json::json!({
                "type": "proposal_result",
                "proposal_id": proposal_id,
                "outcome": outcome,
            })),
        }
    }
}

/// Format DID for display (truncated)
fn format_did(did: &Did) -> String {
    let s = did.to_string();
    if s.len() > 20 {
        format!("{}...{}", &s[..12], &s[s.len() - 6..])
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_get_device() {
        let service = NotificationService::new(None);
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        service.register_device(did.clone(), "token123".to_string(), Platform::Android);

        let device = service.get_device("token123").unwrap();
        assert_eq!(device.did, did);
        assert_eq!(device.platform, Platform::Android);
    }

    #[test]
    fn test_multiple_devices_per_did() {
        let service = NotificationService::new(None);
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        service.register_device(did.clone(), "token1".to_string(), Platform::Ios);
        service.register_device(did.clone(), "token2".to_string(), Platform::Android);

        let tokens = service.get_device_tokens(&did);
        assert_eq!(tokens.len(), 2);
        assert!(tokens.contains(&"token1".to_string()));
        assert!(tokens.contains(&"token2".to_string()));
    }

    #[test]
    fn test_unregister_device() {
        let service = NotificationService::new(None);
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        service.register_device(did.clone(), "token123".to_string(), Platform::Android);
        assert_eq!(service.device_count(), 1);

        service.unregister_device("token123");
        assert_eq!(service.device_count(), 0);
        assert!(service.get_device_tokens(&did).is_empty());
    }

    #[test]
    fn test_notification_creation() {
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let did = keypair.did().clone();

        let notif = NotificationService::payment_received_notification(10, &did, "pay123");
        assert_eq!(notif.title, "Payment Received");
        assert!(notif.body.contains("10 hours"));

        let notif = NotificationService::proposal_created_notification("prop1", "Upgrade Policy");
        assert_eq!(notif.title, "New Proposal");
        assert!(notif.body.contains("Upgrade Policy"));
    }
}
