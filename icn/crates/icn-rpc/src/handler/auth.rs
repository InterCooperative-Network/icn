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

/// Handle auth.revoke RPC call - revoke a JWT token
///
/// This endpoint allows users to revoke their own tokens or administrators
/// to revoke any token. Revoked tokens will be rejected on future verification.
pub async fn handle_auth_revoke(
    id: u64,
    params: &serde_json::Value,
    state: &Arc<RpcServer>,
    caller_claims: Option<&crate::auth::RpcTokenClaims>,
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

    // Check if revocation is supported
    if !auth_manager.has_revocation_support() {
        return RpcResponse::error(
            id,
            -32000,
            "Token revocation not available (no persistent storage configured)".to_string(),
        );
    }

    #[derive(serde::Deserialize)]
    struct RevokeParams {
        /// The token to revoke
        token: String,
        /// Optional reason for revocation
        #[serde(default)]
        reason: Option<String>,
    }

    let params: RevokeParams = match serde_json::from_value(params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid params: {e}"));
        }
    };

    // Parse the token to get claims (without expiration validation to allow revoking expired tokens)
    let claims = match auth_manager.parse_token_claims(&params.token) {
        Ok(c) => c,
        Err(e) => {
            return RpcResponse::error(id, -32602, format!("Invalid token: {e}"));
        }
    };

    // Check if token was already revoked (idempotent operation)
    if auth_manager.is_token_revoked(&claims.jti) {
        return RpcResponse::success(
            id,
            serde_json::json!({
                "revoked": true,
                "message": "Token was already revoked"
            }),
        );
    }

    // Authorization check: only the token owner or an admin can revoke
    // If caller is not authenticated, they can only revoke if they have the token itself
    if let Some(caller) = caller_claims {
        // Check if caller is either the token owner or has admin scope
        if caller.sub != claims.sub && !caller.has_scope("admin") {
            counter!("icn_rpc_auth_revoke_denied_total").increment(1);
            return RpcResponse::error(
                id,
                -32403,
                "Not authorized to revoke this token".to_string(),
            );
        }
    }
    // If no caller claims, we allow revocation since they have the token
    // (you can always revoke a token you possess)

    // Revoke the token
    match auth_manager.revoke_token(&claims, params.reason) {
        Ok(()) => {
            counter!("icn_rpc_auth_revocations_total").increment(1);
            tracing::info!(
                jti = %claims.jti,
                subject = %claims.sub,
                "Token revoked via RPC"
            );
            RpcResponse::success(
                id,
                serde_json::json!({
                    "revoked": true,
                    "jti": claims.jti
                }),
            )
        }
        Err(e) => RpcResponse::error(id, -32000, format!("Failed to revoke token: {e}")),
    }
}
