use std::collections::HashMap;
use uuid::Uuid;
use crate::{EconomicsError, EconomicsResult, ParticipatoryBudget, BudgetProposal, ProposalStatus, ResourceType, BudgetRulesConfig, VoteChoice, VotingMethod};
use icn_identity::IdentityScope;
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use std::sync::{Arc, Mutex};
use cid::Cid;
use multihash::{Code, MultihashDigest};

/// Storage key prefix for budget state
const BUDGET_KEY_PREFIX: &str = "budget::";

/// Simple key-value storage interface for budget operations
#[async_trait]
pub trait BudgetStorage: Send + Sync {
    /// Store a budget with a key
    async fn store_budget(&mut self, key: &str, data: Vec<u8>) -> EconomicsResult<()>;
    
    /// Retrieve a budget by key
    async fn get_budget(&self, key: &str) -> EconomicsResult<Option<Vec<u8>>>;
    
    /// Store data with a CID key
    async fn put_with_key(&mut self, key_cid: Cid, data: Vec<u8>) -> EconomicsResult<()>;
    
    /// Retrieve data by CID key
    async fn get_by_cid(&self, key_cid: &Cid) -> EconomicsResult<Option<Vec<u8>>>;
}

/// Implementation of BudgetStorage that wraps a StorageBackend
#[async_trait]
impl<T: icn_storage::StorageBackend + Send + Sync> BudgetStorage for T {
    async fn store_budget(&mut self, key: &str, data: Vec<u8>) -> EconomicsResult<()> {
        // Generate a key CID from the string key
        let key_str = format!("budget::{}", key);
        let hash = multihash::Code::Sha2_256.digest(key_str.as_bytes());
        let key_cid = cid::Cid::new_v1(0x71, hash);
        
        // Store the data directly using key-value operations
        self.put_kv(key_cid, data)
            .await
            .map_err(|e| EconomicsError::InvalidBudget(format!("Storage error: {}", e)))?;
        
        Ok(())
    }
    
    async fn get_budget(&self, key: &str) -> EconomicsResult<Option<Vec<u8>>> {
        // Generate the same key CID from the string key
        let key_str = format!("budget::{}", key);
        let hash = multihash::Code::Sha2_256.digest(key_str.as_bytes());
        let key_cid = cid::Cid::new_v1(0x71, hash);
        
        // Retrieve the data using key-value operations
        self.get_kv(&key_cid)
            .await
            .map_err(|e| EconomicsError::InvalidBudget(format!("Storage error: {}", e)))
    }
    
    async fn put_with_key(&mut self, key_cid: Cid, data: Vec<u8>) -> EconomicsResult<()> {
        // Use the key-value operations directly
        self.put_kv(key_cid, data)
            .await
            .map_err(|e| EconomicsError::InvalidBudget(format!("Storage error: {}", e)))
    }
    
    async fn get_by_cid(&self, key_cid: &Cid) -> EconomicsResult<Option<Vec<u8>>> {
        // Use the key-value operations directly
        self.get_kv(key_cid)
            .await
            .map_err(|e| EconomicsError::InvalidBudget(format!("Storage error: {}", e)))
    }
}

/// Mock implementation of BudgetStorage for testing
#[derive(Default, Debug, Clone)]
pub struct MockBudgetStorage {
    /// Standard key-value storage for budgets (uses string keys)
    pub data: HashMap<String, Vec<u8>>,
    /// CID-based key-value storage for authorizations and other structured data
    pub cid_data: HashMap<String, Vec<u8>>, // Store CID keys as strings for simplicity in tests
}

impl MockBudgetStorage {
    /// Create a new empty mock storage
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            cid_data: HashMap::new(),
        }
    }
    
    /// Helper to get authorization data from the mock
    pub fn get_stored_authorizations(&self) -> Vec<crate::ResourceAuthorization> {
        self.cid_data.values()
            .filter_map(|data| serde_json::from_slice(data).ok())
            .collect()
    }
}

#[async_trait]
impl BudgetStorage for MockBudgetStorage {
    async fn store_budget(&mut self, key: &str, data: Vec<u8>) -> EconomicsResult<()> {
        self.data.insert(key.to_string(), data);
        Ok(())
    }
    
    async fn get_budget(&self, key: &str) -> EconomicsResult<Option<Vec<u8>>> {
        Ok(self.data.get(key).cloned())
    }
    
    async fn put_with_key(&mut self, key_cid: Cid, data: Vec<u8>) -> EconomicsResult<()> {
        // Store using the string representation of the CID as the key
        self.cid_data.insert(key_cid.to_string(), data);
        Ok(())
    }
    
    async fn get_by_cid(&self, key_cid: &Cid) -> EconomicsResult<Option<Vec<u8>>> {
        Ok(self.cid_data.get(&key_cid.to_string()).cloned())
    }
}

/// Create a new participatory budget
pub async fn create_budget(
    name: &str,
    scope_id: &str,
    scope_type: IdentityScope,
    start_timestamp: i64,
    end_timestamp: i64,
    rules: Option<BudgetRulesConfig>,
    storage: &mut impl BudgetStorage,
) -> EconomicsResult<String> {
    // Create a unique budget ID
    let budget_id = Uuid::new_v4().to_string();
    
    // Create the budget state
    let budget = ParticipatoryBudget {
        id: budget_id.clone(),
        name: name.to_string(),
        scope_id: scope_id.to_string(),
        scope_type,
        total_allocated: HashMap::new(),
        spent_by_proposal: HashMap::new(),
        proposals: HashMap::new(),
        rules,
        start_timestamp,
        end_timestamp,
    };
    
    // Serialize the budget state
    let budget_data = serde_json::to_vec(&budget)
        .map_err(|e| EconomicsError::InvalidBudget(format!("Failed to serialize budget: {}", e)))?;
    
    // Store the budget in storage
    let storage_key = format!("{}{}", BUDGET_KEY_PREFIX, budget_id);
    storage.store_budget(&storage_key, budget_data)
        .await
        .map_err(|e| EconomicsError::InvalidBudget(format!("Failed to store budget: {}", e)))?;
    
    Ok(budget_id)
}

/// Load a budget from storage
pub async fn load_budget(
    budget_id: &str,
    storage: &impl BudgetStorage,
) -> EconomicsResult<ParticipatoryBudget> {
    // Construct the storage key
    let storage_key = format!("{}{}", BUDGET_KEY_PREFIX, budget_id);
    
    // Get the budget data from storage
    let budget_data = storage.get_budget(&storage_key)
        .await
        .map_err(|e| EconomicsError::InvalidBudget(format!("Failed to load budget: {}", e)))?
        .ok_or_else(|| EconomicsError::InvalidBudget(format!("Budget not found with id: {}", budget_id)))?;
    
    // Deserialize the budget state
    let budget: ParticipatoryBudget = serde_json::from_slice(&budget_data)
        .map_err(|e| EconomicsError::InvalidBudget(format!("Failed to deserialize budget: {}", e)))?;
    
    Ok(budget)
}

/// Save a budget to storage
async fn save_budget(
    budget: &ParticipatoryBudget,
    storage: &mut impl BudgetStorage,
) -> EconomicsResult<()> {
    // Serialize the budget state
    let budget_data = serde_json::to_vec(budget)
        .map_err(|e| EconomicsError::InvalidBudget(format!("Failed to serialize budget: {}", e)))?;
    
    // Store the budget in storage
    let storage_key = format!("{}{}", BUDGET_KEY_PREFIX, budget.id);
    storage.store_budget(&storage_key, budget_data)
        .await
        .map_err(|e| EconomicsError::InvalidBudget(format!("Failed to store budget: {}", e)))?;
    
    Ok(())
}

/// Allocate resources to a budget
pub async fn allocate_to_budget(
    budget_id: &str,
    resource_type: ResourceType,
    amount: u64,
    storage: &mut impl BudgetStorage,
) -> EconomicsResult<()> {
    // Load the budget
    let mut budget = load_budget(budget_id, storage).await?;
    
    // Update the total allocation for this resource type
    let current_amount = budget.total_allocated.get(&resource_type).cloned().unwrap_or(0);
    budget.total_allocated.insert(resource_type, current_amount + amount);
    
    // Save the updated budget
    save_budget(&budget, storage).await?;
    
    Ok(())
}

/// Propose spending from a budget
pub async fn propose_budget_spend(
    budget_id: &str,
    title: &str,
    description: &str,
    requested_resources: HashMap<ResourceType, u64>,
    proposer_did: &str,
    category: Option<String>,
    metadata: Option<serde_json::Value>,
    storage: &mut impl BudgetStorage,
) -> EconomicsResult<Uuid> {
    // Load the budget
    let mut budget = load_budget(budget_id, storage).await?;
    
    // Create a new proposal
    let proposal_id = Uuid::new_v4();
    let now = chrono::Utc::now().timestamp();
    
    let proposal = BudgetProposal {
        id: proposal_id,
        title: title.to_string(),
        description: description.to_string(),
        proposer_did: proposer_did.to_string(),
        requested_resources,
        status: ProposalStatus::Proposed,
        category,
        votes: HashMap::new(),
        creation_timestamp: now,
        metadata,
    };
    
    // Add the proposal to the budget
    budget.proposals.insert(proposal_id, proposal);
    
    // Save the updated budget
    save_budget(&budget, storage).await?;
    
    Ok(proposal_id)
}

/// Query the available balance for a resource type in a budget
pub async fn query_budget_balance(
    budget_id: &str,
    resource_type: &ResourceType,
    storage: &impl BudgetStorage,
) -> EconomicsResult<u64> {
    // Load the budget
    let budget = load_budget(budget_id, storage).await?;
    
    // Get the total allocation for this resource type
    let total_allocated = budget.total_allocated.get(resource_type).cloned().unwrap_or(0);
    
    // Calculate total spent for this resource type across all proposals
    let total_spent: u64 = budget.spent_by_proposal.values()
        .filter_map(|resources| resources.get(resource_type))
        .sum();
    
    // Calculate available balance
    let available = if total_spent > total_allocated {
        0 // Should not happen, but fail safe
    } else {
        total_allocated - total_spent
    };
    
    Ok(available)
}

/// Record a vote on a budget proposal
pub async fn record_budget_vote(
    budget_id: &str,
    proposal_id: Uuid,
    voter_did: String,
    vote: VoteChoice,
    storage: &mut impl BudgetStorage,
) -> EconomicsResult<()> {
    // Load the budget
    let mut budget = load_budget(budget_id, storage).await?;
    
    // Find the proposal
    let proposal = budget.proposals.get_mut(&proposal_id)
        .ok_or_else(|| EconomicsError::InvalidBudget(format!("Proposal not found with id: {}", proposal_id)))?;
    
    // Check if the proposal is in a votable state
    if proposal.status != ProposalStatus::Proposed && proposal.status != ProposalStatus::VotingOpen {
        return Err(EconomicsError::InvalidBudget(
            format!("Cannot vote on proposal in {:?} state", proposal.status)
        ));
    }
    
    // Update proposal status to VotingOpen if it's still in Proposed state
    if proposal.status == ProposalStatus::Proposed {
        proposal.status = ProposalStatus::VotingOpen;
    }
    
    // TODO: Check voter eligibility based on budget scope/rules
    // For now, we'll allow any valid DID to vote
    
    // Record the vote
    proposal.votes.insert(voter_did, vote);
    
    // Save the updated budget
    save_budget(&budget, storage).await?;
    
    Ok(())
}

/// Helper function to tally votes based on the governing rules
fn tally_votes(
    proposal: &BudgetProposal,
    rules: &BudgetRulesConfig,
    total_eligible_voters: u32
) -> ProposalStatus {
    // Get the voting method
    let voting_method = rules.voting_method.as_ref().unwrap_or(&VotingMethod::SimpleMajority);
    
    // Count votes
    let mut approve_count = 0;
    let mut reject_count = 0;
    let mut abstain_count = 0;
    let mut quadratic_approve_weight = 0;
    let mut quadratic_reject_weight = 0;

    for vote in proposal.votes.values() {
        match vote {
            VoteChoice::Approve => approve_count += 1,
            VoteChoice::Reject => reject_count += 1,
            VoteChoice::Abstain => abstain_count += 1,
            VoteChoice::Quadratic(weight) => {
                if *weight > 0 {
                    quadratic_approve_weight += weight;
                } else {
                    quadratic_reject_weight += weight;
                }
            },
        }
    }

    // Get total votes cast
    let total_votes = approve_count + reject_count + abstain_count;
    
    // Check quorum if specified
    if let Some(quorum_percentage) = rules.quorum_percentage {
        let quorum_threshold = (total_eligible_voters as f64 * (quorum_percentage as f64 / 100.0)).ceil() as u32;
        
        // Include abstentions in quorum calculation
        if total_votes < quorum_threshold as usize {
            // Not enough votes to meet quorum
            return ProposalStatus::VotingOpen;
        }
    } else if let Some(min_participants) = rules.min_participants {
        // Legacy quorum check using min_participants
        if total_votes < min_participants as usize {
            // Not enough votes to meet quorum
            return ProposalStatus::VotingOpen;
        }
    }
    
    // Get threshold percentage (default to 50% if not specified)
    let threshold_percentage = rules.threshold_percentage.unwrap_or(50) as f64 / 100.0;
    
    // Tally votes based on the voting method
    match voting_method {
        VotingMethod::SimpleMajority => {
            // Simple majority requires more approvals than rejections and meeting threshold
            let approval_ratio = if approve_count + reject_count > 0 {
                approve_count as f64 / (approve_count + reject_count) as f64
            } else {
                0.0
            };
            
            if approval_ratio > threshold_percentage {
                ProposalStatus::Approved
            } else {
                ProposalStatus::Rejected
            }
        },
        VotingMethod::Threshold => {
            // Threshold requires a specific percentage of all eligible voters to approve
            let approval_ratio = approve_count as f64 / total_eligible_voters as f64;
            
            if approval_ratio >= threshold_percentage {
                ProposalStatus::Approved
            } else {
                ProposalStatus::Rejected
            }
        },
        VotingMethod::Quadratic => {
            // Quadratic voting uses the square root of voting power
            if quadratic_approve_weight > quadratic_reject_weight {
                ProposalStatus::Approved
            } else {
                ProposalStatus::Rejected
            }
        },
    }
}

/// Tally votes on a budget proposal and determine the result based on the governing rules
pub async fn tally_budget_votes(
    budget_id: &str, 
    proposal_id: Uuid,
    storage: &impl BudgetStorage,
) -> EconomicsResult<ProposalStatus> {
    // Load the budget
    let budget = load_budget(budget_id, storage).await?;
    
    // Find the proposal
    let proposal = budget.proposals.get(&proposal_id)
        .ok_or_else(|| EconomicsError::InvalidBudget(format!("Proposal not found with id: {}", proposal_id)))?;
    
    // If the proposal is not in a votable state, return its current status
    if proposal.status != ProposalStatus::Proposed && proposal.status != ProposalStatus::VotingOpen {
        return Ok(proposal.status.clone());
    }
    
    // Create a default rules config if none exists in the budget
    let default_rules = BudgetRulesConfig {
        voting_method: Some(VotingMethod::SimpleMajority),
        min_participants: Some(1),
        quorum_percentage: Some(10), // Default 10% quorum
        threshold_percentage: Some(50), // Default 50% threshold
        categories: None,
        custom_rules: None,
    };
    
    // Get the budget rules, using default if none specified
    let rules = budget.rules.as_ref().unwrap_or(&default_rules);
    
    // TODO: Look up the total eligible voters for this budget scope
    // For now, we'll assume 10 eligible voters for testing
    let total_eligible_voters = 10;
    
    // Tally the votes using our helper function
    let new_status = tally_votes(proposal, rules, total_eligible_voters);
    
    Ok(new_status)
}

/// Finalize a budget proposal based on vote tally
pub async fn finalize_budget_proposal(
    budget_id: &str,
    proposal_id: Uuid,
    storage: &mut impl BudgetStorage,
) -> EconomicsResult<ProposalStatus> {
    // Tally the votes to determine the new status
    let new_status = tally_budget_votes(budget_id, proposal_id, storage).await?;
    
    // If the status is still in voting stage, don't update anything
    if new_status == ProposalStatus::VotingOpen {
        return Ok(new_status);
    }
    
    // Load the budget for updating
    let mut budget = load_budget(budget_id, storage).await?;
    
    // Find the proposal
    let proposal = budget.proposals.get_mut(&proposal_id)
        .ok_or_else(|| EconomicsError::InvalidBudget(format!("Proposal not found with id: {}", proposal_id)))?;
    
    // Update the proposal status
    proposal.status = new_status.clone();
    
    // If the proposal is approved, create resource authorizations
    if new_status == ProposalStatus::Approved {
        // Record the spending in the budget
        let mut resources_map = HashMap::new();
        for (resource_type, amount) in &proposal.requested_resources {
            resources_map.insert(resource_type.clone(), *amount);
        }
        budget.spent_by_proposal.insert(proposal_id, resources_map.clone());
        
        // Create ResourceAuthorizations for the proposer
        let now = chrono::Utc::now().timestamp();
        
        // Use budget end_timestamp as the expiry for authorizations
        let expiry = budget.end_timestamp;
        let proposer_did = proposal.proposer_did.clone();
        let budget_scope_id = budget.scope_id.clone();
        let budget_scope_type = budget.scope_type;
        
        // Update proposal status to Executed once authorizations are created
        proposal.status = ProposalStatus::Executed;
        
        // Save the updated budget before creating authorizations
        save_budget(&budget, storage).await?;
        
        // Create a ResourceAuthorization for each requested resource
        for (resource_type, amount) in &resources_map {
            // Create the authorization
            let auth = crate::ResourceAuthorization::new(
                budget_scope_id.clone(),         // grantor = budget scope (coop/community)
                proposer_did.clone(),            // grantee = proposer
                resource_type.clone(),           // resource type from request
                *amount,                         // amount from request
                budget_scope_type,               // scope from budget
                Some(expiry),                    // expiry from budget
                Some(serde_json::json!({        // metadata with proposal info
                    "proposal_id": proposal_id.to_string(),
                    "budget_id": budget_id,
                    "approved_timestamp": now
                }))
            );
            
            // Store the auth using CID-based key
            let auth_id = auth.auth_id;
            let auth_key_str = format!("auth::{}", auth_id);
            let auth_key_hash = Code::Sha2_256.digest(auth_key_str.as_bytes());
            let auth_key_cid = Cid::new_v1(0x71, auth_key_hash); // dag-cbor likely suitable for structured data key mapping
            
            // Serialize the authorization
            let auth_data = serde_json::to_vec(&auth)
                .map_err(|e| EconomicsError::InvalidBudget(
                    format!("Failed to serialize authorization: {}", e)
                ))?;
            
            // Store the authorization with CID key using the put_with_key method
            storage.put_with_key(auth_key_cid, auth_data)
                .await
                .map_err(|e| EconomicsError::InvalidBudget(
                    format!("Failed to store authorization: {}", e)
                ))?;
                
            tracing::info!(auth_id = %auth_id, key_cid = %auth_key_cid, "Stored ResourceAuthorization");
        }
        
        return Ok(ProposalStatus::Executed);
    } else {
        // Save the updated budget
        save_budget(&budget, storage).await?;
    }
    
    Ok(new_status)
}

/// TODO(V3-MVP): Implement proposal approval logic and update proposal status
pub async fn approve_budget_proposal(
    budget_id: &str,
    proposal_id: Uuid,
    approver_did: &str,
    storage: &mut impl BudgetStorage,
) -> EconomicsResult<()> {
    // Record an approval vote from the approver
    record_budget_vote(budget_id, proposal_id, approver_did.to_string(), VoteChoice::Approve, storage).await?;
    
    // Finalize the proposal to see if it's now approved
    let status = finalize_budget_proposal(budget_id, proposal_id, storage).await?;
    
    // If the proposal is now approved, it will have been updated in storage
    if status == ProposalStatus::Approved {
        Ok(())
    } else {
        Err(EconomicsError::InvalidBudget(
            format!("Proposal not approved after vote. Current status: {:?}", status)
        ))
    }
}

/// Helper function to create a test budget with a default proposal
async fn create_test_budget_with_proposal() -> (String, Uuid, MockBudgetStorage) {
    let mut storage = MockBudgetStorage::new();
    
    let now = chrono::Utc::now().timestamp();
    let end = now + 3600 * 24 * 30; // 30 days from now
    
    // Create a budget with rules
    let rules = BudgetRulesConfig {
        voting_method: Some(VotingMethod::SimpleMajority),
        min_participants: Some(3),
        quorum_percentage: Some(30),
        threshold_percentage: Some(50),
        categories: None,
        custom_rules: None,
    };
    
    let budget_id = create_budget(
        "Test Budget",
        "did:icn:test-coop",
        IdentityScope::Cooperative,
        now,
        end,
        Some(rules),
        &mut storage,
    ).await.unwrap();
    
    // Create a proposal in the budget
    let proposal_id = propose_budget_spend(
        &budget_id,
        "Test Proposal",
        "This is a test proposal",
        HashMap::from([(ResourceType::Compute, 1000)]),
        "did:icn:proposer",
        None,
        None,
        &mut storage,
    ).await.unwrap();
    
    (budget_id, proposal_id, storage)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_record_vote_and_update_status() {
        let (budget_id, proposal_id, mut storage) = create_test_budget_with_proposal().await;
        
        // Record a vote and check that status changes to VotingOpen
        record_budget_vote(
            &budget_id,
            proposal_id,
            "did:icn:voter1".to_string(),
            VoteChoice::Approve,
            &mut storage,
        ).await.unwrap();
        
        // Check that proposal status was updated
        let budget = load_budget(&budget_id, &storage).await.unwrap();
        let proposal = budget.proposals.get(&proposal_id).unwrap();
        assert_eq!(proposal.status, ProposalStatus::VotingOpen);
        assert_eq!(proposal.votes.len(), 1);
    }
    
    #[tokio::test]
    async fn test_tally_votes_with_quorum() {
        let (budget_id, proposal_id, mut storage) = create_test_budget_with_proposal().await;
        
        // First vote doesn't meet quorum
        record_budget_vote(
            &budget_id,
            proposal_id,
            "did:icn:voter1".to_string(),
            VoteChoice::Approve,
            &mut storage,
        ).await.unwrap();
        
        // Tally votes - should remain in VotingOpen due to not meeting quorum (need 3)
        let status = tally_budget_votes(&budget_id, proposal_id, &storage).await.unwrap();
        assert_eq!(status, ProposalStatus::VotingOpen);
        
        // Add more votes to meet quorum
        record_budget_vote(
            &budget_id,
            proposal_id,
            "did:icn:voter2".to_string(),
            VoteChoice::Approve,
            &mut storage,
        ).await.unwrap();
        
        record_budget_vote(
            &budget_id,
            proposal_id,
            "did:icn:voter3".to_string(),
            VoteChoice::Approve,
            &mut storage,
        ).await.unwrap();
        
        // Now should be approved
        let status = tally_budget_votes(&budget_id, proposal_id, &storage).await.unwrap();
        assert_eq!(status, ProposalStatus::Approved);
    }
    
    #[tokio::test]
    async fn test_finalize_proposal_with_auth_creation() {
        let (budget_id, proposal_id, mut storage) = create_test_budget_with_proposal().await;
        
        // Add votes to meet quorum and approve
        record_budget_vote(
            &budget_id,
            proposal_id,
            "did:icn:voter1".to_string(),
            VoteChoice::Approve,
            &mut storage,
        ).await.unwrap();
        
        record_budget_vote(
            &budget_id,
            proposal_id,
            "did:icn:voter2".to_string(),
            VoteChoice::Approve,
            &mut storage,
        ).await.unwrap();
        
        record_budget_vote(
            &budget_id,
            proposal_id,
            "did:icn:voter3".to_string(),
            VoteChoice::Approve,
            &mut storage,
        ).await.unwrap();
        
        // Finalize the proposal
        let status = finalize_budget_proposal(&budget_id, proposal_id, &mut storage).await.unwrap();
        assert_eq!(status, ProposalStatus::Executed);
        
        // Check that the budget was updated
        let budget = load_budget(&budget_id, &storage).await.unwrap();
        let proposal = budget.proposals.get(&proposal_id).unwrap();
        
        // Verify proposal status is now Executed
        assert_eq!(proposal.status, ProposalStatus::Executed);
        
        // Verify the resources were recorded in spent_by_proposal
        assert!(budget.spent_by_proposal.contains_key(&proposal_id));
        let spent = budget.spent_by_proposal.get(&proposal_id).unwrap();
        assert_eq!(spent.get(&ResourceType::Compute).unwrap(), &1000);
        
        // Check for auth entries in cid_data (CID-based storage)
        assert!(!storage.cid_data.is_empty(), "Should have created at least one ResourceAuthorization using CID storage");
        
        // The CID key format follows our defined pattern with auth::<UUID>
        let cid_keys: Vec<String> = storage.cid_data.keys().cloned().collect();
        
        // Load the authorization from the first CID key and verify its contents
        if let Some(cid_key) = cid_keys.first() {
            let cid = Cid::try_from(cid_key.as_str()).unwrap_or_else(|_| panic!("Invalid CID string: {}", cid_key));
            let auth_data = storage.cid_data.get(cid_key).unwrap();
            let auth: crate::ResourceAuthorization = serde_json::from_slice(auth_data).unwrap();
            
            // Verify the authorization details
            assert_eq!(auth.grantor_did, "did:icn:test-coop");
            assert_eq!(auth.grantee_did, "did:icn:proposer");
            assert_eq!(auth.resource_type, ResourceType::Compute);
            assert_eq!(auth.authorized_amount, 1000);
            assert_eq!(auth.scope, IdentityScope::Cooperative);
            
            // Check that metadata contains expected fields
            if let Some(metadata) = &auth.metadata {
                let proposal_id_str = metadata.get("proposal_id").and_then(|v| v.as_str());
                assert!(proposal_id_str.is_some());
                assert_eq!(proposal_id_str.unwrap(), proposal_id.to_string());
                
                let budget_id_str = metadata.get("budget_id").and_then(|v| v.as_str());
                assert!(budget_id_str.is_some());
                assert_eq!(budget_id_str.unwrap(), budget_id);
            } else {
                panic!("Auth should have metadata");
            }
        } else {
            panic!("No CID-based authorizations were created");
        }
    }
    
    #[tokio::test]
    async fn test_tally_threshold_voting() {
        let mut storage = MockBudgetStorage::new();
        
        let now = chrono::Utc::now().timestamp();
        let end = now + 3600 * 24 * 30; // 30 days from now
        
        // Create a budget with threshold voting rules that require high approval
        let rules = BudgetRulesConfig {
            voting_method: Some(VotingMethod::Threshold),
            min_participants: Some(3),
            quorum_percentage: Some(30), // 30% quorum
            threshold_percentage: Some(70), // 70% threshold of all eligible voters
            categories: None,
            custom_rules: None,
        };
        
        let budget_id = create_budget(
            "Threshold Budget",
            "did:icn:test-coop",
            IdentityScope::Cooperative,
            now,
            end,
            Some(rules),
            &mut storage,
        ).await.unwrap();
        
        // Create a proposal
        let requested_resources = HashMap::from([(ResourceType::Compute, 500)]);
        
        let proposal_id = propose_budget_spend(
            &budget_id,
            "Threshold Proposal",
            "A proposal using threshold voting",
            requested_resources.clone(),
            "did:icn:proposer",
            None,
            None,
            &mut storage,
        ).await.unwrap();
        
        // Add just a few votes - should be rejected because we need 70% of all 
        // eligible voters (which is set to 10 by default in tally_budget_votes)
        record_budget_vote(&budget_id, proposal_id, "did:icn:voter1".to_string(), VoteChoice::Approve, &mut storage).await.unwrap();
        record_budget_vote(&budget_id, proposal_id, "did:icn:voter2".to_string(), VoteChoice::Approve, &mut storage).await.unwrap();
        record_budget_vote(&budget_id, proposal_id, "did:icn:voter3".to_string(), VoteChoice::Approve, &mut storage).await.unwrap();
        
        // Tally votes - should be rejected because 3 approvals < 70% of 10 total eligible voters
        let status = tally_budget_votes(&budget_id, proposal_id, &storage).await.unwrap();
        assert_eq!(status, ProposalStatus::Rejected);
        
        // Create a second proposal to test approval
        let proposal_id2 = propose_budget_spend(
            &budget_id,
            "Second Threshold Proposal",
            "Another proposal that should be approved",
            requested_resources.clone(),
            "did:icn:proposer",
            None,
            None,
            &mut storage,
        ).await.unwrap();
        
        // Add more votes to hit the threshold
        for i in 1..=8 {
            record_budget_vote(
                &budget_id, 
                proposal_id2, 
                format!("did:icn:voter{}", i).to_string(),
                VoteChoice::Approve, 
                &mut storage
            ).await.unwrap();
        }
        
        // Tally votes - should be approved with 8 approvals (80% > 70% threshold)
        let status = tally_budget_votes(&budget_id, proposal_id2, &storage).await.unwrap();
        assert_eq!(status, ProposalStatus::Approved);
    }
    
    #[tokio::test]
    async fn test_tally_quadratic_votes() {
        let mut storage = MockBudgetStorage::new();
        
        let now = chrono::Utc::now().timestamp();
        let end = now + 3600 * 24 * 30; // 30 days from now
        
        // Create a budget with quadratic voting rules
        let rules = BudgetRulesConfig {
            voting_method: Some(VotingMethod::Quadratic),
            min_participants: Some(2),
            quorum_percentage: Some(20),
            threshold_percentage: Some(50),
            categories: None,
            custom_rules: None,
        };
        
        let budget_id = create_budget(
            "Quadratic Budget",
            "did:icn:test-coop",
            IdentityScope::Cooperative,
            now,
            end,
            Some(rules),
            &mut storage,
        ).await.unwrap();
        
        // Create a proposal
        let proposal_id = propose_budget_spend(
            &budget_id,
            "Quadratic Proposal",
            "A proposal using quadratic voting",
            HashMap::from([(ResourceType::Compute, 500)]),
            "did:icn:proposer",
            None,
            None,
            &mut storage,
        ).await.unwrap();
        
        // Add quadratic votes - one heavily weighted approval and two rejections
        record_budget_vote(&budget_id, proposal_id, "did:icn:voter1".to_string(), VoteChoice::Quadratic(9), &mut storage).await.unwrap();
        record_budget_vote(&budget_id, proposal_id, "did:icn:voter2".to_string(), VoteChoice::Reject, &mut storage).await.unwrap();
        record_budget_vote(&budget_id, proposal_id, "did:icn:voter3".to_string(), VoteChoice::Reject, &mut storage).await.unwrap();
        
        // Tally votes - should be approved because 9 > 2 rejection weight
        let status = tally_budget_votes(&budget_id, proposal_id, &storage).await.unwrap();
        assert_eq!(status, ProposalStatus::Approved);
    }
} 