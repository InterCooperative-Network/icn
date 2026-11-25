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

/// Domain tag of [`served_domain_commitment`] (EXPERIMENTAL, unratified).
pub const SERVED_DOMAIN_DOMAIN: &[u8] = b"icn.governance.served-domain.v1";

/// Why a [`GovernanceDomainId`] has no served-domain commitment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ServedDomainError {
    /// The id is the empty string, which names no decision space.
    #[error("governance domain id is empty")]
    Empty,
    /// The id's UTF-8 length does not fit the `u32be` length frame.
    #[error("governance domain id exceeds the u32 length frame")]
    TooLong,
}

/// The commitment a context inception's `served_subject` carries when the context serves the
/// governance decision space `domain`:
///
/// `SHA-256( u32be(len(tag)) || tag || u32be(len(utf8 id)) || utf8 id )`, with
/// `tag = icn.governance.served-domain.v1`.
///
/// The id is committed as its exact UTF-8 bytes — no trimming, case folding or normalization —
/// and the empty id is refused. The commitment names a **decision space** only: it is not an
/// `EntityId` and says nothing about which institution owns the domain. EXPERIMENTAL, unratified.
pub fn served_domain_commitment(
    domain: &GovernanceDomainId,
) -> Result<[u8; 32], ServedDomainError> {
    use sha2::{Digest, Sha256};

    let id = domain.0.as_bytes();
    if id.is_empty() {
        return Err(ServedDomainError::Empty);
    }
    let id_len = u32::try_from(id.len()).map_err(|_| ServedDomainError::TooLong)?;
    let tag_len =
        u32::try_from(SERVED_DOMAIN_DOMAIN.len()).map_err(|_| ServedDomainError::TooLong)?;
    let mut hasher = Sha256::new();
    hasher.update(tag_len.to_be_bytes());
    hasher.update(SERVED_DOMAIN_DOMAIN);
    hasher.update(id_len.to_be_bytes());
    hasher.update(id);
    Ok(hasher.finalize().into())
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
        // Leaving Subject mode would re-open the legacy rails on a domain that
        // opted out of them; there is no downgrade path in this slice.
        if self.human_actor_model == HumanActorModel::ContextSubjectV1
            && model != HumanActorModel::ContextSubjectV1
        {
            return Err(ActorModelError::ModeChangeRefused {
                domain: self.id.0.clone(),
            });
        }
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

    /// Reference vectors computed independently (Python `hashlib`, u32be framing), not by this
    /// implementation.
    #[test]
    fn served_domain_commitment_reference_vectors() {
        let vectors: &[(&str, &str)] = &[
            (
                "coop-a",
                "dc0235281c57ac74fa3106a07927ff206c7bfe8117e340501d4a3df30d534e52",
            ),
            (
                "a",
                "2f3c66e82fbf13929f1c31e228d59d2e5551ee0fe75e41a0f170c1b86914a5ba",
            ),
            (
                "coop:x:y",
                "670ebf5440aa9f828b3f7f5079db92878f9180a69147d4bbf8bcd1d0ee638769",
            ),
            (
                "Coop-A",
                "596a472dc68ec12013789d8be6414d621cada5ce78a4cbad150710367c020a48",
            ),
            (
                "coop-a ",
                "77676cc359aa47a266137b101e00a122226e2a10bbd522d4175c21a7b034943d",
            ),
            (
                "d\u{e9}f",
                "5df8d0526a71db7bad00063fd0ffd8ded3e76a9005eece0c0763f67efb763951",
            ),
        ];
        for (id, expected) in vectors {
            let got = served_domain_commitment(&GovernanceDomainId::new(*id)).unwrap();
            assert_eq!(hex::encode(got), *expected, "id {id:?}");
        }
    }

    #[test]
    fn served_domain_commitment_refuses_empty() {
        assert_eq!(
            served_domain_commitment(&GovernanceDomainId::new("")),
            Err(ServedDomainError::Empty)
        );
    }

    /// Exact bytes: no trimming or case folding, and distinct ids never share a commitment.
    #[test]
    fn served_domain_commitment_is_exact_and_injective_on_samples() {
        let ids = [
            "coop-a", "Coop-A", "coop-a ", " coop-a", "coop-a\0", "coop-b", "coop",
        ];
        let commits: std::collections::BTreeSet<[u8; 32]> = ids
            .iter()
            .map(|id| served_domain_commitment(&GovernanceDomainId::new(*id)).unwrap())
            .collect();
        assert_eq!(commits.len(), ids.len());
    }
}
