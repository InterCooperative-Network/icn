use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Network status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStatus {
    /// Whether the network is online
    pub online: bool,
    
    /// Network type (e.g., "testnet", "mainnet")
    pub network_type: String,
    
    /// Number of connected peers
    pub peer_count: u32,
    
    /// Current block height
    pub block_height: u64,
    
    /// Network latency in milliseconds
    pub latency_ms: u64,
    
    /// Sync status percentage (0-100)
    pub sync_percent: u8,
    
    /// Additional status information
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Response from node after submitting data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSubmissionResponse {
    /// Success status
    pub success: bool,
    
    /// Transaction or submission ID
    pub id: String,
    
    /// Timestamp of the submission
    pub timestamp: String,
    
    /// Block number (if applicable)
    pub block_number: Option<u64>,
    
    /// Error message (if any)
    pub error: Option<String>,
    
    /// Additional response data
    #[serde(default)]
    pub data: HashMap<String, String>,
}
