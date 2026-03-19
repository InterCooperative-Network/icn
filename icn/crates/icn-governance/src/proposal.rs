//! Proposal types and state machine for cooperative governance.
//!
//! This module implements the proposal lifecycle for ICN governance, supporting
//! democratic decision-making in cooperatives with optional deliberation phases,
//! emergency actions, and comprehensive state tracking.
//!
//! # Proposal State Machine
//!
//! Proposals move through a defined set of states with explicit transitions.
//! The state machine supports two paths: standard (with deliberation) and
//! expedited (skipping deliberation for emergency or simple proposals).
//!
//! ## State Diagram
//!
//! ```text
//!                              ┌─────────────────────┐
//!                              │                     │
//!                              │        Draft        │
//!                              │                     │
//!                              └──────────┬──────────┘
//!                                         │
//!              ┌──────────────────────────┼──────────────────────────┐
//!              │ start_deliberation()     │ open()                   │
//!              ▼                          ▼                          │
//!   ┌──────────────────────┐   ┌──────────────────────┐              │
//!   │                      │   │                      │              │
//!   │    Deliberation      │   │        Open          │              │
//!   │  (discussion only)   │──▶│   (voting active)    │              │
//!   │                      │   │                      │              │
//!   └──────────────────────┘   └──────────┬──────────┘              │
//!     │  end_deliberation_and_open()      │                          │
//!     │  force_end_deliberation()         │ close()                  │
//!     │                                   │                          │
//!     │                    ┌──────────────┼──────────────┐           │
//!     │                    ▼              ▼              ▼           │
//!     │         ┌──────────────┐ ┌──────────────┐ ┌──────────────┐   │
//!     │         │   Accepted   │ │   Rejected   │ │   NoQuorum   │   │
//!     │         │  (terminal)  │ │  (terminal)  │ │  (terminal)  │   │
//!     │         └──────────────┘ └──────────────┘ └──────────────┘   │
//!     │                                                              │
//!     │
//!     │  Emergency actions available from any non-terminal state:
//!     │  ┌─────────────────────────────────────────────────────────────┐
//!     │  │  cancel() → Cancelled    (from Draft, Deliberation, Open)  │
//!     │  │  veto() → Vetoed         (from Draft, Deliberation, Open)  │
//!     │  │  force_close() → ForceClosed  (from Deliberation, Open)    │
//!     └──┴─────────────────────────────────────────────────────────────┘
//!                      │                │               │
//!                      ▼                ▼               ▼
//!           ┌──────────────┐ ┌──────────────┐ ┌──────────────┐
//!           │  Cancelled   │ │    Vetoed    │ │ ForceClosed  │
//!           │  (terminal)  │ │  (terminal)  │ │  (terminal)  │
//!           └──────────────┘ └──────────────┘ └──────────────┘
//! ```
//!
//! ## State Descriptions
//!
//! | State | Description | Can Vote | Can Comment |
//! |-------|-------------|----------|-------------|
//! | `Draft` | Initial state, proposal not yet submitted | No | No |
//! | `Deliberation` | Discussion phase before voting | No | Yes |
//! | `Open` | Active voting period | Yes | Yes |
//! | `Accepted` | Voting closed, proposal passed | No | No |
//! | `Rejected` | Voting closed, proposal failed | No | No |
//! | `NoQuorum` | Voting closed, insufficient participation | No | No |
//! | `Cancelled` | Proposer withdrew the proposal | No | No |
//! | `Vetoed` | Emergency override by authorized party | No | No |
//! | `ForceClosed` | Administratively closed with forced outcome | No | No |
//!
//! ## Valid State Transitions
//!
//! | From | To | Method | Conditions |
//! |------|-----|--------|------------|
//! | `Draft` | `Deliberation` | `start_deliberation()` | - |
//! | `Draft` | `Open` | `open()` | Skips deliberation |
//! | `Draft` | `Cancelled` | `cancel()` | Proposer request |
//! | `Draft` | `Vetoed` | `veto()` | Emergency action |
//! | `Deliberation` | `Open` | `end_deliberation_and_open()` | Deliberation period ended |
//! | `Deliberation` | `Open` | `force_end_deliberation()` | Administrative override |
//! | `Deliberation` | `Cancelled` | `cancel()` | Proposer request |
//! | `Deliberation` | `Vetoed` | `veto()` | Emergency action |
//! | `Deliberation` | `ForceClosed` | `force_close()` | Emergency action |
//! | `Open` | `Accepted` | `close()` | Quorum met, majority yes |
//! | `Open` | `Rejected` | `close()` | Quorum met, majority no |
//! | `Open` | `NoQuorum` | `close()` | Quorum not met |
//! | `Open` | `Cancelled` | `cancel()` | Proposer request |
//! | `Open` | `Vetoed` | `veto()` | Emergency action |
//! | `Open` | `ForceClosed` | `force_close()` | Emergency action |
//!
//! **Note:** The `cancel()`, `veto()`, and `force_close()` methods can be invoked
//! from any non-terminal state (i.e., any state where `is_closed()` returns false).
//! In the current lifecycle, the non-terminal states are `Draft`, `Deliberation`,
//! and `Open`.
//!
//! ## Example Lifecycle
//!
//! ### Standard Proposal (with deliberation)
//!
//! ```rust
//! use icn_governance::{Proposal, ProposalPayload, ProposalState, GovernanceDomainId};
//! use icn_identity::KeyPair;
//!
//! // Create a proposal
//! let kp = KeyPair::generate().unwrap();
//! let mut proposal = Proposal::new(
//!     GovernanceDomainId::new("coop-alpha"),
//!     kp.did().clone(),
//!     "Increase member dues".to_string(),
//!     "Proposal to increase monthly dues by 5%".to_string(),
//!     ProposalPayload::Text { body: "...".to_string() },
//! );
//! assert!(proposal.state.is_draft());
//!
//! // Start deliberation (7 days)
//! proposal.start_deliberation(7 * 24 * 3600).unwrap();
//! assert!(proposal.state.is_deliberating());
//!
//! // After deliberation period expires naturally, use:
//! //   proposal.end_deliberation_and_open(voting_period).unwrap();
//! // For this doctest, we force the transition:
//! proposal.force_end_deliberation(5 * 24 * 3600).unwrap();
//! assert!(proposal.state.is_open());
//!
//! // After voting closes, close with outcome
//! let now = icn_time::current_timestamp_secs();
//! proposal.close(ProposalState::Accepted { closed_at: now }).unwrap();
//! assert!(proposal.state.is_closed());
//! ```
//!
//! ### Emergency Proposal (skipping deliberation)
//!
//! ```rust
//! use icn_governance::{Proposal, ProposalPayload, ProposalState, GovernanceDomainId};
//! use icn_identity::KeyPair;
//!
//! let kp = KeyPair::generate().unwrap();
//! let mut proposal = Proposal::new(
//!     GovernanceDomainId::new("coop-alpha"),
//!     kp.did().clone(),
//!     "Emergency: Freeze compromised account".to_string(),
//!     "Security incident requires immediate action".to_string(),
//!     ProposalPayload::FreezeMember {
//!         member: kp.did().clone(),
//!         reason: "Account compromised".to_string(),
//!         duration_seconds: Some(86400),
//!     },
//! );
//!
//! // Skip deliberation, open immediately (24 hour voting)
//! proposal.open(24 * 3600).unwrap();
//! assert!(proposal.state.is_open());
//! ```
//!
//! ### Cancellation
//!
//! ```rust
//! use icn_governance::{Proposal, ProposalPayload, ProposalState, GovernanceDomainId};
//! use icn_identity::KeyPair;
//!
//! let kp = KeyPair::generate().unwrap();
//! let mut proposal = Proposal::new(
//!     GovernanceDomainId::new("coop-alpha"),
//!     kp.did().clone(),
//!     "Some proposal".to_string(),
//!     "Description".to_string(),
//!     ProposalPayload::Text { body: "...".to_string() },
//! );
//!
//! // Proposer decides to cancel
//! proposal.cancel().unwrap();
//! assert!(matches!(proposal.state, ProposalState::Cancelled { .. }));
//! assert!(proposal.state.is_closed());
//! ```
//!
//! # Terminal States
//!
//! All states except `Draft`, `Deliberation`, and `Open` are terminal.
//! Once a proposal reaches a terminal state, no further transitions are possible.
//!
//! | Terminal State | Outcome | Execution |
//! |----------------|---------|-----------|
//! | `Accepted` | Proposal enacted | Payload executed |
//! | `Rejected` | Proposal defeated | No action |
//! | `NoQuorum` | Insufficient votes | No action |
//! | `Cancelled` | Proposer withdrew | No action |
//! | `Vetoed` | Emergency override | No action |
//! | `ForceClosed` | Administrative | Per forced outcome |
//!
//! # Deliberation Phase
//!
//! The deliberation phase is an optional period between `Draft` and `Open` where
//! members can discuss and refine the proposal before voting begins. This supports:
//!
//! - **Informed decision-making**: Members understand implications before voting
//! - **Amendment opportunity**: Proposer can modify based on feedback
//! - **Conflict resolution**: Issues identified before binding vote
//!
//! Use `open()` to skip deliberation for:
//! - Emergency proposals requiring immediate action
//! - Simple procedural matters
//! - Time-sensitive decisions
//!
//! **Note:** This state machine does not define a dedicated amendment API for
//! in-place modification of proposals during deliberation. Amendments are typically
//! handled by cancelling and resubmitting a revised proposal, or via separate
//! governance processes defined at the cooperative level.
//!
//! # Emergency Actions
//!
//! Three methods enable emergency governance:
//!
//! - **`cancel()`**: Proposer withdraws their own proposal
//! - **`veto()`**: Authorized party blocks a proposal (requires emergency quorum)
//! - **`force_close()`**: Administratively close with specified outcome
//!
//! These are intended for exceptional circumstances and typically require
//! higher approval thresholds than normal proposals.

use crate::domain::GovernanceDomainId;
use crate::Timestamp;
use icn_federation::SettlementInterval;
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Action for resource access proposal
///
/// The model is embedded in the Grant variant to ensure at compile-time
/// that all grant proposals include an access model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceAccessAction {
    /// Grant access to a resource with the specified access model
    Grant {
        /// The access model defining duration, renewability, and duties
        model: icn_ledger::AccessModel,
    },
    /// Revoke access from a resource
    Revoke,
}

/// Unique identifier for a proposal
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProposalId(pub String);

impl ProposalId {
    /// Generate a new random proposal ID
    pub fn generate() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Create from an existing ID string
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for ProposalId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// State of a proposal in its lifecycle
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposalState {
    /// Draft proposal, not yet open for discussion or voting
    Draft,

    /// Open for deliberation (discussion before voting)
    ///
    /// During deliberation, members can comment, discuss, and refine the proposal.
    /// No voting occurs during this phase. When deliberation ends, the proposal
    /// transitions to Open for voting.
    Deliberation {
        /// When deliberation started
        started_at: Timestamp,
        /// When deliberation ends (and voting can begin)
        ends_at: Timestamp,
    },

    /// Open for voting
    Open {
        /// When voting opened
        opened_at: Timestamp,
        /// When voting closes
        closes_at: Timestamp,
    },

    /// Voting closed, proposal accepted
    Accepted {
        /// When voting closed
        closed_at: Timestamp,
    },

    /// Voting closed, proposal rejected
    Rejected {
        /// When voting closed
        closed_at: Timestamp,
    },

    /// Voting closed, quorum not met
    NoQuorum {
        /// When voting closed
        closed_at: Timestamp,
    },

    /// Proposal cancelled by author
    Cancelled {
        /// When cancelled
        cancelled_at: Timestamp,
    },

    /// Proposal vetoed by emergency governance action
    Vetoed {
        /// When vetoed
        vetoed_at: Timestamp,
        /// Reason for veto
        reason: String,
    },

    /// Proposal force-closed before voting period ended
    ForceClosed {
        /// When force-closed
        closed_at: Timestamp,
        /// Forced outcome
        outcome: super::ProposalOutcome,
        /// Reason for force close
        reason: String,
    },
}

impl ProposalState {
    /// Check if proposal is in draft state
    pub fn is_draft(&self) -> bool {
        matches!(self, ProposalState::Draft)
    }

    /// Check if proposal is in deliberation phase
    pub fn is_deliberating(&self) -> bool {
        matches!(self, ProposalState::Deliberation { .. })
    }

    /// Check if proposal is open for voting
    pub fn is_open(&self) -> bool {
        matches!(self, ProposalState::Open { .. })
    }

    /// Check if proposal allows comments (deliberation or open for voting)
    pub fn allows_comments(&self) -> bool {
        matches!(
            self,
            ProposalState::Deliberation { .. } | ProposalState::Open { .. }
        )
    }

    /// Check if proposal is closed (any terminal state)
    pub fn is_closed(&self) -> bool {
        matches!(
            self,
            ProposalState::Accepted { .. }
                | ProposalState::Rejected { .. }
                | ProposalState::NoQuorum { .. }
                | ProposalState::Cancelled { .. }
                | ProposalState::Vetoed { .. }
                | ProposalState::ForceClosed { .. }
        )
    }

    /// Get the timestamp when deliberation ends (if deliberating)
    pub fn deliberation_ends_at(&self) -> Option<Timestamp> {
        match self {
            ProposalState::Deliberation { ends_at, .. } => Some(*ends_at),
            _ => None,
        }
    }

    /// Get the timestamp when voting closes (if open)
    pub fn closes_at(&self) -> Option<Timestamp> {
        match self {
            ProposalState::Open { closes_at, .. } => Some(*closes_at),
            _ => None,
        }
    }

    /// Check if deliberation period has ended (for transition to voting)
    pub fn can_end_deliberation(&self, current_time: Timestamp) -> bool {
        match self {
            ProposalState::Deliberation { ends_at, .. } => current_time >= *ends_at,
            _ => false,
        }
    }
}

/// A single option within a participatory budget allocation.
///
/// Each option represents a competing use for the resource pool. Members
/// vote to distribute the pool across these options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AllocationOption {
    /// Short label for this option (e.g., "Infrastructure", "Education").
    pub label: String,
    /// Longer description of what the allocation would fund.
    pub description: String,
    /// Recipient DID (the entity that would receive this allocation).
    pub recipient: Did,
    /// Requested amount from the pool. The sum of all requested amounts
    /// may exceed the pool (competing claims).
    pub requested_amount: i64,
}

/// Payload of a proposal (what is being decided)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProposalPayload {
    /// Free-form text proposal
    Text {
        /// Full text of the proposal
        body: String,
    },

    /// Budget allocation
    Budget {
        /// Amount to allocate
        amount: i64,
        /// Currency
        currency: String,
        /// Recipient DID
        recipient: Did,
        /// Purpose
        purpose: String,
    },

    /// Participatory budget allocation — distribute a pool among competing options.
    ///
    /// Unlike `Budget` (single-recipient), `Allocation` presents a pool of
    /// resources and a set of `AllocationOption`s. Members vote to distribute
    /// the pool across options. This is the participatory budgeting primitive:
    /// "resource scheduling" semantics, not payment semantics.
    ///
    /// The tally engine counts votes per option; the accepted allocation is
    /// recorded as a governance decision with a `decision_hash` that downstream
    /// actors (e.g., `ExecutionReceiptGate`) can reference.
    Allocation {
        /// Total pool to distribute (in the specified unit).
        pool_amount: i64,
        /// Unit of account for the pool (e.g., "compute-hours", "participation-units").
        unit: String,
        /// Competing allocation options. Must contain at least 2 options.
        options: Vec<AllocationOption>,
        /// Purpose or context for this allocation round.
        purpose: String,
    },

    /// Membership change
    Membership {
        /// Add or remove
        action: MembershipAction,
        /// Member DID
        member: Did,
    },

    /// Governance config change
    ConfigChange {
        /// New config (JSON-encoded)
        new_config: String,
    },

    /// Cooperative scheduling policy update (Phase 16E integration)
    SchedulingPolicy {
        /// Cooperative identifier
        coop_id: String,
        /// New policy (JSON-encoded CoopSchedulingPolicy)
        policy_json: String,
    },

    // === Emergency Proposals (Issue #25) ===
    /// Freeze a member - blocks all ledger transactions for the member
    ///
    /// This is an emergency action requiring super-majority approval.
    /// Used when a member's account may be compromised or they're acting maliciously.
    FreezeMember {
        /// Member DID to freeze
        member: Did,
        /// Reason for freezing
        reason: String,
        /// Duration in seconds (None = indefinite until unfrozen)
        duration_seconds: Option<u64>,
    },

    /// Unfreeze a previously frozen member
    UnfreezeMember {
        /// Member DID to unfreeze
        member: Did,
        /// Reason for unfreezing
        reason: String,
    },

    /// Veto an existing proposal before it closes
    ///
    /// Requires super-majority to override normal governance process.
    VetoProposal {
        /// ID of proposal to veto
        target_proposal_id: String,
        /// Reason for veto
        reason: String,
    },

    /// Force close an open proposal immediately
    ///
    /// Used in emergencies when a proposal must be halted.
    ForceCloseProposal {
        /// ID of proposal to force close
        target_proposal_id: String,
        /// Reason for force closing
        reason: String,
        /// Outcome to set (Accepted, Rejected, or NoQuorum)
        forced_outcome: ForcedOutcome,
    },

    /// Rollback ledger to a specific state
    ///
    /// This is the most severe emergency action - requires highest threshold.
    /// Should only be used when ledger corruption or fraud is detected.
    RollbackLedger {
        /// Hash of the entry to roll back to
        target_hash: String,
        /// Reason for rollback
        reason: String,
        /// List of affected accounts (for notification)
        affected_accounts: Vec<Did>,
    },

    // === Dispute Escalation (Issue #52) ===
    /// Escalated dispute resolution requiring community vote
    ///
    /// When a ledger dispute cannot be resolved by mediators (large value,
    /// conflict of interest, or rejected decision), it escalates to governance.
    DisputeResolution {
        /// Hash of the disputed ledger entry (links to icn-ledger dispute)
        dispute_entry_hash: String,
        /// Original filer of the dispute
        filer: Did,
        /// Original reason for the dispute
        reason: String,
        /// Reason for escalation (why mediator couldn't resolve)
        escalation_reason: String,
        /// Proposed resolution outcome if the proposal passes
        proposed_outcome: DisputeResolutionOutcome,
    },

    // === SDIS Governance (Phase S6) ===
    /// SDIS-specific governance proposal
    ///
    /// Handles steward management, threshold modifications, and identity
    /// governance through the SDIS module.
    Sdis {
        /// The SDIS-specific proposal
        proposal: crate::sdis::SdisProposal,
    },

    // === Protocol Upgrade (Phase 19.1) ===
    /// Protocol version upgrade proposal
    ///
    /// Enables governance-driven protocol upgrades with:
    /// - Version tracking and adoption monitoring
    /// - Migration guide for operators
    /// - Deadline enforcement for old versions
    /// - Breaking change documentation
    ///
    /// Requires super-majority (0.66) for breaking changes.
    ProtocolUpgrade {
        /// New protocol version (semantic versioning)
        version: Version,
        /// Breaking changes description
        breaking_changes: Vec<String>,
        /// Migration guide URL
        migration_guide: Option<String>,
        /// Upgrade deadline (Unix timestamp)
        /// Nodes below min_required_version will be rejected after this
        deadline: u64,
        /// Minimum version required after deadline
        /// If None, no enforcement (non-breaking upgrade)
        min_required_version: Option<Version>,
    },

    // === Protocol Parameter Change (Phase 20) ===
    /// Protocol parameter change proposal
    ///
    /// Enables governance-controlled modification of protocol parameters.
    /// Parameters can be scoped to:
    /// - Global (network-wide defaults)
    /// - Federation (federation-specific override)
    /// - Cooperative (cooperative-specific override)
    ///
    /// Parameters are validated against constraints before application.
    /// Changes are recorded with full history for auditing.
    ProtocolChange {
        /// The parameter change proposal
        proposal: crate::protocol::ProtocolChangeProposal,
    },

    // === Treasury Operations (Issue #246) ===
    /// Treasury management proposal
    ///
    /// Enables governance-controlled treasury operations including:
    /// - Budget allocation and management
    /// - Large withdrawals requiring approval
    /// - Spending rule modifications
    /// - Budget transfers and cancellations
    ///
    /// Approval thresholds vary by operation type.
    Treasury {
        /// The treasury operation to execute
        operation: TreasuryProposalOperation,
    },

    // === Labor Share Operations (Issue #389) ===
    /// Approve periodic surplus allocation to labor shareholders
    ///
    /// Distributes cooperative surplus to active labor shareholders in
    /// proportion to their labor days. This is Razeto's core mechanism
    /// for equitable value distribution.
    ///
    /// Requires quorum vote from membership. Upon approval, the surplus
    /// is allocated to each share based on their labor contribution ratio.
    SurplusAllocation {
        /// The calculated surplus allocation
        allocation: icn_ledger::SurplusAllocation,
    },

    /// Approve share redemption for a departing member
    ///
    /// When a member leaves the cooperative, their labor shares must be
    /// redeemed at unit value. Governance approves the payout schedule
    /// to ensure treasury can meet obligations.
    ///
    /// Can be immediate or installment-based depending on treasury capacity.
    ShareRedemption {
        /// Member whose shares are being redeemed
        member: Did,
        /// Share IDs to redeem
        share_ids: Vec<icn_ledger::ShareId>,
        /// Approved payout schedule
        payout_schedule: Vec<icn_ledger::ScheduledPayout>,
        /// Reason for redemption (voluntary departure, retirement, etc.)
        reason: String,
    },

    /// Approve bond issuance for cooperative financing
    ///
    /// Cooperatives can issue bonds to raise capital from other cooperatives
    /// or members. This enables solidarity-based lending where stronger
    /// cooperatives support developing ones.
    ///
    /// Requires governance approval with purpose justification.
    BondIssuance {
        /// The bond offering details
        bond_offering: icn_ledger::BondOffering,
    },

    // === Federation Governance (Issue #514) ===
    /// Federation relationship management
    ///
    /// Enables governance-controlled management of federation relationships
    /// including joining/leaving federations, establishing clearing agreements,
    /// and vouching for other cooperatives.
    ///
    /// Federation actions require member approval to ensure democratic control
    /// over inter-cooperative relationships.
    Federation(FederationProposal),

    // === Use-Based Resource Access (anti-rent-seeking) ===
    /// Resource access grant or revocation
    ///
    /// Implements use-based access model to prevent rent-seeking and speculation.
    /// Resources are accessed based on active use rather than passive ownership.
    ///
    /// Access models:
    /// - UseAccess: Time-limited, renewable access with accumulation limits
    /// - Stewardship: Access tied to duties and responsibilities
    ///
    /// Enforces anti-speculation rules:
    /// - Use it or lose it (idle revocation)
    /// - No profit transfer (can't sell access for profit)
    ///
    /// Note: The access model is embedded in the `action` field for `Grant` actions,
    /// ensuring type-safe proposals (can't create a Grant without a model).
    ResourceAccess {
        /// Grant (with model) or revoke access
        action: ResourceAccessAction,
        /// Resource identifier
        resource_id: String,
        /// Entity receiving/losing access
        holder: icn_entity::EntityId,
        /// Reason for grant/revocation
        reason: String,
    },

    /// Charter ratification — members vote to adopt a CCL charter document.
    ///
    /// On acceptance the charter YAML is deployed to the `CharterPolicyOracle`,
    /// making it immediately enforceable by the kernel.
    Charter {
        /// Stable identifier that the oracle will use to key this charter.
        /// Typically the cooperative's DID or a human-readable name.
        charter_id: String,
        /// Complete YAML charter document (CCL format, schema_version: v0).
        charter_yaml: String,
    },
}

impl ProposalPayload {
    /// Get a string name for the proposal type (for metrics labeling)
    pub fn type_name(&self) -> &'static str {
        match self {
            ProposalPayload::Text { .. } => "text",
            ProposalPayload::Budget { .. } => "budget",
            ProposalPayload::Allocation { .. } => "allocation",
            ProposalPayload::Membership { .. } => "membership",
            ProposalPayload::ConfigChange { .. } => "config_change",
            ProposalPayload::SchedulingPolicy { .. } => "scheduling_policy",
            ProposalPayload::FreezeMember { .. } => "freeze_member",
            ProposalPayload::UnfreezeMember { .. } => "unfreeze_member",
            ProposalPayload::VetoProposal { .. } => "veto_proposal",
            ProposalPayload::ForceCloseProposal { .. } => "force_close_proposal",
            ProposalPayload::RollbackLedger { .. } => "rollback_ledger",
            ProposalPayload::DisputeResolution { .. } => "dispute_resolution",
            ProposalPayload::Sdis { .. } => "sdis",
            ProposalPayload::ProtocolUpgrade { .. } => "protocol_upgrade",
            ProposalPayload::ProtocolChange { .. } => "protocol_change",
            ProposalPayload::Treasury { .. } => "treasury",
            ProposalPayload::SurplusAllocation { .. } => "surplus_allocation",
            ProposalPayload::ShareRedemption { .. } => "share_redemption",
            ProposalPayload::BondIssuance { .. } => "bond_issuance",
            ProposalPayload::Federation(_) => "federation",
            ProposalPayload::ResourceAccess { .. } => "resource_access",
            ProposalPayload::Charter { .. } => "charter",
        }
    }

    /// Get the emergency type if this is an emergency proposal
    ///
    /// Returns Some(emergency_type) for freeze, veto, force_close, and rollback proposals.
    /// Returns None for normal proposals.
    pub fn emergency_type(&self) -> Option<&'static str> {
        match self {
            ProposalPayload::FreezeMember { .. } | ProposalPayload::UnfreezeMember { .. } => {
                Some("freeze")
            }
            ProposalPayload::VetoProposal { .. } => Some("veto"),
            ProposalPayload::ForceCloseProposal { .. } => Some("force_close"),
            ProposalPayload::RollbackLedger { .. } => Some("rollback"),
            _ => None,
        }
    }

    /// Check if this is an emergency proposal (requires higher quorum)
    pub fn is_emergency(&self) -> bool {
        self.emergency_type().is_some()
    }
}

/// Treasury operations that require governance approval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TreasuryProposalOperation {
    /// Create a new budget allocation from treasury
    CreateBudget {
        /// Treasury DID
        treasury_did: Did,
        /// Purpose of the budget
        purpose: String,
        /// Amount to allocate
        amount: i64,
        /// Currency
        currency: String,
        /// Budget period end (None = indefinite)
        period_end: Option<u64>,
    },

    /// Modify spending rules for a treasury
    ModifySpendingRule {
        /// Treasury DID
        treasury_did: Did,
        /// Rule ID to modify (None = create new)
        rule_id: Option<String>,
        /// Rule name
        name: String,
        /// Threshold amount
        threshold_amount: i64,
        /// Currency
        currency: String,
        /// Required approval type
        approval_type: TreasuryApprovalType,
        /// Whether the rule is active
        is_active: bool,
    },

    /// Withdraw funds from treasury (for amounts requiring governance approval)
    Withdraw {
        /// Treasury DID
        treasury_did: Did,
        /// Recipient DID
        recipient: Did,
        /// Amount to withdraw
        amount: i64,
        /// Currency
        currency: String,
        /// Purpose of withdrawal
        purpose: String,
        /// Optional budget to charge against
        budget_id: Option<String>,
        /// Expected treasury nonce at execution time.
        ///
        /// Defaults to 0 for backwards-compatible deserialization of older
        /// proposals that did not carry a nonce field.
        #[serde(default)]
        nonce: u64,
    },

    /// Transfer funds between budgets
    TransferBetweenBudgets {
        /// Treasury DID
        treasury_did: Did,
        /// Source budget ID
        from_budget: String,
        /// Destination budget ID
        to_budget: String,
        /// Amount to transfer
        amount: i64,
        /// Currency
        currency: String,
        /// Reason for transfer
        reason: String,
    },

    /// Cancel a budget and optionally return funds to treasury
    CancelBudget {
        /// Budget ID to cancel
        budget_id: String,
        /// Reason for cancellation
        reason: String,
        /// Whether to return remaining funds to treasury
        return_to_treasury: bool,
    },

    /// Reclaim unspent budget funds back to treasury
    ReclaimBudget {
        /// Budget ID
        budget_id: String,
        /// Amount to reclaim
        amount: i64,
        /// Currency
        currency: String,
        /// Reason for reclamation
        reason: String,
    },

    /// Direct treasury spend approved through governance
    ///
    /// A simplified treasury disbursement that transfers funds directly from
    /// the cooperative treasury to a recipient. Unlike `Withdraw`, this does
    /// not require specifying a budget -- the spend is charged against the
    /// treasury's unallocated balance.
    ///
    /// Requires governance approval (same thresholds as `Withdraw`).
    Spend {
        /// Treasury DID
        treasury_did: Did,
        /// Amount to spend (must be positive)
        amount: i64,
        /// Currency
        currency: String,
        /// Recipient DID (must be a valid `did:icn:` identifier)
        recipient: Did,
        /// Human-readable description of the spend (must be non-empty)
        memo: String,
        /// Expected treasury nonce at execution time.
        ///
        /// The nonce is checked atomically before the ledger transfer.
        /// If the stored nonce does not match, the spend is rejected as
        /// a replay / double-spend attempt. Defaults to 0 for backward
        /// compatibility with proposals created before nonce support.
        #[serde(default)]
        nonce: u64,
    },
}

/// Type of approval required for treasury spending
/// Mirrors the ledger ApprovalType but defined here for governance independence
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TreasuryApprovalType {
    /// No approval needed (within threshold)
    None,
    /// Simple majority vote (>50%)
    SimpleMajority,
    /// Super-majority (>66%)
    SuperMajority,
    /// Board/council approval only
    BoardOnly,
    /// Emergency threshold (75%+)
    Emergency,
}

/// Semantic version for protocol versioning (re-exported from kernel API)
pub use icn_kernel_api::Version;

/// Possible outcomes for a dispute resolution governance proposal
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisputeResolutionOutcome {
    /// Uphold the dispute - original entry was wrong
    Uphold,
    /// Reject the dispute - original entry was correct
    Reject,
    /// Partial resolution with adjustments
    Partial {
        /// Adjustment amount (positive = credit to filer, negative = credit to counterparty)
        adjustment: i64,
        /// Currency of adjustment
        currency: String,
    },
    /// Void the transaction entirely
    VoidTransaction,
}

/// Forced outcome for emergency proposal closure
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ForcedOutcome {
    /// Force accept the proposal
    Accept,
    /// Force reject the proposal
    Reject,
    /// Cancel without outcome
    Cancel,
}

/// Membership action
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MembershipAction {
    Add,
    Remove,
}

// === Federation Governance (Issue #514) ===

/// Terms for joining a federation
///
/// Specifies the obligations and expectations when a cooperative joins a federation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FederationTerms {
    /// Required trust threshold for federation membership (0.0-1.0)
    pub min_trust_threshold: f64,
    /// Whether the cooperative agrees to follow federation governance
    pub governance_binding: bool,
    /// Data sharing level (none, metadata_only, full)
    pub data_sharing_level: DataSharingLevel,
    /// Dispute resolution method
    pub dispute_resolution: DisputeResolutionMethod,
}

impl Default for FederationTerms {
    fn default() -> Self {
        Self {
            min_trust_threshold: 0.5,
            governance_binding: true,
            data_sharing_level: DataSharingLevel::MetadataOnly,
            dispute_resolution: DisputeResolutionMethod::FederationMediation,
        }
    }
}

impl FederationTerms {
    /// Validate the federation terms
    ///
    /// Returns an error message if validation fails.
    pub fn validate(&self) -> Result<(), String> {
        // Validate trust threshold range
        if !(0.0..=1.0).contains(&self.min_trust_threshold) || !self.min_trust_threshold.is_finite()
        {
            return Err(format!(
                "min_trust_threshold must be between 0.0 and 1.0, got {}",
                self.min_trust_threshold
            ));
        }

        // Validate dispute resolution-specific fields
        if let DisputeResolutionMethod::ArbitratorCooperative { arbitrator_id } =
            &self.dispute_resolution
        {
            if arbitrator_id.is_empty() {
                return Err(
                    "arbitrator_id must be non-empty when using ArbitratorCooperative dispute resolution"
                        .to_string(),
                );
            }
        }

        Ok(())
    }
}

/// Level of data sharing agreed to in federation terms
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DataSharingLevel {
    /// No automatic data sharing
    None,
    /// Share metadata only (activity summaries, not individual transactions)
    #[default]
    MetadataOnly,
    /// Full transparency (all ledger data visible to federation)
    Full,
}

/// Method for resolving disputes within a federation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DisputeResolutionMethod {
    /// Disputes escalate to federation mediators
    #[default]
    FederationMediation,
    /// Disputes resolved by designated arbitrator cooperative
    ArbitratorCooperative { arbitrator_id: String },
    /// Disputes resolved by member vote across federation
    FederationVote,
}

/// Federation-related governance proposals
///
/// These proposals enable governance-controlled management of federation relationships.
/// All federation actions require member approval through the proposal system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FederationProposal {
    /// Join an existing federation network
    ///
    /// Requires member approval as it commits the cooperative to federation rules.
    /// The cooperative will be announced to the federation registry upon approval.
    JoinFederation {
        /// Federation network identifier
        federation_id: String,
        /// Terms agreed to for joining
        terms: FederationTerms,
        /// Optional sponsor cooperative that vouches for the joining coop
        sponsor_coop_id: Option<String>,
    },

    /// Leave a federation network
    ///
    /// Triggers wind-down process including settlement of pending clearing balances.
    /// The cooperative will be removed from the federation registry.
    LeaveFederation {
        /// Federation network identifier
        federation_id: String,
        /// Reason for leaving
        reason: String,
        /// Grace period in days before full departure (for settlement)
        grace_period_days: u32,
    },

    /// Establish bilateral clearing agreement with another cooperative
    ///
    /// Sets up the ability to transact across cooperative boundaries with
    /// periodic settlement of balances.
    EstablishClearing {
        /// Partner cooperative's identifier
        partner_coop_id: String,
        /// Partner cooperative's DID
        partner_coop_did: Did,
        /// Maximum credit imbalance before requiring settlement
        max_imbalance: i64,
        /// How often to settle balances
        settlement_interval: SettlementInterval,
        /// Currency for the clearing arrangement
        currency: String,
    },

    /// Terminate an existing clearing agreement
    ///
    /// Requires settling all outstanding balances before termination.
    TerminateClearing {
        /// Partner cooperative's identifier
        partner_coop_id: String,
        /// Reason for termination
        reason: String,
    },

    /// Vouch for another cooperative's trustworthiness
    ///
    /// Creates a federated trust attestation that other cooperatives can use
    /// when evaluating the target cooperative.
    VouchForCooperative {
        /// Cooperative being vouched for
        target_coop_id: String,
        /// Target cooperative's DID
        target_coop_did: Did,
        /// Trust score (0.0 to 1.0) being attested
        trust_score: f64,
        /// Context for the vouch (e.g., "trade", "governance", "technical")
        context: String,
        /// Optional evidence/justification
        evidence: Option<String>,
    },

    /// Revoke a previous vouch for a cooperative
    ///
    /// Removes the trust attestation, affecting the target's federated trust score.
    RevokeVouch {
        /// Cooperative whose vouch is being revoked
        target_coop_id: String,
        /// Reason for revocation
        reason: String,
    },

    /// Update federation policy for the cooperative
    ///
    /// Modifies how the cooperative participates in federation activities.
    UpdateFederationPolicy {
        /// New auto-accept threshold for incoming vouches (-1.0 to disable)
        auto_accept_vouch_threshold: Option<f64>,
        /// New default trust decay factor for attestations (0.0-1.0)
        trust_decay_factor: Option<f64>,
        /// New maximum attestations per minute (rate limiting)
        max_attestations_per_minute: Option<u32>,
    },
}

impl FederationProposal {
    /// Get a descriptive name for the federation action
    pub fn action_name(&self) -> &'static str {
        match self {
            FederationProposal::JoinFederation { .. } => "join_federation",
            FederationProposal::LeaveFederation { .. } => "leave_federation",
            FederationProposal::EstablishClearing { .. } => "establish_clearing",
            FederationProposal::TerminateClearing { .. } => "terminate_clearing",
            FederationProposal::VouchForCooperative { .. } => "vouch_for_cooperative",
            FederationProposal::RevokeVouch { .. } => "revoke_vouch",
            FederationProposal::UpdateFederationPolicy { .. } => "update_federation_policy",
        }
    }

    /// Validate the federation proposal
    ///
    /// Returns an error message if validation fails.
    ///
    /// Note: This performs structural validation only. Full validation
    /// (e.g., verifying that a DID corresponds to a cooperative ID, or
    /// checking for existing memberships/agreements) requires federation
    /// registry lookup and should be done at proposal creation time by
    /// the gateway or at execution time by the handler.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            FederationProposal::JoinFederation {
                federation_id,
                terms,
                ..
            } => {
                if federation_id.is_empty() {
                    return Err("federation_id cannot be empty".to_string());
                }
                terms.validate()?;
            }
            FederationProposal::LeaveFederation {
                federation_id,
                reason,
                grace_period_days,
            } => {
                if federation_id.is_empty() {
                    return Err("federation_id cannot be empty".to_string());
                }
                if reason.is_empty() {
                    return Err("reason cannot be empty".to_string());
                }
                if *grace_period_days == 0 {
                    return Err("grace_period_days must be at least 1".to_string());
                }
                if *grace_period_days > 365 {
                    return Err("grace_period_days cannot exceed 365".to_string());
                }
            }
            FederationProposal::EstablishClearing {
                partner_coop_id,
                max_imbalance,
                currency,
                ..
            } => {
                if partner_coop_id.is_empty() {
                    return Err("partner_coop_id cannot be empty".to_string());
                }
                if *max_imbalance <= 0 {
                    return Err(format!(
                        "max_imbalance must be positive, got {max_imbalance}"
                    ));
                }
                if currency.is_empty() {
                    return Err("currency cannot be empty".to_string());
                }
            }
            FederationProposal::TerminateClearing {
                partner_coop_id,
                reason,
            } => {
                if partner_coop_id.is_empty() {
                    return Err("partner_coop_id cannot be empty".to_string());
                }
                if reason.is_empty() {
                    return Err("reason cannot be empty".to_string());
                }
            }
            FederationProposal::VouchForCooperative {
                target_coop_id,
                trust_score,
                context,
                ..
            } => {
                if target_coop_id.is_empty() {
                    return Err("target_coop_id cannot be empty".to_string());
                }
                if !(0.0..=1.0).contains(trust_score) {
                    return Err(format!(
                        "trust_score must be between 0.0 and 1.0, got {trust_score}"
                    ));
                }
                if context.is_empty() {
                    return Err("context cannot be empty".to_string());
                }
            }
            FederationProposal::RevokeVouch {
                target_coop_id,
                reason,
            } => {
                if target_coop_id.is_empty() {
                    return Err("target_coop_id cannot be empty".to_string());
                }
                if reason.is_empty() {
                    return Err("reason cannot be empty".to_string());
                }
            }
            FederationProposal::UpdateFederationPolicy {
                auto_accept_vouch_threshold,
                trust_decay_factor,
                max_attestations_per_minute,
            } => {
                if let Some(threshold) = auto_accept_vouch_threshold {
                    // -1.0 is valid (means disabled), otherwise must be in [0.0, 1.0]
                    if *threshold != -1.0 && !(0.0..=1.0).contains(threshold) {
                        return Err(format!(
                            "auto_accept_vouch_threshold must be -1.0 (disabled) or between 0.0 and 1.0, got {threshold}"
                        ));
                    }
                }
                // Validate trust_decay_factor if present
                if let Some(factor) = trust_decay_factor {
                    if !factor.is_finite() || !(0.0..=1.0).contains(factor) {
                        return Err(format!(
                            "trust_decay_factor must be between 0.0 and 1.0, got {factor}"
                        ));
                    }
                }
                if let Some(rate) = max_attestations_per_minute {
                    if *rate == 0 {
                        return Err(
                            "max_attestations_per_minute must be greater than 0".to_string()
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

/// Scope of a proposal — determines gossip routing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalScope {
    /// Local to this node's governance domain.
    Local,
    /// Visible across a federation.
    Federation(String),
}

impl Default for ProposalScope {
    fn default() -> Self {
        Self::Local
    }
}

/// A proposal for a decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    /// Unique identifier
    pub id: ProposalId,

    /// Domain this proposal belongs to
    pub domain_id: GovernanceDomainId,

    /// DID of the proposer
    pub proposer: Did,

    /// Title of the proposal
    pub title: String,

    /// Description
    pub description: String,

    /// Proposal payload (what is being decided)
    pub payload: ProposalPayload,

    /// Current state
    pub state: ProposalState,

    /// When created
    pub created_at: Timestamp,

    /// When last updated
    pub updated_at: Timestamp,

    /// Scope (local or federation). Defaults to Local for backward compat.
    #[serde(default)]
    pub scope: ProposalScope,
}

impl Proposal {
    /// Create a new draft proposal
    pub fn new(
        domain_id: GovernanceDomainId,
        proposer: Did,
        title: String,
        description: String,
        payload: ProposalPayload,
    ) -> Self {
        let now = icn_time::current_timestamp_secs();

        Self {
            id: ProposalId::generate(),
            domain_id,
            proposer,
            title,
            description,
            payload,
            state: ProposalState::Draft,
            created_at: now,
            updated_at: now,
            scope: ProposalScope::Local,
        }
    }

    /// Set the proposal scope.
    #[must_use]
    pub fn with_scope(mut self, scope: ProposalScope) -> Self {
        self.scope = scope;
        self
    }

    /// Start deliberation period for the proposal
    ///
    /// Transitions: Draft → Deliberation
    ///
    /// During deliberation, members can comment and discuss the proposal.
    /// Voting is not yet open. Call `end_deliberation_and_open()` to
    /// transition to voting when the deliberation period ends.
    pub fn start_deliberation(&mut self, deliberation_period_seconds: u64) -> anyhow::Result<()> {
        if !matches!(self.state, ProposalState::Draft) {
            anyhow::bail!("Can only start deliberation from Draft state");
        }

        let now = icn_time::try_current_timestamp_secs()
            .map_err(|e| anyhow::anyhow!("Cannot start deliberation: {e}"))?;

        self.state = ProposalState::Deliberation {
            started_at: now,
            ends_at: now + deliberation_period_seconds,
        };
        self.updated_at = now;

        Ok(())
    }

    /// End deliberation and open for voting
    ///
    /// Transitions: Deliberation → Open
    ///
    /// Can only be called after the deliberation period has ended.
    /// For early transition (before deliberation ends), use `force_end_deliberation()`.
    pub fn end_deliberation_and_open(&mut self, voting_period_seconds: u64) -> anyhow::Result<()> {
        let now = icn_time::try_current_timestamp_secs()
            .map_err(|e| anyhow::anyhow!("Cannot end deliberation: {e}"))?;

        match &self.state {
            ProposalState::Deliberation { ends_at, .. } => {
                if now < *ends_at {
                    anyhow::bail!(
                        "Deliberation period not yet ended ({} seconds remaining)",
                        ends_at.saturating_sub(now)
                    );
                }
            }
            _ => {
                anyhow::bail!("Can only end deliberation from Deliberation state");
            }
        }

        self.state = ProposalState::Open {
            opened_at: now,
            closes_at: now + voting_period_seconds,
        };
        self.updated_at = now;

        Ok(())
    }

    /// Force end deliberation early and open for voting
    ///
    /// This is an administrative action that bypasses the deliberation timer.
    /// Use with caution - deliberation exists to ensure adequate discussion.
    pub fn force_end_deliberation(&mut self, voting_period_seconds: u64) -> anyhow::Result<()> {
        if !matches!(self.state, ProposalState::Deliberation { .. }) {
            anyhow::bail!("Can only force end deliberation from Deliberation state");
        }

        let now = icn_time::try_current_timestamp_secs()
            .map_err(|e| anyhow::anyhow!("Cannot force end deliberation: {e}"))?;

        self.state = ProposalState::Open {
            opened_at: now,
            closes_at: now + voting_period_seconds,
        };
        self.updated_at = now;

        Ok(())
    }

    /// Open the proposal for voting directly (skip deliberation)
    ///
    /// Transitions: Draft → Open
    ///
    /// Use this when deliberation is not required (e.g., emergency proposals).
    /// For normal proposals, prefer `start_deliberation()` followed by
    /// `end_deliberation_and_open()`.
    pub fn open(&mut self, voting_period_seconds: u64) -> anyhow::Result<()> {
        if !matches!(self.state, ProposalState::Draft) {
            anyhow::bail!("Can only open proposals in Draft state");
        }

        let now = icn_time::try_current_timestamp_secs()
            .map_err(|e| anyhow::anyhow!("Cannot open proposal: {e}"))?;

        self.state = ProposalState::Open {
            opened_at: now,
            closes_at: now + voting_period_seconds,
        };
        self.updated_at = now;

        Ok(())
    }

    /// Close the proposal with a final state
    pub fn close(&mut self, final_state: ProposalState) -> anyhow::Result<()> {
        if !self.state.is_open() {
            anyhow::bail!("Can only close proposals in Open state");
        }

        if !final_state.is_closed() {
            anyhow::bail!("Must close with a terminal state");
        }

        self.state = final_state;
        self.updated_at = icn_time::try_current_timestamp_secs()
            .map_err(|e| anyhow::anyhow!("Cannot close proposal: {e}"))?;

        Ok(())
    }

    /// Cancel the proposal
    ///
    /// Can cancel from Draft, Deliberation, or Open states.
    /// Cannot cancel already closed proposals.
    pub fn cancel(&mut self) -> anyhow::Result<()> {
        if self.state.is_closed() {
            anyhow::bail!("Cannot cancel a closed proposal");
        }

        let now = icn_time::try_current_timestamp_secs()
            .map_err(|e| anyhow::anyhow!("Cannot cancel proposal: {e}"))?;

        self.state = ProposalState::Cancelled { cancelled_at: now };
        self.updated_at = now;

        Ok(())
    }

    /// Veto the proposal (emergency governance action)
    ///
    /// Can be applied to proposals in Draft, Deliberation, or Open state.
    /// Vetoed proposals cannot be reopened or executed.
    pub fn veto(&mut self, reason: String) -> anyhow::Result<()> {
        if self.state.is_closed() {
            anyhow::bail!("Cannot veto a closed proposal");
        }

        let now = icn_time::try_current_timestamp_secs()
            .map_err(|e| anyhow::anyhow!("Cannot veto proposal: {e}"))?;

        self.state = ProposalState::Vetoed {
            vetoed_at: now,
            reason,
        };
        self.updated_at = now;

        Ok(())
    }

    /// Force close the proposal with a specified outcome
    ///
    /// Can be applied to proposals in Deliberation or Open state.
    /// Used for emergency situations where normal process cannot proceed.
    pub fn force_close(
        &mut self,
        outcome: super::ProposalOutcome,
        reason: String,
    ) -> anyhow::Result<()> {
        if !self.state.is_open() && !self.state.is_deliberating() {
            anyhow::bail!("Can only force close proposals in Deliberation or Open state");
        }

        let now = icn_time::try_current_timestamp_secs()
            .map_err(|e| anyhow::anyhow!("Cannot force close proposal: {e}"))?;

        self.state = ProposalState::ForceClosed {
            closed_at: now,
            outcome,
            reason,
        };
        self.updated_at = now;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::GovernanceDomainId;
    use icn_identity::KeyPair;

    #[test]
    fn test_proposal_creation() {
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let domain_id = GovernanceDomainId::new("test-domain");

        let proposal = Proposal::new(
            domain_id,
            did,
            "Test Proposal".to_string(),
            "A test proposal".to_string(),
            ProposalPayload::Text {
                body: "Should we do this?".to_string(),
            },
        );

        assert_eq!(proposal.title, "Test Proposal");
        assert_eq!(proposal.state, ProposalState::Draft);
        assert!(proposal.created_at > 0);
    }

    #[test]
    fn test_proposal_lifecycle() {
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let domain_id = GovernanceDomainId::new("test-domain");

        let mut proposal = Proposal::new(
            domain_id,
            did,
            "Test".to_string(),
            "Test".to_string(),
            ProposalPayload::Text {
                body: "Test".to_string(),
            },
        );

        // Start in Draft
        assert_eq!(proposal.state, ProposalState::Draft);
        assert!(!proposal.state.is_open());
        assert!(!proposal.state.is_closed());

        // Open for voting
        proposal.open(3600).unwrap();
        assert!(proposal.state.is_open());
        assert!(!proposal.state.is_closed());
        assert!(proposal.state.closes_at().is_some());

        // Cannot open again
        assert!(proposal.open(3600).is_err());

        // Close as accepted
        let now = icn_time::current_timestamp_secs();
        proposal
            .close(ProposalState::Accepted { closed_at: now })
            .unwrap();
        assert!(!proposal.state.is_open());
        assert!(proposal.state.is_closed());

        // Cannot close again
        assert!(proposal
            .close(ProposalState::Rejected { closed_at: now })
            .is_err());
    }

    #[test]
    fn test_proposal_cancellation() {
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let domain_id = GovernanceDomainId::new("test-domain");

        let mut proposal = Proposal::new(
            domain_id,
            did,
            "Test".to_string(),
            "Test".to_string(),
            ProposalPayload::Text {
                body: "Test".to_string(),
            },
        );

        // Can cancel from Draft
        proposal.cancel().unwrap();
        assert!(matches!(proposal.state, ProposalState::Cancelled { .. }));

        // Cannot cancel after closed
        assert!(proposal.cancel().is_err());
    }

    #[test]
    fn test_proposal_veto() {
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let domain_id = GovernanceDomainId::new("test-domain");

        let mut proposal = Proposal::new(
            domain_id,
            did,
            "Veto Test".to_string(),
            "Test proposal for veto".to_string(),
            ProposalPayload::Text {
                body: "Test".to_string(),
            },
        );

        // Can veto from Draft
        proposal.veto("Security concern".to_string()).unwrap();
        assert!(matches!(
            proposal.state,
            ProposalState::Vetoed { ref reason, .. } if reason == "Security concern"
        ));
        assert!(proposal.state.is_closed());

        // Cannot veto after already vetoed
        assert!(proposal.veto("Another reason".to_string()).is_err());
    }

    #[test]
    fn test_proposal_veto_from_open() {
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let domain_id = GovernanceDomainId::new("test-domain");

        let mut proposal = Proposal::new(
            domain_id,
            did,
            "Veto Open Test".to_string(),
            "Test proposal for veto from open state".to_string(),
            ProposalPayload::Text {
                body: "Test".to_string(),
            },
        );

        // Open for voting
        proposal.open(3600).unwrap();
        assert!(proposal.state.is_open());

        // Can veto from Open
        proposal.veto("Emergency veto".to_string()).unwrap();
        assert!(matches!(proposal.state, ProposalState::Vetoed { .. }));
        assert!(proposal.state.is_closed());
    }

    #[test]
    fn test_proposal_force_close() {
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let domain_id = GovernanceDomainId::new("test-domain");

        let mut proposal = Proposal::new(
            domain_id,
            did,
            "Force Close Test".to_string(),
            "Test proposal for force close".to_string(),
            ProposalPayload::Text {
                body: "Test".to_string(),
            },
        );

        // Cannot force close from Draft (must be Deliberation or Open)
        assert!(proposal
            .force_close(crate::ProposalOutcome::Accepted, "Emergency".to_string())
            .is_err());

        // Open for voting
        proposal.open(3600).unwrap();
        assert!(proposal.state.is_open());

        // Can force close from Open
        proposal
            .force_close(
                crate::ProposalOutcome::Accepted,
                "Emergency acceptance".to_string(),
            )
            .unwrap();
        assert!(matches!(
            proposal.state,
            ProposalState::ForceClosed { ref outcome, ref reason, .. }
            if *outcome == crate::ProposalOutcome::Accepted && reason == "Emergency acceptance"
        ));
        assert!(proposal.state.is_closed());
    }

    // ========== Labor Share Proposal Tests ==========

    #[test]
    fn test_surplus_allocation_proposal() {
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let domain_id = GovernanceDomainId::new("test-coop");

        // Create a surplus allocation
        let allocation = icn_ledger::SurplusAllocation {
            id: "alloc-2025-q4".to_string(),
            cooperative_id: "test-coop".to_string(),
            total_surplus: 100_000,
            period: "2025-Q4".to_string(),
            share_unit_value: 100.0,
            total_labor_days: 1000,
            allocations: vec![
                (icn_ledger::ShareId::new("share-001"), 50_000),
                (icn_ledger::ShareId::new("share-002"), 50_000),
            ],
            proposal_id: String::new(), // Will be set after proposal creation
            allocated_at: 0,            // Will be set on execution
            currency: "COOP".to_string(),
        };

        let proposal = Proposal::new(
            domain_id,
            did,
            "Q4 2025 Surplus Allocation".to_string(),
            "Distribute Q4 surplus to labor shareholders".to_string(),
            ProposalPayload::SurplusAllocation { allocation },
        );

        assert_eq!(proposal.title, "Q4 2025 Surplus Allocation");
        assert!(matches!(
            proposal.payload,
            ProposalPayload::SurplusAllocation { ref allocation }
            if allocation.total_surplus == 100_000
        ));
    }

    #[test]
    fn test_share_redemption_proposal() {
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let domain_id = GovernanceDomainId::new("test-coop");

        let member_kp = KeyPair::generate().unwrap();
        let member_did = member_kp.did().clone();

        // Create a payout schedule (3 installments over 90 days)
        let payout_schedule = vec![
            icn_ledger::ScheduledPayout::new(
                icn_time::current_timestamp_secs() + 30 * 86400,
                10_000,
            ),
            icn_ledger::ScheduledPayout::new(
                icn_time::current_timestamp_secs() + 60 * 86400,
                10_000,
            ),
            icn_ledger::ScheduledPayout::new(
                icn_time::current_timestamp_secs() + 90 * 86400,
                10_000,
            ),
        ];

        let proposal = Proposal::new(
            domain_id,
            did,
            "Share Redemption - Member Departure".to_string(),
            "Approve redemption of shares for departing member".to_string(),
            ProposalPayload::ShareRedemption {
                member: member_did.clone(),
                share_ids: vec![
                    icn_ledger::ShareId::new("share-001"),
                    icn_ledger::ShareId::new("share-002"),
                ],
                payout_schedule,
                reason: "Voluntary departure".to_string(),
            },
        );

        assert!(matches!(
            proposal.payload,
            ProposalPayload::ShareRedemption { ref member, ref share_ids, ref payout_schedule, .. }
            if *member == member_did && share_ids.len() == 2 && payout_schedule.len() == 3
        ));
    }

    #[test]
    fn test_bond_issuance_proposal() {
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let domain_id = GovernanceDomainId::new("test-coop");

        let bond_offering = icn_ledger::BondOffering {
            issuer_id: "test-coop".to_string(),
            principal_requested: 500_000,
            interest_rate_bps: 300, // 3%
            term_days: 365,
            purpose: "Equipment purchase for new production line".to_string(),
            payment_schedule: icn_ledger::PaymentSchedule::InterestOnly { interval_days: 90 },
            currency: "COOP".to_string(),
            collateral: vec![icn_ledger::Collateral::Equipment {
                description: "CNC Machine Model X".to_string(),
                value: 200_000,
            }],
        };

        let proposal = Proposal::new(
            domain_id,
            did,
            "Bond Issuance - Equipment Financing".to_string(),
            "Issue bonds to finance equipment purchase".to_string(),
            ProposalPayload::BondIssuance { bond_offering },
        );

        assert!(matches!(
            proposal.payload,
            ProposalPayload::BondIssuance { ref bond_offering }
            if bond_offering.principal_requested == 500_000 && bond_offering.interest_rate_bps == 300
        ));
    }

    // ========== Federation Proposal Tests (Issue #514) ==========

    #[test]
    fn test_federation_join_proposal() {
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let domain_id = GovernanceDomainId::new("test-coop");

        let terms = FederationTerms {
            min_trust_threshold: 0.6,
            governance_binding: true,
            data_sharing_level: DataSharingLevel::MetadataOnly,
            dispute_resolution: DisputeResolutionMethod::FederationMediation,
        };

        let proposal = Proposal::new(
            domain_id,
            did,
            "Join Regional Food Federation".to_string(),
            "Proposal to join the regional food cooperative federation".to_string(),
            ProposalPayload::Federation(FederationProposal::JoinFederation {
                federation_id: "regional-food-fed".to_string(),
                terms: terms.clone(),
                sponsor_coop_id: Some("organic-farms-coop".to_string()),
            }),
        );

        assert_eq!(proposal.title, "Join Regional Food Federation");
        assert!(matches!(
            proposal.payload,
            ProposalPayload::Federation(FederationProposal::JoinFederation { ref federation_id, .. })
            if federation_id == "regional-food-fed"
        ));
    }

    #[test]
    fn test_federation_establish_clearing_proposal() {
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let domain_id = GovernanceDomainId::new("test-coop");

        let partner_kp = KeyPair::generate().unwrap();
        let partner_did = partner_kp.did().clone();

        let proposal = Proposal::new(
            domain_id,
            did,
            "Establish Clearing with Farm Coop".to_string(),
            "Set up bilateral clearing for cross-coop transactions".to_string(),
            ProposalPayload::Federation(FederationProposal::EstablishClearing {
                partner_coop_id: "farm-coop".to_string(),
                partner_coop_did: partner_did.clone(),
                max_imbalance: 50_000,
                settlement_interval: SettlementInterval::Weekly,
                currency: "HOURS".to_string(),
            }),
        );

        assert!(matches!(
            proposal.payload,
            ProposalPayload::Federation(FederationProposal::EstablishClearing {
                ref partner_coop_id,
                max_imbalance,
                ..
            })
            if partner_coop_id == "farm-coop" && max_imbalance == 50_000
        ));
    }

    #[test]
    fn test_federation_vouch_proposal() {
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let domain_id = GovernanceDomainId::new("test-coop");

        let target_kp = KeyPair::generate().unwrap();
        let target_did = target_kp.did().clone();

        let proposal = Proposal::new(
            domain_id,
            did,
            "Vouch for New Coop".to_string(),
            "Vouch for the newly formed worker cooperative".to_string(),
            ProposalPayload::Federation(FederationProposal::VouchForCooperative {
                target_coop_id: "new-worker-coop".to_string(),
                target_coop_did: target_did.clone(),
                trust_score: 0.75,
                context: "trade".to_string(),
                evidence: Some(
                    "We have been trading with them successfully for 6 months".to_string(),
                ),
            }),
        );

        assert!(matches!(
            proposal.payload,
            ProposalPayload::Federation(FederationProposal::VouchForCooperative {
                trust_score,
                ref context,
                ..
            })
            if (trust_score - 0.75).abs() < 0.001 && context == "trade"
        ));
    }

    #[test]
    fn test_federation_proposal_type_name() {
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let domain_id = GovernanceDomainId::new("test-coop");

        let proposal = Proposal::new(
            domain_id,
            did,
            "Federation Test".to_string(),
            "Test proposal".to_string(),
            ProposalPayload::Federation(FederationProposal::LeaveFederation {
                federation_id: "test-fed".to_string(),
                reason: "Strategic realignment".to_string(),
                grace_period_days: 30,
            }),
        );

        assert_eq!(proposal.payload.type_name(), "federation");
    }

    #[test]
    fn test_federation_proposal_action_names() {
        assert_eq!(
            FederationProposal::JoinFederation {
                federation_id: "test".to_string(),
                terms: FederationTerms::default(),
                sponsor_coop_id: None,
            }
            .action_name(),
            "join_federation"
        );

        assert_eq!(
            FederationProposal::LeaveFederation {
                federation_id: "test".to_string(),
                reason: "test".to_string(),
                grace_period_days: 30,
            }
            .action_name(),
            "leave_federation"
        );

        let target_did = KeyPair::generate().unwrap().did().clone();
        assert_eq!(
            FederationProposal::EstablishClearing {
                partner_coop_id: "test".to_string(),
                partner_coop_did: target_did.clone(),
                max_imbalance: 1000,
                settlement_interval: SettlementInterval::Daily,
                currency: "HOURS".to_string(),
            }
            .action_name(),
            "establish_clearing"
        );

        assert_eq!(
            FederationProposal::VouchForCooperative {
                target_coop_id: "test".to_string(),
                target_coop_did: target_did,
                trust_score: 0.8,
                context: "trade".to_string(),
                evidence: None,
            }
            .action_name(),
            "vouch_for_cooperative"
        );

        assert_eq!(
            FederationProposal::TerminateClearing {
                partner_coop_id: "test".to_string(),
                reason: "ending partnership".to_string(),
            }
            .action_name(),
            "terminate_clearing"
        );

        assert_eq!(
            FederationProposal::RevokeVouch {
                target_coop_id: "test".to_string(),
                reason: "trust violation".to_string(),
            }
            .action_name(),
            "revoke_vouch"
        );

        assert_eq!(
            FederationProposal::UpdateFederationPolicy {
                auto_accept_vouch_threshold: Some(0.5),
                trust_decay_factor: None,
                max_attestations_per_minute: None,
            }
            .action_name(),
            "update_federation_policy"
        );
    }

    #[test]
    fn test_federation_terms_default() {
        let terms = FederationTerms::default();
        assert!((terms.min_trust_threshold - 0.5).abs() < 0.001);
        assert!(terms.governance_binding);
        assert_eq!(terms.data_sharing_level, DataSharingLevel::MetadataOnly);
        assert_eq!(
            terms.dispute_resolution,
            DisputeResolutionMethod::FederationMediation
        );
    }

    #[test]
    fn test_federation_terms_validation() {
        // Valid terms
        let valid_terms = FederationTerms::default();
        assert!(valid_terms.validate().is_ok());

        // FederationTerms.validate() now validates range on f64 directly.
        // Test that validate() rejects invalid values:
        let invalid_high = FederationTerms {
            min_trust_threshold: 1.5, // Too high
            ..Default::default()
        };
        assert!(invalid_high.validate().is_err());

        let invalid_negative = FederationTerms {
            min_trust_threshold: -0.1, // Negative
            ..Default::default()
        };
        assert!(invalid_negative.validate().is_err());

        // Edge cases that should be valid
        let edge_zero = FederationTerms {
            min_trust_threshold: 0.0,
            ..Default::default()
        };
        assert!(edge_zero.validate().is_ok());

        let edge_one = FederationTerms {
            min_trust_threshold: 1.0,
            ..Default::default()
        };
        assert!(edge_one.validate().is_ok());

        // ArbitratorCooperative with empty arbitrator_id
        let empty_arbitrator = FederationTerms {
            dispute_resolution: DisputeResolutionMethod::ArbitratorCooperative {
                arbitrator_id: "".to_string(),
            },
            ..Default::default()
        };
        assert!(empty_arbitrator.validate().is_err());
        assert!(empty_arbitrator
            .validate()
            .unwrap_err()
            .contains("arbitrator_id"));

        // ArbitratorCooperative with valid arbitrator_id
        let valid_arbitrator = FederationTerms {
            dispute_resolution: DisputeResolutionMethod::ArbitratorCooperative {
                arbitrator_id: "arbitrator-coop".to_string(),
            },
            ..Default::default()
        };
        assert!(valid_arbitrator.validate().is_ok());
    }

    #[test]
    fn test_federation_proposal_validation() {
        let target_did = KeyPair::generate().unwrap().did().clone();

        // Valid vouch
        let valid_vouch = FederationProposal::VouchForCooperative {
            target_coop_id: "test-coop".to_string(),
            target_coop_did: target_did.clone(),
            trust_score: 0.8,
            context: "trade".to_string(),
            evidence: None,
        };
        assert!(valid_vouch.validate().is_ok());

        // Invalid trust score (too high)
        let invalid_score = FederationProposal::VouchForCooperative {
            target_coop_id: "test-coop".to_string(),
            target_coop_did: target_did.clone(),
            trust_score: 1.5,
            context: "trade".to_string(),
            evidence: None,
        };
        assert!(invalid_score.validate().is_err());
        assert!(invalid_score
            .validate()
            .unwrap_err()
            .contains("trust_score"));

        // Empty target_coop_id
        let empty_id = FederationProposal::VouchForCooperative {
            target_coop_id: "".to_string(),
            target_coop_did: target_did.clone(),
            trust_score: 0.5,
            context: "trade".to_string(),
            evidence: None,
        };
        assert!(empty_id.validate().is_err());
        assert!(empty_id.validate().unwrap_err().contains("target_coop_id"));

        // Empty context
        let empty_context = FederationProposal::VouchForCooperative {
            target_coop_id: "test-coop".to_string(),
            target_coop_did: target_did.clone(),
            trust_score: 0.5,
            context: "".to_string(),
            evidence: None,
        };
        assert!(empty_context.validate().is_err());

        // Invalid max_imbalance (zero)
        let zero_imbalance = FederationProposal::EstablishClearing {
            partner_coop_id: "test-coop".to_string(),
            partner_coop_did: target_did.clone(),
            max_imbalance: 0,
            settlement_interval: SettlementInterval::Weekly,
            currency: "HOURS".to_string(),
        };
        assert!(zero_imbalance.validate().is_err());
        assert!(zero_imbalance
            .validate()
            .unwrap_err()
            .contains("max_imbalance"));

        let valid_policy = FederationProposal::UpdateFederationPolicy {
            auto_accept_vouch_threshold: Some(-1.0),
            trust_decay_factor: Some(0.5),
            max_attestations_per_minute: Some(10),
        };
        assert!(valid_policy.validate().is_ok());

        // Invalid policy update
        let invalid_policy = FederationProposal::UpdateFederationPolicy {
            auto_accept_vouch_threshold: Some(-0.5), // not -1.0 and not in [0,1]
            trust_decay_factor: None,
            max_attestations_per_minute: None,
        };
        assert!(invalid_policy.validate().is_err());

        // Invalid grace_period_days (zero)
        let zero_grace = FederationProposal::LeaveFederation {
            federation_id: "test-fed".to_string(),
            reason: "Leaving".to_string(),
            grace_period_days: 0,
        };
        assert!(zero_grace.validate().is_err());
        assert!(zero_grace
            .validate()
            .unwrap_err()
            .contains("grace_period_days"));

        // Invalid grace_period_days (too long)
        let long_grace = FederationProposal::LeaveFederation {
            federation_id: "test-fed".to_string(),
            reason: "Leaving".to_string(),
            grace_period_days: 500,
        };
        assert!(long_grace.validate().is_err());

        // Valid grace_period_days (edge cases)
        let min_grace = FederationProposal::LeaveFederation {
            federation_id: "test-fed".to_string(),
            reason: "Leaving".to_string(),
            grace_period_days: 1,
        };
        assert!(min_grace.validate().is_ok());

        let max_grace = FederationProposal::LeaveFederation {
            federation_id: "test-fed".to_string(),
            reason: "Leaving".to_string(),
            grace_period_days: 365,
        };
        assert!(max_grace.validate().is_ok());

        // JoinFederation validation - empty federation_id
        let empty_fed_id = FederationProposal::JoinFederation {
            federation_id: "".to_string(),
            terms: FederationTerms::default(),
            sponsor_coop_id: None,
        };
        assert!(empty_fed_id.validate().is_err());
        assert!(empty_fed_id
            .validate()
            .unwrap_err()
            .contains("federation_id"));

        // FederationTerms.validate() now validates min_trust_threshold as f64.
        // Invalid values will be rejected by validate()
        let invalid_threshold = FederationTerms {
            min_trust_threshold: 1.5, // Too high
            ..Default::default()
        };
        assert!(invalid_threshold.validate().is_err());

        // Valid terms should pass validation
        let valid_terms_join = FederationProposal::JoinFederation {
            federation_id: "test-fed".to_string(),
            terms: FederationTerms::default(),
            sponsor_coop_id: None,
        };
        assert!(valid_terms_join.validate().is_ok());

        // TerminateClearing validation - empty partner_coop_id
        let empty_partner_terminate = FederationProposal::TerminateClearing {
            partner_coop_id: "".to_string(),
            reason: "Ending partnership".to_string(),
        };
        assert!(empty_partner_terminate.validate().is_err());
        assert!(empty_partner_terminate
            .validate()
            .unwrap_err()
            .contains("partner_coop_id"));

        // TerminateClearing validation - empty reason
        let empty_reason_terminate = FederationProposal::TerminateClearing {
            partner_coop_id: "partner-coop".to_string(),
            reason: "".to_string(),
        };
        assert!(empty_reason_terminate.validate().is_err());
        assert!(empty_reason_terminate
            .validate()
            .unwrap_err()
            .contains("reason"));

        // RevokeVouch validation - empty target_coop_id
        let empty_target_revoke = FederationProposal::RevokeVouch {
            target_coop_id: "".to_string(),
            reason: "Trust violation".to_string(),
        };
        assert!(empty_target_revoke.validate().is_err());
        assert!(empty_target_revoke
            .validate()
            .unwrap_err()
            .contains("target_coop_id"));

        // RevokeVouch validation - empty reason
        let empty_reason_revoke = FederationProposal::RevokeVouch {
            target_coop_id: "target-coop".to_string(),
            reason: "".to_string(),
        };
        assert!(empty_reason_revoke.validate().is_err());
        assert!(empty_reason_revoke
            .validate()
            .unwrap_err()
            .contains("reason"));

        // UpdateFederationPolicy - max_attestations_per_minute = 0
        let zero_attestations = FederationProposal::UpdateFederationPolicy {
            auto_accept_vouch_threshold: None,
            trust_decay_factor: None,
            max_attestations_per_minute: Some(0),
        };
        assert!(zero_attestations.validate().is_err());
        assert!(zero_attestations
            .validate()
            .unwrap_err()
            .contains("max_attestations_per_minute"));
    }

    #[test]
    fn test_federation_proposal_serialization_roundtrip() {
        let target_did = KeyPair::generate().unwrap().did().clone();

        let proposals = vec![
            FederationProposal::JoinFederation {
                federation_id: "test-fed".to_string(),
                terms: FederationTerms::default(),
                sponsor_coop_id: Some("sponsor".to_string()),
            },
            FederationProposal::LeaveFederation {
                federation_id: "test-fed".to_string(),
                reason: "Strategic change".to_string(),
                grace_period_days: 30,
            },
            FederationProposal::EstablishClearing {
                partner_coop_id: "partner".to_string(),
                partner_coop_did: target_did.clone(),
                max_imbalance: 50000,
                settlement_interval: SettlementInterval::Weekly,
                currency: "HOURS".to_string(),
            },
            FederationProposal::VouchForCooperative {
                target_coop_id: "target".to_string(),
                target_coop_did: target_did,
                trust_score: 0.75,
                context: "trade".to_string(),
                evidence: Some("Good history".to_string()),
            },
            FederationProposal::TerminateClearing {
                partner_coop_id: "partner".to_string(),
                reason: "Partnership ended".to_string(),
            },
            FederationProposal::RevokeVouch {
                target_coop_id: "target".to_string(),
                reason: "Trust violation".to_string(),
            },
            FederationProposal::UpdateFederationPolicy {
                auto_accept_vouch_threshold: Some(0.7),
                trust_decay_factor: Some(0.05),
                max_attestations_per_minute: Some(30),
            },
        ];

        for proposal in proposals {
            let json = serde_json::to_string(&proposal).unwrap();
            let deserialized: FederationProposal = serde_json::from_str(&json).unwrap();
            assert_eq!(proposal.action_name(), deserialized.action_name());
        }
    }

    #[test]
    fn test_federation_terms_serialization_roundtrip() {
        let terms = FederationTerms {
            min_trust_threshold: 0.7,
            governance_binding: true,
            data_sharing_level: DataSharingLevel::Full,
            dispute_resolution: DisputeResolutionMethod::FederationVote,
        };

        let json = serde_json::to_string(&terms).unwrap();
        let deserialized: FederationTerms = serde_json::from_str(&json).unwrap();

        assert!((terms.min_trust_threshold - deserialized.min_trust_threshold).abs() < 0.001);
        assert_eq!(terms.governance_binding, deserialized.governance_binding);
        assert_eq!(terms.data_sharing_level, deserialized.data_sharing_level);
        assert_eq!(terms.dispute_resolution, deserialized.dispute_resolution);
    }

    #[test]
    fn test_data_sharing_level_serialization() {
        for level in [
            DataSharingLevel::None,
            DataSharingLevel::MetadataOnly,
            DataSharingLevel::Full,
        ] {
            let json = serde_json::to_string(&level).unwrap();
            let deserialized: DataSharingLevel = serde_json::from_str(&json).unwrap();
            assert_eq!(level, deserialized);
        }
    }

    #[test]
    fn test_dispute_resolution_method_serialization() {
        let methods = vec![
            DisputeResolutionMethod::FederationMediation,
            DisputeResolutionMethod::ArbitratorCooperative {
                arbitrator_id: "arbitrator-coop".to_string(),
            },
            DisputeResolutionMethod::FederationVote,
        ];

        for method in methods {
            let json = serde_json::to_string(&method).unwrap();
            let deserialized: DisputeResolutionMethod = serde_json::from_str(&json).unwrap();
            assert_eq!(method, deserialized);
        }
    }

    // ========== TreasurySpend Tests (Issue #1088) ==========

    #[test]
    fn test_treasury_spend_serialization_roundtrip() {
        let treasury_did = KeyPair::generate().unwrap().did().clone();
        let recipient = KeyPair::generate().unwrap().did().clone();

        let operation = TreasuryProposalOperation::Spend {
            treasury_did: treasury_did.clone(),
            amount: 5000,
            currency: "credits".to_string(),
            recipient: recipient.clone(),
            memo: "Office supplies reimbursement".to_string(),
            nonce: 0,
        };

        let json = serde_json::to_string(&operation).unwrap();
        let deserialized: TreasuryProposalOperation = serde_json::from_str(&json).unwrap();

        match deserialized {
            TreasuryProposalOperation::Spend {
                treasury_did: parsed_treasury_did,
                amount,
                currency,
                recipient: r,
                memo,
                nonce,
            } => {
                assert_eq!(parsed_treasury_did, treasury_did);
                assert_eq!(amount, 5000);
                assert_eq!(currency, "credits");
                assert_eq!(r, recipient);
                assert_eq!(memo, "Office supplies reimbursement");
                assert_eq!(nonce, 0);
            }
            other => panic!("Expected Spend, got {:?}", other),
        }
    }

    #[test]
    fn test_treasury_spend_proposal_creation() {
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let domain_id = GovernanceDomainId::new("test-coop");
        let treasury_did = KeyPair::generate().unwrap().did().clone();
        let recipient = KeyPair::generate().unwrap().did().clone();

        let proposal = Proposal::new(
            domain_id,
            did,
            "Community garden supplies".to_string(),
            "Purchase supplies for the community garden project".to_string(),
            ProposalPayload::Treasury {
                operation: TreasuryProposalOperation::Spend {
                    treasury_did,
                    amount: 2500,
                    currency: "credits".to_string(),
                    recipient: recipient.clone(),
                    memo: "Garden tools and seeds".to_string(),
                    nonce: 0,
                },
            },
        );

        assert_eq!(proposal.title, "Community garden supplies");
        assert_eq!(proposal.payload.type_name(), "treasury");
        assert!(matches!(
            proposal.payload,
            ProposalPayload::Treasury {
                operation: TreasuryProposalOperation::Spend { amount: 2500, .. }
            }
        ));
    }

    #[test]
    fn test_treasury_spend_zero_amount_is_representable() {
        // Zero amount is structurally valid at the type level; validation
        // happens at the handler level (handler rejects amount <= 0).
        let treasury_did = KeyPair::generate().unwrap().did().clone();
        let recipient = KeyPair::generate().unwrap().did().clone();

        let operation = TreasuryProposalOperation::Spend {
            treasury_did,
            amount: 0,
            currency: "credits".to_string(),
            recipient,
            memo: "Should be rejected by handler".to_string(),
            nonce: 0,
        };

        // Serialization still works -- the handler is responsible for rejection.
        let json = serde_json::to_string(&operation).unwrap();
        assert!(json.contains("\"amount\":0"));
    }

    #[test]
    fn test_treasury_spend_empty_memo_is_representable() {
        // Empty memo is structurally valid; handler-level validation rejects it.
        let treasury_did = KeyPair::generate().unwrap().did().clone();
        let recipient = KeyPair::generate().unwrap().did().clone();

        let operation = TreasuryProposalOperation::Spend {
            treasury_did,
            amount: 100,
            currency: "credits".to_string(),
            recipient,
            memo: String::new(),
            nonce: 0,
        };

        let json = serde_json::to_string(&operation).unwrap();
        assert!(json.contains("\"memo\":\"\""));
    }

    #[test]
    fn test_treasury_spend_payload_in_full_lifecycle() {
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let domain_id = GovernanceDomainId::new("test-coop");
        let treasury_did = KeyPair::generate().unwrap().did().clone();
        let recipient = KeyPair::generate().unwrap().did().clone();

        let mut proposal = Proposal::new(
            domain_id,
            did,
            "Emergency repair fund".to_string(),
            "Approve spend for emergency roof repair".to_string(),
            ProposalPayload::Treasury {
                operation: TreasuryProposalOperation::Spend {
                    treasury_did,
                    amount: 15000,
                    currency: "credits".to_string(),
                    recipient,
                    memo: "Roof repair after storm damage".to_string(),
                    nonce: 0,
                },
            },
        );

        assert!(proposal.state.is_draft());

        // Open directly (emergency-style)
        proposal.open(3600).unwrap();
        assert!(proposal.state.is_open());

        // Close as accepted
        let now = icn_time::current_timestamp_secs();
        proposal
            .close(ProposalState::Accepted { closed_at: now })
            .unwrap();
        assert!(proposal.state.is_closed());
    }

    #[test]
    fn test_proposal_scope_serde_roundtrip_local() {
        let scope = ProposalScope::Local;
        let json = serde_json::to_string(&scope).unwrap();
        assert_eq!(json, "\"local\"");
        let back: ProposalScope = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ProposalScope::Local);
    }

    #[test]
    fn test_proposal_scope_serde_roundtrip_federation() {
        let scope = ProposalScope::Federation("food-coop-fed".to_string());
        let json = serde_json::to_string(&scope).unwrap();
        // Assert exact wire format for determinism
        assert_eq!(json, r#"{"federation":"food-coop-fed"}"#);
        let back: ProposalScope = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ProposalScope::Federation("food-coop-fed".to_string()));
    }

    #[test]
    fn test_proposal_scope_default_is_local() {
        assert_eq!(ProposalScope::default(), ProposalScope::Local);
    }

    #[test]
    fn test_proposal_backward_compat_no_scope_field() {
        // Simulate a Proposal JSON from before the scope field existed.
        // When deserialized, scope should default to Local.
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let domain_id = GovernanceDomainId::new("test-domain");

        let proposal = Proposal::new(
            domain_id,
            did,
            "Compat Test".to_string(),
            "Testing backward compat".to_string(),
            ProposalPayload::Text {
                body: "Test".to_string(),
            },
        );

        // Serialize, then strip the scope field, then deserialize
        let mut val: serde_json::Value = serde_json::to_value(&proposal).unwrap();
        val.as_object_mut().unwrap().remove("scope");

        let restored: Proposal = serde_json::from_value(val).unwrap();
        assert_eq!(restored.scope, ProposalScope::Local);
    }

    #[test]
    fn test_proposal_with_scope_builder() {
        let kp = KeyPair::generate().unwrap();
        let did = kp.did().clone();
        let domain_id = GovernanceDomainId::new("test-domain");

        let proposal = Proposal::new(
            domain_id,
            did,
            "Federation Proposal".to_string(),
            "A proposal scoped to a federation".to_string(),
            ProposalPayload::Text {
                body: "Test".to_string(),
            },
        )
        .with_scope(ProposalScope::Federation("my-fed".to_string()));

        assert_eq!(
            proposal.scope,
            ProposalScope::Federation("my-fed".to_string())
        );
    }

    // --- AllocationProposal tests ---

    fn test_did() -> Did {
        let kp = KeyPair::generate().unwrap();
        kp.did().clone()
    }

    #[test]
    fn test_allocation_proposal_creation() {
        let proposer = test_did();
        let domain_id = GovernanceDomainId::new("test-domain");

        let proposal = Proposal::new(
            domain_id,
            proposer,
            "Q2 Compute Budget".to_string(),
            "Distribute 10,000 compute-hours across projects".to_string(),
            ProposalPayload::Allocation {
                pool_amount: 10_000,
                unit: "compute-hours".to_string(),
                options: vec![
                    AllocationOption {
                        label: "Infrastructure".to_string(),
                        description: "Cluster maintenance and upgrades".to_string(),
                        recipient: test_did(),
                        requested_amount: 6_000,
                    },
                    AllocationOption {
                        label: "Education".to_string(),
                        description: "Training programs".to_string(),
                        recipient: test_did(),
                        requested_amount: 4_000,
                    },
                ],
                purpose: "Q2 participatory budgeting".to_string(),
            },
        );

        assert_eq!(proposal.payload.type_name(), "allocation");
        assert!(!proposal.payload.is_emergency());
    }

    #[test]
    fn test_allocation_option_serde_roundtrip() {
        let opt = AllocationOption {
            label: "Research".to_string(),
            description: "Fund cooperative research".to_string(),
            recipient: test_did(),
            requested_amount: 5_000,
        };

        let json = serde_json::to_string(&opt).unwrap();
        let deserialized: AllocationOption = serde_json::from_str(&json).unwrap();
        assert_eq!(opt, deserialized);
    }

    #[test]
    fn test_allocation_payload_serde_roundtrip() {
        let payload = ProposalPayload::Allocation {
            pool_amount: 10_000,
            unit: "participation-units".to_string(),
            options: vec![
                AllocationOption {
                    label: "Option A".to_string(),
                    description: "First option".to_string(),
                    recipient: test_did(),
                    requested_amount: 7_000,
                },
                AllocationOption {
                    label: "Option B".to_string(),
                    description: "Second option".to_string(),
                    recipient: test_did(),
                    requested_amount: 5_000,
                },
            ],
            purpose: "Test allocation".to_string(),
        };

        let json = serde_json::to_string(&payload).unwrap();
        let deserialized: ProposalPayload = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.type_name(), "allocation");
        if let ProposalPayload::Allocation {
            pool_amount,
            unit,
            options,
            purpose,
        } = &deserialized
        {
            assert_eq!(*pool_amount, 10_000);
            assert_eq!(unit, "participation-units");
            assert_eq!(options.len(), 2);
            assert_eq!(purpose, "Test allocation");
        } else {
            panic!("Expected Allocation variant after deserialization");
        }
    }
}
