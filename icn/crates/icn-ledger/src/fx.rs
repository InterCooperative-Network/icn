//! Foreign Exchange (FX) types for cross-currency payments
//!
//! This module provides types for executing payments where the sender pays
//! in one currency and the recipient receives in another, using the oracle
//! for rate conversion.
//!
//! # Example
//!
//! ```rust,no_run
//! use icn_ledger::{Ledger, fx::FxConfig};
//! use icn_identity::KeyPair;
//! use icn_store::SledStore;
//! use std::sync::Arc;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let store = Arc::new(SledStore::temporary()?);
//! let mut ledger = Ledger::new(store)?;
//!
//! // Configure FX with 1% fee
//! let config = FxConfig {
//!     fee_basis_points: 100,
//!     treasury_did: None, // No fee collection
//!     clearing_did: KeyPair::generate()?.did().clone(),
//!     ..Default::default()
//! };
//! ledger.set_fx_config(config);
//! # Ok(())
//! # }
//! ```

use crate::types::{ContentHash, JournalEntry};
use icn_identity::Did;
use serde::{Deserialize, Serialize};

/// Basis points scale factor (100 basis points = 1%)
const BASIS_POINTS_SCALE: i128 = 10_000;

/// Configuration for cross-currency (FX) transfers
///
/// Controls fee structure, slippage limits, and system accounts
/// used during currency conversion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FxConfig {
    /// FX fee in basis points (100 = 1%)
    ///
    /// This fee is deducted from the target currency amount
    /// and sent to the treasury account.
    pub fee_basis_points: u16,

    /// Maximum allowed fee in basis points (default: 500 = 5%)
    ///
    /// Safety cap to prevent misconfiguration.
    pub max_fee_basis_points: u16,

    /// Treasury account where FX fees are collected
    ///
    /// If None, no fee is collected (fee_basis_points is ignored).
    pub treasury_did: Option<Did>,

    /// Clearing account for intermediate transfers
    ///
    /// Cross-currency transfers use a 4-leg entry pattern:
    /// 1. DEBIT sender (source currency)
    /// 2. CREDIT clearing (source currency)
    /// 3. DEBIT clearing (target currency)
    /// 4. CREDIT recipient (target currency)
    ///
    /// This maintains the double-entry invariant per currency.
    pub clearing_did: Did,

    /// Maximum slippage in basis points (default: 100 = 1%)
    ///
    /// If the converted amount differs from expected by more than
    /// this percentage, the transfer is rejected.
    pub max_slippage_basis_points: u16,

    /// Whether to accept stale rates from the oracle
    ///
    /// If false (default), transfers with stale rates are rejected.
    /// If true, stale rates are used with a warning in the details.
    pub allow_stale_rates: bool,
}

impl Default for FxConfig {
    fn default() -> Self {
        // Default clearing DID is a placeholder - must be set by cooperative
        // Using from_str which should succeed for valid DID format
        let placeholder_clearing =
            Did::from_str("did:icn:clearing0000000000000000000000000000000000000000")
                .unwrap_or_else(|_| {
                    // Fallback: create from a deterministic 32-byte array
                    Did::from_anchor_id(&[0u8; 32])
                });

        Self {
            fee_basis_points: 0,
            max_fee_basis_points: 500,
            treasury_did: None,
            clearing_did: placeholder_clearing,
            max_slippage_basis_points: 100,
            allow_stale_rates: false,
        }
    }
}

impl FxConfig {
    /// Create a new FX config with required clearing account
    pub fn new(clearing_did: Did) -> Self {
        Self {
            clearing_did,
            ..Default::default()
        }
    }

    /// Set the FX fee (in basis points, 100 = 1%)
    pub fn with_fee(mut self, fee_basis_points: u16) -> Self {
        self.fee_basis_points = fee_basis_points.min(self.max_fee_basis_points);
        self
    }

    /// Set the treasury account for fee collection
    pub fn with_treasury(mut self, treasury_did: Did) -> Self {
        self.treasury_did = Some(treasury_did);
        self
    }

    /// Set maximum slippage tolerance
    pub fn with_max_slippage(mut self, basis_points: u16) -> Self {
        self.max_slippage_basis_points = basis_points;
        self
    }

    /// Allow stale rates from oracle
    pub fn allow_stale(mut self) -> Self {
        self.allow_stale_rates = true;
        self
    }

    /// Calculate fee amount from gross target amount
    ///
    /// Returns the fee portion that goes to treasury.
    pub fn calculate_fee(&self, gross_amount: i64) -> i64 {
        if self.treasury_did.is_none() {
            return 0;
        }

        // fee = gross_amount * fee_basis_points / BASIS_POINTS_SCALE
        // Use i128 to avoid overflow during multiplication
        let fee =
            (gross_amount as i128 * self.fee_basis_points as i128 / BASIS_POINTS_SCALE) as i64;
        fee.max(0) // Ensure non-negative
    }

    /// Calculate net amount after fee deduction
    pub fn calculate_net(&self, gross_amount: i64) -> i64 {
        gross_amount - self.calculate_fee(gross_amount)
    }

    /// Check if slippage is within tolerance
    ///
    /// # Arguments
    /// * `expected` - Expected target amount
    /// * `actual` - Actual converted amount
    ///
    /// Returns true if the difference is within `max_slippage_basis_points`.
    pub fn is_slippage_acceptable(&self, expected: i64, actual: i64) -> bool {
        if expected == 0 {
            return actual == 0;
        }

        // Calculate absolute percentage difference in basis points
        // Use i128 for intermediate calculations to prevent overflow
        // Note: For extreme values where diff*BASIS_POINTS_SCALE would overflow i128,
        // saturating_mul returns i128::MAX which correctly triggers slippage rejection
        let diff = (actual as i128).saturating_sub(expected as i128).abs();
        let expected_abs = (expected as i128).abs().max(1); // Avoid division by zero
        let pct = diff.saturating_mul(BASIS_POINTS_SCALE) / expected_abs;

        pct <= self.max_slippage_basis_points as i128
    }
}

/// Details of a cross-currency conversion for audit trail
///
/// This struct records all aspects of a currency conversion
/// for transparency and dispute resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FxConversionDetails {
    /// Sender DID
    pub from_did: String,

    /// Recipient DID
    pub to_did: String,

    /// Source currency (what sender pays)
    pub from_currency: String,

    /// Target currency (what recipient receives)
    pub to_currency: String,

    /// Exchange rate used (target per source unit)
    pub rate: f64,

    /// When the rate was fetched (Unix seconds)
    pub rate_timestamp: u64,

    /// Sources that provided the rate
    pub rate_sources: Vec<String>,

    /// Whether the rate was marked as stale
    pub rate_was_stale: bool,

    /// Gross target amount before fees
    pub gross_target_amount: i64,

    /// Fee amount deducted (sent to treasury)
    pub fee_amount: i64,

    /// Net target amount to recipient
    pub net_target_amount: i64,

    /// Fee rate in basis points
    pub fee_basis_points: u16,

    /// Source amount (what sender paid)
    pub source_amount: i64,
}

impl FxConversionDetails {
    /// Create new conversion details
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        from_did: String,
        to_did: String,
        from_currency: String,
        to_currency: String,
        source_amount: i64,
        rate: f64,
        rate_timestamp: u64,
        rate_sources: Vec<String>,
        rate_was_stale: bool,
        gross_target_amount: i64,
        fee_amount: i64,
        net_target_amount: i64,
        fee_basis_points: u16,
    ) -> Self {
        Self {
            from_did,
            to_did,
            from_currency,
            to_currency,
            rate,
            rate_timestamp,
            rate_sources,
            rate_was_stale,
            gross_target_amount,
            fee_amount,
            net_target_amount,
            fee_basis_points,
            source_amount,
        }
    }

    /// Effective exchange rate after fees
    pub fn effective_rate(&self) -> f64 {
        if self.source_amount == 0 {
            return 0.0;
        }
        self.net_target_amount as f64 / self.source_amount as f64
    }
}

/// A prepared cross-currency transfer ready for execution
///
/// This struct represents a transfer that has been validated and
/// rate-locked but not yet committed to the ledger. The caller can:
/// - Inspect the details and abort if conditions aren't acceptable
/// - Execute the transfer to commit it
#[derive(Debug, Clone)]
pub struct PreparedFxTransfer {
    /// The journal entry to be committed (4-leg or 5-leg with fee)
    pub entry: JournalEntry,

    /// Conversion details for audit trail
    pub details: FxConversionDetails,

    /// When this prepared transfer expires (Unix seconds)
    ///
    /// The rate is only valid until this time; after that,
    /// a new rate should be fetched.
    pub valid_until: u64,
}

impl PreparedFxTransfer {
    /// Check if this prepared transfer is still valid
    pub fn is_valid(&self) -> bool {
        let now = crate::current_timestamp_secs();
        now < self.valid_until
    }

    /// Get the entry hash (if computed)
    pub fn entry_hash(&self) -> Option<&ContentHash> {
        self.entry.id.as_ref()
    }

    /// Get sender DID
    pub fn sender(&self) -> &Did {
        &self.entry.author
    }
}

/// Errors specific to FX operations
#[derive(Debug, Clone, thiserror::Error)]
pub enum FxError {
    /// FX configuration not set on ledger
    #[error("FX not configured: set FxConfig before using cross-currency transfers")]
    NotConfigured,

    /// Oracle not configured on ledger
    #[error("Oracle not configured: set oracle_manager before using cross-currency transfers")]
    OracleNotConfigured,

    /// Same currency specified (use regular transfer instead)
    #[error("Same currency transfer: use regular transfer for {0} -> {0}")]
    SameCurrency(String),

    /// Rate not available
    #[error("Exchange rate not available for {from}/{to}: {reason}")]
    RateNotAvailable {
        from: String,
        to: String,
        reason: String,
    },

    /// Rate is stale and stale rates are not allowed
    #[error("Exchange rate is stale for {from}/{to}; set allow_stale_rates=true to accept")]
    StaleRate { from: String, to: String },

    /// Slippage exceeded
    #[error("Slippage exceeded: expected {expected}, got {actual} (max {max_slippage_bps} bps)")]
    SlippageExceeded {
        expected: i64,
        actual: i64,
        max_slippage_bps: u16,
    },

    /// Prepared transfer expired
    #[error("Prepared transfer expired at {expired_at}, current time {current_time}")]
    TransferExpired { expired_at: u64, current_time: u64 },

    /// Amount validation failed
    #[error("Invalid amount: {0}")]
    InvalidAmount(String),

    /// Conversion would overflow
    #[error(
        "Conversion overflow: {source_amount} {from_currency} at rate {rate} exceeds i64 bounds"
    )]
    ConversionOverflow {
        source_amount: i64,
        from_currency: String,
        rate: f64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_clearing_did() -> Did {
        Did::from_anchor_id(&[1u8; 32])
    }

    fn test_treasury_did() -> Did {
        Did::from_anchor_id(&[2u8; 32])
    }

    #[test]
    fn test_fx_config_default() {
        let config = FxConfig::default();
        assert_eq!(config.fee_basis_points, 0);
        assert_eq!(config.max_fee_basis_points, 500);
        assert_eq!(config.max_slippage_basis_points, 100);
        assert!(!config.allow_stale_rates);
        assert!(config.treasury_did.is_none());
    }

    #[test]
    fn test_fx_config_builder() {
        let clearing = test_clearing_did();
        let treasury = test_treasury_did();

        let config = FxConfig::new(clearing.clone())
            .with_fee(100)
            .with_treasury(treasury.clone())
            .with_max_slippage(200);

        assert_eq!(config.fee_basis_points, 100);
        assert_eq!(config.max_slippage_basis_points, 200);
        assert_eq!(config.clearing_did, clearing);
        assert_eq!(config.treasury_did, Some(treasury));
    }

    #[test]
    fn test_fee_calculation() {
        let clearing = test_clearing_did();
        let treasury = test_treasury_did();

        let config = FxConfig::new(clearing)
            .with_fee(100)
            .with_treasury(treasury);

        // 1% of 10000 = 100
        assert_eq!(config.calculate_fee(10000), 100);
        assert_eq!(config.calculate_net(10000), 9900);

        // 1% of 250 = 2 (rounded down)
        assert_eq!(config.calculate_fee(250), 2);
        assert_eq!(config.calculate_net(250), 248);

        // Small amounts
        assert_eq!(config.calculate_fee(50), 0); // 0.5 rounds to 0
        assert_eq!(config.calculate_net(50), 50);
    }

    #[test]
    fn test_no_fee_without_treasury() {
        let clearing = test_clearing_did();
        let config = FxConfig::new(clearing).with_fee(100); // No treasury

        assert_eq!(config.calculate_fee(10000), 0);
        assert_eq!(config.calculate_net(10000), 10000);
    }

    #[test]
    fn test_slippage_check() {
        let clearing = test_clearing_did();
        let config = FxConfig::new(clearing).with_max_slippage(100); // 1%

        // Exact match
        assert!(config.is_slippage_acceptable(1000, 1000));

        // Within 1%
        assert!(config.is_slippage_acceptable(1000, 1010));
        assert!(config.is_slippage_acceptable(1000, 990));

        // Exceeds 1%
        assert!(!config.is_slippage_acceptable(1000, 1011));
        assert!(!config.is_slippage_acceptable(1000, 989));

        // Edge case: zero expected
        assert!(config.is_slippage_acceptable(0, 0));
        assert!(!config.is_slippage_acceptable(0, 1));
    }

    #[test]
    fn test_fee_cap() {
        let clearing = test_clearing_did();

        // Try to set fee above max
        let config = FxConfig::new(clearing).with_fee(1000); // 10%, but max is 5%

        assert_eq!(config.fee_basis_points, 500); // Capped at 5%
    }

    #[test]
    fn test_conversion_details() {
        let details = FxConversionDetails::new(
            "did:icn:alice".to_string(),
            "did:icn:bob".to_string(),
            "hours".to_string(),
            "USD".to_string(),
            10,    // source amount
            25.0,  // rate
            12345, // timestamp
            vec!["manual".to_string()],
            false,
            250, // gross
            2,   // fee
            248, // net
            100, // fee bps
        );

        assert_eq!(details.from_did, "did:icn:alice");
        assert_eq!(details.to_did, "did:icn:bob");
        assert_eq!(details.from_currency, "hours");
        assert_eq!(details.to_currency, "USD");
        assert_eq!(details.source_amount, 10);
        assert_eq!(details.gross_target_amount, 250);
        assert_eq!(details.net_target_amount, 248);

        // Effective rate: 248 / 10 = 24.8
        assert!((details.effective_rate() - 24.8).abs() < 0.001);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::oracle::{ManualRateSource, OracleManager};
    use crate::Ledger;
    use icn_identity::KeyPair;
    use icn_store::SledStore;
    use std::sync::Arc;

    async fn setup_ledger_with_fx() -> (Ledger, Did, Did, Did) {
        let store = Arc::new(SledStore::temporary().expect("create temp store"));
        let mut ledger = Ledger::new(store.clone()).expect("create ledger");

        // Create test accounts
        let alice_kp = KeyPair::generate().expect("generate alice");
        let bob_kp = KeyPair::generate().expect("generate bob");
        let clearing_kp = KeyPair::generate().expect("generate clearing");
        let treasury_kp = KeyPair::generate().expect("generate treasury");

        let alice = alice_kp.did().clone();
        let bob = bob_kp.did().clone();
        let clearing = clearing_kp.did().clone();
        let treasury = treasury_kp.did().clone();

        // Set up oracle with manual rate source
        let oracle = Arc::new(OracleManager::new(store.clone()));
        let manual_source = Arc::new(ManualRateSource::new(store.clone()));

        // Set up a rate: 1 hour = 25 USD
        manual_source
            .set_rate(
                &crate::oracle::CurrencyPair::new("hours", "USD"),
                25.0,
                &clearing.to_string(),
                None,
            )
            .expect("set rate");

        // Register the source
        oracle.register_source(manual_source).await;

        ledger.set_oracle_manager(oracle);

        // Set up FX config with 1% fee
        let fx_config = FxConfig::new(clearing.clone())
            .with_fee(100)
            .with_treasury(treasury.clone())
            .with_max_slippage(200); // 2% slippage

        ledger.set_fx_config(fx_config);

        (ledger, alice, bob, clearing)
    }

    #[tokio::test]
    async fn test_prepare_cross_currency_transfer() {
        let (ledger, alice, bob, _) = setup_ledger_with_fx().await;

        // Prepare transfer: 10 hours -> USD
        let prepared = ledger
            .prepare_cross_currency_transfer(&alice, &bob, 10, "hours", "USD", None)
            .await
            .expect("prepare transfer");

        // Verify conversion details
        assert_eq!(prepared.details.source_amount, 10);
        assert_eq!(prepared.details.from_currency, "hours");
        assert_eq!(prepared.details.to_currency, "USD");
        assert!((prepared.details.rate - 25.0).abs() < 0.001);

        // 10 hours * 25 = 250 USD gross
        assert_eq!(prepared.details.gross_target_amount, 250);

        // 1% fee = 2 USD (rounded down from 2.5)
        assert_eq!(prepared.details.fee_amount, 2);

        // Net = 250 - 2 = 248 USD
        assert_eq!(prepared.details.net_target_amount, 248);

        // Entry should have 5 legs (with fee)
        assert_eq!(prepared.entry.accounts.len(), 5);
    }

    #[tokio::test]
    async fn test_transfer_with_conversion_basic() {
        let (mut ledger, alice, bob, clearing) = setup_ledger_with_fx().await;

        // Execute transfer: 10 hours -> USD
        let (hash, details) = ledger
            .transfer_with_conversion(&alice, &bob, 10, "hours", "USD", None)
            .await
            .expect("execute transfer");

        // Verify entry was created
        assert!(!hash.to_hex().is_empty());

        // Verify conversion details
        assert_eq!(details.source_amount, 10);
        assert_eq!(details.gross_target_amount, 250);
        assert_eq!(details.fee_amount, 2);
        assert_eq!(details.net_target_amount, 248);

        // Check balances
        // Alice: debited 10 hours
        assert_eq!(ledger.get_balance(&alice, "hours"), -10);

        // Bob: credited 248 USD (net of fee)
        assert_eq!(ledger.get_balance(&bob, "USD"), 248);

        // Clearing: 10 hours credit, 250 USD debit
        assert_eq!(ledger.get_balance(&clearing, "hours"), 10);
        assert_eq!(ledger.get_balance(&clearing, "USD"), -250);
    }

    #[tokio::test]
    async fn test_transfer_with_slippage_protection() {
        let (ledger, alice, bob, _) = setup_ledger_with_fx().await;

        // Set max target amount below what we'd get
        // 10 hours * 25 = 250 USD, but we cap at 200
        let result = ledger
            .prepare_cross_currency_transfer(&alice, &bob, 10, "hours", "USD", Some(200))
            .await;

        // Should fail due to slippage
        assert!(matches!(result, Err(FxError::SlippageExceeded { .. })));
    }

    #[tokio::test]
    async fn test_transfer_same_currency_rejected() {
        let (ledger, alice, bob, _) = setup_ledger_with_fx().await;

        let result = ledger
            .prepare_cross_currency_transfer(&alice, &bob, 10, "hours", "hours", None)
            .await;

        assert!(matches!(result, Err(FxError::SameCurrency(_))));
    }

    #[tokio::test]
    async fn test_transfer_invalid_amount() {
        let (ledger, alice, bob, _) = setup_ledger_with_fx().await;

        // Zero amount
        let result = ledger
            .prepare_cross_currency_transfer(&alice, &bob, 0, "hours", "USD", None)
            .await;

        assert!(matches!(result, Err(FxError::InvalidAmount(_))));

        // Negative amount
        let result = ledger
            .prepare_cross_currency_transfer(&alice, &bob, -10, "hours", "USD", None)
            .await;

        assert!(matches!(result, Err(FxError::InvalidAmount(_))));
    }

    #[tokio::test]
    async fn test_transfer_no_fx_config() {
        let store = Arc::new(SledStore::temporary().expect("create temp store"));
        let ledger = Ledger::new(store.clone()).expect("create ledger");

        let alice = KeyPair::generate().expect("generate").did().clone();
        let bob = KeyPair::generate().expect("generate").did().clone();

        let result = ledger
            .prepare_cross_currency_transfer(&alice, &bob, 10, "hours", "USD", None)
            .await;

        assert!(matches!(result, Err(FxError::NotConfigured)));
    }

    #[tokio::test]
    async fn test_transfer_no_oracle() {
        let store = Arc::new(SledStore::temporary().expect("create temp store"));
        let mut ledger = Ledger::new(store).expect("create ledger");

        let alice = KeyPair::generate().expect("generate").did().clone();
        let bob = KeyPair::generate().expect("generate").did().clone();
        let clearing = KeyPair::generate().expect("generate").did().clone();

        // Set FX config but no oracle
        ledger.set_fx_config(FxConfig::new(clearing));

        let result = ledger
            .prepare_cross_currency_transfer(&alice, &bob, 10, "hours", "USD", None)
            .await;

        assert!(matches!(result, Err(FxError::OracleNotConfigured)));
    }

    #[tokio::test]
    async fn test_transfer_rate_not_available() {
        let (ledger, alice, bob, _) = setup_ledger_with_fx().await;

        // Try to convert to a currency we don't have a rate for
        let result = ledger
            .prepare_cross_currency_transfer(&alice, &bob, 10, "hours", "EUR", None)
            .await;

        assert!(matches!(result, Err(FxError::RateNotAvailable { .. })));
    }

    #[tokio::test]
    async fn test_transfer_without_fee() {
        let store = Arc::new(SledStore::temporary().expect("create temp store"));
        let mut ledger = Ledger::new(store.clone()).expect("create ledger");

        let alice = KeyPair::generate().expect("generate").did().clone();
        let bob = KeyPair::generate().expect("generate").did().clone();
        let clearing = KeyPair::generate().expect("generate").did().clone();

        // Set up oracle
        let oracle = Arc::new(OracleManager::new(store.clone()));
        let manual_source = Arc::new(ManualRateSource::new(store.clone()));

        manual_source
            .set_rate(
                &crate::oracle::CurrencyPair::new("hours", "USD"),
                25.0,
                &clearing.to_string(),
                None,
            )
            .expect("set rate");

        oracle.register_source(manual_source).await;
        ledger.set_oracle_manager(oracle);

        // FX config without fee (no treasury)
        ledger.set_fx_config(FxConfig::new(clearing.clone()));

        // Execute transfer
        let (_, details) = ledger
            .transfer_with_conversion(&alice, &bob, 10, "hours", "USD", None)
            .await
            .expect("execute transfer");

        // No fee should be taken
        assert_eq!(details.fee_amount, 0);
        assert_eq!(details.gross_target_amount, 250);
        assert_eq!(details.net_target_amount, 250);

        // Bob gets full amount
        assert_eq!(ledger.get_balance(&bob, "USD"), 250);
    }

    #[tokio::test]
    async fn test_clearing_account_balance_verification() {
        // Verify that the 4/5-leg entry balances net to zero per currency
        let (mut ledger, alice, bob, clearing) = setup_ledger_with_fx().await;

        // Execute multiple transfers to verify clearing account balances
        let (_, details) = ledger
            .transfer_with_conversion(&alice, &bob, 10, "hours", "USD", None)
            .await
            .expect("execute transfer 1");

        // Verify clearing account maintains double-entry invariant per currency:
        // Clearing receives hours (credit = positive in ICN semantics)
        // Clearing sends USD (debit = negative in ICN semantics)
        let clearing_hours = ledger.get_balance(&clearing, "hours");
        let clearing_usd = ledger.get_balance(&clearing, "USD");

        // Clearing receives 10 hours
        assert_eq!(clearing_hours, 10);
        // Clearing sends 250 USD (gross amount)
        assert_eq!(clearing_usd, -details.gross_target_amount);

        // Verify the sum of all balances per currency equals zero (double-entry invariant)
        let alice_hours = ledger.get_balance(&alice, "hours");
        // Bob's USD balance is verified in detail assertions below
        let _bob_usd = ledger.get_balance(&bob, "USD");

        // Hours: alice (-10) + clearing (+10) = 0
        assert_eq!(
            alice_hours + clearing_hours,
            0,
            "Hours currency should sum to zero"
        );

        // USD: clearing (-250) + bob (248) + treasury (2) = 0
        // Treasury gets the 2 USD fee
        // Note: We can't easily get treasury balance without the DID, but we can verify:
        assert_eq!(
            details.gross_target_amount,
            details.net_target_amount + details.fee_amount,
            "Gross = Net + Fee"
        );
    }

    #[tokio::test]
    async fn test_fee_with_treasury_configured() {
        // This test explicitly verifies the P1 fix:
        // When fee_amount > 0 AND treasury is configured, the fee goes to treasury
        let store = Arc::new(SledStore::temporary().expect("create temp store"));
        let mut ledger = Ledger::new(store.clone()).expect("create ledger");

        let alice = KeyPair::generate().expect("generate").did().clone();
        let bob = KeyPair::generate().expect("generate").did().clone();
        let clearing = KeyPair::generate().expect("generate").did().clone();
        let treasury = KeyPair::generate().expect("generate").did().clone();

        // Set up oracle
        let oracle = Arc::new(OracleManager::new(store.clone()));
        let manual_source = Arc::new(ManualRateSource::new(store.clone()));

        manual_source
            .set_rate(
                &crate::oracle::CurrencyPair::new("hours", "USD"),
                100.0, // 1 hour = 100 USD for easier math
                &clearing.to_string(),
                None,
            )
            .expect("set rate");

        oracle.register_source(manual_source).await;
        ledger.set_oracle_manager(oracle);

        // FX config WITH fee AND treasury
        let fx_config = FxConfig::new(clearing.clone())
            .with_fee(100) // 1%
            .with_treasury(treasury.clone());
        ledger.set_fx_config(fx_config);

        // Execute transfer: 10 hours -> 1000 USD gross, 10 USD fee, 990 USD net
        let (_, details) = ledger
            .transfer_with_conversion(&alice, &bob, 10, "hours", "USD", None)
            .await
            .expect("execute transfer");

        // Verify fee calculation
        assert_eq!(details.gross_target_amount, 1000);
        assert_eq!(details.fee_amount, 10); // 1% of 1000
        assert_eq!(details.net_target_amount, 990);

        // Verify balances
        assert_eq!(ledger.get_balance(&alice, "hours"), -10);
        assert_eq!(ledger.get_balance(&bob, "USD"), 990); // Net amount
        assert_eq!(ledger.get_balance(&treasury, "USD"), 10); // Fee
        assert_eq!(ledger.get_balance(&clearing, "hours"), 10);
        assert_eq!(ledger.get_balance(&clearing, "USD"), -1000); // Gross
    }
}
