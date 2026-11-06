//! Governance domain - the decision space for a community

use crate::actor_model::{
    check_no_legacy_membership, ActorModelError, HumanActorModel, LegacyRailOp,
};
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
#[serde(try_from = "GovernanceDomainWire")]
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

    /// Which kind of human actor this domain recognises.
    ///
    /// Absent in serialized domains that predate the field, which load as
    /// [`HumanActorModel::LegacyPrincipalV0`]. Omitted again on serialization
    /// when legacy, so legacy encodings are unchanged. Set only through
    /// [`GovernanceDomain::with_human_actor_model`]; see [`crate::actor_model`].
    #[serde(default, skip_serializing_if = "HumanActorModel::is_legacy")]
    human_actor_model: HumanActorModel,
}

/// Deserialization shape for [`GovernanceDomain`]. Exists so that the
/// mixed-state invariant is checked on every load path (store, gossip,
/// import) rather than only where a caller remembers to.
#[derive(Deserialize)]
struct GovernanceDomainWire {
    id: GovernanceDomainId,
    name: String,
    description: Option<String>,
    config: GovernanceConfig,
    created_at: u64,
    updated_at: u64,
    #[serde(default)]
    human_actor_model: HumanActorModel,
}

impl TryFrom<GovernanceDomainWire> for GovernanceDomain {
    type Error = ActorModelError;

    fn try_from(w: GovernanceDomainWire) -> Result<Self, Self::Error> {
        let domain = Self {
            id: w.id,
            name: w.name,
            description: w.description,
            config: w.config,
            created_at: w.created_at,
            updated_at: w.updated_at,
            human_actor_model: w.human_actor_model,
        };
        domain.check_actor_model_invariant()?;
        Ok(domain)
    }
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
            human_actor_model: HumanActorModel::default(),
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
            human_actor_model: HumanActorModel::default(),
        }
    }

    /// Set the description
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// The domain's human actor model.
    pub fn human_actor_model(&self) -> HumanActorModel {
        self.human_actor_model
    }

    /// Select the human actor model at construction time.
    ///
    /// Refuses (returns `Err`, consuming nothing observable) when the domain
    /// already carries a legacy membership source and `model` is
    /// [`HumanActorModel::ContextSubjectV1`]. There is no in-place switch for
    /// an existing domain: membership is never converted or dropped.
    pub fn with_human_actor_model(
        mut self,
        model: HumanActorModel,
    ) -> Result<Self, ActorModelError> {
        self.human_actor_model = model;
        self.check_actor_model_invariant()?;
        Ok(self)
    }

    /// Verify the mixed-state invariant: a Subject-mode domain holds no legacy
    /// membership source.
    pub fn check_actor_model_invariant(&self) -> Result<(), ActorModelError> {
        match self.human_actor_model {
            HumanActorModel::LegacyPrincipalV0 => Ok(()),
            HumanActorModel::ContextSubjectV1 => {
                check_no_legacy_membership(&self.id.0, &self.config.membership.source)
            }
        }
    }

    /// Refuse a legacy Principal-rail operation on a Subject-mode domain.
    ///
    /// Call this **before** any mutation, persistence write, event emission or
    /// authorization decision. Legacy domains always pass.
    pub fn require_legacy_rail(&self, op: LegacyRailOp) -> Result<(), ActorModelError> {
        match self.human_actor_model {
            HumanActorModel::LegacyPrincipalV0 => Ok(()),
            HumanActorModel::ContextSubjectV1 => Err(ActorModelError::LegacyRailRefused {
                domain: self.id.0.clone(),
                op,
            }),
        }
    }

    /// Update the governance configuration.
    ///
    /// On a Subject-mode domain the new configuration must also satisfy the
    /// mixed-state invariant; otherwise nothing is changed.
    pub fn update_config(&mut self, config: GovernanceConfig) -> Result<(), ActorModelError> {
        if self.human_actor_model == HumanActorModel::ContextSubjectV1 {
            check_no_legacy_membership(&self.id.0, &config.membership.source)?;
        }
        self.config = config;
        self.updated_at = icn_time::current_timestamp_secs();
        Ok(())
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
