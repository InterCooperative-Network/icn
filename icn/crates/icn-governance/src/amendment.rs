//! Amendment - Charter and Constitutional changes
//!
//! This module implements the amendment system for modifying charters and
//! constitutional rules within the ICN commons. Amendments follow a structured
//! lifecycle with multi-level ratification requirements.
//!
//! # Amendment Types
//!
//! - **Charter Amendment**: Changes to a jurisdiction's charter (coop, community, federation)
//! - **Constitutional Amendment**: Network-level changes affecting the commons
//! - **Policy Amendment**: Changes to operational policies within a jurisdiction
//!
//! # Ratification Levels
//!
//! Amendments require different levels of approval based on their scope:
//! - Jurisdiction: Simple governance vote within the jurisdiction
//! - Federation: Majority of member jurisdictions must ratify
//! - Network: Super-majority of federations + commons holder threshold

use crate::Timestamp;
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

/// Unique identifier for an amendment
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AmendmentId(pub [u8; 32]);

impl AmendmentId {
    /// Create from bytes
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Generate from content hash
    pub fn from_content(content: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(content);
        let result = hasher.finalize();
        let mut id = [0u8; 32];
        id.copy_from_slice(&result);
        Self(id)
    }

    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Get underlying bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for AmendmentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Type of amendment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmendmentType {
    /// Changes to a jurisdiction's charter
    Charter,
    /// Network-level constitutional changes
    Constitutional,
    /// Policy changes within a jurisdiction
    Policy,
    /// Economic policy changes (credit limits, fees, etc.)
    Economic,
    /// Governance process changes (voting rules, thresholds)
    Governance,
}

impl fmt::Display for AmendmentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AmendmentType::Charter => write!(f, "Charter"),
            AmendmentType::Constitutional => write!(f, "Constitutional"),
            AmendmentType::Policy => write!(f, "Policy"),
            AmendmentType::Economic => write!(f, "Economic"),
            AmendmentType::Governance => write!(f, "Governance"),
        }
    }
}

/// Scope of an amendment (determines ratification requirements)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmendmentScope {
    /// Single jurisdiction (coop, community)
    Jurisdiction { domain_id: String },
    /// Federation-wide
    Federation { federation_id: String },
    /// Network-wide constitutional change
    Network,
}

impl fmt::Display for AmendmentScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AmendmentScope::Jurisdiction { domain_id } => write!(f, "Jurisdiction:{domain_id}"),
            AmendmentScope::Federation { federation_id } => {
                write!(f, "Federation:{federation_id}")
            }
            AmendmentScope::Network => write!(f, "Network"),
        }
    }
}

/// Lifecycle state of an amendment
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmendmentStatus {
    /// Initial draft, not yet submitted
    Draft,
    /// Submitted for review
    Submitted { submitted_at: Timestamp },
    /// Under review period before voting
    UnderReview {
        submitted_at: Timestamp,
        review_ends_at: Timestamp,
    },
    /// Open for voting/ratification
    Voting {
        voting_started_at: Timestamp,
        voting_ends_at: Timestamp,
    },
    /// Ratification in progress (for multi-level)
    Ratifying {
        started_at: Timestamp,
        deadline: Timestamp,
    },
    /// Successfully ratified
    Ratified {
        ratified_at: Timestamp,
        effective_at: Timestamp,
    },
    /// Rejected by vote
    Rejected {
        rejected_at: Timestamp,
        reason: String,
    },
    /// Withdrawn by proposer
    Withdrawn {
        withdrawn_at: Timestamp,
        reason: String,
    },
    /// Expired without sufficient ratification
    Expired { expired_at: Timestamp },
}

impl AmendmentStatus {
    /// Check if amendment is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            AmendmentStatus::Ratified { .. }
                | AmendmentStatus::Rejected { .. }
                | AmendmentStatus::Withdrawn { .. }
                | AmendmentStatus::Expired { .. }
        )
    }

    /// Check if amendment is active (can accept votes)
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            AmendmentStatus::Voting { .. } | AmendmentStatus::Ratifying { .. }
        )
    }
}

/// Ratification requirements for an amendment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatificationRequirements {
    /// Minimum quorum (percentage of eligible voters/jurisdictions)
    pub quorum_percent: u32,
    /// Approval threshold (percentage of votes needed)
    pub approval_percent: u32,
    /// Voting period in seconds
    pub voting_period_secs: u64,
    /// Review period before voting (seconds)
    pub review_period_secs: u64,
    /// For federation/network scope: minimum jurisdictions that must ratify
    pub min_jurisdictions: Option<u32>,
    /// For network scope: minimum federations that must ratify
    pub min_federations: Option<u32>,
    /// Grace period after ratification before taking effect (seconds)
    pub effective_delay_secs: u64,
}

impl RatificationRequirements {
    /// Default requirements for jurisdiction-level amendments
    pub fn jurisdiction_default() -> Self {
        Self {
            quorum_percent: 50,
            approval_percent: 67,                  // 2/3 majority
            voting_period_secs: 14 * 24 * 60 * 60, // 14 days
            review_period_secs: 7 * 24 * 60 * 60,  // 7 days
            min_jurisdictions: None,
            min_federations: None,
            effective_delay_secs: 7 * 24 * 60 * 60, // 7 day grace period
        }
    }

    /// Default requirements for federation-level amendments
    pub fn federation_default() -> Self {
        Self {
            quorum_percent: 60,
            approval_percent: 67,
            voting_period_secs: 21 * 24 * 60 * 60, // 21 days
            review_period_secs: 14 * 24 * 60 * 60, // 14 days
            min_jurisdictions: Some(3),            // At least 3 jurisdictions must ratify
            min_federations: None,
            effective_delay_secs: 14 * 24 * 60 * 60, // 14 day grace period
        }
    }

    /// Default requirements for network-level constitutional amendments
    pub fn network_default() -> Self {
        Self {
            quorum_percent: 75,
            approval_percent: 80,                    // Super-majority
            voting_period_secs: 30 * 24 * 60 * 60,   // 30 days
            review_period_secs: 21 * 24 * 60 * 60,   // 21 days review
            min_jurisdictions: Some(10),             // At least 10 jurisdictions
            min_federations: Some(3),                // At least 3 federations
            effective_delay_secs: 30 * 24 * 60 * 60, // 30 day grace period
        }
    }

    /// Strict requirements for critical changes (identity, rights)
    pub fn critical() -> Self {
        Self {
            quorum_percent: 80,
            approval_percent: 90,                  // Near-unanimous
            voting_period_secs: 45 * 24 * 60 * 60, // 45 days
            review_period_secs: 30 * 24 * 60 * 60, // 30 days
            min_jurisdictions: Some(20),
            min_federations: Some(5),
            effective_delay_secs: 60 * 24 * 60 * 60, // 60 day grace period
        }
    }
}

/// A ratification from a jurisdiction or federation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ratification {
    /// Who is ratifying (jurisdiction or federation ID)
    pub ratifier_id: String,
    /// Type of ratifier
    pub ratifier_type: RatifierType,
    /// DID of the authority who signed the ratification
    pub authority_did: Did,
    /// Whether they approve
    pub approved: bool,
    /// Timestamp of ratification
    pub timestamp: Timestamp,
    /// Optional comment
    pub comment: Option<String>,
    /// Signature over the ratification
    pub signature: Vec<u8>,
}

/// Type of entity providing ratification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RatifierType {
    /// Individual commons holder vote
    CommonsHolder,
    /// Jurisdiction governance decision
    Jurisdiction,
    /// Federation governance decision
    Federation,
}

/// Full amendment record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Amendment {
    /// Unique identifier
    pub id: AmendmentId,
    /// Type of amendment
    pub amendment_type: AmendmentType,
    /// Scope determines ratification requirements
    pub scope: AmendmentScope,
    /// Current status
    pub status: AmendmentStatus,
    /// Title of the amendment
    pub title: String,
    /// Full description/text of the amendment
    pub description: String,
    /// Hash of the full amendment document (for long documents)
    pub document_hash: Option<[u8; 32]>,
    /// Specific changes being made (structured)
    pub changes: Vec<AmendmentChange>,
    /// DID of the proposer
    pub proposer: Did,
    /// Co-sponsors (DIDs that endorsed the proposal)
    pub sponsors: Vec<Did>,
    /// Ratification requirements
    pub requirements: RatificationRequirements,
    /// Ratifications received
    pub ratifications: Vec<Ratification>,
    /// Related proposal ID (if created via governance proposal)
    pub proposal_id: Option<String>,
    /// Reference to parent charter (for charter amendments)
    pub charter_id: Option<[u8; 32]>,
    /// Supersedes previous amendment (for revisions)
    pub supersedes: Option<AmendmentId>,
    /// When created
    pub created_at: Timestamp,
    /// When last updated
    pub updated_at: Timestamp,
}

/// A specific change within an amendment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmendmentChange {
    /// What is being changed
    pub target: ChangeTarget,
    /// Type of change
    pub change_type: ChangeType,
    /// Human-readable description of the change
    pub description: String,
    /// Previous value (if modifying)
    pub old_value: Option<String>,
    /// New value
    pub new_value: String,
}

/// What the change targets
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeTarget {
    /// Governance rules
    GovernanceRules,
    /// Membership policy
    MembershipPolicy,
    /// Economic policy
    EconomicPolicy,
    /// Dispute resolution policy
    DisputePolicy,
    /// Commons rights
    CommonsRights,
    /// Steward requirements
    StewardRequirements,
    /// Network parameters
    NetworkParameters,
    /// Custom/other
    Other { name: String },
}

/// Type of change being made
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeType {
    /// Adding something new
    Add,
    /// Modifying existing
    Modify,
    /// Removing
    Remove,
    /// Replacing entirely
    Replace,
}

impl Amendment {
    /// Create a new amendment in draft status
    pub fn new(
        amendment_type: AmendmentType,
        scope: AmendmentScope,
        title: String,
        description: String,
        proposer: Did,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Generate ID from content
        let content = format!("{amendment_type}:{scope}:{title}:{now}");
        let id = AmendmentId::from_content(content.as_bytes());

        // Default requirements based on scope
        let requirements = match &scope {
            AmendmentScope::Jurisdiction { .. } => RatificationRequirements::jurisdiction_default(),
            AmendmentScope::Federation { .. } => RatificationRequirements::federation_default(),
            AmendmentScope::Network => RatificationRequirements::network_default(),
        };

        Self {
            id,
            amendment_type,
            scope,
            status: AmendmentStatus::Draft,
            title,
            description,
            document_hash: None,
            changes: Vec::new(),
            proposer,
            sponsors: Vec::new(),
            requirements,
            ratifications: Vec::new(),
            proposal_id: None,
            charter_id: None,
            supersedes: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a charter amendment
    pub fn charter_amendment(
        domain_id: String,
        charter_id: [u8; 32],
        title: String,
        description: String,
        proposer: Did,
    ) -> Self {
        let mut amendment = Self::new(
            AmendmentType::Charter,
            AmendmentScope::Jurisdiction { domain_id },
            title,
            description,
            proposer,
        );
        amendment.charter_id = Some(charter_id);
        amendment
    }

    /// Create a constitutional amendment
    pub fn constitutional(title: String, description: String, proposer: Did) -> Self {
        Self::new(
            AmendmentType::Constitutional,
            AmendmentScope::Network,
            title,
            description,
            proposer,
        )
    }

    /// Add a change to the amendment
    pub fn add_change(&mut self, change: AmendmentChange) {
        self.changes.push(change);
        self.touch();
    }

    /// Add a sponsor
    pub fn add_sponsor(&mut self, sponsor: Did) {
        if !self.sponsors.contains(&sponsor) && sponsor != self.proposer {
            self.sponsors.push(sponsor);
            self.touch();
        }
    }

    /// Submit the amendment for review
    pub fn submit(&mut self) -> Result<(), &'static str> {
        if !matches!(self.status, AmendmentStatus::Draft) {
            return Err("Can only submit amendments in Draft status");
        }
        if self.changes.is_empty() {
            return Err("Amendment must have at least one change");
        }

        let now = self.now();
        self.status = AmendmentStatus::Submitted { submitted_at: now };
        self.touch();
        Ok(())
    }

    /// Begin review period
    pub fn begin_review(&mut self) -> Result<(), &'static str> {
        let submitted_at = match &self.status {
            AmendmentStatus::Submitted { submitted_at } => *submitted_at,
            _ => return Err("Can only begin review from Submitted status"),
        };

        let now = self.now();
        let review_ends_at = now + self.requirements.review_period_secs;
        self.status = AmendmentStatus::UnderReview {
            submitted_at,
            review_ends_at,
        };
        self.touch();
        Ok(())
    }

    /// Open voting period
    pub fn open_voting(&mut self) -> Result<(), &'static str> {
        match &self.status {
            AmendmentStatus::UnderReview { review_ends_at, .. } => {
                let now = self.now();
                if now < *review_ends_at {
                    return Err("Review period has not ended");
                }
            }
            AmendmentStatus::Submitted { .. } => {
                // Allow skipping review if no review period
                if self.requirements.review_period_secs > 0 {
                    return Err("Must complete review period first");
                }
            }
            _ => return Err("Invalid status for opening voting"),
        }

        let now = self.now();
        self.status = AmendmentStatus::Voting {
            voting_started_at: now,
            voting_ends_at: now + self.requirements.voting_period_secs,
        };
        self.touch();
        Ok(())
    }

    /// Add a ratification
    pub fn add_ratification(&mut self, ratification: Ratification) -> Result<(), &'static str> {
        if !self.status.is_active() {
            return Err("Amendment is not active for ratification");
        }

        // Check if already ratified by this entity
        if self
            .ratifications
            .iter()
            .any(|r| r.ratifier_id == ratification.ratifier_id)
        {
            return Err("Already ratified by this entity");
        }

        self.ratifications.push(ratification);
        self.touch();
        Ok(())
    }

    /// Check if ratification requirements are met
    pub fn check_ratification(&self) -> RatificationResult {
        let total_ratifications = self.ratifications.len();
        let approvals = self.ratifications.iter().filter(|r| r.approved).count();
        let rejections = total_ratifications - approvals;

        // Count by type
        let jurisdiction_approvals = self
            .ratifications
            .iter()
            .filter(|r| r.approved && r.ratifier_type == RatifierType::Jurisdiction)
            .count();
        let federation_approvals = self
            .ratifications
            .iter()
            .filter(|r| r.approved && r.ratifier_type == RatifierType::Federation)
            .count();

        // Check minimum jurisdiction requirement
        if let Some(min) = self.requirements.min_jurisdictions {
            if jurisdiction_approvals < min as usize {
                return RatificationResult::Pending {
                    approvals,
                    rejections,
                    reason: format!(
                        "Need {min} jurisdiction approvals, have {jurisdiction_approvals}"
                    ),
                };
            }
        }

        // Check minimum federation requirement
        if let Some(min) = self.requirements.min_federations {
            if federation_approvals < min as usize {
                return RatificationResult::Pending {
                    approvals,
                    rejections,
                    reason: format!("Need {min} federation approvals, have {federation_approvals}"),
                };
            }
        }

        // Check approval threshold
        if total_ratifications > 0 {
            let approval_percent = (approvals * 100) / total_ratifications;
            if approval_percent >= self.requirements.approval_percent as usize {
                return RatificationResult::Approved {
                    approvals,
                    rejections,
                };
            }

            // Check if rejection is inevitable
            let rejection_percent = (rejections * 100) / total_ratifications;
            if rejection_percent > (100 - self.requirements.approval_percent as usize) {
                return RatificationResult::Rejected {
                    approvals,
                    rejections,
                    reason: "Cannot achieve approval threshold".to_string(),
                };
            }
        }

        RatificationResult::Pending {
            approvals,
            rejections,
            reason: "Awaiting more ratifications".to_string(),
        }
    }

    /// Finalize the amendment as ratified
    pub fn ratify(&mut self) -> Result<(), &'static str> {
        if !matches!(
            self.check_ratification(),
            RatificationResult::Approved { .. }
        ) {
            return Err("Ratification requirements not met");
        }

        let now = self.now();
        self.status = AmendmentStatus::Ratified {
            ratified_at: now,
            effective_at: now + self.requirements.effective_delay_secs,
        };
        self.touch();
        Ok(())
    }

    /// Reject the amendment
    pub fn reject(&mut self, reason: String) -> Result<(), &'static str> {
        if self.status.is_terminal() {
            return Err("Amendment already in terminal state");
        }

        let now = self.now();
        self.status = AmendmentStatus::Rejected {
            rejected_at: now,
            reason,
        };
        self.touch();
        Ok(())
    }

    /// Withdraw the amendment
    pub fn withdraw(&mut self, reason: String) -> Result<(), &'static str> {
        if self.status.is_terminal() {
            return Err("Amendment already in terminal state");
        }

        let now = self.now();
        self.status = AmendmentStatus::Withdrawn {
            withdrawn_at: now,
            reason,
        };
        self.touch();
        Ok(())
    }

    /// Mark as expired
    pub fn expire(&mut self) -> Result<(), &'static str> {
        if self.status.is_terminal() {
            return Err("Amendment already in terminal state");
        }

        let now = self.now();
        self.status = AmendmentStatus::Expired { expired_at: now };
        self.touch();
        Ok(())
    }

    /// Set custom requirements
    pub fn with_requirements(mut self, requirements: RatificationRequirements) -> Self {
        self.requirements = requirements;
        self
    }

    /// Link to a governance proposal
    pub fn with_proposal(mut self, proposal_id: String) -> Self {
        self.proposal_id = Some(proposal_id);
        self
    }

    fn now(&self) -> Timestamp {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn touch(&mut self) {
        self.updated_at = self.now();
    }
}

/// Result of checking ratification status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RatificationResult {
    /// Requirements met
    Approved { approvals: usize, rejections: usize },
    /// Requirements not met yet
    Pending {
        approvals: usize,
        rejections: usize,
        reason: String,
    },
    /// Cannot achieve approval
    Rejected {
        approvals: usize,
        rejections: usize,
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_did() -> Did {
        Did::from_anchor_id(&[42u8; 32])
    }

    fn test_did_2() -> Did {
        Did::from_anchor_id(&[43u8; 32])
    }

    #[test]
    fn test_amendment_creation() {
        let amendment = Amendment::new(
            AmendmentType::Charter,
            AmendmentScope::Jurisdiction {
                domain_id: "coop:test".to_string(),
            },
            "Update voting threshold".to_string(),
            "Change quorum from 50% to 60%".to_string(),
            test_did(),
        );

        assert!(matches!(amendment.status, AmendmentStatus::Draft));
        assert_eq!(amendment.amendment_type, AmendmentType::Charter);
        assert!(amendment.changes.is_empty());
    }

    #[test]
    fn test_charter_amendment() {
        let amendment = Amendment::charter_amendment(
            "coop:test".to_string(),
            [1u8; 32],
            "Test Amendment".to_string(),
            "Description".to_string(),
            test_did(),
        );

        assert_eq!(amendment.charter_id, Some([1u8; 32]));
        assert_eq!(amendment.amendment_type, AmendmentType::Charter);
    }

    #[test]
    fn test_constitutional_amendment() {
        let amendment = Amendment::constitutional(
            "Network Upgrade".to_string(),
            "Major protocol change".to_string(),
            test_did(),
        );

        assert_eq!(amendment.amendment_type, AmendmentType::Constitutional);
        assert!(matches!(amendment.scope, AmendmentScope::Network));
        // Network amendments should have stricter requirements
        assert!(amendment.requirements.approval_percent >= 80);
    }

    #[test]
    fn test_add_change() {
        let mut amendment = Amendment::new(
            AmendmentType::Policy,
            AmendmentScope::Jurisdiction {
                domain_id: "coop:test".to_string(),
            },
            "Policy Update".to_string(),
            "Description".to_string(),
            test_did(),
        );

        amendment.add_change(AmendmentChange {
            target: ChangeTarget::GovernanceRules,
            change_type: ChangeType::Modify,
            description: "Increase quorum".to_string(),
            old_value: Some("50".to_string()),
            new_value: "60".to_string(),
        });

        assert_eq!(amendment.changes.len(), 1);
    }

    #[test]
    fn test_add_sponsor() {
        let mut amendment = Amendment::new(
            AmendmentType::Charter,
            AmendmentScope::Jurisdiction {
                domain_id: "coop:test".to_string(),
            },
            "Test".to_string(),
            "Description".to_string(),
            test_did(),
        );

        // Can add different sponsor
        amendment.add_sponsor(test_did_2());
        assert_eq!(amendment.sponsors.len(), 1);

        // Cannot add proposer as sponsor
        amendment.add_sponsor(test_did());
        assert_eq!(amendment.sponsors.len(), 1);

        // Cannot add duplicate sponsor
        amendment.add_sponsor(test_did_2());
        assert_eq!(amendment.sponsors.len(), 1);
    }

    #[test]
    fn test_submit_requires_changes() {
        let mut amendment = Amendment::new(
            AmendmentType::Charter,
            AmendmentScope::Jurisdiction {
                domain_id: "coop:test".to_string(),
            },
            "Test".to_string(),
            "Description".to_string(),
            test_did(),
        );

        // Cannot submit without changes
        assert!(amendment.submit().is_err());

        // Add a change
        amendment.add_change(AmendmentChange {
            target: ChangeTarget::MembershipPolicy,
            change_type: ChangeType::Add,
            description: "Add requirement".to_string(),
            old_value: None,
            new_value: "New requirement".to_string(),
        });

        // Now can submit
        assert!(amendment.submit().is_ok());
        assert!(matches!(
            amendment.status,
            AmendmentStatus::Submitted { .. }
        ));
    }

    #[test]
    fn test_amendment_lifecycle() {
        let mut amendment = Amendment::new(
            AmendmentType::Charter,
            AmendmentScope::Jurisdiction {
                domain_id: "coop:test".to_string(),
            },
            "Test".to_string(),
            "Description".to_string(),
            test_did(),
        );
        // Set zero review period for testing
        amendment.requirements.review_period_secs = 0;

        amendment.add_change(AmendmentChange {
            target: ChangeTarget::GovernanceRules,
            change_type: ChangeType::Modify,
            description: "Test change".to_string(),
            old_value: None,
            new_value: "new".to_string(),
        });

        // Submit
        amendment.submit().unwrap();
        assert!(matches!(
            amendment.status,
            AmendmentStatus::Submitted { .. }
        ));

        // Open voting (skip review since period is 0)
        amendment.open_voting().unwrap();
        assert!(matches!(amendment.status, AmendmentStatus::Voting { .. }));
        assert!(amendment.status.is_active());
    }

    #[test]
    fn test_ratification() {
        let mut amendment = Amendment::new(
            AmendmentType::Charter,
            AmendmentScope::Jurisdiction {
                domain_id: "coop:test".to_string(),
            },
            "Test".to_string(),
            "Description".to_string(),
            test_did(),
        );
        amendment.requirements.review_period_secs = 0;
        amendment.requirements.min_jurisdictions = None;
        amendment.requirements.approval_percent = 50;

        amendment.add_change(AmendmentChange {
            target: ChangeTarget::GovernanceRules,
            change_type: ChangeType::Modify,
            description: "Test".to_string(),
            old_value: None,
            new_value: "new".to_string(),
        });

        amendment.submit().unwrap();
        amendment.open_voting().unwrap();

        // Add approving ratification
        amendment
            .add_ratification(Ratification {
                ratifier_id: "holder1".to_string(),
                ratifier_type: RatifierType::CommonsHolder,
                authority_did: test_did(),
                approved: true,
                timestamp: 12345,
                comment: None,
                signature: vec![0u8; 64],
            })
            .unwrap();

        // Check ratification - should be approved with 100%
        let result = amendment.check_ratification();
        assert!(matches!(result, RatificationResult::Approved { .. }));

        // Finalize
        amendment.ratify().unwrap();
        assert!(matches!(amendment.status, AmendmentStatus::Ratified { .. }));
        assert!(amendment.status.is_terminal());
    }

    #[test]
    fn test_ratification_rejection() {
        let mut amendment = Amendment::new(
            AmendmentType::Charter,
            AmendmentScope::Jurisdiction {
                domain_id: "coop:test".to_string(),
            },
            "Test".to_string(),
            "Description".to_string(),
            test_did(),
        );
        amendment.requirements.review_period_secs = 0;
        amendment.requirements.min_jurisdictions = None;
        amendment.requirements.approval_percent = 67; // Need 2/3

        amendment.add_change(AmendmentChange {
            target: ChangeTarget::GovernanceRules,
            change_type: ChangeType::Modify,
            description: "Test".to_string(),
            old_value: None,
            new_value: "new".to_string(),
        });

        amendment.submit().unwrap();
        amendment.open_voting().unwrap();

        // Add one approval and two rejections
        amendment
            .add_ratification(Ratification {
                ratifier_id: "holder1".to_string(),
                ratifier_type: RatifierType::CommonsHolder,
                authority_did: test_did(),
                approved: true,
                timestamp: 12345,
                comment: None,
                signature: vec![0u8; 64],
            })
            .unwrap();

        amendment
            .add_ratification(Ratification {
                ratifier_id: "holder2".to_string(),
                ratifier_type: RatifierType::CommonsHolder,
                authority_did: test_did_2(),
                approved: false,
                timestamp: 12346,
                comment: Some("Oppose".to_string()),
                signature: vec![0u8; 64],
            })
            .unwrap();

        amendment
            .add_ratification(Ratification {
                ratifier_id: "holder3".to_string(),
                ratifier_type: RatifierType::CommonsHolder,
                authority_did: Did::from_anchor_id(&[44u8; 32]),
                approved: false,
                timestamp: 12347,
                comment: None,
                signature: vec![0u8; 64],
            })
            .unwrap();

        // 1/3 approval (33%) cannot reach 67% threshold
        let result = amendment.check_ratification();
        assert!(matches!(result, RatificationResult::Rejected { .. }));
    }

    #[test]
    fn test_withdraw() {
        let mut amendment = Amendment::new(
            AmendmentType::Charter,
            AmendmentScope::Jurisdiction {
                domain_id: "coop:test".to_string(),
            },
            "Test".to_string(),
            "Description".to_string(),
            test_did(),
        );

        amendment.withdraw("Changed mind".to_string()).unwrap();
        assert!(matches!(
            amendment.status,
            AmendmentStatus::Withdrawn { ref reason, .. } if reason == "Changed mind"
        ));
        assert!(amendment.status.is_terminal());
    }

    #[test]
    fn test_federation_requirements() {
        let amendment = Amendment::new(
            AmendmentType::Charter,
            AmendmentScope::Federation {
                federation_id: "federation:pacific-nw".to_string(),
            },
            "Federation Change".to_string(),
            "Description".to_string(),
            test_did(),
        );

        // Federation requirements should include min_jurisdictions
        assert!(amendment.requirements.min_jurisdictions.is_some());
        assert!(amendment.requirements.voting_period_secs > 14 * 24 * 60 * 60);
    }

    #[test]
    fn test_network_requirements() {
        let amendment = Amendment::constitutional(
            "Network Change".to_string(),
            "Major change".to_string(),
            test_did(),
        );

        // Network requirements should be strictest
        assert!(amendment.requirements.min_jurisdictions.is_some());
        assert!(amendment.requirements.min_federations.is_some());
        assert!(amendment.requirements.approval_percent >= 80);
    }

    #[test]
    fn test_duplicate_ratification_prevented() {
        let mut amendment = Amendment::new(
            AmendmentType::Charter,
            AmendmentScope::Jurisdiction {
                domain_id: "coop:test".to_string(),
            },
            "Test".to_string(),
            "Description".to_string(),
            test_did(),
        );
        amendment.requirements.review_period_secs = 0;

        amendment.add_change(AmendmentChange {
            target: ChangeTarget::GovernanceRules,
            change_type: ChangeType::Modify,
            description: "Test".to_string(),
            old_value: None,
            new_value: "new".to_string(),
        });

        amendment.submit().unwrap();
        amendment.open_voting().unwrap();

        // First ratification succeeds
        amendment
            .add_ratification(Ratification {
                ratifier_id: "holder1".to_string(),
                ratifier_type: RatifierType::CommonsHolder,
                authority_did: test_did(),
                approved: true,
                timestamp: 12345,
                comment: None,
                signature: vec![0u8; 64],
            })
            .unwrap();

        // Duplicate from same ratifier fails
        let result = amendment.add_ratification(Ratification {
            ratifier_id: "holder1".to_string(),
            ratifier_type: RatifierType::CommonsHolder,
            authority_did: test_did(),
            approved: false, // Even if vote changes
            timestamp: 12346,
            comment: None,
            signature: vec![0u8; 64],
        });

        assert!(result.is_err());
    }
}
