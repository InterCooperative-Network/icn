use anyhow::Result;
use clap::Command;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod commands;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();
    
    // Build the CLI app
    let cli = Command::new("icn-runtime")
        .version(env!("CARGO_PKG_VERSION"))
        .about("ICN Runtime CLI")
        .subcommand(commands::wallet_test::cli())
        // Add other commands here
        ;
    
    // Parse arguments
    let matches = cli.get_matches();
    
    // Handle subcommands
    match matches.subcommand() {
        Some(("wallet-test", sub_matches)) => {
            let subcmd = sub_matches.subcommand().map_or("", |(s, _)| s);
            commands::wallet_test::execute(subcmd, sub_matches)
                .await?;
        }
        // Handle other commands here
        _ => {
            println!("No command specified. Use --help for available commands.");
        }
    }
    
    Ok(())
} 