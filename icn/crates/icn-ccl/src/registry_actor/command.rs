//! Command types for the Contract Registry Actor
//!
//! Defines the message protocol used to communicate with the actor.

use crate::ast::Contract;
use crate::registry::{ContentHash, ContractMetadata, RegistryError, RegistryStats};
use tokio::sync::oneshot;

use super::types::{
    ApprovalStatus, ContractFilter, ContractRegistryMessage, ContractStatus, ContractSummary,
    DeploymentMetadata,
};

/// Commands sent to the ContractRegistryActor
pub(crate) enum ContractRegistryCommand {
    /// Register a new contract
    Register {
        contract: Contract,
        metadata: DeploymentMetadata,
        resp: oneshot::Sender<Result<ContentHash, RegistryError>>,
    },

    /// Get a contract by hash
    Get {
        hash: ContentHash,
        resp: oneshot::Sender<Option<Contract>>,
    },

    /// Get contract metadata by hash
    GetMetadata {
        hash: ContentHash,
        resp: oneshot::Sender<Option<ContractMetadata>>,
    },

    /// List contracts matching a filter
    List {
        filter: ContractFilter,
        resp: oneshot::Sender<Vec<ContractSummary>>,
    },

    /// Resolve contract by name and optional version
    Resolve {
        name: String,
        version: Option<u32>,
        resp: oneshot::Sender<Option<ContentHash>>,
    },

    /// Revoke a contract (owner only)
    Revoke {
        hash: ContentHash,
        requester: String,
        reason: String,
        resp: oneshot::Sender<Result<(), RegistryError>>,
    },

    /// Deprecate a contract with optional successor (owner only)
    Deprecate {
        hash: ContentHash,
        requester: String,
        successor: Option<ContentHash>,
        reason: String,
        resp: oneshot::Sender<Result<(), RegistryError>>,
    },

    /// Get contract lifecycle status
    GetStatus {
        hash: ContentHash,
        resp: oneshot::Sender<Option<ContractStatus>>,
    },

    /// Get version history for a contract by name
    GetVersionHistory {
        name: String,
        resp: oneshot::Sender<Vec<(u32, ContentHash)>>,
    },

    /// Handle incoming gossip message
    GossipMessage(ContractRegistryMessage),

    /// Check governance approval status for a contract
    CheckApproval {
        hash: ContentHash,
        resp: oneshot::Sender<ApprovalStatus>,
    },

    /// Get registry statistics
    Stats {
        resp: oneshot::Sender<RegistryStats>,
    },

    /// Check if a contract exists
    Exists {
        hash: ContentHash,
        resp: oneshot::Sender<bool>,
    },

    /// Get all contract hashes (for anti-entropy digest)
    GetAllHashes {
        resp: oneshot::Sender<Vec<ContentHash>>,
    },
}

impl std::fmt::Debug for ContractRegistryCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Register { metadata, .. } => f
                .debug_struct("Register")
                .field("owner", &metadata.owner)
                .finish(),
            Self::Get { hash, .. } => f
                .debug_struct("Get")
                .field("hash", &hex::encode(hash))
                .finish(),
            Self::GetMetadata { hash, .. } => f
                .debug_struct("GetMetadata")
                .field("hash", &hex::encode(hash))
                .finish(),
            Self::List { filter, .. } => f.debug_struct("List").field("filter", filter).finish(),
            Self::Resolve { name, version, .. } => f
                .debug_struct("Resolve")
                .field("name", name)
                .field("version", version)
                .finish(),
            Self::Revoke {
                hash, requester, ..
            } => f
                .debug_struct("Revoke")
                .field("hash", &hex::encode(hash))
                .field("requester", requester)
                .finish(),
            Self::Deprecate {
                hash,
                requester,
                successor,
                ..
            } => f
                .debug_struct("Deprecate")
                .field("hash", &hex::encode(hash))
                .field("requester", requester)
                .field("successor", &successor.map(hex::encode))
                .finish(),
            Self::GetStatus { hash, .. } => f
                .debug_struct("GetStatus")
                .field("hash", &hex::encode(hash))
                .finish(),
            Self::GetVersionHistory { name, .. } => f
                .debug_struct("GetVersionHistory")
                .field("name", name)
                .finish(),
            Self::GossipMessage(msg) => f
                .debug_struct("GossipMessage")
                .field("type", &msg.message_type())
                .finish(),
            Self::CheckApproval { hash, .. } => f
                .debug_struct("CheckApproval")
                .field("hash", &hex::encode(hash))
                .finish(),
            Self::Stats { .. } => f.debug_struct("Stats").finish(),
            Self::Exists { hash, .. } => f
                .debug_struct("Exists")
                .field("hash", &hex::encode(hash))
                .finish(),
            Self::GetAllHashes { .. } => f.debug_struct("GetAllHashes").finish(),
        }
    }
}
