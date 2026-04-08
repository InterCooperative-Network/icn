---
title: ICN Overview — What It Is and How It Works
description: A complete introduction to the InterCooperative Network: architecture, primitives, cooperative economics, and deployment model.
Status: descriptive
Canonical: no
Last Reviewed: 2026-04-08
---

# ICN Overview — What It Is and How It Works

The InterCooperative Network (ICN) is a peer-to-peer substrate daemon for cooperatives, communities, and federations. It is not a blockchain. It is not a federation server. It is not a governance platform. It is a **constraint enforcement engine** — infrastructure that cooperatives can own and run themselves.

This document is a complete introduction. Read it once to understand what ICN actually is, what it is not, what works today, and what is still under construction.

---

## 1. What ICN Is

ICN is a daemon (`icnd`) that a cooperative runs on its own hardware. The daemon provides a small set of mechanical primitives — identity, state, authorization, compute, communication, time, coordination, naming — and an enforcement kernel that executes constraints without understanding their meaning.

Cooperatives adopt **charters** (written in a domain-specific language called CCL) that describe how the organization governs itself. Those charters compile into generic constraints. The kernel enforces the constraints. The kernel never interprets the charter — it only sees numbers, capabilities, and policy decisions.

The design goal is narrow and specific:

> **Make it possible for a democratic organization to prove its decisions happened, own its records, and coordinate with other organizations, without renting that capability from a platform.**

Everything else in the architecture exists to serve that goal.

---

## 2. The Problem It Solves

Democratic organizations today rent their institutional infrastructure:

- **Governance** runs on Loomio, Google Forms, or Slack threads.
- **Accounting** runs on QuickBooks or a spreadsheet.
- **Membership records** live in a Mailchimp list or a Notion page.
- **Inter-organizational coordination** runs on email threads.

Each of those is owned by a landlord. When the landlord changes terms, raises prices, or shuts off access, the cooperative's institutional memory goes with it. And in every case, there is no way to prove to someone outside the organization that a decision actually happened, by whom, under what rules, without that outside party trusting the organization's word for it.

Three capabilities do not currently exist as shared infrastructure:

1. **Proving a decision happened** — a cryptographic receipt chaining a proposal, its discussion, the votes cast, and the action taken, verifiable by anyone holding the receipt.
2. **Owning the records** — institutional data on hardware the organization controls, with no central database that can be shut off.
3. **Federating without a platform** — two organizations verifying each other's governance and settling obligations across an organizational boundary without a shared landlord.

ICN is the infrastructure layer that provides those three capabilities.

---

## 3. The Four Entity Types

ICN recognizes four kinds of participants. All of them interact with the same eight primitives. The distinction between them is semantic — it belongs to apps, not the kernel.

### Individual
A self-sovereign identity. Holds cryptographic keys in a local keystore. Can participate from a full daemon, a mobile client, or a web client.

### Cooperative
A group of individuals with shared economic activity — worker co-ops, consumer co-ops, housing co-ops, platform co-ops. Has members, bylaws, a governance process, a treasury, and a ledger. Economically productive.

### Community
A group of individuals with shared civic interests — community land trusts, mutual aid networks, neighborhood associations. Has members, a governance process, and a civic purpose. Not necessarily economically productive.

### Federation
A group of cooperatives and/or communities coordinating across organizational boundaries. Has member organizations, bilateral trust agreements, and clearing for cross-org settlement. Federations bridge organizations without owning them.

These types are **recursive**. A federation's members can be federations. A cooperative's members can include cooperatives. The kernel does not care — it sees entities holding capabilities, and each entity has its own independent capability graph.

---

## 4. The Eight Primitives

Every ICN node exposes eight mechanical primitives. Apps compose them into institution-grade features.

| Primitive | What it does |
|-----------|--------------|
| **Identity** | Create, sign, verify DIDs. Ed25519 keypairs. Age-encrypted keystores. Key rotation. Multi-device support. Optional post-quantum hybrid signatures. |
| **Authorization** | Object-capability model. Capabilities are scoped, delegatable, revocable. `PolicyOracle` trait is how apps plug domain authorization into kernel enforcement. |
| **State** | Append-only logs, blob storage, key-value store. All content-addressed. All tamper-evident. Merkle-DAG for ledgers. |
| **Compute** | Deterministic, sandboxed, fuel-metered execution. CCL interpreter for governance rules; WASM for general-purpose tasks. |
| **Communication** | QUIC/TLS sessions with DID-TLS binding. Topic-based gossip. Signed envelopes with replay protection. mDNS discovery. |
| **Time** | Vector clocks for causal ordering. No dependency on wall-clock synchronization. Logical leases and schedulers. |
| **Coordination** | Causal consistency across peers. CRDT-style merges. Deterministic conflict resolution. Local-first — no global consensus required. |
| **Naming** | Hierarchical name resolution (`/cell/...`, `/org/...`, `/fed/...`). DID resolution. Aliases and registrations. |

These eight primitives are deliberately **mechanical**. They do not know what a "vote" is, what a "credit limit" is, or what a "trust score" means. They enforce rate limits, signature checks, capability grants, and causal orderings. That is all.

---

## 5. The Meaning Firewall

This is the architectural insight that makes the rest of the system work.

The kernel enforces constraints **without understanding their semantic origin**. Applications translate domain meaning into generic constraints. The boundary between them is structural — kernel crates cannot import app crates, and CI enforces this.

```
COOPERATIVE CHARTER (human-authored):
  "Consent-based governance with 72-hour discussion
   and a 75% approval threshold."

                    ↓   (CCL → charter_to_constraints)

APP (CharterPolicyOracle, in apps/charter):
  understands what "consent-based" and "75%" mean
  converts meaning into typed values

                    ↓   (PolicyOracle::evaluate)

KERNEL (icn-core) receives:
  ConstraintSet {
    min_duration_secs: 259200,
    approval_threshold: 0.75,
    capability_required: "governance:vote"
  }

  The kernel enforces these values blindly.
  It never sees "consent-based" or "charter".
```

This separation has two consequences:

1. **The kernel stays small, auditable, and predictable.** Its behavior is determined by the constraint set it receives, not by domain rules embedded in its code.
2. **Apps stay pluggable.** A cooperative can run a different policy oracle (different voting rules, different economic model) without modifying the kernel. The kernel does not need to know which oracle is loaded.

The Meaning Firewall is the reason ICN is infrastructure rather than a product. If the kernel understood meaning, each kind of meaning would become a feature to govern, regulate, and version. Because the kernel is mechanical, applications carry their own semantics and their own regulatory obligations. The substrate itself is neutral.

---

## 6. The Receipt Chain

Every governance decision in ICN produces a cryptographic receipt chain:

```
Proposal (signed by proposer)
  → Discussion comments (signed by participants)
    → Vote attestations (each signed by its voter)
      → Decision Receipt (tally + quorum check + outcome + prior hash)
        → Allocation Receipt (what resources the decision authorized)
          → Execution Receipt (what action was actually taken)
```

Each link is:

- **Content-addressed** — the hash is the identifier.
- **Signed** — a DID signature proves origin.
- **Linked** — each receipt references its predecessor's hash.
- **Deterministic** — the same inputs produce the same receipt on every node.

Anyone holding a receipt can verify the entire chain without contacting the producing organization, without trusting a platform, and without access to the original network. The receipt is self-contained proof.

This is the core value the rest of the architecture exists to deliver. A cooperative can show a funder, a regulator, a legal counterparty, or its own members exactly what was decided, by whom, under what rules, and what followed from it — without trust in anyone's word.

---

## 7. Mutual Credit and Terminology Discipline

ICN's economic model is **mutual credit**: a double-entry system where units are created by reciprocal obligation and net to zero across the cooperative. It is not a token. It is not a currency in the monetary sense. It is a record of who owes what to whom, reconciled continuously by the ledger.

Terminology matters here, and the project is strict about it:

| Avoid | Use instead |
|-------|-------------|
| payment | **settlement** |
| balance | **position** |
| currency | **unit** |
| wallet | **identity** (or keystore) |

These are not cosmetic choices. A "payment system" is regulated as a money transmitter. A "settlement system" reconciling mutual obligations between cooperative members operates under a different regulatory frame. Similarly, a "balance" implies a stored claim; a "position" is a computed view over ledger entries. The words shape the regulatory surface.

The ledger enforces structural invariants:

1. **User-signed transitions only** — nothing moves without a signature from the affected party.
2. **No hosted positions** — the operator never holds units on behalf of users.
3. **No operator routing of value** — transfers go directly from debtor to creditor ledger entries, not through an operator account.
4. **Derived views are not protocol primitives** — a position is a sum over entries, not stored state.
5. **No embedded convertibility** — one cooperative's units are not automatically fungible with another's. Cross-cooperative settlement requires explicit treaty and governance approval.

These invariants are checked on every PR that touches the ledger.

---

## 8. Deployment Model

ICN is a daemon you run. That is the whole deployment model.

```bash
cd icn && cargo build --release

# Node alpha (terminal 1)
./target/release/icnd --config ../config/icn-alpha.toml

# Node beta (terminal 2)
./target/release/icnd --config ../config/icn-beta.toml

# Status (terminal 3)
./target/release/icnctl network peers
```

Peers discover each other locally via mDNS. Over the wider network, peers connect via rendezvous or direct address. All transport is QUIC with TLS 1.3 and DID-binding (the TLS certificate's subject is the peer's DID). Signed envelopes protect against replay; anti-entropy keeps state eventually consistent across partitions.

There is no central server. There is no hosted instance. There is no company operating the network. The cooperative that runs the daemon owns the daemon, the keys, the ledger, and the governance record.

A reference K3s deployment (three nodes, running multiple simulated cooperative pods) exists inside the monorepo at `deploy/k8s/`. It is used for continuous integration and demo validation. It is not — currently — a hosted instance anyone can sign up for. To use ICN today, you build from source.

---

## 9. Current State

This section is honest about what is real, what is partial, and what is not yet ready. Do not infer maturity uniformly — different parts of the system are at different stages.

### Strong now

- **Identity** — DIDs, Ed25519 keypairs, Age-encrypted keystore, key rotation, DID-TLS binding, optional post-quantum hybrid (ML-DSA). Well-tested, production-shaped.
- **Networking** — QUIC/TLS sessions, mDNS discovery, signed envelopes, replay protection, end-to-end encryption on sensitive topics. Multi-node convergence works in tests and on the K3s cluster.
- **Ledger** — Double-entry mutual credit, Merkle-DAG anchoring, treasury, patronage tracking, settlement, disputes, dynamic credit limits. `SurplusPolicyView` recently closed the charter-driven surplus distribution seam; `EconomicPolicyView` does the same for credit limits.
- **Governance** — Proposals, votes, amendments, appeals, charter management, delegation, vote tally, effect manifests. Governance thresholds now read from ratified charters via the policy oracle.
- **Gateway** — REST and WebSocket API with real handler logic, JWT authentication, rate limiting. 70+ endpoints.

### Partial

- **CCL charter enforcement** — Charters parse and compile. Three custom constraint keys are now consumed at real decision boundaries (`governance thresholds`, `credit_limit`, `surplus_reserves_pct`). Two remain orphaned (`equity_range_*`, `settlement_*`). Charter view bindings are in-memory and lost on daemon restart until the next charter re-ratification event.
- **Kernel/app separation ratchet** — Most kernel crates are clean. A few still import domain types; CI tracks the violation count and the number only goes down. The meaning-firewall ratchet is an explicit test.
- **Federation end-to-end** — Attestations, clearing, netting, registry, and router all exist. The AgreementSchema lifecycle that stitches them together is not yet wired through in full.
- **Trust graph** — Scoped per-org. Transitive trust computation works. Rate-limit integration is live. More sophisticated multi-dimensional trust (economic, technical, social axes) is designed but not fully implemented.
- **Mobile SDK** — React Native package exists. Key flows (identity, vote, view ledger) are partially wired. Not production-ready.

### Not ready

- **No hosted instance.** You build from source. No sign-up, no onboarding flow for non-technical users yet.
- **No external security audit.** Internal review only. Do not use ICN to hold or reference anything whose loss would be consequential.
- **Scale testing is limited.** Convergence is tested at 2–10 nodes. Behavior at larger scales is not yet characterized.
- **Legal/regulatory review is ongoing.** The terminology discipline and the five ledger invariants are deliberate choices to stay compatible with cooperative law and money transmitter exemptions, but this has not been externally certified.

The project is deployed as a working reference implementation with a growing test suite. It is not a v1.0 release.

---

## 10. Getting Started

To run a node locally:

```bash
# Clone the repository
git clone https://github.com/InterCooperative-Network/icn
cd icn/icn

# Build (takes a few minutes on first build)
cargo build --release

# Initialize your identity (creates ~/.icn/keystore.age)
./target/release/icnctl id init

# Start a node
./target/release/icnd --config ../config/icn-alpha.toml

# In a second terminal: check status
./target/release/icnctl network peers
./target/release/icnctl id show
```

To contribute:

- Read [CONTRIBUTING.md](../CONTRIBUTING.md) for the Git workflow and PR conventions.
- Read [ARCHITECTURE.md](ARCHITECTURE.md) for the full system architecture.
- Read [architecture/KERNEL_APP_SEPARATION.md](architecture/KERNEL_APP_SEPARATION.md) for the Meaning Firewall in detail.
- Read [GETTING_STARTED.md](GETTING_STARTED.md) for the developer onboarding path.

---

## 11. Where to Go Next

Depending on what you are trying to understand:

- **"I want to see the architecture in detail."** → [ARCHITECTURE.md](ARCHITECTURE.md) and [architecture/KERNEL_APP_SEPARATION.md](architecture/KERNEL_APP_SEPARATION.md)
- **"I want to know what the API looks like."** → [reference/api/](reference/api/)
- **"I want to understand the CCL language."** → [architecture/ccl-architecture.md](architecture/ccl-architecture.md)
- **"I want to understand the economic model."** → [design/economics/](design/economics/)
- **"I want to understand the governance model."** → [design/governance/](design/governance/)
- **"I want the complete phase history."** → [PHASE_HISTORY.md](PHASE_HISTORY.md)
- **"I want to know what's actively being worked on."** → [STATE.md](STATE.md) and [TODO.md](TODO.md)
- **"I want the whitepaper-style pitch."** → [strategy/ICN-Pitch.md](strategy/ICN-Pitch.md)
- **"I want to understand the terminology."** → [glossary.md](glossary.md)

ICN is open source under AGPL v3. The code lives at [github.com/InterCooperative-Network/icn](https://github.com/InterCooperative-Network/icn). Issues, pull requests, and questions are welcome.

---

*This overview is a living document. If something here is wrong, stale, or overclaims, the correct response is to edit it and land the change — not to file around it.*
