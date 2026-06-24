//! Ledger-backed compute callbacks — daemon composition root wiring.
//!
//! These factory functions bridge the compute actor (kernel) with the ledger (domain app).
//! They live here in the daemon binary so neither `icn-core` (kernel) nor `icn-compute`
//! (kernel) needs to import `icn-ledger` (domain).
//!
//! The resulting callbacks are type-erased (`Arc<dyn Fn(...)>`) before being injected
//! into the kernel via `BootstrapHandles`. The meaning firewall stays intact.

use icn_identity::Did;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Concrete ledger handle type (RwLock-wrapped Ledger).
pub type LedgerHandle = Arc<RwLock<icn_ledger::Ledger>>;

/// Build the sled-backed commons settlement engine.
///
/// Uses `SettlementEngine::with_store()` for restart-safe receipt deduplication.
/// Falls back to in-memory dedup on store open failure (warns; does not abort daemon startup).
pub fn build_settlement_engine(
    dedup_path: std::path::PathBuf,
) -> Arc<icn_ledger::SettlementEngine> {
    let store = match icn_store::SledStore::open(&dedup_path) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            warn!(
                "Failed to open settlement dedup store at {}: {e}; \
                 using in-memory dedup (receipts will not survive restart)",
                dedup_path.display()
            );
            return Arc::new(icn_ledger::SettlementEngine::new());
        }
    };
    match icn_ledger::SettlementEngine::with_store(store) {
        Ok(e) => Arc::new(e),
        Err(e) => {
            warn!(
                "Failed to load commons settlement dedup store ({e}); \
                 falling back to in-memory-only dedup"
            );
            Arc::new(icn_ledger::SettlementEngine::new())
        }
    }
}

/// Create the balance callback for commons credit gate checks.
///
/// Returns the caller's commons credit balance (`"commons-capacity"` currency).
/// Used for the advisory credit floor check and the `CommonsPoolPolicy` credit
/// ceiling check inside `ComputeActor::handle_submit`.
///
/// Uses `try_read()` (non-blocking). On lock contention the callback returns
/// `i64::MAX` so the advisory check passes — the authoritative gate is the
/// `CommonsReserveCallback`, which must be wired separately for race-free
/// credit enforcement.
pub fn create_balance_callback(ledger: LedgerHandle) -> icn_compute::BalanceCallback {
    Arc::new(move |did_str: &str| {
        let did: Did = match did_str.parse() {
            Ok(d) => d,
            Err(_) => {
                warn!(
                    "balance_callback: unparseable DID '{}', returning 0",
                    did_str
                );
                return 0;
            }
        };
        match ledger.try_read() {
            Ok(guard) => {
                // The ICN ledger uses net_change = debit - credit, so `credit` entries
                // (which record earnings) produce a NEGATIVE raw balance. Negate to convert
                // "earned credit balance" → "spendable capacity" (positive = can spend).
                // Example: earn 1000 → raw balance = -1000 → capacity = +1000.
                -guard.get_balance(&did, icn_ledger::commons_credits::COMMONS_CREDIT_CURRENCY)
            }
            Err(_) => {
                // Lock contended — fail-open for this advisory check only.
                // This callback is NOT the authoritative gate; it is used for an advisory
                // credit floor check only. The authoritative enforcement gate is the
                // CommonsReserveCallback, which must handle races independently.
                // Returning i64::MAX passes the advisory check rather than blocking on
                // a contended write lock. This is intentional fail-open for advisory use.
                tracing::debug!(
                    did = did_str,
                    "balance_callback: ledger read-contended, skipping advisory check (fail-open)"
                );
                i64::MAX
            }
        }
    })
}

/// Derive a stable 32-byte dedup key for a bilateral compute payment.
///
/// The compute `PaymentRequest` carries no receipt hash, but a task settles exactly once,
/// so the task id is the natural idempotency key: re-delivery of the same task's payment
/// (e.g. gossip redelivery) must not double-settle. Domain-separated so it cannot collide
/// with hashes minted elsewhere.
fn payment_dedup_hash(task_id: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"icn:compute-payment-dedup:v1:");
    hasher.update(task_id.as_bytes());
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

/// Create the payment callback for settling bilateral (non-commons) compute payments.
///
/// Routes through [`icn_ledger::SettlementEngine::settle_receipt`] so the payment gets the
/// same guarantees as every other settlement — scope validation (Local/Cell/Org only;
/// Federation/Commons rejected), positive-amount and no-self-dealing invariants, and
/// receipt deduplication — instead of building a `JournalEntry` and appending it directly.
/// The engine returns a balanced entry (debit submitter / credit executor) which the caller
/// appends, mirroring the commons settlement path.
///
/// `engine` must be pre-created by the caller (the same engine used for commons settlement;
/// see `build_settlement_engine()`).
///
/// # Security (#1342)
/// `req.from` originates from `claimed_task.submitter`, set by the task submitter at
/// submission time. Over the gateway REST API the submitter DID is extracted from the JWT
/// (authenticated); when a TaskSubmitted message arrives via gossip the submitter field comes
/// from the gossip payload and could be spoofed to name a victim DID as payer. Full fix
/// requires the gossip layer to verify `entry.author == task.submitter` (tracked in #1342);
/// this change does not alter that exposure — the payer DID remains recorded as the debit
/// account/author of the settlement entry. `executor_verified` is asserted here to preserve
/// the prior unconditional-settle behavior pending the #1342 gossip-author verification.
pub fn create_payment_callback(
    ledger: LedgerHandle,
    engine: Arc<icn_ledger::SettlementEngine>,
) -> icn_compute::PaymentCallback {
    Arc::new(move |req| {
        let ledger = ledger.clone();
        let engine = engine.clone();
        tokio::spawn(async move {
            let from_did: Did =
                match serde_json::from_value(serde_json::Value::String(req.from.clone())) {
                    Ok(d) => d,
                    Err(e) => {
                        warn!("Failed to parse payer DID: {}", e);
                        return;
                    }
                };
            let to_did: Did =
                match serde_json::from_value(serde_json::Value::String(req.to.clone())) {
                    Ok(d) => d,
                    Err(e) => {
                        warn!("Failed to parse payee DID: {}", e);
                        return;
                    }
                };

            let amount_i64 = match i64::try_from(req.amount) {
                Ok(v) => v,
                Err(_) => {
                    warn!(
                        "Payment amount overflow: cannot convert {} to i64 for compute payment \
                         (task {}, from {}, to {})",
                        req.amount, req.task_id, req.from, req.to
                    );
                    return;
                }
            };

            // Bilateral compute payments are local-domain settlements; `settle_receipt`
            // treats Local/Cell/Org identically (only Federation/Commons are rejected).
            let settlement_req = icn_ledger::SettlementRequest {
                receipt_hash: payment_dedup_hash(&req.task_id),
                executor: to_did,
                submitter: from_did,
                attester: None,
                scope: icn_kernel_api::ScopeLevel::Local,
                amount: amount_i64,
                currency: req.currency.clone(),
                executor_verified: true,
            };

            let entry = match engine.settle_receipt(&settlement_req) {
                Ok(entry) => entry,
                Err(icn_ledger::LedgerError::DuplicateEntry(_)) => {
                    // Idempotent — this task's payment was already settled.
                    return;
                }
                Err(e) => {
                    warn!(
                        "Failed to settle compute payment for task {}: {}",
                        req.task_id, e
                    );
                    return;
                }
            };

            let mut ledger = ledger.write().await;
            match ledger.append_entry(entry).await {
                Ok(_) => {
                    info!(
                        "Compute payment settled: {} {} from {} to {} for task {}",
                        req.amount, req.currency, req.from, req.to, req.task_id
                    );
                }
                Err(e) => {
                    warn!("Failed to settle compute payment: {}", e);
                }
            }
        });
    })
}

/// Create the commons settlement callback for posting completed-task journal entries.
///
/// When a commons-scope task completes successfully, the compute actor fires this callback
/// with a `CommonsPaymentRequest`. This function:
///
/// 1. Fetches the consumer's current commons credit balance (for the balance-floor invariant).
/// 2. Converts the compute-layer `CommonsPaymentRequest` into a ledger-layer
///    `CommonsSettlementRequest` and runs it through `SettlementEngine::settle_commons_receipt()`.
/// 3. Appends the resulting `(earn_entry, spend_entry)` to the ledger.
///
/// `engine` must be pre-created by the caller. Use `build_settlement_engine()` for
/// restart-safe idempotence.
///
/// Duplicate receipts (`LedgerError::DuplicateEntry`) are silently ignored. All other
/// errors are logged as warnings; task completion is not rolled back.
pub fn create_commons_settlement_callback(
    ledger: LedgerHandle,
    engine: Arc<icn_ledger::SettlementEngine>,
) -> icn_compute::CommonsSettlementCallback {
    Arc::new(move |req: icn_compute::CommonsPaymentRequest| {
        let ledger = ledger.clone();
        let engine = engine.clone();

        tokio::spawn(async move {
            // 1. Parse contributor DID (executor — earns credits)
            let contributor: icn_identity::Did = match req.contributor.parse() {
                Ok(d) => d,
                Err(e) => {
                    warn!(
                        "commons_settlement: unparseable contributor DID '{}': {}",
                        req.contributor, e
                    );
                    return;
                }
            };

            // 2. Parse consumer DID (submitter — spends credits)
            let consumer: icn_identity::Did = match req.consumer.parse() {
                Ok(d) => d,
                Err(e) => {
                    warn!(
                        "commons_settlement: unparseable consumer DID '{}': {}",
                        req.consumer, e
                    );
                    return;
                }
            };

            // 3. Fetch consumer balance for the balance-floor invariant.
            //    Negate: ledger credit entries give negative raw balance; negate to get
            //    spendable capacity (positive = can spend). Matches BalanceCallback convention.
            let consumer_balance = {
                let guard = ledger.read().await;
                -guard.get_balance(
                    &consumer,
                    icn_ledger::commons_credits::COMMONS_CREDIT_CURRENCY,
                )
            };

            // 4. Convert amount: u64 → i64 (SettlementEngine uses i64)
            let amount_i64 = match i64::try_from(req.amount) {
                Ok(v) => v,
                Err(_) => {
                    warn!(
                        "commons_settlement: amount {} overflows i64 for task {}",
                        req.amount, req.task_id
                    );
                    return;
                }
            };

            // 5. Build settlement request DTO
            let settlement_req = icn_ledger::CommonsSettlementRequest {
                receipt_hash: req.receipt_hash,
                contributor,
                consumer,
                scope: icn_kernel_api::ScopeLevel::Commons,
                amount: amount_i64,
                consumer_balance,
                executor_verified: true,
                task_id: Some(req.task_id.clone()),
            };

            // 6. Run through settlement engine (dedup + invariant checks).
            //
            // KNOWN LIMITATION: dedup is recorded inside settle_commons_receipt() before
            // the ledger appends below complete. If append_entry() fails after this point,
            // the receipt is permanently marked settled and retries will return DuplicateEntry.
            // True atomicity requires batch-append support in the ledger (not yet available).
            // On partial failure (earn ok, spend fails), manual recovery is required.
            let (earn_entry, spend_entry) = match engine.settle_commons_receipt(&settlement_req) {
                Ok(entries) => entries,
                Err(icn_ledger::LedgerError::DuplicateEntry(_)) => {
                    // Idempotent — receipt already settled (or a prior partial settlement
                    // recorded dedup before ledger appends completed — check audit logs).
                    return;
                }
                Err(e) => {
                    warn!(
                        "commons_settlement: engine rejected receipt {}: {}",
                        hex::encode(req.receipt_hash),
                        e
                    );
                    return;
                }
            };

            // 7. Append both entries to the ledger (double-entry: earn + spend).
            //    These two appends are NOT atomic. If spend fails after earn succeeds,
            //    the ledger has a one-sided entry and the receipt dedup is already committed.
            //    This constitutes a partial settlement requiring manual operator intervention.
            let mut ledger = ledger.write().await;
            if let Err(e) = ledger.append_entry(earn_entry).await {
                warn!(
                    "commons_settlement: failed to append earn entry for task {}: {}",
                    req.task_id, e
                );
                return;
            }
            match ledger.append_entry(spend_entry).await {
                Ok(_) => {
                    info!(
                        "commons_settlement: settled {} credits, consumer='{}', \
                         contributor='{}', task={}",
                        req.amount, req.consumer, req.contributor, req.task_id
                    );
                }
                Err(e) => {
                    // Earn entry committed but spend entry failed — PARTIAL SETTLEMENT.
                    // Dedup is already recorded; retries will be silently ignored.
                    // Manual operator recovery required: inspect receipt {} and
                    // task {} in audit logs.
                    warn!(
                        "commons_settlement: PARTIAL SETTLEMENT for task {} \
                         (earn committed, spend failed): {} — receipt {} will not retry; \
                         manual recovery required",
                        req.task_id,
                        e,
                        hex::encode(req.receipt_hash),
                    );
                }
            }
        });
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Proof test: completed commons compute task produces durable ledger entries.
    ///
    /// This test closes the durability gap identified in the settlement path trace:
    /// `CommonsSettlementCallback` was wired to `None` in the daemon, so completed commons
    /// tasks produced no ledger consequence. After this fix, the callback fires
    /// `SettlementEngine::settle_commons_receipt()` and appends both earn/spend journal
    /// entries to the ledger — making the economic consequence auditable.
    ///
    /// Flow exercised:
    ///   CommonsPaymentRequest → create_commons_settlement_callback → SettlementEngine
    ///   → (earn_entry, spend_entry) → Ledger::append_entry x2
    ///   → get_balance returns updated values
    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn test_commons_settlement_callback_posts_ledger_entries() {
        use icn_identity::KeyPair;
        use std::time::Duration;

        let tmpdir = tempfile::tempdir().unwrap();
        let store = Arc::new(icn_store::SledStore::open(tmpdir.path()).unwrap());
        let ledger = Arc::new(RwLock::new(icn_ledger::Ledger::new(store).unwrap()));

        let executor_kp = KeyPair::generate().unwrap();
        let submitter_kp = KeyPair::generate().unwrap();

        let executor_did = executor_kp.did().clone();
        let submitter_did = submitter_kp.did().clone();

        // Give the consumer (submitter) an initial commons credit balance.
        // build_earn_entry credits the consumer (net_change = -amount → raw balance = -10_000).
        // The BalanceCallback negates this to produce spendable capacity = +10_000.
        let initial_credit =
            icn_ledger::commons_credits::build_earn_entry(&submitter_did, 10_000).unwrap();
        ledger
            .write()
            .await
            .append_entry(initial_credit)
            .await
            .unwrap();

        // Verify initial raw ledger balance (negative = credits received)
        let initial_raw = ledger.read().await.get_balance(
            &submitter_did,
            icn_ledger::commons_credits::COMMONS_CREDIT_CURRENCY,
        );
        assert_eq!(
            initial_raw, -10_000,
            "raw balance after earn should be -10_000"
        );

        // Wire the callback (the unit under test) — in-memory engine for this unit test
        let engine = Arc::new(icn_ledger::SettlementEngine::new());
        let cb = create_commons_settlement_callback(ledger.clone(), engine);

        // Fire the callback as the compute actor does on commons task completion
        let receipt_hash = [0xAB_u8; 32];
        let settlement_amount = 500_u64;
        cb(icn_compute::CommonsPaymentRequest {
            contributor: executor_did.to_string(),
            consumer: submitter_did.to_string(),
            amount: settlement_amount,
            receipt_hash,
            task_id: "test-task-001".to_string(),
        });

        // Yield to let the spawned async task run and append to the ledger.
        tokio::time::sleep(Duration::from_millis(50)).await;

        // --- Assertions: the economic consequence is now a durable ledger fact ---
        //
        // earn_entry_with_receipt: credit(executor, 500) → executor raw = -500
        // spend_entry_with_receipt: debit(consumer, 500) → consumer raw = -10_000 + 500 = -9_500

        let executor_raw = ledger.read().await.get_balance(
            &executor_did,
            icn_ledger::commons_credits::COMMONS_CREDIT_CURRENCY,
        );
        assert_eq!(
            executor_raw,
            -(settlement_amount as i64),
            "executor raw balance after earning: expected -{} (credit entry)",
            settlement_amount
        );

        let submitter_raw = ledger.read().await.get_balance(
            &submitter_did,
            icn_ledger::commons_credits::COMMONS_CREDIT_CURRENCY,
        );
        assert_eq!(
            submitter_raw,
            -(10_000 - settlement_amount as i64),
            "submitter raw balance after spending: expected {}",
            -(10_000 - settlement_amount as i64)
        );

        // Idempotency: firing the same receipt again must not double-settle
        cb(icn_compute::CommonsPaymentRequest {
            contributor: executor_did.to_string(),
            consumer: submitter_did.to_string(),
            amount: settlement_amount,
            receipt_hash,
            task_id: "test-task-001".to_string(),
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        let executor_raw_after_dupe = ledger.read().await.get_balance(
            &executor_did,
            icn_ledger::commons_credits::COMMONS_CREDIT_CURRENCY,
        );
        assert_eq!(
            executor_raw_after_dupe, executor_raw,
            "duplicate receipt must not double-settle the executor balance"
        );
    }

    /// Regression (B1): the non-commons payment callback must route through
    /// `SettlementEngine`, gaining its dedup/idempotency guarantee instead of appending a
    /// `JournalEntry` directly. Firing the same task's payment twice must settle exactly once.
    ///
    /// Flow exercised:
    ///   PaymentRequest → create_payment_callback → SettlementEngine::settle_receipt
    ///   → balanced entry (debit submitter / credit executor) → Ledger::append_entry
    #[tokio::test]
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    async fn test_payment_callback_settles_through_engine_and_dedupes() {
        use icn_identity::KeyPair;
        use std::time::Duration;

        let tmpdir = tempfile::tempdir().unwrap();
        let store = Arc::new(icn_store::SledStore::open(tmpdir.path()).unwrap());
        let ledger = Arc::new(RwLock::new(icn_ledger::Ledger::new(store).unwrap()));

        let payer_kp = KeyPair::generate().unwrap(); // submitter (debited)
        let payee_kp = KeyPair::generate().unwrap(); // executor (credited)
        let payer_did = payer_kp.did().clone();
        let payee_did = payee_kp.did().clone();

        // Same engine instance the daemon shares between commons and bilateral callbacks.
        let engine = Arc::new(icn_ledger::SettlementEngine::new());
        let cb = create_payment_callback(ledger.clone(), engine.clone());

        let amount = 500_u64;
        let make_req = || icn_compute::PaymentRequest {
            from: payer_did.to_string(),
            to: payee_did.to_string(),
            amount,
            currency: "credits".to_string(),
            task_id: "pay-task-001".to_string(),
        };

        // Fire once: payment settles through the engine.
        cb(make_req());
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Mutual-credit convention: debit(submitter) → +amount raw; credit(executor) → -amount raw.
        let payee_raw = ledger.read().await.get_balance(&payee_did, "credits");
        let payer_raw = ledger.read().await.get_balance(&payer_did, "credits");
        assert_eq!(payee_raw, -(amount as i64), "executor credited once");
        assert_eq!(payer_raw, amount as i64, "submitter debited once");
        assert_eq!(
            engine.settled_count(),
            1,
            "one receipt settled via the engine"
        );

        // Fire the same task's payment again: the engine's dedup must prevent a second settlement.
        cb(make_req());
        tokio::time::sleep(Duration::from_millis(50)).await;

        let payee_raw_after = ledger.read().await.get_balance(&payee_did, "credits");
        let payer_raw_after = ledger.read().await.get_balance(&payer_did, "credits");
        assert_eq!(
            payee_raw_after, payee_raw,
            "duplicate task payment must not double-credit the executor"
        );
        assert_eq!(
            payer_raw_after, payer_raw,
            "duplicate task payment must not double-debit the submitter"
        );
        assert_eq!(
            engine.settled_count(),
            1,
            "still one receipt settled (deduped)"
        );
    }
}
