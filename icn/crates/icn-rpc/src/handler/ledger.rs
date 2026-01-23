//! Ledger-related RPC handlers
//!
//! # Coop Isolation
//!
//! TODO(#769): Add `ctx.require_coop()` enforcement when multi-coop ledgers are implemented.
//! Currently the ledger is global. When per-coop ledgers exist, handlers should:
//! 1. Require `ctx` to be `Some` for all operations
//! 2. Call `ctx.require_coop(requested_coop_id)` to validate access
//! 3. Route requests to the appropriate coop-scoped ledger

use std::sync::Arc;

use crate::context::RpcContext;
use crate::error_codes::{INVALID_PARAMS, RESOURCE_NOT_AVAILABLE};
use crate::pagination::{paginate, PageRequest, DEFAULT_MAX_PAGE_SIZE};
use crate::server::RpcServer;
use crate::types::{LedgerAccountDelta, LedgerBalance, LedgerEntry, RpcResponse};

/// Handle ledger.head RPC call - get the most recent ledger entry
///
/// Note: Currently returns the global ledger head. When multi-coop ledgers
/// are implemented, this will respect the coop context for isolation.
pub async fn handle_ledger_head(
    id: u64,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    // Log context for future coop isolation (currently ledger is global)
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "ledger.head called"
        );
    }

    let ledger_handle = match state.ledger_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(
                id,
                RESOURCE_NOT_AVAILABLE,
                "Ledger not available".to_string(),
            );
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
                    Err(e) => RpcResponse::internal_error(id, e),
                }
            } else {
                RpcResponse::success(id, serde_json::json!(null))
            }
        }
        Err(e) => RpcResponse::internal_error(id, e),
    }
}

/// Handle ledger.balance RPC call - get balance for an account
///
/// Note: Currently returns global balances. When multi-coop ledgers
/// are implemented, this will respect the coop context for isolation.
pub async fn handle_ledger_balance(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    // Log context for future coop isolation
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "ledger.balance called"
        );
    }

    let ledger_handle = match state.ledger_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(
                id,
                RESOURCE_NOT_AVAILABLE,
                "Ledger not available".to_string(),
            );
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
            return RpcResponse::error(id, INVALID_PARAMS, format!("Invalid params: {e}"));
        }
    };

    let account_did = match serde_json::from_value(serde_json::Value::String(
        balance_params.account_id.clone(),
    )) {
        Ok(d) => d,
        Err(e) => {
            return RpcResponse::error(id, INVALID_PARAMS, format!("Invalid DID: {e}"));
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
            Err(e) => RpcResponse::internal_error(id, e),
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
            Err(e) => RpcResponse::internal_error(id, e),
        }
    }
}

/// Handle ledger.history RPC call - get recent ledger entries (paginated)
///
/// Uses the ledger's efficient pagination API to avoid loading all entries
/// into memory when only a subset is requested.
///
/// Note: Currently returns global history. When multi-coop ledgers
/// are implemented, this will respect the coop context for isolation.
pub async fn handle_ledger_history(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    // Log context for future coop isolation
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "ledger.history called"
        );
    }

    let ledger_handle = match state.ledger_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(
                id,
                RESOURCE_NOT_AVAILABLE,
                "Ledger not available".to_string(),
            );
        }
    };

    // Parse pagination parameters with explicit validation
    let page_request: PageRequest = match serde_json::from_value(params.clone()) {
        Ok(req) => req,
        Err(e) => {
            // Check if params is null/empty (allow default) vs malformed (reject)
            if params.is_null()
                || (params.is_object() && params.as_object().is_none_or(|o| o.is_empty()))
            {
                PageRequest::default()
            } else {
                return RpcResponse::error(
                    id,
                    INVALID_PARAMS,
                    format!("Invalid pagination params: {e}. Expected {{\"offset\": <number>, \"limit\": <number>}}")
                );
            }
        }
    };

    // Cap limit to server maximum
    let limit = page_request.limit.min(DEFAULT_MAX_PAGE_SIZE);

    let ledger = ledger_handle.read().await;

    // Use paginated API - entries are returned newest-first
    match ledger.get_entries_paginated(page_request.offset, limit) {
        Ok((entries, total)) => {
            // Convert only the paginated entries to RPC types
            let rpc_entries: Vec<LedgerEntry> = entries
                .iter()
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

            // Build paginated response
            let has_more = page_request.offset + rpc_entries.len() < total;
            let response = crate::pagination::PageResponse {
                items: rpc_entries,
                total,
                has_more,
                offset: Some(page_request.offset),
                limit: Some(limit),
            };

            match serde_json::to_value(&response) {
                Ok(value) => RpcResponse::success(id, value),
                Err(e) => RpcResponse::internal_error(id, e),
            }
        }
        Err(e) => RpcResponse::internal_error(id, e),
    }
}

/// Handle ledger.quarantine.list RPC call - list all quarantined entries (paginated)
pub async fn handle_quarantine_list(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "ledger.quarantine.list called"
        );
    }

    let ledger_handle = match state.ledger_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(
                id,
                RESOURCE_NOT_AVAILABLE,
                "Ledger not available".to_string(),
            );
        }
    };

    // Parse pagination parameters with explicit validation
    let page_request: PageRequest = match serde_json::from_value(params.clone()) {
        Ok(req) => req,
        Err(e) => {
            if params.is_null()
                || (params.is_object() && params.as_object().is_none_or(|o| o.is_empty()))
            {
                PageRequest::default()
            } else {
                return RpcResponse::error(
                    id,
                    INVALID_PARAMS,
                    format!("Invalid pagination params: {e}. Expected {{\"offset\": <number>, \"limit\": <number>}}")
                );
            }
        }
    };

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
                Err(e) => RpcResponse::internal_error(id, e),
            }
        }
        Err(e) => RpcResponse::internal_error(id, e),
    }
}

/// Handle ledger.quarantine.get RPC call - get a specific quarantined entry
pub async fn handle_quarantine_get(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    use crate::error_codes::NOT_FOUND;

    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "ledger.quarantine.get called"
        );
    }

    let ledger_handle = match state.ledger_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(
                id,
                RESOURCE_NOT_AVAILABLE,
                "Ledger not available".to_string(),
            );
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
            return RpcResponse::error(id, INVALID_PARAMS, format!("Invalid params: {e}"));
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
            return RpcResponse::error(
                id,
                INVALID_PARAMS,
                "Invalid entry ID format. Expected 64 hex characters.".to_string(),
            );
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
        Ok(None) => RpcResponse::error(id, NOT_FOUND, "Entry not found in quarantine".to_string()),
        Err(e) => RpcResponse::internal_error(id, e),
    }
}

/// Handle ledger.quarantine.release RPC call - release an entry for retry
pub async fn handle_quarantine_release(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    use crate::error_codes::{LEDGER_ERROR, NOT_FOUND};

    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "ledger.quarantine.release called"
        );
    }

    let ledger_handle = match state.ledger_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(
                id,
                RESOURCE_NOT_AVAILABLE,
                "Ledger not available".to_string(),
            );
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
            return RpcResponse::error(id, INVALID_PARAMS, format!("Invalid params: {e}"));
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
            return RpcResponse::error(
                id,
                INVALID_PARAMS,
                "Invalid entry ID format. Expected 64 hex characters.".to_string(),
            );
        }
    };
    let entry_id = icn_ledger::ContentHash::from_bytes(hash_bytes);

    let mut ledger = ledger_handle.write().await;
    match ledger.quarantine_mut().release(&entry_id) {
        Ok(Some(entry)) => {
            // Try to append the released entry back to the ledger
            // The intent of "release" is to retry the entry, so if reappend fails,
            // the operation has not fully succeeded and should return an error.
            match ledger.append_entry(entry).await {
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
                    LEDGER_ERROR,
                    format!("Entry released from quarantine but reappend failed: {e}"),
                ),
            }
        }
        Ok(None) => RpcResponse::error(id, NOT_FOUND, "Entry not found in quarantine".to_string()),
        Err(e) => RpcResponse::internal_error(id, e),
    }
}

/// Handle ledger.quarantine.drop RPC call - permanently drop an entry
pub async fn handle_quarantine_drop(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    use crate::error_codes::NOT_FOUND;

    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "ledger.quarantine.drop called"
        );
    }

    let ledger_handle = match state.ledger_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(
                id,
                RESOURCE_NOT_AVAILABLE,
                "Ledger not available".to_string(),
            );
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
            return RpcResponse::error(id, INVALID_PARAMS, format!("Invalid params: {e}"));
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
            return RpcResponse::error(
                id,
                INVALID_PARAMS,
                "Invalid entry ID format. Expected 64 hex characters.".to_string(),
            );
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
        Ok(false) => RpcResponse::error(id, NOT_FOUND, "Entry not found in quarantine".to_string()),
        Err(e) => RpcResponse::internal_error(id, e),
    }
}

/// Handle ledger.quarantine.purge RPC call - purge all expired entries
pub async fn handle_quarantine_purge(
    id: u64,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "ledger.quarantine.purge called"
        );
    }

    let ledger_handle = match state.ledger_handle() {
        Some(handle) => handle,
        None => {
            return RpcResponse::error(
                id,
                RESOURCE_NOT_AVAILABLE,
                "Ledger not available".to_string(),
            );
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
        Err(e) => RpcResponse::internal_error(id, e),
    }
}

/// Handle receipt.get RPC call - get a receipt by ID
pub async fn handle_receipt_get(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    ctx: Option<&RpcContext>,
) -> RpcResponse {
    use crate::error_codes::NOT_FOUND;

    if let Some(ctx) = ctx {
        tracing::debug!(
            caller = %ctx.caller_did,
            coop_id = ?ctx.coop_id,
            "receipt.get called"
        );
    }

    // Parse parameters
    #[derive(serde::Deserialize)]
    struct GetReceiptParams {
        receipt_id: String,
    }

    let get_params: GetReceiptParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, INVALID_PARAMS, format!("Invalid params: {e}"));
        }
    };

    let receipt_id = crate::receipt::ReceiptId::from_string(get_params.receipt_id);

    match state.receipt_store().get(&receipt_id).await {
        Some(receipt) => match serde_json::to_value(&receipt) {
            Ok(value) => RpcResponse::success(id, value),
            Err(e) => RpcResponse::internal_error(id, e),
        },
        None => RpcResponse::error(id, NOT_FOUND, "Receipt not found".to_string()),
    }
}
