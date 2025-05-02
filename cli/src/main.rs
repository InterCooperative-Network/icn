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
    
    /// Output format (plain, json)
    #[arg(short, long, default_value = "plain")]
    format: String,
    
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
        
        /// AgoraNet API URL
        #[arg(long)]
        agoranet_url: Option<String>,
    },
    
    /// AgoraNet integration commands
    AgoraNet {
        /// Path to identity file
        #[arg(short, long)]
        identity: PathBuf,
        
        /// AgoraNet API URL
        #[arg(short, long, default_value = "https://agoranet.icn.network/api")]
        url: String,
        
        #[command(subcommand)]
        command: AgoraNetCommands,
    },
    
    /// Trust bundle management
    Bundle {
        /// Path to identity file
        #[arg(short, long)]
        identity: PathBuf,
        
        #[command(subcommand)]
        command: BundleCommands,
    },
    
    /// List all identities
    List,
}

#[derive(Subcommand)]
enum AgoraNetCommands {
    /// List threads
    ListThreads {
        /// Filter by proposal ID
        #[arg(short, long)]
        proposal_id: Option<String>,
        
        /// Filter by topic
        #[arg(short, long)]
        topic: Option<String>,
    },
    
    /// Get thread details
    GetThread {
        /// Thread ID
        #[arg(short, long)]
        id: String,
    },
    
    /// Link a credential to a thread
    LinkCredential {
        /// Thread ID
        #[arg(short, long)]
        thread_id: String,
        
        /// Credential ID
        #[arg(short, long)]
        credential_id: String,
    },
    
    /// Notify about proposal events
    NotifyEvent {
        /// Proposal ID
        #[arg(short, long)]
        proposal_id: String,
        
        /// Event type
        #[arg(short, long)]
        event_type: String,
        
        /// Details JSON file
        #[arg(short, long)]
        details: PathBuf,
    },
}

#[derive(Subcommand)]
enum BundleCommands {
    /// List trust bundles
    List {
        /// Output format (json, table)
        #[arg(short, long, default_value = "table")]
        format: String,
    },
    
    /// Sync trust bundles from federation
    Sync {
        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },
    
    /// Check guardian status
    CheckStatus,
    
    /// Create execution receipt
    CreateReceipt {
        /// Proposal ID
        #[arg(short, long)]
        proposal_id: String,
        
        /// Result JSON file
        #[arg(short, long)]
        result: PathBuf,
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
    
    let output_json = cli.format.to_lowercase() == "json";
    
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
            
            if output_json {
                // Output as JSON
                let response = serde_json::json!({
                    "id": id,
                    "did": did,
                    "scope": scope,
                    "document": document,
                    "file_path": file_path.to_string_lossy(),
                });
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                // Output as plain text
                println!("Created new identity:");
                println!("ID: {}", id);
                println!("DID: {}", did);
                println!("Scope: {:?}", wallet.scope);
                println!("Saved to: {}", file_path.display());
                
                // Pretty print the DID document
                println!("\nDID Document:");
                println!("{}", serde_json::to_string_pretty(&document)?);
            }
        },
        
        Commands::List => {
            // List all identities in the data_dir/identities folder
            let identity_dir = cli.data_dir.join("identities");
            if !identity_dir.exists() {
                if output_json {
                    println!("[]");
                } else {
                    println!("No identities found.");
                }
                return Ok(());
            }
            
            let mut identities = Vec::new();
            
            for entry in std::fs::read_dir(&identity_dir)? {
                let entry = entry?;
                let path = entry.path();
                
                if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                    let data = std::fs::read_to_string(&path)?;
                    let wallet: IdentityWallet = serde_json::from_str(&data)?;
                    
                    let file_name = path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("unknown");
                    
                    let id = file_name.trim_end_matches(".json").to_string();
                    
                    if output_json {
                        identities.push(serde_json::json!({
                            "id": id,
                            "did": wallet.did,
                            "scope": format!("{:?}", wallet.scope),
                            "metadata": wallet.metadata,
                        }));
                    } else {
                        println!("ID: {}", id);
                        println!("DID: {}", wallet.did);
                        println!("Scope: {:?}", wallet.scope);
                        println!();
                    }
                }
            }
            
            if output_json {
                println!("{}", serde_json::to_string_pretty(&identities)?);
            }
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
                
            if output_json {
                let response = serde_json::json!({
                    "action_id": action_id,
                    "proposal_type": proposal_type,
                    "signed": true
                });
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                println!("Proposal signed successfully:");
                println!("Action ID: {}", action_id);
                println!("Type: {}", proposal_type);
            }
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
        
        Commands::Serve { host, port, agoranet_url } => {
            let addr: SocketAddr = format!("{}:{}", host, port).parse()
                .context("Invalid host:port combination")?;
                
            println!("Starting wallet API server on {}", addr);
            
            let mut api = WalletAPI::new(&cli.data_dir);
            
            // Configure AgoraNet URL if provided
            if let Some(url) = agoranet_url {
                api = api.with_agoranet_url(&url);
                println!("Using AgoraNet URL: {}", url);
            }
            
            // Use a spawn block to handle the async server without Error trait issues
            tokio::spawn(async move {
                if let Err(e) = api.run(addr).await {
                    eprintln!("Server error: {}", e);
                }
            }).await?;
        },
        
        Commands::AgoraNet { identity, url, command } => {
            // Load the identity
            let identity_data = std::fs::read_to_string(identity)
                .context("Failed to read identity file")?;
                
            let wallet: IdentityWallet = serde_json::from_str(&identity_data)
                .context("Failed to parse identity JSON")?;
                
            // Create AgoraNet client
            let agoranet = wallet_agent::agoranet::AgoraNetClient::new(
                wallet.clone(), 
                Some(url)
            );
            
            match command {
                AgoraNetCommands::ListThreads { proposal_id, topic } => {
                    let threads = agoranet.get_threads(
                        proposal_id.as_deref(),
                        topic.as_deref()
                    ).await
                        .context("Failed to fetch threads")?;
                    
                    if output_json {
                        println!("{}", serde_json::to_string_pretty(&threads)?);
                    } else {
                        println!("Found {} threads:", threads.len());
                        for (i, thread) in threads.iter().enumerate() {
                            println!("{}. {} (by {})", i + 1, thread.title, thread.author);
                            println!("   ID: {}", thread.id);
                            if let Some(pid) = &thread.proposal_id {
                                println!("   Proposal: {}", pid);
                            }
                            println!("   Topic: {}", thread.topic);
                            println!("   Posts: {}", thread.post_count);
                            println!("   Created: {}", thread.created_at);
                            println!();
                        }
                    }
                },
                
                AgoraNetCommands::GetThread { id } => {
                    let thread = agoranet.get_thread(&id).await
                        .context("Failed to fetch thread")?;
                    
                    println!("Thread: {}", thread.title);
                    println!("ID: {}", thread.id);
                    if let Some(pid) = &thread.proposal_id {
                        println!("Proposal: {}", pid);
                    }
                    println!("Topic: {}", thread.topic);
                    println!("Author: {}", thread.author);
                    println!("Created: {}", thread.created_at);
                    println!();
                    
                    println!("Posts:");
                    for (i, post) in thread.posts.iter().enumerate() {
                        println!("{}. From {} at {}", i + 1, post.author, post.created_at);
                        println!("   {}", post.content);
                        println!();
                    }
                    
                    if !thread.credential_links.is_empty() {
                        println!("Linked Credentials:");
                        for (i, link) in thread.credential_links.iter().enumerate() {
                            println!("{}. {} ({})", i + 1, link.credential_type, link.credential_id);
                            println!("   Issuer: {}", link.issuer);
                            println!("   Subject: {}", link.subject);
                            println!();
                        }
                    }
                },
                
                AgoraNetCommands::LinkCredential { thread_id, credential_id } => {
                    // In a real implementation, we'd load the credential from storage
                    // For now, we'll create a dummy credential
                    let signer = wallet_core::credential::CredentialSigner::new(wallet);
                    let credential = signer.issue_credential(
                        serde_json::json!({
                            "id": credential_id,
                            "name": "Sample Credential",
                            "type": "MembershipCredential"
                        }),
                        vec!["MembershipCredential".to_string()]
                    ).context("Failed to create credential")?;
                    
                    let link = agoranet.link_credential(&thread_id, &credential).await
                        .context("Failed to link credential")?;
                    
                    println!("Successfully linked credential:");
                    println!("Link ID: {}", link.id);
                    println!("Thread: {}", link.thread_id);
                    println!("Credential: {} ({})", link.credential_type, link.credential_id);
                    println!("Created: {}", link.created_at);
                },
                
                AgoraNetCommands::NotifyEvent { proposal_id, event_type, details } => {
                    // Load the details JSON
                    let details_data = std::fs::read_to_string(details)
                        .context("Failed to read details file")?;
                        
                    let details_value: Value = serde_json::from_str(&details_data)
                        .context("Failed to parse details JSON")?;
                    
                    agoranet.notify_proposal_event(&proposal_id, &event_type, details_value).await
                        .context("Failed to notify AgoraNet")?;
                    
                    println!("Successfully notified AgoraNet about event:");
                    println!("Proposal: {}", proposal_id);
                    println!("Event Type: {}", event_type);
                },
            }
        },
        
        Commands::Bundle { identity, command } => {
            // Load the identity
            let identity_data = std::fs::read_to_string(identity)
                .context("Failed to read identity file")?;
                
            let wallet: IdentityWallet = serde_json::from_str(&identity_data)
                .context("Failed to parse identity JSON")?;
                
            // Create queue and guardian
            let queue_dir = cli.data_dir.join("queue");
            let bundle_dir = cli.data_dir.join("bundles");
            let queue = ProposalQueue::new(&queue_dir, wallet.clone());
            let guardian = Guardian::new(wallet, queue).with_bundle_storage(bundle_dir);
            
            match command {
                BundleCommands::List { format } => {
                    let bundles = guardian.list_trust_bundles().await
                        .context("Failed to list bundles")?;
                    
                    if output_json || format == "json" {
                        println!("{}", serde_json::to_string_pretty(&bundles)
                            .context("Failed to serialize bundles")?);
                    } else {
                        println!("Found {} trust bundles:", bundles.len());
                        for (i, bundle) in bundles.iter().enumerate() {
                            println!("{}. {} (v{}) - {} guardians, threshold: {}", 
                                i + 1, bundle.name, bundle.version, 
                                bundle.guardians.len(), bundle.threshold);
                            println!("   ID: {}", bundle.id);
                            println!("   Active: {}", bundle.active);
                            println!();
                        }
                    }
                },
                
                BundleCommands::Sync { verbose } => {
                    // Create sync client
                    let client = SyncClient::new(wallet, None)
                        .context("Failed to create sync client")?;
                    
                    // Load local bundles first
                    let local_count = guardian.load_trust_bundles_from_disk().await
                        .context("Failed to load local bundles")?;
                    
                    if *verbose {
                        println!("Loaded {} local trust bundles", local_count);
                    }
                    
                    // Sync from federation
                    let network_bundles = client.sync_trust_bundles().await
                        .context("Failed to sync trust bundles")?;
                    
                    // Store the bundles
                    let mut stored_count = 0;
                    for bundle in &network_bundles {
                        if guardian.store_trust_bundle(bundle.clone()).await.is_ok() {
                            stored_count += 1;
                        }
                    }
                    
                    println!("Synced {} trust bundles from federation", network_bundles.len());
                    println!("Successfully stored {} bundles", stored_count);
                    
                    if *verbose && !network_bundles.is_empty() {
                        println!("\nSynced Trust Bundles:");
                        for (i, bundle) in network_bundles.iter().enumerate() {
                            println!("{}. {} (v{}) - {} guardians, threshold: {}", 
                                i + 1, bundle.name, bundle.version, 
                                bundle.guardians.len(), bundle.threshold);
                                
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
                
                BundleCommands::CheckStatus => {
                    let is_guardian = guardian.is_active_guardian().await
                        .context("Failed to check guardian status")?;
                    
                    if is_guardian {
                        println!("This identity IS an active guardian");
                    } else {
                        println!("This identity is NOT an active guardian");
                    }
                },
                
                BundleCommands::CreateReceipt { proposal_id, result } => {
                    // Load the result JSON
                    let result_data = std::fs::read_to_string(result)
                        .context("Failed to read result file")?;
                        
                    let result_value: Value = serde_json::from_str(&result_data)
                        .context("Failed to parse result JSON")?;
                    
                    let receipt = guardian.create_execution_receipt(&proposal_id, result_value)
                        .context("Failed to create execution receipt")?;
                    
                    println!("Successfully created execution receipt:");
                    println!("Proposal: {}", receipt.proposal_id);
                    println!("Executed by: {}", receipt.executed_by);
                    println!("Timestamp: {}", receipt.timestamp);
                    println!("Result: {}", serde_json::to_string_pretty(&receipt.result)
                        .context("Failed to serialize result")?);
                },
            }
        },
    }
    
    Ok(())
}
