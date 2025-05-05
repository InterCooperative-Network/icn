use cid::Cid;
use icn_identity::{Did, QuorumProof};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Proposal to merge two federations into a new one
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeProposal {
    /// First source federation DID
    pub src_fed_a: Did,
    
    /// Second source federation DID
    pub src_fed_b: Did,
    
    /// Content ID pointing to metadata for the new federation
    pub new_meta_cid: Cid,
    
    /// Quorum configuration for the new federation
    pub quorum_cfg: QuorumConfig,
    
    /// Challenge window in seconds during which the merge can be contested
    pub challenge_window_secs: u64,
    
    /// Approval proof from federation A
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_a: Option<QuorumProof>,
    
    /// Approval proof from federation B
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_b: Option<QuorumProof>,
}

/// Proposal to split a federation into two new ones
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitProposal {
    /// Parent federation DID
    pub parent_fed: Did,
    
    /// Content ID pointing to the partition map defining how assets and members are divided
    pub partition_map_cid: Cid,
    
    /// Quorum configuration for the resulting federations
    pub quorum_cfg: QuorumConfig,
    
    /// Challenge window in seconds during which the split can be contested
    pub challenge_window_secs: u64,
    
    /// Approval proof from parent federation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval: Option<QuorumProof>,
    
    /// DID for the first resulting federation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub federation_a_id: Option<Did>,
    
    /// DID for the second resulting federation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub federation_b_id: Option<Did>,
}

/// The type of lineage attestation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LineageAttestationType {
    /// Attests that federations merged
    Merge,
    
    /// Attests that a federation split
    Split,
}

/// Attestation documenting the lineage relationship between federations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageAttestation {
    /// Parent federation DIDs
    pub parents: Vec<Did>,
    
    /// Child federation DIDs
    pub children: Vec<Did>,
    
    /// Type of lineage attestation
    pub typ: LineageAttestationType,
    
    /// Proof of quorum approval
    pub proof: QuorumProof,
    
    /// Timestamp of attestation creation
    pub timestamp: chrono::DateTime<chrono::Utc>,
    
    /// Additional metadata
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub metadata: HashMap<String, String>,
}

/// Quorum configuration for federation decision-making
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuorumConfig {
    /// Minimum number of signers required for quorum
    pub threshold: u32,
    
    /// Authorized signers for quorum decisions
    pub authorized_signers: Vec<Did>,
    
    /// Optional weights for authorized signers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weights: Option<HashMap<Did, u32>>,
}

/// Mapping of member assets and resources for federation partition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionMap {
    /// Federation members assigned to first partition
    pub members_a: Vec<Did>,
    
    /// Federation members assigned to second partition
    pub members_b: Vec<Did>,
    
    /// Resource allocation for first partition
    pub resources_a: HashMap<String, ResourceAllocation>,
    
    /// Resource allocation for second partition
    pub resources_b: HashMap<String, ResourceAllocation>,
    
    /// Ledger balances for first partition
    pub ledger_a: HashMap<Did, u64>,
    
    /// Ledger balances for second partition
    pub ledger_b: HashMap<Did, u64>,
}

/// Resource allocation details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceAllocation {
    /// Resource identifier
    pub resource_id: String,
    
    /// Resource amount
    pub amount: u64,
    
    /// Resource metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

/// Process status for federation merge operations
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MergeStatus {
    /// Process has been initiated
    Initiated,
    
    /// Process is in challenge window
    InChallengeWindow,
    
    /// Process is being executed
    Executing,
    
    /// Process has completed successfully
    Completed,
    
    /// Process has been cancelled
    Cancelled,
    
    /// Process has failed
    Failed,
}

/// Process status for federation split operations
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SplitStatus {
    /// Process has been initiated
    Initiated,
    
    /// Process is in challenge window
    InChallengeWindow,
    
    /// Process is being executed
    Executing,
    
    /// Process has completed successfully
    Completed,
    
    /// Process has been cancelled
    Cancelled,
    
    /// Process has failed
    Failed,
}

/// Tracks the state of a federation merge process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeProcess {
    /// Process identifier
    pub id: String,
    
    /// First source federation DID
    pub federation_a_id: Did,
    
    /// Second source federation DID
    pub federation_b_id: Did,
    
    /// New federation DID
    pub new_federation_id: Did,
    
    /// Merge proposal
    pub merge_proposal: MergeProposal,
    
    /// Trust mapping between federations
    pub trust_mapping: TrustMapping,
    
    /// Merged governance policy
    pub merged_policy: HashMap<String, String>,
    
    /// Merged trust bundle
    pub merged_bundle: PreMergeBundle,
    
    /// Current status of the process
    pub status: MergeStatus,
    
    /// Process start time
    pub start_time: chrono::DateTime<chrono::Utc>,
    
    /// Process completion time
    pub completion_time: Option<chrono::DateTime<chrono::Utc>>,
}

/// Tracks the state of a federation split process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitProcess {
    /// Process identifier
    pub id: String,
    
    /// Original federation DID
    pub original_federation_id: Did,
    
    /// First resulting federation DID
    pub federation_a_id: Did,
    
    /// Second resulting federation DID
    pub federation_b_id: Did,
    
    /// Split proposal
    pub split_proposal: SplitProposal,
    
    /// Trust mapping for first federation
    pub trust_mapping_a: TrustMapping,
    
    /// Trust mapping for second federation
    pub trust_mapping_b: TrustMapping,
    
    /// Governance policy for first federation
    pub policy_a: HashMap<String, String>,
    
    /// Governance policy for second federation
    pub policy_b: HashMap<String, String>,
    
    /// Trust bundle for first federation
    pub bundle_a: SplitBundle,
    
    /// Trust bundle for second federation
    pub bundle_b: SplitBundle,
    
    /// Current status of the process
    pub status: SplitStatus,
    
    /// Process start time
    pub start_time: chrono::DateTime<chrono::Utc>,
    
    /// Process completion time
    pub completion_time: Option<chrono::DateTime<chrono::Utc>>,
}

/// Trust mapping between federation entities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustMapping {
    /// Mapping of DIDs from source to target
    pub did_mappings: HashMap<Did, Did>,
    
    /// Roles assigned to DIDs in target federation
    pub role_assignments: HashMap<Did, Vec<String>>,
    
    /// Credential validations between federations
    pub credential_validations: Vec<CredentialValidation>,
}

/// Rules for credential validation across federations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialValidation {
    /// Source credential type
    pub source_type: String,
    
    /// Target credential type
    pub target_type: String,
    
    /// Source federation issuer
    pub source_issuer: Did,
    
    /// Target federation issuer
    pub target_issuer: Did,
    
    /// Validation rules
    pub rules: HashMap<String, String>,
}

/// Bundle for merger of two federations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreMergeBundle {
    /// DAG roots of source federations
    pub dag_roots: Vec<Cid>,
    
    /// Metadata for new federation
    pub metadata: HashMap<String, String>,
    
    /// Lineage attestation
    pub lineage: LineageAttestation,
    
    /// Cryptographic proofs
    pub proofs: Vec<QuorumProof>,
}

/// Bundle for splitting a federation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitBundle {
    /// DAG root of parent federation
    pub parent_root: Cid,
    
    /// Partition map
    pub partition_map: PartitionMap,
    
    /// Lineage attestation
    pub lineage: LineageAttestation,
    
    /// Cryptographic proofs
    pub proofs: Vec<QuorumProof>,
} 