use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use colored::*;
use futures::StreamExt;
use icn_identity::{Did, VerifiableCredential};
use indicatif::{ProgressBar, ProgressStyle};
use mesh_net::{MeshExecutionEngine, MeshNetwork, TaskStatus};
use mesh_reputation::{ReputationInterface, ReputationSystem};
use mesh_types::{
    ComputeOffer, ExecutionReceipt, MeshPolicy, PeerInfo, ReputationSnapshot, TaskIntent,
    VerificationReceipt,
};
use prettytable::{row, Cell, Row, Table};
use std::{
    path::PathBuf,
    time::Duration,
};
use tracing::{debug, error, info, warn};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

/// ICN Mesh Compute Control CLI
#[derive(Parser)]
#[clap(name = "meshctl", version = "0.1.0", author = "ICN Mesh Team", about = "Mesh Compute overlay control")]
struct Cli {
    /// Set the log level (info, debug, trace)
    #[clap(short, long, default_value = "info")]
    log_level: String,
    
    /// Commands
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start a mesh node
    Start {
        /// Node ID
        #[clap(short, long)]
        node_id: Option<String>,
        
        /// Multiaddress to listen on
        #[clap(short, long, default_value = "/ip4/0.0.0.0/tcp/8765")]
        listen: String,
        
        /// Multiaddresses to connect to
        #[clap(short, long)]
        peers: Vec<String>,
        
        /// Path to directory for WASM modules
        #[clap(long, default_value = "./wasm")]
        wasm_dir: PathBuf,
        
        /// Path to directory for data (inputs/outputs)
        #[clap(long, default_value = "./data")]
        data_dir: PathBuf,
    },
    
    /// Submit a new task
    Submit {
        /// Path to WASM module
        #[clap(short, long)]
        wasm: PathBuf,
        
        /// Path to input data
        #[clap(short, long)]
        input: PathBuf,
        
        /// Fee to offer (in tokens)
        #[clap(short, long, default_value = "100")]
        fee: u64,
        
        /// Number of verifiers required
        #[clap(short, long, default_value = "3")]
        verifiers: u32,
        
        /// Task expiry in minutes
        #[clap(short, long, default_value = "60")]
        expiry: u64,
    },
    
    /// Offer to execute a task
    Offer {
        /// Task CID to execute
        #[clap(long)]
        task: String,
        
        /// Estimated cost
        #[clap(short, long)]
        cost: u64,
        
        /// Available capacity
        #[clap(short, long, default_value = "100")]
        capacity: u32,
        
        /// Estimated time in milliseconds
        #[clap(short, long)]
        time: u64,
    },
    
    /// Execute a task
    Execute {
        /// Task CID to execute
        #[clap(long)]
        task: String,
    },
    
    /// Verify a task execution
    Verify {
        /// Execution receipt CID to verify
        #[clap(long)]
        receipt: String,
    },
    
    /// List tasks and their status
    Tasks,
    
    /// List peers and their reputation
    Peers,
    
    /// Update mesh policy
    Policy {
        /// Alpha parameter
        #[clap(long, default_value = "0.6")]
        alpha: f64,
        
        /// Beta parameter
        #[clap(long, default_value = "0.4")]
        beta: f64,
        
        /// Gamma parameter
        #[clap(long, default_value = "1.0")]
        gamma: f64,
        
        /// Lambda parameter
        #[clap(long, default_value = "0.01")]
        lambda: f64,
        
        /// Stake weight
        #[clap(long, default_value = "0.2")]
        stake_weight: f64,
        
        /// Minimum fee
        #[clap(long, default_value = "10")]
        min_fee: u64,
        
        /// Capacity units
        #[clap(long, default_value = "100")]
        capacity: u32,
    },
}

/// Initialize logging
fn init_logging(log_level: &str) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(log_level));

    let subscriber = FmtSubscriber::builder()
        .with_env_filter(filter)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set global default subscriber");
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Initialize logging
    init_logging(&cli.log_level);
    
    match cli.command {
        Commands::Start {
            node_id,
            listen,
            peers,
            wasm_dir,
            data_dir,
        } => {
            start_node(node_id, listen, peers, wasm_dir, data_dir).await?;
        }
        Commands::Submit {
            wasm,
            input,
            fee,
            verifiers,
            expiry,
        } => {
            submit_task(wasm, input, fee, verifiers, expiry).await?;
        }
        Commands::Offer {
            task,
            cost,
            capacity,
            time,
        } => {
            offer_execution(task, cost, capacity, time).await?;
        }
        Commands::Execute { task } => {
            execute_task(task).await?;
        }
        Commands::Verify { receipt } => {
            verify_execution(receipt).await?;
        }
        Commands::Tasks => {
            list_tasks().await?;
        }
        Commands::Peers => {
            list_peers().await?;
        }
        Commands::Policy {
            alpha,
            beta,
            gamma,
            lambda,
            stake_weight,
            min_fee,
            capacity,
        } => {
            update_policy(alpha, beta, gamma, lambda, stake_weight, min_fee, capacity).await?;
        }
    }
    
    Ok(())
}

async fn start_node(
    node_id: Option<String>,
    listen: String,
    peers: Vec<String>,
    wasm_dir: PathBuf,
    data_dir: PathBuf,
) -> Result<()> {
    info!("Starting mesh node...");
    
    // Create data directories if they don't exist
    tokio::fs::create_dir_all(&wasm_dir).await?;
    tokio::fs::create_dir_all(&data_dir).await?;
    
    // Use provided node ID or generate one
    let node_did = if let Some(id) = node_id {
        id
    } else {
        format!("did:icn:mesh:{}", uuid::Uuid::new_v4())
    };
    
    info!("Node DID: {}", node_did);
    
    // Parse the listen address
    let listen_addr = listen.parse()?;
    
    // Create mesh network
    let (mut network, event_rx) = MeshNetwork::new().await?;
    
    // Create mesh execution engine
    let event_sender = network.event_sender.clone();
    let execution_engine = MeshExecutionEngine::new(event_sender, wasm_dir, data_dir);
    
    // Create reputation system with default policy
    let policy = MeshPolicy {
        alpha: 0.6,
        beta: 0.4,
        gamma: 1.0,
        lambda: 0.01,
        stake_weight: 0.2,
        min_fee: 10,
        capacity_units: 100,
    };
    
    let reputation = ReputationSystem::new(policy);
    
    // Handle events
    let execution_engine_clone = execution_engine;
    let reputation_clone = reputation;
    
    tokio::spawn(async move {
        handle_events(event_rx, execution_engine_clone, reputation_clone).await;
    });
    
    // Connect to peers
    for peer in peers {
        if let Ok(addr) = peer.parse() {
            info!("Connecting to peer: {}", peer);
            if let Err(e) = network.swarm.dial(addr) {
                warn!("Failed to connect to peer {}: {}", peer, e);
            }
        } else {
            warn!("Invalid peer address: {}", peer);
        }
    }
    
    // Start the network
    info!("Listening on {}", listen);
    
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(ProgressStyle::default_spinner()
        .template("{spinner:.green} {msg}")
        .unwrap()
        .tick_strings(&[
            "◐",
            "◓",
            "◑",
            "◒",
        ]));
    
    spinner.set_message("Running mesh node... Press Ctrl+C to exit");
    
    // Main event loop
    loop {
        spinner.tick();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn handle_events(
    mut receiver: tokio::sync::mpsc::Receiver<mesh_types::events::MeshEvent>,
    execution_engine: MeshExecutionEngine,
    reputation: ReputationSystem,
) {
    while let Some(event) = receiver.recv().await {
        match event {
            mesh_types::events::MeshEvent::TaskPublished(task) => {
                info!("New task published: {:?}", task);
                if let Err(e) = execution_engine.process_task(task).await {
                    error!("Failed to process task: {}", e);
                }
            }
            mesh_types::events::MeshEvent::OfferReceived(offer) => {
                info!("New execution offer received: {:?}", offer);
                if let Err(e) = execution_engine.process_offer(offer).await {
                    error!("Failed to process offer: {}", e);
                }
            }
            mesh_types::events::MeshEvent::TaskExecuted(receipt) => {
                info!("Task execution reported: {:?}", receipt);
                if let Err(e) = execution_engine.process_execution(receipt.clone()).await {
                    error!("Failed to process execution: {}", e);
                }
                
                if let Err(e) = reputation.process_execution(&receipt).await {
                    error!("Failed to update reputation: {}", e);
                }
            }
            mesh_types::events::MeshEvent::TaskVerified(receipt) => {
                info!("Task verification submitted: {:?}", receipt);
                // For simplicity, we'll assume all verifications match consensus
                if let Err(e) = reputation.process_verification(&receipt, receipt.verdict).await {
                    error!("Failed to update reputation: {}", e);
                }
            }
            mesh_types::events::MeshEvent::PeerJoined(info) => {
                info!("New peer joined: {:?}", info);
            }
            mesh_types::events::MeshEvent::PeerLeft(did) => {
                info!("Peer left: {}", did);
            }
            mesh_types::events::MeshEvent::ReputationUpdated(rep) => {
                info!("Reputation updated: {:?}", rep);
            }
        }
    }
}

async fn submit_task(
    wasm: PathBuf,
    input: PathBuf,
    fee: u64,
    verifiers: u32,
    expiry: u64,
) -> Result<()> {
    // This would typically upload the WASM and input to IPFS/content storage
    // and then create and submit a task intent
    
    // For demonstration, print what would have happened
    info!("Submitting task with WASM from {:?}", wasm);
    info!("Input data from {:?}", input);
    info!("Fee: {}, Verifiers: {}, Expiry: {} minutes", fee, verifiers, expiry);
    
    // In a real implementation, we would create and submit the task
    
    Ok(())
}

async fn offer_execution(
    task: String,
    cost: u64,
    capacity: u32,
    time: u64,
) -> Result<()> {
    info!("Offering to execute task {}", task);
    info!("Cost: {}, Capacity: {}, Time: {}ms", cost, capacity, time);
    
    // In a real implementation, we would construct and submit the offer
    
    Ok(())
}

async fn execute_task(task: String) -> Result<()> {
    info!("Executing task {}", task);
    
    // In a real implementation, we would execute the task
    
    Ok(())
}

async fn verify_execution(receipt: String) -> Result<()> {
    info!("Verifying execution receipt {}", receipt);
    
    // In a real implementation, we would verify the execution
    
    Ok(())
}

async fn list_tasks() -> Result<()> {
    info!("Listing tasks");
    
    // For demo, create a sample table
    let mut table = Table::new();
    table.add_row(row![
        "Task CID", 
        "Publisher",
        "Status",
        "Fee",
        "Verifiers"
    ]);
    
    // In a real implementation, we would get tasks from the execution engine
    // For now, add sample data
    table.add_row(Row::new(vec![
        Cell::new("bafy..."),
        Cell::new("did:icn:publisher1"),
        Cell::new(&TaskStatus::Published.to_string()),
        Cell::new("100"),
        Cell::new("3"),
    ]));
    
    table.printstd();
    
    Ok(())
}

async fn list_peers() -> Result<()> {
    info!("Listing peers");
    
    // For demo, create a sample table
    let mut table = Table::new();
    table.add_row(row![
        "Peer DID",
        "Reputation",
        "Capacity",
        "Staked Tokens",
        "Status"
    ]);
    
    // In a real implementation, we would get peers from the network
    // For now, add sample data
    table.add_row(Row::new(vec![
        Cell::new("did:icn:peer1"),
        Cell::new("0.85").style_spec("Fg"),
        Cell::new("100"),
        Cell::new("500"),
        Cell::new("Active").style_spec("Fg"),
    ]));
    
    table.add_row(Row::new(vec![
        Cell::new("did:icn:peer2"),
        Cell::new("0.32").style_spec("Fr"),
        Cell::new("50"),
        Cell::new("200"),
        Cell::new("Active").style_spec("Fg"),
    ]));
    
    table.printstd();
    
    Ok(())
}

async fn update_policy(
    alpha: f64,
    beta: f64,
    gamma: f64,
    lambda: f64,
    stake_weight: f64,
    min_fee: u64,
    capacity: u32,
) -> Result<()> {
    info!("Updating mesh policy");
    
    // Create new policy
    let policy = MeshPolicy {
        alpha,
        beta,
        gamma,
        lambda,
        stake_weight,
        min_fee,
        capacity_units: capacity,
    };
    
    // Print policy details
    println!("New Policy:");
    println!("Alpha (execution weight): {}", alpha);
    println!("Beta (verification weight): {}", beta);
    println!("Gamma (penalty factor): {}", gamma);
    println!("Lambda (decay rate): {}", lambda);
    println!("Stake Weight: {}", stake_weight);
    println!("Minimum Fee: {}", min_fee);
    println!("Capacity Units: {}", capacity);
    
    // In a real implementation, we would apply this policy
    
    Ok(())
}
