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
    /// Which end of this connection we are.
    ///
    /// The Hello exchange has an initiator and a responder, but both run the same
    /// `handle_hello`, and a Hello *response* is not distinguishable in kind from an initial
    /// Hello — same payload variant, same fields. Without knowing our role we answered every
    /// Hello including responses, so each side answered the other's answer until the rate
    /// limiter denied one (#2532). Role is per *connection*, not per peer: simultaneous
    /// cross-dialling gives two connections on which the same pair holds opposite roles, so
    /// global peer state cannot answer this question.
    pub direction: ConnectionDirection,
    /// Whether this connection has already sent its one Hello response.
    ///
    /// Direction alone bounds the exchange, since an initiator never replies. This
    /// additionally makes the responder's obligation exactly once, so a repeated Hello
    /// cannot restart a response chain.
    pub hello_responded: std::sync::atomic::AtomicBool,
    /// The peer identity proven for *this* connection, once Hello has authenticated one.
    ///
    /// `None` until [`ConnectionContext::record_authenticated_peer`] is called, which
    /// happens only after the three DID-TLS facts in `handle_hello` all hold: the binding
    /// names the claimed DID, that DID's key signed it, and it is a binding of the
    /// certificate *this* connection is actually using (#2520).
    ///
    /// This exists because a `NetworkMessage`'s `from` field cannot answer "who is asking".
    /// It is chosen by the sender and verified by nobody — the same confusion that makes
    /// the pre-authentication rate-limit tier forgeable (#2491). Handlers that disclose
    /// something about this node must consult *this* field, which is bound to the
    /// connection's certificate, and never `message.from`.
    ///
    /// Per connection, not per peer: a peer that opens two connections authenticates on
    /// each one separately, and one connection's Hello says nothing about the other.
    authenticated_peer: RwLock<Option<Did>>,
    /// Whether this node answers peer-exchange requests at all.
    ///
    /// Shared with the actor, so an operator posture set once at startup applies to every
    /// connection without re-plumbing. Distinct from authentication: this says whether
    /// *this node* is participating, not who is asking (#2535).
    peer_exchange_enabled: Arc<std::sync::atomic::AtomicBool>,
}

/// Which end of a QUIC connection this node is.
///
/// Established at the two places a connection handler is spawned — the inbound accept loop
/// and `wire_new_connection` for a dial — and carried for the connection's lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionDirection {
    /// We accepted this connection. We answer the peer's Hello; we never send the first one.
    Inbound,
    /// We dialled this connection. We send the first Hello, and the response we get back
    /// completes our handshake rather than asking us a new question.
    Outbound,
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
        direction: ConnectionDirection,
        peer_exchange_enabled: Arc<std::sync::atomic::AtomicBool>,
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
            direction,
            hello_responded: std::sync::atomic::AtomicBool::new(false),
            authenticated_peer: RwLock::new(None),
            peer_exchange_enabled,
        }
    }

    /// Whether this node participates in peer exchange.
    pub(crate) fn peer_exchange_enabled(&self) -> bool {
        self.peer_exchange_enabled
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Record that `peer` has authenticated on this connection.
    ///
    /// Call this only from the Hello path, and only after the DID-TLS binding has been
    /// verified against this connection's current certificate. Calling it anywhere else
    /// would make an unproven DID indistinguishable from a proven one, which is the whole
    /// property this state exists to carry.
    pub(crate) async fn record_authenticated_peer(&self, peer: &Did) {
        *self.authenticated_peer.write().await = Some(peer.clone());
    }

    /// The peer identity proven for this connection, if any.
    ///
    /// `None` means no Hello has authenticated on this connection *yet*. It is not a
    /// statement about the peer — it is a statement about what this node can currently
    /// prove, and the only safe reading of it is "we do not know who this is".
    pub(crate) async fn authenticated_peer(&self) -> Option<Did> {
        self.authenticated_peer.read().await.clone()
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
