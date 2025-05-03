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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn test_node_submission_response_creation() {
        let now = SystemTime::now();
        
        // Test success response creation
        let success = NodeSubmissionResponse::success("test-id".to_string(), now);
        assert_eq!(success.id, "test-id");
        assert_eq!(success.timestamp, now);
        assert!(success.is_success());
        assert!(!success.is_failed());
        assert!(!success.is_pending());
        assert!(success.error.is_none());
        
        // Test failed response creation
        let failed = NodeSubmissionResponse::failed("failed-id".to_string(), "Test error".to_string());
        assert_eq!(failed.id, "failed-id");
        assert!(failed.is_failed());
        assert!(!failed.is_success());
        assert!(!failed.is_pending());
        assert_eq!(failed.error, Some("Test error".to_string()));
        
        // Test pending response creation
        let pending = NodeSubmissionResponse::pending("pending-id".to_string());
        assert_eq!(pending.id, "pending-id");
        assert!(pending.is_pending());
        assert!(!pending.is_success());
        assert!(!pending.is_failed());
    }

    #[test]
    fn test_node_submission_response_with_modifiers() {
        let response = NodeSubmissionResponse::success("test-id".to_string(), SystemTime::now())
            .with_block_number(12345)
            .with_metadata("key1", "value1")
            .with_metadata("key2", "value2");
        
        assert_eq!(response.block_number, Some(12345));
        assert_eq!(response.metadata.len(), 2);
        assert_eq!(response.metadata.get("key1"), Some(&"value1".to_string()));
        assert_eq!(response.metadata.get("key2"), Some(&"value2".to_string()));
    }

    #[test]
    fn test_node_submission_response_serialization() {
        let now = SystemTime::now();
        
        let original = NodeSubmissionResponse {
            id: "test-id".to_string(),
            timestamp: now,
            block_number: Some(42),
            status: RequestStatus::Success,
            error: None,
            metadata: {
                let mut map = HashMap::new();
                map.insert("key1".to_string(), "value1".to_string());
                map
            },
        };
        
        // Serialize to JSON string
        let serialized = serde_json::to_string(&original).expect("Serialization failed");
        
        // Deserialize back
        let deserialized: NodeSubmissionResponse = serde_json::from_str(&serialized)
            .expect("Deserialization failed");
        
        // Check fields
        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.block_number, deserialized.block_number);
        assert_eq!(original.status, deserialized.status);
        assert_eq!(original.error, deserialized.error);
        assert_eq!(original.metadata.len(), deserialized.metadata.len());
        assert_eq!(original.metadata.get("key1"), deserialized.metadata.get("key1"));
    }

    #[test]
    fn test_request_status_defaults() {
        // Test default status is Pending
        let status: RequestStatus = Default::default();
        assert_eq!(status, RequestStatus::Pending);
        
        // Test with default status in response
        let response = NodeSubmissionResponse {
            id: "default-test".to_string(),
            timestamp: SystemTime::now(),
            block_number: None,
            status: Default::default(),
            error: None,
            metadata: HashMap::new(),
        };
        
        assert!(response.is_pending());
    }
} 