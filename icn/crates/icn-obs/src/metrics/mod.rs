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

// Include the legacy metrics file directly
// This keeps backwards compatibility while allowing gradual migration
// Allow missing docs and dead_code for legacy metrics - being migrated to submodules
#[allow(missing_docs, dead_code)]
mod legacy {
    include!("../metrics_legacy.rs");
}

pub use legacy::*;

pub mod action_items;
pub mod agreement;
pub mod exchange;
pub mod gateway;
pub mod governance;
pub mod ledger;
pub mod nat;
pub mod network;
pub mod rpc;
pub mod storage;
