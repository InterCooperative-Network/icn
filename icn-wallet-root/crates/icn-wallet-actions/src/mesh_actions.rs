use crate::error::ActionError;
use anyhow::{anyhow, Result};
use chrono::{Duration, Utc};
use icn_common::utils::cid_utils;
use icn_wallet_types::{Account, ActionType, SignableAction};
use mesh_types::TaskIntent;
use std::{path::Path, sync::Arc};
use tokio::fs;

/// Submit a mesh compute task using the wallet
pub async fn submit_task(
    account: Arc<Account>,
    wasm_path: &Path,
    input_path: &Path,
    fee: u64,
    verifiers: u32,
    expiry_minutes: i64,
) -> Result<TaskIntent, ActionError> {
    // Check if the files exist
    if !wasm_path.exists() {
        return Err(ActionError::InvalidInput(format!(
            "WASM file not found: {}",
            wasm_path.display()
        )));
    }
    if !input_path.exists() {
        return Err(ActionError::InvalidInput(format!(
            "Input file not found: {}",
            input_path.display()
        )));
    }

    // Read the files
    let wasm_bytes = fs::read(wasm_path)
        .await
        .map_err(|e| ActionError::IoError(e.to_string()))?;
    let input_bytes = fs::read(input_path)
        .await
        .map_err(|e| ActionError::IoError(e.to_string()))?;

    // Generate CIDs for WASM and input data
    let wasm_cid = cid_utils::bytes_to_cid(&wasm_bytes)
        .map_err(|e| ActionError::DataError(e.to_string()))?;
    let input_cid = cid_utils::bytes_to_cid(&input_bytes)
        .map_err(|e| ActionError::DataError(e.to_string()))?;

    // Create the task intent
    let task = TaskIntent {
        publisher_did: account.did().to_string(),
        wasm_cid,
        input_cid,
        fee,
        verifiers,
        expiry: Utc::now() + Duration::minutes(expiry_minutes),
        metadata: None,
    };

    // In a full implementation, this would:
    // 1. Upload the WASM and input data to IPFS or another content store
    // 2. Sign the task intent with the account's private key
    // 3. Broadcast the task to the mesh network

    Ok(task)
}

/// Generate a SignableAction for a task intent
pub fn task_to_signable_action(task: &TaskIntent) -> Result<SignableAction, ActionError> {
    // Serialize the task to JSON
    let task_json = serde_json::to_string(task)
        .map_err(|e| ActionError::SerializationError(e.to_string()))?;

    // Create the action
    let action = SignableAction {
        action_type: ActionType::MeshTask,
        data: task_json,
        timestamp: Utc::now(),
    };

    Ok(action)
} 