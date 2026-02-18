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

    /// Find an existing journal entry by governance decision receipt id.
    ///
    /// This provides ledger-bound idempotency for treasury mutations:
    /// a receipt id may map to at most one durable entry.
    fn find_existing_entry_for_receipt(
        ledger: &Ledger,
        decision_receipt_id: &str,
    ) -> Result<Option<(String, Option<String>)>, String> {
        let entries = ledger
            .get_all_entries()
            .map_err(|e| format!("Failed to query ledger entries: {e}"))?;

        for mut existing_entry in entries {
            if existing_entry.decision_receipt_id.as_deref() != Some(decision_receipt_id) {
                continue;
            }

            let entry_hash_hex = if let Some(existing_hash) = existing_entry.id.clone() {
                existing_hash.to_hex()
            } else {
                existing_entry
                    .get_hash()
                    .map_err(|e| format!("Failed to compute existing entry hash: {e}"))?
                    .to_hex()
            };

            return Ok(Some((entry_hash_hex, existing_entry.decision_hash.clone())));
        }

        Ok(None)
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

        // Idempotent append at ledger boundary:
        // - If receipt_id already exists, return existing entry hash.
        // - Otherwise append exactly once.
        let (entry_hash_hex, was_idempotent_replay) = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let mut ledger = self.ledger.write().await;

                if let Some((existing_entry_hash, existing_decision_hash)) =
                    Self::find_existing_entry_for_receipt(&ledger, &req.decision_receipt_id)?
                {
                    if let Some(existing_decision_hash) = existing_decision_hash {
                        if existing_decision_hash != req.decision_hash {
                            return Err(format!(
                                "Decision hash mismatch for receipt {}: existing={}, requested={}",
                                req.decision_receipt_id, existing_decision_hash, req.decision_hash
                            ));
                        }
                    }

                    return Ok((existing_entry_hash, true));
                }

                let entry_hash = ledger
                    .append_entry(entry)
                    .await
                    .map_err(|e| format!("Failed to append ledger entry: {e}"))?;

                Ok((entry_hash.to_hex(), false))
            })
        })?;

        if was_idempotent_replay {
            info!(
                entry_hash = %entry_hash_hex,
                decision_receipt_id = %req.decision_receipt_id,
                decision_hash = %req.decision_hash,
                "Treasury entry already exists for decision receipt (idempotent replay)"
            );
        } else {
            info!(
                entry_hash = %entry_hash_hex,
                decision_receipt_id = %req.decision_receipt_id,
                decision_hash = %req.decision_hash,
                "Treasury entry submitted to ledger"
            );
        }

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
    use icn_identity::KeyPair;
    use icn_kernel_api::AllowAllOracle;
    use icn_store::SledStore;
    use tempfile::TempDir;

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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_submit_treasury_entry_is_idempotent_by_receipt_id() {
        let treasury_did = KeyPair::generate().unwrap().did().clone();
        let recipient_did = KeyPair::generate().unwrap().did().clone();

        let ledger_store = Arc::new(SledStore::temporary().unwrap());
        let ledger = Arc::new(RwLock::new(Ledger::new(ledger_store).unwrap()));
        let service = LedgerServiceImpl::new(
            ledger.clone(),
            Arc::new(AllowAllOracle::wildcard()),
            treasury_did.clone(),
        );

        let request = TreasuryEntryRequest {
            treasury_id: "t1".to_string(),
            operation_type: TreasuryOperationType::Spend,
            amount: 50,
            currency: "HOURS".to_string(),
            recipient: Some(recipient_did.to_string()),
            memo: "idempotency check".to_string(),
            decision_receipt_id: "gov:proposal:pr3:idempotency:001".to_string(),
            decision_hash: "decision-hash-pr3-001".to_string(),
        };

        let first = service.submit_treasury_entry(request.clone()).unwrap();
        let count_after_first = ledger.read().await.count_entries().unwrap();
        let treasury_balance_after_first = ledger.read().await.get_balance(&treasury_did, "HOURS");
        assert_eq!(count_after_first, 1);

        let second = service.submit_treasury_entry(request).unwrap();
        let count_after_second = ledger.read().await.count_entries().unwrap();
        let treasury_balance_after_second = ledger.read().await.get_balance(&treasury_did, "HOURS");

        assert_eq!(
            second.entry_hash, first.entry_hash,
            "replayed decision receipt id must return existing entry hash"
        );
        assert_eq!(
            count_after_second, 1,
            "ledger must append at most one entry per decision receipt id"
        );
        assert_eq!(
            treasury_balance_after_second, treasury_balance_after_first,
            "idempotent replay must not mutate balances"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_submit_treasury_entry_idempotency_survives_restart() {
        let treasury_did = KeyPair::generate().unwrap().did().clone();
        let recipient_did = KeyPair::generate().unwrap().did().clone();
        let tmp = TempDir::new().unwrap();
        let ledger_path = tmp.path().join("ledger");
        std::fs::create_dir_all(&ledger_path).unwrap();

        let request = TreasuryEntryRequest {
            treasury_id: "t1".to_string(),
            operation_type: TreasuryOperationType::Spend,
            amount: 75,
            currency: "HOURS".to_string(),
            recipient: Some(recipient_did.to_string()),
            memo: "restart idempotency".to_string(),
            decision_receipt_id: "gov:proposal:pr3:idempotency:restart".to_string(),
            decision_hash: "decision-hash-pr3-restart".to_string(),
        };

        let first_hash = {
            let ledger_store = Arc::new(SledStore::open(&ledger_path).unwrap());
            let ledger = Arc::new(RwLock::new(Ledger::new(ledger_store).unwrap()));
            let service = LedgerServiceImpl::new(
                ledger.clone(),
                Arc::new(AllowAllOracle::wildcard()),
                treasury_did.clone(),
            );

            let first = service.submit_treasury_entry(request.clone()).unwrap();
            assert_eq!(ledger.read().await.count_entries().unwrap(), 1);
            first.entry_hash
        };

        let second_hash = {
            let ledger_store = Arc::new(SledStore::open(&ledger_path).unwrap());
            let ledger = Arc::new(RwLock::new(Ledger::new(ledger_store).unwrap()));
            let service = LedgerServiceImpl::new(
                ledger.clone(),
                Arc::new(AllowAllOracle::wildcard()),
                treasury_did,
            );

            let second = service.submit_treasury_entry(request).unwrap();
            assert_eq!(ledger.read().await.count_entries().unwrap(), 1);
            second.entry_hash
        };

        assert_eq!(
            second_hash, first_hash,
            "receipt-based idempotency must survive process restart"
        );
    }
}
