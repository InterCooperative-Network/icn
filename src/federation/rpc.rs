use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Error types for federation RPC operations
#[derive(Debug)]
pub enum FederationRpcError {
    NetworkError(String),
    AuthenticationError(String),
    NotFoundError(String),
    SerializationError(String),
    ServerError(String),
    TimeoutError,
    UnknownError(String),
}

impl fmt::Display for FederationRpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FederationRpcError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            FederationRpcError::AuthenticationError(msg) => write!(f, "Authentication error: {}", msg),
            FederationRpcError::NotFoundError(msg) => write!(f, "Not found: {}", msg),
            FederationRpcError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
            FederationRpcError::ServerError(msg) => write!(f, "Server error: {}", msg),
            FederationRpcError::TimeoutError => write!(f, "Request timed out"),
            FederationRpcError::UnknownError(msg) => write!(f, "Unknown error: {}", msg),
        }
    }
}

impl Error for FederationRpcError {}

/// A finalization receipt from the federation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalizationReceipt {
    pub id: String,
    pub federation_id: String,
    pub receipt_type: String,
    pub issuer: String,
    pub issuer_name: Option<String>,
    pub subject_did: String,
    pub action_type: String,
    pub timestamp: DateTime<Utc>,
    pub dag_height: u64,
    pub dag_vertex: String,
    pub metadata: HashMap<String, String>,
    pub signatures: Vec<ReceiptSignature>,
    pub content: serde_json::Value,
}

/// A signature on a federation receipt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptSignature {
    pub signer_did: String,
    pub signer_role: String,
    pub signature_value: String,
    pub signature_type: String,
    pub timestamp: DateTime<Utc>,
}

/// Federation manifest containing member information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationManifest {
    pub federation_id: String,
    pub name: String,
    pub description: Option<String>,
    pub members: HashMap<String, FederationMemberRole>,
    pub quorum_rules: QuorumConfig,
    pub created: DateTime<Utc>,
    pub updated: Option<DateTime<Utc>>,
    pub version: u32,
    pub health_metrics: Option<FederationHealthMetrics>,
}

/// Role of a federation member
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationMemberRole {
    pub role: String,
    pub weight: u32,
    pub voting_power: Option<u32>,
    pub can_veto: Option<bool>,
}

/// Quorum configuration for the federation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuorumConfig {
    pub policy_type: String,
    pub min_participants: u32,
    pub min_approvals: u32,
    pub threshold_percentage: Option<u32>,
    pub timeout_seconds: Option<u64>,
}

/// Health metrics for the federation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationHealthMetrics {
    pub overall_health: u32,
    pub metrics: HashMap<String, f64>,
    pub warnings: Vec<String>,
    pub recommendations: Vec<String>,
}

/// Interface for federation sync operations
pub trait IFederationSync {
    /// Get finalized receipts by DID
    fn get_finalized_receipts_by_did(&self, did: &str) -> Result<Vec<FinalizationReceipt>, FederationRpcError>;
    
    /// Get federation manifest
    fn get_federation_manifest(&self, federation_id: &str) -> Result<FederationManifest, FederationRpcError>;
}

/// Mock implementation of the federation sync interface
pub struct MockFederationSync {
    // Singleton instance
    receipts: HashMap<String, Vec<FinalizationReceipt>>,
    manifests: HashMap<String, FederationManifest>,
}

impl Default for MockFederationSync {
    fn default() -> Self {
        let mut instance = Self {
            receipts: HashMap::new(),
            manifests: HashMap::new(),
        };
        
        // Initialize with some mock data
        instance.init_mock_data();
        instance
    }
}

impl MockFederationSync {
    /// Create a new instance
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Initialize mock data
    fn init_mock_data(&mut self) {
        // Create mock federation manifests
        let federation_id = "fed:governance:alpha";
        let manifest = self.create_mock_manifest(federation_id, "Alpha Governance Federation");
        self.manifests.insert(federation_id.to_string(), manifest);
        
        let federation_id = "fed:resources:beta";
        let manifest = self.create_mock_manifest(federation_id, "Beta Resource Federation");
        self.manifests.insert(federation_id.to_string(), manifest);
        
        // Create mock receipts for some DIDs
        let test_dids = vec![
            "did:icn:governance:alice",
            "did:icn:governance:bob",
            "did:icn:resources:carol",
        ];
        
        for did in test_dids {
            let receipts = self.create_mock_receipts_for_did(did);
            self.receipts.insert(did.to_string(), receipts);
        }
    }
    
    /// Create a mock federation manifest
    fn create_mock_manifest(&self, federation_id: &str, name: &str) -> FederationManifest {
        let mut members = HashMap::new();
        
        // Add admin
        members.insert(
            format!("did:icn:{}:admin", federation_id.split(':').nth(1).unwrap_or("governance")),
            FederationMemberRole {
                role: "Admin".to_string(),
                weight: 3,
                voting_power: Some(3),
                can_veto: Some(true),
            },
        );
        
        // Add guardians
        for i in 1..=3 {
            members.insert(
                format!("did:icn:{}:guardian{}", federation_id.split(':').nth(1).unwrap_or("governance"), i),
                FederationMemberRole {
                    role: "Guardian".to_string(),
                    weight: 2,
                    voting_power: Some(2),
                    can_veto: Some(false),
                },
            );
        }
        
        // Add members
        for i in 1..=5 {
            members.insert(
                format!("did:icn:{}:member{}", federation_id.split(':').nth(1).unwrap_or("governance"), i),
                FederationMemberRole {
                    role: "Member".to_string(),
                    weight: 1,
                    voting_power: Some(1),
                    can_veto: Some(false),
                },
            );
        }
        
        FederationManifest {
            federation_id: federation_id.to_string(),
            name: name.to_string(),
            description: Some(format!("A mock federation for testing: {}", name)),
            members,
            quorum_rules: QuorumConfig {
                policy_type: "Weighted".to_string(),
                min_participants: 3,
                min_approvals: 2,
                threshold_percentage: Some(60),
                timeout_seconds: Some(86400),
            },
            created: Utc::now() - chrono::Duration::days(30),
            updated: Some(Utc::now() - chrono::Duration::days(1)),
            version: 1,
            health_metrics: Some(FederationHealthMetrics {
                overall_health: 85,
                metrics: {
                    let mut metrics = HashMap::new();
                    metrics.insert("approval_rate".to_string(), 0.92);
                    metrics.insert("finalizer_diversity".to_string(), 0.75);
                    metrics.insert("daily_receipts".to_string(), 12.5);
                    metrics
                },
                warnings: vec![],
                recommendations: vec![],
            }),
        }
    }
    
    /// Create mock receipts for a DID
    fn create_mock_receipts_for_did(&self, did: &str) -> Vec<FinalizationReceipt> {
        let mut receipts = Vec::new();
        let scope = did.split(':').nth(2).unwrap_or("governance");
        let federation_id = format!("fed:{}:alpha", scope);
        
        // Create a proposal receipt
        receipts.push(FinalizationReceipt {
            id: Uuid::new_v4().to_string(),
            federation_id: federation_id.clone(),
            receipt_type: "proposal".to_string(),
            issuer: format!("did:icn:{}:admin", scope),
            issuer_name: Some("Federation Admin".to_string()),
            subject_did: did.to_string(),
            action_type: "proposal_submission".to_string(),
            timestamp: Utc::now() - chrono::Duration::days(7),
            dag_height: 1234,
            dag_vertex: format!("vertex_{}", Uuid::new_v4().simple()),
            metadata: {
                let mut metadata = HashMap::new();
                metadata.insert("title".to_string(), "Budget Allocation Proposal".to_string());
                metadata.insert("category".to_string(), "Finance".to_string());
                metadata
            },
            signatures: vec![
                ReceiptSignature {
                    signer_did: format!("did:icn:{}:admin", scope),
                    signer_role: "Admin".to_string(),
                    signature_value: format!("sig_{}", Uuid::new_v4().simple()),
                    signature_type: "Ed25519".to_string(),
                    timestamp: Utc::now() - chrono::Duration::days(7),
                }
            ],
            content: serde_json::json!({
                "proposal_id": Uuid::new_v4().to_string(),
                "title": "Budget Allocation Proposal",
                "description": "Proposal to allocate budget for Q3 projects",
                "amount": 5000,
                "currency": "USD",
                "items": [
                    {
                        "name": "Development",
                        "amount": 3000
                    },
                    {
                        "name": "Marketing",
                        "amount": 2000
                    }
                ]
            }),
        });
        
        // Create a vote receipt
        receipts.push(FinalizationReceipt {
            id: Uuid::new_v4().to_string(),
            federation_id: federation_id.clone(),
            receipt_type: "vote".to_string(),
            issuer: format!("did:icn:{}:guardian1", scope),
            issuer_name: Some("Guardian One".to_string()),
            subject_did: did.to_string(),
            action_type: "proposal_vote".to_string(),
            timestamp: Utc::now() - chrono::Duration::days(5),
            dag_height: 1235,
            dag_vertex: format!("vertex_{}", Uuid::new_v4().simple()),
            metadata: {
                let mut metadata = HashMap::new();
                metadata.insert("vote".to_string(), "approve".to_string());
                metadata.insert("proposal_id".to_string(), receipts[0].content["proposal_id"].as_str().unwrap_or("").to_string());
                metadata
            },
            signatures: vec![
                ReceiptSignature {
                    signer_did: format!("did:icn:{}:guardian1", scope),
                    signer_role: "Guardian".to_string(),
                    signature_value: format!("sig_{}", Uuid::new_v4().simple()),
                    signature_type: "Ed25519".to_string(),
                    timestamp: Utc::now() - chrono::Duration::days(5),
                }
            ],
            content: serde_json::json!({
                "proposal_id": receipts[0].content["proposal_id"].as_str().unwrap_or(""),
                "vote": "approve",
                "comment": "I approve this budget allocation",
                "timestamp": (Utc::now() - chrono::Duration::days(5)).to_rfc3339()
            }),
        });
        
        // Create a finalization receipt
        receipts.push(FinalizationReceipt {
            id: Uuid::new_v4().to_string(),
            federation_id: federation_id.clone(),
            receipt_type: "finalization".to_string(),
            issuer: format!("did:icn:{}:admin", scope),
            issuer_name: Some("Federation Admin".to_string()),
            subject_did: did.to_string(),
            action_type: "proposal_finalization".to_string(),
            timestamp: Utc::now() - chrono::Duration::days(3),
            dag_height: 1240,
            dag_vertex: format!("vertex_{}", Uuid::new_v4().simple()),
            metadata: {
                let mut metadata = HashMap::new();
                metadata.insert("status".to_string(), "approved".to_string());
                metadata.insert("proposal_id".to_string(), receipts[0].content["proposal_id"].as_str().unwrap_or("").to_string());
                metadata.insert("yes_votes".to_string(), "3".to_string());
                metadata.insert("no_votes".to_string(), "1".to_string());
                metadata.insert("abstain_votes".to_string(), "0".to_string());
                metadata
            },
            signatures: vec![
                ReceiptSignature {
                    signer_did: format!("did:icn:{}:admin", scope),
                    signer_role: "Admin".to_string(),
                    signature_value: format!("sig_{}", Uuid::new_v4().simple()),
                    signature_type: "Ed25519".to_string(),
                    timestamp: Utc::now() - chrono::Duration::days(3),
                },
                ReceiptSignature {
                    signer_did: format!("did:icn:{}:guardian1", scope),
                    signer_role: "Guardian".to_string(),
                    signature_value: format!("sig_{}", Uuid::new_v4().simple()),
                    signature_type: "Ed25519".to_string(),
                    timestamp: Utc::now() - chrono::Duration::days(3) + chrono::Duration::minutes(1),
                },
                ReceiptSignature {
                    signer_did: format!("did:icn:{}:guardian2", scope),
                    signer_role: "Guardian".to_string(),
                    signature_value: format!("sig_{}", Uuid::new_v4().simple()),
                    signature_type: "Ed25519".to_string(),
                    timestamp: Utc::now() - chrono::Duration::days(3) + chrono::Duration::minutes(2),
                }
            ],
            content: serde_json::json!({
                "proposal_id": receipts[0].content["proposal_id"].as_str().unwrap_or(""),
                "status": "approved",
                "voting_summary": {
                    "yes_votes": 3,
                    "no_votes": 1,
                    "abstain_votes": 0,
                    "quorum_met": true,
                    "threshold_met": true
                },
                "execution_status": "pending",
                "timestamp": (Utc::now() - chrono::Duration::days(3)).to_rfc3339()
            }),
        });
        
        receipts
    }
}

impl IFederationSync for MockFederationSync {
    fn get_finalized_receipts_by_did(&self, did: &str) -> Result<Vec<FinalizationReceipt>, FederationRpcError> {
        // Check if we have mock receipts for this DID
        if let Some(receipts) = self.receipts.get(did) {
            Ok(receipts.clone())
        } else {
            // For DIDs not in our mock data, return empty list but not an error
            Ok(Vec::new())
        }
    }
    
    fn get_federation_manifest(&self, federation_id: &str) -> Result<FederationManifest, FederationRpcError> {
        // Check if we have this federation manifest
        if let Some(manifest) = self.manifests.get(federation_id) {
            Ok(manifest.clone())
        } else {
            Err(FederationRpcError::NotFoundError(format!(
                "Federation manifest not found for ID: {}", federation_id
            )))
        }
    }
} 