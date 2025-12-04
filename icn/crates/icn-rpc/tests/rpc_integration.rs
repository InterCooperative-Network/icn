//! RPC server integration tests
//!
//! These tests validate end-to-end RPC functionality:
//! 1. Server startup and basic connectivity
//! 2. Authentication flow (challenge-response-token)
//! 3. Protected method access with valid/invalid tokens
//! 4. Error handling and response formats

use anyhow::Result;
use icn_identity::KeyPair;
use icn_rpc::{RpcClient, RpcServer};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

/// Find an available port for testing
fn find_available_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Start an RPC server and return the address
async fn start_test_server(with_auth: bool) -> Result<(SocketAddr, tokio::task::JoinHandle<()>)> {
    let port = find_available_port();
    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse()?;

    let server = if with_auth {
        // Use a test secret for JWT signing
        let jwt_secret = b"test-secret-for-rpc-integration-tests-32bytes".to_vec();
        RpcServer::new_with_auth(addr, jwt_secret)
    } else {
        RpcServer::new(addr)
    };

    let handle = tokio::spawn(async move {
        if let Err(e) = server.run().await {
            eprintln!("Server error: {e}");
        }
    });

    // Give server time to start
    sleep(Duration::from_millis(100)).await;

    Ok((addr, handle))
}

#[tokio::test]
async fn test_server_starts_without_auth() {
    let (addr, handle) = start_test_server(false).await.unwrap();

    // Create client and make a simple request
    let mut client = RpcClient::new(addr);

    // network.peers should work without auth in no-auth mode
    // Note: Will return empty peers since no network handle is attached
    let result = client.get_peers().await;

    // Should get an error about missing network handle, not auth error
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.to_lowercase().contains("network") || err_msg.contains("actor not available"),
        "Expected network error, got: {}",
        err_msg
    );

    handle.abort();
}

#[tokio::test]
async fn test_server_starts_with_auth() {
    let (addr, handle) = start_test_server(true).await.unwrap();

    // Create client without credentials
    let mut client = RpcClient::new(addr);

    // Unauthenticated request to protected method should fail
    let result = client.get_peers().await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Authentication required") || err_msg.contains("401"),
        "Expected auth error, got: {}",
        err_msg
    );

    handle.abort();
}

#[tokio::test]
async fn test_auth_challenge_response_flow() {
    let (addr, handle) = start_test_server(true).await.unwrap();

    // Create credentials
    let keypair = Arc::new(KeyPair::generate().unwrap());
    let mut client = RpcClient::with_credentials(addr, keypair);

    // Authenticate
    let auth_result = client.authenticate(vec!["network:read".to_string()]).await;
    assert!(auth_result.is_ok(), "Auth failed: {:?}", auth_result);
    assert!(client.is_authenticated());

    handle.abort();
}

#[tokio::test]
async fn test_auth_invalid_signature_rejected() {
    let (addr, handle) = start_test_server(true).await.unwrap();

    // Manually call auth.challenge
    let keypair = KeyPair::generate().unwrap();
    let did = keypair.did().to_string();

    let client_http = reqwest::Client::new();

    // Get challenge
    let challenge_req = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "auth.challenge",
        "params": { "did": did },
        "id": 1
    });

    let response = client_http
        .post(format!("http://{}", addr))
        .json(&challenge_req)
        .send()
        .await
        .unwrap();

    let challenge_resp: serde_json::Value = response.json().await.unwrap();
    assert!(challenge_resp["result"]["nonce"].is_string());

    // Send invalid signature
    let verify_req = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "auth.verify",
        "params": {
            "did": did,
            "signature": "0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
            "scopes": ["network:read"]
        },
        "id": 2
    });

    let response = client_http
        .post(format!("http://{}", addr))
        .json(&verify_req)
        .send()
        .await
        .unwrap();

    let verify_resp: serde_json::Value = response.json().await.unwrap();
    assert!(verify_resp["error"].is_object(), "Expected error for invalid signature");
    let error_msg = verify_resp["error"]["message"]
        .as_str()
        .unwrap_or("")
        .to_lowercase();
    assert!(
        error_msg.contains("signature")
            || error_msg.contains("verification")
            || error_msg.contains("invalid")
            || error_msg.contains("failed"),
        "Expected signature verification error, got: {}",
        verify_resp["error"]
    );

    handle.abort();
}

#[tokio::test]
async fn test_expired_token_rejected() {
    let (addr, handle) = start_test_server(true).await.unwrap();

    let client_http = reqwest::Client::new();

    // Create a fake expired token (this won't actually be valid JWT)
    let fake_token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJkaWQ6aWNuOnRlc3QiLCJleHAiOjEsInNjb3BlcyI6WyJuZXR3b3JrOnJlYWQiXX0.invalid";

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "network.peers",
        "params": {},
        "id": 1
    });

    let response = client_http
        .post(format!("http://{}", addr))
        .header("Authorization", format!("Bearer {}", fake_token))
        .json(&request)
        .send()
        .await
        .unwrap();

    let resp: serde_json::Value = response.json().await.unwrap();
    assert!(
        resp["error"].is_object(),
        "Expected auth error for invalid token"
    );

    handle.abort();
}

#[tokio::test]
async fn test_rpc_error_format() {
    let (addr, handle) = start_test_server(false).await.unwrap();

    let client_http = reqwest::Client::new();

    // Call non-existent method
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "nonexistent.method",
        "params": {},
        "id": 1
    });

    let response = client_http
        .post(format!("http://{}", addr))
        .json(&request)
        .send()
        .await
        .unwrap();

    let resp: serde_json::Value = response.json().await.unwrap();

    // Verify JSON-RPC error format
    assert!(resp["error"].is_object(), "Expected error object");
    assert!(resp["error"]["code"].is_number(), "Error should have code");
    assert!(resp["error"]["message"].is_string(), "Error should have message");
    assert_eq!(resp["id"], 1, "Response should echo request id");

    handle.abort();
}

#[tokio::test]
async fn test_malformed_json_rejected() {
    let (addr, handle) = start_test_server(false).await.unwrap();

    let client_http = reqwest::Client::new();

    // Send malformed JSON
    let response = client_http
        .post(format!("http://{}", addr))
        .header("Content-Type", "application/json")
        .body("{ invalid json }")
        .send()
        .await
        .unwrap();

    // Should get parse error
    let resp: serde_json::Value = response.json().await.unwrap();
    assert!(resp["error"].is_object());
    // JSON-RPC parse error code is -32700
    assert!(
        resp["error"]["code"].as_i64().unwrap_or(0) == -32700
            || resp["error"]["message"]
                .as_str()
                .unwrap_or("")
                .to_lowercase()
                .contains("parse"),
        "Expected parse error"
    );

    handle.abort();
}

#[tokio::test]
async fn test_auth_methods_always_accessible() {
    let (addr, handle) = start_test_server(true).await.unwrap();

    let client_http = reqwest::Client::new();

    // auth.challenge should work without token even when auth is enabled
    let challenge_req = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "auth.challenge",
        "params": { "did": "did:icn:test" },
        "id": 1
    });

    let response = client_http
        .post(format!("http://{}", addr))
        .json(&challenge_req)
        .send()
        .await
        .unwrap();

    let resp: serde_json::Value = response.json().await.unwrap();
    // Should succeed (get nonce) or fail with "not found" (if DID tracking), but NOT auth error
    let has_result = resp["result"].is_object();
    let has_error = resp["error"].is_object();

    if has_error {
        let error_code = resp["error"]["code"].as_i64().unwrap_or(0);
        // -32001 is auth required, which should NOT happen for auth methods
        assert_ne!(
            error_code, -32001,
            "auth.challenge should not require authentication"
        );
    } else {
        assert!(has_result, "Expected result for auth.challenge");
    }

    handle.abort();
}
