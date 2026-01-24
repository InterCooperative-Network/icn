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

// Integration tests for ResourceAccessStore
// TODO: These tests are aspirational - ResourceAccessStore and SledResourceAccessStore
// are not yet implemented. Uncomment when store implementation is added.
/*
mod store_integration {
    use super::*;
    use icn_ledger::{ResourceAccessStore, SledResourceAccessStore};
    use icn_store::SledStore;
    use std::sync::Arc;

    #[test]
    fn test_cooperative_tool_access_scenario() {
        // Setup: Create a store for a cooperative's resource access records
        let store = Arc::new(SledStore::temporary().unwrap());
        let access_store = SledResourceAccessStore::new(store);

        // Three members want to access the cooperative's woodworking shop
        let alice = EntityId::from_did(KeyPair::generate().unwrap().did());
        let bob = EntityId::from_did(KeyPair::generate().unwrap().did());
        let _carol = EntityId::from_did(KeyPair::generate().unwrap().did());

        // Grant access to Alice and Bob
        let alice_access = ResourceAccess::new(
            "woodworking-shop".to_string(),
            alice.clone(),
            AccessModel::UseAccess {
                duration_seconds: 30 * 24 * 3600, // 30 days
                renewable: true,
                max_accumulated: 4,
            },
        );

        let bob_access = ResourceAccess::new(
            "woodworking-shop".to_string(),
            bob.clone(),
            AccessModel::UseAccess {
                duration_seconds: 30 * 24 * 3600,
                renewable: true,
                max_accumulated: 4,
            },
        );

        access_store.grant(alice_access.clone()).unwrap();
        access_store.grant(bob_access.clone()).unwrap();

        // List all access for the woodworking shop
        let shop_accesses = access_store.list_by_resource("woodworking-shop").unwrap();
        assert_eq!(shop_accesses.len(), 2);

        // Alice has access to multiple resources
        let alice_tool_access = ResourceAccess::new(
            "table-saw".to_string(),
            alice.clone(),
            AccessModel::UseAccess {
                duration_seconds: 7 * 24 * 3600, // 7 days
                renewable: true,
                max_accumulated: 4,
            },
        );
        access_store.grant(alice_tool_access).unwrap();

        // Check Alice's total access grants
        let alice_accesses = access_store.list_by_holder(&alice).unwrap();
        assert_eq!(alice_accesses.len(), 2);

        // Revoke Bob's access due to policy violation
        access_store
            .revoke(
                "woodworking-shop",
                &bob,
                "Repeated safety violations".to_string(),
            )
            .unwrap();

        // Verify revocation
        let bob_access_after = access_store.get("woodworking-shop", &bob).unwrap().unwrap();
        assert!(bob_access_after.is_revoked());
        assert_eq!(
            bob_access_after.revocation_reason,
            Some("Repeated safety violations".to_string())
        );
    }

    #[test]
    fn test_expired_access_cleanup() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let access_store = SledResourceAccessStore::new(store);

        let alice = EntityId::from_did(KeyPair::generate().unwrap().did());
        let bob = EntityId::from_did(KeyPair::generate().unwrap().did());

        // Grant short-term access to Alice (1 hour)
        let mut alice_access = ResourceAccess::new(
            "guest-wifi".to_string(),
            alice.clone(),
            AccessModel::UseAccess {
                duration_seconds: 3600, // 1 hour
                renewable: false,
                max_accumulated: 1,
            },
        );
        let grant_time = 1000;
        alice_access.granted_at = grant_time;
        alice_access.expires_at = Some(grant_time + 3600);

        // Grant long-term stewardship to Bob
        let bob_access = ResourceAccess::new(
            "community-garden".to_string(),
            bob.clone(),
            AccessModel::Stewardship {
                duties: vec![],
                review_period_seconds: 90 * 24 * 3600,
            },
        );

        access_store.grant(alice_access).unwrap();
        access_store.grant(bob_access).unwrap();

        // Check expired access after 2 hours
        let current_time = grant_time + 7200;
        let expired = access_store.find_expired(current_time).unwrap();

        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].resource_id, "guest-wifi");
        assert_eq!(expired[0].holder, alice);
    }

    #[test]
    fn test_idle_access_detection() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let access_store = SledResourceAccessStore::new(store);

        let alice = EntityId::from_did(KeyPair::generate().unwrap().did());
        let bob = EntityId::from_did(KeyPair::generate().unwrap().did());

        // Alice hasn't used her access
        let mut alice_access = ResourceAccess::new(
            "3d-printer".to_string(),
            alice.clone(),
            AccessModel::UseAccess {
                duration_seconds: 30 * 24 * 3600,
                renewable: true,
                max_accumulated: 4,
            },
        );
        let grant_time = 1000;
        alice_access.granted_at = grant_time;

        // Bob used his access recently
        let mut bob_access = ResourceAccess::new(
            "laser-cutter".to_string(),
            bob.clone(),
            AccessModel::UseAccess {
                duration_seconds: 30 * 24 * 3600,
                renewable: true,
                max_accumulated: 4,
            },
        );
        bob_access.granted_at = grant_time;

        // Record Bob's usage within the idle period
        let current_time = grant_time + 10 * 24 * 3600; // 10 days later
        bob_access
            .record_usage(current_time - 3 * 24 * 3600, "Used for project".to_string())
            .unwrap();

        access_store.grant(alice_access).unwrap();
        access_store.grant(bob_access).unwrap();

        // Find idle access (7 day threshold)
        let max_idle = 7 * 24 * 3600;
        let idle = access_store.find_idle(current_time, max_idle).unwrap();

        assert_eq!(idle.len(), 1);
        assert_eq!(idle[0].resource_id, "3d-printer");
        assert_eq!(idle[0].holder, alice);
    }

    #[test]
    fn test_resource_access_governance_workflow() {
        let store = Arc::new(SledStore::temporary().unwrap());
        let access_store = SledResourceAccessStore::new(store);

        // Scenario: Governance proposal to grant access is approved
        let alice = EntityId::from_did(KeyPair::generate().unwrap().did());

        // Create governance proposal for access
        let domain_id = GovernanceDomainId::new("maker-coop");
        let proposer_keypair = KeyPair::generate().unwrap();

        let _proposal = Proposal::new(
            domain_id,
            proposer_keypair.did().clone(),
            "Grant CNC Machine Access".to_string(),
            "Alice has completed safety training and needs CNC access for project".to_string(),
            ProposalPayload::ResourceAccess {
                action: ResourceAccessAction::Grant {
                    model: AccessModel::UseAccess {
                        duration_seconds: 90 * 24 * 3600, // 90 days
                        renewable: true,
                        max_accumulated: 4,
                    },
                },
                resource_id: "cnc-machine".to_string(),
                holder: alice.clone(),
                reason: "Completed safety certification".to_string(),
            },
        );

        // After proposal passes (simulated), grant access
        let access = ResourceAccess::new(
            "cnc-machine".to_string(),
            alice.clone(),
            AccessModel::UseAccess {
                duration_seconds: 90 * 24 * 3600,
                renewable: true,
                max_accumulated: 4,
            },
        );

        access_store.grant(access).unwrap();

        // Verify access was granted
        let granted_access = access_store.get("cnc-machine", &alice).unwrap();
        assert!(granted_access.is_some());
        let access = granted_access.unwrap();
        assert_eq!(access.resource_id, "cnc-machine");
        assert!(access.is_valid(access.granted_at));
    }
}
*/
