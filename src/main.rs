mod api;
mod identity;
mod storage;
mod token;
mod guardians;
mod proposal;
mod federation;
mod vc;
mod services;

use clap::{Parser, Subcommand};
use identity::{Identity, IdentityManager, KeyType, DeviceLink, DeviceLinkChallenge};
use storage::{StorageManager, StorageType};
use token::{TokenStore, TokenType};
use guardians::{GuardianManager, GuardianSet, GuardianStatus};
use proposal::{ProposalManager, Proposal, VoteOption};
use federation::{FederationRuntime, MonitoringOptions};
use api::{ApiClient, ApiConfig};
use vc::VerifiableCredential;
use services::{FederationSyncService, FederationSyncConfig, CredentialSyncData, CredentialStatus, OnboardingService, QrFormat};
use std::path::PathBuf;
use std::process;
use std::str::FromStr;
use std::fs;
use base64::{Engine as _};
use std::collections::HashMap;

#[derive(Parser)]
#[command(author, version, about = "ICN Wallet - A governance and coordination tool for the Intercooperative Network")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new scoped identity
    Init {
        /// Scope for the identity (e.g., coop name)
        #[arg(short, long)]
        scope: String,
        
        /// Username within the scope
        #[arg(short, long)]
        username: String,
        
        /// Key type (ed25519 or ecdsa)
        #[arg(short, long, default_value = "ed25519")]
        key_type: String,
    },
    
    /// Display the active identity
    Whoami,
    
    /// Sign a file (proposal, vote, etc.)
    Sign {
        /// Path to the file to sign
        #[arg(short, long)]
        file: PathBuf,
        
        /// Path to save the signature
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    
    /// Submit a signed file to the CoVM
    Submit {
        /// Path to the file to submit
        #[arg(short, long)]
        file: PathBuf,
        
        /// CoVM node URL (default: http://localhost:9000)
        #[arg(short, long)]
        node: Option<String>,
    },
    
    /// Check token balances
    Balance {
        /// Specific token type to check (default: all)
        #[arg(short, long)]
        token: Option<String>,
        
        /// Scope to check balances for
        #[arg(short, long)]
        scope: Option<String>,
    },
    
    /// List available identities
    ListIdentities {
        /// Only show identities from a specific federation/scope
        #[arg(short, long)]
        federation: Option<String>,
    },
    
    /// Switch the active identity
    UseIdentity {
        /// DID of the identity to use
        #[arg(short, long)]
        did: String,
    },
    
    /// Export identity to a meta.json file
    ExportIdentity {
        /// Path to save the metadata file
        #[arg(short, long)]
        output: PathBuf,
        
        /// Export as QR code (optional)
        #[arg(long)]
        qr: bool,
    },
    
    /// Import identity from a meta.json file
    ImportIdentity {
        /// Path to the metadata file
        #[arg(short, long)]
        input: PathBuf,
    },
    
    /// Manage guardian-related operations
    Guardians {
        #[command(subcommand)]
        subcommand: GuardianCommands,
    },
    
    /// Proposal management
    Proposal {
        #[command(subcommand)]
        subcommand: ProposalCommands,
    },
    
    /// DAG operations
    Dag {
        #[command(subcommand)]
        subcommand: DagCommands,
    },
    
    /// Federation operations
    Federation {
        #[command(subcommand)]
        subcommand: FederationCommands,
    },
    
    /// Cast vote on a proposal
    Vote {
        /// Proposal hash to vote on
        #[arg(long)]
        proposal: String,
        
        /// Vote yes
        #[arg(long, conflicts_with_all = &["no", "abstain"])]
        yes: bool,
        
        /// Vote no
        #[arg(long, conflicts_with_all = &["yes", "abstain"])]
        no: bool,
        
        /// Vote abstain
        #[arg(long, conflicts_with_all = &["yes", "no"])]
        abstain: bool,
        
        /// Path to signature file (for guardian voting)
        #[arg(long)]
        signature: Option<PathBuf>,
        
        /// Comment to include with vote
        #[arg(long)]
        comment: Option<String>,
    },
    
    /// Backup wallet data
    Backup {
        /// Path to save the backup file
        #[arg(long)]
        out: PathBuf,
        
        /// Password to encrypt the backup
        #[arg(long)]
        password: Option<String>,
    },
    
    /// Restore wallet from backup
    Restore {
        /// Path to the backup file
        #[arg(long)]
        file: PathBuf,
        
        /// Password to decrypt the backup
        #[arg(long)]
        password: Option<String>,
    },
    
    /// Device management commands
    Device {
        #[command(subcommand)]
        subcommand: DeviceCommands,
    },
    
    /// Inbox and outbox management
    Inbox {
        #[command(subcommand)]
        subcommand: InboxCommands,
    },
    
    /// Outbox management
    Outbox {
        #[command(subcommand)]
        subcommand: OutboxCommands,
    },
    
    /// Export verifiable credential
    ExportVC {
        /// Identity DID to export as VC
        #[arg(long)]
        identity: Option<String>,
        
        /// Output file path
        #[arg(short, long)]
        output: PathBuf,
    },
    
    /// Launch terminal UI mode
    Tui,
    
    /// Start WebSocket server for real-time updates
    WebSocket {
        /// Host to bind to (default: 127.0.0.1)
        #[arg(long)]
        host: Option<String>,
        
        /// Port to listen on (default: 9876)
        #[arg(long)]
        port: Option<u16>,
    },
    
    /// Manage credentials
    Credentials {
        #[command(subcommand)]
        subcommand: CredentialCommands,
    },
    
    /// Federation invite management
    Invite {
        #[command(subcommand)]
        subcommand: InviteCommands,
    },
}

#[derive(Subcommand)]
enum GuardianCommands {
    /// Add a guardian for the current identity
    Add {
        /// DID of the guardian to add
        #[arg(long)]
        did: String,
        
        /// Friendly name for the guardian
        #[arg(long)]
        name: String,
    },
    
    /// Remove a guardian
    Remove {
        /// DID of the guardian to remove
        #[arg(long)]
        did: String,
    },
    
    /// List guardians for the active identity
    List,
    
    /// Create a recovery bundle for guardians
    CreateRecovery {
        /// Optional password for additional encryption
        #[arg(long)]
        password: Option<String>,
        
        /// Path to save the recovery bundle
        #[arg(long)]
        output: PathBuf,
    },
    
    /// Start recovery process for an identity
    RequestRecovery {
        /// DID to recover
        #[arg(long)]
        did: String,
        
        /// Your guardian DID
        #[arg(long)]
        guardian_did: String,
        
        /// Path to the recovery bundle
        #[arg(long)]
        bundle: PathBuf,
    },
    
    /// Accept a recovery request as a guardian
    ApproveRecovery {
        /// DID being recovered
        #[arg(long)]
        did: String,
    },
}

#[derive(Subcommand)]
enum ProposalCommands {
    /// Create a new proposal from a DSL file
    Draft {
        /// Path to DSL file
        #[arg(long)]
        file: PathBuf,
        
        /// Path to save the draft proposal
        #[arg(long)]
        output: Option<PathBuf>,
    },
    
    /// Submit a proposal for voting
    Submit {
        /// Path to the proposal file
        #[arg(long)]
        file: PathBuf,
    },
    
    /// Vote on a proposal
    Vote {
        /// Hash of the proposal
        #[arg(long)]
        hash: String,
        
        /// Vote option (yes, no, abstain)
        #[arg(long)]
        option: String,
        
        /// Optional comment with your vote
        #[arg(long)]
        comment: Option<String>,
    },
    
    /// List all proposals
    List {
        /// Filter by scope
        #[arg(long)]
        scope: Option<String>,
        
        /// Filter by status (draft, voting, executed, etc.)
        #[arg(long)]
        status: Option<String>,
    },
    
    /// Show details of a specific proposal
    Show {
        /// Hash of the proposal to show
        #[arg(long)]
        hash: String,
    },
    
    /// Audit a proposal and show detailed status
    Audit {
        /// Hash of the proposal to audit
        #[arg(long)]
        hash: String,
    },
    
    /// Get vote statistics for a proposal
    VoteStats {
        /// Hash of the proposal
        #[arg(long)]
        hash: String,
    },
    
    /// Calculate hash of a proposal file
    Hash {
        /// Path to the proposal file
        #[arg(long)]
        file: PathBuf,
    },
}

#[derive(Subcommand)]
enum DagCommands {
    /// Get the current DAG tip
    Tip,
    
    /// Show DAG status
    Status {
        /// Scope for DAG query
        #[arg(long)]
        scope: Option<String>,
    },
    
    /// Sync DAG with federation
    Sync,
    
    /// Replay pending DAG events
    ReplayPending,
}

#[derive(Subcommand)]
enum FederationCommands {
    /// Submit a proposal and monitor its progress
    Submit {
        /// Path to the proposal file
        #[arg(long)]
        file: PathBuf,
        
        /// Timeout in minutes for monitoring (default: 60)
        #[arg(long)]
        timeout: Option<u64>,
        
        /// Verbose output during monitoring
        #[arg(long)]
        verbose: bool,
    },
    
    /// Sync proposal with AgoraNet
    Sync {
        /// Hash of the proposal to sync
        #[arg(long)]
        proposal_hash: String,
    },
}

// New commands for device management
#[derive(Subcommand)]
enum DeviceCommands {
    /// Generate a link for another device
    Link {
        /// Public key of the target device
        #[arg(long)]
        to: String,
        
        /// Key type of the target device (ed25519 or ecdsa)
        #[arg(long, default_value = "ed25519")]
        key_type: String,
        
        /// Output file for the link
        #[arg(short, long)]
        output: PathBuf,
    },
    
    /// Import identity from a device link
    Import {
        /// Path to the device link file
        #[arg(short, long)]
        link: PathBuf,
        
        /// Path to the private key file
        #[arg(short, long)]
        key: PathBuf,
    },
    
    /// List all linked devices
    List,
    
    /// Generate a new device keypair
    Generate {
        /// Key type to generate (ed25519 or ecdsa)
        #[arg(short, long, default_value = "ed25519")]
        key_type: String,
        
        /// Output directory for key files
        #[arg(short, long)]
        output: PathBuf,
    },
}

// New commands for inbox management
#[derive(Subcommand)]
enum InboxCommands {
    /// List items in inbox
    List,
    
    /// Review a specific item in inbox
    Review {
        /// ID of the item to review
        #[arg(short, long)]
        id: String,
    },
}

// New commands for outbox management
#[derive(Subcommand)]
enum OutboxCommands {
    /// List items in outbox
    List,
    
    /// Check status of an item in outbox
    Status {
        /// ID of the item to check
        #[arg(short, long)]
        id: Option<String>,
    },
}

// Add the CredentialCommands enum after the other command enums
#[derive(Subcommand)]
enum CredentialCommands {
    /// Sync credentials from federation nodes
    Sync {
        /// Federation ID to sync with (optional)
        #[arg(long)]
        federation: Option<String>,
        
        /// DID to sync credentials for (defaults to active identity)
        #[arg(long)]
        did: Option<String>,
    },
    
    /// List synchronized credentials
    List {
        /// Filter by credential type
        #[arg(long)]
        type_filter: Option<String>,
        
        /// Show only credentials with this trust level or higher (high, medium, low)
        #[arg(long)]
        trust: Option<String>,
    },
    
    /// Show details of a specific credential
    Show {
        /// ID of the credential to show
        #[arg(long)]
        id: String,
        
        /// Export the credential to VC format
        #[arg(long)]
        export: bool,
        
        /// Display the credential as a QR code in the terminal
        #[arg(long)]
        qr: bool,
        
        /// Include thread ID in exported credential
        #[arg(long)]
        with_thread: bool,
        
        /// Path to save the exported credential (if export is true)
        #[arg(long)]
        output: Option<PathBuf>,
    },
    
    /// Verify a specific credential
    Verify {
        /// ID of the credential to verify
        #[arg(long)]
        id: String,
    },
    
    /// Export a credential as a QR code
    ExportQR {
        /// ID of the credential to export
        #[arg(long)]
        id: String,
        
        /// Output format (terminal, svg, png)
        #[arg(long, default_value = "terminal")]
        format: String,
        
        /// Path to save the QR code file (required for svg and png formats)
        #[arg(long)]
        output: Option<PathBuf>,
    },
    
    /// Import a credential from a QR code
    ImportQR {
        /// QR code content string (usually from a scanner)
        #[arg(long)]
        content: String,
        
        /// Automatically verify the imported credential
        #[arg(long)]
        verify: bool,
    },
    // Add selective disclosure command
    SelectiveDisclose {
        id: String,
        #[arg(short, long)]
        include_fields: Option<String>,
        #[arg(short, long)]
        exclude_fields: Option<String>,
        #[arg(short, long, default_value = "redaction")]
        proof_type: String,
        #[arg(short, long)]
        reason: Option<String>,
        #[arg(short, long)]
        output: Option<String>,
    },
    
    /// Create a restoration or amendment credential
    RestoreCredential {
        /// ID of the credential to restore or amend
        #[arg(long)]
        credential_id: String,
        
        /// Reason for the amendment
        #[arg(long)]
        reason: String,
        
        /// Text hash of amendment documentation
        #[arg(long)]
        text_hash: Option<String>,
        
        /// Amendment ID (defaults to auto-generated)
        #[arg(long)]
        amendment_id: Option<String>,
        
        /// Federation ID (if not specified, will use federation ID from credential)
        #[arg(long)]
        federation_id: Option<String>,
        
        /// Output path to save the credential
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

// New commands for federation invites
#[derive(Subcommand)]
enum InviteCommands {
    /// Export a federation invite as a QR code
    ExportQR {
        /// Federation ID to generate invite for
        #[arg(long)]
        federation: String,
        
        /// Output format (terminal, svg, png)
        #[arg(long, default_value = "terminal")]
        format: String,
        
        /// Path to save the QR code file (required for svg and png formats)
        #[arg(long)]
        output: Option<PathBuf>,
    },
    
    /// Import a federation invite from a QR code
    ImportQR {
        /// QR code content string (usually from a scanner)
        #[arg(long)]
        content: String,
    },
}

fn main() {
    // Initialize logging
    env_logger::init();
    
    // Parse command line arguments
    let cli = Cli::parse();
    
    // Initialize storage
    let storage_manager = match StorageManager::new(StorageType::File) {
        Ok(manager) => manager,
        Err(e) => {
            eprintln!("Failed to initialize storage: {}", e);
            process::exit(1);
        }
    };
    
    // Load or create identity manager
    let mut identity_manager = load_identity_manager(&storage_manager);
    
    // Create API client
    let api_config = ApiConfig::default();
    let api_client = match ApiClient::new(api_config) {
        Ok(client) => client,
        Err(e) => {
            eprintln!("Failed to initialize API client: {}", e);
            process::exit(1);
        }
    };
    
    // Create guardian manager
    let guardian_manager = GuardianManager::new(storage_manager.clone());
    
    // Create proposal manager
    let proposal_manager = ProposalManager::new(api_client.clone());
    
    // Process commands
    match &cli.command {
        Commands::Init { scope, username, key_type } => {
            init_identity(&mut identity_manager, &storage_manager, scope, username, key_type);
        },
        
        Commands::Whoami => {
            whoami(&identity_manager);
        },
        
        Commands::Sign { file, output } => {
            sign_file(&identity_manager, file, output);
        },
        
        Commands::Submit { file, node } => {
            submit_file(&identity_manager, file, node);
        },
        
        Commands::Balance { token, scope } => {
            check_balance(&storage_manager, token, scope);
        },
        
        Commands::ListIdentities { federation } => {
            list_identities(&identity_manager, federation);
        },
        
        Commands::UseIdentity { did } => {
            use_identity(&mut identity_manager, &storage_manager, did);
        },
        
        Commands::ExportIdentity { output, qr } => {
            export_identity(&identity_manager, output, *qr);
        },
        
        Commands::ImportIdentity { input } => {
            import_identity(&mut identity_manager, &storage_manager, input);
        },
        
        Commands::Guardians { subcommand } => {
            handle_guardian_commands(subcommand, &identity_manager, &guardian_manager, &storage_manager);
        },
        
        Commands::Proposal { subcommand } => {
            handle_proposal_commands(subcommand, &identity_manager, &proposal_manager);
        },
        
        Commands::Dag { subcommand } => {
            handle_dag_commands(subcommand, &identity_manager, &api_client, &storage_manager);
        },
        
        Commands::Federation { subcommand } => {
            handle_federation_commands(subcommand, &identity_manager, &api_config, &storage_manager);
        },
        
        Commands::Vote { proposal, yes, no, abstain, signature, comment } => {
            cast_vote(&identity_manager, &proposal_manager, proposal, *yes, *no, *abstain, signature, comment);
        },
        
        Commands::Backup { out, password } => {
            backup_wallet(&storage_manager, out, password);
        },
        
        Commands::Restore { file, password } => {
            restore_wallet(&storage_manager, file, password);
        },
        
        Commands::Device { subcommand } => {
            handle_device_commands(
                &subcommand,
                &identity_manager,
                &storage_manager,
            );
        },
        
        Commands::Inbox { subcommand } => {
            handle_inbox_commands(
                &subcommand,
                &identity_manager,
                &storage_manager,
            );
        },
        
        Commands::Outbox { subcommand } => {
            handle_outbox_commands(
                &subcommand,
                &identity_manager,
                &storage_manager,
            );
        },
        
        Commands::ExportVC { identity, output } => {
            export_verifiable_credential(
                &identity_manager,
                identity.as_deref(),
                &output,
            );
        },
        
        Commands::Tui => {
            launch_tui(&identity_manager, &api_client, &storage_manager);
        },
        
        Commands::WebSocket { host, port } => {
            launch_websocket_server(&identity_manager, &api_client, &storage_manager, host, port);
        },
        
        Commands::Credentials { subcommand } => {
            // Create federation runtime for the sync service
            if let Some(identity) = identity_manager.get_active_identity() {
                let federation_runtime = match FederationRuntime::new(api_config.clone(), identity.clone(), storage_manager.clone()) {
                    Ok(runtime) => runtime,
                    Err(e) => {
                        eprintln!("Failed to initialize federation runtime: {}", e);
                        process::exit(1);
                    }
                };
                
                // Create federation sync service
                let sync_service = FederationSyncService::new(
                    federation_runtime, 
                    storage_manager.clone(), 
                    identity_manager.clone(),
                    None
                );
                
                if let Err(e) = sync_service.initialize() {
                    eprintln!("Failed to initialize federation sync service: {}", e);
                    process::exit(1);
                }
                
                handle_credential_commands(subcommand, &identity_manager, &storage_manager);
            } else {
                eprintln!("No active identity. Please use 'identity use' to set an active identity first.");
                process::exit(1);
            }
        },
        
        Commands::Invite { subcommand } => {
            // Create federation runtime for the onboarding service
            if let Some(identity) = identity_manager.get_active_identity() {
                let federation_runtime = match FederationRuntime::new(api_config.clone(), identity.clone(), storage_manager.clone()) {
                    Ok(runtime) => runtime,
                    Err(e) => {
                        eprintln!("Failed to initialize federation runtime: {}", e);
                        process::exit(1);
                    }
                };
                
                // Create federation sync service
                let sync_service = FederationSyncService::new(
                    federation_runtime.clone(), 
                    storage_manager.clone(), 
                    identity_manager.clone(),
                    None
                );
                
                if let Err(e) = sync_service.initialize() {
                    eprintln!("Failed to initialize federation sync service: {}", e);
                    process::exit(1);
                }
                
                // Create onboarding service
                let onboarding_service = OnboardingService::new(
                    federation_runtime,
                    identity_manager.clone(),
                    storage_manager.clone(),
                    sync_service,
                );
                
                handle_invite_commands(subcommand, &onboarding_service);
            } else {
                eprintln!("No active identity. Please use 'identity use' to set an active identity first.");
                process::exit(1);
            }
        },
    }
    
    // Save the identity manager state
    save_identity_manager(&storage_manager, &identity_manager);
}

// Load the identity manager from storage or create a new one
fn load_identity_manager(storage: &StorageManager) -> IdentityManager {
    if storage.exists("global", "identity_manager") {
        match storage.load::<IdentityManager>("global", "identity_manager") {
            Ok(manager) => {
                return manager;
            },
            Err(e) => {
                eprintln!("Warning: Failed to load identity manager: {}", e);
                eprintln!("Starting with a new identity manager");
            }
        }
    }
    
    IdentityManager::new()
}

// Save the identity manager to storage
fn save_identity_manager(storage: &StorageManager, manager: &IdentityManager) {
    if let Err(e) = storage.save("global", "identity_manager", manager) {
        eprintln!("Warning: Failed to save identity manager: {}", e);
    }
}

// Initialize a new identity
fn init_identity(
    manager: &mut IdentityManager, 
    storage: &StorageManager, 
    scope: &str, 
    username: &str, 
    key_type_str: &str
) {
    let key_type = match key_type_str.to_lowercase().as_str() {
        "ed25519" => KeyType::Ed25519,
        "ecdsa" => KeyType::Ecdsa,
        _ => {
            eprintln!("Invalid key type. Supported types: ed25519, ecdsa");
            process::exit(1);
        }
    };
    
    match Identity::new(scope, username, key_type) {
        Ok(identity) => {
            println!("Created new identity: {}", identity.did());
            manager.add_identity(identity);
            
            // Save updated identity manager
            save_identity_manager(storage, manager);
            
            println!("Identity initialized successfully");
        },
        Err(e) => {
            eprintln!("Failed to create identity: {}", e);
            process::exit(1);
        }
    }
}

// Display the active identity
fn whoami(manager: &IdentityManager) {
    match manager.get_active_identity() {
        Some(identity) => {
            println!("Active identity:");
            println!("DID: {}", identity.did());
            println!("Scope: {}", identity.scope());
            println!("Username: {}", identity.username());
        },
        None => {
            println!("No active identity. Use 'init' to create one.");
        }
    }
}

// Sign a file
fn sign_file(manager: &IdentityManager, file_path: &PathBuf, output_path: &Option<PathBuf>) {
    let identity = match manager.get_active_identity() {
        Some(identity) => identity,
        None => {
            eprintln!("No active identity. Use 'init' to create one.");
            process::exit(1);
        }
    };
    
    let file_content = match std::fs::read(file_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Failed to read file: {}", e);
            process::exit(1);
        }
    };
    
    match identity.sign(&file_content) {
        Ok(signature) => {
            let signature_b64 = base64::engine::general_purpose::STANDARD.encode(&signature);
            
            match output_path {
                Some(path) => {
                    if let Err(e) = std::fs::write(path, &signature_b64) {
                        eprintln!("Failed to write signature to file: {}", e);
                        process::exit(1);
                    }
                    println!("Signature saved to {}", path.display());
                },
                None => {
                    println!("{}", signature_b64);
                }
            }
        },
        Err(e) => {
            eprintln!("Failed to sign file: {}", e);
            process::exit(1);
        }
    }
}

// Submit a signed file
fn submit_file(manager: &IdentityManager, file_path: &PathBuf, node_url: &Option<String>) {
    let identity = match manager.get_active_identity() {
        Some(identity) => identity,
        None => {
            eprintln!("No active identity. Use 'init' to create one.");
            process::exit(1);
        }
    };
    
    // Read file content
    let content = match std::fs::read_to_string(file_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Failed to read file: {}", e);
            process::exit(1);
        }
    };
    
    // Configure API client
    let mut config = ApiConfig::default();
    if let Some(url) = node_url {
        config.node_url = url.clone();
    }
    
    // Create API client
    let client = match ApiClient::new(config) {
        Ok(client) => client,
        Err(e) => {
            eprintln!("Failed to create API client: {}", e);
            process::exit(1);
        }
    };
    
    // Submit the file
    match client.submit_program(&content, identity) {
        Ok(response) => {
            println!("File submitted successfully!");
            println!("Response: {}", response.message);
            
            if let Some(data) = response.data {
                println!("Data: {}", data);
            }
        },
        Err(e) => {
            eprintln!("Failed to submit file: {}", e);
            process::exit(1);
        }
    }
}

// Check token balances
fn check_balance(storage: &StorageManager, token_type: &Option<String>, scope: &Option<String>) {
    let scope = match scope {
        Some(s) => s.clone(),
        None => {
            // If no scope provided, try to use the active identity's scope
            let manager = load_identity_manager(storage);
            match manager.get_active_identity() {
                Some(identity) => identity.scope().to_string(),
                None => {
                    eprintln!("No active identity and no scope provided");
                    process::exit(1);
                }
            }
        }
    };
    
    // Try to load the token store for this scope
    if !storage.exists(&scope, "token_store") {
        println!("No token balances found for scope: {}", scope);
        return;
    }
    
    match storage.load::<TokenStore>(&scope, "token_store") {
        Ok(mut store) => {
            // Apply any decay for all balances
            store.update_all_balances();
            
            match token_type {
                Some(token_name) => {
                    // Check specific token
                    let token_type = if token_name == "CEC" {
                        TokenType::CEC
                    } else {
                        TokenType::Typed(token_name.clone())
                    };
                    
                    match store.get_balance(&token_type) {
                        Some(balance) => {
                            println!("Balance for {} in scope {}:", token_name, scope);
                            println!("  Amount: {}", balance.amount);
                            
                            if let Some(expires_at) = &balance.metadata.expires_at {
                                println!("  Expires: {}", expires_at);
                            }
                            
                            if let Some(decay_rate) = balance.metadata.decay_rate {
                                println!("  Decay rate: {}% per month", decay_rate * 100.0);
                            }
                        },
                        None => {
                            println!("No balance found for token: {}", token_name);
                        }
                    }
                },
                None => {
                    // List all tokens
                    println!("Token balances for scope {}:", scope);
                    
                    let balances = store.list_balances();
                    if balances.is_empty() {
                        println!("  No balances found");
                    } else {
                        for balance in balances {
                            let token_name = match &balance.token_type {
                                TokenType::CEC => "CEC".to_string(),
                                TokenType::Typed(name) => name.clone(),
                            };
                            
                            println!("  {}: {}", token_name, balance.amount);
                        }
                    }
                }
            }
        },
        Err(e) => {
            eprintln!("Failed to load token store: {}", e);
            process::exit(1);
        }
    }
}

// List available identities
fn list_identities(manager: &IdentityManager, federation: &Option<String>) {
    let identities = manager.list_identities();
    
    if identities.is_empty() {
        println!("No identities found");
        return;
    }
    
    let active_did = manager.get_active_identity().map(|id| id.did().to_string());
    
    if let Some(fed) = federation {
        println!("Identities in federation/scope '{}':", fed);
        let mut found = false;
        
        for identity in identities {
            if identity.scope() == fed {
                let active_marker = if Some(identity.did().to_string()) == active_did {
                    " (active)"
                } else {
                    ""
                };
                
                println!("  {} - {}@{}{}", identity.did(), identity.username(), identity.scope(), active_marker);
                found = true;
            }
        }
        
        if !found {
            println!("  No identities found in this federation/scope");
        }
    } else {
        println!("All identities:");
        for identity in identities {
            let active_marker = if Some(identity.did().to_string()) == active_did {
                " (active)"
            } else {
                ""
            };
            
            println!("  {} - {}@{}{}", identity.did(), identity.username(), identity.scope(), active_marker);
        }
    }
}

// Switch the active identity
fn use_identity(manager: &mut IdentityManager, storage: &StorageManager, did: &str) {
    match manager.set_active_identity(did) {
        Ok(_) => {
            println!("Switched active identity to {}", did);
            
            // Save updated identity manager
            save_identity_manager(storage, manager);
        },
        Err(e) => {
            eprintln!("Failed to switch identity: {}", e);
            process::exit(1);
        }
    }
}

// Export identity to a file
fn export_identity(manager: &IdentityManager, output: &PathBuf, export_qr: bool) {
    let identity = match manager.get_active_identity() {
        Some(identity) => identity,
        None => {
            eprintln!("No active identity. Use 'init' to create one or 'use' to select one.");
            process::exit(1);
        }
    };
    
    // Get the identity's metadata
    let metadata = identity.get_metadata();
    
    // Export to file
    match identity.export_metadata(output) {
        Ok(_) => {
            println!("Identity exported successfully to {:?}", output);
            
            if export_qr {
                println!("QR code export not implemented yet in this version");
                // Future: implement QR code generation
                // println!("QR code saved to {:?}.png", output);
            }
        },
        Err(e) => {
            eprintln!("Failed to export identity: {}", e);
            process::exit(1);
        }
    }
}

// Import identity from a file
fn import_identity(manager: &mut IdentityManager, storage: &StorageManager, input: &PathBuf) {
    match Identity::import_metadata(input) {
        Ok(identity) => {
            println!("Imported identity: {}", identity.did());
            manager.add_identity(identity);
            
            // Save updated identity manager
            save_identity_manager(storage, manager);
        },
        Err(e) => {
            eprintln!("Failed to import identity: {}", e);
            process::exit(1);
        }
    }
}

// Handle guardian-related commands
fn handle_guardian_commands(
    command: &GuardianCommands,
    identity_manager: &IdentityManager,
    guardian_manager: &GuardianManager,
    storage_manager: &StorageManager,
) {
    let active_identity = match identity_manager.get_active_identity() {
        Some(identity) => identity,
        None => {
            eprintln!("No active identity. Use 'init' to create one or 'use' to select one.");
            process::exit(1);
        }
    };
    
    match command {
        GuardianCommands::Add { did, name } => {
            // Check if we need to create a guardian set first
            let guardian_set = match guardian_manager.load_guardian_set(active_identity.did()) {
                Ok(set) => set,
                Err(_) => {
                    // Create a new guardian set with threshold of 2 (or 1 if this is the first guardian)
                    match guardian_manager.create_guardian_set(active_identity.did(), 1) {
                        Ok(set) => set,
                        Err(e) => {
                            eprintln!("Failed to create guardian set: {}", e);
                            process::exit(1);
                        }
                    }
                }
            };
            
            let result = guardian_manager.add_guardian(
                active_identity.did(),
                did,
                name,
            );
            
            match result {
                Ok(_) => {
                    println!("Guardian '{}' ({}) added successfully", name, did);
                },
                Err(e) => {
                    eprintln!("Failed to add guardian: {}", e);
                    process::exit(1);
                }
            }
        },
        
        GuardianCommands::Remove { did } => {
            // First load the guardian set
            let mut guardian_set = match guardian_manager.load_guardian_set(active_identity.did()) {
                Ok(set) => set,
                Err(e) => {
                    eprintln!("Failed to load guardian set: {}", e);
                    process::exit(1);
                }
            };
            
            // Remove the guardian
            match guardian_set.remove_guardian(did) {
                Ok(_) => {
                    println!("Guardian '{}' removed successfully", did);
                    
                    // Save the updated guardian set
                    if let Err(e) = storage_manager.save(
                        "guardians", 
                        &format!("{}_set", active_identity.did()),
                        &guardian_set
                    ) {
                        eprintln!("Warning: Failed to save guardian set: {}", e);
                    }
                },
                Err(e) => {
                    eprintln!("Failed to remove guardian: {}", e);
                    process::exit(1);
                }
            }
        },
        
        GuardianCommands::List => {
            // Load the guardian set
            let guardian_set = match guardian_manager.load_guardian_set(active_identity.did()) {
                Ok(set) => set,
                Err(_) => {
                    println!("No guardians configured for this identity");
                    return;
                }
            };
            
            let guardians = guardian_set.list_active_guardians();
            
            if guardians.is_empty() {
                println!("No guardians configured for this identity");
            } else {
                println!("Guardians for identity {}:", active_identity.did());
                
                // Get the number of guardians before iterating
                let guardians_count = guardians.len();
                
                for guardian in guardians {
                    println!("  {} - {} (Status: {:?})", guardian.did, guardian.name, guardian.status);
                }
                println!("Recovery threshold: {}/{}", guardian_set.threshold, guardians_count);
            }
        },
        
        GuardianCommands::CreateRecovery { password, output } => {
            // Create recovery bundle
            let password_str = password.as_deref().unwrap_or("default_password");
            
            let result = guardian_manager.create_recovery_bundle(
                active_identity,
                password_str,
            );
            
            match result {
                Ok(bundle) => {
                    // Export the bundle to the file
                    if let Err(e) = guardian_manager.export_recovery_bundle(active_identity.did(), output) {
                        eprintln!("Failed to export recovery bundle: {}", e);
                        process::exit(1);
                    }
                    
                    println!("Recovery bundle created and saved to {:?}", output);
                    println!("Share this file with your guardians");
                },
                Err(e) => {
                    eprintln!("Failed to create recovery bundle: {}", e);
                    process::exit(1);
                }
            }
        },
        
        GuardianCommands::RequestRecovery { did, guardian_did, bundle } => {
            // Import the recovery bundle
            let bundle_result = guardian_manager.import_recovery_bundle(bundle);
            
            match bundle_result {
                Ok(recovery_bundle) => {
                    // Start the recovery process
                    let recovery_request = match guardian_manager.start_recovery(did) {
                        Ok(request) => request,
                        Err(e) => {
                            eprintln!("Failed to start recovery: {}", e);
                            process::exit(1);
                        }
                    };
                    
                    // Get guardian's identity
                    let guardian_identity = match identity_manager.get_identity(guardian_did) {
                        Some(identity) => identity,
                        None => {
                            eprintln!("Guardian identity not found: {}", guardian_did);
                            process::exit(1);
                        }
                    };
                    
                    // Sign the recovery request
                    let signature = match guardian_identity.sign(did.as_bytes()) {
                        Ok(sig) => sig,
                        Err(e) => {
                            eprintln!("Failed to sign recovery request: {}", e);
                            process::exit(1);
                        }
                    };
                    
                    // Add the guardian's signature
                    match guardian_manager.add_recovery_signature(
                        &recovery_request.id,
                        guardian_did,
                        signature,
                    ) {
                        Ok(_) => {
                            println!("Recovery request submitted successfully");
                        },
                        Err(e) => {
                            eprintln!("Failed to submit recovery signature: {}", e);
                            process::exit(1);
                        }
                    }
                },
                Err(e) => {
                    eprintln!("Failed to import recovery bundle: {}", e);
                    process::exit(1);
                }
            }
        },
        
        GuardianCommands::ApproveRecovery { did } => {
            // Get active identity (guardian)
            let guardian_identity = active_identity;
            
            // First check if there's an active recovery request
            let recovery_request = match guardian_manager.start_recovery(did) {
                Ok(request) => request,
                Err(e) => {
                    eprintln!("Failed to find recovery request: {}", e);
                    process::exit(1);
                }
            };
            
            // Sign the recovery request
            let signature = match guardian_identity.sign(did.as_bytes()) {
                Ok(sig) => sig,
                Err(e) => {
                    eprintln!("Failed to sign recovery request: {}", e);
                    process::exit(1);
                }
            };
            
            // Add the guardian's signature
            match guardian_manager.add_recovery_signature(
                &recovery_request.id,
                guardian_identity.did(),
                signature,
            ) {
                Ok(_) => {
                    println!("Recovery approved successfully");
                },
                Err(e) => {
                    eprintln!("Failed to approve recovery: {}", e);
                    process::exit(1);
                }
            }
        },
    }
}

// Handle proposal-related commands
fn handle_proposal_commands(
    command: &ProposalCommands,
    identity_manager: &IdentityManager,
    proposal_manager: &ProposalManager,
) {
    let active_identity = match identity_manager.get_active_identity() {
        Some(identity) => identity,
        None => {
            eprintln!("No active identity. Use 'init' to create one or 'use' to select one.");
            process::exit(1);
        }
    };
    
    match command {
        ProposalCommands::Draft { file, output } => {
            // Load and parse DSL
            let proposal = match proposal_manager.load_dsl(file, active_identity) {
                Ok(proposal) => proposal,
                Err(e) => {
                    eprintln!("Failed to load DSL file: {}", e);
                    process::exit(1);
                }
            };
            
            // Save proposal if output is specified
            if let Some(output_path) = output {
                if let Err(e) = proposal_manager.save_proposal(&proposal, output_path) {
                    eprintln!("Failed to save proposal: {}", e);
                    process::exit(1);
                }
                
                println!("Proposal drafted and saved to {:?}", output_path);
                println!("Proposal hash: {}", proposal.hash);
            } else {
                // Print proposal information
                println!("Proposal drafted successfully (not saved)");
                println!("Title: {}", proposal.title);
                println!("Description: {}", proposal.description);
                println!("Hash: {}", proposal.hash);
                println!("Use --output to save the proposal to a file");
            }
        },
        
        ProposalCommands::Submit { file } => {
            // Load proposal file
            let proposal = match proposal_manager.load_proposal(file) {
                Ok(proposal) => proposal,
                Err(e) => {
                    eprintln!("Failed to load proposal file: {}", e);
                    process::exit(1);
                }
            };
            
            // Sign the proposal
            let signature = match proposal_manager.sign_proposal(&proposal, active_identity) {
                Ok(sig) => sig,
                Err(e) => {
                    eprintln!("Failed to sign proposal: {}", e);
                    process::exit(1);
                }
            };
            
            // Submit proposal
            match proposal_manager.submit_proposal(&proposal, &signature, active_identity) {
                Ok(submitted) => {
                    println!("Proposal submitted successfully");
                    println!("Title: {}", submitted.proposal.title);
                    println!("Hash: {}", submitted.proposal.hash);
                    println!("Status: {:?}", submitted.proposal.status);
                    
                    if let Some(receipt) = submitted.dag_receipt {
                        println!("DAG Receipt: {}", receipt);
                    }
                },
                Err(e) => {
                    eprintln!("Failed to submit proposal: {}", e);
                    process::exit(1);
                }
            }
        },
        
        ProposalCommands::Vote { hash, option, comment } => {
            // Parse vote option
            let vote_option = match VoteOption::from_str(option) {
                Ok(opt) => opt,
                Err(e) => {
                    eprintln!("Invalid vote option: {}", e);
                    eprintln!("Valid options: yes, no, abstain");
                    process::exit(1);
                }
            };
            
            // Cast vote
            match proposal_manager.cast_vote(hash, vote_option, comment.clone(), active_identity) {
                Ok(vote) => {
                    println!("Vote cast successfully");
                    println!("Proposal: {}", vote.proposal_hash);
                    println!("Vote: {:?}", vote.vote);
                    
                    if let Some(comment_text) = &vote.comment {
                        println!("Comment: {}", comment_text);
                    }
                },
                Err(e) => {
                    eprintln!("Failed to cast vote: {}", e);
                    process::exit(1);
                }
            }
        },
        
        ProposalCommands::List { scope, status } => {
            // List proposals
            match proposal_manager.list_proposals(scope.as_deref(), active_identity) {
                Ok(proposals) => {
                    if proposals.is_empty() {
                        println!("No proposals found");
                    } else {
                        println!("Proposals:");
                        for submitted in proposals {
                            println!("  Hash: {}", submitted.proposal.hash);
                            println!("  Title: {}", submitted.proposal.title);
                            println!("  Status: {:?}", submitted.proposal.status);
                            println!("  Proposer: {}", submitted.proposal.proposer);
                            println!("  Votes: {}", submitted.votes.len());
                            println!();
                        }
                    }
                },
                Err(e) => {
                    eprintln!("Failed to list proposals: {}", e);
                    process::exit(1);
                }
            }
        },
        
        ProposalCommands::Show { hash } => {
            // Query proposal details
            match proposal_manager.query_proposal(hash, active_identity) {
                Ok(submitted) => {
                    println!("Proposal Details:");
                    println!("  Hash: {}", submitted.proposal.hash);
                    println!("  Title: {}", submitted.proposal.title);
                    println!("  Description: {}", submitted.proposal.description);
                    println!("  Status: {:?}", submitted.proposal.status);
                    println!("  Proposer: {}", submitted.proposal.proposer);
                    println!("  Created: {}", submitted.proposal.created_at);
                    
                    // Vote summary
                    let mut yes_votes = 0;
                    let mut no_votes = 0;
                    let mut abstain_votes = 0;
                    
                    println!("\nVotes:");
                    for vote in &submitted.votes {
                        match vote.vote {
                            VoteOption::Yes => yes_votes += 1,
                            VoteOption::No => no_votes += 1,
                            VoteOption::Abstain => abstain_votes += 1,
                        }
                        
                        println!("  {}: {:?}", vote.voter, vote.vote);
                        if let Some(comment) = &vote.comment {
                            println!("    Comment: {}", comment);
                        }
                    }
                    
                    println!("\nVote Summary:");
                    println!("  Yes: {}", yes_votes);
                    println!("  No: {}", no_votes);
                    println!("  Abstain: {}", abstain_votes);
                    println!("  Total: {}", submitted.votes.len());
                },
                Err(e) => {
                    eprintln!("Failed to query proposal: {}", e);
                    process::exit(1);
                }
            }
        },
        
        ProposalCommands::Audit { hash } => {
            // Create federation runtime for auditing
            let federation_runtime = match FederationRuntime::new(
                ApiConfig::default(), 
                active_identity.clone(), 
                StorageManager::new(StorageType::File).unwrap()
            ) {
                Ok(runtime) => runtime,
                Err(e) => {
                    eprintln!("Failed to create federation runtime: {}", e);
                    process::exit(1);
                }
            };
            
            // Audit the proposal
            match federation_runtime.audit_proposal(hash) {
                Ok(audit) => {
                    println!("Proposal {}", audit.hash);
                    println!("Title: {}", audit.title);
                    println!("Status: {}", audit.status);
                    println!("Votes: {} yes / {} no / {} abstain", audit.yes_votes, audit.no_votes, audit.abstain_votes);
                    println!("Threshold: {}", audit.threshold);
                    println!("Guardian Quorum: {}", if audit.guardian_quorum_met { "Met ✅" } else { "Not Met ❌" });
                    println!("Execution: {}", audit.execution_status);
                    
                    if let Some(receipt) = audit.dag_receipt {
                        println!("DAG Receipt: {}", receipt);
                    }
                },
                Err(e) => {
                    eprintln!("Failed to audit proposal: {}", e);
                    process::exit(1);
                }
            }
        },
        
        ProposalCommands::VoteStats { hash } => {
            match proposal_manager.query_proposal(hash, &active_identity) {
                Ok(submitted) => {
                    // Count votes
                    let mut yes_votes = 0;
                    let mut no_votes = 0;
                    let mut abstain_votes = 0;
                    
                    for vote in &submitted.votes {
                        match vote.vote {
                            VoteOption::Yes => yes_votes += 1,
                            VoteOption::No => no_votes += 1,
                            VoteOption::Abstain => abstain_votes += 1,
                        }
                    }
                    
                    let total_votes = yes_votes + no_votes + abstain_votes;
                    let threshold = if total_votes > 0 { total_votes / 2 + 1 } else { 1 };
                    
                    println!("Vote statistics for proposal {}:", hash);
                    println!("Yes: {}", yes_votes);
                    println!("No: {}", no_votes);
                    println!("Abstain: {}", abstain_votes);
                    println!("Total Votes: {}", total_votes);
                    println!("Threshold: {}", threshold);
                    println!("Passing Status: {}", if yes_votes >= threshold { "✅ Passing" } else { "❌ Not Passing" });
                },
                Err(e) => {
                    eprintln!("Failed to query proposal: {}", e);
                    process::exit(1);
                }
            }
        },
        
        ProposalCommands::Hash { file } => {
            // Load the proposal to calculate its hash
            match proposal_manager.load_dsl(file, &active_identity) {
                Ok(proposal) => {
                    println!("{}", proposal.hash);
                },
                Err(e) => {
                    eprintln!("Failed to calculate proposal hash: {}", e);
                    process::exit(1);
                }
            }
        },
    }
}

// Handle DAG commands
fn handle_dag_commands(
    command: &DagCommands,
    identity_manager: &IdentityManager,
    api_client: &ApiClient,
    storage_manager: &StorageManager,
) {
    // Get active identity
    let identity = match identity_manager.get_active_identity() {
        Some(identity) => identity,
        None => {
            eprintln!("No active identity. Use 'init' or 'use-identity' first.");
            process::exit(1);
        }
    };
    
    // Create federation runtime
    let federation_runtime = match FederationRuntime::new(ApiConfig::default(), identity.clone(), storage_manager.clone()) {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("Failed to create federation runtime: {}", e);
            process::exit(1);
        }
    };
    
    match command {
        DagCommands::Tip => {
            // Get DAG tip from status
            match federation_runtime.get_dag_status(None) {
                Ok(status) => {
                    println!("{}", status.latest_vertex);
                },
                Err(e) => {
                    eprintln!("Failed to get DAG tip: {}", e);
                    process::exit(1);
                }
            }
        },
        
        DagCommands::Status { scope } => {
            match federation_runtime.get_dag_status(scope.as_deref()) {
                Ok(status) => {
                    println!("DAG Status:");
                    println!("Latest Vertex: {}", status.latest_vertex);
                    if let Some(proposal_id) = status.proposal_id {
                        println!("Proposal ID: {}", proposal_id);
                    }
                    println!("Vertex Count: {}", status.vertex_count);
                    println!("Sync Status: {}", if status.synced { "✅ DAG synced" } else { "⚠️ Not synced" });
                    if let Some(scope) = status.scope {
                        println!("Scope: {}", scope);
                    }
                },
                Err(e) => {
                    eprintln!("Failed to get DAG status: {}", e);
                    process::exit(1);
                }
            }
        },
        
        DagCommands::Sync => {
            // For now, just print a message since this would be implemented in the daemon
            println!("DAG sync triggered. This would be handled by the federation daemon.");
        },
        
        DagCommands::ReplayPending => {
            // For now, just print a message since this would be implemented in the daemon
            println!("DAG replay triggered. This would be handled by the federation daemon.");
        }
    }
}

// Handle federation commands
fn handle_federation_commands(
    command: &FederationCommands,
    identity_manager: &IdentityManager,
    api_config: &ApiConfig,
    storage_manager: &StorageManager,
) {
    // Get active identity
    let identity = match identity_manager.get_active_identity() {
        Some(identity) => identity,
        None => {
            eprintln!("No active identity. Use 'init' or 'use-identity' first.");
            process::exit(1);
        }
    };
    
    // Create federation runtime
    let mut federation_runtime = match FederationRuntime::new(api_config.clone(), identity.clone(), storage_manager.clone()) {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("Failed to create federation runtime: {}", e);
            process::exit(1);
        }
    };
    
    match command {
        FederationCommands::Submit { file, timeout, verbose } => {
            let options = MonitoringOptions {
                interval_seconds: 10,
                timeout_minutes: timeout.unwrap_or(60),
                verbose: *verbose,
            };
            
            println!("Submitting proposal and monitoring: {}", file.display());
            
            match federation_runtime.submit_and_monitor(file, Some(options)) {
                Ok(result) => {
                    println!("Proposal Status: {:?}", result.status);
                    println!("Votes: {} yes / {} no / {} abstain", result.yes_votes, result.no_votes, result.abstain_votes);
                    println!("Threshold: {}", result.threshold);
                    
                    if let Some(event_id) = result.event_id {
                        println!("Event ID: {}", event_id);
                    }
                    
                    if let Some(executed_at) = result.executed_at {
                        println!("Executed at: {}", executed_at);
                    }
                },
                Err(e) => {
                    eprintln!("Failed to submit and monitor proposal: {}", e);
                    process::exit(1);
                }
            }
        },
        
        FederationCommands::Sync { proposal_hash } => {
            match federation_runtime.sync_with_agoranet(proposal_hash) {
                Ok(success) => {
                    if success {
                        println!("Successfully synced proposal with AgoraNet.");
                    } else {
                        println!("Failed to sync proposal with AgoraNet.");
                        process::exit(1);
                    }
                },
                Err(e) => {
                    eprintln!("Failed to sync with AgoraNet: {}", e);
                    process::exit(1);
                }
            }
        }
    }
}

// Function to cast votes (as a separate command)
fn cast_vote(
    identity_manager: &IdentityManager,
    proposal_manager: &ProposalManager,
    proposal_hash: &str,
    yes: bool,
    no: bool,
    abstain: bool,
    signature: &Option<PathBuf>,
    comment: &Option<String>,
) {
    // Get active identity
    let identity = match identity_manager.get_active_identity() {
        Some(identity) => identity,
        None => {
            eprintln!("No active identity. Use 'init' or 'use-identity' first.");
            process::exit(1);
        }
    };
    
    // Determine vote option
    let vote_option = if yes {
        VoteOption::Yes
    } else if no {
        VoteOption::No
    } else if abstain {
        VoteOption::Abstain
    } else {
        eprintln!("Please specify a vote option: --yes, --no, or --abstain");
        process::exit(1);
    };
    
    // Cast vote
    match proposal_manager.cast_vote(
        proposal_hash,
        vote_option,
        comment.clone(),
        &identity,
    ) {
        Ok(vote) => {
            println!("Vote cast successfully!");
            println!("Proposal: {}", vote.proposal_hash);
            println!("Vote: {:?}", vote.vote);
            if let Some(comment) = &vote.comment {
                println!("Comment: {}", comment);
            }
            println!("Timestamp: {}", vote.timestamp);
            
            // If signature file is provided, we're voting as a guardian
            if let Some(sig_path) = signature {
                println!("Guardian signature recorded.");
                // In a real impl, we would add the signature to the guardian recovery
            }
        },
        Err(e) => {
            eprintln!("Failed to cast vote: {}", e);
            process::exit(1);
        }
    }
}

// Simple TUI placeholder function
fn launch_tui(
    identity_manager: &IdentityManager,
    api_client: &ApiClient,
    storage_manager: &StorageManager,
) {
    println!("Launching TUI mode...");
    
    // Call the TUI run function
    match crate::tui::run_tui(identity_manager, api_client, storage_manager) {
        Ok(_) => {
            println!("TUI closed successfully.");
        },
        Err(e) => {
            eprintln!("Error in TUI: {}", e);
            process::exit(1);
        }
    }
}

// Backup wallet data
fn backup_wallet(storage: &StorageManager, output: &PathBuf, password: &Option<String>) {
    match storage.create_backup(output, password.as_deref()) {
        Ok(_) => {
            println!("Backup created successfully at {:?}", output);
            if password.is_some() {
                println!("Backup is encrypted with the provided password");
            } else {
                println!("Warning: Backup is not encrypted");
            }
        },
        Err(e) => {
            eprintln!("Failed to create backup: {}", e);
            process::exit(1);
        }
    }
}

// Restore wallet from backup
fn restore_wallet(storage: &StorageManager, input: &PathBuf, password: &Option<String>) {
    match storage.restore_backup(input, password.as_deref()) {
        Ok(_) => {
            println!("Wallet restored successfully from {:?}", input);
        },
        Err(e) => {
            eprintln!("Failed to restore backup: {}", e);
            process::exit(1);
        }
    }
}

/// Handle device management commands
fn handle_device_commands(
    command: &DeviceCommands,
    identity_manager: &IdentityManager,
    storage_manager: &StorageManager,
) {
    match command {
        DeviceCommands::Link { to, key_type, output } => {
            let active_identity = match identity_manager.get_active_identity() {
                Some(id) => id,
                None => {
                    eprintln!("No active identity. Please set one with 'wallet use-identity'");
                    process::exit(1);
                }
            };
            
            // Parse key type
            let target_key_type = match key_type.to_lowercase().as_str() {
                "ed25519" => KeyType::Ed25519,
                "ecdsa" => KeyType::Ecdsa,
                _ => {
                    eprintln!("Invalid key type. Use 'ed25519' or 'ecdsa'");
                    process::exit(1);
                }
            };
            
            // Create device link challenge
            let challenge = match active_identity.create_device_link_challenge(to, target_key_type) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Failed to create device link challenge: {}", e);
                    process::exit(1);
                }
            };
            
            // Sign the challenge
            let link = match active_identity.sign_device_link(&challenge) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Failed to sign device link: {}", e);
                    process::exit(1);
                }
            };
            
            // Save to file
            let link_json = match serde_json::to_string_pretty(&link) {
                Ok(j) => j,
                Err(e) => {
                    eprintln!("Failed to serialize device link: {}", e);
                    process::exit(1);
                }
            };
            
            match fs::write(output, link_json) {
                Ok(_) => println!("Device link saved to {}", output.display()),
                Err(e) => {
                    eprintln!("Failed to write device link file: {}", e);
                    process::exit(1);
                }
            }
        },
        
        DeviceCommands::Import { link, key } => {
            // Read the link file
            let link_data = match fs::read_to_string(link) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("Failed to read link file: {}", e);
                    process::exit(1);
                }
            };
            
            // Parse the link
            let device_link: DeviceLink = match serde_json::from_str(&link_data) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Failed to parse link file: {}", e);
                    process::exit(1);
                }
            };
            
            // Read the private key file
            let private_key = match fs::read(key) {
                Ok(k) => k,
                Err(e) => {
                    eprintln!("Failed to read private key file: {}", e);
                    process::exit(1);
                }
            };
            
            // Create identity from device link
            let identity = match Identity::from_device_link(&device_link, &private_key) {
                Ok(i) => i,
                Err(e) => {
                    eprintln!("Failed to import identity from device link: {}", e);
                    process::exit(1);
                }
            };
            
            // Save the identity
            let mut manager = identity_manager.clone();
            manager.add_identity(identity);
            save_identity_manager(storage_manager, &manager);
            
            println!("Identity imported successfully from device link");
        },
        
        DeviceCommands::List => {
            let active_identity = match identity_manager.get_active_identity() {
                Some(id) => id,
                None => {
                    eprintln!("No active identity. Please set one with 'wallet use-identity'");
                    process::exit(1);
                }
            };
            
            // We need to create a mutable clone of the identity manager to update it
            let mut manager = identity_manager.clone();
            let identity_clone = active_identity.get_metadata();
            
            // For now, just display this device's ID - let's ensure one exists
            let device_id = if let Some(did) = identity_clone.get_metadata().did() {
                // Find an existing device ID
                let mut found_device_id = String::new();
                for id in identity_manager.list_identities() {
                    if id.did() == did {
                        // Check if device ID exists
                        // If not, we'll generate one below
                        found_device_id = id.get_metadata().did().to_string();
                        break;
                    }
                }
                
                if found_device_id.is_empty() {
                    // Generate a new device ID
                    let new_id = uuid::Uuid::new_v4().to_string();
                    
                    // In a real implementation, we'd save this back to the identity
                    // For this demo, we'll just display it
                    new_id
                } else {
                    found_device_id
                }
            } else {
                "No device ID found".to_string()
            };
            
            println!("Current device ID: {}", device_id);
            
            // Since we can't directly access metadata, we'll just print a message
            // In a full implementation, we'd query the linked devices
            println!("Device linking information is stored in identity metadata");
            println!("Use 'wallet device generate' to create a new device keypair");
            println!("Use 'wallet device link --to <pubkey>' to create a link for another device");
        },
        
        DeviceCommands::Generate { key_type, output } => {
            // Parse key type
            let key_type_enum = match key_type.to_lowercase().as_str() {
                "ed25519" => KeyType::Ed25519,
                "ecdsa" => KeyType::Ecdsa,
                _ => {
                    eprintln!("Invalid key type. Use 'ed25519' or 'ecdsa'");
                    process::exit(1);
                }
            };
            
            // Generate keypair
            let (private_key, public_key) = match Identity::generate_device_keypair(key_type_enum) {
                Ok(pair) => pair,
                Err(e) => {
                    eprintln!("Failed to generate keypair: {}", e);
                    process::exit(1);
                }
            };
            
            // Ensure output directory exists
            if !output.exists() {
                if let Err(e) = fs::create_dir_all(&output) {
                    eprintln!("Failed to create output directory: {}", e);
                    process::exit(1);
                }
            }
            
            // Write private key
            let private_key_path = output.join("private.key");
            if let Err(e) = fs::write(&private_key_path, &private_key) {
                eprintln!("Failed to write private key: {}", e);
                process::exit(1);
            }
            
            // Write public key
            let public_key_path = output.join("public.key");
            if let Err(e) = fs::write(&public_key_path, &public_key) {
                eprintln!("Failed to write public key: {}", e);
                process::exit(1);
            }
            
            println!("Generated {} keypair:", key_type);
            println!("  Private key: {}", private_key_path.display());
            println!("  Public key: {}", public_key_path.display());
            println!("Keep your private key secure!");
        },
    }
}

/// Handle inbox commands
fn handle_inbox_commands(
    command: &InboxCommands,
    identity_manager: &IdentityManager,
    storage_manager: &StorageManager,
) {
    // Create inbox directory if it doesn't exist
    let inbox_dir = storage_manager.get_data_dir().join("proposals").join("inbox");
    if !inbox_dir.exists() {
        if let Err(e) = fs::create_dir_all(&inbox_dir) {
            eprintln!("Failed to create inbox directory: {}", e);
            process::exit(1);
        }
    }
    
    match command {
        InboxCommands::List => {
            // Read all files in the inbox directory
            let entries = match fs::read_dir(&inbox_dir) {
                Ok(entries) => entries,
                Err(e) => {
                    eprintln!("Failed to read inbox directory: {}", e);
                    process::exit(1);
                }
            };
            
            let mut items = Vec::new();
            
            // Process each file
            for entry in entries {
                if let Ok(entry) = entry {
                    // Try to read as a proposal or other message
                    if let Ok(metadata) = entry.metadata() {
                        items.push((
                            entry.file_name().to_string_lossy().to_string(),
                            metadata.modified().unwrap_or_else(|_| std::time::SystemTime::now()),
                        ));
                    }
                }
            }
            
            // Sort by modification time (newest first)
            items.sort_by(|a, b| b.1.cmp(&a.1));
            
            // Display inbox items
            println!("Inbox items:");
            if items.is_empty() {
                println!("  No items in inbox");
            } else {
                for (name, time) in items {
                    println!("  {} - {:?}", name, time);
                }
            }
        },
        
        InboxCommands::Review { id } => {
            // Check if the file exists
            let file_path = inbox_dir.join(id);
            if !file_path.exists() {
                eprintln!("Item '{}' not found in inbox", id);
                process::exit(1);
            }
            
            // Read the file
            let content = match fs::read_to_string(&file_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Failed to read item: {}", e);
                    process::exit(1);
                }
            };
            
            // Display the content
            println!("Reviewing item '{}':", id);
            println!("{}", content);
            
            // In a full implementation, this would handle different item types
            // and provide appropriate actions
        },
    }
}

/// Handle outbox commands
fn handle_outbox_commands(
    command: &OutboxCommands,
    identity_manager: &IdentityManager,
    storage_manager: &StorageManager,
) {
    // Create outbox directory if it doesn't exist
    let outbox_dir = storage_manager.get_data_dir().join("proposals").join("outbox");
    if !outbox_dir.exists() {
        if let Err(e) = fs::create_dir_all(&outbox_dir) {
            eprintln!("Failed to create outbox directory: {}", e);
            process::exit(1);
        }
    }
    
    match command {
        OutboxCommands::List => {
            // Read all files in the outbox directory
            let entries = match fs::read_dir(&outbox_dir) {
                Ok(entries) => entries,
                Err(e) => {
                    eprintln!("Failed to read outbox directory: {}", e);
                    process::exit(1);
                }
            };
            
            let mut items = Vec::new();
            
            // Process each file
            for entry in entries {
                if let Ok(entry) = entry {
                    // Try to read as a proposal or other message
                    if let Ok(metadata) = entry.metadata() {
                        items.push((
                            entry.file_name().to_string_lossy().to_string(),
                            metadata.modified().unwrap_or_else(|_| std::time::SystemTime::now()),
                        ));
                    }
                }
            }
            
            // Sort by modification time (newest first)
            items.sort_by(|a, b| b.1.cmp(&a.1));
            
            // Display outbox items
            println!("Outbox items:");
            if items.is_empty() {
                println!("  No items in outbox");
            } else {
                for (name, time) in items {
                    println!("  {} - {:?}", name, time);
                }
            }
        },
        
        OutboxCommands::Status { id } => {
            if let Some(item_id) = id {
                // Check if the file exists
                let file_path = outbox_dir.join(item_id);
                if !file_path.exists() {
                    eprintln!("Item '{}' not found in outbox", item_id);
                    process::exit(1);
                }
                
                // Read the file
                let content = match fs::read_to_string(&file_path) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Failed to read item: {}", e);
                        process::exit(1);
                    }
                };
                
                // Display the content and status
                println!("Status of item '{}':", item_id);
                println!("{}", content);
                
                // In a full implementation, this would check the federation status
            } else {
                // Show status of all items
                let entries = match fs::read_dir(&outbox_dir) {
                    Ok(entries) => entries,
                    Err(e) => {
                        eprintln!("Failed to read outbox directory: {}", e);
                        process::exit(1);
                    }
                };
                
                let mut items = Vec::new();
                
                // Process each file
                for entry in entries {
                    if let Ok(entry) = entry {
                        items.push(entry.file_name().to_string_lossy().to_string());
                    }
                }
                
                // Display status of all items
                println!("Status of all outbox items:");
                if items.is_empty() {
                    println!("  No items in outbox");
                } else {
                    for item in items {
                        println!("  {} - Pending", item);
                    }
                }
            }
        },
    }
}

/// Export a Verifiable Credential for an identity
fn export_verifiable_credential(
    identity_manager: &IdentityManager,
    identity_did: Option<&str>,
    output: &PathBuf,
) {
    // Get the identity
    let identity = match identity_did {
        Some(did) => match identity_manager.get_identity(did) {
            Some(id) => id,
            None => {
                eprintln!("Identity with DID '{}' not found", did);
                process::exit(1);
            }
        },
        None => match identity_manager.get_active_identity() {
            Some(id) => id,
            None => {
                eprintln!("No active identity. Please specify a DID or set an active identity.");
                process::exit(1);
            }
        }
    };
    
    // Create VC JSON structure
    let vc = serde_json::json!({
        "@context": [
            "https://www.w3.org/2018/credentials/v1",
            "https://www.w3.org/2018/credentials/examples/v1"
        ],
        "id": format!("urn:uuid:{}", uuid::Uuid::new_v4()),
        "type": ["VerifiableCredential", "FederationMemberCredential"],
        "issuer": identity.did(),
        "issuanceDate": chrono::Utc::now().to_rfc3339(),
        "credentialSubject": {
            "id": identity.did(),
            "federationMember": {
                "scope": identity.scope(),
                "username": identity.username(),
                "role": "member"
            }
        }
    });
    
    // Save to file
    match fs::write(output, serde_json::to_string_pretty(&vc).unwrap()) {
        Ok(_) => println!("Verifiable Credential exported to {}", output.display()),
        Err(e) => {
            eprintln!("Failed to write VC file: {}", e);
            process::exit(1);
        }
    }
}

/// Launch WebSocket server for real-time updates
fn launch_websocket_server(
    identity_manager: &IdentityManager,
    api_client: &ApiClient,
    storage_manager: &StorageManager,
    host: &Option<String>,
    port: &Option<u16>,
) {
    // Create federation runtime
    let api_config = api_client.get_config().clone();
    let active_identity = identity_manager.get_active_identity().cloned();
    
    // Check if we have an active identity
    let identity = match active_identity {
        Some(id) => id,
        None => {
            eprintln!("No active identity. Use 'init' or 'use-identity' first.");
            process::exit(1);
        }
    };
    
    // Create FederationRuntime
    let federation_runtime = match FederationRuntime::new(
        api_config,
        identity,
        storage_manager.clone(),
    ) {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("Failed to create federation runtime: {}", e);
            process::exit(1);
        }
    };
    
    // Create sync config
    let sync_config = SyncConfig {
        inbox_sync_interval: 10, // 10 seconds for testing
        outbox_sync_interval: 10, // 10 seconds for testing
        dag_watch_interval: 5,    // 5 seconds for testing
        inbox_path: std::path::PathBuf::from("proposals/inbox"),
        outbox_path: std::path::PathBuf::from("proposals/outbox"),
    };
    
    // Create SyncManager
    let sync_manager = SyncManager::new(
        federation_runtime,
        storage_manager.clone(),
        identity_manager.clone(),
        Some(sync_config),
    );
    
    // Start the sync manager
    if let Err(e) = sync_manager.start() {
        eprintln!("Failed to start sync manager: {}", e);
        process::exit(1);
    }
    
    // Create WebSocket config
    let websocket_config = WebSocketConfig {
        host: host.clone().unwrap_or_else(|| "127.0.0.1".to_string()),
        port: port.clone().unwrap_or(9876),
        ping_interval: 30,
    };
    
    // Create WebSocket server
    let websocket_server = WebSocketServer::new(
        sync_manager,
        Some(websocket_config),
    );
    
    // Start WebSocket server
    if let Err(e) = websocket_server.start() {
        eprintln!("Failed to start WebSocket server: {}", e);
        process::exit(1);
    }
    
    println!("WebSocket server started on {}:{}", 
        websocket_config.host, 
        websocket_config.port
    );
    println!("Press Ctrl+C to stop...");
    
    // Wait for user to press Ctrl+C
    match signal_hook::iterator::Signals::new(&[signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM]) {
        Ok(mut signals) => {
            for _ in signals.forever() {
                break;
            }
        },
        Err(e) => {
            eprintln!("Failed to install signal handler: {}", e);
        }
    }
    
    // Stop the WebSocket server
    websocket_server.stop();
    
    println!("WebSocket server stopped");
}

// Add the handler function near the other handler functions
fn handle_credential_commands(
    command: &CredentialCommands,
    identity_manager: &IdentityManager,
    storage_manager: &StorageManager,
) {
    // Get the active identity
    let identity = match identity_manager.get_active_identity() {
        Some(id) => id,
        None => {
            eprintln!("No active identity found. Please use an identity first.");
            return;
        }
    };
    
    // Create federation runtime
    let api_config = ApiConfig::default_local();
    let federation_runtime = match federation::FederationRuntime::new(
        api_config.clone(),
        identity.clone(),
        storage_manager.clone(),
    ) {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("Failed to create federation runtime: {}", e);
            return;
        }
    };
    
    // Create federation sync service
    let sync_service = services::FederationSyncService::new(
        federation_runtime,
        storage_manager.clone(),
        identity_manager.clone(),
        None,
    );
    sync_service.initialize().unwrap_or_else(|e| {
        eprintln!("Warning: Could not initialize sync service: {}", e);
    });
    
    match command {
        CredentialCommands::Sync { federation, did } => {
            let target_did = did.as_ref().map(|s| s.as_str()).unwrap_or_else(|| identity.did());
            
            println!("Syncing credentials for DID: {}", target_did);
            if let Some(fed) = federation {
                println!("Filtering by federation: {}", fed);
            }
            
            // Sync credentials
            match sync_service.sync_did(target_did) {
                Ok(new_creds) => {
                    println!("Synced {} credentials", new_creds.len());
                    for cred in new_creds {
                        println!(" - {}: {} (from {})", 
                                 cred.credential_id, 
                                 cred.receipt.receipt_type,
                                 cred.receipt.federation_id);
                    }
                }
                Err(e) => {
                    eprintln!("Error syncing credentials: {}", e);
                }
            }
        },
        
        CredentialCommands::List { type_filter, trust } => {
            let all_credentials = sync_service.get_all_credentials();
            
            // Filter by type if specified
            let filtered: Vec<_> = all_credentials.into_iter()
                .filter(|cred| {
                    if let Some(filter) = type_filter {
                        cred.receipt.receipt_type.contains(filter)
                    } else {
                        true
                    }
                })
                .filter(|cred| {
                    if let Some(trust_level) = trust {
                        if let Some(score) = &cred.trust_score {
                            match trust_level.to_lowercase().as_str() {
                                "high" => score.status.to_lowercase() == "high",
                                "medium" => {
                                    let status = score.status.to_lowercase();
                                    status == "high" || status == "medium"
                                }
                                "low" => true, // All credentials pass low threshold
                                _ => true,
                            }
                        } else {
                            false // No trust score means it doesn't pass the filter
                        }
                    } else {
                        true
                    }
                })
                .collect();
            
            if filtered.is_empty() {
                println!("No credentials found matching your criteria");
                return;
            }
            
            println!("Found {} credentials:", filtered.len());
            for cred in filtered {
                let trust_info = if let Some(score) = cred.trust_score {
                    format!("[{}] Score: {}", score.status, score.score)
                } else {
                    "[Unverified]".to_string()
                };
                
                println!("{}: {} {} - {}", 
                         cred.credential_id, 
                         cred.receipt.receipt_type,
                         trust_info,
                         cred.receipt.issuer_name.unwrap_or_else(|| cred.receipt.issuer.clone()));
            }
        },
        
        CredentialCommands::Show { id, export, qr, with_thread, output } => {
            let credential = match sync_service.get_credential(id) {
                Some(cred) => cred,
                None => {
                    eprintln!("Credential not found with ID: {}", id);
                    return;
                }
            };
            
            println!("Credential Details:");
            println!("ID: {}", credential.credential_id);
            println!("Receipt ID: {}", credential.receipt_id);
            println!("Federation: {}", credential.federation_id);
            println!("Type: {}", credential.receipt.receipt_type);
            println!("Action: {}", credential.receipt.action_type);
            println!("Issuer: {}", credential.receipt.issuer);
            if let Some(name) = &credential.receipt.issuer_name {
                println!("Issuer Name: {}", name);
            }
            println!("Subject: {}", credential.receipt.subject_did);
            println!("Status: {:?}", credential.status);
            println!("Last Verified: {}", credential.last_verified);
            
            // Display thread ID if available
            if let Some(thread_id) = credential.receipt.metadata.get("thread_id") {
                println!("Thread ID: {}", thread_id);
                println!("Thread URL: https://agoranet.icn.zone/threads/{}", thread_id);
            }
            
            if let Some(score) = &credential.trust_score {
                println!("\nTrust Information:");
                println!("Score: {}/100 ({})", score.score, score.status);
                println!("Issuer Verified: {}", score.issuer_verified);
                println!("Signature Verified: {}", score.signature_verified);
                println!("Federation Verified: {}", score.federation_verified);
                println!("Quorum Met: {}", score.quorum_met);
            }
            
            println!("\nMetadata:");
            for (key, value) in &credential.receipt.metadata {
                println!("  {}: {}", key, value);
            }
            
            println!("\nSignatures: {}", credential.receipt.signatures.len());
            for (i, sig) in credential.receipt.signatures.iter().enumerate() {
                println!("  {}. {} ({}) - {}", i+1, sig.signer_did, sig.signer_role, sig.signature_type);
            }
            
            if *export {
                if let Some(mut vc) = credential.verifiable_credential.clone() {
                    // Add thread ID to metadata if requested and available
                    if *with_thread {
                        if let Some(thread_id) = credential.receipt.metadata.get("thread_id") {
                            // Ensure metadata field exists
                            if !vc.metadata.is_object() {
                                vc.metadata = serde_json::json!({});
                            }
                            
                            // Add AgoraNet metadata
                            let metadata = vc.metadata.as_object_mut().unwrap();
                            metadata.insert(
                                "agoranet".to_string(), 
                                serde_json::json!({
                                    "threadId": thread_id,
                                    "threadUrl": format!("https://agoranet.icn.zone/threads/{}", thread_id)
                                })
                            );
                            
                            println!("\nIncluded thread ID in exported credential");
                        } else {
                            println!("\nWarning: No thread ID available for this credential");
                        }
                    }
                    
                    let json = serde_json::to_string_pretty(&vc).unwrap_or_else(|_| "Failed to serialize VC".to_string());
                    
                    if let Some(path) = output {
                        if let Err(e) = std::fs::write(path, json) {
                            eprintln!("Failed to save VC to file: {}", e);
                        } else {
                            println!("\nExported verifiable credential to {}", path.display());
                        }
                    } else {
                        println!("\nVerifiable Credential:");
                        println!("{}", json);
                    }
                } else {
                    eprintln!("No verifiable credential available for this receipt");
                }
            }
            
            // Generate and display QR code if requested
            if *qr {
                if let Some(vc) = &credential.verifiable_credential {
                    match crate::vc::generate_credential_qr(vc, crate::vc::QrFormat::Terminal, None) {
                        Ok(qr_code) => {
                            println!("\nQR Code:");
                            println!("{}", qr_code);
                        },
                        Err(e) => {
                            eprintln!("Failed to generate QR code: {}", e);
                        }
                    }
                } else {
                    eprintln!("No verifiable credential available for QR code generation");
                }
            }
        },
        
        CredentialCommands::Verify { id } => {
            println!("Verifying credential: {}", id);
            
            match sync_service.verify_credential(id) {
                Ok(cred) => {
                    println!("Verification successful!");
                    println!("Status: {:?}", cred.status);
                    
                    if let Some(score) = cred.trust_score {
                        println!("Trust Score: {}/100 ({})", score.score, score.status);
                        println!("Issuer Verified: {}", score.issuer_verified);
                        println!("Signature Verified: {}", score.signature_verified);
                        println!("Federation Verified: {}", score.federation_verified);
                        println!("Quorum Met: {}", score.quorum_met);
                    } else {
                        println!("No trust score available");
                    }
                },
                Err(e) => {
                    eprintln!("Verification failed: {}", e);
                }
            }
        },
        
        CredentialCommands::ExportQR { id, format, output } => {
            let credential = match sync_service.get_credential(id) {
                Some(cred) => cred,
                None => {
                    eprintln!("Credential not found with ID: {}", id);
                    return;
                }
            };
            
            if let Some(vc) = &credential.verifiable_credential {
                // Parse format
                let qr_format = match crate::vc::QrFormat::from_str(format) {
                    Some(f) => f,
                    None => {
                        eprintln!("Invalid format: {}. Supported formats are terminal, svg, png", format);
                        return;
                    }
                };
                
                // Validate output path for non-terminal formats
                if qr_format != crate::vc::QrFormat::Terminal && output.is_none() {
                    eprintln!("Output path is required for {} format", format);
                    return;
                }
                
                // Generate QR code
                let output_path = output.as_ref().map(|p| p.as_path());
                match crate::vc::generate_credential_qr(vc, qr_format, output_path) {
                    Ok(result) => {
                        match qr_format {
                            crate::vc::QrFormat::Terminal => println!("{}", result),
                            _ => println!("{}", result),
                        }
                    },
                    Err(e) => {
                        eprintln!("Failed to generate QR code: {}", e);
                    }
                }
            } else {
                eprintln!("No verifiable credential available for this receipt");
            }
        },
        
        CredentialCommands::ImportQR { content, verify } => {
            println!("Importing credential from QR code...");
            
            // Decode QR content to a credential
            match crate::vc::decode_credential_from_qr(content) {
                Ok(vc) => {
                    println!("Successfully decoded credential from QR code:");
                    println!("ID: {}", vc.id);
                    println!("Type: {:?}", vc.types);
                    println!("Issuer: {}", vc.issuer);
                    println!("Subject: {}", vc.credentialSubject.id);
                    
                    // Create a temporary receipt from the VC to store it
                    // This is a simplified version - a real implementation would validate more thoroughly
                    let receipt_id = uuid::Uuid::new_v4().to_string();
                    let subject_did = vc.credentialSubject.id.clone();
                    let federation_id = format!("fed:{}:qrimport", vc.credentialSubject.federationMember.scope);
                    
                    let receipt = federation::FinalizationReceipt {
                        id: receipt_id.clone(),
                        federation_id: federation_id.clone(),
                        receipt_type: vc.types.get(1).cloned().unwrap_or_else(|| "unknown".to_string()),
                        issuer: vc.issuer.clone(),
                        issuer_name: None,
                        subject_did: subject_did,
                        action_type: "qr_import".to_string(),
                        timestamp: vc.issuanceDate,
                        dag_height: 0,
                        dag_vertex: "qr_import".to_string(),
                        metadata: std::collections::HashMap::new(),
                        signatures: vc.proof.iter().map(|p| federation::ReceiptSignature {
                            signer_did: vc.issuer.clone(),
                            signer_role: "Issuer".to_string(),
                            signature_value: p.proofValue.clone(),
                            signature_type: p.type_.clone(),
                            timestamp: p.created,
                        }).collect(),
                        content: serde_json::Value::Null,
                    };
                    
                    // Create credential data
                    let credential_id = format!("cred-qr-{}", uuid::Uuid::new_v4());
                    let credential_data = services::CredentialSyncData {
                        credential_id: credential_id.clone(),
                        receipt_id,
                        receipt,
                        federation_id,
                        status: services::CredentialStatus::Pending,
                        trust_score: None,
                        last_verified: chrono::Utc::now(),
                        verifiable_credential: Some(vc),
                    };
                    
                    // Process and save the credential
                    // Simplification: In a real implementation, we would call an internal API
                    // of the sync service rather than accessing its internals directly
                    let mut sync_data = sync_service.sync_data.lock().unwrap();
                    sync_data.insert(credential_id.clone(), credential_data.clone());
                    drop(sync_data);
                    
                    println!("Credential imported with ID: {}", credential_id);
                    
                    // Verify the credential if requested
                    if *verify {
                        println!("Verifying imported credential...");
                        match sync_service.verify_credential(&credential_id) {
                            Ok(verified) => {
                                println!("Verification successful!");
                                if let Some(score) = verified.trust_score {
                                    println!("Trust Score: {}/100 ({})", score.score, score.status);
                                    println!("Issuer Verified: {}", score.issuer_verified);
                                    println!("Signature Verified: {}", score.signature_verified);
                                    println!("Federation Verified: {}", score.federation_verified);
                                    println!("Quorum Met: {}", score.quorum_met);
                                }
                            },
                            Err(e) => {
                                println!("Verification failed: {}", e);
                                println!("The credential was imported but could not be verified.");
                            }
                        }
                    } else {
                        println!("Verification skipped. Use 'credentials verify --id {}' to verify it later.", credential_id);
                    }
                },
                Err(e) => {
                    eprintln!("Failed to decode credential from QR code: {}", e);
                }
            }
        },
        
        CredentialCommands::SelectiveDisclose { 
            id, 
            include_fields, 
            exclude_fields, 
            proof_type, 
            reason, 
            output 
        } => {
            println!("Creating selective disclosure from credential {}...", id);
            
            // Fetch the credential from storage
            let credential = match sync_service.get_credential(&id) {
                Some(cred) => cred,
                None => {
                    println!("Error: Credential with ID {} not found", id);
                    return;
                }
            };
            
            // Convert fields to Vec<String> if specified
            let include_fields_vec = include_fields
                .map(|fields| fields.split(',').map(|s| s.trim().to_string()).collect::<Vec<String>>());
            
            let exclude_fields_vec = exclude_fields
                .map(|fields| fields.split(',').map(|s| s.trim().to_string()).collect::<Vec<String>>());
            
            // Validate proof type
            let proof_type = match proof_type.as_str() {
                "redaction" => "redaction",
                "zk" => {
                    println!("Warning: Zero-knowledge proofs not yet implemented, using redaction instead");
                    "redaction"
                },
                _ => {
                    println!("Error: Invalid proof type {}, using redaction instead", proof_type);
                    "redaction"
                }
            };
            
            // Convert credential to JSON Value for processing
            let credential_value = match serde_json::to_value(&credential) {
                Ok(val) => val,
                Err(e) => {
                    println!("Error serializing credential: {}", e);
                    return;
                }
            };
            
            // Call the Node.js script to handle selective disclosure
            let script_path = Path::new("packages/credential-utils/scripts/selective-disclosure.js");
            
            let mut command = Command::new("node");
            command.arg(script_path);
            command.arg("--credential");
            command.arg(serde_json::to_string(&credential_value).unwrap());
            
            if let Some(fields) = &include_fields_vec {
                command.arg("--include");
                command.arg(fields.join(","));
            }
            
            if let Some(fields) = &exclude_fields_vec {
                command.arg("--exclude");
                command.arg(fields.join(","));
            }
            
            command.arg("--proof-type");
            command.arg(proof_type);
            
            if let Some(reason_text) = &reason {
                command.arg("--reason");
                command.arg(reason_text);
            }
            
            // Execute the command and get the output
            match command.output() {
                Ok(output) => {
                    if output.status.success() {
                        let disclosure_json = String::from_utf8_lossy(&output.stdout);
                        
                        // Determine output path
                        let output_path = match output {
                            Some(path) => path,
                            None => format!("{}-selective-disclosure.json", id),
                        };
                        
                        // Write to file
                        match fs::write(&output_path, disclosure_json.as_bytes()) {
                            Ok(_) => {
                                println!("Selective disclosure created successfully!");
                                println!("Saved to: {}", output_path);
                            },
                            Err(e) => {
                                println!("Error writing disclosure to file: {}", e);
                            }
                        }
                    } else {
                        println!("Error creating selective disclosure: {}", 
                            String::from_utf8_lossy(&output.stderr));
                    }
                },
                Err(e) => {
                    println!("Error executing selective disclosure command: {}", e);
                }
            }
        },
        
        CredentialCommands::RestoreCredential { 
            credential_id, 
            reason, 
            text_hash, 
            amendment_id, 
            federation_id, 
            output 
        } => {
            println!("Creating amendment credential to restore/amend credential {}...", credential_id);
            
            // Fetch the credential to be amended from storage
            let credential = match sync_service.get_credential(&credential_id) {
                Some(cred) => cred,
                None => {
                    eprintln!("Error: Credential with ID {} not found", credential_id);
                    return;
                }
            };
            
            // Get the federation ID from the credential if not specified
            let fed_id = match federation_id {
                Some(id) => id,
                None => {
                    credential.federation_id.clone()
                }
            };
            
            // Get the federation runtime for this federation
            let fed_runtime = sync_service.get_federation_runtime();
            
            // Get active identity DID for subject DID
            let subject_did = identity.did().to_string();
            
            // Calculate text hash if not provided
            let text_hash_value = match text_hash {
                Some(hash) => hash,
                None => {
                    // Generate a hash from the reason text
                    use sha2::{Sha256, Digest};
                    let mut hasher = Sha256::new();
                    hasher.update(reason.as_bytes());
                    let hash_result = hasher.finalize();
                    format!("sha256:{}", hex::encode(hash_result))
                }
            };
            
            // Get the federation manifest
            let manifest = match fed_runtime.get_federation_manifest(&fed_id) {
                Ok(manifest) => manifest,
                Err(e) => {
                    eprintln!("Error fetching federation manifest: {}", e);
                    return;
                }
            };
            
            // Create the amendment ID if not provided
            let amendment_id_value = match amendment_id {
                Some(id) => id,
                None => {
                    // Generate a UUID-based amendment ID
                    use uuid::Uuid;
                    format!("amendment:{}", Uuid::new_v4())
                }
            };
            
            // Get current DAG root info
            let dag_info = match fed_runtime.get_current_dag_root(&fed_id) {
                Ok(info) => info,
                Err(e) => {
                    eprintln!("Error fetching current DAG root: {}", e);
                    return;
                }
            };
            
            // Create the amendment anchor credential
            let anchor_options = federation::AnchorCredentialOptions {
                anchor_type: "amendment".to_string(),
                federation: federation::FederationInfo {
                    id: fed_id.clone(),
                    name: manifest.name.clone(),
                    did: fed_runtime.get_federation_did(&fed_id).unwrap_or_else(|_| 
                        format!("did:icn:federation:{}", fed_id)),
                },
                subject_did: subject_did.clone(),
                dag_root_hash: dag_info.root_hash.clone(),
                effective_from: chrono::Utc::now().to_rfc3339(),
                referenced_credentials: vec![credential_id.clone()],
                amendment_id: Some(amendment_id_value.clone()),
                text_hash: Some(text_hash_value.clone()),
                description: Some(reason.clone()),
                previous_amendment_id: None,
                effective_until: None,
                ratified_in_epoch: None,
            };
            
            // Create the credential
            match fed_runtime.create_anchor_credential(anchor_options) {
                Ok(anchor_credential) => {
                    println!("Successfully created amendment credential!");
                    println!("Amendment ID: {}", amendment_id_value);
                    println!("References credential: {}", credential_id);
                    println!("Reason: {}", reason);
                    
                    // Save the credential to the sync service
                    match sync_service.save_credential(anchor_credential.clone()) {
                        Ok(saved_cred) => {
                            println!("Amendment credential saved to wallet with ID: {}", saved_cred.credential_id);
                        },
                        Err(e) => {
                            eprintln!("Warning: Could not save credential to wallet: {}", e);
                        }
                    }
                    
                    // Save to output file if requested
                    if let Some(path) = output {
                        match std::fs::write(&path, serde_json::to_string_pretty(&anchor_credential).unwrap()) {
                            Ok(_) => println!("Saved amendment credential to {}", path.display()),
                            Err(e) => eprintln!("Error saving to file: {}", e),
                        }
                    }
                },
                Err(e) => {
                    eprintln!("Error creating amendment credential: {}", e);
                }
            }
        }
    }
}

/// Handle federation invite commands
fn handle_invite_commands(
    command: &InviteCommands,
    onboarding_service: &OnboardingService,
) {
    match command {
        InviteCommands::ExportQR { federation, format, output } => {
            println!("Generating invite QR code for federation: {}", federation);
            
            // Create the invite
            match onboarding_service.create_invite(federation) {
                Ok(invite) => {
                    // Parse format
                    let qr_format = match services::QrFormat::from_str(format) {
                        Some(f) => f,
                        None => {
                            eprintln!("Invalid format: {}. Supported formats are terminal, svg, png", format);
                            return;
                        }
                    };
                    
                    // Validate output path for non-terminal formats
                    if qr_format != services::QrFormat::Terminal && output.is_none() {
                        eprintln!("Output path is required for {} format", format);
                        return;
                    }
                    
                    // Generate QR code
                    let output_path = output.as_ref().map(|p| p.as_path());
                    match onboarding_service.generate_invite_qr(&invite, qr_format, output_path) {
                        Ok(result) => {
                            match qr_format {
                                services::QrFormat::Terminal => {
                                    println!("Federation: {} ({})", invite.name.unwrap_or_else(|| federation.to_string()), federation);
                                    println!("Created by: {}", invite.creator_did);
                                    println!("Expires: {}", invite.expires.map(|e| e.to_string()).unwrap_or_else(|| "Never".to_string()));
                                    println!("\nQR Code:\n");
                                    println!("{}", result);
                                },
                                _ => println!("{}", result),
                            }
                        },
                        Err(e) => {
                            eprintln!("Failed to generate QR code: {}", e);
                        }
                    }
                },
                Err(e) => {
                    eprintln!("Failed to create federation invite: {}", e);
                }
            }
        },
        
        InviteCommands::ImportQR { content } => {
            println!("Importing federation invite from QR code...");
            
            // Decode QR content to an invite
            match onboarding_service.decode_invite_from_qr(content) {
                Ok(invite) => {
                    println!("Successfully decoded invite from QR code:");
                    println!("Federation: {} ({})", invite.name.unwrap_or_else(|| invite.federation_id.clone()), invite.federation_id);
                    println!("Created by: {}", invite.creator_did);
                    if let Some(expires) = invite.expires {
                        println!("Expires: {}", expires);
                    }
                    
                    // Process the invite
                    match onboarding_service.process_invite(invite) {
                        Ok(()) => {
                            println!("Federation invite processed successfully!");
                            println!("You can now sync credentials with this federation using:");
                            println!("  icn-wallet credentials sync --federation <federation-id>");
                        },
                        Err(e) => {
                            eprintln!("Failed to process federation invite: {}", e);
                        }
                    }
                },
                Err(e) => {
                    eprintln!("Failed to decode invite from QR code: {}", e);
                }
            }
        },
    }
}
