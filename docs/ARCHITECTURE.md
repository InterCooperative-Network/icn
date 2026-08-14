---
Status: normative
Canonical: yes
Last Reviewed: 2026-06-14
---

# ICN Architecture Reference

<!-- truth: normative -->

## 1. What ICN Is

InterCooperative Network (ICN) is a P2P constraint enforcement engine—infrastructure for distributed coordination without central servers. It translates policy into mechanical enforcement: apps describe what they want through constraints, the kernel executes those constraints blindly, neither understanding nor caring about the domain semantics. Not a blockchain. Not a federation server. Not a payment system. Think TCP/IP for cooperatives: a dumb, reliable substrate that institutions build on top of.

The thesis: **eight mechanical primitives compose into institution-grade capabilities**. Identity proves who you are. Authorization gates what you can do. State lets you persist decisions. Compute executes work. Communication moves data. Time orders events. Coordination synchronizes views. Naming resolves addresses. Cooperatives, communities, and federations hang their governance, economics, and membership on these primitives—not the other way around.

| What ICN Does | What ICN Doesn't Do |
|---|---|
| Enforces constraints (rate limits, authorization, state transitions) | Interpret policy (that's apps' job) |
| Routes messages peer-to-peer | Host services or coordinate operators |
| Verifies digital signatures | Manage keys (clients do that) |
| Persist ledger entries deterministically | Make economic decisions (governance does) |
| Execute WASM modules with bounded fuel | Run unconstrained computation |
| Gossip state causally consistent | Achieve consensus or total ordering |
| Store DIDs, keys, entities | Act as identity provider |
| Sync trust graphs across peers | Judge trustworthiness (that's subjective) |

<!-- truth: normative -->

## 2. Boundary Conditions / Anti-Claims

ICN refuses certain interpretations and integrations. This matters legally, architecturally, and culturally.

**Not a financial product.** ICN contains no payment processing, no settlement of external currencies, no custody, no transmission of funds on behalf of users. Cooperatives using ICN settle their own mutual credit in their own ledgers. No operator routes value. No operator holds positions. This is critical for FinCEN software provider exemption.

**Not a consensus system.** ICN uses causal consistency (vector clocks) and gossip propagation, not Byzantine consensus. No Raft, no PBFT, no proof-of-work. Why? Consensus couples availability to participation: if 33% of nodes go offline, consensus halts. Cooperatives need liveness even with network partitions. Consensus centralizes power (honest majority assumption). Causal consistency lets each peer maintain its own view and lets communities heal partitions when they reconnect.

**Not a blockchain.** No chain-of-blocks. No merkle trees as primary ledger structure (though Merkle-DAG is used for tamper evidence). No immutability religion. Ledger entries are append-only within a node but can be disputed, rolled back, or amended by governance decision.

**Not a federation server.** ICN has no central database, no master registry, no server that must stay online for the network to function. Federation is peer-to-peer: each cooperative gossips its own state, and cross-cooperative transactions settle through mutual credit agreements. The federation crate coordinates discovery and trust, not by centralizing information but by decentralizing it.

**Not a marketplace or matching engine.** ICN will never have built-in bidding, price discovery, or automatic matching. Communities can build marketplaces as apps on top (CCL contracts that match buyers and sellers), but the kernel treats all state equally. This prevents the platform from becoming an extractive intermediary.

**Not a "web3" solution.** No tokens, no speculative instruments, no tokenomic incentives. The system runs on voluntary participation and trust, not financial motivation. If you're looking to tokenize something and raise capital, this is not your substrate.

**Terminology discipline.** To avoid regulatory mischaracterization:
- Use "settlement" not "payment" (settlement = moving units, not converting external currency)
- Use "unit" not "currency" (units are internal to cooperatives, not fungible across them without governance)
- Use "position" not "balance" (positions can be negative, disputed, or subject to governance)
- Use "identity" not "wallet" (wallets store keys; ICN stores DIDs which are public credentials)
- Use "digital public infrastructure" / "coordination OS" when describing ICN to regulators or journalists

<!-- truth: normative -->

## 3. The Meaning Firewall

The hard architectural boundary between kernel and apps. The kernel must never understand domain semantics. Apps translate meaning into constraints. The kernel enforces constraints. This separation protects both layers: the kernel stays simple and auditable; apps stay pluggable and competitive.

**Five immutable invariants from KERNEL_APP_SEPARATION:**

1. **Kernel crates must not import domain crates.** No `icn-ledger` in `icn-core`. No `icn-governance` in `icn-net`. A domain crate is anything that interprets semantics (governance, economics, trust policy). A kernel crate is anything that enforces mechanics (storage, networking, state machines). This is enforced by **strict-mode ratchet tests** in `icn/crates/icn-core/src/meaning_firewall.rs` (run in CI, fail on regression), which pin the exact count of residual domain references per kernel crate; `kernel_surface.toml` is the human-readable per-crate inventory those ratchets track against. *(Verified against icn/crates/icn-core/src/meaning_firewall.rs and kernel_surface.toml, 2026-06-14.)*

2. **Kernel only accepts pure data types.** `Vec<u8>`, `u64`, `bool`, `[u8; 32]`. No `TrustScore`, no `GovernanceProfile`, no semantic enums. The kernel cannot parse or pattern-match on app-specific structures because that would require understanding the app's meaning.

3. **All semantic policy lookup confined to PolicyOracle.** Apps implement a single trait: `PolicyOracle { fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision }`. The kernel calls this once per request. The PolicyOracle returns a `ConstraintSet`—generic rate limits, credit multipliers, voting weights. Nothing domain-specific crosses the boundary.

4. **Kernel must not pattern-match on ConstraintSet.custom keys.** Apps can attach custom constraints via `ConstraintSet::with_custom("my_limit", 42_u64)`. The kernel must never switch on those keys or interpret their values. If the kernel needs to understand a custom constraint, that's a design smell: the constraint probably belongs in standard fields.

5. **Kernel contains enforcement mechanisms, not policy decisions.** The kernel decides "this transaction exceeds the rate limit so drop it." The kernel does NOT decide "this user is too untrustworthy to talk to." That decision lives in apps (the trust policy oracle). Once the app says "this user gets a 10 msg/sec limit," the kernel enforces the 10 msg/sec limit mechanically.

**The PolicyOracle pattern:**

```rust
// App side: translates domain meaning to constraints
pub struct TrustPolicyOracle {
    trust_graph: Arc<TrustGraph>,
    rate_limits: HashMap<TrustClass, RateLimit>,
}

impl PolicyOracle for TrustPolicyOracle {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision {
        let score = self.trust_graph.compute_score(&request.actor);
        let class = self.classify(score);
        let limit = self.rate_limits[&class];

        PolicyDecision::Allow {
            constraints: ConstraintSet::new()
                .with_rate_limit(limit)
                .with_credit_multiplier(score_to_multiplier(score))
        }
    }
}

// Kernel side: enforces constraints blindly
impl RateLimiter {
    fn check(&self, actor: &Did, constraints: &ConstraintSet) -> Result<()> {
        let limit = constraints.rate_limit();  // Just a number
        let current = self.counter.load(Ordering::SeqCst);
        if current >= limit { return Err(RateLimitExceeded) }
        Ok(())
    }
}
```

The app understands trust. The kernel understands rate limits. Neither crosses the boundary.

<!-- truth: normative -->

## 4. The Eight Primitives

Every ICN node provides these eight primitives. Apps compose them into institution-grade features.

### 4.1 Identity

**What it does:** Proves cryptographic identity, enables key rotation and recovery, supports threshold signing, resists Sybil attacks via Sovereign Digital Identity System (SDIS).

**Formats:**
- Traditional DID: `did:icn:<base58-pubkey>` (Ed25519 public key, deterministic)
- SDIS anchor: `did:icn:<anchor-id>` (privacy-preserving, backed by VUI)

**Cryptography:** Ed25519 signing, X25519 key exchange, ChaCha20-Poly1305 AEAD encryption, post-quantum hybrid (optional ML-KEM + ML-DSA, feature-gated).

**Storage:** Age-encrypted keystore at `~/.icn/keystore.age`. Multi-device support: one primary key + secondary device keys tied to the primary. Key rotation is explicit and gossipped.

**Sybil resistance via SDIS:** Steward network computes Verifiable Unique Identifiers (VUIs) using threshold PRF—no single steward knows the full VUI, only users with proof-of-personhood can enroll. Threshold guarantees honest steward majority can compute it; adversary majority cannot.

**Crate:** `icn-identity`, `icn-crypto`, `icn-crypto-pq`, `icn-zkp`, `icn-steward`

<!-- truth: descriptive -->

### 4.2 Authorization

**What it does:** Gates access to resources and actions using object capabilities and policy oracles. No central access control server needed.

**Model:** Capability graph with subjects, actions, resources, and constraints. Apps implement `PolicyOracle` to interpret domain meaning (trust scores, governance rules, membership status) and return `ConstraintSet` for the kernel to enforce.

**E2E example:**
- App receives request: "Alice wants to transfer 10 hours from Bob's account"
- App's AuthorizationOracle: "Bob trusts Alice at 0.6 score → weight=0.6 × request"
- App returns: Allow with constraint { credit_multiplier: 0.6, audit_required: true }
- Kernel: applies 0.6 multiplier to the debit, gates the transfer

**Built-in constraints:** rate limit, credit multiplier, voting weight, resource quota, custom (app-defined).

**Crate:** `icn-authz` (app-layer, not imported by kernel)

<!-- truth: descriptive -->

### 4.3 State

**What it does:** Persists data atomically, provides version history, supports conflict-free merging, enables tamper-evident ledger entries.

**Storage tiers:**
- **Blobs:** opaque byte sequences, content-addressed with blake3
- **Logs:** append-only journals (ledger entries, governance decisions, trust attestations)
- **KV:** key-value store with prefix iteration
- **Snapshots:** periodic consistent checkpoints for faster bootstrap

**Merkle-DAG for ledgers:** Each entry links to its parent and sibling entries. Root hash commits all history. No mutation, only append. Rollback is explicit and consensual (requires governance).

**Conflict resolution:** Deterministic merge for concurrent updates (happens-before ordering via vector clocks). If concurrent updates conflict, governance decides the resolution.

**Crate:** `icn-store` (Sled backend), `icn-snapshot`, `icn-ledger` (double-entry mutual credit), `icn-governance` (decision records)

<!-- truth: descriptive -->

### 4.4 Compute

**What it does:** Executes work—CCL contracts or WASM modules—in a trust-gated, sandboxed, fuel-metered environment. No work happens without explicit authorization.

**CCL (Cooperative Contract Language):** Domain-specific language, not Turing-complete. Fuel metering prevents infinite loops. Determinism guarantees: no system time access, deterministic PRNG, canonical ordering of inputs. Capability-based: contracts declare what they can read (ledger, trust) and write (ledger entries, governance proposals).

**WASM:** Optional feature (`--features wasm`). Sandboxed via wasmtime. **Experimental — not held to CCL's fuel/determinism guarantees:** `icn/crates/icn-compute/src/wasm_executor.rs` uses `Engine::default()` (no wasmtime fuel metering — "fuel metering can be enabled later"; a coarse per-call counter only) plus a wall-clock `SystemTime::now()` host import, so WASM execution is non-deterministic. Real wasmtime fuel metering and deterministic imports are planned (see §11 and `docs/freshness.toml` §11).

**Execution model:** Trust-gated (only peers above a trust threshold can submit tasks), gossip-based distribution (submitter broadcasts to network, executors claim work), proof-of-execution (workers sign completion proofs).

**Crate:** `icn-ccl`, `icn-compute`

<!-- truth: descriptive -->

### 4.5 Communication

**What it does:** Moves data peer-to-peer with publication-subscription topology. Preserves causal consistency. Resists spam through trust-based rate limiting.

**Gossip subsystem:** Topic-based channels. Each topic has an ACL. Nodes subscribe to topics and receive new messages via push (announced by sender) or pull (requested by receiver). Vector clocks track causal ordering. Bloom filters deduplicate. Anti-entropy ensures eventual consistency even with network failures.

**Message types:** Gossip (broadcast), RPC (request-response), Subscribe (topic subscription), Hello (peer discovery), Signed (cryptographically signed envelope).

**Transport:** QUIC/TLS 1.3 with DID-TLS binding (certificate subject is the peer's DID). Peer discovery via mDNS (local) and rendezvous (wide-area). NAT traversal via hole punching + TURN relay fallback.

**Encryption:** End-to-end for sensitive topics. Topic names can be encrypted (metadata protection).

**Crate:** `icn-gossip`, `icn-net`, `icn-gateway` (REST API), `icn-rpc` (JSON-RPC)

<!-- truth: descriptive -->

### 4.6 Time

**What it does:** Orders events causally. Enables scheduling. Prevents replay attacks.

**Mechanism:** Logical clocks (vector clocks, not wall-clock time). Each event carries a vector timestamp. No node needs global synchronization. Wall-clock is used only for human-facing timestamps and lease expiry.

**Determinism:** Protocols use logical time, not wall time. Same inputs always produce same output order. This enables replay-resistant signatures and deterministic contract execution.

**Replay protection:** `SignedEnvelope` includes a sequence number per sender. Network actors reject messages with sequence numbers ≤ their last-seen from that sender.

**Leases:** Temporary permissions (e.g., "vote on this proposal until block 42"). Consensus on block number via gossip of block headers.

**Crate:** `icn-time`, `icn-protocol` (envelope types), `icn-net` (sequence tracking)

<!-- truth: descriptive -->

### 4.7 Coordination

**What it does:** Synchronizes state across peers without consensus. Enables distributed governance decisions, multi-node ledger consistency, federated trust.

**Model:** Causal consistency through gossip. All nodes gossip their changes. Each node independently applies gossipped changes in causal order (vector clocks). No quorum needed. Partition-tolerant: if the network splits, both halves continue operating, and merge deterministically when reconnected.

**Why not consensus?** Consensus (Raft, PBFT) couples liveness to quorum: if 33% go offline, the network halts. Cooperatives need availability. Consensus also centralizes power: honest-majority assumption means you need pre-agreed validators. Causal consistency lets any peer participate, and lets the network heal from partitions.

**Governance coordination:** Proposals and votes gossip. Each node tallies votes independently. Once a threshold is reached, the governance oracle executes the decision. No central authority needed.

> **Current status — governance replication is suspended.** The design above describes the
> intended model, not present behavior. Governance messages received over gossip are
> refused before they are applied to governance state: `GossipEntry` carries a claimed
> author DID but no signature binding it to the entry contents, so a replicated governance
> message is indistinguishable from a forged one and applying it was an unauthenticated
> write primitive. Locally originated governance is unaffected; cross-node governance
> convergence does not currently occur. Authenticated replication is tracked in issue #2469.

**Crate:** `icn-gossip`, `icn-ledger` (deterministic merge), `icn-governance` (proposal dispatch), `icn-protocol` (message ordering)

<!-- truth: descriptive -->

### 4.8 Naming

**What it does:** Resolves human-readable names to DIDs, entities, and resources. Supports hierarchical paths, aliases, and authority-based registration.

**Namespace hierarchy:**
- `/cell/...` — individuals / households
- `/org/...` — organizations / cooperatives
- `/fed/...` — federations
- `/commons/...` — shared resources

**Registration:** Authority must sign name registrations (prevents squatting). Lookups are recursive with bounded depth (prevents infinite loops). Aliases allow indirection.

**Gossip:** Names are announced via gossip topics. Each node maintains a local name cache. Authority-signed names are trusted; unattested names are advisory.

**Crate:** `icn-naming`

<!-- truth: normative -->

## 5. Regulatory Architecture

ICN is designed to be compatible with existing financial regulation, cooperative law, and digital rights frameworks. This requires deliberate architectural choices and terminology discipline.

**Seven regulatory invariants:**

1. **User-signed transitions only.** No transaction happens without the user's cryptographic signature. Users control what happens to their own accounts. No "cold wallet" or "hot wallet" operator logic.

2. **No hosted positions.** ICN does not hold balances on behalf of users. Each cooperative's nodes hold a distributed ledger; members query their position from the ledger. No custodian, no operator account.

3. **No operator routing of value.** Units do not flow through an operator account. Transfers go directly from debtor to creditor ledgers. ICN operators facilitate synchronization; they do not move units.

4. **Derived views are not protocol primitives.** A "balance" is a computed view (sum of crediting entries minus debits). It's not a stored state that the protocol enforces. This means no balance invariants in the protocol itself—governance can enforce them if desired, but the protocol doesn't.

5. **No embedded convertibility.** Units of one cooperative are not automatically convertible to units of another. Cross-coop settlement requires explicit trust agreements (bilateral credit limits) and governance approval.

6. **Matching/market features are opt-in, scoped, governance-authorized.** If a cooperative wants a built-in marketplace, that's a CCL contract they deploy. It's not a platform feature. It's not operator-run. The governance charter must authorize it.

7. **Execution receipts close the loop.** Every state transition (ledger entry, governance decision, contract execution) produces a cryptographically signed receipt. Users can audit the full chain: Proposal → Decision → Allocation → Settlement Intent → Ledger Entry. No hidden state.

**Anti-features table:**

| Feature | Why Not | How to Build It |
|---|---|---|
| Central authority over accounts | Violates user-signed transitions | Apps implement multi-sig governance |
| Automatic token issuance / printing | No embedded monetary policy | Governance decides allocation; apps mint via proposal |
| Atomic cross-network swaps | Violates no-operator-routing | Build CCL escrow contracts; bilateral settlement |
| Platform-enforced price discovery | Becomes extractive intermediary | Apps build peer-to-peer matching (not kernel) |
| Automated market makers (AMM) | Requires pools held by operator | Build as CCL contracts, not kernel |
| Interest on balances / demurrage | Changes over time, not mechanical | Governance decision; ledger rebase at proposal execution |
| Fractional reserve / credit creation | Decouples from trust/governance | Double-entry invariant enforces issuance = allocation + surplus |

**Terminology discipline (critical for regulatory compliance):**

- ✓ "settle units of X" not "pay X dollars"
- ✓ "participant's position: +5 hours, -2 kWh" not "balance of 5 hours"
- ✓ "identity" not "wallet"
- ✓ "cooperative account" not "crypto account"
- ✓ "distributed ledger" not "blockchain"
- ✓ "coordination layer" not "payment network"
- ✓ "digital public infrastructure" when describing to regulators/press

<!-- truth: descriptive -->

## 6. Identity & Cryptography

ICN identity is cryptographic, decentralized, and resistant to Sybil attacks through the Sovereign Digital Identity System (SDIS).

**Traditional DIDs:** `did:icn:<base58-pubkey>` where pubkey is the Ed25519 public key. Deterministic: same key always produces same DID. No registration required. DIDs are valid at genesis.

**SDIS anchors:** `did:icn:<anchor-id>` where anchor-id is tied to a Verifiable Unique Identifier (VUI). VUI is computed by a threshold PRF: 5 stewards each hold a PepperShare, any 3 can compute the VUI, but no 2 can. VUI proves uniqueness (Sybil resistance) without revealing identity (privacy). Requires proof-of-personhood ceremony.

**Key management:**
- **Primary key:** Ed25519 keypair, stored encrypted in Age keystore
- **Secondary keys:** Device keys (e.g., phone, laptop) tied to primary via explicit binding
- **Key rotation:** Explicit, gossipped, history tracked on ledger
- **Recovery:** Social recovery (steward-attested recovery keys) or explicit backup
- **Keystore migration:** On-disk formats are v1 → v2 (TLS binding) → v2.1 (X25519 encryption keys) → v3 (DID Document + multi-device) → v4 (SDIS Anchor + KeyBundle with hybrid signatures). **Automatic migration terminates at v3** (`keystore.rs:1098`): a v3 keystore is unlocked and returned without being upgraded (`:1064`), and v4 is written only when SDIS fields are added (`save_v4`, `:481`). There is no v5 on-disk format — the `v5` comments at `keystore.rs:98,184` label feature-gated optional fields inside `StoredKeyV4`, whose `version` is always 4 (`:143`). *(Corrected 2026-08-14 at 74c832f1; the prior note described migration as reaching v4 automatically.)*

**Cryptographic suite:**
- **Signing:** Ed25519 (512-bit signatures, ~2 million ops/sec on modern hardware)
- **Key exchange:** X25519 (Diffie-Hellman)
- **AEAD:** ChaCha20-Poly1305 (for encrypted payloads)
- **Hashing:** BLAKE3 (streaming, parallelizable)
- **Post-quantum (optional, feature-gated):** ML-KEM (key encapsulation, formerly Kyber) + ML-DSA (signatures, formerly Dilithium)

**Post-quantum strategy:** Hybrid signatures blend Ed25519 + ML-DSA (~3309 byte PQ signatures). Both must verify; either alone is insufficient. Hybrid encryption blends X25519 + ML-KEM for key encapsulation. This ensures forward secrecy: if quantum computers break one scheme, the other still holds. Feature-gated behind `post-quantum` flag.

**Zero-knowledge credentials (for privacy):**
- **Attribute proofs:** Age > 18, citizenship, membership status without revealing exact data
- **Non-revocation proofs:** Prove credential was not revoked
- **Compound proofs:** Combine attribute + non-revocation
- **Accumulators:** RSA (fast, not post-quantum) or Merkle (slower, post-quantum safe)

**Steward network (SDIS backbone):**
- 5 independent stewards, geographically distributed, no mutual trust
- Each holds a PepperShare (1/5 of the secret)
- Threshold 3-of-5: any honest majority can compute VUI
- Stewards provide: VUI computation, enrollment ceremonies, blind credential issuance, key recovery, revocation checking
- No single steward knows who you are

**Status: EXCEEDS SPEC** (operational). Crates: `icn-identity`, `icn-crypto`, `icn-crypto-pq`, `icn-zkp`, `icn-steward`. *(§6 re-verified 2026-06-14: 3-of-5 steward threshold confirmed against icn/crates/icn-steward/src/gossip.rs and enrollment.rs (t-of-n PRF, default threshold 3); keystore migration chain confirmed against icn/crates/icn-identity/src/keystore.rs. The prior "88 modules" descriptor was unverifiable and removed.)*

<!-- truth: descriptive -->

## 7. Trust Graph

ICN trust is a directed, labeled, evidence-based graph. Each edge represents a relationship with explicitly recorded rationale. Trust is NOT transferable or global—it's peer-local and subjective.

**Three independent dimensions** (prevent cross-domain clique capture):

| Dimension | Purpose | Weights |
|---|---|---|
| **Social** | Direct endorsements, working relationships | 60% direct + 40% transitive |
| **Economic** | Settlement history, credit behavior | 80% direct + 20% transitive |
| **Technical** | Node uptime, contract success rate | Configurable |

**Why three dimensions?** A bad actor might be socially trusted but economically risky. Or technically reliable but untrustworthy with data. Separating them prevents one bad axis from dominating.

**Four trust classes** (configurable via `icn.toml`):

| Class | Score Range | Rate Limit | Semantics |
|---|---|---|---|
| Isolated | < 0.1 | 10 msg/s | Unknown, untrusted |
| Known | 0.1–0.4 | 50 msg/s | Some working history |
| Partner | 0.4–0.7 | 100 msg/s | Regular collaboration |
| Federated | 0.7+ | 200 msg/s | Deep trust, federation member |

**Local computation:** Each peer computes its own trust scores. No consensus needed. Node A's trust of B != Node C's trust of B. This is correct: trust is subjective.

**Transitive trust:** I trust Alice (0.7). Alice trusts Bob (0.8). Do I trust Bob? Score = 0.7 × 0.8 × (transitive weight) ≈ 0.45 (Partner). Transitive weight is lower because trust is not transitive by default—it requires deliberate endorsement.

**Bootstrap trust:**
- **Vouching:** I add you as Known (0.2 score) by explicit action
- **Invite codes:** Pre-shared codes let joiners bootstrap to Isolated (0.1)
- **Proof-of-work:** Solve a puzzle to prove you're not a bot (optional, expensive)

**Evidence chains:** Each trust edge has an optional justification (text or embedded proof). "Paid me on time 5 times" vs. "Haven't interacted yet, random voucher." Justifications don't affect the score (kernel doesn't read them), but they're gossiped and visible to the peer.

**Cache invalidation:** LRU cache with 5-minute TTL per peer. When A→B edge changes, B's score invalidates + all C where B→C invalidates (one-hop bounded). Prevents cascading recalculations.

**Attack resistance:** Sybil attacks (create many fake accounts) are slowed by Sybil resistance in identity layer (SDIS). Bad-mouthing attacks (I accuse Alice of being untrustworthy) don't work because it's my subjective score, not Alice's score globally.

**Status: CONFIRMED** (operational). Crate: `icn-trust`. *(§7 verified accurate 2026-06-14: trust-class thresholds — Isolated <0.1, Known 0.1–0.4, Partner 0.4–0.7, Federated 0.7+ — checked against `TrustClass::from_score` in icn/crates/icn-trust/src/lib.rs; per-class rate limits 10/50/100/200 msg/s checked against icn/crates/icn-net/src/rate_limit.rs.)*

<!-- truth: descriptive -->

## 8. Network & Transport

ICN uses QUIC (UDP-based, multiplexed) with TLS 1.3 for encryption and authentication. Peer discovery is via mDNS (local) and rendezvous (wide-area).

**QUIC advantages over TCP:** Multiplexing (multiple streams on one connection), 0-RTT resumption, connection migration (keep connection alive across IP changes), built-in encryption (TLS is mandatory, not optional).

**DID-TLS binding:** TLS certificate subject is the peer's DID. Each node proves it controls the DID by signing the certificate request. Certificate pinning is automatic: if you trust a DID, you trust that DID's certificate.

**Peer discovery:**

| Method | Range | Use |
|---|---|---|
| **mDNS** | Local (RFC 6762) | Same LAN, local testing |
| **Rendezvous** | Wide-area (custom protocol) | Internet, stable identity |
| **Gossip PEX** | Epidemic (peer exchange) | Bootstrap from known peer |
| **Manual** | Manual configuration | Explicit trust, bootstrapping |

**NAT traversal:**
- **Hole punching:** Both peers send to each other simultaneously; NAT creates bidirectional pinhole
- **STUN:** Detect external IP and port
- **TURN relay:** If hole punching fails, relay traffic through a trusted third party
- **Fallback:** If no NAT traversal works, offer relay option

**Connection management:**
- **Trust-gated limits:** Isolated peers can open 1 connection; Federated can open 10
- **Backoff:** If connection fails repeatedly, exponential backoff before retry
- **Timeouts:** 30s idle timeout; 5 min before closing unused streams
- **Graceful shutdown:** Drain in-flight messages before closing

**Ports:**
| Service | Port | Protocol | Binding |
|---|---|---|---|
| Peer Transport | 7777 | QUIC/UDP | 0.0.0.0:7777 |
| RPC API | 5601 | HTTP | 127.0.0.1:5601 (icnctl) |
| Metrics | 9100 | HTTP | Prometheus exporter |
| Health | 8080 | HTTP | Health checks |
| mDNS | 5353 | multicast | `_icn._udp.local.` (30s scan interval) |

**Status: CONFIRMED** (operational). Crate: `icn-net`. *(§8 verified accurate 2026-06-14: default ports — 7777/UDP transport, 5601 RPC, 9100 metrics, 8080 health — checked against the defaults in icn/crates/icn-core/src/config/mod.rs; gateway health bind 8080 in icn/crates/icn-core/src/config/gateway.rs.)*

<!-- truth: descriptive -->

## 9. Gossip & Synchronization

ICN uses causal consistency (not consensus) to synchronize state across peers. Vector clocks order events causally. Bloom filters deduplicate. Topic ACLs control visibility.

**Why causal consistency instead of consensus?**

Consensus (Raft, PBFT, Hotstuff) requires a quorum to proceed. If 33% of nodes go offline, the network halts. Cooperatives operate across timezones and unreliable networks—nodes will go offline. We need liveness.

Consensus also requires pre-agreed validators. You need to know who is "honest" before the protocol starts. Causal consistency has no such requirement: any peer can participate, and the network works with any liveness distribution.

Causal consistency trades total ordering for availability. Events at different peers might be ordered differently until they communicate. This is correct for cooperatives: governance decisions don't need global total order, just local agreement by the time execution starts.

**Vector clocks:** Each node has a logical clock. When a node creates an event, it increments its own clock. When it receives an event from another node, it updates its clock to max(local, remote) + 1. Two events with the same causal history have the same vector timestamp. The clock is part of every message.

**Gossip algorithm:**
1. **Push phase:** After creating an event, announce it to nearby peers
2. **Pull phase:** Periodically ask peers "what do you have that I don't?" (bloom filter exchange)
3. **Anti-entropy:** Slow background sync to catch up after network failures
4. **Dedup:** Bloom filter prevents re-gossip of known messages

**Topic model:** Nodes subscribe to topics. Topics have ACLs (e.g., "only federation members can read"). Each topic gossips independently. A single node might listen to 10 topics concurrently.

**Topic examples used in practice:**
- `trust:updates` — trust edge changes
- `ledger:entries` — new ledger entries (one per cooperative)
- `governance:proposals` — new proposals
- `compute:submit` / `compute:claim` — task distribution
- `community:updates` — community membership changes
- `federation:registry` — federation discovery

**Conflict resolution:** If two nodes receive entries in different order due to network timing, vector clocks reveal the causal order. Deterministic merge: re-apply events in causal order, same inputs always produce same output.

**Status: CONFIRMED** (operational). Crate: `icn-gossip`. *(§9 verified accurate 2026-06-14: vector clocks, Bloom-filter dedup, and anti-entropy each have dedicated modules in icn/crates/icn-gossip/src/ (vector_clock.rs, bloom.rs, anti_entropy.rs) with push/pull handlers; `namespace:purpose` topic convention confirmed in icn/crates/icn-gossip/src/types.rs and gossip.rs. NEEDS OPS RE-CONFIRMATION: the "2+ months K3s runtime" figure is an operational/deployment claim not verifiable from crate source — left to the status.toml network/deployment row, which is owned by ops.)*

<!-- truth: descriptive -->

## 10. State & Ledger

ICN ledger is double-entry mutual credit with Merkle-DAG structure for tamper evidence. All state is append-only within a node.

**Double-entry invariant:** Every debit to one account has a matching credit to another. Σ debits ≡ Σ credits per unit type. Enforced at kernel level before ledger entry is accepted.

**Multi-unit ledger:** Hours, kWh, USD, or custom units can coexist in one ledger. Each unit has its own accounting context (exchange rates, conversion rules). A single entry might move 5 hours AND 0.5 kWh (composite transaction).

**Merkle-DAG structure:** Each entry is content-addressed (hash = blake3(entry)). Each entry links to its parent and siblings. Root hash commits the entire history. No entry can be modified without changing all descendant hashes. No secret copy of the ledger exists—everyone can independently verify the merkle root.

**Receipt chain:** Governance connects to ledger through receipts:
1. **GovernanceProposal:** "Allocate 100 hours to development"
2. **DecisionReceipt:** Proposal accepted, signed by governance oracle
3. **AllocationReceipt:** Treasury makes the allocation decision
4. **SettlementIntent:** Movement approved by both payer and receiver
5. **LedgerEntry:** Actual debit/credit pair, signed by both parties

Users can trace any ledger entry back to the governance decision that authorized it (if it came from governance). Settlement intents allow informal transfers without governance (user-to-user, peer transactions).

**Credit limits:** Per-participant, per-unit. Cooperative sets an overdraft limit (e.g., "-50 hours"). Account cannot go below that limit.

**Demurrage (planned, not yet implemented):** Time-decay on positions to encourage circulation. "Hold 100 hours for 1 year, automatically lose 2 hours to demurrage." Intended as a governance-scheduled rebase of all positions. *(Verified 2026-06-14: no `demurrage` or position-rebase implementation exists in icn/crates/icn-ledger/. The prior "Phase 25" tag used the retired Phase 1–18/19–35 numbering and was removed per the Phase 0/1/2 model.)*

**Conflict resolution:** Concurrent debits to the same account. Both nodes receive debit A and debit B in different orders. Vector clocks reveal which came first. Nodes apply in causal order. If both were valid (no double-spend), both stick. If one exceeds the credit limit and the other comes first, second debit is rejected.

**Status: CONFIRMED** (operational). Crate: `icn-ledger`. *(§10 verified accurate 2026-06-14: double-entry invariant "Σ debits == Σ credits per currency" and Merkle-DAG addressing confirmed in icn/crates/icn-ledger/src/lib.rs and types.rs; the receipt chain GovernanceDecisionReceipt → AllocationReceipt → SettlementIntent → JournalEntry confirmed in icn/crates/icn-ledger/src/allocations.rs. Only correction was the demurrage phase tag, above.)*

<!-- truth: descriptive -->

## 11. Contract Execution (CCL)

The Cooperative Contract Language (CCL) is a domain-specific language for expressing cooperative agreements. It is NOT Turing-complete by design.

**Design constraints:**
- **Fuel metering:** Every operation costs fuel. Contracts have a fuel budget (e.g., 10,000 units). When fuel runs out, execution halts.
- **Determinism:** No system time, no floating point (all math is fixed-point), no random without seed, canonical input ordering
- **Capability-based security:** Contracts declare ReadLedger, WriteLedger, ReadTrust, SubmitProposal capabilities; kernel gates access
- **Non-Turing-complete:** No unbounded loops. Max loop iterations is statically known.

**Examples of contracts:**
- **TimeBank:** `record_service(recipient, hours)` debits recipient, credits provider, triggers notification
- **CSA (Community Supported Agriculture):** `allocate_share(member, crops)` distributes produce per governance decision
- **Peer lending:** Escrow agreement with interest and collateral logic
- **Commons pool:** Track extraction, authorize sanctions, distribute dividends

**Execution model:**
1. User or governance decision submits a CCL contract
2. Contract is gossipped to the network
3. Executors claim the contract (announce interest)
4. Executor runs it locally, produces output
5. Executor signs proof-of-execution (hash of output, fuel burned, side effects)
6. Proof is gossipped; other nodes verify by re-running the contract

**Determinism guarantees:**
- **No system time:** Contracts can reference "current logical block number" but not wall clock
- **Deterministic PRNG:** Contracts get a seeded random number generator; same seed produces same sequence
- **Fixed-point math:** Use scaled integers (e.g., 1_000_000 = 1 unit). No floating point (non-deterministic rounding)
- **Canonical ordering:** Input sets are sorted before passing to contract (prevents non-determinism from iteration order)

**WASM integration (experimental):** The wasmtime runtime is gated behind the `wasm` feature, which lives in the **`icn-compute`** crate (`wasm = ["wasmtime"]` in icn/crates/icn-compute/Cargo.toml), not in `icn-ccl` itself. This WASM path is experimental and is **not yet held to CCL's determinism and fuel guarantees**: the executor constructs `Engine::default()` (no wasmtime fuel metering enabled — the source comments "fuel metering can be enabled later") and applies only a coarse per-call fuel counter, and it links a `timestamp` host function backed by wall-clock `SystemTime::now()`, which is non-deterministic. WASM offers more expressiveness (loops, recursion), but holding it to CCL-equivalent fuel metering, deterministic imports, and capability gating is future work. CCL itself (the interpreter path) retains the deterministic, fuel-metered, capability-gated guarantees described above.

**Status: WORKING BUT IMMATURE** (CCL interpreter executes deterministically; the optional WASM path is experimental). Crate: `icn-ccl` (interpreter + fuel metering); experimental WASM execution in `icn-compute`. *(§11 verified 2026-06-14: FuelConfig, capability-based security (ReadLedger/WriteLedger), deterministic bounded-expression evaluation, and the Interpreter confirmed in icn/crates/icn-ccl/src/ (lib.rs, runtime.rs, schema/mod.rs, fuel_estimator.rs). Corrections: (1) the wasmtime dependency and `wasm` feature were re-attributed from icn-ccl to icn-compute, where they actually live; (2) the prior claim that WASM execution is held to the same fuel/determinism/capability guarantees as CCL was inaccurate and removed — icn/crates/icn-compute/src/wasm_executor.rs uses `Engine::default()` (no fuel metering; comment "fuel metering can be enabled later") plus a coarse per-call fuel counter, and exposes a wall-clock `timestamp` host import (`SystemTime::now()`), so the WASM path is non-deterministic and unmetered today.)*

<!-- truth: descriptive -->

## 12. Governance

Governance is the process by which communities make decisions collectively. ICN provides the infrastructure; communities configure the rules.

**Proposal lifecycle (9 states):**
1. **Draft** — someone writes a proposal (text, budget, membership, config change)
2. **Deliberation** — published for discussion before voting opens (optional phase with configurable duration)
3. **Open** — voting begins (can skip deliberation for expedited proposals)
4. **Accepted** — reached quorum + threshold
5. **Rejected** — failed to reach threshold
6. **NoQuorum** — voting period closed without enough votes
7. **Cancelled** — author withdrew the proposal before terminal state
8. **Vetoed** — emergency governance action overrides the process (with recorded reason)
9. **ForceClosed** — proposal closed before voting period ended with a forced outcome (with reason)

Terminal states: Accepted, Rejected, NoQuorum, Cancelled, Vetoed, ForceClosed. Emergency actions (cancel, veto, force-close) are available from any non-terminal state.

**Proposal types:**
- **Text** — signaling, guidance, no automatic effect
- **Budget** — allocate funds to an activity
- **Membership** — admit/remove/suspend members
- **ConfigChange** — amend governance rules themselves

**Vote types:** For, Against, Abstain

**Quorum:** Minimum percentage of eligible voters who must participate. E.g., "50% of members must vote."

**Approval threshold:** Minimum percentage of votes for acceptance. E.g., "66% of votes must be For."

**Delegation (liquid democracy):** Members can delegate their vote to a representative. Delegation is scoped: "I delegate to Alice for labor decisions, to Bob for finance decisions." Delegation has expiry: "My delegation to Alice expires in 90 days."

**Charter system:** A governance constitution defining proposal types, quorum/threshold rules, committees and amendment procedures. Charters are **authored as CCL YAML**. Two cautions, both verified at `74c832f1`: **(a)** the starter templates in `docs/reference/ccl-charter-templates.md` still wrap every example in a top-level `charter:` object, which `CclDocument` neither recognises nor rejects — submitting one activates an empty, permissive charter, so follow the key set shown below rather than the template wrapper until those templates are fixed; **(b)** active CCL charters are **not persisted** — the acceptance hook loads them into `CharterPolicyOracle`, whose store is an in-memory `HashMap<String, (CclDocument, CharterContext)>` (`icn/apps/charter/src/oracle.rs:48`) initialised empty at startup, so a ratified charter is **lost on restart unless redeployed**. `charter_store.rs` serialises the separate `icn_governance::Charter` founding-document model and has no production consumer outside its own tests; it is not where active CCL charters live. *(Corrected 2026-08-14: this section previously described and demonstrated a TOML charter; `icn/crates/icn-governance/src/charter.rs` contains no TOML support, so the old example would have led authors to the wrong representation.)* Example, abridged from `contracts/templates/worker-coop.yaml`. **The top-level keys must be those `CclDocument` recognises** — `schema_version`, `entity`, `governance`, `economics`, `agreement`, `extensions` (`icn/crates/icn-ccl/src/schema/mod.rs:52-77`). Unknown top-level keys are *not* rejected, so a document wrapped in some other root object parses as empty and translates to no constraints:

```yaml
schema_version: v0

entity:
  name: "Example Worker Cooperative"
  type: cooperative
  subtype: worker
  membership:
    open_to: [individual]
    classes:
      - name: worker_owner
        default: true
    rights_by_class:
      worker_owner: [vote, propose, patronage_refund]

governance:
  bodies:
    - name: general_assembly
      composition: all_members
      convenes:
        regular: quarterly
```

**Amendment process:** Changing the charter requires a super-majority (e.g., 75% approval). Amendments are themselves proposals and can be vetoed by committees (if defined).

**Appeals:** Members can appeal a decision (e.g., "this proposal violates the charter"). Appeals are heard by an appeals committee and can overturn decisions.

**Cryptographic decision proofs:** Every governance decision produces a signed proof (GovernanceProof). The proof commits the proposal, votes, threshold crossed, and decision timestamp. Proofs are append-only and gossipped.

**Status: CONFIRMED** (operational — proven in demo flows). Crate: `icn-governance`. *(§12 verified accurate 2026-06-14: the 9-state proposal lifecycle — Draft, Deliberation, Open, Accepted, Rejected, NoQuorum, Cancelled, Vetoed, ForceClosed — checked against `enum ProposalState` in icn/crates/icn-governance/src/proposal.rs, including the documented emergency-action transitions. The earlier "547 tests" figure is now stale and undercounts — a grep of `#[test]`/`#[tokio::test]` in icn-governance alone returns 600+ — so the exact count was dropped here rather than re-asserted; status.toml governance row carries the count and is flagged for an exact re-derivation.)*

<!-- truth: descriptive -->

## 13. Entity Model

ICN entities are composable, recursive, and non-exclusive. A person can be a member of a cooperative AND a community AND a federation simultaneously. Cooperatives can be members of federations.

**Four entity types:**
- **Person** — individual, requires SDIS identity
- **Cooperative** — economic entity, members are persons or other coops
- **Community** — civic entity, members are persons, coops, or other communities
- **Federation** — cross-institutional entity, bridges coops and communities

**EntityId format:** `entity:icn:<type>:<identifier>`
- `entity:icn:individual:z5TrA8...` (wraps DID)
- `entity:icn:cooperative:food-coop-2024`
- `entity:icn:community:rochester-tech-workers`
- `entity:icn:federation:northeast-fed`

**Membership model:** Recursive and non-exclusive.
```
Person ──→ Cooperative (labor, capital, economics)
       ├→ Community (governance, voice, solidarity)
       └→ Federation (interop, mutual credit)

Cooperative ──→ Federation
Community ──→ Federation
Federation ──→ Federation (nested federations)
```

**Cooperative lifecycle** — five states (`CoopStatus`, `icn/crates/icn-coop/src/types.rs:344`; mirrored by `EntityStatus`, `icn/crates/icn-entity/src/entity.rs:304`):
1. **Forming** — charter drafted, initial capital pledged
2. **Active** — members joined, operations running
3. **Suspended** — participation halted without dissolution
4. **Dissolving** — asset distribution plan approved, wind-down in progress
5. **Dissolved** — wind-down complete

**Treasury management:** Each cooperative has a deterministic treasury **anchor**, derived from the coop id by BLAKE3 (`derive_treasury_did`, `icn/crates/icn-coop/src/lifecycle.rs:22`). Treasury manages budgets, allocates surplus, settles inter-coop trades; no separate treasury entity is needed. **Note:** despite the `did:icn:treasury:…` shape and the function name, this is a **keyless identifier string** held in an `Option<String>` — it has no keypair behind it and would not satisfy `Did::from_str` (`icn/crates/icn-identity/src/lib.rs:210-241`). A second, non-equal treasury identifier is derived independently at `icn/crates/icn-coop/src/actor.rs:489`.

**Community types:** Geographic (city/region), Interest (hobby/profession), Solidarity (mutual aid network), Ecosystem (full cooperative ecosystem) — four variants (`icn/crates/icn-community/src/types.rs:9-14`). Types are configurable; these are defaults.

**Status: CONFIRMED** (descriptive). Crates: `icn-entity`, `icn-coop`, `icn-community`. *(§13 re-verified 2026-08-14 at 74c832f1: four entity types confirmed in `EntityType`/`EntityKind` (icn/crates/icn-entity/src/entity.rs:265,:429). **Two corrections to the 2026-06-14 pass:** the cooperative lifecycle has five states, not three — `Forming, Active, Suspended, Dissolving, Dissolved` (icn/crates/icn-coop/src/types.rs:344, mirrored by `EntityStatus` at icn-entity/src/entity.rs:304); and `CommunityType` has four variants, not three (icn/crates/icn-community/src/types.rs:9-14). Treasury derivation via `derive_treasury_did` (icn/crates/icn-coop/src/lifecycle.rs:22) yields a **keyless identifier string**, not a keypair-backed DID — it would not satisfy `Did::from_str`.)*

<!-- truth: descriptive -->

## 14. Federation

Federation enables multiple cooperatives to coordinate without surrendering autonomy. Each coop keeps its ledger, governance, and trust decisions. Federation is the bridge.

**Capabilities:**
- **Discover** — announce your coop's existence, find others via gossip
- **Attest trust** — endorse another coop's identity (federation says "I vouch for Food Coop A")
- **Settle credits** — transfer units between coops via bilateral credit agreements
- **Route gossip** — cooperatives can optionally share gossip topics (federation-scoped visibility)
- **Resolve DIDs** — federated DID resolution (find Alice in Food Coop A even if you're in Tech Coop B)

**Cross-coop settlement:** Federation does not execute transfers. Each coop settles independently. Example:
1. Alice (member of Food Coop A) transfers 5 hours to Bob (member of Tech Coop B)
2. Food Coop A debits Alice, credits Tech Coop B (in a "inter-coop receivables" account)
3. Tech Coop B credits Bob, debits Food Coop A
4. Both ledgers record the transaction; both can independently verify

**Federated DID resolution:** A DID like `did:icn:food-coop:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK` includes a coop prefix. Federation nodes route DID resolution requests to the named coop.

**Registry and attestations:** Federation gossips coop announcements (coop name, DID, trust context) and attestations in which federation member A attests to member B's identity. The attestation **type** is signature-bearing, but signing is **not installed in production**: `FederationGossipHandler::set_keypair` (`gossip.rs:83`) has callers only under `#[cfg(test)]`, and the composition root (`icn-core/src/supervisor/init_federation.rs:128-133`) never installs a keypair, so announcements are emitted unsigned and are then **rejected** by the receiver's signature check (`gossip.rs:203-210`), meaning production peer discovery through this path does not complete; the RPC issue path (`icn-rpc/src/handler/federation.rs:566-575`) likewise stores attestations without calling `.sign()`. *(Verified 2026-08-14: `FederatedTrustAttestation` in icn/crates/icn-federation/src/attestation.rs; signing-installation gap tracked as #2500.)*

**Clearing (added since the March 2026 snapshot):** Federation has bilateral clearing agreements (`CrossCoopTransfer`, Phase F3) and a receipt-clearing path for cross-scope settlement (`ReceiptClearingManager` with batch clearing, Epic 4 / #941). This is genuine cross-coop settlement infrastructure. A multilateral net-position solver also exists and settles: `NettingEngine` (`netting.rs:44`) computes cancellable cycles via `ClearingManager::perform_multilateral_netting` (`clearing_manager.rs:414-420`, informational per `:412`), and `apply_multilateral_netting` (`:472-535`) updates and persists positions. Both are exposed as gateway endpoints (`icn-gateway/src/api/federation.rs:1140,:1177-1190`, registered at `server.rs:2534-2535`). *(Correction 2026-08-14: this paragraph and the gap below previously stated that no multilateral netting existed. Verified against icn/crates/icn-federation/src/{clearing.rs,receipt_clearing.rs,netting.rs,clearing_manager.rs} and the gateway route registration.)*

**Known gaps (honest; federation gaps re-verified 2026-08-14 against icn/crates/icn-federation/src/, remainder 2026-06-14):**
- *(Resolved 2026-08-14 — retained for history: this previously read "no multilateral netting algorithm". Netting is built end to end, including the apply/settle step; see the clearing paragraph above.)*
- No dispute-resolution engine — agreements carry only an optional `dispute_resolution: Option<String>` *method label* (icn/crates/icn-federation/src/agreement/types.rs); disagreements still require human intervention
- No three-epoch checkpointing / explicit partition-healing protocol (no `checkpoint`/`epoch` types in the crate; partition healing is eventual via gossip)
- No BFT (Byzantine fault tolerance assumes honest majority, not 1/3 adversary — no `bft`/`byzantine` implementation in the crate)
- No cross-federation async proof exchange (federation is single-cloud, not multi-federation)

**Status: PARTIALLY CONFIRMED** (registry types and bilateral clearing work; attestation *signing* is not installed in production, so announcements are rejected by the receiver's signature check — see above; multilateral netting is built including its apply step; BFT and dispute resolution incomplete). Crate: `icn-federation`. *(Note: the `federation:write` and `treasury:write` scopes that the March status snapshot flagged as "missing from K3s ALLOWED_SCOPES" are now present in icn/crates/icn-gateway/src/validation.rs::ALLOWED_SCOPES — see §status.toml correction.)*

<!-- truth: descriptive -->

## 15. Distributed Compute

Distributed compute enables cooperatives to execute work (CCL contracts or WASM) across a network of trusted nodes. Work is trust-gated and auditable. **Fuel metering applies to the CCL interpreter, not to the WASM path** — see below.

**Work submission flow:**
1. User submits CCL contract or WASM module to the network
2. Contract is gossipped via `compute:submit` topic
3. Executors monitor the topic, evaluate trust of submitter
4. Trusted executors claim the work (gossip `compute:claim`)
5. Executor runs the contract locally, produces output
6. Executor signs proof-of-execution (merkle root of output)
7. Proof gossips; others verify by re-running the contract
8. Mismatch triggers fraud accusation and governance intervention

**Trust-gated execution:** Only executors with sufficient trust score attempt to execute. Isolated peers can't claim tasks. Partner+ peers can. (Configurable per cooperative.)

**Fuel metering (CCL only):** CCL contracts have fuel budgets and execution halts when fuel runs out (`icn-ccl/src/types.rs:399-402` returns an error on depletion; charged per statement at `interpreter.rs:162`), preventing infinite loops and resource exhaustion. **The WASM path is NOT fuel-metered:** `icn-compute/src/wasm_executor.rs:58-59` constructs `Engine::default()` with the comment that "fuel metering can be enabled later", and accounting is a flat `saturating_sub(100)` per call (`:163,:211`). WASM host imports also use wall-clock `SystemTime::now()` (`:248,:331,:428`), so that path is neither metered nor deterministic. *(Corrected 2026-08-14 at 74c832f1.)*

**Proof verification:** Other nodes re-execute the contract independently. If output differs, the executor is penalized (reputation hit, possible expulsion). This creates incentive for honest execution without requiring a consensus mechanism.

**Foundation status:** Runtime exists, contract examples work, no marketplace/bidding logic yet. A task cost model has landed since the March snapshot (`icn/crates/icn-compute/src/cost.rs`), so compute is no longer strictly "free". **A workload manifest does now exist** — the E1 field block on `ComputeTask` (`icn/crates/icn-compute/src/types.rs:318-322` ff: `inputs_hash`, `policy_hash`, `determinism_class`, `privacy_class`), recorded in ADR-0030 as `status: accepted` / `implementation_status: implemented`. What remains absent is a job marketplace and bidding. *(Corrected 2026-08-14 at 74c832f1; the prior note inferred absence from a search for a type literally named `WorkloadManifest`.)*

**Status: FOUNDATION ONLY** (runtime, cost model and workload manifest exist; marketplace/bidding not built). Crate: `icn-compute`

<!-- truth: operational -->

## 16. Security

ICN uses a three-layer security model: transport, message, application.

**Layer 1: Transport Security**

QUIC/TLS 1.3 encrypts all peer-to-peer traffic. Each peer has a DID. TLS certificate subject is the peer's DID. Certificate pinning is automatic: if you trust a DID, you trust that DID's certificate. No CA certificate authority needed; trust is peer-to-peer.

**Layer 2: Message Security**

Every gossip message is wrapped in a `SignedEnvelope`:
```rust
pub struct SignedEnvelope {
    pub from: Did,                      // Who signed it
    pub sequence: u64,                  // Per-sender sequence number
    pub timestamp: u64,                 // Wall clock (advisory, not trusted)
    pub payload_type: PayloadType,      // Discriminator (Gossip, Ledger, Trust, Contract, Rpc, Control, Encrypted)
    pub payload: Vec<u8>,               // The actual message
    pub signature_type: SignatureType,  // Classical or Hybrid
    pub signature: Vec<u8>,             // Ed25519 signature
    pub pq_signature: Option<Vec<u8>>,  // Optional ML-DSA post-quantum signature
}
```

Receiver verifies signature against signer's public key (both classical and PQ if present). Receiver rejects messages with sequence numbers ≤ last-seen from that sender (replay protection). Timestamp is checked but not trusted (receiver's clock is the source of truth for ordering). Hybrid signatures require both Ed25519 and ML-DSA to verify.

**Layer 3: Application Security**

End-to-end encryption for sensitive data. Shared secrets negotiated via X25519. Payload encrypted with ChaCha20-Poly1305. Only sender and recipient can decrypt.

**Rate limiting:**
- Isolated peers: 10 msg/s
- Known: 50 msg/s
- Partner: 100 msg/s
- Federated: 200 msg/s

Configurable per cooperative. Trust graph change invalidates rate limit cache (re-evaluate trust).

**Spam defenses:**
- Bloom filter dedup (drop duplicate messages)
- Trust-gated acceptance (drop from unknown peers)
- Rate limiting (drop excess messages)
- Backoff (if peer sends spam, increase reconnection delay)

**Attack resistance table:**

| Attack | Defense |
|---|---|
| Sybil (many fake accounts) | SDIS (Sybil-resistant identity) |
| Replay (reuse old messages) | Sequence tracking + timestamp checks |
| Man-in-the-middle | DID-TLS pinning + signature verification |
| DoS (flood network) | Rate limiting + trust-gated access |
| Bad-mouthing (accuse peer of being untrustworthy) | Trust is subjective local score; bad-mouth doesn't propagate |
| Eclipse (isolate node from network) | Multiple peer connections, peer discovery diversity |
| Double-spend (debit same account twice) | Double-entry invariant + consensus on entry order |

**Privacy / anonymity status (re-verified 2026-06-14):**
- **Onion routing — now implemented** (was previously listed here as deferred). `icn-privacy` ships a 540-line `OnionRouter` (layered encryption, circuits, `wrap_message`, `peel_layer`, metrics) in icn/crates/icn-privacy/src/onion_routing.rs, and it is **wired into the network actor**: `MessagePayload::Onion` is dispatched in icn/crates/icn-net/src/actor/connection.rs and relayed by `handle_onion`/`forward_onion` in icn/crates/icn-net/src/handlers/onion.rs. This is a crate-level multi-hop capability; it is not claimed as production-grade anonymity (no adversarial traffic-analysis audit). NEEDS SME review: end-to-end anonymity guarantees and threat model.
- **Traffic obfuscation / cover traffic — built at crate level, NOT integrated.** `icn/crates/icn-privacy/src/traffic_obfuscation.rs` (385 lines) implements `TrafficObfuscator` with cover-message generation and size padding, exported at `icn-privacy/src/lib.rs:63` — but its only consumers are `icn-privacy/tests/privacy_integration.rs`; nothing in `icn-net` calls it. *(Corrected 2026-08-14: previously stated no implementation existed.)*
- **Perfect forward secrecy (PFS) per session** — not yet a standing guarantee (achievable with session ephemeral keys).

*(Numbering note: the previous "Phase 21" tags on onion routing / traffic obfuscation used the retired Phase 1–18/19–35 scheme and have been removed per the Phase 0/1/2 model in CLAUDE.md / docs/PHASE_PROGRESS.md.)*

**Status: CONFIRMED** (operational). Crates: `icn-net`, `icn-crypto`, `icn-security`, `icn-privacy`. *(§16 verified accurate 2026-06-14: three-layer model — QUIC/TLS transport, `SignedEnvelope` Ed25519 + replay sequence, ChaCha20-Poly1305 E2E — and the per-class rate limits 10/50/100/200 msg/s checked against icn/crates/icn-net/src/rate_limit.rs. Onion-routing claim corrected above.)*

<!-- truth: descriptive -->

## 17. Crate Map

**Workspace: 48 packages** — 38 crates in `icn/crates/`, 7 apps in `icn/apps/` (charter, governance, membership, ledger, ledger-app, governance-app, trust-app), 3 binaries in `icn/bins/` (icnd, icnctl, icn-console). *(Verified 2026-07-13 against `[workspace].members` in icn/Cargo.toml: 38 `crates/*` + 7 `apps/*` + 3 `bins/*` = 48. There are 39 crate directories on disk; `icn-baseline-lock-guest` is present but excluded from the workspace. The previous "44 packages / 37 crates + 4 apps" figure, verified 2026-06-14, was stale.)*

**Kernel crates** (domain-agnostic, enforce constraints):

| Crate | Role | Infection Status |
|---|---|---|
| `icn-kernel-api` | Trait definitions (PolicyOracle, State, Compute, Comms) | Clean |
| `icn-core` | Actor runtime, supervisor, shutdown | Infected (medium — **0** governance refs, **5** ledger refs, **32** ccl refs; pinned by meaning_firewall.rs ratchets) |
| `icn-net` | QUIC/TLS transport, peer discovery | Infected (medium) |
| `icn-gossip` | Pub/sub, vector clocks, anti-entropy | Infected (medium) |
| `icn-store` | KV storage (Sled backend) | Clean |
| `icn-encoding` | Versioned serialization | Clean |
| `icn-obs` | Prometheus metrics, OpenTelemetry | Clean |
| `icn-rpc` | JSON-RPC over HTTP | Infected (low) |
| `icn-time` | Logical clocks, scheduling | Clean |
| `icn-snapshot` | State snapshots | Clean |
| `icn-testkit` | Shared test utilities | Clean |
| `icn-http-kit` | Shared HTTP scaffolding | Clean |
| `icn-gateway` | REST + WebSocket API | Infected (medium — knows about domain endpoints) |
| `icn-api` | Shared API types and validation | Clean |
| `icn-services` | Service registration and lifecycle | Needs Review |
| `icn-crypto` | Core crypto (Ed25519, X25519) | Clean |
| `icn-protocol` | Wire format, message types | Clean |
| `icn-privacy` | Metadata protection, encrypted topics, onion routing | Clean |
| `icn-security` | Security utilities, attack detection | Clean |
| `icn-boundary` | Deterministic host/WASM-guest shared types (postcard, no floats) | Clean |
| `icn-baseline-lock` | Executable single-node baseline-loop fixture (no networking) | Clean |

*(Added 2026-06-14: `icn-boundary` and `icn-baseline-lock` are newer workspace crates not previously in this table; both have zero domain-crate dependencies — verified against their Cargo.toml `[dependencies]`.)*

**Domain crates** (implement PolicyOracle, interpret domain semantics):

| Crate | Role | Status |
|---|---|---|
| `icn-identity` | DID management, SDIS, cryptography | Exceeds Spec |
| `icn-crypto-pq` | Post-quantum (ML-KEM, ML-DSA) | Exceeds Spec |
| `icn-zkp` | Zero-knowledge proofs | Exceeds Spec |
| `icn-steward` | SDIS steward network | Exceeds Spec (infected — low) |
| `icn-trust` | Trust graph, scoring, oracles | Confirmed (infected — high, cascades) |
| `icn-ledger` | Double-entry mutual credit | Confirmed (infected — high) |
| `icn-ccl` | Cooperative Contract Language | Working (clean) |
| `icn-governance` | Proposals, voting, decision records | Confirmed (infected — medium) |
| `icn-authz` | Capability graph, authorization | Confirmed |
| `icn-entity` | Unified entity abstraction | Confirmed (infected — medium) |
| `icn-coop` | Cooperative lifecycle & membership | Confirmed (infected — medium) |
| `icn-community` | Communities (civic engine) | Confirmed (infected — medium) |
| `icn-federation` | Cross-cooperative coordination | Partially Confirmed (infected — medium) |
| `icn-naming` | Name resolution, service discovery | Confirmed (clean) |
| `icn-commons` | Commons store / handle (capital-C Commons primitives) | Infected (needs review) — direct `icn-governance` (domain) dep |
| `icn-compute` | Trust-gated WASM execution | Foundation Only (needs review) |

*(Added 2026-06-14; corrected 2026-06-15: `icn-commons` is a newer workspace crate — commons store/handle/inner — not previously in this table. **It is NOT clean by import:** `icn/crates/icn-commons/Cargo.toml` `[dependencies]` declares a direct `icn-governance = { path = "../icn-governance" }` dep, and `icn-governance` is an infected/domain crate (see Domain crates above and `kernel_surface.toml`). The crate also pulls `icn-identity`, `icn-obs`, `icn-time` (all clean), but the `icn-governance` edge alone makes the earlier "zero domain-crate dependencies" claim false — hence Infected (needs review). Note: `icn-commons` is **not** part of the `kernel_surface.toml` 28-crate inventory, so it is not counted in the Meaning Firewall recount below; the 13/14/1 totals are unchanged by this correction.)*

**Meaning Firewall audit (recomputed 2026-06-14 from the per-crate `[crates.*]` entries in kernel_surface.toml):** of the **28** crates inventoried, **13 are clean**, **14 are infected**, and **1 needs review** (`icn-compute`); plus **1 extracted app** (`icn-governance-actor`).

- **Clean (13):** icn-api, icn-ccl, icn-crypto-pq, icn-encoding, icn-identity, icn-naming, icn-obs, icn-privacy, icn-security, icn-snapshot, icn-store, icn-testkit, icn-time.
- **Infected (14):** icn-community, icn-coop, icn-core, icn-entity, icn-federation, icn-gateway, icn-gossip, icn-governance, icn-ledger, icn-net, icn-rpc, icn-steward, icn-trust, icn-zkp (highest severity: icn-trust and icn-ledger, which cascade).
- **Needs review (1):** icn-compute.

> **Upstream summary regenerated (PR #2110, 2026-06-20):** the `[metadata]` block at the top of kernel_surface.toml previously read `total_crates=29, clean=9, infected=11, needs_review=9` (dated 2026-02-20) and did **not** match its own per-crate entries. It has since been regenerated from those entries (28 / 13 clean / 14 infected / 1 needs-review, + 1 extracted app), so it now matches the recount above and the `[meaning-firewall]` block in docs/status.toml. The per-crate classifications themselves were not re-audited in that change — that remains separate meaning-firewall work.

Extraction is incremental: GovernanceActor already moved to apps/governance (Feb 2026), and icn-core's direct `icn_governance::` references are now **0** (pinned in meaning_firewall.rs). Next targets: icn-trust (cascades infections), icn-ledger, and completing the icn-core ledger/ccl extraction (5 ledger refs, 32 ccl refs remain).

**Apps** (runtime-integrated in `icn/apps/`):
- `apps/governance` — Governance execution (GovernanceActor, REST API)
- `apps/ledger` — Ledger persistence and settlement
- `apps/membership` — Entity lifecycle management
- `apps/charter` — Charter system (governance configuration)

<!-- truth: operational -->

## 18. Institution Primitives

The eight kernel primitives compose into four institution primitives. This is the payoff: infrastructure-grade capabilities that institutions need.

**Institution primitive 1: Identity & Membership**

Uses kernel primitives: Identity, Authorization, State, Naming

Cooperatives need to know who their members are, how they join, how they leave. ICN provides:
- Identity: Cryptographic proofs (DIDs)
- Naming: Member lookup (`/org/food-coop/alice`)
- State: Membership ledger (who is in, who is out)
- Authorization: Membership gates governance voting

**Institution primitive 2: Governance**

Uses kernel primitives: State, Communication, Coordination, Authorization, Time

Cooperatives need to make decisions collectively. ICN provides:
- State: Proposal ledger
- Communication: Announce proposals, gossip votes
- Coordination: Tally votes across peers (no consensus needed, just gossip) — **note:**
  inbound governance replication is currently refused, so votes do not propagate between
  nodes today; tallying is per-node over locally recorded votes. See §4.7 and issue #2469
- Authorization: Gates voting by membership
- Time: Voting period (block numbers, not wall time)

**Institution primitive 3: Economics**

Uses kernel primitives: State, Compute, Authorization

Cooperatives need to track labor, capital, surplus. ICN provides:
- State: Double-entry ledger
- Compute: CCL contracts for allocation rules (who gets paid when)
- Authorization: Gates transfers by credit limits, governance approval

**Institution primitive 4: Federation**

Uses kernel primitives: Communication, Naming, Authorization, Coordination

Cooperatives need to interoperate without surrendering autonomy. ICN provides:
- Communication: Gossip federation announcements
- Naming: Resolve DIDs across coops
- Authorization: Trust attestations (federation vouches for other coop)
- Coordination: Settle credits between ledgers

**Cooperative starter bundle** *(historical working name: "Institution-in-a-Box" — retired as product framing; see [the archived vision](archive/2026/VISION-institution-in-a-box.md))*:

A new cooperative needs:
1. A charter (governance rules) — written in YAML (CCL schema; parsed by `CclDocument::from_yaml`, templates at `contracts/templates/*.yaml`)
2. Initial members (founding group) — DIDs
3. A name (`/org/my-coop`) — registered
4. Ledger initialization (unit types, credit limits) — config

That's it. Governance, voting, member management, ledger operations, inter-coop settlement all follow from the primitives. No new code needed for most cooperatives; CCL contracts handle domain-specific rules.

<!-- truth: normative -->

## 19. Implementation Status

This is the honest scorecard of what's complete, what's working, and what's known to be missing.

| Subsystem | Status | Evidence | Truth Class |
|---|---|---|---|
| **Identity** | EXCEEDS SPEC | SDIS + PQ crypto operational; DID issuance/resolution proven in tests, demos, and the assembled-image witness (K3s deployment liveness: NEEDS OPS RE-CONFIRMATION — see below) | operational |
| **Governance** | CONFIRMED | 9-state proposal lifecycle proven in demos (test count needs re-derivation — see status.toml note; 547 figure is stale) | operational |
| **Economics (mutual credit)** | CONFIRMED | Ledger entries gossip, double-entry enforced, multi-unit works | operational |
| **Federation** | PARTIALLY CONFIRMED | Registry works, bilateral settlement works; async proofs + netting unbuilt | operational |
| **CCL** | WORKING BUT IMMATURE | Runtime executes, contracts run locally and gossip proofs; no pattern library | descriptive |
| **Storage** | SIMPLIFIED | Merkle-DAG for ledger only; blobs/KV simplified since design phase | descriptive |
| **Distributed compute** | FOUNDATION ONLY | Runtime works, fuel metering enforced, no marketplace/compensation yet | operational |
| **Security** | CONFIRMED | TLS + signatures + replay protection live; onion routing now implemented (icn-privacy + icn-net wiring), not deferred | operational |
| **Network** | CONFIRMED | QUIC works, mDNS discovery live, NAT traversal (hole punch + relay) proven | operational |
| **Entity model** | CONFIRMED | Recursive membership works, treasury derivation live | operational |

**Infection status (recomputed 2026-06-14 from kernel_surface.toml per-crate entries — see §17):** of 28 inventoried crates, **13 clean / 14 infected / 1 needs-review**, plus 1 extracted app. Highest priority: icn-trust and icn-ledger (cascade infections). GovernanceActor extraction to apps/ complete (Feb 2026); icn-core governance refs now 0; trust and ledger extractions next. The earlier `9/11/9` figure here came from the then-out-of-date kernel_surface.toml `[metadata]` summary; that summary was regenerated to match its per-crate entries in PR #2110 (2026-06-20).

**Test coverage:** NEEDS RE-DERIVATION. The previously asserted "2,287 tests across 32 crates" is stale: the workspace now has 38 crates + 7 apps + 3 binaries = 48 members (verified 2026-07-13 against icn/Cargo.toml), and a per-crate `#[test]`/`#[tokio::test]` grep of icn-governance alone exceeds 600. An exact, tool-derived total (`cargo test` summary) is owed before a precise number is re-asserted; the count was intentionally not date-stamped as verified here. CI runs on every commit.

**Operational deployment (NEEDS OPS RE-CONFIRMATION):** runtime liveness/uptime/no-data-loss are operational claims not verifiable from source in this docs pass; see the ops-owned network/deployment rows in `docs/status.toml`. Cluster and ports unchanged from the prior revision.

<!-- truth: operational -->

## 20. Reference Index

<!-- truth: descriptive -->

Deep-dive documents by category. Generated from `docs/registry.toml`.

**Architecture & Design**
- [The Commons](architecture/THE_COMMONS.md) — Capital-C Commons doctrine; what ICN exists to enable (doctrine)
- [Member Standing and Accessible Participation](architecture/MEMBER_STANDING.md) — `/me/standing` design contract; accessibility as anti-capture infrastructure (design contract)
- [Kernel/App Separation](architecture/KERNEL_APP_SEPARATION.md) — Five invariants, Meaning Firewall design (normative)
- [Institution-in-a-Box](design/institution-in-a-box.md) — Four institution primitives, starter kit (normative)
- [ADR-001: What ICN Is](strategy/ADR-001-What-ICN-Is.md) — Scope, non-goals, boundary conditions (normative)

**Governance & Regulation**
- [Regulatory-Safe Verifiable State](design/regulatory-safe-verifiable-state.md) — Seven invariants, terminology discipline, anti-features (normative)
- [Gap Analysis March 2026](strategy/ICN-Gap-Analysis-March-2026.md) — Subsystem-by-subsystem: origin, spec, reality, verdict (descriptive)

**Strategy & Planning**
- [Live Roadmap](strategy/ICN-Roadmap-Live.md) — NOW/NEXT/LATER phased plan with dependencies (operational)
- [Technical Whitepaper](strategy/ICN-Technical-Whitepaper.md) — Formal spec for grant proposals (normative)
- [Roadmap Strategy](strategy/ICN-Roadmap-Strategy.md) — Long-term vision stages A through D (normative)

**Deployment & Operations**
- [Getting Started](GETTING_STARTED.md) — Installation and first steps (guide)
- [Contributing](../CONTRIBUTING.md) — Development workflow and architectural guardrails (guide)
- [Production Hardening](production-hardening.md) — Security and deployment (guide)

**Reference**
- [kernel_surface.toml](../kernel_surface.toml) — Crate-by-crate infection audit with ratchet values
- [docs/registry.toml](registry.toml) — Machine-readable doc inventory (auto-maintained)
- [docs/freshness.toml](freshness.toml) — Section-level freshness tracking
- [docs/status.toml](status.toml) — Implementation scorecard (human-asserted + CI-verified)

---

**Last updated:** 2026-06-14 (freshness re-verification for #2047 — see per-section "Verified against …" / "NEEDS … review" annotations above). Prior content edits: 2026-04-28 (#1672, MEMBER_STANDING + THE_COMMONS), which post-dated the previous 2026-03-21 review stamp.

**Next review:** scheduled per the docs/freshness.toml thresholds. *(The previous "After Phase 21 / Phase 22" target used the retired Phase 1–18/19–35 numbering; under the current Phase 0/1/2 model the project is in Phase 2 — Pilot Launch. Privacy/security-hardening work like onion routing has already landed, see §16.)*

**Canonical reference:** This document is the hub. All links point to deep-dive docs; none are repeated here. If a section feels too shallow, the linked doc has the full detail.
