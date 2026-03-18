//! Ledger app — escrow and budget state stores.
//!
//! This crate moves domain-specific ledger storage out of the kernel supervisor
//! and into the app layer, following the same pattern as `apps/governance`.

pub mod budget;
pub mod escrow;
pub mod init;
