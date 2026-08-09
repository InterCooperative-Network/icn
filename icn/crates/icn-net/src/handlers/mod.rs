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
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::warn;

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
    /// It is chosen by the sender and verified by nobody — the same confusion that once made
    /// the rate-limit tier forgeable, until #2491 made the limiter ask this field too.
    /// Handlers that disclose something about this node, or that spend a resource on its
    /// behalf, must consult *this* field, which is bound to the connection's certificate,
    /// and never `message.from`.
    ///
    /// `None` is a statement about what this node can prove, not about the peer, and its
    /// only safe reading is "we do not know who this is" — which is why the rate limiter
    /// treats it as a phase, not as a peer with a default tier.
    ///
    /// Per connection, not per peer: a peer that opens two connections authenticates on
    /// each one separately, and one connection's Hello says nothing about the other.
    authenticated_peer: RwLock<Option<Did>>,
    /// What this connection may spend while [`Self::authenticated_peer`] is still `None`.
    ///
    /// The other half of the same idea. `authenticated_peer` answers "may this connection
    /// have a trust-derived limit at all"; this is what it gets while the answer is no.
    ///
    /// For an inbound connection this carries two bounds: this connection's own burst (#2491)
    /// and a handle to its source's aggregate allowance (#2549).
    ///
    /// One connection, one budget was the right answer to "which claim selects this limit" —
    /// nothing the sender says picks it — but it made *closing the connection* a way to discard
    /// the spent budget and be issued a new one, so a source's aggregate was bounded by nothing.
    /// The source key outlives the connections that spend from it, which is the whole difference.
    /// The per-connection bucket stays because it answers a different question: it is what stops
    /// a single fresh connection from spending the source's entire allowance in one burst.
    ///
    /// [`crate::rate_limit::PreAuthBudget::Connection`] for outbound dials and handler unit
    /// tests, for the same reason `admission_guard` is `None` there.
    pre_auth_limiter: crate::rate_limit::PreAuthBudget,
    /// This connection's share of its source's pre-authentication allowance (#2547).
    ///
    /// The third member of the same idea. `authenticated_peer` says whether this connection
    /// may have a trust-derived limit; `pre_auth_limiter` is what it spends while the answer
    /// is no; this is what stops one source from holding an unbounded number of the other
    /// two.
    ///
    /// `None` for connections that never took a slot — outbound dials, which this node
    /// chose, and the handler unit tests. Taken (and so released) by
    /// [`Self::record_authenticated_peer`], because a peer that has said who it is no longer
    /// spends *anonymous* admission; otherwise released when the context is dropped with its
    /// handler task. Both are the guard's destructor, so no exit path leaks a slot.
    admission_guard: std::sync::Mutex<Option<crate::preauth_admission::AdmissionGuard>>,
    /// Whether this node answers peer-exchange requests at all.
    ///
    /// Shared with the actor, so an operator posture set once at startup applies to every
    /// connection without re-plumbing. Distinct from authentication: this says whether
    /// *this node* is participating, not who is asking (#2535).
    peer_exchange_enabled: Arc<std::sync::atomic::AtomicBool>,
    /// The DID an operator configured for the endpoint this connection was dialled to.
    ///
    /// `Some` only for a bootstrap entry that named a DID — `icn://did:icn:X@HOST:PORT`.
    /// `None` everywhere else, and the everywhere-else cases are the point:
    ///
    /// - **inbound** — we did not choose this peer, so we expected nobody;
    /// - **address-only bootstrap** — `dial_addr` derives a placeholder `Did` from the
    ///   socket address and dials with it, so the dial argument *looks* like an expectation
    ///   while naming nobody. Reading one off it would report a divergence on every
    ///   `icn://HOST:PORT` entry;
    /// - **a DID learned over peer exchange or discovery** — a claim by another node, whose
    ///   inaccuracy is not this operator's misconfiguration.
    ///
    /// Per connection, and owned by it: an expectation belongs to one dial. Two concurrent
    /// dials to one address can carry different ones, an address can be reused by a
    /// different node, and the expected DID is precisely the value that is wrong in the case
    /// this exists to detect — so neither the address nor the DID is a sound key for holding
    /// this anywhere global (#2533).
    expected_peer: Option<Did>,
    /// Whether a divergence has already been reported for this connection.
    ///
    /// Named for what it records, because the distinction is load-bearing: it is set *only*
    /// when a mismatch is reported, never when an expectation is met. A Hello that matches
    /// returns before touching it, so a connection that first authenticates the configured
    /// DID and later authenticates a different one can still report that — a successful match
    /// must not permanently spend the ability to notice a later rebinding.
    ///
    /// It exists because a Hello is verified every time one arrives, so without it a peer
    /// that repeats its Hello would set the rate of a signal about *local* configuration.
    expectation_mismatch_reported: std::sync::atomic::AtomicBool,
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
        expected_peer: Option<Did>,
        admission_guard: Option<crate::preauth_admission::AdmissionGuard>,
        pre_auth_budget: Option<(
            Arc<crate::rate_limit::SourcePreAuthBudget>,
            crate::preauth_admission::SourceKey,
        )>,
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
            pre_auth_limiter: match pre_auth_budget {
                Some((budget, key)) => crate::rate_limit::PreAuthBudget::Source {
                    connection: crate::rate_limit::PreAuthRateLimiter::new(),
                    budget,
                    key,
                },
                None => crate::rate_limit::PreAuthBudget::Connection(
                    crate::rate_limit::PreAuthRateLimiter::new(),
                ),
            },
            admission_guard: std::sync::Mutex::new(admission_guard),
            peer_exchange_enabled,
            expected_peer,
            expectation_mismatch_reported: std::sync::atomic::AtomicBool::new(false),
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

        // This connection has stopped being anonymous, so it stops charging its source's
        // anonymous allowance (#2547). Releasing here rather than at close is what keeps the
        // bound off legitimate peers: a source is limited in how many connections it may
        // hold *while refusing to identify itself*, not in how many it may hold. Dropping
        // the guard is the release; taking it is idempotent, so the eventual context drop
        // has nothing left to do.
        let released = self
            .admission_guard
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if released.is_some() {
            icn_obs::metrics::network::preauth_admission_released_inc();
        }
    }

    /// The peer identity proven for this connection, if any.
    ///
    /// `None` means no Hello has authenticated on this connection *yet*. It is not a
    /// statement about the peer — it is a statement about what this node can currently
    /// prove, and the only safe reading of it is "we do not know who this is".
    pub(crate) async fn authenticated_peer(&self) -> Option<Did> {
        self.authenticated_peer.read().await.clone()
    }

    /// Spend one message against the anonymous budget this connection draws on.
    ///
    /// Call this only on the branch where [`Self::authenticated_peer`] is `None`. It takes
    /// no identity because at that point there is none — see
    /// [`crate::rate_limit::PreAuthBudget`].
    ///
    /// For an inbound connection the budget is its *source's* and is shared with every other
    /// connection from that source (#2549), so this is charged before the Hello that would
    /// authenticate it is dispatched. Authentication cannot refund it, which is the point:
    /// self-issuing a DID and a binding is cheap, so anything an attacker could buy by
    /// authenticating would be no bound at all.
    pub(crate) async fn check_pre_auth_rate_limit(&self) -> bool {
        self.pre_auth_limiter.check().await
    }

    /// Weigh the operator's expectation for this connection against the identity that just
    /// authenticated on it.
    ///
    /// **Call this only from the Hello path, and only after the DID-TLS binding checks have
    /// passed.** `authenticated` has to be a DID proven against *this* connection's
    /// certificate, never `message.from`, never the address, never a session-map key.
    /// Comparing against a claim would hand the far end control of what this node reports
    /// about its own configuration, and would report a divergence at the one moment the node
    /// cannot say who it is talking to at all. A connection that never authenticates
    /// produces no observation: "we do not know who this is" is not evidence that the
    /// operator was wrong.
    ///
    /// ## What a divergence is, and what it is not
    ///
    /// `icn://did:icn:X@HOST:PORT` states an expectation. It is not a cryptographic pin, and
    /// this does not make it one: the connection is kept and `authenticated` remains the
    /// canonical peer — for peer exchange, for trust, for addressing, for everything. Only
    /// the *silence* changes.
    ///
    /// That is a deliberate policy choice, not an oversight. Refusing the connection would
    /// turn a bootstrap entry left stale by a key rotation, a redeploy, or a copy-paste into
    /// a hard connectivity failure with no in-band way to recover — and it would buy less
    /// than it appears to, because an authenticated peer cannot be impersonating the
    /// configured DID (#2520). A divergence means configuration and reality disagree, which
    /// is the operator's to resolve. So nothing is scored, nobody is banned, and neither DID
    /// is penalised: the peer did nothing wrong by being itself. #2533 records the reasoning;
    /// `crates/icn-net/tests/bootstrap_did_expectation.rs` pins the policy so that changing
    /// it has to be deliberate.
    ///
    /// At most once per connection — see [`Self::expectation_mismatch_reported`].
    pub(crate) fn resolve_peer_expectation(&self, authenticated: &Did, remote_addr: SocketAddr) {
        let Some(expected) = self.expected_peer.as_ref() else {
            return;
        };
        if expected == authenticated {
            return;
        }
        // Only a divergence consumes the guard: a connection whose first Hello matched must
        // still be able to report a later one that does not.
        if self
            .expectation_mismatch_reported
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }

        // `warn!`, not `info!`: this is unusual and the operator is the only one who can act
        // on it. Bounded to once per connection, so a reconnect loop against a misconfigured
        // entry produces one line per connection rather than one per Hello.
        warn!(
            expected_did = %expected,
            authenticated_did = %authenticated,
            remote_addr = %remote_addr,
            "Bootstrap entry named a different DID than the peer that authenticated; keeping \
             the authenticated peer. Update the bootstrap entry if this endpoint's identity \
             changed (#2533)"
        );
        icn_obs::metrics::network::bootstrap_did_expectation_mismatch_inc();
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
