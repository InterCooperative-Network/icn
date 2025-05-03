use clap::{Parser, Subcommand};
use serde_json::Value;
use std::path::PathBuf;
use std::net::SocketAddr;
use uuid::Uuid;
use colored::*;
use wallet_core::store::FileStore;
use wallet_core::identity::{IdentityWallet, IdentityScope};
use wallet_core::credential::CredentialSigner;
use wallet_agent::queue::{ActionQueue, ActionType};
use wallet_ui_api::state::AppConfig;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
struct Cli {
    /// Data directory for the wallet
    #[clap(long, env = "ICN_WALLET_DATA_DIR", default_value = "./wallet-data")]
    data_dir: PathBuf,
    
    /// Federation API URL
    #[clap(long, env = "ICN_FEDERATION_URL", default_value = "https://federation.example.com/api")]
    federation_url: String,
    
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Identity management commands
    Identity {
        #[clap(subcommand)]
        command: IdentityCommands,
    },
    
    /// Credential management commands
    Credential {
        #[clap(subcommand)]
        command: CredentialCommands,
    },
    
    /// Action queue management commands
    Action {
        #[clap(subcommand)]
        command: ActionCommands,
    },
    
    /// DAG management commands
    Dag {
        #[clap(subcommand)]
        command: DagCommands,
    },
    
    /// Start the wallet API server
    Serve {
        /// Address to listen on
        #[clap(long, default_value = "127.0.0.1:3000")]
        addr: SocketAddr,
    },
}

#[derive(Subcommand)]
enum IdentityCommands {
    /// Create a new identity
    Create {
        /// The scope of the identity
        #[clap(long, default_value = "personal")]
        scope: String,
        
        /// Optional metadata as JSON
        #[clap(long)]
        metadata: Option<String>,
    },
    
    /// List all identities
    List,
    
    /// Show details of an identity
    Show {
        /// The DID of the identity
        did: String,
    },
}

#[derive(Subcommand)]
enum CredentialCommands {
    /// Issue a new credential
    Issue {
        /// The DID of the issuer
        #[clap(long)]
        issuer: String,
        
        /// The subject data as JSON
        #[clap(long)]
        subject: String,
        
        /// The credential types (comma-separated)
        #[clap(long, use_value_delimiter = true, value_delimiter = ',')]
        types: Vec<String>,
    },
    
    /// List all credentials
    List,
    
    /// Show details of a credential
    Show {
        /// The ID of the credential
        id: String,
    },
}

#[derive(Subcommand)]
enum ActionCommands {
    /// Queue a new action
    Queue {
        /// The DID of the creator
        #[clap(long)]
        creator: String,
        
        /// The type of action
        #[clap(long)]
        action_type: String,
        
        /// The payload as JSON
        #[clap(long)]
        payload: String,
    },
    
    /// List all pending actions
    List,
    
    /// Process a pending action
    Process {
        /// The ID of the action
        id: String,
        
        /// The DID of the processor
        #[clap(long)]
        processor: String,
    },
}

#[derive(Subcommand)]
enum DagCommands {
    /// List all DAG threads
    ListThreads,
    
    /// Show a DAG thread
    ShowThread {
        /// The ID of the thread
        id: String,
    },
    
    /// Show a DAG node
    ShowNode {
        /// The CID of the node
        cid: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    // Parse command line arguments
    let cli = Cli::parse();
    
    // Create the store
    let store = FileStore::new(&cli.data_dir);
    
    // Initialize the store
    store.init().await
        .map_err(|e| format!("Failed to initialize store: {}", e))?;
    
    // Process commands
    match &cli.command {
        Commands::Identity { command } => {
            process_identity_command(command, &store).await?;
        },
        Commands::Credential { command } => {
            process_credential_command(command, &store).await?;
        },
        Commands::Action { command } => {
            process_action_command(command, &store).await?;
        },
        Commands::Dag { command } => {
            process_dag_command(command, &store).await?;
        },
        Commands::Serve { addr } => {
            println!("{}", format!("Starting wallet API server on http://{}", addr).green());
            
            // Create the config
            let config = AppConfig {
                federation_url: cli.federation_url.clone(),
                data_dir: cli.data_dir.to_string_lossy().to_string(),
                auto_sync: true,
                sync_interval: 60,
            };
            
            // Start the server
            wallet_ui_api::start_server(store, config, *addr).await?;
        },
    }
    
    Ok(())
}

async fn process_identity_command(
    command: &IdentityCommands,
    store: &FileStore,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        IdentityCommands::Create { scope, metadata } => {
            let scope_enum = match scope.as_str() {
                "personal" => IdentityScope::Personal,
                "organization" => IdentityScope::Organization,
                "device" => IdentityScope::Device,
                "service" => IdentityScope::Service,
                custom => IdentityScope::Custom(custom.to_string()),
            };
            
            let metadata_value = if let Some(metadata_str) = metadata {
                Some(serde_json::from_str(metadata_str)?)
            } else {
                None
            };
            
            let identity = IdentityWallet::new(scope_enum, metadata_value);
            
            store.save_identity(&identity).await?;
            
            println!("{}", "Identity created successfully:".green());
            println!("DID: {}", identity.did.to_string().yellow());
            println!("Public Key: {}", base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &identity.keypair.public_key_bytes()
            ).yellow());
        },
        IdentityCommands::List => {
            let identities = store.list_identities().await?;
            
            if identities.is_empty() {
                println!("{}", "No identities found.".yellow());
                return Ok(());
            }
            
            println!("{}", "Identities:".green());
            for did in identities {
                println!("- {}", did.yellow());
            }
        },
        IdentityCommands::Show { did } => {
            let identity = store.load_identity(did).await?;
            
            println!("{}", "Identity details:".green());
            println!("DID: {}", identity.did.to_string().yellow());
            println!("Public Key: {}", base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &identity.keypair.public_key_bytes()
            ).yellow());
            println!("Scope: {:?}", identity.scope);
            
            if let Some(metadata) = identity.metadata {
                println!("Metadata: {}", serde_json::to_string_pretty(&metadata)?);
            }
        },
    }
    
    Ok(())
}

async fn process_credential_command(
    command: &CredentialCommands,
    store: &FileStore,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        CredentialCommands::Issue { issuer, subject, types } => {
            let issuer_identity = store.load_identity(issuer).await?;
            let subject_data: Value = serde_json::from_str(subject)?;
            
            let signer = CredentialSigner::new(issuer_identity);
            let credential = signer.issue_credential(subject_data, types.clone())?;
            
            let id = Uuid::new_v4().to_string();
            store.save_credential(&credential, &id).await?;
            
            println!("{}", "Credential issued successfully:".green());
            println!("ID: {}", id.yellow());
            println!("Issuer: {}", credential.issuer.yellow());
            println!("Types: {}", credential.credential_type.join(", ").yellow());
        },
        CredentialCommands::List => {
            let credentials = store.list_credentials().await?;
            
            if credentials.is_empty() {
                println!("{}", "No credentials found.".yellow());
                return Ok(());
            }
            
            println!("{}", "Credentials:".green());
            for id in credentials {
                println!("- {}", id.yellow());
            }
        },
        CredentialCommands::Show { id } => {
            let credential = store.load_credential(id).await?;
            
            println!("{}", "Credential details:".green());
            println!("Issuer: {}", credential.issuer.yellow());
            println!("Types: {}", credential.credential_type.join(", ").yellow());
            println!("Issuance Date: {}", credential.issuance_date);
            println!("Subject Data: {}", serde_json::to_string_pretty(&credential.credential_subject)?);
            
            if let Some(proof) = credential.proof {
                println!("Proof: {}", proof.proof_type);
                println!("Created: {}", proof.created);
                println!("Verification Method: {}", proof.verification_method);
                println!("Purpose: {}", proof.proof_purpose);
            }
        },
    }
    
    Ok(())
}

async fn process_action_command(
    command: &ActionCommands,
    store: &FileStore,
) -> Result<(), Box<dyn std::error::Error>> {
    let action_queue = ActionQueue::new(store.clone());
    
    match command {
        ActionCommands::Queue { creator, action_type, payload } => {
            let action_type_enum = match action_type.as_str() {
                "proposal" => ActionType::Proposal,
                "vote" => ActionType::Vote,
                "anchor" => ActionType::Anchor,
                _ => return Err(format!("Invalid action type: {}", action_type).into()),
            };
            
            let payload_value: Value = serde_json::from_str(payload)?;
            
            let action = action_queue.queue_action(action_type_enum, creator, payload_value).await?;
            
            println!("{}", "Action queued successfully:".green());
            println!("ID: {}", action.id.yellow());
            println!("Type: {:?}", action.action_type);
            println!("Status: {:?}", action.status);
        },
        ActionCommands::List => {
            let actions = action_queue.get_pending_actions().await?;
            
            if actions.is_empty() {
                println!("{}", "No pending actions found.".yellow());
                return Ok(());
            }
            
            println!("{}", "Pending Actions:".green());
            for action in actions {
                println!("- ID: {}, Type: {:?}, Creator: {}", 
                    action.id.yellow(), 
                    action.action_type,
                    action.creator_did);
            }
        },
        ActionCommands::Process { id, processor } => {
            let identity = store.load_identity(processor).await?;
            
            action_queue.process_action(id, &identity).await?;
            
            println!("{}", "Action processed successfully:".green());
        },
    }
    
    Ok(())
}

async fn process_dag_command(
    command: &DagCommands,
    store: &FileStore,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        DagCommands::ListThreads => {
            let threads = store.list_dag_threads().await?;
            
            if threads.is_empty() {
                println!("{}", "No DAG threads found.".yellow());
                return Ok(());
            }
            
            println!("{}", "DAG Threads:".green());
            for id in threads {
                println!("- {}", id.yellow());
            }
        },
        DagCommands::ShowThread { id } => {
            let thread = store.load_dag_thread(id).await?;
            
            println!("{}", "Thread details:".green());
            println!("Type: {:?}", thread.thread_type);
            println!("Creator: {}", thread.creator.yellow());
            println!("Root CID: {}", thread.root_cid.yellow());
            println!("Latest CID: {}", thread.latest_cid.yellow());
            
            if let Some(title) = thread.title {
                println!("Title: {}", title);
            }
            
            if let Some(description) = thread.description {
                println!("Description: {}", description);
            }
            
            println!("Created: {}", thread.created_at);
            println!("Updated: {}", thread.updated_at);
        },
        DagCommands::ShowNode { cid } => {
            let node = store.load_dag_node(cid).await?;
            
            println!("{}", "Node details:".green());
            println!("Created: {}", node.created_at);
            println!("Data: {}", serde_json::to_string_pretty(&node.data)?);
            
            if !node.links.is_empty() {
                println!("{}", "Links:".green());
                for (name, target_cid) in &node.links {
                    println!("- {}: {}", name, target_cid.yellow());
                }
            }
            
            if !node.signatures.is_empty() {
                println!("{}", "Signatures:".green());
                for (did, signature) in &node.signatures {
                    println!("- {}: {}", did, signature);
                }
            }
        },
    }
    
    Ok(())
}
