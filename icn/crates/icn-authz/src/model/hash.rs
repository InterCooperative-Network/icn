//! Deterministic BLAKE3 hashing for capability graph types.
//!
//! Every type gets a unique tag byte prefix to prevent cross-type collisions.
//! Strings are length-prefixed to prevent concatenation ambiguity.
//! All tag bytes are FROZEN -- append only, never change values.

use super::edge::CapabilityEdge;
use super::ids::{
    Action, BlockHeight, Constraint, EdgeSource, ResourceId, ResourceKind, SubjectId,
};

// ---------------------------------------------------------------------------
// Top-level type tags -- FROZEN
// ---------------------------------------------------------------------------

pub(crate) const TAG_SUBJECT: u8 = 0x10;
pub(crate) const TAG_ACTION: u8 = 0x11;
pub(crate) const TAG_RESOURCE: u8 = 0x12;
pub(crate) const TAG_CONSTRAINT: u8 = 0x13;
pub(crate) const TAG_EDGE_SOURCE: u8 = 0x14;
pub(crate) const TAG_EDGE: u8 = 0x15;
pub(crate) const TAG_EDGE_SET: u8 = 0x16;

// ---------------------------------------------------------------------------
// ResourceKind tags -- FROZEN, must match variant declaration order
// ---------------------------------------------------------------------------

pub(crate) const RESOURCE_ENTITY: u8 = 0x01;
pub(crate) const RESOURCE_SCOPE: u8 = 0x02;
pub(crate) const RESOURCE_ASSET: u8 = 0x03;
pub(crate) const RESOURCE_CONTRACT: u8 = 0x04;
pub(crate) const RESOURCE_SYSTEM: u8 = 0x05;

// ---------------------------------------------------------------------------
// Constraint tags -- FROZEN
// ---------------------------------------------------------------------------

pub(crate) const CONSTRAINT_RATE_LIMIT: u8 = 0x01;
pub(crate) const CONSTRAINT_CREDIT_MULTIPLIER: u8 = 0x02;
pub(crate) const CONSTRAINT_MAX_TOPICS: u8 = 0x03;
pub(crate) const CONSTRAINT_TIME_LOCK: u8 = 0x04;
pub(crate) const CONSTRAINT_REQUIRES_QUORUM: u8 = 0x05;
pub(crate) const CONSTRAINT_TAG: u8 = 0x06;

// ---------------------------------------------------------------------------
// EdgeSource tags -- FROZEN
// ---------------------------------------------------------------------------

pub(crate) const SOURCE_CCL_CONTRACT: u8 = 0x01;
pub(crate) const SOURCE_TRUST_SCORE: u8 = 0x02;
pub(crate) const SOURCE_GOVERNANCE_VOTE: u8 = 0x03;
pub(crate) const SOURCE_STATIC: u8 = 0x04;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Length-prefix then append bytes. Prevents concatenation ambiguity.
pub(crate) fn hash_bytes(hasher: &mut blake3::Hasher, data: &[u8]) {
    hasher.update(&(data.len() as u32).to_le_bytes());
    hasher.update(data);
}

/// Hash an `Option<BlockHeight>`. `None` = `0x00`. `Some(h)` = `0x01 ++ h as u64 LE`.
pub(crate) fn hash_option_height(hasher: &mut blake3::Hasher, h: Option<BlockHeight>) {
    match h {
        None => {
            hasher.update(&[0x00]);
        }
        Some(height) => {
            hasher.update(&[0x01]);
            hasher.update(&height.to_le_bytes());
        }
    }
}

// ---------------------------------------------------------------------------
// Private tag mappers -- explicit match arms, no casting
// ---------------------------------------------------------------------------

fn resource_kind_tag(kind: &ResourceKind) -> u8 {
    match kind {
        ResourceKind::Entity => RESOURCE_ENTITY,
        ResourceKind::Scope => RESOURCE_SCOPE,
        ResourceKind::Asset => RESOURCE_ASSET,
        ResourceKind::Contract => RESOURCE_CONTRACT,
        ResourceKind::System => RESOURCE_SYSTEM,
    }
}

fn constraint_tag(c: &Constraint) -> u8 {
    match c {
        Constraint::RateLimit(_) => CONSTRAINT_RATE_LIMIT,
        Constraint::CreditMultiplier(_) => CONSTRAINT_CREDIT_MULTIPLIER,
        Constraint::MaxTopics(_) => CONSTRAINT_MAX_TOPICS,
        Constraint::TimeLock(_) => CONSTRAINT_TIME_LOCK,
        Constraint::RequiresQuorum(_) => CONSTRAINT_REQUIRES_QUORUM,
        Constraint::Tag(_) => CONSTRAINT_TAG,
    }
}

fn edge_source_tag(es: &EdgeSource) -> u8 {
    match es {
        EdgeSource::CclContract(_) => SOURCE_CCL_CONTRACT,
        EdgeSource::TrustScore(_) => SOURCE_TRUST_SCORE,
        EdgeSource::GovernanceVote(_) => SOURCE_GOVERNANCE_VOTE,
        EdgeSource::Static(_) => SOURCE_STATIC,
    }
}

fn edge_source_inner(es: &EdgeSource) -> &str {
    match es {
        EdgeSource::CclContract(s)
        | EdgeSource::TrustScore(s)
        | EdgeSource::GovernanceVote(s)
        | EdgeSource::Static(s) => s,
    }
}

// ---------------------------------------------------------------------------
// Per-type hash functions
// ---------------------------------------------------------------------------

/// Deterministic BLAKE3 hash of a [`SubjectId`].
pub fn hash_subject(s: &SubjectId) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[TAG_SUBJECT]);
    hash_bytes(&mut hasher, s.as_str().as_bytes());
    *hasher.finalize().as_bytes()
}

/// Deterministic BLAKE3 hash of an [`Action`].
pub fn hash_action(a: &Action) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[TAG_ACTION]);
    hash_bytes(&mut hasher, a.as_str().as_bytes());
    *hasher.finalize().as_bytes()
}

/// Deterministic BLAKE3 hash of a [`ResourceId`].
pub fn hash_resource(r: &ResourceId) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[TAG_RESOURCE]);
    hasher.update(&[resource_kind_tag(&r.kind)]);
    hash_bytes(&mut hasher, r.id.as_bytes());
    *hasher.finalize().as_bytes()
}

/// Deterministic BLAKE3 hash of a [`Constraint`].
pub fn hash_constraint(c: &Constraint) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[TAG_CONSTRAINT]);
    hasher.update(&[constraint_tag(c)]);
    match c {
        Constraint::RateLimit(v) => hasher.update(&v.to_le_bytes()),
        Constraint::CreditMultiplier(v) => hasher.update(&v.to_le_bytes()),
        Constraint::MaxTopics(v) => hasher.update(&v.to_le_bytes()),
        Constraint::TimeLock(v) => hasher.update(&v.to_le_bytes()),
        Constraint::RequiresQuorum(v) => hasher.update(&v.to_le_bytes()),
        Constraint::Tag(s) => {
            hash_bytes(&mut hasher, s.as_bytes());
            return *hasher.finalize().as_bytes();
        }
    };
    *hasher.finalize().as_bytes()
}

/// Deterministic BLAKE3 hash of an [`EdgeSource`].
pub fn hash_edge_source(es: &EdgeSource) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[TAG_EDGE_SOURCE]);
    hasher.update(&[edge_source_tag(es)]);
    hash_bytes(&mut hasher, edge_source_inner(es).as_bytes());
    *hasher.finalize().as_bytes()
}

/// Deterministic BLAKE3 hash of a [`CapabilityEdge`].
pub fn hash_edge(e: &CapabilityEdge) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[TAG_EDGE]);
    hasher.update(&hash_subject(&e.subject));
    hasher.update(&hash_action(&e.action));
    hasher.update(&hash_resource(&e.resource));
    hasher.update(&(e.constraints.len() as u32).to_le_bytes());
    for c in &e.constraints {
        hasher.update(&hash_constraint(c));
    }
    hasher.update(&hash_edge_source(&e.source));
    hash_option_height(&mut hasher, e.valid_at);
    *hasher.finalize().as_bytes()
}

/// Deterministic BLAKE3 hash of an edge set (used by [`CapabilityGraph::hash`]).
pub fn hash_edge_set(edges: &[CapabilityEdge]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[TAG_EDGE_SET]);
    hasher.update(&(edges.len() as u32).to_le_bytes());
    for e in edges {
        hasher.update(&hash_edge(e));
    }
    *hasher.finalize().as_bytes()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn alice() -> SubjectId {
        SubjectId::new("did:icn:alice").unwrap()
    }

    fn bob() -> SubjectId {
        SubjectId::new("did:icn:bob").unwrap()
    }

    fn transfer() -> Action {
        Action::new("ledger:transfer").unwrap()
    }

    fn propose() -> Action {
        Action::new("governance:propose").unwrap()
    }

    fn vote() -> Action {
        Action::new("governance:vote").unwrap()
    }

    fn asset_x() -> ResourceId {
        ResourceId::new(ResourceKind::Asset, "x")
    }

    fn entity_x() -> ResourceId {
        ResourceId::new(ResourceKind::Entity, "x")
    }

    fn entity_y() -> ResourceId {
        ResourceId::new(ResourceKind::Entity, "y")
    }

    fn make_edge(
        subject: SubjectId,
        action: Action,
        valid_at: Option<BlockHeight>,
    ) -> CapabilityEdge {
        CapabilityEdge::new(
            subject,
            action,
            asset_x(),
            vec![Constraint::RateLimit(100)],
            EdgeSource::Static("test".into()),
            valid_at,
        )
    }

    // -- SubjectId hashing --------------------------------------------------

    #[test]
    fn hash_subject_deterministic() {
        let h1 = hash_subject(&alice());
        let h2 = hash_subject(&alice());
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_subject_differs_for_different_dids() {
        assert_ne!(hash_subject(&alice()), hash_subject(&bob()));
    }

    // -- Action hashing -----------------------------------------------------

    #[test]
    fn hash_action_deterministic() {
        let h1 = hash_action(&transfer());
        let h2 = hash_action(&transfer());
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_action_differs_for_different_actions() {
        assert_ne!(hash_action(&propose()), hash_action(&vote()));
    }

    // -- Resource hashing ---------------------------------------------------

    #[test]
    fn hash_resource_deterministic() {
        let h1 = hash_resource(&asset_x());
        let h2 = hash_resource(&asset_x());
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_resource_differs_for_different_kinds() {
        assert_ne!(hash_resource(&entity_x()), hash_resource(&asset_x()));
    }

    #[test]
    fn hash_resource_differs_for_different_ids() {
        assert_ne!(hash_resource(&entity_x()), hash_resource(&entity_y()));
    }

    // -- Constraint hashing -------------------------------------------------

    #[test]
    fn hash_constraint_deterministic() {
        let h1 = hash_constraint(&Constraint::RateLimit(100));
        let h2 = hash_constraint(&Constraint::RateLimit(100));
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_constraint_differs_across_variants() {
        // Same inner value (100), different variant tags.
        assert_ne!(
            hash_constraint(&Constraint::RateLimit(100)),
            hash_constraint(&Constraint::MaxTopics(100)),
        );
    }

    #[test]
    fn hash_constraint_tag_differs_by_string() {
        assert_ne!(
            hash_constraint(&Constraint::Tag("foo".into())),
            hash_constraint(&Constraint::Tag("bar".into())),
        );
    }

    // -- EdgeSource hashing -------------------------------------------------

    #[test]
    fn hash_edge_source_deterministic() {
        let es = EdgeSource::CclContract("contract-1".into());
        let h1 = hash_edge_source(&es);
        let h2 = hash_edge_source(&es);
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_edge_source_differs_across_variants() {
        // Same inner string, different variant tags.
        assert_ne!(
            hash_edge_source(&EdgeSource::CclContract("x".into())),
            hash_edge_source(&EdgeSource::Static("x".into())),
        );
    }

    // -- Option<BlockHeight> hashing ----------------------------------------

    #[test]
    fn hash_option_height_none_vs_some_differ() {
        let mut h_none = blake3::Hasher::new();
        hash_option_height(&mut h_none, None);
        let digest_none = *h_none.finalize().as_bytes();

        let mut h_some = blake3::Hasher::new();
        hash_option_height(&mut h_some, Some(100));
        let digest_some = *h_some.finalize().as_bytes();

        assert_ne!(digest_none, digest_some);
    }

    #[test]
    fn hash_option_height_different_values_differ() {
        let mut h1 = blake3::Hasher::new();
        hash_option_height(&mut h1, Some(100));
        let d1 = *h1.finalize().as_bytes();

        let mut h2 = blake3::Hasher::new();
        hash_option_height(&mut h2, Some(200));
        let d2 = *h2.finalize().as_bytes();

        assert_ne!(d1, d2);
    }

    // -- Length-prefix ambiguity --------------------------------------------

    #[test]
    fn length_prefix_prevents_concatenation_ambiguity() {
        // ("ab", "cd") vs ("a", "bcd") must differ when length-prefixed.
        let mut h1 = blake3::Hasher::new();
        hash_bytes(&mut h1, b"ab");
        hash_bytes(&mut h1, b"cd");
        let d1 = *h1.finalize().as_bytes();

        let mut h2 = blake3::Hasher::new();
        hash_bytes(&mut h2, b"a");
        hash_bytes(&mut h2, b"bcd");
        let d2 = *h2.finalize().as_bytes();

        assert_ne!(d1, d2);
    }

    // -- Edge hashing -------------------------------------------------------

    #[test]
    fn hash_edge_deterministic() {
        let e = make_edge(alice(), transfer(), None);
        assert_eq!(hash_edge(&e), hash_edge(&e));
    }

    #[test]
    fn hash_edge_differs_by_subject() {
        let e1 = make_edge(alice(), transfer(), None);
        let e2 = make_edge(bob(), transfer(), None);
        assert_ne!(hash_edge(&e1), hash_edge(&e2));
    }

    #[test]
    fn hash_edge_differs_by_action() {
        let e1 = make_edge(alice(), propose(), None);
        let e2 = make_edge(alice(), vote(), None);
        assert_ne!(hash_edge(&e1), hash_edge(&e2));
    }

    #[test]
    fn hash_edge_differs_by_valid_at() {
        let e1 = make_edge(alice(), transfer(), None);
        let e2 = make_edge(alice(), transfer(), Some(100));
        assert_ne!(hash_edge(&e1), hash_edge(&e2));
    }
}
