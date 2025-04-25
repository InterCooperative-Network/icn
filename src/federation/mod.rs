use crate::api::{ApiClient, ApiConfig, ApiError};
use crate::identity::{Identity, IdentityError};
use crate::proposal::{Proposal, ProposalError, ProposalManager};
use crate::storage::{StorageManager, StorageError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use thiserror::Error;

// Export RPC-related types and implementations
pub mod rpc;
pub use rpc::{
    FederationRpcError, 
    FinalizationReceipt, 
    ReceiptSignature,
    FederationManifest, 
    FederationMemberRole,
    QuorumConfig, 
    FederationHealthMetrics,
    IFederationSync, 
    MockFederationSync
};

/// Errors that can occur in federation operations
#[derive(Debug, Error)]
pub enum FederationError {
    #[error("API error: {0}")]
    ApiError(#[from] ApiError),
    
    #[error("Identity error: {0}")]
    IdentityError(#[from] IdentityError),
    
    #[error("Proposal error: {0}")]
    ProposalError(#[from] ProposalError),
    
    #[error("Storage error: {0}")]
    StorageError(#[from] StorageError),
    
    #[error("RPC error: {0}")]
    RpcError(#[from] FederationRpcError),
    
    #[error("File error: {0}")]
    FileError(String),
    
    #[error("Federation runtime error: {0}")]
    RuntimeError(String),
    
    #[error("Monitoring canceled")]
    MonitoringCanceled,
}

/// Status of a proposal being monitored
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MonitoringStatus {
    Submitted,
    Voting,
    Executed,
    Rejected,
    Failed,
    Unknown,
}

/// Result of monitoring a proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringResult {
    pub proposal_hash: String,
    pub status: MonitoringStatus,
    pub yes_votes: usize,
    pub no_votes: usize,
    pub abstain_votes: usize,
    pub threshold: usize,
    pub event_id: Option<String>,
    pub executed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Options for monitoring a proposal
#[derive(Debug, Clone)]
pub struct MonitoringOptions {
    pub interval_seconds: u64,
    pub timeout_minutes: u64,
    pub verbose: bool,
}

impl Default for MonitoringOptions {
    fn default() -> Self {
        Self {
            interval_seconds: 30,
            timeout_minutes: 60, // 1 hour timeout
            verbose: false,
        }
    }
}

/// DAG status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagStatus {
    pub latest_vertex: String,
    pub proposal_id: Option<String>,
    pub vertex_count: u64,
    pub synced: bool,
    pub scope: Option<String>,
}

/// Audit information for a proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalAudit {
    pub hash: String,
    pub title: String,
    pub status: String,
    pub yes_votes: usize,
    pub no_votes: usize,
    pub abstain_votes: usize,
    pub threshold: usize,
    pub guardian_quorum_met: bool,
    pub execution_status: String,
    pub dag_receipt: Option<String>,
}

/// Federation runtime manager that integrates identity, API, and proposal functionality
pub struct FederationRuntime {
    api_client: ApiClient,
    identity: Identity,
    proposal_manager: ProposalManager,
    storage_manager: StorageManager,
    drafts_dir: PathBuf,
    cancel_monitoring: bool,
    // Add the federation sync for receipt management
    federation_sync: Box<dyn IFederationSync + Send>,
}

impl FederationRuntime {
    /// Create a new federation runtime with the given API config and identity
    pub fn new(
        api_config: ApiConfig, 
        identity: Identity,
        storage_manager: StorageManager
    ) -> Result<Self, FederationError> {
        let api_client = ApiClient::new(api_config)
            .map_err(FederationError::ApiError)?;
        
        let proposal_manager = ProposalManager::new(api_client.clone());
        
        let drafts_dir = PathBuf::from("drafts");
        // Create drafts directory if it doesn't exist
        if !drafts_dir.exists() {
            std::fs::create_dir_all(&drafts_dir)
                .map_err(|e| FederationError::FileError(format!("Failed to create drafts directory: {}", e)))?;
        }
        
        // Use the mock federation sync by default
        let federation_sync: Box<dyn IFederationSync + Send> = Box::new(MockFederationSync::new());
        
        Ok(Self {
            api_client,
            identity,
            proposal_manager,
            storage_manager,
            drafts_dir,
            cancel_monitoring: false,
            federation_sync,
        })
    }
    
    /// Set the federation sync implementation
    pub fn with_federation_sync(mut self, sync_impl: Box<dyn IFederationSync + Send>) -> Self {
        self.federation_sync = sync_impl;
        self
    }
    
    /// Get finalized receipts for a DID from the federation
    pub fn get_finalized_receipts_by_did(&self, did: &str) -> Result<Vec<FinalizationReceipt>, FederationError> {
        self.federation_sync.get_finalized_receipts_by_did(did)
            .map_err(FederationError::RpcError)
    }
    
    /// Get a federation manifest
    pub fn get_federation_manifest(&self, federation_id: &str) -> Result<FederationManifest, FederationError> {
        self.federation_sync.get_federation_manifest(federation_id)
            .map_err(FederationError::RpcError)
    }
    
    /// Set the drafts directory
    pub fn set_drafts_dir(&mut self, dir: PathBuf) {
        self.drafts_dir = dir;
    }
    
    /// Load a proposal from a file
    pub fn load_proposal(&self, path: &Path) -> Result<Proposal, FederationError> {
        self.proposal_manager.load_dsl(path, &self.identity)
            .map_err(FederationError::ProposalError)
    }
    
    /// Submit a proposal to the DAG
    pub fn submit_proposal(&self, path: &Path) -> Result<String, FederationError> {
        // Load the proposal
        let proposal = self.load_proposal(path)?;
        
        // Sign the proposal
        let signature = self.proposal_manager.sign_proposal(&proposal, &self.identity)
            .map_err(FederationError::ProposalError)?;
        
        // Submit the proposal
        let submitted = self.proposal_manager.submit_proposal(&proposal, &signature, &self.identity)
            .map_err(FederationError::ProposalError)?;
        
        // Return the proposal hash
        Ok(submitted.proposal.hash)
    }
    
    /// Submit a proposal and monitor its progress
    pub fn submit_and_monitor(
        &mut self, 
        path: &Path, 
        options: Option<MonitoringOptions>
    ) -> Result<MonitoringResult, FederationError> {
        // Submit the proposal
        let proposal_hash = self.submit_proposal(path)?;
        
        // Move the proposal to submitted directory
        let filename = path.file_name()
            .ok_or_else(|| FederationError::FileError("Invalid file path".to_string()))?
            .to_string_lossy()
            .to_string();
        
        let submitted_dir = self.drafts_dir.join("submitted");
        if !submitted_dir.exists() {
            std::fs::create_dir_all(&submitted_dir)
                .map_err(|e| FederationError::FileError(format!("Failed to create submitted directory: {}", e)))?;
        }
        
        let dest_path = submitted_dir.join(&filename);
        std::fs::copy(path, &dest_path)
            .map_err(|e| FederationError::FileError(format!("Failed to copy proposal: {}", e)))?;
        
        // Only delete the source file if the copy was successful and it's in the drafts directory
        if path.starts_with(&self.drafts_dir) {
            std::fs::remove_file(path)
                .map_err(|e| FederationError::FileError(format!("Failed to remove proposal: {}", e)))?;
        }
        
        // Start monitoring with the specified options or defaults
        self.monitor_proposal(&proposal_hash, options.unwrap_or_default())
    }
    
    /// Monitor a proposal's progress
    pub fn monitor_proposal(
        &mut self, 
        proposal_hash: &str,
        options: MonitoringOptions
    ) -> Result<MonitoringResult, FederationError> {
        println!("Monitoring proposal: {}", proposal_hash);
        
        let start_time = chrono::Utc::now();
        let timeout = chrono::Duration::minutes(options.timeout_minutes as i64);
        self.cancel_monitoring = false;
        
        loop {
            // Check if monitoring was canceled
            if self.cancel_monitoring {
                return Err(FederationError::MonitoringCanceled);
            }
            
            // Check if timeout has been reached
            if chrono::Utc::now() - start_time > timeout {
                println!("Monitoring timeout reached after {} minutes", options.timeout_minutes);
                break;
            }
            
            // Query proposal status
            let query_result = self.proposal_manager.query_proposal(proposal_hash, &self.identity);
            
            match query_result {
                Ok(submitted_proposal) => {
                    let status = submitted_proposal.proposal.status.clone();
                    
                    // Count votes
                    let mut yes_votes = 0;
                    let mut no_votes = 0;
                    let mut abstain_votes = 0;
                    
                    for vote in &submitted_proposal.votes {
                        match vote.vote {
                            crate::proposal::VoteOption::Yes => yes_votes += 1,
                            crate::proposal::VoteOption::No => no_votes += 1,
                            crate::proposal::VoteOption::Abstain => abstain_votes += 1,
                        }
                    }
                    
                    // Default threshold (could be retrieved from proposal metadata)
                    let threshold = submitted_proposal.votes.len() / 2 + 1;
                    
                    // Print status if verbose
                    if options.verbose {
                        println!("Status: {:?}", status);
                        println!("Votes: {} yes / {} no / {} abstain", yes_votes, no_votes, abstain_votes);
                        println!("Threshold: {}", threshold);
                    }
                    
                    // Convert status to MonitoringStatus
                    let monitoring_status = match status {
                        crate::proposal::ProposalStatus::Submitted => MonitoringStatus::Submitted,
                        crate::proposal::ProposalStatus::Voting => MonitoringStatus::Voting,
                        crate::proposal::ProposalStatus::Passed => {
                            // Check if execution has happened
                            if submitted_proposal.dag_receipt.is_some() {
                                MonitoringStatus::Executed
                            } else {
                                MonitoringStatus::Voting
                            }
                        },
                        crate::proposal::ProposalStatus::Rejected => MonitoringStatus::Rejected,
                        crate::proposal::ProposalStatus::Executed => MonitoringStatus::Executed,
                        crate::proposal::ProposalStatus::Failed => MonitoringStatus::Failed,
                        crate::proposal::ProposalStatus::Draft => MonitoringStatus::Submitted,
                    };
                    
                    // If proposal is executed, return the result
                    if monitoring_status == MonitoringStatus::Executed {
                        return Ok(MonitoringResult {
                            proposal_hash: proposal_hash.to_string(),
                            status: monitoring_status,
                            yes_votes,
                            no_votes,
                            abstain_votes,
                            threshold,
                            event_id: submitted_proposal.dag_receipt,
                            executed_at: Some(chrono::Utc::now()),
                        });
                    }
                    
                    // If proposal is rejected or failed, return the result
                    if monitoring_status == MonitoringStatus::Rejected || monitoring_status == MonitoringStatus::Failed {
                        return Ok(MonitoringResult {
                            proposal_hash: proposal_hash.to_string(),
                            status: monitoring_status,
                            yes_votes,
                            no_votes,
                            abstain_votes,
                            threshold,
                            event_id: None,
                            executed_at: None,
                        });
                    }
                },
                Err(e) => {
                    if options.verbose {
                        println!("Error querying proposal: {}", e);
                    }
                }
            }
            
            // Sleep for the specified interval
            thread::sleep(Duration::from_secs(options.interval_seconds));
        }
        
        // If we reached here, we hit the timeout
        Ok(MonitoringResult {
            proposal_hash: proposal_hash.to_string(),
            status: MonitoringStatus::Unknown,
            yes_votes: 0,
            no_votes: 0,
            abstain_votes: 0,
            threshold: 0,
            event_id: None,
            executed_at: None,
        })
    }
    
    /// Cancel ongoing monitoring
    pub fn cancel_monitoring(&mut self) {
        self.cancel_monitoring = true;
    }
    
    /// Get DAG status
    pub fn get_dag_status(&self, scope: Option<&str>) -> Result<DagStatus, FederationError> {
        let url = match scope {
            Some(s) => format!("{}/dag/status?scope={}", self.api_client.base_url(), s),
            None => format!("{}/dag/status", self.api_client.base_url()),
        };
        
        let response: DagStatusResponse = self.api_client.get(&url, Some(&self.identity))
            .map_err(FederationError::ApiError)?;
        
        if !response.success {
            return Err(FederationError::RuntimeError(
                response.error.unwrap_or_else(|| "Unknown error getting DAG status".to_string())
            ));
        }
        
        response.data.ok_or_else(|| FederationError::RuntimeError("No DAG status data returned".to_string()))
    }
    
    /// List active proposals
    pub fn list_active_proposals(&self) -> Result<Vec<Proposal>, FederationError> {
        let url = format!("{}/proposals/list", self.api_client.base_url());
        
        let response: ListProposalsResponse = self.api_client.get(&url, Some(&self.identity))
            .map_err(FederationError::ApiError)?;
        
        if !response.success {
            return Err(FederationError::RuntimeError(
                response.error.unwrap_or_else(|| "Unknown error listing proposals".to_string())
            ));
        }
        
        let proposals_data = response.data.ok_or_else(|| FederationError::RuntimeError("No proposals data returned".to_string()))?;
        
        Ok(proposals_data.proposals.into_iter().map(|p| p.proposal).collect())
    }
    
    /// Audit a proposal
    pub fn audit_proposal(&self, hash: &str) -> Result<ProposalAudit, FederationError> {
        // Query the proposal
        let submitted_proposal = self.proposal_manager.query_proposal(hash, &self.identity)
            .map_err(FederationError::ProposalError)?;
        
        // Count votes
        let mut yes_votes = 0;
        let mut no_votes = 0;
        let mut abstain_votes = 0;
        let mut guardian_votes = 0;
        
        for vote in &submitted_proposal.votes {
            match vote.vote {
                crate::proposal::VoteOption::Yes => yes_votes += 1,
                crate::proposal::VoteOption::No => no_votes += 1,
                crate::proposal::VoteOption::Abstain => abstain_votes += 1,
            }
            
            // Check if vote is from a guardian
            if vote.guardian_signature.is_some() {
                guardian_votes += 1;
            }
        }
        
        // Calculate threshold (simple majority for now)
        let threshold = submitted_proposal.votes.len() / 2 + 1;
        
        // Determine if guardian quorum is met (at least 1 guardian vote)
        let guardian_quorum_met = guardian_votes > 0;
        
        // Determine execution status
        let execution_status = if submitted_proposal.dag_receipt.is_some() {
            "Executed on DAG"
        } else if submitted_proposal.proposal.status == crate::proposal::ProposalStatus::Passed {
            "Approved, awaiting execution"
        } else if submitted_proposal.proposal.status == crate::proposal::ProposalStatus::Rejected {
            "Rejected"
        } else if submitted_proposal.proposal.status == crate::proposal::ProposalStatus::Failed {
            "Failed"
        } else {
            "Pending"
        };
        
        Ok(ProposalAudit {
            hash: submitted_proposal.proposal.hash,
            title: submitted_proposal.proposal.title,
            status: format!("{:?}", submitted_proposal.proposal.status),
            yes_votes,
            no_votes,
            abstain_votes,
            threshold,
            guardian_quorum_met,
            execution_status: execution_status.to_string(),
            dag_receipt: submitted_proposal.dag_receipt,
        })
    }
    
    /// Sync a proposal with AgoraNet
    pub fn sync_with_agoranet(&self, proposal_hash: &str) -> Result<bool, FederationError> {
        let url = format!("{}/dag/sync/{}", self.api_client.base_url(), proposal_hash);
        
        let response: SyncResponse = self.api_client.post(&url, &serde_json::Value::Null, Some(&self.identity))
            .map_err(FederationError::ApiError)?;
        
        Ok(response.success)
    }
    
    /// Get the API config
    pub fn get_api_config(&self) -> &ApiConfig {
        self.api_client.get_config()
    }
    
    /// Get the identity
    pub fn get_identity(&self) -> &Identity {
        &self.identity
    }
}

// Response types for API calls

#[derive(Debug, Deserialize)]
struct DagStatusResponse {
    success: bool,
    data: Option<DagStatus>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListProposalsResponse {
    success: bool,
    data: Option<ProposalsData>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProposalsData {
    proposals: Vec<crate::proposal::SubmittedProposal>,
}

#[derive(Debug, Deserialize)]
struct SyncResponse {
    success: bool,
    message: String,
} 