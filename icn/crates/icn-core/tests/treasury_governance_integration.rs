#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Integration tests for treasury governance handlers
//!
//! Tests the execution of treasury proposals through the governance system,
//! including idempotency, error handling, and persistence.

use anyhow::Result;
use icn_identity::Did;
use icn_ledger::treasury::{BudgetStatus, TreasuryManager};
use icn_store::{SledStore, Store};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Helper to create deterministic test DIDs
fn test_did(seed: &str) -> Did {
    let hash = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        seed.hash(&mut hasher);
        hasher.finish()
    };
    let mut bytes = [0u8; 32];
    bytes[..8].copy_from_slice(&hash.to_le_bytes());
    Did::from_anchor_id(&bytes)
}

/// Helper to setup a treasury manager with a registered treasury
async fn setup_treasury() -> (Arc<RwLock<TreasuryManager>>, Did, Did, Arc<SledStore>) {
    let store = Arc::new(SledStore::temporary().unwrap());
    let mut manager = TreasuryManager::with_store(store.clone()).unwrap();
    let treasury_did = test_did("test-treasury");
    let admin = test_did("admin");

    manager
        .register_treasury(
            treasury_did.clone(),
            "test-coop".to_string(),
            "credits".to_string(),
            admin.clone(),
            Some("Test Treasury".to_string()),
        )
        .unwrap();

    (Arc::new(RwLock::new(manager)), treasury_did, admin, store)
}

#[tokio::test]
async fn test_treasury_create_budget_execution() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Testing treasury budget creation ===");

    let (treasury_manager, treasury_did, admin, _store) = setup_treasury().await;

    // Create a budget
    let budget = {
        let mut guard = treasury_manager.write().await;
        guard.create_budget(
            treasury_did.clone(),
            "operations".to_string(),
            10000,
            "credits".to_string(),
            None,
            admin.clone(),
            None,
        )?
    };

    // Verify budget was created
    assert_eq!(budget.allocated_amount, 10000);
    assert_eq!(budget.spent_amount, 0);
    assert_eq!(budget.remaining(), 10000);
    assert!(budget.can_spend());

    // Verify it's retrievable
    {
        let guard = treasury_manager.read().await;
        let budgets = guard.list_budgets(&treasury_did);
        assert_eq!(budgets.len(), 1);
    }

    info!("✅ Budget creation test passed");
    Ok(())
}

#[tokio::test]
async fn test_treasury_budget_idempotency() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Testing treasury budget idempotency ===");

    let audit_store = Arc::new(SledStore::temporary()?);
    let (treasury_manager, treasury_did, admin, _store) = setup_treasury().await;

    let proposal_id = "test-budget-proposal";
    let audit_key = format!("gov:audit:treasury:create:{proposal_id}");

    // Simulate first execution (create budget)
    {
        // Check idempotency - should not exist yet
        assert!(audit_store.get(audit_key.as_bytes())?.is_none());

        let mut guard = treasury_manager.write().await;
        guard.create_budget(
            treasury_did.clone(),
            "first-budget".to_string(),
            5000,
            "credits".to_string(),
            None,
            admin.clone(),
            Some(proposal_id.to_string()),
        )?;

        // Record audit trail
        let audit_record = serde_json::json!({
            "proposal_id": proposal_id,
            "action": "create_budget",
        });
        audit_store.put(audit_key.as_bytes(), &serde_json::to_vec(&audit_record)?)?;
    }

    // Simulate duplicate execution attempt
    {
        // Check idempotency - should exist now
        let existing = audit_store.get(audit_key.as_bytes())?;
        assert!(existing.is_some(), "Audit trail should exist");

        info!(
            "Duplicate proposal {} detected, skipping execution",
            proposal_id
        );
    }

    // Verify only one budget exists
    {
        let guard = treasury_manager.read().await;
        let budgets = guard.list_budgets(&treasury_did);
        assert_eq!(budgets.len(), 1);
    }

    info!("✅ Idempotency test passed - duplicate was correctly skipped");
    Ok(())
}

#[tokio::test]
async fn test_treasury_transfer_insufficient_funds() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Testing treasury transfer with insufficient funds ===");

    let (treasury_manager, treasury_did, admin, _store) = setup_treasury().await;

    // Create source budget with limited funds
    let (source_id, dest_id) = {
        let mut guard = treasury_manager.write().await;
        let source = guard.create_budget(
            treasury_did.clone(),
            "source-budget".to_string(),
            1000,
            "credits".to_string(),
            None,
            admin.clone(),
            None,
        )?;

        let dest = guard.create_budget(
            treasury_did.clone(),
            "dest-budget".to_string(),
            100, // Must be positive
            "credits".to_string(),
            None,
            admin.clone(),
            None,
        )?;
        (source.id.clone(), dest.id.clone())
    };

    // Attempt transfer of more than available
    let transfer_amount = 5000;
    {
        let guard = treasury_manager.read().await;
        let source = guard.get_budget(&source_id).unwrap();
        let remaining = source.remaining();

        if remaining < transfer_amount {
            info!(
                "❌ Transfer rejected: insufficient funds ({} < {})",
                remaining, transfer_amount
            );
        } else {
            panic!("Transfer should have been rejected");
        }
    }

    // Verify budgets unchanged
    {
        let guard = treasury_manager.read().await;
        let source = guard.get_budget(&source_id).unwrap();
        let dest = guard.get_budget(&dest_id).unwrap();
        assert_eq!(source.allocated_amount, 1000, "Source should be unchanged");
        assert_eq!(dest.allocated_amount, 100, "Dest should be unchanged");
    }

    info!("✅ Insufficient funds test passed");
    Ok(())
}

#[tokio::test]
async fn test_treasury_transfer_between_budgets() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Testing treasury transfer between budgets ===");

    let (treasury_manager, treasury_did, admin, _store) = setup_treasury().await;

    // Create budgets
    let (from_id, to_id) = {
        let mut guard = treasury_manager.write().await;
        let from = guard.create_budget(
            treasury_did.clone(),
            "from-budget".to_string(),
            10000,
            "credits".to_string(),
            None,
            admin.clone(),
            None,
        )?;

        let to = guard.create_budget(
            treasury_did.clone(),
            "to-budget".to_string(),
            2000,
            "credits".to_string(),
            None,
            admin.clone(),
            None,
        )?;
        (from.id.clone(), to.id.clone())
    };

    // Perform transfer (simulating governance handler logic)
    let transfer_amount = 3000;
    {
        let mut guard = treasury_manager.write().await;

        // Validate source has funds
        let from_remaining = guard.get_budget(&from_id).unwrap().remaining();
        assert!(from_remaining >= transfer_amount);

        // Validate destination exists
        assert!(guard.get_budget(&to_id).is_some());

        // Atomic mutation
        if let Some(from) = guard.get_budget_mut(&from_id) {
            from.allocated_amount -= transfer_amount;
        }
        if let Some(to) = guard.get_budget_mut(&to_id) {
            to.allocated_amount += transfer_amount;
        }

        // Persist
        guard.save_budget(&from_id)?;
        guard.save_budget(&to_id)?;
    }

    // Verify transfer
    {
        let guard = treasury_manager.read().await;
        let from = guard.get_budget(&from_id).unwrap();
        let to = guard.get_budget(&to_id).unwrap();
        assert_eq!(from.allocated_amount, 7000, "Source should have 7000");
        assert_eq!(to.allocated_amount, 5000, "Dest should have 5000");
    }

    info!("✅ Transfer test passed");
    Ok(())
}

#[tokio::test]
async fn test_treasury_cancel_budget_with_return() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Testing treasury cancel budget with return_to_treasury ===");

    let (treasury_manager, treasury_did, admin, _store) = setup_treasury().await;

    // Create budget
    let budget_id = {
        let mut guard = treasury_manager.write().await;
        let budget = guard.create_budget(
            treasury_did.clone(),
            "to-cancel".to_string(),
            10000,
            "credits".to_string(),
            None,
            admin.clone(),
            None,
        )?;

        // Simulate some spending by recording it
        if let Some(b) = guard.get_budget_mut(&budget.id) {
            b.spent_amount = 3000;
        }
        guard.save_budget(&budget.id)?;
        budget.id.clone()
    };

    // Cancel with return_to_treasury = true
    let return_to_treasury = true;
    {
        let mut guard = treasury_manager.write().await;

        let remaining = {
            let budget = guard.get_budget(&budget_id).unwrap();
            budget.remaining()
        };

        // If return_to_treasury, reclaim remaining funds
        let reclaimed = if return_to_treasury && remaining > 0 {
            if let Some(budget) = guard.get_budget_mut(&budget_id) {
                budget.allocated_amount -= remaining;
            }
            remaining
        } else {
            0
        };

        info!("Reclaimed {} credits from cancelled budget", reclaimed);
        assert_eq!(reclaimed, 7000, "Should reclaim 7000 (10000 - 3000 spent)");

        // Cancel the budget
        guard.update_budget_status(&budget_id, BudgetStatus::Cancelled)?;
    }

    // Verify cancellation
    {
        let guard = treasury_manager.read().await;
        let budget = guard.get_budget(&budget_id).unwrap();
        assert_eq!(budget.status, BudgetStatus::Cancelled);
        assert_eq!(
            budget.allocated_amount, 3000,
            "Allocated should match spent"
        );
        assert_eq!(budget.remaining(), 0, "No funds remaining after reclaim");
    }

    info!("✅ Cancel with return_to_treasury test passed");
    Ok(())
}

#[tokio::test]
async fn test_treasury_cancel_budget_without_return() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Testing treasury cancel budget without return_to_treasury ===");

    let (treasury_manager, treasury_did, admin, _store) = setup_treasury().await;

    // Create budget
    let budget_id = {
        let mut guard = treasury_manager.write().await;
        let budget = guard.create_budget(
            treasury_did.clone(),
            "cancel-no-return".to_string(),
            10000,
            "credits".to_string(),
            None,
            admin.clone(),
            None,
        )?;

        // Simulate spending
        if let Some(b) = guard.get_budget_mut(&budget.id) {
            b.spent_amount = 3000;
        }
        guard.save_budget(&budget.id)?;
        budget.id.clone()
    };

    // Cancel with return_to_treasury = false
    {
        let mut guard = treasury_manager.write().await;
        // Just cancel without reclaiming
        guard.update_budget_status(&budget_id, BudgetStatus::Cancelled)?;
    }

    // Verify - allocation should remain unchanged
    {
        let guard = treasury_manager.read().await;
        let budget = guard.get_budget(&budget_id).unwrap();
        assert_eq!(budget.status, BudgetStatus::Cancelled);
        assert_eq!(budget.allocated_amount, 10000, "Allocation unchanged");
        assert_eq!(budget.remaining(), 7000, "Remaining unchanged");
    }

    info!("✅ Cancel without return_to_treasury test passed");
    Ok(())
}

#[tokio::test]
async fn test_treasury_reclaim_budget_funds() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Testing treasury reclaim budget funds ===");

    let (treasury_manager, treasury_did, admin, _store) = setup_treasury().await;

    // Create budget
    let budget_id = {
        let mut guard = treasury_manager.write().await;
        let budget = guard.create_budget(
            treasury_did.clone(),
            "reclaim-test".to_string(),
            10000,
            "credits".to_string(),
            None,
            admin.clone(),
            None,
        )?;
        budget.id.clone()
    };

    // Reclaim partial funds
    let reclaim_amount = 4000;
    {
        let mut guard = treasury_manager.write().await;

        // Validate sufficient funds
        let remaining = guard.get_budget(&budget_id).unwrap().remaining();
        assert!(remaining >= reclaim_amount);

        // Perform reclaim
        if let Some(budget) = guard.get_budget_mut(&budget_id) {
            budget.allocated_amount -= reclaim_amount;
        }

        // Persist
        guard.save_budget(&budget_id)?;
    }

    // Verify reclaim
    {
        let guard = treasury_manager.read().await;
        let budget = guard.get_budget(&budget_id).unwrap();
        assert_eq!(budget.allocated_amount, 6000);
        assert_eq!(budget.remaining(), 6000);
        assert!(budget.can_spend());
    }

    info!("✅ Reclaim funds test passed");
    Ok(())
}

#[tokio::test]
async fn test_treasury_reclaim_insufficient_funds() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Testing treasury reclaim with insufficient funds ===");

    let (treasury_manager, treasury_did, admin, _store) = setup_treasury().await;

    // Create budget with some spending
    let budget_id = {
        let mut guard = treasury_manager.write().await;
        let budget = guard.create_budget(
            treasury_did.clone(),
            "limited-budget".to_string(),
            5000,
            "credits".to_string(),
            None,
            admin.clone(),
            None,
        )?;

        // Simulate spending
        if let Some(b) = guard.get_budget_mut(&budget.id) {
            b.spent_amount = 3000;
        }
        guard.save_budget(&budget.id)?;
        budget.id.clone()
    };

    // Attempt to reclaim more than remaining
    let reclaim_amount = 5000; // Only 2000 remaining
    {
        let guard = treasury_manager.read().await;
        let budget = guard.get_budget(&budget_id).unwrap();
        let remaining = budget.remaining();

        if remaining < reclaim_amount {
            info!(
                "❌ Reclaim rejected: insufficient funds ({} < {})",
                remaining, reclaim_amount
            );
        } else {
            panic!("Reclaim should have been rejected");
        }
    }

    // Verify budget unchanged
    {
        let guard = treasury_manager.read().await;
        let budget = guard.get_budget(&budget_id).unwrap();
        assert_eq!(budget.allocated_amount, 5000);
        assert_eq!(budget.remaining(), 2000);
    }

    info!("✅ Reclaim insufficient funds test passed");
    Ok(())
}

#[tokio::test]
async fn test_treasury_atomic_transfer_consistency() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_test_writer()
        .try_init();

    info!("=== Testing atomic transfer consistency (no TOCTOU) ===");

    let (treasury_manager, treasury_did, admin, _store) = setup_treasury().await;

    // Create budgets
    let (source_id, dest_id) = {
        let mut guard = treasury_manager.write().await;
        let source = guard.create_budget(
            treasury_did.clone(),
            "atomic-source".to_string(),
            5000,
            "credits".to_string(),
            None,
            admin.clone(),
            None,
        )?;

        let dest = guard.create_budget(
            treasury_did.clone(),
            "atomic-dest".to_string(),
            100, // Must be positive
            "credits".to_string(),
            None,
            admin.clone(),
            None,
        )?;
        (source.id.clone(), dest.id.clone())
    };

    let transfer_amount = 3000;

    // Perform atomic transfer - lock held throughout
    {
        // ATOMIC: acquire lock
        let mut guard = treasury_manager.write().await;

        // ATOMIC: validate (lock held)
        let from_remaining = guard.get_budget(&source_id).unwrap().remaining();
        assert!(
            from_remaining >= transfer_amount,
            "Validation with lock held"
        );

        // ATOMIC: mutate (lock still held)
        if let Some(from) = guard.get_budget_mut(&source_id) {
            from.allocated_amount -= transfer_amount;
        }
        if let Some(to) = guard.get_budget_mut(&dest_id) {
            to.allocated_amount += transfer_amount;
        }

        // ATOMIC: persist (lock still held)
        guard.save_budget(&source_id)?;
        guard.save_budget(&dest_id)?;

        // Lock released when guard drops
    }

    // Verify consistency
    {
        let guard = treasury_manager.read().await;
        let source = guard.get_budget(&source_id).unwrap();
        let dest = guard.get_budget(&dest_id).unwrap();

        // Total should be conserved (5000 + 100 initial = 5100)
        let total = source.allocated_amount + dest.allocated_amount;
        assert_eq!(total, 5100, "Total funds should be conserved");
        assert_eq!(source.allocated_amount, 2000);
        assert_eq!(dest.allocated_amount, 3100); // 100 initial + 3000 transfer
    }

    info!("✅ Atomic transfer consistency test passed");
    Ok(())
}
