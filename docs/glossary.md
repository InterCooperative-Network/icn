# ICN Glossary

**Status**: Reference Document
**Version**: 1.2.0
**Last Updated**: 2025-12-14

*Expanded with contribution credits terminology (Phase 21) and proposed Commons Evolution terms (RFC)*

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
9. [Related Documents](#related-documents)
10. [Proposed Terms (Commons Evolution RFC)](#proposed-terms-commons-evolution-rfc)

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

**Eligibility requirements** for contribution attesters:
- Membership age ≥ 90 days
- Trust score ≥ 0.3
- Max 10 attestations per period
- Non-reciprocity (cannot attest someone who attested you)

### Organizational Attestation
For large contributions (> 500 credits), at least one attester must be an **organizational DID** (coop, community, or household), not just individuals.

### Sybil Resistance
Protections against fake identity attacks in the attestation system:
- Membership age requirements
- Trust score thresholds
- Attestation budgets
- Non-reciprocity rules
- Organizational anchoring for large claims

### Trust Graph Depth
Attestation weight based on path length from organizational anchors. Members with no path to an org have zero attestation weight.

---

## Organizations

### InstitutionalDomain
The **structural scope** of a governed entity in ICN — an `Individual`, `Cooperative`, `Community`, `Federation`, or another governed class permitted by domain policy. The addressable, self-governing jurisdiction inside which authority decisions are legitimate. Specified in [`docs/spec/institutional-domain.md`](spec/institutional-domain.md).

An `InstitutionalDomain` is **not** an entity class. It is the scope the entity owns. Several entity classes can own a domain — see `owning entity class` below.

### LocalDomain
Shorthand for an `InstitutionalDomain` at the **local institutional layer** (between member/cell-local scope and federation/commons scope). Prefer `LocalDomain` over `Coop` when naming a generic scope-level concept: a local domain may be owned by a `Cooperative`, a `Community`, a `Federation` acting as its own entity, or another governed class permitted by policy.

See [`docs/architecture/INSTITUTION_PACKAGE_BOUNDARY.md`](architecture/INSTITUTION_PACKAGE_BOUNDARY.md) §C3 "Entity-scope vocabulary" for the boundary rule.

### Owning entity class
The class of governed entity that owns an `InstitutionalDomain`. Per [`docs/spec/institutional-domain.md`](spec/institutional-domain.md), the closed (but extensible by policy) set is `Individual` / `Cooperative` / `Community` / `Federation`, or another governed entity class permitted by domain policy. The class shapes which adopted-charter forms are permitted and which authority classes the entity may grant.

The owning entity class is **a property of a domain**, not a substitute for the domain's name. A domain owned by a cooperative is "a domain whose owning entity class is `Cooperative`," not "a Coop."

### Cooperative (Coop)
A formal economic organization with:
- Registered membership
- Bylaws and governance
- Treasury and ledger
- Economic activity (trade, production)

Cooperatives are the **economic engine** of ICN and one possible owning entity class for an `InstitutionalDomain`. In generic ICN-core architecture text, **prefer `LocalDomain` or `InstitutionalDomain` when naming a scope**; use `Cooperative` only when the subject is specifically the cooperative entity class (per `docs/spec/institutional-domain.md`), the cooperative-movement context, or a cooperative-specific setup path / institution package.

### Community
A civic organization for:
- Mutual aid
- Stewardship
- Advocacy
- Public service

Communities are **first-class entities** in ICN, not just informal groups. They have DIDs, fuel pools, and governance. A community is one possible owning entity class for an `InstitutionalDomain`; community-owned domains are parallel to cooperative-owned domains at the local-institutional scope level (per `docs/spec/institutional-domain.md`).

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

### Contribution Routing
How infrastructure credits are split when earned. Contributors specify allocations:
- **Personal**: To individual account
- **Cooperative**: To coop treasury
- **Community**: To community
- **Household**: To shared household pool
- **GlobalCommons**: To network-wide pool
- **Federation**: To federation treasury

**Exploitation safeguards**:
- Real membership requirements (2+ independent DIDs for households)
- Pass-through ceilings (max % from single source: 25%)
- Velocity limits (rapid changes trigger review)

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

### Marketplace
The internal economy where credits are spent on goods, services, and infrastructure.

**Listing types**:
- **Service**: Labor with duration (tutoring, repair, care)
- **Good**: Physical items (food, tools, crafts)
- **Digital**: Content with license (software, designs)
- **Infrastructure**: Compute/storage capacity
- **Subscription**: Recurring service access

### Human Labor Credit
Credits earned through human work (as opposed to infrastructure contributions).

**Labor types**:
- **Organizational labor**: Logged via coop/community work tracking
- **Service delivery**: Peer-to-peer marketplace services
- **Community service**: Care work, mutual aid

**Labor categories**: Governance, Operations, Development, Outreach, Education, Care, Skilled, Creative, Physical

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

### Fuel Guarantees
**Minimum civic fuel** (50 units) ensures every member can participate in governance regardless of contribution level.

**Guaranteed operations** (never blocked by fuel exhaustion):
- Cast vote on eligible proposals (1 per proposal)
- Create one proposal per governance period
- Send 5 direct messages per day
- View own balances (unlimited)
- Respond to disputes involving self

### Fuel Reservation
For compute jobs, fuel is **reserved upfront** from the submitter's account. Unused fuel is returned when the job completes.

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

**Key protocol contracts**:
- `infrastructure-credit/v1`: Rates for infrastructure → credit conversion
- `fuel-allocation/v1`: Fuel allowance calculation rules
- `demurrage/v1`: Circulation charge policies

### Role
A governance position with specific powers:
- **Steward**: Day-to-day administration
- **Facilitator**: Meeting and process management
- **Delegate**: Represents org in federation

### AuthorityClass
Closed enumeration of constitutional authority kinds, frozen in [ADR-0014](adr/ADR-0014-constitutional-object-model.md). Exactly three variants:
- **Representation** — speaking/voting in place of another within a bounded scope
- **Execution** — carrying out an action that mutates institutional state
- **Attestation** — issuing signed statements of fact downstream consumers may rely on

These three are distinct and must not be silently conflated. Representation does not confer Execution authority; Execution does not confer Attestation authority.

### AuthorityGrant
A bounded, auditable grant of authority issued by a sovereign entity to a grantee, in exactly one `AuthorityClass`. Carries: grantor entity, grantee DID or entity identifier (for example, a federation or cooperative, or a shared-service entity acting as an institutional actor), `TypedScope`, optional `proposal_id` + `decision_hash` provenance, validity window, optional revocation timestamp. App-layer constitutional meaning, not a kernel primitive. The ICN runtime, daemon, or gateway are never valid grantors. Frozen in [ADR-0014](adr/ADR-0014-constitutional-object-model.md); not yet implemented.

### TypedScope
Typed replacement path for today's `RoleAssignment.authority_scope: Vec<String>` capability strings. Conjunction of present scope categories: domain, proposal-class, action-kind, amount/ceiling, time-window. Absent categories are "unbounded along that axis"; at least one category must be populated for a grant to be meaningful. Frozen in [ADR-0014](adr/ADR-0014-constitutional-object-model.md).

### Mandate
The constitutional mediation layer between a governance decision and the execution of its effects. Binds a `GovernanceDecisionReceipt` (the decision) to one or more `AuthorityGrant`s, optionally an executor and a deadline, with lifecycle `Pending → InProgress → Discharged | Expired | Revoked`. **Not** a delegation, not a role assignment, not an office, not an `InstitutionalEffectRecord`, not a raw receipt, not a federation agreement, not a charter. It is the missing object between proposal/decision and execution/effect/evidence. Frozen in [ADR-0014](adr/ADR-0014-constitutional-object-model.md); not yet implemented.

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

## Related Documents

- [Contribution Credits Design](design/economics/contribution-credits-design.md) - Full economic design specification
- [ARCHITECTURE.md](ARCHITECTURE.md) - System architecture
- [governance-primitives.md](design/governance/governance-primitives.md) - Governance layer design

---

## Proposed Terms (Commons Evolution RFC)

*These terms are proposed in [COMMONS_EVOLUTION.md](design/COMMONS_EVOLUTION.md) and are under review for v0.3.x+. They are not yet implemented.*

### The Commons
**Status**: Proposed (v0.3.0+)

The shared global coordination layer of ICN: identity, rights, governance, and economic participation held in common by all participants. The Commons provides baseline infrastructure that individual jurisdictions (cooperatives, communities, federations) build upon.

### Commons Holder
**Status**: Proposed (v0.3.0+)

A human participant recognized by the network as holding baseline rights and responsibilities in the commons. Commons Holder status is independent of membership in any specific cooperative or community. A Commons Holder may have multiple affiliations (memberships) with different jurisdictions.

**Key property**: Jurisdictions can revoke membership; they cannot revoke Commons Holder status.

### Personhood Anchor
**Status**: Proposed (v0.3.0+)

The cryptographic root of a Commons Holder's identity, independent of key rotation. A Personhood Anchor proves that a unique human controls the associated keys, enabling Sybil resistance at the network level.

**Requires**: Proof-of-personhood attestations from stewards or existing verified holders.

### Charter
**Status**: Proposed (v0.3.0+)

The founding document that creates a cooperative, community, or federation as a jurisdiction within the commons. A Charter specifies:
- Organization type (coop, community, federation)
- Governance profile
- Membership policy
- Economic policy (if applicable)
- Dispute resolution rules

### Jurisdiction
**Status**: Proposed (v0.3.0+)

A coop, community, or federation with its own charter, governance, and membership rules. Jurisdictions operate within the commons framework and cannot override baseline Commons Rights.

**Relationship to current terms**: This term generalizes "cooperative" and "community" into a unified concept.

### Steward (Commons Context)
**Status**: Proposed (v0.3.0+)

A trusted operator who performs identity verification (proof-of-personhood attestation), infrastructure maintenance, or governance functions on behalf of the network. Stewards are:
- Commons Holders themselves
- Term-limited (not permanent positions)
- Bonded (stake against misbehavior)
- Subject to governance oversight

**Note**: This extends the existing "Steward" role definition in the Governance section.

### Commons Rights
**Status**: Proposed (v0.3.0+)

The baseline rights held by all Commons Holders, which cannot be revoked by individual jurisdictions:
- **Voice**: Vote in commons-level governance
- **Petition**: Propose constitutional changes
- **Fuel Floor**: Guaranteed minimum fuel for civic participation
- **Due Process**: Right to appeal before suspension/revocation
- **Exit**: Leave any jurisdiction freely
- **Audit**: Access public records

### Affiliation
**Status**: Proposed (v0.3.0+)

A Commons Holder's membership relationship with a specific jurisdiction, granting additional capabilities (voting, transacting, holding office) within that scope. A Commons Holder may have multiple affiliations simultaneously.

**Relationship to current terms**: This replaces the simpler "membership" concept with a more precise model.

### Proof-of-Personhood (POP)
**Status**: Proposed (v0.3.0+)

Verification that a Commons Holder represents a unique human, not a bot or duplicate identity. POP methods include:
- **In-person**: Verification by a steward
- **Sponsored**: Vouched for by existing verified holders
- **Biometric**: Privacy-preserving biometric verification
- **Ceremony**: Community key-signing events

---

**Feedback**: If you encounter a term that needs clarification, open an issue at https://github.com/InterCooperative-Network/icn/issues
