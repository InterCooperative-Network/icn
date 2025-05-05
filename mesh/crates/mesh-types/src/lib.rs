use chrono::{DateTime, Utc};
use cid::Cid;
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A task intent representing a request to execute a WASM module in the mesh network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskIntent {
    /// The DID of the publisher requesting the computation
    pub publisher_did: Did,
    
    /// Content ID pointing to the WASM module to be executed
    pub wasm_cid: Cid,
    
    /// Content ID pointing to the input data for the computation
    pub input_cid: Cid,
    
    /// Fee offered for execution (in network tokens)
    pub fee: u64,
    
    /// Required number of verifiers for this task
    pub verifiers: u32,
    
    /// Expiry timestamp after which the task is no longer valid
    pub expiry: DateTime<Utc>,
    
    /// Additional task metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

/// A receipt indicating successful execution of a task by a worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReceipt {
    /// The DID of the worker that executed the task
    pub worker_did: Did,
    
    /// Content ID of the task that was executed
    pub task_cid: Cid,
    
    /// Content ID pointing to the output data of the computation
    pub output_cid: Cid,
    
    /// Amount of computational fuel consumed during execution
    pub fuel_consumed: u64,
    
    /// Timestamp when the execution was completed
    pub timestamp: DateTime<Utc>,
    
    /// Cryptographic signature of the worker
    pub signature: Vec<u8>,
    
    /// Additional execution metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

/// A receipt from a verifier validating (or invalidating) an execution receipt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReceipt {
    /// The DID of the verifier
    pub verifier_did: Did,
    
    /// Content ID of the execution receipt being verified
    pub receipt_cid: Cid,
    
    /// Verification verdict (true = valid, false = invalid)
    pub verdict: bool,
    
    /// Content ID pointing to any proof data supporting the verdict
    pub proof_cid: Cid,
    
    /// Timestamp when the verification was completed
    pub timestamp: DateTime<Utc>,
    
    /// Cryptographic signature of the verifier
    pub signature: Vec<u8>,
    
    /// Additional verification metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

/// Network policy parameters for the mesh compute overlay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshPolicy {
    /// Alpha parameter: weight given to execution performance in reputation
    pub alpha: f64,
    
    /// Beta parameter: weight given to verification accuracy in reputation
    pub beta: f64,
    
    /// Gamma parameter: penalty factor for incorrect verifications
    pub gamma: f64,
    
    /// Lambda parameter: reputation decay rate
    pub lambda: f64,
    
    /// Weight given to staked tokens in worker selection
    pub stake_weight: f64,
    
    /// Minimum acceptable fee for task execution
    pub min_fee: u64,
    
    /// Base capacity units representing computational resources
    pub capacity_units: u32,
}

/// Information about a compute offer in response to a task intent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeOffer {
    /// The DID of the worker offering to execute the task
    pub worker_did: Did,
    
    /// Content ID of the task being offered to execute
    pub task_cid: Cid,
    
    /// Estimated cost to execute the task
    pub cost_estimate: u64,
    
    /// Available capacity of the worker
    pub available_capacity: u32,
    
    /// Estimated time to completion
    pub estimated_time_ms: u64,
    
    /// Timestamp when the offer was created
    pub timestamp: DateTime<Utc>,
    
    /// Cryptographic signature of the worker
    pub signature: Vec<u8>,
}

/// Information about a mesh network peer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    /// The DID of the peer
    pub did: Did,
    
    /// Current reputation score of the peer
    pub reputation_score: f64,
    
    /// Available capacity units of the peer
    pub capacity_units: u32,
    
    /// Amount of tokens staked by the peer
    pub staked_tokens: u64,
    
    /// Is this peer currently active and accepting tasks
    pub is_active: bool,
}

/// A snapshot of a peer's reputation at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationSnapshot {
    /// The DID of the peer
    pub did: Did,
    
    /// Reputation score at this point in time
    pub score: f64,
    
    /// Timestamp when this snapshot was taken
    pub timestamp: DateTime<Utc>,
    
    /// Component scores that make up the total reputation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<HashMap<String, f64>>,
}

/// Implement VerifiableCredential trait for TaskIntent
impl icn_identity::VerifiableCredential for TaskIntent {
    fn get_type(&self) -> String {
        "MeshTaskIntent".to_string()
    }
    
    fn get_issuer(&self) -> Did {
        self.publisher_did.clone()
    }
    
    fn get_issuance_date(&self) -> DateTime<Utc> {
        Utc::now() // In a real implementation, this would be part of the struct
    }
    
    fn get_expiration_date(&self) -> Option<DateTime<Utc>> {
        Some(self.expiry)
    }
    
    fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| e.to_string())
    }
}

/// Implement VerifiableCredential trait for ExecutionReceipt
impl icn_identity::VerifiableCredential for ExecutionReceipt {
    fn get_type(&self) -> String {
        "MeshExecutionReceipt".to_string()
    }
    
    fn get_issuer(&self) -> Did {
        self.worker_did.clone()
    }
    
    fn get_issuance_date(&self) -> DateTime<Utc> {
        self.timestamp
    }
    
    fn get_expiration_date(&self) -> Option<DateTime<Utc>> {
        None // Receipts don't expire
    }
    
    fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| e.to_string())
    }
}

/// Implement VerifiableCredential trait for VerificationReceipt
impl icn_identity::VerifiableCredential for VerificationReceipt {
    fn get_type(&self) -> String {
        "MeshVerificationReceipt".to_string()
    }
    
    fn get_issuer(&self) -> Did {
        self.verifier_did.clone()
    }
    
    fn get_issuance_date(&self) -> DateTime<Utc> {
        self.timestamp
    }
    
    fn get_expiration_date(&self) -> Option<DateTime<Utc>> {
        None // Receipts don't expire
    }
    
    fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| e.to_string())
    }
}

// Export additional types to make them available for use
pub mod events {
    use super::*;
    
    /// Events emitted by the mesh network
    #[derive(Debug, Clone)]
    pub enum MeshEvent {
        /// A new task has been published
        TaskPublished(TaskIntent),
        
        /// A worker has offered to execute a task
        OfferReceived(ComputeOffer),
        
        /// A task has been executed
        TaskExecuted(ExecutionReceipt),
        
        /// A task execution has been verified
        TaskVerified(VerificationReceipt),
        
        /// A peer has joined the network
        PeerJoined(PeerInfo),
        
        /// A peer has left the network
        PeerLeft(Did),
        
        /// Reputation of a peer has been updated
        ReputationUpdated(ReputationSnapshot),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_task_intent_serialization() {
        let task = TaskIntent {
            publisher_did: "did:icn:test:publisher".to_string(),
            wasm_cid: Cid::default(),
            input_cid: Cid::default(),
            fee: 100,
            verifiers: 3,
            expiry: Utc::now(),
            metadata: None,
        };
        
        let json = serde_json::to_string(&task).unwrap();
        let deserialized: TaskIntent = serde_json::from_str(&json).unwrap();
        
        assert_eq!(task.publisher_did, deserialized.publisher_did);
        assert_eq!(task.fee, deserialized.fee);
        assert_eq!(task.verifiers, deserialized.verifiers);
    }
}
