//! Schema versioning and migration support.
//!
//! CCL documents declare their schema version. When loading a document with
//! an older schema version, the system can migrate it to the current version.

use serde::{Deserialize, Serialize};

/// Schema version identifier.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SchemaVersion {
    /// Version 0 - initial schema
    #[default]
    V0,
    // Future versions will be added here:
    // V1,
    // V2,
}

impl SchemaVersion {
    /// Get the current (latest) schema version.
    pub fn current() -> Self {
        SchemaVersion::V0
    }

    /// Check if this version can be migrated to the target version.
    pub fn can_migrate_to(&self, target: SchemaVersion) -> bool {
        // Can only migrate forward
        *self <= target
    }

    /// Get the version number as an integer.
    pub fn as_number(&self) -> u32 {
        match self {
            SchemaVersion::V0 => 0,
        }
    }
}

impl std::fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchemaVersion::V0 => write!(f, "v0"),
        }
    }
}

/// Migration result.
#[derive(Debug)]
pub struct MigrationResult<T> {
    /// The migrated document
    pub document: T,
    /// The source version
    pub from_version: SchemaVersion,
    /// The target version
    pub to_version: SchemaVersion,
    /// Any warnings generated during migration
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_ordering() {
        assert!(SchemaVersion::V0 <= SchemaVersion::V0);
        assert!(SchemaVersion::V0.can_migrate_to(SchemaVersion::V0));
    }

    #[test]
    fn test_version_display() {
        assert_eq!(SchemaVersion::V0.to_string(), "v0");
    }

    #[test]
    fn test_version_serde() {
        let v = SchemaVersion::V0;
        let json = serde_json::to_string(&v).expect("serialize");
        assert_eq!(json, "\"v0\"");
        let parsed: SchemaVersion = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, v);
    }
}
