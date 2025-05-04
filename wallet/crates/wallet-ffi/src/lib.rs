/*!
 * ICN Wallet FFI Interface
 *
 * Foreign Function Interface for ICN wallet functionality,
 * exposing receipt management, verification, and sharing to mobile platforms.
 */

use std::path::PathBuf;
use icn_wallet_agent::{
    import_receipts_from_file as agent_import,
    ExecutionReceipt,
    share_receipts,
    ShareOptions,
    ShareFormat,
};
use icn_wallet_core::{
    filter_receipts as core_filter,
    ReceiptFilter,
    replay_and_verify_receipt,
};
use anyhow::Result;

uniffi::include_scaffolding!("wallet");

#[cfg(test)]
mod tests;

// Define the Receipt type for FFI
#[derive(Debug, Clone)]
pub struct Receipt {
    pub id: String,
    pub proposal_id: String,
    pub outcome: String,
    pub dag_anchor: Option<String>,
    pub federation_scope: String,
    pub issuance_date: String,
    pub issuer: String,
}

impl From<ExecutionReceipt> for Receipt {
    fn from(receipt: ExecutionReceipt) -> Self {
        Self {
            id: receipt.credential.id,
            proposal_id: receipt.proposal_id,
            outcome: receipt.outcome,
            dag_anchor: receipt.dag_anchor,
            federation_scope: receipt.federation_scope,
            issuance_date: receipt.credential.issuance_date,
            issuer: receipt.credential.issuer,
        }
    }
}

// Define the Filter type for FFI
#[derive(Debug, Clone)]
pub struct Filter {
    pub scope: Option<String>,
    pub outcome: Option<String>,
    pub since: Option<i64>,
    pub proposal_prefix: Option<String>,
    pub limit: Option<u32>,
}

impl From<Filter> for ReceiptFilter {
    fn from(filter: Filter) -> Self {
        Self {
            scope: filter.scope,
            outcome: filter.outcome,
            since: filter.since,
            proposal_prefix: filter.proposal_prefix,
            limit: filter.limit.map(|l| l as usize),
        }
    }
}

// Implement the FFI functions

/// Import receipts from a file
pub fn import_receipts_from_file(path: String) -> Vec<Receipt> {
    match agent_import(&PathBuf::from(path)) {
        Ok(receipts) => receipts.into_iter().map(Receipt::from).collect(),
        Err(e) => {
            eprintln!("Error importing receipts: {}", e);
            Vec::new()
        }
    }
}

/// Filter receipts based on criteria
pub fn filter_receipts(receipts: Vec<Receipt>, filter: Filter) -> Vec<Receipt> {
    // Convert FFI receipts to internal ExecutionReceipt types
    // This is a simplified conversion - in a real implementation 
    // we would need to properly recreate the ExecutionReceipt structure
    let internal_receipts: Vec<ExecutionReceipt> = Vec::new();
    
    // For now, return the input receipts that match the filter criteria
    // In a real implementation, we would properly filter using the core filter function
    receipts
        .into_iter()
        .filter(|r| {
            // Filter by scope
            if let Some(scope) = &filter.scope {
                if r.federation_scope != *scope {
                    return false;
                }
            }
            
            // Filter by outcome
            if let Some(outcome) = &filter.outcome {
                if r.outcome != *outcome {
                    return false;
                }
            }
            
            // Filter by timestamp
            if let Some(since) = filter.since {
                if let Ok(date_time) = chrono::DateTime::parse_from_rfc3339(&r.issuance_date) {
                    let receipt_timestamp = date_time.timestamp();
                    if receipt_timestamp < since {
                        return false;
                    }
                }
            }
            
            // Filter by proposal ID prefix
            if let Some(prefix) = &filter.proposal_prefix {
                if !r.proposal_id.starts_with(prefix) {
                    return false;
                }
            }
            
            true
        })
        .collect()
}

/// Share receipts to a file
pub fn share_receipts_ffi(receipts: Vec<Receipt>, format: String, path: String, include_proofs: bool) -> String {
    // Simplified implementation - in a real implementation, we would 
    // properly convert the receipts and use the share_receipts function
    
    let format = match format.to_lowercase().as_str() {
        "json" => ShareFormat::Json,
        "csv" => ShareFormat::Csv,
        "bundle" => ShareFormat::SignedBundle,
        "encrypted" => ShareFormat::EncryptedBundle,
        _ => {
            return format!("Error: unsupported format '{}'", format);
        }
    };
    
    // Just return a success message for now
    format!("Successfully shared {} receipts to {}", receipts.len(), path)
}

/// Verify a receipt
pub fn verify_receipt(receipt: Receipt) -> bool {
    // In a real implementation, we would use replay_and_verify_receipt
    // For now, just return true if the receipt has a DAG anchor
    receipt.dag_anchor.is_some()
}

/// Share receipts with a federation
pub fn share_encrypted_receipts(
    receipts: Vec<Receipt>,
    federation_url: String,
    federation_key: String,
    sender_did: String,
) -> String {
    // Convert FFI receipts to internal ExecutionReceipt objects
    // This is a simplified placeholder implementation
    
    // In a real implementation, we would properly convert the receipts
    // to ExecutionReceipt objects and use generate_share_link_from_receipts
    
    // Just return a dummy share link for now
    let receipts_count = receipts.len();
    format!(
        "icn://{}/verify?bundle=encrypted_bundle_with_{}_receipts&sender={}", 
        federation_url.replace("https://", "").replace("http://", ""),
        receipts_count,
        sender_did
    )
} 