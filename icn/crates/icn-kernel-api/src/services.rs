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
use crate::compute::OperatorMode;
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

    /// Ingest a trust revocation from a peer.
    ///
    /// The kernel routes opaque revocation bytes to the trust service.
    /// The trust service owns deserialization, signature verification,
    /// supersedence checks, and edge removal.
    ///
    /// `source` is the DID of the peer that forwarded this revocation
    /// (the gossip subscriber, not necessarily the revocation issuer).
    /// The trust service decides whether to accept or reject it.
    ///
    /// Default implementation does nothing (no revocation support).
    fn ingest_revocation(&self, _bytes: &[u8], _source: &Did) -> Result<(), String> {
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

/// Abstract proposal executor interface
///
/// This trait provides a kernel-level interface for executing governance proposals
/// without exposing proposal types, voting mechanisms, or governance semantics.
///
/// # The Meaning Firewall
///
/// The kernel sees proposals as opaque JSON blobs with string identifiers.
/// The governance app interprets the JSON, executes domain logic, and returns
/// a success/failure result. The kernel routes the result but never understands
/// the proposal content.
///
/// # Implementation
///
/// Apps that provide governance functionality implement this trait. The kernel
/// uses it to:
/// - Execute proposals after they are accepted
/// - Get execution results (success/failure with error messages)
/// - Maintain separation between kernel dispatch and domain logic
///
/// The kernel NEVER interprets proposal payloads - it just routes them to the executor.
pub trait ProposalExecutor: Send + Sync {
    /// Execute a governance proposal
    ///
    /// The kernel calls this method when a proposal is accepted, passing:
    /// - `proposal_id`: Opaque identifier for the proposal
    /// - `payload`: Serialized proposal payload (opaque JSON)
    /// - `decided_at`: Timestamp when the proposal was accepted (seconds since epoch)
    /// - `domain_id`: Governance domain identifier (opaque string)
    ///
    /// The executor:
    /// 1. Deserializes the payload
    /// 2. Validates it against domain rules
    /// 3. Executes the corresponding action (ledger transfer, parameter change, etc.)
    /// 4. Returns success or failure with an error message
    ///
    /// # Error Handling
    ///
    /// Execution errors are returned as `Err(String)`. The kernel logs these
    /// errors but does not interpret their meaning. The governance app decides
    /// whether to retry, skip, or escalate failures.
    ///
    /// # Sync/Async Note
    ///
    /// This method is synchronous for ergonomic kernel integration. Implementations
    /// may use `tokio::task::block_in_place()` if they need async operations.
    fn execute_proposal(
        &self,
        proposal_id: &str,
        payload: &serde_json::Value,
        decided_at: u64,
        domain_id: &str,
    ) -> Result<(), String>;
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

    /// Submit a treasury entry to the ledger with decision provenance.
    ///
    /// This is the kernel-safe interface for treasury operations to write
    /// ledger entries. The ledger service handles:
    /// - Entry construction and validation
    /// - Signature generation
    /// - Gossip propagation
    ///
    /// Returns the content hash of the created entry.
    ///
    /// # Arguments
    /// * `entry` - Treasury entry details (kernel-safe DTO)
    ///
    /// # Pilot Invariant
    /// The resulting ledger entry MUST carry both `decision_receipt_id` and
    /// `decision_hash` for provenance tracking.
    fn submit_treasury_entry(
        &self,
        _entry: TreasuryEntryRequest,
    ) -> Result<TreasuryEntryResult, String> {
        Err("Treasury entry submission not supported".to_string())
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

// ============================================================================
// Treasury Entry DTOs (kernel-safe ledger interface)
// ============================================================================

/// Request to submit a treasury entry to the ledger.
///
/// This is a kernel-safe DTO for treasury operations. The ledger service
/// translates this into domain-specific journal entries.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TreasuryEntryRequest {
    /// Treasury being operated on
    pub treasury_id: String,
    /// Type of treasury operation
    pub operation_type: TreasuryOperationType,
    /// Amount being transferred
    pub amount: i64,
    /// Currency code
    pub currency: String,
    /// Recipient DID (for spend operations)
    pub recipient: Option<String>,
    /// Human-readable memo
    pub memo: String,

    // Provenance fields (pilot-critical)
    /// Decision receipt ID that authorized this entry (node-local reference)
    pub decision_receipt_id: String,
    /// Canonical decision hash (cross-node equality anchor)
    pub decision_hash: String,
}

/// Type of treasury operation (mirrors TreasuryOperationType from governance.rs)
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TreasuryOperationType {
    Spend,
    Allocate,
    Transfer,
    CreateBudget,
    DistributeSurplus,
    RedeemShares,
    IssueBond,
}

/// Result of a treasury entry submission.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TreasuryEntryResult {
    /// Content hash of the created ledger entry
    pub entry_hash: String,
    /// The decision_receipt_id echoed back for verification
    pub decision_receipt_id: String,
    /// The decision_hash echoed back for verification
    pub decision_hash: String,
}

// ============================================================================
// SettlementIntent → TreasuryEntryRequest Conversion (INT-4)
// ============================================================================

use crate::economics::{AssetType, SettlementIntent};

impl TreasuryEntryRequest {
    /// Convert a SettlementIntent to a TreasuryEntryRequest.
    ///
    /// This is the canonical ledger binding pattern. The conversion is deterministic:
    /// the same SettlementIntent always produces the same TreasuryEntryRequest.
    ///
    /// # Arguments
    /// * `intent` - The settlement intent to convert
    /// * `treasury_id` - The treasury account ID (derived from scope/context)
    ///
    /// # Returns
    /// A TreasuryEntryRequest ready for ledger submission, or an error if the
    /// amount exceeds i64::MAX (which would be ~9.2 quintillion units).
    ///
    /// # Errors
    /// Returns an error if `intent.amount` exceeds `i64::MAX`.
    pub fn from_settlement_intent(
        intent: &SettlementIntent,
        treasury_id: &str,
    ) -> Result<Self, std::num::TryFromIntError> {
        let operation_type = match intent.asset {
            AssetType::Fungible => TreasuryOperationType::Spend,
            AssetType::Service { .. } => TreasuryOperationType::Spend,
            AssetType::Claim { .. } => TreasuryOperationType::IssueBond,
            AssetType::Consumable { .. } => TreasuryOperationType::Spend,
            AssetType::Depreciating { .. } => TreasuryOperationType::Allocate,
            AssetType::Transformable { .. } => TreasuryOperationType::Allocate,
        };

        // Convert canonical hash to hex string for provenance
        let decision_hash_hex = hex::encode(intent.decision_hash);

        // Safe conversion - returns error if amount exceeds i64::MAX
        let amount = i64::try_from(intent.amount)?;

        Ok(TreasuryEntryRequest {
            treasury_id: treasury_id.to_string(),
            operation_type,
            amount,
            currency: intent.unit.clone(),
            recipient: Some(intent.to.clone()),
            memo: intent.memo.clone().unwrap_or_default(),
            decision_receipt_id: intent.decision_receipt_id.clone(),
            decision_hash: decision_hash_hex,
        })
    }

    /// Create a TreasuryEntryRequest from a SettlementIntent with an AllocationReceipt
    /// as provenance anchor.
    ///
    /// This includes the allocation_hash in provenance metadata.
    ///
    /// # Errors
    /// Returns an error if `intent.amount` exceeds `i64::MAX`.
    pub fn from_settlement_with_allocation(
        intent: &SettlementIntent,
        treasury_id: &str,
        allocation_hash: &crate::receipts::Hash,
    ) -> Result<Self, std::num::TryFromIntError> {
        let mut request = Self::from_settlement_intent(intent, treasury_id)?;
        // Include allocation hash in memo for full provenance chain
        let allocation_hex = hex::encode(allocation_hash);
        if request.memo.is_empty() {
            request.memo = format!("allocation:{}", allocation_hex);
        } else {
            request.memo = format!("{} | allocation:{}", request.memo, allocation_hex);
        }
        Ok(request)
    }
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
// Federation Service
// ============================================================================

/// Request to join a federation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FederationJoinRequest {
    /// DID of the cooperative requesting to join
    pub coop_did: String,
    /// Name of the cooperative
    pub coop_name: String,
    /// ID of the federation to join (or create)
    pub federation_id: String,
    /// Gateway endpoints for this cooperative
    pub gateway_endpoints: Vec<String>,
    /// Decision receipt ID that authorized this join
    pub decision_receipt_id: String,
    /// Hash of the decision that authorized this join
    pub decision_hash: String,
}

/// Result of a federation join operation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FederationJoinResult {
    /// Whether the operation succeeded
    pub success: bool,
    /// State change hash (for verification)
    pub state_change_hash: String,
    /// Error message if failed
    pub error: Option<String>,
}

/// Request to vouch for a cooperative
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FederationVouchRequest {
    /// DID of the cooperative doing the vouching
    pub voucher_did: String,
    /// DID of the cooperative being vouched for
    pub vouchee_did: String,
    /// Trust score to assign (0.0-1.0)
    pub trust_score: f64,
    /// Decision receipt ID that authorized this vouch
    pub decision_receipt_id: String,
    /// Hash of the decision that authorized this vouch
    pub decision_hash: String,
}

/// Result of a vouch operation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FederationVouchResult {
    /// Whether the operation succeeded
    pub success: bool,
    /// State change hash (for verification)
    pub state_change_hash: String,
    /// Error message if failed
    pub error: Option<String>,
}

/// Abstract federation management service.
///
/// The kernel uses this to register cooperatives and vouches without
/// knowing the underlying federation semantics. Apps implement this
/// trait to provide actual federation registry operations.
///
/// # Provenance Tracking
///
/// All federation operations carry `decision_receipt_id` and `decision_hash`
/// to maintain the audit chain from governance decisions to state changes.
pub trait FederationService: Send + Sync {
    /// Register a cooperative joining a federation.
    ///
    /// This creates a durable record that can be queried later.
    /// Returns state_change_hash for verification.
    fn join_federation(
        &self,
        request: FederationJoinRequest,
    ) -> Result<FederationJoinResult, anyhow::Error>;

    /// Record a vouch from one cooperative to another.
    ///
    /// This creates a durable vouch record in the registry.
    /// Returns state_change_hash for verification.
    fn vouch_for_cooperative(
        &self,
        request: FederationVouchRequest,
    ) -> Result<FederationVouchResult, anyhow::Error>;

    /// Check if a cooperative is registered in the federation.
    fn is_registered(&self, coop_did: &str) -> bool;

    /// Get provenance info for a cooperative registration.
    ///
    /// Returns (decision_receipt_id, decision_hash) if available.
    fn get_registration_provenance(&self, coop_did: &str) -> Option<(String, String)>;
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
/// # Operator Boundary Rule (E5)
///
/// **A cell is operated by exactly one entity** — a cooperative, community,
/// federation, or individual. Mixed-operator cells are forbidden because:
///
/// - Trust within a cell is implicit (nodes share keys, state, workloads)
/// - Cross-entity clustering belongs at federation scope, not cell scope
/// - Mixed operators would break the security boundary (any node can act as the cell)
///
/// The `operator_entity` method returns the single entity that operates all
/// nodes in this cell. Cell formation must validate that all joining nodes
/// share the same operator entity ID.
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

    /// Get the operator DID for a cell (E5: Operator Boundary).
    ///
    /// Returns the DID of the single entity that operates this cell.
    /// All nodes in a cell MUST share the same operator DID.
    ///
    /// # Cell Formation Validation
    ///
    /// When a node attempts to join a cell, the following must hold:
    /// - the node's configured operator DID matches `cell.cell_operator()`
    ///
    /// Implementations should reject cell join attempts that would violate
    /// the single-operator rule (i.e., when `cell.cell_operator()` is set and
    /// does not match the joining node's operator DID).
    ///
    /// Returns `None` if the cell is unknown or the operator DID is not set.
    fn cell_operator(&self, cell_id: &CellId) -> Option<Did> {
        // Default: not implemented (for backward compatibility)
        let _ = cell_id;
        None
    }

    /// Get the operator mode for a cell (E6: Individual Node Ownership).
    ///
    /// Returns the operator mode of the cell, which determines whether it's
    /// operated by an organization or an individual.
    ///
    /// Returns `None` if the cell is unknown or operator mode is not set.
    fn cell_operator_mode(&self, cell_id: &CellId) -> Option<OperatorMode> {
        // Default: not implemented (for backward compatibility)
        let _ = cell_id;
        None
    }

    /// Check if a node with the given operator mode can join a cell.
    ///
    /// This enforces the E6 operator mode compatibility rule:
    /// - Nodes can only join cells with compatible operator modes
    /// - Compatible means same mode variant AND same operator ID
    ///
    /// Returns `true` if:
    /// - The cell has no operator mode set (backward compatibility)
    /// - The cell's operator mode is compatible with the joining node's mode
    ///
    /// Returns `false` if the modes are incompatible.
    fn can_join_cell(&self, cell_id: &CellId, node_mode: &OperatorMode) -> bool {
        match self.cell_operator_mode(cell_id) {
            Some(cell_mode) => cell_mode.is_compatible_with(node_mode),
            None => true, // No mode set = backward compatible
        }
    }
}

// ============================================================================
// Control Service (Governance Actions)
// ============================================================================

/// Request to veto a proposal
#[derive(Debug, Clone)]
pub struct VetoProposalRequest {
    /// The proposal being vetoed
    pub target_proposal_id: String,
    /// Reason for the veto
    pub veto_reason: String,
    /// The governance domain where this proposal exists
    pub domain_id: String,
    /// The entity exercising the veto power
    pub vetoer_did: String,
    /// Links back to the veto proposal's governance decision
    pub decision_receipt_id: String,
    /// Canonical content hash for verification
    pub decision_hash: String,
}

/// Result of vetoing a proposal
#[derive(Debug, Clone)]
pub struct VetoProposalResult {
    /// Whether the veto was successful
    pub success: bool,
    /// Human-readable outcome message
    pub message: String,
    /// Hash of the state change (proposal status → Vetoed)
    pub state_change_hash: Option<String>,
    /// Timestamp of the veto
    pub vetoed_at: u64,
}

/// Request to force close a proposal
#[derive(Debug, Clone)]
pub struct ForceCloseProposalRequest {
    /// The proposal being force-closed
    pub target_proposal_id: String,
    /// Reason for force closing
    pub close_reason: String,
    /// The forced outcome (e.g., "Rejected", "Expired")
    pub forced_outcome: String,
    /// The governance domain
    pub domain_id: String,
    /// The entity forcing the close
    pub closer_did: String,
    /// Links back to the force-close proposal's governance decision
    pub decision_receipt_id: String,
    /// Canonical content hash for verification
    pub decision_hash: String,
}

/// Result of force closing a proposal
#[derive(Debug, Clone)]
pub struct ForceCloseProposalResult {
    /// Whether the force close was successful
    pub success: bool,
    /// Human-readable outcome message
    pub message: String,
    /// Hash of the state change (proposal status → ForceClosed)
    pub state_change_hash: Option<String>,
    /// The final outcome that was applied
    pub final_outcome: String,
    /// Timestamp of the close
    pub closed_at: u64,
}

/// Abstract control service for governance actions.
///
/// The kernel uses this service to execute governance control effects
/// (veto, force close) without knowing about the internal governance state.
///
/// Apps implement this trait to provide actual governance state mutations.
///
/// # Provenance Tracking
///
/// All control operations carry `decision_receipt_id` and `decision_hash`
/// to maintain the audit chain from governance decisions to state changes.
pub trait ControlService: Send + Sync {
    /// Veto a pending proposal.
    ///
    /// This transitions the target proposal to a Vetoed state.
    /// Returns state_change_hash for verification.
    fn veto_proposal(
        &self,
        request: VetoProposalRequest,
    ) -> Result<VetoProposalResult, anyhow::Error>;

    /// Force close a proposal with a specified outcome.
    ///
    /// This transitions the target proposal to a ForceClosed state
    /// with the specified forced outcome.
    /// Returns state_change_hash for verification.
    fn force_close_proposal(
        &self,
        request: ForceCloseProposalRequest,
    ) -> Result<ForceCloseProposalResult, anyhow::Error>;

    /// Check if a proposal can be vetoed.
    ///
    /// Returns true if the proposal exists and is in a vetoable state.
    fn can_veto(&self, target_proposal_id: &str, domain_id: &str) -> bool;

    /// Check if a proposal can be force closed.
    ///
    /// Returns true if the proposal exists and can be force closed.
    fn can_force_close(&self, target_proposal_id: &str, domain_id: &str) -> bool;
}

// ============================================================================
// Membership Service
// ============================================================================

/// Request to add a new member to an entity (cooperative, community, etc.)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AddMemberRequest {
    /// Entity (cooperative/community) ID
    pub entity_id: String,
    /// DID of the new member
    pub member_did: String,
    /// Role assigned to the member (e.g., "Worker", "Member", "BoardMember")
    pub role: String,
    /// Membership tier (e.g., "Founder", "Standard", "Provisional")
    pub tier: String,
    /// Decision receipt ID that authorized this addition
    pub decision_receipt_id: String,
    /// Hash of the decision that authorized this addition
    pub decision_hash: String,
}

/// Result of adding a member
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AddMemberResult {
    /// Whether the operation succeeded
    pub success: bool,
    /// State change hash for verification
    pub state_change_hash: String,
    /// Error message if failed
    pub error: Option<String>,
}

/// Request to remove a member from an entity
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RemoveMemberRequest {
    /// Entity (cooperative/community) ID
    pub entity_id: String,
    /// DID of the member to remove
    pub member_did: String,
    /// Reason for removal
    pub reason: String,
    /// Decision receipt ID that authorized this removal
    pub decision_receipt_id: String,
    /// Hash of the decision that authorized this removal
    pub decision_hash: String,
}

/// Result of removing a member
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RemoveMemberResult {
    /// Whether the operation succeeded
    pub success: bool,
    /// State change hash for verification
    pub state_change_hash: String,
    /// Error message if failed
    pub error: Option<String>,
}

/// Request to update a member's role or tier
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateMemberRequest {
    /// Entity (cooperative/community) ID
    pub entity_id: String,
    /// DID of the member to update
    pub member_did: String,
    /// New role (if changing)
    pub new_role: Option<String>,
    /// New tier (if changing)
    pub new_tier: Option<String>,
    /// Decision receipt ID that authorized this update
    pub decision_receipt_id: String,
    /// Hash of the decision that authorized this update
    pub decision_hash: String,
}

/// Result of updating a member
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateMemberResult {
    /// Whether the operation succeeded
    pub success: bool,
    /// State change hash for verification
    pub state_change_hash: String,
    /// Error message if failed
    pub error: Option<String>,
}

/// Request to freeze a member (suspend rights temporarily)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FreezeMemberRequest {
    /// Entity (cooperative/community) ID
    pub entity_id: String,
    /// DID of the member to freeze
    pub member_did: String,
    /// Reason for freezing
    pub reason: String,
    /// Duration in seconds (None = indefinite)
    pub duration_secs: Option<u64>,
    /// Decision receipt ID that authorized this freeze
    pub decision_receipt_id: String,
    /// Hash of the decision that authorized this freeze
    pub decision_hash: String,
}

/// Result of freezing a member
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FreezeMemberResult {
    /// Whether the operation succeeded
    pub success: bool,
    /// State change hash for verification
    pub state_change_hash: String,
    /// When the freeze expires (if duration was specified)
    pub expires_at: Option<u64>,
    /// Error message if failed
    pub error: Option<String>,
}

/// Request to unfreeze a member (restore rights)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UnfreezeMemberRequest {
    /// Entity (cooperative/community) ID
    pub entity_id: String,
    /// DID of the member to unfreeze
    pub member_did: String,
    /// Decision receipt ID that authorized this unfreeze
    pub decision_receipt_id: String,
    /// Hash of the decision that authorized this unfreeze
    pub decision_hash: String,
}

/// Result of unfreezing a member
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UnfreezeMemberResult {
    /// Whether the operation succeeded
    pub success: bool,
    /// State change hash for verification
    pub state_change_hash: String,
    /// Error message if failed
    pub error: Option<String>,
}

/// Abstract membership management service.
///
/// The kernel uses this to manage entity membership without knowing the
/// underlying organizational semantics. Apps implement this trait to provide
/// actual membership operations backed by their storage.
///
/// # Provenance Tracking
///
/// All membership operations carry `decision_receipt_id` and `decision_hash`
/// to maintain the audit chain from governance decisions to state changes.
///
/// # Durable State Changes
///
/// All operations must produce durable state changes that survive restarts.
/// The `state_change_hash` in results allows verification of state transitions.
pub trait MembershipService: Send + Sync {
    /// Add a new member to an entity.
    ///
    /// This creates a durable membership record. The member starts in
    /// a pending or active state depending on the entity's policies.
    fn add_member(&self, request: AddMemberRequest) -> Result<AddMemberResult, anyhow::Error>;

    /// Remove a member from an entity.
    ///
    /// This transitions the member to a removed state. The record is
    /// kept for audit purposes but the member loses all rights.
    fn remove_member(
        &self,
        request: RemoveMemberRequest,
    ) -> Result<RemoveMemberResult, anyhow::Error>;

    /// Update a member's role or tier.
    ///
    /// This modifies the member's role and/or tier while keeping their
    /// membership active.
    fn update_member(
        &self,
        request: UpdateMemberRequest,
    ) -> Result<UpdateMemberResult, anyhow::Error>;

    /// Freeze a member (suspend their rights temporarily).
    ///
    /// A frozen member retains their membership but cannot exercise
    /// rights (voting, resource access, etc.) until unfrozen.
    fn freeze_member(
        &self,
        request: FreezeMemberRequest,
    ) -> Result<FreezeMemberResult, anyhow::Error>;

    /// Unfreeze a member (restore their rights).
    ///
    /// This restores a frozen member to active status.
    fn unfreeze_member(
        &self,
        request: UnfreezeMemberRequest,
    ) -> Result<UnfreezeMemberResult, anyhow::Error>;

    /// Check if a DID is a member of an entity.
    fn is_member(&self, entity_id: &str, member_did: &str) -> bool;

    /// Check if a DID is an active (non-frozen, non-removed) member.
    fn is_active_member(&self, entity_id: &str, member_did: &str) -> bool;

    /// Get provenance for a membership operation.
    ///
    /// Returns (decision_receipt_id, decision_hash) for the operation
    /// that created/modified this membership.
    fn get_membership_provenance(
        &self,
        entity_id: &str,
        member_did: &str,
    ) -> Option<(String, String)>;
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
    proposal_executor: Option<Arc<dyn ProposalExecutor>>,
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
            proposal_executor: None,
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

    /// Register a proposal executor
    pub fn with_proposal_executor(mut self, executor: Arc<dyn ProposalExecutor>) -> Self {
        self.proposal_executor = Some(executor);
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

    /// Get the proposal executor (if registered)
    pub fn proposal_executor(&self) -> Option<&Arc<dyn ProposalExecutor>> {
        self.proposal_executor.as_ref()
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economics::{AssetType, SettlementIntent};
    use crate::receipts::{AllocationReceipt, CanonicalReceipt};

    fn make_test_intent() -> SettlementIntent {
        SettlementIntent::new(
            "decision-001",
            [42u8; 32],
            "treasury",
            "recipient-1",
            1000,
            "hours",
        )
        .with_timestamp(1000000)
    }

    #[test]
    fn test_from_settlement_intent_basic() {
        let intent = make_test_intent();
        let request = TreasuryEntryRequest::from_settlement_intent(&intent, "treasury-main");

        assert_eq!(request.treasury_id, "treasury-main");
        assert_eq!(request.amount, 1000);
        assert_eq!(request.currency, "hours");
        assert_eq!(request.recipient, Some("recipient-1".to_string()));
        assert_eq!(request.decision_receipt_id, "decision-001");
        assert_eq!(request.operation_type, TreasuryOperationType::Spend);
    }

    #[test]
    fn test_from_settlement_intent_decision_hash_preserved() {
        let intent = make_test_intent();
        let request = TreasuryEntryRequest::from_settlement_intent(&intent, "treasury-main");

        // Verify decision hash is correctly hex-encoded
        let expected_hex = hex::encode([42u8; 32]);
        assert_eq!(request.decision_hash, expected_hex);
    }

    #[test]
    fn test_from_settlement_intent_asset_type_mapping() {
        // Fungible → Spend
        let intent = SettlementIntent::new("d1", [1u8; 32], "a", "b", 100, "credits");
        let req = TreasuryEntryRequest::from_settlement_intent(&intent, "t");
        assert_eq!(req.operation_type, TreasuryOperationType::Spend);

        // Service → Spend
        let intent = SettlementIntent::new("d2", [2u8; 32], "a", "b", 100, "hours").with_asset(
            AssetType::Service {
                duration_secs: Some(3600),
            },
        );
        let req = TreasuryEntryRequest::from_settlement_intent(&intent, "t");
        assert_eq!(req.operation_type, TreasuryOperationType::Spend);

        // Claim → IssueBond
        let intent = SettlementIntent::new("d3", [3u8; 32], "a", "b", 100, "bond").with_asset(
            AssetType::Claim {
                due_by: None,
                conditions: None,
            },
        );
        let req = TreasuryEntryRequest::from_settlement_intent(&intent, "t");
        assert_eq!(req.operation_type, TreasuryOperationType::IssueBond);

        // Depreciating → Allocate
        let intent = SettlementIntent::new("d4", [4u8; 32], "a", "b", 100, "equipment").with_asset(
            AssetType::Depreciating {
                schedule: crate::economics::DepreciationSchedule::Linear {
                    lifespan_secs: 86400,
                },
            },
        );
        let req = TreasuryEntryRequest::from_settlement_intent(&intent, "t");
        assert_eq!(req.operation_type, TreasuryOperationType::Allocate);
    }

    #[test]
    fn test_from_settlement_with_allocation_includes_hash() {
        let intent = make_test_intent();
        let allocation_hash = [99u8; 32];
        let request = TreasuryEntryRequest::from_settlement_with_allocation(
            &intent,
            "treasury-main",
            &allocation_hash,
        );

        assert!(request.memo.contains("allocation:"));
        assert!(request.memo.contains(&hex::encode([99u8; 32])));
    }

    #[test]
    fn test_conversion_determinism() {
        let intent = make_test_intent();

        let req1 = TreasuryEntryRequest::from_settlement_intent(&intent, "treasury").unwrap();
        let req2 = TreasuryEntryRequest::from_settlement_intent(&intent, "treasury").unwrap();

        // Same intent → same request
        assert_eq!(req1.treasury_id, req2.treasury_id);
        assert_eq!(req1.amount, req2.amount);
        assert_eq!(req1.currency, req2.currency);
        assert_eq!(req1.recipient, req2.recipient);
        assert_eq!(req1.decision_hash, req2.decision_hash);
        assert_eq!(req1.decision_receipt_id, req2.decision_receipt_id);
    }

    #[test]
    fn test_full_chain_provenance() {
        // Simulate: Decision → AllocationReceipt → SettlementIntent → TreasuryEntryRequest
        let decision_hash = [42u8; 32];

        // Create allocation receipt from decision
        let allocation =
            AllocationReceipt::new(decision_hash, ScopeLevel::Org).with_timestamp(1000);
        let allocation_hash = allocation.canonical_hash();

        // Create settlement intent linked to decision (provenance is set in new())
        let intent = SettlementIntent::new(
            "decision-001",
            decision_hash,
            "treasury",
            "member",
            500,
            "hours",
        );

        // Convert to treasury request with allocation provenance
        let request = TreasuryEntryRequest::from_settlement_with_allocation(
            &intent,
            "treasury-main",
            &allocation_hash,
        )
        .unwrap();

        // Verify full chain is preserved
        assert_eq!(request.decision_receipt_id, "decision-001");
        let expected_decision_hex = hex::encode(decision_hash);
        assert_eq!(request.decision_hash, expected_decision_hex);
        let expected_allocation_hex = hex::encode(allocation_hash);
        assert!(request.memo.contains(&expected_allocation_hex));
    }
}
