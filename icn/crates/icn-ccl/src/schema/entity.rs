//! Entity schema definitions.
//!
//! Defines cooperatives, communities, federations, and their membership rules.

use super::SchemaError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Entity type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    /// Economic organization
    Cooperative,
    /// Civic organization
    Community,
    /// Meta-organization containing other entities
    Federation,
}

/// Entity subtype for more specific classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntitySubtype {
    // Cooperative subtypes
    Worker,
    Consumer,
    Producer,
    MultiStakeholder,
    // Community subtypes
    Municipal,
    Regional,
    Sectoral,
    Purpose,
    // Federation subtypes
    Trade,
    Geographic,
    Sectoral2, // Different from community sectoral
    // Custom
    Custom(String),
}

/// Jurisdiction type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JurisdictionType {
    /// Physical geographic bounds
    Geographic,
    /// Network/DID namespace
    Network,
    /// Combination
    Hybrid,
}

/// Jurisdiction definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Jurisdiction {
    /// Type of jurisdiction
    #[serde(rename = "type")]
    pub jurisdiction_type: JurisdictionType,

    /// Bounds description (address, region, or DID namespace)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<String>,
}

/// Types that can be members.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberType {
    Individual,
    Cooperative,
    Community,
    Federation,
}

/// A membership class (e.g., "consumer_owner", "worker_owner").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembershipClass {
    /// Class name
    pub name: String,

    /// Whether this is the default class for new members
    #[serde(default)]
    pub default: bool,

    /// Criteria for membership in this class
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub criteria: Option<Criteria>,
}

/// Membership criteria (conditions that must be met).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Criteria {
    /// All conditions must be true (AND)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub all: Vec<Condition>,

    /// Any condition must be true (OR)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub any: Vec<Condition>,

    /// Reference another class's criteria
    #[serde(default, rename = "ref", skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

/// A single condition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    /// Field to check (if not a reference)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,

    /// Comparison operator
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub op: Option<ComparisonOp>,

    /// Value to compare against
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_yaml::Value>,

    /// Reference to another class (inherits its criteria)
    #[serde(default, rename = "ref", skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

/// Comparison operators for conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComparisonOp {
    #[serde(rename = "==")]
    Eq,
    #[serde(rename = "!=")]
    Ne,
    #[serde(rename = "<")]
    Lt,
    #[serde(rename = "<=")]
    Le,
    #[serde(rename = ">")]
    Gt,
    #[serde(rename = ">=")]
    Ge,
}

/// Rights that can be granted to membership classes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Right {
    /// Vote in decisions
    Vote,
    /// Receive patronage refunds
    PatronageRefund,
    /// Run for elected positions
    RunForBoard,
    /// Participate in workplace governance
    WorkplaceGovernance,
    /// Propose items for agenda
    Propose,
    /// Access member resources
    AccessResources,
    /// Custom right
    Custom(String),
}

/// Membership configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MembershipConfig {
    /// Types that can be members
    #[serde(default)]
    pub open_to: Vec<MemberType>,

    /// Membership classes
    #[serde(default)]
    pub classes: Vec<MembershipClass>,

    /// Rights by class
    #[serde(default)]
    pub rights_by_class: HashMap<String, Vec<Right>>,
}

/// Complete entity schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySchema {
    /// Entity name
    pub name: String,

    /// Entity type
    #[serde(rename = "type")]
    pub entity_type: EntityType,

    /// Entity subtype
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtype: Option<EntitySubtype>,

    /// Jurisdiction
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jurisdiction: Option<Jurisdiction>,

    /// Membership configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub membership: Option<MembershipConfig>,
}

impl EntitySchema {
    /// Validate the entity schema.
    pub fn validate(&self) -> Result<(), SchemaError> {
        if self.name.is_empty() {
            return Err(SchemaError::Validation(
                "Entity name cannot be empty".to_string(),
            ));
        }

        // Validate membership class references
        if let Some(membership) = &self.membership {
            let class_names: Vec<_> = membership.classes.iter().map(|c| &c.name).collect();

            // Check that rights_by_class references valid classes
            for class_name in membership.rights_by_class.keys() {
                if !class_names.contains(&class_name) {
                    return Err(SchemaError::Reference(format!(
                        "rights_by_class references unknown class: {}",
                        class_name
                    )));
                }
            }

            // Check that criteria refs reference valid classes
            for class in &membership.classes {
                if let Some(criteria) = &class.criteria {
                    if let Some(ref_name) = &criteria.reference {
                        if !class_names.contains(&ref_name) {
                            return Err(SchemaError::Reference(format!(
                                "Class {} references unknown class: {}",
                                class.name, ref_name
                            )));
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_entity() {
        let yaml = r#"
name: "Abundance Food Coop"
type: cooperative
subtype: consumer
jurisdiction:
  type: geographic
  bounds: "Rochester, NY"
membership:
  open_to: [individual]
  classes:
    - name: consumer_owner
      default: true
      criteria:
        all:
          - field: equity_paid
            op: ">="
            value: 100
          - field: signed_bylaws
            op: "=="
            value: true
  rights_by_class:
    consumer_owner: [vote, patronage_refund, run_for_board]
"#;
        let entity: EntitySchema = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert_eq!(entity.name, "Abundance Food Coop");
        assert_eq!(entity.entity_type, EntityType::Cooperative);
        entity.validate().expect("Validation failed");
    }

    #[test]
    fn test_parse_community() {
        let yaml = r#"
name: "Rochester Civic Assembly"
type: community
subtype: municipal
jurisdiction:
  type: geographic
  bounds: "Rochester, NY"
membership:
  open_to: [individual, cooperative]
  classes:
    - name: resident
      default: true
    - name: organization
      criteria:
        all:
          - field: registered
            op: "=="
            value: true
"#;
        let entity: EntitySchema = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert_eq!(entity.entity_type, EntityType::Community);
        entity.validate().expect("Validation failed");
    }

    #[test]
    fn test_validate_missing_class_ref() {
        let yaml = r#"
name: "Test Entity"
type: cooperative
membership:
  classes:
    - name: member
  rights_by_class:
    nonexistent_class: [vote]
"#;
        let entity: EntitySchema = serde_yaml::from_str(yaml).expect("Failed to parse");
        assert!(entity.validate().is_err());
    }
}
