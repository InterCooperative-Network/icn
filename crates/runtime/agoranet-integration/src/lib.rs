/*!
# AgoraNet Integration

This crate provides integration between the ICN Runtime and AgoraNet,
allowing deliberation discussions to be linked with runtime events and credentials.
*/

use thiserror::Error;
use cid::Cid;
use serde::{Serialize, Deserialize};
use icn_identity::VerifiableCredential;
use icn_governance_kernel::events::GovernanceEvent;
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, error, info};

/// Errors that can occur during AgoraNet integration
#[derive(Error, Debug)]
pub enum AgoraNetError {
    #[error("Integration error: {0}")]
    IntegrationError(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("Authentication error: {0}")]
    AuthenticationError(String),
    
    #[error("Validation error: {0}")]
    ValidationError(String),
}

/// Result type for AgoraNet integration operations
pub type AgoraNetResult<T> = Result<T, AgoraNetError>;

/// A link between an AgoraNet discussion and a Runtime element
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgoraNetLink {
    /// ID of the AgoraNet discussion or forum
    pub agoranet_id: String,
    
    /// The type of Runtime element this discussion is linked to
    pub link_type: AgoraNetLinkType,
    
    /// The ID of the linked Runtime element (as a string)
    pub runtime_id: String,
    
    /// Additional metadata for the link
    pub metadata: Option<serde_json::Value>,
}

/// Types of AgoraNet-Runtime links
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgoraNetLinkType {
    /// Linked to a governance proposal
    Proposal,
    
    /// Linked to an event
    Event,
    
    /// Linked to a credential
    Credential,
    
    /// Linked to a mandate
    Mandate,
    
    /// Linked to a TrustBundle
    TrustBundle,
    
    /// Other link type
    Other(String),
}

/// A provider for AgoraNet integration services
#[async_trait::async_trait]
pub trait AgoraNetProvider: Send + Sync {
    /// Create a link between an AgoraNet discussion and a Runtime proposal
    async fn link_proposal(&self, agoranet_id: &str, proposal_cid: &Cid, metadata: Option<serde_json::Value>) -> AgoraNetResult<AgoraNetLink>;
    
    /// Create a link between an AgoraNet discussion and a Runtime event
    async fn link_event(&self, agoranet_id: &str, event: &GovernanceEvent) -> AgoraNetResult<AgoraNetLink>;
    
    /// Create a link between an AgoraNet discussion and a Runtime credential
    async fn link_credential(&self, agoranet_id: &str, credential: &VerifiableCredential) -> AgoraNetResult<AgoraNetLink>;
    
    /// Get all links for a specific AgoraNet discussion
    async fn get_links_for_discussion(&self, agoranet_id: &str) -> AgoraNetResult<Vec<AgoraNetLink>>;
    
    /// Get all links for a specific Runtime element
    async fn get_links_for_runtime_element(&self, runtime_id: &str, link_type: AgoraNetLinkType) -> AgoraNetResult<Vec<AgoraNetLink>>;
    
    /// Submit new governance events to AgoraNet
    async fn publish_event(&self, event: &GovernanceEvent) -> AgoraNetResult<String>;
    
    /// Submit verifiable credentials to AgoraNet
    async fn publish_credential(&self, credential: &VerifiableCredential) -> AgoraNetResult<String>;
}

/// Basic in-memory implementation of AgoraNetProvider for testing and development
pub struct InMemoryAgoraNetProvider {
    links: HashMap<String, Vec<AgoraNetLink>>,
    runtime_links: HashMap<String, Vec<AgoraNetLink>>,
}

impl InMemoryAgoraNetProvider {
    /// Create a new in-memory AgoraNet provider
    pub fn new() -> Self {
        Self {
            links: HashMap::new(),
            runtime_links: HashMap::new(),
        }
    }
}

#[async_trait::async_trait]
impl AgoraNetProvider for InMemoryAgoraNetProvider {
    async fn link_proposal(&self, agoranet_id: &str, proposal_cid: &Cid, metadata: Option<serde_json::Value>) -> AgoraNetResult<AgoraNetLink> {
        let link = AgoraNetLink {
            agoranet_id: agoranet_id.to_string(),
            link_type: AgoraNetLinkType::Proposal,
            runtime_id: proposal_cid.to_string(),
            metadata,
        };
        
        // Add to local storage - in a real implementation, this would be persisted
        let mut links = self.links.clone();
        links.entry(agoranet_id.to_string())
            .or_insert_with(Vec::new)
            .push(link.clone());
        
        let mut runtime_links = self.runtime_links.clone();
        runtime_links.entry(proposal_cid.to_string())
            .or_insert_with(Vec::new)
            .push(link.clone());
        
        Ok(link)
    }
    
    async fn link_event(&self, agoranet_id: &str, event: &GovernanceEvent) -> AgoraNetResult<AgoraNetLink> {
        let event_id = event.id.to_string();
        
        let link = AgoraNetLink {
            agoranet_id: agoranet_id.to_string(),
            link_type: AgoraNetLinkType::Event,
            runtime_id: event_id.clone(),
            metadata: Some(serde_json::to_value(event).map_err(|e| AgoraNetError::SerializationError(e.to_string()))?),
        };
        
        // Add to local storage - in a real implementation, this would be persisted
        let mut links = self.links.clone();
        links.entry(agoranet_id.to_string())
            .or_insert_with(Vec::new)
            .push(link.clone());
        
        let mut runtime_links = self.runtime_links.clone();
        runtime_links.entry(event_id)
            .or_insert_with(Vec::new)
            .push(link.clone());
        
        Ok(link)
    }
    
    async fn link_credential(&self, agoranet_id: &str, credential: &VerifiableCredential) -> AgoraNetResult<AgoraNetLink> {
        let credential_id = credential.id.clone();
        
        let link = AgoraNetLink {
            agoranet_id: agoranet_id.to_string(),
            link_type: AgoraNetLinkType::Credential,
            runtime_id: credential_id.clone(),
            metadata: Some(serde_json::to_value(credential).map_err(|e| AgoraNetError::SerializationError(e.to_string()))?),
        };
        
        // Add to local storage - in a real implementation, this would be persisted
        let mut links = self.links.clone();
        links.entry(agoranet_id.to_string())
            .or_insert_with(Vec::new)
            .push(link.clone());
        
        let mut runtime_links = self.runtime_links.clone();
        runtime_links.entry(credential_id)
            .or_insert_with(Vec::new)
            .push(link.clone());
        
        Ok(link)
    }
    
    async fn get_links_for_discussion(&self, agoranet_id: &str) -> AgoraNetResult<Vec<AgoraNetLink>> {
        Ok(self.links.get(agoranet_id).cloned().unwrap_or_default())
    }
    
    async fn get_links_for_runtime_element(&self, runtime_id: &str, link_type: AgoraNetLinkType) -> AgoraNetResult<Vec<AgoraNetLink>> {
        let links = self.runtime_links.get(runtime_id).cloned().unwrap_or_default();
        Ok(links.into_iter().filter(|link| link.link_type == link_type).collect())
    }
    
    async fn publish_event(&self, _event: &GovernanceEvent) -> AgoraNetResult<String> {
        // In a real implementation, this would make an API call to AgoraNet
        Ok("event_published".to_string())
    }
    
    async fn publish_credential(&self, _credential: &VerifiableCredential) -> AgoraNetResult<String> {
        // In a real implementation, this would make an API call to AgoraNet
        Ok("credential_published".to_string())
    }
}

/// HTTP-based implementation of AgoraNetProvider that communicates with AgoraNet via webhooks
pub struct HttpAgoraNetProvider {
    /// The base URL for the AgoraNet webhook endpoint
    webhook_url: String,
    /// HTTP client for making requests
    client: reqwest::Client,
}

impl HttpAgoraNetProvider {
    /// Create a new HTTP-based AgoraNet provider
    pub fn new(webhook_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();
            
        Self {
            webhook_url,
            client,
        }
    }
    
    /// Create a new HTTP-based AgoraNet provider with a custom client
    pub fn with_client(webhook_url: String, client: reqwest::Client) -> Self {
        Self {
            webhook_url,
            client,
        }
    }
}

#[async_trait::async_trait]
impl AgoraNetProvider for HttpAgoraNetProvider {
    async fn link_proposal(&self, agoranet_id: &str, proposal_cid: &Cid, metadata: Option<serde_json::Value>) -> AgoraNetResult<AgoraNetLink> {
        // Create the link
        let link = AgoraNetLink {
            agoranet_id: agoranet_id.to_string(),
            link_type: AgoraNetLinkType::Proposal,
            runtime_id: proposal_cid.to_string(),
            metadata,
        };
        
        // For a full implementation, we might want to POST this link to the webhook
        // but for simplicity, we'll just return the link for now
        Ok(link)
    }
    
    async fn link_event(&self, agoranet_id: &str, event: &GovernanceEvent) -> AgoraNetResult<AgoraNetLink> {
        let event_id = event.id.to_string();
        
        // Create the link
        let link = AgoraNetLink {
            agoranet_id: agoranet_id.to_string(),
            link_type: AgoraNetLinkType::Event,
            runtime_id: event_id.clone(),
            metadata: Some(serde_json::to_value(event).map_err(|e| AgoraNetError::SerializationError(e.to_string()))?),
        };
        
        // For a full implementation, we might want to POST this link to the webhook
        // but for simplicity, we'll just return the link for now
        Ok(link)
    }
    
    async fn link_credential(&self, agoranet_id: &str, credential: &VerifiableCredential) -> AgoraNetResult<AgoraNetLink> {
        let credential_id = credential.id.clone();
        
        // Create the link
        let link = AgoraNetLink {
            agoranet_id: agoranet_id.to_string(),
            link_type: AgoraNetLinkType::Credential,
            runtime_id: credential_id.clone(),
            metadata: Some(serde_json::to_value(credential).map_err(|e| AgoraNetError::SerializationError(e.to_string()))?),
        };
        
        // For a full implementation, we might want to POST this link to the webhook
        // but for simplicity, we'll just return the link for now
        Ok(link)
    }
    
    async fn get_links_for_discussion(&self, _agoranet_id: &str) -> AgoraNetResult<Vec<AgoraNetLink>> {
        // In a real implementation, this would make a GET request to the AgoraNet API
        // For now, return an empty list
        Ok(Vec::new())
    }
    
    async fn get_links_for_runtime_element(&self, _runtime_id: &str, _link_type: AgoraNetLinkType) -> AgoraNetResult<Vec<AgoraNetLink>> {
        // In a real implementation, this would make a GET request to the AgoraNet API
        // For now, return an empty list
        Ok(Vec::new())
    }
    
    async fn publish_event(&self, event: &GovernanceEvent) -> AgoraNetResult<String> {
        // Create the URL for the event webhook endpoint
        let url = format!("{}/events", self.webhook_url.trim_end_matches('/'));
        
        debug!("Publishing event to AgoraNet: {}", url);
        
        // Serialize the event to JSON
        let event_json = serde_json::to_value(event)
            .map_err(|e| AgoraNetError::SerializationError(format!("Failed to serialize event: {}", e)))?;
        
        // Send the POST request to the webhook
        let response = self.client.post(&url)
            .json(&event_json)
            .header("Content-Type", "application/json")
            .send()
            .await
            .map_err(|e| AgoraNetError::NetworkError(format!("Failed to send event to AgoraNet: {}", e)))?;
        
        // Check response status
        let status = response.status();
        if status.is_success() {
            // Parse response JSON to get an ID or confirmation
            let response_body = response.text().await
                .map_err(|e| AgoraNetError::NetworkError(format!("Failed to read AgoraNet response: {}", e)))?;
            
            info!("Successfully published event to AgoraNet: {}", response_body);
            
            // Try to parse a response ID, or just use the response text
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&response_body) {
                if let Some(id) = json.get("id").and_then(|v| v.as_str()) {
                    return Ok(id.to_string());
                }
            }
            
            Ok(response_body)
        } else {
            // Handle error response
            let error_body = response.text().await
                .map_err(|e| AgoraNetError::NetworkError(format!("Failed to read AgoraNet error response: {}", e)))?;
            
            error!("Failed to publish event to AgoraNet: {} - {}", status, error_body);
            
            Err(AgoraNetError::IntegrationError(format!(
                "AgoraNet returned error status {}: {}",
                status,
                error_body
            )))
        }
    }
    
    async fn publish_credential(&self, credential: &VerifiableCredential) -> AgoraNetResult<String> {
        // Create the URL for the credential webhook endpoint
        let url = format!("{}/credentials", self.webhook_url.trim_end_matches('/'));
        
        debug!("Publishing credential to AgoraNet: {}", url);
        
        // Serialize the credential to JSON
        let credential_json = serde_json::to_value(credential)
            .map_err(|e| AgoraNetError::SerializationError(format!("Failed to serialize credential: {}", e)))?;
        
        // Send the POST request to the webhook
        let response = self.client.post(&url)
            .json(&credential_json)
            .header("Content-Type", "application/json")
            .send()
            .await
            .map_err(|e| AgoraNetError::NetworkError(format!("Failed to send credential to AgoraNet: {}", e)))?;
        
        // Check response status
        let status = response.status();
        if status.is_success() {
            // Parse response JSON to get an ID or confirmation
            let response_body = response.text().await
                .map_err(|e| AgoraNetError::NetworkError(format!("Failed to read AgoraNet response: {}", e)))?;
            
            info!("Successfully published credential to AgoraNet: {}", response_body);
            
            // Try to parse a response ID, or just use the response text
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&response_body) {
                if let Some(id) = json.get("id").and_then(|v| v.as_str()) {
                    return Ok(id.to_string());
                }
            }
            
            Ok(response_body)
        } else {
            // Handle error response
            let error_body = response.text().await
                .map_err(|e| AgoraNetError::NetworkError(format!("Failed to read AgoraNet error response: {}", e)))?;
            
            error!("Failed to publish credential to AgoraNet: {} - {}", status, error_body);
            
            Err(AgoraNetError::IntegrationError(format!(
                "AgoraNet returned error status {}: {}",
                status,
                error_body
            )))
        }
    }
}

/// Webhook handler for receiving AgoraNet notifications
pub struct AgoraNetWebhookHandler {
    provider: Box<dyn AgoraNetProvider>,
}

impl AgoraNetWebhookHandler {
    /// Create a new webhook handler
    pub fn new(provider: Box<dyn AgoraNetProvider>) -> Self {
        Self {
            provider,
        }
    }
    
    /// Handle an incoming webhook notification from AgoraNet
    pub async fn handle_webhook(&self, payload: serde_json::Value) -> AgoraNetResult<()> {
        // Extract the notification type
        let notification_type = payload.get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AgoraNetError::ValidationError("Missing notification type".to_string()))?;
        
        match notification_type {
            "discussion_created" => {
                // Handle new discussion creation
                // Extract discussion ID and other metadata
                let discussion_id = payload.get("discussionId")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AgoraNetError::ValidationError("Missing discussionId".to_string()))?;
                
                // Check if this discussion references a proposal
                if let Some(proposal_cid_str) = payload.get("proposalCid").and_then(|v| v.as_str()) {
                    // Parse the CID
                    let proposal_cid = Cid::try_from(proposal_cid_str)
                        .map_err(|e| AgoraNetError::ValidationError(format!("Invalid CID: {}", e)))?;
                    
                    // Create a link
                    self.provider.link_proposal(discussion_id, &proposal_cid, Some(payload.clone())).await?;
                }
                
                Ok(())
            },
            "discussion_updated" => {
                // Handle discussion update
                Ok(())
            },
            "vote_recorded" => {
                // Handle vote recording
                Ok(())
            },
            _ => {
                // Unknown notification type
                Err(AgoraNetError::ValidationError(format!("Unknown notification type: {}", notification_type)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_governance_kernel::events::{GovernanceEventType, GovernanceEvent, EventStatus};
    use icn_identity::{IdentityId, IdentityScope};
    
    #[tokio::test]
    async fn test_in_memory_provider() {
        let provider = InMemoryAgoraNetProvider::new();
        
        // Test event publishing
        let event = create_test_event();
        let result = provider.publish_event(&event).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "event_published");
        
        // Test credential publishing
        let credential = create_test_credential();
        let result = provider.publish_credential(&credential).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "credential_published");
    }
    
    // Test the HttpAgoraNetProvider without using actual HTTP requests
    #[tokio::test]
    async fn test_http_provider_construction() {
        // Create a provider with a fake URL
        let provider = HttpAgoraNetProvider::new("http://example.com/webhook".to_string());
        
        // Verify it was constructed correctly
        assert_eq!(provider.webhook_url, "http://example.com/webhook");
    }
    
    // Test the event publishing logic without making actual HTTP requests
    #[test]
    fn test_http_provider_event_publishing() {
        // We'll use a custom test that doesn't rely on tokio::test to avoid runtime conflicts
        // This is a test-only mock for HttpAgoraNetProvider.publish_event
        
        // Set up mock server
        let mut server = mockito::Server::new();
        
        // Create mock endpoint
        let mock = server.mock("POST", "/events")
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id":"event-123","status":"published"}"#)
            .create();
        
        // Create a runtime for this test only
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        
        // Run the test in the runtime
        rt.block_on(async {
            // Create provider with mock server URL
            let provider = HttpAgoraNetProvider::new(server.url());
            
            // Create test event
            let event = create_test_event();
            
            // Call the method to test
            let result = provider.publish_event(&event).await;
            
            // Verify success
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "event-123");
        });
        
        // Verify the mock was called
        mock.assert();
    }
    
    // Test the credential publishing logic
    #[test]
    fn test_http_provider_credential_publishing() {
        // Set up mock server
        let mut server = mockito::Server::new();
        
        // Create mock endpoint
        let mock = server.mock("POST", "/credentials")
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id":"credential-456","status":"published"}"#)
            .create();
        
        // Create a runtime for this test only
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        
        // Run the test in the runtime
        rt.block_on(async {
            // Create provider with mock server URL
            let provider = HttpAgoraNetProvider::new(server.url());
            
            // Create test credential
            let credential = create_test_credential();
            
            // Call the method to test
            let result = provider.publish_credential(&credential).await;
            
            // Verify success
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "credential-456");
        });
        
        // Verify the mock was called
        mock.assert();
    }
    
    // Test error handling
    #[test]
    fn test_http_provider_error_handling() {
        // Set up mock server
        let mut server = mockito::Server::new();
        
        // Create mock endpoint with error response
        let mock = server.mock("POST", "/events")
            .with_status(400)
            .with_header("content-type", "application/json")
            .with_body(r#"{"error":"Bad request","message":"Invalid event format"}"#)
            .create();
        
        // Create a runtime for this test only
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        
        // Run the test in the runtime
        rt.block_on(async {
            // Create provider with mock server URL
            let provider = HttpAgoraNetProvider::new(server.url());
            
            // Create test event
            let event = create_test_event();
            
            // Call the method to test
            let result = provider.publish_event(&event).await;
            
            // Verify error
            assert!(result.is_err());
            if let Err(AgoraNetError::IntegrationError(msg)) = result {
                assert!(msg.contains("400"));
                assert!(msg.contains("Invalid event format"));
            } else {
                panic!("Expected IntegrationError, got: {:?}", result);
            }
        });
        
        // Verify the mock was called
        mock.assert();
    }
    
    fn create_test_event() -> GovernanceEvent {
        GovernanceEvent {
            id: "test-event-id".to_string(),
            event_type: GovernanceEventType::ProposalCreated,
            timestamp: 1234567890,
            issuer: IdentityId("did:key:test-issuer".to_string()),
            scope: IdentityScope::Federation,
            organization: Some(IdentityId("did:key:test-organization".to_string())),
            proposal_cid: Some("test-proposal-cid".to_string()),
            status: EventStatus::Success,
            data: serde_json::json!({
                "title": "Test Proposal",
                "description": "This is a test proposal"
            }),
        }
    }
    
    fn create_test_credential() -> VerifiableCredential {
        VerifiableCredential {
            id: "test-credential-id".to_string(),
            credential_type: vec!["ProposalCreationCredential".to_string()],
            issuer: "did:key:test-issuer".to_string(),
            issuanceDate: "2023-01-01T12:00:00Z".to_string(),
            credentialSubject: serde_json::json!({
                "id": "did:key:test-subject",
                "proposalId": "test-proposal-cid",
                "action": "created"
            }),
            proof: None,
            expirationDate: None,
        }
    }
} 