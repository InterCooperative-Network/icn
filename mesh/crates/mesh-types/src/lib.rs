use chrono::{DateTime, Utc};
use cid::Cid;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Export task runner module
pub mod task_runner;
pub use task_runner::*;

// Export policy manager module
pub mod policy_manager;
pub use policy_manager::*;

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
    /// Policy version - increments with each update
    pub policy_version: u32,
    
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
    
    /// Reward distribution settings
    pub reward_settings: RewardSettings,
    
    /// Bonding requirements for participants
    pub bonding_requirements: BondingRequirements,
    
    /// Scheduling parameters
    pub scheduling_params: SchedulingParams,
    
    /// Verification quorum requirements
    pub verification_quorum: VerificationQuorum,
    
    /// Content ID of the previous policy version (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_policy_cid: Option<Cid>,
    
    /// Timestamp when this policy was activated
    pub activation_timestamp: DateTime<Utc>,
    
    /// Federation DID that this policy belongs to
    pub federation_did: Did,
}

impl MeshPolicy {
    /// Create a new default policy for a federation
    pub fn new_default(federation_did: &str) -> Self {
        Self {
            policy_version: 1,
            alpha: 0.6,
            beta: 0.3,
            gamma: 0.1,
            lambda: 0.05,
            stake_weight: 0.4,
            min_fee: 10,
            base_capability_scope: CapabilityScope {
                mem_mb: 128,
                cpu_cycles: 1_000_000,
                gpu_flops: 0,
                io_mb: 50,
            },
            reward_settings: RewardSettings::default(),
            bonding_requirements: BondingRequirements::default(),
            scheduling_params: SchedulingParams::default(),
            verification_quorum: VerificationQuorum::default(),
            previous_policy_cid: None,
            activation_timestamp: Utc::now(),
            federation_did: federation_did.to_string(),
        }
    }
    
    /// Apply a policy fragment update to this policy
    pub fn apply_update(&mut self, fragment: &MeshPolicyFragment) -> Result<(), String> {
        // Increment policy version
        self.policy_version += 1;
        
        // Apply reputation parameters if present
        if let Some(params) = &fragment.reputation_params {
            if let Some(alpha) = params.alpha {
                self.alpha = alpha;
            }
            if let Some(beta) = params.beta {
                self.beta = beta;
            }
            if let Some(gamma) = params.gamma {
                self.gamma = gamma;
            }
            if let Some(lambda) = params.lambda {
                self.lambda = lambda;
            }
        }
        
        // Apply reward settings if present
        if let Some(settings) = &fragment.reward_settings {
            // Validate that percentages add up to 100
            if let (Some(worker), Some(verifier), Some(platform)) = (
                settings.worker_percentage,
                settings.verifier_percentage,
                settings.platform_fee_percentage,
            ) {
                if worker + verifier + platform != 100 {
                    return Err("Reward percentages must add up to 100".to_string());
                }
            }
            
            if let Some(worker) = settings.worker_percentage {
                self.reward_settings.worker_percentage = worker;
            }
            if let Some(verifier) = settings.verifier_percentage {
                self.reward_settings.verifier_percentage = verifier;
            }
            if let Some(platform) = settings.platform_fee_percentage {
                self.reward_settings.platform_fee_percentage = platform;
            }
            if let Some(weighting) = settings.use_reputation_weighting {
                self.reward_settings.use_reputation_weighting = weighting;
            }
            if let Some(address) = &settings.platform_fee_address {
                self.reward_settings.platform_fee_address = address.clone();
            }
        }
        
        // Apply bonding requirements if present
        if let Some(bonding) = &fragment.bonding_requirements {
            if let Some(min_stake) = bonding.min_stake_amount {
                self.bonding_requirements.min_stake_amount = min_stake;
            }
            if let Some(lock_period) = bonding.min_lock_period_days {
                self.bonding_requirements.min_lock_period_days = lock_period;
            }
            if let Some(tokens) = &bonding.allowed_token_types {
                self.bonding_requirements.allowed_token_types = tokens.clone();
            }
        }
        
        // Apply scheduling parameters if present
        if let Some(scheduling) = &fragment.scheduling_params {
            if let Some(fair_queue) = scheduling.use_fair_queuing {
                self.scheduling_params.use_fair_queuing = fair_queue;
            }
            if let Some(queue_limit) = scheduling.max_queue_length {
                self.scheduling_params.max_queue_length = queue_limit;
            }
            if let Some(priority_boost) = scheduling.reputation_priority_boost {
                self.scheduling_params.reputation_priority_boost = priority_boost;
            }
            if let Some(timeout) = scheduling.task_timeout_minutes {
                self.scheduling_params.task_timeout_minutes = timeout;
            }
            if let Some(scope) = &scheduling.default_capability_scope {
                self.scheduling_params.default_capability_scope = scope.clone();
            }
        }
        
        // Apply verification quorum if present
        if let Some(quorum) = &fragment.verification_quorum {
            if let Some(req) = quorum.required_percentage {
                if req < 50 || req > 100 {
                    return Err("Quorum percentage must be between 50 and 100".to_string());
                }
                self.verification_quorum.required_percentage = req;
            }
            if let Some(min) = quorum.minimum_verifiers {
                self.verification_quorum.minimum_verifiers = min;
            }
            if let Some(max) = quorum.maximum_verifiers {
                self.verification_quorum.maximum_verifiers = max;
            }
            if let Some(timeout) = quorum.verification_timeout_minutes {
                self.verification_quorum.verification_timeout_minutes = timeout;
            }
        }
        
        // Apply base capability scope if present
        if let Some(scope) = &fragment.base_capability_scope {
            if let Some(mem) = scope.mem_mb {
                self.base_capability_scope.mem_mb = mem;
            }
            if let Some(cpu) = scope.cpu_cycles {
                self.base_capability_scope.cpu_cycles = cpu;
            }
            if let Some(gpu) = scope.gpu_flops {
                self.base_capability_scope.gpu_flops = gpu;
            }
            if let Some(io) = scope.io_mb {
                self.base_capability_scope.io_mb = io;
            }
        }
        
        // Apply other simple parameters
        if let Some(stake_weight) = fragment.stake_weight {
            self.stake_weight = stake_weight;
        }
        if let Some(min_fee) = fragment.min_fee {
            self.min_fee = min_fee;
        }
        
        // Update timestamp
        self.activation_timestamp = Utc::now();
        
        Ok(())
    }
}

/// Reward distribution settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardSettings {
    /// Percentage of total reward for worker (0-100)
    pub worker_percentage: u8,
    
    /// Percentage of total reward for verifiers (0-100)
    pub verifier_percentage: u8,
    
    /// Percentage of total reward for platform fee (0-100)
    pub platform_fee_percentage: u8,
    
    /// Use reputation weighting for distributing verifier rewards
    pub use_reputation_weighting: bool,
    
    /// Platform fee address
    pub platform_fee_address: String,
}

impl Default for RewardSettings {
    fn default() -> Self {
        Self {
            worker_percentage: 70,
            verifier_percentage: 20,
            platform_fee_percentage: 10,
            use_reputation_weighting: true,
            platform_fee_address: "default-platform-fee-address".to_string(),
        }
    }
}

/// Bonding requirements for participants
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BondingRequirements {
    /// Minimum amount of tokens to stake
    pub min_stake_amount: u64,
    
    /// Minimum period tokens must be locked (in days)
    pub min_lock_period_days: u32,
    
    /// Allowed token types for staking
    pub allowed_token_types: Vec<String>,
}

impl Default for BondingRequirements {
    fn default() -> Self {
        Self {
            min_stake_amount: 100,
            min_lock_period_days: 7,
            allowed_token_types: vec!["ICN".to_string()],
        }
    }
}

/// Scheduling parameters for task distribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingParams {
    /// Whether to use fair queuing for tasks
    pub use_fair_queuing: bool,
    
    /// Maximum length of task queue per worker
    pub max_queue_length: u32,
    
    /// Priority boost factor based on reputation
    pub reputation_priority_boost: f64,
    
    /// Default timeout for task execution (minutes)
    pub task_timeout_minutes: u32,
    
    /// Default capability scope if not specified
    pub default_capability_scope: CapabilityScope,
}

impl Default for SchedulingParams {
    fn default() -> Self {
        Self {
            use_fair_queuing: true,
            max_queue_length: 100,
            reputation_priority_boost: 1.5,
            task_timeout_minutes: 30,
            default_capability_scope: CapabilityScope {
                mem_mb: 128,
                cpu_cycles: 1_000_000,
                gpu_flops: 0,
                io_mb: 50,
            },
        }
    }
}

/// Verification quorum requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationQuorum {
    /// Required percentage of verifiers to agree (50-100)
    pub required_percentage: u8,
    
    /// Minimum number of verifiers required
    pub minimum_verifiers: u32,
    
    /// Maximum number of verifiers allowed
    pub maximum_verifiers: u32,
    
    /// Timeout for verification (minutes)
    pub verification_timeout_minutes: u32,
}

impl Default for VerificationQuorum {
    fn default() -> Self {
        Self {
            required_percentage: 66, // Two-thirds majority
            minimum_verifiers: 3,
            maximum_verifiers: 7,
            verification_timeout_minutes: 15,
        }
    }
}

/// Partial update for capability scope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityScopeFragment {
    /// Memory in MB
    pub mem_mb: Option<u32>,
    
    /// CPU cycles
    pub cpu_cycles: Option<u32>,
    
    /// GPU operations (in FLOPS)
    pub gpu_flops: Option<u32>,
    
    /// I/O bandwidth (in MB)
    pub io_mb: Option<u32>,
}

/// Partial update for reputation parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationParamsFragment {
    /// Alpha parameter: weight given to execution performance
    pub alpha: Option<f64>,
    
    /// Beta parameter: weight given to verification accuracy
    pub beta: Option<f64>,
    
    /// Gamma parameter: penalty factor for incorrect verifications
    pub gamma: Option<f64>,
    
    /// Lambda parameter: reputation decay rate
    pub lambda: Option<f64>,
}

/// Partial update for reward settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardSettingsFragment {
    /// Percentage for worker
    pub worker_percentage: Option<u8>,
    
    /// Percentage for verifiers
    pub verifier_percentage: Option<u8>,
    
    /// Percentage for platform fee
    pub platform_fee_percentage: Option<u8>,
    
    /// Use reputation weighting
    pub use_reputation_weighting: Option<bool>,
    
    /// Platform fee address
    pub platform_fee_address: Option<String>,
}

/// Partial update for bonding requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BondingRequirementsFragment {
    /// Minimum stake amount
    pub min_stake_amount: Option<u64>,
    
    /// Minimum lock period
    pub min_lock_period_days: Option<u32>,
    
    /// Allowed token types
    pub allowed_token_types: Option<Vec<String>>,
}

/// Partial update for scheduling parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingParamsFragment {
    /// Use fair queuing
    pub use_fair_queuing: Option<bool>,
    
    /// Maximum queue length
    pub max_queue_length: Option<u32>,
    
    /// Reputation priority boost
    pub reputation_priority_boost: Option<f64>,
    
    /// Task timeout in minutes
    pub task_timeout_minutes: Option<u32>,
    
    /// Default capability scope
    pub default_capability_scope: Option<CapabilityScope>,
}

/// Partial update for verification quorum
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationQuorumFragment {
    /// Required percentage
    pub required_percentage: Option<u8>,
    
    /// Minimum verifiers
    pub minimum_verifiers: Option<u32>,
    
    /// Maximum verifiers
    pub maximum_verifiers: Option<u32>,
    
    /// Verification timeout in minutes
    pub verification_timeout_minutes: Option<u32>,
}

/// A fragment of a mesh policy for updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshPolicyFragment {
    /// Reputation parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reputation_params: Option<ReputationParamsFragment>,
    
    /// Stake weight
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stake_weight: Option<f64>,
    
    /// Minimum fee
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_fee: Option<u64>,
    
    /// Base capability scope
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_capability_scope: Option<CapabilityScopeFragment>,
    
    /// Reward settings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reward_settings: Option<RewardSettingsFragment>,
    
    /// Bonding requirements
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bonding_requirements: Option<BondingRequirementsFragment>,
    
    /// Scheduling parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduling_params: Option<SchedulingParamsFragment>,
    
    /// Verification quorum
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_quorum: Option<VerificationQuorumFragment>,
    
    /// Description of the update
    pub description: String,
    
    /// Proposer DID
    pub proposer_did: Did,
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
