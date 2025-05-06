use crate::{
    EscrowContract, EscrowInterface, EscrowSystem, RewardSettings, TokenAmount,
    payments::{PaymentInterface, SimplePaymentSystem},
};
use anyhow::Result;
use chrono::{Duration, Utc};
use cid::Cid;
use mesh_reputation::ReputationSystem;
use mesh_types::{ExecutionReceipt, TaskIntent, VerificationReceipt};
use std::sync::Arc;

// Helper function to create a test TaskIntent
fn create_test_task() -> TaskIntent {
    let now = Utc::now();
    let expiry = now + Duration::days(1);
    
    TaskIntent {
        publisher_did: "did:icn:test:publisher".to_string(),
        wasm_cid: Cid::default(),
        input_cid: Cid::default(),
        fee: 1000,
        verifiers: 3,
        expiry,
        metadata: None,
    }
}

// Helper function to create a test ExecutionReceipt
fn create_test_execution_receipt(worker_did: &str) -> ExecutionReceipt {
    ExecutionReceipt {
        worker_did: worker_did.to_string(),
        task_cid: Cid::default(),
        output_cid: Cid::default(),
        output_hash: vec![1, 2, 3, 4],
        fuel_consumed: 500,
        timestamp: Utc::now(),
        signature: vec![],
        metadata: None,
    }
}

// Helper function to create a test VerificationReceipt
fn create_test_verification_receipt(verifier_did: &str, verdict: bool) -> VerificationReceipt {
    VerificationReceipt {
        verifier_did: verifier_did.to_string(),
        receipt_cid: Cid::default(),
        verdict,
        proof_cid: Cid::default(),
        timestamp: Utc::now(),
        signature: vec![],
        metadata: None,
    }
}

#[tokio::test]
async fn test_escrow_contract_flow() -> Result<()> {
    // Create reputation system
    let reputation_policy = mesh_reputation::MeshPolicy {
        alpha: 0.6,
        beta: 0.4,
        gamma: 1.0,
        lambda: 0.01,
        stake_weight: 0.2,
        min_fee: 10,
        capacity_units: 100,
    };
    let reputation = Arc::new(ReputationSystem::new(reputation_policy));
    
    // Create payment system
    let payments = Arc::new(SimplePaymentSystem::new(6));
    
    // Add balances to test accounts
    let publisher_did = "did:icn:test:publisher".to_string();
    let worker_did = "did:icn:test:worker".to_string();
    let verifier1_did = "did:icn:test:verifier1".to_string();
    let verifier2_did = "did:icn:test:verifier2".to_string();
    let verifier3_did = "did:icn:test:verifier3".to_string();
    
    payments.add_balance(&publisher_did, 10000)?;
    
    // Create escrow system
    let escrow = EscrowSystem::new(reputation.clone());
    
    // Create test task
    let task = create_test_task();
    
    // Create reward settings
    let reward_settings = RewardSettings {
        worker_percentage: 70,
        verifier_percentage: 20,
        platform_fee_percentage: 10,
        use_reputation_weighting: true,
        platform_fee_address: "did:icn:platform".to_string(),
    };
    
    // 1. Create escrow contract
    let contract = escrow.create_contract(&task, reward_settings).await?;
    assert_eq!(contract.state, crate::EscrowState::Created);
    
    // 2. Lock tokens for the contract
    let amount = TokenAmount::new(1000, 6);
    let lock = escrow.lock_tokens(&contract.id, amount).await?;
    
    // Verify contract state is now InProgress
    let state = escrow.get_contract_state(&contract.id).await?;
    assert_eq!(state, crate::EscrowState::InProgress);
    
    // 3. Process execution receipt
    let execution_receipt = create_test_execution_receipt(&worker_did);
    escrow.process_execution(&contract.id, &execution_receipt).await?;
    
    // 4. Process verification receipts
    let verification1 = create_test_verification_receipt(&verifier1_did, true);
    let verification2 = create_test_verification_receipt(&verifier2_did, true);
    let verification3 = create_test_verification_receipt(&verifier3_did, false);
    
    escrow.process_verification(&contract.id, &verification1).await?;
    escrow.process_verification(&contract.id, &verification2).await?;
    
    // State should still be InProgress (need majority)
    let state = escrow.get_contract_state(&contract.id).await?;
    assert_eq!(state, crate::EscrowState::InProgress);
    
    // Add the third verification, which should trigger completion
    escrow.process_verification(&contract.id, &verification3).await?;
    
    // Verify contract is now completed
    let state = escrow.get_contract_state(&contract.id).await?;
    assert_eq!(state, crate::EscrowState::Completed);
    
    // Check the rewards have been distributed
    let is_completed = escrow.is_contract_completed(&contract.id).await?;
    assert!(is_completed);
    
    // In a real implementation, we would check the token balances
    // of the worker and verifiers here
    
    Ok(())
}

#[tokio::test]
async fn test_escrow_contract_dispute() -> Result<()> {
    // Create reputation system
    let reputation_policy = mesh_reputation::MeshPolicy {
        alpha: 0.6,
        beta: 0.4,
        gamma: 1.0,
        lambda: 0.01,
        stake_weight: 0.2,
        min_fee: 10,
        capacity_units: 100,
    };
    let reputation = Arc::new(ReputationSystem::new(reputation_policy));
    
    // Create escrow system
    let escrow = EscrowSystem::new(reputation.clone());
    
    // Create test task
    let task = create_test_task();
    
    // Create reward settings
    let reward_settings = RewardSettings {
        worker_percentage: 70,
        verifier_percentage: 20,
        platform_fee_percentage: 10,
        use_reputation_weighting: true,
        platform_fee_address: "did:icn:platform".to_string(),
    };
    
    // 1. Create escrow contract
    let contract = escrow.create_contract(&task, reward_settings).await?;
    assert_eq!(contract.state, crate::EscrowState::Created);
    
    // 2. Lock tokens
    let amount = TokenAmount::new(1000, 6);
    escrow.lock_tokens(&contract.id, amount).await?;
    
    // 3. Create a dispute
    let disputer_did = "did:icn:test:publisher".to_string();
    escrow.create_dispute(&contract.id, &disputer_did, "Task taking too long").await?;
    
    // Verify state is now Disputed
    let state = escrow.get_contract_state(&contract.id).await?;
    assert_eq!(state, crate::EscrowState::Disputed);
    
    Ok(())
} 