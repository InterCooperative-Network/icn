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
pub mod executor;
pub mod handlers;
pub mod init;
pub mod registry;

pub use actor::{GovernanceActor, GovernanceCommand, GovernanceConfigLite, GovernanceHandle};
pub use executor::{ExecutionCallback, GovernanceProposalExecutor};
pub use handlers::translate_payload_to_effects;

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
            ..
        } = &event
        {
            // Generate decision_receipt_id from proposal_id and domain
            let decision_receipt_id = format!("gov:{domain_id}:{proposal_id}:receipt");
            let decision_hash = compute_decision_hash(&decision_receipt_id);

            // Deserialize payload
            match serde_json::from_value::<ProposalPayload>(payload.clone()) {
                Ok(proposal_payload) => {
                    // Translate to effects
                    let effects = translate_payload_to_effects(
                        &proposal_payload,
                        &decision_receipt_id,
                        &decision_hash,
                    );

                    if effects.is_empty() {
                        debug!(
                            proposal_id = %proposal_id,
                            "Proposal produced no effects (likely Text or NoOp)"
                        );
                    } else {
                        info!(
                            proposal_id = %proposal_id,
                            effect_count = effects.len(),
                            "Routing proposal through effect path"
                        );
                    }

                    // Dispatch to effect executor
                    effect_callback(effects, decision_receipt_id);
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
mod tests {
    use super::*;
    use icn_governance::{ProposalPayload, TreasuryProposalOperation};
    use icn_identity::Did;
    use icn_kernel_api::events::SystemEvent;
    use std::sync::{Arc, Mutex};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_pilot_proposal_uses_effect_subscription_even_with_legacy_env_flag() {
        let _env_guard = ENV_LOCK.lock().expect("env lock");
        // Legacy env switch was removed; this test proves pilot treasury proposals
        // still route through the effect-path subscription callback.
        std::env::set_var("ICN_USE_EFFECT_PATH", "0");

        let captured: Arc<Mutex<Vec<(Vec<KernelEffect>, String)>>> = Arc::new(Mutex::new(vec![]));
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
        };

        subscription(event);

        let got = captured.lock().expect("capture lock");
        assert_eq!(got.len(), 1, "effect callback should fire exactly once");
        assert_eq!(got[0].1, "gov:domain-pilot:pr-pilot-1:receipt");
        assert!(
            matches!(got[0].0.first(), Some(KernelEffect::Treasury(TreasuryEffect::Spend { .. }))),
            "pilot treasury proposal should route to TreasuryEffect::Spend via effect path"
        );

        std::env::remove_var("ICN_USE_EFFECT_PATH");
    }
}
