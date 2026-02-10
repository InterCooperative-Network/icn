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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
        };
        let json = serde_json::to_string(&op).unwrap();
        let parsed: TreasuryOperation = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.amount, 1000);
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
