/*!
 * ICN Wallet CLI Interface
 *
 * Command-line interface for wallet operations including
 * receipt import, verification, and management.
 */

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use anyhow::{Result, Context};

use crate::import::import_receipts_from_file;
use icn_wallet_core::replay::replay_and_verify_receipt;

/// ICN Wallet CLI
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Subcommand to execute
    #[command(subcommand)]
    command: Command,
}

/// CLI commands
#[derive(Subcommand)]
enum Command {
    /// Manage execution receipts
    #[command(subcommand)]
    Receipts(ReceiptsCommand),
}

/// Receipt management commands
#[derive(Subcommand)]
enum ReceiptsCommand {
    /// Verify imported receipts against local DAG store
    Verify {
        /// Path to the receipt file (JSON)
        #[arg(short, long)]
        file: PathBuf,
        
        /// Skip DAG verification (only do basic validation)
        #[arg(long, default_value = "false")]
        skip_dag_verification: bool,
    },
    
    /// Import receipts from a file
    Import {
        /// Path to the receipt file to import
        #[arg(short, long)]
        file: PathBuf,
    },
}

/// Run the CLI application
pub async fn run_cli() -> Result<()> {
    let cli = Cli::parse();
    
    match &cli.command {
        Command::Receipts(receipt_cmd) => match receipt_cmd {
            ReceiptsCommand::Verify { file, skip_dag_verification } => {
                verify_receipts(file, *skip_dag_verification).await?;
            },
            ReceiptsCommand::Import { file } => {
                import_receipts(file).await?;
            },
        },
    }
    
    Ok(())
}

/// Verify receipts from a file against the local DAG store
async fn verify_receipts(file: &PathBuf, skip_dag_verification: bool) -> Result<()> {
    println!("Verifying receipts from: {}", file.display());
    
    // Import receipts from the file
    let receipts = import_receipts_from_file(file)
        .context("Failed to import receipts")?;
    
    println!("Found {} receipts to verify", receipts.len());
    
    if !skip_dag_verification {
        // Create a local DAG storage manager for verification
        let dag_store = icn_wallet_core::dag::create_local_dag_store()
            .await
            .context("Failed to create DAG store for verification")?;
        
        // Verify each receipt against the DAG
        for (idx, receipt) in receipts.iter().enumerate() {
            print!("Verifying receipt {}/{}: {} ... ", idx + 1, receipts.len(), receipt.proposal_id);
            
            match replay_and_verify_receipt(receipt, &dag_store).await {
                Ok(true) => println!("✅ VERIFIED"),
                Ok(false) => println!("❌ FAILED verification"),
                Err(e) => println!("❌ ERROR: {}", e),
            }
        }
    } else {
        println!("DAG verification skipped - receipts are basically valid but not verified against state");
    }
    
    Ok(())
}

/// Import receipts from a file
async fn import_receipts(file: &PathBuf) -> Result<()> {
    println!("Importing receipts from: {}", file.display());
    
    // Import the receipts
    let receipts = import_receipts_from_file(file)
        .context("Failed to import receipts")?;
    
    println!("Successfully imported {} receipts:", receipts.len());
    
    // Display summary of the imported receipts
    for (idx, receipt) in receipts.iter().enumerate() {
        println!("  {}. ID: {}, Proposal: {}, Outcome: {}", 
            idx + 1, 
            receipt.credential.id, 
            receipt.proposal_id,
            receipt.outcome
        );
    }
    
    Ok(())
} 