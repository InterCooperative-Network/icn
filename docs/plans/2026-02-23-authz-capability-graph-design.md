# Authz Capability Graph Design (Phase B)

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Unify ICN's four disconnected authorization systems into a single canonical capability graph model with deterministic hashing, computed-on-demand evaluation, and adapter traits.

**Architecture:** App-layer crate (`icn-authz`) with a domain-agnostic model subset (`icn-authz::model`) designed to be liftable to `icn-kernel-api` in a future phase. No persistence — graph is computed on demand from adapter sources. Deterministic hashing uses BLAKE3 with length-prefixed fields and explicit tag bytes.

**Tech Stack:** Rust, BLAKE3, `icn-kernel-api` (types only), `icn-identity` (DID types)

---

## 1. Motivation

ICN currently has four independent authorization systems that don't talk to each other:

1. **CCL Capabilities** (`icn-ccl/src/types.rs`): 12-variant `Capability` enum. The governance-related variants (`ProposeAmendment`, `VoteOnProposal`, etc.) are dead code — governance doesn't actually check them.

2. **Trust Classes** (`icn-trust/src/types.rs`): `TrustGraphType` enum maps to hardcoded rate limits. Trust score → rate limit conversion is buried in `TrustPolicyOracle` with no way for governance or CCL to query it.

3. **Kernel ConstraintSet** (`icn-kernel-api/src/authz.rs`): `HashMap<String, ConstraintValue>` custom bag. Powerful but untyped — anyone can stuff anything in, no schema, no auditing.

4. **JWT Scopes** (`icn-gateway/src/auth.rs`): `scopes: Vec<String>` in `TokenClaims`. Static strings like `"governance:read"`, completely disconnected from CCL capabilities.

**Problems this causes:**
- An amendment can grant `ProposeAmendment` capability in CCL but governance never checks it
- Trust-gated rate limits are invisible to the governance safety pipeline (Phase A)
- No way to compute "what changed" across a governance action that touches multiple systems
- PowerDiff (Phase A) can detect *that* power changed but not *what capabilities* changed

## 2. Goals and Non-Goals

### Goals (B0 scope)
- Canonical types for subjects, actions, resources, constraints, and edges
- Deterministic BLAKE3 hashing of capability edges and edge sets
- `GraphBuilder` trait for constructing graphs from heterogeneous sources
- `CapabilityQuery` trait for evaluating "can subject X do action Y on resource Z?"
- Test suite proving ordering invariants and hash determinism

### Non-Goals (deferred)
- Persistence (no sled, no storage)
- Live adapters for CCL/Trust/JWT (Phase B1)
- PowerDiff integration (Phase B2)
- Full pipeline wiring (Phase B3)
- Kernel promotion (future phase, post-pilot)

## 3. Boundary and Meaning Firewall Stance

`icn-authz` is an **app-layer crate**. It lives alongside `icn-governance`, `icn-trust`, etc.

**Why not kernel?** The capability graph interprets domain semantics — it knows what "governance:propose" means, what trust classes map to. This is meaning. The kernel must not understand meaning.

**Promotion path:** The `icn-authz::model` module contains only domain-agnostic types (`SubjectId`, `Action`, `ResourceId`, `Constraint`, `CapabilityEdge`). These types are designed to be copy-pasted into `icn-kernel-api` when the time comes. The `icn-authz::adapters` module imports domain crates and will never move to kernel.

**Dependency direction:**
```
icn-authz::model     → icn-kernel-api (types only: BlockHeight, InvariantId)
icn-authz::adapters  → icn-ccl, icn-trust, icn-gateway (future, B1)
icn-authz::graph     → icn-authz::model (only)
```

## 4. Canonical Model

### 4.1 SubjectId

```rust
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
    pub fn as_str(&self) -> &str { &self.0 }
}
```

**Hashing:** `hash_subject(s)` = BLAKE3(TAG_SUBJECT ++ len_u32_le(s.as_bytes()) ++ s.as_bytes())

### 4.2 Action

Validated canonical string newtype. Format: `domain:verb[:subverb]`.

```rust
/// A capability action. Lowercased, trimmed, colon-separated segments.
///
/// Examples: "governance:propose", "ledger:transfer:credit", "compute:submit"
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Action(String);

impl Action {
    pub fn new(raw: impl Into<String>) -> Result<Self, AuthzError> {
        let s = raw.into().trim().to_ascii_lowercase();
        // Must have at least domain:verb
        let segments: Vec<&str> = s.split(':').collect();
        if segments.len() < 2 || segments.iter().any(|seg| seg.is_empty()) {
            return Err(AuthzError::InvalidAction(s));
        }
        // Only ASCII alphanumeric + colon + hyphen
        if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == ':' || c == '-') {
            return Err(AuthzError::InvalidAction(s));
        }
        Ok(Self(s))
    }
    pub fn as_str(&self) -> &str { &self.0 }
    pub fn domain(&self) -> &str { self.0.split(':').next().unwrap() }
    pub fn verb(&self) -> &str { self.0.splitn(3, ':').nth(1).unwrap() }
}
```

**Normalization:** Soft — `Action::new()` lowercases and trims, then validates. No rejection on case mismatch, just normalization.

**Hashing:** `hash_action(a)` = BLAKE3(TAG_ACTION ++ len_u32_le(a.as_bytes()) ++ a.as_bytes())

### 4.3 ResourceId

```rust
/// Variant order is FROZEN. Do not reorder. Append only.
/// Tag bytes: Entity=0x01, Scope=0x02, Asset=0x03, Contract=0x04, System=0x05
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ResourceKind {
    Entity,    // tag 0x01 — cooperatives, communities, federations, individuals
    Scope,     // tag 0x02 — cells, namespaces, governance scopes
    Asset,     // tag 0x03 — ledger accounts, credit lines, treasury
    Contract,  // tag 0x04 — CCL contracts, amendments
    System,    // tag 0x05 — protocol parameters, invariant config
}

/// A typed resource identifier.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResourceId {
    pub kind: ResourceKind,
    pub id: String,
}
```

**Hashing:** `hash_resource(r)` = BLAKE3(TAG_RESOURCE ++ resource_kind_tag_byte(r.kind) ++ len_u32_le(r.id.as_bytes()) ++ r.id.as_bytes())

Where `resource_kind_tag_byte` uses the explicit tag byte constants (0x01..0x05), NOT enum-as-integer casting.

### 4.4 Constraint

```rust
/// Variant order is FROZEN. Do not reorder. Append only.
/// Tag bytes: RateLimit=0x01, CreditMultiplier=0x02, MaxTopics=0x03,
///            TimeLock=0x04, RequiresQuorum=0x05, Tag=0x06
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Constraint {
    RateLimit(u32),          // tag 0x01 — messages per second
    CreditMultiplier(u32),   // tag 0x02 — basis points (10000 = 1.0x)
    MaxTopics(u32),          // tag 0x03 — topic subscription limit
    TimeLock(u64),           // tag 0x04 — block height before which action is locked
    RequiresQuorum(u32),     // tag 0x05 — minimum votes (absolute count)
    Tag(String),             // tag 0x06 — escape hatch for domain-specific labels
}
```

**Design note:** `CreditMultiplier` uses basis points (u32) instead of f64 for determinism. 10000 = 1.0x, 15000 = 1.5x.

**Hashing:** `hash_constraint(c)` = BLAKE3(TAG_CONSTRAINT ++ constraint_tag_byte ++ value_bytes)
- For `u32` values: 4 bytes LE
- For `u64` values: 8 bytes LE
- For `Tag(s)`: len_u32_le(s.as_bytes()) ++ s.as_bytes()

### 4.5 EdgeSource

```rust
/// Variant order is FROZEN. Do not reorder. Append only.
/// Tag bytes: CclContract=0x01, TrustScore=0x02, GovernanceVote=0x03, Static=0x04
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum EdgeSource {
    CclContract(String),     // tag 0x01 — contract hash or ID
    TrustScore(String),      // tag 0x02 — trust graph identifier
    GovernanceVote(String),  // tag 0x03 — proposal ID
    Static(String),          // tag 0x04 — hardcoded/bootstrap grants
}
```

**Hashing:** `hash_edge_source(es)` = BLAKE3(TAG_EDGE_SOURCE ++ source_tag_byte ++ len_u32_le(inner.as_bytes()) ++ inner.as_bytes())

### 4.6 CapabilityEdge

The fundamental unit of the graph: "subject S can do action A on resource R, with constraints C, granted by source E, valid at height H."

```rust
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CapabilityEdge {
    pub subject: SubjectId,
    pub action: Action,
    pub resource: ResourceId,
    pub constraints: Vec<Constraint>,  // sorted, deduped
    pub source: EdgeSource,
    pub valid_at: Option<BlockHeight>,
}
```

**Invariant:** `constraints` is always sorted and deduped. The constructor enforces this.

**Hashing:** `hash_edge(e)` = BLAKE3 of field-by-field update:
```
hasher.update(TAG_EDGE)
hasher.update(hash_subject(e.subject))
hasher.update(hash_action(e.action))
hasher.update(hash_resource(e.resource))
hasher.update(len_u32_le(e.constraints.len()))
for c in &e.constraints { hasher.update(hash_constraint(c)) }
hasher.update(hash_edge_source(e.source))
hasher.update(hash_option_height(e.valid_at))
```

## 5. Canonicalization Rules

All types implement `Ord`. Canonical ordering is the foundation of deterministic hashing.

### 5.1 Edge Set Canonicalization

Given a `Vec<CapabilityEdge>`:
1. Sort by `Ord` (subject → action → resource → constraints → source → valid_at)
2. Dedup consecutive equal edges
3. Hash: BLAKE3(TAG_EDGE_SET ++ len_u32_le(edges.len()) ++ hash(edge_0) ++ hash(edge_1) ++ ...)

### 5.2 Constraint Canonicalization

Within a single edge, constraints are:
1. Sorted by `Ord` (variant discriminant first, then inner value)
2. Deduped — exact duplicates removed

This is enforced at construction time by `CapabilityEdge::new()`.

### 5.3 No HashMap Anywhere

The canonical model uses only `Vec`, `String`, and fixed-size types. No `HashMap`, no `BTreeMap`, no `HashSet`. All collections are sorted `Vec`s. This eliminates iteration-order nondeterminism.

## 6. Deterministic Hashing Specification

### 6.1 Core Principles

1. **Length-prefix all variable-length fields.** Every `String`, `&[u8]`, or `Vec<T>` is preceded by its length as `u32` little-endian. This prevents ambiguous concatenations (e.g., `"ab" ++ "cd"` vs `"a" ++ "bcd"`).

2. **Explicit tag bytes for all enums.** Every enum variant has a hardcoded tag byte constant. We NEVER cast `variant as u8` or rely on discriminant values. Tag byte constants are defined as named constants with comments noting they are frozen.

3. **Variant order is frozen.** Every enum has a comment: "Variant order is FROZEN. Do not reorder. Append only." New variants get the next sequential tag byte.

4. **Option encoding.** `None` = single byte 0x00. `Some(v)` = byte 0x01 ++ encoded(v). For `Option<BlockHeight>`, `encoded(v)` = `v.0 as u64` as 8 bytes LE.

5. **BLAKE3 only.** All hashing uses BLAKE3. Output is 32 bytes.

### 6.2 Tag Byte Constants

```rust
// Top-level type tags
const TAG_SUBJECT: u8       = 0x10;
const TAG_ACTION: u8        = 0x11;
const TAG_RESOURCE: u8      = 0x12;
const TAG_CONSTRAINT: u8    = 0x13;
const TAG_EDGE_SOURCE: u8   = 0x14;
const TAG_EDGE: u8          = 0x15;
const TAG_EDGE_SET: u8      = 0x16;

// ResourceKind tags — FROZEN, append only
const RESOURCE_ENTITY: u8   = 0x01;
const RESOURCE_SCOPE: u8    = 0x02;
const RESOURCE_ASSET: u8    = 0x03;
const RESOURCE_CONTRACT: u8 = 0x04;
const RESOURCE_SYSTEM: u8   = 0x05;

// Constraint tags — FROZEN, append only
const CONSTRAINT_RATE_LIMIT: u8         = 0x01;
const CONSTRAINT_CREDIT_MULTIPLIER: u8  = 0x02;
const CONSTRAINT_MAX_TOPICS: u8         = 0x03;
const CONSTRAINT_TIME_LOCK: u8          = 0x04;
const CONSTRAINT_REQUIRES_QUORUM: u8    = 0x05;
const CONSTRAINT_TAG: u8               = 0x06;

// EdgeSource tags — FROZEN, append only
const SOURCE_CCL_CONTRACT: u8     = 0x01;
const SOURCE_TRUST_SCORE: u8      = 0x02;
const SOURCE_GOVERNANCE_VOTE: u8  = 0x03;
const SOURCE_STATIC: u8           = 0x04;
```

### 6.3 Length-Prefix Helper

```rust
fn hash_bytes(hasher: &mut blake3::Hasher, data: &[u8]) {
    hasher.update(&(data.len() as u32).to_le_bytes());
    hasher.update(data);
}

fn hash_option_height(hasher: &mut blake3::Hasher, h: Option<BlockHeight>) {
    match h {
        None => hasher.update(&[0x00]),
        Some(height) => {
            hasher.update(&[0x01]);
            hasher.update(&(height.0 as u64).to_le_bytes());
        }
    }
}
```

## 7. Builder and Query Interfaces

### 7.1 CapabilitySource Trait

```rust
/// An adapter that can produce capability edges from a domain-specific source.
///
/// Implementations live in `icn-authz::adapters` (Phase B1).
/// The trait is defined here so B0 can write tests with mock sources.
pub trait CapabilitySource: Send + Sync {
    /// Return all capability edges this source knows about for the given subject.
    fn edges_for_subject(&self, subject: &SubjectId) -> Vec<CapabilityEdge>;

    /// Return all capability edges this source can produce.
    fn all_edges(&self) -> Vec<CapabilityEdge>;
}
```

### 7.2 GraphBuilder

```rust
/// Constructs a CapabilityGraph from one or more CapabilitySource adapters.
pub struct GraphBuilder {
    sources: Vec<Box<dyn CapabilitySource>>,
}

impl GraphBuilder {
    pub fn new() -> Self { Self { sources: vec![] } }

    pub fn add_source(mut self, source: Box<dyn CapabilitySource>) -> Self {
        self.sources.push(source);
        self
    }

    /// Build the graph by polling all sources and canonicalizing.
    pub fn build(&self) -> CapabilityGraph {
        let mut edges: Vec<CapabilityEdge> = self.sources
            .iter()
            .flat_map(|s| s.all_edges())
            .collect();
        edges.sort();
        edges.dedup();
        CapabilityGraph { edges }
    }

    /// Build a subject-scoped graph (cheaper than full build).
    pub fn build_for_subject(&self, subject: &SubjectId) -> CapabilityGraph {
        let mut edges: Vec<CapabilityEdge> = self.sources
            .iter()
            .flat_map(|s| s.edges_for_subject(subject))
            .collect();
        edges.sort();
        edges.dedup();
        CapabilityGraph { edges }
    }
}
```

### 7.3 CapabilityGraph and Query

```rust
pub struct CapabilityGraph {
    edges: Vec<CapabilityEdge>,  // sorted, deduped
}

impl CapabilityGraph {
    pub fn edges(&self) -> &[CapabilityEdge] { &self.edges }

    /// Deterministic hash of the entire edge set.
    pub fn hash(&self) -> [u8; 32] { hash_edge_set(&self.edges) }

    /// Query: can subject do action on resource?
    pub fn query(&self, subject: &SubjectId, action: &Action, resource: &ResourceId) -> Decision {
        let matching: Vec<usize> = self.edges.iter()
            .enumerate()
            .filter(|(_, e)| e.subject == *subject && e.action == *action && e.resource == *resource)
            .map(|(i, _)| i)
            .collect();

        Decision {
            allowed: !matching.is_empty(),
            matching_edges: matching,
        }
    }
}

/// B0 decision: just allowed + matching edge indices. No string reasons.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub allowed: bool,
    pub matching_edges: Vec<usize>,
}
```

## 8. Error Type

```rust
#[derive(Debug, thiserror::Error)]
pub enum AuthzError {
    #[error("invalid subject ID (must start with 'did:'): {0}")]
    InvalidSubjectId(String),

    #[error("invalid action format (expected 'domain:verb[:subverb]'): {0}")]
    InvalidAction(String),
}
```

Minimal for B0. No `anyhow`, no error chains. Grows as adapters land in B1.

## 9. Crate Structure (B0)

```
icn/crates/icn-authz/
  Cargo.toml
  src/
    lib.rs              # pub mod model; pub mod graph; pub mod error;
    error.rs            # AuthzError
    model/
      mod.rs            # pub mod ids; pub mod edge; pub mod hash;
      ids.rs            # SubjectId, Action, ResourceKind, ResourceId, Constraint, EdgeSource
      edge.rs           # CapabilityEdge, CapabilityGraph, Decision
      hash.rs           # All hashing: tag constants, hash_*, hash_edge_set
    graph/
      mod.rs            # pub mod builder; pub mod query;
      builder.rs        # CapabilitySource trait, GraphBuilder
      query.rs          # Query logic (may be inlined into CapabilityGraph for B0)
```

**Cargo.toml dependencies (B0):**
```toml
[dependencies]
blake3 = "1.8"
serde = { workspace = true, features = ["derive"] }
thiserror.workspace = true
icn-kernel-api = { path = "../icn-kernel-api" }  # BlockHeight only

[dev-dependencies]
serde_json = "1.0"
```

## 10. Testing Requirements

### 10.1 Ordering Invariant Tests
- Create 100 random edges, sort, verify sorted
- Serialize to JSON, deserialize, verify still sorted
- Two graphs built from same edges in different insertion order → same `Ord` result

### 10.2 Hash Determinism Tests
- Same edge set → same hash (trivial)
- Same edges, different insertion order → same hash after canonicalization
- Add one edge → hash changes
- Remove one edge → hash changes
- Change one constraint → hash changes

### 10.3 Action Normalization Tests
- `"GOVERNANCE:PROPOSE"` normalizes to `"governance:propose"`
- `"  ledger:transfer  "` normalizes to `"ledger:transfer"`
- `""` → error
- `"single"` → error (no colon)
- `"a::b"` → error (empty segment)
- `"a:b:c"` → ok (3 segments)

### 10.4 SubjectId Validation Tests
- `"did:icn:abc123"` → ok
- `"not-a-did"` → error
- `""` → error

### 10.5 GraphBuilder Tests
- Empty graph → empty edges, query returns `allowed: false`
- Two sources with overlapping edges → deduped
- `build_for_subject` returns subset

## 11. Follow-On Phases

### B1: Adapters (after B0 merges)
- `CclCapabilitySource`: reads CCL contracts, emits edges
- `TrustCapabilitySource`: reads trust scores, emits rate-limit edges
- `JwtCapabilitySource`: reads token claims, emits edges

### B2: PowerDiff Integration
- `PowerDiff::from_graphs(before: &CapabilityGraph, after: &CapabilityGraph)`
- Replaces the current `PowerDiff` stub with real edge-level diff

### B3: Pipeline Wiring
- Amendment lifecycle calls `GraphBuilder` before/after
- `InvariantGate` receives `PowerDiff` from graph comparison
- Full safety pipeline: CCL change → graph diff → invariant check → receipt
