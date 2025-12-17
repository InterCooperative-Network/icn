//! Integration tests for community management

use icn_community::*;
use icn_community::types::*;

#[test]
fn test_community_types() {
    let geo = CommunityType::Geographic;
    let interest = CommunityType::Interest;
    let solidarity = CommunityType::Solidarity;
    let ecosystem = CommunityType::Ecosystem;
    
    assert_ne!(geo, interest);
    assert_ne!(interest, solidarity);
    assert_ne!(solidarity, ecosystem);
}

#[test]
fn test_community_status() {
    assert_eq!(CommunityStatus::Forming, CommunityStatus::Forming);
    assert_ne!(CommunityStatus::Forming, CommunityStatus::Active);
}

#[test]
fn test_community_creation() {
    let community = Community::new(
        "community-1".to_string(),
        "Test Community".to_string(),
        CommunityType::Interest,
        "domain-1".to_string(),
    );
    
    assert_eq!(community.name, "Test Community");
    assert_eq!(community.community_type, CommunityType::Interest);
    assert_eq!(community.status, CommunityStatus::Forming);
    assert_eq!(community.active_member_count(), 0);
}

#[test]
fn test_resource_pool_creation() {
    let pool = ResourcePool {
        name: "Shared Compute".to_string(),
        resource_type: "compute".to_string(),
        total_capacity: 1000,
        allocated: 200,
        unit: "CPU-hours".to_string(),
    };
    
    assert_eq!(pool.available(), 800);
    assert!(pool.can_allocate(500));
    assert!(!pool.can_allocate(1000));
}

#[test]
fn test_resource_pool_allocation() {
    let mut pool = ResourcePool {
        name: "Storage".to_string(),
        resource_type: "storage".to_string(),
        total_capacity: 1000,
        allocated: 0,
        unit: "GB".to_string(),
    };
    
    assert_eq!(pool.available(), 1000);
    
    pool.allocated += 300;
    assert_eq!(pool.available(), 700);
    assert!(pool.can_allocate(700));
}

#[test]
fn test_member_types() {
    let individual = MemberType::Individual("did:icn:alice".to_string());
    let coop = MemberType::Cooperative("coop-1".to_string());
    
    match individual {
        MemberType::Individual(did) => assert_eq!(did, "did:icn:alice"),
        _ => panic!("Wrong type"),
    }
    
    match coop {
        MemberType::Cooperative(id) => assert_eq!(id, "coop-1"),
        _ => panic!("Wrong type"),
    }
}

#[test]
fn test_community_lifecycle_formation() {
    let lifecycle = CommunityLifecycle::new(2);
    
    let request = FormationRequest {
        name: "New Community".to_string(),
        community_type: CommunityType::Geographic,
        founding_members: vec![
            "did:icn:alice".to_string(),
            "did:icn:bob".to_string(),
        ],
        charter_ccl: "charter_ccl".to_string(),
    };
    
    let result = lifecycle.form(request, "domain-1".to_string());
    assert!(result.is_ok());
    
    let community = result.unwrap();
    assert_eq!(community.name, "New Community");
    assert_eq!(community.status, CommunityStatus::Forming);
}

#[test]
fn test_community_activation() {
    let lifecycle = CommunityLifecycle::new(2);
    
    let request = FormationRequest {
        name: "Test Community".to_string(),
        community_type: CommunityType::Interest,
        founding_members: vec![
            "did:icn:alice".to_string(),
            "did:icn:bob".to_string(),
        ],
        charter_ccl: "charter".to_string(),
    };
    
    let mut community = lifecycle.form(request, "domain".to_string()).unwrap();
    assert_eq!(community.status, CommunityStatus::Forming);
    
    lifecycle.activate(&mut community).unwrap();
    assert_eq!(community.status, CommunityStatus::Active);
}

#[test]
fn test_membership_add_member() {
    let mut community = Community::new(
        "community-1".to_string(),
        "Test Community".to_string(),
        CommunityType::Geographic,
        "domain-1".to_string(),
    );
    
    let manager = MembershipManager::new();
    let member_type = MemberType::Individual("did:icn:alice".to_string());
    
    let result = manager.add_member(&mut community, "did:icn:alice".to_string(), member_type, 1);
    assert!(result.is_ok());
    assert_eq!(community.members.len(), 1);
}

#[test]
fn test_membership_remove_member() {
    let mut community = Community::new(
        "community-1".to_string(),
        "Test Community".to_string(),
        CommunityType::Geographic,
        "domain-1".to_string(),
    );
    
    let manager = MembershipManager::new();
    let member_type = MemberType::Individual("did:icn:alice".to_string());
    
    manager.add_member(&mut community, "did:icn:alice".to_string(), member_type, 1).unwrap();
    assert_eq!(community.active_member_count(), 1);
    
    manager.remove_member(&mut community, "did:icn:alice").unwrap();
    assert_eq!(community.active_member_count(), 0);
}

#[test]
fn test_resource_manager() {
    let mut community = Community::new(
        "community-1".to_string(),
        "Test Community".to_string(),
        CommunityType::Geographic,
        "domain-1".to_string(),
    );
    
    let manager = ResourceManager::new();
    manager.create_pool(&mut community, "compute".to_string(), "CPU".to_string(), 1000, "hours".to_string()).unwrap();
    
    assert_eq!(community.resource_pools.len(), 1);
}

#[test]
fn test_resource_allocation_struct() {
    let allocation = ResourceAllocation {
        pool_name: "compute".to_string(),
        member_id: "did:icn:alice".to_string(),
        amount: 100,
        expires_at: None,
    };
    
    assert_eq!(allocation.amount, 100);
    assert_eq!(allocation.pool_name, "compute");
}
