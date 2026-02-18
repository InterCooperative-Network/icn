# ICN Repo Reality Map

> **Generated**: 2026-02-17 | **Branch**: `main` @ `6943792e`
> **Purpose**: Evidence-driven inventory of every ICN plane. Every claim cites a file path.
> If a claim cannot cite a file path, it is labeled **[SPECULATION]**.

---

## Table of Contents

1. [Red-Flag Questions (Answered First)](#1-red-flag-questions)
2. [Governance → Effects → Ledger Saga](#2-governance--effects--ledger-saga)
3. [Identity Plane](#3-identity-plane)
4. [Network Plane](#4-network-plane)
5. [Gossip Plane](#5-gossip-plane)
6. [Ledger/Economics Plane](#6-ledgereconomics-plane)
7. [Compute Plane](#7-compute-plane)
8. [Gateway/UI Surface](#8-gatewayui-surface)
9. [Privacy/Crypto Plane](#9-privacycrypto-plane)
10. [Idempotency Layer Map](#10-idempotency-layer-map)
11. [Critical Gap Summary](#11-critical-gap-summary)

---

## 1. Red-Flag Questions

### Q1: Where is the canonical decision hash derived?

**Answer**: `icn-governance/src/proof.rs:330-345`

```rust
pub fn compute_decision_hash(
    proposal_id: &str,
    domain_id: &str,
    outcome: ProofOutcome,
    vote_tally: &VoteTally,
    vote_hash: &Hash,
) -> Hash
```

**What it hashes** (domain tag `b"icn:gov:decision:v1"`):
- proposal_id (length-prefixed)
- domain_id (length-prefixed)
- outcome ordinal (0=Accepted, 1=Rejected, 2=NoQuorum)
- vote_tally: for_votes, against_votes, abstain_votes (each u64 LE)
- vote_hash: 32-byte Merkle root of sorted votes

**Vote hash** (`proof.rs:116-139`): Sorts votes by `(voter_did, choice_ordinal)` deterministically, creates Merkle root from `[voter_did, choice_ordinal, weight]` tuples. Domain tag: `b"icn:vote-hash:v1"`.

**Property**: Canonical — same decision produces same hash across all nodes. This is the idempotency key for the entire execution pipeline.

### Q2: Where is the treasury nonce checked on the actual mutation path?

**Answer**: **IT IS NOT.** The nonce mechanism exists but is not wired into the execution path.

- **Nonce mechanism defined**: `icn-coop/src/store.rs:112-206` — `check_and_increment_treasury_nonce()` does atomic sled CAS (compare-and-swap)
- **Not called from**: `KernelGovernanceExecutor::execute_treasury_operation()` in `icn-core/src/supervisor/governance_executor.rs:318-571`
- **Result**: A single governance decision could replay at the ledger level if the executor store write crashes after the ledger write but before the execution record is marked Confirmed

### Q3: What ledger-level idempotency mechanism exists?

**Answer**: **None today.** Idempotency is enforced only at:
- Level 1: `ExecutionStore` (decision_hash → terminal check) — `icn-core/src/supervisor/decision_executor.rs:225`
- Level 2: Domain stores (BudgetRecord tracks decision_hash per spend; EscrowRecord tracks release_decision_hash)
- Level 3: **[MISSING]** — No ledger-level deduplication. No `(decision_hash, entry_hash)` mapping in the ledger.

### Q4: What is the minimal gateway API contract for a pilot UI + wallet?

**Answer**: The gateway has 40+ route modules but no unified service layer boundary.

- `icn-api/src/lib.rs` exists (45 lines) with partial `ComputeService` — no LedgerService, GovernanceService, EntityService
- Gateway routes in `icn-gateway/src/api/*.rs` call managers directly (not through icn-api)
- RPC handlers in `icn-rpc/src/handler/compute.rs:50-95` DO use `icn_api::SubmitTaskParams`
- **Result**: Two code paths to the same operation (REST vs RPC), likely with divergent behavior

---

## 2. Governance → Effects → Ledger Saga

This is the **critical vertical path** for the pilot. End-to-end:

### Step 1: Proposal Closes → Decision Receipt

**Source**: `icn-governance/src/proof.rs:250-271`

When a proposal closes with Accepted/Rejected/NoQuorum, a `GovernanceDecisionReceipt` is created containing the canonical `decision_hash`.

**Proposal state machine**: `icn-governance/src/proposal.rs:1-217`
- States: Draft → Deliberation → Open → {Accepted | Rejected | NoQuorum | Cancelled | Vetoed | ForceClosed}
- Transitions enforced by methods: `start_deliberation()`, `open()`, `close()`, etc.

### Step 2: Event Emission

**Source**: `apps/governance/src/actor.rs` (event bus integration)

Emits `SystemEvent::ProposalAccepted { proposal_id, domain_id, payload, decided_at }` via `EventBus::emit()`.

### Step 3: Effect Translation (Payload → KernelEffect)

**Status**: **[GAP]** — No explicit code translates `ProposalPayload` → `KernelEffect[]`.

- Tests manually construct effects
- Payload variants defined in `icn-governance/src/proposal.rs:220+`: Budget, FreezeMember, ResourceAccess, ConfigChange, ProtocolChange, TextResolution
- Effect variants defined in `icn-kernel-api/src/effects.rs`: TreasuryEffect::Spend, CreateBudget, ReleaseEscrow, etc.
- **Risk**: Payload variants and effect variants could drift silently

### Step 4: Decision Executor

**Source**: `icn-core/src/supervisor/decision_executor.rs:217-379`

```
execute(effects, decision_receipt_id, decision_hash, proposal_id) → Vec<EffectResult>
```

**Idempotency check** (line 225): If `store.get(decision_hash)?.is_terminal()` → skip.

**Status machine**: `icn-kernel-api/src/execution.rs:14-166`
```
Pending → Executing → Confirmed
                   ↘ Failed → (retry) → Executing
                            ↘ PermanentlyFailed (MAX_RETRIES=3)
```

**Persistence**: Sled-backed `ExecutionStore`. Effects stored in record for crash recovery.

**Backpressure**: `tokio::sync::Semaphore` with `MAX_CONCURRENT_EXECUTIONS=16`.

**Observability** (PR #1203):
- Recovery scan log with status distribution counts
- Per-decision `status_before → status_after` transition logs
- Semaphore saturation warning (>100ms wait)

### Step 5: Effect Dispatch

**Source**: `icn-core/src/supervisor/governance_executor.rs:318-571`

`KernelGovernanceExecutor` implements `EffectExecutor` trait. Routes to specialized executors:
- TreasuryExecutor (Spend, Allocate, Release)
- ProtocolExecutor (SetParameter, Config)
- ControlExecutor (Veto, ForceClose)
- MembershipExecutor (Add, Remove, Freeze)
- FederationExecutor (Join, Vouch)

### Step 6: Treasury Sagas

**Budget Spend** (governance_executor.rs, Path 2):
1. `budget.begin_spend(decision_hash, amount)` → validates capacity, stores pending
2. Ledger mutation via `submit_treasury_entry()`
3. `budget.confirm_spend()` → transitions Pending → Spent

**Escrow Release** (governance_executor.rs, Path 3):
1. `escrow.begin_release(decision_hash)` → transitions Locked → Releasing, stores `release_decision_hash`
2. Ledger mutation via `submit_treasury_entry()`
3. `escrow.confirm_release()` → transitions Releasing → Released

### Step 7: Ledger Mutation

**Source**: `icn-ledger/src/ledger.rs` (5,447 LOC)

- Double-entry accounting: each journal entry has debit[] and credit[] sides
- `append_entry()` returns `entry_hash`
- Merkle-DAG for immutable history

**Gap**: `submit_treasury_entry()` exists and returns entry_hash, but:
- Entry hash is NOT routed back to `ExecutionRecord.ledger_entry_ids` (audit trail incomplete)
- No `state_change_hash` computed for treasury operations (protocol/federation effects DO compute them)
- Ledger service integration is stubbed in tests — `if let Some(ledger) = &self.ledger` falls back to placeholder

### What Tests Prove

| Test File | What It Proves |
|-----------|---------------|
| `icn-core/tests/decision_executor_runtime_test.rs::test_callback_auto_executes_decision` | Finalized decision auto-executes via callback |
| `...::test_callback_idempotent_no_double_execute` | Replay doesn't double-execute |
| `...::test_startup_recovery_completes_executing_decision` | Crash recovery picks up Executing state |
| `...::test_backpressure_no_deadlock` | Semaphore backpressure under load |
| `...::test_budget_spend_with_enforcement` | Budget saga begin→execute→confirm |
| `icn-core/tests/governance_ledger_integration.rs::test_budget_proposal_executes_ledger_transaction` | Governance approves → ledger transaction created |
| `icn-core/tests/escrow_release_integration.rs::test_replay_idempotent_no_double_spend` | Same decision twice → no double release |
| `...::test_crash_recovery_completes_releasing_escrow` | Escrow stays in Releasing across restart |
| `...::test_conflicting_decisions_rejected` | Two decisions can't both release same escrow |
| `icn-core/tests/budget_enforcement_integration.rs::test_budget_created_and_enforced` | CreateBudget → Spend validates capacity |
| `...::test_budget_overspend_rejected` | Over-limit spend fails before ledger mutation |
| `...::test_budget_crash_recovery` | Budget pending_spend survives restart |
| `icn-core/src/supervisor/decision_executor.rs` (unit tests) | 6 unit tests: extract_decision_hash, idempotency_skips, execute_records_status, treasury_effect_with_decision_hash |

---

## 3. Identity Plane

### What Exists

| Component | Location | Status |
|-----------|----------|--------|
| Did newtype | `icn-identity/src/lib.rs` | `did:icn:<multibase-base58-ed25519-pubkey>` |
| KeyPair | `icn-identity/src/lib.rs` | Ed25519, optional PQ hybrid (`#[cfg(feature = "post-quantum")]`) |
| Anchor (SDIS root) | `icn-identity/src/anchor.rs` | `H(VUI \|\| genesis_random)`, immutable across rotations |
| KeyBundle (rotatable) | `icn-identity/src/keybundle.rs` | Bound to Anchor, version monotonic, rotation protocol |
| PersonhoodAnchor | `icn-identity/src/personhood.rs` | Commons Layer 0, status: Active/Suspended/Revoked |
| DidDocument v2 | `icn-identity/src/multi_device.rs` | Multi-device, verification methods per device |
| Social Recovery | `icn-identity/src/recovery.rs` | M-of-N trustees, delay period, cancellation |
| Revocation Registry | `icn-identity/src/revocation.rs` | Appeal deadline, scope: Global/Federation/Jurisdiction |
| AgeKeyStore | `icn-identity/src/keystore.rs` | Age-encrypted at `~/.icn/keystore.age` |
| DidSigner trait | `icn-identity/src/did_signer.rs` | Software + Hardware backend abstraction |
| Batch verify | `icn-identity/src/batch_verify.rs` | Batch Ed25519 signature verification |
| CLI commands | `icn/bins/icnctl/src/main.rs:422-451` | init, show, rotate, upgrade-pq, export, import |

### What Tests Prove

| Test | Location |
|------|----------|
| KeyPair gen + sign/verify roundtrip | `icn-identity/src/lib.rs::test_generate_keypair`, `test_sign_verify` |
| DID parsing (valid/invalid/edge cases) | `lib.rs::test_did_from_str_*` (8 tests) |
| DID deserialization validates | `lib.rs::test_did_deserialization_*` (4 tests) |
| Multi-device workflow | `icn-identity/tests/multi_device_integration.rs` |
| Backend abstraction | `icn-identity/tests/backend_abstraction_test.rs` |
| Identity sync via gossip | `icn-identity/tests/identity_sync_test.rs` |

### Missing/Risky

- HSM/TPM backends feature-gated and experimental (`keystore_pkcs11.rs`, `keystore_tpm.rs`)
- Recovery → KeyBundle rotation integration unclear (RecoveryReason::Recovery exists but wiring not visible)
- Revocation appeal deadline enforcement: no periodic task auto-finalizes
- DID resolution over network: DidDocument exists but no resolution endpoint in identity crate **[SPECULATION: likely in gateway]**
- PQ upgrade path (`icnctl id upgrade-pq`) not exercised in tests

---

## 4. Network Plane

### What Exists

| Component | Location | Status |
|-----------|----------|--------|
| NetworkActor | `icn-net/src/actor/mod.rs` (1,999 LOC) | QUIC event loop, session/relay management |
| NetworkHandle | `icn-net/src/actor/mod.rs:165` | 20+ public async methods |
| Wire protocol | `icn-net/src/protocol.rs` | NetworkMessage, 10+ MessagePayload variants |
| Encoding | `icn-net/src/protocol.rs:24-25` | Postcard (current) + Bincode (legacy, rejected). 1-byte header: `(encoding<<4)\|compression` |
| SignedEnvelope | `icn-net/src/envelope.rs:60` | Ed25519 signature, optional PQ hybrid |
| ReplayGuard | `icn-net/src/replay_guard.rs` | Per-sender sequence tracking, 300s clock skew, persistent mode |
| Discovery | `icn-net/src/discovery.rs` | mDNS `_icn._udp.local`, 30s scan interval |
| TLS binding | `icn-net/src/tls.rs` | DID in X.509 SAN, verified against Hello message |
| NAT traversal | `icn-net/src/nat.rs`, `stun.rs`, `turn.rs` | STUN discovery, TURN relay fallback (PR #1183) |
| Rate limiting | `icn-net/src/rate_limit.rs` | Trust-gated: 10/20/100/200 msg/sec by trust class |
| Topology | `icn-net/src/topology.rs` | NeighborSets, RTT refresh 60s |
| Blob registry | `icn-net/src/blob_registry.rs` | Hash → peers, 24h TTL |
| E2E encryption | `icn-net/src/encryption.rs` | X25519 session keys |

### What Tests Prove

| Test File | What It Tests |
|-----------|--------------|
| `icn-net/tests/nat_traversal_integration.rs` | Candidate cache flow, stale rejection, priority, multi-peer |
| `icn-net/tests/relay_fallback.rs` | TURN relay when direct dial times out |
| `icn-net/tests/encrypted_message_integration.rs` | E2E encryption roundtrip |
| `icn-net/tests/did_tls_binding_integration.rs` | DID-TLS SAN verification |
| `icn-net/tests/encoding_negotiation_integration.rs` | Postcard vs Bincode negotiation |
| `icn-net/tests/signed_envelope_roundtrip.rs` | SignedEnvelope serialization |
| `icn-net/tests/trust_gated_rate_limiting_integration.rs` | PolicyOracle rate limits |
| `icn-net/src/actor/mod.rs` (unit tests) | 13 tests: actor start, peer capabilities, blob registry, NAT status |

### Missing/Risky

- No Hello handshake timeout — peer can delay indefinitely before identity proof
- Broadcast dedup only at gossip layer (no network-level nonce guard)
- Blob registry no refresh protocol — announced blob from failed peer still returned to callers
- E2E encryption fail-closed (`init_send_callback.rs:197-206`) — messages dropped if peer can't decrypt
- Misbehavior detector created but not wired (`actor/mod.rs:1122-1129`)

---

## 5. Gossip Plane

### What Exists

| Component | Location | Status |
|-----------|----------|--------|
| GossipActor | `icn-gossip/src/gossip.rs` | State machine: topics, entries, subscriptions, sync state |
| GossipMessage | `icn-gossip/src/types.rs` | 20 variants (push, pull, bloom, blob, replica, partition, storage) |
| VectorClock | `icn-gossip/src/vector_clock.rs` | Lamport clocks, LRU eviction at 10K entries |
| BloomFilter | `icn-gossip/src/bloom.rs` | Configurable k/m, anti-entropy |
| Dispatch | `icn-gossip/src/handlers/dispatch.rs` | Single routing table: GossipMessage → handler |
| Blob transfer | `icn-gossip/src/handlers/blob_transfer.rs` | Session manager, chunk reassembly, hash verification |
| BlobNonceGuard | `icn-gossip/src/handlers/blob_nonce_guard.rs` | Per-request nonce tracking, 5min expiry |
| Pull protocol | `icn-gossip/src/handlers/pull.rs` | Digest→PullRequest→PullResponse with SyncCursor pagination |
| Partition healing | `icn-gossip/src/handlers/partition.rs` | Phase 18 Week 3 |
| Storage challenges | `icn-gossip/src/handlers/storage_challenge.rs` | Proof-of-Storage |
| Topic ACL | `icn-gossip/src/types.rs:540` | Public, MinTrustScore(f64), Participants(Vec<Did>) |
| Key rotation cache | `icn-gossip/src/key_rotation.rs` | Issue #469 grace period |

### What Tests Prove

| Test File | What It Tests |
|-----------|--------------|
| `icn-gossip/tests/gossip_integration.rs` | Vector clocks (5 tests), Bloom filters (4 tests), Topics (4 tests), Entry compression (4 tests), Message serialization (4 tests) |
| `icn-gossip/tests/service_discovery_auth_boundary.rs` | Topic ACL enforcement, MinTrustScore, Participant allow-list |
| `icn-gossip/tests/service_discovery_integration.rs` | Topic subscription flow |
| `icn-core/tests/gossip_pull_protocol_integration.rs` | Pull protocol convergence between TestNodes |
| `icn-core/tests/multi_node_gossip_convergence.rs` | Multi-node entry propagation |

### Missing/Risky

- Storage quota manager created but never called (`gossip.rs:131`)
- Blob transfer no expiry reaper (expires_at set but not enforced)
- Partition healing no retry logic (PartitionHeal messages may be lost)
- Pull request no priority (critical topics starved under load)
- Bloom filter auto-resize not triggered (`scalability.rs` exists but not wired)
- Key rotation grace period incomplete (cache created but not used in publish handler)

---

## 6. Ledger/Economics Plane

### What Exists

| Component | Location | Status |
|-----------|----------|--------|
| Ledger (double-entry) | `icn-ledger/src/ledger.rs` (5,447 LOC) | JournalEntry with debit[]/credit[], Merkle-DAG |
| Treasury ops | `icn-ledger/src/treasury.rs` | Treasury spend, allocate |
| Budget allocation | `icn-ledger/src/treasury/budgets.rs` | Budget records |
| Audit trail | `icn-ledger/src/treasury/audit.rs` | Audit log |
| BudgetStore trait | `icn-kernel-api/src/budget.rs` | BudgetRecord, begin_spend/confirm_spend saga |
| EscrowStore trait | `icn-kernel-api/src/escrow.rs` | EscrowRecord, begin_release/confirm_release saga |
| TreasuryEffect | `icn-kernel-api/src/effects.rs` | Spend, CreateBudget, ReleaseEscrow variants |
| ExecutionStore | `icn-kernel-api/src/execution.rs` | ExecutionRecord, status machine, persistence trait |
| Treasury nonce | `icn-coop/src/store.rs:112-206` | Atomic sled CAS — `check_and_increment_treasury_nonce()` |
| Cooperative types | `icn-coop/src/types.rs` | Cooperative, Member, Treasury structures |
| Recurring payments | `icn-gateway/src/api/recurring_payments.rs` | Gateway-level only |

### What Tests Prove

| Test | What It Proves |
|------|---------------|
| `icn-core/tests/governance_ledger_integration.rs` | Budget proposal → ledger transaction → audit trail |
| `icn-core/tests/escrow_release_integration.rs` | Escrow saga idempotency + crash recovery (3 tests) |
| `icn-core/tests/budget_enforcement_integration.rs` | Budget creation + enforcement + crash recovery (3 tests) |
| `icn-core/tests/treasury_integration.rs` | Treasury operations (14 filtered) |
| `icn-core/tests/treasury_governance_integration.rs` | Treasury-governance binding (9 filtered) |
| `icn-coop/src/store.rs` (unit tests) | Treasury nonce mechanism works in isolation |

### Missing/Risky

- **Treasury nonce NOT in execution path** — defined but not called from treasury executor
- **Ledger service integration stubbed** — tests use mock/placeholder
- **No ledger-level dedup** — no `(decision_hash → entry_hash)` mapping
- **Recurring payment scheduler not wired** — `execute_due_payments()` exists, never called from daemon loop
- **Budget enforcement in Gateway only** — BudgetStore in-memory, not in ledger consensus
- **No GL account types** — DID→currency→balance only (no financial statement classification)
- **Cross-coop settlement unproven** — netting engine designed, never exercised
- **Dispute workflow missing** — types exist, no submission API

---

## 7. Compute Plane

### What Exists

| Component | Location | Status |
|-----------|----------|--------|
| ComputeActor | `icn-compute/src/actor/mod.rs` | Trust-gated task execution, 5+ gossip topics |
| ComputeTask | `icn-compute/src/types.rs` | TaskCode: CCL, CclRef, Wasm. DeterminismClass: Canonical, Advisory |
| ComputeHandle | `icn-compute/src/actor/mod.rs` | submit, status, cancel, handle_gossip, policy, disputes |
| WasmRegistry | `icn-compute/src/wasm_registry.rs` | Deploy/retrieve WASM blobs by hash |
| LocalExecutor | `icn-compute/src/executor.rs` | CCL execution, WASM feature-gated |
| Placement | `icn-compute/src/types.rs` | PlacementRequest/Offer, 500ms deliberation |
| Verification | `icn-compute/src/result_quorum.rs` | Multi-executor quorum (Issue #511) |
| Trust gates | `icn-compute/src/actor/mod.rs` | MIN_TRUST_SUBMIT=0.1, MIN_TRUST_EXECUTE=0.3 |
| Coop scheduling | `icn-compute/src/policy.rs` | Per-coop quotas, time windows, whitelist/blacklist |

### What Tests Prove

| Test File | What It Tests |
|-----------|--------------|
| `icn-compute/tests/compute_integration.rs` | LocalExecutor CCL execution, capability checks, Ed25519 result signatures |
| `icn-compute/tests/commons_integration.rs` | Commons pool operations |
| `icn-compute/tests/federation_integration.rs` | Cross-coop federation |
| `icn-compute/src/types.rs` (30+ unit tests) | Task hash determinism, fuel defaults, message serialization, result signing/verification, 30+ validation tests |
| `icn-gateway/tests/compute_events_integration.rs` | Compute events → WebSocket delivery |

### Missing/Risky

- CCL interpreter bridge to `icn_ccl::Interpreter` not visible in read code paths **[SPECULATION]**
- WASM executor feature-gated (`#[cfg(feature = "wasm")]`) — default builds may not support WASM
- Multi-executor consensus algorithm incomplete (how to break ties if 2/3 disagree)
- Actor migration rollback strategy unclear (Phase 16D migration_manager)
- Federated executor attestation verification not read

---

## 8. Gateway/UI Surface

### What Exists

| Component | Location | Status |
|-----------|----------|--------|
| GatewayServer | `icn-gateway/src/server.rs` | Actix-web, Prometheus, CORS, JWT, rate limiting |
| API routes | `icn-gateway/src/api/*.rs` | 38+ sub-modules, 40+ endpoints |
| GatewayError | `icn-gateway/src/error.rs` | 20+ variants, HTTP status mapping, i18n |
| JWT auth | `icn-gateway/src/auth.rs` + `middleware.rs` | TokenClaims with scopes, per-scope validation |
| OpenAPI | `icn-gateway/src/openapi.rs` | utoipa v0.1.0, 80+ schemas |
| EventBroadcaster | `icn-gateway/src/events.rs` | Pub-sub channels → WebSocket |
| WsSession | `icn-gateway/src/websocket.rs` | Real-time event delivery |
| Notification system | `icn-gateway/src/notification_*.rs` | Queue, processor, store, triggers |
| 18 Managers | `icn-gateway/src/*.rs` | Compute, Ledger, Trust, Entity, Governance, Commons, Coop, Community, Federation, Treasury, Identity, Session, Steward, Listings, ServiceDiscovery, EntityAudit |
| Rate limiting | `icn-gateway/src/rate_limit.rs` | Trust-gated per-DID limits |

### What Tests Prove

20 integration test files in `icn-gateway/tests/`:
- `compute_events_integration.rs` — events flow compute→WebSocket
- `websocket_integration.rs` — WebSocket connection
- `governance_flows_integration.rs` — governance proposal workflow
- `budget_integration.rs` — member budget enforcement
- `escrow_integration.rs` — escrow hold/release
- `recurring_payments_integration.rs` — subscription payments
- `entity_integration.rs` — entity lifecycle
- `treasury_custody_test.rs` — treasury multi-sig
- `pilot_features_integration.rs` — pilot-specific features
- Plus 10+ more (SDIS, service discovery, commons, steward, dissolution, constitutional, metrics, i18n, services, queries)

### Missing/Risky

- **No unified service layer** — `icn-api` exists with partial ComputeService only. Gateway routes bypass it entirely.
- **Two code paths** — RPC uses `icn_api::SubmitTaskParams`; Gateway uses managers directly → divergent behavior
- **Error inconsistency** — Gateway uses GatewayError + i18n; RPC uses RpcErrorCode (JSON-RPC). Not bridged.
- **WebSocket event filtering** — EventBroadcaster broadcasts to channels but no visible coop_id scope filtering
- **Coop isolation audit needed** — managers likely scoped to coop_id but enforcement not visible in route handlers
- **OpenAPI schema vs reality** — 80+ schemas registered but endpoint-path mapping not verified complete

---

## 9. Privacy/Crypto Plane

### What Exists

| Component | Location | Status |
|-----------|----------|--------|
| Topic encryption | `icn-privacy/src/topic_encryption.rs` | ChaCha20-Poly1305 AEAD, Bloom hints |
| Onion routing | `icn-privacy/src/onion_routing.rs` | Multi-hop via X25519. Types exist. |
| Traffic obfuscation | `icn-privacy/src/traffic_obfuscation.rs` | Random delays, size padding, cover traffic (disabled default) |
| Hybrid Ed25519+ML-DSA | `icn-crypto-pq/src/hybrid.rs` | HybridKeypair, HybridSignature (~3.4KB) |
| ML-DSA-65 | `icn-crypto-pq/src/ml_dsa.rs` | NIST FIPS 204, Level 3 |
| ML-KEM-768 | `icn-crypto-pq/src/ml_kem.rs` | NIST FIPS 203, key encapsulation |
| Shamir sharing | `icn-crypto-pq/src/shamir.rs` | Secret distribution |
| Blind signatures | `icn-crypto-pq/src/blind.rs` | Privacy-preserving enrollment |
| ZKP (STARK) | `icn-zkp/src/types.rs`, `prover.rs` | StarkProof, ProofType: Age, Citizenship, Membership, NonRevocation |

### What Tests Prove

| Test | What It Proves |
|------|---------------|
| `icn-privacy/tests/privacy_integration.rs::test_multi_party_topic_encryption` | Shared key encrypt/decrypt |
| `...::test_bloom_filter_topic_discovery_multiple_subscribers` | Bloom filters match correctly |
| `...::test_topic_encryption_prevents_linkability` | Same topic → different nonces |
| `icn-crypto-pq/src/lib.rs::test_hybrid_roundtrip` | Generate, sign, verify both sigs |
| `...::test_hybrid_tampered_message` | Tampered → fail |
| `...::test_hybrid_wrong_key` | Wrong key → fail |

### Missing/Risky

- Onion routing not end-to-end tested (types exist, no circuit creation test)
- ZKP proofs simulated only (`simulated: true` → no cryptographic security)
- ZKP circuits marked `#[allow(dead_code)]` — STARK proving/verification likely stubbed
- PQ key distribution not implemented — hybrid verification deferred in SignedEnvelope
- Cover traffic disabled by default (bandwidth intensive)

---

## 10. Idempotency Layer Map

```
Layer 1: Decision Hash (canonical, deterministic)
├── Source: icn-governance/src/proof.rs:330-345
├── Key: blake3(domain_tag || proposal_id || domain_id || outcome || vote_tally || vote_hash)
└── Property: Same decision → same hash across all nodes

Layer 2: ExecutionStore (sled, persistent)
├── Source: icn-core/src/supervisor/decision_executor.rs:225
├── Check: If decision_hash already terminal → skip
├── Record: Pending → Executing → Confirmed | Failed | PermanentlyFailed
└── Test: test_callback_idempotent_no_double_execute

Layer 3: Domain Stores (per-effect-type)
├── BudgetRecord: stores decision_hash per spend (begin_spend/confirm_spend)
│   └── Source: icn-kernel-api/src/budget.rs
├── EscrowRecord: stores release_decision_hash (one decision per escrow)
│   └── Source: icn-kernel-api/src/escrow.rs
│   └── Test: test_conflicting_decisions_rejected
└── Property: Domain-specific dedup before ledger mutation

Layer 4: Treasury Nonce (DEFINED BUT NOT WIRED)
├── Source: icn-coop/src/store.rs:112-206
├── Mechanism: Atomic sled CAS — check_and_increment_treasury_nonce()
├── NOT called from: governance_executor.rs::execute_treasury_operation()
└── Issue: #1204 (created for this gap)

Layer 5: Ledger Deduplication (DOES NOT EXIST)
├── No (decision_hash → entry_hash) mapping in ledger
├── No append_entry() idempotency check
└── Risk: Crash between ledger write and executor store write → double-apply
```

---

## 11. Critical Gap Summary

### Tier 1: Blocking for Pilot

| # | Gap | Location | Risk | Issue |
|---|-----|----------|------|-------|
| 1 | Treasury nonce not in execution path | `icn-coop/src/store.rs` defined, `governance_executor.rs` doesn't call it | Double-spend on replay at ledger level | #1204 |
| 2 | Ledger service integration stubbed | `governance_executor.rs:622-659` falls back to placeholder | No actual persistent ledger mutations in pilot | — |
| 3 | Payload→Effect translation missing | No explicit code maps ProposalPayload→KernelEffect[] | Effect construction is manual in tests only | — |
| 4 | No unified service layer | `icn-api` has partial ComputeService only | REST and RPC diverge, SDK consumers confused | — |

### Tier 2: Hardening for Pilot v1+1

| # | Gap | Location | Risk | Issue |
|---|-----|----------|------|-------|
| 5 | Ledger entry IDs not returned to execution record | `ExecutionRecord.ledger_entry_ids` always empty | Audit trail incomplete | — |
| 6 | No state_change_hash for treasury effects | Protocol effects compute it, treasury doesn't | Verification gap | — |
| 7 | Recurring payment scheduler not wired | `execute_due_payments()` never called from daemon | No payroll | — |
| 8 | Budget enforcement in Gateway only | BudgetStore in-memory | Bypassable | — |
| 9 | Error code inconsistency (Gateway vs RPC) | `GatewayError` vs `RpcErrorCode` | SDK sees different errors for same operation | — |

### Tier 3: Post-Pilot Robustness

| # | Gap | Location | Risk |
|---|-----|----------|------|
| 10 | Storage quota manager never called | `gossip.rs:131` | Per-DID storage limits not enforced |
| 11 | Blob transfer no expiry reaper | `handlers/blob_transfer.rs` | Stale transfers consume resources |
| 12 | Partition healing no retry | `handlers/partition.rs` | Partitions may not self-heal |
| 13 | ZKP proofs simulated only | `icn-zkp/src/types.rs` | No cryptographic security |
| 14 | PQ key distribution not implemented | `icn-net/src/envelope.rs` hybrid deferred | Only classical verification works |
| 15 | Onion routing incomplete | `icn-privacy/src/onion_routing.rs` | No end-to-end test |
| 16 | Misbehavior detector not wired | `icn-net/src/actor/mod.rs:1122-1129` | Can't distinguish malicious from slow peers |
