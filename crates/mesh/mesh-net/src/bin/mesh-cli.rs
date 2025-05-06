use anyhow::{anyhow, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use cid::Cid;
use libp2p::Multiaddr;
use mesh_net::MeshNetwork;
use mesh_types::{ComputeOffer, ExecutionReceipt, TaskIntent, VerificationReceipt, events::MeshEvent};
use std::str::FromStr;
use tokio::sync::mpsc;
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start a mesh network node
    Start {
        /// Multiaddress to listen on
        #[arg(short, long, default_value = "/ip4/0.0.0.0/tcp/9000")]
        listen: String,
    },
    
    /// Publish a new task to the network
    PublishTask {
        /// WASM module CID
        #[arg(short, long)]
        wasm_cid: String,
        
        /// Input data CID
        #[arg(short, long)]
        input_cid: String,
        
        /// Fee offered (in tokens)
        #[arg(short, long, default_value_t = 100)]
        fee: u64,
        
        /// Required number of verifiers
        #[arg(short, long, default_value_t = 3)]
        verifiers: u32,
    },
    
    /// Offer to execute a task
    OfferExecution {
        /// Task CID
        #[arg(short, long)]
        task_cid: String,
        
        /// Estimated cost
        #[arg(short, long)]
        cost: u64,
    },
    
    /// List active peers in the network
    ListPeers,
}

/// Handle events from the network
async fn handle_events(mut receiver: mpsc::Receiver<MeshEvent>) {
    while let Some(event) = receiver.recv().await {
        match event {
            MeshEvent::TaskPublished(task) => {
                info!("New task published: {:?}", task);
            }
            MeshEvent::OfferReceived(offer) => {
                info!("New execution offer received: {:?}", offer);
            }
            MeshEvent::TaskExecuted(receipt) => {
                info!("Task execution reported: {:?}", receipt);
            }
            MeshEvent::TaskVerified(receipt) => {
                info!("Task verification submitted: {:?}", receipt);
            }
            MeshEvent::PeerJoined(info) => {
                info!("New peer joined: {:?}", info);
            }
            MeshEvent::PeerLeft(did) => {
                info!("Peer left: {}", did);
            }
            MeshEvent::ReputationUpdated(rep) => {
                info!("Reputation updated: {:?}", rep);
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Start { listen } => {
            info!("Starting mesh network node...");
            
            let listen_addr = Multiaddr::from_str(&listen)?;
            let (mut network, event_rx) = MeshNetwork::new().await?;
            
            // Spawn task to handle incoming events
            tokio::spawn(handle_events(event_rx));
            
            // Start the network (this will block)
            network.start(listen_addr).await?;
        },
        Commands::PublishTask { wasm_cid, input_cid, fee, verifiers } => {
            info!("Publishing task to network...");
            
            let (mut network, _) = MeshNetwork::new().await?;
            
            let task = TaskIntent {
                publisher_did: "did:icn:mesh:publisher".to_string(),
                wasm_cid: Cid::from_str(&wasm_cid)?,
                input_cid: Cid::from_str(&input_cid)?,
                fee,
                verifiers,
                expiry: Utc::now() + chrono::Duration::hours(24),
                metadata: None,
            };
            
            network.event_receiver.send(MeshEvent::TaskPublished(task)).await?;
            
            info!("Task published successfully");
        },
        Commands::OfferExecution { task_cid, cost } => {
            info!("Offering to execute task...");
            
            let (mut network, _) = MeshNetwork::new().await?;
            
            let offer = ComputeOffer {
                worker_did: "did:icn:mesh:worker".to_string(),
                task_cid: Cid::from_str(&task_cid)?,
                cost_estimate: cost,
                available_capacity: 100,
                estimated_time_ms: 5000,
                timestamp: Utc::now(),
                signature: vec![],
            };
            
            network.event_receiver.send(MeshEvent::OfferReceived(offer)).await?;
            
            info!("Execution offer submitted successfully");
        },
        Commands::ListPeers => {
            info!("Listing active peers...");
            
            let (network, _) = MeshNetwork::new().await?;
            let peers = network.get_peers();
            
            if peers.is_empty() {
                info!("No peers connected");
            } else {
                for (peer_id, info) in peers {
                    info!("Peer {}: {:?}", peer_id, info);
                }
            }
        },
    }
    
    Ok(())
} 