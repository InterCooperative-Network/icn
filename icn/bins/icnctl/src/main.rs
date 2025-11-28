//! icnctl - CLI for managing ICNd

use anyhow::{bail, Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::generate;
// Governance types no longer needed - using RPC instead
use icn_identity::{
    AgeKeyStore, Capability, Did, KeyPair, KeyStore, KeyType, RecoveryAttestation, RecoveryEvent,
    RecoveryMethod, RecoveryConfig as IdentityRecoveryConfig,
};
use icn_store::{SledStore, Store};
use icn_trust::{TrustEdge, TrustGraph};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tar::{Archive, Builder};
use zeroize::Zeroizing;

#[derive(Parser, Debug)]
#[command(name = "icnctl")]
#[command(about = "ICN control CLI", long_about = None)]
struct Args {
    /// Data directory (defaults to ~/.icn)
    #[arg(short, long)]
    data_dir: Option<PathBuf>,

    /// RPC endpoint (defaults to 127.0.0.1:5601)
    #[arg(short, long, default_value = "127.0.0.1:5601")]
    endpoint: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Show daemon status
    Status,

    /// Identity management
    #[command(subcommand)]
    Id(IdCommands),

    /// Device management (multi-device identity)
    #[command(subcommand)]
    Device(DeviceCommands),

    /// Social recovery for lost devices
    #[command(subcommand)]
    Recovery(RecoveryCommands),

    /// Trust graph management
    #[command(subcommand)]
    Trust(TrustCommands),

    /// Ledger operations
    #[command(subcommand)]
    Ledger(LedgerCommands),

    /// Contract operations
    #[command(subcommand)]
    Contract(ContractCommands),

    /// Network operations (mDNS discovery, QUIC sessions)
    #[command(subcommand)]
    Network(NetworkCommands),

    /// Federation management (cross-network connectivity)
    #[command(subcommand)]
    Federation(FederationCommands),

    /// Governance operations (domains, proposals, votes)
    #[command(subcommand)]
    Gov(GovCommands),

    /// Backup data directory
    Backup {
        /// Output file path for backup archive
        output: PathBuf,
    },

    /// Restore data directory from backup
    Restore {
        /// Input backup archive path
        input: PathBuf,

        /// Force restore even if data directory exists
        #[arg(short, long)]
        force: bool,
    },

    /// Snapshot management
    #[command(subcommand)]
    Snapshot(SnapshotCommands),

    /// Initialize a new cooperative (guided setup wizard)
    InitCoop {
        /// Cooperative name
        #[arg(short, long)]
        name: Option<String>,

        /// Initial member DIDs (comma-separated)
        #[arg(short, long)]
        members: Option<String>,

        /// Skip confirmation prompts
        #[arg(short, long)]
        yes: bool,

        /// Don't start the daemon after setup
        #[arg(long)]
        no_start: bool,
    },

    /// Gateway authentication (get JWT tokens)
    #[command(subcommand)]
    Auth(AuthCommands),

    /// Distributed compute operations
    #[command(subcommand)]
    Compute(ComputeCommands),

    /// Cooperative scheduling policy management (Phase 16E)
    #[command(subcommand)]
    Policy(PolicyCommands),

    /// Resource quota management (Phase 16E)
    #[command(subcommand)]
    Quota(QuotaCommands),

    /// Generate shell completions
    Completions {
        /// Shell type
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand, Debug)]
enum AuthCommands {
    /// Get a JWT token for the Gateway API
    Token {
        /// Gateway API URL
        #[arg(short, long, default_value = "http://localhost:8080")]
        gateway: String,

        /// Cooperative ID
        #[arg(short, long)]
        coop_id: String,

        /// Scopes to request (comma-separated)
        #[arg(short, long, default_value = "ledger:read,ledger:write,coop:read,gov:read,gov:write")]
        scopes: String,
    },
}

#[derive(Subcommand, Debug)]
enum ComputeCommands {
    /// Submit a CCL contract for distributed execution
    Submit {
        /// Path to CCL contract JSON file
        #[arg(short, long)]
        contract: PathBuf,

        /// Task ID (auto-generated if not provided)
        #[arg(short, long)]
        id: Option<String>,

        /// Fuel limit (default 10000)
        #[arg(short, long, default_value = "10000")]
        fuel: u64,

        /// Task priority: low, normal, high, or critical (default: normal)
        #[arg(short = 'P', long, default_value = "normal")]
        priority: String,

        /// Path to inputs JSON file
        #[arg(long)]
        inputs: Option<PathBuf>,

        /// Payment rate per 1000 fuel (optional)
        #[arg(short, long)]
        payment_rate: Option<u64>,

        /// Payment currency (default: credits)
        #[arg(long)]
        payment_currency: Option<String>,
    },

    /// Check task status
    Status {
        /// Task hash (hex)
        task_hash: String,
    },

    /// Cancel a task
    Cancel {
        /// Task hash (hex)
        task_hash: String,

        /// Cancellation reason
        #[arg(short, long, default_value = "Cancelled by user")]
        reason: String,
    },
}

#[derive(Subcommand, Debug)]
enum PolicyCommands {
    /// Set or update policy for a cooperative
    Set {
        /// Cooperative ID
        #[arg(short, long)]
        coop_id: String,

        /// Path to policy JSON file
        #[arg(short, long)]
        policy: PathBuf,
    },

    /// Show policy for a cooperative
    Show {
        /// Cooperative ID
        #[arg(short, long)]
        coop_id: String,
    },

    /// List all policies
    List,

    /// Remove policy for a cooperative
    Remove {
        /// Cooperative ID
        #[arg(short, long)]
        coop_id: String,
    },
}

#[derive(Subcommand, Debug)]
enum QuotaCommands {
    /// Show resource usage for a member
    Show {
        /// Cooperative ID
        #[arg(short, long)]
        coop_id: String,

        /// Member DID
        #[arg(short, long)]
        member: String,
    },

    /// List usage for all members in a cooperative
    List {
        /// Cooperative ID
        #[arg(short, long)]
        coop_id: String,
    },
}

#[derive(Subcommand, Debug)]
enum IdCommands {
    /// Initialize identity with new keystore
    Init,

    /// Show current identity
    Show,

    /// Rotate to a new key
    Rotate {
        /// Reason for rotation
        #[arg(short, long)]
        reason: Option<String>,
    },

    /// Export identity (backup)
    Export {
        /// Output file path
        output: PathBuf,
    },

    /// Import identity (restore)
    Import {
        /// Input file path
        input: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
enum DeviceCommands {
    /// List all devices for this identity
    List,

    /// Create a device-add request for a new device
    Add {
        /// Device name/label
        name: String,
    },

    /// Approve a device-add request (from an existing authorized device)
    Approve {
        /// Path to device-add request file
        request_file: PathBuf,
    },

    /// Revoke a device
    Revoke {
        /// Device ID to revoke
        device_id: String,

        /// Reason for revocation
        #[arg(short, long)]
        reason: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum RecoveryCommands {
    /// Configure social recovery trustees
    Setup {
        /// Trustee DIDs (comma-separated)
        trustees: String,

        /// Number of trustees required (M-of-N threshold)
        #[arg(short, long)]
        threshold: usize,

        /// Delay period in seconds before finalization (default: 86400 = 24 hours)
        #[arg(short, long, default_value = "86400")]
        delay: u64,
    },

    /// Show current recovery configuration
    Config,

    /// Initiate recovery for a lost identity
    Initiate {
        /// Old DID to recover
        old_did: String,
    },

    /// Sign a recovery attestation as a trustee
    Attest {
        /// Recovery ID to attest
        recovery_id: String,

        /// How you verified the user's identity
        #[arg(short, long)]
        verification: String,
    },

    /// List all active recovery requests
    List,

    /// Show status of a specific recovery
    Status {
        /// Recovery ID
        recovery_id: String,
    },

    /// Finalize a recovery (after M signatures + delay)
    Finalize {
        /// Recovery ID to finalize
        recovery_id: String,
    },

    /// Cancel a recovery (if fraudulent or device found)
    Cancel {
        /// Recovery ID to cancel
        recovery_id: String,

        /// Reason for cancellation
        #[arg(short, long)]
        reason: String,
    },
}

#[derive(Subcommand, Debug)]
enum TrustCommands {
    /// Add trust edge
    Add {
        /// Target DID
        did: String,

        /// Trust score (0.0-1.0)
        score: f64,

        /// Optional label
        #[arg(short, long)]
        label: Option<String>,
    },

    /// List all trust edges
    List,

    /// Show computed trust score for a DID
    Show {
        /// Target DID
        did: String,
    },

    /// Remove trust edge
    Remove {
        /// Target DID
        did: String,
    },
}

#[derive(Subcommand, Debug)]
enum LedgerCommands {
    /// Show most recent ledger entry
    Head,

    /// Show balance for an account
    Balance {
        /// Account DID
        account_id: String,

        /// Optional currency filter
        #[arg(short, long)]
        currency: Option<String>,
    },

    /// Show recent ledger history
    History {
        /// Number of entries to show (default: 10)
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },

    /// Quarantine management
    #[command(subcommand)]
    Quarantine(QuarantineCommands),
}

#[derive(Subcommand, Debug)]
enum QuarantineCommands {
    /// List all quarantined entries
    List,

    /// Get detailed info about a quarantined entry
    Get {
        /// Entry ID (content hash)
        entry_id: String,
    },

    /// Release an entry from quarantine (retry)
    Release {
        /// Entry ID (content hash)
        entry_id: String,
    },

    /// Permanently drop an entry from quarantine
    Drop {
        /// Entry ID (content hash)
        entry_id: String,
    },

    /// Purge all expired entries
    Purge,
}

#[derive(Subcommand, Debug)]
enum ContractCommands {
    /// Deploy a contract from JSON file (single-party deployment)
    Deploy {
        /// Path to contract JSON file
        contract_file: PathBuf,
    },

    /// Prepare a contract for multi-party deployment (Phase 10C)
    /// Creates a deployment message with your signature
    Prepare {
        /// Path to contract JSON file
        contract_file: PathBuf,

        /// Output file for partial deployment message
        #[arg(short, long)]
        output: PathBuf,
    },

    /// Add your signature to a partial deployment (Phase 10C)
    Sign {
        /// Path to partial deployment JSON file
        deployment_file: PathBuf,

        /// Output file for updated deployment message
        #[arg(short, long)]
        output: PathBuf,
    },

    /// Deploy a fully-signed contract (Phase 10C)
    DeploySigned {
        /// Path to fully-signed deployment JSON file
        deployment_file: PathBuf,
    },

    /// Call a contract rule
    Call {
        /// Contract code hash (hex)
        code_hash: String,

        /// Rule name to execute
        rule_name: String,

        /// Caller DID
        caller: String,

        /// Arguments as JSON
        #[arg(short, long)]
        args: Option<String>,
    },

    /// List deployed contracts
    List,
}

#[derive(Subcommand, Debug)]
enum NetworkCommands {
    /// List discovered peers (via mDNS)
    Peers,

    /// Connect to a peer via QUIC
    Dial {
        /// Target DID
        did: String,

        /// Optional socket address (IP:port)
        #[arg(short, long)]
        addr: Option<String>,
    },

    /// Show network statistics
    Stats,

    /// Show network actor status
    Status,
}

#[derive(Subcommand, Debug)]
enum FederationCommands {
    /// Show current federation status
    Status,

    /// List configured bootstrap peers
    Peers,

    /// Add a bootstrap peer
    Add {
        /// Peer URL in format: icn://did:icn:PUBKEY@IP:PORT
        peer_url: String,

        /// Initial trust score for this peer (0.0-1.0)
        #[arg(short, long, default_value = "0.3")]
        trust: f64,
    },

    /// Remove a bootstrap peer
    Remove {
        /// DID of the peer to remove
        did: String,
    },

    /// Connect to a bootstrap peer immediately
    Connect {
        /// Peer URL in format: icn://did:icn:PUBKEY@IP:PORT
        peer_url: String,
    },

    /// Show federation configuration
    Config,

    /// Set federation configuration options
    Set {
        /// Configuration key (e.g., enabled, network_name, bootstrap_peer_trust)
        key: String,

        /// Configuration value
        value: String,
    },

    /// Generate a federation invite URL for this node
    Invite,
}

#[derive(Subcommand, Debug)]
enum GovCommands {
    /// Domain management
    #[command(subcommand)]
    Domain(DomainCommands),

    /// Proposal management
    #[command(subcommand)]
    Proposal(ProposalCommands),

    /// Vote on proposals
    #[command(subcommand)]
    Vote(VoteCommands),
}

#[derive(Subcommand, Debug)]
enum DomainCommands {
    /// Create a new governance domain
    Create {
        /// Domain ID (e.g., "coop:tiny-food")
        #[arg(long)]
        domain_id: String,

        /// Human-readable name
        #[arg(long)]
        name: String,

        /// Comma-separated member DIDs
        #[arg(long)]
        members: String,

        /// Governance profile (default: cooperative_default)
        #[arg(long, default_value = "cooperative_default")]
        profile: String,

        /// Quorum percentage (0-100)
        #[arg(long, default_value = "50")]
        quorum: u8,

        /// Approval threshold percentage (0-100)
        #[arg(long, default_value = "50")]
        approval: u8,

        /// Voting period in seconds (default: 7 days)
        #[arg(long, default_value = "604800")]
        voting_period: u64,
    },

    /// Show domain details
    Show {
        /// Domain ID
        #[arg(long)]
        domain_id: String,
    },

    /// List all domains
    List,
}

#[derive(Subcommand, Debug)]
enum ProposalCommands {
    /// Create a new proposal
    Create {
        /// Domain ID
        #[arg(long)]
        domain_id: String,

        /// Proposal title
        #[arg(long)]
        title: String,

        /// Proposal description
        #[arg(long)]
        description: String,

        /// Proposal type (text, budget, membership, config-change)
        #[arg(long)]
        kind: String,

        /// For text proposals: body content
        #[arg(long)]
        body: Option<String>,

        /// For budget proposals: amount
        #[arg(long)]
        amount: Option<i64>,

        /// For budget proposals: currency
        #[arg(long)]
        currency: Option<String>,

        /// For budget proposals: recipient DID
        #[arg(long)]
        recipient: Option<String>,

        /// For budget proposals: purpose
        #[arg(long)]
        purpose: Option<String>,

        /// For membership proposals: member DID
        #[arg(long)]
        member: Option<String>,

        /// For membership proposals: action (add/remove)
        #[arg(long)]
        action: Option<String>,

        /// For config-change proposals: new config JSON
        #[arg(long)]
        new_config: Option<String>,
    },

    /// Open a proposal for voting
    Open {
        /// Proposal ID
        #[arg(long)]
        proposal_id: String,

        /// Optional voting duration override (seconds)
        #[arg(long)]
        duration: Option<u64>,
    },

    /// List proposals in a domain
    List {
        /// Domain ID
        #[arg(long)]
        domain_id: String,

        /// Optional state filter (draft, open, accepted, rejected, noquorum, cancelled)
        #[arg(long)]
        state: Option<String>,
    },

    /// Show proposal details with current tally
    Show {
        /// Proposal ID
        #[arg(long)]
        proposal_id: String,
    },

    /// Close a proposal and compute outcome
    Close {
        /// Proposal ID
        #[arg(long)]
        proposal_id: String,
    },

    /// Cancel a proposal
    Cancel {
        /// Proposal ID
        #[arg(long)]
        proposal_id: String,
    },
}

#[derive(Subcommand, Debug)]
enum VoteCommands {
    /// Cast or update a vote
    Cast {
        /// Proposal ID
        #[arg(long)]
        proposal_id: String,

        /// Vote choice (for, against, abstain)
        #[arg(long)]
        choice: String,

        /// Optional comment
        #[arg(long)]
        comment: Option<String>,
    },

    /// Show your vote on a proposal
    Show {
        /// Proposal ID
        #[arg(long)]
        proposal_id: String,
    },
}

#[derive(Subcommand, Debug)]
enum SnapshotCommands {
    /// Create a manual snapshot
    Create,

    /// List all available snapshots
    List,

    /// Verify snapshot integrity
    Verify {
        /// Snapshot filename (optional, defaults to state.snapshot)
        snapshot: Option<String>,
    },

    /// Delete a snapshot
    Delete {
        /// Snapshot filename to delete
        snapshot: String,
    },

    /// Delete all old snapshots
    Cleanup {
        /// Number of snapshots to keep (default: 3)
        #[arg(short, long, default_value = "3")]
        keep: usize,
    },
}

fn get_data_dir(data_dir: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(dir) = data_dir {
        Ok(dir)
    } else {
        // Default to ~/.icn
        let home = dirs::home_dir().context("Could not determine home directory")?;
        Ok(home.join(".icn"))
    }
}

fn get_keystore_path(data_dir: &PathBuf) -> PathBuf {
    data_dir.join("identity.age")
}

fn get_store_path(data_dir: &PathBuf) -> PathBuf {
    data_dir.join("store")
}

fn read_passphrase(prompt: &str) -> Result<Vec<u8>> {
    // Check for ICN_PASSPHRASE environment variable first
    if let Ok(passphrase) = std::env::var("ICN_PASSPHRASE") {
        return Ok(passphrase.into_bytes());
    }

    print!("{prompt}");
    io::stdout().flush()?;
    let passphrase = rpassword::read_password()?;
    Ok(passphrase.into_bytes())
}

fn confirm_passphrase() -> Result<Vec<u8>> {
    // If ICN_PASSPHRASE is set, use it without confirmation
    if let Ok(passphrase) = std::env::var("ICN_PASSPHRASE") {
        return Ok(passphrase.into_bytes());
    }

    let pass1 = read_passphrase("Enter passphrase: ")?;
    let pass2 = read_passphrase("Confirm passphrase: ")?;

    if pass1 != pass2 {
        bail!("Passphrases do not match");
    }

    Ok(pass1)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize simple logging
    icn_obs::init()?;

    let data_dir = get_data_dir(args.data_dir)?;

    match args.command {
        Commands::Status => {
            println!("ICNd Status: Not implemented yet");
            println!("Data directory: {}", data_dir.display());
            // TODO: Connect to daemon via RPC and get status
        }

        Commands::Id(id_cmd) => handle_id_command(id_cmd, &data_dir)?,

        Commands::Device(device_cmd) => handle_device_command(device_cmd, &data_dir)?,

        Commands::Recovery(recovery_cmd) => handle_recovery_command(recovery_cmd, &data_dir)?,

        Commands::Trust(trust_cmd) => handle_trust_command(trust_cmd, &data_dir)?,

        Commands::Ledger(ledger_cmd) => handle_ledger_command(ledger_cmd, &args.endpoint)?,

        Commands::Contract(contract_cmd) => handle_contract_command(contract_cmd, &args.endpoint, &data_dir)?,

        Commands::Network(net_cmd) => handle_network_command(net_cmd, &args.endpoint)?,

        Commands::Federation(fed_cmd) => handle_federation_command(fed_cmd, &data_dir, &args.endpoint).await?,

        Commands::Gov(gov_cmd) => handle_gov_command(gov_cmd, &data_dir, &args.endpoint)?,

        Commands::Snapshot(snapshot_cmd) => handle_snapshot_command(snapshot_cmd, &data_dir)?,

        Commands::Backup { output } => handle_backup_command(&data_dir, &output)?,

        Commands::Restore { input, force } => handle_restore_command(&data_dir, &input, force)?,

        Commands::InitCoop { name, members, yes, no_start } => {
            handle_init_coop_command(&data_dir, name, members, yes, no_start).await?
        }

        Commands::Auth(auth_cmd) => {
            handle_auth_command(auth_cmd, &data_dir).await?
        }

        Commands::Compute(compute_cmd) => {
            handle_compute_command(compute_cmd, &args.endpoint)?
        }

        Commands::Policy(policy_cmd) => {
            handle_policy_command(policy_cmd, &args.endpoint)?
        }

        Commands::Quota(quota_cmd) => {
            handle_quota_command(quota_cmd, &args.endpoint)?
        }

        Commands::Completions { shell } => {
            let mut cmd = Args::command();
            let bin_name = cmd.get_name().to_string();
            generate(shell, &mut cmd, bin_name, &mut std::io::stdout());
        }
    }

    Ok(())
}

fn handle_id_command(cmd: IdCommands, data_dir: &PathBuf) -> Result<()> {
    let keystore_path = get_keystore_path(data_dir);

    match cmd {
        IdCommands::Init => {
            // Check if keystore already exists
            if keystore_path.exists() {
                bail!(
                    "Identity already exists at {}. Use 'id show' to view it.",
                    keystore_path.display()
                );
            }

            println!("Initializing new ICN identity...\n");

            // Get passphrase
            let passphrase = confirm_passphrase()?;

            // Create data directory if needed
            std::fs::create_dir_all(data_dir)
                .context("Failed to create data directory")?;

            // Initialize keystore (generates keypair internally)
            println!("\nGenerating Ed25519 keypair...");
            let keystore = AgeKeyStore::init(&keystore_path, &passphrase)?;

            println!("\n✓ Identity created successfully!");
            println!("  DID: {}", keystore.get_keypair()?.did());
            println!("  Keystore: {}", keystore_path.display());
            println!(
                "\nIMPORTANT: Store your passphrase securely. It cannot be recovered."
            );
        }

        IdCommands::Show => {
            // Check if keystore exists
            if !keystore_path.exists() {
                bail!(
                    "No identity found. Run 'icnctl id init' to create one."
                );
            }

            // Get passphrase
            let passphrase = read_passphrase("Enter passphrase: ")?;

            // Open and unlock keystore
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;

            let keypair = keystore.get_keypair()?;
            println!("Identity:");
            println!("  DID: {}", keypair.did());
            println!("  Keystore: {}", keystore_path.display());
        }

        IdCommands::Rotate { reason } => {
            // Check if keystore exists
            if !keystore_path.exists() {
                bail!(
                    "No identity found. Run 'icnctl id init' to create one."
                );
            }

            println!("Rotating identity key...\n");

            // Get passphrase
            let passphrase = read_passphrase("Enter passphrase: ")?;

            // Open and unlock keystore
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;

            let old_did = keystore.get_keypair()?.did().clone();

            // Generate new keypair
            println!("Generating new Ed25519 keypair...");
            let new_keypair = KeyPair::generate()?;
            let new_did = new_keypair.did().clone();

            // Perform rotation
            let rotation = keystore.rotate(new_keypair)?;

            println!("\n✓ Key rotation successful!");
            println!("  Old DID: {old_did}");
            println!("  New DID: {new_did}");
            if let Some(r) = reason {
                println!("  Reason: {r}");
            } else {
                println!("  Reason: {:?}", rotation.reason);
            }
            println!(
                "\nIMPORTANT: Publish the rotation proof to maintain identity continuity."
            );
            println!("  Timestamp: {}", rotation.timestamp);
        }

        IdCommands::Export { output } => {
            // Check if keystore exists
            if !keystore_path.exists() {
                bail!(
                    "No identity found at {}. Run 'id init' first.",
                    keystore_path.display()
                );
            }

            // Check if output file already exists
            if output.exists() {
                bail!(
                    "Output file already exists: {}. Remove it first or choose a different path.",
                    output.display()
                );
            }

            // Verify passphrase before allowing export
            let passphrase = read_passphrase("Enter passphrase to authorize export: ")?;

            // Open and unlock keystore to verify ownership
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)
                .context("Failed to unlock keystore. Incorrect passphrase.")?;

            // Get DID for export confirmation
            let did = keystore.get_keypair()?.did().clone();

            // Create output directory if needed
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)
                    .context("Failed to create output directory")?;
            }

            // Copy the encrypted keystore to output
            std::fs::copy(&keystore_path, &output)
                .with_context(|| format!("Failed to export keystore to {}", output.display()))?;

            println!("✓ Identity exported successfully!");
            println!("  DID:  {did}");
            println!("  From: {}", keystore_path.display());
            println!("  To:   {}", output.display());
            println!("\nIMPORTANT:");
            println!("  • Store this file securely");
            println!("  • The file is encrypted with your passphrase");
            println!("  • You'll need the same passphrase to import it");
        }

        IdCommands::Import { input } => {
            // Check if input file exists
            if !input.exists() {
                bail!("Input file not found: {}", input.display());
            }

            // Check if target keystore already exists
            if keystore_path.exists() {
                // Prompt for confirmation
                print!(
                    "Identity already exists at {}. Overwrite? (y/N): ",
                    keystore_path.display()
                );
                io::stdout().flush()?;

                let mut response = String::new();
                io::stdin().read_line(&mut response)?;
                if !response.trim().eq_ignore_ascii_case("y") {
                    println!("Import cancelled.");
                    return Ok(());
                }

                println!("Overwriting existing identity...");
            }

            // Create data directory if needed
            std::fs::create_dir_all(data_dir)
                .context("Failed to create data directory")?;

            // Verify the input file by attempting to load it
            print!("Enter passphrase for imported identity: ");
            io::stdout().flush()?;
            let passphrase = rpassword::read_password()?;
            let passphrase = Zeroizing::new(passphrase.into_bytes());

            // Test unlock on the input file
            let mut test_keystore = AgeKeyStore::new(&input);
            test_keystore.unlock(&passphrase)
                .context("Failed to unlock imported keystore. Check your passphrase.")?;

            let imported_did = test_keystore.get_keypair()?.did().clone();

            // Copy the validated keystore to target location
            std::fs::copy(&input, &keystore_path)
                .with_context(|| format!("Failed to import keystore to {}", keystore_path.display()))?;

            println!("\n✓ Identity imported successfully!");
            println!("  From: {}", input.display());
            println!("  To:   {}", keystore_path.display());
            println!("  DID:  {imported_did}");
        }
    }

    Ok(())
}

fn handle_recovery_command(cmd: RecoveryCommands, data_dir: &PathBuf) -> Result<()> {
    let keystore_path = get_keystore_path(data_dir);
    let store_path = get_store_path(data_dir);

    match cmd {
        RecoveryCommands::Setup { trustees, threshold, delay } => {
            // Check if keystore exists
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' to create one.");
            }

            // Parse trustee DIDs
            let trustee_dids: Result<Vec<Did>> = trustees
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(Did::from_str)
                .collect();
            let trustee_dids = trustee_dids.context("Failed to parse trustee DIDs")?;

            // Validate threshold
            if threshold > trustee_dids.len() {
                bail!(
                    "Threshold ({}) cannot be greater than number of trustees ({})",
                    threshold,
                    trustee_dids.len()
                );
            }
            if threshold == 0 {
                bail!("Threshold must be at least 1");
            }

            // Get passphrase and unlock keystore
            let passphrase = read_passphrase("Enter passphrase: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;

            // Create recovery configuration
            let recovery_config = IdentityRecoveryConfig {
                method: RecoveryMethod::Social {
                    m: threshold as u8,
                    n: trustee_dids.len() as u8,
                },
                threshold: threshold as u8,
                trustees: trustee_dids.clone(),
                delay_period: delay,
            };

            // Update DID document with recovery config
            keystore.update_did_document(
                |did_doc| {
                    did_doc.recovery = Some(recovery_config.clone());
                    Ok(())
                },
                None,  // No rotation event
                &passphrase,
            )?;

            println!("✓ Recovery configuration saved!");
            println!("\nRecovery trustees ({}):", trustee_dids.len());
            for (i, trustee) in trustee_dids.iter().enumerate() {
                println!("  {}. {}", i + 1, trustee);
            }
            println!("\nThreshold: {} of {}", threshold, trustee_dids.len());
            println!("Delay period: {} seconds ({} hours)", delay, delay / 3600);
            println!("\n⚠️  IMPORTANT:");
            println!("  • Choose trustees you trust and who know you well");
            println!("  • Trustees must verify your identity out-of-band (phone, video, in-person)");
            println!("  • If you lose all devices, contact {threshold} trustees to initiate recovery");
        }

        RecoveryCommands::Config => {
            // Check if keystore exists
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' to create one.");
            }

            // Get passphrase and unlock keystore
            let passphrase = read_passphrase("Enter passphrase: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;

            // Get DID document
            let did_doc = keystore.get_did_document()?;

            if let Some(recovery_config) = &did_doc.recovery {
                println!("Recovery Configuration:");
                println!("  Identity: {}", did_doc.id);
                println!("\nTrustees ({}):", recovery_config.trustees.len());
                for (i, trustee) in recovery_config.trustees.iter().enumerate() {
                    println!("  {}. {}", i + 1, trustee);
                }
                println!("\nThreshold: {}", recovery_config.threshold);
                println!("Delay period: {} seconds ({} hours)", recovery_config.delay_period, recovery_config.delay_period / 3600);

                match &recovery_config.method {
                    RecoveryMethod::Social { m, n } => {
                        println!("Method: Social recovery ({m}-of-{n})");
                    }
                    RecoveryMethod::BackupSeed => {
                        println!("Method: Backup seed");
                    }
                    RecoveryMethod::None => {
                        println!("Method: None (no recovery)");
                    }
                }
            } else {
                println!("No recovery configuration found.");
                println!("\nTo configure social recovery, run:");
                println!("  icnctl recovery setup <trustees> --threshold <M>");
            }
        }

        RecoveryCommands::Initiate { old_did } => {
            // Parse old DID
            let old_did = Did::from_str(&old_did).context("Invalid old DID")?;

            // Check if keystore exists (new identity)
            if !keystore_path.exists() {
                bail!("No identity found. Create a new identity first with 'icnctl id init'");
            }

            // Get passphrase and unlock keystore
            let passphrase = read_passphrase("Enter passphrase for NEW identity: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;

            let new_did = keystore.get_keypair()?.did().clone();

            println!("Initiating recovery:");
            println!("  Old DID: {old_did}");
            println!("  New DID: {new_did}");
            println!();

            // TODO: Get recovery config from old DID (would need gossip integration)
            // For now, prompt user for threshold and delay
            print!("Enter threshold (M-of-N): ");
            io::stdout().flush()?;
            let mut threshold_input = String::new();
            io::stdin().read_line(&mut threshold_input)?;
            let threshold: usize = threshold_input.trim().parse()
                .context("Invalid threshold")?;

            print!("Enter delay period in seconds (default 86400 = 24 hours): ");
            io::stdout().flush()?;
            let mut delay_input = String::new();
            io::stdin().read_line(&mut delay_input)?;
            let delay: u64 = if delay_input.trim().is_empty() {
                86400
            } else {
                delay_input.trim().parse().context("Invalid delay")?
            };

            // Create recovery event
            let recovery = RecoveryEvent::new(old_did.clone(), new_did.clone(), threshold, delay);

            // Save recovery event to store
            let store = SledStore::open(&store_path)?;
            let recovery_key = format!("recovery:{}", recovery.id);
            let recovery_json = serde_json::to_vec(&recovery)?;
            store.put(recovery_key.as_bytes(), &recovery_json)?;

            // TODO: Publish to gossip (done by daemon, not CLI)
            // let msg = RecoveryMessage::initiated(&recovery);
            // gossip.publish(IDENTITY_RECOVERY_TOPIC, &msg.to_bytes()?)?;

            println!("\n✓ Recovery initiated!");
            println!("  Recovery ID: {}", recovery.id);
            println!("  Status: {}", recovery.progress_summary());
            println!("\nNext steps:");
            println!("  1. Contact your {threshold} trustees out-of-band (phone, video, in-person)");
            println!("  2. Ask them to run: icnctl recovery attest {}", recovery.id);
            println!("  3. After {threshold} attestations, wait {delay} seconds delay period");
            println!("  4. Finalize recovery: icnctl recovery finalize {}", recovery.id);
        }

        RecoveryCommands::Attest { recovery_id, verification } => {
            // Load recovery event from store
            let store = SledStore::open(&store_path)?;
            let recovery_key = format!("recovery:{recovery_id}");
            let recovery_data = store.get(recovery_key.as_bytes())?
                .context("Recovery not found")?;
            let mut recovery: RecoveryEvent = serde_json::from_slice(&recovery_data)?;

            println!("Recovery attestation for:");
            println!("  Old DID: {}", recovery.old_did);
            println!("  New DID: {}", recovery.new_did);
            println!("  Status: {}", recovery.progress_summary());
            println!();

            // Get trustee's keystore
            if !keystore_path.exists() {
                bail!("No identity found. Trustees must have an ICN identity.");
            }

            let passphrase = read_passphrase("Enter your passphrase (trustee): ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;
            let keypair = keystore.get_keypair()?;

            println!("Signing attestation as: {}", keypair.did());
            println!();

            // Create attestation
            let attestation = RecoveryAttestation::new(
                keypair,
                recovery.old_did.clone(),
                recovery.new_did.clone(),
                verification.clone(),
            )?;

            // Add attestation to recovery
            let threshold_reached = recovery.add_attestation(attestation)?;

            // Save updated recovery
            let recovery_json = serde_json::to_vec(&recovery)?;
            store.put(recovery_key.as_bytes(), &recovery_json)?;

            // TODO: Publish attestation to gossip (done by daemon, not CLI)
            // let msg = RecoveryMessage::attestation(recovery_id.clone(), attestation);
            // gossip.publish(IDENTITY_RECOVERY_TOPIC, &msg.to_bytes()?)?;

            println!("✓ Attestation signed and added!");
            println!("  Trustee: {}", keypair.did());
            println!("  Verification: {verification}");
            println!("  Status: {}", recovery.progress_summary());

            if threshold_reached {
                println!("\n🎉 Threshold reached! Recovery entering delay period.");
                if recovery.delay_period > 0 {
                    println!("   Wait {} seconds before finalizing.", recovery.delay_period);
                } else {
                    println!("   No delay configured. Ready to finalize now!");
                }
            }
        }

        RecoveryCommands::List => {
            // List all recovery events from store
            let store = SledStore::open(&store_path)?;

            println!("Active recovery requests:\n");

            let mut found_any = false;
            let items = store.scan(b"recovery:")?;

            for (_key, value) in items {
                let recovery: RecoveryEvent = serde_json::from_slice(&value)?;

                if recovery.is_active() {
                    found_any = true;
                    println!("Recovery ID: {}", recovery.id);
                    println!("  Old DID: {}", recovery.old_did);
                    println!("  New DID: {}", recovery.new_did);
                    println!("  Status: {}", recovery.progress_summary());
                    println!("  Attestations: {}/{}", recovery.attestations.len(), recovery.threshold);
                    println!();
                }
            }

            if !found_any {
                println!("No active recovery requests found.");
            }
        }

        RecoveryCommands::Status { recovery_id } => {
            // Load recovery event
            let store = SledStore::open(&store_path)?;
            let recovery_key = format!("recovery:{recovery_id}");
            let recovery_data = store.get(recovery_key.as_bytes())?
                .context("Recovery not found")?;
            let recovery: RecoveryEvent = serde_json::from_slice(&recovery_data)?;

            println!("Recovery Status");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("ID: {}", recovery.id);
            println!("Old DID: {}", recovery.old_did);
            println!("New DID: {}", recovery.new_did);
            println!("Initiated: {}", recovery.initiated_at);
            println!();
            println!("Configuration:");
            println!("  Threshold: {}", recovery.threshold);
            println!("  Delay: {} seconds", recovery.delay_period);
            println!();
            println!("Progress: {}", recovery.progress_summary());
            println!();
            println!("Attestations ({}/{}):", recovery.attestations.len(), recovery.threshold);
            for (i, att) in recovery.attestations.iter().enumerate() {
                println!("  {}. Trustee: {}", i + 1, att.trustee);
                println!("     Verification: {}", att.verification_method);
                println!("     Timestamp: {}", att.timestamp);
            }

            if recovery.is_finalized() {
                println!("\n✓ Recovery finalized at: {}", recovery.finalized_at.unwrap());
            }
        }

        RecoveryCommands::Finalize { recovery_id } => {
            // Load recovery event
            let store = SledStore::open(&store_path)?;
            let recovery_key = format!("recovery:{recovery_id}");
            let recovery_data = store.get(recovery_key.as_bytes())?
                .context("Recovery not found")?;
            let mut recovery: RecoveryEvent = serde_json::from_slice(&recovery_data)?;

            println!("Finalizing recovery:");
            println!("  Old DID: {}", recovery.old_did);
            println!("  New DID: {}", recovery.new_did);
            println!("  Status: {}", recovery.progress_summary());
            println!();

            // Check if delay expired
            recovery.check_delay_expired();

            // Finalize
            recovery.finalize()?;

            // Save updated recovery
            let recovery_json = serde_json::to_vec(&recovery)?;
            store.put(recovery_key.as_bytes(), &recovery_json)?;

            // TODO: Publish finalization to gossip (done by daemon, not CLI)
            // let msg = RecoveryMessage::finalized(&recovery)?;
            // gossip.publish(IDENTITY_RECOVERY_TOPIC, &msg.to_bytes()?)?;

            println!("✓ Recovery finalized successfully!");
            println!("\nThe new DID ({}) now inherits the old identity.", recovery.new_did);
            println!("\nNext steps:");
            println!("  • Trust graph and ledger will recognize the new DID");
            println!("  • All relationships and balances are preserved");
            println!("  • Old DID is marked as recovered");
        }

        RecoveryCommands::Cancel { recovery_id, reason } => {
            // Load recovery event
            let store = SledStore::open(&store_path)?;
            let recovery_key = format!("recovery:{recovery_id}");
            let recovery_data = store.get(recovery_key.as_bytes())?
                .context("Recovery not found")?;
            let mut recovery: RecoveryEvent = serde_json::from_slice(&recovery_data)?;

            // Get current identity
            if !keystore_path.exists() {
                bail!("No identity found.");
            }

            let passphrase = read_passphrase("Enter passphrase: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;
            let keypair = keystore.get_keypair()?;
            let canceller_did = keypair.did().clone();

            println!("Cancelling recovery:");
            println!("  Recovery ID: {}", recovery.id);
            println!("  Old DID: {}", recovery.old_did);
            println!("  New DID: {}", recovery.new_did);
            println!("  Cancelled by: {canceller_did}");
            println!("  Reason: {reason}");
            println!();

            // Cancel recovery
            recovery.cancel(canceller_did, reason.clone())?;

            // Save updated recovery
            let recovery_json = serde_json::to_vec(&recovery)?;
            store.put(recovery_key.as_bytes(), &recovery_json)?;

            // TODO: Publish cancellation to gossip (done by daemon, not CLI)
            // let msg = RecoveryMessage::cancelled(recovery.id.clone(), canceller_did, reason.clone(), recovery.cancelled_at);
            // gossip.publish(IDENTITY_RECOVERY_TOPIC, &msg.to_bytes()?)?;

            println!("✓ Recovery cancelled!");
            println!("\n⚠️  This recovery attempt has been marked as fraudulent.");
            println!("   Reason: {reason}");
        }
    }

    Ok(())
}

fn handle_trust_command(cmd: TrustCommands, data_dir: &PathBuf) -> Result<()> {
    let keystore_path = get_keystore_path(data_dir);
    let store_path = get_store_path(data_dir);

    // Load identity
    if !keystore_path.exists() {
        bail!(
            "No identity found. Run 'icnctl id init' to create one first."
        );
    }

    let passphrase = read_passphrase("Enter passphrase: ")?;

    let mut keystore = AgeKeyStore::open(&keystore_path)?;
    keystore.unlock(&passphrase)?;

    let own_did = keystore.get_keypair()?.did().clone();

    // Create store and trust graph
    let store: Arc<dyn Store> = Arc::new(SledStore::open(&store_path)?);
    let mut graph = TrustGraph::new(store, own_did.clone());

    match cmd {
        TrustCommands::Add { did, score, label } => {
            // Validate score
            if !(0.0..=1.0).contains(&score) {
                bail!("Trust score must be between 0.0 and 1.0");
            }

            // Parse target DID
            let target_did = parse_did(&did)?;

            // Create edge
            let mut edge = TrustEdge::new(own_did, target_did.clone(), score);

            if let Some(l) = label {
                edge = edge.with_label(l);
            }

            // Add to graph
            graph.add_edge(edge)?;

            println!("✓ Added trust edge");
            println!("  Target: {target_did}");
            println!("  Score: {score:.2}");
        }

        TrustCommands::List => {
            let edges = graph.get_outgoing_edges(&own_did)?;

            if edges.is_empty() {
                println!("No trust edges found.");
            } else {
                println!("Trust edges from {own_did}:\n");
                for edge in edges {
                    println!("  → {}", edge.target);
                    println!("    Score: {:.2}", edge.score);
                    if !edge.labels.is_empty() {
                        println!("    Labels: {}", edge.labels.join(", "));
                    }
                    if !edge.evidence.is_empty() {
                        println!("    Evidence: {} items", edge.evidence.len());
                    }
                    println!();
                }
            }
        }

        TrustCommands::Show { did } => {
            let target_did = parse_did(&did)?;

            // Compute trust score
            let score = graph.compute_trust_score(&target_did)?;
            let class = graph.trust_class(&target_did)?;

            println!("Trust score for {target_did}:");
            println!("  Score: {score:.4}");
            println!("  Class: {class:?}");

            // Show direct edge if exists
            if let Some(edge) = graph.get_edge(&own_did, &target_did)? {
                println!("\nDirect trust edge:");
                println!("  Score: {:.2}", edge.score);
                if !edge.labels.is_empty() {
                    println!("  Labels: {}", edge.labels.join(", "));
                }
            } else {
                println!("\nNo direct trust edge (score computed transitively)");
            }
        }

        TrustCommands::Remove { did } => {
            let target_did = parse_did(&did)?;

            graph.remove_edge(&own_did, &target_did)?;

            println!("✓ Removed trust edge to {target_did}");
        }
    }

    Ok(())
}

#[tokio::main]
async fn handle_network_command(cmd: NetworkCommands, endpoint: &str) -> Result<()> {
    // Network commands communicate with daemon via RPC
    let rpc_addr = endpoint.parse()?;
    let mut client = icn_rpc::RpcClient::new(rpc_addr);

    match cmd {
        NetworkCommands::Peers => {
            let peers = client
                .get_peers()
                .await
                .context("Failed to get peers from daemon. Is icnd running?")?;

            if peers.is_empty() {
                println!("No peers discovered yet.");
                println!("\nTip: Ensure other ICN nodes are running on the network.");
            } else {
                println!("Discovered Peers:\n");
                println!("{:<50} {:<22} Version", "DID", "Address");
                println!("{}", "-".repeat(80));
                for peer in peers {
                    println!("{:<50} {:<22} {}", peer.did, peer.addr, peer.version);
                }
            }
        }

        NetworkCommands::Dial { did, addr } => {
            let addr_str = addr.unwrap_or_else(|| "auto-discover".to_string());
            println!("Dialing peer...");
            println!("  Target DID: {did}");
            println!("  Address: {addr_str}\n");

            client
                .dial(did.clone(), addr_str)
                .await
                .context("Failed to dial peer. Is icnd running?")?;

            println!("✓ Successfully established connection to {did}");
        }

        NetworkCommands::Stats => {
            let stats = client
                .get_stats()
                .await
                .context("Failed to get network stats from daemon. Is icnd running?")?;

            println!("Network Statistics:\n");
            println!("  Peers discovered:      {}", stats.peers_discovered);
            println!("  Active connections:    {}", stats.connections_active);
            println!("  Total connections:     {}", stats.connections_total);
        }

        NetworkCommands::Status => {
            let status = client
                .get_status()
                .await
                .context("Failed to get network status from daemon. Is icnd running?")?;

            println!("Network Actor Status:\n");
            println!("  Running:               {}", status.running);
            println!("  Listener address:      {}", status.listen_addr);
        }
    }

    Ok(())
}

async fn handle_federation_command(cmd: FederationCommands, data_dir: &PathBuf, endpoint: &str) -> Result<()> {
    use icn_core::config::{Config, FederationConfig};

    let config_path = data_dir.join("icn.toml");

    match cmd {
        FederationCommands::Status => {
            // Try to get status from daemon first
            let rpc_addr: std::net::SocketAddr = endpoint.parse()?;
            let mut client = icn_rpc::RpcClient::new(rpc_addr);

            match client.get_peers().await {
                Ok(peers) => {
                    // Load config for federation settings
                    let fed_config = if config_path.exists() {
                        let config = Config::from_file(&config_path)?;
                        config.federation
                    } else {
                        FederationConfig::default()
                    };

                    println!("Federation Status:\n");
                    println!("  Enabled:           {}", fed_config.enabled);
                    println!("  Network name:      {}", fed_config.network_name);
                    println!("  Connected peers:   {}", peers.len());
                    println!("  Max federations:   {}", fed_config.max_federations);
                    println!("  Auto-accept:       {}", fed_config.auto_accept_invites);
                    if fed_config.auto_accept_invites {
                        println!("  Min invite trust:  {:.2}", fed_config.min_invite_trust);
                    }
                }
                Err(_) => {
                    // Daemon not running, show config-only status
                    let fed_config = if config_path.exists() {
                        let config = Config::from_file(&config_path)?;
                        config.federation
                    } else {
                        FederationConfig::default()
                    };

                    println!("Federation Status (daemon not running):\n");
                    println!("  Enabled:           {}", fed_config.enabled);
                    println!("  Network name:      {}", fed_config.network_name);
                    println!("  Max federations:   {}", fed_config.max_federations);
                    println!("  Auto-accept:       {}", fed_config.auto_accept_invites);
                    println!("\nNote: Start icnd to see connected peers.");
                }
            }
        }

        FederationCommands::Peers => {
            // Load config to get bootstrap peers
            let config = if config_path.exists() {
                Config::from_file(&config_path)?
            } else {
                Config::default()
            };

            if config.network.bootstrap_peers.is_empty() {
                println!("No bootstrap peers configured.\n");
                println!("Add peers with: icnctl federation add <peer_url>");
            } else {
                println!("Configured Bootstrap Peers:\n");
                println!("{:<60} Status", "URL");
                println!("{}", "-".repeat(70));

                // Try to check status from daemon
                let rpc_addr: std::net::SocketAddr = endpoint.parse()?;
                let mut client = icn_rpc::RpcClient::new(rpc_addr);
                let connected_peers = client.get_peers().await.unwrap_or_default();
                let connected_dids: std::collections::HashSet<String> = connected_peers
                    .iter()
                    .map(|p| p.did.clone())
                    .collect();

                for peer_url in &config.network.bootstrap_peers {
                    // Extract DID from URL
                    let status = if let Some(did) = extract_did_from_peer_url(peer_url) {
                        if connected_dids.contains(&did) {
                            "Connected"
                        } else {
                            "Disconnected"
                        }
                    } else {
                        "Invalid URL"
                    };
                    println!("{peer_url:<60} {status}");
                }
            }
        }

        FederationCommands::Add { peer_url, trust } => {
            // Validate URL format
            if !peer_url.starts_with("icn://") {
                bail!("Invalid peer URL format. Expected: icn://did:icn:PUBKEY@IP:PORT");
            }

            // Validate trust range
            if !(0.0..=1.0).contains(&trust) {
                bail!("Trust score must be between 0.0 and 1.0");
            }

            // Load existing config
            let mut config = if config_path.exists() {
                Config::from_file(&config_path)?
            } else {
                Config::default()
            };

            // Check if already exists
            if config.network.bootstrap_peers.contains(&peer_url) {
                println!("Peer already configured: {peer_url}");
                return Ok(());
            }

            // Add peer
            config.network.bootstrap_peers.push(peer_url.clone());
            config.to_file(&config_path)?;

            println!("✓ Added bootstrap peer: {peer_url}");
            println!("  Initial trust: {trust:.2}");
            println!("\nNote: Restart icnd or use 'icnctl federation connect' to connect immediately.");
        }

        FederationCommands::Remove { did } => {
            // Load existing config
            let mut config = if config_path.exists() {
                Config::from_file(&config_path)?
            } else {
                bail!("No configuration file found at {}", config_path.display());
            };

            // Find and remove peer by DID
            let original_len = config.network.bootstrap_peers.len();
            config.network.bootstrap_peers.retain(|url| {
                !url.contains(&did)
            });

            if config.network.bootstrap_peers.len() == original_len {
                println!("No peer found with DID: {did}");
            } else {
                config.to_file(&config_path)?;
                println!("✓ Removed bootstrap peer: {did}");
            }
        }

        FederationCommands::Connect { peer_url } => {
            // Validate URL format
            if !peer_url.starts_with("icn://") {
                bail!("Invalid peer URL format. Expected: icn://did:icn:PUBKEY@IP:PORT");
            }

            // Extract DID and address from URL
            let (did, addr) = parse_peer_url(&peer_url)?;

            // Connect via RPC
            let rpc_addr: std::net::SocketAddr = endpoint.parse()?;
            let mut client = icn_rpc::RpcClient::new(rpc_addr);

            println!("Connecting to peer...");
            println!("  DID:     {did}");
            println!("  Address: {addr}\n");

            client
                .dial(did.clone(), addr)
                .await
                .context("Failed to connect to peer. Is icnd running?")?;

            println!("✓ Successfully connected to {did}");
        }

        FederationCommands::Config => {
            // Show federation configuration
            let config = if config_path.exists() {
                Config::from_file(&config_path)?
            } else {
                Config::default()
            };

            let fed = &config.federation;

            println!("Federation Configuration:\n");
            println!("  enabled:              {}", fed.enabled);
            println!("  network_name:         {}", fed.network_name);
            println!("  bootstrap_peer_trust: {:.2}", fed.bootstrap_peer_trust);
            println!("  auto_accept_invites:  {}", fed.auto_accept_invites);
            println!("  min_invite_trust:     {:.2}", fed.min_invite_trust);
            println!("  max_federations:      {}", fed.max_federations);
            println!("  announce_public_addr: {}", fed.announce_public_addr);
            println!("\nRetry Configuration:");
            println!("  max_retries:            {}", fed.retry.max_retries);
            println!("  initial_delay_secs:     {}", fed.retry.initial_delay_secs);
            println!("  max_delay_secs:         {}", fed.retry.max_delay_secs);
            println!("  reconnect_interval_secs:{}", fed.retry.reconnect_interval_secs);

            println!("\nBootstrap Peers: {}", config.network.bootstrap_peers.len());
            for peer in &config.network.bootstrap_peers {
                println!("  • {peer}");
            }
        }

        FederationCommands::Set { key, value } => {
            // Load existing config
            let mut config = if config_path.exists() {
                Config::from_file(&config_path)?
            } else {
                Config::default()
            };

            // Clone value for display after potential move
            let display_value = value.clone();

            match key.as_str() {
                "enabled" => {
                    config.federation.enabled = value.parse()
                        .context("Invalid value for 'enabled'. Use 'true' or 'false'.")?;
                }
                "network_name" => {
                    config.federation.network_name = value;
                }
                "bootstrap_peer_trust" => {
                    let trust: f64 = value.parse()
                        .context("Invalid value for 'bootstrap_peer_trust'. Use a number between 0.0 and 1.0.")?;
                    if !(0.0..=1.0).contains(&trust) {
                        bail!("Trust score must be between 0.0 and 1.0");
                    }
                    config.federation.bootstrap_peer_trust = trust;
                }
                "auto_accept_invites" => {
                    config.federation.auto_accept_invites = value.parse()
                        .context("Invalid value for 'auto_accept_invites'. Use 'true' or 'false'.")?;
                }
                "min_invite_trust" => {
                    let trust: f64 = value.parse()
                        .context("Invalid value for 'min_invite_trust'. Use a number between 0.0 and 1.0.")?;
                    if !(0.0..=1.0).contains(&trust) {
                        bail!("Trust score must be between 0.0 and 1.0");
                    }
                    config.federation.min_invite_trust = trust;
                }
                "max_federations" => {
                    config.federation.max_federations = value.parse()
                        .context("Invalid value for 'max_federations'. Use a positive integer.")?;
                }
                "announce_public_addr" => {
                    config.federation.announce_public_addr = value.parse()
                        .context("Invalid value for 'announce_public_addr'. Use 'true' or 'false'.")?;
                }
                _ => {
                    bail!("Unknown configuration key: {key}\n\nValid keys:\n  enabled, network_name, bootstrap_peer_trust, auto_accept_invites,\n  min_invite_trust, max_federations, announce_public_addr");
                }
            }

            config.to_file(&config_path)?;
            println!("✓ Set federation.{key} = {display_value}");
        }

        FederationCommands::Invite => {
            // Generate invite URL for this node
            let keystore_path = data_dir.join("identity.age");
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' first.");
            }

            // Get DID from keystore
            let passphrase = read_passphrase("Enter keystore passphrase: ")?;
            let mut keystore = AgeKeyStore::new(&keystore_path);
            keystore.unlock(&passphrase)?;
            let did = keystore.get_keypair()?.did().to_string();

            // Try to get our listen address from daemon
            let rpc_addr: std::net::SocketAddr = endpoint.parse()?;
            let mut client = icn_rpc::RpcClient::new(rpc_addr);

            match client.get_status().await {
                Ok(status) => {
                    let invite_url = format!("icn://{}@{}", did, status.listen_addr);
                    println!("Federation Invite URL:\n");
                    println!("  {invite_url}");
                    println!("\nShare this URL with peers who want to connect to your node.");
                    println!("They can add it with: icnctl federation add '{invite_url}'");
                }
                Err(_) => {
                    println!("Your DID: {did}\n");
                    println!("Start icnd to generate a complete invite URL with your network address.");
                }
            }
        }
    }

    Ok(())
}

/// Extract DID from a peer URL (icn://DID@IP:PORT)
fn extract_did_from_peer_url(url: &str) -> Option<String> {
    let url = url.strip_prefix("icn://")?;
    let parts: Vec<&str> = url.split('@').collect();
    if parts.len() == 2 {
        Some(parts[0].to_string())
    } else {
        None
    }
}

/// Parse peer URL into (DID, address)
fn parse_peer_url(url: &str) -> Result<(String, String)> {
    let url = url.strip_prefix("icn://")
        .context("URL must start with 'icn://'")?;

    let parts: Vec<&str> = url.split('@').collect();
    if parts.len() != 2 {
        bail!("Invalid peer URL format. Expected: icn://did:icn:PUBKEY@IP:PORT");
    }

    Ok((parts[0].to_string(), parts[1].to_string()))
}

#[tokio::main]
async fn handle_ledger_command(cmd: LedgerCommands, endpoint: &str) -> Result<()> {
    // Ledger commands communicate with daemon via RPC
    let rpc_addr = endpoint.parse()?;
    let mut client = icn_rpc::RpcClient::new(rpc_addr);

    match cmd {
        LedgerCommands::Head => {
            match client
                .get_ledger_head()
                .await
                .context("Failed to get ledger head from daemon. Is icnd running?")?
            {
                Some(entry) => {
                    println!("Most recent ledger entry:\n");
                    println!("  Hash:      {}", entry.hash);
                    println!("  Timestamp: {}", entry.timestamp);
                    println!("  Author:    {}", entry.author);
                    println!("\n  Account deltas:");
                    for delta in entry.accounts {
                        println!("    • {}", delta.account_id);
                        println!("      Currency: {}", delta.currency);
                        if let Some(debit) = delta.debit {
                            println!("      Debit:    {debit}");
                        }
                        if let Some(credit) = delta.credit {
                            println!("      Credit:   {credit}");
                        }
                    }
                }
                None => {
                    println!("Ledger is empty.");
                }
            }
        }

        LedgerCommands::Balance {
            account_id,
            currency,
        } => {
            let balances = client
                .get_ledger_balance(account_id.clone(), currency.clone())
                .await
                .context("Failed to get balance from daemon. Is icnd running?")?;

            if balances.is_empty() {
                println!("No balances found for account: {account_id}");
            } else if balances.len() == 1 && currency.is_some() {
                let balance = &balances[0];
                println!("Balance for {account_id}:\n");
                println!("  Currency: {}", balance.currency);
                println!("  Amount:   {}", balance.amount);
            } else {
                println!("Balances for {account_id}:\n");
                for balance in balances {
                    println!("  {:<10} {}", balance.currency, balance.amount);
                }
            }
        }

        LedgerCommands::History { limit } => {
            let entries = client
                .get_ledger_history(Some(limit))
                .await
                .context("Failed to get ledger history from daemon. Is icnd running?")?;

            if entries.is_empty() {
                println!("Ledger is empty.");
            } else {
                println!("Recent ledger entries (showing {}):\n", entries.len());

                for entry in entries {
                    println!("Hash:      {}", entry.hash);
                    println!("Timestamp: {}", entry.timestamp);
                    println!("Author:    {}", entry.author);
                    println!("Accounts:");
                    for delta in entry.accounts {
                        print!("  • {} ({}): ", delta.account_id, delta.currency);
                        if let Some(debit) = delta.debit {
                            print!("debit {debit} ");
                        }
                        if let Some(credit) = delta.credit {
                            print!("credit {credit} ");
                        }
                        println!();
                    }
                    println!();
                }
            }
        }

        LedgerCommands::Quarantine(q_cmd) => {
            handle_quarantine_command(q_cmd, &mut client).await?;
        }
    }

    Ok(())
}

async fn handle_quarantine_command(cmd: QuarantineCommands, client: &mut icn_rpc::RpcClient) -> Result<()> {
    match cmd {
        QuarantineCommands::List => {
            let result = client
                .quarantine_list()
                .await
                .context("Failed to list quarantine from daemon. Is icnd running?")?;

            // RPC returns PageResponse with "items" field
            let quarantined: Vec<serde_json::Value> = result
                .get("items")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            if quarantined.is_empty() {
                println!("No entries in quarantine.");
            } else {
                println!("Quarantined entries (showing {}):\n", quarantined.len());

                for item in quarantined {
                    println!("Entry ID:    {}", item["entry_id"].as_str().unwrap_or("N/A"));
                    println!("Reason:      {}", item["reason"].as_str().unwrap_or("N/A"));
                    println!("Author:      {}", item["author"].as_str().unwrap_or("N/A"));
                    println!("Observed at: {}", item["observed_at"].as_u64().unwrap_or(0));
                    if let Some(metadata) = item["metadata"].as_str() {
                        println!("Metadata:    {metadata}");
                    }
                    println!();
                }
            }
        }

        QuarantineCommands::Get { entry_id } => {
            let result = client
                .quarantine_get(entry_id.clone())
                .await
                .context("Failed to get quarantine entry from daemon. Is icnd running?")?;

            println!("Quarantined Entry: {entry_id}\n");

            if let Some(entry) = result.get("entry") {
                println!("Entry Details:");
                println!("  ID:          {}", entry["id"].as_str().unwrap_or("None"));
                println!("  Author:      {}", entry["author"].as_str().unwrap_or("N/A"));
                println!("  Timestamp:   {}", entry["timestamp"].as_u64().unwrap_or(0));
                println!("  Parents:     {:?}", entry["parents"].as_array().map(|v| v.len()).unwrap_or(0));
                println!("  Accounts:    {}", entry["num_accounts"].as_u64().unwrap_or(0));
                println!();
            }

            if let Some(info) = result.get("quarantine_info") {
                println!("Quarantine Info:");
                println!("  Reason:      {}", info["reason"].as_str().unwrap_or("N/A"));
                println!("  Author:      {}", info["author"].as_str().unwrap_or("N/A"));
                println!("  Observed:    {}", info["observed_at"].as_u64().unwrap_or(0));
                if let Some(metadata) = info["metadata"].as_str() {
                    println!("  Metadata:    {metadata}");
                }
            }
        }

        QuarantineCommands::Release { entry_id } => {
            // Note: This returns an error if reappend fails, as the intent of "release"
            // is to retry the entry. Standard error handling will display the error message.
            client
                .quarantine_release(entry_id.clone())
                .await
                .context("Failed to release entry from daemon. Is icnd running?")?;

            println!("✓ Released entry: {entry_id}");
            println!("✓ Successfully reappended to ledger");
        }

        QuarantineCommands::Drop { entry_id } => {
            let result = client
                .quarantine_drop(entry_id.clone())
                .await
                .context("Failed to drop entry from daemon. Is icnd running?")?;

            if result.get("dropped").and_then(|v| v.as_bool()).unwrap_or(false) {
                println!("✓ Permanently dropped entry: {entry_id}");
            } else {
                println!("✗ Failed to drop entry (not found in quarantine)");
            }
        }

        QuarantineCommands::Purge => {
            let result = client
                .quarantine_purge()
                .await
                .context("Failed to purge expired entries from daemon. Is icnd running?")?;

            let purged = result.get("purged").and_then(|v| v.as_u64()).unwrap_or(0);
            println!("✓ Purged {purged} expired entries");
        }
    }

    Ok(())
}

#[tokio::main]
async fn handle_contract_command(cmd: ContractCommands, endpoint: &str, data_dir: &PathBuf) -> Result<()> {
    // Contract commands communicate with daemon via RPC
    let rpc_addr = endpoint.parse()?;
    let mut client = icn_rpc::RpcClient::new(rpc_addr);

    match cmd {
        ContractCommands::Deploy { contract_file } => {
            // Read contract JSON from file
            let contract_json = std::fs::read_to_string(&contract_file)
                .with_context(|| format!("Failed to read contract file: {}", contract_file.display()))?;

            println!("Deploying contract from {}...\n", contract_file.display());

            // Parse contract to validate
            let contract: icn_ccl::Contract = serde_json::from_str(&contract_json)
                .context("Failed to parse contract JSON")?;

            // Validate contract
            contract.validate()
                .context("Contract validation failed")?;

            println!("✓ Contract validated");
            println!("  Name: {}", contract.name);
            println!("  Participants: {}", contract.participants.len());
            println!("  Rules: {}", contract.rules.len());
            println!();

            // Load keystore to sign deployment
            let keystore_path = get_keystore_path(data_dir);
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' to create one first.");
            }

            let passphrase = read_passphrase("Enter passphrase to sign deployment: ")?;

            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;
            let keypair = keystore.get_keypair()?;
            let deployer_did = keypair.did().clone();

            println!("Signing deployment as {deployer_did}");

            // Compute code hash (must match ContractActor::compute_code_hash)
            let code_hash = {
                use sha2::{Sha256, Digest};
                let mut hasher = Sha256::new();
                hasher.update(contract.name.as_bytes());
                for participant in &contract.participants {
                    hasher.update(format!("{participant:?}").as_bytes());
                }
                icn_ledger::ContentHash::from_bytes(hasher.finalize().into())
            };

            // Create installation record
            let installed_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();

            // For now, only deployer signs (multi-party signing is future work)
            let signing_bytes = icn_ccl::ContractDeploymentMessage::compute_signing_bytes(
                &code_hash,
                installed_at,
            );
            let deployer_signature = keypair.sign(&signing_bytes);

            let installation = icn_ccl::ContractInstallation {
                code_hash: code_hash.clone(),
                installed_by: deployer_did.clone(),
                capabilities: vec![], // No special capabilities for now
                participants: contract.participants.clone(),
                signatures: vec![(deployer_did.clone(), deployer_signature.to_bytes().to_vec())],
                installed_at,
                min_caller_trust: None, // Only participants can invoke
            };

            let deployment_msg = icn_ccl::ContractDeploymentMessage {
                code_hash: code_hash.clone(),
                contract: contract.clone(),
                installation,
                deployer_signature: deployer_signature.to_bytes().to_vec(),
            };

            // Serialize deployment message
            let deployment_json = serde_json::to_string(&deployment_msg)
                .context("Failed to serialize deployment message")?;

            println!("✓ Deployment message signed");
            println!();

            // Send to daemon
            match client
                .deploy_contract(deployment_json)
                .await
                .context("Failed to deploy contract to daemon. Is icnd running?")?
            {
                code_hash => {
                    println!("✓ Contract deployed successfully!");
                    println!("  Code Hash: {code_hash}");
                    println!("\nYou can now call contract rules using:");
                    println!("  icnctl contract call {code_hash} <rule_name> <caller_did> --args '{{}}'");
                }
            }
        }

        ContractCommands::Call {
            code_hash,
            rule_name,
            caller,
            args,
        } => {
            // Parse args JSON (default to empty object)
            let args_value: serde_json::Value = if let Some(args_str) = args {
                serde_json::from_str(&args_str)
                    .context("Failed to parse args JSON")?
            } else {
                serde_json::json!({})
            };

            println!("Calling contract {code_hash}...");
            println!("  Rule: {rule_name}");
            println!("  Caller: {caller}");
            println!("  Args: {args_value}\n");

            match client
                .call_contract(code_hash.clone(), rule_name.clone(), caller.clone(), args_value)
                .await
                .context("Failed to call contract. Is icnd running?")?
            {
                response => {
                    if response.success {
                        println!("✓ Contract execution successful!");
                        println!("  Fuel consumed: {}", response.fuel_consumed);
                        println!("  Return value: {}", response.return_value);
                    } else {
                        println!("✗ Contract execution failed!");
                    }
                }
            }
        }

        ContractCommands::Prepare { contract_file, output } => {
            handle_contract_prepare(&contract_file, &output, data_dir)?;
        }

        ContractCommands::Sign { deployment_file, output } => {
            handle_contract_sign(&deployment_file, &output, data_dir)?;
        }

        ContractCommands::DeploySigned { deployment_file } => {
            handle_contract_deploy_signed(&deployment_file, &mut client).await?;
        }

        ContractCommands::List => {
            match client
                .list_contracts()
                .await
            {
                Ok(contracts) => {
                    if contracts.is_empty() {
                        println!("No contracts deployed.");
                    } else {
                        println!("Deployed contracts:\n");
                        for contract in contracts {
                            println!("Code Hash: {}", contract.code_hash);
                            println!("  Name: {}", contract.name);
                            println!("  Participants: {}", contract.participants.join(", "));
                            if let Some(currency) = contract.currency {
                                println!("  Currency: {currency}");
                            }
                            println!("  Rules: {}", contract.rules.join(", "));
                            println!();
                        }
                    }
                }
                Err(e) => {
                    println!("Note: Contract listing not yet fully implemented.");
                    println!("Error: {e}");
                    println!("\nYou can still deploy and call contracts by code hash.");
                }
            }
        }
    }

    Ok(())
}

/// Handle contract prepare command - create initial deployment message with first signature
fn handle_contract_prepare(contract_file: &PathBuf, output: &PathBuf, data_dir: &PathBuf) -> Result<()> {
    // Read and validate contract
    let contract_json = std::fs::read_to_string(contract_file)
        .with_context(|| format!("Failed to read contract file: {}", contract_file.display()))?;

    println!("Preparing contract from {}...\n", contract_file.display());

    let contract: icn_ccl::Contract = serde_json::from_str(&contract_json)
        .context("Failed to parse contract JSON")?;

    contract.validate()
        .context("Contract validation failed")?;

    println!("✓ Contract validated");
    println!("  Name: {}", contract.name);
    println!("  Participants: {}", contract.participants.len());
    for (i, did) in contract.participants.iter().enumerate() {
        println!("    {}. {}", i + 1, did);
    }
    println!("  Rules: {}", contract.rules.len());
    println!();

    // Load keystore to sign
    let keystore_path = get_keystore_path(data_dir);
    if !keystore_path.exists() {
        bail!("No identity found. Run 'icnctl id init' to create one first.");
    }

    let passphrase = read_passphrase("Enter passphrase to sign deployment: ")?;

    let mut keystore = AgeKeyStore::open(&keystore_path)?;
    keystore.unlock(&passphrase)?;
    let keypair = keystore.get_keypair()?;
    let signer_did = keypair.did().clone();

    // Check if signer is a participant
    if !contract.participants.contains(&signer_did) {
        bail!("You ({signer_did}) are not a participant in this contract");
    }

    println!("Signing as {} ({} of {} participants)",
             signer_did,
             1,
             contract.participants.len());

    // Compute code hash
    let code_hash = {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(contract.name.as_bytes());
        for participant in &contract.participants {
            hasher.update(format!("{participant:?}").as_bytes());
        }
        icn_ledger::ContentHash::from_bytes(hasher.finalize().into())
    };

    // Create installation record
    let installed_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    // Generate signature
    let signing_bytes = icn_ccl::ContractDeploymentMessage::compute_signing_bytes(
        &code_hash,
        installed_at,
    );
    let signature = keypair.sign(&signing_bytes);

    let installation = icn_ccl::ContractInstallation {
        code_hash: code_hash.clone(),
        installed_by: signer_did.clone(),
        capabilities: vec![],
        participants: contract.participants.clone(),
        signatures: vec![(signer_did.clone(), signature.to_bytes().to_vec())],
        installed_at,
        min_caller_trust: None,
    };

    let deployment_msg = icn_ccl::ContractDeploymentMessage {
        code_hash: code_hash.clone(),
        contract: contract.clone(),
        installation,
        deployer_signature: signature.to_bytes().to_vec(),
    };

    // Write to output file
    let deployment_json = serde_json::to_string_pretty(&deployment_msg)
        .context("Failed to serialize deployment message")?;

    std::fs::write(output, deployment_json)
        .with_context(|| format!("Failed to write to {}", output.display()))?;

    println!("✓ Deployment message created: {}", output.display());
    println!();
    println!("Signatures collected: 1/{}", contract.participants.len());
    println!("Signed by:");
    println!("  ✓ {signer_did}");
    println!();
    println!("Next steps:");
    if contract.participants.len() > 1 {
        println!("  1. Send {} to other participants", output.display());
        println!("  2. They run: icnctl contract sign {} -o <output>", output.display());
        println!("  3. Once all {} signatures collected, deploy with:", contract.participants.len());
        println!("     icnctl contract deploy-signed <fully-signed.json>");
    } else {
        println!("  This is a single-participant contract. Deploy with:");
        println!("     icnctl contract deploy-signed {}", output.display());
    }

    Ok(())
}

/// Handle contract sign command - add your signature to a partial deployment
fn handle_contract_sign(deployment_file: &PathBuf, output: &PathBuf, data_dir: &PathBuf) -> Result<()> {
    // Read partial deployment
    let deployment_json = std::fs::read_to_string(deployment_file)
        .with_context(|| format!("Failed to read deployment file: {}", deployment_file.display()))?;

    println!("Adding signature to {}...\n", deployment_file.display());

    let mut deployment_msg: icn_ccl::ContractDeploymentMessage = serde_json::from_str(&deployment_json)
        .context("Failed to parse deployment JSON")?;

    println!("Contract: {}", deployment_msg.contract.name);
    println!("Participants: {}", deployment_msg.contract.participants.len());
    println!();

    // Load keystore
    let keystore_path = get_keystore_path(data_dir);
    if !keystore_path.exists() {
        bail!("No identity found. Run 'icnctl id init' to create one first.");
    }

    let passphrase = read_passphrase("Enter passphrase to sign: ")?;

    let mut keystore = AgeKeyStore::open(&keystore_path)?;
    keystore.unlock(&passphrase)?;
    let keypair = keystore.get_keypair()?;
    let signer_did = keypair.did().clone();

    // Check if signer is a participant
    if !deployment_msg.contract.participants.contains(&signer_did) {
        bail!("You ({signer_did}) are not a participant in this contract");
    }

    // Check if already signed
    if deployment_msg.installation.signatures.iter().any(|(did, _)| did == &signer_did) {
        bail!("You ({signer_did}) have already signed this deployment");
    }

    // Generate signature
    let signing_bytes = deployment_msg.signing_bytes();
    let signature = keypair.sign(&signing_bytes);

    // Add signature
    deployment_msg.installation.signatures.push((signer_did.clone(), signature.to_bytes().to_vec()));

    // Write to output
    let updated_json = serde_json::to_string_pretty(&deployment_msg)
        .context("Failed to serialize updated deployment")?;

    std::fs::write(output, updated_json)
        .with_context(|| format!("Failed to write to {}", output.display()))?;

    let signatures_count = deployment_msg.installation.signatures.len();
    let total_participants = deployment_msg.contract.participants.len();

    println!("✓ Signature added: {}", output.display());
    println!();
    println!("Signatures collected: {signatures_count}/{total_participants}");
    println!("Signed by:");
    for (did, _) in &deployment_msg.installation.signatures {
        println!("  ✓ {did}");
    }

    if signatures_count < total_participants {
        println!();
        println!("Still waiting for signatures from:");
        for participant in &deployment_msg.contract.participants {
            if !deployment_msg.installation.signatures.iter().any(|(did, _)| did == participant) {
                println!("  ⏳ {participant}");
            }
        }
        println!();
        println!("Next steps:");
        println!("  Send {} to remaining participants", output.display());
    } else {
        println!();
        println!("✓ All signatures collected! Ready to deploy.");
        println!();
        println!("Deploy with:");
        println!("  icnctl contract deploy-signed {}", output.display());
    }

    Ok(())
}

/// Handle deploy-signed command - deploy a fully-signed contract
async fn handle_contract_deploy_signed(deployment_file: &PathBuf, client: &mut icn_rpc::RpcClient) -> Result<()> {
    // Read deployment
    let deployment_json = std::fs::read_to_string(deployment_file)
        .with_context(|| format!("Failed to read deployment file: {}", deployment_file.display()))?;

    println!("Deploying signed contract from {}...\n", deployment_file.display());

    let deployment_msg: icn_ccl::ContractDeploymentMessage = serde_json::from_str(&deployment_json)
        .context("Failed to parse deployment JSON")?;

    println!("Contract: {}", deployment_msg.contract.name);
    println!("Participants: {}", deployment_msg.contract.participants.len());
    println!("Signatures: {}", deployment_msg.installation.signatures.len());
    println!();

    // Validate all participants have signed
    let participant_set: std::collections::HashSet<_> = deployment_msg.contract.participants.iter().collect();
    let signature_set: std::collections::HashSet<_> = deployment_msg.installation.signatures.iter().map(|(did, _)| did).collect();

    if participant_set != signature_set {
        let missing: Vec<_> = participant_set.difference(&signature_set).collect();
        bail!("Missing signatures from: {missing:?}");
    }

    println!("✓ All {} participants have signed", deployment_msg.contract.participants.len());
    println!();

    // Send to daemon
    match client
        .deploy_contract(deployment_json)
        .await
        .context("Failed to deploy contract to daemon. Is icnd running?")?
    {
        code_hash => {
            println!("✓ Contract deployed successfully!");
            println!("  Code Hash: {code_hash}");
            println!("\nYou can now call contract rules using:");
            println!("  icnctl contract call {code_hash} <rule_name> <caller_did> --args '{{}}'");
        }
    }

    Ok(())
}

fn parse_did(s: &str) -> Result<Did> {
    // For now, just validate format and wrap in Did
    // TODO: Add proper DID parsing when Did has a parse method
    if !s.starts_with("did:icn:") {
        bail!("Invalid DID format. Expected: did:icn:<base58btc-key>");
    }
    Ok(serde_json::from_value(serde_json::Value::String(
        s.to_string(),
    ))?)
}

/// Device add request - created on new device, approved on existing device
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeviceAddRequest {
    /// The DID this request is for
    did: String,

    /// Device label (e.g., "Matt's Laptop")
    label: String,

    /// Ed25519 public key (hex encoded)
    ed25519_public_key: String,

    /// X25519 public key for encryption (hex encoded)
    x25519_public_key: String,

    /// Requested capabilities
    capabilities: Vec<Capability>,

    /// Timestamp when request was created
    created_at: u64,
}

fn handle_device_command(cmd: DeviceCommands, data_dir: &PathBuf) -> Result<()> {
    let keystore_path = get_keystore_path(data_dir);

    match cmd {
        DeviceCommands::List => {
            // Check if keystore exists
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' to create one.");
            }

            // Get passphrase and unlock
            let passphrase = read_passphrase("Enter passphrase: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;

            // Get DID document
            let did_doc = keystore.get_did_document()?;
            let device_id = keystore.get_device_id()?;

            println!("Identity: {}", did_doc.id);
            println!("Version: {}", did_doc.version);
            println!("Updated: {}", did_doc.updated_at);
            println!("\nDevices ({}):", did_doc.verification_method.len() / 2); // Ed25519 + X25519 per device
            println!();

            // Group verification methods by device (Ed25519 + X25519 pairs)
            let mut device_map: std::collections::HashMap<String, Vec<&icn_identity::VerificationMethod>> = std::collections::HashMap::new();

            for vm in &did_doc.verification_method {
                // Extract device number from id (e.g., "device-1" or "enc-1")
                let base_id = if vm.id.starts_with("device-") {
                    vm.id.clone()
                } else if vm.id.starts_with("enc-") {
                    format!("device-{}", &vm.id[4..])
                } else {
                    vm.id.clone()
                };

                device_map.entry(base_id).or_default().push(vm);
            }

            // Display devices in order
            let mut device_ids: Vec<_> = device_map.keys().collect();
            device_ids.sort();

            for dev_id in device_ids {
                let vms = &device_map[dev_id];

                // Find the signing key for this device
                let signing_vm = vms.iter().find(|vm| vm.key_type == KeyType::Ed25519);

                if let Some(vm) = signing_vm {
                    let current_marker = if dev_id == device_id { " (current device)" } else { "" };
                    let revoked_marker = if vm.revoked_at.is_some() { " [REVOKED]" } else { "" };

                    println!("Device: {}{}{}", vm.id, current_marker, revoked_marker);
                    println!("  Label: {}", vm.label);
                    println!("  Added: {}", vm.added_at);

                    if let Some(revoked) = vm.revoked_at {
                        println!("  Revoked: {revoked}");
                    }

                    println!("  Capabilities:");
                    for cap in &vm.capabilities {
                        println!("    - {cap:?}");
                    }

                    // Show associated encryption key
                    let enc_vm = vms.iter().find(|v| v.key_type == KeyType::X25519);
                    if enc_vm.is_some() {
                        println!("  Encryption: Yes");
                    }

                    println!();
                }
            }
        }

        DeviceCommands::Add { name } => {
            println!("Creating device-add request for '{name}'...\n");

            // Prompt for the target DID (the identity to add this device to)
            print!("Enter the DID to add this device to: ");
            io::stdout().flush()?;
            let mut did_input = String::new();
            io::stdin().read_line(&mut did_input)?;
            let target_did = did_input.trim();

            if !target_did.starts_with("did:icn:") {
                bail!("Invalid DID format. Expected: did:icn:<base58btc-key>");
            }

            println!("Target DID: {target_did}");
            println!();

            // Generate new Ed25519 keypair for this device
            println!("Generating Ed25519 keypair for this device...");
            let keypair = KeyPair::generate()?;

            // Generate X25519 encryption key
            use rand::rngs::OsRng;
            use x25519_dalek::{PublicKey, StaticSecret};

            let x25519_secret = StaticSecret::random_from_rng(OsRng);
            let x25519_public = PublicKey::from(&x25519_secret);

            println!("✓ Generated keys for new device");
            println!("  Ed25519 public key: {}", hex::encode(keypair.verifying_key().as_bytes()));
            println!();

            // Create request
            let request = DeviceAddRequest {
                did: target_did.to_string(),
                label: name.clone(),
                ed25519_public_key: hex::encode(keypair.verifying_key().as_bytes()),
                x25519_public_key: hex::encode(x25519_public.as_bytes()),
                capabilities: vec![
                    Capability::Sign,
                    Capability::Encrypt,
                ],
                created_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs(),
            };

            // Save request to file
            let request_file = data_dir.join(format!("device-add-{}.json", name.replace(" ", "-").to_lowercase()));
            let request_json = serde_json::to_string_pretty(&request)?;
            std::fs::create_dir_all(data_dir)?;
            std::fs::write(&request_file, request_json)?;

            println!("✓ Device-add request created: {}", request_file.display());
            println!();
            println!("Next steps:");
            println!("  1. Transfer {} to an authorized device for identity {}", request_file.display(), target_did);
            println!("  2. On authorized device, run:");
            println!("     icnctl device approve {}", request_file.display());
            println!();
            println!("⚠️  IMPORTANT:");
            println!("  • This device will be added to identity: {target_did}");
            println!("  • Do NOT create a new keystore on this device");
            println!("  • After approval, this device will share the same DID");
        }

        DeviceCommands::Approve { request_file } => {
            // Check if request file exists
            if !request_file.exists() {
                bail!("Request file not found: {}", request_file.display());
            }

            // Load request
            let request_json = std::fs::read_to_string(&request_file)
                .with_context(|| format!("Failed to read request file: {}", request_file.display()))?;

            let request: DeviceAddRequest = serde_json::from_str(&request_json)
                .context("Failed to parse device-add request")?;

            println!("Approving device-add request...\n");
            println!("  Label: {}", request.label);
            println!("  DID: {}", request.did);
            println!("  Requested at: {}", request.created_at);
            println!();

            // Check if keystore exists
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' to create one.");
            }

            // Get passphrase and unlock
            let passphrase = read_passphrase("Enter passphrase to approve: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;

            // Verify DID matches
            let own_did = keystore.get_keypair()?.did();
            if own_did.as_str() != request.did {
                bail!(
                    "DID mismatch: request is for {}, but your identity is {}",
                    request.did,
                    own_did
                );
            }

            // Get DID document and check capabilities
            let did_doc = keystore.get_did_document()?;
            let device_id = keystore.get_device_id()?;

            if !did_doc.has_capability(device_id, Capability::AddDevice) {
                bail!("This device does not have AddDevice capability");
            }

            // Determine next device ID
            let max_device_num = did_doc.verification_method
                .iter()
                .filter_map(|vm| {
                    if vm.id.starts_with("device-") {
                        vm.id[7..].parse::<u32>().ok()
                    } else {
                        None
                    }
                })
                .max()
                .unwrap_or(0);

            let new_device_id = format!("device-{}", max_device_num + 1);

            println!("Adding device as: {new_device_id}");

            // Decode public keys
            let ed25519_bytes = hex::decode(&request.ed25519_public_key)
                .context("Invalid Ed25519 public key encoding")?;
            let x25519_bytes = hex::decode(&request.x25519_public_key)
                .context("Invalid X25519 public key encoding")?;

            if ed25519_bytes.len() != 32 {
                bail!("Invalid Ed25519 key length: expected 32 bytes, got {}", ed25519_bytes.len());
            }
            if x25519_bytes.len() != 32 {
                bail!("Invalid X25519 key length: expected 32 bytes, got {}", x25519_bytes.len());
            }

            // Create rotation event for this device add (with both keys)
            let rotation_event = icn_identity::RotationEvent {
                did: own_did.clone(),
                event_type: icn_identity::RotationEventType::AddDeviceWithEncryption {
                    device_id: new_device_id.clone(),
                    label: request.label.clone(),
                    ed25519_public_key: ed25519_bytes.clone(),
                    x25519_public_key: x25519_bytes.clone(),
                    signing_capabilities: request.capabilities.clone(),
                },
                proof: vec![], // TODO: Sign this event with current device's key
                signed_by: device_id.to_string(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs(),
                new_version: did_doc.version + 1,
            };

            // Update DID document and save
            println!("Updating DID document...");
            let new_device_id_clone = new_device_id.clone();
            let request_label = request.label.clone();
            let request_caps = request.capabilities.clone();

            keystore.update_did_document(
                |did_doc| {
                    // Add both Ed25519 signing key and X25519 encryption key
                    // This increments the version only once, matching the rotation event
                    did_doc.add_device_with_encryption_key(
                        new_device_id_clone,
                        request_label,
                        ed25519_bytes,
                        x25519_bytes,
                        request_caps,
                    )?;

                    Ok(())
                },
                Some(rotation_event),
                &passphrase,
            )?;

            println!("✓ Device approved and added to DID document");
            println!("  Device ID: {new_device_id}");
            println!("  Label: {}", request.label);
            println!();
            println!("DID document updated:");
            let updated_doc = keystore.get_did_document()?;
            println!("  Version: {}", updated_doc.version);
            println!("  Devices: {}", updated_doc.verification_method.len() / 2);
        }

        DeviceCommands::Revoke { device_id, reason } => {
            // Check if keystore exists
            if !keystore_path.exists() {
                bail!("No identity found. Run 'icnctl id init' to create one.");
            }

            println!("Revoking device: {device_id}");
            if let Some(r) = &reason {
                println!("Reason: {r}");
            }
            println!();

            // Get passphrase and unlock
            let passphrase = read_passphrase("Enter passphrase to authorize revocation: ")?;
            let mut keystore = AgeKeyStore::open(&keystore_path)?;
            keystore.unlock(&passphrase)?;

            // Get DID document and check capabilities
            let did_doc = keystore.get_did_document()?;
            let current_device_id = keystore.get_device_id()?;

            if !did_doc.has_capability(current_device_id, Capability::RevokeDevice) {
                bail!("This device does not have RevokeDevice capability");
            }

            // Check device exists
            if did_doc.get_verification_method(&device_id).is_none() {
                bail!("Device '{device_id}' not found in DID document");
            }

            // Determine revocation reason
            use icn_identity::RevocationReason;
            let revocation_reason = match reason.as_deref() {
                Some("compromised") => RevocationReason::Compromised,
                Some("lost") => RevocationReason::Lost,
                Some("rotated") => RevocationReason::Rotated,
                _ => RevocationReason::Removed,
            };

            // Create rotation event for this device revocation
            let rotation_event = icn_identity::RotationEvent {
                did: keystore.get_keypair()?.did().clone(),
                event_type: icn_identity::RotationEventType::RevokeDevice {
                    device_id: device_id.clone(),
                    reason: revocation_reason,
                },
                proof: vec![], // TODO: Sign this event with current device's key
                signed_by: current_device_id.to_string(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs(),
                new_version: did_doc.version + 1,
            };

            // Update DID document
            let device_id_clone = device_id.clone();
            keystore.update_did_document(
                |did_doc| {
                    did_doc.revoke_device(&device_id_clone)?;
                    Ok(())
                },
                Some(rotation_event),
                &passphrase,
            )?;

            println!("✓ Device revoked");
            println!("  Device: {device_id}");
            if let Some(r) = reason {
                println!("  Reason: {r}");
            }
            println!();
            println!("DID document updated:");
            let updated_doc = keystore.get_did_document()?;
            println!("  Version: {}", updated_doc.version);
            println!("  Active devices: {}",
                updated_doc.verification_method.iter()
                    .filter(|vm| vm.revoked_at.is_none() && vm.key_type == KeyType::Ed25519)
                    .count());
        }
    }

    Ok(())
}

/// Backup metadata stored with the tarball
#[derive(Debug, Serialize, Deserialize)]
struct BackupMetadata {
    /// ICN version (from Cargo.toml)
    icn_version: String,
    /// Timestamp when backup was created
    created_at: u64,
    /// SHA256 checksum of the data directory content
    checksum: String,
}

fn handle_backup_command(data_dir: &PathBuf, output: &PathBuf) -> Result<()> {
    // Check if data directory exists
    if !data_dir.exists() {
        bail!("Data directory does not exist: {}", data_dir.display());
    }

    println!("Creating backup of {}...", data_dir.display());

    // Create output file
    let output_file = File::create(output)
        .with_context(|| format!("Failed to create backup file: {}", output.display()))?;

    // Create tar archive builder
    let mut tar_builder = Builder::new(output_file);

    // Add data directory to tarball
    println!("Archiving data directory...");
    tar_builder
        .append_dir_all(".", data_dir)
        .context("Failed to archive data directory")?;

    // Calculate checksum of the data directory
    println!("Calculating checksum...");
    let checksum = calculate_dir_checksum(data_dir)?;

    // Create metadata
    let metadata = BackupMetadata {
        icn_version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs(),
        checksum: checksum.clone(),
    };

    // Add metadata to tarball
    let metadata_json = serde_json::to_string_pretty(&metadata)?;
    let metadata_bytes = metadata_json.as_bytes();
    let mut header = tar::Header::new_gnu();
    header.set_size(metadata_bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar_builder
        .append_data(&mut header, "backup_metadata.json", metadata_bytes)
        .context("Failed to add metadata to backup")?;

    // Finish the tarball
    tar_builder.finish().context("Failed to finalize backup")?;

    println!("✓ Backup created successfully");
    println!("  Output: {}", output.display());
    println!("  ICN version: {}", metadata.icn_version);
    println!("  Checksum: {checksum}");
    println!();
    println!("IMPORTANT: Store this backup securely. It contains your identity keystore.");

    Ok(())
}

fn handle_restore_command(data_dir: &PathBuf, input: &PathBuf, force: bool) -> Result<()> {
    // Check if input backup file exists
    if !input.exists() {
        bail!("Backup file not found: {}", input.display());
    }

    // Check if data directory already exists
    if data_dir.exists() && !force {
        bail!(
            "Data directory already exists: {}. Use --force to overwrite.",
            data_dir.display()
        );
    }

    println!("Restoring backup from {}...", input.display());

    // Open the backup archive
    let input_file = File::open(input)
        .with_context(|| format!("Failed to open backup file: {}", input.display()))?;
    let mut archive = Archive::new(input_file);

    // Extract metadata first
    println!("Reading backup metadata...");
    let metadata = extract_backup_metadata(&mut archive, input)?;

    println!("Backup information:");
    println!("  ICN version: {}", metadata.icn_version);
    println!(
        "  Created: {}",
        chrono::DateTime::from_timestamp(metadata.created_at as i64, 0)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| "Unknown".to_string())
    );
    println!("  Checksum: {}", metadata.checksum);
    println!();

    // If force, backup existing data directory
    if data_dir.exists() {
        println!("Backing up existing data directory...");
        let backup_dir = format!("{}.backup-{}", data_dir.display(), metadata.created_at);
        std::fs::rename(data_dir, &backup_dir)
            .with_context(|| "Failed to backup existing data directory".to_string())?;
        println!("  Existing data moved to: {backup_dir}");
    }

    // Create data directory if it doesn't exist
    std::fs::create_dir_all(data_dir).context("Failed to create data directory")?;

    // Extract the archive (excluding metadata file)
    println!("Extracting backup...");
    let input_file = File::open(input)
        .with_context(|| format!("Failed to reopen backup file: {}", input.display()))?;
    let mut archive = Archive::new(input_file);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;

        // Skip the metadata file - we've already read it
        if path.to_string_lossy() == "backup_metadata.json" {
            continue;
        }

        entry.unpack_in(data_dir)?;
    }

    // Verify checksum
    println!("Verifying checksum...");
    let restored_checksum = calculate_dir_checksum(data_dir)?;
    if restored_checksum != metadata.checksum {
        bail!(
            "Checksum mismatch! Expected: {}, Got: {}. Restore may be corrupted.",
            metadata.checksum,
            restored_checksum
        );
    }

    println!("✓ Backup restored successfully");
    println!("  Restored to: {}", data_dir.display());
    println!("  Checksum verified: {restored_checksum}");
    println!();
    println!("You can now use 'icnctl id show' to verify your restored identity.");

    Ok(())
}

/// Calculate SHA256 checksum of all files in a directory
fn calculate_dir_checksum(dir: &PathBuf) -> Result<String> {
    use std::collections::BTreeMap;

    let mut file_hashes: BTreeMap<String, String> = BTreeMap::new();

    // Walk directory and hash each file
    for entry in walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            let path = entry.path();
            let relative_path = path
                .strip_prefix(dir)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();

            // Read file and calculate hash
            let mut file = File::open(path)
                .with_context(|| format!("Failed to open file for hashing: {}", path.display()))?;
            let mut hasher = Sha256::new();
            std::io::copy(&mut file, &mut hasher)?;
            let hash = format!("{:x}", hasher.finalize());

            file_hashes.insert(relative_path, hash);
        }
    }

    // Create combined hash of all file hashes
    let mut combined_hasher = Sha256::new();
    for (path, hash) in file_hashes {
        combined_hasher.update(path.as_bytes());
        combined_hasher.update(hash.as_bytes());
    }

    Ok(format!("{:x}", combined_hasher.finalize()))
}

/// Extract backup metadata from the archive
fn extract_backup_metadata(_archive: &mut Archive<File>, input: &PathBuf) -> Result<BackupMetadata> {
    // Re-open to read metadata
    let input_file = File::open(input)
        .with_context(|| format!("Failed to reopen backup file: {}", input.display()))?;
    let mut archive = Archive::new(input_file);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;

        if path.to_string_lossy() == "backup_metadata.json" {
            let mut metadata_json = String::new();
            entry.read_to_string(&mut metadata_json)?;
            let metadata: BackupMetadata = serde_json::from_str(&metadata_json)
                .context("Failed to parse backup metadata")?;
            return Ok(metadata);
        }
    }

    bail!("Backup metadata not found in archive. This may not be a valid ICN backup.");
}

// RPC client helpers

#[derive(Serialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    params: serde_json::Value,
}

#[derive(Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: u64,
    result: Option<serde_json::Value>,
    error: Option<JsonRpcError>,
}

#[derive(Deserialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

fn rpc_call(endpoint: &str, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
    let url = format!("http://{endpoint}/rpc");
    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: 1,
        method: method.to_string(),
        params,
    };

    let client = reqwest::blocking::Client::new();
    let response: JsonRpcResponse = client
        .post(&url)
        .json(&request)
        .send()
        .context("Failed to connect to daemon. Is icnd running?")?
        .json()
        .context("Failed to parse RPC response")?;

    if let Some(error) = response.error {
        bail!("RPC error ({}): {}", error.code, error.message);
    }

    response
        .result
        .ok_or_else(|| anyhow::anyhow!("RPC response missing result"))
}

fn handle_gov_command(cmd: GovCommands, _data_dir: &PathBuf, endpoint: &str) -> Result<()> {
    match cmd {
        GovCommands::Domain(domain_cmd) => match domain_cmd {
            DomainCommands::Create {
                domain_id,
                name,
                members,
                profile,
                quorum,
                approval,
                voting_period,
            } => {
                // Parse member DIDs as strings
                let member_list: Vec<String> = members
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect();

                if member_list.is_empty() {
                    bail!("At least one member DID is required");
                }

                // Build RPC request
                let params = serde_json::json!({
                    "domain_id": domain_id,
                    "name": name,
                    "profile": profile,
                    "params": {
                        "quorum_percentage": quorum,
                        "approval_threshold_percentage": approval,
                        "voting_period_seconds": voting_period,
                    },
                    "membership": {
                        "type": "static_list",
                        "members": member_list,
                    },
                });

                rpc_call(endpoint, "governance.domain.create", params)?;

                println!("✓ Governance domain created:");
                println!("  ID: {domain_id}");
                println!("  Name: {name}");
                println!("  Members: {}", member_list.len());
                println!("  Profile: {profile}");
                println!("  Quorum: {quorum}%");
                println!("  Approval: {approval}%");
                println!("  Voting period: {voting_period} seconds");
            }

            DomainCommands::Show { domain_id } => {
                let params = serde_json::json!({ "domain_id": domain_id });
                let result = rpc_call(endpoint, "governance.domain.get", params)?;
                let domain = result.as_object().context("Invalid domain data")?;

                println!("Governance Domain:");
                println!("  ID: {}", domain.get("id").and_then(|v| v.as_str()).unwrap_or("unknown"));
                println!("  Name: {}", domain.get("name").and_then(|v| v.as_str()).unwrap_or("unnamed"));
                println!("  Profile: {}", domain.get("profile").and_then(|v| v.as_str()).unwrap_or("unknown"));

                if let Some(params_obj) = domain.get("params").and_then(|v| v.as_object()) {
                    println!("  Quorum: {}%", params_obj.get("quorum_percentage").and_then(|v| v.as_u64()).unwrap_or(0));
                    println!("  Approval: {}%", params_obj.get("approval_threshold_percentage").and_then(|v| v.as_u64()).unwrap_or(0));
                    println!("  Voting period: {} seconds", params_obj.get("voting_period_seconds").and_then(|v| v.as_u64()).unwrap_or(0));
                }

                println!("  Membership: {}", domain.get("membership_type").and_then(|v| v.as_str()).unwrap_or("unknown"));
            }

            DomainCommands::List => {
                let result = rpc_call(endpoint, "governance.domain.list", serde_json::json!({}))?;
                let domains: Vec<serde_json::Value> = serde_json::from_value(result)
                    .context("Failed to parse domain list")?;

                println!("Governance Domains:");
                for domain in domains {
                    let id = domain["id"].as_str().unwrap_or("unknown");
                    let name = domain["name"].as_str().unwrap_or("unnamed");
                    println!("  - {id} ({name})");
                }
            }
        },

        GovCommands::Proposal(proposal_cmd) => match proposal_cmd {
            ProposalCommands::Create {
                domain_id,
                title,
                description,
                kind,
                body,
                amount,
                currency,
                recipient,
                purpose,
                member,
                action,
                new_config,
            } => {
                // Build payload JSON for RPC (daemon handles proposer DID)
                let payload_json = match kind.as_str() {
                    "text" => {
                        let body_text = body.context("--body required for text proposals")?;
                        serde_json::json!({
                            "type": "text",
                            "body": body_text
                        })
                    }
                    "budget" => {
                        let amt = amount.context("--amount required for budget proposals")?;
                        let curr = currency.context("--currency required for budget proposals")?;
                        let recip_str = recipient.context("--recipient required for budget proposals")?;
                        let purp = purpose.unwrap_or_default();

                        serde_json::json!({
                            "type": "budget",
                            "amount": amt,
                            "currency": curr,
                            "recipient": recip_str,
                            "purpose": purp
                        })
                    }
                    "membership" => {
                        let member_str = member.context("--member required for membership proposals")?;
                        let action_str = action.unwrap_or_else(|| "add".to_string());

                        if action_str != "add" && action_str != "remove" {
                            bail!("Invalid action: must be 'add' or 'remove'");
                        }

                        serde_json::json!({
                            "type": "membership",
                            "action": action_str,
                            "member": member_str
                        })
                    }
                    "config-change" => {
                        let cfg = new_config.context("--new-config required for config-change proposals")?;
                        serde_json::json!({
                            "type": "config_change",
                            "new_config": cfg
                        })
                    }
                    _ => bail!("Invalid proposal kind. Must be: text, budget, membership, config-change"),
                };

                // Create proposal via RPC
                let params = serde_json::json!({
                    "domain_id": domain_id,
                    "title": title,
                    "description": description,
                    "payload": payload_json
                });

                let result = rpc_call(endpoint, "governance.proposal.create", params)?;
                let proposal_id = result["proposal_id"].as_str().context("Missing proposal_id in response")?;

                println!("✓ Proposal created:");
                println!("  ID: {proposal_id}");
                println!("  State: Draft");
            }

            ProposalCommands::Open {
                proposal_id,
                duration,
            } => {
                // Get proposal to find its domain
                let get_params = serde_json::json!({ "proposal_id": proposal_id });
                let proposal_data = rpc_call(endpoint, "governance.proposal.get", get_params)?;
                let domain_id = proposal_data["domain_id"].as_str().context("Missing domain_id")?;

                // Get domain to determine voting period
                let domain_params = serde_json::json!({ "domain_id": domain_id });
                let domain_data = rpc_call(endpoint, "governance.domain.get", domain_params)?;
                let default_period = domain_data["params"]["voting_period_seconds"]
                    .as_u64()
                    .unwrap_or(86400);

                let voting_period = duration.unwrap_or(default_period);

                // Open the proposal
                let open_params = serde_json::json!({
                    "proposal_id": proposal_id,
                    "voting_period_seconds": voting_period
                });
                rpc_call(endpoint, "governance.proposal.open", open_params)?;

                println!("✓ Proposal opened for voting:");
                println!("  ID: {proposal_id}");
                println!("  Duration: {voting_period} seconds");
            }

            ProposalCommands::List { domain_id, state } => {
                println!("Proposals in domain '{domain_id}':");

                let result = rpc_call(endpoint, "governance.proposal.list", serde_json::json!({}))?;
                let proposals: Vec<serde_json::Value> = serde_json::from_value(result)
                    .context("Failed to parse proposal list")?;

                for proposal in proposals {
                    let prop_domain_id = proposal["domain_id"].as_str().unwrap_or("");
                    if prop_domain_id != domain_id {
                        continue;
                    }

                    let prop_state = proposal["state"].as_str().unwrap_or("unknown");

                    if let Some(ref state_filter) = state {
                        if prop_state != *state_filter {
                            continue;
                        }
                    }

                    let state_upper = prop_state.to_uppercase();
                    let id = proposal["id"].as_str().unwrap_or("unknown");
                    let title = proposal["title"].as_str().unwrap_or("untitled");

                    println!("  [{state_upper}] {id} - {title}");
                }
            }

            ProposalCommands::Show { proposal_id } => {
                let params = serde_json::json!({ "proposal_id": proposal_id });
                let proposal = rpc_call(endpoint, "governance.proposal.get", params)?;

                println!("Proposal:");
                println!("  ID: {}", proposal["id"].as_str().unwrap_or("unknown"));
                println!("  Title: {}", proposal["title"].as_str().unwrap_or("untitled"));
                println!("  Description: {}", proposal["description"].as_str().unwrap_or(""));
                println!("  State: {}", proposal["state"].as_str().unwrap_or("unknown"));
                println!("  Proposer: {}", proposal["proposer"].as_str().unwrap_or("unknown"));
                println!("  Domain: {}", proposal["domain_id"].as_str().unwrap_or("unknown"));

                if let Some(opened_at) = proposal["opened_at"].as_u64() {
                    println!("  Opened at: {opened_at} (Unix timestamp)");
                }
                if let Some(closes_at) = proposal["closes_at"].as_u64() {
                    println!("  Closes at: {closes_at} (Unix timestamp)");
                }
                if let Some(closed_at) = proposal["closed_at"].as_u64() {
                    println!("  Closed at: {closed_at} (Unix timestamp)");
                }
            }

            ProposalCommands::Close { proposal_id } => {
                let params = serde_json::json!({ "proposal_id": proposal_id });
                rpc_call(endpoint, "governance.proposal.close", params)?;

                println!("✓ Proposal closed:");
                println!("  ID: {proposal_id}");
                println!("  The daemon has evaluated votes and determined the outcome.");
            }

            ProposalCommands::Cancel { proposal_id: _ } => {
                bail!("Cancel command not yet supported via RPC. Use 'proposal close' instead, or stop the daemon and modify the store directly.");
            }
        },

        GovCommands::Vote(vote_cmd) => match vote_cmd {
            VoteCommands::Cast {
                proposal_id,
                choice,
                comment,
            } => {
                // Validate choice
                let choice_lower = choice.to_lowercase();
                if choice_lower != "for" && choice_lower != "against" && choice_lower != "abstain" {
                    bail!("Invalid choice. Must be: for, against, abstain");
                }

                // Cast vote via RPC
                let params = serde_json::json!({
                    "proposal_id": proposal_id,
                    "choice": choice_lower,
                    "comment": comment
                });

                rpc_call(endpoint, "governance.vote.cast", params)?;

                println!("✓ Vote recorded:");
                println!("  Proposal: {proposal_id}");
                println!("  Choice: {choice_lower}");
            }

            VoteCommands::Show { proposal_id } => {
                bail!("Vote show command not yet supported via RPC. Use 'proposal show {proposal_id}' to see the proposal.");
            }
        },
    }

    Ok(())
}

fn handle_snapshot_command(cmd: SnapshotCommands, data_dir: &PathBuf) -> Result<()> {
    // Snapshots are stored in the store subdirectory
    let store_dir = data_dir.join("store");

    match cmd {
        SnapshotCommands::Create => {
            println!("Creating manual snapshot...");

            // Check if store directory exists
            if !store_dir.exists() {
                bail!("Store directory does not exist: {}. Has the daemon been run?", store_dir.display());
            }

            // Load current snapshot (if it exists)
            match icn_snapshot::load_snapshot(&store_dir) {
                Ok(Some(snapshot)) => {
                    // Create timestamped backup
                    icn_snapshot::save_timestamped_snapshot(&snapshot, &store_dir)
                        .context("Failed to create timestamped snapshot")?;

                    // Also save as main snapshot with updated checksum
                    icn_snapshot::save_snapshot(&snapshot, &store_dir)
                        .context("Failed to save snapshot with checksum")?;

                    println!("✓ Snapshot created successfully");
                    println!("  Location: {}/state.snapshot.{}",
                             store_dir.display(), snapshot.created_at);

                    let gossip_peers = snapshot.gossip_state.as_ref()
                        .map_or(0, |g| g.vector_clock.len());
                    let network_peers = snapshot.network_state.as_ref()
                        .map_or(0, |n| n.peer_x25519_keys.len());

                    println!("  Gossip peers: {gossip_peers}");
                    println!("  Network peers: {network_peers}");
                    println!("  SHA256 checksum: generated");
                }
                Ok(None) => {
                    println!("⚠ No snapshot exists yet. Start the daemon to generate initial state.");
                }
                Err(e) => {
                    bail!("Failed to load snapshot: {e}");
                }
            }
        }

        SnapshotCommands::List => {
            println!("Available snapshots in {}:", store_dir.display());
            println!();

            match icn_snapshot::list_snapshots(&store_dir) {
                Ok(snapshots) => {
                    if snapshots.is_empty() {
                        println!("No snapshots found.");
                    } else {
                        println!("{:<30} {:<20} {:<15}", "Snapshot", "Created", "Size");
                        println!("{}", "-".repeat(65));

                        for (filename, timestamp, size) in snapshots {
                            // Format timestamp as human-readable date
                            let datetime = std::time::UNIX_EPOCH + std::time::Duration::from_secs(timestamp);
                            let formatted_date = format!("{datetime:?}");

                            // Format size in KB/MB
                            let formatted_size = if size < 1024 {
                                format!("{size} B")
                            } else if size < 1024 * 1024 {
                                format!("{:.1} KB", size as f64 / 1024.0)
                            } else {
                                format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
                            };

                            println!("{:<30} {:<20} {:<15}",
                                     filename,
                                     &formatted_date[..std::cmp::min(19, formatted_date.len())],
                                     formatted_size);
                        }

                        println!();
                        println!("Use 'icnctl snapshot verify <snapshot>' to check integrity");
                    }
                }
                Err(e) => {
                    bail!("Failed to list snapshots: {e}");
                }
            }
        }

        SnapshotCommands::Verify { snapshot } => {
            let snapshot_name = snapshot.unwrap_or_else(|| "state.snapshot".to_string());
            println!("Verifying snapshot: {snapshot_name}");

            // If a specific snapshot was given, we need to temporarily copy it
            if snapshot_name != "state.snapshot" {
                // TODO: Implement verification for timestamped snapshots
                // For now, just verify the main snapshot
                println!("⚠ Verifying timestamped snapshots not yet implemented.");
                println!("  Verifying main snapshot instead...");
            }

            match icn_snapshot::verify_snapshot(&store_dir) {
                Ok(()) => {
                    println!("✓ Snapshot checksum verified successfully");

                    // Also load and display info
                    if let Ok(Some(snapshot)) = icn_snapshot::load_snapshot(&store_dir) {
                        println!();
                        println!("Snapshot details:");
                        println!("  Created: {}", snapshot.created_at);

                        let gossip_peers = snapshot.gossip_state.as_ref()
                            .map_or(0, |g| g.vector_clock.len());
                        let gossip_topics = snapshot.gossip_state.as_ref()
                            .map_or(0, |g| g.subscriptions.len());
                        let network_peers = snapshot.network_state.as_ref()
                            .map_or(0, |n| n.peer_x25519_keys.len());

                        println!("  Gossip peers: {gossip_peers}");
                        println!("  Gossip topics: {gossip_topics}");
                        println!("  Network peers: {network_peers}");
                    }
                }
                Err(e) => {
                    println!("✗ Snapshot verification failed!");
                    bail!("{e}");
                }
            }
        }

        SnapshotCommands::Delete { snapshot } => {
            println!("Deleting snapshot: {snapshot}");

            let snapshot_path = store_dir.join(&snapshot);
            let checksum_path = store_dir.join(format!("{snapshot}.sha256"));

            if !snapshot_path.exists() {
                bail!("Snapshot not found: {}", snapshot_path.display());
            }

            // Delete snapshot file
            std::fs::remove_file(&snapshot_path)
                .with_context(|| format!("Failed to delete snapshot: {}", snapshot_path.display()))?;

            // Delete checksum file if it exists
            if checksum_path.exists() {
                std::fs::remove_file(&checksum_path)
                    .with_context(|| format!("Failed to delete checksum: {}", checksum_path.display()))?;
            }

            println!("✓ Snapshot deleted successfully");
        }

        SnapshotCommands::Cleanup { keep } => {
            println!("Cleaning up old snapshots (keeping {keep} most recent)...");

            match icn_snapshot::cleanup_old_snapshots(&store_dir, keep) {
                Ok(deleted) => {
                    if deleted > 0 {
                        println!("✓ Deleted {deleted} old snapshot(s)");
                    } else {
                        println!("No snapshots to delete (under retention limit)");
                    }
                }
                Err(e) => {
                    bail!("Failed to cleanup snapshots: {e}");
                }
            }
        }
    }

    Ok(())
}

/// Handle authentication commands
async fn handle_auth_command(cmd: AuthCommands, data_dir: &PathBuf) -> Result<()> {
    match cmd {
        AuthCommands::Token { gateway, coop_id, scopes } => {
            // Get keystore path and unlock
            let keystore_path = get_keystore_path(data_dir);
            if !keystore_path.exists() {
                bail!("No keystore found. Run 'icnctl id init' first.");
            }

            // Prompt for passphrase
            let passphrase = rpassword::prompt_password("Keystore passphrase: ")
                .context("Failed to read passphrase")?;

            // Unlock keystore
            let mut keystore = icn_identity::keystore::AgeKeyStore::open(&keystore_path)
                .context("Failed to open keystore")?;
            keystore.unlock(passphrase.as_bytes())
                .context("Failed to unlock keystore")?;

            let bundle = keystore.get_identity_bundle()
                .context("Keystore is locked")?;
            let did = bundle.did().to_string();
            let keypair = bundle.keypair();

            // Parse scopes
            let scope_list: Vec<String> = scopes.split(',').map(|s| s.trim().to_string()).collect();

            println!("Getting token for DID: {did}");
            println!("Gateway: {gateway}");
            println!("Cooperative: {coop_id}");
            println!("Scopes: {}", scope_list.join(", "));
            println!();

            // Create HTTP client
            let client = reqwest::Client::new();

            // Step 1: Get challenge
            let challenge_url = format!("{gateway}/v1/auth/challenge");
            let challenge_req = serde_json::json!({
                "did": did
            });

            let challenge_resp = client
                .post(&challenge_url)
                .json(&challenge_req)
                .send()
                .await
                .context("Failed to connect to gateway")?;

            if !challenge_resp.status().is_success() {
                let status = challenge_resp.status();
                let body = challenge_resp.text().await.unwrap_or_default();
                bail!("Failed to get challenge: {status} - {body}");
            }

            let challenge_data: serde_json::Value = challenge_resp
                .json()
                .await
                .context("Failed to parse challenge response")?;

            let challenge = challenge_data["challenge"]
                .as_str()
                .context("Missing challenge in response")?;

            // Step 2: Sign the challenge
            let signature = keypair.sign(challenge.as_bytes());
            let signature_hex = hex::encode(signature.to_bytes());

            // Step 3: Verify signature and get token
            let verify_url = format!("{gateway}/v1/auth/verify");
            let verify_req = serde_json::json!({
                "did": did,
                "signature": signature_hex,
                "coop_id": coop_id,
                "scopes": scope_list
            });

            let verify_resp = client
                .post(&verify_url)
                .json(&verify_req)
                .send()
                .await
                .context("Failed to verify signature")?;

            if !verify_resp.status().is_success() {
                let status = verify_resp.status();
                let body = verify_resp.text().await.unwrap_or_default();
                bail!("Failed to verify: {status} - {body}");
            }

            let token_data: serde_json::Value = verify_resp
                .json()
                .await
                .context("Failed to parse token response")?;

            let token = token_data["token"]
                .as_str()
                .context("Missing token in response")?;

            let expires_at = token_data["expires_at"]
                .as_i64()
                .unwrap_or(0);

            let expiry_time = chrono::DateTime::from_timestamp(expires_at, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                .unwrap_or_else(|| "unknown".to_string());

            println!("✓ Token obtained successfully!");
            println!();
            println!("Token (copy this to use with web UI):");
            println!("────────────────────────────────────────");
            println!("{token}");
            println!("────────────────────────────────────────");
            println!();
            println!("Expires: {expiry_time}");
        }
    }

    Ok(())
}

/// Interactive wizard for setting up a new cooperative
async fn handle_init_coop_command(
    data_dir: &PathBuf,
    name: Option<String>,
    members: Option<String>,
    yes: bool,
    no_start: bool,
) -> Result<()> {
    println!();
    println!("╔════════════════════════════════════════╗");
    println!("║   ICN Cooperative Setup Wizard         ║");
    println!("╚════════════════════════════════════════╝");
    println!();

    // Step 1: Get cooperative name
    let coop_name = if let Some(n) = name {
        n
    } else {
        print!("Cooperative name: ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        input.trim().to_string()
    };

    if coop_name.is_empty() {
        bail!("Cooperative name cannot be empty");
    }

    // Generate a domain ID from the name
    let domain_id = coop_name
        .to_lowercase()
        .replace(' ', "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect::<String>();

    println!("  Name: {coop_name}");
    println!("  Domain ID: {domain_id}");
    println!();

    // Step 2: Check/create identity
    let keystore_path = get_keystore_path(data_dir);
    let my_did = if keystore_path.exists() {
        // Use existing identity
        println!("Step 1: Using existing identity");
        let passphrase = read_passphrase("Enter passphrase: ")?;
        let mut keystore = AgeKeyStore::open(&keystore_path)?;
        keystore.unlock(&passphrase)?;
        let did = keystore.get_keypair()?.did().clone();
        println!("  DID: {did}");
        println!();
        did
    } else {
        // Create new identity
        println!("Step 1: Creating new identity");
        std::fs::create_dir_all(data_dir)
            .context("Failed to create data directory")?;

        print!("Choose a passphrase: ");
        io::stdout().flush()?;
        let passphrase1 = rpassword::read_password()?;

        print!("Confirm passphrase: ");
        io::stdout().flush()?;
        let passphrase2 = rpassword::read_password()?;

        if passphrase1 != passphrase2 {
            bail!("Passphrases do not match");
        }
        if passphrase1.len() < 8 {
            bail!("Passphrase must be at least 8 characters");
        }

        let passphrase = Zeroizing::new(passphrase1.into_bytes());

        // Initialize keystore (generates keypair internally)
        let keystore = AgeKeyStore::init(&keystore_path, &passphrase)?;
        let did = keystore.get_keypair()?.did().clone();

        println!("  DID: {did}");
        println!("  Keystore: {}", keystore_path.display());
        println!();
        did
    };

    // Step 3: Parse initial members
    let mut member_dids = vec![my_did.clone()];

    if let Some(members_str) = members {
        println!("Step 2: Adding initial members");
        for member_str in members_str.split(',') {
            let member_str = member_str.trim();
            if member_str.is_empty() {
                continue;
            }
            let member_did = Did::from_str(member_str)
                .with_context(|| format!("Invalid DID: {member_str}"))?;
            if !member_dids.contains(&member_did) {
                member_dids.push(member_did);
            }
        }
    } else if !yes {
        println!("Step 2: Add initial members (or press Enter to skip)");
        println!("  Enter member DIDs, one per line. Empty line to finish:");

        loop {
            print!("  > ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let input = input.trim();

            if input.is_empty() {
                break;
            }

            match Did::from_str(input) {
                Ok(member_did) => {
                    if !member_dids.contains(&member_did) {
                        member_dids.push(member_did);
                        println!("    Added: {input}");
                    } else {
                        println!("    Already added: {input}");
                    }
                }
                Err(e) => {
                    println!("    Invalid DID: {e}");
                }
            }
        }
    }

    println!();
    println!("Initial members ({}):", member_dids.len());
    for (i, did) in member_dids.iter().enumerate() {
        let marker = if did == &my_did { " (you)" } else { "" };
        println!("  {}. {}{}", i + 1, did, marker);
    }
    println!();

    // Step 4: Create configuration file
    let config_path = data_dir.join("icn.toml");
    if !config_path.exists() {
        println!("Step 3: Creating configuration");
        let config_content = format!(r#"# ICN Configuration for {coop_name}
# Generated by icnctl init-coop

data_dir = "{}"

[network]
mdns_enabled = true
listen_addr = "0.0.0.0:7777"
rpc_port = 5601

[observability]
metrics_port = 9090
health_port = 8080
log_level = "info"

[rate_limiting]
enabled = true
refill_interval_ms = 100

[rate_limiting.isolated]
max_messages_per_second = 10
burst_capacity = 2

[rate_limiting.known]
max_messages_per_second = 50
burst_capacity = 10

[rate_limiting.partner]
max_messages_per_second = 100
burst_capacity = 20

[rate_limiting.federated]
max_messages_per_second = 200
burst_capacity = 50

[gateway]
enabled = true
bind_addr = "127.0.0.1:8080"
# jwt_secret = "CHANGE_ME"  # Set this before starting!
token_expiry_hours = 24
"#, data_dir.display());

        std::fs::write(&config_path, config_content)
            .context("Failed to write configuration")?;
        println!("  Config: {}", config_path.display());
    } else {
        println!("Step 3: Using existing configuration");
        println!("  Config: {}", config_path.display());
    }
    println!();

    // Step 5: Show summary and confirm
    println!("════════════════════════════════════════");
    println!("Summary:");
    println!("  Cooperative: {coop_name}");
    println!("  Domain ID: {domain_id}");
    println!("  Your DID: {my_did}");
    println!("  Members: {}", member_dids.len());
    println!("  Data dir: {}", data_dir.display());
    println!("════════════════════════════════════════");
    println!();

    if !yes {
        print!("Proceed with setup? (Y/n): ");
        io::stdout().flush()?;
        let mut response = String::new();
        io::stdin().read_line(&mut response)?;
        if response.trim().eq_ignore_ascii_case("n") {
            println!("Setup cancelled.");
            return Ok(());
        }
    }

    // Step 6: Create initial governance domain
    // Note: This would normally be done via RPC to a running daemon.
    // For now, we just prepare the governance setup that will be
    // executed when the daemon starts.
    let governance_setup_path = data_dir.join("governance_setup.json");
    let governance_setup = serde_json::json!({
        "domain": {
            "id": domain_id,
            "name": coop_name,
            "members": member_dids.iter().map(|d| d.to_string()).collect::<Vec<_>>(),
            "profile": "cooperative_default"
        }
    });

    std::fs::write(
        &governance_setup_path,
        serde_json::to_string_pretty(&governance_setup)?,
    )
    .context("Failed to write governance setup")?;

    println!("✓ Governance domain configured");
    println!("  Domain ID: {domain_id}");
    println!("  Profile: cooperative_default (1-member-1-vote, 50% quorum)");
    println!();

    // Step 7: Create trust edges for initial members
    let store_path = get_store_path(data_dir);
    std::fs::create_dir_all(&store_path)?;
    let store = SledStore::open(&store_path).context("Failed to open store")?;
    let store = Arc::new(store);
    let mut trust_graph = TrustGraph::new(store, my_did.clone());

    for member_did in &member_dids {
        if member_did != &my_did {
            // Add bidirectional trust at "partner" level (0.5)
            let edge = TrustEdge::new(my_did.clone(), member_did.clone(), 0.5);
            trust_graph.add_edge(edge)?;
        }
    }

    println!("✓ Trust edges created for {} member(s)", member_dids.len() - 1);
    println!();

    // Step 8: Final instructions
    println!("════════════════════════════════════════");
    println!("  Setup Complete!");
    println!("════════════════════════════════════════");
    println!();

    if no_start {
        println!("Next steps:");
        println!();
        println!("  1. Edit configuration:");
        println!("     $EDITOR {}", config_path.display());
        println!();
        println!("  2. Set JWT secret for gateway:");
        println!("     export ICN_GATEWAY_JWT_SECRET=\"your-secret-here\"");
        println!();
        println!("  3. Start the daemon:");
        println!("     icnd --config {}", config_path.display());
        println!();
        println!("  4. Create the governance domain:");
        println!("     icnctl gov domain create --domain-id {domain_id} --name \"{coop_name}\" \\");
        let members_str = member_dids
            .iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join(",");
        println!("       --members \"{members_str}\"");
        println!();
        println!("  5. Share the invite info with other members:");
        println!("     Your DID: {my_did}");
        println!("     Domain ID: {domain_id}");
    } else {
        // TODO: Actually start the daemon
        println!("Note: Automatic daemon start not yet implemented.");
        println!();
        println!("To complete setup:");
        println!();
        println!("  1. Set JWT secret:");
        println!("     export ICN_GATEWAY_JWT_SECRET=\"your-secret-here\"");
        println!();
        println!("  2. Start the daemon:");
        println!("     icnd --config {}", config_path.display());
        println!();
        println!("  3. Create governance domain (after daemon starts):");
        println!("     icnctl gov domain create --domain-id {domain_id} --name \"{coop_name}\" \\");
        let members_str = member_dids
            .iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join(",");
        println!("       --members \"{members_str}\"");
    }

    println!();
    println!("Documentation: https://icn.coop/docs");
    println!();

    Ok(())
}

fn handle_compute_command(cmd: ComputeCommands, endpoint: &str) -> Result<()> {
    match cmd {
        ComputeCommands::Submit {
            contract,
            id,
            fuel,
            priority,
            inputs,
            payment_rate,
            payment_currency,
        } => {
            // Read contract JSON
            let contract_json = std::fs::read_to_string(&contract)
                .with_context(|| format!("Failed to read contract file: {contract:?}"))?;

            // Read inputs if provided
            let inputs_value: serde_json::Value = if let Some(inputs_path) = inputs {
                let inputs_json = std::fs::read_to_string(&inputs_path)
                    .with_context(|| format!("Failed to read inputs file: {inputs_path:?}"))?;
                serde_json::from_str(&inputs_json)?
            } else {
                serde_json::Value::Null
            };

            let task_id = id.unwrap_or_else(|| format!("task-{}", uuid::Uuid::new_v4()));

            let params = serde_json::json!({
                "task_id": task_id,
                "code": contract_json,
                "inputs": inputs_value,
                "fuel_limit": fuel,
                "priority": priority,
                "payment_rate": payment_rate,
                "payment_currency": payment_currency,
            });

            let result = rpc_call(endpoint, "compute.submit", params)?;

            let task_hash = result
                .get("task_hash")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            println!("Task submitted successfully!");
            println!("Task ID:   {task_id}");
            println!("Task hash: {task_hash}");
            println!();
            println!("Check status with:");
            println!("  icnctl compute status {task_hash}");
        }

        ComputeCommands::Status { task_hash } => {
            let params = serde_json::json!({ "task_hash": task_hash });
            let result = rpc_call(endpoint, "compute.status", params)?;

            let status = result.get("status").and_then(|v| v.as_str()).unwrap_or("unknown");
            println!("Task:   {task_hash}");
            println!("Status: {status}");

            if let Some(executor) = result.get("executor").and_then(|v| v.as_str()) {
                println!("Executor: {executor}");
            }

            if let Some(task_result) = result.get("result") {
                let outcome = task_result.get("outcome").and_then(|v| v.as_str()).unwrap_or("unknown");
                let fuel_used = task_result.get("fuel_used").and_then(|v| v.as_u64()).unwrap_or(0);
                let duration_ms = task_result.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(0);

                println!();
                println!("Result:");
                println!("  Outcome:     {outcome}");
                println!("  Fuel used:   {fuel_used}");
                println!("  Duration:    {duration_ms}ms");

                if let Some(output) = task_result.get("output") {
                    if !output.is_null() {
                        println!("  Output:      {}", serde_json::to_string_pretty(output)?);
                    }
                }

                if let Some(error) = task_result.get("error").and_then(|v| v.as_str()) {
                    println!("  Error:       {error}");
                }
            }
        }

        ComputeCommands::Cancel { task_hash, reason } => {
            let params = serde_json::json!({
                "task_hash": task_hash,
                "reason": reason,
            });
            let result = rpc_call(endpoint, "compute.cancel", params)?;

            let status = result.get("status").and_then(|v| v.as_str()).unwrap_or("unknown");
            println!("Task cancelled successfully!");
            println!("Task hash: {task_hash}");
            println!("Status:    {status}");
            println!("Reason:    {reason}");
        }
    }

    Ok(())
}

fn handle_policy_command(cmd: PolicyCommands, endpoint: &str) -> Result<()> {
    match cmd {
        PolicyCommands::Set { coop_id, policy } => {
            // Read policy JSON
            let policy_json = std::fs::read_to_string(&policy)
                .with_context(|| format!("Failed to read policy file: {policy:?}"))?;

            let policy_value: serde_json::Value = serde_json::from_str(&policy_json)?;

            let params = serde_json::json!({
                "coop_id": coop_id,
                "policy": policy_value,
            });

            rpc_call(endpoint, "policy.set", params)?;

            println!("✓ Policy set for cooperative: {coop_id}");
            println!();
            println!("View policy with:");
            println!("  icnctl policy show --coop-id {coop_id}");
        }

        PolicyCommands::Show { coop_id } => {
            let params = serde_json::json!({ "coop_id": coop_id });
            let result = rpc_call(endpoint, "policy.get", params)?;

            if result.is_null() {
                println!("No policy set for cooperative: {coop_id}");
                return Ok(());
            }

            println!("Policy for cooperative: {coop_id}");
            println!();
            println!("{}", serde_json::to_string_pretty(&result)?);
        }

        PolicyCommands::List => {
            let params = serde_json::json!({});
            let result = rpc_call(endpoint, "policy.list", params)?;

            let policies = result
                .as_array()
                .context("Expected array of policies")?;

            if policies.is_empty() {
                println!("No policies configured");
                return Ok(());
            }

            println!("Configured Policies:");
            for policy in policies {
                let coop_id = policy.get("coop_id").and_then(|v| v.as_str()).unwrap_or("unknown");
                let enforcement_mode = policy.get("enforcement_mode").and_then(|v| v.as_str()).unwrap_or("unknown");
                let rules_count = policy.get("rules").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
                println!("  - {coop_id}: {rules_count} rules ({enforcement_mode})");
            }
        }

        PolicyCommands::Remove { coop_id } => {
            let params = serde_json::json!({ "coop_id": coop_id });
            rpc_call(endpoint, "policy.remove", params)?;

            println!("✓ Policy removed for cooperative: {coop_id}");
        }
    }

    Ok(())
}

fn handle_quota_command(cmd: QuotaCommands, endpoint: &str) -> Result<()> {
    match cmd {
        QuotaCommands::Show { coop_id, member } => {
            let params = serde_json::json!({
                "coop_id": coop_id,
                "member_did": member,
            });
            let result = rpc_call(endpoint, "quota.usage", params)?;

            println!("Usage for {member} in {coop_id}:");
            println!();

            let cpu_hours_month = result.get("cpu_hours_this_month").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let cpu_hours_total = result.get("cpu_hours_total").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let concurrent = result.get("concurrent_tasks").and_then(|v| v.as_u64()).unwrap_or(0);
            let completed = result.get("tasks_completed_this_month").and_then(|v| v.as_u64()).unwrap_or(0);
            let credits_spent = result.get("credits_spent_this_month").and_then(|v| v.as_u64()).unwrap_or(0);

            println!("  CPU Hours (this month): {cpu_hours_month:.2}");
            println!("  CPU Hours (total):      {cpu_hours_total:.2}");
            println!("  Concurrent tasks:       {concurrent}");
            println!("  Tasks completed:        {completed}");
            println!("  Credits spent:          {credits_spent}");
        }

        QuotaCommands::List { coop_id } => {
            let params = serde_json::json!({ "coop_id": coop_id });
            let result = rpc_call(endpoint, "quota.list", params)?;

            let usage_records = result
                .as_array()
                .context("Expected array of usage records")?;

            if usage_records.is_empty() {
                println!("No usage records for cooperative: {coop_id}");
                return Ok(());
            }

            println!("Resource Usage for {coop_id}:");
            println!();
            println!("{:<60} {:>12} {:>10} {:>12}", "Member", "CPU Hours", "Tasks", "Credits");
            println!("{:-<96}", "");

            for record in usage_records {
                let member = record.get("member_did").and_then(|v| v.as_str()).unwrap_or("unknown");
                let cpu_hours = record.get("cpu_hours_this_month").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let tasks = record.get("tasks_completed_this_month").and_then(|v| v.as_u64()).unwrap_or(0);
                let credits = record.get("credits_spent_this_month").and_then(|v| v.as_u64()).unwrap_or(0);

                let short_member = if member.len() > 55 {
                    format!("{}...", &member[0..52])
                } else {
                    member.to_string()
                };

                println!("{short_member:<60} {cpu_hours:>12.2} {tasks:>10} {credits:>12}");
            }
        }
    }

    Ok(())
}
