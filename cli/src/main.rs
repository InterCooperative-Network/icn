use clap::{Parser, Subcommand};
use serde_json::Value;
use std::path::PathBuf;
use std::net::SocketAddr;
use anyhow::{Result, Context};
use wallet_core::identity::{IdentityWallet, IdentityScope};
use wallet_agent::queue::ProposalQueue;
use wallet_agent::governance::Guardian;
use wallet_sync::client::SyncClient;
use wallet_ui_api::WalletAPI;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Storage directory for wallet data
    #[arg(short, long, default_value = "./wallet-data")]
    data_dir: PathBuf,
    
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new identity
    Create {
        /// Type of identity (personal, organization, device, service)
        #[arg(short, long, default_value = "personal")]
        scope: String,
        
        /// Optional JSON metadata
        #[arg(short, long)]
        metadata: Option<String>,
    },
    
    /// Sign a proposal
    Sign {
        /// Path to identity file
        #[arg(short, long)]
        identity: PathBuf,
        
        /// Type of proposal
        #[arg(short, long)]
        proposal_type: String,
        
        /// Path to proposal content JSON file
        #[arg(short, long)]
        content: PathBuf,
    },
    
    /// Sync from federation
    Sync {
        /// Path to identity file
        #[arg(short, long)]
        identity: PathBuf,
        
        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },
    
    /// Start the wallet API server
    Serve {
        /// Host to bind to
        #[arg(short, long, default_value = "127.0.0.1")]
        host: String,
        
        /// Port to listen on
        #[arg(short, long, default_value_t = 3000)]
        port: u16,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    let cli = Cli::parse();
    
    // Ensure data directory exists
    if !cli.data_dir.exists() {
        std::fs::create_dir_all(&cli.data_dir)
            .context("Failed to create data directory")?;
    }
    
    match &cli.command {
        Commands::Create { scope, metadata } => {
            let scope_enum = match scope.to_lowercase().as_str() {
                "personal" => IdentityScope::Personal,
                "organization" => IdentityScope::Organization,
                "device" => IdentityScope::Device,
                "service" => IdentityScope::Service,
                _ => IdentityScope::Custom(scope.clone()),
            };
            
            let metadata_value = if let Some(meta_str) = metadata {
                let value: Value = serde_json::from_str(meta_str)
                    .context("Failed to parse metadata JSON")?;
                Some(value)
            } else {
                None
            };
            
            let wallet = IdentityWallet::new(scope_enum, metadata_value);
            let did = wallet.did.to_string();
            let document = wallet.to_document();
            
            // Save the identity
            let id = uuid::Uuid::new_v4().to_string();
            let identity_dir = cli.data_dir.join("identities");
            std::fs::create_dir_all(&identity_dir)
                .context("Failed to create identities directory")?;
                
            let file_path = identity_dir.join(format!("{}.json", id));
            let serialized = serde_json::to_string_pretty(&wallet)
                .context("Failed to serialize identity")?;
                
            std::fs::write(&file_path, serialized)
                .context("Failed to write identity file")?;
                
            println!("Created new identity:");
            println!("ID: {}", id);
            println!("DID: {}", did);
            println!("Scope: {:?}", wallet.scope);
            println!("Saved to: {}", file_path.display());
            
            // Pretty print the DID document
            println!("\nDID Document:");
            println!("{}", serde_json::to_string_pretty(&document)?);
        },
        
        Commands::Sign { identity, proposal_type, content } => {
            // Load the identity
            let identity_data = std::fs::read_to_string(identity)
                .context("Failed to read identity file")?;
                
            let wallet: IdentityWallet = serde_json::from_str(&identity_data)
                .context("Failed to parse identity JSON")?;
                
            // Load the proposal content
            let content_data = std::fs::read_to_string(content)
                .context("Failed to read content file")?;
                
            let content_value: Value = serde_json::from_str(&content_data)
                .context("Failed to parse content JSON")?;
                
            // Create queue and guardian
            let queue_dir = cli.data_dir.join("queue");
            let queue = ProposalQueue::new(queue_dir, wallet.clone());
            let guardian = Guardian::new(wallet, queue);
            
            // Create and sign the proposal
            let action_id = guardian.create_proposal(proposal_type, content_value)
                .context("Failed to create proposal")?;
                
            println!("Proposal signed successfully:");
            println!("Action ID: {}", action_id);
            println!("Type: {}", proposal_type);
        },
        
        Commands::Sync { identity, verbose } => {
            // Load the identity
            let identity_data = std::fs::read_to_string(identity)
                .context("Failed to read identity file")?;
                
            let wallet: IdentityWallet = serde_json::from_str(&identity_data)
                .context("Failed to parse identity JSON")?;
                
            // Create sync client
            let client = SyncClient::new(wallet, None)
                .context("Failed to create sync client")?;
                
            println!("Syncing trust bundles...");
            let bundles = client.sync_trust_bundles().await
                .context("Failed to sync trust bundles")?;
                
            println!("Synced {} trust bundles", bundles.len());
            
            if *verbose && !bundles.is_empty() {
                println!("\nSynced Trust Bundles:");
                for (i, bundle) in bundles.iter().enumerate() {
                    println!("{}. {} (v{}) - {} guardians, threshold: {}", 
                        i + 1, bundle.name, bundle.version, bundle.guardians.len(), bundle.threshold);
                        
                    if *verbose {
                        println!("   Guardians:");
                        for (j, guardian) in bundle.guardians.iter().enumerate() {
                            println!("   {}. {}", j + 1, guardian);
                        }
                        println!();
                    }
                }
            }
        },
        
        Commands::Serve { host, port } => {
            let addr: SocketAddr = format!("{}:{}", host, port).parse()
                .context("Invalid host:port combination")?;
                
            println!("Starting wallet API server on {}", addr);
            
            let api = WalletAPI::new(&cli.data_dir);
            
            // Use a spawn block to handle the async server without Error trait issues
            tokio::spawn(async move {
                if let Err(e) = api.run(addr).await {
                    eprintln!("Server error: {}", e);
                }
            }).await?;
        },
    }
    
    Ok(())
}
