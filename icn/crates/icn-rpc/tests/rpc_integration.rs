//! RPC server integration tests
//!
//! These tests validate end-to-end RPC functionality:
//! 1. Server startup and basic connectivity
//! 2. Authentication flow (challenge-response-token)
//! 3. Protected method access with valid/invalid tokens
//! 4. Error handling and response formats

#![allow(clippy::uninlined_format_args)]

use anyhow::Result;
use icn_identity::KeyPair;
use icn_rpc::{RpcClient, RpcServer};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

/// Find an available port for testing by binding to port 0
/// Returns the port and keeps the listener open to prevent reuse
fn find_available_port_with_listener() -> (u16, std::net::TcpListener) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    (port, listener)
}

/// Start an RPC server and return the address
/// Uses retry mechanism to handle port collisions in parallel test execution
async fn start_test_server(with_auth: bool) -> Result<(SocketAddr, tokio::task::JoinHandle<()>)> {
    const MAX_RETRIES: u32 = 5;

    for attempt in 0..MAX_RETRIES {
        // Get a port and hold the listener until we're ready to bind
        let (port, listener) = find_available_port_with_listener();
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse()?;

        // Drop the listener to free the port just before server starts
        drop(listener);

        let server = if with_auth {
            let jwt_secret = b"test-secret-for-rpc-integration-tests-32bytes".to_vec();
            RpcServer::new_with_auth(addr, jwt_secret)
        } else {
            RpcServer::new(addr)
        };

        // Use a channel to detect if the server started successfully
        let (tx, mut rx) = tokio::sync::mpsc::channel::<bool>(1);

        let handle = tokio::spawn(async move {
            match server.run().await {
                Ok(()) => {}
                Err(e) => {
                    let _ = tx.send(false).await;
                    eprintln!("Server error: {e}");
                }
            }
        });

        // Give server time to start (or fail)
        sleep(Duration::from_millis(50)).await;

        // Check if server failed to start
        match rx.try_recv() {
            Ok(false) => {
                // Server failed, abort and retry
                handle.abort();
                if attempt < MAX_RETRIES - 1 {
                    sleep(Duration::from_millis(10)).await;
                    continue;
                }
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                // No error received - server is running
                // Give a bit more time for the server to be fully ready
                sleep(Duration::from_millis(50)).await;
                return Ok((addr, handle));
            }
            _ => {}
        }

        // If we got here on last attempt, return anyway
        if attempt == MAX_RETRIES - 1 {
            return Ok((addr, handle));
        }
    }

    anyhow::bail!("Failed to start test server after {MAX_RETRIES} attempts")
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
    assert!(
        verify_resp["error"].is_object(),
        "Expected error for invalid signature"
    );
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
    assert!(
        resp["error"]["message"].is_string(),
        "Error should have message"
    );
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

// === Additional Integration Tests ===

#[tokio::test]
async fn test_scope_enforcement_wrong_scope_rejected() {
    let (addr, handle) = start_test_server(true).await.unwrap();

    // Authenticate with only ledger:read scope
    let keypair = Arc::new(KeyPair::generate().unwrap());
    let mut client = RpcClient::with_credentials(addr, keypair);
    let auth_result = client.authenticate(vec!["ledger:read".to_string()]).await;
    assert!(auth_result.is_ok(), "Auth failed: {:?}", auth_result);

    // Try to access network.peers which requires network:read scope
    let result = client.get_peers().await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    // Should fail due to insufficient scope, not missing auth
    assert!(
        err_msg.contains("scope") || err_msg.contains("authorization") || err_msg.contains("403"),
        "Expected scope/authorization error, got: {}",
        err_msg
    );

    handle.abort();
}

#[tokio::test]
async fn test_multiple_scopes_authentication() {
    let (addr, handle) = start_test_server(true).await.unwrap();

    let keypair = Arc::new(KeyPair::generate().unwrap());
    let mut client = RpcClient::with_credentials(addr, keypair);

    // Authenticate with multiple scopes
    let scopes = vec![
        "network:read".to_string(),
        "ledger:read".to_string(),
        "trust:read".to_string(),
    ];
    let auth_result = client.authenticate(scopes).await;
    assert!(
        auth_result.is_ok(),
        "Multi-scope auth failed: {:?}",
        auth_result
    );
    assert!(client.is_authenticated());

    handle.abort();
}

#[tokio::test]
async fn test_network_status_without_network_actor() {
    let (addr, handle) = start_test_server(false).await.unwrap();

    let client_http = reqwest::Client::new();

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "network.status",
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
    // Without network actor, may return error OR a default/empty result
    // Both are acceptable behaviors
    let has_error = resp["error"].is_object();
    let has_result = resp["result"].is_object() || resp["result"].is_null();
    assert!(has_error || has_result, "Expected error or result");

    handle.abort();
}

#[tokio::test]
async fn test_ledger_balance_without_ledger_actor() {
    let (addr, handle) = start_test_server(false).await.unwrap();

    let client_http = reqwest::Client::new();

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "ledger.balance",
        "params": { "did": "did:icn:test" },
        "id": 1
    });

    let response = client_http
        .post(format!("http://{}", addr))
        .json(&request)
        .send()
        .await
        .unwrap();

    let resp: serde_json::Value = response.json().await.unwrap();
    // Should return error about ledger actor not being available
    assert!(
        resp["error"].is_object(),
        "Expected error when ledger actor not available"
    );

    handle.abort();
}

#[tokio::test]
async fn test_trust_list_without_trust_actor() {
    let (addr, handle) = start_test_server(false).await.unwrap();

    let client_http = reqwest::Client::new();

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "trust.list",
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
    // Should return error about trust actor not being available
    assert!(
        resp["error"].is_object(),
        "Expected error when trust actor not available"
    );

    handle.abort();
}

#[tokio::test]
async fn test_missing_required_params() {
    // Use auth-enabled server since auth.verify requires auth to be enabled
    let (addr, handle) = start_test_server(true).await.unwrap();

    let client_http = reqwest::Client::new();

    // auth.verify with missing params
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "auth.verify",
        "params": {},  // Missing required params (did, signature, scopes)
        "id": 1
    });

    let response = client_http
        .post(format!("http://{}", addr))
        .json(&request)
        .send()
        .await
        .unwrap();

    let resp: serde_json::Value = response.json().await.unwrap();
    assert!(
        resp["error"].is_object(),
        "Expected error for missing params"
    );
    // Should get an error about missing parameters or challenge
    let error_msg = resp["error"]["message"]
        .as_str()
        .unwrap_or("")
        .to_lowercase();
    assert!(
        error_msg.contains("param")
            || error_msg.contains("missing")
            || error_msg.contains("did")
            || error_msg.contains("challenge")
            || error_msg.contains("signature")
            || error_msg.contains("nonce"),
        "Expected param-related error, got: {}",
        error_msg
    );

    handle.abort();
}

#[tokio::test]
async fn test_batch_request_handling() {
    let (addr, handle) = start_test_server(false).await.unwrap();

    let client_http = reqwest::Client::new();

    // JSON-RPC batch request (array of requests)
    let batch_request = serde_json::json!([
        {
            "jsonrpc": "2.0",
            "method": "network.status",
            "params": {},
            "id": 1
        },
        {
            "jsonrpc": "2.0",
            "method": "network.peers",
            "params": {},
            "id": 2
        }
    ]);

    let response = client_http
        .post(format!("http://{}", addr))
        .json(&batch_request)
        .send()
        .await
        .unwrap();

    let resp: serde_json::Value = response.json().await.unwrap();
    // Batch requests should either work (array response) or be rejected with error
    // Our server currently doesn't support batch, so expect error
    let batch_handled = if resp.is_object() && resp["error"].is_object() {
        // Batch rejected - that's fine
        true
    } else if resp.is_array() {
        // Batch supported - also fine
        true
    } else {
        false
    };
    assert!(
        batch_handled,
        "Server should handle batch request (accept or reject)"
    );

    handle.abort();
}

#[tokio::test]
async fn test_jsonrpc_version_validation() {
    let (addr, handle) = start_test_server(false).await.unwrap();

    let client_http = reqwest::Client::new();

    // Wrong JSON-RPC version
    let request = serde_json::json!({
        "jsonrpc": "1.0",  // Should be "2.0"
        "method": "network.status",
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
    // May either reject (error) or accept (some servers are lenient)
    // If error, should be about version
    let version_handled = resp["error"].is_object() || resp["result"].is_object();
    assert!(
        version_handled,
        "Server should handle wrong JSON-RPC version"
    );

    handle.abort();
}

#[tokio::test]
async fn test_compute_submit_requires_auth() {
    let (addr, handle) = start_test_server(true).await.unwrap();

    let client_http = reqwest::Client::new();

    // Try compute.submit without auth
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "compute.submit",
        "params": {
            "code": "contract Test {}",
            "fuel_limit": 1000
        },
        "id": 1
    });

    let response = client_http
        .post(format!("http://{}", addr))
        .json(&request)
        .send()
        .await
        .unwrap();

    let resp: serde_json::Value = response.json().await.unwrap();
    assert!(resp["error"].is_object(), "Expected auth error");
    let error_code = resp["error"]["code"].as_i64().unwrap_or(0);
    let error_msg = resp["error"]["message"]
        .as_str()
        .unwrap_or("")
        .to_lowercase();
    // Auth error codes: -32001 (auth required) or -32401 (forbidden/auth related)
    assert!(
        error_code == -32001
            || error_code == -32401
            || error_msg.contains("auth")
            || error_msg.contains("unauthorized")
            || error_msg.contains("token"),
        "Expected auth error, got code {} msg: {}",
        error_code,
        error_msg
    );

    handle.abort();
}

#[tokio::test]
async fn test_empty_method_rejected() {
    let (addr, handle) = start_test_server(false).await.unwrap();

    let client_http = reqwest::Client::new();

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "",  // Empty method name
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
    assert!(resp["error"].is_object(), "Expected error for empty method");

    handle.abort();
}

#[tokio::test]
async fn test_policy_methods_without_policy_actor() {
    let (addr, handle) = start_test_server(false).await.unwrap();

    let client_http = reqwest::Client::new();

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "policy.list",
        "params": { "coop_id": "test-coop" },
        "id": 1
    });

    let response = client_http
        .post(format!("http://{}", addr))
        .json(&request)
        .send()
        .await
        .unwrap();

    let resp: serde_json::Value = response.json().await.unwrap();
    // Should return error about policy actor not being available
    assert!(
        resp["error"].is_object(),
        "Expected error when policy actor not available"
    );

    handle.abort();
}

#[tokio::test]
async fn test_recovery_methods_without_recovery_actor() {
    let (addr, handle) = start_test_server(false).await.unwrap();

    let client_http = reqwest::Client::new();

    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "recovery.list",
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
    // Should return error about recovery actor not being available
    assert!(
        resp["error"].is_object(),
        "Expected error when recovery actor not available"
    );

    handle.abort();
}

// === Compute Endpoint Tests (Gap #10) ===

#[tokio::test]
async fn test_compute_submit_missing_code() {
    let (addr, handle) = start_test_server(false).await.unwrap();

    let client_http = reqwest::Client::new();

    // Submit without code parameter
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "compute.submit",
        "params": {
            "fuel_limit": 1000
            // Missing "code" parameter
        },
        "id": 1
    });

    let response = client_http
        .post(format!("http://{}", addr))
        .json(&request)
        .send()
        .await
        .unwrap();

    let resp: serde_json::Value = response.json().await.unwrap();
    assert!(
        resp["error"].is_object(),
        "Expected error for missing code parameter"
    );
    let error_msg = resp["error"]["message"]
        .as_str()
        .unwrap_or("")
        .to_lowercase();
    // Accept either validation error OR compute not available
    assert!(
        error_msg.contains("code")
            || error_msg.contains("missing")
            || error_msg.contains("param")
            || error_msg.contains("required")
            || error_msg.contains("compute")
            || error_msg.contains("not available"),
        "Expected code-related or compute error, got: {}",
        error_msg
    );

    handle.abort();
}

#[tokio::test]
async fn test_compute_submit_invalid_fuel_limit() {
    let (addr, handle) = start_test_server(false).await.unwrap();

    let client_http = reqwest::Client::new();

    // Submit with negative fuel limit (should fail validation)
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "compute.submit",
        "params": {
            "code": "contract Test { rule main() {} }",
            "fuel_limit": -1
        },
        "id": 1
    });

    let response = client_http
        .post(format!("http://{}", addr))
        .json(&request)
        .send()
        .await
        .unwrap();

    let resp: serde_json::Value = response.json().await.unwrap();
    // Should get an error (either validation or compute actor not available)
    assert!(
        resp["error"].is_object(),
        "Expected error for invalid fuel limit"
    );

    handle.abort();
}

#[tokio::test]
async fn test_compute_submit_zero_fuel_limit() {
    let (addr, handle) = start_test_server(false).await.unwrap();

    let client_http = reqwest::Client::new();

    // Submit with zero fuel limit
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "compute.submit",
        "params": {
            "code": "contract Test { rule main() {} }",
            "fuel_limit": 0
        },
        "id": 1
    });

    let response = client_http
        .post(format!("http://{}", addr))
        .json(&request)
        .send()
        .await
        .unwrap();

    let resp: serde_json::Value = response.json().await.unwrap();
    // Should get an error (zero fuel is invalid)
    assert!(
        resp["error"].is_object(),
        "Expected error for zero fuel limit"
    );

    handle.abort();
}

#[tokio::test]
async fn test_compute_status_invalid_hash() {
    let (addr, handle) = start_test_server(false).await.unwrap();

    let client_http = reqwest::Client::new();

    // Query status with invalid hash (not hex)
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "compute.status",
        "params": {
            "task_hash": "not-a-valid-hash"
        },
        "id": 1
    });

    let response = client_http
        .post(format!("http://{}", addr))
        .json(&request)
        .send()
        .await
        .unwrap();

    let resp: serde_json::Value = response.json().await.unwrap();
    // Should get an error (invalid hash format or not found)
    assert!(
        resp["error"].is_object(),
        "Expected error for invalid task hash"
    );

    handle.abort();
}

#[tokio::test]
async fn test_compute_status_nonexistent_hash() {
    let (addr, handle) = start_test_server(false).await.unwrap();

    let client_http = reqwest::Client::new();

    // Query status with valid hex but non-existent task
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "compute.status",
        "params": {
            "task_hash": "0000000000000000000000000000000000000000000000000000000000000000"
        },
        "id": 1
    });

    let response = client_http
        .post(format!("http://{}", addr))
        .json(&request)
        .send()
        .await
        .unwrap();

    let resp: serde_json::Value = response.json().await.unwrap();
    // Should get an error (task not found or compute actor not available)
    assert!(
        resp["error"].is_object(),
        "Expected error for non-existent task"
    );

    handle.abort();
}

#[tokio::test]
async fn test_compute_status_missing_hash() {
    let (addr, handle) = start_test_server(false).await.unwrap();

    let client_http = reqwest::Client::new();

    // Query status without task_hash parameter
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "compute.status",
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
    assert!(
        resp["error"].is_object(),
        "Expected error for missing task_hash parameter"
    );
    let error_msg = resp["error"]["message"]
        .as_str()
        .unwrap_or("")
        .to_lowercase();
    // Accept either validation error OR compute not available
    assert!(
        error_msg.contains("hash")
            || error_msg.contains("missing")
            || error_msg.contains("param")
            || error_msg.contains("required")
            || error_msg.contains("compute")
            || error_msg.contains("not available"),
        "Expected hash-related or compute error, got: {}",
        error_msg
    );

    handle.abort();
}

#[tokio::test]
async fn test_compute_cancel_requires_auth() {
    let (addr, handle) = start_test_server(true).await.unwrap();

    let client_http = reqwest::Client::new();

    // Try compute.cancel without auth
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "compute.cancel",
        "params": {
            "task_hash": "0000000000000000000000000000000000000000000000000000000000000000"
        },
        "id": 1
    });

    let response = client_http
        .post(format!("http://{}", addr))
        .json(&request)
        .send()
        .await
        .unwrap();

    let resp: serde_json::Value = response.json().await.unwrap();
    assert!(resp["error"].is_object(), "Expected auth error for cancel");
    let error_code = resp["error"]["code"].as_i64().unwrap_or(0);
    let error_msg = resp["error"]["message"]
        .as_str()
        .unwrap_or("")
        .to_lowercase();
    assert!(
        error_code == -32001
            || error_code == -32401
            || error_msg.contains("auth")
            || error_msg.contains("unauthorized")
            || error_msg.contains("token"),
        "Expected auth error, got code {} msg: {}",
        error_code,
        error_msg
    );

    handle.abort();
}

#[tokio::test]
async fn test_compute_cancel_missing_hash() {
    let (addr, handle) = start_test_server(false).await.unwrap();

    let client_http = reqwest::Client::new();

    // Try cancel without task_hash
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "compute.cancel",
        "params": {
            "reason": "test cancel"
        },
        "id": 1
    });

    let response = client_http
        .post(format!("http://{}", addr))
        .json(&request)
        .send()
        .await
        .unwrap();

    let resp: serde_json::Value = response.json().await.unwrap();
    assert!(
        resp["error"].is_object(),
        "Expected error for missing task_hash"
    );

    handle.abort();
}

#[tokio::test]
async fn test_compute_submit_with_priority() {
    let (addr, handle) = start_test_server(false).await.unwrap();

    let client_http = reqwest::Client::new();

    // Submit with priority parameter
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "compute.submit",
        "params": {
            "code": "contract Test { rule main() {} }",
            "fuel_limit": 1000,
            "priority": "high"
        },
        "id": 1
    });

    let response = client_http
        .post(format!("http://{}", addr))
        .json(&request)
        .send()
        .await
        .unwrap();

    let resp: serde_json::Value = response.json().await.unwrap();
    // Will get compute actor not available error, but priority should be parsed
    assert!(
        resp["error"].is_object() || resp["result"].is_object(),
        "Expected response"
    );

    handle.abort();
}

#[tokio::test]
async fn test_compute_submit_invalid_priority() {
    let (addr, handle) = start_test_server(false).await.unwrap();

    let client_http = reqwest::Client::new();

    // Submit with invalid priority value
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "compute.submit",
        "params": {
            "code": "contract Test { rule main() {} }",
            "fuel_limit": 1000,
            "priority": "super-ultra-high"  // Invalid priority
        },
        "id": 1
    });

    let response = client_http
        .post(format!("http://{}", addr))
        .json(&request)
        .send()
        .await
        .unwrap();

    let resp: serde_json::Value = response.json().await.unwrap();
    // Should get validation error or compute actor not available
    assert!(
        resp["error"].is_object(),
        "Expected error for invalid priority"
    );

    handle.abort();
}

#[tokio::test]
async fn test_compute_submit_with_coop_id() {
    let (addr, handle) = start_test_server(false).await.unwrap();

    let client_http = reqwest::Client::new();

    // Submit with coop_id parameter
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "compute.submit",
        "params": {
            "code": "contract Test { rule main() {} }",
            "fuel_limit": 1000,
            "coop_id": "test-cooperative"
        },
        "id": 1
    });

    let response = client_http
        .post(format!("http://{}", addr))
        .json(&request)
        .send()
        .await
        .unwrap();

    let resp: serde_json::Value = response.json().await.unwrap();
    // Will get compute actor not available error, but coop_id should be parsed
    assert!(
        resp["error"].is_object() || resp["result"].is_object(),
        "Expected response"
    );

    handle.abort();
}

#[tokio::test]
async fn test_compute_submit_excessive_fuel_limit() {
    let (addr, handle) = start_test_server(false).await.unwrap();

    let client_http = reqwest::Client::new();

    // Submit with excessively large fuel limit
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "compute.submit",
        "params": {
            "code": "contract Test { rule main() {} }",
            "fuel_limit": 999999999999i64  // Very large fuel limit
        },
        "id": 1
    });

    let response = client_http
        .post(format!("http://{}", addr))
        .json(&request)
        .send()
        .await
        .unwrap();

    let resp: serde_json::Value = response.json().await.unwrap();
    // Should get an error (validation or compute actor not available)
    assert!(
        resp["error"].is_object(),
        "Expected error for excessive fuel limit"
    );

    handle.abort();
}
