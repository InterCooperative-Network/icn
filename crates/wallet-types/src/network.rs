use serde::{Serialize, Deserialize};
use std::time::SystemTime;

/// Network status information for monitoring connectivity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStatus {
    /// Connection status to federation nodes
    pub is_connected: bool,
    /// Latency to primary federation node in milliseconds
    pub primary_node_latency: Option<u64>,
    /// Last successful sync time
    pub last_successful_sync: Option<SystemTime>,
    /// Number of pending submissions
    pub pending_submissions: usize,
    /// Current federation node in use
    pub active_federation_url: String,
    /// Number of successful operations
    pub successful_operations: usize,
    /// Number of failed operations
    pub failed_operations: usize,
}

/// Response from node submission
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeSubmissionResponse {
    /// Success status
    pub success: bool,
    /// CID assigned to the node
    pub cid: Option<String>,
    /// Error message if submission failed
    pub error: Option<String>,
} 