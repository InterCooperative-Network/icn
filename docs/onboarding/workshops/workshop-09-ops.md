# Workshop 9: Local Deployment and Observability

## Learning Objectives

By the end of this workshop, you will be able to:

1. **Deploy ICN locally** - Run ICN via native binary or Docker
2. **Verify health status** - Check component health via API endpoints
3. **Monitor with Prometheus** - Scrape and query ICN metrics
4. **Configure logging** - Adjust log levels and understand log output
5. **Troubleshoot issues** - Diagnose common deployment problems

## Goal
Deploy ICN locally using Docker or native binaries, verify health endpoints,
explore Prometheus metrics, and understand logging and tracing.

## Prerequisites
- Completed [Module 9: Operations](../reference/module-09-ops-deploy.md)
- Docker installed (for container deployment) or Rust toolchain (for native)
- curl and jq installed

## Estimated time
2-3 hours

## Related Materials
- [Module 9: Operations](../reference/module-09-ops-deploy.md) - Background reading
- [HOMELAB_DEPLOYMENT.md](../../operations/deployment/HOMELAB_DEPLOYMENT.md) - K3s production deployment
- [production-hardening.md](../../security/production-hardening.md) - Security hardening

## Part 1: Deployment Options

### Overview
ICN can be deployed in several ways:

| Method | Use Case |
|--------|----------|
| Native binary | Development, single-node testing |
| Docker | Local testing, reproducible environments |
| Docker Compose | Multi-node local clusters |
| Kubernetes/K3s | Production deployment |

### Locate deployment files
```bash
ls -la deploy/
ls -la deploy/docker/ 2>/dev/null
ls -la deploy/k8s/ 2>/dev/null
```

### Checkpoint
- [ ] You identified available deployment methods
- [ ] You know which files configure each method

## Part 2: Native Binary Deployment

### Steps
1. Build release binaries:
   ```bash
   cd icn && cargo build --release
   ```

2. Create a data directory:
   ```bash
   export ICN_DATA=$(mktemp -d)
   export ICN_PASSPHRASE="workshop"
   ```

3. Initialize identity:
   ```bash
   ./target/release/icnctl --data-dir "$ICN_DATA" id init
   ```

4. Start the daemon:
   ```bash
   ./target/release/icnd --data-dir "$ICN_DATA"
   ```

### Configuration file
Create `$ICN_DATA/config.toml`:
```toml
data_dir = "$ICN_DATA"

[network]
listen_addr = "0.0.0.0:4001"
mdns_enabled = true

[gateway]
enabled = true
bind_addr = "0.0.0.0:8080"

[observability]
metrics_port = 9100
health_port = 8081
log_level = "info"
```

### Questions to answer
1. What ports are used by default?
2. How do you change the gateway port?
3. Where are logs written?

### Checkpoint
- [ ] Daemon starts successfully
- [ ] You can access the gateway

## Part 3: Docker Deployment

### Steps
1. Build the Docker image:
   ```bash
   docker build -t icn:local -f deploy/docker/Dockerfile .
   ```

2. Run the container:
   ```bash
   docker run -d \
     --name icn-node \
     -p 8080:8080 \
     -p 9100:9100 \
     -p 8081:8081 \
     -p 4001:4001 \
     -e ICN_KEYSTORE_PASSPHRASE=workshop \
     icn:local
   ```

   > **Security Note**: The example above passes the passphrase via environment
   > variable for simplicity. In production, use Docker secrets or mount a
   > credentials file to avoid exposing passphrases in process listings or
   > container inspect output.

3. Check logs:
   ```bash
   docker logs -f icn-node
   ```

### Docker Compose (multi-node)
If available:
```bash
cd deploy/docker
docker-compose up -d
docker-compose logs -f
```

### Questions to answer
1. How are secrets passed to the container?
2. What volumes are mounted?
3. How do containers discover each other?

### Checkpoint
- [ ] Docker container running
- [ ] Gateway accessible from host

## Part 4: Health Endpoint

### Steps
1. Check the basic health endpoint:
   ```bash
   curl -s http://localhost:8080/health | jq
   ```

2. Basic health response:
   ```json
   {
     "status": "ok",
     "version": "0.1.0"
   }
   ```

3. For detailed component checks, use `/health/detailed`:
   ```bash
   curl -s http://localhost:8080/health/detailed | jq
   ```

4. Detailed health response:
   ```json
   {
     "status": "healthy",
     "version": "0.1.0",
     "checks": {
       "cooperative_manager": {"status": "ok", "details": "..."},
       "notification_queue": {"status": "ok", "details": "..."},
       "system": {"status": "ok", "details": "..."}
     }
   }
   ```

### Health check details
Find the health endpoint implementation:
```bash
grep -r "health" icn/crates/icn-gateway/src/ --include="*.rs" | head -10
```

### Questions to answer
1. What components are checked in `/health/detailed`?
2. What's the difference between `/health` and `/health/detailed`?
3. How often should health be polled?

### Checkpoint
- [ ] Basic health endpoint returns 200
- [ ] Detailed health shows component status

## Part 5: Prometheus Metrics

### Steps
1. Access the metrics endpoint:
   ```bash
   curl -s http://localhost:9100/metrics | head -50
   ```

2. Identify key metric types:
   - Counters (total counts)
   - Gauges (current values)
   - Histograms (distributions)

### Key metrics to find
| Metric | Type | Description |
|--------|------|-------------|
| `icn_gateway_requests_total` | Counter | Total HTTP requests |
| `icn_gossip_messages_received_total` | Counter | Gossip messages received |
| `icn_ledger_entries_total` | Counter | Total ledger entries |
| `icn_trust_edges_total` | Gauge | Current trust edges |
| `icn_network_peers_connected` | Gauge | Connected peers |

### Exercise
Search for metric definitions:
```bash
grep -r "counter!\|gauge!\|histogram!" icn/crates/icn-obs/src/ --include="*.rs" | head -20
```

### Questions to answer
1. What naming convention do metrics follow?
2. How are labels used?
3. What is the scrape interval for Prometheus?

### Checkpoint
- [ ] Metrics endpoint accessible
- [ ] You can identify key metrics

## Part 6: Prometheus + Grafana Setup (Optional)

### Steps
1. Create `prometheus.yml`:
   ```yaml
   global:
     scrape_interval: 15s

   scrape_configs:
     - job_name: 'icn'
       static_configs:
         - targets: ['localhost:9100']
   ```

2. Run Prometheus:
   ```bash
   docker run -d \
     --name prometheus \
     -p 9090:9090 \
     -v $(pwd)/prometheus.yml:/etc/prometheus/prometheus.yml \
     prom/prometheus
   ```

3. Access Prometheus UI: http://localhost:9090

4. Run Grafana (optional):
   ```bash
   docker run -d \
     --name grafana \
     -p 3000:3000 \
     grafana/grafana
   ```

5. Access Grafana: http://localhost:3000 (admin/admin)

### PromQL queries to try
```promql
# Request rate over 5 minutes
rate(icn_gateway_requests_total[5m])

# Current connected peers
icn_network_peers_connected

# Gossip message rate by type
rate(icn_gossip_messages_received_total[5m])
```

### Checkpoint
- [ ] Prometheus scraping ICN metrics
- [ ] You can query metrics in PromQL

## Part 7: Logging Configuration

### Steps
1. Find logging configuration:
   ```bash
   grep -r "tracing\|logging" icn/crates/icn-obs/src/ --include="*.rs" | head -15
   ```

2. Test different log levels:
   ```bash
   RUST_LOG=debug ./target/release/icnd --data-dir "$ICN_DATA"
   RUST_LOG=icn_gossip=trace ./target/release/icnd --data-dir "$ICN_DATA"
   ```

### Log levels
| Level | Use Case |
|-------|----------|
| error | Critical failures |
| warn | Recoverable issues |
| info | Normal operations |
| debug | Development debugging |
| trace | Deep inspection |

### Log format (JSON)
```json
{
  "timestamp": "2024-01-01T12:00:00Z",
  "level": "INFO",
  "target": "icn_gossip::actor",
  "message": "Subscribed to topic",
  "topic": "ledger:entries"
}
```

### Questions to answer
1. How do you filter logs by module?
2. What format is used for timestamps?
3. How are structured fields included?

### Checkpoint
- [ ] You can adjust log levels
- [ ] You understand log format

## Part 8: Distributed Tracing (Optional)

### Steps
1. Find tracing configuration in `icn-obs`
2. Look for OpenTelemetry integration
3. Understand span structure

### Tracing concepts
```
Request arrives at gateway
  └── Span: "gateway.handle_request"
        └── Span: "ledger.create_entry"
              └── Span: "store.put"
        └── Span: "gossip.announce"
              └── Span: "network.broadcast"
```

### Jaeger setup (optional)
```bash
docker run -d \
  --name jaeger \
  -p 16686:16686 \
  -p 6831:6831/udp \
  jaegertracing/all-in-one
```

### Questions to answer
1. What is a trace vs a span?
2. How are trace IDs propagated?
3. What exporter is configured?

### Checkpoint
- [ ] You understand tracing structure
- [ ] You know how to enable tracing

## Part 9: Troubleshooting Checklist

### Common issues and diagnostics

**Daemon won't start:**
```bash
# Check port availability
lsof -i :8080
lsof -i :4001

# Check keystore
./target/release/icnctl --data-dir "$ICN_DATA" id show
```

**Gateway not responding:**
```bash
# Check health
curl -v http://localhost:8080/health

# Check logs
RUST_LOG=debug ./target/release/icnd --data-dir "$ICN_DATA" 2>&1 | head -50
```

**Metrics not available:**
```bash
# Check metrics endpoint
curl -v http://localhost:9100/metrics

# Verify metrics are enabled in config
```

**Gossip not working:**
```bash
# Check network connectivity
./target/release/icnctl --data-dir "$ICN_DATA" status

# Check for mDNS
RUST_LOG=icn_net=debug ./target/release/icnd --data-dir "$ICN_DATA"
```

### Checkpoint
- [ ] You can diagnose common issues
- [ ] You know which logs to check

## Part 10: Production Considerations

### Security checklist
- [ ] TLS certificates configured
- [ ] Keystore passphrase secured
- [ ] Rate limiting enabled
- [ ] CORS origins restricted

### Monitoring checklist
- [ ] Prometheus scraping metrics
- [ ] Alerting configured for critical metrics
- [ ] Log aggregation in place
- [ ] Dashboards created for key metrics

### Backup checklist
- [ ] Keystore backed up securely
- [ ] Sled database backup strategy
- [ ] Configuration versioned

## Summary

After completing this workshop you should be able to:
- Deploy ICN using native binaries or Docker
- Verify health and component status
- Access and understand Prometheus metrics
- Configure and filter logging
- Troubleshoot common operational issues

## Key Takeaways

| Concept | Key Point |
|---------|-----------|
| **Health Endpoints** | `/health` (basic), `/health/detailed` (component status) |
| **Metrics Port** | 9100 by default; Prometheus format |
| **Gateway Port** | 8080 by default; REST + WebSocket |
| **Network Port** | 4001 by default; QUIC/UDP |
| **Log Levels** | error < warn < info < debug < trace |
| **Log Filtering** | `RUST_LOG=icn_gossip=debug,icn_ledger=trace` |

## Try It Yourself

**Challenge 1**: Create a Grafana dashboard
1. Set up Prometheus + Grafana (as shown in Part 6)
2. Create a dashboard with:
   - Connected peers gauge
   - Request rate graph
   - Gossip message throughput
   - Ledger entry count

**Challenge 2**: Alert configuration
Create a Prometheus alerting rule for:
- Health check failure
- No connected peers for 5 minutes
- Gossip message rate drops to zero

**Challenge 3**: Log analysis
1. Run with `RUST_LOG=debug` for 10 minutes
2. Analyze the logs to answer:
   - How many gossip messages were processed?
   - Were there any warnings or errors?
   - What was the most chatty component?

**Challenge 4**: Multi-node deployment
Use Docker Compose to deploy 3 ICN nodes that form a network:
1. Nodes discover each other via mDNS
2. Subscribe to a shared topic
3. Verify message propagation across all nodes

## Cleanup

```bash
# Stop containers
docker stop icn-node prometheus grafana jaeger 2>/dev/null
docker rm icn-node prometheus grafana jaeger 2>/dev/null

# Remove temp data
rm -rf "$ICN_DATA"
```

## Troubleshooting

### Port already in use
Stop any existing processes using the ports or change the configuration.

### Docker image build fails
Ensure you're in the repository root and Docker daemon is running.

### Metrics not appearing in Prometheus
Check that the scrape target is accessible and the job is configured correctly.

## Next steps
Proceed to Workshop 10: Contributor Workflow
