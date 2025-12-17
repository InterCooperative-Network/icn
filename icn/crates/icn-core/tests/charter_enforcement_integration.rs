//! Integration test for charter enforcement
//!
//! Tests that charter rules are enforced during ledger transactions.

use anyhow::Result;
use icn_ccl::{CharterRule, CharterRuleSet, CharterValidator};
use icn_identity::IdentityBundle;
use icn_ledger::{entry::JournalEntryBuilder, AccountDelta, Ledger, QuarantineReason};
use icn_store::SledStore;
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to create a test ledger with charter validation
async fn create_test_ledger_with_charter() -> Result<(Arc<tokio::sync::RwLock<Ledger>>, TempDir)> {
    let temp_dir = tempfile::tempdir()?;
    let store_path = temp_dir.path().join("ledger");
    let store = Arc::new(SledStore::open(&store_path)?);

    let mut ledger = Ledger::new(store)?;

    // Set up charter validator with cooperative defaults
    let validator = Arc::new(CharterValidator::cooperative_default(
        "test-coop".to_string(),
        500, // Min trust 0.5
    ));

    // Set validation hook
    let validator_clone = validator.clone();
    ledger.set_validation_hook(move |entry| validator_clone.validate_entry(entry));

    Ok((Arc::new(tokio::sync::RwLock::new(ledger)), temp_dir))
}

#[tokio::test]
async fn test_charter_validator_allows_valid_transaction() -> Result<()> {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();

    let (ledger, _temp_dir) = create_test_ledger_with_charter().await?;

    let alice = IdentityBundle::generate()?.did().clone();
    let bob = IdentityBundle::generate()?.did().clone();

    // Create a balanced, valid transaction
    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice.clone(), "hours".to_string(), 10)
        .credit(bob.clone(), "hours".to_string(), 10)
        .build()?;

    // Should be accepted (passes charter validation)
    let result = ledger.write().await.append_entry(entry);
    assert!(result.is_ok(), "Valid transaction should be accepted");

    println!("✅ Valid transaction passed charter validation");

    Ok(())
}

#[tokio::test]
async fn test_charter_validator_passes_with_default_rules() -> Result<()> {
    // With default (optimistic) validation, transactions should pass
    let validator = CharterValidator::cooperative_default("test".to_string(), 500);

    let alice = IdentityBundle::generate()?.did().clone();
    let bob = IdentityBundle::generate()?.did().clone();

    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice, "hours".to_string(), 10)
        .credit(bob, "hours".to_string(), 10)
        .build()?;

    let result = validator.validate_entry(&entry);
    assert!(result.is_ok(), "Default rules should pass");

    println!("✅ Charter validator with default rules passes");

    Ok(())
}

#[tokio::test]
async fn test_charter_validator_hook_integration() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let store_path = temp_dir.path().join("ledger");
    let store = Arc::new(SledStore::open(&store_path)?);

    let mut ledger = Ledger::new(store)?;

    // Override with a validation hook that actually rejects
    ledger.set_validation_hook(move |_entry| {
        // This hook always fails for testing
        anyhow::bail!("Test rejection")
    });

    let alice = IdentityBundle::generate()?.did().clone();
    let bob = IdentityBundle::generate()?.did().clone();

    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice, "hours".to_string(), 10)
        .credit(bob, "hours".to_string(), 10)
        .build()?;

    // Should be rejected by validation hook
    let result = ledger.append_entry(entry);
    assert!(result.is_err(), "Should be rejected by validation hook");
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Test rejection"));

    println!("✅ Validation hook correctly rejects transactions");

    Ok(())
}

#[tokio::test]
async fn test_charter_validator_quarantines_violations() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let store_path = temp_dir.path().join("ledger");
    let store = Arc::new(SledStore::open(&store_path)?);

    let mut ledger = Ledger::new(store)?;

    // Set up validation hook that rejects with charter violation
    ledger.set_validation_hook(|_entry| {
        anyhow::bail!("Violates charter rule: max_transaction_amount")
    });

    let alice = IdentityBundle::generate()?.did().clone();
    let bob = IdentityBundle::generate()?.did().clone();

    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice, "hours".to_string(), 1000)
        .credit(bob, "hours".to_string(), 1000)
        .build()?;

    // Should be rejected and quarantined
    let result = ledger.append_entry(entry);
    assert!(result.is_err());

    // Check quarantine store
    let quarantined = ledger.quarantine().list()?;
    assert_eq!(quarantined.len(), 1, "Entry should be quarantined");

    let quarantine_item = &quarantined[0];
    assert!(matches!(
        quarantine_item.reason,
        QuarantineReason::CharterViolation
    ));

    println!("✅ Charter violations are properly quarantined");

    Ok(())
}

#[tokio::test]
async fn test_charter_validator_create_hook() -> Result<()> {
    let validator = Arc::new(CharterValidator::cooperative_default(
        "test-coop".to_string(),
        500,
    ));

    let hook = validator.create_hook();

    let alice = IdentityBundle::generate()?.did().clone();
    let bob = IdentityBundle::generate()?.did().clone();

    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice, "hours".to_string(), 10)
        .credit(bob, "hours".to_string(), 10)
        .build()?;

    // Hook should be callable
    let result = hook(&entry);
    assert!(result.is_ok(), "Hook should validate successfully");

    println!("✅ Validation hook creation works correctly");

    Ok(())
}

#[tokio::test]
async fn test_charter_validator_detailed_results() -> Result<()> {
    let validator = CharterValidator::cooperative_default("test-coop".to_string(), 500);

    let alice = IdentityBundle::generate()?.did().clone();
    let bob = IdentityBundle::generate()?.did().clone();

    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice, "hours".to_string(), 50)
        .credit(bob, "hours".to_string(), 50)
        .build()?;

    let results = validator.validate_entry_detailed(&entry)?;

    // Should have results for transaction rules
    assert!(!results.is_empty(), "Should have validation results");

    for result in &results {
        println!(
            "Rule '{}': {} {}",
            result.rule_name,
            if result.passed { "✓" } else { "✗" },
            result.reason.as_deref().unwrap_or("")
        );
    }

    println!("✅ Detailed validation results available");

    Ok(())
}

#[tokio::test]
async fn test_add_custom_charter_rule() -> Result<()> {
    let mut validator = CharterValidator::cooperative_default("test-coop".to_string(), 500);

    let initial_count = validator.rules().transaction_rules.len();

    // Add a custom rule
    validator.add_transaction_rule(CharterRule::transaction_credit_limit());

    assert_eq!(
        validator.rules().transaction_rules.len(),
        initial_count + 1,
        "Should add new rule"
    );

    println!("✅ Custom charter rules can be added dynamically");

    Ok(())
}

#[tokio::test]
async fn test_charter_validator_with_multiple_deltas() -> Result<()> {
    let validator = CharterValidator::cooperative_default("test-coop".to_string(), 500);

    let alice = IdentityBundle::generate()?.did().clone();
    let bob = IdentityBundle::generate()?.did().clone();
    let charlie = IdentityBundle::generate()?.did().clone();

    // Multi-party transaction using builder
    let entry = JournalEntryBuilder::new(alice.clone())
        .debit(alice, "hours".to_string(), 20)
        .credit(bob, "hours".to_string(), 10)
        .credit(charlie, "hours".to_string(), 10)
        .build()?;

    let result = validator.validate_entry(&entry);
    assert!(result.is_ok(), "Multi-party transaction should validate");

    println!("✅ Multi-party transactions validate correctly");

    Ok(())
}
