use serde::{Serialize, Deserialize};
use serde_json::Value;
use reqwest::Client as HttpClient;
use crate::error::{AgentResult, AgentError};
use wallet_core::identity::IdentityWallet;
use wallet_core::credential::VerifiableCredential;
use std::collections::HashMap;

const DEFAULT_AGORANET_URL: &str = "https://agoranet.icn.network/api";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadSummary {
    pub id: String,
    pub title: String,
    pub proposal_id: Option<String>,
    pub topic: String,
    pub author: String,
    pub created_at: String,
    pub post_count: usize,
    pub credential_links: Vec<CredentialLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadDetail {
    pub id: String,
    pub title: String,
    pub proposal_id: Option<String>,
    pub topic: String,
    pub author: String,
    pub created_at: String,
    pub posts: Vec<Post>,
    pub credential_links: Vec<CredentialLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: String,
    pub thread_id: String,
    pub content: String,
    pub author: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialLink {
    pub id: String,
    pub thread_id: String,
    pub credential_id: String,
    pub credential_type: String,
    pub issuer: String,
    pub subject: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCredentialLinkRequest {
    pub thread_id: String,
    pub credential: VerifiableCredential,
}

pub struct AgoraNetClient {
    base_url: String,
    http_client: HttpClient,
    identity: IdentityWallet,
}

impl AgoraNetClient {
    pub fn new(identity: IdentityWallet, base_url: Option<String>) -> Self {
        Self {
            base_url: base_url.unwrap_or_else(|| DEFAULT_AGORANET_URL.to_string()),
            http_client: HttpClient::new(),
            identity,
        }
    }
    
    /// Fetch threads from AgoraNet
    pub async fn get_threads(&self, proposal_id: Option<&str>, topic: Option<&str>) -> AgentResult<Vec<ThreadSummary>> {
        let mut query_params = HashMap::new();
        
        if let Some(pid) = proposal_id {
            query_params.insert("proposal_id", pid);
        }
        
        if let Some(t) = topic {
            query_params.insert("topic", t);
        }
        
        let url = format!("{}/threads", self.base_url);
        
        let response = self.http_client.get(&url)
            .query(&query_params)
            .header("Authorization", format!("DID {}", self.identity.did))
            .send().await
            .map_err(|e| AgentError::GovernanceError(format!("Failed to connect to AgoraNet: {}", e)))?;
            
        if !response.status().is_success() {
            return Err(AgentError::GovernanceError(format!(
                "AgoraNet returned error: {}", response.status()
            )));
        }
        
        let threads: Vec<ThreadSummary> = response.json().await
            .map_err(|e| AgentError::SerializationError(format!("Failed to parse threads: {}", e)))?;
            
        Ok(threads)
    }
    
    /// Fetch a specific thread by ID
    pub async fn get_thread(&self, thread_id: &str) -> AgentResult<ThreadDetail> {
        let url = format!("{}/threads/{}", self.base_url, thread_id);
        
        let response = self.http_client.get(&url)
            .header("Authorization", format!("DID {}", self.identity.did))
            .send().await
            .map_err(|e| AgentError::GovernanceError(format!("Failed to connect to AgoraNet: {}", e)))?;
            
        if !response.status().is_success() {
            return Err(AgentError::GovernanceError(format!(
                "AgoraNet returned error: {}", response.status()
            )));
        }
        
        let thread: ThreadDetail = response.json().await
            .map_err(|e| AgentError::SerializationError(format!("Failed to parse thread: {}", e)))?;
            
        Ok(thread)
    }
    
    /// Link a credential to a thread
    pub async fn link_credential(&self, thread_id: &str, credential: &VerifiableCredential) -> AgentResult<CredentialLink> {
        let url = format!("{}/threads/credential-link", self.base_url);
        
        // Sign the request with our identity
        let payload = serde_json::to_string(&CreateCredentialLinkRequest {
            thread_id: thread_id.to_string(),
            credential: credential.clone(),
        }).map_err(|e| AgentError::SerializationError(format!("Failed to serialize request: {}", e)))?;
        
        let signature = self.identity.sign_message(payload.as_bytes());
        let signature_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &signature);
        
        let response = self.http_client.post(&url)
            .header("Authorization", format!("DID {}", self.identity.did))
            .header("X-Signature", signature_b64)
            .json(&CreateCredentialLinkRequest {
                thread_id: thread_id.to_string(),
                credential: credential.clone(),
            })
            .send().await
            .map_err(|e| AgentError::GovernanceError(format!("Failed to connect to AgoraNet: {}", e)))?;
            
        if !response.status().is_success() {
            return Err(AgentError::GovernanceError(format!(
                "AgoraNet returned error: {}", response.status()
            )));
        }
        
        let credential_link: CredentialLink = response.json().await
            .map_err(|e| AgentError::SerializationError(format!("Failed to parse credential link: {}", e)))?;
            
        Ok(credential_link)
    }
    
    /// Get credential links for a thread
    pub async fn get_credential_links(&self, thread_id: &str) -> AgentResult<Vec<CredentialLink>> {
        let url = format!("{}/threads/{}/credential-links", self.base_url, thread_id);
        
        let response = self.http_client.get(&url)
            .header("Authorization", format!("DID {}", self.identity.did))
            .send().await
            .map_err(|e| AgentError::GovernanceError(format!("Failed to connect to AgoraNet: {}", e)))?;
            
        if !response.status().is_success() {
            return Err(AgentError::GovernanceError(format!(
                "AgoraNet returned error: {}", response.status()
            )));
        }
        
        let links: Vec<CredentialLink> = response.json().await
            .map_err(|e| AgentError::SerializationError(format!("Failed to parse credential links: {}", e)))?;
            
        Ok(links)
    }
    
    /// Notify AgoraNet about a proposal event
    pub async fn notify_proposal_event(&self, proposal_id: &str, event_type: &str, details: Value) -> AgentResult<()> {
        let url = format!("{}/proposals/{}/events", self.base_url, proposal_id);
        
        let payload = serde_json::json!({
            "event_type": event_type,
            "details": details,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        
        let response = self.http_client.post(&url)
            .header("Authorization", format!("DID {}", self.identity.did))
            .json(&payload)
            .send().await
            .map_err(|e| AgentError::GovernanceError(format!("Failed to connect to AgoraNet: {}", e)))?;
            
        if !response.status().is_success() {
            return Err(AgentError::GovernanceError(format!(
                "AgoraNet returned error: {}", response.status()
            )));
        }
        
        Ok(())
    }
} 