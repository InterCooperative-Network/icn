//! Revocation Registry for Commons Evolution
//!
//! Provides tracking of revoked credentials, memberships, and anchors.
//! Key properties:
//! - Revocations are immutable once recorded
//! - Appeal process is supported with deadlines
//! - Scope determines visibility (global, federation, jurisdiction)

use crate::{Did, JurisdictionId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

/// Record of a revocation action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevocationRecord {
    /// Unique revocation ID (derived from target and timestamp)
    pub revocation_id: [u8; 32],

    /// What type of thing was revoked
    pub target_type: RevocationType,

    /// ID of the revoked item (hex-encoded)
    pub target_id: String,

    /// Scope of revocation
    pub scope: RevocationScope,

    /// Authority that issued revocation
    pub authority: Did,

    /// Reason for revocation
    pub reason: CommonsRevocationReason,

    /// Evidence references (hashes of evidence documents)
    pub evidence_refs: Vec<[u8; 32]>,

    /// Appeal deadline (Unix timestamp)
    pub appeal_deadline: u64,

    /// Appeal status
    pub appeal_status: AppealStatus,

    /// When revocation was created
    pub created_at: u64,

    /// When revocation takes effect (may be after appeal period)
    pub effective_at: u64,
}

impl RevocationRecord {
    /// Create a new revocation record
    pub fn new(
        target_type: RevocationType,
        target_id: String,
        scope: RevocationScope,
        authority: Did,
        reason: CommonsRevocationReason,
        appeal_window_days: u64,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let appeal_deadline = now + (appeal_window_days * 24 * 60 * 60);

        // Generate revocation ID from components + random nonce for uniqueness
        let mut nonce = [0u8; 16];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut nonce);

        let mut hasher = Sha256::new();
        hasher.update(b"revocation:");
        hasher.update(target_id.as_bytes());
        hasher.update(b":");
        hasher.update(now.to_le_bytes());
        hasher.update(b":");
        hasher.update(nonce);
        let result = hasher.finalize();
        let mut revocation_id = [0u8; 32];
        revocation_id.copy_from_slice(&result);

        Self {
            revocation_id,
            target_type,
            target_id,
            scope,
            authority,
            reason,
            evidence_refs: Vec::new(),
            appeal_deadline,
            appeal_status: AppealStatus::None,
            created_at: now,
            effective_at: appeal_deadline, // Takes effect after appeal window
        }
    }

    /// Create an immediate revocation (no appeal period)
    pub fn immediate(
        target_type: RevocationType,
        target_id: String,
        scope: RevocationScope,
        authority: Did,
        reason: CommonsRevocationReason,
    ) -> Self {
        let mut record = Self::new(target_type, target_id, scope, authority, reason, 0);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        record.effective_at = now;
        record
    }

    /// Get the revocation ID as hex
    pub fn id_hex(&self) -> String {
        hex::encode(self.revocation_id)
    }

    /// Add evidence reference
    pub fn add_evidence(&mut self, evidence_hash: [u8; 32]) {
        self.evidence_refs.push(evidence_hash);
    }

    /// Check if revocation is currently effective
    pub fn is_effective(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Not effective if successfully appealed
        if matches!(self.appeal_status, AppealStatus::Upheld) {
            return false;
        }

        now >= self.effective_at
    }

    /// Check if appeal window is still open
    pub fn can_appeal(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        matches!(self.appeal_status, AppealStatus::None) && now < self.appeal_deadline
    }

    /// File an appeal
    pub fn file_appeal(&mut self, appeal_reason: String) -> bool {
        if !self.can_appeal() {
            return false;
        }
        self.appeal_status = AppealStatus::Pending {
            filed_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            reason: appeal_reason,
        };
        true
    }

    /// Resolve an appeal
    pub fn resolve_appeal(&mut self, upheld: bool, resolution_notes: String) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if upheld {
            self.appeal_status = AppealStatus::Upheld;
        } else {
            self.appeal_status = AppealStatus::Denied {
                resolved_at: now,
                notes: resolution_notes,
            };
            // Appeal denied, revocation becomes immediately effective
            self.effective_at = now;
        }
    }

    /// Make effective immediately (e.g., after appeal window passes)
    pub fn make_effective(&mut self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.effective_at = now;
    }
}

/// Type of entity being revoked
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RevocationType {
    /// PersonhoodAnchor revocation (most severe)
    Anchor,
    /// CommonsHolderRecord revocation
    Holder,
    /// Jurisdiction membership revocation
    Membership,
    /// Specific credential revocation
    Credential,
    /// Steward status revocation
    Steward,
}

impl fmt::Display for RevocationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RevocationType::Anchor => write!(f, "Anchor"),
            RevocationType::Holder => write!(f, "Holder"),
            RevocationType::Membership => write!(f, "Membership"),
            RevocationType::Credential => write!(f, "Credential"),
            RevocationType::Steward => write!(f, "Steward"),
        }
    }
}

/// Scope of the revocation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RevocationScope {
    /// Network-wide (affects all jurisdictions)
    Global,
    /// Within a federation
    Federation { federation_id: JurisdictionId },
    /// Within a single cooperative/community
    Jurisdiction { jurisdiction_id: JurisdictionId },
}

impl fmt::Display for RevocationScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RevocationScope::Global => write!(f, "Global"),
            RevocationScope::Federation { federation_id } => {
                write!(f, "Federation({federation_id})")
            }
            RevocationScope::Jurisdiction { jurisdiction_id } => {
                write!(f, "Jurisdiction({jurisdiction_id})")
            }
        }
    }
}

/// Reason for commons/membership revocation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommonsRevocationReason {
    /// Sybil attack detected (multiple identities)
    SybilAttack { evidence_summary: String },
    /// Identity fraud
    IdentityFraud { details: String },
    /// Policy violation
    PolicyViolation { policy_ref: String, details: String },
    /// Governance decision
    GovernanceDecision {
        proposal_id: String,
        vote_outcome: String,
    },
    /// Key compromise (not punitive)
    KeyCompromise,
    /// Voluntary exit
    VoluntaryExit { reason: String },
    /// Inactive/abandoned
    Abandoned { last_activity: u64 },
    /// Custom reason
    Other { description: String },
}

impl fmt::Display for CommonsRevocationReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommonsRevocationReason::SybilAttack { .. } => write!(f, "Sybil Attack"),
            CommonsRevocationReason::IdentityFraud { .. } => write!(f, "Identity Fraud"),
            CommonsRevocationReason::PolicyViolation { policy_ref, .. } => {
                write!(f, "Policy Violation: {policy_ref}")
            }
            CommonsRevocationReason::GovernanceDecision { proposal_id, .. } => {
                write!(f, "Governance Decision: {proposal_id}")
            }
            CommonsRevocationReason::KeyCompromise => write!(f, "Key Compromise"),
            CommonsRevocationReason::VoluntaryExit { .. } => write!(f, "Voluntary Exit"),
            CommonsRevocationReason::Abandoned { .. } => write!(f, "Abandoned"),
            CommonsRevocationReason::Other { description } => write!(f, "Other: {description}"),
        }
    }
}

/// Appeal status for a revocation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AppealStatus {
    /// No appeal filed
    None,
    /// Appeal pending review
    Pending { filed_at: u64, reason: String },
    /// Appeal denied, revocation stands
    Denied { resolved_at: u64, notes: String },
    /// Appeal upheld, revocation nullified
    Upheld,
}

impl fmt::Display for AppealStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppealStatus::None => write!(f, "None"),
            AppealStatus::Pending { .. } => write!(f, "Pending"),
            AppealStatus::Denied { .. } => write!(f, "Denied"),
            AppealStatus::Upheld => write!(f, "Upheld"),
        }
    }
}

/// Check result from revocation registry
#[derive(Debug, Clone)]
pub struct RevocationCheck {
    /// Whether the target is revoked
    pub is_revoked: bool,
    /// The revocation record if revoked
    pub record: Option<RevocationRecord>,
    /// Scope at which revocation applies
    pub scope: Option<RevocationScope>,
}

impl RevocationCheck {
    /// Create a "not revoked" result
    pub fn not_revoked() -> Self {
        Self {
            is_revoked: false,
            record: None,
            scope: None,
        }
    }

    /// Create a "revoked" result
    pub fn revoked(record: RevocationRecord) -> Self {
        let scope = Some(record.scope.clone());
        Self {
            is_revoked: true,
            record: Some(record),
            scope,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::KeyPair;

    fn test_authority() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    #[test]
    fn test_revocation_record_creation() {
        let authority = test_authority();
        let record = RevocationRecord::new(
            RevocationType::Membership,
            "holder123".to_string(),
            RevocationScope::Jurisdiction {
                jurisdiction_id: JurisdictionId::coop("test-coop"),
            },
            authority,
            CommonsRevocationReason::PolicyViolation {
                policy_ref: "POL-001".to_string(),
                details: "Violated terms of service".to_string(),
            },
            30, // 30-day appeal window
        );

        assert!(!record.is_effective()); // Not effective yet (appeal window)
        assert!(record.can_appeal());
        assert!(matches!(record.appeal_status, AppealStatus::None));
    }

    #[test]
    fn test_immediate_revocation() {
        let authority = test_authority();
        let record = RevocationRecord::immediate(
            RevocationType::Anchor,
            "anchor123".to_string(),
            RevocationScope::Global,
            authority,
            CommonsRevocationReason::SybilAttack {
                evidence_summary: "Detected duplicate VUI".to_string(),
            },
        );

        assert!(record.is_effective()); // Immediate
        assert!(!record.can_appeal()); // No appeal window for immediate
    }

    #[test]
    fn test_appeal_process() {
        let authority = test_authority();
        let mut record = RevocationRecord::new(
            RevocationType::Holder,
            "holder456".to_string(),
            RevocationScope::Global,
            authority,
            CommonsRevocationReason::IdentityFraud {
                details: "Suspected fraud".to_string(),
            },
            30,
        );

        // File appeal
        assert!(record.file_appeal("I am innocent".to_string()));
        assert!(matches!(record.appeal_status, AppealStatus::Pending { .. }));

        // Can't file another appeal
        assert!(!record.file_appeal("Second appeal".to_string()));

        // Resolve appeal (upheld = revocation nullified)
        record.resolve_appeal(true, "Evidence insufficient".to_string());
        assert!(matches!(record.appeal_status, AppealStatus::Upheld));
        assert!(!record.is_effective()); // Revocation nullified
    }

    #[test]
    fn test_appeal_denied() {
        let authority = test_authority();
        let mut record = RevocationRecord::new(
            RevocationType::Steward,
            "steward789".to_string(),
            RevocationScope::Global,
            authority,
            CommonsRevocationReason::GovernanceDecision {
                proposal_id: "GOV-123".to_string(),
                vote_outcome: "Passed 75%".to_string(),
            },
            7,
        );

        record.file_appeal("Procedural error".to_string());
        record.resolve_appeal(false, "Appeal lacks merit".to_string());

        assert!(matches!(record.appeal_status, AppealStatus::Denied { .. }));
        assert!(record.is_effective()); // Now effective
    }

    #[test]
    fn test_add_evidence() {
        let authority = test_authority();
        let mut record = RevocationRecord::new(
            RevocationType::Credential,
            "cred123".to_string(),
            RevocationScope::Jurisdiction {
                jurisdiction_id: JurisdictionId::federation("pacific-nw"),
            },
            authority,
            CommonsRevocationReason::KeyCompromise,
            14,
        );

        assert!(record.evidence_refs.is_empty());

        record.add_evidence([1u8; 32]);
        record.add_evidence([2u8; 32]);

        assert_eq!(record.evidence_refs.len(), 2);
    }

    #[test]
    fn test_revocation_type_display() {
        assert_eq!(format!("{}", RevocationType::Anchor), "Anchor");
        assert_eq!(format!("{}", RevocationType::Holder), "Holder");
        assert_eq!(format!("{}", RevocationType::Membership), "Membership");
        assert_eq!(format!("{}", RevocationType::Credential), "Credential");
        assert_eq!(format!("{}", RevocationType::Steward), "Steward");
    }

    #[test]
    fn test_revocation_scope_display() {
        assert_eq!(format!("{}", RevocationScope::Global), "Global");
        assert_eq!(
            format!(
                "{}",
                RevocationScope::Federation {
                    federation_id: JurisdictionId::federation("test")
                }
            ),
            "Federation(federation:test)"
        );
    }

    #[test]
    fn test_revocation_reason_display() {
        let reason = CommonsRevocationReason::PolicyViolation {
            policy_ref: "POL-001".to_string(),
            details: "Details".to_string(),
        };
        assert!(format!("{reason}").contains("POL-001"));

        let reason2 = CommonsRevocationReason::VoluntaryExit {
            reason: "Moving".to_string(),
        };
        assert_eq!(format!("{reason2}"), "Voluntary Exit");
    }

    #[test]
    fn test_revocation_check() {
        let check = RevocationCheck::not_revoked();
        assert!(!check.is_revoked);
        assert!(check.record.is_none());

        let authority = test_authority();
        let record = RevocationRecord::immediate(
            RevocationType::Holder,
            "holder".to_string(),
            RevocationScope::Global,
            authority,
            CommonsRevocationReason::Abandoned { last_activity: 0 },
        );

        let check = RevocationCheck::revoked(record);
        assert!(check.is_revoked);
        assert!(check.record.is_some());
    }

    #[test]
    fn test_serialization() {
        let authority = test_authority();
        let record = RevocationRecord::new(
            RevocationType::Membership,
            "member123".to_string(),
            RevocationScope::Jurisdiction {
                jurisdiction_id: JurisdictionId::coop("test"),
            },
            authority,
            CommonsRevocationReason::Other {
                description: "Custom reason".to_string(),
            },
            30,
        );

        let json = serde_json::to_string(&record).unwrap();
        let deserialized: RevocationRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.target_id, record.target_id);
        assert_eq!(deserialized.revocation_id, record.revocation_id);
    }
}
