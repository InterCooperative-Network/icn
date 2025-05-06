/*!
 * ICN Wallet Agent
 *
 * Command-line and API interface for ICN wallet operations including
 * receipt import, verification, and management.
 */

use std::path::Path;

pub mod import;
pub mod cli;
pub mod share;

pub use import::{import_receipts_from_file, ImportError, ExecutionReceipt};
pub use share::{
    share_receipts, share_receipts_as_json, share_receipts_as_bundle, 
    share_receipts_as_encrypted_bundle, ShareOptions, ShareFormat, ShareError,
    encrypt_receipt_bundle, decrypt_receipt_bundle, EncryptedBundle,
    generate_share_link, generate_share_link_from_receipts
};
pub use cli::run_cli;

// MESH COMPUTE FUNCTIONS

/// Submit a mesh compute task
pub async fn submit_mesh_task(
    &self,
    wasm_path: impl AsRef<Path>,
    input_path: impl AsRef<Path>,
    fee: u64,
    verifiers: u32,
    expiry_minutes: i64,
) -> Result<String, AgentError> {
    let active_account = self.get_active_account().await?;
    
    // Submit the task
    let task = icn_wallet_actions::mesh_actions::submit_task(
        active_account,
        wasm_path.as_ref(),
        input_path.as_ref(),
        fee,
        verifiers,
        expiry_minutes,
    )
    .await
    .map_err(|e| AgentError::ActionError(e.to_string()))?;
    
    // Convert to signable action
    let action = icn_wallet_actions::mesh_actions::task_to_signable_action(&task)
        .map_err(|e| AgentError::ActionError(e.to_string()))?;
        
    // Sign the action
    let signed = self
        .sign_action(action)
        .await
        .map_err(|e| AgentError::SigningError(e.to_string()))?;
        
    // In a full implementation, this would broadcast the task to the mesh network
    
    // Return the task CID
    Ok(task.wasm_cid.to_string())
}

/// End of implementations 