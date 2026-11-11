#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The retry window after a treasury spend is journaled but its nonce does not advance
//! (icn#2872, defect 2), executed through the real `LedgerServiceImpl::submit_treasury_entry`.
//!
//! `submit_treasury_entry` claims the receipt, then under the ledger lock runs
//! `check_spend_nonce` -> `append_entry` -> `advance_spend_nonce`. If the advance fails, the
//! entry is already journaled and its balance delta applied, yet the whole step reports an error
//! and the receipt claim is released. A retry of the same accepted decision then finds no claim
//! and an unadvanced nonce, and journals the spend a second time.
//!
//! How the failure is produced: `advance_spend_nonce` has two error sources, a failed store
//! write and `checked_add` overflow. The receipt index is a concrete `SledStore` with no
//! injection seam, so these tests use the overflow: the lane is seeded at `u64::MAX`, which
//! passes the nonce check (stored == expected) and fails the advance. No production code is
//! changed and nothing is mocked. Both error sources leave `submit_treasury_entry` through the
//! same `?`, so what happens after the error is the same; the store-write trigger itself is not
//! executed here.
//!
//! Scope: the retry is a second call to `submit_treasury_entry` with the same request. Whether
//! and when the governance executor re-dispatches after such an error is not exercised.
//!
//! Test classes: `control_*` and `characterization_*` pass today. `invariant_*` states the safe
//! behaviour, FAILS today, and is `#[ignore]`d with the reason (run with `--ignored`).

use icn_core::services::LedgerServiceImpl;
use icn_identity::{Did, KeyPair};
use icn_kernel_api::authz::AllowAllOracle;
use icn_kernel_api::services::{LedgerService, TreasuryEntryRequest, TreasuryOperationType};
use icn_ledger::types::{JournalEntry, ProvenanceRef};
use icn_ledger::Ledger;
use icn_store::SledStore;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;
use tokio::sync::RwLock;

const NONCE_PREFIX: &str = "ledger:treasury_nonce:";
const RECEIPT_INDEX_PREFIX: &str = "ledger:receipt_index:";
const RECEIPT: &str = "gov:test-coop:spend-prop-001:receipt";
const DECISION: &str = "sha256:spend-prop-001";
const UNIT: &str = "HOURS";
const AMOUNT: i64 = 10;

struct Harness {
    _tmp: TempDir,
    ledger: Arc<RwLock<Ledger>>,
    service: LedgerServiceImpl,
    index: Arc<SledStore>,
    treasury: Did,
    recipient: Did,
}

impl Harness {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        let treasury = KeyPair::generate().unwrap().did().clone();
        let recipient = KeyPair::generate().unwrap().did().clone();
        let ledger_store = Arc::new(SledStore::open(tmp.path().join("ledger")).unwrap());
        let ledger = Arc::new(RwLock::new(Ledger::new(ledger_store).unwrap()));
        let index = Arc::new(SledStore::open(tmp.path().join("ledger-receipt-index")).unwrap());
        // Same constructor production uses (supervisor/lifecycle.rs): persistent nonce index.
        let service = LedgerServiceImpl::new_with_receipt_index(
            ledger.clone(),
            Arc::new(AllowAllOracle::wildcard()),
            treasury.clone(),
            index.clone(),
        );
        Self {
            _tmp: tmp,
            ledger,
            service,
            index,
            treasury,
            recipient,
        }
    }

    /// One accepted spend decision. Every call builds the same request: a retry, not a new spend.
    fn spend(&self, expected_nonce: u64) -> TreasuryEntryRequest {
        TreasuryEntryRequest {
            treasury_id: self.treasury.to_string(),
            operation_type: TreasuryOperationType::Spend,
            amount: AMOUNT,
            currency: UNIT.to_string(),
            recipient: Some(self.recipient.to_string()),
            memo: "nonce advance window".to_string(),
            expected_nonce: Some(expected_nonce),
            distributions: Vec::new(),
            decision_receipt_id: RECEIPT.to_string(),
            decision_hash: DECISION.to_string(),
        }
    }

    fn seed_nonce(&self, value: u64) {
        self.index
            .db()
            .insert(self.nonce_key(), value.to_le_bytes().as_slice())
            .unwrap();
    }

    fn nonce_key(&self) -> Vec<u8> {
        format!("{NONCE_PREFIX}{}", self.treasury).into_bytes()
    }

    fn stored_nonce(&self) -> Option<u64> {
        self.index.db().get(self.nonce_key()).unwrap().map(|raw| {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(raw.as_ref());
            u64::from_le_bytes(bytes)
        })
    }

    /// The raw receipt-claim row for the decision, if any.
    fn claim_row(&self) -> Option<Vec<u8>> {
        self.index
            .db()
            .get(format!("{RECEIPT_INDEX_PREFIX}{RECEIPT}"))
            .unwrap()
            .map(|raw| raw.to_vec())
    }

    /// Journal entries that carry the decision's receipt.
    async fn journaled_for_receipt(&self) -> Vec<JournalEntry> {
        let ledger = self.ledger.read().await;
        ledger
            .get_all_entries()
            .unwrap()
            .into_iter()
            .filter(|entry| {
                matches!(
                    &entry.provenance,
                    ProvenanceRef::Governance { receipt_id, .. } if receipt_id == RECEIPT
                )
            })
            .collect()
    }

    /// Magnitude of what the treasury has paid out (the ledger's sign convention is
    /// `debit - credit`, so compare magnitudes).
    async fn paid_out(&self) -> i64 {
        self.ledger
            .read()
            .await
            .get_balance(&self.treasury, UNIT)
            .abs()
    }

    /// Wait until the wall clock has left the millisecond of every journaled entry.
    ///
    /// The entry id hashes a millisecond timestamp and treasury entries carry no nonce, so a
    /// retry built in the same millisecond has the same id as the first attempt (icn#2871).
    /// That is a different defect; stepping past the millisecond keeps it out of this test.
    /// This is a wait on a condition, bounded by one clock tick, not a fixed delay.
    async fn leave_the_journaled_millisecond(&self) {
        let latest = self
            .journaled_for_receipt()
            .await
            .iter()
            .map(|entry| entry.timestamp)
            .max()
            .unwrap_or(0);
        while now_ms() <= latest {
            std::hint::spin_loop();
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Control: at an ordinary nonce the fixture applies a spend, advances the lane, and answers a
/// retry of the same decision with the same entry.
#[tokio::test(flavor = "multi_thread")]
async fn control_a_spend_at_an_ordinary_nonce_is_idempotent_by_receipt() {
    let h = Harness::new();

    let first = h.service.submit_treasury_entry(h.spend(0)).unwrap();
    h.leave_the_journaled_millisecond().await;
    let retry = h.service.submit_treasury_entry(h.spend(0)).unwrap();

    assert_eq!(retry.entry_hash, first.entry_hash);
    assert_eq!(h.journaled_for_receipt().await.len(), 1);
    assert_eq!(h.paid_out().await, AMOUNT);
    assert_eq!(h.stored_nonce(), Some(1));
    assert!(h.claim_row().is_some(), "the claim is finalized");
}

/// The failed step reports an error although the spend is journaled and applied, and it leaves
/// no claim behind.
#[tokio::test(flavor = "multi_thread")]
async fn characterization_a_failed_nonce_advance_errors_after_the_spend_is_journaled() {
    let h = Harness::new();
    h.seed_nonce(u64::MAX);

    let err = h
        .service
        .submit_treasury_entry(h.spend(u64::MAX))
        .unwrap_err();
    assert!(
        err.contains("Treasury nonce overflow"),
        "the failure must be the nonce advance, got: {err}"
    );

    assert_eq!(
        h.journaled_for_receipt().await.len(),
        1,
        "the entry was journaled before the advance failed"
    );
    assert_eq!(h.paid_out().await, AMOUNT, "and its delta was applied");
    assert_eq!(h.stored_nonce(), Some(u64::MAX), "the lane did not move");
    assert!(
        h.claim_row().is_none(),
        "the receipt claim was released, so nothing records that this decision already ran"
    );
}

/// Today's behaviour: a retry of the same accepted decision journals the spend a second time
/// and the treasury pays twice.
#[tokio::test(flavor = "multi_thread")]
async fn characterization_a_retry_after_a_failed_nonce_advance_journals_the_spend_again() {
    let h = Harness::new();
    h.seed_nonce(u64::MAX);

    h.service
        .submit_treasury_entry(h.spend(u64::MAX))
        .unwrap_err();
    h.leave_the_journaled_millisecond().await;
    h.service
        .submit_treasury_entry(h.spend(u64::MAX))
        .unwrap_err();

    assert_eq!(h.journaled_for_receipt().await.len(), 2);
    assert_eq!(h.paid_out().await, 2 * AMOUNT);
}

/// The safe behaviour: one accepted decision is one journaled spend, however many times its
/// execution is retried after a failure that followed the journal write.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "RED contract icn#2872 defect 2: a retry after a failed nonce advance journals the spend again"]
async fn invariant_a_retry_after_a_failed_nonce_advance_must_not_journal_again() {
    let h = Harness::new();
    h.seed_nonce(u64::MAX);

    let _ = h.service.submit_treasury_entry(h.spend(u64::MAX));
    h.leave_the_journaled_millisecond().await;
    let _ = h.service.submit_treasury_entry(h.spend(u64::MAX));

    assert_eq!(
        h.journaled_for_receipt().await.len(),
        1,
        "one decision, one journaled spend"
    );
    assert_eq!(h.paid_out().await, AMOUNT, "the treasury pays once");
}
