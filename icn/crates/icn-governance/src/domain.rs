//! Governance domain - the decision space for a community

use crate::config::GovernanceConfig;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a governance domain
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GovernanceDomainId(pub String);

impl GovernanceDomainId {
    /// Create a new domain ID from a string
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Generate a random domain ID
    pub fn generate() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl std::fmt::Display for GovernanceDomainId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A governance domain represents a decision space for a community
///
/// Examples: a cooperative, a working group, a project team
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceDomain {
    /// Unique identifier for this domain
    pub id: GovernanceDomainId,

    /// Human-readable name (e.g., "Tech Workers Coop")
    pub name: String,

    /// Optional description
    pub description: Option<String>,

    /// Governance configuration (rules, membership, etc.)
    pub config: GovernanceConfig,

    /// When this domain was created (Unix timestamp)
    pub created_at: u64,

    /// When this domain was last updated
    pub updated_at: u64,
}

impl GovernanceDomain {
    /// Create a new governance domain
    pub fn new(name: String, config: GovernanceConfig) -> Self {
        let now = icn_time::current_timestamp_secs();

        Self {
            id: GovernanceDomainId::generate(),
            name,
            description: None,
            config,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a domain with a specific ID (for testing or import)
    pub fn with_id(id: GovernanceDomainId, name: String, config: GovernanceConfig) -> Self {
        let now = icn_time::current_timestamp_secs();

        Self {
            id,
            name,
            description: None,
            config,
            created_at: now,
            updated_at: now,
        }
    }

    /// Set the description
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Update the governance configuration
    pub fn update_config(&mut self, config: GovernanceConfig) {
        self.config = config;
        self.updated_at = icn_time::current_timestamp_secs();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_creation() {
        let config = GovernanceConfig::cooperative_default();
        let domain = GovernanceDomain::new("Test Coop".to_string(), config);

        assert_eq!(domain.name, "Test Coop");
        assert!(domain.description.is_none());
        assert!(domain.created_at > 0);
        assert_eq!(domain.created_at, domain.updated_at);
    }

    #[test]
    fn test_domain_with_description() {
        let config = GovernanceConfig::cooperative_default();
        let domain = GovernanceDomain::new("Test Coop".to_string(), config)
            .with_description("A test cooperative".to_string());

        assert_eq!(domain.description, Some("A test cooperative".to_string()));
    }

    #[test]
    fn test_domain_id_display() {
        let id = GovernanceDomainId::new("test-domain");
        assert_eq!(format!("{id}"), "test-domain");
    }
}
