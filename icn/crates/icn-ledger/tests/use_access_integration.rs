//! Integration test for use-based resource access with governance
#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_entity::EntityId;
use icn_governance::{
    GovernanceConfig, GovernanceDomainId, Proposal, ProposalPayload, ResourceAccessAction,
};
use icn_identity::KeyPair;
use icn_ledger::{AccessModel, AntiSpeculationRules, ResourceAccess, StewardshipDuty};

#[test]
fn test_resource_access_proposal_creation() {
    let proposer_keypair = KeyPair::generate().unwrap();
    let proposer = proposer_keypair.did().clone();
    let holder_keypair = KeyPair::generate().unwrap();
    let holder = EntityId::from_did(holder_keypair.did());

    let domain_id = GovernanceDomainId::new("test-coop");

    // Create a proposal to grant resource access
    let proposal = Proposal::new(
        domain_id.clone(),
        proposer.clone(),
        "Grant Tool Access".to_string(),
        "Grant Alice access to the community workshop tools".to_string(),
        ProposalPayload::ResourceAccess {
            action: ResourceAccessAction::Grant {
                model: AccessModel::UseAccess {
                    duration_seconds: 7 * 24 * 3600, // 1 week
                    renewable: true,
                    max_accumulated: 4,
                },
            },
            resource_id: "workshop-tools-001".to_string(),
            holder: holder.clone(),
            reason: "Active member needs tools for community project".to_string(),
        },
    );

    assert_eq!(proposal.title, "Grant Tool Access");
    assert_eq!(proposal.proposer, proposer);
    assert!(matches!(
        proposal.state,
        icn_governance::ProposalState::Draft
    ));
}

#[test]
fn test_resource_access_revocation_proposal() {
    let proposer_keypair = KeyPair::generate().unwrap();
    let proposer = proposer_keypair.did().clone();
    let holder_keypair = KeyPair::generate().unwrap();
    let holder = EntityId::from_did(holder_keypair.did());

    let domain_id = GovernanceDomainId::new("test-coop");

    // Create a proposal to revoke resource access
    let proposal = Proposal::new(
        domain_id.clone(),
        proposer.clone(),
        "Revoke Tool Access".to_string(),
        "Revoke access due to violation of usage policy".to_string(),
        ProposalPayload::ResourceAccess {
            action: ResourceAccessAction::Revoke,
            resource_id: "workshop-tools-001".to_string(),
            holder: holder.clone(),
            reason: "Repeated violations of tool usage policy".to_string(),
        },
    );

    assert_eq!(proposal.title, "Revoke Tool Access");
    assert!(matches!(
        proposal.state,
        icn_governance::ProposalState::Draft
    ));
}

#[test]
fn test_stewardship_access_proposal() {
    let proposer_keypair = KeyPair::generate().unwrap();
    let proposer = proposer_keypair.did().clone();
    let holder_keypair = KeyPair::generate().unwrap();
    let holder = EntityId::from_did(holder_keypair.did());

    let domain_id = GovernanceDomainId::new("test-coop");

    // Create a proposal for stewardship-based access
    let proposal = Proposal::new(
        domain_id.clone(),
        proposer.clone(),
        "Grant Garden Stewardship".to_string(),
        "Appoint steward for community garden".to_string(),
        ProposalPayload::ResourceAccess {
            action: ResourceAccessAction::Grant {
                model: AccessModel::Stewardship {
                    duties: vec![
                        StewardshipDuty::Maintenance {
                            description: "Water plants and maintain beds".to_string(),
                            frequency_seconds: 7 * 24 * 3600, // Weekly
                        },
                        StewardshipDuty::UsageReporting {
                            min_reports: 4,
                            period_seconds: 30 * 24 * 3600, // Monthly
                        },
                        StewardshipDuty::CommunityBenefit {
                            description: "Host monthly gardening workshops".to_string(),
                            due_by: icn_time::current_timestamp_secs() + 90 * 24 * 3600, // 90 days
                        },
                    ],
                    review_period_seconds: 90 * 24 * 3600, // Quarterly review
                },
            },
            resource_id: "community-garden-001".to_string(),
            holder: holder.clone(),
            reason: "Experienced gardener with community engagement".to_string(),
        },
    );

    assert_eq!(proposal.title, "Grant Garden Stewardship");
}

#[test]
fn test_resource_access_lifecycle() {
    let entity = EntityId::from_did(KeyPair::generate().unwrap().did());

    // Create use-based access
    let mut access = ResourceAccess::new(
        "tool-001".to_string(),
        entity.clone(),
        AccessModel::UseAccess {
            duration_seconds: 7 * 24 * 3600,
            renewable: true,
            max_accumulated: 4,
        },
    )
    .with_rules(AntiSpeculationRules::standard());

    // Access is valid initially
    let current_time = access.granted_at;
    assert!(access.is_valid(current_time));

    // Record usage
    access
        .record_usage(current_time, "Used for repairs".to_string())
        .unwrap();
    assert_eq!(access.usage_log.len(), 1);

    // Renew access
    let renew_time = current_time + 6 * 24 * 3600; // 6 days later
    access.renew(renew_time).unwrap();
    assert_eq!(access.renewal_count, 1);

    // Validate anti-speculation rules
    assert!(access.validate_rules(renew_time).is_ok());

    // No profit transfer allowed
    assert!(access.validate_transfer(Some(100)).is_err());
    assert!(access.validate_transfer(None).is_ok());
}

#[test]
fn test_idle_revocation_scenario() {
    let entity = EntityId::from_did(KeyPair::generate().unwrap().did());

    let mut access = ResourceAccess::new(
        "tool-001".to_string(),
        entity.clone(),
        AccessModel::UseAccess {
            duration_seconds: 90 * 24 * 3600, // 90 days
            renewable: true,
            max_accumulated: 4,
        },
    )
    .with_rules(AntiSpeculationRules::strict()); // 7-day idle limit

    let current_time = access.granted_at;

    // Record initial usage
    access
        .record_usage(current_time, "Initial use".to_string())
        .unwrap();

    // After 6 days - still OK
    let time_6_days = current_time + 6 * 24 * 3600;
    assert!(access.validate_rules(time_6_days).is_ok());

    // After 8 days - idle too long
    let time_8_days = current_time + 8 * 24 * 3600;
    assert!(access.validate_rules(time_8_days).is_err());
}

#[test]
fn test_stewardship_duty_validation() {
    let entity = EntityId::from_did(KeyPair::generate().unwrap().did());

    let mut access = ResourceAccess::new(
        "garden-001".to_string(),
        entity.clone(),
        AccessModel::Stewardship {
            duties: vec![StewardshipDuty::Maintenance {
                description: "Weekly watering".to_string(),
                frequency_seconds: 7 * 24 * 3600,
            }],
            review_period_seconds: 90 * 24 * 3600,
        },
    );

    let current_time = access.granted_at + 8 * 24 * 3600; // 8 days later

    // No maintenance recorded - should fail
    assert!(access.check_duties(current_time).is_err());

    // Record maintenance
    let maintenance_time = access.granted_at + 6 * 24 * 3600;
    access
        .record_usage(maintenance_time, "Performed maintenance".to_string())
        .unwrap();

    // Should pass now
    assert!(access.check_duties(maintenance_time + 3600).is_ok());
}

#[test]
fn test_governance_thresholds_for_resource_access() {
    let config = GovernanceConfig::cooperative_default();

    let entity = EntityId::from_did(KeyPair::generate().unwrap().did());

    // Resource access should use normal thresholds
    let payload = ProposalPayload::ResourceAccess {
        action: ResourceAccessAction::Grant {
            model: AccessModel::UseAccess {
                duration_seconds: 7 * 24 * 3600,
                renewable: true,
                max_accumulated: 4,
            },
        },
        resource_id: "tool-001".to_string(),
        holder: entity.clone(),
        reason: "Community project".to_string(),
    };

    let thresholds = config.thresholds_for_proposal(&payload);

    // Should use normal cooperative thresholds
    assert_eq!(
        thresholds.quorum_percentage,
        config.params.quorum_percentage
    );
    assert_eq!(
        thresholds.approval_percentage,
        config.params.approval_threshold_percentage
    );
}

#[test]
fn test_proposal_payload_type_name() {
    let entity = EntityId::from_did(KeyPair::generate().unwrap().did());

    let payload = ProposalPayload::ResourceAccess {
        action: ResourceAccessAction::Grant {
            model: AccessModel::UseAccess {
                duration_seconds: 7 * 24 * 3600,
                renewable: true,
                max_accumulated: 4,
            },
        },
        resource_id: "tool-001".to_string(),
        holder: entity.clone(),
        reason: "Community project".to_string(),
    };

    assert_eq!(payload.type_name(), "resource_access");
    assert!(!payload.is_emergency());
}

#[tokio::test]
async fn test_resource_access_transfer_with_events() {
    use icn_ledger::{
        create_shared_emitter, LedgerEvent, ResourceAccessStore, SledResourceAccessStore,
    };
    use icn_store::SledStore;
    use std::sync::Arc;

    // Setup
    let store = Arc::new(SledStore::temporary().unwrap());
    let access_store = SledResourceAccessStore::new(store);
    let event_emitter = create_shared_emitter();
    let mut receiver = event_emitter.subscribe();

    let alice_keypair = KeyPair::generate().unwrap();
    let alice = EntityId::from_did(alice_keypair.did());
    let bob_keypair = KeyPair::generate().unwrap();
    let bob = EntityId::from_did(bob_keypair.did());

    // Create and store access for Alice
    let access = icn_ledger::ResourceAccess::new(
        "community-tool".to_string(),
        alice.clone(),
        icn_ledger::AccessModel::UseAccess {
            duration_seconds: 7 * 24 * 3600, // 1 week
            renewable: true,
            max_accumulated: 4,
        },
    )
    .with_rules(icn_ledger::AntiSpeculationRules::standard());

    access_store.put(&access).unwrap();

    // Transfer to Bob (free transfer - should succeed)
    let current_time = access.granted_at + 3600;
    let (old_holder, updated_access) = access_store
        .transfer("community-tool", bob.clone(), None, current_time)
        .unwrap();

    // Verify transfer
    assert_eq!(old_holder, alice);
    assert_eq!(updated_access.holder, bob);

    // Emit event
    let access_model_str = match updated_access.model {
        icn_ledger::AccessModel::UseAccess { .. } => "UseAccess",
        icn_ledger::AccessModel::Stewardship { .. } => "Stewardship",
    };

    event_emitter.emit_resource_access_transferred(
        updated_access.resource_id.clone(),
        old_holder.to_string(),
        updated_access.holder.to_string(),
        None,
        access_model_str.to_string(),
        current_time,
    );

    // Verify event was emitted
    let event = tokio::time::timeout(std::time::Duration::from_secs(1), receiver.recv())
        .await
        .expect("timeout")
        .expect("receive error");

    match event {
        LedgerEvent::ResourceAccessTransferred(transfer_event) => {
            assert_eq!(transfer_event.resource_id, "community-tool");
            assert_eq!(transfer_event.from_holder, alice.to_string());
            assert_eq!(transfer_event.to_holder, bob.to_string());
            assert_eq!(transfer_event.price, None);
            assert_eq!(transfer_event.access_model, "UseAccess");
            assert_eq!(transfer_event.transferred_at, current_time);
        }
        _ => panic!("Expected ResourceAccessTransferred event"),
    }
}

#[test]
fn test_stewardship_handoff_procedure_complete_flow() {
    use icn_ledger::{ResourceAccessStore, SledResourceAccessStore};
    use icn_store::SledStore;
    use std::sync::Arc;

    let store = Arc::new(SledStore::temporary().unwrap());
    let access_store = SledResourceAccessStore::new(store);

    let alice_keypair = KeyPair::generate().unwrap();
    let alice = EntityId::from_did(alice_keypair.did());
    let bob_keypair = KeyPair::generate().unwrap();
    let bob = EntityId::from_did(bob_keypair.did());

    let handoff_steps = vec![
        "Document maintenance schedule".to_string(),
        "Train incoming steward".to_string(),
        "Transfer access keys".to_string(),
    ];

    // Create stewardship access with handoff procedure
    let mut access = icn_ledger::ResourceAccess::new(
        "community-workshop".to_string(),
        alice.clone(),
        icn_ledger::AccessModel::Stewardship {
            duties: vec![icn_ledger::StewardshipDuty::HandoffProcedure {
                steps: handoff_steps.clone(),
            }],
            review_period_seconds: 90 * 24 * 3600,
        },
    );

    let current_time = access.granted_at + 1000;

    // Complete handoff steps
    access
        .record_usage(
            current_time,
            "Document maintenance schedule completed".to_string(),
        )
        .unwrap();
    access
        .record_usage(
            current_time + 100,
            "Train incoming steward - completed training session".to_string(),
        )
        .unwrap();
    access
        .record_usage(
            current_time + 200,
            "Transfer access keys - keys securely transferred".to_string(),
        )
        .unwrap();

    access_store.put(&access).unwrap();

    // Now transfer should succeed
    let (old_holder, updated_access) = access_store
        .transfer("community-workshop", bob.clone(), None, current_time + 300)
        .unwrap();

    assert_eq!(old_holder, alice);
    assert_eq!(updated_access.holder, bob);

    // Verify persisted
    let retrieved = access_store.get("community-workshop").unwrap().unwrap();
    assert_eq!(retrieved.holder, bob);
    assert_eq!(retrieved.usage_log.len(), 3); // All handoff steps recorded
}
