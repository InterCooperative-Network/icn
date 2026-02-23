//! Canonical identity and resource types for the capability graph.
//!
//! All types implement full ordering and serde round-tripping.
//! Enum variant order is FROZEN -- append only, never reorder.

use serde::{Deserialize, Serialize};

use crate::AuthzError;

// ---------------------------------------------------------------------------
// SubjectId
// ---------------------------------------------------------------------------

/// A validated DID identifying a capability subject.
///
/// Must start with `"did:"`. Empty strings are rejected.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SubjectId(String);

impl SubjectId {
    /// Create a new `SubjectId`, rejecting values that do not start with `"did:"`.
    pub fn new(did: impl Into<String>) -> Result<Self, AuthzError> {
        let s = did.into();
        if !s.starts_with("did:") {
            return Err(AuthzError::InvalidSubjectId(s));
        }
        Ok(Self(s))
    }

    /// Borrow the inner DID string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// Action
// ---------------------------------------------------------------------------

/// A validated, canonicalized action string in `domain:verb[:subverb...]` form.
///
/// Construction normalizes the input:
/// - Trims whitespace
/// - Lowercases to ASCII
/// - Validates at least two colon-separated segments, no empty segments,
///   and only ASCII alphanumeric, colon, or hyphen characters.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Action(String);

impl Action {
    /// Create a new `Action` with soft normalization (trim + lowercase).
    ///
    /// Returns `Err` if the result doesn't match `domain:verb[:subverb...]`
    /// with only ASCII alphanumeric, colon, and hyphen characters.
    pub fn new(raw: impl Into<String>) -> Result<Self, AuthzError> {
        let s: String = raw.into().trim().to_ascii_lowercase();

        if s.is_empty() {
            return Err(AuthzError::InvalidAction(s));
        }

        // Only ASCII alphanumeric + colon + hyphen allowed
        if !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b':' || b == b'-')
        {
            return Err(AuthzError::InvalidAction(s));
        }

        let segments: Vec<&str> = s.split(':').collect();

        // Must have >= 2 segments, no empty segments
        if segments.len() < 2 || segments.iter().any(|seg| seg.is_empty()) {
            return Err(AuthzError::InvalidAction(s));
        }

        Ok(Self(s))
    }

    /// Borrow the canonical action string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The first colon-delimited segment (e.g. `"ledger"` in `"ledger:transfer"`).
    pub fn domain(&self) -> &str {
        // SAFETY: constructor validated at least two segments separated by ':'.
        // split(':').next() always returns Some on a non-empty string.
        self.0.split(':').next().unwrap_or("")
    }

    /// The second colon-delimited segment (e.g. `"transfer"` in `"ledger:transfer"`).
    pub fn verb(&self) -> &str {
        // SAFETY: constructor validated at least two segments separated by ':'.
        self.0.split(':').nth(1).unwrap_or("")
    }
}

// ---------------------------------------------------------------------------
// ResourceKind
// ---------------------------------------------------------------------------

/// The kind of resource a capability applies to.
///
/// Variant order is FROZEN. Do not reorder. Append only.
/// Tag bytes: Entity=0x01, Scope=0x02, Asset=0x03, Contract=0x04, System=0x05
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ResourceKind {
    /// A cooperative entity (individual, coop, federation).
    Entity,
    /// A scope/namespace boundary.
    Scope,
    /// A tracked asset (credit line, token, etc.).
    Asset,
    /// A CCL contract instance.
    Contract,
    /// A system-level resource (metrics, config, etc.).
    System,
}

// ---------------------------------------------------------------------------
// ResourceId
// ---------------------------------------------------------------------------

/// Identifies a specific resource by kind and ID.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResourceId {
    pub kind: ResourceKind,
    pub id: String,
}

impl ResourceId {
    /// Create a new resource identifier.
    pub fn new(kind: ResourceKind, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Constraint
// ---------------------------------------------------------------------------

/// A constraint attached to a capability edge.
///
/// Variant order is FROZEN. Do not reorder. Append only.
/// Tag bytes: RateLimit=0x01, CreditMultiplier=0x02, MaxTopics=0x03,
///            TimeLock=0x04, RequiresQuorum=0x05, Tag=0x06
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Constraint {
    /// Maximum messages per second.
    RateLimit(u32),
    /// Credit multiplier in basis points (10000 = 1.0x).
    CreditMultiplier(u32),
    /// Maximum gossip topic subscriptions.
    MaxTopics(u32),
    /// Unix timestamp before which the capability is locked.
    TimeLock(u64),
    /// Minimum quorum size required to exercise the capability.
    RequiresQuorum(u32),
    /// Opaque tag for domain-specific constraints.
    Tag(String),
}

// ---------------------------------------------------------------------------
// EdgeSource
// ---------------------------------------------------------------------------

/// The provenance of a capability edge -- what granted it.
///
/// Variant order is FROZEN. Do not reorder. Append only.
/// Tag bytes: CclContract=0x01, TrustScore=0x02, GovernanceVote=0x03, Static=0x04
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum EdgeSource {
    /// Granted by a CCL contract (contract ID).
    CclContract(String),
    /// Derived from a trust score computation (score label).
    TrustScore(String),
    /// Granted by a governance vote (proposal ID).
    GovernanceVote(String),
    /// Statically configured (description).
    Static(String),
}

// ---------------------------------------------------------------------------
// BlockHeight type alias
// ---------------------------------------------------------------------------

/// Block height for temporal validity. Alias for `u64`.
pub type BlockHeight = u64;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // -- SubjectId ----------------------------------------------------------

    #[test]
    fn subject_id_valid_did() {
        let sid = SubjectId::new("did:icn:abc123").unwrap();
        assert_eq!(sid.as_str(), "did:icn:abc123");
    }

    #[test]
    fn subject_id_rejects_non_did() {
        assert!(SubjectId::new("not-a-did").is_err());
    }

    #[test]
    fn subject_id_rejects_empty() {
        assert!(SubjectId::new("").is_err());
    }

    // -- Action -------------------------------------------------------------

    #[test]
    fn action_normalizes_uppercase() {
        let a = Action::new("LEDGER:TRANSFER").unwrap();
        assert_eq!(a.as_str(), "ledger:transfer");
        assert_eq!(a.domain(), "ledger");
        assert_eq!(a.verb(), "transfer");
    }

    #[test]
    fn action_trims_whitespace() {
        let a = Action::new("  gossip:subscribe  ").unwrap();
        assert_eq!(a.as_str(), "gossip:subscribe");
    }

    #[test]
    fn action_three_segments_ok() {
        let a = Action::new("ledger:transfer:credit").unwrap();
        assert_eq!(a.domain(), "ledger");
        assert_eq!(a.verb(), "transfer");
    }

    #[test]
    fn action_rejects_empty() {
        assert!(Action::new("").is_err());
    }

    #[test]
    fn action_rejects_no_colon() {
        assert!(Action::new("ledger").is_err());
    }

    #[test]
    fn action_rejects_empty_segment() {
        assert!(Action::new("ledger:").is_err());
        assert!(Action::new(":verb").is_err());
        assert!(Action::new("a::b").is_err());
    }

    #[test]
    fn action_rejects_non_ascii() {
        assert!(Action::new("ledger:tr\u{00e4}nsfer").is_err());
    }

    #[test]
    fn action_allows_hyphens() {
        let a = Action::new("my-domain:my-verb").unwrap();
        assert_eq!(a.as_str(), "my-domain:my-verb");
    }

    // -- ResourceKind ordering ----------------------------------------------

    #[test]
    fn resource_kind_ordering_is_declaration_order() {
        assert!(ResourceKind::Entity < ResourceKind::Scope);
        assert!(ResourceKind::Scope < ResourceKind::Asset);
        assert!(ResourceKind::Asset < ResourceKind::Contract);
        assert!(ResourceKind::Contract < ResourceKind::System);
    }

    // -- Constraint ordering ------------------------------------------------

    #[test]
    fn constraint_variant_ordering() {
        // Cross-variant: declaration order
        assert!(Constraint::RateLimit(100) < Constraint::CreditMultiplier(100));
        assert!(Constraint::CreditMultiplier(100) < Constraint::MaxTopics(100));
        assert!(Constraint::MaxTopics(100) < Constraint::TimeLock(100));
        assert!(Constraint::TimeLock(100) < Constraint::RequiresQuorum(100));
        assert!(Constraint::RequiresQuorum(100) < Constraint::Tag("a".into()));
    }

    #[test]
    fn constraint_same_variant_orders_by_value() {
        assert!(Constraint::RateLimit(10) < Constraint::RateLimit(20));
        assert!(Constraint::Tag("a".into()) < Constraint::Tag("b".into()));
    }

    // -- EdgeSource ordering ------------------------------------------------

    #[test]
    fn edge_source_ordering_is_declaration_order() {
        assert!(EdgeSource::CclContract("a".into()) < EdgeSource::TrustScore("a".into()));
        assert!(EdgeSource::TrustScore("a".into()) < EdgeSource::GovernanceVote("a".into()));
        assert!(EdgeSource::GovernanceVote("a".into()) < EdgeSource::Static("a".into()));
    }

    // -- Serde roundtrips ---------------------------------------------------

    #[test]
    fn subject_id_serde_roundtrip() {
        let sid = SubjectId::new("did:icn:abc").unwrap();
        let json = serde_json::to_string(&sid).unwrap();
        let back: SubjectId = serde_json::from_str(&json).unwrap();
        assert_eq!(sid, back);
    }

    #[test]
    fn action_serde_roundtrip() {
        let a = Action::new("ledger:transfer").unwrap();
        let json = serde_json::to_string(&a).unwrap();
        let back: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(a, back);
    }
}
