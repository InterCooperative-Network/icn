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

/// An action item tracked within a governance domain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionItem {
    /// Unique identifier
    pub id: ActionItemId,

    /// Domain this action item belongs to
    pub domain_id: GovernanceDomainId,

    /// Short title describing the action
    pub title: String,

    /// Detailed description (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Member responsible for this item (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<Did>,

    /// Due date as Unix timestamp (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_proposal: Option<ProposalId>,

    /// Meeting context (e.g., "2026-01-17 board meeting")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meeting_context: Option<String>,

    /// Tags for categorization
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// Notes/updates history
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<ActionItemNote>,
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
            tags: Vec::new(),
            notes: Vec::new(),
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
}

/// In-memory action item store for testing
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
