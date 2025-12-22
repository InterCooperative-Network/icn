//! ICN Obs - Observability (logging, metrics, tracing)
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
    metrics::init_descriptions();
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
    let builder = PrometheusBuilder::new();
    builder
        .with_http_listener(addr)
        .install()
        .context("Failed to install Prometheus exporter")?;

    tracing::info!("Prometheus metrics available at http://{}/metrics", addr);
    Ok(())
}
