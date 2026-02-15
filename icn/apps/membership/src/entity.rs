//! Unified Entity Model
//!
//! This module consolidates the entity models from icn-entity, icn-coop,
//! and icn-community into a single unified representation.

use serde::{Deserialize, Serialize};

/// Re-export EntityId and related types from entity_core
pub use crate::entity_core::entity::{
    AccountId, AccountReference, CooperativeEntity, EntityId, EntityStatus, EntityType,
};

/// Unified entity configuration that works for all entity types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityConfig {
    /// Unique entity identifier
    pub id: EntityId,

    /// Entity name
    pub name: String,

    /// Entity type
    pub entity_type: EntityType,

    /// Entity status
    pub status: EntityStatus,

    /// Optional description
    pub description: Option<String>,

    /// Membership configuration
    pub membership_config: MembershipConfig,

    /// Custom metadata
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

/// Membership configuration for an entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembershipConfig {
    /// Minimum trust threshold for membership (0.0 - 1.0)
    pub min_trust_threshold: f64,

    /// Membership classes with CCL-based criteria
    #[serde(default)]
    pub classes: Vec<MembershipClass>,

    /// Default role for new members
    pub default_role: String,

    /// Whether membership requires approval
    pub requires_approval: bool,
}

impl Default for MembershipConfig {
    fn default() -> Self {
        Self {
            min_trust_threshold: 0.3,
            classes: Vec::new(),
            default_role: "member".to_string(),
            requires_approval: true,
        }
    }
}

/// A membership class with CCL-based criteria
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembershipClass {
    /// Class name (e.g., "resident", "worker", "consumer")
    pub name: String,

    /// CCL criteria for membership in this class
    pub criteria: MembershipCriteria,

    /// Voting weight multiplier for this class
    #[serde(default = "default_voting_weight")]
    pub voting_weight: u32,
}

fn default_voting_weight() -> u32 {
    1
}

/// CCL-based membership criteria
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembershipCriteria {
    /// All conditions must be true
    #[serde(default)]
    pub all: Vec<Condition>,

    /// At least one condition must be true
    #[serde(default)]
    pub any: Vec<Condition>,
}

/// A single condition in membership criteria
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    /// Field to check
    pub field: String,

    /// Operator
    pub op: String,

    /// Value to compare against
    pub value: serde_json::Value,
}

impl EntityConfig {
    /// Create a new entity configuration
    pub fn new(id: EntityId, name: String, entity_type: EntityType) -> Self {
        Self {
            id,
            name,
            entity_type,
            status: EntityStatus::Active,
            description: None,
            membership_config: MembershipConfig::default(),
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Create a cooperative entity configuration
    pub fn cooperative(id: impl Into<String>, name: String) -> anyhow::Result<Self> {
        let id_str: String = id.into();
        let entity_id = EntityId::cooperative(&id_str)?;
        Ok(Self::new(entity_id, name, EntityType::Cooperative))
    }

    /// Create a community entity configuration
    pub fn community(id: impl Into<String>, name: String) -> anyhow::Result<Self> {
        let id_str: String = id.into();
        let entity_id = EntityId::community(&id_str)?;
        Ok(Self::new(entity_id, name, EntityType::Community))
    }

    /// Create a federation entity configuration
    pub fn federation(id: impl Into<String>, name: String) -> anyhow::Result<Self> {
        let id_str: String = id.into();
        let entity_id = EntityId::federation(&id_str)?;
        Ok(Self::new(entity_id, name, EntityType::Federation))
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set membership config
    pub fn with_membership_config(mut self, config: MembershipConfig) -> Self {
        self.membership_config = config;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_cooperative_config() {
        let config =
            EntityConfig::cooperative("food-coop", "Food Cooperative".to_string()).unwrap();
        assert_eq!(config.name, "Food Cooperative");
        assert!(matches!(config.entity_type, EntityType::Cooperative));
        assert_eq!(config.membership_config.min_trust_threshold, 0.3);
    }

    #[test]
    fn test_create_community_config() {
        let config =
            EntityConfig::community("rochester-civic", "Rochester Civic".to_string()).unwrap();
        assert_eq!(config.name, "Rochester Civic");
        assert!(matches!(config.entity_type, EntityType::Community));
    }

    #[test]
    fn test_membership_class() {
        let class = MembershipClass {
            name: "resident".to_string(),
            criteria: MembershipCriteria {
                all: vec![Condition {
                    field: "verified_address".to_string(),
                    op: "==".to_string(),
                    value: serde_json::json!(true),
                }],
                any: vec![],
            },
            voting_weight: 1,
        };

        assert_eq!(class.name, "resident");
        assert_eq!(class.voting_weight, 1);
    }
}
