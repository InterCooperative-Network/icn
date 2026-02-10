//! Decision Registry data models.
//!
//! These are APP-LAYER types for indexing governance decisions and meetings.
//! They provide a "Google Drive replacement" for co-op meetings/decisions,
//! indexing over canonical receipts without touching kernel primitives.

use serde::{Deserialize, Serialize};

/// Status of a governance decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionStatus {
    /// Decision is pending (voting in progress).
    Pending,
    /// Decision was approved.
    Approved,
    /// Decision was rejected.
    Rejected,
}

impl Default for DecisionStatus {
    fn default() -> Self {
        Self::Pending
    }
}

/// A cooperative meeting record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meeting {
    /// Unique meeting identifier, e.g., "mtg-2026-02-10-summit".
    pub meeting_id: String,
    /// Cooperative DID that owns this meeting.
    pub coop_id: String,
    /// Human-readable title, e.g., "Summit Planning 2026".
    pub title: String,
    /// Meeting start time as Unix timestamp.
    pub starts_at: u64,
    /// Optional meeting notes.
    pub notes: Option<String>,
    /// Creation timestamp.
    pub created_at: u64,
}

/// An index entry for a governance decision.
///
/// This is an APP-LAYER INDEX over canonical receipts. The `decision_receipt_id`
/// and `decision_hash` link back to the immutable canonical receipt, while the
/// other fields provide searchable metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionIndexEntry {
    /// Primary key, links to canonical receipt.
    pub decision_receipt_id: String,
    /// Canonical hash (immutable anchor).
    pub decision_hash: String,
    /// Cooperative DID.
    pub coop_id: String,
    /// Optional meeting association.
    pub meeting_id: Option<String>,
    /// Human-readable summary.
    pub title: String,
    /// Searchable tags, e.g., ["budget", "approved"].
    pub tags: Vec<String>,
    /// Current status of the decision.
    pub status: DecisionStatus,
    /// Creation timestamp.
    pub created_at: u64,
    /// Type of proposal, e.g., "text", "treasury_spend".
    pub proposal_type: String,
    /// If this decision triggered a treasury action, the effect ID.
    pub treasury_effect_id: Option<String>,
}

/// Filter criteria for querying decisions.
#[derive(Debug, Clone, Default)]
pub struct DecisionFilter {
    /// Filter by cooperative DID.
    pub coop_id: Option<String>,
    /// Filter by meeting ID.
    pub meeting_id: Option<String>,
    /// Filter by tag (any match).
    pub tag: Option<String>,
    /// Filter decisions created after this timestamp.
    pub after: Option<u64>,
    /// Filter decisions created before this timestamp.
    pub before: Option<u64>,
    /// Filter by proposal type.
    pub proposal_type: Option<String>,
}

impl DecisionFilter {
    /// Create an empty filter (matches all).
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by cooperative ID.
    pub fn with_coop_id(mut self, coop_id: impl Into<String>) -> Self {
        self.coop_id = Some(coop_id.into());
        self
    }

    /// Filter by meeting ID.
    pub fn with_meeting_id(mut self, meeting_id: impl Into<String>) -> Self {
        self.meeting_id = Some(meeting_id.into());
        self
    }

    /// Filter by tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    /// Filter by creation time range (after).
    pub fn after(mut self, timestamp: u64) -> Self {
        self.after = Some(timestamp);
        self
    }

    /// Filter by creation time range (before).
    pub fn before(mut self, timestamp: u64) -> Self {
        self.before = Some(timestamp);
        self
    }

    /// Filter by proposal type.
    pub fn with_proposal_type(mut self, proposal_type: impl Into<String>) -> Self {
        self.proposal_type = Some(proposal_type.into());
        self
    }

    /// Check if a decision entry matches this filter.
    pub fn matches(&self, entry: &DecisionIndexEntry) -> bool {
        if let Some(ref coop_id) = self.coop_id {
            if &entry.coop_id != coop_id {
                return false;
            }
        }
        if let Some(ref meeting_id) = self.meeting_id {
            if entry.meeting_id.as_ref() != Some(meeting_id) {
                return false;
            }
        }
        if let Some(ref tag) = self.tag {
            if !entry.tags.contains(tag) {
                return false;
            }
        }
        if let Some(after) = self.after {
            if entry.created_at < after {
                return false;
            }
        }
        if let Some(before) = self.before {
            if entry.created_at > before {
                return false;
            }
        }
        if let Some(ref proposal_type) = self.proposal_type {
            if &entry.proposal_type != proposal_type {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decision_status_default() {
        assert_eq!(DecisionStatus::default(), DecisionStatus::Pending);
    }

    #[test]
    fn test_decision_filter_matches() {
        let entry = DecisionIndexEntry {
            decision_receipt_id: "receipt-001".to_string(),
            decision_hash: "abc123".to_string(),
            coop_id: "coop-alpha".to_string(),
            meeting_id: Some("mtg-2026-01".to_string()),
            title: "Budget approval".to_string(),
            tags: vec!["budget".to_string(), "approved".to_string()],
            status: DecisionStatus::Approved,
            created_at: 1700000000,
            proposal_type: "treasury_spend".to_string(),
            treasury_effect_id: Some("effect-001".to_string()),
        };

        // Empty filter matches all
        assert!(DecisionFilter::new().matches(&entry));

        // Coop filter
        assert!(DecisionFilter::new()
            .with_coop_id("coop-alpha")
            .matches(&entry));
        assert!(!DecisionFilter::new()
            .with_coop_id("coop-beta")
            .matches(&entry));

        // Meeting filter
        assert!(DecisionFilter::new()
            .with_meeting_id("mtg-2026-01")
            .matches(&entry));
        assert!(!DecisionFilter::new()
            .with_meeting_id("mtg-2026-02")
            .matches(&entry));

        // Tag filter
        assert!(DecisionFilter::new().with_tag("budget").matches(&entry));
        assert!(!DecisionFilter::new().with_tag("urgent").matches(&entry));

        // Time range filter
        assert!(DecisionFilter::new().after(1699999999).matches(&entry));
        assert!(!DecisionFilter::new().after(1700000001).matches(&entry));
        assert!(DecisionFilter::new().before(1700000001).matches(&entry));
        assert!(!DecisionFilter::new().before(1699999999).matches(&entry));

        // Proposal type filter
        assert!(DecisionFilter::new()
            .with_proposal_type("treasury_spend")
            .matches(&entry));
        assert!(!DecisionFilter::new()
            .with_proposal_type("text")
            .matches(&entry));

        // Combined filters
        assert!(DecisionFilter::new()
            .with_coop_id("coop-alpha")
            .with_tag("budget")
            .after(1699999999)
            .matches(&entry));
    }

    #[test]
    fn test_meeting_serialization() {
        let meeting = Meeting {
            meeting_id: "mtg-2026-02-10-summit".to_string(),
            coop_id: "did:icn:abc123".to_string(),
            title: "Summit Planning 2026".to_string(),
            starts_at: 1800000000,
            notes: Some("Discuss budget allocation".to_string()),
            created_at: 1799999900,
        };

        let json = serde_json::to_string(&meeting).unwrap();
        let deserialized: Meeting = serde_json::from_str(&json).unwrap();

        assert_eq!(meeting.meeting_id, deserialized.meeting_id);
        assert_eq!(meeting.title, deserialized.title);
        assert_eq!(meeting.notes, deserialized.notes);
    }

    #[test]
    fn test_decision_entry_serialization() {
        let entry = DecisionIndexEntry {
            decision_receipt_id: "receipt-001".to_string(),
            decision_hash: "abc123def456".to_string(),
            coop_id: "did:icn:coop123".to_string(),
            meeting_id: None,
            title: "New member application".to_string(),
            tags: vec!["membership".to_string()],
            status: DecisionStatus::Pending,
            created_at: 1700000000,
            proposal_type: "text".to_string(),
            treasury_effect_id: None,
        };

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: DecisionIndexEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(entry.decision_receipt_id, deserialized.decision_receipt_id);
        assert_eq!(entry.status, deserialized.status);
        assert_eq!(entry.tags, deserialized.tags);
    }
}
