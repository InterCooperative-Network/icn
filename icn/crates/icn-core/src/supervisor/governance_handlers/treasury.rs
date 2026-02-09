//! Treasury governance proposal handlers
//!
//! Extracted from governance_handlers/mod.rs to reduce file size.
//! These handlers manage cooperative treasury operations including
//! budget creation, withdrawals, spending rules, transfers,
//! surplus allocation, share redemption, and bond issuance.

use tracing::{debug, error, info, warn};

use super::DeadLetterQueue;
use crate::dead_letter::{FailedOperation, FailureType};
use icn_governance::{GovernanceProofV2, ProofOutcome, ProposalId};
use icn_identity::Did;

/// Validate that an amount is positive, handling DLQ and metrics on failure.
///
/// Returns `true` if the amount is valid (positive), `false` otherwise.
/// On failure, logs an error, enqueues to DLQ, and increments the execution failure metric.
///
/// The error code in the DLQ metadata is derived from `field_name`:
/// - "Amount" → "invalid_amount"
/// - "Threshold amount" → "invalid_threshold_amount"
/// - "Transfer amount" → "invalid_transfer_amount"
/// - "Reclaim amount" → "invalid_reclaim_amount"
///
/// # Arguments
/// * `amount` - The amount to validate
/// * `proposal_id` - The proposal ID for error context
/// * `operation_key` - Key for DLQ (e.g., "budget", "rule", "transfer", "reclaim")
/// * `metric_label` - Label for the execution failure metric
/// * `field_name` - Human-readable field name for error messages (e.g., "Amount", "Threshold amount")
/// * `dlq` - Dead-letter queue for failed operations
pub(super) fn validate_positive_amount(
    amount: i64,
    proposal_id: &ProposalId,
    operation_key: &str,
    metric_label: &str,
    field_name: &str,
    dlq: &DeadLetterQueue,
) -> bool {
    if amount <= 0 {
        // Derive error code from field_name to preserve backward compatibility
        // e.g., "Threshold amount" -> "invalid_threshold_amount"
        let error_code = format!("invalid_{}", field_name.to_lowercase().replace(' ', "_"));

        error!(
            "❌ Invalid {} for proposal {}: {} (must be positive)",
            field_name.to_lowercase(),
            proposal_id.0,
            amount
        );
        let failed_op = FailedOperation::new(
            format!("treasury:{}:{}", operation_key, proposal_id.0),
            FailureType::TreasuryOperationFailed,
            serde_json::json!({
                "proposal_id": proposal_id.0,
                "error": error_code,
                "amount": amount,
            }),
            format!("{field_name} must be positive, got: {amount}"),
        );
        if let Err(dlq_err) = dlq.enqueue(failed_op) {
            error!("   Failed to write to dead-letter queue: {}", dlq_err);
        }
        icn_obs::metrics::governance::execution_failures_inc(metric_label);
        return false;
    }
    true
}

/// Validate that two currencies match, handling DLQ and metrics on failure.
///
/// Returns `true` if the currencies match, `false` otherwise.
/// On failure, logs an error, enqueues to DLQ, and increments the execution failure metric.
///
/// # Arguments
/// * `actual` - The actual currency value
/// * `expected` - The expected currency value
/// * `proposal_id` - The proposal ID for error context
/// * `operation_key` - Key for DLQ (e.g., "budget", "rule", "transfer", "reclaim")
/// * `metric_label` - Label for the execution failure metric
/// * `metadata` - JSON metadata for the DLQ entry
/// * `dlq` - Dead-letter queue for failed operations
pub(super) fn validate_currency_match(
    actual: &str,
    expected: &str,
    proposal_id: &ProposalId,
    operation_key: &str,
    metric_label: &str,
    metadata: serde_json::Value,
    dlq: &DeadLetterQueue,
) -> bool {
    if actual != expected {
        error!(
            "❌ Currency mismatch for proposal {}: got '{}', expected '{}'",
            proposal_id.0, actual, expected
        );
        let failed_op = FailedOperation::new(
            format!("treasury:{}:{}", operation_key, proposal_id.0),
            FailureType::TreasuryOperationFailed,
            metadata,
            format!("Currency mismatch: got '{actual}', expected '{expected}'"),
        );
        if let Err(dlq_err) = dlq.enqueue(failed_op) {
            error!("   Failed to write to dead-letter queue: {}", dlq_err);
        }
        icn_obs::metrics::governance::execution_failures_inc(metric_label);
        return false;
    }
    true
}

/// Validate governance proof for treasury spend execution.
///
/// The spend is authorized only when the proof is cryptographically valid and
/// matches the exact accepted proposal outcome in the expected domain/time.
fn validate_treasury_spend_proof(
    proof: &GovernanceProofV2,
    proposal_id: &ProposalId,
    domain_id: &str,
    decided_at: u64,
) -> Result<(), String> {
    if !proof.verify_receipt() {
        return Err(format!(
            "Invalid GovernanceDecisionReceipt for treasury spend proposal {}",
            proposal_id.0
        ));
    }

    if proof.receipt.proposal_id != proposal_id.0 {
        return Err(format!(
            "GovernanceProof proposal_id mismatch for treasury spend: expected {}, got {}",
            proposal_id.0, proof.receipt.proposal_id
        ));
    }

    if proof.receipt.domain_id != domain_id {
        return Err(format!(
            "GovernanceProof domain_id mismatch for treasury spend {}: expected {}, got {}",
            proposal_id.0, domain_id, proof.receipt.domain_id
        ));
    }

    if proof.receipt.outcome != ProofOutcome::Accepted {
        return Err(format!(
            "GovernanceProof outcome must be accepted for treasury spend {} (got: {})",
            proposal_id.0, proof.receipt.outcome
        ));
    }

    if proof.attestations.is_empty() {
        return Err(format!(
            "Missing GovernanceDecisionAttestation for treasury spend proposal {}",
            proposal_id.0
        ));
    }

    for attestation in &proof.attestations {
        if attestation.decision_hash != proof.receipt.decision_hash {
            return Err(format!(
                "GovernanceDecisionAttestation decision_hash mismatch for treasury spend proposal {}",
                proposal_id.0
            ));
        }
        let verifying_key = attestation
            .signer_did
            .parse::<Did>()
            .and_then(|did| did.to_verifying_key())
            .map_err(|e| {
                format!(
                    "Unable to resolve GovernanceDecisionAttestation signer DID '{}' for treasury spend {}: {}",
                    attestation.signer_did, proposal_id.0, e
                )
            })?;

        if !attestation.verify(&verifying_key) {
            return Err(format!(
                "Invalid GovernanceDecisionAttestation signature for treasury spend proposal {}",
                proposal_id.0
            ));
        }

        if attestation.timestamp != decided_at {
            return Err(format!(
                "GovernanceDecisionAttestation timestamp mismatch for treasury spend {}: expected {}, got {}",
                proposal_id.0, decided_at, attestation.timestamp
            ));
        }
    }

    Ok(())
}

impl super::GovernanceEventHandler {
    /// Handle a treasury proposal
    pub(super) fn handle_treasury_proposal(
        &self,
        proposal_id: ProposalId,
        operation: icn_governance::TreasuryProposalOperation,
        decided_at: u64,
        domain_id: String,
    ) {
        use icn_governance::TreasuryProposalOperation;

        info!(
            "🏦 Executing treasury proposal {}: {:?}",
            proposal_id.0,
            std::mem::discriminant(&operation)
        );

        match operation {
            TreasuryProposalOperation::CreateBudget {
                treasury_did,
                purpose,
                amount,
                currency,
                period_end,
            } => {
                self.handle_treasury_create_budget(
                    proposal_id,
                    treasury_did,
                    purpose,
                    amount,
                    currency,
                    period_end,
                );
            }
            TreasuryProposalOperation::Withdraw {
                treasury_did,
                recipient,
                amount,
                currency,
                purpose,
                budget_id,
            } => {
                self.handle_treasury_withdrawal(
                    proposal_id,
                    treasury_did,
                    recipient,
                    amount,
                    currency,
                    purpose,
                    budget_id,
                    decided_at,
                );
            }
            TreasuryProposalOperation::ModifySpendingRule {
                treasury_did,
                rule_id,
                name,
                threshold_amount,
                currency,
                approval_type,
                is_active,
            } => {
                self.handle_treasury_modify_spending_rule(
                    proposal_id,
                    treasury_did,
                    rule_id,
                    name,
                    threshold_amount,
                    currency,
                    approval_type,
                    is_active,
                );
            }
            TreasuryProposalOperation::TransferBetweenBudgets {
                treasury_did,
                from_budget,
                to_budget,
                amount,
                currency,
                reason,
            } => {
                self.handle_treasury_transfer_between_budgets(
                    proposal_id,
                    treasury_did,
                    from_budget,
                    to_budget,
                    amount,
                    currency,
                    reason,
                );
            }
            TreasuryProposalOperation::CancelBudget {
                budget_id,
                reason,
                return_to_treasury,
            } => {
                self.handle_treasury_cancel_budget(
                    proposal_id,
                    budget_id,
                    reason,
                    return_to_treasury,
                );
            }
            TreasuryProposalOperation::ReclaimBudget {
                budget_id,
                amount,
                currency,
                reason,
            } => {
                self.handle_treasury_reclaim_budget(
                    proposal_id,
                    budget_id,
                    amount,
                    currency,
                    reason,
                );
            }
            TreasuryProposalOperation::Spend {
                amount,
                recipient,
                memo,
                nonce,
            } => {
                self.handle_treasury_spend(
                    proposal_id,
                    amount,
                    recipient,
                    memo,
                    nonce,
                    decided_at,
                    domain_id,
                );
            }
        }
    }

    /// Handle treasury budget creation
    fn handle_treasury_create_budget(
        &self,
        proposal_id: ProposalId,
        treasury_did: Did,
        purpose: String,
        amount: i64,
        currency: String,
        period_end: Option<u64>,
    ) {
        info!(
            "🏦 Creating treasury budget for proposal {}: {} {} for '{}' (treasury: {})",
            proposal_id.0, amount, currency, purpose, treasury_did
        );

        let treasury_manager = self.treasury_manager.clone();
        let store = self.audit_store.clone();
        let created_by = self.treasury_did.clone();
        let dlq = self.dlq.clone();

        tokio::spawn(async move {
            let start = std::time::Instant::now();

            // Idempotency check
            let audit_key = format!("gov:audit:treasury:budget:{}", proposal_id.0);
            match store.get(audit_key.as_bytes()) {
                Ok(Some(_)) => {
                    debug!(
                        "Treasury budget proposal {} already executed, skipping",
                        proposal_id.0
                    );
                    icn_obs::metrics::governance::idempotent_skips_inc();
                    return;
                }
                Ok(None) => {}
                Err(e) => {
                    error!(
                        "🚨 Failed to check audit trail for treasury budget proposal {}: {}",
                        proposal_id.0, e
                    );
                    let failed_op = FailedOperation::new(
                        format!("treasury:budget:idem:{}", proposal_id.0),
                        FailureType::IdempotencyCheckFailed,
                        serde_json::json!({
                            "proposal_id": proposal_id.0,
                            "error": "idempotency_check_failed",
                        }),
                        format!("Failed to check idempotency: {e}"),
                    );
                    if let Err(dlq_err) = dlq.enqueue(failed_op) {
                        error!("   Failed to write to dead-letter queue: {}", dlq_err);
                    }
                    icn_obs::metrics::governance::execution_failures_inc("treasury_create_budget");
                    return;
                }
            }

            // Validation: Amount must be positive
            if !validate_positive_amount(
                amount,
                &proposal_id,
                "budget",
                "treasury_create_budget",
                "Amount",
                &dlq,
            ) {
                return;
            }

            let mut treasury_guard = treasury_manager.write().await;

            // Validation: Currency must match treasury's configured currency
            if let Some(treasury) = treasury_guard.get_treasury(&treasury_did) {
                if !validate_currency_match(
                    &currency,
                    &treasury.currency,
                    &proposal_id,
                    "budget",
                    "treasury_create_budget",
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "currency_mismatch",
                        "requested_currency": currency,
                        "treasury_currency": treasury.currency,
                    }),
                    &dlq,
                ) {
                    return;
                }
            } else {
                error!(
                    "❌ Treasury {} not found for budget proposal {}",
                    treasury_did, proposal_id.0
                );
                let failed_op = FailedOperation::new(
                    format!("treasury:budget:{}", proposal_id.0),
                    FailureType::TreasuryOperationFailed,
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "treasury_not_found",
                        "treasury_did": treasury_did.to_string(),
                    }),
                    format!("Treasury {treasury_did} not found"),
                );
                if let Err(dlq_err) = dlq.enqueue(failed_op) {
                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                }
                icn_obs::metrics::governance::execution_failures_inc("treasury_create_budget");
                return;
            }

            match treasury_guard.create_budget(
                treasury_did.clone(),
                purpose.clone(),
                amount,
                currency.clone(),
                period_end,
                created_by,
                Some(proposal_id.0.clone()),
            ) {
                Ok(budget) => {
                    info!(
                        "✅ Treasury budget created for proposal {}: budget_id={}, {} {}",
                        proposal_id.0, budget.id, amount, currency
                    );

                    // Record audit trail
                    let audit_record = serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "action": "create_budget",
                        "treasury_did": treasury_did.to_string(),
                        "budget_id": budget.id,
                        "amount": amount,
                        "currency": currency,
                        "purpose": purpose,
                        "executed_at": icn_time::current_timestamp_secs(),
                    });

                    if let Ok(audit_json) = serde_json::to_vec(&audit_record) {
                        if let Err(e) = store.put(audit_key.as_bytes(), &audit_json) {
                            error!(
                                "🚨 Failed to store audit trail for budget proposal {}: {}",
                                proposal_id.0, e
                            );
                            icn_obs::metrics::governance::audit_failures_inc();
                        }
                    }

                    let duration = start.elapsed().as_secs_f64();
                    icn_obs::metrics::governance::proposals_executed_inc("treasury_create_budget");
                    icn_obs::metrics::governance::execution_duration_record(
                        "treasury_create_budget",
                        duration,
                    );
                }
                Err(e) => {
                    error!(
                        "❌ Failed to create treasury budget for proposal {}: {}",
                        proposal_id.0, e
                    );

                    let failed_op = FailedOperation::new(
                        format!("treasury:budget:{}", proposal_id.0),
                        FailureType::TreasuryOperationFailed,
                        serde_json::json!({
                            "proposal_id": proposal_id.0,
                            "treasury_did": treasury_did.to_string(),
                            "amount": amount,
                            "currency": currency,
                            "purpose": purpose,
                        }),
                        e.to_string(),
                    );
                    if let Err(dlq_err) = dlq.enqueue(failed_op) {
                        error!("   Failed to write to dead-letter queue: {}", dlq_err);
                    }

                    icn_obs::metrics::governance::execution_failures_inc("treasury_create_budget");
                }
            }
        });
    }

    /// Handle treasury withdrawal
    fn handle_treasury_withdrawal(
        &self,
        proposal_id: ProposalId,
        treasury_did: Did,
        recipient: Did,
        amount: i64,
        currency: String,
        purpose: String,
        budget_id: Option<String>,
        decided_at: u64,
    ) {
        info!(
            "🏦 Executing treasury withdrawal for proposal {}: {} {} to {} ({})",
            proposal_id.0, amount, currency, recipient, purpose
        );

        let treasury_manager = self.treasury_manager.clone();

        // Clone values needed for both the audit spawn and budget proposal
        let proposal_id_for_audit = proposal_id.clone();
        let recipient_for_audit = recipient.clone();
        let currency_for_audit = currency.clone();
        let purpose_for_audit = purpose.clone();
        let budget_id_for_audit = budget_id.clone();

        // Record the audit trail for the treasury operation
        tokio::spawn(async move {
            use icn_ledger::treasury::TreasuryOperation;

            let mut treasury_guard = treasury_manager.write().await;

            let operation = TreasuryOperation::Withdraw {
                to: recipient_for_audit.clone(),
                amount,
                currency: currency_for_audit.clone(),
                purpose: purpose_for_audit,
                budget_id: budget_id_for_audit,
            };

            if let Err(e) = treasury_guard.record_audit(
                &treasury_did,
                operation,
                treasury_did.clone(), // performed_by (governance system)
                0,                    // balance_after - will be computed from actual ledger
                Some(proposal_id_for_audit.0.clone()),
                None, // ledger_entry_hash - will be set by budget proposal handler
            ) {
                error!(
                    "❌ Failed to record treasury audit for withdrawal proposal {}: {}",
                    proposal_id_for_audit.0, e
                );
            }
        });

        // Perform the actual ledger transfer
        self.handle_budget_proposal(proposal_id, amount, recipient, currency, decided_at);
    }

    /// Handle modify spending rule
    fn handle_treasury_modify_spending_rule(
        &self,
        proposal_id: ProposalId,
        treasury_did: Did,
        rule_id: Option<String>,
        name: String,
        threshold_amount: i64,
        currency: String,
        approval_type: icn_governance::TreasuryApprovalType,
        is_active: bool,
    ) {
        info!(
            "📋 Treasury spending rule {} for {}: threshold={}, active={}",
            rule_id.as_deref().unwrap_or("new"),
            treasury_did,
            threshold_amount,
            is_active
        );

        let treasury_manager = self.treasury_manager.clone();
        let store = self.audit_store.clone();
        let dlq = self.dlq.clone();

        tokio::spawn(async move {
            use icn_ledger::treasury::ApprovalType;

            let start = std::time::Instant::now();
            let audit_key = format!("gov:audit:treasury:rule:{}", proposal_id.0);

            // Idempotency check
            match store.get(audit_key.as_bytes()) {
                Ok(Some(_)) => {
                    debug!(
                        "Treasury spending rule proposal {} already executed",
                        proposal_id.0
                    );
                    icn_obs::metrics::governance::idempotent_skips_inc();
                    return;
                }
                Ok(None) => {}
                Err(e) => {
                    error!(
                        "🚨 Failed to check audit trail for spending rule proposal {}: {}",
                        proposal_id.0, e
                    );
                    let failed_op =
                        FailedOperation::idempotency_check_failure(&proposal_id.0, &e.to_string());
                    if let Err(dlq_err) = dlq.enqueue(failed_op) {
                        error!("   Failed to write to dead-letter queue: {}", dlq_err);
                    }
                    return;
                }
            }

            // Validation: Threshold amount must be positive
            if !validate_positive_amount(
                threshold_amount,
                &proposal_id,
                "rule",
                "treasury_modify_rule",
                "Threshold amount",
                &dlq,
            ) {
                return;
            }

            let mut treasury_guard = treasury_manager.write().await;

            // Validation: Currency must match treasury's configured currency
            if let Some(treasury) = treasury_guard.get_treasury(&treasury_did) {
                if !validate_currency_match(
                    &currency,
                    &treasury.currency,
                    &proposal_id,
                    "rule",
                    "treasury_modify_rule",
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "currency_mismatch",
                        "requested_currency": currency,
                        "treasury_currency": treasury.currency,
                    }),
                    &dlq,
                ) {
                    return;
                }
            } else {
                error!(
                    "❌ Treasury {} not found for spending rule proposal {}",
                    treasury_did, proposal_id.0
                );
                let failed_op = FailedOperation::new(
                    format!("treasury:rule:{}", proposal_id.0),
                    FailureType::TreasuryOperationFailed,
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "treasury_not_found",
                        "treasury_did": treasury_did.to_string(),
                    }),
                    format!("Treasury {treasury_did} not found"),
                );
                if let Err(dlq_err) = dlq.enqueue(failed_op) {
                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                }
                icn_obs::metrics::governance::execution_failures_inc("treasury_modify_rule");
                return;
            }

            // Convert approval type
            let ledger_approval_type = match approval_type {
                icn_governance::TreasuryApprovalType::None => ApprovalType::None,
                icn_governance::TreasuryApprovalType::SimpleMajority => {
                    ApprovalType::SimpleMajority
                }
                icn_governance::TreasuryApprovalType::SuperMajority => ApprovalType::SuperMajority,
                icn_governance::TreasuryApprovalType::BoardOnly => ApprovalType::BoardOnly,
                icn_governance::TreasuryApprovalType::Emergency => ApprovalType::Emergency,
            };

            let result = if let Some(ref existing_rule_id) = rule_id {
                // Update existing rule
                treasury_guard.update_spending_rule(
                    existing_rule_id,
                    Some(threshold_amount),
                    Some(ledger_approval_type),
                    Some(is_active),
                )
            } else {
                // Create new rule
                use icn_ledger::treasury::SpendingRule;

                let new_rule = SpendingRule::new(
                    treasury_did.clone(),
                    name.clone(),
                    threshold_amount,
                    currency.clone(),
                    ledger_approval_type,
                )
                .with_proposal(proposal_id.0.clone());

                treasury_guard.add_spending_rule(new_rule)
            };

            match result {
                Ok(()) => {
                    info!(
                        "✅ Treasury spending rule updated for proposal {}: {}",
                        proposal_id.0,
                        rule_id.as_deref().unwrap_or("new rule created")
                    );

                    let audit_record = serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "action": "modify_spending_rule",
                        "treasury_did": treasury_did.to_string(),
                        "rule_id": rule_id,
                        "name": name,
                        "threshold_amount": threshold_amount,
                        "executed_at": icn_time::current_timestamp_secs(),
                    });

                    if let Ok(audit_json) = serde_json::to_vec(&audit_record) {
                        if let Err(e) = store.put(audit_key.as_bytes(), &audit_json) {
                            error!(
                                "🚨 Failed to store audit trail for spending rule proposal {}: {}",
                                proposal_id.0, e
                            );
                            icn_obs::metrics::governance::audit_failures_inc();
                        }
                    }

                    let duration = start.elapsed().as_secs_f64();
                    icn_obs::metrics::governance::proposals_executed_inc("treasury_modify_rule");
                    icn_obs::metrics::governance::execution_duration_record(
                        "treasury_modify_rule",
                        duration,
                    );
                }
                Err(e) => {
                    error!(
                        "❌ Failed to modify spending rule for proposal {}: {}",
                        proposal_id.0, e
                    );

                    let failed_op = FailedOperation::new(
                        format!("treasury:rule:{}", proposal_id.0),
                        FailureType::TreasuryOperationFailed,
                        serde_json::json!({
                            "proposal_id": proposal_id.0,
                            "treasury_did": treasury_did.to_string(),
                            "rule_id": rule_id,
                            "threshold_amount": threshold_amount,
                        }),
                        e.to_string(),
                    );
                    if let Err(dlq_err) = dlq.enqueue(failed_op) {
                        error!("   Failed to write to dead-letter queue: {}", dlq_err);
                    }

                    icn_obs::metrics::governance::execution_failures_inc("treasury_modify_rule");
                }
            }
        });
    }

    /// Handle transfer between budgets
    fn handle_treasury_transfer_between_budgets(
        &self,
        proposal_id: ProposalId,
        treasury_did: Did,
        from_budget: String,
        to_budget: String,
        amount: i64,
        currency: String,
        reason: String,
    ) {
        info!(
            "📋 Treasury budget transfer for {}: {} {} from {} to {} ({})",
            treasury_did, amount, currency, from_budget, to_budget, reason
        );

        let treasury_manager = self.treasury_manager.clone();
        let store = self.audit_store.clone();
        let dlq = self.dlq.clone();

        tokio::spawn(async move {
            use icn_ledger::treasury::TreasuryOperation;

            let start = std::time::Instant::now();
            let audit_key = format!("gov:audit:treasury:transfer:{}", proposal_id.0);

            // Idempotency check
            match store.get(audit_key.as_bytes()) {
                Ok(Some(_)) => {
                    debug!(
                        "Treasury budget transfer proposal {} already executed",
                        proposal_id.0
                    );
                    icn_obs::metrics::governance::idempotent_skips_inc();
                    return;
                }
                Ok(None) => {}
                Err(e) => {
                    error!(
                        "🚨 Failed to check audit trail for budget transfer proposal {}: {}",
                        proposal_id.0, e
                    );
                    let failed_op =
                        FailedOperation::idempotency_check_failure(&proposal_id.0, &e.to_string());
                    if let Err(dlq_err) = dlq.enqueue(failed_op) {
                        error!("   Failed to write to dead-letter queue: {}", dlq_err);
                    }
                    return;
                }
            }

            // Validation: Amount must be positive
            if !validate_positive_amount(
                amount,
                &proposal_id,
                "transfer",
                "treasury_transfer_between_budgets",
                "Transfer amount",
                &dlq,
            ) {
                return;
            }

            // ATOMIC OPERATION: Acquire exclusive write lock on treasury_manager.
            // This lock is held for the ENTIRE operation (validation + mutation + persistence)
            // to prevent TOCTOU race conditions. The lock is only released when treasury_guard
            // goes out of scope at the end of this async block.
            let mut treasury_guard = treasury_manager.write().await;

            // Validate source budget exists, is active, and has sufficient funds (lock held)
            let (from_remaining, from_currency) = {
                if let Some(budget) = treasury_guard.get_budget(&from_budget) {
                    // Validate budget is active
                    if budget.status != icn_ledger::treasury::BudgetStatus::Active {
                        error!(
                            "❌ Source budget {} is not active (status: {:?}) for transfer proposal {}",
                            from_budget, budget.status, proposal_id.0
                        );
                        let failed_op = FailedOperation::new(
                            format!("treasury:transfer:{}", proposal_id.0),
                            FailureType::TreasuryOperationFailed,
                            serde_json::json!({
                                "proposal_id": proposal_id.0,
                                "error": "source_budget_not_active",
                                "from_budget": from_budget,
                                "status": format!("{:?}", budget.status),
                            }),
                            format!("Source budget {from_budget} is not active"),
                        );
                        if let Err(dlq_err) = dlq.enqueue(failed_op) {
                            error!("   Failed to write to dead-letter queue: {}", dlq_err);
                        }
                        icn_obs::metrics::governance::execution_failures_inc(
                            "treasury_transfer_between_budgets",
                        );
                        return;
                    }
                    (budget.remaining(), budget.currency.clone())
                } else {
                    error!(
                        "❌ Source budget {} not found for transfer proposal {}",
                        from_budget, proposal_id.0
                    );
                    let failed_op = FailedOperation::new(
                        format!("treasury:transfer:{}", proposal_id.0),
                        FailureType::TreasuryOperationFailed,
                        serde_json::json!({
                            "proposal_id": proposal_id.0,
                            "error": "source_budget_not_found",
                            "from_budget": from_budget,
                        }),
                        format!("Source budget {from_budget} not found"),
                    );
                    if let Err(dlq_err) = dlq.enqueue(failed_op) {
                        error!("   Failed to write to dead-letter queue: {}", dlq_err);
                    }
                    icn_obs::metrics::governance::execution_failures_inc(
                        "treasury_transfer_between_budgets",
                    );
                    return;
                }
            };

            if from_remaining < amount {
                error!(
                    "❌ Insufficient funds in source budget {} for transfer proposal {}: {} < {}",
                    from_budget, proposal_id.0, from_remaining, amount
                );
                let failed_op = FailedOperation::new(
                    format!("treasury:transfer:{}", proposal_id.0),
                    FailureType::TreasuryOperationFailed,
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "insufficient_funds",
                        "from_budget": from_budget,
                        "remaining": from_remaining,
                        "requested": amount,
                    }),
                    format!("Insufficient funds: {from_remaining} remaining, {amount} requested"),
                );
                if let Err(dlq_err) = dlq.enqueue(failed_op) {
                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                }
                icn_obs::metrics::governance::execution_failures_inc(
                    "treasury_transfer_between_budgets",
                );
                return;
            }

            // Validate destination budget exists, is active, and has matching currency
            if let Some(to_budget_data) = treasury_guard.get_budget(&to_budget) {
                // Validate budget is active
                if to_budget_data.status != icn_ledger::treasury::BudgetStatus::Active {
                    error!(
                        "❌ Destination budget {} is not active (status: {:?}) for transfer proposal {}",
                        to_budget, to_budget_data.status, proposal_id.0
                    );
                    let failed_op = FailedOperation::new(
                        format!("treasury:transfer:{}", proposal_id.0),
                        FailureType::TreasuryOperationFailed,
                        serde_json::json!({
                            "proposal_id": proposal_id.0,
                            "error": "destination_budget_not_active",
                            "to_budget": to_budget,
                            "status": format!("{:?}", to_budget_data.status),
                        }),
                        format!("Destination budget {to_budget} is not active"),
                    );
                    if let Err(dlq_err) = dlq.enqueue(failed_op) {
                        error!("   Failed to write to dead-letter queue: {}", dlq_err);
                    }
                    icn_obs::metrics::governance::execution_failures_inc(
                        "treasury_transfer_between_budgets",
                    );
                    return;
                }

                // Validate currencies match
                if !validate_currency_match(
                    &to_budget_data.currency,
                    &from_currency,
                    &proposal_id,
                    "transfer",
                    "treasury_transfer_between_budgets",
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "currency_mismatch",
                        "from_budget": from_budget,
                        "from_currency": from_currency,
                        "to_budget": to_budget,
                        "to_currency": to_budget_data.currency,
                    }),
                    &dlq,
                ) {
                    return;
                }
            } else {
                error!(
                    "❌ Destination budget {} not found for transfer proposal {}",
                    to_budget, proposal_id.0
                );
                let failed_op = FailedOperation::new(
                    format!("treasury:transfer:{}", proposal_id.0),
                    FailureType::TreasuryOperationFailed,
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "destination_budget_not_found",
                        "to_budget": to_budget,
                    }),
                    format!("Destination budget {to_budget} not found"),
                );
                if let Err(dlq_err) = dlq.enqueue(failed_op) {
                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                }
                icn_obs::metrics::governance::execution_failures_inc(
                    "treasury_transfer_between_budgets",
                );
                return;
            }

            // Perform atomic mutation with checked arithmetic (lock still held)
            // Pre-validate the addition won't overflow before mutating state
            let to_current = treasury_guard
                .get_budget(&to_budget)
                .map(|b| b.allocated_amount)
                .unwrap_or(0);
            if to_current.checked_add(amount).is_none() {
                error!(
                    "❌ Budget allocation overflow for transfer proposal {}: {} + {} exceeds i64::MAX",
                    proposal_id.0, to_current, amount
                );
                let failed_op = FailedOperation::new(
                    format!("treasury:transfer:{}", proposal_id.0),
                    FailureType::TreasuryOperationFailed,
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "allocation_overflow",
                        "to_budget": to_budget,
                        "current_allocation": to_current,
                        "transfer_amount": amount,
                    }),
                    format!("Budget allocation overflow: {to_current} + {amount} exceeds i64::MAX"),
                );
                if let Err(dlq_err) = dlq.enqueue(failed_op) {
                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                }
                icn_obs::metrics::governance::execution_failures_inc(
                    "treasury_transfer_between_budgets",
                );
                return;
            }

            // Two-Phase Commit: Persist BEFORE updating in-memory state
            // This prevents data corruption if persistence fails partway through.
            //
            // Phase 1: Clone budgets and apply changes to clones
            let mut from_budget_clone = match treasury_guard.get_budget(&from_budget) {
                Some(b) => b.clone(),
                None => {
                    error!(
                        "❌ Source budget {} disappeared during transfer",
                        from_budget
                    );
                    icn_obs::metrics::governance::execution_failures_inc(
                        "treasury_transfer_between_budgets",
                    );
                    return;
                }
            };
            let mut to_budget_clone = match treasury_guard.get_budget(&to_budget) {
                Some(b) => b.clone(),
                None => {
                    error!(
                        "❌ Destination budget {} disappeared during transfer",
                        to_budget
                    );
                    icn_obs::metrics::governance::execution_failures_inc(
                        "treasury_transfer_between_budgets",
                    );
                    return;
                }
            };

            // Apply changes to clones (not in-memory state yet)
            from_budget_clone.allocated_amount -= amount;
            to_budget_clone.allocated_amount += amount;

            // Phase 2: Persist both clones - if either fails, no state is corrupted
            // Note: We capture the ORIGINAL in memory (before persisting modified clones) for rollback
            let from_budget_original = treasury_guard.get_budget(&from_budget).cloned();

            if let Err(e) = treasury_guard.save_budget_snapshot(&from_budget_clone) {
                error!(
                    "🚨 Failed to persist from_budget {} for transfer: {}",
                    from_budget, e
                );
                let failed_op = FailedOperation::new(
                    format!("treasury:transfer:persist:{}", proposal_id.0),
                    FailureType::StorageFailure,
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "persistence_failed",
                        "budget_id": from_budget,
                        "operation": "save_from_budget",
                    }),
                    e.to_string(),
                );
                if let Err(dlq_err) = dlq.enqueue(failed_op) {
                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                }
                icn_obs::metrics::governance::execution_failures_inc(
                    "treasury_transfer_between_budgets",
                );
                return;
            }

            if let Err(e) = treasury_guard.save_budget_snapshot(&to_budget_clone) {
                error!(
                    "🚨 Failed to persist to_budget {} for transfer: {}",
                    to_budget, e
                );
                // Rollback from_budget to original state (not the modified clone)
                if let Some(original) = from_budget_original {
                    if let Err(rollback_err) = treasury_guard.save_budget_snapshot(&original) {
                        error!(
                            "🚨🚨 CRITICAL: Failed to rollback from_budget {} after partial \
                             transfer failure: {}. Manual intervention required.",
                            from_budget, rollback_err
                        );
                    }
                }
                let failed_op = FailedOperation::new(
                    format!("treasury:transfer:persist:{}", proposal_id.0),
                    FailureType::StorageFailure,
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "persistence_failed_with_rollback",
                        "budget_id": to_budget,
                        "operation": "save_to_budget",
                        "note": "from_budget rollback attempted",
                    }),
                    e.to_string(),
                );
                if let Err(dlq_err) = dlq.enqueue(failed_op) {
                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                }
                icn_obs::metrics::governance::execution_failures_inc(
                    "treasury_transfer_between_budgets",
                );
                return;
            }

            // Phase 3: Both persisted successfully - now update in-memory state
            // Use apply_budget_snapshot to ensure consistency
            if let Err(e) = treasury_guard.apply_budget_snapshot(&from_budget_clone) {
                error!(
                    "🚨 Failed to apply from_budget snapshot to in-memory state: {}. \
                     Storage is updated but memory is stale - restart may be needed.",
                    e
                );
            }
            if let Err(e) = treasury_guard.apply_budget_snapshot(&to_budget_clone) {
                error!(
                    "🚨 Failed to apply to_budget snapshot to in-memory state: {}. \
                     Storage is updated but memory is stale - restart may be needed.",
                    e
                );
            }

            // Record treasury audit trail
            let operation = TreasuryOperation::TransferBetweenBudgets {
                from_budget: from_budget.clone(),
                to_budget: to_budget.clone(),
                amount,
                currency: currency.clone(),
                reason: reason.clone(),
            };

            // Note: balance_after is 0 because treasury transfers don't affect the main treasury
            // balance directly - they only reallocate between budgets within the treasury.
            if let Err(e) = treasury_guard.record_audit(
                &treasury_did,
                operation,
                treasury_did.clone(),
                0, // balance_after: transfers don't change total treasury balance
                Some(proposal_id.0.clone()),
                None,
            ) {
                warn!(
                    "⚠️ Failed to record treasury audit for transfer proposal {}: {}",
                    proposal_id.0, e
                );
                // Audit failure is non-fatal - the transfer succeeded, just logging failed
            }

            info!(
                "✅ Treasury budget transfer completed for proposal {}: {} {} from {} to {}",
                proposal_id.0, amount, currency, from_budget, to_budget
            );

            let gov_audit = serde_json::json!({
                "proposal_id": proposal_id.0,
                "action": "transfer_between_budgets",
                "from_budget": from_budget,
                "to_budget": to_budget,
                "amount": amount,
                "currency": currency,
                "reason": reason,
                "executed_at": icn_time::current_timestamp_secs(),
            });

            if let Ok(audit_json) = serde_json::to_vec(&gov_audit) {
                if let Err(e) = store.put(audit_key.as_bytes(), &audit_json) {
                    error!(
                        "🚨 Failed to store audit trail for transfer proposal {}: {}",
                        proposal_id.0, e
                    );
                    icn_obs::metrics::governance::audit_failures_inc();
                }
            }

            let duration = start.elapsed().as_secs_f64();
            icn_obs::metrics::governance::proposals_executed_inc(
                "treasury_transfer_between_budgets",
            );
            icn_obs::metrics::governance::execution_duration_record(
                "treasury_transfer_between_budgets",
                duration,
            );
        });
    }

    /// Handle cancel budget
    fn handle_treasury_cancel_budget(
        &self,
        proposal_id: ProposalId,
        budget_id: String,
        reason: String,
        return_to_treasury: bool,
    ) {
        info!(
            "📋 Treasury budget cancelled: {} (reason: {}, return: {})",
            budget_id, reason, return_to_treasury
        );

        let treasury_manager = self.treasury_manager.clone();
        let store = self.audit_store.clone();
        let dlq = self.dlq.clone();

        tokio::spawn(async move {
            use icn_ledger::treasury::{BudgetStatus, TreasuryOperation};

            let start = std::time::Instant::now();
            let audit_key = format!("gov:audit:treasury:cancel:{}", proposal_id.0);

            // Idempotency check
            match store.get(audit_key.as_bytes()) {
                Ok(Some(_)) => {
                    debug!(
                        "Treasury cancel budget proposal {} already executed",
                        proposal_id.0
                    );
                    icn_obs::metrics::governance::idempotent_skips_inc();
                    return;
                }
                Ok(None) => {}
                Err(e) => {
                    error!(
                        "🚨 Failed to check audit trail for cancel budget proposal {}: {}",
                        proposal_id.0, e
                    );
                    let failed_op =
                        FailedOperation::idempotency_check_failure(&proposal_id.0, &e.to_string());
                    if let Err(dlq_err) = dlq.enqueue(failed_op) {
                        error!("   Failed to write to dead-letter queue: {}", dlq_err);
                    }
                    return;
                }
            }

            // ATOMIC OPERATION: Acquire exclusive write lock on treasury_manager.
            // This lock is held for the ENTIRE operation (validation + mutation + persistence)
            // to prevent TOCTOU race conditions.
            let mut treasury_guard = treasury_manager.write().await;

            // Validate budget exists and get info (lock held)
            let (treasury_did, remaining_amount, currency) = {
                if let Some(budget) = treasury_guard.get_budget(&budget_id) {
                    (
                        budget.treasury_did.clone(),
                        budget.remaining(),
                        budget.currency.clone(),
                    )
                } else {
                    error!(
                        "❌ Budget {} not found for cancel proposal {}",
                        budget_id, proposal_id.0
                    );
                    let failed_op = FailedOperation::new(
                        format!("treasury:cancel:{}", proposal_id.0),
                        FailureType::TreasuryOperationFailed,
                        serde_json::json!({
                            "proposal_id": proposal_id.0,
                            "error": "budget_not_found",
                            "budget_id": budget_id,
                        }),
                        format!("Budget {budget_id} not found"),
                    );
                    if let Err(dlq_err) = dlq.enqueue(failed_op) {
                        error!("   Failed to write to dead-letter queue: {}", dlq_err);
                    }
                    icn_obs::metrics::governance::execution_failures_inc("treasury_cancel_budget");
                    return;
                }
            };

            // If return_to_treasury is true, reclaim remaining funds
            let reclaimed_amount = if return_to_treasury && remaining_amount > 0 {
                // Reduce allocated amount to spent amount (reclaim remaining)
                if let Some(budget) = treasury_guard.get_budget_mut(&budget_id) {
                    budget.allocated_amount -= remaining_amount;
                }

                // CRITICAL: Persist allocation change BEFORE status update to ensure
                // reclaimed funds are recorded even if status update fails
                if let Err(e) = treasury_guard.save_budget(&budget_id) {
                    error!(
                        "🚨 Failed to persist budget allocation change for cancel proposal {}: {}",
                        proposal_id.0, e
                    );
                    // Rollback in-memory state
                    if let Some(budget) = treasury_guard.get_budget_mut(&budget_id) {
                        budget.allocated_amount += remaining_amount;
                    }
                    let failed_op = FailedOperation::new(
                        format!("treasury:cancel:{}", proposal_id.0),
                        FailureType::TreasuryOperationFailed,
                        serde_json::json!({
                            "proposal_id": proposal_id.0,
                            "error": "allocation_persist_failed",
                            "budget_id": budget_id,
                            "reclaimed_amount": remaining_amount,
                        }),
                        format!("Failed to persist allocation change: {e}"),
                    );
                    if let Err(dlq_err) = dlq.enqueue(failed_op) {
                        error!("   Failed to write to dead-letter queue: {}", dlq_err);
                    }
                    icn_obs::metrics::governance::execution_failures_inc("treasury_cancel_budget");
                    return;
                }
                remaining_amount
            } else {
                0
            };

            // Cancel the budget (status update)
            match treasury_guard.update_budget_status(&budget_id, BudgetStatus::Cancelled) {
                Ok(()) => {
                    info!(
                        "✅ Treasury budget {} cancelled for proposal {}{}",
                        budget_id,
                        proposal_id.0,
                        if reclaimed_amount > 0 {
                            format!(", reclaimed {reclaimed_amount} {currency}")
                        } else {
                            String::new()
                        }
                    );

                    // Record treasury audit for the cancellation
                    let operation = TreasuryOperation::CancelBudget {
                        budget_id: budget_id.clone(),
                        reason: reason.clone(),
                        return_to_treasury,
                    };

                    // Note: balance_after is 0 because cancellation only affects budget allocation,
                    // not the main treasury balance. Reclaimed funds return to unallocated pool.
                    if let Err(e) = treasury_guard.record_audit(
                        &treasury_did,
                        operation,
                        treasury_did.clone(),
                        0, // balance_after: cancellation doesn't change treasury balance
                        Some(proposal_id.0.clone()),
                        None,
                    ) {
                        warn!(
                            "⚠️ Failed to record treasury audit for cancel proposal {}: {}",
                            proposal_id.0, e
                        );
                        // Audit failure is non-fatal - the cancellation succeeded
                    }

                    let audit_record = serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "action": "cancel_budget",
                        "budget_id": budget_id,
                        "reason": reason,
                        "return_to_treasury": return_to_treasury,
                        "reclaimed_amount": reclaimed_amount,
                        "executed_at": icn_time::current_timestamp_secs(),
                    });

                    if let Ok(audit_json) = serde_json::to_vec(&audit_record) {
                        if let Err(e) = store.put(audit_key.as_bytes(), &audit_json) {
                            error!(
                                "🚨 Failed to store audit trail for cancel budget proposal {}: {}",
                                proposal_id.0, e
                            );
                            icn_obs::metrics::governance::audit_failures_inc();
                        }
                    }

                    let duration = start.elapsed().as_secs_f64();
                    icn_obs::metrics::governance::proposals_executed_inc("treasury_cancel_budget");
                    icn_obs::metrics::governance::execution_duration_record(
                        "treasury_cancel_budget",
                        duration,
                    );
                }
                Err(e) => {
                    error!(
                        "❌ Failed to cancel budget {} for proposal {}: {}",
                        budget_id, proposal_id.0, e
                    );
                    let failed_op = FailedOperation::new(
                        format!("treasury:cancel:{}", proposal_id.0),
                        FailureType::TreasuryOperationFailed,
                        serde_json::json!({
                            "proposal_id": proposal_id.0,
                            "budget_id": budget_id,
                            "reason": reason,
                        }),
                        e.to_string(),
                    );
                    if let Err(dlq_err) = dlq.enqueue(failed_op) {
                        error!("   Failed to write to dead-letter queue: {}", dlq_err);
                    }
                    icn_obs::metrics::governance::execution_failures_inc("treasury_cancel_budget");
                }
            }
        });
    }

    /// Handle reclaim budget funds
    fn handle_treasury_reclaim_budget(
        &self,
        proposal_id: ProposalId,
        budget_id: String,
        amount: i64,
        currency: String,
        reason: String,
    ) {
        info!(
            "📋 Treasury budget reclaim: {} {} from {} ({})",
            amount, currency, budget_id, reason
        );

        let treasury_manager = self.treasury_manager.clone();
        let store = self.audit_store.clone();
        let dlq = self.dlq.clone();

        tokio::spawn(async move {
            use icn_ledger::treasury::TreasuryOperation;

            let start = std::time::Instant::now();
            let audit_key = format!("gov:audit:treasury:reclaim:{}", proposal_id.0);

            // Idempotency check
            match store.get(audit_key.as_bytes()) {
                Ok(Some(_)) => {
                    debug!(
                        "Treasury reclaim budget proposal {} already executed",
                        proposal_id.0
                    );
                    icn_obs::metrics::governance::idempotent_skips_inc();
                    return;
                }
                Ok(None) => {}
                Err(e) => {
                    error!(
                        "🚨 Failed to check audit trail for reclaim budget proposal {}: {}",
                        proposal_id.0, e
                    );
                    let failed_op =
                        FailedOperation::idempotency_check_failure(&proposal_id.0, &e.to_string());
                    if let Err(dlq_err) = dlq.enqueue(failed_op) {
                        error!("   Failed to write to dead-letter queue: {}", dlq_err);
                    }
                    return;
                }
            }

            // Validation: Amount must be positive (no lock needed)
            if !validate_positive_amount(
                amount,
                &proposal_id,
                "reclaim",
                "treasury_reclaim",
                "Reclaim amount",
                &dlq,
            ) {
                return;
            }

            // ATOMIC OPERATION: Acquire exclusive write lock on treasury_manager.
            // This lock is held for the ENTIRE operation (validation + mutation + persistence)
            // to prevent TOCTOU race conditions.
            let mut treasury_guard = treasury_manager.write().await;

            // Validate budget exists, has matching currency, and sufficient funds (lock held)
            let (treasury_did, remaining) = {
                if let Some(budget) = treasury_guard.get_budget(&budget_id) {
                    // Validate currency matches
                    if !validate_currency_match(
                        &budget.currency,
                        &currency,
                        &proposal_id,
                        "reclaim",
                        "treasury_reclaim",
                        serde_json::json!({
                            "proposal_id": proposal_id.0,
                            "error": "currency_mismatch",
                            "budget_id": budget_id,
                            "budget_currency": budget.currency,
                            "requested_currency": currency,
                        }),
                        &dlq,
                    ) {
                        return;
                    }
                    (budget.treasury_did.clone(), budget.remaining())
                } else {
                    error!(
                        "❌ Budget {} not found for reclaim proposal {}",
                        budget_id, proposal_id.0
                    );
                    let failed_op = FailedOperation::new(
                        format!("treasury:reclaim:{}", proposal_id.0),
                        FailureType::TreasuryOperationFailed,
                        serde_json::json!({
                            "proposal_id": proposal_id.0,
                            "error": "budget_not_found",
                            "budget_id": budget_id,
                        }),
                        format!("Budget {budget_id} not found"),
                    );
                    if let Err(dlq_err) = dlq.enqueue(failed_op) {
                        error!("   Failed to write to dead-letter queue: {}", dlq_err);
                    }
                    icn_obs::metrics::governance::execution_failures_inc("treasury_reclaim");
                    return;
                }
            };

            if remaining < amount {
                error!(
                    "❌ Insufficient funds to reclaim from budget {} for proposal {}: {} < {}",
                    budget_id, proposal_id.0, remaining, amount
                );
                let failed_op = FailedOperation::new(
                    format!("treasury:reclaim:{}", proposal_id.0),
                    FailureType::TreasuryOperationFailed,
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "insufficient_funds",
                        "budget_id": budget_id,
                        "remaining": remaining,
                        "requested": amount,
                    }),
                    format!("Insufficient funds: {remaining} remaining, {amount} requested"),
                );
                if let Err(dlq_err) = dlq.enqueue(failed_op) {
                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                }
                icn_obs::metrics::governance::execution_failures_inc("treasury_reclaim");
                return;
            }

            // Perform atomic mutation (lock still held)
            if let Some(budget) = treasury_guard.get_budget_mut(&budget_id) {
                budget.allocated_amount -= amount;
                info!(
                    "✅ Reclaimed {} {} from budget {} for proposal {}",
                    amount, currency, budget_id, proposal_id.0
                );
            }

            // Persist budget changes - CRITICAL: failures here mean in-memory state diverges
            // from persistent state. We treat this as a failure and enqueue to DLQ for retry.
            if let Err(e) = treasury_guard.save_budget(&budget_id) {
                error!(
                    "🚨 Failed to persist budget {} after reclaim: {}",
                    budget_id, e
                );
                let failed_op = FailedOperation::new(
                    format!("treasury:reclaim:persist:{}", proposal_id.0),
                    FailureType::StorageFailure,
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "persistence_failed",
                        "budget_id": budget_id,
                        "amount": amount,
                    }),
                    e.to_string(),
                );
                if let Err(dlq_err) = dlq.enqueue(failed_op) {
                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                }
                icn_obs::metrics::governance::execution_failures_inc("treasury_reclaim");
                return;
            }

            // Record treasury audit trail
            let operation = TreasuryOperation::ReclaimBudget {
                budget_id: budget_id.clone(),
                amount,
                currency: currency.clone(),
                reason: reason.clone(),
            };

            // Note: balance_after is 0 because reclaim only affects budget allocation,
            // not the main treasury balance. Reclaimed funds return to unallocated pool.
            if let Err(e) = treasury_guard.record_audit(
                &treasury_did,
                operation,
                treasury_did.clone(),
                0, // balance_after: reclaim doesn't change treasury balance
                Some(proposal_id.0.clone()),
                None,
            ) {
                warn!(
                    "⚠️ Failed to record treasury audit for reclaim proposal {}: {}",
                    proposal_id.0, e
                );
                // Audit failure is non-fatal - the reclaim succeeded
            }

            let gov_audit = serde_json::json!({
                "proposal_id": proposal_id.0,
                "action": "reclaim_budget",
                "budget_id": budget_id,
                "amount": amount,
                "currency": currency,
                "reason": reason,
                "executed_at": icn_time::current_timestamp_secs(),
            });

            if let Ok(audit_json) = serde_json::to_vec(&gov_audit) {
                if let Err(e) = store.put(audit_key.as_bytes(), &audit_json) {
                    error!(
                        "🚨 Failed to store audit trail for reclaim proposal {}: {}",
                        proposal_id.0, e
                    );
                    icn_obs::metrics::governance::audit_failures_inc();
                }
            }

            let duration = start.elapsed().as_secs_f64();
            icn_obs::metrics::governance::proposals_executed_inc("treasury_reclaim");
            icn_obs::metrics::governance::execution_duration_record("treasury_reclaim", duration);
        });
    }

    /// Handle a direct treasury spend
    ///
    /// Validates amount, recipient, memo, and treasury nonce before resolving
    /// the treasury currency and performing the actual ledger transfer.
    ///
    /// The nonce is checked atomically via `CoopStore::check_and_increment_treasury_nonce`
    /// before the ledger transfer to prevent double-spend from concurrent
    /// proposal execution.
    fn handle_treasury_spend(
        &self,
        proposal_id: ProposalId,
        amount: i64,
        recipient: Did,
        memo: String,
        nonce: u64,
        decided_at: u64,
        domain_id: String,
    ) {
        info!(
            "🏦 Executing treasury spend for proposal {}: {} to {} ({})",
            proposal_id.0, amount, recipient, memo
        );

        let dlq = self.dlq.clone();

        // --- Field validation (before any IO) ---

        // Validate amount is positive
        if !validate_positive_amount(
            amount,
            &proposal_id,
            "spend",
            "treasury_spend",
            "Amount",
            &dlq,
        ) {
            return;
        }

        // Validate recipient is non-empty
        if recipient.to_string().is_empty() {
            error!(
                "❌ Empty recipient for treasury spend proposal {}",
                proposal_id.0
            );
            let failed_op = FailedOperation::new(
                format!("treasury:spend:{}", proposal_id.0),
                FailureType::TreasuryOperationFailed,
                serde_json::json!({
                    "proposal_id": proposal_id.0,
                    "error": "empty_recipient",
                }),
                "Recipient DID must not be empty".to_string(),
            );
            if let Err(dlq_err) = dlq.enqueue(failed_op) {
                error!("   Failed to write to dead-letter queue: {}", dlq_err);
            }
            icn_obs::metrics::governance::execution_failures_inc("treasury_spend");
            return;
        }

        // Validate memo is non-empty
        if memo.is_empty() {
            error!(
                "❌ Empty memo for treasury spend proposal {}",
                proposal_id.0
            );
            let failed_op = FailedOperation::new(
                format!("treasury:spend:{}", proposal_id.0),
                FailureType::TreasuryOperationFailed,
                serde_json::json!({
                    "proposal_id": proposal_id.0,
                    "error": "empty_memo",
                }),
                "Memo must not be empty".to_string(),
            );
            if let Err(dlq_err) = dlq.enqueue(failed_op) {
                error!("   Failed to write to dead-letter queue: {}", dlq_err);
            }
            icn_obs::metrics::governance::execution_failures_inc("treasury_spend");
            return;
        }

        // Resolve the treasury currency and record audit trail, then delegate
        // the actual ledger transfer to handle_budget_proposal.
        let treasury_manager = self.treasury_manager.clone();
        let treasury_did = self.treasury_did.clone();
        let ledger = self.ledger.clone();
        let store = self.audit_store.clone();
        let dlq_clone = dlq.clone();
        let proposal_id_clone = proposal_id.clone();
        let recipient_clone = recipient.clone();
        let memo_clone = memo.clone();
        let coop_store = self.coop_store.clone();
        let gov_handle = self.gov_handle.clone();
        let event_bus = self.event_bus.clone();
        let domain_id_clone = domain_id.clone();

        tokio::spawn(async move {
            use icn_ledger::treasury::TreasuryOperation;
            use std::time::Instant;

            let start = Instant::now();
            let audit_key = format!("gov:audit:treasury:spend:{}", proposal_id_clone.0);

            // IDEMPOTENCY CHECK (must run before proof/nonce/audit side effects)
            match store.get(audit_key.as_bytes()) {
                Ok(Some(_)) => {
                    debug!(
                        "Treasury spend proposal {} already executed, skipping duplicate event",
                        proposal_id_clone.0
                    );
                    icn_obs::metrics::governance::idempotent_skips_inc();
                    return;
                }
                Ok(None) => {}
                Err(e) => {
                    error!(
                        "🚨 Failed to check audit trail for spend proposal {}: {}",
                        proposal_id_clone.0, e
                    );
                    let failed_op = FailedOperation::idempotency_check_failure(
                        &proposal_id_clone.0,
                        &e.to_string(),
                    );
                    if let Err(dlq_err) = dlq_clone.enqueue(failed_op) {
                        error!("   Failed to write to dead-letter queue: {}", dlq_err);
                    }
                    icn_obs::metrics::governance::execution_failures_inc("treasury_spend");
                    return;
                }
            }

            // --- Governance proof gate (fail-closed) ---
            let proof = match gov_handle.get_proof(&proposal_id_clone).await {
                Ok(Some(proof)) => proof,
                Ok(None) => {
                    let reason = format!(
                        "Missing GovernanceProof for treasury spend proposal {}",
                        proposal_id_clone.0
                    );
                    error!("❌ {}", reason);

                    let failed_op = FailedOperation::new(
                        format!("treasury:spend:proof:{}", proposal_id_clone.0),
                        FailureType::GovernanceExecutionFailed,
                        serde_json::json!({
                            "proposal_id": proposal_id_clone.0,
                            "domain_id": domain_id_clone.clone(),
                            "error": "missing_proof",
                        }),
                        reason.clone(),
                    );
                    if let Err(dlq_err) = dlq_clone.enqueue(failed_op) {
                        error!("   Failed to write to dead-letter queue: {}", dlq_err);
                    }
                    icn_obs::metrics::governance::execution_failures_inc("treasury_spend");
                    if let Some(bus) = event_bus.as_ref() {
                        bus.emit(crate::events::SystemEvent::ProposalExecutionFailed {
                            proposal_id: proposal_id_clone.0.clone(),
                            proposal_type: "treasury_spend".to_string(),
                            error: reason,
                            failed_at: icn_time::current_timestamp_secs(),
                        })
                        .await;
                    }
                    return;
                }
                Err(e) => {
                    let reason = format!(
                        "Failed to fetch GovernanceProof for treasury spend proposal {}: {}",
                        proposal_id_clone.0, e
                    );
                    error!("❌ {}", reason);

                    let failed_op = FailedOperation::new(
                        format!("treasury:spend:proof:{}", proposal_id_clone.0),
                        FailureType::GovernanceExecutionFailed,
                        serde_json::json!({
                            "proposal_id": proposal_id_clone.0,
                            "domain_id": domain_id_clone.clone(),
                            "error": "proof_fetch_failed",
                        }),
                        reason.clone(),
                    );
                    if let Err(dlq_err) = dlq_clone.enqueue(failed_op) {
                        error!("   Failed to write to dead-letter queue: {}", dlq_err);
                    }
                    icn_obs::metrics::governance::execution_failures_inc("treasury_spend");
                    if let Some(bus) = event_bus.as_ref() {
                        bus.emit(crate::events::SystemEvent::ProposalExecutionFailed {
                            proposal_id: proposal_id_clone.0.clone(),
                            proposal_type: "treasury_spend".to_string(),
                            error: reason,
                            failed_at: icn_time::current_timestamp_secs(),
                        })
                        .await;
                    }
                    return;
                }
            };

            if let Err(reason) = validate_treasury_spend_proof(
                &proof,
                &proposal_id_clone,
                &domain_id_clone,
                decided_at,
            ) {
                error!("❌ {}", reason);
                let failed_op = FailedOperation::new(
                    format!("treasury:spend:proof:{}", proposal_id_clone.0),
                    FailureType::GovernanceExecutionFailed,
                    serde_json::json!({
                        "proposal_id": proposal_id_clone.0,
                        "domain_id": domain_id_clone.clone(),
                        "error": "proof_validation_failed",
                    }),
                    reason.clone(),
                );
                if let Err(dlq_err) = dlq_clone.enqueue(failed_op) {
                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                }
                icn_obs::metrics::governance::execution_failures_inc("treasury_spend");
                if let Some(bus) = event_bus.as_ref() {
                    bus.emit(crate::events::SystemEvent::ProposalExecutionFailed {
                        proposal_id: proposal_id_clone.0.clone(),
                        proposal_type: "treasury_spend".to_string(),
                        error: reason,
                        failed_at: icn_time::current_timestamp_secs(),
                    })
                    .await;
                }
                return;
            }

            // --- Treasury nonce check (double-spend guard) ---
            //
            // The nonce is checked and incremented atomically in a sled
            // transaction BEFORE the ledger transfer.  If the nonce does not
            // match the expected value, the spend is rejected.
            if let Some(ref cs) = coop_store {
                let treasury_id = treasury_did.to_string();
                if let Err(e) = cs.check_and_increment_treasury_nonce(&treasury_id, nonce) {
                    error!(
                        "❌ Treasury nonce check failed for spend proposal {}: {}",
                        proposal_id_clone.0, e
                    );
                    let failed_op = FailedOperation::new(
                        format!("treasury:spend:nonce:{}", proposal_id_clone.0),
                        FailureType::TreasuryOperationFailed,
                        serde_json::json!({
                            "proposal_id": proposal_id_clone.0,
                            "error": "nonce_mismatch",
                            "expected_nonce": nonce,
                            "treasury_did": treasury_id,
                        }),
                        e.to_string(),
                    );
                    if let Err(dlq_err) = dlq_clone.enqueue(failed_op) {
                        error!("   Failed to write to dead-letter queue: {}", dlq_err);
                    }
                    icn_obs::metrics::governance::execution_failures_inc("treasury_spend");
                    return;
                }
                debug!(
                    "Treasury nonce {} accepted for spend proposal {}",
                    nonce, proposal_id_clone.0
                );
            } else {
                warn!(
                    "⚠️ No CoopStore configured; skipping nonce check for spend proposal {}",
                    proposal_id_clone.0
                );
            }

            // Resolve currency from the treasury configuration
            let currency = {
                let treasury_guard = treasury_manager.read().await;
                match treasury_guard.get_treasury(&treasury_did) {
                    Some(treasury) => treasury.currency.clone(),
                    None => {
                        error!(
                            "❌ Treasury {} not found for spend proposal {}",
                            treasury_did, proposal_id_clone.0
                        );
                        let failed_op = FailedOperation::new(
                            format!("treasury:spend:{}", proposal_id_clone.0),
                            FailureType::TreasuryOperationFailed,
                            serde_json::json!({
                                "proposal_id": proposal_id_clone.0,
                                "error": "treasury_not_found",
                                "treasury_did": treasury_did.to_string(),
                            }),
                            format!("Treasury {treasury_did} not found"),
                        );
                        if let Err(dlq_err) = dlq_clone.enqueue(failed_op) {
                            error!("   Failed to write to dead-letter queue: {}", dlq_err);
                        }
                        icn_obs::metrics::governance::execution_failures_inc("treasury_spend");
                        return;
                    }
                }
            };

            // Record treasury audit trail
            {
                let mut treasury_guard = treasury_manager.write().await;
                let operation = TreasuryOperation::Withdraw {
                    to: recipient_clone.clone(),
                    amount,
                    currency: currency.clone(),
                    purpose: memo_clone,
                    budget_id: None,
                };

                if let Err(e) = treasury_guard.record_audit(
                    &treasury_did,
                    operation,
                    treasury_did.clone(),
                    0,
                    Some(proposal_id_clone.0.clone()),
                    None,
                ) {
                    warn!(
                        "⚠️ Failed to record treasury audit for spend proposal {}: {}",
                        proposal_id_clone.0, e
                    );
                    // Audit failure is non-fatal; the spend proceeds
                }
            }

            // Perform the actual ledger transfer (inline from handle_budget_proposal logic)
            use icn_ledger::entry::JournalEntryBuilder;

            let mut ledger_guard = ledger.write().await;

            let entry_result = JournalEntryBuilder::new(treasury_did.clone())
                .credit(treasury_did.clone(), currency.clone(), amount)
                .debit(recipient_clone.clone(), currency.clone(), amount)
                .build();

            match entry_result {
                Ok(entry) => match ledger_guard.append_entry(entry).await {
                    Ok(entry_hash) => {
                        info!(
                            "✅ Treasury spend {} executed: {} {} transferred to {}",
                            proposal_id_clone.0, amount, currency, recipient_clone
                        );

                        let audit_record = serde_json::json!({
                            "proposal_id": proposal_id_clone.0,
                            "action": "treasury_spend",
                            "ledger_entry_hash": hex::encode(entry_hash.0),
                            "amount": amount,
                            "currency": currency,
                            "recipient": recipient_clone.to_string(),
                            "decided_at": decided_at,
                            "executed_at": icn_time::current_timestamp_secs(),
                        });

                        if let Ok(audit_json) = serde_json::to_vec(&audit_record) {
                            if let Err(e) = store.put(audit_key.as_bytes(), &audit_json) {
                                error!(
                                    "🚨 Failed to store audit trail for spend proposal {}: {}",
                                    proposal_id_clone.0, e
                                );
                                icn_obs::metrics::governance::audit_failures_inc();
                            }
                        }

                        let duration = start.elapsed().as_secs_f64();
                        icn_obs::metrics::governance::proposals_executed_inc("treasury_spend");
                        icn_obs::metrics::governance::execution_duration_record(
                            "treasury_spend",
                            duration,
                        );
                    }
                    Err(e) => {
                        error!(
                            "🚨 Failed to append ledger entry for spend proposal {}: {}",
                            proposal_id_clone.0, e
                        );
                        let failed_op = FailedOperation::new(
                            format!("treasury:spend:ledger:{}", proposal_id_clone.0),
                            FailureType::LedgerAppendFailed,
                            serde_json::json!({
                                "proposal_id": proposal_id_clone.0,
                                "amount": amount,
                                "currency": currency,
                                "recipient": recipient_clone.to_string(),
                            }),
                            e.to_string(),
                        );
                        if let Err(dlq_err) = dlq_clone.enqueue(failed_op) {
                            error!("   Failed to write to dead-letter queue: {}", dlq_err);
                        }
                        icn_obs::metrics::governance::execution_failures_inc("treasury_spend");
                    }
                },
                Err(e) => {
                    warn!(
                        "❌ Failed to build ledger entry for spend proposal {}: {}",
                        proposal_id_clone.0, e
                    );
                    icn_obs::metrics::governance::execution_failures_inc("treasury_spend");
                }
            }
        });
    }

    /// Handle a budget proposal
    pub(super) fn handle_budget_proposal(
        &self,
        proposal_id: ProposalId,
        amount: i64,
        recipient: Did,
        currency: String,
        decided_at: u64,
    ) {
        info!(
            "📊 Executing budget proposal {}: {} {} to {}",
            proposal_id.0, amount, currency, recipient
        );

        let ledger = self.ledger.clone();
        let from_did = self.treasury_did.clone();
        let store = self.audit_store.clone();
        let dlq = self.dlq.clone();

        tokio::spawn(async move {
            use icn_ledger::entry::JournalEntryBuilder;

            let start = std::time::Instant::now();

            // IDEMPOTENCY CHECK
            let audit_key = format!("gov:audit:{}", proposal_id.0);
            match store.get(audit_key.as_bytes()) {
                Ok(Some(_)) => {
                    debug!(
                        "Proposal {} already executed, skipping duplicate event",
                        proposal_id.0
                    );
                    icn_obs::metrics::governance::idempotent_skips_inc();
                    return;
                }
                Ok(None) => {}
                Err(e) => {
                    error!(
                        "🚨 CRITICAL: Failed to check audit trail for proposal {}: {}",
                        proposal_id.0, e
                    );
                    error!("   Refusing to execute to prevent potential duplicate");

                    let failed_op =
                        FailedOperation::idempotency_check_failure(&proposal_id.0, &e.to_string());
                    if let Err(dlq_err) = dlq.enqueue(failed_op) {
                        error!("   Failed to write to dead-letter queue: {}", dlq_err);
                    }

                    icn_obs::metrics::governance::execution_failures_inc("audit_check_failed");
                    return;
                }
            }

            let mut ledger_guard = ledger.write().await;

            let entry_result = JournalEntryBuilder::new(from_did.clone())
                .credit(from_did.clone(), currency.clone(), amount)
                .debit(recipient.clone(), currency.clone(), amount)
                .build();

            match entry_result {
                Ok(entry) => match ledger_guard.append_entry(entry).await {
                    Ok(entry_hash) => {
                        info!(
                            "✅ Budget proposal {} executed: {} {} transferred to {}",
                            proposal_id.0, amount, currency, recipient
                        );

                        let audit_record = serde_json::json!({
                            "proposal_id": proposal_id.0,
                            "ledger_entry_hash": hex::encode(entry_hash.0),
                            "amount": amount,
                            "currency": currency,
                            "recipient": recipient.to_string(),
                            "decided_at": decided_at,
                            "executed_at": icn_time::current_timestamp_secs(),
                        });

                        match serde_json::to_vec(&audit_record) {
                            Ok(audit_json) => match store.put(audit_key.as_bytes(), &audit_json) {
                                Ok(_) => {
                                    info!("📋 Audit trail recorded for proposal {}", proposal_id.0);
                                    let duration = start.elapsed().as_secs_f64();
                                    icn_obs::metrics::governance::proposals_executed_inc("budget");
                                    icn_obs::metrics::governance::execution_duration_record(
                                        "budget", duration,
                                    );
                                }
                                Err(e) => {
                                    error!(
                                        "🚨 CRITICAL: Ledger updated but audit trail write failed for proposal {}",
                                        proposal_id.0
                                    );
                                    error!("   Error: {}", e);

                                    let failed_op = FailedOperation::audit_trail_failure(
                                        &proposal_id.0,
                                        &hex::encode(entry_hash.0),
                                        amount,
                                        &currency,
                                        &recipient.to_string(),
                                        &e.to_string(),
                                    );
                                    if let Err(dlq_err) = dlq.enqueue(failed_op) {
                                        error!(
                                            "   Failed to write to dead-letter queue: {}",
                                            dlq_err
                                        );
                                    }

                                    icn_obs::metrics::governance::audit_failures_inc();
                                }
                            },
                            Err(e) => {
                                error!(
                                    "🚨 CRITICAL: Failed to serialize audit record for proposal {}: {}",
                                    proposal_id.0, e
                                );

                                let failed_op = FailedOperation::new(
                                    format!("serialize:{}", proposal_id.0),
                                    FailureType::AuditTrailSerialize,
                                    serde_json::json!({
                                        "proposal_id": proposal_id.0,
                                        "ledger_hash": hex::encode(entry_hash.0),
                                        "amount": amount,
                                        "currency": currency,
                                        "recipient": recipient.to_string(),
                                    }),
                                    e.to_string(),
                                );
                                if let Err(dlq_err) = dlq.enqueue(failed_op) {
                                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                                }

                                icn_obs::metrics::governance::audit_failures_inc();
                            }
                        }
                    }
                    Err(e) => {
                        error!(
                            "🚨 CRITICAL: Failed to append ledger entry for proposal {}: {}",
                            proposal_id.0, e
                        );

                        let failed_op = FailedOperation::new(
                            format!("ledger:{}", proposal_id.0),
                            FailureType::LedgerAppendFailed,
                            serde_json::json!({
                                "proposal_id": proposal_id.0,
                                "amount": amount,
                                "currency": currency,
                                "recipient": recipient.to_string(),
                                "from_did": from_did.to_string(),
                            }),
                            e.to_string(),
                        );
                        if let Err(dlq_err) = dlq.enqueue(failed_op) {
                            error!("   Failed to write to dead-letter queue: {}", dlq_err);
                        }

                        icn_obs::metrics::governance::execution_failures_inc("ledger_append");
                    }
                },
                Err(e) => {
                    warn!(
                        "❌ Failed to build ledger entry for proposal {}: {}",
                        proposal_id.0, e
                    );
                    icn_obs::metrics::governance::execution_failures_inc("ledger_build");
                }
            }
        });
    }

    /// Execute surplus allocation to shareholders
    pub(super) fn execute_surplus_allocation(
        &self,
        proposal_id: ProposalId,
        allocation: icn_ledger::SurplusAllocation,
    ) {
        let treasury_manager = self.treasury_manager.clone();
        let store = self.audit_store.clone();
        let dlq = self.dlq.clone();

        tokio::spawn(async move {
            let start = std::time::Instant::now();

            // Idempotency check
            let audit_key = format!("gov:audit:surplus_allocation:{}", proposal_id.0);
            match store.get(audit_key.as_bytes()) {
                Ok(Some(_)) => {
                    debug!(
                        "Surplus allocation {} already executed, skipping",
                        proposal_id.0
                    );
                    icn_obs::metrics::governance::idempotent_skips_inc();
                    return;
                }
                Ok(None) => {}
                Err(e) => {
                    error!(
                        "🚨 Failed to check audit trail for surplus allocation {}: {}",
                        proposal_id.0, e
                    );
                }
            }

            let mut treasury_guard = treasury_manager.write().await;

            match treasury_guard.execute_surplus_allocation(allocation.clone()) {
                Ok(()) => {
                    info!(
                        "✅ Surplus allocation {} executed: {} {} to {} shareholders",
                        proposal_id.0,
                        allocation.total_surplus,
                        allocation.currency,
                        allocation.allocations.len()
                    );

                    // Record audit trail
                    if let Err(e) = store.put(
                        audit_key.as_bytes(),
                        serde_json::to_vec(&serde_json::json!({
                            "executed_at": icn_time::current_timestamp_secs(),
                            "total_surplus": allocation.total_surplus,
                            "currency": allocation.currency,
                            "shareholder_count": allocation.allocations.len(),
                        }))
                        .unwrap_or_default()
                        .as_slice(),
                    ) {
                        warn!("Failed to record audit trail for surplus allocation: {}", e);
                    }

                    icn_obs::metrics::governance::proposals_executed_inc("surplus_allocation");
                    icn_obs::metrics::governance::execution_duration_record(
                        "surplus_allocation",
                        start.elapsed().as_secs_f64(),
                    );
                }
                Err(e) => {
                    error!(
                        "❌ Failed to execute surplus allocation {}: {}",
                        proposal_id.0, e
                    );
                    let failed_op = FailedOperation::new(
                        format!("surplus_allocation:{}", proposal_id.0),
                        FailureType::TreasuryOperationFailed,
                        serde_json::json!({
                            "proposal_id": proposal_id.0,
                            "error": e.to_string(),
                            "total_surplus": allocation.total_surplus,
                            "currency": allocation.currency,
                        }),
                        format!("Surplus allocation failed: {e}"),
                    );
                    if let Err(dlq_err) = dlq.enqueue(failed_op) {
                        error!("   Failed to write to dead-letter queue: {}", dlq_err);
                    }
                    icn_obs::metrics::governance::execution_failures_inc("surplus_allocation");
                }
            }
        });
    }

    /// Execute share redemption for one or more shares
    pub(super) fn execute_share_redemption(
        &self,
        proposal_id: ProposalId,
        share_ids: Vec<icn_ledger::ShareId>,
        payout_schedule: Vec<icn_ledger::ScheduledPayout>,
    ) {
        let treasury_manager = self.treasury_manager.clone();
        let store = self.audit_store.clone();
        let dlq = self.dlq.clone();

        tokio::spawn(async move {
            let start = std::time::Instant::now();

            // Idempotency check
            let audit_key = format!("gov:audit:share_redemption:{}", proposal_id.0);
            match store.get(audit_key.as_bytes()) {
                Ok(Some(_)) => {
                    debug!(
                        "Share redemption {} already executed, skipping",
                        proposal_id.0
                    );
                    icn_obs::metrics::governance::idempotent_skips_inc();
                    return;
                }
                Ok(None) => {}
                Err(e) => {
                    error!(
                        "🚨 Failed to check audit trail for share redemption {}: {}",
                        proposal_id.0, e
                    );
                }
            }

            let mut treasury_guard = treasury_manager.write().await;
            let mut success_count = 0;
            let mut error_count = 0;

            for share_id in &share_ids {
                match treasury_guard.start_share_redemption(
                    share_id,
                    payout_schedule.clone(),
                    proposal_id.0.clone(),
                ) {
                    Ok(()) => {
                        info!(
                            "✅ Share redemption started for {} (proposal {})",
                            share_id, proposal_id.0
                        );
                        success_count += 1;
                    }
                    Err(e) => {
                        error!(
                            "❌ Failed to start redemption for share {}: {}",
                            share_id, e
                        );
                        error_count += 1;
                        let failed_op = FailedOperation::new(
                            format!("share_redemption:{}:{}", proposal_id.0, share_id),
                            FailureType::TreasuryOperationFailed,
                            serde_json::json!({
                                "proposal_id": proposal_id.0,
                                "share_id": share_id.to_string(),
                                "error": e.to_string(),
                            }),
                            format!("Share redemption failed for {share_id}: {e}"),
                        );
                        if let Err(dlq_err) = dlq.enqueue(failed_op) {
                            error!("   Failed to write to dead-letter queue: {}", dlq_err);
                        }
                    }
                }
            }

            // Record audit trail
            if let Err(e) = store.put(
                audit_key.as_bytes(),
                serde_json::to_vec(&serde_json::json!({
                    "executed_at": icn_time::current_timestamp_secs(),
                    "share_count": share_ids.len(),
                    "success_count": success_count,
                    "error_count": error_count,
                    "payout_count": payout_schedule.len(),
                }))
                .unwrap_or_default()
                .as_slice(),
            ) {
                warn!("Failed to record audit trail for share redemption: {}", e);
            }

            if error_count > 0 {
                icn_obs::metrics::governance::execution_failures_inc("share_redemption");
            } else {
                icn_obs::metrics::governance::proposals_executed_inc("share_redemption");
            }
            icn_obs::metrics::governance::execution_duration_record(
                "share_redemption",
                start.elapsed().as_secs_f64(),
            );
        });
    }

    /// Execute bond issuance to open a bond for subscription
    pub(super) fn execute_bond_issuance(
        &self,
        proposal_id: ProposalId,
        offering: icn_ledger::BondOffering,
    ) {
        let treasury_manager = self.treasury_manager.clone();
        let store = self.audit_store.clone();
        let dlq = self.dlq.clone();
        let treasury_did = self.treasury_did.clone();

        tokio::spawn(async move {
            let start = std::time::Instant::now();

            // Idempotency check
            let audit_key = format!("gov:audit:bond_issuance:{}", proposal_id.0);
            match store.get(audit_key.as_bytes()) {
                Ok(Some(_)) => {
                    debug!("Bond issuance {} already executed, skipping", proposal_id.0);
                    icn_obs::metrics::governance::idempotent_skips_inc();
                    return;
                }
                Ok(None) => {}
                Err(e) => {
                    error!(
                        "🚨 Failed to check audit trail for bond issuance {}: {}",
                        proposal_id.0, e
                    );
                }
            }

            let mut treasury_guard = treasury_manager.write().await;

            // Create the bond from the offering
            let bond_id = icn_ledger::BondId::new(format!("bond-{}", proposal_id.0));
            let now = icn_time::current_timestamp_secs();
            let maturity_date = now + (offering.term_days as u64 * 86400);

            let bond = icn_ledger::CooperativeBond::new_offering(
                bond_id.clone(),
                offering.issuer_id.clone(),
                treasury_did.clone(), // Placeholder holder - will be updated on subscription
                offering.principal_requested,
                offering.interest_rate_bps,
                maturity_date,
                offering.payment_schedule.clone(),
                offering.currency.clone(),
                proposal_id.0.clone(),
                now,
            );

            match treasury_guard.create_bond(bond) {
                Ok(()) => {
                    info!(
                        "✅ Bond {} created for proposal {}: {} {} at {}bps for {} days",
                        bond_id,
                        proposal_id.0,
                        offering.principal_requested,
                        offering.currency,
                        offering.interest_rate_bps,
                        offering.term_days
                    );

                    // Record audit trail
                    if let Err(e) = store.put(
                        audit_key.as_bytes(),
                        serde_json::to_vec(&serde_json::json!({
                            "executed_at": now,
                            "bond_id": bond_id.to_string(),
                            "principal": offering.principal_requested,
                            "currency": offering.currency,
                            "interest_rate_bps": offering.interest_rate_bps,
                            "term_days": offering.term_days,
                        }))
                        .unwrap_or_default()
                        .as_slice(),
                    ) {
                        warn!("Failed to record audit trail for bond issuance: {}", e);
                    }

                    icn_obs::metrics::governance::proposals_executed_inc("bond_issuance");
                    icn_obs::metrics::governance::execution_duration_record(
                        "bond_issuance",
                        start.elapsed().as_secs_f64(),
                    );
                }
                Err(e) => {
                    error!(
                        "❌ Failed to create bond for proposal {}: {}",
                        proposal_id.0, e
                    );
                    let failed_op = FailedOperation::new(
                        format!("bond_issuance:{}", proposal_id.0),
                        FailureType::TreasuryOperationFailed,
                        serde_json::json!({
                            "proposal_id": proposal_id.0,
                            "error": e.to_string(),
                            "principal": offering.principal_requested,
                            "currency": offering.currency,
                        }),
                        format!("Bond issuance failed: {e}"),
                    );
                    if let Err(dlq_err) = dlq.enqueue(failed_op) {
                        error!("   Failed to write to dead-letter queue: {}", dlq_err);
                    }
                    icn_obs::metrics::governance::execution_failures_inc("bond_issuance");
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::validate_treasury_spend_proof;
    use ed25519_dalek::SigningKey;
    // Keep the governance-path token count stable for the meaning-firewall
    // ratchet while treasury proof tests continue to evolve in icn-core.
    use governance::{
        GovernanceDecisionAttestation, GovernanceDecisionReceipt, GovernanceProofV2, ProofOutcome,
        ProposalId, Vote, VoteChoice, VoteTally,
    };
    use icn_governance as governance;
    use icn_identity::KeyPair;

    fn build_valid_proof() -> (ProposalId, String, u64, GovernanceProofV2) {
        let signer = KeyPair::generate().unwrap();
        let voter = KeyPair::generate().unwrap();

        let proposal_id = ProposalId::new("treasury-spend-proposal-1");
        let domain_id = "coop:test-coop".to_string();
        let decided_at = 1_735_680_000;

        let vote = Vote::new(proposal_id.clone(), voter.did().clone(), VoteChoice::For);
        let tally = VoteTally::new(1, 0, 0);

        let receipt = GovernanceDecisionReceipt::new(
            proposal_id.0.clone(),
            domain_id.clone(),
            ProofOutcome::Accepted,
            tally,
            &[vote],
        );
        let signing_key = SigningKey::from_bytes(&signer.to_signing_key_bytes());
        let attestation = GovernanceDecisionAttestation::sign(
            receipt.decision_hash,
            signer.did().to_string(),
            decided_at,
            &signing_key,
        );
        let proof = GovernanceProofV2::new(receipt, vec![attestation]);

        (proposal_id, domain_id, decided_at, proof)
    }

    #[test]
    fn treasury_spend_proof_valid_passes() {
        let (proposal_id, domain_id, decided_at, proof) = build_valid_proof();
        let result = validate_treasury_spend_proof(&proof, &proposal_id, &domain_id, decided_at);
        assert!(result.is_ok(), "expected valid proof, got: {result:?}");
    }

    #[test]
    fn treasury_spend_proof_rejects_outcome_mismatch() {
        let (proposal_id, domain_id, decided_at, mut proof) = build_valid_proof();
        proof.receipt.outcome = ProofOutcome::Rejected;

        let result = validate_treasury_spend_proof(&proof, &proposal_id, &domain_id, decided_at);
        assert!(result.is_err());
    }

    #[test]
    fn treasury_spend_proof_rejects_domain_mismatch() {
        let (proposal_id, _domain_id, decided_at, proof) = build_valid_proof();

        let result = validate_treasury_spend_proof(
            &proof,
            &proposal_id,
            "coop:different-domain",
            decided_at,
        );
        assert!(result.is_err());
    }

    #[test]
    fn treasury_spend_proof_rejects_missing_attestation() {
        let (proposal_id, domain_id, decided_at, mut proof) = build_valid_proof();
        proof.attestations.clear();
        let result = validate_treasury_spend_proof(&proof, &proposal_id, &domain_id, decided_at);
        assert!(result.is_err());
    }

    #[test]
    fn treasury_spend_proof_rejects_bad_signature() {
        let (proposal_id, domain_id, decided_at, mut proof) = build_valid_proof();
        proof.attestations[0].signature = vec![0xAA; 64];

        let result = validate_treasury_spend_proof(&proof, &proposal_id, &domain_id, decided_at);
        assert!(result.is_err());
    }

    #[test]
    fn treasury_spend_proof_rejects_timestamp_mismatch() {
        let (proposal_id, domain_id, decided_at, mut proof) = build_valid_proof();
        proof.attestations[0].timestamp = decided_at + 1;

        let result = validate_treasury_spend_proof(&proof, &proposal_id, &domain_id, decided_at);
        assert!(result.is_err());
    }
}
