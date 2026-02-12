# Distributed Tracing with OpenTelemetry

ICN supports distributed tracing using OpenTelemetry, enabling end-to-end visibility into requests as they flow through the P2P network. This document covers configuration, integration with Grafana Tempo, and best practices.

## Overview

Distributed tracing helps debug and understand:
- Request flow through the gateway API
- Actor operations (Gossip, Network, Ledger, Trust, CCL)
- Cross-node message propagation
- Performance bottlenecks

### Architecture

```
┌─────────┐     ┌─────────┐     ┌─────────┐
│  Node A │────▶│  Node B │────▶│  Node C │
│  spans  │     │  spans  │     │  spans  │
└────┬────┘     └────┬────┘     └────┬────┘
     │               │               │
     └───────────────┼───────────────┘
                     ▼
               ┌──────────┐
               │  Tempo   │
               │ (traces) │
               └────┬─────┘
                    │
                    ▼
               ┌──────────┐
               │ Grafana  │
               │ (UI)     │
               └──────────┘
```

## Configuration

### Environment Variables

The simplest way to enable tracing is via environment variables:

```bash
# Enable tracing with OTLP endpoint
export OTEL_EXPORTER_OTLP_ENDPOINT="http://tempo:4317"
export OTEL_SERVICE_NAME="icnd"

# Run daemon
./icnd
```

### CLI Arguments

```bash
./icnd --tracing-enable \
       --tracing-otlp-endpoint http://tempo:4317 \
       --tracing-sampling-rate 0.1
```

### Configuration File

Add to your `icn.toml`:

```toml
[observability.tracing]
enabled = true
otlp_endpoint = "http://tempo:4317"
sampling_rate = 0.1  # 10% sampling for production
service_name = "icnd"
```

### Configuration Options

| Option | Default | Description |
|--------|---------|-------------|
| `enabled` | `false` | Enable distributed tracing |
| `otlp_endpoint` | `http://localhost:4317` | OTLP gRPC endpoint for Tempo |
| `sampling_rate` | `0.1` | Percentage of traces to sample (0.0-1.0) |
| `service_name` | `icnd` | Service name in traces |

## Kubernetes Deployment

### Deploy Tempo

Apply the Tempo deployment:

```bash
kubectl apply -f deploy/k8s/tempo.yaml
```

This creates:
- ConfigMap with Tempo configuration
- Deployment running Grafana Tempo 2.3.1
- Service exposing OTLP ports (4317/4318) and HTTP API (3200)

### Enable Tracing in ICN Daemon

Edit `deploy/k8s/deployment.yaml` and uncomment the OTEL environment variables:

```yaml
env:
  - name: OTEL_EXPORTER_OTLP_ENDPOINT
    value: "http://tempo:4317"
  - name: OTEL_SERVICE_NAME
    value: "icnd"
```

Then apply:

```bash
kubectl apply -f deploy/k8s/deployment.yaml
```

### Configure Grafana

Add Tempo as a data source in Grafana:

1. Navigate to **Configuration > Data Sources**
2. Click **Add data source**
3. Select **Tempo**
4. Configure:
   - URL: `http://tempo:3200`
   - HTTP Method: `GET`
5. Click **Save & Test**

## Instrumented Operations

### Gateway HTTP Requests

All HTTP requests through the gateway are automatically traced:
- Request method, path, status code
- User ID (if authenticated)
- Response time

### Actor Operations

Key actor operations are instrumented with `#[instrument]`:

**GossipActor:**
- `publish()` - Topic, data size, publisher DID
- `subscribe()` - Topic, subscriber DID

**NetworkActor:**
- `send_message()` - Recipient DID
- `broadcast()` - Message broadcast

**Ledger:**
- `detect_and_resolve_forks()` - Fork detection

**TrustGraph:**
- `compute_trust_score()` - Trust computation for target DID

**CCL Interpreter:**
- `execute_rule()` - Contract rule execution with fuel limit

### Trace Context Propagation

Trace context is propagated across network messages using W3C Trace Context format:

```rust
// Messages automatically include trace context when tracing is enabled
let msg = NetworkMessage::gossip(from, to, payload)
    .with_current_trace_context();
```

The `trace_context` field in `NetworkMessage` contains:
- `traceparent`: W3C traceparent header
- `tracestate`: W3C tracestate header (optional)

## Viewing Traces

### Grafana Explore

1. Go to **Explore** in Grafana
2. Select **Tempo** data source
3. Use TraceQL queries:

```traceql
# Find traces by service
{ resource.service.name = "icnd" }

# Find slow operations
{ span.duration > 100ms }

# Find specific operations
{ span.name = "publish" && span.topic = "ledger:sync" }
```

### Trace Details

Click on a trace to see:
- Span hierarchy (parent-child relationships)
- Duration breakdown
- Attributes (DIDs, topics, message types)
- Logs attached to spans

## Sampling Strategies

### Production (Recommended)

Use 10% sampling to balance visibility and overhead:

```toml
[observability.tracing]
sampling_rate = 0.1
```

### Development

Use 100% sampling for complete visibility:

```toml
[observability.tracing]
sampling_rate = 1.0
```

### High-Volume Production

Use 1-5% sampling for very busy nodes:

```toml
[observability.tracing]
sampling_rate = 0.01  # 1%
```

## Performance Considerations

1. **Sampling**: Always use sampling in production. Full tracing adds ~5% overhead.

2. **Span Attributes**: Avoid high-cardinality attributes (raw DIDs in span names).

3. **Batch Export**: Traces are batched before export to reduce network overhead.

4. **Graceful Shutdown**: Call `icn_obs::shutdown_tracing()` to flush pending traces.

## Troubleshooting

### No Traces Appearing

1. Check tracing is enabled:
   ```bash
   grep -r "OTEL" /proc/$(pgrep icnd)/environ
   ```

2. Verify Tempo connectivity:
   ```bash
   curl http://tempo:3200/ready
   ```

3. Check sampling rate isn't too low

### Missing Spans

1. Ensure the operation has `#[instrument]` attribute
2. Check the span isn't being filtered by sampling
3. Verify trace context propagation in network messages

### High Memory Usage

1. Reduce batch size in OTLP exporter
2. Lower sampling rate
3. Check for span attribute explosion

## Integration with Existing Observability

### Prometheus Metrics

Tempo can generate metrics from traces. Configure in `tempo.yaml`:

```yaml
metrics_generator:
  storage:
    remote_write:
      - url: http://prometheus:9090/api/v1/write
```

### Correlation with Logs

Traces include span IDs that can correlate with structured logs:

```rust
tracing::info!(parent: &span, "Processing message");
```

## API Reference

### TracingConfig

```rust
pub struct TracingConfig {
    /// Enable distributed tracing
    pub enabled: bool,

    /// OTLP gRPC endpoint (e.g., "http://localhost:4317")
    pub otlp_endpoint: String,

    /// Sampling rate (0.0-1.0, default: 0.1)
    pub sampling_rate: f64,

    /// Service name for traces (default: "icnd")
    pub service_name: String,
}
```

### TraceContext

```rust
pub struct TraceContext {
    /// W3C traceparent header value
    pub traceparent: Option<String>,

    /// W3C tracestate header value
    pub tracestate: Option<String>,
}

impl TraceContext {
    /// Create context from current span
    pub fn from_current() -> Self;

    /// Check if context is valid
    pub fn is_valid(&self) -> bool;
}
```

### Functions

```rust
/// Initialize OpenTelemetry tracing
pub fn init_tracing(config: &TracingConfig) -> Result<()>;

/// Shutdown tracing and flush pending spans
pub fn shutdown_tracing();
```

## See Also

- [Production Hardening Guide](../../security/production-hardening.md)
- [Kubernetes Deployment](../../../deploy/k8s/README.md)
- [Architecture Overview](../../ARCHITECTURE.md)
