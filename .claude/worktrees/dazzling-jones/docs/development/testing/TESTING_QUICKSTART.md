# ICN Internal Testing Quick Start Guide

Get your multi-node ICN test network running in under 10 minutes.

---

## ⚠️ Security Warning

**This test environment uses a hardcoded passphrase for convenience:**
```
ICN_PASSPHRASE=test-passphrase-insecure-do-not-use-in-production
```

**DO NOT** use this configuration in production or with real identities. This is **ONLY** for internal testing on isolated networks.

For production deployments, use secure passphrases and proper secret management (e.g., Docker secrets, Kubernetes secrets, HashiCorp Vault).

---

## Prerequisites

- Docker 24+ and Docker Compose 2.20+
- 4 GB RAM minimum (8 GB recommended)
- 10 GB disk space
- **Isolated test network** (not exposed to internet)

---

## Quick Start (Docker Compose)

### 1. Build ICN Docker Image

```bash
cd /workspaces/icn
docker build -t icn:latest -f Dockerfile icn/
```

**Expected time**: 5-10 minutes (first build)

### 2. Start Test Network (3 Honest Nodes)

```bash
docker compose -f docker-compose.test.yml up -d
```

This starts:
- **node1, node2, node3**: Honest ICN nodes
- **prometheus**: Metrics collection at [http://localhost:9095](http://localhost:9095)
- **grafana**: Dashboard at [http://localhost:3000](http://localhost:3000) (default dev creds below)

**Expected time**: 30 seconds

### 3. Verify Network Health

```bash
# Check all containers are running
docker compose -f docker-compose.test.yml ps

# View node1 logs
docker compose -f docker-compose.test.yml logs -f node1

# Check metrics endpoint
curl http://localhost:9091/metrics | grep icn_system_actors_active
```

**Expected output**: `icn_system_actors_active 7` (7 actors running on node1)

**Note**: Without bootstrap peer configuration, nodes won't auto-discover each other (mDNS doesn't work in Docker bridge mode). See [DEPLOY_TEST_NETWORK.md](../../deployment/DEPLOY_TEST_NETWORK.md) for peer connectivity setup.

### 4. Access Monitoring

Open in browser:
- **Grafana**: [http://localhost:3000](http://localhost:3000)
  - Username: `admin`
  - Password: `admin` (development only - use secrets management for production)
  - Navigate to: Dashboards → ICN Dashboard

- **Prometheus**: [http://localhost:9095](http://localhost:9095)
  - Try query: `icn_network_connections_active`

### 5. Run Basic Commands

```bash
# Enter node1 container
docker exec -it icn-node1 bash

# Inside container:
icnctl status
icnctl network peers
icnctl id show

# Exit container
exit
```

### 6. Start Byzantine Node (Optional)

```bash
# Start node4 with byzantine profile
docker compose -f docker-compose.test.yml --profile byzantine up -d node4

# Check it joined
docker exec -it icn-node1 icnctl network peers
```

**Expected**: 3 peers listed (node2, node3, node4)

### 7. Stop Test Network

```bash
# Stop all containers
docker compose -f docker-compose.test.yml down

# Stop and remove volumes (DELETES ALL DATA)
docker compose -f docker-compose.test.yml down -v
```

---

## Running Test Scenarios

### Baseline Functionality Tests

#### Test 1.1: Network Formation

```bash
# Start network
docker compose -f docker-compose.test.yml up -d

# Wait 30 seconds for mDNS discovery
sleep 30

# Check connections on each node
for node in node1 node2 node3; do
  echo "=== $node ==="
  docker exec icn-$node icnctl network peers | wc -l
done
```

**Expected**: Each node reports 2 peers

#### Test 1.2: Trust Graph Sync

```bash
# Set trust edges from node1
docker exec -it icn-node1 bash
icnctl trust set did:icn:node2 0.8
icnctl trust set did:icn:node3 0.7

# Wait for gossip sync
sleep 30

# Verify on node2
docker exec -it icn-node2 icnctl trust list
```

**Expected**: node2 sees trust edges for node3

#### Test 1.3: Gossip Message Propagation

```bash
# Node1 publishes messages
docker exec -it icn-node1 bash
icnctl gossip publish test:messages "Hello from node1" --count 10

# Check node2 received
docker exec -it icn-node2 icnctl gossip list test:messages
```

**Expected**: 10 messages visible on node2

#### Test 1.4: Ledger Transaction

```bash
# Node1 creates transaction
docker exec -it icn-node1 bash
icnctl ledger transfer did:icn:node2 50

# Wait for sync
sleep 30

# Check balances on all nodes
for node in node1 node2 node3; do
  echo "=== $node ==="
  docker exec icn-$node icnctl ledger balance did:icn:node1
  docker exec icn-$node icnctl ledger balance did:icn:node2
done
```

**Expected**: Consistent balances (node1=-50, node2=+50) across all nodes

#### Test 1.6-1.9: Governance Tests

```bash
# Node1 creates governance domain
docker exec -it icn-node1 bash
icnctl gov domain create \
  --domain-id "test-coop" \
  --name "Test Cooperative" \
  --members "did:icn:node1,did:icn:node2,did:icn:node3"

# Create proposal
icnctl gov proposal create \
  --domain-id "test-coop" \
  --title "Upgrade to v2" \
  --kind text

# Open for voting
icnctl gov proposal open --proposal-id <ID>

# Vote from each node
# Node1
icnctl gov vote cast --proposal-id <ID> --choice for

# Node2
docker exec -it icn-node2 icnctl gov vote cast --proposal-id <ID> --choice for

# Node3
docker exec -it icn-node3 icnctl gov vote cast --proposal-id <ID> --choice against

# Close proposal
icnctl gov proposal close --proposal-id <ID>

# Check outcome
icnctl gov vote show --proposal-id <ID>
```

**Expected**:
- Tally: 2 For, 1 Against
- Outcome: Accepted (66% > 50%)

### Byzantine Detection Tests

#### Test 2.1: Invalid Signature Attack

This requires modifying node4's code to forge signatures - covered in automated test suite.

#### Test 2.2: Replay Attack (Manual Simulation)

```bash
# Start network with byzantine node
docker compose -f docker-compose.test.yml --profile byzantine up -d

# Monitor violations
watch -n 1 'curl -s http://localhost:9091/metrics | grep icn_misbehavior'
```

**Expected**: Violations increment when replay attack detected

#### Test 2.7: Governance Vote Manipulation

```bash
# Create governance domain with nodes 1-3
docker exec -it icn-node1 bash
icnctl gov domain create --domain-id "coop" --members "did:icn:node1,did:icn:node2,did:icn:node3"

# Create and open proposal
PROPOSAL_ID=$(icnctl gov proposal create --domain-id "coop" --title "Test" --kind text | grep -oP 'ID: \K\S+')
icnctl gov proposal open --proposal-id $PROPOSAL_ID

# Node4 tries to vote (not a member)
docker exec -it icn-node4 icnctl gov vote cast --proposal-id $PROPOSAL_ID --choice for
```

**Expected**: Vote rejected with error "not a member of domain"

---

## Monitoring Queries

### Prometheus Queries

Access [http://localhost:9095/graph](http://localhost:9095/graph) and try:

```promql
# Network connections per node
icn_network_connections_active

# Gossip message rate
rate(icn_gossip_announces_sent_total[1m])

# Byzantine violations
icn_misbehavior_violations_total

# Quarantined peers
icn_misbehavior_quarantined_peers

# Ledger entries
icn_ledger_entries_total

# Compute tasks
rate(icn_compute_tasks_completed_total[5m])

# Governance proposals by state
icn_governance_proposals_total
```

### Grafana Dashboards

1. Open [http://localhost:3000](http://localhost:3000)
2. Login ( default creds - change for prod)
3. Navigate: Dashboards → ICN Dashboard

**Panels to watch**:
- Network Connections (should be 2 per node)
- Byzantine Violations (should be 0 for honest nodes)
- Quarantined Peers (should be 0 initially)
- Gossip Message Rate
- Ledger Transaction Rate
- Governance Proposal Outcomes

---

## Performance Testing

### Gossip Throughput Test

```bash
# Terminal 1: Start monitoring
docker compose -f docker-compose.test.yml logs -f node1 | grep "gossip"

# Terminal 2: Publish 1000 messages
docker exec -it icn-node1 bash
for i in {1..1000}; do
  icnctl gossip publish load:test "Message $i"
done

# Check latency in Grafana
# Query: histogram_quantile(0.99, rate(icn_gossip_latency_seconds_bucket[5m]))
```

**Expected**: P99 latency < 500ms

### Ledger Transaction Volume

```bash
# Automated script (run inside node1)
docker exec -it icn-node1 bash

cat > /tmp/ledger_load.sh <<'EOF'
#!/bin/bash
for i in {1..1000}; do
  icnctl ledger transfer did:icn:node2 1 &
  if [ $((i % 50)) -eq 0 ]; then
    wait  # Batch every 50 transactions
  fi
done
wait
EOF

chmod +x /tmp/ledger_load.sh
time /tmp/ledger_load.sh
```

**Expected**: ~50 tx/sec, all successful

---

## Troubleshooting

### Nodes not discovering each other

```bash
# Check mDNS is working
docker exec -it icn-node1 icnctl network discover

# Check logs for connection errors
docker compose -f docker-compose.test.yml logs node1 | grep -i error

# Restart network
docker compose -f docker-compose.test.yml restart
```

### Metrics not available

```bash
# Check Prometheus targets
curl http://localhost:9095/api/v1/targets | jq '.data.activeTargets[] | {instance, health}'

# Should show all nodes as "up"
```

### Grafana dashboard not loading

```bash
# Check Grafana logs
docker compose -f docker-compose.test.yml logs grafana

# Re-provision datasource
docker compose -f docker-compose.test.yml restart grafana
```

### High memory usage

```bash
# Check container stats
docker stats icn-node1 icn-node2 icn-node3

# If >2 GB per node, check for memory leaks
docker exec -it icn-node1 bash
ps aux
top
```

### Containers keep restarting

```bash
# Check logs for crash reason
docker compose -f docker-compose.test.yml logs --tail=100 node1

# Common causes:
# - Keystore unlock fails (needs --keystore-passphrase)
# - Port conflicts (another process using 5001-5004)
# - Insufficient resources (increase Docker RAM limit)
```

---

## Automated Test Execution

### Run Full Test Suite

```bash
cd /workspaces/icn
./scripts/run-internal-tests.sh
```

This script:
1. Builds Docker images
2. Starts test network
3. Runs all 38 test scenarios
4. Collects logs and metrics
5. Generates test report: `test-results/report.md`

**Expected time**: 2-3 hours

### Run Specific Test Category

```bash
# Baseline tests only
./scripts/run-internal-tests.sh --category baseline

# Byzantine tests only
./scripts/run-internal-tests.sh --category byzantine

# Governance tests only
./scripts/run-internal-tests.sh --category governance

# Performance tests only
./scripts/run-internal-tests.sh --category performance
```

---

## Next Steps

1. **Run Baseline Tests**: Verify all 10 baseline scenarios pass
2. **Run Byzantine Tests**: Simulate attacks and verify detection
3. **Run Governance Tests**: Test voting workflows end-to-end
4. **Performance Baseline**: Establish throughput and latency targets
5. **Soak Test**: 24-hour stability test with continuous load
6. **Review Metrics**: Analyze Grafana dashboards for anomalies
7. **Document Findings**: Fill out test execution log
8. **Go/No-Go Decision**: Ready for pilot or more hardening needed?

---

## Useful Commands Reference

```bash
# Docker Compose
docker compose -f docker-compose.test.yml up -d          # Start network
docker compose -f docker-compose.test.yml down           # Stop network
docker compose -f docker-compose.test.yml logs -f node1  # View logs
docker compose -f docker-compose.test.yml ps             # Status
docker compose -f docker-compose.test.yml restart node1  # Restart node

# Container access
docker exec -it icn-node1 bash                           # Shell into container
docker exec -it icn-node1 icnctl status                  # Run command

# Monitoring
curl http://localhost:9091/metrics                       # Node1 metrics
curl http://localhost:9095/api/v1/query?query=up        # Prometheus query
curl http://localhost:3000/api/health                    # Grafana health

# Cleanup
docker compose -f docker-compose.test.yml down -v        # Remove volumes
docker system prune -a                                   # Full cleanup
```

---

## Support

- **Documentation**: See `/docs/INTERNAL_TESTING_PLAN.md` for full test plan
- **Issues**: Check logs in `docker compose logs`
- **Metrics**: Prometheus at `localhost:9090`, Grafana at `localhost:3000`
- **Community**: (Add Slack/Discord link if available)

---

**Testing Status**: Ready to Execute
**Last Updated**: 2025-12-04
