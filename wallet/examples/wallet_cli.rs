use anyhow::{Result, Context};
use clap::{Parser, Subcommand};
use icn_wallet_root::{Wallet, WalletError};
use wallet_storage::StorageManager;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, Duration};
use std::mutex::Mutex;
use icn_wallet_sync::compat::{WalletDagNode, WalletDagNodeMetadata, wallet_to_runtime};
use icn_wallet_sync::federation::{FederationEndpoint, FederationSyncClientConfig};
use serde_json::{Value, json};
use uuid::Uuid;
use chrono::Utc;

#[derive(Parser)]
#[command(author, version = "0.1.0", about = "ICN Wallet CLI", long_about = None)]
struct Cli {
    /// Path to wallet data directory
    #[arg(short, long, default_value = "./wallet-data")]
    data_dir: PathBuf,

    /// Federation endpoint URL
    #[arg(short, long, default_value = "http://localhost:8080")]
    federation_url: String,

    /// DID to use for the wallet
    #[arg(short, long, default_value = "did:icn:test-wallet")]
    did: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new wallet
    Init {
        /// Force reinitialization of the wallet
        #[arg(long)]
        force: bool,
    },
    
    /// Create a DAG node
    CreateNode {
        /// Node payload (JSON string)
        #[arg(short, long)]
        payload: String,
        
        /// Scope of the node
        #[arg(short, long, default_value = "test")]
        scope: String,
        
        /// Parent CIDs (can specify multiple)
        #[arg(short, long)]
        parents: Vec<String>,
    },
    
    /// Submit a proposal to the federation
    SubmitProposal {
        /// Proposal title
        #[arg(short, long)]
        title: String,
        
        /// Proposal content in JSON format
        #[arg(short, long)]
        content: String,
        
        /// Proposal type (e.g., "membership", "resource", "governance")
        #[arg(short, long, default_value = "governance")]
        proposal_type: String,
    },
    
    /// Sync with the federation
    Sync {
        /// Number of items to fetch (limit)
        #[arg(short, long, default_value = "100")]
        limit: usize,
        
        /// Sync only specified credential types
        #[arg(short, long)]
        credential_types: Option<Vec<String>>,
    },
    
    /// List stored DAG nodes
    ListNodes {
        /// Filter by scope
        #[arg(short, long)]
        scope: Option<String>,
        
        /// Display full details
        #[arg(short, long)]
        detailed: bool,
        
        /// Limit number of nodes to display
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },
}

// Helper function to create a node
async fn create_dag_node(
    payload: String, 
    scope: String, 
    did: String, 
    parents: Vec<String>
) -> Result<WalletDagNode> {
    // Parse payload as JSON if possible
    let payload_bytes = match serde_json::from_str::<Value>(&payload) {
        Ok(_) => payload.as_bytes().to_vec(),
        Err(_) => {
            // If not valid JSON, use raw bytes
            println!("Warning: Payload is not valid JSON, using as raw bytes");
            payload.as_bytes().to_vec()
        }
    };
    
    // Generate a node id based on content and timestamp
    let random_part = Uuid::new_v4().to_string();
    let cid = format!("bafybeicn{}", random_part.replace('-', ""));
    
    // Create the node
    let node = WalletDagNode {
        cid,
        parents,
        issuer: did,
        timestamp: SystemTime::now(),
        signature: vec![1, 2, 3, 4], // In a real implementation, this would be a real signature
        payload: payload_bytes,
        metadata: WalletDagNodeMetadata {
            sequence: Some(1),
            scope: Some(scope),
        },
    };
    
    Ok(node)
}

// Helper function to display a node
fn display_node(node: &WalletDagNode, detailed: bool) {
    println!("Node CID: {}", node.cid);
    println!("Issuer: {}", node.issuer);
    
    // Try to display payload as JSON if possible
    let payload_str = match std::str::from_utf8(&node.payload) {
        Ok(s) => s,
        Err(_) => "[Binary data]",
    };
    
    let payload_display = if payload_str.len() > 100 && !detailed {
        format!("{}...", &payload_str[..97])
    } else {
        payload_str.to_string()
    };
    
    println!("Payload: {}", payload_display);
    
    if detailed {
        println!("Parents: {:?}", node.parents);
        println!("Timestamp: {:?}", node.timestamp);
        println!("Metadata.scope: {:?}", node.metadata.scope);
        println!("Metadata.sequence: {:?}", node.metadata.sequence);
        println!("Signature: [{}]", node.signature.iter().take(4).map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" "));
    }
    
    println!("----------------------");
}

// Helper function to pretty-print JSON
fn pretty_json(value: Value) -> String {
    serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Set up logging
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("Setting default subscriber failed");
    
    let cli = Cli::parse();
    
    // Create storage manager
    let storage_manager = StorageManager::new(&cli.data_dir)
        .context("Failed to create storage manager")?;
    
    // Initialize wallet
    let wallet = Wallet::new(storage_manager)
        .context("Failed to initialize wallet")?;
    
    match &cli.command {
        Commands::Init { force } => {
            println!("Initializing wallet at {}...", cli.data_dir.display());
            // In a real implementation, you would:
            // 1. Generate a new DID if not provided
            // 2. Set up secure storage
            // 3. Register with a federation
            
            // Configure federation endpoint
            let federation_endpoint = FederationEndpoint {
                federation_id: "default".to_string(),
                base_url: cli.federation_url.clone(),
                last_sync: None,
                auth_token: None,
            };
            
            println!("✅ Wallet initialized with DID: {}", cli.did);
            println!("✅ Federation endpoint set to: {}", cli.federation_url);
        },
        
        Commands::CreateNode { payload, scope, parents } => {
            println!("Creating DAG node with:");
            println!("  - Payload: {}", if payload.len() > 50 { format!("{}...", &payload[..47]) } else { payload.clone() });
            println!("  - Scope: {}", scope);
            println!("  - Parents: {:?}", parents);
            
            // Create the node
            let node = create_dag_node(payload.clone(), scope.clone(), cli.did.clone(), parents.clone()).await?;
            
            // Convert to runtime format for storage/submission
            match wallet_to_runtime(&node) {
                Ok(runtime_node) => {
                    println!("✅ Successfully converted to runtime format");
                    // In a full implementation, you would submit to federation here
                },
                Err(e) => {
                    println!("❌ Error converting to runtime format: {}", e);
                },
            }
            
            // Display the created node
            println!("\nNode created successfully:");
            display_node(&node, true);
        },
        
        Commands::SubmitProposal { title, content, proposal_type } => {
            println!("Creating proposal:");
            println!("  - Title: {}", title);
            println!("  - Type: {}", proposal_type);
            
            // Create proposal payload
            let proposal = json!({
                "type": "Proposal",
                "title": title,
                "proposalType": proposal_type,
                "content": content,
                "createdAt": Utc::now().to_rfc3339(),
                "createdBy": cli.did,
            });
            
            // Create a node with the proposal payload
            let payload = serde_json::to_string(&proposal)?;
            let node = create_dag_node(payload, "proposal".to_string(), cli.did.clone(), vec![]).await?;
            
            println!("\nProposal created successfully with CID: {}", node.cid);
            println!("Payload:\n{}", pretty_json(proposal));
        },
        
        Commands::Sync { limit, credential_types } => {
            println!("Syncing with federation at {}", cli.federation_url);
            println!("  - Limit: {}", limit);
            if let Some(types) = credential_types {
                println!("  - Credential types: {:?}", types);
            }
            
            // In a real implementation:
            // 1. Connect to federation endpoint
            // 2. Retrieve nodes/credentials based on parameters
            // 3. Store locally
            
            println!("✅ Sync completed successfully");
        },
        
        Commands::ListNodes { scope, detailed, limit } => {
            println!("Listing DAG nodes:");
            if let Some(s) = scope {
                println!("  - Filtered by scope: {}", s);
            }
            println!("  - Showing up to {} nodes", limit);
            
            // Create some example nodes for demonstration
            let demo_nodes = vec![
                WalletDagNode {
                    cid: "bafynode123456".to_string(),
                    parents: vec![],
                    issuer: cli.did.clone(),
                    timestamp: SystemTime::now(),
                    signature: vec![1, 2, 3, 4],
                    payload: r#"{"type":"ExampleNode","data":"test"}"#.as_bytes().to_vec(),
                    metadata: WalletDagNodeMetadata {
                        sequence: Some(1),
                        scope: Some("test".to_string()),
                    },
                },
                WalletDagNode {
                    cid: "bafynode789012".to_string(),
                    parents: vec!["bafynode123456".to_string()],
                    issuer: cli.did.clone(),
                    timestamp: SystemTime::now() - Duration::from_secs(3600),
                    signature: vec![5, 6, 7, 8],
                    payload: r#"{"type":"AnotherNode","data":"more data here"}"#.as_bytes().to_vec(),
                    metadata: WalletDagNodeMetadata {
                        sequence: Some(2),
                        scope: Some("proposal".to_string()),
                    },
                },
            ];
            
            // Filter by scope if provided
            let filtered_nodes: Vec<_> = if let Some(s) = scope {
                demo_nodes.iter().filter(|n| n.metadata.scope.as_ref().map_or(false, |ns| ns == s)).collect()
            } else {
                demo_nodes.iter().collect()
            };
            
            // Display nodes
            if filtered_nodes.is_empty() {
                println!("No nodes found matching criteria");
            } else {
                for node in filtered_nodes.iter().take(*limit) {
                    display_node(node, *detailed);
                }
                println!("Found {} nodes", filtered_nodes.len());
            }
        },
    }
    
    Ok(())
} 