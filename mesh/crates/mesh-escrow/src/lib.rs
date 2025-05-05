use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use cid::Cid;
use icn_dag::{DagEvent, DagManager};
use icn_identity::Did;
use mesh_reputation::ReputationInterface;
use mesh_types::{
    ExecutionReceipt, TaskIntent, VerificationReceipt, events::MeshEvent,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tracing::{debug, error, info, warn};

// Export contract and token modules
pub mod contracts;
pub mod payments;
pub mod errors;

#[cfg(test)]
pub mod tests;

/// Current state of an escrow contract
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EscrowState {
    /// Contract created, tokens locked
    Created,
    
    /// Task is in progress
    InProgress,
    
    /// Task completed successfully, tokens released
    Completed,
    
    /// Task disputed, resolution pending
    Disputed,
    
    /// Contract failed, tokens returned (minus fees)
    Failed,
    
    /// Contract expired, tokens returned (minus fees)
    Expired,
}

/// A token amount with decimal precision
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenAmount {
    /// The amount in the smallest unit
    pub value: u64,
    
    /// Decimal precision (e.g., 6 for millionths)
    pub decimals: u8,
}

impl TokenAmount {
    /// Create a new token amount
    pub fn new(value: u64, decimals: u8) -> Self {
        Self { value, decimals }
    }
    
    /// Add two token amounts (assumes same decimal precision)
    pub fn add(&self, other: &Self) -> Result<Self> {
        if self.decimals != other.decimals {
            return Err(anyhow!("Cannot add token amounts with different decimal precision"));
        }
        
        Ok(Self {
            value: self.value.checked_add(other.value)
                .ok_or_else(|| anyhow!("Token amount addition overflow"))?,
            decimals: self.decimals,
        })
    }
    
    /// Subtract one token amount from another (assumes same decimal precision)
    pub fn sub(&self, other: &Self) -> Result<Self> {
        if self.decimals != other.decimals {
            return Err(anyhow!("Cannot subtract token amounts with different decimal precision"));
        }
        
        Ok(Self {
            value: self.value.checked_sub(other.value)
                .ok_or_else(|| anyhow!("Token amount subtraction underflow"))?,
            decimals: self.decimals,
        })
    }
    
    /// Multiply token amount by a scalar value
    pub fn mul(&self, scalar: f64) -> Result<Self> {
        let new_value = (self.value as f64 * scalar).round() as u64;
        
        Ok(Self {
            value: new_value,
            decimals: self.decimals,
        })
    }
}

/// Smart contract controlling token escrow for task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscrowContract {
    /// Unique identifier for this contract
    pub id: String,
    
    /// The DID of the task publisher
    pub publisher_did: Did,
    
    /// Content ID of the associated task
    pub task_cid: Cid,
    
    /// Total tokens locked in escrow
    pub token_amount: TokenAmount,
    
    /// Current state of the contract
    pub state: EscrowState,
    
    /// Timestamp when the contract was created
    pub created_at: DateTime<Utc>,
    
    /// Timestamp when the contract expires
    pub expires_at: DateTime<Utc>,
    
    /// Content ID of the execution receipt (if task completed)
    pub execution_receipt_cid: Option<Cid>,
    
    /// DIDs of verifiers and their verification status
    pub verifications: HashMap<Did, bool>,
    
    /// Quorum required for task verification (percentage 0-100)
    pub verification_quorum: u8,
    
    /// Maximum number of verifiers allowed
    pub max_verifiers: u8,
    
    /// Reward distribution settings
    pub reward_settings: RewardSettings,
    
    /// Additional contract metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

/// Settings for reward distribution
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

impl RewardSettings {
    /// Create default reward settings
    pub fn default() -> Self {
        Self {
            worker_percentage: 70,
            verifier_percentage: 20,
            platform_fee_percentage: 10,
            use_reputation_weighting: true,
            platform_fee_address: "fee-address".to_string(),
        }
    }
    
    /// Validate that percentages add up to 100
    pub fn validate(&self) -> bool {
        self.worker_percentage + self.verifier_percentage + self.platform_fee_percentage == 100
    }
}

/// A token lock for a specific task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenLock {
    /// Unique identifier for this lock
    pub id: String,
    
    /// The DID of the token owner
    pub owner_did: Did,
    
    /// Associated escrow contract ID
    pub contract_id: String,
    
    /// Amount of tokens locked
    pub amount: TokenAmount,
    
    /// Timestamp when the lock was created
    pub created_at: DateTime<Utc>,
    
    /// Timestamp when the lock expires
    pub expires_at: DateTime<Utc>,
    
    /// Whether the lock has been released
    pub released: bool,
    
    /// Timestamp when the lock was released (if applicable)
    pub released_at: Option<DateTime<Utc>>,
}

/// Result of reward distribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardDistribution {
    /// Contract ID that generated this distribution
    pub contract_id: String,
    
    /// Associated task CID
    pub task_cid: Cid,
    
    /// Total amount of tokens distributed
    pub total_amount: TokenAmount,
    
    /// Timestamp when the distribution occurred
    pub timestamp: DateTime<Utc>,
    
    /// Worker reward
    pub worker_reward: WorkerReward,
    
    /// Verifier rewards
    pub verifier_rewards: Vec<VerifierReward>,
    
    /// Platform fee amount
    pub platform_fee: TokenAmount,
}

/// Reward for a worker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerReward {
    /// DID of the worker
    pub worker_did: Did,
    
    /// Amount of tokens rewarded
    pub amount: TokenAmount,
}

/// Reward for a verifier
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifierReward {
    /// DID of the verifier
    pub verifier_did: Did,
    
    /// Amount of tokens rewarded
    pub amount: TokenAmount,
    
    /// Reputation weight used in calculation
    pub reputation_weight: Option<f64>,
}

/// Events related to escrow contracts
#[derive(Debug, Clone)]
pub enum EscrowEvent {
    /// Escrow contract created
    ContractCreated(EscrowContract),
    
    /// Tokens locked in escrow
    TokensLocked(TokenLock),
    
    /// Tokens released from escrow
    TokensReleased(TokenLock),
    
    /// Rewards distributed
    RewardsDistributed(RewardDistribution),
    
    /// Contract state changed
    ContractStateChanged {
        /// Contract ID
        contract_id: String,
        /// Previous state
        previous_state: EscrowState,
        /// New state
        new_state: EscrowState,
        /// Timestamp
        timestamp: DateTime<Utc>,
    },
    
    /// Contract disputed
    ContractDisputed {
        /// Contract ID
        contract_id: String,
        /// Disputer DID
        disputer_did: Did,
        /// Dispute reason
        reason: String,
        /// Timestamp
        timestamp: DateTime<Utc>,
    },
}

/// Interface for escrow system implementations
#[async_trait]
pub trait EscrowInterface: Send + Sync {
    /// Create a new escrow contract for a task
    async fn create_contract(&self, task: &TaskIntent, settings: RewardSettings) -> Result<EscrowContract>;
    
    /// Lock tokens in escrow
    async fn lock_tokens(&self, contract_id: &str, amount: TokenAmount) -> Result<TokenLock>;
    
    /// Process an execution receipt
    async fn process_execution(&self, contract_id: &str, receipt: &ExecutionReceipt) -> Result<()>;
    
    /// Process a verification receipt
    async fn process_verification(&self, contract_id: &str, receipt: &VerificationReceipt) -> Result<()>;
    
    /// Calculate and distribute rewards
    async fn distribute_rewards(&self, contract_id: &str) -> Result<RewardDistribution>;
    
    /// Create a dispute for a contract
    async fn create_dispute(&self, contract_id: &str, disputer_did: &Did, reason: &str) -> Result<()>;
    
    /// Get current contract state
    async fn get_contract_state(&self, contract_id: &str) -> Result<EscrowState>;
    
    /// Check if a contract is completed
    async fn is_contract_completed(&self, contract_id: &str) -> Result<bool>;
}

/// DAG event for escrow contract data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscrowContractEvent {
    /// Contract ID
    pub contract_id: String,
    
    /// Event type
    pub event_type: String,
    
    /// Content ID of the contract data
    pub contract_cid: Cid,
    
    /// Timestamp
    pub timestamp: DateTime<Utc>,
}

/// Claims rewards for a successful worker after verification
pub async fn claim_reward(escrow_cid: &Cid, worker_did: &str, reward_amount: u64) -> Result<()> {
    info!("Claiming reward of {} tokens for worker {} from escrow {}", 
          reward_amount, worker_did, escrow_cid);
    
    // Call the compute_escrow contract's release action
    execute_escrow_action(
        escrow_cid, 
        "release", 
        &[worker_did.to_string(), reward_amount.to_string()]
    ).await?;
    
    // Anchor event to DAG
    anchor_escrow_event(
        escrow_cid,
        "claimed",
        Some(worker_did),
        Some(reward_amount)
    ).await?;
    
    info!("Reward claim successful");
    Ok(())
}

/// Refunds tokens for an invalid execution
pub async fn refund(escrow_cid: &Cid) -> Result<()> {
    info!("Refunding tokens for escrow {}", escrow_cid);
    
    // Call the compute_escrow contract's refund action
    execute_escrow_action(
        escrow_cid, 
        "refund", 
        &[]
    ).await?;
    
    // Anchor event to DAG
    anchor_escrow_event(
        escrow_cid,
        "refunded",
        None,
        None
    ).await?;
    
    info!("Refund successful");
    Ok(())
}

/// Helper to execute an action on the escrow contract
async fn execute_escrow_action(escrow_cid: &Cid, action: &str, params: &[String]) -> Result<()> {
    // In a real implementation, this would:
    // 1. Use the CCL interpreter to execute the contract action
    // 2. Update the contract state in storage
    
    // For now, we just log that it happened
    info!("Executing escrow action: {} with params: {:?}", action, params);
    Ok(())
}

/// Helper to anchor an escrow event to the DAG
async fn anchor_escrow_event(
    escrow_cid: &Cid,
    event_type: &str,
    worker_did: Option<&str>,
    amount: Option<u64>
) -> Result<()> {
    // In a real implementation, this would:
    // 1. Create a DagEvent::MeshEscrowEvent
    // 2. Anchor it to the DAG
    
    // For now, we just log that it happened
    info!("Anchoring escrow event: {} for worker {:?} with amount {:?}", 
          event_type, worker_did, amount);
    Ok(())
}

#[cfg(test)]
mod base_tests {
    use super::*;
    
    #[test]
    fn test_token_amount_operations() {
        let a = TokenAmount::new(100, 6);
        let b = TokenAmount::new(50, 6);
        
        let sum = a.add(&b).unwrap();
        assert_eq!(sum.value, 150);
        
        let diff = a.sub(&b).unwrap();
        assert_eq!(diff.value, 50);
        
        let scaled = a.mul(0.5).unwrap();
        assert_eq!(scaled.value, 50);
    }
    
    #[test]
    fn test_reward_settings_validation() {
        let valid = RewardSettings {
            worker_percentage: 70,
            verifier_percentage: 20,
            platform_fee_percentage: 10,
            use_reputation_weighting: true,
            platform_fee_address: "valid".to_string(),
        };
        assert!(valid.validate());
        
        let invalid = RewardSettings {
            worker_percentage: 70,
            verifier_percentage: 20,
            platform_fee_percentage: 15,
            use_reputation_weighting: true,
            platform_fee_address: "invalid".to_string(),
        };
        assert!(!invalid.validate());
    }
} 