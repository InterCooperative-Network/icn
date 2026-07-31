//! ICN Metrics
//!
//! This module provides Prometheus-compatible metrics for all ICN components.
//! Metrics are organized by subsystem (network, gossip, ledger, etc.).
//!
//! # Usage
//!
//! ```ignore
//! use icn_obs::metrics;
//!
//! // Initialize all metric descriptions
//! metrics::init_descriptions();
//!
//! // Use metrics
//! metrics::network::connections_total_inc();
//! metrics::gossip::messages_published_inc();
//! metrics::gateway::websocket_connections_total_inc();
//! ```

// Declare the legacy metrics file as a module via #[path] rather than include!().
// This keeps backwards compatibility while allowing gradual migration, and unlike
// include!() it is traversed by rustfmt, so the file stays under `cargo fmt --all`.
// Allow missing docs and dead_code for legacy metrics - being migrated to submodules
#[allow(missing_docs, dead_code)]
#[path = "../metrics_legacy.rs"]
mod legacy;

pub use legacy::*;

pub mod action_items;
pub mod agreement;
pub mod apps;
pub mod exchange;
pub mod gateway;
pub mod governance;
pub mod ledger;
pub mod nat;
pub mod network;
pub mod resource_enforcer;
pub mod rpc;
pub mod service_discovery;
pub mod storage;
pub mod trust;
