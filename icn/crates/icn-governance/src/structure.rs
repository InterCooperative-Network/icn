//! Internal organizational structures (committees, working groups, teams).
//!
//! Structures are NOT sovereign entities. They exist inside a parent entity
//! (typically a cooperative or community) with delegated authority. They cannot
//! join federations, hold independent treasury, or persist without their parent.
//!
//! See `docs/design/institutional-structure-spec.md` for the full design.

use crate::{GovernanceError, ProposalId, Timestamp};
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use uuid::Uuid;

/// Unique identifier for a structure
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StructureId(pub String);

impl StructureId {
    /// Create a new random structure ID
    pub fn generate() -> Self {
        Self(format!("struct-{}", Uuid::new_v4()))
    }

    /// Create from a raw string identifier (useful for deterministic IDs like "nycn-steering")
    pub fn from_raw(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl std::fmt::Display for StructureId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Kind of internal structure.
///
/// All kinds share the same invariant: they are owned by a parent entity and have
/// no independent sovereignty. The kind is descriptive, used for UI/categorization
/// and optionally for role-scope defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructureKind {
    /// A standing committee with a defined mandate (finance, content, steering).
    Committee,
    /// An ad-hoc working group, typically time-bounded or goal-bounded.
    WorkingGroup,
    /// A functional team within a cooperative (engineering team, design team).
    Team,
    /// An office or officer role (e.g., a secretary office).
    Office,
}

/// Lifecycle status of a structure.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructureStatus {
    /// Active and operational.
    #[default]
    Active,
    /// Temporarily paused; roles remain assigned but work is deferred.
    Suspended,
    /// Permanently dissolved. Should not be revived — create a new structure instead.
    Dissolved,
}

/// An internal organizational structure owned by a parent entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Structure {
    /// Unique identifier
    pub id: StructureId,

    /// Entity ID of the parent (cooperative, community, or federation).
    /// Using `String` rather than `EntityId` to avoid a cross-crate dependency on `icn-entity`.
    pub parent_entity_id: String,

    /// Kind of structure
    pub kind: StructureKind,

    /// Display name (e.g., "Finance Committee", "Content Working Group")
    pub name: String,

    /// Optional mandate describing what this structure is authorized to do
    #[serde(default)]
    pub mandate: Option<String>,

    /// Delegated authority scopes (e.g., ["approve_sponsorship_below_500", "schedule_sessions"])
    #[serde(default)]
    pub scope: Vec<String>,

    /// Current lifecycle status
    #[serde(default)]
    pub status: StructureStatus,

    /// Unix timestamp when the structure was created
    pub created_at: Timestamp,

    /// If proposal-backed, the proposal that authorized creation of this structure
    #[serde(default)]
    pub created_by_decision: Option<ProposalId>,

    /// Unix timestamp of last modification
    pub updated_at: Timestamp,
}

impl Structure {
    /// Create a new active structure with minimal required fields.
    pub fn new(
        id: StructureId,
        parent_entity_id: String,
        kind: StructureKind,
        name: String,
        now: Timestamp,
    ) -> Self {
        Self {
            id,
            parent_entity_id,
            kind,
            name,
            mandate: None,
            scope: Vec::new(),
            status: StructureStatus::Active,
            created_at: now,
            created_by_decision: None,
            updated_at: now,
        }
    }

    /// Whether this structure is currently active (not suspended or dissolved).
    pub fn is_active(&self) -> bool {
        matches!(self.status, StructureStatus::Active)
    }
}

// ========== Role Assignments ==========

/// Unique identifier for a role assignment within a structure.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RoleAssignmentId(pub Uuid);

impl RoleAssignmentId {
    /// Create a new random role assignment ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create from a UUID
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for RoleAssignmentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for RoleAssignmentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Assignment of a role within a structure to a specific DID.
///
/// Roles carry delegated authority scopes but do NOT grant sovereignty.
/// A facilitator role, for example, does not imply governance authority — only
/// coordination. Authority scopes must be explicit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleAssignment {
    /// Unique identifier for this assignment
    pub id: RoleAssignmentId,

    /// The structure this role belongs to
    pub structure_id: StructureId,

    /// DID of the person holding this role
    pub person_did: Did,

    /// Role name (e.g., "coordinator", "note_taker", "facilitator", "member")
    pub role: String,

    /// Delegated authority scopes for this specific role assignment.
    /// Narrower than or equal to the structure's scope.
    #[serde(default)]
    pub authority_scope: Vec<String>,

    /// Unix timestamp when this assignment started
    pub start_date: Timestamp,

    /// Unix timestamp when this assignment ends (None = indefinite)
    #[serde(default)]
    pub end_date: Option<Timestamp>,

    /// If proposal-backed, the proposal that authorized this role assignment
    #[serde(default)]
    pub assigned_by_decision: Option<ProposalId>,
}

impl RoleAssignment {
    /// Create a new role assignment with minimal required fields.
    pub fn new(structure_id: StructureId, person_did: Did, role: String, now: Timestamp) -> Self {
        Self {
            id: RoleAssignmentId::new(),
            structure_id,
            person_did,
            role,
            authority_scope: Vec::new(),
            start_date: now,
            end_date: None,
            assigned_by_decision: None,
        }
    }

    /// Whether this role assignment is active at the given timestamp.
    pub fn is_active_at(&self, at: Timestamp) -> bool {
        if at < self.start_date {
            return false;
        }
        match self.end_date {
            Some(end) => at < end,
            None => true,
        }
    }

    /// Compatibility bridge from the stringly-typed `authority_scope` to
    /// the typed [`crate::authority::TypedScope`] frozen by ADR-0014.
    ///
    /// This is a read-only projection: the underlying `Vec<String>`
    /// field is unchanged. Unrecognized labels are silently ignored. A
    /// return value of `None` means no label in the current scope matched
    /// any recognized form — i.e. the typed projection would be empty.
    ///
    /// See [`crate::authority::parse_authority_scope_strings`] for the
    /// grammar and the extensibility story.
    ///
    /// This helper adds no behavioral enforcement: existing consumers
    /// that read `authority_scope` directly continue to work unchanged.
    /// Future consumers that want a typed view can call this method
    /// without forcing a migration of any call site today.
    pub fn typed_scope(&self) -> Option<crate::authority::TypedScope> {
        crate::authority::parse_authority_scope_strings(&self.authority_scope)
    }
}

// ========== Store Backend ==========

/// Storage backend trait for structures and role assignments.
pub trait StructureStoreBackend: Send + Sync {
    /// Save (create or update) a structure.
    fn save_structure(&self, s: &Structure) -> std::result::Result<(), GovernanceError>;

    /// Retrieve a structure by ID.
    fn get_structure(
        &self,
        id: &StructureId,
    ) -> std::result::Result<Option<Structure>, GovernanceError>;

    /// List all structures owned by a given parent entity.
    fn list_structures_by_entity(
        &self,
        entity_id: &str,
    ) -> std::result::Result<Vec<Structure>, GovernanceError>;

    /// Delete a structure (hard delete — use `Dissolved` status for soft-dissolution).
    fn delete_structure(&self, id: &StructureId) -> std::result::Result<bool, GovernanceError>;

    /// Save (create or update) a role assignment.
    fn save_role(&self, r: &RoleAssignment) -> std::result::Result<(), GovernanceError>;

    /// Retrieve a role assignment by ID.
    fn get_role(
        &self,
        id: &RoleAssignmentId,
    ) -> std::result::Result<Option<RoleAssignment>, GovernanceError>;

    /// List all role assignments for a given structure.
    fn list_roles_by_structure(
        &self,
        sid: &StructureId,
    ) -> std::result::Result<Vec<RoleAssignment>, GovernanceError>;

    /// Delete a role assignment.
    fn delete_role(&self, id: &RoleAssignmentId) -> std::result::Result<bool, GovernanceError>;

    /// List all role assignments held by a given person (cross-structure).
    fn list_roles_by_person(
        &self,
        did: &icn_identity::Did,
    ) -> std::result::Result<Vec<RoleAssignment>, GovernanceError>;
}

// ========== In-Memory Store (for tests and default config) ==========

/// In-memory implementation of [`StructureStoreBackend`]. Primarily for tests
/// and default-configured actors without persistent storage.
#[derive(Default)]
pub struct InMemoryStructureStore {
    structures: RwLock<HashMap<StructureId, Structure>>,
    roles: RwLock<HashMap<RoleAssignmentId, RoleAssignment>>,
}

impl InMemoryStructureStore {
    /// Create a new empty in-memory store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl StructureStoreBackend for InMemoryStructureStore {
    fn save_structure(&self, s: &Structure) -> std::result::Result<(), GovernanceError> {
        let mut guard = self
            .structures
            .write()
            .map_err(|e| GovernanceError::Internal(format!("structures lock poisoned: {e}")))?;
        guard.insert(s.id.clone(), s.clone());
        Ok(())
    }

    fn get_structure(
        &self,
        id: &StructureId,
    ) -> std::result::Result<Option<Structure>, GovernanceError> {
        let guard = self
            .structures
            .read()
            .map_err(|e| GovernanceError::Internal(format!("structures lock poisoned: {e}")))?;
        Ok(guard.get(id).cloned())
    }

    fn list_structures_by_entity(
        &self,
        entity_id: &str,
    ) -> std::result::Result<Vec<Structure>, GovernanceError> {
        let guard = self
            .structures
            .read()
            .map_err(|e| GovernanceError::Internal(format!("structures lock poisoned: {e}")))?;
        let mut out: Vec<Structure> = guard
            .values()
            .filter(|s| s.parent_entity_id == entity_id)
            .cloned()
            .collect();
        out.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        Ok(out)
    }

    fn delete_structure(&self, id: &StructureId) -> std::result::Result<bool, GovernanceError> {
        // Cascade: remove all role assignments for this structure first
        {
            let mut roles_guard = self
                .roles
                .write()
                .map_err(|e| GovernanceError::Internal(format!("roles lock poisoned: {e}")))?;
            roles_guard.retain(|_, r| r.structure_id != *id);
        }
        let mut guard = self
            .structures
            .write()
            .map_err(|e| GovernanceError::Internal(format!("structures lock poisoned: {e}")))?;
        Ok(guard.remove(id).is_some())
    }

    fn save_role(&self, r: &RoleAssignment) -> std::result::Result<(), GovernanceError> {
        let mut guard = self
            .roles
            .write()
            .map_err(|e| GovernanceError::Internal(format!("roles lock poisoned: {e}")))?;
        guard.insert(r.id.clone(), r.clone());
        Ok(())
    }

    fn get_role(
        &self,
        id: &RoleAssignmentId,
    ) -> std::result::Result<Option<RoleAssignment>, GovernanceError> {
        let guard = self
            .roles
            .read()
            .map_err(|e| GovernanceError::Internal(format!("roles lock poisoned: {e}")))?;
        Ok(guard.get(id).cloned())
    }

    fn list_roles_by_structure(
        &self,
        sid: &StructureId,
    ) -> std::result::Result<Vec<RoleAssignment>, GovernanceError> {
        let guard = self
            .roles
            .read()
            .map_err(|e| GovernanceError::Internal(format!("roles lock poisoned: {e}")))?;
        let mut out: Vec<RoleAssignment> = guard
            .values()
            .filter(|r| r.structure_id == *sid)
            .cloned()
            .collect();
        out.sort_by_key(|a| a.start_date);
        Ok(out)
    }

    fn delete_role(&self, id: &RoleAssignmentId) -> std::result::Result<bool, GovernanceError> {
        let mut guard = self
            .roles
            .write()
            .map_err(|e| GovernanceError::Internal(format!("roles lock poisoned: {e}")))?;
        Ok(guard.remove(id).is_some())
    }

    fn list_roles_by_person(
        &self,
        did: &icn_identity::Did,
    ) -> std::result::Result<Vec<RoleAssignment>, GovernanceError> {
        let guard = self
            .roles
            .read()
            .map_err(|e| GovernanceError::Internal(format!("roles lock poisoned: {e}")))?;
        let mut out: Vec<RoleAssignment> = guard
            .values()
            .filter(|r| &r.person_did == did)
            .cloned()
            .collect();
        out.sort_by_key(|a| a.start_date);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::Did;

    fn test_did() -> Did {
        Did::from_anchor_id(&[1; 32])
    }

    #[test]
    fn test_structure_lifecycle() {
        let store = InMemoryStructureStore::new();
        let id = StructureId::from_raw("nycn-finance");
        let s = Structure::new(
            id.clone(),
            "nycn-organizers".to_string(),
            StructureKind::Committee,
            "Finance Committee".to_string(),
            1000,
        );

        store.save_structure(&s).unwrap();

        let retrieved = store.get_structure(&id).unwrap();
        assert!(retrieved.is_some());
        let r = retrieved.unwrap();
        assert_eq!(r.name, "Finance Committee");
        assert!(r.is_active());
        assert_eq!(r.parent_entity_id, "nycn-organizers");
    }

    #[test]
    fn test_list_structures_by_entity() {
        let store = InMemoryStructureStore::new();
        let s1 = Structure::new(
            StructureId::from_raw("nycn-content"),
            "nycn-organizers".to_string(),
            StructureKind::Committee,
            "Content".to_string(),
            1000,
        );
        let s2 = Structure::new(
            StructureId::from_raw("nycn-finance"),
            "nycn-organizers".to_string(),
            StructureKind::Committee,
            "Finance".to_string(),
            1000,
        );
        let s3 = Structure::new(
            StructureId::from_raw("greenstar-worker-group"),
            "greenstar".to_string(),
            StructureKind::WorkingGroup,
            "Workers".to_string(),
            1000,
        );

        store.save_structure(&s1).unwrap();
        store.save_structure(&s2).unwrap();
        store.save_structure(&s3).unwrap();

        let nycn_structures = store.list_structures_by_entity("nycn-organizers").unwrap();
        assert_eq!(nycn_structures.len(), 2);

        let greenstar_structures = store.list_structures_by_entity("greenstar").unwrap();
        assert_eq!(greenstar_structures.len(), 1);

        let empty = store.list_structures_by_entity("nonexistent").unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn test_structure_delete() {
        let store = InMemoryStructureStore::new();
        let id = StructureId::from_raw("temp");
        let s = Structure::new(
            id.clone(),
            "parent".to_string(),
            StructureKind::Team,
            "Temp".to_string(),
            1000,
        );
        store.save_structure(&s).unwrap();
        assert!(store.delete_structure(&id).unwrap());
        assert!(store.get_structure(&id).unwrap().is_none());
        assert!(!store.delete_structure(&id).unwrap()); // idempotent
    }

    #[test]
    fn test_role_assignment_lifecycle() {
        let store = InMemoryStructureStore::new();
        let sid = StructureId::from_raw("nycn-finance");

        let role = RoleAssignment::new(sid.clone(), test_did(), "coordinator".to_string(), 1000);
        let rid = role.id.clone();
        store.save_role(&role).unwrap();

        let retrieved = store.get_role(&rid).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().role, "coordinator");

        let roles = store.list_roles_by_structure(&sid).unwrap();
        assert_eq!(roles.len(), 1);

        assert!(store.delete_role(&rid).unwrap());
        assert!(store.get_role(&rid).unwrap().is_none());
    }

    #[test]
    fn test_role_active_at() {
        let sid = StructureId::from_raw("s");
        let mut role = RoleAssignment::new(sid, test_did(), "member".to_string(), 1000);

        assert!(!role.is_active_at(500)); // before start
        assert!(role.is_active_at(1000)); // at start (inclusive)
        assert!(role.is_active_at(5000)); // after start, no end

        role.end_date = Some(2000);
        assert!(role.is_active_at(1500));
        assert!(!role.is_active_at(2000)); // at end (exclusive)
        assert!(!role.is_active_at(3000));
    }

    #[test]
    fn test_role_typed_scope_bridge_none_when_empty_or_unrecognized() {
        // ADR-0014 compatibility bridge: empty authority_scope and
        // unrecognized labels both yield None.
        let sid = StructureId::from_raw("s");
        let role = RoleAssignment::new(sid.clone(), test_did(), "coordinator".into(), 1000);
        assert!(role.typed_scope().is_none());

        let mut with_unknown = role.clone();
        with_unknown.authority_scope = vec!["not-a-known-label".into()];
        assert!(with_unknown.typed_scope().is_none());
    }

    #[test]
    fn test_role_typed_scope_bridge_projects_known_labels() {
        // ADR-0014 compatibility bridge: recognized labels project into
        // a typed scope without mutating the underlying string vector.
        let sid = StructureId::from_raw("s");
        let mut role = RoleAssignment::new(sid, test_did(), "treasurer".into(), 1000);
        role.authority_scope = vec![
            "domain:treasury".into(),
            "action_kind:Treasury::Spend".into(),
            "amount_ceiling:500:credit_units".into(),
        ];

        let scope = role.typed_scope().expect("bridge should yield Some");
        assert_eq!(
            scope.domain.as_ref().map(|d| d.0.as_str()),
            Some("treasury")
        );
        assert_eq!(scope.action_kind, vec!["Treasury::Spend".to_string()]);
        assert!(scope.amount_ceiling.is_some());

        // The underlying string vector remains as-is — no migration side-effect.
        assert_eq!(role.authority_scope.len(), 3);
    }

    #[test]
    fn test_list_roles_only_in_structure() {
        let store = InMemoryStructureStore::new();
        let s1 = StructureId::from_raw("s1");
        let s2 = StructureId::from_raw("s2");

        store
            .save_role(&RoleAssignment::new(s1.clone(), test_did(), "a".into(), 1))
            .unwrap();
        store
            .save_role(&RoleAssignment::new(s1.clone(), test_did(), "b".into(), 1))
            .unwrap();
        store
            .save_role(&RoleAssignment::new(s2.clone(), test_did(), "c".into(), 1))
            .unwrap();

        assert_eq!(store.list_roles_by_structure(&s1).unwrap().len(), 2);
        assert_eq!(store.list_roles_by_structure(&s2).unwrap().len(), 1);
    }

    #[test]
    fn test_structure_serde_defaults() {
        // Minimal JSON — all optional fields omitted
        let json = r#"{
            "id": "nycn-finance",
            "parent_entity_id": "nycn-organizers",
            "kind": "committee",
            "name": "Finance",
            "created_at": 1000,
            "updated_at": 1000
        }"#;
        let s: Structure = serde_json::from_str(json).unwrap();
        assert_eq!(s.id.0, "nycn-finance");
        assert!(s.mandate.is_none());
        assert!(s.scope.is_empty());
        assert!(s.is_active());
        assert!(s.created_by_decision.is_none());
    }
}
