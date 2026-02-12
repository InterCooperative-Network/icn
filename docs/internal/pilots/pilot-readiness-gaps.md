# ICN Pilot Readiness Gaps

**Status**: Living document tracking critical gaps between current implementation and pilot deployment readiness
**Last Updated**: 2025-01-17
**Context**: Post-Phase 14 (Gateway API Complete), Pre-Track C1 (Pilot Deployment)

## Executive Summary

ICN has excellent architectural foundations, but significant gaps exist between "technically sound" and "cooperative-ready". This document tracks the critical missing pieces that must be addressed before pilot deployment with real cooperatives.

**Progress Since Original Analysis**:
- ✅ Gateway API (REST + WebSocket) - Complete
- ✅ Message authentication (SignedEnvelope) - Complete
- ✅ Backup/restore system - Complete
- ✅ Multi-tenant isolation - Complete
- ✅ Network state persistence - Complete

**Remaining Critical Gaps**: 12 high-priority items (detailed below)

---

## 1. Architecture & Integration Gaps

### 1.1 GovernanceActor Not Integrated Into Daemon
**Status**: ✅ **COMPLETE** (2025-01-17)
**Effort**: Already done!

**What Was Implemented**:
- ✅ GovernanceActor fully integrated in `icn-core/src/governance/actor.rs`
- ✅ Spawned in supervisor at lines 959-972 of `supervisor.rs`
- ✅ Time-based proposal closures via scheduler (lines 291-332)
- ✅ Gossip integration for distributed coordination
- ✅ RPC integration for all operations (9 RPC methods)
- ✅ Storage persistence with SledStore
- ✅ Full proposal lifecycle (create → open → vote → close)
- ✅ Integration test passes: `cargo test --test governance_integration -- --ignored`

**RPC Methods Available**:
- `governance.domain.list` / `governance.domain.get` / `governance.domain.create`
- `governance.proposal.list` / `governance.proposal.get` / `governance.proposal.create`
- `governance.proposal.open` / `governance.proposal.close`
- `governance.vote.cast`

**Remaining Gap**:
The GovernanceActor exists and works perfectly. The actual gap is the **Governance → Contract → Ledger flow** (section 1.4 below), which will enable accepted proposals to trigger economic actions.

---

### 1.2 icnctl Bypasses Daemon (Wrong Architecture)
**Status**: 🔴 Critical
**Effort**: Medium (3-4 days)
**Blocks**: Production architecture soundness

**Current State**:
```rust
// WRONG: icnctl opens Sled directly
let store = SledStore::open(&data_dir)?;
store.put("trust_edge", edge)?;
```

**Correct Architecture**:
```
icnctl → gRPC → RpcActor → GovernanceActor/TrustActor/etc. → Store
```

**Why This Matters**:
- Breaks actor model isolation
- Bypasses validation logic in actors
- No access control or auditing
- Race conditions with running daemon
- Cannot work with remote daemons

**Implementation Path**:
1. Audit all `icnctl` commands that touch store
2. Add missing RPC endpoints to `icn-rpc/proto/`
3. Implement RPC handlers in actors
4. Refactor `icnctl` commands to call RPC only
5. Remove Sled dependency from `icnctl` crate
6. Add `--daemon-addr` flag for remote control

**Affected Commands**:
- `icnctl gov domain create/list/show`
- `icnctl gov proposal create/open/close`
- `icnctl gov vote cast/show`
- `icnctl trust add/list`
- `icnctl backup` (currently works correctly via daemon)

**Acceptance Criteria**:
- [ ] All `icnctl` commands use RPC exclusively
- [ ] `icnctl` crate has no Sled dependency
- [ ] Works with remote daemon via `--daemon-addr`
- [ ] All commands pass integration tests against running daemon

---

### 1.3 Trust Graph Underutilized
**Status**: 🟡 High Priority
**Effort**: Large (5-7 days)
**Blocks**: Security model completeness

**Current State**:
- Trust calculation works (transitive trust with decay)
- Trust edges stored and queried
- **BUT**: Trust not deeply integrated into other subsystems

**Missing Trust Integrations**:

| Subsystem | Current State | Missing Integration |
|-----------|---------------|---------------------|
| **Gossip** | Trust scores queryable | Topic access gates not enforced |
| **Governance** | Static membership lists | Trust-based dynamic membership |
| **Contracts** | No participant validation | Trust threshold requirements |
| **RPC** | No rate limiting | Trust-based API quotas |
| **Network** | Accepts all valid DIDs | Trust-based peer acceptance |
| **Federation** | Not implemented | Trust propagation across bridges |

**Implementation Path**:
1. **Gossip Integration**:
   - Add `trust_threshold` field to `TopicMetadata`
   - Validate trust score before allowing subscription
   - Implement in `gossip.rs::subscribe()`

2. **Governance Integration**:
   - Implement `TrustThresholdMembershipResolver`
   - Replace static lists with dynamic trust queries
   - Add trust requirements to `DecisionProfile`

3. **Contract Integration**:
   - Add `required_trust_score` to contract participants
   - Validate on contract invocation
   - Fail execution if trust insufficient

4. **Network Integration**:
   - Add trust check to peer acceptance logic
   - Implement trust-based connection prioritization
   - Persist trusted peers across restarts (see 1.6)

**Acceptance Criteria**:
- [ ] Gossip topics enforce trust thresholds
- [ ] Governance can use trust-based membership
- [ ] Contracts validate participant trust
- [ ] Network prioritizes trusted peers
- [ ] Documentation for trust threshold configuration

---

### 1.4 Governance → Contract → Ledger Flow ✅ COMPLETE
**Status**: ✅ Complete (as of 2025-01-17)
**Actual Effort**: 8 hours (vs 7-10 days estimated)
**Unblocks**: Core value proposition, pilot deployment ready

**Current State**:
- Governance: Proposals and voting work
- Contracts: CCL execution works
- Ledger: Mutual credit transactions work
- **BUT**: No pipeline connecting them

**The Missing Flow**:
```
1. Governance proposal created (e.g., "Approve supplier payment")
2. Community votes (quorum + approval threshold met)
3. Proposal ACCEPTED
4. ??? (nothing happens)
```

**Should Be**:
```
1. Governance proposal: "Pay supplier 5000 credits"
   - Proposal type: Budget
   - Payload: LedgerTransaction { from: coop_did, to: supplier_did, amount: 5000 }
2. Community votes → ACCEPTED
3. GovernanceActor emits ProposalAccepted event
4. ContractRuntime catches event, executes contract
5. Contract invokes ledger.record_transaction()
6. Ledger mutation committed
7. Event broadcast: "Payment executed per governance decision"
8. Audit trail: governance_id → contract_id → ledger_entry_id
```

**Implementation Requirements**:

**1. Governance Payload Types** (`icn-governance/src/types.rs`):
```rust
pub enum ProposalPayload {
    Text(String),
    Budget {
        from: Did,
        to: Did,
        amount: i64,
        currency: String,
    },
    ContractExecution {
        contract_id: String,
        rule: String,
        args: HashMap<String, Value>,
    },
    ConfigChange {
        key: String,
        value: String,
    },
    Membership {
        action: MemberAction, // Add/Remove
        did: Did,
    },
}
```

**2. Event System** (`icn-core/src/events.rs`):
```rust
pub enum SystemEvent {
    ProposalAccepted { proposal_id: String, payload: ProposalPayload },
    ProposalRejected { proposal_id: String },
    ContractExecuted { contract_id: String, outcome: Value },
    LedgerMutation { entry_hash: String, tx: Transaction },
}
```

**3. Event Bus** (actor message passing):
```rust
// GovernanceActor → ContractActor
governance_actor.on_proposal_accepted(|proposal| {
    contract_actor.execute_if_applicable(proposal).await
});

// ContractActor → Ledger
contract_runtime.on_ledger_write(|tx| {
    ledger.record_transaction(tx).await
});
```

**4. Audit Trail** (`icn-store`):
```rust
// Key: governance_decision:{proposal_id}
// Value: { contract_id?, ledger_entry_hash?, timestamp }
```

**Implementation Path**:
1. Extend `ProposalPayload` with actionable types
2. Create event bus infrastructure (`icn-core/src/events.rs`)
3. Wire GovernanceActor to emit events on closure
4. Create ContractActor to listen for governance events
5. Implement contract → ledger bridge
6. Add audit trail storage
7. Create end-to-end integration test:
   - 3 nodes
   - Budget proposal
   - Vote → accept
   - Verify ledger mutation
   - Verify audit trail

**Acceptance Criteria**:
- [x] Budget proposals execute ledger transactions ✅
- [ ] Contract execution proposals work (OPTIONAL - Phase 5, deferred)
- [x] Full audit trail: proposal → ledger entry ✅
- [x] Events broadcast across gossip network ✅
- [x] Integration test demonstrates complete flow ✅ (2 tests passing)
- [x] Documentation with examples ✅

**Implementation Summary**:
- **Phase 1-4**: Event infrastructure, emission, ledger handler, audit trail (6-7 hours)
- **Phase 6**: Integration tests with full validation (1 hour)
- **Files Created**: `icn-core/src/events.rs`, `icn-core/tests/governance_ledger_integration.rs`
- **Files Modified**: `supervisor.rs`, `governance/actor.rs`, `lib.rs`, `Cargo.toml`
- **Test Coverage**: 3 event bus tests + 2 integration tests = 5 total, all passing
- **Critical Fix**: Corrected debit/credit semantics in ledger transactions

See [governance-ledger integration session note](../../development/sessions/2026-01/2025-01-17-governance-ledger-integration.md) for complete details.

---

## 2. Operational Gaps

### 2.1 Ledger Efficiency Issues
**Status**: 🟡 High Priority
**Effort**: Large (7-10 days)
**Blocks**: Scalability, multi-coop support

**Current State**:
- Ledger is correct (double-entry, Merkle-DAG)
- Works for small-scale testing
- **BUT**: Will struggle at production scale

**Missing Optimizations**:

| Feature | Current | Needed For |
|---------|---------|------------|
| **Balance index** | Brute-force DAG traversal | O(1) balance lookups |
| **Compaction** | None (unbounded growth) | Long-running nodes |
| **Anti-spam cost** | Zero transaction cost | Prevent micro-spam attacks |
| **Credit limit auto-adjust** | Static limits | Phase 16 economic safety |
| **Pruning/snapshots** | Full history forever | Disk space management |
| **Multi-ledger** | Single global ledger | Multi-coop nodes |
| **Contract triggers** | Manual transactions | Automated governance flows |

**Implementation Path**:

**Phase 1 - Indexing** (3 days):
```rust
// Add to icn-ledger/src/ledger.rs
pub struct Ledger {
    entries: Vec<JournalEntry>,
    merkle_dag: MerkleDAG,
    // NEW: In-memory balance cache
    balance_index: HashMap<Did, i64>,
}

impl Ledger {
    pub fn get_balance(&self, did: &Did) -> i64 {
        self.balance_index.get(did).copied().unwrap_or(0)
    }

    fn update_balance_index(&mut self, entry: &JournalEntry) {
        // Incremental updates on every append
    }
}
```

**Phase 2 - Multi-Ledger** (2 days):
```rust
// Change from global singleton to namespaced ledgers
pub struct LedgerManager {
    ledgers: HashMap<String, Arc<RwLock<Ledger>>>, // coop_id → Ledger
}

// In supervisor.rs:
let ledger_mgr = LedgerManager::new();
ledger_mgr.create_ledger("food-coop").await?;
ledger_mgr.create_ledger("housing-coop").await?;
```

**Phase 3 - Compaction** (3 days):
```rust
// Periodic task: compact entries older than 1 year
async fn compact_ledger_task(ledger: Arc<RwLock<Ledger>>) {
    loop {
        sleep(Duration::from_secs(86400)).await; // Daily
        let snapshot = ledger.write().create_snapshot()?;
        ledger.write().prune_before(snapshot.cutoff_timestamp)?;
    }
}
```

**Phase 4 - Anti-Spam** (2 days):
```rust
// Add minimum transaction amount or rate limiting
pub struct LedgerConfig {
    min_transaction_amount: i64, // e.g., 1 credit
    max_transactions_per_hour: u32, // e.g., 100
}
```

**Acceptance Criteria**:
- [ ] Balance lookups are O(1)
- [ ] Multi-ledger support works
- [ ] Compaction tested (simulated 1-year history)
- [ ] Anti-spam limits enforced
- [ ] Performance benchmarks documented

---

### 2.2 Storage Schema Normalization
**Status**: 🟡 Medium Priority
**Effort**: Medium (4-5 days)
**Blocks**: Production reliability, migrations

**Current Issues**:
- Mixed serialization formats (bincode, JSON, raw bytes)
- Inconsistent key prefixes
- No indexing strategy
- No schema versioning
- Ephemeral and persistent data mixed
- No per-coop/per-app namespacing

**Target Schema Design**:

```
# Key Prefix Conventions
{namespace}:{entity_type}:{id}

# Examples:
coop:food-coop:config
coop:food-coop:ledger:entry:abc123
coop:food-coop:governance:proposal:prop456
coop:housing:ledger:entry:def789
global:identity:did:icn:alice
global:trust:edge:alice→bob
```

**Schema Versioning**:
```rust
pub struct SchemaVersion {
    version: u32,
    migrations: Vec<MigrationFn>,
}

// In store.rs:
fn open(path: &Path) -> Result<SledStore> {
    let db = sled::open(path)?;
    let current_version = db.get(b"schema_version")?;
    if current_version < REQUIRED_VERSION {
        run_migrations(&db, current_version, REQUIRED_VERSION)?;
    }
    Ok(SledStore { db })
}
```

**Implementation Path**:
1. Document current schema (audit all `.insert()` calls)
2. Design normalized schema with prefixes
3. Implement `SchemaVersion` and migration framework
4. Write migration #1: v1 → v2 (add prefixes)
5. Add namespacing to all store operations
6. Test migration with real data
7. Document schema in `docs/storage-schema.md`

**Acceptance Criteria**:
- [ ] All keys follow `{namespace}:{type}:{id}` convention
- [ ] Schema version tracked in store
- [ ] Migration framework tested
- [ ] Per-coop namespacing works
- [ ] Documentation complete

---

### 2.3 Threat Model Documentation
**Status**: 🟡 Medium Priority
**Effort**: Medium (3-4 days)
**Blocks**: Security audit, production deployment

**Current State**:
- Security is excellent in practice
- Many hardening measures implemented (Phase 7, Phase 9)
- **BUT**: No formal threat model document

**Missing Analysis**:
- Adversary classes (insider, outsider, network, economic)
- Attack surface mapping
- Cross-layer attack analysis
- Governance-political failure modes
- Economic attack vectors (credit abuse, Sybil)
- STRIDE-style systematic analysis

**Implementation Path**:
1. Create `docs/threat-model.md`
2. Define adversary classes:
   - Outsider (no DID, network attacker)
   - Malicious peer (valid DID, Byzantine behavior)
   - Compromised node (stolen keys)
   - Economic attacker (credit manipulation)
   - Governance attacker (voting manipulation)
3. Map attack surface for each layer:
   - Identity (key theft, DID spoofing)
   - Network (DoS, eclipse, MitM)
   - Gossip (spam, replay, fork)
   - Trust (Sybil, edge manipulation)
   - Ledger (double-spend, credit abuse)
   - Governance (vote buying, quorum attacks)
   - Contracts (reentrancy, fuel exhaustion)
4. Document mitigations (reference existing code)
5. Identify gaps in current mitigations
6. Prioritize remaining security work

**Acceptance Criteria**:
- [ ] STRIDE analysis for each component
- [ ] Adversary class definitions
- [ ] Attack surface map
- [ ] Mitigation reference for each threat
- [ ] Gap analysis with priority rankings

---

### 2.4 Kill-Switch & Governance Override
**Status**: 🟡 Medium Priority
**Effort**: Medium (3-4 days)
**Blocks**: Production incident response

**Current State**:
- Governance assumes perfect behavior
- No emergency mechanisms
- No fraud recovery
- No conflict resolution beyond voting

**Missing Capabilities**:
- Emergency veto (super-majority override)
- Fraud lock (freeze accounts pending investigation)
- Temporary freeze (pause operations)
- Quorum call (force immediate vote)
- Conflict resolution (mediation, arbitration)
- Recovery procedures (restore from backup, rollback)

**Implementation Requirements**:

**1. Emergency Proposal Types**:
```rust
pub enum EmergencyProposal {
    FreezeMember { did: Did, reason: String },
    VetoProposal { proposal_id: String },
    ForceClose { proposal_id: String },
    RollbackLedger { before_timestamp: u64 },
}
```

**2. Super-Majority Thresholds**:
```rust
pub struct DecisionProfile {
    quorum: f64,           // e.g., 0.5 (50%)
    approval: f64,         // e.g., 0.66 (66%)
    emergency_quorum: f64, // e.g., 0.75 (75%)
    emergency_approval: f64, // e.g., 0.9 (90%)
}
```

**3. Ledger Freeze**:
```rust
pub struct Ledger {
    frozen_members: HashSet<Did>,
}

impl Ledger {
    pub fn record_transaction(&mut self, tx: Transaction) -> Result<()> {
        if self.frozen_members.contains(&tx.from) {
            return Err(Error::MemberFrozen);
        }
        // ...
    }
}
```

**Implementation Path**:
1. Add emergency proposal types
2. Implement super-majority voting
3. Add member freeze mechanism to ledger
4. Create conflict resolution workflow
5. Document emergency procedures in `docs/incident-response.md`
6. Test emergency scenarios

**Acceptance Criteria**:
- [ ] Emergency proposals require super-majority
- [ ] Member freeze prevents transactions
- [ ] Veto mechanism works
- [ ] Documented recovery procedures
- [ ] Integration test for emergency flow

---

## 3. Usability & Adoption Gaps

### 3.1 Onboarding Flow for Non-Engineers
**Status**: 🔴 Critical
**Effort**: Large (7-10 days)
**Blocks**: Pilot deployment, cooperative adoption

**Current State**:
- Only engineers can set up ICN
- No guided experience
- No defaults for common cooperative types
- No validation or safety checks for beginners

**Missing User Experience**:

**What cooperatives need**:
```
1. Install daemon (one command)
2. Create cooperative (wizard with presets)
3. Invite members (email/QR code)
4. Set up governance rules (templates)
5. Configure credit system (defaults)
6. Deploy first contract (examples)
7. Start using (dashboard)
```

**What they currently get**:
```
1. cargo build --release (requires Rust toolchain)
2. ./target/release/icnd (daemon starts, creates identity)
3. ??? (now what?)
```

**Implementation Requirements**:

**1. Cooperative Creation Wizard** (`icnctl coop init`):
```bash
$ icnctl coop init

Welcome to ICN! Let's set up your cooperative.

What type of cooperative is this?
  1. Worker cooperative (shared labor)
  2. Consumer cooperative (buying club)
  3. Housing cooperative (shared housing)
  4. Platform cooperative (shared services)
  5. Custom

> 1

Great! Worker cooperatives use democratic decision-making.

How many founding members? [3-50]: 5

What currency should you use?
  1. Hours (time-based)
  2. Credits (generic)
  3. USD equivalent
  4. Custom

> 1

Governance model:
  1. Consensus (100% agreement)
  2. Super-majority (75% approval)
  3. Simple majority (51% approval)

> 2

✅ Creating cooperative "my-worker-coop"...
✅ Governance profile: 75% approval, 50% quorum
✅ Credit system: Time-based (hours)
✅ Initial credit limit: 20 hours debt

Next steps:
  1. Invite members: icnctl coop invite member@example.com
  2. View dashboard: http://localhost:8080
  3. Create first proposal: icnctl gov proposal create
```

**2. Cooperative Templates** (`icn-core/src/templates/`):
```rust
pub struct CoopTemplate {
    name: String,
    governance_profile: DecisionProfile,
    credit_config: LedgerConfig,
    default_contracts: Vec<Contract>,
    trust_bootstrap: TrustConfig,
}

pub fn worker_coop_template() -> CoopTemplate {
    CoopTemplate {
        governance_profile: DecisionProfile {
            quorum: 0.5,
            approval: 0.75,
            weighted_voting: false,
        },
        credit_config: LedgerConfig {
            currency: "hours".into(),
            initial_limit: -20,
            max_limit: -500,
        },
        default_contracts: vec![
            Contract::parse(include_str!("contracts/work_acceptance.ccl")),
        ],
        trust_bootstrap: TrustConfig {
            initial_trust: 0.5, // Founding members trust each other
            new_member_trust: 0.1, // New members start lower
        },
    }
}
```

**3. Member Invitation System**:
```bash
$ icnctl coop invite alice@example.com

Generating invite token for alice@example.com...

Share this link with alice@example.com:
  https://icn.local/join?token=eyJhbGc...

Or show this QR code:
  [QR code displayed]

Invite expires in 24 hours.
```

**4. Trust Bootstrap**:
```rust
// When founding members create coop:
for member in founding_members {
    for other in founding_members {
        if member != other {
            trust_graph.add_edge(member, other, 0.5)?;
        }
    }
}
```

**Implementation Path**:
1. Design wizard UX (mockups)
2. Create cooperative templates (worker, consumer, housing, platform)
3. Implement `icnctl coop init` command
4. Create invite token system
5. Add trust bootstrapping logic
6. Write user documentation (`docs/getting-started.md`)
7. User testing with non-technical users

**Acceptance Criteria**:
- [ ] Non-engineer can set up coop in <15 minutes
- [ ] 4 cooperative templates available
- [ ] Invite system works (email + QR code)
- [ ] Trust graph bootstrapped automatically
- [ ] User documentation tested with non-technical users

---

### 3.2 Reference Application
**Status**: 🔴 Critical
**Effort**: Very Large (15-20 days)
**Blocks**: Pilot deployment, usability validation

**Current State**:
- ROADMAP mentions "Tiny Food Co-op Shopper's Club"
- Gateway API exists
- **BUT**: No actual application implementation

**Why This Is Critical**:
- Validates entire stack end-to-end
- Reveals usability issues
- Provides concrete example for other developers
- Demonstrates value proposition to cooperatives
- Essential for Track C1 (pilot deployment)

**Application Spec: "Tiny Food Co-op"**

**Features**:
1. **Member Management**
   - Join coop (invitation flow)
   - View member directory
   - Member profiles with trust scores

2. **Ordering System**
   - Browse products (local suppliers)
   - Place orders (commit to buy)
   - Track order fulfillment

3. **Mutual Credit Ledger**
   - View balance
   - Make payments (for orders, monthly dues)
   - Transaction history

4. **Governance**
   - Submit proposals (new suppliers, policy changes)
   - Vote on active proposals
   - View outcomes

5. **Work Contributions**
   - Log volunteer hours (delivery, admin, etc.)
   - Earn credits for work
   - View contribution leaderboard

**Tech Stack**:
- **Frontend**: React + TypeScript
- **Backend**: ICN Gateway API (REST + WebSocket)
- **Auth**: JWT with DID-based claims
- **Real-time**: WebSocket for order updates, vote results

**Architecture**:
```
[React App] ←→ [ICN Gateway :8080] ←→ [icnd daemon]
                      ↓
              [WebSocket: Real-time events]
```

**Implementation Path**:

**Phase 1 - Basic UI** (5 days):
1. React app scaffold
2. Gateway authentication (challenge/verify)
3. Member directory (read from cooperative)
4. Balance display (ledger API)

**Phase 2 - Ordering** (4 days):
1. Product catalog (stored in governance)
2. Place order (creates ledger transaction)
3. Order status tracking

**Phase 3 - Governance** (3 days):
1. Proposal list
2. Vote submission
3. Real-time vote count updates (WebSocket)

**Phase 4 - Work Logging** (3 days):
1. Work entry form
2. Credit calculation
3. Contribution history

**Phase 5 - Polish** (5 days):
1. Mobile-responsive design
2. Error handling
3. Loading states
4. User testing
5. Documentation

**Acceptance Criteria**:
- [ ] All 5 feature areas implemented
- [ ] Works on mobile and desktop
- [ ] Real-time updates via WebSocket
- [ ] User documentation (screenshots, guides)
- [ ] Deployed and accessible for pilot
- [ ] Non-technical users successfully complete tasks

---

### 3.3 Human Interfaces
**Status**: 🟡 Medium Priority
**Effort**: Very Large (20-30 days)
**Blocks**: Cooperative adoption, debugging, trust building

**Current State**:
- CLI only (`icnctl`)
- No visual tools
- No dashboards
- No explorers

**Missing Tools**:

**1. Admin Dashboard** (for cooperative administrators):
- Member overview (trust scores, balances, activity)
- System health (metrics, peer status)
- Pending proposals (governance queue)
- Recent transactions (ledger activity)
- Alerts (low credit limits, failed votes)

**2. Trust Graph Visualizer**:
- Interactive graph (D3.js force-directed)
- Zoom/pan to explore connections
- Highlight paths between members
- Show trust score calculations
- Export to PNG/SVG

**3. Ledger Explorer**:
- Transaction history (searchable, filterable)
- Balance charts over time
- Credit limit utilization
- Transaction flow diagrams (Sankey)
- Export to CSV

**4. Governance Dashboard**:
- Active proposals (status, votes, countdown)
- Voting history
- Participation metrics (who votes most)
- Proposal templates
- Decision simulation (what-if analysis)

**5. Contract Playground**:
- CCL syntax highlighting
- Real-time validation
- Test execution (with mock data)
- Debugger (step through execution)
- Template library

**Implementation Priority**:
1. **Admin Dashboard** (highest ROI)
2. **Ledger Explorer** (essential for transparency)
3. **Governance Dashboard** (drives engagement)
4. **Trust Graph Visualizer** (educational)
5. **Contract Playground** (developer tool)

**Tech Stack**:
- Framework: Next.js (React + SSR)
- Charts: Recharts, D3.js
- UI: Tailwind CSS, shadcn/ui
- Real-time: WebSocket to Gateway

**Acceptance Criteria**:
- [ ] Admin dashboard deployed and functional
- [ ] Ledger explorer shows full transaction history
- [ ] Governance dashboard tracks active proposals
- [ ] Documentation for each tool

---

## 4. Design & Specification Gaps

### 4.1 Bridge/Federation Model Undefined
**Status**: 🟡 Low Priority (Phase 15)
**Effort**: Very Large (research + design + implementation)
**Blocks**: Multi-cooperative networks

**Current State**:
- Phase 15 says "cross-network bridges"
- No design document
- No specification

**Critical Questions**:
1. **What is federated?**
   - Identity (DIDs)? → Yes, via global DID resolution
   - Trust? → Decay across federation boundaries?
   - Ledger? → Separate or unified?
   - Governance? → Separate domains or nested?
   - Contracts? → Cross-coop execution?

2. **Bridge Architecture**:
   - Passive relays (just forward messages)?
   - Active validators (verify cross-network messages)?
   - Cryptographic attestations?
   - Trusted vs trustless bridges?

3. **Trust Propagation**:
   - Does trust cross federation boundaries?
   - Decay factor for cross-network trust?
   - How to prevent Sybil attacks across federations?

4. **Topic Namespaces**:
   - Global topics (all cooperatives)?
   - Federated topics (subset of cooperatives)?
   - Private topics (single cooperative)?

5. **Security Model**:
   - How to prevent one cooperative from attacking another?
   - Quarantine mechanisms?
   - Reputation systems for federations?

**Recommended Approach**:
1. **Defer until post-pilot** (not critical for single-coop deployment)
2. **Research existing models**:
   - ActivityPub (Mastodon federation)
   - Matrix (chat federation)
   - Cosmos IBC (blockchain bridges)
   - BGP (internet routing)
3. **Write design document** before implementation
4. **Prototype simple bridge** (two cooperatives, identity only)
5. **Validate with pilot cooperatives** (do they actually need this?)

**This is a Phase 15+ concern, not pilot-blocking.**

---

## 5. Priority Matrix

### Critical (Blocks Pilot Deployment)
1. ~~**GovernanceActor integration**~~ ✅ COMPLETE (2-3 days)
2. ~~**Governance→Contract→Ledger flow**~~ ✅ COMPLETE (8 hours actual)
3. **Onboarding wizard** (7-10 days) - **NEXT PRIORITY**
4. **Reference application** (15-20 days) - **CRITICAL PATH**

**Total Critical Path**: ~35-45 days

### High Priority (Needed for Production)
5. **icnctl RPC refactor** (3-4 days)
6. **Trust Graph integration** (5-7 days)
7. **Ledger efficiency** (7-10 days)

**Total High Priority**: ~15-21 days

### Medium Priority (Quality & Reliability)
8. **Storage normalization** (4-5 days)
9. **Threat model** (3-4 days)
10. **Kill-switch mechanisms** (3-4 days)
11. **Admin dashboard** (10-15 days)

**Total Medium Priority**: ~20-28 days

### Low Priority (Future Enhancements)
12. **Bridge/federation design** (deferred to Phase 15)
13. **Full UI suite** (20-30 days, incremental)

---

## 6. Recommended Implementation Order

### Sprint 1: Governance Integration (Week 1-2)
- **Day 1-3**: GovernanceActor implementation
- **Day 4-7**: Governance→Ledger event system
- **Day 8-10**: End-to-end integration test
- **Deliverable**: Budget proposals execute ledger transactions

### Sprint 2: Developer Experience (Week 3-4)
- **Day 11-14**: icnctl RPC refactor
- **Day 15-17**: Trust Graph integration (gossip + governance)
- **Day 18-21**: Ledger multi-tenant + indexing
- **Deliverable**: Production-ready daemon architecture

### Sprint 3: User Onboarding (Week 5-6)
- **Day 22-28**: Cooperative templates + wizard
- **Day 29-35**: Trust bootstrap + invite system
- **Deliverable**: Non-engineers can create cooperatives

### Sprint 4: Reference Application (Week 7-10)
- **Day 36-42**: Food co-op app (Phase 1-3)
- **Day 43-49**: Governance + work logging (Phase 4-5)
- **Day 50-56**: Polish + user testing
- **Deliverable**: Deployable food co-op application

### Sprint 5: Production Readiness (Week 11-12)
- **Day 57-60**: Threat model documentation
- **Day 61-64**: Emergency procedures
- **Day 65-70**: Admin dashboard
- **Deliverable**: Pilot deployment package

**Total Time**: ~12 weeks (3 months)

---

## 7. Acceptance Criteria for Pilot Readiness

A cooperative can use ICN for pilot deployment when:

- [ ] **Non-technical member can set up a cooperative** (onboarding wizard)
- [ ] **Members can vote on decisions** (governance working)
- [ ] **Votes trigger economic actions** (governance→ledger flow)
- [ ] **Members can see their balance** (ledger UI)
- [ ] **Members can make payments** (mutual credit transactions)
- [ ] **System runs reliably for 30 days** (no crashes, data loss)
- [ ] **Clear incident response procedures** (emergency mechanisms)
- [ ] **Trust relationships work** (trust graph integrated)
- [ ] **Reference app demonstrates value** (food co-op or similar)
- [ ] **Documentation for end-users** (not just developers)

---

## 8. Out of Scope (Deliberately Deferred)

These are valid long-term needs but NOT required for pilot:

- ❌ Mobile SDKs (can use web app on mobile browsers)
- ❌ Federation/bridges (single cooperative for pilot)
- ❌ Advanced economic modeling (Phase 16, post-pilot)
- ❌ Contract playground (developer tool, not user-facing)
- ❌ Trust graph visualizer (nice-to-have, not essential)
- ❌ Full UI suite (admin dashboard sufficient for pilot)
- ❌ Multi-language support (English-only for pilot)
- ❌ Offline-first sync (assume reliable internet for pilot)

---

## 9. Risk Assessment

### High Risk (Likely to cause pilot failure)
- **Governance not integrated**: Cooperatives can't make decisions
- **No onboarding flow**: Can't recruit members
- **No reference app**: No clear value demonstration
- **Data loss**: Backup/restore not tested in production

### Medium Risk (Could cause pilot friction)
- **Performance issues**: Ledger lookups too slow
- **UI confusion**: Dashboard unclear, members frustrated
- **Trust not enforced**: Security bypass discovered
- **Emergency procedures unclear**: Crisis causes panic

### Low Risk (Manageable with workarounds)
- **Missing visualizations**: Can export data to external tools
- **CLI-only governance**: Power users can script workflows
- **No mobile app**: Web app works on mobile browsers

---

## 10. Next Actions

**Immediate** (this week):
1. Review and approve this document
2. Create GitHub issues for critical items
3. Set up project board with sprints
4. Begin GovernanceActor implementation

**Short-term** (next 4 weeks):
1. Complete Sprint 1 (Governance Integration)
2. Complete Sprint 2 (Developer Experience)
3. Begin Sprint 3 (Onboarding)

**Medium-term** (next 3 months):
1. Complete reference application
2. Pilot deployment with test cooperative
3. Iterate based on real-world feedback

---

## Appendix A: Completed Work (Reference)

These were identified as gaps in the original analysis but have since been completed:

✅ **Gateway API** (Phase 14)
- REST endpoints for cooperatives, ledger, governance
- WebSocket real-time event streaming
- JWT authentication with DID-based claims
- Rate limiting (100 burst, 10/sec per DID)
- Scope-based authorization

✅ **Message Authentication** (Phase 9)
- SignedEnvelope with Ed25519 signatures
- ReplayGuard with sequence tracking
- NetworkActor automatic verification

✅ **Backup/Restore** (Track B1)
- `icnctl backup` creates encrypted tarballs
- Includes keystore, store, config, state.snapshot
- `icnctl restore` validates and restores

✅ **Multi-Tenant Isolation** (Phase 14)
- Per-coop namespaces in gateway
- Separate ledgers per cooperative
- Event isolation via cooperative ID

✅ **Network State Persistence** (Track B1)
- Graceful restart saves peer X25519 keys
- Vector clocks persisted
- Topic subscriptions restored
- 11 Prometheus metrics for state operations

---

**Document Maintenance**: This is a living document. As items are completed, move them to Appendix A and update the priority matrix.
