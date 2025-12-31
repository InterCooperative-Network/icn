//! Labor shares and cooperative bonds for Razeto integration
//!
//! This module implements Razeto's Cooperative Share Exchange concepts:
//! - Labor shares representing accumulated value from worker contributions
//! - Cooperative bonds as debt securities for inter-coop financing
//! - Surplus allocation for periodic distribution to shareholders
//!
//! ## Key Concepts
//!
//! - **Labor Share**: Unit value = (total surplus) / (total labor-days)
//! - **No speculation**: Shares are earned through labor, not purchased
//! - **Redemption**: At unit value when leaving the cooperative
//! - **Governance-gated**: All major operations require governance approval

use icn_identity::Did;
use serde::{Deserialize, Serialize};

/// Unique identifier for a labor share
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ShareId(pub String);

impl ShareId {
    /// Create a new ShareId from a string
    pub fn new(id: impl Into<String>) -> Self {
        ShareId(id.into())
    }
}

impl std::fmt::Display for ShareId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a cooperative bond
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BondId(pub String);

impl BondId {
    /// Create a new BondId from a string
    pub fn new(id: impl Into<String>) -> Self {
        BondId(id.into())
    }
}

impl std::fmt::Display for BondId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Status of a labor share in its lifecycle
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ShareStatus {
    /// Actively accumulating value through labor and surplus allocation
    #[default]
    Active,

    /// Redemption in progress (governance approved, payouts scheduled)
    Redeeming {
        /// When redemption was approved (Unix timestamp)
        approved_at: u64,
        /// Scheduled payouts for the redemption
        payout_schedule: Vec<ScheduledPayout>,
    },

    /// Fully redeemed (no longer active)
    Redeemed {
        /// When redemption completed (Unix timestamp)
        completed_at: u64,
        /// Total amount paid out
        total_payout: i64,
    },
}


/// Scheduled payout for share redemption or bond payment
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledPayout {
    /// When the payout is due (Unix timestamp)
    pub due_date: u64,
    /// Amount to pay
    pub amount: i64,
    /// Whether this payout has been completed
    pub paid: bool,
    /// When it was paid, if completed
    pub paid_at: Option<u64>,
}

impl ScheduledPayout {
    /// Create a new scheduled payout
    pub fn new(due_date: u64, amount: i64) -> Self {
        ScheduledPayout {
            due_date,
            amount,
            paid: false,
            paid_at: None,
        }
    }

    /// Mark this payout as completed
    pub fn mark_paid(&mut self, at: u64) {
        self.paid = true;
        self.paid_at = Some(at);
    }
}

/// Event in a labor share's lifecycle for audit trail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShareEvent {
    /// Share was created
    Created {
        /// When created (Unix timestamp)
        at: u64,
    },

    /// Labor contribution recorded
    LaborContributed {
        /// When contribution occurred (Unix timestamp)
        at: u64,
        /// Number of labor days contributed
        days: u64,
        /// Optional description of work
        description: Option<String>,
    },

    /// Surplus was allocated to this share
    SurplusAllocated {
        /// When allocated (Unix timestamp)
        at: u64,
        /// Amount allocated
        amount: i64,
        /// Period identifier (e.g., "2025-Q4")
        period: String,
    },

    /// Redemption process started
    RedemptionStarted {
        /// When started (Unix timestamp)
        at: u64,
        /// Governance proposal that approved redemption
        proposal_id: String,
    },

    /// A payout was completed
    PayoutCompleted {
        /// When completed (Unix timestamp)
        at: u64,
        /// Amount paid
        amount: i64,
    },

    /// Share was fully redeemed
    FullyRedeemed {
        /// When completed (Unix timestamp)
        at: u64,
    },
}

/// Labor share representing accumulated cooperative value
///
/// Labor shares are the core mechanism for tracking worker contributions
/// and distributing surplus according to Razeto's framework. They differ
/// from traditional equity in that they:
/// - Are earned through labor, not purchased
/// - Cannot be traded speculatively
/// - Are redeemed at unit value when leaving
/// - Accumulate allocated surplus proportionally
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaborShare {
    /// Unique identifier
    pub id: ShareId,

    /// Member holding this share (must be active cooperative member)
    pub holder: Did,

    /// Cooperative that issued this share
    pub cooperative_id: String,

    /// Accumulated labor contribution in days
    /// This is the denominator for calculating share value
    pub labor_days: u64,

    /// Allocated surplus in cooperative currency
    /// This accumulates from periodic surplus allocations
    pub accumulated_surplus: i64,

    /// Currency denomination for surplus
    pub currency: String,

    /// Current status in the share lifecycle
    pub status: ShareStatus,

    /// Audit trail of all events affecting this share
    pub provenance: Vec<ShareEvent>,

    /// When this share was created (Unix timestamp)
    pub created_at: u64,
}

impl LaborShare {
    /// Create a new labor share for a member joining the cooperative
    pub fn new(
        id: ShareId,
        holder: Did,
        cooperative_id: String,
        currency: String,
        created_at: u64,
    ) -> Self {
        LaborShare {
            id,
            holder,
            cooperative_id,
            labor_days: 0,
            accumulated_surplus: 0,
            currency,
            status: ShareStatus::Active,
            provenance: vec![ShareEvent::Created { at: created_at }],
            created_at,
        }
    }

    /// Record a labor contribution to this share
    pub fn record_labor(&mut self, days: u64, at: u64, description: Option<String>) {
        self.labor_days = self.labor_days.saturating_add(days);
        self.provenance.push(ShareEvent::LaborContributed {
            at,
            days,
            description,
        });
    }

    /// Allocate surplus to this share
    pub fn allocate_surplus(&mut self, amount: i64, period: String, at: u64) {
        self.accumulated_surplus = self.accumulated_surplus.saturating_add(amount);
        self.provenance
            .push(ShareEvent::SurplusAllocated { at, amount, period });
    }

    /// Calculate the current value of this share
    ///
    /// Value is based on accumulated surplus, not labor days directly.
    /// Labor days determine proportional allocation during surplus distribution.
    pub fn current_value(&self) -> i64 {
        self.accumulated_surplus
    }

    /// Check if this share is active and can accumulate value
    pub fn is_active(&self) -> bool {
        matches!(self.status, ShareStatus::Active)
    }

    /// Start the redemption process for this share
    pub fn start_redemption(
        &mut self,
        payout_schedule: Vec<ScheduledPayout>,
        proposal_id: String,
        at: u64,
    ) {
        self.status = ShareStatus::Redeeming {
            approved_at: at,
            payout_schedule,
        };
        self.provenance
            .push(ShareEvent::RedemptionStarted { at, proposal_id });
    }

    /// Record a completed payout during redemption
    pub fn record_payout(&mut self, amount: i64, at: u64) {
        self.provenance
            .push(ShareEvent::PayoutCompleted { at, amount });

        // Check if all payouts are complete
        if let ShareStatus::Redeeming {
            approved_at,
            ref mut payout_schedule,
        } = self.status
        {
            let total_paid: i64 = payout_schedule
                .iter()
                .filter(|p| p.paid)
                .map(|p| p.amount)
                .sum();

            let total_scheduled: i64 = payout_schedule.iter().map(|p| p.amount).sum();

            if total_paid + amount >= total_scheduled {
                self.status = ShareStatus::Redeemed {
                    completed_at: at,
                    total_payout: total_paid + amount,
                };
                self.provenance.push(ShareEvent::FullyRedeemed { at });
            } else {
                // Keep the approved_at value when updating status
                let _ = approved_at;
            }
        }
    }
}

/// Status of a cooperative bond in its lifecycle
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BondStatus {
    /// Bond offering is open, not yet fully subscribed
    Offering {
        /// Amount subscribed so far
        subscribed: i64,
    },

    /// Fully subscribed and actively accruing interest
    Active,

    /// Interest/principal payments in progress
    Maturing,

    /// Fully repaid
    Matured {
        /// When fully repaid (Unix timestamp)
        completed_at: u64,
    },

    /// Defaulted (requires governance intervention)
    Defaulted {
        /// When default occurred (Unix timestamp)
        at: u64,
        /// Reason for default
        reason: String,
    },
}

/// Payment schedule for bond payments
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PaymentSchedule {
    /// All principal and interest paid at maturity
    #[default]
    Bullet,

    /// Regular interest payments, principal at maturity
    InterestOnly {
        /// Days between interest payments
        interval_days: u32,
    },

    /// Equal periodic payments (principal + interest)
    Amortizing {
        /// Days between payments
        interval_days: u32,
    },
}


/// Type of bond payment
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BondPaymentType {
    /// Interest payment only
    Interest,
    /// Principal payment only
    Principal,
    /// Combined interest and principal
    Combined,
}

/// Cooperative bond (debt security)
///
/// Bonds allow cooperatives to raise capital from other cooperatives
/// or members without sacrificing worker control. Key characteristics:
/// - Governance-approved issuance
/// - Administered interest rates (not market)
/// - Solidarity-based lending (stronger coops support developing ones)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CooperativeBond {
    /// Unique identifier
    pub id: BondId,

    /// Issuing cooperative
    pub issuer_id: String,

    /// Current holder (can be individual or cooperative DID)
    pub holder: Did,

    /// Principal amount
    pub principal: i64,

    /// Annual interest rate in basis points (100 = 1%)
    pub interest_rate_bps: u16,

    /// Maturity date (Unix timestamp)
    pub maturity_date: u64,

    /// Payment schedule
    pub payment_schedule: PaymentSchedule,

    /// Currency denomination
    pub currency: String,

    /// Current status
    pub status: BondStatus,

    /// Governance proposal that approved issuance
    pub approval_proposal: String,

    /// When this bond was created (Unix timestamp)
    pub created_at: u64,

    /// Payments made on this bond
    pub payments: Vec<BondPayment>,
}

/// Record of a bond payment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BondPayment {
    /// When paid (Unix timestamp)
    pub paid_at: u64,
    /// Type of payment
    pub payment_type: BondPaymentType,
    /// Amount paid
    pub amount: i64,
}

impl CooperativeBond {
    /// Create a new bond offering
    pub fn new_offering(
        id: BondId,
        issuer_id: String,
        holder: Did,
        principal: i64,
        interest_rate_bps: u16,
        maturity_date: u64,
        payment_schedule: PaymentSchedule,
        currency: String,
        approval_proposal: String,
        created_at: u64,
    ) -> Self {
        CooperativeBond {
            id,
            issuer_id,
            holder,
            principal,
            interest_rate_bps,
            maturity_date,
            payment_schedule,
            currency,
            status: BondStatus::Offering { subscribed: 0 },
            approval_proposal,
            created_at,
            payments: Vec::new(),
        }
    }

    /// Calculate interest for a period
    ///
    /// Returns the interest amount for the given number of days
    pub fn calculate_interest(&self, days: u32) -> i64 {
        // Interest = principal * rate * (days / 365)
        // Rate is in basis points (100 = 1%)
        let annual_interest = (self.principal * i64::from(self.interest_rate_bps)) / 10_000;
        (annual_interest * i64::from(days)) / 365
    }

    /// Check if this bond has reached maturity
    pub fn is_matured(&self, current_time: u64) -> bool {
        current_time >= self.maturity_date
    }

    /// Record a payment on this bond
    pub fn record_payment(&mut self, payment_type: BondPaymentType, amount: i64, at: u64) {
        self.payments.push(BondPayment {
            paid_at: at,
            payment_type,
            amount,
        });
    }

    /// Get total payments made
    pub fn total_paid(&self) -> i64 {
        self.payments.iter().map(|p| p.amount).sum()
    }

    /// Activate the bond (move from Offering to Active)
    pub fn activate(&mut self) {
        self.status = BondStatus::Active;
    }

    /// Mark the bond as matured (fully repaid)
    pub fn mark_matured(&mut self, at: u64) {
        self.status = BondStatus::Matured { completed_at: at };
    }

    /// Mark the bond as defaulted
    pub fn mark_defaulted(&mut self, at: u64, reason: String) {
        self.status = BondStatus::Defaulted { at, reason };
    }
}

/// Periodic surplus allocation to labor shareholders
///
/// This represents a governance-approved distribution of surplus
/// to all active labor shareholders in proportion to their labor days.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurplusAllocation {
    /// Unique identifier
    pub id: String,

    /// Cooperative performing the allocation
    pub cooperative_id: String,

    /// Total surplus being allocated
    pub total_surplus: i64,

    /// Period identifier (e.g., "2025-Q4")
    pub period: String,

    /// Calculated unit value: surplus / total_labor_days
    /// Each share receives: labor_days * share_unit_value
    pub share_unit_value: f64,

    /// Total labor days across all shareholders (denominator)
    pub total_labor_days: u64,

    /// Individual allocations: (share_id, amount)
    pub allocations: Vec<(ShareId, i64)>,

    /// Governance proposal that approved this allocation
    pub proposal_id: String,

    /// When allocation was performed (Unix timestamp)
    pub allocated_at: u64,

    /// Currency of the surplus
    pub currency: String,
}

impl SurplusAllocation {
    /// Create a new surplus allocation
    ///
    /// Calculates individual allocations based on labor days for each share.
    pub fn new(
        id: String,
        cooperative_id: String,
        total_surplus: i64,
        period: String,
        shares: &[LaborShare],
        proposal_id: String,
        allocated_at: u64,
        currency: String,
    ) -> Self {
        // Calculate total labor days
        let total_labor_days: u64 = shares.iter().map(|s| s.labor_days).sum();

        // Calculate unit value (surplus per labor day)
        let share_unit_value = if total_labor_days > 0 {
            (total_surplus as f64) / (total_labor_days as f64)
        } else {
            0.0
        };

        // Calculate individual allocations
        let allocations: Vec<(ShareId, i64)> = shares
            .iter()
            .filter(|s| s.is_active())
            .map(|s| {
                let allocation = (s.labor_days as f64 * share_unit_value) as i64;
                (s.id.clone(), allocation)
            })
            .collect();

        SurplusAllocation {
            id,
            cooperative_id,
            total_surplus,
            period,
            share_unit_value,
            total_labor_days,
            allocations,
            proposal_id,
            allocated_at,
            currency,
        }
    }

    /// Get the allocation amount for a specific share
    pub fn get_allocation(&self, share_id: &ShareId) -> Option<i64> {
        self.allocations
            .iter()
            .find(|(id, _)| id == share_id)
            .map(|(_, amount)| *amount)
    }
}

/// Bond offering for governance approval
///
/// This is the proposal payload submitted for governance vote
/// before a bond can be issued.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BondOffering {
    /// Issuing cooperative
    pub issuer_id: String,

    /// Principal amount requested
    pub principal_requested: i64,

    /// Proposed annual interest rate in basis points
    pub interest_rate_bps: u16,

    /// Term in days
    pub term_days: u32,

    /// Purpose of the bond (why capital is needed)
    pub purpose: String,

    /// Proposed payment schedule
    pub payment_schedule: PaymentSchedule,

    /// Currency for the bond
    pub currency: String,

    /// Any collateral offered
    pub collateral: Vec<Collateral>,
}

/// Collateral for a bond or loan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Collateral {
    /// Labor shares as collateral
    LaborShares {
        /// Share IDs pledged
        share_ids: Vec<ShareId>,
        /// Appraised value
        value: i64,
    },

    /// Other bonds as collateral
    Bonds {
        /// Bond IDs pledged
        bond_ids: Vec<BondId>,
        /// Appraised value
        value: i64,
    },

    /// Cooperative guarantee (another coop backing the bond)
    CoopGuarantee {
        /// Guaranteeing cooperative
        guarantor: String,
        /// Amount guaranteed
        amount: i64,
    },

    /// Physical equipment or assets
    Equipment {
        /// Description of equipment
        description: String,
        /// Appraised value
        value: i64,
    },
}

impl Collateral {
    /// Get the value of this collateral
    pub fn value(&self) -> i64 {
        match self {
            Collateral::LaborShares { value, .. } => *value,
            Collateral::Bonds { value, .. } => *value,
            Collateral::CoopGuarantee { amount, .. } => *amount,
            Collateral::Equipment { value, .. } => *value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    #[test]
    fn test_share_id_display() {
        let id = ShareId::new("share-001");
        assert_eq!(format!("{}", id), "share-001");
    }

    #[test]
    fn test_bond_id_display() {
        let id = BondId::new("bond-001");
        assert_eq!(format!("{}", id), "bond-001");
    }

    #[test]
    fn test_labor_share_creation() {
        let share = LaborShare::new(
            ShareId::new("share-001"),
            test_did(),
            "coop-001".to_string(),
            "hours".to_string(),
            1000,
        );

        assert_eq!(share.labor_days, 0);
        assert_eq!(share.accumulated_surplus, 0);
        assert!(share.is_active());
        assert_eq!(share.provenance.len(), 1);
    }

    #[test]
    fn test_labor_contribution() {
        let mut share = LaborShare::new(
            ShareId::new("share-001"),
            test_did(),
            "coop-001".to_string(),
            "hours".to_string(),
            1000,
        );

        share.record_labor(10, 1001, Some("Week 1 work".to_string()));
        share.record_labor(8, 1002, None);

        assert_eq!(share.labor_days, 18);
        assert_eq!(share.provenance.len(), 3);
    }

    #[test]
    fn test_surplus_allocation() {
        let mut share = LaborShare::new(
            ShareId::new("share-001"),
            test_did(),
            "coop-001".to_string(),
            "hours".to_string(),
            1000,
        );

        share.record_labor(100, 1001, None);
        share.allocate_surplus(500, "2025-Q4".to_string(), 2000);

        assert_eq!(share.accumulated_surplus, 500);
        assert_eq!(share.current_value(), 500);
    }

    #[test]
    fn test_share_redemption() {
        let mut share = LaborShare::new(
            ShareId::new("share-001"),
            test_did(),
            "coop-001".to_string(),
            "hours".to_string(),
            1000,
        );

        share.allocate_surplus(1000, "2025-Q4".to_string(), 2000);

        let schedule = vec![
            ScheduledPayout::new(3000, 500),
            ScheduledPayout::new(4000, 500),
        ];

        share.start_redemption(schedule, "proposal-001".to_string(), 2500);

        assert!(!share.is_active());
        assert!(matches!(share.status, ShareStatus::Redeeming { .. }));
    }

    #[test]
    fn test_bond_interest_calculation() {
        let bond = CooperativeBond::new_offering(
            BondId::new("bond-001"),
            "coop-001".to_string(),
            test_did(),
            10000,  // principal
            500,    // 5% annual rate
            100000, // maturity
            PaymentSchedule::Bullet,
            "hours".to_string(),
            "proposal-001".to_string(),
            1000,
        );

        // 365 days at 5% on 10000 = 500
        assert_eq!(bond.calculate_interest(365), 500);

        // 30 days at 5% on 10000 = ~41
        assert_eq!(bond.calculate_interest(30), 41);
    }

    #[test]
    fn test_surplus_allocation_distribution() {
        let holder = test_did();
        let shares = vec![
            {
                let mut s = LaborShare::new(
                    ShareId::new("share-001"),
                    holder.clone(),
                    "coop-001".to_string(),
                    "hours".to_string(),
                    1000,
                );
                s.labor_days = 100;
                s
            },
            {
                let mut s = LaborShare::new(
                    ShareId::new("share-002"),
                    holder.clone(),
                    "coop-001".to_string(),
                    "hours".to_string(),
                    1000,
                );
                s.labor_days = 200;
                s
            },
        ];

        let allocation = SurplusAllocation::new(
            "alloc-001".to_string(),
            "coop-001".to_string(),
            300, // total surplus
            "2025-Q4".to_string(),
            &shares,
            "proposal-001".to_string(),
            2000,
            "hours".to_string(),
        );

        // share-001 has 100/300 = 1/3 of labor days -> 100 units
        assert_eq!(
            allocation.get_allocation(&ShareId::new("share-001")),
            Some(100)
        );

        // share-002 has 200/300 = 2/3 of labor days -> 200 units
        assert_eq!(
            allocation.get_allocation(&ShareId::new("share-002")),
            Some(200)
        );
    }

    #[test]
    fn test_collateral_value() {
        let shares_collateral = Collateral::LaborShares {
            share_ids: vec![ShareId::new("s1")],
            value: 1000,
        };
        assert_eq!(shares_collateral.value(), 1000);

        let guarantee = Collateral::CoopGuarantee {
            guarantor: "coop-002".to_string(),
            amount: 5000,
        };
        assert_eq!(guarantee.value(), 5000);
    }
}
