# Contribution Credits: Infrastructure Incentives Design

**Status**: Design Document (RFC)
**Version**: 0.3.0
**Last Updated**: 2025-12-05
**Author**: ICN Foundation

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2025-12-05 | Initial draft |
| 0.2.0 | 2025-12-05 | Added unified fuel model, communities as first-class entities, organizational structures, protocol contracts |
| 0.3.0 | 2025-12-05 | Added fuel guarantees, sybil resistance, human labor system, organizational fuel, dispute resolution, issuance principles, anti-capture principles, fraud prevention, demurrage tradeoffs, routing protection, export layer protection |

---

## Executive Summary

This document defines how ICN credits infrastructure contributors (compute, storage, bandwidth) in a **non-speculative** way. The core insight: **infrastructure provision is labor directed at the network**. Contributors earn mutual credit that can be used within the cooperative ecosystem, traded for goods/services, or (with governance approval) exchanged externally.

**Key Principles**:
- Credits represent real resource contribution, not speculative value
- No external market = no speculation
- Value flows from reciprocity, not scarcity
- Governance controls exchangeability

**Relationship to Fuel**: Credits are **claims on value**. Fuel is **permission to act**. Contributors earn credits AND receive higher fuel allowances. Both systems work together—fuel is consumed from a single regenerating pool for all network operations including compute execution.

**Crucially, this design does not introduce a tradeable token at the protocol layer.** All accounting uses mutual credit on existing ledgers, governed by cooperatives. There is no ICN coin, no mining, no external exchange.

---

## TL;DR

- **Run a node** → Earn credits for compute/storage/bandwidth
- **Credits are hours** → Spendable on goods, services, or infrastructure (or your coop's chosen base currency)
- **No speculation** → Demurrage, provenance tracking, governance controls
- **Fuel gates actions** → Regenerates over time, scales with contribution
- **Two pillars** → Communities (civic) + Cooperatives (economic)
- **Start informal** → Individuals → Households → Communities → Coops → Federations
- **Rules are contracts** → CCL protocol contracts, governable and auditable

---

## Table of Contents

1. [Problem Statement](#problem-statement)
2. [Design Philosophy](#design-philosophy)
3. [Three-Tier Credit System](#three-tier-credit-system)
4. [Infrastructure as Labor](#infrastructure-as-labor)
5. [Human Labor Credit System](#human-labor-credit-system)
6. [Contribution Verification](#contribution-verification)
7. [Fuel System](#fuel-system)
8. [Organizational Structures](#organizational-structures)
9. [Internal Marketplace](#internal-marketplace)
10. [Anti-Speculation Mechanisms](#anti-speculation-mechanisms)
11. [Credit Issuance Principles](#credit-issuance-principles)
12. [Anti-Capture Principles](#anti-capture-principles)
13. [Dispute Resolution](#dispute-resolution)
14. [Protocol Contracts](#protocol-contracts)
15. [Exchange Architecture](#exchange-architecture)
16. [Implementation Roadmap](#implementation-roadmap)
17. [Technical Specifications](#technical-specifications)
18. [Design Invariants](#design-invariants)
19. [Non-Goals](#non-goals)
20. [Open Questions](#open-questions)
21. [References](#references)

---

## Problem Statement

### The Challenge

ICN needs to:
1. **Reward infrastructure contributors** (compute, storage, bandwidth providers)
2. **Avoid speculation** (no tradeable tokens, no mining races)
3. **Stay true to mutual credit principles** (value comes from reciprocity, not scarcity)
4. **Enable economic activity** within the cooperative ecosystem

### Why This Matters

People ask: "How can I run an incentivized node?" and "How can I donate resources to the network?"

Traditional answers create problems:
- **Tokens** → speculation, early-adopter wealth concentration
- **Mining** → race to the bottom, environmental concerns
- **Pure donation** → unsustainable, no reciprocity

ICN needs a third way: **contribution accounting** that rewards real work without creating speculative assets.

---

## Design Philosophy

### Core Insight: Infrastructure is Labor

Just like members earn hours for providing services to each other, **nodes earn credits for providing infrastructure services to the network**.

```
┌─────────────────────────────────────────────────────────────────────┐
│                    LABOR = LABOR = LABOR                            │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│   Alice tutors Bob      ═══  Alice earns hours                      │
│   Carol sells tomatoes  ═══  Carol earns hours                      │
│   Dave runs a node      ═══  Dave earns hours  ← Infrastructure     │
│   Eve reviews proposals ═══  Eve earns hours                        │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

The network itself has a "treasury DID" that issues credits when nodes contribute resources.

### Why This Isn't Speculative

| Speculative Token | ICN Infrastructure Credits |
|-------------------|---------------------------|
| Tradeable on exchanges | Only usable within network |
| Price determined by market | Value = resource cost |
| Hoarding incentivized | Hoarding = wasted credits (demurrage) |
| Early adopters profit | All contributors equal |
| Deflationary/inflationary games | Balanced by usage |
| "Number go up" narrative | "How much did we contribute?" narrative |

**The key difference**: Credits represent actual resource contribution, not speculative value. You can't get rich by hoarding them—you can only use them to access network resources or trade for goods/services.

---

## Three-Tier Credit System

Credits have **graduated exchangeability** based on governance and trust.

```
┌─────────────────────────────────────────────────────────────────────┐
│                     CREDIT TIERS                                     │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  TIER 1: INTERNAL CREDITS (Default)                                 │
│  ├── Earned by: Contributing resources                              │
│  ├── Usable for: Network resources only                             │
│  ├── Transferable: Only within your cooperative                     │
│  └── Exchangeable: No                                               │
│                                                                      │
│  TIER 2: FEDERATED CREDITS (Earned)                                 │
│  ├── Earned by: Sustained contribution + trust threshold            │
│  ├── Usable for: Resources across federated cooperatives            │
│  ├── Transferable: Between federated coops                          │
│  └── Exchangeable: Coop-to-coop only (not individuals)              │
│                                                                      │
│  TIER 3: BRIDGE CREDITS (Governance-controlled)                     │
│  ├── Earned by: Governance proposal approval                        │
│  ├── Usable for: External exchange (fiat, other networks)           │
│  ├── Transferable: Yes, but with cooperative oversight              │
│  └── Exchangeable: Yes, through approved bridges                    │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### What Members Experience

| Tier | Member Experience |
|------|-------------------|
| **1: Internal** | "My credits work only in my coop's marketplace & infrastructure. I can pay for compute, buy from fellow members, trade services." |
| **2: Federated** | "Some of my credits can pay for services in partner coops. Our federation agreement sets the exchange rates. I can buy food from the Food Coop using credits I earned in Tech Coop." |
| **3: Bridge** | "With governance approval, I can convert some credits to external value (fiat, other networks). This is for interfacing with the outside economy when needed, not for speculation." |

**Progression is earned**: Tier 2 requires federation agreements between coops. Tier 3 requires demonstrated governance maturity and explicit proposals.

### Tier 1: Internal Credits

The default tier. Credits stay within a single cooperative.

```rust
pub struct InternalCredit {
    pub holder: Did,
    pub amount: u64,
    pub currency: String,  // e.g., "hours"
    pub cooperative: CoopId,
    pub earned_at: Timestamp,
    // Cannot leave this cooperative
}
```

**Use cases**:
- Pay for compute jobs within your coop
- Reserve storage quota
- Access bandwidth allocation

### Tier 2: Federated Credits

When cooperatives federate, they can agree to honor each other's credits.

```rust
pub struct FederationAgreement {
    pub coops: Vec<CoopId>,
    pub exchange_rates: HashMap<(Currency, Currency), f64>,
    pub settlement_period: Duration,  // e.g., monthly
    pub governance: FederationGovernance,
}
```

**Key constraint**: This is coop-to-coop, not individual speculation. The coops collectively govern exchange rates.

**Example**: Tech Coop and Food Coop federate
- 1 hour of tech support = 2 hours of food coop labor
- Settled monthly via mutual credit between coop treasuries

### Tier 3: Bridge Credits

For interfacing with the external economy. Requires governance approval and guardrails.

```rust
pub struct BridgeCredit {
    pub holder: Did,
    pub amount: u64,
    pub backing: BridgeBacking,
    pub restrictions: Vec<Restriction>,
    pub governance_approval: ProposalId,  // Must be approved!
}

pub enum BridgeBacking {
    // Backed by actual fiat held in cooperative accounts
    FiatReserve { currency: FiatCurrency, reserve_ratio: f64 },

    // Backed by real assets (equipment, property)
    AssetBacked { asset_registry: ContentHash },

    // Backed by future labor commitments
    LaborCommitment { hours: u64, skills: Vec<Skill> },

    // Backed by other crypto (for interop)
    CryptoBridge { chain: ChainId, contract: Address },
}
```

---

## Infrastructure as Labor

### Currency Model (v1)

**Default for v1**: All infrastructure contributions convert to the cooperative's base currency (typically "hours") at governance-defined rates.

```
┌─────────────────────────────────────────────────────────────────────┐
│  CONTRIBUTION → METRICS → CREDITS                                   │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  Dave's node provides:                                              │
│    • 100 CPU-hours                                                  │
│    • 50 GB-months storage                                           │
│    • 200 GB bandwidth                                               │
│                                                                      │
│  Protocol contract applies rates:                                   │
│    • 1 CPU-hour = 1.0 hours                                         │
│    • 1 GB-month = 0.1 hours                                         │
│    • 1 GB bandwidth = 0.02 hours                                    │
│                                                                      │
│  Dave receives: 100 + 5 + 4 = 109 hours                             │
│  (Metrics recorded for transparency; ledger sees only "hours")     │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

**Why single currency for v1**:
- Simpler mental model for members
- Reduces friction in internal marketplace
- Exchange rates are a governance knob, not a system constraint
- Metrics are still recorded for fairness audits

**Future (v2)**: Multi-currency coops may choose to keep infrastructure currencies distinct, using internal exchange pools. This requires the `exchange/v1` protocol contract.

### Equity Invariants

Infrastructure contribution rates are **political choices**, not neutral market prices. To prevent infrastructure from creating class hierarchies within coops, the following invariants should guide rate-setting:

**It must always be possible for someone with no hardware and no capital to:**
- Earn credits through human labor alone
- Attain governance power equal to any infrastructure contributor
- Access shared infrastructure indirectly via their coop/community
- Hold any elected or appointed role

**Coops should consider:**
- Capping infrastructure earnings as a percentage of total credit issuance
- Applying slightly higher demurrage to infrastructure-earned credits (optional)
- Establishing hardware lending/grant programs so contribution isn't wealth-gated
- Regularly reviewing the ratio of infrastructure rates to human labor rates

**The goal**: Infrastructure contribution should be rewarded fairly, but should not create a class of "infrastructure rentiers" who dominate the coop economy through capital ownership rather than labor.

### Resource Types

Infrastructure contributions are measured in concrete units:

| Resource Type | Unit of Measure | How Verified |
|---------------|-----------------|--------------|
| Compute | CPU-hours | Job completion proofs + peer attestation |
| Storage | GB-months | Replica health checks + peer attestation |
| Bandwidth | GB transferred | Peer attestations |
| Uptime | Node-hours | Heartbeat consensus |

### The Network Treasury

The network itself acts as a member that receives services:

```rust
pub const NETWORK_TREASURY_DID: &str = "did:icn:network:infrastructure";

// When Dave provides 10 hours of compute:
let entry = JournalEntryBuilder::new(network_treasury_did.clone())
    .debit(dave_did.clone(), "hours".into(), 10)      // Dave is OWED 10 hours
    .credit(network_treasury_did.clone(), "hours".into(), 10)  // Network OWES Dave
    .memo("Infrastructure: 10 CPU-hours provided")
    .build()?;
```

Now Dave's infrastructure credits are just hours—**spendable anywhere in the cooperative** for anything. Infrastructure capacity is just another listing type in the marketplace, priced in the same mutual credit system as tutoring, tomatoes, and childcare. Running a node is labor; the marketplace treats it equally.

---

## Human Labor Credit System

Human labor earning uses the same attestation infrastructure as infrastructure, but with different workflows.

### Labor Types

```rust
pub enum LaborContribution {
    /// Logged via coop/community work tracking
    OrganizationalLabor {
        org: OrgId,
        hours: u64,
        category: LaborCategory,
        supervisor_attestation: Option<Attestation>,
    },

    /// Peer-to-peer service delivery (marketplace)
    ServiceDelivery {
        listing: ListingId,
        recipient: Did,
        hours: u64,
        recipient_attestation: Attestation,
    },

    /// Community service (care work, mutual aid)
    CommunityService {
        community: CommunityId,
        description: String,
        hours: u64,
        beneficiary_attestations: Vec<Attestation>,
    },
}

pub enum LaborCategory {
    // Organizational roles
    Governance,      // Facilitation, coordination, administration
    Operations,      // Day-to-day coop work
    Development,     // Software, infrastructure
    Outreach,        // Community building, marketing

    // Service categories
    Education,       // Teaching, tutoring, training
    Care,            // Childcare, eldercare, health support
    Skilled,         // Trades, professional services
    Creative,        // Art, design, media
    Physical,        // Manual labor, agriculture
}
```

### Attestation Workflows

**For organizational labor**:
1. Member logs hours in coop/community system
2. Supervisor or coordinator attests (single attestation sufficient for orgs with designated roles)
3. Credits issued weekly/monthly per org policy

**For marketplace services**:
1. Listing defines terms
2. Service delivered
3. Recipient attests completion and quality
4. Credits transfer automatically

**For community service**:
1. Member claims hours with description
2. Beneficiaries attest (minimum 2 for claims > 10 hours)
3. Community reviews and approves
4. Credits issued from community treasury or via protocol contract

### Integration with CCL

Labor agreements can be codified in CCL contracts:

```json
{
  "name": "WeeklyLabor",
  "rules": [
    {
      "name": "log_hours",
      "params": ["member", "hours", "category"],
      "requires": [{"MemberOf": {"member": {"Var": "member"}, "org": "this"}}],
      "body": [
        {"LedgerTransfer": {
          "from": "org_treasury",
          "to": {"Var": "member"},
          "amount": {"Var": "hours"},
          "currency": "hours"
        }}
      ]
    }
  ]
}
```

### Governance Oversight

Each coop/community sets:
- Hourly rate equivalencies (if different from 1:1)
- Category-specific rates (e.g., hazardous work pays 1.5x)
- Maximum hours claimable per period
- Attestation requirements per labor type
- Appeals process for disputed hours

---

## Contribution Verification

### The Hard Problem

How do you prove a node actually contributed without trusted third parties?

### Peer Attestation (Recommended for ICN)

Fits ICN's trust model perfectly.

```rust
pub struct ContributionAttestation {
    pub contributor: Did,
    pub resource_type: ResourceType,
    pub amount: u64,
    pub period: (Timestamp, Timestamp),
    pub attesters: Vec<(Did, Signature)>,  // Peers who witnessed
    pub evidence: ContentHash,              // Proof (e.g., job results)
}

// Requires M-of-N attestations from trusted peers
// Weight attestations by trust score
fn validate_contribution(attestation: &ContributionAttestation) -> bool {
    let weighted_attestations: f64 = attestation.attesters
        .iter()
        .map(|(did, _)| trust_graph.compute_trust_score(did))
        .sum();

    weighted_attestations >= CONTRIBUTION_THRESHOLD  // e.g., 2.0
}
```

**Advantages**:
- Uses existing trust graph
- No special hardware or proofs required
- Socially verifiable

**Disadvantages**:
- Requires network of attesters
- Could be gamed by colluding nodes

### Sybil Resistance Model

**Important constraint**: "Peer attestation" does not mean "any DID can attest." In practice, attesters are constrained by:

- **Membership status**: Must be a member of the same coop/community or a federated org
- **Membership age**: New members may have reduced attestation weight or be ineligible to attest large contributions
- **Trust graph thresholds**: Attestations from untrusted or low-trust DIDs carry minimal weight
- **Organizational attestation**: Contributions above certain thresholds may require attestation from an organizational DID (coop, community, or household), not just individuals

This social anchoring is the first line of defense against Sybil attacks. Identity issuance and membership onboarding are the gatekeeping mechanisms; attestation validates within that trust boundary.

### Attestation Eligibility Requirements

To attest contributions above trivial thresholds, an attester must satisfy:

| Constraint | Requirement | Rationale |
|------------|-------------|-----------|
| **Membership age** | ≥ 90 days in good standing | New members cannot immediately vouch |
| **Trust score** | ≥ 0.3 | Isolated or distrusted members cannot attest |
| **Attestation budget** | Max 10 attestations per period per attester | Prevents attestation factories |
| **Non-reciprocity** | Cannot attest someone who attested you in same period | Breaks simple collusion rings |
| **Org anchor** | For claims > 500 credits: requires ≥1 organizational attestation | Large claims need institutional backing |

### Organizational Attestation

For significant contributions, at least one attester must be an **organizational DID**:

```rust
pub enum AttesterType {
    Individual(Did),
    Organization(OrgDid),  // Coop, community, or household
}

pub fn validate_large_contribution(claim: &Claim, attestations: &[Attestation]) -> bool {
    if claim.amount > LARGE_CLAIM_THRESHOLD {
        // Requires at least one org attestation
        let has_org = attestations.iter().any(|a| matches!(a.attester_type, AttesterType::Organization(_)));
        if !has_org {
            return false;
        }
    }
    // Normal weighted validation
    validate_attestation_weights(attestations)
}
```

### Trust Graph Depth

Attestations are weighted not just by trust score, but by **graph depth from organizational anchors**:

```
Org DID (coop/community) ─── depth 0 (weight: 1.0)
    │
    ├── Long-standing member ─── depth 1 (weight: 0.9)
    │       │
    │       └── Newer member vouched by them ─── depth 2 (weight: 0.7)
    │
    └── Another path...
```

Members with no path to an organizational anchor have zero attestation weight.

### Attestation Fraud Prevention

#### Attack Vectors

| Attack | Description |
|--------|-------------|
| **Collusion ring** | Small group vouches for each other's fake contributions |
| **Attestation factory** | One member attests everything without verification |
| **Org capture** | Corrupt org leadership attests fake member contributions |
| **Time inflation** | Real work, exaggerated hours |

#### Countermeasures

| Measure | Implementation |
|---------|----------------|
| **Attestation budget** | Max 10 per member per period |
| **Non-reciprocity** | Can't attest someone who attested you this period |
| **Org attestation for large claims** | Claims > 500 require org-level DID |
| **Membership age threshold** | New members can't attest for 90 days |
| **Random audits** | X% of claims randomly selected for deeper review |
| **Attestation accuracy tracking** | Historical accuracy affects future weight |

#### Penalties

| Offense | Penalty |
|---------|---------|
| First fraudulent attestation | Attestation privileges suspended 30 days |
| Second offense | Suspended 180 days, trust score penalty |
| Third offense | Membership review, potential expulsion |
| Organized fraud (multiple participants) | All participants face membership review |

#### Detection

```rust
pub struct AttestationAudit {
    /// Check for reciprocal attestation patterns
    pub fn detect_reciprocity_clusters(&self) -> Vec<Cluster>;

    /// Check for attesters who approve everything
    pub fn detect_rubber_stampers(&self) -> Vec<Did>;

    /// Check for contribution patterns that don't match observed capacity
    pub fn detect_implausible_claims(&self) -> Vec<Claim>;

    /// Compare claimed vs measured (for infra)
    pub fn detect_inflation(&self) -> Vec<DiscrepancyReport>;
}
```

### Verification Tiers Based on Value

Combine approaches based on contribution size:

| Contribution Value | Verification Method |
|-------------------|---------------------|
| Small (<100 credits) | Peer attestation only |
| Medium (100-1000) | Attestation + spot checks |
| Large (>1000) | Full cryptographic proof + org attestation |

### Provenance Scope

Not all credits need full provenance tracking:

| Credit Type | Provenance Requirement | Rationale |
|-------------|----------------------|-----------|
| **Tier 1 (Internal)** | Compressed/batched | Storage efficiency; never leaves coop |
| **Tier 2 (Federated)** | Origin + last transfer | Enough for dispute resolution |
| **Tier 3 (Bridge candidate)** | Full chain | Required for external auditability |

**Implementation note**: Credits start with compressed provenance. When a member requests bridge-out eligibility, the system reconstructs or requires full provenance for the specific units being bridged. Credits that have been transferred internally may not be reconstructible and thus remain Tier 1/2 only.

**Storage philosophy**: ICN will never store full provenance for all credits indefinitely. Full-chain provenance is an opt-in requirement for bridgeable units only.

For members who may want to bridge credits in the future:
- They may retain their own detailed provenance proofs (encrypted, off-ledger)
- These proofs can be presented when bridge eligibility is requested
- The network validates but does not permanently store the full chain

For coop-level auditing:
- Coops choose how far back they keep detailed provenance
- The network does not enforce a global retention horizon
- Archived provenance may be compressed or summarized after a governance-set period

This keeps the ledger from becoming a forever-archive of every micro-transfer while still enabling auditability where it matters.

---

## Fuel System

Credits are **claims on value**. Fuel is **permission to act**. Both are needed.

### Why Fuel Exists

Fuel prevents:
- **Spam**: Can't flood the network with garbage
- **Tragedy of the commons**: Shared resources need rate limiting
- **Free riding**: Using without contributing

### Fuel Is Not a Token

```
┌─────────────────────────────────────────────────────────────────────┐
│                                                                      │
│  TRADITIONAL GAS                    ICN FUEL                        │
│  ────────────────                   ────────                        │
│                                                                      │
│  • You buy gas tokens               • Network has fuel capacity     │
│  • Pay per transaction              • You have fuel allowance       │
│  • Gas goes to miners               • Fuel regenerates over time    │
│  • Speculation on gas price         • No speculation possible       │
│  • Rich users outbid poor           • Fair allocation by trust/need │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### Unified Fuel Model

All operations draw from a single regenerating fuel pool—both network operations (posting, trading, voting) and compute execution.

```rust
pub struct FuelAccount {
    pub did: Did,
    pub available: u64,
    pub reserved: u64,   // Fuel set aside for pending compute jobs
    pub max: u64,
    pub regen_rate: u64,  // Fuel per hour
    pub last_update: Timestamp,
}

impl FuelAccount {
    pub fn usable(&self) -> u64 {
        self.available - self.reserved
    }

    /// Reserve fuel for a compute job (returns unused when job completes)
    pub fn reserve(&mut self, amount: u64) -> Result<(), FuelError> {
        self.regenerate();
        if self.usable() < amount {
            return Err(FuelError::InsufficientFuel);
        }
        self.reserved += amount;
        Ok(())
    }

    /// Consume fuel for an operation
    pub fn consume(&mut self, amount: u64) -> Result<(), FuelError> {
        self.regenerate();
        if self.available < amount {
            return Err(FuelError::Exhausted);
        }
        self.available -= amount;
        Ok(())
    }

    /// Lazy regeneration on access
    fn regenerate(&mut self) {
        let now = now_timestamp();
        let elapsed_hours = (now - self.last_update) / 3600;
        let regen = elapsed_hours * self.regen_rate;
        self.available = (self.available + regen).min(self.max);
        self.last_update = now;
    }
}
```

### Fuel Pools at Every Layer

```
┌─────────────────────────────────────────────────────────────────────┐
│  LAYER          │ FUEL POOL         │ USED FOR                      │
│  ═══════════════╪═══════════════════╪══════════════════════════════ │
│  Network        │ Global capacity   │ Cross-federation operations   │
│  Federation     │ Regional capacity │ Cross-coop operations         │
│  Cooperative    │ Local capacity    │ Internal operations           │
│  Member         │ Personal allowance│ Individual activity           │
└─────────────────────────────────────────────────────────────────────┘
```

### Fuel Allowance Calculation

```rust
pub const MINIMUM_CIVIC_FUEL: u64 = 50;
pub const MINIMUM_REGEN_RATE: u64 = 10;  // Per day

impl FuelAllowance {
    pub fn calculate(
        did: &Did,
        trust_score: f64,
        contribution_history: &ContributionHistory,
    ) -> Self {
        let base = 100;  // Everyone gets this
        let trust_bonus = (trust_score * 500.0) as u64;
        let contribution_bonus = contribution_history.total_contributed / 10;

        let calculated_max = base + trust_bonus + contribution_bonus;
        let calculated_regen = calculated_max / 24;  // Fully regenerate in 24 hours

        FuelAllowance {
            did: did.clone(),
            available: calculated_max.max(MINIMUM_CIVIC_FUEL),
            reserved: 0,
            max: calculated_max.max(MINIMUM_CIVIC_FUEL),
            regen_rate: calculated_regen.max(MINIMUM_REGEN_RATE),
            last_update: now()
        }
    }
}
```

### What Costs Fuel

| Operation | Fuel Cost | Rationale |
|-----------|-----------|-----------|
| Publish gossip message | 1 | Prevent spam |
| Ledger transaction | 10 | Rate limit transfers |
| Create proposal | 50 | Prevent proposal spam |
| Cast vote | 5 | Encourage participation |
| Submit compute job | Variable | Reserved upfront, unused returned |
| Marketplace listing | 10 | Quality over quantity |
| Execute trade | 20 | Meaningful transactions |

### Fuel Guarantees

To prevent fuel from becoming a barrier to democratic participation, ICN makes the following guarantees:

#### Minimum Civic Allowance

Even members with zero credits and minimal trust receive a base fuel allowance sufficient for:
- Casting votes on proposals they're eligible for
- Creating at least one proposal per governance period
- Sending basic messages to their coop/community
- Viewing their balances and contribution history

#### Guaranteed Minimum Operations

The following operations can **never** be blocked due to fuel exhaustion:

| Operation | Guaranteed Minimum | Rationale |
|-----------|-------------------|-----------|
| Cast vote on eligible proposals | 1 per proposal | Democratic participation is non-negotiable |
| Create proposal | 1 per governance period | Right to raise issues |
| Send direct message | 5 per day | Basic communication |
| View own balances/history | Unlimited | Transparency is not a privilege |
| Respond to disputes involving self | Unlimited | Right to defend oneself |

#### No Permanent Lockout

Fuel always regenerates. A member who exhausts their fuel today will have usable fuel tomorrow. There is no mechanism by which someone can be permanently excluded from participation.

#### Credits ≠ Fuel

Having many credits does not grant more fuel. Having zero credits does not prevent fuel regeneration. These are independent resources:
- **Credits** = long-term economic power (what you've earned and can spend)
- **Fuel** = short-term activity budget (permission to act, regenerates daily)

#### Emergency Provisions

Coops may establish emergency fuel grants for members facing unusual circumstances (e.g., coordinating disaster response). This is a governance decision, not a protocol feature.

### Fuel Regeneration Mechanics

**When regeneration happens**: Fuel is computed **lazily on read**. When a member attempts an action, the system:
1. Calculates time since `last_update`
2. Adds `elapsed_hours × regen_rate` to `available`
3. Caps at `min(member_max, coop_pool_remaining, federation_pool_remaining)`
4. Updates `last_update` to now

**Pool exhaustion**: When a pool is exhausted:
- **Member pool empty**: Member waits for regeneration (hours, not days)
- **Coop pool empty**: All coop members are throttled; governance should investigate
- **Federation pool empty**: Cross-coop operations blocked; federation governance decides priority
- **Network pool empty**: Cross-federation operations blocked (should be rare)

**Priority during scarcity**: Coops define priority policies (e.g., "critical operations first", "equal degradation", "contribution-weighted"). This is a governance decision, not hardcoded.

### Compute Job Fuel Reservation

When submitting a compute job, fuel is reserved upfront and unused fuel is returned:

```rust
// When job is submitted
impl ComputeManager {
    pub async fn submit_job(&self, job: ComputeJob) -> Result<JobId> {
        // Reserve fuel from submitter's account
        let fuel_account = self.get_fuel_account(&job.submitter)?;
        fuel_account.reserve(job.fuel_budget)?;

        // Submit job with reserved budget
        let job_id = self.executor.submit(job)?;
        Ok(job_id)
    }
}

// When job completes
impl ComputeManager {
    pub async fn complete_job(&self, job_id: JobId, result: JobResult) -> Result<()> {
        let job = self.get_job(&job_id)?;
        let fuel_used = result.fuel_consumed;

        let fuel_account = self.get_fuel_account(&job.submitter)?;
        fuel_account.unreserve(job.fuel_budget);   // Release reservation
        fuel_account.consume(fuel_used)?;           // Actually consume what was used

        Ok(())
    }
}
```

### Organizational Fuel

Organizations (coops, communities, households) are first-class fuel consumers.

#### Organizational Fuel Pool

```rust
pub struct OrganizationalFuel {
    pub org: OrgId,
    pub available: u64,
    pub reserved: u64,
    pub max: u64,
    pub regen_rate: u64,
    pub last_update: Timestamp,
}
```

#### How Organizations Earn Fuel

| Source | Mechanism |
|--------|-----------|
| **Member contributions routed to org** | Each hour of infra credit routed to org adds to org's fuel bonus |
| **Org-owned infrastructure** | Coops running their own nodes earn fuel directly |
| **Member count** | Base fuel scales with active membership |
| **Federation membership** | Federated orgs receive federation fuel allocations |

#### Organizational Fuel Calculation

```rust
impl OrganizationalFuel {
    pub fn calculate(org: &Organization) -> Self {
        let member_base = org.active_members().len() as u64 * 50;
        let infra_bonus = org.routed_contributions_this_period() / 10;
        let federation_bonus = if org.is_federated() { 500 } else { 0 };

        let max = member_base + infra_bonus + federation_bonus;
        let regen_rate = max / 24;

        OrganizationalFuel { max, regen_rate, /* ... */ }
    }
}
```

#### What Organizations Use Fuel For

| Operation | Fuel Cost | Notes |
|-----------|-----------|-------|
| Publish org-wide announcements | 10 | Per message |
| Run gateway services | 1/hour | Continuous cost |
| Execute org governance actions | 50 | Per proposal |
| Send federation communications | 20 | Cross-org messaging |
| Host compute jobs on behalf of members | Variable | Reserved like member jobs |

#### Rate Limits

Even organizations have limits:
- Maximum burst operations per hour
- Fuel exhaustion blocks non-critical operations
- Critical operations (governance, emergency comms) have separate reserve pool

---

## Organizational Structures

ICN supports multiple forms of collective organization, recognizing that human needs span both **civic life** (belonging, care, stewardship) and **economic life** (livelihood, trade, production).

### Two Pillars: Communities and Cooperatives

| Aspect | Community | Cooperative |
|--------|-----------|-------------|
| **Purpose** | Civic / Public service | Economic / Livelihood |
| **Focus** | Mutual aid, care, stewardship, advocacy | Production, trade, services |
| **Currency** | Optional (gift, time bank, or own credits) | Required (coop currency) |
| **Examples** | Neighborhoods, mutual aid networks, faith groups, advocacy orgs | Worker coops, consumer coops, producer coops |
| **Marketplace** | Optional | Core feature |

**Key insight**: These are complementary, not competing. A healthy ecosystem needs both. Individuals can (and should) be members of both communities and cooperatives.

### The Organizational Spectrum

```
INFORMAL ◄────────────────────────────────────────────────────► FORMAL

┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐
│INDIVIDUAL│ │HOUSEHOLD │ │COMMUNITY │ │COOPERATIVE│ │FEDERATION│
│          │ │          │ │          │ │          │ │          │
│ 1 person │ │ Family/  │ │ Civic/   │ │ Economic │ │ Multiple │
│ 1+ device│ │ friends  │ │ public   │ │ engine   │ │ orgs     │
│          │ │          │ │ service  │ │          │ │          │
└──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────┘
```

### Community Entity

```rust
pub struct Community {
    pub id: CommunityId,
    pub name: String,
    pub description: String,
    pub community_type: CommunityType,
    pub members: Vec<CommunityMember>,
    pub governance: CommunityGovernance,
    pub fuel_pool: FuelPool,
    pub treasury: Option<CommunityTreasury>,
    pub coop_partnerships: Vec<CoopPartnership>,
    pub federation: Option<FederationId>,
}

pub enum CommunityType {
    // Geographic
    Neighborhood { location: Location },
    Bioregion { region: Region },

    // Identity
    Cultural { culture: String },
    Faith { tradition: String },
    Affinity { affinity: String },

    // Purpose
    MutualAid { focus: String },
    EnvironmentalStewardship { scope: String },
    Advocacy { cause: String },
    DisasterResponse { region: Region },

    // Commons
    CommonsManagement { resource: String },
    DigitalCommons { domain: String },
}
```

### Community-Cooperative Partnerships

Communities and cooperatives can form partnerships:

```rust
pub enum PartnershipType {
    /// Community uses coop's economic infrastructure
    EconomicServices {
        marketplace_access: bool,
        currency_access: bool
    },

    /// Coop provides services to community
    ServiceProvider { services: Vec<ServiceAgreement> },

    /// Community provides civic services to coop members
    CivicServices { services: Vec<CivicService> },

    /// Resource sharing
    ResourceSharing {
        infra_for_credits: bool,
        shared_spaces: Vec<SpaceAgreement>
    },

    /// Full integration
    Integrated {
        auto_membership: bool,
        benefit_sharing: BenefitSharing
    },
}

pub enum CivicService {
    MutualAid,
    DisasterResponse,
    ElderCare,
    ChildCare,
    CommunityGarden,
    ToolLibrary,
    SkillSharing,
    Mediation,
}
```

### Households

```rust
pub struct Household {
    pub id: HouseholdId,
    pub name: String,
    pub members: Vec<HouseholdMember>,
    pub nodes: Vec<NodeId>,
    pub benefit_sharing: BenefitSharing,
    pub fuel_pool: FuelPool,  // Shared among household members
}

pub enum BenefitSharing {
    Single { recipient: Did },
    Equal,
    ProportionalToContribution,
    Custom { shares: HashMap<Did, f64> },
}
```

### Multi-Membership

Individuals can belong to multiple organizations simultaneously:

```rust
pub struct IndividualMemberships {
    pub did: Did,
    pub household: Option<HouseholdId>,
    pub communities: Vec<CommunityMembership>,   // Zero or more
    pub cooperatives: Vec<CoopMembership>,       // Zero or more
}
```

### Contribution Routing

When contributing infrastructure, individuals specify how to split between their organizations:

```rust
pub struct ContributionRouting {
    pub allocations: Vec<ContributionAllocation>,
}

pub struct ContributionAllocation {
    pub destination: ContributionDestination,
    pub percentage: f64,
}

pub enum ContributionDestination {
    Personal,
    GlobalCommons,
    Cooperative(CoopId),
    Community {
        community_id: CommunityId,
        handling: CommunityContributionHandling
    },
    Federation(FederationId),
}

pub enum CommunityContributionHandling {
    EarnCommunityCurrency,
    CommunityTreasury,
    PartnerCoop(CoopId),
    Gift,  // No credits earned
}
```

### Routing Exploitation Prevention

#### The Attack

A wealthy member could:
1. Create a "household" they control
2. Route 100% of contributions there
3. Household routes to a coop treasury they influence
4. Use org-level privileges to gain disproportionate influence

#### Countermeasures

| Measure | Implementation |
|---------|----------------|
| **Real membership requirement** | Households need 2+ real DIDs with independent histories |
| **Routing audit** | All routing is publicly visible in ledger |
| **Pass-through ceilings** | Max % of org funds can come from single source |
| **Velocity limits** | Rapid routing changes trigger review |
| **Beneficial ownership disclosure** | Orgs must disclose controlling members |

#### Detection

```rust
pub struct RoutingAudit {
    /// Detect single-source dependency
    pub fn detect_concentration(&self, org: OrgId) -> ConcentrationReport;

    /// Detect circular routing patterns
    pub fn detect_loops(&self) -> Vec<RoutingLoop>;

    /// Detect rapid routing changes
    pub fn detect_velocity_anomalies(&self) -> Vec<VelocityAlert>;

    /// Check household legitimacy
    pub fn verify_household_independence(&self, household: HouseholdId) -> bool;
}
```

#### Policy Recommendations

| Policy | Suggested Default |
|--------|-------------------|
| Max single-source contribution to org | 25% |
| Min household members | 2 |
| Min member independence (shared history < X%) | 50% |
| Routing change cooldown | 30 days |

### Community Currency Options

Communities choose their currency model:

| Model | Description | Best For |
|-------|-------------|----------|
| **Gift Economy** | No credits tracked, pure reciprocity | Small, high-trust groups |
| **Time Bank** | 1 hour = 1 hour (radical equality) | Egalitarian communities |
| **Community Credits** | Own currency like coop credits | Larger, diverse communities |
| **Partner Currency** | Use partner coop's currency | Close coop alignment |

### Incentive Gradient

More formalization = more benefits, but informal participation is welcomed:

| Destination | Fuel Access | Credit Earning | Trade Access | Governance |
|-------------|-------------|----------------|--------------|------------|
| Personal | 10% | Minimal | Limited | None |
| Global Commons | 30% | Federation Credits | Network-wide | None |
| Community | 50% | Community credits | Local + partners | Local |
| Cooperative | 100% | Full coop credits | Coop + federation | Full |

### Example: Alex's Multi-Membership

Alex lives in a four-person household, belongs to a neighborhood community, and works in a tech worker coop. Here's how their contributions flow:

**Contribution routing**:
```
Alex's infrastructure contribution: 100 CPU-hours/month
├── 50% → Tech Workers Coop (livelihood)     → 50 hours as tech-hours
├── 30% → Smith Household (shared costs)     → 30 hours to household pool
└── 20% → Oakwood Mutual Aid (community)     → 20 hours as oakwood-credits
```

**Fuel calculation**: Alex's fuel allowance is based on their **total** contribution (100 hours worth), plus trust bonuses. The credits flow according to routing, but fuel reflects overall network contribution.

**Spending**:
- Alex uses tech-hours to buy services from fellow coop members
- The household pool covers shared subscriptions and childcare exchanges
- Oakwood-credits go to the community garden and tool library

**Governance**: Alex votes in:
- Tech Workers Coop (full member)
- Oakwood Community (full member)
- Their household doesn't have formal governance (decisions made at the kitchen table)

**Federation access**: Because Tech Workers Coop is in the PNW Federation, Alex can also spend tech-hours at Food Coop (federated) at the agreed exchange rate.

---

## Internal Marketplace

Credits aren't just for infrastructure—they enable a full internal economy.

### Listing Types

```rust
pub struct Listing {
    pub id: ListingId,
    pub seller: Did,
    pub cooperative: CoopId,
    pub listing_type: ListingType,
    pub title: String,
    pub description: String,
    pub price: Price,
    pub availability: Availability,
    pub trust_requirement: Option<f64>,
    pub federation_scope: FederationScope,
}

pub enum ListingType {
    Service { category: ServiceCategory, duration: Option<Duration> },
    Good { category: GoodCategory, condition: Condition },
    Digital { content_hash: ContentHash, license: License },
    Infrastructure { resource_type: ResourceType, capacity: u64, duration: Duration },
    Subscription { service: Box<ListingType>, period: Duration },
}
```

### Price in Multiple Currencies

```rust
pub struct Price {
    pub amount: u64,
    pub currency: String,  // "hours", "compute-hours", "tomatoes", etc.
    pub negotiable: bool,
}
```

### Trade Proposals

```rust
pub struct TradeProposal {
    pub id: TradeId,
    pub proposer: Did,
    pub counterparty: Did,
    pub offer: Vec<TradeItem>,
    pub request: Vec<TradeItem>,
    pub expires_at: Timestamp,
    pub status: TradeStatus,
}
```

### Fuel + Credits Together

```rust
pub async fn execute_trade(&self, trade: &Trade) -> Result<TradeReceipt> {
    // Step 1: Check and consume FUEL (permission)
    let fuel_cost = self.fuel_system.get_trade_cost(&trade)?;
    self.fuel_system.consume(&trade.buyer, fuel_cost)?;

    // Step 2: Check and transfer CREDITS (value)
    self.ledger.transfer(&trade.buyer, &trade.seller, trade.amount)?;

    Ok(TradeReceipt { /* ... */ })
}
```

---

## Anti-Speculation Mechanisms

### 1. Demurrage (Circulation Charge)

Credits lose value over time if not used. Demurrage may also be called a **"circulation charge"**—a small cost for holding credits idle, encouraging them to flow through the economy rather than accumulate.

```rust
pub struct DemurragePolicy {
    pub rate: f64,           // e.g., 5% per year
    pub period: Duration,    // Applied monthly
    pub exemptions: Vec<ExemptionRule>,
}
```

**Effect**: Encourages circulation, discourages hoarding.

#### Demurrage Tradeoffs

Demurrage is the most politically sensitive economic parameter. It must be understood as a tradeoff, not a pure good.

| If demurrage is too weak | If demurrage is too strong |
|--------------------------|---------------------------|
| Hoarding begins | Resentment toward the system |
| Store-of-value accumulation | People flee to bridgeable variants |
| Proto-capitalist dynamics | Shadow currencies emerge |
| Credit stratification | Members feel punished for saving |
| Circulation slows | Loophole-seeking behavior |

#### Finding Balance

**Recommended starting point**: 2-5% annually, applied monthly

**Adjustments based on**:
- Circulation velocity (if high, lower demurrage; if low, raise it)
- Member satisfaction surveys
- Credit concentration metrics
- Hoarding detection (large idle balances)

#### Demurrage Governance Layers

Demurrage is **not** a single global policy. It operates at multiple levels:

| Level | Sets | Constraints |
|-------|------|-------------|
| **Network** | Maximum allowed rate, minimum review cadence | e.g., "No coop may set demurrage > 20%/year" |
| **Federation** | Recommended defaults for member coops | e.g., "PNW Federation recommends 5%/year" |
| **Cooperative** | Actual policy via `demurrage/v1` contract | Must stay within network constraints |

#### Exemptions (Required)

| Exemption | Rationale |
|-----------|-----------|
| **Emergency reserves** | Safety funds must not decay |
| **Disaster response pools** | Critical infrastructure |
| **Committed liquidity** | Already serving circulation |
| **Short-term savings (< 3 months)** | Normal cash flow management |
| **Hardship cases** | Governance-approved exemptions |

#### Exemptions (Optional, Governance Choice)

| Exemption | Consideration |
|-----------|---------------|
| New member grace period | Helps onboarding |
| Seasonal worker buffers | Accounts for uneven earning |
| Long-term project reserves | Saving for equipment, property |

#### Exemption Examples

Demurrage should penalize idle hoarding, not structural precarity. Coops MAY exempt:

| Exemption | Rationale |
|-----------|-----------|
| **Disability flag** | Members with reduced capacity shouldn't be penalized |
| **Parental leave** | Temporary absence for caregiving |
| **Medical crisis** | Illness, hospitalization, recovery |
| **Seasonal workers** | Some work is inherently cyclical |
| **New members** | Grace period (e.g., first 6 months) |

**What demurrage never applies to**:
- Governance-designated safety reserves (e.g., emergency funds, disaster response pools)
- Community treasuries with explicit mandates for accumulation (e.g., saving for a building purchase)
- Credits actively committed to liquidity pools

These exemptions require explicit governance designation. The default is that all idle credits are subject to demurrage.

#### Transparency Requirement

Demurrage rates, exemptions, and collected fees must be publicly visible:
- How much demurrage collected this period
- Where it went (burned, redistributed, treasury)
- Who received exemptions

### 2. Contribution-Locked Exchange

You can only bridge credits you've earned, not bought:

```rust
pub struct CreditProvenance {
    pub original_contributor: Did,
    pub contribution_type: ContributionType,
    pub earned_at: Timestamp,
    pub transfer_count: u32,
}

// Rule: Only credits with transfer_count == 0 can be bridged to fiat
fn can_bridge_to_fiat(credit: &Credit) -> bool {
    credit.provenance.transfer_count == 0 &&
    credit.provenance.original_contributor == credit.holder
}
```

**Effect**: Speculators can't buy low and sell high to fiat. Only original contributors can cash out.

**Important implication**: Once you circulate your credits inside the cooperative economy, that value is locked to the commons; bridgeability is only for direct earned credits.

### 3. Cooperative Approval for Large Exchanges

```rust
pub async fn request_bridge_out(
    requester: Did,
    amount: u64,
    destination: BridgeDestination,
) -> Result<BridgeRequest> {
    if amount > GOVERNANCE_THRESHOLD {
        let proposal = create_bridge_proposal(requester, amount, destination)?;
        return Err(Error::RequiresGovernanceApproval(proposal.id));
    }
    execute_bridge(requester, amount, destination)
}
```

### 4. Exchange Rate Anchoring

Instead of market-determined prices, anchor to real costs:

```rust
pub struct ExchangeRatePolicy {
    pub compute_anchor: FiatAmount,  // e.g., $0.05 USD per CPU-hour
    pub storage_anchor: FiatAmount,  // e.g., $0.02 USD per GB-month
    pub last_updated: Timestamp,
    pub update_governance: ProposalId,
}
```

### 5. Bridge Restrictions

```rust
pub struct BridgeRestrictions {
    pub outflow_limit_per_period: u64,
    pub holding_period: Duration,
    pub governance_threshold: u64,
    pub churn_penalty: f64,
    pub identity_requirement: IdentityRequirement,
}
```

---

## Credit Issuance Principles

### Who Can Issue Credits

Credits enter the system through:

| Source | Mechanism | Governance |
|--------|-----------|------------|
| **Infrastructure contribution** | Protocol contract calculates from verified metrics | Network-level formula |
| **Human labor** | Attestation workflow + org approval | Coop/community policy |
| **Bridge-in** | External value deposited | Bridge contract terms |
| **Governance grant** | Explicit proposal + vote | Coop governance |

### What Cannot Issue Credits

- **Self-attestation alone** (requires peer/org verification)
- **Unverified claims** (must pass attestation threshold)
- **Protocol exploits** (detected and penalized)
- **Off-ledger agreements** (if not recorded, doesn't exist)

### Issuance Rate Governance

Each coop sets:
- Maximum credits issuable per period
- Ratio of infrastructure vs. labor credits
- Bridge-in limits
- Emergency issuance procedures

---

## Anti-Capture Principles

### Governance Capture Prevention

| Risk | Mitigation |
|------|------------|
| **Wealthy members dominate** | Voting power capped; 1-member-1-vote default |
| **Infrastructure whales** | Infra earnings capped as % of total issuance |
| **Bridge arbitrage** | Only original earner can bridge; governance approval for large amounts |
| **Org treasury capture** | Multi-sig requirements; transparency mandates |

### Economic Capture Prevention

| Risk | Mitigation |
|------|------------|
| **Credit hoarding** | Demurrage penalizes idle balances |
| **Market manipulation** | No external market; internal rates governance-set |
| **Liquidity drain** | Outflow limits; holding periods |
| **Sybil accumulation** | Attestation requires real membership + trust |

---

## Dispute Resolution

### Dispute Types

| Type | Examples | Resolution Path |
|------|----------|-----------------|
| **Contribution disputes** | "They didn't do the work they claimed" | Attestation audit → org mediation → governance |
| **Trade disputes** | "Service wasn't delivered as described" | Escrow release → mediation → governance |
| **Attestation disputes** | "False attestation harmed me" | Fraud investigation → penalties |
| **Membership disputes** | "Wrongful expulsion" | Appeals process → federation arbitration |

### Resolution Process

```rust
pub enum DisputeStatus {
    Filed { filed_at: Timestamp, filer: Did },
    InMediation { mediator: Did, started_at: Timestamp },
    Escalated { proposal_id: ProposalId, escalation_reason: String, escalated_at: Timestamp },
    Resolved { mediator: Did, outcome: DisputeOutcome, resolved_at: Timestamp },
}

pub enum DisputeOutcome {
    Upheld,           // Filer wins
    Rejected,         // Respondent wins
    PartialAdjustment { amount: i64, currency: String },
    VoidTransaction,  // Undo the disputed action
}
```

### Escalation to Governance

When disputes cannot be resolved through mediation:
- Value exceeds mediator authority
- Mediator conflict of interest
- Disputed party rejects mediator decision
- Involves community-wide policy

Escalated disputes become governance proposals for community vote.

---

## Protocol Contracts

Economic rules are defined in CCL, not hardcoded. This enables:
- **Governance**: Communities can update rules through proposals
- **Customization**: Coops can extend or replace protocols
- **Auditability**: Everyone can read the rules
- **Interoperability**: Coops using same protocol are compatible

### Network Standard Contracts

```
icn://protocol/infrastructure-credit/v1    # Credit calculation
icn://protocol/fuel-allocation/v1          # Fuel allowance formula
icn://protocol/exchange/v1                 # AMM exchange pools
icn://protocol/demurrage/v1                # Credit decay rules
icn://protocol/membership/v1               # Coop membership rules
```

### Example: Infrastructure Credit Protocol

```json
{
  "name": "InfrastructureCreditProtocol",
  "version": "1.0.0",
  "state_vars": [
    {"name": "compute_rate", "initial_value": {"Int": 100}},
    {"name": "storage_rate", "initial_value": {"Int": 10}},
    {"name": "bandwidth_rate", "initial_value": {"Int": 5}}
  ],
  "rules": [
    {
      "name": "calculate_credits",
      "params": ["compute_hours", "storage_gb", "bandwidth_gb"],
      "body": [
        {"Return": {"value": {"BinOp": {
          "op": "Add",
          "left": {"BinOp": {"op": "Mul", "left": {"Var": "compute_hours"}, "right": {"Var": "compute_rate"}}},
          "right": {"BinOp": {"op": "Add",
            "left": {"BinOp": {"op": "Mul", "left": {"Var": "storage_gb"}, "right": {"Var": "storage_rate"}}},
            "right": {"BinOp": {"op": "Mul", "left": {"Var": "bandwidth_gb"}, "right": {"Var": "bandwidth_rate"}}}
          }}
        }}}}
      ]
    },
    {
      "name": "update_rates",
      "params": ["new_compute", "new_storage", "new_bandwidth"],
      "requires": [{"Comment": "Must be called via governance proposal"}],
      "body": [
        {"Assign": {"var": "compute_rate", "value": {"Var": "new_compute"}}},
        {"Assign": {"var": "storage_rate", "value": {"Var": "new_storage"}}},
        {"Assign": {"var": "bandwidth_rate", "value": {"Var": "new_bandwidth"}}}
      ]
    }
  ]
}
```

### Protocol Adoption

Coops can:
1. **Adopt** protocol contracts as-is
2. **Extend** with custom rules
3. **Create** entirely custom contracts
4. **Require** protocol compliance for federation membership

---

## Exchange Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                                                                      │
│                        ICN INTERNAL                                  │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐             │
│  │   Coop A    │◄──►│   Coop B    │◄──►│   Coop C    │             │
│  │  (internal) │    │  (internal) │    │  (internal) │             │
│  └─────────────┘    └─────────────┘    └─────────────┘             │
│         │                  │                  │                      │
│         └──────────────────┼──────────────────┘                     │
│                            │                                         │
│                   ┌────────▼────────┐                               │
│                   │   Federation    │  ◄── Tier 2                   │
│                   │     Ledger      │      (Coop-to-coop)           │
│                   └────────┬────────┘                               │
│                            │                                         │
│ ═══════════════════════════╪════════════════════════════════════════│
│                            │                                         │
│                   ┌────────▼────────┐                               │
│                   │  Bridge Layer   │  ◄── Tier 3                   │
│                   │  (Governed)     │      (Requires approval)      │
│                   └────────┬────────┘                               │
│                            │                                         │
└────────────────────────────┼────────────────────────────────────────┘
                             │
              ┌──────────────┼──────────────┐
              │              │              │
       ┌──────▼──────┐ ┌─────▼─────┐ ┌─────▼─────┐
       │    Fiat     │ │  Crypto   │ │  Other    │
       │   (Banks)   │ │ (Bridges) │ │ Networks  │
       └─────────────┘ └───────────┘ └───────────┘
```

### Exchange Manager

```rust
pub struct ExchangeManager {
    ledger: Arc<RwLock<Ledger>>,
    trust_graph: Arc<RwLock<TrustGraph>>,
    governance: Arc<GovernanceManager>,
    federation_agreements: HashMap<(CoopId, CoopId), FederationAgreement>,
    bridge_config: BridgeConfig,
}

impl ExchangeManager {
    /// Internal transfer (Tier 1) - always allowed within coop
    pub async fn internal_transfer(&self, from: Did, to: Did, amount: u64, currency: Currency) -> Result<TransferReceipt>;

    /// Federated transfer (Tier 2) - requires federation agreement
    pub async fn federated_transfer(&self, from: Did, from_coop: CoopId, to_coop: CoopId, amount: u64) -> Result<TransferReceipt>;

    /// Bridge out (Tier 3) - requires governance for large amounts
    pub async fn bridge_out(&self, requester: Did, amount: u64, destination: BridgeDestination) -> Result<BridgeReceipt>;

    /// Bridge in - convert external value to credits
    pub async fn bridge_in(&self, depositor: Did, source: BridgeSource, amount: u64) -> Result<DepositReceipt>;
}
```

---

## Implementation Roadmap

### Phase 0: Foundations (2 weeks)

- [ ] Terminology documentation (glossary)
- [ ] Protocol contracts v1 (CCL files)
  - `infrastructure-credit-v1.ccl.json`
  - `fuel-allocation-v1.ccl.json`
- [ ] Organization lifecycle RFC
- [ ] Bootstrapping RFC

### Phase 1: Contribution & Metering (4 weeks)

- [ ] Resource metering (extend `icn-obs`)
- [ ] Contribution tracking module
- [ ] Attestation protocol (extend `icn-gossip`)
- [ ] Credit issuance (extend `icn-ledger`)
- [ ] CLI: `icnctl contribution show/claim/attest`

### Phase 2: Fuel System (4 weeks)

- [ ] New crate: `icn-fuel`
- [ ] Operation costs integration
- [ ] Contribution → fuel bonus
- [ ] Compute job reservation model
- [ ] CLI: `icnctl fuel status`

### Phase 3: Organizations (4 weeks)

- [ ] New crate: `icn-organization`
- [ ] Community entity
- [ ] Household entity
- [ ] Multi-membership support
- [ ] Contribution routing
- [ ] CLI: `icnctl org create/join/leave`

### Phase 4: Exchange (6 weeks)

- [ ] New crate: `icn-exchange`
- [ ] Federation credits
- [ ] Exchange pools (AMM)
- [ ] Demurrage implementation
- [ ] Bridge framework
- [ ] CLI: `icnctl exchange swap/provide/bridge`

### Phase 5: Marketplace (4 weeks)

- [ ] New crate: `icn-marketplace`
- [ ] Listings
- [ ] Trade proposals
- [ ] Search/discovery
- [ ] CLI: `icnctl marketplace list/create/trade`

### Phase 6: Hardening (4 weeks)

- [ ] Dispute resolution system
- [ ] Security threat model
- [ ] Privacy audit
- [ ] Failure mode testing

### Phase 7: UX (8 weeks)

- [ ] Web dashboard
- [ ] Mobile client (`icn-lite`)
- [ ] Onboarding flows
- [ ] Admin tools

---

## Technical Specifications

### Provenance Tracking

```rust
pub struct CreditUnit {
    pub id: ContentHash,
    pub amount: u64,
    pub currency: Currency,
    pub provenance: Provenance,
}

pub struct Provenance {
    pub origin: Origin,
    pub history: Vec<Transfer>,
}

pub enum Origin {
    Contribution { contributor: Did, resource_type: ResourceType, verified_by: Vec<Did>, timestamp: Timestamp },
    BridgedIn { source: BridgeSource, original_value: ExternalValue, timestamp: Timestamp },
    GovernanceCreated { proposal_id: ProposalId, purpose: String },
}
```

### Donation Configuration

```rust
pub struct DonationConfig {
    pub donor_did: Did,
    pub resource_types: Vec<ResourceType>,
    pub credit_destination: CreditDestination,
}

pub enum CreditDestination {
    CooperativeTreasury(CoopId),
    NewMemberPool,
    Burn,
    UniversalBasicInfrastructure,
}
```

### API Endpoints

```
POST   /v1/contribution/claim           # Submit contribution claim
GET    /v1/contribution/claims          # List pending claims
POST   /v1/contribution/attest          # Attest to a claim
GET    /v1/contribution/balance         # View contribution balance
GET    /v1/contribution/history         # Contribution history

POST   /v1/marketplace/listing          # Create listing
GET    /v1/marketplace/listings         # Browse listings
POST   /v1/marketplace/trade            # Propose trade
GET    /v1/marketplace/trades           # View trade proposals
POST   /v1/marketplace/trade/:id/accept # Accept trade

POST   /v1/exchange/internal            # Internal transfer
POST   /v1/exchange/federated           # Federated transfer
POST   /v1/exchange/bridge/out          # Request bridge out
POST   /v1/exchange/bridge/in           # Bridge in external value
GET    /v1/exchange/rates               # View exchange rates

POST   /v1/fuel/status                  # View fuel status
GET    /v1/fuel/history                 # Fuel consumption history

POST   /v1/org/create                   # Create organization
POST   /v1/org/join                     # Join organization
POST   /v1/org/leave                    # Leave organization
GET    /v1/org/memberships              # View memberships
```

### CLI Commands

```bash
# Contribution
icnctl contribution show
icnctl contribution claim --type compute --amount 100 --evidence <hash>
icnctl contribution attest <claim-hash>
icnctl contribution balance

# Fuel
icnctl fuel status
icnctl fuel history

# Marketplace
icnctl marketplace list
icnctl marketplace create --type service --title "Tutoring" --price "10 hours"
icnctl marketplace trade --offer "20 compute-hours" --request "2 hours"

# Exchange
icnctl exchange transfer --to <did> --amount 10 --currency hours
icnctl exchange federate --to-coop <coop-id> --amount 100
icnctl exchange bridge --amount 50 --destination bank:iban:...
icnctl exchange rates

# Organization
icnctl org create --type coop --name "Tech Workers"
icnctl org create --type community --name "Oakwood Neighbors"
icnctl org join <org-id>
icnctl org leave <org-id>
icnctl org memberships

# Contribution routing
icnctl contribution route --to coop:tech-workers --percent 70
icnctl contribution route --to community:oakwood --percent 30
```

### Prometheus Metrics

```prometheus
# Contribution
icn_contribution_claims_total{resource_type}
icn_contribution_attestations_total{resource_type}
icn_contribution_verified_total{resource_type}
icn_contribution_balance{did, currency}

# Fuel
icn_fuel_consumed_total{operation}
icn_fuel_available{did}
icn_fuel_reserved{did}
icn_fuel_regenerated_total
icn_fuel_exhausted_total

# Marketplace
icn_marketplace_listings_total{type}
icn_marketplace_trades_total{status}
icn_marketplace_trade_volume{currency}

# Exchange
icn_exchange_internal_total{currency}
icn_exchange_federated_total{from_coop, to_coop}
icn_exchange_bridge_out_total{destination_type}
icn_exchange_bridge_in_total{source_type}

# Organizations
icn_org_members_total{org_type, org_id}
icn_org_partnerships_total{type}

# Anti-speculation
icn_demurrage_applied_total{currency}
icn_bridge_blocked_governance_total
```

### CCL Contract References

| Contract | Purpose | Governance Level |
|----------|---------|------------------|
| `icn://protocol/infrastructure-credit/v1` | Credit calculation formulas | Network-wide |
| `icn://protocol/fuel-allocation/v1` | Fuel allowance rules | Federation/Coop |
| `icn://protocol/exchange/v1` | Exchange pool mechanics | Federation |
| `icn://protocol/demurrage/v1` | Credit decay policy | Coop |
| `icn://protocol/membership/v1` | Membership rules | Coop |

---

## Design Invariants

These properties must hold at all times:

| Invariant | Description |
|-----------|-------------|
| **No external token** | There is no ICN coin tradeable on public exchanges |
| **Fuel always regenerates** | No permanent lockout from participation |
| **Credits require verification** | Self-attestation alone cannot create credits |
| **Demurrage exemptions are explicit** | Default is all idle credits decay |
| **Bridge requires governance** | Large outflows need community approval |
| **Provenance is reconstructible** | For bridge-eligible credits only |
| **Orgs are transparent** | Routing, treasury, membership visible |

---

## Non-Goals

To maintain focus, this RFC explicitly does **not** cover:

- **External token creation** - No ICN token on public exchanges
- **Proof-of-work mining** - No competitive hash racing
- **Fully anonymous contribution** - DIDs required (pseudonymous OK)
- **Unlimited bridging** - External exchange is governed, not permissionless
- **Smart contract Turing-completeness** - CCL is intentionally limited
- **Non-infrastructure contribution rewards** - Future RFC will address governance participation, care work, etc. via peer endorsement

These may be revisited in future RFCs based on community needs.

---

## Open Questions

### Economic

1. **Credit exchange rates**: Should 1 compute-hour = 1 storage-GB-month? Or let cooperatives set their own rates?

2. **Decay/demurrage rate**: What's the optimal rate? Too high punishes savers, too low doesn't prevent hoarding.

3. **New member bootstrapping**: How do new nodes earn initial trust to have their contributions attested?

4. **Cross-cooperative credits**: Can infrastructure credits earned in Coop A be spent in Coop B? (Federation tier handles this, but what are the default policies?)

5. **Infrastructure vs human labor equity**: Infra contributors may be wealthier members with hardware, power, and capital. If infra-hours are valued higher than human labor hours, this could create internal class dynamics. Coops should consciously decide relative rates.

6. **Hardware access programs**: Should federations facilitate hardware lending/grants so infrastructure contribution isn't limited to those who can afford equipment?

### Technical

7. **Verification scaling**: How do we verify contributions from thousands of nodes efficiently?

8. **Sybil resistance**: How do we prevent someone from running many fake nodes to game attestations?

9. **Storage of provenance**: Do we track full provenance for all credits, or just Tier 3 bridge candidates?

### Governance

10. **Policy defaults**: What should the default policies be for new cooperatives?

11. **Bridge governance**: Who approves bridge requests? Cooperative governance? Federation governance?

12. **Exchange rate governance**: How often should anchored exchange rates be updated?

### Fuel System

13. **Fuel regeneration curve**: Linear? Logarithmic? Based on network load?

14. **Cross-federation fuel**: If federations federate, how does fuel work?

15. **Emergency fuel**: What if someone legitimately needs to do something but is out of fuel?

16. **Fuel vs. credit limits**: How do fuel limits interact with credit limits? Double protection or redundant?

### Organizations

17. **Organization lifecycle**: How do coops/communities get created, dissolved, merged? (Needs separate RFC)

18. **Federation joining**: What are the requirements for joining a federation? (Needs separate RFC)

19. **Dispute resolution**: How are disputes between members, between orgs, handled? (Needs separate RFC)

### Privacy

20. **Contribution surveillance**: How much granularity do we expose in contribution metrics without deanonymizing or surveilling members? What's the right balance between transparency and privacy?

21. **Attestation privacy**: When peers attest to contributions, how much do they learn about each other's activity patterns?

### Protocol Contracts

22. **Contract upgrade path**: How do we migrate coops from v1 to v2 of a protocol contract?

23. **Custom contract security**: How do we audit/trust custom economic contracts?

24. **Cross-coop protocol conflicts**: What if federated coops use incompatible protocol versions?

### Non-Infrastructure Contributions (Future)

25. **Peer endorsement system**: How do we recognize governance participation, moderation, care work without enabling spam? Counting activities (proposals created, votes cast) incentivizes quantity over quality. Needs endorsement-based design.

---

## References

### ICN Documentation

- [ROADMAP.md](/ROADMAP.md) - Strategic roadmap
- [docs/glossary.md](glossary.md) - Authoritative terminology definitions
- [docs/economic-safety.md](economic-safety.md) - Credit limits and disputes
- [docs/econ-modeling.md](econ-modeling.md) - Economic simulation results
- [docs/governance.md](governance.md) - Governance primitives
- [docs/federation-roadmap-implementation.md](federation-roadmap-implementation.md) - Federation layer

### External References

- Lietaer, Bernard. "The Future of Money" (2001) - Mutual credit history
- Gesell, Silvio. "The Natural Economic Order" (1916) - Demurrage theory
- North, Peter. "Money and Liberation" (2007) - Cooperative currency philosophy

---

## Core Principles

The design reflects these foundational beliefs:

1. **Infrastructure is Labor**
   Running a node is work, just like tutoring or growing tomatoes. All labor earns credits in the same mutual credit system.

2. **No Speculation**
   No token. No external exchange. Value from reciprocity, not scarcity. Demurrage prevents hoarding. Governance controls bridging.

3. **Contribution Enables Action**
   Your fuel (capacity to act) comes from your contribution. More contribution = more fuel = more you can do. But fuel regenerates, so no one is locked out.

4. **Two Pillars: Civic + Economic**
   Communities for belonging, care, stewardship. Cooperatives for livelihoods, trade, production. Both are essential. Individuals belong to both.

5. **Rules Are Contracts, Not Code**
   Economic parameters live in CCL protocol contracts. Governance can update them. No hard forks needed. Communities can customize.

6. **Subsidiarity**
   Decisions at the lowest appropriate level: Member < Coop/Community < Federation < Network. Higher levels only for cross-boundary concerns.

7. **Peer Recognition, Not Self-Report**
   You earn credit when others attest to your contribution. Trust comes from the network, not from assertion.

8. **Inclusive On-Ramp**
   Start as individual → join community → join coop → federate. Global Commons accepts anyone. Progressive benefits incentivize formalization.

---

## Conclusion

The contribution credit system enables ICN to:

1. **Reward infrastructure contributors** with fungible credits
2. **Avoid speculation** through demurrage, provenance tracking, and governance
3. **Enable internal economic activity** through marketplace and multi-currency support
4. **Interface with external economy** through governed bridge layer
5. **Support diverse organizations** through communities and cooperatives as equal pillars
6. **Rate-limit fairly** through unified, regenerative fuel system

**Philosophy**: Start with Tier 1 (internal credits), add Tier 2 (federation) when multiple coops exist, add Tier 3 (bridges) only after governance processes are battle-tested. Let communities earn exchangeability through demonstrated good governance.

---

**Status**: Design Document (RFC)
**Next Steps**: Community review → Governance proposal → Implementation planning
**Feedback**: Open issues at https://github.com/InterCooperative-Network/icn/issues
