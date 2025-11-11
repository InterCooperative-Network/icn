//! icnctl - CLI for managing ICNd

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use icn_identity::{AgeKeyStore, Did, KeyPair, KeyStore};
use icn_store::{SledStore, Store};
use icn_trust::{TrustEdge, TrustGraph};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "icnctl")]
#[command(about = "ICN control CLI", long_about = None)]
struct Args {
    /// Data directory (defaults to ~/.icn)
    #[arg(short, long)]
    data_dir: Option<PathBuf>,

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

        Commands::Network(net_cmd) => handle_network_command(net_cmd)?,
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
            println!("Export: Not implemented yet");
            println!("  Output: {}", output.display());
            // TODO: Implement export
        }

        IdCommands::Import { input } => {
            println!("Import: Not implemented yet");
            println!("  Input: {}", input.display());
            // TODO: Implement import
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

fn handle_network_command(cmd: NetworkCommands) -> Result<()> {
    // Network commands require daemon RPC communication
    // TODO: Implement RPC client in icn-rpc crate
    //
    // For now, these are documented stubs that show the intended API

    match cmd {
        NetworkCommands::Peers => {
            println!("Network: Discovered Peers");
            println!("Status: Not implemented - requires daemon RPC\n");
            println!("This command will list all peers discovered via mDNS.");
            println!("When implemented, it will call: NetworkHandle::get_peers()");
            println!("\nExample output:");
            println!("  DID                                           Address            Version");
            println!("  did:icn:z6Mk...                                192.168.1.100:4433 0.1.0");
            println!("  did:icn:z6Mk...                                192.168.1.101:4433 0.1.0");
            println!("\n→ Start the daemon with: icnd");
            println!("→ Ensure mDNS is enabled in config");
        }

        NetworkCommands::Dial { did, addr } => {
            println!("Network: Dial Peer");
            println!("Status: Not implemented - requires daemon RPC\n");
            println!("  Target DID: {}", did);
            if let Some(a) = addr {
                println!("  Address: {}", a);
            } else {
                println!("  Address: Will be discovered via mDNS");
            }
            println!("\nThis command will establish a QUIC connection to the peer.");
            println!("When implemented, it will call: NetworkHandle::dial(addr, did)");
            println!("\n→ Start the daemon with: icnd");
        }

        NetworkCommands::Stats => {
            println!("Network: Statistics");
            println!("Status: Not implemented - requires daemon RPC\n");
            println!("This command will show network statistics:");
            println!("  - Peers discovered (via mDNS)");
            println!("  - Active QUIC connections");
            println!("  - Total connections established");
            println!("\nWhen implemented, it will call: NetworkHandle::get_stats()");
            println!("\nExample output:");
            println!("  Peers discovered:     5");
            println!("  Active connections:   3");
            println!("  Total connections:    42");
            println!("\n→ Start the daemon with: icnd");
        }

        NetworkCommands::Status => {
            println!("Network: Actor Status");
            println!("Status: Not implemented - requires daemon RPC\n");
            println!("This command will show the NetworkActor status:");
            println!("  - Is NetworkActor running?");
            println!("  - mDNS discovery status");
            println!("  - QUIC listener address");
            println!("  - Recent errors/warnings");
            println!("\n→ Start the daemon with: icnd");
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
