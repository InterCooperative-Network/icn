//! ICN Obs - Observability (logging, metrics, tracing)

use anyhow::Result;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize observability stack
pub fn init() -> Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    Ok(())
}

/// Start metrics exporter on the given port
pub async fn start_metrics_server(port: u16) -> Result<()> {
    // Stub - will implement Prometheus exporter
    tracing::info!("Metrics server would start on port {}", port);
    Ok(())
}
