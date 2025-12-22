//! Contract Registry Actor
//!
//! Provides an actor-based interface around the [`ContractRegistry`] with
//! gossip-based synchronization for distributed contract management.
//!
//! ## Features
//!
//! - **Contract Registration**: Deploy contracts with metadata and versioning
//! - **Gossip Sync**: Automatically synchronize contracts across nodes
//! - **Visibility Control**: Private, cooperative-scoped, or public contracts
//! - **Anti-Entropy**: Request missing contracts from peers
//!
//! ## Example
//!
//! ```ignore
//! use icn_ccl::registry_actor::{ContractRegistryActor, DeploymentMetadata};
//! use icn_ccl::ContractRegistry;
//! use std::sync::Arc;
//!
//! let registry = Arc::new(ContractRegistry::new());
//! let actor = ContractRegistryActor::new("did:icn:node1".into(), registry);
//! let handle = actor.spawn();
//!
//! // Register a contract
//! let hash = handle.register_contract(contract, DeploymentMetadata::new("did:icn:alice")).await?;
//!
//! // Get a contract by hash
//! let contract = handle.get_contract(&hash).await?;
//! ```

mod command;
mod handle;
pub mod types;

pub use handle::ContractRegistryHandle;
pub use types::*;

use crate::ast::Contract;
use crate::registry::{ContentHash, ContractRegistry, RegistryError};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

use command::ContractRegistryCommand;

/// Actor managing contract registry with gossip synchronization
pub struct ContractRegistryActor {
    /// This node's DID
    own_did: String,

    /// The underlying contract registry
    registry: Arc<ContractRegistry>,

    /// Callback for sending gossip messages
    send_callback: Option<SendCallback>,

    /// Callback for emitting registry events
    event_callback: Option<EventCallback>,

    /// Pending governance approvals (hash -> proposal_id)
    pending_approvals: Arc<RwLock<HashMap<ContentHash, String>>>,

    /// Revoked contracts (tracked in memory, persisted via metadata)
    revoked_contracts: Arc<RwLock<HashMap<ContentHash, String>>>,
}

impl ContractRegistryActor {
    /// Create a new ContractRegistryActor
    ///
    /// # Arguments
    /// * `own_did` - This node's DID for gossip identification
    /// * `registry` - The underlying contract registry to wrap
    pub fn new(own_did: String, registry: Arc<ContractRegistry>) -> Self {
        Self {
            own_did,
            registry,
            send_callback: None,
            event_callback: None,
            pending_approvals: Arc::new(RwLock::new(HashMap::new())),
            revoked_contracts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set the callback for sending gossip messages
    pub fn set_send_callback(&mut self, cb: SendCallback) {
        self.send_callback = Some(cb);
    }

    /// Set the callback for emitting registry events
    pub fn set_event_callback(&mut self, cb: EventCallback) {
        self.event_callback = Some(cb);
    }

    /// Spawn the actor and return a handle for interacting with it
    ///
    /// The actor runs in a background Tokio task until the handle is dropped.
    pub fn spawn(self) -> ContractRegistryHandle {
        let (tx, rx) = mpsc::channel::<ContractRegistryCommand>(256);
        let did = self.own_did.clone();

        tokio::spawn(self.run(rx));

        info!(did = %did, "Contract registry actor spawned");
        ContractRegistryHandle::new(tx)
    }

    /// Main actor loop
    async fn run(self, mut rx: mpsc::Receiver<ContractRegistryCommand>) {
        while let Some(cmd) = rx.recv().await {
            debug!(?cmd, "Processing command");
            self.handle_command(cmd).await;
        }
        info!("Contract registry actor stopped");
    }

    /// Handle a single command
    async fn handle_command(&self, cmd: ContractRegistryCommand) {
        match cmd {
            ContractRegistryCommand::Register {
                contract,
                metadata,
                resp,
            } => {
                let result = self.handle_register(contract, metadata).await;
                let _ = resp.send(result);
            }
            ContractRegistryCommand::Get { hash, resp } => {
                let result = self.registry.get(&hash).await.ok().flatten();
                let _ = resp.send(result);
            }
            ContractRegistryCommand::GetMetadata { hash, resp } => {
                let result = self.registry.get_metadata(&hash).await.ok().flatten();
                let _ = resp.send(result);
            }
            ContractRegistryCommand::List { filter, resp } => {
                let result = self.handle_list(filter).await;
                let _ = resp.send(result);
            }
            ContractRegistryCommand::Resolve { name, version, resp } => {
                let result = self.registry.resolve(&name, version).await.ok().flatten();
                let _ = resp.send(result);
            }
            ContractRegistryCommand::Revoke {
                hash,
                requester,
                reason,
                resp,
            } => {
                let result = self.handle_revoke(&hash, &requester, reason).await;
                let _ = resp.send(result);
            }
            ContractRegistryCommand::GossipMessage(msg) => {
                if let Err(e) = self.handle_gossip_message(msg).await {
                    warn!("Error handling gossip message: {}", e);
                }
            }
            ContractRegistryCommand::CheckApproval { hash, resp } => {
                let status = self.check_approval_status(&hash).await;
                let _ = resp.send(status);
            }
            ContractRegistryCommand::Stats { resp } => {
                let stats = self.registry.stats().await;
                let _ = resp.send(stats);
            }
            ContractRegistryCommand::Exists { hash, resp } => {
                let exists = self.registry.exists(&hash).await;
                let _ = resp.send(exists);
            }
            ContractRegistryCommand::GetAllHashes { resp } => {
                let hashes = self.get_all_hashes().await;
                let _ = resp.send(hashes);
            }
        }
    }

    /// Handle contract registration
    async fn handle_register(
        &self,
        contract: Contract,
        metadata: DeploymentMetadata,
    ) -> Result<ContentHash, RegistryError> {
        // Validate contract first
        contract
            .validate()
            .map_err(|e| RegistryError::InvalidContract(e.to_string()))?;

        // Deploy to registry
        let hash = self
            .registry
            .deploy(contract.clone(), &metadata.owner, metadata.version)
            .await?;

        // Get the created metadata for gossip
        let contract_metadata = self.registry.get_metadata(&hash).await?.ok_or_else(|| {
            RegistryError::StorageError("Metadata not found after deploy".to_string())
        })?;

        // Broadcast to gossip
        if let Some(ref send_cb) = self.send_callback {
            let msg = ContractRegistryMessage::Deployed {
                hash,
                contract: contract.clone(),
                metadata: contract_metadata,
            };
            send_cb(msg);
        }

        // Emit event
        if let Some(ref event_cb) = self.event_callback {
            event_cb(ContractRegistryEvent::Deployed {
                hash,
                name: contract.name.clone(),
            });
        }

        info!(
            hash = %hex::encode(hash),
            name = %contract.name,
            owner = %metadata.owner,
            "Contract registered via actor"
        );

        Ok(hash)
    }

    /// Handle contract listing with filter
    async fn handle_list(&self, filter: ContractFilter) -> Vec<ContractSummary> {
        let all_metadata = match self.registry.list_all().await {
            Ok(m) => m,
            Err(_) => return vec![],
        };

        let revoked = self.revoked_contracts.read().await;

        let mut results: Vec<ContractSummary> = all_metadata
            .iter()
            .filter(|meta| {
                // Filter by owner
                if let Some(ref owner) = filter.owner {
                    if &meta.owner != owner {
                        return false;
                    }
                }
                // Filter by name prefix
                if let Some(ref prefix) = filter.name_prefix {
                    if !meta.name.starts_with(prefix) {
                        return false;
                    }
                }
                true
            })
            .map(|meta| {
                let mut summary = ContractSummary::from(meta);
                summary.revoked = revoked.contains_key(&meta.code_hash);
                summary
            })
            .collect();

        // Apply limit
        if let Some(limit) = filter.limit {
            results.truncate(limit);
        }

        results
    }

    /// Handle contract revocation
    async fn handle_revoke(
        &self,
        hash: &ContentHash,
        requester: &str,
        reason: String,
    ) -> Result<(), RegistryError> {
        // Get metadata to check ownership
        let metadata = self
            .registry
            .get_metadata(hash)
            .await?
            .ok_or_else(|| RegistryError::NotFound(hex::encode(hash)))?;

        // Verify requester is owner
        if metadata.owner != requester {
            return Err(RegistryError::AuthorizationError(format!(
                "Only owner {} can revoke contract",
                metadata.owner
            )));
        }

        // Mark as revoked
        {
            let mut revoked = self.revoked_contracts.write().await;
            revoked.insert(*hash, reason.clone());
        }

        // Broadcast revocation
        if let Some(ref send_cb) = self.send_callback {
            let msg = ContractRegistryMessage::Revoked {
                hash: *hash,
                owner: requester.to_string(),
                reason: reason.clone(),
                revoked_at: icn_time::current_timestamp_millis(),
            };
            send_cb(msg);
        }

        // Emit event
        if let Some(ref event_cb) = self.event_callback {
            event_cb(ContractRegistryEvent::Revoked {
                hash: *hash,
                reason: reason.clone(),
            });
        }

        info!(
            hash = %hex::encode(hash),
            owner = %requester,
            reason = %reason,
            "Contract revoked"
        );

        Ok(())
    }

    /// Handle incoming gossip message
    async fn handle_gossip_message(&self, msg: ContractRegistryMessage) -> Result<(), String> {
        match msg {
            ContractRegistryMessage::Deployed {
                hash,
                contract,
                metadata,
            } => {
                // Check if we already have this contract
                if self.registry.exists(&hash).await {
                    debug!(hash = %hex::encode(hash), "Contract already exists, skipping");
                    return Ok(());
                }

                // Validate and deploy locally
                if let Err(e) = contract.validate() {
                    warn!(hash = %hex::encode(hash), error = %e, "Invalid contract from gossip");
                    return Err(format!("Invalid contract: {e}"));
                }

                match self
                    .registry
                    .deploy(contract.clone(), &metadata.owner, Some(metadata.version))
                    .await
                {
                    Ok(_) => {
                        info!(
                            hash = %hex::encode(hash),
                            name = %contract.name,
                            "Contract synced from gossip"
                        );

                        // Emit event
                        if let Some(ref event_cb) = self.event_callback {
                            event_cb(ContractRegistryEvent::SyncedFromPeer {
                                hash,
                                peer: metadata.owner.clone(),
                            });
                        }
                    }
                    Err(RegistryError::AlreadyExists(_)) => {
                        // Race condition, contract was added by another path
                        debug!(hash = %hex::encode(hash), "Contract already exists (race)");
                    }
                    Err(e) => {
                        warn!(hash = %hex::encode(hash), error = %e, "Failed to deploy contract from gossip");
                        return Err(format!("Deploy failed: {e}"));
                    }
                }
            }

            ContractRegistryMessage::Revoked {
                hash,
                owner,
                reason,
                ..
            } => {
                // Verify the revocation is from the owner
                if let Ok(Some(metadata)) = self.registry.get_metadata(&hash).await {
                    if metadata.owner == owner {
                        let mut revoked = self.revoked_contracts.write().await;
                        revoked.insert(hash, reason.clone());
                        info!(hash = %hex::encode(hash), "Contract revocation synced from gossip");
                    } else {
                        warn!(
                            hash = %hex::encode(hash),
                            claimed_owner = %owner,
                            actual_owner = %metadata.owner,
                            "Invalid revocation: owner mismatch"
                        );
                    }
                }
            }

            ContractRegistryMessage::Request { hash, requester } => {
                // Respond with contract if we have it
                if let (Ok(contract), Ok(metadata)) = (
                    self.registry.get(&hash).await,
                    self.registry.get_metadata(&hash).await,
                ) {
                    if let Some(ref send_cb) = self.send_callback {
                        let msg = ContractRegistryMessage::Response {
                            hash,
                            contract,
                            metadata,
                        };
                        send_cb(msg);
                        debug!(
                            hash = %hex::encode(hash),
                            requester = %requester,
                            "Sent contract response"
                        );
                    }
                }
            }

            ContractRegistryMessage::Response {
                hash,
                contract,
                metadata,
            } => {
                // Store the contract if we don't have it
                if let (Some(contract), Some(metadata)) = (contract, metadata) {
                    if !self.registry.exists(&hash).await {
                        if let Err(e) = self
                            .registry
                            .deploy(contract.clone(), &metadata.owner, Some(metadata.version))
                            .await
                        {
                            if !matches!(e, RegistryError::AlreadyExists(_)) {
                                warn!(hash = %hex::encode(hash), error = %e, "Failed to store contract response");
                            }
                        } else {
                            info!(hash = %hex::encode(hash), "Stored contract from response");
                        }
                    }
                }
            }

            ContractRegistryMessage::Digest { hashes, sender } => {
                // Find contracts we don't have and request them
                for hash in hashes {
                    if !self.registry.exists(&hash).await {
                        if let Some(ref send_cb) = self.send_callback {
                            let msg = ContractRegistryMessage::Request {
                                hash,
                                requester: self.own_did.clone(),
                            };
                            send_cb(msg);
                            debug!(
                                hash = %hex::encode(hash),
                                from = %sender,
                                "Requesting missing contract"
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Check approval status for a contract
    async fn check_approval_status(&self, hash: &ContentHash) -> ApprovalStatus {
        let pending = self.pending_approvals.read().await;
        if let Some(proposal_id) = pending.get(hash) {
            ApprovalStatus::Pending {
                proposal_id: proposal_id.clone(),
            }
        } else {
            ApprovalStatus::NotRequired
        }
    }

    /// Get all contract hashes for anti-entropy
    async fn get_all_hashes(&self) -> Vec<ContentHash> {
        match self.registry.list_all().await {
            Ok(metadata) => metadata.iter().map(|m| m.code_hash).collect(),
            Err(_) => vec![],
        }
    }

    /// Send a digest of all contracts to peers (for anti-entropy)
    pub async fn broadcast_digest(&self) {
        if let Some(ref send_cb) = self.send_callback {
            let hashes = self.get_all_hashes().await;
            if !hashes.is_empty() {
                let msg = ContractRegistryMessage::Digest {
                    hashes,
                    sender: self.own_did.clone(),
                };
                send_cb(msg);
                debug!("Broadcast contract digest");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Expr, Rule, Stmt, Value};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn create_test_contract(name: &str) -> Contract {
        Contract::new(name.to_string())
            .add_participant(icn_identity::KeyPair::generate().unwrap().did().clone())
            .add_rule(Rule::new("test_rule".to_string()).add_stmt(Stmt::Return {
                value: Expr::Literal(Value::String("ok".to_string())),
            }))
    }

    #[tokio::test]
    async fn test_actor_register_and_get() {
        let registry = Arc::new(ContractRegistry::new());
        let actor = ContractRegistryActor::new("did:icn:test".into(), registry);
        let handle = actor.spawn();

        let contract = create_test_contract("TestContract");
        let metadata = DeploymentMetadata::new("did:icn:alice");

        // Register
        let hash = handle
            .register_contract(contract.clone(), metadata)
            .await
            .unwrap();

        // Get
        let retrieved = handle.get_contract(&hash).await.unwrap().unwrap();
        assert_eq!(retrieved.name, "TestContract");

        // Get metadata
        let meta = handle.get_metadata(&hash).await.unwrap().unwrap();
        assert_eq!(meta.owner, "did:icn:alice");
        assert_eq!(meta.version, 1);
    }

    #[tokio::test]
    async fn test_actor_list_with_filter() {
        let registry = Arc::new(ContractRegistry::new());
        let actor = ContractRegistryActor::new("did:icn:test".into(), registry);
        let handle = actor.spawn();

        // Register contracts from different owners
        handle
            .register_contract(
                create_test_contract("AliceContract"),
                DeploymentMetadata::new("did:icn:alice"),
            )
            .await
            .unwrap();

        handle
            .register_contract(
                create_test_contract("BobContract"),
                DeploymentMetadata::new("did:icn:bob"),
            )
            .await
            .unwrap();

        // List all
        let all = handle.list_contracts(ContractFilter::new()).await.unwrap();
        assert_eq!(all.len(), 2);

        // Filter by owner
        let alice_only = handle
            .list_contracts(ContractFilter::new().with_owner("did:icn:alice"))
            .await
            .unwrap();
        assert_eq!(alice_only.len(), 1);
        assert_eq!(alice_only[0].name, "AliceContract");
    }

    #[tokio::test]
    async fn test_actor_revoke() {
        let registry = Arc::new(ContractRegistry::new());
        let actor = ContractRegistryActor::new("did:icn:test".into(), registry);
        let handle = actor.spawn();

        let contract = create_test_contract("RevocableContract");
        let hash = handle
            .register_contract(contract, DeploymentMetadata::new("did:icn:alice"))
            .await
            .unwrap();

        // Owner can revoke
        handle
            .revoke_contract(&hash, "did:icn:alice", "No longer needed".into())
            .await
            .unwrap();

        // Contract is marked as revoked in list
        let list = handle.list_contracts(ContractFilter::new()).await.unwrap();
        assert!(list[0].revoked);
    }

    #[tokio::test]
    async fn test_actor_revoke_non_owner_fails() {
        let registry = Arc::new(ContractRegistry::new());
        let actor = ContractRegistryActor::new("did:icn:test".into(), registry);
        let handle = actor.spawn();

        let contract = create_test_contract("OwnerOnlyContract");
        let hash = handle
            .register_contract(contract, DeploymentMetadata::new("did:icn:alice"))
            .await
            .unwrap();

        // Non-owner cannot revoke
        let result = handle
            .revoke_contract(&hash, "did:icn:bob", "Trying to revoke".into())
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_actor_resolve() {
        let registry = Arc::new(ContractRegistry::new());
        let actor = ContractRegistryActor::new("did:icn:test".into(), registry);
        let handle = actor.spawn();

        let contract = create_test_contract("NamedContract");
        let hash = handle
            .register_contract(contract, DeploymentMetadata::new("did:icn:alice"))
            .await
            .unwrap();

        // Resolve by name
        let resolved = handle.resolve("NamedContract", None).await.unwrap().unwrap();
        assert_eq!(resolved, hash);

        // Non-existent name
        let not_found = handle.resolve("DoesNotExist", None).await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_actor_event_callback() {
        let registry = Arc::new(ContractRegistry::new());
        let mut actor = ContractRegistryActor::new("did:icn:test".into(), registry);

        let event_count = Arc::new(AtomicUsize::new(0));
        let count_clone = event_count.clone();

        actor.set_event_callback(Arc::new(move |_event| {
            count_clone.fetch_add(1, Ordering::SeqCst);
        }));

        let handle = actor.spawn();

        // Register should emit event
        let contract = create_test_contract("EventContract");
        handle
            .register_contract(contract, DeploymentMetadata::new("did:icn:alice"))
            .await
            .unwrap();

        // Give time for event processing
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        assert!(event_count.load(Ordering::SeqCst) >= 1);
    }

    #[tokio::test]
    async fn test_actor_stats() {
        let registry = Arc::new(ContractRegistry::new());
        let actor = ContractRegistryActor::new("did:icn:test".into(), registry);
        let handle = actor.spawn();

        // Initial stats
        let stats = handle.stats().await.unwrap();
        assert_eq!(stats.total_contracts, 0);

        // Register a contract
        handle
            .register_contract(
                create_test_contract("StatsContract"),
                DeploymentMetadata::new("did:icn:alice"),
            )
            .await
            .unwrap();

        // Updated stats
        let stats = handle.stats().await.unwrap();
        assert_eq!(stats.total_contracts, 1);
        assert_eq!(stats.unique_owners, 1);
    }
}
