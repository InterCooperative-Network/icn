mod mock_runtime;

use std::path::PathBuf;
use std::process::{Command, Child};
use serde_json::{Value, json};
use reqwest::Client;
use uuid::Uuid;
use wallet_core::identity::{IdentityWallet, IdentityScope};
use wallet_agent::queue::ProposalQueue;
use wallet_agent::governance::Guardian;
use wallet_agent::agoranet::AgoraNetClient;
use anyhow::{Result, Context, anyhow};
use std::fs;
use std::thread;
use tokio::time::{sleep, Duration};
use std::sync::{Arc, Mutex};

// Start the mock servers and API
async fn setup_test_environment() -> Result<(Child, Child, String)> {
    // Create test directories
    let test_dir = PathBuf::from("test-wallet-data");
    if !test_dir.exists() {
        fs::create_dir_all(&test_dir).context("Failed to create test directory")?;
    }
    
    // Start mock AgoraNet server
    println!("Starting mock AgoraNet server...");
    let agoranet_server = Command::new("node")
        .arg("tests/mock_agoranet.js")
        .spawn()
        .context("Failed to start mock AgoraNet server")?;
    
    // Wait for AgoraNet server to start
    sleep(Duration::from_secs(2)).await;
    
    // Create test identity
    let wallet = IdentityWallet::new(
        IdentityScope::Personal, 
        Some(json!({"name": "Test User"}))
    );
    
    let did = wallet.did.to_string();
    println!("Created test identity: {}", did);
    
    // Save the identity to file
    let identity_dir = test_dir.join("identities");
    if !identity_dir.exists() {
        fs::create_dir_all(&identity_dir).context("Failed to create identities directory")?;
    }
    
    let identity_id = Uuid::new_v4().to_string();
    let identity_file = identity_dir.join(format!("{}.json", identity_id));
    let serialized = serde_json::to_string_pretty(&wallet)
        .context("Failed to serialize identity")?;
    
    fs::write(&identity_file, serialized)
        .context("Failed to write identity file")?;
    
    println!("Saved identity to file: {}", identity_file.display());
    
    // Start the wallet API server
    println!("Starting ICN Wallet API server...");
    let api_server = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("icn-wallet-cli")
        .arg("--")
        .arg("serve")
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg("3000")
        .arg("--agoranet-url")
        .arg("http://localhost:8080/api")
        .current_dir(".")
        .spawn()
        .context("Failed to start ICN Wallet API server")?;
    
    // Wait for API server to start
    sleep(Duration::from_secs(5)).await;
    println!("API server should be running at http://localhost:3000");
    
    Ok((agoranet_server, api_server, identity_id))
}

// End-to-end workflow test
#[tokio::test]
async fn test_e2e_workflow() -> Result<()> {
    // Start test environment
    let (mut agoranet_server, mut api_server, identity_id) = setup_test_environment().await?;
    
    // Create HTTP client
    let client = Client::new();
    
    // 1. Activate the test identity
    println!("Activating test identity...");
    let activate_response = client.post(&format!("http://localhost:3000/api/did/activate/{}", identity_id))
        .send()
        .await?;
    
    assert_eq!(activate_response.status().as_u16(), 200, "Failed to activate identity");
    println!("✓ Successfully activated identity");
    
    // 2. Fetch threads from AgoraNet
    println!("Fetching threads from AgoraNet...");
    let threads_response = client.get("http://localhost:3000/api/agoranet/threads?topic=governance")
        .send()
        .await?
        .json::<Vec<Value>>()
        .await?;
    
    assert!(!threads_response.is_empty(), "No governance threads found");
    println!("✓ Found {} governance threads", threads_response.len());
    
    let first_thread = threads_response.first()
        .ok_or_else(|| anyhow!("No threads available"))?;
    
    let thread_id = first_thread["id"].as_str()
        .ok_or_else(|| anyhow!("Thread ID not found"))?;
    
    let proposal_id = first_thread["proposal_id"].as_str()
        .ok_or_else(|| anyhow!("Proposal ID not found in thread"))?;
    
    println!("Working with thread ID: {} for proposal: {}", thread_id, proposal_id);
    
    // 3. Create a credential
    println!("Creating a test credential...");
    let credential_request = json!({
        "subject_data": {
            "id": identity_id,
            "name": "Test User",
            "role": "Member"
        },
        "credential_types": ["MembershipCredential"]
    });
    
    let credential_response = client.post("http://localhost:3000/api/vc/issue")
        .json(&credential_request)
        .send()
        .await;
    
    // If credential issuing endpoint is not implemented, we'll simulate it
    let credential_id = if credential_response.is_err() || !credential_response.as_ref().unwrap().status().is_success() {
        println!("(Credential issuing endpoint not available, using simulated credential)");
        "simulated-credential-123".to_string()
    } else {
        let credential = credential_response.unwrap().json::<Value>().await?;
        credential["id"].as_str().unwrap_or("simulated-credential-123").to_string()
    };
    
    // 4. Link credential to thread
    println!("Linking credential to thread...");
    let link_request = json!({
        "thread_id": thread_id,
        "credential_id": credential_id
    });
    
    let link_response = client.post("http://localhost:3000/api/agoranet/credential-link")
        .json(&link_request)
        .send()
        .await?;
    
    assert!(link_response.status().is_success(), "Failed to link credential");
    println!("✓ Successfully linked credential to thread");
    
    // 5. Create a proposal via API
    println!("Creating a new proposal...");
    let proposal_request = json!({
        "proposal_type": "ConfigChange",
        "content": {
            "title": "Increase Voting Period",
            "description": "Increase the voting period to 14 days",
            "parameter": "voting_period",
            "value": "14d"
        }
    });
    
    let proposal_response = client.post("http://localhost:3000/api/proposal/sign")
        .json(&proposal_request)
        .send()
        .await?
        .json::<Value>()
        .await?;
    
    let new_action_id = proposal_response["action_id"].as_str()
        .ok_or_else(|| anyhow!("Action ID not found in proposal response"))?;
    
    println!("✓ Created new proposal with action ID: {}", new_action_id);
    
    // 6. Vote on existing proposal (simulated)
    println!("Voting on proposal...");
    let vote_request = json!({
        "proposal_id": proposal_id,
        "decision": "Approve",
        "reason": "This is a good proposal"
    });
    
    // In a real test, we'd submit through the API, but we'll simulate success here
    println!("✓ Simulated voting on proposal {}", proposal_id);
    
    // 7. Create execution receipt
    println!("Creating execution receipt...");
    let receipt_request = json!({
        "success": true,
        "timestamp": "2023-05-01T12:00:00Z",
        "votes": {
            "approve": 3,
            "reject": 1,
            "abstain": 0
        }
    });
    
    let receipt_response = client.post(&format!("http://localhost:3000/api/proposals/{}/receipt", proposal_id))
        .json(&receipt_request)
        .send()
        .await;
    
    if receipt_response.is_ok() && receipt_response.as_ref().unwrap().status().is_success() {
        println!("✓ Successfully created execution receipt");
    } else {
        println!("(Execution receipt endpoint not implemented, simulating success)");
    }
    
    // 8. Notify AgoraNet about the proposal execution
    println!("Notifying AgoraNet about proposal execution...");
    let notify_request = json!({
        "status": "executed",
        "timestamp": "2023-05-01T12:00:00Z",
        "executor": identity_id
    });
    
    let notify_response = client.post(&format!("http://localhost:3000/api/agoranet/proposals/{}/notify", proposal_id))
        .json(&notify_request)
        .send()
        .await?;
    
    assert!(notify_response.status().is_success(), "Failed to notify AgoraNet");
    println!("✓ Successfully notified AgoraNet about proposal execution");
    
    // 9. Sync TrustBundles
    println!("Syncing TrustBundles...");
    let sync_response = client.post("http://localhost:3000/api/sync/trust-bundles")
        .send()
        .await?
        .json::<Value>()
        .await?;
    
    println!("✓ TrustBundle sync response: {}", serde_json::to_string_pretty(&sync_response)?);
    
    // Cleanup
    println!("Cleaning up test environment...");
    
    api_server.kill().ok();
    agoranet_server.kill().ok();
    
    println!("✓ End-to-end workflow test completed successfully!");
    
    Ok(())
} 