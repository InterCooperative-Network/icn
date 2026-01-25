# ICN Kernel Contracts Specification

**Version**: 0.1.0
**Status**: Draft
**Last Updated**: 2025-01-25

This document defines the contracts for ICN kernel primitives. It is the constitutional document for the kernel architecture - all kernel code must conform to these contracts.

---

## Table of Contents

1. [Contract Rules](#0-contract-rules)
2. [Common Types](#1-common-types)
3. [Identity Primitive](#2-identity-primitive)
4. [Authorization Primitive](#3-authorization-primitive)
5. [State Primitive](#4-state-primitive)
6. [Compute Primitive](#5-compute-primitive)
7. [Communication Primitive](#6-communication-primitive)
8. [Time Primitive](#7-time-primitive)
9. [Coordination Primitive](#8-coordination-primitive)
10. [Naming Primitive](#9-naming-primitive)
11. [App Runtime Contract](#10-app-runtime-contract)

---

## 0. Contract Rules

### 0.1 Stability Policy

The kernel follows semantic versioning for API stability:

| Version Type | Allowed Changes |
|--------------|-----------------|
| Patch (0.1.x) | Bugfixes only. No API or wire format changes. |
| Minor (0.x.0) | Additive changes only. New methods, new optional fields. |
| Major (x.0.0) | Breaking changes allowed with migration path. |

**Wire format stability**: All serialized formats (events, capabilities, messages) MUST be versioned and support at least N-2 versions for reading.

### 0.2 The Meaning Firewall

The kernel provides **mechanisms**, not **semantics**.

**The kernel MUST NOT:**
- Define domain schemas (ledger transactions, governance proposals, membership records)
- Embed trust computation logic (trust scores, reputation algorithms)
- Embed membership logic (who's in an org, role definitions)
- Privilege "official" apps over third-party apps
- Interpret the meaning of events or credentials

**The kernel MUST:**
- Provide generic, domain-agnostic primitives
- Treat all apps equally (same APIs for core and third-party apps)
- Use policy oracles for authorization decisions
- Accept arbitrary bytes/schemas without interpretation

**Litmus test**: "Could a developer build a DIFFERENT economic model / governance system / trust algorithm using these primitives?" If no, the primitives are too specific.

### 0.3 Extension Points

Each primitive MUST define:

1. **Adapter interfaces**: Pluggable implementations (e.g., different storage backends, different DID methods)
2. **Policy oracle hooks**: Points where apps can inject authorization decisions
3. **Resource accounting hooks**: Metering points for quotas and limits

### 0.4 PR Checklist

Every PR touching kernel code MUST answer:

- [ ] Does this add a new kernel operation? → Update `kernel_surface.toml`
- [ ] Does this interpret domain semantics? → Must be an app, not kernel
- [ ] Does this hardcode a schema? → Must be an app, not kernel
- [ ] Does this privilege a specific application? → Reject or use oracle pattern
- [ ] Does this add policy logic? → Must use policy oracle hook
- [ ] Does this break wire format compatibility? → Requires major version bump

---

## 1. Common Types

All primitives share these foundational types:

```rust
// === Identifiers ===

/// Decentralized Identifier (e.g., "did:icn:z6MkABC...")
pub type Did = String;

/// Node identifier (unique per physical/virtual node)
pub type NodeId = String;

/// Organization identifier (maps to a DID)
pub type OrgId = Did;

/// Namespace identifier (hierarchical: "/org/app/sub")
pub type NamespaceId = String;

/// Application identifier
pub type AppId = String;

/// Capability token identifier
pub type CapabilityId = String;

// === Time and Ordering ===

/// Logical timestamp for causal ordering
/// Monotonically increasing within a node, comparable across nodes via vector clocks
pub type LogicalTimestamp = u64;

/// Version number for optimistic concurrency (CAS operations)
pub type Version = u64;

/// Offset position in a log
pub type Offset = u64;

// === Storage ===

/// Content hash for content-addressed storage (SHA-256)
pub type Hash = [u8; 32];

/// Generic key for KV operations
pub type Key = Vec<u8>;

/// Generic value for KV operations
pub type Value = Vec<u8>;

// === Namespace ===

/// Hierarchical namespace for resource isolation
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Namespace {
    /// Organization that owns this namespace
    pub org: OrgId,
    /// Application within the org
    pub app: AppId,
    /// Optional sub-namespace for further isolation
    pub sub: Option<String>,
}

impl Namespace {
    /// Creates a new namespace: /<org>/<app>/
    pub fn new(org: impl Into<String>, app: impl Into<String>) -> Self {
        Self {
            org: org.into(),
            app: app.into(),
            sub: None,
        }
    }

    /// Adds a sub-namespace: /<org>/<app>/<sub>/
    pub fn with_sub(mut self, sub: impl Into<String>) -> Self {
        self.sub = Some(sub.into());
        self
    }

    /// Returns the full path as a string
    pub fn to_path(&self) -> String {
        match &self.sub {
            Some(s) => format!("/{}/{}/{}", self.org, self.app, s),
            None => format!("/{}/{}", self.org, self.app),
        }
    }
}

// === Errors ===

/// Standard error structure for kernel operations
#[derive(Debug, Clone)]
pub struct KernelError {
    /// Error code for programmatic handling
    pub code: ErrorCode,
    /// Human-readable message
    pub message: String,
    /// Whether the operation can be retried
    pub retryable: bool,
    /// Who is at fault: caller, kernel, or adapter
    pub blame: Blame,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    // Authorization errors
    Unauthorized,
    CapabilityExpired,
    CapabilityRevoked,
    InsufficientCapability,

    // State errors
    NotFound,
    AlreadyExists,
    VersionMismatch,
    NamespaceNotFound,

    // Resource errors
    QuotaExceeded,
    RateLimited,

    // System errors
    Internal,
    Unavailable,
    Timeout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blame {
    /// Caller made an invalid request
    Caller,
    /// Kernel had an internal error
    Kernel,
    /// External adapter/service failed
    Adapter,
}
```

---

## 2. Identity Primitive

The Identity primitive provides decentralized identity management without credential semantics.

### 2.1 Contract

**Provides:**
- DID creation, rotation, and deactivation
- Cryptographic signing and verification
- DID document resolution via pluggable resolvers
- Keystore interface (sign without key export)

**Does NOT provide:**
- Credential semantics (what credentials mean)
- Membership (who's in an org)
- Trust/reputation scores
- Steward ceremonies or verification rituals

### 2.2 Trait Definitions

```rust
/// Core identity operations
pub trait IdentityService: Send + Sync {
    /// Create a new DID with the specified method
    fn did_create(&self, method: DidMethod) -> Result<(Did, DidDocument), IdentityError>;

    /// Rotate keys for an existing DID
    fn did_rotate(&self, did: &Did, new_keys: KeyBundle) -> Result<DidDocument, IdentityError>;

    /// Deactivate a DID (mark as no longer valid)
    fn did_deactivate(&self, did: &Did) -> Result<(), IdentityError>;

    /// Sign arbitrary bytes with a DID's signing key
    fn sign(&self, did: &Did, payload: &[u8]) -> Result<Signature, IdentityError>;

    /// Verify a signature against a DID's public key
    fn verify(&self, did: &Did, payload: &[u8], signature: &Signature) -> Result<bool, IdentityError>;
}

/// DID document resolution
pub trait DidResolver: Send + Sync {
    /// Resolve a DID to its document
    fn resolve(&self, did: &Did) -> Result<DidDocument, ResolveError>;

    /// Check if a DID is still active
    fn is_active(&self, did: &Did) -> Result<bool, ResolveError>;
}

/// Keystore for secure key management
pub trait Keystore: Send + Sync {
    /// Sign payload using a key (key never exported)
    fn sign(&self, key_id: &KeyId, payload: &[u8]) -> Result<Signature, KeystoreError>;

    /// Get public key for a key ID
    fn public_key(&self, key_id: &KeyId) -> Result<PublicKey, KeystoreError>;

    /// List all key IDs in the keystore
    fn list_keys(&self) -> Result<Vec<KeyId>, KeystoreError>;

    // Note: No export_private_key - signing without exposure
}

/// Supported DID methods
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DidMethod {
    /// did:key - self-certifying, no network lookup
    Key,
    /// did:icn - ICN-native, resolved via ICN network
    Icn,
    /// did:web - web-based, requires HTTP lookup
    Web { domain: String },
}

/// DID Document structure
#[derive(Debug, Clone)]
pub struct DidDocument {
    pub id: Did,
    pub verification_methods: Vec<VerificationMethod>,
    pub authentication: Vec<KeyId>,
    pub assertion_method: Vec<KeyId>,
    pub key_agreement: Vec<KeyId>,
    pub created: LogicalTimestamp,
    pub updated: LogicalTimestamp,
}

/// Key bundle for rotation
#[derive(Debug, Clone)]
pub struct KeyBundle {
    pub signing_key: PublicKey,
    pub encryption_key: Option<PublicKey>,
}

/// Cryptographic signature
#[derive(Debug, Clone)]
pub struct Signature {
    pub algorithm: SignatureAlgorithm,
    pub value: Vec<u8>,
    pub key_id: KeyId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureAlgorithm {
    Ed25519,
    // Future: post-quantum algorithms
}
```

### 2.3 Design Notes

- **DID resolution for networked methods** (did:web) runs as an adapter/service with egress capability
- **Pure methods** (did:key, did:icn) can resolve in deterministic compute
- **Credentials** are "signed blobs with schema reference" - kernel verifies signature, apps interpret meaning
- **Steward ceremonies** and identity verification are apps built on top, not kernel features

---

## 3. Authorization Primitive

The Authorization primitive provides object-capability tokens and policy oracle integration.

### 3.1 Contract

**Provides:**
- Object-capability tokens (NOT RBAC)
- Issue, verify, delegate, revoke capabilities
- Policy oracle interface for external decisions
- Rate limiting hooks based on oracle decisions

**Does NOT provide:**
- Role hierarchies
- Trust scores or reputation
- Predefined capability taxonomies
- Any policy logic (just enforcement)

### 3.2 Trait Definitions

```rust
/// Capability token granting specific access
#[derive(Clone, Debug)]
pub struct Capability {
    /// Unique identifier for this capability
    pub id: CapabilityId,
    /// What resource this grants access to
    pub resource: String,
    /// What action is permitted
    pub action: String,
    /// Additional constraints on use
    pub constraints: Constraints,
    /// If set, holder must co-sign operations
    pub holder: Option<Did>,
    /// Who issued this capability
    pub issuer: Did,
    /// When this capability expires
    pub expiration: LogicalTimestamp,
    /// Cryptographic proof of issuance
    pub signature: Signature,
}

/// Constraints on capability use
#[derive(Clone, Debug, Default)]
pub struct Constraints {
    /// Maximum amount (for numeric resources)
    pub max_amount: Option<u64>,
    /// Maximum number of uses
    pub max_uses: Option<u32>,
    /// Restrict to specific targets
    pub allowed_targets: Option<Vec<String>>,
    /// Additional app-defined constraints (opaque to kernel)
    pub custom: Option<Vec<u8>>,
}

/// Result of a policy evaluation
#[derive(Clone, Debug)]
pub enum PolicyDecision {
    /// Request is allowed to proceed
    Allow,
    /// Request is denied
    Deny { reason: String },
    /// Request is allowed but rate-limited
    RateLimit { class: RateLimitClass },
}

/// Rate limit classification
#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum RateLimitClass {
    /// No rate limiting applied
    Unlimited,
    /// Standard rate limits
    Standard,
    /// Reduced limits (e.g., low trust)
    Throttled,
    /// Severely restricted
    Restricted,
}

/// Context for policy evaluation
#[derive(Clone, Debug)]
pub struct PolicyContext {
    /// Who is making the request
    pub actor: Did,
    /// What resource is being accessed
    pub resource: String,
    /// What action is being performed
    pub action: String,
    /// Namespace context (if applicable)
    pub namespace: Option<Namespace>,
    /// Whether this is a cross-org request
    pub cross_org: bool,
    /// Source org for cross-org requests
    pub source_org: Option<OrgId>,
    /// App-specific metadata
    pub metadata: std::collections::HashMap<String, String>,
}

/// Request being evaluated
#[derive(Clone, Debug)]
pub struct PolicyRequest {
    pub context: PolicyContext,
    pub capability: Option<Capability>,
}

/// Policy oracle interface - implemented by apps (e.g., trust app)
pub trait PolicyOracle: Send + Sync {
    /// Evaluate whether a request should be allowed
    fn evaluate(&self, request: &PolicyRequest) -> PolicyDecision;

    /// Get cache TTL for decisions (allows kernel to cache)
    fn cache_ttl(&self) -> std::time::Duration {
        std::time::Duration::from_secs(60)
    }

    /// Whether this oracle handles cross-org requests
    fn handles_cross_org(&self) -> bool {
        false
    }
}

/// Default oracle that allows everything (for bootstrap and simple deployments)
pub struct AllowAllOracle;

impl PolicyOracle for AllowAllOracle {
    fn evaluate(&self, _request: &PolicyRequest) -> PolicyDecision {
        PolicyDecision::Allow
    }
}

/// Capability management engine
pub trait CapabilityEngine: Send + Sync {
    /// Issue a new capability
    fn issue(
        &self,
        resource: &str,
        action: &str,
        holder: Option<&Did>,
        constraints: Constraints,
        expiration: LogicalTimestamp,
    ) -> Result<Capability, CapabilityError>;

    /// Check if a capability authorizes a request
    fn check(
        &self,
        capability: &Capability,
        resource: &str,
        action: &str,
    ) -> Result<(), CapabilityError>;

    /// Delegate a capability with additional restrictions
    fn delegate(
        &self,
        capability: &Capability,
        to: &Did,
        restrictions: Constraints,
    ) -> Result<Capability, CapabilityError>;

    /// Revoke a capability by ID
    fn revoke(&self, capability_id: &CapabilityId) -> Result<(), CapabilityError>;

    /// Check if a capability has been revoked
    fn is_revoked(&self, capability_id: &CapabilityId) -> Result<bool, CapabilityError>;
}

#[derive(Debug)]
pub enum CapabilityError {
    NotFound,
    Expired,
    Revoked,
    Unauthorized,
    InvalidDelegation,
    ConstraintViolation(String),
    Internal(String),
}
```

### 3.3 Bootstrap Sequence

The capability bootstrapping paradox (trust is an app, but apps need capabilities) is solved via genesis bootstrap:

```rust
/// Bootstrap phases for node startup
pub enum BootstrapPhase {
    /// Phase 0: Kernel only, no apps loaded
    /// - Node identity established
    /// - Genesis capabilities issued to operator DID
    /// - AllowAllOracle active
    Genesis,

    /// Phase 1: Core apps loading
    /// - Trust app loads with genesis capabilities
    /// - Trust app registers as PolicyOracle
    /// - Other core apps load
    CoreApps,

    /// Phase 2: Normal operation
    /// - PolicyOracle now active
    /// - User apps can load
    /// - Full capability checking enabled
    Running,
}

/// Genesis capabilities issued to node operator
/// These CANNOT be revoked by apps
pub struct GenesisCapabilities {
    /// Can install/uninstall apps
    pub app_management: Capability,
    /// Can issue capabilities to apps
    pub capability_issuance: Capability,
    /// Can configure node settings
    pub node_admin: Capability,
}
```

### 3.4 Design Notes

- **Bearer tokens** for automation; **holder-bound tokens** for humans (require co-signature)
- **Short expirations + refresh model** because revocation lists don't scale globally
- **Trust computation is an APP** that implements PolicyOracle; kernel just enforces decisions
- **Default oracle = AllowAllOracle** for simple single-coop deployments

---

## 4. State Primitive

The State primitive provides namespaced, typed storage without domain semantics.

### 4.1 Contract

**Provides:**
- Namespaced storage: `/<org>/<app>/<sub>/`
- Three storage types: Logs (append-only), Blobs (content-addressed), KV (mutable)
- Replication policies per namespace
- Cross-namespace access via capability only

**Does NOT provide:**
- Projections/materialized views (runtime layer)
- Search/indexing (runtime layer)
- Domain-specific schemas
- "Ledger" or "governance" data types

### 4.2 Trait Definitions

```rust
/// Append-only, ordered event streams
pub trait LogService: Send + Sync {
    /// Create a new log in a namespace
    fn create(
        &self,
        namespace: &Namespace,
        name: &str,
        schema: Option<SchemaRef>,
        replication: ReplicationPolicy,
    ) -> Result<LogId, StateError>;

    /// Append an event to a log
    fn append(&self, log: &LogId, event: &[u8]) -> Result<Offset, StateError>;

    /// Read events from a log
    fn read(&self, log: &LogId, from: Offset, limit: usize) -> Result<Vec<LogEntry>, StateError>;

    /// Get current log length
    fn length(&self, log: &LogId) -> Result<Offset, StateError>;

    /// Subscribe to new events
    fn subscribe(&self, log: &LogId, from: Offset) -> Result<Subscription<LogEntry>, StateError>;
}

/// Content-addressed immutable blob storage
pub trait BlobService: Send + Sync {
    /// Store a blob, returns content hash
    fn put(&self, namespace: &Namespace, data: &[u8]) -> Result<Hash, StateError>;

    /// Retrieve a blob by hash
    fn get(&self, hash: &Hash) -> Result<Vec<u8>, StateError>;

    /// Check if a blob exists
    fn exists(&self, hash: &Hash) -> Result<bool, StateError>;

    /// Delete a blob (if permitted by retention policy)
    fn delete(&self, hash: &Hash) -> Result<(), StateError>;
}

/// Mutable key-value storage with optimistic concurrency
pub trait KvService: Send + Sync {
    /// Get a value and its version
    fn get(&self, namespace: &Namespace, key: &[u8]) -> Result<(Value, Version), StateError>;

    /// Put a value with CAS (compare-and-swap)
    fn put(
        &self,
        namespace: &Namespace,
        key: &[u8],
        value: &[u8],
        expected_version: Version,
    ) -> Result<Version, StateError>;

    /// Delete a key with CAS
    fn delete(
        &self,
        namespace: &Namespace,
        key: &[u8],
        expected_version: Version,
    ) -> Result<(), StateError>;

    /// List keys with a prefix
    fn list(&self, namespace: &Namespace, prefix: &[u8]) -> Result<Vec<Key>, StateError>;

    /// Watch for changes to a key
    fn watch(&self, namespace: &Namespace, key: &[u8]) -> Result<Subscription<KvChange>, StateError>;
}

/// Log entry returned from reads
#[derive(Clone, Debug)]
pub struct LogEntry {
    pub offset: Offset,
    pub timestamp: LogicalTimestamp,
    pub author: Did,
    pub data: Vec<u8>,
}

/// KV change notification
#[derive(Clone, Debug)]
pub struct KvChange {
    pub key: Key,
    pub old_value: Option<Value>,
    pub new_value: Option<Value>,
    pub new_version: Version,
}

/// Replication policy for storage
#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum ReplicationPolicy {
    /// Single node, no replication
    LocalOnly,
    /// Consensus group, linearizable reads/writes
    ClusterStrong,
    /// Gossip/CRDT, eventually consistent
    FederationEventual,
    /// Durable archive, retained indefinitely
    Archive,
}

/// Reference to a schema (for type checking)
#[derive(Clone, Debug)]
pub struct SchemaRef {
    pub name: String,
    pub version: String,
    pub hash: Option<Hash>,
}

/// Unique log identifier
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LogId {
    pub namespace: Namespace,
    pub name: String,
}

#[derive(Debug)]
pub enum StateError {
    NotFound,
    AlreadyExists,
    VersionMismatch { expected: Version, actual: Version },
    NamespaceNotFound,
    QuotaExceeded,
    Unauthorized,
    Internal(String),
}
```

### 4.3 Ordering Guarantees

- **Logs** are ordered per-writer by default (cheap, scalable)
- **Total ordering** requires consensus group (`ClusterStrong`) - opt-in, expensive
- **Apps that need global order** must explicitly request `ClusterStrong` replication

---

## 5. Compute Primitive

The Compute primitive provides WASM execution with metering and scheduling.

### 5.1 Contract

**Provides:**
- WASM execution with resource metering
- Two execution modes: deterministic and service
- Scheduling (immediate, triggered, periodic)
- Resource quotas per app

**Does NOT provide:**
- Domain-specific libraries
- Privileged execution for "official" apps
- Any interpretation of what code does

### 5.2 Trait Definitions

```rust
/// Compute execution engine
pub trait ComputeEngine: Send + Sync {
    /// Execute deterministic computation (no I/O, reproducible)
    fn execute_deterministic(
        &self,
        module: &WasmModule,
        function: &str,
        input: &[u8],
        fuel: u64,
    ) -> Result<ExecutionResult, ComputeError>;

    /// Start a long-running service
    fn start_service(
        &self,
        module: &WasmModule,
        config: ServiceConfig,
        quota: ResourceQuota,
    ) -> Result<ServiceId, ComputeError>;

    /// Stop a running service
    fn stop_service(&self, id: &ServiceId) -> Result<(), ComputeError>;

    /// Get service status
    fn service_status(&self, id: &ServiceId) -> Result<ServiceStatus, ComputeError>;

    /// Schedule a job
    fn schedule(&self, job: Job, trigger: Trigger) -> Result<JobId, ComputeError>;

    /// Cancel a scheduled job
    fn cancel(&self, job_id: &JobId) -> Result<(), ComputeError>;
}

/// Result of deterministic execution
#[derive(Clone, Debug)]
pub struct ExecutionResult {
    pub output: Vec<u8>,
    pub fuel_consumed: u64,
    pub logs: Vec<String>,
}

/// WASM module reference
#[derive(Clone, Debug)]
pub struct WasmModule {
    pub hash: Hash,
    pub abi: AbiVersion,
}

/// ABI version for WASM modules
#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum AbiVersion {
    /// Deterministic compute: no I/O, no wall clock, no randomness
    /// Same input → same output, always. Can be replayed to verify.
    /// Use for: reducers, validators, governance tallying
    IcnDeterministicV1,

    /// Service compute: capability-gated I/O allowed
    /// Long-running processes with network/storage access.
    /// Use for: indexers, bridges, WebSocket handlers
    IcnServiceV1,
}

/// Configuration for a service
#[derive(Clone, Debug)]
pub struct ServiceConfig {
    pub name: String,
    pub entry_point: String,
    pub environment: std::collections::HashMap<String, String>,
}

/// Resource quota for compute
#[derive(Clone, Debug)]
pub struct ResourceQuota {
    pub max_memory_bytes: u64,
    pub max_cpu_ms_per_second: u64,
    pub max_storage_bytes: u64,
    pub max_network_bytes_per_second: u64,
}

/// Service status
#[derive(Clone, Debug)]
pub enum ServiceStatus {
    Starting,
    Running { uptime_seconds: u64 },
    Stopping,
    Stopped,
    Failed { error: String },
}

/// Job definition for scheduling
#[derive(Clone, Debug)]
pub struct Job {
    pub module: WasmModule,
    pub function: String,
    pub input: Vec<u8>,
    pub fuel_limit: u64,
}

/// Trigger for scheduled jobs
#[derive(Clone, Debug)]
pub enum Trigger {
    /// Execute immediately
    Immediate,
    /// Execute when an event appears on a log
    OnEvent { log: LogId, filter: Option<String> },
    /// Execute at a specific logical time
    AtTime { time: LogicalTimestamp },
    /// Execute periodically
    Every { interval: std::time::Duration },
}

#[derive(Debug)]
pub enum ComputeError {
    ModuleNotFound,
    FuelExhausted,
    MemoryExceeded,
    Timeout,
    ServiceNotFound,
    QuotaExceeded,
    InvalidAbi,
    ExecutionFailed(String),
    Internal(String),
}
```

### 5.3 Design Notes

- **ABIs are versioned and stable** (like syscalls) - never break without major version
- **Scheduling is at-least-once** - exactly-once requires coordination pattern at app layer
- **All apps use the same compute APIs** - no privileged syscalls for "core" functionality

---

## 6. Communication Primitive

The Communication primitive provides messaging without message semantics.

### 6.1 Contract

**Provides:**
- Pub/sub with at-least-once delivery
- Request/response routing
- Long-lived bidirectional streams
- Adapter interface for external protocols

**Does NOT provide:**
- Message semantics (what messages mean)
- Moderation/filtering logic (apps via oracles)
- Exactly-once delivery (apps build this if needed)

### 6.2 Trait Definitions

```rust
/// Publish-subscribe messaging
pub trait PubSub: Send + Sync {
    /// Publish a message to a topic
    fn publish(&self, topic: &TopicId, message: &[u8]) -> Result<MessageId, CommsError>;

    /// Subscribe to a topic
    fn subscribe(&self, topic: &TopicId, from: Offset) -> Result<Subscription<Message>, CommsError>;

    /// Unsubscribe from a topic
    fn unsubscribe(&self, subscription: SubscriptionId) -> Result<(), CommsError>;
}

/// Request-response pattern
pub trait RequestResponse: Send + Sync {
    /// Register a handler for a path
    fn register(&self, path: &Path, handler: HandlerId) -> Result<(), CommsError>;

    /// Unregister a handler
    fn unregister(&self, path: &Path) -> Result<(), CommsError>;

    /// Send a request and await response
    fn request(
        &self,
        endpoint: &Endpoint,
        request: &[u8],
        timeout: std::time::Duration,
    ) -> Result<Vec<u8>, CommsError>;
}

/// Bidirectional streaming
pub trait Streams: Send + Sync {
    /// Open a bidirectional stream to an endpoint
    fn open(&self, endpoint: &Endpoint) -> Result<BidirectionalStream, CommsError>;

    /// Accept incoming streams on a path
    fn accept(&self, path: &Path) -> Result<Subscription<BidirectionalStream>, CommsError>;
}

/// External protocol adapters
pub trait Adapter: Send + Sync {
    /// Send via external protocol (email, push, webhooks)
    fn send(
        &self,
        protocol: Protocol,
        destination: &str,
        payload: &[u8],
    ) -> Result<(), CommsError>;

    /// Receive from external protocol
    fn receive(&self, protocol: Protocol) -> Result<Subscription<ExternalMessage>, CommsError>;
}

/// Message from pub/sub
#[derive(Clone, Debug)]
pub struct Message {
    pub id: MessageId,
    pub topic: TopicId,
    pub author: Did,
    pub timestamp: LogicalTimestamp,
    pub data: Vec<u8>,
    pub signature: Signature,
}

/// Topic identifier
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TopicId {
    pub namespace: Namespace,
    pub name: String,
}

/// Endpoint for request/response and streams
#[derive(Clone, Debug)]
pub struct Endpoint {
    pub node: Option<NodeId>,
    pub org: OrgId,
    pub path: Path,
}

/// Path for routing
pub type Path = String;

/// Bidirectional stream handle
pub struct BidirectionalStream {
    // Implementation details hidden
}

impl BidirectionalStream {
    pub fn send(&self, data: &[u8]) -> Result<(), CommsError>;
    pub fn recv(&self) -> Result<Vec<u8>, CommsError>;
    pub fn close(&self) -> Result<(), CommsError>;
}

/// External protocol types
#[derive(Clone, Debug)]
pub enum Protocol {
    Email,
    WebPush,
    Webhook { url: String },
    Custom(String),
}

#[derive(Debug)]
pub enum CommsError {
    TopicNotFound,
    EndpointUnreachable,
    Timeout,
    StreamClosed,
    Unauthorized,
    Internal(String),
}
```

### 6.3 Ordering Guarantees

- **Per-topic partition ordering** (not global)
- **Message IDs are stable and signed** (can verify authenticity)
- **Consumer offsets are explicit** (client tracks position)
- **Replay is first-class** (projections can rebuild from any offset)

---

## 7. Time Primitive

The Time primitive provides logical clocks and scheduling.

### 7.1 Contract

**Provides:**
- Logical clocks for causal ordering
- Scheduling interface for deferred execution
- Leases for distributed coordination

**Does NOT provide:**
- Wall-clock truth (impossible in distributed systems)
- Calendar/timezone concepts
- "Vote ends at 5pm" semantics (app layer)

### 7.2 Trait Definitions

```rust
/// Time services
pub trait TimeService: Send + Sync {
    /// Get current logical timestamp
    fn now(&self) -> LogicalTimestamp;

    /// Compare two timestamps (happens-before relationship)
    fn compare(&self, a: LogicalTimestamp, b: LogicalTimestamp) -> Ordering;

    /// Schedule a callback at a logical time
    fn schedule_at(
        &self,
        time: LogicalTimestamp,
        callback: CallbackId,
    ) -> Result<ScheduleId, TimeError>;

    /// Schedule a callback after a duration
    fn schedule_after(
        &self,
        duration: std::time::Duration,
        callback: CallbackId,
    ) -> Result<ScheduleId, TimeError>;

    /// Cancel a scheduled callback
    fn cancel(&self, id: ScheduleId) -> Result<(), TimeError>;

    /// Acquire a lease on a resource
    fn acquire_lease(
        &self,
        resource: &str,
        duration: std::time::Duration,
    ) -> Result<Lease, TimeError>;

    /// Renew an existing lease
    fn renew_lease(
        &self,
        lease: &Lease,
        duration: std::time::Duration,
    ) -> Result<Lease, TimeError>;

    /// Release a lease early
    fn release_lease(&self, lease: Lease) -> Result<(), TimeError>;
}

/// Lease for distributed coordination
#[derive(Clone, Debug)]
pub struct Lease {
    pub id: LeaseId,
    pub resource: String,
    pub holder: Did,
    pub expires: LogicalTimestamp,
}

pub type LeaseId = String;
pub type ScheduleId = String;
pub type CallbackId = String;

#[derive(Debug)]
pub enum TimeError {
    LeaseNotAvailable,
    LeaseExpired,
    LeaseNotHeld,
    ScheduleNotFound,
    Internal(String),
}
```

---

## 8. Coordination Primitive

The Coordination primitive provides consensus and CRDTs.

### 8.1 Contract

**Provides:**
- Scoped consensus groups (Raft-like)
- CRDT replication for eventual consistency
- Atomic compare-and-swap operations
- Leader election with leases

**Does NOT provide:**
- Global consensus (no global chain)
- Cross-scope transactions (app layer via sagas)
- Arbitrary merge functions (security risk)

### 8.2 Trait Definitions

```rust
/// Coordination services
pub trait Coordination: Send + Sync {
    // === Consensus Groups ===

    /// Create a consensus group with specified members
    fn create_group(
        &self,
        members: Vec<NodeId>,
        config: ConsensusConfig,
    ) -> Result<GroupId, CoordError>;

    /// Propose a value to a consensus group
    fn propose(&self, group: &GroupId, value: &[u8]) -> Result<Decision, CoordError>;

    /// Get current group status
    fn group_status(&self, group: &GroupId) -> Result<GroupStatus, CoordError>;

    // === CRDTs ===

    /// Create a CRDT in a namespace
    fn create_crdt(
        &self,
        namespace: &Namespace,
        name: &str,
        crdt_type: CrdtType,
    ) -> Result<CrdtId, CoordError>;

    /// Apply an operation to a CRDT
    fn update_crdt(&self, id: &CrdtId, op: CrdtOp) -> Result<(), CoordError>;

    /// Read current CRDT value
    fn read_crdt(&self, id: &CrdtId) -> Result<CrdtValue, CoordError>;

    // === Leader Election ===

    /// Elect a leader for a group
    fn elect_leader(
        &self,
        group: &GroupId,
        lease_duration: std::time::Duration,
    ) -> Result<LeaderLease, CoordError>;

    /// Check if we hold leadership
    fn is_leader(&self, group: &GroupId) -> Result<bool, CoordError>;
}

/// Consensus group configuration
#[derive(Clone, Debug)]
pub struct ConsensusConfig {
    pub min_quorum: usize,
    pub election_timeout: std::time::Duration,
    pub heartbeat_interval: std::time::Duration,
}

/// Consensus decision
#[derive(Clone, Debug)]
pub enum Decision {
    Accepted { value: Vec<u8>, index: u64 },
    Rejected { reason: String },
}

/// Group status
#[derive(Clone, Debug)]
pub struct GroupStatus {
    pub leader: Option<NodeId>,
    pub members: Vec<NodeId>,
    pub commit_index: u64,
}

/// Supported CRDT types (finite set for security)
#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum CrdtType {
    /// Grow-only counter
    GCounter,
    /// Positive-negative counter
    PNCounter,
    /// Observed-remove set
    OrSet,
    /// Last-writer-wins register
    LwwRegister,
}

/// CRDT operation
#[derive(Clone, Debug)]
pub enum CrdtOp {
    Increment(u64),
    Decrement(u64),
    Add(Vec<u8>),
    Remove(Vec<u8>),
    Set(Vec<u8>),
}

/// CRDT value
#[derive(Clone, Debug)]
pub enum CrdtValue {
    Counter(i64),
    Set(Vec<Vec<u8>>),
    Register(Vec<u8>),
}

#[derive(Debug)]
pub enum CoordError {
    GroupNotFound,
    NotLeader,
    QuorumNotReached,
    CrdtNotFound,
    InvalidOperation,
    Internal(String),
}
```

### 8.3 Critical Design Rule

**Consensus is always scoped** (per namespace, per dataset). No global chain. This prevents:
- Imperial scaling (one chain to rule them all)
- Single points of failure
- Unbounded coordination costs

---

## 9. Naming Primitive

The Naming primitive provides name resolution and service discovery.

### 9.1 Contract

**Provides:**
- Name records (name → target, signed by authority)
- Resolution within scopes (local, org, federation)
- Service discovery and announcement
- Watch for changes

**Does NOT provide:**
- Human-readable name policies
- Dispute resolution (just enforces signatures)
- Global namespace (scoped by design)

### 9.2 Trait Definitions

```rust
/// Name resolution service
pub trait NamingService: Send + Sync {
    /// Register a name record
    fn register(
        &self,
        name: &Name,
        target: Target,
        authority: &Did,
        signature: &Signature,
        ttl: std::time::Duration,
    ) -> Result<(), NamingError>;

    /// Resolve a name to its target
    fn resolve(&self, name: &Name) -> Result<ResolvedTarget, NamingError>;

    /// Update an existing name record
    fn update(
        &self,
        name: &Name,
        new_target: Target,
        signature: &Signature,
    ) -> Result<(), NamingError>;

    /// Delete a name record
    fn delete(&self, name: &Name, signature: &Signature) -> Result<(), NamingError>;

    /// Watch for changes to a name
    fn watch(&self, name: &Name) -> Result<Subscription<NameChange>, NamingError>;

    /// List names under a prefix
    fn list(&self, prefix: &Name, scope: Scope) -> Result<Vec<Name>, NamingError>;
}

/// Service discovery
pub trait Discovery: Send + Sync {
    /// Announce a service
    fn announce(
        &self,
        scope: Scope,
        service_type: &str,
        endpoint: Endpoint,
        metadata: DiscoveryMetadata,
    ) -> Result<AnnouncementId, NamingError>;

    /// Discover services of a type
    fn discover(
        &self,
        scope: Scope,
        service_type: &str,
    ) -> Result<Vec<DiscoveredService>, NamingError>;

    /// Stop announcing a service
    fn withdraw(&self, id: AnnouncementId) -> Result<(), NamingError>;
}

/// Hierarchical name (e.g., "/org-123/apps/ledger")
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Name(pub String);

impl Name {
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    pub fn join(&self, segment: &str) -> Self {
        Self(format!("{}/{}", self.0, segment))
    }
}

/// Target of a name record
#[derive(Clone, Debug)]
pub enum Target {
    /// Service endpoint
    Service { endpoint: Endpoint },
    /// Content-addressed blob
    Blob { hash: Hash },
    /// Namespace reference
    Namespace { ns: Namespace },
    /// Alias to another name
    Alias { name: Name },
}

/// Name record with metadata
#[derive(Clone, Debug)]
pub struct NameRecord {
    pub name: Name,
    pub target: Target,
    pub authority: Did,
    pub signature: Signature,
    pub ttl: std::time::Duration,
    pub created: LogicalTimestamp,
    pub updated: LogicalTimestamp,
}

/// Resolved target with resolution path
#[derive(Clone, Debug)]
pub struct ResolvedTarget {
    pub target: Target,
    pub path: Vec<Name>,  // Resolution path for debugging
    pub cached: bool,
}

/// Resolution scope
#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum Scope {
    /// This node only
    Local,
    /// Within organization
    Org,
    /// Across federated orgs
    Federation,
}

/// Metadata for service discovery
#[derive(Clone, Debug, Default)]
pub struct DiscoveryMetadata {
    pub version: Option<String>,
    pub protocols: Vec<String>,
    pub capabilities: Vec<String>,
    pub custom: std::collections::HashMap<String, String>,
}

/// Discovered service
#[derive(Clone, Debug)]
pub struct DiscoveredService {
    pub endpoint: Endpoint,
    pub metadata: DiscoveryMetadata,
    pub announced_by: Did,
    pub last_seen: LogicalTimestamp,
}

#[derive(Debug)]
pub enum NamingError {
    NotFound,
    AlreadyExists,
    Unauthorized,
    InvalidSignature,
    ResolutionLoop,
    Internal(String),
}
```

### 9.3 Why Naming Matters

Without naming, apps hardcode addresses. With naming:
- Apps compose via stable names that survive scaling, failover, and federation
- Service discovery enables dynamic topology
- Aliases enable gradual migrations

---

## 10. App Runtime Contract

This section defines how applications interact with kernel primitives.

### 10.1 App Manifest Schema (v0)

```yaml
# Every app—official or third-party—uses this format
name: my-app
version: 1.0.0
publisher: did:icn:publisher-did

# What this app needs from other apps/kernel
capabilities_required:
  - identity:read
  - state:logs:append:self      # Can append to own logs
  - state:kv:write:self         # Can write to own KV
  - comms:publish:my-app:*      # Can publish to own topics

# What this app provides to others
capabilities_provided:
  - my-app:read
  - my-app:write
  - my-app:admin

# Dependencies on other apps
dependencies:
  - app: trust
    capabilities_needed:
      - trust:query:score
    required: true   # App won't start without this

  - app: membership
    capabilities_needed:
      - membership:read
    required: false  # Graceful degradation if unavailable
    fallback:
      strategy: cache_then_error
      cache_ttl: 300

# Event types this app produces
event_types:
  MyEvent:
    fields:
      foo: string
      bar: integer

# State this app uses
state:
  logs:
    - name: events
      schema: MyEvent
      replication: cluster_strong
  kv:
    - name: cache
      replication: local_only
  blobs:
    - name: attachments
      replication: federation_eventual

# Compute handlers
handlers:
  reducers:  # Deterministic (icn_abi_v1)
    - trigger: on_event(events)
      module: reducer.wasm

  services:  # Long-running (icn_service_abi_v1)
    - name: indexer
      module: indexer.wasm

  endpoints:  # Request/response
    - path: /query
      module: query_handler.wasm

# Resource requirements
resources:
  compute:
    cpu_limit: 100ms_per_event
    memory_limit: 64mb
  storage:
    log_retention: 90d
    blob_quota: 1gb

# Scaling intent (not topology-specific)
placement:
  replicas: 3
  locality: org_scope
```

### 10.2 App Lifecycle

```rust
pub enum AppState {
    /// Manifest loaded, not yet configured
    Installed,
    /// Capabilities granted, ready to start
    Configured,
    /// Handlers running, processing events
    Running,
    /// Temporarily stopped
    Stopped,
    /// Upgrade in progress
    Upgrading { from: Version, to: Version },
    /// Uninstall in progress
    Uninstalling,
}

pub trait AppRuntime: Send + Sync {
    /// Install an app from manifest
    fn install(&self, manifest: AppManifest) -> Result<AppId, RuntimeError>;

    /// Configure an installed app (grant capabilities)
    fn configure(&self, app: &AppId, config: AppConfig) -> Result<(), RuntimeError>;

    /// Start an app
    fn start(&self, app: &AppId) -> Result<(), RuntimeError>;

    /// Stop an app
    fn stop(&self, app: &AppId) -> Result<(), RuntimeError>;

    /// Upgrade an app to a new version
    fn upgrade(&self, app: &AppId, new_manifest: AppManifest) -> Result<(), RuntimeError>;

    /// Uninstall an app
    fn uninstall(&self, app: &AppId) -> Result<(), RuntimeError>;

    /// Get app state
    fn state(&self, app: &AppId) -> Result<AppState, RuntimeError>;
}
```

### 10.3 Isolation Guarantees

1. **Namespace isolation**: Each app gets `/<org>/<app>/`
2. **Cross-namespace access**: Requires explicit capability
3. **Resource quotas**: Enforced per-app
4. **Private state**: Apps cannot access other apps' private state
5. **Public streams**: Apps CAN read other apps' public event streams (with capability)

### 10.4 Handler Contracts

```rust
/// Reducer (deterministic) handler contract
/// Called for each event on subscribed logs
pub trait Reducer {
    /// Process an event, return state updates
    /// MUST be deterministic: same input → same output
    fn reduce(&self, event: &LogEntry) -> Result<Vec<StateUpdate>, ReducerError>;
}

/// Service handler contract
/// Long-running process with I/O capabilities
pub trait Service {
    /// Initialize the service
    fn init(&mut self, config: ServiceConfig) -> Result<(), ServiceError>;

    /// Handle incoming request
    fn handle(&mut self, request: &[u8]) -> Result<Vec<u8>, ServiceError>;

    /// Periodic tick (for background work)
    fn tick(&mut self) -> Result<(), ServiceError>;

    /// Shutdown gracefully
    fn shutdown(&mut self) -> Result<(), ServiceError>;
}
```

---

## Appendix A: Capability Naming Convention

Standard capability format: `<domain>:<resource>:<action>`

**Examples:**
- `identity:read` - Read any identity info
- `state:logs:append:self` - Append to own logs
- `state:kv:write:/org-123/ledger/*` - Write to specific namespace
- `comms:publish:my-app:*` - Publish to own topics
- `trust:query:score` - Query trust scores
- `governance:propose` - Create proposals

**Special scopes:**
- `:self` - Own namespace only
- `:*` - All resources in category
- `:<path>` - Specific resource path

---

## Appendix B: Wire Format Versioning

All serialized formats include version information:

```rust
/// Envelope for all wire messages
pub struct WireEnvelope {
    /// Format version
    pub version: u8,
    /// Message type
    pub message_type: String,
    /// Serialized payload
    pub payload: Vec<u8>,
    /// Signature over (version || message_type || payload)
    pub signature: Signature,
}
```

**Version compatibility:**
- Readers MUST handle versions N, N-1, N-2
- Writers SHOULD use latest version
- Unknown versions MUST be rejected with clear error

---

## Appendix C: Error Handling Guidelines

1. **Errors include blame**: Caller, Kernel, or Adapter
2. **Errors indicate retryability**: Can this be retried?
3. **Error codes are stable**: Numeric codes don't change
4. **Messages are for humans**: May change across versions

```rust
// Example error handling
match result {
    Err(e) if e.retryable => {
        // Retry with backoff
    }
    Err(e) if e.blame == Blame::Caller => {
        // Invalid request, don't retry
    }
    Err(e) => {
        // Log and escalate
    }
    Ok(v) => { /* proceed */ }
}
```

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 0.1.0 | 2025-01-25 | Initial draft |

---

*This document is the constitutional foundation for ICN kernel development. All kernel code must conform to these contracts.*
