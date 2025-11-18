//! ICNd - The ICN substrate daemon

use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use icn_core::{Config, Runtime};
use icn_identity::{AgeKeyStore, KeyStore};
use std::io::{self, Write};
use std::path::PathBuf;
use zeroize::Zeroizing;

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

    /// Enable the Gateway API server
    #[arg(long)]
    gateway_enable: bool,

    /// Gateway API bind address (format: "IP:PORT")
    #[arg(long)]
    gateway_bind: Option<String>,

    /// Gateway JWT secret
    #[arg(long)]
    gateway_jwt_secret: Option<String>,
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

    // Apply gateway CLI arguments
    if args.gateway_enable {
        config.gateway.enabled = true;
    }

    if let Some(bind_addr) = args.gateway_bind {
        config.gateway.bind_addr = bind_addr;
    }

    // Read JWT secret from CLI arg, then env var ICN_GATEWAY_JWT_SECRET, then config
    if let Some(jwt_secret) = args.gateway_jwt_secret {
        config.gateway.jwt_secret = jwt_secret;
    } else if let Ok(jwt_secret) = std::env::var("ICN_GATEWAY_JWT_SECRET") {
        config.gateway.jwt_secret = jwt_secret;
        tracing::debug!("Gateway JWT secret loaded from ICN_GATEWAY_JWT_SECRET environment variable");
    }

    // Ensure data directory exists
    std::fs::create_dir_all(&config.data_dir)?;

    tracing::info!("Data directory: {:?}", config.data_dir);
    tracing::info!("Log level: {}", config.observability.log_level);

    if config.gateway.enabled {
        tracing::info!("Gateway API enabled on {}", config.gateway.bind_addr);
        if config.gateway.jwt_secret.is_empty() {
            tracing::warn!("Gateway enabled but JWT secret not configured!");
        }
    } else {
        tracing::debug!("Gateway API disabled");
    }

    // Check for identity keystore
    let keystore_path = config.keystore_path();
    let identity_bundle = if keystore_path.exists() {
        tracing::info!("Identity keystore found at: {:?}", keystore_path);

        // Prompt for passphrase (returns Zeroizing<Vec<u8>> for secure memory handling)
        // Security: Passphrase is automatically zeroed from memory when it goes out of scope,
        // preventing recovery from memory dumps or swap space.
        // Note: This will fail when run as a systemd service (non-interactive)
        // Consider using environment variable or socket-based authentication for production
        let passphrase = read_passphrase("Enter keystore passphrase: ")
            .context("Failed to read passphrase")?;

        // Load and unlock keystore
        let mut keystore = AgeKeyStore::open(&keystore_path)
            .context("Failed to open keystore")?;
        keystore.unlock(&passphrase)
            .context("Failed to unlock keystore - incorrect passphrase?")?;

        let bundle = keystore.get_identity_bundle()
            .context("Failed to get identity bundle from keystore")?;

        tracing::info!("Identity loaded: {} (with DID-TLS binding)", bundle.did());
        Some(bundle.clone())
    } else {
        tracing::warn!("No identity keystore found at: {:?}", keystore_path);
        tracing::warn!("Run 'icnctl id init' to create an identity");
        tracing::warn!("Daemon will run without Identity and Network actors");
        None
    };

    // Create runtime
    let runtime = Runtime::new(config.clone(), identity_bundle);

    // Get shutdown signal before moving runtime
    let shutdown_tx = runtime.shutdown_tx();

    // Spawn runtime task
    let mut runtime_handle = tokio::spawn(async move {
        runtime.run().await
    });

    // Wait for shutdown signal or runtime completion
    let shutdown_result = tokio::select! {
        result = &mut runtime_handle => {
            // Runtime exited on its own
            tracing::info!("Runtime exited");
            Some(result??)
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received Ctrl+C, shutting down gracefully...");
            let _ = shutdown_tx.send(());
            None
        }
        _ = wait_for_sigterm() => {
            tracing::info!("Received SIGTERM, shutting down gracefully...");
            let _ = shutdown_tx.send(());
            None
        }
    };

    // If we signaled shutdown, wait for runtime to finish
    if shutdown_result.is_none() {
        runtime_handle.await??;
    }

    tracing::info!("ICNd stopped");
    Ok(())
}

/// Wait for SIGTERM signal on Unix systems
#[cfg(unix)]
async fn wait_for_sigterm() -> Result<()> {
    use tokio::signal::unix::{signal, SignalKind};
    let mut stream = signal(SignalKind::terminate())?;
    stream.recv().await;
    Ok(())
}

/// Wait for SIGTERM signal on non-Unix systems (no-op)
#[cfg(not(unix))]
async fn wait_for_sigterm() -> Result<()> {
    // On non-Unix systems, just wait forever
    std::future::pending().await
}

/// Read passphrase from stdin
///
/// Returns a zeroizing container that automatically clears the passphrase
/// from memory when it goes out of scope, preventing sensitive data leakage.
///
/// Security: Both the String returned by rpassword and the final Vec<u8> are
/// wrapped in Zeroizing to ensure complete memory cleanup.
fn read_passphrase(prompt: &str) -> Result<Zeroizing<Vec<u8>>> {
    // Check for ICN_PASSPHRASE environment variable first
    if let Ok(passphrase) = std::env::var("ICN_PASSPHRASE") {
        return Ok(Zeroizing::new(passphrase.into_bytes()));
    }

    print!("{prompt}");
    io::stdout().flush()?;
    // Wrap the String immediately in Zeroizing to prevent it from lingering in memory
    let passphrase_str = Zeroizing::new(
        rpassword::read_password()
            .context("Failed to read password")?
    );
    // Convert to bytes (copies from zeroized String, which is then dropped and zeroed)
    Ok(Zeroizing::new(passphrase_str.as_bytes().to_vec()))
}
