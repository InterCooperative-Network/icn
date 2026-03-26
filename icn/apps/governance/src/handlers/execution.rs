//! Proposal payload to kernel-effect translation.
//!
//! This is the app-layer boundary used by the production effect path.

use icn_governance::ProposalPayload;
use icn_kernel_api::effects::{
    ControlEffect, DisputeEffect, FederationEffect, KernelEffect, MembershipEffect, ProtocolEffect,
    ResourceEffect, SdisEffect, TreasuryEffect,
};

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
) -> Vec<KernelEffect> {
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
        } => vec![KernelEffect::Treasury(TreasuryEffect::CreateBudget {
            treasury_did: domain_id.to_string(),
            budget_id: format!("budget-{purpose}"),
            total_amount: *amount,
            currency: currency.clone(),
            name: purpose.clone(),
            validity_start: 0,
            validity_end: u64::MAX,
            decision_receipt_id: decision_receipt_id.to_string(),
            decision_hash: decision_hash.to_string(),
        })],

        ProposalPayload::SurplusAllocation { allocation } => {
            let distributions = allocation
                .allocations
                .iter()
                .map(|(share_id, amount)| (share_id.0.clone(), *amount))
                .collect();
            vec![KernelEffect::Treasury(TreasuryEffect::DistributeSurplus {
                treasury_did: domain_id.to_string(),
                total_amount: allocation.total_surplus,
                currency: allocation.currency.clone(),
                distributions,
            })]
        }

        // Membership proposals
        ProposalPayload::Membership { action, member } => {
            translate_membership_action(action, member, domain_id)
        }

        ProposalPayload::FreezeMember { member, reason, .. } => {
            vec![KernelEffect::Membership(MembershipEffect::FreezeMember {
                entity_id: domain_id.to_string(),
                member_did: member.to_string(),
                reason: reason.clone(),
                duration_secs: None,
            })]
        }

        ProposalPayload::UnfreezeMember { member, reason: _ } => {
            vec![KernelEffect::Membership(MembershipEffect::UnfreezeMember {
                entity_id: domain_id.to_string(),
                member_did: member.to_string(),
            })]
        }

        // Protocol proposals
        ProposalPayload::ConfigChange { new_config } => {
            vec![KernelEffect::Protocol(
                ProtocolEffect::SetGovernanceConfig {
                    domain_id: domain_id.to_string(),
                    config_hash: blake3::hash(new_config.as_bytes()).to_hex().to_string(),
                    config_json: new_config.clone(),
                },
            )]
        }

        ProposalPayload::SchedulingPolicy {
            coop_id,
            policy_json,
        } => {
            vec![KernelEffect::Protocol(
                ProtocolEffect::SetSchedulingPolicy {
                    coop_id: coop_id.clone(),
                    policy_hash: blake3::hash(policy_json.as_bytes()).to_hex().to_string(),
                    policy_json: policy_json.clone(),
                },
            )]
        }

        ProposalPayload::ProtocolUpgrade { version, .. } => {
            vec![KernelEffect::Protocol(ProtocolEffect::Upgrade {
                version: version.to_string(),
                upgrade_hash: String::new(),
                activation_height: 0,
            })]
        }

        ProposalPayload::ProtocolChange { proposal } => {
            vec![KernelEffect::Protocol(ProtocolEffect::SetParameter {
                parameter_name: proposal.parameter_id.clone(),
                old_value_hash: String::new(), // Not carried by proposal payload
                new_value_json: proposal.new_value.to_string(),
                effective_at: proposal.effective_at.unwrap_or(0),
            })]
        }

        // Control proposals
        ProposalPayload::VetoProposal {
            target_proposal_id,
            reason,
        } => vec![KernelEffect::Control(ControlEffect::VetoProposal {
            target_proposal_id: target_proposal_id.clone(),
            veto_reason: reason.clone(),
        })],

        ProposalPayload::ForceCloseProposal {
            target_proposal_id,
            reason,
            ..
        } => vec![KernelEffect::Control(ControlEffect::ForceCloseProposal {
            target_proposal_id: target_proposal_id.clone(),
            close_reason: reason.clone(),
        })],

        ProposalPayload::Text { body } => {
            vec![KernelEffect::Control(ControlEffect::TextResolution {
                resolution_hash: blake3::hash(body.as_bytes()).to_hex().to_string(),
            })]
        }

        // Dispute proposals
        ProposalPayload::RollbackLedger {
            target_hash,
            reason,
            affected_accounts: _,
        } => vec![KernelEffect::Dispute(DisputeEffect::RollbackLedger {
            entry_ids: vec![target_hash.clone()],
            rollback_reason: reason.clone(),
            authorized_by: decision_receipt_id.to_string(),
        })],

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
            vec![KernelEffect::Dispute(DisputeEffect::ResolveDispute {
                dispute_id: dispute_entry_hash.clone(),
                resolution_hash: blake3::hash(reason.as_bytes()).to_hex().to_string(),
                compensations,
            })]
        }

        // SDIS proposals
        ProposalPayload::Sdis { proposal } => match proposal {
            icn_governance::sdis::SdisProposal::AppointSteward { candidate, .. } => {
                vec![KernelEffect::Sdis(SdisEffect::ApproveSteward {
                    steward_did: candidate.to_string(),
                    capabilities_hash: String::new(),
                })]
            }
            icn_governance::sdis::SdisProposal::RemoveSteward {
                steward, reason, ..
            } => {
                vec![KernelEffect::Sdis(SdisEffect::RevokeSteward {
                    steward_did: steward.to_string(),
                    reason: reason.clone(),
                })]
            }
            _ => vec![KernelEffect::NoOp {
                reason: format!(
                    "SDIS proposal type not yet translated: {:?}",
                    std::mem::discriminant(proposal)
                ),
            }],
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
                vec![KernelEffect::Resource(ResourceEffect::GrantAccess {
                    grantee_did: holder.to_string(),
                    resource_type: resource_id.clone(),
                    access_model_hash: blake3::hash(format!("{model:?}").as_bytes())
                        .to_hex()
                        .to_string(),
                })]
            }
            icn_governance::ResourceAccessAction::Revoke => {
                vec![KernelEffect::Resource(ResourceEffect::RevokeAccess {
                    grantee_did: holder.to_string(),
                    resource_type: resource_id.clone(),
                })]
            }
        },

        // Participatory budgeting: create an envelope budget, then allocate per option.
        //
        // CreateBudget defines the governance-authorized spending envelope (not a debit).
        // Each Allocate reserves a portion of the envelope for a specific option.
        // The budget_id includes the decision_receipt_id to prevent collision across
        // allocation rounds with the same purpose string.
        //
        // NOTE: treasury_did is String::new() — filled by caller context, same pattern
        // as Budget and other treasury translations. recipient attribution (from
        // AllocationOption::recipient) is not yet expressible in TreasuryEffect::Allocate;
        // it will be carried once the kernel type is extended.
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
                    amount: opt.requested_amount,
                    currency: unit.clone(),
                }));
            }
            effects
        }

        // Fallback for unhandled types
        _ => vec![KernelEffect::NoOp {
            reason: format!(
                "Unhandled proposal type: {:?}",
                std::mem::discriminant(payload)
            ),
        }],
    }
}

/// Translate treasury operations to kernel effects
fn translate_treasury_operation(
    operation: &icn_governance::TreasuryProposalOperation,
    decision_receipt_id: &str,
    decision_hash: &str,
) -> Vec<KernelEffect> {
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
        } => vec![KernelEffect::Treasury(TreasuryEffect::Spend {
            treasury_did: treasury_did.to_string(),
            recipient_did: recipient.to_string(),
            amount: *amount,
            currency: currency.clone(),
            memo: purpose.clone(),
            budget_id: budget_id.clone(),
            expected_nonce: *nonce,
            decision_receipt_id: decision_receipt_id.to_string(),
            decision_hash: decision_hash.to_string(),
        })],

        TreasuryProposalOperation::Spend {
            treasury_did,
            recipient,
            amount,
            currency,
            memo,
            nonce,
        } => vec![KernelEffect::Treasury(TreasuryEffect::Spend {
            treasury_did: treasury_did.to_string(),
            recipient_did: recipient.to_string(),
            amount: *amount,
            currency: currency.clone(),
            memo: memo.clone(),
            budget_id: None,
            expected_nonce: *nonce,
            decision_receipt_id: decision_receipt_id.to_string(),
            decision_hash: decision_hash.to_string(),
        })],

        TreasuryProposalOperation::CreateBudget {
            treasury_did,
            purpose,
            amount,
            currency,
            period_end,
        } => vec![KernelEffect::Treasury(TreasuryEffect::CreateBudget {
            treasury_did: treasury_did.to_string(),
            budget_id: format!("budget-{purpose}"),
            total_amount: *amount,
            currency: currency.clone(),
            name: purpose.clone(),
            validity_start: 0,
            validity_end: period_end.unwrap_or(u64::MAX),
            decision_receipt_id: decision_receipt_id.to_string(),
            decision_hash: decision_hash.to_string(),
        })],

        // Fallback for other treasury operations
        _ => vec![KernelEffect::NoOp {
            reason: format!(
                "Treasury operation not yet translated: {:?}",
                std::mem::discriminant(operation)
            ),
        }],
    }
}

/// Translate membership actions to kernel effects
fn translate_membership_action(
    action: &icn_governance::MembershipAction,
    member: &icn_identity::Did,
    domain_id: &str,
) -> Vec<KernelEffect> {
    use icn_governance::MembershipAction;

    match action {
        MembershipAction::Add => {
            vec![KernelEffect::Membership(MembershipEffect::AddMember {
                entity_id: domain_id.to_string(),
                member_did: member.to_string(),
                role: String::new(),
                tier: String::new(),
            })]
        }
        MembershipAction::Remove => {
            vec![KernelEffect::Membership(MembershipEffect::RemoveMember {
                entity_id: domain_id.to_string(),
                member_did: member.to_string(),
                reason: String::new(),
            })]
        }
    }
}

/// Translate federation proposals to kernel effects
fn translate_federation_proposal(
    proposal: &icn_governance::FederationProposal,
) -> Vec<KernelEffect> {
    use icn_governance::FederationProposal;

    match proposal {
        FederationProposal::JoinFederation { federation_id, .. } => {
            vec![KernelEffect::Federation(FederationEffect::JoinFederation {
                coop_did: String::new(),
                federation_id: federation_id.clone(),
            })]
        }
        FederationProposal::LeaveFederation { federation_id, .. } => {
            vec![KernelEffect::Federation(
                FederationEffect::LeaveFederation {
                    coop_did: String::new(),
                    federation_id: federation_id.clone(),
                },
            )]
        }
        FederationProposal::EstablishClearing {
            partner_coop_did, ..
        } => {
            vec![KernelEffect::Federation(
                FederationEffect::EstablishClearing {
                    coop_a_did: String::new(),
                    coop_b_did: partner_coop_did.to_string(),
                    agreement_hash: String::new(),
                },
            )]
        }
        FederationProposal::VouchForCooperative {
            target_coop_did, ..
        } => {
            vec![KernelEffect::Federation(FederationEffect::VouchForCoop {
                voucher_did: String::new(),
                vouchee_did: target_coop_did.to_string(),
                attestation_hash: String::new(),
            })]
        }
        // Fallback for other federation proposals
        _ => vec![KernelEffect::NoOp {
            reason: format!(
                "Federation proposal not yet translated: {:?}",
                std::mem::discriminant(proposal)
            ),
        }],
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
        );
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
    fn test_translate_unhandled_payload_to_noop() {
        let payload = icn_governance::ProposalPayload::ShareRedemption {
            member: "did:icn:zAKnL4NNf3DGWZJS6cPknBuEGnVsV4A4m5tgebLHaRSZ9"
                .parse()
                .expect("valid did"),
            share_ids: vec![],
            payout_schedule: vec![],
            reason: "voluntary departure".to_string(),
        };
        let effects = translate_payload_to_effects(
            &payload,
            "receipt-abc",
            "decision-hash-abc",
            "domain-translation-test",
        );
        assert_eq!(effects.len(), 1);
        assert!(matches!(effects[0], KernelEffect::NoOp { .. }));
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
            );
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
        );

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
                ..
            }) => {
                assert_eq!(budget_id, "alloc-q1-budget-receipt-alloc-1");
                assert_eq!(*amount, 6_000);
                assert_eq!(currency, "compute-hours");
            }
            other => panic!("expected Allocate for Infrastructure, got {other:?}"),
        }

        // Third effect: Allocate for second option
        match &effects[2] {
            KernelEffect::Treasury(TreasuryEffect::Allocate {
                budget_id,
                amount,
                currency,
                ..
            }) => {
                assert_eq!(budget_id, "alloc-q1-budget-receipt-alloc-1");
                assert_eq!(*amount, 4_000);
                assert_eq!(currency, "compute-hours");
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
}
