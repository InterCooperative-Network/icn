//! Meetings: institutional trace objects for deliberation and decision-making.
//!
//! A meeting exists because it produces deliberation context, witness context,
//! and linked outputs — not as a generic calendar entry. Every meeting is
//! scoped to a governance domain and may link to structures, activities,
//! proposals, and action items.
//!
//! **Meaning-firewall note**: `MeetingRole` values (`Facilitator`, `NoteTaker`,
//! etc.) are *coordination labels* on attendance records. They do NOT grant
//! governance authority or voting rights. Authority requires an explicit
//! `RoleAssignment` on a `Structure`.
//!
//! See `docs/design/institutional-structure-spec.md` for the full design.

use crate::structure::StructureId;
use crate::{ActivityId, GovernanceError, ProposalId, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use uuid::Uuid;

// ========== Identifiers ==========

/// Unique identifier for a meeting.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MeetingId(pub String);

impl MeetingId {
    /// Create a new random meeting ID.
    pub fn generate() -> Self {
        Self(format!("meeting-{}", Uuid::new_v4()))
    }

    /// Create from a raw string (useful for deterministic IDs like "nycn-q1-2026-all-hands").
    pub fn from_raw(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl std::fmt::Display for MeetingId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for an agenda item within a meeting.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgendaItemId(pub Uuid);

impl AgendaItemId {
    /// Create a new random agenda item ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AgendaItemId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AgendaItemId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ========== Enums ==========

/// Lifecycle status of a meeting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeetingStatus {
    /// Scheduled but not yet started.
    #[default]
    Scheduled,
    /// Currently in progress.
    InProgress,
    /// Finished and minutes finalized.
    Completed,
    /// Cancelled before it began.
    Cancelled,
}

/// Attendance status of a participant in a meeting.
///
/// Richer than `(Did, bool)` — captures invitation, presence, and remote attendance
/// for quorum and witness context.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttendanceStatus {
    /// Invited but not yet confirmed.
    #[default]
    Invited,
    /// Attended in person.
    Present,
    /// Did not attend.
    Absent,
    /// Attended remotely (e.g., video call).
    Remote,
}

/// Coordination role a participant holds in this specific meeting.
///
/// **CRITICAL INVARIANT**: These are *coordination labels only*. A `Facilitator`
/// role does NOT grant governance authority, voting weight, or decision power.
/// Those require an explicit `RoleAssignment` on a `Structure` linked by charter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeetingRole {
    /// Person running the meeting process.
    Facilitator,
    /// Person capturing notes and producing minutes.
    NoteTaker,
    /// Regular participant with full engagement.
    #[default]
    Participant,
    /// Present but non-voting observer (e.g., guest, new member in onboarding).
    Observer,
}

// ========== Core Types ==========

/// A participant in a specific meeting with their attendance and coordination role.
///
/// `meeting_role` is a coordination label for this meeting instance only.
/// It does not derive from or grant any persistent governance authority.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingAttendee {
    /// DID of the participant. Using `String` to avoid cross-crate DID dependency
    /// in the governance crate.
    pub did: String,

    /// Attendance status: invited, present, absent, or remote.
    #[serde(default)]
    pub status: AttendanceStatus,

    /// Coordination role for this meeting. Defaults to `Participant`.
    /// NOT a governance authority grant — see module docs.
    #[serde(default)]
    pub meeting_role: MeetingRole,
}

impl MeetingAttendee {
    /// Create a new attendee with Invited status and Participant role.
    pub fn new(did: impl Into<String>) -> Self {
        Self {
            did: did.into(),
            status: AttendanceStatus::Invited,
            meeting_role: MeetingRole::Participant,
        }
    }
}

/// A single item on a meeting agenda.
///
/// Each agenda item may be linked to an in-flight proposal (for discussion or
/// ratification) and generates action items on meeting close for unresolved
/// discussion outcomes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgendaItem {
    /// Unique identifier within this meeting.
    pub id: AgendaItemId,

    /// Short title of the agenda item (e.g., "Q2 Budget Review", "Membership vote: Alice").
    pub title: String,

    /// Optional longer description.
    #[serde(default)]
    pub description: Option<String>,

    /// DID of the person presenting this item.
    #[serde(default)]
    pub presenter: Option<String>,

    /// A proposal being discussed or voted on during this agenda item.
    #[serde(default)]
    pub linked_proposal: Option<ProposalId>,

    /// Free-text notes captured during discussion of this item.
    #[serde(default)]
    pub discussion_notes: Option<String>,

    /// Outcome of discussion: "resolved", "tabled", "referred", "no_action", etc.
    /// Free-form but bounded — used to determine whether action items are needed.
    #[serde(default)]
    pub outcome: Option<String>,

    /// Action item IDs generated from this agenda item (populated at meeting close).
    #[serde(default)]
    pub generated_action_items: Vec<crate::ActionItemId>,
}

impl AgendaItem {
    /// Create a new agenda item with a title.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            id: AgendaItemId::new(),
            title: title.into(),
            description: None,
            presenter: None,
            linked_proposal: None,
            discussion_notes: None,
            outcome: None,
            generated_action_items: Vec::new(),
        }
    }

    /// Whether this item has been resolved (outcome is set to "resolved").
    pub fn is_resolved(&self) -> bool {
        self.outcome.as_deref().is_some_and(|o| o == "resolved")
    }
}

/// A meeting — the institutional trace object for deliberation.
///
/// Meetings link: agenda → proposals → attendance (witness context) →
/// action items → records. They are scoped to a governance domain and may
/// attach to structures (committees) and activities (events).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meeting {
    /// Unique identifier.
    pub id: MeetingId,

    /// Governance domain this meeting belongs to.
    pub domain_id: String,

    /// Display title (e.g., "NYCN Steering Committee – March 2026").
    pub title: String,

    /// Optional description or purpose statement.
    #[serde(default)]
    pub description: Option<String>,

    /// Scheduled start time (Unix seconds). May differ from actual start.
    #[serde(default)]
    pub scheduled_at: Option<Timestamp>,

    /// Actual start time (set when status transitions to InProgress).
    #[serde(default)]
    pub started_at: Option<Timestamp>,

    /// Actual end time (set when status transitions to Completed or Cancelled).
    #[serde(default)]
    pub ended_at: Option<Timestamp>,

    /// Current lifecycle status.
    #[serde(default)]
    pub status: MeetingStatus,

    /// Invited/present participants with their attendance and coordination roles.
    #[serde(default)]
    pub attendees: Vec<MeetingAttendee>,

    /// Ordered agenda items for this meeting.
    #[serde(default)]
    pub agenda: Vec<AgendaItem>,

    /// Structures (committees, working groups) hosting or contributing to this meeting.
    /// Informational linkage — does not grant authority from those structures.
    #[serde(default)]
    pub linked_structures: Vec<StructureId>,

    /// Activities (events, programs) this meeting is part of.
    #[serde(default)]
    pub linked_activities: Vec<ActivityId>,

    /// DocumentId for meeting notes/minutes (set after completion).
    /// `String` to avoid forward dependency on the document type (added in Tranche 3).
    #[serde(default)]
    pub notes_doc_id: Option<String>,

    /// DID of the person who created this meeting record.
    pub created_by: String,

    /// Unix timestamp when the meeting record was created.
    pub created_at: Timestamp,
}

impl Meeting {
    /// Create a new scheduled meeting with minimal required fields.
    pub fn new(
        id: MeetingId,
        domain_id: impl Into<String>,
        title: impl Into<String>,
        created_by: impl Into<String>,
        now: Timestamp,
    ) -> Self {
        Self {
            id,
            domain_id: domain_id.into(),
            title: title.into(),
            description: None,
            scheduled_at: None,
            started_at: None,
            ended_at: None,
            status: MeetingStatus::Scheduled,
            attendees: Vec::new(),
            agenda: Vec::new(),
            linked_structures: Vec::new(),
            linked_activities: Vec::new(),
            notes_doc_id: None,
            created_by: created_by.into(),
            created_at: now,
        }
    }

    /// Whether the meeting is currently active.
    pub fn is_in_progress(&self) -> bool {
        matches!(self.status, MeetingStatus::InProgress)
    }

    /// Whether the meeting has been completed.
    pub fn is_completed(&self) -> bool {
        matches!(self.status, MeetingStatus::Completed)
    }

    /// Count attendees with Present or Remote status.
    pub fn present_count(&self) -> usize {
        self.attendees
            .iter()
            .filter(|a| {
                matches!(
                    a.status,
                    AttendanceStatus::Present | AttendanceStatus::Remote
                )
            })
            .count()
    }

    /// Find the agenda item with the given ID.
    pub fn get_agenda_item(&self, id: &AgendaItemId) -> Option<&AgendaItem> {
        self.agenda.iter().find(|item| &item.id == id)
    }

    /// Find the agenda item with the given ID, mutably.
    pub fn get_agenda_item_mut(&mut self, id: &AgendaItemId) -> Option<&mut AgendaItem> {
        self.agenda.iter_mut().find(|item| &item.id == id)
    }

    /// Agenda items that have no outcome set (unresolved at meeting close).
    pub fn unresolved_agenda_items(&self) -> Vec<&AgendaItem> {
        self.agenda
            .iter()
            .filter(|item| item.outcome.is_none())
            .collect()
    }
}

// ========== Store Backend ==========

/// Storage backend trait for meetings.
pub trait MeetingStoreBackend: Send + Sync {
    /// Save (create or update) a meeting.
    fn save(&self, m: &Meeting) -> std::result::Result<(), GovernanceError>;

    /// Retrieve a meeting by ID.
    fn get(&self, id: &MeetingId) -> std::result::Result<Option<Meeting>, GovernanceError>;

    /// List all meetings in a governance domain, newest first.
    fn list_by_domain(&self, domain_id: &str)
        -> std::result::Result<Vec<Meeting>, GovernanceError>;

    /// Delete a meeting (hard delete — prefer `Cancelled` status for soft-cancel).
    fn delete(&self, id: &MeetingId) -> std::result::Result<bool, GovernanceError>;
}

// ========== In-Memory Store ==========

/// In-memory implementation of [`MeetingStoreBackend`]. Primarily for tests.
#[derive(Default)]
pub struct InMemoryMeetingStore {
    meetings: RwLock<HashMap<MeetingId, Meeting>>,
}

impl InMemoryMeetingStore {
    /// Create a new empty in-memory store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl MeetingStoreBackend for InMemoryMeetingStore {
    fn save(&self, m: &Meeting) -> std::result::Result<(), GovernanceError> {
        let mut guard = self
            .meetings
            .write()
            .map_err(|e| GovernanceError::Internal(format!("meetings lock poisoned: {e}")))?;
        guard.insert(m.id.clone(), m.clone());
        Ok(())
    }

    fn get(&self, id: &MeetingId) -> std::result::Result<Option<Meeting>, GovernanceError> {
        let guard = self
            .meetings
            .read()
            .map_err(|e| GovernanceError::Internal(format!("meetings lock poisoned: {e}")))?;
        Ok(guard.get(id).cloned())
    }

    fn list_by_domain(
        &self,
        domain_id: &str,
    ) -> std::result::Result<Vec<Meeting>, GovernanceError> {
        let guard = self
            .meetings
            .read()
            .map_err(|e| GovernanceError::Internal(format!("meetings lock poisoned: {e}")))?;
        let mut out: Vec<Meeting> = guard
            .values()
            .filter(|m| m.domain_id == domain_id)
            .cloned()
            .collect();
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(out)
    }

    fn delete(&self, id: &MeetingId) -> std::result::Result<bool, GovernanceError> {
        let mut guard = self
            .meetings
            .write()
            .map_err(|e| GovernanceError::Internal(format!("meetings lock poisoned: {e}")))?;
        Ok(guard.remove(id).is_some())
    }
}

// ========== Tests ==========

#[cfg(test)]
mod tests {
    use super::*;

    fn make_meeting(id: &str, domain: &str) -> Meeting {
        Meeting::new(
            MeetingId::from_raw(id),
            domain,
            "Test Meeting",
            "did:icn:creator",
            1000,
        )
    }

    #[test]
    fn test_meeting_lifecycle() {
        let store = InMemoryMeetingStore::new();
        let id = MeetingId::from_raw("nycn-march-2026");
        let mut m = make_meeting("nycn-march-2026", "nycn-domain");
        m.scheduled_at = Some(2000);
        m.attendees = vec![
            MeetingAttendee {
                did: "did:icn:alice".to_string(),
                status: AttendanceStatus::Present,
                meeting_role: MeetingRole::Facilitator,
            },
            MeetingAttendee {
                did: "did:icn:bob".to_string(),
                status: AttendanceStatus::Remote,
                meeting_role: MeetingRole::NoteTaker,
            },
            MeetingAttendee {
                did: "did:icn:charlie".to_string(),
                status: AttendanceStatus::Absent,
                meeting_role: MeetingRole::Participant,
            },
        ];

        store.save(&m).unwrap();
        let retrieved = store.get(&id).unwrap().unwrap();
        assert_eq!(retrieved.title, "Test Meeting");
        assert_eq!(retrieved.attendees.len(), 3);
        assert_eq!(retrieved.present_count(), 2); // alice + bob
        assert_eq!(retrieved.status, MeetingStatus::Scheduled);
    }

    #[test]
    fn test_meeting_status_transitions() {
        let mut m = make_meeting("test", "domain");
        assert!(!m.is_in_progress());
        assert!(!m.is_completed());

        m.status = MeetingStatus::InProgress;
        m.started_at = Some(1100);
        assert!(m.is_in_progress());

        m.status = MeetingStatus::Completed;
        m.ended_at = Some(1300);
        assert!(m.is_completed());
        assert!(!m.is_in_progress());
    }

    #[test]
    fn test_agenda_items() {
        let mut m = make_meeting("test", "domain");
        let item1 = AgendaItem::new("Budget review");
        let item2 = AgendaItem::new("Membership vote");
        let id1 = item1.id.clone();
        let id2 = item2.id.clone();
        m.agenda.push(item1);
        m.agenda.push(item2);

        assert_eq!(m.unresolved_agenda_items().len(), 2);

        // Resolve one
        if let Some(item) = m.get_agenda_item_mut(&id1) {
            item.outcome = Some("resolved".to_string());
        }
        assert_eq!(m.unresolved_agenda_items().len(), 1);
        assert!(m.get_agenda_item(&id2).is_some());
    }

    #[test]
    fn test_list_by_domain() {
        let store = InMemoryMeetingStore::new();

        store.save(&make_meeting("m1", "nycn")).unwrap();
        store.save(&make_meeting("m2", "nycn")).unwrap();
        store.save(&make_meeting("m3", "greenstar")).unwrap();

        assert_eq!(store.list_by_domain("nycn").unwrap().len(), 2);
        assert_eq!(store.list_by_domain("greenstar").unwrap().len(), 1);
        assert!(store.list_by_domain("other").unwrap().is_empty());
    }

    #[test]
    fn test_delete() {
        let store = InMemoryMeetingStore::new();
        let id = MeetingId::from_raw("temp");
        store.save(&make_meeting("temp", "domain")).unwrap();
        assert!(store.delete(&id).unwrap());
        assert!(store.get(&id).unwrap().is_none());
        assert!(!store.delete(&id).unwrap()); // idempotent
    }

    #[test]
    fn test_serde_defaults() {
        let json = r#"{
            "id": "m-abc",
            "domain_id": "nycn",
            "title": "Test",
            "created_by": "did:icn:alice",
            "created_at": 1000
        }"#;
        let m: Meeting = serde_json::from_str(json).unwrap();
        assert_eq!(m.id.0, "m-abc");
        assert_eq!(m.status, MeetingStatus::Scheduled);
        assert!(m.attendees.is_empty());
        assert!(m.agenda.is_empty());
        assert!(m.linked_structures.is_empty());
        assert!(m.notes_doc_id.is_none());
    }

    #[test]
    fn test_meeting_role_is_not_authority() {
        // Facilitator and NoteTaker roles are coordination labels.
        // This test asserts they are distinct from governance roles.
        let facilitator = MeetingRole::Facilitator;
        let note_taker = MeetingRole::NoteTaker;
        assert_ne!(facilitator, note_taker);
        // They serialize to snake_case strings, not special authority tokens.
        let s = serde_json::to_string(&facilitator).unwrap();
        assert_eq!(s, "\"facilitator\"");
    }
}
