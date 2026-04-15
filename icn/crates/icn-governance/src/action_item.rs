//! Action Item tracking for governance domains
//!
//! Enables cooperatives to track action items from meetings, link them to proposals,
//! and maintain accountability across sessions.

use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::{GovernanceDomainId, ProposalId, Result};

/// Unique identifier for an action item
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ActionItemId(pub Uuid);

impl ActionItemId {
    /// Create a new random action item ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create from a UUID
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for ActionItemId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ActionItemId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for ActionItemId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

/// Status of an action item
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionItemStatus {
    /// Not yet started
    #[default]
    Pending,
    /// Work in progress
    InProgress,
    /// Successfully completed
    Completed,
    /// Postponed to a later date
    Deferred,
    /// No longer relevant
    Cancelled,
}

impl std::fmt::Display for ActionItemStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Completed => write!(f, "completed"),
            Self::Deferred => write!(f, "deferred"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Priority level for action items
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionItemPriority {
    Low,
    #[default]
    Medium,
    High,
    Critical,
}

/// A template for creating action items when a proposal is accepted.
///
/// Attached to proposals via `Proposal::action_items_on_accept`. When the proposal
/// is accepted, each spec produces an `ActionItem` linked to the originating proposal
/// via `linked_proposal`. This is the first bridge pattern for obligation-producing
/// proposals — other proposal types (Budget→ledger, Charter→oracle) keep their own
/// execution hooks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItemSpec {
    /// Short title describing the action
    pub title: String,

    /// Detailed description (optional)
    #[serde(default)]
    pub description: Option<String>,

    /// Member responsible for this item (optional DID)
    #[serde(default)]
    pub assignee: Option<Did>,

    /// Seconds after proposal acceptance before this item is due (optional).
    /// Converted to absolute timestamp at creation time.
    #[serde(default)]
    pub due_offset_seconds: Option<u64>,

    /// Priority level
    #[serde(default)]
    pub priority: ActionItemPriority,

    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,
}

impl ActionItemSpec {
    /// Materialize this spec into a concrete ActionItem linked to the given proposal.
    ///
    /// The `now` timestamp is used for created_at and to compute due_date from the offset.
    pub fn materialize(
        &self,
        domain_id: GovernanceDomainId,
        proposal_id: ProposalId,
        created_by: Did,
        now: u64,
    ) -> ActionItem {
        let mut item = ActionItem::new(domain_id, self.title.clone(), created_by, now);
        item.description = self.description.clone();
        item.assignee = self.assignee.clone();
        item.due_date = self
            .due_offset_seconds
            .map(|offset| now.saturating_add(offset));
        item.priority = self.priority;
        item.tags = self.tags.clone();
        item.linked_proposal = Some(proposal_id);
        item
    }
}

/// An action item tracked within a governance domain
///
/// Note: Option fields use `#[serde(default)]` for binary serialization compatibility
/// with postcard. The `skip_serializing_if` is only used for JSON API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    /// Unique identifier
    pub id: ActionItemId,

    /// Domain this action item belongs to
    pub domain_id: GovernanceDomainId,

    /// Short title describing the action
    pub title: String,

    /// Detailed description (optional)
    #[serde(default)]
    pub description: Option<String>,

    /// Member responsible for this item (optional)
    #[serde(default)]
    pub assignee: Option<Did>,

    /// Due date as Unix timestamp (optional)
    #[serde(default)]
    pub due_date: Option<u64>,

    /// Current status
    pub status: ActionItemStatus,

    /// Priority level
    pub priority: ActionItemPriority,

    /// Who created this action item
    pub created_by: Did,

    /// When this was created (Unix timestamp)
    pub created_at: u64,

    /// When this was last updated (Unix timestamp)
    pub updated_at: u64,

    /// Linked proposal (if this came from a proposal discussion)
    #[serde(default)]
    pub linked_proposal: Option<ProposalId>,

    /// Meeting context (e.g., "2026-01-17 board meeting"). Free-text; kept for
    /// backward compatibility. Prefer `meeting_id` for structured linkage.
    #[serde(default)]
    pub meeting_context: Option<String>,

    /// Typed link to the meeting that generated this action item.
    ///
    /// Optional reference to the meeting in which this action item originated.
    /// Set by callers when creating action items from a meeting context.
    /// Coexists with `meeting_context` (the free-text field remains for backward
    /// compatibility and manual entry).
    #[serde(default)]
    pub meeting_id: Option<crate::meeting::MeetingId>,

    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,

    /// Notes/updates history
    #[serde(default)]
    pub notes: Vec<ActionItemNote>,

    /// Institutional parent (entity, structure, or activity).
    ///
    /// When set, ties this action item to a specific institutional scope
    /// beyond the governance domain (e.g., attached to a committee structure
    /// or summit event). Coexists with `linked_proposal` — an action item can
    /// belong to a committee (parent) and also be generated by a proposal
    /// (linked_proposal).
    #[serde(default)]
    pub parent: Option<crate::parent::InstitutionalParent>,
}

impl ActionItem {
    /// Create a new action item
    pub fn new(domain_id: GovernanceDomainId, title: String, created_by: Did, now: u64) -> Self {
        Self {
            id: ActionItemId::new(),
            domain_id,
            title,
            description: None,
            assignee: None,
            due_date: None,
            status: ActionItemStatus::Pending,
            priority: ActionItemPriority::Medium,
            created_by,
            created_at: now,
            updated_at: now,
            linked_proposal: None,
            meeting_context: None,
            meeting_id: None,
            tags: Vec::new(),
            notes: Vec::new(),
            parent: None,
        }
    }

    /// Check if this action item is overdue.
    ///
    /// An item is overdue if:
    /// - It has a due date that has passed (now > due_date, i.e., after the deadline)
    /// - It is not already completed or cancelled
    ///
    /// Note: Items are NOT overdue at exactly the due date - only after it passes.
    pub fn is_overdue(&self, now: u64) -> bool {
        if self.status == ActionItemStatus::Completed || self.status == ActionItemStatus::Cancelled
        {
            return false;
        }
        self.due_date.is_some_and(|due| now > due)
    }

    /// Check if this item is open (not completed or cancelled)
    pub fn is_open(&self) -> bool {
        !matches!(
            self.status,
            ActionItemStatus::Completed | ActionItemStatus::Cancelled
        )
    }

    /// Add a note to this action item
    pub fn add_note(&mut self, author: Did, content: String, now: u64) {
        self.notes.push(ActionItemNote {
            id: Uuid::new_v4(),
            author,
            content,
            created_at: now,
        });
        self.updated_at = now;
    }

    /// Update the status
    pub fn set_status(&mut self, status: ActionItemStatus, now: u64) {
        self.status = status;
        self.updated_at = now;
    }

    /// Assign to a member
    pub fn assign(&mut self, assignee: Did, now: u64) {
        self.assignee = Some(assignee);
        self.updated_at = now;
    }

    /// Unassign
    pub fn unassign(&mut self, now: u64) {
        self.assignee = None;
        self.updated_at = now;
    }
}

/// A note/update on an action item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItemNote {
    /// Unique note ID
    pub id: Uuid,

    /// Who wrote this note
    pub author: Did,

    /// Note content
    pub content: String,

    /// When this note was added
    pub created_at: u64,
}

/// Filter criteria for listing action items
#[derive(Debug, Clone, Default)]
pub struct ActionItemFilter {
    /// Filter by status
    pub status: Option<ActionItemStatus>,

    /// Filter by assignee
    pub assignee: Option<Did>,

    /// Filter by priority
    pub priority: Option<ActionItemPriority>,

    /// Filter by linked proposal
    pub linked_proposal: Option<ProposalId>,

    /// Filter by tag
    pub tag: Option<String>,

    /// Only show overdue items (requires current timestamp)
    pub overdue_only: Option<u64>,

    /// Only show open items
    pub open_only: bool,

    /// Filter by institutional parent (entity, structure, or activity)
    pub parent: Option<crate::parent::InstitutionalParent>,
}

impl ActionItemFilter {
    /// Create a filter for items assigned to a specific member
    pub fn assigned_to(did: Did) -> Self {
        Self {
            assignee: Some(did),
            open_only: true,
            ..Default::default()
        }
    }

    /// Create a filter for overdue items
    pub fn overdue(now: u64) -> Self {
        Self {
            overdue_only: Some(now),
            open_only: true,
            ..Default::default()
        }
    }

    /// Check if an action item matches this filter
    pub fn matches(&self, item: &ActionItem) -> bool {
        if let Some(status) = self.status {
            if item.status != status {
                return false;
            }
        }

        if let Some(ref assignee) = self.assignee {
            if item.assignee.as_ref() != Some(assignee) {
                return false;
            }
        }

        if let Some(priority) = self.priority {
            if item.priority != priority {
                return false;
            }
        }

        if let Some(ref proposal_id) = self.linked_proposal {
            if item.linked_proposal.as_ref() != Some(proposal_id) {
                return false;
            }
        }

        if let Some(ref tag) = self.tag {
            if !item.tags.contains(tag) {
                return false;
            }
        }

        if let Some(now) = self.overdue_only {
            if !item.is_overdue(now) {
                return false;
            }
        }

        if self.open_only && !item.is_open() {
            return false;
        }

        if let Some(ref parent) = self.parent {
            if item.parent.as_ref() != Some(parent) {
                return false;
            }
        }

        true
    }
}

/// Storage backend for action items
pub trait ActionItemStoreBackend: Send + Sync {
    /// Save an action item
    fn save(&self, item: &ActionItem) -> Result<()>;

    /// Get an action item by ID
    fn get(&self, domain_id: &GovernanceDomainId, id: &ActionItemId) -> Result<Option<ActionItem>>;

    /// List action items for a domain with optional filter
    fn list(
        &self,
        domain_id: &GovernanceDomainId,
        filter: &ActionItemFilter,
    ) -> Result<Vec<ActionItem>>;

    /// Delete an action item
    fn delete(&self, domain_id: &GovernanceDomainId, id: &ActionItemId) -> Result<bool>;

    /// Count action items matching filter
    fn count(&self, domain_id: &GovernanceDomainId, filter: &ActionItemFilter) -> Result<usize>;

    /// Delete all action items for a domain
    fn delete_all(&self, domain_id: &GovernanceDomainId) -> Result<usize>;

    /// List all action items assigned to a specific DID across all domains.
    ///
    /// Implementations backed by a secondary index (e.g. Sled) override this for
    /// O(index-hits) performance. The default falls back to returning an empty
    /// vec — in-memory stores (tests) degrade gracefully; callers that need
    /// cross-domain assignee results in tests should use the per-domain `list`
    /// method with `ActionItemFilter::assigned_to`.
    fn list_by_assignee(&self, _assignee: &Did) -> Result<Vec<ActionItem>> {
        Ok(vec![])
    }
}

/// In-memory action item store for testing
///
/// # Concurrency Model
/// Uses RwLock for exclusive write access, preventing lost updates within a single
/// process. For horizontal scaling, version-based optimistic locking would be needed.
/// Deferred as the pilot phase uses a single gateway instance.
#[derive(Debug, Default)]
pub struct InMemoryActionItemStore {
    items: std::sync::RwLock<HashMap<(String, Uuid), ActionItem>>,
}

impl InMemoryActionItemStore {
    /// Create a new in-memory store
    pub fn new() -> Self {
        Self::default()
    }
}

impl ActionItemStoreBackend for InMemoryActionItemStore {
    fn save(&self, item: &ActionItem) -> Result<()> {
        let key = (item.domain_id.0.clone(), item.id.0);
        let mut items = self
            .items
            .write()
            .map_err(|_| crate::GovernanceError::LockPoisoned("action item store".to_string()))?;
        items.insert(key, item.clone());
        Ok(())
    }

    fn get(&self, domain_id: &GovernanceDomainId, id: &ActionItemId) -> Result<Option<ActionItem>> {
        let key = (domain_id.0.clone(), id.0);
        let items = self
            .items
            .read()
            .map_err(|_| crate::GovernanceError::LockPoisoned("action item store".to_string()))?;
        Ok(items.get(&key).cloned())
    }

    fn list(
        &self,
        domain_id: &GovernanceDomainId,
        filter: &ActionItemFilter,
    ) -> Result<Vec<ActionItem>> {
        let items = self
            .items
            .read()
            .map_err(|_| crate::GovernanceError::LockPoisoned("action item store".to_string()))?;

        let mut result: Vec<_> = items
            .values()
            .filter(|item| item.domain_id == *domain_id && filter.matches(item))
            .cloned()
            .collect();

        // Sort by due date (nulls last), then by priority, then by created_at
        result.sort_by(|a, b| {
            // First by due date (items with due dates come first, then by date)
            match (a.due_date, b.due_date) {
                (Some(a_due), Some(b_due)) => a_due.cmp(&b_due),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => {
                    // Then by priority (higher priority first)
                    let priority_order = |p: &ActionItemPriority| match p {
                        ActionItemPriority::Critical => 0,
                        ActionItemPriority::High => 1,
                        ActionItemPriority::Medium => 2,
                        ActionItemPriority::Low => 3,
                    };
                    match priority_order(&a.priority).cmp(&priority_order(&b.priority)) {
                        std::cmp::Ordering::Equal => a.created_at.cmp(&b.created_at),
                        other => other,
                    }
                }
            }
        });

        Ok(result)
    }

    fn delete(&self, domain_id: &GovernanceDomainId, id: &ActionItemId) -> Result<bool> {
        let key = (domain_id.0.clone(), id.0);
        let mut items = self
            .items
            .write()
            .map_err(|_| crate::GovernanceError::LockPoisoned("action item store".to_string()))?;
        Ok(items.remove(&key).is_some())
    }

    fn count(&self, domain_id: &GovernanceDomainId, filter: &ActionItemFilter) -> Result<usize> {
        let items = self
            .items
            .read()
            .map_err(|_| crate::GovernanceError::LockPoisoned("action item store".to_string()))?;

        Ok(items
            .values()
            .filter(|item| item.domain_id == *domain_id && filter.matches(item))
            .count())
    }

    fn delete_all(&self, domain_id: &GovernanceDomainId) -> Result<usize> {
        let mut items = self
            .items
            .write()
            .map_err(|_| crate::GovernanceError::LockPoisoned("action item store".to_string()))?;

        let keys_to_remove: Vec<_> = items
            .keys()
            .filter(|(d, _)| d == &domain_id.0)
            .cloned()
            .collect();

        let count = keys_to_remove.len();
        for key in keys_to_remove {
            items.remove(&key);
        }

        Ok(count)
    }
}

/// Sled key version for schema migration support.
/// When the schema changes, increment this version and add migration logic.
const SLED_KEY_VERSION: &str = "v1";

/// Sled-backed persistent action item store
pub struct SledActionItemStore {
    db: std::sync::Arc<sled::Db>,
}

impl SledActionItemStore {
    /// Create a new Sled-backed action item store
    pub fn new(db: std::sync::Arc<sled::Db>) -> Self {
        Self { db }
    }

    /// Generate a versioned key for action items
    #[inline]
    fn item_key(domain_id: &GovernanceDomainId, id: &ActionItemId) -> String {
        format!("{SLED_KEY_VERSION}:action_item:{}:{}", domain_id.0, id.0)
    }

    /// Generate a versioned key prefix for domain scans
    #[inline]
    fn domain_prefix(domain_id: &GovernanceDomainId) -> String {
        format!("{SLED_KEY_VERSION}:action_item:{}:", domain_id.0)
    }
}

impl ActionItemStoreBackend for SledActionItemStore {
    fn save(&self, item: &ActionItem) -> Result<()> {
        let key = Self::item_key(&item.domain_id, &item.id);
        let value = icn_encoding::encode_versioned(item)
            .map_err(|e| crate::GovernanceError::Storage(e.to_string()))?;
        self.db
            .insert(key.as_bytes(), value)
            .map_err(|e| crate::GovernanceError::Storage(e.to_string()))?;
        Ok(())
    }

    fn get(&self, domain_id: &GovernanceDomainId, id: &ActionItemId) -> Result<Option<ActionItem>> {
        let key = Self::item_key(domain_id, id);
        match self
            .db
            .get(key.as_bytes())
            .map_err(|e| crate::GovernanceError::Storage(e.to_string()))?
        {
            Some(value) => {
                let item: ActionItem = icn_encoding::decode_versioned(&value)
                    .map_err(|e| crate::GovernanceError::Storage(e.to_string()))?;
                Ok(Some(item))
            }
            None => Ok(None),
        }
    }

    fn list(
        &self,
        domain_id: &GovernanceDomainId,
        filter: &ActionItemFilter,
    ) -> Result<Vec<ActionItem>> {
        let prefix = Self::domain_prefix(domain_id);
        let mut result = Vec::new();

        for item in self.db.scan_prefix(prefix.as_bytes()) {
            let (_, value) = item.map_err(|e| crate::GovernanceError::Storage(e.to_string()))?;
            if let Ok(action_item) = icn_encoding::decode_versioned::<ActionItem>(&value) {
                if filter.matches(&action_item) {
                    result.push(action_item);
                }
            }
        }

        // Sort by due date (nulls last), then by priority, then by created_at
        result.sort_by(|a, b| match (a.due_date, b.due_date) {
            (Some(a_due), Some(b_due)) => a_due.cmp(&b_due),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => {
                let priority_order = |p: &ActionItemPriority| match p {
                    ActionItemPriority::Critical => 0,
                    ActionItemPriority::High => 1,
                    ActionItemPriority::Medium => 2,
                    ActionItemPriority::Low => 3,
                };
                match priority_order(&a.priority).cmp(&priority_order(&b.priority)) {
                    std::cmp::Ordering::Equal => a.created_at.cmp(&b.created_at),
                    other => other,
                }
            }
        });

        Ok(result)
    }

    fn delete(&self, domain_id: &GovernanceDomainId, id: &ActionItemId) -> Result<bool> {
        let key = Self::item_key(domain_id, id);
        let existed = self
            .db
            .remove(key.as_bytes())
            .map_err(|e| crate::GovernanceError::Storage(e.to_string()))?
            .is_some();
        Ok(existed)
    }

    fn count(&self, domain_id: &GovernanceDomainId, filter: &ActionItemFilter) -> Result<usize> {
        let prefix = Self::domain_prefix(domain_id);
        let mut count = 0;

        for item in self.db.scan_prefix(prefix.as_bytes()) {
            let (_, value) = item.map_err(|e| crate::GovernanceError::Storage(e.to_string()))?;
            if let Ok(action_item) = icn_encoding::decode_versioned::<ActionItem>(&value) {
                if filter.matches(&action_item) {
                    count += 1;
                }
            }
        }

        Ok(count)
    }

    fn delete_all(&self, domain_id: &GovernanceDomainId) -> Result<usize> {
        let prefix = Self::domain_prefix(domain_id);
        let mut count = 0;

        // Collect keys first to avoid iterator invalidation
        let keys: Vec<_> = self
            .db
            .scan_prefix(prefix.as_bytes())
            .filter_map(|r| r.ok().map(|(k, _)| k))
            .collect();

        for key in keys {
            if self
                .db
                .remove(key)
                .map_err(|e| crate::GovernanceError::Storage(e.to_string()))?
                .is_some()
            {
                count += 1;
            }
        }

        Ok(count)
    }
}

/// Action item store with pluggable backend
pub struct ActionItemStore {
    backend: Box<dyn ActionItemStoreBackend>,
}

impl ActionItemStore {
    /// Create a new store with the given backend
    pub fn new(backend: Box<dyn ActionItemStoreBackend>) -> Self {
        Self { backend }
    }

    /// Create an in-memory store
    pub fn in_memory() -> Self {
        Self::new(Box::new(InMemoryActionItemStore::new()))
    }

    /// Create a Sled-backed persistent store
    pub fn with_sled(db: std::sync::Arc<sled::Db>) -> Self {
        Self::new(Box::new(SledActionItemStore::new(db)))
    }

    /// Save an action item
    pub fn save(&self, item: &ActionItem) -> Result<()> {
        self.backend.save(item)
    }

    /// Get an action item by ID
    pub fn get(
        &self,
        domain_id: &GovernanceDomainId,
        id: &ActionItemId,
    ) -> Result<Option<ActionItem>> {
        self.backend.get(domain_id, id)
    }

    /// List action items for a domain with optional filter
    pub fn list(
        &self,
        domain_id: &GovernanceDomainId,
        filter: &ActionItemFilter,
    ) -> Result<Vec<ActionItem>> {
        self.backend.list(domain_id, filter)
    }

    /// Delete an action item
    pub fn delete(&self, domain_id: &GovernanceDomainId, id: &ActionItemId) -> Result<bool> {
        self.backend.delete(domain_id, id)
    }

    /// Count action items matching filter
    pub fn count(
        &self,
        domain_id: &GovernanceDomainId,
        filter: &ActionItemFilter,
    ) -> Result<usize> {
        self.backend.count(domain_id, filter)
    }

    /// Delete all action items in a domain
    pub fn delete_all(&self, domain_id: &GovernanceDomainId) -> Result<usize> {
        self.backend.delete_all(domain_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_test_sled_db() -> (sled::Db, tempfile::TempDir) {
        let base = std::env::var_os("ICN_TEST_TMPDIR")
            .or_else(|| std::env::var_os("TMPDIR"))
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
        let tempdir = tempfile::Builder::new()
            .prefix("icn-governance-action-items-")
            .tempdir_in(base)
            .expect("Failed to create temp db directory");
        let db = sled::Config::new()
            .path(tempdir.path().join("sled"))
            .temporary(true)
            .open()
            .expect("Failed to create temp db");
        (db, tempdir)
    }

    fn test_did() -> Did {
        // Create deterministic DID for testing
        Did::from_anchor_id(&[1; 32])
    }

    fn test_domain() -> GovernanceDomainId {
        GovernanceDomainId("test-domain".to_string())
    }

    #[test]
    fn test_create_action_item() {
        let item = ActionItem::new(
            test_domain(),
            "Review budget proposal".to_string(),
            test_did(),
            1000,
        );

        assert_eq!(item.title, "Review budget proposal");
        assert_eq!(item.status, ActionItemStatus::Pending);
        assert!(item.is_open());
        assert!(!item.is_overdue(1000));
    }

    #[test]
    fn test_overdue_detection() {
        let mut item =
            ActionItem::new(test_domain(), "Submit report".to_string(), test_did(), 1000);

        // No due date - not overdue
        assert!(!item.is_overdue(2000));

        // Set due date in past
        item.due_date = Some(1500);
        assert!(item.is_overdue(2000));
        assert!(!item.is_overdue(1000));

        // Edge case: exactly at due date - NOT overdue (only after deadline)
        assert!(
            !item.is_overdue(1500),
            "Item at exact due date should NOT be overdue - only after deadline passes"
        );
        // One second after due date - now it's overdue
        assert!(
            item.is_overdue(1501),
            "Item should be overdue after deadline passes"
        );

        // Completed items are never overdue
        item.status = ActionItemStatus::Completed;
        assert!(!item.is_overdue(2000));

        // Cancelled items are never overdue
        item.status = ActionItemStatus::Cancelled;
        assert!(!item.is_overdue(2000));
    }

    #[test]
    fn test_filter_matches() {
        let mut item = ActionItem::new(test_domain(), "Test item".to_string(), test_did(), 1000);
        item.assignee = Some(test_did());
        item.priority = ActionItemPriority::High;
        item.tags = vec!["budget".to_string()];

        // Empty filter matches all
        assert!(ActionItemFilter::default().matches(&item));

        // Status filter
        assert!(ActionItemFilter {
            status: Some(ActionItemStatus::Pending),
            ..Default::default()
        }
        .matches(&item));

        assert!(!ActionItemFilter {
            status: Some(ActionItemStatus::Completed),
            ..Default::default()
        }
        .matches(&item));

        // Assignee filter
        assert!(ActionItemFilter::assigned_to(test_did()).matches(&item));

        // Tag filter
        assert!(ActionItemFilter {
            tag: Some("budget".to_string()),
            ..Default::default()
        }
        .matches(&item));

        assert!(!ActionItemFilter {
            tag: Some("other".to_string()),
            ..Default::default()
        }
        .matches(&item));
    }

    #[test]
    fn test_in_memory_store() {
        let store = ActionItemStore::in_memory();
        let domain = test_domain();

        // Create and save
        let item = ActionItem::new(domain.clone(), "Test task".to_string(), test_did(), 1000);
        let item_id = item.id.clone();

        store.save(&item).expect("save should succeed");

        // Retrieve
        let retrieved = store.get(&domain, &item_id).expect("get should succeed");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().title, "Test task");

        // List
        let items = store
            .list(&domain, &ActionItemFilter::default())
            .expect("list should succeed");
        assert_eq!(items.len(), 1);

        // Count
        let count = store
            .count(&domain, &ActionItemFilter::default())
            .expect("count should succeed");
        assert_eq!(count, 1);

        // Delete
        let deleted = store
            .delete(&domain, &item_id)
            .expect("delete should succeed");
        assert!(deleted);

        let retrieved = store.get(&domain, &item_id).expect("get should succeed");
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_sled_store_roundtrip() {
        // Create a temporary Sled database
        let (db, _tempdir) = open_test_sled_db();
        let store = ActionItemStore::with_sled(std::sync::Arc::new(db));
        let domain = test_domain();

        // Create and save
        let item = ActionItem::new(
            domain.clone(),
            "Sled test task".to_string(),
            test_did(),
            1000,
        );
        let item_id = item.id.clone();

        store.save(&item).expect("save should succeed");

        // Retrieve
        let retrieved = store.get(&domain, &item_id).expect("get should succeed");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().title, "Sled test task");

        // List
        let items = store
            .list(&domain, &ActionItemFilter::default())
            .expect("list should succeed");
        assert_eq!(items.len(), 1);

        // Count
        let count = store
            .count(&domain, &ActionItemFilter::default())
            .expect("count should succeed");
        assert_eq!(count, 1);

        // Delete
        let deleted = store
            .delete(&domain, &item_id)
            .expect("delete should succeed");
        assert!(deleted);

        let retrieved = store.get(&domain, &item_id).expect("get should succeed");
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_sled_delete_all() {
        // Create a temporary Sled database
        let (db, _tempdir) = open_test_sled_db();
        let store = ActionItemStore::with_sled(std::sync::Arc::new(db));
        let domain1 = test_domain();
        let domain2 = GovernanceDomainId("other-domain".to_string());

        // Create items in two domains
        let item1 = ActionItem::new(
            domain1.clone(),
            "Domain 1 task 1".to_string(),
            test_did(),
            1000,
        );
        let item2 = ActionItem::new(
            domain1.clone(),
            "Domain 1 task 2".to_string(),
            test_did(),
            1001,
        );
        let item3 = ActionItem::new(
            domain2.clone(),
            "Domain 2 task 1".to_string(),
            test_did(),
            1002,
        );

        store.save(&item1).unwrap();
        store.save(&item2).unwrap();
        store.save(&item3).unwrap();

        // Verify counts
        assert_eq!(
            store.count(&domain1, &ActionItemFilter::default()).unwrap(),
            2
        );
        assert_eq!(
            store.count(&domain2, &ActionItemFilter::default()).unwrap(),
            1
        );

        // Delete all from domain1
        let deleted = store.delete_all(&domain1).unwrap();
        assert_eq!(deleted, 2);

        // Verify domain1 is empty, domain2 unchanged
        assert_eq!(
            store.count(&domain1, &ActionItemFilter::default()).unwrap(),
            0
        );
        assert_eq!(
            store.count(&domain2, &ActionItemFilter::default()).unwrap(),
            1
        );
    }

    #[test]
    fn test_action_item_spec_materialize() {
        let spec = ActionItemSpec {
            title: "Draft committee report".to_string(),
            description: Some("Prepare Q1 report for the board".to_string()),
            assignee: Some(test_did()),
            due_offset_seconds: Some(7 * 24 * 3600), // 1 week
            priority: ActionItemPriority::High,
            tags: vec!["committee".to_string(), "report".to_string()],
        };

        let domain = test_domain();
        let proposal_id = crate::ProposalId("prop-123".to_string());
        let proposer = test_did();
        let now = 1_000_000u64;

        let item = spec.materialize(domain.clone(), proposal_id.clone(), proposer.clone(), now);

        assert_eq!(item.domain_id, domain);
        assert_eq!(item.title, "Draft committee report");
        assert_eq!(
            item.description,
            Some("Prepare Q1 report for the board".to_string())
        );
        assert_eq!(item.assignee, Some(test_did()));
        assert_eq!(item.due_date, Some(now + 7 * 24 * 3600));
        assert_eq!(item.priority, ActionItemPriority::High);
        assert_eq!(item.tags, vec!["committee", "report"]);
        assert_eq!(item.linked_proposal, Some(proposal_id));
        assert_eq!(item.status, ActionItemStatus::Pending);
        assert_eq!(item.created_at, now);
    }

    #[test]
    fn test_action_item_spec_materialize_minimal() {
        let spec = ActionItemSpec {
            title: "Follow up".to_string(),
            description: None,
            assignee: None,
            due_offset_seconds: None,
            priority: ActionItemPriority::Medium,
            tags: Vec::new(),
        };

        let item = spec.materialize(
            test_domain(),
            crate::ProposalId("prop-456".to_string()),
            test_did(),
            500,
        );

        assert_eq!(item.title, "Follow up");
        assert!(item.description.is_none());
        assert!(item.assignee.is_none());
        assert!(item.due_date.is_none());
        assert_eq!(item.priority, ActionItemPriority::Medium);
        assert!(item.tags.is_empty());
        assert!(item.linked_proposal.is_some());
    }

    #[test]
    fn test_action_item_spec_materialize_saturating_due_date() {
        let spec = ActionItemSpec {
            title: "Overflow test".to_string(),
            description: None,
            assignee: None,
            due_offset_seconds: Some(u64::MAX),
            priority: ActionItemPriority::Medium,
            tags: Vec::new(),
        };

        let item = spec.materialize(
            test_domain(),
            crate::ProposalId("prop-overflow".to_string()),
            test_did(),
            1_000_000,
        );

        // Should saturate to u64::MAX, not panic or wrap
        assert_eq!(item.due_date, Some(u64::MAX));
    }

    #[test]
    fn test_materialize_creates_linked_items_in_store() {
        let store = ActionItemStore::in_memory();
        let domain = test_domain();
        let proposal_id = crate::ProposalId("prop-accepted".to_string());

        let specs = vec![
            ActionItemSpec {
                title: "Task A".to_string(),
                description: None,
                assignee: None,
                due_offset_seconds: Some(3600),
                priority: ActionItemPriority::High,
                tags: vec!["urgent".to_string()],
            },
            ActionItemSpec {
                title: "Task B".to_string(),
                description: Some("Second task".to_string()),
                assignee: Some(test_did()),
                due_offset_seconds: None,
                priority: ActionItemPriority::Low,
                tags: Vec::new(),
            },
        ];

        let now = 1_000_000u64;
        for spec in &specs {
            let item = spec.materialize(domain.clone(), proposal_id.clone(), test_did(), now);
            store.save(&item).unwrap();
        }

        // All items linked to the proposal
        let linked = store
            .list(
                &domain,
                &ActionItemFilter {
                    linked_proposal: Some(proposal_id.clone()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(linked.len(), 2);
        assert!(linked
            .iter()
            .all(|i| i.linked_proposal.as_ref() == Some(&proposal_id)));

        // Idempotency check: if items already exist, a second pass would find them
        let existing = store
            .list(
                &domain,
                &ActionItemFilter {
                    linked_proposal: Some(proposal_id),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(existing.len(), 2, "dedup check should find existing items");
    }

    #[test]
    fn test_action_item_spec_serde_defaults() {
        // Minimal JSON — all optional fields omitted
        let json = r#"{"title":"Minimal task"}"#;
        let spec: ActionItemSpec = serde_json::from_str(json).unwrap();

        assert_eq!(spec.title, "Minimal task");
        assert!(spec.description.is_none());
        assert!(spec.assignee.is_none());
        assert!(spec.due_offset_seconds.is_none());
        assert_eq!(spec.priority, ActionItemPriority::Medium);
        assert!(spec.tags.is_empty());
    }

    #[test]
    fn test_action_item_parent_attachment() {
        use crate::activity::ActivityId;
        use crate::parent::InstitutionalParent;
        use crate::structure::StructureId;

        let store = ActionItemStore::in_memory();
        let domain = test_domain();

        // Item attached to a structure (committee)
        let mut item1 = ActionItem::new(
            domain.clone(),
            "Committee task".to_string(),
            test_did(),
            1000,
        );
        item1.parent = Some(InstitutionalParent::structure(StructureId::from_raw(
            "nycn-finance",
        )));

        // Item attached to an activity (event)
        let mut item2 =
            ActionItem::new(domain.clone(), "Summit task".to_string(), test_did(), 1000);
        item2.parent = Some(InstitutionalParent::activity(ActivityId::from_raw(
            "summit-2026",
        )));

        // Item attached to an entity
        let mut item3 = ActionItem::new(
            domain.clone(),
            "Federation task".to_string(),
            test_did(),
            1000,
        );
        item3.parent = Some(InstitutionalParent::entity("nycn"));

        // Item with no parent (backward compat)
        let item4 = ActionItem::new(
            domain.clone(),
            "Unscoped task".to_string(),
            test_did(),
            1000,
        );
        assert!(item4.parent.is_none());

        store.save(&item1).unwrap();
        store.save(&item2).unwrap();
        store.save(&item3).unwrap();
        store.save(&item4).unwrap();

        // Filter by structure parent
        let struct_parent = InstitutionalParent::structure(StructureId::from_raw("nycn-finance"));
        let items = store
            .list(
                &domain,
                &ActionItemFilter {
                    parent: Some(struct_parent),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Committee task");

        // Filter by activity parent
        let act_parent = InstitutionalParent::activity(ActivityId::from_raw("summit-2026"));
        let items = store
            .list(
                &domain,
                &ActionItemFilter {
                    parent: Some(act_parent),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Summit task");
    }

    #[test]
    fn test_action_item_backward_compat_no_parent() {
        // Simulate old JSON that predates the `parent` field: the key is missing entirely.
        // serde should deserialize it via `#[serde(default)]` without error.
        let item = ActionItem::new(test_domain(), "x".to_string(), test_did(), 1000);
        assert!(item.parent.is_none());
        let mut json_value: serde_json::Value = serde_json::to_value(&item).unwrap();
        // Remove the `parent` key to replicate a payload from before the field existed
        json_value.as_object_mut().unwrap().remove("parent");
        assert!(!json_value.as_object().unwrap().contains_key("parent"));
        let json = serde_json::to_string(&json_value).unwrap();
        let round: ActionItem = serde_json::from_str(&json).unwrap();
        assert!(round.parent.is_none());
    }
}
