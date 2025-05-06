use crate::federation::network::{FederationNetwork, NetworkError};
use libp2p::Multiaddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, interval};
use tokio::task::JoinHandle;
use std::str::FromStr;
use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};
use tracing::{info, warn, error, debug};
use dotenv::dotenv;

/// Bootstrap node configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapNode {
    /// Multi-address of the bootstrap node
    pub addr: String,
    
    /// Friendly name of the node
    pub name: String,
    
    /// Federation ID this node belongs to
    pub federation_id: String,
    
    /// Optional node DID for verification
    pub node_did: Option<String>,
    
    /// Optional fingerprint for verification
    pub fingerprint: Option<String>,
}

/// Peer discovery service
pub struct PeerDiscovery {
    /// Federation network
    network: Arc<RwLock<FederationNetwork>>,
    
    /// Bootstrap nodes
    bootstrap_nodes: Vec<BootstrapNode>,
    
    /// Task handle
    task: Option<JoinHandle<()>>,
    
    /// Running flag
    running: bool,
}

impl PeerDiscovery {
    /// Create a new peer discovery service
    pub fn new(network: Arc<RwLock<FederationNetwork>>) -> Self {
        // Load bootstrap nodes from environment or config file
        let bootstrap_nodes = Self::load_bootstrap_nodes();
        
        Self {
            network,
            bootstrap_nodes,
            task: None,
            running: false,
        }
    }
    
    /// Load bootstrap nodes from environment or config file
    fn load_bootstrap_nodes() -> Vec<BootstrapNode> {
        // Try to load from environment variable
        let mut nodes = Vec::new();
        
        // First check if there's a config file path specified
        let config_path = std::env::var("ICN_BOOTSTRAP_CONFIG")
            .unwrap_or_else(|_| "config/bootstrap_nodes.toml".to_string());
            
        if Path::new(&config_path).exists() {
            // Try to load from config file
            match fs::read_to_string(&config_path) {
                Ok(content) => {
                    match toml::from_str::<Vec<BootstrapNode>>(&content) {
                        Ok(file_nodes) => {
                            info!("Loaded {} bootstrap nodes from {}", file_nodes.len(), config_path);
                            nodes.extend(file_nodes);
                        },
                        Err(e) => {
                            error!("Failed to parse bootstrap nodes from {}: {}", config_path, e);
                        }
                    }
                },
                Err(e) => {
                    error!("Failed to read bootstrap config file {}: {}", config_path, e);
                }
            }
        }
        
        // Check environment variables for additional nodes
        if let Ok(env_nodes) = std::env::var("ICN_BOOTSTRAP_NODES") {
            for node_str in env_nodes.split(',') {
                let parts: Vec<&str> = node_str.trim().split(';').collect();
                if parts.len() >= 1 {
                    let addr = parts[0].to_string();
                    let name = parts.get(1).map(|s| s.to_string()).unwrap_or_else(|| "Unknown".to_string());
                    let federation_id = parts.get(2).map(|s| s.to_string()).unwrap_or_else(|| "default".to_string());
                    let node_did = parts.get(3).map(|s| s.to_string());
                    let fingerprint = parts.get(4).map(|s| s.to_string());
                    
                    nodes.push(BootstrapNode {
                        addr,
                        name,
                        federation_id,
                        node_did,
                        fingerprint,
                    });
                }
            }
        }
        
        // If no nodes are configured, use fallback nodes only in development mode
        if nodes.is_empty() && cfg!(debug_assertions) {
            warn!("No bootstrap nodes configured, using development fallbacks");
            nodes.push(BootstrapNode {
                addr: "/ip4/127.0.0.1/tcp/4001/p2p/QmYyQSo1c1Ym7orWxLYvCrM2EmxFTANf8wXmmE7DWjhx5N".to_string(),
                name: "Local Dev Node".to_string(),
                federation_id: "dev".to_string(),
                node_did: None,
                fingerprint: None,
            });
        }
        
        nodes
    }
    
    /// Get the list of bootstrap nodes
    pub fn get_bootstrap_nodes(&self) -> &[BootstrapNode] {
        &self.bootstrap_nodes
    }
    
    /// Start the discovery service
    pub async fn start(&mut self) -> Result<(), NetworkError> {
        if self.running {
            return Ok(());
        }
        
        self.running = true;
        
        let network = self.network.clone();
        let bootstrap_nodes = self.bootstrap_nodes.clone();
        
        if bootstrap_nodes.is_empty() {
            warn!("No bootstrap nodes configured for peer discovery");
        } else {
            info!("Starting peer discovery with {} bootstrap nodes", bootstrap_nodes.len());
        }
        
        // Spawn the discovery task
        let task = tokio::spawn(async move {
            // Connect to bootstrap nodes initially
            for node in &bootstrap_nodes {
                debug!("Connecting to bootstrap node: {} ({})", node.name, node.addr);
                if let Ok(addr) = node.addr.parse::<Multiaddr>() {
                    match network.write().await.connect(addr).await {
                        Ok(_) => {
                            info!("Connected to bootstrap node: {}", node.name);
                        },
                        Err(e) => {
                            warn!("Failed to connect to bootstrap node {}: {:?}", node.name, e);
                        }
                    }
                } else {
                    error!("Invalid multiaddress for bootstrap node {}: {}", node.name, node.addr);
                }
            }
            
            // Periodically try to discover new peers
            let mut interval = interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                
                // Reconnect to any disconnected bootstrap nodes
                for node in &bootstrap_nodes {
                    if let Ok(addr) = node.addr.parse::<Multiaddr>() {
                        let net = network.read().await;
                        let connected_peers = net.get_connected_peers().await;
                        
                        // Check if the bootstrap node's peer ID is in the connected peers
                        let peer_id = match addr.iter().find_map(|p| {
                            if let libp2p::multiaddr::Protocol::P2p(peer_id) = p {
                                Some(peer_id.to_string())
                            } else {
                                None
                            }
                        }) {
                            Some(peer_id) => peer_id,
                            None => continue,
                        };
                        
                        // If not connected, try to reconnect
                        if !connected_peers.contains(&peer_id) {
                            debug!("Reconnecting to bootstrap node: {}", node.name);
                            drop(net); // Release the read lock before acquiring write lock
                            let _ = network.write().await.connect(addr).await;
                        }
                    }
                }
            }
        });
        
        self.task = Some(task);
        
        Ok(())
    }
    
    /// Stop the discovery service
    pub async fn stop(&mut self) -> Result<(), NetworkError> {
        if !self.running {
            return Ok(());
        }
        
        if let Some(task) = self.task.take() {
            task.abort();
            let _ = task.await;
        }
        
        self.running = false;
        
        Ok(())
    }
} 