//! ICNd - The ICN substrate daemon

use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use icn_core::{Config, Runtime};
use icn_identity::{AgeKeyStore, KeyStore};
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "icnd")]
#[command(about = "ICN substrate daemon", long_about = None)]
struct Args {
    /// Path to configuration file
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Data directory
    #[arg(short, long)]
    data_dir: Option<PathBuf>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize rustls crypto provider (required for QUIC/TLS)
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("Failed to install default crypto provider"))?;

    // Initialize observability
    icn_obs::init()?;
    tracing::info!("ICNd starting");

    // Load or create config
    let mut config = if let Some(config_path) = args.config {
        Config::from_file(config_path)?
    } else {
        Config::default()
    };

    // Override with CLI args
    if let Some(data_dir) = args.data_dir {
        config.data_dir = data_dir;
    }

    config.observability.log_level = args.log_level;

    // Ensure data directory exists
    std::fs::create_dir_all(&config.data_dir)?;

    tracing::info!("Data directory: {:?}", config.data_dir);
    tracing::info!("Log level: {}", config.observability.log_level);

    // Check for identity keystore
    let keystore_path = config.keystore_path();
    let keypair = if keystore_path.exists() {
        tracing::info!("Identity keystore found at: {:?}", keystore_path);

        // Prompt for passphrase
        // Note: This will fail when run as a systemd service (non-interactive)
        // Consider using environment variable or socket-based authentication for production
        let passphrase = read_passphrase("Enter keystore passphrase: ")
            .context("Failed to read passphrase")?;

        // Load and unlock keystore
        let mut keystore = AgeKeyStore::open(&keystore_path)
            .context("Failed to open keystore")?;
        keystore.unlock(&passphrase)
            .context("Failed to unlock keystore - incorrect passphrase?")?;

        let kp = keystore.get_keypair()
            .context("Failed to get keypair from keystore")?;

        tracing::info!("Identity loaded: {}", kp.did());
        Some(kp.clone())
    } else {
        tracing::warn!("No identity keystore found at: {:?}", keystore_path);
        tracing::warn!("Run 'icnctl id init' to create an identity");
        tracing::warn!("Daemon will run without Identity and Network actors");
        None
    };

    // Create and run runtime
    let runtime = Runtime::new(config, keypair);
    runtime.run().await?;

    tracing::info!("ICNd stopped");
    Ok(())
}

/// Read passphrase from stdin
fn read_passphrase(prompt: &str) -> Result<Vec<u8>> {
    print!("{}", prompt);
    io::stdout().flush()?;
    let passphrase = rpassword::read_password()
        .context("Failed to read password")?;
    Ok(passphrase.into_bytes())
}
