//! Core entity types for the ICN unified cooperative model
//!
//! This module defines the foundational types for representing entities at all scales:
//! - Individuals (backed by DIDs)
//! - Cooperatives (worker, consumer, multi-stakeholder)
//! - Communities (geographic, interest-based, or practice-based groups)
//! - Federations (cooperatives of cooperatives)
//!
//! The key insight is that membership is recursive: a cooperative's members
//! can themselves be cooperatives. Communities bridge individuals and cooperatives,
//! allowing diverse membership models.

use crate::error::{EntityError, Result};
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

// ============================================================================
// EntityId
// ============================================================================

/// Unique identifier for any entity (individual, cooperative, community, or federation)
///
/// EntityId is designed to be compatible with DIDs:
/// - For individuals: wraps their DID directly
/// - For cooperatives: derived from cooperative creation ceremony
/// - For communities: derived from community formation
/// - For federations: derived from federation charter
///
/// # Format
///
/// `entity:icn:<type>:<identifier>`
///
/// # Examples
///
/// - `entity:icn:individual:z5TrA8...` (wraps DID)
/// - `entity:icn:cooperative:food-coop-2024`
/// - `entity:icn:community:neighborhood-2024`
/// - `entity:icn:federation:midwest-fed`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(String);

impl EntityId {
    /// Create an EntityId from an individual's DID
    ///
    /// This wraps the DID in the entity format, allowing individuals
    /// to participate in the entity system while maintaining their
    /// cryptographic identity.
    pub fn from_did(did: &Did) -> Self {
        let did_str = did.as_str();
        let identifier = did_str.strip_prefix("did:icn:").unwrap_or(did_str);
        EntityId(format!("entity:icn:individual:{identifier}"))
    }

    /// Create an EntityId for a cooperative
    ///
    /// The slug should be a URL-safe identifier for the cooperative.
    /// It will be validated for allowed characters.
    pub fn cooperative(slug: &str) -> Result<Self> {
        Self::validate_slug(slug)?;
        Ok(EntityId(format!("entity:icn:cooperative:{slug}")))
    }

    /// Create an EntityId for a federation
    ///
    /// The slug should be a URL-safe identifier for the federation.
    pub fn federation(slug: &str) -> Result<Self> {
        Self::validate_slug(slug)?;
        Ok(EntityId(format!("entity:icn:federation:{slug}")))
    }

    /// Create an EntityId for a community
    ///
    /// The slug should be a URL-safe identifier for the community.
    /// Communities can have both Individual and Cooperative members,
    /// and can join Federations.
    pub fn community(slug: &str) -> Result<Self> {
        Self::validate_slug(slug)?;
        Ok(EntityId(format!("entity:icn:community:{slug}")))
    }

    /// Validate a slug for use in EntityId
    ///
    /// Rules:
    /// - 4-64 characters (minimum 4 to avoid namespace collisions)
    /// - Lowercase letters, numbers, hyphens only
    /// - Must start with a letter
    /// - No consecutive hyphens
    fn validate_slug(slug: &str) -> Result<()> {
        if slug.len() < 4 {
            return Err(EntityError::InvalidFormat(
                "Slug must be at least 4 characters".into(),
            ));
        }
        if slug.len() > 64 {
            return Err(EntityError::InvalidFormat(
                "Slug cannot exceed 64 characters".into(),
            ));
        }

        let mut chars = slug.chars().peekable();

        // Must start with a lowercase letter
        match chars.next() {
            Some(c) if c.is_ascii_lowercase() => {}
            _ => {
                return Err(EntityError::InvalidFormat(
                    "Slug must start with a lowercase letter".into(),
                ));
            }
        }

        // Check remaining characters
        let mut prev_hyphen = false;
        for c in chars {
            if c == '-' {
                if prev_hyphen {
                    return Err(EntityError::InvalidFormat(
                        "Slug cannot contain consecutive hyphens".into(),
                    ));
                }
                prev_hyphen = true;
            } else if c.is_ascii_lowercase() || c.is_ascii_digit() {
                prev_hyphen = false;
            } else {
                return Err(EntityError::InvalidFormat(
                    "Slug can only contain lowercase letters, numbers, and hyphens".into(),
                ));
            }
        }

        Ok(())
    }

    /// Get the entity type from this ID
    ///
    /// Parses the format `entity:icn:<type>:<identifier>` and extracts the type.
    pub fn entity_type(&self) -> EntityType {
        // Expected format: entity:icn:<type>:<identifier>
        let mut parts = self.0.split(':');
        // Skip "entity" and "icn"
        let _ = parts.next();
        let _ = parts.next();
        match parts.next() {
            Some("individual") => EntityType::Individual,
            Some("cooperative") => EntityType::Cooperative,
            Some("community") => EntityType::Community,
            Some("federation") => EntityType::Federation,
            _ => EntityType::Unknown,
        }
    }

    /// Try to extract the underlying DID (only works for individuals)
    ///
    /// Returns `Some(Did)` if this entity represents an individual,
    /// `None` for cooperatives and federations.
    pub fn to_did(&self) -> Option<Did> {
        if self.entity_type() == EntityType::Individual {
            let identifier = self.0.strip_prefix("entity:icn:individual:")?;
            Did::from_str(&format!("did:icn:{identifier}")).ok()
        } else {
            None
        }
    }

    /// Get the raw identifier portion (after the type prefix)
    pub fn identifier(&self) -> &str {
        self.0.split(':').next_back().unwrap_or(&self.0)
    }

    /// Get the full entity ID string
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Check if this entity represents an individual
    pub fn is_individual(&self) -> bool {
        self.entity_type() == EntityType::Individual
    }

    /// Check if this entity represents a cooperative
    pub fn is_cooperative(&self) -> bool {
        self.entity_type() == EntityType::Cooperative
    }

    /// Check if this entity represents a federation
    pub fn is_federation(&self) -> bool {
        self.entity_type() == EntityType::Federation
    }

    /// Check if this entity represents a community
    pub fn is_community(&self) -> bool {
        self.entity_type() == EntityType::Community
    }

    /// Check if this entity represents an organization (cooperative, community, or federation)
    pub fn is_organization(&self) -> bool {
        matches!(
            self.entity_type(),
            EntityType::Cooperative | EntityType::Community | EntityType::Federation
        )
    }
}

impl FromStr for EntityId {
    type Err = EntityError;

    fn from_str(s: &str) -> Result<Self> {
        if !s.starts_with("entity:icn:") {
            return Err(EntityError::InvalidFormat(
                "EntityId must start with 'entity:icn:'".into(),
            ));
        }

        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 4 {
            return Err(EntityError::InvalidFormat(
                "EntityId must have format 'entity:icn:<type>:<identifier>'".into(),
            ));
        }

        let type_part = parts[2];
        let identifier = parts[3];

        match type_part {
            "individual" => {
                // Individual identifiers are base58-encoded public keys
                // Just verify it's non-empty
                if identifier.is_empty() {
                    return Err(EntityError::InvalidFormat(
                        "Individual identifier cannot be empty".into(),
                    ));
                }
            }
            "cooperative" | "community" | "federation" => {
                // Cooperative/community/federation identifiers must be valid slugs
                Self::validate_slug(identifier)?;
            }
            _ => {
                return Err(EntityError::InvalidFormat(format!(
                    "Unknown entity type: {type_part}"
                )));
            }
        }

        Ok(EntityId(s.to_string()))
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ============================================================================
// EntityType
// ============================================================================

/// Entity types supported by ICN
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntityType {
    /// An individual person (backed by DID/SDIS anchor)
    Individual,

    /// A cooperative (worker, consumer, producer, multi-stakeholder, etc.)
    Cooperative,

    /// A community (geographic, interest-based, or practice-based group)
    ///
    /// Communities can have both Individual and Cooperative members.
    /// They can join Federations, and Cooperatives can join Communities.
    Community,

    /// A federation of cooperatives
    Federation,

    /// Unknown/unrecognized type
    Unknown,
}

impl fmt::Display for EntityType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EntityType::Individual => write!(f, "individual"),
            EntityType::Cooperative => write!(f, "cooperative"),
            EntityType::Community => write!(f, "community"),
            EntityType::Federation => write!(f, "federation"),
            EntityType::Unknown => write!(f, "unknown"),
        }
    }
}

// ============================================================================
// EntityStatus
// ============================================================================

/// Entity lifecycle status
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityStatus {
    /// Entity is being formed (pre-charter ratification)
    #[default]
    Forming,

    /// Entity is active and operational
    Active,

    /// Entity is suspended (frozen)
    Suspended {
        /// Reason for suspension
        reason: String,
        /// When suspended (Unix timestamp)
        suspended_at: u64,
    },

    /// Entity is in the process of dissolving
    Dissolving {
        /// When dissolution started
        started_at: u64,
    },

    /// Entity has been dissolved
    Dissolved {
        /// When dissolved
        dissolved_at: u64,
    },

    /// Entity has merged into another entity
    Merged {
        /// The entity this was merged into
        into: EntityId,
        /// When merged
        merged_at: u64,
    },

    /// Entity has split into multiple entities
    Split {
        /// The entities created from this split
        into: Vec<EntityId>,
        /// When split
        split_at: u64,
    },

    /// Entity has been deleted (tombstone for gossip sync)
    Deleted,
}

impl EntityStatus {
    /// Check if entity is operational (can perform normal operations)
    pub fn is_operational(&self) -> bool {
        matches!(self, EntityStatus::Active)
    }

    /// Check if entity lifecycle has ended
    pub fn is_terminated(&self) -> bool {
        matches!(
            self,
            EntityStatus::Dissolved { .. }
                | EntityStatus::Merged { .. }
                | EntityStatus::Split { .. }
                | EntityStatus::Deleted
        )
    }
}

impl std::fmt::Display for EntityStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityStatus::Forming => write!(f, "Forming"),
            EntityStatus::Active => write!(f, "Active"),
            EntityStatus::Suspended { reason, .. } => write!(f, "Suspended: {reason}"),
            EntityStatus::Dissolving { .. } => write!(f, "Dissolving"),
            EntityStatus::Dissolved { .. } => write!(f, "Dissolved"),
            EntityStatus::Merged { into, .. } => write!(f, "Merged into {}", into.identifier()),
            EntityStatus::Split { into, .. } => {
                write!(f, "Split into {} entities", into.len())
            }
            EntityStatus::Deleted => write!(f, "Deleted"),
        }
    }
}

// ============================================================================
// AccountReference
// ============================================================================

/// Reference to an account in the ledger
///
/// This links an entity to its treasury/ledger account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountReference {
    /// The account identifier (can be EntityId string or legacy Did string)
    pub account_id: String,

    /// Primary currency managed by this account
    pub currency: String,
}

impl AccountReference {
    /// Create a new account reference
    pub fn new(account_id: impl Into<String>, currency: impl Into<String>) -> Self {
        AccountReference {
            account_id: account_id.into(),
            currency: currency.into(),
        }
    }
}

// ============================================================================
// CooperativeEntity
// ============================================================================

/// A cooperative entity that works at all scales
///
/// This is the unified model that can represent:
/// - Individuals (with DID binding)
/// - Cooperatives of various types
/// - Federations of cooperatives
///
/// The recursive membership model allows entities to contain other entities
/// as members, enabling arbitrary organizational hierarchies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CooperativeEntity {
    /// Unique entity identifier
    pub id: EntityId,

    /// Human-readable name
    pub name: String,

    /// Entity type (individual, cooperative, federation)
    pub entity_type: EntityType,

    /// Lifecycle status
    pub status: EntityStatus,

    /// Parent entity (for hierarchical membership)
    /// None for top-level entities or individuals not in any organization
    pub parent_id: Option<EntityId>,

    /// Governance domain ID (links to icn-governance)
    /// Individuals typically don't have governance domains
    pub governance_domain_id: Option<String>,

    /// Treasury account (links to icn-ledger)
    /// Individuals may not have treasury; coops always do
    pub treasury_account: Option<AccountReference>,

    /// When the entity was created (Unix timestamp)
    pub created_at: u64,

    /// When the entity was last updated (Unix timestamp)
    pub updated_at: u64,

    /// Optional description
    pub description: Option<String>,

    /// Arbitrary metadata for extensibility
    pub metadata: HashMap<String, String>,
}

impl CooperativeEntity {
    /// Create a new entity for an individual
    pub fn individual(did: &Did, name: impl Into<String>) -> Self {
        let now = icn_time::current_timestamp_secs();
        CooperativeEntity {
            id: EntityId::from_did(did),
            name: name.into(),
            entity_type: EntityType::Individual,
            status: EntityStatus::Active, // Individuals are active immediately
            parent_id: None,
            governance_domain_id: None,
            treasury_account: None,
            created_at: now,
            updated_at: now,
            description: None,
            metadata: HashMap::new(),
        }
    }

    /// Create a new cooperative entity
    pub fn cooperative(slug: &str, name: impl Into<String>) -> Result<Self> {
        let now = icn_time::current_timestamp_secs();
        Ok(CooperativeEntity {
            id: EntityId::cooperative(slug)?,
            name: name.into(),
            entity_type: EntityType::Cooperative,
            status: EntityStatus::Forming,
            parent_id: None,
            governance_domain_id: None,
            treasury_account: None,
            created_at: now,
            updated_at: now,
            description: None,
            metadata: HashMap::new(),
        })
    }

    /// Create a new federation entity
    pub fn federation(slug: &str, name: impl Into<String>) -> Result<Self> {
        let now = icn_time::current_timestamp_secs();
        Ok(CooperativeEntity {
            id: EntityId::federation(slug)?,
            name: name.into(),
            entity_type: EntityType::Federation,
            status: EntityStatus::Forming,
            parent_id: None,
            governance_domain_id: None,
            treasury_account: None,
            created_at: now,
            updated_at: now,
            description: None,
            metadata: HashMap::new(),
        })
    }

    /// Create a new community entity
    ///
    /// Communities are geographic, interest-based, or practice-based groups
    /// that can have both Individual and Cooperative members. They can
    /// also join Federations.
    pub fn community(slug: &str, name: impl Into<String>) -> Result<Self> {
        let now = icn_time::current_timestamp_secs();
        Ok(CooperativeEntity {
            id: EntityId::community(slug)?,
            name: name.into(),
            entity_type: EntityType::Community,
            status: EntityStatus::Forming,
            parent_id: None,
            governance_domain_id: None,
            treasury_account: None,
            created_at: now,
            updated_at: now,
            description: None,
            metadata: HashMap::new(),
        })
    }

    // ========================================================================
    // Builder methods
    //
    // Note: Builder methods are for initial construction and do NOT update
    // the `updated_at` timestamp. When modifying an existing entity, use
    // `EntityRegistry::update()` which should update the timestamp.
    // ========================================================================

    /// Set the parent entity
    #[must_use]
    pub fn with_parent(mut self, parent_id: EntityId) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    /// Set the governance domain
    #[must_use]
    pub fn with_governance_domain(mut self, domain_id: impl Into<String>) -> Self {
        self.governance_domain_id = Some(domain_id.into());
        self
    }

    /// Set the treasury account
    #[must_use]
    pub fn with_treasury(mut self, account: AccountReference) -> Self {
        self.treasury_account = Some(account);
        self
    }

    /// Set the description
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add metadata
    #[must_use]
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Check if entity can be activated
    pub fn can_activate(&self) -> bool {
        matches!(self.status, EntityStatus::Forming)
    }

    /// Check if entity can be suspended
    pub fn can_suspend(&self) -> bool {
        matches!(self.status, EntityStatus::Active)
    }

    /// Check if entity can be dissolved
    pub fn can_dissolve(&self) -> bool {
        matches!(
            self.status,
            EntityStatus::Active | EntityStatus::Suspended { .. }
        )
    }
}

// ============================================================================
// AccountId (Backward Compatibility)
// ============================================================================

/// Account identifier that works with both legacy DIDs and new EntityIds
///
/// This type provides backward compatibility during the migration from
/// DID-based account identification to EntityId-based identification.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AccountId {
    /// Legacy DID-based account (individual)
    Did(Did),

    /// New EntityId-based account (individual, cooperative, or federation)
    Entity(EntityId),
}

impl AccountId {
    /// Get string representation
    pub fn as_str(&self) -> String {
        match self {
            AccountId::Did(did) => did.to_string(),
            AccountId::Entity(id) => id.to_string(),
        }
    }

    /// Check if this is an individual account
    pub fn is_individual(&self) -> bool {
        match self {
            AccountId::Did(_) => true,
            AccountId::Entity(id) => id.is_individual(),
        }
    }
}

impl From<Did> for AccountId {
    fn from(did: Did) -> Self {
        AccountId::Did(did)
    }
}

impl From<EntityId> for AccountId {
    fn from(id: EntityId) -> Self {
        AccountId::Entity(id)
    }
}

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    #[test]
    fn test_entity_id_from_did() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();
        let entity_id = EntityId::from_did(&did);

        assert!(entity_id.is_individual());
        assert!(!entity_id.is_cooperative());
        assert!(!entity_id.is_federation());
        assert_eq!(entity_id.entity_type(), EntityType::Individual);

        // Should be able to convert back to DID
        let recovered_did = entity_id.to_did().unwrap();
        assert_eq!(recovered_did, did);
    }

    #[test]
    fn test_entity_id_cooperative() {
        let entity_id = EntityId::cooperative("food-coop-2024").unwrap();

        assert!(!entity_id.is_individual());
        assert!(entity_id.is_cooperative());
        assert!(!entity_id.is_federation());
        assert!(entity_id.is_organization());
        assert_eq!(entity_id.entity_type(), EntityType::Cooperative);
        assert_eq!(entity_id.identifier(), "food-coop-2024");

        // Should not be able to convert to DID
        assert!(entity_id.to_did().is_none());
    }

    #[test]
    fn test_entity_id_federation() {
        let entity_id = EntityId::federation("midwest-fed").unwrap();

        assert!(entity_id.is_federation());
        assert!(entity_id.is_organization());
        assert_eq!(entity_id.entity_type(), EntityType::Federation);
    }

    #[test]
    fn test_entity_id_community() {
        let entity_id = EntityId::community("neighborhood-2024").unwrap();

        assert!(!entity_id.is_individual());
        assert!(!entity_id.is_cooperative());
        assert!(!entity_id.is_federation());
        assert!(entity_id.is_community());
        assert!(entity_id.is_organization());
        assert_eq!(entity_id.entity_type(), EntityType::Community);
        assert_eq!(entity_id.identifier(), "neighborhood-2024");

        // Should not be able to convert to DID
        assert!(entity_id.to_did().is_none());
    }

    #[test]
    fn test_community_entity_creation() {
        let entity =
            CooperativeEntity::community("local-community", "Local Community Network").unwrap();

        assert!(entity.id.is_community());
        assert_eq!(entity.entity_type, EntityType::Community);
        assert_eq!(entity.name, "Local Community Network");
        assert!(matches!(entity.status, EntityStatus::Forming));
    }

    #[test]
    fn test_is_organization_includes_community() {
        let keypair = KeyPair::generate().unwrap();
        let individual_id = EntityId::from_did(keypair.did());
        let coop_id = EntityId::cooperative("test-coop").unwrap();
        let community_id = EntityId::community("test-community").unwrap();
        let fed_id = EntityId::federation("test-fed").unwrap();

        // Individuals are NOT organizations
        assert!(!individual_id.is_organization());

        // Cooperatives, communities, and federations ARE organizations
        assert!(coop_id.is_organization());
        assert!(community_id.is_organization());
        assert!(fed_id.is_organization());
    }

    #[test]
    fn test_entity_id_parse() {
        let id_str = "entity:icn:cooperative:test-coop";
        let entity_id: EntityId = id_str.parse().unwrap();
        assert_eq!(entity_id.as_str(), id_str);
        assert!(entity_id.is_cooperative());
    }

    #[test]
    fn test_entity_id_invalid_format() {
        assert!(EntityId::from_str("invalid").is_err());
        assert!(EntityId::from_str("entity:other:cooperative:test").is_err());
        assert!(EntityId::from_str("entity:icn:unknown:test").is_err());
    }

    #[test]
    fn test_entity_id_fromstr_validates_slug() {
        // Valid FromStr parsing
        assert!(EntityId::from_str("entity:icn:cooperative:valid-slug").is_ok());
        assert!(EntityId::from_str("entity:icn:federation:test-fed").is_ok());
        assert!(EntityId::from_str("entity:icn:individual:z5TrA8Qk").is_ok());

        // Invalid: cooperative/federation slugs must pass validation
        assert!(EntityId::from_str("entity:icn:cooperative:ab").is_err()); // too short
        assert!(EntityId::from_str("entity:icn:cooperative:abc").is_err()); // too short (< 4)
        assert!(EntityId::from_str("entity:icn:federation:123").is_err()); // doesn't start with letter
        assert!(EntityId::from_str("entity:icn:cooperative:Test").is_err()); // uppercase

        // Individual identifiers just need to be non-empty
        assert!(EntityId::from_str("entity:icn:individual:").is_err()); // empty
    }

    #[test]
    fn test_entity_id_slug_validation() {
        // Valid slugs (lowercase, 4-64 chars, start with letter)
        assert!(EntityId::cooperative("valid-slug").is_ok());
        assert!(EntityId::cooperative("abcd").is_ok());
        assert!(EntityId::cooperative("test123").is_ok());
        assert!(EntityId::cooperative("food-coop-2024").is_ok());

        // Invalid: too short (minimum 4 chars)
        assert!(EntityId::cooperative("abc").is_err());
        assert!(EntityId::cooperative("ab").is_err());

        // Invalid: doesn't start with letter
        assert!(EntityId::cooperative("123-coop").is_err());
        assert!(EntityId::cooperative("-test").is_err());

        // Invalid: uppercase
        assert!(EntityId::cooperative("ValidSlug").is_err());

        // Invalid: underscores not allowed
        assert!(EntityId::cooperative("valid_slug").is_err());

        // Invalid: consecutive hyphens
        assert!(EntityId::cooperative("test--coop").is_err());

        // Invalid: empty or spaces
        assert!(EntityId::cooperative("").is_err());
        assert!(EntityId::cooperative("invalid slug").is_err());
        assert!(EntityId::cooperative("invalid/slug").is_err());
    }

    #[test]
    fn test_cooperative_entity_creation() {
        let entity = CooperativeEntity::cooperative("test-coop", "Test Cooperative").unwrap();

        assert_eq!(entity.name, "Test Cooperative");
        assert!(entity.id.is_cooperative());
        assert!(matches!(entity.status, EntityStatus::Forming));
        assert!(entity.can_activate());
    }

    #[test]
    fn test_individual_entity_creation() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();
        let entity = CooperativeEntity::individual(&did, "Alice");

        assert_eq!(entity.name, "Alice");
        assert!(entity.id.is_individual());
        assert!(matches!(entity.status, EntityStatus::Active));
    }

    #[test]
    fn test_entity_serialization() {
        let entity = CooperativeEntity::cooperative("test", "Test").unwrap();
        let json = serde_json::to_string(&entity).unwrap();
        let parsed: CooperativeEntity = serde_json::from_str(&json).unwrap();
        assert_eq!(entity.id, parsed.id);
        assert_eq!(entity.name, parsed.name);
    }

    #[test]
    fn test_account_id_from_did() {
        let keypair = KeyPair::generate().unwrap();
        let did = keypair.did().clone();
        let account_id: AccountId = did.clone().into();

        assert!(account_id.is_individual());
        assert!(matches!(account_id, AccountId::Did(_)));
    }

    #[test]
    fn test_account_id_from_entity() {
        let entity_id = EntityId::cooperative("test").unwrap();
        let account_id: AccountId = entity_id.into();

        assert!(!account_id.is_individual());
        assert!(matches!(account_id, AccountId::Entity(_)));
    }
}
