//! Cooperative namespace API endpoints

use actix_web::{delete, get, post, put, web, HttpRequest, HttpResponse};
use std::sync::Arc;

use crate::coop::{CoopManager, MemberRole};
use crate::error::Result;
use crate::events::{EventBroadcaster, GatewayEvent};
use crate::middleware::{require_coop_access, require_scope};
use crate::models::{
    AddMemberRequest, CreateCoopRequest, UpdateRoleRequest, UpdateSettingsRequest,
};
use crate::validation;
use icn_obs::metrics::gateway;

fn timestamp() -> u64 {
    icn_time::current_timestamp_secs()
}

fn parse_role(role_str: &str) -> Result<MemberRole> {
    match role_str.to_lowercase().as_str() {
        // New cooperative role names (preferred)
        "steward" => Ok(MemberRole::Steward),
        "facilitator" => Ok(MemberRole::Facilitator),
        "participant" => Ok(MemberRole::Participant),
        // Legacy role names (for backwards compatibility)
        "owner" => Ok(MemberRole::Steward),
        "admin" => Ok(MemberRole::Facilitator),
        "member" => Ok(MemberRole::Participant),
        _ => Err(crate::error::GatewayError::BadRequest(format!(
            "Invalid role: {role_str}. Valid roles: steward, facilitator, participant"
        ))),
    }
}

/// POST /coops - Create a new cooperative
#[post("")]
pub async fn create_coop(
    http_req: HttpRequest,
    coop_mgr: web::Data<Arc<CoopManager>>,
    req: web::Json<CreateCoopRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "coop:write")?;

    // Extract owner DID from authenticated token
    use crate::middleware::get_claims;
    let claims = get_claims(&http_req).ok_or_else(|| {
        crate::error::GatewayError::AuthenticationFailed("No claims found".to_string())
    })?;

    let owner: icn_identity::Did = claims.sub.parse().map_err(|e| {
        crate::error::GatewayError::BadRequest(format!("Invalid DID in token: {e}"))
    })?;

    // Validate inputs
    validation::validate_coop_id(&req.id)?;
    validation::validate_coop_name(&req.name)?;

    // Note: Global cooperative limit is checked atomically inside create_coop()
    // to prevent TOCTOU race condition
    coop_mgr
        .create_coop(req.id.clone(), req.name.clone(), owner, timestamp())
        .await?;

    // Track cooperative creation
    gateway::coops_created_inc();

    let coop = coop_mgr.get_coop(&req.id).await?;
    Ok(HttpResponse::Created().json(coop))
}

/// GET /coops/:id - Get cooperative info
#[get("/{id}")]
pub async fn get_coop(
    req: HttpRequest,
    coop_mgr: web::Data<Arc<CoopManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&req, "coop:read")?;
    require_coop_access(&req, &id)?; // CRITICAL: Prevent cross-coop privacy leaks

    let coop = coop_mgr.get_coop(&id).await?;
    Ok(HttpResponse::Ok().json(coop))
}

/// PUT /coops/:id/settings - Update cooperative settings
#[put("/{id}/settings")]
pub async fn update_settings(
    http_req: HttpRequest,
    coop_mgr: web::Data<Arc<CoopManager>>,
    broadcaster: web::Data<Arc<EventBroadcaster>>,
    id: web::Path<String>,
    req: web::Json<UpdateSettingsRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "coop:admin")?;
    require_coop_access(&http_req, &id)?; // CRITICAL: Prevent cross-coop attacks

    // Validate settings fields upfront
    if let Some(gov) = &req.governance_model {
        validation::validate_governance_model(gov)?;
    }
    if let Some(policy) = &req.credit_policy {
        validation::validate_credit_policy(policy)?;
    }
    if let Some(currency) = &req.currency {
        validation::validate_currency(currency)?;
    }

    // Clone the request data for the closure
    let req_data = req.into_inner();

    // Use atomic operation to prevent race conditions
    let coop = coop_mgr.update_settings_atomic(&id, |coop| {
        if let Some(gov) = &req_data.governance_model {
            coop.settings.governance_model = gov.clone();
        }
        if let Some(policy) = &req_data.credit_policy {
            coop.settings.credit_policy = policy.clone();
        }
        if let Some(currency) = &req_data.currency {
            coop.settings.currency = currency.clone();
        }
        Ok(())
    })?;

    // Broadcast settings updated event
    let event = GatewayEvent::SettingsUpdated {
        coop_id: id.to_string(),
    };
    let broadcaster_clone = broadcaster.clone();
    let coop_id = id.into_inner();
    tokio::spawn(async move {
        broadcaster_clone.broadcast(&coop_id, event).await;
    });

    Ok(HttpResponse::Ok().json(coop))
}

/// DELETE /coops/:id - Delete cooperative
#[delete("/{id}")]
pub async fn delete_coop(
    req: HttpRequest,
    coop_mgr: web::Data<Arc<CoopManager>>,
    broadcaster: web::Data<Arc<EventBroadcaster>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&req, "coop:admin")?;

    let coop_id = id.into_inner();

    // CRITICAL: Verify token is for this cooperative (prevent cross-coop deletion)
    require_coop_access(&req, &coop_id)?;

    coop_mgr.delete_coop(&coop_id)?;

    // Track cooperative deletion
    gateway::coops_deleted_inc();

    // Clean up any WebSocket subscribers for this deleted cooperative
    let broadcaster_clone = broadcaster.clone();
    let coop_id_clone = coop_id.clone();
    tokio::spawn(async move {
        broadcaster_clone.cleanup(&coop_id_clone).await;
    });

    Ok(HttpResponse::NoContent().finish())
}

/// POST /coops/:id/members - Add a member to cooperative
#[post("/{id}/members")]
pub async fn add_member(
    http_req: HttpRequest,
    coop_mgr: web::Data<Arc<CoopManager>>,
    broadcaster: web::Data<Arc<EventBroadcaster>>,
    id: web::Path<String>,
    req: web::Json<AddMemberRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "coop:admin")?;
    require_coop_access(&http_req, &id)?; // CRITICAL: Prevent cross-coop attacks

    // Parse and validate inputs first
    let did: icn_identity::Did = req
        .did
        .parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let role = parse_role(&req.role)?;

    // Note: Member count limit is checked atomically inside add_member()
    // to prevent TOCTOU race condition
    let coop = coop_mgr.add_member_atomic(&id, did.clone(), role.clone(), timestamp())?;

    // Track member addition
    gateway::members_added_inc();

    // Broadcast member added event
    let event = GatewayEvent::MemberAdded {
        coop_id: id.to_string(),
        did: did.to_string(),
        role: format!("{role:?}"),
    };
    let broadcaster_clone = broadcaster.clone();
    let coop_id = id.into_inner();
    tokio::spawn(async move {
        broadcaster_clone.broadcast(&coop_id, event).await;
    });

    Ok(HttpResponse::Ok().json(coop))
}

/// DELETE /coops/:id/members/:did - Remove a member from cooperative
#[delete("/{id}/members/{did}")]
pub async fn remove_member(
    req: HttpRequest,
    coop_mgr: web::Data<Arc<CoopManager>>,
    broadcaster: web::Data<Arc<EventBroadcaster>>,
    path: web::Path<(String, String)>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&req, "coop:admin")?;

    let (coop_id, did_str) = path.into_inner();
    require_coop_access(&req, &coop_id)?; // CRITICAL: Prevent cross-coop attacks

    let did = did_str
        .parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    // Use atomic operation to prevent race conditions
    let coop = coop_mgr.remove_member_atomic(&coop_id, &did)?;

    // Track member removal
    gateway::members_removed_inc();

    // Broadcast member removed event
    let event = GatewayEvent::MemberRemoved {
        coop_id: coop_id.clone(),
        did: did_str.clone(),
    };
    let broadcaster_clone = broadcaster.clone();
    let coop_id_clone = coop_id.clone();
    tokio::spawn(async move {
        broadcaster_clone.broadcast(&coop_id_clone, event).await;
    });

    Ok(HttpResponse::Ok().json(coop))
}

/// PUT /coops/:id/members/:did/role - Update member role
#[put("/{id}/members/{did}/role")]
pub async fn update_member_role(
    http_req: HttpRequest,
    coop_mgr: web::Data<Arc<CoopManager>>,
    broadcaster: web::Data<Arc<EventBroadcaster>>,
    path: web::Path<(String, String)>,
    req: web::Json<UpdateRoleRequest>,
) -> Result<HttpResponse> {
    // Check authorization
    require_scope(&http_req, "coop:admin")?;

    let (coop_id, did_str) = path.into_inner();
    require_coop_access(&http_req, &coop_id)?; // CRITICAL: Prevent cross-coop attacks

    let did = did_str
        .parse()
        .map_err(|e| crate::error::GatewayError::BadRequest(format!("Invalid DID: {e}")))?;

    let new_role = parse_role(&req.role)?;

    // Use atomic operation to prevent race conditions
    let coop = coop_mgr.update_role_atomic(&coop_id, &did, new_role.clone())?;

    // Broadcast role updated event
    let event = GatewayEvent::RoleUpdated {
        coop_id: coop_id.clone(),
        did: did_str.clone(),
        new_role: format!("{new_role:?}"),
    };
    let broadcaster_clone = broadcaster.clone();
    let coop_id_clone = coop_id.clone();
    tokio::spawn(async move {
        broadcaster_clone.broadcast(&coop_id_clone, event).await;
    });

    Ok(HttpResponse::Ok().json(coop))
}

/// Public statistics response for a cooperative
#[derive(serde::Serialize)]
pub struct CoopStatsResponse {
    /// Cooperative ID
    pub coop_id: String,

    /// Cooperative name
    pub name: String,

    /// Total number of members
    pub total_members: usize,

    /// Total hours exchanged (absolute sum of all transactions)
    pub total_hours_exchanged: f64,

    /// Number of transactions
    pub transaction_count: usize,

    /// Average transaction size
    pub avg_transaction_size: f64,

    /// When the cooperative was created (unix timestamp)
    pub created_at: u64,
}

/// GET /coops/:id/stats - Get public statistics for a cooperative
///
/// This endpoint is PUBLIC (no authentication required) and returns
/// aggregate statistics that are safe to share publicly.
#[get("/coops/{id}/stats")]
pub async fn get_coop_stats(
    coop_mgr: web::Data<Arc<CoopManager>>,
    ledger_mgr: web::Data<Arc<crate::ledger_mgr::LedgerManager>>,
    id: web::Path<String>,
) -> Result<HttpResponse> {
    let coop_id = id.into_inner();

    // Get cooperative info
    let coop = coop_mgr.get_coop(&coop_id).await?;

    // Get transaction statistics from ledger
    let (transaction_count, total_hours_exchanged) = ledger_mgr
        .get_transaction_stats(&coop_id)
        .unwrap_or((0, 0.0));

    let avg_transaction_size = if transaction_count > 0 {
        total_hours_exchanged / transaction_count as f64
    } else {
        0.0
    };

    let stats = CoopStatsResponse {
        coop_id: coop.id.clone(),
        name: coop.name.clone(),
        total_members: coop.members.len(),
        total_hours_exchanged,
        transaction_count,
        avg_transaction_size,
        created_at: coop.created_at,
    };

    // Track stats access
    gateway::stats_accessed_inc();

    Ok(HttpResponse::Ok().json(stats))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::TokenClaims;
    use actix_web::{test, App, HttpMessage};
    use icn_identity::IdentityBundle;

    #[actix_web::test]
    async fn test_create_and_get_coop() {
        let coop_mgr = Arc::new(CoopManager::new());
        let alice = IdentityBundle::generate().unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(coop_mgr.clone()))
                .service(web::scope("/coops").service(create_coop).service(get_coop)),
        )
        .await;

        // Create coop with authorization
        let req_body = CreateCoopRequest {
            id: "test-coop".to_string(),
            name: "Test Cooperative".to_string(),
        };

        let claims = TokenClaims {
            sub: alice.did().to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["coop:write".to_string()],
            exp: 9999999999,
        };

        let req = test::TestRequest::post()
            .uri("/coops")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Get coop with authorization
        let claims = TokenClaims {
            sub: alice.did().to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["coop:read".to_string()],
            exp: 9999999999,
        };

        let req = test::TestRequest::get()
            .uri("/coops/test-coop")
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_add_remove_member() {
        let coop_mgr = Arc::new(CoopManager::new());
        let broadcaster = Arc::new(EventBroadcaster::new());
        let owner = IdentityBundle::generate().unwrap();
        let member = IdentityBundle::generate().unwrap();

        // Create coop directly
        coop_mgr
            .create_coop(
                "test-coop".to_string(),
                "Test".to_string(),
                owner.did().clone(),
                timestamp(),
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(coop_mgr.clone()))
                .app_data(web::Data::new(broadcaster.clone()))
                .service(
                    web::scope("/coops")
                        .service(add_member)
                        .service(remove_member),
                ),
        )
        .await;

        // Add member with authorization
        let req_body = AddMemberRequest {
            did: member.did().to_string(),
            role: "member".to_string(),
        };

        let claims = TokenClaims {
            sub: owner.did().to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["coop:admin".to_string()],
            exp: 9999999999,
        };

        let req = test::TestRequest::post()
            .uri("/coops/test-coop/members")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims.clone());

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Remove member with authorization
        let uri = format!("/coops/test-coop/members/{}", member.did());
        let req = test::TestRequest::delete().uri(&uri).to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_authorization_scope_check() {
        let coop_mgr = Arc::new(CoopManager::new());
        let broadcaster = Arc::new(EventBroadcaster::new());
        let owner = IdentityBundle::generate().unwrap();
        let member = IdentityBundle::generate().unwrap();

        // Create coop directly
        coop_mgr
            .create_coop(
                "test-coop".to_string(),
                "Test".to_string(),
                owner.did().clone(),
                timestamp(),
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(coop_mgr.clone()))
                .app_data(web::Data::new(broadcaster.clone()))
                .service(
                    web::scope("/coops")
                        .service(add_member)
                        .service(update_settings),
                ),
        )
        .await;

        // Try to add member with only "coop:read" scope (should fail)
        let req_body = AddMemberRequest {
            did: member.did().to_string(),
            role: "member".to_string(),
        };

        let claims = TokenClaims {
            sub: owner.did().to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["coop:read".to_string()], // Wrong scope!
            exp: 9999999999,
        };

        let req = test::TestRequest::post()
            .uri("/coops/test-coop/members")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);

        // Try to update settings with "coop:read" scope (should fail)
        let req_body = UpdateSettingsRequest {
            governance_model: Some("consensus".to_string()),
            credit_policy: None,
            currency: None,
        };

        let claims = TokenClaims {
            sub: owner.did().to_string(),
            iat: 1000000000,
            coop_id: "test-coop".to_string(),
            scopes: vec!["coop:read".to_string()], // Wrong scope!
            exp: 9999999999,
        };

        let req = test::TestRequest::put()
            .uri("/coops/test-coop/settings")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);
    }

    #[actix_web::test]
    async fn test_create_coop_uses_authenticated_did() {
        let coop_mgr = Arc::new(CoopManager::new());
        let alice = IdentityBundle::generate().unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(coop_mgr.clone()))
                .service(web::scope("/coops").service(create_coop)),
        )
        .await;

        // Create coop with Alice's token
        let req_body = CreateCoopRequest {
            id: "alice-coop".to_string(),
            name: "Alice's Cooperative".to_string(),
        };

        let claims = TokenClaims {
            sub: alice.did().to_string(),
            iat: 1000000000,
            coop_id: "alice-coop".to_string(),
            scopes: vec!["coop:write".to_string()],
            exp: 9999999999,
        };

        let req = test::TestRequest::post()
            .uri("/coops")
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Verify that Alice is the steward
        let coop = coop_mgr.get_coop(&"alice-coop".to_string()).await.unwrap();
        assert_eq!(
            coop.members.len(),
            1,
            "Coop should have exactly one member (the steward)"
        );

        let alice_member = coop
            .members
            .iter()
            .find(|m| m.did == *alice.did())
            .expect("Alice should be a member");
        assert_eq!(
            alice_member.role,
            MemberRole::Steward,
            "Alice should be the steward"
        );
    }

    #[actix_web::test]
    async fn test_cross_cooperative_authorization_fails() {
        let coop_mgr = Arc::new(CoopManager::new());
        let broadcaster = Arc::new(EventBroadcaster::new());
        let alice = IdentityBundle::generate().unwrap();
        let bob = IdentityBundle::generate().unwrap();

        // Create two cooperatives
        coop_mgr
            .create_coop(
                "coop-food".to_string(),
                "Food Coop".to_string(),
                alice.did().clone(),
                timestamp(),
            )
            .await
            .unwrap();

        coop_mgr
            .create_coop(
                "coop-tech".to_string(),
                "Tech Coop".to_string(),
                bob.did().clone(),
                timestamp(),
            )
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(coop_mgr.clone()))
                .app_data(web::Data::new(broadcaster.clone()))
                .service(
                    web::scope("/coops")
                        .service(get_coop)
                        .service(update_settings)
                        .service(add_member)
                        .service(delete_coop),
                ),
        )
        .await;

        // Alice tries to read coop-tech info using her coop-food token (should fail)
        let claims = TokenClaims {
            sub: alice.did().to_string(),
            iat: 1000000000,
            coop_id: "coop-food".to_string(),
            scopes: vec!["coop:read".to_string()],
            exp: 9999999999,
        };

        let req = test::TestRequest::get()
            .uri("/coops/coop-tech")
            .to_request();
        req.extensions_mut().insert(claims.clone());

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);

        // Alice tries to modify coop-tech using her coop-food token (should fail)
        let req_body = UpdateSettingsRequest {
            governance_model: Some("malicious".to_string()),
            credit_policy: None,
            currency: None,
        };

        let claims = TokenClaims {
            sub: alice.did().to_string(),
            iat: 1000000000,
            coop_id: "coop-food".to_string(), // Alice's token is for coop-food
            scopes: vec!["coop:admin".to_string()],
            exp: 9999999999,
        };

        let req = test::TestRequest::put()
            .uri("/coops/coop-tech/settings") // But trying to modify coop-tech!
            .set_json(&req_body)
            .to_request();
        req.extensions_mut().insert(claims.clone());

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);

        // Also test add_member cross-coop attack
        let add_req = AddMemberRequest {
            did: alice.did().to_string(),
            role: "admin".to_string(),
        };

        let req = test::TestRequest::post()
            .uri("/coops/coop-tech/members")
            .set_json(&add_req)
            .to_request();
        req.extensions_mut().insert(claims.clone());

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);

        // Alice tries to DELETE coop-tech using her coop-food token (should fail - Bug #28)
        let req = test::TestRequest::delete()
            .uri("/coops/coop-tech")
            .to_request();
        req.extensions_mut().insert(claims);

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);

        // Verify coop-tech still exists (wasn't deleted)
        let coop_tech = coop_mgr.get_coop(&"coop-tech".to_string()).await.unwrap();
        assert_eq!(coop_tech.name, "Tech Coop");
    }
}
