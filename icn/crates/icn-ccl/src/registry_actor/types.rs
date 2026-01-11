//! Types for the Contract Registry Actor
//!
//! Defines gossip messages, deployment metadata, filters, and events.

use crate::ast::Contract;
use crate::registry::{ContentHash, ContractMetadata, Visibility};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Gossip topic for contract registry sync
pub const TOPIC_CONTRACTS: &str = "icn:contracts";

/// Gossip topic for contract deployment announcements
pub const TOPIC_CONTRACTS_DEPLOY: &str = "icn:contracts:deploy";

/// Gossip topic for contract revocation announcements
pub const TOPIC_CONTRACTS_REVOKE: &str = "icn:contracts:revoke";

/// Messages sent via gossip for contract registry synchronization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContractRegistryMessage {
    /// A new contract has been deployed
    Deployed {
        hash: ContentHash,
        contract: Contract,
        metadata: ContractMetadata,
    },

    /// A contract has been revoked by its owner
    Revoked {
        hash: ContentHash,
        owner: String,
        reason: String,
        revoked_at: u64,
    },

    /// A contract has been deprecated
    Deprecated {
        hash: ContentHash,
        owner: String,
        successor: Option<ContentHash>,
        reason: String,
        deprecated_at: u64,
    },

    /// Request for a specific contract (anti-entropy)
    Request {
        hash: ContentHash,
        requester: String,
    },

    /// Response with contract data
    Response {
        hash: ContentHash,
        contract: Option<Contract>,
        metadata: Option<ContractMetadata>,
    },

    /// Digest of available contracts (for anti-entropy)
    Digest {
        hashes: Vec<ContentHash>,
        sender: String,
    },
}

impl ContractRegistryMessage {
    /// Serialize message to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, icn_encoding::Error> {
        icn_encoding::encode_bincode_legacy(self)
    }

    /// Deserialize message from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, icn_encoding::Error> {
        icn_encoding::decode_bincode_legacy(bytes)
    }

    /// Get the message type name for logging
    pub fn message_type(&self) -> &'static str {
        match self {
            Self::Deployed { .. } => "Deployed",
            Self::Revoked { .. } => "Revoked",
            Self::Deprecated { .. } => "Deprecated",
            Self::Request { .. } => "Request",
            Self::Response { .. } => "Response",
            Self::Digest { .. } => "Digest",
        }
    }
}

/// Deployment metadata provided by the deployer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentMetadata {
    /// Owner/deployer DID
    pub owner: String,
    /// Version (None for auto-increment)
    pub version: Option<u32>,
    /// Visibility level
    pub visibility: Visibility,
    /// Optional description
    pub description: Option<String>,
}

impl DeploymentMetadata {
    /// Create new deployment metadata
    pub fn new(owner: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
            version: None,
            visibility: Visibility::default(),
            description: None,
        }
    }

    /// Set visibility
    pub fn with_visibility(mut self, visibility: Visibility) -> Self {
        self.visibility = visibility;
        self
    }

    /// Set version
    pub fn with_version(mut self, version: u32) -> Self {
        self.version = Some(version);
        self
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// Filter for listing contracts
#[derive(Debug, Clone, Default)]
pub struct ContractFilter {
    /// Filter by owner DID
    pub owner: Option<String>,
    /// Filter by name prefix
    pub name_prefix: Option<String>,
    /// Filter by visibility
    pub visibility: Option<Visibility>,
    /// Maximum number of results
    pub limit: Option<usize>,
}

impl ContractFilter {
    /// Create a new empty filter
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by owner
    pub fn with_owner(mut self, owner: impl Into<String>) -> Self {
        self.owner = Some(owner.into());
        self
    }

    /// Filter by name prefix
    pub fn with_name_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.name_prefix = Some(prefix.into());
        self
    }

    /// Filter by visibility
    pub fn with_visibility(mut self, visibility: Visibility) -> Self {
        self.visibility = Some(visibility);
        self
    }

    /// Limit results
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

/// Contract lifecycle status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum ContractStatus {
    /// Contract is active and can be executed
    #[default]
    Active,
    /// Contract is deprecated, with optional successor
    Deprecated {
        /// Hash of the replacement contract (if any)
        successor: Option<ContentHash>,
        /// Reason for deprecation
        reason: String,
        /// When the contract was deprecated (Unix millis)
        deprecated_at: u64,
    },
    /// Contract has been revoked and cannot be executed
    Revoked {
        /// Reason for revocation
        reason: String,
        /// When the contract was revoked (Unix millis)
        revoked_at: u64,
    },
}

impl ContractStatus {
    /// Check if the contract is active
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    /// Check if the contract is deprecated
    pub fn is_deprecated(&self) -> bool {
        matches!(self, Self::Deprecated { .. })
    }

    /// Check if the contract is revoked
    pub fn is_revoked(&self) -> bool {
        matches!(self, Self::Revoked { .. })
    }

    /// Get the successor hash if deprecated
    pub fn successor(&self) -> Option<ContentHash> {
        match self {
            Self::Deprecated { successor, .. } => *successor,
            _ => None,
        }
    }
}

/// Summary of a contract for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractSummary {
    /// Content hash
    pub hash: ContentHash,
    /// Contract name
    pub name: String,
    /// Owner DID
    pub owner: String,
    /// Version number
    pub version: u32,
    /// Deployment timestamp
    pub deployed_at: u64,
    /// Rule names
    pub rules: Vec<String>,
    /// Whether the contract is revoked
    pub revoked: bool,
    /// Contract lifecycle status
    pub status: ContractStatus,
}

impl From<&ContractMetadata> for ContractSummary {
    fn from(meta: &ContractMetadata) -> Self {
        Self {
            hash: meta.code_hash,
            name: meta.name.clone(),
            owner: meta.owner.clone(),
            version: meta.version,
            deployed_at: meta.deployed_at,
            rules: meta.rules.clone(),
            revoked: false,
            status: ContractStatus::Active,
        }
    }
}

/// Events emitted by the registry actor
#[derive(Debug, Clone)]
pub enum ContractRegistryEvent {
    /// Contract was deployed
    Deployed { hash: ContentHash, name: String },
    /// Contract was deprecated
    Deprecated {
        hash: ContentHash,
        successor: Option<ContentHash>,
        reason: String,
    },
    /// Contract was revoked
    Revoked { hash: ContentHash, reason: String },
    /// Governance approval is required for a public contract
    GovernanceApprovalRequired {
        hash: ContentHash,
        proposal_id: String,
    },
    /// Governance approved the contract
    GovernanceApproved { hash: ContentHash },
    /// Governance rejected the contract
    GovernanceRejected { hash: ContentHash },
    /// Contract was synced from a peer
    SyncedFromPeer { hash: ContentHash, peer: String },
}

/// Approval status for contracts requiring governance
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum ApprovalStatus {
    /// Approval not required (private/coop contracts)
    #[default]
    NotRequired,
    /// Pending governance approval
    Pending { proposal_id: String },
    /// Approved by governance
    Approved,
    /// Rejected by governance
    Rejected,
}

/// Callback type for sending gossip messages
pub type SendCallback = Arc<dyn Fn(ContractRegistryMessage) + Send + Sync>;

/// Callback type for emitting registry events
pub type EventCallback = Arc<dyn Fn(ContractRegistryEvent) + Send + Sync>;

/// Actor error type
#[derive(Debug, thiserror::Error)]
pub enum ActorError {
    #[error("Actor channel closed")]
    ChannelClosed,

    #[error("No response from actor")]
    NoResponse,

    #[error("Registry error: {0}")]
    Registry(#[from] crate::registry::RegistryError),

    #[error("Internal error: {0}")]
    Internal(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deployment_metadata_builder() {
        let meta = DeploymentMetadata::new("did:icn:alice")
            .with_visibility(Visibility::Public)
            .with_version(2)
            .with_description("Test contract");

        assert_eq!(meta.owner, "did:icn:alice");
        assert_eq!(meta.visibility, Visibility::Public);
        assert_eq!(meta.version, Some(2));
        assert_eq!(meta.description, Some("Test contract".to_string()));
    }

    #[test]
    fn test_contract_filter_builder() {
        let filter = ContractFilter::new()
            .with_owner("did:icn:bob")
            .with_name_prefix("Time")
            .with_limit(10);

        assert_eq!(filter.owner, Some("did:icn:bob".to_string()));
        assert_eq!(filter.name_prefix, Some("Time".to_string()));
        assert_eq!(filter.limit, Some(10));
    }

    #[test]
    fn test_message_serialization() {
        let msg = ContractRegistryMessage::Request {
            hash: [1u8; 32],
            requester: "did:icn:alice".to_string(),
        };

        let bytes = msg.to_bytes().expect("serialize");
        let decoded = ContractRegistryMessage::from_bytes(&bytes).expect("deserialize");

        match decoded {
            ContractRegistryMessage::Request { hash, requester } => {
                assert_eq!(hash, [1u8; 32]);
                assert_eq!(requester, "did:icn:alice");
            }
            _ => panic!("Wrong message type"),
        }
    }
}
