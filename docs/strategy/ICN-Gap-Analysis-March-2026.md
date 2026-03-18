# ICN System-by-System Gap Analysis: Vision vs. Reality

**March 17, 2026 | Tracing each subsystem from conceptual origin through protocol spec to current implementation**

*Cross-referenced from: 9 protocol specs (Aug 2025), 7 current status files (Mar 2026), 11 research/ecosystem docs, and the full conceptual archive.*

---

## How to Read This Document

For each of ICN's core subsystems, I trace three layers:

1. **The Origin** — where the idea came from (philosophical, academic, or practical)
2. **The Spec** — what the August 2025 protocol suite promised
3. **The Reality** — what actually exists in code as of March 2026
4. **The Verdict** — confirmed, gap, or divergence

---

## 1. IDENTITY & CRYPTOGRAPHY

### Origin
The March 31, 2024 founding conversation asked: "Can't the ICN be the IdP?" — establishing that identity IS the network, not something external. The DSL Design Research (March 2024) established that cooperative members must understand the rules governing them, which requires identity to be self-sovereign and human-legible. The Revolutionary Strategy Synthesis grounded this in Ella Baker's "strong people don't need strong leaders" — identity as self-determination.

### Spec (Identity Protocol, Aug 2025)
- Self-sovereign DIDs (`did:icn:` format), W3C compliant
- Five DID types: Person, Organization, Device, Service, Ephemeral
- Verification methods: Ed25519, ECDSA, RSA, BLS12381
- Mana burn on DID creation (anti-Sybil)
- Key rotation, social recovery, multi-device binding
- Zero-knowledge proofs for attribute verification
- ICNMetadata: org membership, node type, compute score, trust score, recovery contacts

### Reality (March 2026)
- **WORKING:** Full DID system (Ed25519-based `did:icn:<base58pubkey>`)
- **WORKING:** Two DID types implemented: Traditional + SDIS Anchor (privacy-preserving via VUI)
- **WORKING:** SDIS steward network, threshold PRF, enrollment ceremonies
- **WORKING:** VUI (Verifiable Unique Identifier) via Merkle + RSA accumulators
- **WORKING:** Post-quantum cryptography (Kyber/Dilithium) as optional feature
- **WORKING:** Zero-knowledge proofs for SDIS (attribute, non-revocation, compound)
- **WORKING:** Multi-device support with HSM + TPM 2.0 optional
- **WORKING:** Social key recovery via steward attestations
- **WORKING:** Age-encrypted keystore with auto-migration

### Verdict: CONFIRMED — Identity is the strongest subsystem

The implementation actually **exceeds** the original spec in some areas. SDIS (Sovereign Digital Identity System) with VUI provides stronger Sybil resistance than the spec's mana-burn approach alone. Post-quantum crypto was speculative in the spec; it's operational with feature flags.

**Minor gap:** The spec called for five DID types (Person, Organization, Device, Service, Ephemeral). Implementation has two (Traditional, SDIS Anchor). Organization/Device/Service DIDs exist implicitly through the entity model but aren't distinct DID types. This is a design simplification, not a deficit — the entity system handles the differentiation at a higher layer.

---

## 2. GOVERNANCE

### Origin
The entire governance architecture traces to three sources: Ella Baker's group-centered leadership model (from the Revolutionary Strategy Synthesis), Elinor Ostrom's eight design principles for commons governance, and Mondragon's nested polycentric structure (local autonomy + federated coordination). The DSL Design Research directly seeded the idea that governance rules should be machine-verifiable.

### Spec (Governance Protocol, Aug 2025)
- One member, one vote (never token-weighted)
- Seven voting models: SimpleMajority, SuperMajority, Consensus, Sociocracy, LiquidDemocracy, Futarchy, Random
- Three scope levels: Local, Federation, Global
- Soul-bound membership credentials
- Mana fee waivers for below-threshold members
- Proposal lifecycle: Draft → Open → Accepted/Rejected/NoQuorum
- Delegation system with liquid democracy
- All proposals/votes immutable in DAG

### Reality (March 2026)
- **WORKING:** Full proposal state machine (Draft → Open → Accepted/Rejected/NoQuorum)
- **WORKING:** Proposal types: Text, Budget, Membership, ConfigChange
- **WORKING:** Vote types: For/Against/Abstain with comments + discussion
- **WORKING:** Quorum + approval thresholds, configurable per-domain
- **WORKING:** Liquid democracy delegation (scope + expiry aware)
- **WORKING:** Charter system (TOML format per coop/community)
- **WORKING:** Amendment system for constitutional changes
- **WORKING:** Appeal system for due process
- **WORKING:** Cryptographic decision proofs (Ed25519 hashes + voter attestations)
- **WORKING:** Capability-based authorization (Meaning Firewall compliant)
- **WORKING:** Full REST API (20+ endpoints)
- **547 tests passing** in governance module

### Verdict: CONFIRMED with minor scope reduction

The spec promised seven voting models. The implementation has the core ones (majority, supermajority, consent, delegation) but **Futarchy, Sociocracy circles, and Random (sortition) are not yet implemented**. This is reasonable prioritization — you built the ones cooperatives actually use first.

**Active gap:** Charter templates (pre-built TOML configs for worker-coop, consumer-coop, community-land, federation, housing) don't exist yet. Estimated 1-2 days to create. This is a Phase 2 blocker for demo simplicity.

**Structural gap:** CCL formula extraction (#1308) — the commons credit formula is hardcoded in Rust rather than expressed in CCL. This violates the Meaning Firewall principle (kernel contains semantic business logic). Flagged as Phase 1 compliance work, not yet started.

---

## 3. ECONOMICS & LEDGER

### Origin
Three theoretical lineages converge:
1. **Mondragon's Caja Laboral** — cooperative bank as active incubator, not passive lender
2. **Sardex (Sardinia mutual credit)** — proof that bilateral credit systems work at regional scale
3. **Kropotkin's Mutual Aid** — evolutionary basis for inter-cooperative economics

The Dual Power Strategy (Sep 2024) specified three financial layers: Cooperative Banking, Mutual Credit, and Inter-Cooperative Risk Pool.

**NOTE (March 17, 2026):** The "Mana" economic model — regenerative computational credits with decay, demurrage, trust multipliers, and the regeneration formula — is **not part of the current iteration**. It was an earlier theoretical design that has been set aside. The current economic layer is the mutual credit ledger, patronage distribution, and budget management. This is a deliberate scoping decision, not a gap.

### Spec (Current Scope)
- Double-entry mutual credit (bilateral, not blockchain)
- Multi-currency support
- Credit limits, settlement, patronage distribution
- Treasury management and budgeting
- Escrow for governance-approved allocations

### Reality (March 2026)
- **WORKING:** Double-entry mutual credit ledger (Merkle-DAG, append-only, content-addressed)
- **WORKING:** Multi-currency support (Hours, USD, kWh, custom units)
- **WORKING:** Credit limits (per-participant, per-currency)
- **WORKING:** Settlement operations (cross-cooperative transfers)
- **WORKING:** Patronage distribution (S14 complete, internal capital accounts)
- **WORKING:** Treasury anchor (deterministic DID derivation)
- **WORKING:** Budget management + escrow system
- **WORKING:** Full REST API for ledger operations
- **WORKING:** Python Mesa simulation validating mutual credit parameters

### Verdict: CONFIRMED — the economic layer does what it needs to for this iteration

The double-entry mutual credit ledger, patronage distribution, multi-currency support, and settlement all work. This is a solid, practical cooperative economics layer that handles real cooperative needs (budgets, patronage, cross-coop settlement) without overcommitting to untested theoretical models.

**Remaining work:**
- Commons credit formula extraction from Rust to CCL (#1308) — still a Meaning Firewall compliance issue regardless of Mana's status
- If/when a more sophisticated economic model is desired in the future, the ledger infrastructure can support it

---

## 4. FEDERATION & INTEROPERATION

### Origin
Directly from Ostrom's design principle #8 ("nested enterprises for larger systems") and the Mondragon model (individual co-ops → Corporación → Social Council). The Cooperative DSL research framed federation as the mechanism for "cooperation among cooperatives" (Rochdale Principle #6). The Federation Sync Protocol was one of seven protocols written in the August 2025 sprint.

### Spec (Federation Sync Protocol, Aug 2025)
- Local-first operation with periodic checkpoints
- Semi-autonomous federation clusters with own DAG segments
- Byzantine fault tolerance (up to 1/3 faulty federations)
- Eventual consistency across boundaries
- Automatic partition healing
- Cross-federation resource sharing
- Three-epoch checkpoint system
- Four connection types: Peer, ParentChild, Bridge, Emergency
- Formation requires: 2+ founding orgs, 3+ validators, mana stake, charter approval

### Reality (March 2026)
- **WORKING:** Federation registry, attestations, clearing, vouching
- **WORKING:** Gossip-based registry discovery
- **WORKING:** Federated attestations (cross-boundary trust scores)
- **WORKING:** Cross-coop settlement
- **WORKING:** DID resolution across federation (`did:icn:coop-prefix:pubkey`)
- **WORKING:** Vouching (cooperative-to-coop endorsements)
- **WORKING:** Demo federation (4 personas: BrightWorks, River City Tool Library, Harbor Homes, Finger Lakes CDN)
- **WORKING:** Federation joining/leaving proposal types
- **BLOCKED:** `treasury:write` and `federation:write` scopes missing from K3s ALLOWED_SCOPES

### Verdict: PARTIALLY CONFIRMED — federation exists but async proof exchange undefined

The basic federation model works. Four demo cooperatives coordinate, vouch for each other, and settle across boundaries. But the spec's more sophisticated features are absent:

**Gaps:**

| Feature | Spec | Status | Severity |
|---------|------|--------|----------|
| Three-epoch checkpoint system | Periodic federation checkpoints | NOT BUILT | MEDIUM |
| Partition healing | Automatic reconciliation after split | NOT BUILT | MEDIUM |
| Byzantine tolerance (1/3) | Formal BFT across federations | NOT IMPLEMENTED | HIGH for production |
| Cross-federation async proof exchange | Cryptographic proofs between separate federations | NOT SPECIFIED (mechanism undefined) | HIGH |
| Market maker for exchange rates | Algorithmic or governed rate-setting | NOT SPECIFIED | HIGH for real inter-federation trade |
| Netting algorithm | Automatic offsetting of bilateral obligations | NOT BUILT | MEDIUM |
| Inter-org dispute resolution | Arbitration for settlement failures | NOT BUILT | HIGH |

The ecosystem docs flag this clearly: "Settlement finality is undefined. How are inter-org disputes resolved when organizations disagree on exchange rates or outstanding balances?" This is a gap between the theoretical model (which assumes cooperative good faith) and the adversarial-by-default security posture (which assumes bad actors).

---

## 5. CCL (Cooperative Contract Language)

### Origin
The DSL Design Research (March 2024) is the direct ancestor. It analyzed how domain-specific languages could replace natural language bylaws — the tension between human-readability and machine-verifiability. This is arguably the most intellectually distinctive component of ICN.

### Spec (CCL Protocol, Aug 2025)
- WASM-based smart contracts with Rust-like syntax
- Deterministic execution (no external I/O during execution)
- Strict resource metering via mana
- Built-in governance primitives (voting, proposals)
- Mutual aid automation (resource sharing, collective agreements)
- Organization/federation scoped contracts
- Type system: Bool, U64, I64, F64, String, Bytes + ICN types (DID, CID, Mana, Epoch, TokenClass)
- Built-in operations: identity, economic, governance, time, DAG
- Constructor, public, automatic, and view functions
- Complete auditability (all executions in DAG)

### Reality (March 2026)
- **WORKING:** CCL interpreter with fuel metering
- **WORKING:** Deterministic execution
- **WORKING:** Capability-based security
- **WORKING:** WASM execution via wasmtime (optional feature flag)
- **WORKING:** Fuzz testing target for CCL
- **PARTIAL:** CCL is "not Turing-complete" (intentional design choice for safety)
- **GAP:** Commons credit formula hardcoded, not in CCL (#1308)
- **GAP:** No pre-built contract library for common cooperative patterns
- **GAP:** No charter-to-CCL translation pipeline

### Verdict: WORKING but immature — the language exists, the ecosystem doesn't

The CCL interpreter runs, metered, deterministic, and fuzz-tested. That's real. But the spec promised rich built-in operations (identity, economic, governance, time, DAG), a type system with ICN-specific types, and contract scoping by organization. The current implementation appears to be a capable runtime without the library of cooperative governance primitives that would make it accessible to non-programmers.

**Critical gap:** The original DSL research emphasized that non-technical members must be able to understand the rules governing their cooperative. If CCL requires programming expertise to write governance rules, it has failed its core design requirement. The charter template system (TOML configs) may serve as the "non-programmer interface" with CCL underneath, but that pipeline doesn't exist yet.

---

## 6. STORAGE (DAG)

### Origin
The choice of content-addressed DAG (not blockchain) traces to the founding architecture. "Boring tech where possible" from the PoC spec. Event sourcing was in the original March 2024 conversations. The DAG model comes from IPLD/IPFS lineage, adapted for cooperative governance.

### Spec (DAG Storage Protocol, Aug 2025)
- Immutable content-addressed DAG (CID-based)
- Local-first (no global consensus required)
- Three-tier storage: Hot (24h), Warm (2 epochs), Cold (forever)
- Archive cooperatives compensated with tokens
- Mana costs per storage operation (scaled by size)
- Eight block types: Genesis, Identity, Economic, Governance, Execution, Federation, Checkpoint, Emergency
- Four node classes: Mobile (<100MB), Community (<10GB), Federation (<100GB), Archive (unlimited)
- Causal ordering and topological sorting

### Reality (March 2026)
- **WORKING:** Merkle-DAG, append-only journal, content-addressed (in icn-ledger)
- **WORKING:** Tamper-evident, independently verifiable
- **WORKING:** Sled KV store for local persistence
- **WORKING:** NFS-backed persistent volumes on K3s

### Verdict: SIMPLIFIED — core works, tiered storage not implemented

The Merkle-DAG exists and works for the ledger. But the spec's sophisticated three-tier storage system (Hot/Warm/Cold), archive cooperative compensation model, mana-based storage pricing, and node class differentiation are not implemented. This is reasonable for the current phase — you need governance and economics working before you need a storage economy. But it represents significant future work.

---

## 7. COMPUTE & MESH JOBS

### Origin
The Mesh Job Protocol emerged from the idea that cooperative members should be able to contribute compute resources and be compensated — extending the mutual aid principle to infrastructure. The COVM (April 2025) was the breakthrough: governance as executable computation.

### Spec (Mesh Job Protocol, Aug 2025)
- Distributed compute marketplace with transparent bidding
- Four runtimes: WASM, Docker, Native, CCL
- Job categories: Rendering, ML, Scientific, DataProcessing, MapReduce, etc.
- Deterministic verification with multi-validator sampling
- Privacy-preserving execution (optional ZK proofs)
- Automatic mana payment on completion
- Slashing for failures

### Reality (March 2026)
- **WORKING:** Trust-gated distributed execution (icn-compute)
- **WORKING:** WASM execution via wasmtime
- **WORKING:** CCL contract execution with fuel metering
- **WORKING:** Gossip-based task distribution
- **PARTIAL:** Execution receipts (ExecutionReceiptGate merged, commons loop incomplete)
- **NOT BUILT:** Workload manifest schema (Phase 2 Track E)
- **NOT BUILT:** Deterministic vs. advisory classification
- **NOT BUILT:** Job marketplace with bidding
- **NOT BUILT:** Storage specification for compute tasks
- **NOT BUILT:** Cell operator boundary definition
- **NOT BUILT:** Commons compute governance (Mana formula + CCL)

### Verdict: FOUNDATION ONLY — runtime works, marketplace and economics don't

The compute substrate runs. WASM contracts execute deterministically with fuel metering. But the spec's vision of a cooperative compute marketplace — with transparent bidding, multi-validator verification, job categories, and automatic mana compensation — is almost entirely unbuilt. Six of seven Phase 2 Track E specs are written but zero are implemented.

This is the biggest gap between vision and reality by spec coverage, though it's appropriately deprioritized. You don't need a compute marketplace to demonstrate governance and economics. It's Phase 2+ work.

---

## 8. SECURITY & PRIVACY

### Origin
"Adversarial-by-default" is an architectural principle from the earliest specs. The Security Protocol's dual-layer model — adversarial at machine layer, cooperative at social layer — maps directly to the Meaning Firewall concept. The privacy work traces to the recognition that cooperative members in hostile political environments need protection.

### Spec (Security Protocol, Aug 2025)
- Comprehensive threat model: state actors, corporate, criminal, internal, governance capture
- Defense in depth (no single point of failure)
- End-to-end encryption and surveillance resistance
- Democratic security (no barriers to legitimate participation)
- Zero-trust architecture
- Three-layer security: Transport → Message → Application

### Reality (March 2026)
- **WORKING:** Transport security (QUIC/TLS with DID-TLS binding)
- **WORKING:** Message authentication (SignedEnvelope, Ed25519)
- **WORKING:** Replay protection (sequence tracking)
- **WORKING:** Encrypted topics (ChaCha20-Poly1305)
- **WORKING:** Bloom filter deduplication
- **WORKING:** Trust graph (3D: Social, Economic, Technical)
- **NOT BUILT:** Onion routing (Phase 20, Week 3-4)
- **NOT BUILT:** Traffic obfuscation (Phase 20, Week 5-6)
- **STUB:** Active attack detection (icn-security crate exists, minimal)
- **WORKING:** Meaning Firewall surface audit (kernel_surface.toml: 9 clean, 11 infected, 9 needs-review)

### Verdict: CONFIRMED for current phase — privacy features are correctly deprioritized

Transport, message, and application security all work. The three-layer model is implemented. The missing pieces (onion routing, traffic obfuscation) are correctly classified as future work — they matter for deployment in hostile political environments but not for the NY Coop Summit demo.

**Important note:** The Meaning Firewall audit (kernel_surface.toml) shows 11 "infected" crates — meaning 11 crates in the kernel contain semantic business logic that should be in the apps layer. This is the most architecturally significant security finding. Cleaning these infected crates is ongoing work that directly affects regulatory positioning.

---

## 9. NETWORK TRANSPORT

### Origin
QUIC was chosen early for its multiplexing, built-in encryption, and NAT-friendliness. The gossip layer follows the anti-entropy model (not blockchain consensus). mDNS for local discovery is "boring tech where possible."

### Spec
- QUIC/TLS with DID-TLS binding
- mDNS + DHT discovery
- NAT traversal (STUN + TURN)
- Topic-based gossip pub/sub
- Vector clocks for ordering
- `icn://DID@IP:PORT` bootstrap format

### Reality (March 2026)
- **WORKING:** QUIC/TLS (quinn + rustls)
- **WORKING:** mDNS + DHT discovery
- **WORKING:** Topic-based gossip with anti-entropy + Bloom filter dedup
- **WORKING:** SignedEnvelope + EncryptedEnvelope formats
- **WORKING:** DID bootstrap format
- **PARTIAL:** NAT traversal (configured, not fully tested for production)

### Verdict: CONFIRMED — network layer is solid

This is one of the most mature subsystems. The only production gap is NAT traversal hardening, which matters for federation across the public internet but not for homelab/K3s demos.

---

## 10. ENTITY MODEL

### Origin
The four-level hierarchy (Person → Cooperative → Community → Federation) maps directly to Ostrom's nested enterprises and Mondragon's organizational structure.

### Spec
- Recursive entity abstraction: `entity:icn:<type>:<id>`
- Entity types: Person, Cooperative, Community, Federation
- Composable membership (person can be in multiple orgs)
- Hierarchical naming service
- Deterministic treasury derivation from entity ID

### Reality (March 2026)
- **WORKING:** Full EntityId format with type + identifier
- **WORKING:** Cooperative actor (full lifecycle: Formation → Active → Dissolution)
- **WORKING:** Community entity (Geographic, Interest, Solidarity types)
- **WORKING:** Federation entity (inter-coop coordination)
- **WORKING:** Recursive composite membership
- **WORKING:** Naming service with bounded alias resolution
- **WORKING:** Deterministic treasury DID derivation

### Verdict: CONFIRMED — entity model fully implements the spec

No gaps identified. This subsystem does exactly what was designed.

---

## Summary: The Scorecard

| Subsystem | Origin → Spec | Spec → Code | Assessment |
|-----------|:---:|:---:|---|
| **Identity & Crypto** | ✅ | ✅ | **EXCEEDS SPEC** — SDIS is more sophisticated than original DID-only design |
| **Governance** | ✅ | ✅ (minor reduction) | **CONFIRMED** — core pipeline works; 3 of 7 voting models deferred; charter templates missing |
| **Economics (Ledger)** | ✅ | ✅ | **CONFIRMED** — mutual credit, patronage, multi-currency, settlement all working. Mana model deliberately descoped from current iteration |
| **Federation** | ✅ | ⚠️ | **PARTIALLY CONFIRMED** — basic federation works; checkpointing, BFT, partition healing, cross-federation settlement undefined |
| **CCL** | ✅ | ⚠️ | **WORKING BUT IMMATURE** — runtime exists; no cooperative pattern library; charter-to-CCL pipeline missing |
| **Storage (DAG)** | ✅ | ⬜ (simplified) | **SIMPLIFIED** — Merkle-DAG works for ledger; tiered storage economy not built |
| **Compute (Mesh)** | ✅ | ⬜ (foundation only) | **FOUNDATION ONLY** — runtime executes; marketplace, bidding, compensation not built |
| **Security** | ✅ | ✅ (phase-appropriate) | **CONFIRMED** — three-layer model works; privacy features correctly deferred |
| **Network** | ✅ | ✅ | **CONFIRMED** — QUIC, gossip, discovery all working; NAT needs hardening |
| **Entity Model** | ✅ | ✅ | **CONFIRMED** — fully implements spec with recursive membership |

---

## The Two Critical Gaps

### Gap 1: Meaning Firewall Enforcement (HIGH)

The Meaning Firewall is ICN's regulatory immunity strategy. If the kernel contains business logic (what a "vote" means, how patronage distributes), then ICN looks like a regulated financial system. If the kernel is purely mechanical, it's infrastructure (like TCP/IP).

The kernel_surface.toml audit shows **11 "infected" crates** — kernel code containing semantic business logic. The commons credit formula is hardcoded in Rust instead of expressed in CCL (#1308). This is the most architecturally urgent gap because it undermines the regulatory positioning that protects the entire project.

**To close:** Clean infected kernel crates. Extract commons credit formula to CCL. Verify constraint engine has zero semantic interpretation. This is ongoing Phase 1 compliance work.

### Gap 2: Federation Settlement Finality (HIGH for production)

The federation model works for demo purposes (four cooperatives coordinating). But the spec's promises of Byzantine fault tolerance, automatic partition healing, and cross-federation settlement are not implemented. More critically, the mechanism for settling disputes between federations — what happens when Org A and Org B disagree about a transaction — is undefined at both the spec and code level.

**What this means:** ICN can demo inter-cooperative coordination with good-faith actors. It cannot handle adversarial federation scenarios. Given the "adversarial-by-default" security posture, this is a contradiction that must be resolved before production federation.

**To close:** Define async proof format for cross-federation claims. Implement dispute resolution protocol. Build netting algorithm for bilateral obligations. Test with adversarial scenarios.

### Note: Mana is Descoped

The Mana regenerative economic model (computational credits with decay, demurrage, trust multipliers, and the regeneration formula) is **not part of the current iteration**. This was a deliberate scoping decision. The mutual credit ledger, patronage distribution, and budget management that are currently implemented cover the actual economic needs of cooperatives in the near term. Mana remains a theoretical design that can be revisited in a future iteration if the cooperative ecosystem demands it.

---

## What This Confirms

Despite the gaps, the analysis confirms that ICN's architecture is fundamentally sound:

1. **The spec-to-code pipeline works.** Every subsystem that's been prioritized (identity, governance, network, entity model) implements the spec faithfully. The team (you + AI agents) can execute specs.

2. **The prioritization is correct.** Identity → Governance → Ledger → Federation is the right build order. You can't have economics without governance, can't have governance without identity. The foundation is solid.

3. **The deferred work is appropriately deferred.** Compute marketplace, tiered storage economy, onion routing — these matter for production at scale but not for proving the core thesis at a summit demo.

4. **The theoretical grounding holds up.** Every subsystem traces cleanly to a philosophical or practical origin (Ostrom, Baker, Mondragon, etc.). The architecture isn't ad hoc — it implements a coherent theory of cooperative digital infrastructure.

5. **414,000 lines of Rust is not vaporware.** 38 crates, 2,287 tests, 547 governance tests, K3s deployed, four demo personas running. This is real infrastructure.

The question isn't "does the architecture work?" It does. The question is: **can the two critical gaps (Meaning Firewall cleanup, Federation settlement) be closed before the Sovereign Tech Fund application and the October summit?**

With Mana descoped from the current iteration, the economic layer is in better shape than the earlier analysis suggested — the mutual credit ledger, patronage, and settlement all work and cover real cooperative needs. The pressure shifts to Meaning Firewall compliance (#1308, infected kernel crates) and federation hardening.

The forward plan says Phase 1 compliance (including #1308 CCL extraction) targets March 20. Phase 2 targets April 15 for v1.0.0 tag. With one fewer critical gap, this timeline is more realistic.
