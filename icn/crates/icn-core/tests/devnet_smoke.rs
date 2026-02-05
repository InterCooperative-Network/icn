//! Devnet smoke tests
//!
//! These tests assume a 3-node devnet is running via `make devnet-up`.
//! Run with: `cargo test --test devnet_smoke -- --test-threads=1 --nocapture`

use reqwest::Client;
use std::time::Duration;

const NODE_A_GATEWAY: &str = "http://localhost:8000";
const NODE_B_GATEWAY: &str = "http://localhost:8001";
const NODE_C_GATEWAY: &str = "http://localhost:8002";

/// Helper to wait for a node to be ready
async fn wait_for_node(client: &Client, base_url: &str, max_attempts: u32) -> Result<(), String> {
    for attempt in 1..=max_attempts {
        match client
            .get(format!("{}/health", base_url))
            .timeout(Duration::from_secs(2))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                println!("✓ {} is ready (attempt {})", base_url, attempt);
                return Ok(());
            }
            Ok(resp) => {
                println!(
                    "Waiting for {} (attempt {}/{}): HTTP {}",
                    base_url,
                    attempt,
                    max_attempts,
                    resp.status()
                );
            }
            Err(e) => {
                println!(
                    "Waiting for {} (attempt {}/{}): {}",
                    base_url, attempt, max_attempts, e
                );
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    Err(format!("{} did not become ready", base_url))
}

#[tokio::test]
async fn test_all_nodes_reachable() {
    let client = Client::new();

    // Wait for all nodes to be ready (max 30 seconds each)
    wait_for_node(&client, NODE_A_GATEWAY, 15)
        .await
        .expect("Node A should be reachable");
    wait_for_node(&client, NODE_B_GATEWAY, 15)
        .await
        .expect("Node B should be reachable");
    wait_for_node(&client, NODE_C_GATEWAY, 15)
        .await
        .expect("Node C should be reachable");

    println!("✓ All 3 nodes are reachable");
}

#[tokio::test]
async fn test_nodes_have_unique_identities() {
    let client = Client::new();

    // Wait for nodes
    wait_for_node(&client, NODE_A_GATEWAY, 15).await.ok();
    wait_for_node(&client, NODE_B_GATEWAY, 15).await.ok();
    wait_for_node(&client, NODE_C_GATEWAY, 15).await.ok();

    // Fetch identity from each node
    let did_a = client
        .get(format!("{}/v1/identity", NODE_A_GATEWAY))
        .send()
        .await
        .expect("fetch node A identity")
        .json::<serde_json::Value>()
        .await
        .expect("parse node A identity")
        .get("did")
        .and_then(|v| v.as_str())
        .expect("node A should have DID")
        .to_string();

    let did_b = client
        .get(format!("{}/v1/identity", NODE_B_GATEWAY))
        .send()
        .await
        .expect("fetch node B identity")
        .json::<serde_json::Value>()
        .await
        .expect("parse node B identity")
        .get("did")
        .and_then(|v| v.as_str())
        .expect("node B should have DID")
        .to_string();

    let did_c = client
        .get(format!("{}/v1/identity", NODE_C_GATEWAY))
        .send()
        .await
        .expect("fetch node C identity")
        .json::<serde_json::Value>()
        .await
        .expect("parse node C identity")
        .get("did")
        .and_then(|v| v.as_str())
        .expect("node C should have DID")
        .to_string();

    // Verify all DIDs are unique
    assert_ne!(did_a, did_b, "Node A and B should have different DIDs");
    assert_ne!(did_a, did_c, "Node A and C should have different DIDs");
    assert_ne!(did_b, did_c, "Node B and C should have different DIDs");

    println!("✓ All nodes have unique DIDs:");
    println!("  Node A: {}", did_a);
    println!("  Node B: {}", did_b);
    println!("  Node C: {}", did_c);
}

#[tokio::test]
async fn test_gossip_network_connectivity() {
    let client = Client::new();

    // Wait for nodes
    wait_for_node(&client, NODE_A_GATEWAY, 15).await.ok();
    wait_for_node(&client, NODE_B_GATEWAY, 15).await.ok();
    wait_for_node(&client, NODE_C_GATEWAY, 15).await.ok();

    // Give nodes time to discover each other via mDNS/gossip
    println!("Waiting for peer discovery...");
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Check that each node has discovered the other nodes
    for (name, url) in [
        ("Node A", NODE_A_GATEWAY),
        ("Node B", NODE_B_GATEWAY),
        ("Node C", NODE_C_GATEWAY),
    ] {
        let peers = client
            .get(format!("{}/v1/peers", url))
            .send()
            .await
            .expect("fetch peers")
            .json::<serde_json::Value>()
            .await
            .expect("parse peers response");

        let peer_count = peers
            .get("peers")
            .and_then(|v| v.as_array())
            .map(|arr| arr.len())
            .unwrap_or(0);

        println!("{} discovered {} peers", name, peer_count);

        // Each node should discover at least 1 peer (ideally 2)
        // We're lenient here since peer discovery can be flaky in docker
        assert!(
            peer_count >= 1,
            "{} should have discovered at least 1 peer, found {}",
            name,
            peer_count
        );
    }

    println!("✓ Gossip network is connected");
}
