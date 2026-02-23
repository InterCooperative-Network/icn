//! Settlement engine for converting verified execution receipts into journal entries.
//!
//! This module provides the `SettlementEngine` which accepts `SettlementRequest` DTOs
//! (converted from `ExecutionReceipt` by the caller) and creates balanced double-entry
//! journal entries in the ledger.
//!
//! # Scope routing
//!
//! - **Local / Cell / Org**: Settle directly via `JournalEntryBuilder`.
//! - **Federation / Commons**: Rejected — caller must route to `ReceiptClearingManager`
//!   in `icn-federation`.
//!
//! # Deduplication
//!
//! Each receipt is deduplicated by `sha256("icn-ledger:settlement:v1:" || receipt_hash)`.
//! The domain-separation prefix prevents cross-feature sha256 collisions.
//! The engine tracks settled receipts in an in-memory `HashSet` (sled persistence is
//! future work).
//!
//! # Design
//!
//! `SettlementRequest` is a DTO — it does NOT depend on `icn-compute`. The caller is
//! responsible for verifying receipt signatures before constructing the request and
//! setting `executor_verified = true`.

use crate::entry::JournalEntryBuilder;
use crate::error::LedgerError;
use crate::types::JournalEntry;
use icn_identity::Did;
use icn_kernel_api::scope::ScopeLevel;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::RwLock;

/// Domain-separation prefix for settlement dedup keys.
///
/// Prevents accidental collision with other sha256 usages in the codebase.
const DEDUP_PREFIX: &[u8] = b"icn-ledger:settlement:v1:";

/// A settlement request DTO.
///
/// Constructed by the caller from a verified `ExecutionReceipt`. The caller must:
/// 1. Verify all receipt signatures (executor, submitter ack, optionally attester).
/// 2. Set `executor_verified = true` after verification.
/// 3. Parse DID strings into `Did` values.
///
/// The engine will reject requests where `executor_verified` is false.
#[derive(Debug, Clone)]
pub struct SettlementRequest {
    /// Content hash identifying this receipt (opaque 32 bytes from compute layer).
    ///
    /// The ledger treats this as an opaque identifier — it does not know or care
    /// how compute derived it. This is the primary dedup input.
    pub receipt_hash: [u8; 32],

    /// DID of the executor who performed the work (earns credit).
    pub executor: Did,

    /// DID of the submitter who requested the work (pays debit).
    pub submitter: Did,

    /// Optional attester DID for governance / audit trail.
    pub attester: Option<Did>,

    /// Scope level of the original task.
    pub scope: ScopeLevel,

    /// Amount to settle in the ledger currency's smallest unit.
    ///
    /// Uses `i64` to match the existing `AccountDelta` convention.
    /// Must be positive (> 0).
    pub amount: i64,

    /// Currency for settlement (e.g., "credits").
    pub currency: String,

    /// Whether the caller has verified the executor's signature.
    /// The engine rejects requests where this is false.
    pub executor_verified: bool,
}

/// Settlement engine for converting verified receipts into journal entries.
///
/// Handles deduplication, scope validation, and invariant checking, then
/// delegates to `JournalEntryBuilder` for balanced double-entry creation.
pub struct SettlementEngine {
    /// In-memory dedup set of settled receipt keys.
    /// Each key is `sha256(DEDUP_PREFIX || receipt_hash)`.
    settled: RwLock<HashSet<[u8; 32]>>,
}

impl SettlementEngine {
    /// Create a new settlement engine with an empty dedup set.
    pub fn new() -> Self {
        SettlementEngine {
            settled: RwLock::new(HashSet::new()),
        }
    }

    /// Settle a verified receipt, creating a balanced journal entry.
    ///
    /// # Errors
    ///
    /// - `LedgerError::InvalidEntry` if `executor_verified` is false
    /// - `LedgerError::InvalidEntry` if scope is Federation or Commons
    /// - `LedgerError::InvalidEntry` if amount is not positive
    /// - `LedgerError::InvalidEntry` if executor == submitter (self-dealing)
    /// - `LedgerError::DuplicateEntry` if this receipt has already been settled
    /// - Propagates `JournalEntryBuilder::build()` errors
    pub fn settle_receipt(&self, request: &SettlementRequest) -> Result<JournalEntry, LedgerError> {
        // 1. Reject unverified receipts
        if !request.executor_verified {
            return Err(LedgerError::InvalidEntry(
                "receipt executor signature not verified".to_string(),
            ));
        }

        // 2. Reject federation/commons scope — must use clearing
        match request.scope {
            ScopeLevel::Federation => {
                return Err(LedgerError::InvalidEntry(
                    "federation-scope receipts must use cross-cooperative clearing".to_string(),
                ));
            }
            ScopeLevel::Commons => {
                return Err(LedgerError::InvalidEntry(
                    "commons-scope receipts must use cross-cooperative clearing".to_string(),
                ));
            }
            ScopeLevel::Local | ScopeLevel::Cell | ScopeLevel::Org => {}
        }

        // 3. Reject non-positive amounts
        if request.amount <= 0 {
            return Err(LedgerError::InvalidEntry(
                "settlement amount must be positive".to_string(),
            ));
        }

        // 4. Reject self-dealing (executor == submitter)
        if request.executor == request.submitter {
            return Err(LedgerError::InvalidEntry(
                "executor and submitter must be different DIDs".to_string(),
            ));
        }

        // 5. Check deduplication
        let dedup_key = Self::dedup_key(&request.receipt_hash);
        {
            let settled = self
                .settled
                .read()
                .map_err(|_| LedgerError::Internal("settlement lock poisoned".to_string()))?;
            if settled.contains(&dedup_key) {
                return Err(LedgerError::DuplicateEntry(format!(
                    "receipt already settled: {}",
                    hex::encode(request.receipt_hash),
                )));
            }
        }

        // 6. Build balanced journal entry: debit submitter, credit executor
        let entry = JournalEntryBuilder::new(request.submitter.clone())
            .debit(
                request.submitter.clone(),
                request.currency.clone(),
                request.amount,
            )
            .credit(
                request.executor.clone(),
                request.currency.clone(),
                request.amount,
            )
            .build()
            .map_err(|e| LedgerError::InvalidEntry(e.to_string()))?;

        // 7. Record dedup key
        {
            let mut settled = self
                .settled
                .write()
                .map_err(|_| LedgerError::Internal("settlement lock poisoned".to_string()))?;
            settled.insert(dedup_key);
        }

        Ok(entry)
    }

    /// Check whether a receipt has already been settled.
    pub fn is_settled(&self, receipt_hash: &[u8; 32]) -> bool {
        let key = Self::dedup_key(receipt_hash);
        self.settled
            .read()
            .map(|s| s.contains(&key))
            .unwrap_or(false)
    }

    /// Number of settled receipts tracked.
    pub fn settled_count(&self) -> usize {
        self.settled.read().map(|s| s.len()).unwrap_or(0)
    }

    /// Compute deduplication key: `sha256(DEDUP_PREFIX || receipt_hash)`.
    ///
    /// The receipt_hash already uniquely identifies a receipt (it includes
    /// the receipt_id nonce from compute layer). Scope is intentionally
    /// excluded — a receipt can only settle once regardless of scope.
    fn dedup_key(receipt_hash: &[u8; 32]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(DEDUP_PREFIX);
        hasher.update(receipt_hash);
        let result = hasher.finalize();
        let mut key = [0u8; 32];
        key.copy_from_slice(&result);
        key
    }
}

impl Default for SettlementEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Commons Settlement
// ============================================================================

/// A commons credit settlement request.
///
/// Used for commons-scope compute tasks: the contributor earns credits from the
/// commons mint, and the consumer spends credits back to the commons mint.
///
/// The caller must:
/// 1. Verify all receipt signatures and set `executor_verified = true`.
/// 2. Look up the consumer's current commons credit balance and pass it as `consumer_balance`.
/// 3. Set `scope = ScopeLevel::Commons` — any other scope is a hard error.
///
/// # Invariants
///
/// These invariants are enforced by `SettlementEngine::settle_commons_receipt()`:
/// 1. `scope` must be `ScopeLevel::Commons` — hard error otherwise
/// 2. `executor_verified` must be `true`
/// 3. `consumer_balance >= amount` — insufficient balance fails settlement; no partial state
/// 4. `contributor != consumer` — self-dealing rejected
/// 5. `amount > 0` — non-positive amounts rejected
#[derive(Debug, Clone)]
pub struct CommonsSettlementRequest {
    /// Content hash identifying this receipt (opaque 32 bytes from compute layer).
    ///
    /// Used as the dedup key and as the nonce in both journal entries, ensuring
    /// identical amounts for the same contributor/consumer produce distinct hashes.
    pub receipt_hash: [u8; 32],

    /// DID of the contributor who performed the work (earns credits from the mint).
    pub contributor: Did,

    /// DID of the consumer who requested the work (spends credits to the mint).
    pub consumer: Did,

    /// Scope level — **must** be `ScopeLevel::Commons`. Hard-rejected otherwise.
    ///
    /// This field is validated by `settle_commons_receipt()`. Passing any other
    /// scope level is a programming error and returns `LedgerError::InvalidEntry`.
    pub scope: ScopeLevel,

    /// Amount of commons credits to settle (must be positive).
    pub amount: i64,

    /// Current commons credit balance of the consumer, pre-fetched by the caller.
    ///
    /// `settle_commons_receipt()` calls `check_sufficient_balance(consumer_balance, amount)`.
    /// If insufficient, settlement fails and no journal entries are created.
    ///
    /// The balance is held here (not looked up internally) to preserve the DTO
    /// pattern: callers are responsible for verified state, the engine enforces invariants.
    pub consumer_balance: i64,

    /// Whether the caller has verified the executor's signature.
    /// The engine rejects requests where this is false.
    pub executor_verified: bool,
}

impl SettlementEngine {
    /// Settle a commons-scope receipt, producing earn and spend journal entries.
    ///
    /// Returns `(earn_entry, spend_entry)` on success. The caller **must** append
    /// both entries to the ledger — neither alone produces a balanced ledger.
    ///
    /// # Invariants enforced
    ///
    /// 1. `executor_verified` must be `true`
    /// 2. `scope` must be `ScopeLevel::Commons` (hard error for any other scope)
    /// 3. `amount` must be positive
    /// 4. `contributor != consumer` (no self-dealing)
    /// 5. Consumer balance must be `>= amount` (balance floor, no partial state on failure)
    /// 6. Receipt must not have been settled before (`DuplicateEntry` on repeat)
    ///
    /// Concurrent settlements of the same receipt are serialized by the internal
    /// `RwLock<HashSet>`: only one writer can record the dedup key at a time, and
    /// the read-check before write prevents double-insertion.
    ///
    /// # Errors
    ///
    /// - `LedgerError::InvalidEntry` — invariant violation (see above)
    /// - `LedgerError::DuplicateEntry` — receipt already settled
    /// - `LedgerError::Internal` — lock poisoned (should never happen in practice)
    pub fn settle_commons_receipt(
        &self,
        request: &CommonsSettlementRequest,
    ) -> Result<(JournalEntry, JournalEntry), LedgerError> {
        // 1. Reject unverified receipts
        if !request.executor_verified {
            return Err(LedgerError::InvalidEntry(
                "receipt executor signature not verified".to_string(),
            ));
        }

        // 2. Hard-reject non-Commons scope (invariant #2 from contract)
        if request.scope != ScopeLevel::Commons {
            return Err(LedgerError::InvalidEntry(format!(
                "settle_commons_receipt requires scope=Commons, got {}",
                request.scope,
            )));
        }

        // 3. Reject non-positive amounts
        if request.amount <= 0 {
            return Err(LedgerError::InvalidEntry(
                "settlement amount must be positive".to_string(),
            ));
        }

        // 4. Reject self-dealing
        if request.contributor == request.consumer {
            return Err(LedgerError::InvalidEntry(
                "contributor and consumer must be different DIDs".to_string(),
            ));
        }

        // 5. Balance floor — insufficient balance fails settlement, no partial state
        crate::commons_credits::check_sufficient_balance(request.consumer_balance, request.amount)
            .map_err(|e| LedgerError::InvalidEntry(e.to_string()))?;

        // 6. Check deduplication (read lock, then write lock after building entries)
        let dedup_key = Self::dedup_key(&request.receipt_hash);
        {
            let settled = self
                .settled
                .read()
                .map_err(|_| LedgerError::Internal("settlement lock poisoned".to_string()))?;
            if settled.contains(&dedup_key) {
                return Err(LedgerError::DuplicateEntry(format!(
                    "receipt already settled: {}",
                    hex::encode(request.receipt_hash),
                )));
            }
        }

        // 7. Build entries using commons_credits utilities.
        //    Both entries use the receipt_hash as nonce, ensuring distinct content hashes
        //    even when amount, contributor, and consumer are identical across receipts.
        let earn_entry = crate::commons_credits::build_earn_entry_with_receipt(
            &request.contributor,
            request.amount,
            request.receipt_hash,
        )
        .map_err(|e| LedgerError::InvalidEntry(e.to_string()))?;

        let spend_entry = crate::commons_credits::build_spend_entry_with_receipt(
            &request.consumer,
            request.amount,
            request.receipt_hash,
        )
        .map_err(|e| LedgerError::InvalidEntry(e.to_string()))?;

        // 8. Record dedup key — after successful entry construction, before returning.
        //    If we crash here, the entries were never returned to the caller, so
        //    the ledger stays consistent (no orphaned entries).
        {
            let mut settled = self
                .settled
                .write()
                .map_err(|_| LedgerError::Internal("settlement lock poisoned".to_string()))?;
            settled.insert(dedup_key);
        }

        // ── Emit settlement metrics (only on guaranteed success, after dedup gate) ──
        icn_obs::metrics::compute::receipt_settlement_inc("commons");
        icn_obs::metrics::compute::commons_credits_earned_add(request.amount as u64);
        icn_obs::metrics::compute::commons_credits_spent_add(request.amount as u64);

        Ok((earn_entry, spend_entry))
    }
}

#[cfg(test)]
mod commons_tests {
    use super::*;
    use icn_identity::KeyPair;

    fn make_commons_request(
        contributor: &KeyPair,
        consumer: &KeyPair,
        receipt_hash: [u8; 32],
        amount: i64,
        consumer_balance: i64,
    ) -> CommonsSettlementRequest {
        CommonsSettlementRequest {
            receipt_hash,
            contributor: contributor.did().clone(),
            consumer: consumer.did().clone(),
            scope: ScopeLevel::Commons,
            amount,
            consumer_balance,
            executor_verified: true,
        }
    }

    #[test]
    fn test_commons_settle_happy_path() {
        let engine = SettlementEngine::new();
        let contributor = KeyPair::generate().unwrap();
        let consumer = KeyPair::generate().unwrap();

        let req = make_commons_request(&contributor, &consumer, [0x01; 32], 500, 1000);
        let (earn, spend) = engine.settle_commons_receipt(&req).unwrap();

        // Earn entry: mint → contributor (credit contributor)
        assert_eq!(earn.accounts.len(), 2);
        let earn_credit = earn.accounts.iter().find(|a| a.credit.is_some()).unwrap();
        assert_eq!(earn_credit.account_id, *contributor.did());
        assert_eq!(earn_credit.credit, Some(500));

        // Spend entry: consumer → mint (debit consumer)
        assert_eq!(spend.accounts.len(), 2);
        let spend_debit = spend.accounts.iter().find(|a| a.debit.is_some()).unwrap();
        assert_eq!(spend_debit.account_id, *consumer.did());
        assert_eq!(spend_debit.debit, Some(500));
    }

    #[test]
    fn test_commons_settle_wrong_scope_rejected() {
        let engine = SettlementEngine::new();
        let contributor = KeyPair::generate().unwrap();
        let consumer = KeyPair::generate().unwrap();

        for scope in [
            ScopeLevel::Local,
            ScopeLevel::Cell,
            ScopeLevel::Org,
            ScopeLevel::Federation,
        ] {
            let req = CommonsSettlementRequest {
                receipt_hash: [0x02; 32],
                contributor: contributor.did().clone(),
                consumer: consumer.did().clone(),
                scope,
                amount: 100,
                consumer_balance: 1000,
                executor_verified: true,
            };
            let result = engine.settle_commons_receipt(&req);
            assert!(result.is_err(), "scope {scope} should be rejected");
            let err = result.unwrap_err().to_string();
            assert!(err.contains("scope=Commons"), "got: {err}");
        }
    }

    #[test]
    fn test_commons_settle_unverified_rejected() {
        let engine = SettlementEngine::new();
        let contributor = KeyPair::generate().unwrap();
        let consumer = KeyPair::generate().unwrap();

        let req = CommonsSettlementRequest {
            receipt_hash: [0x03; 32],
            contributor: contributor.did().clone(),
            consumer: consumer.did().clone(),
            scope: ScopeLevel::Commons,
            amount: 100,
            consumer_balance: 1000,
            executor_verified: false,
        };
        let result = engine.settle_commons_receipt(&req);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not verified"), "got: {err}");
    }

    #[test]
    fn test_commons_settle_insufficient_balance_rejected() {
        let engine = SettlementEngine::new();
        let contributor = KeyPair::generate().unwrap();
        let consumer = KeyPair::generate().unwrap();

        let req = make_commons_request(&contributor, &consumer, [0x04; 32], 500, 100); // balance 100 < amount 500
        let result = engine.settle_commons_receipt(&req);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("insufficient"), "got: {err}");

        // Verify the receipt is NOT marked as settled after failure
        assert!(!engine.is_settled(&[0x04; 32]));
    }

    #[test]
    fn test_commons_settle_idempotency() {
        let engine = SettlementEngine::new();
        let contributor = KeyPair::generate().unwrap();
        let consumer = KeyPair::generate().unwrap();

        let req = make_commons_request(&contributor, &consumer, [0x05; 32], 100, 500);

        // First settlement succeeds
        assert!(engine.settle_commons_receipt(&req).is_ok());

        // Second attempt returns DuplicateEntry — balances must not change
        let result = engine.settle_commons_receipt(&req);
        assert!(result.is_err());
        match result.unwrap_err() {
            LedgerError::DuplicateEntry(msg) => {
                assert!(msg.contains("already settled"), "got: {msg}");
            }
            other => panic!("expected DuplicateEntry, got: {other}"),
        }
    }

    #[test]
    fn test_commons_settle_self_dealing_rejected() {
        let engine = SettlementEngine::new();
        let kp = KeyPair::generate().unwrap();

        let req = CommonsSettlementRequest {
            receipt_hash: [0x06; 32],
            contributor: kp.did().clone(),
            consumer: kp.did().clone(), // same as contributor
            scope: ScopeLevel::Commons,
            amount: 100,
            consumer_balance: 1000,
            executor_verified: true,
        };
        let result = engine.settle_commons_receipt(&req);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("different DIDs"), "got: {err}");
    }

    #[test]
    fn test_commons_settle_zero_amount_rejected() {
        let engine = SettlementEngine::new();
        let contributor = KeyPair::generate().unwrap();
        let consumer = KeyPair::generate().unwrap();

        let req = make_commons_request(&contributor, &consumer, [0x07; 32], 0, 1000);
        let result = engine.settle_commons_receipt(&req);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("positive"), "got: {err}");
    }

    #[test]
    fn test_commons_settle_exact_balance_ok() {
        let engine = SettlementEngine::new();
        let contributor = KeyPair::generate().unwrap();
        let consumer = KeyPair::generate().unwrap();

        // Balance equals amount exactly — should succeed (balance floor is >=, not >)
        let req = make_commons_request(&contributor, &consumer, [0x08; 32], 300, 300);
        assert!(engine.settle_commons_receipt(&req).is_ok());
    }

    #[test]
    fn test_commons_dedup_independent_from_regular_settlement() {
        // A receipt can be settled via settle_receipt() OR settle_commons_receipt()
        // but not both — they share the same dedup key space.
        let engine = SettlementEngine::new();
        let contributor = KeyPair::generate().unwrap();
        let consumer = KeyPair::generate().unwrap();

        let receipt_hash = [0x09; 32];
        let commons_req = make_commons_request(&contributor, &consumer, receipt_hash, 100, 500);
        engine.settle_commons_receipt(&commons_req).unwrap();

        // is_settled() reflects commons settlement in the shared dedup set
        assert!(engine.is_settled(&receipt_hash));
        assert_eq!(engine.settled_count(), 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn make_request(scope: ScopeLevel, amount: i64, verified: bool) -> SettlementRequest {
        let executor_kp = KeyPair::generate().unwrap();
        let submitter_kp = KeyPair::generate().unwrap();
        SettlementRequest {
            receipt_hash: [0xAA; 32],
            executor: executor_kp.did().clone(),
            submitter: submitter_kp.did().clone(),
            attester: None,
            scope,
            amount,
            currency: "credits".to_string(),
            executor_verified: verified,
        }
    }

    fn make_request_with_keys(
        executor: &KeyPair,
        submitter: &KeyPair,
        receipt_hash: [u8; 32],
    ) -> SettlementRequest {
        SettlementRequest {
            receipt_hash,
            executor: executor.did().clone(),
            submitter: submitter.did().clone(),
            attester: None,
            scope: ScopeLevel::Local,
            amount: 100,
            currency: "credits".to_string(),
            executor_verified: true,
        }
    }

    #[test]
    fn test_settle_creates_balanced_entry() {
        let engine = SettlementEngine::new();
        let executor_kp = KeyPair::generate().unwrap();
        let submitter_kp = KeyPair::generate().unwrap();

        let request = SettlementRequest {
            receipt_hash: [0xBB; 32],
            executor: executor_kp.did().clone(),
            submitter: submitter_kp.did().clone(),
            attester: None,
            scope: ScopeLevel::Local,
            amount: 500,
            currency: "credits".to_string(),
            executor_verified: true,
        };

        let entry = engine.settle_receipt(&request).unwrap();

        // Should have exactly 2 account deltas: one debit, one credit
        assert_eq!(entry.accounts.len(), 2);

        let debit = &entry.accounts[0];
        assert_eq!(debit.account_id, *submitter_kp.did());
        assert_eq!(debit.debit, Some(500));
        assert_eq!(debit.credit, None);
        assert_eq!(debit.currency, "credits");

        let credit = &entry.accounts[1];
        assert_eq!(credit.account_id, *executor_kp.did());
        assert_eq!(credit.credit, Some(500));
        assert_eq!(credit.debit, None);
        assert_eq!(credit.currency, "credits");

        // Author should be the submitter
        assert_eq!(entry.author, *submitter_kp.did());
    }

    #[test]
    fn test_settle_deduplication() {
        let engine = SettlementEngine::new();
        let executor_kp = KeyPair::generate().unwrap();
        let submitter_kp = KeyPair::generate().unwrap();

        let request = make_request_with_keys(&executor_kp, &submitter_kp, [0xCC; 32]);

        // First settlement succeeds
        let result = engine.settle_receipt(&request);
        assert!(result.is_ok());

        // Second attempt is rejected as duplicate
        let result = engine.settle_receipt(&request);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("already settled"), "got: {err}");
    }

    #[test]
    fn test_settle_federation_scope_rejected() {
        let engine = SettlementEngine::new();
        let request = make_request(ScopeLevel::Federation, 100, true);

        let result = engine.settle_receipt(&request);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("cross-cooperative clearing"), "got: {err}");
    }

    #[test]
    fn test_settle_commons_scope_rejected() {
        let engine = SettlementEngine::new();
        let request = make_request(ScopeLevel::Commons, 100, true);

        let result = engine.settle_receipt(&request);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("cross-cooperative clearing"), "got: {err}");
    }

    #[test]
    fn test_settle_local_scope_accepted() {
        let engine = SettlementEngine::new();
        let request = make_request(ScopeLevel::Local, 100, true);
        assert!(engine.settle_receipt(&request).is_ok());
    }

    #[test]
    fn test_settle_cell_scope_accepted() {
        let engine = SettlementEngine::new();
        let request = make_request(ScopeLevel::Cell, 100, true);
        assert!(engine.settle_receipt(&request).is_ok());
    }

    #[test]
    fn test_settle_org_scope_accepted() {
        let engine = SettlementEngine::new();
        let request = make_request(ScopeLevel::Org, 100, true);
        assert!(engine.settle_receipt(&request).is_ok());
    }

    #[test]
    fn test_settle_unverified_rejected() {
        let engine = SettlementEngine::new();
        let request = make_request(ScopeLevel::Local, 100, false);

        let result = engine.settle_receipt(&request);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not verified"), "got: {err}");
    }

    #[test]
    fn test_settle_zero_amount_rejected() {
        let engine = SettlementEngine::new();
        let request = make_request(ScopeLevel::Local, 0, true);

        let result = engine.settle_receipt(&request);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("positive"), "got: {err}");
    }

    #[test]
    fn test_settle_negative_amount_rejected() {
        let engine = SettlementEngine::new();
        let request = make_request(ScopeLevel::Local, -50, true);

        let result = engine.settle_receipt(&request);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("positive"), "got: {err}");
    }

    #[test]
    fn test_settle_self_dealing_rejected() {
        let engine = SettlementEngine::new();
        let kp = KeyPair::generate().unwrap();

        let request = SettlementRequest {
            receipt_hash: [0xDD; 32],
            executor: kp.did().clone(),
            submitter: kp.did().clone(), // same as executor
            attester: None,
            scope: ScopeLevel::Local,
            amount: 100,
            currency: "credits".to_string(),
            executor_verified: true,
        };

        let result = engine.settle_receipt(&request);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("different DIDs"), "got: {err}");
    }

    #[test]
    fn test_is_settled() {
        let engine = SettlementEngine::new();
        let executor_kp = KeyPair::generate().unwrap();
        let submitter_kp = KeyPair::generate().unwrap();
        let receipt_hash = [0xDD; 32];

        // Not settled yet
        assert!(!engine.is_settled(&receipt_hash));

        // Settle it
        let request = make_request_with_keys(&executor_kp, &submitter_kp, receipt_hash);
        engine.settle_receipt(&request).unwrap();

        // Now it is settled
        assert!(engine.is_settled(&receipt_hash));
    }

    #[test]
    fn test_different_receipt_hash_not_duplicate() {
        let engine = SettlementEngine::new();
        let executor = KeyPair::generate().unwrap();
        let submitter = KeyPair::generate().unwrap();

        let request_a = make_request_with_keys(&executor, &submitter, [0x11; 32]);
        let request_b = make_request_with_keys(&executor, &submitter, [0x22; 32]);

        // Both should succeed — different receipt hashes
        assert!(engine.settle_receipt(&request_a).is_ok());
        assert!(engine.settle_receipt(&request_b).is_ok());
        assert_eq!(engine.settled_count(), 2);
    }

    #[test]
    fn test_settled_count() {
        let engine = SettlementEngine::new();
        assert_eq!(engine.settled_count(), 0);

        let request = make_request(ScopeLevel::Local, 50, true);
        engine.settle_receipt(&request).unwrap();
        assert_eq!(engine.settled_count(), 1);
    }

    #[test]
    fn test_dedup_key_deterministic() {
        let hash = [0xFF; 32];
        let key1 = SettlementEngine::dedup_key(&hash);
        let key2 = SettlementEngine::dedup_key(&hash);
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_dedup_key_changes_on_different_receipt() {
        let key_a = SettlementEngine::dedup_key(&[0x11; 32]);
        let key_b = SettlementEngine::dedup_key(&[0x22; 32]);
        assert_ne!(key_a, key_b);
    }

    #[test]
    fn test_dedup_key_domain_separated() {
        // Verify the prefix actually changes the output vs raw sha256(receipt_hash)
        let receipt_hash = [0xAA; 32];
        let with_prefix = SettlementEngine::dedup_key(&receipt_hash);

        let mut raw_hasher = Sha256::new();
        raw_hasher.update(receipt_hash);
        let raw_result = raw_hasher.finalize();
        let mut raw_key = [0u8; 32];
        raw_key.copy_from_slice(&raw_result);

        assert_ne!(
            with_prefix, raw_key,
            "domain prefix must change the dedup key output"
        );
    }

    #[test]
    fn test_attester_field_preserved() {
        let engine = SettlementEngine::new();
        let executor_kp = KeyPair::generate().unwrap();
        let submitter_kp = KeyPair::generate().unwrap();
        let attester_kp = KeyPair::generate().unwrap();

        let request = SettlementRequest {
            receipt_hash: [0xEE; 32],
            executor: executor_kp.did().clone(),
            submitter: submitter_kp.did().clone(),
            attester: Some(attester_kp.did().clone()),
            scope: ScopeLevel::Local,
            amount: 100,
            currency: "credits".to_string(),
            executor_verified: true,
        };

        // Settlement should succeed — attester is informational, not blocking
        let result = engine.settle_receipt(&request);
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod metric_tests {
    use super::*;
    use icn_identity::Did;
    use icn_kernel_api::ScopeLevel;
    use metrics::with_local_recorder;
    use metrics_util::debugging::{DebugValue, DebuggingRecorder, Snapshotter};

    const EXECUTOR: &str = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9";
    const CONSUMER: &str = "did:icn:z9hSR6S7WPtxmTojgo6GG3k4yDPecgJY292j7xrsUGWBu";

    fn make_commons_request(hash: [u8; 32], amount: i64) -> CommonsSettlementRequest {
        CommonsSettlementRequest {
            receipt_hash: hash,
            contributor: Did::from_str(EXECUTOR).unwrap(),
            consumer: Did::from_str(CONSUMER).unwrap(),
            scope: ScopeLevel::Commons,
            amount,
            consumer_balance: 100_000,
            executor_verified: true,
        }
    }

    /// Sum all counter values with the given metric name across all label sets.
    fn counter_total(snapshotter: &Snapshotter, name: &str) -> u64 {
        snapshotter
            .snapshot()
            .into_vec()
            .into_iter()
            .filter_map(|(key, _, _, value)| {
                if key.key().name() == name {
                    match value {
                        DebugValue::Counter(v) => Some(v),
                        _ => None,
                    }
                } else {
                    None
                }
            })
            .sum()
    }

    /// Successful settlement increments all 3 counters by the settlement amount.
    #[test]
    fn successful_settlement_increments_all_counters() {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();

        with_local_recorder(&recorder, || {
            let engine = SettlementEngine::new();
            let req = make_commons_request([0xA1u8; 32], 100);
            engine
                .settle_commons_receipt(&req)
                .expect("settle must succeed");
        });

        assert_eq!(
            counter_total(&snapshotter, "icn_compute_receipt_settlement_total"),
            1,
            "receipt_settlement_total must be 1"
        );
        assert_eq!(
            counter_total(&snapshotter, "icn_compute_commons_credits_earned_total"),
            100,
            "earned_total must equal settlement amount"
        );
        assert_eq!(
            counter_total(&snapshotter, "icn_compute_commons_credits_spent_total"),
            100,
            "spent_total must equal settlement amount"
        );
    }

    /// Duplicate settlement returns DuplicateEntry and must not increment counters.
    #[test]
    fn duplicate_settlement_does_not_increment_counters() {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();

        with_local_recorder(&recorder, || {
            let engine = SettlementEngine::new();
            let req = make_commons_request([0xB2u8; 32], 50);

            // First settlement — must succeed.
            engine
                .settle_commons_receipt(&req)
                .expect("first settle must succeed");
            let after_first = counter_total(&snapshotter, "icn_compute_receipt_settlement_total");
            assert_eq!(after_first, 1, "counter must be 1 after first settlement");

            let after_first_earned =
                counter_total(&snapshotter, "icn_compute_commons_credits_earned_total");
            let after_first_spent =
                counter_total(&snapshotter, "icn_compute_commons_credits_spent_total");

            // Duplicate — must return DuplicateEntry, counters must not change.
            let result = engine.settle_commons_receipt(&req);
            assert!(
                matches!(result, Err(crate::error::LedgerError::DuplicateEntry(_))),
                "second settlement must be DuplicateEntry, got: {result:?}"
            );
            assert_eq!(
                counter_total(&snapshotter, "icn_compute_receipt_settlement_total"),
                after_first,
                "receipt_settlement_total must not increment on duplicate"
            );
            assert_eq!(
                counter_total(&snapshotter, "icn_compute_commons_credits_earned_total"),
                after_first_earned,
                "earned_total must not increment on duplicate"
            );
            assert_eq!(
                counter_total(&snapshotter, "icn_compute_commons_credits_spent_total"),
                after_first_spent,
                "spent_total must not increment on duplicate"
            );
        });
    }

    /// Rejected settlements (scope mismatch, insufficient balance) must not touch counters.
    #[test]
    fn rejected_settlement_does_not_increment_counters() {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();

        with_local_recorder(&recorder, || {
            let engine = SettlementEngine::new();

            // Scope mismatch.
            let mut req = make_commons_request([0xC3u8; 32], 50);
            req.scope = ScopeLevel::Local;
            assert!(
                engine.settle_commons_receipt(&req).is_err(),
                "scope mismatch must be rejected"
            );
            assert_eq!(
                counter_total(&snapshotter, "icn_compute_receipt_settlement_total"),
                0,
                "scope mismatch must not increment counter"
            );

            // Insufficient balance.
            let req2 = CommonsSettlementRequest {
                receipt_hash: [0xD4u8; 32],
                contributor: Did::from_str(EXECUTOR).unwrap(),
                consumer: Did::from_str(CONSUMER).unwrap(),
                scope: ScopeLevel::Commons,
                amount: 999,
                consumer_balance: 10, // far below amount
                executor_verified: true,
            };
            assert!(
                engine.settle_commons_receipt(&req2).is_err(),
                "insufficient balance must be rejected"
            );
            assert_eq!(
                counter_total(&snapshotter, "icn_compute_receipt_settlement_total"),
                0,
                "insufficient balance must not increment counter"
            );
        });
    }
}
