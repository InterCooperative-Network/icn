# Razeto's Four Intercooperative Bodies: ICN Integration Design

This document describes the integration of Luis Razeto's four intercooperative bodies from "Cooperative Enterprise and Market Economy" (Chapter 14) into ICN's infrastructure.

## Background

Razeto identifies four key institutions that enable cooperatives to transcend individual limitations while preserving their cooperative character:

1. **Cooperative Share Exchange** - Labor shares representing accumulated value
2. **Cooperative Labor Exchange** - Worker mobility between cooperatives
3. **Cooperative Financial Entity** - Inter-coop capital market
4. **Technology/R&D Service** - Shared technology development

These institutions allow cooperatives to achieve economies of scale, efficient capital allocation, and labor mobility without sacrificing worker ownership or democratic control.

## Design Principles

From Razeto's analysis, these principles guide our implementation:

1. **No severance of capital from labor** - Labor shares remain tied to membership
2. **Worker control preserved** - All major operations require governance approval
3. **Cooperative autonomy** - Each cooperative decides its participation level
4. **Anti-speculation** - Demurrage and contribution-locked exchange maintained
5. **Solidarity over profit** - CFE operates at cost-recovery, not profit
6. **Technology as commons** - R&D outputs default to cooperative licensing

## Current State Analysis

### ICN Strengths (Already Aligned with Razeto)

| Capability | ICN Implementation | Razeto Alignment |
|------------|-------------------|------------------|
| Mutual credit | `icn-ledger` double-entry | Foundation for labor shares |
| Treasury | Budgets, spending rules, governance | Foundation for CFE |
| Bilateral clearing | `icn-federation/clearing.rs` | Foundation for multilateral |
| Entity model | Recursive individuals→coops→federations | Matches Razeto hierarchy |
| Governance | Proposals, voting, domains | Democratic control preserved |
| Demurrage | Designed in contribution-credits-design.md | Anti-speculation |

### Gaps to Fill

| Gap | Impact | Priority |
|-----|--------|----------|
| No labor shares | Can't track accumulated surplus per worker | HIGH |
| No bonds/debt securities | Can't issue cooperative financing instruments | HIGH |
| No temporal membership | Can't track worker assignments between coops | HIGH |
| No labor credit routing | Credits don't flow through home coop | MEDIUM |
| Only bilateral clearing | No multi-party netting | MEDIUM |
| No shared service primitives | No R&D coordination | MEDIUM |
| No scheduled triggers | CCL has Trigger AST but no executor | HIGH |

---

## Part 1: Cooperative Share Exchange

Labor shares represent accumulated value from worker contributions. Unlike traditional equity, labor shares:
- Are earned through labor, not purchased
- Cannot be traded speculatively
- Are redeemed at unit value when leaving
- Accumulate allocated surplus proportionally

### New Types

**Location:** `icn-ledger/src/labor_shares.rs`

```rust
use serde::{Deserialize, Serialize};
use icn_identity::Did;

/// Unique identifier for a labor share
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ShareId(pub String);

/// Unique identifier for a bond
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BondId(pub String);

/// Labor share status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShareStatus {
    /// Actively accumulating value
    Active,
    /// Redemption in progress (governance approved)
    Redeeming { approved_at: u64, payout_schedule: Vec<ScheduledPayout> },
    /// Fully redeemed
    Redeemed { completed_at: u64, total_payout: i64 },
}

/// Event in share lifecycle for audit trail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShareEvent {
    Created { at: u64 },
    LaborContributed { at: u64, days: u64 },
    SurplusAllocated { at: u64, amount: i64, period: String },
    RedemptionStarted { at: u64, proposal_id: String },
    PayoutCompleted { at: u64, amount: i64 },
}

/// Labor share - accumulated cooperative value
/// Unit value = (total surplus) / (total labor-days)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaborShare {
    /// Unique identifier
    pub id: ShareId,
    /// Member holding this share (must be active member)
    pub holder: Did,
    /// Cooperative issuing this share
    pub cooperative_id: String,
    /// Accumulated labor contribution in days
    pub labor_days: u64,
    /// Allocated surplus in cooperative currency
    pub accumulated_surplus: i64,
    /// Currency denomination
    pub currency: String,
    /// Current status
    pub status: ShareStatus,
    /// Audit trail of all events
    pub provenance: Vec<ShareEvent>,
}

/// Bond status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BondStatus {
    /// Offered but not fully subscribed
    Offering { subscribed: i64 },
    /// Fully subscribed and active
    Active,
    /// Interest/principal payments in progress
    Maturing,
    /// Fully repaid
    Matured { completed_at: u64 },
    /// Defaulted (governance intervention needed)
    Defaulted { at: u64, reason: String },
}

/// Payment schedule for bonds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PaymentSchedule {
    /// All at maturity
    Bullet,
    /// Regular interest, principal at maturity
    InterestOnly { interval_days: u32 },
    /// Equal periodic payments
    Amortizing { interval_days: u32 },
}

/// Cooperative bond (debt security)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CooperativeBond {
    /// Unique identifier
    pub id: BondId,
    /// Issuing cooperative
    pub issuer_id: String,
    /// Current holder (can be individual or cooperative)
    pub holder: Did,
    /// Principal amount
    pub principal: i64,
    /// Annual interest rate in basis points (100 = 1%)
    pub interest_rate_bps: u16,
    /// Maturity date (unix timestamp)
    pub maturity_date: u64,
    /// Payment schedule
    pub payment_schedule: PaymentSchedule,
    /// Currency denomination
    pub currency: String,
    /// Current status
    pub status: BondStatus,
    /// Governance proposal that approved issuance
    pub approval_proposal: String,
}

/// Scheduled payout for share redemption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledPayout {
    pub due_date: u64,
    pub amount: i64,
    pub paid: bool,
}

/// Periodic surplus allocation to shareholders
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurplusAllocation {
    /// Unique identifier
    pub id: String,
    /// Cooperative performing allocation
    pub cooperative_id: String,
    /// Total surplus being allocated
    pub total_surplus: i64,
    /// Period identifier (e.g., "2025-Q4")
    pub period: String,
    /// Calculated unit value: surplus / total_labor_days
    pub share_unit_value: f64,
    /// Individual allocations: (share_id, amount)
    pub allocations: Vec<(ShareId, i64)>,
    /// Governance proposal that approved this allocation
    pub proposal_id: String,
    /// Timestamp of allocation
    pub allocated_at: u64,
}
```

### Treasury Integration

Extend `TreasuryOperation` in `icn-ledger/src/treasury.rs`:

```rust
pub enum TreasuryOperation {
    // ... existing variants ...

    /// Allocate surplus to labor shareholders
    AllocateSurplus {
        allocation: SurplusAllocation,
    },

    /// Redeem labor share (payout to departing member)
    RedeemShare {
        share_id: ShareId,
        payout_amount: i64,
        recipient: Did,
    },

    /// Issue cooperative bond
    IssueBond {
        bond: CooperativeBond,
    },

    /// Make bond payment (interest or principal)
    BondPayment {
        bond_id: BondId,
        payment_type: BondPaymentType,
        amount: i64,
    },
}

pub enum BondPaymentType {
    Interest,
    Principal,
    Combined,
}
```

### Governance Integration

Add to `ProposalPayload` in `icn-governance/src/proposal.rs`:

```rust
pub enum ProposalPayload {
    // ... existing variants ...

    /// Approve periodic surplus allocation
    SurplusAllocation {
        allocation: SurplusAllocation,
    },

    /// Approve share redemption for departing member
    ShareRedemption {
        member: Did,
        share_ids: Vec<ShareId>,
        payout_schedule: Vec<ScheduledPayout>,
    },

    /// Approve bond issuance
    BondIssuance {
        bond_offering: BondOffering,
    },
}

/// Bond offering for governance approval
pub struct BondOffering {
    pub issuer_id: String,
    pub principal_requested: i64,
    pub interest_rate_bps: u16,
    pub term_days: u32,
    pub purpose: String,
    pub payment_schedule: PaymentSchedule,
}
```

### Gossip Topics

- `labor-shares:allocations` - Surplus allocation announcements
- `bonds:issuance` - New bond offerings
- `bonds:payments` - Bond payment notifications

---

## Part 2: Cooperative Labor Exchange

Enables worker mobility between cooperatives while preserving membership ties and ensuring credits flow appropriately.

### New Types

**Location:** `icn-entity/src/labor_exchange.rs`

```rust
use serde::{Deserialize, Serialize};

/// Unique identifier for an assignment
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AssignmentId(pub String);

/// Type of labor assignment
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssignmentType {
    /// Fixed-term project work
    Project { project_id: String },
    /// Seasonal or cyclical work
    Seasonal { season: String },
    /// Trial period before potential membership
    Trial { duration_days: u32 },
    /// Dual membership arrangement
    DualMembership,
}

/// Assignment status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssignmentStatus {
    /// Proposed, awaiting governance approval
    Proposed,
    /// Active assignment
    Active,
    /// Completed normally
    Completed { completed_at: u64 },
    /// Terminated early
    Terminated { at: u64, reason: String },
}

/// How credits earned at host flow to worker/home coop
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RoutingMode {
    /// All credits go through home coop, which pays worker
    ThroughHomeCoop,
    /// Credits go directly to worker, home coop gets fee
    DirectToWorker,
    /// Split between worker (direct) and home coop
    Split { worker_pct: u8 },
}

/// Credit routing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreditRouting {
    /// How credits flow
    pub routing_mode: RoutingMode,
    /// Home coop's coordination fee in basis points
    pub admin_fee_bps: u16,
    /// Does host contribute to worker's home coop shares?
    pub share_contribution: bool,
}

/// Worker assignment to host cooperative
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaborAssignment {
    /// Unique identifier
    pub id: AssignmentId,
    /// Worker being assigned (individual entity)
    pub worker: String,
    /// Cooperative holding worker's primary membership
    pub home_coop: String,
    /// Cooperative where work happens
    pub host_coop: String,
    /// Type of assignment
    pub assignment_type: AssignmentType,
    /// Start date (unix timestamp)
    pub start_date: u64,
    /// End date (None for open-ended)
    pub end_date: Option<u64>,
    /// Credit routing configuration
    pub credit_routing: CreditRouting,
    /// Current status
    pub status: AssignmentStatus,
    /// Governance proposals (from both coops)
    pub approvals: AssignmentApprovals,
}

/// Required approvals for assignment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssignmentApprovals {
    pub home_coop_proposal: Option<String>,
    pub host_coop_proposal: Option<String>,
}

/// Worker registration in labor pool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolRegistration {
    pub worker: String,
    pub home_coop: String,
    pub skills: Vec<String>,
    pub availability: Availability,
    pub registered_at: u64,
}

/// Availability specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Availability {
    Immediate,
    FromDate(u64),
    Seasonal { seasons: Vec<String> },
    PartTime { hours_per_week: u32 },
}

/// Open position in labor pool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaborNeed {
    pub id: String,
    pub requesting_coop: String,
    pub role: String,
    pub skills_required: Vec<String>,
    pub duration: Option<u32>,
    pub posted_at: u64,
}

/// Labor pool for federation-wide coordination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaborPool {
    /// Unique identifier
    pub id: String,
    /// Federation managing this pool
    pub manager: String,
    /// Available workers
    pub available_workers: Vec<PoolRegistration>,
    /// Open positions
    pub open_needs: Vec<LaborNeed>,
}
```

### Membership Integration

Extend `Membership` in `icn-entity/src/membership.rs`:

```rust
pub struct Membership {
    // ... existing fields ...

    /// Labor share IDs held by this member
    pub labor_shares: Vec<ShareId>,

    /// Active assignments (working at other coops)
    pub assignments: Vec<AssignmentId>,

    /// Is this the primary membership?
    pub is_primary: bool,
}
```

### Federation Clearing Integration

Extend `CrossCoopTransfer` in `icn-federation/src/clearing.rs`:

```rust
pub enum TransferType {
    // ... existing variants ...

    /// Labor credit for assigned worker
    LaborCredit {
        assignment_id: AssignmentId,
        period: String,
    },

    /// Admin fee for labor coordination
    LaborAdminFee {
        assignment_id: AssignmentId,
    },
}
```

### Gossip Topics

- `labor-pool:registrations` - Worker availability announcements
- `labor-pool:needs` - Open position postings
- `assignments:status` - Assignment updates (trust-gated)

---

## Part 3: Cooperative Financial Entity (CFE)

A cooperative of cooperatives providing capital market functions without profit motive.

### Key Characteristics

- **Not a bank** - No fractional reserve, no profit extraction
- **Member-owned** - Each cooperative has governance rights
- **Cost-recovery** - Interest rates cover admin costs only
- **Solidarity-based** - Stronger coops support developing ones

### New Types

**Location:** `icn-federation/src/financial_entity.rs`

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Deposit term
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DepositTerm {
    /// Withdrawable anytime
    Demand,
    /// Up to 90 days
    Short,
    /// 90 days to 1 year
    Medium,
    /// Over 1 year
    Long { months: u32 },
}

/// Term deposit from member cooperative
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermDeposit {
    pub id: String,
    pub depositor: String,
    pub principal: i64,
    pub term: DepositTerm,
    pub interest_rate_bps: u16,
    pub maturity_date: u64,
    pub currency: String,
}

/// Loan purpose categories
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoanPurpose {
    WorkingCapital,
    Equipment,
    Expansion,
    ShareBuyout,
    Emergency,
    Other(String),
}

/// Collateral types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Collateral {
    LaborShares { share_ids: Vec<String>, value: i64 },
    Bonds { bond_ids: Vec<String>, value: i64 },
    CoopGuarantee { guarantor: String, amount: i64 },
    Equipment { description: String, value: i64 },
}

/// Loan to member cooperative
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CooperativeLoan {
    pub id: String,
    pub borrower: String,
    pub principal: i64,
    pub interest_rate_bps: u16,
    pub purpose: LoanPurpose,
    pub collateral: Vec<Collateral>,
    pub disbursed_at: u64,
    pub maturity_date: u64,
    pub approval_proposal: String,
    pub currency: String,
}

/// Clearing cycle status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClearingCycleStatus {
    Proposed,
    CollectingPositions,
    Computing,
    AwaitingSettlement,
    Settled { settled_at: u64 },
    Failed { reason: String },
}

/// Multilateral clearing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultilateralClearing {
    pub cycle_id: String,
    pub participants: Vec<String>,
    /// Gross positions: (from, to) -> amount
    pub gross_positions: HashMap<(String, String), i64>,
    /// Net positions after netting: entity -> net amount (positive = receive)
    pub net_positions: HashMap<String, i64>,
    /// Netting efficiency: 1 - (sum of net / sum of gross)
    pub netting_efficiency: f64,
    pub status: ClearingCycleStatus,
    pub initiated_at: u64,
}

/// Rate policy (administered, not market)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatePolicy {
    /// Base deposit rate by term
    pub deposit_rates: HashMap<String, u16>,
    /// Base lending rate by purpose
    pub lending_rates: HashMap<String, u16>,
    /// Solidarity adjustment for developing coops
    pub solidarity_discount_bps: u16,
    /// Last updated
    pub updated_at: u64,
    /// Governance proposal that set this policy
    pub proposal_id: String,
}

/// Cooperative Financial Entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CooperativeFinancialEntity {
    /// Entity ID (as federation member)
    pub id: String,
    /// Member cooperatives
    pub member_coops: Vec<String>,
    /// Active deposits
    pub deposits: Vec<TermDeposit>,
    /// Active loans
    pub loans: Vec<CooperativeLoan>,
    /// Current clearing cycle
    pub clearing: Option<MultilateralClearing>,
    /// Reserve ratio in basis points
    pub reserve_ratio_bps: u16,
    /// Current rate policy
    pub rate_policy: RatePolicy,
    /// Governance domain for CFE decisions
    pub governance_domain: String,
}
```

### Governance Integration

```rust
pub enum ProposalPayload {
    // ... existing variants ...

    /// Add/remove cooperative from CFE
    CFEMembership {
        coop: String,
        action: MembershipAction,
    },

    /// Approve loan request
    LoanApproval {
        loan_request: LoanRequest,
    },

    /// Change rate policy
    RatePolicyChange {
        new_policy: RatePolicy,
    },

    /// Initiate multilateral clearing cycle
    ClearingCycleInitiate {
        participants: Vec<String>,
        cycle_end: u64,
    },
}

pub enum MembershipAction {
    Add,
    Remove { reason: String },
    Suspend { until: u64 },
}

pub struct LoanRequest {
    pub borrower: String,
    pub amount: i64,
    pub purpose: LoanPurpose,
    pub term_days: u32,
    pub collateral_offered: Vec<Collateral>,
}
```

### Gossip Topics

- `cfe:deposits` - Deposit availability
- `cfe:loans` - Loan requests/approvals (trust-gated)
- `cfe:clearing` - Multilateral clearing cycles
- `cfe:rates` - Rate policy updates

---

## Part 4: Technology/R&D Service

Shared technology development and service provision across cooperatives.

### New Types

**Location:** `icn-federation/src/shared_services.rs`

```rust
use serde::{Deserialize, Serialize};

/// Service type categories
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceType {
    Compute,
    Storage,
    Expertise { domain: String },
    TechDev,
    Training,
    Other(String),
}

/// Service pricing model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServicePricing {
    /// No charge (commons)
    Free,
    /// Cover costs only
    CostRecovery { rate: i64, unit: String },
    /// Scaled by cooperative size/resources
    SlidingScale { base_rate: i64, scale_factor: String },
    /// Exchanged for labor
    LaborExchange { hours_per_unit: f32 },
}

/// Service access control
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServiceAccess {
    /// Open to all federation members
    FederationOpen,
    /// Requires minimum trust score
    TrustGated { min_score: f64 },
    /// Subscription required
    Subscription { members: Vec<String> },
    /// Invitation only
    InviteOnly,
}

/// Shared service through federation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedService {
    pub id: String,
    pub provider: String,
    pub name: String,
    pub description: String,
    pub service_type: ServiceType,
    pub pricing: ServicePricing,
    pub access_control: ServiceAccess,
    pub created_at: u64,
}

/// R&D participant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RDParticipant {
    pub coop: String,
    pub contribution_type: ContributionType,
    pub commitment: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContributionType {
    Funding { amount: i64 },
    Labor { hours: u32 },
    Equipment { description: String },
    Expertise { domain: String },
}

/// Project budget
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectBudget {
    pub total: i64,
    pub contributions: Vec<(String, i64)>,
    pub spent: i64,
    pub currency: String,
}

/// Project deliverable
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deliverable {
    pub id: String,
    pub description: String,
    pub due_date: u64,
    pub status: DeliverableStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeliverableStatus {
    Planned,
    InProgress,
    Completed { at: u64 },
    Delayed { new_date: u64 },
}

/// Technology licensing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TechLicensing {
    /// Free for all cooperatives
    CooperativeCommons,
    /// Free with attribution
    CoopPermissive,
    /// Tiered by cooperative type
    Tiered { rates: Vec<(String, i64)> },
    /// Revenue sharing
    RevenueShare { percentage: u8 },
}

/// Project governance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectGovernance {
    pub decision_method: DecisionMethod,
    pub steering_committee: Vec<String>,
    pub governance_domain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecisionMethod {
    Consensus,
    Majority,
    WeightedByContribution,
}

/// R&D project coordinated across cooperatives
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RDProject {
    pub id: String,
    pub title: String,
    pub description: String,
    pub participants: Vec<RDParticipant>,
    pub budget: ProjectBudget,
    pub deliverables: Vec<Deliverable>,
    pub licensing: TechLicensing,
    pub governance: ProjectGovernance,
    pub started_at: u64,
}

/// Technology registry entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source_project: Option<String>,
    pub licensing: TechLicensing,
    pub maintainer: String,
    pub registered_at: u64,
}

/// Technology registry (commons catalog)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechRegistry {
    pub id: String,
    pub manager: String,
    pub technologies: Vec<TechEntry>,
}
```

### Integration with icn-compute

`SharedService::Compute` integrates with `icn-compute` scheduler:

```rust
impl SharedService {
    pub fn as_compute_provider(&self) -> Option<ComputeProvider> {
        match &self.service_type {
            ServiceType::Compute => Some(ComputeProvider {
                id: self.id.clone(),
                provider_entity: self.provider.clone(),
                pricing: self.pricing.clone(),
                access: self.access_control.clone(),
            }),
            _ => None,
        }
    }
}
```

### Governance Integration

```rust
pub enum ProposalPayload {
    // ... existing variants ...

    /// Propose R&D project
    RDProjectProposal {
        project: RDProject,
    },

    /// Offer shared service
    ServiceOffering {
        service: SharedService,
    },

    /// Change technology licensing
    TechLicensingChange {
        tech_id: String,
        new_licensing: TechLicensing,
    },
}
```

### Gossip Topics

- `services:catalog` - Shared service announcements
- `rd:projects` - R&D project coordination
- `tech:registry` - Technology registry updates

---

## Part 5: Scheduler (Cross-Cutting)

All four bodies require scheduled operations. CCL has `Trigger` AST but no executor.

### New Crate: `icn-scheduler`

```rust
use serde::{Deserialize, Serialize};

/// Schedule specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Schedule {
    /// Cron-style expression
    Cron(String),
    /// Fixed interval
    Interval { seconds: u64 },
    /// One-time execution
    OneTime { at: u64 },
    /// After another task completes
    AfterTask { task_id: String },
}

/// Task status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed { at: u64 },
    Failed { at: u64, error: String },
    Disabled,
}

/// Scheduled task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub id: String,
    /// CCL contract reference
    pub contract_ref: String,
    /// Rule to execute
    pub rule_name: String,
    /// Schedule
    pub schedule: Schedule,
    /// Next scheduled run
    pub next_run: u64,
    /// Last run result
    pub last_run: Option<TaskRun>,
    /// Current status
    pub status: TaskStatus,
    /// Created by (entity)
    pub owner: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRun {
    pub started_at: u64,
    pub completed_at: u64,
    pub success: bool,
    pub fuel_used: u64,
    pub output: Option<String>,
}
```

### Use Cases

| Body | Scheduled Operation |
|------|---------------------|
| Labor Shares | Periodic surplus allocation (quarterly) |
| Labor Shares | Bond interest/principal payments |
| Labor Exchange | Assignment end notifications |
| Labor Exchange | Trial period expiry checks |
| CFE | Interest calculations |
| CFE | Clearing cycle initiation |
| R&D | Milestone deadline reminders |

### Supervisor Integration

Add to `icn-core/src/supervisor.rs`:

```rust
impl Supervisor {
    pub async fn spawn_scheduler(&self, config: SchedulerConfig) -> Result<SchedulerHandle> {
        let scheduler = Scheduler::new(config, self.store.clone());
        let handle = scheduler.spawn().await?;
        // Wire to CCL executor
        // ...
        Ok(handle)
    }
}
```

---

## Implementation Roadmap

### Phase 1: Labor Shares Foundation (4-6 weeks)

1. Add `LaborShare`, `CooperativeBond` types to `icn-ledger`
2. Add `TreasuryOperation::AllocateSurplus`, `RedeemShare`, `IssueBond`
3. Add `ProposalPayload::SurplusAllocation` to `icn-governance`
4. Link shares to `Membership` tracking
5. Create gossip topics
6. Unit tests

### Phase 2: Labor Exchange (4-6 weeks)

1. Add `LaborAssignment`, `CreditRouting` to `icn-entity`
2. Extend `Membership` with assignments
3. Add `TransferType::LaborCredit` to federation clearing
4. Add `ProposalPayload::LaborAssignment`
5. Create labor pool primitives
6. Integration tests

### Phase 3: CFE Core (6-8 weeks)

1. Add CFE types to `icn-federation`
2. Implement deposit/loan lifecycle
3. Implement multilateral clearing algorithm
4. Add governance proposals
5. Wire to treasury for settlements
6. End-to-end tests

### Phase 4: Scheduler (3-4 weeks, parallel with Phase 2-3)

1. Create `icn-scheduler` crate
2. Implement cron-style scheduling
3. Wire to CCL trigger execution
4. Add to supervisor spawn
5. Tests for scheduling edge cases

### Phase 5: Shared Services (4-6 weeks)

1. Add service types to `icn-federation`
2. Link `ServiceType::Compute` to `icn-compute`
3. Implement service catalog
4. Add governance proposals
5. Tests

### Phase 6: R&D Coordination (4-6 weeks)

1. Add R&D project types
2. Implement technology registry
3. Add licensing enforcement
4. Wire to scheduler for milestones
5. Integration tests

---

## Critical Files Reference

| File | Purpose |
|------|---------|
| `icn-ledger/src/treasury.rs` | Pattern for labor shares/bonds; extend TreasuryOperation |
| `icn-entity/src/membership.rs` | Extend with assignments, share links |
| `icn-federation/src/clearing.rs` | Extend bilateral to multilateral; base for CFE |
| `icn-governance/src/proposal.rs` | Add ProposalPayload variants |
| `icn-ccl/src/ast.rs` | Trigger type exists; needs executor |
| `icn-core/src/supervisor.rs` | Actor spawning for scheduler |
| `docs/contribution-credits-design.md` | Maintain economic principles |

---

## Related Documents

- [contribution-credits-design.md](economics/contribution-credits-design.md) - Mutual credit and demurrage design
- [ARCHITECTURE.md](../ARCHITECTURE.md) - System architecture overview
- [PHASE_HISTORY.md](../PHASE_HISTORY.md) - Development phase history
