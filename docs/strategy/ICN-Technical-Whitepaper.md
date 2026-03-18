# ICN Technical Whitepaper: Architecture of a P2P Constraint Engine for Democratic Organizations

**Version 0.1 | March 17, 2026**
**Author: Matt Faherty**
**Status: Working Draft**

---

## Abstract

ICN (InterCooperative Network) is a peer-to-peer daemon that provides verifiable decision infrastructure for democratic organizations. This paper describes the architectural design of ICN's constraint engine, the Meaning Firewall pattern that separates mechanical enforcement from semantic interpretation, the cryptographic receipt chain that enables provable governance, and the threat model that governs the system's security posture. We compare ICN to related systems (Holochain, Secure Scuttlebutt, ActivityPub, blockchain-based DAOs) and articulate the specific design trade-offs that differentiate ICN from each.

---

## 1. Design Motivation

Democratic organizations — cooperatives, community land trusts, mutual aid networks, civic federations — share a structural problem: they make collective decisions, but they cannot produce independently verifiable proof that those decisions occurred.

Existing tools (Loomio, Google Workspace, custom web applications) record decisions but do not prove them. The distinction is critical. A record is a document someone produced. A proof is a cryptographic artifact that anyone can verify without trusting the producer. The difference between "someone wrote it down" and "the math confirms it" is the difference between trust-dependent and trust-independent institutions.

ICN provides the infrastructure layer that makes provable collective decisions possible. It does not interpret what decisions mean. It enforces the constraints that make decisions verifiable.

---

## 2. The Meaning Firewall

The Meaning Firewall is ICN's load-bearing architectural decision. It establishes a hard boundary between two layers:

**The Kernel** enforces constraints mechanically. It processes rate limits, capability checks, fuel budgets, quorum thresholds, and state transitions. It does not interpret the semantic content of any operation. The kernel does not know what a "vote" is, what a "cooperative" is, or what "governance" means. It sees entities, capabilities, signed state transitions, and numerical constraints.

**Applications** sit above the Meaning Firewall and assign semantic meaning. A governance application translates "consent-based decision with 72-hour discussion period and 75% approval threshold" into a `ConstraintSet` that the kernel enforces blindly. A mutual credit application translates "bilateral obligation between two cooperatives" into state transitions the kernel processes without understanding their economic meaning.

The interface between layers is the `PolicyOracle` trait:

```rust
pub trait PolicyOracle: Send + Sync {
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision;
}
```

Applications implement `PolicyOracle`. The kernel calls it. The kernel never inspects the internal logic of the oracle. It receives a `PolicyDecision` containing a `ConstraintSet` and enforces it.

**Enforcement mechanism.** The Meaning Firewall is not aspirational. It is structurally enforced through the Rust crate dependency graph. Kernel crates cannot import application crates. CI checks this invariant on every pull request. The `kernel_surface.toml` audit file tracks 29 crates and their classification (kernel, application, or support). As of March 2026, 9 crates are clean, 11 contain residual semantic leakage being extracted, and 9 require review.

**Regulatory significance.** If the kernel interprets meaning, ICN is classifiable as a product — a governance platform, a payment processor, a voting system — each a regulated category with specific legal obligations. If the kernel is mechanical infrastructure (analogous to TCP/IP, the Linux kernel, or an electrical grid), it functions as a public utility. Applications carry their own regulatory obligations. The infrastructure itself is neutral. This is not clever framing; it is an architectural property enforced by the dependency graph.

---

## 3. The Eight Kernel Primitives

ICN's kernel exposes eight primitives. All system behavior is composed from these:

**Identity.** Create, sign, verify, and resolve decentralized identifiers (DIDs). Format: `did:icn:<base58-ed25519-pubkey>`. Key management uses Age-encrypted keystores with automatic version migration (v1 → v2 → v2.1 adding TLS binding and X25519 key agreement).

**Authorization.** Issue and check unforgeable capability tokens following an object-capability security model. Capabilities are scoped (e.g., `governance:vote`, `federation:admin`), delegatable, and revocable. No ambient authority exists — every action requires an explicit capability grant.

**State.** Append-only logs, blob storage, and key-value storage. All entries are content-addressed (CID-based) and tamper-evident. State is the substrate for the receipt chain — every decision, allocation, and execution is a state entry linked by hash references.

**Compute.** Execute deterministic sandboxed programs with fuel metering. The compute substrate targets WASM with NaN canonicalization, seeded RNG, and canonical serialization. Programs that exhaust their fuel budget halt deterministically. Two nodes processing the same input produce bit-identical output.

**Communication.** Pub/sub, request/response, and streaming over QUIC/TLS with DID-based mutual authentication. Messages are signed envelopes with replay protection (sequence tracking per sender). Discovery uses mDNS (local network) and gossip (wide-area).

**Time.** Logical clocks, scheduling, and leases. ICN does not depend on wall-clock synchronization. Causal ordering is established through vector clocks. Scheduling uses logical time with configurable duration semantics.

**Coordination.** Consensus groups, CRDTs, and deterministic conflict resolution. Local-first by default — entities agree on their own state and resolve conflicts without global consensus. Cross-entity coordination uses bilateral or multilateral agreement protocols with signed settlement.

**Naming.** Hierarchical name resolution and service discovery. Entities and services are addressable by name (e.g., `nycn/ithaca-solar/governance`) with resolution backed by the identity and communication primitives.

---

## 4. The Receipt Chain

The receipt chain is ICN's primary value artifact. It provides cryptographic proof that a sequence of institutional actions occurred.

A complete receipt chain for a governance decision contains:

```
Proposal (content-addressed, signed by proposer)
  → Discussion (comment hashes, signed by participants)
    → Votes (individual vote attestations, signed by each voter)
      → Decision Receipt (tally + quorum check + outcome + hash chain)
        → Allocation Receipt (resource allocation authorized by decision)
          → Execution Receipt (action taken, linked to allocation)
```

Each link in the chain is:
1. Content-addressed (the hash of the entry is its identifier)
2. Signed (the producer's DID signature is attached)
3. Linked (references the hash of its predecessor)
4. Deterministic (any node processing the same inputs produces the same receipt)

**Verification is peer-to-peer.** Anyone with a receipt can verify it without contacting the producing organization, without trusting a platform, and without access to the original network. The receipt is self-contained proof.

**The ExecutionReceiptGate** (implemented in PR #1327) enforces Invariant 7: governance decisions must produce execution receipts that close the loop. An allocation cannot be executed without a Decision Receipt authorizing it. An execution cannot be recorded without linking to its authorization. This closes the audit chain mechanically.

---

## 5. Entity Model

ICN recognizes four entity types. All interact with the same eight primitives. The kernel is agnostic about what type an entity is — the distinction is app-level meaning.

**Person.** A self-sovereign identity holding keys in a local wallet. Participates via daemon, mobile client (React Native SDK), or web client (TypeScript SDK).

**Cooperative.** A group of people with shared economic activity. Represented as an entity ID, member DID set, state log, and capability graph.

**Community.** A group of people with shared civic interests. Kernel-identical to a cooperative. Different app-level meaning. Communities and cooperatives are co-equal.

**Federation.** A group of cooperatives and communities coordinating across organizational boundaries. Kernel-identical representation. Federations bridge organizations without owning them.

Membership is recursive. Cooperatives contain cooperatives. Federations contain federations. Communities contain cooperatives. The kernel sees entities containing entities, each with independent capability graphs.

---

## 6. Network Architecture

ICN daemons communicate over QUIC/TLS with DID-based mutual authentication (DID-TLS binding). The transport layer provides:

**Session establishment.** QUIC connection with TLS 1.3. Server and client authenticate by proving ownership of their respective DIDs via Ed25519 signature challenge.

**Message format.** `SignedEnvelope` containing payload, sender DID, Ed25519 signature, and `ReplayGuard` (per-sender sequence number). Payload types: Gossip, Rpc, Subscribe, Hello, Signed.

**Gossip protocol.** Topic-based pub/sub with push announcements, pull requests, and anti-entropy sweeps. Causal ordering via vector clocks. Deduplication via Bloom filters. This is the primary mechanism for state replication across organizational boundaries.

**Discovery.** mDNS for local network discovery. Gossip-based peer exchange for wide-area discovery. Federation registry (`/v1/federation/coops`) for organizational discovery.

**Trust-gated rate limiting.** The trust graph (web-of-participation trust scores from 0.0 to 1.0) gates communication rates per trust class: Isolated (<0.1): 10 msg/sec, Known (0.1-0.4): 20 msg/sec, Partner (0.4-0.7): 100 msg/sec, Federated (0.7+): 200 msg/sec.

---

## 7. Security Model

### 7.1 Threat Model

ICN assumes an adversarial-by-default posture. Every peer is untrusted until trust is established through verifiable participation. The system is designed to operate correctly in the presence of:

- **Byzantine nodes** that send malformed, replayed, or fabricated messages
- **Sybil attacks** where a single actor operates multiple identities to gain disproportionate influence
- **Network partitions** where groups of nodes cannot communicate for extended periods
- **Compromised keys** where an attacker gains access to a legitimate identity's signing key

### 7.2 Defense Layers

**Transport security.** QUIC/TLS 1.3 with DID-TLS binding. All communication is encrypted. Identity is verified at the transport layer before any application messages are exchanged.

**Message integrity.** Every message is a `SignedEnvelope`. Ed25519 signatures provide authentication and non-repudiation. Replay protection via per-sender sequence tracking prevents message reuse.

**Application-layer encryption.** `EncryptedEnvelope` provides end-to-end encryption for sensitive payloads. Only intended recipients can decrypt. The transport layer and relay nodes see ciphertext.

**Capability-based authorization.** Object-capability security model. No ambient authority. Every action requires an explicit, unforgeable capability token. Capabilities are scoped, delegatable, and revocable. Delegation is tracked and auditable.

**Sybil resistance.** MeritRank reputation system based on verifiable participation. Trust scores are computed from: honoring obligations, participating in governance, vouching accuracy, and uptime. Creating a fake identity is free. Making it useful is expensive — each identity must independently build reputation through sustained participation.

### 7.3 Invariants

Five non-negotiable properties enforced across the system:

1. **Adversarial-by-default.** No trust assumption anywhere in the protocol. Security comes from architecture, not faith.
2. **Determinism.** Same inputs produce same outputs. No floating point. Seeded RNG. Canonical serialization. Two nodes processing the same log arrive at identical state.
3. **Canonical encodings.** One valid byte representation per value. No ambiguity in wire format, proof format, or storage format.
4. **No panics in protocol paths.** All failures are handled. The daemon does not crash on malformed input. No `.unwrap()` or `.expect()` in network/protocol/actor code paths.
5. **Kernel/app boundary inviolable.** Enforced by dependency graph. Checked by CI. Structural, not aspirational.

---

## 8. Comparison to Related Systems

### 8.1 Holochain

Holochain and ICN share a philosophical commitment to agent-centric, local-first architecture without global consensus. Both use content-addressed data and signed entries.

**Key difference: the Meaning Firewall.** Holochain DNAs (distributed apps) contain their validation rules directly — the DNA is the application and the enforcement layer simultaneously. ICN separates these concerns. The kernel enforces constraints without semantic interpretation. Applications implement `PolicyOracle` to translate domain meaning into generic constraints. This separation gives ICN regulatory neutrality that Holochain DNAs do not inherently possess.

**Key difference: receipt chain.** Holochain provides tamper-evident records through DHT validation. ICN provides a complete governance receipt chain — proposal → discussion → votes → decision → allocation → execution — where each link is independently verifiable and the chain itself constitutes proof of democratic process. This specific chain structure is designed for institutional proof, not general-purpose data validation.

### 8.2 Secure Scuttlebutt (SSB)

SSB and ICN share the local-first, no-server architecture. Both use append-only logs and gossip for replication.

**Key difference: institutional vs. social.** SSB is designed for social relationships between individuals. ICN is designed for institutional relationships between organizations. The entity model (Person, Cooperative, Community, Federation), the receipt chain, and the trust-gated federation protocol have no SSB equivalent. SSB does not provide organizational governance primitives.

**Key difference: trust model.** SSB trust is social (follows and blocks). ICN trust is participatory (verifiable actions that build reputation over time with decay). SSB trust is binary. ICN trust is scored and class-gated.

### 8.3 ActivityPub / Fediverse

ActivityPub federates social content. ICN federates institutional decisions.

**Key difference: the receipt.** When a Mastodon post federates, nobody needs cryptographic proof that it was democratically authorized. When a cooperative's budget allocation federates to a partner cooperative, proof of authorization is the entire point. ActivityPub has no concept of governance receipts, quorum verification, or decision audit chains.

**Key difference: identity model.** ActivityPub identities are server-bound (@user@server). ICN identities are self-sovereign DIDs that survive server changes. An organization's identity in ICN is not tied to any infrastructure operator.

### 8.4 Blockchain-based DAOs

DAOs and ICN both aim to provide verifiable collective governance. Both produce cryptographic proofs of decisions.

**Key difference: consensus scope.** DAOs require global consensus — every node validates every transaction. ICN is local-first. A three-person cooperative validates its own decisions. Outsiders can verify the proof without participating in consensus. This eliminates gas fees, mining, chain congestion, and the absurd computational overhead of proving a small cooperative's vote to the entire planet.

**Key difference: the Meaning Firewall.** DAO smart contracts encode domain semantics directly — a governance contract knows what a "vote" is. ICN's kernel does not. This is not a limitation; it is a regulatory design decision. Smart contracts that encode financial semantics are financial instruments. ICN's kernel is infrastructure.

**Key difference: exit cost.** Leaving a DAO means leaving a chain. Leaving ICN means stopping your daemon. Your data is local. Your identity is portable. Your records are yours. Exit is free.

---

## 9. Current Implementation Status

As of March 2026:

- **Codebase:** 38 Rust workspace crates, ~414K LOC, 2,287 tests
- **Binaries:** `icnd` (daemon), `icnctl` (CLI), `icn-console` (TUI)
- **API:** 70+ REST endpoints via actix-web gateway, all with real handler logic, zero stubs
- **SDKs:** TypeScript (npm), React Native (mobile)
- **Deployment:** K3s cluster (3 nodes, 4 federated cooperative pods, Prometheus + Grafana monitoring)
- **Demo:** 4 working flows (governance, patronage, federation, reporting) with presenter mode
- **Completion:** ~75% toward first external deployment
- **Phase:** 0+1 complete, Phase 2 (demo-ready vertical slice) in progress

**Known technical debt:**
- 11 kernel crates with residual semantic leakage (Meaning Firewall cleanup in progress)
- Compliance epic #1302 at ~60% (terminology rename, CI linter, structural attestation remaining)
- Federation settlement finality not yet formally specified
- Post-quantum cryptography (`icn-crypto-pq` crate) not yet integrated into transport

---

## 10. Conclusion

ICN is not a governance platform, a ledger, a blockchain, or a payment system. It is a constraint engine that provides eight mechanical primitives, a cryptographic receipt chain, and a structural separation between enforcement and meaning. Organizations that govern themselves democratically can use ICN to prove their decisions, own their infrastructure, and coordinate with each other without trusting a platform.

The Meaning Firewall is the core architectural contribution. By ensuring that the kernel never interprets the semantic content of the operations it processes, ICN achieves regulatory neutrality, application flexibility, and the structural property that makes it infrastructure rather than a product.

The receipt chain is the core value proposition. By linking proposals, votes, decisions, allocations, and executions in a tamper-evident, independently verifiable chain, ICN provides something that no existing tool in the cooperative ecosystem provides: cryptographic proof that democratic governance happened.

---

## References

- ICN Source: github.com/InterCooperative-Network/icn
- Holochain: holochain.org
- Secure Scuttlebutt: scuttlebutt.nz
- ActivityPub: w3.org/TR/activitypub
- MeritRank: Research domain B (Sybil resistance)
- Object-Capability Model: Research domain F
- Elinor Ostrom, "Governing the Commons" (1990)
- NY Cooperative Corporations Law, Article 5-A, §82-§93
