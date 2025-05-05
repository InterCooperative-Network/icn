use chrono::{DateTime, Utc};
use cid::Cid;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Export task runner module
pub mod task_runner;
pub use task_runner::*;

/// Type alias for DID string
pub type Did = String;

/// Hardware capabilities information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HwCaps {
    /// Available memory in MB
    pub mem_mb: u32,
    
    /// Available CPU cycles (relative measure)
    pub cpu_cycles: u32,
    
    /// Available GPU operations (in FLOPS)
    pub gpu_flops: u32,
    
    /// Available I/O bandwidth (in MB)
    pub io_mb: u32,
}

/// Capability scope defines the resources required for task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityScope {
    /// Required memory in MB
    pub mem_mb: u32,
    
    /// Required CPU cycles (relative measure)
    pub cpu_cycles: u32,
    
    /// Required GPU operations (in FLOPS)
    pub gpu_flops: u32,
    
    /// Required I/O bandwidth (in MB)
    pub io_mb: u32,
}

impl CapabilityScope {
    /// Check if these capabilities fit within the provided hardware capabilities
    pub fn fits(&self, hw_caps: &HwCaps) -> bool {
        self.mem_mb <= hw_caps.mem_mb &&
        self.cpu_cycles <= hw_caps.cpu_cycles &&
        self.gpu_flops <= hw_caps.gpu_flops &&
        self.io_mb <= hw_caps.io_mb
    }
}

/// Execution summary containing information about resource usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSummary {
    /// Memory used in MB
    pub mem_mb: u32,
    
    /// CPU cycles used (relative measure)
    pub cpu_cycles: u32,
    
    /// GPU operations used (in FLOPS)
    pub gpu_flops: u32,
    
    /// I/O bandwidth used (in MB)
    pub io_mb: u32,
    
    /// Overall contribution score (0.0-1.0)
    pub contribution_score: f64,
}

/// A participation intent representing a request to execute a WASM module in the mesh network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticipationIntent {
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
    
    /// Required capability scope for execution
    pub capability_scope: CapabilityScope,
    
    /// Content ID of the escrow contract (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub escrow_cid: Option<Cid>,
    
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
    
    /// Raw hash of the output data (for deterministic verification)
    pub output_hash: Vec<u8>,
    
    /// Summary of resources consumed during execution
    pub execution_summary: ExecutionSummary,
    
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
    
    /// Base capability scope representing computational resources
    pub base_capability_scope: CapabilityScope,
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
    
    /// Available hardware capabilities of the worker
    pub available_hw_caps: HwCaps,
    
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
    
    /// Available hardware capabilities of the peer
    pub hw_caps: HwCaps,
    
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

// Export additional types to make them available for use
pub mod events {
    use super::*;
    
    /// Events emitted by the mesh network
    #[derive(Debug, Clone)]
    pub enum MeshEvent {
        /// A new task has been published
        ParticipationRequested(ParticipationIntent),
        
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
    fn test_participation_intent_serialization() {
        let intent = ParticipationIntent {
            publisher_did: "did:icn:test:publisher".to_string(),
            wasm_cid: Cid::default(),
            input_cid: Cid::default(),
            fee: 100,
            verifiers: 3,
            expiry: Utc::now(),
            capability_scope: CapabilityScope {
                mem_mb: 512,
                cpu_cycles: 1000,
                gpu_flops: 0,
                io_mb: 100,
            },
            escrow_cid: None,
            metadata: None,
        };
        
        let json = serde_json::to_string(&intent).unwrap();
        let deserialized: ParticipationIntent = serde_json::from_str(&json).unwrap();
        
        assert_eq!(intent.publisher_did, deserialized.publisher_did);
        assert_eq!(intent.fee, deserialized.fee);
        assert_eq!(intent.verifiers, deserialized.verifiers);
        assert_eq!(intent.capability_scope.mem_mb, deserialized.capability_scope.mem_mb);
    }
    
    #[test]
    fn test_capability_scope_fits() {
        let capability = CapabilityScope {
            mem_mb: 512,
            cpu_cycles: 1000,
            gpu_flops: 0,
            io_mb: 100,
        };
        
        let hw_sufficient = HwCaps {
            mem_mb: 1024,
            cpu_cycles: 2000,
            gpu_flops: 0,
            io_mb: 200,
        };
        
        let hw_insufficient = HwCaps {
            mem_mb: 256,
            cpu_cycles: 2000,
            gpu_flops: 0,
            io_mb: 200,
        };
        
        assert!(capability.fits(&hw_sufficient));
        assert!(!capability.fits(&hw_insufficient));
    }
}
