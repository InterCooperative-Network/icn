//! Invite API endpoints
//!
//! RESTful API for managing cooperative invitations.

use actix_web::{get, post, web, HttpRequest, HttpResponse};
use std::sync::Arc;

use crate::commons_mgr::CommonsManager;
use crate::error::Result;
use crate::invite::InviteManager;
use crate::middleware::{get_claims, require_coop_access, require_scope};
use crate::models::{
    CreateInviteRequest, InviteInfo, InviteListResponse, InviteResponse, JoinRequest, JoinResponse,
};
use crate::session_authority::SessionAuthority;
use crate::validation;
use icn_identity::Did;
use icn_obs::metrics::gateway;

// ============================================================================
// Invite Endpoints
// ============================================================================

/// POST /invites - Create a new invite code
#[post("")]
pub async fn create_invite(
    http_req: HttpRequest,
    invite_mgr: web::Data<Arc<InviteManager>>,
    commons_mgr: web::Data<Arc<CommonsManager>>,
    req: web::Json<CreateInviteRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "coop:admin")?;

    // Extract authenticated DID from JWT claims
    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let creator_did: Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    // Validate inputs
    validation::validate_coop_id(&req.coop_id)?;
    // Flat coop-namespace guard (NOT entity/community/federation hierarchy — see
    // #2061): bind the body-supplied coop_id to the caller's token namespace.
    // `coop:admin` is requestable per namespace, so without this a coop A admin could
    // mint invites (which grant membership) for coop B. Prevent a cross-namespace write.
    require_coop_access(&http_req, &req.coop_id)?;
    validation::validate_role(&req.role)?;

    // Default to 7 days if not specified
    let expires_in = req.expires_in_seconds.unwrap_or(7 * 24 * 3600);

    // Validate expiration (max 30 days)
    if expires_in > 30 * 24 * 3600 {
        return Err(crate::error::GatewayError::BadRequest(
            "Expiration cannot exceed 30 days".to_string(),
        ));
    }

    // Create invite
    let invite = invite_mgr
        .create_invite(
            req.coop_id.clone(),
            req.role.clone(),
            creator_did,
            expires_in,
        )
        .await
        .map_err(|e| {
            crate::error::GatewayError::InternalError(format!("Failed to create invite: {e}"))
        })?;

    // Get coop name from charter
    let coop_name = match commons_mgr.get_charter_by_domain(&req.coop_id).await {
        Ok(Some(charter)) => charter.name,
        _ => format!("Coop {}", req.coop_id), // Fallback if charter not found
    };

    // Construct invite URL.
    //
    // NOTE (#2569): this reads `GATEWAY_BASE_URL` for a *different* purpose than
    // `crate::advertised_origin`, and the two meanings do not agree. That module treats the
    // variable as the gateway's own externally reachable API origin — the `{origin}/v1/...`
    // a scanning device posts a bearer credential to — and fails closed without it. Here the
    // same value is used as a *member-facing UI* base, and `/join` is not a gateway route;
    // it exists nowhere in this repository. So under the k8s configmap
    // (`gateway_base_url: http://…:30080`, the gateway NodePort, while the pilot UI is on
    // 30030) this already yields a link to a route the gateway does not serve, and the
    // `localhost:3000` fallback names the compose web-UI port rather than any gateway.
    //
    // That is a pre-existing wrong-destination bug, not the #2569 header-authority class:
    // nothing here is request-derived, so no caller can influence where this points. It is
    // deliberately left alone rather than half-fixed — giving invites their own operator
    // variable is a new config surface across every deployment profile, which belongs in its
    // own change. Do not "unify" these by pointing this at `advertised_origin()`: that would
    // silently repoint invite links at the API origin and hard-fail invite creation on
    // gateways that legitimately issue no QR material.
    let base_url =
        std::env::var("GATEWAY_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
    let invite_url = format!("{}/join?code={}", base_url, invite.code);

    // Record metrics
    gateway::invites_created(&req.coop_id);

    Ok(HttpResponse::Created().json(InviteResponse {
        code: invite.code,
        coop_id: invite.coop_id,
        coop_name,
        role: invite.role,
        expires_at: invite.expires_at,
        invite_url,
    }))
}

/// GET /invites - List all invites for a cooperative
#[get("")]
pub async fn list_invites(
    http_req: HttpRequest,
    invite_mgr: web::Data<Arc<InviteManager>>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "coop:read")?;

    // Get coop_id from query params
    let coop_id = query.get("coop_id").ok_or_else(|| {
        crate::error::GatewayError::BadRequest("Missing coop_id parameter".to_string())
    })?;

    validation::validate_coop_id(coop_id)?;
    // Invite codes are membership-granting bearer secrets; binding the queried
    // coop_id to the caller's token namespace stops a coop A reader from listing (and
    // then redeeming) coop B's invites. Flat coop-namespace guard, not entity
    // hierarchy (#2061). Prevent cross-namespace exposure.
    require_coop_access(&http_req, coop_id)?;

    // List invites
    let invites = invite_mgr.list_invites(coop_id).await.map_err(|e| {
        crate::error::GatewayError::InternalError(format!("Failed to list invites: {e}"))
    })?;

    // Convert to response format
    let invite_infos: Vec<InviteInfo> = invites
        .into_iter()
        .map(|i| InviteInfo {
            code: i.code,
            role: i.role,
            created_by: i.created_by.to_string(),
            created_at: i.created_at,
            expires_at: i.expires_at,
            used: i.used,
        })
        .collect();

    Ok(HttpResponse::Ok().json(InviteListResponse {
        invites: invite_infos,
    }))
}

/// POST /invites/join - Join a cooperative via invite code
#[post("/join")]
pub async fn join_via_invite(
    invite_mgr: web::Data<Arc<InviteManager>>,
    authority: web::Data<Arc<SessionAuthority>>,
    req: web::Json<JoinRequest>,
) -> Result<HttpResponse> {
    // Validate invite code
    let invite = invite_mgr
        .validate_invite(&req.invite_code)
        .await
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid invite: {e}")))?;

    // Validate the provided DID
    let did: Did = req
        .did
        .parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    // Mark invite as used
    invite_mgr
        .mark_used(&req.invite_code, did.clone())
        .await
        .map_err(|e| {
            crate::error::GatewayError::InternalError(format!("Failed to mark invite as used: {e}"))
        })?;

    // Generate capability token
    let scopes = vec![
        "coop:read".to_string(),
        "coop:write".to_string(),
        "ledger:read".to_string(),
        "ledger:transact".to_string(),
    ];

    let token = authority
        .auth_manager()
        .issue_token(&did, &invite.coop_id, scopes)
        .map_err(|e| {
            crate::error::GatewayError::InternalError(format!("Failed to generate token: {e}"))
        })?;

    // Record metrics
    gateway::invites_used(&invite.coop_id);

    Ok(HttpResponse::Ok().json(JoinResponse {
        did: did.to_string(),
        token,
        token_expires_in: authority.lifetime().ttl().as_secs(),
        coop_id: invite.coop_id,
        role: invite.role,
        private_key: String::new(), // Client generates their own keypair
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthManager, TokenClaims};
    use crate::commons_mgr::CommonsManager;
    use crate::invite::InviteManager;
    use actix_web::http::StatusCode;
    use actix_web::{test as actix_test, App, HttpMessage};

    fn admin_claims(coop: &str, sub: &str) -> TokenClaims {
        TokenClaims {
            entity_id: None,
            entity_type: None,
            sub: sub.to_string(),
            iat: 1_000_000_000,
            exp: 9_999_999_999,
            coop_id: coop.to_string(),
            scopes: vec!["coop:admin".to_string(), "coop:read".to_string()],
            jti: None,
        }
    }

    /// `coop:admin` is requestable per coop, so `create_invite` binds the body
    /// coop_id to the token's coop. A same-coop admin mints an invite (201) that is
    /// then listed; a cross-coop admin is rejected (403) and creates nothing under
    /// the target coop. Invites grant membership, so this is a privilege-escalation
    /// boundary. (coop_ids avoid ':' to satisfy validate_coop_id.)
    #[actix_web::test]
    async fn create_invite_rejects_cross_coop_write() {
        async fn run(
            token_coop: &str,
            body_coop: &str,
            sub: &str,
        ) -> (StatusCode, Arc<InviteManager>) {
            let invite_mgr = Arc::new(InviteManager::new());
            let commons_mgr = Arc::new(CommonsManager::new());
            let app = actix_test::init_service(
                App::new()
                    .app_data(web::Data::new(invite_mgr.clone()))
                    .app_data(web::Data::new(commons_mgr))
                    .service(web::scope("/invites").service(create_invite)),
            )
            .await;
            let req = actix_test::TestRequest::post()
                .uri("/invites")
                .set_json(serde_json::json!({ "coop_id": body_coop, "role": "member" }))
                .to_request();
            req.extensions_mut().insert(admin_claims(token_coop, sub));
            let status = actix_test::call_service(&app, req).await.status();
            (status, invite_mgr)
        }

        // The success path parses claims.sub into a Did, so use a real DID subject.
        let sub = icn_identity::KeyPair::generate().unwrap().did().to_string();

        let (same, same_mgr) = run("coopA", "coopA", &sub).await;
        assert_eq!(same, StatusCode::CREATED);
        assert_eq!(
            same_mgr.list_invites("coopA").await.unwrap().len(),
            1,
            "same-coop admin should create exactly one invite"
        );

        let (cross, cross_mgr) = run("coopA", "coopB", &sub).await;
        assert_eq!(cross, StatusCode::FORBIDDEN);
        assert!(
            cross_mgr.list_invites("coopB").await.unwrap().is_empty(),
            "cross-coop reject must not create an invite under coopB"
        );
    }

    /// Invite codes are membership-granting bearer secrets; `list_invites` binds the
    /// queried coop_id to the token's coop so a coopA reader cannot enumerate (and
    /// then redeem) coopB's invites. Same-coop read succeeds (200); cross-coop is 403.
    #[actix_web::test]
    async fn list_invites_rejects_cross_coop_read() {
        async fn run(token_coop: &str, query_coop: &str) -> StatusCode {
            let invite_mgr = Arc::new(InviteManager::new());
            let app = actix_test::init_service(
                App::new()
                    .app_data(web::Data::new(invite_mgr))
                    .service(web::scope("/invites").service(list_invites)),
            )
            .await;
            let req = actix_test::TestRequest::get()
                .uri(&format!("/invites?coop_id={query_coop}"))
                .to_request();
            req.extensions_mut()
                .insert(admin_claims(token_coop, "did:icn:reader"));
            actix_test::call_service(&app, req).await.status()
        }

        assert_eq!(run("coopA", "coopA").await, StatusCode::OK);
        assert_eq!(run("coopA", "coopB").await, StatusCode::FORBIDDEN);
    }

    #[actix_web::test]
    async fn join_reports_the_installed_non_default_token_lifetime() {
        let invite_mgr = Arc::new(InviteManager::new());
        let creator = icn_identity::KeyPair::generate().unwrap().did().clone();
        let invite = invite_mgr
            .create_invite("coopA".to_string(), "member".to_string(), creator, 3600)
            .await
            .unwrap();
        let auth = Arc::new(
            AuthManager::new(b"invite-lifetime-test-secret-32b".to_vec())
                .with_token_ttl(std::time::Duration::from_secs(2 * 3600)),
        );
        let authority = Arc::new(SessionAuthority::evaluator(auth.clone()));
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(invite_mgr))
                .app_data(web::Data::new(authority))
                .service(join_via_invite),
        )
        .await;
        let member = icn_identity::KeyPair::generate().unwrap().did().clone();
        let request = actix_test::TestRequest::post()
            .uri("/join")
            .set_json(JoinRequest {
                invite_code: invite.code,
                did: member.to_string(),
            })
            .to_request();

        let response = actix_test::call_service(&app, request).await;
        assert_eq!(response.status(), StatusCode::OK);
        let response: JoinResponse = actix_test::read_body_json(response).await;

        assert_eq!(response.token_expires_in, 2 * 3600);
        let claims = auth.verify_token(&response.token).unwrap();
        assert_eq!(claims.exp - claims.iat, response.token_expires_in);
    }
}
