//! CCL Schema Definitions
//!
//! Declarative YAML-based schemas for defining cooperative entities, governance
//! structures, economic rules, and federation agreements.
//!
//! # Design Principles
//!
//! 1. **Declarative over procedural**: Schemas define what should be true, not how to make it true
//! 2. **Deterministic expressions**: All computed values use a bounded expression language
//! 3. **Schema versioning**: Documents declare their schema version for migration support
//! 4. **Meaning firewall**: Schemas produce constraints that the kernel enforces mechanically
//!
//! # Schema Types
//!
//! - **Entity**: Defines cooperatives, communities, federations with membership rules
//! - **Governance**: Defines bodies, decision types, voting rules, delegation
//! - **Economics**: Defines capital structure, surplus allocation, credit policies
//! - **Agreement**: Defines federation agreements, boundary protocols, disputes
//!
//! # Expression Mini-Language
//!
//! Schemas can contain expressions for computed values:
//!
//! ```yaml
//! threshold: "0.67 * members"
//! credit_limit: "min(1000, patronage * 0.5 * trust_score)"
//! ```
//!
//! Expressions are deterministic (same input → same output) and bounded (no loops).

pub mod agreement;
pub mod economics;
pub mod entity;
pub mod expr;
pub mod governance;
pub mod version;

pub use agreement::*;
pub use economics::*;
pub use entity::*;
pub use expr::*;
pub use governance::*;
pub use version::*;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A complete CCL document that can contain multiple schema sections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CclDocument {
    /// Schema version for this document
    #[serde(default = "default_schema_version")]
    pub schema_version: SchemaVersion,

    /// Entity definition (if present)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<EntitySchema>,

    /// Governance definition (if present)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub governance: Option<GovernanceSchema>,

    /// Economics definition (if present)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub economics: Option<EconomicsSchema>,

    /// Agreement definition (if present)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agreement: Option<AgreementSchema>,

    /// Custom extensions
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, serde_yaml::Value>,
}

fn default_schema_version() -> SchemaVersion {
    SchemaVersion::V0
}

impl CclDocument {
    /// Parse a CCL document from YAML.
    pub fn from_yaml(yaml: &str) -> Result<Self, SchemaError> {
        serde_yaml::from_str(yaml).map_err(|e| SchemaError::Parse(e.to_string()))
    }

    /// Serialize to YAML.
    pub fn to_yaml(&self) -> Result<String, SchemaError> {
        serde_yaml::to_string(self).map_err(|e| SchemaError::Serialize(e.to_string()))
    }

    /// Validate the document structure and references.
    pub fn validate(&self) -> Result<(), SchemaError> {
        if let Some(entity) = &self.entity {
            entity.validate()?;
        }
        if let Some(governance) = &self.governance {
            governance.validate()?;
        }
        if let Some(economics) = &self.economics {
            economics.validate()?;
        }
        if let Some(agreement) = &self.agreement {
            agreement.validate()?;
        }
        Ok(())
    }
}

/// Errors that can occur during schema operations.
#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    /// YAML parsing error
    #[error("Parse error: {0}")]
    Parse(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialize(String),

    /// Validation error
    #[error("Validation error: {0}")]
    Validation(String),

    /// Expression error
    #[error("Expression error: {0}")]
    Expression(String),

    /// Reference error (unknown field, body, etc.)
    #[error("Reference error: {0}")]
    Reference(String),

    /// Version mismatch
    #[error("Version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: String, actual: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_document() {
        let yaml = "schema_version: v0\n";
        let doc = CclDocument::from_yaml(yaml).expect("Failed to parse");
        assert_eq!(doc.schema_version, SchemaVersion::V0);
        assert!(doc.entity.is_none());
        assert!(doc.governance.is_none());
    }

    #[test]
    fn test_document_roundtrip() {
        let doc = CclDocument {
            schema_version: SchemaVersion::V0,
            entity: None,
            governance: None,
            economics: None,
            agreement: None,
            extensions: HashMap::new(),
        };
        let yaml = doc.to_yaml().expect("Failed to serialize");
        let parsed = CclDocument::from_yaml(&yaml).expect("Failed to parse");
        assert_eq!(parsed.schema_version, doc.schema_version);
    }
}
