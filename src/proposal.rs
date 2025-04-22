use crate::api::{ApiClient, ApiError};
use crate::identity::{Identity, IdentityError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::str::FromStr;
use thiserror::Error;
use base64::{Engine as _};

#[derive(Debug, Error)]
pub enum ProposalError {
    #[error("Failed to read proposal file: {0}")]
    FileReadError(String),
    
    #[error("Invalid DSL syntax: {0}")]
    SyntaxError(String),
    
    #[error("Failed to sign proposal: {0}")]
    SigningError(#[from] IdentityError),
    
    #[error("API error: {0}")]
    ApiError(#[from] ApiError),
    
    #[error("Failed to parse proposal: {0}")]
    ParseError(String),
    
    #[error("Proposal submission error: {0}")]
    SubmissionError(String),
    
    #[error("Invalid vote option: {0}")]
    InvalidVoteOption(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
}

/// Vote options for proposals
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VoteOption {
    Yes,
    No,
    Abstain,
}

impl FromStr for VoteOption {
    type Err = ProposalError;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "yes" => Ok(VoteOption::Yes),
            "no" => Ok(VoteOption::No),
            "abstain" => Ok(VoteOption::Abstain),
            _ => Err(ProposalError::InvalidVoteOption(s.to_string())),
        }
    }
}

/// Status of a proposal
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProposalStatus {
    Draft,
    Submitted,
    Voting,
    Passed,
    Rejected,
    Executed,
    Failed,
}

/// A proposal represents a .dsl program that can be voted on
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    /// Unique hash of the proposal
    pub hash: String,
    /// Title of the proposal
    pub title: String,
    /// Description/purpose of the proposal
    pub description: String,
    /// DID of the proposer
    pub proposer: String,
    /// When the proposal was created
    pub created_at: DateTime<Utc>,
    /// DSL program content
    pub program: String,
    /// Metadata of the proposal
    pub metadata: HashMap<String, String>,
    /// Current status
    pub status: ProposalStatus,
}

/// A vote on a proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    /// Proposal hash being voted on
    pub proposal_hash: String,
    /// DID of the voter
    pub voter: String,
    /// The vote cast
    pub vote: VoteOption,
    /// Any reasoning or comment
    pub comment: Option<String>,
    /// When the vote was cast
    pub timestamp: DateTime<Utc>,
    /// Signature of the vote
    pub signature: String,
}

/// SubmittedProposal contains both the proposal and votes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmittedProposal {
    /// The proposal
    pub proposal: Proposal,
    /// Collected votes
    pub votes: Vec<Vote>,
    /// Verification that this proposal exists on the DAG
    pub dag_receipt: Option<String>,
    /// When this proposal was last updated
    pub updated_at: DateTime<Utc>,
}

/// The ProposalManager handles proposals and votes
pub struct ProposalManager {
    api_client: ApiClient,
}

impl ProposalManager {
    /// Create a new proposal manager
    pub fn new(api_client: ApiClient) -> Self {
        Self { api_client }
    }
    
    /// Load a DSL file and parse it into a proposal
    pub fn load_dsl(&self, path: &Path, identity: &Identity) -> Result<Proposal, ProposalError> {
        // Read the file
        let contents = fs::read_to_string(path)
            .map_err(|e| ProposalError::FileReadError(e.to_string()))?;
        
        // Parse the file to extract metadata from comments
        let (title, description) = self.parse_dsl_metadata(&contents)?;
        
        // Generate a hash of the content
        let hash = self.hash_proposal(&contents);
        
        Ok(Proposal {
            hash,
            title,
            description,
            proposer: identity.did().to_string(),
            created_at: Utc::now(),
            program: contents,
            metadata: HashMap::new(),
            status: ProposalStatus::Draft,
        })
    }
    
    /// Parse metadata from DSL comments
    /// Expects comments like:
    /// // Title: My Proposal Title
    /// // Description: This proposal will...
    fn parse_dsl_metadata(&self, content: &str) -> Result<(String, String), ProposalError> {
        let mut title = String::new();
        let mut description = String::new();
        
        for line in content.lines() {
            let line = line.trim();
            
            if line.starts_with("//") {
                let comment = line[2..].trim();
                
                if let Some(title_content) = comment.strip_prefix("Title:") {
                    title = title_content.trim().to_string();
                } else if let Some(desc_content) = comment.strip_prefix("Description:") {
                    description = desc_content.trim().to_string();
                }
            }
        }
        
        if title.is_empty() {
            return Err(ProposalError::ParseError("Title not found in DSL file".to_string()));
        }
        
        Ok((title, description))
    }
    
    /// Compute a hash for a proposal
    fn hash_proposal(&self, content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let result = hasher.finalize();
        
        format!("{:x}", result)
    }
    
    /// Sign a proposal using an identity
    pub fn sign_proposal(
        &self,
        proposal: &Proposal,
        identity: &Identity,
    ) -> Result<String, ProposalError> {
        // Create the data to sign (hash + proposer + timestamp)
        let signing_data = format!(
            "{}:{}:{}",
            proposal.hash,
            proposal.proposer,
            proposal.created_at.timestamp()
        );
        
        // Sign the data
        let signature = identity.sign(signing_data.as_bytes())?;
        
        // Convert to base64
        Ok(base64::engine::general_purpose::STANDARD.encode(&signature))
    }
    
    /// Submit a proposal to the CoVM
    pub fn submit_proposal(
        &self,
        proposal: &Proposal,
        signature: &str,
        identity: &Identity,
    ) -> Result<SubmittedProposal, ProposalError> {
        // Create a submission object
        let mut submission_data = HashMap::new();
        submission_data.insert("type".to_string(), "proposal".to_string());
        submission_data.insert("hash".to_string(), proposal.hash.clone());
        submission_data.insert("title".to_string(), proposal.title.clone());
        submission_data.insert("description".to_string(), proposal.description.clone());
        submission_data.insert("program".to_string(), proposal.program.clone());
        submission_data.insert("proposer".to_string(), identity.did().to_string());
        submission_data.insert("signature".to_string(), signature.to_string());
        submission_data.insert("timestamp".to_string(), Utc::now().timestamp().to_string());
        
        // Convert to JSON
        let submission_json = serde_json::to_string(&submission_data)
            .map_err(|e| ProposalError::SerializationError(e.to_string()))?;
        
        // Submit to the API
        let response = self.api_client.query::<String>(&submission_json, identity)?;
        
        if !response.success {
            return Err(ProposalError::SubmissionError(
                response.message.clone(),
            ));
        }
        
        // Create a submitted proposal object
        let mut submitted = SubmittedProposal {
            proposal: proposal.clone(),
            votes: Vec::new(),
            dag_receipt: response.data,
            updated_at: Utc::now(),
        };
        
        // Update status
        submitted.proposal.status = ProposalStatus::Submitted;
        
        Ok(submitted)
    }
    
    /// Cast a vote on a proposal
    pub fn cast_vote(
        &self,
        proposal_hash: &str,
        vote_option: VoteOption,
        comment: Option<String>,
        identity: &Identity,
    ) -> Result<Vote, ProposalError> {
        // Create a vote object
        let vote = Vote {
            proposal_hash: proposal_hash.to_string(),
            voter: identity.did().to_string(),
            vote: vote_option,
            comment,
            timestamp: Utc::now(),
            signature: String::new(), // Will be set below
        };
        
        // Create the data to sign
        let vote_data = format!(
            "{}:{}:{:?}:{}",
            vote.proposal_hash,
            vote.voter,
            vote.vote,
            vote.timestamp.timestamp()
        );
        
        // Sign the vote data
        let signature = identity.sign(vote_data.as_bytes())?;
        let signature_b64 = base64::engine::general_purpose::STANDARD.encode(&signature);
        
        // Create a complete vote with signature
        let mut complete_vote = vote;
        complete_vote.signature = signature_b64;
        
        // Create a submission object
        let mut submission_data = HashMap::new();
        submission_data.insert("type".to_string(), "vote".to_string());
        submission_data.insert("proposal_hash".to_string(), complete_vote.proposal_hash.clone());
        submission_data.insert("voter".to_string(), complete_vote.voter.clone());
        submission_data.insert("vote".to_string(), format!("{:?}", complete_vote.vote));
        if let Some(comment) = &complete_vote.comment {
            submission_data.insert("comment".to_string(), comment.clone());
        }
        submission_data.insert("timestamp".to_string(), complete_vote.timestamp.timestamp().to_string());
        submission_data.insert("signature".to_string(), complete_vote.signature.clone());
        
        // Convert to JSON
        let submission_json = serde_json::to_string(&submission_data)
            .map_err(|e| ProposalError::SerializationError(e.to_string()))?;
        
        // Submit to the API
        let response = self.api_client.query::<String>(&submission_json, identity)?;
        
        if !response.success {
            return Err(ProposalError::SubmissionError(
                response.message.clone(),
            ));
        }
        
        Ok(complete_vote)
    }
    
    /// Query a proposal's status and votes
    pub fn query_proposal(
        &self,
        proposal_hash: &str,
        identity: &Identity,
    ) -> Result<SubmittedProposal, ProposalError> {
        // Create a query object
        let mut query_data = HashMap::new();
        query_data.insert("type".to_string(), "query_proposal".to_string());
        query_data.insert("hash".to_string(), proposal_hash.to_string());
        
        // Convert to JSON
        let query_json = serde_json::to_string(&query_data)
            .map_err(|e| ProposalError::SerializationError(e.to_string()))?;
        
        // Submit to the API
        let response = self.api_client.query::<SubmittedProposal>(&query_json, identity)?;
        
        match response.data {
            Some(proposal) => Ok(proposal),
            None => Err(ProposalError::ParseError(format!(
                "Proposal not found: {}",
                proposal_hash
            ))),
        }
    }
    
    /// List all proposals
    pub fn list_proposals(
        &self,
        scope: Option<&str>,
        identity: &Identity,
    ) -> Result<Vec<SubmittedProposal>, ProposalError> {
        // Create a query object
        let mut query_data = HashMap::new();
        query_data.insert("type".to_string(), "list_proposals".to_string());
        if let Some(scope_value) = scope {
            query_data.insert("scope".to_string(), scope_value.to_string());
        }
        
        // Convert to JSON
        let query_json = serde_json::to_string(&query_data)
            .map_err(|e| ProposalError::SerializationError(e.to_string()))?;
        
        // Submit to the API
        let response = self.api_client.query::<Vec<SubmittedProposal>>(&query_json, identity)?;
        
        match response.data {
            Some(proposals) => Ok(proposals),
            None => Ok(Vec::new()),
        }
    }
    
    /// Save a proposal to a file
    pub fn save_proposal(&self, proposal: &Proposal, path: &Path) -> Result<(), ProposalError> {
        let json = serde_json::to_string_pretty(proposal)
            .map_err(|e| ProposalError::SerializationError(e.to_string()))?;
        
        fs::write(path, json)
            .map_err(|e| ProposalError::FileReadError(e.to_string()))?;
        
        Ok(())
    }
    
    /// Load a proposal from a file
    pub fn load_proposal(&self, path: &Path) -> Result<Proposal, ProposalError> {
        let json = fs::read_to_string(path)
            .map_err(|e| ProposalError::FileReadError(e.to_string()))?;
        
        let proposal: Proposal = serde_json::from_str(&json)
            .map_err(|e| ProposalError::ParseError(e.to_string()))?;
        
        Ok(proposal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ApiConfig;
    use crate::identity::KeyType;
    use std::io::Write;
    use tempfile::NamedTempFile;
    
    fn create_test_identity() -> Identity {
        Identity::new("test", "alice", KeyType::Ed25519).unwrap()
    }
    
    fn create_test_dsl_file() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        
        let content = r#"// Title: Test Proposal
// Description: This is a test proposal for unit testing
function main() {
    // This is a test DSL program
    return {
        type: "test",
        action: "none"
    };
}
"#;
        
        file.write_all(content.as_bytes()).unwrap();
        file
    }
    
    #[test]
    fn test_proposal_parsing() {
        // Create a mock API client
        let api_client = ApiClient::new(ApiConfig::default()).unwrap();
        let proposal_manager = ProposalManager::new(api_client);
        
        // Create a test DSL file
        let dsl_file = create_test_dsl_file();
        
        // Create a test identity
        let identity = create_test_identity();
        
        // Test loading and parsing
        let proposal = proposal_manager.load_dsl(dsl_file.path(), &identity).unwrap();
        
        assert_eq!(proposal.title, "Test Proposal");
        assert_eq!(proposal.description, "This is a test proposal for unit testing");
        assert_eq!(proposal.proposer, identity.did());
        assert_eq!(proposal.status, ProposalStatus::Draft);
    }
    
    #[test]
    fn test_proposal_hash_consistency() {
        // Create a mock API client
        let api_client = ApiClient::new(ApiConfig::default()).unwrap();
        let proposal_manager = ProposalManager::new(api_client);
        
        // Create the same content twice
        let content = "function test() { return 42; }";
        
        // Hash should be consistent
        let hash1 = proposal_manager.hash_proposal(content);
        let hash2 = proposal_manager.hash_proposal(content);
        
        assert_eq!(hash1, hash2);
        
        // Different content should have different hash
        let different_content = "function test() { return 43; }";
        let hash3 = proposal_manager.hash_proposal(different_content);
        
        assert_ne!(hash1, hash3);
    }
    
    #[test]
    fn test_vote_option_parsing() {
        assert_eq!(VoteOption::from_str("yes").unwrap(), VoteOption::Yes);
        assert_eq!(VoteOption::from_str("YES").unwrap(), VoteOption::Yes);
        assert_eq!(VoteOption::from_str("no").unwrap(), VoteOption::No);
        assert_eq!(VoteOption::from_str("No").unwrap(), VoteOption::No);
        assert_eq!(VoteOption::from_str("abstain").unwrap(), VoteOption::Abstain);
        
        assert!(VoteOption::from_str("maybe").is_err());
    }
} 