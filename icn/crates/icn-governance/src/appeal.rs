//! Appeal - Due process mechanisms for governance decisions
//!
//! This module implements the appeal system for challenging governance decisions,
//! revocations, and other actions that affect member rights. Appeals are a core
//! component of due process in the commons.
//!
//! # Appeal Types
//!
//! - **Revocation Appeal**: Challenge a revocation of membership, anchor, or holder status
//! - **Suspension Appeal**: Challenge a suspension
//! - **Governance Appeal**: Challenge a governance decision
//! - **Dispute Appeal**: Challenge a dispute resolution outcome
//!
//! # Appeal Process
//!
//! 1. Filing: Appellant files appeal with grounds and evidence
//! 2. Review: Appeal body reviews the case
//! 3. Hearing: Optional hearing/discussion period
//! 4. Resolution: Decision rendered (Upheld, Denied, Remanded)

use crate::Timestamp;
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

/// Unique identifier for an appeal
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AppealId(pub [u8; 32]);

impl AppealId {
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

impl fmt::Display for AppealId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Type of appeal
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppealType {
    /// Appeal of a revocation
    Revocation {
        /// The revocation ID being appealed
        revocation_id: [u8; 32],
        /// What was revoked
        revocation_type: String,
    },
    /// Appeal of a suspension
    Suspension {
        /// ID of the suspended entity
        target_id: String,
        /// Type of suspension
        suspension_type: String,
    },
    /// Appeal of a governance decision
    GovernanceDecision {
        /// The proposal ID
        proposal_id: String,
        /// The decision being challenged
        decision: String,
    },
    /// Appeal of a dispute resolution
    DisputeResolution {
        /// The dispute ID
        dispute_id: String,
        /// The resolution being challenged
        resolution: String,
    },
    /// Appeal of membership denial
    MembershipDenial {
        /// The jurisdiction
        jurisdiction_id: String,
        /// The application ID
        application_id: Option<String>,
    },
    /// Appeal of steward action
    StewardAction {
        /// The steward DID
        steward_did: Did,
        /// Description of action
        action: String,
    },
    /// General appeal (other)
    Other {
        /// Category
        category: String,
        /// Description
        description: String,
    },
}

impl fmt::Display for AppealType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppealType::Revocation { .. } => write!(f, "Revocation"),
            AppealType::Suspension { .. } => write!(f, "Suspension"),
            AppealType::GovernanceDecision { .. } => write!(f, "GovernanceDecision"),
            AppealType::DisputeResolution { .. } => write!(f, "DisputeResolution"),
            AppealType::MembershipDenial { .. } => write!(f, "MembershipDenial"),
            AppealType::StewardAction { .. } => write!(f, "StewardAction"),
            AppealType::Other { category, .. } => write!(f, "Other:{category}"),
        }
    }
}

/// Scope of the appeal (determines appeal body)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppealScope {
    /// Jurisdiction-level appeal (decided by jurisdiction governance)
    Jurisdiction { domain_id: String },
    /// Federation-level appeal
    Federation { federation_id: String },
    /// Network-level appeal (highest level)
    Network,
}

impl fmt::Display for AppealScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppealScope::Jurisdiction { domain_id } => write!(f, "Jurisdiction:{domain_id}"),
            AppealScope::Federation { federation_id } => write!(f, "Federation:{federation_id}"),
            AppealScope::Network => write!(f, "Network"),
        }
    }
}

/// Status of an appeal
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppealStatus {
    /// Appeal filed, awaiting review
    Filed { filed_at: Timestamp },
    /// Under review by appeal body
    UnderReview {
        filed_at: Timestamp,
        review_started_at: Timestamp,
    },
    /// Hearing period (discussion/evidence submission)
    Hearing {
        filed_at: Timestamp,
        hearing_started_at: Timestamp,
        hearing_ends_at: Timestamp,
    },
    /// Appeal resolved
    Resolved {
        filed_at: Timestamp,
        resolved_at: Timestamp,
        outcome: AppealOutcome,
    },
    /// Appeal dismissed (procedural issues)
    Dismissed {
        filed_at: Timestamp,
        dismissed_at: Timestamp,
        reason: String,
    },
    /// Appeal withdrawn by appellant
    Withdrawn {
        filed_at: Timestamp,
        withdrawn_at: Timestamp,
        reason: Option<String>,
    },
}

impl AppealStatus {
    /// Check if appeal is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            AppealStatus::Resolved { .. }
                | AppealStatus::Dismissed { .. }
                | AppealStatus::Withdrawn { .. }
        )
    }

    /// Check if appeal is active
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            AppealStatus::Filed { .. }
                | AppealStatus::UnderReview { .. }
                | AppealStatus::Hearing { .. }
        )
    }

    /// Get the outcome if resolved
    pub fn outcome(&self) -> Option<&AppealOutcome> {
        match self {
            AppealStatus::Resolved { outcome, .. } => Some(outcome),
            _ => None,
        }
    }
}

/// Outcome of a resolved appeal
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppealOutcome {
    /// Appeal upheld - original decision overturned
    Upheld {
        /// Explanation
        reason: String,
        /// Remedy ordered
        remedy: AppealRemedy,
    },
    /// Appeal denied - original decision stands
    Denied {
        /// Explanation
        reason: String,
    },
    /// Appeal partially upheld
    PartiallyUpheld {
        /// Explanation
        reason: String,
        /// Partial remedy
        remedy: AppealRemedy,
    },
    /// Remanded for further consideration
    Remanded {
        /// Instructions
        instructions: String,
        /// To which body
        remanded_to: String,
    },
}

/// Remedy ordered if appeal is upheld
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppealRemedy {
    /// Reverse the original decision
    Reverse,
    /// Reinstate membership/status
    Reinstate,
    /// Modify the decision
    Modify { modification: String },
    /// Compensation required
    Compensation { amount: u64, currency: String },
    /// Custom remedy
    Custom { description: String },
    /// No specific remedy (declaratory)
    None,
}

/// Grounds for appeal
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppealGrounds {
    /// Procedural error in original decision
    ProceduralError { description: String },
    /// New evidence not considered
    NewEvidence { description: String },
    /// Decision exceeded authority
    ExceededAuthority { description: String },
    /// Violation of rights
    RightsViolation { right_violated: String },
    /// Factual error
    FactualError { description: String },
    /// Bias or conflict of interest
    Bias { description: String },
    /// Disproportionate penalty
    DisproportionatePenalty { description: String },
    /// Other grounds
    Other { description: String },
}

impl fmt::Display for AppealGrounds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppealGrounds::ProceduralError { .. } => write!(f, "Procedural Error"),
            AppealGrounds::NewEvidence { .. } => write!(f, "New Evidence"),
            AppealGrounds::ExceededAuthority { .. } => write!(f, "Exceeded Authority"),
            AppealGrounds::RightsViolation { .. } => write!(f, "Rights Violation"),
            AppealGrounds::FactualError { .. } => write!(f, "Factual Error"),
            AppealGrounds::Bias { .. } => write!(f, "Bias"),
            AppealGrounds::DisproportionatePenalty { .. } => write!(f, "Disproportionate Penalty"),
            AppealGrounds::Other { .. } => write!(f, "Other"),
        }
    }
}

/// Configuration for appeal deadlines
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppealDeadlines {
    /// Time to file appeal after original decision (seconds)
    pub filing_window_secs: u64,
    /// Review period (seconds)
    pub review_period_secs: u64,
    /// Hearing period (seconds)
    pub hearing_period_secs: u64,
    /// Maximum total appeal duration (seconds)
    pub max_duration_secs: u64,
}

impl Default for AppealDeadlines {
    fn default() -> Self {
        Self {
            filing_window_secs: 30 * 24 * 60 * 60,   // 30 days to file
            review_period_secs: 14 * 24 * 60 * 60,  // 14 days review
            hearing_period_secs: 7 * 24 * 60 * 60,  // 7 days hearing
            max_duration_secs: 90 * 24 * 60 * 60,   // 90 days max
        }
    }
}

impl AppealDeadlines {
    /// Shorter deadlines for urgent matters
    pub fn expedited() -> Self {
        Self {
            filing_window_secs: 7 * 24 * 60 * 60,  // 7 days
            review_period_secs: 3 * 24 * 60 * 60,  // 3 days
            hearing_period_secs: 2 * 24 * 60 * 60, // 2 days
            max_duration_secs: 21 * 24 * 60 * 60,  // 21 days
        }
    }

    /// Extended deadlines for complex matters
    pub fn extended() -> Self {
        Self {
            filing_window_secs: 60 * 24 * 60 * 60,  // 60 days
            review_period_secs: 30 * 24 * 60 * 60,  // 30 days
            hearing_period_secs: 14 * 24 * 60 * 60, // 14 days
            max_duration_secs: 180 * 24 * 60 * 60,  // 180 days
        }
    }
}

/// Evidence submitted with an appeal
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppealEvidence {
    /// Unique ID for this evidence item
    pub evidence_id: [u8; 32],
    /// Type of evidence
    pub evidence_type: EvidenceType,
    /// Description
    pub description: String,
    /// Hash of evidence content (for documents)
    pub content_hash: Option<[u8; 32]>,
    /// URI to evidence (if external)
    pub uri: Option<String>,
    /// Who submitted this evidence
    pub submitted_by: Did,
    /// When submitted
    pub submitted_at: Timestamp,
}

/// Type of evidence
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceType {
    /// Document (text, PDF, etc.)
    Document,
    /// Transaction record
    Transaction,
    /// Communication record
    Communication,
    /// Witness statement
    WitnessStatement,
    /// Expert opinion
    ExpertOpinion,
    /// Technical evidence (logs, etc.)
    Technical,
    /// Other
    Other,
}

/// Response to an appeal (from respondent or appeal body)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppealResponse {
    /// Who is responding
    pub responder: Did,
    /// Response type
    pub response_type: ResponseType,
    /// Content of response
    pub content: String,
    /// Evidence attached
    pub evidence: Vec<AppealEvidence>,
    /// When submitted
    pub submitted_at: Timestamp,
}

/// Type of response
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseType {
    /// Initial response from respondent
    InitialResponse,
    /// Reply from appellant
    Reply,
    /// Comment from third party
    Comment,
    /// Question from appeal body
    Question,
    /// Clarification
    Clarification,
}

/// Full appeal record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Appeal {
    /// Unique identifier
    pub id: AppealId,
    /// Type of appeal
    pub appeal_type: AppealType,
    /// Scope determines appeal body
    pub scope: AppealScope,
    /// Current status
    pub status: AppealStatus,
    /// DID of appellant
    pub appellant: Did,
    /// DID of respondent (who made the original decision)
    pub respondent: Option<Did>,
    /// Grounds for appeal
    pub grounds: Vec<AppealGrounds>,
    /// Statement of appeal
    pub statement: String,
    /// Requested remedy
    pub requested_remedy: AppealRemedy,
    /// Evidence submitted
    pub evidence: Vec<AppealEvidence>,
    /// Responses received
    pub responses: Vec<AppealResponse>,
    /// Deadlines configuration
    pub deadlines: AppealDeadlines,
    /// Appeal body members (DIDs)
    pub appeal_body: Vec<Did>,
    /// Original decision reference
    pub original_decision_ref: Option<String>,
    /// Filing deadline (must file before this)
    pub filing_deadline: Option<Timestamp>,
    /// Created timestamp
    pub created_at: Timestamp,
    /// Last updated
    pub updated_at: Timestamp,
}

impl Appeal {
    /// Create a new appeal
    pub fn new(
        appeal_type: AppealType,
        scope: AppealScope,
        appellant: Did,
        grounds: Vec<AppealGrounds>,
        statement: String,
        requested_remedy: AppealRemedy,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Generate ID from content
        let content = format!("{appeal_type}:{scope}:{appellant}:{now}");
        let id = AppealId::from_content(content.as_bytes());

        Self {
            id,
            appeal_type,
            scope,
            status: AppealStatus::Filed { filed_at: now },
            appellant,
            respondent: None,
            grounds,
            statement,
            requested_remedy,
            evidence: Vec::new(),
            responses: Vec::new(),
            deadlines: AppealDeadlines::default(),
            appeal_body: Vec::new(),
            original_decision_ref: None,
            filing_deadline: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a revocation appeal
    pub fn revocation_appeal(
        revocation_id: [u8; 32],
        revocation_type: String,
        scope: AppealScope,
        appellant: Did,
        grounds: Vec<AppealGrounds>,
        statement: String,
    ) -> Self {
        Self::new(
            AppealType::Revocation {
                revocation_id,
                revocation_type,
            },
            scope,
            appellant,
            grounds,
            statement,
            AppealRemedy::Reverse,
        )
    }

    /// Create a suspension appeal
    pub fn suspension_appeal(
        target_id: String,
        suspension_type: String,
        scope: AppealScope,
        appellant: Did,
        grounds: Vec<AppealGrounds>,
        statement: String,
    ) -> Self {
        Self::new(
            AppealType::Suspension {
                target_id,
                suspension_type,
            },
            scope,
            appellant,
            grounds,
            statement,
            AppealRemedy::Reinstate,
        )
    }

    /// Set respondent
    pub fn with_respondent(mut self, respondent: Did) -> Self {
        self.respondent = Some(respondent);
        self
    }

    /// Set deadlines
    pub fn with_deadlines(mut self, deadlines: AppealDeadlines) -> Self {
        self.deadlines = deadlines;
        self
    }

    /// Set original decision reference
    pub fn with_decision_ref(mut self, reference: String) -> Self {
        self.original_decision_ref = Some(reference);
        self
    }

    /// Set filing deadline
    pub fn with_filing_deadline(mut self, deadline: Timestamp) -> Self {
        self.filing_deadline = Some(deadline);
        self
    }

    /// Add evidence
    pub fn add_evidence(&mut self, evidence: AppealEvidence) -> Result<(), &'static str> {
        if self.status.is_terminal() {
            return Err("Cannot add evidence to terminal appeal");
        }
        self.evidence.push(evidence);
        self.touch();
        Ok(())
    }

    /// Add response
    pub fn add_response(&mut self, response: AppealResponse) -> Result<(), &'static str> {
        if self.status.is_terminal() {
            return Err("Cannot add response to terminal appeal");
        }
        self.responses.push(response);
        self.touch();
        Ok(())
    }

    /// Begin review
    pub fn begin_review(&mut self) -> Result<(), &'static str> {
        let filed_at = match &self.status {
            AppealStatus::Filed { filed_at } => *filed_at,
            _ => return Err("Can only begin review from Filed status"),
        };

        let now = self.now();
        self.status = AppealStatus::UnderReview {
            filed_at,
            review_started_at: now,
        };
        self.touch();
        Ok(())
    }

    /// Begin hearing period
    pub fn begin_hearing(&mut self) -> Result<(), &'static str> {
        let filed_at = match &self.status {
            AppealStatus::UnderReview { filed_at, .. } => *filed_at,
            _ => return Err("Can only begin hearing from UnderReview status"),
        };

        let now = self.now();
        self.status = AppealStatus::Hearing {
            filed_at,
            hearing_started_at: now,
            hearing_ends_at: now + self.deadlines.hearing_period_secs,
        };
        self.touch();
        Ok(())
    }

    /// Resolve the appeal
    pub fn resolve(&mut self, outcome: AppealOutcome) -> Result<(), &'static str> {
        if self.status.is_terminal() {
            return Err("Appeal already in terminal state");
        }

        let filed_at = match &self.status {
            AppealStatus::Filed { filed_at }
            | AppealStatus::UnderReview { filed_at, .. }
            | AppealStatus::Hearing { filed_at, .. } => *filed_at,
            _ => return Err("Invalid status for resolution"),
        };

        let now = self.now();
        self.status = AppealStatus::Resolved {
            filed_at,
            resolved_at: now,
            outcome,
        };
        self.touch();
        Ok(())
    }

    /// Dismiss the appeal
    pub fn dismiss(&mut self, reason: String) -> Result<(), &'static str> {
        if self.status.is_terminal() {
            return Err("Appeal already in terminal state");
        }

        let filed_at = match &self.status {
            AppealStatus::Filed { filed_at }
            | AppealStatus::UnderReview { filed_at, .. }
            | AppealStatus::Hearing { filed_at, .. } => *filed_at,
            _ => return Err("Invalid status for dismissal"),
        };

        let now = self.now();
        self.status = AppealStatus::Dismissed {
            filed_at,
            dismissed_at: now,
            reason,
        };
        self.touch();
        Ok(())
    }

    /// Withdraw the appeal
    pub fn withdraw(&mut self, reason: Option<String>) -> Result<(), &'static str> {
        if self.status.is_terminal() {
            return Err("Appeal already in terminal state");
        }

        let filed_at = match &self.status {
            AppealStatus::Filed { filed_at }
            | AppealStatus::UnderReview { filed_at, .. }
            | AppealStatus::Hearing { filed_at, .. } => *filed_at,
            _ => return Err("Invalid status for withdrawal"),
        };

        let now = self.now();
        self.status = AppealStatus::Withdrawn {
            filed_at,
            withdrawn_at: now,
            reason,
        };
        self.touch();
        Ok(())
    }

    /// Check if appeal is upheld
    pub fn is_upheld(&self) -> bool {
        matches!(
            self.status.outcome(),
            Some(AppealOutcome::Upheld { .. }) | Some(AppealOutcome::PartiallyUpheld { .. })
        )
    }

    /// Check if appeal is denied
    pub fn is_denied(&self) -> bool {
        matches!(self.status.outcome(), Some(AppealOutcome::Denied { .. }))
    }

    /// Check if filing deadline has passed
    pub fn is_past_filing_deadline(&self) -> bool {
        if let Some(deadline) = self.filing_deadline {
            self.now() > deadline
        } else {
            false
        }
    }

    /// Set appeal body members
    pub fn set_appeal_body(&mut self, members: Vec<Did>) {
        self.appeal_body = members;
        self.touch();
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
    fn test_appeal_creation() {
        let appeal = Appeal::new(
            AppealType::Revocation {
                revocation_id: [1u8; 32],
                revocation_type: "Membership".to_string(),
            },
            AppealScope::Jurisdiction {
                domain_id: "coop:test".to_string(),
            },
            test_did(),
            vec![AppealGrounds::ProceduralError {
                description: "No notice given".to_string(),
            }],
            "I appeal this revocation because...".to_string(),
            AppealRemedy::Reverse,
        );

        assert!(matches!(appeal.status, AppealStatus::Filed { .. }));
        assert_eq!(appeal.grounds.len(), 1);
    }

    #[test]
    fn test_revocation_appeal() {
        let appeal = Appeal::revocation_appeal(
            [1u8; 32],
            "Membership".to_string(),
            AppealScope::Jurisdiction {
                domain_id: "coop:test".to_string(),
            },
            test_did(),
            vec![AppealGrounds::RightsViolation {
                right_violated: "Due process".to_string(),
            }],
            "Statement of appeal".to_string(),
        );

        assert!(matches!(
            appeal.appeal_type,
            AppealType::Revocation { .. }
        ));
        assert!(matches!(appeal.requested_remedy, AppealRemedy::Reverse));
    }

    #[test]
    fn test_suspension_appeal() {
        let appeal = Appeal::suspension_appeal(
            "holder:123".to_string(),
            "CommonsHolder".to_string(),
            AppealScope::Network,
            test_did(),
            vec![AppealGrounds::FactualError {
                description: "Wrong person".to_string(),
            }],
            "I was wrongly suspended".to_string(),
        );

        assert!(matches!(
            appeal.appeal_type,
            AppealType::Suspension { .. }
        ));
        assert!(matches!(appeal.requested_remedy, AppealRemedy::Reinstate));
    }

    #[test]
    fn test_appeal_lifecycle() {
        let mut appeal = Appeal::new(
            AppealType::GovernanceDecision {
                proposal_id: "prop-123".to_string(),
                decision: "Rejected".to_string(),
            },
            AppealScope::Jurisdiction {
                domain_id: "coop:test".to_string(),
            },
            test_did(),
            vec![AppealGrounds::ProceduralError {
                description: "Quorum not met".to_string(),
            }],
            "Appeal statement".to_string(),
            AppealRemedy::Reverse,
        );

        // Start in Filed
        assert!(matches!(appeal.status, AppealStatus::Filed { .. }));
        assert!(appeal.status.is_active());

        // Begin review
        appeal.begin_review().unwrap();
        assert!(matches!(appeal.status, AppealStatus::UnderReview { .. }));

        // Begin hearing
        appeal.begin_hearing().unwrap();
        assert!(matches!(appeal.status, AppealStatus::Hearing { .. }));

        // Resolve
        appeal
            .resolve(AppealOutcome::Upheld {
                reason: "Procedural error confirmed".to_string(),
                remedy: AppealRemedy::Reverse,
            })
            .unwrap();

        assert!(matches!(appeal.status, AppealStatus::Resolved { .. }));
        assert!(appeal.status.is_terminal());
        assert!(appeal.is_upheld());
    }

    #[test]
    fn test_appeal_denied() {
        let mut appeal = Appeal::new(
            AppealType::Suspension {
                target_id: "holder:123".to_string(),
                suspension_type: "Member".to_string(),
            },
            AppealScope::Jurisdiction {
                domain_id: "coop:test".to_string(),
            },
            test_did(),
            vec![AppealGrounds::Other {
                description: "Unfair".to_string(),
            }],
            "Appeal".to_string(),
            AppealRemedy::Reinstate,
        );

        appeal.begin_review().unwrap();
        appeal
            .resolve(AppealOutcome::Denied {
                reason: "Suspension was justified".to_string(),
            })
            .unwrap();

        assert!(appeal.is_denied());
        assert!(!appeal.is_upheld());
    }

    #[test]
    fn test_appeal_dismissed() {
        let mut appeal = Appeal::new(
            AppealType::Other {
                category: "Test".to_string(),
                description: "Test appeal".to_string(),
            },
            AppealScope::Jurisdiction {
                domain_id: "coop:test".to_string(),
            },
            test_did(),
            vec![],
            "Appeal".to_string(),
            AppealRemedy::None,
        );

        appeal.dismiss("No grounds provided".to_string()).unwrap();
        assert!(matches!(appeal.status, AppealStatus::Dismissed { .. }));
        assert!(appeal.status.is_terminal());
    }

    #[test]
    fn test_appeal_withdrawn() {
        let mut appeal = Appeal::new(
            AppealType::MembershipDenial {
                jurisdiction_id: "coop:test".to_string(),
                application_id: Some("app-123".to_string()),
            },
            AppealScope::Jurisdiction {
                domain_id: "coop:test".to_string(),
            },
            test_did(),
            vec![AppealGrounds::ProceduralError {
                description: "Test".to_string(),
            }],
            "Appeal".to_string(),
            AppealRemedy::Custom {
                description: "Grant membership".to_string(),
            },
        );

        appeal
            .withdraw(Some("Issue resolved".to_string()))
            .unwrap();
        assert!(matches!(appeal.status, AppealStatus::Withdrawn { .. }));
    }

    #[test]
    fn test_add_evidence() {
        let mut appeal = Appeal::new(
            AppealType::Revocation {
                revocation_id: [1u8; 32],
                revocation_type: "Anchor".to_string(),
            },
            AppealScope::Network,
            test_did(),
            vec![AppealGrounds::NewEvidence {
                description: "New evidence available".to_string(),
            }],
            "Appeal".to_string(),
            AppealRemedy::Reverse,
        );

        let evidence = AppealEvidence {
            evidence_id: [2u8; 32],
            evidence_type: EvidenceType::Document,
            description: "Supporting document".to_string(),
            content_hash: Some([3u8; 32]),
            uri: None,
            submitted_by: test_did(),
            submitted_at: 12345,
        };

        appeal.add_evidence(evidence).unwrap();
        assert_eq!(appeal.evidence.len(), 1);
    }

    #[test]
    fn test_add_response() {
        let mut appeal = Appeal::new(
            AppealType::StewardAction {
                steward_did: test_did_2(),
                action: "Denied attestation".to_string(),
            },
            AppealScope::Jurisdiction {
                domain_id: "coop:test".to_string(),
            },
            test_did(),
            vec![AppealGrounds::Bias {
                description: "Personal conflict".to_string(),
            }],
            "Appeal".to_string(),
            AppealRemedy::Custom {
                description: "Reassign to different steward".to_string(),
            },
        )
        .with_respondent(test_did_2());

        let response = AppealResponse {
            responder: test_did_2(),
            response_type: ResponseType::InitialResponse,
            content: "I deny the allegations".to_string(),
            evidence: Vec::new(),
            submitted_at: 12345,
        };

        appeal.add_response(response).unwrap();
        assert_eq!(appeal.responses.len(), 1);
    }

    #[test]
    fn test_cannot_modify_terminal_appeal() {
        let mut appeal = Appeal::new(
            AppealType::Other {
                category: "Test".to_string(),
                description: "Test".to_string(),
            },
            AppealScope::Jurisdiction {
                domain_id: "coop:test".to_string(),
            },
            test_did(),
            vec![],
            "Appeal".to_string(),
            AppealRemedy::None,
        );

        appeal
            .resolve(AppealOutcome::Denied {
                reason: "Test".to_string(),
            })
            .unwrap();

        // Should fail
        assert!(appeal.begin_review().is_err());
        assert!(appeal
            .add_evidence(AppealEvidence {
                evidence_id: [1u8; 32],
                evidence_type: EvidenceType::Other,
                description: "Test".to_string(),
                content_hash: None,
                uri: None,
                submitted_by: test_did(),
                submitted_at: 12345,
            })
            .is_err());
    }

    #[test]
    fn test_appeal_deadlines() {
        let default = AppealDeadlines::default();
        assert_eq!(default.filing_window_secs, 30 * 24 * 60 * 60);

        let expedited = AppealDeadlines::expedited();
        assert!(expedited.filing_window_secs < default.filing_window_secs);

        let extended = AppealDeadlines::extended();
        assert!(extended.filing_window_secs > default.filing_window_secs);
    }

    #[test]
    fn test_partial_uphold() {
        let mut appeal = Appeal::new(
            AppealType::DisputeResolution {
                dispute_id: "dispute-123".to_string(),
                resolution: "Rejected".to_string(),
            },
            AppealScope::Federation {
                federation_id: "federation:pacific".to_string(),
            },
            test_did(),
            vec![AppealGrounds::DisproportionatePenalty {
                description: "Amount too high".to_string(),
            }],
            "Appeal".to_string(),
            AppealRemedy::Modify {
                modification: "Reduce amount".to_string(),
            },
        );

        appeal.begin_review().unwrap();
        appeal
            .resolve(AppealOutcome::PartiallyUpheld {
                reason: "Amount was excessive".to_string(),
                remedy: AppealRemedy::Compensation {
                    amount: 50,
                    currency: "credits".to_string(),
                },
            })
            .unwrap();

        assert!(appeal.is_upheld());
    }

    #[test]
    fn test_remanded() {
        let mut appeal = Appeal::new(
            AppealType::GovernanceDecision {
                proposal_id: "prop-456".to_string(),
                decision: "Accepted".to_string(),
            },
            AppealScope::Federation {
                federation_id: "federation:pacific".to_string(),
            },
            test_did(),
            vec![AppealGrounds::ProceduralError {
                description: "Missing review".to_string(),
            }],
            "Appeal".to_string(),
            AppealRemedy::Reverse,
        );

        appeal.begin_review().unwrap();
        appeal
            .resolve(AppealOutcome::Remanded {
                instructions: "Redo the vote with proper notice".to_string(),
                remanded_to: "coop:test governance body".to_string(),
            })
            .unwrap();

        assert!(!appeal.is_upheld());
        assert!(!appeal.is_denied());
    }

    #[test]
    fn test_set_appeal_body() {
        let mut appeal = Appeal::new(
            AppealType::Other {
                category: "Test".to_string(),
                description: "Test".to_string(),
            },
            AppealScope::Jurisdiction {
                domain_id: "coop:test".to_string(),
            },
            test_did(),
            vec![],
            "Appeal".to_string(),
            AppealRemedy::None,
        );

        let members = vec![
            Did::from_anchor_id(&[1u8; 32]),
            Did::from_anchor_id(&[2u8; 32]),
            Did::from_anchor_id(&[3u8; 32]),
        ];

        appeal.set_appeal_body(members.clone());
        assert_eq!(appeal.appeal_body.len(), 3);
    }
}
