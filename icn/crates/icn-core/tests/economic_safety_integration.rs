#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Economic Safety Integration Tests
//!
//! Tests server-side credit limit enforcement to ensure malicious clients
//! cannot bypass economic safety guardrails.

use anyhow::Result;
use icn_identity::KeyPair;
use icn_ledger::{
    entry::JournalEntryBuilder, CreditPolicy, CreditPolicyManager, Ledger, MembershipStore,
    NewMemberPolicy, SledMembershipStore,
};
use icn_store::SledStore;
use std::sync::Arc;
use tempfile::TempDir;
use tracing::info;

#[test]
fn test_credit_limit_enforcement_rejects_excessive_spending() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Testing credit limit enforcement ===");

    let temp_dir = TempDir::new()?;

    // Create keypairs for participants
    let alice_kp = KeyPair::generate()?;
    let bob_kp = KeyPair::generate()?;
    let alice = alice_kp.did().clone();
    let bob = bob_kp.did().clone();

    // Create ledger with credit policy (no trust graph for sync test)
    let ledger_path = temp_dir.path().join("ledger");
    let store = Arc::new(SledStore::open(&ledger_path)?);
    let mut ledger = Ledger::new(store)?;

    // Set up credit policy with conservative limits
    let credit_policy = CreditPolicy::conservative("hours".to_string());
    let new_member_policy = NewMemberPolicy::conservative("hours".to_string());
    let credit_manager = CreditPolicyManager::new(credit_policy, new_member_policy);
    ledger.set_credit_policy_manager(credit_manager);

    info!("Ledger initialized with conservative credit policy");

    // Test 1: Valid transaction within limit
    // Conservative policy: baseline = 10,000 centihours (100 hours)
    // No trust graph = no trust bonus, just baseline
    let hash1 = {
        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 5_000) // 50 hours
            .credit(bob.clone(), "hours".to_string(), 5_000)
            .build()?;

        let result = ledger.append_entry(entry);
        assert!(
            result.is_ok(),
            "Valid transaction should be accepted: {result:?}"
        );
        info!("✓ Valid transaction (50 hours) accepted");
        result.unwrap()
    };

    // Test 2: Transaction that would exceed credit limit
    // Alice already spent 50 hours, trying to spend 60 more = 110 total
    // This exceeds the 100 hour baseline limit
    {
        let entry = JournalEntryBuilder::new(alice.clone())
            .add_parent(hash1.clone())
            .debit(alice.clone(), "hours".to_string(), 6_000) // 60 hours
            .credit(bob.clone(), "hours".to_string(), 6_000)
            .build()?;

        let result = ledger.append_entry(entry);
        assert!(
            result.is_err(),
            "Transaction exceeding limit should be rejected"
        );
        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("credit limit"),
            "Error should mention credit limit: {error_msg}"
        );
        info!("✓ Excessive transaction (would exceed limit) correctly rejected");
    }

    // Test 3: Transaction right at the limit succeeds
    // Alice is at -50 hours, can spend up to 50 more to reach -100
    let hash3 = {
        let entry = JournalEntryBuilder::new(alice.clone())
            .add_parent(hash1.clone())
            .debit(alice.clone(), "hours".to_string(), 4_750) // 47.5 hours - leaves small margin
            .credit(bob.clone(), "hours".to_string(), 4_750)
            .build()?;

        let result = ledger.append_entry(entry);
        assert!(
            result.is_ok(),
            "Transaction near limit should be accepted: {result:?}"
        );
        info!("✓ Transaction near limit (97.5 hours total) accepted");
        result.unwrap()
    };

    // Test 4: Large transaction that clearly exceeds any reasonable limit
    // Alice is at -9750, let's try to spend 5000 more (50 hours)
    // That would put her at -14750, well over any calculated limit
    {
        let entry = JournalEntryBuilder::new(alice.clone())
            .add_parent(hash3.clone())
            .debit(alice.clone(), "hours".to_string(), 5_000) // 50 hours - clearly over
            .credit(bob.clone(), "hours".to_string(), 5_000)
            .build()?;

        let result = ledger.append_entry(entry);
        assert!(
            result.is_err(),
            "Transaction beyond limit should be rejected"
        );
        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("credit limit"),
            "Error should mention credit limit: {error_msg}"
        );
        info!("✓ Transaction beyond limit correctly rejected");
    }

    info!("✅ Credit limit enforcement test passed");
    Ok(())
}

#[test]
fn test_credit_policy_baseline_calculation() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Testing credit policy baseline calculation ===");

    // Verify conservative policy values
    let policy = CreditPolicy::conservative("hours".to_string());

    // Conservative baseline is 10,000 centihours (100 hours)
    assert_eq!(
        policy.baseline, 10_000,
        "Baseline should be 10,000 centihours"
    );
    assert!(
        (policy.trust_multiplier - 0.3).abs() < 0.001,
        "Trust multiplier should be 0.3"
    );
    assert!(
        (policy.history_bonus_rate - 0.05).abs() < 0.001,
        "History bonus rate should be 0.05"
    );

    info!("✓ Conservative policy values verified");
    info!(
        "  Baseline: {} centihours ({} hours)",
        policy.baseline,
        policy.baseline / 100
    );
    info!("  Trust multiplier: {}", policy.trust_multiplier);
    info!("  History bonus rate: {}", policy.history_bonus_rate);

    info!("✅ Credit policy baseline test passed");
    Ok(())
}

#[test]
fn test_new_member_policy_values() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Testing new member policy values ===");

    // Verify conservative new member policy values
    let policy = NewMemberPolicy::conservative("hours".to_string());

    // Initial limit: 1,000 centihours (10 hours)
    assert_eq!(
        policy.initial_limit, 1_000,
        "Initial limit should be 1,000 centihours"
    );

    // Contribution threshold: 5,000 centihours (50 hours)
    assert_eq!(
        policy.contribution_threshold, 5_000,
        "Contribution threshold should be 5,000 centihours"
    );

    // Ramp period: 90 days
    let expected_ramp_secs = 90 * 24 * 60 * 60; // 90 days in seconds
    assert_eq!(
        policy.ramp_period.as_secs(),
        expected_ramp_secs,
        "Ramp period should be 90 days"
    );

    info!("✓ New member policy values verified");
    info!(
        "  Initial limit: {} centihours ({} hours)",
        policy.initial_limit,
        policy.initial_limit / 100
    );
    info!(
        "  Contribution threshold: {} centihours ({} hours)",
        policy.contribution_threshold,
        policy.contribution_threshold / 100
    );
    info!(
        "  Ramp period: {} days",
        policy.ramp_period.as_secs() / 86400
    );
    info!("  New member ramping is now enforced via MembershipStore");

    info!("✅ New member policy test passed");
    Ok(())
}

#[test]
fn test_new_member_ramping_enforced() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Testing new member credit limit ramping ===");

    let temp_dir = TempDir::new()?;

    // Create keypairs for participants
    let new_member_kp = KeyPair::generate()?;
    let established_kp = KeyPair::generate()?;
    let new_member = new_member_kp.did().clone();
    let established = established_kp.did().clone();

    // Create ledger with credit policy AND membership store
    let ledger_path = temp_dir.path().join("ledger");
    let store = Arc::new(SledStore::open(&ledger_path)?);
    let mut ledger = Ledger::new(store.clone())?;

    // Set up membership store (keep reference for verification)
    let membership_store = Arc::new(SledMembershipStore::new(store));
    let membership_store_ref = membership_store.clone();
    ledger.set_membership_store(membership_store);

    // Set up credit policy with conservative limits
    let credit_policy = CreditPolicy::conservative("hours".to_string());
    let new_member_policy = NewMemberPolicy::conservative("hours".to_string());
    let credit_manager = CreditPolicyManager::new(credit_policy, new_member_policy);
    ledger.set_credit_policy_manager(credit_manager);

    info!("Ledger initialized with membership tracking");
    info!("  New member initial limit: 1,000 centihours (10 hours)");
    info!("  Full limit: 10,000 centihours (100 hours)");

    // Test 1: New member can spend up to initial limit (10 hours = 1,000 centihours)
    let hash1 = {
        let entry = JournalEntryBuilder::new(new_member.clone())
            .debit(new_member.clone(), "hours".to_string(), 800) // 8 hours - within initial limit
            .credit(established.clone(), "hours".to_string(), 800)
            .build()?;

        let result = ledger.append_entry(entry);
        assert!(
            result.is_ok(),
            "New member should be able to spend within initial limit: {result:?}"
        );
        info!("✓ New member can spend 8 hours (within 10 hour initial limit)");
        result.unwrap()
    };

    // Test 2: New member CANNOT spend beyond initial limit
    // Initial limit is 1,000 centihours, already spent 800, trying to spend 500 more = 1,300 total
    {
        let entry = JournalEntryBuilder::new(new_member.clone())
            .add_parent(hash1.clone())
            .debit(new_member.clone(), "hours".to_string(), 500) // 5 hours more - exceeds initial limit
            .credit(established.clone(), "hours".to_string(), 500)
            .build()?;

        let result = ledger.append_entry(entry);
        assert!(
            result.is_err(),
            "New member should NOT be able to exceed initial limit"
        );
        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("credit limit"),
            "Error should mention credit limit: {error_msg}"
        );
        info!("✓ New member correctly blocked from exceeding 10 hour initial limit");
    }

    // Test 3: Small additional transaction within remaining limit succeeds
    let hash3 = {
        let entry = JournalEntryBuilder::new(new_member.clone())
            .add_parent(hash1.clone())
            .debit(new_member.clone(), "hours".to_string(), 150) // 1.5 hours more - within limit
            .credit(established.clone(), "hours".to_string(), 150)
            .build()?;

        let result = ledger.append_entry(entry);
        assert!(
            result.is_ok(),
            "Small transaction within limit should succeed: {result:?}"
        );
        info!(
            "✓ New member can spend additional 1.5 hours (9.5 hours total, within 10 hour limit)"
        );
        result.unwrap()
    };

    // Test 4: Verify member-since was recorded
    // The membership store should have tracked when the new member first transacted
    {
        let member_since = membership_store_ref.get_member_since(&new_member)?;
        assert!(
            member_since.is_some(),
            "New member should be registered in membership store"
        );
        let timestamp = member_since.unwrap();
        assert!(timestamp > 0, "Member-since timestamp should be valid");
        info!(
            "✓ Member registration verified: new_member registered at timestamp {}",
            timestamp
        );

        // Also verify the established member was registered (they receive credits)
        let established_since = membership_store_ref.get_member_since(&established)?;
        assert!(
            established_since.is_some(),
            "Established member should also be registered"
        );
        info!("✓ Both transaction participants registered automatically");
    }

    info!("✅ New member ramping enforcement test passed");
    info!("  New members are correctly limited to initial 10 hour credit limit");
    info!("  Full limits require: 90 days tenure OR 50 hours contribution");

    // Cleanup: use hash3 to avoid unused warning
    let _ = hash3;

    Ok(())
}
