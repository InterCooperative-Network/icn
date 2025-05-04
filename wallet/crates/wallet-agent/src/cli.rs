/*!
 * ICN Wallet CLI Interface
 *
 * Command-line interface for wallet operations including
 * receipt import, verification, and management.
 */

use clap::{Parser, Subcommand, Args};
use std::path::PathBuf;
use anyhow::{Result, Context};

use crate::import::{import_receipts_from_file, ExecutionReceipt};
use crate::share::{share_receipts, ShareOptions, ShareFormat};
use icn_wallet_core::replay::replay_and_verify_receipt;
use icn_wallet_core::filter::{filter_receipts, ReceiptFilter};

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
    
    /// Share receipts with selective disclosure
    Share {
        /// Path to the receipt file to process
        #[arg(short, long)]
        file: PathBuf,
        
        /// Output path for the shared receipts
        #[arg(short, long)]
        output: PathBuf,
        
        /// Output format (json, csv, bundle, encrypted)
        #[arg(short, long, default_value = "json")]
        format: String,
        
        /// Filter options
        #[command(flatten)]
        filter: FilterOptions,
        
        /// Whether to include cryptographic proofs
        #[arg(long, default_value = "true")]
        include_proofs: bool,
        
        /// Recipient public key (for encrypted format)
        #[arg(short, long)]
        recipient: Option<String>,
        
        /// Additional metadata as JSON string
        #[arg(short, long)]
        metadata: Option<String>,
    },
    
    /// Share receipts with a federation
    FederationShare {
        /// Path to the receipt file to process
        #[arg(short, long)]
        file: PathBuf,
        
        /// Federation URL to share with
        #[arg(short, long)]
        federation: String,
        
        /// Federation public key for encryption
        #[arg(short, long)]
        federation_key: String,
        
        /// Filter options
        #[command(flatten)]
        filter: FilterOptions,
        
        /// Sender DID
        #[arg(long, default_value = "did:icn:sender")]
        sender_did: String,
        
        /// Output to browser (opens the share link)
        #[arg(short, long, default_value = "false")]
        browser: bool,
    },
}

/// Filter options for receipt selection
#[derive(Args, Default)]
struct FilterOptions {
    /// Filter by federation scope
    #[arg(long)]
    scope: Option<String>,
    
    /// Filter by execution outcome (Success, Failure)
    #[arg(long)]
    outcome: Option<String>,
    
    /// Filter receipts after this Unix timestamp
    #[arg(long)]
    since: Option<i64>,
    
    /// Filter by proposal ID prefix
    #[arg(long)]
    prefix: Option<String>,
    
    /// Limit number of receipts
    #[arg(long)]
    limit: Option<usize>,
}

impl From<FilterOptions> for ReceiptFilter {
    fn from(options: FilterOptions) -> Self {
        Self {
            scope: options.scope,
            outcome: options.outcome,
            since: options.since,
            proposal_prefix: options.prefix,
            limit: options.limit,
        }
    }
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
            ReceiptsCommand::Share { file, output, format, filter, include_proofs, recipient, metadata } => {
                share_receipts_cmd(
                    file, 
                    output, 
                    format, 
                    filter, 
                    *include_proofs, 
                    recipient.as_deref(), 
                    metadata.as_deref()
                ).await?;
            },
            ReceiptsCommand::FederationShare { file, federation, federation_key, filter, sender_did, browser } => {
                federation_share_cmd(
                    file, 
                    federation, 
                    federation_key, 
                    filter, 
                    sender_did, 
                    *browser
                ).await?;
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

/// Share receipts with selective disclosure
async fn share_receipts_cmd(
    file: &PathBuf,
    output: &PathBuf,
    format_str: &str,
    filter_options: &FilterOptions,
    include_proofs: bool,
    recipient: Option<&str>,
    metadata_str: Option<&str>,
) -> Result<()> {
    println!("Sharing receipts from: {}", file.display());
    
    // Import the receipts
    let all_receipts = import_receipts_from_file(file)
        .context("Failed to import receipts")?;
    
    println!("Loaded {} receipts", all_receipts.len());
    
    // Apply filters
    let filter: ReceiptFilter = filter_options.clone().into();
    let filtered_receipts = filter_receipts(&all_receipts, &filter);
    
    println!("Selected {} receipts after filtering", filtered_receipts.len());
    
    // Parse format
    let format = match format_str.to_lowercase().as_str() {
        "json" => ShareFormat::Json,
        "csv" => ShareFormat::Csv,
        "bundle" => ShareFormat::SignedBundle,
        "encrypted" => ShareFormat::EncryptedBundle,
        _ => return Err(anyhow::anyhow!("Unsupported format: {}", format_str)),
    };
    
    // Parse metadata if provided
    let metadata = if let Some(meta_str) = metadata_str {
        Some(serde_json::from_str(meta_str)
            .context("Failed to parse metadata JSON")?)
    } else {
        None
    };
    
    // Create share options
    let options = ShareOptions {
        format,
        recipient_key: recipient.map(|s| s.to_string()),
        include_proofs,
        metadata,
    };
    
    // Share the receipts
    share_receipts(&filtered_receipts, options, output)
        .context("Failed to share receipts")?;
    
    println!("Shared {} receipts to: {}", filtered_receipts.len(), output.display());
    
    Ok(())
}

/// Share receipts with a federation
async fn federation_share_cmd(
    file: &PathBuf,
    federation: &str,
    federation_key: &str,
    filter_options: &FilterOptions,
    sender_did: &str,
    browser: bool,
) -> Result<()> {
    println!("Sharing receipts with a federation");
    
    // Import the receipts
    let all_receipts = import_receipts_from_file(file)
        .context("Failed to import receipts")?;
    
    println!("Loaded {} receipts", all_receipts.len());
    
    // Apply filters
    let filter: ReceiptFilter = filter_options.clone().into();
    let filtered_receipts = filter_receipts(&all_receipts, &filter);
    
    println!("Selected {} receipts after filtering", filtered_receipts.len());
    
    // Create share options
    let options = ShareOptions {
        format: ShareFormat::EncryptedBundle,
        recipient_key: Some(federation_key.to_string()),
        include_proofs: true,
        metadata: None,
    };
    
    // Share the receipts
    let share_link = share_receipts(&filtered_receipts, options, &PathBuf::new())
        .context("Failed to share receipts")?;
    
    println!("Shared receipts with a federation");
    
    if browser {
        open::that(share_link)?;
    }
    
    Ok(())
} 