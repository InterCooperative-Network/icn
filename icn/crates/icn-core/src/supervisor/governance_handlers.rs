//! Governance event handlers for proposal execution
//!
//! This module extracts the governance event handlers from the supervisor,
//! providing testable, focused handlers for each proposal type.

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::dead_letter::{FailedOperation, FailureType};
use crate::governance::GovernanceHandle;
use icn_federation::{AttestationStore, ClearingManager, CooperativeRegistry};
use icn_governance::{GovernanceDomainId, ProposalId, ProposalPayload};
use icn_identity::Did;
use icn_ledger::{DisputeManager, TreasuryManager};
use icn_store::SledStore;

/// Type alias for the ledger handle
pub type LedgerHandle = Arc<RwLock<icn_ledger::Ledger>>;

/// Type alias for the dead-letter queue
pub type DeadLetterQueue = Arc<crate::dead_letter::DeadLetterQueue<SledStore>>;

/// Type alias for the audit store
pub type AuditStore = Arc<dyn icn_store::Store>;

/// Type alias for the dispute manager handle
pub type DisputeManagerHandle = Arc<RwLock<DisputeManager>>;

/// Type alias for the treasury manager handle
pub type TreasuryManagerHandle = Arc<RwLock<TreasuryManager>>;

/// Type alias for the event bus
pub type EventBus = Arc<crate::events::EventBus>;

/// Type alias for the cooperative registry handle
pub type CooperativeRegistryHandle = Arc<CooperativeRegistry>;

/// Type alias for the clearing manager handle
pub type ClearingManagerHandle = Arc<ClearingManager>;

/// Type alias for the attestation store handle
pub type AttestationStoreHandle = Arc<AttestationStore>;

// =============================================================================
// Validation helpers - shared logic for treasury operation validation
// =============================================================================

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
fn validate_positive_amount(
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
fn validate_currency_match(
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

/// Handler for governance proposal events
///
/// Encapsulates all dependencies needed to execute governance proposals,
/// providing methods for each proposal type.
#[derive(Clone)]
pub struct GovernanceEventHandler {
    /// Ledger for balance transfers
    ledger: LedgerHandle,
    /// Audit store for idempotency and audit trail
    audit_store: AuditStore,
    /// Dead-letter queue for failed operations
    dlq: DeadLetterQueue,
    /// Governance actor handle
    gov_handle: GovernanceHandle,
    /// Dispute manager for dispute resolution
    dispute_manager: DisputeManagerHandle,
    /// Treasury manager for cooperative treasury operations
    treasury_manager: TreasuryManagerHandle,
    /// Treasury DID for budget payouts
    treasury_did: Did,
    /// Event bus for emitting system events
    event_bus: Option<EventBus>,

    // Federation components (optional - only set if federation is enabled)
    /// Cooperative registry for federation membership
    federation_registry: Option<CooperativeRegistryHandle>,
    /// Clearing manager for bilateral clearing agreements
    clearing_manager: Option<ClearingManagerHandle>,
    /// Attestation store for trust attestations
    attestation_store: Option<AttestationStoreHandle>,
}

impl GovernanceEventHandler {
    /// Create a new governance event handler
    pub fn new(
        ledger: LedgerHandle,
        audit_store: AuditStore,
        dlq: DeadLetterQueue,
        gov_handle: GovernanceHandle,
        dispute_manager: DisputeManagerHandle,
        treasury_manager: TreasuryManagerHandle,
        treasury_did: Did,
    ) -> Self {
        Self {
            ledger,
            audit_store,
            dlq,
            gov_handle,
            dispute_manager,
            treasury_manager,
            treasury_did,
            event_bus: None,
            federation_registry: None,
            clearing_manager: None,
            attestation_store: None,
        }
    }

    /// Set the event bus for error reporting
    pub fn with_event_bus(mut self, event_bus: EventBus) -> Self {
        self.event_bus = Some(event_bus);
        self
    }

    /// Set the federation components for federation proposal execution
    ///
    /// Call this when federation is enabled to enable execution of
    /// federation governance proposals.
    pub fn with_federation(
        mut self,
        registry: CooperativeRegistryHandle,
        clearing: ClearingManagerHandle,
        attestations: AttestationStoreHandle,
    ) -> Self {
        self.federation_registry = Some(registry);
        self.clearing_manager = Some(clearing);
        self.attestation_store = Some(attestations);
        self
    }

    /// Emit a proposal execution failure event
    fn emit_execution_failure(&self, proposal_id: &ProposalId, proposal_type: &str, error: &str) {
        if let Some(ref bus) = self.event_bus {
            let bus = bus.clone();
            let event = crate::events::SystemEvent::ProposalExecutionFailed {
                proposal_id: proposal_id.clone(),
                proposal_type: proposal_type.to_string(),
                error: error.to_string(),
                failed_at: icn_time::current_timestamp_secs(),
            };
            // Spawn async emit in the background
            // Note: EventBus::emit() is infallible (broadcasts to all subscribers)
            tokio::spawn(async move {
                bus.emit(event).await;
            });
        }
    }

    /// Emit a protocol parameter changed event for audit logging
    fn emit_parameter_changed(
        &self,
        parameter_id: &str,
        old_value: &str,
        new_value: &str,
        proposal_id: Option<String>,
        changed_by: Option<String>,
    ) {
        if let Some(ref bus) = self.event_bus {
            let bus = bus.clone();
            let event = crate::events::SystemEvent::ProtocolParameterChanged {
                parameter_id: parameter_id.to_string(),
                old_value: old_value.to_string(),
                new_value: new_value.to_string(),
                proposal_id,
                changed_by,
                changed_at: icn_time::current_timestamp_secs(),
            };
            // Spawn async emit in the background
            // Note: EventBus::emit() is infallible (broadcasts to all subscribers)
            tokio::spawn(async move {
                bus.emit(event).await;
            });
        }
    }

    /// Handle a proposal accepted event
    pub fn handle_proposal_accepted(
        &self,
        proposal_id: ProposalId,
        payload: ProposalPayload,
        decided_at: u64,
        domain_id: String,
    ) {
        match payload {
            ProposalPayload::Budget {
                amount,
                recipient,
                currency,
                purpose: _,
            } => {
                self.handle_budget_proposal(proposal_id, amount, recipient, currency, decided_at);
            }
            ProposalPayload::ConfigChange { new_config } => {
                self.handle_config_change(proposal_id, new_config, domain_id);
            }
            ProposalPayload::Membership { action, member } => {
                self.handle_membership_change(proposal_id, action, member, domain_id);
            }
            ProposalPayload::Text { .. } => {
                info!(
                    "📝 Text proposal {} accepted (no action required)",
                    proposal_id.0
                );
            }
            ProposalPayload::SchedulingPolicy { coop_id, .. } => {
                // Handled by separate subscription after compute actor spawns
                info!(
                    "📋 Scheduling policy proposal {} accepted for {} (handled separately)",
                    proposal_id.0, coop_id
                );
            }
            ProposalPayload::FreezeMember {
                member,
                reason,
                duration_seconds,
            } => {
                self.handle_freeze_member(
                    proposal_id,
                    member,
                    reason,
                    duration_seconds.unwrap_or(0),
                );
            }
            ProposalPayload::UnfreezeMember { member, reason } => {
                self.handle_unfreeze_member(proposal_id, member, reason);
            }
            ProposalPayload::VetoProposal {
                target_proposal_id,
                reason,
            } => {
                self.handle_veto_proposal(proposal_id, target_proposal_id, reason);
            }
            ProposalPayload::ForceCloseProposal {
                target_proposal_id,
                reason,
                forced_outcome,
            } => {
                self.handle_force_close(proposal_id, target_proposal_id, reason, forced_outcome);
            }
            ProposalPayload::RollbackLedger {
                target_hash,
                reason,
                affected_accounts: _,
            } => {
                self.handle_rollback_ledger(proposal_id, target_hash, reason);
            }
            ProposalPayload::DisputeResolution {
                dispute_entry_hash,
                filer: _,
                reason: _,
                escalation_reason: _,
                proposed_outcome,
            } => {
                self.handle_dispute_resolution(
                    proposal_id,
                    dispute_entry_hash,
                    proposed_outcome,
                    decided_at,
                );
            }
            ProposalPayload::Sdis { proposal } => {
                self.handle_sdis_proposal(proposal_id, proposal);
            }
            ProposalPayload::ProtocolUpgrade {
                version,
                breaking_changes,
                migration_guide,
                deadline,
                min_required_version,
            } => {
                self.handle_protocol_upgrade(
                    proposal_id,
                    version.to_string(),
                    breaking_changes,
                    migration_guide,
                    deadline,
                    min_required_version.map(|v| v.to_string()),
                );
            }
            ProposalPayload::Treasury { operation } => {
                self.handle_treasury_proposal(proposal_id, operation, decided_at);
            }
            ProposalPayload::ProtocolChange { proposal } => {
                self.handle_protocol_change(proposal_id, proposal);
            }
            // Labor Share Operations (Issue #389)
            // These trigger treasury operations when executed
            ProposalPayload::SurplusAllocation { allocation } => {
                info!(
                    "💰 Surplus allocation {} approved: {} {} to {} shareholders",
                    proposal_id.0,
                    allocation.total_surplus,
                    allocation.currency,
                    allocation.allocations.len()
                );
                self.execute_surplus_allocation(proposal_id, allocation);
            }
            ProposalPayload::ShareRedemption {
                member,
                share_ids,
                payout_schedule,
                reason,
            } => {
                info!(
                    "🎫 Share redemption {} approved: {} shares for {} ({}), {} payouts scheduled",
                    proposal_id.0,
                    share_ids.len(),
                    member,
                    reason,
                    payout_schedule.len()
                );
                self.execute_share_redemption(proposal_id, share_ids, payout_schedule);
            }
            ProposalPayload::BondIssuance { bond_offering } => {
                info!(
                    "📜 Bond issuance {} approved: {} {} at {}bps for {} days",
                    proposal_id.0,
                    bond_offering.principal_requested,
                    bond_offering.currency,
                    bond_offering.interest_rate_bps,
                    bond_offering.term_days
                );
                self.execute_bond_issuance(proposal_id, bond_offering);
            }
            // Federation governance (Issue #514)
            ProposalPayload::Federation(federation_proposal) => {
                self.handle_federation_proposal(proposal_id, federation_proposal);
            }
        }
    }

    /// Handle a treasury proposal
    fn handle_treasury_proposal(
        &self,
        proposal_id: ProposalId,
        operation: icn_governance::TreasuryProposalOperation,
        decided_at: u64,
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

    /// Handle a budget proposal
    fn handle_budget_proposal(
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

    /// Handle a config change proposal
    fn handle_config_change(&self, proposal_id: ProposalId, new_config: String, domain_id: String) {
        info!("⚙️  Config change proposal {} accepted", proposal_id.0);

        match serde_json::from_str::<icn_governance::GovernanceConfig>(&new_config) {
            Ok(parsed_config) => {
                let gov_handle = self.gov_handle.clone();
                let dom_id = GovernanceDomainId::new(domain_id);

                tokio::spawn(async move {
                    use crate::governance::GovernanceCommand;

                    match gov_handle
                        .submit(GovernanceCommand::UpdateDomainConfig {
                            domain_id: dom_id.clone(),
                            new_config: parsed_config,
                        })
                        .await
                    {
                        Ok(_) => {
                            info!(
                                "✅ Config change proposal {} applied to domain {}",
                                proposal_id.0, dom_id.0
                            );
                            icn_obs::metrics::governance::proposals_executed_inc("config_change");
                        }
                        Err(e) => {
                            error!(
                                "❌ Failed to apply config change proposal {}: {}",
                                proposal_id.0, e
                            );
                            icn_obs::metrics::governance::execution_failures_inc("config_change");
                        }
                    }
                });
            }
            Err(e) => {
                error!(
                    "❌ Failed to parse config change proposal {}: {}",
                    proposal_id.0, e
                );
                icn_obs::metrics::governance::execution_failures_inc("config_parse");
            }
        }
    }

    /// Handle a membership change proposal
    fn handle_membership_change(
        &self,
        proposal_id: ProposalId,
        action: icn_governance::MembershipAction,
        member: Did,
        domain_id: String,
    ) {
        info!(
            "👥 Membership change proposal {} accepted: {:?} for {}",
            proposal_id.0, action, member
        );

        let gov_handle = self.gov_handle.clone();
        let dom_id = GovernanceDomainId::new(domain_id);

        tokio::spawn(async move {
            use crate::governance::GovernanceCommand;

            match gov_handle
                .submit(GovernanceCommand::UpdateMembership {
                    domain_id: dom_id.clone(),
                    action,
                    member: member.clone(),
                })
                .await
            {
                Ok(_) => {
                    info!(
                        "✅ Membership proposal {} applied to domain {}",
                        proposal_id.0, dom_id.0
                    );
                    icn_obs::metrics::governance::proposals_executed_inc("membership");
                }
                Err(e) => {
                    error!(
                        "❌ Failed to apply membership proposal {}: {}",
                        proposal_id.0, e
                    );
                    icn_obs::metrics::governance::execution_failures_inc("membership");
                }
            }
        });
    }

    /// Handle a freeze member proposal
    fn handle_freeze_member(
        &self,
        proposal_id: ProposalId,
        member: Did,
        reason: String,
        duration_seconds: u64,
    ) {
        info!(
            "🔒 EMERGENCY: Freeze member proposal {} accepted - freezing {}",
            proposal_id.0, member
        );

        let ledger = self.ledger.clone();
        // Convert 0 to None for indefinite freeze
        let duration = if duration_seconds == 0 {
            None
        } else {
            Some(duration_seconds)
        };

        tokio::spawn(async move {
            let mut ledger_guard = ledger.write().await;
            ledger_guard.freeze_member_with_metadata(
                member.clone(),
                reason,
                duration,
                Some(proposal_id.0.clone()),
                None,
            );
            info!("✅ Member {} frozen via proposal {}", member, proposal_id.0);
        });
    }

    /// Handle an unfreeze member proposal
    fn handle_unfreeze_member(&self, proposal_id: ProposalId, member: Did, reason: String) {
        info!(
            "🔓 EMERGENCY: Unfreeze member proposal {} accepted - unfreezing {}",
            proposal_id.0, member
        );

        let ledger = self.ledger.clone();

        tokio::spawn(async move {
            let mut ledger_guard = ledger.write().await;
            if ledger_guard
                .unfreeze_member_with_metadata(&member, reason, Some(proposal_id.0.clone()), None)
                .is_some()
            {
                info!(
                    "✅ Member {} unfrozen via proposal {}",
                    member, proposal_id.0
                );
            } else {
                warn!(
                    "⚠️ Member {} was not frozen, unfreeze proposal {} had no effect",
                    member, proposal_id.0
                );
            }
        });
    }

    /// Handle a veto proposal
    fn handle_veto_proposal(
        &self,
        proposal_id: ProposalId,
        target_proposal_id: String,
        reason: String,
    ) {
        info!(
            "🚫 EMERGENCY: Veto proposal {} accepted - vetoing proposal {}",
            proposal_id.0, target_proposal_id
        );
        info!("   Reason: {}", reason);

        let gov_handle = self.gov_handle.clone();
        let target_id = ProposalId(target_proposal_id);

        tokio::spawn(async move {
            use crate::governance::GovernanceCommand;

            match gov_handle
                .submit(GovernanceCommand::VetoProposal {
                    proposal_id: target_id.clone(),
                    reason,
                })
                .await
            {
                Ok(_) => {
                    info!("✅ Successfully vetoed proposal {}", target_id.0);
                }
                Err(e) => {
                    error!("❌ Failed to veto proposal {}: {}", target_id.0, e);
                }
            }
        });
    }

    /// Handle a force close proposal
    fn handle_force_close(
        &self,
        proposal_id: ProposalId,
        target_proposal_id: String,
        reason: String,
        forced_outcome: icn_governance::ForcedOutcome,
    ) {
        info!(
            "⚡ EMERGENCY: Force close proposal {} accepted - closing proposal {} as {:?}",
            proposal_id.0, target_proposal_id, forced_outcome
        );
        info!("   Reason: {}", reason);

        let gov_handle = self.gov_handle.clone();
        let target_id = ProposalId(target_proposal_id);

        tokio::spawn(async move {
            use crate::governance::GovernanceCommand;

            match gov_handle
                .submit(GovernanceCommand::ForceCloseProposal {
                    proposal_id: target_id.clone(),
                    forced_outcome: forced_outcome.clone(),
                    reason,
                })
                .await
            {
                Ok(_) => {
                    info!(
                        "✅ Successfully force-closed proposal {} as {:?}",
                        target_id.0, forced_outcome
                    );
                }
                Err(e) => {
                    error!("❌ Failed to force-close proposal {}: {}", target_id.0, e);
                }
            }
        });
    }

    /// Handle a ledger rollback proposal
    fn handle_rollback_ledger(&self, proposal_id: ProposalId, target_hash: String, reason: String) {
        error!(
            "🚨 CRITICAL EMERGENCY: Ledger rollback proposal {} accepted",
            proposal_id.0
        );
        error!("   Target hash: {}", target_hash);
        error!("   Reason: {}", reason);

        let ledger = self.ledger.clone();
        let store = self.audit_store.clone();

        tokio::spawn(async move {
            use icn_ledger::ContentHash;

            let target_bytes = match hex::decode(&target_hash) {
                Ok(bytes) if bytes.len() == 32 => {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&bytes);
                    arr
                }
                Ok(_) => {
                    error!("❌ Invalid target hash length for rollback");
                    icn_obs::metrics::governance::execution_failures_inc("invalid_rollback_hash");
                    return;
                }
                Err(e) => {
                    error!("❌ Failed to decode rollback target hash: {}", e);
                    icn_obs::metrics::governance::execution_failures_inc("invalid_rollback_hash");
                    return;
                }
            };
            let content_hash = ContentHash::from_bytes(target_bytes);

            let mut ledger_write = ledger.write().await;
            match ledger_write
                .rollback_to_entry(&content_hash, &reason, true)
                .await
            {
                Ok(archived_hashes) => {
                    info!(
                        "✅ Ledger rollback complete: archived {} entries",
                        archived_hashes.len()
                    );

                    if let Err(e) = store.put(
                        format!("gov:executed:{}", proposal_id.0).as_bytes(),
                        serde_json::json!({
                            "proposal_id": proposal_id.0,
                            "action": "ledger_rollback",
                            "target_hash": target_hash,
                            "archived_count": archived_hashes.len(),
                            "executed_at": icn_time::current_timestamp_secs()
                        })
                        .to_string()
                        .as_bytes(),
                    ) {
                        warn!("Failed to record rollback execution: {}", e);
                    }

                    icn_obs::metrics::governance::proposals_executed_inc("rollback_ledger");
                }
                Err(e) => {
                    error!("❌ Ledger rollback failed: {}", e);
                    icn_obs::metrics::governance::execution_failures_inc("rollback_failed");
                }
            }
        });
    }

    /// Handle a dispute resolution proposal
    fn handle_dispute_resolution(
        &self,
        proposal_id: ProposalId,
        dispute_entry_hash: String,
        proposed_outcome: icn_governance::DisputeResolutionOutcome,
        decided_at: u64,
    ) {
        info!("⚖️ Dispute resolution proposal {} accepted", proposal_id.0);
        info!("   Dispute entry: {}", dispute_entry_hash);
        info!("   Proposed outcome: {:?}", proposed_outcome);

        let dispute_manager = self.dispute_manager.clone();
        let store = self.audit_store.clone();

        tokio::spawn(async move {
            use icn_ledger::{ContentHash, DisputeOutcome};

            let entry_bytes = match hex::decode(&dispute_entry_hash) {
                Ok(bytes) if bytes.len() == 32 => {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&bytes);
                    arr
                }
                Ok(_) => {
                    error!("❌ Invalid dispute entry hash length");
                    icn_obs::metrics::governance::execution_failures_inc("invalid_dispute_hash");
                    return;
                }
                Err(e) => {
                    error!("❌ Failed to decode dispute entry hash: {}", e);
                    icn_obs::metrics::governance::execution_failures_inc("invalid_dispute_hash");
                    return;
                }
            };
            let content_hash = ContentHash::from_bytes(entry_bytes);

            let ledger_outcome = match &proposed_outcome {
                icn_governance::DisputeResolutionOutcome::Uphold => DisputeOutcome::Reversed,
                icn_governance::DisputeResolutionOutcome::Reject => DisputeOutcome::Upheld,
                icn_governance::DisputeResolutionOutcome::Partial {
                    adjustment,
                    currency,
                } => DisputeOutcome::Settlement {
                    terms: format!("Partial adjustment: {adjustment} {currency}"),
                    replacement_entry: None,
                },
                icn_governance::DisputeResolutionOutcome::VoidTransaction => {
                    DisputeOutcome::Reversed
                }
            };

            let mut dm = dispute_manager.write().await;
            match dm.resolve_escalated_dispute(&content_hash, ledger_outcome.clone(), decided_at) {
                Ok(()) => {
                    info!(
                        "✅ Dispute {} resolved: {:?}",
                        dispute_entry_hash, ledger_outcome
                    );

                    if let Err(e) = store.put(
                        format!("gov:executed:{}", proposal_id.0).as_bytes(),
                        serde_json::json!({
                            "proposal_id": proposal_id.0,
                            "action": "dispute_resolution",
                            "entry_hash": dispute_entry_hash,
                            "outcome": format!("{:?}", ledger_outcome),
                            "executed_at": decided_at
                        })
                        .to_string()
                        .as_bytes(),
                    ) {
                        warn!("Failed to record dispute resolution execution: {}", e);
                    }

                    icn_obs::metrics::governance::proposals_executed_inc("dispute_resolution");
                }
                Err(e) => {
                    error!("❌ Failed to resolve dispute {}: {}", dispute_entry_hash, e);
                    icn_obs::metrics::governance::execution_failures_inc(
                        "dispute_resolution_failed",
                    );
                }
            }
        });
    }

    /// Handle an SDIS proposal
    fn handle_sdis_proposal(
        &self,
        proposal_id: ProposalId,
        proposal: icn_governance::SdisProposal,
    ) {
        info!(
            "🆔 SDIS proposal {} accepted: {:?}",
            proposal_id.0, proposal
        );

        match proposal {
            icn_governance::SdisProposal::AppointSteward { candidate, .. } => {
                info!("   Action: Appoint steward {}", candidate);
                icn_obs::metrics::governance::proposals_executed_inc("sdis_appoint_steward");
            }
            icn_governance::SdisProposal::RemoveSteward { steward, .. } => {
                info!("   Action: Remove steward {}", steward);
                icn_obs::metrics::governance::proposals_executed_inc("sdis_remove_steward");
            }
            icn_governance::SdisProposal::SanctionSteward {
                steward, penalty, ..
            } => {
                info!("   Action: Sanction steward {} with {:?}", steward, penalty);
                icn_obs::metrics::governance::proposals_executed_inc("sdis_sanction_steward");
            }
            _ => {
                info!("   Action: Other SDIS operation");
                icn_obs::metrics::governance::proposals_executed_inc("sdis_other");
            }
        }
    }

    /// Handle a federation governance proposal (Issue #514)
    ///
    /// Federation proposals enable governance-controlled management of inter-cooperative
    /// relationships including joining/leaving federations, establishing clearing agreements,
    /// and managing trust attestations.
    fn handle_federation_proposal(
        &self,
        proposal_id: ProposalId,
        proposal: icn_governance::FederationProposal,
    ) {
        use icn_governance::FederationProposal;

        info!(
            "🌐 Federation proposal {} accepted: {}",
            proposal_id.0,
            proposal.action_name()
        );

        // Check if federation components are available
        let (registry, clearing, attestations) = match (
            &self.federation_registry,
            &self.clearing_manager,
            &self.attestation_store,
        ) {
            (Some(r), Some(c), Some(a)) => (r.clone(), c.clone(), a.clone()),
            _ => {
                warn!(
                    "⚠️ Federation components not available - proposal {} logged but not executed",
                    proposal_id.0
                );
                // Still log and record metrics even if federation is disabled
                icn_obs::metrics::governance::proposals_executed_inc(&format!(
                    "federation_{}_skipped",
                    proposal.action_name()
                ));
                return;
            }
        };

        match proposal {
            FederationProposal::JoinFederation {
                federation_id,
                terms,
                sponsor_coop_id,
            } => {
                self.execute_join_federation(
                    &proposal_id,
                    &registry,
                    &federation_id,
                    &terms,
                    sponsor_coop_id.as_deref(),
                );
            }
            FederationProposal::LeaveFederation {
                federation_id,
                reason,
                grace_period_days,
            } => {
                self.execute_leave_federation(
                    &proposal_id,
                    &registry,
                    &federation_id,
                    &reason,
                    grace_period_days,
                );
            }
            FederationProposal::EstablishClearing {
                partner_coop_id,
                partner_coop_did,
                max_imbalance,
                settlement_interval,
                currency,
            } => {
                self.execute_establish_clearing(
                    &proposal_id,
                    &registry,
                    &clearing,
                    &partner_coop_id,
                    &partner_coop_did,
                    max_imbalance,
                    settlement_interval,
                    &currency,
                );
            }
            FederationProposal::TerminateClearing {
                partner_coop_id,
                reason,
            } => {
                self.execute_terminate_clearing(
                    &proposal_id,
                    &registry,
                    &clearing,
                    &partner_coop_id,
                    &reason,
                );
            }
            FederationProposal::VouchForCooperative {
                target_coop_id,
                target_coop_did,
                trust_score,
                context,
                evidence,
            } => {
                self.execute_vouch_for_cooperative(
                    &proposal_id,
                    &registry,
                    &attestations,
                    &target_coop_id,
                    &target_coop_did,
                    trust_score,
                    &context,
                    evidence.as_deref(),
                );
            }
            FederationProposal::RevokeVouch {
                target_coop_id,
                reason,
            } => {
                self.execute_revoke_vouch(
                    &proposal_id,
                    &registry,
                    &attestations,
                    &target_coop_id,
                    &reason,
                );
            }
            FederationProposal::UpdateFederationPolicy {
                auto_accept_vouch_threshold,
                trust_decay_factor,
                max_attestations_per_minute,
            } => {
                self.execute_update_federation_policy(
                    &proposal_id,
                    &registry,
                    auto_accept_vouch_threshold,
                    trust_decay_factor.map(|s| s.value()),
                    max_attestations_per_minute,
                );
            }
        }
    }

    // ==========================================================================
    // Federation proposal execution helpers
    // ==========================================================================

    fn execute_join_federation(
        &self,
        proposal_id: &ProposalId,
        registry: &CooperativeRegistryHandle,
        federation_id: &str,
        terms: &icn_governance::FederationTerms,
        sponsor_coop_id: Option<&str>,
    ) {
        use crate::dead_letter::{FailedOperation, FailureType};

        // Idempotency check - prevent duplicate execution
        let idem_key = format!("federation:join:idem:{}", proposal_id.0);
        match self.audit_store.get(idem_key.as_bytes()) {
            Ok(Some(_)) => {
                debug!(
                    "Federation join proposal {} already executed, skipping",
                    proposal_id.0
                );
                icn_obs::metrics::governance::idempotent_skips_inc();
                return;
            }
            Ok(None) => {}
            Err(e) => {
                error!(
                    "🚨 Failed to check idempotency for federation join {}: {}",
                    proposal_id.0, e
                );
                let failed_op = FailedOperation::new(
                    format!("federation:join:idem:{}", proposal_id.0),
                    FailureType::IdempotencyCheckFailed,
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "idempotency_check_failed",
                    }),
                    format!("Failed to check idempotency: {e}"),
                );
                if let Err(dlq_err) = self.dlq.enqueue(failed_op) {
                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                }
                icn_obs::metrics::governance::execution_failures_inc("federation_join");
                return;
            }
        }

        info!(
            "   Action: Join federation '{}' (sponsor: {:?})",
            federation_id, sponsor_coop_id
        );
        info!(
            "   Terms: trust_threshold={}, governance_binding={}, data_sharing={:?}",
            terms.min_trust_threshold, terms.governance_binding, terms.data_sharing_level
        );

        // Update own cooperative info with federation membership
        // Note: Full implementation would update federation membership list and
        // announce to federation:registry topic. For now, we record the action.
        let own_info = registry.own_coop_info();
        info!(
            "   Coop '{}' joining federation '{}'",
            own_info.coop_id, federation_id
        );

        // Verify sponsor if provided
        if let Some(sponsor) = sponsor_coop_id {
            match registry.get(sponsor) {
                Ok(Some(_)) => {
                    info!("   Sponsor '{}' verified in registry", sponsor);
                }
                Ok(None) => {
                    warn!(
                        "   Sponsor '{}' not found in registry - proceeding without sponsor verification",
                        sponsor
                    );
                }
                Err(e) => {
                    warn!("   Failed to verify sponsor '{}': {}", sponsor, e);
                }
            }
        }

        // Record audit entry for the join
        // Include proposal_id in key to preserve history across multiple join/leave cycles
        let audit_key = format!(
            "federation:join:{}:{}:{}",
            own_info.coop_id, federation_id, proposal_id.0
        );
        if let Err(e) = self.audit_store.put(
            audit_key.as_bytes(),
            serde_json::json!({
                "proposal_id": proposal_id.0,
                "federation_id": federation_id,
                "coop_id": own_info.coop_id,
                "terms": {
                    "min_trust_threshold": terms.min_trust_threshold,
                    "governance_binding": terms.governance_binding,
                },
                "sponsor_coop_id": sponsor_coop_id,
                "joined_at": icn_time::current_timestamp_secs(),
            })
            .to_string()
            .as_bytes(),
        ) {
            warn!("   Failed to record federation join audit entry: {}", e);
        }

        // Mark idempotency key to prevent re-execution
        if let Err(e) = self.audit_store.put(idem_key.as_bytes(), b"completed") {
            warn!("   Failed to record idempotency marker: {}", e);
        }

        icn_obs::metrics::governance::proposals_executed_inc("federation_join");
    }

    fn execute_leave_federation(
        &self,
        proposal_id: &ProposalId,
        registry: &CooperativeRegistryHandle,
        federation_id: &str,
        reason: &str,
        grace_period_days: u32,
    ) {
        use crate::dead_letter::{FailedOperation, FailureType};

        // Idempotency check
        let idem_key = format!("federation:leave:idem:{}", proposal_id.0);
        match self.audit_store.get(idem_key.as_bytes()) {
            Ok(Some(_)) => {
                debug!(
                    "Federation leave proposal {} already executed, skipping",
                    proposal_id.0
                );
                icn_obs::metrics::governance::idempotent_skips_inc();
                return;
            }
            Ok(None) => {}
            Err(e) => {
                error!(
                    "🚨 Failed to check idempotency for federation leave {}: {}",
                    proposal_id.0, e
                );
                let failed_op = FailedOperation::new(
                    format!("federation:leave:idem:{}", proposal_id.0),
                    FailureType::IdempotencyCheckFailed,
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "idempotency_check_failed",
                    }),
                    format!("Failed to check idempotency: {e}"),
                );
                if let Err(dlq_err) = self.dlq.enqueue(failed_op) {
                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                }
                icn_obs::metrics::governance::execution_failures_inc("federation_leave");
                return;
            }
        }

        info!(
            "   Action: Leave federation '{}' (grace period: {} days)",
            federation_id, grace_period_days
        );
        info!("   Reason: {}", reason);

        let own_info = registry.own_coop_info();
        // Use saturating arithmetic to prevent overflow on very large grace periods
        let grace_seconds = u64::from(grace_period_days).saturating_mul(86400);
        let departure_timestamp = icn_time::current_timestamp_secs().saturating_add(grace_seconds);

        info!(
            "   Coop '{}' scheduled to leave federation '{}' at timestamp {}",
            own_info.coop_id, federation_id, departure_timestamp
        );

        // Record audit entry for the leave
        // Include proposal_id in key to preserve history across multiple join/leave cycles
        let audit_key = format!(
            "federation:leave:{}:{}:{}",
            own_info.coop_id, federation_id, proposal_id.0
        );
        if let Err(e) = self.audit_store.put(
            audit_key.as_bytes(),
            serde_json::json!({
                "proposal_id": proposal_id.0,
                "federation_id": federation_id,
                "coop_id": own_info.coop_id,
                "reason": reason,
                "grace_period_days": grace_period_days,
                "scheduled_departure": departure_timestamp,
                "initiated_at": icn_time::current_timestamp_secs(),
            })
            .to_string()
            .as_bytes(),
        ) {
            warn!("   Failed to record federation leave audit entry: {}", e);
        }

        // Mark idempotency key to prevent re-execution
        if let Err(e) = self.audit_store.put(idem_key.as_bytes(), b"completed") {
            warn!("   Failed to record idempotency marker: {}", e);
        }

        icn_obs::metrics::governance::proposals_executed_inc("federation_leave");
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_establish_clearing(
        &self,
        proposal_id: &ProposalId,
        registry: &CooperativeRegistryHandle,
        clearing: &ClearingManagerHandle,
        partner_coop_id: &str,
        partner_coop_did: &Did,
        max_imbalance: i64,
        settlement_interval: icn_federation::SettlementInterval,
        currency: &str,
    ) {
        use crate::dead_letter::{FailedOperation, FailureType};
        use icn_federation::BilateralClearingAgreement;

        // Idempotency check
        let idem_key = format!("federation:clearing:idem:{}", proposal_id.0);
        match self.audit_store.get(idem_key.as_bytes()) {
            Ok(Some(_)) => {
                debug!(
                    "Federation establish clearing proposal {} already executed, skipping",
                    proposal_id.0
                );
                icn_obs::metrics::governance::idempotent_skips_inc();
                return;
            }
            Ok(None) => {}
            Err(e) => {
                error!(
                    "🚨 Failed to check idempotency for establish clearing {}: {}",
                    proposal_id.0, e
                );
                let failed_op = FailedOperation::new(
                    format!("federation:clearing:idem:{}", proposal_id.0),
                    FailureType::IdempotencyCheckFailed,
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "idempotency_check_failed",
                    }),
                    format!("Failed to check idempotency: {e}"),
                );
                if let Err(dlq_err) = self.dlq.enqueue(failed_op) {
                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                }
                icn_obs::metrics::governance::execution_failures_inc(
                    "federation_establish_clearing",
                );
                return;
            }
        }

        // Validate max_imbalance is positive
        if max_imbalance <= 0 {
            error!(
                "   Invalid max_imbalance for proposal {}: {} (must be positive)",
                proposal_id.0, max_imbalance
            );
            let failed_op = FailedOperation::new(
                format!("federation:clearing:{}", proposal_id.0),
                FailureType::FederationOperationFailed,
                serde_json::json!({
                    "proposal_id": proposal_id.0,
                    "error": "invalid_max_imbalance",
                    "max_imbalance": max_imbalance,
                }),
                format!("max_imbalance must be positive, got: {max_imbalance}"),
            );
            if let Err(dlq_err) = self.dlq.enqueue(failed_op) {
                error!("   Failed to write to dead-letter queue: {}", dlq_err);
            }
            icn_obs::metrics::governance::execution_failures_inc("federation_establish_clearing");
            return;
        }

        info!(
            "   Action: Establish clearing with '{}' ({})",
            partner_coop_id, partner_coop_did
        );
        info!(
            "   Terms: max_imbalance={} {}, settlement={:?}",
            max_imbalance, currency, settlement_interval
        );

        // Verify partner exists in registry
        match registry.get(partner_coop_id) {
            Ok(Some(partner_info)) => {
                if partner_info.public_did != *partner_coop_did {
                    warn!(
                        "   Partner DID mismatch: registry has {}, proposal has {}",
                        partner_info.public_did, partner_coop_did
                    );
                    // Still proceed but log the warning
                }
            }
            Ok(None) => {
                warn!(
                    "   Partner '{}' not found in registry - proceeding with provided DID",
                    partner_coop_id
                );
            }
            Err(e) => {
                error!("   Failed to lookup partner in registry: {}", e);
                let failed_op = FailedOperation::new(
                    format!("federation:clearing:{}", proposal_id.0),
                    FailureType::FederationOperationFailed,
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "partner_lookup_failed",
                        "partner_coop_id": partner_coop_id,
                    }),
                    format!("Failed to lookup partner: {e}"),
                );
                if let Err(dlq_err) = self.dlq.enqueue(failed_op) {
                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                }
                icn_obs::metrics::governance::execution_failures_inc(
                    "federation_establish_clearing",
                );
                return;
            }
        }

        // Create the clearing agreement
        let own_info = registry.own_coop_info();
        let agreement_id = format!(
            "clearing:{}:{}:{}",
            own_info.coop_id,
            partner_coop_id,
            icn_time::current_timestamp_secs()
        );

        let mut agreement = BilateralClearingAgreement::new(
            agreement_id.clone(),
            own_info.coop_id.clone(),
            own_info.public_did.clone(),
            partner_coop_id.to_string(),
            partner_coop_did.clone(),
        );
        agreement.max_imbalance = max_imbalance;
        agreement.settlement_interval = settlement_interval;
        // NOTE: Clearing agreements are currently same-currency only.
        // We encode this as a 1:1 exchange rate keyed by `{currency}:{currency}`,
        // i.e. the same currency on both sides. Cross-currency clearing and
        // non-1:1 exchange rates are not yet supported and would require
        // extending the clearing model and agreement schema.
        agreement
            .exchange_rates
            .insert(format!("{currency}:{currency}"), 1.0);

        match clearing.create_agreement(agreement) {
            Ok(id) => {
                info!("   ✓ Clearing agreement created: {}", id);

                // Record audit entry
                // Include proposal_id in key to preserve history across multiple agreements
                let audit_key = format!(
                    "federation:clearing:{}:{}:{}",
                    own_info.coop_id, partner_coop_id, proposal_id.0
                );
                if let Err(e) = self.audit_store.put(
                    audit_key.as_bytes(),
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "agreement_id": id,
                        "partner_coop_id": partner_coop_id,
                        "partner_coop_did": partner_coop_did.to_string(),
                        "max_imbalance": max_imbalance,
                        "currency": currency,
                        "created_at": icn_time::current_timestamp_secs(),
                    })
                    .to_string()
                    .as_bytes(),
                ) {
                    warn!("   Failed to record clearing agreement audit entry: {}", e);
                }

                // Mark idempotency key to prevent re-execution
                if let Err(e) = self.audit_store.put(idem_key.as_bytes(), b"completed") {
                    warn!("   Failed to record idempotency marker: {}", e);
                }

                icn_obs::metrics::governance::proposals_executed_inc(
                    "federation_establish_clearing",
                );
            }
            Err(e) => {
                error!("   ✗ Failed to create clearing agreement: {}", e);
                icn_obs::metrics::governance::execution_failures_inc(
                    "federation_establish_clearing",
                );
            }
        }
    }

    fn execute_terminate_clearing(
        &self,
        proposal_id: &ProposalId,
        registry: &CooperativeRegistryHandle,
        clearing: &ClearingManagerHandle,
        partner_coop_id: &str,
        reason: &str,
    ) {
        use crate::dead_letter::{FailedOperation, FailureType};

        // Idempotency check
        let idem_key = format!("federation:terminate:idem:{}", proposal_id.0);
        match self.audit_store.get(idem_key.as_bytes()) {
            Ok(Some(_)) => {
                debug!(
                    "Federation terminate clearing proposal {} already executed, skipping",
                    proposal_id.0
                );
                icn_obs::metrics::governance::idempotent_skips_inc();
                return;
            }
            Ok(None) => {}
            Err(e) => {
                error!(
                    "🚨 Failed to check idempotency for terminate clearing {}: {}",
                    proposal_id.0, e
                );
                let failed_op = FailedOperation::new(
                    format!("federation:terminate:idem:{}", proposal_id.0),
                    FailureType::IdempotencyCheckFailed,
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "idempotency_check_failed",
                    }),
                    format!("Failed to check idempotency: {e}"),
                );
                if let Err(dlq_err) = self.dlq.enqueue(failed_op) {
                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                }
                icn_obs::metrics::governance::execution_failures_inc(
                    "federation_terminate_clearing",
                );
                return;
            }
        }

        info!("   Action: Terminate clearing with '{}'", partner_coop_id);
        info!("   Reason: {}", reason);

        // Find the agreement between us and this partner
        // We need to verify we're actually a party to this agreement
        let agreements = clearing.list_agreements();
        let own_coop_id = registry.own_coop_id().to_string();
        let matching_agreement = agreements.iter().find(|a| {
            (a.coop_a == partner_coop_id && a.coop_b == own_coop_id)
                || (a.coop_b == partner_coop_id && a.coop_a == own_coop_id)
        });

        match matching_agreement {
            Some(agreement) => {
                // First, try to trigger final settlement
                match clearing.trigger_settlement(&agreement.agreement_id) {
                    Ok(report) => {
                        info!(
                            "   Final settlement completed: {} transfers, net settlement: {}",
                            report.transfers_settled, report.net_settlement
                        );
                    }
                    Err(e) => {
                        warn!("   Could not complete final settlement: {}", e);
                        // Continue with termination anyway
                    }
                }

                // Record audit entry for termination
                let audit_key =
                    format!("federation:clearing:terminated:{}", agreement.agreement_id);
                if let Err(e) = self.audit_store.put(
                    audit_key.as_bytes(),
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "agreement_id": agreement.agreement_id,
                        "partner_coop_id": partner_coop_id,
                        "reason": reason,
                        "terminated_at": icn_time::current_timestamp_secs(),
                    })
                    .to_string()
                    .as_bytes(),
                ) {
                    warn!("   Failed to record termination audit entry: {}", e);
                }

                // NOTE: ClearingManager currently lacks a delete_agreement method.
                // The agreement remains in the store but is marked as terminated
                // via the audit entry. A future enhancement should add proper
                // agreement deletion to ClearingManager.
                // See: https://github.com/InterCooperative-Network/icn/issues/517#termination
                info!(
                    "   ✓ Clearing agreement '{}' terminated (settlement complete, audit recorded)",
                    agreement.agreement_id
                );

                // Mark idempotency key to prevent re-execution
                if let Err(e) = self.audit_store.put(idem_key.as_bytes(), b"completed") {
                    warn!("   Failed to record idempotency marker: {}", e);
                }

                icn_obs::metrics::governance::proposals_executed_inc(
                    "federation_terminate_clearing",
                );
            }
            None => {
                warn!(
                    "   No clearing agreement found with partner '{}'",
                    partner_coop_id
                );
                icn_obs::metrics::governance::execution_failures_inc(
                    "federation_terminate_clearing",
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_vouch_for_cooperative(
        &self,
        proposal_id: &ProposalId,
        registry: &CooperativeRegistryHandle,
        attestations: &AttestationStoreHandle,
        target_coop_id: &str,
        target_coop_did: &Did,
        trust_score: f64,
        context: &str,
        evidence: Option<&str>,
    ) {
        use crate::dead_letter::{FailedOperation, FailureType};
        use icn_federation::{EvidenceSummary, FederatedTrustAttestation, TrustContext};

        // Idempotency check
        let idem_key = format!("federation:vouch:idem:{}", proposal_id.0);
        match self.audit_store.get(idem_key.as_bytes()) {
            Ok(Some(_)) => {
                debug!(
                    "Federation vouch proposal {} already executed, skipping",
                    proposal_id.0
                );
                icn_obs::metrics::governance::idempotent_skips_inc();
                return;
            }
            Ok(None) => {}
            Err(e) => {
                error!(
                    "🚨 Failed to check idempotency for vouch {}: {}",
                    proposal_id.0, e
                );
                let failed_op = FailedOperation::new(
                    format!("federation:vouch:idem:{}", proposal_id.0),
                    FailureType::IdempotencyCheckFailed,
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "idempotency_check_failed",
                    }),
                    format!("Failed to check idempotency: {e}"),
                );
                if let Err(dlq_err) = self.dlq.enqueue(failed_op) {
                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                }
                icn_obs::metrics::governance::execution_failures_inc("federation_vouch");
                return;
            }
        }

        // Validate trust_score is in valid range [0.0, 1.0]
        if !trust_score.is_finite() || !(0.0..=1.0).contains(&trust_score) {
            error!(
                "   Invalid trust_score for proposal {}: {} (must be in [0.0, 1.0])",
                proposal_id.0, trust_score
            );
            let failed_op = FailedOperation::new(
                format!("federation:vouch:{}", proposal_id.0),
                FailureType::FederationOperationFailed,
                serde_json::json!({
                    "proposal_id": proposal_id.0,
                    "error": "invalid_trust_score",
                    "trust_score": trust_score,
                }),
                format!("trust_score must be in [0.0, 1.0], got: {trust_score}"),
            );
            if let Err(dlq_err) = self.dlq.enqueue(failed_op) {
                error!("   Failed to write to dead-letter queue: {}", dlq_err);
            }
            icn_obs::metrics::governance::execution_failures_inc("federation_vouch");
            return;
        }

        info!(
            "   Action: Vouch for '{}' ({}) with score {}",
            target_coop_id, target_coop_did, trust_score
        );
        info!("   Context: {}", context);
        if let Some(ev) = evidence {
            info!("   Evidence: {}", ev);
        }

        // Verify target exists in registry (optional - they might not be registered yet)
        match registry.get(target_coop_id) {
            Ok(Some(target_info)) => {
                if target_info.public_did != *target_coop_did {
                    warn!(
                        "   Target DID mismatch: registry has {}, proposal has {}",
                        target_info.public_did, target_coop_did
                    );
                }
            }
            Ok(None) => {
                info!(
                    "   Target '{}' not in registry - creating attestation for unregistered cooperative",
                    target_coop_id
                );
            }
            Err(e) => {
                warn!("   Failed to lookup target in registry: {}", e);
            }
        }

        // Create the trust attestation
        let own_info = registry.own_coop_info();

        // Map context string to TrustContext enum
        let context_lower = context.to_lowercase();
        let trust_context = match context_lower.as_str() {
            "economic" | "trade" => TrustContext::Economic,
            "social" | "community" => TrustContext::Social,
            "governance" | "voting" => TrustContext::Governance,
            "general" => TrustContext::General,
            other => {
                warn!(
                    "   Unrecognized trust context '{}', defaulting to General",
                    other
                );
                TrustContext::General
            }
        };

        // Build evidence summary
        let evidence_summary = evidence
            .map(|ev| {
                vec![EvidenceSummary {
                    kind: "governance_proposal".to_string(),
                    description: ev.to_string(),
                    count: None,
                }]
            })
            .unwrap_or_default();

        // Create attestation (unsigned for now - signing would require keypair access)
        let now = icn_time::current_timestamp_secs();
        let attestation = FederatedTrustAttestation {
            source_coop_id: own_info.coop_id.clone(),
            source_coop_did: own_info.public_did.clone(),
            member_did: target_coop_did.clone(),
            trust_score,
            trust_context,
            evidence_summary,
            issued_at: now,
            expires_at: now + icn_federation::defaults::ATTESTATION_EXPIRY_SECS,
            signature: Vec::new(), // Unsigned - would need keypair for signing
        };

        match attestations.store_attestation(attestation) {
            Ok(()) => {
                info!(
                    "   ✓ Trust attestation created for '{}' with score {}",
                    target_coop_id, trust_score
                );

                // Also record the vouch in the registry
                let vouch = icn_federation::Vouch::new(
                    own_info.coop_id.clone(),
                    own_info.public_did.clone(),
                    target_coop_id.to_string(),
                    trust_score,
                );
                if let Err(e) = registry.add_vouch(&vouch) {
                    warn!("   Failed to add vouch to registry: {}", e);
                }

                // Record audit entry
                // Include proposal_id in key to preserve history across multiple vouches
                let audit_key = format!(
                    "federation:vouch:{}:{}:{}",
                    own_info.coop_id, target_coop_id, proposal_id.0
                );
                if let Err(e) = self.audit_store.put(
                    audit_key.as_bytes(),
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "target_coop_id": target_coop_id,
                        "target_coop_did": target_coop_did.to_string(),
                        "trust_score": trust_score,
                        "context": context,
                        "evidence": evidence,
                        "created_at": now,
                    })
                    .to_string()
                    .as_bytes(),
                ) {
                    warn!("   Failed to record vouch audit entry: {}", e);
                }

                // Mark idempotency key to prevent re-execution
                if let Err(e) = self.audit_store.put(idem_key.as_bytes(), b"completed") {
                    warn!("   Failed to record idempotency marker: {}", e);
                }

                icn_obs::metrics::governance::proposals_executed_inc("federation_vouch");
            }
            Err(e) => {
                error!("   ✗ Failed to store trust attestation: {}", e);
                icn_obs::metrics::governance::execution_failures_inc("federation_vouch");
            }
        }
    }

    fn execute_revoke_vouch(
        &self,
        proposal_id: &ProposalId,
        registry: &CooperativeRegistryHandle,
        attestations: &AttestationStoreHandle,
        target_coop_id: &str,
        reason: &str,
    ) {
        use crate::dead_letter::{FailedOperation, FailureType};

        // Idempotency check
        let idem_key = format!("federation:revoke:idem:{}", proposal_id.0);
        match self.audit_store.get(idem_key.as_bytes()) {
            Ok(Some(_)) => {
                debug!(
                    "Federation revoke vouch proposal {} already executed, skipping",
                    proposal_id.0
                );
                icn_obs::metrics::governance::idempotent_skips_inc();
                return;
            }
            Ok(None) => {}
            Err(e) => {
                error!(
                    "🚨 Failed to check idempotency for revoke vouch {}: {}",
                    proposal_id.0, e
                );
                let failed_op = FailedOperation::new(
                    format!("federation:revoke:idem:{}", proposal_id.0),
                    FailureType::IdempotencyCheckFailed,
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "idempotency_check_failed",
                    }),
                    format!("Failed to check idempotency: {e}"),
                );
                if let Err(dlq_err) = self.dlq.enqueue(failed_op) {
                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                }
                icn_obs::metrics::governance::execution_failures_inc("federation_revoke_vouch");
                return;
            }
        }

        info!("   Action: Revoke vouch for '{}'", target_coop_id);
        info!("   Reason: {}", reason);

        let own_info = registry.own_coop_info();

        // Look up the target's DID to remove the attestation.
        // We require the target to be in the registry to know which DID to revoke.
        // Without the DID, we cannot safely identify which attestation to remove
        // (the attestation store indexes by DID, not coop_id).
        let target_did = match registry.get(target_coop_id) {
            Ok(Some(info)) => info.public_did,
            Ok(None) => {
                // Target not in registry - we cannot safely revoke without knowing the DID.
                // This could happen if the target coop was removed from the registry before
                // the revoke proposal was executed. In production, consider adding coop_id
                // indexing to the attestation store to handle this case.
                error!(
                    "   Cannot revoke vouch: target '{}' not found in registry (DID unknown)",
                    target_coop_id
                );
                let failed_op = FailedOperation::new(
                    format!("federation:revoke:{}", proposal_id.0),
                    FailureType::FederationOperationFailed,
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "target_not_found",
                        "target_coop_id": target_coop_id,
                    }),
                    format!("Target '{target_coop_id}' not found in registry"),
                );
                if let Err(dlq_err) = self.dlq.enqueue(failed_op) {
                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                }
                icn_obs::metrics::governance::execution_failures_inc("federation_revoke_vouch");
                return;
            }
            Err(e) => {
                error!("   Failed to lookup target in registry: {}", e);
                let failed_op = FailedOperation::new(
                    format!("federation:revoke:registry:{}", proposal_id.0),
                    FailureType::FederationOperationFailed,
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "registry_lookup_failed",
                        "target_coop_id": target_coop_id,
                    }),
                    format!("Registry lookup failed for '{target_coop_id}': {e}"),
                );
                if let Err(dlq_err) = self.dlq.enqueue(failed_op) {
                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                }
                icn_obs::metrics::governance::execution_failures_inc("federation_revoke_vouch");
                return;
            }
        };

        // Remove the attestation
        match attestations.remove_attestation(&target_did, &own_info.coop_id) {
            Ok(()) => {
                info!("   ✓ Trust attestation removed for '{}'", target_coop_id);
            }
            Err(e) => {
                warn!(
                    "   Attestation removal returned error (may already be gone): {}",
                    e
                );
            }
        }

        // Remove the vouch from registry
        if let Err(e) = registry.remove_vouch(&own_info.coop_id, target_coop_id) {
            warn!("   Failed to remove vouch from registry: {}", e);
        }

        // Record audit entry
        let audit_key = format!(
            "federation:vouch:revoked:{}:{}",
            own_info.coop_id, target_coop_id
        );
        if let Err(e) = self.audit_store.put(
            audit_key.as_bytes(),
            serde_json::json!({
                "proposal_id": proposal_id.0,
                "target_coop_id": target_coop_id,
                "reason": reason,
                "revoked_at": icn_time::current_timestamp_secs(),
            })
            .to_string()
            .as_bytes(),
        ) {
            warn!("   Failed to record vouch revocation audit entry: {}", e);
        }

        // Mark idempotency key to prevent re-execution
        if let Err(e) = self.audit_store.put(idem_key.as_bytes(), b"completed") {
            warn!("   Failed to record idempotency marker: {}", e);
        }

        icn_obs::metrics::governance::proposals_executed_inc("federation_revoke_vouch");
    }

    fn execute_update_federation_policy(
        &self,
        proposal_id: &ProposalId,
        registry: &CooperativeRegistryHandle,
        auto_accept_vouch_threshold: Option<f64>,
        trust_decay_factor: Option<f64>,
        max_attestations_per_minute: Option<u32>,
    ) {
        use crate::dead_letter::{FailedOperation, FailureType};

        // Idempotency check
        let idem_key = format!("federation:policy:idem:{}", proposal_id.0);
        match self.audit_store.get(idem_key.as_bytes()) {
            Ok(Some(_)) => {
                debug!(
                    "Federation policy update proposal {} already executed, skipping",
                    proposal_id.0
                );
                icn_obs::metrics::governance::idempotent_skips_inc();
                return;
            }
            Ok(None) => {}
            Err(e) => {
                error!(
                    "🚨 Failed to check idempotency for policy update {}: {}",
                    proposal_id.0, e
                );
                let failed_op = FailedOperation::new(
                    format!("federation:policy:idem:{}", proposal_id.0),
                    FailureType::IdempotencyCheckFailed,
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "idempotency_check_failed",
                    }),
                    format!("Failed to check idempotency: {e}"),
                );
                if let Err(dlq_err) = self.dlq.enqueue(failed_op) {
                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                }
                icn_obs::metrics::governance::execution_failures_inc("federation_update_policy");
                return;
            }
        }

        info!("   Action: Update federation policy");

        let own_info = registry.own_coop_info();
        let mut changes = Vec::new();

        if let Some(threshold) = auto_accept_vouch_threshold {
            info!("   Auto-accept vouch threshold: {}", threshold);
            changes.push(format!("auto_accept_vouch_threshold={threshold}"));
        }
        if let Some(decay) = trust_decay_factor {
            info!("   Trust decay factor: {}", decay);
            changes.push(format!("trust_decay_factor={decay}"));
        }
        if let Some(rate) = max_attestations_per_minute {
            info!("   Max attestations per minute: {}", rate);
            changes.push(format!("max_attestations_per_minute={rate}"));
        }

        // Record audit entry for policy update
        // Note: Actual policy application would require storing these values
        // in a configuration store and applying them to the federation components.
        let audit_key = format!(
            "federation:policy:{}:{}",
            own_info.coop_id,
            icn_time::current_timestamp_secs()
        );
        if let Err(e) = self.audit_store.put(
            audit_key.as_bytes(),
            serde_json::json!({
                "proposal_id": proposal_id.0,
                "coop_id": own_info.coop_id,
                "auto_accept_vouch_threshold": auto_accept_vouch_threshold,
                "trust_decay_factor": trust_decay_factor,
                "max_attestations_per_minute": max_attestations_per_minute,
                "updated_at": icn_time::current_timestamp_secs(),
            })
            .to_string()
            .as_bytes(),
        ) {
            warn!("   Failed to record policy update audit entry: {}", e);
        }

        // Mark idempotency key to prevent re-execution
        if let Err(e) = self.audit_store.put(idem_key.as_bytes(), b"completed") {
            warn!("   Failed to record idempotency marker: {}", e);
        }

        info!("   ✓ Federation policy update recorded: {:?}", changes);
        icn_obs::metrics::governance::proposals_executed_inc("federation_update_policy");
    }

    /// Handle a protocol upgrade proposal
    fn handle_protocol_upgrade(
        &self,
        proposal_id: ProposalId,
        version: String,
        breaking_changes: Vec<String>,
        migration_guide: Option<String>,
        deadline: u64,
        min_required_version: Option<String>,
    ) {
        info!(
            "🔄 Protocol upgrade proposal {} accepted: -> {}",
            proposal_id.0, version
        );

        info!("   Target version: {}", version);
        info!("   Deadline: {}", deadline);
        if !breaking_changes.is_empty() {
            info!("   Breaking changes: {} items", breaking_changes.len());
        }
        if let Some(guide) = migration_guide {
            info!("   Migration guide: {}", guide);
        }
        if let Some(min_ver) = min_required_version {
            info!("   Minimum required version: {}", min_ver);
        }

        icn_obs::metrics::governance::proposals_executed_inc("protocol_upgrade");
    }

    /// Handle a protocol parameter change proposal (Phase 20)
    fn handle_protocol_change(
        &self,
        proposal_id: ProposalId,
        proposal: icn_governance::ProtocolChangeProposal,
    ) {
        let proposal_id_str = proposal_id.0.clone();

        // Check if this is a delayed execution
        if let Some(effective_at) = proposal.effective_at {
            self.handle_delayed_protocol_change(proposal_id, proposal, effective_at);
            return;
        }

        info!(
            "⚙️  Protocol change proposal {} accepted: {} -> {:?}",
            proposal_id.0, proposal.parameter_id, proposal.new_value
        );

        // Get the protocol parameter store through the governance handle
        let param_result = self
            .gov_handle
            .get_protocol_parameter(&proposal.parameter_id);
        match param_result {
            Ok(Some(mut param)) => {
                // Capture old value for audit event (serialize to string for logging)
                let old_value_str = format!("{:?}", param.value);

                // Validate the new value against parameter constraints
                if let Err(e) = param.validate(&proposal.new_value) {
                    let error_msg = format!(
                        "Validation failed for parameter '{}': {}",
                        proposal.parameter_id, e
                    );
                    warn!("{} (proposal {})", error_msg, proposal_id_str);
                    self.emit_execution_failure(&proposal_id, "protocol_change", &error_msg);
                    return;
                }

                // Update the parameter with the new value
                param.value = proposal.new_value.clone();
                param.updated_at = icn_time::current_timestamp_secs();
                param.updated_by = Some(proposal_id_str.clone());

                // Update the scope if specified in the proposal (with validation)
                if let Some(scope) = proposal.scope {
                    // Defense-in-depth: verify scope override is allowed
                    // (should have been validated at proposal creation)
                    if !param.constraints.allow_override
                        && !matches!(scope, icn_governance::ParameterScope::Global)
                    {
                        let error_msg = format!(
                            "Parameter '{}' does not allow scope overrides",
                            proposal.parameter_id
                        );
                        warn!("{} (proposal {})", error_msg, proposal_id_str);
                        self.emit_execution_failure(&proposal_id, "protocol_change", &error_msg);
                        return;
                    }

                    // Re-validate entity existence at execution time (CRITICAL #3)
                    // Entity may have been deleted between proposal creation and execution.
                    // This prevents orphaned scoped parameters.
                    //
                    // Note: A narrow TOCTOU window exists between this check and parameter
                    // persistence below. In practice, entity deletion is governed and
                    // rate-limited, making this race extremely rare. Orphaned scoped
                    // parameters can be cleaned up via periodic parameter audit if needed.
                    if let Some(entity_id) = scope.entity_id() {
                        let entity_id_str = entity_id.as_str();
                        match self.gov_handle.entity_exists(entity_id_str) {
                            Ok(true) => {
                                // Entity exists, proceed with scope change
                            }
                            Ok(false) => {
                                let error_msg = format!(
                                    "Entity '{entity_id_str}' no longer exists. Cannot create scoped parameter."
                                );
                                warn!("{} (proposal {})", error_msg, proposal_id_str);
                                self.emit_execution_failure(
                                    &proposal_id,
                                    "protocol_change",
                                    &error_msg,
                                );
                                return;
                            }
                            Err(e) => {
                                let error_msg = format!(
                                    "Failed to verify entity '{entity_id_str}' existence: {e}"
                                );
                                warn!("{} (proposal {})", error_msg, proposal_id_str);
                                self.emit_execution_failure(
                                    &proposal_id,
                                    "protocol_change",
                                    &error_msg,
                                );
                                return;
                            }
                        }
                    }

                    param.scope = scope;
                }

                // Serialize new value for audit event
                let new_value_str = format!("{:?}", proposal.new_value);

                // Persist the updated parameter
                if let Err(e) = self.gov_handle.set_protocol_parameter(
                    param,
                    Some(proposal_id_str.clone()),
                    None,
                ) {
                    let error_msg = format!(
                        "Failed to persist parameter '{}': {}",
                        proposal.parameter_id, e
                    );
                    warn!("{} (proposal {})", error_msg, proposal_id_str);
                    self.emit_execution_failure(&proposal_id, "protocol_change", &error_msg);
                } else {
                    info!(
                        "✓ Protocol parameter {} updated to {:?}",
                        proposal.parameter_id, proposal.new_value
                    );

                    // Emit audit event for parameter change
                    self.emit_parameter_changed(
                        &proposal.parameter_id,
                        &old_value_str,
                        &new_value_str,
                        Some(proposal_id_str.clone()),
                        None, // changed_by is the proposal, not a specific user
                    );
                }
            }
            Ok(None) => {
                let error_msg = format!(
                    "Parameter '{}' not found, cannot apply change",
                    proposal.parameter_id
                );
                warn!("{} (proposal {})", error_msg, proposal_id_str);
                self.emit_execution_failure(&proposal_id, "protocol_change", &error_msg);
            }
            Err(e) => {
                let error_msg =
                    format!("Failed to get parameter '{}': {}", proposal.parameter_id, e);
                warn!("{} (proposal {})", error_msg, proposal_id_str);
                self.emit_execution_failure(&proposal_id, "protocol_change", &error_msg);
            }
        }

        icn_obs::metrics::governance::proposals_executed_inc("protocol_change");
    }

    /// Handle a protocol parameter change with delayed execution
    fn handle_delayed_protocol_change(
        &self,
        proposal_id: ProposalId,
        proposal: icn_governance::ProtocolChangeProposal,
        effective_at: u64,
    ) {
        let proposal_id_str = proposal_id.0.clone();

        // Calculate delay for logging
        let now = icn_time::current_timestamp_secs();
        let delay_secs = effective_at.saturating_sub(now);
        let delay_human = if delay_secs < 3600 {
            format!("{} minutes", delay_secs / 60)
        } else if delay_secs < 86400 {
            format!("{} hours", delay_secs / 3600)
        } else {
            format!("{} days", delay_secs / 86400)
        };

        info!(
            "⏰ Protocol change proposal {} scheduled for delayed execution: {} -> {:?} (effective in {})",
            proposal_id.0, proposal.parameter_id, proposal.new_value, delay_human
        );

        // Determine the scope for the pending change
        let scope = proposal
            .scope
            .clone()
            .unwrap_or(icn_governance::ParameterScope::Global);

        // Create the pending change
        let pending_change = icn_governance::PendingParameterChange::new(
            icn_governance::PendingParameterChange::generate_id(&proposal.parameter_id),
            &proposal.parameter_id,
            proposal.new_value.clone(),
            effective_at,
            scope,
            &proposal_id_str,
            &proposal.rationale,
        );

        // Store the pending change
        if let Err(e) = self.gov_handle.schedule_pending_change(pending_change) {
            let error_msg = format!(
                "Failed to schedule delayed parameter change for '{}': {}",
                proposal.parameter_id, e
            );
            warn!("{} (proposal {})", error_msg, proposal_id_str);
            self.emit_execution_failure(&proposal_id, "protocol_change", &error_msg);
            return;
        }

        info!(
            "✓ Protocol parameter change {} scheduled (effective_at: {})",
            proposal.parameter_id, effective_at
        );

        icn_obs::metrics::governance::proposals_executed_inc("protocol_change_scheduled");
    }

    // =========================================================================
    // Labor Share Operations (Issue #389)
    // =========================================================================

    /// Execute surplus allocation to shareholders
    fn execute_surplus_allocation(
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
    fn execute_share_redemption(
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
    fn execute_bond_issuance(&self, proposal_id: ProposalId, offering: icn_ledger::BondOffering) {
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

/// Create the event subscription callback using the handler
pub fn create_governance_subscription(
    handler: GovernanceEventHandler,
) -> Arc<dyn Fn(crate::events::SystemEvent) + Send + Sync> {
    Arc::new(move |event| {
        use crate::events::SystemEvent;

        match &event {
            SystemEvent::ProposalAccepted {
                proposal_id,
                payload,
                decided_at,
                domain_id,
                ..
            } => {
                handler.handle_proposal_accepted(
                    proposal_id.clone(),
                    payload.clone(),
                    *decided_at,
                    domain_id.clone(),
                );
            }
            SystemEvent::ProposalRejected { proposal_id, .. } => {
                info!("❌ Proposal {} rejected - no action taken", proposal_id.0);
            }
            _ => {}
        }
    })
}

/// Handler for scheduling policy governance events
///
/// Separate from the main GovernanceEventHandler because it requires
/// access to the compute handle, which is initialized after governance.
#[derive(Clone)]
pub struct PolicyEventHandler {
    /// Compute handle for applying policy updates
    compute_handle: icn_compute::ComputeHandle,
    /// Audit store for idempotency and audit trail
    audit_store: AuditStore,
}

impl PolicyEventHandler {
    /// Create a new policy event handler
    pub fn new(compute_handle: icn_compute::ComputeHandle, audit_store: AuditStore) -> Self {
        Self {
            compute_handle,
            audit_store,
        }
    }

    /// Handle a scheduling policy proposal
    pub fn handle_scheduling_policy(
        &self,
        proposal_id: ProposalId,
        coop_id: String,
        policy_json: String,
        decided_at: u64,
    ) {
        info!(
            "📋 Executing scheduling policy proposal {}: update policy for {}",
            proposal_id.0, coop_id
        );

        let compute_handle = self.compute_handle.clone();
        let store = self.audit_store.clone();

        tokio::spawn(async move {
            let start = std::time::Instant::now();

            // IDEMPOTENCY CHECK: Skip if proposal already executed
            let audit_key = format!("gov:audit:policy:{}", proposal_id.0);
            match store.get(audit_key.as_bytes()) {
                Ok(Some(_)) => {
                    debug!(
                        "Policy proposal {} already executed, skipping duplicate",
                        proposal_id.0
                    );
                    return;
                }
                Ok(None) => {}
                Err(e) => {
                    error!(
                        "🚨 Failed to check audit trail for policy proposal {}: {}",
                        proposal_id.0, e
                    );
                    error!("   Refusing to execute to prevent potential duplicate");
                    return;
                }
            }

            // Parse policy JSON
            match serde_json::from_str::<icn_compute::CoopSchedulingPolicy>(&policy_json) {
                Ok(policy) => {
                    // Apply policy update via ComputeHandle
                    match compute_handle.set_policy(policy.clone()).await {
                        Ok(_) => {
                            info!(
                                "✅ Scheduling policy proposal {} executed: policy updated for {}",
                                proposal_id.0, coop_id
                            );

                            // Store audit trail
                            let audit_record = serde_json::json!({
                                "proposal_id": proposal_id.0,
                                "coop_id": coop_id,
                                "decided_at": decided_at,
                                "executed_at": icn_time::current_timestamp_secs(),
                            });

                            if let Ok(audit_json) = serde_json::to_vec(&audit_record) {
                                if let Err(e) = store.put(audit_key.as_bytes(), &audit_json) {
                                    error!(
                                        "🚨 Failed to store audit trail for policy proposal {}: {}",
                                        proposal_id.0, e
                                    );
                                    icn_obs::metrics::governance::audit_failures_inc();
                                } else {
                                    info!(
                                        "📋 Audit trail recorded for policy proposal {}",
                                        proposal_id.0
                                    );

                                    // Metrics: successful execution
                                    let duration = start.elapsed().as_secs_f64();
                                    icn_obs::metrics::governance::proposals_executed_inc(
                                        "scheduling_policy",
                                    );
                                    icn_obs::metrics::governance::execution_duration_record(
                                        "scheduling_policy",
                                        duration,
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            error!(
                                "❌ Failed to apply scheduling policy for proposal {}: {}",
                                proposal_id.0, e
                            );
                            icn_obs::metrics::governance::execution_failures_inc("policy_apply");
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "❌ Failed to parse policy JSON for proposal {}: {}",
                        proposal_id.0, e
                    );
                    icn_obs::metrics::governance::execution_failures_inc("policy_parse");
                }
            }
        });
    }
}

/// Create the policy subscription callback using the handler
pub fn create_policy_subscription(
    handler: PolicyEventHandler,
) -> Arc<dyn Fn(crate::events::SystemEvent) + Send + Sync> {
    Arc::new(move |event| {
        use crate::events::SystemEvent;
        use icn_governance::ProposalPayload;

        if let SystemEvent::ProposalAccepted {
            proposal_id,
            payload:
                ProposalPayload::SchedulingPolicy {
                    coop_id,
                    policy_json,
                },
            decided_at,
            ..
        } = &event
        {
            handler.handle_scheduling_policy(
                proposal_id.clone(),
                coop_id.clone(),
                policy_json.clone(),
                *decided_at,
            );
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_store::SledStore;

    fn test_dlq() -> DeadLetterQueue {
        Arc::new(crate::dead_letter::DeadLetterQueue::new(Arc::new(
            SledStore::temporary().unwrap(),
        )))
    }

    fn test_proposal_id() -> ProposalId {
        ProposalId("test-proposal-123".to_string())
    }

    #[test]
    fn test_governance_event_handler_clone() {
        // Verify GovernanceEventHandler implements Clone
        fn assert_clone<T: Clone>() {}
        assert_clone::<GovernanceEventHandler>();
    }

    // =========================================================================
    // Tests for validate_positive_amount helper
    // =========================================================================

    #[test]
    fn test_validate_positive_amount_valid() {
        let dlq = test_dlq();
        let proposal_id = test_proposal_id();

        // Test positive amount returns true
        assert!(validate_positive_amount(
            100,
            &proposal_id,
            "budget",
            "treasury_create_budget",
            "Amount",
            &dlq,
        ));

        // Verify no DLQ entries were created
        let pending = dlq.list_pending().unwrap();
        assert!(pending.is_empty());
    }

    #[test]
    fn test_validate_positive_amount_zero() {
        let dlq = test_dlq();
        let proposal_id = test_proposal_id();

        // Test zero amount returns false
        assert!(!validate_positive_amount(
            0,
            &proposal_id,
            "budget",
            "treasury_create_budget",
            "Amount",
            &dlq,
        ));

        // Verify DLQ entry was created
        let pending = dlq.list_pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert!(pending[0].id.contains("treasury:budget:"));
        assert_eq!(
            pending[0].failure_type,
            crate::dead_letter::FailureType::TreasuryOperationFailed
        );
    }

    #[test]
    fn test_validate_positive_amount_negative() {
        let dlq = test_dlq();
        let proposal_id = test_proposal_id();

        // Test negative amount returns false
        assert!(!validate_positive_amount(
            -50,
            &proposal_id,
            "transfer",
            "treasury_transfer",
            "Transfer amount",
            &dlq,
        ));

        // Verify DLQ entry was created with correct metadata
        let pending = dlq.list_pending().unwrap();
        assert_eq!(pending.len(), 1);

        let entry = &pending[0];
        assert!(entry.id.contains("treasury:transfer:"));
        assert!(entry.error_message.contains("Transfer amount"));
        assert!(entry.error_message.contains("-50"));

        // Verify context contains amount and derived error code
        let context = &entry.context;
        assert_eq!(context["error"], "invalid_transfer_amount"); // Derived from "Transfer amount"
        assert_eq!(context["amount"], -50);
    }

    #[test]
    fn test_validate_positive_amount_threshold_field_name() {
        let dlq = test_dlq();
        let proposal_id = test_proposal_id();

        // Test with "Threshold amount" field name - verifies backward compatibility
        // Original code used "invalid_threshold_amount", helper should preserve this
        assert!(!validate_positive_amount(
            -1,
            &proposal_id,
            "rule",
            "treasury_modify_rule",
            "Threshold amount",
            &dlq,
        ));

        let pending = dlq.list_pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert!(pending[0].error_message.contains("Threshold amount"));

        // Verify the error code is derived correctly for backward compatibility
        assert_eq!(pending[0].context["error"], "invalid_threshold_amount");
    }

    // =========================================================================
    // Tests for validate_currency_match helper
    // =========================================================================

    #[test]
    fn test_validate_currency_match_valid() {
        let dlq = test_dlq();
        let proposal_id = test_proposal_id();

        // Test matching currencies returns true
        assert!(validate_currency_match(
            "USD",
            "USD",
            &proposal_id,
            "budget",
            "treasury_create_budget",
            serde_json::json!({
                "proposal_id": proposal_id.0,
                "error": "currency_mismatch",
            }),
            &dlq,
        ));

        // Verify no DLQ entries were created
        let pending = dlq.list_pending().unwrap();
        assert!(pending.is_empty());
    }

    #[test]
    fn test_validate_currency_match_mismatch() {
        let dlq = test_dlq();
        let proposal_id = test_proposal_id();

        let metadata = serde_json::json!({
            "proposal_id": proposal_id.0,
            "error": "currency_mismatch",
            "requested_currency": "EUR",
            "treasury_currency": "USD",
        });

        // Test mismatched currencies returns false
        assert!(!validate_currency_match(
            "EUR",
            "USD",
            &proposal_id,
            "budget",
            "treasury_create_budget",
            metadata.clone(),
            &dlq,
        ));

        // Verify DLQ entry was created
        let pending = dlq.list_pending().unwrap();
        assert_eq!(pending.len(), 1);

        let entry = &pending[0];
        assert!(entry.id.contains("treasury:budget:"));
        assert_eq!(
            entry.failure_type,
            crate::dead_letter::FailureType::TreasuryOperationFailed
        );
        assert!(entry.error_message.contains("EUR"));
        assert!(entry.error_message.contains("USD"));

        // Verify metadata preserved
        assert_eq!(entry.context["error"], "currency_mismatch");
        assert_eq!(entry.context["requested_currency"], "EUR");
        assert_eq!(entry.context["treasury_currency"], "USD");
    }

    #[test]
    fn test_validate_currency_match_case_sensitive() {
        let dlq = test_dlq();
        let proposal_id = test_proposal_id();

        // Test that currency comparison is case-sensitive
        assert!(!validate_currency_match(
            "usd",
            "USD",
            &proposal_id,
            "budget",
            "treasury_create_budget",
            serde_json::json!({"error": "currency_mismatch"}),
            &dlq,
        ));

        // Verify DLQ entry was created (currencies don't match due to case)
        let pending = dlq.list_pending().unwrap();
        assert_eq!(pending.len(), 1);
    }

    #[test]
    fn test_validate_currency_match_transfer_metadata() {
        let dlq = test_dlq();
        let proposal_id = test_proposal_id();

        // Test with transfer-specific metadata (different format)
        let metadata = serde_json::json!({
            "proposal_id": proposal_id.0,
            "error": "currency_mismatch",
            "from_budget": "budget-1",
            "from_currency": "USD",
            "to_budget": "budget-2",
            "to_currency": "EUR",
        });

        assert!(!validate_currency_match(
            "EUR",
            "USD",
            &proposal_id,
            "transfer",
            "treasury_transfer_between_budgets",
            metadata.clone(),
            &dlq,
        ));

        // Verify metadata is preserved exactly as passed
        let pending = dlq.list_pending().unwrap();
        let entry = &pending[0];
        assert_eq!(entry.context["from_budget"], "budget-1");
        assert_eq!(entry.context["to_budget"], "budget-2");
        assert_eq!(entry.context["from_currency"], "USD");
        assert_eq!(entry.context["to_currency"], "EUR");
    }
}
