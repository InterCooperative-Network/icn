# Deploy ICN Test Network - Host System Instructions

**Status**: ✅ All Docker configuration fixes complete
**Ready**: Deploy from your host system (outside dev container)

---

## Prerequisites Check

On your **host system** (not in the dev container), verify:

```bash
# Check Docker version
docker --version
# Required: Docker 24+

# Check Docker Compose version
docker compose version
# Required: Docker Compose 2.20+

# Check available resources
docker system info | grep -E "CPUs:|Total Memory"
# Required: 4 GB RAM minimum (8 GB recommended)
```

### Optional: Validate Configuration

Before deploying, you can run the validation script to check all configuration files:

```bash
./scripts/validate-test-config.sh
```

This script verifies:
- All required files exist
- Port configurations are correct (9100 for metrics, 9095 for Prometheus)
- Config files use proper settings
- Documentation is consistent
- Prerequisites are installed

---

## Step 1: Build ICN Docker Image

From your **host system**, navigate to the ICN repository and build:

```bash
# Navigate to repository root
cd /path/to/icn

# Build the image (5-10 minutes first time)
docker build -t icn:latest -f Dockerfile icn/

# Verify image was created
docker images | grep icn
# Expected output: icn    latest    <image-id>    <size>
```

**What's happening**: Multi-stage build compiles Rust binaries in a builder container, then creates a minimal runtime image with just the binaries and dependencies.

---

## Step 2: Start 3-Node Test Network

```bash
# Start node1, node2, node3, prometheus, grafana
docker compose -f docker-compose.test.yml up -d

# Wait ~30 seconds for network formation
sleep 30

# Check all containers are running
docker compose -f docker-compose.test.yml ps
```

**Expected output**:
```
NAME                COMMAND                  SERVICE      STATUS       PORTS
icn-grafana         "/run.sh"                grafana      Up           0.0.0.0:3000->3000/tcp
icn-node1           "icnd --config /conf…"   node1        Up (healthy) 0.0.0.0:5001->5001/tcp, 0.0.0.0:8081->8080/tcp, 0.0.0.0:9091->9100/tcp
icn-node2           "icnd --config /conf…"   node2        Up (healthy) 0.0.0.0:5002->5002/tcp, 0.0.0.0:8082->8080/tcp, 0.0.0.0:9092->9100/tcp
icn-node3           "icnd --config /conf…"   node3        Up (healthy) 0.0.0.0:5003->5003/tcp, 0.0.0.0:8083->8080/tcp, 0.0.0.0:9093->9100/tcp
icn-prometheus      "/bin/prometheus --c…"   prometheus   Up (healthy) 0.0.0.0:9095->9090/tcp
```

---

## Step 3: Verify Network Formation

### 3.1 Check Node Logs
```bash
# View node1 logs (look for "Connected to peer" messages)
docker compose -f docker-compose.test.yml logs node1 | tail -20

# View all nodes' connection status
docker compose -f docker-compose.test.yml logs | grep -i "connected\|peer"
```

**What to look for**:
- ✅ "Successfully connected to peer did:icn:..."
- ✅ "Network connections: 2" (each node connected to 2 others)
- ❌ Connection errors or timeouts (troubleshoot if present)

### 3.2 Check Metrics Endpoint
```bash
# Query node1 metrics
curl -s http://localhost:9091/metrics | grep icn_network_connections_active

# Expected output:
# icn_network_connections_active 2

# Check all 3 nodes
for port in 9091 9092 9093; do
  echo "Node on port $port:"
  curl -s http://localhost:$port/metrics | grep icn_network_connections_active
done
```

**Expected**: Each node shows 2 active connections

### 3.3 Interactive Node Access
```bash
# Enter node1 container
docker exec -it icn-node1 bash

# Inside container, check status
icnctl status
icnctl network peers
icnctl id show

# Exit container
exit
```

---

## Step 4: Access Monitoring Dashboards

### Grafana
- **URL**: http://localhost:3000
- **Username**: `admin`
- **Password**: `admin`
- **Dashboard**: Navigate to "Dashboards" → "ICN Dashboard"

**Panels to check**:
- Network Connections (should show 2 per node)
- Byzantine Violations (should be 0)
- Gossip Message Rate (should be increasing)
- Ledger Transaction Rate (initially 0)

### Prometheus
- **URL**: http://localhost:9095
- **Query Examples**:
  ```promql
  icn_network_connections_active
  rate(icn_gossip_announces_sent_total[1m])
  icn_misbehavior_violations_total
  ```

---

## Step 5: Run Basic Tests

### Test 1: Network Connectivity
```bash
# Node1 should see node2 and node3 as peers
docker exec icn-node1 icnctl network peers | wc -l
# Expected: 2

# Verify from all nodes
for node in node1 node2 node3; do
  echo "=== $node ==="
  docker exec icn-$node icnctl network peers
done
```

### Test 2: Gossip Message Propagation
```bash
# Publish a test message from node1
docker exec icn-node1 bash -c 'echo "Hello ICN" | icnctl gossip publish test:messages -'

# Wait for gossip propagation
sleep 5

# Check node2 received it
docker exec icn-node2 icnctl gossip list test:messages
# Expected: Should show the message
```

### Test 3: Metrics Collection
```bash
# Check Prometheus is scraping all targets
curl -s http://localhost:9095/api/v1/targets | jq '.data.activeTargets[] | {instance: .labels.instance, health: .health}'

# Expected output (all "up"):
# {"instance": "node1:9090", "health": "up"}
# {"instance": "node2:9090", "health": "up"}
# {"instance": "node3:9090", "health": "up"}
```

---

## Step 6: Start Byzantine Node (Optional)

```bash
# Start node4 with byzantine profile
docker compose -f docker-compose.test.yml --profile byzantine up -d node4

# Wait for it to join
sleep 30

# Verify node1 sees 3 peers now (including node4)
docker exec icn-node1 icnctl network peers | wc -l
# Expected: 3

# Monitor for Byzantine violations
watch -n 1 'curl -s http://localhost:9091/metrics | grep icn_misbehavior'
```

---

## Troubleshooting

### Issue: Containers Keep Restarting

```bash
# Check logs for crash reason
docker compose -f docker-compose.test.yml logs --tail=100 node1

# Common causes:
# 1. Keystore unlock fails → Check ICN_PASSPHRASE is set
# 2. Port conflicts → Check ports 5001-5004, 9090-9094, 8080-8084, 3000 are free
# 3. Insufficient resources → Increase Docker RAM limit to 8 GB
```

### Issue: Nodes Not Discovering Each Other

```bash
# Check mDNS is working
docker exec icn-node1 icnctl network discover

# Check network connectivity
docker network inspect docker-compose_icn_test

# Restart network
docker compose -f docker-compose.test.yml restart
```

### Issue: Metrics Not Available

```bash
# Check Prometheus targets
curl http://localhost:9095/api/v1/targets | jq '.data.activeTargets[] | {instance: .labels.instance, health: .health}'

# Should show all nodes as "up"

# If down, check firewall rules and container networking
docker compose -f docker-compose.test.yml logs prometheus
```

### Issue: Grafana Dashboard Not Loading

```bash
# Check Grafana logs
docker compose -f docker-compose.test.yml logs grafana

# Re-provision datasource
docker compose -f docker-compose.test.yml restart grafana

# Wait 30 seconds then reload http://localhost:3000
```

### Issue: High Memory Usage

```bash
# Check container stats
docker stats icn-node1 icn-node2 icn-node3

# If >2 GB per node, check for memory leaks
docker exec icn-node1 ps aux
docker exec icn-node1 top -bn1
```

---

## Stop and Cleanup

### Stop Network (Preserve Data)
```bash
docker compose -f docker-compose.test.yml down
```

**Data preserved** in Docker volumes:
- `node1_data`, `node2_data`, `node3_data`, `node4_data`
- `prometheus_data`, `grafana_data`

### Stop and Remove All Data
```bash
# ⚠️ WARNING: This deletes all node data, metrics, and dashboards
docker compose -f docker-compose.test.yml down -v
```

### Complete Cleanup
```bash
# Remove everything including images
docker compose -f docker-compose.test.yml down -v
docker rmi icn:latest
docker system prune -a
```

---

## Success Criteria

After completing steps 1-5, you should have:

- ✅ ICN Docker image built successfully
- ✅ 3 nodes running and healthy
- ✅ Each node connected to 2 peers
- ✅ Prometheus scraping metrics from all nodes
- ✅ Grafana dashboard showing real-time data
- ✅ All containers passing health checks
- ✅ No error messages in logs

---

## Next Steps After Deployment

Once the network is running successfully:

1. **Baseline Testing** (Week 1)
   - Run 10 baseline test scenarios from [INTERNAL_TESTING_PLAN.md](docs/INTERNAL_TESTING_PLAN.md)
   - Document results in test execution log

2. **Byzantine Testing** (Week 2)
   - Start node4 with `--profile byzantine`
   - Run 10 Byzantine attack scenarios
   - Verify detection and quarantine mechanisms

3. **Governance Testing** (Week 2)
   - Run 9 governance scenarios
   - Test domain creation, proposals, voting
   - Verify WebSocket event delivery

4. **Performance Baseline** (Week 2)
   - Establish throughput and latency targets
   - Run 24-hour soak test
   - Monitor resource usage

5. **Go/No-Go Decision**
   - All 38 test scenarios must pass
   - No crashes or panics
   - Byzantine detection < 1 min SLA
   - Governance voting works correctly
   - Network recovers from partitions < 2 min

---

## Configuration Files Reference

### Docker Compose
- **File**: `docker-compose.test.yml`
- **Services**: node1, node2, node3, node4 (optional), prometheus, grafana
- **Networks**: icn_test (172.20.0.0/16)
- **Volumes**: 6 persistent volumes

### Security Configuration
- **Passphrase**: `test-passphrase-insecure-do-not-use-in-production`
- **⚠️ CRITICAL**: ONLY for isolated internal testing
- **Production**: Use secure secrets management (Docker secrets, Vault)

### Port Mapping
| Service    | P2P  | Metrics | Gateway | Dashboard |
|------------|------|---------|---------|-----------|
| node1      | 5001 | 9091    | 8081    | -         |
| node2      | 5002 | 9092    | 8082    | -         |
| node3      | 5003 | 9093    | 8083    | -         |
| node4      | 5004 | 9094    | 8084    | -         |
| prometheus | -    | 9095    | -       | -         |
| grafana    | -    | -       | -       | 3000      |

### Monitoring
- **Prometheus**: http://localhost:9095
- **Grafana**: http://localhost:3000
- **Node Metrics**: http://localhost:9091-9094/metrics
- **Alert Rules**: 25 rules in `monitoring/alert_rules.yml`

---

## Documentation

- **[INTERNAL_TESTING_PLAN.md](docs/INTERNAL_TESTING_PLAN.md)** - 38 test scenarios
- **[TESTING_QUICKSTART.md](docs/TESTING_QUICKSTART.md)** - Quick reference guide
- **[/tmp/DOCKER_FIXES_COMPLETE.md](/tmp/DOCKER_FIXES_COMPLETE.md)** - Fix summary

---

## Support

**Issue**: Check logs first
```bash
docker compose -f docker-compose.test.yml logs <service>
```

**Health**: Check container health
```bash
docker compose -f docker-compose.test.yml ps
```

**Metrics**: Verify Prometheus targets
```bash
curl http://localhost:9095/api/v1/targets
```

---

**Deployment Status**: ✅ **READY**
**Last Updated**: 2025-12-04
**All Fixes Applied**: 5 commits (build context, passphrase, env vars, --bind, gitignore)

🚀 **Run these commands on your host system to start testing!**
