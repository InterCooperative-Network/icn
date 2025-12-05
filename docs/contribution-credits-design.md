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
| 0.2.0 | 2025-12-05 | Added unified fuel model, communities as first-class entities, organizational structures, protocol contracts, comprehensive open questions |
| 0.3.0 | 2025-12-05 | Major revision: unified fuel reservation model, full Community entity with types and partnerships, provenance scope table, expanded demurrage exemptions, reorganized structure |

---

## Executive Summary

This document defines how ICN credits infrastructure contributors (compute, storage, bandwidth) in a **non-speculative** way. The core insight: **infrastructure provision is labor directed at the network**. Contributors earn mutual credit that can be used within the cooperative ecosystem, traded for goods/services, or (with governance approval) exchanged externally.

**Key Principles**:
- Credits represent real resource contribution, not speculative value
- No external market = no speculation
- Value flows from reciprocity, not scarcity
- Governance controls exchangeability

**Relationship to Fuel**: Credits are **claims on value**. Fuel is **permission to act**. Contributors earn credits AND receive higher fuel allowances. Both systems work together - fuel is consumed from a single regenerating pool for all network operations including compute execution.

**Crucially, this design does not introduce a tradeable token.** All accounting uses mutual credit on existing ledgers, governed by cooperatives. There is no ICN coin, no mining, no external exchange.

**Terminology**: See [glossary.md](glossary.md) for authoritative definitions of all terms used in this document.

---

## TL;DR

- **Run a node** → Earn credits for compute/storage/bandwidth
- **Credits are hours** → Spendable on goods, services, or infrastructure
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
5. [Contribution Verification](#contribution-verification)
6. [Fuel System](#fuel-system)
7. [Organizational Structures](#organizational-structures)
8. [Internal Marketplace](#internal-marketplace)
9. [Anti-Speculation Mechanisms](#anti-speculation-mechanisms)
10. [Protocol Contracts](#protocol-contracts)
11. [Exchange Architecture](#exchange-architecture)
12. [Implementation Roadmap](#implementation-roadmap)
13. [Technical Specifications](#technical-specifications)
14. [Non-Goals](#non-goals)
15. [Open Questions](#open-questions)
16. [Example: Dave's Journey](#example-daves-journey)
17. [References](#references)
18. [Core Principles](#core-principles)

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

**The key difference**: Credits represent actual resource contribution, not speculative value. You can't get rich by hoarding them - you can only use them to access network resources or trade for goods/services.

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

Now Dave's infrastructure credits are just hours - **spendable anywhere in the cooperative** for anything. Infrastructure capacity is just another listing type in the marketplace, priced in the same mutual credit system as tutoring, tomatoes, and childcare. Running a node is labor; the marketplace treats it equally.

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

### Verification Tiers Based on Value

Combine approaches based on contribution size:

| Contribution Value | Verification Method |
|-------------------|---------------------|
| Small (<100 credits) | Peer attestation only |
| Medium (100-1000) | Attestation + spot checks |
| Large (>1000) | Full cryptographic proof |

### Provenance Scope

Not all credits need full provenance tracking:

| Credit Type | Provenance Requirement | Rationale |
|-------------|----------------------|-----------|
| **Tier 1 (Internal)** | Compressed/batched | Storage efficiency; never leaves coop |
| **Tier 2 (Federated)** | Origin + last transfer | Enough for dispute resolution |
| **Tier 3 (Bridge candidate)** | Full chain | Required for external auditability |

**Implementation note**: Credits start with compressed provenance. When a member requests bridge-out eligibility, the system reconstructs or requires full provenance for the specific units being bridged. Credits that have been transferred internally may not be reconstructible and thus remain Tier 1/2 only.

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

All operations draw from a single regenerating fuel pool - both network operations (posting, trading, voting) and compute execution.

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

    /// Consume fuel for an operation (respects reserved fuel)
    pub fn consume(&mut self, amount: u64) -> Result<(), FuelError> {
        self.regenerate();
        if self.usable() < amount {
            return Err(FuelError::InsufficientFuel);
        }
        self.available -= amount;
        Ok(())
    }

    /// Release reserved fuel (unused portion returned)
    pub fn unreserve(&mut self, amount: u64) {
        self.reserved = self.reserved.saturating_sub(amount);
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
impl FuelAllowance {
    pub fn calculate(
        did: &Did,
        trust_score: f64,
        contribution_history: &ContributionHistory,
    ) -> Self {
        let base = 100;  // Everyone gets this
        let trust_bonus = (trust_score * 500.0) as u64;
        let contribution_bonus = contribution_history.total_contributed / 10;

        let max = base + trust_bonus + contribution_bonus;
        let regen_rate = max / 24;  // Fully regenerate in 24 hours

        FuelAllowance {
            did: did.clone(),
            available: max,
            reserved: 0,
            max,
            regen_rate,
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

---

## Internal Marketplace

Credits aren't just for infrastructure - they enable a full internal economy.

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

### 1. Demurrage (Negative Interest)

Credits lose value over time if not used:

```rust
pub struct DemurragePolicy {
    pub rate: f64,           // e.g., 5% per year
    pub period: Duration,    // Applied monthly
    pub exemptions: Vec<ExemptionRule>,
}
```

**Effect**: Encourages circulation, discourages hoarding.

#### Demurrage Governance Layers

Demurrage is **not** a single global policy. It operates at multiple levels:

| Level | Sets | Constraints |
|-------|------|-------------|
| **Network** | Maximum allowed rate, minimum review cadence | e.g., "No coop may set demurrage > 20%/year" |
| **Federation** | Recommended defaults for member coops | e.g., "PNW Federation recommends 5%/year" |
| **Cooperative** | Actual policy via `demurrage/v1` contract | Must stay within network constraints |

#### Exemption Examples

Demurrage should penalize idle hoarding, not structural precarity. Coops MAY exempt:

| Exemption | Rationale |
|-----------|-----------|
| **Disability accommodations** | Members with reduced capacity shouldn't be penalized |
| **Parental/caregiver leave** | Temporary absence for caregiving |
| **Medical crisis** | Illness, hospitalization, recovery |
| **Seasonal workers** | Some work is inherently cyclical |
| **New members** | Grace period (e.g., first 90 days) |
| **Reserve accounts** | Designated emergency funds |
| **Escrow balances** | Funds held for pending transactions |

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

## Example: Dave's Journey

This section illustrates how a contributor progresses through the system.

### Week 1: Starting Out

Dave installs `icnd` on his home server. He doesn't join any coop yet.

```bash
icnd init --passphrase "..."
icnd start
```

His node starts contributing to the Global Commons. He earns minimal benefits (30% fuel, federation credits only).

### Week 4: Getting Attested

After a month of reliable uptime, other nodes start attesting to Dave's contributions:

```bash
# Another node operator attests
icnctl contribution attest \
  --contributor did:icn:dave \
  --resource compute \
  --amount 720 \
  --period "2025-01-01/2025-01-31"
```

Once Dave has enough weighted attestations, his contributions are verified and he receives credits.

### Week 8: Joining a Cooperative

Dave joins the local Tech Coop. He routes 70% of his contribution there:

```bash
icnctl node route --to coop:tech-workers --percent 70
icnctl node route --to global-commons --percent 30
```

Now he earns full coop credits (70%) plus federation credits (30%).

### Week 12: First Trade

Dave uses his credits to get tutoring from Alice (also in Tech Coop):

```bash
icnctl marketplace trade \
  --offer "20 hours" \
  --request "2 hours tutoring from did:icn:alice"
```

Alice accepts. Dave's balance decreases by 20 hours, Alice's increases. No fiat changed hands.

### Month 6: Household Pooling

Dave's household pools 4 devices. His kids' tablets contribute when charging overnight:

```bash
icnctl household create "Smith Family"
icnctl device add --type tablet --availability when-charging
icnctl device add --type laptop --availability when-idle
```

Benefits are shared equally among household members.

### Year 1: Cross-Coop Trading

Tech Coop federates with Food Coop. Dave buys organic vegetables with credits he earned running infrastructure:

```bash
icnctl exchange federate \
  --from coop:tech-workers \
  --to coop:food-collective \
  --amount 50
```

The exchange rate (1 tech-hour = 1.5 food-hours) was set by the federated coops' governance.

### The Journey Summarized

```
Week 1:  Individual → Global Commons (minimal benefits)
Week 4:  Attested contributor (earning credits)
Week 8:  Coop member (full benefits)
Week 12: Active trader (using credits for services)
Month 6: Household (pooled devices)
Year 1:  Federated (cross-coop trading)
```

Dave never bought tokens. He earned value through genuine contribution, and spent it within the cooperative ecosystem.

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
