# ICN Systems Textbook

This manual is a comprehensive, explanatory guide to ICN: what it is, how the
systems work, and why they are designed the way they are. Coding and setup are
secondary and live in the appendix.

Use this manual as the narrative foundation. When you want code-level detail,
follow the "Explore the Code" sections to the corresponding modules.

## How to Use This Manual

- Read Parts 0-8 in order to build a mental model of ICN.
- Use the module links in each chapter to connect concepts to code.
- Treat this as a textbook: reread the system map and data flows frequently.

## Part 0: What is ICN and Why it Exists

ICN (InterCooperative Network) is a decentralized coordination substrate for
cooperative organizations. It focuses on trust-based coordination, cooperative
economics, and federation rather than global consensus.

### The Problem ICN Solves

Cooperatives need to coordinate across boundaries without a central authority,
without assuming every peer is adversarial, and without the overhead of global
consensus. ICN provides:

- A shared identity system anchored in cryptographic proof.
- A trust model that mirrors real relationships.
- A coordination protocol that tolerates offline operation.
- A mutual credit ledger for exchange without external money supply.
- A governance layer for human decision-making.

### Core Design Principles (How and Why)

- **Local-first**: Nodes operate independently and reconcile via gossip.
  This keeps the system resilient to partitions and offline work.
- **Trust-native**: Access and coordination are gated by trust scores.
  This reflects cooperative social dynamics.
- **Deterministic compute**: The same inputs yield the same outputs.
  This allows verification without global consensus.
- **Capability-based security**: Actions require explicit permissions.
  This limits blast radius and supports auditing.
- **Human-governed**: Policy changes are decided by people.
  This aligns with cooperative governance norms.

### What ICN is Not

- Not a blockchain: there is no global consensus or gas fee model.
- Not a federation server: ICN is a P2P coordination layer.
- Not a single app: it is a substrate for many applications.

## Part 1: System Map and Architecture Overview

ICN is composed of several subsystems that collaborate through the actor model.
Each subsystem has a clear responsibility and explicit interfaces.

### System Map (Conceptual)

```
Transport (icn-net)
  -> Identity (icn-identity)
    -> Trust (icn-trust)
      -> Gossip (icn-gossip)
        -> Ledger (icn-ledger)
          -> Contracts (icn-ccl)
            -> Governance (icn-governance)
          -> Gateway (icn-gateway) -> SDK/UI
Storage (icn-store) and Observability (icn-obs) support all layers
Security (icn-security) and Privacy (icn-privacy) wrap critical paths
Federation (icn-federation) coordinates across cooperatives
```

### Why a Layered Architecture?

Layering isolates concerns and keeps reasoning local:

- Transport focuses on secure connectivity.
- Identity and trust establish who you are and how you are treated.
- Gossip provides eventual consistency without centralized consensus.
- Ledger and contracts encode economic agreements.
- Governance formalizes human decision-making.
- Gateway and UI provide application-facing access.

### Key Data Flows

1. **Incoming Message Flow**: Network -> Gossip -> Ledger -> Storage
2. **Outgoing Transaction Flow**: Gateway -> Ledger -> Gossip -> Network
3. **Governance Flow**: Proposal -> Vote -> Policy -> Enforcement

### Explore the Code

- `docs/onboarding/modules/module-02-architecture-overview.md`
- `docs/ARCHITECTURE.md`
- `docs/architecture/ARCHITECTURE_MAP.md`

## Part 2: Identity and Trust

Identity and trust turn anonymous peers into accountable collaborators.

### Identity (icn-identity)

**How it works:**
- DIDs are derived from Ed25519 public keys: `did:icn:<base58>`.
- Keys are stored in an age-encrypted keystore.
- Rotation creates a signed chain of custody for identity continuity.

**Why it exists:**
- Self-certifying identifiers eliminate centralized registries.
- Key rotation preserves security without losing reputation.

### Trust (icn-trust)

**How it works:**
- Trust edges form a weighted graph between DIDs.
- Trust propagates transitively with decay.
- Trust classes gate rate limits and access policies.

**Why it exists:**
- Cooperatives require graduated access, not binary trust.
- Trust-native behavior enables social accountability.

### Key Interactions

- Identity anchors the trust graph.
- Trust scores drive rate limiting and access control.

### Explore the Code

- `docs/onboarding/modules/module-04-identity-trust.md`
- `icn/crates/icn-identity/`
- `icn/crates/icn-trust/`

## Part 3: Networking and Gossip

ICN favors eventual consistency with strong identity and trust guarantees.

### Transport (icn-net)

**How it works:**
- QUIC/TLS for secure streams and connection migration.
- mDNS for peer discovery on local networks.
- Message envelopes include DIDs and payload types.

**Why it exists:**
- QUIC provides multiplexing and built-in encryption.
- Local discovery removes centralized dependency.

### Gossip (icn-gossip)

**How it works:**
- Topics partition data by purpose.
- Vector clocks track causality.
- Anti-entropy repairs partitions and missing data.

**Why it exists:**
- Gossip scales without global consensus.
- Vector clocks provide causal ordering without locks.

### Key Interactions

- Network authenticates peers; gossip synchronizes state.
- Trust gates topic access and message rate limits.

### Explore the Code

- `docs/onboarding/modules/module-05-network-gossip.md`
- `icn/crates/icn-net/`
- `icn/crates/icn-gossip/`

## Part 4: Ledger and Contracts

The ledger encodes cooperative economics; contracts encode cooperative rules.

### Ledger (icn-ledger)

**How it works:**
- Mutual credit creates balances via transactions.
- Double-entry entries ensure conservation of value.
- Merkle-DAG structure supports integrity and sync.

**Why it exists:**
- Mutual credit enables exchange without external currency.
- Merkle-DAG enables auditability without total ordering.

### Contracts (icn-ccl)

**How it works:**
- Deterministic contract execution with fuel limits.
- Capability-based permissions define allowed actions.

**Why it exists:**
- Cooperative agreements must be explicit and auditable.
- Capabilities prevent unbounded behavior in contracts.

### Explore the Code

- `docs/onboarding/modules/module-06-ledger-contracts.md`
- `icn/crates/icn-ledger/`
- `icn/crates/icn-ccl/`

## Part 5: Governance

Governance is how communities change rules without breaking trust.

### Governance (icn-governance)

**How it works:**
- Proposals define policy changes.
- Votes determine acceptance.
- Policies become enforceable configuration and contract rules.

**Why it exists:**
- Human governance aligns with cooperative values.
- Transparent decision records enable accountability.

### Key Interactions

- Governance events are gossiped and stored.
- Contract capabilities and policy settings enforce decisions.

### Explore the Code

- `docs/onboarding/modules/module-14-governance-ccl-deep-dive.md`
- `icn/crates/icn-governance/`
- `docs/governance-primitives.md`

## Part 6: Gateway, SDK, and Web UI

These layers provide application access and user interfaces.

### Gateway (icn-gateway)

**How it works:**
- REST + WebSocket APIs expose core functionality.
- Auth and scopes gate access to sensitive operations.

**Why it exists:**
- Apps need stable interfaces, not internal actor wiring.

### SDK and UI

**How it works:**
- SDKs wrap gateway APIs with typed interfaces.
- Pilot UI demonstrates cooperative workflows.

**Why it exists:**
- Developer ergonomics and consistent client behavior.

### Explore the Code

- `docs/onboarding/modules/module-07-gateway-sdk.md`
- `docs/onboarding/modules/module-08-web-ui.md`
- `icn/crates/icn-gateway/`
- `sdk/typescript/`
- `web/pilot-ui/`

## Part 7: Observability and Security

ICN must be observable and secure under real-world conditions.

### Observability (icn-obs)

**How it works:**
- Metrics are defined per subsystem.
- Tracing and logging expose internal behavior.
- Prometheus and Grafana visualize health.

**Why it exists:**
- Distributed systems require visibility to debug and operate safely.

### Security and Privacy (icn-security, icn-privacy)

**How it works:**
- Transport security: QUIC/TLS with DID binding.
- Message security: Signed envelopes + replay guards.
- Application security: Encrypted payloads when needed.

**Why it exists:**
- Trust-native does not mean trust-blind.
- Security layers ensure integrity, authenticity, and confidentiality.

### Explore the Code

- `docs/onboarding/modules/module-12-observability.md`
- `docs/onboarding/modules/module-13-security-privacy.md`
- `icn/crates/icn-obs/`
- `icn/crates/icn-security/`
- `icn/crates/icn-privacy/`
- `monitoring/`

## Part 8: Operations and Federation

ICN is designed to run in production and across cooperative boundaries.

### Operations

**How it works:**
- Configuration defines runtime behavior and limits.
- Deployment uses containerized workflows.
- Monitoring and alerting protect reliability.

**Why it exists:**
- Cooperative infrastructure must be reliable and auditable.

### Federation

**How it works:**
- Agreements formalize inter-cooperative relationships.
- Clearing and settlement reduce transaction volume.

**Why it exists:**
- Coops coordinate without merging governance structures.

### Explore the Code

- `docs/onboarding/modules/module-09-ops-deploy.md`
- `docs/onboarding/modules/module-11-federation.md`
- `icn/crates/icn-federation/`
- `deploy/`

## Appendix: Developer Prerequisites

If you are new to the codebase or Rust, use these modules as references:

- `docs/onboarding/modules/module-00-setup.md`
- `docs/onboarding/modules/module-01-rust-fundamentals.md`

## Glossary (Abbreviated)

- **DID**: Decentralized Identifier anchored in a public key.
- **Gossip**: Peer-to-peer synchronization protocol.
- **Mutual Credit**: Ledger model where transactions create balances.
- **CCL**: Cooperative Contract Language.
- **Capability**: Explicit permission for a contract action.
- **Supervisor**: Actor orchestrator that wires subsystems together.
