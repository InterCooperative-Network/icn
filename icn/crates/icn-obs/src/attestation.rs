//! Contribution attestation protocol (Phase 21.1)
//!
//! This module provides types for peer attestation of infrastructure contributions.
//! Contributions must be verified by trusted peers before credits are issued.
//!
//! # Gossip Topics
//!
//! - `contribution:claim` - Contribution claims submitted by nodes
//! - `contribution:attestation` - Peer attestations for claims
//! - `contribution:verified` - Verified claims that reached threshold
//!
//! # Sybil Resistance
//!
//! | Constraint | Requirement |
//! |------------|-------------|
//! | Membership age | ≥ 90 days to attest |
//! | Trust score | ≥ 0.3 minimum |
//! | Attestation budget | Max 10 per period per attester |
//! | Non-reciprocity | Can't attest someone who attested you |
//! | Org anchor | Claims > 500 credits require org attestation |

use serde::{Deserialize, Serialize};

// ============================================================================
// Gossip Topics
// ============================================================================

/// Topic for contribution claims
pub const TOPIC_CONTRIBUTION_CLAIM: &str = "contribution:claim";

/// Topic for peer attestations
pub const TOPIC_CONTRIBUTION_ATTESTATION: &str = "contribution:attestation";

/// Topic for verified claims
pub const TOPIC_CONTRIBUTION_VERIFIED: &str = "contribution:verified";

use crate::contribution::{ResourceType, Timestamp};

/// Content hash for evidence linking
pub type ContentHash = [u8; 32];

/// Minimum trust score required to attest
pub const MIN_TRUST_TO_ATTEST: f64 = 0.3;

/// Minimum membership age in seconds (90 days)
pub const MIN_MEMBERSHIP_AGE_SECS: u64 = 90 * 24 * 60 * 60;

/// Maximum attestations per period per attester
pub const MAX_ATTESTATIONS_PER_PERIOD: u32 = 10;

/// Threshold for requiring org attestation (in credit units)
pub const ORG_ATTESTATION_THRESHOLD: u64 = 500;

/// Minimum weighted attestation score for claim verification
pub const CONTRIBUTION_THRESHOLD: f64 = 1.0;

/// Status of a contribution claim
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ClaimStatus {
    /// Claim submitted, awaiting attestations
    Pending,
    /// Claim has sufficient attestations
    Verified,
    /// Claim rejected (insufficient attestations or fraud detected)
    Rejected,
    /// Claim expired without sufficient attestations
    Expired,
}

/// A contribution claim submitted by a node
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContributionClaim {
    /// Unique hash of this claim
    pub claim_hash: ContentHash,
    /// The DID of the contributing node
    pub contributor: String,
    /// Type of resource contributed
    pub resource_type: ResourceType,
    /// Amount contributed (in base units for the resource type)
    pub amount: u64,
    /// Time period of contribution (start, end)
    pub period: (Timestamp, Timestamp),
    /// Hash of evidence supporting the claim
    pub evidence: ContentHash,
    /// When the claim was submitted
    pub submitted_at: Timestamp,
    /// Current status
    pub status: ClaimStatus,
}

impl ContributionClaim {
    /// Create a new contribution claim
    pub fn new(
        contributor: impl Into<String>,
        resource_type: ResourceType,
        amount: u64,
        period: (Timestamp, Timestamp),
        evidence: ContentHash,
        submitted_at: Timestamp,
    ) -> Self {
        let contributor = contributor.into();

        // Generate claim hash from content
        let mut hasher = blake3::Hasher::new();
        hasher.update(contributor.as_bytes());
        hasher.update(&[resource_type as u8]);
        hasher.update(&amount.to_le_bytes());
        hasher.update(&period.0.to_le_bytes());
        hasher.update(&period.1.to_le_bytes());
        hasher.update(&evidence);
        hasher.update(&submitted_at.to_le_bytes());
        let claim_hash: ContentHash = hasher.finalize().into();

        Self {
            claim_hash,
            contributor,
            resource_type,
            amount,
            period,
            evidence,
            submitted_at,
            status: ClaimStatus::Pending,
        }
    }

    /// Check if this claim requires org-level attestation
    pub fn requires_org_attestation(&self) -> bool {
        self.amount > ORG_ATTESTATION_THRESHOLD
    }
}

/// A single attestation by a peer
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerAttestation {
    /// DID of the attester
    pub attester: String,
    /// Ed25519 signature over the claim hash
    pub signature: Vec<u8>,
    /// When the attestation was made
    pub attested_at: Timestamp,
}

/// A contribution claim with its attestations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributionAttestation {
    /// The underlying claim
    pub claim: ContributionClaim,
    /// List of peer attestations
    pub attestations: Vec<PeerAttestation>,
}

impl ContributionAttestation {
    /// Create a new attestation from a claim
    pub fn new(claim: ContributionClaim) -> Self {
        Self {
            claim,
            attestations: Vec::new(),
        }
    }

    /// Add an attestation from a peer
    pub fn add_attestation(&mut self, attestation: PeerAttestation) {
        // Don't add duplicate attestations from same attester
        if !self
            .attestations
            .iter()
            .any(|a| a.attester == attestation.attester)
        {
            self.attestations.push(attestation);
        }
    }

    /// Get the claim hash
    pub fn claim_hash(&self) -> &ContentHash {
        &self.claim.claim_hash
    }

    /// Get the contributor DID
    pub fn contributor(&self) -> &str {
        &self.claim.contributor
    }

    /// Get all attester DIDs
    pub fn attesters(&self) -> Vec<&str> {
        self.attestations
            .iter()
            .map(|a| a.attester.as_str())
            .collect()
    }
}

/// Eligibility status for an attester
#[derive(Debug, Clone)]
pub enum EligibilityStatus {
    /// Attester is eligible
    Eligible,
    /// Trust score too low
    InsufficientTrust(f64),
    /// Membership too recent
    InsufficientAge(u64),
    /// Attestation budget exhausted
    BudgetExhausted(u32),
    /// Reciprocal attestation not allowed
    ReciprocalNotAllowed,
    /// Attesting own claim not allowed
    SelfAttestationNotAllowed,
}

impl PartialEq for EligibilityStatus {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Eligible, Self::Eligible) => true,
            (Self::InsufficientTrust(_), Self::InsufficientTrust(_)) => true,
            (Self::InsufficientAge(a), Self::InsufficientAge(b)) => a == b,
            (Self::BudgetExhausted(a), Self::BudgetExhausted(b)) => a == b,
            (Self::ReciprocalNotAllowed, Self::ReciprocalNotAllowed) => true,
            (Self::SelfAttestationNotAllowed, Self::SelfAttestationNotAllowed) => true,
            _ => false,
        }
    }
}

impl Eq for EligibilityStatus {}

impl EligibilityStatus {
    /// Check if eligible
    pub fn is_eligible(&self) -> bool {
        matches!(self, EligibilityStatus::Eligible)
    }
}

/// Context for eligibility checking
pub struct EligibilityContext {
    /// Attester's trust score
    pub trust_score: f64,
    /// Attester's membership age in seconds
    pub membership_age_secs: u64,
    /// Attestations made by this attester in current period
    pub attestations_this_period: u32,
    /// DIDs that have attested for the attester (for reciprocity check)
    pub attesters_of_attester: Vec<String>,
}

/// Check if an attester is eligible to attest a claim
pub fn check_eligibility(
    attester_did: &str,
    claim: &ContributionClaim,
    context: &EligibilityContext,
) -> EligibilityStatus {
    // Can't attest own claim
    if attester_did == claim.contributor {
        return EligibilityStatus::SelfAttestationNotAllowed;
    }

    // Check trust score
    if context.trust_score < MIN_TRUST_TO_ATTEST {
        return EligibilityStatus::InsufficientTrust(context.trust_score);
    }

    // Check membership age
    if context.membership_age_secs < MIN_MEMBERSHIP_AGE_SECS {
        return EligibilityStatus::InsufficientAge(context.membership_age_secs);
    }

    // Check attestation budget
    if context.attestations_this_period >= MAX_ATTESTATIONS_PER_PERIOD {
        return EligibilityStatus::BudgetExhausted(context.attestations_this_period);
    }

    // Check reciprocity - can't attest someone who attested you
    if context
        .attesters_of_attester
        .iter()
        .any(|d| d == &claim.contributor)
    {
        return EligibilityStatus::ReciprocalNotAllowed;
    }

    EligibilityStatus::Eligible
}

/// Validation result for a contribution attestation
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether the claim is verified
    pub verified: bool,
    /// Sum of weighted attestations
    pub weighted_score: f64,
    /// Number of eligible attesters
    pub eligible_count: usize,
    /// Number of ineligible attesters (with reasons)
    pub ineligible: Vec<(String, EligibilityStatus)>,
    /// Fraud indicators detected
    pub fraud_indicators: Vec<FraudIndicator>,
}

/// Callback type for looking up trust scores
pub type TrustLookup = Box<dyn Fn(&str) -> Option<f64> + Send + Sync>;

/// Callback type for looking up membership age
pub type MembershipAgeLookup = Box<dyn Fn(&str) -> Option<u64> + Send + Sync>;

/// Callback type for looking up attestation count in current period
pub type AttestationCountLookup = Box<dyn Fn(&str) -> u32 + Send + Sync>;

/// Callback type for looking up who has attested for a DID
pub type AttestersOfLookup = Box<dyn Fn(&str) -> Vec<String> + Send + Sync>;

/// Validator for contribution attestations with trust-weighted scoring
pub struct ContributionValidator {
    /// Lookup function for trust scores
    trust_lookup: TrustLookup,
    /// Lookup function for membership age
    membership_age_lookup: MembershipAgeLookup,
    /// Lookup function for attestation counts
    attestation_count_lookup: AttestationCountLookup,
    /// Lookup function for who has attested for a DID
    attesters_of_lookup: AttestersOfLookup,
}

impl ContributionValidator {
    /// Create a new validator with the provided lookup functions
    pub fn new(
        trust_lookup: TrustLookup,
        membership_age_lookup: MembershipAgeLookup,
        attestation_count_lookup: AttestationCountLookup,
        attesters_of_lookup: AttestersOfLookup,
    ) -> Self {
        Self {
            trust_lookup,
            membership_age_lookup,
            attestation_count_lookup,
            attesters_of_lookup,
        }
    }

    /// Build eligibility context for an attester
    fn build_context(&self, attester_did: &str) -> Option<EligibilityContext> {
        let trust_score = (self.trust_lookup)(attester_did)?;
        let membership_age_secs = (self.membership_age_lookup)(attester_did)?;
        let attestations_this_period = (self.attestation_count_lookup)(attester_did);
        let attesters_of_attester = (self.attesters_of_lookup)(attester_did);

        Some(EligibilityContext {
            trust_score,
            membership_age_secs,
            attestations_this_period,
            attesters_of_attester,
        })
    }

    /// Validate a contribution attestation using trust-weighted scoring
    ///
    /// The weighted score is the sum of trust scores for all eligible attesters.
    /// A claim is verified if the weighted score exceeds `CONTRIBUTION_THRESHOLD`.
    pub fn validate(&self, attestation: &ContributionAttestation) -> ValidationResult {
        let mut weighted_score = 0.0;
        let mut eligible_count = 0;
        let mut ineligible = Vec::new();
        let fraud_indicators = Vec::new(); // Fraud detection is a stub for now

        for peer_att in &attestation.attestations {
            let attester_did = &peer_att.attester;

            // Try to get eligibility context
            let Some(context) = self.build_context(attester_did) else {
                ineligible.push((
                    attester_did.clone(),
                    EligibilityStatus::InsufficientTrust(0.0), // Unknown DID
                ));
                continue;
            };

            // Check eligibility
            let status = check_eligibility(attester_did, &attestation.claim, &context);

            if status.is_eligible() {
                // Add trust score to weighted sum
                weighted_score += context.trust_score;
                eligible_count += 1;
            } else {
                ineligible.push((attester_did.clone(), status));
            }
        }

        let verified = weighted_score >= CONTRIBUTION_THRESHOLD;

        ValidationResult {
            verified,
            weighted_score,
            eligible_count,
            ineligible,
            fraud_indicators,
        }
    }

    /// Check if a specific attester is eligible to attest a claim
    pub fn check_attester_eligibility(
        &self,
        attester_did: &str,
        claim: &ContributionClaim,
    ) -> EligibilityStatus {
        let Some(context) = self.build_context(attester_did) else {
            return EligibilityStatus::InsufficientTrust(0.0);
        };
        check_eligibility(attester_did, claim, &context)
    }
}

/// Fraud indicator for suspicious activity
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FraudIndicator {
    /// Unusually high claim amount
    UnusuallyHighAmount { amount: u64, average: u64 },
    /// Rapid successive claims
    RapidClaims { count: u32, window_secs: u64 },
    /// Attestation ring detected (circular attestations)
    AttestationRing { participants: Vec<String> },
    /// Claim conflicts with network observations
    ConflictsWithObservations { reason: String },
}

// ============================================================================
// Fraud Detection (Stubs)
// ============================================================================

/// Callback type for looking up claim history for a DID
pub type ClaimHistoryLookup = Box<dyn Fn(&str) -> Vec<ContributionClaim> + Send + Sync>;

/// Callback type for looking up attestation graph (who attested whom)
pub type AttestationGraphLookup = Box<dyn Fn(&str) -> Vec<(String, String)> + Send + Sync>;

/// Callback type for looking up network observations (e.g., actual compute metrics)
pub type NetworkObservationsLookup = Box<dyn Fn(&str, ResourceType) -> Option<u64> + Send + Sync>;

/// Fraud detector for analyzing contribution claims (stub implementation)
///
/// This is a placeholder implementation that provides the API for fraud detection.
/// The actual detection algorithms will be implemented in a future phase.
pub struct FraudDetector {
    /// Lookup for claim history
    claim_history: ClaimHistoryLookup,
    /// Lookup for attestation graph (used in future ring detection)
    #[allow(dead_code)]
    attestation_graph: AttestationGraphLookup,
    /// Lookup for network observations
    network_observations: NetworkObservationsLookup,
    /// Threshold multiplier for unusually high amounts (default: 3.0)
    high_amount_threshold: f64,
    /// Window in seconds for detecting rapid claims (default: 1 hour)
    rapid_claim_window_secs: u64,
    /// Maximum claims per window (default: 5)
    max_claims_per_window: u32,
}

impl FraudDetector {
    /// Create a new fraud detector with the provided lookup functions
    pub fn new(
        claim_history: ClaimHistoryLookup,
        attestation_graph: AttestationGraphLookup,
        network_observations: NetworkObservationsLookup,
    ) -> Self {
        Self {
            claim_history,
            attestation_graph,
            network_observations,
            high_amount_threshold: 3.0,
            rapid_claim_window_secs: 3600, // 1 hour
            max_claims_per_window: 5,
        }
    }

    /// Set the threshold for detecting unusually high amounts
    pub fn with_high_amount_threshold(mut self, threshold: f64) -> Self {
        self.high_amount_threshold = threshold;
        self
    }

    /// Set the window for detecting rapid claims
    pub fn with_rapid_claim_window(mut self, window_secs: u64, max_claims: u32) -> Self {
        self.rapid_claim_window_secs = window_secs;
        self.max_claims_per_window = max_claims;
        self
    }

    /// Analyze a claim for fraud indicators
    ///
    /// Returns a list of detected fraud indicators. An empty list means no fraud detected.
    pub fn analyze(&self, claim: &ContributionClaim) -> Vec<FraudIndicator> {
        let mut indicators = Vec::new();

        // Check for unusually high amount
        if let Some(indicator) = self.check_unusually_high_amount(claim) {
            indicators.push(indicator);
        }

        // Check for rapid successive claims
        if let Some(indicator) = self.check_rapid_claims(claim) {
            indicators.push(indicator);
        }

        // Check for conflicts with network observations
        if let Some(indicator) = self.check_conflicts_with_observations(claim) {
            indicators.push(indicator);
        }

        indicators
    }

    /// Check if claim amount is unusually high compared to history
    ///
    /// STUB: Returns None (no fraud detected) - actual implementation pending
    fn check_unusually_high_amount(&self, claim: &ContributionClaim) -> Option<FraudIndicator> {
        let history = (self.claim_history)(&claim.contributor);

        if history.is_empty() {
            return None; // No history to compare against
        }

        // Calculate average of same resource type
        let same_type: Vec<_> = history
            .iter()
            .filter(|c| c.resource_type == claim.resource_type)
            .collect();

        if same_type.is_empty() {
            return None;
        }

        let total: u64 = same_type.iter().map(|c| c.amount).sum();
        let average = total / same_type.len() as u64;

        if average > 0 && claim.amount > (average as f64 * self.high_amount_threshold) as u64 {
            return Some(FraudIndicator::UnusuallyHighAmount {
                amount: claim.amount,
                average,
            });
        }

        None
    }

    /// Check for rapid successive claims
    ///
    /// STUB: Returns None (no fraud detected) - actual implementation pending
    fn check_rapid_claims(&self, claim: &ContributionClaim) -> Option<FraudIndicator> {
        let history = (self.claim_history)(&claim.contributor);

        let window_start = claim
            .submitted_at
            .saturating_sub(self.rapid_claim_window_secs);
        let recent_count = history
            .iter()
            .filter(|c| c.submitted_at >= window_start && c.submitted_at < claim.submitted_at)
            .count() as u32;

        if recent_count >= self.max_claims_per_window {
            return Some(FraudIndicator::RapidClaims {
                count: recent_count + 1, // +1 for current claim
                window_secs: self.rapid_claim_window_secs,
            });
        }

        None
    }

    /// Check if claim conflicts with network observations
    ///
    /// STUB: Returns None (no fraud detected) - actual implementation pending
    fn check_conflicts_with_observations(
        &self,
        claim: &ContributionClaim,
    ) -> Option<FraudIndicator> {
        let observed = (self.network_observations)(&claim.contributor, claim.resource_type);

        if let Some(observed_amount) = observed {
            // If claimed amount is more than 2x observed, flag it
            if claim.amount > observed_amount * 2 {
                return Some(FraudIndicator::ConflictsWithObservations {
                    reason: format!(
                        "Claimed {} but network observed only {}",
                        claim.amount, observed_amount
                    ),
                });
            }
        }

        None
    }

    /// Detect attestation rings (circular attestation patterns)
    ///
    /// STUB: Returns None (no ring detected) - actual implementation pending
    /// This would require graph analysis to detect cycles in the attestation graph.
    pub fn detect_attestation_ring(
        &self,
        _claim: &ContributionAttestation,
    ) -> Option<FraudIndicator> {
        // TODO: Implement cycle detection in attestation graph
        // For now, this is a stub that returns None
        None
    }
}

/// Gossip message types for contribution attestation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContributionMessage {
    /// A new contribution claim was submitted
    ClaimSubmitted(ContributionClaim),
    /// An attestation was added to a claim
    AttestationAdded {
        claim_hash: ContentHash,
        attestation: PeerAttestation,
    },
    /// A claim was verified (reached threshold)
    ClaimVerified {
        claim_hash: ContentHash,
        weighted_score: f64,
    },
    /// A claim was rejected
    ClaimRejected {
        claim_hash: ContentHash,
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_DID_CONTRIBUTOR: &str = "did:icn:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
    const TEST_DID_ATTESTER: &str = "did:icn:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH";
    const TEST_DID_ATTESTER_2: &str = "did:icn:z6MknGc3ocHs3zdPiJbnaaqDi58NGb4pk1Sp9WNhZ8bVxnCC";

    fn test_evidence() -> ContentHash {
        [0u8; 32]
    }

    #[test]
    fn test_contribution_claim_creation() {
        let claim = ContributionClaim::new(
            TEST_DID_CONTRIBUTOR,
            ResourceType::Compute,
            100,
            (1000, 2000),
            test_evidence(),
            1000,
        );

        assert_eq!(claim.contributor, TEST_DID_CONTRIBUTOR);
        assert_eq!(claim.resource_type, ResourceType::Compute);
        assert_eq!(claim.amount, 100);
        assert_eq!(claim.status, ClaimStatus::Pending);
        assert!(!claim.requires_org_attestation());
    }

    #[test]
    fn test_large_claim_requires_org_attestation() {
        let claim = ContributionClaim::new(
            TEST_DID_CONTRIBUTOR,
            ResourceType::Compute,
            1000, // > 500 threshold
            (1000, 2000),
            test_evidence(),
            1000,
        );

        assert!(claim.requires_org_attestation());
    }

    #[test]
    fn test_add_attestation() {
        let claim = ContributionClaim::new(
            TEST_DID_CONTRIBUTOR,
            ResourceType::Compute,
            100,
            (1000, 2000),
            test_evidence(),
            1000,
        );

        let mut attestation = ContributionAttestation::new(claim);

        let peer_att = PeerAttestation {
            attester: TEST_DID_ATTESTER.to_string(),
            signature: vec![1, 2, 3],
            attested_at: 1001,
        };

        attestation.add_attestation(peer_att);
        assert_eq!(attestation.attestations.len(), 1);

        // Duplicate attestation should be ignored
        let peer_att_dup = PeerAttestation {
            attester: TEST_DID_ATTESTER.to_string(),
            signature: vec![4, 5, 6],
            attested_at: 1002,
        };
        attestation.add_attestation(peer_att_dup);
        assert_eq!(attestation.attestations.len(), 1);

        // Different attester should be added
        let peer_att_2 = PeerAttestation {
            attester: TEST_DID_ATTESTER_2.to_string(),
            signature: vec![7, 8, 9],
            attested_at: 1003,
        };
        attestation.add_attestation(peer_att_2);
        assert_eq!(attestation.attestations.len(), 2);
    }

    #[test]
    fn test_eligibility_self_attestation() {
        let claim = ContributionClaim::new(
            TEST_DID_CONTRIBUTOR,
            ResourceType::Compute,
            100,
            (1000, 2000),
            test_evidence(),
            1000,
        );

        let context = EligibilityContext {
            trust_score: 0.5,
            membership_age_secs: MIN_MEMBERSHIP_AGE_SECS + 1,
            attestations_this_period: 0,
            attesters_of_attester: vec![],
        };

        let result = check_eligibility(TEST_DID_CONTRIBUTOR, &claim, &context);
        assert_eq!(result, EligibilityStatus::SelfAttestationNotAllowed);
    }

    #[test]
    fn test_eligibility_insufficient_trust() {
        let claim = ContributionClaim::new(
            TEST_DID_CONTRIBUTOR,
            ResourceType::Compute,
            100,
            (1000, 2000),
            test_evidence(),
            1000,
        );

        let context = EligibilityContext {
            trust_score: 0.2, // Below threshold
            membership_age_secs: MIN_MEMBERSHIP_AGE_SECS + 1,
            attestations_this_period: 0,
            attesters_of_attester: vec![],
        };

        let result = check_eligibility(TEST_DID_ATTESTER, &claim, &context);
        assert!(matches!(result, EligibilityStatus::InsufficientTrust(_)));
    }

    #[test]
    fn test_eligibility_insufficient_age() {
        let claim = ContributionClaim::new(
            TEST_DID_CONTRIBUTOR,
            ResourceType::Compute,
            100,
            (1000, 2000),
            test_evidence(),
            1000,
        );

        let context = EligibilityContext {
            trust_score: 0.5,
            membership_age_secs: 30 * 24 * 60 * 60, // Only 30 days
            attestations_this_period: 0,
            attesters_of_attester: vec![],
        };

        let result = check_eligibility(TEST_DID_ATTESTER, &claim, &context);
        assert!(matches!(result, EligibilityStatus::InsufficientAge(_)));
    }

    #[test]
    fn test_eligibility_budget_exhausted() {
        let claim = ContributionClaim::new(
            TEST_DID_CONTRIBUTOR,
            ResourceType::Compute,
            100,
            (1000, 2000),
            test_evidence(),
            1000,
        );

        let context = EligibilityContext {
            trust_score: 0.5,
            membership_age_secs: MIN_MEMBERSHIP_AGE_SECS + 1,
            attestations_this_period: MAX_ATTESTATIONS_PER_PERIOD,
            attesters_of_attester: vec![],
        };

        let result = check_eligibility(TEST_DID_ATTESTER, &claim, &context);
        assert!(matches!(result, EligibilityStatus::BudgetExhausted(_)));
    }

    #[test]
    fn test_eligibility_reciprocal_not_allowed() {
        let claim = ContributionClaim::new(
            TEST_DID_CONTRIBUTOR,
            ResourceType::Compute,
            100,
            (1000, 2000),
            test_evidence(),
            1000,
        );

        let context = EligibilityContext {
            trust_score: 0.5,
            membership_age_secs: MIN_MEMBERSHIP_AGE_SECS + 1,
            attestations_this_period: 0,
            // Contributor has attested for the attester before
            attesters_of_attester: vec![TEST_DID_CONTRIBUTOR.to_string()],
        };

        let result = check_eligibility(TEST_DID_ATTESTER, &claim, &context);
        assert_eq!(result, EligibilityStatus::ReciprocalNotAllowed);
    }

    #[test]
    fn test_eligibility_eligible() {
        let claim = ContributionClaim::new(
            TEST_DID_CONTRIBUTOR,
            ResourceType::Compute,
            100,
            (1000, 2000),
            test_evidence(),
            1000,
        );

        let context = EligibilityContext {
            trust_score: 0.5,
            membership_age_secs: MIN_MEMBERSHIP_AGE_SECS + 1,
            attestations_this_period: 5,
            attesters_of_attester: vec![TEST_DID_ATTESTER_2.to_string()], // Different DID
        };

        let result = check_eligibility(TEST_DID_ATTESTER, &claim, &context);
        assert_eq!(result, EligibilityStatus::Eligible);
    }

    #[test]
    fn test_contribution_message_serialization() {
        let claim = ContributionClaim::new(
            TEST_DID_CONTRIBUTOR,
            ResourceType::Compute,
            100,
            (1000, 2000),
            test_evidence(),
            1000,
        );

        let msg = ContributionMessage::ClaimSubmitted(claim);
        let serialized = serde_json::to_string(&msg).unwrap();
        let deserialized: ContributionMessage = serde_json::from_str(&serialized).unwrap();

        match deserialized {
            ContributionMessage::ClaimSubmitted(c) => {
                assert_eq!(c.contributor, TEST_DID_CONTRIBUTOR);
                assert_eq!(c.amount, 100);
            }
            _ => panic!("Wrong message type"),
        }
    }

    // Helper to create a test validator with configurable trust scores
    fn create_test_validator(
        trust_scores: std::collections::HashMap<String, f64>,
    ) -> ContributionValidator {
        let trust_scores_clone = trust_scores.clone();
        ContributionValidator::new(
            Box::new(move |did| trust_scores_clone.get(did).copied()),
            Box::new(|_| Some(MIN_MEMBERSHIP_AGE_SECS + 1)), // Always old enough
            Box::new(|_| 0),                                 // No attestations yet
            Box::new(|_| vec![]),                            // No attesters
        )
    }

    #[test]
    fn test_validator_no_attestations() {
        let validator = create_test_validator(std::collections::HashMap::new());

        let claim = ContributionClaim::new(
            TEST_DID_CONTRIBUTOR,
            ResourceType::Compute,
            100,
            (1000, 2000),
            test_evidence(),
            1000,
        );

        let attestation = ContributionAttestation::new(claim);
        let result = validator.validate(&attestation);

        assert!(!result.verified);
        assert_eq!(result.weighted_score, 0.0);
        assert_eq!(result.eligible_count, 0);
    }

    #[test]
    fn test_validator_single_attestation_below_threshold() {
        let mut trust_scores = std::collections::HashMap::new();
        trust_scores.insert(TEST_DID_ATTESTER.to_string(), 0.5); // Below threshold of 1.0

        let validator = create_test_validator(trust_scores);

        let claim = ContributionClaim::new(
            TEST_DID_CONTRIBUTOR,
            ResourceType::Compute,
            100,
            (1000, 2000),
            test_evidence(),
            1000,
        );

        let mut attestation = ContributionAttestation::new(claim);
        attestation.add_attestation(PeerAttestation {
            attester: TEST_DID_ATTESTER.to_string(),
            signature: vec![1, 2, 3],
            attested_at: 1001,
        });

        let result = validator.validate(&attestation);

        assert!(!result.verified); // 0.5 < 1.0
        assert!((result.weighted_score - 0.5).abs() < 0.001);
        assert_eq!(result.eligible_count, 1);
    }

    #[test]
    fn test_validator_multiple_attestations_reach_threshold() {
        let mut trust_scores = std::collections::HashMap::new();
        trust_scores.insert(TEST_DID_ATTESTER.to_string(), 0.5);
        trust_scores.insert(TEST_DID_ATTESTER_2.to_string(), 0.6);

        let validator = create_test_validator(trust_scores);

        let claim = ContributionClaim::new(
            TEST_DID_CONTRIBUTOR,
            ResourceType::Compute,
            100,
            (1000, 2000),
            test_evidence(),
            1000,
        );

        let mut attestation = ContributionAttestation::new(claim);
        attestation.add_attestation(PeerAttestation {
            attester: TEST_DID_ATTESTER.to_string(),
            signature: vec![1, 2, 3],
            attested_at: 1001,
        });
        attestation.add_attestation(PeerAttestation {
            attester: TEST_DID_ATTESTER_2.to_string(),
            signature: vec![4, 5, 6],
            attested_at: 1002,
        });

        let result = validator.validate(&attestation);

        assert!(result.verified); // 0.5 + 0.6 = 1.1 >= 1.0
        assert!((result.weighted_score - 1.1).abs() < 0.001);
        assert_eq!(result.eligible_count, 2);
    }

    #[test]
    fn test_validator_excludes_ineligible_attesters() {
        let mut trust_scores = std::collections::HashMap::new();
        trust_scores.insert(TEST_DID_ATTESTER.to_string(), 0.2); // Below MIN_TRUST_TO_ATTEST
        trust_scores.insert(TEST_DID_ATTESTER_2.to_string(), 0.6);

        let validator = create_test_validator(trust_scores);

        let claim = ContributionClaim::new(
            TEST_DID_CONTRIBUTOR,
            ResourceType::Compute,
            100,
            (1000, 2000),
            test_evidence(),
            1000,
        );

        let mut attestation = ContributionAttestation::new(claim);
        attestation.add_attestation(PeerAttestation {
            attester: TEST_DID_ATTESTER.to_string(),
            signature: vec![1, 2, 3],
            attested_at: 1001,
        });
        attestation.add_attestation(PeerAttestation {
            attester: TEST_DID_ATTESTER_2.to_string(),
            signature: vec![4, 5, 6],
            attested_at: 1002,
        });

        let result = validator.validate(&attestation);

        assert!(!result.verified); // Only 0.6 from ATTESTER_2
        assert!((result.weighted_score - 0.6).abs() < 0.001);
        assert_eq!(result.eligible_count, 1);
        assert_eq!(result.ineligible.len(), 1);
        assert!(matches!(
            result.ineligible[0].1,
            EligibilityStatus::InsufficientTrust(_)
        ));
    }

    #[test]
    fn test_validator_unknown_attester() {
        let validator = create_test_validator(std::collections::HashMap::new());

        let claim = ContributionClaim::new(
            TEST_DID_CONTRIBUTOR,
            ResourceType::Compute,
            100,
            (1000, 2000),
            test_evidence(),
            1000,
        );

        let mut attestation = ContributionAttestation::new(claim);
        attestation.add_attestation(PeerAttestation {
            attester: TEST_DID_ATTESTER.to_string(),
            signature: vec![1, 2, 3],
            attested_at: 1001,
        });

        let result = validator.validate(&attestation);

        assert!(!result.verified);
        assert_eq!(result.eligible_count, 0);
        assert_eq!(result.ineligible.len(), 1);
    }

    // ========================================================================
    // Fraud Detector Tests
    // ========================================================================

    fn create_test_fraud_detector(
        claim_history: Vec<ContributionClaim>,
        observations: std::collections::HashMap<String, u64>,
    ) -> FraudDetector {
        let claim_history = std::sync::Arc::new(claim_history);
        let observations = std::sync::Arc::new(observations);

        FraudDetector::new(
            Box::new(move |_did| claim_history.clone().to_vec()),
            Box::new(|_| vec![]), // Empty attestation graph
            Box::new(move |did, _resource_type| observations.get(did).copied()),
        )
    }

    #[test]
    fn test_fraud_detector_no_fraud() {
        let detector = create_test_fraud_detector(vec![], std::collections::HashMap::new());

        let claim = ContributionClaim::new(
            TEST_DID_CONTRIBUTOR,
            ResourceType::Compute,
            100,
            (1000, 2000),
            test_evidence(),
            1000,
        );

        let indicators = detector.analyze(&claim);
        assert!(indicators.is_empty());
    }

    #[test]
    fn test_fraud_detector_unusually_high_amount() {
        // History with average of 100
        let history = vec![
            ContributionClaim::new(
                TEST_DID_CONTRIBUTOR,
                ResourceType::Compute,
                80,
                (0, 100),
                test_evidence(),
                100,
            ),
            ContributionClaim::new(
                TEST_DID_CONTRIBUTOR,
                ResourceType::Compute,
                100,
                (100, 200),
                test_evidence(),
                200,
            ),
            ContributionClaim::new(
                TEST_DID_CONTRIBUTOR,
                ResourceType::Compute,
                120,
                (200, 300),
                test_evidence(),
                300,
            ),
        ];

        let detector = create_test_fraud_detector(history, std::collections::HashMap::new())
            .with_high_amount_threshold(3.0);

        // Claim 4x the average (400 vs avg of 100)
        let claim = ContributionClaim::new(
            TEST_DID_CONTRIBUTOR,
            ResourceType::Compute,
            400,
            (1000, 2000),
            test_evidence(),
            1000,
        );

        let indicators = detector.analyze(&claim);
        assert_eq!(indicators.len(), 1);
        assert!(matches!(
            indicators[0],
            FraudIndicator::UnusuallyHighAmount { .. }
        ));
    }

    #[test]
    fn test_fraud_detector_rapid_claims() {
        // 5 claims in the last hour
        let history = vec![
            ContributionClaim::new(
                TEST_DID_CONTRIBUTOR,
                ResourceType::Compute,
                100,
                (0, 100),
                test_evidence(),
                500,
            ),
            ContributionClaim::new(
                TEST_DID_CONTRIBUTOR,
                ResourceType::Compute,
                100,
                (0, 100),
                test_evidence(),
                600,
            ),
            ContributionClaim::new(
                TEST_DID_CONTRIBUTOR,
                ResourceType::Compute,
                100,
                (0, 100),
                test_evidence(),
                700,
            ),
            ContributionClaim::new(
                TEST_DID_CONTRIBUTOR,
                ResourceType::Compute,
                100,
                (0, 100),
                test_evidence(),
                800,
            ),
            ContributionClaim::new(
                TEST_DID_CONTRIBUTOR,
                ResourceType::Compute,
                100,
                (0, 100),
                test_evidence(),
                900,
            ),
        ];

        let detector = create_test_fraud_detector(history, std::collections::HashMap::new())
            .with_rapid_claim_window(3600, 5); // 5 claims per hour max

        // 6th claim in the window
        let claim = ContributionClaim::new(
            TEST_DID_CONTRIBUTOR,
            ResourceType::Compute,
            100,
            (1000, 2000),
            test_evidence(),
            1000, // Within 3600 seconds of all history claims
        );

        let indicators = detector.analyze(&claim);
        assert!(indicators
            .iter()
            .any(|i| matches!(i, FraudIndicator::RapidClaims { .. })));
    }

    #[test]
    fn test_fraud_detector_conflicts_with_observations() {
        let mut observations = std::collections::HashMap::new();
        observations.insert(TEST_DID_CONTRIBUTOR.to_string(), 50); // Network observed 50

        let detector = create_test_fraud_detector(vec![], observations);

        // Claiming 150 (3x what network observed)
        let claim = ContributionClaim::new(
            TEST_DID_CONTRIBUTOR,
            ResourceType::Compute,
            150,
            (1000, 2000),
            test_evidence(),
            1000,
        );

        let indicators = detector.analyze(&claim);
        assert_eq!(indicators.len(), 1);
        assert!(matches!(
            indicators[0],
            FraudIndicator::ConflictsWithObservations { .. }
        ));
    }

    #[test]
    fn test_fraud_detector_multiple_indicators() {
        // High history average
        let history = vec![
            ContributionClaim::new(
                TEST_DID_CONTRIBUTOR,
                ResourceType::Compute,
                100,
                (0, 100),
                test_evidence(),
                500,
            ),
            ContributionClaim::new(
                TEST_DID_CONTRIBUTOR,
                ResourceType::Compute,
                100,
                (0, 100),
                test_evidence(),
                600,
            ),
            ContributionClaim::new(
                TEST_DID_CONTRIBUTOR,
                ResourceType::Compute,
                100,
                (0, 100),
                test_evidence(),
                700,
            ),
            ContributionClaim::new(
                TEST_DID_CONTRIBUTOR,
                ResourceType::Compute,
                100,
                (0, 100),
                test_evidence(),
                800,
            ),
            ContributionClaim::new(
                TEST_DID_CONTRIBUTOR,
                ResourceType::Compute,
                100,
                (0, 100),
                test_evidence(),
                900,
            ),
        ];

        let mut observations = std::collections::HashMap::new();
        observations.insert(TEST_DID_CONTRIBUTOR.to_string(), 50);

        let detector = create_test_fraud_detector(history, observations)
            .with_high_amount_threshold(2.0)
            .with_rapid_claim_window(3600, 5);

        // Claim that triggers multiple fraud indicators
        let claim = ContributionClaim::new(
            TEST_DID_CONTRIBUTOR,
            ResourceType::Compute,
            500, // 5x average (triggers high amount)
            (1000, 2000),
            test_evidence(),
            1000, // 6th claim in window (triggers rapid claims)
        );

        let indicators = detector.analyze(&claim);
        assert!(indicators.len() >= 2); // Should have at least high amount and rapid claims
    }
}
