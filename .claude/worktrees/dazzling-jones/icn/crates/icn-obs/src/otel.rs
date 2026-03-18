//! Distributed tracing with OpenTelemetry
//!
//! This module provides OpenTelemetry integration for distributed tracing across
//! ICN nodes. It exports spans to an OTLP-compatible collector (e.g., Grafana Tempo).

use anyhow::{Context, Result};
use opentelemetry::trace::{
    SamplingDecision, SamplingResult, SpanKind, TraceId, TracerProvider as _,
};
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::{RandomIdGenerator, SdkTracerProvider, ShouldSample};
use opentelemetry_sdk::Resource;
use serde::{Deserialize, Serialize};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// Configuration for distributed tracing
///
/// # Priority Sampling
///
/// ICN uses priority-based sampling to reduce overhead while capturing important traces:
///
/// ## Implemented (Head-Based)
///
/// - **Security events**: Always sampled (spans with "security" in name)
/// - **Normal operations**: Sampled at `sampling_rate`
///
/// ## Requires Collector-Side Configuration (Tail-Based)
///
/// Error and slow request sampling require tail-based sampling decisions (after span
/// completion). These config fields are reserved for:
///
/// 1. **Collector policy generation**: Export config for Tempo/Jaeger tail-based policies
/// 2. **Full capture mode**: Set `sampling_rate = 1.0` and filter at the collector
/// 3. **Future integration**: Native tail-based sampling support
///
/// - `always_sample_errors` - Reserved for error-based tail sampling
/// - `always_sample_slow` - Reserved for latency-based tail sampling
/// - `slow_threshold_ms` - Threshold for slow request detection
///
/// # Example Configuration
///
/// ```toml
/// [tracing]
/// enabled = true
/// otlp_endpoint = "http://tempo:4317"
/// sampling_rate = 0.1          # 10% of normal traces
/// always_sample_errors = true  # Reserved for collector-side error sampling
/// always_sample_slow = true    # Reserved for collector-side latency sampling
/// slow_threshold_ms = 1000     # Threshold for "slow" classification
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    /// Whether tracing is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// OTLP endpoint for trace export (e.g., "http://localhost:4317")
    #[serde(default = "default_otlp_endpoint")]
    pub otlp_endpoint: String,

    /// Base sampling rate (0.0 to 1.0) for normal operations.
    /// Default is 0.1 (10%) for production.
    ///
    /// Security spans (names containing "security") are always sampled
    /// regardless of this rate.
    #[serde(default = "default_sampling_rate")]
    pub sampling_rate: f64,

    /// Reserved for collector-side error sampling configuration.
    ///
    /// This field does NOT enable head-based error sampling (which is impossible
    /// since errors are unknown at span start). Instead, use this to:
    /// - Generate collector tail-sampling policies
    /// - Document intent for full-capture + collector filtering setups
    #[serde(default = "default_always_sample_errors")]
    pub always_sample_errors: bool,

    /// Reserved for collector-side latency sampling configuration.
    ///
    /// This field does NOT enable head-based slow request sampling (which is
    /// impossible since duration is unknown at span start). Instead, use this to:
    /// - Generate collector tail-sampling policies
    /// - Document intent for full-capture + collector filtering setups
    #[serde(default = "default_always_sample_slow")]
    pub always_sample_slow: bool,

    /// Threshold (in milliseconds) for classifying a request as "slow".
    ///
    /// Used with collector-side tail-based sampling policies. Has no effect
    /// on head-based sampling decisions.
    #[serde(default = "default_slow_threshold_ms")]
    pub slow_threshold_ms: u64,

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
    // Check standard OpenTelemetry environment variable first
    std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_string())
}

fn default_sampling_rate() -> f64 {
    0.1 // 10% sampling for production
}

fn default_always_sample_errors() -> bool {
    true // Always capture errors by default
}

fn default_always_sample_slow() -> bool {
    true // Always capture slow requests by default
}

fn default_slow_threshold_ms() -> u64 {
    1000 // 1 second
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
            always_sample_errors: default_always_sample_errors(),
            always_sample_slow: default_always_sample_slow(),
            slow_threshold_ms: default_slow_threshold_ms(),
            service_name: default_service_name(),
            node_did: None,
        }
    }
}

/// Priority sampler for ICN distributed tracing
///
/// This sampler implements head-based priority sampling:
/// - **Security spans**: Always sampled (span names containing "security")
/// - **Other spans**: Sampled according to `sampling_rate`
///
/// # Limitations (Head-Based Sampling)
///
/// Error and slow request sampling are not implemented here because they require
/// tail-based sampling (decision after span completion). For those features:
///
/// 1. **Collector-side filtering**: Configure your OTLP collector (e.g., Grafana Tempo,
///    Jaeger) to use tail-based sampling policies
/// 2. **Full sampling**: Set `sampling_rate = 1.0` to capture all traces, then use
///    collector-side filtering for storage
///
/// The `always_sample_errors` and `always_sample_slow` config fields are preserved
/// for future tail-based sampling integration or collector policy generation.
#[derive(Debug, Clone)]
pub struct PrioritySampler {
    /// Base sampling rate for normal traces (0.0 to 1.0)
    sampling_rate: f64,
    /// Pattern to match for always-sample spans (e.g., "security")
    priority_pattern: &'static str,
}

impl PrioritySampler {
    /// Create a new priority sampler with the given base sampling rate
    ///
    /// Security-related spans (containing "security" in their name) are always sampled.
    pub fn new(sampling_rate: f64) -> Self {
        Self {
            sampling_rate: sampling_rate.clamp(0.0, 1.0),
            priority_pattern: "security",
        }
    }

    /// Check if this span should be prioritized (always sampled)
    fn is_priority_span(&self, name: &str) -> bool {
        name.to_lowercase().contains(self.priority_pattern)
    }

    /// Make sampling decision based on trace ID (consistent hashing)
    fn sample_by_trace_id(&self, trace_id: TraceId) -> bool {
        if self.sampling_rate >= 1.0 {
            return true;
        }
        if self.sampling_rate <= 0.0 {
            return false;
        }

        // Use trace ID ratio-based sampling (same algorithm as TraceIdRatioBased)
        // This ensures consistent sampling decisions for the same trace ID
        let trace_id_bytes = trace_id.to_bytes();
        let trace_id_high = u64::from_be_bytes([
            trace_id_bytes[0],
            trace_id_bytes[1],
            trace_id_bytes[2],
            trace_id_bytes[3],
            trace_id_bytes[4],
            trace_id_bytes[5],
            trace_id_bytes[6],
            trace_id_bytes[7],
        ]);

        // Compare against threshold derived from sampling rate
        let threshold = (self.sampling_rate * u64::MAX as f64) as u64;
        trace_id_high < threshold
    }
}

impl ShouldSample for PrioritySampler {
    fn should_sample(
        &self,
        parent_context: Option<&opentelemetry::Context>,
        trace_id: TraceId,
        name: &str,
        _span_kind: &SpanKind,
        _attributes: &[KeyValue],
        _links: &[opentelemetry::trace::Link],
    ) -> SamplingResult {
        // If parent context exists and is sampled, respect parent decision
        if let Some(ctx) = parent_context {
            use opentelemetry::trace::TraceContextExt;
            let parent_span = ctx.span();
            let span_context = parent_span.span_context();
            if span_context.is_valid() && span_context.is_sampled() {
                return SamplingResult {
                    decision: SamplingDecision::RecordAndSample,
                    attributes: vec![],
                    trace_state: span_context.trace_state().clone(),
                };
            }
        }

        // Priority spans are always sampled
        if self.is_priority_span(name) {
            return SamplingResult {
                decision: SamplingDecision::RecordAndSample,
                attributes: vec![KeyValue::new("sampling.priority", "security")],
                trace_state: Default::default(),
            };
        }

        // Use trace ID ratio-based sampling for other spans
        let decision = if self.sample_by_trace_id(trace_id) {
            SamplingDecision::RecordAndSample
        } else {
            SamplingDecision::Drop
        };

        SamplingResult {
            decision,
            attributes: vec![],
            trace_state: Default::default(),
        }
    }
}

/// Global tracer provider handle for shutdown
static TRACER_PROVIDER: std::sync::OnceLock<SdkTracerProvider> = std::sync::OnceLock::new();

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

    let resource = Resource::builder().with_attributes(resource_attrs).build();

    // Configure the OTLP exporter with tonic transport
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&config.otlp_endpoint)
        .build()
        .context("Failed to create OTLP exporter")?;

    // Build the tracer provider with priority-based sampling
    // Security spans are always sampled, other spans use the configured rate
    let sampler = PrioritySampler::new(config.sampling_rate);

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
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
        always_sample_errors = config.always_sample_errors,
        always_sample_slow = config.always_sample_slow,
        slow_threshold_ms = config.slow_threshold_ms,
        service_name = %config.service_name,
        "Distributed tracing initialized with priority sampling (security spans always sampled)"
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
        // Save and clear env var to test true default
        let saved = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
        std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");

        let config = TracingConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.otlp_endpoint, "http://localhost:4317");
        assert!((config.sampling_rate - 0.1).abs() < f64::EPSILON);
        assert_eq!(config.service_name, "icnd");
        assert!(config.node_did.is_none());

        // Restore env var if it was set
        if let Some(val) = saved {
            std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", val);
        }
    }

    #[test]
    fn test_tracing_config_respects_env_var() {
        // Save current value
        let saved = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();

        // Set env var and verify it's used
        std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "http://tempo:4317");
        let config = TracingConfig::default();
        assert_eq!(config.otlp_endpoint, "http://tempo:4317");

        // Restore env var
        match saved {
            Some(val) => std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", val),
            None => std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT"),
        }
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

    #[test]
    fn test_tracing_config_priority_sampling_defaults() {
        // Save and clear env var to test true default
        let saved = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
        std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");

        let config = TracingConfig::default();

        // Priority sampling should be enabled by default
        assert!(config.always_sample_errors);
        assert!(config.always_sample_slow);
        assert_eq!(config.slow_threshold_ms, 1000); // 1 second default

        // Restore env var if it was set
        if let Some(val) = saved {
            std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", val);
        }
    }

    #[test]
    fn test_tracing_config_priority_sampling_serde() {
        let json = r#"{
            "enabled": true,
            "sampling_rate": 0.1,
            "always_sample_errors": false,
            "always_sample_slow": true,
            "slow_threshold_ms": 500
        }"#;

        let config: TracingConfig = serde_json::from_str(json).expect("deserialize");
        assert!(!config.always_sample_errors);
        assert!(config.always_sample_slow);
        assert_eq!(config.slow_threshold_ms, 500);
    }

    #[test]
    fn test_priority_sampler_always_samples_security_spans() {
        let sampler = PrioritySampler::new(0.0); // 0% base rate

        // Security spans should always be sampled
        assert!(sampler.is_priority_span("security_check"));
        assert!(sampler.is_priority_span("verify_security"));
        assert!(sampler.is_priority_span("SECURITY_AUDIT"));
        assert!(sampler.is_priority_span("check_security_policy"));

        // Non-security spans should not be prioritized
        assert!(!sampler.is_priority_span("normal_operation"));
        assert!(!sampler.is_priority_span("ledger_append"));
        assert!(!sampler.is_priority_span("gossip_message"));
    }

    #[test]
    fn test_priority_sampler_sampling_rate_bounds() {
        // Test that sampling rate is clamped to [0.0, 1.0]
        let sampler_high = PrioritySampler::new(2.0);
        assert!((sampler_high.sampling_rate - 1.0).abs() < f64::EPSILON);

        let sampler_low = PrioritySampler::new(-0.5);
        assert!(sampler_low.sampling_rate.abs() < f64::EPSILON);

        let sampler_normal = PrioritySampler::new(0.5);
        assert!((sampler_normal.sampling_rate - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_priority_sampler_trace_id_sampling() {
        let sampler = PrioritySampler::new(0.5);

        // Test with known trace IDs to verify sampling behavior
        // TraceId with high bytes = 0 should always be sampled at 50%
        let trace_id_low = TraceId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        assert!(sampler.sample_by_trace_id(trace_id_low));

        // TraceId with high bytes = max should never be sampled at 50%
        let trace_id_high = TraceId::from_bytes([
            255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 1,
        ]);
        assert!(!sampler.sample_by_trace_id(trace_id_high));
    }

    #[test]
    fn test_priority_sampler_always_on_and_off() {
        // 100% sampling rate should always sample
        let sampler_on = PrioritySampler::new(1.0);
        let trace_id_high = TraceId::from_bytes([
            255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 1,
        ]);
        assert!(sampler_on.sample_by_trace_id(trace_id_high));

        // 0% sampling rate should never sample
        let sampler_off = PrioritySampler::new(0.0);
        let trace_id_low = TraceId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        assert!(!sampler_off.sample_by_trace_id(trace_id_low));
    }

    #[test]
    fn test_should_sample_integration() {
        use opentelemetry::trace::{SamplingDecision, SpanKind};

        let sampler = PrioritySampler::new(0.5); // 50% sampling

        // Trace ID with high bytes = 0 will be sampled at 50% (0 < threshold)
        let sampled_trace_id =
            TraceId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        // Trace ID with high bytes = MAX will NOT be sampled at 50% (MAX >= threshold)
        let not_sampled_trace_id = TraceId::from_bytes([
            255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 1,
        ]);

        // Test 1: Security span is always sampled (even with high trace ID)
        let result = sampler.should_sample(
            None,
            not_sampled_trace_id,
            "handle_security_event",
            &SpanKind::Internal,
            &[],
            &[],
        );
        assert_eq!(result.decision, SamplingDecision::RecordAndSample);
        assert!(result
            .attributes
            .iter()
            .any(|kv| kv.key.as_str() == "sampling.priority"));

        // Test 2: Normal span with high trace ID is NOT sampled
        let result = sampler.should_sample(
            None,
            not_sampled_trace_id,
            "process_message",
            &SpanKind::Internal,
            &[],
            &[],
        );
        assert_eq!(result.decision, SamplingDecision::Drop);

        // Test 3: Normal span with low trace ID IS sampled
        let result = sampler.should_sample(
            None,
            sampled_trace_id,
            "process_message",
            &SpanKind::Internal,
            &[],
            &[],
        );
        assert_eq!(result.decision, SamplingDecision::RecordAndSample);
    }
}
