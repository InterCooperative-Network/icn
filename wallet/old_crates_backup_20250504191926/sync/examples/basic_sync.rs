use std::env;
use std::error::Error;

use serde_json::json;
use icn_wallet_sync::{SyncClient, SyncService, DagNode, TrustManager, TrustBundle};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    // Get node URL from environment or use default
    let node_url = env::var("ICN_NODE_URL").unwrap_or_else(|_| "http://localhost:8080".to_string());
    
    // Create sync client
    let client = SyncClient::new(node_url);
    
    // Create sync service
    let sync_service = SyncService::new(client.clone());
    
    println!("Connected to ICN node at: {}", client.base_url);
    
    // Create a sample DAG node
    let node = DagNode::new(
        "example-id".to_string(),
        json!({
            "type": "Example",
            "name": "Test Node",
            "attributes": {
                "category": "example",
                "version": "1.0.0"
            }
        }),
        vec![],
    );
    
    // Submit the node with retries
    match sync_service.submit_node_with_retry(&node).await {
        Ok(response) => {
            println!("Successfully submitted node:");
            println!("  ID: {}", response.id);
            println!("  Timestamp: {}", response.timestamp);
            
            if let Some(block_number) = response.block_number {
                println!("  Block: {}", block_number);
            }
        },
        Err(e) => {
            eprintln!("Failed to submit node: {}", e);
        }
    }
    
    // Create and submit a trust bundle
    let trust_manager = TrustManager::new(client.clone());
    
    let mut trust_bundle = TrustBundle::new(
        "Example Trust Bundle".to_string(),
        "did:icn:example-issuer".to_string(),
        vec![
            "did:icn:trusted-1".to_string(),
            "did:icn:trusted-2".to_string(),
        ],
    );
    
    match trust_manager.submit_trust_bundle(&mut trust_bundle).await {
        Ok(bundle_id) => {
            println!("Successfully submitted trust bundle:");
            println!("  ID: {}", bundle_id);
            
            // Try to retrieve the trust bundle
            match trust_manager.get_trust_bundle(&bundle_id).await {
                Ok(retrieved_bundle) => {
                    println!("Retrieved trust bundle:");
                    println!("  Name: {}", retrieved_bundle.name);
                    println!("  Trusted DIDs: {:?}", retrieved_bundle.trusted_dids);
                },
                Err(e) => {
                    eprintln!("Failed to retrieve trust bundle: {}", e);
                }
            }
        },
        Err(e) => {
            eprintln!("Failed to submit trust bundle: {}", e);
        }
    }
    
    // Try to discover federation nodes
    match client.discover_federation().await {
        Ok(endpoints) => {
            println!("Discovered federation nodes:");
            for (i, endpoint) in endpoints.iter().enumerate() {
                println!("  {}. {}", i+1, endpoint);
            }
        },
        Err(e) => {
            eprintln!("Failed to discover federation: {}", e);
        }
    }
    
    Ok(())
} 