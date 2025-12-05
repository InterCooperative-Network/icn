# ICN Glossary

**Status**: Reference Document
**Version**: 1.0.0
**Last Updated**: 2025-12-05

This glossary defines the authoritative terminology for ICN. All documentation, code, and user-facing text should use these terms consistently.

---

## Table of Contents

1. [Identity & Trust](#identity--trust)
2. [Organizations](#organizations)
3. [Economic System](#economic-system)
4. [Fuel System](#fuel-system)
5. [Infrastructure](#infrastructure)
6. [Governance](#governance)
7. [Technical Terms](#technical-terms)
8. [Disambiguation](#disambiguation)

---

## Identity & Trust

### DID (Decentralized Identifier)
A self-sovereign identifier in the format `did:icn:<base58-pubkey>`. Every participant, organization, and the network itself has a DID.

### Trust Score
A computed value (0.0 to 1.0) representing the network's confidence in an entity, derived from the trust graph using PageRank-like algorithms.

### Trust Graph
The network of trust relationships between DIDs. Edges are directed and labeled (e.g., "vouches for", "has transacted with").

### Trust Class
Categories of trust levels that determine rate limits and access:
- **Isolated** (< 0.1): Very limited access
- **Known** (0.1 - 0.4): Basic access
- **Partner** (0.4 - 0.7): Standard access
- **Federated** (0.7+): Full access

### Attestation
A signed statement by one DID about another. Used for contribution verification, trust building, and identity claims.

---

## Organizations

### Cooperative (Coop)
A formal economic organization with:
- Registered membership
- Bylaws and governance
- Treasury and ledger
- Economic activity (trade, production)

Cooperatives are the **economic engine** of ICN.

### Community
A civic organization for:
- Mutual aid
- Stewardship
- Advocacy
- Public service

Communities are **first-class entities** in ICN, not just informal groups. They have DIDs, fuel pools, and governance.

### Federation
A group of cooperatives and/or communities that:
- Share exchange agreements
- Have common governance
- Can trade using federation credits

### Household
A grouping of related DIDs (family, friends) that:
- Pool device contributions
- Share benefits
- Have shared resource accounting

### Global Commons
The network-wide pool for:
- Unaffiliated individual contributions
- Network infrastructure
- Public goods

Anyone can contribute to the Global Commons without joining a coop.

### Network
The ICN network as a whole. Has:
- Constitutional governance
- Network-wide parameters
- The Network Treasury DID

---

## Economic System

### Credit
A unit of value in ICN's mutual credit system. Credits are **internal accounting entries**, not tradeable tokens.

**Always qualify which type:**
- **Coop-Credits**: Spendable within a single cooperative
- **Federation-Credits**: Spendable across federated organizations
- **Infra-Credits**: Earned from infrastructure contribution (converts to coop-credits)

### Mutual Credit
An economic system where:
- Value comes from reciprocity, not scarcity
- Balances can be negative (you owe) or positive (you're owed)
- The system is zero-sum (total credits = total debits)
- No external tokens or currency backing required

### Hours
The default currency unit in most cooperatives. Represents an hour of labor or equivalent value.

**Always qualify which type:**
- **Labor-Hours**: Human work (tutoring, cooking, etc.)
- **CPU-Hours**: Compute contribution
- **Uptime-Hours**: Node availability

### Demurrage
Negative interest on idle credit balances. Encourages circulation, discourages hoarding.

### Provenance
The tracked history of a credit unit, including:
- Original contributor
- How it was earned
- Transfer history

Used to determine bridge eligibility.

### Bridge
The interface between ICN internal credits and external value (fiat, other networks). Governed and restricted.

### Three-Tier System
The graduated exchangeability of credits:
1. **Internal** (Tier 1): Within one coop only
2. **Federated** (Tier 2): Across federated orgs
3. **Bridge** (Tier 3): External exchange (governed)

### Network Treasury
The DID `did:icn:network:infrastructure` that issues credits for infrastructure contributions.

### Exchange Pool
An automated market maker (AMM) for swapping between currencies within ICN. Governance-controlled, not speculative.

---

## Fuel System

### Fuel
Permission to perform network operations. Fuel is:
- **Regenerative**: Refills over time
- **Non-transferable**: Can't be traded
- **Contribution-based**: More contribution = higher max

Fuel is **not** a token or currency. It's a rate-limiting mechanism.

### Fuel Allowance
Your personal fuel capacity, calculated from:
- Base allowance (everyone gets this)
- Trust bonus (higher trust = more fuel)
- Contribution bonus (more contribution = more fuel)

### Fuel Pool
Fuel exists at every organizational level:
- **Network Pool**: Cross-federation operations
- **Federation Pool**: Cross-coop operations
- **Coop Pool**: Internal operations
- **Member Allowance**: Individual activity

### Regeneration Rate
How fast fuel refills. Default: full regeneration in 24 hours.

### Fuel Cost
Each operation consumes fuel:
- Publish message: 1 fuel
- Ledger transaction: 10 fuel
- Create proposal: 50 fuel
- Submit compute job: Variable

### Compute Limit
The execution limit for CCL contract execution. This is **distinct from fuel** but draws from the same fuel pool when submitting jobs.

**Important**: In CCL code, the internal execution limit is still called "fuel" for historical reasons. In user-facing documentation, use "compute limit" when referring to contract execution bounds.

---

## Infrastructure

### Node
An instance of `icnd` (the ICN daemon) providing network infrastructure.

### Contribution
Resources provided to the network:
- **Compute**: CPU cycles for job execution
- **Storage**: Disk space for data replication
- **Bandwidth**: Network relay capacity
- **Uptime**: Continuous availability

### Device Network
A collection of devices (phones, laptops, servers) owned by one DID, contributing resources together.

### Attestation (Contribution)
When peers verify that a node actually provided claimed resources. Requires trust-weighted signatures.

### Metering
Automatic measurement of resource contribution via Prometheus metrics.

---

## Governance

### Proposal
A formal suggestion for change, requiring votes to pass.

### Domain
A governance scope (e.g., "coop:food-collective", "federation:pnw").

### Quorum
The minimum participation required for a vote to be valid.

### Threshold
The percentage of approval required for a proposal to pass.

### Protocol Contract
Economic rules encoded in CCL. Can be:
- **Adopted**: Used as-is
- **Extended**: Customized with additions
- **Replaced**: Completely custom

### Role
A governance position with specific powers:
- **Steward**: Day-to-day administration
- **Facilitator**: Meeting and process management
- **Delegate**: Represents org in federation

---

## Technical Terms

### CCL (Cooperative Contract Language)
ICN's domain-specific language for expressing agreements and economic rules. Intentionally limited (not Turing-complete).

### Ledger
The double-entry accounting system tracking all credit movements. Uses Merkle-DAG for integrity.

### Gossip
The P2P messaging protocol for distributing information across the network.

### QUIC
The transport protocol used for node-to-node communication (over UDP, with TLS).

### Gateway
The REST/WebSocket API for applications to interact with ICN.

---

## Disambiguation

### Fuel vs. Gas
| Concept | ICN Fuel | Blockchain Gas |
|---------|----------|----------------|
| Acquired by | Contribution + trust | Buying tokens |
| Transferable | No | Yes |
| Speculation | Impossible | Common |
| Regeneration | Yes (time-based) | No |
| Purpose | Rate limiting | Payment |

### Fuel vs. Compute Limit
| Context | Term to Use |
|---------|-------------|
| User-facing docs | "Fuel" for network ops, "compute limit" for CCL jobs |
| CCL internal code | "fuel" (historical, internal only) |
| API parameters | `fuel_limit` for compute jobs |
| Metrics | `icn_fuel_*` for all fuel metrics |

### Credit vs. Token
| ICN Credits | Crypto Tokens |
|-------------|---------------|
| Internal accounting | External asset |
| Mutual credit (can be negative) | Always positive |
| No exchange trading | Exchange tradeable |
| Value from reciprocity | Value from scarcity |
| Designed to circulate | Designed to appreciate |

### Community vs. Cooperative
| Community | Cooperative |
|-----------|-------------|
| Civic focus | Economic focus |
| Mutual aid, stewardship | Trade, production |
| Belonging | Livelihood |
| May not have currency | Has currency/ledger |
| Less formal governance | Formal bylaws |

Both are first-class ICN entities. Individuals typically belong to both.

### Internal vs. Federated vs. Bridge
| Tier | Scope | Can Cash Out? |
|------|-------|---------------|
| Internal | One coop | No |
| Federated | Partner orgs | No |
| Bridge | External | Yes (governed) |

---

## Usage Guidelines

### In Documentation
- Always qualify credit types: "coop-credits", not just "credits"
- Always qualify hour types: "labor-hours", "CPU-hours"
- Use "fuel" for network operations
- Use "compute limit" for CCL execution bounds

### In Code
- Variable names should match glossary terms
- Comments should use glossary terms
- Error messages should use glossary terms

### In User Interfaces
- Prefer plain language with glossary terms
- Provide tooltips linking to glossary
- Avoid jargon without explanation

---

**Feedback**: If you encounter a term that needs clarification, open an issue at https://github.com/InterCooperative-Network/icn/issues
