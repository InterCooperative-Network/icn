//! Proposal payload to kernel-effect translation.
//!
//! This is the app-layer boundary used by the production effect path.

use icn_governance::ProposalPayload;
use icn_kernel_api::effects::{
    ControlEffect, DisputeEffect, FederationEffect, KernelEffect, MembershipEffect, ProtocolEffect,
    ResourceEffect, SdisEffect, TreasuryEffect,
};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslationError {
    pub kind: &'static str,
    pub detail: String,
}

impl TranslationError {
    fn unsupported(kind: &'static str, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for TranslationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} is not executable in current kernel path: {}",
            self.kind, self.detail
        )
    }
}

/// Translate a governance proposal payload to kernel-safe effects.
///
/// This is the key boundary function: the governance app understands domain
/// types (ProposalPayload, TreasuryProposalOperation, etc.), but the kernel
/// only sees KernelEffect with primitive types.
///
/// # Arguments
/// * `payload` - The domain-specific proposal payload
/// * `decision_receipt_id` - The receipt ID for audit linkage
///
/// # Returns
/// A vector of kernel effects (usually 1, but some proposals produce multiple)
pub fn translate_payload_to_effects(
    payload: &ProposalPayload,
    decision_receipt_id: &str,
    decision_hash: &str,
    domain_id: &str,
) -> Result<Vec<KernelEffect>, TranslationError> {
    match payload {
        // Treasury proposals
        ProposalPayload::Treasury { operation } => {
            translate_treasury_operation(operation, decision_receipt_id, decision_hash)
        }

        ProposalPayload::Budget {
            amount,
            currency,
            purpose,
            ..
        } => Ok(vec![KernelEffect::Treasury(TreasuryEffect::CreateBudget {
            treasury_did: domain_id.to_string(),
            budget_id: format!("budget-{purpose}"),
            total_amount: *amount,
            currency: currency.clone(),
            name: purpose.clone(),
            validity_start: 0,
            validity_end: u64::MAX,
            decision_receipt_id: decision_receipt_id.to_string(),
            decision_hash: decision_hash.to_string(),
        })]),

        ProposalPayload::SurplusAllocation { allocation } => {
            // Use member_payments (resolved holder DIDs) not allocations (ShareId keys).
            // ShareId strings are not valid DIDs; the ledger executor parses distributions as DIDs.
            let distributions = allocation
                .member_payments
                .iter()
                .map(|(did, amount)| (did.to_string(), *amount))
                .collect();
            Ok(vec![KernelEffect::Treasury(
                TreasuryEffect::DistributeSurplus {
                    treasury_did: domain_id.to_string(),
                    total_amount: allocation.total_surplus,
                    currency: allocation.currency.clone(),
                    distributions,
                    decision_receipt_id: decision_receipt_id.to_string(),
                    decision_hash: decision_hash.to_string(),
                },
            )])
        }

        // Membership proposals
        ProposalPayload::Membership { action, member } => {
            translate_membership_action(action, member, domain_id, decision_hash)
        }

        ProposalPayload::FreezeMember {
            member,
            reason,
            duration_seconds,
        } => Ok(vec![KernelEffect::Membership(
            MembershipEffect::FreezeMember {
                entity_id: domain_id.to_string(),
                member_did: member.to_string(),
                reason: reason.clone(),
                duration_secs: *duration_seconds,
                decision_hash: decision_hash.to_string(),
            },
        )]),

        ProposalPayload::UnfreezeMember { member, reason: _ } => {
            Ok(vec![KernelEffect::Membership(
                MembershipEffect::UnfreezeMember {
                    entity_id: domain_id.to_string(),
                    member_did: member.to_string(),
                    decision_hash: decision_hash.to_string(),
                },
            )])
        }

        // Protocol proposals
        ProposalPayload::ConfigChange { new_config } => Ok(vec![KernelEffect::Protocol(
            ProtocolEffect::SetGovernanceConfig {
                domain_id: domain_id.to_string(),
                config_hash: blake3::hash(new_config.as_bytes()).to_hex().to_string(),
                config_json: new_config.clone(),
            },
        )]),

        ProposalPayload::SchedulingPolicy {
            coop_id,
            policy_json,
        } => Ok(vec![KernelEffect::Protocol(
            ProtocolEffect::SetSchedulingPolicy {
                coop_id: coop_id.clone(),
                policy_hash: blake3::hash(policy_json.as_bytes()).to_hex().to_string(),
                policy_json: policy_json.clone(),
            },
        )]),

        ProposalPayload::ProtocolUpgrade { version, .. } => {
            Ok(vec![KernelEffect::Protocol(ProtocolEffect::Upgrade {
                version: version.to_string(),
                upgrade_hash: String::new(),
                activation_height: 0,
            })])
        }

        ProposalPayload::ProtocolChange { proposal } => {
            Ok(vec![KernelEffect::Protocol(ProtocolEffect::SetParameter {
                parameter_name: proposal.parameter_id.clone(),
                old_value_hash: String::new(), // Not carried by proposal payload
                new_value_json: proposal.new_value.to_string(),
                effective_at: proposal.effective_at.unwrap_or(0),
            })])
        }

        // Control proposals
        ProposalPayload::VetoProposal {
            target_proposal_id,
            reason,
        } => Ok(vec![KernelEffect::Control(ControlEffect::VetoProposal {
            target_proposal_id: target_proposal_id.clone(),
            veto_reason: reason.clone(),
        })]),

        ProposalPayload::ForceCloseProposal {
            target_proposal_id,
            reason,
            ..
        } => Ok(vec![KernelEffect::Control(
            ControlEffect::ForceCloseProposal {
                target_proposal_id: target_proposal_id.clone(),
                close_reason: reason.clone(),
            },
        )]),

        ProposalPayload::Text { body } => {
            Ok(vec![KernelEffect::Control(ControlEffect::TextResolution {
                resolution_hash: blake3::hash(body.as_bytes()).to_hex().to_string(),
            })])
        }

        // Dispute proposals
        ProposalPayload::RollbackLedger {
            target_hash,
            reason,
            affected_accounts: _,
        } => Ok(vec![KernelEffect::Dispute(DisputeEffect::RollbackLedger {
            entry_ids: vec![target_hash.clone()],
            rollback_reason: reason.clone(),
            authorized_by: decision_receipt_id.to_string(),
        })]),

        ProposalPayload::DisputeResolution {
            dispute_entry_hash,
            reason,
            proposed_outcome,
            ..
        } => {
            let compensations = match proposed_outcome {
                icn_governance::DisputeResolutionOutcome::Partial {
                    adjustment,
                    currency,
                } => vec![(String::new(), *adjustment, currency.clone())],
                _ => vec![],
            };
            Ok(vec![KernelEffect::Dispute(DisputeEffect::ResolveDispute {
                dispute_id: dispute_entry_hash.clone(),
                resolution_hash: blake3::hash(reason.as_bytes()).to_hex().to_string(),
                compensations,
            })])
        }

        // SDIS proposals
        ProposalPayload::Sdis { proposal } => match proposal {
            icn_governance::sdis::SdisProposal::AppointSteward {
                candidate,
                region,
                term_length,
                ..
            } => Ok(vec![KernelEffect::Sdis(SdisEffect::ApproveSteward {
                steward_did: candidate.to_string(),
                jurisdiction_id: domain_id.to_string(),
                term_length_seconds: *term_length as i64,
                region: if region.is_empty() {
                    None
                } else {
                    Some(region.clone())
                },
                proposal_id: decision_receipt_id.to_string(),
                capabilities_hash: String::new(),
            })]),
            icn_governance::sdis::SdisProposal::RemoveSteward {
                steward, reason, ..
            } => Ok(vec![KernelEffect::Sdis(SdisEffect::RevokeSteward {
                steward_did: steward.to_string(),
                reason: reason.clone(),
            })]),
            icn_governance::sdis::SdisProposal::ReconfirmSteward {
                steward,
                new_term_end,
                ..
            } => Ok(vec![KernelEffect::Sdis(SdisEffect::ReconfirmSteward {
                steward_did: steward.to_string(),
                new_term_end: *new_term_end,
                proposal_id: decision_receipt_id.to_string(),
            })]),
            icn_governance::sdis::SdisProposal::ReinstateSteward { steward, .. } => {
                Ok(vec![KernelEffect::Sdis(SdisEffect::ReinstateSteward {
                    steward_did: steward.to_string(),
                    proposal_id: decision_receipt_id.to_string(),
                })])
            }
            icn_governance::sdis::SdisProposal::SuspendSteward {
                steward,
                reason,
                duration,
                ..
            } => Ok(vec![KernelEffect::Sdis(SdisEffect::SuspendSteward {
                steward_did: steward.to_string(),
                reason: reason.clone(),
                duration_seconds: *duration,
                proposal_id: decision_receipt_id.to_string(),
            })]),

            // SanctionSteward fans out to different kernel effects based on the
            // penalty type.
            //
            // - Warning / Censure / Probation: record-only sanctions. The governance
            //   vote itself is the institutional act; no CommonsHandle state changes.
            //   These resolve to KernelEffect::NoOp so the receipt/audit chain still
            //   records the accepted proposal. Policy/CCL may add downstream trust or
            //   reputation consequences. See ADR-0014.
            //
            // - Suspension: delegates to SdisEffect::SuspendSteward (existing path).
            //
            // - TierDemotion / Removal: delegates to appropriate Sdis effects.
            //   TierDemotion is Deferred pending UpdateJurisdictionTier kernel support.
            icn_governance::sdis::SdisProposal::SanctionSteward {
                steward,
                penalty,
                reason,
                ..
            } => match penalty {
                icn_governance::sdis::StewardPenalty::Warning => {
                    Ok(vec![KernelEffect::NoOp {
                        reason: format!(
                            "SanctionSteward(Warning): governance warning recorded for {steward}: {reason}"
                        ),
                    }])
                }
                icn_governance::sdis::StewardPenalty::Censure => {
                    Ok(vec![KernelEffect::NoOp {
                        reason: format!(
                            "SanctionSteward(Censure): institutional censure recorded for {steward}: {reason}"
                        ),
                    }])
                }
                icn_governance::sdis::StewardPenalty::Probation { duration, .. } => {
                    Ok(vec![KernelEffect::NoOp {
                        reason: format!(
                            "SanctionSteward(Probation): {duration}s probation recorded for {steward}: {reason}"
                        ),
                    }])
                }
                icn_governance::sdis::StewardPenalty::Suspension { duration } => {
                    Ok(vec![KernelEffect::Sdis(SdisEffect::SuspendSteward {
                        steward_did: steward.to_string(),
                        reason: reason.clone(),
                        duration_seconds: *duration,
                        proposal_id: decision_receipt_id.to_string(),
                    })])
                }
                icn_governance::sdis::StewardPenalty::Removal => {
                    Ok(vec![KernelEffect::Sdis(SdisEffect::RevokeSteward {
                        steward_did: steward.to_string(),
                        reason: reason.clone(),
                    })])
                }
                icn_governance::sdis::StewardPenalty::TierDemotion { new_tier } => {
                    use icn_governance::sdis::JurisdictionTier;
                    // TierDemotion → UpdateJurisdictionTier kernel effect.
                    // The executor records this as deferred (not_executed=true) pending
                    // CommonsHandle.update_jurisdiction_tier implementation. See ADR-0015.
                    let tier_str = match new_tier {
                        JurisdictionTier::Tier1 => "Tier1",
                        JurisdictionTier::Tier2 => "Tier2",
                        JurisdictionTier::Tier3 => "Tier3",
                    };
                    Ok(vec![KernelEffect::Sdis(SdisEffect::UpdateJurisdictionTier {
                        steward_did: steward.to_string(),
                        new_tier: tier_str.to_string(),
                        reason: reason.clone(),
                        proposal_id: decision_receipt_id.to_string(),
                    })])
                }
            },

            // UpdateJurisdictionTier proposal: governance-ratified tier change.
            // The executor records this as deferred pending CommonsHandle support.
            icn_governance::sdis::SdisProposal::UpdateJurisdictionTier {
                steward,
                new_tier,
                reason,
            } => {
                use icn_governance::sdis::JurisdictionTier;
                let tier_str = match new_tier {
                    JurisdictionTier::Tier1 => "Tier1",
                    JurisdictionTier::Tier2 => "Tier2",
                    JurisdictionTier::Tier3 => "Tier3",
                };
                Ok(vec![KernelEffect::Sdis(SdisEffect::UpdateJurisdictionTier {
                    steward_did: steward.to_string(),
                    new_tier: tier_str.to_string(),
                    reason: reason.clone(),
                    proposal_id: decision_receipt_id.to_string(),
                })])
            }

            // Explicitly deferred SDIS proposals.
            //
            // These are recognized proposal types that require infrastructure not yet
            // present in the kernel execution path. Each fails fast with a named error
            // so the gap is visible (not silently swallowed). The governance receipt IS
            // recorded; only execution is deferred.
            //
            // See ADR-0015 Invariant C: "Where governance gaps exist, they must be named."
            icn_governance::sdis::SdisProposal::ModifyThreshold { .. } => {
                Err(TranslationError::unsupported(
                    "sdis:ModifyThreshold",
                    "Deferred: no kernel threshold-registry effect. \
                     Threshold values are currently policy-layer constants.",
                ))
            }
            icn_governance::sdis::SdisProposal::ApproveAuthority { .. } => {
                Err(TranslationError::unsupported(
                    "sdis:ApproveAuthority",
                    "Deferred: no kernel authority-registry effect. \
                     Institutional authority approval requires a dedicated registry.",
                ))
            }
            icn_governance::sdis::SdisProposal::RevokeAuthority { .. } => {
                Err(TranslationError::unsupported(
                    "sdis:RevokeAuthority",
                    "Deferred: no kernel authority-registry effect. \
                     Revoking institutional authority requires a dedicated registry.",
                ))
            }
            icn_governance::sdis::SdisProposal::RevocationAppeal { .. } => {
                Err(TranslationError::unsupported(
                    "sdis:RevocationAppeal",
                    "Deferred: revocation appeal is a multi-step adjudication flow \
                     without a single kernel effect. Requires arbitration infrastructure.",
                ))
            }
            icn_governance::sdis::SdisProposal::ForceKeyRotation { .. } => {
                Err(TranslationError::unsupported(
                    "sdis:ForceKeyRotation",
                    "Deferred: forced key rotation requires a DID rotation flow \
                     across identity, network, and steward layers. No single kernel effect.",
                ))
            }
        },

        // Federation proposals
        ProposalPayload::Federation(fed_proposal) => translate_federation_proposal(fed_proposal),

        // Resource access proposals
        ProposalPayload::ResourceAccess {
            action,
            resource_id,
            holder,
            reason: _,
        } => match action {
            icn_governance::ResourceAccessAction::Grant { model } => {
                Ok(vec![KernelEffect::Resource(ResourceEffect::GrantAccess {
                    grantee_did: holder.to_string(),
                    resource_type: resource_id.clone(),
                    access_model_hash: blake3::hash(format!("{model:?}").as_bytes())
                        .to_hex()
                        .to_string(),
                })])
            }
            icn_governance::ResourceAccessAction::Revoke => {
                Ok(vec![KernelEffect::Resource(ResourceEffect::RevokeAccess {
                    grantee_did: holder.to_string(),
                    resource_type: resource_id.clone(),
                })])
            }
        },

        // Participatory budgeting: create an envelope budget, then allocate per option.
        //
        // CreateBudget defines the governance-authorized spending envelope (not a debit).
        // Each Allocate reserves a portion of the envelope for a specific option.
        // The budget_id includes the decision_receipt_id to prevent collision across
        // allocation rounds with the same purpose string.
        //
        // NOTE: treasury_did is wired from domain_id (caller context), same pattern
        // as Budget and other treasury translations.
        ProposalPayload::Allocation {
            pool_amount,
            unit,
            options,
            purpose,
        } => {
            let budget_id = format!("alloc-{purpose}-{decision_receipt_id}");
            let mut effects = vec![KernelEffect::Treasury(TreasuryEffect::CreateBudget {
                treasury_did: domain_id.to_string(),
                budget_id: budget_id.clone(),
                total_amount: *pool_amount,
                currency: unit.clone(),
                name: purpose.clone(),
                validity_start: 0,
                validity_end: u64::MAX,
                decision_receipt_id: decision_receipt_id.to_string(),
                decision_hash: decision_hash.to_string(),
            })];
            for opt in options {
                effects.push(KernelEffect::Treasury(TreasuryEffect::Allocate {
                    treasury_did: domain_id.to_string(),
                    budget_id: budget_id.clone(),
                    recipient_did: opt.recipient.to_string(),
                    amount: opt.requested_amount,
                    currency: unit.clone(),
                    decision_hash: decision_hash.to_string(),
                }));
            }
            Ok(effects)
        }

        // Bond issuance: governance-approved capital raise via cooperative bond.
        //
        // The bond_id is derived from the issuer and decision receipt to ensure
        // uniqueness across proposals. `maturity_date` encodes term duration as
        // seconds (term_days × 86400) — deterministic given the payload, not wall-clock.
        // `interest_rate_bps` and full `BondOffering` terms are preserved in the
        // governance decision payload; only the financial effect is recorded in the ledger.
        ProposalPayload::BondIssuance { bond_offering } => {
            let bond_id = format!("bond-{}-{decision_receipt_id}", bond_offering.issuer_id);
            Ok(vec![KernelEffect::Treasury(TreasuryEffect::IssueBond {
                treasury_did: domain_id.to_string(),
                bond_id,
                principal: bond_offering.principal_requested,
                interest_rate_bps: bond_offering.interest_rate_bps as u32,
                maturity_date: bond_offering.term_days as u64 * 86400,
                currency: bond_offering.currency.clone(),
                decision_hash: decision_hash.to_string(),
            })])
        }

        // Share redemption: member exchanges shares for payout from treasury.
        //
        // `payout_amount` is the sum of all approved installments. The payout
        // schedule is governance-approved; only the financial effect lands in
        // the ledger here. Installment tracking is out of scope for this tranche.
        ProposalPayload::ShareRedemption {
            member,
            share_ids,
            payout_schedule,
            currency,
            ..
        } => {
            let payout_amount: i64 = payout_schedule.iter().map(|s| s.amount).sum();
            Ok(vec![KernelEffect::Treasury(TreasuryEffect::RedeemShares {
                treasury_did: domain_id.to_string(),
                member_did: member.to_string(),
                share_count: share_ids.len() as u64,
                payout_amount,
                currency: currency.clone(),
                decision_hash: decision_hash.to_string(),
            })])
        }

        // Charter ratification: deploy a CCL charter document to the policy layer.
        //
        // When members ratify a charter, the charter YAML is stored as a governance
        // config (SetGovernanceConfig effect). The CharterPolicyOracle reads this config
        // and enforces it via ConstraintSet. The charter_id is stored as the domain_id.
        ProposalPayload::Charter {
            charter_id,
            charter_yaml,
        } => Ok(vec![KernelEffect::Protocol(
            icn_kernel_api::effects::ProtocolEffect::SetGovernanceConfig {
                domain_id: charter_id.clone(),
                config_hash: blake3::hash(charter_yaml.as_bytes()).to_hex().to_string(),
                config_json: charter_yaml.clone(),
            },
        )]),

    }
}

/// Translate treasury operations to kernel effects
fn translate_treasury_operation(
    operation: &icn_governance::TreasuryProposalOperation,
    decision_receipt_id: &str,
    decision_hash: &str,
) -> Result<Vec<KernelEffect>, TranslationError> {
    use icn_governance::TreasuryProposalOperation;

    match operation {
        TreasuryProposalOperation::Withdraw {
            treasury_did,
            recipient,
            amount,
            currency,
            purpose,
            nonce,
            budget_id,
        } => Ok(vec![KernelEffect::Treasury(TreasuryEffect::Spend {
            treasury_did: treasury_did.to_string(),
            recipient_did: recipient.to_string(),
            amount: *amount,
            currency: currency.clone(),
            memo: purpose.clone(),
            budget_id: budget_id.clone(),
            expected_nonce: *nonce,
            decision_receipt_id: decision_receipt_id.to_string(),
            decision_hash: decision_hash.to_string(),
        })]),

        TreasuryProposalOperation::Spend {
            treasury_did,
            recipient,
            amount,
            currency,
            memo,
            nonce,
        } => Ok(vec![KernelEffect::Treasury(TreasuryEffect::Spend {
            treasury_did: treasury_did.to_string(),
            recipient_did: recipient.to_string(),
            amount: *amount,
            currency: currency.clone(),
            memo: memo.clone(),
            budget_id: None,
            expected_nonce: *nonce,
            decision_receipt_id: decision_receipt_id.to_string(),
            decision_hash: decision_hash.to_string(),
        })]),

        TreasuryProposalOperation::CreateBudget {
            treasury_did,
            purpose,
            amount,
            currency,
            period_end,
        } => Ok(vec![KernelEffect::Treasury(TreasuryEffect::CreateBudget {
            treasury_did: treasury_did.to_string(),
            budget_id: format!("budget-{purpose}"),
            total_amount: *amount,
            currency: currency.clone(),
            name: purpose.clone(),
            validity_start: 0,
            validity_end: period_end.unwrap_or(u64::MAX),
            decision_receipt_id: decision_receipt_id.to_string(),
            decision_hash: decision_hash.to_string(),
        })]),

        // TransferBetweenBudgets: moves funds from one budget to another.
        // Maps to TreasuryEffect::Transfer between the two budget internal accounts.
        // The budget_ids are used as the from/to identifiers in the transfer.
        TreasuryProposalOperation::TransferBetweenBudgets {
            treasury_did,
            from_budget,
            to_budget,
            amount,
            currency,
            reason,
        } => Ok(vec![KernelEffect::Treasury(TreasuryEffect::Transfer {
            from_did: format!("{}:budget:{}", treasury_did, from_budget),
            to_did: format!("{}:budget:{}", treasury_did, to_budget),
            amount: *amount,
            currency: currency.clone(),
            memo: reason.clone(),
            decision_hash: decision_hash.to_string(),
        })]),

        // CancelBudget: no kernel effect exists for cancellation.
        // The governance vote is the cancellation record; the budget becomes
        // inactive at the CCL/policy layer. Future: add TreasuryEffect::CancelBudget.
        TreasuryProposalOperation::CancelBudget {
            budget_id, reason, ..
        } => Ok(vec![KernelEffect::NoOp {
            reason: format!(
                "CancelBudget({budget_id}): governance record is the cancellation act. \
                     Deferred: no TreasuryEffect::CancelBudget in kernel. Reason: {reason}"
            ),
        }]),

        // ReclaimBudget: no kernel effect exists for reclamation.
        // Deferred: requires a TreasuryEffect that returns unspent funds to unallocated pool.
        TreasuryProposalOperation::ReclaimBudget {
            budget_id, reason, ..
        } => Ok(vec![KernelEffect::NoOp {
            reason: format!(
                "ReclaimBudget({budget_id}): deferred — no TreasuryEffect::ReclaimBudget. \
                     Reason: {reason}"
            ),
        }]),

        // ModifySpendingRule: no kernel effect. Spending rules are CCL/policy-layer
        // governance documents. The ratified rule is the operative change.
        TreasuryProposalOperation::ModifySpendingRule { .. } => Ok(vec![KernelEffect::NoOp {
            reason: "ModifySpendingRule: spending rules are CCL/policy-layer governance. \
                     No kernel effect — the ratified rule is the operative change."
                .to_string(),
        }]),
    }
}

/// Translate membership actions to kernel effects
fn translate_membership_action(
    action: &icn_governance::MembershipAction,
    member: &icn_identity::Did,
    domain_id: &str,
    decision_hash: &str,
) -> Result<Vec<KernelEffect>, TranslationError> {
    use icn_governance::MembershipAction;

    match action {
        MembershipAction::Add => Ok(vec![KernelEffect::Membership(
            MembershipEffect::AddMember {
                entity_id: domain_id.to_string(),
                member_did: member.to_string(),
                role: String::new(),
                tier: String::new(),
                decision_hash: decision_hash.to_string(),
            },
        )]),
        MembershipAction::Remove => Ok(vec![KernelEffect::Membership(
            MembershipEffect::RemoveMember {
                entity_id: domain_id.to_string(),
                member_did: member.to_string(),
                reason: String::new(),
                decision_hash: decision_hash.to_string(),
            },
        )]),
    }
}

/// Translate federation proposals to kernel effects
fn translate_federation_proposal(
    proposal: &icn_governance::FederationProposal,
) -> Result<Vec<KernelEffect>, TranslationError> {
    use icn_governance::FederationProposal;

    match proposal {
        FederationProposal::JoinFederation { federation_id, .. } => {
            Ok(vec![KernelEffect::Federation(
                FederationEffect::JoinFederation {
                    coop_did: String::new(),
                    federation_id: federation_id.clone(),
                },
            )])
        }
        FederationProposal::LeaveFederation { federation_id, .. } => {
            Ok(vec![KernelEffect::Federation(
                FederationEffect::LeaveFederation {
                    coop_did: String::new(),
                    federation_id: federation_id.clone(),
                },
            )])
        }
        FederationProposal::EstablishClearing {
            partner_coop_did,
            settlement_interval,
            max_imbalance,
            source_agreement_id,
            ..
        } => Ok(vec![KernelEffect::Federation(
            FederationEffect::EstablishClearing {
                coop_a_did: String::new(),
                coop_b_did: partner_coop_did.to_string(),
                agreement_hash: String::new(),
                settlement_interval: Some(
                    match settlement_interval {
                        icn_federation::SettlementInterval::Daily => "daily",
                        icn_federation::SettlementInterval::Weekly => "weekly",
                        icn_federation::SettlementInterval::Monthly => "monthly",
                        icn_federation::SettlementInterval::Manual => "manual",
                    }
                    .to_string(),
                ),
                max_imbalance: Some(*max_imbalance),
                source_agreement_id: source_agreement_id.clone(),
            },
        )]),
        FederationProposal::VouchForCooperative {
            target_coop_did, ..
        } => Ok(vec![KernelEffect::Federation(
            FederationEffect::VouchForCoop {
                voucher_did: String::new(),
                vouchee_did: target_coop_did.to_string(),
                attestation_hash: String::new(),
            },
        )]),

        // TerminateClearing and RevokeVouch: governance-ratified federation lifecycle
        // operations. The kernel effects exist; the FederationService execution path is
        // deferred (executor handles with not_executed=true). See ADR-0015.
        FederationProposal::TerminateClearing {
            partner_coop_id, ..
        } => {
            Ok(vec![KernelEffect::Federation(
                FederationEffect::TerminateClearing {
                    coop_a_did: String::new(), // filled by executor from session context
                    coop_b_did: partner_coop_id.clone(),
                    decision_receipt_id: String::new(), // filled by executor
                },
            )])
        }
        FederationProposal::RevokeVouch { target_coop_id, .. } => {
            Ok(vec![KernelEffect::Federation(
                FederationEffect::RevokeVouch {
                    revoker_did: String::new(), // filled by executor from session context
                    revokee_did: target_coop_id.clone(),
                    decision_receipt_id: String::new(), // filled by executor
                },
            )])
        }

        // UpdateFederationPolicy: no kernel effect. Federation policy is a governance
        // document whose terms are enforced by the CCL/policy layer, not the kernel.
        // This is intentionally record-only — the accepted proposal IS the policy change.
        FederationProposal::UpdateFederationPolicy { .. } => Ok(vec![KernelEffect::NoOp {
            reason: "UpdateFederationPolicy: governance record is the policy update. \
                         Federation policy terms are enforced by CCL/policy layer, not kernel."
                .to_string(),
        }]),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use icn_governance::TreasuryProposalOperation;
    use icn_identity::Did;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_translate_treasury_spend_preserves_decision_provenance() {
        let treasury_did: icn_identity::Did =
            "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9"
                .parse()
                .expect("valid did");
        let recipient: icn_identity::Did = "did:icn:z8eQZfY3RY75YwQ6MrFCHt9phbi3HGx1caFXE3291ow8t"
            .parse()
            .expect("valid did");
        let payload = icn_governance::ProposalPayload::Treasury {
            operation: icn_governance::TreasuryProposalOperation::Spend {
                treasury_did: treasury_did.clone(),
                amount: 42,
                currency: "hours".to_string(),
                recipient: recipient.clone(),
                memo: "Pilot payout".to_string(),
                nonce: 7,
            },
        };

        let effects = translate_payload_to_effects(
            &payload,
            "receipt-123",
            "decision-hash-123",
            "domain-translation-test",
        )
        .expect("translation should succeed");
        assert_eq!(effects.len(), 1);

        match &effects[0] {
            KernelEffect::Treasury(TreasuryEffect::Spend {
                treasury_did: got_treasury,
                recipient_did: got_recipient,
                decision_receipt_id,
                decision_hash,
                expected_nonce,
                ..
            }) => {
                assert_eq!(got_treasury, &treasury_did.to_string());
                assert_eq!(got_recipient, &recipient.to_string());
                assert_eq!(decision_receipt_id, "receipt-123");
                assert_eq!(decision_hash, "decision-hash-123");
                assert_eq!(*expected_nonce, 7);
            }
            other => panic!("expected treasury spend effect, got {other:?}"),
        }
    }

    #[test]
    fn test_translate_bond_issuance_produces_issue_bond_effect() {
        let bond_offering = icn_ledger::BondOffering {
            issuer_id: "test-coop".to_string(),
            principal_requested: 500_000,
            interest_rate_bps: 300,
            term_days: 365,
            purpose: "Equipment purchase".to_string(),
            payment_schedule: icn_ledger::PaymentSchedule::InterestOnly { interval_days: 90 },
            currency: "COOP".to_string(),
            collateral: vec![],
        };
        let payload = icn_governance::ProposalPayload::BondIssuance { bond_offering };
        let effects = translate_payload_to_effects(
            &payload,
            "receipt-bond-001",
            "decision-hash-bond-001",
            "did:icn:zTreasury",
        )
        .expect("BondIssuance must translate successfully");

        assert_eq!(effects.len(), 1, "expected exactly one effect");
        match &effects[0] {
            KernelEffect::Treasury(TreasuryEffect::IssueBond {
                treasury_did,
                bond_id,
                principal,
                interest_rate_bps,
                maturity_date,
                currency,
                decision_hash,
            }) => {
                assert_eq!(treasury_did, "did:icn:zTreasury");
                assert!(
                    bond_id.starts_with("bond-test-coop-"),
                    "bond_id must include issuer: {bond_id}"
                );
                assert_eq!(*principal, 500_000);
                assert_eq!(*interest_rate_bps, 300);
                assert_eq!(
                    *maturity_date,
                    365 * 86400,
                    "maturity encodes term_days as seconds"
                );
                assert_eq!(currency, "COOP");
                assert_eq!(decision_hash, "decision-hash-bond-001");
            }
            other => panic!("expected IssueBond treasury effect, got {other:?}"),
        }
    }

    #[test]
    fn test_translate_charter_produces_set_governance_config() {
        // Charter ratification is now wired: it produces a SetGovernanceConfig effect
        // so the CharterPolicyOracle can deploy the charter at runtime.
        let charter_yaml = "schema_version: v0\nentity: coop\n";
        let payload = icn_governance::ProposalPayload::Charter {
            charter_id: "test-coop-charter".to_string(),
            charter_yaml: charter_yaml.to_string(),
        };
        let effects = translate_payload_to_effects(
            &payload,
            "receipt-abc",
            "decision-hash-abc",
            "domain-translation-test",
        )
        .expect("Charter payload must produce effects");
        assert_eq!(effects.len(), 1, "Charter must produce exactly one effect");
        let expected_hash = blake3::hash(charter_yaml.as_bytes()).to_hex().to_string();
        assert!(
            matches!(
                &effects[0],
                KernelEffect::Protocol(
                    icn_kernel_api::effects::ProtocolEffect::SetGovernanceConfig {
                        domain_id,
                        config_hash,
                        ..
                    }
                ) if domain_id == "test-coop-charter" && config_hash == &expected_hash
            ),
            "Charter must translate to SetGovernanceConfig, got {:?}",
            effects
        );
    }

    #[test]
    fn test_translate_share_redemption_produces_redeem_shares_effect() {
        let member_did = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9";
        let payout_schedule = vec![
            icn_ledger::ScheduledPayout::new(30 * 86400, 5_000),
            icn_ledger::ScheduledPayout::new(60 * 86400, 5_000),
        ];
        let payload = icn_governance::ProposalPayload::ShareRedemption {
            member: member_did.parse().expect("valid did"),
            share_ids: vec![
                icn_ledger::ShareId::new("share-001"),
                icn_ledger::ShareId::new("share-002"),
                icn_ledger::ShareId::new("share-003"),
            ],
            payout_schedule,
            reason: "Voluntary departure".to_string(),
            currency: "COOP".to_string(),
        };

        let effects = translate_payload_to_effects(
            &payload,
            "receipt-redeem-001",
            "decision-hash-redeem-001",
            "did:icn:zTreasury",
        )
        .expect("ShareRedemption must translate successfully");

        assert_eq!(effects.len(), 1, "expected exactly one effect");
        match &effects[0] {
            KernelEffect::Treasury(TreasuryEffect::RedeemShares {
                treasury_did,
                member_did: effect_member_did,
                share_count,
                payout_amount,
                currency,
                decision_hash,
            }) => {
                assert_eq!(treasury_did, "did:icn:zTreasury");
                assert_eq!(effect_member_did, member_did);
                assert_eq!(*share_count, 3, "share_count from share_ids.len()");
                assert_eq!(
                    *payout_amount, 10_000,
                    "payout_amount is sum of schedule: 5000+5000"
                );
                assert_eq!(currency, "COOP");
                assert_eq!(decision_hash, "decision-hash-redeem-001");
            }
            other => panic!("expected RedeemShares treasury effect, got {other:?}"),
        }
    }

    #[test]
    fn test_pilot_treasury_ops_do_not_fall_back_to_noop_with_legacy_env_flag() {
        let _env_guard = ENV_LOCK.lock().expect("env lock");
        // Legacy env gating was removed from runtime; keep this regression test to
        // ensure pilot treasury operations remain effect-path translated even when
        // old env flags are present.
        std::env::set_var("ICN_USE_EFFECT_PATH", "0");

        let treasury_did: Did = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9"
            .parse()
            .expect("valid did");
        let recipient: Did = "did:icn:z8eQZfY3RY75YwQ6MrFCHt9phbi3HGx1caFXE3291ow8t"
            .parse()
            .expect("valid did");

        let pilot_cases = vec![
            ProposalPayload::Treasury {
                operation: TreasuryProposalOperation::Spend {
                    treasury_did: treasury_did.clone(),
                    amount: 10,
                    currency: "hours".to_string(),
                    recipient: recipient.clone(),
                    memo: "pilot spend".to_string(),
                    nonce: 1,
                },
            },
            ProposalPayload::Treasury {
                operation: TreasuryProposalOperation::Withdraw {
                    treasury_did: treasury_did.clone(),
                    recipient: recipient.clone(),
                    amount: 11,
                    currency: "hours".to_string(),
                    purpose: "pilot withdraw".to_string(),
                    nonce: 2,
                    budget_id: None,
                },
            },
            ProposalPayload::Treasury {
                operation: TreasuryProposalOperation::CreateBudget {
                    treasury_did,
                    purpose: "pilot-budget".to_string(),
                    amount: 12,
                    currency: "hours".to_string(),
                    period_end: Some(42),
                },
            },
        ];

        for payload in pilot_cases {
            let effects = translate_payload_to_effects(
                &payload,
                "receipt-pilot",
                "hash-pilot",
                "domain-pilot",
            )
            .expect("pilot treasury payloads should translate");
            assert!(
                !effects
                    .iter()
                    .any(|e| matches!(e, KernelEffect::NoOp { .. })),
                "pilot treasury operation must not translate to NoOp: {payload:?}"
            );
        }

        std::env::remove_var("ICN_USE_EFFECT_PATH");
    }

    #[test]
    fn test_translate_allocation_produces_budget_and_allocate_effects() {
        let recipient_a: Did = "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9"
            .parse()
            .expect("valid did");
        let recipient_b: Did = "did:icn:z8eQZfY3RY75YwQ6MrFCHt9phbi3HGx1caFXE3291ow8t"
            .parse()
            .expect("valid did");

        let payload = ProposalPayload::Allocation {
            pool_amount: 10_000,
            unit: "compute-hours".to_string(),
            options: vec![
                icn_governance::AllocationOption {
                    label: "Infrastructure".to_string(),
                    description: "Cluster ops".to_string(),
                    recipient: recipient_a,
                    requested_amount: 6_000,
                },
                icn_governance::AllocationOption {
                    label: "Education".to_string(),
                    description: "Training".to_string(),
                    recipient: recipient_b,
                    requested_amount: 4_000,
                },
            ],
            purpose: "q1-budget".to_string(),
        };

        let effects = translate_payload_to_effects(
            &payload,
            "receipt-alloc-1",
            "decision-hash-alloc-1",
            "domain-alloc",
        )
        .expect("allocation should translate");

        // 1 CreateBudget + 2 Allocate = 3 effects total
        assert_eq!(effects.len(), 3, "expected 3 effects, got {effects:?}");

        // First effect: CreateBudget with decision provenance
        match &effects[0] {
            KernelEffect::Treasury(TreasuryEffect::CreateBudget {
                budget_id,
                total_amount,
                currency,
                name,
                decision_receipt_id,
                decision_hash,
                ..
            }) => {
                assert_eq!(budget_id, "alloc-q1-budget-receipt-alloc-1");
                assert_eq!(*total_amount, 10_000);
                assert_eq!(currency, "compute-hours");
                assert_eq!(name, "q1-budget");
                assert_eq!(decision_receipt_id, "receipt-alloc-1");
                assert_eq!(decision_hash, "decision-hash-alloc-1");
            }
            other => panic!("expected CreateBudget, got {other:?}"),
        }

        // Second effect: Allocate for first option
        match &effects[1] {
            KernelEffect::Treasury(TreasuryEffect::Allocate {
                budget_id,
                amount,
                currency,
                decision_hash,
                ..
            }) => {
                assert_eq!(budget_id, "alloc-q1-budget-receipt-alloc-1");
                assert_eq!(*amount, 6_000);
                assert_eq!(currency, "compute-hours");
                assert_eq!(
                    decision_hash, "decision-hash-alloc-1",
                    "Allocate must carry decision_hash from proposal context (empty → Deferred)"
                );
            }
            other => panic!("expected Allocate for Infrastructure, got {other:?}"),
        }

        // Third effect: Allocate for second option
        match &effects[2] {
            KernelEffect::Treasury(TreasuryEffect::Allocate {
                budget_id,
                amount,
                currency,
                decision_hash,
                ..
            }) => {
                assert_eq!(budget_id, "alloc-q1-budget-receipt-alloc-1");
                assert_eq!(*amount, 4_000);
                assert_eq!(currency, "compute-hours");
                assert_eq!(
                    decision_hash, "decision-hash-alloc-1",
                    "Allocate must carry decision_hash from proposal context (empty → Deferred)"
                );
            }
            other => panic!("expected Allocate for Education, got {other:?}"),
        }

        // No NoOp effects
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, KernelEffect::NoOp { .. })),
            "Allocation must not produce NoOp effects"
        );
    }

    #[test]
    fn test_translate_surplus_allocation_carries_decision_provenance() {
        let member_a_did: icn_identity::Did =
            "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9"
                .parse()
                .unwrap();
        let member_b_did: icn_identity::Did =
            "did:icn:zGyGKxMyg1p9SsHfm15MkNUu1u9TN2JtTspcdmrtGUdse"
                .parse()
                .unwrap();
        let allocation = icn_ledger::SurplusAllocation {
            id: "alloc-q1".to_string(),
            cooperative_id: "coop-x".to_string(),
            total_surplus: 500,
            period: "2025-Q1".to_string(),
            share_unit_value: 2.5,
            total_labor_days: 100,
            allocations: vec![
                (icn_ledger::ShareId::new("share-a"), 300),
                (icn_ledger::ShareId::new("share-b"), 200),
            ],
            // member_payments carries the resolved holder DIDs used by the execution path.
            member_payments: vec![(member_a_did.clone(), 300), (member_b_did.clone(), 200)],
            proposal_id: "prop-1".to_string(),
            allocated_at: 0,
            currency: "HOURS".to_string(),
        };
        let payload = icn_governance::ProposalPayload::SurplusAllocation { allocation };
        let effects = translate_payload_to_effects(
            &payload,
            "receipt-surplus-1",
            "hash-surplus-1",
            "domain-coop-x",
        )
        .expect("surplus allocation should translate");
        assert_eq!(effects.len(), 1);
        match &effects[0] {
            KernelEffect::Treasury(TreasuryEffect::DistributeSurplus {
                treasury_did,
                total_amount,
                currency,
                distributions,
                decision_receipt_id,
                decision_hash,
            }) => {
                assert_eq!(treasury_did, "domain-coop-x");
                assert_eq!(*total_amount, 500);
                assert_eq!(currency, "HOURS");
                // distributions must carry member DIDs (not ShareId strings).
                assert_eq!(
                    distributions,
                    &vec![
                        (member_a_did.to_string(), 300),
                        (member_b_did.to_string(), 200)
                    ]
                );
                assert_eq!(decision_receipt_id, "receipt-surplus-1");
                assert_eq!(decision_hash, "hash-surplus-1");
            }
            other => panic!("expected DistributeSurplus effect, got {other:?}"),
        }
    }
}
