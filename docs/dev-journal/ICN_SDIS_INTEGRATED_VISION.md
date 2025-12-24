# ICN + SDIS Integrated Vision

## The Cooperative Operating System for Human Civilization

**Version**: 1.0 Draft
**Date**: 2025-12-23
**Status**: Vision Document

---

## Executive Summary

The InterCooperative Network (ICN) and the Sovereign Digital Identity System (SDIS) are not separate projects—they are two halves of one whole. Together they form a complete **cooperative operating system**:

- **SDIS provides identity**: proof of personhood, selective disclosure, accountability without surveillance
- **ICN provides coordination**: economic settlement, governance, compute, trust relationships

Neither is complete without the other. SDIS without ICN is identity without economy. ICN without SDIS is economy without verified humans.

### The Core Insight: ICN as Cooperative Middle Layer

**ICN is not infrastructure FOR cooperatives—it IS cooperative infrastructure that is itself democratically governed.**

This makes ICN a "cooperative middle layer" like a secondary/tertiary cooperative, where:
- The protocols are adjustable by the organizations using them
- Governance rules, economic parameters, capability tiers—all democratically modifiable
- Different federations can customize within limits set by their parent level
- The system governs itself using the same patterns it provides to users

---

## Part 1: The Unified Recursive Model

### Everything is the Same Structure

The breakthrough insight: **every level of organization follows the same pattern**:

| Scale | Entity | Members | Governance | Economy |
|-------|--------|---------|------------|---------|
| **Individual** | Person | Devices | Personal policies | Personal accounts |
| **Cooperative** | Coop | Workers/Members | Bylaws, votes | Internal ledger |
| **Federation** | Regional Fed | Cooperatives | Federation charter | Inter-coop clearing |
| **Meta-Federation** | National/Continental | Federations | Alliance treaty | Cross-regional settlement |
| **Global Commons** | ICN Protocol | All meta-federations | Protocol governance | Universal settlement |

This means **identity/multi-device IS the same pattern as coop/federation**, just at different scales.

### The CooperativeEntity: Universal Building Block

```rust
/// Universal entity structure - same at every scale
pub struct CooperativeEntity {
    // === SDIS Identity Layer ===
    anchor: Anchor,                        // Permanent identity (from SDIS L0)
    key_bundle: KeyBundle,                 // Rotatable keys (SDIS)
    credentials: Vec<Credential>,          // L2 proofs + L3 seals
    citizenship: Option<CitizenshipCredential>, // SDIS L1 (network participation)

    // === Recursive Membership ===
    entity_type: EntityType,
    members: Vec<EntityRef>,               // Can be people OR other entities
    membership_rules: MembershipRules,     // Who can join, how
    parent_federation: Option<EntityRef>,  // Who we federate UP to

    // === ICN Governance ===
    governance_config: GovernanceConfig,
    voting_power: VotingPowerScheme,       // One person one vote (enforced by SDIS)
    delegation_rules: DelegationRules,
    proposals: Vec<Proposal>,

    // === ICN Economics ===
    treasury: Option<Treasury>,            // For coops and above
    accounts: Vec<Account>,
    clearing_agreements: Vec<ClearingAgreement>,
    contribution_ledger: ContributionLedger,

    // === ICN Trust ===
    trust_endorsements: Vec<Endorsement>,  // L3 seals from other entities
    trust_delegations: Vec<Delegation>,
    trust_scores: HashMap<TrustDomain, f64>,

    // === ICN Compute ===
    executors: Vec<EntityRef>,             // Nodes that can execute for this entity
    compute_policies: Vec<SchedulingPolicy>,
}

pub enum EntityType {
    Individual,          // A person (SDIS anchor required)
    Device,              // A device bound to a person
    Cooperative,         // A single coop (workers, housing, etc.)
    Federation,          // Multiple coops federated
    MetaFederation,      // Multiple federations federated
    ProtocolCommons,     // The ICN protocol itself
}
```

### How Multi-Device Identity Works

Your personal identity is a federation of devices, governed by YOU:

```
You (SDIS Anchor: did:sdis:alice...)
│
├── Your Governance Rules:
│   ├── Key rotation: I approve
│   ├── New device: requires 2FA + approval
│   └── Recovery: 3-of-5 trusted contacts
│
├── Devices (each has sub-keys derived from your L0):
│   ├── Phone (subkey, limited capabilities)
│   │   └── Can: sign transactions < 100 credits
│   │   └── Cannot: governance votes, large transfers
│   │
│   ├── Laptop (subkey, full capabilities)
│   │   └── Can: all operations
│   │
│   └── Hardware Key (recovery, high-security ops)
│       └── Required for: key rotation, large governance
│
└── SDIS Properties:
    ├── Anchor is permanent (survives key rotation)
    ├── Each device key derived from L0 seed
    └── Credentials reference Anchor, not device keys
```

**Key insight**: The same pattern that governs your devices is the same pattern that governs a cooperative's members, or a federation's cooperatives. **It's recursive all the way down.**

---

## Part 2: SDIS as ICN's Identity Foundation

### SDIS Layers Map to ICN Entities

SDIS defines four identity layers. These map directly to ICN's recursive entity model:

| SDIS Layer | What It Proves | ICN Integration |
|------------|----------------|-----------------|
| **L0: Proof of Personhood** | "I am a unique human" | Root identity anchor for all ICN entities |
| **L1: Digital Citizenship** | "I can participate in networks" | Membership in cooperatives, federations |
| **L2: Attribute Proofs** | "I have property X" (age, location, skills) | Trust domains, capability unlocks |
| **L3: Institutional Seals** | "Authority Y vouches for me" | Cooperative endorsements, federation attestations |

### One Person, One Vote (Sybil-Resistant Democracy)

ICN governance REQUIRES SDIS identity because:
- **One person, one vote** only works if each identity is one person
- Democratic participation requires verified humans (not bots)
- Accountability requires traceable-if-fraudulent identities
- Privacy requires selective disclosure (vote without revealing who you are)

### Zero-Knowledge Voting

```
ZK_Vote_Structure:
    Private Inputs (known only to voter):
        - Voter's SDIS anchor
        - Membership proof (I belong to this coop)
        - Vote choice (yes/no/abstain)

    Public Inputs:
        - Proposal ID
        - Coop's member accumulator root
        - Current revocation accumulator root
        - Voting period bounds

    Proof Statement:
        "I know anchor A and membership witness M such that:
         1. A is a valid, non-revoked SDIS anchor
         2. A is a member of coop C (membership accumulator check)
         3. I have not already voted on this proposal (nullifier check)
         4. My vote is V"

    Output:
        - STARK proof (~50-100 KB)
        - Nullifier (prevents double-voting without revealing voter)
        - Encrypted vote (revealed in tally phase)
```

---

## Part 3: The Cooperative Middle Layer

### ICN Governs Itself Cooperatively

The ICN protocol itself is a CooperativeEntity—the "Protocol Commons"—democratically governed by its users:

```
┌─────────────────────────────────────────┐
│          ICN Protocol Commons            │
│  (The cooperative that governs ICN)     │
├─────────────────────────────────────────┤
│  Members: All meta-federations          │
│  Governance: Protocol-level decisions   │
│  Economy: Cross-meta-federation settle  │
│                                         │
│  What Can Be Changed:                   │
│  ├── Protocol versions and upgrades    │
│  ├── Base capability tier thresholds   │
│  ├── Settlement currency standards     │
│  ├── Security parameter minimums       │
│  └── Federation admission criteria     │
│                                         │
│  What Cannot Be Changed:                │
│  ├── One person one vote principle     │
│  ├── Data sovereignty rights           │
│  ├── Member exit rights                │
│  └── Anti-extraction guarantees        │
└─────────────────────────────────────────┘
```

### Subsidiarity: Decisions at the Lowest Appropriate Level

```rust
pub enum DecisionScope {
    /// Decided by the individual
    Personal,
    /// Decided by the cooperative
    Local,
    /// Decided by the federation
    Regional,
    /// Decided by the meta-federation
    Continental,
    /// Decided by the Protocol Commons
    Global,
}

pub struct ProtocolParameter {
    name: String,
    current_value: ParameterValue,

    /// Who can modify this parameter
    modification_scope: DecisionScope,

    /// Constraints from higher levels
    constraints: Vec<ParameterConstraint>,

    /// Last modification
    last_changed: ChangeRecord,
}
```

### Customizable Parameters at Each Level

| Level | Customizable | Constrained By |
|-------|--------------|----------------|
| **Individual** | Device permissions, delegation prefs | Cooperative policies |
| **Cooperative** | Credit limits, voting rules, pricing | Federation minimums |
| **Federation** | Settlement terms, admission criteria | Meta-federation standards |
| **Meta-Federation** | Regional policies, exchange rates | Protocol guarantees |
| **Protocol** | Base parameters, upgrade cadence | Constitutional principles |

---

## Part 4: Real Economics - Enabling the Cooperative Ecosystem

### The Three-Layer Economic Stack

```
┌─────────────────────────────────────────┐
│  Layer 3: External Interface            │
│  (Transition to/from capitalist economy)│
│  - Fiat on/off ramps                   │
│  - Exchange with credit unions          │
│  - Tax compliance interfaces            │
└─────────────────────────────────────────┘
           ↑ Minimal, regulated ↑
┌─────────────────────────────────────────┐
│  Layer 2: Inter-Cooperative             │
│  (Federation-level coordination)        │
│  - Bilateral clearing agreements        │
│  - Federation settlement credits        │
│  - Netting and batch settlement         │
│  - Cross-coop group purchasing          │
└─────────────────────────────────────────┘
           ↑ Democratic governance ↑
┌─────────────────────────────────────────┐
│  Layer 1: Internal (within a coop)      │
│  (Use whatever accounting works)        │
│  - Labor hours, points, internal credit │
│  - Work tracking, contribution records  │
│  - Tailored to cooperative's culture    │
└─────────────────────────────────────────┘
```

### Value Extraction FROM Capitalism, INTO Cooperatives

The cooperative ecosystem extracts value that capitalism normally captures as profit:

| Value Source | Traditional Capture | Cooperative Capture | Estimated Savings |
|--------------|--------------------|--------------------|-------------------|
| **Group Purchasing** | Retailer margins | Bulk discounts stay in coop | 15-25% |
| **Shared Services** | Duplicate overhead | Pooled infrastructure | 20-40% |
| **Direct Trade** | Intermediary fees | Peer-to-peer exchange | 10-30% |
| **Labor Exchange** | Platform extraction | Reciprocal credit | 20-35% |
| **Financing** | Bank interest | Mutual credit, internal loans | 5-15% |
| **Insurance** | Corporate profits | Mutual coverage pools | 10-20% |

**This value stays in the cooperative ecosystem** instead of leaking to shareholders, landlords, and bankers.

### Coop-to-Coop Agreements

The foundation of real cooperative economics:

```rust
pub struct InterCoopAgreement {
    /// Parties to the agreement
    party_a: EntityRef,  // e.g., FoodCoop
    party_b: EntityRef,  // e.g., TechCoop

    /// What's being exchanged
    exchange_terms: ExchangeTerms,

    /// Settlement mechanics
    settlement: SettlementConfig,

    /// Governance
    modification_rules: ModificationRules,
    dispute_resolution: DisputeResolution,

    /// Status
    status: AgreementStatus,
    signed_at: Timestamp,
    signatures: Vec<EntitySignature>,
}

pub enum ExchangeTerms {
    /// Direct barter: X hours of tech support for Y produce credits
    DirectExchange { a_provides: Offer, b_provides: Offer },

    /// Credit clearing: net positions settled periodically
    ClearingAgreement { clearing_frequency: Duration, currency: CreditCurrency },

    /// Group purchasing: pooled buying power
    GroupPurchasing { categories: Vec<ProductCategory>, discount_sharing: SharingRule },

    /// Shared service: jointly operated resource
    SharedService { service: ServiceDefinition, cost_sharing: SharingRule },

    /// Labor exchange: workers available across coops
    LaborExchange { skill_categories: Vec<Skill>, hourly_rates: RateSchedule },
}
```

### Group Purchasing Networks

```
┌─────────────────────────────────────────┐
│     Food Cooperative Federation         │
│           (15 coops, 50K members)       │
├─────────────────────────────────────────┤
│  Pooled Purchasing Power:               │
│  ├── Organic produce: $2M/year          │
│  ├── Dry goods: $500K/year              │
│  └── Equipment: $200K/year              │
│                                         │
│  Negotiating Leverage:                  │
│  ├── Direct farmer relationships        │
│  ├── Volume discounts (15-25%)          │
│  └── Quality standards enforcement      │
│                                         │
│  Value Distribution:                    │
│  ├── Savings → member patronage refunds │
│  ├── Surplus → federation development   │
│  └── Reserve → new coop startup fund    │
└─────────────────────────────────────────┘
```

### Contribution-Based Credit

Instead of paying for compute/services, you **earn credit by contributing**:

```rust
pub struct ContributionRecord {
    contributor: EntityRef,
    contribution_type: ContributionType,
    amount: ContributionAmount,
    timestamp: Timestamp,
    verified_by: Vec<EntityRef>,
    decay_rate: f64,  // Contributions decay over time
}

pub enum ContributionType {
    /// Running executor node
    ComputeProvision { fuel_processed: u64, uptime_hours: f64 },
    /// Storing data for the network
    StorageProvision { gb_hours: f64 },
    /// Participating in governance
    GovernanceParticipation { votes_cast: u32, proposals_reviewed: u32 },
    /// Providing labor to cooperatives
    LaborContribution { hours: f64, skill_level: SkillLevel },
    /// Providing capital/resources
    ResourceContribution { resource_type: String, value: CreditAmount },
}

// Credit limit calculation
impl CreditPolicy {
    fn calculate_limit(&self, entity: &CooperativeEntity) -> CreditAmount {
        let base = self.base_limit;
        let trust_bonus = entity.trust_score() * self.trust_multiplier;
        let history_bonus = entity.cleared_obligations() / 10;
        let contribution_bonus = entity.contribution_score() * self.contribution_multiplier;

        base + trust_bonus + history_bonus + contribution_bonus
    }
}
```

### Anti-Extraction Policies

Prevent exploitation while enabling cooperation:

```rust
pub struct AntiExtractionPolicy {
    /// Can't consume more than N times what you contribute
    max_consumption_ratio: f64,  // e.g., 5.0

    /// Must contribute before consuming
    min_contribution_before_consumption: ContributionAmount,

    /// Velocity limits prevent draining
    max_consumption_per_day: f64,  // As fraction of credit limit

    /// New member ramp period
    new_member_ramp_days: u32,

    /// Surplus redistribution
    surplus_distribution: SurplusDistribution,
}

pub enum SurplusDistribution {
    /// Equal to all members
    EqualDistribution,
    /// Proportional to contribution
    ContributionWeighted,
    /// Into infrastructure fund
    InfrastructureReserve,
    /// Hybrid approach
    Hybrid { members: f64, infrastructure: f64, development: f64 },
}
```

---

## Part 5: Recursive Federation Architecture

### The Vision: Unlimited Scale Through Hierarchy

ICN must scale beyond any single network. The answer is **recursive federation**—cooperatives form federations, federations form meta-federations, ad infinitum.

```
                            ┌─────────────────────────────────────┐
                            │       Global Cooperative Commons     │
                            │      (Meta-Meta-Federation)          │
                            └────────────────┬────────────────────┘
                                             │
                   ┌─────────────────────────┼─────────────────────────┐
                   │                         │                         │
        ┌──────────▼──────────┐   ┌──────────▼──────────┐   ┌──────────▼──────────┐
        │   Americas Alliance  │   │   European Commons   │   │   Asia-Pacific Net  │
        │   (Meta-Federation)  │   │   (Meta-Federation)  │   │   (Meta-Federation)  │
        └──────────┬──────────┘   └──────────┬──────────┘   └──────────┬──────────┘
                   │                         │                         │
         ┌─────────┼─────────┐     ┌─────────┼─────────┐     ┌─────────┼─────────┐
         │         │         │     │         │         │     │         │         │
      ┌──▼──┐   ┌──▼──┐   ┌──▼──┐ ┌──▼──┐   ┌──▼──┐   ┌──▼──┐ ┌──▼──┐   ┌──▼──┐   ┌──▼──┐
      │ US  │   │ MX  │   │ BR  │ │ UK  │   │ DE  │   │ FR  │ │ JP  │   │ AU  │   │ IN  │
      │ Fed │   │ Fed │   │ Fed │ │ Fed │   │ Fed │   │ Fed │ │ Fed │   │ Fed │   │ Fed │
      └──┬──┘   └──┬──┘   └──┬──┘ └──┬──┘   └──┬──┘   └──┬──┘ └──┬──┘   └──┬──┘   └──┬──┘
         │         │         │     │         │         │     │         │         │
       coops     coops     coops  coops     coops     coops  coops     coops     coops
```

### How Recursive Trust Works

Trust flows up AND down the hierarchy:

```
Trust UP (Delegation):
└── Coop trusts Federation to represent it in meta-federation
└── Implies: Federation can sign on coop's behalf (scoped)

Trust DOWN (Endorsement):
└── Federation endorses coop to external parties
└── Implies: Coop inherits some of federation's reputation

Trust ACROSS (Peer):
└── Coops in same federation trust each other (transitively)
└── Implies: Can transact without explicit bilateral trust
```

**Example Trust Calculation**:
```
Alice (individual) in FoodCoop
FoodCoop in Northeast Federation
Northeast in Americas Alliance

External party queries: "Do I trust Alice?"

1. Americas Alliance trust = 0.9 (well-known)
2. Northeast trust = 0.8 × 0.9 = 0.72
3. FoodCoop trust = 0.7 × 0.72 = 0.504
4. Alice trust = 0.6 × 0.504 = 0.302

Result: Alice has 0.302 effective trust to external party
(Even though external party never met Alice directly)
```

### Recursive Governance: Proposals Bubble Up

```
Local Issue → Local Vote
  └── 50% quorum, 50% approval at coop level

Regional Issue → Federation Vote
  └── Each coop gets weighted vote (by membership)
  └── 40% quorum of coops, 60% approval

Cross-Regional → Meta-Federation Vote
  └── Each federation gets weighted vote
  └── 30% quorum of federations, 70% approval

Global Protocol Change → Global Commons Vote
  └── All meta-federations participate
  └── 25% quorum, 80% supermajority required
```

### Recursive Economic Settlement

Multi-level clearing with netting at each level:

```
Level 1: Intra-Coop
└── Alice pays Bob (same coop)
└── Instant settlement in coop ledger

Level 2: Intra-Federation
└── Alice (FoodCoop) pays Carol (TechCoop)
└── Both in Northeast Fed
└── Daily batch settlement via federation clearing

Level 3: Cross-Federation
└── Alice (Northeast) pays Dave (Midwest)
└── Both in Americas Alliance
└── Weekly settlement via meta-federation clearing

Level 4: Cross-Meta-Federation
└── Alice (Americas) pays Eve (European Commons)
└── Monthly settlement via Global Commons
└── Exchange rate governance at global level
```

---

## Part 6: What This Enables

### Replacing Traditional Enterprise

The cooperative ecosystem can replace every function currently performed by capitalist enterprises:

| Capitalist Function | Cooperative Replacement | ICN Enabler |
|---------------------|------------------------|-------------|
| Banks | Credit unions + mutual credit | Ledger + clearing agreements |
| Insurance | Mutual coverage pools | Smart contracts + risk pooling |
| Platforms (Uber, etc.) | Worker-owned cooperatives | Compute + governance |
| Supply chains | Federation purchasing | Group purchasing + settlement |
| HR/Payroll | Member contribution tracking | Contribution ledger |
| Logistics | Federated coordination | Compute + cross-coop agreements |

### Rendering the State Less Necessary

As cooperatives provide more services democratically:

| State Function | Cooperative Alternative | How |
|----------------|------------------------|-----|
| Welfare | Mutual aid networks | Contribution-based credit, reciprocity |
| Regulation | Self-governance + transparency | Democratic policies, audit trails |
| Courts | Dispute resolution | Multi-stage arbitration, federation appeals |
| Currency | Mutual credit networks | Democratic monetary policy |
| Property records | Distributed ledger | Immutable, transparent records |

**Not "no governance"—but governance by those affected, not distant bureaucrats.**

---

## Part 7: Implementation Path

### Phase 1: Foundation (Current)
- [x] Basic identity (DID)
- [x] Trust graph
- [x] Mutual credit ledger
- [x] CCL contracts
- [x] Gateway API
- [x] Treasury management
- [ ] SDIS integration (in progress)

### Phase 2: Cooperative Middle Layer
- [ ] CooperativeEntity as universal primitive
- [ ] Recursive membership model
- [ ] Inter-coop agreements
- [ ] Protocol governance framework
- [ ] Subsidiarity enforcement

### Phase 3: Real Economics
- [ ] Group purchasing support
- [ ] Bilateral clearing
- [ ] Contribution tracking
- [ ] Anti-extraction policies
- [ ] Federation settlement

### Phase 4: Scale
- [ ] Multi-level federation
- [ ] Cross-federation trust
- [ ] Global commons governance
- [ ] Exchange rate governance
- [ ] Planetary-scale deployment

---

## Part 8: Open Questions

These require community input and experimentation:

1. **Exchange Rate Governance**: How do different credit systems exchange value fairly?
2. **Capital Formation**: How do cooperatives accumulate capital without recreating capitalist finance?
3. **Asymmetric Federations**: How to prevent larger coops from dominating smaller ones?
4. **Existing Coop Integration**: How do established credit unions and grocery coops join?
5. **Legal Recognition**: How does this map to existing cooperative law?
6. **Transition Economics**: How to interface with fiat while building alternative?
7. **Dispute Escalation**: When do disputes escalate vs. stay local?
8. **Protocol Amendments**: What requires supermajority vs. simple majority?
9. **Emergency Powers**: How to handle crises without creating permanent authorities?
10. **Exit Rights**: How can coops/federations exit cleanly?

---

## Appendix: SDIS Integration Details

### Stewards as Cooperative Members

SDIS Stewards (who issue POPs) are integrated into the cooperative structure:

```
Steward Network = A specialized cooperative within ICN

Members: Individual Stewards (each has personal POP)
Governance: Democratic (SDIS governance rules + ICN governance)
Economics:
  - Stewards compensated via ICN economic layer
  - Enrollment fees flow through ICN clearing
  - Steward bonds held in ICN-managed escrow

Benefits:
  - Steward compensation uses same economics as other work
  - Steward governance uses same patterns as other coops
  - Steward accountability enforced by cooperative norms
```

### Cooperative SDIS Enrollment

A cooperative is not a person, but it needs identity for:
- Signing agreements with other coops
- Receiving institutional attestations
- Endorsing its members to external parties
- Participating in federation governance

```
Cooperative_Enrollment:
    Prerequisites:
        - Founding members have valid POPs
        - Articles of incorporation registered (L3 seal from legal authority)
        - Governance structure defined

    Process:
        1. Founding ceremony: 3-of-5 founding members present
        2. Each signs coop formation document with personal SDIS keys
        3. Coop generates organizational Anchor:
           Anchor = SHA3-256(
               Member_1_Anchor || Member_2_Anchor || ... ||
               Formation_Document_Hash ||
               Timestamp
           )
        4. Coop generates KeyBundle (threshold-controlled by officers)
        5. Steward network attests: "This coop exists with these founding members"

    Result:
        - Coop has SDIS Anchor (permanent identifier)
        - Coop has KeyBundle (requires threshold of officers to sign)
        - Coop can receive L3 seals (business licenses, certifications)
        - Coop can endorse members (issue L3 attestations)
```

---

## Conclusion

ICN + SDIS together form something unprecedented: a **cooperative operating system** that is itself cooperatively governed. It enables:

- **Real economics** without capitalist extraction
- **Real democracy** with verified humans and delegation
- **Real scale** through recursive federation
- **Real sovereignty** with data that stays home
- **Real transition** from capitalist to cooperative economy

The path forward is not to build infrastructure *for* cooperatives, but to build infrastructure *as* a cooperative—one that governs itself using the same democratic principles it enables for others.

**The medium is the message. The infrastructure is the movement.**
