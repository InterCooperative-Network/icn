# ICN Monitoring

Prometheus and Grafana monitoring for ICN nodes.

## Quick Start

### 1. Start Prometheus

Create `prometheus.yml`:

```yaml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: 'icn-nodes'
    static_configs:
      - targets:
        - 'localhost:9090'  # Your local ICN node
        # Add more nodes as needed:
        # - 'node2:9090'
        # - 'node3:9090'
```

Run Prometheus:

```bash
# Docker
docker run -d \
  --name prometheus \
  -p 9091:9090 \
  -v $(pwd)/prometheus.yml:/etc/prometheus/prometheus.yml \
  prom/prometheus

# Or direct binary
prometheus --config.file=prometheus.yml
```

### 2. Start Grafana

```bash
# Docker
docker run -d \
  --name grafana \
  -p 3000:3000 \
  grafana/grafana-oss

# Default login: admin/admin (change immediately for production)
```

### 3. Import Dashboard

1. Open Grafana at http://localhost:3000
2. Add Prometheus data source:
   - Go to Configuration > Data Sources > Add data source
   - Select "Prometheus"
   - Set URL to `http://localhost:9091` (or where Prometheus runs)
   - Click "Save & Test"
3. Import the dashboard:
   - Go to Dashboards > Import
   - Upload `grafana-dashboard.json`
   - Select your Prometheus data source
   - Click "Import"

## Dashboard Panels

The ICN Node Dashboard includes:

### Network Overview
- **Active Connections**: Current peer count (green >3, yellow 1-2, red 0)
- **Total Connections**: Cumulative connection count
- **Connections Over Time**: Historical connection graph

### Gossip Protocol
- **Message Rate**: Real-time ops/sec for announces, requests, responses
- **Message Totals**: Cumulative message counts

### Ledger
- **Total Ledger Entries**: Transaction count
- **Quarantined Entries**: Conflicts pending resolution (alert if >10)
- **Ledger Growth**: Transaction volume over time

### Security & Rate Limiting
- **Messages Rate Limited**: Count of throttled messages
- **Rate Limiting Activity**: Indicates potential attacks

### Graceful Restart & Snapshots
- **Snapshot Duration**: Time to save/load state (p99)
- **Vector Clocks/Subscriptions**: State preserved across restarts

### Version Negotiation
- **Negotiation Results**: Success/failure/legacy peer counts
- **Peer Capabilities**: Distribution of node features

## Alerting

Recommended alert rules (add to Prometheus or Grafana):

```yaml
groups:
  - name: icn-alerts
    rules:
      - alert: ICNNoConnections
        expr: icn_network_connections_active == 0
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "ICN node has no peer connections"

      - alert: ICNHighQuarantine
        expr: icn_ledger_entries_quarantined > 10
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High number of quarantined ledger entries"

      - alert: ICNRateLimiting
        expr: rate(icn_network_messages_rate_limited_total[5m]) > 10
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "Possible attack - high rate limiting activity"
```

## Docker Compose

For a complete monitoring stack:

```yaml
version: '3.8'

services:
  prometheus:
    image: prom/prometheus:latest
    ports:
      - "9091:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml
      - prometheus-data:/prometheus
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.path=/prometheus'
      - '--storage.tsdb.retention.time=30d'

  grafana:
    image: grafana/grafana-oss:latest
    ports:
      - "3000:3000"
    volumes:
      - grafana-data:/var/lib/grafana
      - ./grafana-dashboard.json:/var/lib/grafana/dashboards/icn.json
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=admin
      - GF_DASHBOARDS_DEFAULT_HOME_DASHBOARD_PATH=/var/lib/grafana/dashboards/icn.json

volumes:
  prometheus-data:
  grafana-data:
```

Run with: `docker-compose up -d`

## Available Metrics

ICN exposes these Prometheus metrics on port 9090:

### Network
- `icn_network_connections_active` - Current active connections
- `icn_network_connections_total` - Total connections (all time)
- `icn_network_messages_rate_limited_total` - Messages dropped due to rate limiting

### Gossip
- `icn_gossip_announces_sent_total` - Announce messages sent
- `icn_gossip_announces_received_total` - Announce messages received
- `icn_gossip_requests_sent_total` - Pull requests sent
- `icn_gossip_responses_sent_total` - Pull responses sent

### Ledger
- `icn_ledger_entries_total` - Total ledger entries
- `icn_ledger_entries_quarantined` - Entries in quarantine

### Snapshots
- `icn_snapshot_save_duration_seconds` - Histogram of save times
- `icn_snapshot_load_duration_seconds` - Histogram of load times
- `icn_snapshot_saves_total` - Total snapshot saves
- `icn_snapshot_loads_total` - Total snapshot loads
- `icn_snapshot_vector_clocks_count` - Vector clocks in last snapshot
- `icn_snapshot_subscriptions_count` - Subscriptions in last snapshot
- `icn_snapshot_peer_keys_count` - Peer X25519 keys in last snapshot

### Version Negotiation
- `icn_version_negotiation_success_total` - Successful negotiations
- `icn_version_negotiation_failure_total` - Failed negotiations
- `icn_version_negotiation_legacy_total` - Legacy peer connections
- `icn_peer_capability_count` - Count by capability flag
