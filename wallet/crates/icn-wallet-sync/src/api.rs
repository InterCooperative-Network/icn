use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};

use crate::error::SyncError;
use crate::SyncClient;

/// Federation information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationInfo {
    /// Federation ID
    pub id: String,
    
    /// Federation name
    pub name: String,
    
    /// List of peer IDs
    pub peers: Vec<String>,
    
    /// Creation timestamp
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub created_at: DateTime<Utc>,
    
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Peer information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    /// Peer ID
    pub id: String,
    
    /// Peer name
    pub name: String,
    
    /// Peer URL
    pub url: String,
    
    /// Peer type
    pub peer_type: String,
    
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// API client for federation info
pub struct FederationApiClient {
    /// Base URL for the federation API
    base_url: String,
    
    /// HTTP client
    client: reqwest::Client,
}

impl FederationApiClient {
    /// Create a new federation API client
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: reqwest::Client::new(),
        }
    }
    
    /// Get information about the federation
    pub async fn get_federation_info(&self) -> Result<FederationInfo, SyncError> {
        let url = format!("{}/api/v1/federation", self.base_url);
        
        let response = self.client.get(&url).send().await?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(SyncError::Federation(format!("Failed to get federation info: HTTP {}: {}", status, error_text)));
        }
        
        let federation_info = response.json::<FederationInfo>().await?;
        
        Ok(federation_info)
    }
    
    /// Get information about a peer
    pub async fn get_peer_info(&self, peer_id: &str) -> Result<PeerInfo, SyncError> {
        let url = format!("{}/api/v1/federation/peers/{}", self.base_url, peer_id);
        
        let response = self.client.get(&url).send().await?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(SyncError::Federation(format!("Failed to get peer info: HTTP {}: {}", status, error_text)));
        }
        
        let peer_info = response.json::<PeerInfo>().await?;
        
        Ok(peer_info)
    }
}

/// Federation API client extension for SyncClient
impl SyncClient {
    /// Get information about the federation
    pub async fn get_federation_info(&self) -> Result<FederationInfo, SyncError> {
        let url = format!("{}/api/v1/federation", self.base_url);
        
        let mut request = self.client.get(&url);
        
        // Add authentication if available
        if let Some(token) = &self.auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }
        
        let response = request.send().await?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(SyncError::Federation(format!("Failed to get federation info: HTTP {}: {}", status, error_text)));
        }
        
        let federation_info = response.json::<FederationInfo>().await?;
        Ok(federation_info)
    }
    
    /// Get information about a specific peer
    pub async fn get_peer_info(&self, peer_id: &str) -> Result<PeerInfo, SyncError> {
        let url = format!("{}/api/v1/federation/peers/{}", self.base_url, peer_id);
        
        let mut request = self.client.get(&url);
        
        // Add authentication if available
        if let Some(token) = &self.auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }
        
        let response = request.send().await?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(SyncError::Federation(format!("Failed to get peer info: HTTP {}: {}", status, error_text)));
        }
        
        let peer_info = response.json::<PeerInfo>().await?;
        Ok(peer_info)
    }
    
    /// Discover federation nodes from a seed node
    pub async fn discover_federation(&self) -> Result<Vec<String>, SyncError> {
        let federation_info = self.get_federation_info().await?;
        
        // Just return the peer IDs from the federation info
        // We'll need to look up specific details later if needed
        Ok(federation_info.peers)
    }
} 