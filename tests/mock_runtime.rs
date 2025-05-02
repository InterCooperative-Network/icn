use std::sync::{Arc, Mutex};
use std::collections::{HashMap, HashSet};
use serde_json::{Value, json};
use uuid::Uuid;
use anyhow::{Result, anyhow};
use serde::{Serialize, Deserialize};
use axum::{
    routing::{get, post},
    Router, Json, extract::{Path, State},
    http::StatusCode,
};
use std::net::SocketAddr;
use chrono::{DateTime, Utc};
use tower_http::cors::{CorsLayer, Any};

/// Mock implementation of the ICN Runtime for testing
pub struct MockRuntime {
    proposals: Mutex<HashMap<String, Value>>,
    votes: Mutex<HashMap<String, Vec<Value>>>,
    executed: Mutex<HashSet<String>>,
    trust_bundles: Mutex<Vec<Value>>,
    guardians: Mutex<HashSet<String>>,
}

impl MockRuntime {
    pub fn new() -> Self {
        let mut runtime = Self {
            proposals: Mutex::new(HashMap::new()),
            votes: Mutex::new(HashMap::new()),
            executed: Mutex::new(HashSet::new()),
            trust_bundles: Mutex::new(Vec::new()),
            guardians: Mutex::new(HashSet::new()),
        };
        
        // Add some initial guardians
        {
            let mut guardians = runtime.guardians.lock().unwrap();
            guardians.insert("did:icn:guardian1".to_string());
            guardians.insert("did:icn:guardian2".to_string());
            guardians.insert("did:icn:guardian3".to_string());
        }
        
        // Add initial trust bundle
        {
            let mut trust_bundles = runtime.trust_bundles.lock().unwrap();
            trust_bundles.push(json!({
                "id": "bundle1",
                "name": "Initial Trust Bundle",
                "version": 1,
                "guardians": [
                    "did:icn:guardian1",
                    "did:icn:guardian2",
                    "did:icn:guardian3"
                ],
                "threshold": 2,
                "active": true
            }));
        }
        
        runtime
    }
    
    /// Handle a new proposal
    pub fn handle_proposal(&self, proposal: Value) -> Result<String> {
        // Generate a UUID for the proposal
        let id = Uuid::new_v4().to_string();
        
        // Store the proposal
        let mut proposals = self.proposals.lock().unwrap();
        proposals.insert(id.clone(), proposal);
        
        // Initialize votes for this proposal
        let mut votes = self.votes.lock().unwrap();
        votes.insert(id.clone(), Vec::new());
        
        Ok(id)
    }
    
    /// Handle a vote on a proposal
    pub fn handle_vote(&self, vote: Value) -> Result<()> {
        let proposal_id = vote["proposal_id"].as_str()
            .ok_or_else(|| anyhow!("Missing proposal_id in vote"))?;
        
        // Verify the proposal exists
        let proposals = self.proposals.lock().unwrap();
        if !proposals.contains_key(proposal_id) {
            return Err(anyhow!("Proposal not found: {}", proposal_id));
        }
        
        // Add the vote
        let mut votes = self.votes.lock().unwrap();
        let proposal_votes = votes.get_mut(proposal_id)
            .ok_or_else(|| anyhow!("Vote tracking not initialized for proposal: {}", proposal_id))?;
            
        proposal_votes.push(vote);
        
        Ok(())
    }
    
    /// Execute a proposal if it has sufficient votes
    pub fn handle_execute(&self, id: &str) -> Result<Value> {
        // Check if the proposal exists
        let proposals = self.proposals.lock().unwrap();
        let proposal = proposals.get(id)
            .ok_or_else(|| anyhow!("Proposal not found: {}", id))?;
            
        // Check if already executed
        {
            let executed = self.executed.lock().unwrap();
            if executed.contains(id) {
                return Err(anyhow!("Proposal already executed: {}", id));
            }
        }
        
        // Check votes
        let votes = self.votes.lock().unwrap();
        let proposal_votes = votes.get(id)
            .ok_or_else(|| anyhow!("No votes found for proposal: {}", id))?;
            
        // Count approve votes (simplified logic)
        let approve_count = proposal_votes.iter()
            .filter(|v| v["decision"].as_str().unwrap_or("") == "Approve")
            .count();
            
        let reject_count = proposal_votes.iter()
            .filter(|v| v["decision"].as_str().unwrap_or("") == "Reject")
            .count();
            
        // Require at least 2 votes and more approvals than rejections
        if proposal_votes.len() < 2 || approve_count <= reject_count {
            return Err(anyhow!("Insufficient votes to execute proposal: {}", id));
        }
        
        // Mark as executed
        {
            let mut executed = self.executed.lock().unwrap();
            executed.insert(id.to_string());
        }
        
        // Create execution result
        let result = json!({
            "proposal_id": id,
            "success": true,
            "executed_at": chrono::Utc::now().to_rfc3339(),
            "vote_count": {
                "approve": approve_count,
                "reject": reject_count,
                "abstain": proposal_votes.len() - approve_count - reject_count
            }
        });
        
        Ok(result)
    }
    
    /// Get trust bundles
    pub fn get_trust_bundles(&self) -> Vec<Value> {
        let trust_bundles = self.trust_bundles.lock().unwrap();
        trust_bundles.clone()
    }
    
    /// Check if a DID is a guardian
    pub fn is_guardian(&self, did: &str) -> bool {
        let guardians = self.guardians.lock().unwrap();
        guardians.contains(did)
    }
    
    /// Add a guardian
    pub fn add_guardian(&self, did: &str) -> Result<()> {
        let mut guardians = self.guardians.lock().unwrap();
        guardians.insert(did.to_string());
        Ok(())
    }
    
    /// Create a new trust bundle
    pub fn create_trust_bundle(&self, bundle: Value) -> Result<()> {
        let mut trust_bundles = self.trust_bundles.lock().unwrap();
        trust_bundles.push(bundle);
        Ok(())
    }
    
    /// Get all proposals
    pub fn get_proposals(&self) -> HashMap<String, Value> {
        let proposals = self.proposals.lock().unwrap();
        proposals.clone()
    }
    
    /// Get votes for a proposal
    pub fn get_votes(&self, proposal_id: &str) -> Option<Vec<Value>> {
        let votes = self.votes.lock().unwrap();
        votes.get(proposal_id).cloned()
    }
}

/// Create a shared instance for testing
pub fn create_test_runtime() -> Arc<MockRuntime> {
    Arc::new(MockRuntime::new())
}

// Types for proposal handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,
    pub proposal_type: String,
    pub content: serde_json::Value,
    pub status: ProposalStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub votes: HashMap<String, Vote>,
    pub execution_receipt: Option<ExecutionReceipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProposalStatus {
    Pending,
    Voting,
    Approved,
    Rejected,
    Executed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    pub guardian: String,
    pub decision: VoteDecision,
    pub reason: Option<String>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VoteDecision {
    Approve,
    Reject,
    Abstain,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReceipt {
    pub success: bool,
    pub timestamp: DateTime<Utc>,
    pub executor: String,
    pub votes: VoteSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteSummary {
    pub approve: usize,
    pub reject: usize,
    pub abstain: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProposalRequest {
    pub proposal_type: String,
    pub content: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteRequest {
    pub guardian: String,
    pub decision: VoteDecision,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionReceiptRequest {
    pub success: bool,
    pub executor: String,
}

// State for our mock runtime
#[derive(Clone)]
pub struct AppState {
    proposals: Arc<Mutex<HashMap<String, Proposal>>>,
    guardians: Arc<Mutex<Vec<String>>>,
}

impl AppState {
    pub fn new() -> Self {
        let guardians = vec![
            "did:icn:guardian1".to_string(),
            "did:icn:guardian2".to_string(),
            "did:icn:guardian3".to_string(),
        ];
        
        Self {
            proposals: Arc::new(Mutex::new(HashMap::new())),
            guardians: Arc::new(Mutex::new(guardians)),
        }
    }
    
    pub fn add_guardian(&self, did: String) {
        let mut guardians = self.guardians.lock().unwrap();
        if !guardians.contains(&did) {
            guardians.push(did);
        }
    }
}

// API Handlers
async fn health_check() -> StatusCode {
    StatusCode::OK
}

async fn get_proposals(
    State(state): State<AppState>,
) -> Json<Vec<Proposal>> {
    let proposals = state.proposals.lock().unwrap();
    let result: Vec<Proposal> = proposals.values().cloned().collect();
    Json(result)
}

async fn get_proposal(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Proposal>, StatusCode> {
    let proposals = state.proposals.lock().unwrap();
    
    match proposals.get(&id) {
        Some(proposal) => Ok(Json(proposal.clone())),
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn create_proposal(
    State(state): State<AppState>,
    Json(request): Json<CreateProposalRequest>,
) -> Json<Proposal> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now();
    
    let proposal = Proposal {
        id: id.clone(),
        proposal_type: request.proposal_type,
        content: request.content,
        status: ProposalStatus::Pending,
        created_at: now,
        updated_at: now,
        votes: HashMap::new(),
        execution_receipt: None,
    };
    
    let mut proposals = state.proposals.lock().unwrap();
    proposals.insert(id, proposal.clone());
    
    Json(proposal)
}

async fn vote_on_proposal(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<VoteRequest>,
) -> Result<Json<Proposal>, StatusCode> {
    let mut proposals = state.proposals.lock().unwrap();
    
    if let Some(proposal) = proposals.get_mut(&id) {
        if matches!(proposal.status, ProposalStatus::Pending | ProposalStatus::Voting) {
            // Update proposal status
            proposal.status = ProposalStatus::Voting;
            proposal.updated_at = Utc::now();
            
            // Add the vote
            let vote = Vote {
                guardian: request.guardian.clone(),
                decision: request.decision,
                reason: request.reason,
                timestamp: Utc::now(),
            };
            
            proposal.votes.insert(request.guardian, vote);
            
            // Check if we have all votes
            let guardians = state.guardians.lock().unwrap();
            let remaining = guardians.iter()
                .filter(|g| !proposal.votes.contains_key(g.as_str()))
                .count();
                
            if remaining == 0 {
                // Count votes
                let mut approve = 0;
                let mut reject = 0;
                let mut abstain = 0;
                
                for vote in proposal.votes.values() {
                    match vote.decision {
                        VoteDecision::Approve => approve += 1,
                        VoteDecision::Reject => reject += 1,
                        VoteDecision::Abstain => abstain += 1,
                    }
                }
                
                // Decide outcome
                if approve > reject {
                    proposal.status = ProposalStatus::Approved;
                } else {
                    proposal.status = ProposalStatus::Rejected;
                }
            }
            
            return Ok(Json(proposal.clone()));
        } else {
            return Err(StatusCode::CONFLICT);
        }
    }
    
    Err(StatusCode::NOT_FOUND)
}

async fn execute_proposal(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ExecutionReceiptRequest>,
) -> Result<Json<Proposal>, StatusCode> {
    let mut proposals = state.proposals.lock().unwrap();
    
    if let Some(proposal) = proposals.get_mut(&id) {
        if matches!(proposal.status, ProposalStatus::Approved) {
            proposal.updated_at = Utc::now();
            
            // Process votes for receipt
            let mut approve = 0;
            let mut reject = 0;
            let mut abstain = 0;
            
            for vote in proposal.votes.values() {
                match vote.decision {
                    VoteDecision::Approve => approve += 1,
                    VoteDecision::Reject => reject += 1,
                    VoteDecision::Abstain => abstain += 1,
                }
            }
            
            // Create receipt
            let receipt = ExecutionReceipt {
                success: request.success,
                timestamp: Utc::now(),
                executor: request.executor,
                votes: VoteSummary {
                    approve,
                    reject,
                    abstain,
                },
            };
            
            proposal.execution_receipt = Some(receipt);
            
            if request.success {
                proposal.status = ProposalStatus::Executed;
            } else {
                proposal.status = ProposalStatus::Failed;
            }
            
            return Ok(Json(proposal.clone()));
        } else {
            return Err(StatusCode::CONFLICT);
        }
    }
    
    Err(StatusCode::NOT_FOUND)
}

async fn get_guardians(
    State(state): State<AppState>,
) -> Json<Vec<String>> {
    let guardians = state.guardians.lock().unwrap();
    Json(guardians.clone())
}

async fn add_guardian(
    State(state): State<AppState>,
    Json(guardian): Json<String>,
) -> StatusCode {
    state.add_guardian(guardian);
    StatusCode::CREATED
}

// Main function to run the server
#[tokio::main]
async fn main() {
    let state = AppState::new();
    
    // Create a test proposal
    let mut proposals = state.proposals.lock().unwrap();
    let test_proposal = Proposal {
        id: "test-proposal-1".to_string(),
        proposal_type: "ConfigChange".to_string(),
        content: serde_json::json!({
            "title": "Increase Voting Period",
            "description": "Increase the voting period to 14 days",
            "parameter": "voting_period",
            "value": "14d"
        }),
        status: ProposalStatus::Pending,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        votes: HashMap::new(),
        execution_receipt: None,
    };
    proposals.insert(test_proposal.id.clone(), test_proposal);
    drop(proposals);
    
    // Build our application with routes
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
        
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/api/proposals", get(get_proposals))
        .route("/api/proposals", post(create_proposal))
        .route("/api/proposals/:id", get(get_proposal))
        .route("/api/proposals/:id/vote", post(vote_on_proposal))
        .route("/api/proposals/:id/execute", post(execute_proposal))
        .route("/api/guardians", get(get_guardians))
        .route("/api/guardians", post(add_guardian))
        .layer(cors)
        .with_state(state);
    
    // Run the server
    let addr = SocketAddr::from(([0, 0, 0, 0], 8081));
    println!("Mock Runtime server listening on {}", addr);
    
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

// For use as a library in tests that don't need the HTTP server
pub mod mock {
    use super::*;
    
    pub struct MockRuntime {
        state: AppState,
    }
    
    impl MockRuntime {
        pub fn new() -> Self {
            Self {
                state: AppState::new(),
            }
        }
        
        pub fn create_proposal(&self, request: CreateProposalRequest) -> Proposal {
            let id = Uuid::new_v4().to_string();
            let now = Utc::now();
            
            let proposal = Proposal {
                id: id.clone(),
                proposal_type: request.proposal_type,
                content: request.content,
                status: ProposalStatus::Pending,
                created_at: now,
                updated_at: now,
                votes: HashMap::new(),
                execution_receipt: None,
            };
            
            let mut proposals = self.state.proposals.lock().unwrap();
            proposals.insert(id, proposal.clone());
            
            proposal
        }
        
        pub fn vote_on_proposal(&self, id: &str, request: VoteRequest) -> Option<Proposal> {
            let mut proposals = self.state.proposals.lock().unwrap();
            
            if let Some(proposal) = proposals.get_mut(id) {
                if matches!(proposal.status, ProposalStatus::Pending | ProposalStatus::Voting) {
                    // Update proposal status
                    proposal.status = ProposalStatus::Voting;
                    proposal.updated_at = Utc::now();
                    
                    // Add the vote
                    let vote = Vote {
                        guardian: request.guardian.clone(),
                        decision: request.decision,
                        reason: request.reason,
                        timestamp: Utc::now(),
                    };
                    
                    proposal.votes.insert(request.guardian, vote);
                    
                    return Some(proposal.clone());
                }
            }
            
            None
        }
        
        pub fn execute_proposal(&self, id: &str, request: ExecutionReceiptRequest) -> Option<Proposal> {
            let mut proposals = self.state.proposals.lock().unwrap();
            
            if let Some(proposal) = proposals.get_mut(id) {
                if matches!(proposal.status, ProposalStatus::Approved) {
                    proposal.updated_at = Utc::now();
                    
                    // Process votes for receipt
                    let mut approve = 0;
                    let mut reject = 0;
                    let mut abstain = 0;
                    
                    for vote in proposal.votes.values() {
                        match vote.decision {
                            VoteDecision::Approve => approve += 1,
                            VoteDecision::Reject => reject += 1,
                            VoteDecision::Abstain => abstain += 1,
                        }
                    }
                    
                    // Create receipt
                    let receipt = ExecutionReceipt {
                        success: request.success,
                        timestamp: Utc::now(),
                        executor: request.executor,
                        votes: VoteSummary {
                            approve,
                            reject,
                            abstain,
                        },
                    };
                    
                    proposal.execution_receipt = Some(receipt);
                    
                    if request.success {
                        proposal.status = ProposalStatus::Executed;
                    } else {
                        proposal.status = ProposalStatus::Failed;
                    }
                    
                    return Some(proposal.clone());
                }
            }
            
            None
        }
    }
} 