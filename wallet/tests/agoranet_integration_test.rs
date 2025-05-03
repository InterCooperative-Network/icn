use std::env;
use std::process::Command;
use std::path::PathBuf;
use serde_json::{Value, json};
use reqwest::Client;
use tokio::time::{sleep, Duration};
use anyhow::{Result, anyhow, Context};

// This test requires the mock_agoranet.js server to be running
// Run with: cargo test -- --test agoranet_integration_test --nocapture
#[tokio::test]
async fn test_agoranet_integration() -> Result<()> {
    // Check if AgoraNet server is running
    let client = Client::new();
    
    // Try to connect to health endpoint
    let health_check = client.get("http://localhost:8080/api/health")
        .send()
        .await;
        
    if health_check.is_err() {
        eprintln!("Warning: AgoraNet server not running. Starting mock server...");
        
        // Start the mock server
        Command::new("node")
            .arg("tests/mock_agoranet.js")
            .spawn()
            .context("Failed to start mock AgoraNet server")?;
            
        // Wait for server to start
        sleep(Duration::from_secs(3)).await;
        
        // Verify server is running
        client.get("http://localhost:8080/api/health")
            .send()
            .await
            .context("Mock AgoraNet server failed to start")?;
    }
    
    println!("AgoraNet server is running");
    
    // 1. Test fetching threads
    let threads_response = client.get("http://localhost:8080/api/threads")
        .send()
        .await?
        .json::<Vec<Value>>()
        .await?;
        
    assert!(!threads_response.is_empty(), "Expected threads from AgoraNet");
    println!("✓ Successfully fetched {} threads from AgoraNet", threads_response.len());
    
    // 2. Test fetching a specific thread
    if let Some(thread) = threads_response.first() {
        let thread_id = thread["id"].as_str().unwrap();
        
        let thread_detail = client.get(&format!("http://localhost:8080/api/threads/{}", thread_id))
            .send()
            .await?
            .json::<Value>()
            .await?;
            
        assert_eq!(thread_detail["id"], thread["id"], "Thread ID mismatch");
        println!("✓ Successfully fetched thread details for thread {}", thread_id);
        
        // 3. Test linking a credential to a thread
        let credential = json!({
            "id": "test-credential-1",
            "type": ["VerifiableCredential", "MembershipCredential"],
            "issuer": "did:icn:test-issuer",
            "issuanceDate": "2023-05-03T00:00:00Z",
            "credentialSubject": {
                "id": "did:icn:test-subject",
                "role": "Member"
            }
        });
        
        let link_request = json!({
            "thread_id": thread_id,
            "credential": credential
        });
        
        let link_response = client.post("http://localhost:8080/api/threads/credential-link")
            .json(&link_request)
            .send()
            .await?
            .json::<Value>()
            .await?;
            
        assert_eq!(link_response["thread_id"], thread_id, "Thread ID mismatch in credential link");
        println!("✓ Successfully linked credential to thread {}", thread_id);
        
        // 4. Test fetching credential links
        let links_response = client.get(&format!("http://localhost:8080/api/threads/{}/credential-links", thread_id))
            .send()
            .await?
            .json::<Vec<Value>>()
            .await?;
            
        assert!(!links_response.is_empty(), "Expected credential links");
        println!("✓ Successfully fetched {} credential links for thread {}", links_response.len(), thread_id);
        
        // 5. Test notifying about proposal events
        let proposal_id = thread["proposal_id"].as_str().unwrap_or("proposal1");
        
        let event_request = json!({
            "event_type": "status_update",
            "details": {
                "status": "approved",
                "votes": 3
            },
            "timestamp": "2023-05-04T00:00:00Z"
        });
        
        let event_response = client.post(&format!("http://localhost:8080/api/proposals/{}/events", proposal_id))
            .json(&event_request)
            .send()
            .await?
            .json::<Value>()
            .await?;
            
        assert_eq!(event_response["success"], json!(true), "Event notification failed");
        println!("✓ Successfully notified about proposal event for proposal {}", proposal_id);
    }
    
    println!("✓ All AgoraNet integration tests passed!");
    
    Ok(())
} 