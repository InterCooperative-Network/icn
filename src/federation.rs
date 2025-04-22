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

/// Federation runtime manager that integrates identity, API, and proposal functionality
pub struct FederationRuntime {
    api_client: ApiClient,
    identity: Identity,
    proposal_manager: ProposalManager,
    storage_manager: StorageManager,
    drafts_dir: PathBuf,
    cancel_monitoring: bool,
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
        
        Ok(Self {
            api_client,
            identity,
            proposal_manager,
            storage_manager,
            drafts_dir,
            cancel_monitoring: false,
        })
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
                    
                    // If the proposal has been executed or rejected, we're done
                    if matches!(monitoring_status, 
                        MonitoringStatus::Executed | 
                        MonitoringStatus::Rejected | 
                        MonitoringStatus::Failed
                    ) {
                        return Ok(MonitoringResult {
                            proposal_hash: proposal_hash.to_string(),
                            status: monitoring_status,
                            yes_votes,
                            no_votes,
                            abstain_votes,
                            threshold,
                            event_id: submitted_proposal.dag_receipt,
                            executed_at: Some(submitted_proposal.updated_at),
                        });
                    }
                },
                Err(e) => {
                    if options.verbose {
                        println!("Error querying proposal: {}", e);
                    }
                }
            }
            
            // Wait for the specified interval
            thread::sleep(Duration::from_secs(options.interval_seconds));
        }
        
        // If we've reached here, we timed out without a definitive result
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
    
    /// Get the DAG status for a specific scope
    pub fn get_dag_status(&self, scope: Option<&str>) -> Result<DagStatus, FederationError> {
        // Query the DAG status from the API
        let status_query = match scope {
            Some(s) => format!("{{ \"type\": \"dag_status\", \"scope\": \"{}\" }}", s),
            None => "{ \"type\": \"dag_status\" }".to_string(),
        };
        
        let status_response = self.api_client.query::<DagStatusResponse>(&status_query, &self.identity)
            .map_err(FederationError::ApiError)?;
        
        if !status_response.success {
            return Err(FederationError::RuntimeError(
                status_response.error.unwrap_or_else(|| "Unknown error".to_string())
            ));
        }
        
        status_response.data.ok_or_else(|| FederationError::RuntimeError(
            "No DAG status data returned".to_string()
        ))
    }
    
    /// Get all active proposals
    pub fn list_active_proposals(&self) -> Result<Vec<Proposal>, FederationError> {
        let query = "{ \"type\": \"list_proposals\", \"status\": \"voting\" }";
        
        let response = self.api_client.query::<ListProposalsResponse>(query, &self.identity)
            .map_err(FederationError::ApiError)?;
        
        if !response.success {
            return Err(FederationError::RuntimeError(
                response.error.unwrap_or_else(|| "Unknown error".to_string())
            ));
        }
        
        let proposals = response.data.ok_or_else(|| FederationError::RuntimeError(
            "No proposal data returned".to_string()
        ))?;
        
        Ok(proposals.proposals.into_iter().map(|sp| sp.proposal).collect())
    }
    
    /// Audit a proposal by hash
    pub fn audit_proposal(&self, hash: &str) -> Result<ProposalAudit, FederationError> {
        let submitted = self.proposal_manager.query_proposal(hash, &self.identity)
            .map_err(FederationError::ProposalError)?;
        
        // Count votes
        let mut yes_votes = 0;
        let mut no_votes = 0;
        let mut abstain_votes = 0;
        
        for vote in &submitted.votes {
            match vote.vote {
                crate::proposal::VoteOption::Yes => yes_votes += 1,
                crate::proposal::VoteOption::No => no_votes += 1,
                crate::proposal::VoteOption::Abstain => abstain_votes += 1,
            }
        }
        
        // Default threshold (could be retrieved from proposal metadata)
        let threshold = submitted.votes.len() / 2 + 1;
        
        // Determine if guardian quorum is met
        let guardian_quorum_met = yes_votes >= threshold;
        
        // Convert status to string
        let status = format!("{:?}", submitted.proposal.status);
        
        // Determine execution status
        let execution_status = if submitted.dag_receipt.is_some() {
            "Executed"
        } else if submitted.proposal.status == crate::proposal::ProposalStatus::Rejected {
            "Rejected"
        } else if submitted.proposal.status == crate::proposal::ProposalStatus::Failed {
            "Failed"
        } else {
            "Pending"
        };
        
        Ok(ProposalAudit {
            hash: hash.to_string(),
            title: submitted.proposal.title,
            status,
            yes_votes,
            no_votes,
            abstain_votes,
            threshold,
            guardian_quorum_met,
            execution_status: execution_status.to_string(),
            dag_receipt: submitted.dag_receipt,
        })
    }
    
    /// Sync proposal with AgoraNet
    pub fn sync_with_agoranet(&self, proposal_hash: &str) -> Result<bool, FederationError> {
        let query = format!(
            "{{ \"type\": \"agoranet_sync\", \"proposal_hash\": \"{}\" }}", 
            proposal_hash
        );
        
        let response = self.api_client.query::<SyncResponse>(&query, &self.identity)
            .map_err(FederationError::ApiError)?;
        
        Ok(response.success)
    }
    
    /// Get the API configuration
    pub fn get_api_config(&self) -> &ApiConfig {
        &self.api_client.config
    }
    
    /// Get the identity used by this runtime
    pub fn get_identity(&self) -> &Identity {
        &self.identity
    }
}

/// Response from DAG status query
#[derive(Debug, Deserialize)]
struct DagStatusResponse {
    success: bool,
    data: Option<DagStatus>,
    error: Option<String>,
}

/// DAG status information
#[derive(Debug, Serialize, Deserialize)]
pub struct DagStatus {
    pub latest_vertex: String,
    pub proposal_id: Option<String>,
    pub vertex_count: u64,
    pub synced: bool,
    pub scope: Option<String>,
}

/// Response from list proposals query
#[derive(Debug, Deserialize)]
struct ListProposalsResponse {
    success: bool,
    data: Option<ProposalsData>,
    error: Option<String>,
}

/// Proposals data
#[derive(Debug, Deserialize)]
struct ProposalsData {
    proposals: Vec<crate::proposal::SubmittedProposal>,
}

/// Response from sync query
#[derive(Debug, Deserialize)]
struct SyncResponse {
    success: bool,
    message: String,
}

/// Proposal audit information
#[derive(Debug, Serialize, Deserialize)]
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