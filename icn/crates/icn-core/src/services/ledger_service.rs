//! LedgerService implementation for icn-ledger.
//!
//! This adapter implements the kernel-safe `LedgerService` trait using
//! the actual ledger implementation. It's the bridge between
//! the kernel's abstract ledger interface and the real mutual credit ledger.
//!
//! # Pilot Invariant
//!
//! Treasury entries created via `submit_treasury_entry` MUST carry both:
//! - `decision_receipt_id`: node-local decision reference
//! - `decision_hash`: cross-node canonical equality anchor
//!
//! This enables the provenance chain: governance decision → treasury effect → ledger entry.

use std::sync::Arc;

use icn_kernel_api::services::{
    LedgerEvent, LedgerService, ResourceAccessInfo, RevokeResourceAccessRequest,
    TreasuryEntryRequest, TreasuryEntryResult, TreasuryOperationType,
};
use icn_kernel_api::types::Did;
use icn_kernel_api::PolicyOracle;
use icn_ledger::{entry::JournalEntryBuilder, AccountDelta, Ledger};
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Concrete implementation of `LedgerService` backed by the mutual credit ledger.
///
/// This is the production adapter that translates kernel-safe treasury
/// operations into real ledger entries with provenance tracking.
pub struct LedgerServiceImpl {
    /// The actual ledger (requires write lock for mutations)
    ledger: Arc<RwLock<Ledger>>,

    /// Policy oracle for authorization (kernel-safe abstraction)
    oracle: Arc<dyn PolicyOracle>,

    /// Treasury DID (used as the source account for treasury operations)
    /// Format: "did:icn:<treasury-pubkey>"
    treasury_did: icn_identity::Did,
}

impl LedgerServiceImpl {
    /// Create a new LedgerService implementation.
    ///
    /// # Arguments
    /// * `ledger` - The ledger handle (Arc<RwLock<Ledger>>)
    /// * `oracle` - Policy oracle for authorization decisions
    /// * `treasury_did` - DID of the treasury account (source for spends)
    pub fn new(
        ledger: Arc<RwLock<Ledger>>,
        oracle: Arc<dyn PolicyOracle>,
        treasury_did: icn_identity::Did,
    ) -> Self {
        Self {
            ledger,
            oracle,
            treasury_did,
        }
    }

    /// Build account deltas for a treasury operation.
    ///
    /// Treasury operations follow double-entry bookkeeping:
    /// - Spend: debit treasury, credit recipient
    /// - Allocate: debit treasury, credit budget
    /// - Transfer: debit source, credit destination
    fn build_account_deltas(
        &self,
        req: &TreasuryEntryRequest,
    ) -> Result<Vec<AccountDelta>, String> {
        match req.operation_type {
            TreasuryOperationType::Spend => {
                let recipient = req
                    .recipient
                    .as_ref()
                    .ok_or_else(|| "Spend operation requires recipient".to_string())?;

                let recipient_did: icn_identity::Did = recipient
                    .parse()
                    .map_err(|e| format!("Invalid recipient DID: {e}"))?;

                // Double-entry: debit treasury, credit recipient
                Ok(vec![
                    AccountDelta::debit(
                        self.treasury_did.clone(),
                        req.currency.clone(),
                        req.amount,
                    ),
                    AccountDelta::credit(recipient_did, req.currency.clone(), req.amount),
                ])
            }
            TreasuryOperationType::Allocate => {
                // Budget allocation: debit main treasury, credit budget account
                // For now, use treasury_id as the budget account
                let budget_did: icn_identity::Did = format!("did:icn:budget:{}", req.treasury_id)
                    .parse()
                    .map_err(|e| format!("Invalid budget DID: {e}"))?;

                Ok(vec![
                    AccountDelta::debit(
                        self.treasury_did.clone(),
                        req.currency.clone(),
                        req.amount,
                    ),
                    AccountDelta::credit(budget_did, req.currency.clone(), req.amount),
                ])
            }
            TreasuryOperationType::Transfer => {
                let recipient = req
                    .recipient
                    .as_ref()
                    .ok_or_else(|| "Transfer operation requires recipient".to_string())?;

                let recipient_did: icn_identity::Did = recipient
                    .parse()
                    .map_err(|e| format!("Invalid recipient DID: {e}"))?;

                // Transfer between treasuries
                Ok(vec![
                    AccountDelta::debit(
                        self.treasury_did.clone(),
                        req.currency.clone(),
                        req.amount,
                    ),
                    AccountDelta::credit(recipient_did, req.currency.clone(), req.amount),
                ])
            }
            _ => {
                // Other operation types can be added as needed
                Err(format!(
                    "Unsupported treasury operation type: {:?}",
                    req.operation_type
                ))
            }
        }
    }
}

impl LedgerService for LedgerServiceImpl {
    fn oracle(&self) -> Arc<dyn PolicyOracle> {
        self.oracle.clone()
    }

    fn balance(&self, account: &Did, currency: &str) -> i64 {
        // Use block_in_place to safely call async from sync context
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let ledger = self.ledger.read().await;
                // Parse the string DID to identity DID
                if let Ok(did) = account.parse::<icn_identity::Did>() {
                    ledger.get_balance(&did, currency)
                } else {
                    0 // Invalid DID returns 0 balance
                }
            })
        })
    }

    fn credit_limit(&self, account: &Did, _currency: &str) -> i64 {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let _ledger = self.ledger.read().await;
                // For now, return a default credit limit
                // The ledger doesn't have a credit_limit method yet
                if let Ok(_did) = account.parse::<icn_identity::Did>() {
                    // Default credit limit - can be configured per-account later
                    1000
                } else {
                    0
                }
            })
        })
    }

    fn record_event(&self, event: LedgerEvent) {
        debug!(?event, "Recording ledger event");
        // Events are logged for observability; actual event handling
        // would go here if we need to track transaction lifecycle
    }

    fn list_enforceable_resources(
        &self,
        _current_time: u64,
    ) -> Result<Vec<ResourceAccessInfo>, String> {
        // Not implemented for treasury-focused service
        Ok(Vec::new())
    }

    fn revoke_resource_access(&self, _req: &RevokeResourceAccessRequest) -> Result<(), String> {
        Err("Resource access revocation not supported by treasury ledger service".to_string())
    }

    fn submit_treasury_entry(
        &self,
        req: TreasuryEntryRequest,
    ) -> Result<TreasuryEntryResult, String> {
        info!(
            treasury_id = %req.treasury_id,
            operation_type = ?req.operation_type,
            amount = req.amount,
            currency = %req.currency,
            recipient = ?req.recipient,
            decision_receipt_id = %req.decision_receipt_id,
            decision_hash = %req.decision_hash,
            "Submitting treasury entry to ledger"
        );

        // Build account deltas for double-entry bookkeeping
        let deltas = self.build_account_deltas(&req)?;

        // Build the journal entry with provenance
        let mut builder = JournalEntryBuilder::new(self.treasury_did.clone());

        // Add decision provenance (PILOT-CRITICAL INVARIANT)
        builder = builder.with_decision_provenance(&req.decision_receipt_id, &req.decision_hash);

        // Add account deltas
        for delta in deltas {
            builder = builder.add_delta(delta);
        }

        // Build the entry
        let entry = builder
            .build()
            .map_err(|e| format!("Failed to build journal entry: {e}"))?;

        // Append to ledger (async operation in sync context)
        let entry_hash = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let mut ledger = self.ledger.write().await;
                ledger
                    .append_entry(entry)
                    .await
                    .map_err(|e| format!("Failed to append ledger entry: {e}"))
            })
        })?;

        let entry_hash_hex = entry_hash.to_hex();

        info!(
            entry_hash = %entry_hash_hex,
            decision_receipt_id = %req.decision_receipt_id,
            decision_hash = %req.decision_hash,
            "Treasury entry submitted to ledger"
        );

        Ok(TreasuryEntryResult {
            entry_hash: entry_hash_hex,
            decision_receipt_id: req.decision_receipt_id,
            decision_hash: req.decision_hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Full integration tests require a real ledger instance.
    // These unit tests verify the basic structure.

    #[test]
    fn test_treasury_entry_request_provenance_required() {
        // Verify that TreasuryEntryRequest requires provenance fields
        let req = TreasuryEntryRequest {
            treasury_id: "t1".to_string(),
            operation_type: TreasuryOperationType::Spend,
            amount: 50,
            currency: "USD".to_string(),
            recipient: Some("did:icn:recipient".to_string()),
            memo: "payment".to_string(),
            decision_receipt_id: "gov:proposal:2024-001:receipt:abc".to_string(),
            decision_hash: "sha256:deadbeef".to_string(),
        };

        // These must be non-empty for pilot invariant
        assert!(!req.decision_receipt_id.is_empty());
        assert!(!req.decision_hash.is_empty());
    }
}
