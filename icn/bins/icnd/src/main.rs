//! ICNd - The ICN substrate daemon
//!
//! # Safety
//! This binary denies panicking on unwrap/expect to prevent runtime crashes.
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
// Allow cfg checks for optional HSM/TPM features defined in Cargo.toml
#![allow(unexpected_cfgs)]

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

    /// Allow running Gateway API without JWT authentication (INSECURE - development only)
    #[arg(long)]
    insecure_gateway_no_jwt: bool,

    /// Validate configuration and exit (useful for CI/CD)
    #[arg(long)]
    validate_config: bool,

    /// Enable distributed tracing with OpenTelemetry
    #[arg(long)]
    tracing_enable: bool,

    /// OTLP endpoint for trace export (e.g., "http://tempo:4317")
    #[arg(long)]
    tracing_otlp_endpoint: Option<String>,

    /// Tracing sampling rate (0.0 to 1.0, default: 0.1)
    #[arg(long)]
    tracing_sampling_rate: Option<f64>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize rustls crypto provider (required for QUIC/TLS)
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("Failed to install default crypto provider"))?;

    // Load or create config (before tracing init so we can use tracing config)
    let mut config = if let Some(config_path) = &args.config {
        Config::from_file(config_path).context("Failed to load config file")?
    } else {
        Config::default()
    };

    // Apply tracing CLI args before initialization
    if args.tracing_enable {
        config.observability.tracing.enabled = true;
    }
    if let Some(endpoint) = args.tracing_otlp_endpoint {
        config.observability.tracing.otlp_endpoint = endpoint;
    }
    if let Some(rate) = args.tracing_sampling_rate {
        config.observability.tracing.sampling_rate = rate;
    }

    // Check for OTEL environment variables (standard OpenTelemetry config)
    if let Ok(endpoint) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT") {
        config.observability.tracing.otlp_endpoint = endpoint;
        config.observability.tracing.enabled = true;
    }
    if let Ok(service_name) = std::env::var("OTEL_SERVICE_NAME") {
        config.observability.tracing.service_name = service_name;
    }

    // Initialize observability with distributed tracing support
    let tracing_config = config.observability.tracing.to_obs_config(None);
    icn_obs::init_tracing(&tracing_config)?;
    tracing::info!("ICNd starting");

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
        tracing::debug!(
            "Gateway JWT secret loaded from ICN_GATEWAY_JWT_SECRET environment variable"
        );
    }

    // If --insecure-gateway-no-jwt is set, use a placeholder to pass validation
    // (the actual insecure mode warning is logged later)
    let insecure_no_jwt = args.insecure_gateway_no_jwt
        && config.gateway.enabled
        && config.gateway.jwt_secret.is_empty();
    if insecure_no_jwt {
        config.gateway.jwt_secret = "__INSECURE_NO_JWT__".to_string();
    }

    // Handle --validate-config flag
    if args.validate_config {
        println!("Validating configuration...\n");
        match config.validate() {
            Ok(warnings) => {
                if warnings.is_empty() {
                    println!("\x1b[32m✓ Configuration is valid (no warnings)\x1b[0m");
                } else {
                    println!("\x1b[32m✓ Configuration is valid\x1b[0m\n");
                    println!("Warnings:");
                    for warning in &warnings {
                        println!("  \x1b[33m⚠\x1b[0m {warning}");
                    }
                }
                std::process::exit(0);
            }
            Err(errors) => {
                println!("\x1b[31m✗ Configuration has errors:\x1b[0m\n");
                for error in &errors {
                    println!("  \x1b[31m✗\x1b[0m {error}");
                }
                std::process::exit(1);
            }
        }
    }

    // Validate config on normal startup (log warnings, exit on errors)
    match config.validate() {
        Ok(warnings) => {
            for warning in warnings {
                tracing::warn!("Config: {}", warning);
            }
        }
        Err(errors) => {
            for error in &errors {
                tracing::error!("Config error: {}", error);
            }
            tracing::error!(
                "Configuration has {} error(s). Run with --validate-config for details.",
                errors.len()
            );
            std::process::exit(1);
        }
    }

    // Ensure data directory exists
    std::fs::create_dir_all(&config.data_dir)?;

    tracing::info!("Data directory: {:?}", config.data_dir);
    tracing::info!("Log level: {}", config.observability.log_level);

    if config.gateway.enabled {
        tracing::info!("Gateway API enabled on {}", config.gateway.bind_addr);
        if insecure_no_jwt {
            tracing::warn!(
                "⚠️  Gateway running WITHOUT JWT authentication (--insecure-gateway-no-jwt)"
            );
            tracing::warn!("⚠️  This is insecure and should only be used for development!");
            // Clear the placeholder so gateway knows to skip JWT validation
            config.gateway.jwt_secret = String::new();
        } else {
            tracing::info!(
                "Gateway JWT secret configured (length: {})",
                config.gateway.jwt_secret.len()
            );
        }
    } else {
        tracing::debug!("Gateway API disabled");
    }

    // Check for identity keystore
    let keystore_path = config.keystore_path();
    let identity_bundle = if keystore_path.exists() {
        tracing::info!("Identity keystore found at: {:?}", keystore_path);
        tracing::info!("Using identity backend: {}", config.identity.backend);

        // Get passphrase: tries ICN_KEYSTORE_PASSPHRASE env var first, then ICN_PASSPHRASE,
        // then falls back to interactive prompt. For automated deployments (systemd, Docker, K8s),
        // set one of the environment variables.
        // Security: Passphrase uses Zeroizing<Vec<u8>> to automatically clear from memory.
        let passphrase =
            read_passphrase("Enter keystore passphrase: ").context("Failed to read passphrase")?;

        // Load and unlock keystore
        // Check backend type and use appropriate loading strategy
        let mut keystore = match config.identity.backend.as_str() {
            "age" => {
                // Age backend: use direct AgeKeyStore
                AgeKeyStore::open(&keystore_path).context("Failed to open Age keystore")?
            }
            "pkcs11" | "tpm" => {
                // Hardware backends: not yet implemented
                // Return explicit error instead of calling unimplemented factory
                anyhow::bail!(
                    "Hardware identity backend '{}' is not yet implemented. \
                     See docs/identity-backend-configuration.md for status.",
                    config.identity.backend
                );
            }
            other => {
                anyhow::bail!("Unknown identity backend '{}'", other);
            }
        };

        keystore
            .unlock(&passphrase)
            .context("Failed to unlock keystore - incorrect passphrase?")?;

        let bundle = keystore
            .get_identity_bundle()
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
    let mut runtime_handle = tokio::spawn(async move { runtime.run().await });

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

    // Flush any pending traces before exit
    icn_obs::shutdown_tracing();

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

/// Read passphrase from environment or stdin
///
/// Checks in order:
/// 1. ICN_KEYSTORE_PASSPHRASE environment variable (preferred for production)
/// 2. ICN_PASSPHRASE environment variable (legacy, for backward compatibility)
/// 3. Interactive prompt (for development)
///
/// Returns a zeroizing container that automatically clears the passphrase
/// from memory when it goes out of scope, preventing sensitive data leakage.
///
/// Security: Both the String returned by rpassword and the final `Vec<u8>` are
/// wrapped in Zeroizing to ensure complete memory cleanup.
fn read_passphrase(prompt: &str) -> Result<Zeroizing<Vec<u8>>> {
    // Check for ICN_KEYSTORE_PASSPHRASE environment variable first (preferred)
    if let Ok(passphrase) = std::env::var("ICN_KEYSTORE_PASSPHRASE") {
        tracing::debug!("Passphrase loaded from ICN_KEYSTORE_PASSPHRASE environment variable");
        return Ok(Zeroizing::new(passphrase.into_bytes()));
    }

    // Check for ICN_PASSPHRASE environment variable (legacy, backward compatible)
    if let Ok(passphrase) = std::env::var("ICN_PASSPHRASE") {
        tracing::debug!("Passphrase loaded from ICN_PASSPHRASE environment variable");
        return Ok(Zeroizing::new(passphrase.into_bytes()));
    }

    // Fall back to interactive prompt
    print!("{prompt}");
    io::stdout().flush()?;
    // Wrap the String immediately in Zeroizing to prevent it from lingering in memory
    let passphrase_str =
        Zeroizing::new(rpassword::read_password().context("Failed to read password")?);
    // Convert to bytes (copies from zeroized String, which is then dropped and zeroed)
    Ok(Zeroizing::new(passphrase_str.as_bytes().to_vec()))
}
