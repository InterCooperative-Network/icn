//! Governance executor adapter - implements kernel traits for governance app.
//!
//! This module provides the adapter that connects the governance app to
//! the kernel's execution services (ledger, treasury, protocol params).
//!
//! The [`KernelGovernanceExecutor`] implements [`GovernanceExecutor`] by
//! delegating to concrete kernel implementations:
//! - [`KernelTreasuryExecutor`] for treasury operations
//! - [`KernelProtocolExecutor`] for protocol parameter changes
//! - [`KernelFederationExecutor`] for federation operations
//! - [`KernelControlExecutor`] for governance control operations (veto, force close)
//! - [`KernelMembershipExecutor`] for membership operations

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use icn_kernel_api::budget::{BeginSpendOutcome, BudgetRecord, BudgetSpendError, BudgetStore};
use icn_kernel_api::effects::{
    ControlEffect, EffectResult, KernelEffect, MembershipEffect, TreasuryEffect,
};
use icn_kernel_api::escrow::{BeginReleaseOutcome, EscrowReleaseError, EscrowStore};
use icn_kernel_api::governance::{
    federation_effect_to_operation, treasury_effect_to_operation, DecisionReceiptId,
    EffectExecutor, ExecutionOutcome, FederationExecutor, FederationOperation, GovernanceExecutor,
    ProtocolChange, ProtocolExecutor, TreasuryExecutor, TreasuryOperation,
};
use icn_kernel_api::protocol_params::ProtocolParameterStore;
use icn_kernel_api::{ControlService, ForceCloseProposalRequest, VetoProposalRequest};
use sha2::{Digest, Sha256};
use tracing::{info, warn};

const LEDGER_ENTRY_MARKER: &str = "-> ledger entry ";

/// Adapter that implements [`GovernanceExecutor`] by delegating to kernel services.
///
/// This is the bridge between the governance app (domain logic) and the kernel's
/// execution capabilities. It holds references to the treasury and protocol
/// executors which perform the actual operations.
pub struct KernelGovernanceExecutor {
    treasury: Arc<dyn TreasuryExecutor>,
    protocol: Arc<KernelProtocolExecutor>,
    federation: Arc<KernelFederationExecutor>,
    control: Arc<KernelControlExecutor>,
    membership: Arc<KernelMembershipExecutor>,
    /// Escrow store for domain-level idempotency on escrow releases.
    escrow_store: Option<Arc<dyn EscrowStore>>,
    /// Budget store for budget enforcement.
    budget_store: Option<Arc<dyn BudgetStore>>,
}

impl KernelGovernanceExecutor {
    /// Create a new governance executor with the given dependencies.
    ///
    /// # Arguments
    /// * `protocol_param_store` - Store for protocol parameters (used by protocol executor)
    pub fn new(protocol_param_store: Arc<dyn ProtocolParameterStore>) -> Self {
        let treasury: Arc<dyn TreasuryExecutor> = Arc::new(KernelTreasuryExecutor::new());
        Self {
            treasury,
            protocol: Arc::new(KernelProtocolExecutor::new(protocol_param_store)),
            federation: Arc::new(KernelFederationExecutor::new()),
            control: Arc::new(KernelControlExecutor::new()),
            membership: Arc::new(KernelMembershipExecutor::new()),
            escrow_store: None,
            budget_store: None,
        }
    }

    /// Set the control service for governance control operations.
    pub fn with_control_service(mut self, service: Arc<dyn ControlService>) -> Self {
        self.control = Arc::new(KernelControlExecutor::with_service(service));
        self
    }

    /// Set the ledger service for treasury operations.
    pub fn with_ledger_service(mut self, service: Arc<dyn icn_kernel_api::LedgerService>) -> Self {
        let treasury: Arc<dyn TreasuryExecutor> =
            Arc::new(KernelTreasuryExecutor::with_ledger(service));
        self.treasury = treasury;
        self
    }

    /// Set the federation service for federation operations.
    pub fn with_federation_service(
        mut self,
        service: Arc<dyn icn_kernel_api::FederationService>,
    ) -> Self {
        self.federation = Arc::new(KernelFederationExecutor::with_service(service));
        self
    }

    /// Set the membership service for membership operations.
    pub fn with_membership_service(
        mut self,
        service: Arc<dyn icn_kernel_api::MembershipService>,
    ) -> Self {
        self.membership = Arc::new(KernelMembershipExecutor::with_service(service));
        self
    }

    /// Set the escrow store for domain-level idempotency on escrow releases.
    pub fn with_escrow_store(mut self, store: Arc<dyn EscrowStore>) -> Self {
        self.escrow_store = Some(store);
        self
    }

    /// Set the budget store for budget enforcement.
    pub fn with_budget_store(mut self, store: Arc<dyn BudgetStore>) -> Self {
        self.budget_store = Some(store);
        self
    }

    /// Execute a treasury effect with optional budget enforcement.
    ///
    /// Three paths:
    /// 1. `CreateBudget` → create a BudgetRecord in the store, then delegate to treasury
    /// 2. `Spend { budget_id: Some(...) }` → saga-enforced budget check, then treasury
    /// 3. All other treasury effects → delegate directly to treasury executor
    async fn execute_treasury_with_budget(
        &self,
        treasury_effect: TreasuryEffect,
        receipt_id: &DecisionReceiptId,
        decision_receipt_id: &str,
    ) -> Result<EffectResult> {
        match &treasury_effect {
            // Path 1: CreateBudget → ledger entry first, then persist BudgetRecord on success
            TreasuryEffect::CreateBudget {
                treasury_did,
                budget_id,
                total_amount,
                currency,
                decision_hash,
                ..
            } => {
                if let Some(ref budget_store) = self.budget_store {
                    // Check for idempotent re-creation
                    if let Some(existing) = budget_store.get(budget_id)? {
                        if existing.decision_hash == *decision_hash {
                            info!(
                                budget_id = %budget_id,
                                "Budget already created (idempotent)"
                            );
                            return Ok(EffectResult {
                                effect_id: decision_receipt_id.to_string(),
                                success: true,
                                message: format!(
                                    "Budget {} already exists (idempotent)",
                                    budget_id
                                ),
                                state_change_hash: None,
                                ledger_entry_id: None,
                                not_executed: false,
                            });
                        }
                    }
                }

                let operation = treasury_effect_to_operation(&treasury_effect);
                let outcome = self
                    .treasury
                    .execute_treasury_operation(receipt_id, operation)
                    .await?;
                let result = execution_outcome_to_effect_result(outcome, decision_receipt_id);

                if result.success {
                    if let Some(ref budget_store) = self.budget_store {
                        let record = BudgetRecord::new(
                            budget_id.clone(),
                            treasury_did.clone(),
                            treasury_did.clone(),
                            currency.clone(),
                            *total_amount,
                            decision_hash.clone(),
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs(),
                        );
                        budget_store.put(&record)?;
                        info!(
                            budget_id = %budget_id,
                            total_limit = total_amount,
                            currency = %currency,
                            "Budget record created"
                        );
                    }
                }

                Ok(result)
            }

            // Path 2: Spend with budget_id → saga-enforced budget check
            TreasuryEffect::Spend {
                budget_id: Some(budget_id),
                decision_hash,
                amount,
                ..
            } => {
                let budget_store = match &self.budget_store {
                    Some(store) => store,
                    None => {
                        warn!(
                            budget_id = %budget_id,
                            "Spend references budget but no budget store configured"
                        );
                        return Ok(EffectResult {
                            effect_id: decision_receipt_id.to_string(),
                            success: false,
                            message: "Budget store not configured".to_string(),
                            state_change_hash: None,
                            ledger_entry_id: None,
                            not_executed: false,
                        });
                    }
                };

                let mut budget = match budget_store.get(budget_id)? {
                    Some(b) => b,
                    None => {
                        return Ok(EffectResult {
                            effect_id: decision_receipt_id.to_string(),
                            success: false,
                            message: format!("Budget {} not found", budget_id),
                            state_change_hash: None,
                            ledger_entry_id: None,
                            not_executed: false,
                        });
                    }
                };

                // Phase 1: begin_spend (validates capacity, records pending)
                match budget.begin_spend(decision_hash, *amount) {
                    Ok(BeginSpendOutcome::Proceed) => {
                        budget_store.put(&budget)?;
                    }
                    Ok(BeginSpendOutcome::RetryNeeded) => {
                        // Crash recovery — pending spend already recorded, skip persist
                    }
                    Err(BudgetSpendError::AlreadySpentSameDecision) => {
                        return Ok(EffectResult {
                            effect_id: decision_receipt_id.to_string(),
                            success: true,
                            message: format!(
                                "Budget spend already confirmed (idempotent, budget={})",
                                budget_id
                            ),
                            state_change_hash: None,
                            ledger_entry_id: None,
                            not_executed: false,
                        });
                    }
                    Err(e) => {
                        warn!(
                            budget_id = %budget_id,
                            error = %e,
                            "Budget spend rejected"
                        );
                        return Ok(EffectResult {
                            effect_id: decision_receipt_id.to_string(),
                            success: false,
                            message: e.to_string(),
                            state_change_hash: None,
                            ledger_entry_id: None,
                            not_executed: false,
                        });
                    }
                }

                // Phase 2: Ledger mutation
                let operation = treasury_effect_to_operation(&treasury_effect);
                match self
                    .treasury
                    .execute_treasury_operation(receipt_id, operation)
                    .await
                {
                    Ok(outcome) => {
                        let result =
                            execution_outcome_to_effect_result(outcome, decision_receipt_id);
                        if result.success {
                            // Phase 3: confirm_spend
                            budget.confirm_spend();
                            budget_store.put(&budget)?;
                            info!(
                                budget_id = %budget_id,
                                spent_total = budget.spent_total,
                                remaining = budget.remaining(),
                                "Budget spend confirmed"
                            );
                        }
                        // If !result.success, budget stays with pending_spend
                        // for crash recovery
                        Ok(result)
                    }
                    Err(e) => {
                        // Ledger error — budget stays in pending state
                        // for retry by DecisionExecutor
                        Err(e)
                    }
                }
            }

            // Surplus distribution: fan-out payment to N members in a single journal entry.
            //
            // Routes through treasury_effect_to_operation → TreasuryOperation::DistributeSurplus
            // → execute_treasury_operation → submit_treasury_entry (DistributeSurplus arm).
            // All N debit/credit deltas land in one JournalEntry so the one-receipt-one-entry
            // idempotency invariant is preserved across retries.
            TreasuryEffect::DistributeSurplus { .. } => {
                let operation = treasury_effect_to_operation(&treasury_effect);
                let outcome = self
                    .treasury
                    .execute_treasury_operation(receipt_id, operation)
                    .await?;
                Ok(execution_outcome_to_effect_result(
                    outcome,
                    decision_receipt_id,
                ))
            }

            // RedeemShares and IssueBond: not yet implemented in this executor.
            // Return an honest non-deferred failure rather than falling through to
            // `treasury_effect_to_operation`'s `_ =>` placeholder which produces
            // zeroed-out operation data and a misleading "missing provenance" Deferred.
            TreasuryEffect::RedeemShares { .. } => {
                warn!("RedeemShares received: not yet implemented in the kernel treasury executor");
                Ok(EffectResult {
                    effect_id: decision_receipt_id.to_string(),
                    success: false,
                    message: "RedeemShares: not yet implemented in the kernel treasury executor (no balances changed)".to_string(),
                    state_change_hash: None,
                    ledger_entry_id: None,
                    not_executed: false,
                })
            }

            TreasuryEffect::IssueBond { .. } => {
                warn!("IssueBond received: not yet implemented in the kernel treasury executor");
                Ok(EffectResult {
                    effect_id: decision_receipt_id.to_string(),
                    success: false,
                    message: "IssueBond: not yet implemented in the kernel treasury executor (no balances changed)".to_string(),
                    state_change_hash: None,
                    ledger_entry_id: None,
                    not_executed: false,
                })
            }

            // Path 3: All other treasury effects → direct delegation
            _ => {
                let operation = treasury_effect_to_operation(&treasury_effect);
                let outcome = self
                    .treasury
                    .execute_treasury_operation(receipt_id, operation)
                    .await?;
                Ok(execution_outcome_to_effect_result(
                    outcome,
                    decision_receipt_id,
                ))
            }
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

    fn federation(&self) -> Option<&dyn FederationExecutor> {
        Some(self.federation.as_ref())
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
            KernelEffect::Treasury(TreasuryEffect::ReleaseEscrow {
                ref escrow_id,
                ref decision_hash,
                ..
            }) => {
                // Domain-level idempotency via saga: Locked → Releasing → Released.
                // Releasing is persisted BEFORE ledger mutation so crash recovery
                // can detect the in-flight state and re-attempt.
                let escrow_store = match &self.escrow_store {
                    Some(store) => store,
                    None => {
                        warn!(
                            escrow_id = %escrow_id,
                            "Escrow store not configured — cannot release escrow"
                        );
                        return Ok(EffectResult {
                            effect_id: decision_receipt_id.to_string(),
                            success: false,
                            message: "Escrow store not configured".to_string(),
                            state_change_hash: None,
                            ledger_entry_id: None,
                            not_executed: false,
                        });
                    }
                };

                // Look up escrow
                let mut escrow = match escrow_store.get(escrow_id)? {
                    Some(e) => e,
                    None => {
                        warn!(
                            escrow_id = %escrow_id,
                            "Escrow not found"
                        );
                        return Ok(EffectResult {
                            effect_id: decision_receipt_id.to_string(),
                            success: false,
                            message: format!("Escrow not found: {}", escrow_id),
                            state_change_hash: None,
                            ledger_entry_id: None,
                            not_executed: false,
                        });
                    }
                };

                // Phase 1: Record intent to release (saga).
                // begin_release transitions Locked → Releasing, or signals retry/idempotent.
                match escrow.begin_release(decision_hash) {
                    Ok(BeginReleaseOutcome::Proceed) => {
                        // Fresh release — persist Releasing state BEFORE ledger mutation.
                        escrow_store.put(&escrow)?;
                        info!(
                            escrow_id = %escrow_id,
                            decision_hash = %decision_hash,
                            "Escrow entering Releasing state, proceeding to ledger mutation"
                        );
                    }
                    Ok(BeginReleaseOutcome::RetryNeeded) => {
                        // Crash recovery — escrow already in Releasing with same hash.
                        // Re-attempt the ledger mutation (skip redundant persist).
                        info!(
                            escrow_id = %escrow_id,
                            decision_hash = %decision_hash,
                            "Escrow in Releasing state (crash recovery), retrying ledger mutation"
                        );
                    }
                    Err(EscrowReleaseError::AlreadyReleasedSameDecision) => {
                        // Fully released — same decision already completed this escrow.
                        info!(
                            escrow_id = %escrow_id,
                            decision_hash = %decision_hash,
                            "Escrow already released by this decision (idempotent)"
                        );
                        return Ok(EffectResult {
                            effect_id: decision_receipt_id.to_string(),
                            success: true,
                            message: format!(
                                "Escrow {} already released by this decision (idempotent)",
                                escrow_id
                            ),
                            state_change_hash: None,
                            ledger_entry_id: None,
                            not_executed: false,
                        });
                    }
                    Err(EscrowReleaseError::AlreadyReleasedByOther {
                        existing_decision_hash,
                    }) => {
                        warn!(
                            escrow_id = %escrow_id,
                            decision_hash = %decision_hash,
                            existing_decision_hash = %existing_decision_hash,
                            "Escrow already released by a different decision — conflict"
                        );
                        return Ok(EffectResult {
                            effect_id: decision_receipt_id.to_string(),
                            success: false,
                            message: format!(
                                "Escrow {} already released by decision {}",
                                escrow_id, existing_decision_hash
                            ),
                            state_change_hash: None,
                            ledger_entry_id: None,
                            not_executed: false,
                        });
                    }
                    Err(EscrowReleaseError::Cancelled) => {
                        warn!(
                            escrow_id = %escrow_id,
                            "Cannot release cancelled escrow"
                        );
                        return Ok(EffectResult {
                            effect_id: decision_receipt_id.to_string(),
                            success: false,
                            message: format!("Escrow {} was cancelled", escrow_id),
                            state_change_hash: None,
                            ledger_entry_id: None,
                            not_executed: false,
                        });
                    }
                    Err(EscrowReleaseError::Failed {
                        existing_decision_hash,
                        reason,
                    }) => {
                        warn!(
                            escrow_id = %escrow_id,
                            decision_hash = %decision_hash,
                            existing_decision_hash = %existing_decision_hash,
                            reason = %reason,
                            "Cannot release terminally failed escrow"
                        );
                        return Ok(EffectResult {
                            effect_id: decision_receipt_id.to_string(),
                            success: false,
                            message: format!(
                                "Escrow {} terminally failed by decision {}: {}",
                                escrow_id, existing_decision_hash, reason
                            ),
                            state_change_hash: None,
                            ledger_entry_id: None,
                            not_executed: false,
                        });
                    }
                }

                // Phase 2: Execute ledger mutation (escrow is now in Releasing state).
                if let KernelEffect::Treasury(ref treasury_effect) = effect {
                    let operation = treasury_effect_to_operation(treasury_effect);
                    match self
                        .treasury
                        .execute_treasury_operation(&receipt_id, operation)
                        .await
                    {
                        Ok(outcome) => {
                            let result =
                                execution_outcome_to_effect_result(outcome, decision_receipt_id);
                            if result.success {
                                // Phase 3: Confirm release — ledger mutation succeeded.
                                // Transition Releasing → Released.
                                escrow.confirm_release();
                                escrow_store.put(&escrow)?;
                                info!(
                                    escrow_id = %escrow_id,
                                    "Escrow release confirmed (Released)"
                                );
                            } else {
                                // Treasury returned logical failure (e.g. insufficient funds).
                                // Escrow stays in Releasing — retry via same decision_hash.
                                warn!(
                                    escrow_id = %escrow_id,
                                    message = %result.message,
                                    "Treasury operation failed, escrow stays in Releasing for retry"
                                );
                            }
                            Ok(result)
                        }
                        Err(e) => {
                            // System error — escrow stays in Releasing for crash recovery.
                            warn!(
                                escrow_id = %escrow_id,
                                error = %e,
                                "Ledger mutation error, escrow stays in Releasing"
                            );
                            Err(e)
                        }
                    }
                } else {
                    unreachable!("already matched Treasury variant")
                }
            }
            KernelEffect::Treasury(ref treasury_effect) => {
                // Budget-aware treasury dispatch: handles CreateBudget, Spend with budget_id,
                // and all other treasury effects (direct passthrough).
                self.execute_treasury_with_budget(
                    treasury_effect.clone(),
                    &receipt_id,
                    decision_receipt_id,
                )
                .await
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
                    // PILOT V1 DECISION: Protocol::Upgrade and Protocol::SetSchedulingPolicy
                    // return explicit failure here because:
                    // 1. Upgrade requires migration coordination not implemented in pilot
                    // 2. SetSchedulingPolicy already works via EventBus side-channel
                    //    (see governance_policy_integration.rs test)
                    //
                    // This explicit failure prevents lying about success. The tripwire
                    // test documents these as expected failures.
                    tracing::warn!(
                        effect = ?protocol_effect,
                        "Protocol effect not supported via kernel path in pilot v1"
                    );
                    Ok(EffectResult {
                        effect_id: decision_receipt_id.to_string(),
                        success: false,
                        message: format!(
                            "Protocol effect not supported in pilot v1 (use EventBus for SchedulingPolicy): {:?}",
                            protocol_effect
                        ),
                        state_change_hash: None,
                        ledger_entry_id: None,
                        not_executed: false,
                    })
                }
            }
            KernelEffect::Federation(federation_effect) => {
                // Convert FederationEffect to FederationOperation
                let operation = federation_effect_to_operation(&federation_effect);
                let outcome = self
                    .federation
                    .execute_federation_operation(&receipt_id, operation)
                    .await?;
                Ok(execution_outcome_to_effect_result(
                    outcome,
                    decision_receipt_id,
                ))
            }
            KernelEffect::NoOp { reason } => {
                info!(
                    ?reason,
                    "KernelEffect::NoOp: no executable effect (honest non-success)"
                );
                Ok(EffectResult {
                    effect_id: decision_receipt_id.to_string(),
                    success: false,
                    message: format!(
                        "NoOp: decision produced no executable kernel effect (not executed): {}",
                        reason
                    ),
                    state_change_hash: None,
                    ledger_entry_id: None,
                    not_executed: true,
                })
            }
            KernelEffect::Control(control_effect) => {
                // Execute control effect (veto, force close)
                let outcome = self
                    .control
                    .execute_control_operation(&receipt_id, control_effect)
                    .await?;
                Ok(execution_outcome_to_effect_result(
                    outcome,
                    decision_receipt_id,
                ))
            }
            KernelEffect::Membership(membership_effect) => {
                // Execute membership effect (add, remove, update, freeze, unfreeze)
                let outcome = self
                    .membership
                    .execute_membership_operation(&receipt_id, membership_effect)
                    .await?;
                Ok(execution_outcome_to_effect_result(
                    outcome,
                    decision_receipt_id,
                ))
            }
            KernelEffect::Dispute(dispute_effect) => {
                info!(
                    ?dispute_effect,
                    "Executing dispute effect (not implemented in kernel executor)"
                );
                Ok(EffectResult {
                    effect_id: decision_receipt_id.to_string(),
                    success: false,
                    message: format!(
                        "Dispute effect not implemented in kernel executor: {:?}",
                        dispute_effect
                    ),
                    state_change_hash: None,
                    ledger_entry_id: None,
                    not_executed: false,
                })
            }
            KernelEffect::Resource(resource_effect) => {
                info!(
                    ?resource_effect,
                    "Executing resource effect (not implemented in kernel executor)"
                );
                Ok(EffectResult {
                    effect_id: decision_receipt_id.to_string(),
                    success: false,
                    message: format!(
                        "Resource effect not implemented in kernel executor: {:?}",
                        resource_effect
                    ),
                    state_change_hash: None,
                    ledger_entry_id: None,
                    not_executed: false,
                })
            }
            KernelEffect::Sdis(sdis_effect) => {
                info!(
                    ?sdis_effect,
                    "Executing SDIS effect (not implemented in kernel executor)"
                );
                Ok(EffectResult {
                    effect_id: decision_receipt_id.to_string(),
                    success: false,
                    message: format!(
                        "SDIS effect not implemented in kernel executor: {:?}",
                        sdis_effect
                    ),
                    state_change_hash: None,
                    ledger_entry_id: None,
                    not_executed: false,
                })
            }
        }
    }
}
/// Treasury executor implementation.
///
/// Executes treasury operations (spend, allocate, reserve, release) by
/// delegating to the kernel's ledger services.
pub struct KernelTreasuryExecutor {
    /// Ledger service for submitting treasury entries
    ledger: Option<Arc<dyn icn_kernel_api::LedgerService>>,
}

impl KernelTreasuryExecutor {
    /// Create a new treasury executor without ledger (placeholder mode).
    pub fn new() -> Self {
        Self { ledger: None }
    }

    /// Create a treasury executor with ledger service (production mode).
    pub fn with_ledger(ledger: Arc<dyn icn_kernel_api::LedgerService>) -> Self {
        Self {
            ledger: Some(ledger),
        }
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
        tracing::info!(
            receipt_id = %receipt_id,
            treasury_id = %operation.treasury_id,
            op_type = ?operation.operation_type,
            amount = operation.amount,
            currency = %operation.currency,
            recipient = ?operation.recipient,
            memo = %operation.memo,
            expected_nonce = ?operation.expected_nonce,
            decision_hash = ?operation.decision_hash,
            "Executing treasury operation"
        );

        let ledger = match &self.ledger {
            None => {
                return Ok(ExecutionOutcome::Deferred {
                    receipt_id: receipt_id.clone(),
                    reason: "Treasury ledger service is not configured; settlement entry cannot be applied yet"
                        .to_string(),
                });
            }
            Some(l) => l,
        };

        if let Some(decision_hash) = &operation.decision_hash {
            let request = icn_kernel_api::TreasuryEntryRequest {
                treasury_id: operation.treasury_id.clone(),
                operation_type: convert_operation_type(operation.operation_type),
                amount: operation.amount,
                currency: operation.currency.clone(),
                recipient: operation.recipient.clone(),
                memo: operation.memo.clone(),
                expected_nonce: operation.expected_nonce,
                distributions: operation.distributions.clone(),
                decision_receipt_id: receipt_id.to_string(),
                decision_hash: decision_hash.clone(),
            };

            match ledger.submit_treasury_entry(request) {
                Ok(result) => {
                    tracing::info!(
                        entry_hash = %result.entry_hash,
                        decision_receipt_id = %result.decision_receipt_id,
                        decision_hash = %result.decision_hash,
                        "Treasury entry submitted to ledger"
                    );
                    return Ok(ExecutionOutcome::Success {
                        receipt_id: receipt_id.clone(),
                        effects: vec![format!(
                            "{:?} {} {} from treasury {} -> state_hash={} {}{}",
                            operation.operation_type,
                            operation.amount,
                            operation.currency,
                            operation.treasury_id,
                            result.entry_hash,
                            LEDGER_ENTRY_MARKER,
                            result.entry_hash
                        )],
                    });
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to submit treasury entry to ledger");
                    return Err(anyhow::anyhow!("Ledger submission failed: {}", e));
                }
            }
        }

        // Ledger is configured but the operation lacks governance provenance. Pilot settlement
        // requires decision_hash on ledger entries; this is deferrable (not a hard ledger error),
        // distinct from the "no ledger service" case above.
        Ok(ExecutionOutcome::Deferred {
            receipt_id: receipt_id.clone(),
            reason: "Treasury operation lacks decision_hash; governed settlement deferred until provenance is present"
                .to_string(),
        })
    }

    async fn get_treasury_balance(&self, treasury_id: &str, currency: &str) -> Result<i64> {
        if let Some(ledger) = &self.ledger {
            let did = treasury_id.to_string();
            let balance = ledger.balance(&did, currency);
            tracing::debug!(
                treasury_id,
                currency,
                balance,
                "Queried treasury balance from ledger"
            );
            return Ok(balance);
        }
        tracing::debug!(
            treasury_id,
            currency,
            "Treasury balance query: no ledger wired, returning 0"
        );
        Ok(0)
    }
}

/// Convert from governance TreasuryOperationType to services TreasuryOperationType
fn convert_operation_type(
    op: icn_kernel_api::governance::TreasuryOperationType,
) -> icn_kernel_api::services::TreasuryOperationType {
    use icn_kernel_api::governance::TreasuryOperationType as GovOp;
    use icn_kernel_api::services::TreasuryOperationType as SvcOp;

    match op {
        GovOp::Spend => SvcOp::Spend,
        GovOp::Allocate => SvcOp::Allocate,
        GovOp::Reserve => SvcOp::Allocate, // Map Reserve to Allocate
        GovOp::Release => SvcOp::Transfer, // Map Release to Transfer
        GovOp::DistributeSurplus => SvcOp::DistributeSurplus,
    }
}

/// Protocol executor implementation.
///
/// Executes protocol parameter changes by delegating to the kernel's
/// protocol parameter store. Changes are durable (persisted to Sled)
/// and include provenance tracking via decision receipt IDs.
pub struct KernelProtocolExecutor {
    /// Protocol parameter store for reading/writing parameters
    param_store: Arc<dyn ProtocolParameterStore>,
}

impl KernelProtocolExecutor {
    /// Create a new protocol executor with the given parameter store.
    pub fn new(param_store: Arc<dyn ProtocolParameterStore>) -> Self {
        Self { param_store }
    }

    /// Compute a state change hash for a protocol parameter change.
    ///
    /// The hash includes: parameter name, old value, new value, receipt ID,
    /// and effective_at timestamp to provide a unique, verifiable fingerprint
    /// of the state transition.
    fn compute_state_change_hash(
        receipt_id: &DecisionReceiptId,
        change: &ProtocolChange,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"protocol:set_parameter:");
        hasher.update(change.parameter_name.as_bytes());
        hasher.update(b":");
        hasher.update(change.old_value.as_bytes());
        hasher.update(b"->");
        hasher.update(change.new_value.as_bytes());
        hasher.update(b":");
        hasher.update(receipt_id.to_string().as_bytes());
        hasher.update(b":");
        hasher.update(change.effective_at.to_le_bytes());
        format!("{:x}", hasher.finalize())
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

        // Apply the change to the parameter store (durable via Sled)
        self.param_store
            .set(updated_param, Some(receipt_id.to_string()), None)?;

        // Compute state change hash for provenance tracking
        let state_change_hash = Self::compute_state_change_hash(receipt_id, &change);

        tracing::info!(
            parameter = %change.parameter_name,
            new_value = %change.new_value,
            state_change_hash = %state_change_hash,
            "Protocol parameter updated (durable)"
        );

        Ok(ExecutionOutcome::Success {
            receipt_id: receipt_id.clone(),
            effects: vec![format!(
                "Changed {} from {} to {} -> state_hash={}",
                change.parameter_name, change.old_value, change.new_value, state_change_hash
            )],
        })
    }

    async fn get_parameter(&self, name: &str) -> Result<Option<String>> {
        tracing::debug!(name, "Querying protocol parameter");
        Ok(self.param_store.get(name)?.map(|p| p.value.to_string()))
    }
}

/// Federation executor implementation.
///
/// Executes federation operations (join, leave, vouch, clearing) by
/// delegating to the FederationService for durable state mutations.
pub struct KernelFederationExecutor {
    /// Federation service for durable state operations
    service: Option<Arc<dyn icn_kernel_api::FederationService>>,
}

impl KernelFederationExecutor {
    /// Create a new federation executor (placeholder mode - no durable state).
    pub fn new() -> Self {
        Self { service: None }
    }

    /// Create a new federation executor with a real service.
    pub fn with_service(service: Arc<dyn icn_kernel_api::FederationService>) -> Self {
        Self {
            service: Some(service),
        }
    }

    /// Set the federation service after construction.
    pub fn set_service(&mut self, service: Arc<dyn icn_kernel_api::FederationService>) {
        self.service = Some(service);
    }
}

impl Default for KernelFederationExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FederationExecutor for KernelFederationExecutor {
    async fn execute_federation_operation(
        &self,
        receipt_id: &DecisionReceiptId,
        operation: FederationOperation,
    ) -> Result<ExecutionOutcome> {
        use icn_kernel_api::governance::FederationOperationType;

        tracing::info!(
            receipt_id = %receipt_id,
            op_type = ?operation.operation_type,
            coop_did = %operation.coop_did,
            target_id = ?operation.target_id,
            has_service = self.service.is_some(),
            "Executing federation operation"
        );

        // If we have a real service, use it for durable state
        if let Some(ref service) = self.service {
            match operation.operation_type {
                FederationOperationType::JoinFederation => {
                    let request = icn_kernel_api::FederationJoinRequest {
                        coop_did: operation.coop_did.clone(),
                        coop_name: operation.coop_did.clone(), // Use DID as name for now
                        federation_id: operation.target_id.clone().unwrap_or_default(),
                        gateway_endpoints: vec![],
                        decision_receipt_id: receipt_id.to_string(),
                        decision_hash: operation.decision_hash.clone().unwrap_or_default(),
                    };

                    let result = service.join_federation(request)?;

                    if result.success {
                        tracing::info!(
                            state_change_hash = %result.state_change_hash,
                            "Federation join completed with durable state"
                        );
                        Ok(ExecutionOutcome::Success {
                            receipt_id: receipt_id.clone(),
                            effects: vec![format!(
                                "Coop {} joined federation {} -> state_hash={}",
                                operation.coop_did,
                                operation.target_id.unwrap_or_default(),
                                result.state_change_hash
                            )],
                        })
                    } else {
                        Ok(ExecutionOutcome::Failed {
                            receipt_id: receipt_id.clone(),
                            reason: result.error.unwrap_or_else(|| "Unknown error".to_string()),
                        })
                    }
                }
                FederationOperationType::VouchForCoop => {
                    let request = icn_kernel_api::FederationVouchRequest {
                        voucher_did: operation.coop_did.clone(),
                        vouchee_did: operation.target_id.clone().unwrap_or_default(),
                        trust_score: 0.7, // Default trust score for now
                        decision_receipt_id: receipt_id.to_string(),
                        decision_hash: operation.decision_hash.clone().unwrap_or_default(),
                    };

                    let result = service.vouch_for_cooperative(request)?;

                    if result.success {
                        tracing::info!(
                            state_change_hash = %result.state_change_hash,
                            "Federation vouch completed with durable state"
                        );
                        Ok(ExecutionOutcome::Success {
                            receipt_id: receipt_id.clone(),
                            effects: vec![format!(
                                "Coop {} vouched for {} -> state_hash={}",
                                operation.coop_did,
                                operation.target_id.unwrap_or_default(),
                                result.state_change_hash
                            )],
                        })
                    } else {
                        Ok(ExecutionOutcome::Failed {
                            receipt_id: receipt_id.clone(),
                            reason: result.error.unwrap_or_else(|| "Unknown error".to_string()),
                        })
                    }
                }
                FederationOperationType::EstablishClearing => {
                    let coop_b_did = match operation.target_id.as_deref().filter(|s| !s.is_empty())
                    {
                        Some(did) => did.to_string(),
                        None => {
                            return Ok(ExecutionOutcome::Failed {
                                receipt_id: receipt_id.clone(),
                                reason:
                                    "EstablishClearing: missing or empty target_id (coop_b_did)"
                                        .to_string(),
                            });
                        }
                    };
                    let agreement_id = match operation
                        .agreement_hash
                        .as_deref()
                        .filter(|s| !s.is_empty())
                    {
                        Some(id) => id.to_string(),
                        None => {
                            return Ok(ExecutionOutcome::Failed {
                                receipt_id: receipt_id.clone(),
                                reason: "EstablishClearing: missing or empty agreement_hash (agreement_id)".to_string(),
                            });
                        }
                    };
                    let log_coop_b = coop_b_did.clone();
                    let request = icn_kernel_api::FederationClearingRequest {
                        coop_a_did: operation.coop_did.clone(),
                        coop_b_did,
                        agreement_id,
                        decision_receipt_id: receipt_id.to_string(),
                        decision_hash: operation.decision_hash.clone().unwrap_or_default(),
                    };

                    let result = service.establish_clearing(request)?;

                    if result.success {
                        tracing::info!(
                            state_change_hash = %result.state_change_hash,
                            "Bilateral clearing agreement established with durable state"
                        );
                        Ok(ExecutionOutcome::Success {
                            receipt_id: receipt_id.clone(),
                            effects: vec![format!(
                                "Clearing established: {} <-> {} -> state_hash={}",
                                operation.coop_did, log_coop_b, result.state_change_hash
                            )],
                        })
                    } else {
                        Ok(ExecutionOutcome::Failed {
                            receipt_id: receipt_id.clone(),
                            reason: result.error.unwrap_or_else(|| "Unknown error".to_string()),
                        })
                    }
                }
                // LeaveFederation not yet wired to service
                FederationOperationType::LeaveFederation => {
                    tracing::warn!("LeaveFederation not yet implemented with durable state");
                    Ok(ExecutionOutcome::Failed {
                        receipt_id: receipt_id.clone(),
                        reason: "LeaveFederation not yet implemented".to_string(),
                    })
                }
            }
        } else {
            // No service configured - return failure instead of lying
            tracing::warn!(
                "Federation executor has no service configured - cannot execute durable operation"
            );
            Ok(ExecutionOutcome::Failed {
                receipt_id: receipt_id.clone(),
                reason: "Federation service not configured".to_string(),
            })
        }
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

/// Convert an ExecutionOutcome to an EffectResult.
///
/// Extracts state_change_hash from effects if present (embedded as `-> state_hash=XXX`
/// by Protocol and Federation executors).
fn execution_outcome_to_effect_result(outcome: ExecutionOutcome, effect_id: &str) -> EffectResult {
    match outcome {
        ExecutionOutcome::Success { effects, .. } => {
            // Extract state_change_hash from effects if present
            // Format: "... -> state_hash=<hash>"
            let state_change_hash = effects.iter().find_map(|e| {
                e.find("-> state_hash=").map(|pos| {
                    e[pos + 14..]
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_string() // Skip "-> state_hash="
                })
            });
            let ledger_entry_id = effects.iter().find_map(|e| {
                let (_, entry_id) = e.rsplit_once(LEDGER_ENTRY_MARKER)?;
                let entry_id = entry_id.split_whitespace().next()?;
                if entry_id.is_empty() {
                    None
                } else {
                    Some(entry_id.to_string())
                }
            });

            EffectResult {
                effect_id: effect_id.to_string(),
                success: true,
                message: effects.join("; "),
                state_change_hash,
                ledger_entry_id,
                not_executed: false,
            }
        }
        ExecutionOutcome::Failed { reason, .. } => EffectResult {
            effect_id: effect_id.to_string(),
            success: false,
            message: reason,
            state_change_hash: None,
            ledger_entry_id: None,
            not_executed: false,
        },
        ExecutionOutcome::Deferred { reason, .. } => EffectResult {
            effect_id: effect_id.to_string(),
            success: false,
            message: format!("Deferred: {}", reason),
            state_change_hash: None,
            ledger_entry_id: None,
            not_executed: true,
        },
    }
}

// ============================================================================
// Control Executor
// ============================================================================

/// Control executor implementation.
///
/// Executes governance control operations (veto, force close) by
/// delegating to the kernel's control service.
pub struct KernelControlExecutor {
    /// Control service for executing veto/force close
    service: Option<Arc<dyn ControlService>>,
}

impl KernelControlExecutor {
    /// Create a new control executor (placeholder mode - no service).
    pub fn new() -> Self {
        Self { service: None }
    }

    /// Create a new control executor with a real service.
    pub fn with_service(service: Arc<dyn ControlService>) -> Self {
        Self {
            service: Some(service),
        }
    }

    /// Execute a control operation.
    pub async fn execute_control_operation(
        &self,
        receipt_id: &DecisionReceiptId,
        effect: ControlEffect,
    ) -> Result<ExecutionOutcome> {
        info!(
            receipt_id = %receipt_id,
            effect = ?effect,
            has_service = self.service.is_some(),
            "Executing control operation"
        );

        // TextResolution is a no-op - it doesn't need a service
        // Handle it before the service check
        if let ControlEffect::TextResolution { resolution_hash } = &effect {
            info!(
                resolution_hash = %resolution_hash,
                "Text resolution recorded (no state change)"
            );
            return Ok(ExecutionOutcome::Success {
                receipt_id: receipt_id.clone(),
                effects: vec![format!("Text resolution: {}", resolution_hash)],
            });
        }

        // Other control effects require a service
        if let Some(ref service) = self.service {
            match effect {
                ControlEffect::VetoProposal {
                    target_proposal_id,
                    veto_reason,
                } => {
                    let request = VetoProposalRequest {
                        target_proposal_id: target_proposal_id.clone(),
                        veto_reason,
                        domain_id: String::new(), // Not used in current impl
                        vetoer_did: String::new(), // Not used in current impl
                        decision_receipt_id: receipt_id.to_string(),
                        decision_hash: String::new(), // Will be computed from receipt
                    };

                    let result = service.veto_proposal(request)?;

                    if result.success {
                        info!(
                            target_proposal = %target_proposal_id,
                            state_change_hash = ?result.state_change_hash,
                            "Veto completed successfully"
                        );
                        Ok(ExecutionOutcome::Success {
                            receipt_id: receipt_id.clone(),
                            effects: vec![format!(
                                "Vetoed proposal {} -> state_hash={}",
                                target_proposal_id,
                                result.state_change_hash.unwrap_or_default()
                            )],
                        })
                    } else {
                        warn!(
                            target_proposal = %target_proposal_id,
                            message = %result.message,
                            "Veto failed"
                        );
                        Ok(ExecutionOutcome::Failed {
                            receipt_id: receipt_id.clone(),
                            reason: result.message,
                        })
                    }
                }
                ControlEffect::ForceCloseProposal {
                    target_proposal_id,
                    close_reason,
                } => {
                    let request = ForceCloseProposalRequest {
                        target_proposal_id: target_proposal_id.clone(),
                        close_reason,
                        forced_outcome: "reject".to_string(), // Default outcome
                        domain_id: String::new(),
                        closer_did: String::new(),
                        decision_receipt_id: receipt_id.to_string(),
                        decision_hash: String::new(),
                    };

                    let result = service.force_close_proposal(request)?;

                    if result.success {
                        info!(
                            target_proposal = %target_proposal_id,
                            state_change_hash = ?result.state_change_hash,
                            "Force close completed successfully"
                        );
                        Ok(ExecutionOutcome::Success {
                            receipt_id: receipt_id.clone(),
                            effects: vec![format!(
                                "Force closed proposal {} as {} -> state_hash={}",
                                target_proposal_id,
                                result.final_outcome,
                                result.state_change_hash.unwrap_or_default()
                            )],
                        })
                    } else {
                        warn!(
                            target_proposal = %target_proposal_id,
                            message = %result.message,
                            "Force close failed"
                        );
                        Ok(ExecutionOutcome::Failed {
                            receipt_id: receipt_id.clone(),
                            reason: result.message,
                        })
                    }
                }
                // TextResolution is handled above, before the service check
                // This arm should never be reached
                ControlEffect::TextResolution { .. } => unreachable!(),
            }
        } else {
            // Placeholder mode: log and return failure
            warn!("Control executor has no service configured - cannot execute control operation");
            Ok(ExecutionOutcome::Failed {
                receipt_id: receipt_id.clone(),
                reason: "Control service not configured".to_string(),
            })
        }
    }
}

impl Default for KernelControlExecutor {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Membership Executor
// =============================================================================

/// Kernel executor for membership operations.
///
/// This executor translates [`MembershipEffect`] into calls to the [`MembershipService`].
/// If no service is configured, operations will fail (no silent success).
pub struct KernelMembershipExecutor {
    /// Membership service for durable state operations
    service: Option<Arc<dyn icn_kernel_api::MembershipService>>,
}

impl KernelMembershipExecutor {
    /// Create a new membership executor (placeholder mode - no durable state).
    pub fn new() -> Self {
        Self { service: None }
    }

    /// Create a new membership executor with a real service.
    pub fn with_service(service: Arc<dyn icn_kernel_api::MembershipService>) -> Self {
        Self {
            service: Some(service),
        }
    }

    /// Set the membership service after construction.
    pub fn set_service(&mut self, service: Arc<dyn icn_kernel_api::MembershipService>) {
        self.service = Some(service);
    }

    /// Execute a membership operation.
    pub async fn execute_membership_operation(
        &self,
        receipt_id: &DecisionReceiptId,
        effect: MembershipEffect,
    ) -> Result<ExecutionOutcome> {
        tracing::info!(
            receipt_id = %receipt_id,
            effect = ?effect,
            has_service = self.service.is_some(),
            "Executing membership operation"
        );

        // If we have a real service, use it for durable state
        if let Some(ref service) = self.service {
            match effect {
                MembershipEffect::AddMember {
                    entity_id,
                    member_did,
                    role,
                    tier,
                } => {
                    let request = icn_kernel_api::AddMemberRequest {
                        entity_id: entity_id.clone(),
                        member_did: member_did.clone(),
                        role,
                        tier,
                        decision_receipt_id: receipt_id.to_string(),
                        decision_hash: compute_decision_hash(receipt_id),
                    };

                    let result = service.add_member(request)?;

                    if result.success {
                        tracing::info!(
                            state_change_hash = %result.state_change_hash,
                            "Member added with durable state"
                        );
                        Ok(ExecutionOutcome::Success {
                            receipt_id: receipt_id.clone(),
                            effects: vec![format!(
                                "Added member {} to {} -> state_hash={}",
                                member_did, entity_id, result.state_change_hash
                            )],
                        })
                    } else {
                        Ok(ExecutionOutcome::Failed {
                            receipt_id: receipt_id.clone(),
                            reason: result.error.unwrap_or_else(|| "Unknown error".to_string()),
                        })
                    }
                }
                MembershipEffect::RemoveMember {
                    entity_id,
                    member_did,
                    reason,
                } => {
                    let request = icn_kernel_api::RemoveMemberRequest {
                        entity_id: entity_id.clone(),
                        member_did: member_did.clone(),
                        reason,
                        decision_receipt_id: receipt_id.to_string(),
                        decision_hash: compute_decision_hash(receipt_id),
                    };

                    let result = service.remove_member(request)?;

                    if result.success {
                        tracing::info!(
                            state_change_hash = %result.state_change_hash,
                            "Member removed with durable state"
                        );
                        Ok(ExecutionOutcome::Success {
                            receipt_id: receipt_id.clone(),
                            effects: vec![format!(
                                "Removed member {} from {} -> state_hash={}",
                                member_did, entity_id, result.state_change_hash
                            )],
                        })
                    } else {
                        Ok(ExecutionOutcome::Failed {
                            receipt_id: receipt_id.clone(),
                            reason: result.error.unwrap_or_else(|| "Unknown error".to_string()),
                        })
                    }
                }
                MembershipEffect::UpdateMember {
                    entity_id,
                    member_did,
                    new_role,
                    new_tier,
                } => {
                    let request = icn_kernel_api::UpdateMemberRequest {
                        entity_id: entity_id.clone(),
                        member_did: member_did.clone(),
                        new_role,
                        new_tier,
                        decision_receipt_id: receipt_id.to_string(),
                        decision_hash: compute_decision_hash(receipt_id),
                    };

                    let result = service.update_member(request)?;

                    if result.success {
                        tracing::info!(
                            state_change_hash = %result.state_change_hash,
                            "Member updated with durable state"
                        );
                        Ok(ExecutionOutcome::Success {
                            receipt_id: receipt_id.clone(),
                            effects: vec![format!(
                                "Updated member {} in {} -> state_hash={}",
                                member_did, entity_id, result.state_change_hash
                            )],
                        })
                    } else {
                        Ok(ExecutionOutcome::Failed {
                            receipt_id: receipt_id.clone(),
                            reason: result.error.unwrap_or_else(|| "Unknown error".to_string()),
                        })
                    }
                }
                MembershipEffect::FreezeMember {
                    entity_id,
                    member_did,
                    reason,
                    duration_secs,
                } => {
                    let request = icn_kernel_api::FreezeMemberRequest {
                        entity_id: entity_id.clone(),
                        member_did: member_did.clone(),
                        reason,
                        duration_secs,
                        decision_receipt_id: receipt_id.to_string(),
                        decision_hash: compute_decision_hash(receipt_id),
                    };

                    let result = service.freeze_member(request)?;

                    if result.success {
                        tracing::info!(
                            state_change_hash = %result.state_change_hash,
                            expires_at = ?result.expires_at,
                            "Member frozen with durable state"
                        );
                        Ok(ExecutionOutcome::Success {
                            receipt_id: receipt_id.clone(),
                            effects: vec![format!(
                                "Froze member {} in {} -> state_hash={}",
                                member_did, entity_id, result.state_change_hash
                            )],
                        })
                    } else {
                        Ok(ExecutionOutcome::Failed {
                            receipt_id: receipt_id.clone(),
                            reason: result.error.unwrap_or_else(|| "Unknown error".to_string()),
                        })
                    }
                }
                MembershipEffect::UnfreezeMember {
                    entity_id,
                    member_did,
                } => {
                    let request = icn_kernel_api::UnfreezeMemberRequest {
                        entity_id: entity_id.clone(),
                        member_did: member_did.clone(),
                        decision_receipt_id: receipt_id.to_string(),
                        decision_hash: compute_decision_hash(receipt_id),
                    };

                    let result = service.unfreeze_member(request)?;

                    if result.success {
                        tracing::info!(
                            state_change_hash = %result.state_change_hash,
                            "Member unfrozen with durable state"
                        );
                        Ok(ExecutionOutcome::Success {
                            receipt_id: receipt_id.clone(),
                            effects: vec![format!(
                                "Unfroze member {} in {} -> state_hash={}",
                                member_did, entity_id, result.state_change_hash
                            )],
                        })
                    } else {
                        Ok(ExecutionOutcome::Failed {
                            receipt_id: receipt_id.clone(),
                            reason: result.error.unwrap_or_else(|| "Unknown error".to_string()),
                        })
                    }
                }
            }
        } else {
            // No service configured - return failure instead of lying
            tracing::warn!(
                "Membership executor has no service configured - cannot execute durable operation"
            );
            Ok(ExecutionOutcome::Failed {
                receipt_id: receipt_id.clone(),
                reason: "Membership service not configured".to_string(),
            })
        }
    }
}

impl Default for KernelMembershipExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute a decision hash from the receipt ID (placeholder implementation).
fn compute_decision_hash(receipt_id: &DecisionReceiptId) -> String {
    let mut hasher = Sha256::new();
    hasher.update(receipt_id.to_string().as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
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
    async fn test_treasury_executor_defers_without_ledger() {
        let executor = KernelTreasuryExecutor::new();
        let receipt_id = DecisionReceiptId::new("test-receipt-1");
        let operation = TreasuryOperation {
            treasury_id: "treasury-1".to_string(),
            operation_type: TreasuryOperationType::Spend,
            amount: 100,
            currency: "HOURS".to_string(),
            recipient: Some("did:icn:recipient".to_string()),
            memo: "Test payment".to_string(),
            expected_nonce: Some(0),
            decision_hash: Some("sha256:abc123".to_string()),
            distributions: Vec::new(),
        };

        let result = executor
            .execute_treasury_operation(&receipt_id, operation)
            .await;
        assert!(result.is_ok());

        match result.unwrap() {
            ExecutionOutcome::Deferred { reason, .. } => {
                assert!(
                    reason.contains("ledger"),
                    "expected defer reason to mention ledger, got {reason}"
                );
            }
            other => panic!("Expected Deferred without ledger, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_treasury_executor_defers_without_decision_hash_when_ledger_configured() {
        struct StubLedger {
            oracle: Arc<dyn icn_kernel_api::authz::PolicyOracle>,
        }
        impl icn_kernel_api::LedgerService for StubLedger {
            fn oracle(&self) -> Arc<dyn icn_kernel_api::authz::PolicyOracle> {
                self.oracle.clone()
            }
            fn balance(&self, _account: &icn_kernel_api::Did, _currency: &str) -> i64 {
                0
            }
            fn credit_limit(&self, _account: &icn_kernel_api::Did, _currency: &str) -> i64 {
                0
            }
            fn record_event(&self, _event: icn_kernel_api::LedgerEvent) {}
            fn submit_treasury_entry(
                &self,
                _entry: icn_kernel_api::TreasuryEntryRequest,
            ) -> Result<icn_kernel_api::TreasuryEntryResult, String> {
                panic!("submit_treasury_entry must not be called when decision_hash is missing");
            }
            fn get_treasury_nonce(&self, _treasury_id: &str) -> Result<u64, String> {
                Ok(0)
            }
        }

        let oracle = Arc::new(icn_kernel_api::authz::AllowAllOracle::wildcard());
        let ledger: Arc<dyn icn_kernel_api::LedgerService> = Arc::new(StubLedger {
            oracle: oracle.clone(),
        });
        let executor = KernelTreasuryExecutor::with_ledger(ledger);
        let receipt_id = DecisionReceiptId::new("test-receipt-provenance-defer");
        let operation = TreasuryOperation {
            treasury_id: "treasury-1".to_string(),
            operation_type: TreasuryOperationType::Spend,
            amount: 100,
            currency: "HOURS".to_string(),
            recipient: Some("did:icn:recipient".to_string()),
            memo: "Test payment".to_string(),
            expected_nonce: Some(0),
            decision_hash: None,
            distributions: Vec::new(),
        };

        let result = executor
            .execute_treasury_operation(&receipt_id, operation)
            .await;
        assert!(result.is_ok());

        match result.unwrap() {
            ExecutionOutcome::Deferred { reason, .. } => {
                assert!(
                    reason.contains("decision_hash"),
                    "expected defer reason to mention decision_hash, got {reason}"
                );
            }
            other => panic!("Expected Deferred without decision_hash, got {:?}", other),
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
    async fn test_treasury_executor_get_balance_no_ledger() {
        // Without a ledger wired, the balance falls back to 0.
        let executor = KernelTreasuryExecutor::new();
        let result = executor.get_treasury_balance("treasury-1", "HOURS").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_treasury_executor_get_balance_with_ledger() {
        // With a ledger wired, balance is read from the ledger service.
        struct FixedLedger {
            oracle: Arc<dyn icn_kernel_api::authz::PolicyOracle>,
        }
        impl icn_kernel_api::LedgerService for FixedLedger {
            fn oracle(&self) -> Arc<dyn icn_kernel_api::authz::PolicyOracle> {
                self.oracle.clone()
            }
            fn balance(&self, _account: &icn_kernel_api::Did, currency: &str) -> i64 {
                if currency == "HOURS" {
                    42
                } else {
                    0
                }
            }
            fn credit_limit(&self, _account: &icn_kernel_api::Did, _currency: &str) -> i64 {
                100
            }
            fn record_event(&self, _event: icn_kernel_api::LedgerEvent) {}
            fn submit_treasury_entry(
                &self,
                _entry: icn_kernel_api::TreasuryEntryRequest,
            ) -> Result<icn_kernel_api::TreasuryEntryResult, String> {
                panic!("not called in this test")
            }
            fn get_treasury_nonce(&self, _treasury_id: &str) -> Result<u64, String> {
                Ok(0)
            }
        }

        let oracle = Arc::new(icn_kernel_api::authz::AllowAllOracle::wildcard());
        let ledger: Arc<dyn icn_kernel_api::LedgerService> = Arc::new(FixedLedger {
            oracle: oracle.clone(),
        });
        let executor = KernelTreasuryExecutor::with_ledger(ledger);

        let result = executor
            .get_treasury_balance("did:icn:treasury-1", "HOURS")
            .await;
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            42,
            "Should return ledger balance when wired"
        );

        let result_other = executor
            .get_treasury_balance("did:icn:treasury-1", "USD")
            .await;
        assert_eq!(result_other.unwrap(), 0, "Unknown currency returns 0");
    }

    #[test]
    fn test_execution_outcome_maps_ledger_entry_id() {
        let outcome = ExecutionOutcome::Success {
            receipt_id: DecisionReceiptId::new("receipt-1"),
            effects: vec![format!(
                "Spend 10 HOURS from treasury did:icn:t1 {}abc123",
                LEDGER_ENTRY_MARKER
            )],
        };

        let result = execution_outcome_to_effect_result(outcome, "receipt-1");
        assert_eq!(result.ledger_entry_id.as_deref(), Some("abc123"));
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

    // =========================================================================
    // Pilot V1 Protocol Effect Handling Tests
    // =========================================================================

    #[tokio::test]
    async fn test_protocol_upgrade_returns_explicit_failure() {
        // PILOT V1 DECISION: Protocol::Upgrade is not supported via kernel effect path.
        // This test asserts the expected failure behavior.
        use icn_kernel_api::effects::ProtocolEffect;

        let store = Arc::new(MockParamStore::new());
        let executor = KernelGovernanceExecutor::new(store);
        let receipt_id = "test-upgrade-receipt";

        let effect = KernelEffect::Protocol(ProtocolEffect::Upgrade {
            version: "2.0.0".to_string(),
            upgrade_hash: "sha256:abc123".to_string(),
            activation_height: 1000,
        });

        let result = executor.execute_effect(effect, receipt_id).await;
        assert!(result.is_ok());

        let effect_result = result.unwrap();
        assert!(
            !effect_result.success,
            "Protocol::Upgrade should fail in pilot v1"
        );
        assert!(
            effect_result.message.contains("not supported in pilot v1"),
            "Failure message should explain pilot limitation: {}",
            effect_result.message
        );
    }

    #[tokio::test]
    async fn test_protocol_set_scheduling_policy_returns_explicit_failure() {
        // PILOT V1 DECISION: Protocol::SetSchedulingPolicy is not supported via kernel
        // effect path. SchedulingPolicy proposals work via EventBus side-channel instead.
        // See governance_policy_integration.rs for the working flow.
        use icn_kernel_api::effects::ProtocolEffect;

        let store = Arc::new(MockParamStore::new());
        let executor = KernelGovernanceExecutor::new(store);
        let receipt_id = "test-scheduling-policy-receipt";

        let effect = KernelEffect::Protocol(ProtocolEffect::SetSchedulingPolicy {
            coop_id: "test-coop".to_string(),
            policy_hash: "sha256:policy123".to_string(),
            policy_json: r#"{"coop_id":"test-coop"}"#.to_string(),
        });

        let result = executor.execute_effect(effect, receipt_id).await;
        assert!(result.is_ok());

        let effect_result = result.unwrap();
        assert!(
            !effect_result.success,
            "Protocol::SetSchedulingPolicy should fail via kernel path in pilot v1"
        );
        assert!(
            effect_result.message.contains("not supported in pilot v1"),
            "Failure message should explain pilot limitation: {}",
            effect_result.message
        );
        assert!(
            effect_result.message.contains("EventBus"),
            "Failure message should point to EventBus alternative: {}",
            effect_result.message
        );
    }

    #[tokio::test]
    async fn test_dispute_effect_returns_explicit_failure() {
        use icn_kernel_api::effects::{DisputeEffect, KernelEffect};

        let store = Arc::new(MockParamStore::new());
        let executor = KernelGovernanceExecutor::new(store);
        let receipt_id = "test-dispute-receipt";

        let effect = KernelEffect::Dispute(DisputeEffect::ResolveDispute {
            dispute_id: "dispute-1".to_string(),
            resolution_hash: "resolution-hash".to_string(),
            compensations: vec![],
        });

        let result = executor.execute_effect(effect, receipt_id).await;
        assert!(result.is_ok());

        let effect_result = result.unwrap();
        assert!(
            !effect_result.success,
            "Dispute effect must not report success when kernel executor has no implementation"
        );
        assert!(
            effect_result
                .message
                .contains("Dispute effect not implemented in kernel executor"),
            "Failure message should explain missing kernel implementation: {}",
            effect_result.message
        );
    }

    #[tokio::test]
    async fn test_resource_effect_returns_explicit_failure() {
        use icn_kernel_api::effects::{KernelEffect, ResourceEffect};

        let store = Arc::new(MockParamStore::new());
        let executor = KernelGovernanceExecutor::new(store);
        let receipt_id = "test-resource-receipt";

        let effect = KernelEffect::Resource(ResourceEffect::GrantAccess {
            grantee_did: "did:icn:grantee".to_string(),
            resource_type: "resource-type".to_string(),
            access_model_hash: "model-hash".to_string(),
        });

        let result = executor.execute_effect(effect, receipt_id).await;
        assert!(result.is_ok());

        let effect_result = result.unwrap();
        assert!(
            !effect_result.success,
            "Resource effect must not report success when kernel executor has no implementation"
        );
        assert!(
            effect_result
                .message
                .contains("Resource effect not implemented in kernel executor"),
            "Failure message should explain missing kernel implementation: {}",
            effect_result.message
        );
    }

    #[tokio::test]
    async fn test_sdis_effect_returns_explicit_failure() {
        use icn_kernel_api::effects::{KernelEffect, SdisEffect};

        let store = Arc::new(MockParamStore::new());
        let executor = KernelGovernanceExecutor::new(store);
        let receipt_id = "test-sdis-receipt";

        let effect = KernelEffect::Sdis(SdisEffect::ApproveSteward {
            steward_did: "did:icn:steward".to_string(),
            capabilities_hash: "capabilities-hash".to_string(),
        });

        let result = executor.execute_effect(effect, receipt_id).await;
        assert!(result.is_ok());

        let effect_result = result.unwrap();
        assert!(
            !effect_result.success,
            "SDIS effect must not report success when kernel executor has no implementation"
        );
        assert!(
            effect_result
                .message
                .contains("SDIS effect not implemented in kernel executor"),
            "Failure message should explain missing kernel implementation: {}",
            effect_result.message
        );
    }

    #[tokio::test]
    async fn test_distribute_surplus_reports_not_executed_without_ledger_smear() {
        use icn_kernel_api::effects::{KernelEffect, TreasuryEffect};

        let store = Arc::new(MockParamStore::new());
        let executor = KernelGovernanceExecutor::new(store);
        let receipt_id = "test-surplus-receipt";

        let effect = KernelEffect::Treasury(TreasuryEffect::DistributeSurplus {
            treasury_did: "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9".to_string(),
            total_amount: 100,
            currency: "HOURS".to_string(),
            distributions: vec![(
                "did:icn:zGyGKxMyg1p9SsHfm15MkNUu1u9TN2JtTspcdmrtGUdse".to_string(),
                100,
            )],
            decision_receipt_id: receipt_id.to_string(),
            decision_hash: "hash-surplus-test".to_string(),
        });

        let result = executor.execute_effect(effect, receipt_id).await;
        assert!(result.is_ok());
        let effect_result = result.unwrap();
        assert!(
            !effect_result.success,
            "DistributeSurplus must not report success without real multi-recipient settlement"
        );
        assert!(
            effect_result.not_executed,
            "DistributeSurplus without ledger service must defer (not_executed=true): {}",
            effect_result.message
        );
        assert!(
            effect_result.message.starts_with("Deferred:"),
            "message must be a Deferred reason when ledger service is not configured: {}",
            effect_result.message
        );
    }

    #[tokio::test]
    async fn test_noop_reports_not_executed() {
        use icn_kernel_api::effects::KernelEffect;

        let store = Arc::new(MockParamStore::new());
        let executor = KernelGovernanceExecutor::new(store);
        let receipt_id = "test-noop-receipt";

        let effect = KernelEffect::NoOp {
            reason: "translation produced no executable effect".to_string(),
        };

        let result = executor.execute_effect(effect, receipt_id).await;
        assert!(result.is_ok());

        let effect_result = result.unwrap();
        assert!(
            !effect_result.success,
            "NoOp must not report success when no kernel effect was executed"
        );
        assert!(effect_result.not_executed);
        assert!(
            effect_result
                .message
                .contains("no executable kernel effect (not executed)"),
            "NoOp message should state not executed: {}",
            effect_result.message
        );
    }
}
