# Internal Testing Plan: Multi-Node Network Validation

**Status**: Ready to Execute
**Phase**: Pre-Pilot Internal Testing
**Timeline**: 1-2 weeks
**Prerequisites**: Phase 18 Complete ✅

---

## Objectives

Validate the ICN system in realistic multi-node scenarios before pilot deployment:

1. **Functional Correctness**: All components work together as designed
2. **Byzantine Detection**: Misbehavior is detected and isolated correctly
3. **Performance**: System handles realistic workloads efficiently
4. **Resilience**: Recovers gracefully from failures and network partitions
5. **Monitoring**: Metrics and alerts provide actionable operational visibility
6. **Stability**: System runs continuously without crashes or memory leaks

---

## Test Environment Architecture

### Network Topology

```
                  ┌─────────────┐
                  │   Metrics   │
                  │  (Prometheus│
                  │  + Grafana) │
                  └──────┬──────┘
                         │
        ┌────────────────┼────────────────┐
        │                │                │
   ┌────▼────┐      ┌────▼────┐     ┌────▼────┐
   │ Node 1  │◄────►│ Node 2  │◄───►│ Node 3  │
   │(Honest) │      │(Honest) │     │(Honest) │
   └────┬────┘      └────┬────┘     └────┬────┘
        │                │                │
        └────────────────┼────────────────┘
                         │
                    ┌────▼────┐
                    │ Node 4  │
                    │(Byzantine│
                    │ Attacker)│
                    └─────────┘
```

**Node Configuration**:
- **Nodes 1-3**: Honest nodes with full trust relationships
- **Node 4**: Byzantine node for attack simulation
- **Metrics Server**: Centralized Prometheus + Grafana on separate host

### Infrastructure Setup

**Option 1: Docker Compose (Recommended for Development)**

Use the production-like configuration in `docker-compose.test.yml`:

```bash
# Build and start
docker build -t icn:latest -f Dockerfile icn/
docker compose -f docker-compose.test.yml up -d

# Check status
docker compose -f docker-compose.test.yml ps
```

**Port mapping summary** (see `docker-compose.test.yml` for complete config):
| Service | P2P | Metrics (host:container) | Gateway |
|---------|-----|--------------------------|---------|
| node1 | 5001 | 9091:9100 | 8081 |
| node2 | 5002 | 9092:9100 | 8082 |
| node3 | 5003 | 9093:9100 | 8083 |
| node4 | 5004 | 9094:9100 | 8084 |
| prometheus | - | 9095:9090 | - |
| grafana | - | - | 3000 |

**Note**: ICN daemon runs metrics on port 9100 internally.

**Option 2: Local Processes (Quick Start)**
```bash
# Terminal 1: Node 1
cat > /tmp/icn-node1.toml <<EOF
data_dir = "/tmp/icn-node1"
[network]
listen_addr = "127.0.0.1:5001"
[observability]
metrics_port = 9101
health_port = 18081
log_level = "info"
EOF
cargo run --release --bin icnd -- --config /tmp/icn-node1.toml

# Terminal 2: Node 2
cat > /tmp/icn-node2.toml <<EOF
data_dir = "/tmp/icn-node2"
[network]
listen_addr = "127.0.0.1:5002"
[observability]
metrics_port = 9102
health_port = 18082
log_level = "info"
EOF
cargo run --release --bin icnd -- --config /tmp/icn-node2.toml

# Terminal 3: Node 3
cat > /tmp/icn-node3.toml <<EOF
data_dir = "/tmp/icn-node3"
[network]
listen_addr = "127.0.0.1:5003"
[observability]
metrics_port = 9103
health_port = 18083
log_level = "info"
EOF
cargo run --release --bin icnd -- --config /tmp/icn-node3.toml

# Terminal 4: Node 4 (Byzantine)
cat > /tmp/icn-node4.toml <<EOF
data_dir = "/tmp/icn-node4"
[network]
listen_addr = "127.0.0.1:5004"
[observability]
metrics_port = 9104
health_port = 18084
log_level = "info"
EOF
cargo run --release --bin icnd -- --config /tmp/icn-node4.toml

# Terminal 5: Prometheus
prometheus --config.file=monitoring/prometheus.yml

# Terminal 6: Grafana
grafana-server --config monitoring/grafana.ini
```

**Option 3: Kubernetes (Production-like)**
- Deploy to local k8s cluster (minikube/kind)
- 4 ICN pods with persistent volumes
- Prometheus operator for metrics
- Grafana for dashboards

---

## Test Scenarios

### 1. Baseline Functionality Tests (2 days)

**Goal**: Verify all components work correctly in normal operation

#### 1.1 Network Formation
- **Setup**: Start all 4 nodes sequentially
- **Test**:
  - Nodes discover each other via mDNS
  - QUIC/TLS connections established
  - DID-TLS binding verified
  - X25519 key exchange completed
- **Success Criteria**:
  - All nodes see 3 peers in `icnctl network peers`
  - No connection errors in logs
  - `icn_network_connections_active` = 3 per node

#### 1.2 Trust Graph Sync
- **Setup**: Node1 sets trust edges
- **Test**:
  - Node1: Trust Node2=0.8, Node3=0.7, Node4=0.3
  - Wait for gossip propagation (30s)
  - Query trust from other nodes
- **Success Criteria**:
  - All nodes have consistent trust graph
  - Trust class calculations correct (Partner, Federated, Isolated)
  - `icn_trust_edges_total` matches across nodes

#### 1.3 Gossip Message Propagation
- **Setup**: Nodes 1-3 subscribed to topic "test:messages"
- **Test**:
  - Node1 publishes 100 messages to "test:messages"
  - Measure time to convergence
- **Success Criteria**:
  - All nodes receive all 100 messages within 5 seconds
  - Vector clocks show correct causal ordering
  - No duplicate message processing
  - `icn_gossip_announces_received_total` = 100 on nodes 2-3

#### 1.4 Ledger Transaction Sync
- **Setup**: Initialize ledgers on all nodes
- **Test**:
  - Node1 → Node2: Transfer 50 credits
  - Node2 → Node3: Transfer 30 credits
  - Node3 → Node1: Transfer 20 credits
  - Wait for gossip sync (60s)
- **Success Criteria**:
  - All nodes have identical ledger state
  - Balances correct: Node1=-30, Node2=+20, Node3=+10
  - No quarantined entries
  - `icn_ledger_entries_total` = 3 on all nodes

#### 1.5 Compute Task Execution
- **Setup**: Node1 submits task, Node2 configured as executor
- **Test**:
  - Submit CCL contract: `rule example() { return 42; }`
  - Node2 claims and executes
  - Result propagated via gossip
- **Success Criteria**:
  - Task completes within 10 seconds
  - Result verified: output = 42
  - Payment settled: Node1 → Node2
  - `icn_compute_tasks_completed_total` = 1

#### 1.6 Governance Domain Creation & Sync
- **Setup**: All nodes running
- **Test**:
  - Node1 creates governance domain "test-coop" with members: Node1, Node2, Node3
  - Wait for gossip propagation (30s)
  - Query domain from all nodes
- **Success Criteria**:
  - All nodes see the same domain configuration
  - Membership list correct (3 members)
  - Governance profile = cooperative_default (1-member-1-vote)
  - Domain created event in gossip logs

#### 1.7 Proposal Lifecycle (Simple Majority)
- **Setup**: Governance domain "test-coop" with 3 members
- **Test**:
  - Node1 creates text proposal: "Should we upgrade to Protocol v2?"
  - Node1 opens proposal for voting
  - Node1 votes: For
  - Node2 votes: For
  - Node3 votes: Against
  - Node1 closes proposal
  - Check outcome
- **Success Criteria**:
  - Proposal created and synced to all nodes
  - All votes recorded (3 total)
  - Tally: 2 For, 1 Against, 0 Abstain
  - Outcome: Accepted (66% approval > 50% threshold)
  - Proposal state transitions: Draft → Open → Voting → Closed
  - All events propagated via governance gossip

#### 1.8 Proposal Lifecycle (Quorum Failure)
- **Setup**: Governance domain "test-coop" with 3 members, quorum = 100%
- **Test**:
  - Node1 creates budget proposal
  - Node1 opens proposal
  - Only Node1 and Node2 vote (2/3 = 66% turnout)
  - Node1 closes proposal
- **Success Criteria**:
  - Tally recorded correctly
  - Outcome: Rejected (failed quorum requirement)
  - Rejection reason: "quorum not met (66% < 100%)"
  - All nodes see consistent outcome

#### 1.9 Governance WebSocket Events
- **Setup**: Gateway running, WebSocket client connected
- **Test**:
  - Create domain, proposal, cast votes
  - Monitor WebSocket for events
- **Success Criteria**:
  - Client receives: GovernanceDomainCreated event
  - Client receives: GovernanceProposalCreated event
  - Client receives: GovernanceProposalOpened event
  - Client receives: GovernanceVoteCast events (3 total)
  - Client receives: GovernanceProposalClosed event
  - All events have correct timestamps and payload

#### 1.10 Graceful Restart
- **Setup**: Nodes running with active workload
- **Test**:
  - Send SIGTERM to Node2
  - Wait for graceful shutdown
  - Restart Node2
  - Resume workload
- **Success Criteria**:
  - State snapshot saved (vector clocks, subscriptions, X25519 keys)
  - Node2 rejoins network within 30 seconds
  - No message loss or duplicates
  - `icn_snapshot_save_duration_seconds` < 0.1s

---

### 2. Byzantine Behavior Detection Tests (3 days)

**Goal**: Verify misbehavior is detected and isolated correctly

#### 2.1 Invalid Signature Attack
- **Setup**: Node4 (Byzantine) attempts to forge signatures
- **Test**:
  - Modify Node4 to send messages with invalid Ed25519 signatures
  - Send 5 forged messages to Node1
- **Expected Behavior**:
  - Node1 detects InvalidSignature violations (5 total)
  - Node1's misbehavior detector records violations
  - Node4's reputation drops (1.0 → 0.75 after 5 violations)
  - Node4 NOT quarantined yet (threshold = 0.5)
- **Success Criteria**:
  - `icn_misbehavior_violations_total{violation_type="InvalidSignature"}` = 5
  - Node1 reputation for Node4 = 0.75 ± 0.01
  - Grafana panel shows violations

#### 2.2 Replay Attack Detection
- **Setup**: Node4 attempts to replay captured messages
- **Test**:
  - Capture signed message from Node2
  - Node4 replays same message 3 times to Node1
- **Expected Behavior**:
  - First message accepted (valid)
  - Subsequent replays detected by sequence number tracking
  - ReplayAttack violation recorded (severity 10, auto-ban)
  - Node4 immediately banned (reputation → 0.0)
- **Success Criteria**:
  - `icn_misbehavior_violations_total{violation_type="ReplayAttack"}` ≥ 1
  - `icn_misbehavior_banned_peers` = 1
  - `icn_misbehavior_auto_bans_total` = 1
  - Node4 isolated from network (no further messages accepted)

#### 2.3 Ledger Fork Attack
- **Setup**: Node4 attempts double-spending
- **Test**:
  - Node4 creates two conflicting ledger entries with same parent
  - Entry A: Transfer 100 credits to Node1
  - Entry B: Transfer 100 credits to Node2 (conflicting)
  - Gossip both entries to network
- **Expected Behavior**:
  - First entry accepted by honest nodes
  - Second entry detected as conflict
  - ConflictingLedgerEntries violation (severity 10, auto-ban)
  - Conflicting entry quarantined
  - Node4 auto-banned
- **Success Criteria**:
  - `icn_ledger_entries_quarantined` = 1
  - `icn_misbehavior_violations_total{violation_type="ConflictingLedgerEntries"}` = 1
  - Node4 banned on all honest nodes
  - Ledger state consistent across Node1-3

#### 2.4 Compute Result Forgery
- **Setup**: Node4 claims task but returns forged result
- **Test**:
  - Node1 submits task with known result (e.g., hash computation)
  - Node4 claims task, returns incorrect result with invalid signature
  - Node1 verifies result
- **Expected Behavior**:
  - Signature verification fails
  - FailedComputeVerification violation recorded (severity 5)
  - Node4's reputation decreases
  - No payment issued (verification failed)
- **Success Criteria**:
  - `icn_compute_signatures_invalid_total` = 1
  - `icn_misbehavior_violations_total{violation_type="FailedComputeVerification"}` = 1
  - Node4 reputation reduced
  - Task remains in "failed" state

#### 2.5 ACL Violation Spam
- **Setup**: Node4 attempts rapid unauthorized subscriptions
- **Test**:
  - Node1 creates private topic (TrustClass::Partner, requires trust > 0.9)
  - Node4 (trust 0.3) attempts 15 subscription requests in 10 seconds
- **Expected Behavior**:
  - All subscription attempts rejected (ACL violation)
  - 15 violations recorded
  - After 10 violations in 1 hour: Node4 quarantined
  - Node4's trust reduced via trust penalty callback
- **Success Criteria**:
  - `icn_misbehavior_violations_total` = 15
  - `icn_misbehavior_quarantined_peers` = 1
  - Node4 quarantined (reputation < 0.5)
  - Grafana shows rate-limit quarantine event

#### 2.6 Multi-Node Byzantine Isolation
- **Setup**: Node4 sends conflicting statements to different nodes
- **Test**:
  - Node4 → Node1: "Balance(Alice) = 100"
  - Node4 → Node2: "Balance(Alice) = 200" (conflicting)
  - Nodes gossip received statements
- **Expected Behavior**:
  - Both Node1 and Node2 independently detect conflict
  - ConflictingSignedStatements violation (severity 10, auto-ban)
  - Node4 banned on both nodes
  - Node3 learns of ban via reputation gossip (future Phase 19)
- **Success Criteria**:
  - Node1 and Node2 both ban Node4 independently
  - `icn_misbehavior_auto_bans_total` ≥ 2 (across network)
  - Node4 isolated from all honest nodes

#### 2.7 Governance Vote Manipulation
- **Setup**: Governance domain with Node1, Node2, Node3; Node4 NOT a member
- **Test**:
  - Node1 creates proposal
  - Node1 opens proposal
  - Node4 attempts to vote (not a member)
- **Expected Behavior**:
  - Node4's vote rejected (not in membership list)
  - Vote not recorded in tally
  - Potential violation recorded (attempted unauthorized action)
- **Success Criteria**:
  - Vote count remains 0
  - Node4's vote not in proposal.votes map
  - Error logged: "unauthorized voter"
  - Proposal outcome unaffected

#### 2.8 Governance Double Voting Attack
- **Setup**: Governance domain with Node1, Node2, Node3
- **Test**:
  - Node1 creates proposal
  - Node1 opens proposal
  - Node2 votes: For
  - Node2 attempts to vote again: Against (double vote)
- **Expected Behavior**:
  - First vote accepted and recorded
  - Second vote rejected (already voted)
  - Tally shows only 1 vote from Node2
  - Warning logged: "double vote attempt"
- **Success Criteria**:
  - Vote count = 1 for Node2 (not 2)
  - Tally: 1 For, 0 Against (first vote wins)
  - Double vote attempt logged
  - No violation recorded (benign error, could be network duplicate)

#### 2.9 Governance Proposal Spam
- **Setup**: Node4 (low trust) creates governance domain
- **Test**:
  - Node4 creates 50 proposals in 60 seconds
  - All proposals in same domain
- **Expected Behavior**:
  - Proposals accepted (governance has no built-in rate limit yet)
  - Gossip propagates all proposals
  - Note: This validates current behavior; Phase 19 may add rate limits
- **Success Criteria**:
  - 50 proposals created successfully
  - No crashes or out-of-memory errors
  - Gossip convergence time measured (should be < 2 minutes)
  - Resource usage within acceptable bounds

#### 2.10 Governance Conflicting Outcomes
- **Setup**: Network partition scenario with governance
- **Test**:
  - Create governance domain with Node1, Node2, Node3
  - Create proposal
  - Partition: [Node1, Node2] vs [Node3]
  - Node1 and Node2 vote: For (2/3 = 66%)
  - Node1 closes proposal (sees Accepted with 2/3 votes)
  - Node3 (in partition) votes: Against
  - Heal partition
  - Both sides have different tallies
- **Expected Behavior**:
  - Before healing: Both partitions see different state
  - After healing: Gossip reconciles votes
  - Final tally: 2 For, 1 Against
  - Outcome recalculated if needed (may require manual review)
- **Success Criteria**:
  - All votes eventually recorded (3 total after healing)
  - Conflicting outcomes detected (if any)
  - Operator alerted to review proposal
  - Note: This is a known edge case; Phase 19 may add partition-aware voting

---

### 3. Performance & Load Tests (2 days)

**Goal**: Validate system performance under realistic and stress conditions

#### 3.1 Gossip Throughput
- **Setup**: 3-node network
- **Test**:
  - Node1 publishes 1000 messages/sec to 5 topics
  - Measure propagation latency and throughput
  - Run for 10 minutes
- **Success Criteria**:
  - Median latency < 100ms
  - P99 latency < 500ms
  - No message loss
  - CPU usage < 50% per node
  - Memory growth < 100 MB over 10 minutes

#### 3.2 Ledger Transaction Volume
- **Setup**: 3-node network
- **Test**:
  - Simulate 100 concurrent users making transactions
  - 50 transactions/sec sustained for 5 minutes
  - Random transaction amounts and participants
- **Success Criteria**:
  - All transactions processed without conflicts
  - Ledger convergence within 60 seconds
  - No quarantined entries (all valid)
  - `icn_ledger_entries_total` = 15,000 (50 tx/s × 300s)
  - Balances sum to zero (double-entry invariant)

#### 3.3 Compute Task Queue
- **Setup**: 1 submitter (Node1), 2 executors (Node2, Node3)
- **Test**:
  - Submit 500 compute tasks with varying fuel limits
  - Tasks include: math operations, string parsing, conditional logic
  - Measure task completion rate and latency
- **Success Criteria**:
  - All 500 tasks complete successfully
  - Median completion time < 5 seconds
  - Tasks distributed evenly (Node2 ≈ 250, Node3 ≈ 250)
  - No task timeouts or executor crashes
  - All payments settled correctly

#### 3.4 Byzantine Detection Under Load
- **Setup**: 3 honest nodes + 1 Byzantine node
- **Test**:
  - Normal workload: 100 tx/sec + 50 compute tasks/min
  - Byzantine workload: 10 violations/sec (mixed types)
  - Run for 30 minutes
- **Success Criteria**:
  - Byzantine node quarantined within 1 minute
  - Byzantine node auto-banned after critical violation
  - Honest nodes maintain throughput (< 10% degradation)
  - No false positives (honest nodes not flagged)
  - `icn_misbehavior_violations_total` > 600 (10/s × 60s)

#### 3.5 Governance Load Test
- **Setup**: 3-node network with 1 governance domain (3 members)
- **Test**:
  - Create 100 proposals concurrently
  - Each node opens 33-34 proposals
  - All 3 nodes vote on all proposals (300 votes total)
  - Close all proposals
  - Measure convergence time
- **Success Criteria**:
  - All 100 proposals created successfully
  - All 300 votes recorded correctly
  - All proposals reach consistent outcome across nodes
  - Convergence time < 5 minutes
  - No vote loss or duplication
  - Gossip overhead acceptable (< 50% CPU)

#### 3.6 Memory Leak Detection
- **Setup**: 4-node network with continuous workload
- **Test**:
  - Run for 24 hours with:
    - 10 tx/sec ledger transactions
    - 5 compute tasks/min
    - 100 gossip messages/sec
  - Monitor memory usage every hour
- **Success Criteria**:
  - Memory growth < 500 MB over 24 hours
  - No unbounded growth (linear or exponential)
  - Resident set size (RSS) stable after initial ramp-up
  - No out-of-memory crashes

---

### 4. Resilience & Fault Tolerance Tests (2 days)

**Goal**: Verify system recovers gracefully from failures

#### 4.1 Node Crash Recovery
- **Setup**: 4-node network with active workload
- **Test**:
  - Kill Node2 with SIGKILL (unclean shutdown)
  - Wait 2 minutes
  - Restart Node2
  - Verify recovery
- **Success Criteria**:
  - Node2 rejoins network within 60 seconds
  - Gossip anti-entropy fetches missed messages
  - Ledger state restored via sync
  - No data loss or corruption
  - Workload resumes normally

#### 4.2 Network Partition
- **Setup**: 4-node network split into 2 partitions
- **Test**:
  - Partition 1: Node1, Node2
  - Partition 2: Node3, Node4
  - Block traffic between partitions for 5 minutes
  - Heal partition
  - Measure convergence time
- **Success Criteria**:
  - Nodes detect partition via heartbeat timeouts
  - Both partitions continue operating independently
  - After healing: gossip anti-entropy reconciles state
  - Ledger conflicts detected and quarantined (if any)
  - Full convergence within 2 minutes of healing

#### 4.3 Byzantine Node Recovery
- **Setup**: Node4 quarantined due to violations
- **Test**:
  - Stop Node4
  - Upgrade Node4 to honest behavior
  - Restart Node4
  - Wait for reputation decay
- **Expected Behavior**:
  - Node4 starts with quarantined reputation (loaded from snapshot - Phase 19)
  - Reputation decays at 0.01 points/hour
  - After ~50 hours: reputation > 0.5 (out of quarantine)
  - Node4 regains network privileges gradually
- **Success Criteria**:
  - Reputation decay works as expected
  - Node4 can rejoin network after sufficient decay
  - No manual intervention required (automatic recovery)

#### 4.4 Disk Full Scenario
- **Setup**: Node2 with limited disk quota (1 GB)
- **Test**:
  - Fill disk with gossip entries, ledger data
  - Monitor behavior as disk approaches full
- **Expected Behavior**:
  - Node logs disk space warnings
  - Gossip entries evicted (LRU) to free space
  - Node does NOT crash
  - Graceful degradation (may miss some gossip entries)
- **Success Criteria**:
  - No crashes or panics
  - Node continues operating with reduced capacity
  - Alerts triggered in Grafana
  - Operator notified to add capacity

#### 4.5 Prometheus/Grafana Failure
- **Setup**: Running network with monitoring
- **Test**:
  - Stop Prometheus server
  - Continue workload for 10 minutes
  - Restart Prometheus
- **Success Criteria**:
  - ICN nodes continue operating normally (monitoring is non-critical)
  - No crashes due to metrics export failures
  - After Prometheus restart: metrics collection resumes
  - No data loss (metrics buffered or dropped gracefully)

---

### 5. Operational Procedures Tests (1 day)

**Goal**: Validate operational workflows documented in deployment guide

#### 5.1 Backup & Restore
- **Test**:
  - Create backup using `icnctl backup create /tmp/backup.tar.gz.age`
  - Verify backup contains: keystore, store, config, state.snapshot
  - Corrupt Node2's data directory
  - Restore using `icnctl backup restore /tmp/backup.tar.gz.age`
- **Success Criteria**:
  - Backup completes without errors
  - Restore recovers all data
  - Node2 rejoins network successfully
  - No data loss (ledger, trust graph, gossip subscriptions)

#### 5.2 Version Upgrade
- **Test**:
  - Build new version with protocol version bump
  - Rolling upgrade: Node1 → Node2 → Node3 → Node4
  - Verify version negotiation and compatibility
- **Success Criteria**:
  - Nodes negotiate correct protocol version
  - Backward compatibility maintained (if within compatibility window)
  - No downtime for network (rolling upgrade successful)
  - `icn_network_version_negotiation_success_total` increments

#### 5.3 Security Incident Response
- **Test**:
  - Detect Node4 compromised (simulated via intentional violations)
  - Follow incident response playbook:
    1. Identify compromised node via Grafana alerts
    2. Investigate logs and violation records
    3. Confirm Byzantine behavior
    4. Verify automatic isolation (ban)
    5. Document incident
- **Success Criteria**:
  - Incident detected within 1 minute (via alert)
  - Compromised node automatically banned
  - No manual intervention required for isolation
  - Playbook steps executable and accurate

#### 5.4 Capacity Planning
- **Test**:
  - Monitor resource usage under load
  - Calculate capacity limits:
    - Max transactions/sec before latency degrades
    - Max gossip topics before memory pressure
    - Max concurrent compute tasks per executor
- **Success Criteria**:
  - Baseline capacity metrics documented
  - Recommendations for scaling (add nodes, increase resources)
  - Grafana dashboards show capacity utilization

---

## Monitoring Setup

### Prometheus Configuration

```yaml
# monitoring/prometheus.yml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'icn-nodes'
    static_configs:
      - targets:
          - 'node1:9090'
          - 'node2:9090'
          - 'node3:9090'
          - 'node4:9090'
    relabel_configs:
      - source_labels: [__address__]
        target_label: instance
        regex: '([^:]+):.*'
        replacement: '$1'
```

### Key Metrics to Monitor

**Byzantine Detection**:
- `icn_misbehavior_violations_total` (by violation_type, did)
- `icn_misbehavior_quarantined_peers`
- `icn_misbehavior_banned_peers`
- `icn_misbehavior_auto_bans_total`

**Network Health**:
- `icn_network_connections_active`
- `icn_network_messages_sent_total`
- `icn_network_messages_received_total`
- `icn_network_messages_rate_limited_total`

**Gossip Performance**:
- `icn_gossip_announces_sent_total`
- `icn_gossip_requests_sent_total`
- `icn_gossip_entries_total` (per topic)
- `icn_gossip_vector_clock_updates_total`

**Ledger Consistency**:
- `icn_ledger_entries_total`
- `icn_ledger_entries_quarantined`
- `icn_ledger_balances_total`

**Compute Layer**:
- `icn_compute_tasks_submitted_total`
- `icn_compute_tasks_completed_total`
- `icn_compute_task_duration_seconds`
- `icn_compute_signatures_invalid_total`

**Governance**:
- `icn_governance_domains_total`
- `icn_governance_proposals_total` (by state: draft, open, closed)
- `icn_governance_votes_total`
- `icn_governance_proposals_accepted_total`
- `icn_governance_proposals_rejected_total`

### Alert Rules

```yaml
# monitoring/alert_rules.yml
groups:
  - name: byzantine_detection
    interval: 10s
    rules:
      - alert: ByzantineNodeDetected
        expr: icn_misbehavior_quarantined_peers > 0
        for: 1m
        annotations:
          summary: "Byzantine node quarantined"
          description: "{{ $value }} nodes have been quarantined due to misbehavior"

      - alert: AutoBanTriggered
        expr: increase(icn_misbehavior_auto_bans_total[5m]) > 0
        annotations:
          summary: "Critical violation auto-ban"
          description: "A node has been auto-banned for critical violations"

      - alert: HighViolationRate
        expr: rate(icn_misbehavior_violations_total[5m]) > 1
        for: 5m
        annotations:
          summary: "High violation rate detected"
          description: "{{ $value }} violations/sec detected"

  - name: network_health
    interval: 10s
    rules:
      - alert: NetworkPartition
        expr: icn_network_connections_active < 2
        for: 2m
        annotations:
          summary: "Possible network partition"
          description: "Node has less than 2 active connections"

      - alert: HighMessageLoss
        expr: rate(icn_network_messages_failed_total[5m]) > 0.1
        for: 5m
        annotations:
          summary: "High message failure rate"
          description: "{{ $value }} messages/sec failing"

  - name: ledger_consistency
    interval: 30s
    rules:
      - alert: LedgerConflict
        expr: icn_ledger_entries_quarantined > 0
        annotations:
          summary: "Ledger entries quarantined"
          description: "{{ $value }} ledger entries in quarantine (possible fork attack)"
```

---

## Test Execution Plan

### Week 1: Infrastructure & Baseline Tests

**Day 1-2**: Environment Setup
- Create Docker Compose configuration
- Build ICN Docker image
- Set up Prometheus + Grafana
- Deploy 4-node test network
- Verify metrics collection

**Day 3-4**: Baseline Functionality Tests
- Network formation (1.1)
- Trust graph sync (1.2)
- Gossip propagation (1.3)
- Ledger sync (1.4)
- Compute execution (1.5)
- Graceful restart (1.6)

**Day 5**: Performance Baseline
- Gossip throughput (3.1)
- Ledger transaction volume (3.2)
- Establish performance baselines

### Week 2: Byzantine Detection & Stress Tests

**Day 6-8**: Byzantine Behavior Tests
- Invalid signature attack (2.1)
- Replay attack detection (2.2)
- Ledger fork attack (2.3)
- Compute result forgery (2.4)
- ACL violation spam (2.5)
- Multi-node isolation (2.6)

**Day 9**: Performance Under Load
- Compute task queue (3.3)
- Byzantine detection under load (3.4)

**Day 10**: Resilience Tests
- Node crash recovery (4.1)
- Network partition (4.2)
- Byzantine node recovery (4.3)

**Day 11**: Operational Procedures
- Backup & restore (5.1)
- Security incident response (5.3)
- Capacity planning (5.4)

**Day 12**: Soak Test
- Memory leak detection (3.5)
- 24-hour stability test
- Final validation

---

## Success Criteria

### Mandatory Requirements (Blockers for Pilot)

- [  ] All 38 test scenarios pass without failures (10 baseline + 10 Byzantine + 6 performance + 5 resilience + 4 operational + 3 governance)
- [  ] No crashes or panics during any test
- [  ] Byzantine nodes detected and isolated within SLA (1 minute for critical violations)
- [  ] No false positives (honest nodes never quarantined/banned)
- [  ] Ledger consistency maintained across all nodes (no undetected forks)
- [  ] Governance voting works correctly (no vote loss, correct outcomes)
- [  ] Graceful restart preserves all critical state
- [  ] Network recovers from partitions within 2 minutes
- [  ] 24-hour soak test completes with stable memory usage

### Performance Benchmarks (Targets)

- [  ] Gossip latency: median < 100ms, P99 < 500ms
- [  ] Ledger transactions: 50 tx/sec sustained
- [  ] Compute tasks: 10 tasks/min per executor
- [  ] Byzantine detection overhead: < 0.1% CPU
- [  ] Memory overhead: < 500 MB growth over 24 hours
- [  ] Network partition recovery: < 2 minutes to full convergence

### Optional Goals (Nice-to-Have)

- [  ] 1000 tx/sec ledger throughput (stretch goal)
- [  ] 1-week soak test (extended stability validation)
- [  ] Chaos testing with random node failures
- [  ] Performance comparison vs. baseline (pre-Phase 18)

---

## Test Artifacts

### Required Deliverables

1. **Test Execution Log** - Detailed results for each scenario
2. **Performance Report** - Throughput, latency, resource usage metrics
3. **Bug Report** - Any issues discovered with severity classification
4. **Grafana Screenshots** - Key metrics during Byzantine attacks
5. **Incident Timeline** - Step-by-step analysis of Byzantine detection events
6. **Capacity Recommendations** - Resource requirements for pilot deployment
7. **Go/No-Go Decision** - Final readiness assessment

### Bug Tracking Template

```markdown
## Bug Report: [Title]

**Severity**: Critical / Major / Minor
**Test Scenario**: [e.g., 2.2 Replay Attack Detection]
**Environment**: [Docker Compose / Local Processes]

**Steps to Reproduce**:
1. Start 4-node network
2. ...

**Expected Behavior**:
[What should happen]

**Actual Behavior**:
[What actually happened]

**Logs**:
```
[Relevant log excerpts]
```

**Metrics**:
[Screenshots or PromQL queries]

**Impact**:
[Pilot blocker? Workaround available?]

**Root Cause Analysis**:
[If known]

**Proposed Fix**:
[If known]
```

---

## Risk Assessment

### High-Risk Areas

1. **Reputation Persistence** (Known Limitation)
   - **Risk**: Reputation reset on restart enables attackers to rejoin
   - **Mitigation**: Phase 19 will add persistent storage; for testing, document workaround (manual ban via config)

2. **Cross-Node Reputation Sync** (Known Limitation)
   - **Risk**: Byzantine node could exploit different reputations on different nodes
   - **Mitigation**: Test multi-node isolation (scenario 2.6) validates independent detection

3. **Network Partition Handling**
   - **Risk**: Ledger forks during partition may not be detected immediately
   - **Mitigation**: Quarantine mechanism catches conflicts on partition healing

4. **Compute Task Timeouts**
   - **Risk**: Long-running tasks may not be killed after timeout
   - **Mitigation**: Verify timeout enforcement in scenario 3.3

### Medium-Risk Areas

1. **Gossip Convergence Time**
   - **Risk**: Large networks may have slow convergence
   - **Mitigation**: Measure and document convergence time in scenario 1.3

2. **Trust Graph Scalability**
   - **Risk**: Trust computation may be slow with many edges
   - **Mitigation**: Performance test with realistic trust graph size

3. **Metrics Export Overhead**
   - **Risk**: High-cardinality metrics may impact performance
   - **Mitigation**: Monitor CPU usage during load tests

---

## Timeline

**Week 1**: Infrastructure setup + baseline tests
**Week 2**: Byzantine detection + stress tests

**Total Duration**: 2 weeks (12 working days)

**Go/No-Go Decision**: End of Week 2

---

## Next Steps

1. **Create Docker Compose setup** - Start with Option 1 (recommended)
2. **Build test automation scripts** - Bash scripts for each test scenario
3. **Set up CI/CD integration** - Automated nightly test runs
4. **Establish baseline metrics** - Run baseline tests first to set performance targets
5. **Execute test plan systematically** - Follow day-by-day schedule
6. **Document all findings** - Comprehensive test report
7. **Make Go/No-Go decision** - Ready for pilot or additional hardening needed?

---

**Status**: Ready to Execute
**Owner**: [Assign owner]
**Start Date**: [TBD]
**Target Completion**: [Start Date + 2 weeks]
