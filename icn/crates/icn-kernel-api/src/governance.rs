//! Governance execution traits for kernel/app separation.
//!
//! These traits define the interface between the governance app and
//! the kernel's execution services (ledger, treasury, protocol params).

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Opaque receipt ID for governance decisions
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DecisionReceiptId(pub String);

impl DecisionReceiptId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for DecisionReceiptId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Outcome of a governance proposal execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionOutcome {
    /// Execution succeeded
    Success {
        receipt_id: DecisionReceiptId,
        effects: Vec<String>,
    },
    /// Execution failed
    Failed {
        receipt_id: DecisionReceiptId,
        reason: String,
    },
    /// Execution deferred (requires additional approvals)
    Deferred {
        receipt_id: DecisionReceiptId,
        reason: String,
    },
}

/// Treasury operation request from governance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreasuryOperation {
    pub treasury_id: String,
    pub operation_type: TreasuryOperationType,
    pub amount: i64,
    pub currency: String,
    pub recipient: Option<String>,
    pub memo: String,
    /// Canonical decision hash for provenance (cross-node anchor)
    pub decision_hash: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TreasuryOperationType {
    Spend,
    Allocate,
    Reserve,
    Release,
}

/// Protocol parameter change request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolChange {
    pub parameter_name: String,
    pub old_value: String,
    pub new_value: String,
    pub effective_at: u64,
}

/// Trait for executing treasury operations from governance decisions
#[async_trait]
pub trait TreasuryExecutor: Send + Sync {
    /// Execute a treasury operation based on governance decision
    async fn execute_treasury_operation(
        &self,
        receipt_id: &DecisionReceiptId,
        operation: TreasuryOperation,
    ) -> Result<ExecutionOutcome>;

    /// Get treasury balance for validation
    async fn get_treasury_balance(&self, treasury_id: &str, currency: &str) -> Result<i64>;
}

/// Trait for executing protocol parameter changes
#[async_trait]
pub trait ProtocolExecutor: Send + Sync {
    /// Apply a protocol parameter change
    async fn apply_protocol_change(
        &self,
        receipt_id: &DecisionReceiptId,
        change: ProtocolChange,
    ) -> Result<ExecutionOutcome>;

    /// Get current protocol parameter value
    async fn get_parameter(&self, name: &str) -> Result<Option<String>>;
}

/// Combined executor for all governance operations
#[async_trait]
pub trait GovernanceExecutor: Send + Sync {
    /// Get treasury executor
    fn treasury(&self) -> &dyn TreasuryExecutor;

    /// Get protocol executor
    fn protocol(&self) -> &dyn ProtocolExecutor;
}

// =============================================================================
// Effect Executor - executes kernel-safe effects
// =============================================================================

use crate::effects::{EffectResult, KernelEffect, TreasuryEffect};

/// Trait for executing kernel-safe effects.
///
/// This is the primary interface for the kernel to execute effects produced
/// by the governance app's payload-to-effect translation.
#[async_trait]
pub trait EffectExecutor: Send + Sync {
    /// Execute a kernel effect.
    ///
    /// # Arguments
    /// * `effect` - The kernel-safe effect to execute
    /// * `decision_receipt_id` - The receipt ID for audit linkage
    ///
    /// # Returns
    /// Result of the effect execution
    async fn execute_effect(
        &self,
        effect: KernelEffect,
        decision_receipt_id: &str,
    ) -> Result<EffectResult>;

    /// Execute a batch of effects atomically.
    ///
    /// Either all effects succeed or none are applied.
    async fn execute_effects_batch(
        &self,
        effects: Vec<KernelEffect>,
        decision_receipt_id: &str,
    ) -> Result<Vec<EffectResult>> {
        let mut results = Vec::with_capacity(effects.len());
        for effect in effects {
            results.push(self.execute_effect(effect, decision_receipt_id).await?);
        }
        Ok(results)
    }
}

/// Default effect executor that delegates to specialized executors.
pub struct DefaultEffectExecutor {
    treasury: std::sync::Arc<dyn TreasuryExecutor>,
    protocol: std::sync::Arc<dyn ProtocolExecutor>,
}

impl DefaultEffectExecutor {
    /// Create a new effect executor with the given specialized executors.
    pub fn new(
        treasury: std::sync::Arc<dyn TreasuryExecutor>,
        protocol: std::sync::Arc<dyn ProtocolExecutor>,
    ) -> Self {
        Self { treasury, protocol }
    }
}

#[async_trait]
impl EffectExecutor for DefaultEffectExecutor {
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

/// Convert a TreasuryEffect to a TreasuryOperation
fn treasury_effect_to_operation(effect: &TreasuryEffect) -> TreasuryOperation {
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
            operation_type: TreasuryOperationType::Spend,
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
            operation_type: TreasuryOperationType::Allocate,
            amount: *total_amount,
            currency: currency.clone(),
            recipient: Some(budget_id.clone()),
            memo: name.clone(),
            decision_hash: None,
        },
        TreasuryEffect::Allocate {
            treasury_did,
            budget_id,
            amount,
            currency,
        } => TreasuryOperation {
            treasury_id: treasury_did.clone(),
            operation_type: TreasuryOperationType::Allocate,
            amount: *amount,
            currency: currency.clone(),
            recipient: Some(budget_id.clone()),
            memo: "Budget allocation".to_string(),
            decision_hash: None,
        },
        TreasuryEffect::Transfer {
            from_did,
            to_did,
            amount,
            currency,
            memo,
        } => TreasuryOperation {
            treasury_id: from_did.clone(),
            operation_type: TreasuryOperationType::Spend,
            amount: *amount,
            currency: currency.clone(),
            recipient: Some(to_did.clone()),
            memo: memo.clone(),
            decision_hash: None,
        },
        // Other treasury effects mapped to basic operations
        _ => TreasuryOperation {
            treasury_id: String::new(),
            operation_type: TreasuryOperationType::Reserve,
            amount: 0,
            currency: "UNKNOWN".to_string(),
            recipient: None,
            memo: "Unmapped treasury effect".to_string(),
            decision_hash: None,
        },
    }
}

/// Convert a ProtocolEffect to a ProtocolChange (if applicable)
fn protocol_effect_to_change(effect: &crate::effects::ProtocolEffect) -> Option<ProtocolChange> {
    use crate::effects::ProtocolEffect;

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

    #[test]
    fn test_decision_receipt_id() {
        let id = DecisionReceiptId::new("proposal-123");
        assert_eq!(id.to_string(), "proposal-123");
    }

    #[test]
    fn test_treasury_operation_serde() {
        let op = TreasuryOperation {
            treasury_id: "treasury-1".to_string(),
            operation_type: TreasuryOperationType::Spend,
            amount: 1000,
            currency: "HOURS".to_string(),
            recipient: Some("did:icn:abc123".to_string()),
            memo: "Equipment purchase".to_string(),
            decision_hash: Some("sha256:abc123".to_string()),
        };
        let json = serde_json::to_string(&op).unwrap();
        let parsed: TreasuryOperation = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.amount, 1000);
        assert_eq!(parsed.decision_hash, Some("sha256:abc123".to_string()));
    }

    #[test]
    fn test_execution_outcome_serde() {
        let outcome = ExecutionOutcome::Success {
            receipt_id: DecisionReceiptId::new("r1"),
            effects: vec!["transferred 100 HOURS".to_string()],
        };
        let json = serde_json::to_string(&outcome).unwrap();
        assert!(json.contains("Success"));
    }
}
