//! Handle for interacting with the Contract Registry Actor
//!
//! Provides a cloneable, async API for contract registry operations.

use crate::ast::Contract;
use crate::registry::{ContentHash, ContractMetadata, RegistryStats};
use tokio::sync::{mpsc, oneshot};

use super::command::ContractRegistryCommand;
use super::types::{
    ActorError, ApprovalStatus, ContractFilter, ContractRegistryMessage, ContractStatus,
    ContractSummary, DeploymentMetadata,
};

/// Handle for interacting with the ContractRegistryActor
///
/// This handle is cloneable and can be shared across tasks.
#[derive(Clone)]
pub struct ContractRegistryHandle {
    pub(crate) tx: mpsc::Sender<ContractRegistryCommand>,
}

impl ContractRegistryHandle {
    /// Create a new handle with the given sender
    pub(crate) fn new(tx: mpsc::Sender<ContractRegistryCommand>) -> Self {
        Self { tx }
    }

    /// Register a new contract in the registry
    ///
    /// # Arguments
    /// * `contract` - The CCL contract to register
    /// * `metadata` - Deployment metadata (owner, visibility, etc.)
    ///
    /// # Returns
    /// The content hash of the registered contract
    pub async fn register_contract(
        &self,
        contract: Contract,
        metadata: DeploymentMetadata,
    ) -> Result<ContentHash, ActorError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(ContractRegistryCommand::Register {
                contract,
                metadata,
                resp: resp_tx,
            })
            .await
            .map_err(|_| ActorError::ChannelClosed)?;
        resp_rx
            .await
            .map_err(|_| ActorError::NoResponse)?
            .map_err(ActorError::Registry)
    }

    /// Get a contract by its content hash
    ///
    /// This is the primary method used by ComputeActor to resolve CclRef.
    pub async fn get_contract(&self, hash: &ContentHash) -> Result<Option<Contract>, ActorError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(ContractRegistryCommand::Get {
                hash: *hash,
                resp: resp_tx,
            })
            .await
            .map_err(|_| ActorError::ChannelClosed)?;
        resp_rx.await.map_err(|_| ActorError::NoResponse)
    }

    /// Get contract metadata by hash
    pub async fn get_metadata(
        &self,
        hash: &ContentHash,
    ) -> Result<Option<ContractMetadata>, ActorError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(ContractRegistryCommand::GetMetadata {
                hash: *hash,
                resp: resp_tx,
            })
            .await
            .map_err(|_| ActorError::ChannelClosed)?;
        resp_rx.await.map_err(|_| ActorError::NoResponse)
    }

    /// List contracts matching a filter
    pub async fn list_contracts(
        &self,
        filter: ContractFilter,
    ) -> Result<Vec<ContractSummary>, ActorError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(ContractRegistryCommand::List {
                filter,
                resp: resp_tx,
            })
            .await
            .map_err(|_| ActorError::ChannelClosed)?;
        resp_rx.await.map_err(|_| ActorError::NoResponse)
    }

    /// Resolve a contract by name and optional version
    ///
    /// If version is None, returns the latest version.
    pub async fn resolve(
        &self,
        name: &str,
        version: Option<u32>,
    ) -> Result<Option<ContentHash>, ActorError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(ContractRegistryCommand::Resolve {
                name: name.to_string(),
                version,
                resp: resp_tx,
            })
            .await
            .map_err(|_| ActorError::ChannelClosed)?;
        resp_rx.await.map_err(|_| ActorError::NoResponse)
    }

    /// Revoke a contract (owner only)
    ///
    /// # Arguments
    /// * `hash` - Content hash of the contract to revoke
    /// * `requester` - DID of the requester (must be owner)
    /// * `reason` - Reason for revocation
    pub async fn revoke_contract(
        &self,
        hash: &ContentHash,
        requester: &str,
        reason: String,
    ) -> Result<(), ActorError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(ContractRegistryCommand::Revoke {
                hash: *hash,
                requester: requester.to_string(),
                reason,
                resp: resp_tx,
            })
            .await
            .map_err(|_| ActorError::ChannelClosed)?;
        resp_rx
            .await
            .map_err(|_| ActorError::NoResponse)?
            .map_err(ActorError::Registry)
    }

    /// Deprecate a contract with optional successor (owner only)
    ///
    /// # Arguments
    /// * `hash` - Content hash of the contract to deprecate
    /// * `requester` - DID of the requester (must be owner)
    /// * `successor` - Optional hash of the replacement contract
    /// * `reason` - Reason for deprecation
    pub async fn deprecate_contract(
        &self,
        hash: &ContentHash,
        requester: &str,
        successor: Option<ContentHash>,
        reason: String,
    ) -> Result<(), ActorError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(ContractRegistryCommand::Deprecate {
                hash: *hash,
                requester: requester.to_string(),
                successor,
                reason,
                resp: resp_tx,
            })
            .await
            .map_err(|_| ActorError::ChannelClosed)?;
        resp_rx
            .await
            .map_err(|_| ActorError::NoResponse)?
            .map_err(ActorError::Registry)
    }

    /// Get the lifecycle status of a contract
    pub async fn get_status(
        &self,
        hash: &ContentHash,
    ) -> Result<Option<ContractStatus>, ActorError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(ContractRegistryCommand::GetStatus {
                hash: *hash,
                resp: resp_tx,
            })
            .await
            .map_err(|_| ActorError::ChannelClosed)?;
        resp_rx.await.map_err(|_| ActorError::NoResponse)
    }

    /// Get version history for a contract by name
    ///
    /// Returns a list of (version, hash) pairs for all versions of the contract.
    pub async fn get_version_history(
        &self,
        name: &str,
    ) -> Result<Vec<(u32, ContentHash)>, ActorError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(ContractRegistryCommand::GetVersionHistory {
                name: name.to_string(),
                resp: resp_tx,
            })
            .await
            .map_err(|_| ActorError::ChannelClosed)?;
        resp_rx.await.map_err(|_| ActorError::NoResponse)
    }

    /// Handle an incoming gossip message
    ///
    /// Called by the supervisor when a message arrives on contract registry topics.
    pub async fn handle_gossip(&self, msg: ContractRegistryMessage) -> Result<(), ActorError> {
        self.tx
            .send(ContractRegistryCommand::GossipMessage(Box::new(msg)))
            .await
            .map_err(|_| ActorError::ChannelClosed)
    }

    /// Check governance approval status for a contract
    pub async fn check_approval(&self, hash: &ContentHash) -> Result<ApprovalStatus, ActorError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(ContractRegistryCommand::CheckApproval {
                hash: *hash,
                resp: resp_tx,
            })
            .await
            .map_err(|_| ActorError::ChannelClosed)?;
        resp_rx.await.map_err(|_| ActorError::NoResponse)
    }

    /// Get registry statistics
    pub async fn stats(&self) -> Result<RegistryStats, ActorError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(ContractRegistryCommand::Stats { resp: resp_tx })
            .await
            .map_err(|_| ActorError::ChannelClosed)?;
        resp_rx.await.map_err(|_| ActorError::NoResponse)
    }

    /// Check if a contract exists in the registry
    pub async fn exists(&self, hash: &ContentHash) -> Result<bool, ActorError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(ContractRegistryCommand::Exists {
                hash: *hash,
                resp: resp_tx,
            })
            .await
            .map_err(|_| ActorError::ChannelClosed)?;
        resp_rx.await.map_err(|_| ActorError::NoResponse)
    }

    /// Get all contract hashes (for anti-entropy)
    pub async fn get_all_hashes(&self) -> Result<Vec<ContentHash>, ActorError> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.tx
            .send(ContractRegistryCommand::GetAllHashes { resp: resp_tx })
            .await
            .map_err(|_| ActorError::ChannelClosed)?;
        resp_rx.await.map_err(|_| ActorError::NoResponse)
    }
}

impl std::fmt::Debug for ContractRegistryHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContractRegistryHandle")
            .field("channel_open", &!self.tx.is_closed())
            .finish()
    }
}
