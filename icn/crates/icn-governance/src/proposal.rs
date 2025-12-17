//! Proposal types and state machine

use crate::domain::GovernanceDomainId;
use crate::Timestamp;
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
    /// Draft proposal, not yet open for voting
    Draft,

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
    /// Check if proposal is open for voting
    pub fn is_open(&self) -> bool {
        matches!(self, ProposalState::Open { .. })
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

    /// Get the timestamp when voting closes (if open)
    pub fn closes_at(&self) -> Option<Timestamp> {
        match self {
            ProposalState::Open { closes_at, .. } => Some(*closes_at),
            _ => None,
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
            return Err(format!("Invalid version format: {}", s));
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
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

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

    /// Open the proposal for voting
    pub fn open(&mut self, voting_period_seconds: u64) -> anyhow::Result<()> {
        if !matches!(self.state, ProposalState::Draft) {
            anyhow::bail!("Can only open proposals in Draft state");
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

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
        self.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Ok(())
    }

    /// Cancel the proposal
    pub fn cancel(&mut self) -> anyhow::Result<()> {
        if self.state.is_closed() {
            anyhow::bail!("Cannot cancel a closed proposal");
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        self.state = ProposalState::Cancelled { cancelled_at: now };
        self.updated_at = now;

        Ok(())
    }

    /// Veto the proposal (emergency governance action)
    ///
    /// Can be applied to proposals in Draft or Open state.
    /// Vetoed proposals cannot be reopened or executed.
    pub fn veto(&mut self, reason: String) -> anyhow::Result<()> {
        if self.state.is_closed() {
            anyhow::bail!("Cannot veto a closed proposal");
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        self.state = ProposalState::Vetoed {
            vetoed_at: now,
            reason,
        };
        self.updated_at = now;

        Ok(())
    }

    /// Force close the proposal with a specified outcome
    ///
    /// Can be applied to proposals in Open state only.
    /// Used for emergency situations where normal voting cannot proceed.
    pub fn force_close(
        &mut self,
        outcome: super::ProposalOutcome,
        reason: String,
    ) -> anyhow::Result<()> {
        if !self.state.is_open() {
            anyhow::bail!("Can only force close proposals in Open state");
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

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
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
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

        // Cannot force close from Draft (must be Open)
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
}
