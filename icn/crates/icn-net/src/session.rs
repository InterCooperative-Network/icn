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

/// Session manager for peer connections
pub struct SessionManager {
    /// QUIC endpoint for listening and dialing
    endpoint: Arc<RwLock<Option<Endpoint>>>,

    /// Active connections by peer DID
    connections: Arc<RwLock<HashMap<String, quinn::Connection>>>,

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
            _shutdown_rx: shutdown_rx,
        }
    }

    /// Start the session manager with a QUIC endpoint
    ///
    /// This creates a QUIC endpoint bound to the given address and starts
    /// listening for incoming connections.
    pub async fn start(&mut self, keypair: &KeyPair, listen_addr: SocketAddr) -> Result<()> {
        info!("Session manager starting on {}", listen_addr);

        // Generate TLS certificate for this DID
        let (certs, key) = tls::generate_self_signed_cert(keypair)?;

        // Create server config
        let server_config = tls::create_server_config(certs.clone(), key.clone_key())?;
        let mut server_config = ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(server_config)?,
        ));

        // Configure transport parameters
        let mut transport_config = quinn::TransportConfig::default();
        transport_config.max_concurrent_bidi_streams(100u32.into());
        transport_config.max_concurrent_uni_streams(100u32.into());
        server_config.transport_config(Arc::new(transport_config));

        // Create client config
        let client_config = tls::create_client_config()?;
        let client_config = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(client_config)?,
        ));

        // Create endpoint
        let mut endpoint = Endpoint::server(server_config, listen_addr)?;
        endpoint.set_default_client_config(client_config);

        info!("QUIC endpoint listening on {}", endpoint.local_addr()?);

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

        // Store connection
        self.connections
            .write()
            .await
            .insert(peer_did, connection.clone());

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

    /// Get all active connections
    pub async fn connections(&self) -> Vec<(String, quinn::Connection)> {
        self.connections
            .read()
            .await
            .iter()
            .map(|(did, conn)| (did.clone(), conn.clone()))
            .collect()
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

        manager.start(&keypair, addr).await.unwrap();

        // Endpoint should be initialized
        assert!(manager.endpoint.read().await.is_some());

        manager.stop().await.unwrap();
    }

    #[tokio::test]
    #[ignore] // TODO: Fix DID certificate verification for QUIC handshake
    async fn test_dial_and_accept() {
        setup();

        // Start server
        let mut server_manager = SessionManager::new();
        let server_keypair = KeyPair::generate().unwrap();
        let server_addr: SocketAddr = "127.0.0.1:0".parse().unwrap();

        server_manager
            .start(&server_keypair, server_addr)
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
            .start(&client_keypair, client_addr)
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
            _shutdown_rx: mpsc::channel(1).1,
        }
    }
}
