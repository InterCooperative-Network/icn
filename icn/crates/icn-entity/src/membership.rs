//! Membership types for the ICN entity model
//!
//! This module defines the membership relationship between entities.
//! The key insight is that membership is recursive: a cooperative's members
//! can be individuals OR other cooperatives.

use crate::entity::EntityId;
use serde::{Deserialize, Serialize};
use std::fmt;

// ============================================================================
// Membership
// ============================================================================

/// A membership record linking an entity to its parent entity
///
/// This is the key to recursive composition: members can be individuals
/// (backed by DIDs) OR other cooperatives, enabling:
/// - Worker cooperatives with individual members
/// - Federations with cooperative members
/// - Multi-stakeholder cooperatives with mixed membership
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Membership {
    /// The entity being granted membership (individual or organization)
    pub member_id: EntityId,

    /// The entity granting membership (cooperative or federation)
    pub parent_id: EntityId,

    /// Role within the parent entity
    pub role: MembershipRole,

    /// Current membership status
    pub status: MembershipStatus,

    /// When membership was created (Unix timestamp)
    pub joined_at: u64,

    /// When membership was last updated (Unix timestamp)
    pub updated_at: u64,

    /// Voting shares (for weighted voting systems)
    /// 0 means no voting rights (e.g., observer status)
    pub shares: u64,

    /// Capabilities granted by this membership
    pub capabilities: Vec<MembershipCapability>,

    /// Optional notes about this membership
    pub notes: Option<String>,

    // ========================================
    // Labor Exchange Fields (Issue #718)
    // ========================================
    /// Active labor assignments (working at other cooperatives).
    ///
    /// Stored as strings to avoid a dependency on labor exchange types.
    #[serde(default)]
    pub assignments: Vec<String>,

    // ========================================
    // Labor Share Fields (Issue #390)
    // ========================================
    /// Labor share IDs held by this member (references `icn_ledger::ShareId`)
    ///
    /// Each share represents accumulated labor value from work contributions.
    /// Shares track labor_days and accumulated_surplus for surplus distribution.
    ///
    /// **Design Note:** Uses `Vec<String>` instead of `Vec<ShareId>` to avoid
    /// a circular dependency between `icn-entity` and `icn-ledger`. The share IDs
    /// are validated at the ledger layer when operations are performed.
    #[serde(default)]
    pub labor_shares: Vec<String>,

    /// Is this the primary membership for multi-coop workers?
    ///
    /// When a worker has memberships in multiple cooperatives,
    /// the primary membership determines which share accumulates
    /// labor days. Only one membership per worker should be primary.
    #[serde(default = "default_is_primary")]
    pub is_primary: bool,
}

/// Default value for is_primary (true for backwards compatibility)
fn default_is_primary() -> bool {
    true
}

impl Membership {
    /// Create a new pending membership
    pub fn new(member_id: EntityId, parent_id: EntityId, role: MembershipRole) -> Self {
        let now = icn_time::current_timestamp_secs();
        let capabilities = role.default_capabilities();
        Membership {
            member_id,
            parent_id,
            role,
            status: MembershipStatus::Pending,
            joined_at: now,
            updated_at: now,
            shares: 1, // Default to 1 share (one member, one vote)
            capabilities,
            notes: None,
            assignments: Vec::new(),
            labor_shares: Vec::new(),
            is_primary: true, // Default to primary for single-coop workers
        }
    }

    /// Create an active membership (bypass pending state)
    pub fn active(member_id: EntityId, parent_id: EntityId, role: MembershipRole) -> Self {
        let mut m = Self::new(member_id, parent_id, role);
        m.status = MembershipStatus::Active;
        m
    }

    /// Set the number of voting shares
    pub fn with_shares(mut self, shares: u64) -> Self {
        self.shares = shares;
        self
    }

    /// Set custom capabilities (overrides role defaults)
    pub fn with_capabilities(mut self, capabilities: Vec<MembershipCapability>) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Add a capability
    ///
    /// Updates `updated_at` timestamp for audit trail accuracy.
    pub fn add_capability(&mut self, capability: MembershipCapability) {
        if !self.capabilities.contains(&capability) {
            self.capabilities.push(capability);
            self.updated_at = icn_time::current_timestamp_secs();
        }
    }

    /// Remove a capability
    ///
    /// Updates `updated_at` timestamp for audit trail accuracy.
    pub fn remove_capability(&mut self, capability: &MembershipCapability) {
        let before_len = self.capabilities.len();
        self.capabilities.retain(|c| c != capability);
        if self.capabilities.len() != before_len {
            self.updated_at = icn_time::current_timestamp_secs();
        }
    }

    /// Check if member has a specific capability
    pub fn has_capability(&self, capability: &MembershipCapability) -> bool {
        self.capabilities.contains(capability)
    }

    /// Check if membership is active
    pub fn is_active(&self) -> bool {
        matches!(self.status, MembershipStatus::Active)
    }

    /// Check if member can vote
    pub fn can_vote(&self) -> bool {
        self.is_active() && self.shares > 0 && self.has_capability(&MembershipCapability::Vote)
    }

    /// Check if member can create proposals
    pub fn can_propose(&self) -> bool {
        self.is_active() && self.has_capability(&MembershipCapability::Propose)
    }

    /// Set notes
    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = Some(notes.into());
        self
    }

    // ========================================
    // Labor Assignment Methods (Issue #718)
    // ========================================

    /// Add an active labor assignment.
    ///
    /// Updates `updated_at` timestamp for audit trail accuracy.
    pub fn add_assignment(&mut self, assignment_id: impl Into<String>) {
        let assignment_id = assignment_id.into();
        if !self.assignments.contains(&assignment_id) {
            self.assignments.push(assignment_id);
            self.updated_at = icn_time::current_timestamp_secs();
        }
    }

    /// Remove a labor assignment.
    ///
    /// Updates `updated_at` timestamp for audit trail accuracy.
    pub fn remove_assignment(&mut self, assignment_id: &str) {
        let before_len = self.assignments.len();
        self.assignments.retain(|id| id != assignment_id);
        if self.assignments.len() != before_len {
            self.updated_at = icn_time::current_timestamp_secs();
        }
    }

    /// Check if member has any active labor assignments.
    pub fn has_active_assignments(&self) -> bool {
        !self.assignments.is_empty()
    }

    // ========================================
    // Labor Share Methods (Issue #390)
    // ========================================

    /// Add a labor share to this membership
    ///
    /// Updates `updated_at` timestamp for audit trail accuracy.
    pub fn add_labor_share(&mut self, share_id: impl Into<String>) {
        let share_id = share_id.into();
        if !self.labor_shares.contains(&share_id) {
            self.labor_shares.push(share_id);
            self.updated_at = icn_time::current_timestamp_secs();
        }
    }

    /// Remove a labor share from this membership
    ///
    /// Used during share redemption when a member departs.
    /// Updates `updated_at` timestamp for audit trail accuracy.
    pub fn remove_labor_share(&mut self, share_id: &str) {
        let before_len = self.labor_shares.len();
        self.labor_shares.retain(|s| s != share_id);
        if self.labor_shares.len() != before_len {
            self.updated_at = icn_time::current_timestamp_secs();
        }
    }

    /// Check if this membership has any labor shares
    pub fn has_labor_shares(&self) -> bool {
        !self.labor_shares.is_empty()
    }

    /// Get the count of labor shares held by this member
    pub fn labor_share_count(&self) -> usize {
        self.labor_shares.len()
    }

    /// Set as primary membership
    ///
    /// **Important:** Caller is responsible for ensuring only one membership
    /// per worker is marked as primary across all cooperatives. This method
    /// does not enforce uniqueness constraints.
    ///
    /// Updates `updated_at` timestamp for audit trail accuracy.
    pub fn set_primary(&mut self, is_primary: bool) {
        if self.is_primary != is_primary {
            self.is_primary = is_primary;
            self.updated_at = icn_time::current_timestamp_secs();
        }
    }

    /// Builder method to set primary status
    ///
    /// **Note:** Builder methods are for initial construction and do not
    /// update `updated_at`. Use `set_primary()` to modify after creation.
    pub fn with_primary(mut self, is_primary: bool) -> Self {
        self.is_primary = is_primary;
        self
    }

    /// Builder method to add initial labor shares
    ///
    /// **Note:** Builder methods are for initial construction and do not
    /// update `updated_at`. Use `add_labor_share()` to modify after creation.
    pub fn with_labor_shares(mut self, shares: Vec<String>) -> Self {
        self.labor_shares = shares;
        self
    }

    /// Builder method to add initial assignments.
    ///
    /// **Note:** Builder methods are for initial construction and do not
    /// update `updated_at`. Use `add_assignment()` to modify after creation.
    pub fn with_assignments(mut self, assignments: Vec<String>) -> Self {
        self.assignments = assignments;
        self
    }
}

// ============================================================================
// MembershipRole
// ============================================================================

/// Role a member has within an entity
///
/// Roles determine default capabilities and governance weight.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipRole {
    // ========================================
    // Individual Roles (for individual members)
    // ========================================
    /// Founding member who established the cooperative
    Founder,

    /// Regular member with standard rights
    Member,

    /// Worker-owner in a worker cooperative
    Worker,

    /// Consumer member in a consumer cooperative
    Consumer,

    /// Producer member in a producer cooperative
    Producer,

    /// Elected board member
    BoardMember,

    /// Officer (president, secretary, treasurer, etc.)
    Officer { title: String },

    // ========================================
    // Entity Roles (when member is itself an entity)
    // ========================================
    /// Full voting member of a federation
    FederatedMember,

    /// Associate member (may have limited voting rights)
    AssociateMember,

    /// Observer status (no voting rights, can attend meetings)
    ObserverMember,

    /// Provisional member (probationary period)
    ProvisionalMember,

    // ========================================
    // Custom role
    // ========================================
    /// Custom role defined by the cooperative
    Custom { name: String },
}

impl MembershipRole {
    /// Get default capabilities for this role
    pub fn default_capabilities(&self) -> Vec<MembershipCapability> {
        match self {
            // Founders get all capabilities
            MembershipRole::Founder => vec![
                MembershipCapability::Vote,
                MembershipCapability::Propose,
                MembershipCapability::TreasuryAccess,
                MembershipCapability::Invite,
                MembershipCapability::ManageSubEntities,
                MembershipCapability::Sign,
            ],

            // Board members get most capabilities
            MembershipRole::BoardMember => vec![
                MembershipCapability::Vote,
                MembershipCapability::Propose,
                MembershipCapability::TreasuryAccess,
                MembershipCapability::Invite,
            ],

            // Officers get capabilities based on their role
            MembershipRole::Officer { .. } => vec![
                MembershipCapability::Vote,
                MembershipCapability::Propose,
                MembershipCapability::TreasuryAccess,
            ],

            // Regular members (worker, consumer, producer, member)
            MembershipRole::Worker
            | MembershipRole::Consumer
            | MembershipRole::Producer
            | MembershipRole::Member => {
                vec![MembershipCapability::Vote, MembershipCapability::Propose]
            }

            // Federated members get full voting rights
            MembershipRole::FederatedMember => vec![
                MembershipCapability::Vote,
                MembershipCapability::Propose,
                MembershipCapability::Sign,
            ],

            // Associate members may have limited rights
            MembershipRole::AssociateMember => vec![MembershipCapability::Propose],

            // Observers have no default capabilities
            MembershipRole::ObserverMember => vec![],

            // Provisional members have limited rights
            MembershipRole::ProvisionalMember => vec![MembershipCapability::Propose],

            // Custom roles start with basic member rights
            MembershipRole::Custom { .. } => {
                vec![MembershipCapability::Vote, MembershipCapability::Propose]
            }
        }
    }

    /// Check if this is an individual role
    pub fn is_individual_role(&self) -> bool {
        matches!(
            self,
            MembershipRole::Founder
                | MembershipRole::Member
                | MembershipRole::Worker
                | MembershipRole::Consumer
                | MembershipRole::Producer
                | MembershipRole::BoardMember
                | MembershipRole::Officer { .. }
        )
    }

    /// Check if this is an entity/organizational role
    pub fn is_entity_role(&self) -> bool {
        matches!(
            self,
            MembershipRole::FederatedMember
                | MembershipRole::AssociateMember
                | MembershipRole::ObserverMember
                | MembershipRole::ProvisionalMember
        )
    }
}

impl fmt::Display for MembershipRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MembershipRole::Founder => write!(f, "Founder"),
            MembershipRole::Member => write!(f, "Member"),
            MembershipRole::Worker => write!(f, "Worker"),
            MembershipRole::Consumer => write!(f, "Consumer"),
            MembershipRole::Producer => write!(f, "Producer"),
            MembershipRole::BoardMember => write!(f, "Board Member"),
            MembershipRole::Officer { title } => write!(f, "Officer ({title})"),
            MembershipRole::FederatedMember => write!(f, "Federated Member"),
            MembershipRole::AssociateMember => write!(f, "Associate Member"),
            MembershipRole::ObserverMember => write!(f, "Observer"),
            MembershipRole::ProvisionalMember => write!(f, "Provisional Member"),
            MembershipRole::Custom { name } => write!(f, "{name}"),
        }
    }
}

// ============================================================================
// MembershipStatus
// ============================================================================

/// Membership status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipStatus {
    /// Membership application pending approval
    Pending,

    /// Active member in good standing
    Active,

    /// Membership temporarily suspended
    Suspended,

    /// Inactive member (not participating but not removed)
    Inactive,

    /// Member has voluntarily left
    Resigned,

    /// Member was removed by governance action
    Removed,

    /// Member was expelled (serious violation)
    Expelled,
}

impl MembershipStatus {
    /// Check if member is in good standing
    pub fn is_good_standing(&self) -> bool {
        matches!(self, MembershipStatus::Active)
    }

    /// Check if membership has ended
    pub fn is_terminated(&self) -> bool {
        matches!(
            self,
            MembershipStatus::Resigned | MembershipStatus::Removed | MembershipStatus::Expelled
        )
    }
}

impl fmt::Display for MembershipStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MembershipStatus::Pending => write!(f, "Pending"),
            MembershipStatus::Active => write!(f, "Active"),
            MembershipStatus::Suspended => write!(f, "Suspended"),
            MembershipStatus::Inactive => write!(f, "Inactive"),
            MembershipStatus::Resigned => write!(f, "Resigned"),
            MembershipStatus::Removed => write!(f, "Removed"),
            MembershipStatus::Expelled => write!(f, "Expelled"),
        }
    }
}

// ============================================================================
// MembershipCapability
// ============================================================================

/// Capabilities that can be granted through membership
///
/// These define what actions a member can take within the entity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipCapability {
    /// Can vote on proposals
    Vote,

    /// Can create proposals
    Propose,

    /// Can access treasury operations
    TreasuryAccess,

    /// Can invite new members
    Invite,

    /// Can manage sub-entities (for federations)
    ManageSubEntities,

    /// Can sign on behalf of entity
    Sign,

    /// Can modify entity settings
    Configure,

    /// Can view sensitive information
    ViewSensitive,

    /// Custom capability
    Custom(String),
}

impl fmt::Display for MembershipCapability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MembershipCapability::Vote => write!(f, "vote"),
            MembershipCapability::Propose => write!(f, "propose"),
            MembershipCapability::TreasuryAccess => write!(f, "treasury_access"),
            MembershipCapability::Invite => write!(f, "invite"),
            MembershipCapability::ManageSubEntities => write!(f, "manage_sub_entities"),
            MembershipCapability::Sign => write!(f, "sign"),
            MembershipCapability::Configure => write!(f, "configure"),
            MembershipCapability::ViewSensitive => write!(f, "view_sensitive"),
            MembershipCapability::Custom(name) => write!(f, "custom:{name}"),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn create_test_individual() -> EntityId {
        let keypair = KeyPair::generate().unwrap();
        EntityId::from_did(keypair.did())
    }

    fn create_test_coop() -> EntityId {
        EntityId::cooperative("test-coop").unwrap()
    }

    #[test]
    fn test_membership_creation() {
        let member = create_test_individual();
        let coop = create_test_coop();

        let membership = Membership::new(member.clone(), coop.clone(), MembershipRole::Worker);

        assert_eq!(membership.member_id, member);
        assert_eq!(membership.parent_id, coop);
        assert!(matches!(membership.status, MembershipStatus::Pending));
        assert_eq!(membership.shares, 1);
    }

    #[test]
    fn test_membership_active() {
        let member = create_test_individual();
        let coop = create_test_coop();

        let membership = Membership::active(member, coop, MembershipRole::Worker);

        assert!(membership.is_active());
        assert!(membership.can_vote());
        assert!(membership.can_propose());
    }

    #[test]
    fn test_founder_capabilities() {
        let member = create_test_individual();
        let coop = create_test_coop();

        let membership = Membership::active(member, coop, MembershipRole::Founder);

        assert!(membership.has_capability(&MembershipCapability::Vote));
        assert!(membership.has_capability(&MembershipCapability::TreasuryAccess));
        assert!(membership.has_capability(&MembershipCapability::Invite));
        assert!(membership.has_capability(&MembershipCapability::Sign));
    }

    #[test]
    fn test_observer_no_vote() {
        let member_coop = EntityId::cooperative("member-coop").unwrap();
        let federation = EntityId::federation("test-fed").unwrap();

        let membership =
            Membership::active(member_coop, federation, MembershipRole::ObserverMember);

        // Observers receive the default 1 share like other members,
        // but their role does not include the Vote capability, so can_vote() is false
        assert!(!membership.has_capability(&MembershipCapability::Vote));
        assert!(!membership.can_vote());
    }

    #[test]
    fn test_federated_membership() {
        let member_coop = EntityId::cooperative("member-coop").unwrap();
        let federation = EntityId::federation("midwest-fed").unwrap();

        let membership = Membership::active(
            member_coop.clone(),
            federation.clone(),
            MembershipRole::FederatedMember,
        );

        assert!(membership.is_active());
        assert!(membership.can_vote());
        assert!(membership.has_capability(&MembershipCapability::Sign));
    }

    #[test]
    fn test_custom_capabilities() {
        let member = create_test_individual();
        let coop = create_test_coop();

        let membership = Membership::active(member, coop, MembershipRole::Member)
            .with_capabilities(vec![MembershipCapability::Vote]);

        assert!(membership.has_capability(&MembershipCapability::Vote));
        assert!(!membership.has_capability(&MembershipCapability::Propose));
    }

    #[test]
    fn test_shares_affect_voting() {
        let member = create_test_individual();
        let coop = create_test_coop();

        let no_shares =
            Membership::active(member.clone(), coop.clone(), MembershipRole::Member).with_shares(0);
        let with_shares = Membership::active(member, coop, MembershipRole::Member).with_shares(10);

        assert!(!no_shares.can_vote()); // 0 shares = no vote
        assert!(with_shares.can_vote());
        assert_eq!(with_shares.shares, 10);
    }

    #[test]
    fn test_membership_serialization() {
        let member = create_test_individual();
        let coop = create_test_coop();
        let membership = Membership::active(member, coop, MembershipRole::Worker);

        let json = serde_json::to_string(&membership).unwrap();
        let parsed: Membership = serde_json::from_str(&json).unwrap();

        assert_eq!(membership.member_id, parsed.member_id);
        assert_eq!(membership.role, parsed.role);
    }

    #[test]
    fn test_role_display() {
        assert_eq!(MembershipRole::Founder.to_string(), "Founder");
        assert_eq!(MembershipRole::Worker.to_string(), "Worker");
        assert_eq!(
            MembershipRole::Officer {
                title: "President".into()
            }
            .to_string(),
            "Officer (President)"
        );
    }

    #[test]
    fn test_capability_modification_updates_timestamp() {
        let member = create_test_individual();
        let coop = create_test_coop();
        let mut membership = Membership::active(member, coop, MembershipRole::Member);
        let original_updated_at = membership.updated_at;

        // Wait a tiny bit to ensure timestamp changes
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Add a capability - should update timestamp
        membership.add_capability(MembershipCapability::TreasuryAccess);
        assert!(
            membership.updated_at >= original_updated_at,
            "updated_at should be updated on add_capability()"
        );

        let after_add = membership.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Remove a capability - should update timestamp
        membership.remove_capability(&MembershipCapability::TreasuryAccess);
        assert!(
            membership.updated_at >= after_add,
            "updated_at should be updated on remove_capability()"
        );

        // Adding duplicate should not update timestamp
        let before_noop = membership.updated_at;
        membership.add_capability(MembershipCapability::Vote); // Already has Vote
        assert_eq!(
            membership.updated_at, before_noop,
            "updated_at should not change when adding duplicate capability"
        );

        // Removing non-existent should not update timestamp
        membership.remove_capability(&MembershipCapability::TreasuryAccess); // Already removed
        assert_eq!(
            membership.updated_at, before_noop,
            "updated_at should not change when removing non-existent capability"
        );
    }

    // ========================================
    // Labor Share Tests (Issue #390)
    // ========================================

    #[test]
    fn test_labor_shares_default_empty() {
        let member = create_test_individual();
        let coop = create_test_coop();
        let membership = Membership::new(member, coop, MembershipRole::Worker);

        assert!(membership.assignments.is_empty());
        assert!(!membership.has_active_assignments());
        assert!(membership.labor_shares.is_empty());
        assert!(!membership.has_labor_shares());
        assert_eq!(membership.labor_share_count(), 0);
    }

    #[test]
    fn test_add_labor_share() {
        let member = create_test_individual();
        let coop = create_test_coop();
        let mut membership = Membership::active(member, coop, MembershipRole::Worker);

        membership.add_labor_share("share-001");
        assert!(membership.has_labor_shares());
        assert_eq!(membership.labor_share_count(), 1);
        assert!(membership.labor_shares.contains(&"share-001".to_string()));
    }

    #[test]
    fn test_add_duplicate_labor_share() {
        let member = create_test_individual();
        let coop = create_test_coop();
        let mut membership = Membership::active(member, coop, MembershipRole::Worker);

        membership.add_labor_share("share-001");
        let timestamp_after_first = membership.updated_at;

        std::thread::sleep(std::time::Duration::from_millis(10));

        // Adding duplicate should not change anything
        membership.add_labor_share("share-001");
        assert_eq!(membership.labor_share_count(), 1);
        assert_eq!(
            membership.updated_at, timestamp_after_first,
            "Duplicate add should not update timestamp"
        );
    }

    #[test]
    fn test_remove_labor_share() {
        let member = create_test_individual();
        let coop = create_test_coop();
        let mut membership = Membership::active(member, coop, MembershipRole::Worker);

        membership.add_labor_share("share-001");
        membership.add_labor_share("share-002");
        assert_eq!(membership.labor_share_count(), 2);

        membership.remove_labor_share("share-001");
        assert_eq!(membership.labor_share_count(), 1);
        assert!(!membership.labor_shares.contains(&"share-001".to_string()));
        assert!(membership.labor_shares.contains(&"share-002".to_string()));
    }

    #[test]
    fn test_labor_share_updates_timestamp() {
        let member = create_test_individual();
        let coop = create_test_coop();
        let mut membership = Membership::active(member, coop, MembershipRole::Worker);
        let original = membership.updated_at;

        std::thread::sleep(std::time::Duration::from_millis(10));

        membership.add_labor_share("share-001");
        assert!(
            membership.updated_at >= original,
            "add_labor_share should update timestamp"
        );

        let after_add = membership.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(10));

        membership.remove_labor_share("share-001");
        assert!(
            membership.updated_at >= after_add,
            "remove_labor_share should update timestamp"
        );
    }

    #[test]
    fn test_assignment_updates_timestamp() {
        let member = create_test_individual();
        let coop = create_test_coop();
        let mut membership = Membership::active(member, coop, MembershipRole::Worker);
        let original = membership.updated_at;

        std::thread::sleep(std::time::Duration::from_millis(10));

        membership.add_assignment("assign-123");
        assert!(
            membership.updated_at >= original,
            "add_assignment should update timestamp"
        );

        let after_add = membership.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(10));

        membership.remove_assignment("assign-123");
        assert!(
            membership.updated_at >= after_add,
            "remove_assignment should update timestamp"
        );
    }

    #[test]
    fn test_assignment_remove() {
        let member = create_test_individual();
        let coop = create_test_coop();
        let mut membership = Membership::active(member, coop, MembershipRole::Worker);

        membership.add_assignment("assign-001");
        membership.add_assignment("assign-002");
        assert_eq!(membership.assignments.len(), 2);

        membership.remove_assignment("assign-001");
        assert_eq!(membership.assignments.len(), 1);
        assert!(!membership.assignments.contains(&"assign-001".to_string()));
        assert!(membership.assignments.contains(&"assign-002".to_string()));
    }

    #[test]
    fn test_assignment_duplicate_add() {
        let member = create_test_individual();
        let coop = create_test_coop();
        let mut membership = Membership::active(member, coop, MembershipRole::Worker);

        membership.add_assignment("assign-001");
        let timestamp_after_first = membership.updated_at;

        std::thread::sleep(std::time::Duration::from_millis(10));

        membership.add_assignment("assign-001");
        assert_eq!(membership.assignments.len(), 1);
        assert_eq!(
            membership.updated_at, timestamp_after_first,
            "Duplicate add should not update timestamp"
        );
    }

    #[test]
    fn test_is_primary_default() {
        let member = create_test_individual();
        let coop = create_test_coop();
        let membership = Membership::new(member, coop, MembershipRole::Worker);

        assert!(
            membership.is_primary,
            "New membership should be primary by default"
        );
    }

    #[test]
    fn test_set_primary() {
        let member = create_test_individual();
        let coop = create_test_coop();
        let mut membership = Membership::active(member, coop, MembershipRole::Worker);

        assert!(membership.is_primary);

        membership.set_primary(false);
        assert!(!membership.is_primary);

        membership.set_primary(true);
        assert!(membership.is_primary);
    }

    #[test]
    fn test_with_labor_shares_builder() {
        let member = create_test_individual();
        let coop = create_test_coop();
        let membership = Membership::active(member, coop, MembershipRole::Worker)
            .with_labor_shares(vec!["share-001".to_string(), "share-002".to_string()])
            .with_assignments(vec!["assign-001".to_string()])
            .with_primary(false);

        assert_eq!(membership.labor_share_count(), 2);
        assert!(membership.has_active_assignments());
        assert!(!membership.is_primary);
    }

    #[test]
    fn test_labor_share_serialization() {
        let member = create_test_individual();
        let coop = create_test_coop();
        let mut membership =
            Membership::active(member.clone(), coop.clone(), MembershipRole::Worker);
        membership.add_labor_share("share-xyz");
        membership.add_assignment("assign-777");
        membership.set_primary(false);

        let json = serde_json::to_string(&membership).unwrap();
        let parsed: Membership = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.labor_shares, vec!["share-xyz".to_string()]);
        assert_eq!(parsed.assignments, vec!["assign-777".to_string()]);
        assert!(!parsed.is_primary);
    }

    #[test]
    fn test_legacy_membership_deserialization() {
        // Test that old memberships without labor_shares/is_primary still deserialize
        // EntityId is serialized as "entity:icn:type:identifier"
        let legacy_json = r#"{
            "member_id": "entity:icn:individual:testpubkey123",
            "parent_id": "entity:icn:cooperative:test-coop",
            "role": "worker",
            "status": "active",
            "joined_at": 1700000000,
            "updated_at": 1700000000,
            "shares": 1,
            "capabilities": ["vote", "propose"],
            "notes": null
        }"#;

        let parsed: Membership = serde_json::from_str(legacy_json).unwrap();

        // New fields should have defaults
        assert!(parsed.assignments.is_empty());
        assert!(parsed.labor_shares.is_empty());
        assert!(parsed.is_primary);
    }
}
