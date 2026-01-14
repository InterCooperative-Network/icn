//! Inter-Cooperative Agreement Types
//!
//! Core types for the agreement framework including Agreement, AgreementType,
//! AgreementStatus, and related structures.

use icn_identity::Did;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::current_timestamp;

/// Unique identifier for an agreement
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgreementId(pub String);

impl AgreementId {
    /// Generate a new random agreement ID
    pub fn generate() -> Self {
        Self(format!("agr-{}", Uuid::new_v4()))
    }

    /// Create from an existing ID string
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Get the inner string value
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AgreementId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for AgreementId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Types of inter-cooperative agreements
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgreementType {
    /// Trade agreement for exchange of goods/services
    Trade {
        /// Items being traded
        items: Vec<TradeItem>,
        /// Settlement currency
        currency: String,
    },

    /// Credit agreement establishing a credit line
    Credit {
        /// Maximum credit limit
        credit_limit: i64,
        /// Interest rate in basis points (1 bp = 0.01%)
        interest_rate_bps: u32,
        /// Currency for the credit line
        currency: String,
    },

    /// Resource sharing agreement
    ResourceSharing {
        /// Type of resource being shared
        resource_type: ResourceType,
        /// Duration of the sharing agreement in days
        duration_days: u32,
        /// Compensation model for resource usage
        compensation: CompensationModel,
    },

    /// Federation membership agreement
    FederationMembership {
        /// Federation network identifier
        federation_id: String,
        /// Membership terms
        terms: FederationMembershipTerms,
    },

    /// Custom agreement with free-form terms
    Custom {
        /// Custom agreement type name
        agreement_type_name: String,
        /// JSON-encoded custom terms
        terms_json: String,
    },
}

impl AgreementType {
    /// Get the type name for display/metrics
    pub fn type_name(&self) -> &'static str {
        match self {
            AgreementType::Trade { .. } => "trade",
            AgreementType::Credit { .. } => "credit",
            AgreementType::ResourceSharing { .. } => "resource_sharing",
            AgreementType::FederationMembership { .. } => "federation_membership",
            AgreementType::Custom { .. } => "custom",
        }
    }
}

/// An item in a trade agreement
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TradeItem {
    /// Description of the item
    pub description: String,
    /// Quantity
    pub quantity: u32,
    /// Unit of measurement (e.g., "kg", "hours", "units")
    pub unit: String,
    /// Price per unit
    pub unit_price: i64,
    /// Currency
    pub currency: String,
}

impl TradeItem {
    /// Create a new trade item
    pub fn new(
        description: impl Into<String>,
        quantity: u32,
        unit: impl Into<String>,
        unit_price: i64,
        currency: impl Into<String>,
    ) -> Self {
        Self {
            description: description.into(),
            quantity,
            unit: unit.into(),
            unit_price,
            currency: currency.into(),
        }
    }

    /// Calculate total value
    /// Returns None if multiplication would overflow
    pub fn total_value(&self) -> Result<i64, String> {
        self.unit_price
            .checked_mul(i64::from(self.quantity))
            .ok_or_else(|| {
                format!(
                    "Integer overflow: {} * {} exceeds i64::MAX",
                    self.unit_price, self.quantity
                )
            })
    }
}

/// Types of resources that can be shared
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    /// Physical equipment
    Equipment,
    /// Physical space/facility
    Facility,
    /// Computational resources
    Compute,
    /// Storage resources
    Storage,
    /// Human labor/expertise
    Labor,
    /// Intellectual property/licenses
    IntellectualProperty,
    /// Custom resource type
    Other(String),
}

/// Compensation model for resource sharing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "model", rename_all = "snake_case")]
pub enum CompensationModel {
    /// Fixed periodic payment
    FixedRate {
        /// Amount per period
        amount: i64,
        /// Currency
        currency: String,
        /// Period in days
        period_days: u32,
    },

    /// Pay per use
    PerUse {
        /// Amount per use
        amount: i64,
        /// Currency
        currency: String,
        /// Unit of use measurement
        unit: String,
    },

    /// Revenue sharing
    RevenueShare {
        /// Percentage of revenue (0-100)
        percentage: u8,
    },

    /// Reciprocal exchange (no monetary compensation)
    Reciprocal {
        /// Description of reciprocal arrangement
        description: String,
    },

    /// No compensation (donation/grant)
    None,
}

/// Terms for federation membership
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FederationMembershipTerms {
    /// Required trust threshold for membership
    pub min_trust_threshold: f64,
    /// Whether governance decisions are binding
    pub governance_binding: bool,
    /// Data sharing requirements
    pub data_sharing_level: DataSharingLevel,
    /// Contribution requirements
    pub contribution_requirements: Option<String>,
}

impl Default for FederationMembershipTerms {
    fn default() -> Self {
        Self {
            min_trust_threshold: 0.5,
            governance_binding: true,
            data_sharing_level: DataSharingLevel::MetadataOnly,
            contribution_requirements: None,
        }
    }
}

/// Level of data sharing required
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DataSharingLevel {
    /// No automatic data sharing
    None,
    /// Share metadata only (summaries, not individual transactions)
    #[default]
    MetadataOnly,
    /// Full transparency
    Full,
}

/// State of an agreement in its lifecycle
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AgreementStatus {
    /// Draft agreement, not yet proposed
    Draft,

    /// Proposed and awaiting signatures
    Proposed {
        /// When the agreement was proposed
        proposed_at: u64,
        /// DIDs of parties who still need to sign
        awaiting_signatures: Vec<Did>,
    },

    /// Active agreement
    Active {
        /// When the agreement became active
        activated_at: u64,
        /// Optional expiration timestamp
        expires_at: Option<u64>,
    },

    /// Temporarily suspended
    Suspended {
        /// When the agreement was suspended
        suspended_at: u64,
        /// Reason for suspension
        reason: String,
        /// DID of the party who suspended
        suspended_by: Did,
    },

    /// Permanently terminated
    Terminated {
        /// When the agreement was terminated
        terminated_at: u64,
        /// Reason for termination
        reason: TerminationReason,
    },
}

impl AgreementStatus {
    /// Check if the agreement is in draft state
    pub fn is_draft(&self) -> bool {
        matches!(self, AgreementStatus::Draft)
    }

    /// Check if the agreement is proposed and awaiting signatures
    pub fn is_proposed(&self) -> bool {
        matches!(self, AgreementStatus::Proposed { .. })
    }

    /// Check if the agreement is active
    pub fn is_active(&self) -> bool {
        matches!(self, AgreementStatus::Active { .. })
    }

    /// Check if the agreement is suspended
    pub fn is_suspended(&self) -> bool {
        matches!(self, AgreementStatus::Suspended { .. })
    }

    /// Check if the agreement is terminated
    pub fn is_terminated(&self) -> bool {
        matches!(self, AgreementStatus::Terminated { .. })
    }

    /// Check if the agreement is in a terminal state
    pub fn is_terminal(&self) -> bool {
        self.is_terminated()
    }

    /// Get the expiration timestamp if active
    pub fn expires_at(&self) -> Option<u64> {
        match self {
            AgreementStatus::Active { expires_at, .. } => *expires_at,
            _ => None,
        }
    }

    /// Check if the agreement has expired
    pub fn is_expired(&self, current_time: u64) -> bool {
        match self {
            AgreementStatus::Active {
                expires_at: Some(exp),
                ..
            } => current_time >= *exp,
            _ => false,
        }
    }
}

/// Reason for agreement termination
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "reason_type", rename_all = "snake_case")]
pub enum TerminationReason {
    /// Agreement expired naturally
    Expired,

    /// Mutual consent of all parties
    MutualConsent {
        /// Optional explanation
        explanation: Option<String>,
    },

    /// Breach of terms by a party
    Breach {
        /// DID of the breaching party
        breaching_party: Did,
        /// Description of the breach
        breach_description: String,
    },

    /// Unilateral withdrawal (if permitted by terms)
    Withdrawal {
        /// DID of the withdrawing party
        withdrawing_party: Did,
        /// Notice period observed (in days)
        notice_days: u32,
    },

    /// Force majeure or external circumstances
    ForceMajeure {
        /// Description of the circumstances
        description: String,
    },
}

/// Role of a party in an agreement
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PartyRole {
    /// The party who initiated/proposed the agreement
    Proposer,
    /// A counterparty to the agreement
    Counterparty,
    /// A witness to the agreement (doesn't sign, but validates)
    Witness,
    /// A guarantor backing the agreement
    Guarantor,
}

impl PartyRole {
    /// Check if this role requires a signature
    pub fn requires_signature(&self) -> bool {
        matches!(
            self,
            PartyRole::Proposer | PartyRole::Counterparty | PartyRole::Guarantor
        )
    }
}

/// A party to an agreement
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgreementParty {
    /// DID of the party
    pub did: Did,
    /// Cooperative/organization ID
    pub coop_id: String,
    /// Role in the agreement
    pub role: PartyRole,
    /// Display name (optional)
    pub display_name: Option<String>,
}

impl AgreementParty {
    /// Create a new agreement party
    pub fn new(did: Did, coop_id: impl Into<String>, role: PartyRole) -> Self {
        Self {
            did,
            coop_id: coop_id.into(),
            role,
            display_name: None,
        }
    }

    /// Add a display name
    pub fn with_display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }
}

/// A signature on an agreement
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgreementSignature {
    /// DID of the signer
    pub signer: Did,
    /// Cooperative ID of the signer
    pub coop_id: String,
    /// Ed25519 signature bytes
    pub signature: Vec<u8>,
    /// When the signature was created
    pub signed_at: u64,
    /// Agreement version that was signed
    pub version_signed: u32,
}

impl AgreementSignature {
    /// Create a new signature
    pub fn new(
        signer: Did,
        coop_id: impl Into<String>,
        signature: Vec<u8>,
        version_signed: u32,
    ) -> Self {
        Self {
            signer,
            coop_id: coop_id.into(),
            signature,
            signed_at: current_timestamp(),
            version_signed,
        }
    }
}

/// Terms and conditions of an agreement
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AgreementTerms {
    /// Effective date (when the agreement takes effect)
    pub effective_date: Option<u64>,
    /// Expiration date
    pub expiration_date: Option<u64>,
    /// Auto-renewal settings
    pub auto_renewal: Option<AutoRenewal>,
    /// Notice period for termination (in days)
    pub termination_notice_days: Option<u32>,
    /// Dispute resolution method
    pub dispute_resolution: Option<String>,
    /// Governing law/jurisdiction
    pub governing_law: Option<String>,
    /// Additional terms as free-form text
    pub additional_terms: Option<String>,
}

impl AgreementTerms {
    /// Create a new empty terms object
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the effective date
    pub fn with_effective_date(mut self, date: u64) -> Self {
        self.effective_date = Some(date);
        self
    }

    /// Set the expiration date
    pub fn with_expiration_date(mut self, date: u64) -> Self {
        self.expiration_date = Some(date);
        self
    }

    /// Set auto-renewal
    pub fn with_auto_renewal(mut self, renewal: AutoRenewal) -> Self {
        self.auto_renewal = Some(renewal);
        self
    }

    /// Set termination notice period
    pub fn with_termination_notice(mut self, days: u32) -> Self {
        self.termination_notice_days = Some(days);
        self
    }

    /// Set dispute resolution method
    pub fn with_dispute_resolution(mut self, method: impl Into<String>) -> Self {
        self.dispute_resolution = Some(method.into());
        self
    }
}

/// Auto-renewal configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutoRenewal {
    /// Whether auto-renewal is enabled
    pub enabled: bool,
    /// Renewal period in days
    pub period_days: u32,
    /// Maximum number of renewals (None = unlimited)
    pub max_renewals: Option<u32>,
    /// Days before expiration to notify parties
    pub notification_days: u32,
}

impl Default for AutoRenewal {
    fn default() -> Self {
        Self {
            enabled: false,
            period_days: 365,
            max_renewals: None,
            notification_days: 30,
        }
    }
}

/// An amendment to an existing agreement
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Amendment {
    /// Unique amendment ID
    pub id: String,
    /// Agreement being amended
    pub agreement_id: AgreementId,
    /// Description of the amendment
    pub description: String,
    /// Changes being made
    pub changes: Vec<AmendmentChange>,
    /// Signatures on the amendment
    pub signatures: Vec<AgreementSignature>,
    /// Amendment status
    pub status: AmendmentStatus,
    /// When the amendment was proposed
    pub proposed_at: u64,
    /// DID of the proposer
    pub proposed_by: Did,
}

impl Amendment {
    /// Generate a new amendment ID
    pub fn generate_id() -> String {
        format!("amd-{}", Uuid::new_v4())
    }

    /// Create a new amendment
    pub fn new(
        agreement_id: AgreementId,
        description: impl Into<String>,
        proposed_by: Did,
    ) -> Self {
        Self {
            id: Self::generate_id(),
            agreement_id,
            description: description.into(),
            changes: Vec::new(),
            signatures: Vec::new(),
            status: AmendmentStatus::Proposed,
            proposed_at: current_timestamp(),
            proposed_by,
        }
    }

    /// Add a change to the amendment
    pub fn with_change(mut self, change: AmendmentChange) -> Self {
        self.changes.push(change);
        self
    }

    /// Check if all required parties have signed
    pub fn is_fully_signed(&self, required_signers: &[Did]) -> bool {
        required_signers
            .iter()
            .all(|did| self.signatures.iter().any(|sig| sig.signer == *did))
    }

    /// Add a signature
    pub fn add_signature(&mut self, signature: AgreementSignature) {
        // Don't add duplicate signatures
        if !self.signatures.iter().any(|s| s.signer == signature.signer) {
            self.signatures.push(signature);
        }
    }

    /// Verify a single signature for this amendment
    pub fn verify_signature(&self, sig: &AgreementSignature) -> Result<(), String> {
        // Get the amendment bytes to verify
        let bytes = self.signing_bytes();

        // Get the public key from the DID
        let public_key = sig
            .signer
            .to_verifying_key()
            .map_err(|e| format!("Invalid public key for {}: {}", sig.signer, e))?;

        // Parse the signature
        let signature = ed25519_dalek::Signature::from_slice(&sig.signature)
            .map_err(|e| format!("Invalid signature format: {}", e))?;

        // Verify
        use ed25519_dalek::Verifier;
        public_key
            .verify(&bytes, &signature)
            .map_err(|e| format!("Signature verification failed for {}: {}", sig.signer, e))
    }

    /// Get the canonical bytes for signing
    fn signing_bytes(&self) -> Vec<u8> {
        // Use a deterministic encoding
        format!(
            "{}:{}:{}:{}",
            self.id, self.agreement_id, self.description, self.proposed_at
        )
        .into_bytes()
    }
}

/// A specific change in an amendment
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "change_type", rename_all = "snake_case")]
pub enum AmendmentChange {
    /// Update a term
    UpdateTerm {
        /// Field being updated
        field: String,
        /// Old value (JSON-encoded)
        old_value: String,
        /// New value (JSON-encoded)
        new_value: String,
    },

    /// Add a new party
    AddParty {
        /// The party being added
        party: AgreementParty,
    },

    /// Remove a party
    RemoveParty {
        /// DID of the party being removed
        party_did: Did,
    },

    /// Extend the agreement duration
    ExtendDuration {
        /// New expiration date
        new_expiration: u64,
    },

    /// Modify the agreement type details
    ModifyType {
        /// New agreement type configuration (JSON-encoded)
        new_type_json: String,
    },
}

/// Status of an amendment
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AmendmentStatus {
    /// Proposed, awaiting signatures
    Proposed,
    /// Ratified (all parties signed)
    Ratified,
    /// Rejected by one or more parties
    Rejected,
    /// Withdrawn by the proposer
    Withdrawn,
}

/// The core agreement structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agreement {
    /// Unique agreement identifier
    pub id: AgreementId,
    /// Agreement title
    pub title: String,
    /// Detailed description
    pub description: String,
    /// Type of agreement
    pub agreement_type: AgreementType,
    /// Parties to the agreement
    pub parties: Vec<AgreementParty>,
    /// Current status
    pub status: AgreementStatus,
    /// Terms and conditions
    pub terms: AgreementTerms,
    /// Collected signatures
    pub signatures: Vec<AgreementSignature>,
    /// Amendments history
    pub amendments: Vec<Amendment>,
    /// Current version (increments with each amendment)
    pub version: u32,
    /// When the agreement was created
    pub created_at: u64,
    /// When the agreement was last updated
    pub updated_at: u64,
}

impl Agreement {
    /// Create a new draft agreement
    pub fn new(
        title: impl Into<String>,
        description: impl Into<String>,
        agreement_type: AgreementType,
    ) -> Self {
        let now = current_timestamp();
        Self {
            id: AgreementId::generate(),
            title: title.into(),
            description: description.into(),
            agreement_type,
            parties: Vec::new(),
            status: AgreementStatus::Draft,
            terms: AgreementTerms::default(),
            signatures: Vec::new(),
            amendments: Vec::new(),
            version: 1,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create with a specific ID (for testing or restoration)
    pub fn with_id(mut self, id: AgreementId) -> Self {
        self.id = id;
        self
    }

    /// Add a party to the agreement
    pub fn with_party(mut self, did: Did, coop_id: impl Into<String>, role: PartyRole) -> Self {
        self.parties.push(AgreementParty::new(did, coop_id, role));
        self.updated_at = current_timestamp();
        self
    }

    /// Set the agreement terms
    pub fn with_terms(mut self, terms: AgreementTerms) -> Self {
        self.terms = terms;
        self.updated_at = current_timestamp();
        self
    }

    /// Get the proposer (if any)
    pub fn proposer(&self) -> Option<&AgreementParty> {
        self.parties.iter().find(|p| p.role == PartyRole::Proposer)
    }

    /// Get all counterparties
    pub fn counterparties(&self) -> Vec<&AgreementParty> {
        self.parties
            .iter()
            .filter(|p| p.role == PartyRole::Counterparty)
            .collect()
    }

    /// Get all parties that need to sign
    pub fn required_signers(&self) -> Vec<&Did> {
        self.parties
            .iter()
            .filter(|p| p.role.requires_signature())
            .map(|p| &p.did)
            .collect()
    }

    /// Check if a specific party has signed
    pub fn has_signed(&self, did: &Did) -> bool {
        self.signatures.iter().any(|s| &s.signer == did)
    }

    /// Check if all required parties have signed
    pub fn is_fully_signed(&self) -> bool {
        self.required_signers()
            .iter()
            .all(|did| self.has_signed(did))
    }

    /// Get the DIDs of parties who still need to sign
    pub fn pending_signers(&self) -> Vec<Did> {
        self.required_signers()
            .into_iter()
            .filter(|did| !self.has_signed(did))
            .cloned()
            .collect()
    }

    /// Get the canonical bytes for signing
    ///
    /// This creates a deterministic byte representation of the agreement
    /// excluding the signatures, so all parties sign the same content.
    pub fn signing_bytes(&self) -> Vec<u8> {
        // Create a signable version without signatures
        let signable = SignableAgreement {
            id: &self.id,
            title: &self.title,
            description: &self.description,
            agreement_type: &self.agreement_type,
            parties: &self.parties,
            terms: &self.terms,
            version: self.version,
            created_at: self.created_at,
        };

        serde_json::to_vec(&signable).unwrap_or_default()
    }

    /// Sign the agreement
    pub fn sign(&mut self, keypair: &icn_identity::KeyPair, coop_id: impl Into<String>) {
        let bytes = self.signing_bytes();
        let signature_bytes = keypair.sign(&bytes);

        let signature = AgreementSignature::new(
            keypair.did().clone(),
            coop_id,
            signature_bytes.to_vec(),
            self.version,
        );

        // Don't add duplicate signatures
        if !self.has_signed(&signature.signer) {
            self.signatures.push(signature);
            self.updated_at = current_timestamp();
        }
    }

    /// Verify all signatures on the agreement
    pub fn verify_signatures(&self) -> Result<(), String> {
        let bytes = self.signing_bytes();

        for sig in &self.signatures {
            // Find the party with this DID
            let party = self.parties.iter().find(|p| p.did == sig.signer);
            if party.is_none() {
                return Err(format!(
                    "Signer {} is not a party to this agreement",
                    sig.signer
                ));
            }

            // Verify the signature
            let public_key = sig
                .signer
                .to_verifying_key()
                .map_err(|e| format!("Invalid public key for {}: {e}", sig.signer))?;

            let signature = ed25519_dalek::Signature::from_slice(&sig.signature)
                .map_err(|e| format!("Invalid signature format: {e}"))?;

            use ed25519_dalek::Verifier;
            public_key
                .verify(&bytes, &signature)
                .map_err(|e| format!("Signature verification failed for {}: {e}", sig.signer))?;
        }

        Ok(())
    }

    /// Propose the agreement (transition from Draft to Proposed)
    pub fn propose(&mut self) -> Result<(), String> {
        if !self.status.is_draft() {
            return Err("Can only propose agreements in Draft status".to_string());
        }

        if self.parties.len() < 2 {
            return Err("Agreement must have at least 2 parties".to_string());
        }

        if self.proposer().is_none() {
            return Err("Agreement must have a proposer".to_string());
        }

        // Validate timestamps
        let now = current_timestamp();
        if let Some(effective) = self.terms.effective_date {
            if effective < now {
                return Err("Effective date must be in the future".to_string());
            }
            if let Some(expiration) = self.terms.expiration_date {
                if expiration <= effective {
                    return Err("Expiration date must be after effective date".to_string());
                }
            }
        }

        let awaiting = self.pending_signers();
        self.status = AgreementStatus::Proposed {
            proposed_at: current_timestamp(),
            awaiting_signatures: awaiting,
        };
        self.updated_at = current_timestamp();

        Ok(())
    }

    /// Activate the agreement (transition from Proposed to Active)
    pub fn activate(&mut self) -> Result<(), String> {
        if !self.status.is_proposed() {
            return Err("Can only activate agreements in Proposed status".to_string());
        }

        if !self.is_fully_signed() {
            let pending: Vec<_> = self
                .pending_signers()
                .iter()
                .map(|d| d.to_string())
                .collect();
            return Err(format!(
                "Not all parties have signed. Pending: {}",
                pending.join(", ")
            ));
        }

        // Verify all signatures before activation
        self.verify_signatures()?;

        let expires_at = self.terms.expiration_date;
        self.status = AgreementStatus::Active {
            activated_at: current_timestamp(),
            expires_at,
        };
        self.updated_at = current_timestamp();

        Ok(())
    }

    /// Suspend the agreement (transition from Active to Suspended)
    pub fn suspend(&mut self, reason: impl Into<String>, suspended_by: Did) -> Result<(), String> {
        if !self.status.is_active() {
            return Err("Can only suspend agreements in Active status".to_string());
        }

        // Verify the suspender is a party
        if !self.parties.iter().any(|p| p.did == suspended_by) {
            return Err("Only parties to the agreement can suspend it".to_string());
        }

        self.status = AgreementStatus::Suspended {
            suspended_at: current_timestamp(),
            reason: reason.into(),
            suspended_by,
        };
        self.updated_at = current_timestamp();

        Ok(())
    }

    /// Resume the agreement (transition from Suspended to Active)
    pub fn resume(&mut self, resumed_by: &Did) -> Result<(), String> {
        match &self.status {
            AgreementStatus::Suspended { suspended_by, .. } => {
                // Only the party who suspended can resume
                if resumed_by != suspended_by {
                    return Err("Only the party who suspended can resume".to_string());
                }
            }
            _ => return Err("Can only resume agreements in Suspended status".to_string()),
        }

        let expires_at = self.terms.expiration_date;
        self.status = AgreementStatus::Active {
            activated_at: current_timestamp(),
            expires_at,
        };
        self.updated_at = current_timestamp();

        Ok(())
    }

    /// Terminate the agreement
    pub fn terminate(&mut self, reason: TerminationReason) -> Result<(), String> {
        if self.status.is_terminated() {
            return Err("Agreement is already terminated".to_string());
        }

        if self.status.is_draft() {
            return Err("Cannot terminate a draft agreement. Delete it instead.".to_string());
        }

        self.status = AgreementStatus::Terminated {
            terminated_at: current_timestamp(),
            reason,
        };
        self.updated_at = current_timestamp();

        Ok(())
    }

    /// Reject the agreement (transition from Proposed to Draft)
    pub fn reject(&mut self) -> Result<(), String> {
        if !self.status.is_proposed() {
            return Err("Can only reject agreements in Proposed status".to_string());
        }

        // Clear signatures and return to draft
        self.signatures.clear();
        self.status = AgreementStatus::Draft;
        self.updated_at = current_timestamp();

        Ok(())
    }

    /// Check if the agreement should be auto-activated
    /// (call this after adding a signature)
    pub fn try_auto_activate(&mut self) -> bool {
        if self.status.is_proposed() && self.is_fully_signed() {
            self.activate().is_ok()
        } else {
            false
        }
    }

    /// Apply a ratified amendment
    pub fn apply_amendment(&mut self, amendment: Amendment) -> Result<(), String> {
        if amendment.status != AmendmentStatus::Ratified {
            return Err("Can only apply ratified amendments".to_string());
        }

        if amendment.agreement_id != self.id {
            return Err("Amendment is for a different agreement".to_string());
        }

        // Apply each change
        for change in &amendment.changes {
            match change {
                AmendmentChange::ExtendDuration { new_expiration } => {
                    self.terms.expiration_date = Some(*new_expiration);
                    if let AgreementStatus::Active { activated_at, .. } = self.status {
                        self.status = AgreementStatus::Active {
                            activated_at,
                            expires_at: Some(*new_expiration),
                        };
                    }
                }
                AmendmentChange::AddParty { party } => {
                    if !self.parties.iter().any(|p| p.did == party.did) {
                        self.parties.push(party.clone());
                    }
                }
                AmendmentChange::RemoveParty { party_did } => {
                    self.parties.retain(|p| p.did != *party_did);
                    self.signatures.retain(|s| s.signer != *party_did);
                }
                AmendmentChange::UpdateTerm { .. } | AmendmentChange::ModifyType { .. } => {
                    return Err(format!(
                        "Amendment change type {:?} not yet supported",
                        change
                    ));
                }
            }
        }

        self.version += 1;
        self.amendments.push(amendment);
        self.updated_at = current_timestamp();

        Ok(())
    }
}

/// Helper struct for creating signing bytes
#[derive(Serialize)]
struct SignableAgreement<'a> {
    id: &'a AgreementId,
    title: &'a str,
    description: &'a str,
    agreement_type: &'a AgreementType,
    parties: &'a [AgreementParty],
    terms: &'a AgreementTerms,
    version: u32,
    created_at: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    #[test]
    fn test_agreement_id_generation() {
        let id1 = AgreementId::generate();
        let id2 = AgreementId::generate();

        assert!(id1.0.starts_with("agr-"));
        assert!(id2.0.starts_with("agr-"));
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_trade_agreement_creation() {
        let proposer = test_did();
        let counterparty = test_did();

        let agreement = Agreement::new(
            "Produce Exchange",
            "Monthly exchange of organic vegetables",
            AgreementType::Trade {
                items: vec![TradeItem::new("vegetables", 100, "kg", 5, "USD")],
                currency: "USD".to_string(),
            },
        )
        .with_party(proposer.clone(), "food-coop", PartyRole::Proposer)
        .with_party(counterparty.clone(), "tech-coop", PartyRole::Counterparty);

        assert_eq!(agreement.title, "Produce Exchange");
        assert!(agreement.status.is_draft());
        assert_eq!(agreement.parties.len(), 2);
        assert_eq!(agreement.version, 1);
    }

    #[test]
    fn test_agreement_lifecycle() {
        let proposer_kp = KeyPair::generate().unwrap();
        let counterparty_kp = KeyPair::generate().unwrap();

        let mut agreement = Agreement::new(
            "Test Agreement",
            "A test agreement",
            AgreementType::Credit {
                credit_limit: 10000,
                interest_rate_bps: 500,
                currency: "USD".to_string(),
            },
        )
        .with_party(proposer_kp.did().clone(), "coop-a", PartyRole::Proposer)
        .with_party(
            counterparty_kp.did().clone(),
            "coop-b",
            PartyRole::Counterparty,
        );

        // Draft -> Proposed
        assert!(agreement.propose().is_ok());
        assert!(agreement.status.is_proposed());

        // Sign by proposer
        agreement.sign(&proposer_kp, "coop-a");
        assert!(agreement.has_signed(proposer_kp.did()));
        assert!(!agreement.is_fully_signed());

        // Sign by counterparty
        agreement.sign(&counterparty_kp, "coop-b");
        assert!(agreement.is_fully_signed());

        // Proposed -> Active
        assert!(agreement.activate().is_ok());
        assert!(agreement.status.is_active());

        // Active -> Suspended
        assert!(agreement
            .suspend("Need review", proposer_kp.did().clone())
            .is_ok());
        assert!(agreement.status.is_suspended());

        // Suspended -> Active
        assert!(agreement.resume(proposer_kp.did()).is_ok());
        assert!(agreement.status.is_active());

        // Active -> Terminated
        assert!(agreement
            .terminate(TerminationReason::MutualConsent {
                explanation: Some("Agreement fulfilled".to_string()),
            })
            .is_ok());
        assert!(agreement.status.is_terminated());
    }

    #[test]
    fn test_signature_verification() {
        let proposer_kp = KeyPair::generate().unwrap();
        let counterparty_kp = KeyPair::generate().unwrap();

        let mut agreement = Agreement::new(
            "Signature Test",
            "Testing signature verification",
            AgreementType::Custom {
                agreement_type_name: "test".to_string(),
                terms_json: "{}".to_string(),
            },
        )
        .with_party(proposer_kp.did().clone(), "coop-a", PartyRole::Proposer)
        .with_party(
            counterparty_kp.did().clone(),
            "coop-b",
            PartyRole::Counterparty,
        );

        agreement.sign(&proposer_kp, "coop-a");
        agreement.sign(&counterparty_kp, "coop-b");

        assert!(agreement.verify_signatures().is_ok());
    }

    #[test]
    fn test_required_signers() {
        let proposer = test_did();
        let counterparty = test_did();
        let witness = test_did();

        let agreement = Agreement::new(
            "Test",
            "Test",
            AgreementType::Custom {
                agreement_type_name: "test".to_string(),
                terms_json: "{}".to_string(),
            },
        )
        .with_party(proposer.clone(), "coop-a", PartyRole::Proposer)
        .with_party(counterparty.clone(), "coop-b", PartyRole::Counterparty)
        .with_party(witness.clone(), "coop-c", PartyRole::Witness);

        let required = agreement.required_signers();
        assert_eq!(required.len(), 2); // Proposer and Counterparty
        assert!(required.contains(&&proposer));
        assert!(required.contains(&&counterparty));
        assert!(!required.contains(&&witness)); // Witness doesn't sign
    }

    #[test]
    fn test_auto_activate() {
        let proposer_kp = KeyPair::generate().unwrap();
        let counterparty_kp = KeyPair::generate().unwrap();

        let mut agreement = Agreement::new(
            "Auto Activate Test",
            "Test",
            AgreementType::Credit {
                credit_limit: 5000,
                interest_rate_bps: 300,
                currency: "USD".to_string(),
            },
        )
        .with_party(proposer_kp.did().clone(), "coop-a", PartyRole::Proposer)
        .with_party(
            counterparty_kp.did().clone(),
            "coop-b",
            PartyRole::Counterparty,
        );

        agreement.propose().unwrap();

        agreement.sign(&proposer_kp, "coop-a");
        assert!(!agreement.try_auto_activate()); // Not all signed yet

        agreement.sign(&counterparty_kp, "coop-b");
        assert!(agreement.try_auto_activate()); // Should activate now
        assert!(agreement.status.is_active());
    }

    #[test]
    fn test_trade_item_total_value() {
        let item = TradeItem::new("widgets", 10, "units", 100, "USD");
        assert_eq!(item.total_value().unwrap(), 1000);

        // Test overflow
        let overflow_item = TradeItem::new("widgets", 1_000_000, "units", i64::MAX / 100, "USD");
        assert!(overflow_item.total_value().is_err());
    }

    #[test]
    fn test_agreement_type_names() {
        assert_eq!(
            AgreementType::Trade {
                items: vec![],
                currency: "USD".to_string()
            }
            .type_name(),
            "trade"
        );
        assert_eq!(
            AgreementType::Credit {
                credit_limit: 0,
                interest_rate_bps: 0,
                currency: "USD".to_string()
            }
            .type_name(),
            "credit"
        );
        assert_eq!(
            AgreementType::ResourceSharing {
                resource_type: ResourceType::Equipment,
                duration_days: 30,
                compensation: CompensationModel::None,
            }
            .type_name(),
            "resource_sharing"
        );
    }

    #[test]
    fn test_amendment_workflow() {
        let proposer_kp = KeyPair::generate().unwrap();
        let counterparty_kp = KeyPair::generate().unwrap();

        // Create and activate an agreement
        let mut agreement = Agreement::new(
            "Amendment Test",
            "Testing amendments",
            AgreementType::Credit {
                credit_limit: 5000,
                interest_rate_bps: 300,
                currency: "USD".to_string(),
            },
        )
        .with_party(proposer_kp.did().clone(), "coop-a", PartyRole::Proposer)
        .with_party(
            counterparty_kp.did().clone(),
            "coop-b",
            PartyRole::Counterparty,
        );

        agreement.propose().unwrap();
        agreement.sign(&proposer_kp, "coop-a");
        agreement.sign(&counterparty_kp, "coop-b");
        agreement.activate().unwrap();

        assert_eq!(agreement.version, 1);

        // Create an amendment
        let mut amendment = Amendment::new(
            agreement.id.clone(),
            "Extend agreement duration",
            proposer_kp.did().clone(),
        )
        .with_change(AmendmentChange::ExtendDuration {
            new_expiration: current_timestamp() + 365 * 24 * 3600,
        });

        // Sign the amendment
        let sig1 = AgreementSignature::new(
            proposer_kp.did().clone(),
            "coop-a",
            vec![1, 2, 3], // Mock signature
            1,
        );
        let sig2 = AgreementSignature::new(
            counterparty_kp.did().clone(),
            "coop-b",
            vec![4, 5, 6], // Mock signature
            1,
        );

        amendment.add_signature(sig1);
        amendment.add_signature(sig2);
        amendment.status = AmendmentStatus::Ratified;

        // Apply the amendment
        assert!(agreement.apply_amendment(amendment).is_ok());
        assert_eq!(agreement.version, 2);
        assert_eq!(agreement.amendments.len(), 1);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let proposer = test_did();
        let counterparty = test_did();

        let agreement = Agreement::new(
            "Serialization Test",
            "Testing JSON roundtrip",
            AgreementType::Trade {
                items: vec![TradeItem::new("item", 5, "units", 100, "USD")],
                currency: "USD".to_string(),
            },
        )
        .with_party(proposer, "coop-a", PartyRole::Proposer)
        .with_party(counterparty, "coop-b", PartyRole::Counterparty)
        .with_terms(
            AgreementTerms::new()
                .with_termination_notice(30)
                .with_dispute_resolution("mediation"),
        );

        let json = serde_json::to_string(&agreement).unwrap();
        let deserialized: Agreement = serde_json::from_str(&json).unwrap();

        assert_eq!(agreement.id, deserialized.id);
        assert_eq!(agreement.title, deserialized.title);
        assert_eq!(agreement.parties.len(), deserialized.parties.len());
    }

    #[test]
    fn test_party_role_requires_signature() {
        assert!(PartyRole::Proposer.requires_signature());
        assert!(PartyRole::Counterparty.requires_signature());
        assert!(PartyRole::Guarantor.requires_signature());
        assert!(!PartyRole::Witness.requires_signature());
    }

    #[test]
    fn test_cannot_propose_with_insufficient_parties() {
        let proposer = test_did();

        let mut agreement = Agreement::new(
            "Single Party",
            "Only one party",
            AgreementType::Custom {
                agreement_type_name: "test".to_string(),
                terms_json: "{}".to_string(),
            },
        )
        .with_party(proposer, "coop-a", PartyRole::Proposer);

        assert!(agreement.propose().is_err());
    }

    #[test]
    fn test_status_checks() {
        let status = AgreementStatus::Active {
            activated_at: 1000,
            expires_at: Some(2000),
        };

        assert!(status.is_active());
        assert!(!status.is_draft());
        assert!(!status.is_terminated());
        assert_eq!(status.expires_at(), Some(2000));
        assert!(!status.is_expired(1500));
        assert!(status.is_expired(2500));
    }
}
