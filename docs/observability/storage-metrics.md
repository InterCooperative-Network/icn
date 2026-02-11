# Storage Metrics Reference

ICN exposes storage-related metrics via Prometheus for monitoring database health, performance, and maintenance operations.

## Metric Naming Convention

All storage metrics follow the pattern: `icn_storage_<component>_<measurement>_<unit>`

## General Storage Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `icn_storage_size_bytes` | Gauge | Current size of the storage backend on disk in bytes |
| `icn_storage_space_amplification` | Gauge | Space amplification factor (actual size / logical size) |
| `icn_storage_flush_total` | Counter | Total number of storage flush operations |
| `icn_storage_flush_bytes_total` | Counter | Total bytes flushed to disk |
| `icn_storage_flush_duration_seconds` | Histogram | Duration of flush operations in seconds |
| `icn_storage_operations_total` | Counter | Total storage operations by type (labels: `operation`) |

### Operation Labels

The `icn_storage_operations_total` metric includes an `operation` label with these values:
- `get` - Read operations
- `put` - Write operations
- `delete` - Delete operations
- `scan` - Range scan operations

## Maintenance Metrics

These metrics track periodic storage maintenance operations (flush, space monitoring).

| Metric | Type | Description |
|--------|------|-------------|
| `icn_storage_maintenance_runs_total` | Counter | Total number of maintenance runs |
| `icn_storage_maintenance_duration_seconds` | Histogram | Duration of maintenance operations in seconds |
| `icn_storage_maintenance_space_reclaimed_bytes` | Counter | Total bytes reclaimed by maintenance |
| `icn_storage_maintenance_errors_total` | Counter | Total number of maintenance errors |

### Maintenance Configuration

Maintenance is configured in `icn.toml` under `[supervisor.storage_maintenance]`:

```toml
[supervisor.storage_maintenance]
enabled = true                      # Enable periodic maintenance (default: true)
interval_secs = 3600                # Interval between runs (default: 3600 = 1 hour)
amplification_threshold = 2.0       # Warn if space amplification exceeds this
size_threshold_bytes = 1073741824   # Warn if size exceeds this (default: 1GB)
flush_on_maintenance = true         # Flush data to disk during maintenance
```

## Alerting Recommendations

### Space Amplification Alert

```yaml
# Alert when space amplification is high for extended period
- alert: StorageHighSpaceAmplification
  expr: icn_storage_space_amplification > 2.5
  for: 1h
  labels:
    severity: warning
  annotations:
    summary: "High storage space amplification"
    description: "Space amplification {{ $value }} exceeds threshold"
```

### Maintenance Errors Alert

```yaml
# Alert on maintenance failures
- alert: StorageMaintenanceErrors
  expr: increase(icn_storage_maintenance_errors_total[1h]) > 0
  labels:
    severity: warning
  annotations:
    summary: "Storage maintenance errors detected"
    description: "{{ $value }} maintenance errors in the last hour"
```

### Database Size Alert

```yaml
# Alert when database grows large
- alert: StorageSizeLarge
  expr: icn_storage_size_bytes > 5e9  # 5GB
  for: 30m
  labels:
    severity: warning
  annotations:
    summary: "Storage size exceeds threshold"
    description: "Database size is {{ humanize $value }}"
```

## Grafana Dashboard Queries

### Storage Size Over Time

```promql
icn_storage_size_bytes
```

### Maintenance Duration P99

```promql
histogram_quantile(0.99, sum(rate(icn_storage_maintenance_duration_seconds_bucket[5m])) by (le))
```

### Operations Rate by Type

```promql
sum(rate(icn_storage_operations_total[5m])) by (operation)
```

### Maintenance Health

```promql
# Successful maintenance rate
1 - (rate(icn_storage_maintenance_errors_total[1h]) / rate(icn_storage_maintenance_runs_total[1h]))
```

## See Also

- [Distributed Tracing](../operations/deployment/distributed-tracing.md)
- [Tail-Based Sampling](tail-based-sampling.md)
- [Production Hardening](../security/production-hardening.md)
