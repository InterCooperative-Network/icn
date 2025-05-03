use libp2p::{
    identity::{Keypair, PublicKey},
    core::transport::Transport,
    gossipsub::{
        Gossipsub, GossipsubConfig, GossipsubMessage, 
        MessageAuthenticity, ValidationMode, MessageId
    },
    noise,
    swarm::{Swarm, SwarmEvent, SwarmBuilder},
    tcp, yamux, PeerId, Multiaddr, futures::StreamExt,
};
use tokio::sync::mpsc;
use std::collections::HashSet;
use thiserror::Error;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task::JoinHandle;

/// Errors that can occur in the network layer
#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Transport error: {0}")]
    Transport(String),
    
    #[error("Swarm error: {0}")]
    Swarm(String),
    
    #[error("Gossipsub error: {0}")]
    Gossipsub(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Channel error: {0}")]
    Channel(String),
}

type Result<T> = std::result::Result<T, NetworkError>;

/// Topics in the gossipsub network
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum NetworkTopic {
    /// Announce new threads
    ThreadAnnounce,
    
    /// Announce new credential links
    CredentialLinkAnnounce,
    
    /// Request sync for a thread
    ThreadSyncRequest,
}

impl NetworkTopic {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ThreadAnnounce => "icn/threads/announce/v1",
            Self::CredentialLinkAnnounce => "icn/links/announce/v1",
            Self::ThreadSyncRequest => "icn/threads/sync/v1",
        }
    }
}

/// Events that can occur in the network
#[derive(Clone, Debug)]
pub enum NetworkEvent {
    /// Received a message on a topic
    Message {
        peer_id: String,
        topic: NetworkTopic,
        data: Vec<u8>,
    },
    
    /// New peer connected
    PeerConnected(String),
    
    /// Peer disconnected
    PeerDisconnected(String),
}

/// Commands to control the network
#[derive(Debug)]
pub enum NetworkCommand {
    /// Publish a message to a topic
    Publish {
        topic: NetworkTopic,
        data: Vec<u8>,
    },
    
    /// Connect to a peer
    Connect(Multiaddr),
    
    /// Disconnect from a peer
    Disconnect(PeerId),
    
    /// Stop the network
    Stop,
}

/// Main network layer for p2p communication using libp2p
pub struct FederationNetwork {
    /// Local peer ID
    local_peer_id: PeerId,
    
    /// Local peer key
    local_key: Keypair,
    
    /// Known peers
    known_peers: Arc<Mutex<HashSet<String>>>,
    
    /// Command sender for the network task
    command_tx: Option<mpsc::Sender<NetworkCommand>>,
    
    /// Event receiver from the network task
    event_rx: Option<mpsc::Receiver<NetworkEvent>>,
    
    /// Handle for the network task
    network_task: Option<JoinHandle<()>>,
}

impl FederationNetwork {
    /// Create a new federation network
    pub fn new() -> Result<Self> {
        // Generate a new identity
        let local_key = Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(local_key.public());
        
        Ok(Self {
            local_peer_id,
            local_key,
            known_peers: Arc::new(Mutex::new(HashSet::new())),
            command_tx: None,
            event_rx: None,
            network_task: None,
        })
    }
    
    /// Start the network
    pub async fn start(&mut self) -> Result<()> {
        // Create channels for communication
        let (command_tx, mut command_rx) = mpsc::channel::<NetworkCommand>(32);
        let (event_tx, event_rx) = mpsc::channel::<NetworkEvent>(32);
        
        self.command_tx = Some(command_tx);
        self.event_rx = Some(event_rx);
        
        // Create the transport
        let transport = tcp::tokio::Transport::new(tcp::Config::default())
            .upgrade(libp2p::core::upgrade::Version::V1)
            .authenticate(noise::Config::new(&self.local_key).expect("Failed to create noise config"))
            .multiplex(yamux::Config::default())
            .timeout(Duration::from_secs(20))
            .boxed();
        
        // Create the gossipsub protocol
        let gossipsub_config = GossipsubConfig::default();
        let message_authenticity = MessageAuthenticity::Signed(self.local_key.clone());
        let mut gossipsub = Gossipsub::new(message_authenticity, gossipsub_config)
            .map_err(|e| NetworkError::Gossipsub(e.to_string()))?;
        
        // Subscribe to topics
        for topic in [
            NetworkTopic::ThreadAnnounce,
            NetworkTopic::CredentialLinkAnnounce,
            NetworkTopic::ThreadSyncRequest,
        ] {
            let topic_hash = libp2p::gossipsub::IdentTopic::new(topic.as_str());
            gossipsub
                .subscribe(&topic_hash)
                .map_err(|e| NetworkError::Gossipsub(format!("Failed to subscribe to topic: {}", e)))?;
        }
        
        // Create the swarm
        let mut swarm = SwarmBuilder::with_tokio_executor(transport, gossipsub, self.local_peer_id)
            .build();
        
        // Start listening on a port
        swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse().unwrap())
            .map_err(|e| NetworkError::Swarm(e.to_string()))?;
        
        // Clone for use in the task
        let known_peers = self.known_peers.clone();
        let event_tx_clone = event_tx.clone();
        
        // Spawn the network task
        let network_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Handle swarm events
                    event = swarm.select_next_some() => {
                        match event {
                            SwarmEvent::Behaviour(gossipsub_event) => {
                                if let libp2p::gossipsub::GossipsubEvent::Message { 
                                    propagation_source, 
                                    message_id, 
                                    message 
                                } = gossipsub_event {
                                    // Determine the topic
                                    let topic_str = message.topic.as_str();
                                    let topic = match topic_str {
                                        "icn/threads/announce/v1" => NetworkTopic::ThreadAnnounce,
                                        "icn/links/announce/v1" => NetworkTopic::CredentialLinkAnnounce,
                                        "icn/threads/sync/v1" => NetworkTopic::ThreadSyncRequest,
                                        _ => continue, // Unknown topic
                                    };
                                    
                                    // Send the event
                                    let peer_id = propagation_source.to_string();
                                    let _ = event_tx.send(NetworkEvent::Message {
                                        peer_id,
                                        topic,
                                        data: message.data,
                                    }).await;
                                }
                            }
                            SwarmEvent::NewListenAddr { address, .. } => {
                                println!("Listening on {address:?}");
                            }
                            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                                let peer_id_str = peer_id.to_string();
                                {
                                    let mut peers = known_peers.lock().unwrap();
                                    peers.insert(peer_id_str.clone());
                                }
                                let _ = event_tx.send(NetworkEvent::PeerConnected(peer_id_str)).await;
                            }
                            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                                let peer_id_str = peer_id.to_string();
                                {
                                    let mut peers = known_peers.lock().unwrap();
                                    peers.remove(&peer_id_str);
                                }
                                let _ = event_tx.send(NetworkEvent::PeerDisconnected(peer_id_str)).await;
                            }
                            _ => {} // Ignore other events
                        }
                    }
                    
                    // Handle commands
                    Some(command) = command_rx.recv() => {
                        match command {
                            NetworkCommand::Publish { topic, data } => {
                                let topic_hash = libp2p::gossipsub::IdentTopic::new(topic.as_str());
                                if let Err(e) = swarm.behaviour_mut().publish(topic_hash, data) {
                                    eprintln!("Failed to publish message: {}", e);
                                }
                            }
                            NetworkCommand::Connect(addr) => {
                                if let Err(e) = swarm.dial(addr) {
                                    eprintln!("Failed to dial: {}", e);
                                }
                            }
                            NetworkCommand::Disconnect(peer_id) => {
                                swarm.disconnect_peer_id(peer_id);
                            }
                            NetworkCommand::Stop => {
                                break;
                            }
                        }
                    }
                }
            }
        });
        
        self.network_task = Some(network_task);
        
        Ok(())
    }
    
    /// Stop the network
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(tx) = &self.command_tx {
            let _ = tx.send(NetworkCommand::Stop).await;
        }
        
        if let Some(task) = self.network_task.take() {
            task.await.map_err(|e| NetworkError::Swarm(e.to_string()))?;
        }
        
        self.command_tx = None;
        self.event_rx = None;
        
        Ok(())
    }
    
    /// Publish a message to a topic
    pub async fn publish(&self, topic: NetworkTopic, data: Vec<u8>) -> Result<()> {
        if let Some(tx) = &self.command_tx {
            tx.send(NetworkCommand::Publish { topic, data })
                .await
                .map_err(|e| NetworkError::Channel(e.to_string()))?;
            Ok(())
        } else {
            Err(NetworkError::Swarm("Network not started".to_string()))
        }
    }
    
    /// Connect to a peer
    pub async fn connect(&self, addr: Multiaddr) -> Result<()> {
        if let Some(tx) = &self.command_tx {
            tx.send(NetworkCommand::Connect(addr))
                .await
                .map_err(|e| NetworkError::Channel(e.to_string()))?;
            Ok(())
        } else {
            Err(NetworkError::Swarm("Network not started".to_string()))
        }
    }
    
    /// Get known peers
    pub async fn known_peers(&self) -> Result<Vec<String>> {
        let peers = self.known_peers.lock().unwrap();
        Ok(peers.iter().cloned().collect())
    }
    
    /// Get the next event from the network
    pub async fn next_event(&mut self) -> Option<NetworkEvent> {
        if let Some(rx) = &mut self.event_rx {
            rx.recv().await
        } else {
            None
        }
    }
} 