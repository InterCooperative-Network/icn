//! Abstract service traits for domain subsystems
//!
//! These traits define kernel-level interfaces that domain apps implement.
//! The kernel uses these traits without knowing about concrete implementations.
//!
//! # The Meaning Firewall
//!
//! These traits are carefully designed to maintain the meaning firewall:
//! - The kernel sees opaque handles and numeric values
//! - Domain semantics (trust classes, governance rules) stay in apps
//! - The kernel enforces constraints without understanding their origin

use crate::authz::PolicyOracle;
use crate::scope::{CellId, ScopeLevel};
use crate::types::Did;
use std::sync::Arc;

// ============================================================================
// Trust Score Thresholds
// ============================================================================
//
// These thresholds map to the trust class boundaries used by apps. The kernel
// uses these numeric values without understanding the underlying "trust class"
// semantics. Apps are free to use different thresholds if needed.

/// Minimum trust score threshold for "Known" level (recognized but not endorsed)
///
/// Peers below this threshold are considered "Isolated" (untrusted).
pub const TRUST_THRESHOLD_KNOWN: f64 = 0.1;

/// Minimum trust score threshold for "Partner" level (trusted collaborator)
///
/// This is the recommended threshold for peer selection in replication,
/// relay selection, and other trust-sensitive operations.
pub const TRUST_THRESHOLD_PARTNER: f64 = 0.4;

/// Minimum trust score threshold for "Federated" level (highly trusted)
///
/// Reserved for federation members and established long-term relationships.
pub const TRUST_THRESHOLD_FEDERATED: f64 = 0.7;

/// Trust classification for policy decisions
///
/// This enum provides a kernel-level abstraction for trust classification.
/// It mirrors the semantics of `icn_trust::TrustClass` but is defined in
/// kernel-api to avoid domain crate dependencies.
///
/// Kernel components should use this enum for policy decisions. Apps
/// translate their domain-specific trust models to these classes.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum TrustClass {
    /// Not yet evaluated or untrusted (score < 0.1)
    #[default]
    Isolated = 0,
    /// Known but not trusted (score 0.1-0.4)
    Known = 1,
    /// Trusted partner (score 0.4-0.7)
    Partner = 2,
    /// Federated peer (score >= 0.7)
    Federated = 3,
}

impl TrustClass {
    /// Convert a trust score to a trust class
    pub fn from_score(score: f64) -> Self {
        if score >= TRUST_THRESHOLD_FEDERATED {
            TrustClass::Federated
        } else if score >= TRUST_THRESHOLD_PARTNER {
            TrustClass::Partner
        } else if score >= TRUST_THRESHOLD_KNOWN {
            TrustClass::Known
        } else {
            TrustClass::Isolated
        }
    }

    /// Get the minimum score for this class
    pub fn min_score(&self) -> f64 {
        match self {
            TrustClass::Isolated => 0.0,
            TrustClass::Known => TRUST_THRESHOLD_KNOWN,
            TrustClass::Partner => TRUST_THRESHOLD_PARTNER,
            TrustClass::Federated => TRUST_THRESHOLD_FEDERATED,
        }
    }

    /// Check if a score meets or exceeds this trust class
    pub fn meets(&self, score: f64) -> bool {
        score >= self.min_score()
    }
}

impl std::fmt::Display for TrustClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrustClass::Isolated => write!(f, "Isolated"),
            TrustClass::Known => write!(f, "Known"),
            TrustClass::Partner => write!(f, "Partner"),
            TrustClass::Federated => write!(f, "Federated"),
        }
    }
}

/// Enriched trust score with provenance metadata.
///
/// This is the kernel-safe counterpart of a "scored result" — it includes
/// enough metadata to make caching safe (version + epoch) and debugging
/// possible (input_count + computed_at) without leaking domain semantics.
///
/// The `reducer_version` field tracks which scoring algorithm produced this
/// result. Cache consumers can use it to invalidate entries when the
/// algorithm changes.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrustScoreResult {
    /// The computed trust score (0.0-1.0)
    pub score: f64,
    /// Monotonic epoch — incremented each time inputs change
    pub epoch: u64,
    /// Unix timestamp when this score was computed
    pub computed_at: u64,
    /// Number of attestation inputs that fed into this score
    pub input_count: u32,
    /// Hash of the inputs (for determinism verification)
    ///
    /// Two nodes with the same inputs_hash MUST produce the same score.
    pub inputs_hash: [u8; 32],
    /// Reducer algorithm version (e.g. "1.0.0")
    ///
    /// Changing the scoring algorithm increments this, signaling caches to
    /// invalidate.
    pub reducer_version: String,
}

impl TrustScoreResult {
    /// Get the score value (convenience accessor)
    pub fn value(&self) -> f64 {
        self.score
    }
}

/// Abstract trust service interface
///
/// This trait provides trust-related functionality to the kernel without
/// exposing TrustGraph, TrustClass, or other domain-specific types.
///
/// # Implementation
///
/// Apps that provide trust functionality implement this trait. The kernel
/// uses it to:
/// - Get a PolicyOracle for authorization decisions
/// - Query trust scores (as opaque f64 values)
/// - Record trust-affecting events
///
/// The kernel NEVER interprets trust scores - it just passes them through.
///
/// # Sync/Async Note
///
/// Methods are synchronous for ergonomic kernel integration. Implementations
/// using async locks (e.g. `tokio::RwLock`) should use
/// `tokio::task::block_in_place()` to bridge sync/async contexts safely.
/// This requires a multi-threaded tokio runtime. Monitor lock contention
/// via the `trust_oracle_block_in_place_total` metric.
pub trait TrustService: Send + Sync {
    /// Get the PolicyOracle for this trust service
    ///
    /// The oracle converts trust decisions into ConstraintSets that the
    /// kernel can enforce blindly.
    fn oracle(&self) -> Arc<dyn PolicyOracle>;

    /// Get trust score for an actor (opaque value 0.0-1.0)
    ///
    /// The kernel may use this for routing decisions, but NEVER interprets
    /// the semantic meaning of the score.
    fn trust_score(&self, actor: &Did) -> f64;

    /// Get enriched trust score with provenance metadata.
    ///
    /// Returns a `TrustScoreResult` that includes the score, epoch,
    /// inputs hash, and reducer version. This is used by caches to
    /// determine staleness and by auditors to verify determinism.
    ///
    /// Default implementation wraps `trust_score()` with minimal metadata.
    fn trust_score_detailed(&self, actor: &Did) -> TrustScoreResult {
        let score = self.trust_score(actor);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        TrustScoreResult {
            score,
            epoch: 0,
            computed_at: now,
            input_count: 0,
            inputs_hash: [0u8; 32],
            reducer_version: "0.0.0".to_string(),
        }
    }

    /// Check if an actor meets a minimum trust threshold
    ///
    /// This is a convenience method for filtering peers by trust level
    /// without exposing domain-specific trust classes.
    ///
    /// Default implementation compares trust_score against threshold.
    fn meets_trust_threshold(&self, actor: &Did, min_score: f64) -> bool {
        self.trust_score(actor) >= min_score
    }

    /// Record a trust-affecting event
    ///
    /// Called by the kernel when violations are detected. The trust service
    /// decides how to update trust scores based on the event type.
    fn record_event(&self, actor: &Did, event: TrustEvent);

    /// Ingest a trust attestation from a peer.
    ///
    /// The kernel routes opaque attestation bytes to the trust service.
    /// The kernel may apply coarse rate limiting before calling this method,
    /// but the trust service owns deserialization, signature verification,
    /// and state updates.
    ///
    /// `source` is the DID of the peer that forwarded this attestation
    /// (the gossip subscriber, not necessarily the attestation issuer).
    /// The trust service decides whether to accept or reject it.
    ///
    /// Default implementation does nothing (no attestation support).
    fn ingest_attestation(&self, _bytes: &[u8], _source: &Did) -> Result<(), String> {
        Ok(())
    }

    /// Recover identity: migrate trust state from old DID to new DID.
    ///
    /// Called when an identity recovery is finalized. The trust service
    /// decides how to transfer trust relationships.
    ///
    /// Returns the number of relationships migrated.
    /// Default implementation does nothing.
    fn recover_identity(&self, _old_did: &Did, _new_did: &Did) -> Result<usize, String> {
        Ok(0)
    }

    /// Get trust edges for a DID (read-only, for API/admin queries).
    ///
    /// Returns serialized edge data as JSON. The kernel does not interpret
    /// the structure — it just routes it to API consumers.
    ///
    /// Default implementation returns an empty list.
    fn get_edges(&self, _actor: &Did) -> Vec<serde_json::Value> {
        Vec::new()
    }

    /// Get all edges in the trust graph (read-only, for API/admin queries).
    ///
    /// Returns serialized edge data as JSON.
    /// Default implementation returns an empty list.
    fn get_all_edges(&self) -> Vec<serde_json::Value> {
        Vec::new()
    }

    /// Submit a trust attestation from this node to a target.
    ///
    /// The trust service creates, signs, and stores the attestation.
    /// Returns serialized attestation bytes for gossip broadcast.
    ///
    /// Default implementation returns an error.
    fn submit_attestation(
        &self,
        _target: &Did,
        _score: f64,
        _labels: Vec<String>,
    ) -> Result<Vec<u8>, String> {
        Err("Attestation submission not supported".to_string())
    }

    /// Remove trust relationship with a target.
    ///
    /// Returns serialized revocation bytes for gossip broadcast.
    /// Default implementation returns an error.
    fn revoke_trust(&self, _target: &Did) -> Result<Vec<u8>, String> {
        Err("Trust revocation not supported".to_string())
    }
}

/// Trust-affecting events that the kernel can report
///
/// These are generic event types that don't expose domain semantics.
/// The trust service interprets them according to its own rules.
#[derive(Debug, Clone)]
pub enum TrustEvent {
    /// Protocol violation detected
    ProtocolViolation {
        /// Severity (0.0-1.0, higher = more severe)
        severity: f64,
        /// Event category for logging
        category: String,
    },
    /// Positive interaction (attestation, successful cooperation)
    PositiveInteraction {
        /// Weight of the interaction (0.0-1.0)
        weight: f64,
    },
    /// Quarantine requested by security subsystem
    QuarantineRequested {
        /// Duration in seconds
        duration_secs: u64,
    },
}

/// Abstract security service interface
///
/// This trait provides misbehavior detection and response functionality.
/// The kernel uses it to record violations and query ban/quarantine status.
pub trait SecurityService: Send + Sync {
    /// Check if an actor is banned
    fn is_banned(&self, actor: &Did) -> bool;

    /// Check if an actor is quarantined
    fn is_quarantined(&self, actor: &Did) -> bool;

    /// Record a violation
    ///
    /// The security service decides how to respond based on violation type
    /// and the actor's history.
    fn record_violation(&self, actor: &Did, violation: SecurityViolation);
}

/// Security violations that the kernel can report
#[derive(Debug, Clone)]
pub enum SecurityViolation {
    /// Invalid cryptographic signature
    InvalidSignature,
    /// Message replay detected
    ReplayAttack,
    /// Rate limit exceeded
    RateLimitExceeded,
    /// Protocol message malformed
    MalformedMessage,
    /// Unauthorized action attempted
    UnauthorizedAction,
    /// Generic violation with severity
    Generic { severity: f64, description: String },
}

/// Abstract governance service interface
///
/// This trait provides governance-related functionality to the kernel without
/// exposing proposals, voting mechanisms, or governance rules directly.
///
/// # Implementation
///
/// Apps that provide governance functionality implement this trait. The kernel
/// uses it to:
/// - Get a PolicyOracle for governance authorization decisions
/// - Query governance parameters (as opaque key-value pairs)
/// - Record governance events
///
/// The kernel NEVER interprets governance rules - it just enforces constraints.
pub trait GovernanceService: Send + Sync {
    /// Get the PolicyOracle for governance authorization
    ///
    /// The oracle converts governance rules into ConstraintSets that the
    /// kernel can enforce blindly.
    fn oracle(&self) -> Arc<dyn PolicyOracle>;

    /// Get a governance parameter value
    ///
    /// Parameters are returned as opaque strings. The kernel may use these
    /// for configuration but NEVER interprets their semantic meaning.
    fn get_parameter(&self, key: &str) -> Option<String>;

    /// List all governance parameter keys
    fn list_parameters(&self) -> Vec<String>;

    /// Record a governance event
    ///
    /// Called by the kernel when governance-relevant actions occur.
    fn record_event(&self, event: GovernanceEvent);
}

/// Governance events that the kernel can report
///
/// These are generic event types that don't expose governance semantics.
/// The governance service interprets them according to its own rules.
#[derive(Debug, Clone)]
pub enum GovernanceEvent {
    /// Parameter was accessed
    ParameterAccessed { key: String, accessor: Did },
    /// Parameter change was requested
    ParameterChangeRequested {
        key: String,
        new_value: String,
        requestor: Did,
    },
    /// Generic governance action
    Action {
        action_type: String,
        actor: Did,
        metadata: std::collections::HashMap<String, String>,
    },
}

/// Abstract ledger service interface
///
/// This trait provides ledger-related functionality to the kernel without
/// exposing account structures, credit policies, or ledger rules directly.
///
/// # Implementation
///
/// Apps that provide ledger functionality implement this trait. The kernel
/// uses it to:
/// - Get a PolicyOracle for transaction authorization decisions
/// - Query account balances (as opaque values)
/// - Record ledger events
///
/// The kernel NEVER interprets ledger rules - it just enforces constraints.
///
/// # Sync/Async Note
///
/// Methods are synchronous for ergonomic kernel integration. Implementations
/// using async locks (e.g. `tokio::RwLock`) should use
/// `tokio::task::block_in_place()` to bridge sync/async contexts safely.
/// This requires a multi-threaded tokio runtime.
pub trait LedgerService: Send + Sync {
    /// Get the PolicyOracle for ledger authorization
    ///
    /// The oracle converts credit policies and account states into ConstraintSets
    /// that the kernel can enforce blindly.
    fn oracle(&self) -> Arc<dyn PolicyOracle>;

    /// Get balance for an account (opaque value)
    ///
    /// The kernel may use this for routing decisions, but NEVER interprets
    /// the semantic meaning of the balance.
    fn balance(&self, account: &Did, currency: &str) -> i64;

    /// Get credit limit for an account (opaque value)
    fn credit_limit(&self, account: &Did, currency: &str) -> i64;

    /// Record a ledger event
    ///
    /// Called by the kernel when ledger-relevant actions occur.
    fn record_event(&self, event: LedgerEvent);

    /// List resource access entries with their idle-violation status.
    ///
    /// The ledger service evaluates anti-speculation rules internally and
    /// returns kernel-level DTOs. `current_time` is seconds since epoch.
    ///
    /// Default implementation returns an empty list (no resource access tracking).
    fn list_enforceable_resources(
        &self,
        _current_time: u64,
    ) -> Result<Vec<ResourceAccessInfo>, String> {
        Ok(Vec::new())
    }

    /// Revoke resource access and persist the change.
    ///
    /// The ledger service handles the domain-level revocation logic
    /// (updating internal state, audit trail, etc.).
    ///
    /// Default implementation returns an error (not supported).
    fn revoke_resource_access(&self, _req: &RevokeResourceAccessRequest) -> Result<(), String> {
        Err("Resource access revocation not supported".to_string())
    }
}

// ============================================================================
// Resource Access DTOs
// ============================================================================

/// A resource access entry as seen by the kernel.
///
/// This is a kernel-level view of resource access. The ledger service
/// translates its internal `ResourceAccess` into this type so the kernel
/// can enforce idle-resource revocation without importing domain types.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResourceAccessInfo {
    /// Unique identifier of the resource
    pub resource_id: String,
    /// DID of the resource holder
    pub holder: Did,
    /// Timestamp when access was granted (seconds since epoch)
    pub granted_at: u64,
    /// Whether the access has already been revoked
    pub is_revoked: bool,
    /// Present when the resource violates idle rules at the queried time
    pub idle_violation: Option<IdleViolationInfo>,
}

/// Information about an idle rule violation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IdleViolationInfo {
    /// How long the resource has been idle (seconds)
    pub idle_seconds: u64,
    /// Maximum allowed idle time (seconds)
    pub max_idle_seconds: u64,
}

/// Request to revoke resource access.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RevokeResourceAccessRequest {
    /// Resource to revoke
    pub resource_id: String,
    /// Reason for revocation (for audit trail)
    pub reason: String,
}

/// Ledger events that the kernel can report
///
/// These are generic event types that don't expose ledger semantics.
/// The ledger service interprets them according to its own rules.
#[derive(Debug, Clone)]
pub enum LedgerEvent {
    /// Transaction submitted
    TransactionSubmitted {
        from: Did,
        to: Did,
        amount: i64,
        currency: String,
    },
    /// Transaction completed
    TransactionCompleted { transaction_id: String },
    /// Transaction failed
    TransactionFailed {
        transaction_id: String,
        reason: String,
    },
    /// Balance queried
    BalanceQueried { account: Did, querier: Did },
}

// ============================================================================
// Cell Service
// ============================================================================

/// Abstract cell management service.
///
/// The kernel uses this to query cell membership and scope topology
/// without knowing the organizational semantics behind cells.
///
/// A "cell" is an HA clustering envelope — a named group of nodes that
/// share identity, state, and capacity. The kernel treats `CellId` as
/// an opaque identifier and `ScopeLevel` as an ordered integer.
///
/// Apps implement this trait to provide cell lifecycle management with
/// their own organizational semantics.
pub trait CellService: Send + Sync {
    /// Get the cell this node belongs to (if any).
    ///
    /// Returns `None` if the node is not a member of any cell
    /// (e.g., an independent commons participant).
    fn local_cell(&self) -> Option<CellId>;

    /// Get the scope level of a given cell.
    ///
    /// Returns `None` if the cell is unknown.
    fn cell_scope(&self, cell_id: &CellId) -> Option<ScopeLevel>;

    /// List DIDs of all members in a cell.
    ///
    /// Returns an empty vec if the cell is unknown.
    fn cell_members(&self, cell_id: &CellId) -> Vec<Did>;

    /// Check if a DID is in the same cell as the local node.
    ///
    /// Returns `false` if the local node has no cell or the peer is unknown.
    fn is_cell_peer(&self, did: &Did) -> bool;

    /// Check if a DID is in the same organization (any cell in the org).
    ///
    /// Returns `false` if the local node has no org or the peer is unknown.
    fn is_org_peer(&self, did: &Did) -> bool;

    /// Get the scope relationship between the local node and a peer.
    ///
    /// Returns the narrowest scope that contains both the local node and
    /// the peer. If the peer is completely unknown, returns `Commons`.
    fn peer_scope(&self, did: &Did) -> ScopeLevel;
}

// ============================================================================
// Service Registry
// ============================================================================

/// Builder for creating domain services
///
/// This allows the daemon to construct services and inject them into the kernel.
/// The kernel never needs to know about concrete implementations.
///
/// Concrete domain handles (ledger, contract runtime, parameter store, etc.)
/// are passed separately via `BootstrapHandles` in `icn-core`, keeping this
/// registry free of domain types.
pub struct ServiceRegistry {
    trust: Option<Arc<dyn TrustService>>,
    security: Option<Arc<dyn SecurityService>>,
    governance: Option<Arc<dyn GovernanceService>>,
    ledger: Option<Arc<dyn LedgerService>>,
    cell: Option<Arc<dyn CellService>>,
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceRegistry {
    /// Create a new empty service registry
    pub fn new() -> Self {
        Self {
            trust: None,
            security: None,
            governance: None,
            ledger: None,
            cell: None,
        }
    }

    /// Register a trust service
    pub fn with_trust(mut self, service: Arc<dyn TrustService>) -> Self {
        self.trust = Some(service);
        self
    }

    /// Register a security service
    pub fn with_security(mut self, service: Arc<dyn SecurityService>) -> Self {
        self.security = Some(service);
        self
    }

    /// Register a governance service
    pub fn with_governance(mut self, service: Arc<dyn GovernanceService>) -> Self {
        self.governance = Some(service);
        self
    }

    /// Register a ledger service
    pub fn with_ledger(mut self, service: Arc<dyn LedgerService>) -> Self {
        self.ledger = Some(service);
        self
    }

    /// Register a cell service
    pub fn with_cell(mut self, service: Arc<dyn CellService>) -> Self {
        self.cell = Some(service);
        self
    }

    /// Get the trust service (if registered)
    pub fn trust(&self) -> Option<&Arc<dyn TrustService>> {
        self.trust.as_ref()
    }

    /// Get the security service (if registered)
    pub fn security(&self) -> Option<&Arc<dyn SecurityService>> {
        self.security.as_ref()
    }

    /// Get the governance service (if registered)
    pub fn governance(&self) -> Option<&Arc<dyn GovernanceService>> {
        self.governance.as_ref()
    }

    /// Get the ledger service (if registered)
    pub fn ledger(&self) -> Option<&Arc<dyn LedgerService>> {
        self.ledger.as_ref()
    }

    /// Get the cell service (if registered)
    pub fn cell(&self) -> Option<&Arc<dyn CellService>> {
        self.cell.as_ref()
    }
}
