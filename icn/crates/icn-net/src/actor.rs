//! Network actor - coordinates discovery and session management
//!
//! The network actor provides a unified interface for:
//! - Peer discovery via mDNS
//! - QUIC session management
//! - Automatic connection establishment to discovered peers
//! - Connection lifecycle management

use anyhow::{Context, Result};
use icn_identity::{Did, KeyPair};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{info, warn};

use crate::{protocol::{NetworkMessage, read_message, write_message}, Discovery, PeerInfo, SessionManager};

/// Callback for handling incoming network messages
pub type IncomingMessageHandler = Arc<dyn Fn(NetworkMessage) + Send + Sync>;

/// Messages that can be sent to the network actor
#[derive(Debug)]
pub enum NetworkMsg {
    /// Get all discovered peers
    GetPeers(oneshot::Sender<Vec<PeerInfo>>),

    /// Dial a specific peer
    Dial {
        addr: SocketAddr,
        did: Did,
        response: oneshot::Sender<Result<()>>,
    },

    /// Send a network message to a specific peer
    SendMessage {
        did: Did,
        message: NetworkMessage,
        response: oneshot::Sender<Result<()>>,
    },

    /// Broadcast a message to all connected peers
    Broadcast {
        message: NetworkMessage,
        response: oneshot::Sender<Result<()>>,
    },

    /// Get connection statistics
    GetStats(oneshot::Sender<NetworkStats>),
}

/// Network statistics
#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub peers_discovered: usize,
    pub connections_active: usize,
    pub connections_total: u64,
}

/// Handle to interact with the network actor
#[derive(Clone)]
pub struct NetworkHandle {
    tx: mpsc::Sender<NetworkMsg>,
}

impl NetworkHandle {
    /// Get all discovered peers
    pub async fn get_peers(&self) -> Result<Vec<PeerInfo>> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(NetworkMsg::GetPeers(tx))
            .await
            .context("Network actor closed")?;
        rx.await.context("Response channel closed")
    }

    /// Dial a peer
    pub async fn dial(&self, addr: SocketAddr, did: Did) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(NetworkMsg::Dial {
                addr,
                did,
                response: tx,
            })
            .await
            .context("Network actor closed")?;
        rx.await.context("Response channel closed")?
    }

    /// Send a network message to a specific peer
    pub async fn send_message(&self, did: Did, message: NetworkMessage) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(NetworkMsg::SendMessage {
                did,
                message,
                response: tx,
            })
            .await
            .context("Network actor closed")?;
        rx.await.context("Response channel closed")?
    }

    /// Broadcast a message to all connected peers
    pub async fn broadcast(&self, message: NetworkMessage) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(NetworkMsg::Broadcast {
                message,
                response: tx,
            })
            .await
            .context("Network actor closed")?;
        rx.await.context("Response channel closed")?
    }

    /// Get network statistics
    pub async fn get_stats(&self) -> Result<NetworkStats> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(NetworkMsg::GetStats(tx))
            .await
            .context("Network actor closed")?;
        rx.await.context("Response channel closed")
    }
}

/// Network actor state
pub struct NetworkActor {
    discovery: Discovery,
    session_manager: Arc<RwLock<SessionManager>>,
    stats: Arc<RwLock<NetworkStats>>,
    rx: mpsc::Receiver<NetworkMsg>,
    incoming_handler: Option<IncomingMessageHandler>,
}

impl NetworkActor {
    /// Start the network actor
    ///
    /// Initializes discovery and session management on the given address.
    pub async fn spawn(
        keypair: &KeyPair,
        listen_addr: SocketAddr,
        shutdown_tx: tokio::sync::broadcast::Sender<()>,
        incoming_handler: Option<IncomingMessageHandler>,
    ) -> Result<NetworkHandle> {
        let did = keypair.did().clone();

        info!("Network actor starting for DID: {}", did);

        // Start discovery
        let mut discovery = Discovery::new();
        discovery
            .start(did.clone(), listen_addr)
            .await
            .context("Failed to start discovery")?;

        // Start session manager
        let mut session_manager = SessionManager::new();
        session_manager
            .start(keypair, listen_addr)
            .await
            .context("Failed to start session manager")?;

        // Create stats
        let stats = Arc::new(RwLock::new(NetworkStats {
            peers_discovered: 0,
            connections_active: 0,
            connections_total: 0,
        }));

        // Create channel
        let (tx, rx) = mpsc::channel(32);

        // Wrap session_manager in Arc for sharing between tasks
        let session_manager = Arc::new(RwLock::new(session_manager));

        // Spawn incoming connection handler if handler is provided
        if let Some(handler) = incoming_handler.clone() {
            let session_manager_clone = session_manager.clone();
            let shutdown_rx = shutdown_tx.subscribe();
            tokio::spawn(async move {
                if let Err(e) = Self::handle_incoming_connections(
                    session_manager_clone,
                    handler,
                    shutdown_rx,
                )
                .await
                {
                    warn!("Incoming connection handler error: {}", e);
                }
            });
        }

        // Create actor
        let actor = NetworkActor {
            discovery,
            session_manager,
            stats: stats.clone(),
            rx,
            incoming_handler,
        };

        // Spawn actor task
        let stats_clone = stats.clone();
        tokio::spawn(async move {
            if let Err(e) = actor.run(shutdown_tx, stats_clone).await {
                warn!("Network actor error: {}", e);
            }
        });

        Ok(NetworkHandle { tx })
    }

    /// Run the network actor event loop
    async fn run(
        mut self,
        shutdown_tx: tokio::sync::broadcast::Sender<()>,
        _stats: Arc<RwLock<NetworkStats>>,
    ) -> Result<()> {
        info!("Network actor running");

        let mut shutdown_rx = shutdown_tx.subscribe();

        loop {
            tokio::select! {
                msg = self.rx.recv() => {
                    match msg {
                        Some(msg) => self.handle_message(msg).await,
                        None => {
                            info!("Network actor channel closed");
                            break;
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Network actor received shutdown signal");
                    break;
                }
            }
        }

        // Shutdown services
        self.session_manager.write().await.stop().await?;
        self.discovery.stop().await?;

        info!("Network actor stopped");
        Ok(())
    }

    /// Handle a single message
    async fn handle_message(&mut self, msg: NetworkMsg) {
        match msg {
            NetworkMsg::GetPeers(tx) => {
                let peers = self.discovery.peers().await;
                let _ = tx.send(peers);
            }

            NetworkMsg::Dial { addr, did, response } => {
                let result = self
                    .session_manager
                    .read()
                    .await
                    .dial(addr, did.as_str().to_string())
                    .await
                    .map(|_| {
                        // Increment connection counter
                        let stats = self.stats.clone();
                        tokio::spawn(async move {
                            stats.write().await.connections_total += 1;
                        });

                        // Track metrics
                        icn_obs::metrics::network::connections_total_inc();
                    });

                let _ = response.send(result);
            }

            NetworkMsg::SendMessage {
                did,
                message,
                response,
            } => {
                let result = self.send_message_to_peer(&did, message).await;
                let _ = response.send(result);
            }

            NetworkMsg::Broadcast { message, response } => {
                let result = self.broadcast_message(message).await;
                let _ = response.send(result);
            }

            NetworkMsg::GetStats(tx) => {
                // Calculate stats on-demand
                let peers = self.discovery.peers().await;
                let connections = self.session_manager.read().await.connections().await;
                let total = self.stats.read().await.connections_total;

                let stats = NetworkStats {
                    peers_discovered: peers.len(),
                    connections_active: connections.len(),
                    connections_total: total,
                };

                // Update gauge metrics
                icn_obs::metrics::network::peers_discovered_set(stats.peers_discovered as u64);
                icn_obs::metrics::network::connections_active_set(stats.connections_active as u64);

                let _ = tx.send(stats);
            }
        }
    }

    /// Send a message to a specific peer
    async fn send_message_to_peer(&self, did: &Did, message: NetworkMessage) -> Result<()> {
        // Get connection for this peer
        let connections = self.session_manager.read().await.connections().await;
        let connection = connections
            .iter()
            .find(|(peer_did, _)| peer_did == did.as_str())
            .map(|(_, conn)| conn.clone())
            .context("No connection to peer")?;

        // Open a new stream and send the message
        let (mut send, _recv) = connection
            .open_bi()
            .await
            .context("Failed to open stream")?;

        write_message(&mut send, &message)
            .await
            .context("Failed to write message")?;

        send.finish().context("Failed to finish stream")?;

        // Track metrics
        icn_obs::metrics::network::messages_sent_inc();

        Ok(())
    }

    /// Broadcast a message to all connected peers
    async fn broadcast_message(&self, message: NetworkMessage) -> Result<()> {
        let connections = self.session_manager.read().await.connections().await;

        // Send to all connected peers
        let mut sent_count = 0;
        for (_did, connection) in connections {
            // Open a new stream and send the message
            if let Ok((mut send, _recv)) = connection.open_bi().await {
                if write_message(&mut send, &message).await.is_ok() {
                    let _ = send.finish();
                    sent_count += 1;
                }
            }
        }

        // Track metrics (one increment per successful send)
        for _ in 0..sent_count {
            icn_obs::metrics::network::messages_sent_inc();
        }

        Ok(())
    }

    /// Handle incoming QUIC connections
    async fn handle_incoming_connections(
        session_manager: Arc<RwLock<SessionManager>>,
        handler: IncomingMessageHandler,
        mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<()> {
        info!("Starting incoming connection handler");

        loop {
            // Check for shutdown signal first (non-blocking)
            match shutdown_rx.try_recv() {
                Ok(_) | Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                    info!("Incoming connection handler received shutdown signal");
                    break;
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) |
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {
                    // Continue to accept connections
                }
            }

            // Accept with timeout to periodically check shutdown
            let conn_result = {
                let guard = session_manager.read().await;
                match tokio::time::timeout(tokio::time::Duration::from_millis(100), guard.accept()).await {
                    Ok(result) => Some(result),
                    Err(_) => None, // Timeout
                }
            };

            if let Some(conn_result) = conn_result {
                match conn_result {
                    Ok(Some(connection)) => {
                        // Spawn handler for this connection
                        let handler_clone = handler.clone();
                        tokio::spawn(async move {
                            if let Err(e) = Self::handle_connection(connection, handler_clone).await {
                                warn!("Connection handler error: {}", e);
                            }
                        });
                    }
                    Ok(None) => {
                        info!("Session manager shut down");
                        break;
                    }
                    Err(e) => {
                        warn!("Failed to accept connection: {}", e);
                    }
                }
            }
        }

        info!("Incoming connection handler stopped");
        Ok(())
    }

    /// Handle a single QUIC connection (process all incoming streams)
    async fn handle_connection(
        connection: quinn::Connection,
        handler: IncomingMessageHandler,
    ) -> Result<()> {
        info!("Handling connection from {}", connection.remote_address());

        loop {
            // Accept incoming bidirectional stream
            match connection.accept_bi().await {
                Ok((mut send, mut recv)) => {
                    // Read network message
                    match read_message(&mut recv).await {
                        Ok(message) => {
                            info!("Received message from {}", message.from);

                            // Track metrics
                            icn_obs::metrics::network::messages_received_inc();

                            // Call the handler
                            handler(message);
                        }
                        Err(e) => {
                            warn!("Failed to read message: {}", e);
                        }
                    }

                    // Close the stream
                    let _ = send.finish();
                }
                Err(quinn::ConnectionError::ApplicationClosed(_)) => {
                    info!("Connection closed by peer");
                    break;
                }
                Err(e) => {
                    warn!("Error accepting stream: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires network interfaces
    async fn test_network_actor_start() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        let keypair = KeyPair::generate().unwrap();
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(1);

        let handle = NetworkActor::spawn(&keypair, addr, shutdown_tx.clone(), None)
            .await
            .unwrap();

        // Should be able to get stats
        let stats = handle.get_stats().await.unwrap();
        assert_eq!(stats.peers_discovered, 0);
        assert_eq!(stats.connections_active, 0);

        // Shutdown
        let _ = shutdown_tx.send(());
    }
}
