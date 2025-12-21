//! Comprehensive membership tests for communities
#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_community::types::MemberType;
use icn_community::*;

fn create_test_community() -> Community {
    let lifecycle = CommunityLifecycle::new(3);
    let request = FormationRequest {
        name: "Test Community".to_string(),
        community_type: CommunityType::Geographic,
        founding_members: vec![
            "did:icn:alice".to_string(),
            "did:icn:bob".to_string(),
            "did:icn:carol".to_string(),
        ],
        charter_ccl: String::new(),
    };
    lifecycle.form(request, "test-domain".to_string()).unwrap()
}

#[test]
fn test_add_individual_member() {
    let mut community = create_test_community();
    let mgr = MembershipManager::new();

    let result = mgr.add_member(
        &mut community,
        "did:icn:dave".to_string(),
        MemberType::Individual("did:icn:dave".to_string()),
        1,
    );

    assert!(result.is_ok());
    assert!(community.members.contains_key("did:icn:dave"));

    let member = &community.members["did:icn:dave"];
    assert!(matches!(member.member_type, MemberType::Individual(_)));
    assert_eq!(member.voting_weight, 1);
    assert!(member.active);
}

#[test]
fn test_add_cooperative_member() {
    let mut community = create_test_community();
    let mgr = MembershipManager::new();

    let result = mgr.add_member(
        &mut community,
        "coop:food-coop".to_string(),
        MemberType::Cooperative("coop:food-coop".to_string()),
        5,
    );

    assert!(result.is_ok());
    assert!(community.members.contains_key("coop:food-coop"));

    let member = &community.members["coop:food-coop"];
    assert!(matches!(member.member_type, MemberType::Cooperative(_)));
    assert_eq!(member.voting_weight, 5);
}

#[test]
fn test_remove_member() {
    let mut community = create_test_community();
    let mgr = MembershipManager::new();

    mgr.add_member(
        &mut community,
        "did:icn:dave".to_string(),
        MemberType::Individual("did:icn:dave".to_string()),
        1,
    )
    .unwrap();
    assert!(community.members["did:icn:dave"].active);

    let result = mgr.remove_member(&mut community, "did:icn:dave");
    assert!(result.is_ok());
    assert!(!community.members["did:icn:dave"].active);
}

#[test]
fn test_remove_nonexistent_member() {
    let mut community = create_test_community();
    let mgr = MembershipManager::new();

    let result = mgr.remove_member(&mut community, "did:icn:nobody");
    assert!(result.is_err());
}

#[test]
fn test_member_application() {
    let app = MemberApplication {
        community_id: "community:test".to_string(),
        applicant_id: "did:icn:eve".to_string(),
        member_type: MemberType::Individual("did:icn:eve".to_string()),
        statement: "I want to join the community".to_string(),
    };

    assert_eq!(app.community_id, "community:test");
    assert_eq!(app.applicant_id, "did:icn:eve");
    assert!(matches!(app.member_type, MemberType::Individual(_)));
}

#[test]
fn test_member_voting_weights() {
    let mut community = create_test_community();
    let mgr = MembershipManager::new();

    mgr.add_member(
        &mut community,
        "did:icn:alice".to_string(),
        MemberType::Individual("did:icn:alice".to_string()),
        1,
    )
    .unwrap();
    mgr.add_member(
        &mut community,
        "coop:big-coop".to_string(),
        MemberType::Cooperative("coop:big-coop".to_string()),
        10,
    )
    .unwrap();

    assert_eq!(community.members["did:icn:alice"].voting_weight, 1);
    assert_eq!(community.members["coop:big-coop"].voting_weight, 10);
}

#[test]
fn test_multiple_member_types() {
    let mut community = create_test_community();
    let mgr = MembershipManager::new();

    mgr.add_member(
        &mut community,
        "did:icn:person".to_string(),
        MemberType::Individual("did:icn:person".to_string()),
        1,
    )
    .unwrap();
    mgr.add_member(
        &mut community,
        "coop:worker".to_string(),
        MemberType::Cooperative("coop:worker".to_string()),
        5,
    )
    .unwrap();

    assert!(matches!(
        community.members["did:icn:person"].member_type,
        MemberType::Individual(_)
    ));
    assert!(matches!(
        community.members["coop:worker"].member_type,
        MemberType::Cooperative(_)
    ));
}
