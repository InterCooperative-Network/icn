#![allow(clippy::unwrap_used, clippy::expect_used)]

use icn_gateway::error::GatewayError;
use icn_gateway::ledger_mgr::LedgerManager;
use icn_identity::IdentityBundle;
use icn_store::budgets::{Budget, BudgetPeriod, BudgetStatus, BudgetStore};
use std::sync::Arc;
use std::time::SystemTime;

fn temp_db() -> sled::Db {
    sled::Config::new().temporary(true).open().unwrap()
}

#[tokio::test]
async fn test_budget_enforcement() {
    let db = temp_db();
    let budget_store = Arc::new(BudgetStore::new(db));
    let mut ledger_mgr = LedgerManager::new();
    ledger_mgr.set_budget_store(budget_store.clone());

    // Setup identities
    let alice = IdentityBundle::generate().unwrap();
    let bob = IdentityBundle::generate().unwrap();
    let coop_id = "test-coop".to_string();
    let currency = "USD".to_string();

    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // 1. Create Budget for Alice (Limit: 100)
    let budget = Budget {
        id: "budget-1".to_string(),
        owner: alice.did().to_string(),
        account: alice.did().to_string(),
        currency: currency.clone(),
        limit: 100,
        spent: 0,
        period: BudgetPeriod::Monthly,
        period_start: now,
        period_end: now + 2592000,
        status: BudgetStatus::Active,
        notification_thresholds: vec![80, 100],
        notified_thresholds: vec![],
        description: "Test Budget".to_string(),
        created_at: now,
        updated_at: now,
    };
    budget_store.insert(budget.clone()).unwrap();

    // 2. Make Payment (50) -> Success
    let res = ledger_mgr
        .create_payment(&coop_id, alice.did(), bob.did(), 50, currency.clone())
        .await;
    assert!(res.is_ok());

    // Verify spent updated
    let updated_budget = budget_store.get("budget-1").unwrap().unwrap();
    assert_eq!(updated_budget.spent, 50);

    // 3. Make Payment (60) -> Fail (Total 110 > 100)
    let res_fail = ledger_mgr
        .create_payment(&coop_id, alice.did(), bob.did(), 60, currency.clone())
        .await;

    assert!(res_fail.is_err());
    match res_fail {
        Err(GatewayError::BudgetExceeded(_)) => (),
        _ => panic!("Expected BudgetExceeded error, got {res_fail:?}"),
    }

    // Verify spent NOT updated (or at least not for the failed tx)
    let updated_budget_2 = budget_store.get("budget-1").unwrap().unwrap();
    assert_eq!(updated_budget_2.spent, 50);

    // 4. Make Payment (50) -> Success (Total 100 == 100)
    let res_success = ledger_mgr
        .create_payment(&coop_id, alice.did(), bob.did(), 50, currency.clone())
        .await;
    assert!(res_success.is_ok());

    // Verify spent updated
    let updated_budget_3 = budget_store.get("budget-1").unwrap().unwrap();
    assert_eq!(updated_budget_3.spent, 100);
    assert!(updated_budget_3.is_exceeded());

    // Status should update to Exceeded?
    // The implementation of record_spending updates status if exceeded.
    assert_eq!(updated_budget_3.status, BudgetStatus::Exceeded);

    // 5. Make Payment (1) -> Fail (Status is Exceeded or Limit hit)
    let res_fail_2 = ledger_mgr
        .create_payment(&coop_id, alice.did(), bob.did(), 1, currency.clone())
        .await;
    assert!(res_fail_2.is_err());
}
