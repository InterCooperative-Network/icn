//! ICNd - The ICN substrate daemon

use anyhow::Result;
use clap::Parser;
use icn_core::{Config, Runtime};
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

    // Create and run runtime
    let runtime = Runtime::new(config);
    runtime.run().await?;

    tracing::info!("ICNd stopped");
    Ok(())
}
