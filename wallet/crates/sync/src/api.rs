use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use serde_json::Value;
use chrono::{DateTime, Utc};

use crate::SyncError;
use crate::SyncClient;

/// Federation information returned from the node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationInfo {
    /// Federation ID
    pub id: String,
    
    /// Federation name
    pub name: String,
    
    /// Federation status
    pub status: String,
    
    /// List of peers in the federation
    pub peers: Vec<PeerInfo>,
    
    /// Federation configuration
    pub config: Value,
}

/// Information about a federation peer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    /// Peer ID
    pub id: String,
    
    /// Peer DID
    pub did: String,
    
    /// Peer status (online, offline, etc.)
    pub status: String,
    
    /// Peer endpoint URL
    pub endpoint: Option<String>,
    
    /// Last seen timestamp
    #[serde(with = "chrono::serde::ts_milliseconds_option")]
    pub last_seen: Option<DateTime<Utc>>,
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
            return Err(SyncError::NodeError(format!("Failed to get federation info: HTTP {}: {}", status, error_text)));
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
            return Err(SyncError::NodeError(format!("Failed to get peer info: HTTP {}: {}", status, error_text)));
        }
        
        let peer_info = response.json::<PeerInfo>().await?;
        Ok(peer_info)
    }
    
    /// Discover federation nodes from a seed node
    pub async fn discover_federation(&self) -> Result<Vec<String>, SyncError> {
        let federation_info = self.get_federation_info().await?;
        
        // Extract endpoints from online peers
        let endpoints: Vec<String> = federation_info.peers.iter()
            .filter(|peer| peer.status == "online")
            .filter_map(|peer| peer.endpoint.clone())
            .collect();
        
        Ok(endpoints)
    }
} 