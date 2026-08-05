//! Secure session management with QUIC/TLS
//!
//! The session manager handles QUIC connections between ICN peers, providing:
//! - Secure transport with TLS 1.3
//! - Multiplexed bidirectional streams
//! - Connection pooling and management
//! - DID-based peer authentication

use anyhow::{Context, Result};
use quinn::{ClientConfig, Endpoint, EndpointConfig, ServerConfig};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

use crate::tls;

/// Interval between TURN refresh checks (30 seconds)
const TURN_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

/// Create transport configuration with DoS protection limits
pub(crate) fn create_transport_config() -> quinn::TransportConfig {
    let mut config = quinn::TransportConfig::default();

    // Limit concurrent streams to prevent resource exhaustion
    // 10 bidirectional streams per connection is sufficient for gossip + RPC
    config.max_concurrent_bidi_streams(10u32.into());

    // Unidirectional streams not currently used, set to minimal
    config.max_concurrent_uni_streams(0u32.into());

    // Set reasonable idle timeout (60 seconds)
    // Connections idle longer than this are closed
    // SAFETY: 60 seconds is a valid VarInt value (< 2^62 ms)
    #[allow(clippy::unwrap_used)]
    config.max_idle_timeout(Some(std::time::Duration::from_secs(60).try_into().unwrap()));

    // Enable keep-alive to detect broken connections (30 seconds)
    config.keep_alive_interval(Some(std::time::Duration::from_secs(30)));

    // Limit stream data to prevent memory exhaustion
    // 1MB per stream is sufficient for gossip messages
    config.stream_receive_window((1024u32 * 1024u32).into());
    config.receive_window((10u32 * 1024u32 * 1024u32).into()); // 10MB total per connection

    config
}

/// Session manager for peer connections
pub struct SessionManager {
    /// QUIC endpoint for listening and dialing
    endpoint: Arc<RwLock<Option<Endpoint>>>,

    /// Active connections by peer DID
    connections: Arc<RwLock<HashMap<String, quinn::Connection>>>,

    /// Discovered public endpoint (if NAT traversal enabled)
    public_endpoint: Arc<RwLock<Option<SocketAddr>>>,

    /// TURN relay client for NAT traversal fallback
    turn_client: Arc<RwLock<Option<crate::TurnClient>>>,

    /// TURN relay address (if allocation is active)
    relay_addr: Arc<RwLock<Option<SocketAddr>>>,

    /// UDP socket for TURN communication (kept alive for refresh)
    turn_socket: Arc<RwLock<Option<UdpSocket>>>,

    /// TURN configuration for re-allocation on failure
    turn_config: Arc<RwLock<Option<crate::TurnConfig>>>,

    /// Shutdown channel sender for stopping background tasks
    shutdown_tx: Option<mpsc::Sender<()>>,

    /// Shutdown channel receiver
    _shutdown_rx: mpsc::Receiver<()>,

    /// Explicit advertised local address override.
    ///
    /// When set, this address is used instead of `endpoint.local_addr()` when building
    /// connection candidates. Required in containerized environments (Docker/K8s) where
    /// the QUIC endpoint binds to `0.0.0.0` but must advertise the container/pod IP.
    advertised_addr: Option<SocketAddr>,

    /// This node's own DID, populated by [`SessionManager::start`].
    ///
    /// `connections` maps *remote* peer DIDs to connections, so the local DID is never a valid
    /// key in it (#2506). Holding the identity here is what lets both insertion paths enforce
    /// that without each caller having to remember to.
    local_did: Arc<RwLock<Option<String>>>,
}

/// What a successful [`SessionManager::dial`] actually did.
///
/// Dial success does not necessarily mean a new connection was created. Asking to dial a peer
/// we already hold a live authenticated session with succeeds — we *are* connected to it — but
/// that session is already wired, and treating it as new work is a defect rather than a
/// no-op: connection setup would run a second time on a connection that has already had it.
///
/// The two cases are returned as distinct variants because the caller must behave differently,
/// and a bare `quinn::Connection` gave it no way to tell them apart. There is deliberately no
/// accessor that yields the connection without choosing a variant: erasing the distinction at
/// the point of use is precisely the bug this type exists to prevent (#2536).
///
/// This says nothing about connection *uniqueness*. Two nodes dialling each other
/// simultaneously, or a second dial that lands during the pre-authentication handshake window,
/// can still produce two physical connections — the session map is populated only by an
/// authenticated Hello (#2530), so before that there is no live session for this check to find.
#[derive(Debug, Clone)]
pub enum DialOutcome {
    /// A new QUIC connection was established, and nothing is reading from it yet.
    ///
    /// The caller receives the sole handle and must install a connection handler; dropping it
    /// closes the connection, because nothing else holds it.
    Established(quinn::Connection),

    /// A live authenticated session already existed for this peer, and this is that
    /// connection.
    ///
    /// It already has a handler loop and has completed its Hello handshake. The connection
    /// opened by this dial attempt has been closed as a duplicate. Wiring this connection
    /// again would spawn a second `handle_connection` loop over it — two loops then race for
    /// each inbound stream, so per-connection handler state becomes ambiguous — and would
    /// write a redundant Hello onto an already-handshaken session.
    ///
    /// This hands the caller a *borrowed* view of that session rather than custody of it. The
    /// connection map entry it was cloned from — and the session's own handler task — still
    /// hold handles, so dropping this one decrements the reference count without closing
    /// anything. There is nothing here for the caller to install, own, or release.
    AlreadyConnected(quinn::Connection),
}

impl SessionManager {
    /// Create a new session manager
    pub fn new() -> Self {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);

        SessionManager {
            endpoint: Arc::new(RwLock::new(None)),
            connections: Arc::new(RwLock::new(HashMap::new())),
            public_endpoint: Arc::new(RwLock::new(None)),
            turn_client: Arc::new(RwLock::new(None)),
            relay_addr: Arc::new(RwLock::new(None)),
            turn_socket: Arc::new(RwLock::new(None)),
            turn_config: Arc::new(RwLock::new(None)),
            shutdown_tx: Some(shutdown_tx),
            _shutdown_rx: shutdown_rx,
            advertised_addr: None,
            local_did: Arc::new(RwLock::new(None)),
        }
    }

    /// Whether `peer_did` is this node's own DID.
    ///
    /// Identity, never address: two nodes legitimately share a source address behind NAT, a
    /// proxy, or a shared pod IP, so an address-based test would partition real deployments.
    pub(crate) async fn is_local_did(&self, peer_did: &str) -> bool {
        self.local_did
            .read()
            .await
            .as_deref()
            .is_some_and(|own| own == peer_did)
    }

    /// This node's own DID, once [`SessionManager::start`] has run.
    pub(crate) async fn local_did(&self) -> Option<String> {
        self.local_did.read().await.clone()
    }

    /// Set an explicit advertised address for connection candidates.
    ///
    /// Use this in containerized environments where the bind address (`0.0.0.0`) is not
    /// a dialable address. When set, this takes precedence over `endpoint.local_addr()`
    /// and the auto-detection fallback.
    pub fn set_advertised_addr(&mut self, addr: SocketAddr) {
        self.advertised_addr = Some(addr);
    }

    /// Start the session manager with a QUIC endpoint
    ///
    /// This creates a QUIC endpoint bound to the given address and starts
    /// listening for incoming connections using TOFU (Trust-On-First-Use) model:
    /// - Server accepts all valid self-signed certificates
    /// - Client accepts all valid self-signed certificates  
    /// - Trust enforcement happens at application layer via PolicyOracle
    ///
    /// If stun_servers is provided, performs NAT traversal discovery to determine
    /// the node's public endpoint (IP and port visible from the internet).
    ///
    /// If turn_config is provided, creates a TURN relay allocation for NAT traversal
    /// fallback when direct connections fail.
    pub async fn start(
        &mut self,
        identity_bundle: &icn_identity::IdentityBundle,
        listen_addr: SocketAddr,
        stun_servers: Option<Vec<SocketAddr>>,
        turn_config: Option<crate::TurnConfig>,
    ) -> Result<()> {
        info!("Session manager starting on {}", listen_addr);

        // Remembered so the connection map can refuse to key on it (#2506).
        *self.local_did.write().await = Some(identity_bundle.did().as_str().to_string());

        // Use TLS certificate from IdentityBundle (already bound to DID)
        // This ensures the cert hash matches what's in BindingInfo
        let certs = vec![identity_bundle.tls_cert().clone()];
        let key = identity_bundle.tls_key();

        // Create server config with TOFU trust model
        // Identity verification happens at application layer (Hello message), not TLS layer
        let server_config = tls::create_server_config(certs.clone(), key.clone_key())?;

        let mut server_config = ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(server_config)?,
        ));

        // Configure transport parameters for DoS protection
        let transport_config = Arc::new(create_transport_config());
        server_config.transport_config(transport_config.clone());

        // Create client config with TOFU mode (trust enforcement at application layer via PolicyOracle)
        info!("TOFU mode enabled - trust enforcement at application layer via PolicyOracle");
        let client_config = tls::create_tofu_client_config(certs.clone(), key.clone_key())?;
        let mut client_config = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(client_config)?,
        ));
        client_config.transport_config(transport_config);

        // Create UDP socket FIRST (before Quinn takes ownership)
        // This allows us to use the same socket for STUN discovery before creating the QUIC endpoint
        let std_socket = std::net::UdpSocket::bind(listen_addr)
            .context("Failed to bind UDP socket for QUIC endpoint")?;
        std_socket
            .set_nonblocking(true)
            .context("Failed to set socket to non-blocking mode")?;

        debug!("UDP socket bound to {}", listen_addr);

        // Convert to tokio socket for async STUN discovery
        let tokio_socket = tokio::net::UdpSocket::from_std(std_socket)
            .context("Failed to convert socket to tokio")?;

        // Perform STUN discovery BEFORE creating the QUIC endpoint
        // This uses the same socket that will later be used for QUIC
        if let Some(servers) = stun_servers {
            debug!("Performing STUN discovery with {} servers", servers.len());
            let stun_client = crate::stun::StunClient::new(servers);

            match stun_client.discover_public_endpoint(&tokio_socket).await {
                Ok(public_addr) => {
                    info!(
                        "✅ Discovered public endpoint: {} (local: {})",
                        public_addr, listen_addr
                    );
                    *self.public_endpoint.write().await = Some(public_addr);
                    icn_obs::metrics::nat::stun_discovery_inc("success");
                }
                Err(e) => {
                    warn!(
                        "Failed to discover public endpoint via STUN: {}. \
                         Node will only be reachable on local network.",
                        e
                    );
                    icn_obs::metrics::nat::stun_discovery_inc("failed");
                }
            }
        }

        // Convert back to std socket for Quinn
        let std_socket = tokio_socket
            .into_std()
            .context("Failed to convert tokio socket back to std")?;

        // Create QUIC endpoint from the existing socket
        // This avoids the double-bind issue since we reuse the already-bound socket
        let runtime = quinn::default_runtime()
            .ok_or_else(|| anyhow::anyhow!("No async runtime found for Quinn"))?;

        let mut endpoint = Endpoint::new(
            EndpointConfig::default(),
            Some(server_config),
            std_socket,
            runtime,
        )
        .context("Failed to create QUIC endpoint")?;

        endpoint.set_default_client_config(client_config);

        info!("QUIC endpoint listening on {}", endpoint.local_addr()?);

        // Initialize TURN relay if configured (NAT traversal fallback)
        if let Some(config) = turn_config {
            info!("TURN relay fallback enabled - server: {}", config.server);
            let turn_client = crate::TurnClient::new(config.clone());

            // Create a UDP socket for TURN communication
            // We bind to 0.0.0.0:0 to get an ephemeral port
            match UdpSocket::bind("0.0.0.0:0").await {
                Ok(turn_socket) => {
                    // Try to create a TURN allocation
                    match turn_client.allocate(&turn_socket).await {
                        Ok(allocation) => {
                            info!(
                                "✅ TURN relay allocated: {} (mapped: {})",
                                allocation.relay_addr, allocation.mapped_addr
                            );
                            *self.relay_addr.write().await = Some(allocation.relay_addr);
                            icn_obs::metrics::nat::turn_allocation_inc();

                            // Store socket for refresh operations
                            *self.turn_socket.write().await = Some(turn_socket);

                            // Store config for re-allocation on failure
                            *self.turn_config.write().await = Some(config);
                        }
                        Err(e) => {
                            warn!(
                                "Failed to create TURN allocation: {}. Relay fallback will not be available.",
                                e
                            );
                            icn_obs::metrics::nat::turn_allocation_failure_inc("allocation_failed");
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to create UDP socket for TURN: {}. Relay fallback will not be available.",
                        e
                    );
                    icn_obs::metrics::nat::turn_allocation_failure_inc("socket_bind_failed");
                }
            }

            *self.turn_client.write().await = Some(turn_client);

            // Spawn TURN refresh background task
            self.spawn_turn_refresh_task();
        }

        // Store endpoint
        *self.endpoint.write().await = Some(endpoint);

        Ok(())
    }

    /// Dial `addr`, expecting to reach `peer_did`.
    ///
    /// A successful dial does **not** mean a connection was created — see [`DialOutcome`].
    ///
    /// The connection it reports is **not** registered as a
    /// peer: `peer_did` is what the caller *expects* to find there, and expectation is not
    /// proof. It comes from operator configuration, from a DID another node advertised over
    /// peer exchange, or from a discovery candidate — none of which is a cryptographic
    /// statement about who will answer. Registration happens in
    /// [`install_incoming_connection`], once Hello has bound a DID to this connection's
    /// certificate (#2520).
    ///
    /// `peer_did` is still load-bearing here: it suppresses duplicate connections to a peer
    /// we already hold a live session with, and it is what the outgoing Hello is addressed
    /// to. It just never becomes an entry in the connection map (#2530).
    ///
    /// **Ownership of the reported connection differs per variant**, and a caller that treats
    /// the two alike is wrong in one of them. `quinn::Connection` is a handle onto a
    /// reference-counted connection, so dropping one closes the QUIC connection only when it
    /// was the *last* handle. Which case applies is precisely what [`DialOutcome`] encodes.
    ///
    /// [`DialOutcome::Established`] transfers sole ownership. Nothing else holds that
    /// connection — registering the dial in the connection map used to keep it alive as a side
    /// effect, and that is exactly the pre-authentication entry this no longer creates — so
    /// dropping it does close the connection. In production `NetworkActor::handle_dial` hands
    /// it to `wire_new_connection`, which passes it to the spawned connection handler — the
    /// task that reads from it — and that task is its owner until Hello registers it or it
    /// closes.
    ///
    /// [`DialOutcome::AlreadyConnected`] transfers no ownership at all. It is a handle cloned
    /// out of the connection map, and the live session owns itself: the map entry it was
    /// cloned from still holds the connection, as does that session's running
    /// `handle_connection` task. Dropping this handle therefore only decrements the reference
    /// count — the existing session is unaffected and stays live. The caller has nothing to
    /// install and nothing to release, so discarding the value is the correct response, which
    /// is what `handle_dial` does. Note that the connection this call actually opened is *not*
    /// the one reported: it was closed as a duplicate before returning.
    pub async fn dial(&self, addr: SocketAddr, peer_did: String) -> Result<DialOutcome> {
        // Never dial ourselves into our own remote-peer map (#2506). Discovery sources echo our
        // own advertisement back to us — over `network:candidates`, peer exchange, or mDNS — and
        // the resulting self-connection carries real signed traffic that trips our own replay
        // guard. Refuse before spending a connection attempt on it.
        if self.is_local_did(&peer_did).await {
            debug!(
                %addr,
                "Refusing to dial our own DID as a remote peer (#2506)"
            );
            anyhow::bail!("refusing to dial the local DID as a remote peer");
        }

        let endpoint = self
            .endpoint
            .read()
            .await
            .as_ref()
            .context("Session manager not started")?
            .clone();

        info!("Dialing peer at {}", addr);

        // Connect to peer
        let connection = endpoint
            .connect(addr, "localhost")?
            .await
            .context("Failed to connect to peer")?;

        info!("Connected to peer at {}", addr);

        // Return the existing connection instead if we already hold a *live, authenticated*
        // session with the peer we expected to reach, so a redundant dial does not displace
        // a working session with a second one.
        //
        // A closed entry must never win over a freshly dialed connection: after a peer restarts,
        // our cached connection to it is dead, and preferring it would make every send fail
        // until this process restarts (#2504). So a closed entry is simply passed over here —
        // the peer's Hello on the new connection replaces it.
        //
        // This only *reads* the map. Nothing is inserted: `peer_did` is the caller's
        // expectation, and a connection becomes a peer when Hello proves an identity for it,
        // not when the transport comes up (#2530).
        let existing_live = {
            let connections = self.connections.read().await;
            match connections.get(&peer_did) {
                Some(existing) if existing.close_reason().is_none() => Some(existing.clone()),
                Some(existing) => {
                    if let Some(reason) = existing.close_reason() {
                        info!(
                            "Existing connection for {} is closed ({}); using the connection we just dialed",
                            peer_did, reason
                        );
                    }
                    None
                }
                None => None,
            }
        };
        if let Some(existing_conn) = existing_live {
            info!(
                "Live session already exists for {}, returning it and closing the connection we just dialed",
                peer_did
            );
            connection.close(0u32.into(), b"duplicate");
            return Ok(DialOutcome::AlreadyConnected(existing_conn));
        }

        Ok(DialOutcome::Established(connection))
    }

    /// Clone the live QUIC endpoint, if the manager has been started.
    ///
    /// `quinn::Endpoint` is internally reference-counted, so this is a cheap handle and
    /// **not** a borrow of the session manager. That matters: it lets a caller wait for
    /// inbound connections without holding a `SessionManager` lock across the await.
    /// [`Self::stop`] needs the write side of that lock, so a long-lived read guard here
    /// would deadlock shutdown — which is what forced the accept loop to poll on a short
    /// timeout in the first place, and hence caused #2521.
    pub async fn endpoint_handle(&self) -> Option<Endpoint> {
        self.endpoint.read().await.clone()
    }

    /// Accept an incoming connection
    ///
    /// Blocks until a new connection arrives or the endpoint is shut down.
    ///
    /// # Cancel safety
    ///
    /// **This future is not cancel-safe, and must never be wrapped in a timeout or raced
    /// in a `select!` against anything that can win.** It fuses two phases with opposite
    /// cancellation properties:
    ///
    /// 1. waiting for a new connection ([`Endpoint::accept`]) — cancel-safe; a pending
    ///    `Incoming` stays queued on the endpoint;
    /// 2. driving that connection's QUIC/TLS handshake (`incoming.await`) — cancel-*unsafe*;
    ///    the `Incoming` has already been consumed into a `Connecting`, and dropping it
    ///    drops the last `ConnectionRef`, which quinn turns into an implicit close.
    ///
    /// Cancelling anywhere in phase 2 therefore destroys a legitimate connection attempt
    /// silently — no error, no application-level trace, and the dialing peer may still
    /// observe a *successful* handshake. See #2521 and
    /// `tests/accept_handshake_cancellation.rs`.
    ///
    /// Callers that need to remain interruptible (the inbound accept loop) must instead
    /// use [`Self::endpoint_handle`], cancel only the `Endpoint::accept()` wait, and drive
    /// the handshake to completion outside any cancellation scope.
    pub async fn accept(&self) -> Result<Option<quinn::Connection>> {
        let endpoint = self
            .endpoint
            .read()
            .await
            .as_ref()
            .context("Session manager not started")?
            .clone();

        // Accept incoming connection
        match endpoint.accept().await {
            Some(incoming) => {
                let connection = incoming.await?;
                info!("Accepted connection from {}", connection.remote_address());
                Ok(Some(connection))
            }
            None => Ok(None),
        }
    }

    /// Get reference to the connections map for direct access
    ///
    /// This is useful when the caller needs to acquire the lock themselves
    /// to avoid holding locks across await points.
    pub fn connections_arc(&self) -> Arc<RwLock<HashMap<String, quinn::Connection>>> {
        self.connections.clone()
    }

    /// Store an incoming connection by peer DID
    ///
    /// This is called when we receive a Hello message from a peer on an incoming
    /// connection, allowing us to send messages back on that connection.
    ///
    /// Note: If a *live* connection already exists for this peer (e.g., from a dial we made),
    /// we don't overwrite it to avoid connection confusion. Both connections will
    /// have handlers running, but we prefer the connection we dialed.
    ///
    /// A **closed** entry is replaced. Map occupancy is not proof of liveness: when a peer
    /// restarts, our cached connection to it is dead, and the peer's reconnect is the only
    /// signal we get. Keeping the dead entry made every subsequent send to that peer fail —
    /// permanently and in one direction only — until this process itself restarted (#2504).
    /// Replacing a connection that is already closed cannot displace a live session, so this
    /// preserves the anti-connection-confusion property above.
    pub async fn store_incoming_connection(&self, peer_did: String, connection: quinn::Connection) {
        let local_did = self.local_did().await;
        install_incoming_connection(
            &self.connections,
            local_did.as_deref(),
            peer_did,
            connection,
        )
        .await;
    }

    /// Get all active connections
    pub async fn connections(&self) -> Vec<(String, quinn::Connection)> {
        self.connections
            .read()
            .await
            .iter()
            .map(|(did, conn)| (did.clone(), conn.clone()))
            .collect()
    }

    /// Disconnect a peer by closing and removing their connection
    ///
    /// This closes the QUIC connection gracefully and removes it from the
    /// connection map. Returns true if a connection was found and closed.
    ///
    /// # Arguments
    ///
    /// * `peer_did` - The DID string of the peer to disconnect
    /// * `reason` - Optional reason message for the closure
    pub async fn disconnect_peer(&self, peer_did: &str, reason: Option<&str>) -> bool {
        let mut connections = self.connections.write().await;
        if let Some(connection) = connections.remove(peer_did) {
            let reason_bytes = reason.unwrap_or("disconnected").as_bytes();
            connection.close(0u32.into(), reason_bytes);
            info!(
                "Disconnected peer {}: {}",
                peer_did,
                reason.unwrap_or("no reason")
            );
            true
        } else {
            false
        }
    }

    /// Get the discovered public endpoint (if NAT traversal was enabled)
    ///
    /// Returns None if NAT traversal is disabled or STUN discovery failed.
    pub async fn public_endpoint(&self) -> Option<SocketAddr> {
        *self.public_endpoint.read().await
    }

    /// Get the TURN relay address (if allocation is active)
    ///
    /// Returns None if TURN is disabled or allocation failed.
    pub async fn relay_addr(&self) -> Option<SocketAddr> {
        *self.relay_addr.read().await
    }

    /// Get the TURN server address (if configured)
    ///
    /// Returns None if TURN was not configured at startup.
    pub async fn turn_server_addr(&self) -> Option<SocketAddr> {
        self.turn_config.read().await.as_ref().map(|c| c.server)
    }

    /// Get a clone of the full TURN config (if configured)
    ///
    /// Used by the relay fallback path to construct a TurnClient with
    /// the same credentials and timeouts as the original allocation.
    pub async fn turn_config(&self) -> Option<crate::TurnConfig> {
        self.turn_config.read().await.clone()
    }

    /// Generate a connection candidate for gossip announcement
    ///
    /// Creates a ConnectionCandidate message that can be published to the
    /// `network:candidates` gossip topic for NAT traversal peer discovery.
    ///
    /// Returns None if the endpoint hasn't been started yet.
    pub async fn connection_candidate(
        &self,
        did: icn_identity::Did,
    ) -> Result<crate::candidate::ConnectionCandidate> {
        let endpoint_guard = self.endpoint.read().await;
        let endpoint = endpoint_guard
            .as_ref()
            .context("Session manager not started")?;

        let raw_local_addr = endpoint.local_addr()?;
        let public_addr = *self.public_endpoint.read().await;

        // Resolve the actual dialable local address.
        //
        // When the endpoint binds to 0.0.0.0 (wildcard), the raw local_addr is not
        // dialable by peers. Resolve in priority order:
        //   1. Explicit `advertised_addr` config override (e.g. K8s pod IP)
        //   2. Auto-detect via routing-socket trick (works in containers/VMs)
        //   3. Raw endpoint address as last resort (may be 0.0.0.0)
        let local_addr = if let Some(addr) = self.advertised_addr {
            addr
        } else if raw_local_addr.ip().is_unspecified() {
            resolve_local_addr(raw_local_addr.port()).unwrap_or(raw_local_addr)
        } else {
            raw_local_addr
        };

        // Include relay address if TURN allocation is active (Phase 4 M1)
        let relay_addr = *self.relay_addr.read().await;

        Ok(crate::candidate::ConnectionCandidate::new(
            did,
            local_addr,
            public_addr,
            relay_addr,
        ))
    }

    /// Stop the session manager
    pub async fn stop(&mut self) -> Result<()> {
        info!("Session manager stopping");

        // Signal shutdown to background tasks
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }

        // Close endpoint
        if let Some(endpoint) = self.endpoint.write().await.take() {
            endpoint.close(0u32.into(), b"shutdown");
            endpoint.wait_idle().await;
        }

        // Clear connections
        self.connections.write().await.clear();

        // Clear TURN state
        *self.turn_socket.write().await = None;
        *self.turn_client.write().await = None;
        *self.relay_addr.write().await = None;

        info!("Session manager stopped");
        Ok(())
    }

    /// Spawn background task to refresh TURN allocation
    ///
    /// This task runs every 30 seconds and checks if the allocation needs refresh.
    /// TURN allocations expire after 10 minutes, so we refresh at the 9 minute mark.
    /// On refresh failure, attempts full re-allocation.
    fn spawn_turn_refresh_task(&self) {
        let turn_client = self.turn_client.clone();
        let turn_socket = self.turn_socket.clone();
        let turn_config = self.turn_config.clone();
        let relay_addr = self.relay_addr.clone();

        // Create a new shutdown receiver for this task
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

        // Store the sender so we can signal shutdown later
        // Note: This replaces the existing shutdown_tx which is fine since
        // the TURN refresh task is the only background task that needs explicit shutdown
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(TURN_REFRESH_INTERVAL);
            interval.tick().await; // Skip immediate first tick

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Check if we have an active allocation that needs refresh
                        let client_guard = turn_client.read().await;
                        let Some(client) = client_guard.as_ref() else {
                            continue;
                        };

                        if !client.needs_refresh().await {
                            continue;
                        }

                        drop(client_guard); // Release read lock before write operations

                        debug!("TURN allocation needs refresh, attempting refresh...");

                        // Get socket for refresh
                        let socket_guard = turn_socket.read().await;
                        if let Some(socket) = socket_guard.as_ref() {
                            let client_guard = turn_client.read().await;
                            if let Some(client) = client_guard.as_ref() {
                                match client.refresh(socket).await {
                                    Ok(()) => {
                                        info!("TURN allocation refreshed successfully");
                                        icn_obs::metrics::nat::turn_permission_refresh_inc();
                                    }
                                    Err(e) => {
                                        warn!("TURN refresh failed: {}. Attempting re-allocation...", e);
                                        icn_obs::metrics::nat::turn_allocation_failure_inc("refresh_failed");

                                        // Drop guards before re-allocation
                                        drop(client_guard);
                                        drop(socket_guard);

                                        // Attempt full re-allocation
                                        Self::reattempt_turn_allocation(
                                            &turn_client,
                                            &turn_socket,
                                            &turn_config,
                                            &relay_addr,
                                        ).await;
                                    }
                                }
                            }
                        } else {
                            debug!("No TURN socket available for refresh");
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("TURN refresh task shutting down");
                        break;
                    }
                }
            }
        });

        // Store the shutdown sender (drop the old one)
        // This is a slight simplification - in production you might want to track
        // all background task shutdown handles separately
        drop(shutdown_tx);
    }

    /// Attempt to re-allocate TURN after refresh failure
    async fn reattempt_turn_allocation(
        turn_client: &Arc<RwLock<Option<crate::TurnClient>>>,
        turn_socket: &Arc<RwLock<Option<UdpSocket>>>,
        turn_config: &Arc<RwLock<Option<crate::TurnConfig>>>,
        relay_addr: &Arc<RwLock<Option<SocketAddr>>>,
    ) {
        // Get config for new client
        let config_guard = turn_config.read().await;
        let Some(config) = config_guard.as_ref().cloned() else {
            warn!("No TURN config available for re-allocation");
            return;
        };
        drop(config_guard);

        // Create new client
        let new_client = crate::TurnClient::new(config);

        // Try to create new socket and allocation
        match UdpSocket::bind("0.0.0.0:0").await {
            Ok(new_socket) => match new_client.allocate(&new_socket).await {
                Ok(allocation) => {
                    info!(
                        "✅ TURN re-allocation successful: {} (mapped: {})",
                        allocation.relay_addr, allocation.mapped_addr
                    );
                    *relay_addr.write().await = Some(allocation.relay_addr);
                    *turn_socket.write().await = Some(new_socket);
                    *turn_client.write().await = Some(new_client);
                    icn_obs::metrics::nat::turn_allocation_inc();
                }
                Err(e) => {
                    warn!(
                        "TURN re-allocation failed: {}. Relay will be unavailable.",
                        e
                    );
                    *relay_addr.write().await = None;
                    icn_obs::metrics::nat::turn_allocation_failure_inc("reallocation_failed");
                }
            },
            Err(e) => {
                warn!("Failed to create socket for TURN re-allocation: {}", e);
                *relay_addr.write().await = None;
                icn_obs::metrics::nat::turn_allocation_failure_inc("socket_bind_failed");
            }
        }
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    #[cfg(test)]
    fn clone_for_test(&self) -> Self {
        SessionManager {
            endpoint: self.endpoint.clone(),
            connections: self.connections.clone(),
            public_endpoint: self.public_endpoint.clone(),
            turn_client: self.turn_client.clone(),
            relay_addr: self.relay_addr.clone(),
            turn_socket: self.turn_socket.clone(),
            turn_config: self.turn_config.clone(),
            shutdown_tx: None, // Test clones don't control shutdown
            _shutdown_rx: mpsc::channel(1).1,
            advertised_addr: self.advertised_addr,
            local_did: self.local_did.clone(),
        }
    }
}

/// Determine the local IP that would be used to route packets to an external host.
///
/// Uses the routing-socket trick: binding a UDP socket and calling `connect()` to a
/// well-known external address makes the OS select the egress interface without
/// sending any packets. The bound socket's local address then reveals the real IP.
///
/// Works correctly in Docker/K8s containers where the bind address is `0.0.0.0`
/// but the container has a real routable IP on eth0. In IPv6-only environments,
/// falls back to the same trick using an IPv6 wildcard and destination.
///
/// Returns None if the trick fails (e.g., no network, weird routing table).
fn resolve_local_addr(port: u16) -> Option<SocketAddr> {
    // Try IPv4 first to preserve existing behavior on dual-stack/IPv4 hosts.
    // Connecting UDP does not send any packets — it only sets the routing destination.
    // 8.8.8.8:80 is used as a well-known routable external IPv4 address.
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(local_addr) = socket.local_addr() {
                return Some(SocketAddr::new(local_addr.ip(), port));
            }
        }
    }

    // Fallback: try the same routing trick using IPv6 for IPv6-only environments.
    // 2001:4860:4860::8888 is Google's well-known routable IPv6 address.
    if let Ok(socket) = std::net::UdpSocket::bind("[::]:0") {
        if socket.connect("[2001:4860:4860::8888]:80").is_ok() {
            if let Ok(local_addr) = socket.local_addr() {
                return Some(SocketAddr::new(local_addr.ip(), port));
            }
        }
    }

    None
}

/// Install an inbound connection for `peer_did`, replacing an entry that is already closed.
///
/// This is the single decision point for inbound connection installation. Both the
/// `SessionManager::store_incoming_connection` helper and the Hello handler (which holds the
/// connection map directly via `connections_arc`) route through here, so the replacement rule
/// cannot drift between the two paths.
///
/// The rule is:
///
/// * a **live** existing connection wins — a duplicate inbound connection never displaces it,
///   which is the anti-connection-confusion property;
/// * a **closed** existing connection is treated as absent and replaced.
///
/// Map occupancy is not proof of liveness. When a peer restarts, our cached connection to it is
/// dead and the peer's reconnect is the only signal we get; keeping the dead entry made every
/// subsequent send to that peer fail — permanently, and in one direction only — until this
/// process itself restarted (#2504). Replacing an already-closed connection cannot displace a
/// live session, so recovery is bought without weakening duplicate suppression.
///
/// Callers must have authenticated `peer_did` first. On the Hello path the DID-TLS binding is
/// verified before this is reached, so a remote cannot claim another node's DID to displace its
/// entry.
/// `local_did` is `None` only before [`SessionManager::start`] has run, when there is no identity
/// to compare against and therefore no connections either; both production callers pass `Some`.
/// It is a required parameter rather than ambient state so that a future insertion path cannot
/// install a connection without first stating which DID is ours (#2506).
///
/// On return, no key other than `peer_did` maps to `connection`: this is the point at which a
/// connection acquires its single canonical identity, so any provisional key that was standing in
/// for it beforehand is dropped here (#2530).
pub(crate) async fn install_incoming_connection(
    connections: &Arc<RwLock<HashMap<String, quinn::Connection>>>,
    local_did: Option<&str>,
    peer_did: String,
    connection: quinn::Connection,
) {
    use std::collections::hash_map::Entry;

    // `connections` is keyed by *remote* peer DID, so our own DID is not a valid key (#2506).
    // Callers must have authenticated `peer_did` before reaching here, so an authenticated DID
    // equal to ours means this connection really is a self-loop, not an impostor claiming our
    // identity — the DID-TLS binding check on the Hello path already rejects the latter.
    if local_did.is_some_and(|own| own == peer_did) {
        warn!(
            remote_addr = %connection.remote_address(),
            "Refusing to register our own DID as a remote peer (#2506)"
        );
        connection.close(0u32.into(), b"self-connection");
        return;
    }

    let mut connections = connections.write().await;

    // One physical connection, one identity. Any *other* key mapping to this same
    // connection is an alias left over from before the peer authenticated — a placeholder
    // from an address-only dial, or a DID that was configured for this address but turned
    // out not to be who answered. Consumers read this map as authenticated peer topology:
    // peer exchange publishes its keys to other nodes, broadcast opens a stream per entry,
    // and `connections_active` counts entries as peers. So an alias is not a duplicate
    // record, it is a phantom peer (#2530).
    //
    // Matching on `stable_id` compares connection *identity*, not address or key, so this
    // can only ever drop an alias of the connection in hand — never a different, newer
    // connection to the same peer. Done under the same write guard as the insert below, so
    // no reader observes both keys.
    let alias_id = connection.stable_id();
    connections.retain(|key, existing| {
        let is_alias = key != &peer_did && existing.stable_id() == alias_id;
        if is_alias {
            info!(
                evicted_key = %key,
                authenticated_did = %peer_did,
                remote_addr = %connection.remote_address(),
                "Evicting provisional connection key superseded by the authenticated DID (#2530)"
            );
        }
        !is_alias
    });

    match connections.entry(peer_did) {
        Entry::Occupied(mut entry) if entry.get().close_reason().is_some() => {
            info!(
                "Replacing closed connection for {} with incoming connection from {}",
                entry.key(),
                connection.remote_address()
            );
            entry.insert(connection);
        }
        Entry::Occupied(entry) => {
            info!(
                "Connection already exists for {}, not overwriting with incoming connection from {}",
                entry.key(),
                connection.remote_address()
            );
        }
        Entry::Vacant(entry) => {
            info!(
                "Storing incoming connection from {} at {}",
                entry.key(),
                connection.remote_address()
            );
            entry.insert(connection);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() {
        // Install default crypto provider for tests
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    }

    #[tokio::test]
    async fn test_session_manager_start() {
        setup();

        let mut manager = SessionManager::new();
        let identity_bundle = icn_identity::IdentityBundle::generate().unwrap();
        let addr = "127.0.0.1:0".parse().unwrap();

        manager
            .start(&identity_bundle, addr, None, None)
            .await
            .unwrap();

        // Endpoint should be initialized
        assert!(manager.endpoint.read().await.is_some());

        manager.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_dial_and_accept() {
        setup();

        // Start server
        let mut server_manager = SessionManager::new();
        let server_identity = icn_identity::IdentityBundle::generate().unwrap();
        let server_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

        server_manager
            .start(&server_identity, server_addr, None, None)
            .await
            .unwrap();

        let server_addr = server_manager
            .endpoint
            .read()
            .await
            .as_ref()
            .unwrap()
            .local_addr()
            .unwrap();

        // Start client
        let mut client_manager = SessionManager::new();
        let client_identity = icn_identity::IdentityBundle::generate().unwrap();
        let client_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

        client_manager
            .start(&client_identity, client_addr, None, None)
            .await
            .unwrap();

        // Spawn accept task
        let server_manager_clone = server_manager.clone_for_test();
        let accept_task = tokio::spawn(async move { server_manager_clone.accept().await });

        // Give server time to start accepting
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Dial from client
        let _client_conn = client_manager
            .dial(server_addr, "server".to_string())
            .await
            .unwrap();

        // Server should accept connection
        let server_conn = accept_task.await.unwrap().unwrap();
        assert!(server_conn.is_some());

        // Cleanup
        client_manager.stop().await.unwrap();
        server_manager.stop().await.unwrap();
    }

    /// Start a manager on an ephemeral loopback port and return it with its bound address.
    async fn start_manager() -> (SessionManager, SocketAddr) {
        let mut manager = SessionManager::new();
        let identity = icn_identity::IdentityBundle::generate().unwrap();
        manager
            .start(&identity, "127.0.0.1:0".parse().unwrap(), None, None)
            .await
            .unwrap();
        let addr = manager
            .endpoint
            .read()
            .await
            .as_ref()
            .unwrap()
            .local_addr()
            .unwrap();
        (manager, addr)
    }

    /// Have `dialer` connect to `listener`, returning the connection `listener` accepted.
    /// Dialer-side connections, kept alive for the lifetime of the test binary.
    ///
    /// `SessionManager::dial` transfers sole ownership on [`DialOutcome::Established`] — the
    /// only outcome this helper accepts — so dropping it closes the QUIC connection and tears
    /// down the listener side these tests are about to assert on. Production keeps it alive by
    /// moving it into the spawned connection handler; there is no such handler here, so the
    /// test binary holds it.
    static DIALER_SIDE: std::sync::Mutex<Vec<quinn::Connection>> =
        std::sync::Mutex::new(Vec::new());

    async fn connect(
        listener: &SessionManager,
        dialer: &SessionManager,
        addr: SocketAddr,
    ) -> quinn::Connection {
        let listener_clone = listener.clone_for_test();
        let accept_task = tokio::spawn(async move { listener_clone.accept().await });
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        let dialer_side = match dialer.dial(addr, "listener".to_string()).await.unwrap() {
            DialOutcome::Established(conn) => conn,
            DialOutcome::AlreadyConnected(_) => {
                panic!("helper dialled a peer it already held a live session with")
            }
        };
        DIALER_SIDE
            .lock()
            .expect("dialer-side lock poisoned")
            .push(dialer_side);
        accept_task.await.unwrap().unwrap().unwrap()
    }

    /// Whether `manager` can actually open a stream on the connection it holds for `peer_did`.
    ///
    /// This is the property that matters operationally: `send_message_to_peer` resolves peers by
    /// DID lookup into this map and then calls `open_bi()`, so a dead entry means every send fails.
    async fn can_reach(manager: &SessionManager, peer_did: &str) -> bool {
        let Some((_, conn)) = manager
            .connections()
            .await
            .into_iter()
            .find(|(did, _)| did == peer_did)
        else {
            return false;
        };
        conn.open_bi().await.is_ok()
    }

    /// Regression test for #2530: installing an authenticated DID collapses any other key
    /// standing for the same physical connection.
    ///
    /// No outbound path registers a connection any more, so nothing in production *should*
    /// be able to create such an alias. This asserts the installer's own invariant directly
    /// rather than through a path that happens to produce one, so the guarantee survives a
    /// future writer that reintroduces a pre-authentication key — which is exactly how the
    /// placeholder got in.
    #[tokio::test]
    async fn test_install_evicts_other_keys_for_the_same_connection() {
        setup();

        let (survivor, survivor_addr) = start_manager().await;
        let (peer, _) = start_manager().await;
        let inbound = connect(&survivor, &peer, survivor_addr).await;

        // A key standing in for the peer before it authenticated: an address-derived
        // placeholder, a configured DID, a DID some other node advertised — the installer
        // does not care which, only that it is not the DID that just authenticated.
        let provisional = "did:icn:zProvisionalStandIn".to_string();
        let authenticated = "did:icn:zAuthenticatedPeer".to_string();
        survivor
            .connections
            .write()
            .await
            .insert(provisional.clone(), inbound.clone());

        install_incoming_connection(
            &survivor.connections,
            survivor.local_did().await.as_deref(),
            authenticated.clone(),
            inbound.clone(),
        )
        .await;

        let keys: Vec<String> = survivor.connections.read().await.keys().cloned().collect();
        assert_eq!(
            keys,
            vec![authenticated],
            "one physical connection must end up under exactly one identity; {provisional} \
             survived alongside it (#2530)"
        );
    }

    /// A key for a *different* connection must survive the alias collapse.
    ///
    /// The eviction matches on connection identity, not on the key, so it must never be
    /// able to unregister a peer that simply happens to be in the map at the time — the
    /// "removal of the wrong connection" hazard.
    #[tokio::test]
    async fn test_install_does_not_evict_keys_for_other_connections() {
        setup();

        let (survivor, survivor_addr) = start_manager().await;
        let (peer_a, _) = start_manager().await;
        let (peer_b, _) = start_manager().await;
        let conn_a = connect(&survivor, &peer_a, survivor_addr).await;
        let conn_b = connect(&survivor, &peer_b, survivor_addr).await;

        let unrelated = "did:icn:zUnrelatedLivePeer".to_string();
        survivor
            .connections
            .write()
            .await
            .insert(unrelated.clone(), conn_a);

        install_incoming_connection(
            &survivor.connections,
            survivor.local_did().await.as_deref(),
            "did:icn:zNewlyAuthenticated".to_string(),
            conn_b,
        )
        .await;

        assert!(
            survivor.connections.read().await.contains_key(&unrelated),
            "installing one connection must not unregister a different, live connection"
        );
    }

    /// Regression test for #2504.
    ///
    /// When a peer restarts, the surviving node holds a dead `quinn::Connection` for that peer's
    /// DID. The restarted peer reconnects with the same DID, but `store_incoming_connection`
    /// treated map occupancy as proof of liveness and discarded the live connection — so every
    /// subsequent send to that peer failed, permanently and one-way, until the survivor itself
    /// restarted.
    #[tokio::test]
    async fn test_reconnect_after_peer_restart_replaces_dead_connection() {
        setup();

        let (mut survivor, survivor_addr) = start_manager().await;
        let peer_did = "did:icn:zRestartingPeer".to_string();

        // --- peer's first incarnation connects and is registered ---
        let (mut peer_v1, _) = start_manager().await;
        let inbound_v1 = connect(&survivor, &peer_v1, survivor_addr).await;
        let v1_probe = inbound_v1.clone();
        survivor
            .store_incoming_connection(peer_did.clone(), inbound_v1)
            .await;
        assert!(
            can_reach(&survivor, &peer_did).await,
            "precondition: survivor should reach the peer's first incarnation"
        );

        // --- the peer restarts: its process goes away ---
        peer_v1.stop().await.unwrap();
        tokio::time::timeout(tokio::time::Duration::from_secs(5), v1_probe.closed())
            .await
            .expect("survivor's connection to the peer should observe closure");
        assert!(
            !can_reach(&survivor, &peer_did).await,
            "precondition: the survivor's cached connection is now dead"
        );

        // --- the peer comes back with the SAME DID and reconnects ---
        let (mut peer_v2, _) = start_manager().await;
        let inbound_v2 = connect(&survivor, &peer_v2, survivor_addr).await;
        survivor
            .store_incoming_connection(peer_did.clone(), inbound_v2)
            .await;

        // The regression: the survivor must be able to send to the peer again, without
        // restarting the survivor and without operator intervention.
        assert!(
            can_reach(&survivor, &peer_did).await,
            "#2504: survivor still holds the dead connection after the peer reconnected, \
             so every send to it will fail until the survivor restarts"
        );

        peer_v2.stop().await.unwrap();
        survivor.stop().await.unwrap();
    }

    /// A *live* connection must still win over a duplicate inbound one.
    ///
    /// This is the anti-connection-confusion property the original code was protecting; the
    /// #2504 fix must not trade it away to make reconnection work.
    #[tokio::test]
    async fn test_live_connection_is_not_replaced_by_duplicate_inbound() {
        setup();

        let (mut survivor, survivor_addr) = start_manager().await;
        let peer_did = "did:icn:zChattyPeer".to_string();

        let (mut peer, _) = start_manager().await;
        let first = connect(&survivor, &peer, survivor_addr).await;
        let first_id = first.stable_id();
        survivor
            .store_incoming_connection(peer_did.clone(), first)
            .await;

        // A second, concurrent inbound connection from the same peer must not evict the first.
        let second = connect(&survivor, &peer, survivor_addr).await;
        survivor
            .store_incoming_connection(peer_did.clone(), second)
            .await;

        let held = survivor
            .connections()
            .await
            .into_iter()
            .find(|(did, _)| *did == peer_did)
            .map(|(_, c)| c.stable_id());
        assert_eq!(
            held,
            Some(first_id),
            "a live connection must not be displaced by a duplicate inbound connection"
        );

        peer.stop().await.unwrap();
        survivor.stop().await.unwrap();
    }

    /// The Hello handler does not call `store_incoming_connection`; it holds the connection map
    /// directly via `connections_arc()`. That is the path a real peer's reconnect takes, so it
    /// gets its own coverage against the same invariant — a fix that only reached the helper
    /// would have left production untouched (#2504).
    #[tokio::test]
    async fn test_hello_path_install_replaces_dead_connection() {
        setup();

        let (mut survivor, survivor_addr) = start_manager().await;
        let peer_did = "did:icn:zHelloPathPeer".to_string();
        let map = survivor.connections_arc();

        let (mut peer_v1, _) = start_manager().await;
        let inbound_v1 = connect(&survivor, &peer_v1, survivor_addr).await;
        let v1_probe = inbound_v1.clone();
        install_incoming_connection(&map, None, peer_did.clone(), inbound_v1).await;
        assert!(can_reach(&survivor, &peer_did).await);

        peer_v1.stop().await.unwrap();
        tokio::time::timeout(tokio::time::Duration::from_secs(5), v1_probe.closed())
            .await
            .expect("cached connection should observe closure");

        let (mut peer_v2, _) = start_manager().await;
        let inbound_v2 = connect(&survivor, &peer_v2, survivor_addr).await;
        install_incoming_connection(&map, None, peer_did.clone(), inbound_v2).await;

        assert!(
            can_reach(&survivor, &peer_did).await,
            "#2504: the Hello-handler install path must also replace a dead cached connection"
        );

        peer_v2.stop().await.unwrap();
        survivor.stop().await.unwrap();
    }

    /// Like `start_manager`, but also returns the identity the manager was started with.
    async fn start_manager_with_identity() -> (SessionManager, SocketAddr, icn_identity::Did) {
        let mut manager = SessionManager::new();
        let identity = icn_identity::IdentityBundle::generate().unwrap();
        let did = identity.did().clone();
        manager
            .start(&identity, "127.0.0.1:0".parse().unwrap(), None, None)
            .await
            .unwrap();
        let addr = manager
            .endpoint
            .read()
            .await
            .as_ref()
            .unwrap()
            .local_addr()
            .unwrap();
        (manager, addr, did)
    }

    /// Regression test for #2506.
    ///
    /// A node must never dial itself into its own remote-connection map. Live, a node received
    /// its own connection candidate back over gossip and dialed both of its own advertised
    /// endpoints; the resulting self-connection carried signed traffic that tripped the node's
    /// own replay guard and ended in it banning its own DID.
    #[tokio::test]
    async fn test_dial_refuses_local_did() {
        setup();

        let (mut manager, _addr, own_did) = start_manager_with_identity().await;
        // A *reachable* target, so the dial cannot fail for transport reasons and the test is
        // actually exercising the identity check. Live, the reachable target is the node's own
        // Service/pod address, which hairpins back to its listener — the dial succeeds and the
        // node registers itself as a remote peer.
        let (mut target, target_addr) = start_manager().await;
        let acceptor = target.clone_for_test();
        let accept_task = tokio::spawn(async move { acceptor.accept().await });
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let result = manager
            .dial(target_addr, own_did.as_str().to_string())
            .await;
        accept_task.abort();

        assert!(
            result.is_err(),
            "#2506: a dial that would register our own DID must be refused before connecting"
        );
        assert!(
            !manager
                .connections()
                .await
                .iter()
                .any(|(did, _)| did == own_did.as_str()),
            "#2506: the local DID must never become a key in the remote connection map"
        );

        target.stop().await.unwrap();
        manager.stop().await.unwrap();
    }

    /// The inbound counterpart: even an *authenticated* connection whose peer DID equals ours
    /// must not be registered as a remote peer. On the Hello path the DID-TLS binding is verified
    /// before installation, so reaching this point with our own DID means the connection really is
    /// a self-loop rather than an impostor.
    #[tokio::test]
    async fn test_store_incoming_connection_refuses_local_did() {
        setup();

        let (mut manager, addr, own_did) = start_manager_with_identity().await;
        let (mut other, _) = start_manager().await;
        let inbound = connect(&manager, &other, addr).await;

        manager
            .store_incoming_connection(own_did.as_str().to_string(), inbound)
            .await;

        assert!(
            !manager
                .connections()
                .await
                .iter()
                .any(|(did, _)| did == own_did.as_str()),
            "#2506: an incoming connection claiming our own DID must not be installed"
        );

        other.stop().await.unwrap();
        manager.stop().await.unwrap();
    }

    /// Negative control for #2506: self-exclusion must be *identity*-based, never address-based.
    ///
    /// Two nodes legitimately share a source address behind NAT, a proxy, or a shared pod IP.
    /// Rejecting a peer because its address looks like ours would partition exactly those
    /// deployments, so a distinct DID arriving from our own address must still be admitted.
    #[tokio::test]
    async fn test_remote_peer_from_local_address_is_still_admitted() {
        setup();

        let (mut manager, addr, _own_did) = start_manager_with_identity().await;
        let (mut other, _) = start_manager().await;

        // `other` dials over loopback, so the connection manager sees a source address in the
        // same host as its own listener — the address-based "is this me?" heuristic would fire.
        let inbound = connect(&manager, &other, addr).await;
        let remote_did = "did:icn:zLegitimatePeerSharingOurAddress".to_string();
        manager
            .store_incoming_connection(remote_did.clone(), inbound)
            .await;

        assert!(
            can_reach(&manager, &remote_did).await,
            "#2506: a distinct DID must still be admitted even when it shares our address"
        );

        other.stop().await.unwrap();
        manager.stop().await.unwrap();
    }
}
