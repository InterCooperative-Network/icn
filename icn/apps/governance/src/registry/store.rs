//! Decision Registry in-memory storage.
//!
//! Provides thread-safe storage for meetings and decision index entries.
//! This is an APP-LAYER store, not a kernel primitive.

use std::collections::HashMap;
use std::sync::RwLock;

use anyhow::{anyhow, Result};

use super::models::{DecisionFilter, DecisionIndexEntry, Meeting};

/// In-memory store for governance meetings and decisions.
///
/// This registry provides a "Google Drive replacement" for co-op meetings
/// and decisions. It indexes over canonical receipts without touching
/// kernel primitives.
///
/// Thread-safe via `RwLock`.
#[derive(Debug, Default)]
pub struct DecisionRegistry {
    /// Meetings indexed by meeting_id.
    meetings: RwLock<HashMap<String, Meeting>>,
    /// Decisions indexed by decision_receipt_id.
    decisions: RwLock<HashMap<String, DecisionIndexEntry>>,
}

impl DecisionRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a meeting to the registry.
    ///
    /// Returns an error if a meeting with the same ID already exists.
    pub fn add_meeting(&self, meeting: Meeting) -> Result<()> {
        let mut meetings = self
            .meetings
            .write()
            .map_err(|_| anyhow!("failed to acquire meetings write lock"))?;

        if meetings.contains_key(&meeting.meeting_id) {
            return Err(anyhow!("meeting already exists: {}", meeting.meeting_id));
        }

        meetings.insert(meeting.meeting_id.clone(), meeting);
        Ok(())
    }

    /// Get a meeting by ID.
    pub fn get_meeting(&self, meeting_id: &str) -> Option<Meeting> {
        self.meetings
            .read()
            .ok()
            .and_then(|m| m.get(meeting_id).cloned())
    }

    /// List all meetings for a cooperative.
    pub fn list_meetings(&self, coop_id: &str) -> Vec<Meeting> {
        self.meetings
            .read()
            .map(|m| {
                m.values()
                    .filter(|meeting| meeting.coop_id == coop_id)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Index a decision entry.
    ///
    /// If an entry with the same receipt ID exists, it will be replaced.
    pub fn index_decision(&self, entry: DecisionIndexEntry) -> Result<()> {
        let mut decisions = self
            .decisions
            .write()
            .map_err(|_| anyhow!("failed to acquire decisions write lock"))?;

        decisions.insert(entry.decision_receipt_id.clone(), entry);
        Ok(())
    }

    /// Get a decision by receipt ID.
    pub fn get_decision(&self, receipt_id: &str) -> Option<DecisionIndexEntry> {
        self.decisions
            .read()
            .ok()
            .and_then(|d| d.get(receipt_id).cloned())
    }

    /// List decisions matching the given filter.
    pub fn list_decisions(&self, filter: DecisionFilter) -> Vec<DecisionIndexEntry> {
        self.decisions
            .read()
            .map(|d| d.values().filter(|e| filter.matches(e)).cloned().collect())
            .unwrap_or_default()
    }

    /// Get the count of meetings in the registry.
    pub fn meeting_count(&self) -> usize {
        self.meetings.read().map(|m| m.len()).unwrap_or(0)
    }

    /// Get the count of decisions in the registry.
    pub fn decision_count(&self) -> usize {
        self.decisions.read().map(|d| d.len()).unwrap_or(0)
    }

    /// Clear all data (useful for testing).
    #[cfg(test)]
    pub fn clear(&self) {
        if let Ok(mut meetings) = self.meetings.write() {
            meetings.clear();
        }
        if let Ok(mut decisions) = self.decisions.write() {
            decisions.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::models::DecisionStatus;

    fn sample_meeting(id: &str, coop_id: &str) -> Meeting {
        Meeting {
            meeting_id: id.to_string(),
            coop_id: coop_id.to_string(),
            title: format!("Meeting {}", id),
            starts_at: 1700000000,
            notes: None,
            created_at: 1699999900,
        }
    }

    fn sample_decision(receipt_id: &str, coop_id: &str) -> DecisionIndexEntry {
        DecisionIndexEntry {
            decision_receipt_id: receipt_id.to_string(),
            decision_hash: format!("hash-{}", receipt_id),
            coop_id: coop_id.to_string(),
            meeting_id: None,
            title: format!("Decision {}", receipt_id),
            tags: vec![],
            status: DecisionStatus::Pending,
            created_at: 1700000000,
            proposal_type: "text".to_string(),
            treasury_effect_id: None,
        }
    }

    #[test]
    fn test_registry_add_and_get_meeting() {
        let registry = DecisionRegistry::new();
        let meeting = sample_meeting("mtg-001", "coop-alpha");

        registry.add_meeting(meeting.clone()).unwrap();

        let retrieved = registry.get_meeting("mtg-001").unwrap();
        assert_eq!(retrieved.meeting_id, "mtg-001");
        assert_eq!(retrieved.coop_id, "coop-alpha");
    }

    #[test]
    fn test_registry_duplicate_meeting_error() {
        let registry = DecisionRegistry::new();
        let meeting = sample_meeting("mtg-001", "coop-alpha");

        registry.add_meeting(meeting.clone()).unwrap();
        let result = registry.add_meeting(meeting);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_registry_list_meetings_by_coop() {
        let registry = DecisionRegistry::new();

        registry
            .add_meeting(sample_meeting("mtg-001", "coop-alpha"))
            .unwrap();
        registry
            .add_meeting(sample_meeting("mtg-002", "coop-alpha"))
            .unwrap();
        registry
            .add_meeting(sample_meeting("mtg-003", "coop-beta"))
            .unwrap();

        let alpha_meetings = registry.list_meetings("coop-alpha");
        assert_eq!(alpha_meetings.len(), 2);

        let beta_meetings = registry.list_meetings("coop-beta");
        assert_eq!(beta_meetings.len(), 1);

        let gamma_meetings = registry.list_meetings("coop-gamma");
        assert!(gamma_meetings.is_empty());
    }

    #[test]
    fn test_registry_index_and_get_decision() {
        let registry = DecisionRegistry::new();
        let entry = sample_decision("receipt-001", "coop-alpha");

        registry.index_decision(entry).unwrap();

        let retrieved = registry.get_decision("receipt-001").unwrap();
        assert_eq!(retrieved.decision_receipt_id, "receipt-001");
        assert_eq!(retrieved.coop_id, "coop-alpha");
    }

    #[test]
    fn test_registry_index_decision_upsert() {
        let registry = DecisionRegistry::new();

        let entry1 = DecisionIndexEntry {
            decision_receipt_id: "receipt-001".to_string(),
            decision_hash: "hash-v1".to_string(),
            coop_id: "coop-alpha".to_string(),
            meeting_id: None,
            title: "Original".to_string(),
            tags: vec![],
            status: DecisionStatus::Pending,
            created_at: 1700000000,
            proposal_type: "text".to_string(),
            treasury_effect_id: None,
        };

        let entry2 = DecisionIndexEntry {
            decision_receipt_id: "receipt-001".to_string(),
            decision_hash: "hash-v1".to_string(),
            coop_id: "coop-alpha".to_string(),
            meeting_id: None,
            title: "Updated".to_string(),
            tags: vec!["updated".to_string()],
            status: DecisionStatus::Approved,
            created_at: 1700000000,
            proposal_type: "text".to_string(),
            treasury_effect_id: None,
        };

        registry.index_decision(entry1).unwrap();
        registry.index_decision(entry2).unwrap();

        let retrieved = registry.get_decision("receipt-001").unwrap();
        assert_eq!(retrieved.title, "Updated");
        assert_eq!(retrieved.status, DecisionStatus::Approved);
        assert_eq!(registry.decision_count(), 1);
    }

    #[test]
    fn test_registry_list_decisions_with_filter() {
        let registry = DecisionRegistry::new();

        // Add decisions with different coops and tags
        let mut entry1 = sample_decision("receipt-001", "coop-alpha");
        entry1.tags = vec!["budget".to_string()];
        entry1.created_at = 1700000000;

        let mut entry2 = sample_decision("receipt-002", "coop-alpha");
        entry2.tags = vec!["membership".to_string()];
        entry2.created_at = 1700001000;

        let mut entry3 = sample_decision("receipt-003", "coop-beta");
        entry3.tags = vec!["budget".to_string()];
        entry3.created_at = 1700002000;

        registry.index_decision(entry1).unwrap();
        registry.index_decision(entry2).unwrap();
        registry.index_decision(entry3).unwrap();

        // Filter by coop
        let alpha = registry.list_decisions(DecisionFilter::new().with_coop_id("coop-alpha"));
        assert_eq!(alpha.len(), 2);

        // Filter by tag
        let budget = registry.list_decisions(DecisionFilter::new().with_tag("budget"));
        assert_eq!(budget.len(), 2);

        // Filter by coop + tag
        let alpha_budget = registry.list_decisions(
            DecisionFilter::new()
                .with_coop_id("coop-alpha")
                .with_tag("budget"),
        );
        assert_eq!(alpha_budget.len(), 1);
        assert_eq!(alpha_budget[0].decision_receipt_id, "receipt-001");

        // Filter by time range
        let recent =
            registry.list_decisions(DecisionFilter::new().after(1700000500).before(1700001500));
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].decision_receipt_id, "receipt-002");
    }

    #[test]
    fn test_registry_counts() {
        let registry = DecisionRegistry::new();

        assert_eq!(registry.meeting_count(), 0);
        assert_eq!(registry.decision_count(), 0);

        registry
            .add_meeting(sample_meeting("mtg-001", "coop-alpha"))
            .unwrap();
        registry
            .index_decision(sample_decision("receipt-001", "coop-alpha"))
            .unwrap();
        registry
            .index_decision(sample_decision("receipt-002", "coop-alpha"))
            .unwrap();

        assert_eq!(registry.meeting_count(), 1);
        assert_eq!(registry.decision_count(), 2);
    }

    #[test]
    fn test_registry_get_nonexistent() {
        let registry = DecisionRegistry::new();

        assert!(registry.get_meeting("nonexistent").is_none());
        assert!(registry.get_decision("nonexistent").is_none());
    }

    #[test]
    fn test_registry_decision_with_meeting_association() {
        let registry = DecisionRegistry::new();

        registry
            .add_meeting(sample_meeting("mtg-001", "coop-alpha"))
            .unwrap();

        let mut entry = sample_decision("receipt-001", "coop-alpha");
        entry.meeting_id = Some("mtg-001".to_string());

        registry.index_decision(entry).unwrap();

        let decisions = registry.list_decisions(DecisionFilter::new().with_meeting_id("mtg-001"));
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].decision_receipt_id, "receipt-001");
    }

    #[test]
    fn test_registry_clear() {
        let registry = DecisionRegistry::new();

        registry
            .add_meeting(sample_meeting("mtg-001", "coop-alpha"))
            .unwrap();
        registry
            .index_decision(sample_decision("receipt-001", "coop-alpha"))
            .unwrap();

        assert_eq!(registry.meeting_count(), 1);
        assert_eq!(registry.decision_count(), 1);

        registry.clear();

        assert_eq!(registry.meeting_count(), 0);
        assert_eq!(registry.decision_count(), 0);
    }
}
