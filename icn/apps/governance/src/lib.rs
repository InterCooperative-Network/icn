//! ICN Governance Actor
//!
//! This crate provides the [`GovernanceActor`] for distributed decision-making
//! across the ICN network. It manages governance state, proposal lifecycle,
//! voting, and outcome evaluation.
//!
//! ## Architecture
//!
//! The governance actor is a domain-specific application that sits above the
//! kernel layer. It depends on kernel-api traits (e.g., [`EventEmitter`],
//! [`ProtocolParameterStore`], [`GovernanceExecutor`]) rather than concrete
//! kernel implementations, maintaining clean separation between kernel
//! mechanisms and domain semantics.
//!
//! ## Effect Translation Boundary
//!
//! The [`handlers`] module translates accepted domain proposals into
//! kernel-safe effects. Proposal execution is then delegated to kernel
//! effect dispatch.
//!
//! [`EventEmitter`]: icn_kernel_api::events::EventEmitter
//! [`ProtocolParameterStore`]: icn_kernel_api::protocol_params::ProtocolParameterStore
//! [`GovernanceExecutor`]: icn_kernel_api::governance::GovernanceExecutor
//! [`TreasuryExecutor`]: icn_kernel_api::governance::TreasuryExecutor
//! [`ProtocolExecutor`]: icn_kernel_api::governance::ProtocolExecutor

pub mod actor;
pub mod events;
pub mod executor;
pub mod handlers;
pub mod http;
pub mod init;
pub mod manager;
pub mod receipt_backend;
pub mod registry;
pub mod state_store;

pub use actor::{GovernanceActor, GovernanceCommand, GovernanceConfigLite, GovernanceHandle};
pub use events::{GovernanceEventEmitter, NoopEventEmitter};
pub use executor::{ExecutionCallback, GovernanceProposalExecutor};
pub use handlers::translate_payload_to_effects;
pub use manager::{
    GovernanceManager, SledActionItemStore, SledActivityStore, SledMeetingStore, SledStructureStore,
};
pub use receipt_backend::GovernanceReceiptBackend;
pub use state_store::{GovernanceStateStore, SledGovernanceStateStore};

// Re-export registry types for decision/meeting management
pub use registry::{DecisionFilter, DecisionIndexEntry, DecisionRegistry, DecisionStatus, Meeting};

// Re-export kernel effect types for kernel/app boundary
pub use icn_kernel_api::effects::{
    ControlEffect, EffectResult, FederationEffect, KernelEffect, MembershipEffect, ProtocolEffect,
    TreasuryEffect,
};

// Re-export kernel governance traits for convenience
pub use icn_kernel_api::governance::{
    DecisionReceiptId, ExecutionOutcome, GovernanceExecutor, ProtocolChange, ProtocolExecutor,
    TreasuryExecutor, TreasuryOperation, TreasuryOperationType,
};

// Re-export governance types needed by supervisor initialization.
// This allows icn-core to import from icn_governance_actor instead of
// icn_governance directly, reducing direct governance coupling.
pub use icn_governance::{ForcedOutcome, MembershipResolver, ProposalId, StaticMembershipResolver};

/// Create a ProposalExecutor instance.
///
/// This is the entry point for proposal execution. The executor requires
/// an execution callback that has access to icn-core internals.
///
/// # Arguments
/// * `execution_callback` - Callback for executing proposals with domain logic
pub fn create_executor(
    execution_callback: executor::ExecutionCallback,
) -> std::sync::Arc<dyn icn_kernel_api::services::ProposalExecutor> {
    std::sync::Arc::new(GovernanceProposalExecutor::new(execution_callback))
}

/// Type alias for effect execution callback.
///
/// This callback receives pre-translated kernel effects and a decision receipt ID.
/// The kernel's EffectDispatcher typically wraps this.
pub type EffectExecutionCallback = std::sync::Arc<dyn Fn(Vec<KernelEffect>, String) + Send + Sync>;

fn compute_decision_hash(decision_receipt_id: &str) -> String {
    // Temporary deterministic bridge for effect routing; replace with
    // canonical decision artifact hash when available at this boundary.
    blake3::hash(decision_receipt_id.as_bytes())
        .to_hex()
        .to_string()
}

const NON_EXECUTABLE_ACCEPTED_PREFIX: &str = "non-executable accepted proposal";

/// Create an event subscription that routes proposals through the effect system.
///
/// This subscription:
/// 1. Receives `ProposalAccepted` events
/// 2. Deserializes the payload to `ProposalPayload`
/// 3. Translates to `Vec<KernelEffect>` via `translate_payload_to_effects()`
/// 4. Dispatches to the provided callback (typically EffectDispatcher)
///
/// This is the production path for effect-based governance execution.
///
/// # Arguments
/// * `effect_callback` - Callback that executes the translated effects
///
/// # Returns
/// An event subscription function suitable for `EventBus::subscribe()`
pub fn create_effect_subscription<F>(
    effect_callback: F,
) -> std::sync::Arc<dyn Fn(icn_kernel_api::events::SystemEvent) + Send + Sync>
where
    F: Fn(Vec<KernelEffect>, String) + Send + Sync + 'static,
{
    use icn_governance::ProposalPayload;
    use icn_kernel_api::events::SystemEvent;
    use tracing::{debug, error, info};

    std::sync::Arc::new(move |event| {
        if let SystemEvent::ProposalAccepted {
            proposal_id,
            payload,
            domain_id,
            canonical_payload_hash,
            governance_decision_hash,
            ..
        } = &event
        {
            // Generate decision_receipt_id from proposal_id and domain
            let decision_receipt_id = format!("gov:{domain_id}:{proposal_id}:receipt");
            // Prefer governance_decision_hash (canonical GovernanceDecisionReceipt hash,
            // includes votes + tally + outcome) when provided by actor.rs Invariant 7 gate.
            // Fall back to canonical_payload_hash (content hash) for sprint 26 compatibility,
            // then to blake3(receipt_id) for legacy events with no hash at all.
            let decision_hash = governance_decision_hash
                .clone()
                .or_else(|| canonical_payload_hash.clone())
                .unwrap_or_else(|| compute_decision_hash(&decision_receipt_id));

            // Deserialize payload
            match serde_json::from_value::<ProposalPayload>(payload.clone()) {
                Ok(proposal_payload) => {
                    // Translate to effects
                    let effects = translate_payload_to_effects(
                        &proposal_payload,
                        &decision_receipt_id,
                        &decision_hash,
                        domain_id,
                    );
                    match effects {
                        Ok(effects) => {
                            if effects.is_empty() {
                                debug!(
                                    proposal_id = %proposal_id,
                                    "Proposal produced no effects (unexpected empty translation)"
                                );
                            } else {
                                info!(
                                    proposal_id = %proposal_id,
                                    effect_count = effects.len(),
                                    "Routing proposal through effect path"
                                );
                            }
                            effect_callback(effects, decision_receipt_id);
                        }
                        Err(e) => {
                            // Explicit non-executable classification: keep terminal outcome
                            // honest by dispatching a classified NoOp row.
                            let reason = format!(
                                "{NON_EXECUTABLE_ACCEPTED_PREFIX} [{}]: {}",
                                e.kind, e.detail
                            );
                            error!(
                                proposal_id = %proposal_id,
                                domain_id = %domain_id,
                                error = %e,
                                "Accepted proposal is non-executable in current kernel path"
                            );
                            effect_callback(
                                vec![KernelEffect::NoOp { reason }],
                                decision_receipt_id,
                            );
                        }
                    }
                }
                Err(e) => {
                    error!(
                        proposal_id = %proposal_id,
                        error = %e,
                        "Failed to deserialize ProposalPayload for effect routing"
                    );
                }
            }
        }
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use icn_governance::{ProposalPayload, TreasuryProposalOperation};
    use icn_identity::Did;
    use icn_kernel_api::events::SystemEvent;
    use std::sync::{Arc, Mutex};

    type CapturedEffects = Arc<Mutex<Vec<(Vec<KernelEffect>, String)>>>;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_pilot_proposal_uses_effect_subscription_even_with_legacy_env_flag() {
        let _env_guard = ENV_LOCK.lock().expect("env lock");
        // Legacy env switch was removed; this test proves pilot treasury proposals
        // still route through the effect-path subscription callback.
        std::env::set_var("ICN_USE_EFFECT_PATH", "0");

        let captured: CapturedEffects = Arc::new(Mutex::new(vec![]));
        let sink = captured.clone();
        let subscription = create_effect_subscription(move |effects, decision_receipt_id| {
            sink.lock()
                .expect("capture lock")
                .push((effects, decision_receipt_id));
        });

        let treasury_did: Did = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9"
            .parse()
            .expect("valid did");
        let recipient: Did = "did:icn:z8eQZfY3RY75YwQ6MrFCHt9phbi3HGx1caFXE3291ow8t"
            .parse()
            .expect("valid did");

        let payload = ProposalPayload::Treasury {
            operation: TreasuryProposalOperation::Spend {
                treasury_did,
                amount: 5,
                currency: "hours".to_string(),
                recipient,
                memo: "pilot".to_string(),
                nonce: 7,
            },
        };

        let event = SystemEvent::ProposalAccepted {
            proposal_id: "pr-pilot-1".to_string(),
            domain_id: "domain-pilot".to_string(),
            payload: serde_json::to_value(payload).expect("serialize payload"),
            decided_at: 1_700_000_000,
            canonical_payload_hash: None,
            governance_decision_hash: None,
        };

        subscription(event);

        let got = captured.lock().expect("capture lock");
        assert_eq!(got.len(), 1, "effect callback should fire exactly once");
        assert_eq!(got[0].1, "gov:domain-pilot:pr-pilot-1:receipt");
        assert!(
            matches!(
                got[0].0.first(),
                Some(KernelEffect::Treasury(TreasuryEffect::Spend { .. }))
            ),
            "pilot treasury proposal should route to TreasuryEffect::Spend via effect path"
        );

        std::env::remove_var("ICN_USE_EFFECT_PATH");
    }

    #[test]
    fn test_unsupported_accepted_payload_emits_classified_noop() {
        let captured: CapturedEffects = Arc::new(Mutex::new(vec![]));
        let sink = captured.clone();
        let subscription = create_effect_subscription(move |effects, decision_receipt_id| {
            sink.lock()
                .expect("capture lock")
                .push((effects, decision_receipt_id));
        });

        // Charter is a still-unsupported payload; ShareRedemption is wired as of Tranche 11.
        let payload = ProposalPayload::Charter {
            charter_id: "test-coop-charter".to_string(),
            charter_yaml: "schema_version: v0\nentity: coop\n".to_string(),
        };

        let event = SystemEvent::ProposalAccepted {
            proposal_id: "pr-unsupported-1".to_string(),
            domain_id: "domain-pilot".to_string(),
            payload: serde_json::to_value(payload).expect("serialize payload"),
            decided_at: 1_700_000_001,
            canonical_payload_hash: None,
            governance_decision_hash: None,
        };

        subscription(event);

        let got = captured.lock().expect("capture lock");
        assert_eq!(got.len(), 1, "effect callback should fire exactly once");
        assert_eq!(got[0].1, "gov:domain-pilot:pr-unsupported-1:receipt");
        assert_eq!(
            got[0].0.len(),
            1,
            "classified fallback should be single effect"
        );
        match &got[0].0[0] {
            KernelEffect::NoOp { reason } => {
                assert!(
                    reason.starts_with(NON_EXECUTABLE_ACCEPTED_PREFIX),
                    "NoOp reason must be classified and stable, got: {reason}"
                );
                assert!(
                    reason.contains("[payload]"),
                    "NoOp reason must include error kind, got: {reason}"
                );
            }
            other => panic!("expected classified NoOp fallback, got {other:?}"),
        }
    }

    /// Prove forced-accept provenance: the governance_decision_hash produced by the
    /// forced-accept path is a canonical GovernanceDecisionReceipt hash (not a content
    /// hash), is deterministic, and is distinct from canonical_payload_hash.
    ///
    /// This validates the actor.rs forced-accept fix without requiring the full actor
    /// to be spawned: we construct the event exactly as the actor would and verify that
    /// create_effect_subscription forwards the governance_decision_hash (not the content
    /// hash fallback) as the decision_hash in the captured effect receipt_id.
    #[test]
    fn forced_accept_emits_canonical_governance_decision_hash_not_content_hash() {
        use icn_governance::proof::{GovernanceDecisionReceipt, ProofOutcome};
        use icn_governance::tally::VoteTally;

        let domain_id = "test-forced-domain";
        let proposal_id = "forced-prop-001";

        // Replicate the exact hash computation from the actor's forced-accept path.
        let forced_decision_hash = {
            let receipt = GovernanceDecisionReceipt::new(
                proposal_id.to_string(),
                domain_id.to_string(),
                ProofOutcome::Accepted,
                VoteTally::empty(),
                &[],
            );
            hex::encode(receipt.decision_hash)
        };

        // The content hash is different — compute it so we can prove non-equality.
        let payload = ProposalPayload::Treasury {
            operation: TreasuryProposalOperation::Spend {
                treasury_did: "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9"
                    .parse::<Did>()
                    .expect("valid did"),
                amount: 10,
                currency: "HOURS".to_string(),
                recipient: "did:icn:z8eQZfY3RY75YwQ6MrFCHt9phbi3HGx1caFXE3291ow8t"
                    .parse::<Did>()
                    .expect("valid did"),
                memo: "forced-accept test".to_string(),
                nonce: 0,
            },
        };
        let payload_value = serde_json::to_value(&payload).expect("serialize payload");
        let canonical_payload_hash = serde_json::to_string(&payload_value)
            .ok()
            .map(|s| blake3::hash(s.as_bytes()).to_hex().to_string())
            .expect("content hash");

        // Invariant: forced_decision_hash must differ from canonical_payload_hash.
        assert_ne!(
            forced_decision_hash, canonical_payload_hash,
            "governance decision hash must not equal content hash — they encode different things"
        );

        // Invariant: forced_decision_hash is 64 hex chars (canonical receipt hash format).
        assert_eq!(
            forced_decision_hash.len(),
            64,
            "forced decision hash must be 64 hex chars"
        );
        assert!(
            forced_decision_hash.chars().all(|c| c.is_ascii_hexdigit()),
            "forced decision hash must be valid hex"
        );

        // Invariant: deterministic — same inputs produce the same hash.
        let second_hash = {
            let receipt = GovernanceDecisionReceipt::new(
                proposal_id.to_string(),
                domain_id.to_string(),
                ProofOutcome::Accepted,
                VoteTally::empty(),
                &[],
            );
            hex::encode(receipt.decision_hash)
        };
        assert_eq!(
            forced_decision_hash, second_hash,
            "forced decision hash must be deterministic"
        );

        // Prove create_effect_subscription uses governance_decision_hash (not content hash)
        // when governance_decision_hash is present — which is what the fixed actor now sets.
        let captured: CapturedEffects = Arc::new(Mutex::new(vec![]));
        let sink = captured.clone();
        let subscription = create_effect_subscription(move |effects, receipt_id| {
            sink.lock().expect("lock").push((effects, receipt_id));
        });

        let expected_receipt_id = format!("gov:{domain_id}:{proposal_id}:receipt");

        subscription(SystemEvent::ProposalAccepted {
            proposal_id: proposal_id.to_string(),
            domain_id: domain_id.to_string(),
            payload: payload_value,
            decided_at: 1_700_000_099,
            canonical_payload_hash: Some(canonical_payload_hash.clone()),
            governance_decision_hash: Some(forced_decision_hash.clone()),
        });

        let got = captured.lock().expect("lock");
        assert_eq!(got.len(), 1, "subscription must fire exactly once");
        assert_eq!(
            got[0].1, expected_receipt_id,
            "receipt_id must use standard gov:domain:proposal:receipt format"
        );

        // The effect carries the forced_decision_hash as its provenance reference.
        // For treasury spend, this means TreasuryEffect::Spend.decision_hash == forced_hash.
        assert!(
            matches!(
                got[0].0.first(),
                Some(KernelEffect::Treasury(TreasuryEffect::Spend { decision_hash, .. }))
                    if decision_hash == &forced_decision_hash
            ),
            "TreasuryEffect::Spend must carry forced decision hash, not content hash. \
             Got effect: {:?}",
            got[0].0.first()
        );
    }
}
