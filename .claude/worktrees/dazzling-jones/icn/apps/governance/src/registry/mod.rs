//! Decision Registry module.
//!
//! Provides a "Google Drive replacement" for cooperative meetings and decisions.
//! This is an APP-LAYER INDEX over canonical receipts, not new kernel primitives.
//!
//! ## Architecture
//!
//! The registry maintains two collections:
//! - **Meetings**: Records of cooperative meetings with metadata
//! - **Decisions**: Index entries linking to canonical decision receipts
//!
//! Decisions can optionally be associated with meetings, enabling queries like
//! "show all decisions from the February summit meeting."
//!
//! ## Usage
//!
//! ```rust,ignore
//! use icn_governance_actor::registry::{DecisionRegistry, Meeting, DecisionIndexEntry, DecisionFilter};
//!
//! let registry = DecisionRegistry::new();
//!
//! // Add a meeting
//! registry.add_meeting(Meeting {
//!     meeting_id: "mtg-2026-02-10".to_string(),
//!     coop_id: "did:icn:coop123".to_string(),
//!     title: "February Summit".to_string(),
//!     starts_at: 1800000000,
//!     notes: None,
//!     created_at: 1799999900,
//! })?;
//!
//! // Index a decision
//! registry.index_decision(DecisionIndexEntry {
//!     decision_receipt_id: "receipt-001".to_string(),
//!     decision_hash: "abc123".to_string(),
//!     coop_id: "did:icn:coop123".to_string(),
//!     meeting_id: Some("mtg-2026-02-10".to_string()),
//!     title: "Approve 2026 budget".to_string(),
//!     tags: vec!["budget".to_string(), "approved".to_string()],
//!     status: DecisionStatus::Approved,
//!     created_at: 1800000100,
//!     proposal_type: "treasury_spend".to_string(),
//!     treasury_effect_id: Some("effect-001".to_string()),
//! })?;
//!
//! // Query decisions
//! let budget_decisions = registry.list_decisions(
//!     DecisionFilter::new()
//!         .with_coop_id("did:icn:coop123")
//!         .with_tag("budget")
//! );
//! ```

pub mod models;
pub mod store;

pub use models::{DecisionFilter, DecisionIndexEntry, DecisionStatus, Meeting};
pub use store::DecisionRegistry;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_module_loads() {
        let registry = DecisionRegistry::new();
        assert_eq!(registry.meeting_count(), 0);
    }
}
