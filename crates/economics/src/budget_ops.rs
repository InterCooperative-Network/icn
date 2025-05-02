use std::collections::HashMap;
use uuid::Uuid;
use crate::{EconomicsError, EconomicsResult, ParticipatoryBudget, BudgetProposal, ProposalStatus, ResourceType, BudgetRulesConfig, VoteChoice};
use icn_identity::IdentityScope;
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use std::sync::{Arc, Mutex};

/// Storage key prefix for budget state
const BUDGET_KEY_PREFIX: &str = "budget::";

/// Simple key-value storage interface for budget operations
#[async_trait]
pub trait BudgetStorage: Send + Sync {
    /// Store a budget with a key
    async fn store_budget(&mut self, key: &str, data: Vec<u8>) -> EconomicsResult<()>;
    
    /// Retrieve a budget by key
    async fn get_budget(&self, key: &str) -> EconomicsResult<Option<Vec<u8>>>;
}

/// Implementation of BudgetStorage that wraps a StorageBackend
#[async_trait]
impl<T: icn_storage::StorageBackend + Send + Sync> BudgetStorage for T {
    async fn store_budget(&mut self, key: &str, data: Vec<u8>) -> EconomicsResult<()> {
        let cid = self.put(&data)
            .await
            .map_err(|e| EconomicsError::InvalidBudget(format!("Storage error: {}", e)))?;
        
        // Use a special key to remember the mapping from string key to CID
        let map_key = format!("key_to_cid::{}", key);
        let cid_bytes = cid.to_bytes();
        
        self.put(&cid_bytes)
            .await
            .map_err(|e| EconomicsError::InvalidBudget(format!("Storage error: {}", e)))?;
        
        Ok(())
    }
    
    async fn get_budget(&self, key: &str) -> EconomicsResult<Option<Vec<u8>>> {
        // Use a special key to find the CID
        let map_key = format!("key_to_cid::{}", key);
        
        // This is a simplification - in a real implementation we'd need to handle key->CID mapping
        // For our implementation tests, we'll use a mock that handles this differently
        
        // Just return None for now, the mock implementation for tests will work directly
        Ok(None)
    }
}

/// Mock implementation of BudgetStorage for testing
#[derive(Default, Debug, Clone)]
pub struct MockBudgetStorage {
    data: HashMap<String, Vec<u8>>,
}

impl MockBudgetStorage {
    /// Create a new empty mock storage
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
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
    voter_did: &str,
    vote: VoteChoice,
    storage: &mut impl BudgetStorage,
) -> EconomicsResult<()> {
    // Load the budget
    let mut budget = load_budget(budget_id, storage).await?;
    
    // Find the proposal
    let proposal = budget.proposals.get_mut(&proposal_id)
        .ok_or_else(|| EconomicsError::InvalidBudget(format!("Proposal not found with id: {}", proposal_id)))?;
    
    // Check if the proposal is still in a votable state
    if proposal.status != ProposalStatus::Proposed {
        return Err(EconomicsError::InvalidBudget(
            format!("Cannot vote on proposal in {:?} state", proposal.status)
        ));
    }
    
    // TODO(V3-MVP): Check if voter is eligible based on budget scope rules
    // For MVP, we'll allow any vote
    
    // Record the vote
    proposal.votes.insert(voter_did.to_string(), vote);
    
    // Save the updated budget
    save_budget(&budget, storage).await?;
    
    Ok(())
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
    
    // If the proposal is not in proposed state, return its current status
    if proposal.status != ProposalStatus::Proposed {
        return Ok(proposal.status.clone());
    }
    
    // Create a default rules config if none exists in the budget
    let default_rules = BudgetRulesConfig {
        voting_method: Some("simple_majority".to_string()),
        min_participants: Some(1),
        categories: None,
        custom_rules: None,
    };
    
    // Get the budget rules, using default if none specified
    let rules = budget.rules.as_ref().unwrap_or(&default_rules);
    
    // Get voting method
    let voting_method = rules.voting_method.as_ref()
        .unwrap_or(&"simple_majority".to_string())
        .to_lowercase();
    
    // Get minimum participants required (quorum)
    let min_participants = rules.min_participants.unwrap_or(1); // Default to 1
    
    // Check quorum
    if proposal.votes.len() < min_participants as usize {
        // Not enough votes yet, keep in proposed state
        return Ok(ProposalStatus::Proposed);
    }
    
    // Tally the votes according to the voting method
    match voting_method.as_str() {
        "quadratic" => tally_quadratic_votes(proposal),
        "consensus" => tally_consensus_votes(proposal),
        _ => tally_simple_majority_votes(proposal), // Default to simple majority
    }
}

/// Tally votes using simple majority method
fn tally_simple_majority_votes(proposal: &BudgetProposal) -> EconomicsResult<ProposalStatus> {
    let mut approve_count = 0;
    let mut reject_count = 0;
    
    // Count approve and reject votes
    for vote in proposal.votes.values() {
        match vote {
            VoteChoice::Approve => approve_count += 1,
            VoteChoice::Reject => reject_count += 1,
            VoteChoice::Abstain => {}, // Abstentions count for quorum but not for the tally
            VoteChoice::Quadratic(_weight) => {
                // In simple majority, treat quadratic votes as approve with weight 1
                approve_count += 1;
            }
        }
    }
    
    // Determine the outcome
    if approve_count > reject_count {
        Ok(ProposalStatus::Approved)
    } else if reject_count > approve_count {
        Ok(ProposalStatus::Rejected)
    } else {
        // Tie votes - the proposal remains in proposed state
        Ok(ProposalStatus::Proposed)
    }
}

/// Tally votes using quadratic voting method
fn tally_quadratic_votes(proposal: &BudgetProposal) -> EconomicsResult<ProposalStatus> {
    let mut approve_score = 0;
    let mut reject_score = 0;
    
    // Calculate scores using quadratic voting formula
    for vote in proposal.votes.values() {
        match vote {
            VoteChoice::Approve => approve_score += 1, // Regular approve counts as 1
            VoteChoice::Reject => reject_score += 1, // Regular reject counts as 1
            VoteChoice::Abstain => {}, // Abstentions don't count in the tally
            VoteChoice::Quadratic(weight) => {
                if *weight > 0 {
                    // In quadratic voting, cost scales with square of votes but impact is linear
                    // We use the weight directly as the score contribution
                    approve_score += *weight as i32;
                } else {
                    // Negative weights are used for rejections in this implementation
                    reject_score += 1; // Default to 1 for now
                }
            }
        }
    }
    
    // Determine the outcome
    if approve_score > reject_score {
        Ok(ProposalStatus::Approved)
    } else if reject_score > approve_score {
        Ok(ProposalStatus::Rejected)
    } else {
        // Tie votes - the proposal remains in proposed state
        Ok(ProposalStatus::Proposed)
    }
}

/// Tally votes using consensus method (requires near-unanimous approval)
fn tally_consensus_votes(proposal: &BudgetProposal) -> EconomicsResult<ProposalStatus> {
    let mut approve_count = 0;
    let mut reject_count = 0;
    let mut _abstain_count = 0;
    
    // Count approve, reject, and abstain votes
    for vote in proposal.votes.values() {
        match vote {
            VoteChoice::Approve => approve_count += 1,
            VoteChoice::Reject => reject_count += 1,
            VoteChoice::Abstain => _abstain_count += 1,
            VoteChoice::Quadratic(_) => {
                // In consensus voting, quadratic votes aren't supported
                // Treat as a regular approve
                approve_count += 1;
            }
        }
    }
    
    // Calculate total non-abstain votes
    let total_votes = approve_count + reject_count;
    
    // For consensus, we require at least 90% approval of non-abstaining voters
    if total_votes > 0 {
        let approval_percentage = (approve_count as f64 / total_votes as f64) * 100.0;
        
        if approval_percentage >= 90.0 {
            Ok(ProposalStatus::Approved)
        } else if reject_count > 0 {
            // Any rejection in consensus typically blocks the proposal
            Ok(ProposalStatus::Rejected)
        } else {
            // Not enough consensus yet
            Ok(ProposalStatus::Proposed)
        }
    } else {
        // No non-abstain votes - remain in proposed state
        Ok(ProposalStatus::Proposed)
    }
}

/// Finalize a budget proposal based on vote tally
pub async fn finalize_budget_proposal(
    budget_id: &str,
    proposal_id: Uuid,
    storage: &mut impl BudgetStorage,
) -> EconomicsResult<ProposalStatus> {
    // Tally the votes to determine the new status
    let new_status = tally_budget_votes(budget_id, proposal_id, storage).await?;
    
    // If the status is still Proposed, don't update anything
    if new_status == ProposalStatus::Proposed {
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
        budget.spent_by_proposal.insert(proposal_id, resources_map);
        
        // TODO(V3-MVP): Store generated ResourceAuthorizations for the proposer
        // This would involve creating ResourceAuthorization objects and storing them
    }
    
    // Save the updated budget
    save_budget(&budget, storage).await?;
    
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
    record_budget_vote(budget_id, proposal_id, approver_did, VoteChoice::Approve, storage).await?;
    
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

#[cfg(test)]
mod tests {
    use super::*;
    
    /// Create a test budget for testing
    async fn create_test_budget() -> (String, MockBudgetStorage) {
        let mut storage = MockBudgetStorage::new();
        
        let now = chrono::Utc::now().timestamp();
        let end = now + 3600 * 24 * 30; // 30 days from now
        
        let budget_id = create_budget(
            "Test Budget",
            "did:icn:test-coop",
            IdentityScope::Cooperative,
            now,
            end,
            None,
            &mut storage,
        ).await.unwrap();
        
        (budget_id, storage)
    }
    
    /// Create a test budget with a proposal for testing
    async fn create_test_budget_with_proposal() -> (String, Uuid, MockBudgetStorage) {
        let (budget_id, mut storage) = create_test_budget().await;
        
        // Allocate some resources first
        allocate_to_budget(
            &budget_id,
            ResourceType::Compute,
            1000,
            &mut storage,
        ).await.unwrap();
        
        // Create resource request
        let mut requested_resources = HashMap::new();
        requested_resources.insert(ResourceType::Compute, 500);
        
        // Create a proposal
        let proposal_id = propose_budget_spend(
            &budget_id,
            "Test Proposal",
            "A proposal to test budget spending",
            requested_resources,
            "did:icn:proposer",
            None,
            None,
            &mut storage,
        ).await.unwrap();
        
        (budget_id, proposal_id, storage)
    }
    
    #[tokio::test]
    async fn test_create_budget() {
        let mut storage = MockBudgetStorage::new();
        
        let now = chrono::Utc::now().timestamp();
        let end = now + 3600 * 24 * 30; // 30 days from now
        
        let budget_id = create_budget(
            "Test Budget",
            "did:icn:test-coop",
            IdentityScope::Cooperative,
            now,
            end,
            None,
            &mut storage,
        ).await.unwrap();
        
        // Verify that budget was stored
        let budget = load_budget(&budget_id, &storage).await.unwrap();
        assert_eq!(budget.name, "Test Budget");
        assert_eq!(budget.scope_id, "did:icn:test-coop");
        assert!(budget.total_allocated.is_empty());
    }
    
    #[tokio::test]
    async fn test_allocate_to_budget() {
        let (budget_id, mut storage) = create_test_budget().await;
        
        // Allocate some resources
        allocate_to_budget(
            &budget_id,
            ResourceType::Compute,
            1000,
            &mut storage,
        ).await.unwrap();
        
        // Verify allocation
        let budget = load_budget(&budget_id, &storage).await.unwrap();
        assert_eq!(budget.total_allocated.get(&ResourceType::Compute).cloned().unwrap_or(0), 1000);
        
        // Allocate more of the same resource type
        allocate_to_budget(
            &budget_id,
            ResourceType::Compute,
            500,
            &mut storage,
        ).await.unwrap();
        
        // Verify cumulative allocation
        let budget = load_budget(&budget_id, &storage).await.unwrap();
        assert_eq!(budget.total_allocated.get(&ResourceType::Compute).cloned().unwrap_or(0), 1500);
        
        // Allocate a different resource type
        allocate_to_budget(
            &budget_id,
            ResourceType::Storage,
            2000,
            &mut storage,
        ).await.unwrap();
        
        // Verify multiple resource types
        let budget = load_budget(&budget_id, &storage).await.unwrap();
        assert_eq!(budget.total_allocated.get(&ResourceType::Compute).cloned().unwrap_or(0), 1500);
        assert_eq!(budget.total_allocated.get(&ResourceType::Storage).cloned().unwrap_or(0), 2000);
    }
    
    #[tokio::test]
    async fn test_propose_budget_spend() {
        let (budget_id, mut storage) = create_test_budget().await;
        
        // Allocate some resources first
        allocate_to_budget(
            &budget_id,
            ResourceType::Compute,
            1000,
            &mut storage,
        ).await.unwrap();
        
        allocate_to_budget(
            &budget_id,
            ResourceType::Storage,
            2000,
            &mut storage,
        ).await.unwrap();
        
        // Create resource request
        let mut requested_resources = HashMap::new();
        requested_resources.insert(ResourceType::Compute, 500);
        requested_resources.insert(ResourceType::Storage, 800);
        
        // Create a proposal
        let proposal_id = propose_budget_spend(
            &budget_id,
            "Test Proposal",
            "A proposal to test budget spending",
            requested_resources,
            "did:icn:test-user",
            Some("maintenance".to_string()),
            None,
            &mut storage,
        ).await.unwrap();
        
        // Verify proposal was created
        let budget = load_budget(&budget_id, &storage).await.unwrap();
        assert_eq!(budget.proposals.len(), 1);
        
        let proposal = budget.proposals.get(&proposal_id).unwrap();
        assert_eq!(proposal.title, "Test Proposal");
        assert_eq!(proposal.proposer_did, "did:icn:test-user");
        assert_eq!(proposal.status, ProposalStatus::Proposed);
        assert_eq!(proposal.requested_resources.get(&ResourceType::Compute).cloned().unwrap_or(0), 500);
        assert_eq!(proposal.requested_resources.get(&ResourceType::Storage).cloned().unwrap_or(0), 800);
    }
    
    #[tokio::test]
    async fn test_query_budget_balance() {
        let (budget_id, mut storage) = create_test_budget().await;
        
        // Allocate some resources first
        allocate_to_budget(
            &budget_id,
            ResourceType::Compute,
            1000,
            &mut storage,
        ).await.unwrap();
        
        // Check balance with no spending
        let balance = query_budget_balance(
            &budget_id,
            &ResourceType::Compute,
            &storage,
        ).await.unwrap();
        
        assert_eq!(balance, 1000);
        
        // For now, spent_by_proposal isn't being updated since we haven't implemented
        // the approval logic, so we'll manually update it for testing
        let mut budget = load_budget(&budget_id, &storage).await.unwrap();
        let mut spent = HashMap::new();
        spent.insert(ResourceType::Compute, 300);
        budget.spent_by_proposal.insert(Uuid::new_v4(), spent);
        save_budget(&budget, &mut storage).await.unwrap();
        
        // Check balance after spending
        let balance = query_budget_balance(
            &budget_id,
            &ResourceType::Compute,
            &storage,
        ).await.unwrap();
        
        assert_eq!(balance, 700);
    }
    
    #[tokio::test]
    async fn test_record_vote() {
        let (budget_id, proposal_id, mut storage) = create_test_budget_with_proposal().await;
        
        // Record an approval vote
        record_budget_vote(
            &budget_id,
            proposal_id,
            "did:icn:voter1",
            VoteChoice::Approve,
            &mut storage,
        ).await.unwrap();
        
        // Check that the vote was recorded
        let budget = load_budget(&budget_id, &storage).await.unwrap();
        let proposal = budget.proposals.get(&proposal_id).unwrap();
        
        assert_eq!(proposal.votes.len(), 1);
        assert_eq!(proposal.votes.get("did:icn:voter1").unwrap(), &VoteChoice::Approve);
        
        // Record a rejection vote from another voter
        record_budget_vote(
            &budget_id,
            proposal_id,
            "did:icn:voter2",
            VoteChoice::Reject,
            &mut storage,
        ).await.unwrap();
        
        // Check that both votes were recorded
        let budget = load_budget(&budget_id, &storage).await.unwrap();
        let proposal = budget.proposals.get(&proposal_id).unwrap();
        
        assert_eq!(proposal.votes.len(), 2);
        assert_eq!(proposal.votes.get("did:icn:voter2").unwrap(), &VoteChoice::Reject);
        
        // Try to vote on a non-existent proposal
        let non_existent_id = Uuid::new_v4();
        let result = record_budget_vote(
            &budget_id,
            non_existent_id,
            "did:icn:voter1",
            VoteChoice::Approve,
            &mut storage,
        ).await;
        
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_tally_simple_majority() {
        let (budget_id, proposal_id, mut storage) = create_test_budget_with_proposal().await;
        
        // Initially no votes, should remain proposed
        let status = tally_budget_votes(&budget_id, proposal_id, &storage).await.unwrap();
        assert_eq!(status, ProposalStatus::Proposed);
        
        // Record votes
        record_budget_vote(&budget_id, proposal_id, "did:icn:voter1", VoteChoice::Approve, &mut storage).await.unwrap();
        record_budget_vote(&budget_id, proposal_id, "did:icn:voter2", VoteChoice::Approve, &mut storage).await.unwrap();
        record_budget_vote(&budget_id, proposal_id, "did:icn:voter3", VoteChoice::Reject, &mut storage).await.unwrap();
        
        // Tally votes - should be approved with 2/3 approval
        let status = tally_budget_votes(&budget_id, proposal_id, &storage).await.unwrap();
        assert_eq!(status, ProposalStatus::Approved);
        
        // Create a new proposal for testing rejection
        let mut requested_resources = HashMap::new();
        requested_resources.insert(ResourceType::Compute, 300);
        
        let proposal_id2 = propose_budget_spend(
            &budget_id,
            "Another Proposal",
            "A proposal that will be rejected",
            requested_resources,
            "did:icn:proposer",
            None,
            None,
            &mut storage,
        ).await.unwrap();
        
        // Record votes that will lead to rejection
        record_budget_vote(&budget_id, proposal_id2, "did:icn:voter1", VoteChoice::Reject, &mut storage).await.unwrap();
        record_budget_vote(&budget_id, proposal_id2, "did:icn:voter2", VoteChoice::Reject, &mut storage).await.unwrap();
        record_budget_vote(&budget_id, proposal_id2, "did:icn:voter3", VoteChoice::Approve, &mut storage).await.unwrap();
        
        // Tally votes - should be rejected with 2/3 rejection
        let status = tally_budget_votes(&budget_id, proposal_id2, &storage).await.unwrap();
        assert_eq!(status, ProposalStatus::Rejected);
    }
    
    #[tokio::test]
    async fn test_finalize_proposal() {
        let (budget_id, proposal_id, mut storage) = create_test_budget_with_proposal().await;
        
        // Record votes for approval
        record_budget_vote(&budget_id, proposal_id, "did:icn:voter1", VoteChoice::Approve, &mut storage).await.unwrap();
        record_budget_vote(&budget_id, proposal_id, "did:icn:voter2", VoteChoice::Approve, &mut storage).await.unwrap();
        
        // Finalize the proposal
        let status = finalize_budget_proposal(&budget_id, proposal_id, &mut storage).await.unwrap();
        assert_eq!(status, ProposalStatus::Approved);
        
        // Check that the status was updated in storage
        let budget = load_budget(&budget_id, &storage).await.unwrap();
        let proposal = budget.proposals.get(&proposal_id).unwrap();
        assert_eq!(proposal.status, ProposalStatus::Approved);
        
        // Check that the resources were recorded as spent
        assert!(budget.spent_by_proposal.contains_key(&proposal_id));
        let spent = budget.spent_by_proposal.get(&proposal_id).unwrap();
        assert_eq!(spent.get(&ResourceType::Compute).unwrap(), &500);
        
        // Try to vote on the proposal after it's approved
        let result = record_budget_vote(
            &budget_id,
            proposal_id,
            "did:icn:voter3",
            VoteChoice::Approve,
            &mut storage,
        ).await;
        
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_consensus_voting() {
        let mut storage = MockBudgetStorage::new();
        
        let now = chrono::Utc::now().timestamp();
        let end = now + 3600 * 24 * 30; // 30 days from now
        
        // Create budget with consensus voting rules
        let rules = BudgetRulesConfig {
            voting_method: Some("consensus".to_string()),
            min_participants: Some(3),
            categories: None,
            custom_rules: None,
        };
        
        let budget_id = create_budget(
            "Consensus Budget",
            "did:icn:test-coop",
            IdentityScope::Cooperative,
            now,
            end,
            Some(rules),
            &mut storage,
        ).await.unwrap();
        
        // Allocate resources
        allocate_to_budget(
            &budget_id,
            ResourceType::Compute,
            1000,
            &mut storage,
        ).await.unwrap();
        
        // Create a proposal
        let mut requested_resources = HashMap::new();
        requested_resources.insert(ResourceType::Compute, 500);
        
        let proposal_id = propose_budget_spend(
            &budget_id,
            "Consensus Proposal",
            "A proposal using consensus voting",
            requested_resources.clone(), // Clone here to keep ownership
            "did:icn:proposer",
            None,
            None,
            &mut storage,
        ).await.unwrap();
        
        // Add votes but not meeting quorum yet
        record_budget_vote(&budget_id, proposal_id, "did:icn:voter1", VoteChoice::Approve, &mut storage).await.unwrap();
        record_budget_vote(&budget_id, proposal_id, "did:icn:voter2", VoteChoice::Approve, &mut storage).await.unwrap();
        
        // Tally votes - should still be proposed due to not meeting quorum
        let status = tally_budget_votes(&budget_id, proposal_id, &storage).await.unwrap();
        assert_eq!(status, ProposalStatus::Proposed);
        
        // Add more votes to meet quorum and achieve consensus
        record_budget_vote(&budget_id, proposal_id, "did:icn:voter3", VoteChoice::Approve, &mut storage).await.unwrap();
        record_budget_vote(&budget_id, proposal_id, "did:icn:voter4", VoteChoice::Approve, &mut storage).await.unwrap();
        record_budget_vote(&budget_id, proposal_id, "did:icn:voter5", VoteChoice::Approve, &mut storage).await.unwrap();
        
        // Tally votes - should be approved with unanimous consensus
        let status = tally_budget_votes(&budget_id, proposal_id, &storage).await.unwrap();
        assert_eq!(status, ProposalStatus::Approved);
        
        // Create another proposal to test rejection
        let proposal_id2 = propose_budget_spend(
            &budget_id,
            "Another Consensus Proposal",
            "A proposal that will be rejected",
            requested_resources.clone(), // Clone again for the new proposal
            "did:icn:proposer",
            None,
            None,
            &mut storage,
        ).await.unwrap();
        
        // Add votes with one rejection (should block consensus)
        record_budget_vote(&budget_id, proposal_id2, "did:icn:voter1", VoteChoice::Approve, &mut storage).await.unwrap();
        record_budget_vote(&budget_id, proposal_id2, "did:icn:voter2", VoteChoice::Approve, &mut storage).await.unwrap();
        record_budget_vote(&budget_id, proposal_id2, "did:icn:voter3", VoteChoice::Approve, &mut storage).await.unwrap();
        record_budget_vote(&budget_id, proposal_id2, "did:icn:voter4", VoteChoice::Reject, &mut storage).await.unwrap();
        
        // Tally votes - should be rejected with one rejection in consensus voting
        let status = tally_budget_votes(&budget_id, proposal_id2, &storage).await.unwrap();
        assert_eq!(status, ProposalStatus::Rejected);
    }
    
    #[tokio::test]
    async fn test_quadratic_voting() {
        let mut storage = MockBudgetStorage::new();
        
        let now = chrono::Utc::now().timestamp();
        let end = now + 3600 * 24 * 30; // 30 days from now
        
        // Create budget with quadratic voting rules
        let rules = BudgetRulesConfig {
            voting_method: Some("quadratic".to_string()),
            min_participants: Some(2),
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
        
        // Allocate resources
        allocate_to_budget(
            &budget_id,
            ResourceType::Compute,
            1000,
            &mut storage,
        ).await.unwrap();
        
        // Create a proposal
        let mut requested_resources = HashMap::new();
        requested_resources.insert(ResourceType::Compute, 500);
        
        let proposal_id = propose_budget_spend(
            &budget_id,
            "Quadratic Proposal",
            "A proposal using quadratic voting",
            requested_resources,
            "did:icn:proposer",
            None,
            None,
            &mut storage,
        ).await.unwrap();
        
        // Add quadratic votes
        record_budget_vote(&budget_id, proposal_id, "did:icn:voter1", VoteChoice::Quadratic(4), &mut storage).await.unwrap();
        record_budget_vote(&budget_id, proposal_id, "did:icn:voter2", VoteChoice::Reject, &mut storage).await.unwrap();
        record_budget_vote(&budget_id, proposal_id, "did:icn:voter3", VoteChoice::Approve, &mut storage).await.unwrap();
        
        // Tally votes - should be approved because of quadratic weight (4 + 1 = 5 approvals vs 1 rejection)
        let status = tally_budget_votes(&budget_id, proposal_id, &storage).await.unwrap();
        assert_eq!(status, ProposalStatus::Approved);
    }
    
    #[tokio::test]
    async fn test_approve_budget_proposal() {
        let (budget_id, proposal_id, mut storage) = create_test_budget_with_proposal().await;
        
        // This should add an approval vote and attempt to finalize
        // Since there's only one vote, it will be approved (default quorum is 1)
        let result = approve_budget_proposal(
            &budget_id,
            proposal_id,
            "did:icn:approver",
            &mut storage,
        ).await;
        
        assert!(result.is_ok());
        
        // Check that the proposal was approved
        let budget = load_budget(&budget_id, &storage).await.unwrap();
        let proposal = budget.proposals.get(&proposal_id).unwrap();
        assert_eq!(proposal.status, ProposalStatus::Approved);
    }
} 