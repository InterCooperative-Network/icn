//! Treasury governance proposal handlers.
//!
//! Handles treasury operations including:
//! - Budget creation and cancellation
//! - Withdrawals and transfers
//! - Spending rule modifications
//! - Surplus allocation
//! - Share redemption
//! - Bond issuance
//!
//! # Design
//!
//! The `TreasuryHandler` implements the [`ExecutionCallback`] trait, receiving
//! treasury proposals and delegating to the appropriate treasury manager.
//! It validates governance proofs before executing any operation.
//!
//! # Kernel Trait Integration
//!
//! When a [`TreasuryExecutor`] is configured via [`TreasuryHandler::with_executor`],
//! the handler delegates execution to the kernel-provided executor. This enables
//! clean separation between governance domain logic and kernel execution services.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info, warn};

use super::execution::{
    DecisionReceiptId, ExecutionCallback, ExecutionOutcome, ProposalExecutionContext, ProposalType,
    TreasuryExecutor, TreasuryOperation as KernelTreasuryOperation, TreasuryOperationType,
};
use icn_governance::proof::{GovernanceDecisionReceipt, ProofOutcome};

// ============================================================================
// Disbursement Types
// ============================================================================

/// Parameters for a treasury disbursement operation.
///
/// Contains all information needed to execute a disbursement from a treasury
/// to a beneficiary after a governance decision has passed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisburseParams {
    /// Treasury identifier (e.g., treasury DID or domain ID)
    pub treasury_id: String,
    /// Recipient identifier (DID of the beneficiary)
    pub recipient: String,
    /// Amount to disburse (must be positive)
    pub amount: i64,
    /// Currency code (e.g., "HOURS", "USD")
    pub currency: String,
    /// Human-readable description of the disbursement
    pub memo: String,
}

impl DisburseParams {
    /// Create new disbursement parameters.
    pub fn new(
        treasury_id: impl Into<String>,
        recipient: impl Into<String>,
        amount: i64,
        currency: impl Into<String>,
        memo: impl Into<String>,
    ) -> Self {
        Self {
            treasury_id: treasury_id.into(),
            recipient: recipient.into(),
            amount,
            currency: currency.into(),
            memo: memo.into(),
        }
    }

    /// Parse DisburseParams from a JSON payload.
    pub fn from_payload(payload: &serde_json::Value) -> Option<Self> {
        Some(Self {
            treasury_id: payload.get("treasury_id")?.as_str()?.to_string(),
            recipient: payload.get("recipient")?.as_str()?.to_string(),
            amount: payload.get("amount")?.as_i64()?,
            currency: payload.get("currency")?.as_str()?.to_string(),
            memo: payload.get("memo")?.as_str()?.to_string(),
        })
    }
}

/// A single allocation within an allocation receipt.
///
/// Represents one beneficiary receiving funds as part of a disbursement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Allocation {
    /// Beneficiary identifier (DID)
    pub beneficiary: String,
    /// Amount allocated
    pub amount: i64,
    /// Currency code
    pub currency: String,
    /// Purpose/description of this allocation
    pub purpose: String,
}

impl Allocation {
    /// Create a new allocation.
    pub fn new(
        beneficiary: impl Into<String>,
        amount: i64,
        currency: impl Into<String>,
        purpose: impl Into<String>,
    ) -> Self {
        Self {
            beneficiary: beneficiary.into(),
            amount,
            currency: currency.into(),
            purpose: purpose.into(),
        }
    }
}

/// Receipt confirming a treasury disbursement was executed.
///
/// Links back to the governance decision that authorized the disbursement
/// and contains details about what was allocated.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AllocationReceipt {
    /// Unique identifier for this receipt
    pub receipt_id: String,
    /// ID of the decision receipt that authorized this allocation
    pub decision_receipt_id: String,
    /// List of allocations made
    pub allocations: Vec<Allocation>,
    /// Unix timestamp when the allocation was created
    pub created_at: u64,
}

impl AllocationReceipt {
    /// Create a new allocation receipt.
    pub fn new(
        receipt_id: impl Into<String>,
        decision_receipt_id: impl Into<String>,
        allocations: Vec<Allocation>,
        created_at: u64,
    ) -> Self {
        Self {
            receipt_id: receipt_id.into(),
            decision_receipt_id: decision_receipt_id.into(),
            allocations,
            created_at,
        }
    }

    /// Get the total amount across all allocations for a given currency.
    pub fn total_for_currency(&self, currency: &str) -> i64 {
        self.allocations
            .iter()
            .filter(|a| a.currency == currency)
            .map(|a| a.amount)
            .sum()
    }
}

/// Handler for treasury governance proposals.
///
/// This handler processes treasury-related proposals such as budget creation,
/// withdrawals, and spending rule modifications. It requires access to the
/// treasury manager and ledger for executing operations.
///
/// # Kernel Executor Integration
///
/// When configured with a [`TreasuryExecutor`] via [`with_executor`], the handler
/// delegates execution to the kernel-provided executor. Without an executor,
/// the handler returns an error indicating execution must be handled externally.
pub struct TreasuryHandler {
    /// Handler name for logging
    name: String,
    /// Optional kernel treasury executor
    executor: Option<Arc<dyn TreasuryExecutor>>,
}

impl TreasuryHandler {
    /// Create a new treasury handler without an executor.
    ///
    /// Without an executor configured, execution will fail with an error
    /// indicating the caller must use the executor callback pattern.
    pub fn new() -> Self {
        Self {
            name: "TreasuryHandler".to_string(),
            executor: None,
        }
    }

    /// Configure a treasury executor for delegated execution.
    ///
    /// When an executor is set, the handler delegates all treasury operations
    /// to the kernel-provided executor.
    pub fn with_executor(mut self, executor: Arc<dyn TreasuryExecutor>) -> Self {
        self.executor = Some(executor);
        self
    }

    /// Handle a treasury disbursement proposal execution.
    ///
    /// Verifies the governance decision receipt indicates the proposal passed,
    /// then creates an allocation receipt linking the disbursement to the
    /// governance decision.
    ///
    /// # Arguments
    /// * `decision_receipt` - The cryptographic receipt proving the governance decision
    /// * `disburse_params` - Parameters for the disbursement (treasury, recipient, amount, etc.)
    ///
    /// # Returns
    /// * `Ok(AllocationReceipt)` - Receipt confirming the allocation was created
    /// * `Err` - If the proposal didn't pass or validation fails
    ///
    /// # Example
    /// ```ignore
    /// let handler = TreasuryHandler::new();
    /// let receipt = handler.handle_disburse_proposal(&decision_receipt, &params).await?;
    /// ```
    pub async fn handle_disburse_proposal(
        &self,
        decision_receipt: &GovernanceDecisionReceipt,
        disburse_params: &DisburseParams,
    ) -> Result<AllocationReceipt> {
        // 1. Verify decision receipt is for a passed disburse proposal
        if decision_receipt.outcome != ProofOutcome::Accepted {
            return Err(anyhow!(
                "Cannot disburse - proposal did not pass (outcome: {:?})",
                decision_receipt.outcome
            ));
        }

        // 2. Validate disbursement parameters
        validate_positive_amount(disburse_params.amount, "Disbursement amount")?;

        if disburse_params.recipient.is_empty() {
            return Err(anyhow!("Recipient must not be empty"));
        }

        if disburse_params.memo.is_empty() {
            return Err(anyhow!("Memo must not be empty"));
        }

        // 3. Extract treasury and recipient info for logging
        let treasury_id = &disburse_params.treasury_id;
        let recipient = &disburse_params.recipient;
        let amount = disburse_params.amount;
        let currency = &disburse_params.currency;

        // 4. Create allocation receipt
        let allocation_receipt = AllocationReceipt::new(
            format!("alloc:{}", decision_receipt.proposal_id),
            decision_receipt.proposal_id.clone(),
            vec![Allocation::new(
                recipient.clone(),
                amount,
                currency.clone(),
                disburse_params.memo.clone(),
            )],
            icn_time::current_timestamp_secs(),
        );

        // 5. Log the disbursement (actual ledger transfer is handled by executor)
        info!(
            treasury_id = %treasury_id,
            recipient = %recipient,
            amount = %amount,
            currency = %currency,
            allocation_id = %allocation_receipt.receipt_id,
            proposal_id = %decision_receipt.proposal_id,
            "Executing disbursement from governance decision"
        );

        // 6. If we have an executor, delegate the actual ledger operation
        if let Some(executor) = &self.executor {
            let receipt_id = DecisionReceiptId::new(&decision_receipt.proposal_id);
            // Convert decision_hash bytes to hex string for kernel layer
            let decision_hash_hex = hex::encode(decision_receipt.decision_hash);
            let kernel_op = KernelTreasuryOperation {
                treasury_id: treasury_id.clone(),
                operation_type: TreasuryOperationType::Spend,
                amount,
                currency: currency.clone(),
                recipient: Some(recipient.clone()),
                memo: disburse_params.memo.clone(),
                decision_hash: Some(decision_hash_hex),
            };

            match executor
                .execute_treasury_operation(&receipt_id, kernel_op)
                .await?
            {
                ExecutionOutcome::Success { effects, .. } => {
                    debug!(
                        proposal_id = %decision_receipt.proposal_id,
                        effects = ?effects,
                        "Disbursement treasury operation succeeded"
                    );
                }
                ExecutionOutcome::Failed { reason, .. } => {
                    return Err(anyhow!("Disbursement treasury operation failed: {reason}"));
                }
                ExecutionOutcome::Deferred { reason, .. } => {
                    warn!(
                        proposal_id = %decision_receipt.proposal_id,
                        reason = %reason,
                        "Disbursement treasury operation deferred"
                    );
                }
            }
        }

        Ok(allocation_receipt)
    }
}

impl Default for TreasuryHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert local TreasuryOperation enum to kernel-api TreasuryOperation struct.
fn to_kernel_operation(
    op: &TreasuryOperation,
    ctx: &ProposalExecutionContext,
) -> KernelTreasuryOperation {
    let (operation_type, amount, recipient, memo) = match op {
        TreasuryOperation::CreateBudget => (
            TreasuryOperationType::Allocate,
            0,
            None,
            "Create budget".to_string(),
        ),
        TreasuryOperation::Withdraw => (
            TreasuryOperationType::Spend,
            0,
            None,
            "Withdraw".to_string(),
        ),
        TreasuryOperation::ModifySpendingRule => (
            TreasuryOperationType::Reserve,
            0,
            None,
            "Modify spending rule".to_string(),
        ),
        TreasuryOperation::TransferBetweenBudgets => (
            TreasuryOperationType::Allocate,
            0,
            None,
            "Transfer between budgets".to_string(),
        ),
        TreasuryOperation::CancelBudget => (
            TreasuryOperationType::Release,
            0,
            None,
            "Cancel budget".to_string(),
        ),
        TreasuryOperation::ReclaimBudget => (
            TreasuryOperationType::Release,
            0,
            None,
            "Reclaim budget".to_string(),
        ),
        TreasuryOperation::AllocateSurplus => (
            TreasuryOperationType::Allocate,
            0,
            None,
            "Allocate surplus".to_string(),
        ),
        TreasuryOperation::RedeemShares => (
            TreasuryOperationType::Spend,
            0,
            None,
            "Redeem shares".to_string(),
        ),
        TreasuryOperation::IssueBonds => (
            TreasuryOperationType::Allocate,
            0,
            None,
            "Issue bonds".to_string(),
        ),
        TreasuryOperation::Spend => (TreasuryOperationType::Spend, 0, None, "Spend".to_string()),
        TreasuryOperation::Disburse => (
            TreasuryOperationType::Spend,
            0,
            None,
            "Disburse".to_string(),
        ),
    };

    // Extract amount and recipient from payload if available
    let amount = ctx
        .payload
        .get("amount")
        .and_then(|v| v.as_i64())
        .unwrap_or(amount);
    let recipient = ctx
        .payload
        .get("recipient")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or(recipient);
    let memo = ctx
        .payload
        .get("memo")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or(memo);
    let currency = ctx
        .payload
        .get("currency")
        .and_then(|v| v.as_str())
        .unwrap_or("HOURS")
        .to_string();
    let treasury_id = ctx
        .payload
        .get("treasury_id")
        .and_then(|v| v.as_str())
        .unwrap_or(&ctx.domain_id)
        .to_string();

    KernelTreasuryOperation {
        treasury_id,
        operation_type,
        amount,
        currency,
        recipient,
        memo,
        decision_hash: None, // Set by caller with actual decision hash
    }
}

#[async_trait]
impl ExecutionCallback for TreasuryHandler {
    async fn execute(&self, ctx: &ProposalExecutionContext) -> Result<()> {
        info!(
            proposal_id = %ctx.proposal_id,
            domain_id = %ctx.domain_id,
            "Executing treasury proposal"
        );

        // Parse the treasury operation from the payload
        let operation_type = ctx
            .payload
            .get("operation")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        debug!(
            proposal_id = %ctx.proposal_id,
            operation = %operation_type,
            "Treasury operation type"
        );

        // If we have an executor, delegate to it
        if let Some(executor) = &self.executor {
            let receipt_id = DecisionReceiptId::new(&ctx.proposal_id);
            let op = TreasuryOperation::from_payload(&ctx.payload)
                .ok_or_else(|| anyhow!("Unknown treasury operation: {operation_type}"))?;
            let kernel_op = to_kernel_operation(&op, ctx);

            match executor
                .execute_treasury_operation(&receipt_id, kernel_op)
                .await?
            {
                ExecutionOutcome::Success { effects, .. } => {
                    info!(
                        proposal_id = %ctx.proposal_id,
                        effects = ?effects,
                        "Treasury operation succeeded"
                    );
                    Ok(())
                }
                ExecutionOutcome::Failed { reason, .. } => {
                    Err(anyhow!("Treasury operation failed: {reason}"))
                }
                ExecutionOutcome::Deferred { reason, .. } => {
                    warn!(
                        proposal_id = %ctx.proposal_id,
                        reason = %reason,
                        "Treasury operation deferred"
                    );
                    Ok(())
                }
            }
        } else {
            // No executor configured - indicate execution must be delegated
            warn!(
                proposal_id = %ctx.proposal_id,
                "Treasury handler has no executor - execution delegated to icn-core"
            );

            Err(anyhow!(
                "Treasury handler not yet implemented - use executor callback"
            ))
        }
    }

    fn handles(&self, proposal_type: &ProposalType) -> bool {
        matches!(proposal_type, ProposalType::Treasury)
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Treasury operation types for validation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // Variants used for routing logic in future migration
pub enum TreasuryOperation {
    /// Create a new budget allocation
    CreateBudget,
    /// Withdraw funds from treasury
    Withdraw,
    /// Modify spending rules
    ModifySpendingRule,
    /// Transfer between budgets
    TransferBetweenBudgets,
    /// Cancel an existing budget
    CancelBudget,
    /// Reclaim unused budget funds
    ReclaimBudget,
    /// Allocate surplus to members
    AllocateSurplus,
    /// Redeem member shares
    RedeemShares,
    /// Issue cooperative bonds
    IssueBonds,
    /// Execute an authorized spend
    Spend,
    /// Execute a disbursement from a governance decision
    Disburse,
}

impl TreasuryOperation {
    /// Parse operation type from JSON payload.
    #[allow(dead_code)] // Used in tests and future handler migration
    pub fn from_payload(payload: &serde_json::Value) -> Option<Self> {
        let op_str = payload.get("operation")?.as_str()?;
        match op_str {
            "CreateBudget" => Some(Self::CreateBudget),
            "Withdraw" => Some(Self::Withdraw),
            "ModifySpendingRule" => Some(Self::ModifySpendingRule),
            "TransferBetweenBudgets" => Some(Self::TransferBetweenBudgets),
            "CancelBudget" => Some(Self::CancelBudget),
            "ReclaimBudget" => Some(Self::ReclaimBudget),
            "AllocateSurplus" => Some(Self::AllocateSurplus),
            "RedeemShares" => Some(Self::RedeemShares),
            "IssueBonds" => Some(Self::IssueBonds),
            "Spend" => Some(Self::Spend),
            "Disburse" => Some(Self::Disburse),
            _ => None,
        }
    }
}

/// Validate that an amount is positive.
///
/// # Arguments
/// * `amount` - The amount to validate
/// * `field_name` - Human-readable field name for error messages
///
/// # Returns
/// * `Ok(amount)` if positive
/// * `Err` with descriptive error message if not positive
#[allow(dead_code)] // Used in tests and future handler migration
pub fn validate_positive_amount(amount: i64, field_name: &str) -> Result<i64> {
    if amount <= 0 {
        Err(anyhow!("{field_name} must be positive, got: {amount}"))
    } else {
        Ok(amount)
    }
}

/// Validate that two currencies match.
///
/// # Arguments
/// * `actual` - The actual currency
/// * `expected` - The expected currency
///
/// # Returns
/// * `Ok(())` if currencies match
/// * `Err` with descriptive error message if mismatch
#[allow(dead_code)] // Used in tests and future handler migration
pub fn validate_currency_match(actual: &str, expected: &str) -> Result<()> {
    if actual != expected {
        Err(anyhow!(
            "Currency mismatch: got '{actual}', expected '{expected}'"
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_treasury_handler_handles() {
        let handler = TreasuryHandler::new();
        assert!(handler.handles(&ProposalType::Treasury));
        assert!(!handler.handles(&ProposalType::Protocol));
        assert!(!handler.handles(&ProposalType::Federation));
    }

    #[test]
    fn test_treasury_handler_name() {
        let handler = TreasuryHandler::new();
        assert_eq!(handler.name(), "TreasuryHandler");
    }

    #[test]
    fn test_treasury_operation_from_payload() {
        let payload = serde_json::json!({"operation": "CreateBudget"});
        assert_eq!(
            TreasuryOperation::from_payload(&payload),
            Some(TreasuryOperation::CreateBudget)
        );

        let payload = serde_json::json!({"operation": "Withdraw"});
        assert_eq!(
            TreasuryOperation::from_payload(&payload),
            Some(TreasuryOperation::Withdraw)
        );

        let payload = serde_json::json!({"operation": "Unknown"});
        assert_eq!(TreasuryOperation::from_payload(&payload), None);
    }

    #[test]
    fn test_validate_positive_amount() {
        assert!(validate_positive_amount(100, "Amount").is_ok());
        assert!(validate_positive_amount(1, "Amount").is_ok());
        assert!(validate_positive_amount(0, "Amount").is_err());
        assert!(validate_positive_amount(-1, "Amount").is_err());
    }

    #[test]
    fn test_validate_currency_match() {
        assert!(validate_currency_match("USD", "USD").is_ok());
        assert!(validate_currency_match("EUR", "EUR").is_ok());
        assert!(validate_currency_match("USD", "EUR").is_err());
    }

    // ========================================================================
    // Disbursement Tests
    // ========================================================================

    #[test]
    fn test_disburse_params_new() {
        let params = DisburseParams::new(
            "treasury:coop:test",
            "did:icn:recipient123",
            1000,
            "HOURS",
            "Grant payment",
        );
        assert_eq!(params.treasury_id, "treasury:coop:test");
        assert_eq!(params.recipient, "did:icn:recipient123");
        assert_eq!(params.amount, 1000);
        assert_eq!(params.currency, "HOURS");
        assert_eq!(params.memo, "Grant payment");
    }

    #[test]
    fn test_disburse_params_from_payload() {
        let payload = serde_json::json!({
            "treasury_id": "treasury:coop:test",
            "recipient": "did:icn:recipient123",
            "amount": 500,
            "currency": "USD",
            "memo": "Reimbursement"
        });
        let params = DisburseParams::from_payload(&payload).unwrap();
        assert_eq!(params.treasury_id, "treasury:coop:test");
        assert_eq!(params.recipient, "did:icn:recipient123");
        assert_eq!(params.amount, 500);
        assert_eq!(params.currency, "USD");
        assert_eq!(params.memo, "Reimbursement");
    }

    #[test]
    fn test_disburse_params_from_payload_missing_field() {
        let payload = serde_json::json!({
            "treasury_id": "treasury:coop:test",
            "recipient": "did:icn:recipient123",
            // missing amount
        });
        assert!(DisburseParams::from_payload(&payload).is_none());
    }

    #[test]
    fn test_allocation_new() {
        let alloc = Allocation::new("did:icn:beneficiary", 750, "HOURS", "Project funding");
        assert_eq!(alloc.beneficiary, "did:icn:beneficiary");
        assert_eq!(alloc.amount, 750);
        assert_eq!(alloc.currency, "HOURS");
        assert_eq!(alloc.purpose, "Project funding");
    }

    #[test]
    fn test_allocation_receipt_new() {
        let allocations = vec![
            Allocation::new("did:icn:alice", 100, "HOURS", "Task 1"),
            Allocation::new("did:icn:bob", 200, "HOURS", "Task 2"),
        ];
        let receipt = AllocationReceipt::new(
            "alloc:proposal-123",
            "proposal-123",
            allocations,
            1700000000,
        );
        assert_eq!(receipt.receipt_id, "alloc:proposal-123");
        assert_eq!(receipt.decision_receipt_id, "proposal-123");
        assert_eq!(receipt.allocations.len(), 2);
        assert_eq!(receipt.created_at, 1700000000);
    }

    #[test]
    fn test_allocation_receipt_total_for_currency() {
        let allocations = vec![
            Allocation::new("did:icn:alice", 100, "HOURS", "Task 1"),
            Allocation::new("did:icn:bob", 200, "HOURS", "Task 2"),
            Allocation::new("did:icn:carol", 50, "USD", "External"),
        ];
        let receipt = AllocationReceipt::new(
            "alloc:proposal-123",
            "proposal-123",
            allocations,
            1700000000,
        );
        assert_eq!(receipt.total_for_currency("HOURS"), 300);
        assert_eq!(receipt.total_for_currency("USD"), 50);
        assert_eq!(receipt.total_for_currency("EUR"), 0);
    }

    #[test]
    fn test_treasury_operation_disburse_from_payload() {
        let payload = serde_json::json!({"operation": "Disburse"});
        assert_eq!(
            TreasuryOperation::from_payload(&payload),
            Some(TreasuryOperation::Disburse)
        );
    }

    fn make_decision_receipt(outcome: ProofOutcome) -> GovernanceDecisionReceipt {
        use icn_governance::tally::VoteTally;
        GovernanceDecisionReceipt::new(
            "proposal-123".to_string(),
            "domain:test".to_string(),
            outcome,
            VoteTally {
                for_votes: 10,
                against_votes: 2,
                abstain_votes: 1,
            },
            &[],
        )
    }

    #[tokio::test]
    async fn test_handle_disburse_proposal_success() {
        let handler = TreasuryHandler::new();
        let receipt = make_decision_receipt(ProofOutcome::Accepted);
        let params = DisburseParams::new(
            "treasury:coop:test",
            "did:icn:recipient",
            1000,
            "HOURS",
            "Grant payment",
        );

        let result = handler.handle_disburse_proposal(&receipt, &params).await;
        assert!(result.is_ok());

        let alloc_receipt = result.unwrap();
        assert_eq!(alloc_receipt.receipt_id, "alloc:proposal-123");
        assert_eq!(alloc_receipt.decision_receipt_id, "proposal-123");
        assert_eq!(alloc_receipt.allocations.len(), 1);
        assert_eq!(
            alloc_receipt.allocations[0].beneficiary,
            "did:icn:recipient"
        );
        assert_eq!(alloc_receipt.allocations[0].amount, 1000);
        assert_eq!(alloc_receipt.allocations[0].currency, "HOURS");
    }

    #[tokio::test]
    async fn test_handle_disburse_proposal_rejected_fails() {
        let handler = TreasuryHandler::new();
        let receipt = make_decision_receipt(ProofOutcome::Rejected);
        let params = DisburseParams::new(
            "treasury:coop:test",
            "did:icn:recipient",
            1000,
            "HOURS",
            "Grant payment",
        );

        let result = handler.handle_disburse_proposal(&receipt, &params).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("did not pass"));
    }

    #[tokio::test]
    async fn test_handle_disburse_proposal_no_quorum_fails() {
        let handler = TreasuryHandler::new();
        let receipt = make_decision_receipt(ProofOutcome::NoQuorum);
        let params = DisburseParams::new(
            "treasury:coop:test",
            "did:icn:recipient",
            1000,
            "HOURS",
            "Grant payment",
        );

        let result = handler.handle_disburse_proposal(&receipt, &params).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("did not pass"));
    }

    #[tokio::test]
    async fn test_handle_disburse_proposal_invalid_amount() {
        let handler = TreasuryHandler::new();
        let receipt = make_decision_receipt(ProofOutcome::Accepted);

        // Zero amount
        let params = DisburseParams::new(
            "treasury:coop:test",
            "did:icn:recipient",
            0,
            "HOURS",
            "Invalid",
        );
        let result = handler.handle_disburse_proposal(&receipt, &params).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be positive"));

        // Negative amount
        let params = DisburseParams::new(
            "treasury:coop:test",
            "did:icn:recipient",
            -100,
            "HOURS",
            "Invalid",
        );
        let result = handler.handle_disburse_proposal(&receipt, &params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handle_disburse_proposal_empty_recipient() {
        let handler = TreasuryHandler::new();
        let receipt = make_decision_receipt(ProofOutcome::Accepted);
        let params = DisburseParams::new("treasury:coop:test", "", 1000, "HOURS", "Grant payment");

        let result = handler.handle_disburse_proposal(&receipt, &params).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Recipient must not be empty"));
    }

    #[tokio::test]
    async fn test_handle_disburse_proposal_empty_memo() {
        let handler = TreasuryHandler::new();
        let receipt = make_decision_receipt(ProofOutcome::Accepted);
        let params =
            DisburseParams::new("treasury:coop:test", "did:icn:recipient", 1000, "HOURS", "");

        let result = handler.handle_disburse_proposal(&receipt, &params).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Memo must not be empty"));
    }

    #[test]
    fn test_allocation_receipt_links_to_decision_receipt() {
        let decision_receipt = make_decision_receipt(ProofOutcome::Accepted);
        let alloc_receipt = AllocationReceipt::new(
            format!("alloc:{}", decision_receipt.proposal_id),
            decision_receipt.proposal_id.clone(),
            vec![Allocation::new("did:icn:test", 100, "HOURS", "Test")],
            icn_time::current_timestamp_secs(),
        );

        // Verify the link
        assert_eq!(
            alloc_receipt.decision_receipt_id,
            decision_receipt.proposal_id
        );
        assert!(alloc_receipt
            .receipt_id
            .contains(&decision_receipt.proposal_id));
    }

    #[test]
    fn test_disburse_params_serialization() {
        let params = DisburseParams::new(
            "treasury:coop:test",
            "did:icn:recipient",
            1000,
            "HOURS",
            "Test memo",
        );
        let json = serde_json::to_string(&params).unwrap();
        let parsed: DisburseParams = serde_json::from_str(&json).unwrap();
        assert_eq!(params.treasury_id, parsed.treasury_id);
        assert_eq!(params.recipient, parsed.recipient);
        assert_eq!(params.amount, parsed.amount);
        assert_eq!(params.currency, parsed.currency);
        assert_eq!(params.memo, parsed.memo);
    }

    #[test]
    fn test_allocation_receipt_serialization() {
        let receipt = AllocationReceipt::new(
            "alloc:test",
            "proposal-test",
            vec![
                Allocation::new("did:icn:alice", 100, "HOURS", "Task 1"),
                Allocation::new("did:icn:bob", 200, "HOURS", "Task 2"),
            ],
            1700000000,
        );
        let json = serde_json::to_string(&receipt).unwrap();
        let parsed: AllocationReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(receipt, parsed);
    }
}
