# ICN Gap Remediation Sprint Plan

**Date:** 2025-12-17  
**Status:** ACTIVE - Sprint 1 Starting  
**Goal:** Close HIGH-priority gaps for production readiness  
**Timeline:** 4 weeks (Sprint 1), then ongoing  

---

## Sprint 1: Quick Wins & Critical Path (Week 1-4)

### Priority Order (Impact × Effort)

1. **Ledger Replay Attack Fix** ⚡ HIGH IMPACT, LOW EFFORT (1-2 days)
2. **Upgrade Coordination Foundation** 🎯 HIGH IMPACT, MEDIUM EFFORT (1 week)  
3. **Trust Graph Gaming - Basic Detection** 🎯 HIGH IMPACT, MEDIUM EFFORT (1 week)
4. **Gossip Amplification Protection** ⚡ MEDIUM IMPACT, LOW EFFORT (2-3 days)
5. **Scalability Testing Setup** 📊 HIGH IMPACT, MEDIUM EFFORT (3-4 days)

### Why This Order?

- **Replay Attack**: Security vulnerability, easy fix, immediate value
- **Upgrade Coordination**: Foundational for all future protocol changes
- **Trust Gaming**: Prevents abuse, essential for pilot scaling
- **Gossip Protection**: Network stability, prevents DoS
- **Scalability Testing**: Parallel track, informs all other work

---

## Task Breakdown

### 1. Ledger Replay Attack Fix ⚡

**File:** `icn/crates/icn-net/src/envelope.rs`, `icn-ledger/src/ledger.rs`  
**Duration:** 1-2 days  
**Priority:** CRITICAL  

**Current Vulnerability:**
```
Time 0:00: Transaction submitted (nonce: 0xABC)
Time 0:01: Transaction processed
Time 0:04: Attacker replays (still within 5min window)
Result: Duplicate processing possible
```

**Fix Strategy:**
```rust
// Phase 1: Nonce finalization in ReplayGuard
pub struct NonceClaim {
    nonce: [u8; 16],
    claimed_at: u64,
    finalized_at: Option<u64>,
    purpose: NonceUsage,
}

pub enum NonceUsage {
    LedgerTransaction { entry_hash: ContentHash },
    NetworkMessage { msg_hash: ContentHash },
    ContractExecution { task_hash: ContentHash },
}

impl ReplayGuard {
    /// Claim nonce (returns false if already claimed)
    pub fn claim_nonce(&mut self, nonce: &[u8; 16], purpose: NonceUsage) -> bool;
    
    /// Finalize nonce (permanent, non-replayable)
    pub fn finalize_nonce(&mut self, nonce: &[u8; 16]) -> Result<()>;
    
    /// Check allows only unclaimed or unfinalizedنonces
    pub fn check_nonce(&self, nonce: &[u8; 16]) -> bool {
        match self.nonces.get(nonce) {
            None => true,  // New nonce
            Some(claim) => claim.finalized_at.is_none(),  // Not finalized yet
        }
    }
}

// Phase 2: Ledger integration
impl Ledger {
    pub fn append_entry(&mut self, entry: JournalEntry) -> Result<()> {
        // Validate entry...
        
        // Finalize nonces from all transactions
        for tx in &entry.transactions {
            if let Some(nonce) = tx.extract_nonce() {
                self.replay_guard.finalize_nonce(&nonce)?;
            }
        }
        
        // Append to journal...
    }
}
```

**Tasks:**
- [ ] Add `finalized_at` field to `ReplayGuard`
- [ ] Implement `finalize_nonce()` method
- [ ] Update ledger to finalize on append
- [ ] Add tests for replay within window
- [ ] Update documentation

**Test Cases:**
```rust
#[test]
fn test_replay_after_finalization() {
    // Submit transaction at T=0
    // Finalize nonce at T=1
    // Attempt replay at T=2 (should fail even within 5min)
    // Assert: Rejected
}

#[test]
fn test_replay_before_finalization() {
    // Submit transaction at T=0
    // Attempt replay at T=1 (before finalization)
    // Assert: Allowed (for idempotency during processing)
}
```

---

### 2. Upgrade Coordination Foundation 🎯

**Files:** `icn/crates/icn-governance/src/proposal.rs`, `icn-core/src/supervisor.rs`  
**Duration:** 1 week  
**Priority:** HIGH  

**Goal:** Enable governance-driven protocol upgrades

**Implementation Plan:**

#### Step 1: New Proposal Type (Day 1-2)

```rust
// icn-governance/src/proposal.rs

pub enum ProposalPayload {
    // ... existing variants
    
    /// Protocol upgrade proposal
    ProtocolUpgrade {
        /// New version (e.g., "1.2.0")
        version: Version,
        
        /// Breaking changes description
        breaking_changes: Vec<String>,
        
        /// Migration guide URL
        migration_guide: Option<String>,
        
        /// Upgrade deadline (Unix timestamp)
        deadline: u64,
        
        /// Minimum version required (nodes below this rejected)
        min_required_version: Option<Version>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    pub fn parse(s: &str) -> Result<Self>;
    pub fn is_compatible_with(&self, other: &Version) -> bool;
}
```

#### Step 2: Version Tracking (Day 2-3)

```rust
// icn-core/src/supervisor.rs

pub struct Supervisor {
    // ... existing fields
    
    /// Current protocol version
    protocol_version: Version,
    
    /// Pending upgrade (if governance approved)
    pending_upgrade: Option<PendingUpgrade>,
    
    /// Version adoption tracker
    version_tracker: VersionTracker,
}

pub struct PendingUpgrade {
    pub proposal_id: ProposalId,
    pub target_version: Version,
    pub approved_at: u64,
    pub deadline: u64,
    pub migration_applied: bool,
}

pub struct VersionTracker {
    /// Peer versions (DID -> Version)
    peer_versions: HashMap<Did, Version>,
    
    /// Update timestamp
    last_updated: HashMap<Did, u64>,
}

impl VersionTracker {
    /// Record peer version from Hello message
    pub fn record_peer_version(&mut self, peer: Did, version: Version);
    
    /// Get adoption rate for a version
    pub fn adoption_rate(&self, version: &Version) -> f64;
    
    /// Get peers below minimum version
    pub fn peers_below_version(&self, min: &Version) -> Vec<Did>;
}
```

#### Step 3: Hello Message Update (Day 3-4)

```rust
// icn-net/src/protocol.rs

pub struct NetworkMessage {
    // ... existing variants
    
    Hello {
        did: Did,
        version: u32,  // <- Already exists, rename to protocol_version
        capabilities: CapabilityFlags,
        x25519_key: [u8; 32],
        
        // NEW: Semantic version
        protocol_semver: Version,
    },
}

// Version negotiation
impl NetworkActor {
    fn handle_hello(&mut self, peer: Did, hello: HelloMsg) -> Result<()> {
        // Record peer version
        self.version_tracker.record_peer_version(peer.clone(), hello.protocol_semver);
        
        // Check compatibility
        if !self.protocol_version.is_compatible_with(&hello.protocol_semver) {
            warn!("Peer {} has incompatible version: {}", peer, hello.protocol_semver);
            
            // If below minimum required, reject connection
            if let Some(min) = &self.min_required_version {
                if hello.protocol_semver < *min {
                    return Err(anyhow!("Version too old: {} < {}", hello.protocol_semver, min));
                }
            }
        }
        
        // Continue with connection...
    }
}
```

#### Step 4: Metrics & Monitoring (Day 4-5)

```rust
// icn-obs/src/metrics.rs

pub fn init_upgrade_metrics() {
    gauge!(
        "icn_upgrade_adoption_rate",
        "Percentage of peers on target version"
    );
    
    gauge!(
        "icn_upgrade_deadline_remaining_seconds",
        "Seconds until upgrade deadline"
    );
    
    counter!(
        "icn_upgrade_rejected_connections_total",
        "Connections rejected due to old version"
    );
}
```

#### Step 5: Governance Integration (Day 5-7)

```rust
// icn-governance/src/execution.rs

impl GovernanceActor {
    async fn execute_protocol_upgrade(&mut self, proposal: ProtocolUpgradeProposal) -> Result<()> {
        info!("Executing protocol upgrade to version {}", proposal.version);
        
        // Store pending upgrade
        let pending = PendingUpgrade {
            proposal_id: proposal.id.clone(),
            target_version: proposal.version.clone(),
            approved_at: now_ms(),
            deadline: proposal.deadline,
            migration_applied: false,
        };
        
        // Notify supervisor
        self.supervisor_tx.send(SupervisorMsg::UpgradeApproved(pending)).await?;
        
        // Announce via gossip
        self.gossip.announce(
            "governance:upgrade",
            bincode::serialize(&pending)?,
        ).await?;
        
        Ok(())
    }
}
```

**Tasks:**
- [ ] Add `ProtocolUpgrade` proposal type
- [ ] Implement `Version` struct with parsing
- [ ] Add `VersionTracker` to supervisor
- [ ] Update Hello message with semver
- [ ] Add version compatibility checks
- [ ] Add metrics (adoption rate, deadline)
- [ ] Integrate with governance execution
- [ ] Write tests (version negotiation, rejection)
- [ ] Document upgrade workflow

---

### 3. Trust Graph Gaming - Basic Detection 🎯

**Files:** `icn/crates/icn-trust/src/`, new file `anomaly.rs`  
**Duration:** 1 week  
**Priority:** HIGH  

**Goal:** Detect and flag suspicious trust patterns

#### Step 1: Anomaly Detector Skeleton (Day 1)

```rust
// icn-trust/src/anomaly.rs

pub struct TrustGraphAnalyzer {
    trust_graph: Arc<RwLock<TrustGraph>>,
    config: AnomalyConfig,
}

pub struct AnomalyConfig {
    /// Circular vouch cycle length threshold
    pub max_cycle_length: usize,  // 5
    
    /// Minimum cycle strength to flag
    pub min_cycle_strength: f64,  // 0.8
    
    /// Sybil cluster detection ratio
    pub sybil_density_ratio: f64,  // 5.0 (internal/external)
    
    /// Rapid growth threshold (% per week)
    pub rapid_growth_threshold: f64,  // 0.5 (50%)
}

pub enum TrustAnomaly {
    CircularVouching {
        cycle: Vec<Did>,
        cycle_strength: f64,
        detected_at: u64,
    },
    SybilCluster {
        cluster: Vec<Did>,
        internal_density: f64,
        external_density: f64,
        detected_at: u64,
    },
    RapidTrustGrowth {
        did: Did,
        growth_rate: f64,
        baseline: f64,
        current: f64,
        detected_at: u64,
    },
}
```

#### Step 2: Circular Vouch Detection (Day 2-3)

```rust
impl TrustGraphAnalyzer {
    /// Detect circular vouching using DFS cycle detection
    pub fn detect_circular_vouching(&self) -> Vec<TrustAnomaly> {
        let graph = self.trust_graph.read().unwrap();
        let mut anomalies = vec![];
        
        // Tarjan's SCC algorithm for cycle detection
        let sccs = self.find_strongly_connected_components(&graph);
        
        for scc in sccs {
            if scc.len() > 1 && scc.len() <= self.config.max_cycle_length {
                // Calculate cycle strength
                let strength = self.calculate_cycle_strength(&scc, &graph);
                
                if strength >= self.config.min_cycle_strength {
                    anomalies.push(TrustAnomaly::CircularVouching {
                        cycle: scc,
                        cycle_strength: strength,
                        detected_at: now_ms(),
                    });
                }
            }
        }
        
        anomalies
    }
    
    fn find_strongly_connected_components(&self, graph: &TrustGraph) -> Vec<Vec<Did>> {
        // Tarjan's algorithm implementation
        // Returns: Vec of SCCs (each SCC is a Vec of DIDs)
        todo!("Implement Tarjan's SCC")
    }
    
    fn calculate_cycle_strength(&self, cycle: &[Did], graph: &TrustGraph) -> f64 {
        let mut total_strength = 0.0;
        let mut edge_count = 0;
        
        for i in 0..cycle.len() {
            let from = &cycle[i];
            let to = &cycle[(i + 1) % cycle.len()];
            
            if let Some(edge) = graph.get_edge(from, to) {
                total_strength += edge.weight;
                edge_count += 1;
            }
        }
        
        if edge_count > 0 {
            total_strength / edge_count as f64
        } else {
            0.0
        }
    }
}
```

#### Step 3: Sybil Cluster Detection (Day 3-4)

```rust
impl TrustGraphAnalyzer {
    /// Detect Sybil clusters (high internal, low external trust)
    pub fn detect_sybil_clusters(&self) -> Vec<TrustAnomaly> {
        let graph = self.trust_graph.read().unwrap();
        let mut anomalies = vec![];
        
        // Use community detection (Louvain algorithm)
        let communities = self.detect_communities(&graph);
        
        for community in communities {
            let internal = self.calculate_internal_density(&community, &graph);
            let external = self.calculate_external_density(&community, &graph);
            
            let ratio = if external > 0.0 {
                internal / external
            } else {
                f64::INFINITY
            };
            
            if ratio >= self.config.sybil_density_ratio {
                anomalies.push(TrustAnomaly::SybilCluster {
                    cluster: community,
                    internal_density: internal,
                    external_density: external,
                    detected_at: now_ms(),
                });
            }
        }
        
        anomalies
    }
    
    fn calculate_internal_density(&self, community: &[Did], graph: &TrustGraph) -> f64 {
        let mut edge_count = 0;
        let max_edges = community.len() * (community.len() - 1);
        
        for from in community {
            for to in community {
                if from != to && graph.has_edge(from, to) {
                    edge_count += 1;
                }
            }
        }
        
        edge_count as f64 / max_edges as f64
    }
    
    fn calculate_external_density(&self, community: &[Did], graph: &TrustGraph) -> f64 {
        let all_dids: Vec<Did> = graph.all_dids().collect();
        let external: Vec<_> = all_dids.iter()
            .filter(|did| !community.contains(did))
            .collect();
        
        if external.is_empty() {
            return 0.0;
        }
        
        let mut edge_count = 0;
        let max_edges = community.len() * external.len();
        
        for from in community {
            for to in &external {
                if graph.has_edge(from, to) {
                    edge_count += 1;
                }
            }
        }
        
        edge_count as f64 / max_edges as f64
    }
}
```

#### Step 4: Rapid Growth Detection (Day 4-5)

```rust
// icn-trust/src/history.rs (new file)

pub struct TrustScoreHistory {
    /// (DID, timestamp) -> trust score
    history: BTreeMap<(Did, u64), f64>,
}

impl TrustScoreHistory {
    /// Record current trust score
    pub fn record(&mut self, did: Did, score: f64) {
        let timestamp = now_ms();
        self.history.insert((did, timestamp), score);
        
        // Prune old entries (>90 days)
        self.prune_old_entries();
    }
    
    /// Get trust score N days ago
    pub fn get_historical(&self, did: &Did, days_ago: u64) -> Option<f64> {
        let target_ts = now_ms() - (days_ago * 24 * 60 * 60 * 1000);
        
        // Find closest entry
        self.history.range(..(did.clone(), target_ts))
            .rev()
            .find(|((d, _), _)| d == did)
            .map(|(_, &score)| score)
    }
    
    /// Calculate growth rate over period
    pub fn growth_rate(&self, did: &Did, days: u64) -> Option<f64> {
        let current = self.get_historical(did, 0)?;
        let baseline = self.get_historical(did, days)?;
        
        if baseline > 0.0 {
            Some((current - baseline) / baseline)
        } else {
            None
        }
    }
}

// icn-trust/src/anomaly.rs (continued)

impl TrustGraphAnalyzer {
    /// Detect rapid trust growth
    pub fn detect_rapid_growth(&self, history: &TrustScoreHistory) -> Vec<TrustAnomaly> {
        let graph = self.trust_graph.read().unwrap();
        let mut anomalies = vec![];
        
        for did in graph.all_dids() {
            if let Some(growth_rate) = history.growth_rate(&did, 7) {  // 7-day window
                if growth_rate >= self.config.rapid_growth_threshold {
                    let baseline = history.get_historical(&did, 7).unwrap_or(0.0);
                    let current = history.get_historical(&did, 0).unwrap_or(0.0);
                    
                    anomalies.push(TrustAnomaly::RapidTrustGrowth {
                        did,
                        growth_rate,
                        baseline,
                        current,
                        detected_at: now_ms(),
                    });
                }
            }
        }
        
        anomalies
    }
}
```

#### Step 5: Integration & Reporting (Day 5-7)

```rust
// icn-core/src/supervisor.rs

pub struct Supervisor {
    // ... existing fields
    
    /// Trust anomaly detector
    trust_analyzer: Option<TrustGraphAnalyzer>,
    
    /// Detected anomalies (for operator review)
    trust_anomalies: Vec<TrustAnomaly>,
}

impl Supervisor {
    /// Run anomaly detection (periodic task, every 24 hours)
    async fn run_trust_anomaly_detection(&mut self) {
        if let Some(analyzer) = &self.trust_analyzer {
            let circular = analyzer.detect_circular_vouching();
            let sybil = analyzer.detect_sybil_clusters();
            let rapid = analyzer.detect_rapid_growth(&self.trust_history);
            
            let mut new_anomalies = Vec::new();
            new_anomalies.extend(circular);
            new_anomalies.extend(sybil);
            new_anomalies.extend(rapid);
            
            if !new_anomalies.is_empty() {
                warn!("Detected {} trust anomalies", new_anomalies.len());
                
                // Store for operator review
                self.trust_anomalies.extend(new_anomalies.clone());
                
                // Publish to governance for community review
                for anomaly in &new_anomalies {
                    self.publish_anomaly_report(anomaly).await;
                }
                
                // Update metrics
                metrics::trust::anomalies_detected_inc(new_anomalies.len());
            }
        }
    }
    
    async fn publish_anomaly_report(&self, anomaly: &TrustAnomaly) {
        // Create governance proposal for community review
        // (Optional, can be manual for now)
    }
}
```

**Tasks:**
- [ ] Create `anomaly.rs` module
- [ ] Implement Tarjan's SCC algorithm
- [ ] Implement circular vouch detection
- [ ] Implement Sybil cluster detection
- [ ] Create `TrustScoreHistory` for tracking
- [ ] Implement rapid growth detection
- [ ] Add periodic detection task (24h)
- [ ] Add metrics (anomalies detected, types)
- [ ] Write tests (each detection type)
- [ ] Document anomaly types & thresholds

---

### 4. Gossip Amplification Protection ⚡

**Files:** `icn/crates/icn-gossip/src/actor.rs`, `icn-net/src/rate_limit.rs`  
**Duration:** 2-3 days  
**Priority:** MEDIUM-HIGH  

**Goal:** Prevent Sybil attack via gossip spam

#### Global Rate Limiter (Day 1-2)

```rust
// icn-gossip/src/rate_limit.rs (new file)

pub struct GlobalRateLimiter {
    /// Time window for rate limiting
    window: Duration,  // 60 seconds
    
    /// Maximum messages per window (across all peers)
    max_messages_per_window: usize,  // 1000
    
    /// Current count (resets every window)
    current_count: AtomicUsize,
    
    /// Window start time
    window_start: AtomicU64,
    
    /// Trust-weighted allocation enabled
    trust_weighted: bool,
}

impl GlobalRateLimiter {
    /// Allocate message budget based on peer trust
    pub fn allocate_budget(&self, peer_trust: f64, total_peers: usize) -> usize {
        if !self.trust_weighted {
            return self.max_messages_per_window / total_peers;
        }
        
        // Trust-weighted allocation
        // Higher trust = more budget
        let base = self.max_messages_per_window as f64 / total_peers as f64;
        let multiplier = 1.0 + peer_trust;  // 1.0-2.0x based on trust
        
        (base * multiplier).round() as usize
    }
    
    /// Check if message should be accepted (global limit)
    pub fn check_global_limit(&self) -> bool {
        let now = now_ms();
        let window_start = self.window_start.load(Ordering::Relaxed);
        
        // Reset window if expired
        if now - window_start > self.window.as_millis() as u64 {
            self.current_count.store(0, Ordering::Relaxed);
            self.window_start.store(now, Ordering::Relaxed);
        }
        
        // Check limit
        let count = self.current_count.fetch_add(1, Ordering::Relaxed);
        count < self.max_messages_per_window
    }
}

// icn-gossip/src/actor.rs

impl GossipActor {
    fn handle_incoming_message(&mut self, from: Did, msg: GossipMessage) -> Result<()> {
        // Check per-peer rate limit (existing)
        if !self.rate_limiter.check_and_consume(&from, 1.0)? {
            warn!("Rate limit exceeded for peer {}", from);
            return Err(anyhow!("Rate limit exceeded"));
        }
        
        // NEW: Check global rate limit
        if !self.global_rate_limiter.check_global_limit() {
            warn!("Global rate limit exceeded (total messages)");
            metrics::gossip::global_rate_limit_exceeded_inc();
            return Err(anyhow!("Global rate limit exceeded"));
        }
        
        // Process message...
    }
}
```

**Tasks:**
- [ ] Create `GlobalRateLimiter`
- [ ] Add trust-weighted budget allocation
- [ ] Integrate with GossipActor
- [ ] Add metrics (global limit exceeded)
- [ ] Add config options (window, max)
- [ ] Write tests (Sybil spam scenario)

---

### 5. Scalability Testing Setup 📊

**Files:** New `icn/loadtest/` directory  
**Duration:** 3-4 days  
**Priority:** HIGH (parallel track)  

**Goal:** Automated load testing to find breaking points

#### Setup (Day 1)

```bash
# Create loadtest directory
mkdir -p icn/loadtest
cd icn/loadtest

# Setup Python virtual environment
python3 -m venv venv
source venv/bin/activate

# Install locust
pip install locust requests websocket-client

# Create locustfile.py
```

#### Locust Test Script (Day 1-2)

```python
# icn/loadtest/locustfile.py

from locust import HttpUser, task, between
import random
import time

class ICNUser(HttpUser):
    wait_time = between(1, 3)
    
    def on_start(self):
        """Initialize user (create DID, connect)"""
        # Generate test DID
        self.did = f"did:icn:test{random.randint(1000, 9999)}"
        
        # Connect to gateway
        response = self.client.post("/api/v1/auth/connect", json={
            "did": self.did
        })
        
        if response.status_code == 200:
            self.token = response.json()["token"]
    
    @task(5)
    def create_trust_edge(self):
        """Create trust edge (common operation)"""
        target = f"did:icn:test{random.randint(1000, 9999)}"
        
        self.client.post("/api/v1/trust", json={
            "target": target,
            "weight": random.uniform(0.5, 1.0)
        }, headers={"Authorization": f"Bearer {self.token}"})
    
    @task(3)
    def ledger_transaction(self):
        """Submit ledger transaction"""
        recipient = f"did:icn:test{random.randint(1000, 9999)}"
        
        self.client.post("/api/v1/ledger/transfer", json={
            "to": recipient,
            "amount": random.randint(1, 100),
            "currency": "hours"
        }, headers={"Authorization": f"Bearer {self.token}"})
    
    @task(2)
    def query_balance(self):
        """Query ledger balance"""
        self.client.get(
            f"/api/v1/ledger/balance/{self.did}",
            headers={"Authorization": f"Bearer {self.token}"}
        )
    
    @task(1)
    def submit_compute_task(self):
        """Submit compute task"""
        self.client.post("/api/v1/compute/tasks", json={
            "wasm_hash": "0x1234...",
            "args": {"input": "test"}
        }, headers={"Authorization": f"Bearer {self.token}"})


class StressTest(HttpUser):
    """High-load stress test"""
    wait_time = between(0.1, 0.5)  # Faster
    
    @task
    def rapid_transactions(self):
        # Burst of transactions
        for _ in range(10):
            self.client.post("/api/v1/ledger/transfer", json={
                "to": "did:icn:stress_target",
                "amount": 1,
                "currency": "hours"
            })
```

#### Test Scenarios (Day 2-3)

```bash
# Scenario 1: Baseline (10 users, 5 minutes)
locust -f locustfile.py --host=http://localhost:8080 \
    --users=10 --spawn-rate=2 --run-time=5m --headless

# Scenario 2: Medium load (100 users, 10 minutes)
locust -f locustfile.py --host=http://localhost:8080 \
    --users=100 --spawn-rate=10 --run-time=10m --headless

# Scenario 3: High load (500 users, 15 minutes)
locust -f locustfile.py --host=http://localhost:8080 \
    --users=500 --spawn-rate=50 --run-time=15m --headless

# Scenario 4: Stress test (1000 users, 20 minutes)
locust -f locustfile.py --host=http://localhost:8080 \
    --users=1000 --spawn-rate=100 --run-time=20m --headless

# Scenario 5: Spike test (0→500→0 users)
# Run manually with web UI: http://localhost:8089
```

#### Analysis Script (Day 3-4)

```python
# icn/loadtest/analyze.py

import pandas as pd
import matplotlib.pyplot as plt

def analyze_loadtest_results(csv_file):
    """Analyze locust CSV results"""
    df = pd.read_csv(csv_file)
    
    # Calculate metrics
    print("=== Load Test Results ===")
    print(f"Total Requests: {len(df)}")
    print(f"Success Rate: {(df['success'] == True).mean() * 100:.2f}%")
    print(f"Mean Response Time: {df['response_time'].mean():.2f}ms")
    print(f"P50 Response Time: {df['response_time'].quantile(0.50):.2f}ms")
    print(f"P95 Response Time: {df['response_time'].quantile(0.95):.2f}ms")
    print(f"P99 Response Time: {df['response_time'].quantile(0.99):.2f}ms")
    print(f"Requests/sec: {len(df) / df['timestamp'].max():.2f}")
    
    # Plot response time distribution
    plt.figure(figsize=(12, 6))
    plt.subplot(1, 2, 1)
    plt.hist(df['response_time'], bins=50)
    plt.xlabel('Response Time (ms)')
    plt.ylabel('Count')
    plt.title('Response Time Distribution')
    
    # Plot over time
    plt.subplot(1, 2, 2)
    df['timestamp'] = pd.to_datetime(df['timestamp'], unit='s')
    df.set_index('timestamp')['response_time'].plot()
    plt.xlabel('Time')
    plt.ylabel('Response Time (ms)')
    plt.title('Response Time Over Time')
    
    plt.tight_layout()
    plt.savefig('loadtest_results.png')
    print("\nPlot saved to loadtest_results.png")

if __name__ == "__main__":
    import sys
    if len(sys.argv) < 2:
        print("Usage: python analyze.py <results.csv>")
        sys.exit(1)
    
    analyze_loadtest_results(sys.argv[1])
```

**Tasks:**
- [ ] Create loadtest directory structure
- [ ] Write locustfile.py with scenarios
- [ ] Create analysis script
- [ ] Run baseline tests (10, 100, 500 users)
- [ ] Document breaking points
- [ ] Create loadtest README

---

## Success Criteria

### Week 1 Complete When:
- [ ] Replay attack fix merged & tested
- [ ] Upgrade coordination foundation implemented
- [ ] Trust anomaly detection running
- [ ] Global rate limiter deployed
- [ ] Load testing framework operational

### Metrics to Track:
- Zero replay attacks in testing
- Upgrade proposals can be created
- Anomalies detected in test data
- Global rate limit triggers under load
- Performance baselines established

---

## Next Sprint Preview (Week 5-8)

### Sprint 2: Scalability Fixes
- Bottleneck fixes from load testing
- Vector clock optimization
- Trust graph caching
- Gossip fan-out optimization

### Sprint 3: Operational Maturity
- Disk monitoring
- Split-brain detection
- Fork mediation workflow
- TURN relay deployment

---

**Status:** Ready to begin ✅  
**Start Date:** 2025-12-17  
**Team:** Open for contributors  

