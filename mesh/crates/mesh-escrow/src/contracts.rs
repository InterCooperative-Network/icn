use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use cid::Cid;
use icn_dag::{DagEvent, DagManager};
use icn_identity::Did;
use mesh_reputation::ReputationInterface;
use mesh_types::{ExecutionReceipt, TaskIntent, VerificationReceipt};
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::{
    EscrowContract, EscrowContractEvent, EscrowEvent, EscrowInterface, EscrowState,
    RewardDistribution, RewardSettings, TokenAmount, TokenLock, WorkerReward, VerifierReward,
};

/// Implementation of the escrow system
pub struct EscrowSystem {
    /// Active contracts by ID
    contracts: Arc<Mutex<HashMap<String, EscrowContract>>>,
    
    /// Token locks by ID
    locks: Arc<Mutex<HashMap<String, TokenLock>>>,
    
    /// Reward distributions by contract ID
    distributions: Arc<Mutex<HashMap<String, RewardDistribution>>>,
    
    /// Reputation system reference
    reputation: Arc<dyn ReputationInterface>,
    
    /// DAG manager for anchoring events
    dag: Option<Arc<dyn DagManager>>,
}

impl EscrowSystem {
    /// Create a new escrow system
    pub fn new(reputation: Arc<dyn ReputationInterface>) -> Self {
        Self {
            contracts: Arc::new(Mutex::new(HashMap::new())),
            locks: Arc::new(Mutex::new(HashMap::new())),
            distributions: Arc::new(Mutex::new(HashMap::new())),
            reputation,
            dag: None,
        }
    }
    
    /// Set the DAG manager for anchoring events
    pub fn with_dag(mut self, dag: Arc<dyn DagManager>) -> Self {
        self.dag = Some(dag);
        self
    }
    
    /// Generate a unique contract ID
    fn generate_contract_id() -> String {
        Uuid::new_v4().to_string()
    }
    
    /// Generate a unique lock ID
    fn generate_lock_id() -> String {
        Uuid::new_v4().to_string()
    }
    
    /// Update contract state
    async fn update_contract_state(&self, contract_id: &str, new_state: EscrowState) -> Result<()> {
        let mut contracts = self.contracts.lock().unwrap();
        
        let contract = contracts.get_mut(contract_id)
            .ok_or_else(|| anyhow!("Contract not found: {}", contract_id))?;
        
        let previous_state = contract.state.clone();
        contract.state = new_state.clone();
        
        // Log the state change
        info!("Contract {} state changed: {:?} -> {:?}", contract_id, previous_state, new_state);
        
        // Emit state change event
        let event = EscrowEvent::ContractStateChanged {
            contract_id: contract_id.to_string(),
            previous_state,
            new_state,
            timestamp: Utc::now(),
        };
        
        // TODO: publish event to subscribers
        
        // Anchor to DAG if available
        if let Some(ref dag) = self.dag {
            let dag_event = DagEvent::EscrowStateChanged {
                contract_id: contract_id.to_string(),
                status: format!("{:?}", new_state),
                timestamp: Utc::now(),
            };
            
            // Submit to DAG
            match dag.submit_event(dag_event).await {
                Ok(_) => debug!("Anchored escrow state change to DAG"),
                Err(e) => warn!("Failed to anchor escrow state change to DAG: {}", e),
            }
        }
        
        Ok(())
    }
    
    /// Check if verification quorum has been reached
    fn check_verification_quorum(&self, contract_id: &str) -> Result<bool> {
        let contracts = self.contracts.lock().unwrap();
        
        let contract = contracts.get(contract_id)
            .ok_or_else(|| anyhow!("Contract not found: {}", contract_id))?;
        
        // Get total verifications and positive verifications
        let total_verifications = contract.verifications.len();
        let positive_verifications = contract.verifications.values()
            .filter(|&&v| v)
            .count();
        
        // Calculate percentage of positive verifications
        if total_verifications == 0 {
            return Ok(false);
        }
        
        let positive_percentage = 
            (positive_verifications as f64 / total_verifications as f64) * 100.0;
        
        // Check if quorum is reached
        Ok(positive_percentage >= contract.verification_quorum as f64)
    }
    
    /// Check for and process contract expiry
    async fn check_contract_expiry(&self, contract_id: &str) -> Result<bool> {
        let contracts = self.contracts.lock().unwrap();
        
        let contract = contracts.get(contract_id)
            .ok_or_else(|| anyhow!("Contract not found: {}", contract_id))?;
        
        // Check if contract is expired
        let now = Utc::now();
        let is_expired = now > contract.expires_at;
        
        drop(contracts); // Release lock before update_contract_state
        
        // If expired and not already in a final state, update state
        if is_expired {
            let current_state = self.get_contract_state(contract_id).await?;
            
            match current_state {
                EscrowState::Created | EscrowState::InProgress => {
                    self.update_contract_state(contract_id, EscrowState::Expired).await?;
                    
                    // Return tokens to publisher (not implemented here)
                    // This would trigger refund logic
                    
                    info!("Contract {} expired, refunding tokens to publisher", contract_id);
                },
                _ => {} // Already in a final state, nothing to do
            }
        }
        
        Ok(is_expired)
    }
    
    /// Anchor contract to DAG
    async fn anchor_contract_to_dag(&self, contract: &EscrowContract, event_type: &str) -> Result<()> {
        if let Some(ref dag) = self.dag {
            // Serialize contract to JSON for storage
            let contract_json = serde_json::to_string(contract)?;
            
            // Create a temporary CID (in a real implementation, this would be stored in IPFS/IPLD)
            let contract_cid = icn_common::utils::calculate_dummy_cid_for_json(&contract_json)?;
            
            // Create DAG event
            let dag_event = DagEvent::EscrowContract {
                contract_id: contract.id.clone(),
                event_type: event_type.to_string(),
                contract_cid: contract_cid.clone(),
                timestamp: Utc::now(),
            };
            
            // Submit to DAG
            match dag.submit_event(dag_event).await {
                Ok(_) => debug!("Anchored escrow contract to DAG"),
                Err(e) => warn!("Failed to anchor escrow contract to DAG: {}", e),
            }
        }
        
        Ok(())
    }
}

#[async_trait]
impl EscrowInterface for EscrowSystem {
    async fn create_contract(&self, task: &TaskIntent, settings: RewardSettings) -> Result<EscrowContract> {
        // Validate settings
        if !settings.validate() {
            return Err(anyhow!("Invalid reward settings: percentages must add up to 100"));
        }
        
        // Generate a new contract ID
        let contract_id = Self::generate_contract_id();
        
        // Create expiry date (typically matching task expiry)
        let expires_at = task.expiry;
        
        // Create the contract
        let contract = EscrowContract {
            id: contract_id,
            publisher_did: task.publisher_did.clone(),
            task_cid: task.wasm_cid.clone(), // Assuming task_cid is the wasm_cid
            token_amount: TokenAmount::new(task.fee, 6), // Assuming 6 decimal places
            state: EscrowState::Created,
            created_at: Utc::now(),
            expires_at,
            execution_receipt_cid: None,
            verifications: HashMap::new(),
            verification_quorum: 51, // Default to 51% quorum
            max_verifiers: task.verifiers as u8,
            reward_settings: settings,
            metadata: None,
        };
        
        // Store the contract
        {
            let mut contracts = self.contracts.lock().unwrap();
            contracts.insert(contract.id.clone(), contract.clone());
        }
        
        // Emit contract created event
        let event = EscrowEvent::ContractCreated(contract.clone());
        // TODO: publish event to subscribers
        
        // Anchor to DAG
        self.anchor_contract_to_dag(&contract, "created").await?;
        
        info!("Created escrow contract {} for task {}", contract.id, task.wasm_cid);
        
        Ok(contract)
    }
    
    async fn lock_tokens(&self, contract_id: &str, amount: TokenAmount) -> Result<TokenLock> {
        let contracts = self.contracts.lock().unwrap();
        
        // Get the contract
        let contract = contracts.get(contract_id)
            .ok_or_else(|| anyhow!("Contract not found: {}", contract_id))?;
        
        // Check if contract is in a state where tokens can be locked
        if contract.state != EscrowState::Created {
            return Err(anyhow!("Cannot lock tokens for contract in state: {:?}", contract.state));
        }
        
        // Create token lock
        let lock = TokenLock {
            id: Self::generate_lock_id(),
            owner_did: contract.publisher_did.clone(),
            contract_id: contract_id.to_string(),
            amount,
            created_at: Utc::now(),
            expires_at: contract.expires_at,
            released: false,
            released_at: None,
        };
        
        // Store lock
        {
            let mut locks = self.locks.lock().unwrap();
            locks.insert(lock.id.clone(), lock.clone());
        }
        
        // Update contract state
        drop(contracts); // Release the contract lock before calling update_contract_state
        self.update_contract_state(contract_id, EscrowState::InProgress).await?;
        
        // Emit tokens locked event
        let event = EscrowEvent::TokensLocked(lock.clone());
        // TODO: publish event to subscribers
        
        info!("Locked {} tokens for contract {}", amount.value, contract_id);
        
        Ok(lock)
    }
    
    async fn process_execution(&self, contract_id: &str, receipt: &ExecutionReceipt) -> Result<()> {
        let mut contracts = self.contracts.lock().unwrap();
        
        // Get the contract
        let contract = contracts.get_mut(contract_id)
            .ok_or_else(|| anyhow!("Contract not found: {}", contract_id))?;
        
        // Check if contract is in progress
        if contract.state != EscrowState::InProgress {
            return Err(anyhow!("Cannot process execution for contract in state: {:?}", contract.state));
        }
        
        // Store the receipt CID
        contract.execution_receipt_cid = Some(receipt.task_cid.clone());
        
        info!("Processed execution receipt for contract {}", contract_id);
        
        // If we don't need verifications, complete immediately
        if contract.max_verifiers == 0 {
            drop(contracts); // Release lock before calling update_contract_state
            self.update_contract_state(contract_id, EscrowState::Completed).await?;
            
            // Distribute rewards
            self.distribute_rewards(contract_id).await?;
        }
        
        Ok(())
    }
    
    async fn process_verification(&self, contract_id: &str, receipt: &VerificationReceipt) -> Result<()> {
        let mut contracts = self.contracts.lock().unwrap();
        
        // Get the contract
        let contract = contracts.get_mut(contract_id)
            .ok_or_else(|| anyhow!("Contract not found: {}", contract_id))?;
        
        // Check if contract is in progress
        if contract.state != EscrowState::InProgress {
            return Err(anyhow!("Cannot process verification for contract in state: {:?}", contract.state));
        }
        
        // Check if already at max verifiers
        if contract.verifications.len() >= contract.max_verifiers as usize {
            return Err(anyhow!("Maximum number of verifiers reached for contract {}", contract_id));
        }
        
        // Add verification
        contract.verifications.insert(receipt.verifier_did.clone(), receipt.verdict);
        
        info!("Added verification from {} for contract {}, verdict: {}", 
            receipt.verifier_did, contract_id, receipt.verdict);
        
        // Check if we've reached quorum
        let reached_quorum = {
            let total = contract.verifications.len();
            let required = contract.max_verifiers as usize;
            
            // Either we have all required verifications or we have enough positive ones for quorum
            total >= required || self.check_verification_quorum(contract_id)?
        };
        
        if reached_quorum {
            drop(contracts); // Release lock before update_contract_state
            
            // If quorum is positive, complete the contract
            if self.check_verification_quorum(contract_id)? {
                self.update_contract_state(contract_id, EscrowState::Completed).await?;
                
                // Distribute rewards
                self.distribute_rewards(contract_id).await?;
            } else {
                // If quorum is negative, fail the contract
                self.update_contract_state(contract_id, EscrowState::Failed).await?;
                
                // Return tokens to publisher (minus fees)
                // This would trigger refund logic
            }
        }
        
        Ok(())
    }
    
    async fn distribute_rewards(&self, contract_id: &str) -> Result<RewardDistribution> {
        let contracts = self.contracts.lock().unwrap();
        
        // Get the contract
        let contract = contracts.get(contract_id)
            .ok_or_else(|| anyhow!("Contract not found: {}", contract_id))?;
        
        // Check if contract is completed
        if contract.state != EscrowState::Completed {
            return Err(anyhow!("Cannot distribute rewards for contract in state: {:?}", contract.state));
        }
        
        // Check if contract has an execution receipt
        let receipt_cid = contract.execution_receipt_cid
            .as_ref()
            .ok_or_else(|| anyhow!("Contract has no execution receipt"))?;
        
        // Get the worker DID
        let worker_did = {
            // In a real implementation, we'd look up the execution receipt
            // For now, just access the worker from verifications (fake it)
            let positive_verifiers: Vec<_> = contract.verifications.iter()
                .filter(|&(_, &verdict)| verdict)
                .map(|(did, _)| did)
                .collect();
            
            if positive_verifiers.is_empty() {
                return Err(anyhow!("No positive verifications found"));
            }
            
            // Just use the first positive verifier as the worker for this example
            positive_verifiers[0].clone()
        };
        
        // Calculate reward distribution
        let total_reward = contract.token_amount;
        
        // Worker reward
        let worker_percentage = contract.reward_settings.worker_percentage as f64 / 100.0;
        let worker_reward_amount = total_reward.mul(worker_percentage)?;
        
        // Verifier rewards
        let verifier_percentage = contract.reward_settings.verifier_percentage as f64 / 100.0;
        let verifier_total_reward = total_reward.mul(verifier_percentage)?;
        
        // Platform fee
        let platform_percentage = contract.reward_settings.platform_fee_percentage as f64 / 100.0;
        let platform_fee = total_reward.mul(platform_percentage)?;
        
        // Calculate individual verifier rewards
        let verifier_rewards = if contract.reward_settings.use_reputation_weighting {
            // Use reputation weighting for verifier rewards
            let mut rewards = Vec::new();
            let positive_verifiers: Vec<_> = contract.verifications.iter()
                .filter(|&(_, &verdict)| verdict)
                .map(|(did, _)| did)
                .collect();
            
            if positive_verifiers.is_empty() {
                return Err(anyhow!("No positive verifications found"));
            }
            
            // Calculate total reputation
            let mut total_reputation = 0.0;
            for verifier_did in &positive_verifiers {
                total_reputation += self.reputation.get_score(verifier_did);
            }
            
            // Calculate individual rewards
            for verifier_did in positive_verifiers {
                let reputation = self.reputation.get_score(verifier_did);
                let weight = if total_reputation > 0.0 {
                    reputation / total_reputation
                } else {
                    1.0 / (positive_verifiers.len() as f64)
                };
                
                let reward_amount = verifier_total_reward.mul(weight)?;
                
                rewards.push(VerifierReward {
                    verifier_did: verifier_did.clone(),
                    amount: reward_amount,
                    reputation_weight: Some(weight),
                });
            }
            
            rewards
        } else {
            // Equal distribution
            let positive_verifiers: Vec<_> = contract.verifications.iter()
                .filter(|&(_, &verdict)| verdict)
                .map(|(did, _)| did)
                .collect();
            
            if positive_verifiers.is_empty() {
                return Err(anyhow!("No positive verifications found"));
            }
            
            let reward_per_verifier = verifier_total_reward.mul(1.0 / positive_verifiers.len() as f64)?;
            
            positive_verifiers.iter().map(|did| {
                VerifierReward {
                    verifier_did: (*did).clone(),
                    amount: reward_per_verifier,
                    reputation_weight: None,
                }
            }).collect()
        };
        
        // Create the reward distribution
        let distribution = RewardDistribution {
            contract_id: contract_id.to_string(),
            task_cid: receipt_cid.clone(),
            total_amount: total_reward,
            timestamp: Utc::now(),
            worker_reward: WorkerReward {
                worker_did: worker_did.clone(),
                amount: worker_reward_amount,
            },
            verifier_rewards,
            platform_fee,
        };
        
        // Store the distribution
        {
            let mut distributions = self.distributions.lock().unwrap();
            distributions.insert(contract_id.to_string(), distribution.clone());
        }
        
        // Release tokens
        let mut locks = self.locks.lock().unwrap();
        for lock in locks.values_mut() {
            if lock.contract_id == contract_id && !lock.released {
                lock.released = true;
                lock.released_at = Some(Utc::now());
                
                // Emit tokens released event
                let event = EscrowEvent::TokensReleased(lock.clone());
                // TODO: publish event to subscribers
            }
        }
        
        // Emit rewards distributed event
        let event = EscrowEvent::RewardsDistributed(distribution.clone());
        // TODO: publish event to subscribers
        
        // Anchor distribution to DAG
        if let Some(ref dag) = self.dag {
            let dag_event = DagEvent::RewardDistributed {
                contract_id: contract_id.to_string(),
                worker_did: worker_did.clone(),
                amount: total_reward.value,
                timestamp: Utc::now(),
            };
            
            // Submit to DAG
            match dag.submit_event(dag_event).await {
                Ok(_) => debug!("Anchored reward distribution to DAG"),
                Err(e) => warn!("Failed to anchor reward distribution to DAG: {}", e),
            }
        }
        
        info!("Distributed rewards for contract {}: {} tokens total", 
            contract_id, total_reward.value);
        
        Ok(distribution)
    }
    
    async fn create_dispute(&self, contract_id: &str, disputer_did: &Did, reason: &str) -> Result<()> {
        let contracts = self.contracts.lock().unwrap();
        
        // Get the contract
        let contract = contracts.get(contract_id)
            .ok_or_else(|| anyhow!("Contract not found: {}", contract_id))?;
        
        // Check if contract is still active
        match contract.state {
            EscrowState::Created | EscrowState::InProgress => {
                // Contract can be disputed
            },
            _ => {
                return Err(anyhow!("Cannot dispute contract in state: {:?}", contract.state));
            }
        }
        
        drop(contracts); // Release lock before update_contract_state
        
        // Update state to disputed
        self.update_contract_state(contract_id, EscrowState::Disputed).await?;
        
        // Emit dispute event
        let event = EscrowEvent::ContractDisputed {
            contract_id: contract_id.to_string(),
            disputer_did: disputer_did.clone(),
            reason: reason.to_string(),
            timestamp: Utc::now(),
        };
        // TODO: publish event to subscribers
        
        // Anchor dispute to DAG
        if let Some(ref dag) = self.dag {
            let dag_event = DagEvent::EscrowDispute {
                contract_id: contract_id.to_string(),
                disputer_did: disputer_did.clone(),
                reason: reason.to_string(),
                timestamp: Utc::now(),
            };
            
            // Submit to DAG
            match dag.submit_event(dag_event).await {
                Ok(_) => debug!("Anchored dispute to DAG"),
                Err(e) => warn!("Failed to anchor dispute to DAG: {}", e),
            }
        }
        
        info!("Created dispute for contract {} by {}: {}", 
            contract_id, disputer_did, reason);
        
        Ok(())
    }
    
    async fn get_contract_state(&self, contract_id: &str) -> Result<EscrowState> {
        // Check for expiry first
        let is_expired = self.check_contract_expiry(contract_id).await?;
        if is_expired {
            return Ok(EscrowState::Expired);
        }
        
        // Get current state
        let contracts = self.contracts.lock().unwrap();
        
        let contract = contracts.get(contract_id)
            .ok_or_else(|| anyhow!("Contract not found: {}", contract_id))?;
        
        Ok(contract.state.clone())
    }
    
    async fn is_contract_completed(&self, contract_id: &str) -> Result<bool> {
        let state = self.get_contract_state(contract_id).await?;
        
        Ok(state == EscrowState::Completed)
    }
} 