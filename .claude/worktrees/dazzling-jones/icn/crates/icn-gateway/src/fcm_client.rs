//! Firebase Cloud Messaging (FCM) HTTP v1 API Client
//!
//! Implements push notification delivery via FCM HTTP v1 API.
//! Supports both Android and iOS devices through Firebase.

use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// FCM HTTP v1 API endpoint format
const FCM_V1_ENDPOINT: &str = "https://fcm.googleapis.com/v1/projects/{project_id}/messages:send";

/// OAuth2 token endpoint
const GOOGLE_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";

/// FCM scope for OAuth2
const FCM_SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";

/// FCM client configuration
#[derive(Debug, Clone)]
pub struct FcmConfig {
    /// Firebase project ID
    pub project_id: String,
    /// Service account email
    pub service_account_email: String,
    /// Private key for signing JWTs (PEM format)
    pub private_key: String,
    /// HTTP client timeout
    pub timeout: Duration,
}

impl FcmConfig {
    /// Create config from service account JSON
    pub fn from_service_account_json(json: &str) -> Result<Self, String> {
        let account: ServiceAccountJson =
            serde_json::from_str(json).map_err(|e| format!("Invalid service account JSON: {e}"))?;

        Ok(Self {
            project_id: account.project_id,
            service_account_email: account.client_email,
            private_key: account.private_key,
            timeout: Duration::from_secs(30),
        })
    }
}

/// Service account JSON structure
#[derive(Debug, Deserialize)]
struct ServiceAccountJson {
    project_id: String,
    client_email: String,
    private_key: String,
}

/// OAuth2 access token with expiry
#[derive(Debug)]
struct AccessToken {
    token: String,
    expires_at: u64,
}

impl AccessToken {
    fn is_expired(&self) -> bool {
        let now = icn_time::current_timestamp_secs();
        // Consider expired 60 seconds before actual expiry
        now >= self.expires_at.saturating_sub(60)
    }
}

/// FCM message payload
#[derive(Debug, Clone, Serialize)]
pub struct FcmMessage {
    /// Wrapper for FCM API
    pub message: FcmMessageContent,
}

/// FCM message content
#[derive(Debug, Clone, Serialize)]
pub struct FcmMessageContent {
    /// Device token
    pub token: String,
    /// Notification payload
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification: Option<FcmNotification>,
    /// Data payload
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Map<String, serde_json::Value>>,
    /// Android-specific config
    #[serde(skip_serializing_if = "Option::is_none")]
    pub android: Option<AndroidConfig>,
    /// iOS-specific config (APNS)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apns: Option<ApnsConfig>,
}

/// FCM notification payload
#[derive(Debug, Clone, Serialize)]
pub struct FcmNotification {
    /// Notification title
    pub title: String,
    /// Notification body
    pub body: String,
    /// Image URL (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

/// Android-specific configuration
#[derive(Debug, Clone, Serialize)]
pub struct AndroidConfig {
    /// Notification priority
    pub priority: String,
    /// Notification settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification: Option<AndroidNotification>,
}

/// Android notification settings
#[derive(Debug, Clone, Serialize)]
pub struct AndroidNotification {
    /// Channel ID for notification
    pub channel_id: String,
    /// Notification icon
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Notification color
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// iOS-specific configuration (APNS)
#[derive(Debug, Clone, Serialize)]
pub struct ApnsConfig {
    /// APNS headers
    pub headers: ApnsHeaders,
    /// APNS payload
    pub payload: ApnsPayload,
}

/// APNS headers
#[derive(Debug, Clone, Serialize)]
pub struct ApnsHeaders {
    /// Push type
    #[serde(rename = "apns-push-type")]
    pub push_type: String,
    /// Priority
    #[serde(rename = "apns-priority")]
    pub priority: String,
}

/// APNS payload
#[derive(Debug, Clone, Serialize)]
pub struct ApnsPayload {
    /// APS data
    pub aps: ApnsAps,
}

/// APNS APS settings
#[derive(Debug, Clone, Serialize)]
pub struct ApnsAps {
    /// Alert configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alert: Option<ApnsAlert>,
    /// Sound
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>,
    /// Badge count
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge: Option<i32>,
    /// Content available flag for background updates
    #[serde(rename = "content-available", skip_serializing_if = "Option::is_none")]
    pub content_available: Option<i32>,
}

/// APNS alert
#[derive(Debug, Clone, Serialize)]
pub struct ApnsAlert {
    /// Title
    pub title: String,
    /// Body
    pub body: String,
}

/// FCM API response
#[derive(Debug, Deserialize)]
pub struct FcmResponse {
    /// Message name (ID)
    pub name: Option<String>,
    /// Error details
    pub error: Option<FcmError>,
}

/// FCM error
#[derive(Debug, Deserialize)]
pub struct FcmError {
    /// Error code
    pub code: i32,
    /// Error message
    pub message: String,
    /// Error status
    pub status: String,
    /// Error details
    pub details: Option<Vec<serde_json::Value>>,
}

/// FCM delivery result
#[derive(Debug)]
pub enum FcmResult {
    /// Successfully sent
    Success { message_id: String },
    /// Device token is invalid/unregistered - should be removed
    InvalidToken,
    /// Temporary failure - should retry
    TemporaryFailure { error: String },
    /// Permanent failure - should not retry
    PermanentFailure { error: String },
}

/// FCM client for sending push notifications
pub struct FcmClient {
    config: FcmConfig,
    http_client: reqwest::Client,
    access_token: Arc<RwLock<Option<AccessToken>>>,
}

impl FcmClient {
    /// Create a new FCM client
    pub fn new(config: FcmConfig) -> Result<Self, String> {
        let http_client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

        Ok(Self {
            config,
            http_client,
            access_token: Arc::new(RwLock::new(None)),
        })
    }

    /// Send a notification to a device
    pub async fn send(&self, message: FcmMessage) -> FcmResult {
        // Get or refresh access token
        let token = match self.get_access_token().await {
            Ok(t) => t,
            Err(e) => {
                return FcmResult::TemporaryFailure {
                    error: format!("Failed to get access token: {e}"),
                }
            }
        };

        let endpoint = FCM_V1_ENDPOINT.replace("{project_id}", &self.config.project_id);

        let response = match self
            .http_client
            .post(&endpoint)
            .header("Authorization", format!("Bearer {token}"))
            .json(&message)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return FcmResult::TemporaryFailure {
                    error: format!("HTTP request failed: {e}"),
                }
            }
        };

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if status.is_success() {
            // Parse success response
            if let Ok(resp) = serde_json::from_str::<FcmResponse>(&body) {
                if let Some(name) = resp.name {
                    return FcmResult::Success { message_id: name };
                }
            }
            FcmResult::Success {
                message_id: "unknown".to_string(),
            }
        } else {
            // Parse error response (used for detailed error handling)

            match status.as_u16() {
                400 => {
                    // Bad request - check if it's an invalid token
                    if body.contains("UNREGISTERED") || body.contains("INVALID_ARGUMENT") {
                        FcmResult::InvalidToken
                    } else {
                        FcmResult::PermanentFailure {
                            error: format!("Bad request: {body}"),
                        }
                    }
                }
                401 | 403 => FcmResult::TemporaryFailure {
                    error: format!("Auth error ({}): {body}", status.as_u16()),
                },
                404 => FcmResult::InvalidToken,
                429 => FcmResult::TemporaryFailure {
                    error: "Rate limited".to_string(),
                },
                500..=599 => FcmResult::TemporaryFailure {
                    error: format!("Server error ({}): {body}", status.as_u16()),
                },
                _ => FcmResult::PermanentFailure {
                    error: format!("Unexpected error ({}): {body}", status.as_u16()),
                },
            }
        }
    }

    /// Get or refresh the OAuth2 access token
    async fn get_access_token(&self) -> Result<String, String> {
        // Check if we have a valid token
        {
            let token_guard = self.access_token.read().await;
            if let Some(ref token) = *token_guard {
                if !token.is_expired() {
                    return Ok(token.token.clone());
                }
            }
        }

        // Need to refresh the token
        let new_token = self.refresh_access_token().await?;
        let token_string = new_token.token.clone();

        // Store the new token
        {
            let mut token_guard = self.access_token.write().await;
            *token_guard = Some(new_token);
        }

        Ok(token_string)
    }

    /// Refresh the OAuth2 access token using JWT assertion
    async fn refresh_access_token(&self) -> Result<AccessToken, String> {
        let now = icn_time::current_timestamp_secs();

        // Create JWT claims
        let claims = JwtClaims {
            iss: self.config.service_account_email.clone(),
            scope: FCM_SCOPE.to_string(),
            aud: GOOGLE_TOKEN_ENDPOINT.to_string(),
            iat: now,
            exp: now + 3600, // 1 hour
        };

        // Sign the JWT
        let jwt = self.sign_jwt(&claims)?;

        // Exchange JWT for access token (reqwest 0.13+ requires Serialize for form())
        let mut params = HashMap::new();
        params.insert("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer");
        params.insert("assertion", jwt.as_str());

        let response = self
            .http_client
            .post(GOOGLE_TOKEN_ENDPOINT)
            .form(&params)
            .send()
            .await
            .map_err(|e| format!("Token request failed: {e}"))?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Token request failed: {body}"));
        }

        let token_resp: TokenResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse token response: {e}"))?;

        Ok(AccessToken {
            token: token_resp.access_token,
            expires_at: now + token_resp.expires_in,
        })
    }

    /// Sign a JWT using RS256
    fn sign_jwt(&self, claims: &JwtClaims) -> Result<String, String> {
        // Parse the private key (PKCS#8 PEM format from Google service account)
        let encoding_key = EncodingKey::from_rsa_pem(self.config.private_key.as_bytes())
            .map_err(|e| format!("Failed to parse private key: {e}"))?;

        // Create JWT header with RS256 algorithm
        let header = Header::new(Algorithm::RS256);

        // Encode the JWT
        encode(&header, claims, &encoding_key).map_err(|e| format!("Failed to sign JWT: {e}"))
    }

    /// Create FCM client with a pre-existing access token (for testing/development)
    pub fn with_access_token(project_id: String, access_token: String) -> Result<Self, String> {
        let config = FcmConfig {
            project_id,
            service_account_email: String::new(),
            private_key: String::new(),
            timeout: Duration::from_secs(30),
        };

        let http_client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

        let token = AccessToken {
            token: access_token,
            // Token expires in 1 hour (3600 seconds)
            expires_at: icn_time::current_timestamp_secs() + 3600,
        };

        Ok(Self {
            config,
            http_client,
            access_token: Arc::new(RwLock::new(Some(token))),
        })
    }
}

/// JWT claims for OAuth2 token request
#[derive(Debug, Serialize)]
struct JwtClaims {
    iss: String,
    scope: String,
    aud: String,
    iat: u64,
    exp: u64,
}

/// OAuth2 token response
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    #[allow(dead_code)]
    token_type: String,
}

/// Helper to create a notification message for a device
pub fn create_notification_message(
    device_token: String,
    title: String,
    body: String,
    data: Option<serde_json::Value>,
    is_high_priority: bool,
) -> FcmMessage {
    // Convert data to map if provided
    let data_map = data.and_then(|v| {
        if let serde_json::Value::Object(map) = v {
            // Convert all values to strings for FCM data payload
            let string_map: serde_json::Map<String, serde_json::Value> = map
                .into_iter()
                .map(|(k, v)| {
                    let v_str = match v {
                        serde_json::Value::String(s) => serde_json::Value::String(s),
                        other => serde_json::Value::String(other.to_string()),
                    };
                    (k, v_str)
                })
                .collect();
            Some(string_map)
        } else {
            None
        }
    });

    let priority = if is_high_priority { "high" } else { "normal" };

    FcmMessage {
        message: FcmMessageContent {
            token: device_token,
            notification: Some(FcmNotification {
                title,
                body,
                image: None,
            }),
            data: data_map,
            android: Some(AndroidConfig {
                priority: priority.to_string(),
                notification: Some(AndroidNotification {
                    channel_id: "icn_notifications".to_string(),
                    icon: None,
                    color: None,
                }),
            }),
            apns: Some(ApnsConfig {
                headers: ApnsHeaders {
                    push_type: "alert".to_string(),
                    priority: if is_high_priority { "10" } else { "5" }.to_string(),
                },
                payload: ApnsPayload {
                    aps: ApnsAps {
                        alert: None, // Using notification field instead
                        sound: Some("default".to_string()),
                        badge: None,
                        content_available: None,
                    },
                },
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_notification_message() {
        let msg = create_notification_message(
            "test-token".to_string(),
            "Test Title".to_string(),
            "Test Body".to_string(),
            Some(serde_json::json!({"key": "value"})),
            true,
        );

        assert_eq!(msg.message.token, "test-token");
        assert_eq!(
            msg.message.notification.as_ref().unwrap().title,
            "Test Title"
        );
        assert!(msg.message.data.is_some());
    }

    #[test]
    fn test_fcm_config_from_json() {
        let json = r#"{
            "project_id": "my-project",
            "client_email": "test@test.iam.gserviceaccount.com",
            "private_key": "-----BEGIN PRIVATE KEY-----\ntest\n-----END PRIVATE KEY-----"
        }"#;

        let config = FcmConfig::from_service_account_json(json).unwrap();
        assert_eq!(config.project_id, "my-project");
        assert_eq!(
            config.service_account_email,
            "test@test.iam.gserviceaccount.com"
        );
    }
}
