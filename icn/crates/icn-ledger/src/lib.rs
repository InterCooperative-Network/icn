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
//! ```rust,no_run
//! use icn_ledger::{Ledger, entry::JournalEntryBuilder};
//! use icn_identity::KeyPair;
//! use icn_store::SledStore;
//! use std::sync::Arc;
//!
//! # fn main() -> anyhow::Result<()> {
//! let store = Arc::new(SledStore::open("./data")?);
//! let mut ledger = Ledger::new(store)?;
//!
//! let alice = KeyPair::generate()?.did().clone();
//! let bob = KeyPair::generate()?.did().clone();
//!
//! // Alice delivers 10 hours of work to Bob
//! let entry = JournalEntryBuilder::new(alice.clone())
//!     .debit(alice.clone(), "hours".to_string(), 10)
//!     .credit(bob.clone(), "hours".to_string(), 10)
//!     .build()?;
//!
//! ledger.append_entry(entry)?;
//!
//! // Alice is owed 10 hours
//! assert_eq!(ledger.get_balance(&alice, "hours"), 10);
//! // Bob owes 10 hours
//! assert_eq!(ledger.get_balance(&bob, "hours"), -10);
//! # Ok(())
//! # }
//! ```

pub mod balance;
pub mod credit_policy;
pub mod dispute;
pub mod entry;
#[allow(missing_docs)]
pub mod error;
pub mod events;
#[allow(missing_docs)]
pub mod fork_resolution;
pub mod freeze;
pub mod hash;
pub mod ledger;
pub mod membership;
pub mod merge;
pub mod quarantine;
#[allow(missing_docs)]
pub mod sync;
pub mod treasury;
pub mod types;

pub use credit_policy::{CreditPolicy, CreditPolicyManager, NewMemberPolicy};
pub use dispute::DisputeManager;
pub use error::{LedgerError, Result};
pub use fork_resolution::{
    Fork, ForkDetector, ForkResolution, ForkResolutionStrategy, ForkResolver,
};
pub use freeze::{FreezeManager, FrozenMember, UnfreezeEvent};
pub use ledger::Ledger;
pub use membership::{MembershipStore, SledMembershipStore};
pub use merge::{ConflictPair, MergeDecision, QuarantineItem};
pub use quarantine::QuarantineStore;
pub use sync::{deserialize_sync_message, ledger_topic, serialize_sync_message, LedgerSyncMessage};
pub use treasury::{
    ApprovalType, BudgetStatus, PaginatedAuditTrail, SpendingRule, Treasury, TreasuryAuditRecord,
    TreasuryBudget, TreasuryManager, TreasuryOperation,
};
pub use types::{
    AccountBalances, AccountDelta, ContentHash, CreditLimit, Currency, Dispute, DisputeOutcome,
    DisputeStatus, JournalEntry, QuarantineReason, QuarantinedEntry, Resolution, Signature,
};

// Event types for real-time notifications
pub use events::{
    create_shared_emitter, BalanceChanged, BatchBalanceChanged, ForkDetected, ForkResolved,
    LedgerEvent, LedgerEventEmitter, MemberFrozen, MemberUnfrozen, RollbackPerformed,
    SharedEventEmitter, TransactionConfirmed, TransactionCreated, Transfer,
};
