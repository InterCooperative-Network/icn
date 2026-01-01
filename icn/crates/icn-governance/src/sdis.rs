//! SDIS Governance Proposals
//!
//! Proposal types for Sovereign Digital Identity System governance:
//!
//! - Steward appointment and removal
//! - Steward sanctions and reconfirmation
//! - Threshold modifications
//! - Institutional authority approvals
//! - Revocation appeals

use icn_identity::Did;
use serde::{Deserialize, Serialize};

// ============================================================================
// Steward Governance
// ============================================================================

/// SDIS-specific proposal payloads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SdisProposal {
    /// Appoint a new steward
    ///
    /// Requires sponsors and goes through normal voting.
    /// The candidate must meet steward requirements.
    AppointSteward {
        /// DID of the candidate
        candidate: Did,
        /// DIDs of sponsors vouching for the candidate
        sponsors: Vec<Did>,
        /// Region the steward will serve
        region: String,
        /// Proposed bond amount
        bond_amount: i64,
        /// Proposed term length in seconds
        term_length: u64,
    },

    /// Remove a steward from active duty
    ///
    /// Requires super-majority for removal.
    RemoveSteward {
        /// DID of the steward to remove
        steward: Did,
        /// Reason for removal
        reason: String,
        /// Whether to return bond
        return_bond: bool,
    },

    /// Sanction a steward for misconduct
    ///
    /// Penalties range from warning to removal.
    SanctionSteward {
        /// DID of the steward to sanction
        steward: Did,
        /// Type of penalty
        penalty: StewardPenalty,
        /// Reason for sanction
        reason: String,
        /// Evidence hashes (optional)
        evidence: Vec<String>,
    },

    /// Reconfirm an existing steward for a new term
    ///
    /// Stewards must be reconfirmed periodically.
    ReconfirmSteward {
        /// DID of the steward to reconfirm
        steward: Did,
        /// New term end timestamp
        new_term_end: u64,
        /// Optional performance notes
        performance_notes: Option<String>,
    },

    /// Modify threshold parameters
    ///
    /// Changes to threshold requirements for ceremonies.
    ModifyThreshold {
        /// Which threshold to modify
        threshold_type: ThresholdType,
        /// Operation (increase or decrease)
        operation: ThresholdOp,
        /// New value
        new_value: usize,
    },

    /// Approve an institutional authority
    ///
    /// Allows an institution to issue certain attestations.
    ApproveAuthority {
        /// Authority to approve
        authority: InstitutionalAuthority,
    },

    /// Revoke an institutional authority
    ///
    /// Removes an institution's authority to issue attestations.
    RevokeAuthority {
        /// DID of the authority to revoke
        authority_did: Did,
        /// Reason for revocation
        reason: String,
        /// Effective timestamp (allows grace period)
        effective_at: Option<u64>,
    },

    /// Appeal a revocation decision
    ///
    /// Allows affected parties to appeal identity or credential revocations.
    RevocationAppeal {
        /// What was revoked
        target: RevocationTarget,
        /// Original revocation timestamp
        revoked_at: u64,
        /// Reason for appeal
        reason: String,
        /// Supporting evidence hashes
        evidence: Vec<String>,
    },

    /// Suspend a steward temporarily
    ///
    /// Less severe than removal, allows investigation.
    SuspendSteward {
        /// DID of the steward to suspend
        steward: Did,
        /// Reason for suspension
        reason: String,
        /// Duration of suspension in seconds
        duration: u64,
    },

    /// Reinstate a suspended steward
    ReinstateSteward {
        /// DID of the steward to reinstate
        steward: Did,
        /// Reason for reinstatement
        reason: String,
    },

    /// Update steward jurisdiction tier
    ///
    /// Changes a steward's jurisdiction level based on performance.
    UpdateJurisdictionTier {
        /// DID of the steward
        steward: Did,
        /// New tier
        new_tier: JurisdictionTier,
        /// Reason for change
        reason: String,
    },

    /// Emergency steward rotation
    ///
    /// Forces key rotation for a steward's signing keys.
    ForceKeyRotation {
        /// DID of the steward
        steward: Did,
        /// Reason for forced rotation
        reason: String,
    },
}

/// Penalties that can be applied to stewards
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StewardPenalty {
    /// Formal warning, no other action
    Warning,

    /// Temporary suspension
    Suspension {
        /// Duration in seconds
        duration: u64,
    },

    /// Bond reduction
    BondSlash {
        /// Amount to slash from bond
        amount: i64,
    },

    /// Tier demotion
    TierDemotion {
        /// New tier after demotion
        new_tier: JurisdictionTier,
    },

    /// Permanent removal
    Removal {
        /// Whether to return remaining bond
        return_bond: bool,
    },

    /// Probation period with enhanced monitoring
    Probation {
        /// Duration in seconds
        duration: u64,
        /// Conditions for probation
        conditions: Vec<String>,
    },
}

/// Types of thresholds that can be modified
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThresholdType {
    /// Threshold for POP ceremony (enrollment)
    PopCeremony,
    /// Threshold for recovery ceremony
    Recovery,
    /// Threshold for VUI uniqueness checking
    VuiCheck,
    /// Threshold for steward appointment approval
    StewardApproval,
    /// Threshold for emergency governance actions
    Emergency,
}

/// Operations on thresholds
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThresholdOp {
    /// Set to exact value
    SetTo,
    /// Increase by amount
    IncreaseBy,
    /// Decrease by amount
    DecreaseBy,
}

/// Institutional authority that can issue attestations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstitutionalAuthority {
    /// DID of the institution
    pub did: Did,
    /// Human-readable name
    pub name: String,
    /// Types of attestations this authority can issue
    pub attestation_types: Vec<AttestationType>,
    /// Jurisdiction (country/region code)
    pub jurisdiction: String,
    /// Validity period in seconds
    pub validity_period: u64,
    /// Contact information
    pub contact: String,
    /// Optional verification URL
    pub verification_url: Option<String>,
}

/// Types of attestations an authority can issue
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttestationType {
    /// Age verification
    Age,
    /// Citizenship/nationality
    Citizenship,
    /// Residency status
    Residency,
    /// Professional license
    ProfessionalLicense { category: String },
    /// Educational credential
    Education { level: String },
    /// Employment verification
    Employment,
    /// Government ID verification
    GovernmentId,
    /// Custom attestation type
    Custom { type_id: String },
}

/// Target of a revocation (for appeals)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RevocationTarget {
    /// An anchor was revoked
    Anchor {
        /// The anchor ID
        anchor_id: [u8; 32],
    },
    /// A key bundle was revoked
    KeyBundle {
        /// The anchor ID
        anchor_id: [u8; 32],
        /// The key bundle version
        version: u32,
    },
    /// An attestation was revoked
    Attestation {
        /// Hash of the attestation
        attestation_hash: [u8; 32],
    },
    /// A steward's status was revoked
    StewardStatus {
        /// DID of the steward
        steward_did: Did,
    },
}

/// Jurisdiction tiers for stewards
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JurisdictionTier {
    /// Tier 1: Standard operations
    Tier1,
    /// Tier 2: Enhanced monitoring required
    Tier2,
    /// Tier 3: Requires co-signing for all operations
    Tier3,
}

impl JurisdictionTier {
    /// Check if this tier requires co-signing
    pub fn requires_cosigning(&self) -> bool {
        matches!(self, JurisdictionTier::Tier3)
    }

    /// Check if this tier requires enhanced monitoring
    pub fn requires_monitoring(&self) -> bool {
        matches!(self, JurisdictionTier::Tier2 | JurisdictionTier::Tier3)
    }
}

// ============================================================================
// Steward Statistics
// ============================================================================

/// Performance statistics for a steward
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StewardStats {
    /// Total POPs issued
    pub pops_issued: u64,
    /// Total recoveries participated in
    pub recoveries_participated: u64,
    /// Fraud investigations initiated
    pub fraud_investigations: u64,
    /// Proven fraud cases
    pub proven_fraud: u64,
    /// Appeals filed against this steward
    pub appeals_filed: u64,
    /// Appeals upheld against this steward
    pub appeals_upheld: u64,
    /// Average ceremony completion time (seconds)
    pub avg_ceremony_time: u64,
    /// Uptime percentage (0-100)
    pub uptime_percentage: u8,
}

impl StewardStats {
    /// Calculate a simple performance score (0-100)
    pub fn performance_score(&self) -> u8 {
        let mut score: u64 = 50; // Base score

        // Positive factors
        if self.pops_issued > 100 {
            score += 10;
        }
        if self.recoveries_participated > 10 {
            score += 10;
        }
        if self.uptime_percentage > 95 {
            score += 15;
        }

        // Negative factors
        if self.appeals_upheld > 0 {
            score = score.saturating_sub(self.appeals_upheld * 5);
        }
        if self.avg_ceremony_time > 3600 {
            score = score.saturating_sub(5);
        }

        score.min(100) as u8
    }
}

// ============================================================================
// Voting Requirements
// ============================================================================

/// Voting requirements for different SDIS proposal types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdisVotingRequirements {
    /// Quorum percentage (0-100)
    pub quorum_percentage: u8,
    /// Approval percentage needed (0-100)
    pub approval_percentage: u8,
    /// Minimum voting period in seconds
    pub min_voting_period: u64,
    /// Required delay before execution (seconds)
    pub execution_delay: u64,
}

impl SdisProposal {
    /// Get voting requirements for this proposal type
    pub fn voting_requirements(&self) -> SdisVotingRequirements {
        match self {
            // Steward removal requires super-majority
            SdisProposal::RemoveSteward { .. } => SdisVotingRequirements {
                quorum_percentage: 40,
                approval_percentage: 67,
                min_voting_period: 7 * 24 * 3600, // 7 days
                execution_delay: 24 * 3600,       // 24 hours
            },

            // Sanctions depend on severity
            SdisProposal::SanctionSteward { penalty, .. } => match penalty {
                StewardPenalty::Removal { .. } => SdisVotingRequirements {
                    quorum_percentage: 40,
                    approval_percentage: 67,
                    min_voting_period: 7 * 24 * 3600,
                    execution_delay: 24 * 3600,
                },
                StewardPenalty::BondSlash { .. } | StewardPenalty::TierDemotion { .. } => {
                    SdisVotingRequirements {
                        quorum_percentage: 30,
                        approval_percentage: 60,
                        min_voting_period: 5 * 24 * 3600,
                        execution_delay: 12 * 3600,
                    }
                }
                _ => SdisVotingRequirements {
                    quorum_percentage: 25,
                    approval_percentage: 51,
                    min_voting_period: 3 * 24 * 3600,
                    execution_delay: 6 * 3600,
                },
            },

            // Threshold changes require super-majority
            SdisProposal::ModifyThreshold { .. } => SdisVotingRequirements {
                quorum_percentage: 50,
                approval_percentage: 75,
                min_voting_period: 14 * 24 * 3600, // 14 days
                execution_delay: 48 * 3600,        // 48 hours
            },

            // Authority changes require careful consideration
            SdisProposal::ApproveAuthority { .. } | SdisProposal::RevokeAuthority { .. } => {
                SdisVotingRequirements {
                    quorum_percentage: 35,
                    approval_percentage: 60,
                    min_voting_period: 7 * 24 * 3600,
                    execution_delay: 24 * 3600,
                }
            }

            // Appeals get expedited handling
            SdisProposal::RevocationAppeal { .. } => SdisVotingRequirements {
                quorum_percentage: 25,
                approval_percentage: 51,
                min_voting_period: 3 * 24 * 3600,
                execution_delay: 0, // Immediate if approved
            },

            // Standard proposals
            _ => SdisVotingRequirements {
                quorum_percentage: 25,
                approval_percentage: 51,
                min_voting_period: 5 * 24 * 3600,
                execution_delay: 12 * 3600,
            },
        }
    }

    /// Get a human-readable description of the proposal
    pub fn description(&self) -> String {
        match self {
            SdisProposal::AppointSteward {
                candidate, region, ..
            } => {
                format!("Appoint {candidate} as steward for {region}")
            }
            SdisProposal::RemoveSteward {
                steward, reason, ..
            } => {
                format!("Remove steward {steward}: {reason}")
            }
            SdisProposal::SanctionSteward {
                steward, penalty, ..
            } => {
                format!("Sanction steward {steward} with {penalty:?}")
            }
            SdisProposal::ReconfirmSteward { steward, .. } => {
                format!("Reconfirm steward {steward} for new term")
            }
            SdisProposal::ModifyThreshold {
                threshold_type,
                new_value,
                ..
            } => {
                format!("Modify {threshold_type:?} threshold to {new_value}")
            }
            SdisProposal::ApproveAuthority { authority } => {
                format!("Approve {} as attestation authority", authority.name)
            }
            SdisProposal::RevokeAuthority { authority_did, .. } => {
                format!("Revoke authority {authority_did}")
            }
            SdisProposal::RevocationAppeal { target, .. } => {
                format!("Appeal revocation of {target:?}")
            }
            SdisProposal::SuspendSteward { steward, .. } => {
                format!("Suspend steward {steward}")
            }
            SdisProposal::ReinstateSteward { steward, .. } => {
                format!("Reinstate steward {steward}")
            }
            SdisProposal::UpdateJurisdictionTier {
                steward, new_tier, ..
            } => {
                format!("Update {steward} to {new_tier:?}")
            }
            SdisProposal::ForceKeyRotation { steward, .. } => {
                format!("Force key rotation for steward {steward}")
            }
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
    fn test_steward_penalty_variants() {
        let warning = StewardPenalty::Warning;
        let suspension = StewardPenalty::Suspension { duration: 86400 };
        let slash = StewardPenalty::BondSlash { amount: 1000 };
        let demotion = StewardPenalty::TierDemotion {
            new_tier: JurisdictionTier::Tier2,
        };
        let removal = StewardPenalty::Removal { return_bond: false };

        // Just verify they can be created and compared
        assert_ne!(warning, suspension);
        assert_ne!(slash, demotion);
        assert_ne!(warning, removal);
    }

    #[test]
    fn test_jurisdiction_tier() {
        assert!(!JurisdictionTier::Tier1.requires_cosigning());
        assert!(!JurisdictionTier::Tier2.requires_cosigning());
        assert!(JurisdictionTier::Tier3.requires_cosigning());

        assert!(!JurisdictionTier::Tier1.requires_monitoring());
        assert!(JurisdictionTier::Tier2.requires_monitoring());
        assert!(JurisdictionTier::Tier3.requires_monitoring());
    }

    #[test]
    fn test_steward_stats_performance() {
        let mut stats = StewardStats::default();
        assert_eq!(stats.performance_score(), 50); // Base score

        stats.pops_issued = 150;
        stats.recoveries_participated = 15;
        stats.uptime_percentage = 99;
        assert!(stats.performance_score() > 50);

        stats.appeals_upheld = 5;
        assert!(stats.performance_score() < 100);
    }

    #[test]
    fn test_voting_requirements() {
        let did = test_did();

        // Removal requires super-majority
        let removal = SdisProposal::RemoveSteward {
            steward: did.clone(),
            reason: "test".to_string(),
            return_bond: true,
        };
        let reqs = removal.voting_requirements();
        assert_eq!(reqs.approval_percentage, 67);

        // Threshold changes require highest approval
        let threshold = SdisProposal::ModifyThreshold {
            threshold_type: ThresholdType::PopCeremony,
            operation: ThresholdOp::SetTo,
            new_value: 5,
        };
        let reqs = threshold.voting_requirements();
        assert_eq!(reqs.approval_percentage, 75);

        // Appeals are expedited
        let appeal = SdisProposal::RevocationAppeal {
            target: RevocationTarget::StewardStatus { steward_did: did },
            revoked_at: 0,
            reason: "test".to_string(),
            evidence: vec![],
        };
        let reqs = appeal.voting_requirements();
        assert_eq!(reqs.execution_delay, 0);
    }

    #[test]
    fn test_proposal_descriptions() {
        let did = test_did();

        let appoint = SdisProposal::AppointSteward {
            candidate: did.clone(),
            sponsors: vec![],
            region: "US-CA".to_string(),
            bond_amount: 10000,
            term_length: 365 * 24 * 3600,
        };
        assert!(appoint.description().contains("US-CA"));

        let remove = SdisProposal::RemoveSteward {
            steward: did.clone(),
            reason: "misconduct".to_string(),
            return_bond: false,
        };
        assert!(remove.description().contains("misconduct"));
    }

    #[test]
    fn test_institutional_authority() {
        let authority = InstitutionalAuthority {
            did: test_did(),
            name: "Test Authority".to_string(),
            attestation_types: vec![AttestationType::Age, AttestationType::Citizenship],
            jurisdiction: "US".to_string(),
            validity_period: 365 * 24 * 3600,
            contact: "test@example.com".to_string(),
            verification_url: Some("https://example.com/verify".to_string()),
        };

        assert_eq!(authority.attestation_types.len(), 2);
        assert_eq!(authority.jurisdiction, "US");
    }

    #[test]
    fn test_revocation_targets() {
        let anchor_target = RevocationTarget::Anchor {
            anchor_id: [0u8; 32],
        };

        let keybundle_target = RevocationTarget::KeyBundle {
            anchor_id: [1u8; 32],
            version: 2,
        };

        let attestation_target = RevocationTarget::Attestation {
            attestation_hash: [2u8; 32],
        };

        let steward_target = RevocationTarget::StewardStatus {
            steward_did: test_did(),
        };

        // Verify all variants serialize successfully
        assert!(
            serde_json::to_string(&anchor_target).is_ok(),
            "Anchor variant should serialize"
        );
        assert!(
            serde_json::to_string(&keybundle_target).is_ok(),
            "KeyBundle variant should serialize"
        );
        assert!(
            serde_json::to_string(&attestation_target).is_ok(),
            "Attestation variant should serialize"
        );
        assert!(
            serde_json::to_string(&steward_target).is_ok(),
            "StewardStatus variant should serialize"
        );
    }
}
