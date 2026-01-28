//! ICN Obs - Observability (logging, metrics, tracing)
//!
//! # Metric Cardinality Guidelines (Issue #494)
//!
//! High-cardinality metrics can cause serious performance and memory issues in
//! production environments. This crate enforces strict cardinality limits.
//!
//! ## What is High Cardinality?
//!
//! A metric label has high cardinality when it can take on an unbounded or very
//! large number of distinct values. Examples:
//!
//! - **High cardinality (avoid)**: DIDs, peer IDs, task hashes, timestamps
//! - **Low cardinality (preferred)**: Trust classes (4 values), error types,
//!   org types, severity levels
//!
//! ## Rules for Metric Labels
//!
//! 1. **Never use unbounded identifiers as labels**:
//!    - No `did`, `peer_did`, `account_id`, `task_hash`, etc.
//!    - Use aggregated dimensions instead (trust_class, error_type)
//!
//! 2. **Bounded enums are acceptable**:
//!    - Trust classes: `isolated`, `known`, `partner`, `federated`
//!    - Error types: `timeout`, `rejected`, `invalid_signature`
//!    - Severity levels: `1`, `2`, `3`, etc.
//!
//! 3. **For audit/debugging of specific entities**:
//!    - Use structured logs instead of metrics
//!    - Metrics show aggregate health; logs show specific events
//!
//! 4. **Treasury DIDs are an exception**:
//!    - Treasury DIDs are bounded (one per coop, small count)
//!    - But prefer aggregate metrics when possible
//!
//! ## Examples
//!
//! ```rust,ignore
//! // BAD: Unbounded cardinality - creates one series per DID
//! counter!("icn_violations_total", "did" => did.to_string());
//!
//! // GOOD: Bounded cardinality - only 4 possible values
//! counter!("icn_violations_total", "trust_class" => trust_class);
//!
//! // GOOD: Aggregate without labels
//! counter!("icn_violations_total").increment(1);
//! // Log the specific DID for audit purposes
//! tracing::warn!(did = %did, "Violation detected");
//! ```
//!
//! ## Deprecated Functions
//!
//! Several functions in this crate have been deprecated because they used
//! high-cardinality labels. These functions now ignore their identifier
//! parameters and emit aggregate metrics instead. See individual function
//! docs for recommended replacements.
#![allow(missing_docs)]
// Allow unwrap/expect in test code - panics are acceptable for tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

/// Contribution attestation system
pub mod attestation;
/// Contribution metrics aggregation
pub mod contribution;
/// Health monitoring and status reporting
pub mod health;
/// Prometheus metrics collection
pub mod metrics;
/// Distributed tracing with OpenTelemetry
pub mod otel;

use anyhow::{Context, Result};
use metrics_exporter_prometheus::PrometheusBuilder;
use std::net::SocketAddr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub use attestation::{
    check_eligibility, AttestationCountLookup, AttestationGraphLookup, AttestersOfLookup,
    ClaimHistoryLookup, ClaimStatus, ContributionAttestation, ContributionClaim,
    ContributionMessage, ContributionValidator, EligibilityContext, EligibilityStatus,
    FraudDetector, FraudIndicator, MembershipAgeLookup, NetworkObservationsLookup, PeerAttestation,
    TrustLookup, ValidationResult, CONTRIBUTION_THRESHOLD, MAX_ATTESTATIONS_PER_PERIOD,
    MIN_MEMBERSHIP_AGE_SECS, MIN_TRUST_TO_ATTEST, ORG_ATTESTATION_THRESHOLD,
    TOPIC_CONTRIBUTION_ATTESTATION, TOPIC_CONTRIBUTION_CLAIM, TOPIC_CONTRIBUTION_VERIFIED,
};
pub use contribution::{AggregatedMetrics, ResourceMetrics, ResourceType};
pub use health::{start_monitoring_server, HealthService, HealthState, HealthStatus};
pub use otel::{init_tracing, shutdown_tracing, TraceContext, TracingConfig};

/// Initialize observability stack
pub fn init() -> Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    Ok(())
}

/// Initialize metrics system and descriptions
pub fn init_metrics() -> Result<()> {
    // Initialize legacy metrics descriptions
    metrics::init_descriptions();
    // Initialize new submodule metrics descriptions
    metrics::action_items::init_descriptions();
    metrics::agreement::init_descriptions();
    metrics::exchange::init_descriptions();
    metrics::gateway::init_descriptions();
    metrics::governance::init_descriptions();
    metrics::ledger::init_descriptions();
    metrics::nat::init_descriptions();
    metrics::network::init_descriptions();
    metrics::rpc::init_descriptions();
    metrics::storage::init_descriptions();
    tracing::info!("Metrics descriptions initialized");
    Ok(())
}

/// Start Prometheus metrics exporter on the given port
///
/// Returns a handle that can be used to stop the server
pub async fn start_metrics_server(port: u16) -> Result<()> {
    let addr: SocketAddr = format!("0.0.0.0:{port}")
        .parse()
        .context("Failed to parse metrics address")?;

    tracing::info!("Starting Prometheus metrics server on http://{}", addr);

    // Build and install the Prometheus exporter
    let builder = PrometheusBuilder::new()
        .set_buckets_for_metric(
            metrics_exporter_prometheus::Matcher::Full(
                "trust_oracle_evaluation_duration_seconds".to_string(),
            ),
            &[0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0],
        )
        .context("Failed to set buckets")?;
    builder
        .with_http_listener(addr)
        .install()
        .context("Failed to install Prometheus exporter")?;

    tracing::info!("Prometheus metrics available at http://{}/metrics", addr);
    Ok(())
}
