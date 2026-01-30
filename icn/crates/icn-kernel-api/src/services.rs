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
/// # Transition Handles
///
/// During the kernel/app separation migration, some components may still need
/// direct access to domain objects (e.g., TrustGraph for MisbehaviorDetector).
/// The `raw_handles` field provides type-erased storage for these transition
/// needs. These should be removed as components are fully migrated.
///
/// # Raw Handle Type Pattern
///
/// `raw_handle<T>` requires `T: Sized`, so `dyn Trait` objects cannot be stored
/// directly. Use the appropriate pattern based on the stored type:
///
/// **Concrete type with trait upcast** (when consumers need a trait object):
/// ```ignore
/// // Store: Arc<SledParameterStore> (concrete, Sized)
/// registry.with_raw_handle(ServiceRegistry::PROTOCOL_PARAM_STORE_KEY, store);
/// // Retrieve concrete, then upcast:
/// let store: Arc<SledParameterStore> = registry.raw_handle(KEY)?;
/// let trait_obj: Arc<dyn ProtocolParameterStore> = store;
/// ```
///
/// **Wrapped type** (when the wrapper is Sized, e.g., `RwLock<T>`):
/// ```ignore
/// // Store and retrieve directly:
/// registry.with_raw_handle(ServiceRegistry::LEDGER_KEY, ledger_handle);
/// let handle: Arc<RwLock<Ledger>> = registry.raw_handle(KEY)?;
/// ```
pub struct ServiceRegistry {
    trust: Option<Arc<dyn TrustService>>,
    security: Option<Arc<dyn SecurityService>>,
    governance: Option<Arc<dyn GovernanceService>>,
    ledger: Option<Arc<dyn LedgerService>>,
    cell: Option<Arc<dyn CellService>>,
    /// Type-erased handles for transition period
    ///
    /// Keys use string identifiers like "trust_graph", "ledger_handle", etc.
    /// Values are type-erased via Any trait. The supervisor downcasts as needed.
    ///
    /// **This is a transition mechanism and should be removed when migration is complete.**
    raw_handles: std::collections::HashMap<String, Arc<dyn std::any::Any + Send + Sync>>,
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceRegistry {
    // Raw handle key constants — use these instead of string literals to prevent typos.
    // A typo in the key results in silent `None` at runtime.

    /// Key for `Arc<RwLock<TrustGraph>>` raw handle
    pub const TRUST_GRAPH_KEY: &str = "trust_graph";
    /// Key for `Arc<SledParameterStore>` raw handle (concrete type; upcast after retrieval)
    pub const PROTOCOL_PARAM_STORE_KEY: &str = "protocol_parameter_store";
    /// Key for `Arc<RwLock<Ledger>>` raw handle
    pub const LEDGER_KEY: &str = "ledger";
    /// Key for `Arc<SledStore>` raw handle (shared with DisputeManager/TreasuryManager)
    pub const LEDGER_STORE_KEY: &str = "ledger_store";

    /// Create a new empty service registry
    pub fn new() -> Self {
        Self {
            trust: None,
            security: None,
            governance: None,
            ledger: None,
            cell: None,
            raw_handles: std::collections::HashMap::new(),
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

    /// Register a raw handle for transition period
    ///
    /// **This is a transition mechanism.** Use this to pass domain objects
    /// (like TrustGraph handles) that some components still need directly.
    /// These should be migrated to use proper service interfaces.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let trust_graph = Arc::new(RwLock::new(TrustGraph::new(...)));
    /// registry = registry.with_raw_handle("trust_graph", trust_graph);
    /// ```
    pub fn with_raw_handle<T: Send + Sync + 'static>(mut self, key: &str, handle: Arc<T>) -> Self {
        self.raw_handles.insert(key.to_string(), handle);
        self
    }

    /// Get a raw handle by key, downcasting to the expected type
    ///
    /// **This is a transition mechanism.** Returns None if key doesn't exist
    /// or type doesn't match.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let trust_graph: Option<Arc<RwLock<TrustGraph>>> = registry.raw_handle("trust_graph");
    /// ```
    pub fn raw_handle<T: Send + Sync + 'static>(&self, key: &str) -> Option<Arc<T>> {
        self.raw_handles
            .get(key)
            .and_then(|any| any.clone().downcast::<T>().ok())
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
