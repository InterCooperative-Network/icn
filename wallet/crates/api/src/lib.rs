/*!
 * ICN Wallet API
 *
 * Provides the API for wallet operations, including:
 * - Agent for submitting proposals and interacting with AgoraNet
 * - Integration with federation components
 * - Credential management and verification
 */

use reqwest::Client;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WalletAgentError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("AgoraNet error: {0}")]
    AgoraNet(String),

    #[error("Runtime error: {0}")]
    Runtime(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type for wallet agent operations
pub type WalletAgentResult<T> = Result<T, WalletAgentError>;

/// Client for the AgoraNet API
pub struct AgoraNetClient {
    client: Client,
    base_url: String,
}

impl AgoraNetClient {
    /// Create a new AgoraNet client
    pub fn new(base_url: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
        }
    }

    /// Link a thread to a proposal
    pub async fn link_thread_to_proposal(&self, thread_id: &str, proposal_id: &str) -> WalletAgentResult<()> {
        let url = format!("{}/threads/{}/link_proposal", self.base_url, thread_id);
        
        let response = self.client
            .post(&url)
            .json(&serde_json::json!({
                "proposal_cid": proposal_id
            }))
            .send()
            .await?;
            
        if !response.status().is_success() {
            return Err(WalletAgentError::AgoraNet(format!(
                "Failed to link thread to proposal: {}",
                response.status()
            )));
        }
        
        Ok(())
    }
}

/// Client for submitting proposals to the Runtime
pub struct RuntimeClient {
    client: Client,
    base_url: String,
}

impl RuntimeClient {
    /// Create a new Runtime client
    pub fn new(base_url: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
        }
    }

    /// Submit a proposal to the Runtime
    pub async fn submit_proposal(&self, ccl_content: &str, metadata: serde_json::Value) -> WalletAgentResult<String> {
        let url = format!("{}/proposals", self.base_url);
        
        let response = self.client
            .post(&url)
            .json(&serde_json::json!({
                "ccl": ccl_content,
                "metadata": metadata
            }))
            .send()
            .await?;
            
        if !response.status().is_success() {
            return Err(WalletAgentError::Runtime(format!(
                "Failed to submit proposal: {}",
                response.status()
            )));
        }
        
        // Parse the proposal ID from the response
        let proposal_response: serde_json::Value = response.json().await?;
        let proposal_id = proposal_response
            .get("proposal_id")
            .and_then(|id| id.as_str())
            .ok_or_else(|| WalletAgentError::Runtime("Missing proposal ID in response".to_string()))?
            .to_string();
            
        Ok(proposal_id)
    }
}

/// Wallet agent for federation interactions
pub struct WalletAgent {
    agoranet_client: AgoraNetClient,
    runtime_client: RuntimeClient,
}

impl WalletAgent {
    /// Create a new wallet agent
    pub fn new(agoranet_base_url: String, runtime_base_url: String) -> Self {
        Self {
            agoranet_client: AgoraNetClient::new(agoranet_base_url),
            runtime_client: RuntimeClient::new(runtime_base_url),
        }
    }

    /// Submit a proposal with an associated AgoraNet thread
    pub async fn submit_proposal_with_thread(
        &self, 
        ccl_content: &str, 
        thread_id: &str
    ) -> WalletAgentResult<String> {
        // Create metadata with thread_id
        let metadata = serde_json::json!({
            "thread_id": thread_id
        });
        
        // Submit the proposal to the Runtime
        let proposal_id = self.runtime_client.submit_proposal(ccl_content, metadata).await?;
        
        // Link the proposal to the AgoraNet thread
        self.agoranet_client.link_thread_to_proposal(thread_id, &proposal_id).await?;
        
        Ok(proposal_id)
    }
}

/// Factory for creating wallet agent instances
pub struct WalletAgentFactory;

impl WalletAgentFactory {
    /// Create a new wallet agent with default endpoints
    pub fn create_default() -> WalletAgent {
        WalletAgent::new(
            "http://localhost:3000/api".to_string(),
            "http://localhost:8080/api".to_string(),
        )
    }
    
    /// Create a new wallet agent with custom endpoints
    pub fn create(agoranet_base_url: String, runtime_base_url: String) -> WalletAgent {
        WalletAgent::new(agoranet_base_url, runtime_base_url)
    }
} 