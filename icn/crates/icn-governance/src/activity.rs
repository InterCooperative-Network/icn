//! Activities: events, programs, projects, initiatives.
//!
//! Activities are time-bounded or purpose-bounded endeavors owned by an entity.
//! They are NOT sovereign — they cannot govern themselves, join federations, or
//! hold treasury independently. They gather meetings, documents, tasks, and
//! other operational artifacts under a single institutional scope.
//!
//! See `docs/design/institutional-structure-spec.md` for the full design.

use crate::program::ProgramId;
use crate::structure::StructureId;
use crate::{GovernanceError, ProposalId, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use uuid::Uuid;

/// Unique identifier for an activity
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ActivityId(pub String);

impl ActivityId {
    /// Create a new random activity ID
    pub fn generate() -> Self {
        Self(format!("act-{}", Uuid::new_v4()))
    }

    /// Create from a raw string identifier (useful for deterministic IDs like "summit-2026")
    pub fn from_raw(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl std::fmt::Display for ActivityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Kind of activity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    /// A scheduled event (conference, summit, assembly).
    Event,
    /// An ongoing program with multiple phases.
    Program,
    /// A bounded project with a specific deliverable.
    Project,
    /// A campaign or initiative aiming for a change.
    Initiative,
}

/// Lifecycle status of an activity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityStatus {
    /// Being set up; not yet started.
    #[default]
    Planned,
    /// Currently underway.
    Active,
    /// Finished normally.
    Completed,
    /// Ended before completion.
    Cancelled,
}

/// A time-bounded or purpose-bounded activity owned by a parent entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Activity {
    /// Unique identifier
    pub id: ActivityId,

    /// Entity ID of the parent (cooperative, community, or federation).
    /// Using `String` rather than `EntityId` to avoid a cross-crate dependency.
    pub parent_entity_id: String,

    /// Kind of activity
    pub kind: ActivityKind,

    /// Display name (e.g., "New York Cooperative Summit 2026")
    pub name: String,

    /// Optional description
    #[serde(default)]
    pub description: Option<String>,

    /// Current lifecycle status
    #[serde(default)]
    pub status: ActivityStatus,

    /// When the activity starts (optional — some activities have no fixed start)
    #[serde(default)]
    pub start_date: Option<Timestamp>,

    /// When the activity ends (optional — some activities are open-ended)
    #[serde(default)]
    pub end_date: Option<Timestamp>,

    /// Structures (committees, working groups) contributing to this activity.
    /// Linked structures can attach meetings, docs, and action items to the activity.
    #[serde(default)]
    pub linked_structures: Vec<StructureId>,

    /// Unix timestamp when the activity was created
    pub created_at: Timestamp,

    /// If proposal-backed, the proposal that authorized creation
    #[serde(default)]
    pub created_by_decision: Option<ProposalId>,

    /// Optional link to the parent `Program` that this activity executes within.
    ///
    /// An `Event` activity (e.g., "NY Cooperative Summit 2026") can point to the
    /// `Program` that frames it (e.g., "annual-summit-cycle"), enabling cycle-over-cycle
    /// dashboards and the work-spine display without requiring a second round-trip.
    ///
    /// `None` is valid — stand-alone activities with no program affiliation are common.
    ///
    /// **Field ordering note**: This field is intentionally placed AFTER `created_at` and
    /// `created_by_decision`. ICN uses postcard (positional) binary encoding for persistence;
    /// new optional fields must be appended to preserve backward compatibility with records
    /// written before this field existed. Old records decode with `parent_program_id = None`.
    #[serde(default)]
    pub parent_program_id: Option<ProgramId>,
}

impl Activity {
    /// Create a new planned activity with minimal required fields.
    pub fn new(
        id: ActivityId,
        parent_entity_id: String,
        kind: ActivityKind,
        name: String,
        now: Timestamp,
    ) -> Self {
        Self {
            id,
            parent_entity_id,
            kind,
            name,
            description: None,
            status: ActivityStatus::Planned,
            start_date: None,
            end_date: None,
            linked_structures: Vec::new(),
            created_at: now,
            created_by_decision: None,
            parent_program_id: None,
        }
    }

    /// Whether this activity is currently active.
    pub fn is_active(&self) -> bool {
        matches!(self.status, ActivityStatus::Active)
    }
}

// ========== Store Backend ==========

/// Storage backend trait for activities.
pub trait ActivityStoreBackend: Send + Sync {
    /// Save (create or update) an activity.
    fn save(&self, a: &Activity) -> std::result::Result<(), GovernanceError>;

    /// Retrieve an activity by ID.
    fn get(&self, id: &ActivityId) -> std::result::Result<Option<Activity>, GovernanceError>;

    /// List all activities owned by a given parent entity.
    fn list_by_entity(
        &self,
        entity_id: &str,
    ) -> std::result::Result<Vec<Activity>, GovernanceError>;

    /// Delete an activity (hard delete — use `Cancelled` status for soft-cancel).
    fn delete(&self, id: &ActivityId) -> std::result::Result<bool, GovernanceError>;
}

// ========== In-Memory Store (for tests and default config) ==========

/// In-memory implementation of [`ActivityStoreBackend`]. Primarily for tests.
#[derive(Default)]
pub struct InMemoryActivityStore {
    activities: RwLock<HashMap<ActivityId, Activity>>,
}

impl InMemoryActivityStore {
    /// Create a new empty in-memory store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl ActivityStoreBackend for InMemoryActivityStore {
    fn save(&self, a: &Activity) -> std::result::Result<(), GovernanceError> {
        let mut guard = self
            .activities
            .write()
            .map_err(|e| GovernanceError::Internal(format!("activities lock poisoned: {e}")))?;
        guard.insert(a.id.clone(), a.clone());
        Ok(())
    }

    fn get(&self, id: &ActivityId) -> std::result::Result<Option<Activity>, GovernanceError> {
        let guard = self
            .activities
            .read()
            .map_err(|e| GovernanceError::Internal(format!("activities lock poisoned: {e}")))?;
        Ok(guard.get(id).cloned())
    }

    fn list_by_entity(
        &self,
        entity_id: &str,
    ) -> std::result::Result<Vec<Activity>, GovernanceError> {
        let guard = self
            .activities
            .read()
            .map_err(|e| GovernanceError::Internal(format!("activities lock poisoned: {e}")))?;
        let mut out: Vec<Activity> = guard
            .values()
            .filter(|a| a.parent_entity_id == entity_id)
            .cloned()
            .collect();
        out.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        Ok(out)
    }

    fn delete(&self, id: &ActivityId) -> std::result::Result<bool, GovernanceError> {
        let mut guard = self
            .activities
            .write()
            .map_err(|e| GovernanceError::Internal(format!("activities lock poisoned: {e}")))?;
        Ok(guard.remove(id).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_activity_lifecycle() {
        let store = InMemoryActivityStore::new();
        let id = ActivityId::from_raw("nycn-summit-2026");
        let mut a = Activity::new(
            id.clone(),
            "nycn-organizers".to_string(),
            ActivityKind::Event,
            "NY Cooperative Summit 2026".to_string(),
            1000,
        );
        a.linked_structures = vec![
            StructureId::from_raw("nycn-content"),
            StructureId::from_raw("nycn-logistics"),
        ];

        store.save(&a).unwrap();

        let retrieved = store.get(&id).unwrap().unwrap();
        assert_eq!(retrieved.name, "NY Cooperative Summit 2026");
        assert_eq!(retrieved.linked_structures.len(), 2);
        assert_eq!(retrieved.status, ActivityStatus::Planned);
    }

    #[test]
    fn test_list_by_entity() {
        let store = InMemoryActivityStore::new();
        let a1 = Activity::new(
            ActivityId::from_raw("summit-2026"),
            "nycn-organizers".to_string(),
            ActivityKind::Event,
            "Summit 2026".to_string(),
            1000,
        );
        let a2 = Activity::new(
            ActivityId::from_raw("mentorship-program"),
            "nycn-organizers".to_string(),
            ActivityKind::Program,
            "Mentorship".to_string(),
            1000,
        );
        let a3 = Activity::new(
            ActivityId::from_raw("greenstar-expansion"),
            "greenstar".to_string(),
            ActivityKind::Project,
            "Expansion".to_string(),
            1000,
        );

        store.save(&a1).unwrap();
        store.save(&a2).unwrap();
        store.save(&a3).unwrap();

        assert_eq!(store.list_by_entity("nycn-organizers").unwrap().len(), 2);
        assert_eq!(store.list_by_entity("greenstar").unwrap().len(), 1);
        assert!(store.list_by_entity("nonexistent").unwrap().is_empty());
    }

    #[test]
    fn test_activity_delete() {
        let store = InMemoryActivityStore::new();
        let id = ActivityId::from_raw("temp");
        let a = Activity::new(
            id.clone(),
            "parent".to_string(),
            ActivityKind::Initiative,
            "Temp".to_string(),
            1000,
        );
        store.save(&a).unwrap();
        assert!(store.delete(&id).unwrap());
        assert!(store.get(&id).unwrap().is_none());
        assert!(!store.delete(&id).unwrap()); // idempotent
    }

    #[test]
    fn test_activity_serde_defaults() {
        let json = r#"{
            "id": "summit-2026",
            "parent_entity_id": "nycn-organizers",
            "kind": "event",
            "name": "Summit 2026",
            "created_at": 1000
        }"#;
        let a: Activity = serde_json::from_str(json).unwrap();
        assert_eq!(a.id.0, "summit-2026");
        assert_eq!(a.status, ActivityStatus::Planned);
        assert!(a.description.is_none());
        assert!(a.start_date.is_none());
        assert!(a.linked_structures.is_empty());
        assert!(a.created_by_decision.is_none());
    }

    #[test]
    fn test_activity_parent_program_id() {
        // Create with parent_program_id set
        let id = ActivityId::from_raw("summit-2026");
        let mut a = Activity::new(
            id.clone(),
            "nycn-organizers".to_string(),
            ActivityKind::Event,
            "NY Cooperative Summit 2026".to_string(),
            1000,
        );
        assert!(a.parent_program_id.is_none());

        a.parent_program_id = Some(ProgramId("annual-summit-cycle".to_string()));

        // Roundtrip through serde
        let json = serde_json::to_string(&a).unwrap();
        let restored: Activity = serde_json::from_str(&json).unwrap();
        assert_eq!(
            restored.parent_program_id.as_ref().unwrap().0,
            "annual-summit-cycle"
        );

        // Activities without parent_program_id deserialize cleanly
        let minimal = r#"{
            "id": "standalone",
            "parent_entity_id": "org",
            "kind": "initiative",
            "name": "Standalone",
            "created_at": 1000
        }"#;
        let b: Activity = serde_json::from_str(minimal).unwrap();
        assert!(b.parent_program_id.is_none());
    }

    /// Documents the postcard compat boundary: old Activity records (written before
    /// `parent_program_id` was added) cannot be decoded directly as the new `Activity`
    /// struct because postcard uses positional encoding and will return
    /// `DeserializeUnexpectedEnd` when the trailing field is absent.
    ///
    /// The `SledActivityStore` in `apps/governance` handles this via a V0 fallback:
    /// it decodes as `ActivityV0` on failure, then converts with `parent_program_id: None`.
    /// See `decode_activity_with_migration` in `apps/governance/src/manager.rs`.
    #[test]
    fn test_activity_postcard_old_layout_needs_migration() {
        /// Mirrors the Activity field layout BEFORE `parent_program_id` was added.
        /// Do NOT update this struct when Activity changes — it intentionally
        /// represents the OLD on-disk format to produce old-format bytes.
        #[derive(serde::Serialize, serde::Deserialize)]
        struct ActivityV0 {
            id: ActivityId,
            parent_entity_id: String,
            kind: ActivityKind,
            name: String,
            description: Option<String>,
            status: ActivityStatus,
            start_date: Option<Timestamp>,
            end_date: Option<Timestamp>,
            linked_structures: Vec<StructureId>,
            created_at: Timestamp,
            created_by_decision: Option<ProposalId>,
        }

        let old = ActivityV0 {
            id: ActivityId::from_raw("legacy-summit"),
            parent_entity_id: "nycn".to_string(),
            kind: ActivityKind::Event,
            name: "Legacy Event".to_string(),
            description: None,
            status: ActivityStatus::Planned,
            start_date: None,
            end_date: None,
            linked_structures: vec![],
            created_at: 9999,
            created_by_decision: None,
        };

        let bytes = icn_encoding::encode_versioned(&old).expect("encode old layout");

        // Direct decode fails — postcard returns DeserializeUnexpectedEnd because
        // `parent_program_id` bytes are absent.
        let direct_result = icn_encoding::decode_versioned::<Activity>(&bytes);
        assert!(
            direct_result.is_err(),
            "expected DeserializeUnexpectedEnd; old records REQUIRE migration in the Sled store"
        );

        // V0 decode still works (migration fallback path).
        let v0: ActivityV0 =
            icn_encoding::decode_versioned(&bytes).expect("decode as ActivityV0 must succeed");
        assert_eq!(v0.id.0, "legacy-summit");
        assert_eq!(v0.created_at, 9999);
    }

    #[test]
    fn test_activity_status_transitions() {
        let id = ActivityId::from_raw("x");
        let mut a = Activity::new(
            id,
            "p".to_string(),
            ActivityKind::Project,
            "X".to_string(),
            1000,
        );
        assert!(!a.is_active());
        a.status = ActivityStatus::Active;
        assert!(a.is_active());
        a.status = ActivityStatus::Completed;
        assert!(!a.is_active());
    }
}
