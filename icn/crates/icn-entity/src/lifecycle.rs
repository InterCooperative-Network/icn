//! Entity lifecycle management
//!
//! This module defines the lifecycle events and state transitions for entities.
//! Entities go through various states from formation to dissolution.

use crate::entity::{CooperativeEntity, EntityId, EntityStatus};
use crate::error::{EntityError, Result};
use serde::{Deserialize, Serialize};

// ============================================================================
// LifecycleEvent
// ============================================================================

/// Events that occur during an entity's lifecycle
///
/// These events are recorded for audit trails and can trigger
/// notifications or governance actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LifecycleEvent {
    /// Entity was created
    Created {
        entity_id: EntityId,
        created_by: EntityId,
        timestamp: u64,
    },

    /// Entity was activated (charter ratified, ready for operation)
    Activated {
        entity_id: EntityId,
        charter_hash: String,
        activated_by: EntityId,
        timestamp: u64,
    },

    /// Entity was suspended
    Suspended {
        entity_id: EntityId,
        reason: String,
        suspended_by: EntityId,
        timestamp: u64,
    },

    /// Entity was resumed from suspension
    Resumed {
        entity_id: EntityId,
        resumed_by: EntityId,
        timestamp: u64,
    },

    /// Entity dissolution process started
    DissolutionStarted {
        entity_id: EntityId,
        initiator: EntityId,
        reason: String,
        timestamp: u64,
    },

    /// Entity was fully dissolved
    Dissolved { entity_id: EntityId, timestamp: u64 },

    /// Entity merged into another
    Merged {
        source_id: EntityId,
        target_id: EntityId,
        merger_proposal_id: Option<String>,
        timestamp: u64,
    },

    /// Entity split into multiple entities
    Split {
        source_id: EntityId,
        new_entity_ids: Vec<EntityId>,
        split_proposal_id: Option<String>,
        timestamp: u64,
    },

    /// Entity metadata was updated
    Updated {
        entity_id: EntityId,
        updated_by: EntityId,
        changes: Vec<String>, // List of fields that changed
        timestamp: u64,
    },
}

impl LifecycleEvent {
    /// Get the entity ID this event relates to
    pub fn entity_id(&self) -> &EntityId {
        match self {
            LifecycleEvent::Created { entity_id, .. } => entity_id,
            LifecycleEvent::Activated { entity_id, .. } => entity_id,
            LifecycleEvent::Suspended { entity_id, .. } => entity_id,
            LifecycleEvent::Resumed { entity_id, .. } => entity_id,
            LifecycleEvent::DissolutionStarted { entity_id, .. } => entity_id,
            LifecycleEvent::Dissolved { entity_id, .. } => entity_id,
            LifecycleEvent::Merged { source_id, .. } => source_id,
            LifecycleEvent::Split { source_id, .. } => source_id,
            LifecycleEvent::Updated { entity_id, .. } => entity_id,
        }
    }

    /// Get the timestamp of this event
    pub fn timestamp(&self) -> u64 {
        match self {
            LifecycleEvent::Created { timestamp, .. } => *timestamp,
            LifecycleEvent::Activated { timestamp, .. } => *timestamp,
            LifecycleEvent::Suspended { timestamp, .. } => *timestamp,
            LifecycleEvent::Resumed { timestamp, .. } => *timestamp,
            LifecycleEvent::DissolutionStarted { timestamp, .. } => *timestamp,
            LifecycleEvent::Dissolved { timestamp, .. } => *timestamp,
            LifecycleEvent::Merged { timestamp, .. } => *timestamp,
            LifecycleEvent::Split { timestamp, .. } => *timestamp,
            LifecycleEvent::Updated { timestamp, .. } => *timestamp,
        }
    }

    /// Get a human-readable description of this event
    pub fn description(&self) -> String {
        match self {
            LifecycleEvent::Created { entity_id, .. } => {
                format!("Entity {} was created", entity_id.identifier())
            }
            LifecycleEvent::Activated { entity_id, .. } => {
                format!("Entity {} was activated", entity_id.identifier())
            }
            LifecycleEvent::Suspended {
                entity_id, reason, ..
            } => {
                format!(
                    "Entity {} was suspended: {}",
                    entity_id.identifier(),
                    reason
                )
            }
            LifecycleEvent::Resumed { entity_id, .. } => {
                format!("Entity {} was resumed", entity_id.identifier())
            }
            LifecycleEvent::DissolutionStarted {
                entity_id, reason, ..
            } => {
                format!(
                    "Entity {} dissolution started: {}",
                    entity_id.identifier(),
                    reason
                )
            }
            LifecycleEvent::Dissolved { entity_id, .. } => {
                format!("Entity {} was dissolved", entity_id.identifier())
            }
            LifecycleEvent::Merged {
                source_id,
                target_id,
                ..
            } => {
                format!(
                    "Entity {} merged into {}",
                    source_id.identifier(),
                    target_id.identifier()
                )
            }
            LifecycleEvent::Split {
                source_id,
                new_entity_ids,
                ..
            } => {
                let new_ids: Vec<_> = new_entity_ids.iter().map(|e| e.identifier()).collect();
                format!("Entity {} split into {:?}", source_id.identifier(), new_ids)
            }
            LifecycleEvent::Updated {
                entity_id, changes, ..
            } => {
                format!("Entity {} updated: {:?}", entity_id.identifier(), changes)
            }
        }
    }
}

// ============================================================================
// EntityLifecycle Trait
// ============================================================================

/// Trait for entity lifecycle management
///
/// Implementations of this trait handle state transitions and
/// generate appropriate lifecycle events.
pub trait EntityLifecycle {
    /// Create a new entity
    ///
    /// The entity starts in `Forming` status.
    fn create(
        &mut self,
        entity: CooperativeEntity,
        created_by: &EntityId,
    ) -> Result<LifecycleEvent>;

    /// Activate an entity after charter ratification
    ///
    /// Transitions from `Forming` to `Active`.
    fn activate(
        &mut self,
        entity_id: &EntityId,
        charter_hash: String,
        activated_by: &EntityId,
    ) -> Result<LifecycleEvent>;

    /// Suspend an entity
    ///
    /// Transitions from `Active` to `Suspended`.
    fn suspend(
        &mut self,
        entity_id: &EntityId,
        reason: String,
        suspended_by: &EntityId,
    ) -> Result<LifecycleEvent>;

    /// Resume a suspended entity
    ///
    /// Transitions from `Suspended` to `Active`.
    fn resume(&mut self, entity_id: &EntityId, resumed_by: &EntityId) -> Result<LifecycleEvent>;

    /// Start entity dissolution
    ///
    /// Transitions to `Dissolving`.
    fn start_dissolution(
        &mut self,
        entity_id: &EntityId,
        initiator: &EntityId,
        reason: String,
    ) -> Result<LifecycleEvent>;

    /// Complete entity dissolution
    ///
    /// Transitions from `Dissolving` to `Dissolved`.
    fn complete_dissolution(&mut self, entity_id: &EntityId) -> Result<LifecycleEvent>;

    /// Merge source entity into target entity
    ///
    /// The source entity transitions to `Merged`.
    fn merge(
        &mut self,
        source_id: &EntityId,
        target_id: &EntityId,
        proposal_id: Option<String>,
    ) -> Result<LifecycleEvent>;

    /// Split an entity into multiple new entities
    ///
    /// The source entity transitions to `Split`.
    fn split(
        &mut self,
        source_id: &EntityId,
        new_entities: Vec<CooperativeEntity>,
        proposal_id: Option<String>,
    ) -> Result<Vec<LifecycleEvent>>;
}

// ============================================================================
// State Transition Validation
// ============================================================================

/// Validate a state transition
pub fn validate_transition(from: &EntityStatus, to: &EntityStatus) -> Result<()> {
    let valid = match (from, to) {
        // From Forming
        (EntityStatus::Forming, EntityStatus::Active) => true,
        (EntityStatus::Forming, EntityStatus::Dissolved { .. }) => true, // Abandoned

        // From Active
        (EntityStatus::Active, EntityStatus::Suspended { .. }) => true,
        (EntityStatus::Active, EntityStatus::Dissolving { .. }) => true,
        (EntityStatus::Active, EntityStatus::Merged { .. }) => true,
        (EntityStatus::Active, EntityStatus::Split { .. }) => true,

        // From Suspended
        (EntityStatus::Suspended { .. }, EntityStatus::Active) => true, // Resume
        (EntityStatus::Suspended { .. }, EntityStatus::Dissolving { .. }) => true,

        // From Dissolving
        (EntityStatus::Dissolving { .. }, EntityStatus::Dissolved { .. }) => true,
        (EntityStatus::Dissolving { .. }, EntityStatus::Active) => true, // Cancellation

        // All other transitions are invalid
        _ => false,
    };

    if valid {
        Ok(())
    } else {
        Err(EntityError::InvalidStateTransition {
            from: from.clone(),
            to: to.clone(),
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_transitions() {
        // Forming -> Active
        assert!(validate_transition(&EntityStatus::Forming, &EntityStatus::Active).is_ok());

        // Active -> Suspended
        assert!(validate_transition(
            &EntityStatus::Active,
            &EntityStatus::Suspended {
                reason: "test".into(),
                suspended_at: 0
            }
        )
        .is_ok());

        // Suspended -> Active (resume)
        assert!(validate_transition(
            &EntityStatus::Suspended {
                reason: "test".into(),
                suspended_at: 0
            },
            &EntityStatus::Active
        )
        .is_ok());

        // Active -> Dissolving
        assert!(validate_transition(
            &EntityStatus::Active,
            &EntityStatus::Dissolving { started_at: 0 }
        )
        .is_ok());

        // Dissolving -> Dissolved
        assert!(validate_transition(
            &EntityStatus::Dissolving { started_at: 0 },
            &EntityStatus::Dissolved { dissolved_at: 0 }
        )
        .is_ok());
    }

    #[test]
    fn test_invalid_transitions() {
        // Dissolved -> Active (can't resurrect)
        assert!(validate_transition(
            &EntityStatus::Dissolved { dissolved_at: 0 },
            &EntityStatus::Active
        )
        .is_err());

        // Forming -> Suspended (must activate first)
        assert!(validate_transition(
            &EntityStatus::Forming,
            &EntityStatus::Suspended {
                reason: "test".into(),
                suspended_at: 0
            }
        )
        .is_err());
    }

    #[test]
    fn test_lifecycle_event_serialization() {
        let event = LifecycleEvent::Created {
            entity_id: EntityId::cooperative("test").unwrap(),
            created_by: EntityId::cooperative("creator").unwrap(),
            timestamp: 1234567890,
        };

        let json = serde_json::to_string(&event).unwrap();
        let parsed: LifecycleEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(event.timestamp(), parsed.timestamp());
    }

    #[test]
    fn test_lifecycle_event_description() {
        let event = LifecycleEvent::Merged {
            source_id: EntityId::cooperative("source").unwrap(),
            target_id: EntityId::cooperative("target").unwrap(),
            merger_proposal_id: Some("prop-123".into()),
            timestamp: 0,
        };

        let desc = event.description();
        assert!(desc.contains("merged"));
        assert!(desc.contains("source"));
        assert!(desc.contains("target"));
    }

    #[test]
    fn test_all_valid_state_transitions() {
        // Test all valid transitions from the state machine

        // Forming -> Active (charter ratification)
        assert!(validate_transition(&EntityStatus::Forming, &EntityStatus::Active).is_ok());

        // Forming -> Dissolved (abandoned)
        assert!(validate_transition(
            &EntityStatus::Forming,
            &EntityStatus::Dissolved { dissolved_at: 0 }
        )
        .is_ok());

        // Active -> Suspended
        assert!(validate_transition(
            &EntityStatus::Active,
            &EntityStatus::Suspended {
                reason: "test".into(),
                suspended_at: 0
            }
        )
        .is_ok());

        // Active -> Dissolving
        assert!(validate_transition(
            &EntityStatus::Active,
            &EntityStatus::Dissolving { started_at: 0 }
        )
        .is_ok());

        // Active -> Merged
        assert!(validate_transition(
            &EntityStatus::Active,
            &EntityStatus::Merged {
                into: EntityId::cooperative("target").unwrap(),
                merged_at: 0
            }
        )
        .is_ok());

        // Active -> Split
        assert!(validate_transition(
            &EntityStatus::Active,
            &EntityStatus::Split {
                into: vec![
                    EntityId::cooperative("new1").unwrap(),
                    EntityId::cooperative("new2").unwrap()
                ],
                split_at: 0
            }
        )
        .is_ok());

        // Suspended -> Active (resume)
        assert!(validate_transition(
            &EntityStatus::Suspended {
                reason: "test".into(),
                suspended_at: 0
            },
            &EntityStatus::Active
        )
        .is_ok());

        // Suspended -> Dissolving
        assert!(validate_transition(
            &EntityStatus::Suspended {
                reason: "test".into(),
                suspended_at: 0
            },
            &EntityStatus::Dissolving { started_at: 0 }
        )
        .is_ok());

        // Dissolving -> Dissolved
        assert!(validate_transition(
            &EntityStatus::Dissolving { started_at: 0 },
            &EntityStatus::Dissolved { dissolved_at: 0 }
        )
        .is_ok());

        // Dissolving -> Active (cancellation)
        assert!(validate_transition(
            &EntityStatus::Dissolving { started_at: 0 },
            &EntityStatus::Active
        )
        .is_ok());
    }

    #[test]
    fn test_terminal_states_cannot_transition() {
        let terminal_states = [
            EntityStatus::Dissolved { dissolved_at: 0 },
            EntityStatus::Merged {
                into: EntityId::cooperative("target").unwrap(),
                merged_at: 0,
            },
            EntityStatus::Split {
                into: vec![EntityId::cooperative("new1").unwrap()],
                split_at: 0,
            },
        ];

        for terminal in &terminal_states {
            // Cannot transition to Active
            assert!(
                validate_transition(terminal, &EntityStatus::Active).is_err(),
                "Terminal state {terminal:?} should not transition to Active"
            );

            // Cannot transition to any other state
            assert!(
                validate_transition(
                    terminal,
                    &EntityStatus::Suspended {
                        reason: "test".into(),
                        suspended_at: 0
                    }
                )
                .is_err(),
                "Terminal state {terminal:?} should not transition to Suspended"
            );
        }
    }

    #[test]
    fn test_lifecycle_event_entity_id_extraction() {
        let coop_id = EntityId::cooperative("test-coop").unwrap();

        let events = vec![
            LifecycleEvent::Created {
                entity_id: coop_id.clone(),
                created_by: EntityId::cooperative("creator").unwrap(),
                timestamp: 0,
            },
            LifecycleEvent::Activated {
                entity_id: coop_id.clone(),
                charter_hash: "abc123".into(),
                activated_by: EntityId::cooperative("activator").unwrap(),
                timestamp: 0,
            },
            LifecycleEvent::Suspended {
                entity_id: coop_id.clone(),
                reason: "testing".into(),
                suspended_by: EntityId::cooperative("suspender").unwrap(),
                timestamp: 0,
            },
            LifecycleEvent::Resumed {
                entity_id: coop_id.clone(),
                resumed_by: EntityId::cooperative("resumer").unwrap(),
                timestamp: 0,
            },
            LifecycleEvent::DissolutionStarted {
                entity_id: coop_id.clone(),
                initiator: EntityId::cooperative("initiator").unwrap(),
                reason: "closing".into(),
                timestamp: 0,
            },
            LifecycleEvent::Dissolved {
                entity_id: coop_id.clone(),
                timestamp: 0,
            },
        ];

        for event in events {
            assert_eq!(
                event.entity_id(),
                &coop_id,
                "entity_id() should return correct ID for {event:?}"
            );
        }
    }
}
