//! Message handlers for incoming network connections
//!
//! This module extracts the message handling logic from the main actor loop
//! into separate, testable handler functions organized by protocol.

pub mod encrypted;
pub mod handshake;
pub mod hello;
pub mod onion;
pub mod peer_exchange;
pub mod signed;

use crate::actor::PeerConnectionInfo;
use crate::protocol::{write_message, NetworkMessage};
use crate::replay_guard::ReplayGuard;
use crate::topology::{NeighborSets, TopologyConfig};
use crate::{BlobLocationRegistry, RateLimiter, SessionManager};
use icn_identity::{Did, IdentityBundle};
use icn_security::MisbehaviorDetector;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Context for handling incoming connections
///
/// This struct holds all the shared state needed by message handlers,
/// avoiding the need to pass many parameters through the call chain.
#[allow(dead_code)] // API fields that may be used in future handlers
pub struct ConnectionContext {
    pub handler: crate::IncomingMessageHandler,
    pub rate_limiter: Arc<RateLimiter>,
    pub replay_guard: Arc<RwLock<ReplayGuard>>,
    pub neighbor_sets: Option<Arc<RwLock<NeighborSets>>>,
    pub topology_config: Option<TopologyConfig>,
    pub session_manager: Arc<RwLock<SessionManager>>,
    pub peer_connections: Arc<RwLock<std::collections::HashMap<Did, PeerConnectionInfo>>>,
    pub blob_registry: Option<Arc<RwLock<BlobLocationRegistry>>>,
    pub misbehavior_detector: Option<Arc<RwLock<MisbehaviorDetector>>>,
    pub identity_bundle: IdentityBundle,
    pub own_did: Did,
}

impl ConnectionContext {
    /// Create a new connection context from handler parameters
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        handler: crate::IncomingMessageHandler,
        rate_limiter: Arc<RateLimiter>,
        replay_guard: Arc<RwLock<ReplayGuard>>,
        neighbor_sets: Option<Arc<RwLock<NeighborSets>>>,
        topology_config: Option<TopologyConfig>,
        session_manager: Arc<RwLock<SessionManager>>,
        peer_connections: Arc<RwLock<std::collections::HashMap<Did, PeerConnectionInfo>>>,
        blob_registry: Option<Arc<RwLock<BlobLocationRegistry>>>,
        misbehavior_detector: Option<Arc<RwLock<MisbehaviorDetector>>>,
        identity_bundle: IdentityBundle,
        own_did: Did,
    ) -> Self {
        Self {
            handler,
            rate_limiter,
            replay_guard,
            neighbor_sets,
            topology_config,
            session_manager,
            peer_connections,
            blob_registry,
            misbehavior_detector,
            identity_bundle,
            own_did,
        }
    }

    /// Forward message to external handler
    pub fn forward_to_handler(&self, message: NetworkMessage) {
        (self.handler)(message);
    }

    /// Send a message on a connection
    #[allow(dead_code)] // API method that may be used in future handlers
    pub async fn send_response(
        connection: &quinn::Connection,
        message: &NetworkMessage,
    ) -> anyhow::Result<()> {
        let (mut send, _recv) = connection.open_bi().await?;
        write_message(&mut send, message).await?;
        send.finish()?;
        Ok(())
    }
}
