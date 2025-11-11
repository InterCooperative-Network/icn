# ICN Docker Deployment

This directory contains Docker configuration for running ICN in containers.

## Quick Start

### Build and Run

```bash
# From repository root
docker-compose -f docker/docker-compose.yml up --build

# Or run in background
docker-compose -f docker/docker-compose.yml up -d

# View logs
docker-compose -f docker/docker-compose.yml logs -f

# Stop
docker-compose -f docker/docker-compose.yml down
```

### Access Services

Once running, you can access:

- **Alpha Node RPC**: `http://localhost:5050`
- **Beta Node RPC**: `http://localhost:5051`
- **Alpha Metrics**: `http://localhost:9090/metrics`
- **Beta Metrics**: `http://localhost:9091/metrics`
- **Prometheus UI**: `http://localhost:9000`

### Test the Network

```bash
# Install icnctl locally (or use docker exec)
cd icn && cargo build --release

# Check alpha node status
./target/release/icnctl --endpoint 127.0.0.1:5050 network status

# Check beta node status
./target/release/icnctl --endpoint 127.0.0.1:5051 network status

# List peers (nodes should discover each other)
./target/release/icnctl --endpoint 127.0.0.1:5050 network peers
```

Alternatively, exec into the container:

```bash
# Connect to alpha node
docker exec -it icn-alpha icnctl --endpoint 127.0.0.1:5050 network status

# Connect to beta node
docker exec -it icn-beta icnctl --endpoint 127.0.0.1:5051 network status
```

## Files

### `Dockerfile`
Multi-stage Dockerfile that:
1. Builds `icnd` and `icnctl` binaries in a Rust builder image
2. Creates a minimal Debian runtime image with just the binaries
3. Runs as non-root user `icn`
4. Exposes ports: 4433 (QUIC), 5050 (RPC), 9090 (metrics), 8080 (health)

### `docker-compose.yml`
Production-ready stack with:
- Two ICN nodes (alpha and beta)
- Prometheus for metrics collection
- Persistent volumes for data
- Health checks
- Automatic restarts

### `docker-compose.dev.yml`
Development stack with:
- Debug logging enabled
- No automatic restarts
- Local volume mounts (./volumes/)
- Lighter configuration

### `.dockerignore`
Excludes unnecessary files from Docker build context to speed up builds.

## Development Workflow

For active development, use the dev compose file:

```bash
# Build and run
docker-compose -f docker/docker-compose.dev.yml up --build

# Make changes to code...

# Rebuild and restart
docker-compose -f docker/docker-compose.dev.yml up --build

# Clean up
docker-compose -f docker/docker-compose.dev.yml down -v
```

## Production Deployment

### Environment Variables

Override configuration via environment variables in your compose file:

```yaml
environment:
  - ICN_DATA_DIR=/data
  - ICN_NETWORK_LISTEN_ADDR=0.0.0.0:4433
  - ICN_NETWORK_MDNS_ENABLED=false
  - ICN_OBSERVABILITY_LOG_LEVEL=info
  - ICN_OBSERVABILITY_METRICS_PORT=9090
```

### Custom Configuration

Mount a custom config file:

```yaml
volumes:
  - ./myconfig.toml:/etc/icn/icn.toml:ro
```

### Secrets Management

For production, use Docker secrets or external secret managers:

```yaml
secrets:
  - icn_keystore

services:
  icn-alpha:
    secrets:
      - icn_keystore
    environment:
      - ICN_KEYSTORE_PATH=/run/secrets/icn_keystore
```

### Networking

#### Bridge Network (Default)
Nodes run on isolated bridge network:
- Good for local testing
- mDNS doesn't work across Docker networks
- Use manual peering or bootstrap_peers config

#### Host Network (Production)
For production with mDNS discovery:

```yaml
services:
  icn-alpha:
    network_mode: host
    ports: []  # Not needed in host mode
```

**Note:** Host networking works only on Linux.

### Scaling

Run multiple nodes:

```bash
docker-compose -f docker/docker-compose.yml up --scale icn-node=5
```

For this to work, you'll need to:
1. Remove `container_name` directives
2. Use dynamic port allocation
3. Configure bootstrap peers for discovery

## Monitoring

### Prometheus Queries

Access Prometheus at `http://localhost:9000` and try these queries:

```promql
# Total network connections
icn_network_connections_total

# Active connections by node
icn_network_connections_active{node="alpha"}

# Gossip message rate
rate(icn_gossip_announces_sent_total[5m])

# Ledger entries
icn_ledger_entries_total

# Rate limited messages (attacks?)
rate(icn_network_messages_rate_limited_total[5m])
```

### Grafana Integration

Create a `docker-compose.monitoring.yml`:

```yaml
version: '3.9'

services:
  grafana:
    image: grafana/grafana:latest
    ports:
      - "3000:3000"
    volumes:
      - grafana-data:/var/lib/grafana
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=admin
    depends_on:
      - prometheus

volumes:
  grafana-data:
```

Run together:

```bash
docker-compose \
  -f docker/docker-compose.yml \
  -f docker/docker-compose.monitoring.yml \
  up
```

Access Grafana at `http://localhost:3000` and add Prometheus datasource at `http://prometheus:9090`.

## Troubleshooting

### Container won't start

```bash
# Check logs
docker-compose -f docker/docker-compose.yml logs icn-alpha

# Check health
docker inspect icn-alpha | jq '.[0].State.Health'
```

### Nodes not discovering each other

```bash
# mDNS doesn't work in Docker bridge mode
# Solution 1: Use host networking (Linux only)
network_mode: host

# Solution 2: Manual peering
docker exec icn-beta icnctl network dial \
  did:icn:ALPHA_DID icn-alpha:4433
```

### Port conflicts

If ports are already in use:

```yaml
# Change external ports in docker-compose.yml
ports:
  - "14433:4433/udp"  # Change left side only
  - "15050:5050"
```

### Data persistence

```bash
# List volumes
docker volume ls

# Inspect volume
docker volume inspect icn_icn-alpha-data

# Backup volume
docker run --rm -v icn_icn-alpha-data:/data -v $(pwd):/backup \
  alpine tar czf /backup/alpha-backup.tar.gz /data

# Restore volume
docker run --rm -v icn_icn-alpha-data:/data -v $(pwd):/backup \
  alpine tar xzf /backup/alpha-backup.tar.gz -C /
```

### Clean rebuild

```bash
# Stop and remove everything
docker-compose -f docker/docker-compose.yml down -v

# Remove images
docker rmi icn:latest

# Rebuild from scratch
docker-compose -f docker/docker-compose.yml build --no-cache
docker-compose -f docker/docker-compose.yml up
```

## Security Considerations

### Image Scanning

Scan the built image for vulnerabilities:

```bash
docker scan icn:latest
```

### Non-Root User

The Dockerfile runs as non-root user `icn` (UID 1000) for security.

### Read-Only Filesystem

For extra security, run with read-only root filesystem:

```yaml
services:
  icn-alpha:
    read_only: true
    tmpfs:
      - /tmp
    volumes:
      - icn-alpha-data:/data  # Only /data is writable
```

### Network Policies

In production, use Docker network policies or Kubernetes NetworkPolicies to restrict traffic.

## Next Steps

- See [../docs/deployment-guide.md](../docs/deployment-guide.md) for production deployment
- See [../examples/](../examples/) for usage examples
- See [../config/](../config/) for configuration options
