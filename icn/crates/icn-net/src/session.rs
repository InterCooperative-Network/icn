//! Secure session management with QUIC/TLS
//!
//! The session manager handles QUIC connections between ICN peers, providing:
//! - Secure transport with TLS 1.3
//! - Multiplexed bidirectional streams
//! - Connection pooling and management
//! - DID-based peer authentication

use anyhow::{Context, Result};
use icn_identity::KeyPair;
use quinn::{ClientConfig, Endpoint, ServerConfig};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::info;

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
            _shutdown_rx: shutdown_rx,
        }
    }

    /// Start the session manager with a QUIC endpoint
    ///
    /// This creates a QUIC endpoint bound to the given address and starts
    /// listening for incoming connections.
    ///
    /// If trust_graph is provided, enables trust-gated TLS verification where
    /// connections from peers below min_trust_threshold are rejected.
    ///
    /// If stun_servers is provided, performs NAT traversal discovery to determine
    /// the node's public endpoint (IP and port visible from the internet).
    pub async fn start(
        &mut self,
        keypair: &KeyPair,
        listen_addr: SocketAddr,
        trust_graph: Option<Arc<RwLock<icn_trust::TrustGraph>>>,
        min_trust_threshold: Option<f64>,
        stun_servers: Option<Vec<SocketAddr>>,
    ) -> Result<()> {
        info!("Session manager starting on {}", listen_addr);

        let own_did = keypair.did().clone();

        // Generate TLS certificate for this DID
        let (certs, key) = tls::generate_self_signed_cert(keypair)?;

        // Create server config
        let server_config = tls::create_server_config(certs.clone(), key.clone_key())?;
        let mut server_config = ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(server_config)?,
        ));

        // Configure transport parameters for DoS protection
        let transport_config = Arc::new(create_transport_config());
        server_config.transport_config(transport_config.clone());

        // Create client config with trust-gated verification if trust_graph provided
        let client_config = if let Some(trust_graph) = trust_graph {
            info!("Trust-gated TLS verification enabled (min_threshold: {:?})", min_trust_threshold);
            tls::create_client_config(trust_graph, own_did, min_trust_threshold)?
        } else {
            // Fallback: create a permissive client config (accepts all authenticated DIDs)
            info!("TLS verification in development mode (no trust graph)");
            // Create a temporary trust graph for development mode
            let temp_store: Arc<dyn icn_store::Store> = Arc::new(icn_store::SledStore::temporary()?);
            let temp_trust_graph = icn_trust::TrustGraph::new(temp_store, own_did.clone());
            tls::create_client_config(Arc::new(RwLock::new(temp_trust_graph)), own_did, Some(0.0))?
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
        if let Some(servers) = stun_servers {
            info!("NAT traversal enabled - discovering public endpoint via STUN");
            let stun_client = crate::stun::StunClient::new(servers);

            // Get the UDP socket from the QUIC endpoint for STUN queries
            // Note: We use the same socket to ensure the discovered port matches the QUIC port
            let local_addr = endpoint.local_addr()?;
            let socket = tokio::net::UdpSocket::bind(local_addr).await?;

            match stun_client.discover_public_endpoint(&socket).await {
                Ok(public_addr) => {
                    info!("✅ Discovered public endpoint: {} (local: {})", public_addr, local_addr);
                    *self.public_endpoint.write().await = Some(public_addr);
                }
                Err(e) => {
                    // Log warning but don't fail startup - node can still function on local network
                    tracing::warn!("Failed to discover public endpoint via STUN: {}. Node will only be reachable on local network.", e);
                }
            }
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
                info!("Storing incoming connection from {} at {}", entry.key(), connection.remote_address());
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

        // For Phase 2, relay_addr is always None (TURN relay comes in Phase 4)
        let relay_addr = None;

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
        let keypair = KeyPair::generate().unwrap();
        let addr = "127.0.0.1:0".parse().unwrap();

        manager
            .start(&keypair, addr, None, None, None)
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
        let server_keypair = KeyPair::generate().unwrap();
        let server_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

        server_manager
            .start(&server_keypair, server_addr, None, None, None)
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
        let client_keypair = KeyPair::generate().unwrap();
        let client_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

        client_manager
            .start(&client_keypair, client_addr, None, None, None)
            .await
            .unwrap();

        // Spawn accept task
        let server_manager_clone = server_manager.clone_for_test();
        let accept_task = tokio::spawn(async move {
            server_manager_clone.accept().await
        });

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

impl SessionManager {
    #[cfg(test)]
    fn clone_for_test(&self) -> Self {
        SessionManager {
            endpoint: self.endpoint.clone(),
            connections: self.connections.clone(),
            public_endpoint: self.public_endpoint.clone(),
            _shutdown_rx: mpsc::channel(1).1,
        }
    }
}
