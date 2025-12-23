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
use icn_ledger::DisputeManager;
use icn_store::SledStore;

/// Type alias for the ledger handle
pub type LedgerHandle = Arc<RwLock<icn_ledger::Ledger>>;

/// Type alias for the dead-letter queue
pub type DeadLetterQueue = Arc<crate::dead_letter::DeadLetterQueue<SledStore>>;

/// Type alias for the audit store
pub type AuditStore = Arc<dyn icn_store::Store>;

/// Type alias for the dispute manager handle
pub type DisputeManagerHandle = Arc<RwLock<DisputeManager>>;

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
    /// Treasury DID for budget payouts
    treasury_did: Did,
}

impl GovernanceEventHandler {
    /// Create a new governance event handler
    pub fn new(
        ledger: LedgerHandle,
        audit_store: AuditStore,
        dlq: DeadLetterQueue,
        gov_handle: GovernanceHandle,
        dispute_manager: DisputeManagerHandle,
        treasury_did: Did,
    ) -> Self {
        Self {
            ledger,
            audit_store,
            dlq,
            gov_handle,
            dispute_manager,
            treasury_did,
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
                info!(
                    "📋 Treasury spending rule {} for {}: threshold={}, active={}",
                    rule_id.as_deref().unwrap_or("new"),
                    treasury_did,
                    threshold_amount,
                    is_active
                );
                // PHASE 2: Wire TreasuryManager to modify spending rules.
                // Requires passing TreasuryManager handle to GovernanceEventHandler.
                // See: https://github.com/InterCooperative-Network/icn/issues/246
                warn!("⚠️ ModifySpendingRule execution not yet implemented (Phase 2)");
                let _ = (name, currency, approval_type);
            }
            TreasuryProposalOperation::TransferBetweenBudgets {
                treasury_did,
                from_budget,
                to_budget,
                amount,
                currency,
                reason,
            } => {
                info!(
                    "📋 Treasury budget transfer for {}: {} {} from {} to {} ({})",
                    treasury_did, amount, currency, from_budget, to_budget, reason
                );
                // PHASE 2: Wire TreasuryManager to transfer between budgets.
                // Requires passing TreasuryManager handle to GovernanceEventHandler.
                // See: https://github.com/InterCooperative-Network/icn/issues/246
                warn!("⚠️ TransferBetweenBudgets execution not yet implemented (Phase 2)");
            }
            TreasuryProposalOperation::CancelBudget {
                budget_id,
                reason,
                return_to_treasury,
            } => {
                info!(
                    "📋 Treasury budget cancelled: {} (reason: {}, return: {})",
                    budget_id, reason, return_to_treasury
                );
                // PHASE 2: Wire TreasuryManager to cancel budgets.
                // Requires passing TreasuryManager handle to GovernanceEventHandler.
                // See: https://github.com/InterCooperative-Network/icn/issues/246
                warn!("⚠️ CancelBudget execution not yet implemented (Phase 2)");
            }
            TreasuryProposalOperation::ReclaimBudget {
                budget_id,
                amount,
                currency,
                reason,
            } => {
                info!(
                    "📋 Treasury budget reclaim: {} {} from {} ({})",
                    amount, currency, budget_id, reason
                );
                // PHASE 2: Wire TreasuryManager to reclaim budget funds.
                // Requires passing TreasuryManager handle to GovernanceEventHandler.
                // See: https://github.com/InterCooperative-Network/icn/issues/246
                warn!("⚠️ ReclaimBudget execution not yet implemented (Phase 2)");
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

        // PHASE 2: Wire TreasuryManager to create budgets.
        // Requires passing TreasuryManager handle to GovernanceEventHandler.
        // See: https://github.com/InterCooperative-Network/icn/issues/246
        warn!("⚠️ CreateBudget execution not yet implemented (Phase 2)");
        let _ = period_end;
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

        // Reuse the existing budget proposal logic for the actual ledger transfer
        // The treasury withdrawal is essentially a budget allocation to a recipient
        self.handle_budget_proposal(proposal_id.clone(), amount, recipient, currency, decided_at);

        // PHASE 2: Record in TreasuryManager audit trail with budget_id reference.
        // Requires passing TreasuryManager handle to GovernanceEventHandler.
        // See: https://github.com/InterCooperative-Network/icn/issues/246
        let _ = (treasury_did, budget_id);
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
