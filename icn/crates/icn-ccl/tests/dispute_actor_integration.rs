//! Integration tests for DisputeActor

use icn_ccl::{
    BinOp, Contract, DisputeActor, DisputeConfig, DisputeEvidence, DisputeReason, Expr, Rule, Stmt,
    Value,
};
use icn_identity::KeyPair;
use icn_store::SledStore;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

fn make_test_contract() -> Contract {
    use icn_ccl::ast::Param;

    // Simple contract that adds two numbers
    let add_rule = Rule {
        name: "add".to_string(),
        params: vec![
            Param {
                name: "a".to_string(),
            },
            Param {
                name: "b".to_string(),
            },
        ],
        requires: vec![],
        body: vec![Stmt::Return {
            value: Expr::BinOp {
                op: BinOp::Add,
                left: Box::new(Expr::Var("a".to_string())),
                right: Box::new(Expr::Var("b".to_string())),
            },
        }],
    };

    Contract {
        name: "test_add".to_string(),
        participants: vec![],
        currency: None,
        state_vars: vec![],
        rules: vec![add_rule],
        triggers: vec![],
    }
}

#[tokio::test]
async fn test_dispute_actor_file_and_investigate() {
    // Create dispute actor
    let dispute_store: Arc<dyn icn_store::Store> = Arc::new(SledStore::temporary().unwrap());
    let config = DisputeConfig::default();
    let handle = DisputeActor::spawn(config, dispute_store);

    // Create test data
    let task_hash = [1u8; 32];
    let executor_keypair = KeyPair::generate().unwrap();
    let executor = executor_keypair.did().clone();
    let challenger_keypair = KeyPair::generate().unwrap();
    let challenger = challenger_keypair.did().clone();

    // File a dispute
    let evidence = DisputeEvidence {
        task_hash,
        claimed_result: Value::Int(10), // Executor claimed wrong result
        reason: DisputeReason::IncorrectResult {
            expected: Value::Int(5), // Challenger says it should be 5
            actual: Value::Int(10),
        },
        additional_data: vec![],
        filed_at: SystemTime::now(),
    };

    let dispute_id = handle
        .file_dispute(task_hash, executor.clone(), challenger.clone(), evidence)
        .await
        .expect("Failed to file dispute");

    // Verify dispute was created
    let dispute = handle.get_dispute(dispute_id).await;
    assert!(dispute.is_some());
    let dispute = dispute.unwrap();
    assert_eq!(dispute.executor, executor);
    assert_eq!(dispute.challenger, challenger);

    // Investigate the dispute
    let contract = make_test_contract();
    let mut args = HashMap::new();
    args.insert("a".to_string(), Value::Int(2));
    args.insert("b".to_string(), Value::Int(3));

    let outcome = handle
        .investigate_dispute(dispute_id, contract, "add".to_string(), args)
        .await
        .expect("Failed to investigate dispute");

    // The correct answer is 5, executor claimed 10, challenger expected 5
    // So the challenger (submitter) was correct
    match outcome {
        icn_ccl::DisputeOutcome::SubmitterCorrect { verified_result } => {
            assert_eq!(verified_result, Value::Int(5));
        }
        other => panic!("Expected SubmitterCorrect, got {other:?}"),
    }

    // Check stats
    let stats = handle.get_stats().await;
    assert_eq!(stats.total_disputes, 1);
    assert_eq!(stats.resolved_auto, 1);
    assert_eq!(stats.submitter_correct, 1);
}

#[tokio::test]
async fn test_dispute_actor_mediator_management() {
    // Create dispute actor
    let dispute_store: Arc<dyn icn_store::Store> = Arc::new(SledStore::temporary().unwrap());
    let config = DisputeConfig::default();
    let handle = DisputeActor::spawn(config, dispute_store);

    // Add mediators
    let mediator1_keypair = KeyPair::generate().unwrap();
    let mediator1 = mediator1_keypair.did().clone();
    let mediator2_keypair = KeyPair::generate().unwrap();
    let mediator2 = mediator2_keypair.did().clone();

    handle.add_mediator(mediator1.clone()).await;
    handle.add_mediator(mediator2.clone()).await;

    // Note: We can't directly verify the mediator pool size through the handle
    // as it's internal state. In a real scenario, we'd file a dispute that
    // becomes inconclusive and verify a mediator was assigned.

    // Remove a mediator
    handle.remove_mediator(mediator1).await;

    // If we had a way to get mediator pool size, we'd verify here
    // For now, this test just ensures the API works without panicking
}

#[tokio::test]
async fn test_dispute_actor_concurrent_operations() {
    // Create dispute actor
    let dispute_store: Arc<dyn icn_store::Store> = Arc::new(SledStore::temporary().unwrap());
    let config = DisputeConfig::default();
    let handle = DisputeActor::spawn(config, dispute_store);

    // File multiple disputes concurrently
    let mut handles_vec = vec![];
    for i in 0..5 {
        let h = handle.clone();
        let task_hash = [i; 32];
        let executor_keypair = KeyPair::generate().unwrap();
        let executor = executor_keypair.did().clone();
        let challenger_keypair = KeyPair::generate().unwrap();
        let challenger = challenger_keypair.did().clone();

        let handle_task = tokio::spawn(async move {
            let evidence = DisputeEvidence {
                task_hash,
                claimed_result: Value::Int(10),
                reason: DisputeReason::IncorrectResult {
                    expected: Value::Int(5),
                    actual: Value::Int(10),
                },
                additional_data: vec![],
                filed_at: SystemTime::now(),
            };

            h.file_dispute(task_hash, executor, challenger, evidence)
                .await
        });
        handles_vec.push(handle_task);
    }

    // Wait for all disputes to be filed
    for h in handles_vec {
        h.await.unwrap().expect("Failed to file dispute");
    }

    // Check stats
    let stats = handle.get_stats().await;
    assert_eq!(stats.total_disputes, 5);
    assert_eq!(stats.pending, 5);
}
