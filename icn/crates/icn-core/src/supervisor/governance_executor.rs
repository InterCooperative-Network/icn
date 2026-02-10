//! Governance executor adapter - implements kernel traits for governance app.
//!
//! This module provides the adapter that connects the governance app to
//! the kernel's execution services (ledger, treasury, protocol params).
//!
//! The [`KernelGovernanceExecutor`] implements [`GovernanceExecutor`] by
//! delegating to concrete kernel implementations:
//! - [`KernelTreasuryExecutor`] for treasury operations
//! - [`KernelProtocolExecutor`] for protocol parameter changes

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use icn_kernel_api::effects::{EffectResult, KernelEffect};
use icn_kernel_api::governance::{
    DecisionReceiptId, EffectExecutor, ExecutionOutcome, GovernanceExecutor, ProtocolChange,
    ProtocolExecutor, TreasuryExecutor, TreasuryOperation,
};
use icn_kernel_api::protocol_params::ProtocolParameterStore;

/// Adapter that implements [`GovernanceExecutor`] by delegating to kernel services.
///
/// This is the bridge between the governance app (domain logic) and the kernel's
/// execution capabilities. It holds references to the treasury and protocol
/// executors which perform the actual operations.
pub struct KernelGovernanceExecutor {
    treasury: Arc<KernelTreasuryExecutor>,
    protocol: Arc<KernelProtocolExecutor>,
}

impl KernelGovernanceExecutor {
    /// Create a new governance executor with the given dependencies.
    ///
    /// # Arguments
    /// * `protocol_param_store` - Store for protocol parameters (used by protocol executor)
    pub fn new(protocol_param_store: Arc<dyn ProtocolParameterStore>) -> Self {
        Self {
            treasury: Arc::new(KernelTreasuryExecutor::new()),
            protocol: Arc::new(KernelProtocolExecutor::new(protocol_param_store)),
        }
    }
}

impl GovernanceExecutor for KernelGovernanceExecutor {
    fn treasury(&self) -> &dyn TreasuryExecutor {
        self.treasury.as_ref()
    }

    fn protocol(&self) -> &dyn ProtocolExecutor {
        self.protocol.as_ref()
    }
}

#[async_trait]
impl EffectExecutor for KernelGovernanceExecutor {
    async fn execute_effect(
        &self,
        effect: KernelEffect,
        decision_receipt_id: &str,
    ) -> Result<EffectResult> {
        let receipt_id = DecisionReceiptId::new(decision_receipt_id);

        match effect {
            KernelEffect::Treasury(treasury_effect) => {
                // Convert TreasuryEffect to TreasuryOperation
                let operation = treasury_effect_to_operation(&treasury_effect);
                let outcome = self
                    .treasury
                    .execute_treasury_operation(&receipt_id, operation)
                    .await?;
                Ok(execution_outcome_to_effect_result(
                    outcome,
                    decision_receipt_id,
                ))
            }
            KernelEffect::Protocol(protocol_effect) => {
                // Convert ProtocolEffect to ProtocolChange
                if let Some(change) = protocol_effect_to_change(&protocol_effect) {
                    let outcome = self
                        .protocol
                        .apply_protocol_change(&receipt_id, change)
                        .await?;
                    Ok(execution_outcome_to_effect_result(
                        outcome,
                        decision_receipt_id,
                    ))
                } else {
                    Ok(EffectResult {
                        effect_id: decision_receipt_id.to_string(),
                        success: true,
                        message: "Protocol effect applied".to_string(),
                        state_change_hash: None,
                    })
                }
            }
            KernelEffect::NoOp { reason } => Ok(EffectResult {
                effect_id: decision_receipt_id.to_string(),
                success: true,
                message: format!("NoOp: {}", reason),
                state_change_hash: None,
            }),
            // For other effect types, return success placeholder
            _ => Ok(EffectResult {
                effect_id: decision_receipt_id.to_string(),
                success: true,
                message: "Effect type not yet implemented".to_string(),
                state_change_hash: None,
            }),
        }
    }
}

/// Treasury executor implementation.
///
/// Executes treasury operations (spend, allocate, reserve, release) by
/// delegating to the kernel's ledger services.
pub struct KernelTreasuryExecutor {
    // TODO: Add ledger handle for actual treasury operations
    // ledger: Arc<icn_ledger::Ledger>,
}

impl KernelTreasuryExecutor {
    /// Create a new treasury executor.
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for KernelTreasuryExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TreasuryExecutor for KernelTreasuryExecutor {
    async fn execute_treasury_operation(
        &self,
        receipt_id: &DecisionReceiptId,
        operation: TreasuryOperation,
    ) -> Result<ExecutionOutcome> {
        // TODO: Implement actual treasury operations via ledger
        // For now, log and return success as a placeholder
        tracing::info!(
            receipt_id = %receipt_id,
            treasury_id = %operation.treasury_id,
            op_type = ?operation.operation_type,
            amount = operation.amount,
            currency = %operation.currency,
            recipient = ?operation.recipient,
            memo = %operation.memo,
            "Executing treasury operation"
        );

        Ok(ExecutionOutcome::Success {
            receipt_id: receipt_id.clone(),
            effects: vec![format!(
                "{:?} {} {} from treasury {}",
                operation.operation_type,
                operation.amount,
                operation.currency,
                operation.treasury_id
            )],
        })
    }

    async fn get_treasury_balance(&self, treasury_id: &str, currency: &str) -> Result<i64> {
        // TODO: Query actual treasury balance from ledger
        tracing::debug!(treasury_id, currency, "Querying treasury balance");
        Ok(0) // Placeholder
    }
}

/// Protocol executor implementation.
///
/// Executes protocol parameter changes by delegating to the kernel's
/// protocol parameter store.
pub struct KernelProtocolExecutor {
    /// Protocol parameter store for reading/writing parameters
    param_store: Arc<dyn ProtocolParameterStore>,
}

impl KernelProtocolExecutor {
    /// Create a new protocol executor with the given parameter store.
    pub fn new(param_store: Arc<dyn ProtocolParameterStore>) -> Self {
        Self { param_store }
    }
}

#[async_trait]
impl ProtocolExecutor for KernelProtocolExecutor {
    async fn apply_protocol_change(
        &self,
        receipt_id: &DecisionReceiptId,
        change: ProtocolChange,
    ) -> Result<ExecutionOutcome> {
        tracing::info!(
            receipt_id = %receipt_id,
            parameter = %change.parameter_name,
            old_value = %change.old_value,
            new_value = %change.new_value,
            effective_at = change.effective_at,
            "Applying protocol change"
        );

        // Get the current parameter to validate the old value
        let current_param = match self.param_store.get(&change.parameter_name)? {
            Some(param) => param,
            None => {
                return Ok(ExecutionOutcome::Failed {
                    receipt_id: receipt_id.clone(),
                    reason: format!("Protocol parameter '{}' not found", change.parameter_name),
                });
            }
        };

        // Verify the old value matches what's currently stored (optimistic concurrency)
        let current_value_str = current_param.value.to_string();
        if current_value_str != change.old_value {
            return Ok(ExecutionOutcome::Failed {
                receipt_id: receipt_id.clone(),
                reason: format!(
                    "Protocol parameter '{}' has changed since proposal: expected '{}', found '{}'",
                    change.parameter_name, change.old_value, current_value_str
                ),
            });
        }

        // Parse the new value to match the current parameter's type
        let new_value = parse_value_like(&change.new_value, &current_param.value)?;

        // Create updated parameter
        let mut updated_param = current_param;
        updated_param.value = new_value;
        updated_param.updated_at = change.effective_at;
        updated_param.updated_by = Some(receipt_id.to_string());

        // Apply the change to the parameter store
        self.param_store
            .set(updated_param, Some(receipt_id.to_string()), None)?;

        tracing::info!(
            parameter = %change.parameter_name,
            new_value = %change.new_value,
            "Protocol parameter updated"
        );

        Ok(ExecutionOutcome::Success {
            receipt_id: receipt_id.clone(),
            effects: vec![format!(
                "Changed {} from {} to {}",
                change.parameter_name, change.old_value, change.new_value
            )],
        })
    }

    async fn get_parameter(&self, name: &str) -> Result<Option<String>> {
        tracing::debug!(name, "Querying protocol parameter");
        Ok(self.param_store.get(name)?.map(|p| p.value.to_string()))
    }
}

/// Parse a string value into a ParameterValue, matching the type of the reference value.
///
/// This allows governance proposals to specify new values as strings while ensuring
/// the resulting ParameterValue has the correct type for the parameter.
fn parse_value_like(
    value_str: &str,
    reference: &icn_kernel_api::protocol_params::ParameterValue,
) -> Result<icn_kernel_api::protocol_params::ParameterValue> {
    use icn_kernel_api::protocol_params::ParameterValue;

    match reference {
        ParameterValue::Integer(_) => {
            let v = value_str
                .parse::<i64>()
                .map_err(|e| anyhow::anyhow!("Invalid integer value '{}': {}", value_str, e))?;
            Ok(ParameterValue::Integer(v))
        }
        ParameterValue::Float(_) => {
            let v = value_str
                .parse::<f64>()
                .map_err(|e| anyhow::anyhow!("Invalid float value '{}': {}", value_str, e))?;
            Ok(ParameterValue::Float(v))
        }
        ParameterValue::String(_) => {
            // Strip surrounding quotes if present
            let v = value_str.trim_matches('"').to_string();
            Ok(ParameterValue::String(v))
        }
        ParameterValue::Boolean(_) => {
            let v = match value_str.to_lowercase().as_str() {
                "true" | "1" | "yes" => true,
                "false" | "0" | "no" => false,
                _ => {
                    return Err(anyhow::anyhow!(
                        "Invalid boolean value '{}': expected true/false",
                        value_str
                    ))
                }
            };
            Ok(ParameterValue::Boolean(v))
        }
        ParameterValue::Duration(_) => {
            // Parse duration strings like "7200" (seconds), "2h", "1d"
            let v = parse_duration_seconds(value_str)?;
            Ok(ParameterValue::Duration(v))
        }
        ParameterValue::Bytes(_) => {
            // Parse byte sizes like "1024", "1KB", "1MB"
            let v = parse_bytes(value_str)?;
            Ok(ParameterValue::Bytes(v))
        }
        ParameterValue::Percentage(_) => {
            let v = value_str
                .trim_end_matches('%')
                .parse::<f64>()
                .map_err(|e| anyhow::anyhow!("Invalid percentage value '{}': {}", value_str, e))?;
            Ok(ParameterValue::Percentage(v))
        }
    }
}

/// Parse a duration string into seconds.
fn parse_duration_seconds(s: &str) -> Result<u64> {
    let s = s.trim();

    // Try parsing as plain number (seconds)
    if let Ok(v) = s.parse::<u64>() {
        return Ok(v);
    }

    // Try parsing with suffix
    let (num_str, multiplier) = if let Some(n) = s.strip_suffix('d') {
        (n, 86400u64)
    } else if let Some(n) = s.strip_suffix('h') {
        (n, 3600u64)
    } else if let Some(n) = s.strip_suffix('m') {
        (n, 60u64)
    } else if let Some(n) = s.strip_suffix('s') {
        (n, 1u64)
    } else {
        return Err(anyhow::anyhow!(
            "Invalid duration format '{}': expected number or suffix (s/m/h/d)",
            s
        ));
    };

    let num = num_str
        .trim()
        .parse::<u64>()
        .map_err(|e| anyhow::anyhow!("Invalid duration value '{}': {}", s, e))?;

    Ok(num * multiplier)
}

/// Parse a byte size string into bytes.
fn parse_bytes(s: &str) -> Result<u64> {
    let s = s.trim().to_uppercase();

    // Try parsing as plain number
    if let Ok(v) = s.parse::<u64>() {
        return Ok(v);
    }

    // Try parsing with suffix
    let (num_str, multiplier) = if let Some(n) = s.strip_suffix("GB") {
        (n, 1024u64 * 1024 * 1024)
    } else if let Some(n) = s.strip_suffix("MB") {
        (n, 1024u64 * 1024)
    } else if let Some(n) = s.strip_suffix("KB") {
        (n, 1024u64)
    } else if let Some(n) = s.strip_suffix('B') {
        (n, 1u64)
    } else {
        return Err(anyhow::anyhow!(
            "Invalid byte size format '{}': expected number or suffix (B/KB/MB/GB)",
            s
        ));
    };

    let num = num_str
        .trim()
        .parse::<u64>()
        .map_err(|e| anyhow::anyhow!("Invalid byte size value '{}': {}", s, e))?;

    Ok(num * multiplier)
}

/// Convert a TreasuryEffect to a TreasuryOperation
fn treasury_effect_to_operation(
    effect: &icn_kernel_api::effects::TreasuryEffect,
) -> TreasuryOperation {
    use icn_kernel_api::effects::TreasuryEffect;
    match effect {
        TreasuryEffect::Spend {
            treasury_did,
            recipient_did,
            amount,
            currency,
            memo,
            decision_hash,
            ..
        } => TreasuryOperation {
            treasury_id: treasury_did.clone(),
            operation_type: icn_kernel_api::governance::TreasuryOperationType::Spend,
            amount: *amount,
            currency: currency.clone(),
            recipient: Some(recipient_did.clone()),
            memo: memo.clone(),
            decision_hash: Some(decision_hash.clone()),
        },
        TreasuryEffect::CreateBudget {
            treasury_did,
            budget_id,
            total_amount,
            currency,
            name,
            ..
        } => TreasuryOperation {
            treasury_id: treasury_did.clone(),
            operation_type: icn_kernel_api::governance::TreasuryOperationType::Allocate,
            amount: *total_amount,
            currency: currency.clone(),
            recipient: Some(budget_id.clone()),
            memo: name.clone(),
            decision_hash: None, // CreateBudget doesn't carry provenance
        },
        TreasuryEffect::Allocate {
            treasury_did,
            budget_id,
            amount,
            currency,
        } => TreasuryOperation {
            treasury_id: treasury_did.clone(),
            operation_type: icn_kernel_api::governance::TreasuryOperationType::Allocate,
            amount: *amount,
            currency: currency.clone(),
            recipient: Some(budget_id.clone()),
            memo: "Budget allocation".to_string(),
            decision_hash: None, // No decision provenance for this variant
        },
        TreasuryEffect::Transfer {
            from_did,
            to_did,
            amount,
            currency,
            memo,
        } => TreasuryOperation {
            treasury_id: from_did.clone(),
            operation_type: icn_kernel_api::governance::TreasuryOperationType::Spend,
            amount: *amount,
            currency: currency.clone(),
            recipient: Some(to_did.clone()),
            memo: memo.clone(),
            decision_hash: None, // No decision provenance for this variant
        },
        // Other treasury effects mapped to basic operations
        _ => TreasuryOperation {
            treasury_id: String::new(),
            operation_type: icn_kernel_api::governance::TreasuryOperationType::Reserve,
            amount: 0,
            currency: "UNKNOWN".to_string(),
            recipient: None,
            memo: "Unmapped treasury effect".to_string(),
            decision_hash: None,
        },
    }
}

/// Convert a ProtocolEffect to a ProtocolChange (if applicable)
fn protocol_effect_to_change(
    effect: &icn_kernel_api::effects::ProtocolEffect,
) -> Option<ProtocolChange> {
    use icn_kernel_api::effects::ProtocolEffect;

    match effect {
        ProtocolEffect::SetParameter {
            parameter_name,
            old_value_hash,
            new_value_json,
            effective_at,
        } => Some(ProtocolChange {
            parameter_name: parameter_name.clone(),
            old_value: old_value_hash.clone(),
            new_value: new_value_json.clone(),
            effective_at: *effective_at,
        }),
        ProtocolEffect::SetGovernanceConfig {
            domain_id,
            config_json,
            ..
        } => Some(ProtocolChange {
            parameter_name: format!("governance.config.{}", domain_id),
            old_value: String::new(),
            new_value: config_json.clone(),
            effective_at: 0,
        }),
        _ => None,
    }
}

/// Convert an ExecutionOutcome to an EffectResult
fn execution_outcome_to_effect_result(outcome: ExecutionOutcome, effect_id: &str) -> EffectResult {
    match outcome {
        ExecutionOutcome::Success { effects, .. } => EffectResult {
            effect_id: effect_id.to_string(),
            success: true,
            message: effects.join("; "),
            state_change_hash: None,
        },
        ExecutionOutcome::Failed { reason, .. } => EffectResult {
            effect_id: effect_id.to_string(),
            success: false,
            message: reason,
            state_change_hash: None,
        },
        ExecutionOutcome::Deferred { reason, .. } => EffectResult {
            effect_id: effect_id.to_string(),
            success: true,
            message: format!("Deferred: {}", reason),
            state_change_hash: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_kernel_api::governance::TreasuryOperationType;
    use icn_kernel_api::protocol_params::{
        ParameterChange, ParameterScope, ParameterValidationError, ParameterValue, PendingChangeId,
        PendingParameterChange, ProtocolParameter,
    };

    /// Mock parameter store for testing
    struct MockParamStore {
        params: std::sync::RwLock<std::collections::HashMap<String, ProtocolParameter>>,
    }

    impl MockParamStore {
        fn new() -> Self {
            Self {
                params: std::sync::RwLock::new(std::collections::HashMap::new()),
            }
        }

        /// Helper to create a simple integer parameter
        fn create_param(id: &str, value: i64) -> ProtocolParameter {
            ProtocolParameter::new(
                id,
                id,
                format!("Test parameter {}", id),
                ParameterValue::Integer(value),
            )
        }

        /// Set a parameter by ID with an integer value
        fn set_integer(&self, id: &str, value: i64) {
            let param = Self::create_param(id, value);
            self.params.write().unwrap().insert(id.to_string(), param);
        }
    }

    impl ProtocolParameterStore for MockParamStore {
        fn get(&self, id: &str) -> Result<Option<ProtocolParameter>> {
            Ok(self.params.read().unwrap().get(id).cloned())
        }

        fn get_effective(
            &self,
            id: &str,
            _coop_id: Option<&str>,
            _fed_id: Option<&str>,
        ) -> Result<Option<ProtocolParameter>> {
            self.get(id)
        }

        fn set(
            &self,
            param: ProtocolParameter,
            _proposal_id: Option<String>,
            _changed_by: Option<String>,
        ) -> Result<()> {
            self.params.write().unwrap().insert(param.id.clone(), param);
            Ok(())
        }

        fn list(&self) -> Result<Vec<ProtocolParameter>> {
            Ok(self.params.read().unwrap().values().cloned().collect())
        }

        fn list_by_category(&self, category: &str) -> Result<Vec<ProtocolParameter>> {
            Ok(self
                .params
                .read()
                .unwrap()
                .values()
                .filter(|p| p.id.starts_with(category))
                .cloned()
                .collect())
        }

        fn get_history(&self, _id: &str) -> Result<Vec<ParameterChange>> {
            Ok(vec![])
        }

        fn get_history_paginated(
            &self,
            _id: &str,
            _offset: usize,
            _limit: usize,
        ) -> Result<(Vec<ParameterChange>, usize)> {
            Ok((vec![], 0))
        }

        fn prune_history(&self, _id: &str, _max_entries: usize) -> Result<usize> {
            Ok(0)
        }

        fn delete(&self, id: &str) -> Result<()> {
            self.params.write().unwrap().remove(id);
            Ok(())
        }

        fn exists(&self, id: &str) -> Result<bool> {
            Ok(self.params.read().unwrap().contains_key(id))
        }

        fn count(&self) -> Result<usize> {
            Ok(self.params.read().unwrap().len())
        }

        fn total_history_count(&self) -> Result<usize> {
            Ok(0)
        }

        fn validate(
            &self,
            _id: &str,
            _new_value: &ParameterValue,
        ) -> std::result::Result<(), ParameterValidationError> {
            Ok(())
        }

        fn list_scoped_parameters(&self) -> Result<Vec<ProtocolParameter>> {
            Ok(vec![])
        }

        fn delete_scoped_parameter(&self, _id: &str, _scope: &ParameterScope) -> Result<bool> {
            Ok(false)
        }

        fn add_pending_change(&self, _change: PendingParameterChange) -> Result<()> {
            Ok(())
        }

        fn get_pending_change(
            &self,
            _id: &PendingChangeId,
        ) -> Result<Option<PendingParameterChange>> {
            Ok(None)
        }

        fn list_pending_changes(&self) -> Result<Vec<PendingParameterChange>> {
            Ok(vec![])
        }

        fn list_pending_changes_for_parameter(
            &self,
            _parameter_id: &str,
        ) -> Result<Vec<PendingParameterChange>> {
            Ok(vec![])
        }

        fn get_changes_due_before(&self, _timestamp: u64) -> Result<Vec<PendingParameterChange>> {
            Ok(vec![])
        }

        fn update_pending_change(&self, _change: PendingParameterChange) -> Result<()> {
            Ok(())
        }

        fn cancel_pending_change(&self, _id: &PendingChangeId, _reason: &str) -> Result<()> {
            Ok(())
        }

        fn count_pending_changes(&self) -> Result<usize> {
            Ok(0)
        }
    }

    #[test]
    fn test_kernel_governance_executor_creation() {
        let store = Arc::new(MockParamStore::new());
        let executor = KernelGovernanceExecutor::new(store);

        // Verify we can access both sub-executors
        let _treasury = executor.treasury();
        let _protocol = executor.protocol();
    }

    #[tokio::test]
    async fn test_treasury_executor_placeholder() {
        let executor = KernelTreasuryExecutor::new();
        let receipt_id = DecisionReceiptId::new("test-receipt-1");
        let operation = TreasuryOperation {
            treasury_id: "treasury-1".to_string(),
            operation_type: TreasuryOperationType::Spend,
            amount: 100,
            currency: "HOURS".to_string(),
            recipient: Some("did:icn:recipient".to_string()),
            memo: "Test payment".to_string(),
            decision_hash: Some("sha256:abc123".to_string()),
        };

        let result = executor
            .execute_treasury_operation(&receipt_id, operation)
            .await;
        assert!(result.is_ok());

        match result.unwrap() {
            ExecutionOutcome::Success { effects, .. } => {
                assert!(!effects.is_empty());
            }
            _ => panic!("Expected success outcome"),
        }
    }

    #[tokio::test]
    async fn test_protocol_executor_apply_change() {
        let store = Arc::new(MockParamStore::new());
        // Pre-set the parameter to match old_value (7200 as integer)
        store.set_integer("governance.max_proposal_duration", 7200);

        let executor = KernelProtocolExecutor::new(store.clone());
        let receipt_id = DecisionReceiptId::new("test-receipt-2");
        let change = ProtocolChange {
            parameter_name: "governance.max_proposal_duration".to_string(),
            old_value: "7200".to_string(),
            new_value: "14400".to_string(),
            effective_at: 0,
        };

        let result = executor.apply_protocol_change(&receipt_id, change).await;
        assert!(result.is_ok());

        match result.unwrap() {
            ExecutionOutcome::Success { effects, .. } => {
                assert!(!effects.is_empty());
            }
            _ => panic!("Expected success outcome"),
        }

        // Verify the parameter was updated
        let param = store
            .get("governance.max_proposal_duration")
            .unwrap()
            .unwrap();
        assert_eq!(param.value, ParameterValue::Integer(14400));
    }

    #[tokio::test]
    async fn test_protocol_executor_concurrent_modification() {
        let store = Arc::new(MockParamStore::new());
        // Set a different value than expected (3600 instead of 7200)
        store.set_integer("governance.max_proposal_duration", 3600);

        let executor = KernelProtocolExecutor::new(store);
        let receipt_id = DecisionReceiptId::new("test-receipt-3");
        let change = ProtocolChange {
            parameter_name: "governance.max_proposal_duration".to_string(),
            old_value: "7200".to_string(), // Doesn't match current value of 3600
            new_value: "14400".to_string(),
            effective_at: 0,
        };

        let result = executor.apply_protocol_change(&receipt_id, change).await;
        assert!(result.is_ok());

        match result.unwrap() {
            ExecutionOutcome::Failed { reason, .. } => {
                assert!(reason.contains("has changed since proposal"));
            }
            _ => panic!("Expected failure outcome due to concurrent modification"),
        }
    }

    #[tokio::test]
    async fn test_protocol_executor_get_parameter() {
        let store = Arc::new(MockParamStore::new());
        store.set_integer("governance.test_param", 42);

        let executor = KernelProtocolExecutor::new(store);

        let result = executor.get_parameter("governance.test_param").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some("42".to_string()));

        let result = executor.get_parameter("nonexistent").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[tokio::test]
    async fn test_protocol_executor_parameter_not_found() {
        let store = Arc::new(MockParamStore::new());
        // Don't set any parameters

        let executor = KernelProtocolExecutor::new(store);
        let receipt_id = DecisionReceiptId::new("test-receipt-4");
        let change = ProtocolChange {
            parameter_name: "governance.nonexistent".to_string(),
            old_value: "100".to_string(),
            new_value: "200".to_string(),
            effective_at: 0,
        };

        let result = executor.apply_protocol_change(&receipt_id, change).await;
        assert!(result.is_ok());

        match result.unwrap() {
            ExecutionOutcome::Failed { reason, .. } => {
                assert!(reason.contains("not found"));
            }
            _ => panic!("Expected failure outcome for missing parameter"),
        }
    }

    #[tokio::test]
    async fn test_treasury_executor_get_balance() {
        let executor = KernelTreasuryExecutor::new();

        let result = executor.get_treasury_balance("treasury-1", "HOURS").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0); // Placeholder returns 0
    }

    #[test]
    fn test_parse_duration_seconds() {
        assert_eq!(parse_duration_seconds("3600").unwrap(), 3600);
        assert_eq!(parse_duration_seconds("1h").unwrap(), 3600);
        assert_eq!(parse_duration_seconds("2h").unwrap(), 7200);
        assert_eq!(parse_duration_seconds("1d").unwrap(), 86400);
        assert_eq!(parse_duration_seconds("30m").unwrap(), 1800);
        assert_eq!(parse_duration_seconds("60s").unwrap(), 60);
    }

    #[test]
    fn test_parse_bytes() {
        assert_eq!(parse_bytes("1024").unwrap(), 1024);
        assert_eq!(parse_bytes("1KB").unwrap(), 1024);
        assert_eq!(parse_bytes("1MB").unwrap(), 1024 * 1024);
        assert_eq!(parse_bytes("1GB").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_bytes("100B").unwrap(), 100);
    }

    #[test]
    fn test_parse_value_like() {
        // Integer
        let ref_int = ParameterValue::Integer(0);
        assert_eq!(
            parse_value_like("42", &ref_int).unwrap(),
            ParameterValue::Integer(42)
        );

        // Boolean
        let ref_bool = ParameterValue::Boolean(false);
        assert_eq!(
            parse_value_like("true", &ref_bool).unwrap(),
            ParameterValue::Boolean(true)
        );

        // String
        let ref_str = ParameterValue::String(String::new());
        assert_eq!(
            parse_value_like("hello", &ref_str).unwrap(),
            ParameterValue::String("hello".to_string())
        );

        // Duration
        let ref_dur = ParameterValue::Duration(0);
        assert_eq!(
            parse_value_like("2h", &ref_dur).unwrap(),
            ParameterValue::Duration(7200)
        );

        // Bytes
        let ref_bytes = ParameterValue::Bytes(0);
        assert_eq!(
            parse_value_like("1MB", &ref_bytes).unwrap(),
            ParameterValue::Bytes(1024 * 1024)
        );

        // Percentage
        let ref_pct = ParameterValue::Percentage(0.0);
        assert_eq!(
            parse_value_like("50%", &ref_pct).unwrap(),
            ParameterValue::Percentage(50.0)
        );
    }
}
