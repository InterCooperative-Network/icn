//! Effect-based execution dispatcher for governance proposals.
//!
//! This module provides the kernel's effect execution layer. It receives
//! pre-translated `KernelEffect`s and executes them via `KernelGovernanceExecutor`.
//!
//! **IMPORTANT**: This is a kernel module. It must NOT import app/domain types
//! like `ProposalPayload`. Translation from domain types to effects happens
//! in the app layer (icn-governance-actor) BEFORE calling into the kernel.
//!
//! # Architecture
//!
//! ```text
//! [App Layer - icn-governance-actor]
//! ProposalPayload → translate_payload_to_effects() → Vec<KernelEffect>
//!                                                          |
//!                                                          v
//! [Kernel Layer - icn-core]
//! EffectDispatcher::execute_effects(effects) → Vec<EffectResult>
//! ```
//!
//! This is the production execution path for governance proposals.
//! The legacy `governance_handlers` have been deleted.

use std::sync::Arc;

use anyhow::Result;
use tracing::{debug, error, info, warn};

use icn_kernel_api::effects::{EffectResult, KernelEffect};
use icn_kernel_api::governance::EffectExecutor;

use super::governance_executor::KernelGovernanceExecutor;

/// Dispatcher that executes governance proposals via the effect system.
///
/// This is the new execution path using kernel-safe effects.
pub struct EffectDispatcher {
    /// The kernel executor for effect processing
    executor: Arc<KernelGovernanceExecutor>,
}

impl EffectDispatcher {
    /// Create a new effect dispatcher.
    pub fn new(executor: Arc<KernelGovernanceExecutor>) -> Self {
        Self { executor }
    }

    /// Execute a batch of pre-translated kernel effects.
    ///
    /// This is the primary entry point for the effect execution path.
    /// The caller (app layer) is responsible for translating domain payloads
    /// to `KernelEffect`s before calling this method.
    ///
    /// # Arguments
    /// * `effects` - Pre-translated kernel effects to execute
    /// * `decision_receipt_id` - The receipt ID for audit linkage
    ///
    /// # Returns
    /// Vector of effect results (one per effect)
    pub async fn execute_effects(
        &self,
        effects: Vec<KernelEffect>,
        decision_receipt_id: &str,
    ) -> Result<Vec<EffectResult>> {
        if effects.is_empty() {
            debug!(
                receipt_id = %decision_receipt_id,
                "No effects to execute"
            );
            return Ok(vec![]);
        }

        info!(
            receipt_id = %decision_receipt_id,
            effect_count = effects.len(),
            "Executing {} effects via effect dispatcher",
            effects.len()
        );

        // Execute each effect via the kernel executor
        let mut results = Vec::with_capacity(effects.len());
        for effect in effects {
            let effect_type = effect_type_label(&effect);
            match self
                .executor
                .execute_effect(effect, decision_receipt_id)
                .await
            {
                Ok(result) => {
                    if result.success {
                        debug!(
                            receipt_id = %decision_receipt_id,
                            effect_type = %effect_type,
                            "Effect executed successfully"
                        );
                    } else {
                        warn!(
                            receipt_id = %decision_receipt_id,
                            effect_type = %effect_type,
                            message = %result.message,
                            "Effect execution returned failure"
                        );
                    }
                    results.push(result);
                }
                Err(e) => {
                    error!(
                        receipt_id = %decision_receipt_id,
                        effect_type = %effect_type,
                        error = %e,
                        "Effect execution failed"
                    );
                    results.push(EffectResult {
                        effect_id: decision_receipt_id.to_string(),
                        success: false,
                        message: format!("Execution error: {}", e),
                        state_change_hash: None,
                        ledger_entry_id: None,
                    });
                }
            }
        }

        Ok(results)
    }

    /// Execute effects via the executor's batch API.
    ///
    /// This delegates to the executor's batch method which may have
    /// different atomicity guarantees.
    pub async fn execute_effects_batch(
        &self,
        effects: Vec<KernelEffect>,
        decision_receipt_id: &str,
    ) -> Result<Vec<EffectResult>> {
        self.executor
            .execute_effects_batch(effects, decision_receipt_id)
            .await
    }
}

/// Get a label for an effect type (for logging/metrics).
fn effect_type_label(effect: &KernelEffect) -> &'static str {
    match effect {
        KernelEffect::Treasury(_) => "treasury",
        KernelEffect::Protocol(_) => "protocol",
        KernelEffect::Membership(_) => "membership",
        KernelEffect::Control(_) => "control",
        KernelEffect::Federation(_) => "federation",
        KernelEffect::NoOp { .. } => "noop",
    }
}

/// Create a callback that executes pre-translated effects via the dispatcher.
///
/// This returns a callback suitable for use by the app layer (governance actor)
/// after it has translated proposal payloads into kernel effects.
///
/// # Usage
///
/// The app layer is responsible for:
/// 1. Subscribing to governance events (ProposalAccepted, etc.)
/// 2. Deserializing ProposalPayload from the event
/// 3. Calling translate_payload_to_effects()
/// 4. Calling this callback with the resulting effects
///
/// This ensures the kernel never sees domain types like ProposalPayload.
pub fn create_effect_executor_callback(
    dispatcher: Arc<EffectDispatcher>,
) -> Arc<dyn Fn(Vec<KernelEffect>, String) + Send + Sync> {
    Arc::new(move |effects, decision_receipt_id| {
        if effects.is_empty() {
            debug!(
                receipt_id = %decision_receipt_id,
                "No effects to execute (empty batch)"
            );
            return;
        }

        let dispatcher = dispatcher.clone();
        let effect_count = effects.len();

        tokio::spawn(async move {
            match dispatcher
                .execute_effects(effects, &decision_receipt_id)
                .await
            {
                Ok(results) => {
                    let success_count = results.iter().filter(|r| r.success).count();
                    info!(
                        receipt_id = %decision_receipt_id,
                        total = results.len(),
                        success = success_count,
                        "Effect-based execution complete"
                    );
                }
                Err(e) => {
                    error!(
                        receipt_id = %decision_receipt_id,
                        effect_count = effect_count,
                        error = %e,
                        "Effect-based execution failed"
                    );
                }
            }
        });
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_kernel_api::effects::{ControlEffect, TreasuryEffect};
    use icn_kernel_api::protocol_params::{
        ParameterChange, ParameterScope, ParameterValidationError, ParameterValue, PendingChangeId,
        PendingParameterChange, ProtocolParameter, ProtocolParameterStore,
    };
    use std::collections::HashMap;
    use std::sync::RwLock;

    /// Mock parameter store for testing
    struct MockParamStore {
        params: RwLock<HashMap<String, ProtocolParameter>>,
    }

    impl MockParamStore {
        fn new() -> Self {
            Self {
                params: RwLock::new(HashMap::new()),
            }
        }
    }

    impl ProtocolParameterStore for MockParamStore {
        fn get(&self, id: &str) -> anyhow::Result<Option<ProtocolParameter>> {
            Ok(self.params.read().unwrap().get(id).cloned())
        }

        fn get_effective(
            &self,
            id: &str,
            _coop_id: Option<&str>,
            _fed_id: Option<&str>,
        ) -> anyhow::Result<Option<ProtocolParameter>> {
            self.get(id)
        }

        fn set(
            &self,
            param: ProtocolParameter,
            _proposal_id: Option<String>,
            _changed_by: Option<String>,
        ) -> anyhow::Result<()> {
            self.params.write().unwrap().insert(param.id.clone(), param);
            Ok(())
        }

        fn list(&self) -> anyhow::Result<Vec<ProtocolParameter>> {
            Ok(self.params.read().unwrap().values().cloned().collect())
        }

        fn list_by_category(&self, category: &str) -> anyhow::Result<Vec<ProtocolParameter>> {
            Ok(self
                .params
                .read()
                .unwrap()
                .values()
                .filter(|p| p.id.starts_with(category))
                .cloned()
                .collect())
        }

        fn get_history(&self, _id: &str) -> anyhow::Result<Vec<ParameterChange>> {
            Ok(vec![])
        }

        fn get_history_paginated(
            &self,
            _id: &str,
            _offset: usize,
            _limit: usize,
        ) -> anyhow::Result<(Vec<ParameterChange>, usize)> {
            Ok((vec![], 0))
        }

        fn prune_history(&self, _id: &str, _max_entries: usize) -> anyhow::Result<usize> {
            Ok(0)
        }

        fn delete(&self, id: &str) -> anyhow::Result<()> {
            self.params.write().unwrap().remove(id);
            Ok(())
        }

        fn exists(&self, id: &str) -> anyhow::Result<bool> {
            Ok(self.params.read().unwrap().contains_key(id))
        }

        fn count(&self) -> anyhow::Result<usize> {
            Ok(self.params.read().unwrap().len())
        }

        fn total_history_count(&self) -> anyhow::Result<usize> {
            Ok(0)
        }

        fn validate(
            &self,
            _id: &str,
            _new_value: &ParameterValue,
        ) -> std::result::Result<(), ParameterValidationError> {
            Ok(())
        }

        fn list_scoped_parameters(&self) -> anyhow::Result<Vec<ProtocolParameter>> {
            Ok(vec![])
        }

        fn delete_scoped_parameter(
            &self,
            _id: &str,
            _scope: &ParameterScope,
        ) -> anyhow::Result<bool> {
            Ok(false)
        }

        fn add_pending_change(&self, _change: PendingParameterChange) -> anyhow::Result<()> {
            Ok(())
        }

        fn get_pending_change(
            &self,
            _id: &PendingChangeId,
        ) -> anyhow::Result<Option<PendingParameterChange>> {
            Ok(None)
        }

        fn list_pending_changes(&self) -> anyhow::Result<Vec<PendingParameterChange>> {
            Ok(vec![])
        }

        fn list_pending_changes_for_parameter(
            &self,
            _parameter_id: &str,
        ) -> anyhow::Result<Vec<PendingParameterChange>> {
            Ok(vec![])
        }

        fn get_changes_due_before(
            &self,
            _timestamp: u64,
        ) -> anyhow::Result<Vec<PendingParameterChange>> {
            Ok(vec![])
        }

        fn update_pending_change(&self, _change: PendingParameterChange) -> anyhow::Result<()> {
            Ok(())
        }

        fn cancel_pending_change(
            &self,
            _id: &PendingChangeId,
            _reason: &str,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        fn count_pending_changes(&self) -> anyhow::Result<usize> {
            Ok(0)
        }
    }

    #[tokio::test]
    async fn test_effect_dispatcher_treasury_spend() {
        // Create executor with mock protocol store
        let param_store = Arc::new(MockParamStore::new());
        let executor = Arc::new(KernelGovernanceExecutor::new(param_store));
        let dispatcher = EffectDispatcher::new(executor);

        // Create a treasury spend effect directly (translation happens in app layer)
        let effects = vec![KernelEffect::Treasury(TreasuryEffect::Spend {
            treasury_did: "did:icn:treasury123".to_string(),
            recipient_did: "did:icn:recipient456".to_string(),
            amount: 1000,
            currency: "ICN".to_string(),
            memo: "Test payment".to_string(),
            budget_id: None,
            decision_receipt_id: "test-receipt-001".to_string(),
            decision_hash: "sha256:abc123".to_string(),
        })];

        // Execute through dispatcher with pre-translated effects
        let results = dispatcher
            .execute_effects(effects, "test-receipt-001")
            .await
            .expect("execution should succeed");

        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert!(results[0].message.contains("Spend"));
    }

    #[tokio::test]
    async fn test_effect_dispatcher_text_resolution() {
        // Text resolutions produce Control effects
        let param_store = Arc::new(MockParamStore::new());
        let executor = Arc::new(KernelGovernanceExecutor::new(param_store));
        let dispatcher = EffectDispatcher::new(executor);

        // Create a control effect directly (translation happens in app layer)
        // TextResolution stores a hash of the resolution body, not the body itself
        let effects = vec![KernelEffect::Control(ControlEffect::TextResolution {
            resolution_hash: "sha256:abc123...".to_string(),
        })];

        let results = dispatcher
            .execute_effects(effects, "test-receipt-002")
            .await
            .expect("execution should succeed");

        // Control effects execute successfully
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
    }

    #[tokio::test]
    async fn test_effect_dispatcher_empty_effects() {
        // Empty effects should return empty results
        let param_store = Arc::new(MockParamStore::new());
        let executor = Arc::new(KernelGovernanceExecutor::new(param_store));
        let dispatcher = EffectDispatcher::new(executor);

        let results = dispatcher
            .execute_effects(vec![], "test-receipt-003")
            .await
            .expect("execution should succeed");

        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_effect_type_label() {
        assert_eq!(
            effect_type_label(&KernelEffect::Treasury(
                icn_kernel_api::effects::TreasuryEffect::Spend {
                    treasury_did: "did:icn:test".to_string(),
                    recipient_did: "did:icn:recipient".to_string(),
                    amount: 100,
                    currency: "ICN".to_string(),
                    memo: "test".to_string(),
                    budget_id: None,
                    decision_receipt_id: "r1".to_string(),
                    decision_hash: "sha256:test".to_string(),
                }
            )),
            "treasury"
        );
        assert_eq!(
            effect_type_label(&KernelEffect::NoOp {
                reason: "test".to_string()
            }),
            "noop"
        );
    }

    /// End-to-end integration test proving Protocol Change flows through the
    /// effect path with proper provenance linkage.
    ///
    /// This test verifies:
    /// 1. ProtocolEffect::SetParameter can be created and wrapped in KernelEffect
    /// 2. EffectDispatcher correctly routes to KernelGovernanceExecutor
    /// 3. Execution succeeds with appropriate result message
    /// 4. effect_id in result matches decision_receipt_id (provenance linkage)
    /// 5. Parameter value is actually updated in the store
    #[tokio::test]
    async fn test_protocol_change_end_to_end_provenance() {
        use icn_kernel_api::effects::ProtocolEffect;

        // Known test values
        let parameter_name = "governance.voting_period";
        let old_value = "86400"; // 1 day in seconds
        let new_value = "172800"; // 2 days in seconds
        let decision_receipt_id = "gov:proposal:2024-002:receipt:def456";

        // Step 1: Set up the parameter store with the initial value
        let param_store = Arc::new(MockParamStore::new());
        let initial_param = ProtocolParameter::new(
            parameter_name,
            parameter_name,
            "Voting period duration in seconds",
            ParameterValue::Integer(86400),
        );
        param_store
            .set(initial_param, None, None)
            .expect("should set initial param");

        // Step 2: Create a ProtocolEffect::SetParameter with known values
        let protocol_effect = ProtocolEffect::SetParameter {
            parameter_name: parameter_name.to_string(),
            old_value_hash: old_value.to_string(),
            new_value_json: new_value.to_string(),
            effective_at: 0,
        };

        // Step 3: Wrap in KernelEffect::Protocol
        let kernel_effect = KernelEffect::Protocol(protocol_effect);

        // Step 4: Create an EffectDispatcher with a KernelGovernanceExecutor
        let executor = Arc::new(KernelGovernanceExecutor::new(param_store.clone()));
        let dispatcher = EffectDispatcher::new(executor);

        // Step 5: Execute effects with the decision_receipt_id
        let results = dispatcher
            .execute_effects(vec![kernel_effect], decision_receipt_id)
            .await
            .expect("execution should succeed");

        // Step 6: Assert result is success
        assert_eq!(results.len(), 1, "Expected exactly one result");
        let result = &results[0];
        assert!(
            result.success,
            "Protocol change should succeed, got: {}",
            result.message
        );

        // Step 7: Assert result message contains expected operation description
        assert!(
            result.message.contains("Changed"),
            "Message should describe change operation, got: {}",
            result.message
        );
        assert!(
            result.message.contains(parameter_name),
            "Message should contain parameter name {}, got: {}",
            parameter_name,
            result.message
        );

        // Step 8: Assert effect_id matches decision_receipt_id (provenance linkage)
        assert_eq!(
            result.effect_id, decision_receipt_id,
            "effect_id must match decision_receipt_id for provenance tracking"
        );

        // Step 9: Verify the parameter was actually updated in the store
        let updated_param = param_store
            .get(parameter_name)
            .expect("should get param")
            .expect("param should exist");
        assert_eq!(
            updated_param.value,
            ParameterValue::Integer(172800),
            "Parameter value should be updated to new value"
        );
        assert_eq!(
            updated_param.updated_by,
            Some(decision_receipt_id.to_string()),
            "updated_by should reference the decision receipt"
        );
    }

    /// End-to-end integration test proving Treasury Spend flows through the
    /// effect path with proper provenance linkage.
    ///
    /// This test verifies:
    /// 1. TreasuryEffect::Spend can be created and wrapped in KernelEffect
    /// 2. EffectDispatcher correctly routes to KernelGovernanceExecutor
    /// 3. Execution succeeds with appropriate result message
    /// 4. effect_id in result matches decision_receipt_id (provenance linkage)
    ///
    /// NOTE: This test uses a placeholder treasury executor that logs but doesn't
    /// write to the ledger. The full provenance chain (decision → effect → ledger
    /// entry with decision_hash) is proven separately:
    /// - icn-ledger/src/entry.rs: test_entry_with_decision_provenance
    /// - JournalEntry now carries decision_receipt_id + decision_hash
    ///
    /// TODO: When treasury executor is wired to real ledger, add assertion:
    /// ```ignore
    /// let ledger_entry = ledger.get_entry(result.ledger_entry_id).await?;
    /// assert_eq!(ledger_entry.decision_receipt_id, Some(decision_receipt_id));
    /// assert_eq!(ledger_entry.decision_hash, Some(decision_hash));
    /// ```
    #[tokio::test]
    async fn test_treasury_spend_end_to_end_provenance() {
        // Known test values
        let treasury_did = "did:icn:treasury:coop-alpha-main";
        let recipient_did = "did:icn:member:alice-dev-fund";
        let amount: i64 = 2500;
        let currency = "HOURS";
        let decision_receipt_id = "gov:proposal:2024-001:receipt:abc123";

        // Step 1: Create a TreasuryEffect::Spend with known values
        let treasury_effect = TreasuryEffect::Spend {
            treasury_did: treasury_did.to_string(),
            recipient_did: recipient_did.to_string(),
            amount,
            currency: currency.to_string(),
            memo: "Development grant for Q1".to_string(),
            budget_id: None,
            decision_receipt_id: decision_receipt_id.to_string(),
            decision_hash: "sha256:governance-decision-2024-001".to_string(),
        };

        // Step 2: Wrap in KernelEffect::Treasury
        let kernel_effect = KernelEffect::Treasury(treasury_effect);

        // Step 3: Create an EffectDispatcher with a KernelGovernanceExecutor
        let param_store = Arc::new(MockParamStore::new());
        let executor = Arc::new(KernelGovernanceExecutor::new(param_store));
        let dispatcher = EffectDispatcher::new(executor);

        // Step 4: Execute effects with the decision_receipt_id
        let results = dispatcher
            .execute_effects(vec![kernel_effect], decision_receipt_id)
            .await
            .expect("execution should succeed");

        // Step 5: Assert result is success
        assert_eq!(results.len(), 1, "Expected exactly one result");
        let result = &results[0];
        assert!(
            result.success,
            "Treasury spend should succeed, got: {}",
            result.message
        );

        // Step 6: Assert result message contains expected operation description
        assert!(
            result.message.contains("Spend"),
            "Message should describe Spend operation, got: {}",
            result.message
        );
        assert!(
            result.message.contains(&amount.to_string()),
            "Message should contain amount {}, got: {}",
            amount,
            result.message
        );
        assert!(
            result.message.contains(currency),
            "Message should contain currency {}, got: {}",
            currency,
            result.message
        );

        // Step 7: Assert effect_id matches decision_receipt_id (provenance linkage)
        assert_eq!(
            result.effect_id, decision_receipt_id,
            "effect_id must match decision_receipt_id for provenance tracking"
        );
    }
}
