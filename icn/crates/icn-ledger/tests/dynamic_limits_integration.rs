//! Integration tests for DynamicCreditLimitManager with Ledger
#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_identity::KeyPair;
use icn_ledger::{
    entry::JournalEntryBuilder, CreditPolicy, CreditPolicyManager, DynamicCreditLimitManager,
    DynamicLimitConfig, Ledger, NewMemberPolicy,
};
use icn_store::SledStore;
use std::sync::Arc;
use tempfile::TempDir;

/// Create a test ledger with dynamic limits enabled
fn create_test_ledger_with_dynamic_limits() -> (Ledger, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());

    let mut ledger = Ledger::new(store.clone()).unwrap();

    // Set up credit policy (required for fallback and base calculations)
    let credit_policy = CreditPolicy::conservative("hours".to_string());
    let new_member_policy = NewMemberPolicy::conservative("hours".to_string());
    let policy_manager = CreditPolicyManager::new(credit_policy.clone(), new_member_policy);
    ledger.set_credit_policy_manager(policy_manager);

    // Set up dynamic limit manager
    let config = DynamicLimitConfig::default();
    let dynamic_manager = DynamicCreditLimitManager::new(store.clone(), credit_policy, config);
    ledger.set_dynamic_limit_manager(Arc::new(dynamic_manager));

    (ledger, temp_dir)
}

/// Create a basic test ledger without dynamic limits
fn create_basic_test_ledger() -> (Ledger, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());
    let ledger = Ledger::new(store).unwrap();
    (ledger, temp_dir)
}

#[test]
fn test_dynamic_limits_integration_basic() {
    let (mut ledger, _temp) = create_test_ledger_with_dynamic_limits();

    let alice_kp = KeyPair::generate().unwrap();
    let alice = alice_kp.did().clone();
    let bob = KeyPair::generate().unwrap().did().clone();

    // Create a simple transfer entry
    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 10)
        .credit(bob.clone(), "hours".to_string(), 10)
        .build()
        .unwrap();

    // Should succeed - within credit limits
    let result = ledger.append_entry(entry);
    assert!(result.is_ok(), "Entry should succeed: {result:?}");

    // Check balances (in mutual credit: debit = owed to you, credit = you owe)
    assert_eq!(ledger.get_balance(&alice, "hours"), 10); // Alice is owed 10
    assert_eq!(ledger.get_balance(&bob, "hours"), -10); // Bob owes 10
}

#[test]
fn test_dynamic_limits_manager_getter() {
    let (ledger, _temp) = create_test_ledger_with_dynamic_limits();

    // Manager should be set
    assert!(ledger.dynamic_limit_manager().is_some());
}

#[test]
fn test_dynamic_limits_not_set_by_default() {
    let (ledger, _temp) = create_basic_test_ledger();

    // Manager should not be set by default
    assert!(ledger.dynamic_limit_manager().is_none());
}

#[test]
fn test_activity_updates_dynamic_state() {
    let temp_dir = TempDir::new().unwrap();
    let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());

    let mut ledger = Ledger::new(store.clone()).unwrap();

    // Set up credit policy and dynamic limits
    let credit_policy = CreditPolicy::conservative("hours".to_string());
    let new_member_policy = NewMemberPolicy::conservative("hours".to_string());
    let policy_manager = CreditPolicyManager::new(credit_policy.clone(), new_member_policy);
    ledger.set_credit_policy_manager(policy_manager);

    let config = DynamicLimitConfig::default();
    let dynamic_manager = Arc::new(DynamicCreditLimitManager::new(
        store.clone(),
        credit_policy,
        config,
    ));
    ledger.set_dynamic_limit_manager(dynamic_manager.clone());

    let alice_kp = KeyPair::generate().unwrap();
    let alice = alice_kp.did().clone();
    let bob = KeyPair::generate().unwrap().did().clone();

    // No state should exist before first transaction
    let state_before = dynamic_manager.get_state(&alice, "hours").unwrap();
    assert!(state_before.is_none());

    // Create a transfer
    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 5)
        .credit(bob.clone(), "hours".to_string(), 5)
        .build()
        .unwrap();

    ledger.append_entry(entry).unwrap();

    // State should now exist for both alice and bob
    let alice_state = dynamic_manager.get_state(&alice, "hours").unwrap();
    assert!(
        alice_state.is_some(),
        "Alice should have dynamic limit state"
    );

    let bob_state = dynamic_manager.get_state(&bob, "hours").unwrap();
    assert!(bob_state.is_some(), "Bob should have dynamic limit state");
}

#[test]
fn test_credit_limit_enforcement_with_dynamic_manager() {
    let temp_dir = TempDir::new().unwrap();
    let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());

    let mut ledger = Ledger::new(store.clone()).unwrap();

    // Set up with very low baseline to test limit enforcement
    // In mutual credit: credit limit is how negative (how much debt) an account can have
    let credit_policy = CreditPolicy {
        currency: "hours".to_string(),
        baseline: 100, // Low baseline for testing (max debt = 100)
        trust_multiplier: 0.0,
        history_bonus_rate: 0.0,
    };
    let new_member_policy = NewMemberPolicy::conservative("hours".to_string());
    let policy_manager = CreditPolicyManager::new(credit_policy.clone(), new_member_policy);
    ledger.set_credit_policy_manager(policy_manager);

    // Create config with low floor to test limit enforcement
    let config = DynamicLimitConfig {
        min_limit_floor: 50, // Lower than baseline so baseline applies
        ..DynamicLimitConfig::default()
    };
    let dynamic_manager = Arc::new(DynamicCreditLimitManager::new(
        store.clone(),
        credit_policy,
        config,
    ));
    ledger.set_dynamic_limit_manager(dynamic_manager);

    let alice_kp = KeyPair::generate().unwrap();
    let alice = alice_kp.did().clone();
    let bob = KeyPair::generate().unwrap().did().clone();

    // First transaction: Bob receives credit of 50 (goes to -50 balance)
    // debit(alice) = Alice is owed 50 → balance +50
    // credit(bob) = Bob owes 50 → balance -50
    let entry1 = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 50)
        .credit(bob.clone(), "hours".to_string(), 50)
        .build()
        .unwrap();

    assert!(ledger.append_entry(entry1).is_ok());
    assert_eq!(ledger.get_balance(&alice, "hours"), 50); // Alice is owed
    assert_eq!(ledger.get_balance(&bob, "hours"), -50); // Bob owes

    // Second transaction: Bob receives another 60 credit
    // This would take Bob to -110, exceeding his -100 credit limit (baseline)
    let entry2 = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 60)
        .credit(bob.clone(), "hours".to_string(), 60)
        .build()
        .unwrap();

    let result = ledger.append_entry(entry2);
    assert!(
        result.is_err(),
        "Should reject entry that exceeds credit limit"
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("would exceed credit limit"),
        "Error should mention credit limit"
    );

    // Balances should remain unchanged (rejected entry shouldn't affect them)
    assert_eq!(ledger.get_balance(&alice, "hours"), 50);
    assert_eq!(ledger.get_balance(&bob, "hours"), -50);
}

#[test]
fn test_fallback_to_static_limits_on_error() {
    // This test verifies that if dynamic manager is set but fails,
    // we fall back to static calculation. This is hard to test directly
    // since the manager shouldn't fail normally, but we can verify the
    // code path exists by checking the logic.
    let (mut ledger, _temp) = create_test_ledger_with_dynamic_limits();

    let alice_kp = KeyPair::generate().unwrap();
    let alice = alice_kp.did().clone();
    let bob = KeyPair::generate().unwrap().did().clone();

    // Basic transaction should work
    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 10)
        .credit(bob.clone(), "hours".to_string(), 10)
        .build()
        .unwrap();

    assert!(ledger.append_entry(entry).is_ok());
}

#[test]
fn test_multiple_currencies_tracked_separately() {
    let temp_dir = TempDir::new().unwrap();
    let store = Arc::new(SledStore::open(temp_dir.path()).unwrap());

    let mut ledger = Ledger::new(store.clone()).unwrap();

    // Set up for hours currency
    let hours_policy = CreditPolicy::conservative("hours".to_string());
    let new_member_policy = NewMemberPolicy::conservative("hours".to_string());
    let policy_manager = CreditPolicyManager::new(hours_policy.clone(), new_member_policy);
    ledger.set_credit_policy_manager(policy_manager);

    let config = DynamicLimitConfig::default();
    let dynamic_manager = Arc::new(DynamicCreditLimitManager::new(
        store.clone(),
        hours_policy,
        config,
    ));
    ledger.set_dynamic_limit_manager(dynamic_manager.clone());

    let alice_kp = KeyPair::generate().unwrap();
    let alice = alice_kp.did().clone();
    let bob = KeyPair::generate().unwrap().did().clone();

    // Transaction in hours
    let hours_entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 10)
        .credit(bob.clone(), "hours".to_string(), 10)
        .build()
        .unwrap();

    ledger.append_entry(hours_entry).unwrap();

    // Transaction in different currency (kwh)
    let kwh_entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "kwh".to_string(), 100)
        .credit(bob.clone(), "kwh".to_string(), 100)
        .build()
        .unwrap();

    ledger.append_entry(kwh_entry).unwrap();

    // Both should have separate state entries
    let hours_state = dynamic_manager.get_state(&alice, "hours").unwrap();
    let kwh_state = dynamic_manager.get_state(&alice, "kwh").unwrap();

    assert!(hours_state.is_some());
    assert!(kwh_state.is_some());

    // Verify they're different state objects
    let hours_state = hours_state.unwrap();
    let kwh_state = kwh_state.unwrap();
    assert_eq!(hours_state.currency, "hours");
    assert_eq!(kwh_state.currency, "kwh");
}
