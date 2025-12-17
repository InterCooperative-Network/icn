//! Integration tests for cooperative lifecycle and membership

use icn_cooperative::*;
use icn_store::{Store, SledStore};
use std::collections::HashMap;
use std::sync::Arc;

#[test]
fn test_cooperative_types() {
    let worker = CooperativeType::Worker;
    let consumer = CooperativeType::Consumer;
    let producer = CooperativeType::Producer;
    
    assert_ne!(worker, consumer);
    assert_ne!(consumer, producer);
}

#[test]
fn test_cooperative_formation() {
    let lifecycle = CooperativeLifecycle::new(3);
    
    let request = FormationRequest {
        name: "Test Worker Coop".to_string(),
        coop_type: CooperativeType::Worker,
        founding_members: vec![
            "did:icn:alice".to_string(),
            "did:icn:bob".to_string(),
            "did:icn:carol".to_string(),
        ],
        min_members: 3,
        tiers: vec![],
        bylaws_ccl: vec![],
        initial_capital: HashMap::new(),
    };
    
    let result = lifecycle.form(request, "test-domain".to_string());
    assert!(result.is_ok());
    
    let coop = result.unwrap();
    assert_eq!(coop.name, "Test Worker Coop");
    assert_eq!(coop.coop_type, CooperativeType::Worker);
    assert_eq!(coop.member_count(), 3);
    assert_eq!(coop.status, CooperativeStatus::Forming);
}

#[test]
fn test_formation_requires_minimum_founders() {
    let lifecycle = CooperativeLifecycle::new(5); // Require 5 founders
    
    let request = FormationRequest {
        name: "Test Coop".to_string(),
        coop_type: CooperativeType::Worker,
        founding_members: vec!["did:icn:alice".to_string()], // Only 1
        min_members: 3,
        tiers: vec![],
        bylaws_ccl: vec![],
        initial_capital: HashMap::new(),
    };
    
    let result = lifecycle.form(request, "test-domain".to_string());
    assert!(result.is_err());
}

#[test]
fn test_cooperative_activation() {
    let lifecycle = CooperativeLifecycle::new(3);
    
    let request = FormationRequest {
        name: "Test Coop".to_string(),
        coop_type: CooperativeType::Worker,
        founding_members: vec![
            "did:icn:alice".to_string(),
            "did:icn:bob".to_string(),
            "did:icn:carol".to_string(),
        ],
        min_members: 3,
        tiers: vec![],
        bylaws_ccl: vec![],
        initial_capital: HashMap::new(),
    };
    
    let mut coop = lifecycle.form(request, "test-domain".to_string()).unwrap();
    assert_eq!(coop.status, CooperativeStatus::Forming);
    
    let result = lifecycle.activate(&mut coop);
    assert!(result.is_ok());
    assert_eq!(coop.status, CooperativeStatus::Active);
}

#[test]
fn test_membership_add_member() {
    let mut coop = Cooperative::new(
        "test-coop".to_string(),
        "Test Coop".to_string(),
        CooperativeType::Worker,
        "domain".to_string(),
        3,
    );
    
    coop.tiers.push(MembershipTier {
        name: "Worker".to_string(),
        voting_weight: 1,
        profit_share_weight: 1,
        governance_rights: vec![],
    });
    
    let manager = MembershipManager::new(0.3);
    let result = manager.add_member(&mut coop, "did:icn:alice".to_string(), "Worker", 1000);
    
    assert!(result.is_ok());
    assert_eq!(coop.members.len(), 1);
    assert_eq!(coop.capital_pool, 1000);
}

#[test]
fn test_membership_remove_member() {
    let mut coop = Cooperative::new(
        "test-coop".to_string(),
        "Test Coop".to_string(),
        CooperativeType::Worker,
        "domain".to_string(),
        3,
    );
    
    coop.tiers.push(MembershipTier {
        name: "Worker".to_string(),
        voting_weight: 1,
        profit_share_weight: 1,
        governance_rights: vec![],
    });
    
    let manager = MembershipManager::new(0.3);
    manager.add_member(&mut coop, "did:icn:alice".to_string(), "Worker", 1000).unwrap();
    
    let result = manager.remove_member(&mut coop, "did:icn:alice", true);
    assert!(result.is_ok());
    assert_eq!(coop.capital_pool, 0);
}

#[test]
fn test_cooperative_store() {
    let store: Arc<dyn Store> = Arc::new(SledStore::temporary().unwrap());
    let coop_store = CooperativeStore::new(store);
    
    let coop = Cooperative::new(
        "test-id".to_string(),
        "Test Coop".to_string(),
        CooperativeType::Consumer,
        "domain".to_string(),
        3,
    );
    
    coop_store.put(&coop).unwrap();
    
    let retrieved = coop_store.get(&"test-id".to_string()).unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().name, "Test Coop");
}

#[test]
fn test_cooperative_query_by_status() {
    let store: Arc<dyn Store> = Arc::new(SledStore::temporary().unwrap());
    let coop_store = CooperativeStore::new(store);
    
    let coop = Cooperative::new(
        "test-id".to_string(),
        "Test Coop".to_string(),
        CooperativeType::Worker,
        "domain".to_string(),
        3,
    );
    
    coop_store.put(&coop).unwrap();
    
    let query = CooperativeQuery {
        status: Some(CooperativeStatus::Forming),
        coop_type: None,
        member_did: None,
    };
    
    let result = coop_store.query(&query);
    assert!(result.is_ok());
    let results = result.unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn test_cooperative_dissolution() {
    let lifecycle = CooperativeLifecycle::new(3);
    
    let request = FormationRequest {
        name: "Test Coop".to_string(),
        coop_type: CooperativeType::Worker,
        founding_members: vec![
            "did:icn:alice".to_string(),
            "did:icn:bob".to_string(),
            "did:icn:carol".to_string(),
        ],
        min_members: 3,
        tiers: vec![],
        bylaws_ccl: vec![],
        initial_capital: HashMap::new(),
    };
    
    let mut coop = lifecycle.form(request, "test-domain".to_string()).unwrap();
    lifecycle.activate(&mut coop).unwrap();
    
    let dissolution = DissolutionRequest {
        coop_id: coop.id.clone(),
        reason: "Project complete".to_string(),
        asset_distribution: HashMap::new(),
    };
    
    let result = lifecycle.begin_dissolution(&mut coop, dissolution);
    assert!(result.is_ok());
    assert_eq!(coop.status, CooperativeStatus::Dissolving);
}

#[test]
fn test_profit_share_calculation() {
    let mut coop = Cooperative::new(
        "test-coop".to_string(),
        "Test Coop".to_string(),
        CooperativeType::Worker,
        "domain".to_string(),
        2,
    );
    
    coop.tiers.push(MembershipTier {
        name: "Worker".to_string(),
        voting_weight: 1,
        profit_share_weight: 1,
        governance_rights: vec![],
    });
    
    let manager = MembershipManager::new(0.3);
    manager.add_member(&mut coop, "did:icn:alice".to_string(), "Worker", 1000).unwrap();
    manager.add_member(&mut coop, "did:icn:bob".to_string(), "Worker", 1000).unwrap();
    
    let share = manager.calculate_profit_share(&coop, "did:icn:alice", 10000).unwrap();
    assert_eq!(share, 5000); // 50% of profits
}
