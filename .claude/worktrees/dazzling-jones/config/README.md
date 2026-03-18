# ICN Configuration Files

This directory contains example configuration files for ICN nodes.

## Configuration Files

### `icn.toml.example`
**Comprehensive configuration template** showing all available options with descriptions.

Copy and customize for your deployment:
```bash
cp config/icn.toml.example ~/.icn/icn.toml
# Edit ~/.icn/icn.toml with your values
```

### `icn-minimal.toml.example`
**Minimal configuration** showing only essential options with defaults.

Use as a starting point for simple deployments.

### `icn-alpha.toml` and `icn-beta.toml`
**Two-node local demo** configurations for testing ICN on a single machine.

Each node uses different ports to avoid conflicts:
- **Alpha**: QUIC=7777, RPC=5601, Metrics=9100, Health=8080, Data=/tmp/icn-alpha
- **Beta**: QUIC=7778, RPC=5602, Metrics=9101, Health=8081, Data=/tmp/icn-beta

Run both nodes:
```bash
# Quick start with demo script
./scripts/demo-two-node.sh

# Or manually in separate terminals:
# Terminal 1
cd icn && cargo build --release
ICN_PASSPHRASE="testpass123" ./target/release/icnd --config config/icn-alpha.toml

# Terminal 2
ICN_PASSPHRASE="testpass123" ./target/release/icnd --config config/icn-beta.toml

# Nodes will discover each other via mDNS
```

### `prometheus.yml`
**Prometheus scrape configuration** for monitoring ICN nodes.

Configured to scrape both alpha and beta nodes on ports 9090 and 9091.

Usage:
```bash
prometheus --config.file=config/prometheus.yml
# Access Prometheus UI at http://localhost:9090
```

## Configuration Sections

### `data_dir`
Directory for node state, keystore, and databases.
- Default: `~/.icn`
- Contains: `keystore.age`, `sled/` database trees

### `[network]`
Peer-to-peer networking configuration.
- `listen_addr`: QUIC listener (UDP port)
- `mdns_enabled`: Automatic local peer discovery
- `bootstrap_peers`: WAN seed nodes (optional)

### `[observability]`
Metrics and logging configuration.
- `metrics_port`: Prometheus HTTP exporter
- `health_port`: Health check endpoint
- `log_level`: `trace` | `debug` | `info` | `warn` | `error`

### `[rate_limiting]` **NEW!**
Trust-gated rate limiting protects nodes from DoS attacks while allowing trusted peers higher throughput.

Rate limits are applied per-peer based on their trust classification:
- **Isolated** (trust 0.0-0.1): 10 msg/sec, burst 2 — Untrusted/new peers
- **Known** (trust 0.1-0.4): 50 msg/sec, burst 10 — Basic trust
- **Partner** (trust 0.4-0.7): 100 msg/sec, burst 20 — Trusted collaborators
- **Federated** (trust 0.7-1.0): 200 msg/sec, burst 50 — Highly trusted

Configuration options:
- `enabled`: Toggle trust-gated rate limiting (default: `true`)
- `refill_interval_ms`: Token bucket refill rate (default: `100`)
- Per-class limits: `isolated`, `known`, `partner`, `federated`, `fallback`

Trust upgrades take immediate effect with full token reset, enabling dynamic security without manual intervention.

## Environment Variable Overrides

You can override any config value with environment variables:
```bash
export ICN_DATA_DIR="/custom/path"
export ICN_NETWORK_LISTEN_ADDR="0.0.0.0:7777"
export ICN_OBSERVABILITY_LOG_LEVEL="debug"
icnd --config config/icn.toml
```

Format: `ICN_<SECTION>_<KEY>` (uppercase with underscores)

## Loading Configuration

ICN reads configuration in this order (later sources override earlier):
1. Built-in defaults
2. Config file (if specified with `--config`)
3. `~/.icn/icn.toml` (if exists and no `--config`)
4. Environment variables
5. CLI flags

## Validation

Validate your configuration before starting the node:
```bash
icnd --config myconfig.toml --validate
```

## Next Steps

- See [docs/configuration-reference.md](../docs/configuration-reference.md) for complete reference (coming soon)
- See [docs/deployment-guide.md](../docs/deployment-guide.md) for production deployment
- See [examples/01-quickstart/](../examples/01-quickstart/) for getting started tutorials
