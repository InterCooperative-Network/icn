//! Parent-aware attachment for operational objects.
//!
//! Operational objects (action items, meetings, documents) can attach to any of
//! the three institutional layers. This enum makes the attachment polymorphic
//! without the objects needing to know about entity, structure, or activity
//! types directly.
//!
//! See `docs/design/institutional-structure-spec.md` for the full design.

use crate::activity::ActivityId;
use crate::structure::StructureId;
use serde::{Deserialize, Serialize};

/// Parent of an operational object: either a sovereign entity, an internal
/// structure, or a time-bounded activity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InstitutionalParent {
    /// A sovereign entity (cooperative, community, federation). Carries the
    /// entity's ID as a string to avoid a cross-crate dependency on `icn-entity`.
    Entity { id: String },

    /// An internal structure (committee, working group, team, office).
    Structure { id: StructureId },

    /// A time-bounded activity (event, program, project, initiative).
    Activity { id: ActivityId },
}

impl InstitutionalParent {
    /// Shorthand constructor for entity parent.
    pub fn entity(id: impl Into<String>) -> Self {
        Self::Entity { id: id.into() }
    }

    /// Shorthand constructor for structure parent.
    pub fn structure(id: StructureId) -> Self {
        Self::Structure { id }
    }

    /// Shorthand constructor for activity parent.
    pub fn activity(id: ActivityId) -> Self {
        Self::Activity { id }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_parent_serde() {
        let p = InstitutionalParent::entity("nycn");
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"type\":\"entity\""));
        assert!(json.contains("\"id\":\"nycn\""));
        let round: InstitutionalParent = serde_json::from_str(&json).unwrap();
        assert_eq!(p, round);
    }

    #[test]
    fn test_structure_parent_serde() {
        let p = InstitutionalParent::structure(StructureId::from_raw("nycn-finance"));
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"type\":\"structure\""));
        let round: InstitutionalParent = serde_json::from_str(&json).unwrap();
        assert_eq!(p, round);
    }

    #[test]
    fn test_activity_parent_serde() {
        let p = InstitutionalParent::activity(ActivityId::from_raw("summit-2026"));
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"type\":\"activity\""));
        let round: InstitutionalParent = serde_json::from_str(&json).unwrap();
        assert_eq!(p, round);
    }

    #[test]
    fn test_variants_distinct() {
        let a = InstitutionalParent::entity("x");
        let b = InstitutionalParent::structure(StructureId::from_raw("x"));
        let c = InstitutionalParent::activity(ActivityId::from_raw("x"));
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_ne!(a, c);
    }
}
