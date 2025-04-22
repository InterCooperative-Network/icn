mod api;
mod identity;
mod storage;
mod token;
mod guardians;
mod proposal;

use clap::{Parser, Subcommand};
use identity::{Identity, IdentityManager, KeyType};
use storage::{StorageManager, StorageType};
use token::{TokenStore, TokenType};
use guardians::{GuardianManager, GuardianSet};
use proposal::{ProposalManager, Proposal, VoteOption};
use api::{ApiClient, ApiConfig};
use std::path::PathBuf;
use std::process;
use std::str::FromStr;
use base64::{Engine as _};

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
    },
    
    /// Show details of a specific proposal
    Show {
        /// Hash of the proposal to show
        #[arg(long)]
        hash: String,
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
        
        Commands::Backup { out, password } => {
            backup_wallet(&storage_manager, out, password);
        },
        
        Commands::Restore { file, password } => {
            restore_wallet(&storage_manager, file, password);
        },
    }
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
        
        ProposalCommands::List { scope } => {
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
