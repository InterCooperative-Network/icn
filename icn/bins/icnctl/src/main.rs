//! icnctl - CLI for managing ICNd

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use icn_identity::{AgeKeyStore, Did, KeyPair, KeyStore};
use icn_store::{SledStore, Store};
use icn_trust::{TrustEdge, TrustGraph};
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

    /// Trust graph management
    #[command(subcommand)]
    Trust(TrustCommands),

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
    print!("{}", prompt);
    io::stdout().flush()?;
    let passphrase = rpassword::read_password()?;
    Ok(passphrase.into_bytes())
}

fn confirm_passphrase() -> Result<Vec<u8>> {
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

        Commands::Trust(trust_cmd) => handle_trust_command(trust_cmd, &data_dir)?,

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

            // Create output directory if needed
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)
                    .context("Failed to create output directory")?;
            }

            // Copy the encrypted keystore to output
            std::fs::copy(&keystore_path, &output)
                .with_context(|| format!("Failed to export keystore to {}", output.display()))?;

            println!("✓ Identity exported successfully!");
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
