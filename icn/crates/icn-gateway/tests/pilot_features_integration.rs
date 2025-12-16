//! Integration tests for pilot-ready features
//!
//! Tests notifications, governance UI, and economic features.

use icn_gateway::api::{
    budgets::{Budget, BudgetPeriod, BudgetStatus, BudgetStore},
    escrow::{Escrow, EscrowCondition, EscrowStatus, EscrowStore},
    recurring_payments::{
        PaymentFrequency, RecurringPayment, RecurringPaymentStore, RecurringStatus,
    },
};

#[tokio::test]
async fn test_recurring_payment_lifecycle() {
    let db = sled::Config::new().temporary(true).open().unwrap();
    let store = RecurringPaymentStore::new(db);

    // Create payment
    let payment = RecurringPayment {
        id: "test-1".to_string(),
        coop_id: "test-coop".to_string(),
        owner: "did:icn:alice".to_string(),
        from_account: "alice-checking".to_string(),
        to_account: "bob-savings".to_string(),
        amount: 100,
        currency: "USD".to_string(),
        frequency: PaymentFrequency::Monthly,
        next_execution: 1000,
        end_date: Some(2000),
        status: RecurringStatus::Active,
        execution_count: 0,
        last_execution: None,
        created_at: 500,
        updated_at: 500,
    };

    store.insert(payment.clone()).unwrap();

    // Retrieve
    let retrieved = store.get("test-1").unwrap();
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.owner, "did:icn:alice");
    assert_eq!(retrieved.amount, 100);
    assert_eq!(retrieved.frequency, PaymentFrequency::Monthly);

    // Update
    let mut updated = retrieved.clone();
    updated.status = RecurringStatus::Paused;
    updated.amount = 150;
    store.update("test-1", updated).unwrap();

    let retrieved = store.get("test-1").unwrap().unwrap();
    assert_eq!(retrieved.status, RecurringStatus::Paused);
    assert_eq!(retrieved.amount, 150);

    // List
    let all = store.list().unwrap();
    assert_eq!(all.len(), 1);

    // Get due payments (none since status is paused)
    let due = store.get_due_payments(2000).unwrap();
    assert_eq!(due.len(), 0);
}

#[tokio::test]
async fn test_escrow_conditions() {
    let db = sled::Config::new().temporary(true).open().unwrap();
    let store = EscrowStore::new(db);

    // Create escrow with approval condition
    let escrow = Escrow {
        id: "escrow-1".to_string(),
        coop_id: "test-coop".to_string(),
        creator: "did:icn:alice".to_string(),
        from_account: "alice-account".to_string(),
        to_account: "bob-account".to_string(),
        amount: 500,
        currency: "USD".to_string(),
        status: EscrowStatus::Pending,
        conditions: vec![EscrowCondition::RequiresApproval {
            did: "did:icn:charlie".to_string(),
        }],
        approvals: Vec::new(),
        expires_at: Some(9999),
        description: "Payment for services".to_string(),
        created_at: 100,
        updated_at: 100,
    };

    store.insert(escrow.clone()).unwrap();

    // Retrieve
    let retrieved = store.get("escrow-1").unwrap();
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.status, EscrowStatus::Pending);
    assert_eq!(retrieved.amount, 500);
    assert_eq!(retrieved.conditions.len(), 1);

    // Add approval
    let mut updated = retrieved.clone();
    updated.approvals.push("did:icn:charlie".to_string());
    updated.status = EscrowStatus::Released;
    store.update("escrow-1", updated).unwrap();

    let retrieved = store.get("escrow-1").unwrap().unwrap();
    assert_eq!(retrieved.status, EscrowStatus::Released);
    assert_eq!(retrieved.approvals.len(), 1);

    // List
    let all = store.list().unwrap();
    assert_eq!(all.len(), 1);
}

#[tokio::test]
async fn test_budget_tracking() {
    let db = sled::Config::new().temporary(true).open().unwrap();
    let store = BudgetStore::new(db);

    // Create budget
    let budget = Budget {
        id: "budget-1".to_string(),
        owner: "did:icn:alice".to_string(),
        account: "alice-spending".to_string(),
        currency: "USD".to_string(),
        limit: 1000,
        spent: 0,
        period: BudgetPeriod::Monthly,
        period_start: 100,
        period_end: 100 + 2592000,
        status: BudgetStatus::Active,
        notification_thresholds: vec![80, 100],
        notified_thresholds: Vec::new(),
        description: "Monthly spending limit".to_string(),
        created_at: 100,
        updated_at: 100,
    };

    store.insert(budget.clone()).unwrap();

    // Check initial status
    let retrieved = store.get("budget-1").unwrap().unwrap();
    assert_eq!(retrieved.spent, 0);
    assert_eq!(retrieved.remaining(), 1000);
    assert_eq!(retrieved.percentage_used(), 0.0);
    assert!(!retrieved.is_exceeded());

    // Record spending
    let notified = store.record_spending("alice-spending", 850).unwrap();

    assert_eq!(notified.len(), 1); // 85% triggers 80% threshold

    let retrieved = store.get("budget-1").unwrap().unwrap();
    assert_eq!(retrieved.spent, 850);
    assert_eq!(retrieved.remaining(), 150);
    assert!(retrieved.percentage_used() > 80.0);
    assert!(!retrieved.is_exceeded());
    assert_eq!(retrieved.notified_thresholds.len(), 1);

    // Exceed budget
    store.record_spending("alice-spending", 200).unwrap();

    let retrieved = store.get("budget-1").unwrap().unwrap();
    assert_eq!(retrieved.spent, 1050);
    assert!(retrieved.is_exceeded());
    assert_eq!(retrieved.status, BudgetStatus::Exceeded);
    assert_eq!(retrieved.notified_thresholds.len(), 2); // Both 80% and 100%

    // Check spending enforcement
    let can_spend = store.check_spending("alice-spending", 100).unwrap();
    assert!(!can_spend); // Should deny since exceeded
}

#[tokio::test]
async fn test_budget_period_calculations() {
    let budget = Budget {
        id: "test".to_string(),
        owner: "did:icn:test".to_string(),
        account: "test-account".to_string(),
        currency: "USD".to_string(),
        limit: 100,
        spent: 85, // Changed to 85 to trigger 80% threshold
        period: BudgetPeriod::Monthly,
        period_start: 0,
        period_end: 2592000,
        status: BudgetStatus::Active,
        notification_thresholds: vec![80, 90, 100],
        notified_thresholds: vec![],
        description: "Test".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    // Test percentage calculations
    assert_eq!(budget.percentage_used(), 85.0);
    assert_eq!(budget.remaining(), 15);
    assert!(!budget.is_exceeded());

    // Test notification thresholds
    assert_eq!(budget.should_notify(), Some(80)); // First unnotified threshold >= 85%
}

#[tokio::test]
async fn test_escrow_time_release() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let escrow = Escrow {
        id: "time-escrow".to_string(),
        coop_id: "test-coop".to_string(),
        creator: "did:icn:alice".to_string(),
        from_account: "alice".to_string(),
        to_account: "bob".to_string(),
        amount: 100,
        currency: "USD".to_string(),
        status: EscrowStatus::Pending,
        conditions: vec![EscrowCondition::TimeRelease {
            timestamp: now + 3600, // 1 hour from now
        }],
        approvals: Vec::new(),
        expires_at: None,
        description: "Time-locked payment".to_string(),
        created_at: now,
        updated_at: now,
    };

    // Verify time condition exists
    assert_eq!(escrow.conditions.len(), 1);
    match &escrow.conditions[0] {
        EscrowCondition::TimeRelease { timestamp } => {
            assert!(*timestamp > now);
        }
        _ => panic!("Expected TimeRelease condition"),
    }
}

#[tokio::test]
async fn test_recurring_payment_frequencies() {
    // Test all frequency variants
    let frequencies = vec![
        PaymentFrequency::Daily,
        PaymentFrequency::Weekly,
        PaymentFrequency::Monthly,
        PaymentFrequency::Yearly,
    ];

    for freq in frequencies {
        let payment = RecurringPayment {
            id: format!("test-{freq:?}"),
            coop_id: "test-coop".to_string(),
            owner: "did:icn:test".to_string(),
            from_account: "test".to_string(),
            to_account: "dest".to_string(),
            amount: 100,
            currency: "USD".to_string(),
            frequency: freq,
            next_execution: 1000,
            end_date: None,
            status: RecurringStatus::Active,
            execution_count: 0,
            last_execution: None,
            created_at: 0,
            updated_at: 0,
        };

        assert_eq!(payment.status, RecurringStatus::Active);
    }
}

#[tokio::test]
async fn test_budget_multiple_accounts() {
    let db = sled::Config::new().temporary(true).open().unwrap();
    let store = BudgetStore::new(db);

    // Create budgets for different accounts
    let budget1 = Budget {
        id: "budget-1".to_string(),
        owner: "did:icn:alice".to_string(),
        account: "account-1".to_string(),
        currency: "USD".to_string(),
        limit: 500,
        spent: 0,
        period: BudgetPeriod::Monthly,
        period_start: 0,
        period_end: 2592000,
        status: BudgetStatus::Active,
        notification_thresholds: vec![80, 100],
        notified_thresholds: vec![],
        description: "Account 1".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    let budget2 = Budget {
        id: "budget-2".to_string(),
        owner: "did:icn:alice".to_string(),
        account: "account-2".to_string(),
        currency: "USD".to_string(),
        limit: 1000,
        spent: 0,
        period: BudgetPeriod::Monthly,
        period_start: 0,
        period_end: 2592000,
        status: BudgetStatus::Active,
        notification_thresholds: vec![80, 100],
        notified_thresholds: vec![],
        description: "Account 2".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    store.insert(budget1).unwrap();
    store.insert(budget2).unwrap();

    // Record spending on account-1
    store.record_spending("account-1", 300).unwrap();

    // Verify budgets are independent
    let b1 = store.get("budget-1").unwrap().unwrap();
    let b2 = store.get("budget-2").unwrap().unwrap();

    assert_eq!(b1.spent, 300);
    assert_eq!(b2.spent, 0);
}
