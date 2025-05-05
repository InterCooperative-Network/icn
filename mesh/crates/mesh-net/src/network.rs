use anyhow::{anyhow, Result};
use async_trait::async_trait;
use libp2p::{
    core::upgrade,
    gossipsub::{self, Gossipsub, GossipsubMessage, MessageAuthenticity, MessageId, Topic},
    identity, kad, mdns, noise, swarm::SwarmEvent, tcp, yamux, Multiaddr, PeerId, Swarm,
};
use mesh_types::{
    ComputeOffer, ExecutionReceipt, PeerInfo, ReputationSnapshot, TaskIntent, VerificationReceipt,
    events::MeshEvent,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Core structure managing the Mesh Network P2P connections
pub struct MeshNetwork {
    /// The libp2p Swarm that manages all P2P connections
    swarm: Swarm<MeshBehaviour>,
    
    /// Local peer ID derived from local keypair
    local_peer_id: PeerId,
    
    /// Map of connected peers and their information
    peers: Arc<Mutex<HashMap<PeerId, PeerInfo>>>,
    
    /// Channel for receiving outbound messages to be sent to the network
    pub event_sender: mpsc::Sender<MeshEvent>,
    
    /// Channel for receiving inbound messages from the network
    pub event_receiver: mpsc::Receiver<MeshEvent>,
    
    /// Set of topics this node is subscribed to
    subscribed_topics: HashSet<String>,
}

/// Combined behavior for the Mesh Network
#[derive(libp2p::swarm::NetworkBehaviour)]
#[behaviour(out_event = "MeshNetworkEvent")]
pub struct MeshBehaviour {
    gossipsub: Gossipsub,
    kademlia: kad::Kademlia<kad::store::MemoryStore>,
    mdns: mdns::async_io::Behaviour,
    identify: libp2p::identify::Behaviour,
}

/// Events emitted by the network behavior
#[derive(Debug)]
pub enum MeshNetworkEvent {
    Gossipsub(gossipsub::Event),
    Kademlia(kad::Event),
    Mdns(mdns::Event),
    Identify(libp2p::identify::Event),
}

/// Network message types that can be sent over gossipsub
#[derive(Debug, Serialize, Deserialize)]
pub enum MeshNetworkMessage {
    /// Publish a new task intent
    PublishTask(TaskIntent),
    
    /// Submit an offer to execute a task
    SubmitOffer(ComputeOffer),
    
    /// Report task execution results
    ReportExecution(ExecutionReceipt),
    
    /// Submit verification of a task execution
    SubmitVerification(VerificationReceipt),
    
    /// Announce peer joining the network
    AnnouncePresence(PeerInfo),
    
    /// Announce peer leaving the network
    AnnounceDeparture(String), // DID of departing peer
    
    /// Share reputation updates
    ShareReputation(ReputationSnapshot),
}

impl MeshNetwork {
    /// Create a new MeshNetwork instance
    pub async fn new() -> Result<(Self, mpsc::Receiver<MeshEvent>)> {
        // Generate a new identity keypair
        let local_key = identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(local_key.public());
        info!("Local peer id: {}", local_peer_id);

        // Create channel for network events
        let (event_sender, event_receiver) = mpsc::channel(100);
        let (outbound_sender, outbound_receiver) = mpsc::channel(100);

        // Set up gossipsub configuration
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(10))
            .validation_mode(gossipsub::ValidationMode::Strict)
            .build()
            .map_err(|e| anyhow!("Failed to build gossipsub config: {}", e))?;

        // Create gossipsub
        let gossipsub = Gossipsub::new(
            MessageAuthenticity::Signed(local_key.clone()),
            gossipsub_config,
        )
        .map_err(|e| anyhow!("Failed to create gossipsub: {}", e))?;

        // Set up Kademlia DHT
        let store = kad::store::MemoryStore::new(local_peer_id);
        let kademlia = kad::Kademlia::new(local_peer_id, store);

        // Set up mDNS for local peer discovery
        let mdns = mdns::async_io::Behaviour::new(mdns::Config::default(), local_peer_id)
            .map_err(|e| anyhow!("Failed to create mDNS: {}", e))?;

        // Set up identify protocol
        let identify = libp2p::identify::Behaviour::new(
            libp2p::identify::Config::new("/mesh/1.0.0".to_string(), local_key.public())
        );

        // Combine all behaviors
        let behaviour = MeshBehaviour {
            gossipsub,
            kademlia,
            mdns,
            identify,
        };

        // Build the swarm
        let transport = libp2p::development_transport(local_key).await?;
        
        let swarm = libp2p::SwarmBuilder::with_tokio_executor(
            transport,
            behaviour,
            local_peer_id,
        )
        .build();

        let network = Self {
            swarm,
            local_peer_id,
            peers: Arc::new(Mutex::new(HashMap::new())),
            event_sender,
            event_receiver: outbound_receiver,
            subscribed_topics: HashSet::new(),
        };

        Ok((network, outbound_receiver))
    }

    /// Start the network and begin processing events
    pub async fn start(&mut self, listen_addr: Multiaddr) -> Result<()> {
        // Listen on the provided multiaddress
        self.swarm.listen_on(listen_addr)?;

        // Subscribe to default topics
        self.subscribe_to_topic("tasks")?;
        self.subscribe_to_topic("offers")?;
        self.subscribe_to_topic("executions")?;
        self.subscribe_to_topic("verifications")?;
        self.subscribe_to_topic("peers")?;

        // Main event loop
        loop {
            tokio::select! {
                event = self.swarm.select_next_some() => match event {
                    SwarmEvent::Behaviour(behaviour_event) => {
                        self.handle_behaviour_event(behaviour_event).await?;
                    }
                    SwarmEvent::NewListenAddr { address, .. } => {
                        info!("Listening on {}", address);
                    }
                    SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                        info!("Connected to {}", peer_id);
                    }
                    SwarmEvent::ConnectionClosed { peer_id, .. } => {
                        info!("Disconnected from {}", peer_id);
                    }
                    _ => {}
                },
                Some(outbound_event) = self.event_receiver.recv() => {
                    self.handle_outbound_event(outbound_event).await?;
                }
            }
        }
    }

    /// Handle events from the network behavior
    async fn handle_behaviour_event(&mut self, event: MeshNetworkEvent) -> Result<()> {
        match event {
            MeshNetworkEvent::Gossipsub(gossipsub::Event::Message {
                propagation_source: peer_id,
                message_id: id,
                message,
            }) => {
                debug!("Received gossipsub message: {} from {}", id, peer_id);
                self.handle_gossipsub_message(message).await?;
            }
            MeshNetworkEvent::Mdns(mdns::Event::Discovered(list)) => {
                for (peer_id, addr) in list {
                    info!("mDNS discovered peer: {}", peer_id);
                    self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                    self.swarm.dial(peer_id)?;
                }
            }
            MeshNetworkEvent::Identify(libp2p::identify::Event::Received { peer_id, info }) => {
                info!("Identified peer: {}", peer_id);
                for addr in info.listen_addrs {
                    self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Handle incoming gossipsub messages
    async fn handle_gossipsub_message(&mut self, message: GossipsubMessage) -> Result<()> {
        let topic = message.topic.to_string();
        let data = message.data;
        
        // Deserialize the message based on topic
        match topic.as_str() {
            "tasks" => {
                let msg: MeshNetworkMessage = serde_json::from_slice(&data)?;
                if let MeshNetworkMessage::PublishTask(task) = msg {
                    self.event_sender.send(MeshEvent::TaskPublished(task)).await?;
                }
            }
            "offers" => {
                let msg: MeshNetworkMessage = serde_json::from_slice(&data)?;
                if let MeshNetworkMessage::SubmitOffer(offer) = msg {
                    self.event_sender.send(MeshEvent::OfferReceived(offer)).await?;
                }
            }
            "executions" => {
                let msg: MeshNetworkMessage = serde_json::from_slice(&data)?;
                if let MeshNetworkMessage::ReportExecution(receipt) = msg {
                    self.event_sender.send(MeshEvent::TaskExecuted(receipt)).await?;
                }
            }
            "verifications" => {
                let msg: MeshNetworkMessage = serde_json::from_slice(&data)?;
                if let MeshNetworkMessage::SubmitVerification(receipt) = msg {
                    self.event_sender.send(MeshEvent::TaskVerified(receipt)).await?;
                }
            }
            "peers" => {
                let msg: MeshNetworkMessage = serde_json::from_slice(&data)?;
                match msg {
                    MeshNetworkMessage::AnnouncePresence(peer_info) => {
                        self.event_sender.send(MeshEvent::PeerJoined(peer_info.clone())).await?;
                        self.peers.lock().unwrap().insert(PeerId::random(), peer_info);
                    }
                    MeshNetworkMessage::AnnounceDeparture(did) => {
                        self.event_sender.send(MeshEvent::PeerLeft(did.clone())).await?;
                        self.peers.lock().unwrap().retain(|_, p| p.did != did);
                    }
                    MeshNetworkMessage::ShareReputation(rep) => {
                        self.event_sender.send(MeshEvent::ReputationUpdated(rep)).await?;
                    }
                    _ => {}
                }
            }
            _ => {
                debug!("Received message for unknown topic: {}", topic);
            }
        }
        
        Ok(())
    }

    /// Handle outbound events to be sent to the network
    async fn handle_outbound_event(&mut self, event: MeshEvent) -> Result<()> {
        match event {
            MeshEvent::TaskPublished(task) => {
                let msg = MeshNetworkMessage::PublishTask(task);
                self.publish_to_topic("tasks", msg).await?;
            }
            MeshEvent::OfferReceived(offer) => {
                let msg = MeshNetworkMessage::SubmitOffer(offer);
                self.publish_to_topic("offers", msg).await?;
            }
            MeshEvent::TaskExecuted(receipt) => {
                let msg = MeshNetworkMessage::ReportExecution(receipt);
                self.publish_to_topic("executions", msg).await?;
            }
            MeshEvent::TaskVerified(receipt) => {
                let msg = MeshNetworkMessage::SubmitVerification(receipt);
                self.publish_to_topic("verifications", msg).await?;
            }
            MeshEvent::PeerJoined(info) => {
                let msg = MeshNetworkMessage::AnnouncePresence(info);
                self.publish_to_topic("peers", msg).await?;
            }
            MeshEvent::PeerLeft(did) => {
                let msg = MeshNetworkMessage::AnnounceDeparture(did);
                self.publish_to_topic("peers", msg).await?;
            }
            MeshEvent::ReputationUpdated(rep) => {
                let msg = MeshNetworkMessage::ShareReputation(rep);
                self.publish_to_topic("peers", msg).await?;
            }
        }
        
        Ok(())
    }

    /// Subscribe to a gossipsub topic
    fn subscribe_to_topic(&mut self, topic_str: &str) -> Result<()> {
        let topic = Topic::new(topic_str);
        self.swarm.behaviour_mut().gossipsub.subscribe(&topic)?;
        self.subscribed_topics.insert(topic_str.to_string());
        info!("Subscribed to topic: {}", topic_str);
        Ok(())
    }

    /// Publish a message to a gossipsub topic
    async fn publish_to_topic(&mut self, topic_str: &str, message: MeshNetworkMessage) -> Result<()> {
        let topic = Topic::new(topic_str);
        let data = serde_json::to_vec(&message)?;
        
        self.swarm.behaviour_mut().gossipsub.publish(topic, data)
            .map_err(|e| anyhow!("Failed to publish to {}: {}", topic_str, e))?;
        
        debug!("Published message to topic: {}", topic_str);
        Ok(())
    }

    /// Get a clone of the current peer map
    pub fn get_peers(&self) -> HashMap<PeerId, PeerInfo> {
        self.peers.lock().unwrap().clone()
    }

    /// Get the local peer ID
    pub fn get_local_peer_id(&self) -> PeerId {
        self.local_peer_id
    }
}

/// Trait defining the mesh network interface
#[async_trait]
pub trait MeshNetworkInterface {
    /// Publish a task to the network
    async fn publish_task(&self, task: TaskIntent) -> Result<()>;
    
    /// Submit an offer to execute a task
    async fn submit_offer(&self, offer: ComputeOffer) -> Result<()>;
    
    /// Report execution of a task
    async fn report_execution(&self, receipt: ExecutionReceipt) -> Result<()>;
    
    /// Submit verification of a task
    async fn submit_verification(&self, receipt: VerificationReceipt) -> Result<()>;
    
    /// Get list of all known peers
    async fn get_peers(&self) -> Result<Vec<PeerInfo>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test::block_on;
    
    // Helper function to create a test network
    async fn create_test_network() -> (MeshNetwork, mpsc::Receiver<MeshEvent>) {
        let addr: Multiaddr = "/ip4/127.0.0.1/tcp/0".parse().unwrap();
        let (mut network, event_rx) = MeshNetwork::new().await.unwrap();
        
        // We don't actually start the network here to avoid blocking
        
        (network, event_rx)
    }
    
    #[test]
    fn test_network_creation() {
        block_on(async {
            let (network, _) = create_test_network().await;
            assert!(!network.get_local_peer_id().to_string().is_empty());
            assert!(network.subscribed_topics.is_empty()); // Not subscribed yet since we didn't call start()
        });
    }
} 