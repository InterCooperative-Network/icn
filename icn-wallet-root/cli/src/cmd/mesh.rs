use crate::error::IcnWalletCliError;
use clap::{Args, Subcommand};
use icn_wallet_agent::WalletAgent;
use std::path::PathBuf;

/// Mesh Compute commands
#[derive(Debug, Args)]
pub struct MeshArgs {
    #[clap(subcommand)]
    pub command: MeshCommands,
}

/// Commands for interacting with the Mesh Compute overlay
#[derive(Debug, Subcommand)]
pub enum MeshCommands {
    /// Submit a task to the mesh compute network
    SubmitTask(SubmitTaskArgs),
}

/// Arguments for submitting a task
#[derive(Debug, Args)]
pub struct SubmitTaskArgs {
    /// Path to the WASM module
    #[clap(short, long)]
    pub wasm: PathBuf,
    
    /// Path to the input data
    #[clap(short, long)]
    pub input: PathBuf,
    
    /// Tokens to offer as fee
    #[clap(short, long, default_value = "100")]
    pub fee: u64,
    
    /// Number of verifiers required
    #[clap(short, long, default_value = "3")]
    pub verifiers: u32,
    
    /// Expiry time in minutes
    #[clap(short, long, default_value = "60")]
    pub expiry: i64,
}

/// Handle mesh commands
pub async fn handle_mesh_command(
    agent: &WalletAgent,
    command: MeshCommands,
) -> Result<(), IcnWalletCliError> {
    match command {
        MeshCommands::SubmitTask(args) => {
            // Submit the task
            let task_cid = agent
                .submit_mesh_task(
                    args.wasm,
                    args.input,
                    args.fee,
                    args.verifiers,
                    args.expiry,
                )
                .await?;
                
            println!("Task submitted successfully!");
            println!("Task CID: {}", task_cid);
            
            Ok(())
        }
    }
} 