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
            action: ResourceAccessAction::Grant,
            resource_id: "workshop-tools-001".to_string(),
            holder: holder.clone(),
            model: Some(AccessModel::UseAccess {
                duration_seconds: 7 * 24 * 3600, // 1 week
                renewable: true,
                max_accumulated: 4,
            }),
            reason: "Active member needs tools for community project".to_string(),
        },
    );

    assert_eq!(proposal.title, "Grant Tool Access");
    assert_eq!(proposal.proposer, proposer);
    assert!(matches!(proposal.state, icn_governance::ProposalState::Draft));
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
            model: None, // No model needed for revocation
            reason: "Repeated violations of tool usage policy".to_string(),
        },
    );

    assert_eq!(proposal.title, "Revoke Tool Access");
    assert!(matches!(proposal.state, icn_governance::ProposalState::Draft));
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
            action: ResourceAccessAction::Grant,
            resource_id: "community-garden-001".to_string(),
            holder: holder.clone(),
            model: Some(AccessModel::Stewardship {
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
            }),
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
        action: ResourceAccessAction::Grant,
        resource_id: "tool-001".to_string(),
        holder: entity.clone(),
        model: Some(AccessModel::UseAccess {
            duration_seconds: 7 * 24 * 3600,
            renewable: true,
            max_accumulated: 4,
        }),
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
        action: ResourceAccessAction::Grant,
        resource_id: "tool-001".to_string(),
        holder: entity.clone(),
        model: Some(AccessModel::UseAccess {
            duration_seconds: 7 * 24 * 3600,
            renewable: true,
            max_accumulated: 4,
        }),
        reason: "Community project".to_string(),
    };

    assert_eq!(payload.type_name(), "resource_access");
    assert!(!payload.is_emergency());
}
