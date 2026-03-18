# Vision-to-Implementation Gap Analysis

**Document ID**: ICN-GAP-VISION-01
**Created**: 2025-11-28
**Purpose**: Map the ICN vision to implementation status and prioritize closing gaps

---

## Executive Summary

ICN is **substrate-complete** (~80% architecturally sound) but **integration-incomplete** (~50% functionally integrated). The core protocols work; they're not connected into the coherent experience the vision describes.

**Critical Finding**: The vision describes "a global cooperative operating layer" but several key pieces are either:
- Code exists but never wired into the daemon
- CLI commands are stubs that print "TODO"
- Security holes that would be unacceptable in production

---

## Vision Components Mapped to Implementation

### 1. Identity You Own, Not Rent

**Vision**: "Device-level DIDs that you actually possess. Identity becomes portable, multi-device, cryptographically verifiable, independent of institutions."

**Implementation Status**:
| Component | Code | Integrated | Notes |
|-----------|------|------------|-------|
| DID generation | ✅ | ✅ | Ed25519 keypairs, `did:icn:` format |
| Keystore (Age-encrypted) | ✅ | ✅ | v2.1 with TLS + X25519 |
| Multi-device DID Document | ✅ | ⚠️ | Code exists, IdentityActor commented out |
| Device add/revoke | ✅ | ❌ | CLI stubs: "TODO: Sign this event" |
| Social recovery | ✅ | ❌ | CLI stubs: "TODO: Publish to gossip" |
| Identity sync via gossip | ✅ | ❌ | `identity:updates` topic defined, not used |

**Gap**: Multi-device identity is CLI-local only. The IdentityActor that would sync across nodes is commented out in `supervisor.rs:315`.

**Files**:
- `icn-identity/src/did_document.rs` - DID Document v2 ✅
- `icn-identity/src/bundle.rs` - IdentityBundle ✅
- `icn-core/src/supervisor.rs:315` - IdentityActor COMMENTED OUT
- `bins/icnctl/src/main.rs:3070,3158` - device approve/revoke stubs

---

### 2. Web-of-Participation (Trust Graph)

**Vision**: "You trust people you've worked with. And you trust who they trust to the degree that makes sense. This creates a dynamic graph of actual participation."

**Implementation Status**:
| Component | Code | Integrated | Notes |
|-----------|------|------------|-------|
| Trust graph storage | ✅ | ✅ | Sled-backed |
| Transitive trust computation | ✅ | ✅ | PageRank-style algorithm |
| Trust-gated rate limiting | ✅ | ✅ | 4 trust classes |
| Trust-gated gossip topics | ✅ | ✅ | AccessControl::TrustGated |
| Trust-gated compute | ✅ | ✅ | MIN_TRUST_SUBMIT/EXECUTE |
| Trust in policies | ✅ | ⚠️ | Direct trust only, not transitive |

**Gap**: Transitive trust computation works but is NOT used in access control policies. Only direct trust scores are checked.

**Files**:
- `icn-trust/src/graph.rs` - Trust graph ✅
- `icn-trust/src/compute.rs` - Transitive computation ✅
- `icn-net/src/rate_limit.rs` - Trust-based rate limiting ✅

---

### 3. Mutual Credit Economy

**Vision**: "No tokens to speculate on. No miners. No global blockchain. It models reciprocity: when you contribute value, you earn credit."

**Implementation Status**:
| Component | Code | Integrated | Notes |
|-----------|------|------------|-------|
| Double-entry ledger | ✅ | ✅ | Merkle-DAG journal |
| Multi-currency | ✅ | ✅ | hours, USD, kWh, etc. |
| Dynamic credit limits | ✅ | ✅ | Trust + history based |
| New member protection | ✅ | ✅ | Progressive ramping |
| Dispute resolution | ✅ | ✅ | File, mediate, resolve |
| Cooperative treasury | ❌ | ❌ | No collective fund concept |

**Gap**: No treasury DID per cooperative. Payments are attributed to individuals, not coops. Communities can't hold collective funds.

**Files**:
- `icn-ledger/src/ledger.rs` - Core ledger ✅
- `icn-ledger/src/credit_policy.rs` - Dynamic limits ✅
- `icn-ledger/src/dispute.rs` - Dispute resolution ✅

---

### 4. Contracts as Computation (CCL)

**Vision**: "A tiny deterministic VM that executes agreements as code: membership rules, cooperative bylaws, cost-sharing, approvals, contributions."

**Implementation Status**:
| Component | Code | Integrated | Notes |
|-----------|------|------------|-------|
| CCL AST & parser | ✅ | ✅ | Contract, Rule, Stmt, Expr |
| Interpreter | ✅ | ✅ | Fuel-metered execution |
| Capability system | ✅ | ✅ | ReadLedger, WriteLedger, ReadTrust |
| Compute execution | ✅ | ✅ | LocalExecutor runs CCL |
| Contract registry | ✅ | ⚠️ | Code exists, needs supervisor wiring |
| CclRef task code | ✅ | ⚠️ | TaskCode::CclRef variant ready |
| Gossip deployment | ❌ | ❌ | Registry sync not yet implemented |

**Progress (2025-11-28)**: Contract registry implemented with persistent storage and in-memory caching. Deploy once, invoke by hash pattern ready. Executor handles CclRef variant.

**Files**:
- `icn-ccl/src/ast.rs` - AST types ✅
- `icn-ccl/src/interpreter.rs` - Execution ✅
- `icn-ccl/src/registry.rs` - ContractRegistry ✅ NEW
- `icn-compute/src/types.rs` - TaskCode::CclRef ✅

---

### 5. Gossip Instead of Gods

**Vision**: "Information flows the way it does in communities: locally first, outward second. This makes the network resilient, unkillable, scalable."

**Implementation Status**:
| Component | Code | Integrated | Notes |
|-----------|------|------------|-------|
| Push announcements | ✅ | ✅ | Broadcast content hashes |
| Pull requests | ✅ | ✅ | Request missing content |
| Anti-entropy | ✅ | ✅ | Bloom filter exchange |
| Vector clocks | ✅ | ✅ | Causal ordering |
| Topic subscriptions | ✅ | ✅ | With notification callbacks |
| Partition healing | ✅ | ✅ | Clock recovery |

**Status**: ✅ **COMPLETE** - Gossip protocol is fully implemented and integrated.

**Files**:
- `icn-gossip/src/gossip.rs` - GossipActor ✅
- `icn-gossip/src/partition.rs` - Partition healing ✅
- `icn-gossip/src/bloom.rs` - Bloom filters ✅

---

### 6. Distributed Compute as a Commons

**Vision**: "Nodes can run actors, migrate workloads, share CPU/GPU cycles, schedule tasks to where trust is highest. Every device becomes part of a global cooperative supercomputer."

**Implementation Status**:
| Component | Code | Integrated | Notes |
|-----------|------|------------|-------|
| Task submission | ✅ | ✅ | Via gossip, RPC, Gateway |
| Trust-gated execution | ✅ | ✅ | MIN_TRUST thresholds |
| CCL executor | ✅ | ✅ | Real interpreter |
| WASM executor | ✅ | ⚠️ | Code exists, no blob storage |
| Placement scoring | ✅ | ✅ | 7-factor algorithm |
| Locality awareness | ✅ | ✅ | RTT + data locality |
| Actor checkpoints | ✅ | ✅ | Stateful migration |
| Cooperative policies | ✅ | ✅ | Quotas, rules, enforcement |
| Blob storage | ❌ | ❌ | Can't store/fetch large files |

**Gap**: WASM executor exists but can't fetch WASM binaries - no blob storage. Tasks limited to inline CCL code.

**Files**:
- `icn-compute/src/actor.rs` - ComputeActor ✅
- `icn-compute/src/executor.rs` - LocalExecutor ✅
- `icn-compute/src/scheduler.rs` - Placement scoring ✅
- `icn-compute/src/wasm_executor.rs:299` - Blob storage TODO

---

### 7. Democratic Federation

**Vision**: "Communities choose whom to federate with. Trust determines bandwidth, permissions, and cooperation. No central server; no company to appease."

**Implementation Status**:
| Component | Code | Integrated | Notes |
|-----------|------|------------|-------|
| P2P networking | ✅ | ✅ | QUIC/TLS with mDNS |
| Manual peer connection | ✅ | ✅ | `icnctl network add-peer` |
| NAT traversal | ✅ | ✅ | STUN + hole punching |
| Federation config | ⚠️ | ❌ | FederationConfig exists, unused |
| Federation protocol | ❌ | ❌ | No cross-network routing |
| Federation CLI | ❌ | ❌ | All commands print "not yet implemented" |

**Gap**: Federation is entirely unimplemented. CLI commands are stubs. No way to connect separate ICN networks.

**Files**:
- `icn-net/src/actor.rs` - NetworkActor ✅
- `icn-net/src/nat.rs` - NAT traversal ✅
- `bins/icnctl/src/main.rs` - federation commands are stubs

---

### 8. Privacy Layer

**Vision**: (Implicit) Metadata protection, traffic obfuscation, anonymous routing.

**Implementation Status**:
| Component | Code | Integrated | Notes |
|-----------|------|------------|-------|
| Topic encryption | ✅ | ❌ | Code exists, not wired in |
| Onion routing | ✅ | ❌ | Code exists, not wired in |
| Traffic obfuscation | ✅ | ❌ | Code exists, not wired in |
| Privacy metrics | ✅ | ❌ | Defined, never incremented |

**Gap**: The entire `icn-privacy` crate exists with 23 passing tests but is NOT in the workspace Cargo.toml and never spawned by the supervisor.

**Files**:
- `icn-privacy/src/topic_encryption.rs` - TopicEncryptor ✅
- `icn-privacy/src/onion_routing.rs` - OnionRouter ✅
- `icn-privacy/src/traffic_obfuscation.rs` - TrafficObfuscator ✅
- `icn/Cargo.toml` - icn-privacy NOT LISTED

---

## Security Issues

### ~~CRITICAL: RPC Endpoint Has No Authentication~~ ✅ FIXED (2025-11-28)

**Location**: `icn-rpc/src/auth.rs` (NEW), `icn-rpc/src/server.rs`

**Resolution**: Full JWT authentication added to RPC server:
- `auth.challenge` / `auth.verify` endpoints for DID-based authentication
- Scope-based authorization for all methods (e.g., `compute:write`, `ledger:read`)
- Authenticated DID tracked for compute task submission (no more `"rpc:unknown"`)
- 6 new tests validating auth flow

**Usage**:
```bash
# 1. Get challenge
curl -X POST http://localhost:5601 -d '{"jsonrpc":"2.0","method":"auth.challenge","params":{"did":"did:icn:..."},"id":1}'

# 2. Sign nonce and verify to get JWT
curl -X POST http://localhost:5601 -d '{"jsonrpc":"2.0","method":"auth.verify","params":{"did":"did:icn:...","signature":"...","scopes":["compute:write"]},"id":2}'

# 3. Use token for authenticated requests
curl -H "Authorization: Bearer <token>" -X POST http://localhost:5601 -d '{"jsonrpc":"2.0","method":"compute.submit",...}'
```

---

## CLI Commands That Are Stubs

| Command | Location | Current Behavior |
|---------|----------|------------------|
| ~~`icnctl status`~~ | :889 | ✅ FIXED - Now connects to daemon via RPC |
| `icnctl recovery setup` | :1285 | "TODO: Publish to gossip" |
| `icnctl recovery initiate` | :1341 | "TODO: Publish to gossip" |
| `icnctl recovery attest` | :1424 | "TODO: Publish to gossip" |
| `icnctl recovery finalize` | :1488 | "TODO: Publish to gossip" |
| `icnctl recovery cancel` | :1556 | "TODO: Publish to gossip" |
| `icnctl device approve` | :3070 | "TODO: Sign this event" |
| `icnctl device revoke` | :3158 | "TODO: Sign this event" |
| `icnctl federation invite` | - | "not yet implemented" |
| `icnctl federation accept` | - | "not yet implemented" |
| `icnctl federation list` | - | "not yet implemented" |
| `icnctl federation remove` | - | "not yet implemented" |

---

## Prioritized Implementation Plan

### Tier 1: Security & Core Integration (Week 1)

1. ~~**RPC Authentication** [CRITICAL]~~ ✅ COMPLETED (2025-11-28)
   - ✅ JWT authentication with challenge-response flow
   - ✅ Scope-based authorization for all methods
   - ✅ Authenticated DID tracked for compute tasks

2. **Enable IdentityActor**
   - Uncomment in supervisor.rs
   - Wire gossip callbacks for `identity:updates`
   - Enable daemon-side multi-device sync

3. **Wire icn-privacy**
   - Add to Cargo.toml workspace members
   - Spawn PrivacyActor in supervisor
   - Enable encrypted topics for sensitive data

### Tier 2: Making Contracts Persistent (Week 2)

4. **Contract Registry**
   - Gossip-based contract deployment
   - Content-addressed storage (hash → contract)
   - Invoke by hash instead of inline code

5. **Blob Storage**
   - Content-addressed blob store
   - Gossip announcements for blob locations
   - Enable WASM binary distribution

### Tier 3: Federation & Treasury (Week 3+)

6. **Federation Protocol**
   - Cross-network peer discovery
   - Trust-gated federation handshake
   - Routing across network boundaries

7. **Cooperative Treasury**
   - Treasury DID per cooperative
   - Governance-approved spending
   - Collective fund accounting

8. **Fix CLI Stubs**
   - ✅ `icnctl status` - Now connects to daemon via RPC (2025-11-28)
   - Implement recovery commands (gossip publish)
   - Implement device commands (sign + broadcast)
   - Implement federation commands

---

## What Real Users Can't Do Today

Despite 760+ passing tests, users cannot:

| Action | Blocker |
|--------|---------|
| Add a second device and sync across nodes | IdentityActor disabled |
| Set up social recovery with trustees | CLI stubs |
| Deploy a contract once and reuse it | No registry |
| Join another ICN network | Federation not implemented |
| Send private messages | Privacy not wired in |
| Submit large WASM files | No blob storage |
| Run long-running stateful services | Actor model incomplete |
| Lock funds in cooperative treasury | Treasury not implemented |
| ~~Trust that RPC requests are authenticated~~ | ✅ FIXED - RPC now has JWT auth |

---

## Success Metrics

After implementing Tier 1-3:

- [x] RPC requires authentication (0 unauthenticated mutations possible) ✅ 2025-11-28
- [ ] Multi-device identity syncs across daemon restarts
- [ ] Private topics are encrypted on the wire
- [ ] Contracts can be deployed once and invoked by hash
- [ ] WASM binaries can be stored and fetched
- [ ] Two ICN networks can federate
- [ ] Cooperatives can hold treasury funds
- [ ] All CLI commands are functional (0 stubs)

---

## Appendix: File Locations

### Commented-Out Code
- `icn-core/src/supervisor.rs:315` - IdentityActor spawn

### Stub Implementations
- `icn-rpc/src/server.rs:1658-1672` - Hardcoded "rpc:unknown" submitter
- `icn-ccl/src/actor.rs:125` - Contract registry TODO
- `icn-compute/src/wasm_executor.rs:299` - Blob storage TODO

### Missing Integration
- `icn-privacy/` - Not in Cargo.toml
- `icn-core/src/supervisor.rs` - No PrivacyActor spawn

### Dead-Letter Queue (Missing)
- `icn-core/src/supervisor.rs:1564` - Failed messages silently dropped

---

**Document Status**: Living document, updated as gaps are closed.
