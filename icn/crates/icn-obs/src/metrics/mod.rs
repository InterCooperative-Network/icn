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
//! ```

// Include the legacy metrics file directly
// This keeps backwards compatibility while allowing gradual migration
include!("../metrics_legacy.rs");
