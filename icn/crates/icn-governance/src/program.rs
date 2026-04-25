//! Programs and milestones: recurring cycles and stage-gated initiatives.
//!
//! A `Program` is a higher-level institutional container than `Activity`. Where
//! `Activity` captures a single endeavor (an event, a project, etc.), a
//! `Program` captures a coherent multi-phase effort with:
//!
//! - A typed lifecycle beyond Activity's 4-state status.
//! - A phased sequence of [`Milestone`]s with stage-gate semantics.
//! - An optional `parent_program_id` for cycle-over-cycle handoff (e.g. the
//!   current annual cycle pointing at the previous one).
//! - An ordered set of linked [`Activity`] IDs composing the program's
//!   operational work.
//!
//! ## Design discipline (Layer 1 generics)
//!
//! The canonical spec originally proposed richly named variants —
//! `AnnualSummit`, `PolicyDay`, `VenueLocked` — which bake a specific
//! institution's vocabulary into core enums. That violates the
//! anti-ontology-laundering rule: Layer 1 primitives should be generic enough
//! for a second institution (a housing federation, a mutual-aid collective)
//! to consume unchanged.
//!
//! This module therefore ships a deliberately small kind enum with a
//! `Custom(String)` escape hatch, and a free-form `completion_criteria:
//! Vec<String>` for milestones. Institution-specific kinds and checklist
//! semantics belong in the institution package / template layer (Layer 2)
//! or the institution's own repo (Layer 3), not here.
//!
//! See `docs/strategy/NYCN-Repo-Architecture-Spec.md` §5 for the historical
//! design context, and the plan discipline in `hidden-swinging-sphinx.md`
//! ("Anti-pattern: ontology laundering") for why the enums are narrower here.

use crate::activity::ActivityId;
use crate::{GovernanceDomainId, GovernanceError, ProposalId, Timestamp};
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use uuid::Uuid;

// ============================================================================
// IDs
// ============================================================================

/// Unique identifier for a program.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProgramId(pub String);

impl ProgramId {
    /// Create a new random program ID.
    pub fn generate() -> Self {
        Self(format!("prog-{}", Uuid::new_v4()))
    }

    /// Create from a raw identifier (e.g. deterministic slugs like
    /// `summit-cycle-2026`).
    pub fn from_raw(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl std::fmt::Display for ProgramId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a milestone.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MilestoneId(pub String);

impl MilestoneId {
    pub fn generate() -> Self {
        Self(format!("mile-{}", Uuid::new_v4()))
    }

    pub fn from_raw(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl std::fmt::Display for MilestoneId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ============================================================================
// Kind + status enums
// ============================================================================

/// Kind of program.
///
/// Deliberately narrow. Institution-specific shapes (annual summits,
/// membership drives, chapter-wide campaigns) should either map into one of
/// the generic variants or use [`ProgramKind::Custom`] with a descriptive
/// string. Do not add tenant-specific variants here without a second
/// non-NYCN institutional case proving genericity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgramKind {
    /// A recurring institutional cycle with closure and handoff between
    /// iterations (annual congress, biennial assembly, summit cycle).
    Cycle,
    /// A time-bounded outreach, advocacy, or fundraising push.
    Campaign,
    /// An ongoing improvement or capability-building effort.
    Initiative,
    /// A recurring smaller series (regional meetups, workshop sequences).
    Series,
    /// Institution-specific program shape. Prefer one of the above when the
    /// semantics fit; use this only when a generic variant would distort
    /// meaning.
    Custom(String),
}

/// Lifecycle status of a program.
///
/// Richer than [`crate::activity::ActivityStatus`] because programs have
/// explicit public-commitment and closure phases that stage-gate them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgramStatus {
    /// Being drafted; not yet committed to.
    #[default]
    Draft,
    /// Actively planning; internal work underway, not yet public.
    ActivePlanning,
    /// Publicly launched; external commitments made.
    PublicLaunch,
    /// In execution; the core operational phase.
    InExecution,
    /// Closed; handoff complete but not yet archived.
    Closed,
    /// Archived; no further changes expected.
    Archived,
}

/// Lifecycle status of a milestone within a program.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MilestoneStatus {
    /// Not yet started.
    #[default]
    Pending,
    /// Work underway.
    InProgress,
    /// Completed successfully.
    Completed,
    /// Blocked on an external dependency.
    Blocked,
    /// Deliberately skipped for this cycle.
    Skipped,
}

// ============================================================================
// Program + Milestone records
// ============================================================================

/// A multi-phase institutional endeavor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Program {
    /// Unique identifier.
    pub id: ProgramId,

    /// Governance domain this program lives in.
    pub domain_id: GovernanceDomainId,

    /// Entity ID of the parent (cooperative, community, or federation).
    /// Using `String` rather than an `EntityId` newtype to avoid a cross-crate
    /// dependency — matches the pattern already established by `Activity`.
    pub parent_entity_id: String,

    /// Kind of program.
    pub kind: ProgramKind,

    /// Display name.
    pub name: String,

    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,

    /// Current lifecycle status.
    #[serde(default)]
    pub status: ProgramStatus,

    /// Optional start time (Unix seconds).
    #[serde(default)]
    pub start_at: Option<Timestamp>,

    /// Optional end time (Unix seconds).
    #[serde(default)]
    pub end_at: Option<Timestamp>,

    /// Link to the previous cycle's program (enables year-over-year views and
    /// handoff semantics between iterations of a recurring cycle).
    #[serde(default)]
    pub parent_program_id: Option<ProgramId>,

    /// Milestones that stage-gate this program, in phase order. Stored as IDs
    /// (not embedded) so milestones can be updated without rewriting the
    /// whole program record.
    #[serde(default)]
    pub milestones: Vec<MilestoneId>,

    /// Activities that compose the operational work of this program.
    #[serde(default)]
    pub activities: Vec<ActivityId>,

    /// Unix timestamp when the program was created.
    pub created_at: Timestamp,

    /// Unix timestamp when the program transitioned to `Closed` (if it has).
    #[serde(default)]
    pub closed_at: Option<Timestamp>,

    /// If the program was authorized by a proposal, that proposal's ID.
    #[serde(default)]
    pub created_by_decision: Option<ProposalId>,
}

impl Program {
    /// Create a new draft program with minimal required fields.
    pub fn new(
        id: ProgramId,
        domain_id: GovernanceDomainId,
        parent_entity_id: impl Into<String>,
        kind: ProgramKind,
        name: impl Into<String>,
        now: Timestamp,
    ) -> Self {
        Self {
            id,
            domain_id,
            parent_entity_id: parent_entity_id.into(),
            kind,
            name: name.into(),
            description: None,
            status: ProgramStatus::Draft,
            start_at: None,
            end_at: None,
            parent_program_id: None,
            milestones: Vec::new(),
            activities: Vec::new(),
            created_at: now,
            closed_at: None,
            created_by_decision: None,
        }
    }

    /// Whether this program is in an active lifecycle phase (planning
    /// through execution, but not yet closed).
    pub fn is_active(&self) -> bool {
        matches!(
            self.status,
            ProgramStatus::ActivePlanning
                | ProgramStatus::PublicLaunch
                | ProgramStatus::InExecution
        )
    }
}

/// A stage-gate within a program.
///
/// Milestones do not auto-complete. A user with appropriate scope (e.g. a
/// program lead or a role on the owning structure) marks them completed,
/// optionally referencing which items in `completion_criteria` were
/// satisfied. Enforcement of what it means to "satisfy" a criterion lives in
/// the institution package layer; this module just stores the declared
/// checklist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    /// Unique identifier.
    pub id: MilestoneId,

    /// Program this milestone belongs to.
    pub program_id: ProgramId,

    /// Display name (e.g. "Venue confirmed", "Budget approved").
    pub name: String,

    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,

    /// Ordinal position among the program's milestones (0-based).
    #[serde(default)]
    pub phase_index: u32,

    /// Optional target date (Unix seconds).
    #[serde(default)]
    pub target_date: Option<Timestamp>,

    /// Current lifecycle status.
    #[serde(default)]
    pub status: MilestoneStatus,

    /// Free-form checklist of what must be true for this milestone to be
    /// considered complete. Interpretation / enforcement is Layer-2+ concern
    /// — this module stores declared criteria only.
    #[serde(default)]
    pub completion_criteria: Vec<String>,

    /// Unix timestamp when the milestone was marked completed (if it has).
    #[serde(default)]
    pub completed_at: Option<Timestamp>,

    /// DID that marked the milestone completed (if it has).
    #[serde(default)]
    pub completed_by: Option<Did>,

    /// Unix timestamp when the milestone was created.
    pub created_at: Timestamp,
}

impl Milestone {
    /// Create a new pending milestone with minimal required fields.
    pub fn new(
        id: MilestoneId,
        program_id: ProgramId,
        name: impl Into<String>,
        phase_index: u32,
        now: Timestamp,
    ) -> Self {
        Self {
            id,
            program_id,
            name: name.into(),
            description: None,
            phase_index,
            target_date: None,
            status: MilestoneStatus::Pending,
            completion_criteria: Vec::new(),
            completed_at: None,
            completed_by: None,
            created_at: now,
        }
    }

    /// Whether this milestone is open (not completed or skipped).
    pub fn is_open(&self) -> bool {
        !matches!(
            self.status,
            MilestoneStatus::Completed | MilestoneStatus::Skipped
        )
    }
}

// ============================================================================
// Store backends
// ============================================================================

/// Storage backend trait for programs.
pub trait ProgramStoreBackend: Send + Sync {
    /// Save (create or update) a program.
    fn save(&self, p: &Program) -> std::result::Result<(), GovernanceError>;

    /// Retrieve a program by ID.
    fn get(&self, id: &ProgramId) -> std::result::Result<Option<Program>, GovernanceError>;

    /// List all programs in a governance domain, newest first.
    fn list_by_domain(
        &self,
        domain_id: &GovernanceDomainId,
    ) -> std::result::Result<Vec<Program>, GovernanceError>;

    /// List all programs owned by a given parent entity, newest first.
    fn list_by_entity(&self, entity_id: &str)
        -> std::result::Result<Vec<Program>, GovernanceError>;

    /// Delete a program (hard delete — prefer `Archived` status for
    /// soft-archive).
    fn delete(&self, id: &ProgramId) -> std::result::Result<bool, GovernanceError>;
}

/// Storage backend trait for milestones.
pub trait MilestoneStoreBackend: Send + Sync {
    /// Save (create or update) a milestone.
    fn save(&self, m: &Milestone) -> std::result::Result<(), GovernanceError>;

    /// Retrieve a milestone by ID.
    fn get(&self, id: &MilestoneId) -> std::result::Result<Option<Milestone>, GovernanceError>;

    /// List milestones belonging to a program, ordered by `phase_index`.
    fn list_by_program(
        &self,
        program_id: &ProgramId,
    ) -> std::result::Result<Vec<Milestone>, GovernanceError>;

    /// Delete a milestone (hard delete).
    fn delete(&self, id: &MilestoneId) -> std::result::Result<bool, GovernanceError>;
}

// ============================================================================
// In-memory implementations
// ============================================================================

/// In-memory [`ProgramStoreBackend`] for tests and default config.
#[derive(Default)]
pub struct InMemoryProgramStore {
    programs: RwLock<HashMap<ProgramId, Program>>,
}

impl InMemoryProgramStore {
    /// Create a new empty in-memory store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl ProgramStoreBackend for InMemoryProgramStore {
    fn save(&self, p: &Program) -> std::result::Result<(), GovernanceError> {
        let mut guard = self
            .programs
            .write()
            .map_err(|e| GovernanceError::Internal(format!("programs lock poisoned: {e}")))?;
        guard.insert(p.id.clone(), p.clone());
        Ok(())
    }

    fn get(&self, id: &ProgramId) -> std::result::Result<Option<Program>, GovernanceError> {
        let guard = self
            .programs
            .read()
            .map_err(|e| GovernanceError::Internal(format!("programs lock poisoned: {e}")))?;
        Ok(guard.get(id).cloned())
    }

    fn list_by_domain(
        &self,
        domain_id: &GovernanceDomainId,
    ) -> std::result::Result<Vec<Program>, GovernanceError> {
        let guard = self
            .programs
            .read()
            .map_err(|e| GovernanceError::Internal(format!("programs lock poisoned: {e}")))?;
        let mut out: Vec<Program> = guard
            .values()
            .filter(|p| &p.domain_id == domain_id)
            .cloned()
            .collect();
        out.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        Ok(out)
    }

    fn list_by_entity(
        &self,
        entity_id: &str,
    ) -> std::result::Result<Vec<Program>, GovernanceError> {
        let guard = self
            .programs
            .read()
            .map_err(|e| GovernanceError::Internal(format!("programs lock poisoned: {e}")))?;
        let mut out: Vec<Program> = guard
            .values()
            .filter(|p| p.parent_entity_id == entity_id)
            .cloned()
            .collect();
        out.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        Ok(out)
    }

    fn delete(&self, id: &ProgramId) -> std::result::Result<bool, GovernanceError> {
        let mut guard = self
            .programs
            .write()
            .map_err(|e| GovernanceError::Internal(format!("programs lock poisoned: {e}")))?;
        Ok(guard.remove(id).is_some())
    }
}

/// In-memory [`MilestoneStoreBackend`] for tests and default config.
#[derive(Default)]
pub struct InMemoryMilestoneStore {
    milestones: RwLock<HashMap<MilestoneId, Milestone>>,
}

impl InMemoryMilestoneStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl MilestoneStoreBackend for InMemoryMilestoneStore {
    fn save(&self, m: &Milestone) -> std::result::Result<(), GovernanceError> {
        let mut guard = self
            .milestones
            .write()
            .map_err(|e| GovernanceError::Internal(format!("milestones lock poisoned: {e}")))?;
        guard.insert(m.id.clone(), m.clone());
        Ok(())
    }

    fn get(&self, id: &MilestoneId) -> std::result::Result<Option<Milestone>, GovernanceError> {
        let guard = self
            .milestones
            .read()
            .map_err(|e| GovernanceError::Internal(format!("milestones lock poisoned: {e}")))?;
        Ok(guard.get(id).cloned())
    }

    fn list_by_program(
        &self,
        program_id: &ProgramId,
    ) -> std::result::Result<Vec<Milestone>, GovernanceError> {
        let guard = self
            .milestones
            .read()
            .map_err(|e| GovernanceError::Internal(format!("milestones lock poisoned: {e}")))?;
        let mut out: Vec<Milestone> = guard
            .values()
            .filter(|m| &m.program_id == program_id)
            .cloned()
            .collect();
        out.sort_by_key(|m| m.phase_index);
        Ok(out)
    }

    fn delete(&self, id: &MilestoneId) -> std::result::Result<bool, GovernanceError> {
        let mut guard = self
            .milestones
            .write()
            .map_err(|e| GovernanceError::Internal(format!("milestones lock poisoned: {e}")))?;
        Ok(guard.remove(id).is_some())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_program(id: &str, domain: &str, entity: &str) -> Program {
        Program::new(
            ProgramId::from_raw(id),
            GovernanceDomainId::new(domain),
            entity,
            ProgramKind::Cycle,
            format!("Program {id}"),
            1_700_000_000,
        )
    }

    #[test]
    fn program_new_defaults_to_draft() {
        let p = mk_program("cycle-2026", "dom-a", "ent-a");
        assert_eq!(p.status, ProgramStatus::Draft);
        assert!(p.milestones.is_empty());
        assert!(p.activities.is_empty());
        assert!(!p.is_active());
    }

    #[test]
    fn program_is_active_spans_planning_through_execution() {
        let mut p = mk_program("cycle-2026", "dom-a", "ent-a");
        p.status = ProgramStatus::ActivePlanning;
        assert!(p.is_active());
        p.status = ProgramStatus::PublicLaunch;
        assert!(p.is_active());
        p.status = ProgramStatus::InExecution;
        assert!(p.is_active());
        p.status = ProgramStatus::Closed;
        assert!(!p.is_active());
        p.status = ProgramStatus::Archived;
        assert!(!p.is_active());
    }

    #[test]
    fn milestone_new_defaults_to_pending_and_open() {
        let m = Milestone::new(
            MilestoneId::from_raw("m-venue"),
            ProgramId::from_raw("cycle-2026"),
            "Venue confirmed",
            0,
            1_700_000_000,
        );
        assert_eq!(m.status, MilestoneStatus::Pending);
        assert!(m.is_open());
    }

    #[test]
    fn milestone_is_open_closes_on_completion_or_skip() {
        let mut m = Milestone::new(
            MilestoneId::from_raw("m-venue"),
            ProgramId::from_raw("cycle-2026"),
            "Venue confirmed",
            0,
            1_700_000_000,
        );
        m.status = MilestoneStatus::InProgress;
        assert!(m.is_open());
        m.status = MilestoneStatus::Blocked;
        assert!(m.is_open());
        m.status = MilestoneStatus::Completed;
        assert!(!m.is_open());
        m.status = MilestoneStatus::Skipped;
        assert!(!m.is_open());
    }

    #[test]
    fn program_kind_custom_serde_roundtrip() {
        let p = Program::new(
            ProgramId::from_raw("p"),
            GovernanceDomainId::new("dom"),
            "ent",
            ProgramKind::Custom("membership-drive".to_string()),
            "Drive",
            1_000,
        );
        let json = serde_json::to_string(&p).unwrap();
        let back: Program = serde_json::from_str(&json).unwrap();
        assert!(matches!(back.kind, ProgramKind::Custom(ref s) if s == "membership-drive"));
    }

    #[test]
    fn in_memory_program_store_save_and_get() {
        let store = InMemoryProgramStore::new();
        let p = mk_program("cycle-2026", "dom-a", "ent-a");
        store.save(&p).unwrap();

        let got = store.get(&p.id).unwrap().unwrap();
        assert_eq!(got.id, p.id);
        assert_eq!(got.parent_entity_id, "ent-a");
    }

    #[test]
    fn in_memory_program_store_list_by_domain_filters() {
        let store = InMemoryProgramStore::new();
        store.save(&mk_program("a1", "dom-a", "ent-a")).unwrap();
        store.save(&mk_program("a2", "dom-a", "ent-a")).unwrap();
        store.save(&mk_program("b1", "dom-b", "ent-b")).unwrap();

        let dom_a = store
            .list_by_domain(&GovernanceDomainId::new("dom-a"))
            .unwrap();
        let dom_b = store
            .list_by_domain(&GovernanceDomainId::new("dom-b"))
            .unwrap();
        assert_eq!(dom_a.len(), 2);
        assert_eq!(dom_b.len(), 1);
    }

    #[test]
    fn in_memory_program_store_list_by_entity_filters() {
        let store = InMemoryProgramStore::new();
        store.save(&mk_program("a1", "dom-a", "ent-a")).unwrap();
        store.save(&mk_program("a2", "dom-a", "ent-b")).unwrap();

        let ent_a = store.list_by_entity("ent-a").unwrap();
        let ent_b = store.list_by_entity("ent-b").unwrap();
        assert_eq!(ent_a.len(), 1);
        assert_eq!(ent_b.len(), 1);
    }

    #[test]
    fn in_memory_program_store_delete() {
        let store = InMemoryProgramStore::new();
        let p = mk_program("cycle-2026", "dom-a", "ent-a");
        store.save(&p).unwrap();
        assert!(store.delete(&p.id).unwrap());
        assert!(store.get(&p.id).unwrap().is_none());
        assert!(!store.delete(&p.id).unwrap()); // idempotent
    }

    #[test]
    fn in_memory_milestone_store_list_by_program_ordered_by_phase_index() {
        let store = InMemoryMilestoneStore::new();
        let prog = ProgramId::from_raw("cycle-2026");

        let m2 = Milestone::new(
            MilestoneId::from_raw("m-launch"),
            prog.clone(),
            "Public launch",
            2,
            1_000,
        );
        let m0 = Milestone::new(
            MilestoneId::from_raw("m-venue"),
            prog.clone(),
            "Venue confirmed",
            0,
            1_000,
        );
        let m1 = Milestone::new(
            MilestoneId::from_raw("m-budget"),
            prog.clone(),
            "Budget approved",
            1,
            1_000,
        );

        // Save out of order.
        store.save(&m2).unwrap();
        store.save(&m0).unwrap();
        store.save(&m1).unwrap();

        let listing = store.list_by_program(&prog).unwrap();
        assert_eq!(listing.len(), 3);
        assert_eq!(listing[0].name, "Venue confirmed");
        assert_eq!(listing[1].name, "Budget approved");
        assert_eq!(listing[2].name, "Public launch");
    }

    #[test]
    fn in_memory_milestone_store_list_by_program_isolates_programs() {
        let store = InMemoryMilestoneStore::new();
        let prog_a = ProgramId::from_raw("cycle-2026");
        let prog_b = ProgramId::from_raw("campaign-q3");

        store
            .save(&Milestone::new(
                MilestoneId::from_raw("m-a"),
                prog_a.clone(),
                "A",
                0,
                1_000,
            ))
            .unwrap();
        store
            .save(&Milestone::new(
                MilestoneId::from_raw("m-b"),
                prog_b.clone(),
                "B",
                0,
                1_000,
            ))
            .unwrap();

        let a = store.list_by_program(&prog_a).unwrap();
        let b = store.list_by_program(&prog_b).unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(b.len(), 1);
        assert_eq!(a[0].name, "A");
        assert_eq!(b[0].name, "B");
    }

    #[test]
    fn in_memory_milestone_store_delete() {
        let store = InMemoryMilestoneStore::new();
        let prog = ProgramId::from_raw("cycle-2026");
        let m = Milestone::new(MilestoneId::from_raw("m-a"), prog, "A", 0, 1_000);
        store.save(&m).unwrap();
        assert!(store.delete(&m.id).unwrap());
        assert!(store.get(&m.id).unwrap().is_none());
        assert!(!store.delete(&m.id).unwrap());
    }
}
