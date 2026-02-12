# ICN Architecture Overview

**For**: Demo presentations and onboarding
**Complements**: `docs/ARCHITECTURE.md` (full technical details)

---

## The Big Picture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              USER LAYER                                  │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐    │
│  │   Web UI    │  │  Mobile App │  │    CLI      │  │   SDK       │    │
│  │ (pilot-ui)  │  │  (future)   │  │  (icnctl)   │  │ (TypeScript)│    │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘    │
│         │                │                │                │            │
│         └────────────────┴────────────────┴────────────────┘            │
│                                   │                                      │
│                                   ▼                                      │
├─────────────────────────────────────────────────────────────────────────┤
│                           GATEWAY (REST API)                             │
│                          http://localhost:8080                           │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐           │
│  │ /auth   │ │ /coops  │ │ /ledger │ │ /gov    │ │ /trust  │           │
│  └─────────┘ └─────────┘ └─────────┘ └─────────┘ └─────────┘           │
├─────────────────────────────────────────────────────────────────────────┤
│                             DAEMON (icnd)                                │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                         SUPERVISOR                               │   │
│  │   Manages actor lifecycle, routes messages, handles shutdown     │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│         │           │           │           │           │               │
│         ▼           ▼           ▼           ▼           ▼               │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐          │
│  │Identity │ │  Trust  │ │ Ledger  │ │Governance│ │ Network │          │
│  │  Actor  │ │  Actor  │ │  Actor  │ │  Actor  │ │  Actor  │          │
│  └─────────┘ └─────────┘ └─────────┘ └─────────┘ └─────────┘          │
│         │           │           │           │           │               │
│         └───────────┴───────────┴───────────┴───────────┘               │
│                                   │                                      │
│                                   ▼                                      │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │                         STORAGE (Sled)                           │   │
│  │   Persistent key-value store for identities, ledger, trust, etc. │   │
│  └─────────────────────────────────────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────────────────┤
│                            NETWORK LAYER                                 │
│  ┌─────────────────────────────────────────────────────────────────┐   │
│  │  QUIC/TLS Transport  │  Gossip Protocol  │  mDNS Discovery      │   │
│  └─────────────────────────────────────────────────────────────────┘   │
│                                   │                                      │
│                                   ▼                                      │
│                          ┌───────────────┐                               │
│                          │  Other Nodes  │                               │
│                          └───────────────┘                               │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Core Subsystems

### Identity

```
┌────────────────────────────────────────────┐
│              IDENTITY SYSTEM                │
├────────────────────────────────────────────┤
│  DID (Decentralized Identifier)            │
│  ┌──────────────────────────────────────┐  │
│  │  did:icn:zBFnhJhgvRjgukhQmkq9ddBz... │  │
│  │         ↑                             │  │
│  │  Base58-encoded Ed25519 public key    │  │
│  └──────────────────────────────────────┘  │
│                                            │
│  Keystore (encrypted)                      │
│  ┌──────────────────────────────────────┐  │
│  │  Private key + certificates           │  │
│  │  Protected by passphrase              │  │
│  │  Location: ~/.icn/keystore.age        │  │
│  └──────────────────────────────────────┘  │
│                                            │
│  Capabilities:                             │
│  • Self-sovereign (user controls keys)     │
│  • Multi-device support                    │
│  • Social recovery                         │
└────────────────────────────────────────────┘
```

### Governance Flow

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│   Member    │    │  Proposal   │    │   Voting    │    │  Execution  │
│  Submits    │───►│  Created    │───►│   Period    │───►│  (if pass)  │
└─────────────┘    └─────────────┘    └─────────────┘    └─────────────┘
                          │                  │                  │
                          ▼                  ▼                  ▼
                   ┌─────────────┐    ┌─────────────┐    ┌─────────────┐
                   │  Recorded   │    │  Votes      │    │  Effects    │
                   │  on ledger  │    │  tallied    │    │  applied    │
                   └─────────────┘    └─────────────┘    └─────────────┘

Proposal Types:
• Text        - Policy statements, bylaws
• Budget      - Treasury allocations
• Membership  - Add/remove members
• Config      - System parameters
```

### Ledger & Economic Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                       MUTUAL CREDIT LEDGER                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Transaction Flow:                                               │
│  ┌─────────┐         ┌─────────┐         ┌─────────┐           │
│  │  Alice  │────────►│ Ledger  │────────►│   Bob   │           │
│  │  -2 hrs │  signs  │ records │  syncs  │  +2 hrs │           │
│  └─────────┘         └─────────┘         └─────────┘           │
│                            │                                     │
│                            ▼                                     │
│                     ┌─────────────┐                              │
│                     │ Merkle-DAG  │ ← Tamper-evident chain       │
│                     │   Journal   │                              │
│                     └─────────────┘                              │
│                                                                  │
│  Economic Receipt Chain:                                         │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐                  │
│  │ Receipt₁ │───►│ Receipt₂ │───►│ Receipt₃ │───► ...          │
│  │ hash: a1 │    │ hash: b2 │    │ hash: c3 │                  │
│  └──────────┘    └──────────┘    └──────────┘                  │
│       │               │               │                         │
│       └───────────────┴───────────────┘                         │
│                       │                                          │
│              Any node can verify                                 │
│              the entire chain                                    │
└─────────────────────────────────────────────────────────────────┘
```

### Trust Graph

```
┌─────────────────────────────────────────────────────────────────┐
│                         TRUST GRAPH                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│        Alice ──0.9──► Bob                                        │
│          │              │                                        │
│         0.8            0.7                                       │
│          │              │                                        │
│          ▼              ▼                                        │
│        Carol ◄──0.6── David                                      │
│                                                                  │
│  Trust Computation:                                              │
│  • Direct: Alice trusts Bob at 0.9                              │
│  • Transitive: Alice → Bob → David = 0.9 × 0.7 = 0.63           │
│  • Bounded: 2-hop maximum                                        │
│                                                                  │
│  Used For:                                                       │
│  • Rate limiting (higher trust = more requests)                  │
│  • Access control (minimum trust for actions)                    │
│  • Resource allocation                                           │
└─────────────────────────────────────────────────────────────────┘
```

---

## Multi-Node / Federation

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           FEDERATION                                     │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│   ┌─────────────────┐         ┌─────────────────┐                       │
│   │  Tool Library   │◄───────►│   Food Co-op    │                       │
│   │    (Node A)     │  gossip │    (Node B)     │                       │
│   │                 │         │                 │                       │
│   │  - Members      │         │  - Members      │                       │
│   │  - Tools        │         │  - Products     │                       │
│   │  - Timebank     │         │  - Credits      │                       │
│   └────────┬────────┘         └────────┬────────┘                       │
│            │                           │                                 │
│            │         ┌─────────────────┐                                │
│            └────────►│  Worker Coop    │◄────────┘                      │
│                      │    (Node C)     │                                │
│                      │                 │                                │
│                      │  - Members      │                                │
│                      │  - Services     │                                │
│                      │  - Revenue      │                                │
│                      └─────────────────┘                                │
│                                                                          │
│   Each node is sovereign:                                                │
│   • Controls own membership                                              │
│   • Sets own policies                                                    │
│   • Chooses federation partners                                          │
│                                                                          │
│   Federation enables:                                                    │
│   • Cross-coop transactions                                              │
│   • Shared identity verification                                         │
│   • Federated trust attestations                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Key Differentiators

### vs. Blockchain

| ICN | Blockchain |
|-----|------------|
| Local-first consensus | Global consensus |
| Cooperatives are sovereign | One chain rules all |
| No mining/gas fees | Expensive transactions |
| Social trust | Trustless (adversarial) |

### vs. SaaS

| ICN | Traditional SaaS |
|-----|------------------|
| User owns infrastructure | Vendor owns infrastructure |
| Data stays local | Data on vendor servers |
| Open source | Proprietary |
| No vendor lock-in | Switching costs |

---

## Crate Map (For Developers)

```
icn/crates/
├── icn-core        # Supervisor, runtime, lifecycle
├── icn-identity    # DIDs, keystore, signatures
├── icn-trust       # Trust graph, computation
├── icn-ledger      # Mutual credit, journal
├── icn-governance  # Proposals, voting
├── icn-net         # QUIC/TLS networking
├── icn-gossip      # Peer synchronization
├── icn-gateway     # REST/WebSocket API
├── icn-compute     # WASM execution
└── icn-store       # Persistent storage
```

---

## Further Reading

- **Full Architecture**: `docs/ARCHITECTURE.md`
- **Getting Started**: `docs/GETTING_STARTED.md`
- **API Reference**: `docs/api/`
- **Governance Design**: `docs/design/governance/`
