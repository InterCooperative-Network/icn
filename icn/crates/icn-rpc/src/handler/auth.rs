//! Authentication-related RPC handlers

use std::sync::Arc;

use icn_identity::Did;
use metrics::counter;

use crate::server::RpcServer;
use crate::types::RpcResponse;

/// Handle auth.challenge RPC call - get a challenge nonce for DID authentication
pub async fn handle_auth_challenge(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let auth_manager = match state.auth_manager() {
        Some(am) => am,
        None => {
            return RpcResponse::error(
                id,
                -32000,
                "Authentication not enabled on this server".to_string(),
            );
        }
    };

    #[derive(serde::Deserialize)]
    struct ChallengeParams {
        did: String,
    }

    let params: ChallengeParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse DID
    let did = match Did::from_str(&params.did) {
        Ok(d) => d,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid DID format: {e}"));
        }
    };

    // Create challenge
    match auth_manager.create_challenge(&did) {
        Ok(nonce) => {
            counter!("icn_rpc_auth_challenges_total").increment(1);
            let response = serde_json::json!({
                "nonce": nonce,
                "expires_in_seconds": 300
            });
            RpcResponse::success(id, response)
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to create challenge: {e}")),
    }
}

/// Handle auth.verify RPC call - verify signed challenge and get JWT token
pub async fn handle_auth_verify(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
) -> RpcResponse {
    let auth_manager = match state.auth_manager() {
        Some(am) => am,
        None => {
            return RpcResponse::error(
                id,
                -32000,
                "Authentication not enabled on this server".to_string(),
            );
        }
    };

    #[derive(serde::Deserialize)]
    struct VerifyParams {
        did: String,
        signature: String, // hex-encoded
        scopes: Vec<String>,
    }

    let params: VerifyParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse DID
    let did = match Did::from_str(&params.did) {
        Ok(d) => d,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid DID format: {e}"));
        }
    };

    // Parse signature
    let signature_bytes = match hex::decode(&params.signature) {
        Ok(b) => b,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid signature encoding: {e}"));
        }
    };

    // Verify challenge and issue token
    counter!("icn_rpc_auth_verifications_total").increment(1);
    match auth_manager.verify_challenge(&did, &signature_bytes, params.scopes) {
        Ok(token) => {
            counter!("icn_rpc_auth_successes_total").increment(1);
            let response = serde_json::json!({
                "token": token,
                "token_type": "Bearer",
                "expires_in_seconds": 86400
            });
            RpcResponse::success(id, response)
        }
        Err(e) => {
            counter!("icn_rpc_auth_failures_total", "reason" => "verification_failed").increment(1);
            RpcResponse::error(id, -32401, format!("Authentication failed: {e}"))
        }
    }
}
