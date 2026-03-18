//! ICN Ledger - Double-entry mutual credit ledger with Merkle-DAG
//!
//! # Safety
//! This crate denies panicking on unwrap/expect to prevent runtime crashes.
#![allow(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
// Allow unwrap/expect in test code - panics are acceptable for tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//!
//! This crate implements a double-entry bookkeeping system for mutual credit accounting,
//! structured as a Merkle-DAG for content-addressable, tamper-evident storage.
//!
//! ## Core Concepts
//!
//! - **Journal Entries**: Append-only log of debits and credits
//! - **Double-Entry Invariant**: Σ debits == Σ credits per currency
//! - **Merkle-DAG**: Content-addressed entries with parent links
//! - **Multi-Currency**: Support for hours, USD, kWh, and custom currencies
//! - **Credit Limits**: Per-participant, per-currency overdraft limits
//!
//! ## Example
//!
//! ```rust,ignore
//! use icn_ledger::{Ledger, entry::JournalEntryBuilder};
//! use icn_identity::KeyPair;
//! use icn_store::SledStore;
//! use std::sync::Arc;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let store = Arc::new(SledStore::open("./data")?);
//!     let mut ledger = Ledger::new(store)?;
//!
//!     let alice = KeyPair::generate()?.did().clone();
//!     let bob = KeyPair::generate()?.did().clone();
//!
//!     // Alice delivers 10 hours of work to Bob
//!     let entry = JournalEntryBuilder::new(alice.clone())
//!         .debit(alice.clone(), "hours".to_string(), 10)
//!         .credit(bob.clone(), "hours".to_string(), 10)
//!         .build()?;
//!
//!     ledger.append_entry(entry).await?;
//!
//!     // Alice is owed 10 hours
//!     assert_eq!(ledger.get_balance(&alice, "hours"), 10);
//!     // Bob owes 10 hours
//!     assert_eq!(ledger.get_balance(&bob, "hours"), -10);
//!     Ok(())
//! }
//! ```

pub mod allocations;
pub mod asset_types;
pub mod balance;
pub mod commons_credits;
pub mod credit_policy;
pub mod dispute;
pub mod dynamic_limits;
pub mod entry;
#[allow(missing_docs)]
pub mod error;
pub mod events;
#[allow(missing_docs)]
pub mod fork_resolution;
pub mod freeze;
pub mod fx;
pub mod hash;
pub mod labor_shares;
pub mod ledger;
mod ledger_impl; // Internal implementation modules
pub mod membership;
pub mod merge;
pub mod oracle;
pub mod progressive_limits;
pub mod quarantine;
pub mod settlement;
#[allow(missing_docs)]
pub mod sync;
pub mod treasury;
pub mod types;
pub mod use_access;

pub use allocations::create_budget_allocation;
pub use asset_types::{AssetClass, AssetCondition, AssetId, AssetMetadata, AssetRegistry};
pub use credit_policy::{CreditPolicy, CreditPolicyManager, NewMemberPolicy};
pub use dispute::DisputeManager;
pub use dynamic_limits::{
    AccountLimitState, DynamicCreditLimitManager, DynamicLimitConfig, LimitChangeEvent,
    LimitChangeReason,
};
pub use error::{LedgerError, Result};
pub use fork_resolution::{
    Fork, ForkDetector, ForkResolution, ForkResolutionStrategy, ForkResolver,
};
pub use freeze::{FreezeManager, FrozenMember, UnfreezeEvent};
pub use ledger::{ForkStats, Ledger, PaginationCursor};
pub use membership::{MembershipStore, SledMembershipStore};
pub use merge::{ConflictPair, MergeDecision, QuarantineItem};
pub use progressive_limits::{
    AccountVelocityWindow, BalanceLimits, CommonsHolderLookup, DefaultWeakLookup,
    FnCommonsHolderLookup, ProgressiveLimitConfig, ProgressiveLimitManager, VelocityLimitConfig,
};
pub use quarantine::QuarantineStore;
pub use settlement::{CommonsSettlementRequest, SettlementEngine, SettlementRequest};
pub use sync::{deserialize_sync_message, ledger_topic, serialize_sync_message, LedgerSyncMessage};
pub use treasury::{
    ApprovalType, BudgetStatus, PaginatedAuditTrail, SpendingRule, Treasury, TreasuryAuditRecord,
    TreasuryBudget, TreasuryManager, TreasuryOperation, VelocityLimit, VelocityWindow,
};
pub use types::{
    AccountBalances, AccountDelta, ContentHash, CreditLimit, Currency, Dispute, DisputeOutcome,
    DisputeStatus, JournalEntry, QuarantineReason, QuarantinedEntry, Resolution, Signature,
    WitnessConfig, WitnessPolicy, WitnessSignature, WitnessedEntry,
};

// Use-based resource access (anti-rent-seeking)
pub use use_access::{
    AccessError, AccessModel, AntiSpeculationRules, DutyEventType, ResourceAccess, StewardshipDuty,
    UsageEvent,
};

// Labor shares and cooperative bonds (Razeto integration)
pub use labor_shares::{
    BondId, BondOffering, BondPayment, BondPaymentType, BondStatus, Collateral, CooperativeBond,
    LaborShare, PaymentSchedule, ScheduledPayout, ShareEvent, ShareId, ShareStatus,
    SurplusAllocation,
};

// FX (cross-currency) support
pub use fx::{FxConfig, FxConversionDetails, FxError, PreparedFxTransfer};

// Event types for real-time notifications
pub use events::{
    create_shared_emitter, BalanceChanged, BatchBalanceChanged, CrossCurrencyTransfer,
    ForkDetected, ForkResolved, LedgerEvent, LedgerEventEmitter, MemberFrozen, MemberUnfrozen,
    ResourceAccessTransferred, RollbackPerformed, SharedEventEmitter, TransactionConfirmed,
    TransactionCreated, Transfer,
};

// Commons credit accounting
pub use commons_credits::{
    build_earn_entry, build_spend_entry, check_sufficient_balance, compute_credits_earned,
    EarningCapExceeded, EarningTracker, InsufficientCredits, COMMONS_CREDIT_CURRENCY,
};

// Exchange rate oracle
pub use oracle::{
    CacheStats, CurrencyPair, ExchangeRate, FederationRateSource, ManualRateRecord,
    ManualRateSource, OracleConfig, OracleError, OracleManager, OracleResult, PriceFeed,
    RateObservation, SourceInfo,
};

/// Helper function to get current timestamp in seconds (re-exported from icn_time)
#[inline]
pub fn current_timestamp_secs() -> u64 {
    icn_time::current_timestamp_secs()
}
