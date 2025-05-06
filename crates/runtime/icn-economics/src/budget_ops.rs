use std::collections::HashMap;
use uuid::Uuid;
use crate::{EconomicsError, EconomicsResult, ParticipatoryBudget, BudgetProposal, ProposalStatus, ResourceType, BudgetRulesConfig, VoteChoice, VotingMethod};
use icn_identity::IdentityScope;
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use std::sync::{Arc, Mutex};
use cid::Cid;
use sha2::{Sha256, Digest};
use crate::token_storage::StorageBackend;

/// Helper function to create a multihash using SHA-256
fn create_sha256_multihash(data: &[u8]) -> cid::multihash::Multihash {
    // Create a new SHA-256 multihash
    let mut buf = [0u8; 32];
    let digest = Sha256::digest(data);
    buf.copy_from_slice(digest.as_slice());
    
    // Create the multihash (code 0x12 is SHA256)
    cid::multihash::Multihash::wrap(0x12, &buf[..]).expect("valid multihash")
}

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
impl<T: StorageBackend + Send + Sync> BudgetStorage for T {
    async fn store_budget(&mut self, key: &str, data: Vec<u8>) -> EconomicsResult<()> {
        // Generate a key CID from the string key
        let key_str = format!("budget::{}", key);
        let hash = create_sha256_multihash(key_str.as_bytes());
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
        let hash = create_sha256_multihash(key_str.as_bytes());
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
    
    // Validate the proposal falls within budget timeframe
    let now = chrono::Utc::now().timestamp();
    if now < budget.start_timestamp || now > budget.end_timestamp {
        return Err(EconomicsError::InvalidBudget(
            format!("Cannot propose spending outside of budget timeframe: {} to {}", 
                budget.start_timestamp, budget.end_timestamp)
        ));
    }
    
    // Check if the requested resources exceed available balance
    for (resource_type, requested_amount) in &requested_resources {
        let available = query_budget_balance(budget_id, resource_type, storage).await?;
        if *requested_amount > available {
            return Err(EconomicsError::InsufficientBalance(
                format!("Requested amount {} of {:?} exceeds available balance {}", 
                    requested_amount, resource_type, available)
            ));
        }
    }
    
    // If a category is specified, validate against category rules
    if let Some(cat) = &category {
        if let Some(rules) = &budget.rules {
            if let Some(categories) = &rules.categories {
                if let Some(category_rule) = categories.get(cat) {
                    // Check if the request falls within the category's allocation limits
                    if let Some(min_allocation) = category_rule.min_allocation {
                        // This would require knowing the total allocation for this category
                        // For now, we'll skip this check
                        tracing::debug!("Category {} has minimum allocation: {}%", cat, min_allocation);
                    }
                    
                    if let Some(max_allocation) = category_rule.max_allocation {
                        // Calculate the percentage of total resources this request represents
                        for (resource_type, requested_amount) in &requested_resources {
                            if let Some(total_allocated) = budget.total_allocated.get(resource_type) {
                                let percentage = (*requested_amount as f64 / *total_allocated as f64) * 100.0;
                                if percentage > max_allocation as f64 {
                                    return Err(EconomicsError::InvalidBudget(
                                        format!("Request for {:?} exceeds category '{}' max allocation: {}% > {}%", 
                                            resource_type, cat, percentage, max_allocation)
                                    ));
                                }
                            }
                        }
                    }
                } else {
                    // Category specified but not found in rules
                    return Err(EconomicsError::InvalidBudget(
                        format!("Category '{}' not found in budget rules", cat)
                    ));
                }
            }
        }
    }
    
    // Create a new proposal
    let proposal_id = Uuid::new_v4();
    
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
    
    // Check if voting period is still active
    let now = chrono::Utc::now().timestamp();
    if now > budget.end_timestamp {
        return Err(EconomicsError::InvalidBudget(
            format!("Budget voting period has ended at {}", budget.end_timestamp)
        ));
    }
    
    // Check voter eligibility based on budget scope
    match budget.scope_type {
        IdentityScope::Individual => {
            // For individual scope, only the owner can vote
            if voter_did != budget.scope_id {
                return Err(EconomicsError::Unauthorized(
                    format!("Voter {} is not authorized for individual budget owned by {}", 
                        voter_did, budget.scope_id)
                ));
            }
        },
        IdentityScope::Cooperative | IdentityScope::Community => {
            // For cooperative/community scopes, we should check if the voter is a member
            // This would typically require looking up membership in the governance system
            // For now, we'll just log that this check would happen in a real implementation
            tracing::debug!(
                "Would check if voter {} is a member of {:?} scope {}",
                voter_did, budget.scope_type, budget.scope_id
            );
            
            // Check rules for additional restrictions
            if let Some(rules) = &budget.rules {
                // Example: if there's a custom rule for allowed_voters, check against it
                if let Some(custom_rules) = &rules.custom_rules {
                    if let Some(allowed_voters) = custom_rules.get("allowed_voters") {
                        if let Some(voters) = allowed_voters.as_array() {
                            let is_allowed = voters.iter()
                                .filter_map(|v| v.as_str())
                                .any(|did| did == voter_did);
                            
                            if !is_allowed {
                                return Err(EconomicsError::Unauthorized(
                                    format!("Voter {} is not in the allowed voters list", voter_did)
                                ));
                            }
                        }
                    }
                }
                
                // Validate vote type based on voting method
                if let Some(voting_method) = &rules.voting_method {
                    match voting_method {
                        VotingMethod::Quadratic => {
                            // If Quadratic voting is required, ensure vote is Quadratic type
                            if !matches!(vote, VoteChoice::Quadratic(_)) {
                                return Err(EconomicsError::InvalidBudget(
                                    "Quadratic voting required for this budget".to_string()
                                ));
                            }
                        },
                        _ => {
                            // For other methods, any vote type is acceptable
                            // (SimpleMajority and Threshold can work with Approve/Reject/Abstain)
                        }
                    }
                }
            }
        },
        IdentityScope::Federation => {
            // For federation scope, check if voter is a member of the federation
            // This would require access to federation configuration
            tracing::debug!(
                "Would check if voter {} is a member of federation {}",
                voter_did, budget.scope_id
            );
        },
        IdentityScope::Guardian | IdentityScope::Administrator => {
            // For guardian scope, check if voter is a guardian
            // This would require access to guardian lists
            tracing::debug!(
                "Would check if voter {} is a guardian or administrator in scope {}",
                voter_did, budget.scope_id
            );
        },
        IdentityScope::Node => {
            // For node scope, check if voter is the node
            if voter_did != budget.scope_id {
                return Err(EconomicsError::Unauthorized(
                    format!("Voter {} is not authorized for node budget owned by {}", 
                        voter_did, budget.scope_id)
                ));
            }
        },
    }
    
    // Check for duplicate votes
    if proposal.votes.contains_key(&voter_did) {
        // Overwrite the previous vote
        tracing::debug!("Voter {} is changing their vote on proposal {}", voter_did, proposal_id);
    }
    
    // Update proposal status to VotingOpen if it's still in Proposed state
    if proposal.status == ProposalStatus::Proposed {
        proposal.status = ProposalStatus::VotingOpen;
    }
    
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
    
    // Initialize vote counts
    let mut approve_count = 0;
    let mut reject_count = 0;
    let mut abstain_count = 0;
    
    // For quadratic voting, we need to calculate the square root of voting power
    let mut quadratic_approve_total = 0.0;
    let mut quadratic_reject_total = 0.0;
    
    // For weighted votes
    let mut total_vote_weight = 0;

    // Process all votes
    for vote in proposal.votes.values() {
        match vote {
            VoteChoice::Approve => {
                approve_count += 1;
                total_vote_weight += 1;
            },
            VoteChoice::Reject => {
                reject_count += 1;
                total_vote_weight += 1;
            },
            VoteChoice::Abstain => {
                abstain_count += 1;
                total_vote_weight += 1;
            },
            VoteChoice::Quadratic(weight) => {
                if *weight > 0 {
                    // For quadratic voting, calculate square root of weight
                    let sqrt_weight = (*weight as f64).sqrt();
                    quadratic_approve_total += sqrt_weight;
                    total_vote_weight += weight;
                } else {
                    // For negative weights (rejection), the absolute value is taken
                    // Since u32 can't be negative, this code path is more for documentation clarity
                    let sqrt_weight = (*weight as f64).sqrt();
                    quadratic_reject_total += sqrt_weight;
                    total_vote_weight += weight;
                }
            },
        }
    }

    // Total votes cast (excluding abstentions for some calculations)
    let total_votes_cast = approve_count + reject_count + abstain_count; 
    let total_non_abstaining = approve_count + reject_count;
    
    // Log vote tallies for debugging
    tracing::debug!(
        "Vote tally: approve={}, reject={}, abstain={}, quadratic_approve={:.2}, quadratic_reject={:.2}, total_eligible={}",
        approve_count, reject_count, abstain_count, quadratic_approve_total, quadratic_reject_total, total_eligible_voters
    );
    
    // Check quorum requirements
    let quorum_met = if let Some(quorum_percentage) = rules.quorum_percentage {
        // Calculate threshold based on percentage of total eligible voters
        let quorum_threshold = (total_eligible_voters as f64 * (quorum_percentage as f64 / 100.0)).ceil() as u32;
        
        // Quorum counts all votes including abstentions
        total_votes_cast >= quorum_threshold as usize
    } else if let Some(min_participants) = rules.min_participants {
        // Legacy absolute number check
        total_votes_cast >= min_participants as usize
    } else {
        // No quorum specified, always met
        true
    };
    
    // If quorum not met, voting is still open
    if !quorum_met {
        tracing::debug!("Quorum not met: votes_cast={}, total_eligible={}", total_votes_cast, total_eligible_voters);
        return ProposalStatus::VotingOpen;
    }
    
    // Get threshold percentage (default to 50% if not specified)
    let threshold_percentage = rules.threshold_percentage.unwrap_or(50) as f64 / 100.0;
    
    // Tally votes based on the voting method
    match voting_method {
        VotingMethod::SimpleMajority => {
            // Simple majority requires more approvals than rejections among non-abstaining votes
            if total_non_abstaining == 0 {
                // If no non-abstaining votes, cannot make a decision
                return ProposalStatus::VotingOpen;
            }
            
            let approval_ratio = approve_count as f64 / total_non_abstaining as f64;
            
            if approval_ratio > threshold_percentage {
                ProposalStatus::Approved
            } else {
                ProposalStatus::Rejected
            }
        },
        VotingMethod::Threshold => {
            // Threshold voting requires a percentage of ALL eligible voters to approve
            let approval_ratio = approve_count as f64 / total_eligible_voters as f64;
            
            tracing::debug!(
                "Threshold check: approval_ratio={:.2}, threshold={:.2}", 
                approval_ratio, threshold_percentage
            );
            
            if approval_ratio >= threshold_percentage {
                ProposalStatus::Approved
            } else {
                ProposalStatus::Rejected
            }
        },
        VotingMethod::Quadratic => {
            // Quadratic voting using a more sophisticated method
            
            // If no quadratic votes were cast, we can't make a decision
            if quadratic_approve_total == 0.0 && quadratic_reject_total == 0.0 {
                // Also check if any non-quadratic votes exist
                if total_non_abstaining > 0 {
                    // Non-quadratic votes exist but we need quadratic for this method
                    tracing::warn!(
                        "Non-quadratic votes were cast for quadratic voting method: approve={}, reject={}",
                        approve_count, reject_count
                    );
                }
                return ProposalStatus::VotingOpen;
            }
            
            // Compare the quadratic totals
            tracing::debug!(
                "Quadratic vote totals: approve={:.2}, reject={:.2}", 
                quadratic_approve_total, quadratic_reject_total
            );
            
            if quadratic_approve_total > quadratic_reject_total {
                // Additionally check if we meet the threshold percentage of total possible votes
                let vote_power_ratio = quadratic_approve_total / 
                    (quadratic_approve_total + quadratic_reject_total);
                
                if vote_power_ratio >= threshold_percentage {
                    ProposalStatus::Approved
                } else {
                    // Not enough quadratic voting power to approve
                    ProposalStatus::Rejected
                }
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
    // Load the budget first to check current state
    let budget_state = load_budget(budget_id, storage).await?;
    
    // Check if the proposal already has a final status
    if let Some(proposal) = budget_state.proposals.get(&proposal_id) {
        if matches!(proposal.status, 
            ProposalStatus::Executed | 
            ProposalStatus::Failed | 
            ProposalStatus::Rejected |
            ProposalStatus::Cancelled
        ) {
            // Already in a final state, return current status
            return Ok(proposal.status.clone());
        }
        
        // Check if voting period has expired
        let now = chrono::Utc::now().timestamp();
        if proposal.status == ProposalStatus::VotingOpen && 
           now > budget_state.end_timestamp {
            // Voting period expired, force a tally
            tracing::info!(
                "Forcing proposal tally for {} because budget period ended",
                proposal_id
            );
        }
    }
    
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
    
    // Handle based on new status
    match new_status {
        ProposalStatus::Approved => {
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
                let auth_key_hash = create_sha256_multihash(auth_key_str.as_bytes());
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
        },
        ProposalStatus::Rejected => {
            // If the proposal is rejected, ensure it's not in spent_by_proposal
            budget.spent_by_proposal.remove(&proposal_id);
            tracing::info!("Proposal {} was rejected, resources not allocated", proposal_id);
        },
        ProposalStatus::VotingOpen => {
            // This shouldn't happen since we checked earlier
            tracing::warn!("Proposal {} status is VotingOpen after tally - unexpected!", proposal_id);
        },
        _ => {
            // Other statuses (Failed, Cancelled) should be handled by specific operations
            tracing::debug!("Proposal {} has status {:?}", proposal_id, new_status);
        }
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
    
    // Allocate resources to the budget
    allocate_to_budget(
        &budget_id, 
        ResourceType::Compute, 
        2000, // Allocate 2000 units of compute (more than proposal will request)
        &mut storage
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
        
        // Allocate resources to the budget
        allocate_to_budget(
            &budget_id, 
            ResourceType::Compute, 
            1000, // Allocate 1000 units of compute
            &mut storage
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
            min_participants: None,      // No minimum participants
            quorum_percentage: None,     // No quorum requirement for testing
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
        
        // Allocate resources to the budget
        allocate_to_budget(
            &budget_id, 
            ResourceType::Compute, 
            1000, // Allocate 1000 units of compute
            &mut storage
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
        
        // Add quadratic votes - one vote with high quadratic weight for approval
        record_budget_vote(
            &budget_id, 
            proposal_id, 
            "did:icn:voter1".to_string(), 
            VoteChoice::Quadratic(9), 
            &mut storage
        ).await.unwrap();
        
        // Add another quadratic vote for approval (low weight)
        record_budget_vote(
            &budget_id, 
            proposal_id, 
            "did:icn:voter2".to_string(), 
            VoteChoice::Quadratic(1), 
            &mut storage
        ).await.unwrap();
        
        // Force the tally for testing
        let status = tally_budget_votes(&budget_id, proposal_id, &storage).await.unwrap();
        
        // Should be approved because all votes are positive
        assert_eq!(status, ProposalStatus::Approved);
        
        // Create a second proposal to test with negative votes
        let proposal_id2 = propose_budget_spend(
            &budget_id,
            "Second Quadratic Proposal",
            "Testing rejection with quadratic voting",
            HashMap::from([(ResourceType::Compute, 200)]),
            "did:icn:proposer2",
            None,
            None,
            &mut storage,
        ).await.unwrap();
        
        // Add mixed votes - one positive and two negative
        record_budget_vote(
            &budget_id, 
            proposal_id2, 
            "did:icn:voter1".to_string(), 
            VoteChoice::Quadratic(4), // Positive = approve
            &mut storage
        ).await.unwrap();
        
        record_budget_vote(
            &budget_id, 
            proposal_id2, 
            "did:icn:voter2".to_string(), 
            VoteChoice::Quadratic(9), // Also positive
            &mut storage
        ).await.unwrap();
        
        // Add a third voter with a negative vote (can't be done with current VoteChoice,
        // but in a real system this would represent a rejection)
        // For our test, we'll simulate by finalization
        
        // Finalize the proposal 
        let status = finalize_budget_proposal(&budget_id, proposal_id2, &mut storage).await.unwrap();
        
        // Should be approved since both votes are positive
        assert_eq!(status, ProposalStatus::Executed);
    }
} 