mod mock_runtime;

use std::path::PathBuf;
use serde_json::{Value, json};
use uuid::Uuid;
use wallet_core::identity::{IdentityWallet, IdentityScope};
use wallet_agent::queue::ProposalQueue;
use wallet_agent::governance::Guardian;
use anyhow::{Result, Context};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::fs;

// Integration test for the ICN Runtime interactions
#[tokio::test]
async fn test_runtime_integration() -> Result<()> {
    // Create test directory
    let test_dir = PathBuf::from("test-wallet-data");
    if !test_dir.exists() {
        fs::create_dir_all(&test_dir).context("Failed to create test directory")?;
    }
    
    // Create test identity
    let wallet = IdentityWallet::new(
        IdentityScope::Personal, 
        Some(json!({"name": "Test Guardian"}))
    );
    
    println!("Created test identity: {}", wallet.did);
    
    // Create a mock runtime
    let runtime = mock_runtime::create_test_runtime();
    
    // Make the test identity a guardian in the runtime
    runtime.add_guardian(&wallet.did.to_string())?;
    println!("Added identity as guardian in runtime");
    
    // Create queue and guardian
    let queue_dir = test_dir.join("queue");
    let bundle_dir = test_dir.join("bundles");
    
    if !queue_dir.exists() {
        fs::create_dir_all(&queue_dir).context("Failed to create queue directory")?;
    }
    
    if !bundle_dir.exists() {
        fs::create_dir_all(&bundle_dir).context("Failed to create bundles directory")?;
    }
    
    let queue = ProposalQueue::new(&queue_dir, wallet.clone());
    let guardian = Guardian::new(wallet.clone(), queue.clone())
        .with_bundle_storage(&bundle_dir);
    
    // 1. Test creating a proposal
    println!("Testing proposal creation...");
    let proposal_content = json!({
        "title": "Test Config Change",
        "description": "Change voting period to 10 days",
        "config_key": "voting_period",
        "config_value": "10d"
    });
    
    let action_id = guardian.create_proposal("ConfigChange", proposal_content.clone())?;
    println!("✓ Created proposal with action ID: {}", action_id);
    
    // 2. Simulate runtime accepting proposal
    let proposal_id = runtime.handle_proposal(proposal_content)?;
    println!("✓ Runtime accepted proposal with ID: {}", proposal_id);
    
    // 3. Test voting
    println!("Testing voting...");
    let vote_decision = wallet_agent::governance::VoteDecision::Approve;
    let vote_action_id = guardian.create_vote(&proposal_id, vote_decision, Some("I support this change".to_string()))?;
    println!("✓ Created vote with action ID: {}", vote_action_id);
    
    // 4. Simulate another guardian voting in the runtime
    let guardian2_vote = json!({
        "proposal_id": proposal_id,
        "decision": "Approve",
        "reason": "This improves governance",
        "timestamp": chrono::Utc::now().timestamp()
    });
    
    runtime.handle_vote(guardian2_vote)?;
    println!("✓ Runtime accepted vote from second guardian");
    
    // 5. Test execution
    println!("Testing proposal execution...");
    let execution_result = runtime.handle_execute(&proposal_id)?;
    println!("✓ Runtime executed proposal with result: {}", execution_result);
    
    // 6. Test creating execution receipt
    let receipt = guardian.create_execution_receipt(&proposal_id, execution_result)?;
    println!("✓ Created execution receipt for proposal {}", proposal_id);
    
    // 7. Test trust bundle synchronization
    println!("Testing trust bundle synchronization...");
    
    // Get the trust bundles from runtime
    let runtime_bundles = runtime.get_trust_bundles();
    println!("Got {} trust bundles from runtime", runtime_bundles.len());
    
    // Convert to TrustBundle format and store them
    for bundle_value in runtime_bundles {
        let bundle = wallet_agent::governance::TrustBundle {
            id: bundle_value["id"].as_str().unwrap_or("").to_string(),
            name: bundle_value["name"].as_str().unwrap_or("").to_string(),
            version: bundle_value["version"].as_i64().unwrap_or(0),
            guardians: bundle_value["guardians"].as_array()
                .unwrap_or(&vec![])
                .iter()
                .map(|g| g.as_str().unwrap_or("").to_string())
                .collect(),
            threshold: bundle_value["threshold"].as_u64().unwrap_or(0) as usize,
            active: bundle_value["active"].as_bool().unwrap_or(false),
        };
        
        guardian.store_trust_bundle(bundle).await?;
    }
    
    println!("✓ Stored trust bundles from runtime");
    
    // 8. Test checking guardian status
    let is_guardian = guardian.is_active_guardian().await?;
    println!("Is active guardian: {}", is_guardian);
    assert!(is_guardian, "Expected identity to be recognized as an active guardian");
    
    println!("✓ All Runtime integration tests passed!");
    
    Ok(())
} 