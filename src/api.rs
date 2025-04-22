use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::Duration;
use thiserror::Error;
use crate::identity::{Identity, IdentityError};
use base64::{Engine as _};

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("Failed to read file: {0}")]
    FileReadError(String),
    
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    
    #[error("Authentication error: {0}")]
    AuthError(String),
    
    #[error("Node rejected proposal: {0}")]
    RejectedProposal(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Identity error: {0}")]
    IdentityError(#[from] IdentityError),
}

// ApiConfig holds configuration for connecting to a CoVM node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub node_url: String,
    pub timeout_seconds: u64,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            node_url: "http://localhost:9000".to_string(),
            timeout_seconds: 30,
        }
    }
}

// Different API endpoints
#[derive(Debug, Clone, Copy)]
pub enum ApiEndpoint {
    Submit,
    Query,
    Status,
    Balance,
}

impl ApiEndpoint {
    fn path(&self) -> &'static str {
        match self {
            ApiEndpoint::Submit => "/api/v1/submit",
            ApiEndpoint::Query => "/api/v1/query",
            ApiEndpoint::Status => "/api/v1/status",
            ApiEndpoint::Balance => "/api/v1/balance",
        }
    }
}

// ApiResponse is a generic response from the CoVM API
#[derive(Debug, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub message: String,
    pub data: Option<T>,
    pub error: Option<String>,
}

// Status of a CoVM node
#[derive(Debug, Serialize, Deserialize)]
pub struct NodeStatus {
    pub version: String,
    pub connected_peers: usize,
    pub dag_height: u64,
    pub uptime_seconds: u64,
}

// Represents a DSL program submission
#[derive(Debug, Serialize, Deserialize)]
pub struct ProgramSubmission {
    pub program: String,
    pub signature: String,
    pub did: String,
    pub timestamp: u64,
}

// API client for interacting with CoVM nodes
#[derive(Clone)]
pub struct ApiClient {
    config: ApiConfig,
    client: Client,
}

impl ApiClient {
    pub fn new(config: ApiConfig) -> Result<Self, ApiError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .map_err(|e| ApiError::NetworkError(format!("Failed to create HTTP client: {}", e)))?;
        
        Ok(Self {
            config,
            client,
        })
    }
    
    // Submit a signed DSL program to the CoVM
    pub fn submit_program(&self, program_content: &str, 
                          identity: &Identity) -> Result<ApiResponse<String>, ApiError> {
        // Get current timestamp
        let timestamp = chrono::Utc::now().timestamp() as u64;
        
        // Sign the program content
        let signature = identity.sign(program_content.as_bytes())
            .map_err(|e| ApiError::AuthError(format!("Failed to sign program: {}", e)))?;
        
        let signature_base64 = base64::engine::general_purpose::STANDARD.encode(&signature);
        
        // Create the submission payload
        let submission = ProgramSubmission {
            program: program_content.to_string(),
            signature: signature_base64,
            did: identity.did().to_string(),
            timestamp,
        };
        
        // Serialize the submission
        let submission_json = serde_json::to_string(&submission)
            .map_err(|e| ApiError::SerializationError(format!("Failed to serialize submission: {}", e)))?;
        
        // Set up headers
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        
        // Make the API request
        let url = format!("{}{}", self.config.node_url, ApiEndpoint::Submit.path());
        let response = self.client.post(&url)
            .headers(headers)
            .body(submission_json)
            .send()
            .map_err(|e| ApiError::NetworkError(format!("Failed to submit program: {}", e)))?;
        
        self.parse_response(response)
    }
    
    // Query the CoVM for information
    pub fn query<T: for<'de> Deserialize<'de>>(&self, query: &str,
                                                identity: &Identity) -> Result<ApiResponse<T>, ApiError> {
        // Sign the query
        let signature = identity.sign(query.as_bytes())
            .map_err(|e| ApiError::AuthError(format!("Failed to sign query: {}", e)))?;
        
        let signature_base64 = base64::engine::general_purpose::STANDARD.encode(&signature);
        
        // Set up headers
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert("X-ICN-DID", HeaderValue::from_str(identity.did()).unwrap());
        headers.insert("X-ICN-Signature", HeaderValue::from_str(&signature_base64).unwrap());
        
        // Make the API request
        let url = format!("{}{}", self.config.node_url, ApiEndpoint::Query.path());
        let response = self.client.post(&url)
            .headers(headers)
            .body(query.to_string())
            .send()
            .map_err(|e| ApiError::NetworkError(format!("Failed to send query: {}", e)))?;
        
        self.parse_response(response)
    }
    
    // Get status information from the node
    pub fn get_status(&self) -> Result<ApiResponse<NodeStatus>, ApiError> {
        let url = format!("{}{}", self.config.node_url, ApiEndpoint::Status.path());
        let response = self.client.get(&url)
            .send()
            .map_err(|e| ApiError::NetworkError(format!("Failed to get status: {}", e)))?;
        
        self.parse_response(response)
    }
    
    // Parse API response
    fn parse_response<T: for<'de> Deserialize<'de>>(&self, response: Response) -> Result<ApiResponse<T>, ApiError> {
        if !response.status().is_success() {
            return Err(ApiError::InvalidResponse(format!(
                "Received error status code: {}", response.status()
            )));
        }
        
        let response_text = response.text()
            .map_err(|e| ApiError::InvalidResponse(format!("Failed to read response body: {}", e)))?;
        
        serde_json::from_str(&response_text)
            .map_err(|e| ApiError::InvalidResponse(format!("Failed to parse response: {}", e)))
    }
    
    // Connect to a Unix socket (for local node communication)
    pub fn connect_unix_socket(socket_path: &str) -> Result<Self, ApiError> {
        // For Unix socket, we'll use a special URL format that the reqwest client understands
        let config = ApiConfig {
            node_url: format!("unix://{}", socket_path),
            timeout_seconds: 30,
        };
        
        Self::new(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito;
    use crate::identity::{Identity, KeyType};
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;
    
    #[test]
    fn test_node_status() {
        // Set up a mock server
        let mock_status = mockito::mock("GET", "/api/v1/status")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"
                {
                    "success": true,
                    "message": "Node is operational",
                    "data": {
                        "version": "0.1.0",
                        "connected_peers": 3,
                        "dag_height": 1024,
                        "uptime_seconds": 3600
                    }
                }
            "#)
            .create();
        
        // Configure the client to use our mock server
        let config = ApiConfig {
            node_url: mockito::server_url(),
            timeout_seconds: 5,
        };
        
        let client = ApiClient::new(config).unwrap();
        let response = client.get_status().unwrap();
        
        assert!(response.success);
        assert_eq!(response.message, "Node is operational");
        
        let status = response.data.unwrap();
        assert_eq!(status.version, "0.1.0");
        assert_eq!(status.connected_peers, 3);
        assert_eq!(status.dag_height, 1024);
        assert_eq!(status.uptime_seconds, 3600);
        
        mock_status.assert();
    }
    
    #[test]
    fn test_submit_program() {
        // Create a temporary directory and file
        let temp_dir = tempdir().unwrap();
        let program_path = temp_dir.path().join("test_program.dsl");
        
        // Write sample program content
        let program_content = "function test() { return 42; }";
        let mut file = File::create(&program_path).unwrap();
        file.write_all(program_content.as_bytes()).unwrap();
        
        // Create a test identity
        let identity = Identity::new("test", "user", KeyType::Ed25519).unwrap();
        
        // Set up a mock server
        let mock_submit = mockito::mock("POST", "/api/v1/submit")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"
                {
                    "success": true,
                    "message": "Program submitted successfully",
                    "data": "tx-12345"
                }
            "#)
            .create();
        
        // Configure the client to use our mock server
        let config = ApiConfig {
            node_url: mockito::server_url(),
            timeout_seconds: 5,
        };
        
        let client = ApiClient::new(config).unwrap();
        let response = client.submit_program(program_content, &identity).unwrap();
        
        assert!(response.success);
        assert_eq!(response.message, "Program submitted successfully");
        assert_eq!(response.data.unwrap(), "tx-12345");
        
        mock_submit.assert();
    }
} 