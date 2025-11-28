//! Peer discovery via mDNS and rendezvous
//!
//! The discovery service uses mDNS (multicast DNS) to find peers on the local network.
//! Each ICN node announces itself with:
//! - Service type: `_icn._udp.local`
//! - Instance name: DID
//! - TXT records: version, capabilities, etc.

use anyhow::{Context, Result};
use icn_identity::Did;
use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

const SERVICE_TYPE: &str = "_icn._udp.local.";
const SCAN_INTERVAL: Duration = Duration::from_secs(30);

/// Discovered peer information
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub did: Did,
    pub addr: SocketAddr,
    pub version: String,
}

/// Discovery service for finding peers
pub struct Discovery {
    /// mDNS service daemon
    daemon: Option<ServiceDaemon>,

    /// Own DID for announcing
    own_did: Option<Did>,

    /// Discovered peers
    peers: Arc<RwLock<HashMap<String, PeerInfo>>>,

    /// Shutdown channel
    shutdown_tx: mpsc::Sender<()>,
    #[allow(dead_code)]
    shutdown_rx: mpsc::Receiver<()>,
}

impl Discovery {
    /// Create a new discovery service
    pub fn new() -> Self {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);

        Discovery {
            daemon: None,
            own_did: None,
            peers: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx,
            shutdown_rx,
        }
    }

    /// Start the discovery service
    ///
    /// Announces this node on the local network and starts scanning for peers.
    pub async fn start(&mut self, did: Did, addr: SocketAddr) -> Result<()> {
        info!("Discovery service starting for DID: {}", did);

        // Create mDNS daemon
        let daemon = ServiceDaemon::new().context("Failed to create mDNS daemon")?;

        // Register this node's service
        let instance_name = did.as_str().to_string();
        let mut properties = HashMap::new();
        properties.insert("version".to_string(), "0.1.0".to_string());
        properties.insert("did".to_string(), did.as_str().to_string());

        let service_info = ServiceInfo::new(
            SERVICE_TYPE,
            &instance_name,
            &format!("{}.local.", hostname()),
            addr.ip().to_string(),
            addr.port(),
            Some(properties),
        )
        .context("Failed to create service info")?;

        daemon
            .register(service_info)
            .context("Failed to register mDNS service")?;

        info!("mDNS service registered: {}", instance_name);

        self.daemon = Some(daemon.clone());
        self.own_did = Some(did);

        // Spawn background task to browse for peers
        let peers = self.peers.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        tokio::spawn(async move {
            let browse_result = daemon.browse(SERVICE_TYPE);

            if let Err(e) = browse_result {
                warn!("Failed to start mDNS browse: {}", e);
                return;
            }

            let receiver = browse_result.unwrap();

            info!("Started browsing for ICN peers on mDNS");

            loop {
                tokio::select! {
                    event = receiver.recv_async() => {
                        match event {
                            Ok(event) => {
                                Self::handle_mdns_event(event, &peers).await;
                            }
                            Err(e) => {
                                warn!("mDNS receiver error: {}", e);
                                break;
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("Discovery service shutting down");
                        break;
                    }
                    _ = tokio::time::sleep(SCAN_INTERVAL) => {
                        debug!("Discovery scan interval ({}s)", SCAN_INTERVAL.as_secs());
                    }
                }
            }
        });

        info!("Discovery service started");
        Ok(())
    }

    /// Handle an mDNS service event
    async fn handle_mdns_event(
        event: mdns_sd::ServiceEvent,
        peers: &Arc<RwLock<HashMap<String, PeerInfo>>>,
    ) {
        use mdns_sd::ServiceEvent;

        match event {
            ServiceEvent::ServiceResolved(info) => {
                // Extract DID from TXT properties
                if let Some(did_str) = info.get_property_val_str("did") {
                    // Parse socket address
                    let addr = match info.get_addresses().iter().next() {
                        Some(ip) => SocketAddr::new(*ip, info.get_port()),
                        None => {
                            debug!("No address for peer {}", did_str);
                            return;
                        }
                    };

                    // Parse DID
                    let did = match parse_did(&did_str) {
                        Ok(did) => did,
                        Err(e) => {
                            warn!("Invalid DID {}: {}", did_str, e);
                            return;
                        }
                    };

                    let version = info
                        .get_property_val_str("version")
                        .unwrap_or("unknown")
                        .to_string();

                    let peer_info = PeerInfo {
                        did: did.clone(),
                        addr,
                        version,
                    };

                    info!("Discovered peer: {} at {}", did.as_str(), addr);

                    peers
                        .write()
                        .await
                        .insert(did.as_str().to_string(), peer_info);
                }
            }
            ServiceEvent::ServiceRemoved(_type, instance) => {
                info!("Peer removed: {}", instance);
                peers.write().await.remove(&instance);
            }
            ServiceEvent::SearchStarted(service_type) => {
                debug!("Search started for {}", service_type);
            }
            ServiceEvent::SearchStopped(service_type) => {
                debug!("Search stopped for {}", service_type);
            }
            _ => {}
        }
    }

    /// Get all discovered peers
    pub async fn peers(&self) -> Vec<PeerInfo> {
        self.peers.read().await.values().cloned().collect()
    }

    /// Get a specific peer by DID
    pub async fn get_peer(&self, did: &Did) -> Option<PeerInfo> {
        self.peers.read().await.get(did.as_str()).cloned()
    }

    /// Stop the discovery service
    pub async fn stop(&mut self) -> Result<()> {
        info!("Discovery service stopping");

        // Signal shutdown
        let _ = self.shutdown_tx.send(()).await;

        // Unregister service
        if let Some(daemon) = self.daemon.take() {
            daemon.shutdown().context("Failed to shutdown mDNS daemon")?;
        }

        // Clear peers
        self.peers.write().await.clear();

        info!("Discovery service stopped");
        Ok(())
    }
}

impl Default for Discovery {
    fn default() -> Self {
        Self::new()
    }
}

/// Get the hostname for mDNS announcements
fn hostname() -> String {
    hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "icn-node".to_string())
}

/// Parse a DID from a string
fn parse_did(s: &str) -> Result<Did> {
    // Use validated parsing from icn-identity
    Did::from_str(s)
}

// Add broadcast extension for shutdown signal
trait BroadcastExt {
    fn subscribe(&self) -> mpsc::Receiver<()>;
}

impl BroadcastExt for mpsc::Sender<()> {
    fn subscribe(&self) -> mpsc::Receiver<()> {
        // Create a new receiver pair
        let (tx, rx) = mpsc::channel(1);
        // Clone self to send shutdown signal when original fires
        let self_clone = self.clone();
        tokio::spawn(async move {
            // Wait for any message (shutdown signal)
            let _ = self_clone.closed().await;
            let _ = tx.send(()).await;
        });
        rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    #[tokio::test]
    async fn test_discovery_start_stop() {
        let mut discovery = Discovery::new();
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();
        let addr: SocketAddr = "127.0.0.1:4433".parse().unwrap();

        discovery.start(did, addr).await.unwrap();

        // Should have daemon
        assert!(discovery.daemon.is_some());

        discovery.stop().await.unwrap();

        // Daemon should be cleared
        assert!(discovery.daemon.is_none());
    }

    #[test]
    fn test_parse_did() {
        // Generate a valid DID from a real keypair
        let kp = KeyPair::generate().unwrap();
        let did_str = kp.did().as_str();

        let did = parse_did(did_str).unwrap();
        assert_eq!(did.as_str(), did_str);
    }

    #[test]
    fn test_parse_did_invalid() {
        let result = parse_did("invalid");
        assert!(result.is_err());
    }
}
