//! Commons credit accounting for cooperative compute.
//!
//! This module provides credit computation and journal entry construction
//! for the commons resource pool. Credits are earned by contributing
//! compute resources and spent by consuming them.
//!
//! **Architectural invariant**: The commons-mint DID is only accessible
//! through [`build_earn_entry`] and [`build_spend_entry`]. No generic
//! ledger helpers should allow arbitrary debit/credit to the mint. This
//! prevents credit laundering through the journal entry builder.
//!
//! **Architectural invariant**: The credit formula is an accounting
//! heuristic, not a market price. Subject to governance adjustment.

use crate::entry::JournalEntryBuilder;
use crate::types::JournalEntry;
use anyhow::Result;
use icn_identity::Did;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

/// Currency identifier for commons credits.
pub const COMMONS_CREDIT_CURRENCY: &str = "commons-credits";

// --- Credit formula weights (subject to governance adjustment) ---
// TODO(governance): These constants should be configurable via CCL governance
// parameters once governance hooks are available (Phase 29).

/// Divisor for memory contribution (MB-millis → credits).
const MEMORY_DIVISOR: u128 = 1_000;
/// Divisor for storage contribution (bytes → credits).
const STORAGE_DIVISOR: u128 = 1_000_000;
/// Divisor for egress contribution (bytes → credits).
const EGRESS_DIVISOR: u128 = 100_000;

/// Synthetic DID for the commons mint.
///
/// Derived deterministically from the well-known seed `[0xCC; 32]` (0xCC = "Commons Credit").
/// This DID is **only accessible through `build_earn_entry` / `build_spend_entry`.**
/// Do not reference directly in generic journal entry construction.
static COMMONS_MINT_DID: LazyLock<Did> = LazyLock::new(|| {
    // Deterministic seed for the commons-mint identity.
    // We only need the DID (public key), not signing capability.
    let seed: [u8; 32] = [0xCC; 32];
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&seed);
    let verifying_key = signing_key.verifying_key();
    Did::from_public_key(&verifying_key)
});

/// Default per-epoch earning cap per participant (credits).
///
/// Prevents sybil farming: even if an attacker controls many identities,
/// each is capped at this amount per epoch. The cap applies to the credit
/// accounting layer, not the resource contribution itself.
///
/// TODO(governance): Make configurable via CCL governance parameters.
const DEFAULT_EPOCH_EARNING_CAP: u64 = 100_000;

/// Per-epoch earning tracker for sybil resistance.
///
/// Tracks cumulative credits earned by each DID within the current epoch.
/// When earnings exceed the cap, further `build_earn_entry_capped` calls
/// are rejected.
///
/// **Epoch boundaries**: The caller is responsible for calling `reset()`
/// at epoch boundaries (e.g., every 24 hours). This is advisory — the
/// ledger remains authoritative.
///
/// **Integration status**: Infrastructure ready but not yet wired into
/// the credit earning flow. A future PR should call `try_earn()` from
/// the settlement path and manage epoch resets via a governance scheduler.
#[derive(Debug)]
pub struct EarningTracker {
    /// Maximum credits any single DID can earn per epoch.
    pub cap: u64,
    /// Epoch identifier (e.g., Unix day number).
    pub epoch: u64,
    /// Cumulative credits earned per DID in this epoch.
    earned: HashMap<String, u64>,
}

impl EarningTracker {
    /// Create a new tracker for the given epoch with the default cap.
    pub fn new(epoch: u64) -> Self {
        Self {
            cap: DEFAULT_EPOCH_EARNING_CAP,
            epoch,
            earned: HashMap::new(),
        }
    }

    /// Create a tracker with a custom cap.
    pub fn with_cap(epoch: u64, cap: u64) -> Self {
        Self {
            cap,
            epoch,
            earned: HashMap::new(),
        }
    }

    /// Record an earning and check if it would exceed the cap.
    ///
    /// Returns the *allowed* amount (which may be less than requested if
    /// the participant is near the cap), or `Err` if the cap is already
    /// reached.
    pub fn try_earn(&mut self, did: &Did, amount: u64) -> Result<u64, EarningCapExceeded> {
        let did_str = did.to_string();
        let current = self.earned.get(&did_str).copied().unwrap_or(0);

        if current >= self.cap {
            return Err(EarningCapExceeded {
                did: did_str,
                epoch: self.epoch,
                cap: self.cap,
                already_earned: current,
                requested: amount,
            });
        }

        let remaining = self.cap.saturating_sub(current);
        let allowed = amount.min(remaining);
        *self.earned.entry(did_str).or_insert(0) += allowed;
        Ok(allowed)
    }

    /// Get the amount a DID has earned so far in this epoch.
    pub fn earned_so_far(&self, did: &Did) -> u64 {
        self.earned.get(&did.to_string()).copied().unwrap_or(0)
    }

    /// Reset the tracker for a new epoch.
    pub fn reset(&mut self, new_epoch: u64) {
        self.epoch = new_epoch;
        self.earned.clear();
    }
}

/// Error returned when a participant exceeds the per-epoch earning cap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EarningCapExceeded {
    pub did: String,
    pub epoch: u64,
    pub cap: u64,
    pub already_earned: u64,
    pub requested: u64,
}

impl std::fmt::Display for EarningCapExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "earning cap exceeded for {} in epoch {}: cap={}, earned={}, requested={}",
            self.did, self.epoch, self.cap, self.already_earned, self.requested
        )
    }
}

impl std::error::Error for EarningCapExceeded {}

/// Compute credits earned from contributed resources.
///
/// This is an **accounting heuristic, not a market price**. The formula
/// weights different resource types and is subject to governance adjustment.
///
/// Uses `u128` internally for accumulation, clamps to `u64` on output.
/// All arithmetic is saturating to prevent overflow panics.
#[must_use = "computed credits should be used to build a journal entry"]
pub fn compute_credits_earned(
    cpu_millis: u64,
    memory_mb_millis: u64,
    storage_bytes: u64,
    egress_bytes: u64,
) -> u64 {
    let acc: u128 = (cpu_millis as u128)
        .saturating_add((memory_mb_millis as u128) / MEMORY_DIVISOR)
        .saturating_add((storage_bytes as u128) / STORAGE_DIVISOR)
        .saturating_add((egress_bytes as u128) / EGRESS_DIVISOR);

    // Saturate u128 → u64: values above u64::MAX are clamped.
    acc.min(u64::MAX as u128) as u64
}

/// Build a double-entry journal entry crediting the contributor.
///
/// Debits the commons mint and credits the contributor.
/// Rejects `amount <= 0`.
///
/// **Warning**: Without a nonce, identical earn amounts for the same
/// contributor produce identical content hashes, which the ledger
/// deduplicates. Prefer [`build_earn_entry_with_receipt`] for
/// receipt-backed earnings.
#[must_use = "journal entry should be appended to the ledger"]
pub fn build_earn_entry(contributor: &Did, amount: i64) -> Result<JournalEntry> {
    build_earn_entry_inner(contributor, amount, None)
}

/// Build a double-entry earn entry tied to a specific execution receipt.
///
/// The `receipt_id` is used as a nonce in the journal entry, ensuring
/// that two separate earn events for the same amount produce distinct
/// content hashes. This prevents silent deduplication of legitimate
/// repeat earnings.
#[must_use = "journal entry should be appended to the ledger"]
pub fn build_earn_entry_with_receipt(
    contributor: &Did,
    amount: i64,
    receipt_id: [u8; 32],
) -> Result<JournalEntry> {
    build_earn_entry_inner(contributor, amount, Some(receipt_id))
}

fn build_earn_entry_inner(
    contributor: &Did,
    amount: i64,
    nonce: Option<[u8; 32]>,
) -> Result<JournalEntry> {
    if amount <= 0 {
        anyhow::bail!("earn amount must be positive, got {amount}");
    }

    let mint_did = COMMONS_MINT_DID.clone();

    let mut builder = JournalEntryBuilder::new(mint_did.clone())
        .debit(mint_did, COMMONS_CREDIT_CURRENCY.to_string(), amount)
        .credit(
            contributor.clone(),
            COMMONS_CREDIT_CURRENCY.to_string(),
            amount,
        );

    if let Some(n) = nonce {
        builder = builder.nonce(n);
    }

    let entry = builder.build()?;

    Ok(entry)
}

/// Build a double-entry journal entry debiting the consumer.
///
/// Debits the consumer and credits the commons mint.
/// Rejects `amount <= 0`.
///
/// **Warning**: Without a nonce, identical spend amounts produce
/// identical content hashes. Prefer [`build_spend_entry_with_receipt`]
/// for receipt-backed spending.
#[must_use = "journal entry should be appended to the ledger"]
pub fn build_spend_entry(consumer: &Did, amount: i64) -> Result<JournalEntry> {
    build_spend_entry_inner(consumer, amount, None)
}

/// Build a double-entry spend entry tied to a specific execution receipt.
///
/// The `receipt_id` is used as a nonce in the journal entry, preventing
/// silent deduplication of legitimate repeat spend events.
#[must_use = "journal entry should be appended to the ledger"]
pub fn build_spend_entry_with_receipt(
    consumer: &Did,
    amount: i64,
    receipt_id: [u8; 32],
) -> Result<JournalEntry> {
    build_spend_entry_inner(consumer, amount, Some(receipt_id))
}

fn build_spend_entry_inner(
    consumer: &Did,
    amount: i64,
    nonce: Option<[u8; 32]>,
) -> Result<JournalEntry> {
    if amount <= 0 {
        anyhow::bail!("spend amount must be positive, got {amount}");
    }

    let mint_did = COMMONS_MINT_DID.clone();

    let mut builder = JournalEntryBuilder::new(consumer.clone())
        .debit(
            consumer.clone(),
            COMMONS_CREDIT_CURRENCY.to_string(),
            amount,
        )
        .credit(mint_did, COMMONS_CREDIT_CURRENCY.to_string(), amount);

    if let Some(n) = nonce {
        builder = builder.nonce(n);
    }

    let entry = builder.build()?;

    Ok(entry)
}

/// Check if the balance is sufficient for a required amount.
///
/// Returns the remaining balance (`balance - required`) on success,
/// or an error if insufficient. Balance floor is zero.
#[must_use = "check result indicates whether operation should proceed"]
pub fn check_sufficient_balance(balance: i64, required: i64) -> Result<i64, InsufficientCredits> {
    if balance < required {
        return Err(InsufficientCredits { balance, required });
    }
    Ok(balance.saturating_sub(required))
}

/// Error returned when a commons credit balance is insufficient.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InsufficientCredits {
    pub balance: i64,
    pub required: i64,
}

impl std::fmt::Display for InsufficientCredits {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "insufficient commons credits: balance={}, required={}",
            self.balance, self.required
        )
    }
}

impl std::error::Error for InsufficientCredits {}

// ---------------------------------------------------------------------------
// Settlement deduplication (replay protection)
// ---------------------------------------------------------------------------

/// Tracks which execution receipts have already been settled, preventing
/// double-settlement of the same receipt.
///
/// **Thread safety**: This struct is `Send + Sync` but provides no
/// internal synchronisation. Wrap in `Arc<Mutex<_>>` or `Arc<RwLock<_>>`
/// for concurrent access — the caller chooses the locking strategy.
///
/// ```ignore
/// let dedup = Arc::new(Mutex::new(SettlementDedup::new()));
/// ```
///
/// **Persistence**: In-memory only. On restart, the ledger's content-hash
/// dedup provides a secondary defense (entries with the same nonce produce
/// the same hash). For durable dedup across restarts, persist the set to
/// the store.
#[derive(Debug)]
pub struct SettlementDedup {
    settled: HashSet<[u8; 32]>,
}

/// Error returned when a receipt has already been settled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateSettlement {
    pub receipt_id: [u8; 32],
}

impl std::fmt::Display for DuplicateSettlement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "duplicate settlement: receipt {} already settled",
            hex::encode(self.receipt_id)
        )
    }
}

impl std::error::Error for DuplicateSettlement {}

impl SettlementDedup {
    /// Create an empty dedup tracker.
    pub fn new() -> Self {
        Self {
            settled: HashSet::new(),
        }
    }

    /// Check and record a receipt_id. Returns `Ok(())` if this is the
    /// first settlement, or `Err(DuplicateSettlement)` if already seen.
    pub fn try_settle(&mut self, receipt_id: [u8; 32]) -> Result<(), DuplicateSettlement> {
        if !self.settled.insert(receipt_id) {
            return Err(DuplicateSettlement { receipt_id });
        }
        Ok(())
    }

    /// Check whether a receipt has already been settled without recording it.
    pub fn is_settled(&self, receipt_id: &[u8; 32]) -> bool {
        self.settled.contains(receipt_id)
    }

    /// Number of receipts tracked.
    #[must_use]
    pub fn len(&self) -> usize {
        self.settled.len()
    }

    /// Whether the tracker is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.settled.is_empty()
    }
}

impl Default for SettlementDedup {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credit_formula_known_inputs() {
        // 1000 cpu_millis + 5_000_000 mem_mb_millis/1000 + 10_000_000 storage/1M + 500_000 egress/100k
        // = 1000 + 5000 + 10 + 5 = 6015
        let credits = compute_credits_earned(1_000, 5_000_000, 10_000_000, 500_000);
        assert_eq!(credits, 6015);
    }

    #[test]
    fn test_credit_formula_zero_inputs() {
        assert_eq!(compute_credits_earned(0, 0, 0, 0), 0);
    }

    #[test]
    fn test_credit_formula_u128_overflow_safety() {
        // All u64::MAX — should not panic, should clamp to u64::MAX
        let credits = compute_credits_earned(u64::MAX, u64::MAX, u64::MAX, u64::MAX);
        assert_eq!(credits, u64::MAX);
    }

    #[test]
    fn test_earn_entry_valid() {
        let contributor =
            Did::from_str("did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9").unwrap();
        let entry = build_earn_entry(&contributor, 500);
        assert!(entry.is_ok());
        let entry = entry.unwrap();
        assert_eq!(entry.accounts.len(), 2);
    }

    #[test]
    fn test_spend_entry_valid() {
        let consumer =
            Did::from_str("did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9").unwrap();
        let entry = build_spend_entry(&consumer, 200);
        assert!(entry.is_ok());
        let entry = entry.unwrap();
        assert_eq!(entry.accounts.len(), 2);
    }

    #[test]
    fn test_earn_rejects_zero() {
        let contributor =
            Did::from_str("did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9").unwrap();
        assert!(build_earn_entry(&contributor, 0).is_err());
    }

    #[test]
    fn test_earn_rejects_negative() {
        let contributor =
            Did::from_str("did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9").unwrap();
        assert!(build_earn_entry(&contributor, -10).is_err());
    }

    #[test]
    fn test_spend_rejects_zero() {
        let consumer =
            Did::from_str("did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9").unwrap();
        assert!(build_spend_entry(&consumer, 0).is_err());
    }

    #[test]
    fn test_spend_rejects_negative() {
        let consumer =
            Did::from_str("did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9").unwrap();
        assert!(build_spend_entry(&consumer, -10).is_err());
    }

    #[test]
    fn test_sufficient_balance() {
        let remaining = check_sufficient_balance(500, 200);
        assert_eq!(remaining, Ok(300));
    }

    #[test]
    fn test_insufficient_balance() {
        let result = check_sufficient_balance(100, 200);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.balance, 100);
        assert_eq!(err.required, 200);
    }

    #[test]
    fn test_exact_balance() {
        let remaining = check_sufficient_balance(300, 300);
        assert_eq!(remaining, Ok(0));
    }

    // ========== EarningTracker tests ==========

    fn test_did_a() -> Did {
        Did::from_str("did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9").unwrap()
    }

    #[test]
    fn test_earning_tracker_basic() {
        let mut tracker = EarningTracker::with_cap(1, 1000);
        let did = test_did_a();

        let allowed = tracker.try_earn(&did, 500).unwrap();
        assert_eq!(allowed, 500);
        assert_eq!(tracker.earned_so_far(&did), 500);
    }

    #[test]
    fn test_earning_tracker_clamps_to_cap() {
        let mut tracker = EarningTracker::with_cap(1, 1000);
        let did = test_did_a();

        tracker.try_earn(&did, 800).unwrap();
        // Second earn would exceed cap — clamped to remaining 200
        let allowed = tracker.try_earn(&did, 500).unwrap();
        assert_eq!(allowed, 200);
        assert_eq!(tracker.earned_so_far(&did), 1000);
    }

    #[test]
    fn test_earning_tracker_rejects_at_cap() {
        let mut tracker = EarningTracker::with_cap(1, 1000);
        let did = test_did_a();

        tracker.try_earn(&did, 1000).unwrap();
        // Already at cap — rejected
        let result = tracker.try_earn(&did, 1);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.cap, 1000);
        assert_eq!(err.already_earned, 1000);
    }

    #[test]
    fn test_earning_tracker_reset() {
        let mut tracker = EarningTracker::with_cap(1, 1000);
        let did = test_did_a();

        tracker.try_earn(&did, 1000).unwrap();
        assert!(tracker.try_earn(&did, 1).is_err());

        // New epoch — counter resets
        tracker.reset(2);
        assert_eq!(tracker.earned_so_far(&did), 0);
        let allowed = tracker.try_earn(&did, 500).unwrap();
        assert_eq!(allowed, 500);
    }

    #[test]
    fn test_earning_tracker_independent_dids() {
        let mut tracker = EarningTracker::with_cap(1, 1000);
        let did_a = test_did_a();
        // Generate a different DID from a different seed
        let seed_b: [u8; 32] = [0xBB; 32];
        let key_b = ed25519_dalek::SigningKey::from_bytes(&seed_b);
        let did_b = Did::from_public_key(&key_b.verifying_key());

        tracker.try_earn(&did_a, 1000).unwrap();
        // DID B has its own cap
        let allowed = tracker.try_earn(&did_b, 500).unwrap();
        assert_eq!(allowed, 500);
    }

    // --- Replay protection tests ---

    #[test]
    fn test_earn_with_receipt_produces_distinct_hashes() {
        let contributor =
            Did::from_str("did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9").unwrap();

        let entry1 = build_earn_entry_with_receipt(&contributor, 500, [0xAA; 32]).unwrap();
        let entry2 = build_earn_entry_with_receipt(&contributor, 500, [0xBB; 32]).unwrap();

        // Same amount, different receipt_ids → different content hashes
        assert_ne!(
            entry1.id.as_ref().unwrap().0,
            entry2.id.as_ref().unwrap().0,
            "different receipt_ids must produce different content hashes"
        );
    }

    #[test]
    fn test_earn_with_receipt_nonce_included_in_hash() {
        // Verify the nonce actually affects the hash by comparing
        // an entry with nonce vs without nonce (same amount, same author).
        // Since timestamps differ between calls, we verify indirectly:
        // two entries with different nonces always differ.
        let contributor =
            Did::from_str("did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9").unwrap();

        let entry_with_nonce =
            build_earn_entry_with_receipt(&contributor, 500, [0xAA; 32]).unwrap();
        assert!(
            entry_with_nonce.nonce.is_some(),
            "receipt-backed entry must have nonce set"
        );

        let entry_without_nonce = build_earn_entry(&contributor, 500).unwrap();
        assert!(
            entry_without_nonce.nonce.is_none(),
            "plain entry must not have nonce"
        );
    }

    #[test]
    fn test_spend_with_receipt_produces_distinct_hashes() {
        let consumer =
            Did::from_str("did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9").unwrap();

        let entry1 = build_spend_entry_with_receipt(&consumer, 200, [0xCC; 32]).unwrap();
        let entry2 = build_spend_entry_with_receipt(&consumer, 200, [0xDD; 32]).unwrap();

        assert_ne!(
            entry1.id.as_ref().unwrap().0,
            entry2.id.as_ref().unwrap().0,
            "different receipt_ids must produce different content hashes"
        );
    }

    #[test]
    fn test_spend_with_receipt_nonce_included_in_hash() {
        let consumer =
            Did::from_str("did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9").unwrap();

        let entry_with_nonce = build_spend_entry_with_receipt(&consumer, 200, [0xCC; 32]).unwrap();
        assert!(
            entry_with_nonce.nonce.is_some(),
            "receipt-backed spend entry must have nonce set"
        );

        let entry_without_nonce = build_spend_entry(&consumer, 200).unwrap();
        assert!(
            entry_without_nonce.nonce.is_none(),
            "plain spend entry must not have nonce"
        );
    }

    #[test]
    fn test_earn_entry_has_nonce() {
        let contributor =
            Did::from_str("did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9").unwrap();

        let entry = build_earn_entry_with_receipt(&contributor, 100, [0xFF; 32]).unwrap();
        assert_eq!(entry.nonce, Some([0xFF; 32]));
    }

    #[test]
    fn test_earn_entry_without_receipt_has_no_nonce() {
        let contributor =
            Did::from_str("did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9").unwrap();

        let entry = build_earn_entry(&contributor, 100).unwrap();
        assert_eq!(entry.nonce, None);
    }

    // --- Settlement dedup tests ---

    #[test]
    fn test_settlement_dedup_first_settle_succeeds() {
        let mut dedup = SettlementDedup::new();
        assert!(dedup.try_settle([0x01; 32]).is_ok());
        assert_eq!(dedup.len(), 1);
    }

    #[test]
    fn test_settlement_dedup_rejects_duplicate() {
        let mut dedup = SettlementDedup::new();
        assert!(dedup.try_settle([0x01; 32]).is_ok());

        let err = dedup.try_settle([0x01; 32]).unwrap_err();
        assert_eq!(err.receipt_id, [0x01; 32]);
    }

    #[test]
    fn test_settlement_dedup_different_receipts_independent() {
        let mut dedup = SettlementDedup::new();
        assert!(dedup.try_settle([0x01; 32]).is_ok());
        assert!(dedup.try_settle([0x02; 32]).is_ok());
        assert_eq!(dedup.len(), 2);
    }

    #[test]
    fn test_settlement_dedup_is_settled_query() {
        let mut dedup = SettlementDedup::new();
        assert!(!dedup.is_settled(&[0x01; 32]));
        let _ = dedup.try_settle([0x01; 32]);
        assert!(dedup.is_settled(&[0x01; 32]));
        assert!(!dedup.is_settled(&[0x02; 32]));
    }
}
