//! Federation governance proposal handlers
//!
//! Extracted from governance_handlers/mod.rs to reduce file size.
//! These handlers manage inter-cooperative relationships including
//! joining/leaving federations, establishing clearing agreements,
//! and managing trust attestations.

use tracing::{debug, error, info, warn};

use super::{AttestationStoreHandle, ClearingManagerHandle, CooperativeRegistryHandle};
use icn_governance::{FederationProposal, ProposalId};

impl super::GovernanceEventHandler {
    /// Handle a federation governance proposal (Issue #514)
    ///
    /// Federation proposals enable governance-controlled management of inter-cooperative
    /// relationships including joining/leaving federations, establishing clearing agreements,
    /// and managing trust attestations.
    pub(super) fn handle_federation_proposal(
        &self,
        proposal_id: ProposalId,
        proposal: icn_governance::FederationProposal,
    ) {
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
                    trust_decay_factor,
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
        partner_coop_did: &icn_identity::Did,
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
        target_coop_did: &icn_identity::Did,
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
}
