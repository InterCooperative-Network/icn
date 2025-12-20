//! Secure session management with QUIC/TLS
//!
//! The session manager handles QUIC connections between ICN peers, providing:
//! - Secure transport with TLS 1.3
//! - Multiplexed bidirectional streams
//! - Connection pooling and management
//! - DID-based peer authentication

use anyhow::{Context, Result};
use quinn::{ClientConfig, Endpoint, ServerConfig};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{info, warn};

use crate::tls;

/// Create transport configuration with DoS protection limits
fn create_transport_config() -> quinn::TransportConfig {
    let mut config = quinn::TransportConfig::default();

    // Limit concurrent streams to prevent resource exhaustion
    // 10 bidirectional streams per connection is sufficient for gossip + RPC
    config.max_concurrent_bidi_streams(10u32.into());

    // Unidirectional streams not currently used, set to minimal
    config.max_concurrent_uni_streams(0u32.into());

    // Set reasonable idle timeout (60 seconds)
    // Connections idle longer than this are closed
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

    /// Shutdown channel receiver
    _shutdown_rx: mpsc::Receiver<()>,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new() -> Self {
        let (_shutdown_tx, shutdown_rx) = mpsc::channel(1);

        SessionManager {
            endpoint: Arc::new(RwLock::new(None)),
            connections: Arc::new(RwLock::new(HashMap::new())),
            public_endpoint: Arc::new(RwLock::new(None)),
            turn_client: Arc::new(RwLock::new(None)),
            relay_addr: Arc::new(RwLock::new(None)),
            _shutdown_rx: shutdown_rx,
        }
    }

    /// Start the session manager with a QUIC endpoint
    ///
    /// This creates a QUIC endpoint bound to the given address and starts
    /// listening for incoming connections using TOFU (Trust-On-First-Use) model:
    /// - Server accepts all valid self-signed certificates
    /// - Client accepts all valid self-signed certificates  
    /// - Trust enforcement happens at application layer (Hello message handler)
    ///
    /// If trust_graph is provided, it is used for application-layer trust decisions.
    /// The min_trust_threshold parameter is currently ignored (always uses 0.0 at TLS layer).
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
        trust_graph: Option<Arc<RwLock<icn_trust::TrustGraph>>>,
        min_trust_threshold: Option<f64>,
        stun_servers: Option<Vec<SocketAddr>>,
        turn_config: Option<crate::TurnConfig>,
    ) -> Result<()> {
        info!("Session manager starting on {}", listen_addr);

        let own_did = identity_bundle.did().clone();

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

        // Create client config with TOFU mode (trust enforcement at application layer)
        // Always use threshold 0.0 at TLS layer to allow initial connections
        // Trust-based access control happens in Hello message handler
        let client_config = if let Some(trust_graph) = trust_graph {
            info!(
                "TOFU mode enabled - trust enforcement at application layer (requested threshold: {:?})",
                min_trust_threshold
            );
            tls::create_client_config(
                certs.clone(),
                key.clone_key(),
                trust_graph,
                own_did,
                Some(0.0), // Always use TOFU mode at TLS layer
            )?
        } else {
            // Fallback: create a permissive client config for development mode
            // Note: In production, trust_graph should always be provided
            warn!("TLS verification in development mode (no trust graph) - not suitable for production");
            // Create a temporary trust graph for development mode
            let temp_store: Arc<dyn icn_store::Store> =
                Arc::new(icn_store::SledStore::temporary()?);
            let temp_trust_graph = icn_trust::TrustGraph::new(temp_store, own_did.clone());
            // Development mode uses 0.0 threshold; production should use trust_graph with proper threshold
            tls::create_client_config(
                certs.clone(),
                key.clone_key(),
                Arc::new(RwLock::new(temp_trust_graph)),
                own_did,
                Some(0.0),
            )?
        };
        let mut client_config = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(client_config)?,
        ));
        client_config.transport_config(transport_config);

        // Create endpoint
        let mut endpoint = Endpoint::server(server_config, listen_addr)?;
        endpoint.set_default_client_config(client_config);

        info!("QUIC endpoint listening on {}", endpoint.local_addr()?);

        // Perform STUN discovery if enabled (NAT traversal)
        // TEMPORARY FIX: Disabled due to double-bind bug
        // TODO: Fix by either reusing endpoint's socket or binding before endpoint creation
        if let Some(_servers) = stun_servers {
            tracing::warn!("STUN discovery temporarily disabled due to socket reuse issue");
            tracing::warn!("Node will only be reachable on local network until fixed");
            // let stun_client = crate::stun::StunClient::new(servers);
            // 
            // // BUG: This tries to bind to the same address as the QUIC endpoint above
            // // causing "Address already in use" error
            // let local_addr = endpoint.local_addr()?;
            // let socket = tokio::net::UdpSocket::bind(local_addr).await?;
            // 
            // match stun_client.discover_public_endpoint(&socket).await {
            //     Ok(public_addr) => {
            //         info!(
            //             "✅ Discovered public endpoint: {} (local: {})",
            //             public_addr, local_addr
            //         );
            //         *self.public_endpoint.write().await = Some(public_addr);
            //     }
            //     Err(e) => {
            //         tracing::warn!("Failed to discover public endpoint via STUN: {}. Node will only be reachable on local network.", e);
            //     }
            // }
        }

        // Initialize TURN relay if configured (NAT traversal fallback)
        if let Some(config) = turn_config {
            info!("TURN relay fallback enabled - server: {}", config.server);
            let turn_client = crate::TurnClient::new(config.clone());

            // Create a UDP socket for TURN communication
            // We bind to 0.0.0.0:0 to get an ephemeral port
            match tokio::net::UdpSocket::bind("0.0.0.0:0").await {
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

        let local_addr = endpoint.local_addr()?;
        let public_addr = *self.public_endpoint.read().await;

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

        // Close endpoint
        if let Some(endpoint) = self.endpoint.write().await.take() {
            endpoint.close(0u32.into(), b"shutdown");
            endpoint.wait_idle().await;
        }

        // Clear connections
        self.connections.write().await.clear();

        info!("Session manager stopped");
        Ok(())
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
            _shutdown_rx: mpsc::channel(1).1,
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
            .start(&identity_bundle, addr, None, None, None, None)
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
            .start(&server_identity, server_addr, None, None, None, None)
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
            .start(&client_identity, client_addr, None, None, None, None)
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
