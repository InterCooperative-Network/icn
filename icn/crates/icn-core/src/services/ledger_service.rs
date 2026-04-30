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

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ed25519_dalek::SigningKey;
use icn_kernel_api::services::{
    LedgerEvent, LedgerService, ResourceAccessInfo, RevokeResourceAccessRequest,
    TreasuryEntryRequest, TreasuryEntryResult, TreasuryOperationType,
};
use icn_kernel_api::types::Did;
use icn_kernel_api::PolicyOracle;
use icn_ledger::{entry::JournalEntryBuilder, types::ProvenanceRef, AccountDelta, Ledger};
use icn_store::SledStore;
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;
use tracing::{debug, info};

const RECEIPT_INDEX_PREFIX: &str = "ledger:receipt_index:";
const TREASURY_NONCE_PREFIX: &str = "ledger:treasury_nonce:";
const RECEIPT_INDEX_PENDING: &[u8] = b"__pending__";
const RECEIPT_INDEX_WAIT_ATTEMPTS: usize = 25;
const RECEIPT_INDEX_WAIT_MS: u64 = 20;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ReceiptIndexRecord {
    entry_hash: String,
    decision_hash: String,
}

/// Deterministic synthetic DID for a budget liability line on the ledger.
///
/// Budget IDs from governance are opaque strings. We hash `(treasury, budget_key,
/// currency)` to a 32-byte seed, clamp to an Ed25519 signing key, and derive the
/// canonical `did:icn:z…` from the verifying key so ledger entry walks never hit
/// invalid curve points.
fn budget_liability_did(treasury_id: &str, budget_key: &str, currency: &str) -> icn_identity::Did {
    let mut hasher = Sha256::new();
    hasher.update(b"icn:budget:liability:v1\0");
    hasher.update(treasury_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(budget_key.as_bytes());
    hasher.update(b"\0");
    hasher.update(currency.as_bytes());
    let seed: [u8; 32] = hasher.finalize().into();
    let signing = SigningKey::from_bytes(&seed);
    icn_identity::Did::from_public_key(&signing.verifying_key())
}

/// Derive a deterministic synthetic DID representing the bond-payable liability account.
///
/// When a cooperative issues a bond, the bond principal is received as capital
/// and a corresponding liability is tracked under this synthetic DID.  The same
/// inputs always produce the same DID so the account is stable across replays.
fn bond_payable_did(treasury_id: &str, bond_id: &str, currency: &str) -> icn_identity::Did {
    let mut hasher = Sha256::new();
    hasher.update(b"icn:bond:payable:v1\0");
    hasher.update(treasury_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(bond_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(currency.as_bytes());
    let seed: [u8; 32] = hasher.finalize().into();
    let signing = SigningKey::from_bytes(&seed);
    icn_identity::Did::from_public_key(&signing.verifying_key())
}

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

    /// Optional receipt-id index store for O(1) treasury idempotency.
    ///
    /// Keys: `ledger:receipt_index:{decision_receipt_id}`
    /// Values: `ReceiptIndexRecord` JSON or pending marker during in-flight append.
    receipt_index: Option<Arc<SledStore>>,

    /// Fallback in-memory nonce state when no index store is configured.
    treasury_nonce_cache: Mutex<HashMap<String, u64>>,
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
            receipt_index: None,
            treasury_nonce_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Create a LedgerService with a receipt-id index for idempotent append claims.
    pub fn new_with_receipt_index(
        ledger: Arc<RwLock<Ledger>>,
        oracle: Arc<dyn PolicyOracle>,
        treasury_did: icn_identity::Did,
        receipt_index: Arc<SledStore>,
    ) -> Self {
        Self {
            ledger,
            oracle,
            treasury_did,
            receipt_index: Some(receipt_index),
            treasury_nonce_cache: Mutex::new(HashMap::new()),
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
                // Budget allocation: debit main treasury, credit budget liability account.
                // `recipient` carries the app budget identifier (see `treasury_effect_to_operation`).
                let budget_key = req.recipient.as_ref().ok_or_else(|| {
                    "Allocate operation requires budget identifier (recipient field)".to_string()
                })?;
                let budget_did = budget_liability_did(&req.treasury_id, budget_key, &req.currency);

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
            TreasuryOperationType::DistributeSurplus => {
                // Fan-out: one debit per distribution, one credit per member.
                // All deltas land in a single JournalEntry so the one-receipt-one-entry
                // idempotency invariant is preserved across retries.
                if req.distributions.is_empty() {
                    return Err(
                        "DistributeSurplus requires at least one distribution (empty list)"
                            .to_string(),
                    );
                }

                // Validate individual amounts are positive (zero/negative inverts
                // debit/credit semantics and violates AccountDelta type contract).
                if req.amount <= 0 {
                    return Err(format!(
                        "DistributeSurplus: total amount must be positive, got {}",
                        req.amount
                    ));
                }
                for (did, amount) in &req.distributions {
                    if *amount <= 0 {
                        return Err(format!(
                            "DistributeSurplus: distribution amount for '{did}' must be \
                             positive, got {amount}"
                        ));
                    }
                }

                // Validate: sum of individual amounts must equal req.amount to prevent
                // the logged total from diverging from actual ledger mutations.
                // Use checked arithmetic to surface overflow before it corrupts ledger state.
                let distribution_sum: i64 = req
                    .distributions
                    .iter()
                    .try_fold(0i64, |acc, (_, amt)| acc.checked_add(*amt))
                    .ok_or_else(|| {
                        "DistributeSurplus: distribution amounts overflow i64".to_string()
                    })?;
                if distribution_sum != req.amount {
                    return Err(format!(
                        "DistributeSurplus: distribution sum ({distribution_sum}) \
                         does not equal total amount ({})",
                        req.amount
                    ));
                }

                // Sort by recipient DID string to canonicalize delta ordering.
                // The ledger entry hash covers the AccountDelta sequence, so ordering
                // must be deterministic across all nodes processing the same decision.
                let mut sorted: Vec<&(String, i64)> = req.distributions.iter().collect();
                sorted.sort_by(|(a, _), (b, _)| a.cmp(b));

                let mut deltas = Vec::with_capacity(req.distributions.len() * 2);
                for (member_did_str, amount) in sorted {
                    let member_did: icn_identity::Did = member_did_str
                        .parse()
                        .map_err(|e| format!("Invalid member DID in distribution: {e}"))?;
                    deltas.push(AccountDelta::debit(
                        self.treasury_did.clone(),
                        req.currency.clone(),
                        *amount,
                    ));
                    deltas.push(AccountDelta::credit(
                        member_did,
                        req.currency.clone(),
                        *amount,
                    ));
                }
                Ok(deltas)
            }
            TreasuryOperationType::IssueBond => {
                // Bond issuance: cooperative receives principal (treasury incurs liability,
                // bond-payable liability account is credited with the capital extended).
                //
                // In ICN mutual credit: credit(treasury) → treasury balance decreases
                // (it is in debt to bond holders); debit(bond_payable) → liability account
                // balance increases (it has distributed capital).
                //
                // The bond_payable DID is derived deterministically so the same bond always
                // maps to the same synthetic liability account (idempotency-safe).
                let bond_id = req
                    .recipient
                    .as_ref()
                    .ok_or_else(|| "IssueBond requires bond_id in recipient field".to_string())?;
                let bond_payable = bond_payable_did(&req.treasury_id, bond_id, &req.currency);
                Ok(vec![
                    AccountDelta::credit(
                        self.treasury_did.clone(),
                        req.currency.clone(),
                        req.amount,
                    ),
                    AccountDelta::debit(bond_payable, req.currency.clone(), req.amount),
                ])
            }

            TreasuryOperationType::RedeemShares => {
                // Share redemption: treasury pays out to departing member.
                //
                // In ICN mutual credit: debit(treasury) → treasury balance increases
                // (same direction as Spend — treasury is the sending side);
                // credit(member) → member receives payout (balance changes reflect settlement).
                //
                // `recipient` carries the member DID passed through from the governance effect.
                let member = req.recipient.as_ref().ok_or_else(|| {
                    "RedeemShares requires member DID in recipient field".to_string()
                })?;
                let member_did: icn_identity::Did = member
                    .parse()
                    .map_err(|e| format!("Invalid member DID in RedeemShares: {e}"))?;
                Ok(vec![
                    AccountDelta::debit(
                        self.treasury_did.clone(),
                        req.currency.clone(),
                        req.amount,
                    ),
                    AccountDelta::credit(member_did, req.currency.clone(), req.amount),
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
    fn find_existing_entry_for_receipt(
        ledger: &Ledger,
        decision_receipt_id: &str,
    ) -> Result<Option<(String, Option<String>)>, String> {
        let entries = ledger
            .get_all_entries()
            .map_err(|e| format!("Failed to query ledger entries: {e}"))?;

        for mut existing_entry in entries {
            let (entry_receipt_id, entry_decision_hash) = match &existing_entry.provenance {
                ProvenanceRef::Governance {
                    receipt_id,
                    decision_hash,
                } => (Some(receipt_id.as_str()), Some(decision_hash.clone())),
                _ => (None, None),
            };

            if entry_receipt_id != Some(decision_receipt_id) {
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

            return Ok(Some((entry_hash_hex, entry_decision_hash)));
        }

        Ok(None)
    }

    fn receipt_index_key(decision_receipt_id: &str) -> Vec<u8> {
        format!("{RECEIPT_INDEX_PREFIX}{decision_receipt_id}").into_bytes()
    }

    fn treasury_nonce_key(treasury_id: &str) -> Vec<u8> {
        format!("{TREASURY_NONCE_PREFIX}{treasury_id}").into_bytes()
    }

    fn current_treasury_nonce(&self, treasury_id: &str) -> Result<u64, String> {
        if let Some(index) = self.receipt_index.as_ref() {
            let key = Self::treasury_nonce_key(treasury_id);
            let raw = index
                .db()
                .get(key.as_slice())
                .map_err(|e| format!("Failed to read treasury nonce from index: {e}"))?;
            return match raw {
                Some(bytes) => {
                    if bytes.len() != 8 {
                        Err(format!(
                            "Corrupt treasury nonce for {}: expected 8 bytes, got {}",
                            treasury_id,
                            bytes.len()
                        ))
                    } else {
                        let mut arr = [0u8; 8];
                        arr.copy_from_slice(bytes.as_ref());
                        Ok(u64::from_le_bytes(arr))
                    }
                }
                None => Ok(0),
            };
        }

        let cache = self
            .treasury_nonce_cache
            .lock()
            .map_err(|_| "Treasury nonce cache lock poisoned".to_string())?;
        Ok(*cache.get(treasury_id).unwrap_or(&0))
    }

    fn set_treasury_nonce(&self, treasury_id: &str, nonce: u64) -> Result<(), String> {
        if let Some(index) = self.receipt_index.as_ref() {
            let key = Self::treasury_nonce_key(treasury_id);
            index
                .db()
                .insert(key.as_slice(), nonce.to_le_bytes().as_slice())
                .map_err(|e| format!("Failed to persist treasury nonce: {e}"))?;
            return Ok(());
        }

        let mut cache = self
            .treasury_nonce_cache
            .lock()
            .map_err(|_| "Treasury nonce cache lock poisoned".to_string())?;
        cache.insert(treasury_id.to_string(), nonce);
        Ok(())
    }

    // Treasury Spend enforcement contract:
    //
    // 1) Receipt idempotency (`decision_receipt_id`) prevents re-applying the same decision.
    //    Same receipt returns an existing entry hash with no new append.
    // 2) Nonce enforcement prevents stale/out-of-order spends.
    //    It is evaluated only when receipt idempotency does not short-circuit.
    // 3) Nonce check + append must run under the same ledger write lock in
    //    `submit_treasury_entry`, so mismatch rejects without partial mutation.
    //
    // Meaning Firewall note:
    // - `expected_nonce` is supplied at the policy boundary (wallet/proposal payload).
    // - Translator/wiring does not read ledger state to derive nonce.
    // - Ledger is the enforcement boundary and returns deterministic mismatch errors.
    fn check_spend_nonce(&self, req: &TreasuryEntryRequest) -> Result<(), String> {
        if req.operation_type != TreasuryOperationType::Spend {
            return Ok(());
        }
        let expected_nonce = req.expected_nonce.ok_or_else(|| {
            format!(
                "Missing expected_nonce for treasury spend: receipt={}",
                req.decision_receipt_id
            )
        })?;
        let stored_nonce = self.current_treasury_nonce(&req.treasury_id)?;
        if stored_nonce != expected_nonce {
            return Err(format!(
                "Treasury nonce mismatch for {}: expected {}, stored {}",
                req.treasury_id, expected_nonce, stored_nonce
            ));
        }
        Ok(())
    }

    fn advance_spend_nonce(&self, req: &TreasuryEntryRequest) -> Result<(), String> {
        if req.operation_type != TreasuryOperationType::Spend {
            return Ok(());
        }
        let expected_nonce = req.expected_nonce.ok_or_else(|| {
            format!(
                "Missing expected_nonce for treasury spend: receipt={}",
                req.decision_receipt_id
            )
        })?;
        let next_nonce = expected_nonce.checked_add(1).ok_or_else(|| {
            format!(
                "Treasury nonce overflow for {} at {}",
                req.treasury_id, expected_nonce
            )
        })?;
        self.set_treasury_nonce(&req.treasury_id, next_nonce)
    }

    /// Try to claim a decision receipt for append, or return an existing entry hash.
    ///
    /// Returns:
    /// - `Ok(None)` when claim succeeds and caller should append.
    /// - `Ok(Some(entry_hash))` when an existing finalized entry is found.
    fn claim_or_get_existing_entry_hash(
        &self,
        decision_receipt_id: &str,
        decision_hash: &str,
    ) -> Result<Option<String>, String> {
        let Some(index) = self.receipt_index.as_ref() else {
            // Compatibility fallback when no index store is configured.
            return tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    let ledger = self.ledger.write().await;
                    if let Some((entry_hash, existing_decision_hash)) =
                        Self::find_existing_entry_for_receipt(&ledger, decision_receipt_id)?
                    {
                        if let Some(existing_decision_hash) = existing_decision_hash {
                            if existing_decision_hash != decision_hash {
                                return Err(format!(
                                    "Decision hash mismatch for receipt {}: existing={}, requested={}",
                                    decision_receipt_id, existing_decision_hash, decision_hash
                                ));
                            }
                        }
                        Ok(Some(entry_hash))
                    } else {
                        Ok(None)
                    }
                })
            });
        };

        let key = Self::receipt_index_key(decision_receipt_id);
        let db = index.db();

        for _ in 0..RECEIPT_INDEX_WAIT_ATTEMPTS {
            match db
                .compare_and_swap(key.as_slice(), None::<&[u8]>, Some(RECEIPT_INDEX_PENDING))
                .map_err(|e| format!("Failed to claim receipt index: {e}"))?
            {
                Ok(()) => return Ok(None),
                Err(cas_err) => {
                    let Some(current) = cas_err.current else {
                        // Key was concurrently removed; retry claim.
                        continue;
                    };

                    if current.as_ref() == RECEIPT_INDEX_PENDING {
                        // Another executor is finalizing this receipt; wait briefly.
                        std::thread::sleep(Duration::from_millis(RECEIPT_INDEX_WAIT_MS));
                        continue;
                    }

                    let record: ReceiptIndexRecord = serde_json::from_slice(current.as_ref())
                        .map_err(|e| {
                            format!("Failed to decode receipt index record for replay: {e}")
                        })?;
                    if record.decision_hash != decision_hash {
                        return Err(format!(
                            "Decision hash mismatch for receipt {}: existing={}, requested={}",
                            decision_receipt_id, record.decision_hash, decision_hash
                        ));
                    }
                    return Ok(Some(record.entry_hash));
                }
            }
        }

        // Pending marker exceeded wait budget. Attempt bounded recovery by scanning ledger
        // once and finalizing the index if the entry already exists.
        self.resolve_pending_claim_from_ledger(decision_receipt_id, decision_hash)
    }

    fn resolve_pending_claim_from_ledger(
        &self,
        decision_receipt_id: &str,
        decision_hash: &str,
    ) -> Result<Option<String>, String> {
        let maybe_existing = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let ledger = self.ledger.write().await;
                Self::find_existing_entry_for_receipt(&ledger, decision_receipt_id)
            })
        })?;

        let Some((entry_hash, existing_decision_hash)) = maybe_existing else {
            return Err(format!(
                "Receipt {} is currently in-flight (pending index claim), retry",
                decision_receipt_id
            ));
        };

        if let Some(existing_decision_hash) = existing_decision_hash {
            if existing_decision_hash != decision_hash {
                return Err(format!(
                    "Decision hash mismatch for receipt {}: existing={}, requested={}",
                    decision_receipt_id, existing_decision_hash, decision_hash
                ));
            }
        }

        self.finalize_receipt_claim(
            decision_receipt_id,
            &ReceiptIndexRecord {
                entry_hash: entry_hash.clone(),
                decision_hash: decision_hash.to_string(),
            },
        )?;
        Ok(Some(entry_hash))
    }

    fn finalize_receipt_claim(
        &self,
        decision_receipt_id: &str,
        record: &ReceiptIndexRecord,
    ) -> Result<(), String> {
        let Some(index) = self.receipt_index.as_ref() else {
            return Ok(());
        };

        let key = Self::receipt_index_key(decision_receipt_id);
        let encoded = serde_json::to_vec(record)
            .map_err(|e| format!("Failed to encode receipt index record: {e}"))?;

        match index
            .db()
            .compare_and_swap(
                key.as_slice(),
                Some(RECEIPT_INDEX_PENDING),
                Some(encoded.as_slice()),
            )
            .map_err(|e| format!("Failed to finalize receipt index claim: {e}"))?
        {
            Ok(()) => Ok(()),
            Err(cas_err) => {
                if let Some(current) = cas_err.current {
                    if current.as_ref() == encoded.as_slice() {
                        return Ok(());
                    }

                    if current.as_ref() == RECEIPT_INDEX_PENDING {
                        // Fall back to direct set if pending marker still present.
                        index
                            .db()
                            .insert(key.as_slice(), encoded.as_slice())
                            .map_err(|e| {
                                format!("Failed to persist finalized receipt index: {e}")
                            })?;
                        return Ok(());
                    }

                    let existing: ReceiptIndexRecord = serde_json::from_slice(current.as_ref())
                        .map_err(|e| {
                            format!("Failed to decode existing finalized receipt index record: {e}")
                        })?;
                    if existing.entry_hash == record.entry_hash
                        && existing.decision_hash == record.decision_hash
                    {
                        Ok(())
                    } else {
                        Err(format!(
                            "Receipt index conflict for {}: existing hash {}, new hash {}",
                            decision_receipt_id, existing.entry_hash, record.entry_hash
                        ))
                    }
                } else {
                    index
                        .db()
                        .insert(key.as_slice(), encoded.as_slice())
                        .map_err(|e| format!("Failed to restore missing receipt index key: {e}"))?;
                    Ok(())
                }
            }
        }
    }

    fn clear_pending_receipt_claim(&self, decision_receipt_id: &str) {
        let Some(index) = self.receipt_index.as_ref() else {
            return;
        };

        let key = Self::receipt_index_key(decision_receipt_id);
        let _ =
            index
                .db()
                .compare_and_swap(key.as_slice(), Some(RECEIPT_INDEX_PENDING), None::<&[u8]>);
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

    fn get_treasury_nonce(&self, treasury_id: &str) -> Result<u64, String> {
        self.current_treasury_nonce(treasury_id)
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

        // Claim receipt idempotency key (or return existing entry hash on replay).
        if let Some(existing_entry_hash) =
            self.claim_or_get_existing_entry_hash(&req.decision_receipt_id, &req.decision_hash)?
        {
            info!(
                entry_hash = %existing_entry_hash,
                decision_receipt_id = %req.decision_receipt_id,
                decision_hash = %req.decision_hash,
                "Treasury entry already exists for decision receipt (idempotent replay)"
            );
            return Ok(TreasuryEntryResult {
                entry_hash: existing_entry_hash,
                decision_receipt_id: req.decision_receipt_id,
                decision_hash: req.decision_hash,
            });
        }

        // Build account deltas for double-entry bookkeeping
        let deltas = self.build_account_deltas(&req)?;

        // Build the journal entry with provenance
        let mut builder = JournalEntryBuilder::new(self.treasury_did.clone());

        // Add decision provenance (PILOT-CRITICAL INVARIANT)
        builder = builder.with_governance_provenance(&req.decision_receipt_id, &req.decision_hash);

        // Add account deltas
        for delta in deltas {
            builder = builder.add_delta(delta);
        }

        // Build the entry
        let entry = builder
            .build()
            .map_err(|e| format!("Failed to build journal entry: {e}"))?;

        // Append to ledger (async operation in sync context)
        let append_result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let mut ledger = self.ledger.write().await;

                // Surplus reserves policy check (DistributeSurplus only).
                //
                // Enforces the charter's `surplus_reserves_pct` constraint before
                // writing the journal entry. The check reads the current treasury
                // balance (pre-distribution) and verifies that the requested
                // distribution amount does not leave the treasury below the
                // charter-mandated reserves floor.
                //
                // Truth states:
                //   ENFORCED        — charter has reserves_pct; check passed or failed
                //   FALLBACK_APPLIED — charter deployed but no reserves_pct key
                //   UNSUPPORTED     — no surplus policy view configured
                if req.operation_type == TreasuryOperationType::DistributeSurplus {
                    if let Some((charter_id, view)) = ledger.charter_surplus_view_and_id() {
                        match view.reserves_pct_for(charter_id) {
                            Some(reserves_pct) if reserves_pct > 0.0 && reserves_pct < 1.0 => {
                                // ICN mutual-credit accounting: the treasury's balance is
                                // NEGATIVE when it holds surplus obligations owed to members.
                                // DistributeSurplus debits the treasury (makes it less negative).
                                // Negate to get the "distributable pool" (how much surplus exists).
                                let raw_balance =
                                    ledger.get_balance(&self.treasury_did, &req.currency);
                                let distributable_pool = (-raw_balance).max(0i64);
                                let max_distributable =
                                    (distributable_pool as f64 * (1.0 - reserves_pct)).floor()
                                        as i64;
                                if req.amount > max_distributable {
                                    tracing::warn!(
                                        truth_state = "ENFORCED",
                                        charter_id = %charter_id,
                                        reserves_pct = reserves_pct,
                                        distributable_pool = distributable_pool,
                                        max_distributable = max_distributable,
                                        requested = req.amount,
                                        currency = %req.currency,
                                        "Charter surplus reserves policy: distribution exceeds maximum"
                                    );
                                    return Err(format!(
                                        "Charter surplus reserves policy (reserves_pct={reserves_pct:.4}) \
                                         requires at least {} {} in treasury after distribution; \
                                         requested {} {} but max distributable is {}. \
                                         truth_state=ENFORCED",
                                        (distributable_pool as f64 * reserves_pct).ceil() as i64,
                                        req.currency,
                                        req.amount,
                                        req.currency,
                                        max_distributable,
                                    ));
                                }
                                tracing::info!(
                                    truth_state = "ENFORCED",
                                    charter_id = %charter_id,
                                    reserves_pct = reserves_pct,
                                    distributable_pool = distributable_pool,
                                    max_distributable = max_distributable,
                                    distribution_amount = req.amount,
                                    currency = %req.currency,
                                    "Charter surplus reserves policy: distribution within limit"
                                );
                            }
                            Some(_) | None => {
                                // Charter deployed but no reserves_pct constraint — allow freely.
                                tracing::debug!(
                                    truth_state = "FALLBACK_APPLIED",
                                    charter_id = %charter_id,
                                    "No surplus_reserves_pct in charter; distribution proceeds without reserves check"
                                );
                            }
                        }
                    } else {
                        tracing::debug!(
                            truth_state = "UNSUPPORTED",
                            "No surplus policy view configured; distribution proceeds without reserves check"
                        );
                    }
                }

                self.check_spend_nonce(&req)?;
                let entry_hash = ledger
                    .append_entry(entry)
                    .await
                    .map_err(|e| format!("Failed to append ledger entry: {e}"))?;
                self.advance_spend_nonce(&req)?;
                Ok(entry_hash)
            })
        });

        let entry_hash = match append_result {
            Ok(hash) => hash,
            Err(e) => {
                self.clear_pending_receipt_claim(&req.decision_receipt_id);
                return Err(e);
            }
        };
        let entry_hash_hex = entry_hash.to_hex();

        self.finalize_receipt_claim(
            &req.decision_receipt_id,
            &ReceiptIndexRecord {
                entry_hash: entry_hash_hex.clone(),
                decision_hash: req.decision_hash.clone(),
            },
        )?;

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
    use icn_identity::KeyPair;
    use icn_kernel_api::AllowAllOracle;
    use icn_ledger::credit_policy::SurplusPolicyView;
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
            expected_nonce: Some(0),
            decision_receipt_id: "gov:proposal:2024-001:receipt:abc".to_string(),
            decision_hash: "sha256:deadbeef".to_string(),
            distributions: Vec::new(),
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
        let receipt_index = Arc::new(SledStore::temporary().unwrap());
        let ledger = Arc::new(RwLock::new(Ledger::new(ledger_store).unwrap()));
        let service = LedgerServiceImpl::new_with_receipt_index(
            ledger.clone(),
            Arc::new(AllowAllOracle::wildcard()),
            treasury_did.clone(),
            receipt_index,
        );

        let request = TreasuryEntryRequest {
            treasury_id: "t1".to_string(),
            operation_type: TreasuryOperationType::Spend,
            amount: 50,
            currency: "HOURS".to_string(),
            recipient: Some(recipient_did.to_string()),
            memo: "idempotency check".to_string(),
            expected_nonce: Some(0),
            decision_receipt_id: "gov:proposal:pr3:idempotency:001".to_string(),
            decision_hash: "decision-hash-pr3-001".to_string(),
            distributions: Vec::new(),
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
        let receipt_index_path = tmp.path().join("receipt-index");
        std::fs::create_dir_all(&ledger_path).unwrap();
        std::fs::create_dir_all(&receipt_index_path).unwrap();

        let request = TreasuryEntryRequest {
            treasury_id: "t1".to_string(),
            operation_type: TreasuryOperationType::Spend,
            amount: 75,
            currency: "HOURS".to_string(),
            recipient: Some(recipient_did.to_string()),
            memo: "restart idempotency".to_string(),
            expected_nonce: Some(0),
            decision_receipt_id: "gov:proposal:pr3:idempotency:restart".to_string(),
            decision_hash: "decision-hash-pr3-restart".to_string(),
            distributions: Vec::new(),
        };

        let first_hash = {
            let ledger_store = Arc::new(SledStore::open(&ledger_path).unwrap());
            let receipt_index = Arc::new(SledStore::open(&receipt_index_path).unwrap());
            let ledger = Arc::new(RwLock::new(Ledger::new(ledger_store).unwrap()));
            let service = LedgerServiceImpl::new_with_receipt_index(
                ledger.clone(),
                Arc::new(AllowAllOracle::wildcard()),
                treasury_did.clone(),
                receipt_index,
            );

            let first = service.submit_treasury_entry(request.clone()).unwrap();
            assert_eq!(ledger.read().await.count_entries().unwrap(), 1);
            first.entry_hash
        };

        let second_hash = {
            let ledger_store = Arc::new(SledStore::open(&ledger_path).unwrap());
            let receipt_index = Arc::new(SledStore::open(&receipt_index_path).unwrap());
            let ledger = Arc::new(RwLock::new(Ledger::new(ledger_store).unwrap()));
            let service = LedgerServiceImpl::new_with_receipt_index(
                ledger.clone(),
                Arc::new(AllowAllOracle::wildcard()),
                treasury_did,
                receipt_index,
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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_submit_treasury_entry_concurrent_same_receipt_single_append() {
        let treasury_did = KeyPair::generate().unwrap().did().clone();
        let recipient_did = KeyPair::generate().unwrap().did().clone();

        let ledger_store = Arc::new(SledStore::temporary().unwrap());
        let receipt_index = Arc::new(SledStore::temporary().unwrap());
        let ledger = Arc::new(RwLock::new(Ledger::new(ledger_store).unwrap()));
        let service = Arc::new(LedgerServiceImpl::new_with_receipt_index(
            ledger.clone(),
            Arc::new(AllowAllOracle::wildcard()),
            treasury_did,
            receipt_index,
        ));

        let request = TreasuryEntryRequest {
            treasury_id: "t1".to_string(),
            operation_type: TreasuryOperationType::Spend,
            amount: 25,
            currency: "HOURS".to_string(),
            recipient: Some(recipient_did.to_string()),
            memo: "concurrency idempotency".to_string(),
            expected_nonce: Some(0),
            decision_receipt_id: "gov:proposal:pr3:idempotency:concurrent".to_string(),
            decision_hash: "decision-hash-pr3-concurrent".to_string(),
            distributions: Vec::new(),
        };

        let req_a = request.clone();
        let req_b = request;
        let svc_a = service.clone();
        let svc_b = service.clone();

        let task_a = tokio::spawn(async move { svc_a.submit_treasury_entry(req_a) });
        let task_b = tokio::spawn(async move { svc_b.submit_treasury_entry(req_b) });

        let result_a = task_a.await.unwrap().unwrap();
        let result_b = task_b.await.unwrap().unwrap();

        assert_eq!(
            result_a.entry_hash, result_b.entry_hash,
            "concurrent same-receipt submissions must converge to one entry hash"
        );
        assert_eq!(
            ledger.read().await.count_entries().unwrap(),
            1,
            "concurrent same-receipt submissions must append exactly one entry"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_submit_treasury_entry_rejects_stale_expected_nonce() {
        let treasury_did = KeyPair::generate().unwrap().did().clone();
        let recipient_did = KeyPair::generate().unwrap().did().clone();

        let ledger_store = Arc::new(SledStore::temporary().unwrap());
        let receipt_index = Arc::new(SledStore::temporary().unwrap());
        let ledger = Arc::new(RwLock::new(Ledger::new(ledger_store).unwrap()));
        let service = LedgerServiceImpl::new_with_receipt_index(
            ledger.clone(),
            Arc::new(AllowAllOracle::wildcard()),
            treasury_did.clone(),
            receipt_index,
        );

        let first = TreasuryEntryRequest {
            treasury_id: "t1".to_string(),
            operation_type: TreasuryOperationType::Spend,
            amount: 40,
            currency: "HOURS".to_string(),
            recipient: Some(recipient_did.to_string()),
            memo: "nonce baseline".to_string(),
            expected_nonce: Some(0),
            decision_receipt_id: "gov:proposal:pr4:nonce:first".to_string(),
            decision_hash: "decision-hash-pr4-nonce-first".to_string(),
            distributions: Vec::new(),
        };
        service.submit_treasury_entry(first).unwrap();

        let count_after_first = ledger.read().await.count_entries().unwrap();
        let balance_after_first = ledger.read().await.get_balance(&treasury_did, "HOURS");
        assert_eq!(count_after_first, 1);

        // Use a different receipt id so the idempotency short-circuit does not hide nonce checks.
        let stale = TreasuryEntryRequest {
            treasury_id: "t1".to_string(),
            operation_type: TreasuryOperationType::Spend,
            amount: 10,
            currency: "HOURS".to_string(),
            recipient: Some(recipient_did.to_string()),
            memo: "stale nonce replay".to_string(),
            expected_nonce: Some(0),
            decision_receipt_id: "gov:proposal:pr4:nonce:stale".to_string(),
            decision_hash: "decision-hash-pr4-nonce-stale".to_string(),
            distributions: Vec::new(),
        };
        let err = service.submit_treasury_entry(stale).unwrap_err();
        assert!(
            err.contains("Treasury nonce mismatch"),
            "expected nonce mismatch error, got: {err}"
        );

        let count_after_stale = ledger.read().await.count_entries().unwrap();
        let balance_after_stale = ledger.read().await.get_balance(&treasury_did, "HOURS");
        assert_eq!(
            count_after_stale, count_after_first,
            "stale nonce must not append a new ledger entry"
        );
        assert_eq!(
            balance_after_stale, balance_after_first,
            "stale nonce must not mutate balances"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_submit_treasury_entry_nonce_query_tracks_successful_spend() {
        let treasury_did = KeyPair::generate().unwrap().did().clone();
        let recipient_did = KeyPair::generate().unwrap().did().clone();

        let ledger_store = Arc::new(SledStore::temporary().unwrap());
        let receipt_index = Arc::new(SledStore::temporary().unwrap());
        let ledger = Arc::new(RwLock::new(Ledger::new(ledger_store).unwrap()));
        let service = LedgerServiceImpl::new_with_receipt_index(
            ledger,
            Arc::new(AllowAllOracle::wildcard()),
            treasury_did,
            receipt_index,
        );

        let treasury_id = "did:icn:treasury:nonce-test";
        let baseline = service.get_treasury_nonce(treasury_id).unwrap();
        assert_eq!(baseline, 0);

        let request = TreasuryEntryRequest {
            treasury_id: treasury_id.to_string(),
            operation_type: TreasuryOperationType::Spend,
            amount: 30,
            currency: "HOURS".to_string(),
            recipient: Some(recipient_did.to_string()),
            memo: "nonce query coherence".to_string(),
            expected_nonce: Some(baseline),
            decision_receipt_id: "gov:proposal:pr5:nonce:coherence".to_string(),
            decision_hash: "decision-hash-pr5-nonce-coherence".to_string(),
            distributions: Vec::new(),
        };
        service.submit_treasury_entry(request).unwrap();

        let after = service.get_treasury_nonce(treasury_id).unwrap();
        assert_eq!(after, baseline + 1);
    }

    // ── Surplus reserves policy enforcement tests ─────────────────────────────

    /// Minimal mock implementing `SurplusPolicyView` for use in kernel-layer tests.
    ///
    /// Kernel tests must NOT import app crates — the meaning firewall applies to
    /// test code as much as production code. `SurplusPolicyView` is imported once
    /// at the top of the test module so individual tests can reference the short name.
    struct FixedSurplusView {
        reserves_pct: Option<f64>,
    }

    impl SurplusPolicyView for FixedSurplusView {
        fn reserves_pct_for(&self, _charter_id: &str) -> Option<f64> {
            self.reserves_pct
        }
    }

    /// Helper: create a ledger whose treasury account has `balance` units of surplus.
    ///
    /// In ICN mutual-credit accounting, the treasury has a NEGATIVE balance when it
    /// holds obligations owed to members (surplus available to distribute). A `credit`
    /// delta makes the treasury balance negative — `(-treasury_balance)` is the
    /// distributable pool. The balancing `debit` goes to a synthetic genesis account.
    async fn setup_ledger_with_treasury_balance(
        treasury_did: &icn_identity::Did,
        balance: i64,
        currency: &str,
    ) -> Arc<RwLock<Ledger>> {
        let ledger_arc = Arc::new(RwLock::new(
            Ledger::new(Arc::new(SledStore::temporary().unwrap())).unwrap(),
        ));

        let genesis_did = KeyPair::generate().unwrap().did().clone();
        let entry = JournalEntryBuilder::new(treasury_did.clone())
            .add_delta(AccountDelta::debit(
                genesis_did,
                currency.to_string(),
                balance,
            ))
            .add_delta(AccountDelta::credit(
                treasury_did.clone(),
                currency.to_string(),
                balance,
            ))
            .with_governance_provenance("seed-receipt", "seed-hash")
            .build()
            .unwrap();

        ledger_arc.write().await.append_entry(entry).await.unwrap();
        ledger_arc
    }

    /// Charter `surplus_reserves_pct = 0.10` blocks distributions that leave
    /// less than 10% of the distributable pool as reserves.
    ///
    /// truth_state=ENFORCED when the charter constraint is active.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_surplus_reserves_policy_enforced_blocks_excess_distribution() {
        // Treasury distributable pool = 1000 (treasury.balance = -1000); reserves_pct = 0.10
        // max_distributable = 1000 * (1 - 0.10) = 900
        // So distributing 950 must be REJECTED; distributing 900 must be ALLOWED.

        let treasury_did = KeyPair::generate().unwrap().did().clone();
        let member_did = KeyPair::generate().unwrap().did().clone();
        let currency = "HOURS";
        let treasury_balance = 1000_i64;

        let ledger_arc =
            setup_ledger_with_treasury_balance(&treasury_did, treasury_balance, currency).await;

        let surplus_view: Arc<dyn SurplusPolicyView> = Arc::new(FixedSurplusView {
            reserves_pct: Some(0.10),
        });
        ledger_arc
            .write()
            .await
            .set_charter_surplus_view("coop-test".to_string(), surplus_view);

        let service = LedgerServiceImpl::new(
            ledger_arc.clone(),
            Arc::new(AllowAllOracle::wildcard()),
            treasury_did.clone(),
        );

        // --- Test 1: distribution of 950 exceeds max (900) → REJECTED ---
        let req_too_large = TreasuryEntryRequest {
            treasury_id: treasury_did.to_string(),
            operation_type: TreasuryOperationType::DistributeSurplus,
            amount: 950,
            currency: currency.to_string(),
            recipient: None,
            memo: "patronage distribution".to_string(),
            expected_nonce: None,
            decision_receipt_id: "receipt-reject-1".to_string(),
            decision_hash: "hash-reject-1".to_string(),
            distributions: vec![(member_did.to_string(), 950)],
        };
        let result = service.submit_treasury_entry(req_too_large);
        assert!(
            result.is_err(),
            "Distribution exceeding reserves policy must be rejected"
        );
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("truth_state=ENFORCED"),
            "Error must include truth_state=ENFORCED, got: {err_msg}"
        );
        assert!(
            err_msg.contains("reserves_pct"),
            "Error must mention reserves_pct, got: {err_msg}"
        );

        // --- Test 2: distribution of 900 = max → ALLOWED ---
        let req_within_limit = TreasuryEntryRequest {
            treasury_id: treasury_did.to_string(),
            operation_type: TreasuryOperationType::DistributeSurplus,
            amount: 900,
            currency: currency.to_string(),
            recipient: None,
            memo: "patronage distribution".to_string(),
            expected_nonce: None,
            decision_receipt_id: "receipt-allow-1".to_string(),
            decision_hash: "hash-allow-1".to_string(),
            distributions: vec![(member_did.to_string(), 900)],
        };
        let result = service.submit_treasury_entry(req_within_limit);
        assert!(
            result.is_ok(),
            "Distribution within reserves policy must succeed, got: {:?}",
            result.err()
        );
    }

    /// Without a surplus view configured, distributions proceed with
    /// truth_state=UNSUPPORTED (no enforcement).
    #[tokio::test(flavor = "multi_thread")]
    async fn test_surplus_reserves_policy_unsupported_allows_any_distribution() {
        let treasury_did = KeyPair::generate().unwrap().did().clone();
        let member_did = KeyPair::generate().unwrap().did().clone();
        let currency = "HOURS";

        let ledger_arc = setup_ledger_with_treasury_balance(&treasury_did, 1000, currency).await;

        // No surplus view configured.
        let service = LedgerServiceImpl::new(
            ledger_arc.clone(),
            Arc::new(AllowAllOracle::wildcard()),
            treasury_did.clone(),
        );

        // Distribution of 999 (far over any hypothetical reserves floor) must succeed.
        let req = TreasuryEntryRequest {
            treasury_id: treasury_did.to_string(),
            operation_type: TreasuryOperationType::DistributeSurplus,
            amount: 999,
            currency: currency.to_string(),
            recipient: None,
            memo: "distribution — no charter".to_string(),
            expected_nonce: None,
            decision_receipt_id: "receipt-unsupported-1".to_string(),
            decision_hash: "hash-unsupported-1".to_string(),
            distributions: vec![(member_did.to_string(), 999)],
        };
        let result = service.submit_treasury_entry(req);
        assert!(
            result.is_ok(),
            "Without surplus view, any distribution must proceed (truth_state=UNSUPPORTED): {:?}",
            result.err()
        );
    }

    /// Charter deployed but `reserves_pct_for` returns `None` → FALLBACK_APPLIED,
    /// distribution proceeds without constraint.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_surplus_reserves_policy_fallback_applied_when_charter_has_no_surplus() {
        let treasury_did = KeyPair::generate().unwrap().did().clone();
        let member_did = KeyPair::generate().unwrap().did().clone();
        let currency = "HOURS";

        let ledger_arc = setup_ledger_with_treasury_balance(&treasury_did, 1000, currency).await;

        // View returns None — simulates a governance-only charter with no surplus section.
        let surplus_view: Arc<dyn SurplusPolicyView> =
            Arc::new(FixedSurplusView { reserves_pct: None });
        ledger_arc
            .write()
            .await
            .set_charter_surplus_view("coop-gov-only".to_string(), surplus_view);

        let service = LedgerServiceImpl::new(
            ledger_arc.clone(),
            Arc::new(AllowAllOracle::wildcard()),
            treasury_did.clone(),
        );

        // Distribution of 999 — no reserves constraint in charter → must SUCCEED.
        let req = TreasuryEntryRequest {
            treasury_id: treasury_did.to_string(),
            operation_type: TreasuryOperationType::DistributeSurplus,
            amount: 999,
            currency: currency.to_string(),
            recipient: None,
            memo: "distribution — gov-only charter".to_string(),
            expected_nonce: None,
            decision_receipt_id: "receipt-fallback-1".to_string(),
            decision_hash: "hash-fallback-1".to_string(),
            distributions: vec![(member_did.to_string(), 999)],
        };
        let result = service.submit_treasury_entry(req);
        assert!(
            result.is_ok(),
            "Charter with no surplus policy → FALLBACK_APPLIED, must allow: {:?}",
            result.err()
        );
    }
}
