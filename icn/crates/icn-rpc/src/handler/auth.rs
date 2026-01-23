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
///
/// When `coop_id` is provided in params, validates that the DID is a member
/// of that cooperative before issuing the token. The token will include the
/// coop_id claim, enabling coop isolation in subsequent RPC calls.
///
/// SECURITY: Membership validation occurs AFTER signature verification to
/// prevent enumeration attacks. An attacker cannot probe coop memberships
/// without first proving they control the DID's private key.
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
        /// Optional cooperative ID to bind the token to.
        /// If provided, membership will be validated AFTER signature verification.
        #[serde(default)]
        coop_id: Option<String>,
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

    // Pre-check: if coop_id provided but no coop_handle, fail early
    // (This check is safe - doesn't leak membership info, only config state)
    if params.coop_id.is_some() && state.coop_handle().is_none() {
        tracing::warn!("Coop membership validation requested but CoopHandle not configured");
        return RpcResponse::error(
            id,
            crate::error_codes::RESOURCE_NOT_AVAILABLE,
            "Coop membership validation not available".to_string(),
        );
    }

    // Prepare membership validator closure - runs AFTER signature verification
    // This prevents enumeration attacks: can't probe membership without proving identity
    let coop_id_for_validation = params.coop_id.clone();
    let did_for_validation = did.clone();
    let coop_handle = state.coop_handle();

    // Verify challenge and issue token
    // Membership validation happens inside, AFTER signature is verified
    counter!("icn_rpc_auth_verifications_total").increment(1);

    // Use a two-phase approach for async membership validation:
    // 1. First verify the signature (synchronous)
    // 2. Then validate membership if needed (async)
    // 3. Finally issue the token
    //
    // Since verify_challenge_then_validate is synchronous, we validate membership
    // in the closure using blocking approach, or we restructure to do it separately.
    //
    // For async compatibility, we first verify without coop_id, then validate
    // membership, then the token already has the correct coop_id embedded.
    let validated_coop_id = if let Some(ref requested_coop_id) = coop_id_for_validation {
        // We have a coop_id request and coop_handle is available (checked above)
        let coop_handle = coop_handle.unwrap();

        // First verify the signature to prove identity
        match auth_manager.verify_challenge_with_coop(
            &did,
            &signature_bytes,
            params.scopes.clone(),
            None, // Don't embed coop_id yet
        ) {
            Ok(_temp_token) => {
                // Signature verified! Now we can safely check membership.
                // The temp_token is discarded because it lacks the coop_id claim.
                // We'll issue a new token with coop_id after membership validation
                // using issue_token_for_did() which doesn't require a challenge.
            }
            Err(e) => {
                counter!("icn_rpc_auth_failures_total", "reason" => "verification_failed")
                    .increment(1);
                return RpcResponse::error(id, -32401, format!("Authentication failed: {e}"));
            }
        }

        // Now validate membership (identity is proven)
        match coop_handle
            .get_member_coops(did_for_validation.clone())
            .await
        {
            Ok(member_coops) => {
                if member_coops.contains(requested_coop_id) {
                    Some(requested_coop_id.clone())
                } else {
                    tracing::warn!(
                        did = %did_for_validation,
                        requested_coop = %requested_coop_id,
                        "Coop membership validation failed: DID is not a member"
                    );
                    counter!("icn_rpc_auth_failures_total", "reason" => "coop_access_denied")
                        .increment(1);
                    return RpcResponse::error(
                        id,
                        crate::error_codes::COOP_ACCESS_DENIED,
                        "Access denied: you are not a member of this cooperative".to_string(),
                    );
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to validate coop membership");
                return RpcResponse::internal_error(id, e);
            }
        }
    } else {
        None
    };

    // Issue the final token (with coop_id if validated)
    // If no coop_id requested, this is the first verification
    // If coop_id was requested and validated, issue new token with it embedded
    let token_result = if validated_coop_id.is_some() {
        // We already verified above, now issue token with coop_id
        // Note: The challenge was consumed in the first verify call above,
        // so we use issue_token directly via verify_challenge_with_coop
        // which will fail because challenge is gone. We need a different approach.
        //
        // Since the challenge is already consumed, we need to issue the token
        // directly. Let's use the auth_manager's internal token issuance.
        // For now, we'll create a new challenge-verify cycle, but that's not ideal.
        //
        // Better: Add a public issue_token method or handle this differently.
        // For the security fix, let's restructure to not consume challenge early.
        auth_manager.issue_token_for_did(&did, params.scopes.clone(), validated_coop_id.clone())
    } else {
        // No coop_id requested - standard flow
        auth_manager.verify_challenge_with_coop(&did, &signature_bytes, params.scopes, None)
    };

    match token_result {
        Ok(token) => {
            counter!("icn_rpc_auth_successes_total").increment(1);
            let mut response = serde_json::json!({
                "token": token,
                "token_type": "Bearer",
                "expires_in_seconds": 86400
            });
            // Include coop_id in response if present
            if let Some(coop_id) = validated_coop_id {
                response["coop_id"] = serde_json::Value::String(coop_id);
            }
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
