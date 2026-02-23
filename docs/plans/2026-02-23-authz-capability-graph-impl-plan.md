# Authz Capability Graph (Phase B0) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Scaffold the `icn-authz` crate with canonical model types, deterministic BLAKE3 hashing (length-prefixed, explicit tag bytes), `CapabilitySource` trait, `GraphBuilder`, and `CapabilityGraph` with query — the foundation for unifying ICN's four authorization systems.

**Architecture:** New app-layer crate `icn-authz` in `icn/crates/`. Domain-agnostic model in `icn-authz::model` (promotable to kernel later). Graph builder/query in `icn-authz::graph`. No persistence, no domain adapters (those are Phase B1). All hashing uses BLAKE3 with length-prefixed variable-length fields and explicit frozen tag byte constants.

**Tech Stack:** Rust (1.88.0), BLAKE3, serde, thiserror, icn-kernel-api (BlockHeight type alias = u64)

**Design doc:** `docs/plans/2026-02-23-authz-capability-graph-design.md`

---

## Landmines & Gotchas

1. **`BlockHeight` is `pub type BlockHeight = u64;`** (a type alias, NOT a newtype). The design doc says `height.0 as u64` — this is WRONG. Use `height` directly since it IS u64. Defined in `icn-kernel-api/src/invariants.rs:10`.

2. **`blake3` is NOT a workspace dependency.** Declare it directly: `blake3 = "1.8"` (same as `icn-governance`).

3. **`serde_json` is NOT a workspace dependency for dev-deps.** Use `serde_json = "1.0"` directly in `[dev-dependencies]`.

4. **Workspace members list is NOT alphabetical.** Insert `"crates/icn-authz"` after `"crates/icn-naming"` (line 36) to keep app-layer crates grouped near the end.

5. **Workspace lints say `unwrap_used = "warn"`.** Tests that call `.unwrap()` will emit warnings. Use `#[allow(clippy::unwrap_used)]` on test modules, or use `expect("reason")` (also warned but with context).

6. **No `icn-identity` dependency needed for B0.** `SubjectId` is just a validated String newtype — no DID resolution or crypto needed yet.

7. **`Ord` derive on enums** uses variant declaration order as discriminant. Our "frozen variant order" comments + explicit tag bytes in hashing mean the `Ord` derive is safe for sorting, but hashing MUST use the explicit constants, never rely on derived discriminants.

---

### Task 1: Scaffold crate and workspace registration

**Files:**
- Create: `icn/crates/icn-authz/Cargo.toml`
- Create: `icn/crates/icn-authz/src/lib.rs`
- Create: `icn/crates/icn-authz/src/error.rs`
- Modify: `icn/Cargo.toml:3-41` (workspace members)
- Modify: `icn/Cargo.toml:117-147` (workspace dependencies)

**Step 1: Create directory structure**

```bash
mkdir -p icn/crates/icn-authz/src/model
mkdir -p icn/crates/icn-authz/src/graph
```

**Step 2: Write `icn/crates/icn-authz/Cargo.toml`**

```toml
[package]
name = "icn-authz"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
blake3 = "1.8"
serde = { workspace = true, features = ["derive"] }
thiserror.workspace = true
icn-kernel-api = { path = "../icn-kernel-api" }

[dev-dependencies]
serde_json = "1.0"

[lints]
workspace = true
```

**Step 3: Write `icn/crates/icn-authz/src/error.rs`**

```rust
#[derive(Debug, thiserror::Error)]
pub enum AuthzError {
    #[error("invalid subject ID (must start with 'did:'): {0}")]
    InvalidSubjectId(String),

    #[error("invalid action format (expected 'domain:verb[:subverb]'): {0}")]
    InvalidAction(String),
}
```

**Step 4: Write `icn/crates/icn-authz/src/lib.rs`**

```rust
//! # ICN Authorization
//!
//! Canonical capability graph model for unifying ICN's authorization systems.
//!
//! ## Architecture
//!
//! - `model` — Domain-agnostic types: subjects, actions, resources, constraints, edges.
//!   Designed to be liftable to `icn-kernel-api` in a future phase.
//! - `graph` — Builder and query: `CapabilitySource` trait, `GraphBuilder`, `CapabilityGraph`.
//! - `error` — Error types for validation failures.
//!
//! ## Meaning Firewall
//!
//! This is an **app-layer** crate. It interprets domain semantics (what capabilities mean).
//! The kernel sees only hashes produced by this crate, never the capability model itself.

pub mod error;
pub mod graph;
pub mod model;

pub use error::AuthzError;
```

**Step 5: Write placeholder `icn/crates/icn-authz/src/model/mod.rs`**

```rust
pub mod hash;
pub mod ids;
pub mod edge;
```

**Step 6: Write placeholder `icn/crates/icn-authz/src/graph/mod.rs`**

```rust
pub mod builder;
```

**Step 7: Write placeholder `icn/crates/icn-authz/src/model/ids.rs`**

```rust
// Canonical identity and resource types — Task 2
```

**Step 8: Write placeholder `icn/crates/icn-authz/src/model/edge.rs`**

```rust
// CapabilityEdge and CapabilityGraph — Task 4
```

**Step 9: Write placeholder `icn/crates/icn-authz/src/model/hash.rs`**

```rust
// Deterministic BLAKE3 hashing — Task 3
```

**Step 10: Write placeholder `icn/crates/icn-authz/src/graph/builder.rs`**

```rust
// CapabilitySource trait and GraphBuilder — Task 5
```

**Step 11: Add to workspace members in `icn/Cargo.toml`**

In the `members` array, after `"crates/icn-naming",` (line 36), add:

```toml
    "crates/icn-authz",
```

**Step 12: Add to workspace dependencies in `icn/Cargo.toml`**

In the `[workspace.dependencies]` section, after `icn-naming = { path = "crates/icn-naming" }` (line 147), add:

```toml
icn-authz = { path = "crates/icn-authz" }
```

**Step 13: Verify the crate compiles**

Run: `cd icn/icn && cargo check -p icn-authz`
Expected: Compiles with 0 errors (warnings about empty files are OK)

**Step 14: Run clippy**

Run: `cd icn/icn && cargo clippy -p icn-authz --all-targets -- -D warnings`
Expected: PASS

**Step 15: Commit**

```bash
git add icn/crates/icn-authz/ icn/Cargo.toml
git commit -m "feat(authz): scaffold icn-authz crate with workspace registration"
```

---

### Task 2: Canonical identity types (SubjectId, Action, ResourceKind, ResourceId, Constraint, EdgeSource)

**Files:**
- Create: `icn/crates/icn-authz/src/model/ids.rs` (overwrite placeholder)

**Step 1: Write the failing tests**

Add the following test module at the bottom of `ids.rs`:

```rust
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // --- SubjectId ---

    #[test]
    fn subject_id_valid_did() {
        let s = SubjectId::new("did:icn:abc123").unwrap();
        assert_eq!(s.as_str(), "did:icn:abc123");
    }

    #[test]
    fn subject_id_rejects_non_did() {
        assert!(SubjectId::new("not-a-did").is_err());
    }

    #[test]
    fn subject_id_rejects_empty() {
        assert!(SubjectId::new("").is_err());
    }

    // --- Action ---

    #[test]
    fn action_normalizes_uppercase() {
        let a = Action::new("GOVERNANCE:PROPOSE").unwrap();
        assert_eq!(a.as_str(), "governance:propose");
    }

    #[test]
    fn action_trims_whitespace() {
        let a = Action::new("  ledger:transfer  ").unwrap();
        assert_eq!(a.as_str(), "ledger:transfer");
    }

    #[test]
    fn action_three_segments_ok() {
        let a = Action::new("ledger:transfer:credit").unwrap();
        assert_eq!(a.as_str(), "ledger:transfer:credit");
        assert_eq!(a.domain(), "ledger");
        assert_eq!(a.verb(), "transfer");
    }

    #[test]
    fn action_rejects_empty() {
        assert!(Action::new("").is_err());
    }

    #[test]
    fn action_rejects_no_colon() {
        assert!(Action::new("single").is_err());
    }

    #[test]
    fn action_rejects_empty_segment() {
        assert!(Action::new("a::b").is_err());
    }

    #[test]
    fn action_rejects_non_ascii() {
        assert!(Action::new("gov:propos\u{00e9}").is_err());
    }

    #[test]
    fn action_allows_hyphens() {
        let a = Action::new("compute:long-running").unwrap();
        assert_eq!(a.as_str(), "compute:long-running");
    }

    // --- ResourceKind ordering ---

    #[test]
    fn resource_kind_ordering_is_declaration_order() {
        assert!(ResourceKind::Entity < ResourceKind::Scope);
        assert!(ResourceKind::Scope < ResourceKind::Asset);
        assert!(ResourceKind::Asset < ResourceKind::Contract);
        assert!(ResourceKind::Contract < ResourceKind::System);
    }

    // --- Constraint ordering ---

    #[test]
    fn constraint_variants_ordered_by_discriminant() {
        let rate = Constraint::RateLimit(100);
        let credit = Constraint::CreditMultiplier(10000);
        let topics = Constraint::MaxTopics(50);
        let timelock = Constraint::TimeLock(1000);
        let quorum = Constraint::RequiresQuorum(3);
        let tag = Constraint::Tag("x".into());
        assert!(rate < credit);
        assert!(credit < topics);
        assert!(topics < timelock);
        assert!(timelock < quorum);
        assert!(quorum < tag);
    }

    #[test]
    fn constraint_same_variant_orders_by_value() {
        assert!(Constraint::RateLimit(10) < Constraint::RateLimit(20));
        assert!(Constraint::Tag("a".into()) < Constraint::Tag("b".into()));
    }

    // --- EdgeSource ordering ---

    #[test]
    fn edge_source_ordering_is_declaration_order() {
        assert!(EdgeSource::CclContract("a".into()) < EdgeSource::TrustScore("a".into()));
        assert!(EdgeSource::TrustScore("a".into()) < EdgeSource::GovernanceVote("a".into()));
        assert!(EdgeSource::GovernanceVote("a".into()) < EdgeSource::Static("a".into()));
    }

    // --- Serde round-trip ---

    #[test]
    fn subject_id_serde_roundtrip() {
        let s = SubjectId::new("did:icn:test").unwrap();
        let json = serde_json::to_string(&s).unwrap();
        let back: SubjectId = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn action_serde_roundtrip() {
        let a = Action::new("governance:propose").unwrap();
        let json = serde_json::to_string(&a).unwrap();
        let back: Action = serde_json::from_str(&json).unwrap();
        assert_eq!(a, back);
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cd icn/icn && cargo test -p icn-authz --lib`
Expected: FAIL — `SubjectId`, `Action`, etc. not defined yet

**Step 3: Write the implementation**

Write the full `ids.rs` with types + the test module from Step 1:

```rust
use serde::{Deserialize, Serialize};

use crate::AuthzError;

/// An actor in the capability graph. Always a DID.
///
/// Validated on construction: must start with "did:" prefix.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SubjectId(String);

impl SubjectId {
    pub fn new(did: impl Into<String>) -> Result<Self, AuthzError> {
        let s = did.into();
        if !s.starts_with("did:") {
            return Err(AuthzError::InvalidSubjectId(s));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A capability action. Lowercased, trimmed, colon-separated segments.
///
/// Format: `domain:verb[:subverb]`. Only ASCII alphanumeric, colon, and hyphen.
/// Soft normalization: lowercases and trims on construction.
///
/// Examples: "governance:propose", "ledger:transfer:credit", "compute:submit"
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Action(String);

impl Action {
    pub fn new(raw: impl Into<String>) -> Result<Self, AuthzError> {
        let s = raw.into().trim().to_ascii_lowercase();
        let segments: Vec<&str> = s.split(':').collect();
        if segments.len() < 2 || segments.iter().any(|seg| seg.is_empty()) {
            return Err(AuthzError::InvalidAction(s));
        }
        if !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == ':' || c == '-')
        {
            return Err(AuthzError::InvalidAction(s));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn domain(&self) -> &str {
        self.0.split(':').next().expect("validated in new()")
    }

    pub fn verb(&self) -> &str {
        self.0.splitn(3, ':').nth(1).expect("validated in new()")
    }
}

/// Variant order is FROZEN. Do not reorder. Append only.
/// Tag bytes: Entity=0x01, Scope=0x02, Asset=0x03, Contract=0x04, System=0x05
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ResourceKind {
    /// tag 0x01 — cooperatives, communities, federations, individuals
    Entity,
    /// tag 0x02 — cells, namespaces, governance scopes
    Scope,
    /// tag 0x03 — ledger accounts, credit lines, treasury
    Asset,
    /// tag 0x04 — CCL contracts, amendments
    Contract,
    /// tag 0x05 — protocol parameters, invariant config
    System,
}

/// A typed resource identifier.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResourceId {
    pub kind: ResourceKind,
    pub id: String,
}

impl ResourceId {
    pub fn new(kind: ResourceKind, id: impl Into<String>) -> Self {
        Self {
            kind,
            id: id.into(),
        }
    }
}

/// Variant order is FROZEN. Do not reorder. Append only.
/// Tag bytes: RateLimit=0x01, CreditMultiplier=0x02, MaxTopics=0x03,
///            TimeLock=0x04, RequiresQuorum=0x05, Tag=0x06
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Constraint {
    /// tag 0x01 — messages per second
    RateLimit(u32),
    /// tag 0x02 — basis points (10000 = 1.0x)
    CreditMultiplier(u32),
    /// tag 0x03 — topic subscription limit
    MaxTopics(u32),
    /// tag 0x04 — block height before which action is locked
    TimeLock(u64),
    /// tag 0x05 — minimum votes (absolute count)
    RequiresQuorum(u32),
    /// tag 0x06 — escape hatch for domain-specific labels
    Tag(String),
}

/// Variant order is FROZEN. Do not reorder. Append only.
/// Tag bytes: CclContract=0x01, TrustScore=0x02, GovernanceVote=0x03, Static=0x04
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum EdgeSource {
    /// tag 0x01 — contract hash or ID
    CclContract(String),
    /// tag 0x02 — trust graph identifier
    TrustScore(String),
    /// tag 0x03 — proposal ID
    GovernanceVote(String),
    /// tag 0x04 — hardcoded/bootstrap grants
    Static(String),
}

// --- tests at bottom (see Step 1) ---
```

**Step 4: Run tests to verify they pass**

Run: `cd icn/icn && cargo test -p icn-authz --lib`
Expected: 17 tests PASS

**Step 5: Run clippy**

Run: `cd icn/icn && cargo clippy -p icn-authz --all-targets -- -D warnings`
Expected: PASS

**Step 6: Commit**

```bash
git add icn/crates/icn-authz/src/model/ids.rs
git commit -m "feat(authz): canonical identity types with validation and ordering"
```

---

### Task 3: Deterministic hashing (tag constants, hash functions, length-prefixing)

**Files:**
- Create: `icn/crates/icn-authz/src/model/hash.rs` (overwrite placeholder)

**Step 1: Write the failing tests**

Add the following test module at the bottom of `hash.rs`:

```rust
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::ids::*;

    fn test_subject() -> SubjectId {
        SubjectId::new("did:icn:alice").unwrap()
    }

    fn test_action() -> Action {
        Action::new("governance:propose").unwrap()
    }

    fn test_resource() -> ResourceId {
        ResourceId::new(ResourceKind::Entity, "coop-1")
    }

    // --- Determinism ---

    #[test]
    fn hash_subject_deterministic() {
        let a = hash_subject(&test_subject());
        let b = hash_subject(&test_subject());
        assert_eq!(a, b);
    }

    #[test]
    fn hash_action_deterministic() {
        let a = hash_action(&test_action());
        let b = hash_action(&test_action());
        assert_eq!(a, b);
    }

    #[test]
    fn hash_resource_deterministic() {
        let a = hash_resource(&test_resource());
        let b = hash_resource(&test_resource());
        assert_eq!(a, b);
    }

    // --- Different inputs → different hashes ---

    #[test]
    fn hash_subject_differs_for_different_dids() {
        let a = hash_subject(&SubjectId::new("did:icn:alice").unwrap());
        let b = hash_subject(&SubjectId::new("did:icn:bob").unwrap());
        assert_ne!(a, b);
    }

    #[test]
    fn hash_action_differs_for_different_actions() {
        let a = hash_action(&Action::new("governance:propose").unwrap());
        let b = hash_action(&Action::new("governance:vote").unwrap());
        assert_ne!(a, b);
    }

    #[test]
    fn hash_resource_differs_for_different_kinds() {
        let a = hash_resource(&ResourceId::new(ResourceKind::Entity, "x"));
        let b = hash_resource(&ResourceId::new(ResourceKind::Asset, "x"));
        assert_ne!(a, b);
    }

    #[test]
    fn hash_resource_differs_for_different_ids() {
        let a = hash_resource(&ResourceId::new(ResourceKind::Entity, "x"));
        let b = hash_resource(&ResourceId::new(ResourceKind::Entity, "y"));
        assert_ne!(a, b);
    }

    // --- Constraint hashing ---

    #[test]
    fn hash_constraint_deterministic() {
        let c = Constraint::RateLimit(100);
        assert_eq!(hash_constraint(&c), hash_constraint(&c));
    }

    #[test]
    fn hash_constraint_differs_across_variants() {
        let a = hash_constraint(&Constraint::RateLimit(100));
        let b = hash_constraint(&Constraint::MaxTopics(100));
        assert_ne!(a, b);
    }

    #[test]
    fn hash_constraint_tag_differs_by_string() {
        let a = hash_constraint(&Constraint::Tag("foo".into()));
        let b = hash_constraint(&Constraint::Tag("bar".into()));
        assert_ne!(a, b);
    }

    // --- EdgeSource hashing ---

    #[test]
    fn hash_edge_source_deterministic() {
        let s = EdgeSource::Static("bootstrap".into());
        assert_eq!(hash_edge_source(&s), hash_edge_source(&s));
    }

    #[test]
    fn hash_edge_source_differs_across_variants() {
        let a = hash_edge_source(&EdgeSource::CclContract("x".into()));
        let b = hash_edge_source(&EdgeSource::Static("x".into()));
        assert_ne!(a, b);
    }

    // --- Option<BlockHeight> hashing ---

    #[test]
    fn hash_option_height_none_vs_some_differ() {
        let mut h1 = blake3::Hasher::new();
        hash_option_height(&mut h1, None);
        let mut h2 = blake3::Hasher::new();
        hash_option_height(&mut h2, Some(0));
        assert_ne!(h1.finalize().as_bytes(), h2.finalize().as_bytes());
    }

    #[test]
    fn hash_option_height_different_values_differ() {
        let mut h1 = blake3::Hasher::new();
        hash_option_height(&mut h1, Some(100));
        let mut h2 = blake3::Hasher::new();
        hash_option_height(&mut h2, Some(200));
        assert_ne!(h1.finalize().as_bytes(), h2.finalize().as_bytes());
    }

    // --- Length-prefix prevents ambiguity ---

    #[test]
    fn length_prefix_prevents_concatenation_ambiguity() {
        // "ab" ++ "cd" must differ from "a" ++ "bcd"
        let mut h1 = blake3::Hasher::new();
        hash_bytes(&mut h1, b"ab");
        hash_bytes(&mut h1, b"cd");

        let mut h2 = blake3::Hasher::new();
        hash_bytes(&mut h2, b"a");
        hash_bytes(&mut h2, b"bcd");

        assert_ne!(h1.finalize().as_bytes(), h2.finalize().as_bytes());
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cd icn/icn && cargo test -p icn-authz --lib`
Expected: FAIL — hash functions not defined

**Step 3: Write the implementation**

Write the full `hash.rs` with constants + functions + test module:

```rust
use icn_kernel_api::BlockHeight;

use crate::model::ids::*;

// ─── Top-level type tags ───────────────────────────────────────────
// These tag bytes are FROZEN. Do not change values. Append only.
pub(crate) const TAG_SUBJECT: u8 = 0x10;
pub(crate) const TAG_ACTION: u8 = 0x11;
pub(crate) const TAG_RESOURCE: u8 = 0x12;
pub(crate) const TAG_CONSTRAINT: u8 = 0x13;
pub(crate) const TAG_EDGE_SOURCE: u8 = 0x14;
pub(crate) const TAG_EDGE: u8 = 0x15;
pub(crate) const TAG_EDGE_SET: u8 = 0x16;

// ─── ResourceKind tag bytes ────────────────────────────────────────
// FROZEN. Do not reorder. Append only. Must match ResourceKind variant order.
pub(crate) const RESOURCE_ENTITY: u8 = 0x01;
pub(crate) const RESOURCE_SCOPE: u8 = 0x02;
pub(crate) const RESOURCE_ASSET: u8 = 0x03;
pub(crate) const RESOURCE_CONTRACT: u8 = 0x04;
pub(crate) const RESOURCE_SYSTEM: u8 = 0x05;

// ─── Constraint tag bytes ──────────────────────────────────────────
// FROZEN. Do not reorder. Append only. Must match Constraint variant order.
pub(crate) const CONSTRAINT_RATE_LIMIT: u8 = 0x01;
pub(crate) const CONSTRAINT_CREDIT_MULTIPLIER: u8 = 0x02;
pub(crate) const CONSTRAINT_MAX_TOPICS: u8 = 0x03;
pub(crate) const CONSTRAINT_TIME_LOCK: u8 = 0x04;
pub(crate) const CONSTRAINT_REQUIRES_QUORUM: u8 = 0x05;
pub(crate) const CONSTRAINT_TAG: u8 = 0x06;

// ─── EdgeSource tag bytes ──────────────────────────────────────────
// FROZEN. Do not reorder. Append only. Must match EdgeSource variant order.
pub(crate) const SOURCE_CCL_CONTRACT: u8 = 0x01;
pub(crate) const SOURCE_TRUST_SCORE: u8 = 0x02;
pub(crate) const SOURCE_GOVERNANCE_VOTE: u8 = 0x03;
pub(crate) const SOURCE_STATIC: u8 = 0x04;

// ─── Helpers ───────────────────────────────────────────────────────

/// Length-prefix then append bytes. Prevents concatenation ambiguity.
pub(crate) fn hash_bytes(hasher: &mut blake3::Hasher, data: &[u8]) {
    hasher.update(&(data.len() as u32).to_le_bytes());
    hasher.update(data);
}

/// Hash an Option<BlockHeight>. None = 0x00. Some(h) = 0x01 ++ h as u64 LE.
/// BlockHeight is a type alias for u64, so no .0 accessor needed.
pub(crate) fn hash_option_height(hasher: &mut blake3::Hasher, h: Option<BlockHeight>) {
    match h {
        None => hasher.update(&[0x00]),
        Some(height) => {
            hasher.update(&[0x01]);
            hasher.update(&height.to_le_bytes());
        }
    }
}

// ─── Per-type hash functions ───────────────────────────────────────

pub fn hash_subject(s: &SubjectId) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[TAG_SUBJECT]);
    hash_bytes(&mut hasher, s.as_str().as_bytes());
    *hasher.finalize().as_bytes()
}

pub fn hash_action(a: &Action) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[TAG_ACTION]);
    hash_bytes(&mut hasher, a.as_str().as_bytes());
    *hasher.finalize().as_bytes()
}

fn resource_kind_tag(kind: &ResourceKind) -> u8 {
    match kind {
        ResourceKind::Entity => RESOURCE_ENTITY,
        ResourceKind::Scope => RESOURCE_SCOPE,
        ResourceKind::Asset => RESOURCE_ASSET,
        ResourceKind::Contract => RESOURCE_CONTRACT,
        ResourceKind::System => RESOURCE_SYSTEM,
    }
}

pub fn hash_resource(r: &ResourceId) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[TAG_RESOURCE]);
    hasher.update(&[resource_kind_tag(&r.kind)]);
    hash_bytes(&mut hasher, r.id.as_bytes());
    *hasher.finalize().as_bytes()
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

pub fn hash_edge_source(es: &EdgeSource) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[TAG_EDGE_SOURCE]);
    hasher.update(&[edge_source_tag(es)]);
    hash_bytes(&mut hasher, edge_source_inner(es).as_bytes());
    *hasher.finalize().as_bytes()
}

/// Hash a sorted, deduped slice of edges into a single edge-set hash.
pub fn hash_edge_set(edges: &[crate::model::edge::CapabilityEdge]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[TAG_EDGE_SET]);
    hasher.update(&(edges.len() as u32).to_le_bytes());
    for edge in edges {
        hasher.update(&crate::model::edge::hash_edge(edge));
    }
    *hasher.finalize().as_bytes()
}

// --- tests at bottom (see Step 1) ---
```

**Step 4: Run tests to verify they pass**

Run: `cd icn/icn && cargo test -p icn-authz --lib`
Expected: All hash tests PASS (edge tests may fail if edge.rs is still placeholder — that's expected)

**Step 5: Run clippy**

Run: `cd icn/icn && cargo clippy -p icn-authz --all-targets -- -D warnings`
Expected: PASS (or warn about unused hash_edge_set if edge module is still placeholder — acceptable)

**Step 6: Commit**

```bash
git add icn/crates/icn-authz/src/model/hash.rs
git commit -m "feat(authz): deterministic BLAKE3 hashing with length-prefixed fields and explicit tags"
```

---

### Task 4: CapabilityEdge, CapabilityGraph, Decision, and edge hashing

**Files:**
- Create: `icn/crates/icn-authz/src/model/edge.rs` (overwrite placeholder)

**Step 1: Write the failing tests**

Add the following test module at the bottom of `edge.rs`:

```rust
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::ids::*;

    fn alice() -> SubjectId {
        SubjectId::new("did:icn:alice").unwrap()
    }

    fn bob() -> SubjectId {
        SubjectId::new("did:icn:bob").unwrap()
    }

    fn propose() -> Action {
        Action::new("governance:propose").unwrap()
    }

    fn vote() -> Action {
        Action::new("governance:vote").unwrap()
    }

    fn coop_resource() -> ResourceId {
        ResourceId::new(ResourceKind::Entity, "coop-1")
    }

    fn make_edge(subject: SubjectId, action: Action) -> CapabilityEdge {
        CapabilityEdge::new(
            subject,
            action,
            coop_resource(),
            vec![Constraint::RateLimit(10)],
            EdgeSource::Static("bootstrap".into()),
            None,
        )
    }

    // --- Constructor sorts and dedupes constraints ---

    #[test]
    fn constructor_sorts_constraints() {
        let edge = CapabilityEdge::new(
            alice(),
            propose(),
            coop_resource(),
            vec![
                Constraint::Tag("z".into()),
                Constraint::RateLimit(10),
                Constraint::MaxTopics(5),
            ],
            EdgeSource::Static("test".into()),
            None,
        );
        let kinds: Vec<&str> = edge
            .constraints
            .iter()
            .map(|c| match c {
                Constraint::RateLimit(_) => "RateLimit",
                Constraint::MaxTopics(_) => "MaxTopics",
                Constraint::Tag(_) => "Tag",
                _ => "other",
            })
            .collect();
        assert_eq!(kinds, vec!["RateLimit", "MaxTopics", "Tag"]);
    }

    #[test]
    fn constructor_dedupes_constraints() {
        let edge = CapabilityEdge::new(
            alice(),
            propose(),
            coop_resource(),
            vec![Constraint::RateLimit(10), Constraint::RateLimit(10)],
            EdgeSource::Static("test".into()),
            None,
        );
        assert_eq!(edge.constraints.len(), 1);
    }

    // --- Edge hashing ---

    #[test]
    fn hash_edge_deterministic() {
        let e = make_edge(alice(), propose());
        assert_eq!(hash_edge(&e), hash_edge(&e));
    }

    #[test]
    fn hash_edge_differs_by_subject() {
        let a = hash_edge(&make_edge(alice(), propose()));
        let b = hash_edge(&make_edge(bob(), propose()));
        assert_ne!(a, b);
    }

    #[test]
    fn hash_edge_differs_by_action() {
        let a = hash_edge(&make_edge(alice(), propose()));
        let b = hash_edge(&make_edge(alice(), vote()));
        assert_ne!(a, b);
    }

    #[test]
    fn hash_edge_differs_by_valid_at() {
        let mut e1 = make_edge(alice(), propose());
        e1.valid_at = None;
        let mut e2 = make_edge(alice(), propose());
        e2.valid_at = Some(100);
        assert_ne!(hash_edge(&e1), hash_edge(&e2));
    }

    // --- Graph hashing ---

    #[test]
    fn graph_hash_deterministic() {
        let edges = vec![make_edge(alice(), propose()), make_edge(bob(), vote())];
        let g = CapabilityGraph::from_edges(edges.clone());
        let g2 = CapabilityGraph::from_edges(edges);
        assert_eq!(g.hash(), g2.hash());
    }

    #[test]
    fn graph_hash_order_independent() {
        let e1 = make_edge(alice(), propose());
        let e2 = make_edge(bob(), vote());
        let g1 = CapabilityGraph::from_edges(vec![e1.clone(), e2.clone()]);
        let g2 = CapabilityGraph::from_edges(vec![e2, e1]);
        assert_eq!(g1.hash(), g2.hash());
    }

    #[test]
    fn graph_hash_changes_when_edge_added() {
        let e1 = make_edge(alice(), propose());
        let e2 = make_edge(bob(), vote());
        let g1 = CapabilityGraph::from_edges(vec![e1.clone()]);
        let g2 = CapabilityGraph::from_edges(vec![e1, e2]);
        assert_ne!(g1.hash(), g2.hash());
    }

    #[test]
    fn graph_hash_changes_when_constraint_changes() {
        let mut e1 = make_edge(alice(), propose());
        e1.constraints = vec![Constraint::RateLimit(10)];
        let mut e2 = make_edge(alice(), propose());
        e2.constraints = vec![Constraint::RateLimit(20)];
        // These are different edges so graphs differ
        let g1 = CapabilityGraph::from_edges(vec![e1]);
        let g2 = CapabilityGraph::from_edges(vec![e2]);
        assert_ne!(g1.hash(), g2.hash());
    }

    // --- Query ---

    #[test]
    fn query_allowed_when_edge_exists() {
        let g = CapabilityGraph::from_edges(vec![make_edge(alice(), propose())]);
        let d = g.query(&alice(), &propose(), &coop_resource());
        assert!(d.allowed);
        assert_eq!(d.matching_edges, vec![0]);
    }

    #[test]
    fn query_denied_when_no_edge() {
        let g = CapabilityGraph::from_edges(vec![make_edge(alice(), propose())]);
        let d = g.query(&bob(), &propose(), &coop_resource());
        assert!(!d.allowed);
        assert!(d.matching_edges.is_empty());
    }

    #[test]
    fn query_denied_on_empty_graph() {
        let g = CapabilityGraph::from_edges(vec![]);
        let d = g.query(&alice(), &propose(), &coop_resource());
        assert!(!d.allowed);
    }

    // --- Serde roundtrip ---

    #[test]
    fn edge_serde_roundtrip() {
        let e = make_edge(alice(), propose());
        let json = serde_json::to_string(&e).unwrap();
        let back: CapabilityEdge = serde_json::from_str(&json).unwrap();
        assert_eq!(e, back);
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cd icn/icn && cargo test -p icn-authz --lib`
Expected: FAIL — `CapabilityEdge`, `CapabilityGraph` not defined

**Step 3: Write the implementation**

Write the full `edge.rs`:

```rust
use icn_kernel_api::BlockHeight;
use serde::{Deserialize, Serialize};

use crate::model::hash::*;
use crate::model::ids::*;

/// A capability edge: "subject can do action on resource, with constraints, from source, at height."
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CapabilityEdge {
    pub subject: SubjectId,
    pub action: Action,
    pub resource: ResourceId,
    pub constraints: Vec<Constraint>,
    pub source: EdgeSource,
    pub valid_at: Option<BlockHeight>,
}

impl CapabilityEdge {
    /// Create an edge. Constraints are sorted and deduped automatically.
    pub fn new(
        subject: SubjectId,
        action: Action,
        resource: ResourceId,
        mut constraints: Vec<Constraint>,
        source: EdgeSource,
        valid_at: Option<BlockHeight>,
    ) -> Self {
        constraints.sort();
        constraints.dedup();
        Self {
            subject,
            action,
            resource,
            constraints,
            source,
            valid_at,
        }
    }
}

/// Hash a single edge field-by-field using BLAKE3.
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

/// B0 decision: allowed + matching edge indices. No string reasons.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub allowed: bool,
    pub matching_edges: Vec<usize>,
}

/// A computed capability graph: sorted, deduped edges.
#[derive(Clone, Debug)]
pub struct CapabilityGraph {
    edges: Vec<CapabilityEdge>,
}

impl CapabilityGraph {
    /// Create a graph from edges. Sorts and dedupes.
    pub fn from_edges(mut edges: Vec<CapabilityEdge>) -> Self {
        edges.sort();
        edges.dedup();
        Self { edges }
    }

    pub fn edges(&self) -> &[CapabilityEdge] {
        &self.edges
    }

    /// Deterministic hash of the entire edge set.
    pub fn hash(&self) -> [u8; 32] {
        hash_edge_set(&self.edges)
    }

    /// Query: can subject do action on resource?
    pub fn query(
        &self,
        subject: &SubjectId,
        action: &Action,
        resource: &ResourceId,
    ) -> Decision {
        let matching: Vec<usize> = self
            .edges
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.subject == *subject && e.action == *action && e.resource == *resource
            })
            .map(|(i, _)| i)
            .collect();

        Decision {
            allowed: !matching.is_empty(),
            matching_edges: matching,
        }
    }
}

// --- tests at bottom (see Step 1) ---
```

**Step 4: Run tests to verify they pass**

Run: `cd icn/icn && cargo test -p icn-authz --lib`
Expected: All tests PASS (hash tests from Task 3 + edge/graph tests from Task 4)

**Step 5: Run clippy**

Run: `cd icn/icn && cargo clippy -p icn-authz --all-targets -- -D warnings`
Expected: PASS

**Step 6: Commit**

```bash
git add icn/crates/icn-authz/src/model/edge.rs
git commit -m "feat(authz): CapabilityEdge, CapabilityGraph, Decision with deterministic hashing"
```

---

### Task 5: GraphBuilder and CapabilitySource trait

**Files:**
- Create: `icn/crates/icn-authz/src/graph/builder.rs` (overwrite placeholder)

**Step 1: Write the failing tests**

Add the following test module at the bottom of `builder.rs`:

```rust
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::model::edge::CapabilityGraph;
    use crate::model::ids::*;

    fn alice() -> SubjectId {
        SubjectId::new("did:icn:alice").unwrap()
    }

    fn bob() -> SubjectId {
        SubjectId::new("did:icn:bob").unwrap()
    }

    fn propose() -> Action {
        Action::new("governance:propose").unwrap()
    }

    fn coop_resource() -> ResourceId {
        ResourceId::new(ResourceKind::Entity, "coop-1")
    }

    fn make_edge(subject: SubjectId, action: Action) -> CapabilityEdge {
        CapabilityEdge::new(
            subject,
            action,
            coop_resource(),
            vec![Constraint::RateLimit(10)],
            EdgeSource::Static("bootstrap".into()),
            None,
        )
    }

    /// Mock source that returns a fixed set of edges.
    struct MockSource {
        edges: Vec<CapabilityEdge>,
    }

    impl CapabilitySource for MockSource {
        fn edges_for_subject(&self, subject: &SubjectId) -> Vec<CapabilityEdge> {
            self.edges
                .iter()
                .filter(|e| e.subject == *subject)
                .cloned()
                .collect()
        }

        fn all_edges(&self) -> Vec<CapabilityEdge> {
            self.edges.clone()
        }
    }

    #[test]
    fn empty_builder_produces_empty_graph() {
        let g = GraphBuilder::new().build();
        assert!(g.edges().is_empty());
        let d = g.query(&alice(), &propose(), &coop_resource());
        assert!(!d.allowed);
    }

    #[test]
    fn single_source_produces_edges() {
        let source = MockSource {
            edges: vec![make_edge(alice(), propose())],
        };
        let g = GraphBuilder::new()
            .add_source(Box::new(source))
            .build();
        assert_eq!(g.edges().len(), 1);
    }

    #[test]
    fn two_sources_with_overlapping_edges_deduped() {
        let edge = make_edge(alice(), propose());
        let s1 = MockSource {
            edges: vec![edge.clone()],
        };
        let s2 = MockSource {
            edges: vec![edge],
        };
        let g = GraphBuilder::new()
            .add_source(Box::new(s1))
            .add_source(Box::new(s2))
            .build();
        assert_eq!(g.edges().len(), 1);
    }

    #[test]
    fn two_sources_with_different_edges_merged() {
        let s1 = MockSource {
            edges: vec![make_edge(alice(), propose())],
        };
        let s2 = MockSource {
            edges: vec![make_edge(bob(), propose())],
        };
        let g = GraphBuilder::new()
            .add_source(Box::new(s1))
            .add_source(Box::new(s2))
            .build();
        assert_eq!(g.edges().len(), 2);
    }

    #[test]
    fn build_for_subject_returns_subset() {
        let s1 = MockSource {
            edges: vec![make_edge(alice(), propose()), make_edge(bob(), propose())],
        };
        let g = GraphBuilder::new()
            .add_source(Box::new(s1))
            .build_for_subject(&alice());
        assert_eq!(g.edges().len(), 1);
        assert_eq!(g.edges()[0].subject, alice());
    }

    #[test]
    fn build_same_edges_different_order_same_hash() {
        let e1 = make_edge(alice(), propose());
        let e2 = make_edge(bob(), propose());
        let s1 = MockSource {
            edges: vec![e1.clone(), e2.clone()],
        };
        let s2 = MockSource {
            edges: vec![e2, e1],
        };
        let g1 = GraphBuilder::new().add_source(Box::new(s1)).build();
        let g2 = GraphBuilder::new().add_source(Box::new(s2)).build();
        assert_eq!(g1.hash(), g2.hash());
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cd icn/icn && cargo test -p icn-authz --lib`
Expected: FAIL — `CapabilitySource`, `GraphBuilder` not defined

**Step 3: Write the implementation**

Write the full `builder.rs`:

```rust
use crate::model::edge::{CapabilityEdge, CapabilityGraph};
use crate::model::ids::SubjectId;

/// An adapter that produces capability edges from a domain-specific source.
///
/// Implementations live in `icn-authz::adapters` (Phase B1).
/// The trait is defined here so B0 can write tests with mock sources.
pub trait CapabilitySource: Send + Sync {
    /// Return all capability edges this source knows about for the given subject.
    fn edges_for_subject(&self, subject: &SubjectId) -> Vec<CapabilityEdge>;

    /// Return all capability edges this source can produce.
    fn all_edges(&self) -> Vec<CapabilityEdge>;
}

/// Constructs a CapabilityGraph from one or more CapabilitySource adapters.
pub struct GraphBuilder {
    sources: Vec<Box<dyn CapabilitySource>>,
}

impl GraphBuilder {
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
        }
    }

    pub fn add_source(mut self, source: Box<dyn CapabilitySource>) -> Self {
        self.sources.push(source);
        self
    }

    /// Build the graph by polling all sources and canonicalizing.
    pub fn build(&self) -> CapabilityGraph {
        let edges: Vec<CapabilityEdge> = self
            .sources
            .iter()
            .flat_map(|s| s.all_edges())
            .collect();
        CapabilityGraph::from_edges(edges)
    }

    /// Build a subject-scoped graph (cheaper than full build).
    pub fn build_for_subject(&self, subject: &SubjectId) -> CapabilityGraph {
        let edges: Vec<CapabilityEdge> = self
            .sources
            .iter()
            .flat_map(|s| s.edges_for_subject(subject))
            .collect();
        CapabilityGraph::from_edges(edges)
    }
}

impl Default for GraphBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// --- tests at bottom (see Step 1) ---
```

**Step 4: Run tests to verify they pass**

Run: `cd icn/icn && cargo test -p icn-authz --lib`
Expected: All tests PASS

**Step 5: Run clippy**

Run: `cd icn/icn && cargo clippy -p icn-authz --all-targets -- -D warnings`
Expected: PASS

**Step 6: Commit**

```bash
git add icn/crates/icn-authz/src/graph/builder.rs
git commit -m "feat(authz): CapabilitySource trait and GraphBuilder with dedup and canonicalization"
```

---

### Task 6: Wire up re-exports in lib.rs and model/mod.rs

**Files:**
- Modify: `icn/crates/icn-authz/src/lib.rs`
- Modify: `icn/crates/icn-authz/src/model/mod.rs`
- Modify: `icn/crates/icn-authz/src/graph/mod.rs`

**Step 1: Update `model/mod.rs` with re-exports**

```rust
pub mod edge;
pub mod hash;
pub mod ids;

pub use edge::{CapabilityEdge, CapabilityGraph, Decision};
pub use ids::{Action, Constraint, EdgeSource, ResourceId, ResourceKind, SubjectId};
```

**Step 2: Update `graph/mod.rs` with re-exports**

```rust
pub mod builder;

pub use builder::{CapabilitySource, GraphBuilder};
```

**Step 3: Update `lib.rs` with re-exports**

```rust
//! # ICN Authorization
//!
//! Canonical capability graph model for unifying ICN's authorization systems.
//!
//! ## Architecture
//!
//! - `model` — Domain-agnostic types: subjects, actions, resources, constraints, edges.
//!   Designed to be liftable to `icn-kernel-api` in a future phase.
//! - `graph` — Builder and query: `CapabilitySource` trait, `GraphBuilder`, `CapabilityGraph`.
//! - `error` — Error types for validation failures.
//!
//! ## Meaning Firewall
//!
//! This is an **app-layer** crate. It interprets domain semantics (what capabilities mean).
//! The kernel sees only hashes produced by this crate, never the capability model itself.

pub mod error;
pub mod graph;
pub mod model;

pub use error::AuthzError;
pub use graph::{CapabilitySource, GraphBuilder};
pub use model::{
    Action, CapabilityEdge, CapabilityGraph, Constraint, Decision, EdgeSource, ResourceId,
    ResourceKind, SubjectId,
};
```

**Step 4: Run full test suite**

Run: `cd icn/icn && cargo test -p icn-authz --lib`
Expected: All tests PASS

**Step 5: Run clippy**

Run: `cd icn/icn && cargo clippy -p icn-authz --all-targets -- -D warnings`
Expected: PASS

**Step 6: Run fmt**

Run: `cd icn/icn && cargo fmt -p icn-authz --check`
Expected: PASS

**Step 7: Commit**

```bash
git add icn/crates/icn-authz/src/lib.rs icn/crates/icn-authz/src/model/mod.rs icn/crates/icn-authz/src/graph/mod.rs
git commit -m "feat(authz): wire up re-exports for public API surface"
```

---

### Task 7: Integration tests (ordering invariants, hash determinism across boundaries)

**Files:**
- Create: `icn/crates/icn-authz/tests/capability_graph_integration.rs`

**Step 1: Write integration tests**

```rust
//! Integration tests for the icn-authz capability graph.
//!
//! These test cross-module properties: ordering invariants hold after
//! serialization round-trips, hash determinism across graph boundaries,
//! and the full build→query→hash pipeline.

#[allow(clippy::unwrap_used)]
mod capability_graph_integration {
    use icn_authz::*;

    fn alice() -> SubjectId {
        SubjectId::new("did:icn:alice").unwrap()
    }

    fn bob() -> SubjectId {
        SubjectId::new("did:icn:bob").unwrap()
    }

    fn carol() -> SubjectId {
        SubjectId::new("did:icn:carol").unwrap()
    }

    fn propose() -> Action {
        Action::new("governance:propose").unwrap()
    }

    fn vote() -> Action {
        Action::new("governance:vote").unwrap()
    }

    fn transfer() -> Action {
        Action::new("ledger:transfer").unwrap()
    }

    fn coop() -> ResourceId {
        ResourceId::new(ResourceKind::Entity, "coop-1")
    }

    fn make_edge(
        subject: SubjectId,
        action: Action,
        resource: ResourceId,
        source: EdgeSource,
    ) -> CapabilityEdge {
        CapabilityEdge::new(
            subject,
            action,
            resource,
            vec![Constraint::RateLimit(10)],
            source,
            None,
        )
    }

    struct FixedSource(Vec<CapabilityEdge>);

    impl CapabilitySource for FixedSource {
        fn edges_for_subject(&self, subject: &SubjectId) -> Vec<CapabilityEdge> {
            self.0
                .iter()
                .filter(|e| e.subject == *subject)
                .cloned()
                .collect()
        }
        fn all_edges(&self) -> Vec<CapabilityEdge> {
            self.0.clone()
        }
    }

    // --- Ordering invariant: sort survives serde round-trip ---

    #[test]
    fn edges_sorted_after_serde_roundtrip() {
        let edges = vec![
            make_edge(carol(), vote(), coop(), EdgeSource::Static("s".into())),
            make_edge(alice(), propose(), coop(), EdgeSource::Static("s".into())),
            make_edge(bob(), transfer(), coop(), EdgeSource::Static("s".into())),
        ];
        let g = CapabilityGraph::from_edges(edges);

        let json = serde_json::to_string(g.edges()).unwrap();
        let deserialized: Vec<CapabilityEdge> = serde_json::from_str(&json).unwrap();
        let g2 = CapabilityGraph::from_edges(deserialized);

        assert_eq!(g.hash(), g2.hash());
    }

    // --- Full pipeline: two sources → build → query → hash ---

    #[test]
    fn full_pipeline_two_sources() {
        let s1 = FixedSource(vec![
            make_edge(
                alice(),
                propose(),
                coop(),
                EdgeSource::CclContract("contract-1".into()),
            ),
            make_edge(
                bob(),
                vote(),
                coop(),
                EdgeSource::TrustScore("trust-main".into()),
            ),
        ]);
        let s2 = FixedSource(vec![make_edge(
            carol(),
            transfer(),
            ResourceId::new(ResourceKind::Asset, "treasury"),
            EdgeSource::GovernanceVote("prop-42".into()),
        )]);

        let g = GraphBuilder::new()
            .add_source(Box::new(s1))
            .add_source(Box::new(s2))
            .build();

        assert_eq!(g.edges().len(), 3);

        // Alice can propose
        let d = g.query(&alice(), &propose(), &coop());
        assert!(d.allowed);

        // Bob can vote
        let d = g.query(&bob(), &vote(), &coop());
        assert!(d.allowed);

        // Carol can transfer on treasury
        let d = g.query(
            &carol(),
            &transfer(),
            &ResourceId::new(ResourceKind::Asset, "treasury"),
        );
        assert!(d.allowed);

        // Alice cannot vote
        let d = g.query(&alice(), &vote(), &coop());
        assert!(!d.allowed);
    }

    // --- Hash stability: same edges from different source configs → same hash ---

    #[test]
    fn hash_stable_across_source_configurations() {
        let e1 = make_edge(alice(), propose(), coop(), EdgeSource::Static("s".into()));
        let e2 = make_edge(bob(), vote(), coop(), EdgeSource::Static("s".into()));

        // Config A: one source with both edges
        let g1 = GraphBuilder::new()
            .add_source(Box::new(FixedSource(vec![e1.clone(), e2.clone()])))
            .build();

        // Config B: two sources, one edge each
        let g2 = GraphBuilder::new()
            .add_source(Box::new(FixedSource(vec![e1])))
            .add_source(Box::new(FixedSource(vec![e2])))
            .build();

        assert_eq!(g1.hash(), g2.hash());
    }

    // --- Subject-scoped build produces correct subset ---

    #[test]
    fn build_for_subject_correct_subset() {
        let source = FixedSource(vec![
            make_edge(alice(), propose(), coop(), EdgeSource::Static("s".into())),
            make_edge(alice(), vote(), coop(), EdgeSource::Static("s".into())),
            make_edge(bob(), propose(), coop(), EdgeSource::Static("s".into())),
        ]);

        let g = GraphBuilder::new()
            .add_source(Box::new(source))
            .build_for_subject(&alice());

        assert_eq!(g.edges().len(), 2);
        assert!(g.edges().iter().all(|e| e.subject == alice()));
    }

    // --- Multiple matching edges for same subject+action+resource ---

    #[test]
    fn query_returns_multiple_matching_edges() {
        let e1 = CapabilityEdge::new(
            alice(),
            propose(),
            coop(),
            vec![Constraint::RateLimit(10)],
            EdgeSource::CclContract("c1".into()),
            None,
        );
        let e2 = CapabilityEdge::new(
            alice(),
            propose(),
            coop(),
            vec![Constraint::RateLimit(20)],
            EdgeSource::TrustScore("t1".into()),
            None,
        );
        let g = CapabilityGraph::from_edges(vec![e1, e2]);
        let d = g.query(&alice(), &propose(), &coop());
        assert!(d.allowed);
        assert_eq!(d.matching_edges.len(), 2);
    }
}
```

**Step 2: Run integration tests**

Run: `cd icn/icn && cargo test -p icn-authz --test capability_graph_integration`
Expected: All 6 tests PASS

**Step 3: Run full crate test suite one final time**

Run: `cd icn/icn && cargo test -p icn-authz`
Expected: All unit + integration tests PASS

**Step 4: Run fmt + clippy on the entire crate**

Run: `cd icn/icn && cargo fmt -p icn-authz --check && cargo clippy -p icn-authz --all-targets -- -D warnings`
Expected: PASS

**Step 5: Commit**

```bash
git add icn/crates/icn-authz/tests/
git commit -m "test(authz): integration tests for ordering, hash determinism, and full pipeline"
```

---

## Final Verification

After all 7 tasks, run:

```bash
cd icn/icn && cargo fmt --all --check
cd icn/icn && cargo clippy -p icn-authz --all-targets -- -D warnings
cd icn/icn && cargo test -p icn-authz
```

Expected: fmt clean, clippy clean, all tests pass.

**Test count target:** ~50 tests total (17 ids + 15 hash + 13 edge/graph + 6 builder + 6 integration = ~57).
