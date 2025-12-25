//! Governance event handlers for proposal execution
//!
//! This module extracts the governance event handlers from the supervisor,
//! providing testable, focused handlers for each proposal type.

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::dead_letter::{FailedOperation, FailureType};
use crate::governance::GovernanceHandle;
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
        }
    }

    /// Set the event bus for error reporting
    pub fn with_event_bus(mut self, event_bus: EventBus) -> Self {
        self.event_bus = Some(event_bus);
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
            if amount <= 0 {
                error!(
                    "❌ Invalid amount for budget proposal {}: {} (must be positive)",
                    proposal_id.0, amount
                );
                let failed_op = FailedOperation::new(
                    format!("treasury:budget:{}", proposal_id.0),
                    FailureType::TreasuryOperationFailed,
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "invalid_amount",
                        "amount": amount,
                    }),
                    format!("Amount must be positive, got: {amount}"),
                );
                if let Err(dlq_err) = dlq.enqueue(failed_op) {
                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                }
                icn_obs::metrics::governance::execution_failures_inc("treasury_create_budget");
                return;
            }

            let mut treasury_guard = treasury_manager.write().await;

            // Validation: Currency must match treasury's configured currency
            if let Some(treasury) = treasury_guard.get_treasury(&treasury_did) {
                if treasury.currency != currency {
                    error!(
                        "❌ Currency mismatch for budget proposal {}: got '{}', treasury uses '{}'",
                        proposal_id.0, currency, treasury.currency
                    );
                    let failed_op = FailedOperation::new(
                        format!("treasury:budget:{}", proposal_id.0),
                        FailureType::TreasuryOperationFailed,
                        serde_json::json!({
                            "proposal_id": proposal_id.0,
                            "error": "currency_mismatch",
                            "requested_currency": currency,
                            "treasury_currency": treasury.currency,
                        }),
                        format!(
                            "Currency mismatch: got '{}', expected '{}'",
                            currency, treasury.currency
                        ),
                    );
                    if let Err(dlq_err) = dlq.enqueue(failed_op) {
                        error!("   Failed to write to dead-letter queue: {}", dlq_err);
                    }
                    icn_obs::metrics::governance::execution_failures_inc("treasury_create_budget");
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
            if threshold_amount <= 0 {
                error!(
                    "❌ Invalid threshold amount for spending rule proposal {}: {} (must be positive)",
                    proposal_id.0, threshold_amount
                );
                let failed_op = FailedOperation::new(
                    format!("treasury:rule:{}", proposal_id.0),
                    FailureType::TreasuryOperationFailed,
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "invalid_threshold_amount",
                        "threshold_amount": threshold_amount,
                    }),
                    format!("Threshold amount must be positive, got: {threshold_amount}"),
                );
                if let Err(dlq_err) = dlq.enqueue(failed_op) {
                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                }
                icn_obs::metrics::governance::execution_failures_inc("treasury_modify_rule");
                return;
            }

            let mut treasury_guard = treasury_manager.write().await;

            // Validation: Currency must match treasury's configured currency
            if let Some(treasury) = treasury_guard.get_treasury(&treasury_did) {
                if treasury.currency != currency {
                    error!(
                        "❌ Currency mismatch for spending rule proposal {}: got '{}', treasury uses '{}'",
                        proposal_id.0, currency, treasury.currency
                    );
                    let failed_op = FailedOperation::new(
                        format!("treasury:rule:{}", proposal_id.0),
                        FailureType::TreasuryOperationFailed,
                        serde_json::json!({
                            "proposal_id": proposal_id.0,
                            "error": "currency_mismatch",
                            "requested_currency": currency,
                            "treasury_currency": treasury.currency,
                        }),
                        format!(
                            "Currency mismatch: got '{}', expected '{}'",
                            currency, treasury.currency
                        ),
                    );
                    if let Err(dlq_err) = dlq.enqueue(failed_op) {
                        error!("   Failed to write to dead-letter queue: {}", dlq_err);
                    }
                    icn_obs::metrics::governance::execution_failures_inc("treasury_modify_rule");
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
            if amount <= 0 {
                error!(
                    "❌ Invalid transfer amount for proposal {}: {} (must be positive)",
                    proposal_id.0, amount
                );
                let failed_op = FailedOperation::new(
                    format!("treasury:transfer:{}", proposal_id.0),
                    FailureType::TreasuryOperationFailed,
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "invalid_amount",
                        "amount": amount,
                    }),
                    format!("Amount must be positive, got: {amount}"),
                );
                if let Err(dlq_err) = dlq.enqueue(failed_op) {
                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                }
                icn_obs::metrics::governance::execution_failures_inc(
                    "treasury_transfer_between_budgets",
                );
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
                if to_budget_data.currency != from_currency {
                    error!(
                        "❌ Currency mismatch for transfer proposal {}: source='{}', destination='{}'",
                        proposal_id.0, from_currency, to_budget_data.currency
                    );
                    let failed_op = FailedOperation::new(
                        format!("treasury:transfer:{}", proposal_id.0),
                        FailureType::TreasuryOperationFailed,
                        serde_json::json!({
                            "proposal_id": proposal_id.0,
                            "error": "currency_mismatch",
                            "from_budget": from_budget,
                            "from_currency": from_currency,
                            "to_budget": to_budget,
                            "to_currency": to_budget_data.currency,
                        }),
                        format!(
                            "Currency mismatch: source='{}', destination='{}'",
                            from_currency, to_budget_data.currency
                        ),
                    );
                    if let Err(dlq_err) = dlq.enqueue(failed_op) {
                        error!("   Failed to write to dead-letter queue: {}", dlq_err);
                    }
                    icn_obs::metrics::governance::execution_failures_inc(
                        "treasury_transfer_between_budgets",
                    );
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
            if amount <= 0 {
                error!(
                    "❌ Invalid reclaim amount for proposal {}: {} (must be positive)",
                    proposal_id.0, amount
                );
                let failed_op = FailedOperation::new(
                    format!("treasury:reclaim:{}", proposal_id.0),
                    FailureType::TreasuryOperationFailed,
                    serde_json::json!({
                        "proposal_id": proposal_id.0,
                        "error": "invalid_amount",
                        "amount": amount,
                    }),
                    format!("Amount must be positive, got: {amount}"),
                );
                if let Err(dlq_err) = dlq.enqueue(failed_op) {
                    error!("   Failed to write to dead-letter queue: {}", dlq_err);
                }
                icn_obs::metrics::governance::execution_failures_inc("treasury_reclaim");
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
                    if budget.currency != currency {
                        error!(
                            "❌ Currency mismatch for reclaim proposal {}: budget has '{}', request has '{}'",
                            proposal_id.0, budget.currency, currency
                        );
                        let failed_op = FailedOperation::new(
                            format!("treasury:reclaim:{}", proposal_id.0),
                            FailureType::TreasuryOperationFailed,
                            serde_json::json!({
                                "proposal_id": proposal_id.0,
                                "error": "currency_mismatch",
                                "budget_id": budget_id,
                                "budget_currency": budget.currency,
                                "requested_currency": currency,
                            }),
                            format!(
                                "Currency mismatch: budget has '{}', request has '{}'",
                                budget.currency, currency
                            ),
                        );
                        if let Err(dlq_err) = dlq.enqueue(failed_op) {
                            error!("   Failed to write to dead-letter queue: {}", dlq_err);
                        }
                        icn_obs::metrics::governance::execution_failures_inc("treasury_reclaim");
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
                Ok(entry) => match ledger_guard.append_entry(entry) {
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
            match ledger_write.rollback_to_entry(&content_hash, &reason, true) {
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
        info!(
            "⚙️  Protocol change proposal {} accepted: {} -> {:?}",
            proposal_id.0, proposal.parameter_id, proposal.new_value
        );

        // Get the protocol parameter store through the governance handle
        let param_result = self
            .gov_handle
            .get_protocol_parameter(&proposal.parameter_id);
        let proposal_id_str = proposal_id.0.clone();
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

    #[test]
    fn test_governance_event_handler_clone() {
        // Verify GovernanceEventHandler implements Clone
        fn assert_clone<T: Clone>() {}
        assert_clone::<GovernanceEventHandler>();
    }
}
