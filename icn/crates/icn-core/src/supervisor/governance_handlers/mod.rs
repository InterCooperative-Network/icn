//! Governance event handlers for proposal execution
//!
//! This module extracts the governance event handlers from the supervisor,
//! providing testable, focused handlers for each proposal type.
//!
//! Submodules:
//! - `federation` - Federation proposal handlers (join/leave, clearing, vouch)
//! - `protocol` - Protocol change handlers (upgrades, parameter changes, delayed execution)
//! - `treasury` - Treasury governance handlers (budgets, withdrawals, spending rules)

mod federation;
mod protocol;
mod treasury;

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

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

/// Type alias for the cooperative store handle
pub type CoopStoreHandle = Arc<icn_coop::CoopStore>;

// =============================================================================
// Validation helpers - shared logic for treasury operation validation
// =============================================================================

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
    /// Cooperative store for treasury nonce atomicity
    coop_store: Option<CoopStoreHandle>,

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
            coop_store: None,
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

    /// Set the cooperative store for treasury nonce atomicity
    pub fn with_coop_store(mut self, coop_store: CoopStoreHandle) -> Self {
        self.coop_store = Some(coop_store);
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
                proposal_id: proposal_id.0.clone(),
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
            // Resource access governance (use-based access model)
            ProposalPayload::ResourceAccess {
                action,
                resource_id,
                holder,
                reason,
            } => {
                self.handle_resource_access(proposal_id, action, resource_id, holder, reason);
            }
        }
    }

    /// Handle a resource access proposal
    ///
    /// This handles granting or revoking use-based access to resources
    /// as part of the anti-rent-seeking model.
    fn handle_resource_access(
        &self,
        proposal_id: ProposalId,
        action: icn_governance::ResourceAccessAction,
        resource_id: String,
        holder: icn_entity::EntityId,
        reason: String,
    ) {
        match action {
            icn_governance::ResourceAccessAction::Grant { model } => {
                info!(
                    "🔑 Resource access {} granted: {} to {} ({:?}) - {}",
                    proposal_id.0, resource_id, holder, model, reason
                );
                // TODO: Integrate with ResourceAccessStore when implemented
                // The actual storage and enforcement will be added in follow-up PRs
            }
            icn_governance::ResourceAccessAction::Revoke => {
                info!(
                    "🚫 Resource access {} revoked: {} from {} (reason: {})",
                    proposal_id.0, resource_id, holder, reason
                );
                // TODO: Integrate with ResourceAccessStore when implemented
            }
        }
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

    // =========================================================================
    // Labor Share Operations (Issue #389)
    // =========================================================================
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
                // Deserialize the payload back to ProposalPayload for dispatch
                match serde_json::from_value::<icn_governance::ProposalPayload>(payload.clone()) {
                    Ok(proposal_payload) => {
                        handler.handle_proposal_accepted(
                            icn_governance::ProposalId(proposal_id.clone()),
                            proposal_payload,
                            *decided_at,
                            domain_id.clone(),
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to deserialize ProposalPayload for proposal {}: {}",
                            proposal_id,
                            e
                        );
                        handler.emit_execution_failure(
                            &icn_governance::ProposalId(proposal_id.clone()),
                            "unknown",
                            &format!("payload deserialization failed: {e}"),
                        );
                    }
                }
            }
            SystemEvent::ProposalRejected { proposal_id, .. } => {
                info!("❌ Proposal {} rejected - no action taken", proposal_id);
            }
            _ => {}
        }
    })
}

/// Create an execution callback from a GovernanceEventHandler
///
/// This bridges the kernel-api ProposalExecutor interface with the existing
/// GovernanceEventHandler. The callback deserializes the JSON payload and
/// delegates to the handler's handle_proposal_accepted method.
///
/// # Phase 4 Sprint 2
///
/// This is a temporary bridge. Future sprints will move handler logic into
/// apps/governance, eliminating this callback pattern.
pub fn create_execution_callback(
    handler: GovernanceEventHandler,
) -> icn_governance_actor::executor::ExecutionCallback {
    Arc::new(
        move |proposal_id: &str, payload: &serde_json::Value, decided_at: u64, domain_id: &str| {
            // Deserialize the JSON payload to ProposalPayload
            match serde_json::from_value::<ProposalPayload>(payload.clone()) {
                Ok(proposal_payload) => {
                    handler.handle_proposal_accepted(
                        ProposalId(proposal_id.to_string()),
                        proposal_payload,
                        decided_at,
                        domain_id.to_string(),
                    );
                    Ok(())
                }
                Err(e) => {
                    let error_msg = format!("Failed to deserialize ProposalPayload: {}", e);
                    error!(
                        proposal_id = %proposal_id,
                        error = %error_msg,
                        "Proposal execution failed"
                    );
                    Err(error_msg)
                }
            }
        },
    )
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

        if let SystemEvent::ProposalAccepted {
            proposal_id,
            payload,
            decided_at,
            ..
        } = &event
        {
            // Deserialize and check if it's a SchedulingPolicy proposal
            if let Ok(icn_governance::ProposalPayload::SchedulingPolicy {
                coop_id,
                policy_json,
            }) = serde_json::from_value::<icn_governance::ProposalPayload>(payload.clone())
            {
                handler.handle_scheduling_policy(
                    icn_governance::ProposalId(proposal_id.clone()),
                    coop_id,
                    policy_json,
                    *decided_at,
                );
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::treasury::{validate_currency_match, validate_positive_amount};
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
