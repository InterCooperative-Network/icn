//! icnctl - CLI for managing ICNd

use anyhow::Result;
use clap::{Parser, Subcommand};
use icn_identity::KeyPair;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "icnctl")]
#[command(about = "ICN control CLI", long_about = None)]
struct Args {
    /// Data directory
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

    /// Peer management
    #[command(subcommand)]
    Peers(PeerCommands),
}

#[derive(Subcommand, Debug)]
enum IdCommands {
    /// Generate a new identity
    Generate,

    /// Show current identity
    Show,
}

#[derive(Subcommand, Debug)]
enum PeerCommands {
    /// List known peers
    List,

    /// Add a peer manually
    Add { multiaddr: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize simple logging
    icn_obs::init()?;

    match args.command {
        Commands::Status => {
            println!("ICNd Status: Not implemented yet");
            // TODO: Connect to daemon via RPC and get status
        }

        Commands::Id(id_cmd) => match id_cmd {
            IdCommands::Generate => {
                println!("Generating new identity...");
                let keypair = KeyPair::generate()?;
                println!("DID: {}", keypair.did());
                println!("\nNote: Key storage not yet implemented");
                // TODO: Store keypair securely
            }

            IdCommands::Show => {
                println!("Identity: Not implemented yet");
                // TODO: Load and display stored identity
            }
        },

        Commands::Peers(peer_cmd) => match peer_cmd {
            PeerCommands::List => {
                println!("Peers: Not implemented yet");
                // TODO: Connect to daemon and list peers
            }

            PeerCommands::Add { multiaddr } => {
                println!("Adding peer: {}", multiaddr);
                println!("Not implemented yet");
                // TODO: Connect to daemon and add peer
            }
        },
    }

    Ok(())
}
