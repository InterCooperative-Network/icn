use serde::{Serialize, Deserialize};
use std::time::SystemTime;
use std::collections::HashMap;

/// Status of a network request
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequestStatus {
    /// Request succeeded
    Success,
    /// Request failed
    Failed,
    /// Request is pending
    Pending,
}

impl Default for RequestStatus {
    fn default() -> Self {
        RequestStatus::Pending
    }
}

/// Response from node after submitting data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSubmissionResponse {
    /// The CID of the submitted node
    pub id: String,
    
    /// Timestamp when the node was accepted
    pub timestamp: SystemTime,
    
    /// Block number (if applicable)
    pub block_number: Option<u64>,
    
    /// Status of the submission
    #[serde(default)]
    pub status: RequestStatus,
    
    /// Error message (if any)
    pub error: Option<String>,
    
    /// Additional response data
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl NodeSubmissionResponse {
    /// Create a new successful response
    pub fn success(id: String, timestamp: SystemTime) -> Self {
        Self {
            id,
            timestamp,
            block_number: None,
            status: RequestStatus::Success,
            error: None,
            metadata: HashMap::new(),
        }
    }
    
    /// Create a new failed response
    pub fn failed(id: String, error: String) -> Self {
        Self {
            id,
            timestamp: SystemTime::now(),
            block_number: None,
            status: RequestStatus::Failed,
            error: Some(error),
            metadata: HashMap::new(),
        }
    }
    
    /// Create a new pending response
    pub fn pending(id: String) -> Self {
        Self {
            id,
            timestamp: SystemTime::now(),
            block_number: None,
            status: RequestStatus::Pending,
            error: None,
            metadata: HashMap::new(),
        }
    }
    
    /// Check if the response was successful
    pub fn is_success(&self) -> bool {
        self.status == RequestStatus::Success
    }
    
    /// Check if the response failed
    pub fn is_failed(&self) -> bool {
        self.status == RequestStatus::Failed
    }
    
    /// Check if the response is pending
    pub fn is_pending(&self) -> bool {
        self.status == RequestStatus::Pending
    }
    
    /// Add metadata to the response
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }
    
    /// Set the block number
    pub fn with_block_number(mut self, block_number: u64) -> Self {
        self.block_number = Some(block_number);
        self
    }
} 