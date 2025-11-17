//! ICN Obs - Observability (logging, metrics, tracing)

pub mod health;
pub mod metrics;

use anyhow::{Context, Result};
use metrics_exporter_prometheus::PrometheusBuilder;
use std::net::SocketAddr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub use health::{HealthService, HealthState, HealthStatus, start_monitoring_server};

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
