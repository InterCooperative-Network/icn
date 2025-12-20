# ICN Architecture Wiring Audit
**Date:** 2025-12-17
**Scope:** Verify all components are properly integrated in supervisor

## Actor Initialization Flow

### ✅ Core Actors (Spawned)

1. **IdentityActor** - Line 190
   - Provides signing and trust graph access
   - Handles multi-device identity
   - Status: ✅ Spawned

2. **NetworkActor** (icn-net) - Line 577
   - QUIC/TLS transport
   - mDNS discovery
   - NAT traversal
   - Status: ✅ Spawned

3. **GossipActor** (icn-gossip)
   - Initialized via `init_gossip::init_gossip_services()` - Line 156
   - Handles topic subscriptions
   - Push/pull/anti-entropy
   - Status: ✅ Spawned

4. **Ledger** (icn-ledger)
   - Initialized via `init_ledger::init_ledger_services()` - Line 173
   - Double-entry mutual credit
   - Quarantine system
   - Status: ✅ Spawned

5. **GovernanceActor** - Line 1826
   - Proposals and voting
   - Domain management
   - Status: ✅ Spawned

6. **ComputeActor** (icn-compute) - Line 2530
   - Task execution
   - Trust-gated scheduling
   - Status: ✅ Spawned

7. **StewardActor** (icn-steward) - Line 3049
   - SDIS enrollment and recovery
   - VUI registry
   - Status: ⚠️ Conditionally spawned (only if configured)

8. **UpgradeActor** - Line 1848
   - Version coordination
   - Upgrade negotiation
   - Status: ✅ Spawned

9. **DisputeActor** (icn-ccl) - Line 2736
   - Dispute management
   - Resolution workflow
   - Status: ✅ Spawned

### ✅ Supporting Services (Initialized)

10. **TrustGraph** (icn-trust)
    - Initialized via `init_trust::init_trust_services()` - Line 143
    - Web-of-participation computation
    - Status: ✅ Initialized

11. **MisbehaviorDetector** (icn-security)
    - Initialized in init_trust - Line 145
    - Byzantine fault detection
    - Status: ✅ Initialized

12. **RecoveryStore**
    - Initialized in init_trust - Line 146
    - Identity recovery messages
    - Status: ✅ Initialized

13. **SnapshotCoordinator** (icn-snapshot)
    - Initialized via `init_snapshot::init_snapshot_coordinator()` - Line 149
    - Distributed Chandy-Lamport snapshots
    - Status: ✅ Initialized

14. **ContractRuntime** (icn-ccl)
    - Initialized in init_ledger - Line 186
    - CCL interpreter
    - Status: ✅ Initialized

15. **ContractActor** (icn-ccl)
    - Initialized in init_ledger - Line 187
    - Contract execution coordination
    - Status: ✅ Initialized

16. **RpcServer** (icn-rpc)
    - Initialized via `init_rpc::init_rpc_server()` 
    - gRPC API
    - Status: ✅ Spawned

17. **Gateway** (icn-gateway)
    - Spawned separately in bins/icnd
    - REST + WebSocket API
    - Status: ✅ External process (not in supervisor)

### ❌ Missing: Not Integrated

18. **CoopActor** (icn-coop) 
    - **Status: ❌ NOT SPAWNED**
    - Exists: `icn/crates/icn-coop/src/actor.rs`
    - Purpose: Cooperative lifecycle management
    - Current workaround: Gateway has in-memory CoopManager
    - **Impact:** No persistent cooperative state, no gossip sync

19. **CommunityActor** (icn-community)
    - **Status: ❌ NOT SPAWNED**
    - Exists: `icn/crates/icn-community/src/`
    - Purpose: Multi-coop communities
    - **Impact:** Community features not available

20. **FederationActor** (icn-federation)
    - **Status: ❌ NOT SPAWNED**
    - Exists: `icn/crates/icn-federation/src/`
    - Purpose: Inter-cooperative federation
    - **Impact:** Cross-federation features not available

21. **PrivacyActor** (icn-privacy)
    - **Status: ❌ NOT SPAWNED**
    - Exists: `icn/crates/icn-privacy/src/`
    - Purpose: Privacy-preserving operations
    - **Impact:** Privacy features not available

22. **ZkpActor** (icn-zkp)
    - **Status: ❌ NOT SPAWNED**
    - Exists: `icn/crates/icn-zkp/src/` (stub)
    - Purpose: Zero-knowledge proofs
    - **Impact:** ZKP features not available (expected - Phase S4)

