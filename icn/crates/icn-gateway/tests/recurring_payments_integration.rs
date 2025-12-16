use icn_gateway::api::recurring_payments::{
    execute_due_payments, PaymentFrequency, RecurringPayment, RecurringPaymentStore,
    RecurringStatus,
};
use icn_gateway::ledger_mgr::LedgerManager;
use icn_identity::IdentityBundle;
use std::sync::Arc;
use std::time::SystemTime;

fn temp_db() -> sled::Db {
    sled::Config::new().temporary(true).open().unwrap()
}

#[tokio::test]
async fn test_scheduler_execution_flow() {
    let db = temp_db();
    let store = RecurringPaymentStore::new(db);
    let ledger_mgr = Arc::new(LedgerManager::new()); // Assuming new() creates temporary storage

    // Setup identities
    let alice = IdentityBundle::generate().unwrap();
    let bob = IdentityBundle::generate().unwrap();
    let coop_id = "test-coop".to_string();

    // Create a payment due now (or slightly in the past)
    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let payment = RecurringPayment {
        id: "daily-pay".to_string(),
        coop_id: coop_id.clone(),
        owner: alice.did().to_string(),
        from_account: alice.did().to_string(),
        to_account: bob.did().to_string(),
        amount: 50,
        currency: "USD".to_string(),
        frequency: PaymentFrequency::Daily,
        next_execution: now - 100, // Due 100s ago
        end_date: None,
        status: RecurringStatus::Active,
        execution_count: 0,
        last_execution: None,
        created_at: now,
        updated_at: now,
    };
    store.insert(payment.clone()).unwrap();

    // Execute due payments
    let results = execute_due_payments(&store, &ledger_mgr).await;

    // Verify execution results
    assert_eq!(results.len(), 1);
    let (id, res) = &results[0];
    assert_eq!(id, "daily-pay");
    assert!(res.is_ok(), "Payment execution failed: {res:?}");

    // Verify store update
    let updated = store.get("daily-pay").unwrap().unwrap();
    assert_eq!(updated.execution_count, 1);
    assert!(updated.last_execution.is_some());
    // Next execution should be execution time + 86400 (approx)
    // Actually existing implementation uses now + 86400.
    // Let's just check it's in the future
    assert!(updated.next_execution > now);

    // Verify ledger transaction
    // Note: get_balance requires currency string
    let alice_balance = ledger_mgr
        .get_balance(&coop_id, alice.did(), "USD")
        .unwrap();
    let bob_balance = ledger_mgr.get_balance(&coop_id, bob.did(), "USD").unwrap();

    // Since it's a credit-based ledger initiated from 0?
    // Wait, create_payment just appends entries.
    // If Alice starts at 0, she goes to -50. Bob goes to +50.
    assert_eq!(alice_balance, -50);
    assert_eq!(bob_balance, 50);

    // Run again - should NOT execute
    let results_2 = execute_due_payments(&store, &ledger_mgr).await;
    assert_eq!(results_2.len(), 0);
}
