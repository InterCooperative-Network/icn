//! Distributed tracing with OpenTelemetry
//!
//! This module provides OpenTelemetry integration for distributed tracing across
//! ICN nodes. It exports spans to an OTLP-compatible collector (e.g., Grafana Tempo).

use anyhow::{Context, Result};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::{RandomIdGenerator, Sampler, TracerProvider};
use opentelemetry_sdk::{runtime, Resource};
use serde::{Deserialize, Serialize};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// Configuration for distributed tracing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    /// Whether tracing is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// OTLP endpoint for trace export (e.g., "http://localhost:4317")
    #[serde(default = "default_otlp_endpoint")]
    pub otlp_endpoint: String,

    /// Sampling rate (0.0 to 1.0). Default is 0.1 (10%) for production.
    #[serde(default = "default_sampling_rate")]
    pub sampling_rate: f64,

    /// Service name for traces
    #[serde(default = "default_service_name")]
    pub service_name: String,

    /// Optional node DID for trace attribution
    #[serde(default)]
    pub node_did: Option<String>,
}

fn default_enabled() -> bool {
    false
}

fn default_otlp_endpoint() -> String {
    "http://localhost:4317".to_string()
}

fn default_sampling_rate() -> f64 {
    0.1 // 10% sampling for production
}

fn default_service_name() -> String {
    "icnd".to_string()
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            otlp_endpoint: default_otlp_endpoint(),
            sampling_rate: default_sampling_rate(),
            service_name: default_service_name(),
            node_did: None,
        }
    }
}

/// Global tracer provider handle for shutdown
static TRACER_PROVIDER: std::sync::OnceLock<TracerProvider> = std::sync::OnceLock::new();

/// Initialize distributed tracing with OpenTelemetry
///
/// This sets up a tracing subscriber that:
/// - Exports spans to an OTLP endpoint (e.g., Grafana Tempo)
/// - Uses the configured sampling rate
/// - Includes service metadata as resource attributes
///
/// Call `shutdown_tracing()` on application exit to flush pending spans.
pub fn init_tracing(config: &TracingConfig) -> Result<()> {
    if !config.enabled {
        // Fall back to basic tracing without OpenTelemetry
        tracing_subscriber::registry()
            .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
            .with(tracing_subscriber::fmt::layer())
            .init();

        tracing::info!("Distributed tracing disabled, using local logging only");
        return Ok(());
    }

    // Build resource attributes for trace metadata
    let mut resource_attrs = vec![
        KeyValue::new("service.name", config.service_name.clone()),
        KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
    ];

    if let Some(ref did) = config.node_did {
        resource_attrs.push(KeyValue::new("node.did", did.clone()));
    }

    let resource = Resource::new(resource_attrs);

    // Configure the OTLP exporter
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&config.otlp_endpoint)
        .build()
        .context("Failed to create OTLP exporter")?;

    // Build the tracer provider with sampling
    let sampler = if config.sampling_rate >= 1.0 {
        Sampler::AlwaysOn
    } else if config.sampling_rate <= 0.0 {
        Sampler::AlwaysOff
    } else {
        Sampler::TraceIdRatioBased(config.sampling_rate)
    };

    let provider = TracerProvider::builder()
        .with_batch_exporter(exporter, runtime::Tokio)
        .with_sampler(sampler)
        .with_id_generator(RandomIdGenerator::default())
        .with_resource(resource)
        .build();

    // Store for shutdown
    let _ = TRACER_PROVIDER.set(provider.clone());

    // Create tracer and OpenTelemetry layer
    let tracer = provider.tracer(config.service_name.clone());
    let otel_layer = OpenTelemetryLayer::new(tracer);

    // Initialize the subscriber with both OpenTelemetry and fmt layers
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(otel_layer)
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!(
        endpoint = %config.otlp_endpoint,
        sampling_rate = config.sampling_rate,
        service_name = %config.service_name,
        "Distributed tracing initialized with OpenTelemetry"
    );

    Ok(())
}

/// Shutdown the tracing system, flushing any pending spans
///
/// This should be called during graceful shutdown to ensure all
/// spans are exported before the application exits.
pub fn shutdown_tracing() {
    if let Some(provider) = TRACER_PROVIDER.get() {
        tracing::info!("Shutting down distributed tracing, flushing pending spans...");
        if let Err(e) = provider.shutdown() {
            tracing::warn!("Error during tracing shutdown: {:?}", e);
        }
    }
}

/// W3C Trace Context for propagation across network boundaries
///
/// This structure holds the trace context that can be serialized
/// and passed through network messages for distributed tracing.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TraceContext {
    /// W3C traceparent header value (e.g., "00-trace_id-span_id-flags")
    pub traceparent: Option<String>,
    /// W3C tracestate header value for vendor-specific data
    pub tracestate: Option<String>,
}

impl TraceContext {
    /// Create a new trace context from the current span
    pub fn from_current() -> Self {
        use opentelemetry::trace::TraceContextExt;
        use tracing_opentelemetry::OpenTelemetrySpanExt;

        let span = tracing::Span::current();
        let context = span.context();
        let otel_span = context.span();
        let span_context = otel_span.span_context();

        if span_context.is_valid() {
            let trace_id = span_context.trace_id().to_string();
            let span_id = span_context.span_id().to_string();
            let flags = if span_context.is_sampled() {
                "01"
            } else {
                "00"
            };

            Self {
                traceparent: Some(format!("00-{trace_id}-{span_id}-{flags}")),
                tracestate: None,
            }
        } else {
            Self::default()
        }
    }

    /// Check if this trace context is valid
    pub fn is_valid(&self) -> bool {
        self.traceparent.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracing_config_defaults() {
        let config = TracingConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.otlp_endpoint, "http://localhost:4317");
        assert!((config.sampling_rate - 0.1).abs() < f64::EPSILON);
        assert_eq!(config.service_name, "icnd");
        assert!(config.node_did.is_none());
    }

    #[test]
    fn test_trace_context_default() {
        let ctx = TraceContext::default();
        assert!(!ctx.is_valid());
        assert!(ctx.traceparent.is_none());
    }

    #[test]
    fn test_tracing_config_serde() {
        let json = r#"{
            "enabled": true,
            "otlp_endpoint": "http://tempo:4317",
            "sampling_rate": 0.5,
            "service_name": "test-node",
            "node_did": "did:icn:test123"
        }"#;

        let config: TracingConfig = serde_json::from_str(json).expect("deserialize");
        assert!(config.enabled);
        assert_eq!(config.otlp_endpoint, "http://tempo:4317");
        assert!((config.sampling_rate - 0.5).abs() < f64::EPSILON);
        assert_eq!(config.service_name, "test-node");
        assert_eq!(config.node_did.as_deref(), Some("did:icn:test123"));
    }
}
