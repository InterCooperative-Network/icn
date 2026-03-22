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
        }
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

        let _own_did = identity_bundle.did().clone();

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

    /// Dial a peer at the given address
    ///
    /// Returns a QUIC connection to the peer.
    pub async fn dial(&self, addr: SocketAddr, peer_did: String) -> Result<quinn::Connection> {
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

        // Store connection, or return existing one if we already have it
        let mut connections = self.connections.write().await;
        if let Some(existing) = connections.get(&peer_did) {
            info!(
                "Connection already exists for {} (from incoming), returning existing connection and closing new one",
                peer_did
            );
            let existing_conn = existing.clone();
            drop(connections); // Release lock before closing
            connection.close(0u32.into(), b"duplicate");
            return Ok(existing_conn);
        }
        connections.insert(peer_did, connection.clone());
        drop(connections);

        Ok(connection)
    }

    /// Accept an incoming connection
    ///
    /// Blocks until a new connection arrives or the endpoint is shut down.
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
    /// Note: If a connection already exists for this peer (e.g., from a dial we made),
    /// we don't overwrite it to avoid connection confusion. Both connections will
    /// have handlers running, but we prefer the connection we dialed.
    pub async fn store_incoming_connection(&self, peer_did: String, connection: quinn::Connection) {
        use std::collections::hash_map::Entry;
        let mut connections = self.connections.write().await;
        match connections.entry(peer_did) {
            Entry::Occupied(entry) => {
                info!("Connection already exists for {}, not overwriting with incoming connection from {}",
                      entry.key(), connection.remote_address());
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
}
