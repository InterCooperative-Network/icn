//! Proposal types and state machine

use crate::domain::GovernanceDomainId;
use crate::Timestamp;
use icn_federation::SettlementInterval;
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
}

impl ProposalPayload {
    /// Get a string name for the proposal type (for metrics labeling)
    pub fn type_name(&self) -> &'static str {
        match self {
            ProposalPayload::Text { .. } => "text",
            ProposalPayload::Budget { .. } => "budget",
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

/// Semantic version for protocol versioning
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    /// Create a new version
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Parse version from string (e.g., "1.2.3")
    pub fn parse(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return Err(format!("Invalid version format: {s}"));
        }

        let major = parts[0]
            .parse::<u32>()
            .map_err(|_| format!("Invalid major version: {}", parts[0]))?;
        let minor = parts[1]
            .parse::<u32>()
            .map_err(|_| format!("Invalid minor version: {}", parts[1]))?;
        let patch = parts[2]
            .parse::<u32>()
            .map_err(|_| format!("Invalid patch version: {}", parts[2]))?;

        Ok(Self::new(major, minor, patch))
    }

    /// Check if this version is compatible with another
    ///
    /// Compatible means:
    /// - Same major version (no breaking changes)
    /// - Minor/patch can differ
    pub fn is_compatible_with(&self, other: &Version) -> bool {
        self.major == other.major
    }

    /// Check if this version has breaking changes vs another
    pub fn has_breaking_changes_vs(&self, other: &Version) -> bool {
        self.major != other.major
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

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
    /// Required trust threshold for federation membership
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
        if !(0.0..=1.0).contains(&self.min_trust_threshold) {
            let threshold = self.min_trust_threshold;
            return Err(format!(
                "min_trust_threshold must be between 0.0 and 1.0, got {threshold}"
            ));
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
        /// New default trust decay factor for attestations
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
                ..
            } => {
                if federation_id.is_empty() {
                    return Err("federation_id cannot be empty".to_string());
                }
                if reason.is_empty() {
                    return Err("reason cannot be empty".to_string());
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
                ..
            } => {
                if let Some(threshold) = auto_accept_vouch_threshold {
                    // -1.0 is valid (means disabled), otherwise must be in [0.0, 1.0]
                    if *threshold != -1.0 && !(0.0..=1.0).contains(threshold) {
                        return Err(format!(
                            "auto_accept_vouch_threshold must be -1.0 (disabled) or between 0.0 and 1.0, got {threshold}"
                        ));
                    }
                }
                if let Some(decay) = trust_decay_factor {
                    if !(0.0..=1.0).contains(decay) {
                        return Err(format!(
                            "trust_decay_factor must be between 0.0 and 1.0, got {decay}"
                        ));
                    }
                }
            }
        }
        Ok(())
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
        }
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

        // Invalid trust threshold (too high)
        let invalid_high = FederationTerms {
            min_trust_threshold: 1.5,
            ..Default::default()
        };
        assert!(invalid_high.validate().is_err());
        assert!(invalid_high
            .validate()
            .unwrap_err()
            .contains("min_trust_threshold"));

        // Invalid trust threshold (negative)
        let invalid_negative = FederationTerms {
            min_trust_threshold: -0.1,
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

        // Valid policy update with -1.0 (disabled)
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
}
