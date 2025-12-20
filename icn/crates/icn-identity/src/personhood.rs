//! Personhood Anchor - Commons Evolution Layer 0
//!
//! A PersonhoodAnchor extends the SDIS Anchor with Commons-specific concepts:
//! - Status lifecycle (active, suspended, revoked)
//! - Multiple proof-of-personhood attestations with levels
//! - Key rotation history tracking
//! - Uniqueness proof management
//!
//! This module implements the Commons Evolution RFC (docs/COMMONS_EVOLUTION.md).
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                        PersonhoodAnchor                                  │
//! │  ┌─────────────────────────────────────────────────────────────────────┐│
//! │  │  anchor: Anchor (SDIS layer)                                        ││
//! │  │    - id, created_at, pathway, vui_commitment                        ││
//! │  └─────────────────────────────────────────────────────────────────────┘│
//! │  ┌─────────────────────────────────────────────────────────────────────┐│
//! │  │  status: AnchorStatus (Commons layer)                               ││
//! │  │    - Active | Suspended | Revoked                                   ││
//! │  └─────────────────────────────────────────────────────────────────────┘│
//! │  ┌─────────────────────────────────────────────────────────────────────┐│
//! │  │  pop_attestations: Vec<POPAttestation>                              ││
//! │  │    - Multiple attestations with levels and expiry                   ││
//! │  └─────────────────────────────────────────────────────────────────────┘│
//! │  ┌─────────────────────────────────────────────────────────────────────┐│
//! │  │  key_rotations: Vec<KeyRotationRecord>                              ││
//! │  │    - History of all key rotations for this anchor                   ││
//! │  └─────────────────────────────────────────────────────────────────────┘│
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Key Properties
//!
//! - **Permanent**: Anchor ID never changes, survives key rotations
//! - **Status-aware**: Lifecycle management with due process
//! - **Multi-attestation**: Multiple POP attestations strengthen identity
//! - **Auditable**: Full key rotation history preserved

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::anchor::Anchor;
use crate::Did;

/// Personhood Anchor - Layer 0 of the Commons identity model
///
/// A PersonhoodAnchor wraps an SDIS Anchor with Commons-specific state:
/// status tracking, multiple POP attestations, and key rotation history.
///
/// This is the cryptographic root of a Commons Holder's identity,
/// independent of key rotation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonhoodAnchor {
    /// The underlying SDIS anchor
    pub anchor: Anchor,

    /// Current status of this anchor
    pub status: AnchorStatus,

    /// Proof-of-personhood attestations
    /// Multiple attestations strengthen the identity claim
    pub pop_attestations: Vec<POPAttestation>,

    /// Uniqueness proofs for Sybil resistance
    pub uniqueness_proofs: Vec<UniquenessProof>,

    /// Key rotation history
    pub key_rotations: Vec<KeyRotationRecord>,

    /// Current active public key (Ed25519)
    /// This is the key currently authorized to act on behalf of this anchor
    pub current_key: Option<[u8; 32]>,

    /// Last modification timestamp
    pub updated_at: u64,
}

/// Status of a PersonhoodAnchor
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnchorStatus {
    /// Active and in good standing
    Active,

    /// Temporarily suspended
    Suspended {
        /// Reason for suspension
        reason: String,
        /// When the suspension occurred
        suspended_at: u64,
        /// Optional expiry (auto-reinstate after this time)
        until: Option<u64>,
        /// Who suspended (governance DID or steward)
        authority: Did,
    },

    /// Permanently revoked
    Revoked {
        /// Reason for revocation
        reason: String,
        /// When revocation occurred
        revoked_at: u64,
        /// Evidence references (hashes of documents/proceedings)
        evidence: Vec<[u8; 32]>,
        /// Who revoked (must be governance authority)
        authority: Did,
    },
}

/// Proof-of-personhood attestation from a steward or organization
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct POPAttestation {
    /// Unique attestation ID
    pub attestation_id: [u8; 32],

    /// Issuer (steward or organizational DID)
    pub issuer_did: Did,

    /// Verification method used
    pub method: POPMethod,

    /// Strength level of verification
    pub level: POPLevel,

    /// When attestation was issued
    pub issued_at: u64,

    /// Optional expiry (some methods require renewal)
    pub expiry: Option<u64>,

    /// Cryptographic signature over (anchor_id, method, level, issued_at)
    pub signature: Vec<u8>,

    /// Optional metadata (JSON-encoded)
    pub metadata: Option<String>,
}

/// Method used for proof-of-personhood verification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum POPMethod {
    /// In-person verification by a steward
    InPerson {
        /// Location where verification occurred (optional, hashed)
        location_hash: Option<[u8; 32]>,
    },

    /// Sponsored by an existing verified holder
    Sponsored {
        /// DID of the sponsor
        sponsor_did: Did,
    },

    /// Biometric verification (privacy-preserving)
    Biometric {
        /// Type of biometric (face, fingerprint, iris, etc.)
        biometric_type: BiometricType,
        /// Hash of the biometric template (never stored directly)
        template_hash: [u8; 32],
    },

    /// Community ceremony (key signing party, etc.)
    Ceremony {
        /// Witness DIDs (those present at ceremony)
        witnesses: Vec<Did>,
        /// Ceremony type identifier
        ceremony_type: String,
    },

    /// Video call verification
    VideoCall {
        /// Verifier's DID
        verifier_did: Did,
        /// Hash of the recording (if retained)
        recording_hash: Option<[u8; 32]>,
    },

    /// Government ID verification (privacy-preserving)
    GovernmentId {
        /// ISO 3166-1 alpha-2 country code
        country: [u8; 2],
        /// Document type hash
        doc_type_hash: [u8; 8],
    },

    /// Web of trust vouching by existing members
    WebOfTrust {
        /// DIDs of vouchers
        vouchers: Vec<Did>,
    },

    /// Genesis (for bootstrap/testing only)
    Genesis {
        /// Reason for genesis attestation
        reason: String,
    },
}

/// Type of biometric used for verification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BiometricType {
    /// Facial recognition
    Face,
    /// Fingerprint scan
    Fingerprint,
    /// Iris scan
    Iris,
    /// Voice recognition
    Voice,
    /// Other biometric type with description
    Other(String),
}

/// Strength level of proof-of-personhood verification
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum POPLevel {
    /// Basic - single attestation, limited capabilities
    /// Suitable for initial enrollment, probationary period
    Weak = 1,

    /// Standard - multiple attestations or strong method
    /// Full Commons Holder capabilities
    Strong = 2,

    /// High assurance - multiple methods, periodic renewal
    /// Required for steward roles or high-trust operations
    Verified = 3,
}

/// Proof of uniqueness (Sybil resistance)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UniquenessProof {
    /// Proof ID
    pub proof_id: [u8; 32],

    /// Type of uniqueness proof
    pub proof_type: UniquenessProofType,

    /// When this proof was created
    pub created_at: u64,

    /// Steward attestations (threshold required for validity)
    pub attestations: Vec<UniquenessAttestation>,
}

/// Type of uniqueness proof
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UniquenessProofType {
    /// VUI-based (threshold PRF)
    Vui {
        /// Commitment to the VUI (not the VUI itself)
        vui_commitment: [u8; 32],
    },

    /// Biometric uniqueness
    Biometric {
        /// Type of biometric
        biometric_type: BiometricType,
        /// Commitment to biometric template
        template_commitment: [u8; 32],
    },

    /// Social graph uniqueness (one-person-one-account enforcement)
    SocialGraph {
        /// Hash of the social proof structure
        proof_hash: [u8; 32],
    },
}

/// Steward attestation for uniqueness proof
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UniquenessAttestation {
    /// Steward's DID
    pub steward_did: Did,

    /// Signature over (proof_id, anchor_id, "unique")
    pub signature: Vec<u8>,

    /// When attested
    pub attested_at: u64,
}

/// Record of a key rotation for this anchor
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyRotationRecord {
    /// Rotation event ID
    pub rotation_id: [u8; 32],

    /// Previous public key
    pub old_key: [u8; 32],

    /// New public key
    pub new_key: [u8; 32],

    /// Reason for rotation
    pub reason: KeyRotationReason,

    /// When rotation occurred
    pub rotated_at: u64,

    /// Signature by old key authorizing rotation
    pub authorization_signature: Vec<u8>,

    /// Optional: recovery signatures (if rotated via recovery)
    pub recovery_signatures: Vec<RecoverySignature>,
}

/// Reason for key rotation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyRotationReason {
    /// Scheduled rotation (key hygiene)
    Scheduled,
    /// Key compromise suspected or confirmed
    Compromise,
    /// Device change (new device, lost device)
    DeviceChange,
    /// Recovery (social recovery or backup restore)
    Recovery,
    /// Upgrade (e.g., adding post-quantum keys)
    Upgrade,
}

/// Recovery signature for key rotation via recovery
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoverySignature {
    /// Signer's DID (recovery guardian or steward)
    pub signer_did: Did,
    /// Signature over (rotation_id, new_key)
    pub signature: Vec<u8>,
    /// When signed
    pub signed_at: u64,
}

impl PersonhoodAnchor {
    /// Create a new PersonhoodAnchor from an SDIS Anchor
    ///
    /// This is the migration path for existing anchors to Commons participation.
    pub fn from_anchor(anchor: Anchor) -> Self {
        let now = icn_time::current_timestamp_secs();

        Self {
            anchor,
            status: AnchorStatus::Active,
            pop_attestations: Vec::new(),
            uniqueness_proofs: Vec::new(),
            key_rotations: Vec::new(),
            current_key: None,
            updated_at: now,
        }
    }

    /// Create a new PersonhoodAnchor with initial POP attestation
    pub fn new(anchor: Anchor, initial_attestation: POPAttestation, current_key: [u8; 32]) -> Self {
        let now = icn_time::current_timestamp_secs();

        Self {
            anchor,
            status: AnchorStatus::Active,
            pop_attestations: vec![initial_attestation],
            uniqueness_proofs: Vec::new(),
            key_rotations: Vec::new(),
            current_key: Some(current_key),
            updated_at: now,
        }
    }

    /// Create a genesis PersonhoodAnchor (for bootstrap/testing)
    pub fn genesis(reason: &str, current_key: [u8; 32]) -> Self {
        let anchor = Anchor::genesis(reason);
        let now = icn_time::current_timestamp_secs();

        // Create a genesis attestation
        let attestation =
            POPAttestation::genesis(&anchor.id, Did::from_anchor_id(&anchor.id), reason);

        Self {
            anchor,
            status: AnchorStatus::Active,
            pop_attestations: vec![attestation],
            uniqueness_proofs: Vec::new(),
            key_rotations: Vec::new(),
            current_key: Some(current_key),
            updated_at: now,
        }
    }

    /// Get the anchor ID
    pub fn id(&self) -> &[u8; 32] {
        &self.anchor.id
    }

    /// Get the anchor ID as hex
    pub fn id_hex(&self) -> String {
        hex::encode(self.anchor.id)
    }

    /// Convert to DID
    pub fn to_did(&self) -> Did {
        self.anchor.to_did()
    }

    /// Check if this anchor is active
    pub fn is_active(&self) -> bool {
        matches!(self.status, AnchorStatus::Active)
    }

    /// Check if this anchor is suspended
    pub fn is_suspended(&self) -> bool {
        matches!(self.status, AnchorStatus::Suspended { .. })
    }

    /// Check if this anchor is revoked
    pub fn is_revoked(&self) -> bool {
        matches!(self.status, AnchorStatus::Revoked { .. })
    }

    /// Get the highest POP level from attestations
    pub fn pop_level(&self) -> Option<POPLevel> {
        self.pop_attestations
            .iter()
            .filter(|a| !a.is_expired())
            .map(|a| a.level)
            .max()
    }

    /// Check if this anchor meets a minimum POP level
    pub fn meets_pop_level(&self, required: POPLevel) -> bool {
        self.pop_level().map(|l| l >= required).unwrap_or(false)
    }

    /// Add a POP attestation
    pub fn add_attestation(&mut self, attestation: POPAttestation) {
        self.pop_attestations.push(attestation);
        self.updated_at = icn_time::current_timestamp_secs();
    }

    /// Add a uniqueness proof
    pub fn add_uniqueness_proof(&mut self, proof: UniquenessProof) {
        self.uniqueness_proofs.push(proof);
        self.updated_at = icn_time::current_timestamp_secs();
    }

    /// Record a key rotation
    pub fn record_key_rotation(&mut self, rotation: KeyRotationRecord) {
        self.current_key = Some(rotation.new_key);
        self.key_rotations.push(rotation);
        self.updated_at = icn_time::current_timestamp_secs();
    }

    /// Suspend this anchor
    pub fn suspend(&mut self, reason: String, authority: Did, until: Option<u64>) {
        let now = icn_time::current_timestamp_secs();

        self.status = AnchorStatus::Suspended {
            reason,
            suspended_at: now,
            until,
            authority,
        };
        self.updated_at = now;
    }

    /// Reinstate a suspended anchor
    pub fn reinstate(&mut self) -> Result<(), &'static str> {
        match &self.status {
            AnchorStatus::Suspended { .. } => {
                self.status = AnchorStatus::Active;
                self.updated_at = icn_time::current_timestamp_secs();
                Ok(())
            }
            AnchorStatus::Active => Err("Anchor is already active"),
            AnchorStatus::Revoked { .. } => Err("Cannot reinstate a revoked anchor"),
        }
    }

    /// Revoke this anchor (permanent)
    pub fn revoke(&mut self, reason: String, authority: Did, evidence: Vec<[u8; 32]>) {
        let now = icn_time::current_timestamp_secs();

        self.status = AnchorStatus::Revoked {
            reason,
            revoked_at: now,
            evidence,
            authority,
        };
        self.updated_at = now;
    }

    /// Get count of valid (non-expired) attestations
    pub fn valid_attestation_count(&self) -> usize {
        self.pop_attestations
            .iter()
            .filter(|a| !a.is_expired())
            .count()
    }

    /// Check if anchor has sufficient attestations (typically 2+ for full status)
    pub fn has_sufficient_attestations(&self, threshold: usize) -> bool {
        self.valid_attestation_count() >= threshold
    }
}

impl POPAttestation {
    /// Create a new POP attestation
    pub fn new(
        anchor_id: &[u8; 32],
        issuer_did: Did,
        method: POPMethod,
        level: POPLevel,
        signature: Vec<u8>,
        expiry: Option<u64>,
    ) -> Self {
        let now = icn_time::current_timestamp_secs();

        // Compute attestation ID
        let mut hasher = Sha256::new();
        hasher.update(b"icn-pop-attestation-v1");
        hasher.update(anchor_id);
        hasher.update(issuer_did.as_str().as_bytes());
        hasher.update(now.to_le_bytes());
        let result = hasher.finalize();

        let mut attestation_id = [0u8; 32];
        attestation_id.copy_from_slice(&result);

        Self {
            attestation_id,
            issuer_did,
            method,
            level,
            issued_at: now,
            expiry,
            signature,
            metadata: None,
        }
    }

    /// Create a genesis attestation (for bootstrap/testing)
    pub fn genesis(anchor_id: &[u8; 32], issuer_did: Did, reason: &str) -> Self {
        Self::new(
            anchor_id,
            issuer_did,
            POPMethod::Genesis {
                reason: reason.to_string(),
            },
            POPLevel::Weak,
            Vec::new(), // No signature for genesis
            None,
        )
    }

    /// Check if this attestation is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expiry) = self.expiry {
            let now = icn_time::current_timestamp_secs();
            now > expiry
        } else {
            false
        }
    }

    /// Check if this attestation is still valid
    pub fn is_valid(&self) -> bool {
        !self.is_expired()
    }

    /// Get attestation ID as hex
    pub fn id_hex(&self) -> String {
        hex::encode(self.attestation_id)
    }
}

impl POPLevel {
    /// Get the string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            POPLevel::Weak => "weak",
            POPLevel::Strong => "strong",
            POPLevel::Verified => "verified",
        }
    }

    /// Parse from string
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "weak" => Some(POPLevel::Weak),
            "strong" => Some(POPLevel::Strong),
            "verified" => Some(POPLevel::Verified),
            _ => None,
        }
    }
}

impl std::str::FromStr for POPLevel {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        POPLevel::parse(s).ok_or(())
    }
}

impl std::fmt::Display for POPLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::fmt::Display for AnchorStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnchorStatus::Active => write!(f, "active"),
            AnchorStatus::Suspended { reason, .. } => write!(f, "suspended: {reason}"),
            AnchorStatus::Revoked { reason, .. } => write!(f, "revoked: {reason}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> [u8; 32] {
        [42u8; 32]
    }

    #[test]
    fn test_personhood_anchor_genesis() {
        let pa = PersonhoodAnchor::genesis("test bootstrap", test_key());

        assert!(pa.is_active());
        assert!(!pa.is_suspended());
        assert!(!pa.is_revoked());
        assert_eq!(pa.current_key, Some(test_key()));
        assert_eq!(pa.pop_attestations.len(), 1);
    }

    #[test]
    fn test_personhood_anchor_from_anchor() {
        let anchor = Anchor::genesis("test");
        let pa = PersonhoodAnchor::from_anchor(anchor.clone());

        assert_eq!(pa.anchor.id, anchor.id);
        assert!(pa.is_active());
        assert!(pa.pop_attestations.is_empty());
    }

    #[test]
    fn test_anchor_status_lifecycle() {
        let mut pa = PersonhoodAnchor::genesis("test", test_key());

        // Initially active
        assert!(pa.is_active());

        // Suspend
        let authority = Did::from_anchor_id(&[1u8; 32]);
        pa.suspend("policy violation".to_string(), authority.clone(), None);
        assert!(pa.is_suspended());
        assert!(!pa.is_active());

        // Reinstate
        pa.reinstate().unwrap();
        assert!(pa.is_active());

        // Revoke
        pa.revoke("serious violation".to_string(), authority, vec![[99u8; 32]]);
        assert!(pa.is_revoked());
        assert!(!pa.is_active());

        // Cannot reinstate revoked
        assert!(pa.reinstate().is_err());
    }

    #[test]
    fn test_pop_level_ordering() {
        assert!(POPLevel::Weak < POPLevel::Strong);
        assert!(POPLevel::Strong < POPLevel::Verified);
        assert!(POPLevel::Weak < POPLevel::Verified);
    }

    #[test]
    fn test_pop_attestation_creation() {
        let anchor_id = [1u8; 32];
        let issuer = Did::from_anchor_id(&[2u8; 32]);

        let attestation = POPAttestation::new(
            &anchor_id,
            issuer.clone(),
            POPMethod::InPerson {
                location_hash: None,
            },
            POPLevel::Strong,
            vec![1, 2, 3],
            None,
        );

        assert!(!attestation.is_expired());
        assert!(attestation.is_valid());
        assert_eq!(attestation.level, POPLevel::Strong);
        assert_eq!(attestation.issuer_did, issuer);
    }

    #[test]
    fn test_pop_attestation_expiry() {
        let anchor_id = [1u8; 32];
        let issuer = Did::from_anchor_id(&[2u8; 32]);

        // Create attestation that expired in the past
        let mut attestation = POPAttestation::new(
            &anchor_id,
            issuer,
            POPMethod::InPerson {
                location_hash: None,
            },
            POPLevel::Strong,
            vec![1, 2, 3],
            Some(1), // Expired at timestamp 1
        );

        // Force issued_at to be before expiry for valid test
        attestation.issued_at = 0;

        assert!(attestation.is_expired());
        assert!(!attestation.is_valid());
    }

    #[test]
    fn test_pop_level_parsing() {
        assert_eq!(POPLevel::parse("weak"), Some(POPLevel::Weak));
        assert_eq!(POPLevel::parse("STRONG"), Some(POPLevel::Strong));
        assert_eq!(POPLevel::parse("Verified"), Some(POPLevel::Verified));
        assert_eq!(POPLevel::parse("invalid"), None);
    }

    #[test]
    fn test_pop_level_display() {
        assert_eq!(POPLevel::Weak.to_string(), "weak");
        assert_eq!(POPLevel::Strong.to_string(), "strong");
        assert_eq!(POPLevel::Verified.to_string(), "verified");
    }

    #[test]
    fn test_multiple_attestations() {
        let mut pa = PersonhoodAnchor::genesis("test", test_key());

        // Add another attestation
        let issuer = Did::from_anchor_id(&[2u8; 32]);
        let attestation = POPAttestation::new(
            pa.id(),
            issuer,
            POPMethod::InPerson {
                location_hash: None,
            },
            POPLevel::Strong,
            vec![1, 2, 3],
            None,
        );

        pa.add_attestation(attestation);

        assert_eq!(pa.valid_attestation_count(), 2);
        assert!(pa.has_sufficient_attestations(2));
        assert_eq!(pa.pop_level(), Some(POPLevel::Strong));
    }

    #[test]
    fn test_meets_pop_level() {
        let mut pa = PersonhoodAnchor::genesis("test", test_key());

        // Genesis gives Weak level
        assert!(pa.meets_pop_level(POPLevel::Weak));
        assert!(!pa.meets_pop_level(POPLevel::Strong));

        // Add Strong attestation
        let issuer = Did::from_anchor_id(&[2u8; 32]);
        let attestation = POPAttestation::new(
            pa.id(),
            issuer,
            POPMethod::InPerson {
                location_hash: None,
            },
            POPLevel::Strong,
            vec![],
            None,
        );
        pa.add_attestation(attestation);

        assert!(pa.meets_pop_level(POPLevel::Strong));
        assert!(!pa.meets_pop_level(POPLevel::Verified));
    }

    #[test]
    fn test_key_rotation_record() {
        let mut pa = PersonhoodAnchor::genesis("test", test_key());
        let old_key = test_key();
        let new_key = [43u8; 32];

        let rotation = KeyRotationRecord {
            rotation_id: [1u8; 32],
            old_key,
            new_key,
            reason: KeyRotationReason::Scheduled,
            rotated_at: 12345,
            authorization_signature: vec![1, 2, 3],
            recovery_signatures: vec![],
        };

        pa.record_key_rotation(rotation);

        assert_eq!(pa.current_key, Some(new_key));
        assert_eq!(pa.key_rotations.len(), 1);
    }

    #[test]
    fn test_uniqueness_proof() {
        let mut pa = PersonhoodAnchor::genesis("test", test_key());

        let proof = UniquenessProof {
            proof_id: [1u8; 32],
            proof_type: UniquenessProofType::Vui {
                vui_commitment: [2u8; 32],
            },
            created_at: 12345,
            attestations: vec![],
        };

        pa.add_uniqueness_proof(proof);

        assert_eq!(pa.uniqueness_proofs.len(), 1);
    }

    #[test]
    fn test_serialization() {
        let pa = PersonhoodAnchor::genesis("test", test_key());

        let json = serde_json::to_string(&pa).unwrap();
        let restored: PersonhoodAnchor = serde_json::from_str(&json).unwrap();

        assert_eq!(pa.anchor.id, restored.anchor.id);
        assert_eq!(pa.status, restored.status);
        assert_eq!(pa.current_key, restored.current_key);
    }

    #[test]
    fn test_anchor_status_display() {
        let status = AnchorStatus::Active;
        assert_eq!(status.to_string(), "active");

        let status = AnchorStatus::Suspended {
            reason: "testing".to_string(),
            suspended_at: 0,
            until: None,
            authority: Did::from_anchor_id(&[1u8; 32]),
        };
        assert!(status.to_string().contains("suspended"));

        let status = AnchorStatus::Revoked {
            reason: "violation".to_string(),
            revoked_at: 0,
            evidence: vec![],
            authority: Did::from_anchor_id(&[1u8; 32]),
        };
        assert!(status.to_string().contains("revoked"));
    }
}
