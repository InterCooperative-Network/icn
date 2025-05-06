use anyhow::Result;
use chrono::{Duration, Utc};
use cid::Cid;
use mesh_escrow::{
    EscrowInterface, EscrowSystem, RewardSettings, TokenAmount,
    contracts::EscrowSystem,
    payments::{PaymentInterface, SimplePaymentSystem},
};
use mesh_reputation::{MeshPolicy, ReputationSystem};
use mesh_types::{ExecutionReceipt, TaskIntent, VerificationReceipt};
use std::{sync::Arc, time::Duration as StdDuration};
use tokio::time::sleep;
use tracing_subscriber::{fmt, EnvFilter};

async fn setup_logging() {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .init();
}

async fn create_test_task() -> TaskIntent {
    let now = Utc::now();
    let expiry = now + Duration::days(1);
    
    TaskIntent {
        publisher_did: "did:icn:alice".to_string(),
        wasm_cid: Cid::default(),
        input_cid: Cid::default(),
        fee: 1000,
        verifiers: 2,
        expiry,
        metadata: None,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Setup logging
    setup_logging().await;
    
    println!("=== Mesh Compute Escrow Flow Example ===");
    println!("This example demonstrates the token escrow and reward distribution flow for mesh compute tasks.\n");
    
    // Create reputation system
    let policy = MeshPolicy {
        alpha: 0.6,
        beta: 0.4,
        gamma: 0.2,
        lambda: 0.01,
        stake_weight: 0.3,
        min_fee: 10,
        capacity_units: 100,
    };
    let reputation = Arc::new(ReputationSystem::new(policy));
    
    // Create payment system
    let payments = Arc::new(SimplePaymentSystem::new(6));
    
    // Add funds to test accounts
    let alice = "did:icn:alice".to_string();
    let bob = "did:icn:bob".to_string();
    let carol = "did:icn:carol".to_string();
    let dave = "did:icn:dave".to_string();
    
    payments.add_balance(&alice, 10000)?;
    
    // Create escrow system
    let escrow = EscrowSystem::new(reputation.clone());
    
    // Create a task
    println!("1. Creating a new compute task...");
    let task = create_test_task().await;
    println!("   Task created by: {}", task.publisher_did);
    println!("   Fee offered: {} tokens", task.fee);
    println!("   Verifiers required: {}", task.verifiers);
    
    // Create reward settings
    let reward_settings = RewardSettings {
        worker_percentage: 70,
        verifier_percentage: 20,
        platform_fee_percentage: 10,
        use_reputation_weighting: true,
        platform_fee_address: "did:icn:platform".to_string(),
    };
    
    // Create escrow contract
    println!("\n2. Creating escrow contract...");
    let contract = escrow.create_contract(&task, reward_settings).await?;
    println!("   Contract ID: {}", contract.id);
    println!("   Contract state: {:?}", contract.state);
    println!("   Publisher: {}", contract.publisher_did);
    println!("   Expiry: {}", contract.expires_at);
    
    // Lock tokens
    println!("\n3. Locking tokens in escrow...");
    let amount = TokenAmount::new(task.fee, 6);
    let lock = escrow.lock_tokens(&contract.id, amount).await?;
    println!("   Lock ID: {}", lock.id);
    println!("   Amount locked: {} tokens", lock.amount.value);
    println!("   Owner: {}", lock.owner_did);
    
    // Check contract state
    let state = escrow.get_contract_state(&contract.id).await?;
    println!("   Contract state: {:?}", state);
    
    // Wait a moment
    sleep(StdDuration::from_secs(1)).await;
    
    // Execute task
    println!("\n4. Bob executes the task...");
    let execution_receipt = ExecutionReceipt {
        worker_did: bob.clone(),
        task_cid: task.wasm_cid.clone(),
        output_cid: Cid::default(),
        output_hash: vec![1, 2, 3, 4],
        fuel_consumed: 500,
        timestamp: Utc::now(),
        signature: vec![],
        metadata: None,
    };
    
    // Process execution
    escrow.process_execution(&contract.id, &execution_receipt).await?;
    println!("   Execution completed by: {}", bob);
    println!("   Fuel consumed: {}", execution_receipt.fuel_consumed);
    
    // Verify task (Carol verifies positively)
    println!("\n5. Carol verifies the task execution (approves)...");
    let verification1 = VerificationReceipt {
        verifier_did: carol.clone(),
        receipt_cid: Cid::default(),
        verdict: true,
        proof_cid: Cid::default(),
        timestamp: Utc::now(),
        signature: vec![],
        metadata: None,
    };
    
    escrow.process_verification(&contract.id, &verification1).await?;
    println!("   Verification from: {}", carol);
    println!("   Verdict: {}", verification1.verdict);
    
    // Check contract state
    let state = escrow.get_contract_state(&contract.id).await?;
    println!("   Contract state: {:?}", state);
    
    // Dave verifies the task (rejects)
    println!("\n6. Dave verifies the task execution (rejects)...");
    let verification2 = VerificationReceipt {
        verifier_did: dave.clone(),
        receipt_cid: Cid::default(),
        verdict: false,
        proof_cid: Cid::default(),
        timestamp: Utc::now(),
        signature: vec![],
        metadata: None,
    };
    
    escrow.process_verification(&contract.id, &verification2).await?;
    println!("   Verification from: {}", dave);
    println!("   Verdict: {}", verification2.verdict);
    
    // Check final contract state
    let state = escrow.get_contract_state(&contract.id).await?;
    println!("\n7. Final contract state: {:?}", state);
    
    // Check if completed
    let is_completed = escrow.is_contract_completed(&contract.id).await?;
    
    // If completed, show reward distribution
    if is_completed {
        println!("\n8. Contract completed, rewards distributed:");
        // In a real scenario, we'd show the actual reward distribution details
        println!("   Bob (worker): {} tokens (70%)", task.fee as f64 * 0.7);
        println!("   Carol (verifier): {} tokens (20%)", task.fee as f64 * 0.2);
        println!("   Platform fee: {} tokens (10%)", task.fee as f64 * 0.1);
    } else {
        println!("\n8. Contract not completed yet, no rewards distributed.");
    }
    
    // Example of a dispute
    if !is_completed {
        println!("\n9. Alice is disputing the contract...");
        escrow.create_dispute(&contract.id, &alice, "Results not as expected").await?;
        
        let state = escrow.get_contract_state(&contract.id).await?;
        println!("   New contract state: {:?}", state);
    }
    
    println!("\nExample completed successfully!");
    
    Ok(())
} 