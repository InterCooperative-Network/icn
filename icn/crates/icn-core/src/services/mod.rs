//! Service implementations for kernel-api traits.
//!
//! This module provides concrete implementations of `icn-kernel-api` service
//! traits. These adapters bridge the kernel-safe API boundary to actual
//! domain implementations (ledger, trust, etc.).
//!
//! # Architecture
//!
//! - `icn-kernel-api`: Defines traits + DTOs (kernel-safe boundary)
//! - `icn-ledger`, `icn-trust`, etc.: Domain implementations
//! - `icn-core::services`: Adapters that implement kernel-api traits using domain crates
//!
//! This is the "composition root" where real implementations are wired into
//! kernel-safe traits.

mod federation_service;
mod ledger_service;

pub use federation_service::FederationServiceImpl;
pub use ledger_service::LedgerServiceImpl;
