//! icnctl - CLI for managing ICNd

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use icn_identity::{AgeKeyStore, Capability, Did, KeyPair, KeyStore, KeyType};
use icn_store::{SledStore, Store};
use icn_trust::{TrustEdge, TrustGraph};
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;
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

    print!("{}", prompt);
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

        Commands::Trust(trust_cmd) => handle_trust_command(trust_cmd, &data_dir)?,

        Commands::Ledger(ledger_cmd) => handle_ledger_command(ledger_cmd, &args.endpoint)?,

        Commands::Contract(contract_cmd) => handle_contract_command(contract_cmd, &args.endpoint, &data_dir)?,

        Commands::Network(net_cmd) => handle_network_command(net_cmd, &args.endpoint)?,
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
            println!("  Old DID: {}", old_did);
            println!("  New DID: {}", new_did);
            if let Some(r) = reason {
                println!("  Reason: {}", r);
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
            println!("  DID:  {}", did);
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
            println!("  DID:  {}", imported_did);
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
            println!("  Target: {}", target_did);
            println!("  Score: {:.2}", score);
        }

        TrustCommands::List => {
            let edges = graph.get_outgoing_edges(&own_did)?;

            if edges.is_empty() {
                println!("No trust edges found.");
            } else {
                println!("Trust edges from {}:\n", own_did);
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

            println!("Trust score for {}:", target_did);
            println!("  Score: {:.4}", score);
            println!("  Class: {:?}", class);

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

            println!("✓ Removed trust edge to {}", target_did);
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
                println!("{:<50} {:<22} {}", "DID", "Address", "Version");
                println!("{}", "-".repeat(80));
                for peer in peers {
                    println!("{:<50} {:<22} {}", peer.did, peer.addr, peer.version);
                }
            }
        }

        NetworkCommands::Dial { did, addr } => {
            let addr_str = addr.unwrap_or_else(|| "auto-discover".to_string());
            println!("Dialing peer...");
            println!("  Target DID: {}", did);
            println!("  Address: {}\n", addr_str);

            client
                .dial(did.clone(), addr_str)
                .await
                .context("Failed to dial peer. Is icnd running?")?;

            println!("✓ Successfully established connection to {}", did);
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
                            println!("      Debit:    {}", debit);
                        }
                        if let Some(credit) = delta.credit {
                            println!("      Credit:   {}", credit);
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
                println!("No balances found for account: {}", account_id);
            } else if balances.len() == 1 && currency.is_some() {
                let balance = &balances[0];
                println!("Balance for {}:\n", account_id);
                println!("  Currency: {}", balance.currency);
                println!("  Amount:   {}", balance.amount);
            } else {
                println!("Balances for {}:\n", account_id);
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
                            print!("debit {} ", debit);
                        }
                        if let Some(credit) = delta.credit {
                            print!("credit {} ", credit);
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

            let quarantined: Vec<serde_json::Value> = result
                .get("quarantined")
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
                        println!("Metadata:    {}", metadata);
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

            println!("Quarantined Entry: {}\n", entry_id);

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
                    println!("  Metadata:    {}", metadata);
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

            println!("✓ Released entry: {}", entry_id);
            println!("✓ Successfully reappended to ledger");
        }

        QuarantineCommands::Drop { entry_id } => {
            let result = client
                .quarantine_drop(entry_id.clone())
                .await
                .context("Failed to drop entry from daemon. Is icnd running?")?;

            if result.get("dropped").and_then(|v| v.as_bool()).unwrap_or(false) {
                println!("✓ Permanently dropped entry: {}", entry_id);
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
            println!("✓ Purged {} expired entries", purged);
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

            println!("Signing deployment as {}", deployer_did);

            // Compute code hash (must match ContractActor::compute_code_hash)
            let code_hash = {
                use sha2::{Sha256, Digest};
                let mut hasher = Sha256::new();
                hasher.update(contract.name.as_bytes());
                for participant in &contract.participants {
                    hasher.update(format!("{:?}", participant).as_bytes());
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
                    println!("  Code Hash: {}", code_hash);
                    println!("\nYou can now call contract rules using:");
                    println!("  icnctl contract call {} <rule_name> <caller_did> --args '{{}}'", code_hash);
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

            println!("Calling contract {}...", code_hash);
            println!("  Rule: {}", rule_name);
            println!("  Caller: {}", caller);
            println!("  Args: {}\n", args_value);

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
                                println!("  Currency: {}", currency);
                            }
                            println!("  Rules: {}", contract.rules.join(", "));
                            println!();
                        }
                    }
                }
                Err(e) => {
                    println!("Note: Contract listing not yet fully implemented.");
                    println!("Error: {}", e);
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
        bail!("You ({}) are not a participant in this contract", signer_did);
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
            hasher.update(format!("{:?}", participant).as_bytes());
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
    println!("  ✓ {}", signer_did);
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
        bail!("You ({}) are not a participant in this contract", signer_did);
    }

    // Check if already signed
    if deployment_msg.installation.signatures.iter().any(|(did, _)| did == &signer_did) {
        bail!("You ({}) have already signed this deployment", signer_did);
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
    println!("Signatures collected: {}/{}", signatures_count, total_participants);
    println!("Signed by:");
    for (did, _) in &deployment_msg.installation.signatures {
        println!("  ✓ {}", did);
    }

    if signatures_count < total_participants {
        println!();
        println!("Still waiting for signatures from:");
        for participant in &deployment_msg.contract.participants {
            if !deployment_msg.installation.signatures.iter().any(|(did, _)| did == participant) {
                println!("  ⏳ {}", participant);
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
        bail!("Missing signatures from: {:?}", missing);
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
            println!("  Code Hash: {}", code_hash);
            println!("\nYou can now call contract rules using:");
            println!("  icnctl contract call {} <rule_name> <caller_did> --args '{{}}'", code_hash);
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

                device_map.entry(base_id).or_insert_with(Vec::new).push(vm);
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
                        println!("  Revoked: {}", revoked);
                    }

                    println!("  Capabilities:");
                    for cap in &vm.capabilities {
                        println!("    - {:?}", cap);
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
            println!("Creating device-add request for '{}'...\n", name);

            // Prompt for the target DID (the identity to add this device to)
            print!("Enter the DID to add this device to: ");
            io::stdout().flush()?;
            let mut did_input = String::new();
            io::stdin().read_line(&mut did_input)?;
            let target_did = did_input.trim();

            if !target_did.starts_with("did:icn:") {
                bail!("Invalid DID format. Expected: did:icn:<base58btc-key>");
            }

            println!("Target DID: {}", target_did);
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
            println!("  • This device will be added to identity: {}", target_did);
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
            let new_enc_id = format!("enc-{}", max_device_num + 1);

            println!("Adding device as: {}", new_device_id);

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

            // Create rotation event for this device add
            let rotation_event = icn_identity::RotationEvent {
                did: own_did.clone(),
                event_type: icn_identity::RotationEventType::AddDevice {
                    device_id: new_device_id.clone(),
                    label: request.label.clone(),
                    public_key: ed25519_bytes.clone(),
                    key_type: KeyType::Ed25519,
                    capabilities: request.capabilities.clone(),
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
            let new_enc_id_clone = new_enc_id.clone();
            let request_label = request.label.clone();
            let request_caps = request.capabilities.clone();

            keystore.update_did_document(
                |did_doc| {
                    // Add Ed25519 signing key
                    did_doc.add_device(
                        new_device_id_clone.clone(),
                        request_label.clone(),
                        ed25519_bytes.clone(),
                        KeyType::Ed25519,
                        request_caps.clone(),
                    )?;

                    // Add X25519 encryption key
                    did_doc.add_device(
                        new_enc_id_clone,
                        format!("{} (encryption)", request_label),
                        x25519_bytes,
                        KeyType::X25519,
                        vec![Capability::Encrypt],
                    )?;

                    Ok(())
                },
                Some(rotation_event),
                &passphrase,
            )?;

            println!("✓ Device approved and added to DID document");
            println!("  Device ID: {}", new_device_id);
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

            println!("Revoking device: {}", device_id);
            if let Some(r) = &reason {
                println!("Reason: {}", r);
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
                bail!("Device '{}' not found in DID document", device_id);
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
            println!("  Device: {}", device_id);
            if let Some(r) = reason {
                println!("  Reason: {}", r);
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
