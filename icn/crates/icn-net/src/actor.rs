//! Network actor - coordinates discovery and session management
//!
//! The network actor provides a unified interface for:
//! - Peer discovery via mDNS
//! - QUIC session management
//! - Automatic connection establishment to discovered peers
//! - Connection lifecycle management

use anyhow::{Context, Result};
use icn_identity::{Did, KeyPair};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{debug, info, warn};

use crate::{Discovery, PeerInfo, SessionManager};

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
    session_manager: SessionManager,
    stats: Arc<RwLock<NetworkStats>>,
    rx: mpsc::Receiver<NetworkMsg>,
}

impl NetworkActor {
    /// Start the network actor
    ///
    /// Initializes discovery and session management on the given address.
    pub async fn spawn(
        keypair: &KeyPair,
        listen_addr: SocketAddr,
        shutdown_tx: tokio::sync::broadcast::Sender<()>,
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

        // Create actor
        let actor = NetworkActor {
            discovery,
            session_manager,
            stats: stats.clone(),
            rx,
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
        self.session_manager.stop().await?;
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
                    .dial(addr, did.as_str().to_string())
                    .await
                    .map(|_| {
                        // Increment connection counter
                        let stats = self.stats.clone();
                        tokio::spawn(async move {
                            stats.write().await.connections_total += 1;
                        });
                    });

                let _ = response.send(result);
            }

            NetworkMsg::GetStats(tx) => {
                // Calculate stats on-demand
                let peers = self.discovery.peers().await;
                let connections = self.session_manager.connections().await;
                let total = self.stats.read().await.connections_total;

                let stats = NetworkStats {
                    peers_discovered: peers.len(),
                    connections_active: connections.len(),
                    connections_total: total,
                };

                let _ = tx.send(stats);
            }
        }
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

        let handle = NetworkActor::spawn(&keypair, addr, shutdown_tx.clone())
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
