//! Steward - Trusted operators for identity verification and infrastructure
//!
//! Stewards are Commons Holders who perform:
//! - Identity verification (proof-of-personhood attestation)
//! - Infrastructure maintenance
//! - Governance functions on behalf of the network
//!
//! Stewards are:
//! - Term-limited (not permanent positions)
//! - Bonded (stake against misbehavior)
//! - Subject to governance oversight

use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Unique identifier for a Steward record
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StewardId(pub [u8; 32]);

impl StewardId {
    /// Create a new StewardId from bytes
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Create a StewardId from the steward's DID
    pub fn from_did(did: &Did) -> Self {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"steward:");
        hasher.update(did.as_str().as_bytes());
        let result = hasher.finalize();
        let mut id = [0u8; 32];
        id.copy_from_slice(&result);
        Self(id)
    }

    /// Get the underlying bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl fmt::Display for StewardId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Record of a network steward
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StewardRecord {
    /// Unique steward ID
    pub steward_id: StewardId,

    /// Steward's operational DID (may be different from holder_did for security)
    pub steward_did: Did,

    /// Must be a Commons Holder - their holder DID
    pub holder_did: Did,

    /// Steward status
    pub status: StewardStatus,

    /// Geographic or sectoral scope (None = global)
    pub jurisdiction: Option<String>,

    /// Term start (Unix timestamp)
    pub term_start: u64,

    /// Term end (Unix timestamp) - stewards have limited terms
    pub term_end: u64,

    /// Bond posted (stake against misbehavior, in network credits)
    pub bond_amount: u64,

    /// Computed reputation from attestation outcomes (0.0 to 1.0)
    pub reputation_score: f64,

    /// Total attestations issued
    pub attestations_issued: u64,

    /// Attestations that were later disputed
    pub attestations_disputed: u64,

    /// Disputes filed against this steward
    pub disputes_against: u64,

    /// Disputes resolved in steward's favor
    pub disputes_won: u64,

    /// Governance approval reference (proposal ID that approved this steward)
    pub governance_approval: String,

    /// Optional specializations (e.g., "identity", "infrastructure", "mediation")
    pub specializations: Vec<String>,

    /// Contact/availability information
    pub contact_info: Option<StewardContact>,

    /// When record was created
    pub created_at: u64,

    /// Last update timestamp
    pub updated_at: u64,
}

impl StewardRecord {
    /// Create a new StewardRecord
    pub fn new(
        steward_did: Did,
        holder_did: Did,
        term_start: u64,
        term_end: u64,
        bond_amount: u64,
        governance_approval: String,
    ) -> Self {
        let steward_id = StewardId::from_did(&steward_did);
        let now = icn_time::current_timestamp_secs();

        Self {
            steward_id,
            steward_did,
            holder_did,
            status: StewardStatus::Active,
            jurisdiction: None,
            term_start,
            term_end,
            bond_amount,
            reputation_score: 1.0, // Start with full reputation
            attestations_issued: 0,
            attestations_disputed: 0,
            disputes_against: 0,
            disputes_won: 0,
            governance_approval,
            specializations: Vec::new(),
            contact_info: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Get the steward ID
    pub fn id(&self) -> &StewardId {
        &self.steward_id
    }

    /// Check if steward is active
    pub fn is_active(&self) -> bool {
        matches!(self.status, StewardStatus::Active)
    }

    /// Check if steward is suspended
    pub fn is_suspended(&self) -> bool {
        matches!(self.status, StewardStatus::Suspended { .. })
    }

    /// Check if steward term has expired
    pub fn is_term_expired(&self) -> bool {
        let now = icn_time::current_timestamp_secs();
        now >= self.term_end
    }

    /// Check if steward can issue attestations
    pub fn can_attest(&self) -> bool {
        self.is_active() && !self.is_term_expired() && self.reputation_score >= 0.5
    }

    /// Set jurisdiction scope
    pub fn with_jurisdiction(mut self, jurisdiction: impl Into<String>) -> Self {
        self.jurisdiction = Some(jurisdiction.into());
        self
    }

    /// Add a specialization
    pub fn add_specialization(&mut self, specialization: impl Into<String>) {
        let spec = specialization.into();
        if !self.specializations.contains(&spec) {
            self.specializations.push(spec);
            self.updated_at = icn_time::current_timestamp_secs();
        }
    }

    /// Set contact info
    pub fn set_contact(&mut self, contact: StewardContact) {
        self.contact_info = Some(contact);
        self.updated_at = icn_time::current_timestamp_secs();
    }

    /// Record an attestation issued
    pub fn record_attestation(&mut self) {
        self.attestations_issued += 1;
        self.updated_at = icn_time::current_timestamp_secs();
    }

    /// Record a disputed attestation
    pub fn record_dispute(&mut self) {
        self.attestations_disputed += 1;
        self.disputes_against += 1;
        self.recalculate_reputation();
        self.updated_at = icn_time::current_timestamp_secs();
    }

    /// Record a dispute won
    pub fn record_dispute_won(&mut self) {
        self.disputes_won += 1;
        self.recalculate_reputation();
        self.updated_at = icn_time::current_timestamp_secs();
    }

    /// Recalculate reputation score based on attestation history
    fn recalculate_reputation(&mut self) {
        if self.attestations_issued == 0 {
            self.reputation_score = 1.0;
            return;
        }

        // Base score from attestation success rate
        let disputed_rate = self.attestations_disputed as f64 / self.attestations_issued as f64;
        let base_score = 1.0 - disputed_rate;

        // Bonus for disputes won
        let dispute_bonus = if self.disputes_against > 0 {
            (self.disputes_won as f64 / self.disputes_against as f64) * 0.1
        } else {
            0.0
        };

        self.reputation_score = (base_score + dispute_bonus).clamp(0.0, 1.0);
    }

    /// Suspend the steward
    pub fn suspend(&mut self, reason: String) {
        self.status = StewardStatus::Suspended { reason };
        self.updated_at = icn_time::current_timestamp_secs();
    }

    /// Reinstate a suspended steward
    pub fn reinstate(&mut self) -> bool {
        if matches!(self.status, StewardStatus::Suspended { .. }) {
            self.status = StewardStatus::Active;
            self.updated_at = icn_time::current_timestamp_secs();
            true
        } else {
            false
        }
    }

    /// Retire the steward
    pub fn retire(&mut self) {
        self.status = StewardStatus::Retired;
        self.updated_at = icn_time::current_timestamp_secs();
    }

    /// Revoke the steward for cause
    pub fn revoke(&mut self, reason: String, evidence: Vec<[u8; 32]>) {
        self.status = StewardStatus::Revoked { reason, evidence };
        self.updated_at = icn_time::current_timestamp_secs();
    }

    /// Extend the steward's term
    pub fn extend_term(&mut self, new_term_end: u64) {
        if new_term_end > self.term_end {
            self.term_end = new_term_end;
            self.updated_at = icn_time::current_timestamp_secs();
        }
    }

    /// Add to the bond
    pub fn add_bond(&mut self, amount: u64) {
        self.bond_amount += amount;
        self.updated_at = icn_time::current_timestamp_secs();
    }

    /// Slash the bond (for misbehavior)
    pub fn slash_bond(&mut self, amount: u64) -> u64 {
        let slashed = amount.min(self.bond_amount);
        self.bond_amount -= slashed;
        self.updated_at = icn_time::current_timestamp_secs();
        slashed
    }

    /// Get effectiveness score (combination of activity and reputation)
    pub fn effectiveness_score(&self) -> f64 {
        // Factor in attestation volume and reputation
        let volume_factor = (self.attestations_issued as f64 / 100.0).min(1.0);
        let dispute_rate = if self.attestations_issued > 0 {
            1.0 - (self.attestations_disputed as f64 / self.attestations_issued as f64)
        } else {
            1.0
        };

        (self.reputation_score * 0.6 + volume_factor * 0.2 + dispute_rate * 0.2).clamp(0.0, 1.0)
    }
}

/// Steward lifecycle status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StewardStatus {
    /// Active and able to perform steward duties
    Active,
    /// Temporarily suspended
    Suspended { reason: String },
    /// Voluntarily retired
    Retired,
    /// Revoked for cause
    Revoked {
        reason: String,
        evidence: Vec<[u8; 32]>,
    },
}

impl fmt::Display for StewardStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StewardStatus::Active => write!(f, "Active"),
            StewardStatus::Suspended { reason } => write!(f, "Suspended: {reason}"),
            StewardStatus::Retired => write!(f, "Retired"),
            StewardStatus::Revoked { reason, .. } => write!(f, "Revoked: {reason}"),
        }
    }
}

/// Steward contact/availability information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StewardContact {
    /// Public email (optional)
    pub email: Option<String>,
    /// Geographic location/timezone
    pub location: Option<String>,
    /// Languages spoken
    pub languages: Vec<String>,
    /// Availability hours (e.g., "Mon-Fri 9am-5pm PST")
    pub availability: Option<String>,
    /// Preferred contact method
    pub preferred_contact: ContactMethod,
}

impl Default for StewardContact {
    fn default() -> Self {
        Self {
            email: None,
            location: None,
            languages: vec!["en".to_string()],
            availability: None,
            preferred_contact: ContactMethod::InNetwork,
        }
    }
}

/// Preferred contact method
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContactMethod {
    /// In-network messaging
    InNetwork,
    /// Email
    Email,
    /// Video call
    VideoCall,
    /// In-person
    InPerson,
}

impl fmt::Display for ContactMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContactMethod::InNetwork => write!(f, "In-Network"),
            ContactMethod::Email => write!(f, "Email"),
            ContactMethod::VideoCall => write!(f, "Video Call"),
            ContactMethod::InPerson => write!(f, "In-Person"),
        }
    }
}

/// Steward attestation record (for tracking attestation history)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StewardAttestation {
    /// The steward who issued the attestation
    pub steward_id: StewardId,
    /// Target anchor ID that was attested
    pub target_anchor_id: [u8; 32],
    /// Type of attestation
    pub attestation_type: AttestationType,
    /// When attestation was issued
    pub issued_at: u64,
    /// Whether attestation was later disputed
    pub disputed: bool,
    /// Dispute outcome if disputed
    pub dispute_outcome: Option<DisputeOutcome>,
}

/// Type of attestation a steward can issue
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttestationType {
    /// Initial proof-of-personhood
    InitialPOP,
    /// Renewal of existing POP
    POPRenewal,
    /// Recovery attestation (helping with key recovery)
    Recovery,
    /// Sponsorship (vouching for another holder)
    Sponsorship,
}

impl fmt::Display for AttestationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AttestationType::InitialPOP => write!(f, "Initial POP"),
            AttestationType::POPRenewal => write!(f, "POP Renewal"),
            AttestationType::Recovery => write!(f, "Recovery"),
            AttestationType::Sponsorship => write!(f, "Sponsorship"),
        }
    }
}

/// Outcome of a dispute against an attestation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisputeOutcome {
    /// Steward was upheld (attestation valid)
    StewardUpheld,
    /// Attestation was invalidated
    AttestationInvalidated,
    /// Dispute was dismissed
    Dismissed,
    /// Still pending
    Pending,
}

impl fmt::Display for DisputeOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DisputeOutcome::StewardUpheld => write!(f, "Steward Upheld"),
            DisputeOutcome::AttestationInvalidated => write!(f, "Attestation Invalidated"),
            DisputeOutcome::Dismissed => write!(f, "Dismissed"),
            DisputeOutcome::Pending => write!(f, "Pending"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_did() -> Did {
        Did::from_anchor_id(&[42u8; 32])
    }

    fn test_holder_did() -> Did {
        Did::from_anchor_id(&[43u8; 32])
    }

    fn create_test_steward() -> StewardRecord {
        let now = icn_time::current_timestamp_secs();

        StewardRecord::new(
            test_did(),
            test_holder_did(),
            now,
            now + 365 * 24 * 60 * 60, // 1 year term
            1000,
            "proposal-001".to_string(),
        )
    }

    #[test]
    fn test_steward_creation() {
        let steward = create_test_steward();

        assert!(steward.is_active());
        assert!(!steward.is_suspended());
        assert!(!steward.is_term_expired());
        assert!(steward.can_attest());
        assert_eq!(steward.reputation_score, 1.0);
        assert_eq!(steward.bond_amount, 1000);
    }

    #[test]
    fn test_steward_id() {
        let did = test_did();
        let id1 = StewardId::from_did(&did);
        let id2 = StewardId::from_did(&did);

        assert_eq!(id1.0, id2.0); // Same DID produces same ID
    }

    #[test]
    fn test_steward_status_lifecycle() {
        let mut steward = create_test_steward();

        // Suspend
        steward.suspend("Under review".to_string());
        assert!(steward.is_suspended());
        assert!(!steward.can_attest());

        // Reinstate
        assert!(steward.reinstate());
        assert!(steward.is_active());
        assert!(steward.can_attest());

        // Retire
        steward.retire();
        assert!(matches!(steward.status, StewardStatus::Retired));
        assert!(!steward.can_attest());
    }

    #[test]
    fn test_steward_revocation() {
        let mut steward = create_test_steward();

        let evidence = vec![[1u8; 32], [2u8; 32]];
        steward.revoke("Fraudulent attestations".to_string(), evidence.clone());

        assert!(matches!(steward.status, StewardStatus::Revoked { .. }));
        if let StewardStatus::Revoked { evidence: e, .. } = &steward.status {
            assert_eq!(e.len(), 2);
        }
    }

    #[test]
    fn test_attestation_tracking() {
        let mut steward = create_test_steward();

        // Record some attestations
        for _ in 0..10 {
            steward.record_attestation();
        }
        assert_eq!(steward.attestations_issued, 10);
        assert_eq!(steward.reputation_score, 1.0);

        // Record a dispute
        steward.record_dispute();
        assert_eq!(steward.attestations_disputed, 1);
        assert!(steward.reputation_score < 1.0);

        // Record dispute won
        steward.record_dispute_won();
        assert_eq!(steward.disputes_won, 1);
    }

    #[test]
    fn test_reputation_calculation() {
        let mut steward = create_test_steward();

        // Issue 100 attestations
        for _ in 0..100 {
            steward.record_attestation();
        }
        assert_eq!(steward.reputation_score, 1.0);

        // 10% dispute rate
        for _ in 0..10 {
            steward.record_dispute();
        }
        assert!(steward.reputation_score < 1.0);
        assert!(steward.reputation_score > 0.8); // ~90% base score

        // Win some disputes
        for _ in 0..5 {
            steward.record_dispute_won();
        }
        assert!(steward.reputation_score > 0.9); // Bonus for wins
    }

    #[test]
    fn test_can_attest_reputation_threshold() {
        let mut steward = create_test_steward();

        // Issue some attestations
        for _ in 0..10 {
            steward.record_attestation();
        }

        // Tank reputation
        for _ in 0..6 {
            steward.record_dispute();
        }

        // Below 0.5 reputation threshold
        assert!(steward.reputation_score < 0.5);
        assert!(!steward.can_attest());
    }

    #[test]
    fn test_bond_management() {
        let mut steward = create_test_steward();
        assert_eq!(steward.bond_amount, 1000);

        // Add bond
        steward.add_bond(500);
        assert_eq!(steward.bond_amount, 1500);

        // Slash bond
        let slashed = steward.slash_bond(300);
        assert_eq!(slashed, 300);
        assert_eq!(steward.bond_amount, 1200);

        // Slash more than available
        let slashed = steward.slash_bond(2000);
        assert_eq!(slashed, 1200);
        assert_eq!(steward.bond_amount, 0);
    }

    #[test]
    fn test_term_extension() {
        let mut steward = create_test_steward();
        let original_end = steward.term_end;

        // Extend term
        let new_end = original_end + 180 * 24 * 60 * 60; // 6 months
        steward.extend_term(new_end);
        assert_eq!(steward.term_end, new_end);

        // Can't shorten term
        steward.extend_term(original_end);
        assert_eq!(steward.term_end, new_end);
    }

    #[test]
    fn test_specializations() {
        let mut steward = create_test_steward();

        steward.add_specialization("identity");
        steward.add_specialization("mediation");
        steward.add_specialization("identity"); // Duplicate

        assert_eq!(steward.specializations.len(), 2);
        assert!(steward.specializations.contains(&"identity".to_string()));
        assert!(steward.specializations.contains(&"mediation".to_string()));
    }

    #[test]
    fn test_jurisdiction() {
        let steward = create_test_steward().with_jurisdiction("federation:pacific-nw");

        assert_eq!(
            steward.jurisdiction,
            Some("federation:pacific-nw".to_string())
        );
    }

    #[test]
    fn test_contact_info() {
        let mut steward = create_test_steward();

        let contact = StewardContact {
            email: Some("steward@example.com".to_string()),
            location: Some("Portland, OR".to_string()),
            languages: vec!["en".to_string(), "es".to_string()],
            availability: Some("Mon-Fri 9am-5pm PST".to_string()),
            preferred_contact: ContactMethod::VideoCall,
        };

        steward.set_contact(contact);
        assert!(steward.contact_info.is_some());
        assert_eq!(
            steward.contact_info.as_ref().unwrap().preferred_contact,
            ContactMethod::VideoCall
        );
    }

    #[test]
    fn test_effectiveness_score() {
        let mut steward = create_test_steward();

        // New steward with no history
        let initial_score = steward.effectiveness_score();
        assert!(initial_score > 0.5); // High reputation, low volume

        // Build history
        for _ in 0..100 {
            steward.record_attestation();
        }

        let experienced_score = steward.effectiveness_score();
        assert!(experienced_score > initial_score); // Higher with volume
    }

    #[test]
    fn test_steward_status_display() {
        assert_eq!(format!("{}", StewardStatus::Active), "Active");
        assert_eq!(format!("{}", StewardStatus::Retired), "Retired");

        let suspended = StewardStatus::Suspended {
            reason: "review".to_string(),
        };
        assert!(format!("{suspended}").contains("Suspended"));
    }

    #[test]
    fn test_attestation_type_display() {
        assert_eq!(format!("{}", AttestationType::InitialPOP), "Initial POP");
        assert_eq!(format!("{}", AttestationType::Recovery), "Recovery");
    }

    #[test]
    fn test_serialization() {
        let steward = create_test_steward();

        let json = serde_json::to_string(&steward).unwrap();
        let deserialized: StewardRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.steward_id.0, steward.steward_id.0);
        assert_eq!(deserialized.bond_amount, steward.bond_amount);
    }
}
