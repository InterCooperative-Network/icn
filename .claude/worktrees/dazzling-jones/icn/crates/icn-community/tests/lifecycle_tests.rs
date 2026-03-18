//! Comprehensive lifecycle tests for communities
#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_community::types::MemberType;
use icn_community::*;

#[test]
fn test_community_formation() {
    let lifecycle = CommunityLifecycle::new(3);

    let request = FormationRequest {
        name: "Regional Food Network".to_string(),
        community_type: CommunityType::Geographic,
        founding_members: vec![
            "did:icn:alice".to_string(),
            "did:icn:bob".to_string(),
            "did:icn:carol".to_string(),
        ],
        charter_ccl: "rule \"all_welcome\" { true }".to_string(),
    };

    let result = lifecycle.form(request, "test-domain".to_string());
    assert!(result.is_ok());

    let community = result.unwrap();
    assert_eq!(community.name, "Regional Food Network");
    assert_eq!(community.community_type, CommunityType::Geographic);
    assert_eq!(community.status, CommunityStatus::Forming);
}

#[test]
fn test_formation_requires_minimum_founders() {
    let lifecycle = CommunityLifecycle::new(5);

    let request = FormationRequest {
        name: "Small Community".to_string(),
        community_type: CommunityType::Geographic,
        founding_members: vec!["did:icn:alice".to_string(), "did:icn:bob".to_string()],
        charter_ccl: String::new(),
    };

    let result = lifecycle.form(request, "test-domain".to_string());
    assert!(result.is_err());
}

#[test]
fn test_community_activation() {
    let lifecycle = CommunityLifecycle::new(3);

    let request = FormationRequest {
        name: "Tech Community".to_string(),
        community_type: CommunityType::Interest,
        founding_members: vec![
            "did:icn:alice".to_string(),
            "did:icn:bob".to_string(),
            "did:icn:carol".to_string(),
        ],
        charter_ccl: String::new(),
    };

    let mut community = lifecycle.form(request, "test-domain".to_string()).unwrap();
    assert_eq!(community.status, CommunityStatus::Forming);

    let result = lifecycle.activate(&mut community);
    assert!(result.is_ok());
    assert_eq!(community.status, CommunityStatus::Active);
}

#[test]
fn test_cannot_activate_already_active() {
    let lifecycle = CommunityLifecycle::new(3);

    let request = FormationRequest {
        name: "Test Community".to_string(),
        community_type: CommunityType::Interest,
        founding_members: vec![
            "did:icn:alice".to_string(),
            "did:icn:bob".to_string(),
            "did:icn:carol".to_string(),
        ],
        charter_ccl: String::new(),
    };

    let mut community = lifecycle.form(request, "test-domain".to_string()).unwrap();
    lifecycle.activate(&mut community).unwrap();

    // Try to activate again
    let result = lifecycle.activate(&mut community);
    assert!(result.is_err());
}

#[test]
fn test_community_dissolution() {
    let lifecycle = CommunityLifecycle::new(3);

    let request = FormationRequest {
        name: "Temp Community".to_string(),
        community_type: CommunityType::Solidarity,
        founding_members: vec![
            "did:icn:alice".to_string(),
            "did:icn:bob".to_string(),
            "did:icn:carol".to_string(),
        ],
        charter_ccl: String::new(),
    };

    let mut community = lifecycle.form(request, "test-domain".to_string()).unwrap();
    lifecycle.activate(&mut community).unwrap();

    // Add some members
    let mgr = MembershipManager::new();
    mgr.add_member(
        &mut community,
        "did:icn:dave".to_string(),
        MemberType::Individual("did:icn:dave".to_string()),
        1,
    )
    .unwrap();

    // Dissolve
    let result = lifecycle.dissolve(&mut community);
    assert!(result.is_ok());
    assert_eq!(community.status, CommunityStatus::Dissolved);

    // All members should be inactive
    for member in community.members.values() {
        assert!(!member.active);
    }
}

#[test]
fn test_community_types() {
    assert_ne!(CommunityType::Geographic, CommunityType::Interest);
    assert_ne!(CommunityType::Interest, CommunityType::Solidarity);
    assert_ne!(CommunityType::Solidarity, CommunityType::Ecosystem);
}
