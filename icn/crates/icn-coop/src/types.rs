use chrono::{DateTime, Utc};
use icn_governance::charter::CharterId;
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique identifier for a cooperative
pub type CooperativeId = String;

/// Stored founder signature (serialization-friendly version)
///
/// This is a local type that stores DID as String for better bincode compatibility.
/// Use `from_governance` and `to_governance` for conversion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredFounderSignature {
    /// Founder's DID (stored as string for serialization)
    pub did: String,
    /// Ed25519 signature over charter content
    pub signature: Vec<u8>,
    /// When signature was made (Unix timestamp)
    pub timestamp: u64,
    /// Optional role in founding (e.g., "initiator", "steward")
    pub role: Option<String>,
}

impl StoredFounderSignature {
    /// Create from icn_governance FounderSignature
    pub fn from_governance(sig: &icn_governance::charter::FounderSignature) -> Self {
        Self {
            did: sig.did.to_string(),
            signature: sig.signature.clone(),
            timestamp: sig.timestamp,
            role: sig.role.clone(),
        }
    }

    /// Convert to icn_governance FounderSignature
    pub fn to_governance(&self) -> Result<icn_governance::charter::FounderSignature, String> {
        let did: Did = self.did.parse().map_err(|e| format!("Invalid DID: {e}"))?;
        Ok(icn_governance::charter::FounderSignature {
            did,
            signature: self.signature.clone(),
            timestamp: self.timestamp,
            role: self.role.clone(),
        })
    }

    /// Get the DID (parsing from string)
    pub fn did(&self) -> Result<Did, String> {
        self.did.parse().map_err(|e| format!("Invalid DID: {e}"))
    }
}

/// Formation request for creating a new cooperative
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormationRequest {
    /// Proposed cooperative name
    pub name: String,
    /// Type of cooperative
    pub coop_type: CoopType,
    /// Minimum members required (defaults to 3)
    pub min_members: usize,
    /// Founding members who must sign the charter
    pub founding_members: Vec<Did>,
    /// Optional charter document hash (off-chain document)
    pub charter_document_hash: Option<[u8; 32]>,
    /// Optional description
    pub description: Option<String>,
    /// Currency for economic transactions (defaults to "hours")
    pub currency: Option<String>,
}

impl FormationRequest {
    pub fn new(name: String, coop_type: CoopType, founding_members: Vec<Did>) -> Self {
        Self {
            name,
            coop_type,
            min_members: 3,
            founding_members,
            charter_document_hash: None,
            description: None,
            currency: None,
        }
    }

    pub fn with_min_members(mut self, min: usize) -> Self {
        self.min_members = min;
        self
    }

    pub fn with_charter_hash(mut self, hash: [u8; 32]) -> Self {
        self.charter_document_hash = Some(hash);
        self
    }

    pub fn with_description(mut self, desc: String) -> Self {
        self.description = Some(desc);
        self
    }

    pub fn with_currency(mut self, currency: String) -> Self {
        self.currency = Some(currency);
        self
    }
}

/// Dissolution request for dissolving a cooperative
///
/// This type is intended for use in Gateway API endpoints and external
/// integrations. Internal dissolution uses the `start_dissolution` handle
/// method with individual parameters.
///
/// The `reason` field is stored in the cooperative's metadata for audit purposes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DissolutionRequest {
    /// Cooperative to dissolve
    pub coop_id: String,
    /// DID of the member initiating dissolution
    pub initiator: Did,
    /// Reason for dissolution (stored in metadata for audit trail)
    pub reason: String,
    /// Asset distribution plan
    pub asset_distribution: AssetDistributionPlan,
}

/// Plan for distributing assets during dissolution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetDistributionPlan {
    /// How to handle positive balances
    pub positive_balance_action: BalanceAction,
    /// How to handle negative balances (debts)
    pub negative_balance_action: DebtAction,
    /// Capital return method
    pub capital_return: CapitalReturnMethod,
    /// Target entity for remaining assets (e.g., federation treasury)
    pub residual_recipient: Option<String>,
}

impl Default for AssetDistributionPlan {
    fn default() -> Self {
        Self {
            positive_balance_action: BalanceAction::ReturnToMember,
            negative_balance_action: DebtAction::WriteOff,
            capital_return: CapitalReturnMethod::ProRata,
            residual_recipient: None,
        }
    }
}

/// Action for positive balances during dissolution
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BalanceAction {
    /// Return balance to member
    ReturnToMember,
    /// Transfer to federation
    TransferToFederation,
    /// Donate to commons
    DonateToCommons,
}

/// Action for debts (negative balances) during dissolution
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DebtAction {
    /// Write off all debts
    WriteOff,
    /// Require settlement before dissolution
    RequireSettlement,
    /// Transfer debt to federation
    TransferToFederation,
}

/// Method for returning capital contributions
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CapitalReturnMethod {
    /// Return proportional to contribution
    ProRata,
    /// Equal distribution regardless of contribution
    Equal,
    /// No capital return (donated to residual recipient)
    NoReturn,
}

/// Membership tier with associated rights and responsibilities
///
/// Tiers allow cooperatives to define different levels of membership
/// with varying voting weights, profit shares, and governance rights.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MembershipTier {
    /// Name of the tier (e.g., "Worker", "Consumer", "Patron")
    pub name: String,
    /// Voting weight for governance decisions
    pub voting_weight: u32,
    /// Weight for profit/surplus distribution
    pub profit_share_weight: u32,
    /// List of governance rights (e.g., "vote", "propose", "elect_board")
    pub governance_rights: Vec<String>,
}

impl MembershipTier {
    /// Create a standard member tier with equal voting and profit share
    pub fn standard(name: &str) -> Self {
        Self {
            name: name.to_string(),
            voting_weight: 1,
            profit_share_weight: 1,
            governance_rights: vec!["vote".to_string()],
        }
    }

    /// Create a founder tier with enhanced rights
    pub fn founder() -> Self {
        Self {
            name: "Founder".to_string(),
            voting_weight: 1,
            profit_share_weight: 1,
            governance_rights: vec![
                "vote".to_string(),
                "propose".to_string(),
                "elect_board".to_string(),
                "amend_bylaws".to_string(),
            ],
        }
    }
}

impl Default for MembershipTier {
    fn default() -> Self {
        Self::standard("Member")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cooperative {
    pub id: String,
    pub name: String,
    pub coop_type: CoopType,
    pub status: CoopStatus,
    pub domain_id: Option<String>,
    pub charter_hash: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: HashMap<String, String>,

    // === Fields from icn-cooperative for richer governance ===
    /// Minimum members required for the cooperative to operate
    #[serde(default = "default_min_members")]
    pub min_members: usize,

    /// Available membership tiers
    #[serde(default)]
    pub tiers: Vec<MembershipTier>,

    /// Total capital pool contributed by members
    #[serde(default)]
    pub capital_pool: u64,

    /// Bylaws and governing documents (CCL contract IDs or hashes)
    #[serde(default)]
    pub bylaws: Vec<String>,

    // === Charter and governance fields (Issue #290) ===
    /// Charter ID linking to icn-governance Charter
    #[serde(default)]
    pub charter_id: Option<CharterId>,

    /// Founding signatures collected during formation
    /// Uses StoredFounderSignature for bincode compatibility
    #[serde(default)]
    pub founding_signatures: Vec<StoredFounderSignature>,

    /// Required number of founder signatures for activation
    #[serde(default = "default_min_founders")]
    pub min_founders: usize,

    /// List of DIDs authorized to sign the charter as founders
    /// Only DIDs in this list can add founding signatures
    #[serde(default)]
    pub authorized_founders: Vec<String>,

    /// Whether all required founders have signed
    #[serde(default)]
    pub charter_ratified: bool,

    /// Optional description of the cooperative
    #[serde(default)]
    pub description: Option<String>,

    /// Currency for economic transactions (e.g., "hours", "credits")
    #[serde(default)]
    pub currency: Option<String>,

    /// Governance domain ID for proposals (e.g., "coop:my-coop")
    #[serde(default)]
    pub governance_domain: Option<String>,

    /// Asset distribution plan (set during dissolution)
    #[serde(default)]
    pub dissolution_plan: Option<AssetDistributionPlan>,

    /// Governance proposal ID that approved dissolution (if applicable)
    #[serde(default)]
    pub dissolution_proposal_id: Option<String>,

    /// Treasury DID for this cooperative's funds.
    /// Generated from a fresh Ed25519 keypair on cooperative creation.
    /// `None` only for cooperatives created before treasury support was added.
    #[serde(default)]
    pub treasury_did: Option<String>,
}

fn default_min_founders() -> usize {
    3
}

fn default_min_members() -> usize {
    3
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub enum CoopType {
    Worker,
    Consumer,
    Producer,
    MultiStakeholder,
    Platform,
    Housing,
    Credit,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub enum CoopStatus {
    Forming,
    Active,
    Suspended,
    Dissolving,
    Dissolved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Member {
    pub did: Did,
    pub coop_id: String,
    pub role: MemberRole,
    pub status: MemberStatus,
    pub joined_at: DateTime<Utc>,
    pub shares: u64,
    pub metadata: HashMap<String, String>,

    // === Fields from icn-cooperative for richer membership ===
    /// Optional membership tier (for tier-based governance)
    #[serde(default)]
    pub tier: Option<MembershipTier>,

    /// Capital contribution made by this member
    #[serde(default)]
    pub capital_contribution: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub enum MemberRole {
    Founder,
    Member,
    Worker,
    Consumer,
    Producer,
    BoardMember,
    Officer,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub enum MemberStatus {
    Pending,
    Active,
    Suspended,
    Inactive,
    Removed,
}

/// Application to join a cooperative
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembershipApplication {
    /// DID of the applicant
    pub applicant_did: Did,
    /// Target cooperative ID
    pub coop_id: String,
    /// Desired tier name (if tier-based)
    pub tier: Option<String>,
    /// Desired role
    pub role: MemberRole,
    /// Capital contribution offered
    pub capital_contribution: u64,
    /// Statement explaining why they want to join
    pub statement: String,
    /// When the application was submitted
    pub submitted_at: DateTime<Utc>,
}

impl MembershipApplication {
    pub fn new(applicant_did: Did, coop_id: String, role: MemberRole, statement: String) -> Self {
        Self {
            applicant_did,
            coop_id,
            tier: None,
            role,
            capital_contribution: 0,
            statement,
            submitted_at: Utc::now(),
        }
    }

    pub fn with_capital(mut self, amount: u64) -> Self {
        self.capital_contribution = amount;
        self
    }

    pub fn with_tier(mut self, tier: String) -> Self {
        self.tier = Some(tier);
        self
    }
}

impl Cooperative {
    /// Create a new cooperative with explicit ID
    pub fn new_with_id(id: String, name: String, coop_type: CoopType) -> Self {
        let now = Utc::now();
        Self {
            id,
            name,
            coop_type,
            status: CoopStatus::Forming,
            domain_id: None,
            charter_hash: None,
            created_at: now,
            updated_at: now,
            metadata: HashMap::new(),
            min_members: 3,
            tiers: Vec::new(),
            capital_pool: 0,
            bylaws: Vec::new(),
            // Charter and governance fields
            charter_id: None,
            founding_signatures: Vec::new(),
            min_founders: 3,
            authorized_founders: Vec::new(),
            charter_ratified: false,
            description: None,
            currency: None,
            governance_domain: None,
            dissolution_plan: None,
            dissolution_proposal_id: None,
            treasury_did: None,
        }
    }

    /// Create a new cooperative with auto-generated ID
    pub fn new(name: String, coop_type: CoopType) -> Self {
        let id = format!("coop:{}", uuid::Uuid::new_v4());
        Self::new_with_id(id, name, coop_type)
    }

    /// Create a new cooperative with governance domain
    pub fn new_with_domain(
        id: String,
        name: String,
        coop_type: CoopType,
        domain_id: String,
        min_members: usize,
    ) -> Self {
        let now = Utc::now();
        let governance_domain = Some(format!("coop:{id}"));
        Self {
            id,
            name,
            coop_type,
            status: CoopStatus::Forming,
            domain_id: Some(domain_id),
            charter_hash: None,
            created_at: now,
            updated_at: now,
            metadata: HashMap::new(),
            min_members,
            tiers: Vec::new(),
            capital_pool: 0,
            bylaws: Vec::new(),
            // Charter and governance fields
            charter_id: None,
            founding_signatures: Vec::new(),
            min_founders: 3,
            authorized_founders: Vec::new(),
            charter_ratified: false,
            description: None,
            currency: None,
            governance_domain,
            dissolution_plan: None,
            dissolution_proposal_id: None,
            treasury_did: None,
        }
    }

    /// Create a cooperative from a formation request
    ///
    /// Stores the founding members as authorized signers - only these DIDs
    /// can sign the charter during formation.
    pub fn from_formation_request(request: &FormationRequest, id: String) -> Self {
        let mut coop = Self::new_with_id(id.clone(), request.name.clone(), request.coop_type);
        coop.min_members = request.min_members;
        coop.min_founders = request.founding_members.len().max(3);
        // Store authorized founders for signature validation
        coop.authorized_founders = request
            .founding_members
            .iter()
            .map(|d| d.to_string())
            .collect();
        coop.governance_domain = Some(format!("coop:{id}"));
        coop.description = request.description.clone();
        coop.currency = request.currency.clone();
        if let Some(hash) = &request.charter_document_hash {
            coop.charter_hash = Some(hex::encode(hash));
        }
        coop
    }

    /// Generate a fresh Ed25519 keypair for the cooperative treasury.
    ///
    /// Returns the DID string and the keypair. The caller is responsible for
    /// persisting the keypair securely (e.g., in an Age-encrypted keystore).
    /// Only the DID string is stored on the `Cooperative` struct.
    ///
    /// # Production safety
    /// Uses `OsRng` for key generation -- never derives from public seeds.
    pub fn generate_treasury_did() -> Result<(String, icn_identity::KeyPair), String> {
        let kp = icn_identity::KeyPair::generate()
            .map_err(|e| format!("Treasury keypair generation failed: {e}"))?;
        let did = kp.did().to_string();
        Ok((did, kp))
    }

    /// Generate a treasury keypair from known bytes (test-only).
    ///
    /// This function exists exclusively behind the `test_deterministic_keys`
    /// feature flag and MUST NOT be used in production builds.
    #[cfg(feature = "test_deterministic_keys")]
    pub fn generate_treasury_did_deterministic(
        secret_bytes: &[u8; 32],
        public_bytes: &[u8; 32],
    ) -> Result<(String, icn_identity::KeyPair), String> {
        let kp = icn_identity::KeyPair::from_bytes(secret_bytes, public_bytes)
            .map_err(|e| format!("Deterministic keypair creation failed: {e}"))?;
        let did = kp.did().to_string();
        Ok((did, kp))
    }

    /// Assign a treasury DID to this cooperative.
    ///
    /// Returns `Err` if the cooperative already has a treasury DID assigned,
    /// to prevent accidental key rotation (which should be a deliberate operation).
    pub fn assign_treasury(&mut self, treasury_did: String) -> Result<(), String> {
        if self.treasury_did.is_some() {
            return Err("Cooperative already has a treasury DID assigned".to_string());
        }
        self.treasury_did = Some(treasury_did);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Add a founder signature from governance type
    ///
    /// Returns `Ok(true)` if signature was added successfully.
    /// Returns `Err` if the signer is not an authorized founder.
    /// Returns `Ok(false)` if the founder has already signed.
    pub fn add_founder_signature(
        &mut self,
        signature: &icn_governance::charter::FounderSignature,
    ) -> Result<bool, String> {
        let did_str = signature.did.to_string();

        // Validate signer is an authorized founder (if list is non-empty)
        if !self.authorized_founders.is_empty() && !self.authorized_founders.contains(&did_str) {
            return Err(format!(
                "DID {did_str} is not an authorized founder for this cooperative"
            ));
        }

        // Check if this founder has already signed
        if self.founding_signatures.iter().any(|s| s.did == did_str) {
            return Ok(false);
        }
        self.founding_signatures
            .push(StoredFounderSignature::from_governance(signature));
        self.updated_at = Utc::now();

        // Check if we have enough signatures to ratify
        if self.founding_signatures.len() >= self.min_founders {
            self.charter_ratified = true;
        }
        Ok(true)
    }

    /// Check if the charter has been ratified (all required founders have signed)
    pub fn is_charter_ratified(&self) -> bool {
        self.charter_ratified && self.founding_signatures.len() >= self.min_founders
    }

    /// Get the number of pending founder signatures
    pub fn pending_signatures(&self) -> usize {
        if self.founding_signatures.len() >= self.min_founders {
            0
        } else {
            self.min_founders - self.founding_signatures.len()
        }
    }

    /// Set dissolution plan
    pub fn set_dissolution_plan(
        &mut self,
        plan: AssetDistributionPlan,
        proposal_id: Option<String>,
    ) {
        self.dissolution_plan = Some(plan);
        self.dissolution_proposal_id = proposal_id;
        self.updated_at = Utc::now();
    }

    pub fn can_transition_to(&self, new_status: &CoopStatus) -> bool {
        use CoopStatus::*;
        matches!(
            (&self.status, new_status),
            (Forming, Active)
                | (Active, Suspended)
                | (Active, Dissolving)
                | (Suspended, Active)
                | (Suspended, Dissolving)
                | (Dissolving, Dissolved)
        )
    }

    /// Add a membership tier
    pub fn add_tier(&mut self, tier: MembershipTier) {
        self.tiers.push(tier);
        self.updated_at = Utc::now();
    }

    /// Find a tier by name
    pub fn find_tier(&self, name: &str) -> Option<&MembershipTier> {
        self.tiers.iter().find(|t| t.name == name)
    }

    /// Add capital to the pool
    pub fn add_capital(&mut self, amount: u64) {
        self.capital_pool = self.capital_pool.saturating_add(amount);
        self.updated_at = Utc::now();
    }

    /// Remove capital from the pool (e.g., for member exit)
    pub fn remove_capital(&mut self, amount: u64) {
        self.capital_pool = self.capital_pool.saturating_sub(amount);
        self.updated_at = Utc::now();
    }
}

impl Member {
    pub fn new(did: Did, coop_id: String, role: MemberRole) -> Self {
        Self {
            did,
            coop_id,
            role,
            status: MemberStatus::Pending,
            joined_at: Utc::now(),
            shares: 0,
            metadata: HashMap::new(),
            tier: None,
            capital_contribution: 0,
        }
    }

    /// Create a member with a specific tier
    pub fn with_tier(mut self, tier: MembershipTier) -> Self {
        self.tier = Some(tier);
        self
    }

    /// Set capital contribution
    pub fn with_capital(mut self, amount: u64) -> Self {
        self.capital_contribution = amount;
        self
    }

    /// Get voting weight (from tier or default 1)
    pub fn voting_weight(&self) -> u32 {
        self.tier.as_ref().map(|t| t.voting_weight).unwrap_or(1)
    }

    /// Get profit share weight (from tier or default 1)
    pub fn profit_share_weight(&self) -> u32 {
        self.tier
            .as_ref()
            .map(|t| t.profit_share_weight)
            .unwrap_or(1)
    }

    /// Check if member has a specific governance right
    pub fn has_governance_right(&self, right: &str) -> bool {
        self.tier
            .as_ref()
            .map(|t| t.governance_rights.iter().any(|r| r == right))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_membership_tier_standard() {
        let tier = MembershipTier::standard("Worker");
        assert_eq!(tier.name, "Worker");
        assert_eq!(tier.voting_weight, 1);
        assert_eq!(tier.profit_share_weight, 1);
    }

    #[test]
    fn test_membership_tier_founder() {
        let tier = MembershipTier::founder();
        assert_eq!(tier.name, "Founder");
        assert!(tier.governance_rights.contains(&"amend_bylaws".to_string()));
    }

    #[test]
    fn test_cooperative_with_tiers() {
        let mut coop = Cooperative::new("Test Coop".to_string(), CoopType::Worker);
        coop.add_tier(MembershipTier::founder());
        coop.add_tier(MembershipTier::standard("Worker"));

        assert_eq!(coop.tiers.len(), 2);
        assert!(coop.find_tier("Founder").is_some());
        assert!(coop.find_tier("Worker").is_some());
        assert!(coop.find_tier("Consumer").is_none());
    }

    #[test]
    fn test_cooperative_capital() {
        let mut coop = Cooperative::new("Test Coop".to_string(), CoopType::Worker);
        assert_eq!(coop.capital_pool, 0);

        coop.add_capital(1000);
        assert_eq!(coop.capital_pool, 1000);

        coop.add_capital(500);
        assert_eq!(coop.capital_pool, 1500);

        coop.remove_capital(300);
        assert_eq!(coop.capital_pool, 1200);
    }

    #[test]
    fn test_member_with_tier() {
        let did = icn_identity::KeyPair::generate()
            .expect("keypair")
            .did()
            .clone();

        let tier = MembershipTier {
            name: "Senior Worker".to_string(),
            voting_weight: 2,
            profit_share_weight: 3,
            governance_rights: vec!["vote".to_string(), "propose".to_string()],
        };

        let member = Member::new(did, "coop:test".to_string(), MemberRole::Worker)
            .with_tier(tier)
            .with_capital(500);

        assert_eq!(member.voting_weight(), 2);
        assert_eq!(member.profit_share_weight(), 3);
        assert_eq!(member.capital_contribution, 500);
        assert!(member.has_governance_right("vote"));
        assert!(member.has_governance_right("propose"));
        assert!(!member.has_governance_right("elect_board"));
    }

    // === Treasury DID tests (Issue #1086) ===

    #[test]
    fn test_treasury_did_generated() {
        let (did, kp) = Cooperative::generate_treasury_did().expect("treasury keygen");
        assert!(did.starts_with("did:icn:"));
        assert_eq!(kp.did().to_string(), did);
    }

    #[test]
    fn test_treasury_did_unique() {
        let (did1, _kp1) = Cooperative::generate_treasury_did().expect("treasury keygen 1");
        let (did2, _kp2) = Cooperative::generate_treasury_did().expect("treasury keygen 2");
        assert_ne!(did1, did2, "Two treasury DIDs must be unique");
    }

    #[test]
    fn test_treasury_did_assign() {
        let mut coop = Cooperative::new("Treasury Test".to_string(), CoopType::Worker);
        assert!(coop.treasury_did.is_none());

        let (did, _kp) = Cooperative::generate_treasury_did().expect("treasury keygen");
        coop.assign_treasury(did.clone()).expect("assign treasury");
        assert_eq!(coop.treasury_did.as_deref(), Some(did.as_str()));
    }

    #[test]
    fn test_treasury_did_assign_rejects_double() {
        let mut coop = Cooperative::new("Treasury Test".to_string(), CoopType::Worker);
        let (did1, _) = Cooperative::generate_treasury_did().expect("keygen 1");
        let (did2, _) = Cooperative::generate_treasury_did().expect("keygen 2");

        coop.assign_treasury(did1).expect("first assign");
        let err = coop.assign_treasury(did2).unwrap_err();
        assert!(err.contains("already has a treasury DID"));
    }

    #[test]
    fn test_treasury_did_serialization_roundtrip() {
        let mut coop = Cooperative::new("Serde Test".to_string(), CoopType::Consumer);
        let (did, _kp) = Cooperative::generate_treasury_did().expect("treasury keygen");
        coop.assign_treasury(did.clone()).expect("assign treasury");

        let json = serde_json::to_string(&coop).expect("serialize");
        let deser: Cooperative = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deser.treasury_did.as_deref(), Some(did.as_str()));
    }

    #[test]
    fn test_treasury_did_default_none_backward_compat() {
        // Simulate an old cooperative serialized without treasury_did field.
        // serde(default) should produce None.
        let mut coop = Cooperative::new("Old Coop".to_string(), CoopType::Producer);
        coop.treasury_did = None; // Explicitly None, like old data
        let json = serde_json::to_string(&coop).expect("serialize");

        // Remove the treasury_did field from the JSON to simulate old format
        let json_value: serde_json::Value = serde_json::from_str(&json).expect("parse json");
        let mut obj = json_value.as_object().expect("object").clone();
        obj.remove("treasury_did");
        let old_json = serde_json::to_string(&obj).expect("reserialize");

        let deser: Cooperative = serde_json::from_str(&old_json).expect("deserialize old format");
        assert!(
            deser.treasury_did.is_none(),
            "Missing treasury_did field should deserialize as None"
        );
    }

    #[test]
    fn test_new_cooperative_treasury_did_is_none() {
        // All constructors should produce treasury_did = None by default.
        // The treasury keypair is generated separately and assigned explicitly.
        let coop1 = Cooperative::new("Test 1".to_string(), CoopType::Worker);
        assert!(coop1.treasury_did.is_none());

        let coop2 =
            Cooperative::new_with_id("id1".to_string(), "Test 2".to_string(), CoopType::Consumer);
        assert!(coop2.treasury_did.is_none());

        let coop3 = Cooperative::new_with_domain(
            "id2".to_string(),
            "Test 3".to_string(),
            CoopType::Platform,
            "domain1".to_string(),
            5,
        );
        assert!(coop3.treasury_did.is_none());
    }

    #[cfg(feature = "test_deterministic_keys")]
    #[test]
    fn test_deterministic_treasury_keys() {
        use ed25519_dalek::SigningKey;

        // Use known seed bytes to generate a deterministic keypair
        let secret_bytes: [u8; 32] = [42u8; 32];
        let signing_key = SigningKey::from_bytes(&secret_bytes);
        let public_bytes = signing_key.verifying_key().to_bytes();

        let (did1, _kp1) =
            Cooperative::generate_treasury_did_deterministic(&secret_bytes, &public_bytes)
                .expect("deterministic keygen 1");

        let (did2, _kp2) =
            Cooperative::generate_treasury_did_deterministic(&secret_bytes, &public_bytes)
                .expect("deterministic keygen 2");

        assert_eq!(did1, did2, "Same seed bytes must produce same DID");
        assert!(did1.starts_with("did:icn:"));
    }
}
