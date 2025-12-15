//! Charter - Founding document for jurisdictions
//!
//! A Charter is the founding document that creates a cooperative, community,
//! or federation as a jurisdiction within the commons. It specifies:
//! - Organization type and identity
//! - Governance configuration
//! - Membership policy
//! - Economic policy (for cooperatives)
//! - Dispute resolution rules
//!
//! Charters are immutable once ratified, but can be amended through
//! the governance process defined within them.

use crate::config::GovernanceConfig;
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

/// Unique identifier for a Charter
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CharterId(pub [u8; 32]);

impl CharterId {
    /// Create a new CharterId from bytes
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Create a CharterId from a hash of content
    pub fn from_content(content: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(content);
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

impl fmt::Display for CharterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Type of organization being chartered
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrgType {
    /// Economic cooperative (has ledger, treasury, economic transactions)
    Cooperative,
    /// Civic community (mutual aid, advocacy, stewardship)
    Community,
    /// Federation of coops/communities
    Federation,
}

impl fmt::Display for OrgType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrgType::Cooperative => write!(f, "Cooperative"),
            OrgType::Community => write!(f, "Community"),
            OrgType::Federation => write!(f, "Federation"),
        }
    }
}

impl OrgType {
    /// Check if this org type has economic features (ledger, treasury)
    pub fn has_economics(&self) -> bool {
        matches!(self, OrgType::Cooperative | OrgType::Federation)
    }

    /// Get the domain prefix for this org type
    pub fn domain_prefix(&self) -> &'static str {
        match self {
            OrgType::Cooperative => "coop",
            OrgType::Community => "community",
            OrgType::Federation => "federation",
        }
    }
}

/// Charter - founding document for a jurisdiction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Charter {
    /// Unique charter ID (hash of founding content)
    pub charter_id: CharterId,

    /// Type of organization
    pub org_type: OrgType,

    /// Domain ID for this jurisdiction (e.g., "coop:food-portland")
    pub domain_id: String,

    /// Human-readable name
    pub name: String,

    /// Optional description
    pub description: Option<String>,

    /// Hash of the founding document (off-chain reference)
    pub founding_document_hash: Option<[u8; 32]>,

    /// Founding signatures from initial members
    pub founders: Vec<FounderSignature>,

    /// Governance configuration
    pub governance: GovernanceConfig,

    /// Membership policy
    pub membership_policy: MembershipPolicy,

    /// Economic policy (required for cooperatives, optional for communities)
    pub economic_policy: Option<EconomicPolicy>,

    /// Dispute resolution policy
    pub dispute_policy: DisputePolicy,

    /// Initial network endpoints for bootstrap
    pub bootstrap_endpoints: Vec<String>,

    /// Charter creation timestamp (Unix)
    pub created_at: u64,

    /// Charter status
    pub status: CharterStatus,

    /// Amendment history (references to amendment records)
    pub amendments: Vec<AmendmentRef>,
}

impl Charter {
    /// Create a new charter
    pub fn new(
        org_type: OrgType,
        domain_id: String,
        name: String,
        governance: GovernanceConfig,
        membership_policy: MembershipPolicy,
        dispute_policy: DisputePolicy,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Generate charter ID from content
        let content = format!(
            "{}:{}:{}:{}",
            org_type.domain_prefix(),
            domain_id,
            name,
            now
        );
        let charter_id = CharterId::from_content(content.as_bytes());

        Self {
            charter_id,
            org_type,
            domain_id,
            name,
            description: None,
            founding_document_hash: None,
            founders: Vec::new(),
            governance,
            membership_policy,
            economic_policy: None,
            dispute_policy,
            bootstrap_endpoints: Vec::new(),
            created_at: now,
            status: CharterStatus::Draft,
            amendments: Vec::new(),
        }
    }

    /// Create a cooperative charter with default economics
    pub fn cooperative(
        domain_id: String,
        name: String,
        governance: GovernanceConfig,
        currency: String,
    ) -> Self {
        let mut charter = Self::new(
            OrgType::Cooperative,
            domain_id,
            name,
            governance,
            MembershipPolicy::default(),
            DisputePolicy::default(),
        );
        charter.economic_policy = Some(EconomicPolicy::cooperative_default(currency));
        charter
    }

    /// Create a community charter (no economics)
    pub fn community(domain_id: String, name: String, governance: GovernanceConfig) -> Self {
        Self::new(
            OrgType::Community,
            domain_id,
            name,
            governance,
            MembershipPolicy::default(),
            DisputePolicy::default(),
        )
    }

    /// Create a federation charter
    pub fn federation(
        domain_id: String,
        name: String,
        governance: GovernanceConfig,
        member_jurisdictions: Vec<String>,
    ) -> Self {
        let mut charter = Self::new(
            OrgType::Federation,
            domain_id,
            name,
            governance,
            MembershipPolicy::federation_default(),
            DisputePolicy::default(),
        );
        charter.economic_policy = Some(EconomicPolicy::federation_default());
        charter.membership_policy.jurisdiction_members = Some(member_jurisdictions);
        charter
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set founding document hash
    pub fn with_founding_document(mut self, hash: [u8; 32]) -> Self {
        self.founding_document_hash = Some(hash);
        self
    }

    /// Add a founder signature
    pub fn add_founder(&mut self, signature: FounderSignature) {
        if !self.founders.iter().any(|f| f.did == signature.did) {
            self.founders.push(signature);
        }
    }

    /// Add bootstrap endpoint
    pub fn add_endpoint(&mut self, endpoint: String) {
        if !self.bootstrap_endpoints.contains(&endpoint) {
            self.bootstrap_endpoints.push(endpoint);
        }
    }

    /// Check if charter has minimum founders
    pub fn has_minimum_founders(&self) -> bool {
        self.founders.len() >= self.membership_policy.min_founders as usize
    }

    /// Ratify the charter (transition from Draft to Active)
    pub fn ratify(&mut self) -> Result<(), &'static str> {
        if self.status != CharterStatus::Draft {
            return Err("Charter is not in Draft status");
        }
        if !self.has_minimum_founders() {
            return Err("Insufficient founders");
        }
        if self.org_type.has_economics() && self.economic_policy.is_none() {
            return Err("Economic policy required for this org type");
        }
        self.status = CharterStatus::Active;
        Ok(())
    }

    /// Suspend the charter
    pub fn suspend(&mut self, reason: String) {
        self.status = CharterStatus::Suspended { reason };
    }

    /// Dissolve the charter
    pub fn dissolve(&mut self, reason: String, governance_ref: String) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.status = CharterStatus::Dissolved {
            reason,
            dissolved_at: now,
            governance_ref,
        };
    }

    /// Check if charter is active
    pub fn is_active(&self) -> bool {
        matches!(self.status, CharterStatus::Active)
    }

    /// Check if charter is draft
    pub fn is_draft(&self) -> bool {
        matches!(self.status, CharterStatus::Draft)
    }

    /// Add an amendment reference
    pub fn add_amendment(&mut self, amendment: AmendmentRef) {
        self.amendments.push(amendment);
    }

    /// Get the full domain ID with type prefix
    pub fn full_domain_id(&self) -> String {
        format!("{}:{}", self.org_type.domain_prefix(), self.domain_id)
    }
}

/// Charter lifecycle status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CharterStatus {
    /// Draft - being created, not yet ratified
    Draft,
    /// Active - ratified and operational
    Active,
    /// Suspended - temporarily inactive
    Suspended { reason: String },
    /// Dissolved - permanently closed
    Dissolved {
        reason: String,
        dissolved_at: u64,
        governance_ref: String,
    },
}

impl fmt::Display for CharterStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CharterStatus::Draft => write!(f, "Draft"),
            CharterStatus::Active => write!(f, "Active"),
            CharterStatus::Suspended { reason } => write!(f, "Suspended: {reason}"),
            CharterStatus::Dissolved { reason, .. } => write!(f, "Dissolved: {reason}"),
        }
    }
}

/// Signature from a founder
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FounderSignature {
    /// Founder's DID
    pub did: Did,
    /// Ed25519 signature over charter content
    pub signature: Vec<u8>,
    /// When signature was made (Unix timestamp)
    pub timestamp: u64,
    /// Optional role in founding (e.g., "initiator", "steward")
    pub role: Option<String>,
}

impl FounderSignature {
    /// Create a new founder signature
    pub fn new(did: Did, signature: Vec<u8>) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Self {
            did,
            signature,
            timestamp: now,
            role: None,
        }
    }

    /// Set the founder's role
    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.role = Some(role.into());
        self
    }
}

/// Membership policy for a jurisdiction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembershipPolicy {
    /// Minimum number of founders required
    pub min_founders: u32,

    /// Minimum sponsors required for new members
    pub min_sponsors: u32,

    /// Probation period for new members (in seconds, 0 = none)
    pub probation_duration_secs: u64,

    /// Whether membership is open or invite-only
    pub open_enrollment: bool,

    /// Minimum personhood level required (Weak, Strong, Verified)
    pub required_pop_level: String,

    /// Required training/orientation IDs
    pub training_requirements: Vec<String>,

    /// Maximum members (0 = unlimited)
    pub max_members: u32,

    /// For federations: list of member jurisdiction IDs
    pub jurisdiction_members: Option<Vec<String>>,

    /// Membership fee (if any)
    pub membership_fee: Option<MembershipFee>,
}

impl Default for MembershipPolicy {
    fn default() -> Self {
        Self {
            min_founders: 3,
            min_sponsors: 1,
            probation_duration_secs: 30 * 24 * 60 * 60, // 30 days
            open_enrollment: true,
            required_pop_level: "Weak".to_string(),
            training_requirements: Vec::new(),
            max_members: 0, // unlimited
            jurisdiction_members: None,
            membership_fee: None,
        }
    }
}

impl MembershipPolicy {
    /// Create a policy for federations
    pub fn federation_default() -> Self {
        Self {
            min_founders: 2,                            // Federations need fewer founders
            min_sponsors: 0, // Members are jurisdictions, not individuals
            probation_duration_secs: 90 * 24 * 60 * 60, // 90 days probation
            open_enrollment: false, // Federations are invite-only
            required_pop_level: "Strong".to_string(), // Higher requirements for federation founders
            training_requirements: Vec::new(),
            max_members: 0,
            jurisdiction_members: Some(Vec::new()),
            membership_fee: None,
        }
    }

    /// Create a strict policy (verified personhood, sponsors required)
    pub fn strict() -> Self {
        Self {
            min_founders: 5,
            min_sponsors: 2,
            probation_duration_secs: 60 * 24 * 60 * 60, // 60 days
            open_enrollment: false,
            required_pop_level: "Verified".to_string(),
            training_requirements: Vec::new(),
            max_members: 0,
            jurisdiction_members: None,
            membership_fee: None,
        }
    }
}

/// Membership fee structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembershipFee {
    /// Amount
    pub amount: u64,
    /// Currency (e.g., "hours", "credits")
    pub currency: String,
    /// Fee period (monthly, yearly, one-time)
    pub period: FeePeriod,
}

/// Fee payment period
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeePeriod {
    OneTime,
    Monthly,
    Yearly,
}

/// Economic policy for cooperatives
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomicPolicy {
    /// Currency name (e.g., "hours", "labor-hours")
    pub currency: String,

    /// Credit limit for members (negative balance limit)
    pub member_credit_limit: i64,

    /// Whether demurrage (negative interest) is enabled
    pub demurrage_enabled: bool,

    /// Demurrage rate (percentage per year)
    pub demurrage_rate: f64,

    /// Minimum balance before demurrage applies
    pub demurrage_threshold: i64,

    /// Whether external bridging is allowed
    pub bridging_allowed: bool,

    /// Treasury DID (if set, separate from governance DID)
    pub treasury_did: Option<String>,

    /// Contribution routing policy
    pub contribution_routing: ContributionRouting,
}

impl EconomicPolicy {
    /// Create default cooperative economics
    pub fn cooperative_default(currency: String) -> Self {
        Self {
            currency,
            member_credit_limit: -100,
            demurrage_enabled: false,
            demurrage_rate: 0.0,
            demurrage_threshold: 1000,
            bridging_allowed: false,
            treasury_did: None,
            contribution_routing: ContributionRouting::default(),
        }
    }

    /// Create default federation economics
    pub fn federation_default() -> Self {
        Self {
            currency: "federation-credits".to_string(),
            member_credit_limit: -1000,
            demurrage_enabled: true,
            demurrage_rate: 2.0, // 2% per year
            demurrage_threshold: 10000,
            bridging_allowed: true,
            treasury_did: None,
            contribution_routing: ContributionRouting::default(),
        }
    }
}

/// How infrastructure contributions are routed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributionRouting {
    /// Percentage to individual contributor
    pub personal_percentage: u8,
    /// Percentage to jurisdiction treasury
    pub jurisdiction_percentage: u8,
    /// Percentage to federation (if part of one)
    pub federation_percentage: u8,
    /// Percentage to global commons
    pub commons_percentage: u8,
}

impl Default for ContributionRouting {
    fn default() -> Self {
        Self {
            personal_percentage: 60,
            jurisdiction_percentage: 30,
            federation_percentage: 5,
            commons_percentage: 5,
        }
    }
}

impl ContributionRouting {
    /// Validate percentages sum to 100
    pub fn validate(&self) -> Result<(), &'static str> {
        let total = self.personal_percentage as u16
            + self.jurisdiction_percentage as u16
            + self.federation_percentage as u16
            + self.commons_percentage as u16;
        if total != 100 {
            return Err("Contribution percentages must sum to 100");
        }
        Ok(())
    }
}

/// Dispute resolution policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisputePolicy {
    /// Whether internal dispute resolution is available
    pub internal_resolution: bool,

    /// Number of arbitrators for disputes
    pub arbitrator_count: u32,

    /// How arbitrators are selected
    pub arbitrator_selection: ArbitratorSelection,

    /// Maximum dispute resolution time (in seconds)
    pub resolution_deadline_secs: u64,

    /// Appeal allowed?
    pub appeals_allowed: bool,

    /// External mediation reference (if any)
    pub external_mediation: Option<String>,
}

impl Default for DisputePolicy {
    fn default() -> Self {
        Self {
            internal_resolution: true,
            arbitrator_count: 3,
            arbitrator_selection: ArbitratorSelection::RandomFromPool,
            resolution_deadline_secs: 30 * 24 * 60 * 60, // 30 days
            appeals_allowed: true,
            external_mediation: None,
        }
    }
}

/// How dispute arbitrators are selected
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArbitratorSelection {
    /// Random selection from eligible pool
    RandomFromPool,
    /// Elected by governance
    Elected,
    /// Parties each select one, they select a third
    PartySelection,
    /// Fixed panel (named arbitrators)
    FixedPanel,
}

/// Reference to a charter amendment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmendmentRef {
    /// Amendment ID
    pub amendment_id: [u8; 32],
    /// Brief description
    pub description: String,
    /// When amendment was ratified
    pub ratified_at: u64,
    /// Governance proposal that approved it
    pub proposal_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GovernanceParams;
    use crate::membership::MembershipConfig;
    use crate::profile::GovernanceProfileId;

    fn test_governance_config() -> GovernanceConfig {
        GovernanceConfig::new(
            GovernanceProfileId::builtin("cooperative_default"),
            MembershipConfig::trust_threshold(0.3),
            GovernanceParams::new(50, 50, 7 * 24 * 60 * 60),
        )
    }

    fn test_did() -> Did {
        Did::from_anchor_id(&[42u8; 32])
    }

    #[test]
    fn test_charter_creation() {
        let charter = Charter::new(
            OrgType::Cooperative,
            "food-portland".to_string(),
            "Portland Food Cooperative".to_string(),
            test_governance_config(),
            MembershipPolicy::default(),
            DisputePolicy::default(),
        );

        assert_eq!(charter.org_type, OrgType::Cooperative);
        assert_eq!(charter.domain_id, "food-portland");
        assert_eq!(charter.name, "Portland Food Cooperative");
        assert!(charter.is_draft());
        assert!(!charter.is_active());
    }

    #[test]
    fn test_cooperative_charter() {
        let charter = Charter::cooperative(
            "test-coop".to_string(),
            "Test Cooperative".to_string(),
            test_governance_config(),
            "hours".to_string(),
        );

        assert_eq!(charter.org_type, OrgType::Cooperative);
        assert!(charter.economic_policy.is_some());
        let policy = charter.economic_policy.unwrap();
        assert_eq!(policy.currency, "hours");
    }

    #[test]
    fn test_community_charter() {
        let charter = Charter::community(
            "mutual-aid".to_string(),
            "Mutual Aid Network".to_string(),
            test_governance_config(),
        );

        assert_eq!(charter.org_type, OrgType::Community);
        assert!(charter.economic_policy.is_none());
    }

    #[test]
    fn test_federation_charter() {
        let members = vec![
            "coop:food-portland".to_string(),
            "coop:housing-seattle".to_string(),
        ];
        let charter = Charter::federation(
            "pacific-nw".to_string(),
            "Pacific Northwest Federation".to_string(),
            test_governance_config(),
            members,
        );

        assert_eq!(charter.org_type, OrgType::Federation);
        assert!(charter.economic_policy.is_some());
        assert!(charter.membership_policy.jurisdiction_members.is_some());
        let jm = charter.membership_policy.jurisdiction_members.unwrap();
        assert_eq!(jm.len(), 2);
    }

    #[test]
    fn test_charter_ratification() {
        let mut charter = Charter::cooperative(
            "test".to_string(),
            "Test".to_string(),
            test_governance_config(),
            "hours".to_string(),
        );

        // Should fail without founders
        assert!(charter.ratify().is_err());

        // Add minimum founders
        for i in 0..3 {
            charter.add_founder(FounderSignature::new(
                Did::from_anchor_id(&[i as u8; 32]),
                vec![0u8; 64],
            ));
        }

        // Should succeed now
        assert!(charter.ratify().is_ok());
        assert!(charter.is_active());

        // Can't ratify again
        assert!(charter.ratify().is_err());
    }

    #[test]
    fn test_charter_lifecycle() {
        let mut charter = Charter::community(
            "test".to_string(),
            "Test".to_string(),
            test_governance_config(),
        );

        // Add founders and ratify
        for i in 0..3 {
            charter.add_founder(FounderSignature::new(
                Did::from_anchor_id(&[i as u8; 32]),
                vec![0u8; 64],
            ));
        }
        charter.ratify().unwrap();
        assert!(charter.is_active());

        // Suspend
        charter.suspend("Policy review".to_string());
        assert!(!charter.is_active());
        assert!(matches!(charter.status, CharterStatus::Suspended { .. }));

        // Dissolve
        charter.dissolve(
            "Members voted to dissolve".to_string(),
            "proposal-123".to_string(),
        );
        assert!(matches!(charter.status, CharterStatus::Dissolved { .. }));
    }

    #[test]
    fn test_founder_signatures() {
        let mut charter = Charter::community(
            "test".to_string(),
            "Test".to_string(),
            test_governance_config(),
        );

        let founder = FounderSignature::new(test_did(), vec![0u8; 64]).with_role("initiator");
        charter.add_founder(founder);

        assert_eq!(charter.founders.len(), 1);
        assert_eq!(charter.founders[0].role, Some("initiator".to_string()));

        // Adding same DID again should not duplicate
        charter.add_founder(FounderSignature::new(test_did(), vec![0u8; 64]));
        assert_eq!(charter.founders.len(), 1);
    }

    #[test]
    fn test_org_type() {
        assert!(OrgType::Cooperative.has_economics());
        assert!(!OrgType::Community.has_economics());
        assert!(OrgType::Federation.has_economics());

        assert_eq!(OrgType::Cooperative.domain_prefix(), "coop");
        assert_eq!(OrgType::Community.domain_prefix(), "community");
        assert_eq!(OrgType::Federation.domain_prefix(), "federation");
    }

    #[test]
    fn test_membership_policy() {
        let default = MembershipPolicy::default();
        assert_eq!(default.min_founders, 3);
        assert!(default.open_enrollment);

        let federation = MembershipPolicy::federation_default();
        assert_eq!(federation.min_founders, 2);
        assert!(!federation.open_enrollment);

        let strict = MembershipPolicy::strict();
        assert_eq!(strict.min_founders, 5);
        assert_eq!(strict.required_pop_level, "Verified");
    }

    #[test]
    fn test_economic_policy() {
        let coop = EconomicPolicy::cooperative_default("hours".to_string());
        assert_eq!(coop.currency, "hours");
        assert!(!coop.demurrage_enabled);
        assert!(!coop.bridging_allowed);

        let fed = EconomicPolicy::federation_default();
        assert!(fed.demurrage_enabled);
        assert!(fed.bridging_allowed);
    }

    #[test]
    fn test_contribution_routing() {
        let routing = ContributionRouting::default();
        assert!(routing.validate().is_ok());

        let invalid = ContributionRouting {
            personal_percentage: 50,
            jurisdiction_percentage: 30,
            federation_percentage: 10,
            commons_percentage: 5, // Only sums to 95
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_dispute_policy() {
        let policy = DisputePolicy::default();
        assert!(policy.internal_resolution);
        assert_eq!(policy.arbitrator_count, 3);
        assert!(policy.appeals_allowed);
    }

    #[test]
    fn test_charter_id() {
        let id1 = CharterId::from_content(b"test content 1");
        let id2 = CharterId::from_content(b"test content 2");

        assert_ne!(id1.0, id2.0);

        // Same content should produce same ID
        let id3 = CharterId::from_content(b"test content 1");
        assert_eq!(id1.0, id3.0);
    }

    #[test]
    fn test_charter_serialization() {
        let charter = Charter::cooperative(
            "test".to_string(),
            "Test Coop".to_string(),
            test_governance_config(),
            "hours".to_string(),
        );

        let json = serde_json::to_string(&charter).unwrap();
        let deserialized: Charter = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, charter.name);
        assert_eq!(deserialized.domain_id, charter.domain_id);
    }

    #[test]
    fn test_full_domain_id() {
        let charter = Charter::cooperative(
            "food-portland".to_string(),
            "Test".to_string(),
            test_governance_config(),
            "hours".to_string(),
        );

        assert_eq!(charter.full_domain_id(), "coop:food-portland");
    }

    #[test]
    fn test_charter_status_display() {
        assert_eq!(format!("{}", CharterStatus::Draft), "Draft");
        assert_eq!(format!("{}", CharterStatus::Active), "Active");

        let suspended = CharterStatus::Suspended {
            reason: "review".to_string(),
        };
        assert!(format!("{suspended}").contains("Suspended"));
    }

    #[test]
    fn test_amendment_ref() {
        let amendment = AmendmentRef {
            amendment_id: [1u8; 32],
            description: "Updated voting threshold".to_string(),
            ratified_at: 12345,
            proposal_id: "prop-001".to_string(),
        };

        let mut charter = Charter::community(
            "test".to_string(),
            "Test".to_string(),
            test_governance_config(),
        );
        charter.add_amendment(amendment);

        assert_eq!(charter.amendments.len(), 1);
    }
}
