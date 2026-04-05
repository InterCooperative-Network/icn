//! System-wide event types and emitter trait
//!
//! Provides the `SystemEvent` enum and `EventEmitter` trait for inter-actor
//! communication. The kernel treats events as opaque coordination primitives —
//! it routes them without interpreting their semantics.

use async_trait::async_trait;
use std::sync::Arc;

/// System-wide events that actors can emit and subscribe to.
///
/// All fields use primitive or stringly-typed values so that kernel-api
/// remains free of domain crate dependencies.
#[derive(Clone, Debug)]
pub enum SystemEvent {
    /// A governance proposal was accepted
    ProposalAccepted {
        /// Unique identifier for the proposal
        proposal_id: String,
        /// Domain in which the proposal was made
        domain_id: String,
        /// Payload describing the proposal action (serialized)
        payload: serde_json::Value,
        /// Unix timestamp when the proposal was decided
        decided_at: u64,
        /// Canonical content hash of the accepted proposal payload.
        ///
        /// A BLAKE3 hash of the serialized proposal payload JSON. Allows
        /// third-party verification against the original proposal content.
        /// Superseded by `governance_decision_hash` when present.
        /// When both are absent, `create_effect_subscription` falls back to `blake3(receipt_id)`.
        canonical_payload_hash: Option<String>,
        /// Canonical governance decision hash.
        ///
        /// Hex-encoded SHA3/BLAKE3 hash computed by `GovernanceDecisionReceipt` from
        /// (proposal_id + domain_id + outcome + vote_tally + vote_hash). This is the
        /// same hash that keys the governance receipt, allocation receipts, execution
        /// record, and GovernanceProofV2 — making it the authoritative cross-reference
        /// for `icnctl audit verify`.
        ///
        /// When present, `create_effect_subscription` uses this as `decision_hash` in
        /// journal entry provenance, enabling hash-level verification of the journal
        /// entry against the governance receipt chain without trusting the operator.
        ///
        /// Introduced in sprint 27 to close the treasury-spend provenance seam at the
        /// canonical hash level (superseding `canonical_payload_hash` for provenance).
        governance_decision_hash: Option<String>,
    },

    /// A governance proposal was rejected or failed to reach quorum
    ProposalRejected {
        /// Unique identifier for the proposal
        proposal_id: String,
        /// Domain in which the proposal was made
        domain_id: String,
        /// Unix timestamp when the proposal was decided
        decided_at: u64,
    },

    /// A ledger transaction was executed
    TransactionExecuted {
        /// Hash of the ledger entry
        entry_hash: [u8; 32],
        /// Source DID (as string)
        from: String,
        /// Destination DID (as string)
        to: String,
        /// Transaction amount
        amount: i64,
        /// Currency identifier
        currency: String,
    },

    /// A contract was executed
    ContractExecuted {
        /// Contract identifier
        contract_id: String,
        /// Execution outcome as JSON
        outcome: serde_json::Value,
    },

    /// A proposal execution failed (proposal was accepted but could not be applied)
    ProposalExecutionFailed {
        /// Unique identifier for the proposal
        proposal_id: String,
        /// Type of the proposal (e.g., "protocol_change", "treasury")
        proposal_type: String,
        /// Human-readable error message
        error: String,
        /// Unix timestamp when the failure occurred
        failed_at: u64,
    },

    /// Protocol parameters were initialized (first run)
    ProtocolParametersInitialized {
        /// Number of parameters initialized
        count: usize,
        /// Unix timestamp when the initialization occurred
        initialized_at: u64,
    },

    /// Protocol parameter store was loaded (existing parameters)
    ProtocolParametersLoaded {
        /// Number of parameters loaded
        count: usize,
        /// Unix timestamp when the store was loaded
        loaded_at: u64,
    },

    /// A protocol parameter was changed (for audit logging)
    ProtocolParameterChanged {
        /// Parameter ID that was changed
        parameter_id: String,
        /// Old value (serialized as string for logging)
        old_value: String,
        /// New value (serialized as string for logging)
        new_value: String,
        /// Proposal ID that authorized the change (if any)
        proposal_id: Option<String>,
        /// DID of the actor who made the change
        changed_by: Option<String>,
        /// Unix timestamp when the change occurred
        changed_at: u64,
    },
}

/// Callback function for event subscribers
pub type EventCallback = Arc<dyn Fn(SystemEvent) + Send + Sync>;

/// Trait for emitting system events.
///
/// Implementations handle the actual broadcast mechanism (e.g., in-memory
/// subscriber list, channel-based dispatch). The kernel provides a default
/// `EventBus` implementation.
#[async_trait]
pub trait EventEmitter: Send + Sync {
    /// Emit an event to all subscribers
    async fn emit(&self, event: SystemEvent);
}
