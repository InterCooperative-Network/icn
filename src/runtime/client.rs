use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use reqwest::{Client, StatusCode};
use crate::error::WalletError;

/// Client for interacting with the ICN Runtime
pub struct RuntimeClient {
    /// Base URL for the runtime API
    base_url: String,
    
    /// HTTP client
    client: Client,
}

/// Federation information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Federation {
    /// Federation ID
    pub id: String,
    
    /// Federation name
    pub name: String,
    
    /// Federation description
    pub description: String,
    
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    
    /// Members of the federation (DID -> MemberInfo)
    pub members: HashMap<String, FederationMember>,
}

/// Federation member information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationMember {
    /// Member name
    pub name: String,
    
    /// Member role
    pub role: String,
    
    /// Voting weight
    pub weight: u32,
}

/// Anchor metadata returned from the runtime
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorMetadata {
    /// Merkle root
    pub merkle_root: String,
    
    /// Federation ID
    pub federation_id: String,
    
    /// Finalizer DID
    pub finalizer_did: String,
    
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    
    /// DAG root hash (for epoch credentials)
    pub dag_root: Option<String>,
    
    /// Epoch ID (for epoch credentials)
    pub epoch_id: Option<String>,
    
    /// Mandate (for epoch credentials)
    pub mandate: Option<String>,
    
    /// Amendment ID (for amendment credentials)
    pub amendment_id: Option<String>,
    
    /// Previous amendment ID (for amendment credentials)
    pub previous_amendment_id: Option<String>,
    
    /// Hash of the amendment text (for amendment credentials)
    pub text_hash: Option<String>,
    
    /// Epoch in which amendment was ratified (for amendment credentials)
    pub ratified_in_epoch: Option<String>,
}

impl RuntimeClient {
    /// Create a new runtime client
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            client: Client::new(),
        }
    }
    
    /// Get all federations for a user
    pub async fn get_user_federations(&self, user_did: &str) -> Result<Vec<Federation>, WalletError> {
        let url = format!("{}/api/v1/federations/user/{}", self.base_url, user_did);
        
        let response = self.client.get(&url)
            .send()
            .await
            .map_err(|e| WalletError::RuntimeApiError(format!("Failed to fetch federations: {}", e)))?;
        
        if response.status() != StatusCode::OK {
            return Err(WalletError::RuntimeApiError(format!("Failed to fetch federations: {}", response.status())));
        }
        
        let federations = response.json::<Vec<Federation>>()
            .await
            .map_err(|e| WalletError::RuntimeApiError(format!("Failed to parse federation response: {}", e)))?;
        
        Ok(federations)
    }
    
    /// Get details of a specific federation
    pub async fn get_federation(&self, federation_id: &str) -> Result<Federation, WalletError> {
        let url = format!("{}/api/v1/federations/{}", self.base_url, federation_id);
        
        let response = self.client.get(&url)
            .send()
            .await
            .map_err(|e| WalletError::RuntimeApiError(format!("Failed to fetch federation: {}", e)))?;
        
        if response.status() != StatusCode::OK {
            return Err(WalletError::RuntimeApiError(format!("Failed to fetch federation: {}", response.status())));
        }
        
        let federation = response.json::<Federation>()
            .await
            .map_err(|e| WalletError::RuntimeApiError(format!("Failed to parse federation response: {}", e)))?;
        
        Ok(federation)
    }
    
    /// Create an epoch anchor credential
    pub async fn create_epoch_anchor(
        &self,
        federation_id: &str,
        epoch_id: &str,
        mandate: &str,
        dag_root: Option<&str>,
    ) -> Result<AnchorMetadata, WalletError> {
        let url = format!("{}/api/v1/federations/{}/anchor", self.base_url, federation_id);
        
        // Prepare request body
        let mut body = HashMap::new();
        body.insert("epoch_id", epoch_id.to_string());
        body.insert("mandate", mandate.to_string());
        if let Some(root) = dag_root {
            body.insert("dag_root", root.to_string());
        }
        
        let response = self.client.post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| WalletError::RuntimeApiError(format!("Failed to create anchor: {}", e)))?;
        
        if response.status() != StatusCode::OK && response.status() != StatusCode::CREATED {
            return Err(WalletError::RuntimeApiError(format!("Failed to create anchor: {}", response.status())));
        }
        
        let anchor = response.json::<AnchorMetadata>()
            .await
            .map_err(|e| WalletError::RuntimeApiError(format!("Failed to parse anchor response: {}", e)))?;
        
        Ok(anchor)
    }
    
    /// Create an amendment anchor credential
    pub async fn create_amendment_anchor(
        &self,
        federation_id: &str,
        amendment_id: &str,
        text_path: &str,
        ratified_in_epoch: &str,
        previous_amendment_id: Option<&str>,
        dag_root: Option<&str>,
    ) -> Result<AnchorMetadata, WalletError> {
        let url = format!("{}/api/v1/federations/{}/amendment", self.base_url, federation_id);
        
        // Prepare request body
        let mut body = HashMap::new();
        body.insert("amendment_id", amendment_id.to_string());
        body.insert("text_path", text_path.to_string());
        body.insert("ratified_in_epoch", ratified_in_epoch.to_string());
        
        if let Some(prev_id) = previous_amendment_id {
            body.insert("previous_amendment_id", prev_id.to_string());
        }
        if let Some(root) = dag_root {
            body.insert("dag_root", root.to_string());
        }
        
        let response = self.client.post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| WalletError::RuntimeApiError(format!("Failed to create amendment anchor: {}", e)))?;
        
        if response.status() != StatusCode::OK && response.status() != StatusCode::CREATED {
            return Err(WalletError::RuntimeApiError(format!("Failed to create amendment anchor: {}", response.status())));
        }
        
        let anchor = response.json::<AnchorMetadata>()
            .await
            .map_err(|e| WalletError::RuntimeApiError(format!("Failed to parse amendment anchor response: {}", e)))?;
        
        Ok(anchor)
    }
} 