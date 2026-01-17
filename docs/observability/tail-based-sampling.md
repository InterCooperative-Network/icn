# Tail-Based Sampling Configuration Guide

This guide explains how to configure tail-based sampling for ICN traces using external collectors.

## Overview

ICN supports two complementary sampling strategies:

| Strategy | When Decision Made | Implemented By | Best For |
|----------|-------------------|----------------|----------|
| **Head-based** | At span creation | ICN daemon | Reducing trace volume, always capturing security spans |
| **Tail-based** | After span completion | Collector | Capturing errors, slow requests, and anomalies |

## Head-Based Sampling in ICN

ICN's `PrioritySampler` makes sampling decisions at span creation time:

```toml
[tracing]
enabled = true
otlp_endpoint = "http://tempo:4317"
sampling_rate = 0.1  # 10% of normal traces
```

**Priority spans always sampled** (regardless of `sampling_rate`):
- Security-related spans (`security.*`, `auth.*`, `permission.*`)
- Trust computation spans (`trust.*`)
- Cryptographic operations (`crypto.*`, `signature.*`)

## Why Tail-Based Sampling?

Head-based sampling cannot capture:

1. **Errors** - The span hasn't completed yet, so we don't know if it will error
2. **Slow requests** - Duration is unknown at span creation
3. **Anomalies** - Pattern detection requires seeing the complete trace

The `TracingConfig` includes intent flags for these scenarios:

```toml
[tracing]
always_sample_errors = true   # Capture all error traces
always_sample_slow = true     # Capture slow request traces
slow_threshold_ms = 1000      # Define "slow" as > 1 second
```

These flags are **not enforced by ICN** - they document intent for collector-side configuration.

## Collector Configuration

### Strategy: Full Capture + Collector Filtering

For critical environments, capture 100% of traces and let the collector filter:

```toml
# ICN config: send everything to collector
[tracing]
sampling_rate = 1.0           # 100% to collector
always_sample_errors = true
always_sample_slow = true
slow_threshold_ms = 1000
```

Then configure tail-based sampling in the collector.

### Grafana Tempo

Tempo stores all received traces and provides filtering via TraceQL queries. For true tail-based sampling (dropping traces before storage), use the OTEL Collector as a preprocessor in front of Tempo.

**tempo.yaml:**
```yaml
# Tempo receives all traces and stores them
distributor:
  receivers:
    otlp:
      protocols:
        grpc:
          # Binds to all interfaces - use specific IP in production
          endpoint: 0.0.0.0:4317

# Metrics generator for trace-derived metrics
overrides:
  defaults:
    metrics_generator:
      processors: [span-metrics, service-graphs]

storage:
  trace:
    backend: local
    local:
      path: /var/tempo/traces
```

**Querying for errors and slow traces** (TraceQL in Grafana):
```
# Find error traces
{ status = error }

# Find slow traces (> 1s)
{ duration > 1s }

# Find security-related traces (ICN sets sampling.priority attribute)
{ resource.sampling.priority = "security" }
```

> **Note**: Tempo stores all traces it receives. The TraceQL queries shown above are for **retrieval** only, not filtering during ingestion. To reduce storage costs, place an OTEL Collector with tail sampling in front of Tempo (see configuration below).

### OpenTelemetry Collector

The OTEL Collector provides the most flexible tail-based sampling.

**otel-collector.yaml:**
```yaml
receivers:
  otlp:
    protocols:
      grpc:
        # Binds to all interfaces - use specific IP in production
        endpoint: 0.0.0.0:4317

processors:
  # Tail-based sampling processor
  tail_sampling:
    # Wait for trace spans to arrive before making sampling decision.
    # Increase if you have long-running operations or high network latency.
    decision_wait: 10s
    num_traces: 100000
    expected_new_traces_per_sec: 1000
    policies:
      # Always sample errors
      - name: error-policy
        type: status_code
        status_code:
          status_codes: [ERROR]

      # Always sample slow requests (> 1s)
      - name: latency-policy
        type: latency
        latency:
          threshold_ms: 1000

      # Always sample security spans
      # ICN sets sampling.priority="security" attribute on priority spans
      - name: security-policy
        type: string_attribute
        string_attribute:
          key: sampling.priority
          values: ["security"]

      # Probabilistic sampling for everything else
      - name: probabilistic-policy
        type: probabilistic
        probabilistic:
          sampling_percentage: 10

exporters:
  otlp:
    endpoint: tempo:4317
    tls:
      # Set to false in production with proper TLS certificates
      insecure: true

service:
  pipelines:
    traces:
      receivers: [otlp]
      processors: [tail_sampling]
      exporters: [otlp]
```

### Jaeger

Jaeger uses a remote sampling configuration:

**jaeger-sampling.json:**
```json
{
  "service_strategies": [
    {
      "service": "icnd",
      "type": "probabilistic",
      "param": 0.1,
      "operation_strategies": [
        {
          "operation": "security.*",
          "type": "probabilistic",
          "param": 1.0
        }
      ]
    }
  ],
  "default_strategy": {
    "type": "probabilistic",
    "param": 0.1
  }
}
```

For tail-based sampling with Jaeger, use the OTEL Collector as an intermediary.

## Recommended Configurations

### Development

Capture everything for debugging:

```toml
[tracing]
enabled = true
sampling_rate = 1.0
always_sample_errors = true
always_sample_slow = true
# Lower threshold in dev to catch moderately slow operations during debugging
slow_threshold_ms = 500
```

### Production (Low Volume)

Use head-based sampling only:

```toml
[tracing]
enabled = true
sampling_rate = 0.1  # 10%
always_sample_errors = true
always_sample_slow = true
slow_threshold_ms = 1000
```

### Production (High Volume with Collector)

Full capture with collector-side filtering:

```toml
[tracing]
enabled = true
sampling_rate = 1.0  # Send everything
always_sample_errors = true
always_sample_slow = true
slow_threshold_ms = 1000
```

Use OTEL Collector tail-based sampling (see configuration above).

> **⚠️ Performance Warning**: Sending 100% of traces (`sampling_rate = 1.0`) generates significant network traffic. A node producing 10K spans/sec at ~1KB each results in ~10MB/sec egress. Ensure adequate network capacity and collector resources before enabling full capture.

## Verifying Configuration

### Check Trace Sampling

```bash
# Query for sampled traces in the last hour
curl -G 'http://tempo:3200/api/search' \
  --data-urlencode 'tags=service.name=icnd' \
  --data-urlencode 'minDuration=1s'
```

### Check Error Capture

```bash
# Verify error traces are captured
curl -G 'http://tempo:3200/api/search' \
  --data-urlencode 'tags=status=error'
```

### Monitor Sampling Rates

The OTEL Collector exposes metrics:

- `otelcol_processor_tail_sampling_count_traces_sampled`
- `otelcol_processor_tail_sampling_count_traces_dropped`

## Troubleshooting

### Missing Error Traces

1. Verify `always_sample_errors = true` in ICN config
2. Check collector tail-sampling policy includes error status
3. Ensure `decision_wait` is long enough for traces to complete

### Missing Slow Traces

1. Verify `slow_threshold_ms` matches your latency SLO
2. Check collector latency policy threshold matches ICN config
3. Increase `decision_wait` for long-running operations

### High Memory Usage in Collector

Tail-based sampling buffers traces in memory:

1. Reduce `num_traces` buffer size
2. Decrease `decision_wait` time
3. Use probabilistic pre-filtering before tail sampling

## Related Documentation

- [Production Hardening Guide](../production-hardening.md) - Security configuration
- [ICN Observability](../ARCHITECTURE.md#observability) - Architecture overview
- [OpenTelemetry Tail Sampling Processor](https://github.com/open-telemetry/opentelemetry-collector-contrib/tree/main/processor/tailsamplingprocessor)
