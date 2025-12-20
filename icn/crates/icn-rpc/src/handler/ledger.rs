//! Ledger-related RPC handlers

use std::sync::Arc;

use crate::pagination::{paginate, PageRequest, DEFAULT_MAX_PAGE_SIZE};
use crate::server::RpcServer;
use crate::types::{LedgerAccountDelta, LedgerBalance, LedgerEntry, RpcResponse};

/// Handle ledger.head RPC call - get the most recent ledger entry
pub async fn handle_ledger_head(id: u64, state: &Arc<RpcServer>) -> RpcResponse {
    let ledger_handle = match state.ledger_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Ledger not available".to_string());
        }
    };

    let ledger = ledger_handle.read().await;
    match ledger.get_all_entries() {
        Ok(entries) => {
            if let Some(last_entry) = entries.last() {
                let hash = last_entry
                    .id
                    .as_ref()
                    .map(|h| h.to_hex())
                    .unwrap_or_else(|| "unknown".to_string());

                let rpc_entry = LedgerEntry {
                    hash,
                    timestamp: last_entry.timestamp,
                    author: last_entry.author.as_str().to_string(),
                    accounts: last_entry
                        .accounts
                        .iter()
                        .map(|delta| LedgerAccountDelta {
                            account_id: delta.account_id.as_str().to_string(),
                            currency: delta.currency.clone(),
                            debit: delta.debit,
                            credit: delta.credit,
                        })
                        .collect(),
                };

                match serde_json::to_value(&rpc_entry) {
                    Ok(value) => RpcResponse::success(id, value),
                    Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {e}")),
                }
            } else {
                RpcResponse::success(id, serde_json::json!(null))
            }
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get entries: {e}")),
    }
}

/// Handle ledger.balance RPC call - get balance for an account
pub async fn handle_ledger_balance(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let ledger_handle = match state.ledger_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Ledger not available".to_string());
        }
    };

    // Parse parameters
    #[derive(serde::Deserialize)]
    struct BalanceParams {
        account_id: String,
        currency: Option<String>,
    }

    let balance_params: BalanceParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    let account_did = match serde_json::from_value(serde_json::Value::String(
        balance_params.account_id.clone(),
    )) {
        Ok(d) => d,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid DID: {e}"));
        }
    };

    let ledger = ledger_handle.read().await;

    if let Some(currency) = balance_params.currency {
        // Get balance for specific currency
        let amount = ledger.get_balance(&account_did, &currency);
        let balance = LedgerBalance {
            account_id: balance_params.account_id,
            currency,
            amount,
        };

        match serde_json::to_value(&balance) {
            Ok(value) => RpcResponse::success(id, value),
            Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {e}")),
        }
    } else {
        // Get all balances for account
        let account_balances = ledger.get_account_balances(&account_did);
        let balances: Vec<LedgerBalance> = account_balances
            .balances
            .iter()
            .map(|(currency, amount)| LedgerBalance {
                account_id: balance_params.account_id.clone(),
                currency: currency.clone(),
                amount: *amount,
            })
            .collect();

        match serde_json::to_value(&balances) {
            Ok(value) => RpcResponse::success(id, value),
            Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {e}")),
        }
    }
}

/// Handle ledger.history RPC call - get recent ledger entries (paginated)
pub async fn handle_ledger_history(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let ledger_handle = match state.ledger_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Ledger not available".to_string());
        }
    };

    // Parse pagination parameters
    let page_request: PageRequest = serde_json::from_value(params.clone()).unwrap_or_default();

    let ledger = ledger_handle.read().await;
    match ledger.get_all_entries() {
        Ok(entries) => {
            // Convert all entries (in reverse order - most recent first)
            let all_entries: Vec<LedgerEntry> = entries
                .iter()
                .rev()
                .map(|entry| {
                    let hash = entry
                        .id
                        .as_ref()
                        .map(|h| h.to_hex())
                        .unwrap_or_else(|| "unknown".to_string());

                    LedgerEntry {
                        hash,
                        timestamp: entry.timestamp,
                        author: entry.author.as_str().to_string(),
                        accounts: entry
                            .accounts
                            .iter()
                            .map(|delta| LedgerAccountDelta {
                                account_id: delta.account_id.as_str().to_string(),
                                currency: delta.currency.clone(),
                                debit: delta.debit,
                                credit: delta.credit,
                            })
                            .collect(),
                    }
                })
                .collect();

            // Apply pagination
            let page = paginate(all_entries, &page_request, DEFAULT_MAX_PAGE_SIZE);

            match serde_json::to_value(&page) {
                Ok(value) => RpcResponse::success(id, value),
                Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {e}")),
            }
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get entries: {e}")),
    }
}

/// Handle ledger.quarantine.list RPC call - list all quarantined entries (paginated)
pub async fn handle_quarantine_list(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let ledger_handle = match state.ledger_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Ledger not available".to_string());
        }
    };

    // Parse pagination parameters
    let page_request: PageRequest = serde_json::from_value(params.clone()).unwrap_or_default();

    let ledger = ledger_handle.read().await;
    match ledger.quarantine().list() {
        Ok(items) => {
            let items_json: Vec<serde_json::Value> = items
                .iter()
                .map(|item| {
                    serde_json::json!({
                        "entry_id": item.entry_id.to_hex(),
                        "reason": format!("{:?}", item.reason),
                        "author": format!("{:?}", item.author),
                        "observed_at": item.observed_at,
                        "metadata": item.metadata,
                    })
                })
                .collect();

            // Apply pagination
            let page = paginate(items_json, &page_request, DEFAULT_MAX_PAGE_SIZE);

            match serde_json::to_value(&page) {
                Ok(value) => RpcResponse::success(id, value),
                Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {e}")),
            }
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to list quarantine: {e}")),
    }
}

/// Handle ledger.quarantine.get RPC call - get a specific quarantined entry
pub async fn handle_quarantine_get(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let ledger_handle = match state.ledger_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Ledger not available".to_string());
        }
    };

    // Parse parameters
    #[derive(serde::Deserialize)]
    struct GetParams {
        entry_id: String,
    }

    let get_params: GetParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse entry ID
    let hash_bytes = match hex::decode(&get_params.entry_id) {
        Ok(bytes) if bytes.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            arr
        }
        _ => {
            return RpcResponse::error(id, -32602, "Invalid entry ID format".to_string());
        }
    };
    let entry_id = icn_ledger::ContentHash::from_bytes(hash_bytes);

    let ledger = ledger_handle.read().await;
    match ledger.quarantine().get(&entry_id) {
        Ok(Some((entry, item))) => {
            let result = serde_json::json!({
                "entry": {
                    "id": entry.id.map(|id| id.to_hex()),
                    "author": format!("{:?}", entry.author),
                    "parents": entry.parents.iter().map(|p| p.to_hex()).collect::<Vec<_>>(),
                    "timestamp": entry.timestamp,
                    "num_accounts": entry.accounts.len(),
                },
                "quarantine_info": {
                    "entry_id": item.entry_id.to_hex(),
                    "reason": format!("{:?}", item.reason),
                    "author": format!("{:?}", item.author),
                    "observed_at": item.observed_at,
                    "metadata": item.metadata,
                }
            });
            RpcResponse::success(id, result)
        }
        Ok(None) => RpcResponse::error(id, -32000, "Entry not found in quarantine".to_string()),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to get quarantine entry: {e}")),
    }
}

/// Handle ledger.quarantine.release RPC call - release an entry for retry
pub async fn handle_quarantine_release(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let ledger_handle = match state.ledger_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Ledger not available".to_string());
        }
    };

    // Parse parameters
    #[derive(serde::Deserialize)]
    struct ReleaseParams {
        entry_id: String,
    }

    let release_params: ReleaseParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse entry ID
    let hash_bytes = match hex::decode(&release_params.entry_id) {
        Ok(bytes) if bytes.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            arr
        }
        _ => {
            return RpcResponse::error(id, -32602, "Invalid entry ID format".to_string());
        }
    };
    let entry_id = icn_ledger::ContentHash::from_bytes(hash_bytes);

    let mut ledger = ledger_handle.write().await;
    match ledger.quarantine_mut().release(&entry_id) {
        Ok(Some(entry)) => {
            // Try to append the released entry back to the ledger
            // The intent of "release" is to retry the entry, so if reappend fails,
            // the operation has not fully succeeded and should return an error.
            match ledger.append_entry(entry) {
                Ok(_) => RpcResponse::success(
                    id,
                    serde_json::json!({
                        "released": true,
                        "reappended": true,
                        "entry_id": entry_id.to_hex()
                    }),
                ),
                Err(e) => RpcResponse::error(
                    id,
                    -32000,
                    format!("Entry released from quarantine but reappend failed: {e}"),
                ),
            }
        }
        Ok(None) => RpcResponse::error(id, -32000, "Entry not found in quarantine".to_string()),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to release entry: {e}")),
    }
}

/// Handle ledger.quarantine.drop RPC call - permanently drop an entry
pub async fn handle_quarantine_drop(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let ledger_handle = match state.ledger_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Ledger not available".to_string());
        }
    };

    // Parse parameters
    #[derive(serde::Deserialize)]
    struct DropParams {
        entry_id: String,
    }

    let drop_params: DropParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse entry ID
    let hash_bytes = match hex::decode(&drop_params.entry_id) {
        Ok(bytes) if bytes.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            arr
        }
        _ => {
            return RpcResponse::error(id, -32602, "Invalid entry ID format".to_string());
        }
    };
    let entry_id = icn_ledger::ContentHash::from_bytes(hash_bytes);

    let mut ledger = ledger_handle.write().await;
    match ledger.quarantine_mut().drop(&entry_id) {
        Ok(true) => RpcResponse::success(
            id,
            serde_json::json!({
                "dropped": true,
                "entry_id": entry_id.to_hex()
            }),
        ),
        Ok(false) => RpcResponse::error(id, -32000, "Entry not found in quarantine".to_string()),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to drop entry: {e}")),
    }
}

/// Handle ledger.quarantine.purge RPC call - purge all expired entries
pub async fn handle_quarantine_purge(id: u64, state: &Arc<RpcServer>) -> RpcResponse {
    let ledger_handle = match state.ledger_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(id, -32000, "Ledger not available".to_string());
        }
    };

    let mut ledger = ledger_handle.write().await;
    match ledger.quarantine_mut().purge_expired() {
        Ok(purged) => RpcResponse::success(
            id,
            serde_json::json!({
                "purged": purged
            }),
        ),
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to purge expired entries: {e}")),
    }
}

/// Handle receipt.get RPC call - get a receipt by ID
pub async fn handle_receipt_get(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    // Parse parameters
    #[derive(serde::Deserialize)]
    struct GetReceiptParams {
        receipt_id: String,
    }

    let get_params: GetReceiptParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    let receipt_id = crate::receipt::ReceiptId::from_string(get_params.receipt_id);

    match state.receipt_store().get(&receipt_id).await {
        Some(receipt) => match serde_json::to_value(&receipt) {
            Ok(value) => RpcResponse::success(id, value),
            Err(e) => RpcResponse::error(id, -32603, format!("Internal error: {e}")),
        },
        None => RpcResponse::error(id, -32000, "Receipt not found".to_string()),
    }
}
