//! Commons Layer 1 - CommonsHolderRecord and related types
//!
//! This module implements Layer 1 of the Commons Evolution RFC, providing:
//! - CommonsHolderRecord: A human participant's record in the global commons
//! - HolderStatus: Lifecycle states for commons holders
//! - Affiliation: Relationships to jurisdictions (coops, communities, federations)
//! - CommonsRights: Baseline rights that cannot be revoked by jurisdictions
//!
//! Key principle: A cooperative can revoke your membership.
//! A cooperative cannot revoke your personhood.

use crate::personhood::POPLevel;
use crate::Did;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Unique identifier for a jurisdiction (coop, community, or federation)
///
/// Format: `<type>:<id>` where type is one of:
/// - `coop:` - Economic cooperative
/// - `community:` - Civic community
/// - `federation:` - Federation of coops/communities
/// - `network:` - Network-level domains (commons, governance, stewards)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JurisdictionId(pub String);

impl JurisdictionId {
    /// Create a new jurisdiction ID
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Create a cooperative jurisdiction ID
    pub fn coop(name: &str) -> Self {
        Self(format!("coop:{name}"))
    }

    /// Create a community jurisdiction ID
    pub fn community(name: &str) -> Self {
        Self(format!("community:{name}"))
    }

    /// Create a federation jurisdiction ID
    pub fn federation(name: &str) -> Self {
        Self(format!("federation:{name}"))
    }

    /// Create a network-level jurisdiction ID
    pub fn network(name: &str) -> Self {
        Self(format!("network:{name}"))
    }

    /// Get the jurisdiction type
    pub fn jurisdiction_type(&self) -> Option<JurisdictionType> {
        let parts: Vec<&str> = self.0.splitn(2, ':').collect();
        if parts.len() != 2 {
            return None;
        }
        match parts[0] {
            "coop" => Some(JurisdictionType::Cooperative),
            "community" => Some(JurisdictionType::Community),
            "federation" => Some(JurisdictionType::Federation),
            "network" => Some(JurisdictionType::Network),
            _ => None,
        }
    }

    /// Get the jurisdiction name (without the type prefix)
    pub fn name(&self) -> Option<&str> {
        self.0.split(':').nth(1)
    }

    /// Get the full ID string
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for JurisdictionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Type of jurisdiction within the commons
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JurisdictionType {
    /// Economic cooperative (has ledger, treasury)
    Cooperative,
    /// Civic community (mutual aid, advocacy, stewardship)
    Community,
    /// Federation of coops/communities
    Federation,
    /// Network-level domain (commons, governance, stewards)
    Network,
}

impl fmt::Display for JurisdictionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JurisdictionType::Cooperative => write!(f, "Cooperative"),
            JurisdictionType::Community => write!(f, "Community"),
            JurisdictionType::Federation => write!(f, "Federation"),
            JurisdictionType::Network => write!(f, "Network"),
        }
    }
}

/// Record of a Commons Holder - a human participant with baseline rights
///
/// A CommonsHolderRecord represents Layer 1 of the identity model:
/// - Links to the PersonhoodAnchor (Layer 0)
/// - Holds baseline rights that cannot be revoked by jurisdictions
/// - Tracks affiliations with coops, communities, and federations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommonsHolderRecord {
    /// Unique holder ID (derived from anchor_id)
    pub holder_id: [u8; 32],

    /// The holder's current DID (may change with key rotation)
    pub holder_did: Did,

    /// Link to underlying PersonhoodAnchor (Layer 0)
    pub anchor_id: [u8; 32],

    /// Holder status in the commons
    pub status: HolderStatus,

    /// Current personhood verification level
    pub personhood_level: POPLevel,

    /// Affiliations with jurisdictions (coops, communities, federations)
    pub affiliations: Vec<Affiliation>,

    /// Baseline rights (network-level, non-revocable by jurisdictions)
    pub baseline_rights: CommonsRights,

    /// When holder entered the commons (Unix timestamp)
    pub created_at: u64,

    /// Last governance review for periodic re-verification
    pub last_review_at: Option<u64>,

    /// Last update timestamp
    pub updated_at: u64,
}

impl CommonsHolderRecord {
    /// Create a new CommonsHolderRecord from a PersonhoodAnchor
    pub fn new(anchor_id: [u8; 32], holder_did: Did, personhood_level: POPLevel) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Holder ID is derived from anchor_id (they're the same for now)
        let holder_id = anchor_id;

        Self {
            holder_id,
            holder_did,
            anchor_id,
            status: HolderStatus::Active,
            personhood_level,
            affiliations: Vec::new(),
            baseline_rights: CommonsRights::default(),
            created_at: now,
            last_review_at: None,
            updated_at: now,
        }
    }

    /// Get the holder ID
    pub fn id(&self) -> &[u8; 32] {
        &self.holder_id
    }

    /// Check if holder is active
    pub fn is_active(&self) -> bool {
        matches!(self.status, HolderStatus::Active)
    }

    /// Check if holder is suspended
    pub fn is_suspended(&self) -> bool {
        matches!(self.status, HolderStatus::Suspended { .. })
    }

    /// Check if holder has exited
    pub fn has_exited(&self) -> bool {
        matches!(self.status, HolderStatus::Exited { .. })
    }

    /// Check if holder is revoked
    pub fn is_revoked(&self) -> bool {
        matches!(self.status, HolderStatus::Revoked { .. })
    }

    /// Suspend the holder
    pub fn suspend(&mut self, reason: String, appeal_deadline: u64) {
        self.status = HolderStatus::Suspended {
            reason,
            appeal_deadline,
            suspended_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };
        self.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }

    /// Reinstate a suspended holder
    pub fn reinstate(&mut self) -> bool {
        if matches!(self.status, HolderStatus::Suspended { .. }) {
            self.status = HolderStatus::Active;
            self.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            true
        } else {
            false
        }
    }

    /// Exit the commons voluntarily
    pub fn exit(&mut self, reason: String) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.status = HolderStatus::Exited {
            reason,
            exit_timestamp: now,
        };
        self.updated_at = now;
    }

    /// Revoke holder status (network governance only)
    pub fn revoke(&mut self, reason: String, evidence: Vec<[u8; 32]>) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.status = HolderStatus::Revoked {
            reason,
            evidence,
            revoked_at: now,
        };
        self.updated_at = now;
    }

    /// Add an affiliation with a jurisdiction
    pub fn add_affiliation(&mut self, affiliation: Affiliation) {
        // Check if already affiliated with this jurisdiction
        if !self
            .affiliations
            .iter()
            .any(|a| a.jurisdiction_id == affiliation.jurisdiction_id)
        {
            self.affiliations.push(affiliation);
            self.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
        }
    }

    /// Remove an affiliation with a jurisdiction
    pub fn remove_affiliation(&mut self, jurisdiction_id: &JurisdictionId) -> Option<Affiliation> {
        if let Some(pos) = self
            .affiliations
            .iter()
            .position(|a| &a.jurisdiction_id == jurisdiction_id)
        {
            self.updated_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            Some(self.affiliations.remove(pos))
        } else {
            None
        }
    }

    /// Get an affiliation by jurisdiction ID
    pub fn get_affiliation(&self, jurisdiction_id: &JurisdictionId) -> Option<&Affiliation> {
        self.affiliations
            .iter()
            .find(|a| &a.jurisdiction_id == jurisdiction_id)
    }

    /// Get a mutable affiliation by jurisdiction ID
    pub fn get_affiliation_mut(
        &mut self,
        jurisdiction_id: &JurisdictionId,
    ) -> Option<&mut Affiliation> {
        self.affiliations
            .iter_mut()
            .find(|a| &a.jurisdiction_id == jurisdiction_id)
    }

    /// Check if affiliated with a jurisdiction
    pub fn is_affiliated(&self, jurisdiction_id: &JurisdictionId) -> bool {
        self.affiliations
            .iter()
            .any(|a| &a.jurisdiction_id == jurisdiction_id)
    }

    /// List all active affiliations
    pub fn active_affiliations(&self) -> Vec<&Affiliation> {
        self.affiliations
            .iter()
            .filter(|a| a.membership_status == MembershipStatus::Member)
            .collect()
    }

    /// Check if holder has a specific right
    pub fn has_right(&self, right: CommonsRight) -> bool {
        if !self.is_active() {
            return false;
        }
        self.baseline_rights.has_right(right)
    }

    /// Update personhood level
    pub fn update_personhood_level(&mut self, level: POPLevel) {
        self.personhood_level = level;
        self.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }

    /// Record a governance review
    pub fn record_review(&mut self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.last_review_at = Some(now);
        self.updated_at = now;
    }
}

/// Status of a Commons Holder
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HolderStatus {
    /// Active holder in good standing
    Active,

    /// Temporarily suspended (with appeal rights)
    Suspended {
        reason: String,
        appeal_deadline: u64,
        suspended_at: u64,
    },

    /// Voluntarily exited the commons
    Exited {
        reason: String,
        exit_timestamp: u64,
    },

    /// Revoked by network governance (serious violations)
    Revoked {
        reason: String,
        evidence: Vec<[u8; 32]>,
        revoked_at: u64,
    },
}

impl fmt::Display for HolderStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HolderStatus::Active => write!(f, "Active"),
            HolderStatus::Suspended { reason, .. } => write!(f, "Suspended: {reason}"),
            HolderStatus::Exited { reason, .. } => write!(f, "Exited: {reason}"),
            HolderStatus::Revoked { reason, .. } => write!(f, "Revoked: {reason}"),
        }
    }
}

/// Affiliation with a jurisdiction (coop, community, or federation)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Affiliation {
    /// The jurisdiction this affiliation is with
    pub jurisdiction_id: JurisdictionId,

    /// Membership status within this jurisdiction
    pub membership_status: MembershipStatus,

    /// Capabilities granted by this membership
    pub capabilities: Vec<MembershipCapability>,

    /// Roles held in this jurisdiction
    pub roles: Vec<String>,

    /// When affiliation began (Unix timestamp)
    pub joined_at: u64,

    /// Optional expiry (for term-limited memberships)
    pub expires_at: Option<u64>,
}

impl Affiliation {
    /// Create a new affiliation
    pub fn new(jurisdiction_id: JurisdictionId) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            jurisdiction_id,
            membership_status: MembershipStatus::Candidate,
            capabilities: Vec::new(),
            roles: Vec::new(),
            joined_at: now,
            expires_at: None,
        }
    }

    /// Create an affiliation with full member status
    pub fn as_member(jurisdiction_id: JurisdictionId) -> Self {
        let mut affiliation = Self::new(jurisdiction_id);
        affiliation.membership_status = MembershipStatus::Member;
        affiliation.capabilities = vec![
            MembershipCapability::Vote,
            MembershipCapability::Propose,
            MembershipCapability::Transact,
        ];
        affiliation
    }

    /// Check if this affiliation grants a capability
    pub fn has_capability(&self, capability: MembershipCapability) -> bool {
        self.capabilities.contains(&capability)
    }

    /// Add a capability
    pub fn add_capability(&mut self, capability: MembershipCapability) {
        if !self.capabilities.contains(&capability) {
            self.capabilities.push(capability);
        }
    }

    /// Remove a capability
    pub fn remove_capability(&mut self, capability: MembershipCapability) {
        self.capabilities.retain(|c| c != &capability);
    }

    /// Check if membership is active
    pub fn is_active(&self) -> bool {
        matches!(
            self.membership_status,
            MembershipStatus::Member | MembershipStatus::Provisional
        )
    }

    /// Check if membership has expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            now >= expires_at
        } else {
            false
        }
    }

    /// Approve a candidate to provisional status
    pub fn approve(&mut self) {
        if self.membership_status == MembershipStatus::Candidate {
            self.membership_status = MembershipStatus::Provisional;
        }
    }

    /// Promote from provisional to full member
    pub fn promote(&mut self) {
        if self.membership_status == MembershipStatus::Provisional {
            self.membership_status = MembershipStatus::Member;
        }
    }

    /// Suspend membership
    pub fn suspend(&mut self) {
        self.membership_status = MembershipStatus::Suspended;
    }

    /// Exit membership voluntarily
    pub fn exit(&mut self) {
        self.membership_status = MembershipStatus::Exited;
    }

    /// Ban from jurisdiction
    pub fn ban(&mut self) {
        self.membership_status = MembershipStatus::Banned;
        self.capabilities.clear();
        self.roles.clear();
    }
}

/// Membership status within a jurisdiction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MembershipStatus {
    /// Applied, not yet approved
    Candidate,
    /// Approved, in probation period
    Provisional,
    /// Full member
    Member,
    /// Temporarily suspended
    Suspended,
    /// Voluntarily left
    Exited,
    /// Removed by governance
    Banned,
}

impl fmt::Display for MembershipStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MembershipStatus::Candidate => write!(f, "Candidate"),
            MembershipStatus::Provisional => write!(f, "Provisional"),
            MembershipStatus::Member => write!(f, "Member"),
            MembershipStatus::Suspended => write!(f, "Suspended"),
            MembershipStatus::Exited => write!(f, "Exited"),
            MembershipStatus::Banned => write!(f, "Banned"),
        }
    }
}

/// Capabilities granted by jurisdiction membership
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MembershipCapability {
    /// Can vote in jurisdiction governance
    Vote,
    /// Can create proposals
    Propose,
    /// Can participate in economic transactions
    Transact,
    /// Can hold elected/appointed positions
    HoldOffice,
    /// Can access private/internal resources
    AccessPrivate,
    /// Can invite/sponsor new members
    Sponsor,
}

impl fmt::Display for MembershipCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MembershipCapability::Vote => write!(f, "Vote"),
            MembershipCapability::Propose => write!(f, "Propose"),
            MembershipCapability::Transact => write!(f, "Transact"),
            MembershipCapability::HoldOffice => write!(f, "HoldOffice"),
            MembershipCapability::AccessPrivate => write!(f, "AccessPrivate"),
            MembershipCapability::Sponsor => write!(f, "Sponsor"),
        }
    }
}

/// Individual commons rights that can be checked
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CommonsRight {
    /// Right to vote in commons-level governance
    VoteInCommons,
    /// Right to petition for constitutional changes
    Petition,
    /// Guaranteed minimum fuel for civic participation
    MinimalFuelFloor,
    /// Right to due process before suspension/revocation
    DueProcess,
    /// Right to exit any jurisdiction freely
    ExitFreely,
    /// Right to audit public records
    AuditAccess,
}

impl fmt::Display for CommonsRight {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommonsRight::VoteInCommons => write!(f, "VoteInCommons"),
            CommonsRight::Petition => write!(f, "Petition"),
            CommonsRight::MinimalFuelFloor => write!(f, "MinimalFuelFloor"),
            CommonsRight::DueProcess => write!(f, "DueProcess"),
            CommonsRight::ExitFreely => write!(f, "ExitFreely"),
            CommonsRight::AuditAccess => write!(f, "AuditAccess"),
        }
    }
}

/// Baseline rights held by all Commons Holders
///
/// These rights CANNOT be revoked by individual jurisdictions.
/// They can only be affected by network-level governance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommonsRights {
    /// Right to vote in commons-level governance
    pub vote_in_commons: bool,

    /// Right to petition for constitutional changes
    pub petition: bool,

    /// Guaranteed minimum fuel for civic participation
    pub minimal_fuel_floor: bool,

    /// Right to due process before suspension/revocation
    pub due_process: bool,

    /// Right to exit any jurisdiction freely
    pub exit_freely: bool,

    /// Right to audit public records
    pub audit_access: bool,
}

impl Default for CommonsRights {
    fn default() -> Self {
        // All commons holders get all baseline rights by default
        Self {
            vote_in_commons: true,
            petition: true,
            minimal_fuel_floor: true,
            due_process: true,
            exit_freely: true,
            audit_access: true,
        }
    }
}

impl CommonsRights {
    /// Create rights for a new holder with all rights enabled
    pub fn full() -> Self {
        Self::default()
    }

    /// Create restricted rights (for suspended holders)
    pub fn restricted() -> Self {
        Self {
            vote_in_commons: false,
            petition: false,
            minimal_fuel_floor: true, // Always guaranteed
            due_process: true,        // Always guaranteed
            exit_freely: true,        // Always guaranteed
            audit_access: false,
        }
    }

    /// Check if a specific right is held
    pub fn has_right(&self, right: CommonsRight) -> bool {
        match right {
            CommonsRight::VoteInCommons => self.vote_in_commons,
            CommonsRight::Petition => self.petition,
            CommonsRight::MinimalFuelFloor => self.minimal_fuel_floor,
            CommonsRight::DueProcess => self.due_process,
            CommonsRight::ExitFreely => self.exit_freely,
            CommonsRight::AuditAccess => self.audit_access,
        }
    }

    /// List all active rights
    pub fn active_rights(&self) -> Vec<CommonsRight> {
        let mut rights = Vec::new();
        if self.vote_in_commons {
            rights.push(CommonsRight::VoteInCommons);
        }
        if self.petition {
            rights.push(CommonsRight::Petition);
        }
        if self.minimal_fuel_floor {
            rights.push(CommonsRight::MinimalFuelFloor);
        }
        if self.due_process {
            rights.push(CommonsRight::DueProcess);
        }
        if self.exit_freely {
            rights.push(CommonsRight::ExitFreely);
        }
        if self.audit_access {
            rights.push(CommonsRight::AuditAccess);
        }
        rights
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_anchor_id() -> [u8; 32] {
        [42u8; 32]
    }

    fn test_did() -> Did {
        Did::from_anchor_id(&test_anchor_id())
    }

    #[test]
    fn test_jurisdiction_id_coop() {
        let id = JurisdictionId::coop("food-portland");
        assert_eq!(id.as_str(), "coop:food-portland");
        assert_eq!(id.jurisdiction_type(), Some(JurisdictionType::Cooperative));
        assert_eq!(id.name(), Some("food-portland"));
    }

    #[test]
    fn test_jurisdiction_id_community() {
        let id = JurisdictionId::community("mutual-aid-queens");
        assert_eq!(id.as_str(), "community:mutual-aid-queens");
        assert_eq!(id.jurisdiction_type(), Some(JurisdictionType::Community));
    }

    #[test]
    fn test_jurisdiction_id_federation() {
        let id = JurisdictionId::federation("pacific-nw");
        assert_eq!(id.as_str(), "federation:pacific-nw");
        assert_eq!(id.jurisdiction_type(), Some(JurisdictionType::Federation));
    }

    #[test]
    fn test_jurisdiction_id_network() {
        let id = JurisdictionId::network("commons");
        assert_eq!(id.as_str(), "network:commons");
        assert_eq!(id.jurisdiction_type(), Some(JurisdictionType::Network));
    }

    #[test]
    fn test_commons_holder_creation() {
        let holder = CommonsHolderRecord::new(test_anchor_id(), test_did(), POPLevel::Strong);

        assert!(holder.is_active());
        assert!(!holder.is_suspended());
        assert!(!holder.has_exited());
        assert!(!holder.is_revoked());
        assert_eq!(holder.personhood_level, POPLevel::Strong);
        assert!(holder.affiliations.is_empty());
    }

    #[test]
    fn test_holder_status_lifecycle() {
        let mut holder = CommonsHolderRecord::new(test_anchor_id(), test_did(), POPLevel::Strong);

        // Suspend
        holder.suspend("Policy violation".to_string(), 9999999999);
        assert!(holder.is_suspended());
        assert!(!holder.is_active());

        // Reinstate
        assert!(holder.reinstate());
        assert!(holder.is_active());
        assert!(!holder.is_suspended());

        // Exit
        holder.exit("Moving to another network".to_string());
        assert!(holder.has_exited());
        assert!(!holder.is_active());
    }

    #[test]
    fn test_holder_revocation() {
        let mut holder = CommonsHolderRecord::new(test_anchor_id(), test_did(), POPLevel::Strong);

        let evidence = vec![[1u8; 32], [2u8; 32]];
        holder.revoke("Sybil attack detected".to_string(), evidence.clone());

        assert!(holder.is_revoked());
        if let HolderStatus::Revoked {
            reason, evidence: e, ..
        } = &holder.status
        {
            assert_eq!(reason, "Sybil attack detected");
            assert_eq!(e.len(), 2);
        } else {
            panic!("Expected Revoked status");
        }
    }

    #[test]
    fn test_affiliation_management() {
        let mut holder = CommonsHolderRecord::new(test_anchor_id(), test_did(), POPLevel::Strong);

        let coop_id = JurisdictionId::coop("food-portland");
        let affiliation = Affiliation::as_member(coop_id.clone());

        holder.add_affiliation(affiliation);
        assert!(holder.is_affiliated(&coop_id));
        assert_eq!(holder.affiliations.len(), 1);

        // Try adding duplicate
        let dup_affiliation = Affiliation::as_member(coop_id.clone());
        holder.add_affiliation(dup_affiliation);
        assert_eq!(holder.affiliations.len(), 1); // No duplicate added

        // Remove
        let removed = holder.remove_affiliation(&coop_id);
        assert!(removed.is_some());
        assert!(!holder.is_affiliated(&coop_id));
    }

    #[test]
    fn test_affiliation_lifecycle() {
        let coop_id = JurisdictionId::coop("test-coop");
        let mut affiliation = Affiliation::new(coop_id);

        assert_eq!(affiliation.membership_status, MembershipStatus::Candidate);
        assert!(!affiliation.is_active());

        // Approve to provisional
        affiliation.approve();
        assert_eq!(affiliation.membership_status, MembershipStatus::Provisional);
        assert!(affiliation.is_active());

        // Promote to full member
        affiliation.promote();
        assert_eq!(affiliation.membership_status, MembershipStatus::Member);
        assert!(affiliation.is_active());

        // Suspend
        affiliation.suspend();
        assert_eq!(affiliation.membership_status, MembershipStatus::Suspended);
        assert!(!affiliation.is_active());
    }

    #[test]
    fn test_affiliation_capabilities() {
        let coop_id = JurisdictionId::coop("test-coop");
        let mut affiliation = Affiliation::as_member(coop_id);

        assert!(affiliation.has_capability(MembershipCapability::Vote));
        assert!(affiliation.has_capability(MembershipCapability::Transact));
        assert!(!affiliation.has_capability(MembershipCapability::HoldOffice));

        // Add capability
        affiliation.add_capability(MembershipCapability::HoldOffice);
        assert!(affiliation.has_capability(MembershipCapability::HoldOffice));

        // Remove capability
        affiliation.remove_capability(MembershipCapability::Vote);
        assert!(!affiliation.has_capability(MembershipCapability::Vote));
    }

    #[test]
    fn test_affiliation_ban() {
        let coop_id = JurisdictionId::coop("test-coop");
        let mut affiliation = Affiliation::as_member(coop_id);
        affiliation.roles.push("steward".to_string());

        affiliation.ban();
        assert_eq!(affiliation.membership_status, MembershipStatus::Banned);
        assert!(affiliation.capabilities.is_empty());
        assert!(affiliation.roles.is_empty());
    }

    #[test]
    fn test_commons_rights_default() {
        let rights = CommonsRights::default();

        assert!(rights.vote_in_commons);
        assert!(rights.petition);
        assert!(rights.minimal_fuel_floor);
        assert!(rights.due_process);
        assert!(rights.exit_freely);
        assert!(rights.audit_access);
    }

    #[test]
    fn test_commons_rights_restricted() {
        let rights = CommonsRights::restricted();

        assert!(!rights.vote_in_commons);
        assert!(!rights.petition);
        // These are always guaranteed
        assert!(rights.minimal_fuel_floor);
        assert!(rights.due_process);
        assert!(rights.exit_freely);
        assert!(!rights.audit_access);
    }

    #[test]
    fn test_commons_rights_has_right() {
        let rights = CommonsRights::default();

        assert!(rights.has_right(CommonsRight::VoteInCommons));
        assert!(rights.has_right(CommonsRight::ExitFreely));

        let restricted = CommonsRights::restricted();
        assert!(!restricted.has_right(CommonsRight::VoteInCommons));
        assert!(restricted.has_right(CommonsRight::ExitFreely));
    }

    #[test]
    fn test_holder_has_right() {
        let holder = CommonsHolderRecord::new(test_anchor_id(), test_did(), POPLevel::Strong);

        assert!(holder.has_right(CommonsRight::VoteInCommons));
        assert!(holder.has_right(CommonsRight::ExitFreely));
    }

    #[test]
    fn test_holder_suspended_loses_rights_check() {
        let mut holder = CommonsHolderRecord::new(test_anchor_id(), test_did(), POPLevel::Strong);

        // Active holder has rights
        assert!(holder.has_right(CommonsRight::VoteInCommons));

        // Suspended holder has_right returns false (because is_active check)
        holder.suspend("test".to_string(), 9999999999);
        assert!(!holder.has_right(CommonsRight::VoteInCommons));
    }

    #[test]
    fn test_active_affiliations() {
        let mut holder = CommonsHolderRecord::new(test_anchor_id(), test_did(), POPLevel::Strong);

        // Add active member
        let coop1 = JurisdictionId::coop("coop1");
        holder.add_affiliation(Affiliation::as_member(coop1));

        // Add candidate (not active)
        let coop2 = JurisdictionId::coop("coop2");
        holder.add_affiliation(Affiliation::new(coop2));

        let active = holder.active_affiliations();
        assert_eq!(active.len(), 1);
    }

    #[test]
    fn test_serialization() {
        let mut holder = CommonsHolderRecord::new(test_anchor_id(), test_did(), POPLevel::Strong);

        let coop_id = JurisdictionId::coop("test-coop");
        holder.add_affiliation(Affiliation::as_member(coop_id));

        let json = serde_json::to_string(&holder).unwrap();
        let deserialized: CommonsHolderRecord = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.holder_id, holder.holder_id);
        assert_eq!(deserialized.personhood_level, holder.personhood_level);
        assert_eq!(deserialized.affiliations.len(), 1);
    }

    #[test]
    fn test_holder_status_display() {
        assert_eq!(format!("{}", HolderStatus::Active), "Active");

        let suspended = HolderStatus::Suspended {
            reason: "test".to_string(),
            appeal_deadline: 0,
            suspended_at: 0,
        };
        assert!(format!("{suspended}").contains("Suspended"));
    }

    #[test]
    fn test_jurisdiction_type_display() {
        assert_eq!(format!("{}", JurisdictionType::Cooperative), "Cooperative");
        assert_eq!(format!("{}", JurisdictionType::Community), "Community");
        assert_eq!(format!("{}", JurisdictionType::Federation), "Federation");
        assert_eq!(format!("{}", JurisdictionType::Network), "Network");
    }

    #[test]
    fn test_membership_status_display() {
        assert_eq!(format!("{}", MembershipStatus::Candidate), "Candidate");
        assert_eq!(format!("{}", MembershipStatus::Member), "Member");
        assert_eq!(format!("{}", MembershipStatus::Banned), "Banned");
    }

    #[test]
    fn test_active_rights_list() {
        let rights = CommonsRights::restricted();
        let active = rights.active_rights();

        // Restricted should still have fuel floor, due process, exit
        assert_eq!(active.len(), 3);
        assert!(active.contains(&CommonsRight::MinimalFuelFloor));
        assert!(active.contains(&CommonsRight::DueProcess));
        assert!(active.contains(&CommonsRight::ExitFreely));
    }

    #[test]
    fn test_update_personhood_level() {
        let mut holder = CommonsHolderRecord::new(test_anchor_id(), test_did(), POPLevel::Weak);

        assert_eq!(holder.personhood_level, POPLevel::Weak);

        holder.update_personhood_level(POPLevel::Verified);
        assert_eq!(holder.personhood_level, POPLevel::Verified);
    }

    #[test]
    fn test_record_review() {
        let mut holder = CommonsHolderRecord::new(test_anchor_id(), test_did(), POPLevel::Strong);

        assert!(holder.last_review_at.is_none());

        holder.record_review();
        assert!(holder.last_review_at.is_some());
    }
}
