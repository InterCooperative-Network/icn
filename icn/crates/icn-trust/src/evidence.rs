//! Trust Evidence Types
//!
//! Typed evidence for trust edges, replacing unverified string references.
//! Each evidence type represents a verifiable claim that supports the trust relationship.

use icn_identity::Did;
use serde::{Deserialize, Serialize};

/// Content hash type (32-byte SHA-256)
pub type ContentHash = [u8; 32];

/// Evidence supporting a trust edge
///
/// Each variant represents a different type of verifiable evidence.
/// Unlike the previous `Vec<String>` approach, these can be validated
/// against actual records in the system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TrustEvidence {
    /// Reference to an executed contract between parties
    ///
    /// Validates: Contract exists, both DIDs are parties to it
    ContractExecution {
        /// Hash of the contract content
        contract_id: ContentHash,
        /// Agreement ID if this is part of a federation agreement
        agreement_id: Option<String>,
        /// Execution timestamp
        executed_at: u64,
    },

    /// Reference to a ledger transaction between parties
    ///
    /// Validates: Transaction exists, involves both source and target DIDs
    LedgerTransaction {
        /// DID of the ledger where transaction occurred
        ledger_id: Did,
        /// Hash of the ledger entry
        entry_hash: ContentHash,
        /// Transaction amount (for economic trust weighting)
        amount: Option<i64>,
    },

    /// Reference to shared governance participation
    ///
    /// Validates: Both parties voted on the same proposal
    GovernanceVote {
        /// Cooperative or community ID
        scope_id: String,
        /// Proposal identifier
        proposal_id: String,
        /// Hash of the vote record
        vote_hash: ContentHash,
    },

    /// External attestation from a trusted provider
    ///
    /// Validates: Signature from known attestation provider
    ExternalAttestation {
        /// Provider identifier (e.g., "sdis", "keybase", "github")
        provider: String,
        /// Attestation identifier
        attestation_id: String,
        /// Provider's signature over the attestation
        signature: Vec<u8>,
        /// Optional metadata about what was attested
        metadata: Option<String>,
    },

    /// Peer endorsement from a trusted party
    ///
    /// Validates: Endorser has sufficient trust with us, signature valid
    PeerEndorsement {
        /// DID of the peer providing the endorsement
        endorser: Did,
        /// Endorser's signature over (source, target, timestamp)
        signature: Vec<u8>,
        /// When the endorsement was made
        endorsed_at: u64,
        /// Optional reason for endorsement
        reason: Option<String>,
    },

    /// Technical performance metric observation
    ///
    /// Validates: Observer is trusted, metric is recent
    TechnicalObservation {
        /// DID of the observer who measured the metric
        observer: Did,
        /// Type of observation (uptime, latency, task_success, etc.)
        metric_type: TechnicalMetricType,
        /// Observed value (interpretation depends on metric_type)
        value: f64,
        /// When the observation was made
        observed_at: u64,
        /// Observer's signature over the observation
        signature: Vec<u8>,
    },

    /// Legacy string evidence (for migration from old format)
    ///
    /// These are grandfathered from the previous `Vec<String>` format.
    /// New edges should not use this variant.
    #[serde(rename = "legacy")]
    Legacy {
        /// Original string reference
        reference: String,
        /// When this was migrated
        migrated_at: u64,
    },
}

impl TrustEvidence {
    /// Get the evidence type as a string for metrics/logging
    pub fn evidence_type(&self) -> &'static str {
        match self {
            TrustEvidence::ContractExecution { .. } => "contract_execution",
            TrustEvidence::LedgerTransaction { .. } => "ledger_transaction",
            TrustEvidence::GovernanceVote { .. } => "governance_vote",
            TrustEvidence::ExternalAttestation { .. } => "external_attestation",
            TrustEvidence::PeerEndorsement { .. } => "peer_endorsement",
            TrustEvidence::TechnicalObservation { .. } => "technical_observation",
            TrustEvidence::Legacy { .. } => "legacy",
        }
    }

    /// Check if this is legacy evidence that should be re-attested
    pub fn is_legacy(&self) -> bool {
        matches!(self, TrustEvidence::Legacy { .. })
    }

    /// Get the timestamp associated with this evidence
    pub fn timestamp(&self) -> Option<u64> {
        match self {
            TrustEvidence::ContractExecution { executed_at, .. } => Some(*executed_at),
            TrustEvidence::LedgerTransaction { .. } => None, // No timestamp in ledger evidence
            TrustEvidence::GovernanceVote { .. } => None,
            TrustEvidence::ExternalAttestation { .. } => None,
            TrustEvidence::PeerEndorsement { endorsed_at, .. } => Some(*endorsed_at),
            TrustEvidence::TechnicalObservation { observed_at, .. } => Some(*observed_at),
            TrustEvidence::Legacy { migrated_at, .. } => Some(*migrated_at),
        }
    }

    /// Create a legacy evidence item from a string reference
    ///
    /// Used during migration from the old `Vec<String>` format.
    pub fn from_legacy_string(reference: String) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        TrustEvidence::Legacy {
            reference,
            migrated_at: now,
        }
    }
}

/// Types of technical performance metrics
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TechnicalMetricType {
    /// Node uptime percentage (0.0-1.0)
    Uptime,
    /// Average response latency in milliseconds
    Latency,
    /// Task completion success rate (0.0-1.0)
    TaskSuccessRate,
    /// Storage proof-of-storage success rate (0.0-1.0)
    StorageReliability,
    /// Byzantine protocol compliance (0.0-1.0)
    ProtocolCompliance,
}

impl TechnicalMetricType {
    /// Get the metric type as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            TechnicalMetricType::Uptime => "uptime",
            TechnicalMetricType::Latency => "latency",
            TechnicalMetricType::TaskSuccessRate => "task_success_rate",
            TechnicalMetricType::StorageReliability => "storage_reliability",
            TechnicalMetricType::ProtocolCompliance => "protocol_compliance",
        }
    }
}

/// Error type for evidence validation failures
#[derive(Debug, Clone, thiserror::Error)]
pub enum EvidenceValidationError {
    #[error("Contract not found: {contract_id}")]
    ContractNotFound { contract_id: String },

    #[error("Target DID {target} is not a party to contract {contract_id}")]
    NotContractParty { target: String, contract_id: String },

    #[error("Source DID {source_did} is not a party to contract {contract_id}")]
    SourceNotContractParty {
        source_did: String,
        contract_id: String,
    },

    #[error("Malformed contract data for {contract_id}: {reason}")]
    MalformedContract { contract_id: String, reason: String },

    #[error("Ledger transaction not found: {entry_hash}")]
    TransactionNotFound { entry_hash: String },

    #[error("Transaction does not involve target DID {target}")]
    TransactionNotInvolved { target: String },

    #[error("Governance vote not found: {proposal_id}")]
    VoteNotFound { proposal_id: String },

    #[error("External attestation signature invalid")]
    InvalidAttestationSignature,

    #[error("Unknown attestation provider: {provider}")]
    UnknownProvider { provider: String },

    #[error("Endorser {endorser} signature invalid")]
    InvalidEndorsementSignature { endorser: String },

    #[error("Endorser {endorser} not trusted (score: {score:.2}, required: {required:.2})")]
    EndorserNotTrusted {
        endorser: String,
        score: f64,
        required: f64,
    },

    #[error("Observer {observer} not trusted for technical observations")]
    ObserverNotTrusted { observer: String },

    #[error("Technical observation too old: {age_secs}s (max: {max_secs}s)")]
    ObservationTooOld { age_secs: u64, max_secs: u64 },

    #[error("Legacy evidence not accepted for new edges")]
    LegacyNotAccepted,

    #[error("Evidence expired")]
    EvidenceExpired,

    #[error("Storage error during validation: {details}")]
    StorageError { details: String },
}

/// Result of evidence validation
#[derive(Debug, Clone)]
pub struct EvidenceValidationResult {
    /// Whether the evidence is valid
    pub valid: bool,
    /// Validation errors (if any)
    pub errors: Vec<EvidenceValidationError>,
    /// Suggested trust score adjustment based on evidence strength
    pub score_adjustment: f64,
}

impl EvidenceValidationResult {
    /// Create a successful validation result
    pub fn valid() -> Self {
        Self {
            valid: true,
            errors: Vec::new(),
            score_adjustment: 0.0,
        }
    }

    /// Create a successful validation result with score adjustment
    pub fn valid_with_adjustment(adjustment: f64) -> Self {
        Self {
            valid: true,
            errors: Vec::new(),
            score_adjustment: adjustment,
        }
    }

    /// Create a failed validation result
    pub fn invalid(error: EvidenceValidationError) -> Self {
        Self {
            valid: false,
            errors: vec![error],
            score_adjustment: 0.0,
        }
    }

    /// Create a failed validation result with multiple errors
    pub fn invalid_multiple(errors: Vec<EvidenceValidationError>) -> Self {
        Self {
            valid: false,
            errors,
            score_adjustment: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evidence_type_string() {
        let evidence = TrustEvidence::ContractExecution {
            contract_id: [0u8; 32],
            agreement_id: None,
            executed_at: 12345,
        };
        assert_eq!(evidence.evidence_type(), "contract_execution");
    }

    #[test]
    fn test_legacy_evidence() {
        let evidence = TrustEvidence::from_legacy_string("old_reference_123".to_string());
        assert!(evidence.is_legacy());
        assert!(evidence.timestamp().is_some());
    }

    #[test]
    fn test_evidence_serialization() {
        use icn_identity::KeyPair;

        let keypair = KeyPair::generate().unwrap();
        let evidence = TrustEvidence::LedgerTransaction {
            ledger_id: keypair.did().clone(),
            entry_hash: [1u8; 32],
            amount: Some(1000),
        };

        let json = serde_json::to_string(&evidence).unwrap();
        assert!(json.contains("ledger_transaction"));

        let parsed: TrustEvidence = serde_json::from_str(&json).unwrap();
        assert_eq!(evidence, parsed);
    }

    #[test]
    fn test_technical_metric_type() {
        assert_eq!(TechnicalMetricType::Uptime.as_str(), "uptime");
        assert_eq!(TechnicalMetricType::Latency.as_str(), "latency");
    }

    #[test]
    fn test_validation_result() {
        let valid = EvidenceValidationResult::valid();
        assert!(valid.valid);
        assert!(valid.errors.is_empty());

        let invalid = EvidenceValidationResult::invalid(EvidenceValidationError::LegacyNotAccepted);
        assert!(!invalid.valid);
        assert_eq!(invalid.errors.len(), 1);
    }
}
